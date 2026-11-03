# Enums genéricos

[English](generic-enums.md) · [Guia de uso](../README.pt-BR.md)

Enums genéricos são monomorfizados por aplicação concreta, como as interfaces genéricas. `Maybe<int>` vira um enum nominal próprio com `T` substituído por `int`.

```lift
enum Maybe<T> { Some(T), None }
enum List<T> { Nil, Cons(T, List<T>) }

const some: Maybe<int> = Maybe.Some(5);
const values: List<int> = List.Cons(1, List.Cons(2, List.Nil));
```

## Construção e matching

- Declare com `enum Nome<T, ...> { ... }`; variantes podem usar payloads posicionais ou nomeados contendo `T`.
- Aplique como `Nome<Arg>`, em anotações, parâmetros/retornos, campos, arrays, Option/Result e containers aninhados.
- Construção usa o caminho do template (`Maybe.Some(5)`, `Maybe.None`) e exige **tipo esperado** (`const value: Maybe<int> = ...`, argumento de função, retorno ou campo). A instância esperada desambigua a variante monomorfizada. Sem tipo esperado, a construção é rejeitada.
- `match` usa o caminho do template (`Maybe.Some(x)` / `Maybe.None`) e o tipo concreto do scrutinee; os braços são exaustivos sobre as variantes monomorfizadas.
- Cada instanciação é distinta (`Maybe<int>` vs `Maybe<string>`), com igualdade estrutural, clone e semântica de valor.
- Enums genéricos recursivos são suportados pelos campos `Box` automáticos; variantes nomeadas e containers de enums genéricos funcionam.

## Codegen e limites

O monomorfizador pré-semântico reescreve aplicações para nomes sintéticos (`Maybe<int>`, `List<string>`) e emite um enum Rust concreto por instanciação. Não há enum genérico Rust na saída.

Limites:

- Construção exige tipo esperado; não há inferência de `T` apenas pelos argumentos do construtor.
- Sem métodos/`impl` genéricos, argumentos de tipo explícitos, funções genéricas sobre `Maybe<T>`/`Box<T>`, enums genéricos em FFI/imports ou templates sem argumentos.
- Payloads recursivos diretos são boxeados automaticamente; arrays usam indireção `Vec`.

Testes: `crates/lift_sema/tests/generic_enums_contract.rs`; exemplo [`examples/generic-enums.lift`](../examples/generic-enums.lift).
