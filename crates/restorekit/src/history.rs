//! Persistent capture/restore history.
//!
//! Every Mac that passes through this host — captured in recovery/booted, or
//! restored — is logged to a bundled-SQLite database at `<config_dir>/
//! history.db`. Schema migrations are embedded in the binary (see
//! `../migrations`), so nothing is read from disk at startup and the whole store
//! ships in a single binary. The CLI and desktop app share the same file.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// One logged device: captured in a restore-family mode, or restored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub serial_number: Option<String>,
    pub ecid: String,
    pub model_identifier: Option<String>,
    pub name: String,
    pub mode: String,
    pub status: String,
    pub timestamp_rfc3339: String,
}

/// Migrations, embedded in the binary at compile time.
fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: OnceLock<Migrations<'static>> = OnceLock::new();
    MIGRATIONS.get_or_init(|| {
        Migrations::new(vec![M::up(include_str!("../migrations/001_init.sql"))])
    })
}

fn db(e: rusqlite::Error) -> Error {
    Error::Database(e.to_string())
}

/// `<config_dir>/history.db`, alongside the firmware cache dir (its parent).
fn db_path() -> Result<PathBuf> {
    let cache = crate::firmware::default_cache_dir()?;
    let dir = cache.parent().map(Path::to_path_buf).unwrap_or(cache);
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("history.db"))
}

/// Open the history database, applying any pending embedded migrations.
fn open() -> Result<Connection> {
    let mut conn = Connection::open(db_path()?).map_err(db)?;
    migrations()
        .to_latest(&mut conn)
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(conn)
}

/// Append a device to the history log.
pub fn record(entry: &HistoryEntry) -> Result<()> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO captures \
         (serial_number, ecid, model_identifier, name, mode, status, timestamp_rfc3339) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.serial_number,
            entry.ecid,
            entry.model_identifier,
            entry.name,
            entry.mode,
            entry.status,
            entry.timestamp_rfc3339,
        ],
    )
    .map_err(db)?;
    Ok(())
}

/// Every logged device, newest first.
pub fn list() -> Result<Vec<HistoryEntry>> {
    let conn = open()?;
    let mut stmt = conn
        .prepare(
            "SELECT serial_number, ecid, model_identifier, name, mode, status, timestamp_rfc3339 \
             FROM captures ORDER BY id DESC",
        )
        .map_err(db)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(HistoryEntry {
                serial_number: r.get(0)?,
                ecid: r.get(1)?,
                model_identifier: r.get(2)?,
                name: r.get(3)?,
                mode: r.get(4)?,
                status: r.get(5)?,
                timestamp_rfc3339: r.get(6)?,
            })
        })
        .map_err(db)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db)
}

/// Delete all history.
pub fn clear() -> Result<()> {
    open()?.execute("DELETE FROM captures", []).map_err(db)?;
    Ok(())
}

/// Quote a CSV field when it contains a comma, quote, or newline.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Write the whole history to `path` as a spreadsheet-openable CSV.
pub fn export_csv(path: &Path) -> Result<()> {
    let entries = list()?;
    let mut out = String::from("Timestamp,Serial,ECID,Model,Name,Mode,Status\n");
    for e in &entries {
        let cols = [
            e.timestamp_rfc3339.as_str(),
            e.serial_number.as_deref().unwrap_or(""),
            e.ecid.as_str(),
            e.model_identifier.as_deref().unwrap_or(""),
            e.name.as_str(),
            e.mode.as_str(),
            e.status.as_str(),
        ];
        out.push_str(&cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_are_valid() {
        // Embedded migrations must parse and apply to an in-memory DB.
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        super::migrations().to_latest(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO captures (ecid, name, mode, status, timestamp_rfc3339) \
             VALUES ('0x1', 'Mac', 'recovery', 'captured', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM captures", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
