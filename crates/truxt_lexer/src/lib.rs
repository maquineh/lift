//! Truxt lexer: tokens with half-open UTF-8 byte spans.
//!
//! String tokens preserve raw escape text without quotes. The code generator
//! decodes supported escapes after semantic analysis.
//!
//! PT-BR: Lexer do Truxt com spans de bytes semiabertos, compartilhados com
//! `truxt_ast::Span`. Strings preservam escapes sem aspas; o codegen decodifica
//! os escapes suportados após a análise semântica.

use std::{error::Error, fmt, ops::Range};

use logos::{Lexer, Logos};
use truxt_ast::Span;

/// Converte a fonte em tokens ordenados e seus spans.
///
/// A primeira falha lexical interrompe a tokenização e retorna o span do erro.
pub fn tokenize(source: &str) -> Result<Vec<(Token, Span)>, LexError> {
    let mut result = Vec::new();

    for (token, range) in Token::lexer(source).spanned() {
        let span = Span::from(range.clone());
        match token {
            Ok(Token::UnterminatedBlockComment) => {
                return Err(LexError {
                    kind: LexErrorKind::UnterminatedBlockComment,
                    span,
                });
            }
            Ok(token) => result.push((token, span)),
            Err(kind) => return Err(classify_error(source, kind, range)),
        }
    }

    Ok(result)
}

fn classify_error(source: &str, error: LogosErrorKind, range: Range<usize>) -> LexError {
    let kind = match error {
        LogosErrorKind::UnexpectedCharacter => {
            if let Some(end) = unterminated_string_end(source, range.start) {
                return LexError {
                    kind: LexErrorKind::UnterminatedString,
                    span: Span::new(range.start, end),
                };
            }
            LexErrorKind::UnexpectedCharacter
        }
        LogosErrorKind::IntegerOutOfRange => LexErrorKind::IntegerOutOfRange,
        LogosErrorKind::NumberOutOfRange => LexErrorKind::NumberOutOfRange,
    };

    LexError {
        kind,
        span: Span::from(range),
    }
}

/// Se a falha começa por uma aspa sem literal válido, encontra o fim da linha
/// ou da fonte respeitando escapes. O retorno inclui o caractere de abertura.
fn unterminated_string_end(source: &str, start: usize) -> Option<usize> {
    let remainder = source.get(start..)?;
    let quote = remainder.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let mut escaped = false;
    for (offset, character) in remainder.char_indices().skip(1) {
        if character == '\n' || character == '\r' {
            return Some(start + offset);
        }
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return None;
        }
    }

    Some(source.len())
}

/// Tokens emitidos pelo lexer. Valores de literais ficam disponíveis ao parser,
/// cuja assinatura-alvo recebe tokens e spans, sem receber a fonte original.
#[derive(Clone, Debug, PartialEq, Logos)]
#[logos(error = LogosErrorKind)]
#[logos(skip r"[ \t\r\n\x0C]+")]
#[logos(skip(r"//[^\r\n]*", allow_greedy = true))]
#[logos(skip(r"(?s:/\*.*?\*/)"))]
pub enum Token {
    // Palavras-chave e tipos primitivos.
    #[token("function")]
    Function,
    #[token("import")]
    Import,
    #[token("from")]
    From,
    #[token("interface")]
    Interface,
    #[token("const")]
    Const,
    #[token("let")]
    Let,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("return")]
    Return,
    #[token("int")]
    IntType,
    #[token("number")]
    NumberType,
    #[token("string")]
    StringType,
    #[token("boolean")]
    BooleanType,
    #[token("void")]
    VoidType,
    #[token("true", |_| true)]
    #[token("false", |_| false)]
    BooleanLiteral(bool),

    // Identificadores e literais. Identificadores seguem XID Unicode, além de _.
    #[regex(r"[_\p{XID_Start}][_\p{XID_Continue}]*", |lex| lex.slice().to_owned())]
    Identifier(String),
    #[regex(
        r"(?:[0-9]+\.[0-9]+(?:[eE][+-]?[0-9]+)?|[0-9]+[eE][+-]?[0-9]+)",
        parse_number
    )]
    NumberLiteral(f64),
    // Armazena a magnitude para permitir que a fase semântica aceite
    // -9223372036854775808, cujo valor absoluto não cabe em i64.
    #[regex(r"[0-9]+", |lex| lex.slice().parse::<u64>().map_err(|_| LogosErrorKind::IntegerOutOfRange))]
    IntegerLiteral(u64),
    #[regex(r#""([^"\\\r\n]|\\.)*""#, string_contents)]
    #[regex(r#"'([^'\\\r\n]|\\.)*'"#, string_contents)]
    StringLiteral(String),

    // Operadores compostos antes dos operadores de um caractere.
    #[token("=>")]
    Arrow,
    #[token("==")]
    EqualEqual,
    #[token("!=")]
    NotEqual,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("??")]
    Nullish,

    // Operadores simples.
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("=")]
    Equal,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,
    #[token("!")]
    Bang,

    // Pontuação usada pela gramática core e por tipos futuros.
    #[token(".")]
    Dot,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(";")]
    Semicolon,
    #[token("?")]
    Question,
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,

    // Token auxiliar consumido pelo lexer para diagnosticar comentários sem fim.
    #[regex(r"(?s:/\*(?:[^*]|\*+[^*/])*(?:\*+)?\z)", allow_greedy = true)]
    UnterminatedBlockComment,
}

