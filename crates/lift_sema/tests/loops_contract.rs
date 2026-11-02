//! `for...of`, `while`, and loop-control contracts.
//!
//! PT-BR: Contratos de `for...of`, `while` e controle de laço.

use lift_sema::{
    analyze, HirDeclaration, HirExprKind, HirForOf, HirFunction, HirProgram, HirStatement,
    ResolvedType, SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
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

fn for_loop(function: &HirFunction) -> &HirForOf {
    function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::ForOf(loop_) => Some(loop_),
            _ => None,
        })
        .unwrap()
}

#[test]
fn for_of_iterates_owned_snapshot_and_types_the_binding() {
    let program = lower(
        r#"
        function sum(values: int[]): int {
            let total = 0;
            for (const value of values) {
                total = total + value;
            }
            return total;
        }
        "#,
    )
    .unwrap();
    let sum = function(&program, "sum");
    let loop_ = for_loop(sum);
    assert_eq!(loop_.name, "value");
    assert_eq!(loop_.binding, lift_ast::BindingKind::Const);
    assert_eq!(loop_.element_ty, ResolvedType::Int);
    assert!(matches!(
        loop_.iterable.kind,
        HirExprKind::Clone { .. } | HirExprKind::Move(_)
    ));
    assert!(matches!(
        loop_.body.statements.last(),
        Some(HirStatement::Assignment(_))
    ));
    assert_eq!(loop_.label.0, 0);
}

#[test]
fn break_and_continue_target_the_innermost_loop() {
    let program = lower(
        r#"
        function scan(values: int[]): int {
            let found = 0;
            for (const value of values) {
                while value > 0 {
                    if value == 2 { break; }
                    if value == 3 { continue; }
                    found = found + 1;
                    break;
                }
                if value == 4 { break; }
            }
            return found;
        }
        "#,
    )
    .unwrap();
    let scan = function(&program, "scan");
    let for_of = for_loop(scan);
    let while_ = for_of
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::While(loop_) => Some(loop_),
            _ => None,
        })
        .unwrap();
    assert_ne!(for_of.label, while_.label);
    let while_if = while_
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::If(if_) => Some(if_),
            _ => None,
        })
        .unwrap();
    let HirStatement::Break(target) = &while_if.then_branch.statements[0] else {
        panic!("break")
    };
    assert_eq!(target.target, while_.label);
    let for_if = for_of
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::If(if_) => Some(if_),
            _ => None,
        })
        .unwrap();
    let HirStatement::Break(target) = &for_if.then_branch.statements[0] else {
        panic!("break")
    };
    assert_eq!(target.target, for_of.label);
}

#[test]
fn loop_control_requires_a_loop_and_is_rejected_inside_closures() {
    for source in [
        "break;",
        "continue;",
        r#"
        for (const value of [1]) {
            const f = (): void => { break; };
        }
        "#,
        r#"
        function f(): void {
            continue;
        }
        "#,
    ] {
        let errors = lower(source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == SemaErrorKind::InvalidLoopControl),
            "{errors:?}"
        );
    }

    // `break` dentro de `try`/`catch` dentro do laço é aceito e aponta ao laço.
    let program = lower(
        r#"
        function f(values: int[]): int {
            for (const value of values) {
                try {
                    if value == 2 { break; }
                } catch (error: string) {
                    continue;
                }
            }
            return 1;
        }
        "#,
    )
    .unwrap();
    let f = function(&program, "f");
    let loop_ = for_loop(f);
    let HirStatement::TryCatch(try_) = &loop_.body.statements[0] else {
        panic!("try")
    };
    let HirStatement::If(if_) = &try_.body.statements[0] else {
        panic!("if")
    };
    let HirStatement::Break(target) = &if_.then_branch.statements[0] else {
        panic!("break")
    };
    assert_eq!(target.target, loop_.label);
    assert!(matches!(
        try_.catch_body.statements[0],
        HirStatement::Continue(_)
    ));
}

#[test]
fn for_of_checks_iterable_binding_mutation_scope_and_returns() {
    assert!(lower("for (const value of 1) { console.log(value); }").is_err());
    assert!(lower("for (const value of [1]) { value = 2; }").is_err());
    lower("for (let value of [1]) { value = value + 1; }").unwrap();
    lower("for (const value of [1]) { console.log(value); } const value = 5;").unwrap();
    let errors = lower(
        r#"
        function first(values: int[]): int {
            for (const value of values) { return value; }
        }
        "#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::MissingReturnPath));

    let program = lower(
        r#"
        const values: int?[] = [option.some(1), null];
        for (const value of values) {
            if value != null { console.log(value); }
        }
        "#,
    )
    .unwrap();
    let HirDeclaration::Statement(HirStatement::ForOf(loop_)) = &program.declarations[1] else {
        panic!("for...of")
    };
    assert_eq!(
        loop_.element_ty,
        ResolvedType::Optional(Box::new(ResolvedType::Int))
    );
}

#[test]
fn for_of_cannot_consume_an_external_resource() {
    let errors = lower(
        r#"
        import function create_pool(kind: string, target: string, max_size: int, min_idle: int, timeout: int): Result<DbPool, string> from "lift_db::api::create_pool";
        import function close_pool(pool: DbPool): void from "lift_db::api::close_pool";
        function bad(): Result<void, string> {
            const pending = create_pool("sqlite", ":memory:", 1, 1, 10);
            const pool = pending?;
            for (const value of [1]) {
                close_pool(pool);
            }
            return result.ok();
        }
        "#,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("loop não pode consumir recurso")),
        "{errors:?}"
    );
}
