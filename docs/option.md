# Optional values and lazy coalescing

[Português do Brasil](option.pt-BR.md) · [User guide](../README.md)

Lift spells an optional type as `T?` and lowers it to Rust `Option<T>`. `null` is a reserved literal for the single absent variant (`None`), only where the payload type can be inferred. There is no `undefined` absence value, implicit `T`→`T?` conversion, or general `Option<T>` generic syntax in Lift.

```lift
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9); // 7
console.log(option.some(false) ?? true); // false
```

`option.some(expr)` constructs an owned present payload. `option.is_some(value)` borrows an optional and returns `boolean`. `option.unwrap(value)` borrows it and clones the payload, preserving the original; unwrapping `None` panics explicitly. The `option` builtin namespace must be free of variables, functions, and interfaces with that name.

## Types and required fields

| Lift type | Rust type | Meaning |
| --- | --- | --- |
| `int?` | `Option<i64>` | Optional integer. |
| `int?[]` | `Vec<Option<i64>>` | Array of optional integers. |
| `int[]?` | `Option<Vec<i64>>` | Optional array of integers. |

Existing non-void value types can be payloads. `void?`, directly nested optionals (`T??` or `option.some(optional_value)`), and direct recursive size cycles such as `interface Node { next: Node? }` are rejected. An array still provides indirection, so `Node?[]` does not create that size cycle.

A field declared `name: string?` is still required in every object literal. Write `name: null` or `name: option.some("Ada")`. `name?: string` and omitted optional fields are not implemented. `const value: int? = 1` is an error; use `option.some(1)`.

## Coalescing contract

- `T? ?? T` returns `T`.
- `T? ?? T?` returns `T?`, enabling `a ?? b ?? fallback` chains.
- The left expression executes exactly once. The right expression executes only for `None`.
- `Some(false)`, `Some(0)`, and `Some("")` retain their payload; this is absence checking, not truthiness.
- `??` is left-associative and has lower precedence than `||`. Mixing `??` with `&&`/`||` without parentheses is a syntax error, following the TS-style restriction. Use `(value ?? false) || other` or `value ?? (first && second)`.

`null` uses an expected optional type from an annotation, field, function argument/return, or optional equality operand. `null ?? 42` can infer the payload from its immediate fallback. Bare `const value = null` and uncontextual `null ?? null` are errors. Inference remains local: expected context does not travel through a non-null left operand of another coalesce, so `const value: int = null ?? null ?? 1` is rejected. Annotate an intermediate optional instead.

There is no flow narrowing: after `if option.is_some(value)`, `value` still has type `T?`. Extract with unwrap or `??`. Equality against contextual `null` does not narrow it either. There is no optional chaining, pattern matching, automatic error propagation, or Option support in the FFI. FFI parameters, returns, and Option inside arrays/Result are rejected before Rust generation.

## Ownership and validation

The HIR records owned `OptionSome`, typed `OptionNone`, and a dedicated `Coalesce` node with an explicit value/optional result mode. The left becomes an owned snapshot using existing copy/clone rules. Code generation emits `match` on that snapshot and places the owned fallback only in the `None` arm; it does not use a closure or rediscover ownership. The original optional remains available according to Lift's value-copy rules.

Liveness observes both possible operands conservatively and keeps their clones inside coalesce and Some in this increment. It does not treat the fallback as an unconditional runtime action or infer narrowing. Existing borrow/alias diagnostics remain in effect.

Parser, semantic, codegen, and CLI tests cover suffix ordering, types, required fields, null inference, FFI rejection, recursive types, explicit unwrap failure, and lazy/exactly-once effects. `examples/option.lift` mutates counters to distinguish evaluated paths and verifies false/zero/empty-string payloads through a native binary.
