//! Structured parser errors with expected syntax and source spans.
//!
//! PT-BR: Erros estruturados do parser com sintaxe esperada e spans de fonte.

use std::{error::Error, fmt};

use lift_ast::Span;

/// Classe de erro sintático.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    UnexpectedToken,
    UnexpectedEndOfInput,
}

/// Erro de parsing. `expected` descreve a construção esperada no span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub expected: &'static str,
    pub span: Span,
}

impl ParseError {
    pub(crate) fn new(kind: ParseErrorKind, expected: &'static str, span: Span) -> Self {
        Self {
            kind,
            expected,
            span,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let found = match self.kind {
            ParseErrorKind::UnexpectedToken => "token inesperado",
            ParseErrorKind::UnexpectedEndOfInput => "fim inesperado da fonte",
        };
        write!(
            formatter,
            "{found} no span {}..{}; esperado {}",
            self.span.start, self.span.end, self.expected
        )
    }
}

impl Error for ParseError {}
