//! Owned scalar bindings and materialized row cells. No driver lifetimes escape.
use bytes::BytesMut;
use postgres::types::{FromSql, IsNull, ToSql, Type};
use std::error::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum DbValue {
    Null,
    Int(i64),
    Number(f64),
    Text(String),
    Bool(bool),
}

impl rusqlite::ToSql for DbValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, ValueRef};
        Ok(ToSqlOutput::Borrowed(match self {
            Self::Null => ValueRef::Null,
            Self::Int(v) => ValueRef::Integer(*v),
            Self::Number(v) => ValueRef::Real(*v),
            Self::Text(v) => ValueRef::Text(v.as_bytes()),
            Self::Bool(v) => ValueRef::Integer(i64::from(*v)),
        }))
    }
}

impl ToSql for DbValue {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match self {
            Self::Null => Ok(IsNull::Yes),
            Self::Int(v) if *ty == Type::INT8 => <i64 as ToSql>::to_sql(v, ty, out),
            Self::Int(v) if *ty == Type::INT4 => <i32 as ToSql>::to_sql(&i32::try_from(*v)?, ty, out),
            Self::Int(v) if *ty == Type::INT2 => <i16 as ToSql>::to_sql(&i16::try_from(*v)?, ty, out),
            Self::Number(v) if *ty == Type::FLOAT8 && v.is_finite() => <f64 as ToSql>::to_sql(v, ty, out),
            Self::Number(v) if *ty == Type::FLOAT4 && v.is_finite() && (*v as f32).is_finite() => <f32 as ToSql>::to_sql(&(*v as f32), ty, out),
            Self::Text(v) if text_type(ty) => <String as ToSql>::to_sql(v, ty, out),
            Self::Bool(v) if *ty == Type::BOOL => <bool as ToSql>::to_sql(v, ty, out),
            _ => Err("scalar parameter type does not match the prepared PostgreSQL type".into()),
        }
    }
    fn accepts(_ty: &Type) -> bool { true }
    postgres::types::to_sql_checked!();
}

impl<'a> FromSql<'a> for DbValue {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(if *ty == Type::INT8 { Self::Int(i64::from_sql(ty, raw)?) }
        else if *ty == Type::INT4 { Self::Int(i64::from(i32::from_sql(ty, raw)?)) }
        else if *ty == Type::INT2 { Self::Int(i64::from(i16::from_sql(ty, raw)?)) }
        else if *ty == Type::FLOAT8 { Self::Number(f64::from_sql(ty, raw)?) }
        else if *ty == Type::FLOAT4 { Self::Number(f64::from(f32::from_sql(ty, raw)?)) }
        else if text_type(ty) { Self::Text(String::from_sql(ty, raw)?) }
        else if *ty == Type::BOOL { Self::Bool(bool::from_sql(ty, raw)?) }
        else { return Err("unsupported PostgreSQL result type; cast to a supported scalar in SQL".into()); })
    }
    fn from_sql_null(_ty: &Type) -> Result<Self, Box<dyn Error + Sync + Send>> { Ok(Self::Null) }
    fn accepts(_ty: &Type) -> bool { true }
}

fn text_type(ty: &Type) -> bool {
    matches!(*ty, Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN)
}
