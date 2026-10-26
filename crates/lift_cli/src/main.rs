//! Process entry point that forwards the CLI result as an exit status.
//!
//! PT-BR: Entrada do processo que propaga o resultado da CLI como código de saída.

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(lift_cli::execute())
}
