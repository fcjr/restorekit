//! Firmware resolution, caching, and download.
//!
//! Split across submodules: [`resolve`] (ipsw.me + Apple mesu lookup),
//! [`cache`] (cache dir, checksum sidecars), and [`download`] (resumable,
//! verified fetch). The shared [`Firmware`] type and HTTP client live here.

mod cache;
mod download;
mod resolve;

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub use cache::{cache_path, cached, default_cache_dir};
pub use download::download;
pub use resolve::resolve;

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
            .unwrap_or_else(|| format!("UniversalMac_{}_{}_Restore.ipsw", self.version, self.build))
    }
}

/// Shared blocking HTTP client used by resolution and download.
fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("applerestore/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(Error::Http)
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
}
