//! Canonical, cycle-safe resolution of explicit relative Truxt source imports.
//!
//! PT-BR: Resolução canônica de imports relativos Truxt, com deduplicação e ciclos.

use crate::{diagnostics::report_diagnostic, sources::SourceMap, CliError};
use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};
use truxt_ast::{Declaration, ProgramAst, SourceId, Span};

pub(crate) struct LoadedProgram {
    pub ast: ProgramAst,
    pub sources: SourceMap,
}

pub(crate) fn load_program(root: &Path) -> Result<LoadedProgram, CliError> {
    let mut loader = Loader::default();
    loader.visit(root, true, None)?;
    if loader.had_errors {
        return Err(CliError::DiagnosticsReported);
    }
    let root_span = loader
        .sources
        .get(SourceId(0))
        .map(|source| Span::in_source(SourceId(0), 0, source.text.len()))
        .unwrap_or_default();
    Ok(LoadedProgram {
        ast: ProgramAst::new(loader.declarations, root_span),
        sources: loader.sources,
    })
}

#[derive(Default)]
struct Loader {
    sources: SourceMap,
    seen: HashMap<PathBuf, SourceId>,
    declarations: Vec<Declaration>,
    had_errors: bool,
}

impl Loader {
    fn failure(&self, at: Option<Span>, message: String) -> CliError {
        if let Some(span) = at {
            report_diagnostic(&self.sources, span, None, &message);
            CliError::DiagnosticsReported
        } else {
            CliError::message(message)
        }
    }

    fn visit(
        &mut self,
        path: &Path,
        is_root: bool,
        imported_at: Option<Span>,
    ) -> Result<(), CliError> {
        let path = fs::canonicalize(path).map_err(|error| {
            self.failure(
                imported_at,
                format!(
                    "não foi possível abrir módulo `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        if self.seen.contains_key(&path) {
            return Ok(());
        }
        let text = fs::read_to_string(&path).map_err(|error| {
            self.failure(
                imported_at,
                format!(
                    "não foi possível ler fonte UTF-8 `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        let source = self.sources.add(path.clone(), text);
        // EN: Mark before traversing imports, so cycles (including the root) stop here.
        // PT-BR: Marca antes de visitar imports, encerrando ciclos inclusive pelo root.
        self.seen.insert(path.clone(), source);
        let text = &self.sources.get(source).unwrap().text;
        let tokens = match truxt_lexer::tokenize_with_source(text, source) {
            Ok(tokens) => tokens,
            Err(error) => {
                report_diagnostic(&self.sources, error.span, None, &error.to_string());
                return Err(CliError::DiagnosticsReported);
            }
        };
        let ast = match truxt_parser::parse_with_source(&tokens, source) {
            Ok(ast) => ast,
            Err(errors) => {
                for error in errors {
                    report_diagnostic(&self.sources, error.span, None, &error.to_string());
                }
                return Err(CliError::DiagnosticsReported);
            }
        };
        for declaration in ast.declarations {
            match declaration {
                Declaration::ModuleImport(import) => {
                    let relative = Path::new(&import.path);
                    if relative.components().any(|component| {
                        matches!(component, Component::Prefix(_) | Component::RootDir)
                    }) || !relative
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("truxt"))
                    {
                        return Err(self.failure(
                            Some(import.path_span),
                            "import module exige caminho relativo com extensão `.truxt`".to_owned(),
                        ));
                    }
                    let resolved = path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(relative);
                    self.visit(&resolved, false, Some(import.path_span))?;
                }
                Declaration::Statement(statement) if !is_root => {
                    report_diagnostic(&self.sources, statement.span(), None, "módulo importado não pode conter statement executável de topo; mova-o para uma função ou para o arquivo root");
                    self.had_errors = true;
                }
                declaration => self.declarations.push(declaration),
            }
        }
        Ok(())
    }
}
