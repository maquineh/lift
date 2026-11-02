# Construção de Result, propagação e `try`/`catch`

[English](errors.md) · [Guia de uso](../README.pt-BR.md)

Lift já expõe o tipo embutido `Result<T, E>` para operações falíveis. Este incremento acrescenta o caminho de erro: construtores, o operador postfix `?`, `throw` e um `try`/`catch` tipado que mapeia para controle de fluxo Rust sem panic.

```lift
import function maybe_parse(text: string): Result<int?, string> from "lift_core::interop::maybe_parse";

function parse_positive(text: string): Result<int, string> {
    const parsed = maybe_parse(text)?;   // propaga Err ao chamador
    const value = parsed ?? 0;
    if value <= 0 { throw "not positive"; } // constrói Err("not positive")
    return result.ok(value);                // constrói Ok(value)
}

function describe(text: string): string {
    try {
        const value = parse_positive(text)?; // tem como alvo o catch abaixo
        return `ok ${value}`;
    } catch (error: string) {
        return `error ${error}`;
    }
}
```

## Construtores

`result.ok(value)` e `result.err(error)` exigem contexto `Result<T, E>`: anotação, `return`, argumento de função ou outra posição checada. Não há inferência a partir do construtor isolado, porque só o tipo esperado determina o outro payload.

```lift
const value: Result<int, string> = result.ok(1);
const failure: Result<int, string> = result.err("bad");
function nothing(): Result<void, string> { return result.ok(); }
```

- `result.ok(value)` checa `value` contra `T` e produz `Ok(value)`; o payload é materializado como valor Lift owned.
- `result.ok()` sem argumento só vale quando `T` é `void`, produzindo `Ok(())`.
- `result.err(error)` checa `error` contra `E` e produz `Err(error)`.
- Aridade e tipos de payload são checados antes do codegen. `result` não pode estar sombreado por variável, função ou interface.
- `result.ok`/`result.err` são construtores, não valores de função. Construtor sem contexto, inclusive direto sob `?`, é rejeitado; anote o valor primeiro.

## Propagação com `?`

`expressão?` consome um `Result<T, E>` e produz `T` no sucesso. Em `Err(e)`, transfere o controle para o handler compatível mais próximo:

1. o `try` interno cujo tipo do catch é exatamente `E`, ou
2. a função atual quando o retorno declarado é `Result<_, E>` com esse mesmo erro, ou
3. um diagnóstico Lift (`sema.InvalidPropagation`).

Os tipos de erro precisam ser iguais; não há conversão implícita `From`. `?` é exclusivo de Result — não é optional chaining de TypeScript, e `Option<T>?` é rejeitado.

O ownership segue a semântica de valor de Lift: um place `Result` é clonado antes da propagação quando os payloads são clonáveis, preservando a origem; um `Result` com recurso opaco é movido, porque recursos são afins. O resultado de uma chamada é usado diretamente, sem clone extra.

```lift
function total(text: string): Result<int, string> {
    const first = parse_positive(text)?;
    const second = parse_positive(text)?; // `first` continua disponível
    return result.ok(first + second);
}
```

Dentro de um `try`, `?` tem como alvo o catch quando o tipo de erro coincide; senão cai para a função externa se o tipo de erro dela coincide. Assim, um `try` com outro tipo de catch não engole silenciosamente um erro não relacionado.

## `throw`

`throw valor;` é um statement divergente que produz `Err(valor)` para os mesmos alvos de `?`: o `try` mais próximo com catch do tipo coincidente, senão o retorno `Result<_, E>` da função. `throw` sem alvo compatível, ou com tipo de valor diferente do erro do alvo, é rejeitado antes do codegen. `throw` exige ponto e vírgula final.

`throw` e `?` nunca causam panic. `result.unwrap`/`result.unwrap_err` continuam sendo as operações explícitas que causam panic.

## `try` / `catch`

```text
try { ... } catch (error: E) { ... }
```

- O tipo do parâmetro do catch é explícito e precisa ser tipo de valor (não `void`).
- Os dois blocos têm escopos separados. Lift não tem sombreamento, então o nome do catch não pode reutilizar um nome visível; duplicata é erro comum de declaração.
- O binding do catch se comporta como parâmetro owned: pode ter campos mutados (o padrão Rust gerado usa `mut`), mas o binding em si não pode ser reatribuído diretamente.
- Um corpo `try` bem-sucedido continua depois do `try`/`catch`. Qualquer `throw` ou `?` compatível no corpo transfere para o catch; depois do catch, a execução continua após o statement.
- `return` dentro do corpo do `try` retorna da função e ignora o catch, como em JavaScript.
- `throw`/`?` dentro do catch têm como alvo o próximo `try` externo ou o retorno da função, não o mesmo catch.
- Não há `finally`, catch sem tipo, múltiplos catches nem valor de expressão `try`; `try` é statement.
- Blocos `try` aninhados são suportados; cada `?`/`throw` tem como alvo o catch interno compatível.

O lowering usa um bloco Rust rotulado cujo valor é `Result<(), E>`: `?`/`throw` no corpo fazem `break` com `Err`, e um `match` despacha para o catch. Quando a análise semântica prova que o statement inteiro retorna da função, o codegen fecha a divergência com `unreachable!()` para satisfazer o tipo de retorno declarado.

## Garantia de retorno

Uma função que retorna `Result<T, E>` continua precisando retornar em todos os caminhos. Um `try`/`catch` garante retorno quando o corpo sempre sai (retorna ou lança) e o catch retorna; um corpo que sempre retorna não exige catch com retorno, porque o catch é inalcançável. As regras usuais de `if`/`else` valem dentro dos dois blocos.

## Ownership e liveness

- `?` clona um place `Result` clonável e move um resultado de recurso; o HIR registra o clone/move explicitamente, então o codegen não refaz análise de ownership.
- O passe privado de liveness trata cada `try`/`catch` como um ponto conservador: usos nos dois blocos contam, mas clones internos não viram moves neste incremento. A correção não depende dessa otimização.
- Diagnósticos existentes de borrow, alias e recursos afins continuam ativos. Consumo de recurso no corpo do `try` é propagado conservadoramente para o catch e para a continuação.

## Limites

- Sem exceções ou stack unwinding: `throw` constrói `Err` e `try`/`catch` é um match de Result.
- Sem `?` em `Option`, sem conversão `From` de erro, sem `finally`, sem múltiplos/typed catch arms, sem pattern matching além do `match` de `Option`/`Result`, e sem valor de expressão `try`.
- Construtores exigem contexto `Result<T, E>`; `result.ok(1)?` direto é rejeitado até o valor ser anotado.
- O payload de sucesso é `void` apenas em `result.ok()`; `result.err()` sempre exige valor de erro.
- O tipo do catch precisa coincidir exatamente com o erro propagado; erros não relacionados propagam para handlers externos ou para o retorno da função em vez de serem capturados.

Testes de parser, semântica, codegen e CLI cobrem construtores, exigência de contexto, alvos de propagação, mismatch de tipo de erro, throw, mutabilidade do catch, `try` aninhado, garantia de retorno, move de recurso versus clone, propagação no nível da função e execução nativa de [`examples/errors.lift`](../examples/errors.lift).
