//! Name/type resolution and conservative ownership inference from AST to HIR.
//!
//! PT-BR: Resolução de nomes/tipos e inferência conservadora de ownership de AST para HIR.

use std::collections::{HashMap, HashSet};

use lift_ast::{
    ArrayLiteral, AssignmentStatement, BinaryOperator, BindingKind, Block, Declaration,
    ElseBranchKind, Expression, ExpressionKind, FunctionDeclaration, Identifier,
    ImportInterfaceDeclaration, ProgramAst, Span, Statement, TypeExpression, TypeExpressionKind,
    UnaryOperator,
};

use crate::{
    ArgPassMode, ArrayMethodKind, BindingId, CallTempId, CloneMode, CoalesceMode, ExprDiscardMode,
    HirAssignment, HirBlock, HirBreak, HirCallArgument, HirContinue, HirDeclaration, HirElseBranch,
    HirEnum, HirEnumNamedField, HirEnumVariant, HirExpr, HirExprKind, HirExpressionStatement,
    HirExternStruct, HirForOf, HirFunction, HirIf, HirImport, HirMatch, HirMatchArm,
    HirMatchBinding, HirMatchPattern, HirNarrowing, HirParameter, HirProgram, HirReturn,
    HirStatement, HirStruct, HirStructField, HirStructInitField, HirThrow, HirTryCatch,
    HirVariableDeclaration, HirWhile, LoopId, MapMethodKind, ParameterPassMode, PropagateTarget,
    ResolvedType, RustDependency, SemaError, SemaErrorKind, TryId, CONSOLE_LOG_BUILTIN,
    OPTION_IS_SOME_BUILTIN, OPTION_UNWRAP_BUILTIN, RESULT_IS_OK_BUILTIN, RESULT_UNWRAP_BUILTIN,
    RESULT_UNWRAP_ERR_BUILTIN,
};

fn method_name(target: &str, method: &str) -> String {
    format!("$method:{target}:{method}")
}

fn inject_builtin_map(ast: &mut ProgramAst) {
    if ast
        .declarations
        .iter()
        .any(|declaration| match declaration {
            Declaration::Interface(interface) => interface.name.name == "Map",
            Declaration::Enum(enumeration) => enumeration.name.name == "Map",
            Declaration::Union(union) => union.name.name == "Map",
            Declaration::ImportInterface(interface) => interface.name.name == "Map",
            _ => false,
        })
    {
        return;
    }
    let span = Span::default();
    ast.declarations.push(Declaration::ImportInterface(
        lift_ast::ImportInterfaceDeclaration {
            name: Identifier {
                name: "Map".into(),
                span,
            },
            type_parameters: vec![
                Identifier {
                    name: "K".into(),
                    span,
                },
                Identifier {
                    name: "V".into(),
                    span,
                },
            ],
            type_arguments: Vec::new(),
            fields: Vec::new(),
            source: "lift_core::collections::BTreeMap".into(),
            source_span: span,
            span,
        },
    ));
}

