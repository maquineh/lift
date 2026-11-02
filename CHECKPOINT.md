# Checkpoint do compilador Lift

- **Data:** 2026-11-02 (America/Manaus)
- **Branch:** `main`
- **Último commit antes deste checkpoint:** `7fee193 feat: add inferred generic functions`
- **Estado:** incrementos abaixo implementados, testados, documentados e reunidos neste checkpoint. O working tree fica limpo após o commit; `target/` e `.lift_cache/` são ignorados.

Este arquivo registra o ponto de retomada do compilador Lift (`.lift` → HIR tipado → Rust → binário). Ele complementa a matriz de testes e os contratos em `docs/`; não substitui essas fontes.

## Incrementos concluídos

1. **Option e narrowing** — campos opcionais (`campo?: T`, omitíveis) e obrigatórios (`campo: T?`), `null` contextual, `option.some/is_some/unwrap`, `??` preguiçoso, narrowing por fluxo em `if`, guardas, `else if`, `while` e captures de closure, com leitura somente leitura do payload.
2. **FFI com structs externas e Option direto** — `import interface`, layout externo com validação de campos públicos e traits, `T?` direto em parâmetro/retorno, mapeamentos `Option<&str>`/`Option<&[T]>`/`Option<&P>` e o exemplo `examples/interop.lift`.
3. **Result em parâmetros e payload opcional** — `Result<T, E>` emprestado em parâmetros FFI e um `Option` direto dentro do payload; aninhamentos maiores continuam rejeitados.
4. **Caminho de erro** — `result.ok`/`result.err` contextuais, `?` com alvo `try`/função, `throw`, `try`/`catch` tipado, garantia de retorno e controle de recursos.
5. **Higher-order/callbacks** — tipo de função `(T) => R`, closures e funções nomeadas como valores `Rc<dyn Fn>`, armazenamento/cópia/retorno, captures de callbacks, chamada imediata (`make()(3)`) e callbacks FFI (`&dyn Fn` em parâmetros, `Box<dyn Fn>` convertido para `Rc` no retorno).
6. **Laços** — `for (const|let item of array)` com snapshot owned, `while`, `break`/`continue` no laço mais interno, interação com `try`/`catch` e labels Rust `'lift_loop_N`.
7. **Métodos de array** — `map`, `filter`, `find`, `some`, `every`, `forEach` e `reduce`, monomorfizados por elemento, com callback nomeado ou closure e encadeamento.
8. **Match de Option/Result** — `match` exaustivo com `some(nome)`/`none` e `ok(nome)`/`err(nome)`, `ok()` para sucesso `void`, snapshot do scrutinee, mutação de campos do payload e garantia de retorno por braços.
9. **Enums nominais com payloads** — `enum Nome { Variante, Variante(Tipo, Tipo) }`, construção `Enum.Variant(...)`, igualdade estrutural, uso em arrays/parâmetros/retornos/Option/Result e `match` exaustivo com patterns qualificados `Enum.Variant(bindings)`.
10. **Enums com campos nomeados** — `Variante { campo: Tipo }`, construção `Enum.Variant({ campo: valor })`, validação de campos ausentes/desconhecidos/duplicados e match posicional na ordem de declaração.
11. **Recursão e enums em interfaces** — payloads recursivos com `Box` automático, enums mutuamente recursivos, ciclos struct↔enum e campos de enum em interfaces.
12. **Uniões nominais `type A = B | C`** — membros nominais desaçucarados para um enum, construção por `Shape.Circle(...)`, match exaustivo, membros enum e recursão.
13. **Funções genéricas** — `function f<T>(...)` com inferência por argumento, `T[]`/`T?`/`Result<T, E>`, chamadas genéricas aninhadas e bounds Rust `Clone + Debug + PartialEq`.
14. **Captures mutáveis/FnMut** — closures mutam captures `Copy` via `Rc<RefCell<T>>`, com estado persistente e independente por closure; mutações ficam no snapshot interno e captures não-Copy mutáveis são rejeitadas.

## Contratos e limites atuais

- `Result` exige correspondência exata de tipo de erro; não há conversão `From`, `finally`, múltiplos catches nem `try` como expressão.
- Funções são `Rc<dyn Fn>`; captures são snapshots somente leitura. Não há `FnMut`, captura mutável, igualdade/ordenação de funções nem closures em campos/arrays/Option/Result.
- Callbacks FFI são trait objects Rust (`&dyn Fn`/`Box<dyn Fn>`), não ponteiros C; assinaturas com `Option`/`Result`/aninhamento são rejeitadas.
- Laços não têm `for` estilo C, `for...in` sobre índices, labels de usuário, ranges ou protocolo de iterator; o array é sempre clonado como snapshot.
- Métodos de array não têm `flatMap`, `sort`, parâmetro de índice, iteradores lazy ou métodos mutantes (`push`/`pop`).
- `match` cobre `Option`/`Result` e enums nominais; não há wildcard, guardas, patterns aninhados, match de primitivos ou sintaxe de union TypeScript.
- Generics gerais continuam ausentes (`Result` é o único tipo genérico embutido).
- Enums e uniões suportam payloads posicionais/nomeados, recursão com `Box` automático, campos de interface e `type A = B | C`; não há membros primitivos/arrays/Option em uniões, tipos union inline, discriminantes explícitos, enums em FFI ou métodos em enums.
- Option não entra em arrays/Result aninhado; recursos opacos seguem as regras de ownership e não podem ser payload de `Option` comum nem scrutinee de `match`.

