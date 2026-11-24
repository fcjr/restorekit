//! WinUSB setup for Apple DFU/recovery/restore devices and RecoverKit dongles
//! (Windows only).
//!
//! On Windows, libusb (and `nusb`) can only *open* a device that has WinUSB
//! bound to it and a device interface GUID registered for it. Apple's own
//! drivers (Windows Update installs them on first plug-in) serve DFU, recovery
//! and KIS directly through libirecovery, but not restore mode, where Apple's
//! WinUSB variant can't be opened by libusb; and a machine without them has no
//! driver for those modes at all.
//!
//! Rather than shipping a driver package (which Windows only loads when signed,
//! and on ARM only when Microsoft signed it), restorekit binds Windows' own
//! `winusb.inf` "WinUsb Device" driver to the device instance, the way Device
//! Manager's "Have Disk / show all devices" route does, and writes the
//! interface GUID into the device's registry key the way an INF's `AddReg`
//! would. Both need administrator rights; the callers elevate first.

use restorekit_dongle_proto as proto;

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// Apple's USB vendor id.
pub const APPLE_VID: u16 = 0x05ac;

/// The DFU / recovery / WTF / KIS product ids the C stack (libirecovery) drives
/// — the modes a Mac passes through while being restored.
///
/// Deliberately NOT Apple's normal-mode range (0x129x): on Windows, binding
/// WinUSB *replaces* a device's driver, so covering normal mode would hijack a
/// plugged-in iPhone/iPad from Apple's own driver. Every id here is only ever an
/// Apple device in recovery/DFU, never a normal one.
pub const DRIVER_PIDS: &[u16] = &[
    0x1222, // WTF
    0x1227, // DFU
    0x1280, 0x1281, 0x1282, 0x1283, // recovery 1-4
    0xf014, // port DFU
    0x1881, // KIS
];

/// Apple's normal-mode product-id range (0x1290–0x12af). A Mac in **restore
/// mode** enumerates as a composite device with one of these ids plus a
/// `RESTORE_MODE` qualifier; we bind WinUSB to that qualified interface only,
/// never the bare id, so a normal iPhone/iPad is untouched.
const RESTORE_MODE_PIDS: std::ops::RangeInclusive<u16> = 0x1290..=0x12af;

/// Device interface GUIDs, per mode. The DFU, recovery, port DFU and KIS ones
/// are what Apple's drivers register, which libirecovery's Windows backend
/// enumerates; the restore-mode one is ours, for libusb.
const GUID_DFU: &str = "{B8085869-FEB9-404B-8CB1-1E5C14FA8C54}";
const GUID_IBOOT: &str = "{ED82A167-D61A-4AF6-9AB6-11E52236C576}";
const GUID_PORTDFU: &str = "{AF633FF1-1170-4CA6-AE9E-08D001421EAA}";
const GUID_KIS: &str = "{B36F4137-F4EF-4BFC-A25A-C2410768EE37}";
const GUID_RESTORE: &str = "{3F966BD9-FA04-4EC5-991C-D326973B5128}";

/// Does `pid` name a mode restorekit binds WinUSB for?
pub fn is_target_pid(pid: u16) -> bool {
    DRIVER_PIDS.contains(&pid)
}

/// A connected Apple device that needs WinUSB bound before restorekit can use it.
#[derive(Debug, Clone)]
pub struct Target {
    pub pid: u16,
    /// Human-readable label (product string, or a mode id as a fallback).
    pub name: String,
}

/// Enumerate connected Apple devices in DFU, recovery, or restore mode.
pub fn connected_targets() -> Result<Vec<Target>> {
    Ok(present_instance_ids()?
        .iter()
        .filter_map(|id| target_pid_from_instance_id(id))
        .map(|pid| Target {
            pid,
            name: format!("Apple device (mode {pid:#06x})"),
        })
        .collect())
}

/// The PnP hardware id of a dongle's vendor interface.
fn dongle_interface_hwid() -> String {
    format!(
        "USB\\VID_{:04X}&PID_{:04X}&MI_{:02X}",
        proto::VID,
        proto::PID,
        proto::VENDOR_IFACE
    )
}

fn is_dongle_interface(instance_id: &str) -> bool {
    instance_id
        .to_ascii_uppercase()
        .starts_with(&format!("{}\\", dongle_interface_hwid()))
}

