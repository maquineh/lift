# External structs and Option in Rust imports

[Português do Brasil](ffi-structs-and-options.pt-BR.md) · [User guide](../README.md)

Lift declares the layout of a Rust struct with `import interface`. The declaration is a contract: the type must exist at the given Rust path, its fields must be public with exactly the mapped names and types, and it must implement `Clone`, `Debug`, and `PartialEq`. The generated Rust contains checks that `rustc` validates during `lift check`, `build`, or `run`.

```lift
import interface IntRange {
    start: int,
    end: int,
} from "lift_core::interop::IntRange";

import function int_range(start: int, end: int): IntRange from "lift_core::interop::int_range";
import function range_len(range: IntRange): int from "lift_core::interop::range_len";
import function range_len_or(range: IntRange?, fallback: int): int from "lift_core::interop::range_len_or";

const range = int_range(3, 8);
console.log(range.start);
console.log(range_len_or(null, -1));
```

## Declaration and field mapping

```text
import interface Name { field: Type, optional?: Type } from "pacote@MAJOR.MINOR.PATCH::modulo::Name";
```

Local crates use `lift_core::...`, `lift_db::...`, or `lift_web::...` as the package part. The destination follows the same rules as function imports: ASCII Rust path segments, no keywords, no raw identifiers, and an exact three-part version for registry crates. Names participate in the same duplicate checks as local interfaces and functions, and declaring the same Rust crate with two different versions is rejected.

| Lift field type | Rust field type |
| --- | --- |
| `int` | `i64` |
| `number` | `f64` |
| `boolean` | `bool` |
| `string` | `String` |
| `T[]` (`T` scalar, `string`, or an external struct) | `Vec<T>` |
| external struct `P` | the declared Rust path |
| `field: T?` | `Option<T>`, still required in object literals |
| `field?: T` | `Option<T>`, omittable in object literals; omission becomes `None` |

Field names are used verbatim as Rust field names, so they must be Rust identifiers without reserved words. `Result`, local Lift structs, resources, nested arrays, and nested optionals are rejected as external field types. Unlike local interfaces, an external layout is opaque for the direct size-cycle analysis: the crate owns its own layout, and indirection rules are the crate's responsibility.

The generated project emits two checks per external struct: a layout function that clones every declared field into its mapped type, and a trait bound assertion for `Clone + Debug + PartialEq`. A missing field, private field, renamed field, different field type, missing trait, or non-constructible type (`#[non_exhaustive]`) makes `rustc` reject the generated Rust. Those are Rust diagnostics over generated code; Lift does not remap them to the source. A `lift.toml`/Cargo dependency is added from the `from` destination even when no imported function uses the struct.

## Using external structs in Lift

External structs are ordinary nominal Lift value types:

```lift
const chosen: IntRange = { start: 10, end: 14 };
console.log(chosen.start);
console.log(chosen == int_range(10, 14));
const ranges: IntRange[] = [chosen];
const maybe: IntRange? = option.some(chosen);
```

Object literals initialize every declared field in generated Rust, including omitted optional fields as `None`. The declared layout must match the Rust struct exactly because construction lists every field; extra fields in the Rust type also make `rustc` report a missing-field error. `impl` methods may take an external struct as `this`, with the usual `BorrowImm`/`BorrowMut` inference; mutating a public field writes the actual Rust value.

## Function mapping

Parameters are borrowed according to the existing FFI conventions; returns are owned.

| Lift type | Rust parameter | Rust return |
| --- | --- | --- |
| `int`, `number`, `boolean` | `i64`, `f64`, `bool` | same |
| `string` | `&str` | `String` |
| `T[]` | `&[T]` | `Vec<T>` |
| external struct `P` | `&P` | `P` |
| `T?` | `Option<&str>`, `Option<&[T]>`, `Option<&P>`, or `Option<scalar>` | `Option<String>`, `Option<Vec<T>>`, `Option<P>`, or `Option<scalar>` |
| `Result<T, E>` | `&Result<mapped T, mapped E>` | `Result<mapped T, mapped E>`, with `void` success as `()` |
| `(T) => R` | `&dyn Fn(owned mapped T...) -> owned mapped R` | `Box<dyn Fn(owned mapped T...) -> owned mapped R>`, converted to `Rc` |

