//! Validation of explicit Rust import sources and supported FFI types.
//!
//! PT-BR: Validação de destinos de imports Rust explícitos e tipos FFI suportados.

use lift_ast::Span;

use crate::{ResolvedType, RustDependency, SemaError, SemaErrorKind};

/// The local DB adapter is a closed protocol, not arbitrary resource FFI.
pub(crate) fn resource_signature(
    path: &str,
) -> Option<(Vec<(ResolvedType, crate::ParameterPassMode)>, ResolvedType)> {
    use crate::{ParameterPassMode::*, ResourceKind::*};
    use ResolvedType::*;
    let connection = Resource(SqliteConnection);
    let transaction = Resource(SqliteTransaction);
    let pool = Resource(DbPool);
    let db_transaction = Resource(DbTransaction);
    let rows = Resource(DbRows);
    let values = Array(Box::new(DbValue));
    let result = |ty| Result(Box::new(ty), Box::new(String));
    let (parameters, output) = match path {
        "lift_db::api::create_pool" => (
            vec![
                (String, BorrowImm),
                (String, BorrowImm),
                (Int, ByValue),
                (Int, ByValue),
                (Int, ByValue),
            ],
            result(pool),
        ),
        "lift_db::api::pool_driver" => (vec![(pool, BorrowImm)], String),
        "lift_db::api::pool_size" | "lift_db::api::pool_idle" => (vec![(pool, BorrowImm)], Int),
        "lift_db::api::close_pool" => (vec![(pool, OwnedMove)], Void),
        "lift_db::api::close_rows" => (vec![(rows, OwnedMove)], Void),
        "lift_db::api::execute" => (
            vec![(pool, BorrowImm), (String, BorrowImm), (values, BorrowImm)],
            result(Int),
        ),
        "lift_db::api::query" => (
            vec![(pool, BorrowImm), (String, BorrowImm), (values, BorrowImm)],
            result(rows),
        ),
        "lift_db::api::begin" => (vec![(pool, BorrowImm)], result(db_transaction)),
        "lift_db::api::tx_execute" => (
            vec![
                (db_transaction, BorrowMut),
                (String, BorrowImm),
                (values, BorrowImm),
            ],
            result(Int),
        ),
        "lift_db::api::tx_query" => (
            vec![
                (db_transaction, BorrowMut),
                (String, BorrowImm),
                (values, BorrowImm),
            ],
            result(rows),
        ),
        "lift_db::api::commit" | "lift_db::api::rollback" => {
            (vec![(db_transaction, OwnedMove)], result(Void))
        }
        "lift_db::api::row_count" | "lift_db::api::column_count" => (vec![(rows, BorrowImm)], Int),
        "lift_db::api::column_name" => (vec![(rows, BorrowImm), (Int, ByValue)], result(String)),
        "lift_db::api::get_int" => (
            vec![(rows, BorrowImm), (Int, ByValue), (Int, ByValue)],
            result(Int),
        ),
        "lift_db::api::get_number" => (
            vec![(rows, BorrowImm), (Int, ByValue), (Int, ByValue)],
            result(Number),
        ),
        "lift_db::api::get_text" => (
            vec![(rows, BorrowImm), (Int, ByValue), (Int, ByValue)],
            result(String),
        ),
        "lift_db::api::get_bool" | "lift_db::api::is_null" => (
            vec![(rows, BorrowImm), (Int, ByValue), (Int, ByValue)],
            result(Boolean),
        ),
        "lift_db::api::value_null" => (vec![], DbValue),
        "lift_db::api::value_int" => (vec![(Int, ByValue)], DbValue),
        "lift_db::api::value_number" => (vec![(Number, ByValue)], DbValue),
        "lift_db::api::value_text" => (vec![(String, BorrowImm)], DbValue),
        "lift_db::api::value_bool" => (vec![(Boolean, ByValue)], DbValue),
        "lift_db::resource::open" => (vec![(String, BorrowImm)], result(connection)),
        "lift_db::resource::begin" => (vec![(connection, OwnedMove)], result(transaction)),
        "lift_db::resource::insert" => (
            vec![(transaction, BorrowMut), (String, BorrowImm)],
            result(Int),
        ),
        "lift_db::resource::list" => (vec![(connection, BorrowImm)], result(String)),
        "lift_db::resource::commit" | "lift_db::resource::rollback" => {
            (vec![(transaction, OwnedMove)], result(connection))
        }
        "lift_db::resource::close" => (vec![(connection, OwnedMove)], Void),
        _ => return None,
    };
    Some((parameters, output))
}

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
        "destino inválido: use `pacote@MAJOR.MINOR.PATCH::modulo::funcao` ou `lift_core`, `lift_db` ou `lift_web`", span)
    };
    let (package_spec, path) = source.split_once("::").ok_or_else(invalid)?;
    if !path.split("::").all(rust_identifier) {
        return Err(invalid());
    }
    if package_spec == "lift_core" {
        return Ok((RustDependency::LocalCore, format!("lift_core::{path}")));
    }
    if package_spec == "lift_db" {
        return Ok((RustDependency::LocalDb, format!("lift_db::{path}")));
    }
    if package_spec == "lift_web" {
        return Ok((RustDependency::LocalWeb, format!("lift_web::{path}")));
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
            "lift_core"
                | "lift_db"
                | "lift_web"
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
