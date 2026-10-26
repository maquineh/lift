//! Local package graph and contextual semantics contracts (no Cargo/network).
//! PT-BR: Contratos do grafo local e da semântica contextual, sem Cargo/rede.
use lift_ast::SourceId;
use lift_project::ProjectSession;
use lift_sema::{HirDeclaration, HirExprKind, HirStatement, ParameterPassMode};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lift-packages-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn package(&self, name: &str, kind: &str, deps: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("lift.toml"),format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nkind = \"{kind}\"\nentry = \"main.lift\"\n[dependencies]\n{deps}")).unwrap();
        fs::write(path.join("main.lift"), source).unwrap();
        path.join("main.lift")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn local_packages_keep_identity_privacy_and_transitive_mutation_modes() {
    let temp = Temp::new();
    temp.package(
        "leaf",
        "lib",
        "",
        r#"interface Item { value: int }
        function same(): int {return 1;}
        export function bump(items: int[]): void {items[0] = items[0] + same();}
        export function carry(text: string): string {return text;}
    "#,
    );
    temp.package(
        "middle",
        "lib",
        "leaf = {path = \"../leaf\"}",
        r#"import * as leaf from "leaf";
        interface Item { value: string }
        function same(): int {return 2;}
        export function bump(items: int[]): void {leaf.bump(items);}
        export function carry(text: string): string {return leaf.carry(text);}
    "#,
    );
    let main = temp.package(
        "app",
        "bin",
        "math = {path = \"../middle\"}\nagain = {path = \"../middle/.\"}",
        r#"
        import * as math from "math"; import * as again from "again";
        function forward(items: int[]): void { math.bump(items); }
        const items = [1]; forward(items); console.log(items[0]);
        const text = "kept"; console.log(again.carry(text)); console.log(text);
    "#,
    );
    let session = ProjectSession::new();
    let resolved = session.resolve(&main);
    assert!(
        resolved.diagnostics.is_empty(),
        "{:?}",
        resolved.diagnostics
    );
    assert_eq!(resolved.packages.len(), 3);
    let root = resolved.packages.iter().find(|p| p.name == "app").unwrap();
    assert_eq!(root.dependencies["math"], root.dependencies["again"]);
    let result = session.analyze(&main);
    assert!(result.hir.is_some(), "{:?}", result.diagnostics);
    let hir = result.hir.unwrap();
    let forward = hir
        .declarations
        .iter()
        .find_map(|d| match d {
            HirDeclaration::Function(f) if f.name.ends_with(":forward") => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        forward.parameters[0].pass_mode,
        ParameterPassMode::BorrowMut
    );
    let HirStatement::Expression(statement) = &forward.body.statements[0] else {
        panic!()
    };
    let HirExprKind::Call { function, .. } = &statement.expression.kind else {
        panic!()
    };
    assert!(function.starts_with("$package:"));
    assert!(!function.contains("math."));
    let functions: Vec<_> = hir
        .declarations
        .iter()
        .filter_map(|d| match d {
            HirDeclaration::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        functions
            .iter()
            .filter(|name| name.ends_with(":same"))
            .count(),
        2
    );
    let structures: Vec<_> = hir
        .declarations
        .iter()
        .filter_map(|d| match d {
            HirDeclaration::Struct(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(structures.len(), 2);
    assert_ne!(structures[0], structures[1]);
    assert!(!temp.0.join("app/.lift_cache").exists());
}

#[test]
fn namespace_shadowing_uses_local_methods_and_does_not_promote_external_mutation() {
    let temp = Temp::new();
    temp.package(
        "math",
        "lib",
        "",
        "export function bump(items: int[]): void {items[0] = 2;}",
    );
    let main = temp.package(
        "app",
        "bin",
        "math = {path = \"../math\"}",
        r#"import * as math from "math";
        interface Fake {value: int}
        impl Fake {function bump(items: int[]): void {console.log(items[0]);}}
        function shadow(items: int[]): void {const math: Fake = {value: 1}; math.bump(items);}
        function parameter(math: Fake, items: int[]): void {math.bump(items);}
        const items = [1]; shadow(items); const fake: Fake = {value: 2}; parameter(fake, items);
    "#,
    );
    let result = ProjectSession::new().analyze(main);
    assert!(result.hir.is_some(), "{:?}", result.diagnostics);
    for function in result
        .hir
        .unwrap()
        .declarations
        .iter()
        .filter_map(|d| match d {
            HirDeclaration::Function(f)
                if f.name.ends_with(":shadow") || f.name.ends_with(":parameter") =>
            {
                Some(f)
            }
            _ => None,
        })
    {
        assert_eq!(
            function.parameters.last().unwrap().pass_mode,
            ParameterPassMode::BorrowImm
        );
    }
}

#[test]
fn errors_cover_private_members_custom_exports_undeclared_aliases_and_library_statements() {
    let temp = Temp::new();
    let lib = temp.package("math", "lib", "", "function hidden(): int {return 1;}");
    let main = temp.package(
        "app",
        "bin",
        "math = {path = \"../math\"}",
        "import * as math from \"math\"; console.log(math.hidden());",
    );
    let mut session = ProjectSession::new();
    assert_eq!(
        session.analyze(&main).diagnostics[0].code,
        "sema.PrivatePackageMember"
    );
    session
        .set_overlay(&main, "import * as missing from \"missing\";".to_owned())
        .unwrap();
    assert_eq!(
        session.analyze(&main).diagnostics[0].code,
        "package.undeclared"
    );
    session
        .set_overlay(
            &main,
            "import * as math from \"math\"; function math(): void {}".to_owned(),
        )
        .unwrap();
    assert_eq!(
        session.analyze(&main).diagnostics[0].code,
        "sema.InvalidPackage"
    );
    session.set_overlay(&main, "".to_owned()).unwrap();
    for source in [
        "interface Private {x:int} export function leak(value: Private): void {}",
        "interface Private {x:int} export function leak(value: Private[]): void {}",
    ] {
        session.set_overlay(&lib, source.to_owned()).unwrap();
        assert_eq!(
            session.analyze(&main).diagnostics[0].code,
            "sema.InvalidExport"
        );
    }
    session
        .set_overlay(&lib, "console.log(1);".to_owned())
        .unwrap();
    assert_eq!(
        session.analyze(&main).diagnostics[0].code,
        "project.module-statement"
    );
    let failed = session.analyze(&main);
    let span = failed.diagnostics[0].span.unwrap();
    assert!(failed
        .sources
        .get(span.source)
        .unwrap()
        .path
        .ends_with("math/main.lift"));
}

#[test]
fn manifest_errors_paths_cycles_and_module_boundaries_are_structured() {
    let temp = Temp::new();
    let main = temp.package("app", "bin", "", "console.log(1);");
    let manifest = main.parent().unwrap().join("lift.toml");
    let mut session = ProjectSession::new();
    for (text,code) in [
        ("[package\n", "package.manifest"),
        ("[package]\nname='x'\nversion='1'\nkind='unknown'\nentry='main.lift'", "package.kind"),
        ("[package]\nname='x'\nversion='1'\nkind='bin'\nentry='/absolute.lift'", "package.entry"),
        ("[package]\nname='x'\nversion='1'\nkind='bin'\nentry='../not-saved.lift'", "package.boundary"),
        ("[package]\nname='x'\nversion='1'\nkind='bin'\nentry='main.lift'\n[dependencies]\nx={path='../missing'}", "package.io"),
    ] {session.set_overlay(&manifest,text.to_owned()).unwrap();let result=session.resolve(&main);assert_eq!(result.diagnostics[0].code,code,"{:?}",result.diagnostics);let span=result.diagnostics[0].span.unwrap();assert_eq!(result.sources.get(span.source).unwrap().path,lift_project::source_path(&manifest).unwrap());}
    session.remove_overlay(&manifest).unwrap();
    let lib = temp.package("lib", "lib", "app={path='../app'}", "");
    fs::write(&manifest,"[package]\nname='app'\nversion='1'\nkind='bin'\nentry='main.lift'\n[dependencies]\nlib={path='../lib'}").unwrap();
    assert!(session
        .resolve(&main)
        .diagnostics
        .iter()
        .any(|d| d.code == "package.cycle"));
    fs::write(
        lib.parent().unwrap().join("lift.toml"),
        "[package]\nname='lib'\nversion='1'\nkind='lib'\nentry='main.lift'",
    )
    .unwrap();
    session
        .set_overlay(&main, "import module \"../lib/main.lift\";".to_owned())
        .unwrap();
    assert_eq!(
        session.analyze(&main).diagnostics[0].code,
        "package.boundary"
    );
}

#[test]
fn legacy_flat_modules_and_analyze_api_stay_separate_from_packages() {
    let temp = Temp::new();
    let main = temp.0.join("main.lift");
    let module = temp.0.join("module.lift");
    fs::write(&main, "import module \"module.lift\"; console.log(same());").unwrap();
    fs::write(module, "function same():int {return 1;}").unwrap();
    let result = ProjectSession::new().analyze(main);
    assert!(result.hir.is_some());
    assert!(result.sources.get(SourceId(0)).is_some());
    let tokens = lift_lexer::tokenize("export function public():void {}").unwrap();
    let ast = lift_parser::parse(&tokens).unwrap();
    assert!(lift_sema::analyze(ast).is_err());
}

#[test]
fn resolution_analysis_consumes_snapshot_once_and_unimported_sources_are_explicit_errors() {
    let temp = Temp::new();
    let main = temp.package("app", "bin", "", "console.log(1);");
    let extra = main.parent().unwrap().join("extra.lift");
    fs::write(&extra, "const bad:int = true;").unwrap();
    let session = ProjectSession::new();
    let outside = session.analyze(&extra);
    assert_eq!(outside.diagnostics[0].code, "package.outside-graph");
    assert!(outside.hir.is_none());
    let span = outside.diagnostics[0].span.unwrap();
    assert!(outside
        .sources
        .get(span.source)
        .unwrap()
        .path
        .ends_with("extra.lift"));
    // resolve(source) uses the source only to locate the manifest, unlike analyze.
    let resolved = session.resolve(&extra);
    assert!(resolved.project.is_some());
    assert!(resolved.entry.ends_with("main.lift"));
    fs::remove_file(&main).unwrap();
    fs::remove_file(main.parent().unwrap().join("lift.toml")).unwrap();
    let snapshot = resolved.analyze();
    assert!(snapshot.hir.is_some(), "{:?}", snapshot.diagnostics);
    assert!(session.analyze(main).has_errors());
}

#[test]
fn builtins_are_not_captured_by_package_aliases_and_rust_ffi_versions_conflict_globally() {
    let temp = Temp::new();
    temp.package(
        "math",
        "lib",
        "",
        "export function log(value: int): void {}",
    );
    let main = temp.package("app", "bin", "math={path='../math'}", "console.log(1);");
    let mut session = ProjectSession::new();
    for reserved in ["console", "option", "result"] {
        session
            .set_overlay(&main, format!("import * as {reserved} from \"math\";"))
            .unwrap();
        assert_eq!(
            session.analyze(&main).diagnostics[0].code,
            "package.reserved-alias"
        );
    }
    session
        .set_overlay(&main, "console.log(1);".to_owned())
        .unwrap();
    assert!(session.analyze(&main).hir.is_some());
    temp.package(
        "one",
        "lib",
        "",
        "import function gcd(a:int,b:int):int from \"num-integer@0.1.46::gcd\";",
    );
    temp.package(
        "two",
        "lib",
        "",
        "import function gcd(a:int,b:int):int from \"num-integer@0.1.45::gcd\";",
    );
    let manifest = main.parent().unwrap().join("lift.toml");
    session.set_overlay(&manifest,"[package]\nname='app'\nversion='1'\nkind='bin'\nentry='main.lift'\n[dependencies]\none={path='../one'}\ntwo={path='../two'}".to_owned()).unwrap();
    let result = session.analyze(&main);
    assert_eq!(result.diagnostics[0].code, "sema.InvalidImport");
    assert!(result.diagnostics[0].secondary_span.is_some());
}