/// Instance ids of every present device.
fn present_instance_ids() -> Result<Vec<String>> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    unsafe {
        let devices = SetupDiGetClassDevsW(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_ALLCLASSES | DIGCF_PRESENT,
        );
        if devices == INVALID_HANDLE_VALUE as isize {
            return Err(Error::DriverInstall(
                "could not enumerate Windows devices".into(),
            ));
        }

        let mut ids = Vec::new();
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(devices, index, &mut info) == 0 {
                break;
            }
            index += 1;

            let mut instance_id = [0u16; 512];
            let mut required = 0;
            if SetupDiGetDeviceInstanceIdW(
                devices,
                &info,
                instance_id.as_mut_ptr(),
                instance_id.len() as u32,
                &mut required,
            ) == 0
            {
                continue;
            }
            let len = instance_id
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(required as usize);
            ids.push(String::from_utf16_lossy(&instance_id[..len]));
        }
        SetupDiDestroyDeviceInfoList(devices);
        Ok(ids)
    }
}

fn target_pid_from_instance_id(instance_id: &str) -> Option<u16> {
    let upper = instance_id.to_ascii_uppercase();
    let marker = format!("VID_{APPLE_VID:04X}&PID_");
    let start = upper.find(&marker)? + marker.len();
    let pid = u16::from_str_radix(upper.get(start..start + 4)?, 16).ok()?;
    is_target_pid(pid).then_some(pid)
}

/// The interface GUID restorekit binds `instance_id` with, if it's a device
/// restorekit drives: a DFU-family or recovery device itself (not one of its
/// interfaces), a restore-mode data interface, or a dongle's vendor interface.
fn bind_guid(instance_id: &str) -> Option<&'static str> {
    if is_dongle_interface(instance_id) {
        return Some(proto::WINUSB_INTERFACE_GUID);
    }
    let upper = instance_id.to_ascii_uppercase();
    let rest = upper.strip_prefix(&format!("USB\\VID_{APPLE_VID:04X}&PID_"))?;
    let pid = u16::from_str_radix(rest.get(..4)?, 16).ok()?;
    let qualifier = rest.get(4..)?;
    if qualifier.starts_with("&RESTORE_MODE&MI_00\\") {
        return RESTORE_MODE_PIDS.contains(&pid).then_some(GUID_RESTORE);
    }
    if !qualifier.starts_with('\\') {
        return None;
    }
    match pid {
        0x1222 | 0x1227 => Some(GUID_DFU),
        0x1280..=0x1283 => Some(GUID_IBOOT),
        0xf014 => Some(GUID_PORTDFU),
        0x1881 => Some(GUID_KIS),
        _ => None,
    }
}

/// Make every connected Apple DFU/recovery device and RecoverKit dongle
/// reachable, binding WinUSB where needed. Devices on Apple's own drivers are
/// left alone: libirecovery talks to those directly, and moving a KIS Mac off
/// AppleKIS strands it until it re-enters DFU. Returns the number of devices
/// set up.
///
/// Must run elevated (the CLI handles that).
pub fn install_winusb(progress: ProgressFn) -> Result<usize> {
    let devices: Vec<(String, &str)> = present_instance_ids()?
        .into_iter()
        .filter_map(|id| bind_guid(&id).map(|g| (id, g)))
        .filter(|(id, _)| !id.to_ascii_uppercase().contains("&RESTORE_MODE&"))
        .collect();
    if devices.is_empty() {
        return Err(Error::DriverInstall(
            "no Apple device in DFU or recovery mode and no RecoverKit dongle is connected — \
             plug in the dongle or put the target into DFU (see `restorekit status`) and re-run"
                .into(),
        ));
    }

    progress(Event::DriverSetupStarting);
    let mut failures = Vec::new();
    for (id, guid) in &devices {
        let dongle = is_dongle_interface(id);
        let result = if !dongle && winusb::driver_of(id).is_some_and(|d| d.is_apple()) {
            Ok(())
        } else {
            winusb::bind(id, guid)
        };
        match result {
            Ok(()) => progress(Event::DriverBound {
                name: if dongle {
                    "RecoverKit dongle".into()
                } else {
                    target_pid_from_instance_id(id)
                        .map(|pid| format!("Apple device (mode {pid:#06x})"))
                        .unwrap_or_else(|| id.clone())
                },
            }),
            Err(e) => failures.push(format!("{id}: {e}")),
        }
    }
    if !failures.is_empty() {
        return Err(Error::DriverInstall(format!(
            "WinUSB setup did not complete: {}",
            failures.join("; ")
        )));
    }
    Ok(devices.len())
}

