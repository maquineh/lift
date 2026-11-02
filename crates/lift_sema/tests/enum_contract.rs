//! Enums nominais com payloads posicionais e match exaustivo.
//!
//! PT-BR: contratos de enum nominal com payloads e match.

use lift_sema::{
    analyze, CloneMode, HirDeclaration, HirExprKind, HirFunction, HirMatchPattern, HirProgram,
    HirStatement, ResolvedType, SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}

fn function<'a>(program: &'a HirProgram, name: &str) -> &'a HirFunction {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Function(function) if function.name == name => Some(function),
            _ => None,
        })
        .unwrap()
}

fn enum_decl<'a>(program: &'a HirProgram, name: &str) -> &'a lift_sema::HirEnum {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Enum(enumeration) if enumeration.name == name => Some(enumeration),
            _ => None,
        })
        .unwrap()
}

#[test]
fn enums_construct_match_and_compare_payloads() {
    let program = lower(
        r#"
        enum Direction { North, East, South, West }
        enum Shape { Circle(number), Rect(number, number), Empty }
        interface CircleData { radius: number }
        enum Wrapped { Value(CircleData), Empty }

        function area(shape: Shape): number {
            match shape {
                Shape.Circle(radius) => { return 3.14 * radius * radius; }
                Shape.Rect(width, height) => { return width * height; }
                Shape.Empty => { return 0.0; }
            }
        }
        function turn(direction: Direction): Direction {
            match direction {
                Direction.North => { return Direction.East; }
                Direction.East => { return Direction.South; }
                Direction.South => { return Direction.West; }
                Direction.West => { return Direction.North; }
            }
        }
        const shapes: Shape[] = [Shape.Circle(2.0), Shape.Rect(2.0, 3.0), Shape.Empty];
        const directions: Direction[] = [Direction.North, Direction.West];
        const wrapped = Wrapped.Value({ radius: 2.0 });
        console.log(turn(Direction.North) == Direction.East);
        console.log(Shape.Circle(1.0) == Shape.Empty);
        "#,
    )
    .unwrap();
    let shape = enum_decl(&program, "Shape");
    assert_eq!(shape.variants.len(), 3);
    assert_eq!(shape.variants[0].fields, vec![ResolvedType::Number]);
    assert_eq!(
        shape.variants[1].fields,
        vec![ResolvedType::Number, ResolvedType::Number]
    );
    assert!(shape.variants[2].fields.is_empty());
    assert_eq!(
        enum_decl(&program, "Wrapped").variants[0].fields,
        vec![ResolvedType::Struct("CircleData".into())]
    );

    let area = function(&program, "area");
    let HirStatement::Match(match_) = &area.body.statements[0] else {
        panic!("match")
    };
    assert!(matches!(
        match_.value.kind,
        HirExprKind::Clone {
            mode: CloneMode::BorrowedAggregate,
            ..
        }
    ));
    let HirMatchPattern::EnumVariant {
        enum_name,
        variant,
        bindings,
        ..
    } = &match_.arms[0].pattern
    else {
        panic!("enum pattern")
    };
    assert_eq!(enum_name, "Shape");
    assert_eq!(variant, "Circle");
    assert_eq!(bindings[0].ty, ResolvedType::Number);

    let shapes = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Statement(HirStatement::Variable(variable))
                if variable.name == "shapes" =>
            {
                Some(variable)
            }
            _ => None,
        })
        .unwrap();
    let HirExprKind::ArrayLiteral(elements) = &shapes.initializer.kind else {
        panic!("array")
    };
    assert!(matches!(
        elements[0].kind,
        HirExprKind::EnumConstruct { .. }
    ));
    assert!(matches!(
        elements[1].kind,
        HirExprKind::EnumConstruct { .. }
    ));
    assert!(matches!(
        elements[2].kind,
        HirExprKind::EnumConstruct { .. }
    ));
}

