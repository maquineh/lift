# Enums

[Português do Brasil](enums.pt-BR.md) · [User guide](../README.md)

Lift enums are nominal tagged unions with positional payloads. Variants are constructed with `Enum.Variant(...)` and matched exhaustively.

```lift
enum Shape {
    Circle(number),
    Rect(number, number),
    Empty,
}

function area(shape: Shape): number {
    match shape {
        Shape.Circle(radius) => { return 3.14 * radius * radius; }
        Shape.Rect(width, height) => { return width * height; }
        Shape.Empty => { return 0.0; }
    }
}

const shapes: Shape[] = [Shape.Circle(2.0), Shape.Rect(2.0, 3.0), Shape.Empty];
```

## Declaration

```text
enum Name {
    Variant,
    Variant(Type, Type),
    Variant { field: Type, other: Type },
}
```

- Variants are identified by name and have no payload, **positional** payload types, or **named fields** (mutually exclusive forms). Commas between variants/fields and a trailing comma are optional.
- Payload types are ordinary Lift value types: primitives, `string`, arrays, interfaces, `Option`, and `Result`. A variant payload cannot contain another enum, an opaque resource, a closure, or `DbValue` in this increment.
- Enum declarations may appear before or after the functions that use them. File modules share the flat namespace, so an enum declared in an imported module is visible to the package; package-qualified enums follow the same rules as other local types.
- An enum needs at least one variant. Duplicate enum names (against interfaces/functions) and duplicate variant names are rejected.

## Construction and value semantics

```lift
const empty = Shape.Empty;
const circle = Shape.Circle(2.0);
const named = Shape.NamedCircle({ radius: 2.0 });
```

- Positional variants use `Enum.Variant(args...)`; named-field variants use one object literal `Enum.Variant({ field: value, ... })`. The payload count, field names, missing/unknown/duplicate fields, and each value type are checked before code generation; values are materialized as owned.
- A variant with payload must be called; a variant without payload is used directly. Empty parentheses are accepted for a payload-less variant.
- Enums are value types: they can be stored in arrays, passed/returned, and used as `Option`/`Result` payloads. Assignments/copies follow the normal Lift clone rules (the Rust enum derives `Clone, Debug, PartialEq`).
- Equality is structural: `a == b` / `a != b` compare the variant and payloads. Ordering (`<`, `>`) is not supported.
- An enum cannot be used as an interface field or an FFI parameter/return in this increment. These positions are rejected before codegen.

## Matching

`match` requires one arm per variant (see [matching](match.md)):

```lift
match shape {
    Shape.Circle(radius) => { console.log(radius); }
    Shape.Rect(width, height) => { console.log(width + height); }
    Shape.Empty => { console.log(0); }
}
```

- Patterns are qualified (`Shape.Circle(...)`). Positional variants bind arguments in order; named-field variants bind in field declaration order with the same positional pattern syntax (for example, `Shape.Circle(radius)` for `{ radius: number }`). The binding count must match the payload count.
- Bindings are immutable values scoped to the arm; their fields can be mutated when the payload is a struct (the generated pattern uses `mut`).
- The pattern enum name must equal the scrutinee enum. Missing arms, duplicate variants, unknown variants, and patterns from another enum are `sema.InvalidMatch`.
- A match is exhaustive when every declared variant has exactly one arm. There is no wildcard/`_` arm, guard, or nested pattern in this increment.
- The scrutinee is evaluated once and cloned when it is a place, so the original enum remains usable; payloads are owned values from that clone. Matching an enum is a statement, not an expression.
- `return`, `throw`, `?`, `break`, and `continue` work inside arms with the same targets as other `match` arms.

## Limits

- Explicit discriminants/numeric values, methods/`impl` on enums, generic variants, and recursive enums (a variant cannot carry the enum itself or another enum). Enums cannot be interface fields yet.
- Enums cannot appear in interface fields, external layouts, callbacks payloads through FFI, or `DbValue` positions.
- TypeScript `type A = B | C` union syntax is not implemented; tagged unions are expressed with this nominal `enum`.
- No wildcard/guard/nested patterns, no enum match expression, and no `switch`.
- Equality requires identical enum types; there is no coercion between enums.

Parser, semantic, codegen, and CLI tests cover declarations, positional and named payload construction, exhaustive match, duplicate/missing variants/fields, limits, equality, value semantics, and native execution of [`examples/enums.lift`](../examples/enums.lift) and [`examples/enum-fields.lift`](../examples/enum-fields.lift).
