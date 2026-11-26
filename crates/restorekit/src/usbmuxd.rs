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
    /// Apple's usbmuxd, provided by Apple Mobile Device Service or the
    /// Microsoft Store version of Apple Devices.
    Apple,
    /// Some other process, named when we can read it.
    Other { pid: u32, name: Option<String> },
}

/// Find who listens on the usbmuxd port.
#[cfg(target_os = "windows")]
pub fn mux_port() -> MuxPort {
    let Some(pid) = win::listener_pid(MUX_PORT) else {
        return MuxPort::Free;
    };
    if win::apple_service_pid() == Some(pid) {
        return MuxPort::Apple;
    }
    let image = win::image_path(pid);
    if image
        .as_deref()
        .is_some_and(|path| apple_devices_mux(path, win::staged_package_path))
    {
        return MuxPort::Apple;
    }
    MuxPort::Other {
        pid,
        name: image.and_then(|path| path.rsplit('\\').next().map(str::to_owned)),
    }
}

#[cfg(any(target_os = "windows", test))]
fn apple_devices_mux(image: &str, staged_path: impl Fn(&str) -> Option<String>) -> bool {
    let Some((mut dir, exe)) = image.rsplit_once('\\') else {
        return false;
    };
    if !exe.eq_ignore_ascii_case("AppleMobileDeviceProcess.exe") {
        return false;
    }
    while let Some((parent, package)) = dir.rsplit_once('\\') {
        let mut fields = package.split('_');
        if fields
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("AppleInc.AppleDevices"))
            && fields
                .next_back()
                .is_some_and(|id| id.eq_ignore_ascii_case("nzyj5cx40ttqa"))
        {
            return staged_path(package).is_some_and(|path| path.eq_ignore_ascii_case(dir));
        }
        dir = parent;
    }
    false
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

    use windows_sys::Wdk::System::SystemInformation::NtQuerySystemInformation;
    use windows_sys::Win32::Foundation::{
        ERROR_INSUFFICIENT_BUFFER, HANDLE, NO_ERROR, STATUS_INFO_LENGTH_MISMATCH, UNICODE_STRING,
    };
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;
    use windows_sys::Win32::Storage::FileSystem::QueryDosDeviceW;
    use windows_sys::Win32::Storage::Packaging::Appx::GetStagedPackagePathByFullName;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_MANAGER_CONNECT,
        SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS_PROCESS,
    };

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

    /// Query the image by PID. A service account may not be able to open
    /// the Apple Devices process running in the signed-in user's session.
    pub(super) fn image_path(pid: u32) -> Option<String> {
        // SystemProcessIdInformation returns the executable's native NT path.
        const SYSTEM_PROCESS_ID_INFORMATION: i32 = 88;
        #[repr(C)]
        struct ProcessIdInformation {
            process_id: HANDLE,
            image_name: UNICODE_STRING,
        }
        let mut path = vec![0u16; 260];
        loop {
            let mut info = ProcessIdInformation {
                process_id: pid as usize as HANDLE,
                image_name: UNICODE_STRING {
                    Length: 0,
                    MaximumLength: (path.len() * 2).try_into().ok()?,
                    Buffer: path.as_mut_ptr(),
                },
            };
            let status = unsafe {
                NtQuerySystemInformation(
                    SYSTEM_PROCESS_ID_INFORMATION,
                    (&mut info as *mut ProcessIdInformation).cast(),
                    std::mem::size_of_val(&info) as u32,
                    std::ptr::null_mut(),
                )
            };
            if status >= 0 {
                if info.image_name.Length == 0 {
                    return None;
                }
                return String::from_utf16(path.get(..usize::from(info.image_name.Length) / 2)?)
                    .ok();
            }
            let needed = usize::from(info.image_name.MaximumLength) / 2;
            if status != STATUS_INFO_LENGTH_MISMATCH || needed <= path.len() {
                return None;
            }
            path.resize(needed, 0);
        }
    }

    /// The staged path is available even to accounts the app isn't registered for.
    pub(super) fn staged_package_path(package: &str) -> Option<String> {
        let package: Vec<u16> = package.encode_utf16().chain([0]).collect();
        let mut len = 0;
        if unsafe {
            GetStagedPackagePathByFullName(package.as_ptr(), &mut len, std::ptr::null_mut())
        } != ERROR_INSUFFICIENT_BUFFER
        {
            return None;
        }
        let mut path = vec![0u16; len as usize];
        if unsafe { GetStagedPackagePathByFullName(package.as_ptr(), &mut len, path.as_mut_ptr()) }
            != NO_ERROR
        {
            return None;
        }

        // Put the drive-letter path in the same namespace as the process image.
        let path = path.get(..(len as usize).checked_sub(1)?)?;
        if path.get(1..3)? != [b':' as u16, b'\\' as u16] {
            return None;
        }
        let drive = [path[0], path[1], 0];
        let mut device = [0; 512];
        let len =
            unsafe { QueryDosDeviceW(drive.as_ptr(), device.as_mut_ptr(), device.len() as u32) };
        let end = device[..len as usize].iter().position(|&c| c == 0)?;
        let mut native = String::from_utf16(&device[..end]).ok()?;
        native.push_str(&String::from_utf16(&path[2..]).ok()?);
        Some(native)
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
        let server = match slot.upgrade() {
            Some(server) => server,
            None => {
                let server = Arc::new(Server::start(progress)?);
                *slot = Arc::downgrade(&server);
                server
            }
        };
        Ok(Self {
            server: Some(server),
        })
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
    _restore_mode: crate::driver::RestoreWatcherGuard,
}

impl Server {
    fn start(progress: ProgressFn) -> Result<Self> {
        progress(Event::UsbmuxdStarting);

        // Without the watcher a restore runs for minutes and then stalls
        // waiting for restore mode, so stop now instead.
        #[cfg(target_os = "windows")]
        let restore_mode = crate::driver::spawn_restore_mode_watcher().ok_or_else(|| {
            Error::DriverInstall(
                "couldn't start the restore-mode USB helper (was the Windows UAC prompt \
                 declined?). Without it the restore stalls once the Mac enters restore mode; \
                 retry and approve the prompt"
                    .into(),
            )
        })?;

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
            .map_err(|e| Error::UsbmuxdFailed(format!("failed to spawn usbmuxd thread: {e}")))?;

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
            _restore_mode: restore_mode,
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
    fn recognizes_the_mux_only_in_the_installed_apple_devices_package() {
        let package = "AppleInc.AppleDevices_1.1792.39682.0_x64__nzyj5cx40ttqa";
        let root = format!(r"\Device\HarddiskVolume3\Program Files\WindowsApps\{package}");
        let exe = "AppleMobileDeviceProcess.exe";
        let check =
            |image: &str| apple_devices_mux(image, |p| (p == package).then(|| root.clone()));
        assert!(check(&format!(r"{root}\{exe}")));
        assert!(check(&format!(r"{root}\AMDS\{exe}")));
        assert!(!check(&format!(r"{root}\AppleDevices.exe")));
        assert!(!check(&format!(
            r"\Device\HarddiskVolume3\Users\User\{package}\{exe}"
        )));
        assert!(!check(&format!(r"{root}extra\{exe}")));
        assert!(!apple_devices_mux(&format!(r"{root}\{exe}"), |_| None));
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
