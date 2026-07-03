use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

const IPSW_API: &str = "https://api.ipsw.me/v4";
const MESU_FEED: &str =
    "https://mesu.apple.com/assets/macos/com_apple_macOSIPSW/com_apple_macOSIPSW.xml";

/// Resolved firmware metadata for a specific Mac model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Firmware {
    pub identifier: String,
    pub version: String,
    pub build: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub sha1: Option<String>,
    pub signed: bool,
}

impl Firmware {
    /// Canonical cache filename, derived from the download URL's basename.
    pub fn file_name(&self) -> String {
        self.url
            .rsplit('/')
            .next()
            .filter(|s| s.ends_with(".ipsw"))
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!("UniversalMac_{}_{}_Restore.ipsw", self.version, self.build)
            })
    }
}

#[derive(Deserialize)]
struct IpswDeviceResponse {
    firmwares: Vec<IpswFirmware>,
}

#[derive(Deserialize)]
struct IpswFirmware {
    version: String,
    buildid: String,
    url: String,
    filesize: u64,
    sha256sum: Option<String>,
    sha1sum: Option<String>,
    signed: bool,
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("applerestore/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(Error::Http)
}

/// Resolve firmware for a model identifier. If `version` is given, match that
/// exact macOS version (e.g. "26.5.2"); otherwise pick the newest signed build.
///
/// Uses the ipsw.me API, falling back to Apple's official mesu feed.
pub fn resolve(identifier: &str, version: Option<&str>) -> Result<Firmware> {
    match resolve_ipswme(identifier, version) {
        Ok(fw) => Ok(fw),
        Err(primary) => resolve_mesu(identifier, version).map_err(|fallback| {
            Error::FirmwareResolution(format!(
                "ipsw.me failed ({primary}); mesu fallback failed ({fallback})"
            ))
        }),
    }
}

fn resolve_ipswme(identifier: &str, version: Option<&str>) -> Result<Firmware> {
    let url = format!("{IPSW_API}/device/{identifier}?type=ipsw");
    let resp = http_client()?
        .get(&url)
        .send()?
        .error_for_status()?
        .json::<IpswDeviceResponse>()?;

    let mut candidates: Vec<IpswFirmware> = resp.firmwares;
    if let Some(v) = version {
        candidates.retain(|f| f.version == v);
        let f = candidates.into_iter().next().ok_or_else(|| Error::NoFirmwareFound {
            identifier: identifier.to_string(),
            version: format!(" version {v}"),
        })?;
        return Ok(convert(identifier, f));
    }

    // Newest signed build (fall back to newest of any if none are marked signed).
    let f = candidates
        .iter()
        .position(|f| f.signed)
        .map(|i| candidates.swap_remove(i))
        .or_else(|| candidates.into_iter().next())
        .ok_or_else(|| Error::NoFirmwareFound {
            identifier: identifier.to_string(),
            version: String::new(),
        })?;
    Ok(convert(identifier, f))
}

fn convert(identifier: &str, f: IpswFirmware) -> Firmware {
    Firmware {
        identifier: identifier.to_string(),
        version: f.version,
        build: f.buildid,
        url: f.url,
        size: f.filesize,
        sha256: f.sha256sum,
        sha1: f.sha1sum,
        signed: f.signed,
    }
}

/// Fallback resolver using Apple's mesu plist feed (latest version only).
fn resolve_mesu(identifier: &str, version: Option<&str>) -> Result<Firmware> {
    let bytes = http_client()?
        .get(MESU_FEED)
        .send()?
        .error_for_status()?
        .bytes()?;
    let root: plist::Value = plist::from_bytes(&bytes)
        .map_err(|e| Error::FirmwareResolution(format!("mesu plist parse: {e}")))?;

    let builds = root
        .as_dictionary()
        .and_then(|d| d.get("MobileDeviceSoftwareVersionsByVersion"))
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("1"))
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("MobileDeviceSoftwareVersions"))
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get(identifier))
        .and_then(|v| v.as_dictionary())
        .ok_or_else(|| {
            Error::FirmwareResolution(format!("mesu feed has no entry for {identifier}"))
        })?;

    for (build, entry) in builds {
        let restore = match entry.as_dictionary().and_then(|d| d.get("Restore")) {
            Some(r) => r.as_dictionary().unwrap(),
            None => continue,
        };
        let product_version = restore
            .get("ProductVersion")
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if let Some(v) = version {
            if product_version != v {
                continue;
            }
        }
        let url = restore
            .get("FirmwareURL")
            .and_then(|v| v.as_string())
            .ok_or_else(|| Error::FirmwareResolution("mesu entry missing FirmwareURL".into()))?;
        return Ok(Firmware {
            identifier: identifier.to_string(),
            version: product_version.to_string(),
            build: build.clone(),
            url: url.to_string(),
            size: 0,
            sha256: None,
            sha1: restore
                .get("FirmwareSHA1")
                .and_then(|v| v.as_string())
                .map(str::to_string),
            signed: true,
        });
    }

    Err(Error::NoFirmwareFound {
        identifier: identifier.to_string(),
        version: version.map(|v| format!(" version {v}")).unwrap_or_default(),
    })
}

