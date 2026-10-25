//! CLI orchestration for Truxt analysis, bundled core extraction, and Cargo.
//!
//! The executable embeds its standard-library sources and materializes them in
//! each generated project, so compilation does not depend on the source checkout.
//! Rust/Cargo and a host linker remain external prerequisites.
//!
//! PT-BR: Orquestra análise Truxt, extração do core embutido e Cargo. O executável
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
use serde_json::Value;
use truxt_codegen::{emit_cargo_manifest_for_program, emit_rust};
use truxt_sema::{analyze, HirProgram};

const EMBEDDED_CORE_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", EMBEDDED_CORE_CARGO_TOML),
    (
        "src/lib.rs",
        include_str!("../../../stdlib/truxt_core/src/lib.rs"),
    ),
    (
        "src/array.rs",
        include_str!("../../../stdlib/truxt_core/src/array.rs"),
    ),
    (
        "src/console.rs",
        include_str!("../../../stdlib/truxt_core/src/console.rs"),
    ),
    (
        "src/string.rs",
        include_str!("../../../stdlib/truxt_core/src/string.rs"),
    ),
    (
        "src/interop.rs",
        include_str!("../../../stdlib/truxt_core/src/interop.rs"),
    ),
    (
        "src/io.rs",
        include_str!("../../../stdlib/truxt_core/src/io.rs"),
    ),
];

const EMBEDDED_CORE_CARGO_TOML: &str = concat!(
    "[package]\nname = \"truxt_core\"\nversion = \"",
    env!("CARGO_PKG_VERSION"),
    "\"\nedition = \"2021\"\ndescription = \"Truxt base library: console, arrays, strings, and UTF-8 I/O\"\n"
);

#[derive(Parser)]
#[command(name = "truxt", version, about = "Compilador da linguagem Truxt")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Gera e compila um programa Truxt em modo release.
    Build {
        /// Arquivo fonte `.truxt`.
        input: PathBuf,
    },
    /// Compila e executa um arquivo Truxt.
    Run {
        /// Arquivo fonte `.truxt`.
        input: PathBuf,
        /// Argumentos enviados ao programa (após `--`).
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    /// Gera Rust e valida-o com `cargo check`.
    Check {
        /// Arquivo fonte `.truxt`.
        input: PathBuf,
    },
    /// Cria um diretório com um arquivo `main.truxt` inicial.
    Init {
        /// Diretório destino (padrão: diretório atual).
        #[arg(default_value = ".")]
        directory: PathBuf,
    },
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
            eprintln!("truxt: {message}");
            1
        }
        Err(CliError::DiagnosticsReported) => 1,
    }
}

fn dispatch(command: Commands) -> Result<u8, CliError> {
    match command {
        Commands::Build { input } => {
            let project = generate_project(&input)?;
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
        Commands::Run { input, args } => {
            let project = generate_project(&input)?;
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
        Commands::Check { input } => {
            let project = generate_project(&input)?;
            cargo_check(&project)
        }
        Commands::Init { directory } => {
            init_project(&directory)?;
            Ok(0)
        }
    }
}

fn generate_project(input: &Path) -> Result<GeneratedProject, CliError> {
    let source_path = fs::canonicalize(input).map_err(|error| {
        CliError::message(format!(
            "não foi possível abrir `{}`: {error}",
            input.display()
        ))
    })?;
    if !source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("truxt"))
    {
        return Err(CliError::message(
            "a extensão do arquivo fonte deve ser `.truxt`",
        ));
    }
    let source = fs::read_to_string(&source_path).map_err(|error| {
        CliError::message(format!(
            "não foi possível ler `{}`: {error}",
            source_path.display()
        ))
    })?;

    let tokens = match truxt_lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(error) => {
            report_diagnostic(&source_path, &source, error.span, &error.to_string());
            return Err(CliError::DiagnosticsReported);
        }
    };
    let ast = match truxt_parser::parse(&tokens) {
        Ok(ast) => ast,
        Err(errors) => {
            for error in errors {
                report_diagnostic(&source_path, &source, error.span, &error.to_string());
            }
            return Err(CliError::DiagnosticsReported);
        }
    };
    let hir = match analyze(ast) {
        Ok(hir) => hir,
        Err(errors) => {
            for error in errors {
                report_diagnostic(&source_path, &source, error.span, &error.to_string());
                if let Some(secondary) = error.secondary_span {
                    report_diagnostic(&source_path, &source, secondary, "local relacionado");
                }
            }
            return Err(CliError::DiagnosticsReported);
        }
    };

    write_generated_project(&source_path, &hir)
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
        .unwrap_or("truxt-program");
    let cache_dir = parent.join(".truxt_cache").join(stem);
    let source_dir = cache_dir.join("src");
    fs::create_dir_all(&source_dir).map_err(|error| {
        CliError::message(format!(
            "não foi possível criar `{}`: {error}",
            source_dir.display()
        ))
    })?;

    let rust_source = emit_rust(hir);
    let core_path = cache_dir.join("truxt_core");
    materialize_embedded_core(&core_path)?;
    let manifest = emit_cargo_manifest_for_program(stem, hir, "truxt_core");
    let generated_source = source_dir.join("main.rs");
    let manifest_path = cache_dir.join("Cargo.toml");
    fs::write(&generated_source, rust_source).map_err(|error| {
        CliError::message(format!(
            "não foi possível gravar `{}`: {error}",
            generated_source.display()
        ))
    })?;
    fs::write(&manifest_path, manifest).map_err(|error| {
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
    for (relative_path, contents) in EMBEDDED_CORE_FILES {
        let path = destination.join(relative_path);
        if fs::read(&path).is_ok_and(|existing| existing == contents.as_bytes()) {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CliError::message(format!(
                    "não foi possível criar `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::write(&path, contents).map_err(|error| {
            CliError::message(format!(
                "não foi possível gravar a stdlib embutida em `{}`: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
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
                == Some("truxt_program")
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
        .unwrap_or("truxt_program");
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
    let source = directory.join("main.truxt");
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
    file.write_all(b"// Programa Truxt inicial.\n")
        .map_err(|error| {
            CliError::message(format!(
                "não foi possível gravar `{}`: {error}",
                source.display()
            ))
        })?;
    println!("Projeto inicializado: {}", source.display());
    Ok(())
}

fn report_diagnostic(path: &Path, source: &str, span: truxt_ast::Span, message: &str) {
    let offset = span.start.min(source.len());
    let mut boundary = offset;
    while !source.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let prefix = &source[..boundary];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit('\n')
        .next()
        .map(|line| line.chars().count() + 1)
        .unwrap_or(1);
    eprintln!("{}:{line}:{column}: erro: {message}", path.display());
}

fn exit_code(status: std::process::ExitStatus) -> u8 {
    status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1)
}
