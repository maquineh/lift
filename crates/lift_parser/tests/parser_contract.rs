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
    let program = parse_source("interface User { name?: string, alias: string? }").unwrap();
    let Declaration::Interface(interface) = &program.declarations[0] else {
        panic!("interface")
    };
    assert!(interface.fields[0].optional);
    assert!(matches!(
        interface.fields[0].ty.kind,
        TypeExpressionKind::Named(_)
    ));
    assert!(!interface.fields[1].optional);
    assert!(matches!(
        interface.fields[1].ty.kind,
        TypeExpressionKind::Optional(_)
    ));
    for rejected in [
        "interface User { name? }",
        "interface User { name??: string }",
        "interface User { ?: string }",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
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
fn parses_enum_declarations_and_qualified_match_patterns() {
    let source = r#"
        enum Shape {
            Circle(number),
            Rect(number, number),
            Empty,
        }
        match shape {
            Shape.Circle(radius) => { console.log(radius); }
            Shape.Rect(width, height) => { console.log(width); }
            Shape.Empty => { console.log(0); }
        }
    "#;
    let program = parse_source(source).expect("enum válido");
    let Declaration::Enum(enumeration) = &program.declarations[0] else {
        panic!("enum")
    };
    assert_eq!(enumeration.name.name, "Shape");
    assert_eq!(enumeration.variants.len(), 3);
    assert_eq!(enumeration.variants[0].name.name, "Circle");
    assert_eq!(enumeration.variants[0].fields.len(), 1);
    assert_eq!(enumeration.variants[1].fields.len(), 2);
    assert!(enumeration.variants[2].fields.is_empty());
    let Declaration::Statement(Statement::Match(match_)) = &program.declarations[1] else {
        panic!("match")
    };
    let lift_ast::MatchPatternKind::EnumVariant {
        enum_name,
        variant,
        bindings,
    } = &match_.arms[0].pattern.kind
    else {
        panic!("enum pattern")
    };
    assert_eq!(enum_name.name, "Shape");
    assert_eq!(variant.name, "Circle");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].name, "radius");
    assert!(matches!(
        match_.arms[2].pattern.kind,
        lift_ast::MatchPatternKind::EnumVariant { .. }
    ));

    for rejected in [
        "enum { A }",
        "enum Shape { A( }",
        "enum Shape { A(number }",
        "enum Shape A }",
        "match value { Shape.Circle( => { } }",
        "match value { Shape.Circle(a b) => { } }",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
    // Vírgulas entre variantes são opcionais, inclusive a final.
    parse_source("enum Shape { A B(number,) }").unwrap();
}

