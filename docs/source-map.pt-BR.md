# Mapa de arquivos-fonte

[English](source-map.md) · [Guia de uso](../README.pt-BR.md) · [Result e I/O](result-and-io.pt-BR.md)

O pipeline é `.lift` → tokens → AST não tipada → HIR tipado validado → Rust → Cargo → executável nativo. Cada módulo Rust possui uma visão geral em inglês e em português do Brasil. As tabelas descrevem cada arquivo-fonte, seus contratos e onde o comportamento é verificado.

## Workspace e compilador

| Arquivo | Responsabilidade |
| --- | --- |
| `Cargo.toml` | Membros do workspace, versão/edition compartilhadas e dependências entre crates. |
| `Cargo.lock` | Grafo fixado de dependências do compilador; instalação e empacotamento usam `--locked`. |
| `.gitignore` | Exclui artefatos, projetos gerados em qualquer profundidade, binários dos exemplos e ZIPs de distribuição. |
| `crates/*/Cargo.toml` | Dependências de cada crate; `lift_cli` declara executáveis `lift` e `forklift`. |
| `crates/lift_ast/src/lib.rs` | Declarações, statements, expressões, imports e tipos não tipados, incluindo `Result<T, E>` especial. Não contém parser ou semântica. |
| `crates/lift_ast/src/span.rs` | SourceId e spans de bytes UTF-8; construtores legados com fonte zero e joins no mesmo arquivo. |
| `crates/lift_lexer/src/lib.rs` | Tokens Logos, erros léxicos, offsets, escapes brutos de strings e testes do lexer. |
| `crates/lift_parser/src/lib.rs` | Entrada pública `parse`, retornando AST ou erros sintáticos acumulados. |
| `crates/lift_parser/src/parser.rs` | Declarações/statements por recursive descent, expressões Pratt, imports, argumentos de tipo Result e recuperação. |
| `crates/lift_parser/src/error.rs` | Categorias sintáticas, construções esperadas e spans de erro. |
| `crates/lift_sema/src/lib.rs` | HIR público, tipos resolvidos, modos de ownership, nomes de builtins Result, diagnósticos e export de `analyze`. |
| `crates/lift_sema/src/analyzer.rs` | Escopos, tipos, objetos nominais, funções, ownership conservador, aliasing, Result e produção do HIR. |
| `crates/lift_sema/src/interop.rs` | Valida tipos FFI, caminhos seguros, versões registry exatas e identidade de dependências. |
| `crates/lift_sema/src/liveness.rs` | Passagem privada CFG/liveness após validação e decisões restritas de move de local direto. |
| `crates/lift_codegen/src/lib.rs` | Emite Rust e manifestos Cargo com clones, temporários, wrappers FFI seguros e inspeção de Result. |
| `crates/lift_cli/src/lib.rs` | Comandos, diagnósticos, extração do core embutido, projetos gerados, Cargo, localização de artefatos e execução. |
| `crates/lift_cli/src/diagnostics.rs` | Relatórios Ariadne 0.6 com spans em bytes, labels relacionados, cores de terminal e testes do renderer. |
| `crates/lift_project/src/sources.rs` | Fontes/SourceIds e posições UTF-16 com LF/CRLF/CR. |
| `crates/lift_project/src/modules.rs` | Carga legada de módulos e seleção da análise manifestada. |
| `crates/lift_project/src/lib.rs` | Sessões reutilizáveis, overlays estáveis e resultados SourceMap/HIR/diagnósticos. |
| `crates/lift_project/src/forklift.rs` | Manifestos locais, identidade canônica de pacotes, dependências, fronteiras e snapshots. |
| `crates/lift_project/src/lint.rs`, `diagnostics.rs` | Regras consultivas HIR e códigos sema explícitos estáveis. |
| `crates/lift_sema/src/packages.rs` | Análise contextual sem I/O, exports, HIR qualificado e BindingIds globais. |
| `crates/lift_cli/src/lsp.rs`, `forklift.rs` | Servidor JSON-RPC de diagnósticos e entrada CLI Forklift local. |
| `crates/lift_cli/src/main.rs` | Propaga o resultado da CLI como código de saída do processo. |

