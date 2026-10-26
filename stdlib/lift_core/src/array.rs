//! Native Rust helpers for borrowed slices, preserving their elements.
//!
//! `map`, `filter`, and `find` accept stateful `FnMut` callbacks. For signatures
//! importable by Lift, see [`crate::interop`].
//!
//! PT-BR: Helpers Rust nativos que emprestam slices, sem modificar ou consumir
//! elementos. `map`, `filter` e `find` recebem callbacks `FnMut` com estado.
//! Consulte [`crate::interop`] para assinaturas importáveis por Lift.

/// Retorna o número de elementos do slice.
pub fn len<T>(values: &[T]) -> usize {
    values.len()
}

/// Informa se o slice não contém elementos.
pub fn is_empty<T>(values: &[T]) -> bool {
    values.is_empty()
}

/// Verifica se algum elemento é igual a `value`, usando [`PartialEq`].
pub fn contains<T: PartialEq>(values: &[T], value: &T) -> bool {
    values.contains(value)
}

/// Mapeia elementos emprestados para um novo vetor, na ordem do slice.
///
/// O vetor retornado pode conter valores próprios ou referências vinculadas ao slice de entrada.
pub fn map<'a, T, U, F>(values: &'a [T], mapper: &mut F) -> Vec<U>
where
    F: FnMut(&'a T) -> U,
{
    values.iter().map(mapper).collect()
}

/// Retorna referências aos elementos que satisfazem o predicado, em ordem.
///
/// Os elementos não são clonados. As referências retornadas permanecem
/// vinculadas ao empréstimo do slice, independentemente da vida da closure.
pub fn filter<'a, T, F>(values: &'a [T], predicate: &mut F) -> Vec<&'a T>
where
    F: FnMut(&T) -> bool,
{
    values.iter().filter(|value| predicate(*value)).collect()
}

/// Retorna uma referência ao primeiro elemento que satisfaz o predicado.
///
/// Interrompe a busca no primeiro resultado e retorna `None` sem correspondência.
pub fn find<'a, T, F>(values: &'a [T], predicate: &mut F) -> Option<&'a T>
where
    F: FnMut(&T) -> bool,
{
    values.iter().find(|value| predicate(*value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_length_emptiness_and_membership() {
        let values = [1, 2, 2];
        assert_eq!(len(&values), 3);
        assert!(!is_empty(&values));
        assert!(contains(&values, &2));
        assert!(!contains(&values, &3));
        assert_eq!(len::<i32>(&[]), 0);
        assert!(is_empty::<i32>(&[]));
        assert!(!contains(&[], &1));
    }

    #[test]
    fn map_preserves_order_and_accepts_stateful_closures() {
        let values = [String::from("a"), String::from("bb")];
        let mut calls = 0;
        let mapped = map(&values, &mut |value| {
            calls += 1;
            value.len() + calls
        });
        assert_eq!(mapped, [2, 4]);
        assert_eq!(calls, 2);
        assert_eq!(values[0], "a");
    }

    #[test]
    fn map_can_return_references_borrowed_from_the_input_slice() {
        let values = [String::from("first"), String::from("second")];
        let views = {
            let mut mapper = String::as_str;
            map(&values, &mut mapper)
        };

        assert_eq!(views, ["first", "second"]);
        assert_eq!(values[0], "first");
    }

    #[test]
    fn filter_borrows_non_clone_elements_in_order() {
        #[derive(PartialEq, Debug)]
        struct Item(i32);

        let values = [Item(3), Item(1), Item(3)];
        let filtered = filter(&values, &mut |value| value.0 == 3);
        assert_eq!(filtered, [&values[0], &values[2]]);
        assert!(std::ptr::eq(filtered[0], &values[0]));
        assert!(std::ptr::eq(filtered[1], &values[2]));
    }

    #[test]
    fn find_stops_at_first_match_and_borrows_the_original() {
        let values = [String::from("a"), String::from("bb"), String::from("cc")];
        let mut calls = 0;
        let found = find(&values, &mut |value| {
            calls += 1;
            value.len() == 2
        });
        assert!(std::ptr::eq(found.unwrap(), &values[1]));
        assert_eq!(calls, 2);
        assert_eq!(find(&values, &mut |value| value.is_empty()), None);
    }

    #[test]
    fn empty_slices_never_call_callbacks() {
        let values: [i32; 0] = [];
        let mut mapper = |_: &i32| -> i32 { panic!("empty map") };
        let mut predicate = |_: &i32| -> bool { panic!("empty predicate") };
        assert!(map(&values, &mut mapper).is_empty());
        assert!(filter(&values, &mut predicate).is_empty());
        assert_eq!(find(&values, &mut predicate), None);
    }
}
