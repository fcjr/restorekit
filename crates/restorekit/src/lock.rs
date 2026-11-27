//! Advisory locks restorekit processes on one host hold on a device, for each
//! other to see. A lock goes with its process, so one that dies leaves none.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;
use std::time::Duration;

/// Another process holds the lock.
#[derive(Debug)]
pub(crate) struct Held;

/// On Unix in /tmp rather than the per-user temporary directory, so a restore
/// run as root and one run as the user see each other's.
fn path(name: &str) -> PathBuf {
    let dir = if cfg!(unix) {
        PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    dir.join(format!("restorekit-{name}.lock"))
}

/// Take the lock `name` until the file is dropped. `None` when its file can't
/// be had (another user's, left behind): there is then no lock to hold.
pub(crate) fn take(name: &str) -> Result<Option<File>, Held> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o666).custom_flags(libc::O_NOFOLLOW);
    }
    let Ok(file) = options.open(path(name)) else {
        return Ok(None);
    };
    // A process checking the lock holds it for a moment.
    let mut tries = 0;
    while let Err(TryLockError::WouldBlock) = file.try_lock() {
        if tries == 20 {
            return Err(Held);
        }
        tries += 1;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(Some(file))
}

/// The file of the lock `name`, when a process holds it. Not for a lock this
/// process holds itself.
pub(crate) fn held(name: &str) -> Option<File> {
    let file = File::open(path(name)).ok()?;
    matches!(file.try_lock_shared(), Err(TryLockError::WouldBlock)).then_some(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_is_held_until_its_file_drops() {
        let name = format!("test-{}", std::process::id());
        assert!(held(&name).is_none());
        let file = take(&name).unwrap().unwrap();
        assert!(held(&name).is_some());
        assert!(take(&name).is_err());
        drop(file);
        assert!(held(&name).is_none());
        std::fs::remove_file(path(&name)).unwrap();
    }
}