fn parse_number(lexer: &mut Lexer<'_, Token>) -> Result<f64, LogosErrorKind> {
    let number = lexer
        .slice()
        .parse::<f64>()
        .map_err(|_| LogosErrorKind::NumberOutOfRange)?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(LogosErrorKind::NumberOutOfRange)
    }
}

fn string_contents(lexer: &mut Lexer<'_, Token>) -> String {
    let literal = lexer.slice();
    literal[1..literal.len() - 1].to_owned()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub enum LogosErrorKind {
    #[default]
    UnexpectedCharacter,
    IntegerOutOfRange,
    NumberOutOfRange,
}

/// Categoria pública de uma falha lexical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexErrorKind {
    UnexpectedCharacter,
    UnterminatedString,
    UnterminatedBlockComment,
    IntegerOutOfRange,
    NumberOutOfRange,
}

/// Erro lexical com posição na fonte original.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            LexErrorKind::UnexpectedCharacter => "caractere inesperado",
            LexErrorKind::UnterminatedString => "literal de string não terminado",
            LexErrorKind::UnterminatedBlockComment => "comentário de bloco não terminado",
            LexErrorKind::IntegerOutOfRange => {
                "magnitude do literal inteiro fora do intervalo de u64"
            }
            LexErrorKind::NumberOutOfRange => {
                "literal numérico inválido ou fora do intervalo de f64"
            }
        };
        write!(
            formatter,
            "{message} no span {}..{}",
            self.span.start, self.span.end
        )
    }
}

impl Error for LexError {}

#[cfg(test)]
mod tests {
    use super::{tokenize, LexErrorKind, Token};
    use truxt_ast::Span;

    fn tokens(source: &str) -> Vec<Token> {
        tokenize(source)
            .expect("fonte deve tokenizar")
            .into_iter()
            .map(|(token, _)| token)
            .collect()
    }

    #[test]
    fn tokenizes_function_declaration_and_operators() {
        let source = "function soma(a: int, b: int): int { return a + b; }";
        let actual = tokenize(source).expect("função deve tokenizar");

        assert_eq!(
            actual
                .iter()
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>(),
            vec![
                Token::Function,
                Token::Identifier("soma".into()),
                Token::LeftParen,
                Token::Identifier("a".into()),
                Token::Colon,
                Token::IntType,
                Token::Comma,
                Token::Identifier("b".into()),
                Token::Colon,
                Token::IntType,
                Token::RightParen,
                Token::Colon,
                Token::IntType,
                Token::LeftBrace,
                Token::Return,
                Token::Identifier("a".into()),
                Token::Plus,
                Token::Identifier("b".into()),
                Token::Semicolon,
                Token::RightBrace,
            ]
        );

        assert_eq!(actual[0].1, Span::new(0, 8));
        assert_eq!(actual[1].1, Span::new(9, 13));
    }

