//! Acceptance and rejection contracts for the supported parser grammar.
//!
//! PT-BR: Contratos de aceite e rejeição da gramática suportada pelo parser.

use lift_ast::{
    BinaryOperator, Declaration, ExpressionKind, SourceId, Statement, TypeExpressionKind,
};
use lift_lexer::tokenize;
use lift_parser::{parse, ParseErrorKind};

fn parse_source(source: &str) -> Result<lift_ast::ProgramAst, Vec<lift_parser::ParseError>> {
    let tokens = tokenize(source).expect("fonte de teste deve tokenizar");
    parse(&tokens)
}

#[test]
fn namespace_imports_and_export_functions_preserve_source_spans_and_contextual_identifiers() {
    let source="import * as math from \"math\"; export function add(a:int,b:int):int {return a+b;} const export = 2; const as = 3;";
    let tokens = lift_lexer::tokenize_with_source(source, SourceId(9)).unwrap();
    let ast = lift_parser::parse_with_source(&tokens, SourceId(9)).unwrap();
    let Declaration::PackageImport(import) = &ast.declarations[0] else {
        panic!()
    };
    assert_eq!(import.alias.name, "math");
    assert_eq!(import.dependency, "math");
    assert_eq!(import.span.source, SourceId(9));
    assert_eq!(
        &source[import.dependency_span.start..import.dependency_span.end],
        "\"math\""
    );
    assert!(matches!(
        ast.declarations[1],
        Declaration::ExportFunction(_)
    ));
    for rejected in [
        "function f():void {import * as math from \"math\";}",
        "export interface Public {value:int}",
        "export const x = 1;",
        "import {add} from \"math\";",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn optional_array_suffixes_and_nullish_associativity_are_explicit() {
    let ast = parse_source(
        "function f(elements: int?[], array: int[]?): void {} const value = a ?? b ?? c;",
    )
    .unwrap();
    let Declaration::Function(function) = &ast.declarations[0] else {
        panic!("function")
    };
    let TypeExpressionKind::Array(element) = &function.parameters[0].ty.kind else {
        panic!("array")
    };
    assert!(matches!(element.kind, TypeExpressionKind::Optional(_)));
    let TypeExpressionKind::Optional(array) = &function.parameters[1].ty.kind else {
        panic!("optional")
    };
    assert!(matches!(array.kind, TypeExpressionKind::Array(_)));
    let Declaration::Statement(Statement::Variable(variable)) = &ast.declarations[1] else {
        panic!("variable")
    };
    let ExpressionKind::Binary(binary) = &variable.initializer.kind else {
        panic!("binary")
    };
    assert_eq!(binary.operator, BinaryOperator::Coalesce);
    assert!(
        matches!(&binary.left.kind, ExpressionKind::Binary(left) if left.operator == BinaryOperator::Coalesce)
    );
    assert!(parse_source("const value: int?? = null;").is_err());
    assert!(parse_source("interface User { name?: string }").is_err());
}

#[test]
fn logical_and_coalesce_mixing_requires_parentheses() {
    for source in [
        "a ?? b || c;",
        "a || b ?? c;",
        "a ?? b && c;",
        "a && b ?? c;",
    ] {
        assert!(parse_source(source).is_err(), "{source}");
    }
    for source in [
        "(a ?? b) || c;",
        "a ?? (b || c);",
        "(a && b) ?? c;",
        "a && (b ?? c);",
    ] {
        parse_source(source).unwrap();
    }
}

#[test]
fn parses_module_imports_only_at_top_level_with_source_identity() {
    let source = "import module \"../módulo.lift\"; function module(): int { return 1; }";
    let id = SourceId(7);
    let tokens = lift_lexer::tokenize_with_source(source, id).unwrap();
    let ast = parse(&tokens).unwrap();
    assert_eq!(ast.span.source, id);
    let Declaration::ModuleImport(import) = &ast.declarations[0] else {
        panic!("module import")
    };
    assert_eq!(import.path, "../módulo.lift");
    assert_eq!(import.span.source, id);
    assert_eq!(import.path_span.source, id);
    assert_eq!(&source[import.path_span.as_range()], "\"../módulo.lift\"");
    let Declaration::Function(function) = &ast.declarations[1] else {
        panic!("function")
    };
    assert_eq!(function.name.name, "module");
    assert_eq!(function.span.source, id);
    assert_eq!(function.body.span.source, id);
    assert_eq!(function.return_type.as_ref().unwrap().span.source, id);
    let Statement::Return(statement) = &function.body.statements[0] else {
        panic!("return")
    };
    assert_eq!(statement.span.source, id);
    assert_eq!(statement.value.as_ref().unwrap().span.source, id);
    for invalid in [
        "import module file;",
        "import module \"file.lift\"",
        "import module \"file.lift\" as alias;",
        "function f(): void { import module \"file.lift\"; }",
    ] {
        assert!(parse_source(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn source_aware_parser_preserves_empty_program_and_eof_error_ids() {
    let id = SourceId(9);
    assert_eq!(
        lift_parser::parse_with_source(&[], id).unwrap().span.source,
        id
    );
    let tokens = lift_lexer::tokenize_with_source("function f(): void {", id).unwrap();
    let errors = lift_parser::parse_with_source(&tokens, id).unwrap_err();
    assert!(errors.iter().all(|error| error.span.source == id));
    assert!(errors.iter().any(|error| error.span.is_empty()));
}

#[test]
fn parses_result_types_with_array_and_nested_arguments() {
    let source = r#"import function read(): Result<Result<string[], string>, string> from "sample@1.0.0::read";"#;
    let program = parse_source(source).unwrap();
    let Declaration::Import(import) = &program.declarations[0] else {
        panic!("import")
    };
    let TypeExpressionKind::Result(ok, _) = &import.return_type.kind else {
        panic!("Result externo")
    };
    let TypeExpressionKind::Result(ok, _) = &ok.kind else {
        panic!("Result interno")
    };
    assert!(matches!(ok.kind, TypeExpressionKind::Array(_)));
    assert_eq!(
        &source[import.return_type.span.start..import.return_type.span.end],
        "Result<Result<string[], string>, string>"
    );
}

#[test]
fn explicit_parser_source_normalizes_legacy_token_spans() {
    let id = SourceId(12);
    let tokens =
        tokenize("import module \"dep.lift\"; function f(value: string): void {}").unwrap();
    let ast = lift_parser::parse_with_source(&tokens, id).unwrap();
    let Declaration::ModuleImport(import) = &ast.declarations[0] else {
        panic!("module")
    };
    assert_eq!(import.path_span.source, id);
    assert_eq!(import.span.source, id);
    let Declaration::Function(function) = &ast.declarations[1] else {
        panic!("function")
    };
    assert_eq!(function.name.span.source, id);
    assert_eq!(function.parameters[0].name.span.source, id);
    assert_eq!(function.parameters[0].ty.span.source, id);
    let tokens = tokenize("function unfinished(").unwrap();
    let errors = lift_parser::parse_with_source(&tokens, id).unwrap_err();
    assert!(errors.iter().all(|error| error.span.source == id));
}

#[test]
fn result_requires_exactly_two_type_arguments() {
    for ty in [
        "Result",
        "Result<>",
        "Result<int>",
        "Result<int, string, boolean>",
        "Box<int>",
    ] {
        assert!(
            parse_source(&format!("function f(value: {ty}): void {{}}")).is_err(),
            "{ty}"
        );
    }
}

#[test]
fn parses_explicit_rust_import_signature_and_source_span() {
    let source =
        "import function concat(a: string, b: string,): string from \"lift_core::string::concat\";";
    let program = parse_source(source).expect("import explícito válido");
    let Declaration::Import(import) = &program.declarations[0] else {
        panic!("esperava import")
    };
    assert_eq!(import.name.name, "concat");
    assert_eq!(import.parameters.len(), 2);
    assert_eq!(import.source, "lift_core::string::concat");
    assert_eq!(
        &source[import.source_span.start..import.source_span.end],
        "\"lift_core::string::concat\""
    );
    assert_eq!(import.span.end, source.len());
}

#[test]
fn rejects_imports_without_explicit_signature_source_or_terminator() {
    for source in [
        "import function f(a): int from \"crate@1.2.3::f\";",
        "import function f() from \"crate@1.2.3::f\";",
        "import function f(): int from value;",
        "import function f(): int from \"crate@1.2.3::f\"",
        "function f() { import function g(): int from \"crate@1.2.3::g\"; }",
    ] {
        assert!(parse_source(source).is_err(), "deve rejeitar: {source}");
    }
}

#[test]
fn pratt_parser_respects_precedence_and_left_associativity() {
    let program = parse_source("const value = 1 + 2 * 3 - 4;").expect("programa válido");
    let Declaration::Statement(Statement::Variable(variable)) = &program.declarations[0] else {
        panic!("esperava uma declaração de variável");
    };

    let ExpressionKind::Binary(subtract) = &variable.initializer.kind else {
        panic!("o operador de menor precedência deve ser a raiz");
    };
    assert_eq!(subtract.operator, BinaryOperator::Subtract);
    let ExpressionKind::Binary(add) = &subtract.left.kind else {
        panic!("adição deve ficar à esquerda da subtração");
    };
    assert_eq!(add.operator, BinaryOperator::Add);
    let ExpressionKind::Binary(multiply) = &add.right.kind else {
        panic!("multiplicação deve agrupar antes da adição");
    };
    assert_eq!(multiply.operator, BinaryOperator::Multiply);

    let left_associative = parse_source("const value = 8 - 3 - 1;").expect("expressão válida");
    let Declaration::Statement(Statement::Variable(variable)) = &left_associative.declarations[0]
    else {
        panic!("esperava uma declaração de variável");
    };
    let ExpressionKind::Binary(outer) = &variable.initializer.kind else {
        panic!("esperava expressão binária");
    };
    assert_eq!(outer.operator, BinaryOperator::Subtract);
    assert!(matches!(outer.left.kind, ExpressionKind::Binary(_)));
}

#[test]
fn parses_interfaces_arrays_and_chained_postfix_expressions() {
    let source =
        "interface Box { value: int } function first(boxes: Box[]): int { return boxes[0].value; }";
    let program = parse_source(source).expect("declarações e postfixos válidos");

    assert_eq!(program.declarations.len(), 2);
    let Declaration::Function(function) = &program.declarations[1] else {
        panic!("esperava declaração de função");
    };
    let Statement::Return(return_statement) = &function.body.statements[0] else {
        panic!("esperava return");
    };
    let Some(value) = &return_statement.value else {
        panic!("return deve conter uma expressão");
    };
    let ExpressionKind::FieldAccess(field) = &value.kind else {
        panic!("acesso ao campo deve ser o postfixo externo");
    };
    assert_eq!(field.field.name, "value");
    assert!(matches!(field.object.kind, ExpressionKind::Index(_)));
}

#[test]
fn newlines_do_not_insert_semicolons_after_return_or_inside_expressions() {
    let program = parse_source("function f(): int { return\n1\n+ 2 }")
        .expect("newlines devem ser ignorados entre tokens");
    let Declaration::Function(function) = &program.declarations[0] else {
        panic!("esperava declaração de função");
    };
    assert_eq!(function.body.statements.len(), 1);
    let Statement::Return(return_statement) = &function.body.statements[0] else {
        panic!("esperava return");
    };
    let value = return_statement.value.as_ref().expect("return com valor");
    let ExpressionKind::Binary(add) = &value.kind else {
        panic!("a expressão deve continuar depois do newline");
    };
    assert_eq!(add.operator, BinaryOperator::Add);
    assert!(matches!(add.left.kind, ExpressionKind::IntegerLiteral(1)));
    assert!(matches!(add.right.kind, ExpressionKind::IntegerLiteral(2)));

    let program = parse_source("function f() { return;\n1 + 2 }")
        .expect("semicolon explícito deve encerrar o return");
    let Declaration::Function(function) = &program.declarations[0] else {
        panic!("esperava declaração de função");
    };
    assert_eq!(function.body.statements.len(), 2);
    let Statement::Return(return_statement) = &function.body.statements[0] else {
        panic!("esperava return");
    };
    assert!(return_statement.value.is_none());
    let Statement::Expression(statement) = &function.body.statements[1] else {
        panic!("a expressão depois do semicolon deve ser outro statement");
    };
    let ExpressionKind::Binary(add) = &statement.expression.kind else {
        panic!("o statement seguinte deve conter a expressão 1 + 2");
    };
    assert_eq!(add.operator, BinaryOperator::Add);
}

#[test]
fn recovers_multiple_statement_errors_and_reports_eof_span() {
    let errors = parse_source("const = 1; let = 2; const ok = 3;")
        .expect_err("duas declarações malformadas devem falhar");
    assert_eq!(
        errors.len(),
        2,
        "parser deve recuperar para o próximo statement"
    );
    assert_eq!(errors[0].kind, ParseErrorKind::UnexpectedToken);
    assert_eq!(errors[0].span, lift_ast::Span::new(6, 7));
    assert_eq!(errors[1].kind, ParseErrorKind::UnexpectedToken);
    assert_eq!(errors[1].span, lift_ast::Span::new(15, 16));

    let errors = parse_source("function f(").expect_err("entrada truncada deve falhar");
    assert_eq!(errors[0].kind, ParseErrorKind::UnexpectedEndOfInput);
    assert_eq!(errors[0].span, lift_ast::Span::new(11, 11));
}
