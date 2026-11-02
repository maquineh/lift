# Optional values, optional fields, and lazy coalescing

[Português do Brasil](option.pt-BR.md) · [User guide](../README.md)

Lift spells an optional type as `T?` and lowers it to Rust `Option<T>`. `null` is a reserved literal for the single absent variant (`None`), only where the payload type can be inferred. There is no `undefined` absence value, implicit `T`→`T?` conversion, or general `Option<T>` generic syntax in Lift.

```lift
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9); // 7
console.log(option.some(false) ?? true); // false
```

`option.some(expr)` constructs an owned present payload. `option.is_some(value)` borrows an optional and returns `boolean`. `option.unwrap(value)` borrows it and clones the payload, preserving the original; unwrapping `None` panics explicitly. The `option` builtin namespace must be free of variables, functions, and interfaces with that name.

## Types and optional fields

| Lift type | Rust type | Meaning |
| --- | --- | --- |
| `int?` | `Option<i64>` | Optional integer. |
| `int?[]` | `Vec<Option<i64>>` | Array of optional integers. |
| `int[]?` | `Option<Vec<i64>>` | Optional array of integers. |
| `User?` | `Option<User>` | Optional struct value. |

Existing non-void value types can be payloads. `void?`, directly nested optionals (`T??` or `option.some(optional_value)`), and direct recursive size cycles such as `interface Node { next: Node? }` are rejected. An array still provides indirection, so `Node?[]` does not create that size cycle.

An interface field has two independent notions: the value type `T?` and the declaration `?:`. A field declared `name: string?` is required in every object literal; write `name: null` or `name: option.some("Ada")`. A field declared `nickname?: string` has the same Rust type (`Option<String>`) but may be omitted; omission lowers to `None`, and explicit `nickname: null` or `nickname: option.some("Ada")` are also accepted. There is no implicit lift: `nickname: "Ada"` is a type error because the expected type is `string?`. `name?: string?` is rejected as a nested optional, and `name?: void` is rejected because `void` is not a value type.

Optional fields participate in the same operations as any `T?` value: `??`, `== null`/`!= null`, `option.is_some`, and `option.unwrap`. Struct literals always initialize every field in generated Rust: provided fields keep source order and omitted optional fields are appended as explicit `None`. `const value: int? = 1` remains an error; use `option.some(1)`.

## Flow narrowing

A flow test about a simple immutable binding narrows its type inside the tested region. The narrowed name has type `T`, while the original optional keeps its value and type:

```lift
function greet(value: string?): string {
    if value == null { return "anonymous"; }
    return "hello, " + value; // `value` is `string` here
}

function pick(first: int?, second: int?): int {
    if first != null {
        return first + (second ?? 0); // `first` is `int`
    }
    return 0;
}
```

Accepted tests:

- `option.is_some(x)`, `x != null`, and `null != x` narrow `x` in the true branch.
- `x == null` and `null == x` narrow `x` in the else branch.
- `!condition` swaps the true and false facts.
- `a && b` narrows with every fact from both operands in the true branch.
- `a || b` narrows with every false fact from both operands in the else branch.
- Parentheses are transparent.
- An `if` whose branch always returns narrows the remainder of the enclosing block on the other path, so `if x == null { return; }` makes `x` non-optional afterwards.
- `else if` chains carry the false facts into the next condition's branches, including when the next branch returns.
- A `while` condition's true facts narrow the loop body; no narrowing persists after the loop.
- Closures declared inside a narrowed region capture the narrowed value as the usual read-only snapshot.

Narrowing requires an immutable binding: a `const` local, a parameter, or a closure capture snapshot. `let` bindings do not narrow, because they can be reassigned. Only a bare name narrows; field and array projections (`user.nickname`, `items[0]`) do not. Narrowing does not apply inside the condition expression itself: `if value != null && value > 0` rejects the right operand because `value` is still `int?` there. Use a nested `if` or an explicit `option.unwrap` when a compound condition must combine the test and the use.

The narrowed binding is a compiler-generated read-only clone of the payload. Mutating it would silently change a copy, so Lift rejects assignments through a narrowed value, including field/index writes and calls that would take the value by mutable reference:

```lift
if value != null {
    value = option.some(1); // error: narrowed value is read-only
}
```

The original optional is never moved or consumed by narrowing. Because each narrowed region reads a clone, payloads follow Lift's value semantics; the generated Rust clones the payload once when the region starts and is only valid because the flow test proved `Some`. There is no narrowing for `Result` (`result.is_ok`), no optional chaining, and no automatic propagation; `match` handles `Option`/`Result` explicitly (see [matching Option and Result](match.md)), but it is not flow narrowing. A direct `T?` parameter or return is supported by Rust imports, mapped to `Option<&T>` or the owned option; an `Option` nested inside an array remains outside the FFI, while a direct `Option` inside a `Result` payload is supported on parameters and returns. See [external structs and Option in Rust imports](ffi-structs-and-options.md).

## Coalescing contract

- `T? ?? T` returns `T`.
- `T? ?? T?` returns `T?`, enabling `a ?? b ?? fallback` chains.
- The left expression executes exactly once. The right expression executes only for `None`.
- `Some(false)`, `Some(0)`, and `Some("")` retain their payload; this is absence checking, not truthiness.
- `??` is left-associative and has lower precedence than `||`. Mixing `??` with `&&`/`||` without parentheses is a syntax error, following the TS-style restriction. Use `(value ?? false) || other` or `value ?? (first && second)`.

`null` uses an expected optional type from an annotation, field, function argument/return, or optional equality operand. `null ?? 42` can infer the payload from its immediate fallback. Bare `const value = null` and uncontextual `null ?? null` are errors. Inference remains local: expected context does not travel through a non-null left operand of another coalesce, so `const value: int = null ?? null ?? 1` is rejected. Annotate an intermediate optional instead.

## Ownership and validation

The HIR records owned `OptionSome`, typed `OptionNone`, a dedicated `Coalesce` node with an explicit value/optional result mode, and a `Narrow` statement for every narrowing region. A `Narrow` statement binds a fresh read-only binding to a clone of the optional payload; all later reads of the name in that region use the derived binding. The left operand of `??` becomes an owned snapshot using existing copy/clone rules; code generation emits `match` on that snapshot and places the owned fallback only in the `None` arm, without a closure or new ownership inference.

Liveness treats a `Narrow` statement as a definition of the derived binding and a use of the source optional, so the source stays available and the mandatory payload clone is never optimized away. Other last-use move decisions remain unchanged. Existing borrow/alias diagnostics remain in effect, and mutation through a narrowed binding is rejected explicitly.

Parser, semantic, codegen, and CLI tests cover suffix ordering, types, required versus optional fields, null inference, FFI nesting and local-struct rejection, recursive types, explicit unwrap failure, narrowing in branches, guards, else-if chains, loops and closure captures, rejected mutations, and lazy/exactly-once effects. `examples/option.lift` mutates counters to distinguish evaluated paths and verifies false/zero/empty-string payloads, optional-field omission, and narrowed values through a native binary.
