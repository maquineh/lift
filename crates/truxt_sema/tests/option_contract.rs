//! Optional value typing and explicit lazy HIR; PT-BR: tipos opcionais e HIR preguiçoso.

use truxt_sema::{
    analyze, CloneMode, CoalesceMode, HirDeclaration, HirExprKind, HirProgram, HirStatement,
    HirVariableDeclaration, ResolvedType,
};

fn lower(source: &str) -> Result<HirProgram, Vec<truxt_sema::SemaError>> {
    analyze(truxt_parser::parse(&truxt_lexer::tokenize(source).unwrap()).unwrap())
}
fn variable<'a>(program: &'a HirProgram, name: &str) -> &'a HirVariableDeclaration {
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
fn contextual_null_some_and_coalesce_have_explicit_result_modes() {
    let program = lower(
        r#"
        const missing: int? = null;
        const present = option.some(1);
        const value = missing ?? 7;
        const chain = missing ?? present ?? 9;
        const optional: int? = missing ?? null;
        const inferred = null ?? 3;
        const contextual: int? = null ?? null;
        const absent_check = missing == null;
        const present_check = null != present;
    "#,
    )
    .unwrap();
    let optional_int = ResolvedType::Optional(Box::new(ResolvedType::Int));
    assert_eq!(variable(&program, "missing").ty, optional_int);
    assert!(matches!(
        variable(&program, "missing").initializer.kind,
        HirExprKind::OptionNone
    ));
    assert!(matches!(
        variable(&program, "present").initializer.kind,
        HirExprKind::OptionSome(_)
    ));
    assert!(matches!(
        variable(&program, "value").initializer.kind,
        HirExprKind::Coalesce {
            mode: CoalesceMode::Value,
            ..
        }
    ));
    assert!(matches!(
        variable(&program, "optional").initializer.kind,
        HirExprKind::Coalesce {
            mode: CoalesceMode::Optional,
            ..
        }
    ));
    assert_eq!(variable(&program, "chain").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "inferred").ty, ResolvedType::Int);
    assert_eq!(variable(&program, "contextual").ty, optional_int);
    assert_eq!(variable(&program, "absent_check").ty, ResolvedType::Boolean);
    assert_eq!(
        variable(&program, "present_check").ty,
        ResolvedType::Boolean
    );
}

#[test]
fn arrays_and_required_optional_fields_use_context_without_implicit_lifting() {
    let program = lower(
        r#"
        interface Entry { id: int, tag: string? }
        const entry: Entry = { id: 1, tag: null };
        const elements: int?[] = [null, option.some(2)];
        const optional_array: int[]? = option.some([]);
        const array = optional_array ?? [3, 4];
        const some_entry: Entry? = option.some({ id: 2, tag: option.some("x") });
        const chosen: Entry = some_entry ?? { id: 0, tag: null };
    "#,
    )
    .unwrap();
    assert_eq!(
        variable(&program, "elements").ty,
        ResolvedType::Array(Box::new(ResolvedType::Optional(Box::new(
            ResolvedType::Int
        ))))
    );
    assert_eq!(
        variable(&program, "optional_array").ty,
        ResolvedType::Optional(Box::new(ResolvedType::Array(Box::new(ResolvedType::Int))))
    );
    assert_eq!(
        variable(&program, "chosen").ty,
        ResolvedType::Struct("Entry".to_owned())
    );
}

#[test]
fn invalid_null_nested_optional_lifting_fields_and_fallbacks_are_rejected() {
    for source in [
        "const value = null;", "const value = null ?? null;", "const value: int = null;",
        "const value: int? = 1;", "const value: void? = null;",
        "const value = option.some(option.some(1));", "option.some();", "option.some(1, 2);",
        "function nothing(): void {} const value = option.some(nothing());",
        "const value: int? = null; const result = value ?? \"wrong\";",
        "const value = 1 ?? 2;", "option.is_some(1);", "option.unwrap(1);",
        "const value: int? = undefined;", "const option = 1; option.some(2);",
        "interface User { name: string? } const user: User = {};",
        "interface Node { next: Node? }",
        "const value: int? = option.some(1); if option.is_some(value) { const unwrapped: int = value; }",
    ] { assert!(lower(source).is_err(), "must reject: {source}"); }
    lower("interface Node { children: Node?[] }").unwrap();
}

#[test]
fn option_is_excluded_from_every_ffi_position() {
    for source in [
        r#"import function f(value: int?): int from "sample@1.0.0::f";"#,
        r#"import function f(): int? from "sample@1.0.0::f";"#,
        r#"import function f(): Result<int?, string> from "sample@1.0.0::f";"#,
        r#"import function f(values: int?[]): void from "sample@1.0.0::f";"#,
    ] {
        assert!(lower(source).is_err(), "must reject FFI Option: {source}");
    }
}

#[test]
fn expected_context_does_not_cross_a_non_null_coalesce_lhs() {
    assert!(lower("const value: int = null ?? null ?? 1;").is_err());
}

#[test]
fn coalesce_snapshots_and_conditional_fallback_clones_are_preserved() {
    let program = lower(
        r#"
        const source: string? = option.some("present");
        const fallback = "fallback";
        const copy = fallback;
        const value = source ?? fallback;
    "#,
    )
    .unwrap();
    assert!(matches!(
        variable(&program, "copy").initializer.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
    let HirExprKind::Coalesce {
        option, fallback, ..
    } = &variable(&program, "value").initializer.kind
    else {
        panic!("coalesce")
    };
    assert!(matches!(
        option.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
    assert!(matches!(
        fallback.kind,
        HirExprKind::Clone {
            mode: CloneMode::Value,
            ..
        }
    ));
}
