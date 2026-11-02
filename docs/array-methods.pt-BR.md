# Métodos de array: `map`, `filter`, `find`, `some`, `every`, `forEach`, `reduce`

[English](array-methods.md) · [Guia de uso](../README.pt-BR.md)

Arrays têm métodos higher-order embutidos que recebem um callback. Eles são monomorfizados pelo tipo do elemento na análise, então não precisam de generics Lift.

```lift
function double(value: int): int { return value * 2; }

const values = [1, 2, 3, 4];
const doubled = values.map(double);                 // int[]
const evens = values.filter((value: int): boolean => value % 2 == 0);
const first = values.find((value: int): boolean => value > 2); // int?
const total = values.reduce(0, (accumulator: int, value: int): int => accumulator + value);
console.log(doubled[0]);  // 2
console.log(values[0]);   // 1 — o array original não muda
```

## Métodos

| Método | Callback | Resultado |
| --- | --- | --- |
| `map` | `(T) => U` | `U[]` |
| `filter` | `(T) => boolean` | `T[]` |
| `find` | `(T) => boolean` | `T?` (`null` quando ausente) |
| `some` | `(T) => boolean` | `boolean` (curto-circuito no primeiro `true`) |
| `every` | `(T) => boolean` | `boolean` (curto-circuito no primeiro `false`) |
| `forEach` | `(T) => void` | `void`; executa o callback para cada elemento em ordem |
| `reduce` | `(U, T) => U` + inicial `U` | `U` (fold à esquerda) |

O receptor precisa ser `T[]`. O callback pode ser uma closure ou uma função nomeada como valor:

```lift
const labels = values.map((value: int): string => `#${value}`);
const scaled = values.map(double);
```

## Avaliação e semântica de valor

- O receptor é avaliado uma vez e clonado antes da iteração (`values` nunca é consumido nem mutado). O callback é avaliado uma vez e guardado em um temporário.
- Cada elemento é passado como valor owned. `filter`, `find`, `some` e `every` clonam o elemento antes de chamar o predicado, então predicados recebem o mesmo tipo de valor owned que callbacks de `map`.
- Métodos preservam ordem e encadeiam: `values.filter(predicado).map(transform)` avalia filter primeiro e mapeia o resultado.
- `reduce` parte do valor inicial e processa elementos da esquerda para a direita; o tipo do acumulador é o tipo do inicial, e o callback precisa retornar esse mesmo tipo.
- Array vazio é válido: `map`/`filter` produzem arrays vazios, `find` retorna `null`, `some` é `false`, `every` é `true`, `reduce` retorna o inicial e `forEach` não executa nada.
- Callbacks são valores `Rc`, então uma função nomeada ou closure com captures somente leitura pode ser usada sem anotações extras.
- Liveness trata esses métodos como nós conservadores: usos dentro do callback contam, mas clones internos do callback não viram moves.

## Erros

São rejeitados antes do codegen:

- Nome de método desconhecido em array.
- Callback cujo tipo de parâmetro difere do tipo do elemento.
- `filter`/`find`/`some`/`every` retornando algo diferente de `boolean`, `forEach` retornando algo diferente de `void`, ou `reduce` com callback incompatível com `(U, T) => U`.
- Número errado de argumentos (um callback, ou `initial` mais callback no `reduce`).
- Argumento que não é callback (`values.map(1)`).
- `find` em `T?[]`: o resultado seria `Option` aninhado, rejeitado pelo contrato de Option; use `filter`.
- Chamar esses métodos em receptor que não é array.

## Limites

- Sem `flatMap`, `sort`, `includes`, `indexOf`, `slice` ou métodos mutantes como `push`/`pop`; arrays continuam valores Lift de tamanho fixo.
- Callbacks recebem só o valor — não há parâmetro de índice/array.
- Callbacks não podem mutar bindings capturados; use `reduce` ou um laço `for...of` explícito quando precisar de acumulador.
- O resultado de `forEach` é `void`; não pode ser usado como valor.
- Sem iteradores lazy, sem cadeia de métodos em `Option`/`string`, e sem callbacks que retornam funções dentro desses métodos (valores de função não podem ser elementos de array).

Aceitação de parser, tipagem/rejeições semânticas, Rust gerado (`into_iter().map/filter/find/any/all/fold`) e execução nativa por [`examples/array-methods.lift`](../examples/array-methods.lift) são cobertos por testes.
