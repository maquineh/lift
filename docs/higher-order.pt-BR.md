# Tipos de função e callbacks

[English](higher-order.md) · [Guia de uso](../README.pt-BR.md)

Closures em Lift deixaram de ser apenas locais: um tipo de função pode ser escrito em anotações, closures e funções nomeadas podem ser armazenadas, copiadas, passadas, retornadas, capturadas por outras closures, e passadas/retornadas de imports Rust como callbacks.

```lift
function double(value: int): int { return value * 2; }

function apply(value: int, transform: (int) => int): int {
    return transform(value);
}

function make_adder(amount: int): (int) => int {
    return (value: int): int => value + amount;
}

const stored: (int) => int = double;
const copied = stored;
console.log(apply(3, stored));   // 6
console.log(make_adder(2)(10));  // 12
```

## Sintaxe de tipo de função

`(T, U) => R` é um tipo de função. É aceito em parâmetros, tipos de retorno, anotações de variável e parâmetros de closure. `void` só é permitido como tipo de resultado: `() => void` é um callback com efeito colateral, enquanto `(void) => int` é rejeitado.

Regras de posicionamento:

- Parâmetros, retornos, variáveis `const`/`let` e parâmetros de closure podem conter valores de função.
- Valores de função não podem aparecer em campos de struct, elementos de array, payload de `Option<T>` ou payload de `Result<T, E>`. Essas posições exigem derives/payloads que callbacks `Rc<dyn Fn>` não satisfazem.
- Tipos de função são nominais pela estrutura: `(int) => int` e `(number) => int` são tipos diferentes; não há conversão implícita entre assinaturas.

## Representação e cópias

Um valor de função gera Rust `::std::rc::Rc<dyn Fn(Args owned...) -> Owned>`:

- Parâmetros de closure são tipos owned (`i64`, `String`, `Vec<T>`, structs), nunca emprestados. Um callback pode ser chamado com um valor que Lift já possui.
- Closures e funções nomeadas criam um `Rc`; cópias atribuem/clonam o `Rc`, então a semântica de valor de Lift vale e uma closure pode ser usada em vários lugares.
- Chamadas usam dispatch dinâmico via trait `Fn`; o corpo da closure mantém as captures snapshot existentes, que são somente leitura. Mutar um binding capturado ou campo de struct capturada é rejeitado.
- Passar um callback a uma função Lift passa o `Rc` por valor (clone barato no call site). Uma closure que captura outro callback clona esse `Rc` (a capture e o original continuam utilizáveis).

Funções nomeadas também viram valores: escrever o nome em posição de valor produz um wrapper boxed com parâmetros owned. Parâmetros `string`/array/struct emprestados do alvo são reemprestados dentro do wrapper, então `const trim_fn: (string) => string = trim;` funciona para um import Rust que recebe `&str`. Métodos não são de primeira classe: nomes de método são internos e não podem ser referenciados como valor. Funções cuja assinatura contém recursos opacos são rejeitadas como valor.

## Chamadas e composição higher-order

Um valor de função é chamado como uma função:

- Variável/parâmetro de tipo função: `transform(value)`.
- Resultado direto de outra chamada: `make_adder(2)(10)`.
- Closure imediatamente invocada: `((value: int): int => value + 1)(5)`.

Parâmetros de closure podem ser de tipo função: `function call_higher(callback: ((int) => int) => int, seed: int)`. Closures podem retornar closures e capturar callbacks, permitindo composição e aplicação parcial. Não há distinção `Fn`/`FnMut`/`FnOnce`: todo callback Lift é `Fn` e não pode mutar estado capturado.

## Callback FFI

Um tipo de função em assinatura de import é um parâmetro callback:

```lift
import function apply_twice(value: int, callback: (int) => int): int from "lift_core::interop::apply_twice";
import function apply_text(value: string, callback: (string) => string): string from "lift_core::interop::apply_text";
import function add_constant(amount: int): (int) => int from "lift_core::interop::add_constant";

console.log(apply_twice(2, double));               // 8
console.log(apply_text("  lift  ", trim));          // "lift"
const add_ten = add_constant(10);                  // Box<dyn Fn> do Rust -> Rc
console.log(add_ten(5));                            // 15
```

| Posição Lift | Assinatura Rust |
| --- | --- |
| parâmetro callback `(T) => R` | `&dyn Fn(T Rust owned...) -> R Rust owned` |
| retorno callback `(T) => R` | `Box<dyn Fn(T Rust owned...) -> R Rust owned>`, convertido para `Rc` pelo wrapper |

- Payloads de callback usam tipos Rust owned (`i64`, `String`, `Vec<T>`, caminhos de struct externa) e podem retornar `void` (`()`). Callbacks são chamados sincronamente pelo ponteiro de função seguro.
- `Option`, `Result`, tipos de função aninhados e recursos opacos dentro de assinatura de callback são rejeitados; use um adaptador de valor simples ou um wrapper Rust.
- O wrapper gerado empresta o callback durante a chamada (`&dyn Fn`), então o callback Lift continua utilizável depois.
- Um callback retornado por import fica armazenado como `Rc`; o lado Rust precisa retornar `Box<dyn Fn(...) -> ...>` e o wrapper converte com `Rc::from`.
- Callbacks não são ponteiros C: é um contrato de trait object Rust, não `extern "C"`.

## Limites

- Sem valores de função em campos, arrays, `Option` ou `Result`; sem coleções de callbacks.
- Sem captures mutáveis, igualdade/ordenação de funções, reflection, ponteiros para métodos ou callbacks async.
- Sem generics higher-order: tipos de função precisam ser concretos; uma assinatura de callback não pode ser parametrizada nem inferida além da anotação.
- Um adaptador Rust que exige `FnMut`, `FnOnce`, `impl Fn` genérico ou ownership do callback precisa de wrapper local; o contrato FFI é `&dyn Fn` em parâmetros e `Box<dyn Fn>` em retornos.
- Identidade de callback não é observável; copiar preserva o comportamento, mas não uma identidade endereçável.

Testes de parser, semântica, codegen e CLI cobrem anotações de tipo de função, armazenamento e cópias, funções nomeadas como valores, captures de callback, parâmetros higher-order, chamada imediata de callbacks retornados, parâmetros e retornos de callback FFI e os posicionamentos rejeitados. `examples/higher-order.lift` exercita o caminho executado no binário nativo.
