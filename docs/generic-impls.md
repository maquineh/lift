# Generic impl methods

[Português do Brasil](generic-impls.pt-BR.md) · [User guide](../README.md)

`impl Box<T>` declares methods once for a generic interface; the compiler creates a concrete impl for every application discovered in the program (`Box<int>`, `Box<string>`), substituting `T` in signatures, bodies and local annotations.

```lift
interface Box<T> { value: T }

impl Box<T> {
    get(): T { return this.value; }
    set(value: T): void { this.value = value; }
}

let integer: Box<int> = { value: 1 };
console.log(integer.get()); // 1
integer.set(2);
console.log(integer.get()); // 2

let text: Box<string> = { value: "hi" };
console.log(text.get()); // hi
```

## How it works

- Generic impl declarations are collected before the container monomorphizer.
- All concrete applications of the target interface are discovered from annotations, parameters, returns, local declarations and closure types.
- For each application, methods are cloned, `T` is substituted, and a concrete `impl Box<int>` is generated. The normal impl lowering then adds `this: Box<int>` and mangles method names (`$method:Box<int>:get`).
- Calls like `integer.get()` resolve by the receiver's concrete container type. `this` mutations, pass modes and resource rules are handled by the existing method machinery.
- The generic `impl Box<T>` template is removed after specialization; it is not callable as a raw type.

## Limits

- The target must be a generic interface with a matching arity; generic impls for enums/unions, external layouts, or unknown interfaces are not supported.
- Specialization is driven by discovered applications: an impl with no concrete use produces no methods. Method names are internal/mangled.
- No generic methods nested inside a generic impl (method-level type parameters are not specialized), no FFI generic impls, and no native Map API yet.
- The same application found through multiple annotations produces one impl (deduplicated by canonical name).

Tests: `crates/lift_sema/tests/generic_impls_contract.rs`; example [`examples/generic-impls.lift`](../examples/generic-impls.lift).
