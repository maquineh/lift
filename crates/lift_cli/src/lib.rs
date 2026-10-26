//! CLI orchestration for Lift analysis, bundled core extraction, and Cargo.
//!
//! The executable embeds its standard-library sources and materializes them in
//! each generated project, so compilation does not depend on the source checkout.
//! Rust/Cargo and a host linker remain external prerequisites.
//!
//! PT-BR: Orquestra análise Lift, extração do core embutido e Cargo. O executável
//! incorpora as fontes da stdlib e as materializa no projeto gerado, sem depender
//! do checkout original. Rust/Cargo e um linker do host continuam pré-requisitos.

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
};

use clap::{Parser, Subcommand};
use lift_codegen::{emit_cargo_manifest_for_program, emit_rust};
use lift_project::{ProjectSession, Severity};
use lift_sema::{HirDeclaration, HirProgram, RustDependency};
use serde_json::Value;

mod diagnostics;
mod lsp;
use diagnostics::report_diagnostic;

const EMBEDDED_CORE_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", EMBEDDED_CORE_CARGO_TOML),
    (
        "src/lib.rs",
        include_str!("../../../stdlib/lift_core/src/lib.rs"),
    ),
    (
        "src/array.rs",
        include_str!("../../../stdlib/lift_core/src/array.rs"),
    ),
    (
        "src/console.rs",
        include_str!("../../../stdlib/lift_core/src/console.rs"),
    ),
    (
        "src/string.rs",
        include_str!("../../../stdlib/lift_core/src/string.rs"),
    ),
    (
        "src/interop.rs",
        include_str!("../../../stdlib/lift_core/src/interop.rs"),
    ),
    (
        "src/io.rs",
        include_str!("../../../stdlib/lift_core/src/io.rs"),
    ),
];

const EMBEDDED_CORE_CARGO_TOML: &str = concat!(
    "[package]\nname = \"lift_core\"\nversion = \"",
    env!("CARGO_PKG_VERSION"),
    "\"\nedition = \"2021\"\nrust-version = \"1.94.1\"\ndescription = \"Lift base library: console, arrays, strings, and UTF-8 I/O\"\n"
);

const EMBEDDED_DB_FILES: &[(&str, &str)] = &[
    (
        "src/api.rs",
        include_str!("../../../stdlib/lift_db/src/api.rs"),
    ),
    (
        "src/value.rs",
        include_str!("../../../stdlib/lift_db/src/value.rs"),
    ),
    (
        "src/sql.rs",
        include_str!("../../../stdlib/lift_db/src/sql.rs"),
    ),
    (
        "src/resource.rs",
        include_str!("../../../stdlib/lift_db/src/resource.rs"),
    ),
    (
        "Cargo.toml",
        include_str!("../../../stdlib/lift_db/Cargo.toml"),
    ),
    (
        "src/lib.rs",
        include_str!("../../../stdlib/lift_db/src/lib.rs"),
    ),
];

const EMBEDDED_WEB_FILES: &[(&str, &str)] = &[
    (
        "src/http.rs",
        include_str!("../../../stdlib/lift_web/src/http.rs"),
    ),
    (
        "Cargo.toml",
        include_str!("../../../stdlib/lift_web/Cargo.toml"),
    ),
    (
        "src/lib.rs",
        include_str!("../../../stdlib/lift_web/src/lib.rs"),
    ),
    (
        "src/notes.rs",
        include_str!("../../../stdlib/lift_web/src/notes.rs"),
    ),
];

#[derive(Parser)]
#[command(name = "lift", version, about = "Compilador da linguagem Lift")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Gera e compila um programa Lift em modo release.
    Build {
        /// Arquivo fonte `.lift`.
        input: PathBuf,
    },
    /// Compila e executa um arquivo Lift.
    Run {
        /// Arquivo fonte `.lift`.
        input: PathBuf,
        /// Argumentos enviados ao programa (após `--`).
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    /// Gera Rust e valida-o com `cargo check`.
    Check {
        /// Arquivo fonte `.lift`.
        input: PathBuf,
    },
    /// Analisa fontes e emite avisos HIR sem gerar Rust ou chamar Cargo.
    Lint {
        input: PathBuf,
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Inicia o servidor de linguagem via JSON-RPC em stdio.
    Lsp,
    /// Cria um diretório com um arquivo `main.lift` inicial.
    Init {
        /// Diretório destino (padrão: diretório atual).
        #[arg(default_value = ".")]
        directory: PathBuf,
    },
}

#[derive(Parser)]
#[command(
    name = "forklift",
    version,
    about = "Pacotes Lift locais por path (sem registry)"
)]
struct ForkliftCli {
    #[command(subcommand)]
    command: ForkliftCommands,
}

