//! Contextual API contracts independent of loader/filesystem behavior.
//! PT-BR: Contratos da API contextual sem depender de loader/filesystem.
use lift_ast::{Identifier, PackageAst, PackageBinding, PackageId, ProjectAst, SourceId, Span};
use lift_sema::{
    analyze_project, HirDeclaration, HirExprKind, HirStatement, ParameterPassMode, SemaErrorKind,
};
fn ast(text: &str, id: usize) -> lift_ast::ProgramAst {
    lift_parser::parse_with_source(
        &lift_lexer::tokenize_with_source(text, SourceId(id)).unwrap(),
        SourceId(id),
    )
    .unwrap()
}
fn graph(source: &str) -> ProjectAst {
    ProjectAst{root:PackageId(0),packages:vec![
    PackageAst{id:PackageId(0),is_library:false,program:ast(source,0),imports:vec![PackageBinding{alias:Identifier{name:"lib".to_owned(),span:Span::new(0,0)},target:PackageId(1)}]},
    PackageAst{id:PackageId(1),is_library:true,program:ast("function private():void {} export function bump(values:int[]):void {values[0] = 2;} export function snapshot(text:string):string {return text;}",1),imports:vec![]},
]}
}

#[test]
fn contextual_entry_resolves_import_markers_and_preserves_call_modes_and_liveness() {
    let hir=analyze_project(graph("import * as lib from \"lib\"; function forward(values:int[]):void {lib.bump(values);} const values=[1]; forward(values); console.log(values[0]); const text=\"old\"; const copy=lib.snapshot(text); console.log(text); console.log(copy);")).unwrap();
    let forward = hir
        .declarations
        .iter()
        .find_map(|d| match d {
            HirDeclaration::Function(f) if f.name.ends_with(":forward") => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        forward.parameters[0].pass_mode,
        ParameterPassMode::BorrowMut
    );
    let HirStatement::Expression(e) = &forward.body.statements[0] else {
        panic!()
    };
    let HirExprKind::Call {
        function,
        arguments,
    } = &e.expression.kind
    else {
        panic!()
    };
    assert_eq!(function, "$package:1:bump");
    assert_eq!(arguments[0].pass_mode, lift_sema::ArgPassMode::ByMutRef);
    assert!(hir
        .declarations
        .iter()
        .any(|d| matches!(d,HirDeclaration::Function(f) if f.name=="$package:1:private")));
}

#[test]
fn contextual_api_rejects_missing_targets_cycles_privacy_and_reserved_aliases() {
    let errors = analyze_project(graph("lib.private();")).unwrap_err();
    assert_eq!(errors[0].kind, SemaErrorKind::PrivatePackageMember);
    let mut missing = graph("");
    missing.packages.pop();
    assert_eq!(
        analyze_project(missing).unwrap_err()[0].kind,
        SemaErrorKind::InvalidPackage
    );
    let mut cycle = graph("");
    cycle.packages[1].imports.push(PackageBinding {
        alias: Identifier {
            name: "back".to_owned(),
            span: Span::new(0, 0),
        },
        target: PackageId(0),
    });
    assert_eq!(
        analyze_project(cycle).unwrap_err()[0].kind,
        SemaErrorKind::InvalidPackage
    );
    let mut reserved = graph("");
    reserved.packages[0].imports[0].alias.name = "console".to_owned();
    assert_eq!(
        analyze_project(reserved).unwrap_err()[0].kind,
        SemaErrorKind::InvalidPackage
    );
}
