# Valores opcionais e coalescência preguiçosa

[English](option.md) · [Guia de uso](../README.pt-BR.md)

Truxt escreve tipos opcionais como `T?` e gera Rust `Option<T>`. `null` é literal reservado para a ausência única (`None`), somente onde o tipo do payload é inferível. Não há ausência `undefined`, conversão implícita `T`→`T?` ou sintaxe genérica geral `Option<T>` em Truxt.

```truxt
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9); // 7
console.log(option.some(false) ?? true); // false
```

`option.some(expr)` constrói payload presente e owned. `option.is_some(value)` empresta o opcional e retorna `boolean`. `option.unwrap(value)` empresta e clona o payload, preservando a origem; unwrap de `None` causa panic explícito. O namespace builtin `option` não pode estar ocupado por variável, função ou interface homônima.

## Tipos e campos obrigatórios

| Tipo Truxt | Tipo Rust | Significado |
| --- | --- | --- |
| `int?` | `Option<i64>` | Inteiro opcional. |
| `int?[]` | `Vec<Option<i64>>` | Array de inteiros opcionais. |
| `int[]?` | `Option<Vec<i64>>` | Array opcional de inteiros. |

Tipos de valor não void já suportados podem ser payloads. `void?`, opcionais diretamente aninhados (`T??` ou `option.some(valor_opcional)`) e ciclos de tamanho direto como `interface Node { next: Node? }` são rejeitados. Array continua sendo indireção; `Node?[]` não cria esse ciclo de tamanho.

Um campo `name: string?` continua obrigatório em todo object literal. Escreva `name: null` ou `name: option.some("Ada")`. `name?: string` e omissão de campos opcionais não foram implementados. `const value: int? = 1` é erro; use `option.some(1)`.

## Contrato de `??`

- `T? ?? T` retorna `T`.
- `T? ?? T?` retorna `T?`, permitindo cadeias `a ?? b ?? fallback`.
- A esquerda executa exatamente uma vez. A direita executa somente para `None`.
- `Some(false)`, `Some(0)` e `Some("")` preservam o payload; não há teste de truthiness.
- `??` associa à esquerda e tem precedência menor que `||`. Misturar `??` com `&&`/`||` sem parênteses é erro, seguindo a restrição estilo TS. Use `(value ?? false) || other` ou `value ?? (first && second)`.

`null` recebe contexto opcional de anotação, campo, argumento/retorno de função ou operando de igualdade opcional. `null ?? 42` infere payload do fallback imediato. `const value = null` e `null ?? null` sem contexto são erros. A inferência é local: contexto esperado não atravessa uma esquerda não-null de outro coalesce; portanto `const value: int = null ?? null ?? 1` é rejeitado. Anote um opcional intermediário.

Não há narrowing: após `if option.is_some(value)`, `value` continua `T?`. Extraia com unwrap ou `??`. Igualdade com `null` contextual também não refina tipo. Não há optional chaining, pattern matching, propagação automática ou Option no FFI. Parâmetros, retornos e Option dentro de arrays/Result no FFI são rejeitados antes do Rust.

## Ownership e validação

O HIR registra `OptionSome` owned, `OptionNone` tipado e `Coalesce` próprio com modo explícito de retorno valor/opcional. A esquerda vira snapshot owned pelas regras existentes de cópia/clone. Codegen emite `match` nesse snapshot e mantém o fallback owned somente no braço `None`, sem closure nem inferência nova de ownership. O opcional original continua disponível segundo a semântica de valores Truxt.

Liveness observa ambos os operandos possíveis conservadoramente e preserva clones dentro de coalesce e Some neste incremento. Isso não torna o fallback incondicional em runtime nem prova narrowing. Diagnósticos de borrow/alias continuam ativos.

Testes parser/sema/codegen/CLI cobrem sufixos, tipos, campos obrigatórios, inferência de null, rejeição FFI, ciclos, unwrap inválido e efeitos lazy/exatamente uma vez. `examples/option.truxt` muda contadores para distinguir caminhos e verifica payloads false/zero/string vazia no binário nativo.
