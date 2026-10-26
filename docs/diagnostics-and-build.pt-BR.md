# Diagnósticos de fonte e reuso dos arquivos gerados

[English](diagnostics-and-build.md) · [Guia de uso](../README.pt-BR.md)

A CLI é a única crate do compilador que depende de Ariadne 0.6.0. Lexer/parser/sema continuam produzindo seus diagnósticos e spans de bytes semiabertos; AST e HIR não recebem dependência de apresentação. `diagnostics.rs` renderiza um relatório por erro, com label principal e label opcional `local relacionado` no mesmo relatório. Falhas de fonte continuam retornando código `1`.

Referência da API da dependência: [Ariadne 0.6.0](https://docs.rs/ariadne/0.6.0/ariadne/).

O renderer configura explicitamente `Config::with_index_type(IndexType::Byte)`. Os spans continuam offsets de bytes UTF-8, sem conversão para índices de caracteres. Spans EOF vazios permanecem vazios. Limites e fronteiras UTF-8 são normalizados defensivamente somente na renderização. Ariadne calcula linha/coluna e trecho exibidos. Cores só são ativadas para stderr interativo sem `NO_COLOR`; saída capturada não contém cores ANSI. Se a renderização falhar, a CLI tenta uma mensagem simples com caminho e mantém a falha.

Isso cobre erros léxicos, sintáticos e semânticos na entrada e nos módulos importados. SourceId seleciona a fonte de cada label, inclusive labels secundários em outro arquivo. Diagnósticos Cargo/rustc mantêm a renderização do Cargo e apontam para Rust gerado, sem mapeamento Rust→Truxt. Testes verificam Unicode, EOF, códigos de saída, trechos e labels sem depender de snapshot completo.

## Reuso dos bytes gerados

A CLI lê `src/main.rs` e `Cargo.toml` existentes e só grava quando os bytes diferem. Arquivos ausentes são criados; falhas de leitura/escrita continuam sendo erros da CLI. O mesmo helper preserva arquivos inalterados do core embutido. Não há banco de hashes ou estado global de processo.

O teste de aceite fixa os timestamps gerados em uma data histórica, repete `check` e confirma bytes e timestamps inalterados. Uma constante alterada atualiza Rust preservando manifesto igual; adicionar uso do core atualiza o manifesto. Isso evita sleeps e suposições sobre a resolução de tempo do filesystem.

Isso é reuso de saída, **não um cache completo do compilador por chave de conteúdo**. Cada comando continua lendo/tokenizando/parseando/analisando/emitindo e invocando Cargo. Cargo mantém seus próprios artefatos incrementais. Versões do compilador/toolchain, dependências e outras entradas não integram uma chave de cache Truxt.

## Próxima fronteira

Imports explícitos de arquivos já seguem o [contrato de módulos](modules.pt-BR.md). Spans têm SourceId mantendo offsets de bytes e APIs legadas com fonte zero. Manifesto, namespaces e cache por chave de dependências continuam fora desse contrato.
