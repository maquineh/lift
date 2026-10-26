//! Name/type resolution and conservative ownership inference from AST to HIR.
//!
//! PT-BR: Resolução de nomes/tipos e inferência conservadora de ownership de AST para HIR.

use std::collections::{HashMap, HashSet};

use lift_ast::{
    ArrayLiteral, AssignmentStatement, BinaryOperator, BindingKind, Block, Declaration,
    ElseBranchKind, Expression, ExpressionKind, FunctionDeclaration, Identifier, ProgramAst, Span,
    Statement, TypeExpression, TypeExpressionKind, UnaryOperator,
};

use crate::{
    ArgPassMode, BindingId, CallTempId, CloneMode, CoalesceMode, ExprDiscardMode, HirAssignment,
    HirBlock, HirCallArgument, HirDeclaration, HirElseBranch, HirExpr, HirExprKind,
    HirExpressionStatement, HirFunction, HirIf, HirImport, HirParameter, HirProgram, HirReturn,
    HirStatement, HirStruct, HirStructField, HirStructInitField, HirVariableDeclaration, HirWhile,
    ParameterPassMode, ResolvedType, RustDependency, SemaError, SemaErrorKind, CONSOLE_LOG_BUILTIN,
    OPTION_IS_SOME_BUILTIN, OPTION_UNWRAP_BUILTIN, RESULT_IS_OK_BUILTIN, RESULT_UNWRAP_BUILTIN,
    RESULT_UNWRAP_ERR_BUILTIN,
};

fn method_name(target: &str, method: &str) -> String {
    format!("$method:{target}:{method}")
}