AST e HIR mantêm spans da fonte. `analyze` não retorna HIR parcial após erro. O codegen consome decisões semânticas sem inferir ownership novamente. A lista de fontes embutidas do core é explícita na CLI; novos módulos do core também precisam entrar nela.

O workspace e os manifests gerados exigem Rust/Cargo 1.94.1+. Métodos nominais
impl, closures locais snapshot e interpolação contextual de templates atravessam
AST, lexer/parser, sema/HIR e codegen; veja [contratos e limites](methods-closures-templates.pt-BR.md).
`crates/lift_sema/tests/functions_contract.rs` valida captures lexicais, mutação
do receiver, rejeição de aliasing e spans UTF-8 aninhados. A integração da CLI
compila e executa `examples/methods-closures-templates.lift`, comparando stdout.

## Biblioteca padrão

| Arquivo | Responsabilidade |
| --- | --- |
| `stdlib/lift_core/Cargo.toml` | Biblioteca Rust sem dependências no workspace. A cópia embutida recebe manifesto próprio sem herança do workspace. |
| `stdlib/lift_core/src/lib.rs` | Módulos públicos e reexport de `Console`. |
| `stdlib/lift_core/src/console.rs` | Console Rust com `Display` e `io::Result`; `console.log` é um binding do compilador. |
| `stdlib/lift_core/src/array.rs` | Operações em slices emprestados, incluindo callbacks e referências exclusivos da API Rust. |
| `stdlib/lift_core/src/string.rs` | Helpers UTF-8 emprestados; comprimento conta valores escalares Unicode. |
| `stdlib/lift_core/src/interop.rs` | Comprimentos, trim owned e pertinência de arrays compatíveis com FFI; preserva a API Rust nativa. |
| `stdlib/lift_core/src/io.rs` | Argumentos/stdin/arquivos UTF-8 com `Result<T, String>` e erros contextualizados. |
| `stdlib/lift_core/README.md`, `README.pt-BR.md` | Guias da biblioteca para Rust e Lift nos dois idiomas. |
| `stdlib/lift_db/src/lib.rs` | Recurso SQLite não clonável, transações e snapshots de notas; API limitada à PoC. |
| `stdlib/lift_web/src/notes.rs` | DTOs HTTP owned, handlers síncronos nomeados, rotas locais e limite de body/concurrency. |
| `examples/notes-service/main.lift` | DTOs, handlers, rotas HTTP e operações SQLite escritos em Lift. |
| `stdlib/lift_web/src/http.rs` | Runtime genérico para bridges Lift e registro de rotas estáticas. |
| `stdlib/lift_db/src/resource.rs` | Protocolo de conexão/transação owned com commit, rollback e descarte RAII. |
| `crates/lift_sema/src/resources.rs` | Validação conservadora de fluxo afim e registro HTTP. |
| `scripts/verify_notes.py` | Aceite portátil HTTP/SQLite com reinício e porta ocupada. |
| `fuzz/Cargo.toml`, `fuzz/fuzz_targets/frontend.rs` | Alvo cargo-fuzz independente para fontes UTF-8 no lexer/parser/semântica e corpus inicial versionado. |
| `crates/lift_sema/tests/frontend_robustness.rs` | Truncamentos/mutações determinísticos limitados e invariantes de spans UTF-8. |
| `editors/vscode/test/` | Integração do Extension Host VS Code com o executável Lift/LSP real. |
| `.github/workflows/` | Matriz stable/MSRV/plataformas, formatação, fuzz periódico, cliente VS Code e aceite de release. |
| `examples/notes-service/db-smoke.lift` | Demonstra as operações SQLite fixas importadas pela FFI segura. |
| `examples/notes-service/README.md`, `README.pt-BR.md` | Instruções bilíngues para compilar, configurar e chamar a PoC HTTP/SQLite. |

## Testes de aceite e exemplos

