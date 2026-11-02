//! Optional value typing and explicit lazy HIR; PT-BR: tipos opcionais e HIR preguiçoso.

use lift_sema::{
    analyze, BindingId, CloneMode, CoalesceMode, HirDeclaration, HirElseBranch, HirExpr,
    HirExprKind, HirFunction, HirProgram, HirStatement, HirStruct, HirVariableDeclaration,
    ResolvedType,
};

fn lower(source: &str) -> Result<HirProgram, Vec<lift_sema::SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}
fn variable<'a>(program: &'a HirProgram, name: &str) -> &'a HirVariableDeclaration {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Statement(HirStatement::Variable(variable))
                if variable.name == name =>
            {
                Some(variable)
            }
            _ => None,
        })
        .unwrap()
}
fn function<'a>(program: &'a HirProgram, name: &str) -> &'a HirFunction {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Function(function) if function.name == name => Some(function),
            _ => None,
        })
        .unwrap()
}
fn structure<'a>(program: &'a HirProgram, name: &str) -> &'a HirStruct {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Struct(structure) if structure.name == name => Some(structure),
            _ => None,
        })
        .unwrap()
}
fn local_id(expression: &HirExpr) -> BindingId {
    match &expression.kind {
        HirExprKind::Local { id, .. } => *id,
        HirExprKind::Grouping(inner)
        | HirExprKind::Move(inner)
        | HirExprKind::Clone {
            expression: inner, ..
        } => local_id(inner),
        other => panic!("esperava acesso a binding local, recebeu {other:?}"),
    }
}

#[test]
fn contextual_null_some_and_coalesce_have_explicit_result_modes() {
    let program = lower(
        r#"
        const missing: int? = null;
        const present = option.some(1);
        const value = missing ?? 7;
        const chain = missing ?? present ?? 9;
        const optional: int? = missing ?? null;
        const inferred = null ?? 3;
        const contextual: int? = null ?? null;
        const absent_check = missing == null;
        const present_check = null != present;
    "#,
    )
    .unwrap();
    let optional_int = ResolvedType::Optional(Box::new(ResolvedType::Int));
    assert_eq!(variable(&program, "missing").ty, optional_int);
    assert!(matches!(
        variable(&program, "missing").initializer.kind,
        HirExprKind::OptionNone
    ));
    assert!(matches!(
        variable(&program, "present").initializer.kind,
        HirExprKind::OptionSome(_)
    ));
    assert!(matches!(
        variable(&program, "value").initializer.kind,
        HirExprKind::Coalesce {
            mode: CoalesceMode::Value,
            ..
        }
    ));
    assert!(matches!(
        variable(&program, "optional").initializer.kind,
        HirExprKind::Coalesce {
            mode: CoalesceMode::Optional,
            ..
        }
    ));
    assert_eq!(variable(&program, "chain").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "inferred").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "contextual").ty, optional_int);
    assert_eq!(variable(&program, "absent_check").ty, ResolvedType::Boolean);
    assert_eq!(
        variable(&program, "present_check").ty,
        ResolvedType::Boolean
    );
}

#[test]
fn arrays_and_required_optional_fields_use_context_without_implicit_lifting() {
    let program = lower(
        r#"
        interface Entry { id: int, tag: string? }
        const entry: Entry = { id: 1, tag: null };
        const elements: int?[] = [null, option.some(2)];
        const optional_array: int[]? = option.some([]);
        const array = optional_array ?? [3, 4];
        const some_entry: Entry? = option.some({ id: 2, tag: option.some("x") });
        const chosen: Entry = some_entry ?? { id: 0, tag: null };
    "#,
    )
    .unwrap();
    assert_eq!(
        variable(&program, "elements").ty,
        ResolvedType::Array(Box::new(ResolvedType::Optional(Box::new(
            ResolvedType::Int
        ))))
    );
    assert_eq!(
        variable(&program, "optional_array").ty,
        ResolvedType::Optional(Box::new(ResolvedType::Array(Box::new(ResolvedType::Int))))
    );
    assert_eq!(
        variable(&program, "chosen").ty,
        ResolvedType::Struct("Entry".to_owned())
    );
}

