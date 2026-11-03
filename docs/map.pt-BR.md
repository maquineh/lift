# Map nativo

[English](map.md) · [Guia de uso](../README.pt-BR.md)

`Map<K, V>` é um map embutido, implementado sobre `std::collections::BTreeMap<K, V>`. Não precisa de declaração nem import:

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

- `map.new()` cria um map vazio; exige contexto de tipo `Map<K, V>`.
- `map.insert(key, value): Map<K, V>` é **funcional**: clona o map, insere/atualiza a chave e retorna o novo map. Reatribua um binding `let`/`const` para guardar o resultado.
- `map.get(key): V?` retorna `Option<V>` (`?? fallback` funciona).
- `map.contains(key): boolean`.
- `map.len(): int`.
- Maps são valores: podem ficar em variáveis/arrays/Option/Result, ser passados/retornados, clonados e comparados estruturalmente.
- Chaves podem ser `int`, `string` ou `boolean` (ordenáveis); chaves `number` são rejeitadas porque `f64` não tem `Ord`. Valores podem ser primitivos, strings ou booleanos neste incremento.

## Compilador/runtime

O compilador injeta um template externo genérico `Map<K, V>` implementado por `lift_core::collections::BTreeMap<K, V>`. Aplicações são monomorfizadas em layouts externos concretos; `map.new` e métodos de Map baixam para nós HIR dedicados e operações de BTreeMap. Não há dependência extra além de `std`.

## Limites

- Apenas `insert` funcional (clone+insert); sem mutação in-place, `remove`, iteração/fold ou views emprestadas.
- Chaves/valores restritos a primitivos/string/boolean; containers aninhados, structs, enums e closures não são suportados como chaves/valores neste incremento.
- Sem `Map` em assinaturas FFI, métodos genéricos de impl sobre Map ou integração JSON/serialização.

Testes: `crates/lift_sema/tests/map_contract.rs`; exemplo [`examples/map.lift`](../examples/map.lift).