/// Binding Windows' own WinUSB to one device instance.
mod winusb {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DiInstallDevice, SetupDiBuildDriverInfoList, SetupDiCallClassInstaller,
        SetupDiCreateDevRegKeyW, SetupDiCreateDeviceInfoList, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDriverInfoW, SetupDiGetDeviceInstallParamsW, SetupDiGetDriverInfoDetailW,
        SetupDiOpenDevRegKey, SetupDiOpenDeviceInfoW, SetupDiSetClassInstallParamsW,
        SetupDiSetDeviceInstallParamsW, DICS_FLAG_GLOBAL, DICS_PROPCHANGE, DIF_PROPERTYCHANGE,
        DIREG_DEV, DIREG_DRV, DI_ENUMSINGLEINF, DI_FLAGSEX_ALLOWEXCLUDEDDRVS, HDEVINFO,
        SPDIT_CLASSDRIVER, SP_CLASSINSTALL_HEADER, SP_DEVINFO_DATA, SP_DEVINSTALL_PARAMS_W,
        SP_DRVINFO_DATA_V2_W, SP_DRVINFO_DETAIL_DATA_W, SP_PROPCHANGE_PARAMS,
    };
    use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegQueryValueExW, RegSetValueExW, HKEY, KEY_READ, REG_MULTI_SZ,
    };
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;

    use crate::error::{Error, Result};

    /// `{88BAE032-5A81-49F0-BC3D-A4FF138216D6}`, the USBDevice setup class of
    /// `winusb.inf`.
    const USBDEVICE_CLASS: windows_sys::core::GUID =
        windows_sys::core::GUID::from_u128(0x88bae032_5a81_49f0_bc3d_a4ff138216d6);
    const WINUSB_HWID: &str = "USB\\MS_COMP_WINUSB";

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain([0]).collect()
    }

    fn err(what: &str) -> Error {
        Error::DriverInstall(format!("{what} failed: {:#x}", unsafe { GetLastError() }))
    }

    /// The driver currently installed on a device.
    pub(super) struct Driver {
        inf: String,
        provider: String,
    }

    impl Driver {
        pub(super) fn is_apple(&self) -> bool {
            self.provider.starts_with("Apple")
        }

        /// Plain WinUSB: Windows' own `winusb.inf`, or an older restorekit's
        /// package. (Apple's restore-mode driver is WinUSB too, but a variant
        /// libusb can't open.)
        fn is_plain_winusb(&self) -> bool {
            self.inf.eq_ignore_ascii_case("winusb.inf") || self.provider == "RestoreKit"
        }
    }

    /// An open device-info set holding one device.
    struct Device {
        set: HDEVINFO,
        data: SP_DEVINFO_DATA,
    }

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe { SetupDiDestroyDeviceInfoList(self.set) };
        }
    }

    impl Device {
        fn open(instance_id: &str) -> Result<Device> {
            unsafe {
                let set = SetupDiCreateDeviceInfoList(null(), null_mut());
                if set == INVALID_HANDLE_VALUE as HDEVINFO {
                    return Err(err("SetupDiCreateDeviceInfoList"));
                }
                let mut dev = Device {
                    set,
                    data: SP_DEVINFO_DATA {
                        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                        ..Default::default()
                    },
                };
                if SetupDiOpenDeviceInfoW(
                    set,
                    wide(instance_id).as_ptr(),
                    null_mut(),
                    0,
                    &mut dev.data,
                ) == 0
                {
                    return Err(err("SetupDiOpenDeviceInfo"));
                }
                Ok(dev)
            }
        }

        fn reg_key(&self, which: u32) -> Option<HKEY> {
            let key = unsafe {
                SetupDiOpenDevRegKey(self.set, &self.data, DICS_FLAG_GLOBAL, 0, which, KEY_READ)
            };
            (key != INVALID_HANDLE_VALUE as HKEY).then_some(key)
        }

        fn driver(&self) -> Option<Driver> {
            let key = self.reg_key(DIREG_DRV)?;
            let inf = read_string(key, "InfPath");
            let provider = read_string(key, "ProviderName");
            unsafe { RegCloseKey(key) };
            Some(Driver {
                inf: inf.unwrap_or_default(),
                provider: provider.unwrap_or_default(),
            })
        }

        fn interface_guids(&self) -> Option<String> {
            let key = self.reg_key(DIREG_DEV)?;
            let guids = read_string(key, "DeviceInterfaceGUIDs");
            unsafe { RegCloseKey(key) };
            guids
        }

        fn set_interface_guid(&self, guid: &str) -> Result<()> {
            unsafe {
                let key = SetupDiCreateDevRegKeyW(
                    self.set,
                    &self.data,
                    DICS_FLAG_GLOBAL,
                    0,
                    DIREG_DEV,
                    null_mut(),
                    null(),
                );
                if key == INVALID_HANDLE_VALUE as HKEY {
                    return Err(err("opening the device's registry key"));
                }
                // REG_MULTI_SZ: the string, its NUL, and the list's closing NUL.
                let mut value = wide(guid);
                value.push(0);
                let rc = RegSetValueExW(
                    key,
                    wide("DeviceInterfaceGUIDs").as_ptr(),
                    0,
                    REG_MULTI_SZ,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                );
                RegCloseKey(key);
                if rc != 0 {
                    return Err(Error::DriverInstall(format!(
                        "writing DeviceInterfaceGUIDs failed: {rc:#x}"
                    )));
                }
                Ok(())
            }
        }

        /// Restart the device so its driver re-reads its settings.
        fn restart(&self) -> Result<()> {
            unsafe {
                let params = SP_PROPCHANGE_PARAMS {
                    ClassInstallHeader: SP_CLASSINSTALL_HEADER {
                        cbSize: std::mem::size_of::<SP_CLASSINSTALL_HEADER>() as u32,
                        InstallFunction: DIF_PROPERTYCHANGE,
                    },
                    StateChange: DICS_PROPCHANGE,
                    Scope: DICS_FLAG_GLOBAL,
                    HwProfile: 0,
                };
                if SetupDiSetClassInstallParamsW(
                    self.set,
                    &self.data,
                    &params.ClassInstallHeader,
                    std::mem::size_of::<SP_PROPCHANGE_PARAMS>() as u32,
                ) == 0
                    || SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, self.set, &self.data) == 0
                {
                    return Err(err("restarting the device"));
                }
                Ok(())
            }
        }

        /// Install `winusb.inf`'s "WinUsb Device" on this device. Its hardware
        /// id (`USB\MS_COMP_WINUSB`) doesn't match the device, so allow
        /// non-matching drivers and offer only that one INF.
        fn install_winusb(&mut self) -> Result<()> {
            unsafe {
                let mut params = SP_DEVINSTALL_PARAMS_W {
                    cbSize: std::mem::size_of::<SP_DEVINSTALL_PARAMS_W>() as u32,
                    ..std::mem::zeroed()
                };
                if SetupDiGetDeviceInstallParamsW(self.set, &self.data, &mut params) == 0 {
                    return Err(err("SetupDiGetDeviceInstallParams"));
                }
                let n = GetWindowsDirectoryW(params.DriverPath.as_mut_ptr(), 200) as usize;
                if n == 0 || n >= 200 {
                    return Err(err("GetWindowsDirectory"));
                }
                for (i, c) in "\\INF\\winusb.inf".encode_utf16().chain([0]).enumerate() {
                    params.DriverPath[n + i] = c;
                }
                params.Flags |= DI_ENUMSINGLEINF;
                params.FlagsEx |= DI_FLAGSEX_ALLOWEXCLUDEDDRVS;
                if SetupDiSetDeviceInstallParamsW(self.set, &self.data, &params) == 0 {
                    return Err(err("SetupDiSetDeviceInstallParams"));
                }
                self.data.ClassGuid = USBDEVICE_CLASS;
                if SetupDiBuildDriverInfoList(self.set, &mut self.data, SPDIT_CLASSDRIVER) == 0 {
                    return Err(err("SetupDiBuildDriverInfoList"));
                }
                let mut node = SP_DRVINFO_DATA_V2_W {
                    cbSize: std::mem::size_of::<SP_DRVINFO_DATA_V2_W>() as u32,
                    ..std::mem::zeroed()
                };
                let mut found = false;
                let mut index = 0;
                while SetupDiEnumDriverInfoW(
                    self.set,
                    &self.data,
                    SPDIT_CLASSDRIVER,
                    index,
                    &mut node,
                ) != 0
                {
                    index += 1;
                    // The detail struct ends in a variable-length hardware id.
                    let mut buf = [0u64; 512];
                    let detail = buf.as_mut_ptr().cast::<SP_DRVINFO_DETAIL_DATA_W>();
                    (*detail).cbSize = std::mem::size_of::<SP_DRVINFO_DETAIL_DATA_W>() as u32;
                    if SetupDiGetDriverInfoDetailW(
                        self.set,
                        &self.data,
                        &node,
                        detail,
                        std::mem::size_of_val(&buf) as u32,
                        null_mut(),
                    ) == 0
                    {
                        continue;
                    }
                    let hwid = (*detail).HardwareID.as_ptr();
                    let len = (0..).take_while(|&i| *hwid.add(i) != 0).count();
                    let hwid = String::from_utf16_lossy(std::slice::from_raw_parts(hwid, len));
                    if hwid.eq_ignore_ascii_case(WINUSB_HWID) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(Error::DriverInstall(
                        "Windows' winusb.inf has no WinUsb Device entry".into(),
                    ));
                }
                let mut reboot = 0;
                if DiInstallDevice(null_mut(), self.set, &self.data, &node, 0, &mut reboot) == 0 {
                    return Err(err("installing WinUSB"));
                }
                Ok(())
            }
        }
    }

    fn read_string(key: HKEY, name: &str) -> Option<String> {
        let name = wide(name);
        let mut buf = [0u16; 512];
        let mut size = std::mem::size_of_val(&buf) as u32;
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                null(),
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc != 0 {
            return None;
        }
        let chars = &buf[..(size as usize / 2).min(buf.len())];
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        Some(String::from_utf16_lossy(&chars[..end]))
    }

    /// The driver installed on `instance_id`, if any.
    pub(super) fn driver_of(instance_id: &str) -> Option<Driver> {
        Device::open(instance_id).ok()?.driver()
    }

    /// Whether `instance_id` already has plain WinUSB with `guid` registered.
    pub(super) fn is_bound(instance_id: &str, guid: &str) -> bool {
        Device::open(instance_id).is_ok_and(|d| {
            d.driver().is_some_and(|drv| drv.is_plain_winusb())
                && d.interface_guids()
                    .is_some_and(|g| g.eq_ignore_ascii_case(guid))
        })
    }

    /// Give `instance_id` plain WinUSB with `guid` as its interface GUID:
    /// write the GUID, then install Windows' WinUSB, or restart the device if
    /// it already has it so WinUSB registers the GUID.
    pub(super) fn bind(instance_id: &str, guid: &str) -> Result<()> {
        let mut dev = Device::open(instance_id)?;
        let plain = dev.driver().is_some_and(|d| d.is_plain_winusb());
        let has_guid = dev
            .interface_guids()
            .is_some_and(|g| g.eq_ignore_ascii_case(guid));
        if plain && has_guid {
            return Ok(());
        }
        dev.set_interface_guid(guid)?;
        if plain {
            dev.restart()
        } else {
            dev.install_winusb()
        }
    }
}

