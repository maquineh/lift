//! Local package CLI entry point; PT-BR: entrada do gerenciador de pacotes locais.
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(truxt_cli::execute_forklift())
}
