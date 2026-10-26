# Explicit file modules

[Português do Brasil](modules.pt-BR.md) · [User guide](../README.md)

This document describes legacy flat file imports, preserved without a manifest.
The separate [Forklift increment](forklift.md) adds `lift.toml`, package
namespaces and exported functions; its file modules remain flat within each package.

`import module "relative.lift";` is a top-level declaration, parsed separately from Rust FFI imports. The semicolon is required. `module` is contextual here and remains available as an ordinary identifier elsewhere. Nested imports, aliases, namespaces, exports, package search paths, and a project manifest are outside this increment.

Each path is relative to the importing source file, independently of the caller's current directory. It must have a `.lift` extension (case-insensitive, consistent with CLI entry-file validation). Absolute/rooted paths and drive-qualified paths are rejected. `..` is allowed. Specifiers use their literal token text with no interpolation or runtime string-escape decoding; use `/` for portable paths.

The CLI canonicalizes and deduplicates files before following their imports. Each canonical source is read/parsed once in a compilation. A visited file is marked before recursion, so repeated imports, module-to-module cycles, and imports back to the entry file do not duplicate declarations or run entry statements again. File imports have no runtime execution step.

## Declarations and execution

Only the entry/root file may contain executable top-level statements, including `const`/`let`, assignments, calls, conditionals, and loops. Root statements still obey existing semantic rules; `return` requires a function body. Imported modules may contain functions, interfaces, safe Rust FFI imports, and other file imports. A statement in an imported module is a source error even if it would not produce output. Empty modules are valid.

All functions, interfaces, and FFI declarations join the existing global symbol table. Forward calls and cross-file calls/types work after the complete graph is loaded. Duplicate global declarations retain primary and secondary spans, so Ariadne labels both files. There is no file-private visibility. Existing rules still apply: functions do not capture top-level variables, and interfaces/functions/FFI names cannot be duplicated to simulate namespaces.

The loader removes module-import declarations before calling semantic analysis. The public semantic `analyze` function does not access the filesystem; unresolved module imports passed directly to it are rejected rather than producing partial HIR.

## Source identity and diagnostics

`Span` now contains a `SourceId` and half-open UTF-8 byte offsets. `Span::new` and `tokenize` retain source ID zero for existing single-source consumers. `Span::in_source`, `tokenize_with_source`, and `parse_with_source` allow explicit identities; `parse` preserves the identity from its tokens. Tokens given to `parse` belong to one source. `parse_with_source` normalizes every token span to its explicit ID, including tokens from the legacy API, and assigns an identity to an empty program. Joining spans from different sources is rejected.

Adding the public `source` field is a source-level breaking change for external users that constructed `Span { start, end }` directly; they must add `source` or use a constructor. The preserved single-source constructors and entry points do not imply full source compatibility.

The CLI owns a table of canonical paths and UTF-8 source text. AST and HIR spans retain IDs through analysis; Ariadne resolves each label through this table in byte-index mode. Cache identity includes SourceId independently of displayed paths, so distinct OS paths cannot collide through lossy Unicode display. Lexical, syntax, semantic, and EOF errors inside a module point to that module. Missing/unreadable/non-UTF-8 module files are labeled at the importing path literal, because no valid target source is available. Source failures retain exit code `1`.

## Generation and limits

The graph produces one typed program, one generated Rust source, and one executable in the entry file's cache. Generated-file byte reuse and Cargo incrementality remain in effect. The entire graph is loaded and analyzed on each command; this is not a dependency-hash cache. Changing a transitive module affects the next command even if the entry file is unchanged.

`examples/modules/main.lift` exercises shared interfaces/functions, a Rust FFI declaration in a module, importer-relative transitives, duplicate paths, module cycles, and an entry-file cycle. Tests check stdout and exactly-once root execution, transitive source changes, statement rejection, imported UTF-8/EOF errors, duplicate labels across files, invalid paths, and empty modules. They do not claim support for dynamic loading or separate compilation.
