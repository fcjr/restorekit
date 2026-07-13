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
    /// Encryption-key obliteration verdict for an erase restore (`confirmed`,
    /// `failed`, `unconfirmed`, `not_applicable`); `None` for captures and rows
    /// logged before this was tracked. Defaulted so older records and callers
    /// that omit it still deserialize.
    #[serde(default)]
    pub obliteration: Option<String>,
    /// Full checkpoint messages the device reported during the restore, as a
    /// JSON array of compact-JSON plists (device self-report, not Apple-signed).
    /// `None` for captures and older rows.
    #[serde(default)]
    pub checkpoints_json: Option<String>,
    /// The same checkpoints as a JSON array of the exact plists serialized to XML
    /// (lossless). `None` for captures and older rows.
    #[serde(default)]
    pub checkpoints_raw: Option<String>,
}

/// One device ever seen by this host, deduped by ECID and enriched across the
/// modes it passes through (a serial appears in recovery, the model in DFU, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeenDevice {
    pub ecid: String,
    pub serial_number: Option<String>,
    pub model_identifier: Option<String>,
    pub name: String,
    pub chip: Option<String>,
    pub board: Option<String>,
    pub mode: String,
    pub port: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
}

/// Migrations, embedded in the binary at compile time.
fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: OnceLock<Migrations<'static>> = OnceLock::new();
    MIGRATIONS.get_or_init(|| {
        Migrations::new(vec![
            M::up(include_str!("../migrations/001_init.sql")),
            M::up(include_str!("../migrations/002_seen_devices.sql")),
            M::up(include_str!("../migrations/003_obliteration.sql")),
            M::up(include_str!("../migrations/004_checkpoints.sql")),
        ])
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

/// Current UTC time as an RFC3339 string (e.g. `2026-07-12T20:30:00Z`), for
/// stamping history entries from Rust callers. Uses SQLite (already a
/// dependency) so we don't pull in a date library. Empty only if that fails,
/// which is effectively impossible for an in-memory connection.
pub fn now_rfc3339() -> String {
    Connection::open_in_memory()
        .and_then(|c| {
            c.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now')", [], |r| {
                r.get(0)
            })
        })
        .unwrap_or_default()
}

