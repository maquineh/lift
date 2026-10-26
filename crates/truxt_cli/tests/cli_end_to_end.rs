//! CLI acceptance tests for generated programs, bundled core, and runtime I/O.
//!
//! PT-BR: Testes de aceite da CLI para programas gerados, core embutido e I/O.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, UNIX_EPOCH},
};

static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

#[test]
fn lint_warns_without_generating_and_deny_warnings_controls_the_exit_status() {
    let temp = TempDir::new("lint");
    let source = temp.path().join("lint.truxt");
    fs::write(&source, "const unused = 1; if true { 42; }").unwrap();
    let warned = cli().arg("lint").arg(&source).output().unwrap();
    assert_success(&warned, "lint warnings");
    let stderr = String::from_utf8_lossy(&warned.stderr);
    for rule in ["unused-binding", "literal-condition", "discarded-value"] {
        assert!(stderr.contains(rule), "{stderr}");
    }
    assert!(stderr.contains("Warning:"));
    let denied = cli()
        .arg("lint")
        .arg("--deny-warnings")
        .arg(&source)
        .output()
        .unwrap();
    assert_eq!(denied.status.code(), Some(1));
    assert!(!temp.path().join(".truxt_cache").exists());
    fs::write(&source, "const bad: int = true;").unwrap();
    let failed = cli().arg("lint").arg(&source).output().unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("Error:"));
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("Warning:"));
}

#[test]
fn lsp_executable_emits_framed_protocol_without_cli_chatter() {
    let messages = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
        serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
    ];
    let mut input = String::new();
    for message in messages {
        let body = message.to_string();
        input.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    }
    let mut command = cli();
    command.arg("lsp");
    let result = run_with_input(command, &input);
    assert_success(&result, "lsp handshake");
    assert!(result.stderr.is_empty());
    let mut bytes = result.stdout.as_slice();
    let mut count = 0;
    while !bytes.is_empty() {
        let delimiter = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let header = std::str::from_utf8(&bytes[..delimiter]).unwrap();
        let length: usize = header
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();
        let body = &bytes[delimiter + 4..delimiter + 4 + length];
        let value: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");
        assert!(value.get("result").is_some());
        count += 1;
        bytes = &bytes[delimiter + 4 + length..];
    }
    assert_eq!(count, 2);
}

#[test]
fn methods_closures_and_templates_compile_and_run() {
    let temp = TempDir::new("methods-closures-templates");
    let source = temp.path().join("features.truxt");
    fs::write(
        &source,
        include_str!("../../../examples/methods-closures-templates.truxt"),
    )
    .unwrap();
    let checked = cli().arg("check").arg(&source).output().unwrap();
    assert_success(&checked, "check methods/closures/templates");
    let output = cli().arg("run").arg(&source).output().unwrap();
    assert_success(&output, "run methods/closures/templates");
    assert!(String::from_utf8(output.stdout).unwrap().replace("\r\n", "\n").ends_with("3\n4\ncount:1\ncount:4\nhello, Truxt!\nhello, Rust!\nnested value 4 ${literal} `tick`\norder 4/5/5\ncount:5\n10\nborrowed\n12\n10\n11\nhello\n22\n23\n22\n2\ninside\n"));
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("truxt-{label}-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("diretório temporário deve ser criado");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_truxt"))
}

fn forklift() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forklift"))
}

