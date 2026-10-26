//! Public project/session contracts, including unsaved multi-source diagnostics.
//! PT-BR: Contratos públicos de sessão e diagnósticos multi-fonte não salvos.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use truxt_ast::SourceId;
use truxt_project::{lint::lint, ProjectSession, Severity, SourceFile};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "truxt-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn overlays_cross_modules_retain_sources_on_failure_and_reload_after_changes() {
    let temp = Temp::new();
    let main = temp.0.join("main.truxt");
    let module = temp.0.join("dep.truxt");
    fs::write(&main, "import module \"dep.truxt\"; console.log(answer());").unwrap();
    fs::write(&module, "function answer(): int { return 7; }").unwrap();
    let mut session = ProjectSession::new();
    let first = session.analyze(&main);
    assert!(first.hir.is_some());
    session
        .set_overlay(
            &module,
            "function answer(): int { return true; }".to_owned(),
        )
        .unwrap();
    let failed = session.analyze(&main);
    assert!(failed.has_errors());
    assert!(failed.hir.is_none());
    let diagnostic = &failed.diagnostics[0];
    assert_eq!(diagnostic.severity, Severity::Error);
    let span = diagnostic.span.unwrap();
    assert_eq!(span.source, SourceId(1));
    let source = failed.sources.get(span.source).unwrap();
    assert!(source.path.ends_with("dep.truxt"));
    assert!(source.text[span.start..span.end].contains("true"));
    assert!(fs::read_to_string(&module).unwrap().contains("return 7"));
    session.remove_overlay(&module).unwrap();
    assert!(session.analyze(&main).hir.is_some());
    fs::write(&module, "function answer(): int { return false; }").unwrap();
    assert!(session.analyze(&main).has_errors());
    assert!(!temp.0.join(".truxt_cache").exists());
    assert_eq!(
        first.sources.get(SourceId(1)).unwrap().text,
        "function answer(): int { return 7; }"
    );
}

#[test]
fn unsaved_roots_and_modules_support_cycles_and_preserve_duplicate_labels() {
    let temp = Temp::new();
    let main = temp.0.join("new.truxt");
    let module = temp.0.join("dep.truxt");
    let mut session = ProjectSession::new();
    session
        .set_overlay(
            &main,
            "import module \"dep.truxt\"; function same(): void {}".to_owned(),
        )
        .unwrap();
    session
        .set_overlay(
            &module,
            "import module \"new.truxt\"; function same(): void {}".to_owned(),
        )
        .unwrap();
    let result = session.analyze(&main);
    assert_eq!(result.sources.iter().count(), 2);
    let duplicate = result
        .diagnostics
        .iter()
        .find(|d| d.secondary_span.is_some())
        .unwrap();
    assert_ne!(
        duplicate.span.unwrap().source,
        duplicate.secondary_span.unwrap().source
    );
    assert!(!main.exists());
    assert!(!module.exists());
}

#[test]
fn io_lexer_parser_and_import_failures_keep_structured_errors() {
    let temp = Temp::new();
    let main = temp.0.join("main.truxt");
    let mut session = ProjectSession::new();
    let missing = session.analyze(&main);
    assert!(missing.has_errors());
    assert!(missing.diagnostics[0].span.is_none());
    for (text, code, files) in [
        ("import module \"missing.truxt\";", "project.io", 1),
        ("@", "lexer.invalid-token", 1),
        ("function unfinished(): void {", "parser.syntax", 1),
        ("import module \"absolute.txt\";", "project.import-path", 1),
    ] {
        session.set_overlay(&main, text.to_owned()).unwrap();
        let result = session.analyze(&main);
        assert!(result.has_errors());
        assert!(result.hir.is_none());
        assert_eq!(result.sources.iter().count(), files);
        assert_eq!(result.diagnostics[0].code, code);
        assert!(result.diagnostics[0].span.is_some());
    }
    session
        .set_overlay(&main, "import module \"dep.truxt\";".to_owned())
        .unwrap();
    session
        .set_overlay(temp.0.join("dep.truxt"), "const illegal = 1;".to_owned())
        .unwrap();
    assert_eq!(
        session.analyze(&main).diagnostics[0].code,
        "project.module-statement"
    );
}

