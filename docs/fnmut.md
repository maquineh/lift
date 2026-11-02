# Mutable closure captures (`FnMut`-style)

[Português do Brasil](fnmut.pt-BR.md)

Closures may mutate captured `let` variables. Each closure owns a mutable snapshot of its captures, stored as `Rc<RefCell<T>>` in the generated Rust, so the closure keeps state across calls while still fitting Lift's `Rc<dyn Fn>` callback representation.

```lift
function makeCounter(): () => int {
    let count = 0;
    return (): int => {
        count = count + 1;
        return count;
    };
}

const counter = makeCounter();
console.log(counter()); // 1
console.log(counter()); // 2
const other = makeCounter();
console.log(other());   // 1
```

## Semantics and limits

- Assignments to captured variables are allowed; the snapshot is initialized when the closure is created and persists inside the closure (including across `Rc` clones).
- Mutations do **not** update the original outer variable: captures remain snapshots. Accumulate outside state through the closure's return value, `reduce`, or explicit `for...of`.
- Multiple closures over the same variable get independent snapshots; two counters are independent.
- The closure still implements `Fn`; mutation uses interior mutability (`Rc<RefCell<T>>`), so array methods and callbacks need no signature changes.
- Nested closures capture a read snapshot of an outer mutable capture; sharing one cell across nested closures is not supported.
- Resource captures remain rejected; closure parameters are already mutable when the body assigns them.

Tests: `crates/lift_sema/tests/enum_contract.rs` (mutation marker), E2E `examples/fnmut.lift`; existing higher-order/array-method contracts cover immutable captures.
