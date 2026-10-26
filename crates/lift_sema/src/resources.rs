//! Affine ownership for the closed SQLite resource protocol.
use crate::*;
use std::collections::{HashMap, HashSet};

type State = HashMap<BindingId, (bool, bool)>; // available, owned

/// Dependencies follow all typed HIR positions, including uncalled function
/// bodies and empty DbValue[] locals that require no explicit DB import.
pub(crate) fn uses_db_runtime(program: &HirProgram) -> bool {
    program
        .declarations
        .iter()
        .any(|declaration| match declaration {
            HirDeclaration::Struct(s) => s.fields.iter().any(|f| f.ty.needs_db_runtime()),
            HirDeclaration::Function(f) => {
                f.return_type.needs_db_runtime()
                    || f.parameters.iter().any(|p| p.ty.needs_db_runtime())
                    || f.body.statements.iter().any(statement_uses_db)
            }
            HirDeclaration::Import(i) => {
                i.return_type.needs_db_runtime()
                    || i.parameters.iter().any(ResolvedType::needs_db_runtime)
            }
            HirDeclaration::Statement(s) => statement_uses_db(s),
        })
}

fn statement_uses_db(statement: &HirStatement) -> bool {
    match statement {
        HirStatement::Variable(v) => v.ty.needs_db_runtime() || expression_uses_db(&v.initializer),
        HirStatement::Assignment(a) => {
            expression_uses_db(&a.target) || expression_uses_db(&a.value)
        }
        HirStatement::Expression(e) => expression_uses_db(&e.expression),
        HirStatement::Return(r) => r.value.as_ref().is_some_and(expression_uses_db),
        HirStatement::If(i) => if_uses_db(i),
        HirStatement::While(w) => {
            expression_uses_db(&w.condition) || w.body.statements.iter().any(statement_uses_db)
        }
    }
}

fn if_uses_db(conditional: &HirIf) -> bool {
    expression_uses_db(&conditional.condition)
        || conditional
            .then_branch
            .statements
            .iter()
            .any(statement_uses_db)
        || conditional
            .else_branch
            .as_ref()
            .is_some_and(|branch| match branch {
                HirElseBranch::Block(b) => b.statements.iter().any(statement_uses_db),
                HirElseBranch::ElseIf(i) => if_uses_db(i),
            })
}

fn expression_uses_db(expression: &HirExpr) -> bool {
    if expression.ty.needs_db_runtime() {
        return true;
    }
    match &expression.kind {
        HirExprKind::Closure {
            captures,
            parameters,
            body,
        } => {
            captures.iter().any(|(_, e)| expression_uses_db(e))
                || parameters.iter().any(|p| p.ty.needs_db_runtime())
                || body.statements.iter().any(statement_uses_db)
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            expression_uses_db(callee) || arguments.iter().any(expression_uses_db)
        }
        HirExprKind::Call { arguments, .. } => {
            arguments.iter().any(|a| expression_uses_db(&a.expression))
        }
        HirExprKind::StructLiteral { fields, .. } => {
            fields.iter().any(|f| expression_uses_db(&f.value))
        }
        HirExprKind::ArrayLiteral(items) => items.iter().any(expression_uses_db),
        HirExprKind::Grouping(inner)
        | HirExprKind::Move(inner)
        | HirExprKind::OptionSome(inner)
        | HirExprKind::Clone {
            expression: inner, ..
        }
        | HirExprKind::Unary { operand: inner, .. }
        | HirExprKind::FieldAccess { object: inner, .. } => expression_uses_db(inner),
        HirExprKind::Binary { left, right, .. }
        | HirExprKind::Index {
            object: left,
            index: right,
        }
        | HirExprKind::Coalesce {
            option: left,
            fallback: right,
            ..
        } => expression_uses_db(left) || expression_uses_db(right),
        HirExprKind::IntegerLiteral(_)
        | HirExprKind::NumberLiteral(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::BooleanLiteral(_)
        | HirExprKind::OptionNone
        | HirExprKind::Local { .. } => false,
    }
}

pub(crate) fn uses_http(program: &HirProgram) -> bool {
    // HTTP operations are restricted to the top-level execution block.
    program.declarations.iter().any(|d| match d {
        HirDeclaration::Statement(s) => statement_http(s),
        _ => false,
    })
}
fn statement_http(s: &HirStatement) -> bool {
    match s {
        HirStatement::Expression(e) => expression_http(&e.expression),
        HirStatement::Variable(v) => expression_http(&v.initializer),
        HirStatement::If(i) => {
            expression_http(&i.condition) || i.then_branch.statements.iter().any(statement_http)
        }
        _ => false,
    }
}
fn expression_http(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Call { function, .. } if function.starts_with("$http."))
}

