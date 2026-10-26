//! Advisory HIR rules. They never rewrite the program or weaken semantic errors.
//! PT-BR: Regras consultivas sobre HIR; nunca reescrevem nem relaxam erros sema.

use crate::Diagnostic;
use std::collections::HashSet;
use truxt_ast::Span;
use truxt_sema::*;

/// Lint validated HIR only. Warnings are independent of Cargo/rustc checks.
/// PT-BR: Lint somente de HIR validado, independente de Cargo/rustc.
pub fn lint(program: &HirProgram) -> Vec<Diagnostic> {
    let mut visitor = Visitor::default();
    for declaration in &program.declarations {
        match declaration {
            HirDeclaration::Function(function) => {
                visitor.parameters(&function.parameters);
                visitor.block(&function.body);
            }
            HirDeclaration::Statement(statement) => visitor.statement(statement),
            _ => {}
        }
    }
    for (id, name, span) in visitor.bindings {
        if !name.starts_with('_') && name != "this" && !visitor.references.contains(&id) {
            visitor.warnings.push(Diagnostic::warning(
                "unused-binding",
                format!("binding `{name}` nunca é referenciado; use prefixo `_` se intencional"),
                span,
            ));
        }
    }
    visitor
        .warnings
        .sort_by_key(|d| d.span.map(|s| (s.source.0, s.start, s.end)));
    visitor.warnings
}

#[derive(Default)]
struct Visitor {
    bindings: Vec<(BindingId, String, Span)>,
    references: HashSet<BindingId>,
    warnings: Vec<Diagnostic>,
}

impl Visitor {
    fn parameters(&mut self, parameters: &[HirParameter]) {
        self.bindings
            .extend(parameters.iter().map(|p| (p.id, p.name.clone(), p.span)));
    }

    fn block(&mut self, block: &HirBlock) {
        for statement in &block.statements {
            self.statement(statement);
        }
    }

    fn condition(&mut self, expr: &HirExpr) {
        let mut inner = expr;
        while let HirExprKind::Grouping(value) = &inner.kind {
            inner = value;
        }
        if matches!(inner.kind, HirExprKind::BooleanLiteral(_)) {
            self.warnings.push(Diagnostic::warning(
                "literal-condition",
                "condição é um boolean literal; confirme se é intencional",
                expr.span,
            ));
        }
        self.expression(expr);
    }

    fn branch(&mut self, branch: &HirIf) {
        self.condition(&branch.condition);
        self.block(&branch.then_branch);
        if let Some(alternative) = &branch.else_branch {
            match alternative {
                HirElseBranch::Block(block) => self.block(block),
                HirElseBranch::ElseIf(branch) => self.branch(branch),
            }
        }
    }

    fn statement(&mut self, statement: &HirStatement) {
        match statement {
            HirStatement::Variable(variable) => {
                self.bindings
                    .push((variable.id, variable.name.clone(), variable.span));
                self.expression(&variable.initializer);
            }
            HirStatement::Assignment(assignment) => {
                // A write counts as a reference: no speculative dead-store analysis.
                // PT-BR: Escrita conta como referência; não inferimos dead stores.
                self.expression(&assignment.target);
                self.expression(&assignment.value);
            }
            HirStatement::If(branch) => self.branch(branch),
            HirStatement::While(loop_) => {
                self.condition(&loop_.condition);
                self.block(&loop_.body);
            }
            HirStatement::Return(ret) => {
                if let Some(expr) = &ret.value {
                    self.expression(expr);
                }
            }
            HirStatement::Expression(statement) => {
                let mut inner = &statement.expression;
                while let HirExprKind::Grouping(expr) = &inner.kind {
                    inner = expr;
                }
                if matches!(
                    inner.kind,
                    HirExprKind::IntegerLiteral(_)
                        | HirExprKind::NumberLiteral(_)
                        | HirExprKind::StringLiteral(_)
                        | HirExprKind::BooleanLiteral(_)
                        | HirExprKind::Local { .. }
                        | HirExprKind::OptionNone
                ) {
                    self.warnings.push(Diagnostic::warning(
                        "discarded-value",
                        "valor sem efeito é descartado",
                        statement.span,
                    ));
                }
                self.expression(&statement.expression);
            }
        }
    }

    fn expression(&mut self, expr: &HirExpr) {
        match &expr.kind {
            HirExprKind::Local { id, .. } => {
                self.references.insert(*id);
            }
            HirExprKind::Closure {
                captures,
                parameters,
                body,
            } => {
                for (_, initializer) in captures {
                    self.expression(initializer);
                }
                self.parameters(parameters);
                self.block(body);
            }
            HirExprKind::ClosureCall { callee, arguments } => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            HirExprKind::OptionSome(value)
            | HirExprKind::Grouping(value)
            | HirExprKind::Move(value)
            | HirExprKind::Clone {
                expression: value, ..
            }
            | HirExprKind::Unary { operand: value, .. } => self.expression(value),
            HirExprKind::Coalesce {
                option, fallback, ..
            } => {
                self.expression(option);
                self.expression(fallback);
            }
            HirExprKind::StructLiteral { fields, .. } => {
                for field in fields {
                    self.expression(&field.value);
                }
            }
            HirExprKind::ArrayLiteral(elements) => {
                for element in elements {
                    self.expression(element);
                }
            }
            HirExprKind::Binary { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            HirExprKind::FieldAccess { object, .. } => self.expression(object),
            HirExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    self.expression(&argument.expression);
                }
            }
            HirExprKind::Index { object, index } => {
                self.expression(object);
                self.expression(index);
            }
            HirExprKind::IntegerLiteral(_)
            | HirExprKind::NumberLiteral(_)
            | HirExprKind::StringLiteral(_)
            | HirExprKind::BooleanLiteral(_)
            | HirExprKind::OptionNone => {}
        }
    }
}
