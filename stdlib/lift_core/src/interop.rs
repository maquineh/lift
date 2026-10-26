//! Standard-library adapters for the current Lift FFI type mapping.
//!
//! Lengths use Lift `int` (`i64`), returned strings are owned, and primitive
//! membership arguments are passed by value. Inputs remain borrowed. The native
//! Rust APIs in [`crate::array`] and [`crate::string`] retain their signatures.
//!
//! PT-BR: Adaptadores da biblioteca padrão para o mapeamento FFI atual do Lift.
//! Comprimentos usam `int` (`i64`), strings retornadas são próprias e valores
//! primitivos de busca são passados por valor. As entradas são emprestadas. As
//! APIs Rust nativas em [`crate::array`] e [`crate::string`] mantêm suas assinaturas.

pub fn parse_int(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|_| "value is not a signed 64-bit integer".into())
}

/// Escapes a scalar string for a line-oriented text protocol.
pub fn escape_lines(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| match c {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            c => vec![c],
        })
        .collect()
}

/// Returns the array length as a Lift `int`, without consuming its elements.
///
/// Panics if the length cannot be represented by `i64`.
///
/// PT-BR: Retorna o comprimento do array como `int` Lift, sem consumir os
/// elementos. Causa panic se o comprimento não puder ser representado por `i64`.
pub fn array_len<T>(values: &[T]) -> i64 {
    length_as_int(crate::array::len(values))
}

/// Counts Unicode scalar values as a Lift `int`.
///
/// This counts neither UTF-8 bytes nor visual graphemes. Panics if the count
/// cannot be represented by `i64`.
///
/// PT-BR: Conta valores escalares Unicode como `int` Lift. Não conta bytes
/// UTF-8 nem grafemas visuais. Causa panic se a contagem não couber em `i64`.
pub fn string_len(value: &str) -> i64 {
    length_as_int(crate::string::len(value))
}

/// Returns an owned string with Unicode whitespace removed from both ends.
///
/// PT-BR: Retorna uma string própria sem whitespace Unicode nas extremidades.
pub fn string_trim(value: &str) -> String {
    crate::string::trim(value).to_owned()
}

/// Tests exact integer membership without consuming the array.
///
/// PT-BR: Verifica a presença exata de um inteiro sem consumir o array.
pub fn array_contains_int(values: &[i64], value: i64) -> bool {
    crate::array::contains(values, &value)
}

/// Tests numeric membership with Rust `f64` equality; `NaN` never equals itself.
///
/// PT-BR: Verifica a presença de um número com igualdade `f64` do Rust;
/// `NaN` nunca é igual a si mesmo.
pub fn array_contains_number(values: &[f64], value: f64) -> bool {
    crate::array::contains(values, &value)
}

/// Tests boolean membership without consuming the array.
///
/// PT-BR: Verifica a presença de um booleano sem consumir o array.
pub fn array_contains_boolean(values: &[bool], value: bool) -> bool {
    crate::array::contains(values, &value)
}

/// Tests exact, case-sensitive string membership without normalizing Unicode.
///
/// PT-BR: Verifica a presença exata de uma string, distinguindo maiúsculas e
/// minúsculas e sem normalizar Unicode.
pub fn array_contains_string(values: &[String], value: &str) -> bool {
    values.iter().any(|candidate| candidate == value)
}

fn length_as_int(length: usize) -> i64 {
    i64::try_from(length).expect("length exceeds the Lift int range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_have_signatures_accepted_by_the_current_ffi() {
        let _: fn(&[i64]) -> i64 = array_len::<i64>;
        let _: fn(&[f64]) -> i64 = array_len::<f64>;
        let _: fn(&[bool]) -> i64 = array_len::<bool>;
        let _: fn(&[String]) -> i64 = array_len::<String>;
        let _: fn(&str) -> i64 = string_len;
        let _: fn(&str) -> String = string_trim;
        let _: fn(&[i64], i64) -> bool = array_contains_int;
        let _: fn(&[f64], f64) -> bool = array_contains_number;
        let _: fn(&[bool], bool) -> bool = array_contains_boolean;
        let _: fn(&[String], &str) -> bool = array_contains_string;
    }

    #[test]
    fn lengths_preserve_array_inputs_and_count_unicode_scalars() {
        let values = [String::from("first"), String::from("second")];
        assert_eq!(array_len(&values), 2_i64);
        assert_eq!(array_len::<i64>(&[]), 0_i64);
        assert_eq!(values[0], "first");
        assert_eq!(string_len(""), 0_i64);
        assert_eq!(string_len("é🙂"), 2_i64);
        assert_eq!(string_len("e\u{301}"), 2_i64);
    }

    #[test]
    fn trimmed_strings_own_their_result_and_preserve_the_input() {
        let value = String::from("\u{2003}  Olá  Lift\t\n");
        let mut trimmed = string_trim(&value);
        assert_eq!(trimmed, "Olá  Lift");
        trimmed.push('!');
        assert_eq!(value, "\u{2003}  Olá  Lift\t\n");
        assert_eq!(string_trim(" \t\n"), "");
    }

    #[test]
    fn membership_uses_the_primitive_equality_rules() {
        assert!(array_contains_int(&[i64::MIN, 0, i64::MAX], i64::MAX));
        assert!(!array_contains_int(&[], 1));
        assert!(array_contains_number(&[-0.0, 2.5], 0.0));
        assert!(!array_contains_number(&[f64::NAN], f64::NAN));
        assert!(!array_contains_number(&[], 0.0));
        assert!(array_contains_boolean(&[false, true], true));
        assert!(!array_contains_boolean(&[false], true));
        assert!(!array_contains_boolean(&[], false));
    }

    #[test]
    fn string_membership_is_exact_and_keeps_the_array_available() {
        let values = [String::from("ação"), String::from("é")];
        assert!(array_contains_string(&values, "ação"));
        assert!(!array_contains_string(&values, "AÇÃO"));
        assert!(!array_contains_string(&values, "e\u{301}"));
        assert!(!array_contains_string(&[], ""));
        assert_eq!(values[0], "ação");
    }
}