fn lower_impls(mut ast: ProgramAst) -> Result<ProgramAst, Vec<SemaError>> {
    let names = ast
        .declarations
        .iter()
        .filter_map(|d| {
            if let Declaration::Interface(i) = d {
                Some(i.name.name.clone())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>();
    let mut declarations = Vec::new();
    for declaration in ast.declarations {
        if let Declaration::Impl(implementation) = declaration {
            if !names.contains(&implementation.target.name) {
                return Err(vec![SemaError::new(
                    SemaErrorKind::UnknownType,
                    "impl exige uma interface nominal existente",
                    implementation.target.span,
                )]);
            }
            for mut method in implementation.methods {
                method.name.name = method_name(&implementation.target.name, &method.name.name);
                method.parameters.insert(
                    0,
                    lift_ast::Parameter {
                        name: Identifier {
                            name: "this".into(),
                            span: implementation.target.span,
                        },
                        ty: TypeExpression {
                            kind: TypeExpressionKind::Named(implementation.target.clone()),
                            span: implementation.target.span,
                        },
                        span: implementation.target.span,
                    },
                );
                declarations.push(Declaration::Function(method));
            }
        } else {
            declarations.push(declaration);
        }
    }
    ast.declarations = declarations;
    Ok(ast)
}

// Lexical free names: nested arrows contribute only their actual free names.
fn closure_free_names(closure: &lift_ast::ClosureExpression) -> HashSet<String> {
    fn expression(e: &Expression, bound: &HashSet<String>, free: &mut HashSet<String>) {
        match &e.kind {
            ExpressionKind::Identifier(i) => {
                if !bound.contains(&i.name) {
                    free.insert(i.name.clone());
                }
            }
            ExpressionKind::Closure(c) => {
                for name in closure_free_names(c) {
                    if !bound.contains(&name) {
                        free.insert(name);
                    }
                }
            }
            ExpressionKind::Template(parts) => {
                for e in parts {
                    expression(e, bound, free);
                }
            }
            ExpressionKind::ObjectLiteral(o) => {
                for f in &o.fields {
                    expression(&f.value, bound, free);
                }
            }
            ExpressionKind::ArrayLiteral(a) => {
                for e in &a.elements {
                    expression(e, bound, free);
                }
            }
            ExpressionKind::Unary(u) => expression(&u.operand, bound, free),
            ExpressionKind::Binary(b) => {
                expression(&b.left, bound, free);
                expression(&b.right, bound, free);
            }
            ExpressionKind::FieldAccess(f) => expression(&f.object, bound, free),
            ExpressionKind::Index(i) => {
                expression(&i.object, bound, free);
                expression(&i.index, bound, free);
            }
            ExpressionKind::Grouping(e) => expression(e, bound, free),
            ExpressionKind::Call(c) => {
                expression(&c.callee, bound, free);
                for a in &c.arguments {
                    expression(a, bound, free);
                }
            }
            _ => {}
        }
    }
    fn conditional(i: &lift_ast::IfStatement, bound: &HashSet<String>, free: &mut HashSet<String>) {
        expression(&i.condition, bound, free);
        block(&i.then_branch, &mut bound.clone(), free);
        if let Some(branch) = &i.else_branch {
            match &branch.kind {
                ElseBranchKind::Block(b) => block(b, &mut bound.clone(), free),
                ElseBranchKind::ElseIf(i) => conditional(i, bound, free),
            }
        }
    }
    fn block(b: &Block, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
        for statement in &b.statements {
            match statement {
                Statement::Variable(v) => {
                    expression(&v.initializer, bound, free);
                    bound.insert(v.name.name.clone());
                }
                Statement::Assignment(a) => {
                    expression(&a.target, bound, free);
                    expression(&a.value, bound, free);
                }
                Statement::Expression(e) => expression(&e.expression, bound, free),
                Statement::Return(r) => {
                    if let Some(e) = &r.value {
                        expression(e, bound, free);
                    }
                }
                Statement::While(w) => {
                    expression(&w.condition, bound, free);
                    block(&w.body, &mut bound.clone(), free);
                }
                Statement::If(i) => conditional(i, bound, free),
            }
        }
    }
    let mut bound = closure
        .parameters
        .iter()
        .map(|p| p.name.name.clone())
        .collect();
    let mut free = HashSet::new();
    block(&closure.body, &mut bound, &mut free);
    free
}

#[derive(Clone)]
struct StructInfo {
    fields: Vec<HirStructField>,
    span: Span,
}

#[derive(Clone)]
pub(super) struct ParameterInfo {
    pub(super) name: String,
    pub(super) ty: ResolvedType,
    pub(super) span: Span,
    pub(super) pass_mode: ParameterPassMode,
    pub(super) mutated: bool,
}

#[derive(Clone)]
pub(super) struct FunctionInfo {
    pub(super) name: String,
    pub(super) parameters: Vec<ParameterInfo>,
    pub(super) return_type: ResolvedType,
    pub(super) has_explicit_return_type: bool,
    pub(super) span: Span,
}

#[derive(Clone)]
struct BindingInfo {
    id: BindingId,
    ty: ResolvedType,
    binding: Option<BindingKind>,
    parameter_mode: Option<ParameterPassMode>,
    parameter_mutated: bool,
    span: Span,
}

struct Analyzer<'ast> {
    ast: &'ast ProgramAst,
    structures: HashMap<String, StructInfo>,
    functions: HashMap<String, FunctionInfo>,
    struct_by_declaration: HashMap<usize, HirStruct>,
    function_by_declaration: HashMap<usize, FunctionInfo>,
    import_by_declaration: HashMap<usize, HirImport>,
    errors: Vec<SemaError>,
    next_binding_id: u32,
    mutable_bindings: HashSet<BindingId>,
}

/// Constrói HIR tipado. Um `Ok` nunca contém tipo desconhecido nem recurso
/// da AST que ainda não tenha suporte semântico.
pub fn analyze(ast: ProgramAst) -> Result<HirProgram, Vec<SemaError>> {
    analyze_context(ast, HashMap::new())
}

pub(super) fn analyze_context(
    ast: ProgramAst,
    external: HashMap<String, FunctionInfo>,
) -> Result<HirProgram, Vec<SemaError>> {
    let ast = lower_impls(ast)?;
    let unresolved = ast
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            Declaration::ModuleImport(import) => Some(SemaError::new(
                SemaErrorKind::InvalidImport,
                "imports de módulo devem ser resolvidos antes da análise semântica",
                import.span,
            )),
            Declaration::PackageImport(_) | Declaration::ExportFunction(_) => Some(SemaError::new(
                SemaErrorKind::InvalidPackage,
                "imports/exports de pacote exigem contexto de projeto manifestado",
                declaration.span(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        return Err(unresolved);
    }
    let mut analyzer = Analyzer::new(&ast);
    analyzer.functions = external;
    analyzer.run()
}

impl<'ast> Analyzer<'ast> {
    fn new(ast: &'ast ProgramAst) -> Self {
        Self {
            ast,
            structures: HashMap::new(),
            functions: HashMap::new(),
            struct_by_declaration: HashMap::new(),
            function_by_declaration: HashMap::new(),
            import_by_declaration: HashMap::new(),
            errors: Vec::new(),
            next_binding_id: 0,
            mutable_bindings: HashSet::new(),
        }
    }

    fn run(mut self) -> Result<HirProgram, Vec<SemaError>> {
        self.collect_struct_names();
        self.resolve_structs();
        self.collect_functions();
        self.collect_imports();
        self.reject_recursive_value_types();
        self.infer_parameter_modes();

        let mut functions_by_declaration = HashMap::<usize, HirFunction>::new();
        let function_declarations = self
            .ast
            .declarations
            .iter()
            .enumerate()
            .filter_map(|(index, declaration)| match declaration {
                Declaration::Function(function) => Some((index, function.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (index, function) in function_declarations {
            if let Some(signature) = self.function_by_declaration.get(&index).cloned() {
                match self.check_function(&function, signature) {
                    Ok(function) => {
                        functions_by_declaration.insert(index, function);
                    }
                    Err(error) => self.errors.push(error),
                }
            }
        }

        let mut statements_by_declaration = HashMap::<usize, HirStatement>::new();
        let mut top_level_errors = Vec::new();
        {
            let mut top_level = BodyChecker::new(
                &self.structures,
                &self.functions,
                &mut self.next_binding_id,
                &mut self.mutable_bindings,
                None,
            );
            for (index, declaration) in self.ast.declarations.iter().enumerate() {
                if let Declaration::Statement(statement) = declaration {
                    match top_level.check_statement(statement, None) {
                        Ok(statement) => {
                            statements_by_declaration.insert(index, statement);
                        }
                        Err(error) => top_level_errors.push(error),
                    }
                }
            }
        }
        self.errors.extend(top_level_errors);

        let mut declarations = Vec::with_capacity(self.ast.declarations.len());
        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            match declaration {
                Declaration::ModuleImport(_) => {}
                Declaration::PackageImport(_) | Declaration::ExportFunction(_) => {
                    unreachable!("package syntax resolved before analysis")
                }
                Declaration::Impl(_) => unreachable!("impl lowered before analysis"),
                Declaration::Import(_) => {
                    if let Some(item) = self.import_by_declaration.get(&index) {
                        declarations.push(HirDeclaration::Import(item.clone()));
                    }
                }
                Declaration::Interface(_) => {
                    if let Some(item) = self.struct_by_declaration.get(&index) {
                        declarations.push(HirDeclaration::Struct(item.clone()));
                    }
                }
                Declaration::Function(_) => {
                    if let Some(item) = functions_by_declaration.remove(&index) {
                        declarations.push(HirDeclaration::Function(item));
                    }
                }
                Declaration::Statement(_) => {
                    if let Some(item) = statements_by_declaration.remove(&index) {
                        declarations.push(HirDeclaration::Statement(item));
                    }
                }
            }
        }

        apply_mutable_binding_flags(&mut declarations, &self.mutable_bindings);

        if self.errors.is_empty() {
            let mut program = HirProgram { declarations };
            if crate::resources::uses_db_runtime(&program) {
                program.declarations.push(HirDeclaration::Import(HirImport {
                    name: "$resource_dependency".into(),
                    parameters: vec![],
                    parameter_modes: vec![],
                    return_type: ResolvedType::Void,
                    dependency: RustDependency::LocalDb,
                    rust_path: "lift_db::resource::available".into(),
                    span: Span::default(),
                }));
            }
            if crate::resources::uses_http(&program) {
                program.declarations.push(HirDeclaration::Import(HirImport {
                    name: "$http_dependency".into(),
                    parameters: vec![],
                    parameter_modes: vec![],
                    return_type: ResolvedType::Void,
                    dependency: RustDependency::LocalWeb,
                    rust_path: "lift_web::http::available".into(),
                    span: Span::default(),
                }));
            }
            crate::liveness::optimize(&mut program);
            crate::resources::validate(&program)?;
            Ok(program)
        } else {
            Err(self.errors)
        }
    }

    fn collect_struct_names(&mut self) {
        let mut declared = HashMap::<String, Span>::new();
        for declaration in &self.ast.declarations {
            let Declaration::Interface(interface) = declaration else {
                continue;
            };
            let name = interface.name.name.clone();
            if matches!(
                name.as_str(),
                "SqliteConnection"
                    | "SqliteTransaction"
                    | "DbValue"
                    | "DbPool"
                    | "DbTransaction"
                    | "DbRows"
            ) {
                self.errors.push(SemaError::new(
                    SemaErrorKind::DuplicateDeclaration,
                    "nome reservado para recurso opaco SQLite",
                    interface.name.span,
                ));
            }
            if let Some(previous) = declared.insert(name.clone(), interface.name.span) {
                self.errors.push(
                    SemaError::new(
                        SemaErrorKind::DuplicateDeclaration,
                        format!("declaração duplicada de `{name}`"),
                        interface.name.span,
                    )
                    .with_secondary(previous),
                );
                continue;
            }
        }
    }

    fn resolve_structs(&mut self) {
        let mut names = HashSet::new();
        for declaration in &self.ast.declarations {
            if let Declaration::Interface(interface) = declaration {
                names.insert(interface.name.name.clone());
            }
        }

        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            let Declaration::Interface(interface) = declaration else {
                continue;
            };
            let mut fields = Vec::new();
            let mut field_names = HashMap::<String, Span>::new();
            let mut valid = true;
            for field in &interface.fields {
                if let Some(previous) = field_names.insert(field.name.name.clone(), field.name.span)
                {
                    self.errors.push(
                        SemaError::new(
                            SemaErrorKind::DuplicateField,
                            format!("campo duplicado `{}`", field.name.name),
                            field.name.span,
                        )
                        .with_secondary(previous),
                    );
                    valid = false;
                    continue;
                }
                match resolve_type(&field.ty, &names, TypePosition::Field) {
                    Ok(ty) => fields.push(HirStructField {
                        name: field.name.name.clone(),
                        ty,
                        span: field.span,
                    }),
                    Err(error) => {
                        self.errors.push(error);
                        valid = false;
                    }
                }
            }
            if valid {
                let info = StructInfo {
                    fields: fields.clone(),
                    span: interface.span,
                };
                self.structures.insert(interface.name.name.clone(), info);
                self.struct_by_declaration.insert(
                    index,
                    HirStruct {
                        name: interface.name.name.clone(),
                        fields,
                        span: interface.span,
                    },
                );
            }
        }
    }

    fn collect_functions(&mut self) {
        let mut all_names = HashMap::<String, Span>::new();
        for declaration in &self.ast.declarations {
            match declaration {
                Declaration::Interface(lift_ast::InterfaceDeclaration {
                    name: identifier, ..
                })
                | Declaration::Function(FunctionDeclaration {
                    name: identifier, ..
                })
                | Declaration::Import(lift_ast::ImportFunctionDeclaration {
                    name: identifier,
                    ..
                }) => {
                    let name = identifier.name.clone();
                    if let Some(previous) = all_names.insert(name.clone(), identifier.span) {
                        self.errors.push(
                            SemaError::new(
                                SemaErrorKind::DuplicateDeclaration,
                                format!("declaração duplicada de `{name}`"),
                                identifier.span,
                            )
                            .with_secondary(previous),
                        );
                    }
                }
                Declaration::Statement(_) | Declaration::ModuleImport(_) => {}
                Declaration::PackageImport(_) | Declaration::ExportFunction(_) => {
                    unreachable!("package syntax resolved before analysis")
                }
                Declaration::Impl(_) => unreachable!("impl lowered before analysis"),
            }
        }

        let struct_names = self
            .structures
            .keys()
            .cloned()
            .chain(["int", "number", "string", "boolean", "void"].map(str::to_owned))
            .collect::<HashSet<_>>();

        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            let Declaration::Function(function) = declaration else {
                continue;
            };
            let mut parameters = Vec::new();
            let mut seen = HashMap::<String, Span>::new();
            let mut valid = true;
            for parameter in &function.parameters {
                if let Some(previous) =
                    seen.insert(parameter.name.name.clone(), parameter.name.span)
                {
                    self.errors.push(
                        SemaError::new(
                            SemaErrorKind::DuplicateParameter,
                            format!("parâmetro duplicado `{}`", parameter.name.name),
                            parameter.name.span,
                        )
                        .with_secondary(previous),
                    );
                    valid = false;
                    continue;
                }
                match resolve_type(&parameter.ty, &struct_names, TypePosition::Parameter) {
                    Ok(ty) => parameters.push(ParameterInfo {
                        name: parameter.name.name.clone(),
                        ty,
                        span: parameter.span,
                        pass_mode: ParameterPassMode::ByValue,
                        mutated: false,
                    }),
                    Err(error) => {
                        self.errors.push(error);
                        valid = false;
                    }
                }
            }

            let has_explicit_return_type = function.return_type.is_some();
            let return_type = match &function.return_type {
                Some(ty) => match resolve_type(ty, &struct_names, TypePosition::Return) {
                    Ok(ty) => ty,
                    Err(error) => {
                        self.errors.push(error);
                        valid = false;
                        ResolvedType::Void
                    }
                },
                None => ResolvedType::Void,
            };

            if valid {
                let info = FunctionInfo {
                    name: function.name.name.clone(),
                    parameters,
                    return_type,
                    has_explicit_return_type,
                    span: function.span,
                };
                self.function_by_declaration.insert(index, info.clone());
                self.functions.insert(function.name.name.clone(), info);
            }
        }
    }

    fn reject_recursive_value_types(&mut self) {
        let mut state = HashMap::<String, u8>::new();
        let mut stack = Vec::<String>::new();
        let names = self.structures.keys().cloned().collect::<Vec<_>>();
        for name in names {
            if let Some(cycle_span) = visit_struct(&name, &self.structures, &mut state, &mut stack)
            {
                self.errors.push(SemaError::new(
                    SemaErrorKind::RecursiveValueType,
                    format!(
                        "tipo recursivo `{}` tem tamanho infinito; use um array para introduzir indireção",
                        stack.last().cloned().unwrap_or(name)
                    ),
                    cycle_span,
                ));
                break;
            }
        }
    }

    fn collect_imports(&mut self) {
        let names = self.structures.keys().cloned().collect::<HashSet<_>>();
        let mut dependencies = HashMap::<String, (RustDependency, Span)>::new();
        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            let Declaration::Import(import) = declaration else {
                continue;
            };
            let result = (|| -> Result<(HirImport, FunctionInfo), SemaError> {
                let (dependency, rust_path) =
                    crate::interop::resolve_source(&import.source, import.source_span)?;
                let resource_signature = crate::interop::resource_signature(&rust_path);
                let mut seen = HashMap::new();
                let mut parameters = Vec::new();
                for parameter in &import.parameters {
                    if let Some(previous) = seen.insert(&parameter.name.name, parameter.name.span) {
                        return Err(SemaError::new(
                            SemaErrorKind::DuplicateParameter,
                            format!("parâmetro duplicado `{}`", parameter.name.name),
                            parameter.name.span,
                        )
                        .with_secondary(previous));
                    }
                    let ty = resolve_type(&parameter.ty, &names, TypePosition::Parameter)?;
                    if resource_signature.is_none() {
                        crate::interop::validate_type(&ty, false, parameter.ty.span)?;
                    }
                    let pass_mode = if ty.is_copy() {
                        ParameterPassMode::ByValue
                    } else {
                        ParameterPassMode::BorrowImm
                    };
                    parameters.push(ParameterInfo {
                        name: parameter.name.name.clone(),
                        ty,
                        span: parameter.span,
                        pass_mode,
                        mutated: false,
                    });
                }
                let return_type = resolve_type(&import.return_type, &names, TypePosition::Return)?;
                if let Some((expected, output)) = &resource_signature {
                    if expected
                        .iter()
                        .map(|p| &p.0)
                        .ne(parameters.iter().map(|p| &p.ty))
                        || *output != return_type
                    {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidImport,
                            "assinatura incompatível com o contrato fechado de recursos SQLite",
                            import.span,
                        ));
                    }
                    for (parameter, (_, mode)) in parameters.iter_mut().zip(expected) {
                        parameter.pass_mode = *mode;
                        parameter.mutated = *mode == ParameterPassMode::BorrowMut;
                    }
                } else {
                    crate::interop::validate_type(&return_type, true, import.return_type.span)?;
                }
                let crate_name = rust_path
                    .split("::")
                    .next()
                    .expect("validated Rust path")
                    .to_owned();
                if let Some((previous, previous_span)) = dependencies.get(&crate_name) {
                    if previous != &dependency {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidImport,
                            format!("dependências incompatíveis para a crate `{crate_name}`"),
                            import.source_span,
                        )
                        .with_secondary(*previous_span));
                    }
                } else {
                    dependencies.insert(crate_name, (dependency.clone(), import.source_span));
                }
                let hir = HirImport {
                    name: import.name.name.clone(),
                    parameters: parameters.iter().map(|p| p.ty.clone()).collect(),
                    parameter_modes: parameters.iter().map(|p| p.pass_mode).collect(),
                    return_type: return_type.clone(),
                    dependency,
                    rust_path,
                    span: import.span,
                };
                let signature = FunctionInfo {
                    name: import.name.name.clone(),
                    parameters,
                    return_type,
                    has_explicit_return_type: true,
                    span: import.span,
                };
                Ok((hir, signature))
            })();
            match result {
                Ok((hir, signature)) => {
                    self.functions.insert(signature.name.clone(), signature);
                    self.import_by_declaration.insert(index, hir);
                }
                Err(error) => self.errors.push(error),
            }
        }
    }

    fn infer_parameter_modes(&mut self) {
        let function_decls = self
            .ast
            .declarations
            .iter()
            .enumerate()
            .filter_map(|(index, declaration)| match declaration {
                Declaration::Function(function) => Some((index, function)),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (index, declaration) in &function_decls {
            let Some(signature) = self.function_by_declaration.get_mut(index) else {
                continue;
            };
            for parameter in &mut signature.parameters {
                if parameter.ty.is_copy() {
                    parameter.pass_mode = ParameterPassMode::ByValue;
                    continue;
                }
                let mutated = block_mutates_name(&declaration.body, &parameter.name);
                let escapes = !(signature.name.starts_with("$method:") && parameter.name == "this")
                    && block_returns_name(&declaration.body, &parameter.name);
                parameter.mutated = mutated;
                parameter.pass_mode = if escapes {
                    ParameterPassMode::OwnedMove
                } else if mutated {
                    ParameterPassMode::BorrowMut
                } else {
                    ParameterPassMode::BorrowImm
                };
            }
        }

        // Propaga mutação através de chamadas, inclusive chamadas recursivas.
        // A promoção é monotônica: BorrowImm -> BorrowMut; OwnedMove permanece
        // owned e registra mutabilidade local quando o corpo a requer.
        loop {
            let modes = self
                .function_by_declaration
                .iter()
                .map(|(index, info)| (*index, info.clone()))
                .collect::<HashMap<_, _>>();
            let mut by_name = self.functions.clone();
            by_name.extend(
                modes
                    .values()
                    .map(|info| (info.name.clone(), info.clone()))
                    .collect::<HashMap<_, _>>(),
            );
            let mut promotions = Vec::<(usize, String)>::new();

            for (index, declaration) in &function_decls {
                let Some(caller) = modes.get(index) else {
                    continue;
                };
                let mut needs_mut = HashSet::new();
                collect_forwarded_mut_parameters(
                    &declaration.body,
                    caller,
                    &by_name,
                    &mut needs_mut,
                );
                promotions.extend(needs_mut.into_iter().map(|name| (*index, name)));
            }

            let mut changed = false;
            for (index, name) in promotions {
                if let Some(caller) = self.function_by_declaration.get_mut(&index) {
                    if let Some(parameter) = caller.parameters.iter_mut().find(|p| p.name == name) {
                        parameter.mutated = true;
                        if parameter.pass_mode == ParameterPassMode::BorrowImm {
                            parameter.pass_mode = ParameterPassMode::BorrowMut;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        self.functions.extend(
            self.function_by_declaration
                .values()
                .map(|info| (info.name.clone(), info.clone()))
                .collect::<HashMap<_, _>>(),
        );
    }

    fn check_function(
        &mut self,
        declaration: &FunctionDeclaration,
        signature: FunctionInfo,
    ) -> Result<HirFunction, SemaError> {
        let mut checker = BodyChecker::new(
            &self.structures,
            &self.functions,
            &mut self.next_binding_id,
            &mut self.mutable_bindings,
            Some(FunctionContext {
                return_type: signature.return_type.clone(),
                has_explicit_return_type: signature.has_explicit_return_type,
            }),
        );

        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            let id = checker.fresh_binding_id();
            checker.declare_parameter(
                &parameter.name,
                BindingInfo {
                    id,
                    ty: parameter.ty.clone(),
                    binding: None,
                    parameter_mode: Some(parameter.pass_mode),
                    parameter_mutated: parameter.mutated,
                    span: parameter.span,
                },
            )?;
            parameters.push(HirParameter {
                id,
                name: parameter.name.clone(),
                ty: parameter.ty.clone(),
                pass_mode: parameter.pass_mode,
                requires_mut_binding: parameter.pass_mode == ParameterPassMode::OwnedMove
                    && parameter.mutated,
                span: parameter.span,
            });
        }

        let body = checker.check_block(&declaration.body, true)?;
        if signature.return_type != ResolvedType::Void
            && !block_guarantees_return(&declaration.body)
        {
            return Err(SemaError::new(
                SemaErrorKind::MissingReturnPath,
                format!(
                    "a função `{}` pode terminar sem retornar `{}`",
                    signature.name, signature.return_type
                ),
                declaration.body.span,
            ));
        }

        Ok(HirFunction {
            name: signature.name,
            parameters,
            return_type: signature.return_type,
            body,
            span: signature.span,
        })
    }
}

#[derive(Clone)]
struct FunctionContext {
    return_type: ResolvedType,
    has_explicit_return_type: bool,
}

struct BodyChecker<'a> {
    structures: &'a HashMap<String, StructInfo>,
    functions: &'a HashMap<String, FunctionInfo>,
    next_binding_id: &'a mut u32,
    mutable_bindings: &'a mut HashSet<BindingId>,
    scopes: Vec<HashMap<String, BindingInfo>>,
    function: Option<FunctionContext>,
}

impl<'a> BodyChecker<'a> {
    fn new(
        structures: &'a HashMap<String, StructInfo>,
        functions: &'a HashMap<String, FunctionInfo>,
        next_binding_id: &'a mut u32,
        mutable_bindings: &'a mut HashSet<BindingId>,
        function: Option<FunctionContext>,
    ) -> Self {
        Self {
            structures,
            functions,
            next_binding_id,
            mutable_bindings,
            scopes: vec![HashMap::new()],
            function,
        }
    }

    fn fresh_binding_id(&mut self) -> BindingId {
        let id = BindingId(*self.next_binding_id);
        *self.next_binding_id = self.next_binding_id.saturating_add(1);
        id
    }

    fn declare_parameter(&mut self, name: &str, info: BindingInfo) -> Result<(), SemaError> {
        self.declare(name, info)
    }

    fn declare(&mut self, name: &str, info: BindingInfo) -> Result<(), SemaError> {
        if self
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
        {
            return Err(SemaError::new(
                SemaErrorKind::DuplicateDeclaration,
                format!("`{name}` já está declarado em um escopo ativo"),
                info.span,
            ));
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_owned(), info);
        }
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<&BindingInfo> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn check_block(&mut self, block: &Block, create_scope: bool) -> Result<HirBlock, SemaError> {
        if create_scope {
            self.scopes.push(HashMap::new());
        }
        let function = self.function.clone();
        let statements = block
            .statements
            .iter()
            .map(|statement| self.check_statement(statement, function.as_ref()))
            .collect::<Result<Vec<_>, _>>();
        if create_scope {
            self.scopes.pop();
        }
        Ok(HirBlock {
            statements: statements?,
            span: block.span,
        })
    }

    fn check_statement(
        &mut self,
        statement: &Statement,
        function: Option<&FunctionContext>,
    ) -> Result<HirStatement, SemaError> {
        match statement {
            Statement::Variable(variable) => {
                if self.lookup(&variable.name.name).is_some() {
                    return Err(SemaError::new(
                        SemaErrorKind::DuplicateDeclaration,
                        format!(
                            "`{}` já está declarado em um escopo ativo",
                            variable.name.name
                        ),
                        variable.name.span,
                    ));
                }
                let expected = variable
                    .annotation
                    .as_ref()
                    .map(|ty| self.resolve_local_type(ty, TypePosition::Variable))
                    .transpose()?;
                let initializer = match expected.as_ref() {
                    Some(ty) => self.check_expr(&variable.initializer, ty)?,
                    None => self.synthesize_expr(&variable.initializer)?,
                };
                let is_closure = matches!(initializer.ty, ResolvedType::Closure(..));
                if is_closure
                    && (variable.binding != BindingKind::Const
                        || !matches!(variable.initializer.kind, ExpressionKind::Closure(_)))
                {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "closures exigem declaração const direta e não podem ser copiadas",
                        variable.span,
                    ));
                }
                if !initializer.ty.is_value_type() && !is_closure {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        "uma variável não pode ter tipo `void`",
                        variable.initializer.span,
                    ));
                }
                let ty = expected.unwrap_or_else(|| initializer.ty.clone());
                let initializer = self.materialize_owned(initializer, false);
                let id = self.fresh_binding_id();
                self.declare(
                    &variable.name.name,
                    BindingInfo {
                        id,
                        ty: ty.clone(),
                        binding: Some(variable.binding),
                        parameter_mode: None,
                        parameter_mutated: false,
                        span: variable.name.span,
                    },
                )?;
                Ok(HirStatement::Variable(HirVariableDeclaration {
                    id,
                    binding: variable.binding,
                    name: variable.name.name.clone(),
                    ty,
                    initializer,
                    requires_mut_binding: false,
                    span: variable.span,
                }))
            }
            Statement::Assignment(assignment) => self.check_assignment(assignment),
            Statement::If(statement) => {
                let condition = self.check_expr(&statement.condition, &ResolvedType::Boolean)?;
                let then_branch = self.check_block(&statement.then_branch, true)?;
                let else_branch = statement
                    .else_branch
                    .as_ref()
                    .map(|branch| match &branch.kind {
                        ElseBranchKind::Block(block) => {
                            self.check_block(block, true).map(HirElseBranch::Block)
                        }
                        ElseBranchKind::ElseIf(if_statement) => self
                            .check_if(if_statement)
                            .map(|statement| HirElseBranch::ElseIf(Box::new(statement))),
                    })
                    .transpose()?;
                Ok(HirStatement::If(HirIf {
                    condition,
                    then_branch,
                    else_branch,
                    span: statement.span,
                }))
            }
            Statement::While(statement) => {
                let condition = self.check_expr(&statement.condition, &ResolvedType::Boolean)?;
                let body = self.check_block(&statement.body, true)?;
                Ok(HirStatement::While(HirWhile {
                    condition,
                    body,
                    span: statement.span,
                }))
            }
            Statement::Return(statement) => {
                let Some(function) = function else {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidReturn,
                        "`return` só pode aparecer dentro de uma função",
                        statement.span,
                    ));
                };
                let value = match (&statement.value, &function.return_type) {
                    (None, ResolvedType::Void) => None,
                    (None, expected) => {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidReturn,
                            format!("esperado retorno `{expected}`, mas `return` não tem valor"),
                            statement.span,
                        ));
                    }
                    (Some(_), ResolvedType::Void) if !function.has_explicit_return_type => {
                        return Err(SemaError::new(
                            SemaErrorKind::ReturnTypeRequired,
                            "função que retorna valor precisa declarar o tipo de retorno",
                            statement.span,
                        ));
                    }
                    (Some(_), ResolvedType::Void) => {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidReturn,
                            "função `void` não pode retornar um valor",
                            statement.span,
                        ));
                    }
                    (Some(expression), expected) => {
                        let value = self.check_expr(expression, expected)?;
                        Some(self.materialize_owned(value, true))
                    }
                };
                Ok(HirStatement::Return(HirReturn {
                    value,
                    span: statement.span,
                }))
            }
            Statement::Expression(statement) => {
                let expression = self.synthesize_expr(&statement.expression)?;
                let discard_mode = if !expression.ty.is_copy() && is_place(&expression) {
                    ExprDiscardMode::BorrowPlace
                } else {
                    ExprDiscardMode::Value
                };
                Ok(HirStatement::Expression(HirExpressionStatement {
                    expression,
                    discard_mode,
                    span: statement.span,
                }))
            }
        }
    }

    fn check_if(&mut self, statement: &lift_ast::IfStatement) -> Result<HirIf, SemaError> {
        let condition = self.check_expr(&statement.condition, &ResolvedType::Boolean)?;
        let then_branch = self.check_block(&statement.then_branch, true)?;
        let else_branch = statement
            .else_branch
            .as_ref()
            .map(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => {
                    self.check_block(block, true).map(HirElseBranch::Block)
                }
                ElseBranchKind::ElseIf(if_statement) => self
                    .check_if(if_statement)
                    .map(|statement| HirElseBranch::ElseIf(Box::new(statement))),
            })
            .transpose()?;
        Ok(HirIf {
            condition,
            then_branch,
            else_branch,
            span: statement.span,
        })
    }

    fn check_assignment(
        &mut self,
        assignment: &AssignmentStatement,
    ) -> Result<HirStatement, SemaError> {
        let (target, expected, root) = self.check_place(&assignment.target)?;
        if let Some(root) = root {
            let root_info = self.lookup_by_id(root).cloned();
            match root_info.as_ref() {
                Some(binding)
                    if binding.parameter_mode.is_some() && is_bare_place(&assignment.target) =>
                {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedOwnershipPattern,
                        "reatribuição direta de parâmetros não é suportada nesta fase",
                        assignment.target.span,
                    ));
                }
                Some(binding)
                    if binding.binding == Some(BindingKind::Const)
                        && is_bare_place(&assignment.target) =>
                {
                    return Err(SemaError::new(
                        SemaErrorKind::ImmutableBinding,
                        "`const` não pode receber uma nova atribuição",
                        assignment.target.span,
                    ));
                }
                Some(binding)
                    if !is_bare_place(&assignment.target)
                        && binding.parameter_mode == Some(ParameterPassMode::BorrowImm) =>
                {
                    return Err(SemaError::new(
                        SemaErrorKind::BorrowMutRequiresMutablePlace,
                        "não é possível mutar um parâmetro emprestado imutavelmente",
                        assignment.target.span,
                    ));
                }
                Some(_) => {
                    self.mutable_bindings.insert(root);
                }
                None => {}
            }
        } else if !is_bare_place(&assignment.target) {
            return Err(SemaError::new(
                SemaErrorKind::InvalidAssignmentTarget,
                "mutação de campo ou array exige uma variável local ou parâmetro como raiz",
                assignment.target.span,
            ));
        }
        let value = self.check_expr(&assignment.value, &expected)?;
        let value = self.materialize_owned(value, false);
        Ok(HirStatement::Assignment(HirAssignment {
            target,
            value,
            span: assignment.span,
        }))
    }

    fn check_place(
        &mut self,
        expression: &Expression,
    ) -> Result<(HirExpr, ResolvedType, Option<BindingId>), SemaError> {
        match &expression.kind {
            ExpressionKind::Identifier(identifier) => {
                let binding = self.lookup(&identifier.name).cloned().ok_or_else(|| {
                    SemaError::new(
                        SemaErrorKind::UnknownVariable,
                        format!("variável `{}` não foi declarada", identifier.name),
                        identifier.span,
                    )
                })?;
                Ok((
                    HirExpr {
                        kind: HirExprKind::Local {
                            id: binding.id,
                            name: identifier.name.clone(),
                        },
                        ty: binding.ty.clone(),
                        span: expression.span,
                    },
                    binding.ty,
                    Some(binding.id),
                ))
            }
            ExpressionKind::FieldAccess(access) => {
                let object = self.synthesize_expr(&access.object)?;
                let ResolvedType::Struct(struct_name) = &object.ty else {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!("acesso a campo exige struct, recebeu `{}`", object.ty),
                        access.object.span,
                    ));
                };
                let field = self
                    .structures
                    .get(struct_name)
                    .and_then(|info| {
                        info.fields
                            .iter()
                            .find(|field| field.name == access.field.name)
                    })
                    .ok_or_else(|| {
                        SemaError::new(
                            SemaErrorKind::UnknownField,
                            format!("`{struct_name}` não possui campo `{}`", access.field.name),
                            access.field.span,
                        )
                    })?;
                let ty = field.ty.clone();
                let root = root_binding(&object);
                Ok((
                    HirExpr {
                        kind: HirExprKind::FieldAccess {
                            object: Box::new(object),
                            field: access.field.name.clone(),
                        },
                        ty: ty.clone(),
                        span: expression.span,
                    },
                    ty,
                    root,
                ))
            }
            ExpressionKind::Index(index) => {
                let object = self.synthesize_expr(&index.object)?;
                let ResolvedType::Array(element) = object.ty.clone() else {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!("indexação exige array, recebeu `{}`", object.ty),
                        index.object.span,
                    ));
                };
                let index_expr = self.check_expr(&index.index, &ResolvedType::Int)?;
                let root = root_binding(&object);
                if let Some(root) = root {
                    let mut nested_mut_roots = HashSet::new();
                    collect_nested_mut_borrow_roots(&index_expr, &mut nested_mut_roots);
                    if nested_mut_roots.contains(&root)
                        || expression_reads_any_root(&index_expr, &[root])
                    {
                        return Err(SemaError::new(
                            SemaErrorKind::UnsupportedOwnershipPattern,
                            "índice lê ou muta a própria coleção; avalie-o em uma variável separada",
                            index.index.span,
                        ));
                    }
                }
                Ok((
                    HirExpr {
                        kind: HirExprKind::Index {
                            object: Box::new(object),
                            index: Box::new(index_expr),
                        },
                        ty: (*element).clone(),
                        span: expression.span,
                    },
                    *element,
                    root,
                ))
            }
            ExpressionKind::Grouping(inner) => self.check_place(inner),
            _ => Err(SemaError::new(
                SemaErrorKind::InvalidAssignmentTarget,
                "alvo de atribuição precisa ser variável, campo ou elemento de array",
                expression.span,
            )),
        }
    }

    fn lookup_by_id(&self, id: BindingId) -> Option<&BindingInfo> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.values())
            .find(|binding| binding.id == id)
    }

    fn resolve_local_type(
        &self,
        ty: &TypeExpression,
        position: TypePosition,
    ) -> Result<ResolvedType, SemaError> {
        let names = self
            .structures
            .keys()
            .cloned()
            .chain(["int", "number", "string", "boolean", "void"].map(str::to_owned))
            .collect::<HashSet<_>>();
        resolve_type(ty, &names, position)
    }

    fn check_expr(
        &mut self,
        expression: &Expression,
        expected: &ResolvedType,
    ) -> Result<HirExpr, SemaError> {
        if matches!(expression.kind, ExpressionKind::NullLiteral) {
            if matches!(expected, ResolvedType::Optional(_)) {
                return Ok(self.expr(HirExprKind::OptionNone, expected.clone(), expression.span));
            }
            return Err(SemaError::new(
                SemaErrorKind::TypeMismatch,
                "`null` exige contexto de tipo opcional T?",
                expression.span,
            ));
        }
        if let ExpressionKind::Call(call) = &expression.kind {
            if self.option_operation(call) == Some("some") {
                if let ResolvedType::Optional(payload) = expected {
                    return self.check_some(expression.span, call, Some(payload));
                }
            }
        }
        if let ExpressionKind::Binary(binary) = &expression.kind {
            if binary.operator == BinaryOperator::Coalesce {
                let actual = self.check_coalesce(expression.span, binary, Some(expected))?;
                if &actual.ty != expected {
                    return Err(type_mismatch(&actual.ty, expected, expression.span));
                }
                return Ok(actual);
            }
        }
        if let ExpressionKind::Grouping(inner) = &expression.kind {
            let inner = self.check_expr(inner, expected)?;
            return Ok(self.expr(
                HirExprKind::Grouping(Box::new(inner)),
                expected.clone(),
                expression.span,
            ));
        }
        if let ExpressionKind::ObjectLiteral(object) = &expression.kind {
            return self.check_object_literal(expression.span, object, expected);
        }
        if let ExpressionKind::ArrayLiteral(array) = &expression.kind {
            if let ResolvedType::Array(element) = expected {
                return self.check_array_literal(expression.span, array, element);
            }
        }
        let actual = self.synthesize_expr(expression)?;
        if &actual.ty != expected {
            return Err(type_mismatch(&actual.ty, expected, expression.span));
        }
        Ok(actual)
    }

    fn synthesize_expr(&mut self, expression: &Expression) -> Result<HirExpr, SemaError> {
        match &expression.kind {
            ExpressionKind::Closure(closure) => self.check_closure(closure, expression.span),
            ExpressionKind::Template(parts) => {
                let mut result = self.expr(HirExprKind::StringLiteral(String::new()), ResolvedType::String, expression.span);
                for part in parts {
                    let value = self.synthesize_expr(part)?;
                    if !matches!(value.ty, ResolvedType::String | ResolvedType::Int | ResolvedType::Number | ResolvedType::Boolean) {
                        return Err(SemaError::new(SemaErrorKind::UnsupportedType, "interpolação aceita somente string, int, number ou boolean", part.span));
                    }
                    let value = self.expr(HirExprKind::Call { function: "$template.stringify".into(), arguments: vec![HirCallArgument { span: part.span, expression: value, pass_mode: ArgPassMode::ByRef, clone_mode: None, defer_borrow_until_call: false, pre_evaluation_temp: None }] }, ResolvedType::String, part.span);
                    result = self.expr(HirExprKind::Binary { left: Box::new(result), operator: BinaryOperator::Add, right: Box::new(value) }, ResolvedType::String, expression.span);
                }
                Ok(result)
            }
            ExpressionKind::NullLiteral => Err(SemaError::new(SemaErrorKind::UnsupportedType, "não é possível inferir T? para `null`; informe uma anotação ou contexto opcional", expression.span)),
            ExpressionKind::IntegerLiteral(value) => {
                let value = i64::try_from(*value).map_err(|_| {
                    SemaError::new(
                        SemaErrorKind::InvalidIntegerLiteral,
                        "literal inteiro positivo não cabe em `int` (i64)",
                        expression.span,
                    )
                })?;
                Ok(self.expr(HirExprKind::IntegerLiteral(value), ResolvedType::Int, expression.span))
            }
            ExpressionKind::NumberLiteral(value) => {
                if !value.is_finite() {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidNumberLiteral,
                        "literal `number` precisa ser finito",
                        expression.span,
                    ));
                }
                Ok(self.expr(HirExprKind::NumberLiteral(*value), ResolvedType::Number, expression.span))
            }
            ExpressionKind::StringLiteral(value) => Ok(self.expr(
                HirExprKind::StringLiteral(value.clone()),
                ResolvedType::String,
                expression.span,
            )),
            ExpressionKind::BooleanLiteral(value) => Ok(self.expr(
                HirExprKind::BooleanLiteral(*value),
                ResolvedType::Boolean,
                expression.span,
            )),
            ExpressionKind::Identifier(identifier) => {
                let binding = self.lookup(&identifier.name).cloned().ok_or_else(|| {
                    if self.functions.contains_key(&identifier.name) {
                        SemaError::new(
                            SemaErrorKind::UnsupportedExpression,
                            "funções não podem ser usadas como valores nesta fase",
                            identifier.span,
                        )
                    } else {
                        SemaError::new(
                            SemaErrorKind::UnknownVariable,
                            format!("variável `{}` não foi declarada", identifier.name),
                            identifier.span,
                        )
                    }
                })?;
                Ok(self.expr(
                    HirExprKind::Local {
                        id: binding.id,
                        name: identifier.name.clone(),
                    },
                    binding.ty,
                    expression.span,
                ))
            }
            ExpressionKind::ObjectLiteral(_) => Err(SemaError::new(
                SemaErrorKind::ObjectLiteralNeedsContext,
                "object literal precisa de tipo de struct esperado, por exemplo `const u: User = { ... }`",
                expression.span,
            )),
            ExpressionKind::ArrayLiteral(array) => self.synthesize_array_literal(expression.span, array),
            ExpressionKind::Unary(unary) => {
                if unary.operator == UnaryOperator::Negate {
                    if let Some(magnitude) = integer_magnitude(&unary.operand) {
                        if magnitude == i64::MAX as u64 + 1 {
                            return Ok(self.expr(
                                HirExprKind::IntegerLiteral(i64::MIN),
                                ResolvedType::Int,
                                expression.span,
                            ));
                        }
                    }
                }
                let operand = self.synthesize_expr(&unary.operand)?;
                let ty = match unary.operator {
                    UnaryOperator::Negate if matches!(operand.ty, ResolvedType::Int | ResolvedType::Number) => operand.ty.clone(),
                    UnaryOperator::Not if operand.ty == ResolvedType::Boolean => ResolvedType::Boolean,
                    _ => {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidOperator,
                            format!("operador unário incompatível com `{}`", operand.ty),
                            unary.span,
                        ));
                    }
                };
                Ok(self.expr(
                    HirExprKind::Unary {
                        operator: unary.operator,
                        operand: Box::new(operand),
                    },
                    ty,
                    expression.span,
                ))
            }
            ExpressionKind::Binary(binary) if binary.operator == BinaryOperator::Coalesce => self.check_coalesce(expression.span, binary, None),
            ExpressionKind::Binary(binary) => {
                let equality = matches!(binary.operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
                let (mut left, right) = if equality && is_null_expression(&binary.left) {
                    let right = self.synthesize_expr(&binary.right)?;
                    (self.check_expr(&binary.left, &right.ty)?, right)
                } else {
                    let left = self.synthesize_expr(&binary.left)?;
                    let right = if equality && matches!(left.ty, ResolvedType::Optional(_)) {
                        self.check_expr(&binary.right, &left.ty)?
                    } else { self.synthesize_expr(&binary.right)? };
                    (left, right)
                };
                let mut right_mut_roots = HashSet::new();
                collect_nested_mut_borrow_roots(&right, &mut right_mut_roots);
                if !left.ty.is_copy()
                    && is_place(&left)
                    && expression_reads_any_root(&left, &right_mut_roots.iter().copied().collect::<Vec<_>>())
                {
                    left = self.clone_owned_expression(left);
                }
                let ty = binary_result_type(binary.operator, &left.ty, &right.ty, binary.span)?;
                Ok(self.expr(
                    HirExprKind::Binary {
                        left: Box::new(left),
                        operator: binary.operator,
                        right: Box::new(right),
                    },
                    ty,
                    expression.span,
                ))
            }
            ExpressionKind::FieldAccess(access) => {
                let object = self.synthesize_expr(&access.object)?;
                let ResolvedType::Struct(name) = &object.ty else {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!("acesso a campo exige struct, recebeu `{}`", object.ty),
                        access.object.span,
                    ));
                };
                let field = self
                    .structures
                    .get(name)
                    .and_then(|info| info.fields.iter().find(|field| field.name == access.field.name))
                    .ok_or_else(|| {
                        SemaError::new(
                            SemaErrorKind::UnknownField,
                            format!("`{name}` não possui campo `{}`", access.field.name),
                            access.field.span,
                        )
                    })?;
                Ok(self.expr(
                    HirExprKind::FieldAccess {
                        object: Box::new(object),
                        field: access.field.name.clone(),
                    },
                    field.ty.clone(),
                    expression.span,
                ))
            }
            ExpressionKind::Call(call) => self.check_call(expression.span, call),
            ExpressionKind::Index(index) => {
                let object = self.synthesize_expr(&index.object)?;
                let index_expr = self.check_expr(&index.index, &ResolvedType::Int)?;
                let ResolvedType::Array(element) = object.ty.clone() else {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!("indexação exige array, recebeu `{}`", object.ty),
                        index.object.span,
                    ));
                };
                if let Some(root) = root_binding(&object) {
                    let mut nested_mut_roots = HashSet::new();
                    collect_nested_mut_borrow_roots(&index_expr, &mut nested_mut_roots);
                    if nested_mut_roots.contains(&root) {
                        return Err(SemaError::new(
                            SemaErrorKind::UnsupportedOwnershipPattern,
                            "indexação lê uma coleção que uma chamada interna altera; avalie o índice em variável separada",
                            index.index.span,
                        ));
                    }
                }
                Ok(self.expr(
                    HirExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index_expr),
                    },
                    *element,
                    expression.span,
                ))
            }
            ExpressionKind::Grouping(inner) => {
                let inner = self.synthesize_expr(inner)?;
                let ty = inner.ty.clone();
                Ok(self.expr(HirExprKind::Grouping(Box::new(inner)), ty, expression.span))
            }
        }
    }

    fn check_object_literal(
        &mut self,
        span: Span,
        object: &lift_ast::ObjectLiteral,
        expected: &ResolvedType,
    ) -> Result<HirExpr, SemaError> {
        let ResolvedType::Struct(struct_name) = expected else {
            return Err(type_mismatch(
                &ResolvedType::Struct("<objeto>".into()),
                expected,
                span,
            ));
        };
        let structure = self.structures.get(struct_name).cloned().ok_or_else(|| {
            SemaError::new(
                SemaErrorKind::UnknownType,
                format!("tipo `{struct_name}` não está declarado"),
                span,
            )
        })?;
        let mut provided = HashMap::<String, &lift_ast::ObjectField>::new();
        for field in &object.fields {
            if let Some(previous) = provided.insert(field.name.name.clone(), field) {
                return Err(SemaError::new(
                    SemaErrorKind::DuplicateObjectField,
                    format!("campo `{}` aparece mais de uma vez", field.name.name),
                    field.name.span,
                )
                .with_secondary(previous.name.span));
            }
            if !structure
                .fields
                .iter()
                .any(|known| known.name == field.name.name)
            {
                return Err(SemaError::new(
                    SemaErrorKind::UnexpectedObjectField,
                    format!("`{struct_name}` não declara o campo `{}`", field.name.name),
                    field.name.span,
                ));
            }
        }
        for declared in &structure.fields {
            if !provided.contains_key(&declared.name) {
                return Err(SemaError::new(
                    SemaErrorKind::MissingObjectField,
                    format!(
                        "object literal de `{struct_name}` não informa `{}`",
                        declared.name
                    ),
                    span,
                ));
            }
        }

        // HIR preserva a ordem da fonte para que chamadas/efeitos em valores
        // de campos não sejam reordenados durante a construção.
        let mut fields = Vec::with_capacity(object.fields.len());
        for field in &object.fields {
            let declared = structure
                .fields
                .iter()
                .find(|declared| declared.name == field.name.name)
                .ok_or_else(|| {
                    SemaError::new(
                        SemaErrorKind::UnexpectedObjectField,
                        format!("`{struct_name}` não declara o campo `{}`", field.name.name),
                        field.name.span,
                    )
                })?;
            let value = self.check_expr(&field.value, &declared.ty)?;
            fields.push(HirStructInitField {
                name: declared.name.clone(),
                value: self.materialize_owned(value, false),
                span: field.span,
            });
        }
        Ok(self.expr(
            HirExprKind::StructLiteral {
                struct_name: struct_name.clone(),
                fields,
            },
            expected.clone(),
            span,
        ))
    }

    fn synthesize_array_literal(
        &mut self,
        span: Span,
        array: &ArrayLiteral,
    ) -> Result<HirExpr, SemaError> {
        let first = array.elements.first().ok_or_else(|| {
            SemaError::new(
                SemaErrorKind::EmptyArrayNeedsContext,
                "array vazio precisa de anotação de tipo, por exemplo `const xs: int[] = []`",
                span,
            )
        })?;
        let first = self.synthesize_expr(first)?;
        if !first.ty.is_value_type() {
            return Err(SemaError::new(
                SemaErrorKind::TypeMismatch,
                "elemento de array não pode ter tipo `void`",
                array.elements[0].span,
            ));
        }
        let element_type = first.ty.clone();
        let mut elements = vec![self.materialize_owned(first, false)];
        for element in array.elements.iter().skip(1) {
            let value = match self.check_expr(element, &element_type) {
                Ok(value) => value,
                Err(error) if error.kind == SemaErrorKind::TypeMismatch => {
                    return Err(SemaError::new(
                        SemaErrorKind::HeterogeneousArray,
                        format!("todos os elementos do array precisam ter tipo `{element_type}`"),
                        element.span,
                    ));
                }
                Err(error) => return Err(error),
            };
            elements.push(self.materialize_owned(value, false));
        }
        Ok(self.expr(
            HirExprKind::ArrayLiteral(elements),
            ResolvedType::Array(Box::new(element_type)),
            span,
        ))
    }

    fn check_array_literal(
        &mut self,
        span: Span,
        array: &ArrayLiteral,
        expected_element: &ResolvedType,
    ) -> Result<HirExpr, SemaError> {
        let mut elements = Vec::with_capacity(array.elements.len());
        for element in &array.elements {
            let value = self.check_expr(element, expected_element)?;
            elements.push(self.materialize_owned(value, false));
        }
        Ok(self.expr(
            HirExprKind::ArrayLiteral(elements),
            ResolvedType::Array(Box::new(expected_element.clone())),
            span,
        ))
    }

    fn option_operation<'expression>(
        &self,
        call: &'expression lift_ast::CallExpression,
    ) -> Option<&'expression str> {
        let ExpressionKind::FieldAccess(access) = &call.callee.kind else {
            return None;
        };
        if direct_callee_name(&access.object).as_deref() == Some("option")
            && self.lookup("option").is_none()
            && !self.functions.contains_key("option")
            && !self.structures.contains_key("option")
        {
            Some(access.field.name.as_str())
        } else {
            None
        }
    }

    fn check_some(
        &mut self,
        span: Span,
        call: &lift_ast::CallExpression,
        expected_payload: Option<&ResolvedType>,
    ) -> Result<HirExpr, SemaError> {
        if call.arguments.len() != 1 {
            return Err(SemaError::new(
                SemaErrorKind::ArgumentCountMismatch,
                "option.some espera exatamente um argumento",
                span,
            ));
        }
        let payload = if let Some(expected) = expected_payload {
            self.check_expr(&call.arguments[0], expected)?
        } else {
            self.synthesize_expr(&call.arguments[0])?
        };
        if !payload.ty.is_value_type() || matches!(payload.ty, ResolvedType::Optional(_)) {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "option.some exige valor não void e não opcional",
                payload.span,
            ));
        }
        let ty = ResolvedType::Optional(Box::new(payload.ty.clone()));
        let payload = self.materialize_owned(payload, false);
        Ok(self.expr(HirExprKind::OptionSome(Box::new(payload)), ty, span))
    }

    fn check_coalesce(
        &mut self,
        span: Span,
        binary: &lift_ast::BinaryExpression,
        expected: Option<&ResolvedType>,
    ) -> Result<HirExpr, SemaError> {
        let mut known_fallback = None;
        let option = if is_null_expression(&binary.left) {
            let context = if let Some(expected) = expected {
                if matches!(expected, ResolvedType::Optional(_)) {
                    expected.clone()
                } else {
                    ResolvedType::Optional(Box::new(expected.clone()))
                }
            } else {
                let fallback = self.synthesize_expr(&binary.right)?;
                let context = if matches!(fallback.ty, ResolvedType::Optional(_)) {
                    fallback.ty.clone()
                } else {
                    ResolvedType::Optional(Box::new(fallback.ty.clone()))
                };
                known_fallback = Some(fallback);
                context
            };
            if !context.is_value_type() {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    "não é possível inferir um opcional válido para ??",
                    span,
                ));
            }
            self.check_expr(&binary.left, &context)?
        } else {
            self.synthesize_expr(&binary.left)?
        };
        let ResolvedType::Optional(payload) = option.ty.clone() else {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "?? exige T? no lado esquerdo",
                binary.left.span,
            ));
        };
        let fallback = if let Some(fallback) = known_fallback {
            fallback
        } else if is_null_expression(&binary.right) || self.is_some_expression(&binary.right) {
            self.check_expr(&binary.right, &option.ty)?
        } else if is_contextual_aggregate(&binary.right) {
            self.check_expr(&binary.right, &payload)?
        } else {
            self.synthesize_expr(&binary.right)?
        };
        let (mode, ty) = if fallback.ty == *payload {
            (CoalesceMode::Value, *payload)
        } else if fallback.ty == option.ty {
            (CoalesceMode::Optional, option.ty.clone())
        } else {
            return Err(SemaError::new(
                SemaErrorKind::TypeMismatch,
                format!(
                    "fallback de ?? deve ser `{}` ou `{}`, recebeu `{}`",
                    payload, option.ty, fallback.ty
                ),
                fallback.span,
            ));
        };
        let option = self.materialize_owned(option, false);
        let fallback = self.materialize_owned(fallback, false);
        Ok(self.expr(
            HirExprKind::Coalesce {
                option: Box::new(option),
                fallback: Box::new(fallback),
                mode,
            },
            ty,
            span,
        ))
    }

    fn is_some_expression(&self, expression: &Expression) -> bool {
        match &expression.kind {
            ExpressionKind::Grouping(inner) => self.is_some_expression(inner),
            ExpressionKind::Call(call) => self.option_operation(call) == Some("some"),
            _ => false,
        }
    }

    fn check_closure(
        &mut self,
        closure: &lift_ast::ClosureExpression,
        span: Span,
    ) -> Result<HirExpr, SemaError> {
        let result = self.resolve_local_type(&closure.return_type, TypePosition::Return)?;
        let mut names = closure_free_names(closure).into_iter().collect::<Vec<_>>();
        names.sort();
        let mut snapshots = Vec::new();
        for name in names {
            if let Some(binding) = self.lookup(&name).cloned() {
                if binding.ty.contains_resource() {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedOwnershipPattern,
                        "closures não podem capturar recursos opacos",
                        span,
                    ));
                }
                if matches!(binding.ty, ResolvedType::Closure(..)) {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedOwnershipPattern,
                        "captura de outra closure não é suportada",
                        span,
                    ));
                }
                let source = self.expr(
                    HirExprKind::Local {
                        id: binding.id,
                        name: name.clone(),
                    },
                    binding.ty.clone(),
                    span,
                );
                snapshots.push((name, binding.ty, self.materialize_owned(source, false)));
            }
        }
        let signature = FunctionInfo {
            name: "$closure".into(),
            parameters: closure
                .parameters
                .iter()
                .map(|p| {
                    Ok(ParameterInfo {
                        name: p.name.name.clone(),
                        ty: self.resolve_local_type(&p.ty, TypePosition::Parameter)?,
                        span: p.span,
                        pass_mode: ParameterPassMode::OwnedMove,
                        mutated: false,
                    })
                })
                .collect::<Result<Vec<_>, SemaError>>()?,
            return_type: result.clone(),
            has_explicit_return_type: true,
            span,
        };
        let mut mutated = HashSet::new();
        let mut summary_functions = self.functions.clone();
        for scope in &self.scopes {
            for name in scope.keys() {
                hide_namespace(&mut summary_functions, name);
            }
        }
        collect_forwarded_mut_parameters(
            &closure.body,
            &signature,
            &summary_functions,
            &mut mutated,
        );
        for p in &closure.parameters {
            if block_mutates_name(&closure.body, &p.name.name) {
                mutated.insert(p.name.name.clone());
            }
        }
        let mut checker = BodyChecker::new(
            self.structures,
            self.functions,
            self.next_binding_id,
            self.mutable_bindings,
            Some(FunctionContext {
                return_type: result.clone(),
                has_explicit_return_type: true,
            }),
        );
        let mut captures = Vec::new();
        for (name, ty, value) in snapshots {
            let id = checker.fresh_binding_id();
            checker.declare(
                &name,
                BindingInfo {
                    id,
                    ty,
                    binding: Some(BindingKind::Const),
                    parameter_mode: None,
                    parameter_mutated: false,
                    span,
                },
            )?;
            captures.push((id, value));
        }
        let mut parameters = Vec::new();
        for parameter in &closure.parameters {
            let ty = checker.resolve_local_type(&parameter.ty, TypePosition::Parameter)?;
            let id = checker.fresh_binding_id();
            let pass_mode = if ty.is_copy() {
                ParameterPassMode::ByValue
            } else {
                ParameterPassMode::OwnedMove
            };
            checker.declare_parameter(
                &parameter.name.name,
                BindingInfo {
                    id,
                    ty: ty.clone(),
                    binding: None,
                    parameter_mode: Some(pass_mode),
                    parameter_mutated: mutated.contains(&parameter.name.name),
                    span: parameter.span,
                },
            )?;
            parameters.push(HirParameter {
                id,
                name: parameter.name.name.clone(),
                ty,
                pass_mode,
                requires_mut_binding: false,
                span: parameter.span,
            });
        }
        let body = checker.check_block(&closure.body, false)?;
        let mut wrapped = vec![HirDeclaration::Function(HirFunction {
            name: String::new(),
            parameters: Vec::new(),
            return_type: result.clone(),
            body,
            span,
        })];
        apply_mutable_binding_flags(&mut wrapped, checker.mutable_bindings);
        let HirDeclaration::Function(checked) = wrapped.pop().unwrap() else {
            unreachable!()
        };
        let body = checked.body;
        if result != ResolvedType::Void && !block_guarantees_return(&closure.body) {
            return Err(SemaError::new(
                SemaErrorKind::MissingReturnPath,
                "closure deve retornar em todos os caminhos",
                span,
            ));
        }
        if captures
            .iter()
            .any(|(id, _)| checker.mutable_bindings.contains(id))
        {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedOwnershipPattern,
                "captures de closure são snapshots somente leitura",
                span,
            ));
        }
        for parameter in &mut parameters {
            parameter.requires_mut_binding = mutated.contains(&parameter.name)
                || checker.mutable_bindings.contains(&parameter.id);
        }
        let ty = ResolvedType::Closure(
            parameters.iter().map(|p| p.ty.clone()).collect(),
            Box::new(result),
        );
        Ok(self.expr(
            HirExprKind::Closure {
                captures,
                parameters,
                body,
            },
            ty,
            span,
        ))
    }

    fn check_call(
        &mut self,
        span: Span,
        call: &lift_ast::CallExpression,
    ) -> Result<HirExpr, SemaError> {
        if let ExpressionKind::FieldAccess(access) = &call.callee.kind {
            if direct_callee_name(&access.object).as_deref() == Some("http")
                && self.lookup("http").is_none()
                && !self.functions.contains_key("http.")
                && !self.functions.contains_key("http")
                && !self.structures.contains_key("http")
            {
                return self.check_http_call(span, &access.field.name, &call.arguments);
            }
        }
        if let ExpressionKind::FieldAccess(access) = &call.callee.kind {
            if let Some(alias) = direct_callee_name(&access.object) {
                if self.lookup(&alias).is_none()
                    && self.functions.contains_key(&format!("{alias}."))
                {
                    let name = format!("{alias}.{}", access.field.name);
                    if !self.functions.contains_key(&name) {
                        return Err(SemaError::new(
                            SemaErrorKind::PrivatePackageMember,
                            format!("função `{name}` ausente ou não exportada pelo pacote"),
                            access.field.span,
                        ));
                    }
                    return self.check_call(
                        span,
                        &lift_ast::CallExpression {
                            callee: Box::new(Expression {
                                kind: ExpressionKind::Identifier(Identifier {
                                    name,
                                    span: call.callee.span,
                                }),
                                span: call.callee.span,
                            }),
                            arguments: call.arguments.clone(),
                            span: call.span,
                        },
                    );
                }
            }
        }
        let closure_callee = match &call.callee.kind {
            ExpressionKind::Closure(_) => Some(self.synthesize_expr(&call.callee)?),
            ExpressionKind::Identifier(name)
                if self
                    .lookup(&name.name)
                    .is_some_and(|b| matches!(b.ty, ResolvedType::Closure(..))) =>
            {
                Some(self.synthesize_expr(&call.callee)?)
            }
            ExpressionKind::Grouping(inner) if matches!(inner.kind, ExpressionKind::Closure(_)) => {
                Some(self.synthesize_expr(inner)?)
            }
            _ => None,
        };
        if let Some(callee) = closure_callee {
            let ResolvedType::Closure(parameters, result) = callee.ty.clone() else {
                unreachable!()
            };
            if parameters.len() != call.arguments.len() {
                return Err(SemaError::new(
                    SemaErrorKind::ArgumentCountMismatch,
                    "número incorreto de argumentos da closure",
                    span,
                ));
            }
            let mut arguments = Vec::new();
            for (argument, ty) in call.arguments.iter().zip(parameters) {
                let value = self.check_expr(argument, &ty)?;
                arguments.push(self.materialize_owned(value, false));
            }
            return Ok(self.expr(
                HirExprKind::ClosureCall {
                    callee: Box::new(callee),
                    arguments,
                },
                *result,
                span,
            ));
        }
        if let Some(operation) = self.option_operation(call) {
            if operation == "some" {
                return self.check_some(span, call, None);
            }
            if matches!(operation, "is_some" | "unwrap") {
                if call.arguments.len() != 1 {
                    return Err(SemaError::new(
                        SemaErrorKind::ArgumentCountMismatch,
                        "operações option esperam exatamente um argumento",
                        span,
                    ));
                }
                let expression = self.synthesize_expr(&call.arguments[0])?;
                let ResolvedType::Optional(payload) = expression.ty.clone() else {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "operações option exigem um valor T?",
                        expression.span,
                    ));
                };
                let (function, ty) = if operation == "is_some" {
                    (OPTION_IS_SOME_BUILTIN, ResolvedType::Boolean)
                } else {
                    (OPTION_UNWRAP_BUILTIN, *payload)
                };
                return Ok(self.expr(
                    HirExprKind::Call {
                        function: function.to_owned(),
                        arguments: vec![HirCallArgument {
                            span: expression.span,
                            expression,
                            pass_mode: ArgPassMode::ByRef,
                            clone_mode: None,
                            defer_borrow_until_call: false,
                            pre_evaluation_temp: None,
                        }],
                    },
                    ty,
                    span,
                ));
            }
        }
        if let ExpressionKind::FieldAccess(access) = &call.callee.kind {
            if direct_callee_name(&access.object).as_deref() == Some("result")
                && matches!(
                    access.field.name.as_str(),
                    "is_ok" | "unwrap" | "unwrap_err"
                )
                && self.lookup("result").is_none()
                && !self.functions.contains_key("result")
                && !self.structures.contains_key("result")
            {
                if call.arguments.len() != 1 {
                    return Err(SemaError::new(
                        SemaErrorKind::ArgumentCountMismatch,
                        "operações `result` esperam exatamente um argumento",
                        span,
                    ));
                }
                let expression = self.synthesize_expr(&call.arguments[0])?;
                let ResolvedType::Result(ok, error) = expression.ty.clone() else {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "operações `result` exigem um valor Result<T, E>",
                        expression.span,
                    ));
                };
                let (function, return_type) = match access.field.name.as_str() {
                    "is_ok" => (RESULT_IS_OK_BUILTIN, ResolvedType::Boolean),
                    "unwrap" => (RESULT_UNWRAP_BUILTIN, *ok),
                    "unwrap_err" => (RESULT_UNWRAP_ERR_BUILTIN, *error),
                    _ => unreachable!(),
                };
                let consuming = expression.ty.contains_resource() && access.field.name != "is_ok";
                return Ok(self.expr(
                    HirExprKind::Call {
                        function: function.to_owned(),
                        arguments: vec![HirCallArgument {
                            span: expression.span,
                            expression,
                            pass_mode: if consuming {
                                ArgPassMode::ByValue
                            } else {
                                ArgPassMode::ByRef
                            },
                            clone_mode: None,
                            defer_borrow_until_call: false,
                            pre_evaluation_temp: None,
                        }],
                    },
                    return_type,
                    span,
                ));
            }
            if direct_callee_name(&access.object).as_deref() == Some("console")
                && access.field.name == "log"
                && self.lookup("console").is_none()
                && !self.functions.contains_key("console")
                && !self.structures.contains_key("console")
            {
                if call.arguments.len() != 1 {
                    return Err(SemaError::new(
                        SemaErrorKind::ArgumentCountMismatch,
                        "`console.log` espera exatamente um argumento",
                        span,
                    ));
                }
                let expression = self.synthesize_expr(&call.arguments[0])?;
                if !matches!(
                    expression.ty,
                    ResolvedType::Int
                        | ResolvedType::Number
                        | ResolvedType::Boolean
                        | ResolvedType::String
                ) {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "`console.log` aceita int, number, boolean ou string",
                        expression.span,
                    ));
                }
                return Ok(self.expr(
                    HirExprKind::Call {
                        function: CONSOLE_LOG_BUILTIN.to_owned(),
                        arguments: vec![HirCallArgument {
                            span: expression.span,
                            expression,
                            pass_mode: ArgPassMode::ByRef,
                            clone_mode: None,
                            defer_borrow_until_call: false,
                            pre_evaluation_temp: None,
                        }],
                    },
                    ResolvedType::Void,
                    span,
                ));
            }
        }
        if let ExpressionKind::FieldAccess(access) = &call.callee.kind {
            let receiver = self.synthesize_expr(&access.object)?;
            if let ResolvedType::Struct(target) = receiver.ty {
                let name = method_name(&target, &access.field.name);
                let mut arguments = vec![(*access.object).clone()];
                arguments.extend(call.arguments.clone());
                let callee = Expression {
                    kind: ExpressionKind::Identifier(Identifier {
                        name,
                        span: access.field.span,
                    }),
                    span: call.callee.span,
                };
                return self.check_call(
                    span,
                    &lift_ast::CallExpression {
                        callee: Box::new(callee),
                        arguments,
                        span,
                    },
                );
            }
        }
        let callee = direct_callee_name(&call.callee).ok_or_else(|| {
            SemaError::new(
                SemaErrorKind::NotCallable,
                "somente chamadas diretas a funções nomeadas são suportadas nesta fase",
                call.callee.span,
            )
        })?;
        if self.lookup(&callee).is_some() {
            return Err(SemaError::new(
                SemaErrorKind::NotCallable,
                format!("`{callee}` é uma variável, não uma função"),
                call.callee.span,
            ));
        }
        let function = self.functions.get(&callee).cloned().ok_or_else(|| {
            SemaError::new(
                SemaErrorKind::UnknownFunction,
                format!("função `{callee}` não foi declarada"),
                call.callee.span,
            )
        })?;
        if call.arguments.len() != function.parameters.len() {
            return Err(SemaError::new(
                SemaErrorKind::ArgumentCountMismatch,
                format!(
                    "`{callee}` espera {} argumento(s), recebeu {}",
                    function.parameters.len(),
                    call.arguments.len()
                ),
                span,
            ));
        }

        let mut arguments = Vec::with_capacity(call.arguments.len());
        for (argument, parameter) in call.arguments.iter().zip(&function.parameters) {
            let expression = self.check_expr(argument, &parameter.ty)?;
            let pass_mode = match parameter.pass_mode {
                ParameterPassMode::ByValue => {
                    if !expression.ty.is_copy() && is_place(&expression) {
                        ArgPassMode::ByClone
                    } else {
                        ArgPassMode::ByValue
                    }
                }
                ParameterPassMode::BorrowImm => ArgPassMode::ByRef,
                ParameterPassMode::BorrowMut => ArgPassMode::ByMutRef,
                ParameterPassMode::OwnedMove => {
                    if !expression.ty.contains_resource()
                        && !expression.ty.is_copy()
                        && is_place(&expression)
                    {
                        ArgPassMode::ByClone
                    } else {
                        ArgPassMode::ByValue
                    }
                }
            };
            let clone_mode = (pass_mode == ArgPassMode::ByClone)
                .then(|| self.clone_mode_for_expression(&expression));
            arguments.push(HirCallArgument {
                expression,
                pass_mode,
                clone_mode,
                defer_borrow_until_call: false,
                pre_evaluation_temp: None,
                span: argument.span,
            });
        }

        self.resolve_call_aliasing(&mut arguments)?;
        Ok(self.expr(
            HirExprKind::Call {
                function: function.name,
                arguments,
            },
            function.return_type,
            span,
        ))
    }

    fn check_http_call(
        &mut self,
        span: Span,
        operation: &str,
        args: &[Expression],
    ) -> Result<HirExpr, SemaError> {
        let invalid = || {
            SemaError::new(SemaErrorKind::UnsupportedType, "http.route exige método/caminho literais e handler nomeado (Request[, DbPool emprestado]) -> Response; http.serve exige endereço string, limite int e pool owned opcional", span)
        };
        let (function, expressions, ty) = match operation {
            "route" if args.len() == 3 && self.function.is_none() => {
                let ExpressionKind::StringLiteral(method) = &args[0].kind else {
                    return Err(invalid());
                };
                let ExpressionKind::StringLiteral(path) = &args[1].kind else {
                    return Err(invalid());
                };
                if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH")
                    || !path.starts_with('/')
                    || !path
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"/_-.~".contains(&b))
                {
                    return Err(invalid());
                }
                let name = direct_callee_name(&args[2]).ok_or_else(invalid)?;
                let handler = self.functions.get(&name).ok_or_else(invalid)?;
                if !matches!(handler.parameters.len(), 1 | 2) || self.lookup(&name).is_some() {
                    return Err(invalid());
                }
                let needs_pool = handler.parameters.len() == 2;
                if needs_pool
                    && (handler.parameters[1].ty
                        != ResolvedType::Resource(crate::ResourceKind::DbPool)
                        || handler.parameters[1].pass_mode != ParameterPassMode::BorrowImm)
                {
                    return Err(invalid());
                }
                let ResolvedType::Struct(request) = &handler.parameters[0].ty else {
                    return Err(invalid());
                };
                let ResolvedType::Struct(response) = &handler.return_type else {
                    return Err(invalid());
                };
                let req = &self.structures.get(request).ok_or_else(invalid)?.fields;
                let res = &self.structures.get(response).ok_or_else(invalid)?.fields;
                let exact = |fields: &[HirStructField], wanted: &[(&str, ResolvedType)]| {
                    fields.len() == wanted.len()
                        && wanted
                            .iter()
                            .all(|(name, ty)| fields.iter().any(|f| f.name == *name && f.ty == *ty))
                };
                if !exact(
                    req,
                    &[
                        ("method", ResolvedType::String),
                        ("path", ResolvedType::String),
                        ("query", ResolvedType::String),
                        ("body", ResolvedType::String),
                    ],
                ) || !exact(
                    res,
                    &[
                        ("status", ResolvedType::Int),
                        ("body", ResolvedType::String),
                    ],
                ) || handler.parameters[0].pass_mode == ParameterPassMode::BorrowMut
                {
                    return Err(invalid());
                }
                let mode = if handler.parameters[0].pass_mode == ParameterPassMode::OwnedMove {
                    "owned"
                } else {
                    "borrow"
                };
                (
                    format!(
                        "$http.route|{name}|{request}|{response}|{mode}|{}",
                        if needs_pool { "pool" } else { "plain" }
                    ),
                    vec![
                        self.check_expr(&args[0], &ResolvedType::String)?,
                        self.check_expr(&args[1], &ResolvedType::String)?,
                    ],
                    ResolvedType::Void,
                )
            }
            "serve" if matches!(args.len(), 2 | 3) && self.function.is_none() => {
                let mut arguments = vec![
                    self.check_expr(&args[0], &ResolvedType::String)?,
                    self.check_expr(&args[1], &ResolvedType::Int)?,
                ];
                if args.len() == 3 {
                    arguments.push(self.check_expr(
                        &args[2],
                        &ResolvedType::Resource(crate::ResourceKind::DbPool),
                    )?);
                }
                (
                    "$http.serve".into(),
                    arguments,
                    ResolvedType::Result(
                        Box::new(ResolvedType::Void),
                        Box::new(ResolvedType::String),
                    ),
                )
            }
            _ => return Err(invalid()),
        };
        Ok(self.expr(
            HirExprKind::Call {
                function,
                arguments: expressions
                    .into_iter()
                    .map(|expression| HirCallArgument {
                        span: expression.span,
                        pass_mode: if expression.ty.is_copy() || expression.ty.contains_resource() {
                            ArgPassMode::ByValue
                        } else {
                            ArgPassMode::ByRef
                        },
                        expression,
                        clone_mode: None,
                        defer_borrow_until_call: false,
                        pre_evaluation_temp: None,
                    })
                    .collect(),
            },
            ty,
            span,
        ))
    }

    fn resolve_call_aliasing(
        &mut self,
        arguments: &mut [HirCallArgument],
    ) -> Result<(), SemaError> {
        let mutable_roots = arguments
            .iter()
            .filter(|argument| argument.pass_mode == ArgPassMode::ByMutRef)
            .filter_map(|argument| root_binding(&argument.expression))
            .collect::<Vec<_>>();
        for (i, argument) in arguments.iter().enumerate() {
            if argument.pass_mode != ArgPassMode::ByMutRef {
                continue;
            }
            if let Some(root) = root_binding(&argument.expression) {
                for other in arguments.iter().skip(i + 1) {
                    if other.pass_mode == ArgPassMode::ByMutRef
                        && root_binding(&other.expression) == Some(root)
                    {
                        return Err(SemaError::new(
                            SemaErrorKind::BorrowConflict,
                            "a mesma variável não pode ser emprestada mutavelmente duas vezes na chamada",
                            other.span,
                        )
                        .with_secondary(argument.span));
                    }
                }
            }
        }

        let nested_mut_roots = arguments
            .iter()
            .map(|argument| {
                let mut roots = HashSet::new();
                collect_nested_mut_borrow_roots(&argument.expression, &mut roots);
                roots
            })
            .collect::<Vec<_>>();
        for (index, argument) in arguments.iter_mut().enumerate() {
            if !matches!(
                argument.pass_mode,
                ArgPassMode::ByRef | ArgPassMode::ByMutRef
            ) {
                continue;
            }
            let mut expression_roots = HashSet::new();
            collect_hir_binding_uses(&argument.expression, &mut expression_roots);
            let nested_sibling_mutates_place =
                nested_mut_roots
                    .iter()
                    .enumerate()
                    .any(|(other_index, roots)| {
                        other_index != index
                            && roots.iter().any(|root| expression_roots.contains(root))
                    });
            if !nested_sibling_mutates_place {
                continue;
            }
            if is_direct_binding_place(&argument.expression) {
                argument.defer_borrow_until_call = true;
            } else if argument.pass_mode == ArgPassMode::ByRef {
                argument.expression = self.clone_owned_expression(argument.expression.clone());
                argument.pre_evaluation_temp = Some(CallTempId(0));
            } else {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedOwnershipPattern,
                    "não é seguro adiar um empréstimo mutável de campo/índice sobreposto por outra chamada",
                    argument.span,
                ));
            }
        }

        for argument in arguments.iter_mut() {
            if argument.pass_mode == ArgPassMode::ByMutRef {
                let place = root_binding(&argument.expression).ok_or_else(|| {
                    SemaError::new(
                        SemaErrorKind::BorrowMutRequiresMutablePlace,
                        "`&mut` inferido exige variável, campo ou elemento de array",
                        argument.span,
                    )
                })?;
                if place_projection_conflicts(&argument.expression, place) {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedOwnershipPattern,
                        "índice do place mutável lê a própria raiz; materialize o índice antes do empréstimo",
                        argument.span,
                    ));
                }
                let binding = self.lookup_by_id(place).cloned().ok_or_else(|| {
                    SemaError::new(
                        SemaErrorKind::UnknownVariable,
                        "place mutável não está mais no escopo",
                        argument.span,
                    )
                })?;
                let mutable_parameter = binding.parameter_mode
                    == Some(ParameterPassMode::BorrowMut)
                    || (binding.parameter_mode == Some(ParameterPassMode::OwnedMove)
                        && binding.parameter_mutated);
                if binding.parameter_mode.is_some() && !mutable_parameter {
                    return Err(SemaError::new(
                        SemaErrorKind::BorrowMutRequiresMutablePlace,
                        "parâmetro não tem contrato mutável para encaminhar `&mut`",
                        argument.span,
                    ));
                }
                if binding.parameter_mode.is_none() {
                    self.mutable_bindings.insert(binding.id);
                }
            } else if expression_reads_any_root(&argument.expression, &mutable_roots) {
                // O codegen materializa este valor antes de emitir qualquer `&mut`.
                // Se ele é um place owned, a cópia preserva o snapshot do argumento.
                if !argument.expression.ty.is_copy() && is_place(&argument.expression) {
                    argument.expression = self.clone_owned_expression(argument.expression.clone());
                    if argument.pass_mode == ArgPassMode::ByClone {
                        argument.pass_mode = ArgPassMode::ByValue;
                        argument.clone_mode = None;
                    }
                }
                argument.pre_evaluation_temp = Some(CallTempId(0));
            }
        }

        let requires_staging = arguments
            .iter()
            .any(|argument| argument.pre_evaluation_temp.is_some())
            || arguments
                .iter()
                .any(|argument| argument.defer_borrow_until_call);
        if requires_staging {
            for (index, argument) in arguments.iter_mut().enumerate() {
                if matches!(
                    argument.pass_mode,
                    ArgPassMode::ByRef | ArgPassMode::ByMutRef
                ) {
                    if argument.pre_evaluation_temp.is_some() {
                        argument.defer_borrow_until_call = false;
                        continue;
                    }
                    if hir_contains_call(&argument.expression) {
                        return Err(SemaError::new(
                            SemaErrorKind::UnsupportedOwnershipPattern,
                            "place emprestado com avaliação interna não pode ser reordenado com temporários de chamada",
                            argument.span,
                        ));
                    }

                    let sibling_mut_roots = nested_mut_roots
                        .iter()
                        .enumerate()
                        .filter(|(other_index, _)| *other_index != index)
                        .flat_map(|(_, roots)| roots.iter().copied())
                        .collect::<Vec<_>>();
                    if !is_direct_binding_place(&argument.expression)
                        && expression_reads_any_root(&argument.expression, &sibling_mut_roots)
                    {
                        if argument.pass_mode == ArgPassMode::ByRef {
                            argument.expression =
                                self.clone_owned_expression(argument.expression.clone());
                            argument.pre_evaluation_temp = Some(CallTempId(0));
                            argument.defer_borrow_until_call = false;
                        } else {
                            return Err(SemaError::new(
                                SemaErrorKind::UnsupportedOwnershipPattern,
                                "place mutável projetado seria alterado antes de sua avaliação",
                                argument.span,
                            ));
                        }
                    } else {
                        argument.defer_borrow_until_call = true;
                    }
                } else if argument.pre_evaluation_temp.is_none() {
                    argument.pre_evaluation_temp = Some(CallTempId(0));
                }
            }
        }

        // IDs de temporários acompanham a ordem fonte, mesmo quando a
        // necessidade de materialização foi descoberta em passes diferentes.
        let mut temp_id = 0_u32;
        for argument in arguments.iter_mut() {
            if argument.pre_evaluation_temp.is_some() {
                argument.pre_evaluation_temp = Some(CallTempId(temp_id));
                temp_id += 1;
            }
        }
        Ok(())
    }

    fn materialize_owned(&self, expression: HirExpr, returning: bool) -> HirExpr {
        if expression.ty.contains_resource() && is_place(&expression) {
            return HirExpr {
                ty: expression.ty.clone(),
                span: expression.span,
                kind: HirExprKind::Move(Box::new(expression)),
            };
        }
        if expression.ty.is_copy() || !is_place(&expression) {
            return expression;
        }
        if returning {
            if let HirExprKind::Local { id, .. } = &expression.kind {
                if self.lookup_by_id(*id).is_some_and(|binding| {
                    binding.parameter_mode == Some(ParameterPassMode::OwnedMove)
                }) {
                    return expression;
                }
            }
        }
        self.clone_owned_expression(expression)
    }

    fn clone_owned_expression(&self, expression: HirExpr) -> HirExpr {
        let mode = self.clone_mode_for_expression(&expression);
        HirExpr {
            kind: HirExprKind::Clone {
                expression: Box::new(expression.clone()),
                mode,
            },
            ty: expression.ty.clone(),
            span: expression.span,
        }
    }

    fn clone_mode_for_expression(&self, expression: &HirExpr) -> CloneMode {
        match &expression.kind {
            HirExprKind::Grouping(inner) => self.clone_mode_for_expression(inner),
            HirExprKind::Local { id, .. } => {
                self.lookup_by_id(*id).map_or(CloneMode::Value, |binding| {
                    if matches!(
                        binding.parameter_mode,
                        Some(ParameterPassMode::BorrowImm | ParameterPassMode::BorrowMut)
                    ) {
                        if binding.ty == ResolvedType::String {
                            CloneMode::BorrowedString
                        } else {
                            CloneMode::BorrowedAggregate
                        }
                    } else {
                        CloneMode::Value
                    }
                })
            }
            _ => CloneMode::Value,
        }
    }

    fn expr(&self, kind: HirExprKind, ty: ResolvedType, span: Span) -> HirExpr {
        HirExpr { kind, ty, span }
    }
}

