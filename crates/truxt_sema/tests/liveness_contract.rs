//! Conservative CFG/liveness decisions after successful semantic validation.
//!
//! PT-BR: Decisões conservadoras de CFG/liveness após validação semântica completa.

use truxt_sema::{
    analyze, ArgPassMode, CloneMode, HirDeclaration, HirElseBranch, HirExpr, HirExprKind, HirIf,
    HirProgram, HirStatement,
};

fn lower(source: &str) -> HirProgram {
    let tokens = truxt_lexer::tokenize(source).unwrap();
    analyze(truxt_parser::parse(&tokens).unwrap()).unwrap()
}

fn expressions(program: &HirProgram) -> Vec<&HirExpr> {
    fn expression<'a>(value: &'a HirExpr, out: &mut Vec<&'a HirExpr>) {
        out.push(value);
        match &value.kind {
            HirExprKind::StructLiteral { fields, .. } => {
                for field in fields {
                    expression(&field.value, out);
                }
            }
            HirExprKind::ArrayLiteral(elements) => {
                for element in elements {
                    expression(element, out);
                }
            }
            HirExprKind::Unary { operand, .. }
            | HirExprKind::Grouping(operand)
            | HirExprKind::Move(operand)
            | HirExprKind::Clone {
                expression: operand,
                ..
            } => expression(operand, out),
            HirExprKind::Binary { left, right, .. } => {
                expression(left, out);
                expression(right, out);
            }
            HirExprKind::FieldAccess { object, .. } => expression(object, out),
            HirExprKind::Index { object, index } => {
                expression(object, out);
                expression(index, out);
            }
            HirExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    expression(&argument.expression, out);
                }
            }
            _ => {}
        }
    }
    fn conditional<'a>(value: &'a HirIf, out: &mut Vec<&'a HirExpr>) {
        expression(&value.condition, out);
        for value in &value.then_branch.statements {
            statement(value, out);
        }
        match &value.else_branch {
            Some(HirElseBranch::Block(block)) => {
                for value in &block.statements {
                    statement(value, out);
                }
            }
            Some(HirElseBranch::ElseIf(value)) => conditional(value, out),
            None => {}
        }
    }
    fn statement<'a>(value: &'a HirStatement, out: &mut Vec<&'a HirExpr>) {
        match value {
            HirStatement::Variable(value) => expression(&value.initializer, out),
            HirStatement::Assignment(value) => {
                expression(&value.target, out);
                expression(&value.value, out);
            }
            HirStatement::Expression(value) => expression(&value.expression, out),
            HirStatement::Return(value) => {
                if let Some(value) = &value.value {
                    expression(value, out);
                }
            }
            HirStatement::If(value) => conditional(value, out),
            HirStatement::While(value) => {
                expression(&value.condition, out);
                for value in &value.body.statements {
                    statement(value, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    for declaration in &program.declarations {
        match declaration {
            HirDeclaration::Function(value) => {
                for value in &value.body.statements {
                    statement(value, &mut out);
                }
            }
            HirDeclaration::Statement(value) => statement(value, &mut out),
            _ => {}
        }
    }
    out
}

fn decisions(program: &HirProgram, local: &str) -> Vec<&'static str> {
    expressions(program)
        .into_iter()
        .filter_map(|value| {
            let (operand, decision) = match &value.kind {
                HirExprKind::Move(operand) => (operand, "move"),
                HirExprKind::Clone { expression, mode } => (
                    expression,
                    match mode {
                        CloneMode::Value => "clone",
                        CloneMode::BorrowedString => "borrowed-string",
                        CloneMode::BorrowedAggregate => "borrowed-aggregate",
                    },
                ),
                _ => return None,
            };
            match &operand.kind {
                HirExprKind::Local { name, .. } if name == local => Some(decision),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn last_owned_uses_move_in_script_and_function_bodies() {
    let program = lower(
        r#"
        function f(): string { const owned = "function"; const output = owned; return output; }
        const source = "script"; const copy = source; console.log(copy);
    "#,
    );
    assert_eq!(decisions(&program, "source"), ["move"]);
    assert_eq!(decisions(&program, "owned"), ["move"]);
    assert_eq!(decisions(&program, "output"), ["move"]);
}

#[test]
fn uses_after_a_copy_and_at_if_joins_keep_the_clone() {
    for source in [
        r#"const source = "a"; const copy = source; console.log(source); console.log(copy);"#,
        r#"function f(flag: boolean): void { const source = "a"; if flag { const copy = source; console.log(copy); } console.log(source); }"#,
        r#"function f(flag: boolean): void { let source = "a"; const copy = source; if flag { source = "new"; } console.log(source); }"#,
    ] {
        assert_eq!(decisions(&lower(source), "source"), ["clone"], "{source}");
    }
}

#[test]
fn exclusive_branches_and_return_terminators_have_real_cfg_edges() {
    let branches = lower(
        r#"function f(flag: boolean): void { const source = "a"; if flag { const yes = source; console.log(yes); } else { const no = source; console.log(no); } }"#,
    );
    assert_eq!(decisions(&branches, "source"), ["move", "move"]);
    let else_if = lower(
        r#"function f(first: boolean, second: boolean): void { const source = "a"; if first { const one = source; console.log(one); } else if second { const two = source; console.log(two); } else { const three = source; console.log(three); } }"#,
    );
    assert_eq!(decisions(&else_if, "source"), ["move", "move", "move"]);
    let returns = lower(
        r#"function f(source: string, flag: boolean): string { if flag { return source; } return source; }"#,
    );
    // Owned-return parameters already move as plain Local in the validated HIR.
    // Parâmetros OwnedMove já retornam como Local, sem Clone para otimizar.
    assert!(decisions(&returns, "source").is_empty());
    let owned_parameter = lower(
        r#"function f(source: string, flag: boolean): string { if flag { return source; } const copy = source; return copy; }"#,
    );
    assert_eq!(decisions(&owned_parameter, "source"), ["move"]);
    let early = lower(
        r#"function f(flag: boolean): string { const source = "a"; if flag { return source; } console.log(source); return source; }"#,
    );
    assert_eq!(decisions(&early, "source"), ["move", "move"]);
    let terminal = lower(
        r#"function f(): string { const source = "a"; const copy = source; return copy; console.log(source); }"#,
    );
    assert_eq!(decisions(&terminal, "source"), ["move"]);
}

#[test]
fn loops_reach_a_fixed_point_and_include_the_zero_iteration_edge() {
    for source in [
        r#"function f(flag: boolean): void { const source = "a"; while flag { const copy = source; console.log(copy); } }"#,
        r#"function f(flag: boolean): void { const source = "a"; const copy = source; while flag { console.log(source); } }"#,
        r#"function f(flag: boolean): void { const source = "a"; while flag { const copy = source; console.log(copy); } console.log(source); }"#,
    ] {
        assert_eq!(decisions(&lower(source), "source"), ["clone"], "{source}");
    }
    let after = lower(
        r#"function f(flag: boolean): void { const source = "a"; while flag { console.log(source); } const copy = source; console.log(copy); }"#,
    );
    assert_eq!(decisions(&after, "source"), ["move"]);
}

#[test]
fn reassignment_kills_the_previous_value_but_self_assignment_is_repeated_use() {
    let reassigned = lower(
        r#"let source = "old"; const copy = source; source = "new"; console.log(copy); console.log(source);"#,
    );
    assert_eq!(decisions(&reassigned, "source"), ["move"]);
    let self_assignment = lower(r#"let source = "old"; source = source;"#);
    assert_eq!(decisions(&self_assignment, "source"), ["clone"]);
}

#[test]
fn repeated_occurrences_in_one_statement_keep_every_clone() {
    let array =
        lower(r#"const source = "a"; const copies = [source, source]; console.log(copies[0]);"#);
    assert_eq!(decisions(&array, "source"), ["clone", "clone"]);
    let conditional = lower(
        r#"let source = "a"; if [source] == ["a"] { source = "b"; } else { source = "c"; } console.log(source);"#,
    );
    assert_eq!(decisions(&conditional, "source"), ["clone"]);
}

#[test]
fn borrowed_parameters_and_projection_clones_remain_unchanged() {
    let program = lower(
        r#"
        interface Box { value: string }
        function text(text_arg: string): void { const copy = text_arg; console.log(copy); }
        function shared(shared_arg: Box): void { const copy = shared_arg; console.log(copy.value); }
        function mutated(mutated_arg: Box): void { mutated_arg.value = "new"; const copy = mutated_arg; console.log(copy.value); }
        const box: Box = { value: "field" }; const field = box.value;
        const values: string[] = ["index"]; const element = values[0];
    "#,
    );
    assert_eq!(decisions(&program, "text_arg"), ["borrowed-string"]);
    assert_eq!(decisions(&program, "shared_arg"), ["borrowed-aggregate"]);
    assert_eq!(decisions(&program, "mutated_arg"), ["borrowed-aggregate"]);
    assert_eq!(expressions(&program).iter().filter(|value| matches!(&value.kind, HirExprKind::Clone { expression, .. } if matches!(expression.kind, HirExprKind::FieldAccess { .. } | HirExprKind::Index { .. }))).count(), 2);
}

#[test]
fn owned_call_arguments_remain_by_clone_even_at_last_use() {
    let program = lower(
        r#"function take(value: string): string { return value; } const source = "a"; const output = take(source);"#,
    );
    let arguments = expressions(&program)
        .into_iter()
        .find_map(|expression| match &expression.kind {
            HirExprKind::Call {
                function,
                arguments,
            } if function == "take" => Some(arguments),
            _ => None,
        })
        .unwrap();
    assert_eq!(arguments[0].pass_mode, ArgPassMode::ByClone);
    assert_eq!(arguments[0].clone_mode, Some(CloneMode::Value));
}

#[test]
fn grouped_operands_are_not_treated_as_direct_locals() {
    let program = lower(r#"const source = "a"; const copy = (source);"#);
    assert!(expressions(&program).iter().any(|value| matches!(&value.kind, HirExprKind::Clone { expression, mode: CloneMode::Value } if matches!(expression.kind, HirExprKind::Grouping(_)))));
    assert!(decisions(&program, "source").is_empty());
}
