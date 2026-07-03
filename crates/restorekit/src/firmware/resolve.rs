use serde::Deserialize;

use super::{http_client, Firmware};
use crate::error::{Error, Result};

const IPSW_API: &str = "https://api.ipsw.me/v4";
const MESU_FEED: &str =
    "https://mesu.apple.com/assets/macos/com_apple_macOSIPSW/com_apple_macOSIPSW.xml";

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
        let f = candidates
            .into_iter()
            .next()
            .ok_or_else(|| Error::NoFirmwareFound {
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
