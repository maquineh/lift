//! Monomorphization of generic interfaces/enums (`Box<T>`, `Option<T>`) into
//! synthetic concrete declarations before semantic analysis.
//!
//! PT-BR: monomorfização de interfaces genéricas em declarações concretas.

use std::collections::{HashMap, HashSet};

use lift_ast::{
    Block, Declaration, ElseBranchKind, Expression, ExpressionKind, Identifier,
    InterfaceDeclaration, ProgramAst, Span, Statement, TypeExpression, TypeExpressionKind,
};

use crate::{SemaError, SemaErrorKind};

pub(super) fn expand_generic_containers(ast: &mut ProgramAst, errors: &mut Vec<SemaError>) {
    let mut interface_templates = HashMap::new();
    let mut enum_templates = HashMap::new();
    for declaration in &ast.declarations {
        match declaration {
            Declaration::Interface(interface) if !interface.type_parameters.is_empty() => {
                validate_parameters(&interface.name, &interface.type_parameters, errors);
                interface_templates.insert(interface.name.name.clone(), interface.clone());
            }
            Declaration::Enum(enumeration) if !enumeration.type_parameters.is_empty() => {
                validate_parameters(&enumeration.name, &enumeration.type_parameters, errors);
                enum_templates.insert(enumeration.name.name.clone(), enumeration.clone());
            }
            _ => {}
        }
    }
    if interface_templates.is_empty() && enum_templates.is_empty() {
        return;
    }

    ast.declarations.retain(|declaration| match declaration {
        Declaration::Interface(interface) => interface.type_parameters.is_empty(),
        Declaration::Enum(enumeration) => enumeration.type_parameters.is_empty(),
        _ => true,
    });

    let mut monomorphizer = Monomorphizer {
        interface_templates,
        enum_templates,
        generated_interfaces: HashMap::new(),
        generated_enums: HashMap::new(),
        order: Vec::new(),
        errors,
    };
    for declaration in &mut ast.declarations {
        monomorphizer.declaration(declaration);
    }
    for (name, kind) in monomorphizer.order.clone() {
        match kind {
            GeneratedKind::Interface => {
                if let Some(interface) = monomorphizer.generated_interfaces.remove(&name) {
                    ast.declarations.push(Declaration::Interface(interface));
                }
            }
            GeneratedKind::Enum => {
                if let Some(enumeration) = monomorphizer.generated_enums.remove(&name) {
                    ast.declarations.push(Declaration::Enum(enumeration));
                }
            }
        }
    }
}

fn validate_parameters(name: &Identifier, parameters: &[Identifier], errors: &mut Vec<SemaError>) {
    let mut seen = HashSet::new();
    for parameter in parameters {
        if !seen.insert(parameter.name.clone()) {
            errors.push(SemaError::new(
                SemaErrorKind::DuplicateParameter,
                format!(
                    "parâmetro de tipo duplicado `{}` em `{}`",
                    parameter.name, name.name
                ),
                parameter.span,
            ));
        }
    }
}

#[derive(Clone, Copy)]
enum GeneratedKind {
    Interface,
    Enum,
}

struct Monomorphizer<'a> {
    interface_templates: HashMap<String, InterfaceDeclaration>,
    enum_templates: HashMap<String, lift_ast::EnumDeclaration>,
    generated_interfaces: HashMap<String, InterfaceDeclaration>,
    generated_enums: HashMap<String, lift_ast::EnumDeclaration>,
    order: Vec<(String, GeneratedKind)>,
    errors: &'a mut Vec<SemaError>,
}