fn write_package_example(directory: &Path) -> PathBuf {
    for (path, text) in [
        (
            "app/truxt.toml",
            include_str!("../../../examples/packages/app/truxt.toml"),
        ),
        (
            "app/main.truxt",
            include_str!("../../../examples/packages/app/main.truxt"),
        ),
        (
            "math/truxt.toml",
            include_str!("../../../examples/packages/math/truxt.toml"),
        ),
        (
            "math/main.truxt",
            include_str!("../../../examples/packages/math/main.truxt"),
        ),
        (
            "math/internal.truxt",
            include_str!("../../../examples/packages/math/internal.truxt"),
        ),
        (
            "base/truxt.toml",
            include_str!("../../../examples/packages/base/truxt.toml"),
        ),
        (
            "base/main.truxt",
            include_str!("../../../examples/packages/base/main.truxt"),
        ),
        (
            "text/truxt.toml",
            include_str!("../../../examples/packages/text/truxt.toml"),
        ),
        (
            "text/main.truxt",
            include_str!("../../../examples/packages/text/main.truxt"),
        ),
    ] {
        let path = directory.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    directory.join("app")
}

#[test]
fn forklift_resolves_checks_builds_and_runs_local_packages_with_qualified_calls() {
    let temp = TempDir::new("forklift");
    let app = write_package_example(temp.path());
    let resolved = forklift().arg("resolve").arg(&app).output().unwrap();
    assert_success(&resolved, "forklift resolve");
    let text = String::from_utf8_lossy(&resolved.stdout);
    for name in ["demo", "math", "base", "text"] {
        assert!(text.contains(name), "{text}");
    }
    assert!(!app.join(".truxt_cache").exists());
    for command in ["check", "build", "run"] {
        let output = forklift().arg(command).arg(&app).output().unwrap();
        assert_success(&output, &format!("forklift {command}"));
        if command == "run" {
            assert!(String::from_utf8_lossy(&output.stdout)
                .replace("\r\n", "\n")
                .ends_with("4\n5\nTruxt!\nTruxt\n5\n"));
        }
    }
    let direct = cli()
        .arg("run")
        .arg(app.join("main.truxt"))
        .output()
        .unwrap();
    assert_success(&direct, "truxt manifested entry");
    assert!(String::from_utf8_lossy(&direct.stdout)
        .replace("\r\n", "\n")
        .ends_with("4\n5\nTruxt!\nTruxt\n5\n"));
    let binary = Command::new(app.join(binary_name("main")))
        .output()
        .unwrap();
    assert_success(&binary, "package binary");
    assert_eq!(
        String::from_utf8_lossy(&binary.stdout).replace("\r\n", "\n"),
        "4\n5\nTruxt!\nTruxt\n5\n"
    );
    let manifest = fs::read_to_string(app.join(".truxt_cache/main/Cargo.toml")).unwrap();
    assert!(manifest.contains("truxt_core"));
    assert!(!manifest.contains("../math"));
    assert!(!app.join("forklift.lock").exists());
}

#[test]
fn forklift_init_is_local_and_refuses_overwrites_and_requires_manifest_for_commands() {
    let temp = TempDir::new("forklift-init");
    let app = temp.path().join("app");
    let created = forklift().arg("init").arg(&app).output().unwrap();
    assert_success(&created, "forklift init");
    assert!(app.join("truxt.toml").exists());
    assert!(app.join("main.truxt").exists());
    let before = fs::read(app.join("truxt.toml")).unwrap();
    assert!(!forklift()
        .arg("init")
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(fs::read(app.join("truxt.toml")).unwrap(), before);
    let library = temp.path().join("library");
    assert_success(
        &forklift()
            .arg("init")
            .arg("--lib")
            .arg(&library)
            .output()
            .unwrap(),
        "forklift init --lib",
    );
    assert_success(
        &forklift().arg("check").arg(&library).output().unwrap(),
        "forklift check lib",
    );
    assert!(!forklift()
        .arg("run")
        .arg(&library)
        .output()
        .unwrap()
        .status
        .success());
    let legacy = temp.path().join("legacy");
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("main.truxt"), "console.log(1);").unwrap();
    assert!(!forklift()
        .arg("resolve")
        .arg(legacy)
        .output()
        .unwrap()
        .status
        .success());
    let spaced = temp.path().join("my project ação");
    assert_success(
        &forklift().arg("init").arg(&spaced).output().unwrap(),
        "forklift init spaced unicode directory",
    );
    let manifest = fs::read_to_string(spaced.join("truxt.toml")).unwrap();
    assert!(manifest.contains("name = \"my-project-a-o\""), "{manifest}");
    assert_success(
        &forklift().arg("check").arg(&spaced).output().unwrap(),
        "forklift check initialized unicode path",
    );
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} falhou\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn run_with_input(mut command: Command, input: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("processo deve iniciar");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("entrada deve ser enviada");
    child.wait_with_output().expect("processo deve terminar")
}

fn write_module_example(directory: &Path) -> PathBuf {
    for (relative, contents) in [
        (
            "main.truxt",
            include_str!("../../../examples/modules/main.truxt"),
        ),
        (
            "models.truxt",
            include_str!("../../../examples/modules/models.truxt"),
        ),
        (
            "math/api.truxt",
            include_str!("../../../examples/modules/math/api.truxt"),
        ),
        (
            "math/helpers.truxt",
            include_str!("../../../examples/modules/math/helpers.truxt"),
        ),
    ] {
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    directory.join("main.truxt")
}

#[test]
fn liveness_moves_compile_with_rustc_and_preserve_control_flow_outputs() {
    let directory = TempDir::new("liveness");
    let source = directory.path().join("liveness.truxt");
    fs::write(&source, include_str!("../../../examples/liveness.truxt")).unwrap();
    let expected = "last\nold\nnew\nrepeat\nrepeat\nyes\nno\nloop\nloop\nloop\nloop\nreturn\nreturn\nreturn\nterminal\n";
    for command in ["check", "build", "run"] {
        let output = cli().arg(command).arg(&source).output().unwrap();
        assert_success(&output, &format!("liveness {command}"));
        if command == "run" {
            assert!(String::from_utf8_lossy(&output.stdout)
                .replace("\r\n", "\n")
                .ends_with(expected));
        }
    }
    let output = Command::new(directory.path().join(binary_name("liveness")))
        .output()
        .unwrap();
    assert_success(&output, "binário liveness");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        expected
    );
}

#[test]
fn option_coalesce_is_lazy_exactly_once_and_preserves_optional_sources() {
    let directory = TempDir::new("option");
    let source = directory.path().join("option.truxt");
    fs::write(&source, include_str!("../../../examples/option.truxt")).unwrap();
    let expected = "7\n1\n9\n12\n5\n112\ntrue\n5\n112\nfalse\n0\n\n112\nfalse\nkeep\nkeep\nkeep\nmissing\n1\n2\n3\nunknown\n42\ntrue\ntrue\n";
    for command in ["check", "build", "run"] {
        let output = cli().arg(command).arg(&source).output().unwrap();
        assert_success(&output, &format!("Option {command}"));
        if command == "run" {
            assert!(
                String::from_utf8_lossy(&output.stdout)
                    .replace("\r\n", "\n")
                    .ends_with(expected),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }
    let output = Command::new(directory.path().join(binary_name("option")))
        .output()
        .unwrap();
    assert_success(&output, "binário Option");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        expected
    );
}

#[test]
fn option_unwrap_none_is_an_explicit_runtime_failure() {
    let directory = TempDir::new("option-unwrap");
    let source = directory.path().join("unwrap.truxt");
    fs::write(
        &source,
        "const value: int? = null; console.log(option.unwrap(value));",
    )
    .unwrap();
    let output = cli().arg("run").arg(source).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("option.unwrap called on None"));
}

#[test]
fn modules_resolve_relative_transitive_repeated_and_cyclic_imports_once() {
    let directory = TempDir::new("modules");
    let project = directory.path().join("projeto á");
    let root = write_module_example(&project);
    let unrelated_cwd = directory.path().join("other-cwd");
    fs::create_dir_all(&unrelated_cwd).unwrap();
    let expected = "Truxt\n60\nroot\nroot executed once\n";
    for command in ["check", "build", "run"] {
        let output = cli()
            .current_dir(&unrelated_cwd)
            .arg(command)
            .arg(&root)
            .output()
            .unwrap();
        assert_success(&output, &format!("módulos: {command}"));
        if command == "run" {
            assert!(String::from_utf8_lossy(&output.stdout)
                .replace("\r\n", "\n")
                .ends_with(expected));
        }
    }
    let binary = Command::new(project.join(binary_name("main")))
        .output()
        .unwrap();
    assert_success(&binary, "binário multi-arquivo");
    assert_eq!(
        String::from_utf8(binary.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        expected
    );

    let helper = project.join("math/helpers.truxt");
    let changed = fs::read_to_string(&helper)
        .unwrap()
        .replace("left * right", "left + right");
    fs::write(helper, changed).unwrap();
    let output = cli().arg("run").arg(root).output().unwrap();
    assert_success(&output, "mudança transitiva de módulo");
    assert!(String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .ends_with("Truxt\n23\nroot\nroot executed once\n"));
}

#[test]
fn imported_modules_reject_every_top_level_statement_kind() {
    let directory = TempDir::new("module-statements");
    let root = directory.path().join("main.truxt");
    let module = directory.path().join("module.truxt");
    fs::write(&root, "import module \"module.truxt\"; console.log(1);").unwrap();
    for contents in [
        "const value = 1;",
        "let value = 1;",
        "value = 1;",
        "console.log(1);",
        "if true { console.log(1); }",
        "while false { console.log(1); }",
        "return;",
    ] {
        fs::write(&module, contents).unwrap();
        let output = cli().arg("check").arg(&root).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{contents}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("module.truxt:1:"), "{stderr}");
        assert!(stderr.contains("statement executável de topo"), "{stderr}");
    }
    fs::write(&module, "").unwrap();
    assert_success(
        &cli().arg("check").arg(root).output().unwrap(),
        "módulo vazio",
    );
}

#[test]
fn imported_source_diagnostics_keep_their_file_and_utf8_byte_positions() {
    let directory = TempDir::new("module-diagnostics");
    let root = directory.path().join("main.truxt");
    let module = directory.path().join("dep.truxt");
    fs::write(&root, "import module \"dep.truxt\"; console.log(1);").unwrap();
    for (contents, marker) in [
        (
            "function invalid(): void { const text = \"á🙂\"; @ }",
            Some("@"),
        ),
        (
            "function invalid(): void { const text = \"á🙂\"; const broken: int = ; }",
            Some(";"),
        ),
        (
            "function invalid(): int { const text = \"á🙂\"; return true; }",
            Some("true"),
        ),
        ("function invalid(): void { const text = \"á🙂\";", None),
    ] {
        fs::write(&module, contents).unwrap();
        let output = cli().arg("check").arg(&root).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let offset = marker.map_or(contents.len(), |marker| contents.rfind(marker).unwrap());
        let column = contents[..offset].chars().count() + 1;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("dep.truxt:1:{column}")),
            "{stderr}"
        );
        assert!(stderr.contains(contents), "{stderr}");
        assert!(!stderr.contains("main.truxt:"), "{stderr}");
    }
}

