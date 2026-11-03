//! Generic external layouts and FFI helpers for Lift generic-container interop.
//!
//! PT-BR: layouts externos genéricos e helpers FFI para interop de containers.

/// Generic wrapper importable as `import interface Wrapper<T>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Wrapper<T> {
    pub value: T,
}

/// FFI: wraps an `i64` in a `Wrapper<i64>`.
pub fn wrap_i64(value: i64) -> Wrapper<i64> {
    Wrapper { value }
}

/// FFI: unwraps a `Wrapper<i64>`.
pub fn unwrap_i64(wrapper: &Wrapper<i64>) -> i64 {
    wrapper.value
}
/// FFI: wraps a `String` in a `Wrapper<String>`.
pub fn wrap_string(value: &str) -> Wrapper<String> {
    Wrapper {
        value: value.to_owned(),
    }
}

/// FFI: unwraps a `Wrapper<String>`.
pub fn unwrap_string(wrapper: &Wrapper<String>) -> String {
    wrapper.value.clone()
}
