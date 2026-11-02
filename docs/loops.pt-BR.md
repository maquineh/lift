# Laços: `for...of`, `while`, `break` e `continue`

[English](loops.md) · [Guia de uso](../README.pt-BR.md)

Lift tem dois statements de laço: `while` avalia uma condição booleana antes de cada iteração, e `for...of` itera um array. `break` e `continue` controlam o laço mais interno.

```lift
function first_positive(values: int[]): int? {
    for (const value of values) {
        if value > 0 { return option.some(value); }
    }
    return null;
}

function count_before(values: int[], limit: int): int {
    let count = 0;
    for (const value of values) {
        if value == limit { break; }
        if value < 0 { continue; }
        count = count + 1;
    }
    return count;
}

let index = 0;
while index < 3 {
    index = index + 1;
    if index == 2 { continue; }
    console.log(index); // 1, 3
}
```

## `for...of`

```text
for (const|let item of expression) { ... }
```

- `expression` precisa ter tipo array `T[]`; qualquer outra coisa é rejeitada antes do codegen.
- `const` torna o binding por iteração somente leitura. `let` permite reatribuir o binding dentro do corpo; isso não muta o array original.
- O binding pertence ao escopo do corpo e não pode sombrear um nome ativo. Uma declaração depois do laço pode reutilizar o nome.
- O array é avaliado uma vez e iterado como **snapshot owned**: o compilador clona o array antes do laço e cada elemento é um valor owned. Mudanças posteriores no array de origem não aparecem naquela iteração, e o laço nunca consome o binding de origem.
- Arrays de recursos opacos, closures, unions ou opcionais aninhados não podem ser iterados (esses tipos já são rejeitados em arrays ou pelo contrato de Option).
- Elementos de `T?[]` mantêm o tipo opcional; no corpo, o narrowing usual funciona quando o binding é `const`.

`for...of` sobre uma expressão temporária é permitido: `for (const value of [10, 20]) { ... }`.

## `while`

`while condição { ... }` mantém o contrato existente: a condição precisa ser `boolean`; fatos de condições que estreitam opcionais valem no corpo. Não existe `do...while`.

## `break` e `continue`

- `break;` sai do `while`/`for` mais interno; `continue;` inicia a próxima iteração do laço mais interno.
- Ambos exigem ponto e vírgula final e são rejeitados fora de laço, inclusive dentro de closure declarada dentro de um laço (closures são um corpo separado).
- Dentro de `try`, `break`/`continue` têm como alvo o laço e ignoram o `catch`; depois do laço, a execução continua normalmente. `break`/`continue` dentro do bloco `catch` também miram o laço externo.
- Ambos funcionam em `while` e `for...of`; não há labels, então um laço interno não pode dar `break` no externo.
- `break`/`continue` não contam como retorno da função: a função continua precisando de `return` final mesmo que o corpo do laço retorne em algum caminho, porque o laço pode executar zero vezes.

## Retorno, recursos e liveness

- `for...of`/`while` podem executar zero ou mais vezes, então nunca garantem retorno da função. `return` dentro do corpo retorna da função.
- O laço não pode consumir recurso opaco declarado fora dele; a regra existente para `while` também vale para o corpo de `for...of`. Recursos criados e consumidos dentro da mesma iteração são permitidos.
- Liveness trata cada laço conservadoramente: usos no corpo contam, e clones internos não viram moves no passe de último uso. O array de iteração é clonado uma vez antes do laço.
- O Rust gerado usa laços rotulados (`'lift_loop_N`) para que `break`/`continue` atravessem o bloco rotulado usado no lowering de `try`/`catch` sem mudar a semântica.

## Limites

- Sem `for` estilo C, `for...in` sobre índices/chaves, protocolo de iterator, ranges, labels ou expressão de laço.
- Sem `else` em laço e sem `break` com valor.
- O binding do laço é um valor, não uma referência ao slot do array; mutá-lo nunca muta o array de origem.
- Closures não podem usar `break`/`continue` de um laço externo.

Testes de parser, semântica, codegen e CLI cobrem tipagem e escopo do laço, iteração snapshot, mutação de binding const/let, alvos aninhados, `break`/`continue` em `try`/`catch`, rejeição em closure, análise de retorno ausente, consumo de recursos e execução nativa de [`examples/loops.lift`](../examples/loops.lift).