// ── Restore-mode driver watcher ──────────────────────────────────────────────

/// Internal argument that reruns *this* binary as the elevated restore-mode
/// driver watcher, followed by the liveness-file path. Both the CLI and the
/// desktop app recognize it in `main`, so the library can relaunch whichever
/// binary is running — and the UAC prompt then shows restorekit, not PowerShell.
pub const RESTORE_WATCH_ARG: &str = "--rk-bind-restore-mode";

/// RAII guard for the elevated restore-mode driver watcher. Dropping it removes
/// the liveness file, which is the watcher's signal to exit.
pub struct RestoreWatcherGuard {
    liveness: std::path::PathBuf,
}

impl Drop for RestoreWatcherGuard {
    fn drop(&mut self) {
        // Removing the liveness file tells the watcher to stop.
        let _ = std::fs::remove_file(&self.liveness);
    }
}

/// Spawn an elevated background watcher that binds WinUSB to the Mac's
/// restore-mode interface the moment it appears.
///
/// In restore mode the Mac becomes a USB composite whose data interface
/// (`…&RESTORE_MODE&MI_00`) is claimed by Apple's `appleusb.inf`, whose WinUSB
/// variant libusb can't open (`winusbx_open` → ERROR_NOT_SUPPORTED). Windows
/// re-installs Apple's driver each time the Mac enters restore mode, so the
/// binding has to happen then, which needs admin: the restore relaunches this
/// binary elevated (one UAC) to run [`run_restore_mode_watch_worker`].
///
/// Returns `None` if the watcher couldn't be launched (e.g. UAC declined). The
/// watcher runs until the returned guard is dropped.
pub fn spawn_restore_mode_watcher() -> Option<RestoreWatcherGuard> {
    let liveness = std::env::temp_dir().join(format!(
        "restorekit-restore-live-{}.tmp",
        std::process::id()
    ));
    // The watcher reads our pid from it, to stop if we exit without removing it.
    if std::fs::write(&liveness, std::process::id().to_string()).is_err() {
        return None;
    }

    // Already admin (e.g. launched from an elevated shell or the app already
    // elevated)? Skip UAC entirely — run the watcher inline on a background
    // thread. Otherwise relaunch this binary elevated (one prompt, shown as
    // restorekit).
    if is_elevated() {
        let live = liveness.clone();
        std::thread::spawn(move || run_restore_mode_watch_worker(&live));
        return Some(RestoreWatcherGuard { liveness });
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => {
            let _ = std::fs::remove_file(&liveness);
            return None;
        }
    };
    let params = format!("{RESTORE_WATCH_ARG} \"{}\"", liveness.display());
    if launch_elevated_hidden(&exe.to_string_lossy(), &params).is_err() {
        let _ = std::fs::remove_file(&liveness);
        return None;
    }

    Some(RestoreWatcherGuard { liveness })
}

