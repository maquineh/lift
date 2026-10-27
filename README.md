# Lift

Rust/Cargo **1.94.1+** (MSRV: `1.94.1`).

Lift is a small, statically typed language that compiles a `.lift` entry file and its explicit file modules to Rust, then uses Cargo to check, build, or run it.

> **Current scope:** typed programs with relative modules, local Forklift packages with explicit function exports, a bundled standard library, safe Rust imports, console output, and Result-based UTF-8 input/files. The installed executables can be moved independently of this checkout. Rust, Cargo, and a host linker are still required to compile programs.

Lift aims to feel familiar to TypeScript users within a documented subset; it is not a full TypeScript implementation. New language features must define their Lift semantics and lower to valid Rust while accounting for Rust's ownership, borrowing, and safety rules.

## Requirements

- Rust **1.94.1+**, with `rustc` and `cargo` available on `PATH`.
- A working linker for the Rust host target.
- Network access when a program imports a registry crate that is not already cached by Cargo.

CI is configured for Windows and Ubuntu with Rust `stable` and MSRV 1.94.1. PostgreSQL 17 has a separate integration job. macOS and cross-compilation are not yet in the supported matrix; build and run for the host target.

## Build the CLI from this checkout

From the repository root:

```sh
cargo build --release -p lift_cli
```

The executable is written to `target/release/lift` (or `lift.exe` on Windows). Its standard-library sources are embedded at build time, so the executable does not need the original checkout afterward.

The same build also produces `forklift`, the local package CLI. Start a manifested
project with `forklift init my-app`, then use its resolve/check/build/run commands.
See [Forklift local packages](docs/forklift.md) for `lift.toml`,
`import * as math from "math"`, `export function`, and the distinction from Rust
crate dependencies. Forklift currently resolves only local paths.

Run `lift lint <entry.lift>` for frontend warnings (`--deny-warnings` for CI).
`lift lsp` and the local [VS Code extension](editors/vscode/README.md) diagnose
unsaved files. See [Project API and tooling](docs/project-analysis-and-lsp.md) for API contracts
and the initial editor features and limits.

## Install or create a portable package

Install from the checkout into Cargo's executable directory:

```sh
cargo install --path crates/lift_cli --locked
lift --version
```

You can also copy the release executable into a directory on `PATH`. Only the executable is needed from Lift; Rust/Cargo and the host linker remain prerequisites. On PowerShell, create a ZIP containing the executable, bilingual guides, and examples with:

```powershell
./scripts/package.ps1
```

Extract `dist/lift-<version>-<host>.zip` anywhere and add its directory to `PATH`, or call its executable by path. The package script builds for the current Rust host. It does not install Rust or modify `PATH`.

## Quick start

Create a starter source file and run it:

```sh
cargo run -p lift_cli -- init hello
```

Edit `hello/main.lift`:

```lift
console.log("Hello, Lift!");
```

Then run the program:

```sh
cargo run -p lift_cli -- run hello/main.lift
```

With the built executable on `PATH`, the same commands are `lift init hello` and `lift run hello/main.lift`.

## CLI commands

| Command | Behavior |
| --- | --- |
| `lift init [directory]` | Create `main.lift`; refuse to overwrite an existing file. |
| `lift check file.lift` | Parse and analyze Lift, generate Rust, then run `cargo check`. |
| `lift build file.lift` | Compile in release mode and copy the executable next to the source file. |
| `lift run file.lift [-- args...]` | Compile in release mode and run the generated executable, forwarding arguments. |

Generated projects and Cargo artifacts are stored under `.lift_cache/<entry-name>/` beside the entry file. The CLI extracts its core into that project's `lift_core/` directory and uses a relative Cargo dependency. Generated `main.rs`, `Cargo.toml`, and core files are written only when their bytes change. Changes to the extracted core are overwritten on the next command. This avoids unnecessary rewrites; module loading, analysis, code generation, and Cargo still run every time, so it is not a complete content-keyed cache.

## File modules

Import another Lift source at the top level:

```lift
import module "lib/math.lift";
console.log(double(21));
```

`lib/math.lift` can declare the function:

```lift
function double(value: int): int { return value * 2; }
```

Paths are relative to the importing file and must include `.lift`. Imports are recursive; canonical paths are included once, so repeated imports and cycles are allowed. Only the entry file may contain executable top-level statements. Imported modules may declare functions, interfaces, Rust FFI imports, and further module imports. File modules remain flat, sharing a namespace within their package; duplicate names are errors with labels at both files. Without a manifest this is the legacy global namespace. Manifested projects additionally support package aliases and exported functions through [Forklift](docs/forklift.md). Rust `import function ... from ...` syntax is unchanged.