#[test]
fn semantic_diagnostic_codes_are_public_stable_contract_values() {
    let temp = Temp::new();
    let path = temp.0.join("codes.truxt");
    let mut session = ProjectSession::new();
    for (text, code) in [
        ("const value: int = true;", "sema.TypeMismatch"),
        ("console.log(missing);", "sema.UnknownVariable"),
        ("missing();", "sema.UnknownFunction"),
        ("const value = 1; value = 2;", "sema.ImmutableBinding"),
        (
            "function same(): void {} function same(): void {}",
            "sema.DuplicateDeclaration",
        ),
        ("function f(): int {}", "sema.MissingReturnPath"),
    ] {
        session.set_overlay(&path, text.to_owned()).unwrap();
        let analysis = session.analyze(&path);
        assert_eq!(analysis.diagnostics[0].code, code, "{text}");
    }
}

#[test]
fn utf16_positions_cover_astral_unicode_crlf_and_eof() {
    let source = SourceFile {
        path: "unicode.truxt".into(),
        text: "á🙂x\r\nβ".to_owned(),
    };
    for (byte, line, character) in [
        (0, 0, 0),
        (2, 0, 1),
        (3, 0, 1),
        (6, 0, 3),
        (7, 0, 4),
        (8, 0, 4),
        (9, 1, 0),
        (11, 1, 1),
        (999, 1, 1),
    ] {
        let position = source.position_utf16(byte);
        assert_eq!(
            (position.line, position.character),
            (line, character),
            "byte {byte}"
        );
    }
}

#[test]
fn utf16_positions_recognize_lf_crlf_and_lone_cr() {
    for separator in ["\n", "\r\n", "\r"] {
        let text = format!("á🙂{separator}β{separator}");
        let source = SourceFile {
            path: "lines.truxt".into(),
            text,
        };
        for (byte, line, character) in [
            (6, 0, 3),
            (6 + separator.len(), 1, 0),
            (8 + separator.len(), 1, 1),
            (source.text.len(), 2, 0),
        ] {
            let position = source.position_utf16(byte);
            assert_eq!(
                (position.line, position.character),
                (line, character),
                "{separator:?}, byte {byte}"
            );
        }
    }
}

