//! Generic function contracts.
//!
//! PT-BR: Contratos de funções genéricas.

use lift_sema::{
    analyze, HirDeclaration, HirExprKind, HirFunction, HirProgram, HirStatement, ResolvedType,
    SemaError, SemaErrorKind,
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

#[test]
fn generic_functions_infer_parameters_at_each_call() {
    let program = lower(
        r#"
        function identity<T>(value: T): T { return value; }
        function first<T>(values: T[]): T { return values[0]; }
        function same<T>(left: T, right: T): boolean { return left == right; }
        function twice<T>(value: T): T { return identity(value); }
        const value = identity(1);
        const text = identity("hi");
        const first_number = first([1, 2]);
        const equal = same(1, 1);
        const copied: int = twice(9);
        "#,
    )
    .unwrap();
    let identity = function(&program, "identity");
    assert_eq!(identity.type_parameters, vec!["T"]);
    assert_eq!(
        identity.parameters[0].ty,
        ResolvedType::TypeParam("T".into())
    );
    assert_eq!(identity.return_type, ResolvedType::TypeParam("T".into()));
    assert_eq!(variable(&program, "value").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "text").ty, ResolvedType::String);
    assert_eq!(variable(&program, "first_number").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "equal").ty, ResolvedType::Boolean);
    assert_eq!(variable(&program, "copied").ty, ResolvedType::Int);
    let twice = function(&program, "twice");
    let HirStatement::Return(return_statement) = &twice.body.statements[0] else {
        panic!("return")
    };
    let Some(value) = &return_statement.value else {
        panic!("return value")
    };
    let HirExprKind::Call { function, .. } = &value.kind else {
        panic!("call")
    };
    assert_eq!(function, "identity");
    assert_eq!(value.ty, ResolvedType::TypeParam("T".into()));
}

#[test]
fn generic_functions_allow_containers_optionals_and_local_params() {
    lower(
        r#"
        function fallback<T>(value: T?, other: T): T {
            if value != null { return value; }
            return other;
        }
        function copy<T>(value: T): T {
            const local: T = value;
            return local;
        }
        const result = fallback(option.some(3), 0);
        "#,
    )
    .unwrap();
}

#[test]
fn generic_functions_reject_invalid_declarations_and_calls() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "function f<T, T>(value: T): T { return value; }",
            SemaErrorKind::DuplicateParameter,
            "tipo duplicado",
        ),
        (
            "function make<T>(): T { return null; }",
            SemaErrorKind::UnsupportedType,
            "precisa aparecer",
        ),
        (
            "function f<T>(left: T, right: T): T { return left; } const value = f(1, \"x\");",
            SemaErrorKind::TypeMismatch,
            "parâmetro de tipo",
        ),
        (
            "function f<T>(value: T): T { return value; } const value = f;",
            SemaErrorKind::UnsupportedType,
            "não pode ser usada como valor",
        ),
        (
            "function add<T>(left: T, right: T): T { return left + right; }",
            SemaErrorKind::InvalidOperator,
            "não aceita",
        ),
    ];
    for (source, kind, message) in cases {
        let errors = lower(source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == *kind && error.message.contains(message)),
            "expected {kind:?}/{message} in {errors:?}"
        );
    }
}
