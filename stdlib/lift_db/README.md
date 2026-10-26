# Lift database API

[Português do Brasil](README.pt-BR.md)

`lift_db::api` provides synchronous, parameterized SQL for SQLite and PostgreSQL.
SQL/schema belongs to Lift application code; the adapter provides pooling,
transactions, owned scalar values and materialized result rows. The older
`lift_db::resource` and `interop` SQLite note helpers remain compatible.

```lift
import function create_pool(driver:string,url:string,min_idle:int,max_connections:int,acquire_timeout_ms:int): Result<DbPool,string> from "lift_db::api::create_pool";
import function execute(pool:DbPool,sql:string,params:DbValue[]): Result<int,string> from "lift_db::api::execute";
import function value_text(value:string): DbValue from "lift_db::api::value_text";
const pool = result.unwrap(create_pool("sqlite", "sqlite:app.sqlite3", 1, 4, 1000));
result.unwrap(execute(pool,"CREATE TABLE IF NOT EXISTS messages (body TEXT NOT NULL)",[]));
result.unwrap(execute(pool,"INSERT INTO messages (body) VALUES ($1)",[value_text("Hello Lift")]));
```

## API signatures

All functions below are explicit safe imports from `lift_db::api`. `Result`
errors contain strings; SQL values are bound by each driver, never concatenated.

| Function | Return / ownership |
| --- | --- |
| `create_pool(driver:string,url:string,min_idle:int,max_connections:int,acquire_timeout_ms:int)` | `Result<DbPool,string>` owned |
| `pool_driver(pool:DbPool)` | `string`; immutable borrow |
| `pool_size/pool_idle(pool:DbPool)` | `int`; immutable borrow, instantaneous pool statistics |
| `execute(pool:DbPool,sql:string,params:DbValue[])` | `Result<int,string>` affected rows; immutable pool borrow |
| `query(pool:DbPool,sql:string,params:DbValue[])` | `Result<DbRows,string>`; rows own their data and release the lease before returning |
| `begin(pool:DbPool)` | `Result<DbTransaction,string>`; borrows pool, owns an exclusive lease |
| `tx_execute/tx_query(transaction:DbTransaction,sql:string,params:DbValue[])` | `Result<int,string>` / `Result<DbRows,string>`; mutable transaction borrow |
| `commit/rollback(transaction:DbTransaction)` | `Result<void,string>`; consumes transaction |
| `close_pool(pool:DbPool)` / `close_rows(rows:DbRows)` | `void`; consumes resource |
| `value_null()` / `value_int(int)` / `value_number(number)` / `value_text(string)` / `value_bool(boolean)` | `DbValue`; cloneable owned scalar |
| `row_count/column_count(rows:DbRows)` | `int`; immutable borrow |
| `column_name(rows:DbRows,column:int)` | `Result<string,string>` owned copy |
| `get_int/get_number/get_text/get_bool(rows:DbRows,row:int,column:int)` | `Result<int/number/string/boolean,string>` owned scalar copy |
| `is_null(rows:DbRows,row:int,column:int)` | `Result<boolean,string>` |

Row and column indices start at zero. Wrong type, null passed to a non-null
getter, and out-of-range indices return errors. SQLite booleans are integer
0/1; `get_bool` accepts those as well as PostgreSQL BOOL. Other getters do not
silently stringify or change types. PostgreSQL INT2/4/8, FLOAT4/8, text types and
BOOL map to these scalars. INT2/4 parameters are range-checked, FLOAT4 uses its
native precision, and non-finite number bindings are rejected. Binary, decimal,
dates, arrays and JSON are not decoded automatically: cast them to a supported
scalar in SQL. NULL is supported independently of the result column type.

`params` is an ordered heterogeneous `DbValue[]`. Use `$1`, `$2`, ... with both
backends; SQLite numeric placeholders are explicitly mapped, including reordered
references. SQLite also accepts `?`/`?N`. Named bindings are unsupported.
PostgreSQL parameter types must be inferable from SQL; use explicit casts such
as `CAST($1 AS BIGINT)` when needed. No dialect translation is performed.

