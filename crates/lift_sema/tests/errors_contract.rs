//! Result construction, `?` propagation, `throw`, and try/catch contracts.
//!
//! PT-BR: Contratos de construção de Result, propagação com `?`, `throw` e
//! try/catch.

use lift_sema::{
    analyze, HirDeclaration, HirExprKind, HirFunction, HirProgram, HirStatement, HirTryCatch,
    PropagateTarget, ResolvedType, SemaError,
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

fn try_catch<'a>(function: &'a HirFunction) -> &'a HirTryCatch {
    function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::TryCatch(statement) => Some(statement),
            _ => None,
        })
        .unwrap()
}

fn variable_initializer<'a>(function: &'a HirFunction, name: &str) -> &'a lift_sema::HirExpr {
    function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::Variable(variable) if variable.name == name => {
                Some(&variable.initializer)
            }
            _ => None,
        })
        .unwrap()
}

fn variable_initializer_in_try<'a>(
    function: &'a HirFunction,
    name: &str,
) -> &'a lift_sema::HirExpr {
    try_catch(function)
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            HirStatement::Variable(variable) if variable.name == name => {
                Some(&variable.initializer)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn result_constructors_require_context_and_support_void_success() {
    let program = lower(
        r#"
        function ok_int(): Result<int, string> { return result.ok(1); }
        function err_text(): Result<int, string> { return result.err("bad"); }
        function ok_void(): Result<void, string> { return result.ok(); }
        const annotated: Result<int, string> = result.ok(2);
        "#,
    )
    .unwrap();
    let HirStatement::Return(returned) = &function(&program, "ok_int").body.statements[0] else {
        panic!("return")
    };
    assert!(matches!(
        returned.value.as_ref().unwrap().kind,
        HirExprKind::ResultOk(Some(_))
    ));
    let HirStatement::Return(returned) = &function(&program, "err_text").body.statements[0] else {
        panic!("return")
    };
    assert!(matches!(
        returned.value.as_ref().unwrap().kind,
        HirExprKind::ResultErr(_)
    ));
    let HirStatement::Return(returned) = &function(&program, "ok_void").body.statements[0] else {
        panic!("return")
    };
    assert!(matches!(
        returned.value.as_ref().unwrap().kind,
        HirExprKind::ResultOk(None)
    ));
    let HirDeclaration::Statement(HirStatement::Variable(annotated)) = &program.declarations[3]
    else {
        panic!("variable")
    };
    assert!(matches!(
        annotated.initializer.kind,
        HirExprKind::ResultOk(Some(_))
    ));

    for source in [
        "const value = result.ok(1);",
        "const value: int = result.ok(1);",
        "function f(): Result<int, string> { return result.ok(); }",
        "function f(): Result<int, string> { return result.err(); }",
        "function f(): Result<int, string> { return result.ok(1, 2); }",
        "function f(): Result<int, string> { return result.err(1, 2); }",
        "function f(): Result<int, string> { return result.ok(null); }",
        "const result = 1; const value: Result<int, string> = result.ok(1);",
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}

#[test]
fn propagation_targets_try_then_function_and_checks_error_types() {
    let program = lower(
        r#"
        import function source(): Result<int, string> from "sample@1.0.0::source";
        import function other(): Result<int, int> from "sample@1.0.0::other";
        function direct(): Result<int, string> {
            const value = source()?;
            return result.ok(value);
        }
        function inside(): int {
            try {
                const value = source()?;
                return value;
            } catch (error: string) {
                return -1;
            }
        }
        function skip(): Result<int, string> {
            try {
                const value = source()?;
                return result.ok(value);
            } catch (error: int) {
                return result.ok(-1);
            }
        }

        "#,
    )
    .unwrap();
    let HirExprKind::TryPropagate { target, .. } =
        variable_initializer(function(&program, "direct"), "value").kind
    else {
        panic!("try propagate")
    };
    assert_eq!(target, PropagateTarget::Function);
    let inside = function(&program, "inside");
    let statement = try_catch(inside);
    let HirExprKind::TryPropagate { target, .. } =
        variable_initializer_in_try(inside, "value").kind
    else {
        panic!("try propagate")
    };
    assert_eq!(target, PropagateTarget::Try(statement.label));
    let skip = function(&program, "skip");
    let HirExprKind::TryPropagate { target, .. } = variable_initializer_in_try(skip, "value").kind
    else {
        panic!("try propagate")
    };
    assert_eq!(target, PropagateTarget::Function);

    for source in [
        r#"import function source(): Result<int, string> from "sample@1.0.0::source";
           function f(): int { const value = source()?; return value; }"#,
        r#"import function source(): Result<int, string> from "sample@1.0.0::source";
           function f(): Result<int, int> { const value = source()?; return result.ok(1); }"#,
        r#"import function source(): Result<int, string> from "sample@1.0.0::source";
           const value = source()?;"#,
        "const optional: int? = option.some(1); const value = optional?;",
        "function f(): Result<int, string> { const value = 1?; return result.ok(1); }",
        r#"import function nested(): Result<Result<int, string>, string> from "sample@1.0.0::nested";
           function f(): Result<int, string> { const value = nested()?; return result.ok(1); }"#,
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}

#[test]
fn throw_targets_catch_or_function_and_rejects_type_mismatch() {
    let program = lower(
        r#"
        function direct(): Result<int, string> { throw "bad"; }
        function caught(): int {
            try { throw "bad"; } catch (error: string) { return 1; }
        }
        function skipped(): Result<int, string> {
            try {
                throw "bad";
            } catch (error: int) {
                return result.ok(-1);
            }
        }
        "#,
    )
    .unwrap();
    let HirStatement::Throw(direct) = &function(&program, "direct").body.statements[0] else {
        panic!("throw")
    };
    assert_eq!(direct.target, PropagateTarget::Function);
    let caught = function(&program, "caught");
    let statement = try_catch(caught);
    let HirStatement::Throw(throw) = &statement.body.statements[0] else {
        panic!("throw")
    };
    assert_eq!(throw.target, PropagateTarget::Try(statement.label));
    let skipped = function(&program, "skipped");
    let HirStatement::Throw(throw) = &try_catch(skipped).body.statements[0] else {
        panic!("throw")
    };
    assert_eq!(throw.target, PropagateTarget::Function);

    for source in [
        r#"throw "x";"#,
        r#"function f(): int { throw "x"; }"#,
        r#"function f(): Result<int, string> { throw 1; }"#,
        r#"function f(): int {
               try { throw 1; } catch (error: string) { return 0; }
           }"#,
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}

#[test]
fn try_catch_tracks_catch_mutability_and_return_guarantees() {
    let program = lower(
        r#"
        interface Failure { code: int }
        function returns(): int {
            try { return 1; } catch (error: string) { }
        }
        function mutated(): int {
            try { } catch (error: Failure) { error.code = 1; }
            return 0;
        }
        function throwing(): Result<int, string> {
            try { throw "first"; } catch (error: string) { throw "second"; }
        }
        function nested(): int {
            try {
                try { throw "inner"; } catch (inner_error: string) { return 1; }
            } catch (outer_error: string) { return 2; }
        }
        "#,
    )
    .unwrap();
    assert!(try_catch(function(&program, "returns")).returns);
    let mutated = try_catch(function(&program, "mutated"));
    assert!(mutated.catch_mut);
    assert_eq!(mutated.catch_ty, ResolvedType::Struct("Failure".to_owned()));
    assert!(try_catch(function(&program, "throwing")).returns);
    let nested = try_catch(function(&program, "nested"));
    let HirStatement::TryCatch(inner) = &nested.body.statements[0] else {
        panic!("inner try")
    };
    assert_ne!(inner.label, nested.label);
    let HirStatement::Throw(throw) = &inner.body.statements[0] else {
        panic!("throw")
    };
    assert_eq!(throw.target, PropagateTarget::Try(inner.label));

    for source in [
        r#"function fallthrough(): int {
               try { console.log(1); } catch (error: string) { return 1; }
           }"#,
        r#"const error = 1; try { } catch (error: string) { }"#,
        r#"function f(): int {
               try { } catch (error: void) { }
               return 0;
           }"#,
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}

#[test]
fn question_clones_plain_results_and_moves_resource_results() {
    let program = lower(
        r#"
        import function source(): Result<int, string> from "sample@1.0.0::source";
        import function create_pool(kind: string, target: string, max_size: int, min_idle: int, timeout: int): Result<DbPool, string> from "lift_db::api::create_pool";
        import function close_pool(pool: DbPool): void from "lift_db::api::close_pool";

        function plain(): Result<int, string> {
            const pending = source();
            const value = pending?;
            return result.ok(value);
        }
        function resource(): Result<int, string> {
            const pending = create_pool("sqlite", ":memory:", 1, 1, 10);
            const pool = pending?;
            close_pool(pool);
            return result.ok(1);
        }
        "#,
    )
    .unwrap();
    let plain = variable_initializer(function(&program, "plain"), "value");
    let HirExprKind::TryPropagate { expression, .. } = &plain.kind else {
        panic!("try propagate")
    };
    assert!(matches!(
        expression.kind,
        HirExprKind::Clone {
            mode: lift_sema::CloneMode::Value,
            ..
        }
    ));
    let resource = variable_initializer(function(&program, "resource"), "pool");
    let HirExprKind::TryPropagate { expression, .. } = &resource.kind else {
        panic!("try propagate")
    };
    assert!(matches!(expression.kind, HirExprKind::Move(_)));

    assert!(lower(
        r#"
        import function create_pool(kind: string, target: string, max_size: int, min_idle: int, timeout: int): Result<DbPool, string> from "lift_db::api::create_pool";
        import function close_pool(pool: DbPool): void from "lift_db::api::close_pool";
        function reuse(): Result<int, string> {
            const pending = create_pool("sqlite", ":memory:", 1, 1, 10);
            const pool = pending?;
            const again = pending?;
            close_pool(pool);
            close_pool(again);
            return result.ok(1);
        }
        "#,
    )
    .is_err());
}

#[test]
fn optional_payload_and_void_propagation_are_supported() {
    let program = lower(
        r#"
        import function optional_source(): Result<int?, string> from "sample@1.0.0::optional_source";
        import function nothing(): Result<void, string> from "sample@1.0.0::nothing";
        function use_optional(): Result<int, string> {
            const value = optional_source()?;
            return result.ok(value ?? 0);
        }
        function use_void(): Result<void, string> {
            nothing()?;
            return result.ok();
        }
        "#,
    )
    .unwrap();
    let value = variable_initializer(function(&program, "use_optional"), "value");
    assert_eq!(
        value.ty,
        ResolvedType::Optional(Box::new(ResolvedType::Int))
    );
    let HirStatement::Expression(statement) = &function(&program, "use_void").body.statements[0]
    else {
        panic!("expression")
    };
    assert_eq!(statement.expression.ty, ResolvedType::Void);
    assert!(matches!(
        statement.expression.kind,
        HirExprKind::TryPropagate { .. }
    ));
}