#[test]
fn enum_match_requires_qualified_exhaustive_patterns() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "enum Direction { North, South } function f(d: Direction): int { match d { Direction.North => { return 1; } } }",
            SemaErrorKind::InvalidMatch,
            "não é exaustivo",
        ),
        (
            "enum Direction { North, South } function f(d: Direction): int { match d { Direction.North => { return 1; } Direction.North => { return 2; } Direction.South => { return 0; } } }",
            SemaErrorKind::InvalidMatch,
            "duplicado",
        ),
        (
            "enum Direction { North, South } enum Other { A, B } function f(d: Direction): int { match d { Other.A => { return 1; } Other.B => { return 0; } } }",
            SemaErrorKind::InvalidMatch,
            "enum",
        ),
        (
            "enum Shape { Circle(number) } function f(s: Shape): int { match s { Shape.Circle() => { return 1; } } }",
            SemaErrorKind::InvalidMatch,
            "binding",
        ),
        (
            "enum Shape { Circle(number) } const value = Shape.Triangle(1.0);",
            SemaErrorKind::UnknownField,
            "variante",
        ),
        (
            "enum Shape { Circle(number) } const value = Shape.Circle();",
            SemaErrorKind::ArgumentCountMismatch,
            "payload",
        ),
        (
            "enum Shape { Circle(number) } const value = Shape.Circle(\"x\");",
            SemaErrorKind::TypeMismatch,
            "number",
        ),
        (
            "enum Shape { Circle(number) } const value = Shape.Circle;",
            SemaErrorKind::ArgumentCountMismatch,
            "payload",
        ),
        (
            "interface Point { x: int } function f(p: Point): int { match p { Point.x => { return 1; } } }",
            SemaErrorKind::TypeMismatch,
            "enum",
        ),
        (
            "enum Direction { North, South } function f(d: Direction): int { match d { Direction.North => { return 1; } Direction.South => { console.log(0); } } }",
            SemaErrorKind::MissingReturnPath,
            "",
        ),
    ];
    for (source, kind, message) in cases {
        let errors = lower(source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.kind == *kind
                && (message.is_empty() || error.message.contains(message))),
            "expected {kind:?}/{message} in {errors:?}"
        );
    }
}

#[test]
fn recursive_enums_and_interface_fields_use_box_indirection() {
    let program = lower(
        r#"
        enum Direction { North, South }
        interface Holder { direction: Direction }
        enum List { Nil, Cons(int, List) }
        enum Tree {
            Leaf,
            Node { value: int, left: Tree, right: Tree },
        }
        const value: List = List.Cons(1, List.Nil);
        console.log(value == List.Nil);
        "#,
    )
    .unwrap();
    let list = enum_decl(&program, "List");
    let cons = &list.variants[1];
    assert_eq!(
        cons.fields,
        vec![ResolvedType::Int, ResolvedType::Struct("List".into())]
    );
    assert_eq!(cons.field_boxed, vec![false, true]);
    let tree = enum_decl(&program, "Tree");
    let node = &tree.variants[1];
    assert_eq!(node.named_fields.len(), 3);
    assert!(!node.named_fields[0].boxed);
    assert!(node.named_fields[1].boxed);
    assert!(node.named_fields[2].boxed);
    let holder = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Struct(structure) if structure.name == "Holder" => Some(structure),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        holder.fields[0].ty,
        ResolvedType::Struct("Direction".into())
    );

    // Ciclo struct↔enum: o payload de Holder é boxeado, quebrando o tamanho.
    lower(
        r#"
        enum Wrapper { Value(Holder), Empty }
        interface Holder { wrapper: Wrapper? }
        const empty: Holder = { wrapper: null };
        "#,
    )
    .unwrap();
}

#[test]
fn enum_limits_reject_ffi_and_duplicates() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "enum Empty { }",
            SemaErrorKind::UnsupportedType,
            "ao menos uma variante",
        ),
        (
            "enum Direction { North, North }",
            SemaErrorKind::DuplicateDeclaration,
            "variante duplicada",
        ),
        (
            "enum Bad { Value(DbValue) }",
            SemaErrorKind::UnsupportedType,
            "DbValue",
        ),
        (
            "enum Direction { North } import function f(d: Direction): int from \"lift_core::interop::f\";",
            SemaErrorKind::UnsupportedType,
            "FFI",
        ),
        (
            "enum Pair { Value(int, int) } function f(p: Pair): int { match p { Pair.Value(a) => { return a; } } }",
            SemaErrorKind::InvalidMatch,
            "binding",
        ),
    ];
    for (source, kind, message) in cases {
        let errors = lower(source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == *kind && error.message.contains(message)),
            "expected {kind:?}/{message} in {errors:?}"
        );
    }
    lower("enum Wrapped { Value(Point), Empty } interface Point { x: int }").unwrap();
}

