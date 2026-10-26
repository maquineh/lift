//! Recursive-descent declarations, Pratt expressions, and statement recovery.
//!
//! PT-BR: Declarações por recursive descent, expressões Pratt e recuperação de statements.

use truxt_ast::{
    ArrayLiteral, AssignmentStatement, BinaryExpression, BinaryOperator, BindingKind, Block,
    CallExpression, Declaration, ElseBranch, ElseBranchKind, Expression, ExpressionKind,
    ExpressionStatement, FieldAccessExpression, FunctionDeclaration, Identifier, IfStatement,
    ImportFunctionDeclaration, IndexExpression, InterfaceDeclaration, InterfaceField,
    ModuleImportDeclaration, ObjectField, ObjectLiteral, Parameter, ProgramAst, ReturnStatement,
    SourceId, Span, Statement, TypeExpression, TypeExpressionKind, UnaryExpression, UnaryOperator,
    VariableDeclaration, WhileStatement,
};
use truxt_lexer::Token;

use crate::{ParseError, ParseErrorKind};

const PREFIX_BINDING_POWER: u8 = 14;
const POSTFIX_BINDING_POWER: u8 = 15;

pub(super) struct Parser<'tokens> {
    tokens: &'tokens [(Token, Span)],
    cursor: usize,
    delimiters: Vec<Delimiter>,
    errors: Vec<ParseError>,
    source: SourceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delimiter {
    Parenthesis,
    Bracket,
    Brace,
}

impl<'tokens> Parser<'tokens> {
    pub(super) fn new(tokens: &'tokens [(Token, Span)]) -> Self {
        Self::new_with_source(
            tokens,
            tokens.first().map_or(SourceId(0), |(_, span)| span.source),
        )
    }

    pub(super) fn new_with_source(tokens: &'tokens [(Token, Span)], source: SourceId) -> Self {
        Self {
            tokens,
            cursor: 0,
            delimiters: Vec::new(),
            errors: Vec::new(),
            source,
        }
    }

    pub(super) fn parse_program(mut self) -> Result<ProgramAst, Vec<ParseError>> {
        let mut declarations = Vec::new();

        while !self.is_at_end() {
            let start_cursor = self.cursor;
            match self.parse_declaration() {
                Ok(declaration) => declarations.push(declaration),
                Err(error) => {
                    self.errors.push(error);
                    self.synchronize(None);
                }
            }

            if self.cursor == start_cursor && !self.is_at_end() {
                self.advance();
            }
        }

        let span = match (self.tokens.first(), self.tokens.last()) {
            (Some((_, first)), Some((_, last))) => self.span(first.start, last.end),
            _ => self.span(0, 0),
        };
        let program = ProgramAst::new(declarations, span);

        if self.errors.is_empty() {
            Ok(program)
        } else {
            Err(self.errors)
        }
    }

    fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        match self.current() {
            Some(Token::Impl) => {
                let start = self.current_span().start;
                self.advance();
                let target = self.parse_identifier()?;
                self.expect(is_left_brace, "`{`")?;
                let mut methods = Vec::new();
                while !self.check(is_right_brace) && !self.is_at_end() {
                    methods.push(self.parse_function_declaration()?);
                }
                let end = self.expect(is_right_brace, "`}`")?.end;
                Ok(Declaration::Impl(truxt_ast::ImplDeclaration {
                    target,
                    methods,
                    span: self.span(start, end),
                }))
            }
            Some(Token::Import) if self.check_n(1, is_module) => {
                self.parse_module_import().map(Declaration::ModuleImport)
            }
            Some(Token::Import) => self.parse_import().map(Declaration::Import),
            Some(Token::Interface) => self
                .parse_interface_declaration()
                .map(Declaration::Interface),
            Some(Token::Function) => self.parse_function_declaration().map(Declaration::Function),
            _ => self.parse_statement().map(Declaration::Statement),
        }
    }

    fn parse_module_import(&mut self) -> Result<ModuleImportDeclaration, ParseError> {
        let start = self
            .expect(|token| matches!(token, Token::Import), "`import`")?
            .start;
        self.expect(is_module, "`module`")?;
        let path_span = self.current_span();
        let path = match self.current() {
            Some(Token::StringLiteral(path)) => path.clone(),
            _ => return Err(self.error("caminho do módulo entre aspas")),
        };
        self.advance();
        let end = self.expect(is_semicolon, "`;`")?.end;
        Ok(ModuleImportDeclaration {
            path,
            path_span,
            span: self.span(start, end),
        })
    }

    fn parse_interface_declaration(&mut self) -> Result<InterfaceDeclaration, ParseError> {
        let start = self.expect(is_interface, "`interface`")?.start;
        let name = self.parse_identifier()?;
        self.expect(is_left_brace, "`{`")?;
        let mut fields = Vec::new();

        while !self.check(is_right_brace) && !self.is_at_end() {
            let field_start = self.current_span().start;
            let field_name = self.parse_identifier()?;
            self.expect(is_colon, "`:`")?;
            let ty = self.parse_type()?;
            let field_span = self.span(field_start, ty.span.end);
            fields.push(InterfaceField {
                name: field_name,
                ty,
                span: field_span,
            });

            if self.consume(is_comma).is_some() {
                continue;
            }
            // A comma is optional between interface fields in the supplied EBNF.
            if self.check(is_identifier) {
                continue;
            }
            if !self.check(is_right_brace) {
                return Err(self.error("`,` ou `}`"));
            }
        }

        let close = self.expect(is_right_brace, "`}`")?;
        Ok(InterfaceDeclaration {
            name,
            fields,
            span: self.span(start, close.end),
        })
    }

    fn parse_import(&mut self) -> Result<ImportFunctionDeclaration, ParseError> {
        let start = self
            .expect(|token| matches!(token, Token::Import), "`import`")?
            .start;
        self.expect(is_function, "`function`")?;
        let name = self.parse_identifier()?;
        self.expect(is_left_paren, "`(`")?;
        let mut parameters = Vec::new();
        if !self.check(is_right_paren) {
            loop {
                parameters.push(self.parse_parameter()?);
                if self.consume(is_comma).is_none() || self.check(is_right_paren) {
                    break;
                }
            }
        }
        self.expect(is_right_paren, "`)`")?;
        self.expect(is_colon, "tipo de retorno explícito após `:`")?;
        let return_type = self.parse_type()?;
        self.expect(|token| matches!(token, Token::From), "`from`")?;
        let source_span = self.current_span();
        let source = match self.current() {
            Some(Token::StringLiteral(source)) => source.clone(),
            _ => return Err(self.error("destino Rust entre aspas")),
        };
        self.advance();
        let end = self.expect(is_semicolon, "`;`")?.end;
        Ok(ImportFunctionDeclaration {
            name,
            parameters,
            return_type,
            source,
            source_span,
            span: self.span(start, end),
        })
    }

    fn parse_function_declaration(&mut self) -> Result<FunctionDeclaration, ParseError> {
        let start = self.current_span().start;
        self.consume(is_function);
        let name = self.parse_identifier()?;
        self.expect(is_left_paren, "`(`")?;
        let mut parameters = Vec::new();

        if !self.check(is_right_paren) {
            loop {
                parameters.push(self.parse_parameter()?);
                if self.consume(is_comma).is_none() {
                    break;
                }
                if self.check(is_right_paren) {
                    break;
                }
            }
        }

        self.expect(is_right_paren, "`)`")?;
        let return_type = if self.consume(is_colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;

        Ok(FunctionDeclaration {
            name,
            parameters,
            return_type,
            span: self.span(start, body.span.end),
            body,
        })
    }

    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let start = self.current_span().start;
        let name = self.parse_identifier()?;
        self.expect(is_colon, "`:`")?;
        let ty = self.parse_type()?;
        Ok(Parameter {
            name,
            span: self.span(start, ty.span.end),
            ty,
        })
    }

    fn parse_type(&mut self) -> Result<TypeExpression, ParseError> {
        let mut ty = self.parse_named_type()?;

        loop {
            if self.check(is_left_bracket) && self.check_n(1, is_right_bracket) {
                self.advance();
                let close = self.expect(is_right_bracket, "`]`")?;
                ty = TypeExpression {
                    span: self.span(ty.span.start, close.end),
                    kind: TypeExpressionKind::Array(Box::new(ty)),
                };
            } else if let Some(question) = self.consume(|token| matches!(token, Token::Question)) {
                ty = TypeExpression {
                    span: self.span(ty.span.start, question.end),
                    kind: TypeExpressionKind::Optional(Box::new(ty)),
                };
            } else {
                break;
            }
        }

        Ok(ty)
    }

    fn parse_named_type(&mut self) -> Result<TypeExpression, ParseError> {
        let (name, span) = match self.current() {
            Some(Token::Identifier(name)) => (name.clone(), self.current_span()),
            Some(Token::IntType) => ("int".to_owned(), self.current_span()),
            Some(Token::NumberType) => ("number".to_owned(), self.current_span()),
            Some(Token::StringType) => ("string".to_owned(), self.current_span()),
            Some(Token::BooleanType) => ("boolean".to_owned(), self.current_span()),
            Some(Token::VoidType) => ("void".to_owned(), self.current_span()),
            _ => return Err(self.error("tipo")),
        };
        self.advance();

        if name == "Result" {
            self.expect(|token| matches!(token, Token::Less), "`<`")?;
            let ok = self.parse_type()?;
            self.expect(is_comma, "`,`")?;
            let error = self.parse_type()?;
            let close = self.expect(|token| matches!(token, Token::Greater), "`>`")?;
            return Ok(TypeExpression {
                kind: TypeExpressionKind::Result(Box::new(ok), Box::new(error)),
                span: self.span(span.start, close.end),
            });
        }

        let identifier = Identifier { name, span };
        Ok(TypeExpression {
            kind: TypeExpressionKind::Named(identifier),
            span,
        })
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.expect(is_left_brace, "`{`")?.start;
        let mut statements = Vec::new();

        while !self.check(is_right_brace) && !self.is_at_end() {
            let start_cursor = self.cursor;
            let statement_depth = self.delimiters.len();
            match self.parse_statement() {
                Ok(statement) => statements.push(statement),
                Err(error) => {
                    self.errors.push(error);
                    self.synchronize(Some(statement_depth));
                }
            }

            if self.cursor == start_cursor && !self.is_at_end() && !self.check(is_right_brace) {
                self.advance();
            }
        }

        let close = self.expect(is_right_brace, "`}`")?;
        Ok(Block {
            statements,
            span: self.span(start, close.end),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.current() {
            Some(Token::Const) => self.parse_variable_declaration(BindingKind::Const),
            Some(Token::Let) => self.parse_variable_declaration(BindingKind::Let),
            Some(Token::If) => self.parse_if_statement().map(Statement::If),
            Some(Token::While) => self.parse_while_statement().map(Statement::While),
            Some(Token::Return) => self.parse_return_statement().map(Statement::Return),
            _ => self.parse_assignment_or_expression_statement(),
        }
    }

    fn parse_variable_declaration(
        &mut self,
        binding: BindingKind,
    ) -> Result<Statement, ParseError> {
        let start = self.current_span().start;
        match binding {
            BindingKind::Const => {
                self.expect(is_const, "`const`")?;
            }
            BindingKind::Let => {
                self.expect(is_let, "`let`")?;
            }
        }

        let name = self.parse_identifier()?;
        let annotation = if self.consume(is_colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(is_equal, "`=`")?;
        let initializer = self.parse_expression(0)?;
        let end = self
            .consume(is_semicolon)
            .map_or(initializer.span.end, |span| span.end);

        Ok(Statement::Variable(VariableDeclaration {
            binding,
            name,
            annotation,
            initializer,
            span: self.span(start, end),
        }))
    }

    fn parse_if_statement(&mut self) -> Result<IfStatement, ParseError> {
        let start = self.expect(is_if, "`if`")?.start;
        let condition = self.parse_expression(0)?;
        let then_branch = self.parse_block()?;
        let else_branch = if let Some(else_span) = self.consume(is_else) {
            if self.check(is_if) {
                let nested = self.parse_if_statement()?;
                Some(ElseBranch {
                    span: self.span(else_span.start, nested.span.end),
                    kind: ElseBranchKind::ElseIf(Box::new(nested)),
                })
            } else {
                let block = self.parse_block()?;
                Some(ElseBranch {
                    span: self.span(else_span.start, block.span.end),
                    kind: ElseBranchKind::Block(block),
                })
            }
        } else {
            None
        };
        let end = else_branch
            .as_ref()
            .map_or(then_branch.span.end, |branch| branch.span.end);

        Ok(IfStatement {
            condition,
            then_branch,
            else_branch,
            span: self.span(start, end),
        })
    }

    fn parse_while_statement(&mut self) -> Result<WhileStatement, ParseError> {
        let start = self.expect(is_while, "`while`")?.start;
        let condition = self.parse_expression(0)?;
        let body = self.parse_block()?;
        Ok(WhileStatement {
            condition,
            span: self.span(start, body.span.end),
            body,
        })
    }

    fn parse_return_statement(&mut self) -> Result<ReturnStatement, ParseError> {
        let start_span = self.expect(is_return, "`return`")?;
        let value = if self.current().is_some_and(can_start_expression) {
            Some(self.parse_expression(0)?)
        } else {
            None
        };
        let end = self
            .consume(is_semicolon)
            .map(|span| span.end)
            .or_else(|| value.as_ref().map(|expression| expression.span.end))
            .unwrap_or(start_span.end);

        Ok(ReturnStatement {
            value,
            span: self.span(start_span.start, end),
        })
    }

    fn parse_assignment_or_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let left = self.parse_expression(0)?;
        if self.consume(is_equal).is_some() {
            let value = self.parse_expression(0)?;
            let end = self
                .consume(is_semicolon)
                .map_or(value.span.end, |span| span.end);
            return Ok(Statement::Assignment(AssignmentStatement {
                span: self.span(left.span.start, end),
                target: left,
                value,
            }));
        }

        let end = self
            .consume(is_semicolon)
            .map_or(left.span.end, |span| span.end);
        Ok(Statement::Expression(ExpressionStatement {
            span: self.span(left.span.start, end),
            expression: left,
        }))
    }

    fn parse_expression(&mut self, minimum_binding_power: u8) -> Result<Expression, ParseError> {
        let mut left = self.parse_prefix()?;

        loop {
            if self
                .postfix_binding_power()
                .is_some_and(|power| power >= minimum_binding_power)
            {
                left = self.parse_postfix(left)?;
                continue;
            }

            let Some((operator, left_power, right_power)) = self.infix_binding_power() else {
                break;
            };
            if left_power < minimum_binding_power {
                break;
            }

            let operator_span = self.current_span();
            self.advance();
            let right = self.parse_expression(right_power)?;
            if (operator == BinaryOperator::Coalesce
                && (has_ungrouped_operator(&left, &[BinaryOperator::And, BinaryOperator::Or])
                    || has_ungrouped_operator(&right, &[BinaryOperator::And, BinaryOperator::Or])))
                || (matches!(operator, BinaryOperator::And | BinaryOperator::Or)
                    && (has_ungrouped_operator(&left, &[BinaryOperator::Coalesce])
                        || has_ungrouped_operator(&right, &[BinaryOperator::Coalesce])))
            {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken,
                    "parênteses ao misturar `??` com `&&`/`||`",
                    operator_span,
                ));
            }
            let span = self.span(left.span.start, right.span.end);
            let binary = BinaryExpression {
                left: Box::new(left),
                operator,
                right: Box::new(right),
                span,
            };
            left = Expression {
                kind: ExpressionKind::Binary(binary),
                span,
            };
        }

        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expression, ParseError> {
        let operator = match self.current() {
            Some(Token::Minus) => Some(UnaryOperator::Negate),
            Some(Token::Bang) => Some(UnaryOperator::Not),
            _ => None,
        };

        if let Some(operator) = operator {
            let start = self.current_span().start;
            self.advance();
            let operand = self.parse_expression(PREFIX_BINDING_POWER)?;
            let span = self.span(start, operand.span.end);
            let unary = UnaryExpression {
                operator,
                operand: Box::new(operand),
                span,
            };
            return Ok(Expression {
                kind: ExpressionKind::Unary(unary),
                span,
            });
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        if self.check(is_left_paren) && self.arrow_ahead() {
            let start = self.expect(is_left_paren, "`(`")?.start;
            let mut parameters = Vec::new();
            while !self.check(is_right_paren) {
                parameters.push(self.parse_parameter()?);
                if self.consume(is_comma).is_none() {
                    break;
                }
            }
            self.expect(is_right_paren, "`)`")?;
            self.expect(is_colon, "tipo explícito da arrow após `:`")?;
            let return_type = self.parse_type()?;
            self.expect(|t| matches!(t, Token::Arrow), "`=>`")?;
            let body = if self.check(is_left_brace) {
                self.parse_block()?
            } else {
                let value = self.parse_expression(0)?;
                let span = value.span;
                Block {
                    statements: vec![Statement::Return(ReturnStatement {
                        value: Some(value),
                        span,
                    })],
                    span,
                }
            };
            let span = self.span(start, body.span.end);
            return Ok(Expression {
                kind: ExpressionKind::Closure(truxt_ast::ClosureExpression {
                    parameters,
                    return_type,
                    body,
                }),
                span,
            });
        }
        if let Some(Token::TemplateLiteral(parts)) = self.current().cloned() {
            let span = self.current_span();
            self.advance();
            let mut values = Vec::new();
            for part in parts {
                values.push(match part {
                    truxt_lexer::TemplatePart::Text(text) => Expression {
                        kind: ExpressionKind::StringLiteral(text),
                        span,
                    },
                    truxt_lexer::TemplatePart::Expression(tokens) => {
                        let tokens = tokens
                            .into_iter()
                            .map(|(token, span)| (token, span.with_source(self.source)))
                            .collect::<Vec<_>>();
                        let mut parser = Parser::new_with_source(&tokens, self.source);
                        let expression = parser.parse_expression(0)?;
                        if !parser.is_at_end() {
                            return Err(parser.error("fim da interpolação"));
                        }
                        expression
                    }
                });
            }
            return Ok(Expression {
                kind: ExpressionKind::Template(values),
                span,
            });
        }
        let span = self.current_span();
        let token = self.current().cloned();

        match token {
            Some(Token::IntegerLiteral(value)) => {
                self.advance();
                Ok(Expression {
                    kind: ExpressionKind::IntegerLiteral(value),
                    span,
                })
            }
            Some(Token::NumberLiteral(value)) => {
                self.advance();
                Ok(Expression {
                    kind: ExpressionKind::NumberLiteral(value),
                    span,
                })
            }
            Some(Token::StringLiteral(value)) => {
                self.advance();
                Ok(Expression {
                    kind: ExpressionKind::StringLiteral(value),
                    span,
                })
            }
            Some(Token::BooleanLiteral(value)) => {
                self.advance();
                Ok(Expression {
                    kind: ExpressionKind::BooleanLiteral(value),
                    span,
                })
            }
            Some(Token::Null) => {
                self.advance();
                Ok(Expression {
                    kind: ExpressionKind::NullLiteral,
                    span,
                })
            }
            Some(Token::Identifier(name)) => {
                self.advance();
                let identifier = Identifier { name, span };
                Ok(Expression {
                    kind: ExpressionKind::Identifier(identifier),
                    span,
                })
            }
            Some(Token::LeftParen) => self.parse_grouping_expression(),
            Some(Token::LeftBrace) => self.parse_object_literal(),
            Some(Token::LeftBracket) => self.parse_array_literal(),
            _ => Err(self.error("expressão")),
        }
    }

    fn arrow_ahead(&self) -> bool {
        let mut depth = 0;
        for (index, (token, _)) in self.tokens[self.cursor..].iter().enumerate() {
            match token {
                Token::LeftParen => depth += 1,
                Token::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(self.cursor + index + 1),
                            Some((Token::Colon, _))
                        );
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn parse_grouping_expression(&mut self) -> Result<Expression, ParseError> {
        let open = self.expect(is_left_paren, "`(`")?;
        let inner = self.parse_expression(0)?;
        let close = self.expect(is_right_paren, "`)`")?;
        Ok(Expression {
            kind: ExpressionKind::Grouping(Box::new(inner)),
            span: self.span(open.start, close.end),
        })
    }

    fn parse_object_literal(&mut self) -> Result<Expression, ParseError> {
        let open = self.expect(is_left_brace, "`{`")?;
        let mut fields = Vec::new();

        if !self.check(is_right_brace) {
            loop {
                let field_start = self.current_span().start;
                let name = self.parse_identifier()?;
                self.expect(is_colon, "`:`")?;
                let value = self.parse_expression(0)?;
                let field_span = self.span(field_start, value.span.end);
                fields.push(ObjectField {
                    name,
                    value,
                    span: field_span,
                });

                if self.consume(is_comma).is_none() || self.check(is_right_brace) {
                    break;
                }
            }
        }

        let close = self.expect(is_right_brace, "`}`")?;
        let span = self.span(open.start, close.end);
        let object = ObjectLiteral { fields, span };
        Ok(Expression {
            kind: ExpressionKind::ObjectLiteral(object),
            span,
        })
    }

    fn parse_array_literal(&mut self) -> Result<Expression, ParseError> {
        let open = self.expect(is_left_bracket, "`[`")?;
        let mut elements = Vec::new();

        if !self.check(is_right_bracket) {
            loop {
                elements.push(self.parse_expression(0)?);
                if self.consume(is_comma).is_none() || self.check(is_right_bracket) {
                    break;
                }
            }
        }

        let close = self.expect(is_right_bracket, "`]`")?;
        let span = self.span(open.start, close.end);
        let array = ArrayLiteral { elements, span };
        Ok(Expression {
            kind: ExpressionKind::ArrayLiteral(array),
            span,
        })
    }

    fn parse_postfix(&mut self, left: Expression) -> Result<Expression, ParseError> {
        match self.current() {
            Some(Token::Dot) => {
                self.advance();
                let field = self.parse_identifier()?;
                let span = self.span(left.span.start, field.span.end);
                let access = FieldAccessExpression {
                    object: Box::new(left),
                    field,
                    span,
                };
                Ok(Expression {
                    kind: ExpressionKind::FieldAccess(access),
                    span,
                })
            }
            Some(Token::LeftParen) => {
                self.advance();
                let mut arguments = Vec::new();

                if !self.check(is_right_paren) {
                    loop {
                        arguments.push(self.parse_expression(0)?);
                        if self.consume(is_comma).is_none() || self.check(is_right_paren) {
                            break;
                        }
                    }
                }

                let close = self.expect(is_right_paren, "`)`")?;
                let span = self.span(left.span.start, close.end);
                let call = CallExpression {
                    callee: Box::new(left),
                    arguments,
                    span,
                };
                Ok(Expression {
                    kind: ExpressionKind::Call(call),
                    span,
                })
            }
            Some(Token::LeftBracket) => {
                self.advance();
                let index = self.parse_expression(0)?;
                let close = self.expect(is_right_bracket, "`]`")?;
                let span = self.span(left.span.start, close.end);
                let access = IndexExpression {
                    object: Box::new(left),
                    index: Box::new(index),
                    span,
                };
                Ok(Expression {
                    kind: ExpressionKind::Index(access),
                    span,
                })
            }
            _ => Err(self.error("operador postfix")),
        }
    }

    fn postfix_binding_power(&self) -> Option<u8> {
        match self.current() {
            Some(Token::Dot | Token::LeftParen | Token::LeftBracket) => Some(POSTFIX_BINDING_POWER),
            _ => None,
        }
    }

    fn infix_binding_power(&self) -> Option<(BinaryOperator, u8, u8)> {
        let (operator, left_power) = match self.current()? {
            Token::Nullish => (BinaryOperator::Coalesce, 0),
            Token::OrOr => (BinaryOperator::Or, 1),
            Token::AndAnd => (BinaryOperator::And, 3),
            Token::EqualEqual => (BinaryOperator::Equal, 5),
            Token::NotEqual => (BinaryOperator::NotEqual, 5),
            Token::Less => (BinaryOperator::Less, 7),
            Token::LessEqual => (BinaryOperator::LessEqual, 7),
            Token::Greater => (BinaryOperator::Greater, 7),
            Token::GreaterEqual => (BinaryOperator::GreaterEqual, 7),
            Token::Plus => (BinaryOperator::Add, 9),
            Token::Minus => (BinaryOperator::Subtract, 9),
            Token::Star => (BinaryOperator::Multiply, 11),
            Token::Slash => (BinaryOperator::Divide, 11),
            Token::Percent => (BinaryOperator::Remainder, 11),
            _ => return None,
        };
        Some((operator, left_power, left_power + 1))
    }

    fn parse_identifier(&mut self) -> Result<Identifier, ParseError> {
        match self.current() {
            Some(Token::Identifier(name)) => {
                let identifier = Identifier {
                    name: name.clone(),
                    span: self.current_span(),
                };
                self.advance();
                Ok(identifier)
            }
            _ => Err(self.error("identificador")),
        }
    }

    fn check(&self, predicate: fn(&Token) -> bool) -> bool {
        self.current().is_some_and(predicate)
    }

    fn check_n(&self, offset: usize, predicate: fn(&Token) -> bool) -> bool {
        self.tokens
            .get(self.cursor + offset)
            .is_some_and(|(token, _)| predicate(token))
    }

    fn consume(&mut self, predicate: fn(&Token) -> bool) -> Option<Span> {
        if self.check(predicate) {
            self.advance().map(|(_, span)| span)
        } else {
            None
        }
    }

    fn expect(
        &mut self,
        predicate: fn(&Token) -> bool,
        expected: &'static str,
    ) -> Result<Span, ParseError> {
        if self.check(predicate) {
            self.advance()
                .map(|(_, span)| span)
                .ok_or_else(|| self.error(expected))
        } else {
            Err(self.error(expected))
        }
    }

    fn error(&self, expected: &'static str) -> ParseError {
        let kind = if self.is_at_end() {
            ParseErrorKind::UnexpectedEndOfInput
        } else {
            ParseErrorKind::UnexpectedToken
        };
        ParseError::new(kind, expected, self.current_span())
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.cursor).map(|(token, _)| token)
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.cursor)
            .map(|(_, span)| *span)
            .unwrap_or_else(|| self.eof_span())
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::in_source(self.source, start, end)
    }

    fn eof_span(&self) -> Span {
        self.tokens
            .last()
            .map(|(_, span)| self.span(span.end, span.end))
            .unwrap_or_else(|| self.span(0, 0))
    }

    fn advance(&mut self) -> Option<(Token, Span)> {
        let item = self.tokens.get(self.cursor)?.clone();
        self.cursor += 1;
        self.update_delimiters(&item.0);
        Some(item)
    }

    fn is_at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    fn synchronize(&mut self, block_depth: Option<usize>) {
        let base_depth = block_depth.unwrap_or(0);
        while !self.is_at_end() {
            let at_base_depth = self.delimiters.len() == base_depth;
            if at_base_depth {
                if self.check(is_semicolon) {
                    self.advance();
                    return;
                }
                if block_depth.is_some() && self.check(is_right_brace) {
                    return;
                }
                if self.is_recovery_keyword(block_depth.is_some()) {
                    return;
                }
            } else if block_depth.is_some()
                && self.check(is_right_brace)
                && !self
                    .delimiters
                    .iter()
                    .skip(base_depth)
                    .any(|delimiter| *delimiter == Delimiter::Brace)
            {
                // This is the enclosing block's brace, not an unclosed object.
                return;
            }
            self.advance();
        }
    }

    fn is_recovery_keyword(&self, in_block: bool) -> bool {
        match self.current() {
            Some(Token::Const | Token::Let | Token::If | Token::While | Token::Return) => true,
            Some(Token::Function | Token::Interface | Token::Import) => !in_block,
            _ => false,
        }
    }

    fn update_delimiters(&mut self, token: &Token) {
        let opening = match token {
            Token::LeftParen => Some(Delimiter::Parenthesis),
            Token::LeftBracket => Some(Delimiter::Bracket),
            Token::LeftBrace => Some(Delimiter::Brace),
            _ => None,
        };
        if let Some(delimiter) = opening {
            self.delimiters.push(delimiter);
            return;
        }

        let closing = match token {
            Token::RightParen => Some(Delimiter::Parenthesis),
            Token::RightBracket => Some(Delimiter::Bracket),
            Token::RightBrace => Some(Delimiter::Brace),
            _ => None,
        };
        if let Some(delimiter) = closing {
            if let Some(index) = self.delimiters.iter().rposition(|open| *open == delimiter) {
                // Discard unmatched nested delimiters while recovering at a closer.
                self.delimiters.truncate(index);
            }
        }
    }
}

fn can_start_expression(token: &Token) -> bool {
    if matches!(token, Token::TemplateLiteral(_)) {
        return true;
    }
    matches!(
        token,
        Token::IntegerLiteral(_)
            | Token::NumberLiteral(_)
            | Token::StringLiteral(_)
            | Token::BooleanLiteral(_)
            | Token::Null
            | Token::Identifier(_)
            | Token::LeftParen
            | Token::LeftBrace
            | Token::LeftBracket
            | Token::Minus
            | Token::Bang
    )
}

// EN: Parentheses and other expression forms delimit the logical/coalesce mix rule.
// PT-BR: Parênteses e outras formas delimitam a regra de mistura lógico/coalesce.
fn has_ungrouped_operator(expression: &Expression, operators: &[BinaryOperator]) -> bool {
    if let ExpressionKind::Binary(binary) = &expression.kind {
        operators.contains(&binary.operator)
            || has_ungrouped_operator(&binary.left, operators)
            || has_ungrouped_operator(&binary.right, operators)
    } else {
        false
    }
}

fn is_identifier(token: &Token) -> bool {
    matches!(token, Token::Identifier(_))
}
fn is_module(token: &Token) -> bool {
    matches!(token, Token::Identifier(name) if name == "module")
}
fn is_interface(token: &Token) -> bool {
    matches!(token, Token::Interface)
}
fn is_function(token: &Token) -> bool {
    matches!(token, Token::Function)
}
fn is_left_brace(token: &Token) -> bool {
    matches!(token, Token::LeftBrace)
}
fn is_right_brace(token: &Token) -> bool {
    matches!(token, Token::RightBrace)
}
fn is_left_bracket(token: &Token) -> bool {
    matches!(token, Token::LeftBracket)
}
fn is_right_bracket(token: &Token) -> bool {
    matches!(token, Token::RightBracket)
}
fn is_left_paren(token: &Token) -> bool {
    matches!(token, Token::LeftParen)
}
fn is_right_paren(token: &Token) -> bool {
    matches!(token, Token::RightParen)
}
fn is_const(token: &Token) -> bool {
    matches!(token, Token::Const)
}
fn is_let(token: &Token) -> bool {
    matches!(token, Token::Let)
}
fn is_if(token: &Token) -> bool {
    matches!(token, Token::If)
}
fn is_else(token: &Token) -> bool {
    matches!(token, Token::Else)
}
fn is_while(token: &Token) -> bool {
    matches!(token, Token::While)
}
fn is_return(token: &Token) -> bool {
    matches!(token, Token::Return)
}
fn is_colon(token: &Token) -> bool {
    matches!(token, Token::Colon)
}
fn is_comma(token: &Token) -> bool {
    matches!(token, Token::Comma)
}
fn is_equal(token: &Token) -> bool {
    matches!(token, Token::Equal)
}
fn is_semicolon(token: &Token) -> bool {
    matches!(token, Token::Semicolon)
}
