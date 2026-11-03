# Matriz de testes e aceite

[English](testing-and-release.md) · [Guia de uso](../README.pt-BR.md)

Esta matriz liga os contratos públicos do Lift aos testes que devem protegê-los.
Ela descreve o subconjunto Lift; passar esses testes não significa compatibilidade
completa com TypeScript nem com toda crate Rust.

## Contratos cobertos

| Área | Casos válidos | Casos inválidos e fronteiras |
| --- | --- | --- |
| Lexer, spans e parser | `crates/lift_lexer/src/lib.rs`, `crates/lift_parser/tests/parser_contract.rs` | erros léxicos, recuperação sintática, EOF e UTF-8 em `parser_contract.rs` e `crates/lift_sema/tests/frontend_robustness.rs` |
| Tipos, funções, objetos e fluxo | `crates/lift_sema/tests/semantic_contract.rs`, `functions_contract.rs`; exemplos executados em `cli_end_to_end.rs` | mismatch, nomes/imports inválidos, retornos e chamadas rejeitados antes do codegen |
| Ownership, métodos e closures locais | `liveness_contract.rs`, `functions_contract.rs`; `cross_feature_acceptance_matrix_compiles_and_runs_supported_patterns` | aliasing semântico em `ownership_aliasing_rejection_is_reported_before_rustc`; joins, loops, moves e projeções mantêm as decisões conservadoras do HIR |
| Option e Result | `option_contract.rs`, `semantic_contract.rs`; E2E de coalesce, campos opcionais, narrowing, I/O, unwrap, Option direto no FFI e parâmetro Result emprestado em `cli_end_to_end.rs` | tipos inválidos, fallback eager, unwrap da variante incorreta, mutação através de valor estreitado, `let`/projeções não estreitáveis, campos opcionais aninhados, narrowing dentro da própria condição e Option aninhado em arrays ou Option<Result> |
| Erros e propagação | `errors_contract.rs`; `examples/errors.lift` via check/build/run em `cli_end_to_end.rs` | construtores sem contexto ou com aridade errada, `?`/`throw` sem alvo compatível ou com tipo de erro divergente, `?` em Option, catch sombreado ou com tipo `void` e falta de retorno após `try` que pode terminar normalmente |
| Captures mutáveis | marcador de mutação em `enum_contract.rs`; E2E `examples/fnmut.lift` e `examples/capture-fields.lift` | mutação de variável/campo/método, estado `Copy` compartilhado, snapshots não-`Copy` isolados e estado persistente |
| Containers genéricos | `generic_containers_contract.rs`; E2E `examples/generic-containers.lift` | aridade/parâmetros duplicados, `Box<Box<T>>`, arrays/optionals, `Map`-shaped, uso desconhecido/não autorizado e substituição de tipos de campo |
| Genéricos | `generics_contract.rs`; `examples/generics.lift` via check/build/run e binário nativo em `cli_end_to_end.rs` | parâmetros duplicados/só no retorno, conflitos de inferência, containers/optionals genéricos, chamadas genéricas aninhadas, função como valor rejeitada, aritmética rejeitada e bounds Rust |
| Unions | `union_contract.rs`; `examples/unions.lift` via check/build/run e binário nativo em `cli_end_to_end.rs` | membros duplicados/primitivos/array, nomes de união duplicados, match exaustivo, construção por object literal, membros enum e uniões recursivas |
| Enums | `enum_contract.rs`; `examples/enums.lift`, `examples/enum-fields.lift` e `examples/recursive-enums.lift` via check/build/run e binário nativo em `cli_end_to_end.rs` | variantes duplicadas/vazias, aridade/tipo de construtor errados, campos nomeados ausentes/desconhecidos/duplicados, variantes ausentes/duplicadas/desconhecidas no match, binding count divergente, rejeição FFI/DbValue, recursão Box/campos de interface e semântica de igualdade/valor |
| Match de Option e Result | `match_contract.rs`; `examples/match.lift` via check/build/run e binário nativo em `cli_end_to_end.rs` | patterns ausentes/duplicados/incompatíveis, `ok()` vs `ok(nome)` com `void`, scrutinee que não é Option/Result, sombreamento, reatribuição do binding, Result com recurso e retorno ausente após match que pode terminar normalmente |
| Métodos de array | `array_methods_contract.rs`; `examples/array-methods.lift` via check/build/run e binário nativo em `cli_end_to_end.rs` | métodos desconhecidos, mismatch de parâmetro/resultado do callback, aridade errada, argumento que não é callback, `find` em elementos opcionais e receptor que não é array |
| Laços e controle de laço | `loops_contract.rs`; `examples/loops.lift` via check/build/run e binário nativo em `cli_end_to_end.rs` | `for...of` em não-array, `break`/`continue` fora de laço ou dentro de closure, mutação de binding `const` do laço, retorno ausente após laço e consumo de recurso externo dentro do laço |
| Tipos de função e callbacks | `higher_order_contract.rs`; `examples/higher-order.lift` via check/build/run e binário nativo em `cli_end_to_end.rs`; callback FFI em `ffi_structs_options_contract.rs` | callbacks em campos/arrays/Option/Result, valor de função com recursos, parâmetro callback `void`, captures mutáveis, assinatura de callback divergente e payloads FFI com Option/Result/tipo de função aninhado |
| Módulos e pacotes locais | `crates/lift_project/tests/project_contract.rs`, `packages_contract.rs`; E2E de módulos e Forklift em `cli_end_to_end.rs` | ciclos, paths, fronteiras, exports privados, arquivos ausentes e diagnósticos multifile |
| Rust gerado e FFI | `crates/lift_codegen/tests/codegen_contract.rs`, `ffi_structs_options_contract.rs` (semântica); E2E de assinaturas e execução em `cli_end_to_end.rs` | caminho/assinatura Rust incompatível, layout externo sem campos públicos ou `Clone + Debug + PartialEq`, struct local no FFI, Option aninhado em arrays, Result/Option<Result> aninhados, conflito de versões da crate e parâmetro `Result<void, E>`; o teste registry com crate fixada roda no aceite de release |
| Banco, recursos e HTTP | `crates/lift_sema/tests/resources_web_contract.rs`, `stdlib/lift_db/tests/api_contract.rs`, `scripts/verify_notes.py` | consumo/reuso de recursos, SQL inválido, respostas HTTP de erro e integração PostgreSQL separada |
| Projeto, diagnósticos e editor | `crates/lift_project/tests/project_contract.rs`, testes internos de `crates/lift_cli/src/lsp.rs`, E2E do processo CLI e Extension Host VS Code | fontes não salvas, UTF-16, mudança de versão, correção e limpeza de diagnósticos |

