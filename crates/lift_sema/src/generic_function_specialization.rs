//! Specialize generic functions whose signatures mention generic containers,
//! using explicit call-site type arguments (`unwrap::<int>(box)`).
//!
//! PT-BR: especializa funções genéricas com containers em argumentos
//! explícitos de chamada.

use std::collections::{HashMap, HashSet};

use lift_ast::{
    Block, Declaration, ElseBranchKind, Expression, ExpressionKind, FunctionDeclaration,
    Identifier, ProgramAst, Statement, TypeExpression, TypeExpressionKind, VariableDeclaration,
};

use crate::{SemaError, SemaErrorKind};

pub(super) fn specialize_generic_functions(ast: &mut ProgramAst, errors: &mut Vec<SemaError>) {
    let mut templates = HashMap::new();
    for declaration in &ast.declarations {
        if let Declaration::Function(function) = declaration {
            if !function.type_parameters.is_empty() && signature_uses_container(function) {
                templates.insert(function.name.name.clone(), function.clone());
            }
        }
    }
    if templates.is_empty() {
        return;
    }

    let mut requests = HashMap::<String, Request>::new();
    for declaration in &mut ast.declarations {
        match declaration {
            Declaration::Function(function) if !templates.contains_key(&function.name.name) => {
                rewrite_block(&mut function.body, &templates, &mut requests, errors);
            }
            Declaration::ExportFunction(function)
                if !templates.contains_key(&function.name.name) =>
            {
                rewrite_block(&mut function.body, &templates, &mut requests, errors);
            }
            Declaration::Statement(statement) => {
                rewrite_statement(statement, &templates, &mut requests, errors);
            }
            _ => {}
        }
    }

    let mut generated = HashSet::new();
    let mut specialized = Vec::new();
    let mut generated_count = 0usize;
    loop {
        let next = requests
            .keys()
            .find(|key| !generated.contains(*key))
            .cloned();
        let Some(key) = next else {
            break;
        };
        let request = requests.get(&key).cloned().unwrap();
        let Some(template) = templates.get(&request.template).cloned() else {
            continue;
        };
        if request.arguments.len() != template.type_parameters.len() {
            errors.push(SemaError::new(
                SemaErrorKind::ArgumentCountMismatch,
                format!(
                    "`{}` espera {} argumento(s) de tipo, recebeu {}",
                    request.template,
                    template.type_parameters.len(),
                    request.arguments.len()
                ),
                template.name.span,
            ));
            generated.insert(key);
            continue;
        }
        let mut function = template.clone();
        function.name = Identifier {
            name: key.clone(),
            span: template.name.span,
        };
        function.type_parameters.clear();
        let substitutions = template
            .type_parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .zip(request.arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        substitute_function_types(&mut function, &substitutions);
        rewrite_block(&mut function.body, &templates, &mut requests, errors);
        generated_count += 1;
        if generated_count > 32 {
            errors.push(SemaError::new(
                SemaErrorKind::UnsupportedType,
                "especialização genérica excedeu o limite",
                lift_ast::Span::default(),
            ));
            break;
        }
        generated.insert(key);
        specialized.push(function);
    }

    ast.declarations
        .retain(|declaration| !matches!(declaration, Declaration::Function(function) if templates.contains_key(&function.name.name)));
    for function in specialized {
        ast.declarations.push(Declaration::Function(function));
    }
}

#[derive(Clone)]
struct Request {
    template: String,
    arguments: Vec<TypeExpression>,
}

fn signature_uses_container(function: &FunctionDeclaration) -> bool {
    function
        .parameters
        .iter()
        .any(|parameter| type_uses_container(&parameter.ty))
        || function
            .return_type
            .as_ref()
            .is_some_and(type_uses_container)
}

fn type_uses_container(ty: &TypeExpression) -> bool {
    match &ty.kind {
        TypeExpressionKind::Generic { .. } => true,
        TypeExpressionKind::Array(element) | TypeExpressionKind::Optional(element) => {
            type_uses_container(element)
        }
        TypeExpressionKind::Result(ok, error) => {
            type_uses_container(ok) || type_uses_container(error)
        }
        TypeExpressionKind::Function(parameters, result) => {
            parameters.iter().any(type_uses_container) || type_uses_container(result)
        }
        TypeExpressionKind::Named(_) => false,
    }
}

fn specialized_name(name: &str, arguments: &[TypeExpression]) -> String {
    format!(
        "{name}<{}>",
        arguments
            .iter()
            .map(canonical_type)
            .collect::<Vec<_>>()
            .join(", ")
    )
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

fn rewrite_block(
    block: &mut Block,
    templates: &HashMap<String, FunctionDeclaration>,
    requests: &mut HashMap<String, Request>,
    errors: &mut Vec<SemaError>,
) {
    for statement in &mut block.statements {
        rewrite_statement(statement, templates, requests, errors);
    }
}

fn rewrite_statement(
    statement: &mut Statement,
    templates: &HashMap<String, FunctionDeclaration>,
    requests: &mut HashMap<String, Request>,
    errors: &mut Vec<SemaError>,
) {
    match statement {
        Statement::Variable(variable) => rewrite_variable(variable, templates, requests, errors),
        Statement::Assignment(assignment) => {
            rewrite_expression(&mut assignment.value, templates, requests, errors);
        }
        Statement::If(statement) => {
            rewrite_expression(&mut statement.condition, templates, requests, errors);
            rewrite_block(&mut statement.then_branch, templates, requests, errors);
            if let Some(branch) = &mut statement.else_branch {
                rewrite_else_branch(branch, templates, requests, errors);
            }
        }
        Statement::While(statement) => {
            rewrite_expression(&mut statement.condition, templates, requests, errors);
            rewrite_block(&mut statement.body, templates, requests, errors);
        }
        Statement::ForOf(statement) => {
            rewrite_expression(&mut statement.iterable, templates, requests, errors);
            rewrite_block(&mut statement.body, templates, requests, errors);
        }
        Statement::Match(statement) => {
            rewrite_expression(&mut statement.value, templates, requests, errors);
            for arm in &mut statement.arms {
                rewrite_block(&mut arm.body, templates, requests, errors);
            }
        }
        Statement::TryCatch(statement) => {
            rewrite_block(&mut statement.body, templates, requests, errors);
            rewrite_block(&mut statement.catch_body, templates, requests, errors);
        }
        Statement::Return(statement) => {
            if let Some(value) = &mut statement.value {
                rewrite_expression(value, templates, requests, errors);
            }
        }
        Statement::Throw(statement) => {
            rewrite_expression(&mut statement.value, templates, requests, errors);
        }
        Statement::Expression(statement) => {
            rewrite_expression(&mut statement.expression, templates, requests, errors);
        }
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn rewrite_variable(
    variable: &mut VariableDeclaration,
    templates: &HashMap<String, FunctionDeclaration>,
    requests: &mut HashMap<String, Request>,
    errors: &mut Vec<SemaError>,
) {
    rewrite_expression(&mut variable.initializer, templates, requests, errors);
}

fn rewrite_else_branch(
    branch: &mut lift_ast::ElseBranch,
    templates: &HashMap<String, FunctionDeclaration>,
    requests: &mut HashMap<String, Request>,
    errors: &mut Vec<SemaError>,
) {
    match &mut branch.kind {
        ElseBranchKind::Block(block) => rewrite_block(block, templates, requests, errors),
        ElseBranchKind::ElseIf(statement) => {
            rewrite_expression(&mut statement.condition, templates, requests, errors);
            rewrite_block(&mut statement.then_branch, templates, requests, errors);
            if let Some(branch) = &mut statement.else_branch {
                rewrite_else_branch(branch, templates, requests, errors);
            }
        }
    }
}

fn rewrite_expression(
    expression: &mut Expression,
    templates: &HashMap<String, FunctionDeclaration>,
    requests: &mut HashMap<String, Request>,
    errors: &mut Vec<SemaError>,
) {
    match &mut expression.kind {
        ExpressionKind::Call(call) => {
            if let ExpressionKind::TypeApplication {
                callee, arguments, ..
            } = &mut call.callee.kind
            {
                if let ExpressionKind::Identifier(identifier) = &callee.kind {
                    if templates.contains_key(&identifier.name) {
                        let name = specialized_name(&identifier.name, arguments);
                        requests.entry(name.clone()).or_insert_with(|| Request {
                            template: identifier.name.clone(),
                            arguments: arguments.clone(),
                        });
                        let span = call.callee.span;
                        call.callee = Box::new(Expression {
                            kind: ExpressionKind::Identifier(Identifier {
                                name,
                                span: identifier.span,
                            }),
                            span,
                        });
                    }
                }
            }
            rewrite_expression(&mut call.callee, templates, requests, errors);
            for argument in &mut call.arguments {
                rewrite_expression(argument, templates, requests, errors);
            }
        }
        ExpressionKind::Closure(closure) => {
            rewrite_block(&mut closure.body, templates, requests, errors);
        }
        ExpressionKind::Template(parts) => {
            for part in parts {
                rewrite_expression(part, templates, requests, errors);
            }
        }
        ExpressionKind::TryPropagate(inner) | ExpressionKind::Grouping(inner) => {
            rewrite_expression(inner, templates, requests, errors)
        }
        ExpressionKind::ObjectLiteral(object) => {
            for field in &mut object.fields {
                rewrite_expression(&mut field.value, templates, requests, errors);
            }
        }
        ExpressionKind::ArrayLiteral(array) => {
            for element in &mut array.elements {
                rewrite_expression(element, templates, requests, errors);
            }
        }
        ExpressionKind::Unary(unary) => {
            rewrite_expression(&mut unary.operand, templates, requests, errors);
        }
        ExpressionKind::Binary(binary) => {
            rewrite_expression(&mut binary.left, templates, requests, errors);
            rewrite_expression(&mut binary.right, templates, requests, errors);
        }
        ExpressionKind::FieldAccess(access) => {
            rewrite_expression(&mut access.object, templates, requests, errors);
        }
        ExpressionKind::Index(index) => {
            rewrite_expression(&mut index.object, templates, requests, errors);
            rewrite_expression(&mut index.index, templates, requests, errors);
        }
        ExpressionKind::TypeApplication {
            callee, arguments, ..
        } => {
            for argument in arguments {
                rewrite_type(argument, templates, requests, errors);
            }
            rewrite_expression(callee, templates, requests, errors);
        }
        ExpressionKind::IntegerLiteral(_)
        | ExpressionKind::NumberLiteral(_)
        | ExpressionKind::StringLiteral(_)
        | ExpressionKind::BooleanLiteral(_)
        | ExpressionKind::NullLiteral
        | ExpressionKind::Identifier(_) => {}
    }
}

fn rewrite_type(
    ty: &mut TypeExpression,
    templates: &HashMap<String, FunctionDeclaration>,
    requests: &mut HashMap<String, Request>,
    errors: &mut Vec<SemaError>,
) {
    match &mut ty.kind {
        TypeExpressionKind::Array(element) | TypeExpressionKind::Optional(element) => {
            rewrite_type(element, templates, requests, errors)
        }
        TypeExpressionKind::Result(ok, error) => {
            rewrite_type(ok, templates, requests, errors);
            rewrite_type(error, templates, requests, errors);
        }
        TypeExpressionKind::Function(parameters, result) => {
            for parameter in parameters {
                rewrite_type(parameter, templates, requests, errors);
            }
            rewrite_type(result, templates, requests, errors);
        }
        TypeExpressionKind::Generic { arguments, .. } => {
            for argument in arguments {
                rewrite_type(argument, templates, requests, errors);
            }
        }
        TypeExpressionKind::Named(_) => {}
    }
}

fn substitute_function_types(
    function: &mut FunctionDeclaration,
    substitutions: &HashMap<String, TypeExpression>,
) {
    for parameter in &mut function.parameters {
        parameter.ty = substitute(&parameter.ty, substitutions);
    }
    if let Some(return_type) = &mut function.return_type {
        *return_type = substitute(return_type, substitutions);
    }
    substitute_block(&mut function.body, substitutions);
}

fn substitute_block(block: &mut Block, substitutions: &HashMap<String, TypeExpression>) {
    for statement in &mut block.statements {
        substitute_statement(statement, substitutions);
    }
}

fn substitute_statement(
    statement: &mut Statement,
    substitutions: &HashMap<String, TypeExpression>,
) {
    match statement {
        Statement::Variable(variable) => {
            if let Some(annotation) = &mut variable.annotation {
                *annotation = substitute(annotation, substitutions);
            }
            substitute_expression(&mut variable.initializer, substitutions);
        }
        Statement::Assignment(assignment) => {
            substitute_expression(&mut assignment.value, substitutions);
        }
        Statement::If(statement) => {
            substitute_expression(&mut statement.condition, substitutions);
            substitute_block(&mut statement.then_branch, substitutions);
            if let Some(branch) = &mut statement.else_branch {
                substitute_branch(branch, substitutions);
            }
        }
        Statement::While(statement) => {
            substitute_expression(&mut statement.condition, substitutions);
            substitute_block(&mut statement.body, substitutions);
        }
        Statement::ForOf(statement) => {
            substitute_expression(&mut statement.iterable, substitutions);
            substitute_block(&mut statement.body, substitutions);
        }
        Statement::Match(statement) => {
            substitute_expression(&mut statement.value, substitutions);
            for arm in &mut statement.arms {
                substitute_block(&mut arm.body, substitutions);
            }
        }
        Statement::TryCatch(statement) => {
            substitute_block(&mut statement.body, substitutions);
            substitute_block(&mut statement.catch_body, substitutions);
        }
        Statement::Return(statement) => {
            if let Some(value) = &mut statement.value {
                substitute_expression(value, substitutions);
            }
        }
        Statement::Throw(statement) => {
            substitute_expression(&mut statement.value, substitutions);
        }
        Statement::Expression(statement) => {
            substitute_expression(&mut statement.expression, substitutions);
        }
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn substitute_branch(
    branch: &mut lift_ast::ElseBranch,
    substitutions: &HashMap<String, TypeExpression>,
) {
    match &mut branch.kind {
        ElseBranchKind::Block(block) => substitute_block(block, substitutions),
        ElseBranchKind::ElseIf(statement) => {
            substitute_expression(&mut statement.condition, substitutions);
            substitute_block(&mut statement.then_branch, substitutions);
            if let Some(branch) = &mut statement.else_branch {
                substitute_branch(branch, substitutions);
            }
        }
    }
}

fn substitute_expression(
    expression: &mut Expression,
    substitutions: &HashMap<String, TypeExpression>,
) {
    match &mut expression.kind {
        ExpressionKind::Closure(closure) => {
            for parameter in &mut closure.parameters {
                parameter.ty = substitute(&parameter.ty, substitutions);
            }
            closure.return_type = substitute(&closure.return_type, substitutions);
            substitute_block(&mut closure.body, substitutions);
        }
        ExpressionKind::Template(parts) => {
            for part in parts {
                substitute_expression(part, substitutions);
            }
        }
        ExpressionKind::TryPropagate(inner) | ExpressionKind::Grouping(inner) => {
            substitute_expression(inner, substitutions)
        }
        ExpressionKind::ObjectLiteral(object) => {
            for field in &mut object.fields {
                substitute_expression(&mut field.value, substitutions);
            }
        }
        ExpressionKind::ArrayLiteral(array) => {
            for element in &mut array.elements {
                substitute_expression(element, substitutions);
            }
        }
        ExpressionKind::Unary(unary) => {
            substitute_expression(&mut unary.operand, substitutions);
        }
        ExpressionKind::Binary(binary) => {
            substitute_expression(&mut binary.left, substitutions);
            substitute_expression(&mut binary.right, substitutions);
        }
        ExpressionKind::FieldAccess(access) => {
            substitute_expression(&mut access.object, substitutions);
        }
        ExpressionKind::Call(call) => {
            substitute_expression(&mut call.callee, substitutions);
            for argument in &mut call.arguments {
                substitute_expression(argument, substitutions);
            }
        }
        ExpressionKind::TypeApplication {
            callee, arguments, ..
        } => {
            substitute_expression(callee, substitutions);
            for argument in arguments {
                *argument = substitute(argument, substitutions);
            }
        }
        ExpressionKind::Index(index) => {
            substitute_expression(&mut index.object, substitutions);
            substitute_expression(&mut index.index, substitutions);
        }
        ExpressionKind::IntegerLiteral(_)
        | ExpressionKind::NumberLiteral(_)
        | ExpressionKind::StringLiteral(_)
        | ExpressionKind::BooleanLiteral(_)
        | ExpressionKind::NullLiteral
        | ExpressionKind::Identifier(_) => {}
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
