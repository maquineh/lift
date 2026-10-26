# Mapa de arquivos-fonte

[English](source-map.md) · [Guia de uso](../README.pt-BR.md) · [Result e I/O](result-and-io.pt-BR.md)

O pipeline é `.truxt` → tokens → AST não tipada → HIR tipado validado → Rust → Cargo → executável nativo. Cada módulo Rust possui uma visão geral em inglês e em português do Brasil. As tabelas descrevem cada arquivo-fonte, seus contratos e onde o comportamento é verificado.

## Workspace e compilador

| Arquivo | Responsabilidade |
| --- | --- |
| `Cargo.toml` | Membros do workspace, versão/edition compartilhadas e dependências entre crates. |
| `Cargo.lock` | Grafo fixado de dependências do compilador; instalação e empacotamento usam `--locked`. |
| `.gitignore` | Exclui artefatos, projetos gerados em qualquer profundidade, binários dos exemplos e ZIPs de distribuição. |
| `crates/*/Cargo.toml` | Dependências de cada crate; `truxt_cli` também declara o executável `truxt`. |
| `crates/truxt_ast/src/lib.rs` | Declarações, statements, expressões, imports e tipos não tipados, incluindo `Result<T, E>` especial. Não contém parser ou semântica. |
| `crates/truxt_ast/src/span.rs` | SourceId e spans de bytes UTF-8; construtores legados com fonte zero e joins no mesmo arquivo. |
| `crates/truxt_lexer/src/lib.rs` | Tokens Logos, erros léxicos, offsets, escapes brutos de strings e testes do lexer. |
| `crates/truxt_parser/src/lib.rs` | Entrada pública `parse`, retornando AST ou erros sintáticos acumulados. |
| `crates/truxt_parser/src/parser.rs` | Declarações/statements por recursive descent, expressões Pratt, imports, argumentos de tipo Result e recuperação. |
| `crates/truxt_parser/src/error.rs` | Categorias sintáticas, construções esperadas e spans de erro. |
| `crates/truxt_sema/src/lib.rs` | HIR público, tipos resolvidos, modos de ownership, nomes de builtins Result, diagnósticos e export de `analyze`. |
| `crates/truxt_sema/src/analyzer.rs` | Escopos, tipos, objetos nominais, funções, ownership conservador, aliasing, Result e produção do HIR. |
| `crates/truxt_sema/src/interop.rs` | Valida tipos FFI, caminhos seguros, versões registry exatas e identidade de dependências. |
| `crates/truxt_sema/src/liveness.rs` | Passagem privada CFG/liveness após validação e decisões restritas de move de local direto. |
| `crates/truxt_codegen/src/lib.rs` | Emite Rust e manifestos Cargo com clones, temporários, wrappers FFI seguros e inspeção de Result. |
| `crates/truxt_cli/src/lib.rs` | Comandos, diagnósticos, extração do core embutido, projetos gerados, Cargo, localização de artefatos e execução. |
| `crates/truxt_cli/src/diagnostics.rs` | Relatórios Ariadne 0.6 com spans em bytes, labels relacionados, cores de terminal e testes do renderer. |
| `crates/truxt_cli/src/sources.rs` | Posse de caminhos/textos canônicos e IDs únicos por compilação. |
| `crates/truxt_cli/src/modules.rs` | Imports relativos, deduplicação/ciclos, statements exclusivos da entrada e achatamento da AST. |
| `crates/truxt_cli/src/main.rs` | Propaga o resultado da CLI como código de saída do processo. |

AST e HIR mantêm spans da fonte. `analyze` não retorna HIR parcial após erro. O codegen consome decisões semânticas sem inferir ownership novamente. A lista de fontes embutidas do core é explícita na CLI; novos módulos do core também precisam entrar nela.

O workspace e os manifests gerados exigem Rust/Cargo 1.94.1+. Métodos nominais
impl, closures locais snapshot e interpolação contextual de templates atravessam
AST, lexer/parser, sema/HIR e codegen; veja [contratos e limites](methods-closures-templates.pt-BR.md).
`crates/truxt_sema/tests/functions_contract.rs` valida captures lexicais, mutação
do receiver, rejeição de aliasing e spans UTF-8 aninhados. A integração da CLI
compila e executa `examples/methods-closures-templates.truxt`, comparando stdout.

## Biblioteca padrão

