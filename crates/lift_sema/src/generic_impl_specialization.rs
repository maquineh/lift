//! Monomorphize generic impl blocks (`impl Box<T>`) into concrete impls for
//! every discovered container application (`impl Box<int>`).
//!
//! PT-BR: monomorfiza impls genéricos para cada aplicação de container.

use std::collections::{HashMap, HashSet};

use lift_ast::{
    Block, Declaration, ElseBranchKind, Expression, ExpressionKind, FunctionDeclaration,
    Identifier, ImplDeclaration, ProgramAst, Statement, TypeExpression, TypeExpressionKind,
};

use crate::{SemaError, SemaErrorKind};

pub(super) fn specialize_generic_impls(ast: &mut ProgramAst, errors: &mut Vec<SemaError>) {
    let mut templates = Vec::new();
    for declaration in &ast.declarations {
        let Declaration::Impl(implementation) = declaration else {
            continue;
        };
        if !implementation.type_parameters.is_empty() {
            templates.push(implementation.clone());
        }
    }
    if templates.is_empty() {
        return;
    }

    let mut applications = HashMap::<String, (String, Vec<TypeExpression>)>::new();
    for declaration in &ast.declarations {
        collect_declaration_types(declaration, &mut applications);
    }

    let mut generated = Vec::new();
    for template in &templates {
        for (canonical, (target, arguments)) in &applications {
            if target != &template.target.name {
                continue;
            }
            if arguments.len() != template.type_parameters.len() {
                errors.push(SemaError::new(
                    SemaErrorKind::ArgumentCountMismatch,
                    format!(
                        "impl de `{}` espera {} argumento(s) de tipo",
                        template.target.name,
                        template.type_parameters.len()
                    ),
                    template.span,
                ));
                continue;
            }
            let substitutions = template
                .type_parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .zip(arguments.iter().cloned())
                .collect::<HashMap<_, _>>();
            let mut methods = template.methods.clone();
            for method in &mut methods {
                super::generic_function_specialization::substitute_function_types(
                    method,
                    &substitutions,
                );
            }
            generated.push(ImplDeclaration {
                target: Identifier {
                    name: canonical.clone(),
                    span: template.target.span,
                },
                type_parameters: Vec::new(),
                methods,
                span: template.span,
            });
        }
    }

    ast.declarations.retain(|declaration| {
        !matches!(declaration, Declaration::Impl(implementation) if !implementation.type_parameters.is_empty())
    });
    for implementation in generated {
        ast.declarations.push(Declaration::Impl(implementation));
    }
}

fn collect_declaration_types(
    declaration: &Declaration,
    applications: &mut HashMap<String, (String, Vec<TypeExpression>)>,
) {
    match declaration {
        Declaration::Interface(interface) => {
            for field in &interface.fields {
                collect_type(&field.ty, applications);
            }
        }
        Declaration::ImportInterface(interface) => {
            for field in &interface.fields {
                collect_type(&field.ty, applications);
            }
        }
        Declaration::Enum(enumeration) => {
            for variant in &enumeration.variants {
                for field in &variant.fields {
                    collect_type(field, applications);
                }
                for field in &variant.named_fields {
                    collect_type(&field.ty, applications);
                }
            }
        }
        Declaration::Union(union) => {
            for member in &union.members {
                collect_type(member, applications);
            }
        }
        Declaration::Function(function) => collect_function(function, applications),
        Declaration::ExportFunction(function) => collect_function(function, applications),
        Declaration::Impl(implementation) if implementation.type_parameters.is_empty() => {
            for method in &implementation.methods {
                collect_function(method, applications);
            }
        }
        Declaration::Impl(_) => {}
        Declaration::Import(import) => {
            for parameter in &import.parameters {
                collect_type(&parameter.ty, applications);
            }
            collect_type(&import.return_type, applications);
        }
        Declaration::Statement(statement) => collect_statement(statement, applications),
        Declaration::ModuleImport(_) | Declaration::PackageImport(_) => {}
    }
}

fn collect_function(
    function: &FunctionDeclaration,
    applications: &mut HashMap<String, (String, Vec<TypeExpression>)>,
) {
    for parameter in &function.parameters {
        collect_type(&parameter.ty, applications);
    }
    if let Some(return_type) = &function.return_type {
        collect_type(return_type, applications);
    }
    collect_block(&function.body, applications);
}

fn collect_block(block: &Block, applications: &mut HashMap<String, (String, Vec<TypeExpression>)>) {
    for statement in &block.statements {
        collect_statement(statement, applications);
    }
}