`T?` is accepted directly in a parameter or return position: `Option<i64>`, `Option<String>`, `Option<Vec<T>>`, and `Option<P>` for an external struct. The generated wrapper keeps the Lift-owned convention (`&Option<String>`, `&Option<Vec<T>>`, `&Option<P>` for borrowed parameters) and converts to the declared Rust signature with `as_deref`, `as_ref`, or `*`, so `Option<&str>`, `Option<&[T]>`, and `Option<&P>` work without adapter code.

`Result<T, E>` is accepted as a borrowed parameter (`&Result<...>`) and as an owned return. Its payloads use the owned mapping (`String`, `Vec<T>`, `P`) and may contain one direct `Option`: `Result<int?, string>` maps to `Result<Option<i64>, String>`, and `Result<string, string?>` maps to `Result<String, Option<String>>`. `void` is still only a success type in returns; `Result<void, E>` is rejected as a parameter. The nesting is deliberately shallow: one `Option` inside a `Result` payload, and no `Result` inside another payload.

A function type is a callback: the parameter maps to `&dyn Fn(...)` with owned payload types, and a callback return maps to `Box<dyn Fn(...)>` (the wrapper converts it to `Rc`). Callback signatures may use scalars, `string`, shallow arrays, external structs, and `void` returns. Callbacks are invoked synchronously through the safe function pointer; this is a Rust trait-object contract, not a C function pointer. See [function types and callbacks](higher-order.md) for the language-side contract.

The following remain rejected before code generation:

- `Option` nested inside another `Option`, an array, or a `Result` payload that is itself nested. Only a direct `T?` parameter or return and one direct `Option` inside a `Result` payload are supported.
- Nested `Result` payloads (`Result<Result<T, E>, E>`), `Option<Result<T, E>>` (`Result<T, E>?`), and `Result` inside an array.
- `Result<void, E>` as a parameter; `void` succeeds only in return position.
- Lift-local structs (declared with plain `interface`) in any FFI position, including inside `Option`, `Result`, or arrays. Only `import interface` types cross the FFI.
- Callbacks containing `Option`, `Result`, another function type, arrays of callbacks, or opaque resources; a `void` callback parameter; and function types in other FFI payload positions (only a direct parameter/return is a callback).
- Nested arrays, unions/enums, generics, `unsafe` functions, and raw identifiers.

Because parameters are borrowed, an external function that requires ownership of a struct cannot be imported directly; write a Rust adapter crate or a local function wrapper. Mutation through an FFI parameter is likewise not part of the contract: the safe function pointer only verifies the declared signature.

## Acceptance and limits

Parser, semantic, codegen, and CLI tests cover the `import interface` syntax, layout dependencies and conflicts, field/type validation, rejected nested options/results and local structs, generated layout/trait assertions, wrapper conversions for optional scalars, strings, arrays, and structs, borrowed `Result` parameters with optional payloads, callback parameters/returns with trait-object bridges, and native execution through `examples/interop.lift` and `examples/higher-order.lift`. Passing them demonstrates the exercised layouts and signatures, not compatibility with arbitrary Rust types: field order is irrelevant, but a `#[non_exhaustive]` struct, private fields, renamed fields, or a different Rust field type are rejected by `rustc` rather than adapted by Lift.

There is no automatic serialization, field renaming, enum mapping, generic substitution, or Option adaptation beyond the conversions above. Layouts are named-field structs only: tuple structs, enums, unions, and generic types with parameters cannot be spelled in an `import interface` declaration.
