# Callback and HTTP framework contract

[Português do Brasil](callbacks-and-web-contract.pt-BR.md)

## Status and goal

This document describes the general HTTP API target. The local notes proof of
concept now includes `lift_web`, keeping Axum/Tokio behind a Rust adapter, with
owned `Request`/`Response` DTOs, named synchronous handlers, and static
`GET /health`, `GET /notes`, and `POST /notes` routes. Those DTOs, handlers and
route registrations are now declared in Lift, with generated bridges to a
generic Rust adapter. See [the implemented subset](resources-and-http.md).
Escaping Lift closures and arbitrary FFI callbacks remain unsupported.

## First handler model

The first public handler is synchronous and owned:

```text
Handler = Fn(Request) -> Response
```

`Request` and `Response` are Lift-owned DTOs, not aliases for Axum generics.
The implemented `Request` carries method, path, raw query and a bounded UTF-8
body. `Response` carries status (200..599) and an owned text body; the adapter
sets text/plain UTF-8. Repeated headers remain part of the general target.
The proof of concept returns 400 for invalid UTF-8, 413 for oversized bodies,
`Result` for bind failures, and router 404/405 responses. Invalid response
statuses return 500.

The proof-of-concept increment covers static routes, text responses, 404/405,
and a 16 KiB body limit. Invalid UTF-8 and oversized bodies return 400/413; bind
failures become `Result` errors. JSON DTO conversion, streaming, multipart,
WebSockets, general middleware, arbitrary callbacks, and async handlers are
separate increments.

## Escaping callback rules

The future escaping-closure contract is reusable and immutable (`Fn`), not
`FnMut` or `FnOnce`; today's named handler bridges have no captures:

- Registering a route transfers ownership of the callback to the server.
- Captures are owned snapshots. A callback cannot retain a borrow of the frame
  that registered it.
- A callback may be called repeatedly and concurrently. Its captures must be
  `Send + Sync + 'static` at the Rust adapter boundary.
- Resource handles such as a database transaction, request body stream, or
  mutable connection cannot become cloneable just to satisfy callback storage.
- The adapter may share an immutable handler through `Arc`; that shares one
  callable rather than cloning captured resources.
- HIR records the escaping-callable mode and capture ownership. Codegen follows
  those decisions instead of inferring ownership a second time.

These rules are deliberately narrower than all Rust closures and all
TypeScript/JavaScript callbacks. Lift will document the supported subset and
reject unsupported captures with source diagnostics.

## Axum adapter behavior

Axum expects handlers and returned futures to satisfy `Send` and lifetime
constraints; a normal closure must also meet `Clone + Send + Sync + 'static`.
The Lift adapter will normalize its handler internally. It will read and bound
the request body asynchronously, construct the owned DTO, execute the
synchronous handler on bounded blocking capacity, then convert the DTO response
back to an HTTP response. See the [Axum handler contract](https://docs.rs/axum/latest/axum/handler/trait.Handler.html).

Blocking work needs an explicit concurrency limit. Tokio documents that a
`spawn_blocking` task cannot be aborted after it starts. An HTTP timeout must
not be described as cancelling a running database query or rolling back its
transaction. See [Tokio `spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

## Database boundary

`lift_web` and `lift_db` are separate adapters. In the proof of concept,
`NoteStore` is explicitly shared through `Arc<Mutex<_>>`; a semaphore limits
active synchronous handlers to eight. Transactions stay inside `lift_db` and
are consumed on commit. The HTTP layer does not return SQLite driver details.
Oracle, SQLite, and PostgreSQL drivers need not all be async to use the same
synchronous handler contract.

## Acceptance tests before calling it usable

- A callback is registered in one scope, invoked after that scope returns, and
  called more than once without dangling captures.
- Mutable or borrowed captures and non-shareable resources are rejected by
  Lift semantics before an opaque Rust compiler error.
- Two concurrent requests can invoke an immutable handler without shared-state
  mutation or a data race.
- GET text, POST echo, 404/405, invalid UTF-8, oversized body, invalid response
  status/header, and occupied bind address produce the documented outcomes.
- A slow synchronous route does not block unrelated async request I/O; overload
  and shutdown are bounded and observable.
- HTTP timeout behavior is tested separately from database cancellation and
  transaction rollback.

The runnable service and demo commands are in
[`examples/notes-service`](../examples/notes-service/README.md). The proof of
concept handlers are Rust adapter code, not Lift-defined functions/callbacks.

The official [VS Code LSP guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
and [DAP guide](https://code.visualstudio.com/api/extension-guides/debugger-extension)
remain relevant to the separate editor and debugger tracks.
