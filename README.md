# Truxt

Truxt is a small, statically typed language that compiles a single `.truxt` source file to Rust and then uses Cargo to check, build, or run it.

> **v0.1 scope:** a compiler for single-file programs, with a bundled standard library, safe Rust imports, console output, and UTF-8 input/files. The installed executable can be moved independently of this checkout. Rust, Cargo, and a host linker are still required to compile programs.

## Requirements

- A stable Rust toolchain with `rustc` and `cargo` available on `PATH`.
- A working linker for the Rust host target.
- Network access when a program imports a registry crate that is not already cached by Cargo.

The current validation environment is Windows x86_64 with Rust/Cargo 1.94.1 and the MSVC host target. Other hosts and a minimum supported Rust version have not been validated yet. Build and run for the host target; cross-target binaries may not execute on the current machine.

## Build the CLI from this checkout

From the repository root:

```sh
cargo build --release -p truxt_cli
```

The executable is written to `target/release/truxt` (or `truxt.exe` on Windows). Its standard-library sources are embedded at build time, so the executable does not need the original checkout afterward.

## Install or create a portable package

Install from the checkout into Cargo's executable directory:

```sh
cargo install --path crates/truxt_cli --locked
truxt --version
```

You can also copy the release executable into a directory on `PATH`. Only the executable is needed from Truxt; Rust/Cargo and the host linker remain prerequisites. On PowerShell, create a ZIP containing the executable, bilingual guides, and examples with:

```powershell
./scripts/package.ps1
```

Extract `dist/truxt-<version>-<host>.zip` anywhere and add its directory to `PATH`, or call its executable by path. The package script builds for the current Rust host. It does not install Rust or modify `PATH`.

## Quick start

Create a starter source file and run it:

```sh
cargo run -p truxt_cli -- init hello
```

Edit `hello/main.truxt`:

```truxt
console.log("Hello, Truxt!");
```

Then run the program:

```sh
cargo run -p truxt_cli -- run hello/main.truxt
```

With the built executable on `PATH`, the same commands are `truxt init hello` and `truxt run hello/main.truxt`.

## CLI commands

| Command | Behavior |
| --- | --- |
| `truxt init [directory]` | Create `main.truxt`; refuse to overwrite an existing file. |
| `truxt check file.truxt` | Parse and analyze Truxt, generate Rust, then run `cargo check`. |
| `truxt build file.truxt` | Compile in release mode and copy the executable next to the source file. |
| `truxt run file.truxt [-- args...]` | Compile in release mode and run the generated executable, forwarding arguments. |

Generated Rust projects and Cargo build artifacts are stored under `.truxt_cache/<source-name>/` beside the source file. The CLI extracts its embedded core into that project's `truxt_core/` directory and uses a relative Cargo dependency. Unchanged embedded files are reused; changes to that extracted copy are overwritten on the next command. Each CLI invocation handles one `.truxt` file; multi-file projects are not supported yet.

## Language and interop scope

The current implementation supports typed functions, primitive values (`int`, `number`, `boolean`, `string`), arrays, interfaces/object literals, control flow, and the ownership patterns covered by the examples and tests. The language does not insert semicolons at newlines. Imports are top-level declarations and require a trailing semicolon.

Rust imports declare an explicit Truxt signature and a Rust path:

```truxt
import function concat(left: string, right: string): string from "truxt_core::string::concat";
console.log(concat("Hello, ", "Truxt!"));
```

The FFI accepts safe Rust free functions with scalar and one-level array types. `string` parameters are passed as `&str` and string returns as owned `String`; array parameters are borrowed slices and array returns are owned `Vec<T>`. `void` maps to `()`. An import may also return `Result<T, E>` with supported payload types; Result parameters and nested Result returns remain unsupported by the FFI. Imports from registry crates use an exact `MAJOR.MINOR.PATCH` version. The generated Rust wrapper lets `rustc` verify the target path and signature. Struct FFI, `unsafe`, and nested arrays are unsupported.

`console.log` accepts one `int`, `number`, `boolean`, or `string` expression and writes it with a newline. It currently panics if writing to stdout fails.

