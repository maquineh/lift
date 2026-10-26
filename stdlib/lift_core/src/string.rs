//! Native Rust operations on borrowed UTF-8 strings.
//!
//! PT-BR: Operações Rust nativas sobre strings UTF-8 emprestadas.

/// Conta valores escalares Unicode (`char`), sem alocar.
///
/// Não conta bytes, unidades UTF-16 nem grafemas visuais: `"é"` tem comprimento
/// 1, enquanto `"e\u{301}"` (letra seguida de acento combinante) tem 2.
pub fn len(value: &str) -> usize {
    value.chars().count()
}

/// Informa se a string não contém caracteres.
pub fn is_empty(value: &str) -> bool {
    value.is_empty()
}

/// Verifica a presença exata de uma substring, com distinção entre maiúsculas.
///
/// Não normaliza Unicode. A substring vazia está contida em qualquer string.
pub fn contains(value: &str, substring: &str) -> bool {
    value.contains(substring)
}

/// Concatena duas strings em uma nova [`String`], sem modificar as entradas.
pub fn concat(left: &str, right: &str) -> String {
    let mut result = String::from(left);
    result.push_str(right);
    result
}

/// Remove whitespace Unicode das extremidades, retornando uma fatia emprestada.
///
/// Segue [`str::trim`] e preserva os espaços internos.
pub fn trim(value: &str) -> &str {
    value.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_unicode_scalars_instead_of_bytes_or_graphemes() {
        assert_eq!(len(""), 0);
        assert_eq!(len("abc"), 3);
        assert_eq!(len("é🙂"), 2);
        assert_eq!(len("e\u{301}"), 2);
        assert_eq!(len("👩\u{200d}💻"), 3);
    }

    #[test]
    fn emptiness_and_contains_use_exact_substrings() {
        assert!(is_empty(""));
        assert!(!is_empty(" "));
        assert!(contains("ação🙂", "ção"));
        assert!(contains("ação🙂", "🙂"));
        assert!(!contains("ação", "AÇÃO"));
        assert!(!contains("é", "e\u{301}"));
        assert!(contains("", ""));
        assert!(!contains("", "a"));
    }

    #[test]
    fn concat_preserves_inputs_and_handles_empty_strings() {
        let left = String::from("olá ");
        assert_eq!(concat(&left, "🙂"), "olá 🙂");
        assert_eq!(left, "olá ");
        assert_eq!(concat("", "á"), "á");
        assert_eq!(concat("á", ""), "á");
        assert_eq!(concat("", ""), "");
    }

    #[test]
    fn trim_borrows_unicode_text_and_preserves_internal_spaces() {
        let value = String::from("\u{2003}\t olá  mundo \n\u{2003}");
        let trimmed = trim(&value);
        assert_eq!(trimmed, "olá  mundo");
        let start = value.find("olá").unwrap();
        assert_eq!(trimmed.as_ptr(), value[start..].as_ptr());
        assert_eq!(trim(" \t\n\u{2003}"), "");
        assert_eq!(trim(""), "");
    }
}
