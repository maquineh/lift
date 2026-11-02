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

/// FFI-visible struct: public fields with the exact Lift field mapping.
///
/// PT-BR: Struct visível ao FFI, com campos públicos e mapeamento exato Lift.
#[derive(Clone, Debug, PartialEq)]
pub struct IntRange {
    pub start: i64,
    pub end: i64,
}

/// FFI-visible struct with a required and an optional field.
///
/// PT-BR: Struct visível ao FFI com um campo obrigatório e um opcional.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    pub note: Option<String>,
}

/// Builds an [`IntRange`] by value; the returned struct is owned by the caller.
///
/// PT-BR: Constrói um [`IntRange`] por valor; a struct retornada pertence ao chamador.
pub fn int_range(start: i64, end: i64) -> IntRange {
    IntRange { start, end }
}

/// Reads a borrowed range and never consumes the caller's value.
///
/// PT-BR: Lê um range emprestado sem consumir o valor do chamador.
pub fn range_len(range: &IntRange) -> i64 {
    range.end.saturating_sub(range.start).max(0)
}

/// Sums borrowed ranges without consuming the array.
///
/// PT-BR: Soma ranges emprestados sem consumir o array.
pub fn total_len(ranges: &[IntRange]) -> i64 {
    ranges.iter().map(range_len).sum()
}

/// Reads an optional borrowed range, falling back when it is absent.
///
/// PT-BR: Lê um range opcional emprestado, usando o fallback na ausência.
pub fn range_len_or(range: Option<&IntRange>, fallback: i64) -> i64 {
    range.map(range_len).unwrap_or(fallback)
}

/// Returns an optional owned range.
///
/// PT-BR: Retorna um range opcional próprio.
pub fn maybe_range(start: i64) -> Option<IntRange> {
    (start >= 0).then(|| IntRange {
        start,
        end: start + 2,
    })
}

/// Doubles an optional borrowed integer.
///
/// PT-BR: Dobra um inteiro opcional emprestado.
pub fn maybe_double(value: Option<i64>) -> Option<i64> {
    value.map(|value| value * 2)
}

/// Trims an optional borrowed string into an owned optional string.
///
/// PT-BR: Remove whitespace de uma string opcional emprestada.
pub fn maybe_trim(value: Option<&str>) -> Option<String> {
    value.map(|value| value.trim().to_owned())
}

/// Sums an optional borrowed array of ranges.
///
/// PT-BR: Soma um array opcional emprestado de ranges.
pub fn maybe_total(ranges: Option<&[IntRange]>) -> i64 {
    ranges.map(total_len).unwrap_or(-1)
}

/// Returns a range or a textual error, exercising `Result` with an owned struct.
///
/// PT-BR: Retorna um range ou erro textual, exercitando `Result` com struct própria.
pub fn try_range(start: i64) -> Result<IntRange, String> {
    if start >= 0 {
        Ok(IntRange {
            start,
            end: start + 1,
        })
    } else {
        Err("negative start".to_owned())
    }
}

/// Describes a borrowed Result of a range, including its error text.
///
/// PT-BR: Descreve um Result emprestado de range, incluindo o texto de erro.
pub fn describe_range(result: &Result<IntRange, String>) -> String {
    match result {
        Ok(range) => format!("{}..{}", range.start, range.end),
        Err(error) => format!("error: {error}"),
    }
}

/// Parses an integer into an optional Result payload; empty text means `Ok(None)`.
///
/// PT-BR: Converte texto em payload opcional de Result; vazio significa `Ok(None)`.
pub fn maybe_parse(text: &str) -> Result<Option<i64>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        text.trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("not an int: {text}"))
    }
}

/// Describes a borrowed Result with an optional integer payload.
///
/// PT-BR: Descreve um Result emprestado com payload inteiro opcional.
pub fn describe_optional(result: &Result<Option<i64>, String>) -> String {
    match result {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => "none".to_owned(),
        Err(error) => format!("error: {error}"),
    }
}

/// Reads a borrowed label, including its optional note.
///
/// PT-BR: Lê um label emprestado, incluindo a nota opcional.
pub fn label_text(label: &Label) -> String {
    match &label.note {
        Some(note) => format!("{} ({note})", label.text),
        None => label.text.clone(),
    }
}

/// Applies a borrowed callback twice to an owned integer.
///
/// PT-BR: Aplica um callback emprestado duas vezes a um inteiro owned.
pub fn apply_twice(value: i64, callback: &dyn Fn(i64) -> i64) -> i64 {
    callback(callback(value))
}

/// Maps an owned string through a borrowed callback.
///
/// PT-BR: Mapeia uma string owned por um callback emprestado.
pub fn apply_text(value: &str, callback: &dyn Fn(String) -> String) -> String {
    callback(value.to_owned())
}

/// Returns a boxed callback that adds a constant.
///
/// PT-BR: Retorna um callback boxed que soma uma constante.
pub fn add_constant(amount: i64) -> Box<dyn Fn(i64) -> i64> {
    Box::new(move |value| value + amount)
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
        let _: fn(i64, i64) -> IntRange = int_range;
        let _: fn(&IntRange) -> i64 = range_len;
        let _: fn(&[IntRange]) -> i64 = total_len;
        let _: fn(Option<&IntRange>, i64) -> i64 = range_len_or;
        let _: fn(i64) -> Option<IntRange> = maybe_range;
        let _: fn(Option<i64>) -> Option<i64> = maybe_double;
        let _: fn(Option<&str>) -> Option<String> = maybe_trim;
        let _: fn(&Result<IntRange, String>) -> String = describe_range;
        let _: fn(&str) -> Result<Option<i64>, String> = maybe_parse;
        let _: fn(&Result<Option<i64>, String>) -> String = describe_optional;
        let _: fn(i64, &dyn Fn(i64) -> i64) -> i64 = apply_twice;
        let _: fn(&str, &dyn Fn(String) -> String) -> String = apply_text;
        let _: fn(i64) -> Box<dyn Fn(i64) -> i64> = add_constant;
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
