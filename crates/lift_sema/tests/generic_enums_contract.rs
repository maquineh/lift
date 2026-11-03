//! Generic enums monomorphized before semantic analysis.
//!
//! PT-BR: enums genéricos monomorfizados antes da semântica.

use lift_sema::{
    analyze, HirDeclaration, HirEnum, HirProgram, ResolvedType, SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}

fn enumeration<'a>(program: &'a HirProgram, name: &str) -> &'a HirEnum {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Enum(enumeration) if enumeration.name == name => Some(enumeration),
            _ => None,
        })
        .unwrap_or_else(|| panic!("enum {name}"))
}

#[test]
fn generic_enums_monomorphize_variants_and_payloads() {
    let program = lower(
        r#"
        enum Maybe<T> { Some(T), None }
        enum Pair2<A, B> { Pair(A, B) }
        const value: Maybe<int> = Maybe.Some(5);
        const empty: Maybe<string> = Maybe.None;
        const pair: Pair2<int, string> = Pair2.Pair(1, "x");
        match value {
            Maybe.Some(inner) => { console.log(inner); }
            Maybe.None => { console.log(0); }
        }
        "#,
    )
    .unwrap();
    let some = enumeration(&program, "Maybe<int>");
    assert_eq!(some.variants[0].name, "Some");
    assert_eq!(some.variants[0].fields, vec![ResolvedType::Int]);
    assert_eq!(some.variants[1].name, "None");
    let strings = enumeration(&program, "Maybe<string>");
    assert_eq!(strings.variants[0].fields, vec![ResolvedType::String]);
    let pair = enumeration(&program, "Pair2<int, string>");
    assert_eq!(pair.variants[0].fields[0], ResolvedType::Int);
    assert_eq!(pair.variants[0].fields[1], ResolvedType::String);
}

#[test]
fn generic_enums_support_recursion_named_fields_and_containers() {
    let program = lower(
        r#"
        enum List<T> { Nil, Cons(T, List<T>) }
        enum Tree<T> { Leaf, Node { value: T, left: Tree<T>, right: Tree<T> } }
        function sum(list: List<int>): int {
            match list {
                List.Nil => { return 0; }
                List.Cons(head, tail) => { return head + sum(tail); }
            }
        }
        const values: List<int> = List.Cons(1, List.Cons(2, List.Nil));
        const trees: Tree<int>[] = [Tree.Leaf];
        console.log(sum(values));
        "#,
    )
    .unwrap();
    let list = enumeration(&program, "List<int>");
    assert_eq!(list.variants[1].fields[0], ResolvedType::Int);
    assert_eq!(
        list.variants[1].fields[1],
        ResolvedType::Struct("List<int>".into())
    );
    assert!(list.variants[1].field_boxed[1]);
    let tree = enumeration(&program, "Tree<int>");
    assert_eq!(tree.variants[1].named_fields[0].ty, ResolvedType::Int);
    assert!(tree.variants[1].named_fields[1].boxed);
}

#[test]
fn generic_enums_reject_invalid_arity_unknown_and_duplicates() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "enum Maybe<T> { Some(T) } const value: Maybe<int, string> = Maybe.Some(1);",
            SemaErrorKind::ArgumentCountMismatch,
            "espera 1",
        ),
        (
            "const value: Maybe<int> = Maybe.Some(1);",
            SemaErrorKind::UnsupportedType,
            "container genérico",
        ),
        (
            "enum Maybe<T, T> { Some(T) } const value: Maybe<int, string> = Maybe.Some(1);",
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