#[test]
fn duplicate_global_declarations_label_both_imported_files() {
    let directory = TempDir::new("module-duplicates");
    let root = directory.path().join("main.truxt");
    fs::write(
        &root,
        "import module \"first.truxt\"; import module \"second.truxt\";",
    )
    .unwrap();
    fs::write(
        directory.path().join("first.truxt"),
        "// á🙂\nfunction shared(): int { return 1; }",
    )
    .unwrap();
    fs::write(
        directory.path().join("second.truxt"),
        "// ação🙂\nfunction shared(): int { return 2; }",
    )
    .unwrap();
    let output = cli().arg("check").arg(root).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("Error:").count(), 1, "{stderr}");
    assert!(stderr.contains("first.truxt:2:"), "{stderr}");
    assert!(stderr.contains("second.truxt:2:"), "{stderr}");
    assert!(stderr.contains("local relacionado"), "{stderr}");
    assert!(stderr.contains("return 1;"), "{stderr}");
    assert!(stderr.contains("return 2;"), "{stderr}");
}

#[test]
fn invalid_or_unreadable_module_paths_label_the_importing_source() {
    let directory = TempDir::new("module-paths");
    let root = directory.path().join("main.truxt");
    let absolute = directory
        .path()
        .join("absolute.truxt")
        .to_string_lossy()
        .replace('\\', "/");
    for path in [
        "".to_owned(),
        "dep.txt".to_owned(),
        absolute,
        "missing.truxt".to_owned(),
    ] {
        fs::write(&root, format!("import module \"{path}\";")).unwrap();
        let output = cli().arg("check").arg(&root).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("main.truxt:1:"), "{stderr}");
    }
    fs::write(&root, "import module \"invalid.truxt\";").unwrap();
    fs::write(directory.path().join("invalid.truxt"), [0xff]).unwrap();
    let output = cli().arg("check").arg(root).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("main.truxt:1:"), "{stderr}");
    assert!(stderr.contains("fonte UTF-8"), "{stderr}");
}

