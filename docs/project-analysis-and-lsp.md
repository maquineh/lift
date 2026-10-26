# Project API, lint, and editor diagnostics

[Português do Brasil](project-analysis-and-lsp.pt-BR.md) · [User guide](../README.md)

`lift_project` is the reusable frontend facade for CLI, HIR lint, and the initial
LSP. Existing lexer/parser/sema/codegen APIs and HIR semantics remain unchanged.
Build/check/run consume this facade before their existing Rust/Cargo steps.

The local [Forklift package increment](forklift.md) adds `lift.toml`, contextual
package analysis, namespaces and exported functions. Existing APIs keep their
signatures; AST enums gain variants for the new syntax.

```rust
use lift_project::{ProjectSession, lint};

let mut session = ProjectSession::new();
session.set_overlay("main.lift", "console.log(7);".to_owned())?;
let analysis = session.analyze("main.lift");
if let Some(hir) = &analysis.hir {
    let warnings = lint::lint(hir);
    // emit_rust(hir), or consume the typed program in another frontend.
}
// sources and diagnostics remain available on failure.
# Ok::<(), std::io::Error>(())
```

The session owns full-text overlays keyed by their stable absolute spelling,
preserving `..` rather than collapsing it across symlinks. The API preserves
path spelling; analysis resolves aliases through the filesystem when possible.
Unresolved paths retain that spelling and are not assumed canonically equivalent.
Filesystem-resolvable aliases are resolved anew during each analysis, so the first save
through a symlink/junction keeps the unsaved buffer attached. When overlay paths
resolve to the same canonical file, the latest update wins. Remove an overlay
using the same absolute spelling that opened it. Overlays replace disk text in entry and transitive imports,
including new unsaved files. `remove_overlay` resumes disk reads. Each `analyze`
reloads the graph without writing files, invoking Cargo, or contacting a registry.
Relative API paths use the process's current directory; prefer absolute paths.
Canonical deduplication, cycles, global declarations, and root-only executable
statements preserve the [legacy module contracts](modules.md). Manifested
projects instead use [package boundaries](forklift.md): local modules remain flat
inside one package, while dependencies expose explicit function exports.

`Analysis` owns its entry path, `SourceMap`, optional HIR, and structured diagnostics.
HIR exists only when the whole graph passes loading, lexing, parsing and semantics.
Failures retain successfully read source text without partial HIR. Independent
import-loading failures may be collected. Each `Diagnostic` has code, severity
(`Error`/`Warning`), message, optional primary span, and optional secondary span.
Root I/O/extension errors have no span; failed module reads label the importer's
path literal. Codes use `project.*`, `package.*`, `lexer.invalid-token`, `parser.syntax`, and
`sema.<SemaErrorKind>` through an explicit stable mapping, independent of Rust
Debug output; messages remain presentation text. Spans remain half-open
UTF-8 byte ranges. IDs belong to one analysis and must be resolved through its
map, including secondary labels. Earlier results remain owned snapshots after
session changes. There is no incremental analysis cache.

Validated HIR is guaranteed for results produced by `ProjectSession::analyze`.
Public HIR/Analysis fields remain constructible; manually reconstructed values
do not acquire validation merely by using those types.

`SourceFile::position_utf16` maps byte offsets to zero-based line/UTF-16 columns,
including astral Unicode and LF, CRLF, or lone CR line endings. Out-of-range offsets clamp to the source;
offsets inside UTF-8 code points clamp backward to a boundary. Rust paths remain
native OS paths; LSP supports UTF-8 `file://` URIs, percent escapes, and Windows
drive/UNC paths.

## Lint

```sh
lift lint examples/modules/main.lift
lift lint --deny-warnings examples/modules/main.lift
```

Lint loads and type-checks the full graph, then reads validated HIR. It does not
emit Rust, create `.lift_cache`, or verify Rust imports with Cargo. Warnings exit
0 by default; `--deny-warnings` exits 1 when warnings exist. Source/semantic errors
always remain errors and exit 1. `lift check` still validates generated Rust,
dependency paths and signatures.

| Rule | Advisory behavior |
| --- | --- |
| `unused-binding` | Local variable or parameter has no HIR reference. Tracks binding IDs, closure capture initializers and bodies; writes count as references. `_` prefixes and implicit method `this` are exempt. No reachability/dead-store analysis. |
| `literal-condition` | `if`/`while` condition is a boolean literal, optionally grouped. Intentional `while true` also warns. |
| `discarded-value` | Expression statement discards a literal or direct local, optionally grouped. Calls and potentially effectful expressions do not trigger it. |

Rules never remove initializers, rewrite source, infer visibility, or modify
ownership/codegen decisions. They also visit local closure bodies.

## Initial LSP and VS Code

`lift lsp` serves JSON-RPC over stdio with Content-Length framing and stderr logs.
It advertises full-text synchronization and UTF-16 positions. It supports
initialize/initialized/shutdown/exit, didOpen/didChange/didSave/didClose, and
workspace/didChangeWatchedFiles. Other requests return method-not-found. It
publishes frontend errors, related locations, and lint warnings, without Cargo.
Complete frames with invalid JSON receive JSON-RPC parse errors (-32700); invalid
request envelopes receive -32600. The session then continues. Unrecoverable
framing/transport failures terminate with stderr output and exit status 1.
Completion, hover, navigation, formatting, fixes and debugging remain future work.

Set `initializationOptions.entry` to an absolute path or file URI for a project.
Without it, each open document selects its ancestral manifested project or a
legacy entry. An unimported source under a manifest receives `package.outside-graph`.
An explicit entry limits diagnostics to that entry's graph; unrelated open files
are outside the selected scope. Configured projects reanalyze the full graph using all open
overlays. Publications include the open document version; stale/nonincreasing
didChange versions are ignored. Documents retain their original URI identity
across a first save under a symlink/junction; canonicalization only resolves
analysis aliases. If multiple open URIs resolve to one file, the last accepted
didOpen/didChange across those documents determines the authoritative buffer,
diagnostic URI, and document version. Other aliases receive empty diagnostics.
Closing the authoritative URI restores the most recently updated remaining
buffer, including differently escaped URIs for the same decoded path. Analysis is synchronous, so background results
cannot race newer snapshots. Previous diagnostics are cleared when fixed or when
a file leaves the graph. Closing removes the overlay and resumes disk reads in
configured projects. Watched-file events reload disk dependencies. Reanalysis is
currently full and un-debounced.

The local [VS Code extension](../editors/vscode/README.md) includes a stdio client,
TextMate grammar with template interpolation, comment/bracket rules, and
check/build/lint/run tasks. Configure `lift.serverPath` and, for modules,
`lift.entry`. The first workspace folder resolves relative entries; routing
multiple projects is not implemented. Tasks consume saved files; LSP consumes
overlays. Setting changes restart the client. The extension is not published.

## Next increments

Forklift local path dependencies, manifests and namespaces are now implemented;
see [its exact scope](forklift.md). Library bindings, external callbacks/async
and `lift_web`, then DAP remain next. There is no database driver, web runtime
or debugger. PostgreSQL 8.4/9.1
compatibility must be validated before choosing a driver; modern driver support
for those servers is not assumed.
