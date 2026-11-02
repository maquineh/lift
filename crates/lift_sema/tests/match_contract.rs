//! `match` de `Option`/`Result`: tipagem, exaustividade, clones e fluxo.
//!
//! PT-BR: contratos de `match` exaustivo de Option/Result.

use lift_sema::{
    analyze, CloneMode, HirDeclaration, HirExprKind, HirFunction, HirMatchPattern, HirProgram,
    HirStatement, ResolvedType, SemaError, SemaErrorKind,
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

#[test]
fn match_option_and_result_patterns_are_typed() {
    let program = lower(
        r#"
        interface Point { value: int }
        function describe(value: int?): string {
            match value {
                some(present) => { return `some ${present}`; }
                none => { return "none"; }
            }
        }
        function risky(value: int): Result<int, string> {
            if value < 0 { throw "negative"; }
            return result.ok(value);
        }
        function describe_result(value: int): string {
            match risky(value) {
                ok(ok_value) => { return `ok ${ok_value}`; }
                err(error) => { return `err ${error}`; }
            }
        }
        function make_void(): Result<void, string> { return result.ok(); }
        function describe_void(): string {
            match make_void() {
                ok() => { return "void ok"; }
                err(error) => { return `err ${error}`; }
            }
        }
        "#,
    )
    .unwrap();
    let describe = function(&program, "describe");
    let HirStatement::Match(match_) = &describe.body.statements[0] else {
        panic!("match")
    };
    assert!(matches!(
        match_.value.kind,
        HirExprKind::Clone {
            mode: CloneMode::BorrowedAggregate,
            ..
        }
    ));
    let HirMatchPattern::Some(binding) = &match_.arms[0].pattern else {
        panic!("some")
    };
    assert_eq!(binding.ty, ResolvedType::Int);
    assert!(!binding.requires_mut);
    assert!(matches!(match_.arms[1].pattern, HirMatchPattern::None));

    let describe_result = function(&program, "describe_result");
    let HirStatement::Match(match_) = &describe_result.body.statements[0] else {
        panic!("match result")
    };
    let HirMatchPattern::Ok(Some(ok)) = &match_.arms[0].pattern else {
        panic!("ok")
    };
    assert_eq!(ok.ty, ResolvedType::Int);
    let HirMatchPattern::Err(err) = &match_.arms[1].pattern else {
        panic!("err")
    };
    assert_eq!(err.ty, ResolvedType::String);

    let describe_void = function(&program, "describe_void");
    let HirStatement::Match(match_) = &describe_void.body.statements[0] else {
        panic!("match void")
    };
    assert!(matches!(match_.arms[0].pattern, HirMatchPattern::Ok(None)));
}

#[test]
fn match_is_exhaustive_and_rejects_incompatible_patterns() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            r#"function f(value: int?): int { match value { some(present) => { return present; } } }"#,
            SemaErrorKind::InvalidMatch,
            "não é exaustivo",
        ),
        (
            r#"function f(value: int?): int { match value { some(present) => { return present; } some(other) => { return other; } none => { return 0; } } }"#,
            SemaErrorKind::InvalidMatch,
            "duplicado",
        ),
        (
            r#"function f(value: int?): int { match value { ok(present) => { return present; } none => { return 0; } } }"#,
            SemaErrorKind::InvalidMatch,
            "incompatível",
        ),
        (
            r#"const value = 1; match value { some(x) => { console.log(x); } none => { console.log(0); } }"#,
            SemaErrorKind::TypeMismatch,
            "Option",
        ),
        (
            r#"function make(): Result<int, string> { return result.ok(1); } function f(): int { match make() { ok() => { return 1; } err(error) => { return 0; } } }"#,
            SemaErrorKind::InvalidMatch,
            "ok()",
        ),
        (
            r#"function make(): Result<void, string> { return result.ok(); } function f(): int { match make() { ok(present) => { return 1; } err(error) => { return 0; } } }"#,
            SemaErrorKind::InvalidMatch,
            "ok(nome)",
        ),
        (
            r#"function f(value: int?): int { match value { some(present) => { present = 2; return present; } none => { return 0; } } }"#,
            SemaErrorKind::ImmutableBinding,
            "const",
        ),
        (
            r#"function f(value: int?): int { match value { some(present) => { return present; } none => { console.log(0); } } }"#,
            SemaErrorKind::MissingReturnPath,
            "retornar",
        ),
        (
            r#"function f(value: int?, other: int?): int { match other { some(value) => { return value; } none => { return 0; } } }"#,
            SemaErrorKind::DuplicateDeclaration,
            "declarado",
        ),
        (
            r#"import function create_pool(kind: string, target: string, max_size: int, min_idle: int, timeout: int): Result<DbPool, string> from "lift_db::api::create_pool"; function f(): int { const pending = create_pool("sqlite", ":memory:", 1, 1, 10); match pending { ok(pool) => { return 1; } err(error) => { return 0; } } }"#,
            SemaErrorKind::UnsupportedType,
            "recurso opaco",
        ),
    ];
    for (source, kind, message) in cases {
        let errors = lower(source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.kind == *kind
                && (message.is_empty() || error.message.contains(message))),
            "expected {kind:?}/{message} in {errors:?}"
        );
    }
}

