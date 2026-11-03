//! Generic functions over containers, specialized from explicit type arguments.
//!
//! PT-BR: funções genéricas sobre containers, especializadas por argumentos explícitos.

use lift_sema::{
    analyze, HirDeclaration, HirFunction, HirProgram, ResolvedType, SemaError, SemaErrorKind,
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
        .unwrap_or_else(|| panic!("function {name}"))
}

#[test]
fn generic_container_functions_are_specialized_by_explicit_arguments() {
    let program = lower(
        r#"
        interface Box<T> { value: T }
        function unwrap<T>(box: Box<T>): T { return box.value; }
        function unwrapTwice<T>(box: Box<Box<T>>): T {
            return unwrap::<Box<T>>(box).value;
        }
        const boxed: Box<int> = { value: 41 };
        const nested: Box<Box<string>> = { value: { value: "hi" } };
        console.log(unwrap::<int>(boxed));
        console.log(unwrapTwice::<string>(nested));
        "#,
    )
    .unwrap();
    let unwrap = function(&program, "unwrap<int>");
    assert!(unwrap.type_parameters.is_empty());
    assert_eq!(
        unwrap.parameters[0].ty,
        ResolvedType::Struct("Box<int>".into())
    );
    assert_eq!(unwrap.return_type, ResolvedType::Int);
    let twice = function(&program, "unwrapTwice<string>");
    assert_eq!(
        twice.parameters[0].ty,
        ResolvedType::Struct("Box<Box<string>>".into())
    );
    assert_eq!(twice.return_type, ResolvedType::String);
}

#[test]
fn generic_container_functions_require_explicit_arguments() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "interface Box<T> { value: T } function unwrap<T>(box: Box<T>): T { return box.value; } const boxed: Box<int> = { value: 1 }; const value = unwrap(boxed);",
            SemaErrorKind::UnknownFunction,
            "unwrap",
        ),
        (
            "interface Box<T> { value: T } function unwrap<T>(box: Box<T>): T { return box.value; } const boxed: Box<int> = { value: 1 }; const value = unwrap::<int, string>(boxed);",
            SemaErrorKind::ArgumentCountMismatch,
            "argumento(s) de tipo",
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
