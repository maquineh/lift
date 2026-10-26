# Callback and HTTP framework contract

[Português do Brasil](callbacks-and-web-contract.pt-BR.md)

## Status and goal

This is a design contract for the next language/runtime phases. Escaping
callbacks, `truxt_web`, and Axum bindings are not implemented yet. The goal is
an Express-like routing experience while keeping Axum, Tokio, and Rust-specific
types behind a Truxt adapter.

## First handler model

The first public handler is synchronous and owned:

```text
Handler = Fn(Request) -> Response
```

`Request` and `Response` are Truxt-owned DTOs, not aliases for Axum generics.
An initial `Request` carries method, path, raw query, repeated headers, and a
bounded UTF-8 body. `Response` carries a validated status, repeated headers,
and an owned text body. Invalid UTF-8, invalid headers/status, oversized bodies,
bind failures, 404, and 405 have defined errors; the adapter must not panic or
silently discard bytes.

The first routing increment should cover static GET/POST routes, text responses,
404/405, and a body limit. JSON DTO conversion, streaming, multipart, WebSockets,
general middleware, and async Truxt handlers are separate increments.

## Escaping callback rules

The callable contract is reusable and immutable (`Fn`), not `FnMut` or `FnOnce`:

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
TypeScript/JavaScript callbacks. Truxt will document the supported subset and
reject unsupported captures with source diagnostics.

## Axum adapter behavior

Axum expects handlers and returned futures to satisfy `Send` and lifetime
constraints; a normal closure must also meet `Clone + Send + Sync + 'static`.
The Truxt adapter will normalize its handler internally. It will read and bound
the request body asynchronously, construct the owned DTO, execute the
synchronous handler on bounded blocking capacity, then convert the DTO response
back to an HTTP response. See the [Axum handler contract](https://docs.rs/axum/latest/axum/handler/trait.Handler.html).

Blocking work needs an explicit concurrency limit. Tokio documents that a
`spawn_blocking` task cannot be aborted after it starts. An HTTP timeout must
not be described as cancelling a running database query or rolling back its
transaction. See [Tokio `spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

## Database boundary

`truxt_web` and `truxt_db` remain separate adapters. Share a pool/configuration
across requests only through an explicit shared handle; acquire a connection for
the operation/request and keep transactions non-cloneable and scoped. A
database error stays a `DbError`; application code chooses 404/409/500 mapping.
Oracle, SQLite, and PostgreSQL drivers need not all be async to use the same
synchronous handler contract.

## Acceptance tests before calling it usable

- A callback is registered in one scope, invoked after that scope returns, and
  called more than once without dangling captures.
- Mutable or borrowed captures and non-shareable resources are rejected by
  Truxt semantics before an opaque Rust compiler error.
- Two concurrent requests can invoke an immutable handler without shared-state
  mutation or a data race.
- GET text, POST echo, 404/405, invalid UTF-8, oversized body, invalid response
  status/header, and occupied bind address produce the documented outcomes.
- A slow synchronous route does not block unrelated async request I/O; overload
  and shutdown are bounded and observable.
- HTTP timeout behavior is tested separately from database cancellation and
  transaction rollback.

The official [VS Code LSP guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
and [DAP guide](https://code.visualstudio.com/api/extension-guides/debugger-extension)
remain relevant to the separate editor and debugger tracks.