#[test]
fn match_clones_the_value_and_allows_payload_field_mutation() {
    let program = lower(
        r#"
        interface Point { value: int }
        function mutate(point: Point?): int {
            match point {
                some(inner) => { inner.value = 9; return inner.value; }
                none => { return 0; }
            }
        }
        const stored: Point? = option.some({ value: 1 });
        match stored {
            some(point) => { point.value = 5; console.log(point.value); }
            none => { console.log(0); }
        }
        console.log(option.unwrap(stored).value);
        "#,
    )
    .unwrap();
    let mutate = function(&program, "mutate");
    let HirStatement::Match(match_) = &mutate.body.statements[0] else {
        panic!("match")
    };
    let HirMatchPattern::Some(binding) = &match_.arms[0].pattern else {
        panic!("some")
    };
    assert!(binding.requires_mut);
    let top_level_match = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Statement(HirStatement::Match(match_)) => Some(match_),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        top_level_match.value.kind,
        HirExprKind::Clone { .. }
    ));
    let HirMatchPattern::Some(binding) = &top_level_match.arms[0].pattern else {
        panic!("some")
    };
    assert!(binding.requires_mut);
}

#[test]
fn match_allows_loop_control_and_throw_inside_arms() {
    let program = lower(
        r#"
        function first_present(values: int?[]): int {
            let found = -1;
            for (const value of values) {
                match value {
                    some(present) => { found = present; break; }
                    none => {}
                }
            }
            return found;
        }
        function risky(value: int): Result<int, string> {
            if value < 0 { throw "negative"; }
            return result.ok(value);
        }
        function fallback(value: int): Result<int, string> {
            match risky(value) {
                ok(present) => { return result.ok(present); }
                err(error) => { throw "fallback"; }
            }
        }
        "#,
    )
    .unwrap();
    let first_present = function(&program, "first_present");
    let HirStatement::ForOf(loop_) = &first_present.body.statements[1] else {
        panic!("for")
    };
    let HirStatement::Match(match_) = &loop_.body.statements[0] else {
        panic!("match")
    };
    let HirStatement::Break(target) = &match_.arms[0].body.statements[1] else {
        panic!("break")
    };
    assert_eq!(target.target, loop_.label);
    let fallback = function(&program, "fallback");
    let HirStatement::Match(match_) = &fallback.body.statements[0] else {
        panic!("match")
    };
    assert!(matches!(
        match_.arms[1].body.statements[0],
        HirStatement::Throw(_)
    ));
}