## Verificação executada neste checkpoint

```text
cargo fmt --all --check
cargo test --locked --workspace -- --skip db_value --skip opaque_resource_signature_materializes_db_without_an_explicit_import
```

- Formatação: limpa.
- Testes: **243 passando**, **39 suítes**, 0 falhas.
- E2E por `check`, `build`, `run` e binário nativo dos exemplos:
  - `examples/loops.lift` → `6 1 2 3 7 8 1 3 4 5 10 20`
  - `examples/array-methods.lift` → `2 8 1 2 4 3 true true 1 2 3 4 10 #3 20 200`
  - `examples/match.lift` → `some 3 / none / ok 8 / err negative / void ok / 5 / 1 / 1`
  - `examples/enums.lift` → `12.56 / circle 2 / 6 / rect 2x3 / 0 / empty / true / true / false`
  - `examples/enum-fields.lift` → `12.56 / 6 / 0 / circle 2 / true`
  - `examples/recursive-enums.lift` → `6 / 3 / true`
  - `examples/unions.lift` → `12.56 / 9 / true / false / north / other / 1`
  - `examples/generics.lift` → `42 / hi / true / 1 / true / false / 7 / 1 / 9 / 5 / 4`
  - `examples/fnmut.lift` → `1 / 2 / 3 / 1`
  - `examples/errors.lift` e `examples/higher-order.lift` também cobertos por E2E.
- `lift lint` em `examples/loops.lift`, `examples/array-methods.lift`, `examples/match.lift`, `examples/enums.lift`, `examples/enum-fields.lift` e `examples/recursive-enums.lift` e `examples/unions.lift` e `examples/generics.lift` e `examples/fnmut.lift`: sem avisos.
- Skips conhecidos e **pré-existentes** (não causados por estes incrementos): testes de banco cuja cadeia de dependências (`r2d2`/`scheduled-thread-pool`) pede rustc mais novo que o MSRV 1.94.1. Eles permanecem documentados na matriz de testes e devem ser reavaliados quando a dependência for atualizada.

## Onde retomar

- Contratos e guias:
  - [Option](docs/option.md) / [pt-BR](docs/option.pt-BR.md)
  - [Structs externas e Option na FFI](docs/ffi-structs-and-options.md) / [pt-BR](docs/ffi-structs-and-options.pt-BR.md)
  - [Result e I/O](docs/result-and-io.md) / [pt-BR](docs/result-and-io.pt-BR.md)
  - [Erros e propagação](docs/errors.md) / [pt-BR](docs/errors.pt-BR.md)
  - [Tipos de função e callbacks](docs/higher-order.md) / [pt-BR](docs/higher-order.pt-BR.md)
  - [Laços](docs/loops.md) / [pt-BR](docs/loops.pt-BR.md)
  - [Métodos de array](docs/array-methods.md) / [pt-BR](docs/array-methods.pt-BR.md)
  - [Match de Option/Result](docs/match.md) / [pt-BR](docs/match.pt-BR.md)
  - [Enums](docs/enums.md) / [pt-BR](docs/enums.pt-BR.md)
  - [Uniões](docs/unions.md) / [pt-BR](docs/unions.pt-BR.md)
  - [Funções genéricas](docs/generics.md) / [pt-BR](docs/generics.pt-BR.md)
  - [Captures mutáveis](docs/fnmut.md) / [pt-BR](docs/fnmut.pt-BR.md)
- Matriz de cobertura e pendências: [docs/testing-and-release.md](docs/testing-and-release.md).
- Mapa de crates, arquivos e exemplos: [docs/source-map.md](docs/source-map.md).
- Decisões internas: [phase3](.agents/skills/truxt-compiler/references/phase3-decisions.md), [phase4](.agents/skills/truxt-compiler/references/phase4-decisions.md) e [phase5](.agents/skills/truxt-compiler/references/phase5-decisions.md).
- Exemplos executáveis: `examples/option.lift`, `examples/interop.lift`, `examples/errors.lift`, `examples/higher-order.lift`, `examples/loops.lift`, `examples/array-methods.lift`, `examples/match.lift`, `examples/enums.lift`, `examples/enum-fields.lift`, `examples/recursive-enums.lift`, `examples/unions.lift`, `examples/generics.lift`, `examples/fnmut.lift`.

## Próximos passos sugeridos

1. **Containers genéricos** — estender genéricos para interfaces/structs/enums e argumentos de tipo explícitos; funções genéricas já concluídas.
2. **Captures mutáveis/`FnMut`** — concluído (snapshots internos via `Rc<RefCell<T>>`); falta compartilhar estado com a variável externa/closures aninhadas.
3. Polimento: wildcard/guardas no `match`, `flatMap`/`sort`/índice em callbacks, `finally`/`?` em Option, escapes Unicode, melhorias de LSP.
