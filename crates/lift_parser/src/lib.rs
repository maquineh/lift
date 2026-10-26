//! Parser for the untyped Lift syntax tree.
//!
//! Declarations and statements use recursive descent; expressions use Pratt
//! parsing with operator precedence and postfix operations. Errors retain spans.
//!
//! PT-BR: Parser da AST não tipada do Lift. Declarações e statements usam
//! recursive descent; expressões usam Pratt com precedência e postfixos.
//! Erros preservam spans para os diagnósticos.

mod error;
mod parser;

pub use error::{ParseError, ParseErrorKind};
use lift_ast::ProgramAst;
use lift_ast::{SourceId, Span};
use lift_lexer::Token;

/// Converte tokens com spans em um programa AST.
///
/// Os erros são acumulados com recuperação entre statements/declarations e
/// carregam spans compatíveis com `lift_ast::Span`.
pub fn parse(tokens: &[(Token, Span)]) -> Result<ProgramAst, Vec<ParseError>> {
    parser::Parser::new(tokens).parse_program()
}

/// Parses one source, including empty input; PT-BR: preserva o ID mesmo sem tokens.
pub fn parse_with_source(
    tokens: &[(Token, Span)],
    source: SourceId,
) -> Result<ProgramAst, Vec<ParseError>> {
    if tokens.iter().all(|(_, span)| span.source == source) {
        return parser::Parser::new_with_source(tokens, source).parse_program();
    }
    let normalized = tokens
        .iter()
        .map(|(token, span)| (token.clone(), span.with_source(source)))
        .collect::<Vec<_>>();
    parser::Parser::new_with_source(&normalized, source).parse_program()
}