#[test]
fn copied_cli_uses_bundled_core_after_relocation() {
    let directory = TempDir::new("relocation");
    let original_install = directory.path().join("original-install");
    fs::create_dir_all(&original_install).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_truxt"),
        original_install.join(binary_name("truxt")),
    )
    .unwrap();
    let moved_install = directory.path().join("moved-install");
    fs::rename(&original_install, &moved_install).unwrap();
    let executable = moved_install.join(binary_name("truxt"));
    let work = directory.path().join("independent-project");
    fs::create_dir_all(&work).unwrap();
    let source = work.join("stdlib.truxt");
    fs::write(&source, include_str!("../../../examples/stdlib.truxt")).unwrap();

    for command in ["check", "build", "run"] {
        let output = Command::new(&executable)
            .current_dir(&work)
            .arg(command)
            .arg(&source)
            .output()
            .unwrap();
        assert_success(&output, &format!("CLI relocada: {command}"));
        if command == "run" {
            assert!(String::from_utf8_lossy(&output.stdout)
                .replace("\r\n", "\n")
                .ends_with("12\n3\n2\ntrue\nfalse\ntrue\ntrue\ntrue\n2\nTruxt\n2\nação\n"));
        }
    }

    let project = work.join(".truxt_cache/stdlib");
    let core_manifest = project.join("truxt_core/Cargo.toml");
    let manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    assert!(!manifest.replace('\\', "/").contains(&checkout));
    let metadata = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .output()
        .unwrap();
    assert_success(&metadata, "cargo metadata do projeto relocável");
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let core = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "truxt_core")
        .expect("core embutido deve integrar a resolução Cargo");
    assert_eq!(
        fs::canonicalize(core["manifest_path"].as_str().unwrap()).unwrap(),
        fs::canonicalize(core_manifest).unwrap()
    );
    assert!(project.join("truxt_core/src/interop.rs").is_file());
    assert!(project.join("truxt_core/src/io.rs").is_file());
}

