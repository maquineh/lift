# Testing and release acceptance matrix

[Português do Brasil](testing-and-release.pt-BR.md) · [User guide](../README.md)

This matrix connects Lift's public contracts to the tests that protect them. It
describes the Lift subset; passing these checks does not imply full TypeScript
compatibility or support for every Rust crate.

## Covered contracts

| Area | Accepted cases | Rejected cases and boundaries |
| --- | --- | --- |
| Lexer, spans, and parser | `crates/lift_lexer/src/lib.rs`, `crates/lift_parser/tests/parser_contract.rs` | lexical errors, parser recovery, EOF, and UTF-8 in `parser_contract.rs` and `crates/lift_sema/tests/frontend_robustness.rs` |
| Types, functions, objects, and control flow | `crates/lift_sema/tests/semantic_contract.rs`, `functions_contract.rs`; executed examples in `cli_end_to_end.rs` | mismatches, invalid names/imports, returns, and calls rejected before codegen |
| Ownership, methods, and local closures | `liveness_contract.rs`, `functions_contract.rs`; `cross_feature_acceptance_matrix_compiles_and_runs_supported_patterns` | semantic aliasing in `ownership_aliasing_rejection_is_reported_before_rustc`; joins, loops, moves, and projections keep conservative HIR decisions |
| Option and Result | `option_contract.rs`, `semantic_contract.rs`; coalescing, I/O, and unwrap E2E cases in `cli_end_to_end.rs` | invalid types/FFI, eager fallbacks, and wrong-variant unwraps; narrowing and optional-field omission remain pending |
| Modules and local packages | `crates/lift_project/tests/project_contract.rs`, `packages_contract.rs`; module and Forklift E2E cases in `cli_end_to_end.rs` | cycles, paths, boundaries, private exports, missing files, and multi-file diagnostics |
| Generated Rust and FFI | `crates/lift_codegen/tests/codegen_contract.rs`; signature and execution E2E cases in `cli_end_to_end.rs` | incompatible Rust path/signature; the pinned registry crate test runs at release acceptance |
| Database, resources, and HTTP | `crates/lift_sema/tests/resources_web_contract.rs`, `stdlib/lift_db/tests/api_contract.rs`, `scripts/verify_notes.py` | resource consumption/reuse, invalid SQL, HTTP error responses, and separate PostgreSQL integration |
| Project, diagnostics, and editor | `crates/lift_project/tests/project_contract.rs`, internal tests in `crates/lift_cli/src/lsp.rs`, CLI process E2E, and the VS Code Extension Host | unsaved sources, UTF-16, version changes, correction, and diagnostic clearing |

When adding a language construct, add an accepted and a rejected case at the
appropriate layer. Features lowered to Rust need at least one integrated accepted
case that compiles and checks observable behavior. Semantic rejection tests should
verify a source-level Lift diagnostic before any `rustc` failure.

## Pending features that block broader acceptance

These are unimplemented language features, not gaps the current tests can solve.
This matrix protects the documented subset and must not be read as daily-use
readiness for arbitrary TypeScript code.

| Feature | Current status | Acceptance required after implementation |
| --- | --- | --- |
| General generics | `Result<T, E>` is a special built-in type; general generic type parameters are absent. | valid declarations, instantiations, calls, and nested types; invalid arity/combinations rejected before Rust. |
| Unions and enums | User-defined union and enum types are absent. | construction and use of each variant, equality/control-flow behavior per contract, and invalid variants/combinations rejected. |
| Pattern matching | Lift has no matching syntax. The internal Rust `match` used for `Option` does not count as a language feature. | matching accepted algebraic types, exhaustiveness, unreachable/duplicate arms, and executed behavior. |
| Error construction and propagation | Lift has no `Result` constructors, `?`, or `try/catch`; `unwrap` can panic on the wrong variant. | success/error construction, propagation through nested calls, and catching, with defined behavior and diagnostics for each path. |
| First-class closures and callbacks | Only typed local arrow closures with snapshot captures exist; closures cannot be stored, returned, or received as callbacks. | function types, passing/returning/storing, callbacks, and capture ownership verified in generated Rust and executed results. |
| Option narrowing | `is_some` and comparison with `null` do not refine types inside branches. | valid branch-local narrowing that does not escape, including joins, early returns, and loops; unsafe uses rejected before Rust. |
| Optional fields | A `T?` field is still required; omission syntax is absent. | distinguish omitted fields, `null`, and `Some`; validate initialization and generated Rust struct/FFI mappings per contract. |

Each item first needs an explicit semantic contract. Acceptance must then cover
Lift → AST/HIR → compilable Rust → observable behavior for valid cases, plus Lift
diagnostics for invalid cases. Keep each item marked unsupported until that chain
exists.

## Automated gates

- `notes-contract.yml` runs tests and HTTP/SQLite acceptance on Windows and Ubuntu
  with `stable` and Rust 1.94.1. PostgreSQL 17 runs in its own job.
- The `format` job checks the root workspace and the standalone `lift_db`,
  `lift_web`, and `fuzz` workspaces.
- `release-acceptance.yml` runs on manual dispatch and `v*` tags. It runs stable
  acceptance, Rust 1.94.1, formatting, PostgreSQL 17, HTTP/SQLite acceptance, and
  the ignored test that downloads pinned `num-integer` from the registry.
- `vscode-extension.yml` builds the CLI and runs the real extension client in
  stable VS Code on Linux/Xvfb, checking an unsaved diagnostic and its clearing.
- `frontend-fuzz.yml` runs weekly and on manual dispatch on Linux/nightly. Inputs
  must be UTF-8; each campaign is limited to 60 seconds, 2 KiB per input, and
  5 seconds per case. Preserve any found failure in the corpus or as a deterministic
  regression.

Manual commands:

```sh
cargo test --locked --workspace
cargo fmt --all --check
cargo fmt --manifest-path stdlib/lift_db/Cargo.toml --check
cargo fmt --manifest-path stdlib/lift_web/Cargo.toml --check
cargo fmt --manifest-path fuzz/Cargo.toml --check
cargo test --locked -p lift_cli --test cli_end_to_end registry_import_consumes_a_real_pinned_rust_crate -- --ignored --exact
```

The `cargo-fuzz` target requires nightly and Unix sanitizer support. From the
repository root, run `cargo fuzz run frontend -- -max_len=2048
-max_total_time=60 -timeout=5`. Deterministic robustness checks are part of
`cargo test --workspace`; they cover UTF-8-boundary truncations and 512 reproducible
bounded random sources. A short campaign cannot prove panic freedom for unbounded
input size or nesting depth.
