# Result and UTF-8 I/O in v0.1

[Português do Brasil](result-and-io.pt-BR.md) · [User guide](../README.md)

`Result<T, E>` is a special built-in type, lowered to Rust `std::result::Result<T, E>`. It is accepted in variable annotations, interface fields, arrays, and parameters/returns of Truxt functions. Nested Result types are accepted in Truxt. This does not add general generic types.

`void` is accepted as the success type (`Result<void, string>` → `Result<(), String>`). It remains invalid as an error type, array element, standalone variable, or ordinary parameter. Direct struct cycles through Result are rejected, just like direct struct fields; an array indirection still breaks that size cycle.

## Handle a result

```truxt
import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";

const contents: Result<string, string> = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

`result.is_ok(value)` borrows a Result and returns `boolean`. `result.unwrap(value)` returns an owned copy of the success payload; `result.unwrap_err(value)` returns an owned copy of the error payload. The payload is cloned, so the original Result remains available. A `Result<void, E>` success unwrap is a void expression and can be used as a statement. Each argument expression is evaluated once.

Unwrapping the wrong variant panics. The compiler does not prove that a branch has checked the variant; callers must use `is_ok` appropriately. The `result` namespace must not be shadowed. Result still has no constructors: values come from imports or Truxt functions. Pattern matching, the propagation operator `?`, `try/catch`, unions, and general generics remain unimplemented. Optional types and `??` follow [the separate Option contract](option.md).

## FFI contract

An import may return `Result<T, E>` when `T` is a supported primitive, shallow primitive array, or `void`, and `E` is a supported primitive or shallow primitive array. Nested Result returns and Result parameters are not supported by the FFI. Strings in Result payloads are owned Rust `String`; arrays are owned `Vec<T>`. The generated safe function-pointer wrapper makes Cargo/rustc check the complete Rust return type.

## Library functions

| Import target | Truxt return | Behavior |
| --- | --- | --- |
| `truxt_core::io::arguments` | `Result<string[], string>` | Exclude argv[0]; preserve order, empty arguments, flags, and spaces. Invalid Unicode is an error. |
| `truxt_core::io::read_stdin` | `Result<string, string>` | Read until EOF; preserve line endings; reject invalid UTF-8. |
| `truxt_core::io::read_file` | `Result<string, string>` | Read an entire UTF-8 file. |
| `truxt_core::io::write_file` | `Result<void, string>` | Create or truncate a UTF-8 file; do not create parent directories. |

File paths are strings. Relative paths resolve from the caller's current working directory; `truxt run` preserves it. Error strings include the operation, path when applicable, and operating-system/UTF-8 error. There is no structured error code in this increment. Writes are not atomic and may leave a partial file after failure. The bindings return errors rather than terminating the process; unwrap on the wrong variant and the existing console binding can still panic.

Use `truxt run app.truxt -- --flag "a b"` to forward flags and quoted arguments. The compiler does not consume piped stdin while compiling. See `examples/io.truxt` for handled read/write errors. Unit and CLI tests cover success, empty/multiline stdin, argument forwarding, Unicode files, truncation, missing files/directories, invalid UTF-8, and explicit unwrap misuse.