#[derive(Clone, Copy)]
enum TypePosition {
    Field,
    Parameter,
    Return,
    Variable,
}

fn resolve_type(
    expression: &TypeExpression,
    known_structs: &HashSet<String>,
    position: TypePosition,
) -> Result<ResolvedType, SemaError> {
    let resolved = match &expression.kind {
        TypeExpressionKind::Named(name) => match name.name.as_str() {
            "int" => ResolvedType::Int,
            "number" => ResolvedType::Number,
            "string" => ResolvedType::String,
            "boolean" => ResolvedType::Boolean,
            "void" => ResolvedType::Void,
            "DbValue" => ResolvedType::DbValue,
            "DbPool" => ResolvedType::Resource(crate::ResourceKind::DbPool),
            "DbTransaction" => ResolvedType::Resource(crate::ResourceKind::DbTransaction),
            "DbRows" => ResolvedType::Resource(crate::ResourceKind::DbRows),
            "SqliteConnection" => ResolvedType::Resource(crate::ResourceKind::SqliteConnection),
            "SqliteTransaction" => ResolvedType::Resource(crate::ResourceKind::SqliteTransaction),
            custom if known_structs.contains(custom) => ResolvedType::Struct(custom.to_owned()),
            custom => {
                return Err(SemaError::new(
                    SemaErrorKind::UnknownType,
                    format!("tipo `{custom}` não foi declarado"),
                    name.span,
                ));
            }
        },
        TypeExpressionKind::Array(element) => {
            let element = resolve_type(element, known_structs, TypePosition::Field)?;
            if element == ResolvedType::Void {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    "array de `void` não é permitido",
                    expression.span,
                ));
            }
            ResolvedType::Array(Box::new(element))
        }
        TypeExpressionKind::Result(ok, error) => {
            let ok = resolve_type(ok, known_structs, TypePosition::Return)?;
            let error = resolve_type(error, known_structs, TypePosition::Field)?;
            ResolvedType::Result(Box::new(ok), Box::new(error))
        }
        TypeExpressionKind::Optional(payload) => {
            let payload = resolve_type(payload, known_structs, TypePosition::Field)?;
            if matches!(payload, ResolvedType::Optional(_) | ResolvedType::Void) {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    "opcional aninhado ou void? não é suportado",
                    expression.span,
                ));
            }
            ResolvedType::Optional(Box::new(payload))
        }
    };

    let valid_position = match position {
        TypePosition::Field | TypePosition::Parameter | TypePosition::Variable => {
            resolved.is_value_type()
        }
        TypePosition::Return => true,
    };
    if (matches!(position, TypePosition::Field) && resolved.contains_resource())
        || matches!(&resolved, ResolvedType::Array(t) | ResolvedType::Optional(t) if t.contains_resource())
        || matches!(&resolved, ResolvedType::Result(_, e) if e.contains_resource())
    {
        return Err(SemaError::new(
            SemaErrorKind::UnsupportedType,
            "recursos opacos não podem ser campos, arrays, opcionais nem erros Result",
            expression.span,
        ));
    }
    if matches!(&resolved, ResolvedType::Result(ok, _) if ok.contains_resource() && !matches!(**ok, ResolvedType::Resource(_)))
    {
        return Err(SemaError::new(
            SemaErrorKind::UnsupportedType,
            "Result com recurso exige payload opaco direto",
            expression.span,
        ));
    }
    if !valid_position {
        return Err(SemaError::new(
            SemaErrorKind::UnsupportedType,
            "`void` só pode ser usado como tipo de retorno",
            expression.span,
        ));
    }
    Ok(resolved)
}