fn lower_impls(mut ast: ProgramAst) -> Result<ProgramAst, Vec<SemaError>> {
    let names = ast
        .declarations
        .iter()
        .filter_map(|d| match d {
            Declaration::Interface(i) => Some(i.name.name.clone()),
            Declaration::ImportInterface(i) => Some(i.name.name.clone()),
            _ => None,
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
            ExpressionKind::TryPropagate(inner) => expression(inner, bound, free),
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
                Statement::Match(m) => {
                    expression(&m.value, bound, free);
                    for arm in &m.arms {
                        let mut arm_bound = bound.clone();
                        match &arm.pattern.kind {
                            lift_ast::MatchPatternKind::Some(name)
                            | lift_ast::MatchPatternKind::Err(name) => {
                                arm_bound.insert(name.name.clone());
                            }
                            lift_ast::MatchPatternKind::Ok(Some(name)) => {
                                arm_bound.insert(name.name.clone());
                            }
                            lift_ast::MatchPatternKind::EnumVariant { bindings, .. } => {
                                for binding in bindings {
                                    arm_bound.insert(binding.name.clone());
                                }
                            }
                            lift_ast::MatchPatternKind::None
                            | lift_ast::MatchPatternKind::Ok(None) => {}
                        }
                        block(&arm.body, &mut arm_bound, free);
                    }
                }
                Statement::ForOf(f) => {
                    expression(&f.iterable, bound, free);
                    let mut for_bound = bound.clone();
                    for_bound.insert(f.name.name.clone());
                    block(&f.body, &mut for_bound, free);
                }
                Statement::Break(_) | Statement::Continue(_) => {}
                Statement::If(i) => conditional(i, bound, free),
                Statement::Throw(t) => expression(&t.value, bound, free),
                Statement::TryCatch(t) => {
                    block(&t.body, &mut bound.clone(), free);
                    let mut catch_bound = bound.clone();
                    catch_bound.insert(t.catch_name.name.clone());
                    block(&t.catch_body, &mut catch_bound, free);
                }
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
    /// Tipo Rust externo: não gera `struct` e não participa do ciclo de tamanho.
    /// PT-BR: tipo externo; sem geração e sem aresta no grafo de tamanho.
    external: bool,
}

#[derive(Clone)]
struct EnumInfo {
    variants: Vec<EnumVariantInfo>,
}

#[derive(Clone)]
struct EnumVariantInfo {
    name: String,
    fields: Vec<ResolvedType>,
    field_boxed: Vec<bool>,
    named_fields: Vec<HirEnumNamedField>,
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
    pub(super) type_parameters: Vec<String>,
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
    /// Binding derivado de narrowing: cópia somente leitura do payload.
    /// PT-BR: binding derivado de narrowing; cópia somente leitura.
    narrowed: bool,
    span: Span,
}

impl BindingInfo {
    /// Somente bindings imutáveis podem ser estreitados: `const` e parâmetros.
    /// PT-BR: narrowing exige binding imutável (`const` ou parâmetro).
    fn is_narrowable(&self) -> bool {
        !self.narrowed
            && (self.binding == Some(BindingKind::Const)
                || (self.binding.is_none() && self.parameter_mode.is_some()))
    }
}

/// Fato de fluxo `x: T` derivado de um teste sobre `x: T?`.
/// PT-BR: fato de fluxo que estreita `T?` para `T`.
#[derive(Clone)]
struct NarrowFact {
    source: BindingId,
    name: String,
    payload: ResolvedType,
    span: Span,
}

struct Analyzer<'ast> {
    ast: &'ast ProgramAst,
    structures: HashMap<String, StructInfo>,
    enums: HashMap<String, EnumInfo>,
    functions: HashMap<String, FunctionInfo>,
    struct_by_declaration: HashMap<usize, HirStruct>,
    enum_by_declaration: HashMap<usize, HirEnum>,
    extern_by_declaration: HashMap<usize, HirExternStruct>,
    extern_dependencies: HashMap<String, (RustDependency, Span)>,
    function_by_declaration: HashMap<usize, FunctionInfo>,
    import_by_declaration: HashMap<usize, HirImport>,
    errors: Vec<SemaError>,
    next_binding_id: u32,
    next_try_id: u32,
    next_loop_id: u32,
    mutable_bindings: HashSet<BindingId>,
    shared_capture_sources: HashSet<BindingId>,
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
    let mut ast = ast;
    inject_builtin_map(&mut ast);
    let mut generic_errors = Vec::new();
    crate::generic_function_specialization::specialize_generic_functions(
        &mut ast,
        &mut generic_errors,
    );
    crate::generic_impl_specialization::specialize_generic_impls(&mut ast, &mut generic_errors);
    crate::generic_containers::expand_generic_containers(&mut ast, &mut generic_errors);
    if !generic_errors.is_empty() {
        return Err(generic_errors);
    }
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
            enums: HashMap::new(),
            functions: HashMap::new(),
            struct_by_declaration: HashMap::new(),
            enum_by_declaration: HashMap::new(),
            extern_by_declaration: HashMap::new(),
            extern_dependencies: HashMap::new(),
            function_by_declaration: HashMap::new(),
            import_by_declaration: HashMap::new(),
            errors: Vec::new(),
            next_binding_id: 0,
            next_try_id: 0,
            next_loop_id: 0,
            mutable_bindings: HashSet::new(),
            shared_capture_sources: HashSet::new(),
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
        let empty_type_parameters = HashSet::new();
        {
            let mut top_level = BodyChecker::new(
                &empty_type_parameters,
                &self.structures,
                &self.enums,
                &self.functions,
                &mut self.next_binding_id,
                &mut self.next_try_id,
                &mut self.next_loop_id,
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
                Declaration::Enum(_) | Declaration::Union(_) => {
                    if let Some(item) = self.enum_by_declaration.get(&index) {
                        declarations.push(HirDeclaration::Enum(item.clone()));
                    }
                }
                Declaration::ImportInterface(_) => {
                    if let Some(item) = self.extern_by_declaration.get(&index) {
                        declarations.push(HirDeclaration::ExternStruct(item.clone()));
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
            let name = match declaration {
                Declaration::Interface(interface) => &interface.name,
                Declaration::ImportInterface(interface) => &interface.name,
                Declaration::Enum(enumeration) => &enumeration.name,
                Declaration::Union(union) => &union.name,
                _ => continue,
            };
            let name_text = name.name.clone();
            if matches!(
                name_text.as_str(),
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
                    name.span,
                ));
            }
            if let Some(previous) = declared.insert(name_text.clone(), name.span) {
                self.errors.push(
                    SemaError::new(
                        SemaErrorKind::DuplicateDeclaration,
                        format!("declaração duplicada de `{name_text}`"),
                        name.span,
                    )
                    .with_secondary(previous),
                );
                continue;
            }
        }
    }

    fn resolve_structs(&mut self) {
        let mut names = HashSet::new();
        let mut extern_names = HashSet::new();
        let mut enum_names = HashSet::new();
        for declaration in &self.ast.declarations {
            match declaration {
                Declaration::Interface(interface) => {
                    names.insert(interface.name.name.clone());
                }
                Declaration::ImportInterface(interface) => {
                    names.insert(interface.name.name.clone());
                    extern_names.insert(interface.name.name.clone());
                }
                Declaration::Enum(enumeration) => {
                    names.insert(enumeration.name.name.clone());
                    enum_names.insert(enumeration.name.name.clone());
                }
                Declaration::Union(union) => {
                    names.insert(union.name.name.clone());
                    enum_names.insert(union.name.name.clone());
                }
                _ => {}
            }
        }

        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            match declaration {
                Declaration::Interface(interface) => {
                    let Some(fields) = self.resolve_struct_fields(
                        &interface.fields,
                        &names,
                        &extern_names,
                        &enum_names,
                        false,
                    ) else {
                        continue;
                    };
                    let info = StructInfo {
                        fields: fields.clone(),
                        span: interface.span,
                        external: false,
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
                Declaration::ImportInterface(interface) => {
                    self.resolve_import_interface(
                        index,
                        interface,
                        &names,
                        &extern_names,
                        &enum_names,
                    );
                }
                _ => {}
            }
        }
        self.resolve_enums(&names, &enum_names);
        self.resolve_unions(&names, &enum_names);
    }

    /// Resolve os payloads das variantes; enums não podem conter enums nesta fase.
    fn resolve_enums(&mut self, names: &HashSet<String>, enum_names: &HashSet<String>) {
        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            let Declaration::Enum(enumeration) = declaration else {
                continue;
            };
            let mut variants = Vec::new();
            let mut seen = HashMap::<String, Span>::new();
            let mut valid = true;
            if enumeration.variants.is_empty() {
                self.errors.push(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    format!(
                        "enum `{}` precisa de ao menos uma variante",
                        enumeration.name.name
                    ),
                    enumeration.name.span,
                ));
                continue;
            }
            for variant in &enumeration.variants {
                if let Some(previous) = seen.insert(variant.name.name.clone(), variant.name.span) {
                    self.errors.push(
                        SemaError::new(
                            SemaErrorKind::DuplicateDeclaration,
                            format!(
                                "variante duplicada `{}` em `{}`",
                                variant.name.name, enumeration.name.name
                            ),
                            variant.name.span,
                        )
                        .with_secondary(previous),
                    );
                    valid = false;
                    continue;
                }
                let mut fields = Vec::new();
                for field in &variant.fields {
                    match resolve_type(field, names, &HashSet::new(), TypePosition::Field) {
                        Ok(ty) => {
                            if ty.needs_db_runtime() {
                                self.errors.push(SemaError::new(
                                    SemaErrorKind::UnsupportedType,
                                    "payload de enum não pode usar DbValue nem recursos",
                                    field.span,
                                ));
                                valid = false;
                            } else if !ty.is_value_type() {
                                self.errors.push(SemaError::new(
                                    SemaErrorKind::UnsupportedType,
                                    "payload de enum exige tipo de valor",
                                    field.span,
                                ));
                                valid = false;
                            } else {
                                fields.push(ty);
                            }
                        }
                        Err(error) => {
                            self.errors.push(error);
                            valid = false;
                        }
                    }
                }
                let mut named_fields = Vec::new();
                let mut named_seen = HashMap::<String, Span>::new();
                for field in &variant.named_fields {
                    if let Some(previous) =
                        named_seen.insert(field.name.name.clone(), field.name.span)
                    {
                        self.errors.push(
                            SemaError::new(
                                SemaErrorKind::DuplicateField,
                                format!(
                                    "campo `{}` duplicado na variante `{}`",
                                    field.name.name, variant.name.name
                                ),
                                field.name.span,
                            )
                            .with_secondary(previous),
                        );
                        valid = false;
                        continue;
                    }
                    match resolve_type(&field.ty, names, &HashSet::new(), TypePosition::Field) {
                        Ok(ty) => {
                            if ty.needs_db_runtime() {
                                self.errors.push(SemaError::new(
                                    SemaErrorKind::UnsupportedType,
                                    "payload de enum não pode usar DbValue nem recursos",
                                    field.ty.span,
                                ));
                                valid = false;
                            } else if !ty.is_value_type() {
                                self.errors.push(SemaError::new(
                                    SemaErrorKind::UnsupportedType,
                                    "payload de enum exige tipo de valor",
                                    field.ty.span,
                                ));
                                valid = false;
                            } else {
                                named_fields.push(HirEnumNamedField {
                                    name: field.name.name.clone(),
                                    ty,
                                    boxed: false,
                                    span: field.span,
                                });
                            }
                        }
                        Err(error) => {
                            self.errors.push(error);
                            valid = false;
                        }
                    }
                }
                let field_boxed = fields
                    .iter()
                    .map(|ty| {
                        let mut seen = HashSet::new();
                        type_needs_box(ty, enum_names, &self.structures, &mut seen)
                    })
                    .collect::<Vec<_>>();
                for field in &mut named_fields {
                    let mut seen = HashSet::new();
                    field.boxed =
                        type_needs_box(&field.ty, enum_names, &self.structures, &mut seen);
                }
                variants.push(EnumVariantInfo {
                    name: variant.name.name.clone(),
                    fields,
                    field_boxed,
                    named_fields,
                    span: variant.span,
                });
            }
            if valid {
                self.enums.insert(
                    enumeration.name.name.clone(),
                    EnumInfo {
                        variants: variants.clone(),
                    },
                );
                self.enum_by_declaration.insert(
                    index,
                    HirEnum {
                        name: enumeration.name.name.clone(),
                        variants: variants
                            .into_iter()
                            .map(|variant| HirEnumVariant {
                                name: variant.name,
                                fields: variant.fields,
                                field_boxed: variant.field_boxed,
                                named_fields: variant.named_fields,
                                span: variant.span,
                            })
                            .collect(),
                        span: enumeration.span,
                    },
                );
            }
        }
    }

    /// `type Nome = A | B;` vira um `HirEnum` com uma variante por membro.
    fn resolve_unions(&mut self, names: &HashSet<String>, enum_names: &HashSet<String>) {
        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            let Declaration::Union(union) = declaration else {
                continue;
            };
            let mut variants = Vec::new();
            let mut seen = HashMap::<String, Span>::new();
            let mut valid = true;
            if union.members.is_empty() {
                self.errors.push(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    format!("união `{}` precisa de ao menos um membro", union.name.name),
                    union.name.span,
                ));
                continue;
            }
            for member in &union.members {
                let lift_ast::TypeExpressionKind::Named(identifier) = &member.kind else {
                    self.errors.push(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "união exige tipos nomeados como membros",
                        member.span,
                    ));
                    valid = false;
                    continue;
                };
                let ty = match resolve_type(member, names, &HashSet::new(), TypePosition::Field) {
                    Ok(ty) => ty,
                    Err(error) => {
                        self.errors.push(error);
                        valid = false;
                        continue;
                    }
                };
                let ResolvedType::Struct(member_name) = &ty else {
                    self.errors.push(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        format!(
                            "membro `{}` de união precisa ser um tipo nominal (interface/enum)",
                            identifier.name
                        ),
                        member.span,
                    ));
                    valid = false;
                    continue;
                };
                if let Some(previous) = seen.insert(member_name.clone(), member.span) {
                    self.errors.push(
                        SemaError::new(
                            SemaErrorKind::DuplicateDeclaration,
                            format!(
                                "membro duplicado `{member_name}` na união `{}`",
                                union.name.name
                            ),
                            member.span,
                        )
                        .with_secondary(previous),
                    );
                    valid = false;
                    continue;
                }
                let mut visited = HashSet::new();
                let boxed = type_needs_box(&ty, enum_names, &self.structures, &mut visited);
                variants.push(EnumVariantInfo {
                    name: member_name.clone(),
                    fields: vec![ty],
                    field_boxed: vec![boxed],
                    named_fields: Vec::new(),
                    span: member.span,
                });
            }
            if valid {
                self.enums.insert(
                    union.name.name.clone(),
                    EnumInfo {
                        variants: variants.clone(),
                    },
                );
                self.enum_by_declaration.insert(
                    index,
                    HirEnum {
                        name: union.name.name.clone(),
                        variants: variants
                            .into_iter()
                            .map(|variant| HirEnumVariant {
                                name: variant.name,
                                fields: variant.fields,
                                field_boxed: variant.field_boxed,
                                named_fields: Vec::new(),
                                span: variant.span,
                            })
                            .collect(),
                        span: union.span,
                    },
                );
            }
        }
    }

    /// Campos de interface validados para o tipo local ou para o layout externo.
    /// PT-BR: campos validados para struct local ou layout externo.
    fn resolve_struct_fields(
        &mut self,
        source: &[lift_ast::InterfaceField],
        names: &HashSet<String>,
        extern_names: &HashSet<String>,
        _enum_names: &HashSet<String>,
        external: bool,
    ) -> Option<Vec<HirStructField>> {
        let mut fields = Vec::new();
        let mut field_names = HashMap::<String, Span>::new();
        let mut valid = true;
        for field in source {
            if let Some(previous) = field_names.insert(field.name.name.clone(), field.name.span) {
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
            if external && !crate::interop::rust_identifier(&field.name.name) {
                self.errors.push(SemaError::new(
                    SemaErrorKind::InvalidImport,
                    format!(
                        "campo externo `{}` precisa ser um identificador Rust sem palavra reservada",
                        field.name.name
                    ),
                    field.name.span,
                ));
                valid = false;
                continue;
            }
            match resolve_type(&field.ty, names, &HashSet::new(), TypePosition::Field) {
                Ok(ty) => {
                    if external {
                        if let Err(error) =
                            crate::interop::validate_field_type(&ty, field.ty.span, extern_names)
                        {
                            self.errors.push(error);
                            valid = false;
                            continue;
                        }
                    }

                    let ty = if field.optional {
                        // `field?: T` é o mesmo tipo Rust de `field: T?`, mas
                        // pode ser omitido no object literal. Um aninhamento
                        // direto (`field?: T?`) continua rejeitado.
                        if matches!(ty, ResolvedType::Optional(_)) || !ty.is_value_type() {
                            self.errors.push(SemaError::new(
                                SemaErrorKind::UnsupportedType,
                                "campo opcional `?` exige um tipo T não opcional e não void",
                                field.ty.span,
                            ));
                            valid = false;
                            continue;
                        }
                        ResolvedType::Optional(Box::new(ty))
                    } else {
                        ty
                    };
                    fields.push(HirStructField {
                        name: field.name.name.clone(),
                        ty,
                        optional: field.optional,
                        span: field.span,
                    });
                }
                Err(error) => {
                    self.errors.push(error);
                    valid = false;
                }
            }
        }
        valid.then_some(fields)
    }

    fn resolve_import_interface(
        &mut self,
        index: usize,
        interface: &ImportInterfaceDeclaration,
        names: &HashSet<String>,
        extern_names: &HashSet<String>,
        enum_names: &HashSet<String>,
    ) {
        let (dependency, rust_path) =
            match crate::interop::resolve_source(&interface.source, interface.source_span) {
                Ok(resolved) => resolved,
                Err(error) => {
                    self.errors.push(error);
                    return;
                }
            };
        let rust_path = if interface.type_arguments.is_empty() {
            rust_path
        } else {
            let mut arguments = Vec::new();
            for argument in &interface.type_arguments {
                match resolve_type(argument, names, &HashSet::new(), TypePosition::Field) {
                    Ok(ResolvedType::Int) => arguments.push("i64".to_owned()),
                    Ok(ResolvedType::Number) => arguments.push("f64".to_owned()),
                    Ok(ResolvedType::String) => arguments.push("String".to_owned()),
                    Ok(ResolvedType::Boolean) => arguments.push("bool".to_owned()),
                    Ok(other) => {
                        self.errors.push(SemaError::new(
                            SemaErrorKind::UnsupportedType,
                            format!(
                                "argumento `{other}` não é suportado em interface externa genérica"
                            ),
                            argument.span,
                        ));
                        return;
                    }
                    Err(error) => {
                        self.errors.push(error);
                        return;
                    }
                }
            }
            format!("{rust_path}<{}>", arguments.join(", "))
        };
        let crate_name = rust_path.split("::").next().unwrap_or_default().to_owned();
        if let Some((previous, previous_span)) = self.extern_dependencies.get(&crate_name) {
            if previous != &dependency {
                self.errors.push(
                    SemaError::new(
                        SemaErrorKind::InvalidImport,
                        format!("dependências incompatíveis para a crate `{crate_name}`"),
                        interface.source_span,
                    )
                    .with_secondary(*previous_span),
                );
                return;
            }
        } else {
            self.extern_dependencies
                .insert(crate_name, (dependency.clone(), interface.source_span));
        }
        let Some(fields) =
            self.resolve_struct_fields(&interface.fields, names, extern_names, enum_names, true)
        else {
            return;
        };
        let info = StructInfo {
            fields: fields.clone(),
            span: interface.span,
            external: true,
        };
        self.structures.insert(interface.name.name.clone(), info);
        self.extern_by_declaration.insert(
            index,
            HirExternStruct {
                name: interface.name.name.clone(),
                fields,
                dependency,
                rust_path,
                span: interface.span,
            },
        );
    }

    fn collect_functions(&mut self) {
        let mut all_names = HashMap::<String, Span>::new();
        for declaration in &self.ast.declarations {
            match declaration {
                Declaration::Interface(lift_ast::InterfaceDeclaration {
                    name: identifier, ..
                })
                | Declaration::ImportInterface(ImportInterfaceDeclaration {
                    name: identifier,
                    ..
                })
                | Declaration::Function(FunctionDeclaration {
                    name: identifier, ..
                })
                | Declaration::Import(lift_ast::ImportFunctionDeclaration {
                    name: identifier,
                    ..
                })
                | Declaration::Enum(lift_ast::EnumDeclaration {
                    name: identifier, ..
                })
                | Declaration::Union(lift_ast::UnionDeclaration {
                    name: identifier, ..
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
            .chain(self.enums.keys().cloned())
            .chain(["int", "number", "string", "boolean", "void"].map(str::to_owned))
            .collect::<HashSet<_>>();

        for (index, declaration) in self.ast.declarations.iter().enumerate() {
            let Declaration::Function(function) = declaration else {
                continue;
            };
            let mut type_parameters = Vec::new();
            let mut type_parameter_names = HashSet::new();
            let mut valid = true;
            for parameter in &function.type_parameters {
                if !type_parameter_names.insert(parameter.name.clone()) {
                    self.errors.push(SemaError::new(
                        SemaErrorKind::DuplicateParameter,
                        format!("parâmetro de tipo duplicado `{}`", parameter.name),
                        parameter.span,
                    ));
                    valid = false;
                } else {
                    type_parameters.push(parameter.name.clone());
                }
            }
            let mut parameters = Vec::new();
            let mut seen = HashMap::<String, Span>::new();
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
                match resolve_type(
                    &parameter.ty,
                    &struct_names,
                    &type_parameter_names,
                    TypePosition::Parameter,
                ) {
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
                Some(ty) => match resolve_type(
                    ty,
                    &struct_names,
                    &type_parameter_names,
                    TypePosition::Return,
                ) {
                    Ok(ty) => ty,
                    Err(error) => {
                        self.errors.push(error);
                        valid = false;
                        ResolvedType::Void
                    }
                },
                None => ResolvedType::Void,
            };

            for parameter in &type_parameters {
                let appears_in_input = parameters
                    .iter()
                    .any(|signature| type_contains_param(&signature.ty, parameter));
                if !appears_in_input {
                    self.errors.push(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        format!(
                            "parâmetro de tipo `{parameter}` precisa aparecer em um parâmetro da função"
                        ),
                        function.name.span,
                    ));
                    valid = false;
                }
            }
            if valid {
                let info = FunctionInfo {
                    name: function.name.name.clone(),
                    type_parameters,
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
        let names = self
            .structures
            .keys()
            .cloned()
            .chain(self.enums.keys().cloned())
            .collect::<HashSet<_>>();
        let extern_names = self
            .structures
            .iter()
            .filter(|(_, info)| info.external)
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();
        let mut dependencies = self.extern_dependencies.clone();
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
                    let ty = resolve_type(
                        &parameter.ty,
                        &names,
                        &HashSet::new(),
                        TypePosition::Parameter,
                    )?;
                    if resource_signature.is_none() {
                        crate::interop::validate_type(
                            &ty,
                            false,
                            parameter.ty.span,
                            &extern_names,
                        )?;
                    }
                    let pass_mode = if ty.contains_closure() {
                        // Callback FFI: `Rc<dyn Fn>` por valor; o wrapper empresta
                        // `&dyn Fn` ao ponteiro Rust declarado.
                        ParameterPassMode::ByValue
                    } else if ty.is_copy() {
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
                let return_type = resolve_type(
                    &import.return_type,
                    &names,
                    &HashSet::new(),
                    TypePosition::Return,
                )?;
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
                    crate::interop::validate_type(
                        &return_type,
                        true,
                        import.return_type.span,
                        &extern_names,
                    )?;
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
                    type_parameters: Vec::new(),
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
                if parameter.ty.contains_closure() {
                    // Callbacks são `Rc<dyn Fn>`: passados por valor com clone barato.
                    parameter.pass_mode = ParameterPassMode::ByValue;
                    parameter.mutated = false;
                    continue;
                }
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
        let type_parameter_set = signature
            .type_parameters
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let mut checker = BodyChecker::new(
            &type_parameter_set,
            &self.structures,
            &self.enums,
            &self.functions,
            &mut self.next_binding_id,
            &mut self.next_try_id,
            &mut self.next_loop_id,
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
                    narrowed: false,
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
            && !block_guarantees_return(&declaration.body, 0)
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
            type_parameters: signature.type_parameters,
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

/// Contexto de um bloco `try` ativo durante a análise do corpo.
#[derive(Clone)]
struct TryContext {
    id: TryId,
    error: ResolvedType,
}

struct BodyChecker<'a> {
    type_parameters: &'a HashSet<String>,
    structures: &'a HashMap<String, StructInfo>,
    enums: &'a HashMap<String, EnumInfo>,
    functions: &'a HashMap<String, FunctionInfo>,
    next_binding_id: &'a mut u32,
    next_try_id: &'a mut u32,
    mutable_bindings: &'a mut HashSet<BindingId>,
    scopes: Vec<HashMap<String, BindingInfo>>,
    function: Option<FunctionContext>,
    try_stack: Vec<TryContext>,
    next_loop_id: &'a mut u32,
    loop_stack: Vec<LoopId>,
}

impl<'a> BodyChecker<'a> {
    fn new(
        type_parameters: &'a HashSet<String>,
        structures: &'a HashMap<String, StructInfo>,
        enums: &'a HashMap<String, EnumInfo>,
        functions: &'a HashMap<String, FunctionInfo>,
        next_binding_id: &'a mut u32,
        next_try_id: &'a mut u32,
        next_loop_id: &'a mut u32,
        mutable_bindings: &'a mut HashSet<BindingId>,
        function: Option<FunctionContext>,
    ) -> Self {
        Self {
            type_parameters,
            structures,
            enums,
            functions,
            next_binding_id,
            next_try_id,
            mutable_bindings,
            scopes: vec![HashMap::new()],
            function,
            try_stack: Vec::new(),
            next_loop_id,
            loop_stack: Vec::new(),
        }
    }

    fn fresh_binding_id(&mut self) -> BindingId {
        let id = BindingId(*self.next_binding_id);
        *self.next_binding_id = self.next_binding_id.saturating_add(1);
        id
    }

    fn fresh_try_id(&mut self) -> TryId {
        let id = TryId(*self.next_try_id);
        *self.next_try_id = self.next_try_id.saturating_add(1);
        id
    }

    fn fresh_loop_id(&mut self) -> LoopId {
        let id = LoopId(*self.next_loop_id);
        *self.next_loop_id = self.next_loop_id.saturating_add(1);
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
        let statements = self.check_block_statements(&block.statements);
        if create_scope {
            self.scopes.pop();
        }
        Ok(HirBlock {
            statements: statements?,
            span: block.span,
        })
    }

    /// Bloco com fatos de fluxo aplicados antes dos statements da fonte.
    /// PT-BR: bloco com bindings derivados de narrowing antes dos statements.
    fn check_block_narrowed(
        &mut self,
        block: &Block,
        create_scope: bool,
        facts: &[NarrowFact],
    ) -> Result<HirBlock, SemaError> {
        if create_scope {
            self.scopes.push(HashMap::new());
        }
        let statements = (|| {
            let mut statements = self.push_narrowed(facts);
            statements.extend(self.check_block_statements(&block.statements)?);
            Ok(statements)
        })();
        if create_scope {
            self.scopes.pop();
        }
        Ok(HirBlock {
            statements: statements?,
            span: block.span,
        })
    }

    fn check_block_statements(
        &mut self,
        source: &[Statement],
    ) -> Result<Vec<HirStatement>, SemaError> {
        let function = self.function.clone();
        let mut statements = Vec::with_capacity(source.len());
        for (index, statement) in source.iter().enumerate() {
            statements.push(self.check_statement(statement, function.as_ref())?);
            let Statement::If(conditional) = statement else {
                continue;
            };
            // Um `if` que retorna em um dos braços estreita o tipo no restante
            // do bloco, como em `if x == null { return; }`. O clone só é emitido
            // quando o nome ainda aparece nos statements seguintes.
            let facts = self
                .if_fallthrough_facts(conditional)
                .into_iter()
                .filter(|fact| {
                    source[index + 1..]
                        .iter()
                        .any(|statement| statement_mentions_name(statement, &fact.name))
                })
                .collect::<Vec<_>>();
            statements.extend(self.push_narrowed(&facts));
        }
        Ok(statements)
    }

    /// Declara bindings derivados somente leitura para os fatos recebidos.
    /// PT-BR: cria os bindings derivados de narrowing no escopo atual.
    fn push_narrowed(&mut self, facts: &[NarrowFact]) -> Vec<HirStatement> {
        let mut statements = Vec::with_capacity(facts.len());
        let mut seen = HashSet::new();
        for fact in facts {
            if !seen.insert(fact.source) {
                continue;
            }
            // Fatos só valem enquanto o nome ainda resolve para o mesmo binding
            // imutável; bindings de blocos internos já saíram de escopo.
            if !self
                .lookup(&fact.name)
                .is_some_and(|binding| binding.id == fact.source && binding.is_narrowable())
            {
                continue;
            }
            let id = self.fresh_binding_id();
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(
                    fact.name.clone(),
                    BindingInfo {
                        id,
                        ty: fact.payload.clone(),
                        binding: Some(BindingKind::Const),
                        parameter_mode: None,
                        parameter_mutated: false,
                        narrowed: true,
                        span: fact.span,
                    },
                );
            }
            statements.push(HirStatement::Narrow(HirNarrowing {
                id,
                source: fact.source,
                ty: fact.payload.clone(),
                span: fact.span,
            }));
        }
        statements
    }

    /// Fatos de fluxo inferidos de uma condição de teste.
    /// PT-BR: fatos verdadeiros/falsos inferidos da condição.
    fn condition_facts(&self, condition: &Expression) -> (Vec<NarrowFact>, Vec<NarrowFact>) {
        match &condition.kind {
            ExpressionKind::Grouping(inner) => self.condition_facts(inner),
            ExpressionKind::Unary(unary) if unary.operator == UnaryOperator::Not => {
                let (truthy, falsy) = self.condition_facts(&unary.operand);
                (falsy, truthy)
            }
            ExpressionKind::Binary(binary) if binary.operator == BinaryOperator::And => {
                let (mut truthy, _) = self.condition_facts(&binary.left);
                let (right, _) = self.condition_facts(&binary.right);
                truthy.extend(right);
                (truthy, Vec::new())
            }
            ExpressionKind::Binary(binary) if binary.operator == BinaryOperator::Or => {
                let (_, mut falsy) = self.condition_facts(&binary.left);
                let (_, right) = self.condition_facts(&binary.right);
                falsy.extend(right);
                (Vec::new(), falsy)
            }
            ExpressionKind::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::Equal | BinaryOperator::NotEqual
                ) =>
            {
                let (tested, positive) = if is_null_expression(&binary.left) {
                    (&binary.right, binary.operator == BinaryOperator::NotEqual)
                } else if is_null_expression(&binary.right) {
                    (&binary.left, binary.operator == BinaryOperator::NotEqual)
                } else {
                    return (Vec::new(), Vec::new());
                };
                let Some(fact) = self.narrowable_fact(tested) else {
                    return (Vec::new(), Vec::new());
                };
                if positive {
                    (vec![fact], Vec::new())
                } else {
                    (Vec::new(), vec![fact])
                }
            }
            ExpressionKind::Call(call) => {
                if self.option_operation(call) == Some("is_some") && call.arguments.len() == 1 {
                    return match self.narrowable_fact(&call.arguments[0]) {
                        Some(fact) => (vec![fact], Vec::new()),
                        None => (Vec::new(), Vec::new()),
                    };
                }
                (Vec::new(), Vec::new())
            }
            _ => (Vec::new(), Vec::new()),
        }
    }

    fn narrowable_fact(&self, expression: &Expression) -> Option<NarrowFact> {
        let mut expression = expression;
        while let ExpressionKind::Grouping(inner) = &expression.kind {
            expression = inner;
        }
        let ExpressionKind::Identifier(identifier) = &expression.kind else {
            return None;
        };
        let binding = self.lookup(&identifier.name)?;
        if !binding.is_narrowable() {
            return None;
        }
        let ResolvedType::Optional(payload) = &binding.ty else {
            return None;
        };
        Some(NarrowFact {
            source: binding.id,
            name: identifier.name.clone(),
            payload: (**payload).clone(),
            span: expression.span,
        })
    }

    /// Fatos de fluxo válidos na continuação de um `if`, considerando braços
    /// que retornam e cadeias `else if`. Um fato só permanece quando vale em
    /// todos os caminhos que alcançam a continuação.
    /// PT-BR: fatos válidos após o `if` para todos os caminhos de continuação.
    fn if_fallthrough_facts(&self, statement: &lift_ast::IfStatement) -> Vec<NarrowFact> {
        let (truthy, falsy) = self.condition_facts(&statement.condition);
        let then_exits = block_guarantees_exit(&statement.then_branch);
        let else_exits = statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_guarantees_exit(block),
                ElseBranchKind::ElseIf(nested) => if_guarantees_exit(nested),
            });
        if then_exits && else_exits {
            // Continuação inalcançável: não há caminho para carregar fatos.
            return Vec::new();
        }
        let mut else_facts = falsy;
        match statement.else_branch.as_ref() {
            None => {}
            Some(branch) => match &branch.kind {
                ElseBranchKind::Block(block) => {
                    else_facts.extend(self.block_fallthrough_facts(block));
                }
                ElseBranchKind::ElseIf(nested) => {
                    else_facts.extend(self.if_fallthrough_facts(nested));
                }
            },
        }
        if then_exits {
            return dedupe_facts(else_facts);
        }
        let mut then_facts = truthy;
        then_facts.extend(self.block_fallthrough_facts(&statement.then_branch));
        if else_exits {
            return dedupe_facts(then_facts);
        }
        intersect_facts(&then_facts, &else_facts)
    }

    /// Fatos que valem ao final de um bloco por causa de `if` com retorno.
    /// PT-BR: fatos acumulados por guardas de retorno dentro do bloco.
    fn block_fallthrough_facts(&self, block: &Block) -> Vec<NarrowFact> {
        let mut facts = Vec::new();
        for statement in &block.statements {
            if let Statement::If(inner) = statement {
                for fact in self.if_fallthrough_facts(inner) {
                    if !facts
                        .iter()
                        .any(|existing: &NarrowFact| existing.source == fact.source)
                    {
                        facts.push(fact);
                    }
                }
            }
        }
        facts
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
                if !initializer.ty.is_value_type()
                    && !matches!(initializer.ty, ResolvedType::Closure(..))
                {
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
                        narrowed: false,
                        span: variable.name.span,
                    },
                )?;
                let share_cell = variable.binding == lift_ast::BindingKind::Let && ty.is_copy();
                Ok(HirStatement::Variable(HirVariableDeclaration {
                    id,
                    binding: variable.binding,
                    name: variable.name.name.clone(),
                    ty,
                    initializer,
                    requires_mut_binding: false,
                    captured_mutably: share_cell,
                    span: variable.span,
                }))
            }
            Statement::Assignment(assignment) => self.check_assignment(assignment),
            Statement::If(statement) => Ok(HirStatement::If(self.check_if(statement)?)),
            Statement::Match(statement) => self.check_match(statement),
            Statement::While(statement) => {
                let (truthy, _) = self.condition_facts(&statement.condition);
                let condition = self.check_expr(&statement.condition, &ResolvedType::Boolean)?;
                let label = self.fresh_loop_id();
                self.loop_stack.push(label);
                let body = self.check_block_narrowed(&statement.body, true, &truthy);
                self.loop_stack.pop();
                Ok(HirStatement::While(HirWhile {
                    label,
                    condition,
                    body: body?,
                    span: statement.span,
                }))
            }
            Statement::ForOf(statement) => {
                let iterable = self.synthesize_expr(&statement.iterable)?;
                let ResolvedType::Array(element) = iterable.ty.clone() else {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!("`for...of` exige um array, recebido `{}`", iterable.ty),
                        statement.iterable.span,
                    ));
                };
                if element.contains_resource() {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "`for...of` não itera arrays de recursos opacos",
                        statement.iterable.span,
                    ));
                }
                let iterable = self.materialize_owned(iterable, false);
                let id = self.fresh_binding_id();
                let label = self.fresh_loop_id();
                self.scopes.push(HashMap::new());
                let checked = (|| {
                    self.declare(
                        &statement.name.name,
                        BindingInfo {
                            id,
                            ty: (*element).clone(),
                            binding: Some(statement.binding),
                            parameter_mode: None,
                            parameter_mutated: false,
                            narrowed: false,
                            span: statement.name.span,
                        },
                    )?;
                    self.loop_stack.push(label);
                    let body = self.check_block(&statement.body, true);
                    self.loop_stack.pop();
                    body
                })();
                self.scopes.pop();
                Ok(HirStatement::ForOf(HirForOf {
                    label,
                    id,
                    name: statement.name.name.clone(),
                    binding: statement.binding,
                    element_ty: *element,
                    iterable,
                    body: checked?,
                    span: statement.span,
                }))
            }
            Statement::Break(statement) => {
                let Some(target) = self.loop_stack.last().copied() else {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidLoopControl,
                        "`break` só pode aparecer dentro de `while` ou `for`",
                        statement.span,
                    ));
                };
                Ok(HirStatement::Break(HirBreak {
                    target,
                    span: statement.span,
                }))
            }
            Statement::Continue(statement) => {
                let Some(target) = self.loop_stack.last().copied() else {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidLoopControl,
                        "`continue` só pode aparecer dentro de `while` ou `for`",
                        statement.span,
                    ));
                };
                Ok(HirStatement::Continue(HirContinue {
                    target,
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
            Statement::Throw(statement) => {
                let candidates = self.propagation_candidates();
                if candidates.is_empty() {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidPropagation,
                        "`throw` exige um `try` com catch compatível ou uma função que retorne `Result<_, E>`",
                        statement.span,
                    ));
                }
                let mut checked = None;
                for (target, error) in candidates {
                    if let Ok(value) = self.check_expr(&statement.value, &error) {
                        checked = Some((self.materialize_owned(value, false), target));
                        break;
                    }
                }
                let Some((value, target)) = checked else {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidPropagation,
                        "o valor de `throw` não corresponde ao tipo de erro do `catch` nem ao erro do retorno",
                        statement.value.span,
                    ));
                };
                Ok(HirStatement::Throw(HirThrow {
                    value,
                    target,
                    span: statement.span,
                }))
            }
            Statement::TryCatch(statement) => {
                let catch_ty =
                    self.resolve_local_type(&statement.catch_type, TypePosition::Variable)?;
                if !catch_ty.is_value_type() {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "`catch` exige um tipo de erro de valor, não `void`",
                        statement.catch_type.span,
                    ));
                }
                let label = self.fresh_try_id();
                self.scopes.push(HashMap::new());
                self.try_stack.push(TryContext {
                    id: label,
                    error: catch_ty.clone(),
                });
                let body = self.check_block(&statement.body, true);
                self.try_stack.pop();
                self.scopes.pop();
                let body = body?;
                let catch_id = self.fresh_binding_id();
                self.scopes.push(HashMap::new());
                let checked = (|| {
                    self.declare(
                        &statement.catch_name.name,
                        BindingInfo {
                            id: catch_id,
                            ty: catch_ty.clone(),
                            binding: None,
                            parameter_mode: Some(ParameterPassMode::ByValue),
                            parameter_mutated: false,
                            narrowed: false,
                            span: statement.catch_name.span,
                        },
                    )?;
                    self.check_block(&statement.catch_body, false)
                })();
                self.scopes.pop();
                let catch_body = checked?;
                let depth = self.try_stack.len();
                let returns = block_guarantees_return(&statement.body, depth + 1)
                    || (block_guarantees_exit(&statement.body)
                        && block_guarantees_return(&statement.catch_body, depth));
                Ok(HirStatement::TryCatch(HirTryCatch {
                    label,
                    body,
                    catch_id,
                    catch_name: statement.catch_name.name.clone(),
                    catch_ty,
                    catch_body,
                    catch_mut: self.mutable_bindings.contains(&catch_id),
                    returns,
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
        let (truthy, falsy) = self.condition_facts(&statement.condition);
        let truthy = truthy
            .into_iter()
            .filter(|fact| block_mentions_name(&statement.then_branch, &fact.name))
            .collect::<Vec<_>>();
        let falsy = falsy
            .into_iter()
            .filter(|fact| match statement.else_branch.as_ref() {
                Some(branch) => match &branch.kind {
                    ElseBranchKind::Block(block) => block_mentions_name(block, &fact.name),
                    ElseBranchKind::ElseIf(nested) => if_mentions_name(nested, &fact.name),
                },
                None => false,
            })
            .collect::<Vec<_>>();
        let condition = self.check_expr(&statement.condition, &ResolvedType::Boolean)?;
        let then_branch = self.check_block_narrowed(&statement.then_branch, true, &truthy)?;
        let else_branch = statement
            .else_branch
            .as_ref()
            .map(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => self
                    .check_block_narrowed(block, true, &falsy)
                    .map(HirElseBranch::Block),
                ElseBranchKind::ElseIf(if_statement) => {
                    if falsy.is_empty() {
                        self.check_if(if_statement)
                            .map(|statement| HirElseBranch::ElseIf(Box::new(statement)))
                    } else {
                        // Fatos falsos precisam de um bloco para declarar os
                        // bindings derivados antes do `else if`.
                        let span = if_statement.span;
                        self.scopes.push(HashMap::new());
                        let block = (|| {
                            let mut statements = self.push_narrowed(&falsy);
                            statements.push(HirStatement::If(self.check_if(if_statement)?));
                            Ok(HirBlock { statements, span })
                        })();
                        self.scopes.pop();
                        block.map(HirElseBranch::Block)
                    }
                }
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
            if root_info.as_ref().is_some_and(|binding| binding.narrowed) {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedOwnershipPattern,
                    "valor estreitado é uma cópia somente leitura; atribua ao opcional original ou use `option.unwrap`",
                    assignment.target.span,
                ));
            }
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
            .chain(self.enums.keys().cloned())
            .chain(["int", "number", "string", "boolean", "void"].map(str::to_owned))
            .collect::<HashSet<_>>();
        resolve_type(ty, &names, self.type_parameters, position)
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
            if let Some(operation) = self.result_constructor(call) {
                return self.check_result_constructor(expression.span, operation, call, expected);
            }
            if let ExpressionKind::FieldAccess(access) = &call.callee.kind {
                if let ExpressionKind::Identifier(base) = &access.object.kind {
                    if base.name == "map" && access.field.name == "new" {
                        if let ResolvedType::Struct(instance) = expected {
                            if let Some((key, _)) = map_types(instance) {
                                if self
                                    .structures
                                    .get(instance)
                                    .is_some_and(|structure| structure.external)
                                {
                                    if key == ResolvedType::Number {
                                        return Err(SemaError::new(
                                            SemaErrorKind::UnsupportedType,
                                            "Map não aceita `number` como chave (sem Ord)",
                                            expression.span,
                                        ));
                                    }
                                    if !call.arguments.is_empty() {
                                        return Err(SemaError::new(
                                            SemaErrorKind::ArgumentCountMismatch,
                                            "`map.new` não recebe argumentos",
                                            expression.span,
                                        ));
                                    }
                                    return Ok(self.expr(
                                        HirExprKind::MapNew {
                                            map_type: expected.clone(),
                                        },
                                        expected.clone(),
                                        expression.span,
                                    ));
                                }
                            }
                        }
                        return Err(SemaError::new(
                            SemaErrorKind::UnsupportedType,
                            "`map.new` exige contexto `Map<K, V>`",
                            expression.span,
                        ));
                    }
                    if let ResolvedType::Struct(instance) = expected {
                        if instance.starts_with(&format!("{}<", base.name))
                            && self.enums.contains_key(instance)
                        {
                            return self.check_enum_constructor(
                                expression.span,
                                instance,
                                &access.field.name,
                                call,
                            );
                        }
                    }
                }
            }
        }
        if let ExpressionKind::FieldAccess(access) = &expression.kind {
            if let ExpressionKind::Identifier(base) = &access.object.kind {
                if let ResolvedType::Struct(instance) = expected {
                    if instance.starts_with(&format!("{}<", base.name))
                        && self.enums.contains_key(instance)
                    {
                        let variant = self
                            .enums
                            .get(instance)
                            .and_then(|info| {
                                info.variants
                                    .iter()
                                    .find(|variant| variant.name == access.field.name)
                            })
                            .cloned()
                            .ok_or_else(|| {
                                SemaError::new(
                                    SemaErrorKind::UnknownField,
                                    format!(
                                        "enum `{instance}` não tem variante `{}`",
                                        access.field.name
                                    ),
                                    access.field.span,
                                )
                            })?;
                        if !variant.fields.is_empty() || !variant.named_fields.is_empty() {
                            return Err(SemaError::new(
                                SemaErrorKind::ArgumentCountMismatch,
                                format!(
                                    "variante `{instance}.{}` exige payload; chame-a",
                                    access.field.name
                                ),
                                expression.span,
                            ));
                        }
                        return Ok(self.expr(
                            HirExprKind::EnumConstruct {
                                enum_name: instance.clone(),
                                variant: access.field.name.clone(),
                                fields: Vec::new(),
                                named_fields: Vec::new(),
                            },
                            ResolvedType::Struct(instance.clone()),
                            expression.span,
                        ));
                    }
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
            ExpressionKind::TypeApplication { .. } => Err(SemaError::new(
                SemaErrorKind::NotCallable,
                "argumentos de tipo explícitos só são suportados em chamada",
                expression.span,
            )),
            ExpressionKind::TryPropagate(inner) => {
                let value = self.synthesize_expr(inner)?;
                let ResolvedType::Result(ok, error) = value.ty.clone() else {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "`?` exige um valor `Result<T, E>`",
                        expression.span,
                    ));
                };
                let Some(target) = self.propagation_target(&error) else {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidPropagation,
                        "`?` exige um `try` com catch do mesmo erro ou uma função retornando `Result<_, E>` com esse erro",
                        expression.span,
                    ));
                };
                let value = self.materialize_owned(value, false);
                Ok(self.expr(
                    HirExprKind::TryPropagate {
                        expression: Box::new(value),
                        target,
                    },
                    *ok,
                    expression.span,
                ))
            }
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
                if let Some(binding) = self.lookup(&identifier.name).cloned() {
                    return Ok(self.expr(
                        HirExprKind::Local {
                            id: binding.id,
                            name: identifier.name.clone(),
                        },
                        binding.ty,
                        expression.span,
                    ));
                }
                if let Some(function) = self.functions.get(&identifier.name).cloned() {
                    return self.function_value(identifier.name.clone(), &function, expression.span);
                }
                Err(SemaError::new(
                    SemaErrorKind::UnknownVariable,
                    format!("variável `{}` não foi declarada", identifier.name),
                    identifier.span,
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
                if let Some(enum_name) = direct_callee_name(&access.object) {
                    if self.lookup(&enum_name).is_none() {
                        if let Some(info) = self.enums.get(&enum_name).cloned() {
                            let variant = info
                                .variants
                                .iter()
                                .find(|variant| variant.name == access.field.name)
                                .ok_or_else(|| {
                                    SemaError::new(
                                        SemaErrorKind::UnknownField,
                                        format!(
                                            "enum `{enum_name}` não tem variante `{}`",
                                            access.field.name
                                        ),
                                        access.field.span,
                                    )
                                })?;
                            if !variant.fields.is_empty() || !variant.named_fields.is_empty() {
                                let payloads = variant.fields.len() + variant.named_fields.len();
                                return Err(SemaError::new(
                                    SemaErrorKind::ArgumentCountMismatch,
                                    format!(
                                        "variante `{enum_name}.{}` exige {payloads} payload(s); use `{enum_name}.{}(...)` ou `{{ campo: valor }}`",
                                        access.field.name,
                                        access.field.name
                                    ),
                                    expression.span,
                                ));
                            }
                            let ty = ResolvedType::Struct(enum_name.clone());
                            return Ok(self.expr(
                                HirExprKind::EnumConstruct {
                                    enum_name,
                                    variant: access.field.name.clone(),
                                    fields: Vec::new(),
                                    named_fields: Vec::new(),
                                },
                                ty,
                                expression.span,
                            ));
                        }
                    }
                }
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
            if !provided.contains_key(&declared.name) && !declared.optional {
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
        // de campos não sejam reordenados durante a construção. Campos
        // opcionais omitidos entram ao final como `None` explícito, porque o
        // Rust gerado exige todos os campos na inicialização.
        let mut fields = Vec::with_capacity(structure.fields.len());
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
        for declared in &structure.fields {
            if provided.contains_key(&declared.name) {
                continue;
            }
            fields.push(HirStructInitField {
                name: declared.name.clone(),
                value: self.expr(HirExprKind::OptionNone, declared.ty.clone(), declared.span),
                span: declared.span,
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
                "elemento de array exige tipo de valor (não `void` nem função)",
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

    /// `match` exaustivo de `Option<T>`/`Result<T, E>`.
    fn check_match(
        &mut self,
        statement: &lift_ast::MatchStatement,
    ) -> Result<HirStatement, SemaError> {
        let value = self.synthesize_expr(&statement.value)?;
        if value.ty.contains_resource() {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "`match` não aceita recurso opaco; use `?` ou unwrap",
                statement.value.span,
            ));
        }
        let enum_case = match &value.ty {
            ResolvedType::Struct(name) => self
                .enums
                .get(name)
                .cloned()
                .map(|info| (name.clone(), info.variants)),
            _ => None,
        };
        if let Some((name, variants)) = enum_case {
            return self.check_enum_match(statement, value, name, variants);
        }
        let (payload, error) = match value.ty.clone() {
            ResolvedType::Optional(payload) => ((*payload).clone(), None),
            ResolvedType::Result(ok, error) => ((*ok).clone(), Some((*error).clone())),
            other => {
                return Err(SemaError::new(
                    SemaErrorKind::TypeMismatch,
                    format!(
                        "`match` exige `Option<T>`, `Result<T, E>` ou enum, recebido `{other}`"
                    ),
                    statement.value.span,
                ));
            }
        };
        let value = self.materialize_owned(value, false);
        let mut seen = HashSet::new();
        let mut arms = Vec::new();
        for arm in &statement.arms {
            let (variant, binding) = match (&arm.pattern.kind, &error) {
                (lift_ast::MatchPatternKind::Some(name), None) => {
                    ("some", Some((name, payload.clone())))
                }
                (lift_ast::MatchPatternKind::None, None) => ("none", None),
                (lift_ast::MatchPatternKind::Ok(Some(name)), Some(_)) => {
                    if payload == ResolvedType::Void {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidMatch,
                            "`ok(nome)` não vale para sucesso `void`; use `ok()`",
                            arm.pattern.span,
                        ));
                    }
                    ("ok", Some((name, payload.clone())))
                }
                (lift_ast::MatchPatternKind::Ok(None), Some(_)) => {
                    if payload != ResolvedType::Void {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidMatch,
                            format!("`ok()` exige sucesso `void`, mas o payload é `{payload}`"),
                            arm.pattern.span,
                        ));
                    }
                    ("ok", None)
                }
                (lift_ast::MatchPatternKind::Err(name), Some(_)) => {
                    ("err", Some((name, error.clone().unwrap())))
                }
                _ => {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidMatch,
                        format!(
                            "padrão incompatível com o valor do `match` (`{}`)",
                            value.ty
                        ),
                        arm.pattern.span,
                    ));
                }
            };
            if !seen.insert(variant) {
                return Err(SemaError::new(
                    SemaErrorKind::InvalidMatch,
                    format!("braço `{variant}` duplicado no `match`"),
                    arm.pattern.span,
                ));
            }
            self.scopes.push(HashMap::new());
            let checked = (|| {
                let binding = match binding {
                    Some((name, ty)) => {
                        let id = self.fresh_binding_id();
                        self.declare(
                            &name.name,
                            BindingInfo {
                                id,
                                ty: ty.clone(),
                                binding: Some(BindingKind::Const),
                                parameter_mode: None,
                                parameter_mutated: false,
                                narrowed: false,
                                span: name.span,
                            },
                        )?;
                        Some((id, name.name.clone(), ty))
                    }
                    None => None,
                };
                self.check_block(&arm.body, true)
                    .map(|body| (binding, body))
            })();
            self.scopes.pop();
            let (binding, body) = checked?;
            let pattern = match (&arm.pattern.kind, binding) {
                (lift_ast::MatchPatternKind::Some(_), Some((id, name, ty))) => {
                    HirMatchPattern::Some(HirMatchBinding {
                        id,
                        name,
                        ty,
                        requires_mut: self.mutable_bindings.contains(&id),
                        span: arm.pattern.span,
                    })
                }
                (lift_ast::MatchPatternKind::None, _) => HirMatchPattern::None,
                (lift_ast::MatchPatternKind::Ok(Some(_)), Some((id, name, ty))) => {
                    HirMatchPattern::Ok(Some(HirMatchBinding {
                        id,
                        name,
                        ty,
                        requires_mut: self.mutable_bindings.contains(&id),
                        span: arm.pattern.span,
                    }))
                }
                (lift_ast::MatchPatternKind::Ok(None), _) => HirMatchPattern::Ok(None),
                (lift_ast::MatchPatternKind::Err(_), Some((id, name, ty))) => {
                    HirMatchPattern::Err(HirMatchBinding {
                        id,
                        name,
                        ty,
                        requires_mut: self.mutable_bindings.contains(&id),
                        span: arm.pattern.span,
                    })
                }
                _ => unreachable!("binding validation matches the pattern"),
            };
            arms.push(HirMatchArm {
                pattern,
                body,
                span: arm.span,
            });
        }
        let required: &[&str] = if error.is_some() {
            &["ok", "err"]
        } else {
            &["some", "none"]
        };
        for variant in required {
            if !seen.contains(*variant) {
                return Err(SemaError::new(
                    SemaErrorKind::InvalidMatch,
                    format!("`match` não é exaustivo: falta o braço `{variant}`"),
                    statement.span,
                ));
            }
        }
        Ok(HirStatement::Match(HirMatch {
            value,
            arms,
            span: statement.span,
        }))
    }

    /// `match` exaustivo de um enum nominal com payloads posicionais.
    fn check_enum_match(
        &mut self,
        statement: &lift_ast::MatchStatement,
        value: HirExpr,
        enum_name: String,
        variants: Vec<EnumVariantInfo>,
    ) -> Result<HirStatement, SemaError> {
        let value = self.materialize_owned(value, false);
        let mut seen = HashSet::new();
        let mut arms = Vec::new();
        for arm in &statement.arms {
            let (variant_name, binding_names) = match &arm.pattern.kind {
                lift_ast::MatchPatternKind::EnumVariant {
                    enum_name: pattern_enum,
                    variant,
                    bindings,
                } => {
                    if pattern_enum.name != enum_name
                        && !enum_name.starts_with(&format!("{}<", pattern_enum.name))
                    {
                        return Err(SemaError::new(
                            SemaErrorKind::InvalidMatch,
                            format!(
                                "braço usa enum `{}`, mas o valor é `{enum_name}`",
                                pattern_enum.name
                            ),
                            arm.pattern.span,
                        ));
                    }
                    (variant.name.clone(), bindings)
                }
                _ => {
                    return Err(SemaError::new(
                        SemaErrorKind::InvalidMatch,
                        format!("padrão incompatível com o enum `{enum_name}`"),
                        arm.pattern.span,
                    ));
                }
            };
            let variant = variants
                .iter()
                .find(|variant| variant.name == variant_name)
                .ok_or_else(|| {
                    SemaError::new(
                        SemaErrorKind::InvalidMatch,
                        format!("enum `{enum_name}` não tem variante `{variant_name}`"),
                        arm.pattern.span,
                    )
                })?;
            let field_types: Vec<ResolvedType> = if variant.named_fields.is_empty() {
                variant.fields.clone()
            } else {
                variant
                    .named_fields
                    .iter()
                    .map(|field| field.ty.clone())
                    .collect()
            };
            if binding_names.len() != field_types.len() {
                return Err(SemaError::new(
                    SemaErrorKind::InvalidMatch,
                    format!(
                        "variante `{enum_name}.{variant_name}` espera {} binding(s), recebeu {}",
                        field_types.len(),
                        binding_names.len()
                    ),
                    arm.pattern.span,
                ));
            }
            if !seen.insert(variant_name.clone()) {
                return Err(SemaError::new(
                    SemaErrorKind::InvalidMatch,
                    format!("braço `{enum_name}.{variant_name}` duplicado no `match`"),
                    arm.pattern.span,
                ));
            }
            self.scopes.push(HashMap::new());
            let checked = (|| {
                let mut bindings = Vec::new();
                for (name, ty) in binding_names.iter().zip(field_types.iter()) {
                    let id = self.fresh_binding_id();
                    self.declare(
                        &name.name,
                        BindingInfo {
                            id,
                            ty: ty.clone(),
                            binding: Some(BindingKind::Const),
                            parameter_mode: None,
                            parameter_mutated: false,
                            narrowed: false,
                            span: name.span,
                        },
                    )?;
                    bindings.push((id, name.name.clone(), ty.clone()));
                }
                self.check_block(&arm.body, true)
                    .map(|body| (bindings, body))
            })();
            self.scopes.pop();
            let (bindings, body) = checked?;
            let bindings = bindings
                .into_iter()
                .map(|(id, name, ty)| HirMatchBinding {
                    id,
                    name,
                    ty,
                    requires_mut: self.mutable_bindings.contains(&id),
                    span: arm.pattern.span,
                })
                .collect();
            arms.push(HirMatchArm {
                pattern: HirMatchPattern::EnumVariant {
                    enum_name: enum_name.clone(),
                    variant: variant_name,
                    bindings,
                    named: !variant.named_fields.is_empty(),
                },
                body,
                span: arm.span,
            });
        }
        for variant in &variants {
            if !seen.contains(&variant.name) {
                return Err(SemaError::new(
                    SemaErrorKind::InvalidMatch,
                    format!(
                        "`match` não é exaustivo: falta o braço `{enum_name}.{}`",
                        variant.name
                    ),
                    statement.span,
                ));
            }
        }
        Ok(HirStatement::Match(HirMatch {
            value,
            arms,
            span: statement.span,
        }))
    }

    /// `Enum.Variant(payload...)`: valida aridade/tipos e materializa os campos.
    fn check_enum_constructor(
        &mut self,
        span: Span,
        enum_name: &str,
        variant_name: &str,
        call: &lift_ast::CallExpression,
    ) -> Result<HirExpr, SemaError> {
        let info = self.enums.get(enum_name).cloned().ok_or_else(|| {
            SemaError::new(
                SemaErrorKind::UnknownType,
                format!("enum `{enum_name}` não foi declarado"),
                call.callee.span,
            )
        })?;
        let variant = info
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
            .ok_or_else(|| {
                SemaError::new(
                    SemaErrorKind::UnknownField,
                    format!("enum `{enum_name}` não tem variante `{variant_name}`"),
                    call.callee.span,
                )
            })?;
        if !variant.named_fields.is_empty() {
            if call.arguments.len() != 1 {
                return Err(SemaError::new(
                    SemaErrorKind::ArgumentCountMismatch,
                    format!(
                        "variante `{enum_name}.{variant_name}` espera um object literal com {} campo(s)",
                        variant.named_fields.len()
                    ),
                    span,
                ));
            }
            let argument = &call.arguments[0];
            let lift_ast::ExpressionKind::ObjectLiteral(object) = &argument.kind else {
                return Err(SemaError::new(
                    SemaErrorKind::TypeMismatch,
                    format!("variante `{enum_name}.{variant_name}` espera `{{ campo: valor }}`"),
                    argument.span,
                ));
            };
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
                if !variant
                    .named_fields
                    .iter()
                    .any(|declared| declared.name == field.name.name)
                {
                    return Err(SemaError::new(
                        SemaErrorKind::UnexpectedObjectField,
                        format!(
                            "variante `{enum_name}.{variant_name}` não declara o campo `{}`",
                            field.name.name
                        ),
                        field.name.span,
                    ));
                }
            }
            for declared in &variant.named_fields {
                if !provided.contains_key(&declared.name) {
                    return Err(SemaError::new(
                        SemaErrorKind::MissingObjectField,
                        format!(
                            "payload de `{enum_name}.{variant_name}` não informa `{}`",
                            declared.name
                        ),
                        span,
                    ));
                }
            }
            let mut named_fields = Vec::with_capacity(object.fields.len());
            for field in &object.fields {
                let declared = variant
                    .named_fields
                    .iter()
                    .find(|declared| declared.name == field.name.name)
                    .unwrap();
                let value = self.check_expr(&field.value, &declared.ty)?;
                named_fields.push(HirStructInitField {
                    name: declared.name.clone(),
                    value: self.materialize_owned(value, false),
                    span: field.span,
                });
            }
            return Ok(self.expr(
                HirExprKind::EnumConstruct {
                    enum_name: enum_name.to_owned(),
                    variant: variant_name.to_owned(),
                    fields: Vec::new(),
                    named_fields,
                },
                ResolvedType::Struct(enum_name.to_owned()),
                span,
            ));
        }
        if call.arguments.len() != variant.fields.len() {
            return Err(SemaError::new(
                SemaErrorKind::ArgumentCountMismatch,
                format!(
                    "variante `{enum_name}.{variant_name}` espera {} payload(s), recebeu {}",
                    variant.fields.len(),
                    call.arguments.len()
                ),
                span,
            ));
        }
        let mut fields = Vec::with_capacity(call.arguments.len());
        for (argument, ty) in call.arguments.iter().zip(&variant.fields) {
            let value = self.check_expr(argument, ty)?;
            fields.push(self.materialize_owned(value, false));
        }
        Ok(self.expr(
            HirExprKind::EnumConstruct {
                enum_name: enum_name.to_owned(),
                variant: variant_name.to_owned(),
                fields,
                named_fields: Vec::new(),
            },
            ResolvedType::Struct(enum_name.to_owned()),
            span,
        ))
    }

    fn check_map_method(
        &mut self,
        span: Span,
        map_name: &str,
        method_name: &str,
        map: HirExpr,
        call: &lift_ast::CallExpression,
    ) -> Result<HirExpr, SemaError> {
        let Some((key_type, value_type)) = map_types(map_name) else {
            return Err(SemaError::new(
                SemaErrorKind::UnknownType,
                format!("`{map_name}` não é um Map nativo"),
                span,
            ));
        };
        if key_type == ResolvedType::Number {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "Map não aceita `number` como chave (sem Ord)",
                span,
            ));
        }
        let method = match method_name {
            "insert" => MapMethodKind::Insert,
            "get" => MapMethodKind::Get,
            "contains" => MapMethodKind::Contains,
            "len" => MapMethodKind::Len,
            _ => {
                return Err(SemaError::new(
                    SemaErrorKind::UnknownField,
                    format!("Map não tem método `{method_name}`"),
                    span,
                ));
            }
        };
        let expected = match method {
            MapMethodKind::Insert => 2,
            MapMethodKind::Get | MapMethodKind::Contains => 1,
            MapMethodKind::Len => 0,
        };
        if call.arguments.len() != expected {
            return Err(SemaError::new(
                SemaErrorKind::ArgumentCountMismatch,
                format!("`map.{method_name}` espera {expected} argumento(s)"),
                span,
            ));
        }
        let key = if matches!(
            method,
            MapMethodKind::Insert | MapMethodKind::Get | MapMethodKind::Contains
        ) {
            let value = self.check_expr(&call.arguments[0], &key_type)?;
            Some(Box::new(self.materialize_owned(value, false)))
        } else {
            None
        };
        let value = if method == MapMethodKind::Insert {
            let value = self.check_expr(&call.arguments[1], &value_type)?;
            Some(Box::new(self.materialize_owned(value, false)))
        } else {
            None
        };
        let ty = match method {
            MapMethodKind::Insert => ResolvedType::Struct(map_name.to_owned()),
            MapMethodKind::Get => ResolvedType::Optional(Box::new(value_type)),
            MapMethodKind::Contains => ResolvedType::Boolean,
            MapMethodKind::Len => ResolvedType::Int,
        };
        Ok(self.expr(
            HirExprKind::MapMethod {
                method,
                map: Box::new(map),
                key,
                value,
            },
            ty,
            span,
        ))
    }

    /// Métodos higher-order de array (`map`, `filter`, `forEach`, `find`,
    /// `some`, `every`, `reduce`), monomorfizados pelo tipo do elemento.
    /// PT-BR: métodos de array com callback, sem generics Lift.
    fn check_array_method(
        &mut self,
        span: Span,
        method_name: &str,
        array: HirExpr,
        element: ResolvedType,
        call: &lift_ast::CallExpression,
    ) -> Result<HirExpr, SemaError> {
        let Some(method) = ArrayMethodKind::from_name(method_name) else {
            return Err(SemaError::new(
                SemaErrorKind::UnknownField,
                format!("array não tem método `{method_name}`"),
                call.callee.span,
            ));
        };
        let expected_arguments = if method == ArrayMethodKind::Reduce {
            2
        } else {
            1
        };
        if call.arguments.len() != expected_arguments {
            return Err(SemaError::new(
                SemaErrorKind::ArgumentCountMismatch,
                format!(
                    "`{method_name}` espera {expected_arguments} argumento(s), recebeu {}",
                    call.arguments.len()
                ),
                span,
            ));
        }
        let initial = if method == ArrayMethodKind::Reduce {
            Some(self.synthesize_expr(&call.arguments[0])?)
        } else {
            None
        };
        let callback_index = if method == ArrayMethodKind::Reduce {
            1
        } else {
            0
        };
        let callback = self.synthesize_expr(&call.arguments[callback_index])?;
        let ResolvedType::Closure(parameters, result) = callback.ty.clone() else {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                format!("`{method_name}` espera uma função como argumento"),
                callback.span,
            ));
        };
        if method != ArrayMethodKind::Reduce && (parameters.len() != 1 || parameters[0] != element)
        {
            return Err(SemaError::new(
                SemaErrorKind::TypeMismatch,
                format!("callback de `{method_name}` deve receber `{element}`"),
                callback.span,
            ));
        }
        let return_type = match method {
            ArrayMethodKind::Map => ResolvedType::Array(Box::new((*result).clone())),
            ArrayMethodKind::Filter => {
                self.require_callback_result(
                    method_name,
                    &result,
                    &ResolvedType::Boolean,
                    callback.span,
                )?;
                ResolvedType::Array(Box::new(element.clone()))
            }
            ArrayMethodKind::Find => {
                self.require_callback_result(
                    method_name,
                    &result,
                    &ResolvedType::Boolean,
                    callback.span,
                )?;
                if matches!(element, ResolvedType::Optional(_)) {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "`find` em array de opcionais exigiria Option aninhado; use `filter`",
                        call.callee.span,
                    ));
                }
                ResolvedType::Optional(Box::new(element.clone()))
            }
            ArrayMethodKind::Some | ArrayMethodKind::Every => {
                self.require_callback_result(
                    method_name,
                    &result,
                    &ResolvedType::Boolean,
                    callback.span,
                )?;
                ResolvedType::Boolean
            }
            ArrayMethodKind::ForEach => {
                self.require_callback_result(
                    method_name,
                    &result,
                    &ResolvedType::Void,
                    callback.span,
                )?;
                ResolvedType::Void
            }
            ArrayMethodKind::Reduce => {
                let initial = initial.as_ref().unwrap();
                if parameters.len() != 2
                    || parameters[0] != initial.ty
                    || parameters[1] != element
                    || *result != initial.ty
                {
                    return Err(SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!(
                            "callback de `reduce` deve ser `({}, {element}) => {}`",
                            initial.ty, initial.ty
                        ),
                        callback.span,
                    ));
                }
                initial.ty.clone()
            }
        };
        let array = self.materialize_owned(array, false);
        let callback = self.materialize_owned(callback, false);
        let initial = initial.map(|value| self.materialize_owned(value, false));
        Ok(self.expr(
            HirExprKind::ArrayMethod {
                method,
                array: Box::new(array),
                callback: Box::new(callback),
                initial: initial.map(Box::new),
            },
            return_type,
            span,
        ))
    }

    fn require_callback_result(
        &self,
        method: &str,
        actual: &ResolvedType,
        expected: &ResolvedType,
        span: Span,
    ) -> Result<(), SemaError> {
        if actual != expected {
            return Err(SemaError::new(
                SemaErrorKind::TypeMismatch,
                format!("callback de `{method}` deve retornar `{expected}`, recebido `{actual}`"),
                span,
            ));
        }
        Ok(())
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
                "option.some exige payload de valor (não `void`, função ou opcional)",
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

    /// Converte uma função nomeada em valor `Box<dyn Fn>` com parâmetros owned.
    /// PT-BR: função nomeada como valor de callback.
    fn function_value(
        &self,
        name: String,
        function: &FunctionInfo,
        span: Span,
    ) -> Result<HirExpr, SemaError> {
        if !function.type_parameters.is_empty() {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "função genérica não pode ser usada como valor; chame-a diretamente",
                span,
            ));
        }
        if function.return_type.contains_resource()
            || function
                .parameters
                .iter()
                .any(|parameter| parameter.ty.contains_resource())
        {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedOwnershipPattern,
                "função com recursos opacos não pode ser usada como valor",
                span,
            ));
        }
        let parameters = function
            .parameters
            .iter()
            .map(|parameter| (parameter.ty.clone(), parameter.pass_mode))
            .collect::<Vec<_>>();
        let signature = ResolvedType::Closure(
            parameters
                .iter()
                .map(|(ty, _)| ty.clone())
                .collect::<Vec<_>>(),
            Box::new(function.return_type.clone()),
        );
        Ok(self.expr(
            HirExprKind::FunctionValue {
                name,
                parameters,
                return_type: function.return_type.clone(),
            },
            signature,
            span,
        ))
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
            type_parameters: Vec::new(),
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
            self.type_parameters,
            self.structures,
            self.enums,
            self.functions,
            self.next_binding_id,
            self.next_try_id,
            self.next_loop_id,
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
                    binding: Some(BindingKind::Let),
                    parameter_mode: None,
                    parameter_mutated: false,
                    narrowed: false,
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
                    narrowed: false,
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
            type_parameters: Vec::new(),
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
        if result != ResolvedType::Void && !block_guarantees_return(&closure.body, 0) {
            return Err(SemaError::new(
                SemaErrorKind::MissingReturnPath,
                "closure deve retornar em todos os caminhos",
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
        let mutable_captures = captures
            .iter()
            .filter(|(id, _)| checker.mutable_bindings.contains(id))
            .map(|(id, _)| *id)
            .collect();
        Ok(self.expr(
            HirExprKind::Closure {
                captures,
                mutable_captures,
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
        let (callee_expression, explicit_type_arguments) = match &call.callee.kind {
            ExpressionKind::TypeApplication {
                callee, arguments, ..
            } => (callee.as_ref(), Some(arguments.as_slice())),
            _ => (call.callee.as_ref(), None),
        };
        if self.result_constructor(call).is_some() {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "`result.ok`/`result.err` exigem contexto `Result<T, E>`; anote a variável, use em retorno ou passe para uma função",
                span,
            ));
        }
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
            // Chamada imediata do retorno de outra chamada: `make()(3)`.
            ExpressionKind::Call(_) | ExpressionKind::Grouping(_) => {
                let callee = self.synthesize_expr(&call.callee)?;
                if matches!(callee.ty, ResolvedType::Closure(..)) {
                    Some(callee)
                } else {
                    return Err(SemaError::new(
                        SemaErrorKind::NotCallable,
                        "expressão chamada não é uma função",
                        call.callee.span,
                    ));
                }
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
            if let Some(enum_name) = direct_callee_name(&access.object) {
                if self.lookup(&enum_name).is_none() && self.enums.contains_key(&enum_name) {
                    return self.check_enum_constructor(span, &enum_name, &access.field.name, call);
                }
            }
            let receiver = self.synthesize_expr(&access.object)?;
            if let ResolvedType::Array(element) = receiver.ty.clone() {
                return self.check_array_method(span, &access.field.name, receiver, *element, call);
            }
            if let ResolvedType::Struct(target) = receiver.ty.clone() {
                if self
                    .structures
                    .get(&target)
                    .is_some_and(|structure| structure.external)
                    && map_types(&target).is_some()
                {
                    return self.check_map_method(
                        span,
                        &target,
                        &access.field.name,
                        receiver,
                        call,
                    );
                }
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
        let callee = direct_callee_name(callee_expression).ok_or_else(|| {
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

        let generic = !function.type_parameters.is_empty();
        if explicit_type_arguments.is_some() && !generic {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                format!("`{callee}` não declara parâmetros de tipo"),
                call.callee.span,
            ));
        }
        let mut inferred = HashMap::<String, ResolvedType>::new();
        if let Some(arguments) = explicit_type_arguments {
            if arguments.len() != function.type_parameters.len() {
                return Err(SemaError::new(
                    SemaErrorKind::ArgumentCountMismatch,
                    format!(
                        "`{callee}` espera {} argumento(s) de tipo, recebeu {}",
                        function.type_parameters.len(),
                        arguments.len()
                    ),
                    call.callee.span,
                ));
            }
            for (parameter, argument) in function.type_parameters.iter().zip(arguments) {
                inferred.insert(
                    parameter.clone(),
                    self.resolve_local_type(argument, TypePosition::Field)?,
                );
            }
        }
        let mut arguments = Vec::with_capacity(call.arguments.len());
        for (argument, parameter) in call.arguments.iter().zip(&function.parameters) {
            let expression = if generic {
                let expression = self.synthesize_expr(argument)?;
                if !expression.ty.is_value_type() || expression.ty.needs_db_runtime() {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        format!(
                            "argumento `{}` não pode instanciar um parâmetro de tipo genérico",
                            expression.ty
                        ),
                        argument.span,
                    ));
                }
                unify_type(&parameter.ty, &expression.ty, &mut inferred).map_err(|message| {
                    SemaError::new(
                        SemaErrorKind::TypeMismatch,
                        format!("`{callee}`: {message}"),
                        argument.span,
                    )
                })?;
                expression
            } else {
                self.check_expr(argument, &parameter.ty)?
            };
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
                    if !expression.ty.is_affine()
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
            if pass_mode == ArgPassMode::ByMutRef {
                if let Some(root) = root_binding(&expression) {
                    if self
                        .lookup_by_id(root)
                        .is_some_and(|binding| binding.narrowed)
                    {
                        return Err(SemaError::new(
                            SemaErrorKind::UnsupportedOwnershipPattern,
                            "chamada mutaria apenas a cópia do valor estreitado; use `option.unwrap` em uma variável própria",
                            argument.span,
                        ));
                    }
                }
            }
            arguments.push(HirCallArgument {
                expression,
                pass_mode,
                clone_mode,
                defer_borrow_until_call: false,
                pre_evaluation_temp: None,
                span: argument.span,
            });
        }

        let return_type = if generic {
            for parameter in &function.type_parameters {
                if !inferred.contains_key(parameter) {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        format!(
                            "não foi possível inferir o parâmetro de tipo `{parameter}` de `{callee}`"
                        ),
                        span,
                    ));
                }
            }
            substitute_type(&function.return_type, &inferred)
        } else {
            function.return_type.clone()
        };
        self.resolve_call_aliasing(&mut arguments)?;
        Ok(self.expr(
            HirExprKind::Call {
                function: function.name,
                arguments,
            },
            return_type,
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

    /// Destinos possíveis de `?`/`throw`, do mais interno ao mais externo.
    /// PT-BR: candidatos de propagação na ordem try interno → retorno.
    fn propagation_candidates(&self) -> Vec<(PropagateTarget, ResolvedType)> {
        let mut candidates = self
            .try_stack
            .iter()
            .rev()
            .map(|context| (PropagateTarget::Try(context.id), context.error.clone()))
            .collect::<Vec<_>>();
        if let Some(function) = &self.function {
            if let ResolvedType::Result(_, error) = &function.return_type {
                candidates.push((PropagateTarget::Function, (**error).clone()));
            }
        }
        candidates
    }

    fn propagation_target(&self, error: &ResolvedType) -> Option<PropagateTarget> {
        self.propagation_candidates()
            .into_iter()
            .find(|(_, candidate)| candidate == error)
            .map(|(target, _)| target)
    }

    /// `result.ok`/`result.err` como construtores contextuais de `Result`.
    /// PT-BR: construtores `result.ok`/`result.err`.
    fn result_constructor<'e>(&self, call: &'e lift_ast::CallExpression) -> Option<&'e str> {
        let ExpressionKind::FieldAccess(access) = &call.callee.kind else {
            return None;
        };
        if direct_callee_name(&access.object).as_deref() == Some("result")
            && self.lookup("result").is_none()
            && !self.functions.contains_key("result")
            && !self.structures.contains_key("result")
            && matches!(access.field.name.as_str(), "ok" | "err")
        {
            Some(access.field.name.as_str())
        } else {
            None
        }
    }

    fn check_result_constructor(
        &mut self,
        span: Span,
        operation: &str,
        call: &lift_ast::CallExpression,
        expected: &ResolvedType,
    ) -> Result<HirExpr, SemaError> {
        let ResolvedType::Result(ok, error) = expected else {
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "`result.ok`/`result.err` exigem contexto `Result<T, E>`; anote a variável, use em retorno ou passe para uma função",
                span,
            ));
        };
        match operation {
            "ok" => {
                if **ok == ResolvedType::Void {
                    if !call.arguments.is_empty() {
                        return Err(SemaError::new(
                            SemaErrorKind::ArgumentCountMismatch,
                            "`result.ok` sem argumentos quando o sucesso é `void`",
                            span,
                        ));
                    }
                    return Ok(self.expr(HirExprKind::ResultOk(None), expected.clone(), span));
                }
                if call.arguments.len() != 1 {
                    return Err(SemaError::new(
                        SemaErrorKind::ArgumentCountMismatch,
                        "`result.ok` espera exatamente um argumento",
                        span,
                    ));
                }
                let value = self.check_expr(&call.arguments[0], ok)?;
                let value = self.materialize_owned(value, false);
                Ok(self.expr(
                    HirExprKind::ResultOk(Some(Box::new(value))),
                    expected.clone(),
                    span,
                ))
            }
            "err" => {
                if call.arguments.len() != 1 {
                    return Err(SemaError::new(
                        SemaErrorKind::ArgumentCountMismatch,
                        "`result.err` espera exatamente um argumento",
                        span,
                    ));
                }
                let value = self.check_expr(&call.arguments[0], error)?;
                let value = self.materialize_owned(value, false);
                Ok(self.expr(
                    HirExprKind::ResultErr(Box::new(value)),
                    expected.clone(),
                    span,
                ))
            }
            _ => unreachable!("validated result constructor"),
        }
    }

    fn materialize_owned(&self, expression: HirExpr, returning: bool) -> HirExpr {
        if expression.ty.is_affine() && is_place(&expression) {
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

/// Um payload de variante precisa de `Box` quando seu tipo alcança um enum
/// (direto ou via campos de interface), quebrando ciclos de tamanho.
fn type_needs_box(
    ty: &ResolvedType,
    enum_names: &HashSet<String>,
    structures: &HashMap<String, StructInfo>,
    seen: &mut HashSet<String>,
) -> bool {
    match ty {
        ResolvedType::Struct(name) => {
            if enum_names.contains(name) {
                return true;
            }
            if !seen.insert(name.clone()) {
                return true;
            }
            structures.get(name).is_some_and(|info| {
                info.fields
                    .iter()
                    .any(|field| type_needs_box(&field.ty, enum_names, structures, seen))
            })
        }
        ResolvedType::Optional(inner) => type_needs_box(inner, enum_names, structures, seen),
        ResolvedType::Result(ok, error) => {
            type_needs_box(ok, enum_names, structures, seen)
                || type_needs_box(error, enum_names, structures, seen)
        }
        // Vec já introduz indireção; closures/recursos não são payloads válidos.
        _ => false,
    }
}

fn unify_type(
    pattern: &ResolvedType,
    actual: &ResolvedType,
    bindings: &mut HashMap<String, ResolvedType>,
) -> Result<(), String> {
    match pattern {
        ResolvedType::TypeParam(name) => {
            if let Some(existing) = bindings.get(name) {
                if existing == actual {
                    return Ok(());
                }
                return Err(format!(
                    "parâmetro de tipo `{name}` inferido como `{existing}` e `{actual}`"
                ));
            }
            bindings.insert(name.clone(), actual.clone());
            Ok(())
        }
        ResolvedType::Array(element) | ResolvedType::Optional(element) => {
            let actual_element = match (pattern, actual) {
                (ResolvedType::Array(_), ResolvedType::Array(actual))
                | (ResolvedType::Optional(_), ResolvedType::Optional(actual)) => actual,
                _ => return Err(format!("esperado `{pattern}`, recebeu `{actual}`")),
            };
            unify_type(element, actual_element, bindings)
        }
        ResolvedType::Result(ok, error) => {
            let ResolvedType::Result(actual_ok, actual_error) = actual else {
                return Err(format!("esperado `{pattern}`, recebeu `{actual}`"));
            };
            unify_type(ok, actual_ok, bindings)?;
            unify_type(error, actual_error, bindings)
        }
        ResolvedType::Closure(parameters, result) => {
            let ResolvedType::Closure(actual_parameters, actual_result) = actual else {
                return Err(format!("esperado `{pattern}`, recebeu `{actual}`"));
            };
            if parameters.len() != actual_parameters.len() {
                return Err(format!("esperado `{pattern}`, recebeu `{actual}`"));
            }
            for (pattern, actual) in parameters.iter().zip(actual_parameters) {
                unify_type(pattern, actual, bindings)?;
            }
            unify_type(result, actual_result, bindings)
        }
        _ if pattern == actual => Ok(()),
        _ => Err(format!("esperado `{pattern}`, recebeu `{actual}`")),
    }
}

fn substitute_type(ty: &ResolvedType, bindings: &HashMap<String, ResolvedType>) -> ResolvedType {
    match ty {
        ResolvedType::TypeParam(name) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        ResolvedType::Array(element) => {
            ResolvedType::Array(Box::new(substitute_type(element, bindings)))
        }
        ResolvedType::Optional(element) => {
            ResolvedType::Optional(Box::new(substitute_type(element, bindings)))
        }
        ResolvedType::Result(ok, error) => ResolvedType::Result(
            Box::new(substitute_type(ok, bindings)),
            Box::new(substitute_type(error, bindings)),
        ),
        ResolvedType::Closure(parameters, result) => ResolvedType::Closure(
            parameters
                .iter()
                .map(|parameter| substitute_type(parameter, bindings))
                .collect(),
            Box::new(substitute_type(result, bindings)),
        ),
        _ => ty.clone(),
    }
}

fn type_contains_param(ty: &ResolvedType, parameter: &str) -> bool {
    match ty {
        ResolvedType::TypeParam(name) => name == parameter,
        ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
            type_contains_param(inner, parameter)
        }
        ResolvedType::Result(ok, error) => {
            type_contains_param(ok, parameter) || type_contains_param(error, parameter)
        }
        ResolvedType::Closure(parameters, result) => {
            parameters
                .iter()
                .any(|ty| type_contains_param(ty, parameter))
                || type_contains_param(result, parameter)
        }
        _ => false,
    }
}

fn resolve_type(
    expression: &TypeExpression,
    known_structs: &HashSet<String>,
    type_parameters: &HashSet<String>,
    position: TypePosition,
) -> Result<ResolvedType, SemaError> {
    let resolved = match &expression.kind {
        TypeExpressionKind::Generic { name, arguments } => {
            for argument in arguments {
                let _ = resolve_type(
                    argument,
                    known_structs,
                    type_parameters,
                    TypePosition::Field,
                )?;
            }
            return Err(SemaError::new(
                SemaErrorKind::UnsupportedType,
                format!(
                    "container genérico `{}` ainda não tem instanciação semântica",
                    name.name
                ),
                expression.span,
            ));
        }
        TypeExpressionKind::Named(name) if type_parameters.contains(&name.name) => {
            ResolvedType::TypeParam(name.name.clone())
        }
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
            let element =
                resolve_type(element, known_structs, type_parameters, TypePosition::Field)?;
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
            let ok = resolve_type(ok, known_structs, type_parameters, TypePosition::Return)?;
            let error = resolve_type(error, known_structs, type_parameters, TypePosition::Field)?;
            if ok.contains_closure() || error.contains_closure() {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    "funções não podem ser payload de `Result`; use um parâmetro ou retorno direto",
                    expression.span,
                ));
            }
            ResolvedType::Result(Box::new(ok), Box::new(error))
        }
        TypeExpressionKind::Optional(payload) => {
            let payload =
                resolve_type(payload, known_structs, type_parameters, TypePosition::Field)?;
            if matches!(payload, ResolvedType::Optional(_) | ResolvedType::Void) {
                return Err(SemaError::new(
                    SemaErrorKind::UnsupportedType,
                    "opcional aninhado ou void? não é suportado",
                    expression.span,
                ));
            }
            ResolvedType::Optional(Box::new(payload))
        }
        TypeExpressionKind::Function(parameters, result) => {
            let mut resolved = Vec::with_capacity(parameters.len());
            for parameter in parameters {
                let ty = resolve_type(
                    parameter,
                    known_structs,
                    type_parameters,
                    TypePosition::Parameter,
                )?;
                if ty == ResolvedType::Void {
                    return Err(SemaError::new(
                        SemaErrorKind::UnsupportedType,
                        "parâmetro de função não pode ser `void`",
                        parameter.span,
                    ));
                }
                resolved.push(ty);
            }
            let result =
                resolve_type(result, known_structs, type_parameters, TypePosition::Return)?;
            ResolvedType::Closure(resolved, Box::new(result))
        }
    };

    let valid_position = match position {
        TypePosition::Field => resolved.is_value_type(),
        TypePosition::Parameter | TypePosition::Variable => {
            resolved.is_value_type() || matches!(resolved, ResolvedType::Closure(..))
        }
        TypePosition::Return => true,
    };
    if matches!(position, TypePosition::Field) && matches!(resolved, ResolvedType::Closure(..)) {
        return Err(SemaError::new(
            SemaErrorKind::UnsupportedType,
            "funções não podem ser campos, elementos de array nem payloads de Option/Result",
            expression.span,
        ));
    }
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
        HirExprKind::MapNew { .. } => {}
        HirExprKind::MapMethod {
            map, key, value, ..
        } => {
            collect_nested_mut_borrow_roots(map, roots);
            if let Some(key) = key {
                collect_nested_mut_borrow_roots(key, roots);
            }
            if let Some(value) = value {
                collect_nested_mut_borrow_roots(value, roots);
            }
        }
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
        HirExprKind::ArrayMethod {
            array,
            callback,
            initial,
            ..
        } => {
            collect_nested_mut_borrow_roots(array, roots);
            collect_nested_mut_borrow_roots(callback, roots);
            if let Some(initial) = initial {
                collect_nested_mut_borrow_roots(initial, roots);
            }
        }
        HirExprKind::EnumConstruct {
            fields,
            named_fields,
            ..
        } => {
            for field in fields {
                collect_nested_mut_borrow_roots(field, roots);
            }
            for field in named_fields {
                collect_nested_mut_borrow_roots(&field.value, roots);
            }
        }
        HirExprKind::FunctionValue { .. } => {}

        HirExprKind::OptionNone => {}
        HirExprKind::OptionSome(payload) => collect_nested_mut_borrow_roots(payload, roots),
        HirExprKind::ResultOk(value) => {
            if let Some(value) = value {
                collect_nested_mut_borrow_roots(value, roots);
            }
        }
        HirExprKind::ResultErr(value) => collect_nested_mut_borrow_roots(value, roots),
        HirExprKind::TryPropagate { expression, .. } => {
            collect_nested_mut_borrow_roots(expression, roots)
        }
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
        HirExprKind::MapNew { .. } => false,
        HirExprKind::MapMethod { .. } => true,
        HirExprKind::Closure { captures, .. } => {
            captures.iter().any(|(_, value)| hir_contains_call(value))
        }
        HirExprKind::ClosureCall { .. } => true,
        HirExprKind::ArrayMethod {
            array,
            callback,
            initial,
            ..
        } => {
            hir_contains_call(array)
                || hir_contains_call(callback)
                || initial.as_deref().is_some_and(hir_contains_call)
        }
        HirExprKind::EnumConstruct {
            fields,
            named_fields,
            ..
        } => {
            fields.iter().any(hir_contains_call)
                || named_fields
                    .iter()
                    .any(|field| hir_contains_call(&field.value))
        }
        HirExprKind::FunctionValue { .. } => false,
        HirExprKind::OptionNone => false,
        HirExprKind::OptionSome(payload) => hir_contains_call(payload),
        HirExprKind::ResultOk(value) => value.as_deref().is_some_and(hir_contains_call),
        HirExprKind::ResultErr(value) => hir_contains_call(value),
        HirExprKind::TryPropagate { expression, .. } => hir_contains_call(expression),
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
        HirExprKind::MapNew { .. } => {}
        HirExprKind::MapMethod {
            map, key, value, ..
        } => {
            collect_hir_binding_uses(map, uses);
            if let Some(key) = key {
                collect_hir_binding_uses(key, uses);
            }
            if let Some(value) = value {
                collect_hir_binding_uses(value, uses);
            }
        }
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
        HirExprKind::ArrayMethod {
            array,
            callback,
            initial,
            ..
        } => {
            collect_hir_binding_uses(array, uses);
            collect_hir_binding_uses(callback, uses);
            if let Some(initial) = initial {
                collect_hir_binding_uses(initial, uses);
            }
        }
        HirExprKind::EnumConstruct {
            fields,
            named_fields,
            ..
        } => {
            for field in fields {
                collect_hir_binding_uses(field, uses);
            }
            for field in named_fields {
                collect_hir_binding_uses(&field.value, uses);
            }
        }
        HirExprKind::FunctionValue { .. } => {}

        HirExprKind::OptionNone => {}
        HirExprKind::OptionSome(payload) => collect_hir_binding_uses(payload, uses),
        HirExprKind::ResultOk(value) => {
            if let Some(value) = value {
                collect_hir_binding_uses(value, uses);
            }
        }
        HirExprKind::ResultErr(value) => collect_hir_binding_uses(value, uses),
        HirExprKind::TryPropagate { expression, .. } => collect_hir_binding_uses(expression, uses),
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
        Statement::ForOf(statement) => block_mutates_name(&statement.body, name),
        Statement::Match(statement) => statement
            .arms
            .iter()
            .any(|arm| block_mutates_name(&arm.body, name)),
        Statement::TryCatch(statement) => {
            block_mutates_name(&statement.body, name)
                || (statement.catch_name.name != name
                    && block_mutates_name(&statement.catch_body, name))
        }
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
        Statement::ForOf(statement) => block_returns_name(&statement.body, name),
        Statement::Match(statement) => statement
            .arms
            .iter()
            .any(|arm| block_returns_name(&arm.body, name)),
        Statement::Throw(statement) => expression_contains_owned_name(&statement.value, name),
        Statement::TryCatch(statement) => {
            block_returns_name(&statement.body, name)
                || (statement.catch_name.name != name
                    && block_returns_name(&statement.catch_body, name))
        }
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
        ExpressionKind::TypeApplication { .. } => false,
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
        ExpressionKind::TryPropagate(inner) => expression_contains_owned_name(inner, name),
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
            Statement::Match(statement) => {
                collect_forwarded_mut_expr(&statement.value, caller, functions, needs_mut);
                for arm in &statement.arms {
                    collect_forwarded_mut_parameters(&arm.body, caller, functions, needs_mut);
                }
            }
            Statement::ForOf(statement) => {
                collect_forwarded_mut_expr(&statement.iterable, caller, functions, needs_mut);
                collect_forwarded_mut_parameters(&statement.body, caller, functions, needs_mut);
            }
            Statement::Break(_) | Statement::Continue(_) => {}
            Statement::Return(statement) => {
                if let Some(expression) = &statement.value {
                    collect_forwarded_mut_expr(expression, caller, functions, needs_mut);
                }
            }
            Statement::Throw(statement) => {
                collect_forwarded_mut_expr(&statement.value, caller, functions, needs_mut);
            }
            Statement::TryCatch(statement) => {
                collect_forwarded_mut_parameters(&statement.body, caller, functions, needs_mut);
                collect_forwarded_mut_parameters(
                    &statement.catch_body,
                    caller,
                    functions,
                    needs_mut,
                );
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
        ExpressionKind::TypeApplication { callee, .. } => {
            collect_forwarded_mut_expr(callee, caller, functions, needs_mut);
        }
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
        ExpressionKind::TryPropagate(inner) => {
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

/// Todas as rotas do bloco saem dele (`return`, `throw` ou `if` com os dois
/// braços saindo). `throw` sai do bloco mesmo quando o destino é um `catch`.
fn block_guarantees_exit(block: &Block) -> bool {
    for statement in &block.statements {
        match statement {
            Statement::Return(_) | Statement::Throw(_) => return true,
            Statement::If(statement) => {
                if if_guarantees_exit(statement) {
                    return true;
                }
            }
            Statement::Match(statement) => {
                if statement
                    .arms
                    .iter()
                    .all(|arm| block_guarantees_exit(&arm.body))
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn if_guarantees_exit(statement: &lift_ast::IfStatement) -> bool {
    block_guarantees_exit(&statement.then_branch)
        && statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_guarantees_exit(block),
                ElseBranchKind::ElseIf(nested) => if_guarantees_exit(nested),
            })
}

/// Todas as rotas retornam da função. `try_depth` conta blocos `try` ativos:
/// um `throw` só retorna da função quando não há catch interno (depth 0).
fn block_guarantees_return(block: &Block, try_depth: usize) -> bool {
    for statement in &block.statements {
        match statement {
            Statement::Return(_) => return true,
            Statement::Throw(_) => return try_depth == 0,
            Statement::If(statement) => {
                if if_guarantees_return(statement, try_depth) {
                    return true;
                }
            }
            Statement::Match(statement) => {
                if statement
                    .arms
                    .iter()
                    .all(|arm| block_guarantees_return(&arm.body, try_depth))
                {
                    return true;
                }
            }
            Statement::TryCatch(statement) => {
                if block_guarantees_return(&statement.body, try_depth + 1)
                    || (block_guarantees_exit(&statement.body)
                        && block_guarantees_return(&statement.catch_body, try_depth))
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn if_guarantees_return(statement: &lift_ast::IfStatement, try_depth: usize) -> bool {
    block_guarantees_return(&statement.then_branch, try_depth)
        && statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_guarantees_return(block, try_depth),
                ElseBranchKind::ElseIf(nested) => if_guarantees_return(nested, try_depth),
            })
}

fn dedupe_facts(facts: Vec<NarrowFact>) -> Vec<NarrowFact> {
    let mut seen = HashSet::new();
    facts
        .into_iter()
        .filter(|fact| seen.insert(fact.source))
        .collect()
}

/// Mantém apenas fatos válidos em ambos os caminhos de continuação.
/// PT-BR: interseção de fatos de caminhos alternativos.
fn intersect_facts(left: &[NarrowFact], right: &[NarrowFact]) -> Vec<NarrowFact> {
    let mut kept: Vec<NarrowFact> = Vec::new();
    for fact in left {
        let Some(other) = right.iter().find(|other| other.source == fact.source) else {
            continue;
        };
        if other.payload != fact.payload
            || kept.iter().any(|existing| existing.source == fact.source)
        {
            continue;
        }
        kept.push(fact.clone());
    }
    kept
}

// Detecção textual conservadora de uso de um nome, usada apenas para evitar
// clones de narrowing quando o binding derivado nunca seria lido.
// PT-BR: uso textual conservador do nome para evitar clones desnecessários.
fn block_mentions_name(block: &Block, name: &str) -> bool {
    block
        .statements
        .iter()
        .any(|statement| statement_mentions_name(statement, name))
}

fn statement_mentions_name(statement: &Statement, name: &str) -> bool {
    match statement {
        Statement::Variable(variable) => expression_mentions_name(&variable.initializer, name),
        Statement::Assignment(assignment) => {
            expression_mentions_name(&assignment.target, name)
                || expression_mentions_name(&assignment.value, name)
        }
        Statement::If(statement) => if_mentions_name(statement, name),
        Statement::While(statement) => {
            expression_mentions_name(&statement.condition, name)
                || block_mentions_name(&statement.body, name)
        }
        Statement::Match(statement) => {
            expression_mentions_name(&statement.value, name)
                || statement.arms.iter().any(|arm| {
                    let binding_matches = match &arm.pattern.kind {
                        lift_ast::MatchPatternKind::Some(binding)
                        | lift_ast::MatchPatternKind::Err(binding) => binding.name == name,
                        lift_ast::MatchPatternKind::Ok(Some(binding)) => binding.name == name,
                        lift_ast::MatchPatternKind::EnumVariant { bindings, .. } => {
                            bindings.iter().any(|binding| binding.name == name)
                        }
                        lift_ast::MatchPatternKind::None | lift_ast::MatchPatternKind::Ok(None) => {
                            false
                        }
                    };
                    !binding_matches && block_mentions_name(&arm.body, name)
                })
        }
        Statement::ForOf(statement) => {
            expression_mentions_name(&statement.iterable, name)
                || (statement.name.name != name && block_mentions_name(&statement.body, name))
        }
        Statement::Break(_) | Statement::Continue(_) => false,
        Statement::Return(statement) => statement
            .value
            .as_ref()
            .is_some_and(|value| expression_mentions_name(value, name)),
        Statement::Throw(statement) => expression_mentions_name(&statement.value, name),
        Statement::TryCatch(statement) => {
            block_mentions_name(&statement.body, name)
                || (statement.catch_name.name != name
                    && block_mentions_name(&statement.catch_body, name))
        }
        Statement::Expression(statement) => expression_mentions_name(&statement.expression, name),
    }
}

fn if_mentions_name(statement: &lift_ast::IfStatement, name: &str) -> bool {
    expression_mentions_name(&statement.condition, name)
        || block_mentions_name(&statement.then_branch, name)
        || statement
            .else_branch
            .as_ref()
            .is_some_and(|branch| match &branch.kind {
                ElseBranchKind::Block(block) => block_mentions_name(block, name),
                ElseBranchKind::ElseIf(nested) => if_mentions_name(nested, name),
            })
}

fn expression_mentions_name(expression: &Expression, name: &str) -> bool {
    match &expression.kind {
        ExpressionKind::Identifier(identifier) => identifier.name == name,
        ExpressionKind::Closure(closure) => block_mentions_name(&closure.body, name),
        ExpressionKind::TypeApplication { callee, .. } => expression_mentions_name(callee, name),
        ExpressionKind::Template(parts) => parts
            .iter()
            .any(|part| expression_mentions_name(part, name)),
        ExpressionKind::IntegerLiteral(_)
        | ExpressionKind::NumberLiteral(_)
        | ExpressionKind::StringLiteral(_)
        | ExpressionKind::BooleanLiteral(_)
        | ExpressionKind::NullLiteral => false,
        ExpressionKind::ObjectLiteral(object) => object
            .fields
            .iter()
            .any(|field| expression_mentions_name(&field.value, name)),
        ExpressionKind::ArrayLiteral(array) => array
            .elements
            .iter()
            .any(|element| expression_mentions_name(element, name)),
        ExpressionKind::Unary(unary) => expression_mentions_name(&unary.operand, name),
        ExpressionKind::Binary(binary) => {
            expression_mentions_name(&binary.left, name)
                || expression_mentions_name(&binary.right, name)
        }
        ExpressionKind::FieldAccess(access) => expression_mentions_name(&access.object, name),
        ExpressionKind::Call(call) => {
            expression_mentions_name(&call.callee, name)
                || call
                    .arguments
                    .iter()
                    .any(|argument| expression_mentions_name(argument, name))
        }
        ExpressionKind::Index(index) => {
            expression_mentions_name(&index.object, name)
                || expression_mentions_name(&index.index, name)
        }
        ExpressionKind::Grouping(inner) => expression_mentions_name(inner, name),
        ExpressionKind::TryPropagate(inner) => expression_mentions_name(inner, name),
    }
}

fn visit_struct(
    name: &str,
    structures: &HashMap<String, StructInfo>,
    state: &mut HashMap<String, u8>,
    stack: &mut Vec<String>,
) -> Option<Span> {
    // O layout de um tipo Rust externo é responsabilidade da crate; não há
    // aresta de tamanho a percorrer a partir dele.
    if structures.get(name).is_some_and(|info| info.external) {
        return None;
    }
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
                HirStatement::ForOf(statement) => apply_block(&mut statement.body, mutable),
                HirStatement::Match(statement) => {
                    for arm in &mut statement.arms {
                        match &mut arm.pattern {
                            HirMatchPattern::Some(binding) | HirMatchPattern::Err(binding) => {
                                binding.requires_mut = mutable.contains(&binding.id);
                            }
                            HirMatchPattern::Ok(Some(binding)) => {
                                binding.requires_mut = mutable.contains(&binding.id);
                            }
                            HirMatchPattern::EnumVariant { bindings, .. } => {
                                for binding in bindings {
                                    binding.requires_mut = mutable.contains(&binding.id);
                                }
                            }
                            HirMatchPattern::None | HirMatchPattern::Ok(None) => {}
                        }
                        apply_block(&mut arm.body, mutable);
                    }
                }
                HirStatement::TryCatch(statement) => {
                    apply_block(&mut statement.body, mutable);
                    apply_block(&mut statement.catch_body, mutable);
                }
                HirStatement::Throw(_)
                | HirStatement::Narrow(_)
                | HirStatement::Assignment(_)
                | HirStatement::Return(_)
                | HirStatement::Break(_)
                | HirStatement::Continue(_)
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
            HirDeclaration::Struct(_)
            | HirDeclaration::Enum(_)
            | HirDeclaration::ExternStruct(_)
            | HirDeclaration::Import(_) => {}
        }
    }
}

fn map_types(name: &str) -> Option<(ResolvedType, ResolvedType)> {
    let inner = name.strip_prefix("Map<")?.strip_suffix('>')?;
    let (key, value) = inner.split_once(", ")?;
    Some((simple_map_type(key)?, simple_map_type(value)?))
}

fn simple_map_type(name: &str) -> Option<ResolvedType> {
    match name {
        "int" => Some(ResolvedType::Int),
        "number" => Some(ResolvedType::Number),
        "string" => Some(ResolvedType::String),
        "boolean" => Some(ResolvedType::Boolean),
        _ => None,
    }
}
