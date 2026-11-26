//! Embedded usbmuxd server (Linux + Windows).
//!
//! Runs the usbmuxd event loop on a background thread so `idevicerestore` (via
//! libusbmuxd) can reach USB devices for the restore-mode phase without an
//! external usbmuxd daemon. On Linux it listens on a private Unix socket; on
//! Windows on TCP `127.0.0.1:27015`, which is libusbmuxd's default there — so no
//! client-side configuration is needed.

use std::ffi::CString;
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// Where each process's embedded server listens on Linux, before its pid.
#[cfg(target_os = "linux")]
const SOCKET_PREFIX: &str = "/tmp/restorekit-usbmuxd-";

/// Whether a `USBMUXD_SOCKET_ADDRESS` value points at an embedded server, this
/// process's or a parent's [`SharedUsbmuxd`](crate::SharedUsbmuxd).
#[cfg(target_os = "linux")]
pub(crate) fn is_embedded_address(address: &str) -> bool {
    address
        .strip_prefix("UNIX:")
        .is_some_and(|path| path.starts_with(SOCKET_PREFIX))
}

/// The loopback port usbmuxd listens on under Windows.
#[cfg(any(target_os = "windows", test))]
const MUX_PORT: u16 = 27015;

/// Who holds the usbmuxd port on Windows.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq)]
pub enum MuxPort {
    /// Nobody: the embedded server can take it.
    Free,
    /// Apple Mobile Device Service (iTunes, Apple Devices), Apple's own
    /// usbmuxd, which drives the restore-mode interface through Apple's driver.
    Apple,
    /// Some other process, named when we can read it.
    Other { pid: u32, name: Option<String> },
}

/// Find who listens on the usbmuxd port. Apple's service is told apart by its
/// process id from the service manager, so another restorekit's embedded
/// server (or any other program) isn't mistaken for it.
#[cfg(target_os = "windows")]
pub fn mux_port() -> MuxPort {
    match win::listener_pid(MUX_PORT) {
        None => MuxPort::Free,
        Some(pid) if win::apple_service_pid() == Some(pid) => MuxPort::Apple,
        Some(pid) => MuxPort::Other {
            pid,
            name: win::process_name(pid),
        },
    }
}

/// The pid listening on IPv4 `port` (loopback or any address), from the rows
/// of a `MIB_TCPTABLE_OWNER_PID`: six little-endian words each, with the
/// address and port in network byte order.
#[cfg(any(target_os = "windows", test))]
fn listener_in_rows(rows: &[u8], port: u16) -> Option<u32> {
    rows.as_chunks::<24>().0.iter().find_map(|row| {
        let addr = [row[4], row[5], row[6], row[7]];
        let row_port = u16::from_be_bytes([row[8], row[9]]);
        let pid = u32::from_le_bytes([row[20], row[21], row[22], row[23]]);
        (row_port == port && (addr == [127, 0, 0, 1] || addr == [0, 0, 0, 0])).then_some(pid)
    })
}

#[cfg(target_os = "windows")]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_MANAGER_CONNECT,
        SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS_PROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// The service iTunes and Apple Devices install.
    const APPLE_SERVICE: &str = "Apple Mobile Device Service";

    pub(super) fn listener_pid(port: u16) -> Option<u32> {
        let mut size = 0u32;
        let mut buf: Vec<u8> = Vec::new();
        for _ in 0..4 {
            let rc = unsafe {
                GetExtendedTcpTable(
                    buf.as_mut_ptr().cast(),
                    &mut size,
                    0,
                    AF_INET as u32,
                    TCP_TABLE_OWNER_PID_LISTENER,
                    0,
                )
            };
            match rc {
                NO_ERROR => {
                    let count = u32::from_le_bytes(buf.get(..4)?.try_into().ok()?) as usize;
                    let rows = buf.get(4..4 + count * 24)?;
                    return super::listener_in_rows(rows, port);
                }
                ERROR_INSUFFICIENT_BUFFER => buf = vec![0; size as usize],
                _ => return None,
            }
        }
        None
    }

    pub(super) fn apple_service_pid() -> Option<u32> {
        let name: Vec<u16> = std::ffi::OsStr::new(APPLE_SERVICE)
            .encode_wide()
            .chain([0])
            .collect();
        unsafe {
            let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
            if scm.is_null() {
                return None;
            }
            let svc = OpenServiceW(scm, name.as_ptr(), SERVICE_QUERY_STATUS);
            let mut pid = None;
            if !svc.is_null() {
                let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
                let mut needed = 0;
                if QueryServiceStatusEx(
                    svc,
                    SC_STATUS_PROCESS_INFO,
                    (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
                    std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                    &mut needed,
                ) != 0
                    && status.dwCurrentState == SERVICE_RUNNING
                {
                    pid = Some(status.dwProcessId);
                }
                CloseServiceHandle(svc);
            }
            CloseServiceHandle(scm);
            pid
        }
    }

    /// The executable name of `pid`, when this user may query it (not for
    /// services running as SYSTEM, without elevation).
    pub(super) fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(handle);
            if !ok {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit('\\').next().map(str::to_string)
        }
    }
}

