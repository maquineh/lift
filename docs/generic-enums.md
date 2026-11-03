# Generic enums

[Português do Brasil](generic-enums.pt-BR.md) · [User guide](../README.md)

Generic enums are monomorphized per concrete application, just like generic interfaces. `Maybe<int>` becomes its own nominal enum with `T` replaced by `int`.

```lift
enum Maybe<T> { Some(T), None }
enum List<T> { Nil, Cons(T, List<T>) }

const some: Maybe<int> = Maybe.Some(5);
const values: List<int> = List.Cons(1, List.Cons(2, List.Nil));
```

## Construction and matching

- Declare with `enum Name<T, ...> { ... }`; variants may use positional or named payloads containing `T`.
- Apply as `Name<Arg>`, in annotations, parameters/returns, fields, arrays, Option/Result, and nested containers.
- Construction uses the template path (`Maybe.Some(5)`, `Maybe.None`) and requires an **expected type** (`const value: Maybe<int> = ...`, function argument, return, or field). The expected enum instance disambiguates the monomorphized variant. Without an expected type, construction is rejected.
- `match` uses the template path (`Maybe.Some(x)` / `Maybe.None`) and the scrutinee's concrete enum type; arms are exhaustive over the monomorphized variants.
- Each instantiation is distinct (`Maybe<int>` vs `Maybe<string>`), with structural equality, clone, and value semantics.
- Recursive generic enums are supported through the existing automatic `Box` fields; named-field variants and containers of generic enums work.

## Code generation and limits

The pre-semantic monomorphizer rewrites applications to synthetic enum names (`Maybe<int>`, `List<string>`) and emits one concrete Rust enum per instantiation. There is no Rust generic enum in the output.

Limits:

- Construction requires an expected type; there is no inference of `T` from constructor arguments alone.
- No generic methods/`impl`, explicit type arguments, generic functions over `Maybe<T>`/`Box<T>`, generic enums in FFI/imports, or generic enum templates without arguments.
- Direct recursive payloads are boxed automatically; arrays use `Vec` indirection.

Tests: `crates/lift_sema/tests/generic_enums_contract.rs`; example [`examples/generic-enums.lift`](../examples/generic-enums.lift).
