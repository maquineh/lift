//! Explicit generic type arguments (`f::<T>(...)`).
//!
//! PT-BR: argumentos de tipo explícitos em chamadas genéricas.

use lift_sema::{
    analyze, HirDeclaration, HirProgram, HirStatement, ResolvedType, SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
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
fn explicit_type_arguments_instantiate_generic_functions() {
    let program = lower(
        r#"
        function identity<T>(value: T): T { return value; }
        function first<T>(values: T[]): T { return values[0]; }
        interface Box<T> { value: T }

        const result = identity::<int>(41);
        const text = identity::<string>("hi");
        const first_number = first::<int>([1, 2]);
        const boxes: Box<int>[] = [{ value: 7 }];
        const first_box = first::<Box<int>>(boxes);
        "#,
    )
    .unwrap();
    assert_eq!(variable(&program, "result").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "text").ty, ResolvedType::String);
    assert_eq!(variable(&program, "first_number").ty, ResolvedType::Int);
    assert_eq!(
        variable(&program, "first_box").ty,
        ResolvedType::Struct("Box<int>".into())
    );
}

#[test]
fn explicit_type_arguments_reject_invalid_uses() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "function f<T>(value: T): T { return value; } const x = f::<int, string>(1);",
            SemaErrorKind::ArgumentCountMismatch,
            "argumento(s) de tipo",
        ),
        (
            "function f<T>(value: T): T { return value; } const x = f::<int>(\"x\");",
            SemaErrorKind::TypeMismatch,
            "int",
        ),
        (
            "function f(value: int): int { return value; } const x = f::<int>(1);",
            SemaErrorKind::UnsupportedType,
            "não declara",
        ),
        (
            "function f<T>(value: T): T { return value; } const x = f::<int>;",
            SemaErrorKind::NotCallable,
            "chamada",
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