Run `lift run examples/modules/main.lift` for relative/transitive imports, FFI, shared types, and cycles including the entry file. See [the module contract](docs/modules.md) for source identity and exact limits.

## Source diagnostics

Lexical, syntax, and semantic errors use Ariadne reports showing the source path, message, excerpt, primary label, and a related label when the compiler supplies a secondary span. UTF-8 byte spans are rendered at the correct character position, including empty EOF spans. Source errors retain exit code `1`.

Colors are enabled only on an interactive stderr terminal and can be disabled with `NO_COLOR`; redirected output has no ANSI colors. Cargo/rustc diagnostics remain Cargo's own reports about the generated Rust, without remapping to Lift source. See [diagnostics and generated files](docs/diagnostics-and-build.md) for the implementation contract.

## Language and interop scope

The current implementation supports typed functions, primitive values (`int`, `number`, `boolean`, `string`), arrays, interfaces/object literals, control flow, and the ownership patterns covered by the examples and tests. The language does not insert semicolons at newlines. Imports are top-level declarations and require a trailing semicolon.

Rust imports declare an explicit Lift signature and a Rust path:

```lift
import function concat(left: string, right: string): string from "lift_core::string::concat";
console.log(concat("Hello, ", "Lift!"));
```

The FFI accepts safe Rust free functions with scalar and one-level array types. `string` parameters are passed as `&str` and string returns as owned `String`; array parameters are borrowed slices and array returns are owned `Vec<T>`. `void` maps to `()`. An import may also return `Result<T, E>` with supported payload types; Result parameters and nested Result returns remain unsupported by the FFI. Imports from registry crates use an exact `MAJOR.MINOR.PATCH` version. The generated Rust wrapper lets `rustc` verify the target path and signature. Struct FFI, `unsafe`, and nested arrays are unsupported.

`console.log` accepts one `int`, `number`, `boolean`, or `string` expression and writes it with a newline. It currently panics if writing to stdout fails.

The Rust-native `lift_core` crate includes console, array, and string helpers. FFI-compatible adapters live under `lift_core::interop`:

```lift
import function length(values: int[]): int from "lift_core::interop::array_len";
import function trim(value: string): string from "lift_core::interop::string_trim";
import function contains(values: int[], value: int): boolean from "lift_core::interop::array_contains_int";
```

The adapters also provide `string_len` and `array_contains_number`, `array_contains_boolean`, and `array_contains_string`. Callback helpers (`map`, `filter`, and `find`) remain Rust-only because the current closure subset has no higher-order/FFI function types. See [the core library guide](stdlib/lift_core/README.md) for the full API.

## Optional values

Use `T?`, contextual `null`, and explicit `option.some(value)`:

```lift
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9);
```

`??` evaluates its left once and its fallback only for absence. `T? ?? T` returns `T`; `T? ?? T?` remains optional. `option.is_some` tests and `option.unwrap` clones a present payload; unwrap on absence panics. `int?[]` is an array of optionals, while `int[]?` is an optional array. Fields typed `T?` remain required. There is no implicit lift, `undefined` absence, narrowing, omitted optional field, or FFI Option. See [the exact Option contract](docs/option.md), including inference and operator-parentheses limits.

## Input and files

`Result<T, E>` is a special type available in annotations and Lift function signatures. Use explicit imports for UTF-8 arguments, standard input, and files:

```lift
import function arguments(): Result<string[], string> from "lift_core::io::arguments";
import function read_stdin(): Result<string, string> from "lift_core::io::read_stdin";
import function read_file(path: string): Result<string, string> from "lift_core::io::read_file";
import function write_file(path: string, contents: string): Result<void, string> from "lift_core::io::write_file";
```

`arguments()` returns the arguments without the executable name and preserves order and spaces. Use `--` after the source path to forward flags: `lift run app.lift -- --flag "a b"`. `read_stdin()` reads until EOF, including line endings. `read_file()` requires UTF-8; `write_file()` creates or truncates a file and does not create parent directories. Relative file paths resolve from the caller's working directory, which `lift run` preserves.

I/O failures and invalid UTF-8 become error strings rather than process panics. Handle the result with a branch:

