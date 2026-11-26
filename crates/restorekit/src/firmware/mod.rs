//! Firmware resolution, caching, and download.
//!
//! [`resolve`] looks up the correct IPSW for a model (ipsw.me, with Apple's mesu
//! feed as a fallback); [`download`] fetches it resumably and verifies its
//! checksum; and [`default_cache_dir`]/[`cached`] manage the on-disk cache with
//! `.json` checksum sidecars. The shared [`Firmware`] type lives here.

mod cache;
mod download;
mod resolve;

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Longest IPSW name from the URL kept as the cache file name.
const MAX_FILE_NAME: usize = 64;

#[cfg(target_os = "linux")]
pub(crate) use cache::data_dir;
pub use cache::{cache_path, cached, default_cache_dir};
pub use download::download;
pub use resolve::resolve;

/// Resolved firmware metadata for a specific model.
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
    /// The OS this firmware installs: "macOS", "bridgeOS" for a T2, "iOS",
    /// "iPadOS", and so on.
    pub fn os_name(&self) -> &'static str {
        const OSES: &[(&str, &str)] = &[
            ("iBridge", "bridgeOS"),
            ("iPhone", "iOS"),
            ("iPod", "iOS"),
            ("iPad", "iPadOS"),
            ("AppleTV", "tvOS"),
            ("Watch", "watchOS"),
            ("AudioAccessory", "audioOS"),
            ("RealityDevice", "visionOS"),
        ];
        OSES.iter()
            .find(|(prefix, _)| self.identifier.starts_with(prefix))
            .map_or("macOS", |(_, os)| os)
    }

    /// Canonical cache filename, derived from the download URL's basename.
    pub fn file_name(&self) -> String {
        let from_url = self.url.rsplit('/').next().filter(|s| s.ends_with(".ipsw"));
        match from_url {
            // bridgeOS IPSWs are named after every T2 board they support (~210
            // chars), which pushes the cache path past Windows' MAX_PATH and
            // makes idevicerestore's ipsw_open fail.
            Some(name) if name.len() > MAX_FILE_NAME => format!(
                "{}_{}_{}_Restore.ipsw",
                self.identifier, self.version, self.build
            ),
            Some(name) => name.to_string(),
            None => format!("UniversalMac_{}_{}_Restore.ipsw", self.version, self.build),
        }
    }
}

/// Shared blocking HTTP client used by resolution and download.
pub(crate) fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("restorekit/", env!("CARGO_PKG_VERSION")))
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
    fn os_names() {
        let os = |identifier: &str| {
            Firmware {
                identifier: identifier.into(),
                ..fw()
            }
            .os_name()
        };
        assert_eq!(os("Mac14,2"), "macOS");
        assert_eq!(os("iBridge2,1"), "bridgeOS");
        assert_eq!(os("iPhone12,8"), "iOS");
        assert_eq!(os("iPad17,1"), "iPadOS");
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
    fn file_name_shortens_bridgeos_names() {
        let mut f = fw();
        f.identifier = "iBridge2,3".into();
        f.version = "11.0".into();
        f.build = "24P6005".into();
        f.url = "https://updates.cdn-apple.com/2026FallFCS/fullrestores/140-55161/E2CF296A-8792-4933-8B38-F810EB1D9EAD/iBridge2,1,iBridge2,10,iBridge2,12,iBridge2,14,iBridge2,15,iBridge2,16,iBridge2,19,iBridge2,20,iBridge2,21,iBridge2,22,iBridge2,3,iBridge2,4,iBridge2,5,iBridge2,6,iBridge2,7,iBridge2,8_11.0_24P6005_Restore.ipsw".into();
        assert_eq!(f.file_name(), "iBridge2,3_11.0_24P6005_Restore.ipsw");
    }
}
