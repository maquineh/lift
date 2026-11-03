//! Generic impl blocks specialized per concrete container application.
//!
//! PT-BR: impls genéricos especializados por aplicação concreta de container.

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
fn generic_impls_specialize_methods_for_each_instantiation() {
    let program = lower(
        r#"
        interface Box<T> { value: T }

        impl Box<T> {
            get(): T { return this.value; }
            set(value: T): void { this.value = value; }
        }

        let integer: Box<int> = { value: 1 };
        let text: Box<string> = { value: "x" };
        integer.set(2);
        console.log(integer.get());
        console.log(text.get());
        "#,
    )
    .unwrap();
    let get_int = function(&program, "$method:Box<int>:get");
    assert_eq!(
        get_int.parameters[0].ty,
        ResolvedType::Struct("Box<int>".into())
    );
    assert_eq!(get_int.return_type, ResolvedType::Int);
    let get_text = function(&program, "$method:Box<string>:get");
    assert_eq!(
        get_text.parameters[0].ty,
        ResolvedType::Struct("Box<string>".into())
    );
    assert_eq!(get_text.return_type, ResolvedType::String);
    let set_int = function(&program, "$method:Box<int>:set");
    assert_eq!(set_int.parameters[1].ty, ResolvedType::Int);
}

#[test]
fn generic_impls_require_an_application_and_known_interface() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[(
        "impl Box<T> { get(): T { return this.value; } } const boxed: Box<int> = { value: 1 };",
        SemaErrorKind::UnknownType,
        "impl exige",
    )];
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
