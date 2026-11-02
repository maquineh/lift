# Funções genéricas

[English](generics.md) · [Guia de uso](../README.pt-BR.md)

Funções genéricas declaram parâmetros de tipo entre o nome e a lista de parâmetros. Os argumentos de tipo são inferidos em cada chamada a partir dos tipos dos argumentos; não existe sintaxe explícita `f<int>(...)`.

```lift
function identity<T>(value: T): T { return value; }
function same<T>(left: T, right: T): boolean { return left == right; }
function wrap<T>(value: T): T[] { return [value]; }

console.log(identity(42));
console.log(same("a", "b"));
const numbers: int[] = wrap(7);
```

## Declaração e inferência

- Sintaxe: `function nome<T, U>(...)`. Parâmetros de tipo duplicados são rejeitados, e cada parâmetro precisa aparecer em ao menos um parâmetro de entrada para que toda chamada possa inferi-lo (parâmetros usados só no retorno são rejeitados).
- `T` pode ser usado diretamente, em arrays (`T[]`), `Option` (`T?`) e `Result<T, E>` em parâmetros, retorno, anotações locais e chamadas genéricas aninhadas.
- Na chamada, cada tipo de parâmetro é unificado com o tipo do argumento. `same(1, "x")` falha porque `T` não pode ser `int` e `string`; inferência ausente é erro.
- Argumentos precisam ser inferíveis sem tipo esperado: anote object literals/arrays vazios/`null` em uma variável e passe a variável quando necessário.
- Chamadas genéricas podem instanciar `T` com primitivos, strings, arrays, `Option`, `Result`, interfaces, enums/unions e valores nomeados. Recursos, `DbValue` e closures não podem instanciar um parâmetro de tipo.

## Restrições e operações

- A função Rust gerada é genérica com bounds `Clone + Debug + PartialEq` em cada parâmetro; construção, igualdade e clone são válidos para todas as instanciações válidas.
- Aritmética, templates/formatação, acesso a campo e outras operações que exigem tipo concreto são rejeitadas sobre `T`.
- Uma função genérica pode chamar outra função genérica; o argumento de tipo aninhado pode continuar sendo `T`. Chamadas genéricas recursivas são permitidas.
- Uma função genérica não pode ser usada como valor/callback (`const f = identity`), passada por FFI ou declarada como import. Não há interfaces/enums/structs genéricos, métodos/`impl` genéricos, argumentos de tipo explícitos ou especializações monomorfizadas.
- Parâmetros de tipo têm escopo na função (e closures aninhadas); não podem aparecer em campos de interface/enum/union de módulo.

## Codegen

Cada função genérica vira uma função Rust genérica com nomes de parâmetro mangleados e os três bounds de trait. Chamadas continuam chamadas Rust comuns e dependem da inferência do Rust; o HIR guarda os tipos inferidos para decisões de ownership/passagem.

Limites estão na [matriz de testes](testing-and-release.pt-BR.md); testes em `crates/lift_sema/tests/generics_contract.rs`, exemplo executável [`examples/generics.lift`](../examples/generics.lift).
