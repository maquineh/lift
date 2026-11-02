# Union types

[Português do Brasil](unions.pt-BR.md) · [User guide](../README.md)

`type A = B | C;` declares a nominal union over existing nominal types. It is desugared to an enum with one variant per member, so construction and matching reuse the enum contract.

```lift
interface Circle { radius: number }
interface Square { side: number }

type Shape = Circle | Square;

function area(shape: Shape): number {
    match shape {
        Shape.Circle(circle) => { return 3.14 * circle.radius * circle.radius; }
        Shape.Square(square) => { return square.side * square.side; }
    }
}

const shapes: Shape[] = [
    Shape.Circle({ radius: 2.0 }),
    Shape.Square({ side: 3.0 }),
];
```

## Declaration

```text
type Name = Member | Member;
```

- Each member must be a **named nominal type**: an interface, an enum, or another union. Primitive types, arrays, `Option`, `Result`, closures, and inline structural types are rejected as members; wrap them in an interface when needed.
- The union becomes a nominal enum named `Name`; each member becomes a positional variant named after the member type with one payload of that type. A union needs at least one member, and duplicate members are rejected.
- The name participates in the same type namespace as interfaces/enums: duplicate declarations are rejected. Forward references are allowed.
- `type` is a declaration form; it is not a general alias. There is no alias syntax for a non-union type and no inline `A | B` in parameter/field position.

## Construction and values

```lift
const circle = Shape.Circle({ radius: 2.0 });
const square = Shape.Square({ side: 3.0 });
```

- Variant construction follows the enum constructor rules: the single payload is checked against the member type and materialized as an owned value. Because the payload is a struct, an object literal is accepted directly.
- Union values are enum values: `Clone`, `Debug`, structural `PartialEq`, usable in arrays, parameters, returns, and `Option`/`Result` payloads.
- Recursion is supported when it passes through a field/array: for example `interface Node { children: Tree[] } type Tree = Node;`. Enum-reaching payloads receive automatic `Box` indirection; arrays already use `Vec`.
- Unions can be interface fields, enum payloads, function parameters/returns, and module-level declarations. FFI still rejects union/enum types.

## Matching

A union match is exhaustive over its member variants:

```lift
match shape {
    Shape.Circle(circle) => { console.log(circle.radius); }
    Shape.Square(square) => { console.log(square.side); }
}
```

Bindings are positional (one payload) and immutable, with mutable struct fields when needed. Missing/duplicate/unknown variants are `sema.InvalidMatch`. `return`, `throw`, `?`, `break`, and `continue` behave as in other enum arms.

## Limits

- No primitive/array/Option/Result member; introduce a wrapper interface instead.
- No inline union types, alias declarations for non-unions, or anonymous union construction.
- No discriminant/narrowing inference: matching is the only way to access a member.
- No generic unions, union methods/`impl`, or FFI unions; recursive unions use the same automatic `Box` rules as enums.
- Enums/unions remain nominal; there is no structural equality or coercion between different union names.

Parser, semantic, codegen, and CLI tests cover declaration/member validation, object-literal construction, exhaustive match, enum members, recursion, and native execution of [`examples/unions.lift`](../examples/unions.lift).
