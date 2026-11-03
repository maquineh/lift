# Generic functions over containers

[Português do Brasil](generic-container-functions.pt-BR.md) · [User guide](../README.md)

A generic function whose signature mentions a generic container can be used through explicit type arguments. The compiler specializes the function for each concrete application before semantic analysis:

```lift
interface Box<T> { value: T }

function unwrap<T>(box: Box<T>): T { return box.value; }
function unwrapTwice<T>(box: Box<Box<T>>): T {
    return unwrap::<Box<T>>(box).value;
}

const boxed: Box<int> = { value: 41 };
console.log(unwrap::<int>(boxed));       // 41
const nested: Box<Box<string>> = { value: { value: "hi" } };
console.log(unwrapTwice::<string>(nested)); // hi
```

## How it works

- The specializer finds generic functions whose parameter/return types contain generic applications (for example `Box<T>`).
- Every explicit call `f::<Args>(...)` creates a concrete copy of the function with type parameters substituted in the signature, local annotations, closure types, and nested explicit calls. The copy is named by its application (`unwrap<int>`, `unwrapTwice<string>`), has no type parameters, and is fed back through the container monomorphizer.
- Nested calls such as `unwrap::<Box<T>>` inside `unwrapTwice<T>` are substituted first (`unwrap::<Box<string>>`) and then specialized recursively.
- Calls without explicit type arguments are not specialized: the original container-generic template is removed, so `unwrap(boxed)` is an unknown function. Use `unwrap::<int>(boxed)`.

## Limits

- Explicit type arguments are required for this category; there is no inference from a `Box<int>` argument to `T`.
- Specialized copies are ordinary named functions; the original template cannot be used as a value, callback, or without arguments.
- Container-generic functions cannot be generic over a container template itself beyond the specialization mechanism, and there are no generic methods/`impl`, FFI generic containers, or native Map APIs yet.
- Generated names are internal (`unwrap<int>`); diagnostics display them as function names.

Tests: `crates/lift_sema/tests/generic_container_functions_contract.rs`; example [`examples/generic-container-functions.lift`](../examples/generic-container-functions.lift).
