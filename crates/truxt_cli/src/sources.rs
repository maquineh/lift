//! Source ownership for one compilation and diagnostic source IDs.
//!
//! PT-BR: Posse das fontes de uma compilação e seus IDs para diagnósticos.

use std::path::PathBuf;
use truxt_ast::SourceId;

pub(crate) struct SourceFile {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Default)]
pub(crate) struct SourceMap {
    files: Vec<SourceFile>,
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