| Arquivo | Cobertura ou propósito |
| --- | --- |
| `crates/lift_parser/tests/parser_contract.rs` | Precedência, postfixos, recuperação, ponto e vírgula, imports e sintaxe de Result. |
| `crates/lift_sema/tests/semantic_contract.rs` | Tipos, recursão, empréstimos, rejeição de aliasing, imports, builtins e validação de Result. |
| `crates/lift_sema/tests/frontend_robustness.rs` | Mutações/truncamentos reproduzíveis, ausência de panic e spans de tokens válidos. |
| `crates/lift_sema/tests/liveness_contract.rs` | Elegibilidade e contraexemplos conservadores em joins, retornos, loops, reatribuições, chamadas, parâmetros emprestados e projeções. |
| `crates/lift_sema/tests/option_contract.rs` | Tipos/null, modos de coalesce lazy, campos obrigatórios, limites de inferência, ciclos, rejeição FFI e ownership conservador. |
| `crates/lift_codegen/tests/codegen_contract.rs` | Contratos do Rust/Cargo gerados, wrappers, Result, clones e temporários de chamada. |
| `crates/lift_cli/tests/cli_end_to_end.rs` | Compila/executa exemplos, relocação, caminhos Cargo, argumentos/stdin/arquivos, Result, relatórios Ariadne, sessão LSP em processo, combinação de recursos e rejeição de aliasing. O teste registry usa a rede. |
| `docs/testing-and-release.pt-BR.md` | Matriz recurso→teste, gates de CI, comandos de fuzzing e aceite de release. |
| `examples/fibonacci.lift` | Controle de fluxo inteiro recursivo e iterativo. |
| `examples/ownership.lift` | Mutação e valores próprios sem notação explícita de borrow Rust. |
| `examples/interop.lift` | Imports locais seguros, arrays/strings emprestados e `console.log`. |
| `examples/stdlib.lift` | Comprimentos, pertinência, trim e iteração compatíveis com FFI. |
| `examples/io.lift` | Trata Result ao ler argumentos/stdin e gravar/ler arquivos UTF-8. |
| `examples/registry.lift` | Consome `num-integer` 0.1.46 do registry e imprime um MDC. |
| `examples/liveness.lift` | Decisões clone/move no controle de fluxo com saídas observáveis verificadas por rustc. |
| `examples/option.lift` | Contadores para LHS única/fallback lazy, arrays/campos opcionais e preservação de origens. |
| `examples/modules/main.lift` | Entrada com imports repetidos e execução única dos statements root. |
| `examples/modules/models.lift` | Interface/funções compartilhadas e declaração FFI Rust. |
| `examples/modules/math/api.lift`, `math/helpers.lift` | Imports relativos/transitivos, chamadas cruzadas e ciclos de módulo/root. |
| `scripts/package.ps1` | Compila para o host Rust atual e cria ZIP portátil com executável, guias e exemplos. |

Testes unitários ficam junto das implementações de spans, lexer e stdlib. Execute `cargo test --workspace` e o teste registry indicado no README; o workflow de release o executa automaticamente. Fuzzing e teste Extension Host têm workflows próprios. A aprovação demonstra os casos executados, sem provar todos os padrões de ownership ou APIs Rust possíveis.

## Registros de decisões

Os registros internos ficam em `.agents/skills/truxt-compiler/` (ID legado da skill): `SKILL.md` define o fluxo de contribuição; `references/architecture.md` é o blueprint alvo; as decisões de fases 1/2/3 descrevem sintaxe e semântica implementadas; `codegen-cli-decisions.md` registra lowering/Cargo; `phase5-decisions.md` registra imports seguros limitados e console. O contrato atual de Result/I/O está em `docs/result-and-io.pt-BR.md` e sua versão inglesa. Um item no roadmap não significa que esteja implementado.

Diagnósticos/reuso estão em `docs/diagnostics-and-build.pt-BR.md`; módulos legados em `docs/modules.pt-BR.md`; sessão/lint/LSP em `docs/project-analysis-and-lsp.pt-BR.md`; manifestos/namespaces locais em `docs/forklift.pt-BR.md`. Todos têm versão inglesa. Testes de contrato/API e `examples/packages/app` cobrem o incremento de pacotes locais. Cache por chave de dependências continua futuro.