/// Another restorekit process serving its own embedded usbmuxd, if any. Its
/// server claims every Mac in restore mode, so a second one would fight it for
/// each of them and either restore could stall.
#[cfg(target_os = "linux")]
pub fn other_embedded_server() -> Option<u32> {
    let own = std::process::id();
    let (dir, prefix) = SOCKET_PREFIX.rsplit_once('/')?;
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name();
        let pid = name
            .to_str()?
            .strip_prefix(prefix)?
            .strip_suffix(".sock")?
            .parse::<u32>()
            .ok()?;
        // A stale socket file from a process that's gone refuses the connect.
        (pid != own && std::os::unix::net::UnixStream::connect(entry.path()).is_ok()).then_some(pid)
    })
}

/// Whether the system usbmuxd is up. It serves running devices, and it claims
/// each device in restore mode.
#[cfg(target_os = "linux")]
pub(crate) fn system_running() -> bool {
    std::os::unix::net::UnixStream::connect("/var/run/usbmuxd").is_ok()
}

/// The embedded server, while any guard holds it. The C server is one per
/// process, so a restore and `recovery::enter` share it rather than each
/// starting their own.
static SERVER: Mutex<Weak<Server>> = Mutex::new(Weak::new());

/// How many callers want running iPhones and iPads switched to their mux
/// configuration (see [`RunningDevices`]).
#[cfg(target_os = "linux")]
static SWITCHERS: Mutex<usize> = Mutex::new(0);

/// Who binds the Windows USB driver when a target enters restore mode.
#[cfg(windows)]
#[derive(Clone, Copy)]
pub enum DriverBinding {
    /// RestoreKit starts its driver watcher, requesting elevation if needed.
    RestoreKit,
    /// The embedding application already manages the restore-mode driver.
    External,
}

/// Holds the embedded usbmuxd server, starting it if no other guard in this
/// process has. The server stops when the last guard drops.
pub struct UsbmuxdGuard {
    server: Option<Arc<Server>>,
}

impl UsbmuxdGuard {
    /// Start the embedded usbmuxd server, or join the one already running in
    /// this process, and point libusbmuxd at it. On Windows starting it also
    /// starts the restore-mode driver watcher, which asks for elevation once
    /// unless the process already has it.
    pub fn start(progress: ProgressFn) -> Result<Self> {
        let mut slot = SERVER.lock().unwrap_or_else(|e| e.into_inner());
        let server = Self::server(&mut slot, progress)?;
        #[cfg(windows)]
        server.watch_restore_mode()?;
        Ok(Self {
            server: Some(server),
        })
    }

    fn server(slot: &mut Weak<Server>, progress: ProgressFn) -> Result<Arc<Server>> {
        if let Some(server) = slot.upgrade() {
            return Ok(server);
        }
        let server = Arc::new(Server::start(progress)?);
        *slot = Arc::downgrade(&server);
        Ok(server)
    }