/// Is this process running with an elevated (administrator) token?
fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Where the restore-mode watcher logs what it did, for diagnosing a restore
/// that stalls at "waiting for device to enter restore mode".
pub fn restore_watch_log() -> std::path::PathBuf {
    std::env::temp_dir().join("restorekit-restore-watch.log")
}

/// The elevated watcher worker, invoked when this binary is rerun with
/// [`RESTORE_WATCH_ARG`]. Until the liveness file is removed or the process
/// that wrote it exits, it binds WinUSB to:
///
/// - the restore-mode data interface whenever it appears, and
/// - a DFU, recovery or KIS device that has had no driver for a few seconds
///   (a machine without Apple's drivers, where Windows Update hasn't run):
///   the Mac changes modes during a restore, and each mode is a new device.
///
/// It keeps watching after a Mac leaves restore mode, since one watcher can
/// serve every restore a shared usbmuxd runs. Logs what it did to
/// [`restore_watch_log`].
pub fn run_restore_mode_watch_worker(liveness: &std::path::Path) {
    use std::collections::HashMap;
    use std::io::Write;
    use std::time::{Duration, Instant};

    const DRIVERLESS_GRACE: Duration = Duration::from_secs(5);
    let owner = OwnerProcess::from_liveness(liveness);
    let mut log = std::fs::File::create(restore_watch_log()).ok();
    let mut note = |msg: String| {
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "{msg}\r");
        }
    };
    note("watching for restore mode".into());

    let mut driverless_since: HashMap<String, Instant> = HashMap::new();
    while liveness.exists() && owner.alive() {
        let ids = present_instance_ids().unwrap_or_default();
        driverless_since.retain(|id, _| ids.contains(id));
        for id in &ids {
            let Some(guid) = bind_guid(id) else { continue };
            if is_dongle_interface(id) || winusb::is_bound(id, guid) {
                continue;
            }
            let restore_mode = guid == GUID_RESTORE;
            if !restore_mode {
                if winusb::driver_of(id).is_some() {
                    continue;
                }
                let since = *driverless_since
                    .entry(id.clone())
                    .or_insert_with(Instant::now);
                if since.elapsed() < DRIVERLESS_GRACE {
                    continue;
                }
            }
            match winusb::bind(id, guid) {
                Ok(()) => note(format!("bound WinUSB to {id}")),
                Err(e) => note(format!("binding {id} failed: {e}")),
            }
        }
        std::thread::sleep(Duration::from_millis(350));
    }
    if !liveness.exists() {
        note("restore ended".into());
    } else {
        note("owner exited".into());
    }
}

