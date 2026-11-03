//! Generic external layouts crossing the FFI.
//!
//! PT-BR: layouts externos genéricos cruzando a FFI.

use lift_sema::{
    analyze, HirDeclaration, HirProgram, ResolvedType, RustDependency, SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}

#[test]
fn generic_imported_interfaces_monomorphize_rust_paths_and_fields() {
    let program = lower(
        r#"
        import interface Wrapper<T> { value: T } from "lift_core::collections::Wrapper";
        import function wrap_i64(value: int): Wrapper<int> from "lift_core::collections::wrap_i64";
        import function unwrap_i64(wrapper: Wrapper<int>): int from "lift_core::collections::unwrap_i64";
        import function wrap_string(value: string): Wrapper<string> from "lift_core::collections::wrap_string";
        const wrapped: Wrapper<int> = wrap_i64(41);
        console.log(unwrap_i64(wrapped));
        "#,
    )
    .unwrap();
    let integer = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::ExternStruct(structure) if structure.name == "Wrapper<int>" => {
                Some(structure)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(integer.fields[0].ty, ResolvedType::Int);
    assert_eq!(integer.rust_path, "lift_core::collections::Wrapper<i64>");
    assert_eq!(integer.dependency, RustDependency::LocalCore);
    let text = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::ExternStruct(structure) if structure.name == "Wrapper<string>" => {
                Some(structure)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(text.fields[0].ty, ResolvedType::String);
    assert_eq!(text.rust_path, "lift_core::collections::Wrapper<String>");
    let wrapped = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Import(import) if import.name == "wrap_i64" => Some(import),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        wrapped.return_type,
        ResolvedType::Struct("Wrapper<int>".into())
    );
}

#[test]
fn generic_imported_interfaces_reject_complex_type_arguments() {
    let errors = lower(
        r#"
        import interface Wrapper<T> { value: T } from "lift_core::collections::Wrapper";
        import function wrap_many(values: int[]): Wrapper<int[]> from "lift_core::collections::wrap_many";
        "#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.kind == SemaErrorKind::UnsupportedType
            && error.message.contains("interface externa genérica")
    }));
}
