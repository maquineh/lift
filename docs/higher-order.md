# Function types and callbacks

[Português do Brasil](higher-order.pt-BR.md) · [User guide](../README.md)

Lift closures are no longer local-only: a function type can be written in annotations, closures and named functions can be stored, copied, passed, returned, captured by other closures, and passed to or returned from Rust imports as callbacks.

```lift
function double(value: int): int { return value * 2; }

function apply(value: int, transform: (int) => int): int {
    return transform(value);
}

function make_adder(amount: int): (int) => int {
    return (value: int): int => value + amount;
}

const stored: (int) => int = double;
const copied = stored;
console.log(apply(3, stored));   // 6
console.log(make_adder(2)(10));  // 12
```

## Function type syntax

`(T, U) => R` is a function type. It is accepted in parameters, return types, variable annotations, and closure parameters. `void` is allowed only as the result type: `() => void` is a side-effecting callback, while `(void) => int` is rejected.

Function types follow these placement rules:

- Parameters, returns, `const`/`let` variables, and closure parameters may hold function values.
- Function values cannot appear in struct fields, array elements, `Option<T>` payloads, or `Result<T, E>` payloads. Those positions require `Clone`/`PartialEq`/`Debug` derives or payload rules that `Box`-like callbacks do not satisfy.
- Function types are nominal by structure: `(int) => int` and `(number) => int` are different types; there is no implicit conversion between function signatures.

## Value representation and copies

A function value lowers to Rust `::std::rc::Rc<dyn Fn(OwnedArgs...) -> Owned>`:

- Closure parameters are owned types (`i64`, `String`, `Vec<T>`, structs), never borrowed. A callback can therefore be called with a value Lift already owns.
- Closures and named functions create an `Rc`; copies assign/clone the `Rc`, so Lift copy semantics hold and one closure can be used in several places.
- Calls are dynamic dispatch through the `Fn` trait; the closure body still uses the existing snapshot captures, which are read-only. Mutating a captured binding or a captured struct field is rejected.
- Passing a callback to a Lift function passes the `Rc` by value (cheap clone at the call site). A closure that captures another callback clones that `Rc` (the capture and the original both remain usable).

Named functions also become values: writing the function name in a value position produces a boxed wrapper with owned parameter types. Borrowed `string`/array/struct parameters of the target are re-borrowed inside the wrapper, so `const trim_fn: (string) => string = trim;` works for a Rust import that takes `&str`. Methods are not first-class: method names are internal and cannot be referenced as values. Functions whose signature contains opaque resources are rejected as values.

## Calls and higher-order composition

A function value is called like a function:

- A variable/parameter of function type: `transform(value)`.
- The direct result of another call: `make_adder(2)(10)`.
- An immediately invoked closure: `((value: int): int => value + 1)(5)`.

Closure parameters can themselves be function-typed: `function call_higher(callback: ((int) => int) => int, seed: int)`. Closures can return closures and capture callbacks, enabling composition and partial application. There is no separate `Fn`/`FnMut`/`FnOnce` distinction: all Lift callbacks are `Fn` and cannot mutate captured state.

## Callback FFI

A function type in an import signature is a callback parameter:

```lift
import function apply_twice(value: int, callback: (int) => int): int from "lift_core::interop::apply_twice";
import function apply_text(value: string, callback: (string) => string): string from "lift_core::interop::apply_text";
import function add_constant(amount: int): (int) => int from "lift_core::interop::add_constant";

console.log(apply_twice(2, double));               // 8
console.log(apply_text("  lift  ", trim));          // "lift"
const add_ten = add_constant(10);                  // Box<dyn Fn> from Rust -> Rc
console.log(add_ten(5));                            // 15
```

| Lift position | Rust signature |
| --- | --- |
| callback parameter `(T) => R` | `&dyn Fn(OwnedRust T...) -> OwnedRust R` |
| callback return `(T) => R` | `Box<dyn Fn(OwnedRust T...) -> OwnedRust R>` converted to `Rc` by the wrapper |

- Callback payloads use owned Rust types (`i64`, `String`, `Vec<T>`, external struct paths) and may return `void` (`()`). Callbacks are invoked synchronously through the safe function pointer.
- `Option`, `Result`, nested function types, and opaque resources inside a callback signature are rejected; use a plain value adapter or a Rust wrapper.
- The generated wrapper borrows the callback for the duration of the call (`&dyn Fn`), so the Lift callback remains usable afterwards.
- A callback returned by an import is stored as `Rc`; the Rust side must return `Box<dyn Fn(...) -> ...>` and the wrapper converts it with `Rc::from`.
- Callback parameters are not C function pointers: this is a Rust trait-object contract, not `extern "C"`.

## Limits

- No function values in fields, arrays, `Option`, or `Result`; no collections of callbacks.
- No mutable captures, no closure/function equality or ordering, no reflection, no function pointers to methods, no async callbacks.
- No higher-order generics: function types must be written concretely; a callback signature cannot be parameterized or inferred from context beyond the annotation.
- A Rust adapter that requires `FnMut`, `FnOnce`, generic `impl Fn`, or ownership of the callback needs a local wrapper; the FFI contract is `&dyn Fn` in parameters and `Box<dyn Fn>` in returns.
- Callback identity is not observable; copying preserves the same behavior but not an addressable identity.

Parser, semantic, codegen, and CLI tests cover function type annotations, storage and copies, named functions as values, callback captures, higher-order parameters, immediate calls of returned callbacks, FFI callback parameters and returns, and the rejected placements above. `examples/higher-order.lift` exercises the executed path through the native binary.
