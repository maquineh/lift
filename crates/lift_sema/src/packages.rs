//! Contextual package analysis. Source AST names are never textually rewritten.
//! PT-BR: Análise contextual; nomes da AST fonte nunca são renomeados por texto.
use crate::{
    analyzer::{analyze_context, FunctionInfo, ParameterInfo},
    *,
};
use lift_ast::{Declaration, PackageId, ProjectAst};
use std::collections::{HashMap, HashSet};

/// Analyze a resolved package graph without filesystem access. The legacy
/// `analyze(ProgramAst)` retains its single global namespace.
/// PT-BR: Analisa grafo resolvido sem I/O. A API legada mantém namespace único.
pub fn analyze_project(project: ProjectAst) -> Result<HirProgram, Vec<SemaError>> {
    let mut pending = HashMap::new();
    for package in project.packages {
        let span = package.program.span;
        if pending.insert(package.id, package).is_some() {
            return Err(vec![SemaError::new(
                SemaErrorKind::InvalidPackage,
                "identidade de pacote duplicada",
                span,
            )]);
        }
    }
    let Some(root) = pending.get(&project.root) else {
        return Err(vec![SemaError::new(
            SemaErrorKind::InvalidPackage,
            "pacote root ausente",
            Default::default(),
        )]);
    };
    let root_span = root.program.span;
    let mut exports = HashMap::<PackageId, HashMap<String, FunctionInfo>>::new();
    let mut output = HirProgram {
        declarations: Vec::new(),
    };
    let mut next_binding = 0;
    while !pending.is_empty() {
        let mut ready: Vec<_> = pending
            .values()
            .filter(|package| {
                package
                    .imports
                    .iter()
                    .all(|import| exports.contains_key(&import.target))
            })
            .map(|package| package.id)
            .collect();
        ready.sort();
        let Some(id) = ready.first().copied() else {
            return Err(vec![SemaError::new(
                SemaErrorKind::InvalidPackage,
                "ciclo ou dependência de pacote não resolvida",
                root_span,
            )]);
        };
        let package = pending.remove(&id).unwrap();
        if !package.is_library && id != project.root {
            return Err(vec![SemaError::new(
                SemaErrorKind::InvalidPackage,
                "dependência Lift precisa ser biblioteca",
                package.program.span,
            )]);
        }
        let mut external = HashMap::new();
        let mut aliases = HashMap::new();
        for import in package.imports {
            if matches!(import.alias.name.as_str(), "console" | "option" | "result") {
                return Err(vec![SemaError::new(
                    SemaErrorKind::InvalidPackage,
                    "alias de pacote reservado para builtin: console, option ou result",
                    import.alias.span,
                )]);
            }
            if let Some((target, previous)) = aliases.insert(
                import.alias.name.clone(),
                (import.target, import.alias.span),
            ) {
                if target != import.target {
                    return Err(vec![SemaError::new(
                        SemaErrorKind::InvalidPackage,
                        "alias de pacote ambíguo",
                        import.alias.span,
                    )
                    .with_secondary(previous)]);
                }
            }
            external.insert(
                format!("{}.", import.alias.name),
                FunctionInfo {
                    name: String::new(),
                    parameters: Vec::new(),
                    return_type: ResolvedType::Void,
                    has_explicit_return_type: true,
                    span: import.alias.span,
                },
            );
            for (name, signature) in &exports[&import.target] {
                external.insert(format!("{}.{name}", import.alias.name), signature.clone());
            }
        }
        let mut ast = package.program;
        for declaration in &ast.declarations {
            if let Declaration::PackageImport(import) = declaration {
                if !aliases.contains_key(&import.alias.name) {
                    return Err(vec![SemaError::new(
                        SemaErrorKind::InvalidPackage,
                        "import de pacote não resolvido pelo contexto",
                        import.span,
                    )]);
                }
            }
        }
        ast.declarations
            .retain(|d| !matches!(d, Declaration::PackageImport(_)));
        let mut exported = HashMap::new();
        for declaration in &mut ast.declarations {
            if let Declaration::ExportFunction(function) = declaration {
                exported.insert(function.name.name.clone(), function.name.span);
                *declaration = Declaration::Function(function.clone());
            }
            if let Declaration::Statement(statement) = declaration {
                if package.is_library || id != project.root {
                    return Err(vec![SemaError::new(
                        SemaErrorKind::InvalidPackage,
                        "biblioteca não pode conter statement executável de topo",
                        statement.span(),
                    )]);
                }
            }
            let name = match declaration {
                Declaration::Function(f) => Some(&f.name),
                Declaration::Interface(i) => Some(&i.name),
                Declaration::Import(i) => Some(&i.name),
                _ => None,
            };
            if let Some(name) = name {
                if external.contains_key(&format!("{}.", name.name)) {
                    return Err(vec![SemaError::new(
                        SemaErrorKind::InvalidPackage,
                        "namespace de pacote conflita com declaração",
                        name.span,
                    )]);
                }
            }
        }
        let mut hir = analyze_context(ast, external)?;
        for declaration in &hir.declarations {
            if let HirDeclaration::Function(function) = declaration {
                if let Some(span) = exported.get(&function.name) {
                    if !public_type(&function.return_type)
                        || function.parameters.iter().any(|p| !public_type(&p.ty))
                    {
                        return Err(vec![SemaError::new(SemaErrorKind::InvalidExport, "função exportada aceita somente primitivos/containers; tipos customizados e closures não são públicos nesta fase", *span)]);
                    }
                }
            }
        }
        qualify(&mut hir, id, &mut next_binding);
        let mut public = HashMap::new();
        for (name, span) in exported {
            let qualified = symbol(id, &name);
            if let Some(function) = hir.declarations.iter().find_map(|d| match d {
                HirDeclaration::Function(f) if f.name == qualified => Some(f),
                _ => None,
            }) {
                public.insert(
                    name,
                    FunctionInfo {
                        name: function.name.clone(),
                        parameters: function
                            .parameters
                            .iter()
                            .map(|p| ParameterInfo {
                                name: p.name.clone(),
                                ty: p.ty.clone(),
                                span: p.span,
                                pass_mode: p.pass_mode,
                                mutated: p.requires_mut_binding
                                    || p.pass_mode == ParameterPassMode::BorrowMut,
                            })
                            .collect(),
                        return_type: function.return_type.clone(),
                        has_explicit_return_type: true,
                        span,
                    },
                );
            }
        }
        exports.insert(id, public);
        output.declarations.extend(hir.declarations);
    }
    // Rust FFI constraints apply to the final generated Cargo project as well.
    let mut rust_dependencies = HashMap::new();
    for declaration in &output.declarations {
        if let HirDeclaration::Import(import) = declaration {
            let name = import.rust_path.split("::").next().unwrap();
            if let Some((dependency, span)) =
                rust_dependencies.insert(name.to_owned(), (&import.dependency, import.span))
            {
                if dependency != &import.dependency {
                    return Err(vec![SemaError::new(
                        SemaErrorKind::InvalidImport,
                        format!("dependências Rust incompatíveis para `{name}` entre pacotes"),
                        import.span,
                    )
                    .with_secondary(span)]);
                }
            }
        }
    }
    Ok(output)
}

