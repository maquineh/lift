//! Monomorphization of generic interfaces (`Box<T>`, `Map<K, V>`) into
//! synthetic concrete interface declarations before semantic analysis.
//!
//! PT-BR: monomorfização de interfaces genéricas em declarações concretas.

use std::collections::{HashMap, HashSet};

use lift_ast::{
    Block, Declaration, ElseBranchKind, Expression, ExpressionKind, Identifier,
    InterfaceDeclaration, ProgramAst, Span, Statement, TypeExpression, TypeExpressionKind,
};

use crate::{SemaError, SemaErrorKind};

pub(super) fn expand_generic_interfaces(ast: &mut ProgramAst, errors: &mut Vec<SemaError>) {
    let mut templates = HashMap::new();
    for declaration in &ast.declarations {
        let Declaration::Interface(interface) = declaration else {
            continue;
        };
        if interface.type_parameters.is_empty() {
            continue;
        }
        let mut seen = HashSet::new();
        for parameter in &interface.type_parameters {
            if !seen.insert(parameter.name.clone()) {
                errors.push(SemaError::new(
                    SemaErrorKind::DuplicateParameter,
                    format!(
                        "parâmetro de tipo duplicado `{}` em `{}`",
                        parameter.name, interface.name.name
                    ),
                    parameter.span,
                ));
            }
        }
        templates.insert(interface.name.name.clone(), interface.clone());
    }
    if templates.is_empty() && !ast.declarations.iter().any(is_generic_interface_type) {
        return;
    }

    ast.declarations
        .retain(|declaration| !matches!(declaration, Declaration::Interface(interface) if !interface.type_parameters.is_empty()));

    let mut monomorphizer = Monomorphizer {
        templates,
        generated: HashMap::new(),
        order: Vec::new(),
        errors,
    };
    for declaration in &mut ast.declarations {
        monomorphizer.declaration(declaration);
    }
    for name in monomorphizer.order.clone() {
        if let Some(interface) = monomorphizer.generated.remove(&name) {
            ast.declarations.push(Declaration::Interface(interface));
        }
    }
}

fn is_generic_interface_type(_declaration: &Declaration) -> bool {
    // Usado apenas para evitar montar o monomorphizer quando não há templates;
    // aplicações órfãs continuam caindo no erro semântico normal.
    false
}

struct Monomorphizer<'a> {
    templates: HashMap<String, InterfaceDeclaration>,
    generated: HashMap<String, InterfaceDeclaration>,
    order: Vec<String>,
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
                let Some(template) = self.templates.get(&name.name).cloned() else {
                    return;
                };
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
                if !self.generated.contains_key(&canonical) {
                    self.generated.insert(
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
                    self.order.push(canonical.clone());
                    let substitutions = template
                        .type_parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .zip(arguments.iter().cloned())
                        .collect::<HashMap<_, _>>();
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
                    if let Some(generated) = self.generated.get_mut(&canonical) {
                        generated.fields = fields;
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
