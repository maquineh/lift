use truxt_ast::{Declaration, ExpressionKind, SourceId, Statement};
use truxt_sema::{analyze, HirDeclaration, HirExprKind, HirStatement, ParameterPassMode};

fn lower(source: &str) -> Result<truxt_sema::HirProgram, Vec<truxt_sema::SemaError>> {
    analyze(truxt_parser::parse(&truxt_lexer::tokenize(source).unwrap()).unwrap())
}

#[test]
fn methods_use_receiver_arguments_and_propagate_mutation() {
    let p = lower("interface X { n: int } impl X { bump(): int { this.n = this.n + 1; return this.n; } snapshot(): X { return this; } } function indirect(x: X): int { return x.bump(); } const x: X = { n: 1 }; indirect(x);").unwrap();
    let functions = p
        .declarations
        .iter()
        .filter_map(|d| {
            if let HirDeclaration::Function(f) = d {
                Some(f)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        functions
            .iter()
            .find(|f| f.name == "indirect")
            .unwrap()
            .parameters[0]
            .pass_mode,
        ParameterPassMode::BorrowMut
    );
    assert_eq!(
        functions
            .iter()
            .find(|f| f.name.ends_with(":snapshot"))
            .unwrap()
            .parameters[0]
            .pass_mode,
        ParameterPassMode::BorrowImm
    );
}

#[test]
fn closures_have_distinct_snapshot_bindings_and_explicit_clones() {
    let p =
        lower("const s = \"old\"; const f = (): string => s; console.log(s); console.log(f());")
            .unwrap();
    let HirDeclaration::Statement(HirStatement::Variable(f)) = &p.declarations[1] else {
        panic!()
    };
    let HirExprKind::Closure { captures, .. } = &f.initializer.kind else {
        panic!()
    };
    assert_eq!(captures.len(), 1);
    let HirExprKind::Clone { expression, .. } = &captures[0].1.kind else {
        panic!()
    };
    let HirExprKind::Local { id, .. } = expression.kind else {
        panic!()
    };
    assert_ne!(captures[0].0, id);
}

#[test]
fn nested_parameters_are_bound_and_owned_parameters_forward_mutation() {
    lower("const x = (): int => 1; const outer = (): int => { const inner = (x: int): int => x; return inner(2); }; console.log(outer());").unwrap();
    lower("interface X { n: int } impl X { bump(): int { this.n = this.n + 1; return this.n; } } const f = (x: X): int => x.bump(); const x: X = { n: 1 }; console.log(f(x));").unwrap();
    let p = lower("interface A { n: int } interface B { n: int } impl A { read(): int { return this.n; } } impl B { read(): int { this.n = 1; return this.n; } } function inspect(a: A): int { return a.read(); }").unwrap();
    let function = p
        .declarations
        .iter()
        .find_map(|d| {
            if let HirDeclaration::Function(f) = d {
                if f.name == "inspect" {
                    Some(f)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        function.parameters[0].pass_mode,
        ParameterPassMode::BorrowImm
    );
}

#[test]
fn rejects_unsupported_closure_and_template_ownership_patterns() {
    for source in [
        "let f = (): int => 1;",
        "const f = (): int => 1; const copy = f;",
        "const f = (): int => 1; const xs = [f];",
        "const f = (): int => 1; const x = option.some(f);",
        "const f = (): int => 1; const eq = f == f;",
        "let n = 1; const f = (): int => { n = 2; return n; };",
        "interface X { n: int } const x: X = { n: 1 }; const f = (): int => { x.n = 2; return x.n; };",
        "interface X { n: int } const x: X = { n: 1 }; const s = `${x}`;",
        "impl Missing { f(): int { return 1; } }",
        "interface X { n: int } impl X { f(this: int): int { return 1; } }",
        "interface X { n: int } impl X { f(): int { return 1; } f(): int { return 2; } }",
        "interface X { n: int } impl X { change(other: X): void { this.n = 2; other.n = 3; } } const x: X = { n: 1 }; x.change(x);",
        "interface X { n: int } impl X { bump(): int { this.n = 2; return this.n; } } const x: X = { n: 1 }; const f = (): int => x.bump();",
    ] { assert!(lower(source).is_err(), "accepted {source}"); }
}

#[test]
fn nested_template_spans_preserve_utf8_and_source_identity() {
    let source = "const s = `á ${`🙂 ${missing}`}`;";
    let tokens = truxt_lexer::tokenize_with_source(source, SourceId(9)).unwrap();
    let ast = truxt_parser::parse_with_source(&tokens, SourceId(12)).unwrap();
    let errors = analyze(ast).unwrap_err();
    let span = errors[0].span;
    assert_eq!(span.source, SourceId(12));
    assert_eq!(&source[span.start..span.end], "missing");
    for source in ["`open", "`${1`", "`${\"}\"`", "`${/* comment */"] {
        assert!(truxt_lexer::tokenize(source).is_err());
    }
}

#[test]
fn interpolation_handles_objects_comments_and_multiline_text() {
    let source = "const s = `line\n${identity({n: 1}).n /* } */}`;";
    let ast = truxt_parser::parse(&truxt_lexer::tokenize(source).unwrap()).unwrap();
    let Declaration::Statement(Statement::Variable(v)) = &ast.declarations[0] else {
        panic!()
    };
    assert!(matches!(v.initializer.kind, ExpressionKind::Template(_)));
}
