# Result construction, propagation, and `try`/`catch`

[Português do Brasil](errors.pt-BR.md) · [User guide](../README.md)

Lift already exposes a built-in `Result<T, E>` type for fallible operations. This increment adds the error path itself: constructors, the postfix `?` propagation operator, `throw`, and a typed `try`/`catch` statement that maps to Rust control flow without panics.

```lift
import function maybe_parse(text: string): Result<int?, string> from "lift_core::interop::maybe_parse";

function parse_positive(text: string): Result<int, string> {
    const parsed = maybe_parse(text)?;   // propagate Err to the caller
    const value = parsed ?? 0;
    if value <= 0 { throw "not positive"; } // constructs Err("not positive")
    return result.ok(value);                // constructs Ok(value)
}

function describe(text: string): string {
    try {
        const value = parse_positive(text)?; // targets the catch below
        return `ok ${value}`;
    } catch (error: string) {
        return `error ${error}`;
    }
}
```

## Constructors

`result.ok(value)` and `result.err(error)` require a contextual `Result<T, E>`: an annotation, a `return`, a function argument, or another checked position. There is no inference from a bare constructor, because only the expected type determines the other payload type.

```lift
const value: Result<int, string> = result.ok(1);
const failure: Result<int, string> = result.err("bad");
function nothing(): Result<void, string> { return result.ok(); }
```

- `result.ok(value)` checks `value` against `T` and produces `Ok(value)`; the payload is materialized as an owned Lift value.
- `result.ok()` with no argument is only valid when `T` is `void`, and produces `Ok(())`.
- `result.err(error)` checks `error` against `E` and produces `Err(error)`.
- Arity and payload types are checked before code generation. `result` must not be shadowed by a variable, function, or interface.
- `result.ok`/`result.err` are constructors, not function values. A constructor without context, including one used directly under `?`, is rejected; annotate the value first.

## `?` propagation

`expression?` consumes a `Result<T, E>` and produces `T` on success. On `Err(e)` it transfers control to the nearest compatible handler:

1. the innermost enclosing `try` whose catch type is exactly `E`, or
2. the enclosing function when its declared return type is `Result<_, E>` with exactly that error type, or
3. a Lift diagnostic (`sema.InvalidPropagation`).

The error types must be equal; there is no implicit `From` conversion. `?` is Result-only — it is not TypeScript optional chaining, and `Option<T>?` is rejected.

Ownership follows Lift value semantics: a `Result` place is cloned before propagation when its payloads are cloneable, so the original remains available; a `Result` containing an opaque resource is moved instead, because resources are affine. A call result is used directly, without an extra clone.

```lift
function total(text: string): Result<int, string> {
    const first = parse_positive(text)?;
    const second = parse_positive(text)?; // `first` is still available
    return result.ok(first + second);
}
```

Inside a `try`, `?` targets the catch when the error type matches; otherwise it falls through to the enclosing function if that error type matches. This means a `try` with a different catch type does not silently swallow an unrelated error.

## `throw`

`throw value;` is a diverging statement that produces `Err(value)` for the same targets as `?`: the nearest `try` with the matching catch type, otherwise the function's `Result<_, E>` return. A `throw` with no compatible target, or with a value type different from the target error type, is rejected before code generation. `throw` requires a trailing semicolon.

`throw` and `?` never create panics. `result.unwrap`/`result.unwrap_err` remain the explicit panicking operations.

## `try` / `catch`

```text
try { ... } catch (error: E) { ... }
```

- The catch parameter type is explicit and must be a value type (not `void`).
- The two blocks have separate scopes. Lift has no shadowing, so the catch name cannot reuse a visible name; a duplicate is a regular declaration error.
- The catch binding behaves like an owned parameter: it can have its fields mutated (the generated Rust pattern uses `mut`), but the binding itself cannot be reassigned directly.
- A successful `try` body continues after the `try`/`catch` statement. Any `throw` or matching `?` inside the body transfers to the catch block; after the catch block finishes, execution continues after the statement.
- `return` inside the `try` body returns from the function and skips the catch, as in JavaScript.
- `throw`/`?` inside the catch block target the next outer `try` or the function return, not the same catch.
- There is no `finally`, no catch-all without a type, no multiple catches, and no `try` expression value; `try` is a statement.
- Nested `try` blocks are supported; each `?`/`throw` targets the innermost compatible catch.

The lowering uses a labeled Rust block whose value is `Result<(), E>`: `?`/`throw` inside the body `break` with `Err`, and a `match` dispatches to the catch. When the semantic analysis proves that the whole statement returns from the function, the code generation closes the divergence with `unreachable!()` so the generated Rust satisfies the declared return type.

## Return guarantees

A function returning `Result<T, E>` must still return on every path. A `try`/`catch` statement guarantees a return when its body always exits (returns or throws) and the catch block returns; a body that always returns does not need a returning catch because the catch is unreachable. The usual `if`/`else` rules apply inside both blocks.

## Ownership and liveness

- `?` clones a cloneable `Result` place and moves a resource result; the HIR records the clone/move explicitly, so code generation does not repeat ownership analysis.
- The private liveness pass treats each `try`/`catch` as one conservative point: uses inside both blocks count, but clones inside them are not converted to moves in this increment. Correctness does not depend on that optimization.
- Existing borrow, alias, and affine-resource diagnostics remain active. Resource consumption inside a `try` body is propagated conservatively to the catch and to the continuation.

## Limits

- No exceptions or stack unwinding: `throw` is `Err` construction and `try`/`catch` is a Result match.
- No `?` on `Option`, no `From` error conversion, no `finally`, no multiple/typed catch arms, no pattern matching beyond the `Option`/`Result` `match`, and no `try` expression value.
- Constructors require contextual `Result<T, E>`; a direct `result.ok(1)?` is rejected until the value is annotated.
- The success payload is `void`-only for `result.ok()`; `result.err()` always requires an error value.
- The catch type must exactly match the propagated error; unrelated errors propagate to outer handlers or the function return instead of being captured.

Parser, semantic, codegen, and CLI tests cover constructors, context requirements, propagation targets, error-type mismatches, throw, catch mutability, nested `try`, return guarantees, resource moves vs clones, function-level propagation, and native execution of [`examples/errors.lift`](../examples/errors.lift).