#[test]
fn parses_generic_function_type_parameters() {
    let source = r#"
        function identity<T>(value: T): T { return value; }
        function pair<T, U>(first: T, second: U): T { return first; }
        const result = identity(1);
    "#;
    let program = parse_source(source).expect("funções genéricas");
    let Declaration::Function(identity) = &program.declarations[0] else {
        panic!("identity")
    };
    assert_eq!(identity.type_parameters.len(), 1);
    assert_eq!(identity.type_parameters[0].name, "T");
    let Declaration::Function(pair) = &program.declarations[1] else {
        panic!("pair")
    };
    assert_eq!(
        pair.type_parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        vec!["T", "U"]
    );
    for rejected in [
        "function f<>(value: int): int { return value; }",
        "function f<T(value: T): T { return value; }",
        "function f<T,>(value: T): T { return value; }",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
    parse_source("function f<T>(value: T): T { return value; }").unwrap();
}

#[test]
fn parses_union_type_declarations() {
    let source = r#"
        interface Circle { radius: number }
        interface Square { side: number }
        type Shape = Circle | Square;
        const shapes: Shape[] = [Shape.Circle({ radius: 2.0 })];
    "#;
    let program = parse_source(source).expect("união válida");
    let Declaration::Union(union) = &program.declarations[2] else {
        panic!("union")
    };
    assert_eq!(union.name.name, "Shape");
    assert_eq!(union.members.len(), 2);
    let lift_ast::TypeExpressionKind::Named(first) = &union.members[0].kind else {
        panic!("member")
    };
    assert_eq!(first.name, "Circle");
    let lift_ast::TypeExpressionKind::Named(second) = &union.members[1].kind else {
        panic!("member")
    };
    assert_eq!(second.name, "Square");
    parse_source("type Single = Circle;").unwrap();

    for rejected in [
        "type = Circle;",
        "type Shape = ;",
        "type Shape = Circle | ;",
        "type Shape = Circle",
        "type Shape = Circle[] | Square;",
        "type Shape = (int) => int | Circle;",
        "type Shape = Circle |",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn parses_enum_named_fields_and_object_payloads() {
    let source = r#"
        enum Shape {
            Circle { radius: number },
            Rect { width: number, height: number },
            Empty,
        }
        const value: Shape = Shape.Circle({ radius: 2.0 });
        match value {
            Shape.Circle(radius) => { console.log(radius); }
            Shape.Rect(width, height) => { console.log(width); }
            Shape.Empty => { console.log(0); }
        }
    "#;
    let program = parse_source(source).expect("enum com campos nomeados");
    let Declaration::Enum(enumeration) = &program.declarations[0] else {
        panic!("enum")
    };
    assert!(enumeration.variants[0].fields.is_empty());
    assert_eq!(enumeration.variants[0].named_fields.len(), 1);
    assert_eq!(enumeration.variants[0].named_fields[0].name.name, "radius");
    assert_eq!(enumeration.variants[1].named_fields.len(), 2);
    let Declaration::Statement(Statement::Variable(variable)) = &program.declarations[1] else {
        panic!("variable")
    };
    let ExpressionKind::Call(call) = &variable.initializer.kind else {
        panic!("constructor")
    };
    assert_eq!(call.arguments.len(), 1);
    let ExpressionKind::ObjectLiteral(object) = &call.arguments[0].kind else {
        panic!("object literal")
    };
    assert_eq!(object.fields[0].name.name, "radius");
    let Declaration::Statement(Statement::Match(match_)) = &program.declarations[2] else {
        panic!("match")
    };
    assert!(matches!(
        match_.arms[0].pattern.kind,
        lift_ast::MatchPatternKind::EnumVariant { .. }
    ));

    for rejected in [
        "enum Shape { Circle { radius number } }",
        "enum Shape { Circle { radius: } }",
        "enum Shape { Circle { radius: number }",
        "enum Shape { Circle { : number } }",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn parses_match_option_and_result_patterns() {
    let source = r#"
        match value {
            some(present) => { console.log(present); }
            none => { console.log(0); }
        }
        match result {
            ok(value) => { console.log(value); }
            err(error) => { console.log(error); }
        }
        match result {
            ok() => { console.log(0); }
            err(error) => { console.log(error); }
        }
    "#;
    let program = parse_source(source).expect("match válido");
    let Declaration::Statement(Statement::Match(first)) = &program.declarations[0] else {
        panic!("match")
    };
    assert_eq!(first.arms.len(), 2);
    let lift_ast::MatchPatternKind::Some(binding) = &first.arms[0].pattern.kind else {
        panic!("some")
    };
    assert_eq!(binding.name, "present");
    assert!(matches!(
        first.arms[1].pattern.kind,
        lift_ast::MatchPatternKind::None
    ));
    let Declaration::Statement(Statement::Match(second)) = &program.declarations[1] else {
        panic!("match result")
    };
    assert!(matches!(
        second.arms[0].pattern.kind,
        lift_ast::MatchPatternKind::Ok(Some(_))
    ));
    assert!(matches!(
        second.arms[1].pattern.kind,
        lift_ast::MatchPatternKind::Err(_)
    ));
    let Declaration::Statement(Statement::Match(third)) = &program.declarations[2] else {
        panic!("match void")
    };
    assert!(matches!(
        third.arms[0].pattern.kind,
        lift_ast::MatchPatternKind::Ok(None)
    ));

    for rejected in [
        "match value { some(x) => console.log(x); none => { console.log(0); } }",
        "match value { some(x) { console.log(x); } none => { console.log(0); } }",
        "match value { some(x => { console.log(x); } none => { console.log(0); } }",
        "match value some(x) => { console.log(x); }",
        "match value { } }",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn parses_for_of_and_loop_control() {
    let source = r#"
        for (const value of values) {
            if value == 0 { continue; }
            if value > 9 { break; }
            console.log(value);
        }
        for (let index of [1, 2]) {
            console.log(index);
        }
        while true { break; }
    "#;
    let program = parse_source(source).expect("laços válidos");
    let Declaration::Statement(Statement::ForOf(for_of)) = &program.declarations[0] else {
        panic!("for...of")
    };
    assert_eq!(for_of.binding, lift_ast::BindingKind::Const);
    assert_eq!(for_of.name.name, "value");
    assert!(matches!(
        for_of.iterable.kind,
        ExpressionKind::Identifier(_)
    ));
    let branches = for_of
        .body
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::If(if_) => Some(&if_.then_branch),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(branches
        .iter()
        .any(|branch| matches!(branch.statements[0], Statement::Continue(_))));
    assert!(branches
        .iter()
        .any(|branch| matches!(branch.statements[0], Statement::Break(_))));
    let Declaration::Statement(Statement::ForOf(for_of)) = &program.declarations[1] else {
        panic!("for...of let")
    };
    assert_eq!(for_of.binding, lift_ast::BindingKind::Let);
    assert_eq!(for_of.name.name, "index");
    let Declaration::Statement(Statement::While(while_)) = &program.declarations[2] else {
        panic!("while")
    };
    assert!(matches!(while_.body.statements[0], Statement::Break(_)));

    for rejected in [
        "for (value of [1]) { console.log(value); }",
        "for (const value in [1]) { console.log(value); }",
        "for (const value of [1] { console.log(value); }",
        "for (const value of [1]) { break }",
        "while true { continue }",
        "break",
        "continue",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn parses_function_types_and_higher_order_annotations() {
    let source = r#"
        function apply(value: int, transform: (int) => int): (int) => int {
            return transform;
        }
        function call(callback: ((int) => int) => int): int {
            return callback((value: int): int => value);
        }
    "#;
    let program = parse_source(source).expect("tipos de função válidos");
    let Declaration::Function(apply) = &program.declarations[0] else {
        panic!("function")
    };
    let TypeExpressionKind::Function(parameters, result) = &apply.parameters[1].ty.kind else {
        panic!("parâmetro de função")
    };
    assert_eq!(parameters.len(), 1);
    assert!(matches!(parameters[0].kind, TypeExpressionKind::Named(_)));
    assert!(matches!(result.kind, TypeExpressionKind::Named(_)));
    let TypeExpressionKind::Function(_, _) = &apply.return_type.as_ref().unwrap().kind else {
        panic!("retorno de função")
    };
    let Declaration::Function(call) = &program.declarations[1] else {
        panic!("function")
    };
    let TypeExpressionKind::Function(parameters, _) = &call.parameters[0].ty.kind else {
        panic!("callback de callback")
    };
    assert!(matches!(
        parameters[0].kind,
        TypeExpressionKind::Function(_, _)
    ));
    let closure = parse_source("const f = (callback: (int) => int): int => callback(1);").unwrap();
    let Declaration::Statement(Statement::Variable(variable)) = &closure.declarations[0] else {
        panic!("variable")
    };
    let ExpressionKind::Closure(closure) = &variable.initializer.kind else {
        panic!("closure")
    };
    assert!(matches!(
        closure.parameters[0].ty.kind,
        TypeExpressionKind::Function(_, _)
    ));

    for rejected in [
        "function f(callback: (int) => ): void {}",
        "function f(callback: (int): int): void {}",
        "function f(callback: (int => int)): void {}",
        "function f(callback: ((int) => int)[]): void {}",
        "function f(callback: (int) => int)?: void {}",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn parses_try_throw_and_postfix_try_operator() {
    let source = r#"
        function f(value: int): Result<int, string> {
            try {
                const parsed = g(value)?;
                throw "x";
            } catch (error: string) {
                return result.err(error);
            }
            return result.ok(0);
        }
    "#;
    let program = parse_source(source).expect("try/catch, throw e ? válidos");
    let Declaration::Function(function) = &program.declarations[0] else {
        panic!("function")
    };
    let Statement::TryCatch(statement) = &function.body.statements[0] else {
        panic!("try")
    };
    assert_eq!(statement.catch_name.name, "error");
    let Statement::Variable(variable) = &statement.body.statements[0] else {
        panic!("const do try")
    };
    assert!(matches!(
        variable.initializer.kind,
        ExpressionKind::TryPropagate(_)
    ));
    assert!(matches!(statement.body.statements[1], Statement::Throw(_)));
    let Statement::Return(_) = &statement.catch_body.statements[0] else {
        panic!("return no catch")
    };

    let program = parse_source("const value = f()? + 1;").unwrap();
    let Declaration::Statement(Statement::Variable(variable)) = &program.declarations[0] else {
        panic!("variable")
    };
    let ExpressionKind::Binary(binary) = &variable.initializer.kind else {
        panic!("binary")
    };
    assert_eq!(binary.operator, BinaryOperator::Add);
    assert!(matches!(binary.left.kind, ExpressionKind::TryPropagate(_)));

    for rejected in [
        "function f(): void { try { } }",
        "function f(): void { try { } catch (error) { } }",
        "function f(): void { try { } catch (error: int { } }",
        "function f(): void { catch (error: int) { } }",
        "function f(): void { throw 1 }",
        "function f(): void { throw; }",
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn parses_imported_interface_layouts_and_rejects_invalid_destinations() {
    let source = r#"import interface Range { start: int, note?: string } from "sample@1.0.0::geometry::Range";"#;
    let program = parse_source(source).expect("layout externo válido");
    let Declaration::ImportInterface(import) = &program.declarations[0] else {
        panic!("import interface")
    };
    assert_eq!(import.name.name, "Range");
    assert_eq!(import.fields.len(), 2);
    assert_eq!(import.fields[0].name.name, "start");
    assert!(!import.fields[0].optional);
    assert!(import.fields[1].optional);
    assert_eq!(import.source, "sample@1.0.0::geometry::Range");
    assert_eq!(
        &source[import.source_span.start..import.source_span.end],
        "\"sample@1.0.0::geometry::Range\""
    );
    assert_eq!(import.span.end, source.len());
    for rejected in [
        r#"import interface Range { start: int } "sample@1.0.0::Range";"#,
        r#"import interface Range { start: int } from ;"#,
        r#"import interface Range { start: int } from "sample@1.0.0::Range" as extra;"#,
        r#"import interface Range { start: int } from "sample@1.0.0::Range""#,
        r#"import interface Range from "sample@1.0.0::Range";"#,
        r#"import interface { start: int } from "sample@1.0.0::Range";"#,
    ] {
        assert!(parse_source(rejected).is_err(), "{rejected}");
    }
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
