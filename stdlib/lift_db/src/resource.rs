//! Owned SQLite resources. Transactions own their connection.
use super::NoteStore;
use rusqlite::params;
pub struct SqliteConnection(NoteStore);
pub struct SqliteTransaction(Option<NoteStore>);
pub fn available() {}
pub fn configured_path() -> String {
    std::env::var("LIFT_NOTES_DB").unwrap_or_else(|_| "lift-notes.sqlite3".into())
}
pub fn open(path: &str) -> Result<SqliteConnection, String> {
    NoteStore::open(path)
        .map(SqliteConnection)
        .map_err(|e| e.to_string())
}
pub fn begin(connection: SqliteConnection) -> Result<SqliteTransaction, String> {
    connection
        .0
        .connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    Ok(SqliteTransaction(Some(connection.0)))
}
pub fn insert(transaction: &mut SqliteTransaction, body: &str) -> Result<i64, String> {
    let store = transaction.0.as_mut().ok_or("transaction is finished")?;
    store
        .connection
        .execute("INSERT INTO notes (body) VALUES (?1)", params![body])
        .map_err(|e| e.to_string())?;
    Ok(store.connection.last_insert_rowid())
}
pub fn list(connection: &SqliteConnection) -> Result<String, String> {
    let notes = connection.0.list_notes().map_err(|e| e.to_string())?;
    Ok(notes
        .into_iter()
        .map(|n| format!("{}\t{}", n.id, super::interop::escape_lines(&n.body)))
        .collect::<Vec<_>>()
        .join("\n"))
}
pub fn commit(transaction: SqliteTransaction) -> Result<SqliteConnection, String> {
    finish(transaction, "COMMIT")
}
pub fn rollback(transaction: SqliteTransaction) -> Result<SqliteConnection, String> {
    finish(transaction, "ROLLBACK")
}
fn finish(mut transaction: SqliteTransaction, sql: &str) -> Result<SqliteConnection, String> {
    let store = transaction.0.as_ref().ok_or("transaction is finished")?;
    store
        .connection
        .execute_batch(sql)
        .map_err(|e| e.to_string())?;
    Ok(SqliteConnection(
        transaction.0.take().expect("active transaction"),
    ))
}
pub fn close(connection: SqliteConnection) {
    drop(connection);
}
impl Drop for SqliteTransaction {
    fn drop(&mut self) {
        if let Some(store) = &self.0 {
            let _ = store.connection.execute_batch("ROLLBACK");
        }
    }
}