#[test]
fn invalid_null_nested_optional_lifting_fields_and_fallbacks_are_rejected() {
    for source in [
        "const value = null;",
        "const value = null ?? null;",
        "const value: int = null;",
        "const value: int? = 1;",
        "const value: void? = null;",
        "const value = option.some(option.some(1));",
        "option.some();",
        "option.some(1, 2);",
        "function nothing(): void {} const value = option.some(nothing());",
        "const value: int? = null; const result = value ?? \"wrong\";",
        "const value = 1 ?? 2;",
        "option.is_some(1);",
        "option.unwrap(1);",
        "const value: int? = undefined;",
        "const option = 1; option.some(2);",
        "interface User { name: string? } const user: User = {};",
        "interface Node { next: Node? }",
        "const value: int? = option.some(1); if value != null && value > 0 { console.log(value); }",
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
    lower("interface Node { children: Node?[] }").unwrap();
    lower("const value: int? = option.some(1); if option.is_some(value) { const unwrapped: int = value; }").unwrap();
}

#[test]
fn ffi_accepts_direct_option_and_result_payload_options_but_rejects_deeper_nesting() {
    for source in [
        r#"import function f(value: int?): int? from "sample@1.0.0::f";"#,
        r#"import function f(value: string?): string? from "sample@1.0.0::f";"#,
        r#"import function f(values: int[]?): int[]? from "sample@1.0.0::f";"#,
        r#"import interface Pair { left: int, right: int } from "sample@1.0.0::Pair";
           import function f(value: Pair?): Pair? from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int?, string> from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int, int?> from "sample@1.0.0::f";"#,
        r#"import function f(value: Result<int?, string>): string from "sample@1.0.0::f";"#,
        r#"import function f(value: Result<int?, string>): Result<int?, string> from "sample@1.0.0::f";"#,
    ] {
        lower(source).unwrap();
    }
    for source in [
        r#"import function f(): Result<Result<int, string>, string> from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int, Result<string, string>> from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int, string>? from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int?[], string> from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int, int?[]> from "sample@1.0.0::f";"#,
        r#"import function f(values: int?[]): void from "sample@1.0.0::f";"#,
        r#"interface Local { value: int } import function f(value: Local): int from "sample@1.0.0::f";"#,
        r#"interface Local { value: int } import function f(value: Local?): int from "sample@1.0.0::f";"#,
        r#"import function f(value: Result<void, string>): string from "sample@1.0.0::f";"#,
        r#"import function f(): void? from "sample@1.0.0::f";"#,
    ] {
        assert!(lower(source).is_err(), "must reject FFI: {source}");
    }
}

#[test]
fn expected_context_does_not_cross_a_non_null_coalesce_lhs() {
    assert!(lower("const value: int = null ?? null ?? 1;").is_err());
}

#[test]
fn coalesce_snapshots_and_conditional_fallback_clones_are_preserved() {
    let program = lower(
        r#"
        const source: string? = option.some("present");
        const fallback = "fallback";
        const copy = fallback;
        const value = source ?? fallback;
    "#,
    )
    .unwrap();
    assert!(matches!(
        variable(&program, "copy").initializer.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
    let HirExprKind::Coalesce {
        option, fallback, ..
    } = &variable(&program, "value").initializer.kind
    else {
        panic!("coalesce")
    };
    assert!(matches!(
        option.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
    assert!(matches!(
        fallback.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
}

#[test]
fn optional_fields_are_omittable_and_still_lower_to_option() {
    let program = lower(
        r#"
        interface Profile { name: string, nickname?: string }
        const omitted: Profile = { name: "Ada" };
        const explicit: Profile = { name: "Bo", nickname: option.some("b") };
        const absent: Profile = { name: "Cy", nickname: null };
        const nickname = omitted.nickname;
    "#,
    )
    .unwrap();
    let profile = structure(&program, "Profile");
    let nickname = profile
        .fields
        .iter()
        .find(|field| field.name == "nickname")
        .unwrap();
    assert!(nickname.optional);
    assert_eq!(
        nickname.ty,
        ResolvedType::Optional(Box::new(ResolvedType::String))
    );
    let name = profile
        .fields
        .iter()
        .find(|field| field.name == "name")
        .unwrap();
    assert!(!name.optional);
    let HirExprKind::StructLiteral { fields, .. } = &variable(&program, "omitted").initializer.kind
    else {
        panic!("struct literal")
    };
    assert_eq!(fields.len(), 2);
    assert!(matches!(fields[1].value.kind, HirExprKind::OptionNone));
    assert!(matches!(
        variable(&program, "explicit").initializer.kind,
        HirExprKind::StructLiteral { .. }
    ));
    let HirExprKind::StructLiteral { fields, .. } = &variable(&program, "absent").initializer.kind
    else {
        panic!("struct literal")
    };
    assert!(matches!(
        fields[1].value.ty,
        ResolvedType::Optional(ref payload) if **payload == ResolvedType::String
    ));
    assert_eq!(
        variable(&program, "nickname").ty,
        ResolvedType::Optional(Box::new(ResolvedType::String))
    );
}

#[test]
fn narrowing_derives_read_only_payload_bindings_for_branches_and_guards() {
    let program = lower(
        r#"
        function label(value: string?): string {
            if value != null { return value; }
            return "none";
        }
        function total(value: int?, other: int?): int {
            if value == null { return 0; }
            if option.is_some(other) { return value + other; }
            return value;
        }
        "#,
    )
    .unwrap();
    let label = function(&program, "label");
    let HirStatement::If(conditional) = &label.body.statements[0] else {
        panic!("if")
    };
    let HirStatement::Narrow(narrowing) = &conditional.then_branch.statements[0] else {
        panic!("narrow")
    };
    assert_eq!(narrowing.ty, ResolvedType::String);
    assert_eq!(narrowing.source, label.parameters[0].id);
    let HirStatement::Return(value) = &conditional.then_branch.statements[1] else {
        panic!("return")
    };
    assert_eq!(local_id(value.value.as_ref().unwrap()), narrowing.id);
    assert_eq!(value.value.as_ref().unwrap().ty, ResolvedType::String);

    let total = function(&program, "total");
    let HirStatement::Narrow(guard) = &total.body.statements[1] else {
        panic!("guard narrowing after early return")
    };
    assert_eq!(guard.ty, ResolvedType::Int);
    assert_eq!(guard.source, total.parameters[0].id);

    let HirStatement::If(conditional) = &total.body.statements[2] else {
        panic!("if")
    };
    let HirStatement::Narrow(other) = &conditional.then_branch.statements[0] else {
        panic!("narrow other")
    };
    assert_eq!(other.ty, ResolvedType::Int);
    let HirStatement::Return(value) = &conditional.then_branch.statements[1] else {
        panic!("return")
    };
    let HirExprKind::Binary { left, right, .. } = &value.value.as_ref().unwrap().kind else {
        panic!("binary")
    };
    for (expression, expected) in [(left.as_ref(), guard.id), (right.as_ref(), other.id)] {
        assert_eq!(local_id(expression), expected);
    }
    let HirStatement::Return(value) = &total.body.statements[3] else {
        panic!("return")
    };
    assert_eq!(local_id(value.value.as_ref().unwrap()), guard.id);
}

#[test]
fn narrowing_covers_else_if_while_and_closure_captures() {
    let program = lower(
        r#"
        function choose(first: int?, second: int?): int {
            if first == null { return 0; } else if second == null { return first; }
            return first + second;
        }
        function loops(value: int?): int {
            let total = 0;
            while value != null { total = total + value; }
            return total;
        }
        function captured(value: int?): int {
            if value == null { return 0; }
            const read = (): int => { return value; };
            return read();
        }
        "#,
    )
    .unwrap();
    let choose = function(&program, "choose");
    let HirStatement::If(conditional) = &choose.body.statements[0] else {
        panic!("if")
    };
    let Some(HirElseBranch::Block(alternative)) = &conditional.else_branch else {
        panic!("else-if precisa de bloco com o narrowing falso")
    };
    assert!(matches!(alternative.statements[0], HirStatement::Narrow(_)));
    assert!(matches!(alternative.statements[1], HirStatement::If(_)));
    let narrowed = choose.body.statements[1..]
        .iter()
        .filter(|statement| matches!(statement, HirStatement::Narrow(_)))
        .count();
    assert_eq!(narrowed, 2, "ambos os parâmetros estreitam após o if");

    let loops = function(&program, "loops");
    let HirStatement::While(loop_) = &loops.body.statements[1] else {
        panic!("while")
    };
    assert!(matches!(loop_.body.statements[0], HirStatement::Narrow(_)));

    let captured = function(&program, "captured");
    assert!(matches!(
        captured.body.statements[1],
        HirStatement::Narrow(_)
    ));
    let HirStatement::Variable(read) = &captured.body.statements[2] else {
        panic!("closure")
    };
    let HirExprKind::Closure { captures, body, .. } = &read.initializer.kind else {
        panic!("closure")
    };
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].1.ty, ResolvedType::Int);
    let HirStatement::Return(value) = &body.statements[0] else {
        panic!("return")
    };
    assert_eq!(local_id(value.value.as_ref().unwrap()), captures[0].0);
}

#[test]
fn invalid_optional_fields_and_narrowing_are_rejected() {
    for source in [
        "interface P { name?: string? } const p: P = {};",
        "interface P { name?: void } const p: P = {};",
        "interface P { name?: string } const p: P = { name: \"Ada\" };",
        "interface P { name?: string, id: int } const p: P = { name: null };",
        "interface N { next?: N }",
        "const value: int? = null; if value != null { value = option.some(1); }",
        "let value: int? = option.some(1); if value != null { const unwrapped: int = value; }",
        "interface A { n: int } function f(a: A?): void { if a == null { return; } a.n = 1; }",
        "interface A { n: int } impl A { function bump(): void { this.n = this.n + 1; } } function f(a: A?): void { if a == null { return; } a.bump(); }",
        "const value: int? = option.some(1); if value != null && value > 0 { console.log(value); }",
        "interface I { v: int? } interface O { i: I } const o: O = { i: { v: option.some(1) } }; if o.i.v != null { const direct: int = o.i.v; }",
        "interface A { n: int } function f(a: A?): void { if a == null { return; } a = option.some({ n: 1 }); }",
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}
