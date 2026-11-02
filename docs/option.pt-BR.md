# Valores opcionais, campos opcionais e coalescência preguiçosa

[English](option.md) · [Guia de uso](../README.pt-BR.md)

Lift escreve tipos opcionais como `T?` e gera Rust `Option<T>`. `null` é literal reservado para a ausência única (`None`), somente onde o tipo do payload é inferível. Não há ausência `undefined`, conversão implícita `T`→`T?` ou sintaxe genérica geral `Option<T>` em Lift.

```lift
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9); // 7
console.log(option.some(false) ?? true); // false
```

`option.some(expr)` constrói payload presente e owned. `option.is_some(value)` empresta o opcional e retorna `boolean`. `option.unwrap(value)` empresta e clona o payload, preservando a origem; unwrap de `None` causa panic explícito. O namespace builtin `option` não pode estar ocupado por variável, função ou interface homônima.

## Tipos e campos opcionais

| Tipo Lift | Tipo Rust | Significado |
| --- | --- | --- |
| `int?` | `Option<i64>` | Inteiro opcional. |
| `int?[]` | `Vec<Option<i64>>` | Array de inteiros opcionais. |
| `int[]?` | `Option<Vec<i64>>` | Array opcional de inteiros. |
| `User?` | `Option<User>` | Struct opcional. |

Tipos de valor não void já suportados podem ser payloads. `void?`, opcionais diretamente aninhados (`T??` ou `option.some(valor_opcional)`) e ciclos de tamanho direto como `interface Node { next: Node? }` são rejeitados. Array continua sendo indireção; `Node?[]` não cria esse ciclo de tamanho.

Um campo de interface tem duas noções independentes: o tipo do valor `T?` e a declaração `?:`. Um campo `name: string?` continua obrigatório em todo object literal; escreva `name: null` ou `name: option.some("Ada")`. Um campo `nickname?: string` tem o mesmo tipo Rust (`Option<String>`), mas pode ser omitido; a omissão vira `None`, e `nickname: null` ou `nickname: option.some("Ada")` continuam aceitos. Não há lift implícito: `nickname: "Ada"` é erro de tipo porque o esperado é `string?`. `name?: string?` é rejeitado como opcional aninhado, e `name?: void` é rejeitado porque `void` não é tipo de valor.

Campos opcionais participam das mesmas operações de qualquer `T?`: `??`, `== null`/`!= null`, `option.is_some` e `option.unwrap`. Struct literals sempre inicializam todos os campos no Rust gerado: campos informados preservam a ordem da fonte e campos opcionais omitidos entram ao final como `None` explícito. `const value: int? = 1` continua sendo erro; use `option.some(1)`.

## Narrowing de fluxo

Um teste de fluxo sobre um binding imutável simples refina seu tipo dentro da região testada. O nome estreitado passa a ter tipo `T`, enquanto o opcional original mantém valor e tipo:

```lift
function greet(value: string?): string {
    if value == null { return "anonymous"; }
    return "hello, " + value; // `value` é `string` aqui
}

function pick(first: int?, second: int?): int {
    if first != null {
        return first + (second ?? 0); // `first` é `int`
    }
    return 0;
}
```

Testes aceitos:

- `option.is_some(x)`, `x != null` e `null != x` estreitam `x` no braço verdadeiro.
- `x == null` e `null == x` estreitam `x` no braço `else`.
- `!condição` troca os fatos verdadeiros e falsos.
- `a && b` estreita com todos os fatos dos dois operandos no braço verdadeiro.
- `a || b` estreita com todos os fatos falsos dos dois operandos no braço `else`.
- Parênteses são transparentes.
- Um `if` com braço que sempre retorna estreita o restante do bloco no outro caminho; `if x == null { return; }` deixa `x` não opcional depois.
- Cadeias `else if` levam os fatos falsos para os ramos da próxima condição, inclusive quando o próximo braço retorna.
- Fatos verdadeiros de um `while` estreitam o corpo do laço; nada persiste depois do laço.
- Closures declaradas dentro da região estreitada capturam o valor estreitado como snapshot somente leitura.