/// The process that started a watcher, named by the pid in its liveness file.
/// The handle is opened once, so a later process reusing the pid isn't taken
/// for it, and a process created after the file was written is taken for a
/// reuse. Without a readable pid it falls back to a fixed deadline.
enum OwnerProcess {
    Unknown(std::time::Instant),
    Gone,
    Running(windows_sys::Win32::Foundation::HANDLE),
}

impl OwnerProcess {
    fn from_liveness(liveness: &std::path::Path) -> Self {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };

        let Some(pid) = std::fs::read_to_string(liveness)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        else {
            return Self::Unknown(
                std::time::Instant::now() + std::time::Duration::from_secs(45 * 60),
            );
        };
        let written = std::fs::metadata(liveness).and_then(|m| m.modified()).ok();
        unsafe {
            let handle = OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            );
            if handle.is_null() {
                return Self::Gone;
            }
            let zero = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
            if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                if let (Some(created), Some(written)) = (filetime_to_system(created), written) {
                    if created > written {
                        windows_sys::Win32::Foundation::CloseHandle(handle);
                        return Self::Gone;
                    }
                }
            }
            Self::Running(handle)
        }
    }

    fn alive(&self) -> bool {
        use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        match self {
            Self::Unknown(deadline) => std::time::Instant::now() < *deadline,
            Self::Gone => false,
            Self::Running(handle) => unsafe { WaitForSingleObject(*handle, 0) == WAIT_TIMEOUT },
        }
    }
}

