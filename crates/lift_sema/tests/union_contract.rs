//! `type A = B | C` unions desugared to nominal enums.
//!
//! PT-BR: uniões `type A = B | C` desaçucaradas para enums nominais.

use lift_sema::{
    analyze, HirDeclaration, HirExprKind, HirFunction, HirMatchPattern, HirProgram, HirStatement,
    ResolvedType, SemaError, SemaErrorKind,
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
fn unions_desugar_to_enums_with_typed_variants() {
    let program = lower(
        r#"
        interface Circle { radius: number }
        interface Square { side: number }
        type Shape = Circle | Square;
        function area(shape: Shape): number {
            match shape {
                Shape.Circle(circle) => { return circle.radius * circle.radius; }
                Shape.Square(square) => { return square.side * square.side; }
            }
        }
        const shape: Shape = Shape.Circle({ radius: 2.0 });
        const shapes: Shape[] = [shape, Shape.Square({ side: 3.0 })];
        console.log(area(shape));
        "#,
    )
    .unwrap();
    let shape = enum_decl(&program, "Shape");
    assert_eq!(shape.variants.len(), 2);
    assert_eq!(shape.variants[0].name, "Circle");
    assert_eq!(
        shape.variants[0].fields,
        vec![ResolvedType::Struct("Circle".into())]
    );
    assert_eq!(shape.variants[1].name, "Square");
    assert_eq!(
        shape.variants[1].fields,
        vec![ResolvedType::Struct("Square".into())]
    );
    let area = function(&program, "area");
    let HirStatement::Match(match_) = &area.body.statements[0] else {
        panic!("match")
    };
    let HirMatchPattern::EnumVariant {
        enum_name,
        variant,
        bindings,
        ..
    } = &match_.arms[0].pattern
    else {
        panic!("pattern")
    };
    assert_eq!(enum_name, "Shape");
    assert_eq!(variant, "Circle");
    assert_eq!(bindings[0].ty, ResolvedType::Struct("Circle".into()));
    let init = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Statement(HirStatement::Variable(variable))
                if variable.name == "shape" =>
            {
                Some(variable)
            }
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        init.initializer.kind,
        HirExprKind::EnumConstruct { .. }
    ));
}

#[test]
fn unions_support_recursion_interface_fields_and_enum_members() {
    let program = lower(
        r#"
        interface Node { value: int, children: Tree[] }
        type Tree = Node;
        enum Direction { North, South }
        interface Other { label: string }
        type Either = Direction | Other;
        interface Holder { shape: Either }
        const leaf: Tree = Tree.Node({ value: 1, children: [] });
        "#,
    )
    .unwrap();
    let tree = enum_decl(&program, "Tree");
    assert_eq!(
        tree.variants[0].fields,
        vec![ResolvedType::Struct("Node".into())]
    );
    assert!(tree.variants[0].field_boxed.is_empty() || tree.variants[0].field_boxed[0] == false);
    let either = enum_decl(&program, "Either");
    assert_eq!(
        either.variants[0].fields,
        vec![ResolvedType::Struct("Direction".into())]
    );
    assert!(either.variants[0].field_boxed[0]);
    assert_eq!(
        either.variants[1].fields,
        vec![ResolvedType::Struct("Other".into())]
    );
    assert!(program.declarations.iter().any(|declaration| matches!(
        declaration,
        HirDeclaration::Struct(structure) if structure.name == "Holder"
    )));
}

#[test]
fn unions_reject_invalid_members_and_names() {
    let cases: &[(&str, SemaErrorKind, &str)] = &[
        (
            "type Shape = Circle | Circle; interface Circle { radius: number }",
            SemaErrorKind::DuplicateDeclaration,
            "membro duplicado",
        ),
        (
            "type Value = int | string;",
            SemaErrorKind::UnsupportedType,
            "tipo nominal",
        ),
        (
            "type Shape = Missing | Circle; interface Circle { radius: number }",
            SemaErrorKind::UnknownType,
            "Missing",
        ),
        (
            "type Shape = void | Circle; interface Circle { radius: number }",
            SemaErrorKind::UnsupportedType,
            "void",
        ),
        (
            "enum Shape { A } type Shape = Other; interface Other { x: int }",
            SemaErrorKind::DuplicateDeclaration,
            "duplicada",
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
