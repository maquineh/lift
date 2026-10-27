//! General synchronous SQL API. Pools, transactions and rows are affine Lift
//! resources; r2d2 sharing and exclusive connection leases remain internal.
pub use crate::value::DbValue;
use postgres::{
    config::{Host, SslMode},
    Client, Config, NoTls,
};
use postgres_native_tls::MakeTlsConnector;
use r2d2::{ManageConnection, Pool, PooledConnection};
use std::{error::Error, fmt, sync::Arc, time::Duration};

pub struct DbPool {
    pool: Pool<Manager>,
    driver: &'static str,
}
pub struct DbTransaction {
    lease: Option<PooledConnection<Manager>>,
    failed: bool,
}
pub struct DbRows {
    columns: Vec<String>,
    rows: Vec<Vec<DbValue>>,
}

#[derive(Debug)]
struct AdapterError(&'static str);
impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for AdapterError {}

enum Source {
    Sqlite {
        path: String,
        busy_timeout: Duration,
    },
    Postgres {
        config: Config,
        tls: Option<native_tls::TlsConnector>,
    },
}
struct Manager {
    source: Arc<Source>,
}
enum DriverConnection {
    Sqlite(rusqlite::Connection),
    Postgres(Client),
}
struct ManagedConnection {
    driver: DriverConnection,
    dirty: bool,
    broken: bool,
}

impl ManageConnection for Manager {
    type Connection = ManagedConnection;
    type Error = AdapterError;
    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let driver = match self.source.as_ref() {
            Source::Sqlite { path, busy_timeout } => {
                let connection = rusqlite::Connection::open(path)
                    .map_err(|_| AdapterError("SQLite connection failed"))?;
                connection
                    .busy_timeout(*busy_timeout)
                    .map_err(|_| AdapterError("SQLite busy timeout failed"))?;
                connection
                    .execute_batch("PRAGMA foreign_keys = ON")
                    .map_err(|_| AdapterError("SQLite initialization failed"))?;
                DriverConnection::Sqlite(connection)
            }
            Source::Postgres { config, tls } => {
                let client = match tls {
                    Some(tls) => config.connect(MakeTlsConnector::new(tls.clone())),
                    None => config.connect(NoTls), // Only an explicitly supplied sslmode=disable reaches this branch.
                }.map_err(|_| AdapterError("PostgreSQL connection failed (authentication, network, or TLS verification)"))?;
                DriverConnection::Postgres(client)
            }
        };
        Ok(ManagedConnection {
            driver,
            dirty: false,
            broken: false,
        })
    }
    fn is_valid(&self, connection: &mut Self::Connection) -> Result<(), Self::Error> {
        if self.has_broken(connection) {
            return Err(AdapterError("connection is not reusable"));
        }
        match &mut connection.driver {
            DriverConnection::Sqlite(c) => c
                .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .map(|_| ())
                .map_err(|_| AdapterError("SQLite health check failed")),
            DriverConnection::Postgres(c) => c
                .simple_query("SELECT 1")
                .map(|_| ())
                .map_err(|_| AdapterError("PostgreSQL health check failed")),
        }
    }
    fn has_broken(&self, connection: &mut Self::Connection) -> bool {
        connection.broken
            || connection.dirty
            || match &connection.driver {
                DriverConnection::Sqlite(c) => !c.is_autocommit(),
                DriverConnection::Postgres(c) => c.is_closed(),
            }
    }
}

pub fn available() {}
pub fn value_null() -> DbValue {
    DbValue::Null
}
pub fn value_int(value: i64) -> DbValue {
    DbValue::Int(value)
}
pub fn value_number(value: f64) -> DbValue {
    DbValue::Number(value)
}
pub fn value_text(value: &str) -> DbValue {
    DbValue::Text(value.to_owned())
}
pub fn value_bool(value: bool) -> DbValue {
    DbValue::Bool(value)
}