fn collect_statement(
    statement: &Statement,
    applications: &mut HashMap<String, (String, Vec<TypeExpression>)>,
) {
    match statement {
        Statement::Variable(variable) => {
            if let Some(annotation) = &variable.annotation {
                collect_type(annotation, applications);
            }
            collect_expression(&variable.initializer, applications);
        }
        Statement::Assignment(assignment) => {
            collect_expression(&assignment.value, applications);
        }
        Statement::If(statement) => {
            collect_expression(&statement.condition, applications);
            collect_block(&statement.then_branch, applications);
            if let Some(branch) = &statement.else_branch {
                collect_branch(branch, applications);
            }
        }
        Statement::While(statement) => {
            collect_expression(&statement.condition, applications);
            collect_block(&statement.body, applications);
        }
        Statement::ForOf(statement) => {
            collect_expression(&statement.iterable, applications);
            collect_block(&statement.body, applications);
        }
        Statement::Match(statement) => {
            collect_expression(&statement.value, applications);
            for arm in &statement.arms {
                collect_block(&arm.body, applications);
            }
        }
        Statement::TryCatch(statement) => {
            collect_block(&statement.body, applications);
            collect_block(&statement.catch_body, applications);
        }
        Statement::Return(statement) => {
            if let Some(value) = &statement.value {
                collect_expression(value, applications);
            }
        }
        Statement::Throw(statement) => collect_expression(&statement.value, applications),
        Statement::Expression(statement) => {
            collect_expression(&statement.expression, applications);
        }
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn collect_branch(
    branch: &lift_ast::ElseBranch,
    applications: &mut HashMap<String, (String, Vec<TypeExpression>)>,
) {
    match &branch.kind {
        ElseBranchKind::Block(block) => collect_block(block, applications),
        ElseBranchKind::ElseIf(statement) => {
            collect_expression(&statement.condition, applications);
            collect_block(&statement.then_branch, applications);
            if let Some(branch) = &statement.else_branch {
                collect_branch(branch, applications);
            }
        }
    }
}

fn collect_expression(
    expression: &Expression,
    applications: &mut HashMap<String, (String, Vec<TypeExpression>)>,
) {
    match &expression.kind {
        ExpressionKind::Closure(closure) => {
            for parameter in &closure.parameters {
                collect_type(&parameter.ty, applications);
            }
            collect_type(&closure.return_type, applications);
            collect_block(&closure.body, applications);
        }
        ExpressionKind::Template(parts) => {
            for part in parts {
                collect_expression(part, applications);
            }
        }
        ExpressionKind::TryPropagate(inner) | ExpressionKind::Grouping(inner) => {
            collect_expression(inner, applications)
        }
        ExpressionKind::ObjectLiteral(object) => {
            for field in &object.fields {
                collect_expression(&field.value, applications);
            }
        }
        ExpressionKind::ArrayLiteral(array) => {
            for element in &array.elements {
                collect_expression(element, applications);
            }
        }
        ExpressionKind::Unary(unary) => collect_expression(&unary.operand, applications),
        ExpressionKind::Binary(binary) => {
            collect_expression(&binary.left, applications);
            collect_expression(&binary.right, applications);
        }
        ExpressionKind::FieldAccess(access) => {
            collect_expression(&access.object, applications);
        }
        ExpressionKind::Call(call) => {
            collect_expression(&call.callee, applications);
            for argument in &call.arguments {
                collect_expression(argument, applications);
            }
        }
        ExpressionKind::TypeApplication {
            callee, arguments, ..
        } => {
            collect_expression(callee, applications);
            for argument in arguments {
                collect_type(argument, applications);
            }
        }
        ExpressionKind::Index(index) => {
            collect_expression(&index.object, applications);
            collect_expression(&index.index, applications);
        }
        ExpressionKind::IntegerLiteral(_)
        | ExpressionKind::NumberLiteral(_)
        | ExpressionKind::StringLiteral(_)
        | ExpressionKind::BooleanLiteral(_)
        | ExpressionKind::NullLiteral
        | ExpressionKind::Identifier(_) => {}
    }
}

fn collect_type(
    ty: &TypeExpression,
    applications: &mut HashMap<String, (String, Vec<TypeExpression>)>,
) {
    match &ty.kind {
        TypeExpressionKind::Generic { name, arguments } => {
            let canonical = canonical_type(ty);
            applications
                .entry(canonical)
                .or_insert_with(|| (name.name.clone(), arguments.clone()));
            for argument in arguments {
                collect_type(argument, applications);
            }
        }
        TypeExpressionKind::Array(element) | TypeExpressionKind::Optional(element) => {
            collect_type(element, applications)
        }
        TypeExpressionKind::Result(ok, error) => {
            collect_type(ok, applications);
            collect_type(error, applications);
        }
        TypeExpressionKind::Function(parameters, result) => {
            for parameter in parameters {
                collect_type(parameter, applications);
            }
            collect_type(result, applications);
        }
        TypeExpressionKind::Named(_) => {}
    }
}

fn canonical_type(ty: &TypeExpression) -> String {
    match &ty.kind {
        TypeExpressionKind::Named(identifier) => identifier.name.clone(),
        TypeExpressionKind::Generic { name, arguments } => format!(
            "{}<{}>",
            name.name,
            arguments
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
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
    }
}
