# Structs externas e Option em imports Rust

[English](ffi-structs-and-options.md) · [Guia de uso](../README.pt-BR.md)

Lift declara o layout de uma struct Rust com `import interface`. A declaração é um contrato: o tipo precisa existir no caminho Rust informado, seus campos precisam ser públicos com exatamente os nomes e tipos mapeados, e ele precisa implementar `Clone`, `Debug` e `PartialEq`. O Rust gerado contém checagens que o `rustc` valida durante `lift check`, `build` ou `run`.

```lift
import interface IntRange {
    start: int,
    end: int,
} from "lift_core::interop::IntRange";

import function int_range(start: int, end: int): IntRange from "lift_core::interop::int_range";
import function range_len(range: IntRange): int from "lift_core::interop::range_len";
import function range_len_or(range: IntRange?, fallback: int): int from "lift_core::interop::range_len_or";

const range = int_range(3, 8);
console.log(range.start);
console.log(range_len_or(null, -1));
```

## Declaração e mapeamento de campos

```text
import interface Name { field: Type, optional?: Type } from "pacote@MAJOR.MINOR.PATCH::modulo::Name";
```

Crates locais usam `lift_core::...`, `lift_db::...` ou `lift_web::...` como pacote. O destino segue as mesmas regras de imports de função: segmentos ASCII de caminho Rust, sem palavras reservadas, sem raw identifiers e versão exata de três partes para crates do registry. Os nomes participam das mesmas checagens de duplicidade que interfaces e funções locais, e declarar a mesma crate Rust com duas versões diferentes é rejeitado.

| Tipo do campo Lift | Tipo do campo Rust |
| --- | --- |
| `int` | `i64` |
| `number` | `f64` |
| `boolean` | `bool` |
| `string` | `String` |
| `T[]` (`T` escalar, `string` ou struct externa) | `Vec<T>` |
| struct externa `P` | o caminho Rust declarado |
| `field: T?` | `Option<T>`, ainda obrigatório no object literal |
| `field?: T` | `Option<T>`, omitível no object literal; a omissão vira `None` |

Nomes de campo são usados literalmente como nomes de campo Rust, então precisam ser identificadores Rust sem palavras reservadas. `Result`, structs locais Lift, recursos, arrays aninhados e opcionais aninhados são rejeitados como tipo de campo externo. Diferente de interfaces locais, o layout externo é opaco para a análise de ciclo direto de tamanho: a crate é dona do próprio layout e das regras de indireção.

O projeto gerado emite duas checagens por struct externa: uma função de layout que clona cada campo declarado no tipo mapeado, e uma asserção de trait bounds para `Clone + Debug + PartialEq`. Campo ausente, campo privado, campo renomeado, tipo diferente, trait ausente ou tipo não construível (`#[non_exhaustive]`) fazem o `rustc` rejeitar o Rust gerado. Esses são diagnósticos Rust sobre código gerado; Lift não os remapeia para a fonte. A dependência Cargo/`lift.toml` é acrescentada a partir do destino do `from`, mesmo quando nenhuma função importada usa a struct.

## Uso de structs externas em Lift

Structs externas são tipos de valor nominais comuns em Lift:

```lift
const chosen: IntRange = { start: 10, end: 14 };
console.log(chosen.start);
console.log(chosen == int_range(10, 14));
const ranges: IntRange[] = [chosen];
const maybe: IntRange? = option.some(chosen);
```

Object literals inicializam todos os campos declarados no Rust gerado, incluindo campos opcionais omitidos como `None`. O layout declarado precisa bater exatamente com a struct Rust porque a construção lista todos os campos; campos extras no tipo Rust também geram erro de campo ausente no `rustc`. Métodos `impl` podem usar a struct externa como `this`, com a inferência usual de `BorrowImm`/`BorrowMut`; mutar um campo público escreve no valor Rust real.

## Mapeamento de funções

Parâmetros são emprestados conforme as convenções FFI existentes; retornos são owned.

| Tipo Lift | Parâmetro Rust | Retorno Rust |
| --- | --- | --- |
| `int`, `number`, `boolean` | `i64`, `f64`, `bool` | iguais |
| `string` | `&str` | `String` |
| `T[]` | `&[T]` | `Vec<T>` |
| struct externa `P` | `&P` | `P` |
| `T?` | `Option<&str>`, `Option<&[T]>`, `Option<&P>` ou `Option<escalar>` | `Option<String>`, `Option<Vec<T>>`, `Option<P>` ou `Option<escalar>` |
| `Result<T, E>` | `&Result<T mapeado, E mapeado>` | `Result<T mapeado, E mapeado>`, com sucesso `void` como `()` |
| `(T) => R` | `&dyn Fn(T mapeado owned...) -> R mapeado owned` | `Box<dyn Fn(T mapeado owned...) -> R mapeado owned>`, convertido para `Rc` |

