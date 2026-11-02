# Array methods: `map`, `filter`, `find`, `some`, `every`, `forEach`, `reduce`

[Português do Brasil](array-methods.pt-BR.md) · [User guide](../README.md)

Arrays have built-in higher-order methods that take a callback. They are monomorphized by the element type at analysis time, so no Lift generics are needed.

```lift
function double(value: int): int { return value * 2; }

const values = [1, 2, 3, 4];
const doubled = values.map(double);                 // int[]
const evens = values.filter((value: int): boolean => value % 2 == 0);
const first = values.find((value: int): boolean => value > 2); // int?
const total = values.reduce(0, (accumulator: int, value: int): int => accumulator + value);
console.log(doubled[0]);  // 2
console.log(values[0]);   // 1 — the original array is unchanged
```

## Methods

| Method | Callback | Result |
| --- | --- | --- |
| `map` | `(T) => U` | `U[]` |
| `filter` | `(T) => boolean` | `T[]` |
| `find` | `(T) => boolean` | `T?` (`null` when absent) |
| `some` | `(T) => boolean` | `boolean` (short-circuits on the first `true`) |
| `every` | `(T) => boolean` | `boolean` (short-circuits on the first `false`) |
| `forEach` | `(T) => void` | `void`; runs the callback for every element in order |
| `reduce` | `(U, T) => U` + initial `U` | `U` (left fold) |

The receiver must be `T[]`. The callback can be a closure or a named function value:

```lift
const labels = values.map((value: int): string => `#${value}`);
const scaled = values.map(double);
```

## Evaluation and value semantics

- The receiver is evaluated once and cloned before iteration (`values` itself is never consumed or mutated). The callback is evaluated once and stored in a temporary.
- Each element is passed as an owned value. `filter`, `find`, `some`, and `every` clone the element before calling the predicate, so predicates receive the same owned value type as `map` callbacks.
- Methods preserve order and chain: `values.filter(predicate).map(transform)` evaluates filter first and maps its result.
- `reduce` starts from the initial value and processes elements from left to right; the accumulator type is the initial value type, and the callback must return that same type.
- An empty array is valid: `map`/`filter` produce empty arrays, `find` returns `null`, `some` is `false`, `every` is `true`, `reduce` returns the initial value, and `forEach` runs nothing.
- Callbacks are `Rc` values, so a named function or a closure with read-only captures can be used without extra annotations.
- Liveness treats these methods as conservative expression nodes: uses inside the callback count, but clones inside the callback are not rewritten as moves.

## Errors

The following are rejected before code generation:

- An unknown method name on an array.
- A callback whose parameter type differs from the element type.
- `filter`/`find`/`some`/`every` returning anything other than `boolean`, `forEach` returning anything other than `void`, or `reduce` with a callback that does not match `(U, T) => U`.
- Wrong argument counts (one callback, or `initial` plus callback for `reduce`).
- A non-callback argument (`values.map(1)`).
- `find` on `T?[]`: the result would be a nested `Option`, which the Option contract rejects; use `filter` instead.
- Calling these methods on a non-array receiver.

## Limits

- No `flatMap`, `sort`, `includes`, `indexOf`, `slice`, or mutating methods such as `push`/`pop`; arrays remain fixed-size Lift values.
- Callbacks receive the value only — there is no index/array parameter.
- Callbacks cannot mutate captured bindings; use `reduce` or an explicit `for...of` loop when an accumulator is needed.
- `forEach` result is `void`; it cannot be used as a value.
- No lazy iterators, no method chains on `Option`/`string`, and no callbacks that return function values inside these methods (function values cannot be array elements).

Parser acceptance, semantic typing/rejections, generated Rust (`into_iter().map/filter/find/any/all/fold`), and native execution through [`examples/array-methods.lift`](../examples/array-methods.lift) are covered by tests.
