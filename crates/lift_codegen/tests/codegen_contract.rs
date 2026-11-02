//! Rust-source and manifest contracts for validated Lift programs.
//!
//! PT-BR: Contratos do Rust gerado e manifesto para programas Lift validados.

use lift_codegen::{emit_cargo_manifest, emit_cargo_manifest_for_program, emit_rust};
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
fn emits_external_layout_assertions_and_option_struct_ffi_conversions() {
    let hir = lower(
        r#"
        import interface P { x: int, tag?: string } from "sample@1.0.0::P";
        import function make(x: int): P from "sample@1.0.0::make";
        import function read(value: P?, text: string?, items: P[]?): string? from "sample@1.0.0::read";
        const built: P = { x: 2 };
        const p = make(1);
        console.log(read(option.some(p), null, null) ?? "none");
        console.log(built.x);
    "#,
    );
    let rust = emit_rust(&hir);
    assert!(rust.contains("fn __lift_extern_layout"), "{rust}");
    assert!(rust.contains("__lift_value.x"), "{rust}");
    assert!(rust.contains("__lift_value.tag"), "{rust}");
    assert!(rust.contains("__lift_assert::<sample::P>();"), "{rust}");
    assert!(
        rust.contains(
            "let __lift_ffi: fn(Option<&sample::P>, Option<&str>, Option<&[sample::P]>) -> ::std::option::Option<String>"
        ),
        "{rust}"
    );
    assert!(
        rust.contains(
            "__lift_ffi(__lift_arg_0.as_ref(), __lift_arg_1.as_deref(), __lift_arg_2.as_deref())"
        ),
        "{rust}"
    );
    assert!(
        rust.contains("sample::P { x: 2_i64, tag: ::std::option::Option::<String>::None }"),
        "{rust}"
    );
    assert!(rust.contains("(__lift_binding_0).x"), "{rust}");
    let manifest = emit_cargo_manifest_for_program("probe", &hir, "lift_core");
    assert!(manifest.contains("sample = \"=1.0.0\""), "{manifest}");

    let local =
        lower(r#"import interface Range { start: int } from "lift_core::interop::IntRange";"#);
    let manifest = emit_cargo_manifest_for_program("probe", &local, "lift_core");
    assert!(
        manifest.contains("lift_core = { path = \"lift_core\" }"),
        "{manifest}"
    );
}

#[test]
fn emits_enums_and_qualified_match_patterns() {
    let rust = emit_rust(&lower(
        r#"
        enum Shape { Circle(number), Rect(number, number), Empty }
        function area(shape: Shape): number {
            match shape {
                Shape.Circle(radius) => { return 3.14 * radius * radius; }
                Shape.Rect(width, height) => { return width * height; }
                Shape.Empty => { return 0.0; }
            }
        }
        const value: Shape = Shape.Circle(2.0);
        console.log(area(value));
        "#,
    ));
    assert!(
        rust.contains("#[derive(Clone, Debug, PartialEq)]"),
        "{rust}"
    );
    assert!(rust.contains("enum __lift_type_"), "{rust}");
    assert!(rust.contains("__lift_variant_"), "{rust}");
    assert!(rust.contains("match "), "{rust}");
    assert!(rust.contains("::__lift_variant_"), "{rust}");
    assert!(rust.contains("(f64)"), "{rust}");
}

#[test]
fn emits_named_enum_variants_with_object_payloads() {
    let rust = emit_rust(&lower(
        r#"
        enum Shape {
            Circle { radius: number },
            Empty,
        }
        function area(shape: Shape): number {
            match shape {
                Shape.Circle(radius) => { return radius; }
                Shape.Empty => { return 0.0; }
            }
        }
        const value: Shape = Shape.Circle({ radius: 2.0 });
        console.log(area(value));
        "#,
    ));
    assert!(rust.contains("enum __lift_type_"), "{rust}");
    assert!(rust.contains("__lift_variant_"), "{rust}");
    assert!(rust.contains("__lift_field_"), "{rust}");
    assert!(rust.contains(" { "), "{rust}");
    assert!(rust.contains("__lift_field_"), "{rust}");
}

#[test]
fn emits_exhaustive_matches_for_option_and_result() {
    let rust = emit_rust(&lower(
        r#"
        interface Point { value: int }
        function describe(value: int?): string {
            match value {
                some(present) => { return `some ${present}`; }
                none => { return "none"; }
            }
        }
        function make(): Result<int, string> { return result.ok(1); }
        function use(): int {
            match make() {
                ok(value) => { return value; }
                err(error) => { return 0; }
            }
        }
        function make_void(): Result<void, string> { return result.ok(); }
        function use_void(): int {
            match make_void() {
                ok() => { return 1; }
                err(error) => { return 0; }
            }
        }
        const point: Point? = option.some({ value: 1 });
        match point {
            some(inner) => { inner.value = 2; }
            none => {}
        }
        "#,
    ));
    assert!(rust.contains("match "), "{rust}");
    assert!(rust.contains("Some("), "{rust}");
    assert!(rust.contains("None => {"), "{rust}");
    assert!(rust.contains("Ok("), "{rust}");
    assert!(rust.contains("Err("), "{rust}");
    assert!(rust.contains("Ok(()) => {"), "{rust}");
    assert!(rust.contains("Some(mut __lift_binding_"), "{rust}");
}

#[test]
fn emits_iterator_bridges_for_array_methods() {
    let rust = emit_rust(&lower(
        r#"
        function sum(values: int[]): int {
            return values.reduce(0, (accumulator: int, value: int): int => accumulator + value);
        }
        const values = [1, 2, 3];
        const doubled = values.map((value: int): int => value * 2);
        const labels = values.map((value: int): string => `#${value}`);
        const evens = values.filter((value: int): boolean => value % 2 == 0);
        const found = values.find((value: int): boolean => value > 1);
        const any = values.some((value: int): boolean => value > 2);
        const all = values.every((value: int): boolean => value > 0);
        values.forEach((value: int): void => { console.log(value); });
        console.log(sum(values));
        "#,
    ));
    assert!(
        rust.contains(".into_iter().map(|__lift_item| (")
            && rust.contains(".collect::<::std::vec::Vec<i64>>()"),
        "{rust}"
    );
    assert!(
        rust.contains(".collect::<::std::vec::Vec<String>>()"),
        "{rust}"
    );
    assert!(
        rust.contains(".into_iter().filter(|__lift_item| ("),
        "{rust}"
    );
    assert!(rust.contains(".into_iter().find(|__lift_item| ("), "{rust}");
    assert!(rust.contains(".into_iter().any(|__lift_item| ("), "{rust}");
    assert!(rust.contains(".into_iter().all(|__lift_item| ("), "{rust}");
    assert!(rust.contains(".into_iter().fold("), "{rust}");
    assert!(rust.contains("for __lift_item in"), "{rust}");
}

#[test]
fn emits_labeled_loops_and_loop_control() {
    let rust = emit_rust(&lower(
        r#"
        function scan(values: int[]): int {
            let count = 0;
            for (const value of values) {
                if value == 2 { break; }
                if value == 0 { continue; }
                count = count + 1;
            }
            let index = 0;
            while index < 3 {
                index = index + 1;
                if index == 2 { continue; }
            }
            return count;
        }
        "#,
    ));
    assert!(rust.contains("'lift_loop_0: for"), "{rust}");
    assert!(rust.contains("break 'lift_loop_0;"), "{rust}");
    assert!(rust.contains("continue 'lift_loop_0;"), "{rust}");
    assert!(rust.contains("'lift_loop_1: while"), "{rust}");
    assert!(rust.contains("continue 'lift_loop_1;"), "{rust}");
    assert!(rust.contains(").clone()"), "{rust}");
}

#[test]
fn emits_rc_callbacks_and_ffi_callback_bridges() {
    let rust = emit_rust(&lower(
        r#"
        import function apply_twice(value: int, callback: (int) => int): int from "lift_core::interop::apply_twice";
        import function add_constant(amount: int): (int) => int from "lift_core::interop::add_constant";
        function double(value: int): int { return value * 2; }
        function apply(value: int, transform: (int) => int): int { return transform(value); }
        const stored: (int) => int = double;
        console.log(apply(2, stored));
        console.log(apply_twice(2, double));
        const made = add_constant(3);
        console.log(made(1));
    "#,
    ));
    assert!(
        rust.contains("__lift_binding_2: ::std::rc::Rc<dyn Fn(i64) -> i64>"),
        "{rust}"
    );
    assert!(rust.contains("::std::rc::Rc::new(move |"), "{rust}");
    assert!(
        rust.contains("as ::std::rc::Rc<dyn Fn(i64) -> i64>"),
        "{rust}"
    );
    assert!(
        rust.contains("fn(i64, &dyn Fn(i64) -> i64) -> i64"),
        "{rust}"
    );
    assert!(rust.contains("&*__lift_arg_1"), "{rust}");
    assert!(
        rust.contains("fn(i64) -> ::std::boxed::Box<dyn Fn(i64) -> i64>"),
        "{rust}"
    );
    assert!(rust.contains("::std::rc::Rc::from("), "{rust}");
    assert!(
        rust.contains("move |__lift_cb_0: i64| -> i64 { return __lift_fn_"),
        "{rust}"
    );
}

#[test]
fn emits_result_constructors_propagation_and_try_catch() {
    let rust = emit_rust(&lower(
        r#"
        function source(value: int): Result<int, string> {
            return result.ok(value);
        }
        function parse(value: int): Result<int, string> {
            const parsed = source(value)?;
            return result.ok(parsed + 1);
        }
        function fallback(value: int): int {
            try {
                const parsed = source(value)?;
                return parsed;
            } catch (error: string) {
                console.log(error);
                return -1;
            }
        }
        const failed: Result<int, string> = result.err("no");
        console.log(result.is_ok(failed));
    "#,
    ));
    assert!(
        rust.contains("::std::result::Result::<i64, String>::Ok("),
        "{rust}"
    );
    assert!(
        rust.contains("::std::result::Result::<i64, String>::Err("),
        "{rust}"
    );
    assert!(rust.contains(")?;"), "{rust}");
    assert!(rust.contains("'lift_try_0: {"), "{rust}");
    assert!(
        rust.contains("break 'lift_try_0 ::std::result::Result::Err("),
        "{rust}"
    );
    assert!(
        rust.contains("::std::result::Result::Err(__lift_binding_"),
        "{rust}"
    );
    assert!(rust.contains("unreachable!();"), "{rust}");
}

#[test]
fn emits_borrowed_result_parameters_and_option_payload_nesting() {
    let rust = emit_rust(&lower(
        r#"
        import function start(): Result<int?, string> from "sample@1.0.0::start";
        import function inspect(result: Result<int?, string>): Result<int?, string> from "sample@1.0.0::inspect";
        const first = start();
        const second = inspect(first);
        console.log(result.is_ok(second));
    "#,
    ));
    assert!(
        rust.contains(
            "fn __lift_fn_696e7370656374(__lift_arg_0: &::std::result::Result<::std::option::Option<i64>, String>)"
        ),
        "{rust}"
    );
    assert!(
        rust.contains(
            "let __lift_ffi: fn(&::std::result::Result<::std::option::Option<i64>, String>) -> ::std::result::Result<::std::option::Option<i64>, String>"
        ),
        "{rust}"
    );
    assert!(rust.contains("__lift_ffi(__lift_arg_0)"), "{rust}");
    assert!(
        rust.contains("__lift_fn_696e7370656374(&(__lift_binding_0))"),
        "{rust}"
    );
}

#[test]
fn emits_optional_struct_fields_and_narrowed_payload_reads() {
    let rust = emit_rust(&lower(
        r#"
        interface Profile { name: string, nickname?: string }
        function label(value: string?): string {
            if value == null { return "none"; }
            return value;
        }
        function show(value: string?): void {
            if value == null { return; }
            console.log(value);
        }
        const profile: Profile = { name: "Ada" };
        console.log(profile.nickname ?? label(null));
        show(null);
    "#,
    ));
    assert!(
        rust.contains("__lift_field_6e69636b6e616d65: ::std::option::Option<String>"),
        "{rust}"
    );
    assert!(
        rust.contains("__lift_field_6e69636b6e616d65: ::std::option::Option::<String>::None"),
        "{rust}"
    );
    // Parâmetro owned: o narrowing lê o Option por referência compartilhada.
    assert!(
        rust.contains("::std::option::Option::as_ref(&__lift_binding_"),
        "{rust}"
    );
    // Parâmetro emprestado na assinatura Rust (`&Option<String>`).
    assert!(
        rust.contains("::std::option::Option::as_ref(&*__lift_binding_"),
        "{rust}"
    );
    assert!(
        rust.contains("narrowed optional value must be Some"),
        "{rust}"
    );
    assert!(rust.contains("Console::log(&(__lift_binding_"), "{rust}");
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
