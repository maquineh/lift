# Explicit type arguments

[Português do Brasil](explicit-type-arguments.pt-BR.md) · [User guide](../README.md)

Generic functions normally infer type parameters from their arguments. When inference is ambiguous or documentation benefits from being explicit, use the `::<...>` suffix on the callee:

```lift
function identity<T>(value: T): T { return value; }
function first<T>(values: T[]): T { return values[0]; }

console.log(identity::<int>(41));
console.log(identity::<string>("hi"));
console.log(first::<int>([1, 2]));

interface Box<T> { value: T }
const boxes: Box<int>[] = [{ value: 7 }];
const boxed = first::<Box<int>>(boxes);
```

## Rules

- The suffix belongs to the called function: `f::<T>(args)`. It is only valid directly before a call; using `f::<T>` as a value is rejected.
- The number of type arguments must equal the function's declared type parameters. Every argument must name a valid Lift type, including containers (`Box<int>`) and generic enums.
- Explicit arguments seed inference; the actual call arguments are still checked against the substituted parameter types. A conflict (for example `f::<int>("x")`) is a type mismatch.
- Using `::<...>` on a non-generic named function is rejected.
- The generated Rust call remains an ordinary call and relies on Rust inference; the explicit arguments are a Lift-level check, not emitted as Rust turbofish.
- Explicit arguments do not bypass the generic-function rules: resources/`DbValue`/closures cannot instantiate `T`, and function values/callbacks remain unsupported for generic functions.

## Limits

- No explicit arguments on methods, constructors, closures, FFI imports, or generic containers/enums themselves.
- No partial or inferred explicit lists (`f::<_, int>`), defaults, constraints, or return-only type parameters.
- Generic functions over container parameters (`function f<T>(box: Box<T>)`) are still not supported.

Tests: `crates/lift_sema/tests/explicit_type_arguments_contract.rs`; example [`examples/explicit-type-arguments.lift`](../examples/explicit-type-arguments.lift).
