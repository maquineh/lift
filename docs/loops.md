# Loops: `for...of`, `while`, `break`, and `continue`

[Português do Brasil](loops.pt-BR.md) · [User guide](../README.md)

Lift has two loop statements: `while` evaluates a boolean condition before each iteration, and `for...of` iterates an array. `break` and `continue` control the innermost loop.

```lift
function first_positive(values: int[]): int? {
    for (const value of values) {
        if value > 0 { return option.some(value); }
    }
    return null;
}

function count_before(values: int[], limit: int): int {
    let count = 0;
    for (const value of values) {
        if value == limit { break; }
        if value < 0 { continue; }
        count = count + 1;
    }
    return count;
}

let index = 0;
while index < 3 {
    index = index + 1;
    if index == 2 { continue; }
    console.log(index); // 1, 3
}
```

## `for...of`

```text
for (const|let item of expression) { ... }
```

- `expression` must have an array type `T[]`; anything else is rejected before code generation.
- `const` makes the per-iteration binding read-only. `let` allows reassigning the binding inside the body; it does not mutate the original array.
- The binding is scoped to the loop body and cannot shadow an active name. A declaration after the loop may reuse the name.
- The array is evaluated once and iterated as an **owned snapshot**: the compiler clones the array before the loop, and each element is an owned value. Later changes to the source array are not visible during that iteration, and the loop never consumes the source binding.
- Arrays of opaque resources, closures, unions, or nested optionals cannot be iterated (those types are already rejected in arrays or by the Option contract).
- Elements of `T?[]` keep the optional type; inside the body the usual narrowing works when the binding is `const`.

`for...of` over a temporary expression is allowed: `for (const value of [10, 20]) { ... }`.

## `while`

`while condition { ... }` keeps the existing contract: the condition must be `boolean`; facts from conditions that narrow optionals apply inside the body. There is no `do...while`.

## `break` and `continue`

- `break;` exits the innermost `while`/`for`; `continue;` starts the next iteration of the innermost loop.
- Both require a trailing semicolon and are rejected outside a loop, including inside a closure declared within a loop (closures are a separate body).
- Inside `try`, `break`/`continue` target the loop and skip the `catch`; after the loop, execution continues normally. `break`/`continue` inside a `catch` block also target the enclosing loop.
- Both are supported in `while` and `for...of`; there are no labels, so an inner loop cannot `break` an outer one.
- `break`/`continue` do not count as function returns: a function still needs a final `return` even if a loop body returns on some path, because the loop may run zero times.

## Return, resources, and liveness

- A `for...of`/`while` may execute zero or more times, so it never guarantees that the function returns. `return` inside a loop body returns from the function.
- The loop cannot consume an opaque resource declared outside it; the existing rule that moved a resource inside a `while` body is also applied to `for...of` bodies. Resources created and consumed within one iteration are allowed.
- Liveness treats each loop conservatively: uses in the body count, and clones inside loops are not converted to moves by the last-use pass. The iteration array itself is cloned once before the loop.
- Generated Rust uses labeled loops (`'lift_loop_N`) so `break`/`continue` can cross the labeled block used to lower `try`/`catch` without changing semantics.

## Limits

- No C-style `for`, no `for...in` over indices or object keys, no iterator protocol, ranges, labeled loops, or loop expressions.
- No `else` on loops and no `break` with a value.
- The loop binding is a value, not a reference to the array slot; mutating it never mutates the source array.
- Closures cannot use `break`/`continue` from an enclosing loop.

Parser, semantic, codegen, and CLI tests cover loop typing and scoping, snapshot iteration, const/let binding mutation, nested targets, `break`/`continue` in `try`/`catch`, closure rejection, missing-return analysis, resource consumption, and native execution of [`examples/loops.lift`](../examples/loops.lift).
