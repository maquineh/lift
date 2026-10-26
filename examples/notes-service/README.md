# Local notes service

The example now uses the [general SQL API](../../stdlib/lift_db/README.md).
`LIFT_DB_DRIVER` is `sqlite` (default) or `postgres`; `LIFT_DB_URL` defaults to
`sqlite:lift-notes.sqlite3`. Pool settings are `LIFT_DB_MIN_IDLE` (1),
`LIFT_DB_MAX_CONNECTIONS` (4), `LIFT_DB_ACQUIRE_TIMEOUT_MS` (1000).
`LIFT_NOTES_DB` remains a SQLite-path fallback when LIFT_DB_URL is absent.
PostgreSQL URLs require explicit verified `sslmode=require`, or `sslmode=disable`
for local/CI plaintext. Lift source contains backend DDL, shared `$1` bindings,
row getters and transaction boundaries. `http.serve(...,pool)` owns the pool;
database handlers receive an immutable borrow. Rows close explicitly on normal
paths and drop on early return; unfinished transactions roll back on scope exit.
Pool/schema configuration failure exits at startup; database request failures
return generic 500. `--postgres` acceptance requires a disposable test database
because its fixture replaces/drops the notes table.

This proof-of-concept slice compiles `main.lift` into a native binary and starts
a local HTTP server through the Rust `lift_web` and `lift_db` adapters.

```powershell
cargo run -p lift_cli -- run examples/notes-service/main.lift
```

The service defaults to `127.0.0.1:3000`. Set `LIFT_HTTP_ADDR` to choose a
different local address and `LIFT_NOTES_DB` to select the SQLite file. By
default, the file is `lift-notes.sqlite3` in the command's working directory.
SQLite is bundled with the adapter.

To exercise the fixed SQLite operations through the existing safe FFI directly,
without starting the HTTP service, run:

```powershell
cargo run -p lift_cli -- run examples/notes-service/db-smoke.lift
```

| Request | Result |
| --- | --- |
| `GET /health` | `200`, text `ok`. |
| `GET /notes` | `200`, one note per line as `id<TAB>text`; newlines, tabs, and backslashes are escaped. |
| `POST /notes` | Stores the UTF-8 body and returns `201` with its ID. Empty bodies return `400`. |
| Unknown route | Axum `404`. |
| Unsupported method | Axum `405`. |
| Body larger than 16 KiB | `413`. Invalid UTF-8 returns `400`. |

Example from another terminal:

```powershell
Invoke-WebRequest http://127.0.0.1:3000/health
Invoke-WebRequest -Method Post -Uri http://127.0.0.1:3000/notes -Body 'My first note' -ContentType 'text/plain; charset=utf-8'
Invoke-WebRequest http://127.0.0.1:3000/notes
```

The interfaces, named synchronous handlers and `http.route` registrations live
in `main.lift`. The compiler generates DTO bridges to generic `lift_web::http`.
`lift_db::api` exposes owned, non-cloneable pools/transactions/results
with explicit moves, immutable/mutable borrows, commit/rollback and close.
The handlers acquire pooled connections and return generic 500 text for
database failures. Blocking handlers have capacity eight; overflow returns 503.

Run `python scripts/verify_notes.py` to build and verify restart persistence,
HTTP errors, exact body limits, occupied port, SQL parameters and resource
commit/rollback/drop. Run `resources.lift` to inspect the resource example.
See [the contract and limitations](../../docs/resources-and-http.md).

The SQL API supports CRUD/DDL and documented scalar types on SQLite/PostgreSQL;
it does not claim all database types, PostgreSQL versions, or Oracle support.