| Arquivo | Responsabilidade |
| --- | --- |
| `stdlib/truxt_core/Cargo.toml` | Biblioteca Rust sem dependências no workspace. A cópia embutida recebe manifesto próprio sem herança do workspace. |
| `stdlib/truxt_core/src/lib.rs` | Módulos públicos e reexport de `Console`. |
| `stdlib/truxt_core/src/console.rs` | Console Rust com `Display` e `io::Result`; `console.log` é um binding do compilador. |
| `stdlib/truxt_core/src/array.rs` | Operações em slices emprestados, incluindo callbacks e referências exclusivos da API Rust. |
| `stdlib/truxt_core/src/string.rs` | Helpers UTF-8 emprestados; comprimento conta valores escalares Unicode. |
| `stdlib/truxt_core/src/interop.rs` | Comprimentos, trim owned e pertinência de arrays compatíveis com FFI; preserva a API Rust nativa. |
| `stdlib/truxt_core/src/io.rs` | Argumentos/stdin/arquivos UTF-8 com `Result<T, String>` e erros contextualizados. |
| `stdlib/truxt_core/README.md`, `README.pt-BR.md` | Guias da biblioteca para Rust e Truxt nos dois idiomas. |

## Testes de aceite e exemplos

| Arquivo | Cobertura ou propósito |
| --- | --- |
| `crates/truxt_parser/tests/parser_contract.rs` | Precedência, postfixos, recuperação, ponto e vírgula, imports e sintaxe de Result. |
| `crates/truxt_sema/tests/semantic_contract.rs` | Tipos, recursão, empréstimos, rejeição de aliasing, imports, builtins e validação de Result. |
| `crates/truxt_sema/tests/liveness_contract.rs` | Elegibilidade e contraexemplos conservadores em joins, retornos, loops, reatribuições, chamadas, parâmetros emprestados e projeções. |
| `crates/truxt_sema/tests/option_contract.rs` | Tipos/null, modos de coalesce lazy, campos obrigatórios, limites de inferência, ciclos, rejeição FFI e ownership conservador. |
| `crates/truxt_codegen/tests/codegen_contract.rs` | Contratos do Rust/Cargo gerados, wrappers, Result, clones e temporários de chamada. |
| `crates/truxt_cli/tests/cli_end_to_end.rs` | Compila/executa exemplos, relocação, caminhos Cargo, argumentos/stdin/arquivos, Result, relatórios Ariadne e verificação determinística de não reescrita. O teste registry é optativo por poder acessar a rede. |
| `examples/fibonacci.truxt` | Controle de fluxo inteiro recursivo e iterativo. |
| `examples/ownership.truxt` | Mutação e valores próprios sem notação explícita de borrow Rust. |
| `examples/interop.truxt` | Imports locais seguros, arrays/strings emprestados e `console.log`. |
| `examples/stdlib.truxt` | Comprimentos, pertinência, trim e iteração compatíveis com FFI. |
| `examples/io.truxt` | Trata Result ao ler argumentos/stdin e gravar/ler arquivos UTF-8. |
| `examples/registry.truxt` | Consome `num-integer` 0.1.46 do registry e imprime um MDC. |
| `examples/liveness.truxt` | Decisões clone/move no controle de fluxo com saídas observáveis verificadas por rustc. |
| `examples/option.truxt` | Contadores para LHS única/fallback lazy, arrays/campos opcionais e preservação de origens. |
| `examples/modules/main.truxt` | Entrada com imports repetidos e execução única dos statements root. |
| `examples/modules/models.truxt` | Interface/funções compartilhadas e declaração FFI Rust. |
| `examples/modules/math/api.truxt`, `math/helpers.truxt` | Imports relativos/transitivos, chamadas cruzadas e ciclos de módulo/root. |
| `scripts/package.ps1` | Compila para o host Rust atual e cria ZIP portátil com executável, guias e exemplos. |

Testes unitários ficam junto das implementações de spans, lexer e stdlib. Execute `cargo test --workspace` e o teste registry explícito indicado no README. A aprovação demonstra os casos executados, sem provar todos os padrões de ownership ou APIs Rust possíveis.

## Registros de decisões

Os registros internos ficam em `.agents/skills/truxt-compiler/`: `SKILL.md` define o fluxo de contribuição; `references/architecture.md` é o blueprint alvo; as decisões de fases 1/2/3 descrevem sintaxe e semântica implementadas; `codegen-cli-decisions.md` registra lowering/Cargo; `phase5-decisions.md` registra imports seguros limitados e console. O contrato atual de Result/I/O está em `docs/result-and-io.pt-BR.md` e sua versão inglesa. Um item no roadmap não significa que esteja implementado.

Diagnósticos/reuso estão em `docs/diagnostics-and-build.pt-BR.md`. Imports multi-arquivo e identidade de fonte estão em `docs/modules.pt-BR.md`, ambos com versões inglesas. Manifesto, namespaces e cache por chave de dependências continuam futuros.