The Rust-native `truxt_core` crate includes console, array, and string helpers. FFI-compatible adapters live under `truxt_core::interop`:

```truxt
import function length(values: int[]): int from "truxt_core::interop::array_len";
import function trim(value: string): string from "truxt_core::interop::string_trim";
import function contains(values: int[], value: int): boolean from "truxt_core::interop::array_contains_int";
```

The adapters also provide `string_len` and `array_contains_number`, `array_contains_boolean`, and `array_contains_string`. Callback helpers (`map`, `filter`, and `find`) remain Rust-only because Truxt does not yet support closures. See [the core library guide](stdlib/truxt_core/README.md) for the full API.

## Input and files

`Result<T, E>` is a special type available in annotations and Truxt function signatures. Use explicit imports for UTF-8 arguments, standard input, and files:

```truxt
import function arguments(): Result<string[], string> from "truxt_core::io::arguments";
import function read_stdin(): Result<string, string> from "truxt_core::io::read_stdin";
import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";
import function write_file(path: string, contents: string): Result<void, string> from "truxt_core::io::write_file";
```

`arguments()` returns the arguments without the executable name and preserves order and spaces. Use `--` after the source path to forward flags: `truxt run app.truxt -- --flag "a b"`. `read_stdin()` reads until EOF, including line endings. `read_file()` requires UTF-8; `write_file()` creates or truncates a file and does not create parent directories. Relative file paths resolve from the caller's working directory, which `truxt run` preserves.

I/O failures and invalid UTF-8 become error strings rather than process panics. Handle the result with a branch:

```truxt
const contents = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

Result inspection borrows its argument, and unwrap operations clone the payload so the original Result remains available. Unwrap on the wrong variant panics. General generics, Result constructors, pattern matching, `?`, and `try/catch` are not implemented. See [the Result/I/O contract](docs/result-and-io.md) for the exact scope. For an example that reads piped input and writes it to a file:

```powershell
"Hello from stdin" | truxt run examples/io.truxt -- output.txt sample
```

The resulting file contains `Hello from stdin|sample`. The same program reads standard input until EOF when run interactively.

## Current limitations

- One source file per CLI invocation; no project manifest or multi-file module system.
- No content-keyed cache. Cargo's own incremental build artifacts are reused.
- Ownership analysis is conservative and rejects some complex aliasing patterns. It does not yet implement CFG/liveness analysis or move optimization at the last use.
- General methods/`impl`, arrow functions/closures in Truxt, optional types, `??`, template strings, unions, and `try/catch` are not implemented.
- Source diagnostics are plain text rather than Ariadne reports.
- Result support uses `is_ok` and cloned unwrap operations; constructors, matching, and automatic error propagation are not implemented.

## Examples and tests

- `examples/fibonacci.truxt` — recursive functions and control flow.
- `examples/ownership.truxt` — aggregate mutation, borrowing, and owned return values.
- `examples/interop.truxt` — local Rust imports and `console.log`.
- `examples/stdlib.truxt` — FFI-compatible lengths, membership, Unicode text, and trimming.
- `examples/io.truxt` — arguments, stdin, and UTF-8 file round trips.
- `examples/registry.truxt` — `num-integer` 0.1.46 imported with an exact version.

Run the workspace suite with:

```sh
cargo test --workspace
cargo fmt --all --check
```

The CLI tests copy the executable to another directory and run `check`, `build`, and `run` with the bundled core. They inspect Cargo metadata, exercise arguments/stdin/files, and check runtime failures. The registry test is excluded from the default suite because it may access the network; run it explicitly for release acceptance:

```sh
cargo test -p truxt_cli --test cli_end_to_end registry_import_consumes_a_real_pinned_rust_crate -- --ignored --exact
```

That test checks, builds, and runs the pinned registry example. Local tests validate the exercised language and host combinations; they do not establish compatibility with every safe Rust crate or every platform.

## Repository map

See [the source map](docs/source-map.md) for the role of each crate, source module, example, and contract document. For the Brazilian Portuguese guide, see [README.pt-BR.md](README.pt-BR.md) and [docs/source-map.pt-BR.md](docs/source-map.pt-BR.md).
