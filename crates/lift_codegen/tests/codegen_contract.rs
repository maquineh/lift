//! Rust-source and manifest contracts for validated Lift programs.
//!
//! PT-BR: Contratos do Rust gerado e manifesto para programas Lift validados.

use lift_codegen::{emit_cargo_manifest, emit_rust};
use lift_lexer::tokenize;
use lift_parser::parse;
use lift_sema::{analyze, HirProgram};

fn lower(source: &str) -> HirProgram {
    let tokens = tokenize(source).expect("fonte de teste deve tokenizar");
    let ast = parse(&tokens).expect("fonte de teste deve parsear");
    analyze(ast).expect("fonte de teste deve passar pela semântica")
}

#[test]
fn emits_resource_types_and_safe_mutable_ffi_without_clone() {
    let source = include_str!("../../../examples/notes-service/resources.lift");
    let rust = emit_rust(&lower(source));
    assert!(rust.contains("::lift_db::resource::SqliteConnection"));
    assert!(rust.contains("&mut ::lift_db::resource::SqliteTransaction"));
    assert!(rust.contains("::lift_db::resource::close"));
    assert!(rust.contains("unwrap_or_else(|_| panic!(\"result.unwrap called on Err\"))"));
}

#[test]
fn resource_signature_without_import_still_materializes_its_adapter() {
    let hir = lower("function identity(c: SqliteConnection): SqliteConnection { return c; }");
    let manifest = lift_codegen::emit_cargo_manifest_for_program("identity", &hir, "lift_core");
    assert!(manifest.contains("lift_db = { path = \"lift_db\" }"));
    assert!(emit_rust(&hir).contains("::lift_db::resource::SqliteConnection"));
}

#[test]
fn db_values_in_every_hir_position_request_the_database_runtime() {
    for source in [
        "const parameters: DbValue[] = [];",
        "function unused(): void { const parameters: DbValue[] = []; }",
        "function values(): DbValue[] { return []; }",
        "const local = (): void => { const parameters: DbValue[] = []; };",
        "if true {} else { const parameters: DbValue[] = []; }",
        "interface Values { parameters: DbValue[] }",
    ] {
        let hir = lower(source);
        let manifest = lift_codegen::emit_cargo_manifest_for_program("values", &hir, "lift_core");
        assert!(
            manifest.contains("lift_db = { path = \"lift_db\" }"),
            "missing database runtime for {source}"
        );
    }
}

#[test]
fn bridges_lift_dtos_and_named_handlers_to_generic_http_runtime() {
    let hir = lower(include_str!("../../../examples/notes-service/main.lift"));
    let rust = emit_rust(&hir);
    assert_eq!(rust.matches("::lift_web::http::register_route(").count(), 1);
    assert_eq!(
        rust.matches("::lift_web::http::register_db_route(").count(),
        2
    );
    assert!(rust.contains("request.method"));
    assert!(rust.contains("::lift_web::http::Response"));
    assert!(rust.contains("::lift_web::http::serve_with_pool("));
    let manifest = lift_codegen::emit_cargo_manifest_for_program("notes", &hir, "lift_core");
    assert!(manifest.contains("lift_web = { path = \"lift_web\" }"));
    assert!(manifest.contains("lift_db = { path = \"lift_db\" }"));
}

#[test]
fn emits_owned_option_snapshots_and_a_lazy_match_fallback() {
    let rust = emit_rust(&lower(
        r#"
        const absent: int? = null;
        const present = option.some(7);
        const value = absent ?? present ?? 9;
        console.log(option.is_some(present));
        console.log(option.unwrap(present));
        console.log(value);
    "#,
    ));
    assert!(rust.contains("::std::option::Option::<i64>::None"));
    assert!(rust.contains("::std::option::Option::Some(7_i64)"));
    assert!(rust.contains("match __lift_tmp_"), "{rust}");
    assert!(rust.contains("::std::option::Option::None =>"));
    assert!(rust.contains(".is_some()"));
    assert!(rust.contains("option.unwrap called on None"));
    assert!(!rust.contains("unwrap_or("));
}

#[test]
fn emits_last_use_move_decisions_without_reinferring_ownership() {
    let moved = emit_rust(&lower(
        r#"const source = "a"; const copy = source; console.log(copy);"#,
    ));
    assert!(
        moved.contains("let __lift_binding_1 = __lift_binding_0;"),
        "{moved}"
    );
    let cloned = emit_rust(&lower(
        r#"const source = "a"; const copy = source; console.log(source); console.log(copy);"#,
    ));
    assert!(
        cloned.contains("let __lift_binding_1 = (__lift_binding_0).clone();"),
        "{cloned}"
    );
    let by_clone = emit_rust(&lower(
        r#"function take(value: string): string { return value; } const source = "a"; const copy = take(source);"#,
    ));
    assert!(
        by_clone.contains(".clone()"),
        "ByClone call must stay cloned: {by_clone}"
    );
}