    /// Configure a Windows restore worker before spawning it. Hold the returned
    /// guard until that worker exits. Overlapping workers share one server.
    /// Apple's listener needs no guard or driver binding.
    #[cfg(windows)]
    pub fn for_worker(
        command: &mut std::process::Command,
        binding: DriverBinding,
        progress: ProgressFn,
    ) -> Result<Option<Self>> {
        let mut slot = SERVER.lock().unwrap_or_else(|e| e.into_inner());
        let guard = if slot.strong_count() == 0 && crate::restore::windows_mux_check()? {
            None
        } else {
            let server = Self::server(&mut slot, progress)?;
            if matches!(binding, DriverBinding::RestoreKit) {
                server.watch_restore_mode()?;
            }
            Some(Self {
                server: Some(server),
            })
        };
        command.env("RESTOREKIT_SHARED_USBMUXD", "1");
        command.env("USBMUXD_SOCKET_ADDRESS", "127.0.0.1:27015");
        Ok(guard)
    }

    /// Where libusbmuxd reaches the server, in `USBMUXD_SOCKET_ADDRESS`'s form.
    #[cfg(target_os = "linux")]
    pub(crate) fn address(&self) -> String {
        let server = self.server.as_ref().expect("held until drop");
        format!("UNIX:{}", server.socket_path.display())
    }
}

impl Drop for UsbmuxdGuard {
    fn drop(&mut self) {
        // Under the lock, so a server that's stopping is never joined or
        // started over.
        let _slot = SERVER.lock().unwrap_or_else(|e| e.into_inner());
        self.server.take();
    }
}

/// Holds the embedded server with running iPhones and iPads switched to the
/// USB configuration with their mux interface, as a standalone usbmuxd does.
/// Without this the server only takes devices whose active configuration
/// already has one, which leaves the host's other Apple devices alone during
/// a restore. Devices switched stay claimed by the server until it stops.
#[cfg(target_os = "linux")]
pub(crate) struct RunningDevices {
    guard: UsbmuxdGuard,
}

#[cfg(target_os = "linux")]
impl RunningDevices {
    pub(crate) fn start() -> Result<Self> {
        let guard = UsbmuxdGuard::start(&mut |_| {})?;
        let mut switchers = SWITCHERS.lock().unwrap_or_else(|e| e.into_inner());
        *switchers += 1;
        restorekit_sys::usbmuxd_set_switch_running(true);
        Ok(Self { guard })
    }

    pub(crate) fn address(&self) -> String {
        self.guard.address()
    }
}

#[cfg(target_os = "linux")]
impl Drop for RunningDevices {
    fn drop(&mut self) {
        let mut switchers = SWITCHERS.lock().unwrap_or_else(|e| e.into_inner());
        *switchers -= 1;
        if *switchers == 0 {
            restorekit_sys::usbmuxd_set_switch_running(false);
        }
    }
}

/// Where the embedded server keeps pair records and its SystemBUID, as the
/// system usbmuxd's own directory is root-only: `lockdown` in restorekit's data
/// directory, next to the default firmware cache. Created here, private to the
/// user since it holds the host's pairing keys; usbmuxd's own mkdir only makes
/// the first missing parent.
#[cfg(target_os = "linux")]
fn pair_record_dir() -> Option<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let dir = crate::firmware::data_dir().ok()?.join("lockdown");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .ok()?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    Some(dir)
}

/// A running embedded usbmuxd server.
struct Server {
    /// The Unix socket file to clean up (Linux only).
    #[cfg(target_os = "linux")]
    socket_path: std::path::PathBuf,
    thread: Option<JoinHandle<()>>,
    /// Binds WinUSB to each restore-mode Mac while the server runs. Apple's
    /// driver claims that interface otherwise, and libusb can't open it.
    #[cfg(target_os = "windows")]
    restore_mode: std::sync::OnceLock<crate::driver::RestoreWatcherGuard>,
}

impl Server {
    // Called under SERVER's lock, including when a caller joins a server
    // whose first worker supplied its own driver binding.
    #[cfg(windows)]
    fn watch_restore_mode(&self) -> Result<()> {
        if self.restore_mode.get().is_some() {
            return Ok(());
        }
        let restore_mode = crate::driver::spawn_restore_mode_watcher().ok_or_else(|| {
            Error::DriverInstall(
                "couldn't start the restore-mode USB helper (was the Windows UAC prompt \
                 declined?). Without it the restore stalls once the Mac enters restore mode; \
                 retry and approve the prompt"
                    .into(),
            )
        })?;
        let _ = self.restore_mode.set(restore_mode);
        Ok(())
    }