`T?` é aceito diretamente em posição de parâmetro ou retorno: `Option<i64>`, `Option<String>`, `Option<Vec<T>>` e `Option<P>` para struct externa. O wrapper gerado mantém a convenção owned de Lift (`&Option<String>`, `&Option<Vec<T>>`, `&Option<P>` em parâmetros emprestados) e converte para a assinatura Rust declarada com `as_deref`, `as_ref` ou `*`, de modo que `Option<&str>`, `Option<&[T]>` e `Option<&P>` funcionam sem código adaptador.

`Result<T, E>` é aceito como parâmetro emprestado (`&Result<...>`) e como retorno owned. Os payloads usam o mapeamento owned (`String`, `Vec<T>`, `P`) e podem conter um `Option` direto: `Result<int?, string>` vira `Result<Option<i64>, String>`, e `Result<string, string?>` vira `Result<String, Option<String>>`. `void` continua sendo apenas tipo de sucesso em retornos; `Result<void, E>` é rejeitado como parâmetro. O aninhamento é intencionalmente raso: um `Option` dentro do payload de um `Result`, e nenhum `Result` dentro de outro payload.

Um tipo de função é um callback: o parâmetro vira `&dyn Fn(...)` com payloads owned, e um retorno callback vira `Box<dyn Fn(...)>` (o wrapper converte para `Rc`). Assinaturas de callback podem usar escalares, `string`, arrays rasos, structs externas e retorno `void`. Callbacks são chamados sincronamente pelo ponteiro de função seguro; é um contrato de trait object Rust, não um ponteiro C. Veja [tipos de função e callbacks](higher-order.pt-BR.md) para o contrato do lado da linguagem.

Continuam rejeitados antes do codegen:

- `Option` aninhado em outro `Option`, array ou payload de `Result` que já esteja aninhado. Só `T?` direto em parâmetro/retorno e um `Option` direto dentro de payload de `Result` são suportados.
- Payload de `Result` aninhado (`Result<Result<T, E>, E>`), `Option<Result<T, E>>` (`Result<T, E>?`) e `Result` dentro de array.
- `Result<void, E>` como parâmetro; `void` só tem sucesso em posição de retorno.
- Structs locais Lift (declaradas com `interface` comum) em qualquer posição FFI, inclusive dentro de `Option`, `Result` ou arrays. Só tipos de `import interface` cruzam a FFI.
- Callbacks contendo `Option`, `Result`, outro tipo de função, arrays de callbacks ou recursos opacos; parâmetro callback `void`; e tipo de função em outras posições de payload FFI (só parâmetro/retorno direto é callback).
- Arrays aninhados, unions/enums, generics, funções `unsafe` e raw identifiers.

Como parâmetros são emprestados, uma função externa que exige ownership da struct não pode ser importada diretamente; escreva uma crate adaptadora em Rust ou um wrapper local. Mutação através de parâmetro FFI também não faz parte do contrato: o ponteiro de função seguro só verifica a assinatura declarada.

## Aceite e limites

Testes de parser, semântica, codegen e CLI cobrem a sintaxe `import interface`, dependências e conflitos de layout, validação de campo/tipo, rejeição de Option/Result aninhados e struct local, asserções de layout/traits geradas, conversões do wrapper para escalar, string, array e struct opcionais, parâmetros `Result` emprestados com payload opcional, parâmetros/retornos de callback com pontes de trait object, e execução nativa por `examples/interop.lift` e `examples/higher-order.lift`. Passar esses testes demonstra os layouts e assinaturas exercitados, não compatibilidade com qualquer tipo Rust: a ordem dos campos é irrelevante, mas struct `#[non_exhaustive]`, campos privados, campos renomeados ou tipo Rust diferente são rejeitados pelo `rustc` em vez de adaptados por Lift.

Não há serialização automática, renomeação de campo, mapeamento de enum, substituição genérica ou adaptação de Option além das conversões acima. Layouts são apenas structs com campos nomeados: tuple structs, enums, unions e tipos genéricos com parâmetros não podem ser escritos em uma declaração `import interface`.
