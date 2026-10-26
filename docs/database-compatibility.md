# Database compatibility plan

[Português do Brasil](database-compatibility.pt-BR.md)

## Status

This document records a design target, not shipped database support. Truxt does
not yet expose database connections, transactions, rows, or driver bindings.
The current FFI supports a restricted set of language values; it does not yet
model opaque non-cloneable resources or async futures. Database handles cannot
be safely exposed as ordinary copied values.

## Compatibility targets

| Backend | Target | Support class |
| --- | --- | --- |
| PostgreSQL | 15 and 17 | Primary integration targets. Test supported minor releases and document the driver/runtime/TLS combination. |
| PostgreSQL | 8.4.22 and 9.1.24 | Requested legacy compatibility targets. Keep experimental until each server is tested. Do not advertise “8.4+” from endpoint tests alone. |
| SQLite | Bundled SQLite for the first portable profile | Pin the Rust adapter and bundled SQLite version. A system-library profile must state and test its own minimum. |
| Oracle | Later phase | Keep the backend-neutral API open for an adapter; do not promise Oracle support in the first database release. |

PostgreSQL 8.4 and 9.1 are end-of-life. PostgreSQL's upstream policy supports
major releases for five years; on 2026-10-25, the official table lists 14–18 as
supported, with 14 scheduled to reach end-of-life on 2026-11-12. Connecting to
an old server does not restore upstream security fixes. The requested legacy
points are separate compatibility targets, not a claim that every major
release between 8.4 and 15 has been certified. See the
[PostgreSQL versioning policy](https://www.postgresql.org/support/versioning/).

Driver selection remains open. SQLx's FAQ says its official support range
follows database versions that are still actively maintained. Since PostgreSQL
8.4 and 9.1 are end-of-life, SQLx must not be assumed to provide the requested
legacy compatibility. `tokio-postgres` contains catalog fallbacks for older
server layouts, but that is evidence to test, not an upstream compatibility
guarantee. The synchronous `postgres` facade is a possible fit for today's
synchronous Truxt functions; its exact version, TLS feature, and legacy matrix
must be pinned before implementation. See [SQLx's support
policy](https://github.com/transact-rs/sqlx/blob/main/FAQ.md#what-database-versions-does-sqlx-support),
the [`tokio-postgres` fallback](https://github.com/rust-postgres/rust-postgres/blob/master/tokio-postgres/src/prepare.rs),
and the [`postgres` client API](https://docs.rs/postgres/latest/postgres/struct.Client.html).

For SQLite, the `rusqlite` 0.40.1 README documents a 3.34.1 minimum for its base
bindings; selected features can require a newer release. Its `bundled` feature
compiles SQLite into the application, and that release bundles SQLite 3.53.2.
The implementation should pin the crate and SQLite version and test the chosen
features; it should not rely on an unspecified operating-system library. See
the [`rusqlite` 0.40.1 README](https://github.com/rusqlite/rusqlite/blob/v0.40.1/README.md).

Oracle remains a later adapter. Oracle currently publishes a pure-Rust driver
without an Oracle Client dependency, but the version inspected for this plan is
pre-release. Reassess its maturity and server matrix when Oracle work begins:
[Oracle's Rust driver](https://github.com/oracle/rust-oracledb).

## Truxt API boundary

Database support belongs in a separate `truxt_db` adapter layer, not in
`truxt_core`. The compiler and FFI need an explicit resource contract before
connections can cross the language boundary:

- Database connections, transactions, statements, and streams are opaque,
  non-cloneable resources. Do not represent them as integer or string handles.
- `commit` and `rollback` consume a transaction. Dropping a live transaction
  performs best-effort rollback; a failed commit may have an indeterminate
  server-side outcome.
- Rows and values returned across a call are owned snapshots. Preserve SQL
  `NULL`, bytes, 64-bit integers, exact decimals, and timezone-aware values;
  do not stringify every cell or round decimals through `f64`.
- SQL text and bind parameters are separate inputs. Never interpolate values
  into SQL templates. Placeholders and identifier rules belong to the backend.
- Database errors use a backend-neutral `DbError`; HTTP handlers decide how to
  map them. DSNs, passwords, SQL bind values, and raw driver internals are not
  logged or returned to users by default.
- The initial API should be synchronous to match the current language. Async
  drivers must be isolated behind an explicit runtime adapter until Truxt has
  a defined async/future contract.

PostgreSQL 15/17 and SQLite are the first implementation targets. PostgreSQL
8.4.22/9.1.24 stay behind an explicitly experimental compatibility profile.
This profile must not silently weaken TLS or authentication when an old server
cannot meet the selected security policy.

## Acceptance matrix

Before claiming a backend version, run integration tests against that server
version, not only a newer server:

- PostgreSQL 15 and 17 supported minors; legacy 8.4.22 and 9.1.24 in isolated,
  clearly marked compatibility jobs.
- SQLite in-memory and file databases, with the exact bundled release on
  Windows and Linux.
- Parameter binding/injection, `NULL`, UTF-8 and bytes, integer/decimal/time
  boundaries, empty results, duplicate column names, and conversion errors.
- Transaction commit, rollback, early return, connection loss, timeout, and a
  failed statement followed by rollback.
- TLS certificate/hostname validation, credential redaction, and negative
  capability tests for features absent on a backend/version.
- Ownership tests proving that connections and transactions cannot be cloned,
  reused after consumption, or leaked across request lifetimes.

## References

- [PostgreSQL versioning policy](https://www.postgresql.org/support/versioning/)
- [SQLx database-version support policy](https://github.com/transact-rs/sqlx/blob/main/FAQ.md#what-database-versions-does-sqlx-support)
- [`tokio-postgres` compatibility fallback source](https://github.com/rust-postgres/rust-postgres/blob/master/tokio-postgres/src/prepare.rs)
- [`rusqlite` 0.40.1 README](https://github.com/rusqlite/rusqlite/blob/v0.40.1/README.md)
- [Oracle-maintained Rust driver](https://github.com/oracle/rust-oracledb)