#[derive(Subcommand)]
enum ForkliftCommands {
    /// Cria lift.toml e main.lift sem sobrescrever arquivos.
    Init {
        #[arg(default_value = ".")]
        directory: PathBuf,
        #[arg(long)]
        lib: bool,
    },
    /// Valida/carrega o grafo de pacotes locais sem chamar Cargo.
    Resolve {
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Analisa o projeto e valida o Rust gerado com Cargo.
    Check {
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Compila um projeto bin em release.
    Build {
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Compila e executa o projeto bin.
    Run {
        #[arg(default_value = ".")]
        project: PathBuf,
        #[arg(last = true)]
        args: Vec<OsString>,
    },
}

/// Execute the local-only Forklift package CLI using the same analysis/build API.
/// PT-BR: Executa Forklift local com as mesmas APIs de análise/build.
pub fn execute_forklift() -> u8 {
    match dispatch_forklift(ForkliftCli::parse().command) {
        Ok(code) => code,
        Err(CliError::Message(message)) => {
            eprintln!("forklift: {message}");
            1
        }
        Err(CliError::DiagnosticsReported) => 1,
    }
}

fn dispatch_forklift(command: ForkliftCommands) -> Result<u8, CliError> {
    if let ForkliftCommands::Init { directory, lib } = command {
        init_package(&directory, lib)?;
        return Ok(0);
    }
    let project = match &command {
        ForkliftCommands::Resolve { project }
        | ForkliftCommands::Check { project }
        | ForkliftCommands::Build { project }
        | ForkliftCommands::Run { project, .. } => project,
        ForkliftCommands::Init { .. } => unreachable!(),
    };
    let resolved = ProjectSession::new().resolve(project);
    for diagnostic in &resolved.diagnostics {
        report_diagnostic(&resolved.sources, diagnostic);
    }
    let ast = resolved
        .project
        .as_ref()
        .ok_or(CliError::DiagnosticsReported)?;
    let root_is_library = resolved
        .packages
        .iter()
        .find(|p| p.id == ast.root)
        .ok_or_else(|| CliError::message("pacote root não resolvido"))?
        .is_library;
    match command {
        ForkliftCommands::Resolve { .. } => {
            let mut packages: Vec<_> = resolved.packages.iter().collect();
            packages.sort_by_key(|p| p.id);
            for package in packages {
                println!(
                    "{} {} ({}) [{}] {}",
                    package.name,
                    package.version,
                    if package.is_library { "lib" } else { "bin" },
                    package.id.0,
                    package.manifest.display()
                );
                for (alias, target) in &package.dependencies {
                    println!("  {alias} -> [{}]", target.0);
                }
            }
            Ok(0)
        }
        ForkliftCommands::Check { .. } => cargo_check(&generate_analysis(resolved.analyze())?),
        ForkliftCommands::Build { .. } if !root_is_library => {
            build_generated(generate_analysis(resolved.analyze())?)
        }
        ForkliftCommands::Run { args, .. } if !root_is_library => {
            run_generated(generate_analysis(resolved.analyze())?, args)
        }
        ForkliftCommands::Build { .. } | ForkliftCommands::Run { .. } => Err(CliError::message(
            "build/run exigem package.kind = bin; use check para biblioteca",
        )),
        ForkliftCommands::Init { .. } => unreachable!(),
    }
}

fn init_package(directory: &Path, library: bool) -> Result<(), CliError> {
    fs::create_dir_all(directory).map_err(|e| CliError::message(e.to_string()))?;
    let manifest = directory.join("lift.toml");
    let source = directory.join("main.lift");
    if manifest.exists() || source.exists() {
        return Err(CliError::message(
            "lift.toml ou main.lift já existe; nenhum arquivo foi sobrescrito",
        ));
    }
    let absolute =
        lift_project::source_path(directory).map_err(|e| CliError::message(e.to_string()))?;
    let name = absolute
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("lift-package");
    let name = initial_package_name(name);
    let name = serde_json::to_string(&name).map_err(|e| CliError::message(e.to_string()))?;
    let text=format!("[package]\nname = {name}\nversion = \"0.1.0\"\nkind = \"{}\"\nentry = \"main.lift\"\n\n[dependencies]\n",if library {"lib"}else{"bin"});
    publish_package_files(
        directory,
        text.as_bytes(),
        if library {
            b"export function greeting(): string { return \"Hello, Lift!\"; }\n"
        } else {
            b"console.log(\"Hello, Lift!\");\n"
        },
    )
    .map_err(|e| CliError::message(e.to_string()))?;
    println!("Pacote criado: {}", manifest.display());
    Ok(())
}

fn initial_package_name(directory_name: &str) -> String {
    let filtered: String = directory_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let mut name = filtered
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if name.is_empty() {
        return "lift-package".to_owned();
    }
    if !name.as_bytes()[0].is_ascii_alphabetic() {
        name = format!("lift-{name}");
    }
    name
}

fn publish_package_files(directory: &Path, manifest: &[u8], source: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_INIT: AtomicU64 = AtomicU64::new(0);
    let path = directory.join(format!(
        ".forklift-init-{}-{}",
        std::process::id(),
        NEXT_INIT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path)?;
    let mut stage = PackageStage {
        path,
        published: Vec::new(),
    };
    fs::write(stage.path.join("lift.toml"), manifest)?;
    fs::write(stage.path.join("main.lift"), source)?;
    for name in ["lift.toml", "main.lift"] {
        let destination = directory.join(name);
        // Same-filesystem hard links publish complete bytes without replacing an
        // existing target. A second publication failure rolls the first back.
        // PT-BR: Hard links publicam bytes completos sem sobrescrever; falhar a
        // segunda publicação desfaz a primeira, preservando arquivos anteriores.
        fs::hard_link(stage.path.join(name), &destination)?;
        stage.published.push(destination);
    }
    stage.published.clear();
    Ok(())
}
struct PackageStage {
    path: PathBuf,
    published: Vec<PathBuf>,
}
impl Drop for PackageStage {
    fn drop(&mut self) {
        for path in &self.published {
            let _ = fs::remove_file(path);
        }
        for name in ["lift.toml", "main.lift"] {
            let _ = fs::remove_file(self.path.join(name));
        }
        let _ = fs::remove_dir(&self.path);
    }
}

struct GeneratedProject {
    source_path: PathBuf,
    manifest_path: PathBuf,
    cache_dir: PathBuf,
}

struct CargoBuild {
    exit_code: u8,
    executable: Option<PathBuf>,
}

enum CliError {
    Message(String),
    DiagnosticsReported,
}

impl CliError {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

/// Executa a CLI e devolve o código que o processo deve retornar.
pub fn execute() -> u8 {
    let cli = Cli::parse();
    match dispatch(cli.command) {
        Ok(code) => code,
        Err(CliError::Message(message)) => {
            eprintln!("lift: {message}");
            1
        }
        Err(CliError::DiagnosticsReported) => 1,
    }
}

fn dispatch(command: Commands) -> Result<u8, CliError> {
    match command {
        Commands::Build { input } => build_generated(generate_project(&input)?),
        Commands::Run { input, args } => run_generated(generate_project(&input)?, args),
        Commands::Check { input } => {
            let project = generate_project(&input)?;
            cargo_check(&project)
        }
        Commands::Lint {
            input,
            deny_warnings,
        } => {
            let mut analysis = ProjectSession::new().analyze(&input);
            if let Some(hir) = &analysis.hir {
                analysis.diagnostics.extend(lift_project::lint::lint(hir));
            }
            for diagnostic in &analysis.diagnostics {
                report_diagnostic(&analysis.sources, diagnostic);
            }
            Ok(u8::from(
                analysis.has_errors()
                    || (deny_warnings
                        && analysis
                            .diagnostics
                            .iter()
                            .any(|d| d.severity == Severity::Warning)),
            ))
        }
        Commands::Lsp => lsp::serve().map_err(|error| CliError::message(error.to_string())),
        Commands::Init { directory } => {
            init_project(&directory)?;
            Ok(0)
        }
    }
}

fn build_generated(project: GeneratedProject) -> Result<u8, CliError> {
    println!("Compilando {}...", project.source_path.display());
    let build = cargo_build(&project)?;
    if build.exit_code != 0 {
        return Ok(build.exit_code);
    }
    let binary = build
        .executable
        .ok_or_else(|| CliError::message("Cargo não informou o binário gerado"))?;
    let output = output_binary_path(&project.source_path, &binary);
    fs::copy(&binary, &output).map_err(|error| {
        CliError::message(format!(
            "não foi possível copiar `{}` para `{}`: {error}",
            binary.display(),
            output.display()
        ))
    })?;
    println!("Binário gerado: {}", output.display());
    Ok(0)
}
fn run_generated(project: GeneratedProject, args: Vec<OsString>) -> Result<u8, CliError> {
    println!("Compilando {}...", project.source_path.display());
    let build = cargo_build(&project)?;
    if build.exit_code != 0 {
        return Ok(build.exit_code);
    }
    let executable = build
        .executable
        .ok_or_else(|| CliError::message("Cargo não informou o binário gerado"))?;
    let status = ProcessCommand::new(executable)
        .args(args)
        .status()
        .map_err(|error| {
            CliError::message(format!("não foi possível executar o binário: {error}"))
        })?;
    Ok(exit_code(status))
}

fn generate_project(input: &Path) -> Result<GeneratedProject, CliError> {
    generate_analysis(ProjectSession::new().analyze(input))
}
fn generate_analysis(analysis: lift_project::Analysis) -> Result<GeneratedProject, CliError> {
    for diagnostic in &analysis.diagnostics {
        report_diagnostic(&analysis.sources, diagnostic);
    }
    let hir = analysis.hir.as_ref().ok_or(CliError::DiagnosticsReported)?;
    write_generated_project(&analysis.entry, hir)
}

fn write_generated_project(
    source_path: &Path,
    hir: &HirProgram,
) -> Result<GeneratedProject, CliError> {
    let parent = source_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("lift-program");
    let cache_dir = parent.join(".lift_cache").join(stem);
    let source_dir = cache_dir.join("src");
    fs::create_dir_all(&source_dir).map_err(|error| {
        CliError::message(format!(
            "não foi possível criar `{}`: {error}",
            source_dir.display()
        ))
    })?;

    let rust_source = emit_rust(hir);
    let core_path = cache_dir.join("lift_core");
    materialize_embedded_core(&core_path)?;
    let local_db = needs_local_dependency(hir, &RustDependency::LocalDb);
    let local_web = needs_local_dependency(hir, &RustDependency::LocalWeb);
    if local_db || local_web {
        materialize_embedded_adapter(&cache_dir.join("lift_db"), EMBEDDED_DB_FILES)?;
    }
    if local_web {
        materialize_embedded_adapter(&cache_dir.join("lift_web"), EMBEDDED_WEB_FILES)?;
    }
    let manifest = emit_cargo_manifest_for_program(stem, hir, "lift_core");
    let generated_source = source_dir.join("main.rs");
    let manifest_path = cache_dir.join("Cargo.toml");
    write_if_changed(&generated_source, rust_source.as_bytes()).map_err(|error| {
        CliError::message(format!(
            "não foi possível gravar `{}`: {error}",
            generated_source.display()
        ))
    })?;
    write_if_changed(&manifest_path, manifest.as_bytes()).map_err(|error| {
        CliError::message(format!(
            "não foi possível gravar `{}`: {error}",
            manifest_path.display()
        ))
    })?;

    Ok(GeneratedProject {
        source_path: source_path.to_owned(),
        manifest_path,
        cache_dir,
    })
}

fn materialize_embedded_core(destination: &Path) -> Result<(), CliError> {
    materialize_embedded_adapter(destination, EMBEDDED_CORE_FILES)
}

fn materialize_embedded_adapter(
    destination: &Path,
    files: &[(&str, &str)],
) -> Result<(), CliError> {
    for (relative_path, contents) in files {
        let path = destination.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CliError::message(format!(
                    "não foi possível criar `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        write_if_changed(&path, contents.as_bytes()).map_err(|error| {
            CliError::message(format!(
                "não foi possível gravar a stdlib embutida em `{}`: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

fn needs_local_dependency(program: &HirProgram, dependency: &RustDependency) -> bool {
    program.declarations.iter().any(|declaration| {
        matches!(declaration, HirDeclaration::Import(import) if &import.dependency == dependency)
    })
}

/// Writes different bytes only; PT-BR: grava somente bytes diferentes dos existentes.
fn write_if_changed(path: &Path, contents: &[u8]) -> std::io::Result<bool> {
    match fs::read(path) {
        Ok(existing) if existing == contents => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::write(path, contents)?;
    Ok(true)
}

fn cargo_build(project: &GeneratedProject) -> Result<CargoBuild, CliError> {
    let mut command = ProcessCommand::new("cargo");
    command
        .arg("build")
        .arg("--release")
        .arg("--message-format=json-render-diagnostics")
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .arg("--target-dir")
        .arg(project.cache_dir.join("target"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = command.spawn().map_err(|error| {
        CliError::message(format!("não foi possível iniciar `cargo build`: {error}"))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliError::message("não foi possível ler a saída de cargo build"))?;
    let mut executable = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| {
            CliError::message(format!(
                "não foi possível ler a saída de cargo build: {error}"
            ))
        })?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            eprintln!("{line}");
            continue;
        };
        if message.get("reason").and_then(Value::as_str) == Some("compiler-artifact")
            && message
                .get("target")
                .and_then(|target| target.get("name"))
                .and_then(Value::as_str)
                == Some("lift_program")
        {
            executable = message
                .get("executable")
                .and_then(Value::as_str)
                .map(PathBuf::from);
        }
    }
    let status = child.wait().map_err(|error| {
        CliError::message(format!("não foi possível aguardar cargo build: {error}"))
    })?;
    Ok(CargoBuild {
        exit_code: exit_code(status),
        executable,
    })
}

fn cargo_check(project: &GeneratedProject) -> Result<u8, CliError> {
    let status = ProcessCommand::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .arg("--target-dir")
        .arg(project.cache_dir.join("target"))
        .status()
        .map_err(|error| {
            CliError::message(format!("não foi possível iniciar `cargo check`: {error}"))
        })?;
    Ok(exit_code(status))
}

fn output_binary_path(source_path: &Path, binary: &Path) -> PathBuf {
    let file_name = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("lift_program");
    let output_name = binary
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{file_name}.{extension}"))
        .unwrap_or_else(|| file_name.to_owned());
    source_path.with_file_name(output_name)
}

fn init_project(directory: &Path) -> Result<(), CliError> {
    fs::create_dir_all(directory).map_err(|error| {
        CliError::message(format!(
            "não foi possível criar `{}`: {error}",
            directory.display()
        ))
    })?;
    let source = directory.join("main.lift");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&source)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CliError::message(format!(
                    "`{}` já existe; nenhum arquivo foi alterado",
                    source.display()
                ))
            } else {
                CliError::message(format!(
                    "não foi possível criar `{}`: {error}",
                    source.display()
                ))
            }
        })?;
    file.write_all(b"// Programa Lift inicial.\n")
        .map_err(|error| {
            CliError::message(format!(
                "não foi possível gravar `{}`: {error}",
                source.display()
            ))
        })?;
    println!("Projeto inicializado: {}", source.display());
    Ok(())
}

fn exit_code(status: std::process::ExitStatus) -> u8 {
    status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1)
}

#[cfg(test)]
mod package_init_tests {
    use super::*;
    #[test]
    fn second_publication_failure_rolls_back_first_and_preserves_existing_source() {
        let directory =
            std::env::temp_dir().join(format!("forklift-init-rollback-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("main.lift"), b"user source").unwrap();
        assert!(publish_package_files(&directory, b"new manifest", b"new source").is_err());
        assert!(!directory.join("lift.toml").exists());
        assert_eq!(
            fs::read(directory.join("main.lift")).unwrap(),
            b"user source"
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(directory.join("main.lift")).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