fn public_type(ty: &ResolvedType) -> bool {
    match ty {
        ResolvedType::Struct(_) | ResolvedType::Closure(..) | ResolvedType::Resource(_) => false,
        ResolvedType::Array(t) | ResolvedType::Optional(t) => public_type(t),
        ResolvedType::Result(a, b) => public_type(a) && public_type(b),
        _ => true,
    }
}
fn symbol(package: PackageId, name: &str) -> String {
    format!("$package:{}:{name}", package.0)
}

struct Qualifier {
    package: PackageId,
    functions: HashSet<String>,
    structures: HashSet<String>,
    ids: HashMap<BindingId, BindingId>,
    next: u32,
}
fn qualify(hir: &mut HirProgram, package: PackageId, next: &mut u32) {
    let mut q = Qualifier {
        package,
        functions: HashSet::new(),
        structures: HashSet::new(),
        ids: HashMap::new(),
        next: *next,
    };
    for declaration in &hir.declarations {
        match declaration {
            HirDeclaration::Function(f) => {
                q.functions.insert(f.name.clone());
            }
            HirDeclaration::Import(i) => {
                q.functions.insert(i.name.clone());
            }
            HirDeclaration::Struct(s) => {
                q.structures.insert(s.name.clone());
            }
            _ => {}
        }
    }
    for declaration in &mut hir.declarations {
        match declaration {
            HirDeclaration::Function(f) => {
                f.name = symbol(package, &f.name);
                for p in &mut f.parameters {
                    q.parameter(p);
                }
                q.ty(&mut f.return_type);
                q.block(&mut f.body);
            }
            HirDeclaration::Import(i) => {
                i.name = symbol(package, &i.name);
                for p in &mut i.parameters {
                    q.ty(p);
                }
                q.ty(&mut i.return_type);
            }
            HirDeclaration::Struct(s) => {
                s.name = symbol(package, &s.name);
                for f in &mut s.fields {
                    q.ty(&mut f.ty);
                }
            }
            HirDeclaration::Statement(s) => q.statement(s),
        }
    }
    *next = q.next;
}
impl Qualifier {
    fn id(&mut self, id: &mut BindingId) {
        *id = *self.ids.entry(*id).or_insert_with(|| {
            let new = BindingId(self.next);
            self.next += 1;
            new
        });
    }
    fn ty(&self, ty: &mut ResolvedType) {
        match ty {
            ResolvedType::Struct(name) if self.structures.contains(name) => {
                *name = symbol(self.package, name)
            }
            ResolvedType::Array(t) | ResolvedType::Optional(t) => self.ty(t),
            ResolvedType::Result(a, b) => {
                self.ty(a);
                self.ty(b);
            }
            ResolvedType::Closure(params, ret) => {
                for p in params {
                    self.ty(p);
                }
                self.ty(ret);
            }
            _ => {}
        }
    }
    fn parameter(&mut self, p: &mut HirParameter) {
        self.id(&mut p.id);
        self.ty(&mut p.ty);
    }
    fn block(&mut self, b: &mut HirBlock) {
        for s in &mut b.statements {
            self.statement(s);
        }
    }
    fn branch(&mut self, b: &mut HirIf) {
        self.expr(&mut b.condition);
        self.block(&mut b.then_branch);
        if let Some(e) = &mut b.else_branch {
            match e {
                HirElseBranch::Block(b) => self.block(b),
                HirElseBranch::ElseIf(b) => self.branch(b),
            }
        }
    }
    fn statement(&mut self, s: &mut HirStatement) {
        match s {
            HirStatement::Variable(v) => {
                self.id(&mut v.id);
                self.ty(&mut v.ty);
                self.expr(&mut v.initializer);
            }
            HirStatement::Assignment(a) => {
                self.expr(&mut a.target);
                self.expr(&mut a.value);
            }
            HirStatement::If(b) => self.branch(b),
            HirStatement::While(w) => {
                self.expr(&mut w.condition);
                self.block(&mut w.body);
            }
            HirStatement::Return(r) => {
                if let Some(v) = &mut r.value {
                    self.expr(v);
                }
            }
            HirStatement::Expression(e) => self.expr(&mut e.expression),
        }
    }
    fn expr(&mut self, e: &mut HirExpr) {
        self.ty(&mut e.ty);
        match &mut e.kind {
            HirExprKind::Local { id, .. } => self.id(id),
            HirExprKind::Closure {
                captures,
                parameters,
                body,
            } => {
                for (id, e) in captures {
                    self.id(id);
                    self.expr(e);
                }
                for p in parameters {
                    self.parameter(p);
                }
                self.block(body);
            }
            HirExprKind::ClosureCall { callee, arguments } => {
                self.expr(callee);
                for a in arguments {
                    self.expr(a);
                }
            }
            HirExprKind::OptionSome(e)
            | HirExprKind::Grouping(e)
            | HirExprKind::Move(e)
            | HirExprKind::Clone { expression: e, .. }
            | HirExprKind::Unary { operand: e, .. } => self.expr(e),
            HirExprKind::Coalesce {
                option, fallback, ..
            } => {
                self.expr(option);
                self.expr(fallback);
            }
            HirExprKind::StructLiteral {
                struct_name,
                fields,
            } => {
                if self.structures.contains(struct_name) {
                    *struct_name = symbol(self.package, struct_name);
                }
                for f in fields {
                    self.expr(&mut f.value);
                }
            }
            HirExprKind::ArrayLiteral(a) => {
                for e in a {
                    self.expr(e);
                }
            }
            HirExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            HirExprKind::FieldAccess { object, .. } => self.expr(object),
            HirExprKind::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            HirExprKind::Call {
                function,
                arguments,
            } => {
                if self.functions.contains(function) {
                    *function = symbol(self.package, function);
                }
                for a in arguments {
                    self.expr(&mut a.expression);
                }
            }
            HirExprKind::IntegerLiteral(_)
            | HirExprKind::NumberLiteral(_)
            | HirExprKind::StringLiteral(_)
            | HirExprKind::BooleanLiteral(_)
            | HirExprKind::OptionNone => {}
        }
    }
}
