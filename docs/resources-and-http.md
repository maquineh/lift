# Owned SQLite resources and synchronous HTTP

[Português do Brasil](resources-and-http.pt-BR.md)

The SQLite notes-only protocol below is retained for compatibility. Its
successor is the [general database API](../stdlib/lift_db/README.md): DbPool,
DbTransaction, DbRows and DbValue support parameterized SQLite/PostgreSQL SQL.
HTTP handlers may now take a borrowed DbPool, and `http.serve(address,limit,pool)`
consumes the pool. The ownership restrictions described here still apply.

This increment makes the notes application itself Lift code: its interfaces,
handlers, static route registration and database decisions are in
`examples/notes-service/main.lift`. `lift_web::http` is an application-independent
adapter; generated bridges translate the Lift DTOs to its Rust DTOs.

## Closed resource protocol

`ResolvedType::Resource(ResourceKind)` distinguishes `SqliteConnection` and
`SqliteTransaction` from ordinary value structs. Handles have no public fields,
equality or Clone. `HirImport.parameter_modes` records the adapter signature;
each `HirCallArgument` records value, immutable borrow or mutable borrow.
`Move` records owned local transfers. A semantic flow pass rejects using a
consumed resource, consuming a borrowed parameter, and cloning a handle.

| Import from `lift_db::resource` | Contract |
| --- | --- |
| `open(string)` | `Result<SqliteConnection, string>` |
| `begin(SqliteConnection)` | Consumes connection; `Result<SqliteTransaction, string>` |
| `insert(SqliteTransaction, string)` | Mutably borrows transaction; `Result<int, string>` |
| `list(SqliteConnection)` | Immutably borrows connection; `Result<string, string>` |
| `commit/rollback(SqliteTransaction)` | Consumes transaction; returns `Result<SqliteConnection, string>` |
| `close(SqliteConnection)` | Consumes and drops connection; `void` |

These exact safe imports are checked as a closed local adapter protocol. An
arbitrary external FFI function cannot accept a resource. Imports still declare
the signature explicitly; Rust checks the generated safe function pointer.
The transaction owns its connection, avoiding an escaped Rust lifetime.
Commit/rollback consume it; an unfinished transaction rolls back on scope drop.
Failures also dispose the consumed handle. `close` gives explicit disposal,
while scope exit and overwriting no handles provide the implicit RAII boundary.
Resource `result.is_ok` borrows; `result.unwrap` and `unwrap_err` consume the
whole Result. `unwrap` still panics on the wrong variant, so check it first.

The flow rules are deliberately conservative: resources cannot be fields,
array/optional elements, error payloads, closure captures or package exports.
Nested Results with resource payloads are unsupported. A move in either branch
makes the resource unavailable afterward, including branches that return.
A loop cannot consume a resource created outside it. Resource rebinding is
rejected; use a new name. A single call expression cannot both borrow and
consume the same resource (nested arguments count, conservatively).
Borrow forwarding into named Lift functions works when existing parameter
inference can prove it. A helper that consumes its
borrowed parameter is rejected; general consuming-function annotations and
escaping borrows are future work. Only the legacy `lift_db::resource` protocol
is limited to fixed note operations; the successor `lift_db::api` provides the
documented general CRUD/DDL SQL interface.

## Lift DTOs and routes

```lift
interface Request { method: string, path: string, query: string, body: string }
interface Response { status: int, body: string }
function health(request: Request): Response { return {status: 200, body: "ok"}; }
http.route("GET", "/health", health);
const server = http.serve("127.0.0.1:3000", 16384);
```

Interface names can differ; fields must match these shapes exactly. Route
handlers are named Lift functions with one immutable/owned request parameter
and an owned response. Capturing closures, FFI callbacks, mutable request
parameters, headers and arbitrary HTTP DTO shapes are outside this increment.
The generated bridge is capture-free and reusable. Registration must occur in
direct top-level expression statements before the single `http.serve` call.
Duplicate method/path pairs fail semantic analysis. Methods are GET, POST, PUT,
DELETE and PATCH; paths contain ASCII letters/digits and `/ _ - . ~` only.
Axum also answers HEAD for GET routes. Paths and methods are literal strings.

`http.serve` returns `Result<void, string>`. `LIFT_HTTP_ADDR` overrides its
address for deployment/acceptance checks; the example defaults to loopback.
Body limits are 1..1048576 bytes inclusive (16384 in the example). Body decoding
returns 400 for invalid UTF-8 and 413 for a limit/read failure. Unknown routes
return 404; unsupported methods return 405. The adapter sets text/plain UTF-8,
accepts response statuses 200..599, maps invalid status/handler panic to 500,
and limits blocking handlers to eight concurrent jobs (otherwise 503).
The runtime does not expose Axum/Tokio or async in Lift. No timeout is promised.

The current notes handlers borrow the server-owned DbPool and acquire pooled
connections; transactions retain an exclusive lease until completion/drop.
SQLite concurrency also follows the driver's locking behavior. HTTP database
failures return generic 500 text. `LIFT_DB_DRIVER`/`LIFT_DB_URL` choose the
backend; `LIFT_NOTES_DB` remains a SQLite-path fallback. The older `lift_web::notes` startup
import remains for compatibility; `main.lift` uses the generic adapter.

## Repeatable acceptance

```text
cargo test --locked --workspace
python scripts/verify_notes.py
```

The portable standard-library script builds native programs, asserts all three
routes, SQL parameters, persisted data after process restart, UTF-8/empty body
errors, exact 16384/16385-byte boundaries, 404/405/500, and bind failure while
holding the competing listener open. The resource program covers commit,
explicit rollback, drop rollback and close. It uses temporary SQLite files.
The workflow `notes-contract.yml` runs these commands on Windows and Ubuntu;
configured CI is distinct from a recorded platform execution.
