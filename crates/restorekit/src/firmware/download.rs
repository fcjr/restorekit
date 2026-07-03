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

    // Only one download of a given firmware at a time: a second process
    // appending to the same partial file would corrupt it. Hold an advisory lock
    // for the whole download — the OS releases it automatically if we exit or
    // crash, so a stale lock can't wedge future downloads.
    let lock_path = final_path.with_extension("ipsw.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Err(Error::Download(
                "a download for this firmware is already in progress".into(),
            ));
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
    }
    // `lock` stays in scope until this function returns, keeping the lock held.

    // A truncated or corrupt partial only surfaces at the checksum. Verify after
    // downloading; if it fails, discard the partial and re-download from scratch
    // a bounded number of times before giving up.
    const MAX_VERIFY_RETRIES: u32 = 2;
    let mut verify_attempt = 0;
    loop {
        fetch_to_partial(&partial, fw, progress)?;

        progress(Event::Verifying);
        match cache::verify(&partial, fw) {
            Ok(()) => break,
            Err(e) => {
                std::fs::remove_file(&partial).ok();
                if verify_attempt >= MAX_VERIFY_RETRIES {
                    return Err(e);
                }
                verify_attempt += 1;
            }
        }
    }

    std::fs::rename(&partial, &final_path)?;
    cache::write_sidecar(&final_path, fw)?;
    Ok(final_path)
}

/// Download the whole file into `partial`, resuming on transient errors. These
/// files are ~15-20 GB, so a single dropped connection shouldn't fail the whole
/// download. Keeps going as long as attempts make progress; gives up only after
/// several consecutive stalls (no bytes gained).
fn fetch_to_partial(partial: &Path, fw: &Firmware, progress: ProgressFn) -> Result<()> {
    const MAX_STALLS: u32 = 8;
    let mut stalls = 0;
    let mut last_size = std::fs::metadata(partial).map(|m| m.len()).unwrap_or(0);
    loop {
        match download_stream(partial, fw, progress) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) => {
                let received = std::fs::metadata(partial).map(|m| m.len()).unwrap_or(0);
                if received > last_size {
                    stalls = 0; // made progress — keep resuming
                    last_size = received;
                } else {
                    stalls += 1;
                    if stalls >= MAX_STALLS {
                        return Err(e);
                    }
                }
                progress(Event::DownloadResumed { received });
            }
            Err(e) => return Err(e),
        }
    }
}

/// A single streaming attempt, resuming from whatever is already in `partial`.
fn download_stream(partial: &Path, fw: &Firmware, progress: ProgressFn) -> Result<()> {
    let mut downloaded = std::fs::metadata(partial).map(|m| m.len()).unwrap_or(0);

    let client = http_client()?;
    let mut req = client.get(&fw.url);
    if downloaded > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={downloaded}-"));
    }
    let resp = req.send()?;

    // A 416 means the partial is already at least as large as the resource — it
    // may be a complete download that wasn't verified/renamed yet. Don't discard
    // it; return and let the caller's checksum verify judge (it re-fetches from
    // scratch if the bytes are actually wrong).
    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && downloaded > 0 {
        return Ok(());
    }

    let mut resp = resp.error_for_status()?;

    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if downloaded > 0 && !resuming {
        // Server ignored the range; start over from the beginning.
        downloaded = 0;
    }
    // Bytes this response will deliver — authoritative, unlike the advisory
    // firmware size. For a 206 resume it's the remaining count, so the true total
    // is the current offset plus it.
    let content_len = resp.content_length();
    let total = match content_len {
        Some(c) => downloaded + c,
        None if fw.size > 0 => fw.size,
        None => downloaded,
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(resuming)
        .write(true)
        .truncate(!resuming && downloaded == 0)
        .open(partial)?;

    let mut buf = [0u8; 1 << 20];
    let mut got = 0u64;
    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| Error::Download(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        got += n as u64;
        progress(Event::DownloadProgress {
            received: downloaded,
            total,
        });
    }
    file.flush()?;

    // Judge completion by the server's Content-Length for this response. Fewer
    // bytes than promised means the connection closed early — signal a resumable
    // error rather than returning a truncated "success" that fails the checksum.
    if let Some(len) = content_len {
        if got < len {
            return Err(Error::Download(format!(
                "connection closed early: got {got} of {len} bytes"
            )));
        }
    }
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
