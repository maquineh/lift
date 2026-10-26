//! Validation of explicit Rust import sources and supported FFI types.
//!
//! PT-BR: Validação de destinos de imports Rust explícitos e tipos FFI suportados.

use truxt_ast::Span;

use crate::{ResolvedType, RustDependency, SemaError, SemaErrorKind};

pub(crate) fn validate_type(
    ty: &ResolvedType,
    returning: bool,
    span: Span,
) -> Result<(), SemaError> {
    if contains_optional(ty) {
        return Err(SemaError::new(
            SemaErrorKind::UnsupportedType,
            "Option/T? não é suportado no FFI",
            span,
        ));
    }
    let primitive = |ty: &ResolvedType| {
        matches!(
            ty,
            ResolvedType::Int | ResolvedType::Number | ResolvedType::Boolean | ResolvedType::String
        )
    };
    if let ResolvedType::Result(ok, error) = ty {
        if returning && !matches!(**ok, ResolvedType::Result(_, _)) {
            validate_type(ok, true, span)?;
            validate_type(error, false, span)?;
            return Ok(());
        }
    }
    if primitive(ty)
        || matches!(ty, ResolvedType::Array(element) if primitive(element))
        || (returning && *ty == ResolvedType::Void)
    {
        Ok(())
    } else {
        Err(SemaError::new(
            SemaErrorKind::UnsupportedType,
            "FFI aceita primitivos, arrays de primitivos, retorno void e retorno Result<T, E>",
            span,
        ))
    }
}

fn contains_optional(ty: &ResolvedType) -> bool {
    match ty {
        ResolvedType::Optional(_) => true,
        ResolvedType::Array(element) => contains_optional(element),
        ResolvedType::Result(ok, error) => contains_optional(ok) || contains_optional(error),
        _ => false,
    }
}

pub(crate) fn resolve_source(
    source: &str,
    span: Span,
) -> Result<(RustDependency, String), SemaError> {
    let invalid = || {
        SemaError::new(SemaErrorKind::InvalidImport,
        "destino inválido: use `pacote@MAJOR.MINOR.PATCH::modulo::funcao` ou `truxt_core::modulo::funcao`", span)
    };
    let (package_spec, path) = source.split_once("::").ok_or_else(invalid)?;
    if !path.split("::").all(rust_identifier) {
        return Err(invalid());
    }
    if package_spec == "truxt_core" {
        return Ok((RustDependency::LocalCore, format!("truxt_core::{path}")));
    }
    let (package, version) = package_spec.split_once('@').ok_or_else(invalid)?;
    let crate_name = package.replace('-', "_");
    // A conservative Cargo-compatible ASCII subset. No arbitrary TOML or Rust text.
    if package.is_empty()
        || package.len() > 64
        || !package.as_bytes()[0].is_ascii_alphabetic()
        || !package
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !rust_identifier(&crate_name)
        || matches!(
            crate_name.as_str(),
            "truxt_core"
                | "std"
                | "core"
                | "alloc"
                | "test"
                | "proc_macro"
                | "build"
                | "deps"
                | "examples"
                | "incremental"
        )
    {
        return Err(invalid());
    }
    let components = version.split('.').collect::<Vec<_>>();
    if components.len() != 3
        || components.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
                || part.parse::<u64>().is_err()
        })
    {
        return Err(invalid());
    }
    Ok((
        RustDependency::Registry {
            package: package.to_owned(),
            version: version.to_owned(),
        },
        format!("{crate_name}::{path}"),
    ))
}

fn rust_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !matches!(
            name,
            "_" | "as"
                | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "abstract"
                | "become"
                | "box"
                | "do"
                | "final"
                | "macro"
                | "override"
                | "priv"
                | "try"
                | "typeof"
                | "unsized"
                | "virtual"
                | "yield"
                | "gen"
        )
}