pub(crate) fn validate(program: &HirProgram) -> Result<(), Vec<SemaError>> {
    let mut errors = Vec::new();
    let mut top = State::new();
    let mut routes = HashSet::new();
    let mut serving = false;
    for d in &program.declarations {
        match d {
            HirDeclaration::Function(f) => {
                let mut state = State::new();
                for p in &f.parameters {
                    if p.ty.contains_resource() {
                        state.insert(
                            p.id,
                            (
                                true,
                                matches!(
                                    p.pass_mode,
                                    ParameterPassMode::OwnedMove | ParameterPassMode::ByValue
                                ),
                            ),
                        );
                    }
                }
                for s in &f.body.statements {
                    statement(s, &mut state, &mut errors, false);
                }
            }
            HirDeclaration::Statement(s) => {
                if let HirStatement::Expression(e) = s {
                    if let HirExprKind::Call {
                        function,
                        arguments,
                    } = &e.expression.kind
                    {
                        if function.starts_with("$http.route|") {
                            let key = format!(
                                "{:?}|{:?}",
                                arguments[0].expression.kind, arguments[1].expression.kind
                            );
                            if serving || !routes.insert(key) {
                                error(
                                    &mut errors,
                                    e.span,
                                    "rota duplicada ou registrada depois de http.serve",
                                );
                            }
                        }
                        if function == "$http.serve" {
                            if serving {
                                error(&mut errors, e.span, "somente um http.serve por programa");
                            }
                            serving = true;
                        }
                    }
                }
                if statement_http(s)
                    && !matches!(s, HirStatement::Expression(_) | HirStatement::Variable(_))
                {
                    error(
                        &mut errors,
                        s.span(),
                        "HTTP exige statements diretos de topo",
                    );
                }
                if let HirStatement::Variable(v) = s {
                    if matches!(&v.initializer.kind, HirExprKind::Call {function,..} if function == "$http.serve")
                    {
                        if serving {
                            error(&mut errors, v.span, "somente um http.serve por programa");
                        }
                        serving = true;
                    }
                }
                statement(s, &mut top, &mut errors, true);
            }
            _ => {}
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
fn error(errors: &mut Vec<SemaError>, span: lift_ast::Span, message: &str) {
    errors.push(SemaError::new(
        SemaErrorKind::UnsupportedOwnershipPattern,
        message,
        span,
    ));
}
fn statement(s: &HirStatement, state: &mut State, errors: &mut Vec<SemaError>, direct: bool) {
    match s {
        HirStatement::Variable(v) => {
            expr(&v.initializer, state, errors, false, direct);
            if v.ty.contains_resource() {
                state.insert(v.id, (true, true));
            }
        }
        HirStatement::Assignment(a) => {
            expr(&a.value, state, errors, false, false);
            if a.target.ty.contains_resource() {
                error(
                    errors,
                    a.span,
                    "recursos opacos exigem bindings novos; reatribuição ainda não é suportada",
                );
            } else {
                expr(&a.target, state, errors, false, false);
            }
        }
        HirStatement::Expression(e) => {
            expr(&e.expression, state, errors, false, direct);
        }
        HirStatement::Return(r) => {
            if let Some(e) = &r.value {
                expr(e, state, errors, e.ty.contains_resource(), false);
            }
        }
        HirStatement::If(i) => conditional(i, state, errors),
        HirStatement::While(w) => {
            let before = state.clone();
            expr(&w.condition, state, errors, false, false);
            let mut body = state.clone();
            for s in &w.body.statements {
                statement(s, &mut body, errors, false);
            }
            if before.iter().any(|(id, value)| body.get(id) != Some(value)) {
                error(
                    errors,
                    w.span,
                    "loop não pode consumir recurso externo; mova a operação para fora do loop",
                );
            }
        }
    }
}
fn conditional(i: &HirIf, state: &mut State, errors: &mut Vec<SemaError>) {
    expr(&i.condition, state, errors, false, false);
    let mut left = state.clone();
    let mut right = state.clone();
    for s in &i.then_branch.statements {
        statement(s, &mut left, errors, false);
    }
    if let Some(branch) = &i.else_branch {
        match branch {
            HirElseBranch::Block(b) => {
                for s in &b.statements {
                    statement(s, &mut right, errors, false);
                }
            }
            HirElseBranch::ElseIf(i) => conditional(i, &mut right, errors),
        }
    }
    for (id, available) in state.iter_mut() {
        available.0 = left.get(id).is_some_and(|s| s.0) && right.get(id).is_some_and(|s| s.0);
    }
}
fn expr(e: &HirExpr, state: &mut State, errors: &mut Vec<SemaError>, consume: bool, direct: bool) {
    match &e.kind {
        HirExprKind::Local { id, .. } if e.ty.contains_resource() => {
            let Some((available, owned)) = state.get_mut(id) else {
                return;
            };
            if !*available {
                error(
                    errors,
                    e.span,
                    "recurso já movido ou descartado em um caminho anterior",
                );
            }
            if consume {
                if !*owned {
                    error(
                        errors,
                        e.span,
                        "recurso emprestado não pode ser movido ou descartado",
                    );
                }
                *available = false;
            }
        }
        HirExprKind::Move(inner) => expr(inner, state, errors, true, false),
        HirExprKind::Clone { expression, .. } => {
            if expression.ty.contains_resource() {
                error(errors, e.span, "recursos opacos não implementam Clone");
            }
            expr(expression, state, errors, false, false);
        }
        HirExprKind::Call {
            function,
            arguments,
        } => {
            let mut borrowed = HashSet::new();
            let mut moved = HashSet::new();
            accesses(e, false, &mut borrowed, &mut moved);
            if !borrowed.is_disjoint(&moved) {
                error(errors, e.span, "mesma chamada não pode emprestar e consumir o mesmo recurso, inclusive em argumentos aninhados");
            }
            if function.starts_with("$http.") && !direct {
                error(
                    errors,
                    e.span,
                    "HTTP exige registro/execução em statement direto de topo",
                );
            }
            for a in arguments {
                if a.expression.ty.contains_resource() && a.pass_mode == ArgPassMode::ByClone {
                    error(errors, a.span, "recursos opacos não podem ser clonados");
                }
                expr(
                    &a.expression,
                    state,
                    errors,
                    a.pass_mode == ArgPassMode::ByValue && a.expression.ty.contains_resource(),
                    false,
                );
            }
        }
        HirExprKind::Grouping(inner) => expr(inner, state, errors, consume, false),
        HirExprKind::Unary { operand, .. } => expr(operand, state, errors, false, false),
        HirExprKind::Binary { left, right, .. } => {
            if left.ty.contains_resource() || right.ty.contains_resource() {
                error(
                    errors,
                    e.span,
                    "recursos opacos não suportam operadores ou igualdade",
                );
            }
            expr(left, state, errors, false, false);
            expr(right, state, errors, false, false);
        }
        HirExprKind::ArrayLiteral(items) => {
            if e.ty.contains_resource() {
                error(errors, e.span, "arrays não podem conter recursos opacos");
            }
            for i in items {
                expr(i, state, errors, false, false);
            }
        }
        HirExprKind::StructLiteral { fields, .. } => {
            for f in fields {
                expr(&f.value, state, errors, false, false);
            }
        }
        HirExprKind::FieldAccess { object, .. } => expr(object, state, errors, false, false),
        HirExprKind::Index { object, index } => {
            expr(object, state, errors, false, false);
            expr(index, state, errors, false, false);
        }
        HirExprKind::OptionSome(inner) => {
            if inner.ty.contains_resource() {
                error(errors, e.span, "opcionais não podem conter recursos opacos");
            }
            expr(inner, state, errors, false, false);
        }
        HirExprKind::Coalesce {
            option, fallback, ..
        } => {
            expr(option, state, errors, false, false);
            expr(fallback, state, errors, false, false);
        }
        HirExprKind::Closure {
            captures,
            parameters,
            body,
        } => {
            for (_, capture) in captures {
                if capture.ty.contains_resource() {
                    error(
                        errors,
                        e.span,
                        "closures não podem capturar recursos opacos",
                    );
                }
            }
            if parameters.iter().any(|p| p.ty.contains_resource())
                || matches!(&e.ty, ResolvedType::Closure(_, result) if result.contains_resource())
            {
                error(
                    errors,
                    e.span,
                    "assinaturas de closure não podem conter recursos opacos",
                );
            }
            let mut local = State::new();
            for s in &body.statements {
                statement(s, &mut local, errors, false);
            }
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            expr(callee, state, errors, false, false);
            for a in arguments {
                expr(a, state, errors, false, false);
            }
        }
        _ => {}
    }
}

// Conservative expression-wide overlap, including nested argument evaluation.
fn accesses(
    e: &HirExpr,
    consuming: bool,
    borrowed: &mut HashSet<BindingId>,
    moved: &mut HashSet<BindingId>,
) {
    match &e.kind {
        HirExprKind::Local { id, .. } if e.ty.contains_resource() => {
            if consuming {
                moved.insert(*id);
            } else {
                borrowed.insert(*id);
            }
        }
        HirExprKind::Move(inner) => accesses(inner, true, borrowed, moved),
        HirExprKind::Grouping(inner) => accesses(inner, consuming, borrowed, moved),
        HirExprKind::Clone { expression, .. } => accesses(expression, false, borrowed, moved),
        HirExprKind::Call { arguments, .. } => {
            for a in arguments {
                accesses(
                    &a.expression,
                    a.pass_mode == ArgPassMode::ByValue && a.expression.ty.contains_resource(),
                    borrowed,
                    moved,
                );
            }
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            // Closure callees are borrowed; arguments are owned values in HIR.
            // Walk their evaluation too: a scalar argument can hide a nested
            // call that consumes a resource borrowed by a sibling argument.
            accesses(callee, false, borrowed, moved);
            for argument in arguments {
                accesses(argument, argument.ty.contains_resource(), borrowed, moved);
            }
        }
        HirExprKind::Unary { operand, .. } | HirExprKind::OptionSome(operand) => {
            accesses(operand, false, borrowed, moved)
        }
        HirExprKind::Binary { left, right, .. }
        | HirExprKind::Index {
            object: left,
            index: right,
        }
        | HirExprKind::Coalesce {
            option: left,
            fallback: right,
            ..
        } => {
            accesses(left, false, borrowed, moved);
            accesses(right, false, borrowed, moved);
        }
        HirExprKind::FieldAccess { object, .. } => accesses(object, false, borrowed, moved),
        HirExprKind::StructLiteral { fields, .. } => {
            for f in fields {
                accesses(&f.value, false, borrowed, moved);
            }
        }
        HirExprKind::ArrayLiteral(items) => {
            for item in items {
                accesses(item, false, borrowed, moved);
            }
        }
        _ => {}
    }
}
