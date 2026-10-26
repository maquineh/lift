//! Semantic contracts for types, imports, calls, and ownership diagnostics.
//!
//! PT-BR: Contratos semânticos de tipos, imports, chamadas e diagnósticos de ownership.

use lift_ast::BinaryOperator;
use lift_lexer::tokenize;
use lift_parser::parse;
use lift_sema::{
    analyze, ArgPassMode, HirDeclaration, HirExprKind, HirStatement, ParameterPassMode,
    ResolvedType, SemaErrorKind,
};

fn analyze_source(source: &str) -> Result<lift_sema::HirProgram, Vec<lift_sema::SemaError>> {
    let tokens = tokenize(source).expect("fonte de teste deve tokenizar");
    let ast = parse(&tokens).expect("fonte de teste deve parsear");
    analyze(ast)
}

#[test]
fn unresolved_source_imports_are_rejected_before_hir() {
    let errors = analyze_source("import module \"module.lift\";").unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::InvalidImport));
}

#[test]
fn result_values_have_typed_borrowed_inspection_and_owned_payloads() {
    let hir = analyze_source(
        r#"
        import function read(): Result<string, string> from "sample@1.0.0::read";
        import function write(): Result<void, string> from "sample@1.0.0::write";
        function inspect(value: Result<string, string>): string {
            if result.is_ok(value) { return result.unwrap(value); }
            return result.unwrap_err(value);
        }
        const value: Result<string, string> = read();
        console.log(inspect(value));
        result.unwrap(write());
    "#,
    )
    .unwrap();
    let function = hir
        .declarations
        .iter()
        .find_map(|decl| match decl {
            HirDeclaration::Function(function) => Some(function),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        function.parameters[0].pass_mode,
        ParameterPassMode::BorrowImm
    );
    assert_eq!(function.return_type, ResolvedType::String);
    assert_eq!(
        function.parameters[0].ty,
        ResolvedType::Result(
            Box::new(ResolvedType::String),
            Box::new(ResolvedType::String)
        )
    );
}

#[test]
fn result_rejects_invalid_payloads_ffi_parameters_and_direct_struct_cycles() {
    for source in [
        "function f(value: Result<string, void>): void {}",
        "function f(value: Result<void[], string>): void {}",
        r#"import function f(value: Result<int, string>): void from "sample@1.0.0::f";"#,
        r#"import function f(): Result<Result<int, string>, string> from "sample@1.0.0::f";"#,
        "result.is_ok(1);",
        "result.unwrap();",
        "const result = 1; result.unwrap(1);",
    ] {
        assert!(analyze_source(source).is_err(), "{source}");
    }
    let errors = analyze_source("interface Node { next: Result<Node, string> }").unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::RecursiveValueType));
    analyze_source("interface Node { next: Result<Node[], string> }").unwrap();
}

