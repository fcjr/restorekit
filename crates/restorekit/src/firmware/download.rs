use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::cache::{self, cache_path};
use super::{http_client, Firmware};
use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// Download (or resume) the firmware into the cache and verify its checksum.
/// Returns the path to the verified file. A cache hit short-circuits the download.
pub fn download(cache_dir: &Path, fw: &Firmware, progress: ProgressFn) -> Result<PathBuf> {
    std::fs::create_dir_all(cache_dir)?;
    let final_path = cache_path(cache_dir, fw);

    if let Some(path) = cache::cached(cache_dir, fw) {
        progress(Event::CacheHit {
            path: path.display().to_string(),
        });
        return Ok(path);
    }

    let partial = final_path.with_extension("ipsw.partial");

    // These files are ~20 GB; a single dropped connection shouldn't fail the
    // whole download. Retry a bounded number of times, resuming from the
    // partial file each time. Only network/transfer errors trigger a retry.
    const MAX_ATTEMPTS: u32 = 8;
    let mut attempt = 0;
    loop {
        attempt += 1;
        match download_stream(&partial, fw, progress) {
            Ok(()) => break,
            Err(e) if attempt < MAX_ATTEMPTS && is_transient(&e) => {
                let received = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
                progress(Event::DownloadResumed { received });
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    progress(Event::Verifying);
    cache::verify(&partial, fw)?;

    std::fs::rename(&partial, &final_path)?;
    cache::write_sidecar(&final_path, fw)?;
    Ok(final_path)
}

/// A single streaming attempt, resuming from whatever is already in `partial`.
fn download_stream(partial: &Path, fw: &Firmware, progress: ProgressFn) -> Result<()> {
    let mut downloaded = std::fs::metadata(partial).map(|m| m.len()).unwrap_or(0);

    let client = http_client()?;
    let mut req = client.get(&fw.url);
    if downloaded > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={downloaded}-"));
    }
    let mut resp = req.send()?.error_for_status()?;

    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if downloaded > 0 && !resuming {
        // Server ignored the range; start over from the beginning.
        downloaded = 0;
    }
    let total = downloaded + resp.content_length().unwrap_or(0);
    let total = if fw.size > 0 { fw.size } else { total };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(resuming)
        .write(true)
        .truncate(!resuming && downloaded == 0)
        .open(partial)?;

    let mut buf = [0u8; 1 << 20];
    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| Error::Download(e.to_string()))?;
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
    Ok(())
}

/// Whether an error is worth retrying with resume (transfer/network issues).
fn is_transient(e: &Error) -> bool {
    match e {
        Error::Download(_) => true,
        Error::Http(inner) => {
            inner.is_timeout() || inner.is_request() || inner.is_body() || inner.is_connect()
        }
        _ => false,
    }
}