fn type_mismatch(actual: &ResolvedType, expected: &ResolvedType, span: Span) -> SemaError {
    SemaError::new(
        SemaErrorKind::TypeMismatch,
        format!("tipo incompatível: esperado `{expected}`, recebido `{actual}`"),
        span,
    )
}

fn is_null_expression(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::NullLiteral => true,
        ExpressionKind::Grouping(inner) => is_null_expression(inner),
        _ => false,
    }
}

fn is_contextual_aggregate(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::ObjectLiteral(_) | ExpressionKind::ArrayLiteral(_) => true,
        ExpressionKind::Grouping(inner) => is_contextual_aggregate(inner),
        _ => false,
    }
}

fn binary_result_type(
    operator: BinaryOperator,
    left: &ResolvedType,
    right: &ResolvedType,
    span: Span,
) -> Result<ResolvedType, SemaError> {
    let numeric = matches!(left, ResolvedType::Int | ResolvedType::Number) && left == right;
    let result = match operator {
        BinaryOperator::Add if left == &ResolvedType::String && right == &ResolvedType::String => {
            Some(ResolvedType::String)
        }
        BinaryOperator::Add
        | BinaryOperator::Subtract
        | BinaryOperator::Multiply
        | BinaryOperator::Divide
        | BinaryOperator::Remainder
            if numeric =>
        {
            Some(left.clone())
        }
        BinaryOperator::And | BinaryOperator::Or
            if left == &ResolvedType::Boolean && right == &ResolvedType::Boolean =>
        {
            Some(ResolvedType::Boolean)
        }
        BinaryOperator::Equal | BinaryOperator::NotEqual
            if left == right && left.is_value_type() =>
        {
            Some(ResolvedType::Boolean)
        }
        BinaryOperator::Less
        | BinaryOperator::LessEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterEqual
            if numeric =>
        {
            Some(ResolvedType::Boolean)
        }
        _ => None,
    };
    result.ok_or_else(|| {
        SemaError::new(
            SemaErrorKind::InvalidOperator,
            format!("operador `{operator:?}` não aceita `{left}` e `{right}`"),
            span,
        )
    })
}