One CRUD/DDL statement is accepted per operation. Transaction/session commands,
procedural blocks, COPY, multiple statements and backslash escapes in quoted SQL
literals are excluded; bind text values instead. Driver preparation checks SQL
grammar. `execute` rejects result columns; use `query` for SELECT/RETURNING.
Rows are fully materialized; large results need an application SQL LIMIT.

## Pools, transactions and HTTP

Drivers are `sqlite` and `postgres`. SQLite URLs are `sqlite:<path>` (including
`sqlite:/absolute/path` and `sqlite:C:\path\file.sqlite3`). SQLite `file:` URIs
are rejected, including URI memory/shared-cache forms. `sqlite::memory:`
requires max_connections=1 and disables both idle_timeout and max_lifetime so
r2d2 cannot retire the sole connection and silently erase its data. Its data
still lasts only for that pool/connection lifetime: disposal, connection failure
or process exit loses the memory database; use a file for durable persistence.
Pool bounds are 0 <= min_idle <= max_connections,
max_connections 1..1024, acquire_timeout_ms 1..3600000. Creation validates one
connection even with min_idle=0; r2d2 maintains the requested minimum idle count.
Acquisition timeout bounds waiting for a lease, not SQL execution or rollback.
SQLite busy_timeout also uses that duration. No statement cancellation is promised.

`DbPool`, `DbTransaction` and `DbRows` are opaque, non-cloneable Lift resources.
Transactions keep their exclusive connection alive even if the pool owner is
dropped. Commit/rollback consume the transaction. Any failed transaction
operation prevents commit; dropping an unfinished transaction attempts rollback.
Failed rollback, failed completion or broken connection causes r2d2 to discard
the lease. Session state outside this transaction protocol is not reset.
Scope exit disposes rows and pools; explicit close functions are also available.

`function handler(request:Request,pool:DbPool):Response` borrows the pool.
`http.serve(address,body_limit,pool)` consumes it and keeps it alive for all
handlers; infrastructure shares it internally without exposing Clone in Lift.
The two-argument serve and single-request handler forms remain supported.
The notes example defines SQLite/PostgreSQL DDL and SQL directly in Lift and
reads `LIFT_DB_DRIVER`, `LIFT_DB_URL`, `LIFT_DB_MIN_IDLE`,
`LIFT_DB_MAX_CONNECTIONS`, `LIFT_DB_ACQUIRE_TIMEOUT_MS` via generic core helpers.

## PostgreSQL TLS and acceptance

PostgreSQL URLs must explicitly select `sslmode=require` or `sslmode=disable`.
Require uses native-tls with certificate-chain and effective-hostname validation
enabled and system trust roots; `LIFT_DB_TLS_ROOT_CERT` can add one PEM root CA.
TLS requires an explicit TCP hostname/IP with a matching certificate; connection
or certificate failure is an error, with no plaintext retry. `disable` explicitly
chooses plaintext for local/CI use. Missing/prefer modes fail configuration.
URLs/credentials are redacted from connection errors and not logged by the pool.
These guarantees come from the configured [native-tls validation defaults](https://docs.rs/native-tls/0.2.18/native_tls/struct.TlsConnectorBuilder.html), rather than the word `require` alone.

Dependencies are pinned: rusqlite 0.40.1 (bundled SQLite), postgres 0.19.14,
postgres-native-tls 0.5.3, native-tls 0.2.18 and r2d2 0.8.10. Linux needs
pkg-config/OpenSSL development libraries; Windows uses the platform TLS provider.

```text
cargo test --manifest-path stdlib/lift_db/Cargo.toml
python scripts/verify_notes.py
# Dedicated disposable PostgreSQL DB; the fixture replaces/drops its notes table:
cargo test --manifest-path stdlib/lift_db/Cargo.toml --test api_contract -- --ignored postgres_general_api_contract
python scripts/verify_notes.py --postgres
```

PostgreSQL checks require `LIFT_TEST_POSTGRES_URL`. CI provisions PostgreSQL 17
and runs actual SQL, transactions, pooling and backend-termination recovery.
Configured CI does not itself certify a platform; actual execution results are
recorded separately. The current automated PostgreSQL server uses explicit
`sslmode=disable`; verified-TLS handshake, custom CA and hostname rejection have
not yet been exercised against a real TLS PostgreSQL server. Older PostgreSQL
targets and Oracle remain unvalidated.