pub fn create_pool(
    driver: &str,
    url: &str,
    min_idle: i64,
    max_connections: i64,
    acquire_timeout_ms: i64,
) -> Result<DbPool, String> {
    if !(1..=1024).contains(&max_connections)
        || min_idle < 0
        || min_idle > max_connections
        || !(1..=3_600_000).contains(&acquire_timeout_ms)
    {
        return Err("pool configuration requires 0 <= min_idle <= max_connections (1..1024) and acquire_timeout_ms 1..3600000".into());
    }
    let timeout = Duration::from_millis(acquire_timeout_ms as u64);
    let (source, driver) = match driver {
        "sqlite" => {
            let path = url
                .strip_prefix("sqlite:")
                .ok_or("SQLite URL must use sqlite:<path>")?;
            if path.is_empty() || path.starts_with("//") {
                return Err(
                    "SQLite URL must be sqlite:<path> (use sqlite:/absolute/path, not sqlite://)"
                        .into(),
                );
            }
            if path.to_ascii_lowercase().starts_with("file:") {
                return Err("SQLite file: URIs are unsupported; use a literal file path or :memory: with max_connections=1".into());
            }
            if path == ":memory:" && max_connections != 1 {
                return Err("SQLite :memory: requires max_connections=1".into());
            }
            (
                Source::Sqlite {
                    path: path.to_owned(),
                    busy_timeout: timeout,
                },
                "sqlite",
            )
        }
        "postgres" => {
            if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
                return Err("PostgreSQL URL must use postgres:// or postgresql://".into());
            }
            let mut config = url
                .parse::<Config>()
                .map_err(|_| "invalid PostgreSQL URL (values are redacted)")?;
            config.connect_timeout(timeout);
            let tls = match config.get_ssl_mode() {
                SslMode::Require => {
                    if config.get_hosts().is_empty() || config.get_hosts().iter().any(|host| !matches!(host, Host::Tcp(name) if !name.is_empty())) {
                        return Err("verified PostgreSQL TLS requires an explicit TCP hostname/IP for certificate validation".into());
                    }
                    let mut builder = native_tls::TlsConnector::builder();
                    // Both switches stay false: Require is transport policy,
                    // while native-tls validates trust chain AND effective hostname.
                    builder.danger_accept_invalid_certs(false).danger_accept_invalid_hostnames(false);
                    if let Some(path) = std::env::var_os("LIFT_DB_TLS_ROOT_CERT") {
                        let pem = std::fs::read(path).map_err(|_| "could not read LIFT_DB_TLS_ROOT_CERT")?;
                        let certificate = native_tls::Certificate::from_pem(&pem).map_err(|_| "invalid PEM root certificate")?;
                        builder.add_root_certificate(certificate);
                    }
                    Some(builder.build().map_err(|_| "could not initialize certificate-verifying TLS")?)
                }
                SslMode::Disable => None,
                _ => return Err("PostgreSQL URL must explicitly set sslmode=require (verified TLS) or sslmode=disable (explicit plaintext for local/CI use)".into()),
            };
            (Source::Postgres { config, tls }, "postgres")
        }
        _ => return Err("unknown database driver; use sqlite or postgres".into()),
    };
    let memory_database = matches!(&source,Source::Sqlite {path,..} if path == ":memory:");
    let manager = Manager {
        source: Arc::new(source),
    };
    let mut builder = Pool::builder()
        .max_size(max_connections as u32)
        .min_idle(Some(min_idle as u32))
        .connection_timeout(timeout)
        .test_on_check_out(true)
        .error_handler(Box::new(r2d2::NopErrorHandler));
    if memory_database {
        // The sole connection IS the in-memory database. Retiring an idle or
        // aged connection would silently destroy its schema and committed data.
        builder = builder.idle_timeout(None).max_lifetime(None);
    }
    let pool = builder.build(manager).map_err(|_| "database pool initialization failed (configuration, authentication, network or TLS); URL is redacted")?;
    // min_idle=0 is lazy after construction, but an invalid URL/auth/TLS must
    // still fail create_pool rather than silently exposing a broken pool.
    {
        let _probe = pool
            .get()
            .map_err(|_| "database pool validation failed; URL is redacted")?;
    }
    Ok(DbPool { pool, driver })
}

pub fn pool_driver(pool: &DbPool) -> String {
    pool.driver.into()
}
pub fn pool_size(pool: &DbPool) -> i64 {
    i64::from(pool.pool.state().connections)
}
pub fn pool_idle(pool: &DbPool) -> i64 {
    i64::from(pool.pool.state().idle_connections)
}
pub fn close_pool(pool: DbPool) {
    drop(pool);
}
pub fn close_rows(rows: DbRows) {
    drop(rows);
}

