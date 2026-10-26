//! Cycle-safe module loading; errors and source ownership survive failed analysis.
//! PT-BR: Carga de módulos com ciclos; erros/fontes sobrevivem à falha da análise.

use crate::{source_path, Analysis, Diagnostic, SourceMap};
use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};
use truxt_ast::{Declaration, ProgramAst, SourceId, Span};

pub(crate) fn analyze_project(root: &Path, overlays: &HashMap<PathBuf, String>) -> Analysis {
    if root
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("truxt"))
        && crate::forklift::find_manifest(root, overlays).is_some()
    {
        let mut resolved = crate::forklift::resolve(root, overlays);
        if resolved.project.is_some() {
            if let Ok(requested) = source_path(root) {
                if !resolved
                    .sources
                    .iter()
                    .any(|(_, source)| source.path == requested)
                {
                    let text = overlays
                        .get(&requested)
                        .cloned()
                        .map(Ok)
                        .unwrap_or_else(|| fs::read_to_string(&requested));
                    let span = text.ok().map(|text| {
                        let source = resolved.sources.add(requested.clone(), text);
                        Span::in_source(source, 0, 0)
                    });
                    resolved.diagnostics.push(Diagnostic::error("package.outside-graph","fonte não pertence ao grafo da entry declarada em truxt.toml; adicione import module ou use a entry do projeto",span));
                    resolved.project = None;
                }
            }
        }
        return resolved.analyze();
    }
    let mut loader = Loader {
        overlays,
        sources: SourceMap::default(),
        seen: HashMap::new(),
        declarations: Vec::new(),
        diagnostics: Vec::new(),
    };
    let entry = match source_path(root) {
        Ok(entry) => entry,
        Err(error) => {
            return Analysis {
                entry: root.to_owned(),
                sources: SourceMap::default(),
                hir: None,
                diagnostics: vec![Diagnostic::error("project.io", error.to_string(), None)],
            }
        }
    };
    if !entry
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("truxt"))
    {
        loader.fail(
            None,
            "project.extension",
            "a extensão do arquivo fonte deve ser `.truxt`".to_owned(),
        );
    } else {
        loader.visit(&entry, true, None);
    }
    let mut hir = None;
    if loader.diagnostics.is_empty() {
        let span = loader
            .sources
            .get(SourceId(0))
            .map(|s| Span::in_source(SourceId(0), 0, s.text.len()))
            .unwrap_or_default();
        match truxt_sema::analyze(ProgramAst::new(loader.declarations, span)) {
            Ok(program) => hir = Some(program),
            Err(errors) => loader
                .diagnostics
                .extend(errors.into_iter().map(|e| Diagnostic {
                    code: crate::diagnostics::semantic_code(e.kind).to_owned(),
                    severity: crate::Severity::Error,
                    message: e.to_string(),
                    span: Some(e.span),
                    secondary_span: e.secondary_span,
                })),
        }
    }
    Analysis {
        entry,
        sources: loader.sources,
        hir,
        diagnostics: loader.diagnostics,
    }
}

struct Loader<'a> {
    overlays: &'a HashMap<PathBuf, String>,
    sources: SourceMap,
    seen: HashMap<PathBuf, SourceId>,
    declarations: Vec<Declaration>,
    diagnostics: Vec<Diagnostic>,
}

impl Loader<'_> {
    fn fail(&mut self, span: Option<Span>, code: &str, message: String) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    fn visit(&mut self, path: &Path, is_root: bool, imported_at: Option<Span>) {
        let path = match source_path(path) {
            Ok(path) => path,
            Err(error) => {
                self.fail(
                    imported_at,
                    "project.io",
                    format!(
                        "não foi possível abrir módulo `{}`: {error}",
                        path.display()
                    ),
                );
                return;
            }
        };
        if self.seen.contains_key(&path) {
            return;
        }
        let text = match self
            .overlays
            .get(&path)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| fs::read_to_string(&path))
        {
            Ok(text) => text,
            Err(error) => {
                self.fail(
                    imported_at,
                    "project.io",
                    format!(
                        "não foi possível ler fonte UTF-8 `{}`: {error}",
                        path.display()
                    ),
                );
                return;
            }
        };
        let source = self.sources.add(path.clone(), text);
        // Mark before recursion: repeated imports and cycles share one SourceId.
        self.seen.insert(path.clone(), source);
        let text = &self.sources.get(source).unwrap().text;
        let tokens = match truxt_lexer::tokenize_with_source(text, source) {
            Ok(tokens) => tokens,
            Err(error) => {
                self.fail(Some(error.span), "lexer.invalid-token", error.to_string());
                return;
            }
        };
        let ast = match truxt_parser::parse_with_source(&tokens, source) {
            Ok(ast) => ast,
            Err(errors) => {
                for e in errors {
                    self.fail(Some(e.span), "parser.syntax", e.to_string());
                }
                return;
            }
        };
        for declaration in ast.declarations {
            match declaration {
                Declaration::ModuleImport(import) => {
                    let relative = Path::new(&import.path);
                    if relative.components().any(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) || !relative.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("truxt")) {
                        self.fail(Some(import.path_span), "project.import-path", "import module exige caminho relativo com extensão `.truxt`".to_owned());
                        continue;
                    }
                    let resolved = path.parent().unwrap_or_else(|| Path::new(".")).join(relative);
                    self.visit(&resolved, false, Some(import.path_span));
                }
                Declaration::Statement(statement) if !is_root => self.fail(Some(statement.span()), "project.module-statement", "módulo importado não pode conter statement executável de topo; mova-o para uma função ou para o arquivo root".to_owned()),
                declaration => self.declarations.push(declaration),
            }
        }
    }
}
