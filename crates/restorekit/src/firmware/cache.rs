use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::Firmware;
use crate::error::{Error, Result};

/// The firmware cache directory: `${XDG_CONFIG_HOME:-~/.config}/restorekit/firmwares`.
///
/// Overridable by the caller (CLI `--cache-dir` / `RESTOREKIT_CACHE_DIR`).
pub fn default_cache_dir() -> Result<PathBuf> {
    resolve_cache_dir(
        std::env::var_os("RESTOREKIT_CACHE_DIR"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Pure resolution logic behind [`default_cache_dir`] (testable without touching
/// process-global environment variables).
fn resolve_cache_dir(
    override_dir: Option<std::ffi::OsString>,
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf> {
    if let Some(dir) = override_dir.filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let base = match xdg_config_home {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => PathBuf::from(home.filter(|h| !h.is_empty()).ok_or(Error::NoHomeDir)?).join(".config"),
    };
    Ok(base.join("restorekit").join("firmwares"))
}

/// Path a firmware would occupy in the cache.
pub fn cache_path(cache_dir: &Path, fw: &Firmware) -> PathBuf {
    cache_dir.join(fw.file_name())
}

/// Return the cached path if the firmware is present and verified.
///
/// Verification at download time is recorded in a `.json` sidecar. On repeat
/// lookups we trust that record (file present + size matches + recorded
/// checksum matches what we now expect) and skip re-hashing the ~20 GB file.
/// A full hash runs only when the sidecar is missing or inconsistent.
pub fn cached(cache_dir: &Path, fw: &Firmware) -> Option<PathBuf> {
    let path = cache_path(cache_dir, fw);
    let actual_size = std::fs::metadata(&path).ok()?.len();

    // Fast path: a sidecar we wrote after a verified download.
    if let Some(sidecar) = read_sidecar(&path) {
        let size_ok = actual_size == sidecar.size && (fw.size == 0 || fw.size == actual_size);
        let checksum_ok = match (&fw.sha256, &sidecar.sha256) {
            (Some(want), Some(have)) => want.eq_ignore_ascii_case(have),
            (Some(_), None) => false,
            (None, _) => true,
        };
        if size_ok && checksum_ok {
            return Some(path);
        }
    }

    // Slow path: no trustworthy sidecar — verify by hashing, then record it.
    if let Some(expected) = &fw.sha256 {
        match hash_file::<Sha256>(&path) {
            Ok(actual) if actual.eq_ignore_ascii_case(expected) => {
                let _ = write_sidecar(&path, fw);
                Some(path)
            }
            _ => None,
        }
    } else if fw.size == 0 || fw.size == actual_size {
        Some(path)
    } else {
        None
    }
}

/// Read and parse the `.json` sidecar next to a cached firmware, if present.
fn read_sidecar(final_path: &Path) -> Option<Firmware> {
    let sidecar = final_path.with_extension("ipsw.json");
    let bytes = std::fs::read(sidecar).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Record verified firmware metadata in a `.json` sidecar next to the file.
pub(super) fn write_sidecar(final_path: &Path, fw: &Firmware) -> Result<()> {
    let sidecar = final_path.with_extension("ipsw.json");
    let json = serde_json::to_vec_pretty(fw)
        .map_err(|e| Error::Download(format!("sidecar serialize: {e}")))?;
    std::fs::write(sidecar, json)?;
    Ok(())
}

/// Verify a file against the firmware's checksum (sha256 preferred, sha1 fallback).
pub(super) fn verify(path: &Path, fw: &Firmware) -> Result<()> {
    let (expected, actual) = if let Some(expected) = &fw.sha256 {
        (expected, hash_file::<Sha256>(path)?)
    } else if let Some(expected) = &fw.sha1 {
        (expected, hash_file::<sha1::Sha1>(path)?)
    } else {
        return Ok(());
    };
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(Error::ChecksumMismatch {
            path: path.to_path_buf(),
            expected: expected.clone(),
            actual,
        })
    }
}

/// Stream a file through a digest and return the lowercase hex checksum.
fn hash_file<D: Digest>(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = D::new();
    let mut buf = [0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    use std::fmt::Write;
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fw() -> Firmware {
        Firmware {
            identifier: "MacBookAir10,1".into(),
            version: "26.5.2".into(),
            build: "25F84".into(),
            url: "https://updates.cdn-apple.com/x/UniversalMac_26.5.2_25F84_Restore.ipsw".into(),
            size: 100,
            sha256: None,
            sha1: None,
            signed: true,
        }
    }

    #[test]
    fn cache_dir_prefers_env_override() {
        let got = resolve_cache_dir(
            Some("/tmp/ar-test-cache".into()),
            Some("/tmp/xdg".into()),
            Some("/home/x".into()),
        )
        .unwrap();
        assert_eq!(got, PathBuf::from("/tmp/ar-test-cache"));
    }

    #[test]
    fn cache_dir_uses_xdg() {
        let got = resolve_cache_dir(None, Some("/tmp/xdg".into()), Some("/home/x".into())).unwrap();
        assert_eq!(got, PathBuf::from("/tmp/xdg/restorekit/firmwares"));
    }

    #[test]
    fn cache_dir_falls_back_to_home() {
        let got = resolve_cache_dir(None, None, Some("/home/x".into())).unwrap();
        assert_eq!(got, PathBuf::from("/home/x/.config/restorekit/firmwares"));
    }

    #[test]
    fn cache_hit_trusts_sidecar_without_hashing() {
        let dir = std::env::temp_dir().join(format!("ar-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut f = fw();
        f.size = 5;
        f.sha256 = Some("deadbeef".into()); // deliberately NOT the real hash of the file
        let path = cache_path(&dir, &f);
        std::fs::write(&path, b"hello").unwrap(); // 5 bytes; real sha256 != deadbeef
        write_sidecar(&path, &f).unwrap();

        // Sidecar records size 5 + sha256 "deadbeef" matching the request, so
        // this is a hit even though the file's true hash differs — proving we
        // trusted the sidecar rather than re-hashing.
        assert_eq!(cached(&dir, &f), Some(path.clone()));

        // A different expected checksum invalidates the fast path and falls
        // through to hashing, which won't match "hello" → miss.
        let mut g = f.clone();
        g.sha256 = Some("00ff".into());
        assert_eq!(cached(&dir, &g), None);

        std::fs::remove_dir_all(&dir).ok();
    }
}
