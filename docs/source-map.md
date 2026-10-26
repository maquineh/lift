# Source map

[Português do Brasil](source-map.pt-BR.md) · [User guide](../README.md) · [Result and I/O](result-and-io.md)

The compiler pipeline is `.lift` → tokens → untyped AST → validated typed HIR → Rust → Cargo → native executable. Each Rust source module has an English overview and a Brazilian Portuguese overview. The tables below describe every source file, its contracts, and where its behavior is checked.

## Workspace and compiler

| File | Responsibility |
| --- | --- |
| `Cargo.toml` | Workspace members, shared version/edition, dependencies between compiler crates. |
| `Cargo.lock` | Locked compiler dependency graph; install and package commands use `--locked`. |
| `.gitignore` | Exclude compiler artifacts, generated projects at every depth, native example binaries, and distribution ZIPs. |
| `crates/*/Cargo.toml` | Each compiler crate's dependencies; `lift_cli` declares the `lift` and `forklift` executables. |
| `crates/lift_ast/src/lib.rs` | Untyped declarations, statements, expressions, imports, and type syntax, including special `Result<T, E>` types. No parsing or semantics. |
| `crates/lift_ast/src/span.rs` | SourceId plus half-open UTF-8 byte spans; legacy source-zero constructors and same-file joins. |
| `crates/lift_lexer/src/lib.rs` | Logos tokens, lexical errors, byte offsets, raw string-escape text, and lexer unit tests. |
| `crates/lift_parser/src/lib.rs` | Public `parse` entry point returning AST or accumulated syntax errors. |
| `crates/lift_parser/src/parser.rs` | Recursive-descent declarations/statements, Pratt expressions, explicit import signatures, Result type arguments, and recovery. |
| `crates/lift_parser/src/error.rs` | Syntax-error categories, expected constructs, and source spans. |
| `crates/lift_sema/src/lib.rs` | Public typed HIR, resolved types, ownership modes, Result builtin names, semantic diagnostics, and `analyze` export. |
| `crates/lift_sema/src/analyzer.rs` | Scopes, types, nominal objects, function checks, conservative ownership, alias checks, Result operations, and HIR production. |
| `crates/lift_sema/src/interop.rs` | Validate FFI types, safe import paths, exact registry versions, and dependency identity. |
| `crates/lift_sema/src/liveness.rs` | Private post-validation CFG/liveness pass and narrowly proven direct-local move decisions. |
| `crates/lift_codegen/src/lib.rs` | Emit Rust and Cargo manifests from validated HIR, including clones, evaluation temporaries, safe FFI wrappers, and Result inspection. |
| `crates/lift_cli/src/lib.rs` | Commands, source diagnostics, embedded-core extraction, generated projects, Cargo invocation, artifact lookup, and program execution. |
| `crates/lift_cli/src/diagnostics.rs` | Ariadne 0.6 reports using byte-indexed spans, optional related labels, terminal color policy, and renderer unit tests. |
| `crates/lift_project/src/sources.rs` | Own source paths/text and SourceIds; UTF-16 positions with LF/CRLF/CR. |
| `crates/lift_project/src/modules.rs` | Legacy relative source loading and dispatch to manifested project analysis. |
| `crates/lift_project/src/lib.rs` | Reusable sessions, stable overlays, SourceMap/HIR/diagnostic results. |
| `crates/lift_project/src/forklift.rs` | Local manifests, canonical package identities, dependencies, boundaries and captured resolution. |
| `crates/lift_project/src/lint.rs`, `diagnostics.rs` | Advisory HIR rules and explicit stable semantic diagnostic codes. |
| `crates/lift_sema/src/packages.rs` | Filesystem-free contextual analysis, exports, package-qualified HIR and global binding identities. |
| `crates/lift_cli/src/lsp.rs`, `forklift.rs` | Diagnostic JSON-RPC server and local Forklift CLI entry. |
| `crates/lift_cli/src/main.rs` | Forward the CLI result as the process exit code. |

AST and HIR nodes retain source spans. `analyze` never returns partial HIR after an error. Code generation consumes semantic decisions rather than inferring ownership again. The core's embedded source list is explicit in the CLI; add any new core module there as well.

The workspace and generated manifests require Rust/Cargo 1.94.1+. Nominal impl
methods, local snapshot closures, and contextual template interpolation span AST,
lexer/parser, sema/HIR, and codegen; their contract is in
[methods, closures, and templates](methods-closures-templates.md).
`crates/lift_sema/tests/functions_contract.rs` checks lexical captures, receiver
mutation, alias rejection, and nested UTF-8 spans. The CLI integration compiles and
executes `examples/methods-closures-templates.lift` with expected stdout.

## Standard library

