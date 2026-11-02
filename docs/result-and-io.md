# Result and UTF-8 I/O in v0.1

[Português do Brasil](result-and-io.pt-BR.md) · [User guide](../README.md)

`Result<T, E>` is a special built-in type, lowered to Rust `std::result::Result<T, E>`. It is accepted in variable annotations, interface fields, arrays, and parameters/returns of Lift functions. Nested Result types are accepted in Lift. This does not add general generic types.

`void` is accepted as the success type (`Result<void, string>` → `Result<(), String>`). It remains invalid as an error type, array element, standalone variable, or ordinary parameter. Direct struct cycles through Result are rejected, just like direct struct fields; an array indirection still breaks that size cycle.

## Handle a result

```lift
import function read_file(path: string): Result<string, string> from "lift_core::io::read_file";

const contents: Result<string, string> = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

`result.is_ok(value)` borrows a Result and returns `boolean`. `result.unwrap(value)` returns an owned copy of the success payload; `result.unwrap_err(value)` returns an owned copy of the error payload. The payload is cloned, so the original Result remains available. A `Result<void, E>` success unwrap is a void expression and can be used as a statement. Each argument expression is evaluated once.

Unwrapping the wrong variant panics. The compiler does not prove that a branch has checked the variant; callers must use `is_ok` appropriately, or use the `?`, `throw`, and `try`/`catch` constructs. The `result` namespace must not be shadowed. Result values can now be constructed with `result.ok(value)`/`result.err(error)`; the propagation operator `?`, `throw`, and a typed `try`/`catch` are implemented with exact error-type matching and no implicit `From` conversion. Union/enum matching and general generics remain unimplemented; `Option`/`Result` can be matched exhaustively (see [matching Option and Result](match.md)). Optional types and `??` follow [the separate Option contract](option.md); the error path is documented in [errors and propagation](errors.md).

## FFI contract

An import may return `Result<T, E>` when `T` is a supported primitive, shallow array, external struct, or `void`, and `E` is a supported primitive, shallow array, or external struct; it may also take `Result<T, E>` as a borrowed parameter (`&Result<...>`). Payloads use the owned mapping and may contain one direct `Option` (`Result<int?, string>` → `Result<Option<i64>, String>`). Nested Result, `Option<Result<T, E>>`, Option inside an array, and `Result<void, E>` parameters are not supported, and `void` success is return-only. The generated safe function-pointer wrapper makes Cargo/rustc check the complete Rust signature. See [external structs and Option in Rust imports](ffi-structs-and-options.md) for the struct layout declaration, direct Option parameters/returns, and the exact nesting matrix.

## Library functions

| Import target | Lift return | Behavior |
| --- | --- | --- |
| `lift_core::io::arguments` | `Result<string[], string>` | Exclude argv[0]; preserve order, empty arguments, flags, and spaces. Invalid Unicode is an error. |
| `lift_core::io::read_stdin` | `Result<string, string>` | Read until EOF; preserve line endings; reject invalid UTF-8. |
| `lift_core::io::read_file` | `Result<string, string>` | Read an entire UTF-8 file. |
| `lift_core::io::write_file` | `Result<void, string>` | Create or truncate a UTF-8 file; do not create parent directories. |

File paths are strings. Relative paths resolve from the caller's current working directory; `lift run` preserves it. Error strings include the operation, path when applicable, and operating-system/UTF-8 error. There is no structured error code in this increment. Writes are not atomic and may leave a partial file after failure. The bindings return errors rather than terminating the process; unwrap on the wrong variant and the existing console binding can still panic.

Use `lift run app.lift -- --flag "a b"` to forward flags and quoted arguments. The compiler does not consume piped stdin while compiling. See `examples/io.lift` for handled read/write errors. Unit and CLI tests cover success, empty/multiline stdin, argument forwarding, Unicode files, truncation, missing files/directories, invalid UTF-8, and explicit unwrap misuse.
