//! Base library with a native Rust API and no external dependencies.
//!
//! Provides [`Console`], [`io`], slice and string helpers, and [`interop`] adapters for
//! explicit `.truxt` imports. Only signatures supported by the compiler's FFI
//! type mapping can be imported into Truxt.
//!
//! PT-BR: Biblioteca base com API Rust nativa e sem dependências externas.
//! Disponibiliza [`Console`], [`io`], helpers para slices e strings e adaptadores em
//! [`interop`] para imports explícitos em `.truxt`. Somente assinaturas
//! compatíveis com o mapeamento de tipos FFI do compilador podem ser importadas.

pub mod array;
mod console;
pub mod interop;
pub mod io;
pub mod string;

pub use console::Console;