fn is_place(expression: &HirExpr) -> bool {
    match &expression.kind {
        HirExprKind::Local { .. } | HirExprKind::FieldAccess { .. } | HirExprKind::Index { .. } => {
            true
        }
        HirExprKind::Grouping(inner) => is_place(inner),
        _ => false,
    }
}

fn is_direct_binding_place(expression: &HirExpr) -> bool {
    match &expression.kind {
        HirExprKind::Local { .. } => true,
        HirExprKind::Grouping(inner) => is_direct_binding_place(inner),
        _ => false,
    }
}

fn is_bare_place(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Identifier(_) => true,
        ExpressionKind::Grouping(inner) => is_bare_place(inner),
        _ => false,
    }
}

fn root_binding(expression: &HirExpr) -> Option<BindingId> {
    match &expression.kind {
        HirExprKind::Local { id, .. } => Some(*id),
        HirExprKind::FieldAccess { object, .. } | HirExprKind::Index { object, .. } => {
            root_binding(object)
        }
        HirExprKind::Grouping(inner) => root_binding(inner),
        HirExprKind::Clone { expression, .. } => root_binding(expression),
        _ => None,
    }
}

fn expression_reads_any_root(expression: &HirExpr, roots: &[BindingId]) -> bool {
    let mut uses = HashSet::new();
    collect_hir_binding_uses(expression, &mut uses);
    uses.iter().any(|id| roots.contains(id))
}

