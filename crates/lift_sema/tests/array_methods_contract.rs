//! Higher-order array method contracts (`map`, `filter`, `forEach`, `find`,
//! `some`, `every`, `reduce`).
//!
//! PT-BR: Contratos dos métodos higher-order de array.

use lift_sema::{
    analyze, ArrayMethodKind, HirDeclaration, HirExprKind, HirProgram, HirStatement, ResolvedType,
    SemaError, SemaErrorKind,
};

fn lower(source: &str) -> Result<HirProgram, Vec<SemaError>> {
    analyze(lift_parser::parse(&lift_lexer::tokenize(source).unwrap()).unwrap())
}

fn variable_initializer<'a>(program: &'a HirProgram, name: &str) -> &'a lift_sema::HirExpr {
    program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Statement(HirStatement::Variable(variable))
                if variable.name == name =>
            {
                Some(&variable.initializer)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn array_methods_are_typed_and_keep_callback_signatures() {
    let program = lower(
        r#"
        function double(value: int): int { return value * 2; }
        function sum(values: int[]): int {
            return values.reduce(0, (accumulator: int, value: int): int => accumulator + value);
        }
        const values = [1, 2, 3, 4];
        const doubled = values.map(double);
        const evens = values.filter((value: int): boolean => value % 2 == 0);
        const found = values.find((value: int): boolean => value > 2);
        const any = values.some((value: int): boolean => value > 3);
        const all = values.every((value: int): boolean => value > 0);
        const labels = values.map((value: int): string => `#${value}`);
        const sum_value = sum(values);
        "#,
    )
    .unwrap();

    let sum = program
        .declarations
        .iter()
        .find_map(|declaration| match declaration {
            HirDeclaration::Function(function) if function.name == "sum" => Some(function),
            _ => None,
        })
        .unwrap();
    let HirStatement::Return(returned) = &sum.body.statements[0] else {
        panic!("return")
    };
    let value = returned.value.as_ref().unwrap();
    let HirExprKind::ArrayMethod {
        method: ArrayMethodKind::Reduce,
        initial,
        callback,
        ..
    } = &value.kind
    else {
        panic!("reduce")
    };
    assert!(initial.is_some());
    assert_eq!(
        callback.ty,
        ResolvedType::Closure(
            vec![ResolvedType::Int, ResolvedType::Int],
            Box::new(ResolvedType::Int)
        )
    );
    assert_eq!(value.ty, ResolvedType::Int);

    let doubled = variable_initializer(&program, "doubled");
    let HirExprKind::ArrayMethod {
        method: ArrayMethodKind::Map,
        ..
    } = &doubled.kind
    else {
        panic!("map")
    };
    assert_eq!(doubled.ty, ResolvedType::Array(Box::new(ResolvedType::Int)));
    assert_eq!(
        variable_initializer(&program, "evens").ty,
        ResolvedType::Array(Box::new(ResolvedType::Int))
    );
    assert_eq!(
        variable_initializer(&program, "found").ty,
        ResolvedType::Optional(Box::new(ResolvedType::Int))
    );
    assert_eq!(
        variable_initializer(&program, "any").ty,
        ResolvedType::Boolean
    );
    assert_eq!(
        variable_initializer(&program, "all").ty,
        ResolvedType::Boolean
    );
    assert_eq!(
        variable_initializer(&program, "labels").ty,
        ResolvedType::Array(Box::new(ResolvedType::String))
    );
    assert_eq!(
        variable_initializer(&program, "sum_value").ty,
        ResolvedType::Int
    );
}

#[test]
fn array_methods_clone_the_receiver_and_chain() {
    let program = lower(
        r#"
        const values = [1, 2, 3];
        const result = values
            .filter((value: int): boolean => value > 1)
            .map((value: int): int => value * 10);
        console.log(values[0]);
        console.log(result[0]);
        "#,
    )
    .unwrap();
    let result = variable_initializer(&program, "result");
    let HirExprKind::ArrayMethod {
        method: ArrayMethodKind::Map,
        array,
        ..
    } = &result.kind
    else {
        panic!("map")
    };
    assert!(matches!(array.kind, HirExprKind::ArrayMethod { .. }));
    let HirExprKind::ArrayMethod {
        method: ArrayMethodKind::Filter,
        array,
        ..
    } = &array.kind
    else {
        panic!("filter")
    };
    assert!(matches!(
        array.kind,
        HirExprKind::Clone {
            mode: lift_sema::CloneMode::Value,
            ..
        }
    ));
    assert_eq!(result.ty, ResolvedType::Array(Box::new(ResolvedType::Int)));
}

#[test]
fn array_methods_reject_invalid_callbacks_and_receivers() {
    let cases = [
        ("const values = [1, 2]; values.foo();", "método"),
        (
            "const values = [1, 2]; values.map((value: string): int => 1);",
            "deve receber",
        ),
        (
            "const values = [1, 2]; values.filter((value: int): int => value);",
            "deve retornar",
        ),
        (
            "const values = [1, 2]; values.forEach((value: int): int => value);",
            "deve retornar",
        ),
        (
            "const values: int?[] = [null]; values.find((value: int?): boolean => value == null);",
            "Option aninhado",
        ),
        (
            r#"const values = [1, 2]; values.reduce("x", (accumulator: int, value: int): int => accumulator + value);"#,
            "reduce",
        ),
        (
            "const values = [1, 2]; values.reduce((accumulator: int, value: int): int => accumulator + value);",
            "espera 2",
        ),
        (
            "const values = [1, 2]; values.map((value: int): int => value, 1);",
            "espera 1",
        ),
        ("const values = [1, 2]; values.map(1);", "espera uma função"),
        ("const value = 1; value.map((item: int): int => item);", ""),
    ];
    for (source, message) in cases {
        let errors = lower(source).unwrap_err();
        if !message.is_empty() {
            assert!(
                errors.iter().any(|error| error.message.contains(message)),
                "expected `{message}` in {errors:?}"
            );
        }
    }
    assert!(lower("const values = [1, 2]; values.map((value: int): int => value);").is_ok());
}

#[test]
fn array_methods_accept_named_callbacks_and_captures() {
    let program = lower(
        r#"
        function double(value: int): int { return value * 2; }
        const factor = 3;
        const values = [1, 2];
        const scaled = values.map((value: int): int => value * factor);
        const doubled = values.map(double);
        "#,
    )
    .unwrap();
    let scaled = variable_initializer(&program, "scaled");
    let HirExprKind::ArrayMethod { callback, .. } = &scaled.kind else {
        panic!("map")
    };
    let HirExprKind::Closure { captures, .. } = &callback.kind else {
        panic!("closure")
    };
    assert_eq!(captures.len(), 1);
    let doubled = variable_initializer(&program, "doubled");
    let HirExprKind::ArrayMethod { callback, .. } = &doubled.kind else {
        panic!("map")
    };
    assert!(matches!(callback.kind, HirExprKind::FunctionValue { .. }));
}

#[test]
fn find_and_some_keep_option_and_boolean_results() {
    let errors = lower(
        r#"
        const values: int?[] = [null];
        const found = values.find((value: int?): boolean => value == null);
        "#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::UnsupportedType));
}
