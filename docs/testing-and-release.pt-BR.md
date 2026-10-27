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
| Option e Result | `option_contract.rs`, `semantic_contract.rs`; E2E de coalesce, I/O e falhas de unwrap em `cli_end_to_end.rs` | tipos/FFI inválidos, fallback eager e unwrap da variante incorreta; narrowing e omissão de campos permanecem pendentes |
| Módulos e pacotes locais | `crates/lift_project/tests/project_contract.rs`, `packages_contract.rs`; E2E de módulos e Forklift em `cli_end_to_end.rs` | ciclos, paths, fronteiras, exports privados, arquivos ausentes e diagnósticos multifile |
| Rust gerado e FFI | `crates/lift_codegen/tests/codegen_contract.rs`; E2E de assinaturas e execução em `cli_end_to_end.rs` | caminho/assinatura Rust incompatível; o teste registry com crate fixada roda no aceite de release |
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
| Unions e enums | Não há tipos union nem enum definidos pelo usuário. | construção e uso de cada variante, igualdade/fluxo conforme o contrato, e variantes ou combinações inválidas rejeitadas. |
| Pattern matching | Não há sintaxe de matching em Lift. O `match` emitido internamente para `Option` não conta como recurso da linguagem. | matching de todos os tipos algébricos aceitos, cobertura exaustiva, arms inalcançáveis/duplicados e comportamento executado. |
| Construção e propagação de erros | `Result` não tem construtores Lift; `?` e `try/catch` não existem. `unwrap` pode causar panic na variante errada. | criação de sucesso/erro, propagação por chamadas aninhadas e captura, com saída/efeitos e diagnósticos definidos para cada fluxo. |
| Closures como valores e callbacks | Só há arrow closures tipadas locais com captura por snapshot; não podem ser armazenadas, retornadas ou recebidas como callbacks. | tipos de função, passagem/retorno/armazenamento, callbacks e captures com ownership verificados no Rust gerado e no resultado executado. |
| Narrowing de Option | `is_some` e comparação com `null` não refinam o tipo nos branches. | narrowing válido dentro do branch, sem escapar dele, incluindo joins, early return e loops; usos inseguros rejeitados antes do Rust. |
| Campos opcionais | Campo `T?` continua obrigatório; não há sintaxe para omiti-lo. | distinguir campo omitido, `null` e `Some`, validar inicialização e confirmar o mapeamento Rust em structs/FFI conforme o contrato. |

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