    fn start(progress: ProgressFn) -> Result<Self> {
        progress(Event::UsbmuxdStarting);

        // What we hand the C server (Linux: the socket path; Windows: ignored —
        // it binds a fixed loopback port).
        #[cfg(target_os = "linux")]
        let socket_path = {
            let path =
                std::path::PathBuf::from(format!("{SOCKET_PREFIX}{}.sock", std::process::id()));
            let _ = std::fs::remove_file(&path);
            path
        };
        #[cfg(target_os = "linux")]
        let start_arg = socket_path.to_string_lossy().into_owned();
        #[cfg(target_os = "windows")]
        let start_arg = String::from("127.0.0.1:27015");

        // Only pairing needs it, so a restore goes ahead without it.
        #[cfg(target_os = "linux")]
        if let Some(dir) = pair_record_dir()
            .and_then(|d| CString::new(d.into_os_string().into_encoded_bytes()).ok())
        {
            let _ = restorekit_sys::usbmuxd_set_config_dir(&dir);
        }

        let path_c = CString::new(start_arg.as_bytes())
            .map_err(|_| Error::UsbmuxdFailed("socket path contains a NUL byte".into()))?;
        restorekit_sys::usbmuxd_start(&path_c).map_err(|rc| {
            Error::UsbmuxdFailed(format!("restorekit_usbmuxd_start returned {rc}"))
        })?;

        let thread = thread::Builder::new()
            .name("usbmuxd".into())
            .spawn(restorekit_sys::usbmuxd_run)
            .map_err(|e| {
                restorekit_sys::usbmuxd_cleanup();
                Error::UsbmuxdFailed(format!("failed to spawn usbmuxd thread: {e}"))
            })?;

        // Point libusbmuxd (linked into idevicerestore) at our server. On Windows
        // its default is already TCP 127.0.0.1:27015, so nothing to set.
        #[cfg(target_os = "linux")]
        unsafe {
            std::env::set_var(
                "USBMUXD_SOCKET_ADDRESS",
                format!("UNIX:{}", socket_path.display()),
            );
        }

        // Give the listener a moment to come up.
        #[cfg(target_os = "linux")]
        {
            for _ in 0..50 {
                if socket_path.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        #[cfg(target_os = "windows")]
        std::thread::sleep(std::time::Duration::from_millis(200));

        Ok(Self {
            #[cfg(target_os = "linux")]
            socket_path,
            thread: Some(thread),
            #[cfg(target_os = "windows")]
            restore_mode: std::sync::OnceLock::new(),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Signal the event loop to exit, then wait for the thread.
        restorekit_sys::usbmuxd_stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        restorekit_sys::usbmuxd_cleanup();

        #[cfg(target_os = "linux")]
        {
            let _ = std::fs::remove_file(&self.socket_path);
            unsafe {
                std::env::remove_var("USBMUXD_SOCKET_ADDRESS");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn the_embedded_servers_address_is_recognized() {
        let own = format!("UNIX:{SOCKET_PREFIX}{}.sock", std::process::id());
        assert!(is_embedded_address(&own));
        assert!(!is_embedded_address("UNIX:/var/run/usbmuxd"));
        assert!(!is_embedded_address("127.0.0.1:27015"));
    }

    fn row(addr: [u8; 4], port: u16, pid: u32) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&2u32.to_le_bytes()); // MIB_TCP_STATE_LISTEN
        r.extend_from_slice(&addr);
        r.extend_from_slice(&port.to_be_bytes());
        r.extend_from_slice(&[0, 0]);
        r.extend_from_slice(&[0; 8]);
        r.extend_from_slice(&pid.to_le_bytes());
        r
    }

    #[test]
    fn finds_the_loopback_or_wildcard_listener() {
        let mut rows = row([0, 0, 0, 0], 135, 900);
        rows.extend(row([192, 168, 1, 2], MUX_PORT, 1));
        rows.extend(row([127, 0, 0, 1], MUX_PORT, 7760));
        assert_eq!(listener_in_rows(&rows, MUX_PORT), Some(7760));
        assert_eq!(
            listener_in_rows(&row([0, 0, 0, 0], MUX_PORT, 42), MUX_PORT),
            Some(42)
        );
        assert_eq!(
            listener_in_rows(&row([0, 0, 0, 0], 135, 900), MUX_PORT),
            None
        );
    }
}
