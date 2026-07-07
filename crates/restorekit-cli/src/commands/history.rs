use std::path::PathBuf;

use restorekit::{history, Result};

/// `restorekit history list` — print the capture/restore log, newest first.
pub fn list(json: bool) -> Result<()> {
    let entries = history::list()?;
    if json {
        println!("{}", serde_json::to_string(&entries).unwrap());
        return Ok(());
    }
    if entries.is_empty() {
        println!("No history yet.");
        return Ok(());
    }
    for e in &entries {
        println!(
            "{}  {}  {}  [{}]  {}",
            e.timestamp_rfc3339,
            e.serial_number.as_deref().unwrap_or("—"),
            if e.ecid.is_empty() { "—" } else { &e.ecid },
            e.mode,
            e.status,
        );
    }
    Ok(())
}

/// `restorekit history export <path>` — write the whole log to a CSV file.
pub fn export(path: PathBuf) -> Result<()> {
    history::export_csv(&path)?;
    println!("Wrote history to {}", path.display());
    Ok(())
}

/// `restorekit history clear` — delete all logged history.
pub fn clear() -> Result<()> {
    history::clear()?;
    println!("History cleared.");
    Ok(())
}
