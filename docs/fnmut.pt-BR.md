# Captures mutáveis (`FnMut`)

[English](fnmut.md)

Closures podem mutar variáveis `let` capturadas. Cada closure possui um snapshot mutável das captures, armazenado como `Rc<RefCell<T>>` no Rust gerado, então mantém estado entre chamadas e continua compatível com a representação de callback `Rc<dyn Fn>`.

```lift
function makeCounter(): () => int {
    let count = 0;
    return (): int => {
        count = count + 1;
        return count;
    };
}

const counter = makeCounter();
console.log(counter()); // 1
console.log(counter()); // 2
const other = makeCounter();
console.log(other());   // 1
```

## Semântica e limites

- Atribuições a captures são permitidas; o snapshot é inicializado na criação da closure e persiste dentro dela (inclusive em clones `Rc`).
- Mutações **não** atualizam a variável externa original: captures continuam sendo snapshots. Acumule estado externo pelo retorno da closure, `reduce` ou `for...of` explícito.
- Closures diferentes sobre a mesma variável recebem snapshots independentes; dois contadores são independentes.
- A closure continua implementando `Fn`; a mutação usa mutabilidade interior (`Rc<RefCell<T>>`), então métodos de array/callbacks não mudam de assinatura.
- Closures aninhadas capturam um snapshot de leitura da capture mutável externa; compartilhar a mesma célula entre closures aninhadas não é suportado.
- Atribuição à variável inteira capturada é suportada para tipos de valor (primitivos, strings, structs, enums), inclusive snapshots não-`Copy`. Mutar campo/índice capturado ou chamar método mutável em capture é rejeitado. Captures de recursos continuam rejeitadas; parâmetros de closure já são mutáveis quando o corpo atribui.

Testes: `crates/lift_sema/tests/enum_contract.rs` (marcador de mutação), E2E `examples/fnmut.lift`; contratos de higher-order/array-methods cobrem captures imutáveis.
