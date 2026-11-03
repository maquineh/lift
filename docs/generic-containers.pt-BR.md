# Containers genéricos

[English](generic-containers.md) · [Guia de uso](../README.pt-BR.md)

Interfaces genéricas descrevem containers cujos campos dependem de parâmetros de tipo. Cada uso concreto é **monomorfizado** antes da análise semântica: `Box<int>` vira uma struct nominal própria com `T` substituído por `int`.

```lift
interface Box<T> { value: T }
interface Pair<A, B> { left: A, right: B }
interface Map<K, V> { keys: K[], values: V[] }

const boxed: Box<int> = { value: 41 };
const nested: Box<Box<string>> = { value: { value: "hi" } };
const pair: Pair<int, string> = { left: 1, right: "x" };
const map: Map<string, int> = { keys: ["a"], values: [10] };
```

## Declaração e instanciação

- `interface Nome<T, U> { ... }` declara um template. Campos podem usar `T` diretamente, em arrays (`T[]`), `Option` (`T?`), `Result<T, E>`, tipos de função e outras aplicações genéricas (`Box<T>`).
- `Nome<Arg1, Arg2>` aplica o template. Argumentos são tipos Lift comuns (primitivos, interfaces, enums/unions, arrays, Option, Result, containers aninhados). A aridade precisa coincidir.
- Aplicações podem aparecer em anotações de variável, parâmetros/retornos de função, campos de struct/interface/enum, arrays, Option/Result e argumentos genéricos aninhados.
- Construção usa object literals com a anotação esperada (`const b: Box<int> = { value: 1 };`); acesso a campo é checado com os tipos substituídos.
- Cada instanciação é nominal e distinta: `Box<int>` e `Box<string>` são tipos diferentes, com clone/igualdade próprios. Formas aninhadas como `Box<Box<string>>` são suportadas.
- O nome do template não pode ser usado sem argumentos; aridade errada e parâmetros de tipo duplicados são rejeitados. `Map<K, V>` é uma interface genérica comum (por exemplo arrays de chaves/valores); ainda não há API nativa de hash map.

## Codegen

O monomorfizador cria interfaces concretas sintéticas nomeadas pela aplicação canônica (`Box<int>`, `Map<string, int>`) e reescreve todas as anotações para esses nomes. O backend Rust emite uma `struct` concreta por instanciação (mangleada), sem generics Rust em runtime. Interfaces genéricas recursivas são suportadas quando a recursão passa por arrays (indireção); auto-campos inline diretos são rejeitados pela checagem de ciclo de valor existente.

## Limites

- Sem enums/funções genéricas sobre parâmetros de container (`function f<T>(box: Box<T>)` ainda não é suportado), sintaxe explícita de argumentos de tipo, métodos/`impl` genéricos ou containers genéricos em FFI/imports.
- Sem inferência de argumentos de container a partir de object literal solto: anote a variável/parâmetro.
- Sem operações nativas de `Map`/`Set`; defina campos e funções auxiliares sobre o container.
- A monomorfização ocorre por pacote/módulo analisado; identidade genérica entre pacotes não é garantida além do nome nominal gerado.

Testes: `crates/lift_sema/tests/generic_containers_contract.rs`; exemplo [`examples/generic-containers.lift`](../examples/generic-containers.lift).