| File | Responsibility |
| --- | --- |
| `stdlib/lift_core/Cargo.toml` | Dependency-free native Rust library in the source workspace. The embedded copy receives a standalone manifest without workspace inheritance. |
| `stdlib/lift_core/src/lib.rs` | Public modules and the `Console` re-export. |
| `stdlib/lift_core/src/console.rs` | Rust console output with `Display` and `io::Result`; `console.log` is a compiler binding. |
| `stdlib/lift_core/src/array.rs` | Borrowed slice operations, including Rust-only callbacks and borrowed results. |
| `stdlib/lift_core/src/string.rs` | Borrowed UTF-8 helpers; length counts Unicode scalar values. |
| `stdlib/lift_core/src/interop.rs` | FFI-compatible lengths, owned trim results, and array membership adapters; native Rust APIs remain available. |
| `stdlib/lift_core/src/io.rs` | UTF-8 arguments/stdin/files, returning `Result<T, String>` with contextual errors. |
| `stdlib/lift_core/README.md`, `README.pt-BR.md` | Public Rust and Lift library guides in both languages. |
| `stdlib/lift_db/src/lib.rs` | Non-cloneable SQLite resource, transactions, and note snapshots; scoped to the proof of concept. |
| `stdlib/lift_web/src/notes.rs` | Owned HTTP DTOs, named synchronous handlers, local routes, and body/concurrency limits. |
| `stdlib/lift_web/src/http.rs` | Generic runtime for Lift DTO/handler bridges and static route registration. |
| `stdlib/lift_db/src/resource.rs` | Owned connection/transaction protocol with commit, rollback and RAII disposal. |
| `crates/lift_sema/src/resources.rs` | Conservative affine flow validation and HTTP registration checks. |
| `scripts/verify_notes.py` | Portable process-level HTTP/SQLite acceptance, including restart and occupied port. |
| `examples/notes-service/main.lift` | Lift DTOs, named handlers, static HTTP routes and SQLite resource operations. |
| `examples/notes-service/db-smoke.lift` | Demonstrates the fixed SQLite operations imported through safe FFI. |
| `examples/notes-service/README.md`, `README.pt-BR.md` | Bilingual instructions for building, configuring, and calling the HTTP/SQLite proof of concept. |

## Acceptance tests and examples

| File | Coverage or purpose |
| --- | --- |
| `crates/lift_parser/tests/parser_contract.rs` | Precedence, postfix syntax, recovery, required semicolons, imports, and Result type syntax. |
| `crates/lift_sema/tests/semantic_contract.rs` | Types, recursion, borrow modes, alias rejection, imports, builtins, and Result validation. |
| `crates/lift_sema/tests/liveness_contract.rs` | Last-use eligibility and conservative counterexamples at joins, returns, loops, reassignment, calls, borrowed parameters, and projections. |
| `crates/lift_sema/tests/option_contract.rs` | Optional/null typing, explicit lazy coalescing modes, required fields, inference limits, cycles, FFI rejection, and conservative ownership. |
| `crates/lift_codegen/tests/codegen_contract.rs` | Generated Rust and Cargo contracts, safe wrappers, Result types, clones, and argument staging. |
| `crates/lift_cli/tests/cli_end_to_end.rs` | Compile/run examples, relocation, Cargo paths, arguments/stdin/files, Result handling, Ariadne reports, and deterministic no-rewrite checks. The registry test is opt-in because it can use the network. |
| `examples/fibonacci.lift` | Recursive and iterative integer control flow. |
| `examples/ownership.lift` | Mutation and owned values without explicit Rust borrow notation. |
| `examples/interop.lift` | Safe local imports, borrowed arrays/strings, and `console.log`. |
| `examples/stdlib.lift` | FFI-compatible lengths, membership, trimming, and iteration. |
| `examples/io.lift` | Handle Result values while reading arguments/stdin and writing/reading UTF-8 files. |
| `examples/registry.lift` | Consume `num-integer` 0.1.46 from the registry and print a GCD. |
| `examples/liveness.lift` | Exercise clone/move decisions across control flow and verify observable outputs with rustc. |
| `examples/option.lift` | Counters proving exactly-once/lazy fallbacks, optional arrays/fields, and retained sources. |
| `examples/modules/main.lift` | Entry script with repeated imports and exactly-once root execution. |
| `examples/modules/models.lift` | Shared interface/functions and a Rust FFI declaration. |
| `examples/modules/math/api.lift`, `math/helpers.lift` | Relative/transitive imports, cross-file calls, and module/root cycles. |
| `scripts/package.ps1` | Build for the current Rust host and create a portable ZIP with executable, guides, and examples. |

Unit tests remain alongside span, lexer, and standard-library implementations. Run `cargo test --workspace` and the explicit registry test from the root README. A passing set demonstrates the cases it executes, rather than all possible ownership patterns or Rust APIs.

## Design records

Internal records live in `.agents/skills/lift-compiler/`: `SKILL.md` defines contributor workflow; `references/architecture.md` is the original target blueprint; phase 1/2/3 records describe implemented syntax and semantics; `codegen-cli-decisions.md` records lowering/Cargo behavior; `phase5-decisions.md` records the limited safe import and console contract. The current user-facing Result/I/O contract is documented in `docs/result-and-io.md` and its Portuguese counterpart. A roadmap item in the blueprint does not mean that it is implemented.

Diagnostics/output reuse are documented in `docs/diagnostics-and-build.md`. Legacy file modules are in `docs/modules.md`; sessions/lint/LSP in `docs/project-analysis-and-lsp.md`; local manifests/namespaces in `docs/forklift.md`. Each has a Portuguese counterpart. The package/API contract tests and `examples/packages/app` cover the local package increment. Dependency-keyed caching remains future work.