Ao adicionar uma construção, acrescente um caso aceito e um rejeitado na camada
apropriada. Recursos que chegam ao Rust devem ter pelo menos um aceite integrado
que compile e confira o resultado observável; rejeições semânticas devem verificar
que o erro aponta para a fonte Lift e ocorre antes de uma falha do `rustc`.

## Recursos pendentes que bloqueiam um aceite mais amplo

Estes são recursos ainda não implementados, não lacunas que os testes atuais
possam resolver. O aceite desta matriz protege o subconjunto documentado e não
deve ser interpretado como prontidão diária para código TypeScript arbitrário.

| Recurso | Estado atual | Aceite necessário depois da implementação |
| --- | --- | --- |
| Generics gerais | `Result<T, E>` é um tipo especial; não há parâmetros de tipo genéricos gerais. | declaração, instanciação, chamadas e tipos aninhados válidos; aridade e combinações inválidas rejeitadas antes do Rust. |
| Enums genéricos e funções sobre containers | Containers de interface são monomorfizados, mas enums genéricos, `function f<T>(box: Box<T>)`, argumentos explícitos, métodos e containers genéricos em FFI estão ausentes. | declaração/uso de enums genéricos e funções sobre containers, além de inferência inválida rejeitada. |
| Pattern matching geral | Lift tem `match` para `Option<T>`, `Result<T, E>` e enums nominais, com patterns fixos. Primitivos, guardas, wildcards, patterns aninhados e expressões `match` estão ausentes. | matching de todos os tipos algébricos aceitos, cobertura exaustiva, arms inalcançáveis/duplicados, guardas/aninhamento e comportamento executado. |


Cada item exige primeiro um contrato semântico explícito. Depois, o aceite deve
cobrir Lift → AST/HIR → Rust compilável → comportamento observável para casos
válidos e diagnósticos Lift para casos inválidos. Esses itens devem continuar
marcados como não suportados até que essa cadeia exista.

## Comandos e gates automatizados

- O workflow `notes-contract.yml` roda testes e o aceite HTTP/SQLite em Windows e
  Ubuntu, com `stable` e Rust 1.94.1. PostgreSQL 17 roda em job próprio.
- O job `format` verifica o workspace principal e os workspaces autônomos de
  `lift_db`, `lift_web` e `fuzz`.
- `release-acceptance.yml` roda no despacho manual e em tags `v*`; executa o aceite
  stable, Rust 1.94.1, formatação, PostgreSQL 17, HTTP/SQLite e o teste ignorado que
  baixa `num-integer` fixado no registry.
- `vscode-extension.yml` compila a CLI e executa o cliente real da extensão em
  VS Code estável no Linux/Xvfb, verificando diagnóstico sem salvar e sua limpeza.
- `frontend-fuzz.yml` roda semanalmente e por despacho manual em Linux/nightly.
  A entrada é UTF-8 e o job limita cada campanha a 60 segundos, 2 KiB por entrada
  e 5 segundos por caso. Mantenha falhas encontradas no corpus ou como regressões
  determinísticas.

Para rodar manualmente:

```sh
cargo test --locked --workspace
cargo fmt --all --check
cargo fmt --manifest-path stdlib/lift_db/Cargo.toml --check
cargo fmt --manifest-path stdlib/lift_web/Cargo.toml --check
cargo fmt --manifest-path fuzz/Cargo.toml --check
cargo test --locked -p lift_cli --test cli_end_to_end registry_import_consumes_a_real_pinned_rust_crate -- --ignored --exact
```

O alvo `cargo-fuzz` requer nightly e suporte Unix a sanitizers. Na raiz do
repositório, execute `cargo fuzz run frontend -- -max_len=2048
-max_total_time=60 -timeout=5`. A suíte de robustez determinística faz parte do
`cargo test --workspace`; cobre truncamentos em fronteiras UTF-8 e 512 fontes
aleatórias reproduzíveis de tamanho limitado. Nenhuma campanha curta prova
ausência de panics para profundidade ou tamanho irrestritos.