Narrowing exige binding imutável: `const`, parâmetro ou snapshot de captura de closure. Bindings `let` não estreitam, porque podem ser reatribuídos. Somente um nome simples estreita; projeções de campo e array (`user.nickname`, `items[0]`) não. O narrowing não vale dentro da própria expressão da condição: `if value != null && value > 0` rejeita o operando direito porque `value` ainda é `int?` ali. Use um `if` aninhado ou `option.unwrap` explícito quando a condição composta precisar combinar teste e uso.

O binding estreitado é um clone somente leitura do payload gerado pelo compilador. Mutá-lo alteraria apenas uma cópia, então Lift rejeita atribuições através do valor estreitado, inclusive escrita de campo/índice e chamadas que receberiam o valor por referência mutável:

```lift
if value != null {
    value = option.some(1); // erro: valor estreitado é somente leitura
}
```

O opcional original nunca é movido nem consumido pelo narrowing. Como cada região lê um clone, o payload segue a semântica de valor de Lift; o Rust gerado clona o payload uma vez no início da região e só é válido porque o teste de fluxo provou `Some`. Não há narrowing de `Result` (`result.is_ok`), optional chaining ou propagação automática; `match` trata `Option`/`Result` explicitamente (veja [match de Option e Result](match.pt-BR.md)), mas não é narrowing de fluxo. Um parâmetro/retorno `T?` direto é suportado por imports Rust, mapeado para `Option<&T>` ou o option owned; um `Option` aninhado em array continua fora do FFI, enquanto um `Option` direto dentro de payload de `Result` é suportado em parâmetros e retornos. Consulte [structs externas e Option em imports Rust](ffi-structs-and-options.pt-BR.md).

## Contrato de `??`

- `T? ?? T` retorna `T`.
- `T? ?? T?` retorna `T?`, permitindo cadeias `a ?? b ?? fallback`.
- A esquerda executa exatamente uma vez. A direita executa somente para `None`.
- `Some(false)`, `Some(0)` e `Some("")` preservam o payload; não há teste de truthiness.
- `??` associa à esquerda e tem precedência menor que `||`. Misturar `??` com `&&`/`||` sem parênteses é erro, seguindo a restrição estilo TS. Use `(value ?? false) || other` ou `value ?? (first && second)`.

`null` recebe contexto opcional de anotação, campo, argumento/retorno de função ou operando de igualdade opcional. `null ?? 42` infere payload do fallback imediato. `const value = null` e `null ?? null` sem contexto são erros. A inferência é local: contexto esperado não atravessa uma esquerda não-null de outro coalesce; portanto `const value: int = null ?? null ?? 1` é rejeitado. Anote um opcional intermediário.

## Ownership e validação

O HIR registra `OptionSome` owned, `OptionNone` tipado, um nó `Coalesce` próprio com modo explícito de retorno valor/opcional e um statement `Narrow` para cada região de narrowing. Um `Narrow` liga um binding novo somente leitura a um clone do payload do opcional; todas as leituras posteriores do nome naquela região usam o binding derivado. A esquerda de `??` vira snapshot owned pelas regras existentes de cópia/clone; o codegen emite `match` nesse snapshot e mantém o fallback owned somente no braço `None`, sem closure nem inferência nova de ownership.

O liveness trata `Narrow` como definição do binding derivado e uso do opcional de origem, portanto a origem continua disponível e o clone obrigatório do payload nunca é otimizado para move. As demais decisões de último uso permanecem iguais. Diagnósticos de borrow/alias continuam ativos, e mutação através do valor estreitado é rejeitada explicitamente.

Testes de parser/sema/codegen/CLI cobrem sufixos, tipos, campos obrigatórios versus opcionais, inferência de null, rejeição de FFI aninhada e struct local, ciclos, unwrap inválido, narrowing em ramos, guardas, cadeias `else if`, laços e capturas de closure, mutações rejeitadas e efeitos lazy/exatamente uma vez. `examples/option.lift` muda contadores para distinguir caminhos e verifica payloads false/zero/string vazia, omissão de campos opcionais e valores estreitados no binário nativo.