#[test]
fn io_example_receives_arguments_stdin_and_round_trips_utf8_files() {
    let directory = TempDir::new("io");
    let source_dir = directory.path().join("sources");
    fs::create_dir_all(&source_dir).unwrap();
    let source = source_dir.join("io.truxt");
    fs::write(&source, include_str!("../../../examples/io.truxt")).unwrap();
    for command in ["check", "build"] {
        let output = cli().arg(command).arg(&source).output().unwrap();
        assert_success(&output, command);
    }

    let file = directory.path().join("ação.txt");
    let mut binary = Command::new(source_dir.join(binary_name("io")));
    binary
        .current_dir(directory.path())
        .arg("ação.txt")
        .arg("ação🙂");
    let output = run_with_input(binary, "\t Olá\nTruxt \n");
    assert_success(&output, "binário de I/O");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "2\nOlá\nTruxt|ação🙂\n"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "Olá\nTruxt|ação🙂");

    let mut run = cli();
    run.current_dir(directory.path())
        .arg("run")
        .arg(&source)
        .arg("--")
        .arg(&file)
        .arg("second");
    let output = run_with_input(run, " second input \n");
    assert_success(&output, "truxt run com argumentos e stdin");
    assert!(String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .ends_with("2\nsecond input|second\n"));
    assert_eq!(fs::read_to_string(file).unwrap(), "second input|second");

    let mut empty = Command::new(source_dir.join(binary_name("io")));
    empty
        .current_dir(directory.path())
        .arg("empty.txt")
        .arg("empty");
    let output = run_with_input(empty, "");
    assert_success(&output, "stdin vazio");
    assert_eq!(
        fs::read_to_string(directory.path().join("empty.txt")).unwrap(),
        "|empty"
    );
    assert!(!source_dir.join("ação.txt").exists());
}

#[test]
fn run_preserves_flag_and_spaced_arguments_without_the_executable_name() {
    let directory = TempDir::new("arguments");
    let source = directory.path().join("arguments.truxt");
    fs::write(
        &source,
        r#"
        import function arguments(): Result<string[], string> from "truxt_core::io::arguments";
        import function length(values: string[]): int from "truxt_core::interop::array_len";
        const args: string[] = result.unwrap(arguments());
        console.log(length(args));
        console.log(args[0]);
        console.log(args[1]);
    "#,
    )
    .unwrap();
    let output = cli()
        .arg("run")
        .arg(source)
        .args(["--", "--flag", "a b"])
        .output()
        .unwrap();
    assert_success(&output, "argumentos preservados");
    assert!(String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .ends_with("2\n--flag\na b\n"));
}

