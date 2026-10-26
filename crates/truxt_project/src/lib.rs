//! Reusable, read-only project analysis: sources + structured diagnostics + HIR.
//! PT-BR: Análise reutilizável sem emissão: fontes + diagnósticos + HIR validado.
//!
//! Source IDs belong to one Analysis only. A session retains editor overlays,
//! never a stale HIR; each analyze reloads the graph. No Cargo, codegen or output.
//! PT-BR: IDs pertencem a um Analysis. Sessões retêm overlays, nunca HIR antigo;
//! cada analyze recarrega o grafo, sem Cargo, codegen ou gravação de arquivos.

mod diagnostics;
pub mod forklift;
pub mod lint;
mod modules;
mod sources;

pub use sources::{Position, SourceFile, SourceMap};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use truxt_ast::Span;
use truxt_sema::HirProgram;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Span is absent for root I/O errors; imported I/O errors label the importer.
/// PT-BR: Sem span para I/O da entrada; I/O de import aponta ao importador.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub secondary_span: Option<Span>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            span,
            secondary_span: None,
        }
    }
    pub fn warning(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Warning,
            message: message.into(),
            span: Some(span),
            secondary_span: None,
        }
    }
}

#[derive(Debug)]
pub struct Analysis {
    pub entry: PathBuf,
    pub sources: SourceMap,
    /// Session-produced results contain HIR only after the whole graph validates.
    /// Public fields allow reconstruction; consumers must not assume arbitrary
    /// manually constructed HIR has been checked.
    /// PT-BR: Resultados da sessão só têm HIR após validar todo o grafo. Campos
    /// públicos permitem reconstrução; HIR manual não é validado por construção.
    pub hir: Option<HirProgram>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

#[derive(Default)]
pub struct ProjectSession {
    overlays: HashMap<PathBuf, (u64, String)>,
    revision: u64,
}

impl ProjectSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Full text keyed by a stable absolute spelling, preserving `..` components.
    /// Canonical aliases resolve per analysis; their latest overlay update wins.
    /// PT-BR: Texto indexado por grafia absoluta estável, preservando `..`.
    /// Aliases canônicos são resolvidos por análise; vence a última atualização.
    pub fn set_overlay(&mut self, path: impl AsRef<Path>, text: String) -> std::io::Result<()> {
        self.revision = self.revision.saturating_add(1);
        self.overlays
            .insert(lexical_path(path.as_ref())?, (self.revision, text));
        Ok(())
    }

    pub fn remove_overlay(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        self.overlays.remove(&lexical_path(path.as_ref())?);
        Ok(())
    }

    pub fn analyze(&self, entry: impl AsRef<Path>) -> Analysis {
        modules::analyze_project(entry.as_ref(), &self.resolved_overlays())
    }

    /// Resolve a manifested local project without semantic analysis or Cargo.
    /// Accepts a directory, truxt.toml, or a source under the manifest.
    /// PT-BR: Resolve projeto local sem semântica/Cargo; aceita diretório,
    /// truxt.toml ou fonte dentro do projeto.
    pub fn resolve(&self, input: impl AsRef<Path>) -> forklift::Resolution {
        forklift::resolve(input.as_ref(), &self.resolved_overlays())
    }

    fn resolved_overlays(&self) -> HashMap<PathBuf, String> {
        // Resolve aliases anew: saving a previously absent file under a symlink
        // must not orphan its overlay. Latest update wins for canonical aliases.
        // PT-BR: Resolve aliases a cada análise; salvar sob symlink não perde
        // overlay. A atualização mais recente vence aliases canônicos.
        let mut ordered: Vec<_> = self.overlays.iter().collect();
        ordered.sort_by_key(|(_, (revision, _))| *revision);
        ordered
            .into_iter()
            .filter_map(|(path, (_, text))| source_path(path).ok().map(|path| (path, text.clone())))
            .collect()
    }
}

/// Canonical for existing files; lexically absolute for unsaved editor files.
/// PT-BR: Canônico se existir, absoluto lexical para arquivos ainda não salvos.
pub fn source_path(path: &Path) -> std::io::Result<PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        // Match lexical paths of newly created editor files on Windows as well.
        // PT-BR: Alinha também caminhos lexicais de arquivos novos no Windows.
        #[cfg(windows)]
        if let Some(text) = canonical.to_str() {
            if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
                return Ok(PathBuf::from(format!("\\\\{rest}")));
            }
            if let Some(rest) = text.strip_prefix("\\\\?\\") {
                return Ok(PathBuf::from(rest));
            }
        }
        return Ok(canonical);
    }
    lexical_path(path)
}

fn lexical_path(path: &Path) -> std::io::Result<PathBuf> {
    // A lexical collapse of symlink/.. changes filesystem traversal semantics.
    // Preserve spelling until canonicalize can resolve the actual target.
    // PT-BR: Colapsar symlink/.. altera a travessia real. Preserva a grafia até
    // canonicalize conseguir resolver o destino pelo filesystem.
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    #[cfg(windows)]
    {
        let mut parts = path.components();
        if let Some(std::path::Component::Prefix(prefix)) = parts.next() {
            // Resolve only the drive-relative base, never the remaining `..`.
            let base = std::path::absolute(Path::new(prefix.as_os_str()))?;
            return Ok(base.join(parts.as_path()));
        }
    }
    Ok(std::env::current_dir()?.join(path))
}
