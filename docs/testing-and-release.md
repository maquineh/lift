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
| Option and Result | `option_contract.rs`, `semantic_contract.rs`; coalescing, optional-field, narrowing, I/O, unwrap, direct FFI Option, and borrowed Result parameter E2E cases in `cli_end_to_end.rs` | invalid types, eager fallbacks, wrong-variant unwraps, mutation through narrowed bindings, non-narrowable `let`/projections, nested optional fields, narrowing inside the condition expression, and Option nested in arrays or Option<Result> |
| Errors and propagation | `errors_contract.rs`; `examples/errors.lift` through check/build/run in `cli_end_to_end.rs` | constructors without context or with wrong arity, `?`/`throw` without a compatible target or with mismatched error types, `?` on Option, catch shadowing or `void` type, and missing return after a fallthrough `try` |
| Mutable captures | `enum_contract.rs` mutation marker; `examples/fnmut.lift` and `examples/capture-fields.lift` E2E | whole-variable/field/method mutation, shared `Copy` state, isolated non-`Copy` snapshots, and persistent closure state |
| Generic impl methods | `generic_impls_contract.rs`; `examples/generic-impls.lift` E2E | per-application specialization, `this` mutation, multiple instantiations, unknown targets |
| Generic functions over containers | `generic_container_functions_contract.rs`; `examples/generic-container-functions.lift` E2E | explicit specialization, nested container calls, substitution, calls without explicit arguments rejected |
| Explicit type arguments | `explicit_type_arguments_contract.rs`; `examples/explicit-type-arguments.lift` E2E | arity, conflicts with actual arguments, non-generic callees, value use, container/enum type arguments |
| Generic enums | `generic_enums_contract.rs`; `examples/generic-enums.lift` E2E | arity/duplicate parameters, recursion/Box, named variants, containers, construction/match with expected type, unknown templates |
| Generic containers | `generic_containers_contract.rs`; `examples/generic-containers.lift` E2E | arity/duplicate parameters, nested `Box<Box<T>>`, arrays/optionals, Map-shaped containers, unknown/unauthorized use, and field type substitution |
| Generics | `generics_contract.rs`; `examples/generics.lift` through check/build/run and the native binary in `cli_end_to_end.rs` | duplicate/return-only type parameters, inference conflicts, generic containers/optionals, nested generic calls, function values rejected, arithmetic rejected, and Rust trait bounds |
| Unions | `union_contract.rs`; `examples/unions.lift` through check/build/run and the native binary in `cli_end_to_end.rs` | duplicate/primitive/array members, duplicate union names, exhaustive variant matching, object-literal construction, enum members, and recursive unions |
| Enums | `enum_contract.rs`; `examples/enums.lift`, `examples/enum-fields.lift` and `examples/recursive-enums.lift` through check/build/run and the native binary in `cli_end_to_end.rs` | duplicate/empty variants, wrong constructor arity/type, missing/unknown/duplicate named fields, missing/duplicate/unknown match variants, binding-count mismatch, FFI/DbValue rejection, recursive Box/interface fields, and equality/value semantics |
| Matching Option and Result | `match_contract.rs`; `examples/match.lift` through check/build/run and the native binary in `cli_end_to_end.rs` | missing/duplicate/incompatible patterns, `ok()` vs `ok(nome)` with `void`, non-Option/Result scrutinees, shadowing, bare reassignment, resource results, and missing returns after a fallthrough match |
| Array methods | `array_methods_contract.rs`; `examples/array-methods.lift` through check/build/run and the native binary in `cli_end_to_end.rs` | unknown methods, callback parameter/result mismatches, wrong arity, non-callback arguments, `find` on optional elements, and non-array receivers |
| Loops and loop control | `loops_contract.rs`; `examples/loops.lift` through check/build/run and the native binary in `cli_end_to_end.rs` | non-array `for...of`, `break`/`continue` outside a loop or inside a closure, `const` loop-binding mutation, missing return after a loop, and consuming an external resource inside a loop |
| Function types and callbacks | `higher_order_contract.rs`; `examples/higher-order.lift` through check/build/run and the native binary in `cli_end_to_end.rs`; callback FFI in `ffi_structs_options_contract.rs` | callbacks in fields/arrays/Option/Result, function values over resources, `void` callback parameters, mutable captures, wrong callback signatures, and FFI callback payloads with Option/Result/nested function types |
| Modules and local packages | `crates/lift_project/tests/project_contract.rs`, `packages_contract.rs`; module and Forklift E2E cases in `cli_end_to_end.rs` | cycles, paths, boundaries, private exports, missing files, and multi-file diagnostics |
| Generated Rust and FFI | `crates/lift_codegen/tests/codegen_contract.rs`, `ffi_structs_options_contract.rs` (semantics); signature and execution E2E cases in `cli_end_to_end.rs` | incompatible Rust path/signature, external layout without public fields or `Clone + Debug + PartialEq`, local structs in FFI, Option nested in arrays, nested Result/Option<Result>, conflicting crate versions, and `Result<void, E>` parameters; the pinned registry crate test runs at release acceptance |
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
| Generic FFI containers and Map | Generic impls and functions work through specialization, but FFI generic containers and a native Map API are absent. | FFI generic containers and Map APIs rejected. |
| General pattern matching | Lift has `match` for `Option<T>`, `Result<T, E>`, and nominal enums, with fixed patterns. Primitives, guards, wildcards, nested patterns, and match expressions are absent. | matching accepted algebraic types, exhaustiveness, unreachable/duplicate arms, guards/nesting, and executed behavior. |

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