#[test]
fn file_read_errors_are_values_that_truxt_can_handle() {
    let directory = TempDir::new("io-failures");
    let source = directory.path().join("read.truxt");
    fs::write(
        &source,
        r#"
        import function arguments(): Result<string[], string> from "truxt_core::io::arguments";
        import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";
        function forward(value: Result<string, string>): Result<string, string> { return value; }
        function report(value: Result<string, string>): void {
            if result.is_ok(value) {
                console.log(result.unwrap(value));
            } else {
                console.log(result.unwrap_err(value));
                console.log(result.is_ok(value));
            }
        }
        const args: string[] = result.unwrap(arguments());
        report(forward(read_file(args[0])));
    "#,
    )
    .unwrap();
    let missing = directory.path().join("missing.txt");
    let output = cli()
        .arg("run")
        .arg(&source)
        .arg("--")
        .arg(&missing)
        .output()
        .unwrap();
    assert_success(&output, "erros de arquivo são tratáveis");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("read_file"));
    assert!(stdout.contains("missing.txt"));
    assert!(stdout.replace("\r\n", "\n").ends_with("false\n"));

    let invalid = directory.path().join("invalid.txt");
    fs::write(&invalid, [0xff]).unwrap();
    let output = cli()
        .arg("run")
        .arg(source)
        .arg("--")
        .arg(invalid)
        .output()
        .unwrap();
    assert_success(&output, "UTF-8 inválido é tratável");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("read_file"));
    assert!(stdout.contains("invalid.txt"));
}

