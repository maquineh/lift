# Matching Option and Result

[Português do Brasil](match.pt-BR.md) · [User guide](../README.md)

`match` inspects an `Option<T>`, a `Result<T, E>`, or a nominal enum and runs exactly one arm. For `Option`/`Result` both variants must be present, and for an enum every variant must be present, so an accepted `match` is exhaustive.

```lift
function describe(value: int?): string {
    match value {
        some(present) => { return `some ${present}`; }
        none => { return "none"; }
    }
}

function describe_result(value: Result<int, string>): string {
    match value {
        ok(number) => { return `ok ${number}`; }
        err(error) => { return `err ${error}`; }
    }
}
```

`Result<void, E>` uses `ok()` without a binding:

```lift
function make_void(): Result<void, string> { return result.ok(); }

function describe_void(): string {
    match make_void() {
        ok() => { return "void ok"; }
        err(error) => { return `err ${error}`; }
    }
}
```

## Syntax and patterns

```text
match expressão {
    some(nome) => { ... }
    none => { ... }
}
```

```text
match expressão {
    ok(nome) => { ... }   // ou ok() para sucesso void
    err(nome) => { ... }
}
```

```text
match shape {
    Shape.Circle(radius) => { ... }
    Shape.Rect(width, height) => { ... }
    Shape.Empty => { ... }
}
```

- Arms use `=>` followed by a block. Arms are separated by whitespace or an optional comma; there is no fallthrough.
- `some(nome)` and `err(nome)` require a binding name; `none` has no parentheses.
- `ok(nome)` binds the success payload, and `ok()` is only valid when the success type is `void` (as in `Result<void, E>`). Using the wrong form is rejected.
- The binding is a new immutable value scoped to that arm. Its fields can be mutated (`mut` is emitted in the generated pattern), but the binding itself cannot be reassigned. Prefix the name with `_` if the payload is intentionally ignored.
- Lift has no wildcard, guard, literal, nested, or multiple patterns here; exactly one `some` and one `none`, or one `ok` and one `err`, is required. Duplicate or missing arms are errors (`sema.InvalidMatch`).
- Enum patterns are qualified and bind payloads positionally; the binding count must match the variant. `match` is a statement; arms produce no value. Use `return` or assign inside an arm.

## Evaluation and value semantics

- The scrutinee is evaluated once. If it is a place, it is cloned before matching, so the original optional/result/enum remains available and later mutations are not observed by the match.
- The payload is an owned value taken from that clone; it is not borrowed.
- Matching a `Result<T, E>` that contains an opaque resource is rejected: use `?` or `unwrap`/`unwrap_err` for resource results. Arrays of closures/resources remain unavailable as payloads by their own contracts.
- The generated Rust is a `match` over `Option`/`Result` with the arm patterns; the binding uses the same owned-value rules as other Lift bindings.

## Flow control

- A `match` guarantees that the function returns when **every** arm always returns or throws; a function can then omit a trailing `return`. If any arm falls through, normal return-path checking applies.
- `return` inside an arm returns from the function. `throw` and `?` inside an arm target the enclosing `try` or the function's `Result<_, E>` return, as usual.
- `break`/`continue` inside an arm target the enclosing loop (the loop labels make this work across `try` blocks); `match` itself does not create a loop.
- `match` may appear in a loop, `try` body/catch block, nested arm, or closure body. Inside a closure, `break`/`continue` remain invalid and `return` returns from the closure.

## Errors

Rejected before code generation:

- Missing or duplicate variants (`sema.InvalidMatch`), or patterns from the wrong family (`ok` on `Option`, `some` on `Result`).
- `ok()` on a non-`void` success, or `ok(nome)` on `void`.
- Matching a value that is neither `Option<T>` nor `Result<T, E>`.
- Arm binding shadowing an active name.
- Reassigning the arm binding itself (fields remain mutable).
- A function path that still lacks a return after a non-returning match.
- Matching a `Result`/`Option` containing an opaque resource.

## Limits

- Enum arms must cover every variant with a qualified `Enum.Variant(...)` pattern; the enum name and binding count are checked. Enums are described in [the enum contract](enums.md).
- `Option`, `Result`, and nominal enums can be matched; there is no primitive matching, wildcard, guard, nested pattern, or TypeScript `type A = B | C` union syntax yet.
- No `match` expression value, guards, `or` patterns, nested payload patterns, or `_` wildcard arm (ignore a payload with a `_`-prefixed binding instead).
- No resource payload matching, no mutation of the binding itself, and no `match` as a constant/initializer expression.

Parser, semantic, codegen, and CLI tests cover exhaustiveness for `Option`/`Result`/enums, duplicate/incompatible patterns, `void` success, clone semantics, field mutation, return guarantees, loop control, throw in arms, and native execution of [`examples/match.lift`](../examples/match.lift) and [`examples/enums.lift`](../examples/enums.lift).