#[test]
fn emits_result_return_types_and_non_consuming_inspection() {
    let hir = lower(
        r#"
        import function read(): Result<string, string> from "lift_core::io::read_stdin";
        import function write(path: string, text: string): Result<void, string> from "lift_core::io::write_file";
        function inspect(value: Result<string, string>): string {
            if result.is_ok(value) { return result.unwrap(value); }
            return result.unwrap_err(value);
        }
        console.log(inspect(read()));
        result.unwrap(write("file.txt", "text"));
    "#,
    );
    let rust = emit_rust(&hir);
    assert!(rust
        .contains("fn() -> ::std::result::Result<String, String> = ::lift_core::io::read_stdin"));
    assert!(rust.contains(
        "fn(&str, &str) -> ::std::result::Result<(), String> = ::lift_core::io::write_file"
    ));
    assert!(rust.contains(".is_ok()"));
    assert!(rust.contains("::std::clone::Clone::clone("));
    assert!(rust.contains(".as_ref().expect("));
    assert!(rust.contains(".as_ref().expect_err("));
}

#[test]
fn emits_import_wrappers_that_require_safe_rust_signatures() {
    let hir = lower(
        r#"
        import function concat(a: string, b: string): string from "lift_core::string::concat";
        import function values(xs: string[], n: int, x: number, b: boolean): string[] from "sample-name@1.2.3::values";
        import function clear(xs: int[]): void from "sample-name@1.2.3::clear";
        console.log(concat("a", "b"));
    "#,
    );
    let rust = emit_rust(&hir);
    assert!(rust.contains("fn(&str, &str) -> String = ::lift_core::string::concat"));
    assert!(rust.contains("fn(&[String], i64, f64, bool) -> Vec<String> = ::sample_name::values"));
    assert!(rust.contains("fn(&[i64]) -> () = ::sample_name::clear"));
    assert!(rust.contains("::lift_core::Console::log(&("));
    assert!(rust.contains(".expect(\"console.log failed to write stdout\")"));
}

#[test]
fn manifest_pins_deduplicated_imports_and_only_adds_core_when_needed() {
    use lift_codegen::emit_cargo_manifest_for_program;
    let plain = lower("const value = 1;");
    assert_eq!(
        emit_cargo_manifest_for_program("plain", &plain, "H:/core"),
        emit_cargo_manifest("plain")
    );
    let imports = lower(
        r#"
        import function f(): int from "sample-name@1.2.3::f";
        import function g(): int from "sample-name@1.2.3::g";
    "#,
    );
    let manifest = emit_cargo_manifest_for_program("demo", &imports, "H:/core");
    assert_eq!(manifest.matches("sample-name = \"=1.2.3\"").count(), 1);
    assert!(!manifest.contains("lift_core"));
    for source in [
        r#"import function f(): string from "lift_core::string::concat";"#,
        "function f(x: boolean): void { if x { console.log(1); } else if !x { while x { console.log(2); } } }",
    ] {
        let hir = lower(source);
        let manifest = emit_cargo_manifest_for_program("demo", &hir, "H:\\a folder\\core");
        assert!(manifest.contains("lift_core = { path = \"H:/a folder/core\" }"));
    }
}

#[test]
fn emits_typed_structs_borrowed_strings_and_string_concatenation() {
    let hir = lower(
        "interface Point { x: int } function label(name: string): string { return name + \"!\"; } const point: Point = { x: 3 }; const text = label(\"A\");",
    );
    let rust = emit_rust(&hir);

    assert!(rust.contains("#[derive(Clone, Debug, PartialEq)]"));
    assert!(
        rust.contains("&str"),
        "parâmetro string lido deve virar &str"
    );
    assert!(
        rust.contains("format!(\"{}{}\""),
        "concatenação deve usar format!"
    );
    assert!(rust.contains("struct __lift_type_"));
    assert!(rust.contains("__lift_field_78: i64"));
    assert!(rust.contains("let __lift_binding_"));
}

#[test]
fn emits_mutable_borrows_and_clones_decided_by_semantics() {
    let hir = lower(
        "interface User { name: string } function rename(user: User): void { user.name = \"Renamed\"; } function take(user: User): User { return user; } let user: User = { name: \"Ada\" }; rename(user); const copy = take(user);",
    );
    let rust = emit_rust(&hir);

    assert!(
        rust.contains("&mut __lift_type_"),
        "BorrowMut deve aparecer na assinatura"
    );
    assert!(
        rust.contains("&mut (__lift_binding_"),
        "ByMutRef deve aparecer na chamada"
    );
    assert!(
        rust.contains(".clone()"),
        "ByClone deve materializar um valor owned"
    );
}

#[test]
fn emits_manifest_with_safe_normalized_name_and_own_workspace() {
    let manifest = emit_cargo_manifest("Demo Lift_42!");

    assert!(manifest.contains("name = \"demo-lift-42\""));
    assert!(manifest.contains("rust-version = \"1.94.1\""));
    assert!(manifest.contains("[workspace]"));
    assert!(manifest.contains("name = \"lift_program\""));
    assert!(manifest.contains("path = \"src/main.rs\""));
}