    #[test]
    fn tokenizes_object_literal_with_string_and_boolean_values() {
        assert_eq!(
            tokens(r#"const usuario = { nome: "Ana", ativo: true }"#),
            vec![
                Token::Const,
                Token::Identifier("usuario".into()),
                Token::Equal,
                Token::LeftBrace,
                Token::Identifier("nome".into()),
                Token::Colon,
                Token::StringLiteral("Ana".into()),
                Token::Comma,
                Token::Identifier("ativo".into()),
                Token::Colon,
                Token::BooleanLiteral(true),
                Token::RightBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_array_literal_and_distinguishes_numeric_types() {
        assert_eq!(
            tokens("let valores = [1, 2.5, 3e2];"),
            vec![
                Token::Let,
                Token::Identifier("valores".into()),
                Token::Equal,
                Token::LeftBracket,
                Token::IntegerLiteral(1),
                Token::Comma,
                Token::NumberLiteral(2.5),
                Token::Comma,
                Token::NumberLiteral(300.0),
                Token::RightBracket,
                Token::Semicolon,
            ]
        );
    }

    #[test]
    fn recognizes_compound_operators_before_single_character_operators() {
        assert_eq!(
            tokens("a <= b && b != c || !ok == false; x => x ?? y"),
            vec![
                Token::Identifier("a".into()),
                Token::LessEqual,
                Token::Identifier("b".into()),
                Token::AndAnd,
                Token::Identifier("b".into()),
                Token::NotEqual,
                Token::Identifier("c".into()),
                Token::OrOr,
                Token::Bang,
                Token::Identifier("ok".into()),
                Token::EqualEqual,
                Token::BooleanLiteral(false),
                Token::Semicolon,
                Token::Identifier("x".into()),
                Token::Arrow,
                Token::Identifier("x".into()),
                Token::Nullish,
                Token::Identifier("y".into()),
            ]
        );
    }

    #[test]
    fn spans_are_byte_offsets_and_strings_keep_raw_escapes() {
        let source = r#"const ação = "olá\n";"#;
        let actual = tokenize(source).expect("identificador Unicode deve tokenizar");

        assert_eq!(
            actual[1],
            (Token::Identifier("ação".into()), Span::new(6, 12))
        );
        assert_eq!(actual[3].0, Token::StringLiteral(r"olá\n".into()));
        assert_eq!(actual[3].1, Span::new(15, 23));
    }

    #[test]
    fn skips_line_and_block_comments() {
        assert_eq!(
            tokens("let x = 1; // comentário\n/***/ /* bloco */ x = x + 1;"),
            vec![
                Token::Let,
                Token::Identifier("x".into()),
                Token::Equal,
                Token::IntegerLiteral(1),
                Token::Semicolon,
                Token::Identifier("x".into()),
                Token::Equal,
                Token::Identifier("x".into()),
                Token::Plus,
                Token::IntegerLiteral(1),
                Token::Semicolon,
            ]
        );
    }

    #[test]
    fn reports_unexpected_characters_and_unterminated_literals() {
        let unexpected = tokenize("$").expect_err("$ não é token");
        assert_eq!(unexpected.kind, LexErrorKind::UnexpectedCharacter);
        assert_eq!(unexpected.span, Span::new(0, 1));

        let unterminated = tokenize("const s = \"sem fim").expect_err("string deve fechar");
        assert_eq!(unterminated.kind, LexErrorKind::UnterminatedString);
        assert_eq!(unterminated.span, Span::new(10, 18));

        let unterminated_comment = tokenize("/* sem fim").expect_err("comentário deve fechar");
        assert_eq!(
            unterminated_comment.kind,
            LexErrorKind::UnterminatedBlockComment
        );
        assert_eq!(unterminated_comment.span, Span::new(0, 10));
    }

    #[test]
    fn preserves_minimum_i64_magnitude_and_reports_u64_overflow() {
        assert_eq!(
            tokens("-9223372036854775808"),
            vec![
                Token::Minus,
                Token::IntegerLiteral(9_223_372_036_854_775_808)
            ]
        );

        let error = tokenize("18446744073709551616").expect_err("magnitude não cabe em u64");

        assert_eq!(error.kind, LexErrorKind::IntegerOutOfRange);
        assert_eq!(error.span, Span::new(0, 20));
    }

    #[test]
    fn rejects_non_finite_float_literals() {
        let error = tokenize("1e999").expect_err("número excede f64 finito");

        assert_eq!(error.kind, LexErrorKind::NumberOutOfRange);
        assert_eq!(error.span, Span::new(0, 5));
    }
}
