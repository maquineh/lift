# `lift_core`

Rust/Cargo **1.94.1+** (MSRV: `1.94.1`).

`lift_core` is the Rust-native base library for Lift. It has no external dependencies and provides console output plus array and string helpers.

## Use from Lift

Lift imports declare their local name and complete type signature. The `interop` module contains adapters whose Rust signatures match the current safe FFI contract:

```lift
import function length(values: int[]): int from "lift_core::interop::array_len";
import function trim(value: string): string from "lift_core::interop::string_trim";
import function contains(values: int[], value: int): boolean from "lift_core::interop::array_contains_int";
import function has_text(value: string, part: string): boolean from "lift_core::string::contains";

const values: int[] = [2, 4, 6];
console.log(length(values));
console.log(contains(values, 4));
console.log(trim("  Lift  "));
console.log(has_text("Hello, Lift", "Lift"));
```

Available adapters:

| Rust path | Lift signature | Behavior |
| --- | --- | --- |
| `interop::array_len` | `T[] -> int` | Return the number of elements as `i64`. |
| `interop::string_len` | `string -> int` | Count Unicode scalar values (`char`), not bytes or grapheme clusters. |
| `interop::string_trim` | `string -> string` | Trim Unicode whitespace and return an owned string. |
| `interop::array_contains_int` | `(int[], int) -> boolean` | Check exact `i64` membership. |
| `interop::array_contains_number` | `(number[], number) -> boolean` | Check exact `f64` equality. |
| `interop::array_contains_boolean` | `(boolean[], boolean) -> boolean` | Check boolean membership. |
| `interop::array_contains_string` | `(string[], string) -> boolean` | Check exact string membership. |

`array::is_empty`, `string::is_empty`, `string::contains`, and `string::concat` also fit the FFI contract. The CLI embeds these sources and extracts them into the generated project's cache. Its executable can be moved without retaining the source checkout. Rust/Cargo and a host linker are still required.

## Arguments, stdin, and files

```lift
import function arguments(): Result<string[], string> from "lift_core::io::arguments";
import function read_stdin(): Result<string, string> from "lift_core::io::read_stdin";
import function read_file(path: string): Result<string, string> from "lift_core::io::read_file";
import function write_file(path: string, contents: string): Result<void, string> from "lift_core::io::write_file";

const text = read_file("input.txt");
if result.is_ok(text) {
    console.log(result.unwrap(text));
} else {
    console.log(result.unwrap_err(text));
}
```

Arguments exclude argv[0] and must be valid Unicode. Standard input is read as UTF-8 until EOF with unchanged line endings. Files use UTF-8 text; writing creates or truncates without creating parent directories. Relative paths use the caller's current working directory. Errors are strings containing the operation, path when relevant, and underlying error. Writes are not atomic.

`result.is_ok` borrows a result; `result.unwrap` and `result.unwrap_err` clone the payload. Unwrapping the wrong variant panics. See [Result and I/O](../../docs/result-and-io.md) for limits and examples. I/O errors are recoverable through Result; `console.log` still panics on stdout failure.

## Rust API

The Rust API is broader than the Lift FFI surface:

- `Console::log` and `Console::error` write a `Display` value followed by a newline and return `std::io::Result<()>`.
- `array` provides `len`, `is_empty`, `contains`, `map`, `filter`, and `find` over borrowed slices.
- `string` provides `len`, `is_empty`, `contains`, `concat`, and `trim`.
- `io` provides `arguments`, `read_stdin`, `read_file`, and `write_file`, all returning `Result<_, String>`.
- `map`, `filter`, and `find` accept `FnMut` callbacks. Lift's local snapshot closures have no higher-order function type or FFI mapping, so these helpers remain Rust-only.

`string::len` counts Unicode scalar values; for example, `"é🙂"` has length 2. `string::trim` returns a borrowed `&str` in Rust. The FFI adapter `interop::string_trim` returns an owned `String` to match Lift's current string-return contract.

Run this crate's tests from the workspace root with `cargo test -p lift_core`.
