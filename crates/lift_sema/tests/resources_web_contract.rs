use lift_sema::{analyze, HirDeclaration, ParameterPassMode};
fn lower(source: &str) -> Result<lift_sema::HirProgram, Vec<lift_sema::SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}
const IMPORTS: &str = r#"
import function open(path: string): Result<SqliteConnection, string> from "lift_db::resource::open";
import function begin(c: SqliteConnection): Result<SqliteTransaction, string> from "lift_db::resource::begin";
import function insert(t: SqliteTransaction, body: string): Result<int, string> from "lift_db::resource::insert";
import function close(c: SqliteConnection): void from "lift_db::resource::close";
"#;
#[test]
fn resource_imports_record_borrows_and_consumption() {
    let p = lower(&format!("{IMPORTS} const c = result.unwrap(open(\"a\")); const t = result.unwrap(begin(c)); insert(t, \"note\");")).unwrap();
    let imports = p
        .declarations
        .iter()
        .filter_map(|d| {
            if let HirDeclaration::Import(i) = d {
                Some(i)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        imports
            .iter()
            .find(|i| i.name == "begin")
            .unwrap()
            .parameter_modes,
        vec![ParameterPassMode::OwnedMove]
    );
    assert_eq!(
        imports
            .iter()
            .find(|i| i.name == "insert")
            .unwrap()
            .parameter_modes[0],
        ParameterPassMode::BorrowMut
    );
}
#[test]
fn affine_resource_counterexamples_fail_before_codegen() {
    for tail in [
        "const c = result.unwrap(open(\"a\")); close(c); close(c);",
        "function choose(a: SqliteConnection, b: SqliteConnection): SqliteConnection { return b; } const c = result.unwrap(open(\"a\")); const selected = choose(c, c);",
        "function choose(a: SqliteConnection, b: SqliteConnection): SqliteConnection { return b; } function identity(c: SqliteConnection): SqliteConnection { return c; } const c = result.unwrap(open(\"a\")); const selected = choose(c, identity(c));",
        "const c = result.unwrap(open(\"a\")); const alias = c; close(c);",
        "const c = result.unwrap(open(\"a\")); const items = [c];",
        "const c = result.unwrap(open(\"a\")); const callback = (): void => { close(c); };",
        "const callback = (): void => { const c = result.unwrap(open(\"a\")); close(c); close(c); };",
        "const callback = (c: SqliteConnection): void => {};",
        "const c = result.unwrap(open(\"a\")); if true { close(c); } close(c);",
        "const c = result.unwrap(open(\"a\")); while false { close(c); }",
        "const c = result.unwrap(open(\"a\")); while result.is_ok(begin(c)) {}",
        "function bad(c: SqliteConnection): void { close(c); }",
        "interface Bad { c: SqliteConnection }",
        "function bad(c: SqliteConnection?): void {}",
        "import function bad(c: SqliteConnection): void from \"lift_core::interop::bad\";",
        "import function broken(c: SqliteConnection): void from \"lift_db::resource::begin\";",
    ] {
        assert!(
            lower(&format!("{IMPORTS}{tail}")).is_err(),
            "accepted {tail}"
        );
    }
}
const WEB: &str = r#"
interface Request { method: string, path: string, query: string, body: string }
interface Response { status: int, body: string }
function health(request: Request): Response { return {status: 200, body: "ok"}; }
"#;

#[test]
fn general_database_values_and_pool_ownership_are_explicit() {
    lower(include_str!("../../../examples/database/contract.lift")).unwrap();
    lower(include_str!("../../../examples/notes-service/main.lift")).unwrap();
    let setup = r#"
import function create_pool(driver:string,url:string,min_idle:int,max_connections:int,timeout:int): Result<DbPool,string> from "lift_db::api::create_pool";
import function close_pool(pool:DbPool): void from "lift_db::api::close_pool";
const pool = result.unwrap(create_pool("sqlite","sqlite::memory:",1,1,100));
"#;
    for tail in [
        "close_pool(pool); close_pool(pool);",
        "const values = [pool];",
        "const f = (): void => { close_pool(pool); };",
    ] {
        assert!(lower(&format!("{setup}{tail}")).is_err(), "accepted {tail}");
    }
    let web = format!("{WEB} function list(request: Request,pool:DbPool): Response {{ return {{status:200,body:\"ok\"}}; }}");
    lower(&format!("{setup}{web} http.route(\"GET\",\"/notes\",list); const server=http.serve(\"127.0.0.1:3000\",16384,pool);")).unwrap();
    assert!(lower(&format!("{setup}{web} http.route(\"GET\",\"/notes\",list); const server=http.serve(\"127.0.0.1:3000\",16384,pool); close_pool(pool);")).is_err());
}

#[test]
fn closure_argument_evaluation_cannot_hide_resource_borrow_move_aliasing() {
    let prefix = format!("{IMPORTS} function observe(c: SqliteConnection, flag: boolean): void {{}} const pass = (flag: boolean): boolean => flag;");
    for argument in [
        "pass(result.is_ok(begin(c)))",
        "pass(pass(result.is_ok(begin(c))))",
    ] {
        let errors = lower(&format!(
            "{prefix} const c = result.unwrap(open(\"a\")); observe(c, {argument});"
        ))
        .unwrap_err();
        assert!(
            errors.iter().any(|e| e
                .message
                .contains("mesma chamada não pode emprestar e consumir")),
            "{errors:?}"
        );
    }
    lower(&format!("{prefix} const c = result.unwrap(open(\"a\")); const other = result.unwrap(open(\"b\")); observe(c, pass(pass(result.is_ok(begin(other))))); close(c);")).unwrap();
}
#[test]
fn static_named_routes_validate_dtos_and_unique_registration() {
    lower(&format!("{WEB} http.route(\"GET\",\"/health\",health); const server = http.serve(\"127.0.0.1:3000\",16384);")).unwrap();
    for tail in [
        "http.route(\"GET\",\"/health\",health); http.route(\"GET\",\"/health\",health);",
        "http.route(\"GET\",\"/:id\",health);",
        "http.route(\"GET\",\"/{id}\",health);",
        "const path = \"/health\"; http.route(\"GET\",path,health);",
        "if true { http.route(\"GET\",\"/health\",health); }",
        "http.serve(\"127.0.0.1:3000\",16384); http.route(\"GET\",\"/health\",health);",
        "http.route(\"GET\",\"/health\",(): Response => { return {status:200,body:\"ok\"}; });",
    ] {
        assert!(lower(&format!("{WEB}{tail}")).is_err(), "accepted {tail}");
    }
    assert!(lower("interface Request { body: string } interface Response {status:int,body:string} function h(r: Request): Response {return {status:200,body:r.body};} http.route(\"GET\",\"/\",h);").is_err());
}
