//! Parser for the untyped Truxt syntax tree.
//!
//! Declarations and statements use recursive descent; expressions use Pratt
//! parsing with operator precedence and postfix operations. Errors retain spans.
//!
//! PT-BR: Parser da AST não tipada do Truxt. Declarações e statements usam
//! recursive descent; expressões usam Pratt com precedência e postfixos.
//! Erros preservam spans para os diagnósticos.

mod error;
mod parser;

pub use error::{ParseError, ParseErrorKind};
use truxt_ast::ProgramAst;
use truxt_ast::Span;
use truxt_lexer::Token;

/// Converte tokens com spans em um programa AST.
///
/// Os erros são acumulados com recuperação entre statements/declarations e
/// carregam spans compatíveis com `truxt_ast::Span`.
pub fn parse(tokens: &[(Token, Span)]) -> Result<ProgramAst, Vec<ParseError>> {
    parser::Parser::new(tokens).parse_program()
}
