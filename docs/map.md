# Native Map

[Português do Brasil](map.pt-BR.md) · [User guide](../README.md)

`Map<K, V>` is a built-in map backed by `std::collections::BTreeMap<K, V>`. It needs no declaration or import:

```lift
const numbers: Map<string, int> = map.new();
const one = numbers.insert("a", 1);
const two = one.insert("b", 2);

console.log(two.get("a") ?? 0); // 1
console.log(two.get("z") ?? 0); // 0
console.log(two.contains("b")); // true
console.log(two.len());         // 2
```

## API

- `map.new()` creates an empty map; it requires an expected `Map<K, V>` type context.
- `map.insert(key, value): Map<K, V>` is **functional**: it clones the map, inserts/updates the key, and returns the new map. Reassign a `let`/`const` binding to keep the result.
- `map.get(key): V?` returns `Option<V>` (`?? fallback` works).
- `map.contains(key): boolean`.
- `map.len(): int`.
- Maps are values: they can be stored in variables/arrays/Option/Result, passed/returned, cloned, and compared structurally.
- Keys may be `int`, `string`, or `boolean` (ordered); `number` keys are rejected because `f64` has no `Ord`. Values may be primitives, strings, or booleans in this increment.

## Compiler/runtime

The compiler injects a `Map<K, V>` external generic template implemented by `lift_core::collections::BTreeMap<K, V>`. Applications are monomorphized to concrete external layouts; `map.new` and map methods lower to dedicated HIR nodes and BTreeMap operations. There is no hash-map dependency beyond `std`.

## Limits

- Functional `insert` only (clone+insert); no in-place mutation, `remove`, iteration/fold, or borrowed entry views yet.
- Key/value types are restricted to primitive/string/boolean; nested containers, structs, enums, and closures are not supported as keys/values in this increment.
- No `Map` in FFI signatures, generic impl methods on Map, or JSON/serialization integration.

Tests: `crates/lift_sema/tests/map_contract.rs`; example [`examples/map.lift`](../examples/map.lift).
