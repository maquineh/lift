//! Console output preserving I/O errors for Rust callers.
//!
//! PT-BR: Saída de console que preserva erros de I/O para chamadores Rust.

use std::fmt::Display;
use std::io::{self, Write};

/// Escrita de um valor [`Display`] seguida por `\n`.
///
/// Os métodos propagam erros de I/O ao chamador. A API não adiciona prefixos
/// O compilador expõe `console.log` para primitivos e trata a falha de I/O no binding.
pub struct Console;

impl Console {
    /// Escreve o valor em stdout, acrescentando uma quebra de linha.
    pub fn log(value: impl Display) -> io::Result<()> {
        write_line(&mut io::stdout().lock(), value)
    }

    /// Escreve o valor em stderr, acrescentando uma quebra de linha.
    pub fn error(value: impl Display) -> io::Result<()> {
        write_line(&mut io::stderr().lock(), value)
    }
}

fn write_line(writer: &mut impl Write, value: impl Display) -> io::Result<()> {
    writeln!(writer, "{value}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_display_values_with_one_added_newline() {
        let mut output = Vec::new();
        write_line(&mut output, format_args!("{}: {}", "ação", 42)).unwrap();
        write_line(&mut output, true).unwrap();
        write_line(&mut output, "").unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "ação: 42\ntrue\n\n");
    }

    #[test]
    fn preserves_newlines_in_the_value() {
        let mut output = Vec::new();
        write_line(&mut output, "primeira\nsegunda\n").unwrap();
        assert_eq!(output, b"primeira\nsegunda\n\n");
    }

    #[test]
    fn propagates_io_errors() {
        struct BrokenWriter;

        impl Write for BrokenWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let error = write_line(&mut BrokenWriter, "message").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}
