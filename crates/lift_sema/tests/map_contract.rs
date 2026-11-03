//! Native `Map<K, V>` built on BTreeMap.
//!
//! PT-BR: `Map<K, V>` nativo sobre BTreeMap.

use lift_sema::{
    analyze, HirDeclaration, HirExprKind, HirProgram, HirStatement, ResolvedType, SemaError,
    SemaErrorKind,
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
fn native_map_constructs_and_uses_builtin_methods() {
    let program = lower(
        r#"
        const empty: Map<string, int> = map.new();
        const one = empty.insert("a", 1);
        const two = one.insert("b", 2);
        const value = two.get("a") ?? 0;
        const missing = two.get("z") ?? 0;
        const has = two.contains("b");
        const length = two.len();
        console.log(value);
        console.log(missing);
        console.log(has);
        console.log(length);
        "#,
    )
    .unwrap();
    assert_eq!(
        variable(&program, "empty").ty,
        ResolvedType::Struct("Map<string, int>".into())
    );
    assert_eq!(variable(&program, "value").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "has").ty, ResolvedType::Boolean);
    assert_eq!(variable(&program, "length").ty, ResolvedType::Int);
    let empty = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::ExternStruct(structure) if structure.name == "Map<string, int>" => {
                Some(structure)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        empty.rust_path,
        "lift_core::collections::BTreeMap<String, i64>"
    );
    let two = variable(&program, "two");
    assert!(matches!(
        two.initializer.kind,
        HirExprKind::MapMethod { .. }
    ));
}

#[test]
fn native_map_supports_other_key_and_value_types() {
    lower(
        r#"
        const numbers: Map<int, string> = map.new();
        const booleans: Map<boolean, int> = map.new();
        const one = numbers.insert(1, "one");
        const yes = booleans.insert(true, 1);
        console.log(one.len());
        console.log(yes.len());
        "#,
    )
    .unwrap();
}

#[test]
fn native_map_rejects_invalid_uses() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "const holder = map.new();",
            SemaErrorKind::UnknownVariable,
            "map",
        ),
        (
            "const numbers: Map<number, int> = map.new();",
            SemaErrorKind::UnsupportedType,
            "number",
        ),
        (
            "const values: Map<string, int> = map.new(); const value = values.insert(1, 2);",
            SemaErrorKind::TypeMismatch,
            "string",
        ),
        (
            "const values: Map<string, int> = map.new(); const value = values.get();",
            SemaErrorKind::ArgumentCountMismatch,
            "get",
        ),
        (
            "const values: Map<string, int> = map.new(); const value = values.remove(\"a\");",
            SemaErrorKind::UnknownField,
            "remove",
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