impl Monomorphizer<'_> {
    fn declaration(&mut self, declaration: &mut Declaration) {
        match declaration {
            Declaration::Interface(interface) => {
                for field in &mut interface.fields {
                    self.ty(&mut field.ty);
                }
            }
            Declaration::ImportInterface(interface) => {
                for field in &mut interface.fields {
                    self.ty(&mut field.ty);
                }
            }
            Declaration::Enum(enumeration) => {
                for variant in &mut enumeration.variants {
                    for field in &mut variant.fields {
                        self.ty(field);
                    }
                    for field in &mut variant.named_fields {
                        self.ty(&mut field.ty);
                    }
                }
            }
            Declaration::Union(union) => {
                for member in &mut union.members {
                    self.ty(member);
                }
            }
            Declaration::Function(function) => {
                for parameter in &mut function.parameters {
                    self.ty(&mut parameter.ty);
                }
                if let Some(return_type) = &mut function.return_type {
                    self.ty(return_type);
                }
                self.block(&mut function.body);
            }
            Declaration::ExportFunction(function) => {
                for parameter in &mut function.parameters {
                    self.ty(&mut parameter.ty);
                }
                if let Some(return_type) = &mut function.return_type {
                    self.ty(return_type);
                }
                self.block(&mut function.body);
            }
            Declaration::Import(import) => {
                for parameter in &mut import.parameters {
                    self.ty(&mut parameter.ty);
                }
                self.ty(&mut import.return_type);
            }
            Declaration::Statement(statement) => self.statement(statement),
            Declaration::Impl(_) => {}
            Declaration::ModuleImport(_) | Declaration::PackageImport(_) => {}
        }
    }

    fn block(&mut self, block: &mut Block) {
        for statement in &mut block.statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &mut Statement) {
        match statement {
            Statement::Variable(variable) => {
                if let Some(annotation) = &mut variable.annotation {
                    self.ty(annotation);
                }
                self.expression(&mut variable.initializer);
            }
            Statement::Assignment(assignment) => self.expression(&mut assignment.value),
            Statement::If(statement) => {
                self.expression(&mut statement.condition);
                self.block(&mut statement.then_branch);
                if let Some(branch) = &mut statement.else_branch {
                    self.else_branch(branch);
                }
            }
            Statement::While(statement) => {
                self.expression(&mut statement.condition);
                self.block(&mut statement.body);
            }
            Statement::ForOf(statement) => {
                self.expression(&mut statement.iterable);
                self.block(&mut statement.body);
            }
            Statement::Match(statement) => {
                self.expression(&mut statement.value);
                for arm in &mut statement.arms {
                    self.block(&mut arm.body);
                }
            }
            Statement::TryCatch(statement) => {
                self.block(&mut statement.body);
                self.block(&mut statement.catch_body);
            }
            Statement::Return(statement) => {
                if let Some(value) = &mut statement.value {
                    self.expression(value);
                }
            }
            Statement::Throw(statement) => self.expression(&mut statement.value),
            Statement::Expression(statement) => self.expression(&mut statement.expression),
            Statement::Break(_) | Statement::Continue(_) => {}
        }
    }

    fn else_branch(&mut self, branch: &mut lift_ast::ElseBranch) {
        match &mut branch.kind {
            ElseBranchKind::Block(block) => self.block(block),
            ElseBranchKind::ElseIf(statement) => {
                self.expression(&mut statement.condition);
                self.block(&mut statement.then_branch);
                if let Some(branch) = &mut statement.else_branch {
                    self.else_branch(branch);
                }
            }
        }
    }

    fn expression(&mut self, expression: &mut Expression) {
        match &mut expression.kind {
            ExpressionKind::Closure(closure) => {
                for parameter in &mut closure.parameters {
                    self.ty(&mut parameter.ty);
                }
                self.ty(&mut closure.return_type);
                self.block(&mut closure.body);
            }
            ExpressionKind::Template(parts) => {
                for part in parts {
                    self.expression(part);
                }
            }
            ExpressionKind::TryPropagate(inner) | ExpressionKind::Grouping(inner) => {
                self.expression(inner)
            }
            ExpressionKind::ObjectLiteral(object) => {
                for field in &mut object.fields {
                    self.expression(&mut field.value);
                }
            }
            ExpressionKind::ArrayLiteral(array) => {
                for element in &mut array.elements {
                    self.expression(element);
                }
            }
            ExpressionKind::Unary(unary) => self.expression(&mut unary.operand),
            ExpressionKind::Binary(binary) => {
                self.expression(&mut binary.left);
                self.expression(&mut binary.right);
            }
            ExpressionKind::FieldAccess(access) => self.expression(&mut access.object),
            ExpressionKind::Call(call) => {
                self.expression(&mut call.callee);
                for argument in &mut call.arguments {
                    self.expression(argument);
                }
            }
            ExpressionKind::Index(index) => {
                self.expression(&mut index.object);
                self.expression(&mut index.index);
            }
            ExpressionKind::IntegerLiteral(_)
            | ExpressionKind::NumberLiteral(_)
            | ExpressionKind::StringLiteral(_)
            | ExpressionKind::BooleanLiteral(_)
            | ExpressionKind::NullLiteral
            | ExpressionKind::Identifier(_) => {}
        }
    }

    fn ty(&mut self, ty: &mut TypeExpression) {
        match &mut ty.kind {
            TypeExpressionKind::Named(_) => {}
            TypeExpressionKind::Array(element) | TypeExpressionKind::Optional(element) => {
                self.ty(element)
            }
            TypeExpressionKind::Result(ok, error) => {
                self.ty(ok);
                self.ty(error);
            }
            TypeExpressionKind::Function(parameters, result) => {
                for parameter in parameters {
                    self.ty(parameter);
                }
                self.ty(result);
            }
            TypeExpressionKind::Generic { name, arguments } => {
                for argument in arguments.iter_mut() {
                    self.ty(argument);
                }
                if let Some(template) = self.interface_templates.get(&name.name).cloned() {
                    if template.type_parameters.len() != arguments.len() {
                        self.errors.push(SemaError::new(
                            SemaErrorKind::ArgumentCountMismatch,
                            format!(
                                "container `{}` espera {} argumento(s) de tipo, recebeu {}",
                                name.name,
                                template.type_parameters.len(),
                                arguments.len()
                            ),
                            ty.span,
                        ));
                        return;
                    }
                    let canonical = canonical_name(&name.name, arguments);
                    if !self.generated_interfaces.contains_key(&canonical)
                        && !self.generated_enums.contains_key(&canonical)
                    {
                        self.generated_interfaces.insert(
                            canonical.clone(),
                            InterfaceDeclaration {
                                name: Identifier {
                                    name: canonical.clone(),
                                    span: name.span,
                                },
                                type_parameters: Vec::new(),
                                fields: Vec::new(),
                                span: template.span,
                            },
                        );
                        self.order
                            .push((canonical.clone(), GeneratedKind::Interface));
                        let substitutions = substitutions(&template.type_parameters, arguments);
                        let mut fields = template
                            .fields
                            .iter()
                            .map(|field| {
                                let mut field = field.clone();
                                field.ty = substitute(&field.ty, &substitutions);
                                field
                            })
                            .collect::<Vec<_>>();
                        for field in &mut fields {
                            self.ty(&mut field.ty);
                        }
                        if let Some(generated) = self.generated_interfaces.get_mut(&canonical) {
                            generated.fields = fields;
                        }
                    }
                    ty.kind = TypeExpressionKind::Named(Identifier {
                        name: canonical,
                        span: name.span,
                    });
                } else if let Some(template) = self.enum_templates.get(&name.name).cloned() {
                    if template.type_parameters.len() != arguments.len() {
                        self.errors.push(SemaError::new(
                            SemaErrorKind::ArgumentCountMismatch,
                            format!(
                                "enum genérico `{}` espera {} argumento(s) de tipo, recebeu {}",
                                name.name,
                                template.type_parameters.len(),
                                arguments.len()
                            ),
                            ty.span,
                        ));
                        return;
                    }
                    let canonical = canonical_name(&name.name, arguments);
                    if !self.generated_interfaces.contains_key(&canonical)
                        && !self.generated_enums.contains_key(&canonical)
                    {
                        self.generated_enums.insert(
                            canonical.clone(),
                            lift_ast::EnumDeclaration {
                                name: Identifier {
                                    name: canonical.clone(),
                                    span: name.span,
                                },
                                type_parameters: Vec::new(),
                                variants: Vec::new(),
                                span: template.span,
                            },
                        );
                        self.order.push((canonical.clone(), GeneratedKind::Enum));
                        let substitutions = substitutions(&template.type_parameters, arguments);
                        let mut variants = template
                            .variants
                            .iter()
                            .map(|variant| {
                                let mut variant = variant.clone();
                                for field in &mut variant.fields {
                                    *field = substitute(field, &substitutions);
                                }
                                for field in &mut variant.named_fields {
                                    field.ty = substitute(&field.ty, &substitutions);
                                }
                                variant
                            })
                            .collect::<Vec<_>>();
                        for variant in &mut variants {
                            for field in &mut variant.fields {
                                self.ty(field);
                            }
                            for field in &mut variant.named_fields {
                                self.ty(&mut field.ty);
                            }
                        }
                        if let Some(generated) = self.generated_enums.get_mut(&canonical) {
                            generated.variants = variants;
                        }
                    }
                    ty.kind = TypeExpressionKind::Named(Identifier {
                        name: canonical,
                        span: name.span,
                    });
                }
            }
        }
    }
}

