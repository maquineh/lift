//! Generic interface containers monomorphized before semantic analysis.
//!
//! PT-BR: containers de interface genérica monomorfizados antes da semântica.

use lift_sema::{
    analyze, HirDeclaration, HirFunction, HirProgram, ResolvedType, SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}

fn structure<'a>(program: &'a HirProgram, name: &str) -> &'a lift_sema::HirStruct {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Struct(structure) if structure.name == name => Some(structure),
            _ => None,
        })
        .unwrap_or_else(|| panic!("struct {name}"))
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

#[test]
fn generic_interfaces_monomorphize_nested_and_container_fields() {
    let program = lower(
        r#"
        interface Box<T> { value: T }
        interface Pair<A, B> { left: A, right: B }
        interface Map<K, V> { keys: K[], values: V[] }

        const boxed: Box<int> = { value: 41 };
        const nested: Box<Box<string>> = { value: { value: "hi" } };
        const pair: Pair<int, string> = { left: 1, right: "x" };
        const map: Map<string, int> = { keys: ["a"], values: [10] };

        function read(box: Box<int>): int { return box.value; }
        console.log(read(boxed));
        "#,
    )
    .unwrap();
    let boxed = structure(&program, "Box<int>");
    assert_eq!(boxed.fields[0].ty, ResolvedType::Int);
    let nested = structure(&program, "Box<Box<string>>");
    assert_eq!(
        nested.fields[0].ty,
        ResolvedType::Struct("Box<string>".into())
    );
    assert_eq!(
        structure(&program, "Box<string>").fields[0].ty,
        ResolvedType::String
    );
    let pair = structure(&program, "Pair<int, string>");
    assert_eq!(pair.fields[0].ty, ResolvedType::Int);
    assert_eq!(pair.fields[1].ty, ResolvedType::String);
    let map = structure(&program, "Map<string, int>");
    assert_eq!(
        map.fields[0].ty,
        ResolvedType::Array(Box::new(ResolvedType::String))
    );
    assert_eq!(
        map.fields[1].ty,
        ResolvedType::Array(Box::new(ResolvedType::Int))
    );
    assert_eq!(
        function(&program, "read").parameters[0].ty,
        ResolvedType::Struct("Box<int>".into())
    );
}

#[test]
fn generic_interfaces_accept_optional_and_array_arguments() {
    lower(
        r#"
        interface Box<T> { value: T }
        const optional: Box<int?> = { value: option.some(7) };
        const boxes: Box<int>[] = [{ value: 1 }, { value: 2 }];
        console.log(optional.value == null);
        console.log(boxes[1].value);
        "#,
    )
    .unwrap();
}

#[test]
fn generic_interfaces_reject_invalid_arity_unknown_and_mismatches() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "interface Box<T> { value: T } const value: Box<int, string> = { value: 1 };",
            SemaErrorKind::ArgumentCountMismatch,
            "espera 1",
        ),
        (
            "const value: Box<int> = { value: 1 };",
            SemaErrorKind::UnsupportedType,
            "container genérico",
        ),
        (
            "interface Box<T> { value: T } const value: Box = { value: 1 };",
            SemaErrorKind::UnknownType,
            "Box",
        ),
        (
            "interface Box<T> { value: T } const value: Box<int> = { value: \"x\" };",
            SemaErrorKind::TypeMismatch,
            "int",
        ),
        (
            "interface Box<T, T> { value: T } const value: Box<int, string> = { value: 1 };",
            SemaErrorKind::DuplicateParameter,
            "duplicado",
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