pub fn execute(pool: &DbPool, sql: &str, parameters: &[DbValue]) -> Result<i64, String> {
    validate_input(sql, parameters)?;
    let mut lease = pool
        .pool
        .get()
        .map_err(|_| "database pool acquisition timed out")?;
    execute_on(&mut lease, sql, parameters)
}
pub fn query(pool: &DbPool, sql: &str, parameters: &[DbValue]) -> Result<DbRows, String> {
    validate_input(sql, parameters)?;
    let mut lease = pool
        .pool
        .get()
        .map_err(|_| "database pool acquisition timed out")?;
    query_on(&mut lease, sql, parameters)
}
pub fn begin(pool: &DbPool) -> Result<DbTransaction, String> {
    let mut lease = pool
        .pool
        .get()
        .map_err(|_| "database pool acquisition timed out")?;
    // Mark dirty before sending BEGIN: any ambiguous failure discards the lease.
    lease.dirty = true;
    command(&mut lease, "BEGIN").map_err(|_| "could not begin database transaction")?;
    Ok(DbTransaction {
        lease: Some(lease),
        failed: false,
    })
}
pub fn tx_execute(
    transaction: &mut DbTransaction,
    sql: &str,
    parameters: &[DbValue],
) -> Result<i64, String> {
    let result = validate_input(sql, parameters).and_then(|_| {
        execute_on(
            transaction
                .lease
                .as_mut()
                .ok_or("transaction is finished")?,
            sql,
            parameters,
        )
    });
    transaction.failed |= result.is_err();
    result
}
pub fn tx_query(
    transaction: &mut DbTransaction,
    sql: &str,
    parameters: &[DbValue],
) -> Result<DbRows, String> {
    let result = validate_input(sql, parameters).and_then(|_| {
        query_on(
            transaction
                .lease
                .as_mut()
                .ok_or("transaction is finished")?,
            sql,
            parameters,
        )
    });
    transaction.failed |= result.is_err();
    result
}
pub fn commit(mut transaction: DbTransaction) -> Result<(), String> {
    if transaction.failed {
        return Err(
            "transaction contains a failed operation; rolled back instead of committing".into(),
        );
    }
    finish(&mut transaction, "COMMIT")
}
pub fn rollback(mut transaction: DbTransaction) -> Result<(), String> {
    finish(&mut transaction, "ROLLBACK")
}
fn finish(transaction: &mut DbTransaction, sql: &str) -> Result<(), String> {
    let lease = transaction
        .lease
        .as_mut()
        .ok_or("transaction is finished")?;
    match command(lease, sql) {
        Ok(()) => {
            lease.dirty = false;
            transaction.lease.take();
            Ok(())
        }
        Err(_) => {
            lease.broken = true;
            transaction.lease.take();
            Err("transaction completion failed; connection discarded".into())
        }
    }
}
impl Drop for DbTransaction {
    fn drop(&mut self) {
        if let Some(mut lease) = self.lease.take() {
            if command(&mut lease, "ROLLBACK").is_ok() {
                lease.dirty = false;
            } else {
                lease.broken = true;
            }
            // r2d2::ManageConnection::has_broken discards failed/dirty leases.
        }
    }
}
fn command(connection: &mut ManagedConnection, sql: &str) -> Result<(), ()> {
    match &mut connection.driver {
        DriverConnection::Sqlite(c) => c.execute_batch(sql).map_err(|_| ()),
        DriverConnection::Postgres(c) => c.batch_execute(sql).map_err(|_| ()),
    }
}
fn validate_input(sql: &str, parameters: &[DbValue]) -> Result<(), String> {
    crate::sql::validate(sql)?;
    if parameters
        .iter()
        .any(|p| matches!(p, DbValue::Number(n) if !n.is_finite()))
    {
        return Err("non-finite SQL number parameter".into());
    }
    Ok(())
}
fn sqlite_bind(
    statement: &mut rusqlite::Statement<'_>,
    parameters: &[DbValue],
) -> Result<(), String> {
    let mut used = vec![false; parameters.len()];
    for index in 1..=statement.parameter_count() {
        let position = match statement.parameter_name(index) {
            None => index,
            Some(name) => {
                let digits = name
                    .strip_prefix('$')
                    .or_else(|| name.strip_prefix('?'))
                    .ok_or("SQLite parameters must be positional ?, ?N or $N")?;
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return Err("SQLite parameters must be positional ?, ?N or $N".into());
                }
                digits
                    .parse::<usize>()
                    .map_err(|_| "SQL parameter index is out of range")?
            }
        };
        if position == 0 || position > parameters.len() {
            return Err("SQL parameter count/index mismatch".into());
        }
        used[position - 1] = true;
        statement
            .raw_bind_parameter(index, &parameters[position - 1])
            .map_err(|_| "SQLite parameter binding failed")?;
    }
    if used.iter().any(|used| !used) {
        return Err("SQL parameter count/index mismatch".into());
    }
    Ok(())
}
fn execute_on(
    connection: &mut ManagedConnection,
    sql: &str,
    parameters: &[DbValue],
) -> Result<i64, String> {
    let affected = match &mut connection.driver {
        DriverConnection::Sqlite(c) => {
            let mut statement = c
                .prepare(sql)
                .map_err(|_| "SQLite statement preparation failed")?;
            if statement.column_count() != 0 {
                return Err("execute cannot discard result columns; use query".into());
            }
            sqlite_bind(&mut statement, parameters)?;
            statement
                .raw_execute()
                .map_err(|_| "SQLite statement execution failed")? as u64
        }
        DriverConnection::Postgres(c) => {
            let statement = c
                .prepare(sql)
                .map_err(|_| "PostgreSQL statement preparation failed")?;
            if !statement.columns().is_empty() {
                return Err("execute cannot discard result columns; use query".into());
            }
            let bindings = parameters
                .iter()
                .map(|p| p as &(dyn postgres::types::ToSql + Sync))
                .collect::<Vec<_>>();
            c.execute(&statement, &bindings)
                .map_err(|_| "PostgreSQL statement execution failed")?
        }
    };
    i64::try_from(affected).map_err(|_| "affected row count is outside Lift int range".into())
}
fn query_on(
    connection: &mut ManagedConnection,
    sql: &str,
    parameters: &[DbValue],
) -> Result<DbRows, String> {
    match &mut connection.driver {
        DriverConnection::Sqlite(c) => {
            let mut statement = c
                .prepare(sql)
                .map_err(|_| "SQLite statement preparation failed")?;
            let columns = statement
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            sqlite_bind(&mut statement, parameters)?;
            let mut cursor = statement.raw_query();
            let mut rows = Vec::new();
            while let Some(row) = cursor.next().map_err(|_| "SQLite query failed")? {
                let mut values = Vec::with_capacity(columns.len());
                for index in 0..columns.len() {
                    use rusqlite::types::ValueRef;
                    values.push(
                        match row
                            .get_ref(index)
                            .map_err(|_| "SQLite result column failed")?
                        {
                            ValueRef::Null => DbValue::Null,
                            ValueRef::Integer(n) => DbValue::Int(n),
                            ValueRef::Real(n) => DbValue::Number(n),
                            ValueRef::Text(s) => DbValue::Text(
                                std::str::from_utf8(s)
                                    .map_err(|_| "SQLite text is not UTF-8")?
                                    .to_owned(),
                            ),
                            ValueRef::Blob(_) => {
                                return Err(
                                    "BLOB results are unsupported; cast to a scalar in SQL".into()
                                )
                            }
                        },
                    );
                }
                rows.push(values);
            }
            Ok(DbRows { columns, rows })
        }
        DriverConnection::Postgres(c) => {
            let statement = c
                .prepare(sql)
                .map_err(|_| "PostgreSQL statement preparation failed")?;
            let columns = statement
                .columns()
                .iter()
                .map(|c| c.name().to_owned())
                .collect::<Vec<_>>();
            let bindings = parameters
                .iter()
                .map(|p| p as &(dyn postgres::types::ToSql + Sync))
                .collect::<Vec<_>>();
            let rows = c
                .query(&statement, &bindings)
                .map_err(|_| "PostgreSQL query failed")?
                .into_iter()
                .map(|row| {
                    (0..columns.len())
                        .map(|i| {
                            row.try_get::<_, DbValue>(i).map_err(|_| {
                                "unsupported or invalid PostgreSQL scalar result".to_owned()
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DbRows { columns, rows })
        }
    }
}

pub fn row_count(rows: &DbRows) -> i64 {
    rows.rows.len() as i64
}
pub fn column_count(rows: &DbRows) -> i64 {
    rows.columns.len() as i64
}
pub fn column_name(rows: &DbRows, column: i64) -> Result<String, String> {
    rows.columns
        .get(usize::try_from(column).map_err(|_| "negative column index")?)
        .cloned()
        .ok_or_else(|| "column index out of bounds".into())
}
fn cell(rows: &DbRows, row: i64, column: i64) -> Result<&DbValue, String> {
    let row = usize::try_from(row).map_err(|_| "negative row index")?;
    let column = usize::try_from(column).map_err(|_| "negative column index")?;
    rows.rows
        .get(row)
        .and_then(|row| row.get(column))
        .ok_or_else(|| "row/column index out of bounds".into())
}
pub fn is_null(rows: &DbRows, row: i64, column: i64) -> Result<bool, String> {
    Ok(matches!(cell(rows, row, column)?, DbValue::Null))
}
pub fn get_int(rows: &DbRows, row: i64, column: i64) -> Result<i64, String> {
    match cell(rows, row, column)? {
        DbValue::Int(v) => Ok(*v),
        _ => Err("column is not a non-null int".into()),
    }
}
pub fn get_number(rows: &DbRows, row: i64, column: i64) -> Result<f64, String> {
    match cell(rows, row, column)? {
        DbValue::Number(v) => Ok(*v),
        _ => Err("column is not a non-null number".into()),
    }
}
pub fn get_text(rows: &DbRows, row: i64, column: i64) -> Result<String, String> {
    match cell(rows, row, column)? {
        DbValue::Text(v) => Ok(v.clone()),
        _ => Err("column is not non-null text".into()),
    }
}
pub fn get_bool(rows: &DbRows, row: i64, column: i64) -> Result<bool, String> {
    match cell(rows, row, column)? {
        DbValue::Bool(v) => Ok(*v),
        DbValue::Int(0) => Ok(false),
        DbValue::Int(1) => Ok(true),
        _ => Err("column is not a non-null boolean (or SQLite 0/1)".into()),
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn memory_database_pins_its_only_connection_across_idle_leases() {
        let pool = create_pool("sqlite", "sqlite::memory:", 0, 1, 1000).unwrap();
        assert_eq!(pool.pool.max_size(), 1);
        assert_eq!(pool.pool.idle_timeout(), None);
        assert_eq!(pool.pool.max_lifetime(), None);
        execute(&pool, "CREATE TABLE retained (id BIGINT PRIMARY KEY)", &[]).unwrap();
        for id in 1..=3 {
            let mut transaction = begin(&pool).unwrap();
            tx_execute(
                &mut transaction,
                "INSERT INTO retained VALUES ($1)",
                &[value_int(id)],
            )
            .unwrap();
            commit(transaction).unwrap();
            assert_eq!(pool.pool.state().idle_connections, 1);
            assert_eq!(
                row_count(&query(&pool, "SELECT id FROM retained", &[]).unwrap()),
                id
            );
        }
        // Scope disposal still defines the memory database lifetime.
        drop(pool);
        let replacement = create_pool("sqlite", "sqlite::memory:", 0, 1, 1000).unwrap();
        assert!(query(&replacement, "SELECT id FROM retained", &[]).is_err());
    }

    #[test]
    fn sqlite_uri_forms_cannot_create_private_memory_databases_in_a_pool() {
        for url in [
            "sqlite:file::memory:",
            "sqlite:file:db?mode=memory",
            "sqlite:file:db?mode=memory&cache=shared",
            "sqlite:FILE:db?mode=memory",
        ] {
            assert!(
                create_pool("sqlite", url, 0, 4, 100).is_err(),
                "accepted {url}"
            );
        }
    }
}