#[test]
fn unwrap_on_the_wrong_result_variant_is_an_explicit_runtime_failure() {
    let directory = TempDir::new("unwrap");
    let source = directory.path().join("unwrap.truxt");
    fs::write(&source, r#"
        import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";
        console.log(result.unwrap(read_file("missing-result-file.txt")));
    "#).unwrap();
    let output = cli()
        .current_dir(directory.path())
        .arg("run")
        .arg(source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("result.unwrap called on Err"));
}

#[test]
fn check_reports_a_missing_cargo_toolchain() {
    let directory = TempDir::new("no-cargo");
    let source = directory.path().join("empty.truxt");
    fs::write(&source, "const value: int = 1;").unwrap();
    let output = cli()
        .current_dir(directory.path())
        .env("PATH", "")
        .arg("check")
        .arg(source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cargo check"));
}

#[test]
#[ignore = "downloads a pinned registry crate; run explicitly for release acceptance"]
fn registry_import_consumes_a_real_pinned_rust_crate() {
    let directory = TempDir::new("registry");
    let source = directory.path().join("registry.truxt");
    fs::write(&source, include_str!("../../../examples/registry.truxt")).unwrap();
    for command in ["check", "build", "run"] {
        let output = cli().arg(command).arg(&source).output().unwrap();
        assert_success(&output, &format!("registry: {command}"));
        if command == "run" {
            assert!(String::from_utf8_lossy(&output.stdout)
                .replace("\r\n", "\n")
                .ends_with("6\n"));
        }
    }
    let manifest =
        fs::read_to_string(directory.path().join(".truxt_cache/registry/Cargo.toml")).unwrap();
    assert!(manifest.contains("num-integer"));
    assert!(manifest.contains("=0.1.46"));
}

#[test]
fn interop_check_build_run_prints_through_local_core_and_keeps_borrowed_values() {
    let directory = TempDir::new("interop");
    let source = directory.path().join("interop.truxt");
    fs::write(&source, include_str!("../../../examples/interop.truxt")).unwrap();
    for command in ["check", "build"] {
        let output = cli().arg(command).arg(&source).output().unwrap();
        assert_success(&output, command);
    }
    let binary = directory.path().join(if cfg!(windows) {
        "interop.exe"
    } else {
        "interop"
    });
    let output = Command::new(binary).output().unwrap();
    assert_success(&output, "binário interop");
    let expected = "Olá, Truxt\nTruxt\n42\n2.5\ntrue\nfalse\nfalse\n1\nTruxt\nTruxt\n";
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        expected
    );
    let run = cli().arg("run").arg(&source).output().unwrap();
    assert_success(&run, "truxt run interop");
    assert!(String::from_utf8(run.stdout)
        .unwrap()
        .replace("\r\n", "\n")
        .ends_with(expected));
    let manifest =
        fs::read_to_string(directory.path().join(".truxt_cache/interop/Cargo.toml")).unwrap();
    assert!(manifest.contains("truxt_core = { path = "));
}

#[test]
fn rustc_rejects_an_import_whose_declared_signature_does_not_match() {
    let directory = TempDir::new("ffi-signature");
    let source = directory.path().join("bad_signature.truxt");
    fs::write(
        &source,
        r#"import function concat(left: int, right: int): int from "truxt_core::string::concat";"#,
    )
    .unwrap();
    let output = cli().arg("check").arg(&source).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mismatched types"));
}

#[test]
fn rustc_rejects_private_and_missing_import_targets() {
    let directory = TempDir::new("ffi-targets");
    for (file_name, source, diagnostic) in [
        (
            "private.truxt",
            r#"import function hidden(value: int): int from "truxt_core::console::write_line";"#,
            "private",
        ),
        (
            "missing.truxt",
            r#"import function missing(value: int): int from "truxt_core::string::no_such_function";"#,
            "no_such_function",
        ),
    ] {
        let source_path = directory.path().join(file_name);
        fs::write(&source_path, source).expect("fonte FFI inválida deve ser gravada");
        let output = cli()
            .arg("check")
            .arg(source_path)
            .output()
            .expect("CLI check deve iniciar");
        assert!(
            !output.status.success(),
            "rustc deve rejeitar o destino {file_name}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "diagnóstico deve mencionar `{diagnostic}`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn check_build_and_run_compile_and_execute_a_truxt_example() {
    let directory = TempDir::new("e2e");
    let source_path = directory.path().join("fibonacci.truxt");
    fs::write(
        &source_path,
        include_str!("../../../examples/fibonacci.truxt"),
    )
    .expect("exemplo deve ser copiado para o diretório temporário");

    let check = cli()
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("CLI check deve iniciar");
    assert_success(&check, "truxt check");

    let build = cli()
        .arg("build")
        .arg(&source_path)
        .output()
        .expect("CLI build deve iniciar");
    assert_success(&build, "truxt build");
    let binary_path = if cfg!(windows) {
        directory.path().join("fibonacci.exe")
    } else {
        directory.path().join("fibonacci")
    };
    assert!(
        binary_path.is_file(),
        "build deve copiar o binário ao lado da fonte"
    );

    let run = cli()
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("CLI run deve iniciar");
    assert_success(&run, "truxt run");
}

#[test]
fn check_build_and_run_compile_and_execute_the_ownership_example() {
    let directory = TempDir::new("ownership");
    let source_path = directory.path().join("ownership.truxt");
    fs::write(
        &source_path,
        include_str!("../../../examples/ownership.truxt"),
    )
    .expect("exemplo deve ser copiado para o diretório temporário");

    let check = cli()
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("CLI check deve iniciar");
    assert_success(&check, "truxt check ownership");

    let build = cli()
        .arg("build")
        .arg(&source_path)
        .output()
        .expect("CLI build deve iniciar");
    assert_success(&build, "truxt build ownership");
    let binary_path = if cfg!(windows) {
        directory.path().join("ownership.exe")
    } else {
        directory.path().join("ownership")
    };
    assert!(
        binary_path.is_file(),
        "build deve copiar o binário ao lado da fonte"
    );
    let binary = Command::new(&binary_path)
        .output()
        .expect("binário de ownership deve iniciar");
    assert_success(&binary, "binário de ownership");

    let run = cli()
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("CLI run deve iniciar");
    assert_success(&run, "truxt run ownership");
}

#[test]
fn init_creates_a_starter_file_and_refuses_to_overwrite_it() {
    let directory = TempDir::new("init");
    let init = cli()
        .arg("init")
        .arg(directory.path())
        .output()
        .expect("CLI init deve iniciar");
    assert_success(&init, "truxt init");

    let source = directory.path().join("main.truxt");
    let original = fs::read_to_string(&source).expect("init deve criar main.truxt");
    assert!(original.contains("Programa Truxt inicial"));

    let repeated = cli()
        .arg("init")
        .arg(directory.path())
        .output()
        .expect("CLI init deve iniciar novamente");
    assert!(
        !repeated.status.success(),
        "init não deve sobrescrever arquivo existente"
    );
    assert_eq!(
        fs::read_to_string(source).expect("arquivo deve continuar presente"),
        original
    );
}

#[test]
fn check_reports_source_errors_with_location() {
    let directory = TempDir::new("diagnostic");
    let source = directory.path().join("invalid.truxt");
    fs::write(&source, "const value: int = true;").expect("fonte inválida deve ser gravada");

    let output = cli()
        .arg("check")
        .arg(source)
        .output()
        .expect("CLI check deve iniciar");
    assert!(
        !output.status.success(),
        "programa semanticamente inválido deve falhar"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid.truxt:1:"),
        "diagnóstico deve apontar arquivo e linha: {stderr}"
    );
    assert!(
        stderr.contains("Error:"),
        "diagnóstico deve identificar o erro: {stderr}"
    );
}

#[test]
fn ariadne_reports_lexical_syntax_semantic_and_empty_eof_spans_with_utf8() {
    let directory = TempDir::new("ariadne");
    let cases = [
        ("lexical.truxt", "const texto = \"é🙂\"; @", Some("@")),
        (
            "syntax.truxt",
            "const texto = \"é🙂\"; const value: int = ;",
            Some(";"),
        ),
        (
            "semantic.truxt",
            "const texto = \"é🙂\"; const value: int = true;",
            Some("true"),
        ),
        ("eof.truxt", "function broken(): void {", None),
    ];
    for (file_name, contents, marker) in cases {
        let source = directory.path().join(file_name);
        fs::write(&source, contents).unwrap();
        let output = cli().arg("check").arg(source).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{file_name}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        let offset = marker.map_or(contents.len(), |marker| contents.rfind(marker).unwrap());
        let column = contents[..offset].chars().count() + 1;
        assert!(
            stderr.contains(&format!("{file_name}:1:{column}")),
            "{stderr}"
        );
        assert!(stderr.contains("Error:"), "{stderr}");
        assert!(stderr.contains(contents), "{stderr}");
        assert!(
            !stderr.contains('\u{1b}'),
            "saída capturada deve omitir cores: {stderr}"
        );
    }
}

#[test]
fn ariadne_keeps_related_semantic_labels_in_one_report() {
    let directory = TempDir::new("ariadne-secondary");
    let source = directory.path().join("duplicate.truxt");
    fs::write(
        &source,
        "function item(value: string): void {}\nfunction item(value: int): void {}",
    )
    .unwrap();
    let output = cli().arg("check").arg(source).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.matches("Error:").count(), 1, "{stderr}");
    assert!(stderr.contains("local relacionado"), "{stderr}");
    assert!(
        stderr.contains("function item(value: string): void {}"),
        "{stderr}"
    );
    assert!(
        stderr.contains("function item(value: int): void {}"),
        "{stderr}"
    );
}

#[test]
fn generated_files_keep_timestamps_until_their_content_changes() {
    let directory = TempDir::new("rewrite");
    let source = directory.path().join("rewrite.truxt");
    fs::write(&source, "const value: int = 1;").unwrap();
    let check = || {
        let output = cli().arg("check").arg(&source).output().unwrap();
        assert_success(&output, "verificação de reescrita");
    };
    check();
    let generated = directory.path().join(".truxt_cache/rewrite/src/main.rs");
    let manifest = directory.path().join(".truxt_cache/rewrite/Cargo.toml");
    let original_rust = fs::read(&generated).unwrap();
    let original_manifest = fs::read(&manifest).unwrap();
    let old = UNIX_EPOCH + Duration::from_secs(946_684_800);
    for path in [&generated, &manifest] {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    // EN: Set a fixed historical timestamp; no sleeps or clock-resolution assumptions.
    // PT-BR: Timestamp histórico fixo; sem sleeps ou dependência da resolução do relógio.
    let retained_rust_time = fs::metadata(&generated).unwrap().modified().unwrap();
    let retained_manifest_time = fs::metadata(&manifest).unwrap().modified().unwrap();
    check();
    assert_eq!(
        fs::metadata(&generated).unwrap().modified().unwrap(),
        retained_rust_time
    );
    assert_eq!(
        fs::metadata(&manifest).unwrap().modified().unwrap(),
        retained_manifest_time
    );
    assert_eq!(fs::read(&generated).unwrap(), original_rust);
    assert_eq!(fs::read(&manifest).unwrap(), original_manifest);

    fs::write(&source, "const value: int = 2;").unwrap();
    check();
    assert_ne!(fs::read(&generated).unwrap(), original_rust);
    assert_ne!(
        fs::metadata(&generated).unwrap().modified().unwrap(),
        retained_rust_time
    );
    assert_eq!(
        fs::metadata(&manifest).unwrap().modified().unwrap(),
        retained_manifest_time
    );

    fs::write(&source, "console.log(2);").unwrap();
    check();
    assert_ne!(fs::read(&manifest).unwrap(), original_manifest);
    assert_ne!(
        fs::metadata(&manifest).unwrap().modified().unwrap(),
        retained_manifest_time
    );
}