/// The firmware cache directory: `${XDG_CONFIG_HOME:-~/.config}/applerestore/firmwares`.
///
/// Overridable by the caller (CLI `--cache-dir` / `APPLERESTORE_CACHE_DIR`).
pub fn default_cache_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("APPLERESTORE_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => {
            let home = std::env::var_os("HOME").ok_or(Error::NoHomeDir)?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(base.join("applerestore").join("firmwares"))
}

/// Path a firmware would occupy in the cache.
pub fn cache_path(cache_dir: &Path, fw: &Firmware) -> PathBuf {
    cache_dir.join(fw.file_name())
}

/// Return the cached path if the firmware is present and passes checksum
/// verification (when a checksum is known).
pub fn cached(cache_dir: &Path, fw: &Firmware) -> Option<PathBuf> {
    let path = cache_path(cache_dir, fw);
    if !path.exists() {
        return None;
    }
    if let Some(expected) = &fw.sha256 {
        match sha256_file(&path) {
            Ok(actual) if &actual == expected => Some(path),
            _ => None,
        }
    } else {
        // No checksum to verify against; trust size if we have it.
        match (fw.size, std::fs::metadata(&path).map(|m| m.len())) {
            (0, _) => Some(path),
            (want, Ok(got)) if want == got => Some(path),
            _ => None,
        }
    }
}

/// Download (or resume) the firmware into the cache and verify its checksum.
/// Returns the path to the verified file. A cache hit short-circuits the download.
pub fn download(cache_dir: &Path, fw: &Firmware, progress: ProgressFn) -> Result<PathBuf> {
    std::fs::create_dir_all(cache_dir)?;
    let final_path = cache_path(cache_dir, fw);

    if let Some(path) = cached(cache_dir, fw) {
        progress(Event::CacheHit {
            path: path.display().to_string(),
        });
        return Ok(path);
    }

    let partial = final_path.with_extension("ipsw.partial");
    let mut downloaded = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);

    let client = http_client()?;
    let mut req = client.get(&fw.url);
    if downloaded > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={downloaded}-"));
        progress(Event::DownloadResumed {
            received: downloaded,
        });
    }
    let mut resp = req.send()?.error_for_status()?;

    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if downloaded > 0 && !resuming {
        // Server ignored the range; start over.
        downloaded = 0;
    }
    let total = downloaded
        + resp.content_length().unwrap_or(0);
    let total = if fw.size > 0 { fw.size } else { total };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(resuming)
        .write(true)
        .truncate(!resuming && downloaded == 0)
        .open(&partial)?;

    let mut buf = [0u8; 1 << 20];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        progress(Event::DownloadProgress {
            received: downloaded,
            total,
        });
    }
    file.flush()?;
    drop(file);

    progress(Event::Verifying);
    verify(&partial, fw)?;

    std::fs::rename(&partial, &final_path)?;
    write_sidecar(&final_path, fw)?;
    Ok(final_path)
}

fn verify(path: &Path, fw: &Firmware) -> Result<()> {
    if let Some(expected) = &fw.sha256 {
        let actual = sha256_file(path)?;
        if &actual != expected {
            return Err(Error::ChecksumMismatch {
                path: path.to_path_buf(),
                expected: expected.clone(),
                actual,
            });
        }
    } else if let Some(expected) = &fw.sha1 {
        let actual = sha1_file(path)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(Error::ChecksumMismatch {
                path: path.to_path_buf(),
                expected: expected.clone(),
                actual,
            });
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn sha1_file(path: &Path) -> Result<String> {
    use sha1::Sha1;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn write_sidecar(final_path: &Path, fw: &Firmware) -> Result<()> {
    let sidecar = final_path.with_extension("ipsw.json");
    let json = serde_json::to_vec_pretty(fw)
        .map_err(|e| Error::Download(format!("sidecar serialize: {e}")))?;
    std::fs::write(sidecar, json)?;
    Ok(())
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
    fn file_name_from_url() {
        assert_eq!(fw().file_name(), "UniversalMac_26.5.2_25F84_Restore.ipsw");
    }

    #[test]
    fn file_name_fallback_when_url_has_no_ipsw() {
        let mut f = fw();
        f.url = "https://example.com/redirect".into();
        assert_eq!(f.file_name(), "UniversalMac_26.5.2_25F84_Restore.ipsw");
    }

    #[test]
    fn cache_dir_prefers_env_override() {
        std::env::set_var("APPLERESTORE_CACHE_DIR", "/tmp/ar-test-cache");
        assert_eq!(
            default_cache_dir().unwrap(),
            PathBuf::from("/tmp/ar-test-cache")
        );
        std::env::remove_var("APPLERESTORE_CACHE_DIR");
    }

    #[test]
    fn cache_dir_uses_xdg() {
        std::env::remove_var("APPLERESTORE_CACHE_DIR");
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg");
        assert_eq!(
            default_cache_dir().unwrap(),
            PathBuf::from("/tmp/xdg/applerestore/firmwares")
        );
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}