#[test]
fn imports_are_predeclared_and_record_explicit_borrow_modes() {
    let hir = analyze_source(r#"
        concat("a", "b");
        import function concat(a: string, b: string): string from "lift_core::string::concat";
        import function sum(values: int[], flag: boolean, count: int, scale: number): int from "sample-package@1.2.3::sum";
        const values = [1, 2]; sum(values, true, 2, 1.0);
    "#).expect("imports devem ser resolvidos");
    let HirDeclaration::Import(import) = &hir.declarations[2] else {
        panic!("esperava import")
    };
    assert_eq!(import.rust_path, "sample_package::sum");
    assert_eq!(
        import.dependency,
        lift_sema::RustDependency::Registry {
            package: "sample-package".into(),
            version: "1.2.3".into()
        }
    );
    let HirDeclaration::Statement(HirStatement::Expression(call)) = &hir.declarations[4] else {
        panic!("esperava chamada")
    };
    let HirExprKind::Call { arguments, .. } = &call.expression.kind else {
        panic!("esperava chamada")
    };
    assert_eq!(
        arguments
            .iter()
            .map(|arg| arg.pass_mode)
            .collect::<Vec<_>>(),
        vec![
            ArgPassMode::ByRef,
            ArgPassMode::ByValue,
            ArgPassMode::ByValue,
            ArgPassMode::ByValue
        ]
    );
}

#[test]
fn validates_console_log_types_arity_and_shadowing() {
    let hir = analyze_source(
        r#"console.log(1); console.log(1.5); console.log(true); console.log("ok");"#,
    )
    .unwrap();
    for declaration in hir.declarations {
        let HirDeclaration::Statement(HirStatement::Expression(statement)) = declaration else {
            panic!("esperava expressão")
        };
        assert_eq!(statement.expression.ty, ResolvedType::Void);
        let HirExprKind::Call {
            function,
            arguments,
        } = statement.expression.kind
        else {
            panic!("esperava call")
        };
        assert_eq!(function, lift_sema::CONSOLE_LOG_BUILTIN);
        assert_eq!(arguments[0].pass_mode, ArgPassMode::ByRef);
    }
    for source in [
        "console.log();",
        "console.log(1, 2);",
        "console.log([1]);",
        "const x = console.log(1);",
        "const console = 1; console.log(1);",
        "console.error(1);",
    ] {
        assert!(analyze_source(source).is_err(), "deve rejeitar: {source}");
    }
}

#[test]
fn rejects_invalid_or_conflicting_rust_imports() {
    for target in [
        "sample::f",
        "sample@^1.2.3::f",
        "sample@1.2::f",
        "sample@01.2.3::f",
        "sample@1.2.3-beta::f",
        "sample@1.2.3::",
        "sample@1.2.3::m::::f",
        "sample@1.2.3::fn",
        "sample@1.2.3::f()",
        "sample@1.2.3::ação",
        "sample@1.2.3::r#type",
        "lift_core@1.2.3::f",
        "std@1.2.3::f",
        "../sample@1.2.3::f",
        "sample@1.2.3::f\\n",
        "sample@1.2.18446744073709551616::f",
    ] {
        let source = format!("import function f(): int from \"{target}\";");
        let errors = analyze_source(&source).expect_err("destino deve ser inválido");
        assert!(
            errors
                .iter()
                .any(|error| error.kind == SemaErrorKind::InvalidImport),
            "{target}: {errors:?}"
        );
    }
    for source in [
        r#"import function f(): int from "sample@1.2.3::f"; import function g(): int from "sample@1.2.4::g";"#,
        r#"import function f(): int from "sample-name@1.2.3::f"; import function g(): int from "sample_name@1.2.3::g";"#,
        r#"function f(): int { return 1; } import function f(): int from "sample@1.2.3::f";"#,
        r#"import function f(): int from "sample@1.2.3::f"; interface f { x: int }"#,
        r#"import function f(a: int, a: int): int from "sample@1.2.3::f";"#,
        r#"import function f(a: void): int from "sample@1.2.3::f";"#,
        r#"import function f(a: int[][]): int from "sample@1.2.3::f";"#,
        r#"interface User { n: int } import function f(a: User): int from "sample@1.2.3::f";"#,
        r#"interface User { n: int } import function f(): User from "sample@1.2.3::f";"#,
        r#"import function f(a: int): int from "sample@1.2.3::f"; f("a");"#,
        r#"import function f(a: int): int from "sample@1.2.3::f"; f();"#,
    ] {
        assert!(analyze_source(source).is_err(), "deve rejeitar: {source}");
    }
}

#[test]
fn resolves_recursive_calls_and_types_every_expression() {
    let program = analyze_source(
        "function fibonacci(n: int): int { if n <= 1 { return n; } return fibonacci(n - 1) + fibonacci(n - 2); }",
    )
    .expect("recursão com tipos explícitos deve ser válida");

    let HirDeclaration::Function(function) = &program.declarations[0] else {
        panic!("esperava função no HIR");
    };
    assert_eq!(function.name, "fibonacci");
    assert_eq!(function.parameters[0].pass_mode, ParameterPassMode::ByValue);
    assert_eq!(function.return_type, ResolvedType::Int);
    let HirStatement::If(conditional) = &function.body.statements[0] else {
        panic!("esperava condicional");
    };
    assert_eq!(conditional.condition.ty, ResolvedType::Boolean);
    let HirStatement::Return(return_statement) = &function.body.statements[1] else {
        panic!("esperava retorno recursivo");
    };
    let Some(value) = &return_statement.value else {
        panic!("retorno deve ter valor");
    };
    assert_eq!(value.ty, ResolvedType::Int);
    let HirExprKind::Binary {
        operator,
        left,
        right,
    } = &value.kind
    else {
        panic!("retorno deve conter a soma dos resultados");
    };
    assert_eq!(*operator, BinaryOperator::Add);
    assert_eq!(left.ty, ResolvedType::Int);
    assert_eq!(right.ty, ResolvedType::Int);
    assert!(matches!(left.kind, HirExprKind::Call { .. }));
    assert!(matches!(right.kind, HirExprKind::Call { .. }));
}

#[test]
fn records_mutable_borrow_modes_in_signatures_and_calls() {
    let program = analyze_source(
        "interface Counter { value: int } function increment(counter: Counter): void { counter.value = counter.value + 1; } let counter: Counter = { value: 0 }; increment(counter);",
    )
    .expect("mutação de campo deve ser analisada");

    let HirDeclaration::Function(function) = &program.declarations[1] else {
        panic!("esperava função increment");
    };
    assert_eq!(
        function.parameters[0].pass_mode,
        ParameterPassMode::BorrowMut
    );
    let HirDeclaration::Statement(HirStatement::Expression(statement)) = &program.declarations[3]
    else {
        panic!("esperava chamada no script de topo");
    };
    let HirExprKind::Call { arguments, .. } = &statement.expression.kind else {
        panic!("esperava expressão de chamada");
    };
    assert_eq!(arguments[0].pass_mode, ArgPassMode::ByMutRef);
}

#[test]
fn rejects_two_mutable_borrows_of_the_same_root_in_one_call() {
    let errors = analyze_source(
        "interface Counter { value: int } function increment_both(first: Counter, second: Counter): void { first.value = first.value + 1; second.value = second.value + 1; } let counter: Counter = { value: 0 }; increment_both(counter, counter);",
    )
    .expect_err("dois empréstimos mutáveis da mesma raiz devem ser rejeitados");

    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::BorrowConflict));
}

#[test]
fn accepts_i64_minimum_and_rejects_positive_magnitude_out_of_range() {
    let program = analyze_source("const minimum = -9223372036854775808;")
        .expect("i64::MIN deve ser aceito no contexto unário");
    let HirDeclaration::Statement(HirStatement::Variable(variable)) = &program.declarations[0]
    else {
        panic!("esperava variável");
    };
    assert_eq!(
        variable.initializer.kind,
        HirExprKind::IntegerLiteral(i64::MIN)
    );

    let errors = analyze_source("const too_large = 9223372036854775808;")
        .expect_err("magnitude positiva acima de i64 deve ser rejeitada");
    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::InvalidIntegerLiteral));
}

#[test]
fn reports_type_mismatches_without_returning_partial_hir() {
    let errors = analyze_source("const count: int = true;")
        .expect_err("boolean não deve satisfazer uma anotação int");
    assert!(errors
        .iter()
        .any(|error| error.kind == SemaErrorKind::TypeMismatch));
}
