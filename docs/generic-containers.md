# Generic containers

[Português do Brasil](generic-containers.pt-BR.md) · [User guide](../README.md)

Generic interfaces describe containers whose field types depend on type parameters. Each concrete use is **monomorphized** before semantic analysis: `Box<int>` becomes its own nominal struct with `T` replaced by `int`.

```lift
interface Box<T> { value: T }
interface Pair<A, B> { left: A, right: B }
interface Map<K, V> { keys: K[], values: V[] }

const boxed: Box<int> = { value: 41 };
const nested: Box<Box<string>> = { value: { value: "hi" } };
const pair: Pair<int, string> = { left: 1, right: "x" };
const map: Map<string, int> = { keys: ["a"], values: [10] };
```

## Declaration and instantiation

- `interface Name<T, U> { ... }` declares a template. Fields may use `T` directly, inside arrays (`T[]`), `Option` (`T?`), `Result<T, E>`, function types, and other generic applications (`Box<T>`).
- `Name<Arg1, Arg2>` applies the template. Type arguments are ordinary Lift types (primitives, interfaces, enums/unions, arrays, Option, Result, nested containers). Arity must match.
- Applications can appear in variable annotations, function parameters/returns, struct/interface/enum fields, arrays, Option/Result, and nested generic arguments.
- Construction uses object literals with the expected container annotation (`const b: Box<int> = { value: 1 };`); field access is checked with the substituted types.
- Each instantiation is nominal and distinct: `Box<int>` and `Box<string>` are different types, with their own clone/equality semantics. Nested forms such as `Box<Box<string>>` are supported.
- The template name cannot be used without arguments; arity errors and duplicate type parameters are rejected. `Map<K, V>` is an ordinary generic interface (for example arrays of keys/values); there is no native hash-map API yet.

## Code generation

The monomorphizer creates synthetic concrete interfaces named by their canonical application (`Box<int>`, `Map<string, int>`) and rewrites all annotations to those names. The Rust backend emits one concrete `struct` per instantiation (mangled), with no Rust generics at runtime. Recursive generic interfaces are supported when recursion passes through arrays (indirection); direct inline self-fields are rejected by the existing value-cycle check.

## Limits

- No generic enums/functions over container parameters (`function f<T>(box: Box<T>)` is not supported yet), explicit type-argument syntax, generic methods/`impl`, or FFI/imported generic containers.
- No inference of container type arguments from bare object literals: annotate the variable/parameter.
- No native `Map`/`Set` operations; define fields and helper functions over the container.
- Monomorphization happens per package/module analysis; cross-package generic identity is not guaranteed beyond the generated nominal name.

Tests: `crates/lift_sema/tests/generic_containers_contract.rs`; example [`examples/generic-containers.lift`](../examples/generic-containers.lift).