fn place_projection_conflicts(expression: &HirExpr, root: BindingId) -> bool {
    match &expression.kind {
        HirExprKind::Index { object, index } => {
            (root_binding(object) == Some(root) && expression_reads_any_root(index, &[root]))
                || place_projection_conflicts(object, root)
                || place_projection_conflicts(index, root)
        }
        HirExprKind::FieldAccess { object, .. } | HirExprKind::Grouping(object) => {
            place_projection_conflicts(object, root)
        }
        _ => false,
    }
}

fn collect_nested_mut_borrow_roots(expression: &HirExpr, roots: &mut HashSet<BindingId>) {
    match &expression.kind {
        HirExprKind::Closure { captures, .. } => {
            for (_, value) in captures {
                collect_nested_mut_borrow_roots(value, roots);
            }
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            collect_nested_mut_borrow_roots(callee, roots);
            for value in arguments {
                collect_nested_mut_borrow_roots(value, roots);
            }
        }

        HirExprKind::OptionNone => {}
        HirExprKind::OptionSome(payload) => collect_nested_mut_borrow_roots(payload, roots),
        HirExprKind::Coalesce {
            option, fallback, ..
        } => {
            collect_nested_mut_borrow_roots(option, roots);
            collect_nested_mut_borrow_roots(fallback, roots);
        }
        HirExprKind::Call { arguments, .. } => {
            for argument in arguments {
                if argument.pass_mode == ArgPassMode::ByMutRef {
                    if let Some(root) = root_binding(&argument.expression) {
                        roots.insert(root);
                    }
                }
                collect_nested_mut_borrow_roots(&argument.expression, roots);
            }
        }
        HirExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                collect_nested_mut_borrow_roots(&field.value, roots);
            }
        }
        HirExprKind::ArrayLiteral(elements) => {
            for element in elements {
                collect_nested_mut_borrow_roots(element, roots);
            }
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Grouping(operand)
        | HirExprKind::Move(operand) => collect_nested_mut_borrow_roots(operand, roots),
        HirExprKind::Clone {
            expression: operand,
            ..
        } => collect_nested_mut_borrow_roots(operand, roots),
        HirExprKind::Binary { left, right, .. } => {
            collect_nested_mut_borrow_roots(left, roots);
            collect_nested_mut_borrow_roots(right, roots);
        }
        HirExprKind::FieldAccess { object, .. } => collect_nested_mut_borrow_roots(object, roots),
        HirExprKind::Index { object, index } => {
            collect_nested_mut_borrow_roots(object, roots);
            collect_nested_mut_borrow_roots(index, roots);
        }
        HirExprKind::IntegerLiteral(_)
        | HirExprKind::NumberLiteral(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::BooleanLiteral(_)
        | HirExprKind::Local { .. } => {}
    }
}