impl Drop for OwnerProcess {
    fn drop(&mut self) {
        if let Self::Running(handle) = self {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(*handle) };
        }
    }
}

/// A FILETIME (100 ns ticks since 1601) as a `SystemTime`.
fn filetime_to_system(
    ft: windows_sys::Win32::Foundation::FILETIME,
) -> Option<std::time::SystemTime> {
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    let since_unix = ticks.checked_sub(UNIX_EPOCH_TICKS)?;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(since_unix.saturating_mul(100)))
}

/// Launch `exe params` elevated (UAC) with no visible window, and return without
/// waiting — the caller manages the process's lifetime out of band.
fn launch_elevated_hidden(exe: &str, params: &str) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let verb = wide("runas");
    let file = wide(exe);
    let par = wide(params);

    unsafe {
        let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = par.as_ptr();
        sei.nShow = SW_HIDE;
        if ShellExecuteExW(&mut sei) == 0 {
            return Err(Error::DriverInstall(
                "could not start the restore-mode driver watcher (UAC declined?)".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setupapi_instance_ids_find_unbound_targets() {
        assert_eq!(
            target_pid_from_instance_id("USB\\VID_05AC&PID_1227\\000445E11462001E"),
            Some(0x1227)
        );
        assert_eq!(
            target_pid_from_instance_id("USB\\VID_05AC&PID_1290\\NORMAL_MODE"),
            None
        );
        assert_eq!(
            target_pid_from_instance_id("USB\\VID_1234&PID_1227\\OTHER"),
            None
        );
    }

    #[test]
    fn binds_each_mode_with_its_interface_guid() {
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_1227\\000445E11462001E"),
            Some(GUID_DFU)
        );
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_1281\\SDOM:01_CPID:8132"),
            Some(GUID_IBOOT)
        );
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_F014\\1234"),
            Some(GUID_PORTDFU)
        );
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_1881\\5&111DD99C&0&23"),
            Some(GUID_KIS)
        );
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_12AC&RESTORE_MODE&MI_00\\6&FC6EB5B&0&0000"),
            Some(GUID_RESTORE)
        );
        assert_eq!(
            bind_guid("USB\\VID_16D0&PID_14F0&MI_04\\6&2E2718E1&0&0004"),
            Some(proto::WINUSB_INTERFACE_GUID)
        );
        // Interfaces of a recovery composite, the restore-mode NCM interface,
        // and a normal-mode iPhone are never touched.
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_1281&MI_00\\6&109E202&0&0000"),
            None
        );
        assert_eq!(
            bind_guid("USB\\VID_05AC&PID_12AC&RESTORE_MODE&MI_01\\6&FC6EB5B&0&0001"),
            None
        );
        assert_eq!(bind_guid("USB\\VID_05AC&PID_12A8\\00008101"), None);
        assert_eq!(bind_guid("USB\\VID_1234&PID_1227\\OTHER"), None);
    }

    #[test]
    fn dongle_interface_is_recognized() {
        assert!(is_dongle_interface(
            "USB\\VID_16D0&PID_14F0&MI_04\\6&2E2718E1&0&0004"
        ));
        assert!(!is_dongle_interface(
            "USB\\VID_16D0&PID_14F0&MI_00\\6&2E2718E1&0&0000"
        ));
        assert!(!is_dongle_interface("USB\\VID_16D0&PID_14F0\\DL-6BAF14C4"));
    }
}
