use std::path::PathBuf;

use applerestore::{firmware, Result};

pub fn run(cache_dir: Option<PathBuf>, clear: bool, path_only: bool) -> Result<()> {
    let cache = match cache_dir {
        Some(d) => d,
        None => firmware::default_cache_dir()?,
    };

    if path_only {
        println!("{}", cache.display());
        return Ok(());
    }

    if clear {
        if cache.exists() {
            std::fs::remove_dir_all(&cache)?;
        }
        println!("Cleared firmware cache at {}", cache.display());
        return Ok(());
    }

    println!("Firmware cache: {}", cache.display());
    if !cache.exists() {
        println!("  (empty)");
        return Ok(());
    }

    let mut total = 0u64;
    let mut found = false;
    for entry in std::fs::read_dir(&cache)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("ipsw") {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            total += size;
            found = true;
            println!(
                "  {}  ({:.1} GB)",
                path.file_name().unwrap().to_string_lossy(),
                size as f64 / 1e9
            );
        }
    }
    if !found {
        println!("  (no firmware cached)");
    } else {
        println!("  total: {:.1} GB", total as f64 / 1e9);
    }
    Ok(())
}
