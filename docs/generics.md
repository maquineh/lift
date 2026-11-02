# Generic functions

[Português do Brasil](generics.pt-BR.md) · [User guide](../README.md)

Generic functions declare type parameters between the name and the parameter list. Type arguments are inferred at each call from the argument types; there is no explicit `f<int>(...)` syntax.

```lift
function identity<T>(value: T): T { return value; }
function same<T>(left: T, right: T): boolean { return left == right; }
function wrap<T>(value: T): T[] { return [value]; }

console.log(identity(42));
console.log(same("a", "b"));
const numbers: int[] = wrap(7);
```

## Declaration and inference

- Syntax: `function name<T, U>(...)`. Duplicate type parameters are rejected, and a type parameter must appear in at least one input parameter so every call can infer it (return-only parameters are rejected).
- `T` may be used directly, inside arrays (`T[]`), `Option` (`T?`), and `Result<T, E>` in parameters, return type, local annotations, and nested generic calls.
- At a call, each parameter type is unified with the argument type. `same(1, "x")` fails because `T` cannot be both `int` and `string`; missing inference is an error.
- Arguments must be inferable without an expected type: annotate object literals/empty arrays/nulls in a variable and pass that variable when needed.
- Generic calls may instantiate `T` with primitives, strings, arrays, `Option`, `Result`, interfaces, enums/unions, and named values. Resources, `DbValue`, and closures cannot instantiate a type parameter.

## Constraints and operations

- The generated Rust function is generic with `Clone + Debug + PartialEq` bounds on each parameter; construction, equality, and cloning are valid for all valid instantiations.
- Arithmetic, formatting/templates, field access, and other operations that require a concrete type are rejected on `T`.
- A generic function can call another generic function; the nested type argument may itself remain `T`. Recursive generic calls are allowed.
- A generic function cannot be used as a value/callback (`const f = identity`), passed through FFI, or declared as an import. There are no generic interfaces/enums/structs, generic methods/`impl`, explicit type arguments, or monomorphized specializations.
- Type parameters are scoped to the function (and nested closures); they cannot appear in module-level interface/enum/union fields.

## Code generation

Each generic function becomes a Rust generic function with mangled type-parameter names and the three trait bounds. Calls remain ordinary Rust calls and rely on Rust inference; Lift HIR stores the inferred types for ownership/pass-mode decisions.

Limits are documented in the [testing matrix](testing-and-release.md); tests live in `crates/lift_sema/tests/generics_contract.rs`, and the executable example is [`examples/generics.lift`](../examples/generics.lift).
