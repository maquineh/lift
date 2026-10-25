//! UTF-8 arguments, standard input, and file operations for Truxt programs.
//!
//! Functions return `Result<T, String>` so callers can handle errors. Error
//! strings contain the operation, path when applicable, and original error.
//! `write_file` creates or truncates files without creating parent directories.
//!
//! PT-BR: Argumentos, entrada padrão e arquivos UTF-8 para Truxt. As funções
//! retornam `Result<T, String>` para permitir tratar erros. As mensagens contêm
//! operação, caminho quando aplicável e erro original. `write_file` cria ou
//! trunca arquivos, sem criar diretórios pais.

use std::{ffi::OsString, fs, io::Read};

/// Returns arguments in order, excluding argv[0]; invalid Unicode returns `Err`.
///
/// PT-BR: Retorna argumentos em ordem, excluindo argv[0]; Unicode inválido retorna `Err`.
pub fn arguments() -> Result<Vec<String>, String> {
    arguments_from(std::env::args_os())
}

/// Reads all UTF-8 standard input until EOF, preserving line endings.
/// I/O errors and invalid UTF-8 return `Err`.
///
/// PT-BR: Lê stdin UTF-8 até EOF, preservando quebras de linha.
/// Erros de I/O e UTF-8 inválido retornam `Err`.
pub fn read_stdin() -> Result<String, String> {
    read_stdin_from(&mut std::io::stdin().lock())
}

/// Reads a UTF-8 file; relative paths use the caller's working directory.
/// Errors include the operation, path, and original error.
///
/// PT-BR: Lê arquivo UTF-8; caminhos relativos usam o diretório do chamador.
/// Erros contêm operação, caminho e erro original.
pub fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("read_file `{path}`: {error}"))
}

/// Writes UTF-8 text, creating or truncating a file without creating parent directories.
/// Relative paths use the caller's working directory; failures return `Err`.
///
/// PT-BR: Grava UTF-8, criando ou truncando o arquivo sem criar diretórios pais.
/// Caminhos relativos usam o diretório do chamador; falhas retornam `Err`.
pub fn write_file(path: &str, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| format!("write_file `{path}`: {error}"))
}

fn arguments_from(arguments: impl IntoIterator<Item = OsString>) -> Result<Vec<String>, String> {
    arguments
        .into_iter()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|argument| format!("arguments: argument is not valid UTF-8: {argument:?}"))
        })
        .collect()
}

fn read_stdin_from(reader: &mut impl Read) -> Result<String, String> {
    let mut contents = String::new();
    reader
        .read_to_string(&mut contents)
        .map_err(|error| format!("read_stdin: {error}"))?;
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("truxt-core-io-{}-{sequence}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
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

    #[test]
    fn arguments_skip_the_executable_and_preserve_text() {
        let arguments = ["program", "", "two words", "ação🙂"].map(OsString::from);
        assert_eq!(
            arguments_from(arguments).unwrap(),
            ["", "two words", "ação🙂"]
        );
        assert!(arguments_from([OsString::from("program")])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn arguments_reject_invalid_unicode() {
        #[cfg(unix)]
        let invalid = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![0xff])
        };
        #[cfg(windows)]
        let invalid = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xd800])
        };
        #[cfg(any(unix, windows))]
        {
            let error = arguments_from([OsString::from("program"), invalid]).unwrap_err();
            assert!(error.contains("arguments: argument is not valid UTF-8"));
        }
    }

    #[test]
    fn stdin_reads_until_eof_and_rejects_invalid_utf8() {
        assert_eq!(
            read_stdin_from(&mut "ação🙂\r\nsecond\n".as_bytes()).unwrap(),
            "ação🙂\r\nsecond\n"
        );
        assert_eq!(read_stdin_from(&mut &b""[..]).unwrap(), "");
        assert!(read_stdin_from(&mut &[0xff][..])
            .unwrap_err()
            .contains("read_stdin:"));
    }

    #[test]
    fn file_writes_create_and_truncate_utf8_text() {
        let directory = TempDir::new();
        let file = directory.path().join("text.txt");
        let path = file.to_str().unwrap();
        write_file(path, "ação🙂\nsecond line\n").unwrap();
        assert_eq!(read_file(path).unwrap(), "ação🙂\nsecond line\n");
        write_file(path, "é").unwrap();
        assert_eq!(read_file(path).unwrap(), "é");
        write_file(path, "").unwrap();
        assert_eq!(read_file(path).unwrap(), "");
    }

    #[test]
    fn file_failures_report_the_operation_and_path() {
        let directory = TempDir::new();
        let missing = directory.path().join("missing.txt");
        let message = read_file(missing.to_str().unwrap()).unwrap_err();
        assert!(message.contains("read_file"));
        assert!(message.contains("missing.txt"));
        let invalid = directory.path().join("invalid.txt");
        fs::write(&invalid, [0xff]).unwrap();
        assert!(read_file(invalid.to_str().unwrap())
            .unwrap_err()
            .contains("invalid.txt"));
        let message = write_file(
            directory.path().to_str().unwrap(),
            "cannot replace a directory",
        )
        .unwrap_err();
        assert!(message.contains("write_file"));
        assert!(message.contains(directory.path().to_str().unwrap()));
        let missing_parent = directory.path().join("missing-parent/output.txt");
        let message =
            write_file(missing_parent.to_str().unwrap(), "parent is not created").unwrap_err();
        assert!(message.contains("write_file"));
        assert!(message.contains("missing-parent"));
        assert!(!missing_parent.exists());
    }
}
