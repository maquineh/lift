# Methods, closures, and templates v1

Rust/Cargo **1.94.1+** is required for the compiler, bundled core, and generated
projects. This is a defined Truxt subset, not full TypeScript compatibility.
Run `cargo run -p truxt_cli -- run examples/methods-closures-templates.truxt`.

## Nominal methods

```truxt
interface Counter { value: int }
impl Counter {
    add(n: int): int { this.value = this.value + n; return this.value; }
    snapshot(): Counter { return this; }
}
const c: Counter = { value: 1 };
console.log(c.add(c.value));
```

An `impl` targets an existing nominal interface. Methods use either `name(...)`
or `function name(...)` and the ordinary typed function/return rules. `this` is
an implicit first parameter; rebinding it or declaring an explicit `this`
parameter is rejected. Methods are resolved by the receiver's nominal type;
they are not properties, function values, static functions, overloads, traits,
inheritance, or dynamic dispatch. Duplicate methods are errors. Field reads
remain field reads; extracting a method as `const f = c.add` is unsupported.

Semantic lowering creates an internal function with `this` as its first HIR
argument before ordinary ownership and alias analysis. A read-only receiver is
borrowed, a mutated receiver is borrowed mutably. Returning `this` produces an
owned clone, so a mutation still affects the caller's receiver. Arguments follow
existing Truxt value/copy rules, including cloning owned non-Copy parameters.
Receiver evaluation precedes explicit arguments; established argument staging
materializes conflicting reads/effects before creating the final borrow.
`c.add(c.value)` and `c.add(c.add(1))` are supported and compiled in the E2E test;
two simultaneous mutable arguments rooted at `c` are rejected. Mutable temporary
receivers and unsupported projected alias patterns receive existing diagnostics.
Mutation summaries propagate to callers to a fixed point. A receiver that is a
direct nominal parameter selects its exact method summary. For local/projected
receivers whose type is unavailable to the summary walker, it conservatively
unions same-named method summaries; this can request a mutable borrow even when
another nominal type owns that method.

## Local arrow closures

```truxt
let text = "before";
const snapshot = (): string => text;
text = "after";
console.log(snapshot()); // before
const plus = (x: int): int => x + 1;
console.log(plus(2));
```

Closures require parentheses and explicit parameter and return types. Bodies
may be expressions or blocks; non-void blocks must return on every path.
Closures may initialize a direct `const` local or be invoked immediately. Named
closures can be called repeatedly. Parameters are owned values (non-Copy
arguments are cloned when necessary); parameter projections and ordinary locals
can be mutated, using the normal prohibition on parameter rebinding.

Free local names are resolved lexically at construction. Captures are independent
owned **snapshots**, copied for primitives and explicitly cloned for strings,
structs, arrays, Option and Result. This intentionally differs from TypeScript's
live captured bindings. Later changes to outer bindings/projections are not
visible. Captures cannot be reassigned or mutated, including through a mutating
method/function. `this` inside an arrow is a snapshot of the enclosing receiver.
Global named functions remain directly callable; functions themselves cannot be
captured as values. Capturing another closure, recursion through the local
closure name, reassignment/copying of closure bindings, closure fields/arrays,
returning closures, callback parameters, function type annotations, and FFI
closures are rejected. Standard-library map/filter/find callback adapters remain
Rust-only. Nested closures are supported when they only capture ordinary values.

The HIR records the callable signature, typed body, parameter IDs, and each
capture's fresh binding ID plus explicit owned initializer/CloneMode. Rust emits
a `move` closure. Ownership checking ensures the body does not consume captures,
so repeated calls are valid. Liveness observes capture initializers at creation
and never visits the deferred body in the outer CFG; the body keeps conservative
clones instead of borrowing the outer frame or applying outer last-use moves.

## Template strings

```truxt
console.log(`value ${c.value}, nested ${`ok ${true}`}`);
console.log(`literal \${text}, tick \` and newline\n`);
```

Backticks delimit multiline templates and `${expression}` accepts ordinary
expressions, including nested templates, strings containing braces, comments,
object literals and calls. The contextual lexer balances interpolation braces
and preserves half-open UTF-8 byte spans and SourceId, including nested templates.
Unclosed templates/interpolations are lexical errors; empty or trailing-token
interpolations are parser errors. Tagged templates are unsupported.

Only int, number, boolean and string interpolate. Each expression executes once,
left to right, and converts immediately to an owned String before the next
interpolation. For `${c.value}/${c.add(1)}/${c.value}`, the first value is captured
before mutation and the final value after mutation. HIR uses reserved stringify
calls borrowing one primitive and ordered String concatenation; Rust uses
`format!` with a constant format string, so source braces cannot inject a format.
Formatting follows Rust Display (not all JavaScript/TypeScript number formatting).
Escapes match ordinary Truxt strings: `\\`, `\"`, `\'`, `\n`, `\r`, `\t`, `\0`;
`\`` and `\$` protect a backtick and interpolation opener. Unknown escapes drop
the backslash. Unicode escape syntax, raw tagged forms, and JS line-continuation
rules are not implemented; Unicode characters may be written directly.

The semantic contract tests cover signatures, snapshot HIR, alias rejection,
invalid capture/escape/container patterns, contextual lexical nesting, and UTF-8
diagnostics. CLI integration checks generated Rust and executes the combined
example, comparing its output. These tests cover the exercised combinations,
not unrestricted Rust borrow checking or full TypeScript compatibility.
