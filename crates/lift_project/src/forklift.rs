//! Local Lift packages, distinct from Cargo's Rust crate dependencies.
//! PT-BR: Pacotes Lift locais, separados das dependências de crates Rust do Cargo.
use crate::{source_path, Diagnostic, SourceMap};
use lift_ast::{
    Declaration, PackageAst, PackageBinding, PackageId, ProgramAst, ProjectAst, SourceId, Span,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct PackageMetadata {
    pub id: PackageId,
    pub manifest: PathBuf,
    pub name: String,
    pub version: String,
    pub is_library: bool,
    pub entry: PathBuf,
    pub dependencies: BTreeMap<String, PackageId>,
}

#[derive(Debug)]
pub struct Resolution {
    pub entry: PathBuf,
    pub sources: SourceMap,
    pub packages: Vec<PackageMetadata>,
    pub project: Option<ProjectAst>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    /// Consume the captured graph once; semantic analysis performs no new I/O.
    /// Resolution inputs locate a manifest; analysis always uses its declared
    /// entry, even if resolution was requested using another source path.
    /// PT-BR: Consome o grafo sem reler arquivos; entrada de resolve localiza o
    /// manifesto, mas a análise sempre usa a entry declarada nele.
    pub fn analyze(self) -> crate::Analysis {
        let mut diagnostics = self.diagnostics;
        let hir = if let Some(ast) = self.project {
            match lift_sema::analyze_project(ast) {
                Ok(hir) => Some(hir),
                Err(errors) => {
                    diagnostics.extend(errors.into_iter().map(|error| crate::Diagnostic {
                        code: crate::diagnostics::semantic_code(error.kind).to_owned(),
                        severity: crate::Severity::Error,
                        message: error.to_string(),
                        span: Some(error.span),
                        secondary_span: error.secondary_span,
                    }));
                    None
                }
            }
        } else {
            None
        };
        crate::Analysis {
            entry: self.entry,
            sources: self.sources,
            hir,
            diagnostics,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    package: Package,
    #[serde(default)]
    dependencies: BTreeMap<String, Dependency>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    name: toml::Spanned<String>,
    version: toml::Spanned<String>,
    kind: toml::Spanned<String>,
    entry: toml::Spanned<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dependency {
    path: toml::Spanned<String>,
}

pub(crate) fn find_manifest(input: &Path, overlays: &HashMap<PathBuf, String>) -> Option<PathBuf> {
    let absolute = source_path(input).ok()?;
    if absolute.file_name().is_some_and(|s| s == "lift.toml") {
        return Some(absolute);
    }
    let start = if absolute.is_dir() {
        absolute.as_path()
    } else {
        absolute.parent()?
    };
    start
        .ancestors()
        .map(|dir| dir.join("lift.toml"))
        .find(|path| {
            path.is_file()
                || source_path(path)
                    .ok()
                    .is_some_and(|p| overlays.contains_key(&p))
        })
}

pub(crate) fn resolve(input: &Path, overlays: &HashMap<PathBuf, String>) -> Resolution {
    let mut loader = Loader {
        overlays,
        sources: SourceMap::default(),
        metadata: Vec::new(),
        packages: Vec::new(),
        seen: HashMap::new(),
        active: HashSet::new(),
        diagnostics: Vec::new(),
    };
    let mut entry = input.to_owned();
    let root = match find_manifest(input, overlays) {
        Some(path) => loader.package(&path, None),
        None => {
            loader.fail(
                "package.manifest-not-found",
                "lift.toml ancestral não encontrado".to_owned(),
                None,
            );
            None
        }
    };
    if let Some(root) = root {
        if let Some(metadata) = loader.metadata.iter().find(|p| p.id == root) {
            entry = metadata.entry.clone();
        }
    }
    let project = if loader.diagnostics.is_empty() {
        root.map(|root| ProjectAst {
            root,
            packages: loader.packages,
        })
    } else {
        None
    };
    Resolution {
        entry,
        sources: loader.sources,
        packages: loader.metadata,
        project,
        diagnostics: loader.diagnostics,
    }
}

struct Loader<'a> {
    overlays: &'a HashMap<PathBuf, String>,
    sources: SourceMap,
    metadata: Vec<PackageMetadata>,
    packages: Vec<PackageAst>,
    seen: HashMap<PathBuf, PackageId>,
    active: HashSet<PathBuf>,
    diagnostics: Vec<Diagnostic>,
}
impl Loader<'_> {
    fn fail(&mut self, code: &str, message: String, span: Option<Span>) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }
    fn read(&mut self, path: &Path, at: Option<Span>) -> Option<(PathBuf, SourceId)> {
        let path = match source_path(path) {
            Ok(path) => path,
            Err(e) => {
                self.fail("package.io", e.to_string(), at);
                return None;
            }
        };
        let text = match self
            .overlays
            .get(&path)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| fs::read_to_string(&path))
        {
            Ok(text) => text,
            Err(e) => {
                self.fail(
                    "package.io",
                    format!("não foi possível ler `{}`: {e}", path.display()),
                    at,
                );
                return None;
            }
        };
        let id = self.sources.add(path.clone(), text);
        Some((path, id))
    }
    fn package(&mut self, manifest: &Path, at: Option<Span>) -> Option<PackageId> {
        let path = source_path(manifest).ok()?;
        if self.active.contains(&path) {
            self.fail(
                "package.cycle",
                format!("ciclo de dependências locais em `{}`", path.display()),
                at,
            );
            return None;
        }
        if let Some(id) = self.seen.get(&path) {
            return Some(*id);
        }
        let (path, source) = self.read(&path, at)?;
        let text = &self.sources.get(source).unwrap().text;
        let parsed: Manifest = match toml::from_str(text) {
            Ok(m) => m,
            Err(error) => {
                let range = error.span().unwrap_or(0..0);
                self.fail(
                    "package.manifest",
                    error.to_string(),
                    Some(Span::in_source(source, range.start, range.end)),
                );
                return None;
            }
        };
        let field_span = |value: &toml::Spanned<String>| {
            let range = value.span();
            Span::in_source(source, range.start, range.end)
        };
        let root = path.parent().unwrap();
        if parsed.package.name.get_ref().is_empty() || parsed.package.version.get_ref().is_empty() {
            self.fail(
                "package.metadata",
                "package.name/version precisam ser textos não vazios".to_owned(),
                Some(field_span(&parsed.package.name)),
            );
            return None;
        }
        let library = match parsed.package.kind.get_ref().as_str() {
            "lib" => true,
            "bin" => false,
            _ => {
                self.fail(
                    "package.kind",
                    "package.kind exige `lib` ou `bin`".to_owned(),
                    Some(field_span(&parsed.package.kind)),
                );
                return None;
            }
        };
        let entry_spec = Path::new(parsed.package.entry.get_ref());
        if !relative(entry_spec)
            || !entry_spec
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("lift"))
        {
            self.fail(
                "package.entry",
                "package.entry exige caminho relativo `.lift`".to_owned(),
                Some(field_span(&parsed.package.entry)),
            );
            return None;
        }
        let entry = source_path(&root.join(entry_spec)).ok()?;
        if !inside_package(&entry, root) {
            self.fail(
                "package.boundary",
                "entrada fora da raiz do pacote".to_owned(),
                Some(field_span(&parsed.package.entry)),
            );
            return None;
        }
        let id = PackageId(self.seen.len());
        self.seen.insert(path.clone(), id);
        self.active.insert(path.clone());
        let mut dependencies = BTreeMap::new();
        for (alias, dep) in &parsed.dependencies {
            if !identifier(alias) || matches!(alias.as_str(), "console" | "option" | "result") {
                self.fail(
                    "package.alias",
                    format!("alias de dependência inválido `{alias}`"),
                    Some(field_span(&dep.path)),
                );
                continue;
            }
            let spec = Path::new(dep.path.get_ref());
            if !relative(spec) {
                self.fail(
                    "package.path",
                    "dependência path deve ser relativa ao manifesto".to_owned(),
                    Some(field_span(&dep.path)),
                );
                continue;
            }
            let target = root.join(spec).join("lift.toml");
            if let Some(target_id) = self.package(&target, Some(field_span(&dep.path))) {
                if self
                    .metadata
                    .iter()
                    .find(|p| p.id == target_id)
                    .is_some_and(|p| !p.is_library)
                {
                    self.fail(
                        "package.library",
                        format!("dependência `{alias}` precisa declarar kind = lib"),
                        Some(field_span(&dep.path)),
                    );
                } else {
                    dependencies.insert(alias.clone(), target_id);
                }
            }
        }
        let mut declarations = Vec::new();
        let mut imports = Vec::new();
        let mut seen = HashSet::new();
        self.module(
            &entry,
            &path,
            library,
            true,
            Some(field_span(&parsed.package.entry)),
            &dependencies,
            &mut seen,
            &mut declarations,
            &mut imports,
        );
        let span = declarations
            .first()
            .map(Declaration::span)
            .unwrap_or_else(|| Span::in_source(source, 0, 0));
        self.packages.push(PackageAst {
            id,
            program: ProgramAst::new(declarations, span),
            is_library: library,
            imports,
        });
        self.metadata.push(PackageMetadata {
            id,
            manifest: path.clone(),
            name: parsed.package.name.into_inner(),
            version: parsed.package.version.into_inner(),
            is_library: library,
            entry,
            dependencies,
        });
        self.active.remove(&path);
        Some(id)
    }
    #[allow(clippy::too_many_arguments)]
    fn module(
        &mut self,
        file: &Path,
        manifest: &Path,
        library: bool,
        is_entry: bool,
        at: Option<Span>,
        dependencies: &BTreeMap<String, PackageId>,
        seen: &mut HashSet<PathBuf>,
        declarations: &mut Vec<Declaration>,
        imports: &mut Vec<PackageBinding>,
    ) {
        let path = match source_path(file) {
            Ok(path) => path,
            Err(e) => {
                self.fail("package.io", e.to_string(), at);
                return;
            }
        };
        if !inside_package(&path, manifest.parent().unwrap())
            || find_manifest(&path, self.overlays)
                .is_some_and(|owner| source_path(&owner).ok().as_deref() != Some(manifest))
        {
            self.fail(
                "package.boundary",
                "import module cruza a fronteira do pacote; use dependência e namespace".to_owned(),
                at,
            );
            return;
        }
        if !seen.insert(path.clone()) {
            return;
        }
        let Some((path, source)) = self.read(&path, at) else {
            return;
        };
        let text = &self.sources.get(source).unwrap().text;
        let tokens = match lift_lexer::tokenize_with_source(text, source) {
            Ok(t) => t,
            Err(e) => {
                self.fail("lexer.invalid-token", e.to_string(), Some(e.span));
                return;
            }
        };
        let ast = match lift_parser::parse_with_source(&tokens, source) {
            Ok(a) => a,
            Err(errors) => {
                for e in errors {
                    self.fail("parser.syntax", e.to_string(), Some(e.span));
                }
                return;
            }
        };
        for declaration in ast.declarations {
            match declaration {
                Declaration::ModuleImport(import) => {
                    let spec = Path::new(&import.path);
                    if !relative(spec)
                        || !spec
                            .extension()
                            .and_then(|s| s.to_str())
                            .is_some_and(|s| s.eq_ignore_ascii_case("lift"))
                    {
                        self.fail(
                            "project.import-path",
                            "import module exige caminho relativo `.lift`".to_owned(),
                            Some(import.path_span),
                        );
                    } else {
                        self.module(
                            &path.parent().unwrap().join(spec),
                            manifest,
                            library,
                            false,
                            Some(import.path_span),
                            dependencies,
                            seen,
                            declarations,
                            imports,
                        );
                    }
                }
                Declaration::PackageImport(import) => {
                    if matches!(import.alias.name.as_str(), "console" | "option" | "result") {
                        self.fail(
                            "package.reserved-alias",
                            "alias de pacote reservado para builtin: console, option ou result"
                                .to_owned(),
                            Some(import.alias.span),
                        );
                        continue;
                    }
                    if let Some(target) = dependencies.get(&import.dependency) {
                        imports.push(PackageBinding {
                            alias: import.alias,
                            target: *target,
                        });
                    } else {
                        self.fail(
                            "package.undeclared",
                            format!(
                                "dependência `{}` não declarada em lift.toml",
                                import.dependency
                            ),
                            Some(import.dependency_span),
                        );
                    }
                }
                Declaration::Statement(statement) if library || !is_entry => self.fail(
                    "project.module-statement",
                    "biblioteca/módulo importado não pode conter statement executável de topo"
                        .to_owned(),
                    Some(statement.span()),
                ),
                declaration => declarations.push(declaration),
            }
        }
    }
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path
            .components()
            .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
}
fn identifier(name: &str) -> bool {
    lift_lexer::tokenize(name).is_ok_and(|tokens| {
        tokens.len() == 1
            && matches!(&tokens[0].0,lift_lexer::Token::Identifier(value) if value==name)
    })
}

// Resolve the existing prefix for boundary checks only. Source/overlay keys keep
// their original absolute spelling until the complete path can canonicalize.
// PT-BR: Resolve prefixo existente só para fronteiras; IDs/overlays preservam a
// grafia absoluta até o caminho completo poder ser canonicalizado.
fn boundary_path(path: &Path) -> std::io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => source_path(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(error)?;
            let mut base = boundary_path(parent)?;
            match path.components().next_back() {
                Some(Component::ParentDir) => {
                    base.pop();
                }
                Some(Component::CurDir) => {}
                Some(Component::Normal(name)) => base.push(name),
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "caminho sem prefixo existente",
                    ))
                }
            }
            Ok(base)
        }
        Err(error) => Err(error),
    }
}
fn inside_package(path: &Path, root: &Path) -> bool {
    boundary_path(path)
        .ok()
        .zip(boundary_path(root).ok())
        .is_some_and(|(path, root)| path.starts_with(root))
}
