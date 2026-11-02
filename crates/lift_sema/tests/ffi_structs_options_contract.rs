//! External struct layouts and Option/struct FFI contracts.
//!
//! PT-BR: Contratos de layout de structs externas e de FFI com Option/structs.

use lift_sema::{
    analyze, HirDeclaration, HirExprKind, HirImport, HirProgram, HirStatement, ParameterPassMode,
    ResolvedType, RustDependency, SemaError,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}

fn extern_struct<'a>(program: &'a HirProgram, name: &str) -> &'a lift_sema::HirExternStruct {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::ExternStruct(structure) if structure.name == name => Some(structure),
            _ => None,
        })
        .unwrap()
}

fn import<'a>(program: &'a HirProgram, name: &str) -> &'a HirImport {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Import(import) if import.name == name => Some(import),
            _ => None,
        })
        .unwrap()
}

#[test]
fn imported_interfaces_preserve_layout_dependency_and_optional_fields() {
    let program = lower(
        r#"
        import interface Range { start: int, end?: int } from "sample@1.0.0::geometry::Range";
        const range: Range = { start: 1 };
        const start = range.start;
        const same = range == range;
        const ranges: Range[] = [range];
        const maybe: Range? = option.some(range);
        "#,
    )
    .unwrap();
    let structure = extern_struct(&program, "Range");
    assert_eq!(structure.rust_path, "sample::geometry::Range");
    assert_eq!(
        structure.dependency,
        RustDependency::Registry {
            package: "sample".to_owned(),
            version: "1.0.0".to_owned(),
        }
    );
    assert_eq!(structure.fields.len(), 2);
    assert!(!structure.fields[0].optional);
    assert_eq!(structure.fields[0].ty, ResolvedType::Int);
    assert!(structure.fields[1].optional);
    assert_eq!(
        structure.fields[1].ty,
        ResolvedType::Optional(Box::new(ResolvedType::Int))
    );
    let HirDeclaration::Statement(HirStatement::Variable(range)) = &program.declarations[1] else {
        panic!("variable")
    };
    assert_eq!(range.ty, ResolvedType::Struct("Range".to_owned()));
    let HirExprKind::StructLiteral { fields, .. } = &range.initializer.kind else {
        panic!("struct literal")
    };
    assert_eq!(fields.len(), 2);
    assert!(matches!(fields[1].value.kind, HirExprKind::OptionNone));
    assert_eq!(
        program
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                HirDeclaration::Statement(HirStatement::Variable(variable))
                    if variable.name == "start" =>
                {
                    Some(variable.ty.clone())
                }
                _ => None,
            })
            .unwrap(),
        ResolvedType::Int
    );
}

#[test]
fn external_structs_and_direct_options_are_accepted_by_the_ffi() {
    for source in [
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(value: P): int from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(value: P?): P? from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(values: P[]): P[] from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(): Result<P, string> from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(): Result<P?, string> from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(): Result<P, P?> from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(value: Result<P, string>): string from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(): P[]? from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           impl P { function bump(): void { this.x = this.x + 1; } }"#,
        r#"import function f(value: string?): int? from "sample@1.0.0::f";"#,
    ] {
        lower(source).unwrap();
    }
}

#[test]
fn ffi_rejects_nested_options_local_structs_and_invalid_layout_fields() {
    for source in [
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(): Result<Result<P, string>, string> from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(): Result<P, string>? from "sample@1.0.0::f";"#,
        r#"import interface P { x: int } from "sample@1.0.0::P";
           import function f(values: P?[]): void from "sample@1.0.0::f";"#,
        r#"interface Local { value: int }
           import function f(value: Local): int from "sample@1.0.0::f";"#,
        r#"interface Local { value: int }
           import function f(value: Local?): int from "sample@1.0.0::f";"#,
        r#"import interface R { value: Result<int, string> } from "sample@1.0.0::R";"#,
        r#"interface Local { n: int }
           import interface R { value: Local } from "sample@1.0.0::R";"#,
        r#"import interface R { value: DbPool } from "sample@1.0.0::R";"#,
        r#"import interface R { value?: int? } from "sample@1.0.0::R";"#,
        r#"import interface R { value: int[][] } from "sample@1.0.0::R";"#,
        r#"import interface R { fn: int } from "sample@1.0.0::R";"#,
        r#"import interface R { x: int } from "no-path";"#,
        r#"import interface R { x: int } from "sample::R";"#,
        r#"import interface R { x: int } from "sample@1.0.0::R"; interface R { x: int }"#,
        r#"import interface R { x: int } from "sample@1.0.0::R"; function R(): void {}"#,
        r#"import interface R { x: int } from "sample@1.0.0::R";
           import function f(): int from "sample@2.0.0::f";"#,
    ] {
        assert!(lower(source).is_err(), "must reject: {source}");
    }
}

#[test]
fn ffi_parameter_modes_for_structs_options_and_results_stay_borrowed() {
    let program = lower(
        r#"
        import interface P { x: int } from "sample@1.0.0::P";
        import function f(value: P?, integer: int?, items: P[], outcome: Result<int?, string>): P? from "sample@1.0.0::f";
        "#,
    )
    .unwrap();
    let import = import(&program, "f");
    assert_eq!(
        import.parameter_modes,
        vec![
            ParameterPassMode::BorrowImm,
            ParameterPassMode::BorrowImm,
            ParameterPassMode::BorrowImm,
            ParameterPassMode::BorrowImm
        ]
    );
    assert_eq!(
        import.parameters,
        vec![
            ResolvedType::Optional(Box::new(ResolvedType::Struct("P".to_owned()))),
            ResolvedType::Optional(Box::new(ResolvedType::Int)),
            ResolvedType::Array(Box::new(ResolvedType::Struct("P".to_owned()))),
            ResolvedType::Result(
                Box::new(ResolvedType::Optional(Box::new(ResolvedType::Int))),
                Box::new(ResolvedType::String)
            ),
        ]
    );
    assert_eq!(
        import.return_type,
        ResolvedType::Optional(Box::new(ResolvedType::Struct("P".to_owned())))
    );
}
