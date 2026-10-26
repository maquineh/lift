# Forklift: local Truxt packages

[Português do Brasil](forklift.pt-BR.md) · [User guide](../README.md) · [Project API/LSP](project-analysis-and-lsp.md)

Forklift resolves local `.truxt` libraries by path. A `truxt.toml` manifest opts a
project into package namespaces and explicit function exports. Programs without
a manifest keep the existing flat `import module` behavior. This milestone has
no Truxt registry, version selection, remote dependencies, reexports, or Forklift
lockfile. Database drivers and HTTP/callback support remain separate future work.

## Manifest and imports

Application `app/truxt.toml`:

```toml
[package]
name = "app"
version = "0.1.0"
kind = "bin"
entry = "main.truxt"

[dependencies]
math = { path = "../math" }
```

Library `math/truxt.toml`:

```toml
[package]
name = "math"
version = "0.1.0"
kind = "lib"
entry = "src/lib.truxt"
```

`name`, `version`, `kind`, and `entry` are required strings; name/version must be
nonempty metadata. Version is not a semver constraint or package identity.
Unknown manifest fields and non-path dependency forms are rejected. Entry and
dependency paths are relative to the declaring manifest; entry requires `.truxt`
and must stay in the package root. A dependency path names a directory containing
`truxt.toml`, and dependencies must have `kind = "lib"`. Transitive dependencies
are loaded; package dependency cycles are rejected. Cycle diagnostics identify
the edge and repeated target, without a full dependency-chain trace yet.

The canonical manifest path identifies a package. Two aliases of the same
manifest deduplicate; separate directories with identical name/version remain
distinct packages. Package IDs belong to one resolved graph, never a global
registry. Dependency keys must be Truxt identifiers; `console`, `option`, and
`result` are reserved as dependency/namespace aliases to preserve builtins.

In `math/src/lib.truxt`:

```truxt
function helper(value: int): int { return value + 1; }
export function add(left: int, right: int): int {
    return helper(left) + right;
}
```

In `app/main.truxt`:

```truxt
import * as math from "math";
console.log(math.add(2, 3));
```

The quoted specifier names a declared dependency key, not a file path or Rust
crate. The local alias can differ: `import * as calc from "math"; calc.add(2, 3);`.
Imports establish a package-wide namespace, shared by its flat source modules;
repeating the same alias/target is allowed, while conflicting targets are errors.
Aliases cannot conflict with package-level function/interface/FFI declarations.
Local variables and parameters may shadow an alias: member calls then resolve
against that local value and its nominal methods. Namespaces are not values;
function extraction, named/default imports and dynamic imports are unsupported.
`export` and `as` remain contextual identifiers outside their supported syntax.

Only `export function` crosses a package boundary. Private functions and nominal
interfaces with identical names in different packages coexist. Exported
signatures accept the existing primitive types and valid arrays, Option and
`Result<T, E>` containers, recursively; `void` follows existing position rules.
Custom interfaces and closures are forbidden anywhere in a public signature.
There are no exported interfaces/types, public constants, method exports or
reexports. Private interfaces/methods and local closures remain available inside
each library. Parameter borrowing, mutation propagation and cloning retain the
existing Truxt rules across package calls.

`import module "relative.truxt";` remains flat within a package, allowing repeated
imports and cycles. In manifested projects it cannot escape that package's root
or enter another package's manifest boundary; use a dependency namespace instead.
Only a bin package's declared entry may execute top-level statements. Libraries
and imported source modules contain declarations only. Without a manifest, legacy
relative modules retain their previous unrestricted relative-path contract.

## Commands and build output

Build both executables with `cargo build -p truxt_cli --locked`. `cargo run -p
truxt_cli -- ...` still defaults to `truxt`; use `--bin forklift` for Forklift.

```sh
forklift init my-app
forklift init --lib my-library
forklift resolve examples/packages/app
forklift check examples/packages/app
forklift build examples/packages/app
forklift run examples/packages/app -- --flag "a b"
```

Project arguments default to `.` and accept a directory, `truxt.toml`, or a source
path used to locate the nearest ancestral manifest. The manifest's declared entry
is authoritative. `init` creates `truxt.toml` and `main.truxt`; it derives a safe
ASCII package name from the directory, with a fallback for non-ASCII-only names.
It prepares both files before publication, refuses replacement, and cleans newly
published files if publication fails. Publication uses local hard links and is
not an indivisible transaction across both files.

`resolve` validates manifests, local paths, dependency kinds/cycles, source
loading and parsing without type checking or invoking Cargo. `check` also performs
visibility/type/ownership checks and `cargo check`. `build` and `run` require a bin
root and use release Cargo builds. `check` accepts libraries. Source and Cargo
failures return nonzero status. Source graphs are resolved once per command and
the captured AST/text is consumed by semantic analysis without rereading it.

The generated project remains `.truxt_cache/<entry-stem>/` beside the declared
entry. `build` copies `<entry-stem>` (or `.exe`) beside that entry and replaces an
existing binary at the same destination, preserving the existing Truxt behavior.
`run` executes the cached binary, forwards arguments and inherits the caller's
cwd/stdin/stdout/stderr. The package script includes both executables, source
examples and their manifests. Cargo/Rust/linker remain external requirements.

## Packages versus Rust crates

Forklift combines local Truxt packages into one validated HIR and one generated
Rust/Cargo program. Semantic analysis resolves namespaces before recording
package-qualified HIR identities; source identifiers are not textually renamed.
Binding IDs are rebased across packages, preserving liveness and capture contracts.
The code generator consumes those identities and existing ownership modes.

Rust FFI remains `import function ... from "crate@version::path"`. These are Rust
crate dependencies, handled by Cargo in the generated manifest; they are not
Forklift `.truxt` dependencies. Cargo may fetch Rust registry crates. Conflicting
Rust crate versions across Truxt packages are rejected before code generation.
Cargo may create its usual lockfile in the generated cache; no Forklift lockfile
or semver resolver is implemented.

## API and editor scope

`ProjectSession::resolve(input)` returns package metadata, contextual `ProjectAst`,
SourceMap and diagnostics. `Resolution::analyze(self)` consumes that snapshot,
always using the declared entry; its original input only locates the manifest.
`truxt_sema::analyze_project(ProjectAst)` is filesystem-free. Legacy
`analyze(ProgramAst)` remains a single flat namespace and rejects unresolved
package syntax. New AST enum variants require exhaustive Rust consumers to handle
them; existing entry-point signatures are preserved.

`ProjectSession::analyze(source)` and `truxt check/build/run/lint source` discover
the nearest manifest. The requested source must belong to its declared graph;
otherwise `package.outside-graph` points to that source, avoiding silent success
for an unimported file. Without a manifest the source is the legacy entry.

LSP uses the same rules and unsaved `.truxt` overlays. With explicit `truxt.entry`,
diagnostics cover that entry graph only; other open files are outside the selected
scope. With empty entry, each open source selects its ancestral project or legacy
entry, including an explicit outside-graph diagnostic when applicable. Saved
`truxt.toml` changes trigger reanalysis through the VS Code watcher. Unsaved TOML
buffers are not synchronized by the initial client. No editor-host UI smoke test
is claimed; automated tests cover API, protocol and generated-program execution.
