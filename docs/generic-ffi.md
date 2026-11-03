# Generic external layouts (FFI)

[Português do Brasil](generic-ffi.pt-BR.md) · [User guide](../README.md)

Imported interfaces can be generic. Each concrete application becomes an external layout with a monomorphized Rust path:

```lift
import interface Wrapper<T> { value: T } from "lift_core::collections::Wrapper";
import function wrap_i64(value: int): Wrapper<int> from "lift_core::collections::wrap_i64";
import function unwrap_i64(wrapper: Wrapper<int>): int from "lift_core::collections::unwrap_i64";
import function wrap_string(value: string): Wrapper<string> from "lift_core::collections::wrap_string";
import function unwrap_string(wrapper: Wrapper<string>): string from "lift_core::collections::unwrap_string";

const wrapped_number: Wrapper<int> = wrap_i64(41);
console.log(unwrap_i64(wrapped_number)); // 41

const wrapped_text: Wrapper<string> = wrap_string("hi");
console.log(unwrap_string(wrapped_text)); // hi
```

## How it works

- `import interface Name<T> { ... } from "crate::path"` declares a generic external template. `Name<Arg>` applications become synthetic external layout declarations (`Wrapper<int>`, `Wrapper<string>`) with fields substituted and Rust path `crate::path<i64>`/`crate::path<String>`.
- Type arguments currently map to Rust primitives/string only: `int -> i64`, `number -> f64`, `string -> String`, `boolean -> bool`. Arrays, containers, options, and nested generics are rejected as external type arguments.
- The resulting layouts participate in the normal FFI rules: external structs can be parameters/returns, fields are asserted by codegen, and functions are resolved through the Rust dependency.
- The generic import template is removed after instantiation; `Wrapper` without arguments is unknown. Each application is nominal and cached.
- `lift_core::collections::Wrapper<T>` plus `wrap_i64`/`unwrap_i64`/`wrap_string`/`unwrap_string` provide a runnable example.

## Limits

- Only primitive/string/boolean type arguments; no `Wrapper<int[]>`/`Box<int>` as external generic arguments.
- No construction of external generic layouts in Lift (they cross the FFI), no generic FFI functions, no generic impls on external layouts, and no native Map API yet.
- External generic layouts remain subject to the usual FFI restrictions (Optional/Result shapes, ownership, callbacks).

Tests: `crates/lift_sema/tests/generic_ffi_contract.rs`; example [`examples/generic-ffi.lift`](../examples/generic-ffi.lift).