#[test]
#[cfg(any(unix, windows))]
fn overlays_survive_first_save_through_a_directory_alias() {
    let temp = Temp::new();
    let real = temp.0.join("real");
    let alias = temp.0.join("alias");
    fs::create_dir(&real).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    #[cfg(windows)]
    {
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", "New-Item -ItemType Junction -Path $env:TRUXT_TEST_LINK -Target $env:TRUXT_TEST_TARGET | Out-Null"])
            .env("TRUXT_TEST_LINK", &alias).env("TRUXT_TEST_TARGET", &real).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let entry = alias.join("new.truxt");
    let mut session = ProjectSession::new();
    session
        .set_overlay(&entry, "const _valid = 1;".to_owned())
        .unwrap();
    assert!(session.analyze(&entry).hir.is_some());
    fs::write(&entry, "const bad: int = true;").unwrap();
    assert!(session.analyze(&entry).hir.is_some());
    session
        .set_overlay(&entry, "const bad: int = false;".to_owned())
        .unwrap();
    assert!(session.analyze(&entry).has_errors());
    session
        .set_overlay(&entry, "const _valid = 2;".to_owned())
        .unwrap();
    // Real-path import aliases and cycles still deduplicate canonically.
    let root = temp.0.join("main.truxt");
    session
        .set_overlay(
            &root,
            "import module \"alias/new.truxt\"; import module \"real/new.truxt\";".to_owned(),
        )
        .unwrap();
    let imported = session.analyze(&root);
    // Root-only statement restriction still applies; there is one module source.
    assert_eq!(imported.sources.iter().count(), 2);
    assert_eq!(imported.diagnostics.len(), 1);
    session.remove_overlay(&entry).unwrap();
    assert!(session.analyze(&entry).has_errors());
}

#[test]
#[cfg(any(unix, windows))]
fn parent_components_preserve_filesystem_alias_semantics_and_canonical_dedup() {
    let temp = Temp::new();
    let target = temp.0.join("outside").join("inner");
    let alias = temp.0.join("link");
    fs::create_dir_all(&target).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    #[cfg(windows)]
    {
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", "New-Item -ItemType Junction -Path $env:TRUXT_TEST_LINK -Target $env:TRUXT_TEST_TARGET | Out-Null"])
            .env("TRUXT_TEST_LINK", &alias).env("TRUXT_TEST_TARGET", &target).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let entry = alias.join("..").join("file.truxt");
    // Absent files retain absolute spelling, including .., on every host.
    assert_eq!(truxt_project::source_path(&entry).unwrap(), entry);
    let text = "function answer(): int { return 7; }";
    let mut session = ProjectSession::new();
    session.set_overlay(&entry, text.to_owned()).unwrap();
    assert!(session.analyze(&entry).hir.is_some());
    fs::write(&entry, "function answer(): int { return true; }").unwrap();
    let canonical = truxt_project::source_path(&entry).unwrap();
    #[cfg(unix)]
    assert_eq!(
        canonical,
        truxt_project::source_path(&temp.0.join("outside").join("file.truxt")).unwrap()
    );
    let analysis = session.analyze(&entry);
    assert!(analysis.hir.is_some(), "{:?}", analysis.diagnostics);
    assert_eq!(analysis.sources.get(SourceId(0)).unwrap().text, text);
    let relative = canonical
        .strip_prefix(truxt_project::source_path(&temp.0).unwrap())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let root = temp.0.join("root.truxt");
    session.set_overlay(&root, format!("import module \"link/../file.truxt\"; import module \"{relative}\"; console.log(answer());")).unwrap();
    let graph = session.analyze(&root);
    assert!(graph.hir.is_some(), "{:?}", graph.diagnostics);
    assert_eq!(graph.sources.iter().count(), 2);
    session.remove_overlay(&entry).unwrap();
    assert!(session.analyze(&entry).has_errors());
}

#[test]
fn hir_lint_tracks_binding_ids_captures_methods_and_effectful_expressions() {
    let temp = Temp::new();
    let path = temp.0.join("lint.truxt");
    let mut session = ProjectSession::new();
    session
        .set_overlay(
            &path,
            r#"
        interface Box { value: int }
        impl Box { function get(): int { return this.value; } }
        function f(unused: int, _intentional: int): void {
            const read = 3;
            const unusedLocal = 4;
            const arrow = (x: int): int => x + read;
            console.log(arrow(2));
            const readAgain = 8;
            if true { const inner = 9; console.log(inner); }
            let assigned = 0;
            assigned = 2;
            42;
            console.log("effect");
        }
        function g(): void { const unusedLocal = 5; console.log(unusedLocal); }
        f(1, 2);
    "#
            .to_owned(),
        )
        .unwrap();
    let analysis = session.analyze(&path);
    assert!(!analysis.has_errors(), "{:?}", analysis.diagnostics);
    let warnings = lint(analysis.hir.as_ref().unwrap());
    assert!(warnings.iter().all(|d| d.severity == Severity::Warning));
    let unused: Vec<_> = warnings
        .iter()
        .filter(|d| d.code == "unused-binding")
        .collect();
    assert_eq!(unused.len(), 3, "{warnings:?}");
    for name in ["unused", "unusedLocal", "readAgain"] {
        assert!(unused
            .iter()
            .any(|d| d.message.contains(&format!("`{name}`"))));
    }
    assert_eq!(
        warnings
            .iter()
            .filter(|d| d.code == "literal-condition")
            .count(),
        1
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|d| d.code == "discarded-value")
            .count(),
        1
    );
}
