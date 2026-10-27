use std::panic::{catch_unwind, AssertUnwindSafe};

fn analyze_frontend(source: &str) {
    let Ok(tokens) = lift_lexer::tokenize(source) else {
        return;
    };
    for (_, span) in &tokens {
        assert!(
            source.get(span.start..span.end).is_some(),
            "token span must be a valid UTF-8 range: {span:?} in {source:?}"
        );
    }
    if let Ok(ast) = lift_parser::parse(&tokens) {
        let _ = lift_sema::analyze(ast);
    }
}

fn assert_frontend_does_not_panic(source: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| analyze_frontend(source)));
    assert!(result.is_ok(), "frontend panicked for input {source:?}");
}

#[test]
fn every_utf8_prefix_of_nested_language_constructs_is_panic_free() {
    for source in [
        "function sum(values: int[]): int { return values[0] + values[1]; }",
        "interface Counter { value: int } impl Counter { add(n: int): int { this.value = this.value + n; return this.value; } }",
        "const text = `outer ${`inner ${\"á🙂\"}`} end`;",
        "const maybe: int? = null; console.log(maybe ?? 7);",
        "import module \"nested/module.lift\"; console.log(answer());",
    ] {
        for end in source
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(source.len()))
        {
            assert_frontend_does_not_panic(&source[..end]);
        }
    }
}

#[test]
fn seeded_random_utf8_sources_do_not_panic_or_emit_invalid_token_spans() {
    const FRAGMENTS: &[&str] = &[
        "a",
        "_",
        "0",
        "9",
        "\"",
        "'",
        "`",
        "\\",
        "${",
        "}",
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        ";",
        ":",
        ",",
        ".",
        "?",
        "??",
        "+",
        "-",
        "=",
        "==",
        "&&",
        "||",
        "function",
        "interface",
        "impl",
        "return",
        "if",
        "else",
        "while",
        "const",
        "let",
        "int",
        "string",
        "null",
        "true",
        "false",
        "// comment\n",
        "/* comment */",
        "á",
        "🙂",
        "\n",
    ];
    let mut state = 0x4c49_4654_4655_5a5a_u64;
    for case in 0..512 {
        let length = (next_random(&mut state) % 96) as usize;
        let mut source = String::new();
        for _ in 0..length {
            let index = (next_random(&mut state) as usize) % FRAGMENTS.len();
            source.push_str(FRAGMENTS[index]);
        }
        let result = catch_unwind(AssertUnwindSafe(|| analyze_frontend(&source)));
        assert!(
            result.is_ok(),
            "frontend panicked for deterministic case {case}: {source:?}"
        );
    }
}

fn next_random(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}
