//! A conservative single-statement boundary, not a SQL dialect rewriter.
//! Driver preparation validates the grammar; transaction/session commands are
//! excluded so a user statement cannot bypass the owned transaction protocol.
pub(crate) fn validate(sql: &str) -> Result<(), String> {
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut first = None;
    let mut ended = false;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"--") {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth != 0 {
                return Err("unterminated SQL comment".into());
            }
            continue;
        }
        if ended {
            return Err("exactly one SQL statement is allowed".into());
        }
        if first.is_none() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            if start == i {
                return Err("SQL must begin with a supported statement keyword".into());
            }
            first = Some(sql[start..i].to_ascii_uppercase());
            continue;
        }
        match bytes[i] {
            b';' => {
                ended = true;
                i += 1;
            }
            b'\'' | b'"' | b'`' | b'[' => {
                let closing = if bytes[i] == b'[' { b']' } else { bytes[i] };
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    if bytes[i] == closing {
                        i += 1;
                        if i < bytes.len() && bytes[i] == closing {
                            i += 1;
                        } else {
                            closed = true;
                            break;
                        }
                    } else if bytes[i] == b'\\' {
                        return Err("backslash escapes in SQL literals are unsupported; use bind parameters for text".into());
                    } else {
                        i += 1;
                    }
                }
                if !closed {
                    return Err("unterminated SQL quoted value".into());
                }
            }
            b'$' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'$' {
                    i += 1;
                    let delimiter = &sql[start..i];
                    let Some(end) = sql[i..].find(delimiter) else {
                        return Err("unterminated SQL dollar quote".into());
                    };
                    i += end + delimiter.len();
                }
            }
            _ => i += 1,
        }
    }
    match first.as_deref() {
        Some("SELECT" | "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "WITH" | "CREATE" | "ALTER" | "DROP" | "EXPLAIN" | "ANALYZE" | "VACUUM") => Ok(()),
        _ => Err("SQL command unsupported: use execute/query for one CRUD/DDL statement and the transaction API for begin/commit/rollback".into()),
    }
}
