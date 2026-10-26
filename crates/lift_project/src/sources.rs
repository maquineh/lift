//! Source ownership for one compilation and diagnostic source IDs.
//!
//! PT-BR: Posse das fontes de uma compilação e seus IDs para diagnósticos.

use lift_ast::SourceId;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

/// Zero-based LSP position (UTF-16 code units, not bytes or Unicode scalars).
/// PT-BR: Posição LSP base zero em unidades UTF-16, não bytes/caracteres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl SourceFile {
    pub fn position_utf16(&self, byte: usize) -> Position {
        let mut byte = byte.min(self.text.len());
        while !self.text.is_char_boundary(byte) {
            byte -= 1;
        }
        let mut line = 0;
        let mut line_start = 0;
        let mut offset = 0;
        let bytes = self.text.as_bytes();
        while offset < byte {
            if bytes[offset] == b'\r' || bytes[offset] == b'\n' {
                let width = if bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                // Positions inside CRLF refer to the previous line's end.
                if offset + width > byte {
                    break;
                }
                line += 1;
                offset += width;
                line_start = offset;
            } else {
                offset += 1;
            }
        }
        let column = self.text[line_start..byte]
            .strip_suffix('\r')
            .unwrap_or(&self.text[line_start..byte])
            .encode_utf16()
            .count();
        Position {
            line: line as u32,
            character: column as u32,
        }
    }
}

impl SourceMap {
    pub fn add(&mut self, path: PathBuf, text: String) -> SourceId {
        let id = SourceId(self.files.len());
        self.files.push(SourceFile { path, text });
        id
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SourceId, &SourceFile)> {
        self.files
            .iter()
            .enumerate()
            .map(|(index, file)| (SourceId(index), file))
    }
}
