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
| `crates/lift_ast/src/lib.rs` | Untyped declarations, statements, expressions, imports (functions and external interface layouts), and type syntax, including special `Result<T, E>` types. No parsing or semantics. |
| `crates/lift_ast/src/span.rs` | SourceId plus half-open UTF-8 byte spans; legacy source-zero constructors and same-file joins. |
| `crates/lift_lexer/src/lib.rs` | Logos tokens, lexical errors, byte offsets, raw string-escape text, and lexer unit tests. |
| `crates/lift_parser/src/lib.rs` | Public `parse` entry point returning AST or accumulated syntax errors. |
| `crates/lift_parser/src/parser.rs` | Recursive-descent declarations/statements, Pratt expressions, explicit function/interface import signatures, Result type arguments, and recovery. |
| `crates/lift_parser/src/error.rs` | Syntax-error categories, expected constructs, and source spans. |
| `crates/lift_sema/src/lib.rs` | Public typed HIR, resolved types, ownership modes, builtin names, external struct declarations, semantic diagnostics, and `analyze` export. |
| `crates/lift_sema/src/analyzer.rs` | Scopes, types, nominal objects, external layouts, function checks, conservative ownership, alias checks, Result operations, and HIR production. |
| `crates/lift_sema/src/interop.rs` | Validate FFI types including external structs, direct Option, and borrowed Result parameters; safe import paths, exact registry versions, and dependency identity. |
| `crates/lift_sema/src/liveness.rs` | Private post-validation CFG/liveness pass and narrowly proven direct-local move decisions. |
| `crates/lift_codegen/src/lib.rs` | Emit Rust and Cargo manifests from validated HIR, including clones, evaluation temporaries, safe FFI wrappers (struct/Option conversions and external layout assertions), and Result inspection. |
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
| `fuzz/Cargo.toml`, `fuzz/fuzz_targets/frontend.rs` | Standalone cargo-fuzz target for UTF-8 lexer/parser/semantic inputs, with a small tracked seed corpus. |
| `crates/lift_sema/tests/frontend_robustness.rs` | Reproducible bounded malformed-source/truncation checks and UTF-8 token-span invariants. |
| `editors/vscode/test/` | VS Code Extension Host integration using the real compiled Lift LSP executable. |
| `.github/workflows/` | Stable/MSRV host matrix, formatting, periodic fuzzing, VS Code client integration, and release acceptance. |
| `examples/notes-service/main.lift` | Lift DTOs, named handlers, static HTTP routes and SQLite resource operations. |
| `examples/notes-service/db-smoke.lift` | Demonstrates the fixed SQLite operations imported through safe FFI. |
| `examples/notes-service/README.md`, `README.pt-BR.md` | Bilingual instructions for building, configuring, and calling the HTTP/SQLite proof of concept. |

## Acceptance tests and examples