fn hir_contains_call(expression: &HirExpr) -> bool {
    match &expression.kind {
        HirExprKind::Closure { captures, .. } => {
            captures.iter().any(|(_, value)| hir_contains_call(value))
        }
        HirExprKind::ClosureCall { .. } => true,
        HirExprKind::OptionNone => false,
        HirExprKind::OptionSome(payload) => hir_contains_call(payload),
        HirExprKind::Coalesce {
            option, fallback, ..
        } => hir_contains_call(option) || hir_contains_call(fallback),
        HirExprKind::Call { .. } => true,
        HirExprKind::StructLiteral { fields, .. } => {
            fields.iter().any(|field| hir_contains_call(&field.value))
        }
        HirExprKind::ArrayLiteral(elements) => elements.iter().any(hir_contains_call),
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Grouping(operand)
        | HirExprKind::Move(operand) => hir_contains_call(operand),
        HirExprKind::Clone {
            expression: operand,
            ..
        } => hir_contains_call(operand),
        HirExprKind::Binary { left, right, .. } => {
            hir_contains_call(left) || hir_contains_call(right)
        }
        HirExprKind::FieldAccess { object, .. } => hir_contains_call(object),
        HirExprKind::Index { object, index } => {
            hir_contains_call(object) || hir_contains_call(index)
        }
        HirExprKind::IntegerLiteral(_)
        | HirExprKind::NumberLiteral(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::BooleanLiteral(_)
        | HirExprKind::Local { .. } => false,
    }
}

fn collect_hir_binding_uses(expression: &HirExpr, uses: &mut HashSet<BindingId>) {
    match &expression.kind {
        HirExprKind::Closure { captures, .. } => {
            for (_, value) in captures {
                collect_hir_binding_uses(value, uses);
            }
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            collect_hir_binding_uses(callee, uses);
            for value in arguments {
                collect_hir_binding_uses(value, uses);
            }
        }

        HirExprKind::OptionNone => {}
        HirExprKind::OptionSome(payload) => collect_hir_binding_uses(payload, uses),
        HirExprKind::Coalesce {
            option, fallback, ..
        } => {
            collect_hir_binding_uses(option, uses);
            collect_hir_binding_uses(fallback, uses);
        }
        HirExprKind::Local { id, .. } => {
            uses.insert(*id);
        }
        HirExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                collect_hir_binding_uses(&field.value, uses);
            }
        }
        HirExprKind::ArrayLiteral(elements) => {
            for element in elements {
                collect_hir_binding_uses(element, uses);
            }
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Grouping(operand)
        | HirExprKind::Move(operand) => collect_hir_binding_uses(operand, uses),
        HirExprKind::Clone { expression, .. } => collect_hir_binding_uses(expression, uses),
        HirExprKind::Binary { left, right, .. } => {
            collect_hir_binding_uses(left, uses);
            collect_hir_binding_uses(right, uses);
        }
        HirExprKind::FieldAccess { object, .. } => collect_hir_binding_uses(object, uses),
        HirExprKind::Call { arguments, .. } => {
            for argument in arguments {
                collect_hir_binding_uses(&argument.expression, uses);
            }
        }
        HirExprKind::Index { object, index } => {
            collect_hir_binding_uses(object, uses);
            collect_hir_binding_uses(index, uses);
        }
        HirExprKind::IntegerLiteral(_)
        | HirExprKind::NumberLiteral(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::BooleanLiteral(_) => {}
    }
}

fn direct_callee_name(expression: &Expression) -> Option<String> {
    match &expression.kind {
        ExpressionKind::Identifier(Identifier { name, .. }) => Some(name.clone()),
        ExpressionKind::Grouping(inner) => direct_callee_name(inner),
        _ => None,
    }
}

fn integer_magnitude(expression: &Expression) -> Option<u64> {
    match &expression.kind {
        ExpressionKind::IntegerLiteral(value) => Some(*value),
        ExpressionKind::Grouping(inner) => integer_magnitude(inner),
        _ => None,
    }
}

fn block_mutates_name(block: &Block, name: &str) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Assignment(assignment) => {
            root_name(&assignment.target).is_some_and(|root| root == name)
                && !is_bare_place(&assignment.target)
        }
        Statement::If(statement) => if_mutates_name(statement, name),
        Statement::While(statement) => block_mutates_name(&statement.body, name),
        _ => false,
    })
}

