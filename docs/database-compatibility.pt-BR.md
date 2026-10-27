# Plano de compatibilidade com bancos de dados

[English](database-compatibility.md)

## Estado

`lift_db::api` agora oferece SQL CRUD/DDL parametrizado SQLite/PostgreSQL,
pools configuráveis, transações afins e resultados escalares materializados.
O [guia da API](../stdlib/lift_db/README.pt-BR.md) define assinaturas, política TLS,
limites de tipos e comandos de aceite. Helpers SQLite antigos continuam
compatíveis. A matriz de backends/versões abaixo permanece um alvo: implementação
e CI configurado não certificam combinações ainda não executadas.

## Alvos de compatibilidade

| Backend | Alvo | Classe de suporte |
| --- | --- | --- |
| PostgreSQL | 15 e 17 | Alvos principais de integração. Testar minors suportadas e documentar a combinação de driver/runtime/TLS. |
| PostgreSQL | 8.4.22 e 9.1.24 | Alvos legados solicitados. Permanecem experimentais até cada servidor ser testado. Não anunciar “8.4+” apenas com testes nos extremos. |
| SQLite | SQLite embutido no primeiro perfil portátil | Fixar o adaptador Rust e a versão do SQLite embutido. Um perfil com biblioteca do sistema deve declarar e testar seu mínimo próprio. |
| Oracle | Fase futura | Manter a API neutra aberta para um adaptador; não prometer Oracle na primeira versão de banco. |

PostgreSQL 8.4 e 9.1 estão fora de suporte. A política upstream do PostgreSQL
mantém cada major por cinco anos; em 2026-10-25, a tabela oficial lista 14–18
como suportadas, com a versão 14 prevista para sair de suporte em 2026-11-12.
Conectar a um servidor antigo não restaura as correções de segurança upstream.
Esses pontos legados são alvos separados, não uma alegação de que todas as
majors entre 8.4 e 15 foram certificadas. Consulte a
[política de versionamento do PostgreSQL](https://www.postgresql.org/support/versioning/).

A escolha do driver continua aberta. A FAQ do SQLx diz que sua faixa oficial de
suporte acompanha as versões de banco ainda mantidas ativamente. Como PostgreSQL
8.4 e 9.1 estão fora de suporte, não se deve presumir que SQLx ofereça a
compatibilidade legada solicitada. `tokio-postgres` tem fallbacks de catálogo
para layouts antigos, mas isso é evidência para testar, não garantia upstream.
A fachada síncrona `postgres` pode se encaixar nas funções síncronas atuais de
Lift; a versão exata, feature TLS e matriz legada precisam ser fixadas antes da
implementação. Consulte a
[política de suporte do SQLx](https://github.com/transact-rs/sqlx/blob/main/FAQ.md#what-database-versions-does-sqlx-support), o
[fallback de `tokio-postgres`](https://github.com/rust-postgres/rust-postgres/blob/master/tokio-postgres/src/prepare.rs)
e a [API do cliente `postgres`](https://docs.rs/postgres/latest/postgres/struct.Client.html).

Para SQLite, o README do `rusqlite` 0.40.2 documenta o mínimo 3.34.1 para seus
bindings básicos; algumas features podem exigir uma versão mais nova. A feature
`bundled` compila SQLite junto da aplicação e essa release inclui SQLite 3.53.2.
A implementação deve fixar a crate, a versão de SQLite e testar as features
escolhidas; não deve depender de uma biblioteca indefinida do sistema. Consulte
o [README do `rusqlite` 0.40.2](https://github.com/rusqlite/rusqlite/blob/v0.40.2/README.md).

Oracle fica para um adaptador posterior. A Oracle publica um driver Rust puro
que não depende do Oracle Client, mas a versão consultada para este plano ainda
é pré-release. Reavalie maturidade e matriz de servidores quando a etapa Oracle
começar: [driver Rust da Oracle](https://github.com/oracle/rust-oracledb).

## Fronteira da API Lift

O suporte a banco pertence a uma camada adaptadora separada, `lift_db`, e não
ao `lift_core`. Na PoC, recursos permanecem dentro do adaptador Rust; para
cruzarem a fronteira da linguagem, o compilador e o FFI ainda precisam de um
contrato explícito:

- Conexões, transações, statements e streams de banco são recursos opacos e não
  clonáveis. Não os represente como handles inteiros ou strings.
- `commit` e `rollback` consomem a transação. Descartar uma transação ativa tenta
  fazer rollback; um commit que falha pode ter resultado indeterminado no servidor.
- Linhas e valores que cruzam uma chamada são snapshots owned. Preserve SQL
  `NULL`, bytes, inteiros de 64 bits, decimais exatos e valores com timezone;
  não converta cada célula em string nem arredonde decimais via `f64`.
- Texto SQL e parâmetros bind são entradas separadas. Nunca interpole valores em
  templates SQL. Placeholders e regras de identificadores pertencem ao backend.
- Erros de banco usam `DbError` neutro; handlers HTTP decidem o mapeamento. DSNs,
  senhas, valores de bind e detalhes internos do driver não são registrados nem
  retornados ao usuário por padrão.
- A API inicial deve ser síncrona, de acordo com a linguagem atual. Drivers async
  ficam isolados atrás de um adaptador explícito de runtime até Lift definir
  contratos de async/futures.

PostgreSQL 15/17 e SQLite são os primeiros alvos de implementação. PostgreSQL
8.4.22/9.1.24 permanece num perfil de compatibilidade experimental explícito.
Esse perfil não pode enfraquecer TLS ou autenticação silenciosamente quando um
servidor antigo não atende à política de segurança selecionada.

## Matriz de aceite

Antes de prometer uma versão de backend, rode testes de integração naquela
versão de servidor, não apenas em um servidor mais novo:

- Minors suportadas do PostgreSQL 15 e 17; 8.4.22 e 9.1.24 em jobs isolados e
  claramente marcados como compatibilidade legada.
- SQLite em memória e em arquivo, com a versão embutida exata em Windows e Linux.
- Bind/injection, `NULL`, UTF-8 e bytes, limites de inteiros/decimais/tempo,
  resultados vazios, nomes de coluna duplicados e erros de conversão.
- Commit, rollback, retorno antecipado, perda de conexão, timeout e statement que
  falha seguido de rollback.
- Validação de certificado/hostname TLS, redação de credenciais e testes negativos
  de capabilities ausentes em backend/versão.
- Testes de ownership provando que conexões/transações não podem ser clonadas,
  reutilizadas após consumo ou vazadas entre ciclos de requisição.

## Referências

- [Política de versionamento do PostgreSQL](https://www.postgresql.org/support/versioning/)
- [Política do SQLx para versões de banco](https://github.com/transact-rs/sqlx/blob/main/FAQ.md#what-database-versions-does-sqlx-support)
- [Código fallback de compatibilidade `tokio-postgres`](https://github.com/rust-postgres/rust-postgres/blob/master/tokio-postgres/src/prepare.rs)
- [README do `rusqlite` 0.40.2](https://github.com/rusqlite/rusqlite/blob/v0.40.2/README.md)
- [Driver Rust mantido pela Oracle](https://github.com/oracle/rust-oracledb)
