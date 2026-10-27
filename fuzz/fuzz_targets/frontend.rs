#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let Ok(tokens) = lift_lexer::tokenize(source) else {
        return;
    };
    let Ok(ast) = lift_parser::parse(&tokens) else {
        return;
    };
    let _ = lift_sema::analyze(ast);
});