fn substitutions(
    parameters: &[Identifier],
    arguments: &[TypeExpression],
) -> HashMap<String, TypeExpression> {
    parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .zip(arguments.iter().cloned())
        .collect()
}

fn substitute(
    ty: &TypeExpression,
    substitutions: &HashMap<String, TypeExpression>,
) -> TypeExpression {
    let kind = match &ty.kind {
        TypeExpressionKind::Named(identifier) => {
            if let Some(replacement) = substitutions.get(&identifier.name) {
                return replacement.clone();
            }
            TypeExpressionKind::Named(identifier.clone())
        }
        TypeExpressionKind::Array(element) => {
            TypeExpressionKind::Array(Box::new(substitute(element, substitutions)))
        }
        TypeExpressionKind::Optional(element) => {
            TypeExpressionKind::Optional(Box::new(substitute(element, substitutions)))
        }
        TypeExpressionKind::Result(ok, error) => TypeExpressionKind::Result(
            Box::new(substitute(ok, substitutions)),
            Box::new(substitute(error, substitutions)),
        ),
        TypeExpressionKind::Function(parameters, result) => TypeExpressionKind::Function(
            parameters
                .iter()
                .map(|parameter| substitute(parameter, substitutions))
                .collect(),
            Box::new(substitute(result, substitutions)),
        ),
        TypeExpressionKind::Generic { name, arguments } => TypeExpressionKind::Generic {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute(argument, substitutions))
                .collect(),
        },
    };
    TypeExpression {
        kind,
        span: ty.span,
    }
}

fn canonical_name(name: &str, arguments: &[TypeExpression]) -> String {
    let arguments = arguments
        .iter()
        .map(canonical_type)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}<{arguments}>")
}

fn canonical_type(ty: &TypeExpression) -> String {
    match &ty.kind {
        TypeExpressionKind::Named(identifier) => identifier.name.clone(),
        TypeExpressionKind::Array(element) => format!("{}[]", canonical_type(element)),
        TypeExpressionKind::Optional(element) => format!("{}?", canonical_type(element)),
        TypeExpressionKind::Result(ok, error) => {
            format!("Result<{}, {}>", canonical_type(ok), canonical_type(error))
        }
        TypeExpressionKind::Function(parameters, result) => format!(
            "({}) => {}",
            parameters
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(", "),
            canonical_type(result)
        ),
        TypeExpressionKind::Generic { name, arguments } => canonical_name(&name.name, arguments),
    }
}