/// Append a device to the history log.
pub fn record(entry: &HistoryEntry) -> Result<()> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO captures \
         (serial_number, ecid, model_identifier, name, mode, status, timestamp_rfc3339, \
          obliteration, checkpoints_json, checkpoints_raw) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            entry.serial_number,
            entry.ecid,
            entry.model_identifier,
            entry.name,
            entry.mode,
            entry.status,
            entry.timestamp_rfc3339,
            entry.obliteration,
            entry.checkpoints_json,
            entry.checkpoints_raw,
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
            "SELECT serial_number, ecid, model_identifier, name, mode, status, timestamp_rfc3339, \
             obliteration, checkpoints_json, checkpoints_raw FROM captures ORDER BY id DESC",
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
                obliteration: r.get(7)?,
                checkpoints_json: r.get(8)?,
                checkpoints_raw: r.get(9)?,
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
    let mut out = String::from("Timestamp,Serial,ECID,Model,Name,Mode,Status,Obliteration\n");
    for e in &entries {
        let cols = [
            e.timestamp_rfc3339.as_str(),
            e.serial_number.as_deref().unwrap_or(""),
            e.ecid.as_str(),
            e.model_identifier.as_deref().unwrap_or(""),
            e.name.as_str(),
            e.mode.as_str(),
            e.status.as_str(),
            e.obliteration.as_deref().unwrap_or(""),
        ];
        out.push_str(
            &cols
                .iter()
                .map(|c| csv_field(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

const SEEN_UPSERT: &str = "INSERT INTO seen_devices \
    (ecid, serial_number, model_identifier, name, chip, board, mode, port, first_seen, last_seen) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9) \
    ON CONFLICT(ecid) DO UPDATE SET \
      serial_number = COALESCE(excluded.serial_number, serial_number), \
      model_identifier = COALESCE(excluded.model_identifier, model_identifier), \
      name = excluded.name, \
      chip = COALESCE(excluded.chip, chip), \
      board = COALESCE(excluded.board, board), \
      mode = excluded.mode, \
      port = COALESCE(excluded.port, port), \
      last_seen = excluded.last_seen";

/// Upsert a batch of currently-seen devices, keyed by ECID. Richer data (a
/// serial or model that appears in a later mode) fills in blanks without
/// clobbering what's already known; `last_seen` always advances.
pub fn record_seen(devices: &[SeenDevice]) -> Result<()> {
    let mut conn = open()?;
    let tx = conn.transaction().map_err(db)?;
    {
        let mut stmt = tx.prepare(SEEN_UPSERT).map_err(db)?;
        for d in devices {
            stmt.execute(params![
                d.ecid,
                d.serial_number,
                d.model_identifier,
                d.name,
                d.chip,
                d.board,
                d.mode,
                d.port,
                d.last_seen,
            ])
            .map_err(db)?;
        }
    }
    tx.commit().map_err(db)?;
    Ok(())
}

/// Every device ever seen, most-recently-seen first.
pub fn list_seen() -> Result<Vec<SeenDevice>> {
    let conn = open()?;
    let mut stmt = conn
        .prepare(
            "SELECT ecid, serial_number, model_identifier, name, chip, board, mode, port, \
             first_seen, last_seen FROM seen_devices ORDER BY last_seen DESC",
        )
        .map_err(db)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SeenDevice {
                ecid: r.get(0)?,
                serial_number: r.get(1)?,
                model_identifier: r.get(2)?,
                name: r.get(3)?,
                chip: r.get(4)?,
                board: r.get(5)?,
                mode: r.get(6)?,
                port: r.get(7)?,
                first_seen: r.get(8)?,
                last_seen: r.get(9)?,
            })
        })
        .map_err(db)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db)
}

/// Write the seen-device history to `path` as CSV.
pub fn export_seen_csv(path: &Path) -> Result<()> {
    let entries = list_seen()?;
    let mut out =
        String::from("ECID,Serial,Model,Name,Chip,Board,Mode,Port,First seen,Last seen\n");
    for e in &entries {
        let cols = [
            e.ecid.as_str(),
            e.serial_number.as_deref().unwrap_or(""),
            e.model_identifier.as_deref().unwrap_or(""),
            e.name.as_str(),
            e.chip.as_deref().unwrap_or(""),
            e.board.as_deref().unwrap_or(""),
            e.mode.as_str(),
            e.port.as_deref().unwrap_or(""),
            e.first_seen.as_str(),
            e.last_seen.as_str(),
        ];
        out.push_str(
            &cols
                .iter()
                .map(|c| csv_field(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn now_rfc3339_is_well_formed() {
        let ts = super::now_rfc3339();
        // e.g. 2026-07-12T20:30:00Z
        assert_eq!(ts.len(), 20, "unexpected timestamp: {ts:?}");
        assert!(
            ts.ends_with('Z') && ts.as_bytes()[10] == b'T',
            "bad shape: {ts:?}"
        );
    }

    #[test]
    fn migrations_are_valid() {
        // Embedded migrations must parse and apply to an in-memory DB.
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        super::migrations().to_latest(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO captures (ecid, name, mode, status, timestamp_rfc3339, obliteration) \
             VALUES ('0x1', 'Mac', 'restore', 'restored', '2026-01-01T00:00:00Z', 'confirmed')",
            [],
        )
        .unwrap();
        let obl: Option<String> = conn
            .query_row("SELECT obliteration FROM captures", [], |r| r.get(0))
            .unwrap();
        assert_eq!(obl.as_deref(), Some("confirmed"));
    }
}