fn block_returns_name(block: &Block, name: &str) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Return(statement) => statement
            .value
            .as_ref()
            .is_some_and(|expression| expression_contains_owned_name(expression, name)),
        Statement::If(statement) => if_returns_name(statement, name),
        Statement::While(statement) => block_returns_name(&statement.body, name),
        _ => false,
    })
}

fn if_mutates_name(statement: &lift_ast::IfStatement, name: &str) -> bool {
    block_mutates_name(&statement.then_branch, name)
        || statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_mutates_name(block, name),
                ElseBranchKind::ElseIf(nested) => if_mutates_name(nested, name),
            })
}

fn if_returns_name(statement: &lift_ast::IfStatement, name: &str) -> bool {
    block_returns_name(&statement.then_branch, name)
        || statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_returns_name(block, name),
                ElseBranchKind::ElseIf(nested) => if_returns_name(nested, name),
            })
}

fn expression_contains_owned_name(expression: &Expression, name: &str) -> bool {
    match &expression.kind {
        ExpressionKind::Closure(_) | ExpressionKind::Template(_) => false,
        ExpressionKind::Identifier(identifier) => identifier.name == name,
        ExpressionKind::ObjectLiteral(object) => object
            .fields
            .iter()
            .any(|field| expression_contains_owned_name(&field.value, name)),
        ExpressionKind::ArrayLiteral(array) => array
            .elements
            .iter()
            .any(|element| expression_contains_owned_name(element, name)),
        ExpressionKind::Grouping(inner) => expression_contains_owned_name(inner, name),
        // Campo/elemento calculado e argumento de função não carregam
        // ownership do agregado de origem nesta implementação: lugares são
        // clonados quando convertidos em valores owned.
        ExpressionKind::Unary(_)
        | ExpressionKind::Binary(_)
        | ExpressionKind::FieldAccess(_)
        | ExpressionKind::Call(_)
        | ExpressionKind::Index(_) => false,
        ExpressionKind::IntegerLiteral(_)
        | ExpressionKind::NumberLiteral(_)
        | ExpressionKind::StringLiteral(_)
        | ExpressionKind::BooleanLiteral(_)
        | ExpressionKind::NullLiteral => false,
    }
}

fn root_name(expression: &Expression) -> Option<&str> {
    match &expression.kind {
        ExpressionKind::Identifier(identifier) => Some(&identifier.name),
        ExpressionKind::FieldAccess(access) => root_name(&access.object),
        ExpressionKind::Index(index) => root_name(&index.object),
        ExpressionKind::Grouping(inner) => root_name(inner),
        _ => None,
    }
}

fn collect_forwarded_mut_parameters(
    block: &Block,
    caller: &FunctionInfo,
    functions: &HashMap<String, FunctionInfo>,
    needs_mut: &mut HashSet<String>,
) {
    let mut visible = functions.clone();
    for parameter in &caller.parameters {
        hide_namespace(&mut visible, &parameter.name);
    }
    for statement in &block.statements {
        let functions = &visible;
        match statement {
            Statement::Variable(variable) => {
                collect_forwarded_mut_expr(&variable.initializer, caller, functions, needs_mut)
            }
            Statement::Assignment(assignment) => {
                collect_forwarded_mut_expr(&assignment.target, caller, functions, needs_mut);
                collect_forwarded_mut_expr(&assignment.value, caller, functions, needs_mut);
            }
            Statement::If(statement) => {
                collect_forwarded_mut_if(statement, caller, functions, needs_mut);
            }
            Statement::While(statement) => {
                collect_forwarded_mut_expr(&statement.condition, caller, functions, needs_mut);
                collect_forwarded_mut_parameters(&statement.body, caller, functions, needs_mut);
            }
            Statement::Return(statement) => {
                if let Some(expression) = &statement.value {
                    collect_forwarded_mut_expr(expression, caller, functions, needs_mut);
                }
            }
            Statement::Expression(statement) => {
                collect_forwarded_mut_expr(&statement.expression, caller, functions, needs_mut)
            }
        }
        if let Statement::Variable(variable) = statement {
            hide_namespace(&mut visible, &variable.name.name);
        }
    }
}

fn hide_namespace(functions: &mut HashMap<String, FunctionInfo>, name: &str) {
    let prefix = format!("{name}.");
    functions.retain(|key, _| !key.starts_with(&prefix));
}

fn collect_forwarded_mut_if(
    statement: &lift_ast::IfStatement,
    caller: &FunctionInfo,
    functions: &HashMap<String, FunctionInfo>,
    needs_mut: &mut HashSet<String>,
) {
    collect_forwarded_mut_expr(&statement.condition, caller, functions, needs_mut);
    collect_forwarded_mut_parameters(&statement.then_branch, caller, functions, needs_mut);
    if let Some(branch) = &statement.else_branch {
        match &branch.kind {
            ElseBranchKind::Block(block) => {
                collect_forwarded_mut_parameters(block, caller, functions, needs_mut)
            }
            ElseBranchKind::ElseIf(nested) => {
                collect_forwarded_mut_if(nested, caller, functions, needs_mut)
            }
        }
    }
}

fn collect_forwarded_mut_expr(
    expression: &Expression,
    caller: &FunctionInfo,
    functions: &HashMap<String, FunctionInfo>,
    needs_mut: &mut HashSet<String>,
) {
    match &expression.kind {
        ExpressionKind::Closure(_) => {}
        ExpressionKind::Template(parts) => {
            for part in parts {
                collect_forwarded_mut_expr(part, caller, functions, needs_mut);
            }
        }
        ExpressionKind::Call(call) => {
            if let ExpressionKind::FieldAccess(access) = &call.callee.kind {
                let namespace = direct_callee_name(&access.object)
                    .filter(|name| functions.contains_key(&format!("{name}.")));
                if let Some(alias) = namespace {
                    if let Some(callee) = functions.get(&format!("{alias}.{}", access.field.name)) {
                        for (argument, parameter) in call.arguments.iter().zip(&callee.parameters) {
                            if parameter.pass_mode == ParameterPassMode::BorrowMut {
                                if let Some(root) = root_name(argument) {
                                    if caller
                                        .parameters
                                        .iter()
                                        .any(|p| p.name == root && !p.ty.is_copy())
                                    {
                                        needs_mut.insert(root.to_owned());
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Conservatively union same-named method summaries for projections.
                    let suffix = format!(":{}", access.field.name);
                    let exact = direct_callee_name(&access.object)
                        .and_then(|name| caller.parameters.iter().find(|p| p.name == name))
                        .and_then(|p| {
                            if let ResolvedType::Struct(target) = &p.ty {
                                Some(method_name(target, &access.field.name))
                            } else {
                                None
                            }
                        });
                    for method in functions.values().filter(|f| {
                        exact.as_ref().map_or_else(
                            || f.name.starts_with("$method:") && f.name.ends_with(&suffix),
                            |name| &f.name == name,
                        )
                    }) {
                        let arguments =
                            std::iter::once(access.object.as_ref()).chain(call.arguments.iter());
                        for (argument, parameter) in arguments.zip(&method.parameters) {
                            if parameter.pass_mode == ParameterPassMode::BorrowMut {
                                if let Some(root) = root_name(argument) {
                                    if caller
                                        .parameters
                                        .iter()
                                        .any(|p| p.name == root && !p.ty.is_copy())
                                    {
                                        needs_mut.insert(root.to_owned());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(callee_name) = direct_callee_name(&call.callee) {
                if let Some(callee) = functions.get(&callee_name) {
                    for (argument, parameter) in call.arguments.iter().zip(&callee.parameters) {
                        if parameter.pass_mode != ParameterPassMode::BorrowMut {
                            continue;
                        }
                        if let Some(root) = root_name(argument) {
                            if caller
                                .parameters
                                .iter()
                                .any(|p| p.name == root && !p.ty.is_copy())
                            {
                                needs_mut.insert(root.to_owned());
                            }
                        }
                    }
                }
            }
            collect_forwarded_mut_expr(&call.callee, caller, functions, needs_mut);
            for argument in &call.arguments {
                collect_forwarded_mut_expr(argument, caller, functions, needs_mut);
            }
        }
        ExpressionKind::ObjectLiteral(object) => {
            for field in &object.fields {
                collect_forwarded_mut_expr(&field.value, caller, functions, needs_mut);
            }
        }
        ExpressionKind::ArrayLiteral(array) => {
            for element in &array.elements {
                collect_forwarded_mut_expr(element, caller, functions, needs_mut);
            }
        }
        ExpressionKind::Unary(unary) => {
            collect_forwarded_mut_expr(&unary.operand, caller, functions, needs_mut)
        }
        ExpressionKind::Binary(binary) => {
            collect_forwarded_mut_expr(&binary.left, caller, functions, needs_mut);
            collect_forwarded_mut_expr(&binary.right, caller, functions, needs_mut);
        }
        ExpressionKind::FieldAccess(access) => {
            collect_forwarded_mut_expr(&access.object, caller, functions, needs_mut)
        }
        ExpressionKind::Index(index) => {
            collect_forwarded_mut_expr(&index.object, caller, functions, needs_mut);
            collect_forwarded_mut_expr(&index.index, caller, functions, needs_mut);
        }
        ExpressionKind::Grouping(inner) => {
            collect_forwarded_mut_expr(inner, caller, functions, needs_mut)
        }
        ExpressionKind::IntegerLiteral(_)
        | ExpressionKind::NumberLiteral(_)
        | ExpressionKind::StringLiteral(_)
        | ExpressionKind::BooleanLiteral(_)
        | ExpressionKind::Identifier(_)
        | ExpressionKind::NullLiteral => {}
    }
}

fn block_guarantees_return(block: &Block) -> bool {
    for statement in &block.statements {
        match statement {
            Statement::Return(_) => return true,
            Statement::If(statement) => {
                if if_guarantees_return(statement) {
                    return true;
                }
            }
            // Um while pode executar zero vezes.
            _ => {}
        }
    }
    false
}

fn if_guarantees_return(statement: &lift_ast::IfStatement) -> bool {
    block_guarantees_return(&statement.then_branch)
        && statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_guarantees_return(block),
                ElseBranchKind::ElseIf(nested) => if_guarantees_return(nested),
            })
}

fn visit_struct(
    name: &str,
    structures: &HashMap<String, StructInfo>,
    state: &mut HashMap<String, u8>,
    stack: &mut Vec<String>,
) -> Option<Span> {
    match state.get(name).copied().unwrap_or(0) {
        1 => return structures.get(name).map(|info| info.span),
        2 => return None,
        _ => {}
    }
    state.insert(name.to_owned(), 1);
    stack.push(name.to_owned());
    if let Some(info) = structures.get(name) {
        for field in &info.fields {
            let mut children = Vec::new();
            direct_struct_dependencies(&field.ty, &mut children);
            for child in children {
                if state.get(child).copied() == Some(1) {
                    stack.push(child.to_owned());
                    return Some(field.span);
                }
                if let Some(span) = visit_struct(child, structures, state, stack) {
                    return Some(span);
                }
            }
        }
    }
    stack.pop();
    state.insert(name.to_owned(), 2);
    None
}

fn direct_struct_dependencies<'a>(ty: &'a ResolvedType, children: &mut Vec<&'a str>) {
    match ty {
        ResolvedType::Struct(name) => children.push(name),
        ResolvedType::Result(ok, error) => {
            direct_struct_dependencies(ok, children);
            direct_struct_dependencies(error, children);
        }
        ResolvedType::Optional(payload) => direct_struct_dependencies(payload, children),
        _ => {}
    }
}

fn apply_mutable_binding_flags(declarations: &mut [HirDeclaration], mutable: &HashSet<BindingId>) {
    fn apply_block(block: &mut HirBlock, mutable: &HashSet<BindingId>) {
        for statement in &mut block.statements {
            match statement {
                HirStatement::Variable(variable) => {
                    variable.requires_mut_binding = mutable.contains(&variable.id);
                }
                HirStatement::If(statement) => {
                    apply_block(&mut statement.then_branch, mutable);
                    if let Some(branch) = &mut statement.else_branch {
                        match branch {
                            HirElseBranch::Block(block) => apply_block(block, mutable),
                            HirElseBranch::ElseIf(nested) => apply_if(nested, mutable),
                        }
                    }
                }
                HirStatement::While(statement) => apply_block(&mut statement.body, mutable),
                HirStatement::Assignment(_)
                | HirStatement::Return(_)
                | HirStatement::Expression(_) => {}
            }
        }
    }
    fn apply_if(statement: &mut HirIf, mutable: &HashSet<BindingId>) {
        apply_block(&mut statement.then_branch, mutable);
        if let Some(branch) = &mut statement.else_branch {
            match branch {
                HirElseBranch::Block(block) => apply_block(block, mutable),
                HirElseBranch::ElseIf(nested) => apply_if(nested, mutable),
            }
        }
    }
    for declaration in declarations {
        match declaration {
            HirDeclaration::Function(function) => apply_block(&mut function.body, mutable),
            HirDeclaration::Statement(statement) => {
                let mut one = HirBlock {
                    statements: vec![statement.clone()],
                    span: statement.span(),
                };
                apply_block(&mut one, mutable);
                if let Some(rewritten) = one.statements.pop() {
                    *statement = rewritten;
                }
            }
            HirDeclaration::Struct(_) | HirDeclaration::Import(_) => {}
        }
    }
}