| File | Coverage or purpose |
| --- | --- |
| `crates/lift_parser/tests/parser_contract.rs` | Precedence, postfix syntax, recovery, required semicolons, imports, Result type syntax, and function type syntax. |
| `crates/lift_sema/tests/semantic_contract.rs` | Types, recursion, borrow modes, alias rejection, imports, builtins, and Result validation. |
| `crates/lift_sema/tests/frontend_robustness.rs` | Bounded deterministic source mutations/truncations, panic freedom, and token-span validity. |
| `crates/lift_sema/tests/liveness_contract.rs` | Last-use eligibility and conservative counterexamples at joins, returns, loops, reassignment, calls, borrowed parameters, and projections. |
| `crates/lift_sema/tests/option_contract.rs` | Optional/null typing, explicit lazy coalescing modes, required versus omittable optional fields, flow narrowing (branches, guards, else-if, loops, captures, rejected mutations), inference limits, cycles, FFI nesting/local-struct rejection, and conservative ownership. |
| `crates/lift_sema/tests/ffi_structs_options_contract.rs` | External layout resolution and dependencies, field/type validation, struct/Option/Result FFI acceptance and rejection, and borrowed parameter modes. |
| `crates/lift_sema/tests/generic_impls_contract.rs` | Generic impl collection, per-application specialization, method substitution and unknown-target rejection. |
| `crates/lift_sema/tests/generic_container_functions_contract.rs` | Explicit specialization of container-generic functions, nested calls, substitution, and rejection without explicit arguments. |
| `crates/lift_sema/tests/explicit_type_arguments_contract.rs` | `::<T>` parsing, explicit instantiation, container arguments, and rejection rules. |
| `crates/lift_sema/tests/generic_enums_contract.rs` | Generic enum declarations, monomorphization/substitution, recursive Box fields, named variants, containers. |
| `crates/lift_sema/tests/generic_containers_contract.rs` | Generic interface declarations, monomorphization/substitution, nested containers, arity/duplicate validation. |
| `crates/lift_sema/tests/generics_contract.rs` | Generic function declaration, per-call inference, containers/optionals, nested calls, invalid parameters, and function-value rejection. |
| `crates/lift_sema/tests/union_contract.rs` | `type A = B | C` declarations, member validation, enum desugaring, object-literal construction, exhaustive match, enum members, and recursive unions. |
| `crates/lift_sema/tests/enum_contract.rs` | Enum declaration/typing, positional/named payloads, recursive Box indirection, interface fields, qualified exhaustive match, limits (FFI/DbValue), equality, option payloads, and clone semantics. |
| `crates/lift_sema/tests/match_contract.rs` | `match` typing for Option/Result/void, exhaustiveness, duplicate/incompatible patterns, clone semantics, payload mutation, return guarantees, loop control, and throw in arms. |
| `crates/lift_sema/tests/array_methods_contract.rs` | `map`/`filter`/`find`/`some`/`every`/`forEach`/`reduce` typing, receiver clones, chaining, named callbacks/captures, value semantics, and rejected callbacks/receivers. |
| `crates/lift_sema/tests/loops_contract.rs` | `for...of` typing/scope/snapshot, `while` labels, nested `break`/`continue` targets, `try`/`catch` interaction, missing-return analysis, mutation rules, and resource-consumption rejection. |
| `crates/lift_sema/tests/higher_order_contract.rs` | Function type resolution and storage/copies, named functions as values, callback captures, higher-order parameters, immediate calls of returned callbacks, callback FFI, and rejected placements. |
| `crates/lift_sema/tests/errors_contract.rs` | `result.ok`/`result.err` context and arity, `?`/`throw` propagation targets, error-type matching, catch mutability and return guarantees, nested `try`, and resource move versus clone. |
| `crates/lift_codegen/tests/codegen_contract.rs` | Generated Rust and Cargo contracts, safe wrappers, external layout/trait assertions, Option conversions, Result types, clones, and argument staging. |
| `crates/lift_cli/tests/cli_end_to_end.rs` | Compile/run examples, relocation, Cargo paths, arguments/stdin/files, Result handling, Ariadne reports, LSP process sessions, cross-feature acceptance, ownership rejection, and deterministic no-rewrite checks. The registry test is opt-in because it can use the network. |
| `docs/testing-and-release.md` | Feature-to-test matrix, CI gates, bounded fuzz commands, and release acceptance instructions. |
| `examples/fibonacci.lift` | Recursive and iterative integer control flow. |
| `examples/ownership.lift` | Mutation and owned values without explicit Rust borrow notation. |
| `examples/interop.lift` | Safe local imports, borrowed arrays/strings, external struct layouts, direct Option parameters/returns, and `console.log`. |
| `examples/stdlib.lift` | FFI-compatible lengths, membership, trimming, and iteration. |
| `examples/io.lift` | Handle Result values while reading arguments/stdin and writing/reading UTF-8 files. |
| `examples/registry.lift` | Consume `num-integer` 0.1.46 from the registry and print a GCD. |
| `examples/liveness.lift` | Exercise clone/move decisions across control flow and verify observable outputs with rustc. |
| `examples/option.lift` | Counters proving exactly-once/lazy fallbacks, optional arrays and fields (omitted and explicit), flow narrowing across guards and loops, and retained sources. |
| `examples/capture-fields.lift` | Field and method mutation of captured structs through `Rc<RefCell<T>>`. |
| `examples/fnmut.lift` | Mutable closure captures with persistent independent state. |
| `examples/generic-impls.lift` | Generic `impl Box<T>` methods specialized for concrete containers. |
| `examples/generic-container-functions.lift` | Container-generic function specialization and nested calls. |
| `examples/explicit-type-arguments.lift` | Explicit generic calls with primitive and container type arguments. |
| `examples/generic-enums.lift` | `Maybe<T>`, recursive `List<T>`, named generic variants, matching. |
| `examples/generic-containers.lift` | `Box<T>`, nested containers, `Pair<A,B>`, Map-shaped interface and native execution. |
| `examples/generics.lift` | Inferred generic identity/equality/containers and native execution. |
| `examples/unions.lift` | `type A = B | C` unions, object-literal construction, exhaustive matching, enum members, and recursion. |
| `examples/recursive-enums.lift` | Recursive enums with automatic Box, interface enum fields, and recursive matching. |
| `examples/enum-fields.lift` | Named-field variants, object-literal construction, positional binding by field order, and value/equality semantics. |
| `examples/enums.lift` | Nominal enums with payloads, exhaustive variant match, equality, and arrays of enums. |
| `examples/match.lift` | Exhaustive Option/Result match, `ok()` for void, payload snapshots, and loop control inside arms. |
| `examples/array-methods.lift` | Higher-order array methods, chaining, callback captures, and struct/string results. |
| `examples/loops.lift` | `for...of` over arrays, `while`, nested control flow, loop scoping, and struct/array iteration. |
| `examples/higher-order.lift` | Function types, stored/copied callbacks, returned closures, higher-order parameters, captures, and FFI callbacks. |
| `examples/errors.lift` | Result constructors, `?` propagation into `try`/function, `throw`, typed catch, fallthrough and resource-free native execution. |
| `examples/modules/main.lift` | Entry script with repeated imports and exactly-once root execution. |
| `examples/modules/models.lift` | Shared interface/functions and a Rust FFI declaration. |
| `examples/modules/math/api.lift`, `math/helpers.lift` | Relative/transitive imports, cross-file calls, and module/root cycles. |
| `scripts/package.ps1` | Build for the current Rust host and create a portable ZIP with executable, guides, and examples. |