#[test]
fn enum_payloads_preserve_option_flow_and_value_semantics() {
    let program = lower(
        r#"
        enum Maybe { Value(int?), Empty }
        function get(maybe: Maybe): int {
            match maybe {
                Maybe.Value(present) => {
                    if present != null { return present; }
                    return 0;
                }
                Maybe.Empty => { return -1; }
            }
        }
        const first = Maybe.Value(option.some(5));
        console.log(get(first));
        console.log(get(Maybe.Value(null)));
        "#,
    )
    .unwrap();
    let get = function(&program, "get");
    let HirStatement::Match(match_) = &get.body.statements[0] else {
        panic!("match")
    };
    let HirMatchPattern::EnumVariant { bindings, .. } = &match_.arms[0].pattern else {
        panic!("pattern")
    };
    assert_eq!(
        bindings[0].ty,
        ResolvedType::Optional(Box::new(ResolvedType::Int))
    );
    assert!(matches!(
        match_.value.kind,
        HirExprKind::Clone {
            mode: CloneMode::BorrowedAggregate,
            ..
        }
    ));
}

#[test]
fn enum_named_fields_construct_and_match_in_field_order() {
    let program = lower(
        r#"
        enum Shape {
            Circle { radius: number },
            Rect { width: number, height: number },
            Empty,
        }
        function area(shape: Shape): number {
            match shape {
                Shape.Circle(radius) => { return radius; }
                Shape.Rect(width, height) => { return width * height; }
                Shape.Empty => { return 0.0; }
            }
        }
        const value = Shape.Circle({ radius: 2.0 });
        "#,
    )
    .unwrap();
    let shape = enum_decl(&program, "Shape");
    assert!(shape.variants[0].fields.is_empty());
    assert_eq!(shape.variants[0].named_fields.len(), 1);
    assert_eq!(shape.variants[0].named_fields[0].name, "radius");
    assert_eq!(shape.variants[0].named_fields[0].ty, ResolvedType::Number);
    let area = function(&program, "area");
    let HirStatement::Match(match_) = &area.body.statements[0] else {
        panic!("match")
    };
    let HirMatchPattern::EnumVariant {
        named, bindings, ..
    } = &match_.arms[0].pattern
    else {
        panic!("pattern")
    };
    assert!(named);
    assert_eq!(bindings.len(), 1);
    let value = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Statement(HirStatement::Variable(variable))
                if variable.name == "value" =>
            {
                Some(variable)
            }
            _ => None,
        })
        .unwrap();
    let HirExprKind::EnumConstruct { named_fields, .. } = &value.initializer.kind else {
        panic!("constructor")
    };
    assert_eq!(named_fields.len(), 1);
    assert_eq!(named_fields[0].name, "radius");
}

#[test]
fn enum_named_fields_reject_invalid_object_payloads() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "enum Shape { Circle { radius: number } } const v = Shape.Circle({ radius: \"x\" });",
            SemaErrorKind::TypeMismatch,
            "number",
        ),
        (
            "enum Shape { Circle { radius: number } } const v = Shape.Circle({ });",
            SemaErrorKind::MissingObjectField,
            "radius",
        ),
        (
            "enum Shape { Circle { radius: number } } const v = Shape.Circle({ radius: 1.0, extra: 2.0 });",
            SemaErrorKind::UnexpectedObjectField,
            "extra",
        ),
        (
            "enum Shape { Circle { radius: number } } const v = Shape.Circle({ radius: 1.0, radius: 2.0 });",
            SemaErrorKind::DuplicateObjectField,
            "radius",
        ),
        (
            "enum Shape { Circle { radius: number } } const v = Shape.Circle(2.0);",
            SemaErrorKind::TypeMismatch,
            "campo: valor",
        ),
        (
            "enum Shape { Circle { radius: number } } const v = Shape.Circle;",
            SemaErrorKind::ArgumentCountMismatch,
            "payload",
        ),
        (
            "enum Shape { Circle { radius: number } } function f(s: Shape): int { match s { Shape.Circle() => { return 1; } } }",
            SemaErrorKind::InvalidMatch,
            "binding",
        ),
    ];
    for (source, kind, message) in cases {
        let errors = lower(source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == *kind && error.message.contains(message)),
            "expected {kind:?}/{message} in {errors:?}"
        );
    }
}