```lift
const contents = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

Result inspection borrows its argument, and unwrap operations clone the payload so the original Result remains available. Unwrap on the wrong variant panics. General generics, Result constructors, pattern matching, `?`, and `try/catch` are not implemented. See [the Result/I/O contract](docs/result-and-io.md) for the exact scope. For an example that reads piped input and writes it to a file:

```powershell
"Hello from stdin" | lift run examples/io.lift -- output.txt sample
```

The resulting file contains `Hello from stdin|sample`. The same program reads standard input until EOF when run interactively.

## Current limitations

- One executable entry, with legacy flat file modules or local `lift.toml` packages. Forklift supports path dependencies and exported primitive/container function signatures; no Lift registry, remote dependencies, public custom types or reexports yet.
- No complete content-keyed cache. Identical generated files are preserved, and Cargo reuses its own incremental artifacts; analysis/code generation still run each time.
- Ownership analysis remains conservative and rejects some complex aliasing patterns. A private CFG/liveness pass removes only proven last-use clones of direct owned locals; borrowed values, projections, repeated statement uses, and `ByClone` call arguments retain their copies. See [the ownership optimization contract](docs/liveness.md).
- Nominal methods/`impl`, local snapshot arrow closures, and primitive template interpolation are supported; see [their contracts and limits](docs/methods-closures-templates.md). Unions and `try/catch` remain unimplemented. Option remains outside the FFI, and optional fields cannot be omitted.
- Result support uses `is_ok` and cloned unwrap operations; constructors, matching, and automatic error propagation are not implemented.
- The [local notes service](examples/notes-service/README.md) defines DTOs, synchronous handlers, static routes and SQL in Lift. The [database API](stdlib/lift_db/README.md) provides configurable SQLite/PostgreSQL pools, parameterized execute/query, materialized rows and owned transactions. The server owns the pool and handlers borrow it.

The service exercises the documented SQL/scalar and static HTTP contracts. See the [database compatibility plan](docs/database-compatibility.md) for the backend/version matrix and the [callback/HTTP contract](docs/callbacks-and-web-contract.md) for adapter limits.

## Examples and tests

- `examples/fibonacci.lift` — recursive functions and control flow.
- `examples/ownership.lift` — aggregate mutation, borrowing, and owned return values.
- `examples/interop.lift` — local Rust imports and `console.log`.
- `examples/stdlib.lift` — FFI-compatible lengths, membership, Unicode text, and trimming.
- `examples/io.lift` — arguments, stdin, and UTF-8 file round trips.
- `examples/registry.lift` — `num-integer` 0.1.46 imported with an exact version.
- `examples/modules/main.lift` — recursive file modules, global declarations, repeated imports, and cycles.
- `examples/packages/app` — Forklift manifest, transitive local libraries, qualified calls, private nominal types, closures, mutation and Rust FFI.
- `examples/methods-closures-templates.lift` - receiver mutation, snapshot captures, nested templates, and evaluation order.
- `examples/liveness.lift` — last-use copies, joins, returns, loops, and reassignment validated through Rust compilation.
- `examples/option.lift` — optional values, chained lazy fallbacks, and exactly-once effects.
- `examples/notes-service/main.lift` — starts the HTTP/SQLite local service proof of concept.
- `examples/notes-service/db-smoke.lift` — exercises `save_note` and `list_notes` through safe FFI.

Run the workspace suite with:

```sh
cargo test --locked --workspace
cargo fmt --all --check
```

`lift_db`, `lift_web`, and the fuzz project have independent workspaces; their
formatting and integration checks also run in CI. See the [testing and release
matrix](docs/testing-and-release.md) for feature contracts and the release,
fuzzing, and VS Code gates.

The CLI tests copy the executable to another directory and run `check`, `build`, and `run` with the bundled core. They inspect Cargo metadata, exercise arguments/stdin/files, and check runtime failures. The registry test is excluded from the default suite because it may access the network; run it explicitly for release acceptance:

```sh
cargo test --locked -p lift_cli --test cli_end_to_end registry_import_consumes_a_real_pinned_rust_crate -- --ignored --exact
```

That test checks, builds, and runs the pinned registry example. The [Lift release
acceptance workflow](.github/workflows/release-acceptance.yml) runs it on `v*`
tags or manual dispatch, together with the workspace, MSRV, formatting,
PostgreSQL, and HTTP/SQLite acceptance checks. Passing tests cover the documented
cases; they do not establish compatibility with every safe Rust crate or every
platform.

## Repository map

See [the source map](docs/source-map.md) for the role of each crate, source module, example, and contract document. For the Brazilian Portuguese guide, see [README.pt-BR.md](README.pt-BR.md) and [docs/source-map.pt-BR.md](docs/source-map.pt-BR.md).
