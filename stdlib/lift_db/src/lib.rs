//! Small SQLite adapter used by Lift's notes service proof of concept.
//!
//! Connections and transactions stay as Rust-owned resources. They never cross
//! the current Lift FFI boundary as copied integers or strings.

use std::{error::Error, fmt, path::Path};

use rusqlite::{params, Connection};
pub mod resource;
pub mod api;
mod value;
mod sql;

#[derive(Debug)]
pub struct DbError(rusqlite::Error);

impl fmt::Display for DbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error)
    }
}

/// An owned, non-cloneable connection and the schema required by the demo.
pub struct NoteStore {
    connection: Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub id: i64,
    pub body: String,
}

impl NoteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                body TEXT NOT NULL
            );",
        )?;
        Ok(Self { connection })
    }

    /// Inserts one note in a transaction; dropping an uncommitted transaction
    /// lets rusqlite perform its normal rollback behavior.
    pub fn save_note(&mut self, body: &str) -> Result<i64, DbError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("INSERT INTO notes (body) VALUES (?1)", params![body])?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(id)
    }

    /// Returns an owned snapshot of all notes in insertion order.
    pub fn list_notes(&self) -> Result<Vec<Note>, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, body FROM notes ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// FFI-compatible, stateless helpers for small Lift programs and adapter smoke
/// checks. Each call owns and drops its connection; application servers should
/// use [`NoteStore`] directly and share it through an explicit Rust handle.
pub mod interop {
    use super::NoteStore;

    pub fn save_note(path: &str, body: &str) -> Result<i64, String> {
        let mut store = NoteStore::open(path).map_err(|error| error.to_string())?;
        store.save_note(body).map_err(|error| error.to_string())
    }

    pub fn list_notes(path: &str) -> Result<String, String> {
        let store = NoteStore::open(path).map_err(|error| error.to_string())?;
        let notes = store.list_notes().map_err(|error| error.to_string())?;
        Ok(notes
            .iter()
            .map(|note| format!("{}\t{}", note.id, escape_lines(&note.body)))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub(super) fn escape_lines(value: &str) -> String {
        value
            .chars()
            .flat_map(|character| match character {
                '\\' => "\\\\".chars().collect::<Vec<_>>(),
                '\n' => "\\n".chars().collect(),
                '\r' => "\\r".chars().collect(),
                '\t' => "\\t".chars().collect(),
                other => vec![other],
            })
            .collect()
    }
}