Unit tests remain alongside span, lexer, and standard-library implementations. Run `cargo test --workspace` and the explicit registry test from the root README. The release workflow runs the registry test; the fuzz and VS Code Extension Host workflows have separate commands. A passing set demonstrates the cases it executes, rather than all possible ownership patterns or Rust APIs.

## Design records

Internal records live in `.agents/skills/lift-compiler/`: `SKILL.md` defines contributor workflow; `references/architecture.md` is the original target blueprint; phase 1/2/3 records describe implemented syntax and semantics; `codegen-cli-decisions.md` records lowering/Cargo behavior; `phase5-decisions.md` records the safe import and console contract plus the external-struct/Option FFI increment. The current user-facing Result/I/O contract is documented in `docs/result-and-io.md` and its Portuguese counterpart; struct layouts and Option FFI are in `docs/ffi-structs-and-options.md`, the constructor/`?`/`throw`/`try` error path is in `docs/errors.md`, function types/callbacks in `docs/higher-order.md`, loops in `docs/loops.md`, array methods in `docs/array-methods.md`, Option/Result matching in `docs/match.md`, enums in `docs/enums.md`, and union types in `docs/unions.md`, each with a counterpart. A roadmap item in the blueprint does not mean that it is implemented.

Diagnostics/output reuse are documented in `docs/diagnostics-and-build.md`. Legacy file modules are in `docs/modules.md`; sessions/lint/LSP in `docs/project-analysis-and-lsp.md`; local manifests/namespaces in `docs/forklift.md`. Each has a Portuguese counterpart. The package/API contract tests and `examples/packages/app` cover the local package increment. Dependency-keyed caching remains future work.
