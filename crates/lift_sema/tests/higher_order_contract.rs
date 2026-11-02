//! Function types, callback values, captures, and callback FFI contracts.
//!
//! PT-BR: Contratos de tipos de função, valores de callback, captures e FFI.

use lift_sema::{
    analyze, CloneMode, HirDeclaration, HirExprKind, HirFunction, HirProgram, HirStatement,
    ParameterPassMode, ResolvedType, SemaError,
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

fn variable<'a>(program: &'a HirProgram, name: &str) -> &'a lift_sema::HirVariableDeclaration {
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

fn closure_type(parameters: Vec<ResolvedType>, result: ResolvedType) -> ResolvedType {
    ResolvedType::Closure(parameters, Box::new(result))
}

#[test]
fn function_types_store_copy_and_call_callbacks() {
    let program = lower(
        r#"
        function double(value: int): int { return value * 2; }
        function apply(value: int, transform: (int) => int): int { return transform(value); }
        function make_adder(amount: int): (int) => int {
            return (value: int): int => value + amount;
        }
        const stored: (int) => int = double;
        const copied = stored;
        const chained = make_adder(2)(10);
        console.log(apply(3, double));
        console.log(stored(5));
        console.log(copied(6));
        console.log(chained);
        "#,
    )
    .unwrap();
    let apply = function(&program, "apply");
    assert_eq!(
        apply.parameters[1].ty,
        closure_type(vec![ResolvedType::Int], ResolvedType::Int)
    );
    assert_eq!(apply.parameters[1].pass_mode, ParameterPassMode::ByValue);
    assert_eq!(
        function(&program, "make_adder").return_type,
        closure_type(vec![ResolvedType::Int], ResolvedType::Int)
    );
    let stored = variable(&program, "stored");
    assert_eq!(
        stored.ty,
        closure_type(vec![ResolvedType::Int], ResolvedType::Int)
    );
    let HirExprKind::FunctionValue {
        name, parameters, ..
    } = &stored.initializer.kind
    else {
        panic!("function value")
    };
    assert_eq!(name, "double");
    assert_eq!(
        parameters[0],
        (ResolvedType::Int, ParameterPassMode::ByValue)
    );
    let copied = variable(&program, "copied");
    assert!(matches!(
        copied.initializer.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
    // `make_adder(2)(10)`: chamada imediata do retorno de outra chamada.
    let chained = variable(&program, "chained");
    let HirExprKind::ClosureCall { callee, .. } = &chained.initializer.kind else {
        panic!("closure call")
    };
    assert!(matches!(callee.kind, HirExprKind::Call { .. }));
}

#[test]
fn closures_capture_callbacks_by_rc_clone() {
    let program = lower(
        r#"
        function twice_apply(base: (int) => int): (int) => int {
            return (value: int): int => base(base(value));
        }
        "#,
    )
    .unwrap();
    let twice = function(&program, "twice_apply");
    let HirStatement::Return(returned) = &twice.body.statements[0] else {
        panic!("return")
    };
    let HirExprKind::Closure { captures, .. } = &returned.value.as_ref().unwrap().kind else {
        panic!("closure")
    };
    assert_eq!(captures.len(), 1);
    assert_eq!(
        captures[0].1.ty,
        closure_type(vec![ResolvedType::Int], ResolvedType::Int)
    );
    assert!(matches!(
        captures[0].1.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
}

#[test]
fn higher_order_parameters_and_returned_callback_calls_are_typed() {
    let program = lower(
        r#"
        function double(value: int): int { return value * 2; }
        function make(): (int) => int { return (value: int): int => value; }
        function call_higher(callback: ((int) => int) => int, seed: int): int {
            return callback((value: int): int => value + seed);
        }
        const made = make();
        const called = call_higher((inner: (int) => int): int => inner(5), 10);
        console.log(made(3));
        console.log(called);
        "#,
    )
    .unwrap();
    let call_higher = function(&program, "call_higher");
    assert_eq!(
        call_higher.parameters[0].ty,
        closure_type(
            vec![closure_type(vec![ResolvedType::Int], ResolvedType::Int)],
            ResolvedType::Int
        )
    );
    let made = variable(&program, "made");
    assert_eq!(
        made.ty,
        closure_type(vec![ResolvedType::Int], ResolvedType::Int)
    );
    let called = variable(&program, "called");
    let HirExprKind::Call { arguments, .. } = &called.initializer.kind else {
        panic!("call higher")
    };
    assert!(matches!(
        arguments[0].expression.kind,
        HirExprKind::Closure { .. }
    ));
}

#[test]
fn ffi_accepts_callback_parameters_and_returns() {
    let program = lower(
        r#"
        import function apply(value: int, callback: (int) => int): int from "sample@1.0.0::apply";
        import function factory(seed: int): (int) => int from "sample@1.0.0::factory";
        "#,
    )
    .unwrap();
    let imports = program
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            HirDeclaration::Import(import) => Some(import),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(imports[0].parameter_modes[1], ParameterPassMode::ByValue);
    assert_eq!(
        imports[1].return_type,
        closure_type(vec![ResolvedType::Int], ResolvedType::Int)
    );

    for source in [
        r#"import function f(callback: (int?) => int): int from "sample@1.0.0::f";"#,
        r#"import function f(callback: ((int) => int) => int): int from "sample@1.0.0::f";"#,
        r#"import function f(callback: (int) => Result<int, string>): int from "sample@1.0.0::f";"#,
        r#"import function f(callback: (void) => int): int from "sample@1.0.0::f";"#,
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}

#[test]
fn callbacks_are_rejected_in_aggregates_resources_and_capture_mutation() {
    for source in [
        "interface Holder { callback: (int) => int }",
        r#"import interface Holder { callback: (int) => int } from "sample@1.0.0::Holder";"#,
        r#"import function f(callback: (int) => int?): int from "sample@1.0.0::f";"#,
        "function double(value: int): int { return value * 2; } const values = [double];",
        "function double(value: int): int { return value * 2; } const value = option.some(double);",
        "function double(value: int): int { return value * 2; } function f(): Result<(int) => int, string> { return result.ok(double); }",
        r#"import function close_pool(pool: DbPool): void from "lift_db::api::close_pool"; const f = close_pool;"#,
        "function f(callback: (void) => int): int { return 1; }",
        "function apply(value: int, transform: (int) => int): int { return transform(value); } function text(value: string): string { return value; } const result = apply(1, text);",
        "interface Counter { calls: int } const counter: Counter = { calls: 0 }; const f = (): int => { counter.calls = 1; return counter.calls; };",
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}
