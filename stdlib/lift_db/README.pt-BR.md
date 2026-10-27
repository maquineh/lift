# API de banco do Lift

[English](README.md)

`lift_db::api` oferece SQL síncrono parametrizado para SQLite e PostgreSQL.
SQL/schema ficam no código Lift; o adaptador fornece pool, transações, valores
escalares owned e resultados materializados. Os helpers SQLite antigos de
`lift_db::resource` e `interop` continuam compatíveis.

```lift
import function create_pool(driver:string,url:string,min_idle:int,max_connections:int,acquire_timeout_ms:int): Result<DbPool,string> from "lift_db::api::create_pool";
import function execute(pool:DbPool,sql:string,params:DbValue[]): Result<int,string> from "lift_db::api::execute";
import function value_text(value:string): DbValue from "lift_db::api::value_text";
const pool = result.unwrap(create_pool("sqlite", "sqlite:app.sqlite3", 1, 4, 1000));
result.unwrap(execute(pool,"CREATE TABLE IF NOT EXISTS messages (body TEXT NOT NULL)",[]));
result.unwrap(execute(pool,"INSERT INTO messages (body) VALUES ($1)",[value_text("Olá Lift")]));
```

## Assinaturas

Todas são imports seguros explícitos de `lift_db::api`. Erros Result são strings.
Valores SQL são ligados pelo driver; nunca são concatenados ao comando.

| Função | Retorno / ownership |
| --- | --- |
| `create_pool(driver:string,url:string,min_idle:int,max_connections:int,acquire_timeout_ms:int)` | `Result<DbPool,string>` owned |
| `pool_driver(pool:DbPool)` | `string`; empréstimo imutável |
| `pool_size/pool_idle(pool:DbPool)` | `int`; estatística instantânea, empréstimo imutável |
| `execute(pool:DbPool,sql:string,params:DbValue[])` | `Result<int,string>` com linhas afetadas; empresta pool |
| `query(pool:DbPool,sql:string,params:DbValue[])` | `Result<DbRows,string>`; materializa tudo e libera a conexão antes de retornar |
| `begin(pool:DbPool)` | `Result<DbTransaction,string>`; empresta pool e possui conexão exclusiva |
| `tx_execute/tx_query(transaction:DbTransaction,sql:string,params:DbValue[])` | `Result<int,string>` / `Result<DbRows,string>`; empréstimo mutável da transação |
| `commit/rollback(transaction:DbTransaction)` | `Result<void,string>`; consome transação |
| `close_pool(pool:DbPool)` / `close_rows(rows:DbRows)` | `void`; consome recurso |
| `value_null()` / `value_int(int)` / `value_number(number)` / `value_text(string)` / `value_bool(boolean)` | `DbValue`, escalar owned clonável |
| `row_count/column_count(rows:DbRows)` | `int`; empréstimo imutável |
| `column_name(rows:DbRows,column:int)` | `Result<string,string>` com cópia owned |
| `get_int/get_number/get_text/get_bool(rows:DbRows,row:int,column:int)` | `Result<int/number/string/boolean,string>` com cópia owned |
| `is_null(rows:DbRows,row:int,column:int)` | `Result<boolean,string>` |

Índices começam em zero. Tipo incompatível, NULL em getter não nulo e índice
inválido retornam erro. Booleanos SQLite são inteiros 0/1; `get_bool` aceita esse
formato e BOOL PostgreSQL. Os demais getters não convertem tipos silenciosamente.
PostgreSQL INT2/4/8, FLOAT4/8, textos e BOOL mapeiam para os escalares. INT2/4
verificam faixa; FLOAT4 usa precisão nativa. Bind de número não finito é rejeitado.
Binários, decimal, datas, arrays e JSON não têm decoder automático: use CAST SQL
para escalar suportado. NULL independe do tipo declarado da coluna.

`params` é um `DbValue[]` heterogêneo e ordenado. `$1`, `$2`, ... funcionam nos
dois backends; SQLite mapeia índices numéricos, inclusive referências invertidas.
SQLite também aceita `?`/`?N`. Bindings por nome ficam fora. PostgreSQL precisa
inferir tipos pelo SQL; use `CAST($1 AS BIGINT)` quando necessário. Não há tradução
automática entre dialetos.

Cada operação aceita um statement CRUD/DDL. Comandos de sessão/transação,
blocos procedurais, COPY, múltiplos statements e escapes com barra invertida em
literais SQL ficam fora; passe texto por parâmetros. O driver valida a gramática.
`execute` rejeita colunas de resultado; use `query` para SELECT/RETURNING.
Resultados são materializados; consultas grandes precisam de LIMIT na aplicação.

