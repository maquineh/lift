# Match de Option e Result

[English](match.md) · [Guia de uso](../README.pt-BR.md)

`match` inspeciona um `Option<T>` ou `Result<T, E>` e executa exatamente um braço. As duas variantes precisam estar presentes, então um `match` aceito é exaustivo.

```lift
function describe(value: int?): string {
    match value {
        some(present) => { return `some ${present}`; }
        none => { return "none"; }
    }
}

function describe_result(value: Result<int, string>): string {
    match value {
        ok(number) => { return `ok ${number}`; }
        err(error) => { return `err ${error}`; }
    }
}
```

`Result<void, E>` usa `ok()` sem binding:

```lift
function make_void(): Result<void, string> { return result.ok(); }

function describe_void(): string {
    match make_void() {
        ok() => { return "void ok"; }
        err(error) => { return `err ${error}`; }
    }
}
```

## Sintaxe e patterns

```text
match expressão {
    some(nome) => { ... }
    none => { ... }
}
```

```text
match expressão {
    ok(nome) => { ... }   // ou ok() para sucesso void
    err(nome) => { ... }
}
```

- Braços usam `=>` seguido de bloco. Braços são separados por espaço ou vírgula opcional; não há fallthrough.
- `some(nome)` e `err(nome)` exigem nome de binding; `none` não tem parênteses.
- `ok(nome)` liga o payload de sucesso, e `ok()` só vale quando o sucesso é `void` (como em `Result<void, E>`). Usar a forma errada é rejeitado.
- O binding é um valor novo imutável com escopo no braço. Campos podem ser mutados (`mut` é emitido no pattern Rust), mas o binding em si não pode ser reatribuído. Prefixe com `_` se o payload for ignorado de propósito.
- Lift não tem wildcard, guard, literal, pattern aninhado ou múltiplos patterns aqui; exatamente um `some` e um `none`, ou um `ok` e um `err`, é obrigatório. Braço duplicado ou ausente é erro (`sema.InvalidMatch`).
- `match` é statement; braços não produzem valor. Use `return` ou atribuição dentro do braço.

## Avaliação e semântica de valor

- O scrutinee é avaliado uma vez. Se for um place, é clonado antes do match, então o opcional/result original continua disponível e mutações posteriores não são observadas pelo match.
- O payload é um valor owned retirado desse clone; não é emprestado.
- Fazer match de `Result<T, E>` que contém recurso opaco é rejeitado: use `?` ou `unwrap`/`unwrap_err` para Results com recurso. Arrays de closures/recursos continuam indisponíveis como payloads pelos próprios contratos.
- O Rust gerado é um `match` sobre `Option`/`Result` com os patterns dos braços; o binding segue as mesmas regras de valor owned dos demais bindings Lift.

## Fluxo de controle

- Um `match` garante retorno da função quando **todos** os braços sempre retornam ou lançam; a função pode então omitir o `return` final. Se algum braço termina normalmente, a checagem normal de caminhos de retorno se aplica.
- `return` dentro de um braço retorna da função. `throw` e `?` dentro de um braço miram o `try` externo ou o retorno `Result<_, E>` da função, como de costume.
- `break`/`continue` dentro de um braço miram o laço externo (os labels de laço fazem isso atravessar blocos `try`); `match` em si não cria laço.
- `match` pode aparecer em laço, corpo/catch de `try`, braço aninhado ou corpo de closure. Dentro de closure, `break`/`continue` continuam inválidos e `return` retorna da closure.

## Erros

Rejeitados antes do codegen:

- Variante ausente ou duplicada (`sema.InvalidMatch`), ou pattern da família errada (`ok` em `Option`, `some` em `Result`).
- `ok()` em sucesso não-`void`, ou `ok(nome)` em `void`.
- Match de valor que não é `Option<T>` nem `Result<T, E>`.
- Binding do braço sombreando um nome ativo.
- Reatribuir o binding do braço em si (campos continuam mutáveis).
- Caminho de função que ainda não retorna depois de um match que não garante retorno.
- Match de `Result`/`Option` contendo recurso opaco.

## Limites

- Apenas `Option` e `Result` podem ser alvo; ainda não há unions/enums de usuário, match de primitivos ou sintaxe geral de patterns.
- Sem valor de expressão `match`, guardas, patterns `or`, patterns aninhados de payload ou braço wildcard `_` (ignore um payload com binding prefixado por `_`).
- Sem match de payload com recurso, sem mutação do próprio binding e sem `match` como expressão constante/inicializador.

Testes de parser, semântica, codegen e CLI cobrem exaustividade, patterns duplicados/incompatíveis, sucesso `void`, semântica de clone, mutação de campo, garantia de retorno, controle de laço, throw em braços e execução nativa de [`examples/match.lift`](../examples/match.lift).