## Pool, transações e HTTP

Drivers: `sqlite` e `postgres`. URLs SQLite: `sqlite:<caminho>`, inclusive
`sqlite:/caminho/absoluto` e `sqlite:C:\pasta\arquivo.sqlite3`.
URIs SQLite `file:` são rejeitadas, inclusive memória/cache compartilhado via URI.
`sqlite::memory:` exige max_connections=1 e desativa idle_timeout e max_lifetime,
impedindo que r2d2 aposente a única conexão e apague os dados silenciosamente.
Os dados ainda duram somente pelo tempo de vida desse pool/conexão: descarte,
falha da conexão ou fim do processo perde o banco em memória. Use arquivo para
persistência durável. Configuração: 0 <= min_idle <=
max_connections; máximo de conexões 1..1024; timeout de aquisição 1..3600000 ms.
A criação valida uma conexão mesmo com min_idle=0; r2d2 mantém o mínimo ocioso
configurado. O timeout limita espera por conexão, sem limitar execução SQL ou
rollback; o busy_timeout SQLite também usa essa duração. Não há cancelamento prometido.

`DbPool`, `DbTransaction` e `DbRows` são opacos e não clonáveis em Lift.
A transação mantém sua conexão exclusiva viva mesmo se o dono do pool sair do
escopo. Commit/rollback consomem a transação. Qualquer operação falha impede
commit; drop de transação inacabada tenta rollback. Rollback/completion falho
ou conexão quebrada descarta a conexão em vez de devolvê-la ao pool.
Estado de sessão fora desse protocolo não é reinicializado. Sair do escopo
descarta resultados/pool; funções close permitem descarte explícito.

`function handler(request:Request,pool:DbPool):Response` empresta o pool.
`http.serve(endereço,limite,pool)` consome o recurso e o mantém vivo para todos os
handlers; compartilhamento interno não expõe Clone em Lift. A forma antiga com
dois argumentos e handlers sem pool permanece. Notas define DDL SQLite/PostgreSQL
e SQL em Lift, lendo `LIFT_DB_DRIVER`, `LIFT_DB_URL`, `LIFT_DB_MIN_IDLE`,
`LIFT_DB_MAX_CONNECTIONS`, `LIFT_DB_ACQUIRE_TIMEOUT_MS` por helpers genéricos core.

## TLS PostgreSQL e aceite

A URL PostgreSQL precisa declarar `sslmode=require` ou `sslmode=disable`.
Require usa native-tls validando cadeia de certificados e hostname efetivo, com
raízes do sistema; `LIFT_DB_TLS_ROOT_CERT` adiciona uma CA PEM. Exige hostname/IP
TCP explícito e certificado correspondente. Falha de certificado/conexão retorna
erro, sem retry plaintext. `disable` escolhe plaintext explicitamente para uso
local/CI. Modo ausente/prefer é rejeitado. URLs/credenciais são removidas dos
erros de conexão e o pool não as registra. A verificação depende das
[configurações native-tls](https://docs.rs/native-tls/0.2.18/native_tls/struct.TlsConnectorBuilder.html), não apenas da palavra `require`.

Dependências fixadas: rusqlite 0.40.2 com SQLite bundled, postgres 0.19.14,
postgres-native-tls 0.5.3, native-tls 0.2.18 e r2d2 0.8.10. Linux precisa de
pkg-config e bibliotecas de desenvolvimento OpenSSL; Windows usa TLS do sistema.

```text
cargo test --manifest-path stdlib/lift_db/Cargo.toml
python scripts/verify_notes.py
# PostgreSQL dedicado e descartável: a fixture substitui/remove a tabela notes.
cargo test --manifest-path stdlib/lift_db/Cargo.toml --test api_contract -- --ignored postgres_general_api_contract
python scripts/verify_notes.py --postgres
```

Os comandos PostgreSQL exigem `LIFT_TEST_POSTGRES_URL`. CI provisiona PostgreSQL17
e executa SQL, transações, pool e recuperação após término de backend. Configurar
CI não certifica uma plataforma; execuções reais são registradas separadamente.
O servidor PostgreSQL automatizado atual usa `sslmode=disable` explícito;
handshake TLS verificado, CA própria e rejeição de hostname ainda não foram
exercitados com um servidor PostgreSQL TLS real. PostgreSQL antigo e Oracle
permanecem sem validação neste incremento.
