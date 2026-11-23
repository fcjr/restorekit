//! A USB 3 port's USB 2 companion, for dongles whose SuperSpeed lanes pass
//! straight through (Dongle-Pro v2).
//!
//! Every USB 3 connector is two ports: a USB 2 one, where the dongle's hub and
//! MCU enumerate, and a SuperSpeed one, where a USB 3 target enumerates on its
//! own, outside the hub. The OS knows which two belong together:
//!
//! - macOS: the USB 2 port's `UsbProtocolCompanion (3.x)` IORegistry path.
//! - Linux: the USB 2 port's `peer` link in sysfs.
//! - Elsewhere there is no lookup, so [`fallback`] takes the one SuperSpeed
//!   Apple device that sits beside the hub, if there is exactly one.

use nusb::{DeviceInfo, Speed};

/// True if `dev` is on the SuperSpeed companion of the port `hub` is plugged into.
pub(crate) fn is_companion(hub: &DeviceInfo, dev: &DeviceInfo) -> bool {
    imp::is_companion(hub, dev).unwrap_or(false)
}

/// Whether the platform can answer [`is_companion`] exactly.
pub(crate) const EXACT: bool = cfg!(any(target_os = "macos", target_os = "linux"));

/// Without an exact lookup: the only SuperSpeed device among `candidates` on
/// the hub's bus at the hub's depth, if there is exactly one.
pub(crate) fn fallback<'a>(
    hub: &DeviceInfo,
    candidates: impl Iterator<Item = &'a DeviceInfo>,
) -> Option<&'a DeviceInfo> {
    let mut found = candidates.filter(|d| {
        d.bus_id() == hub.bus_id()
            && d.port_chain().len() == hub.port_chain().len()
            && d.port_chain() != hub.port_chain()
            && matches!(d.speed(), Some(Speed::Super | Speed::SuperPlus))
    });
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

#[cfg(target_os = "linux")]
mod imp {
    use nusb::DeviceInfo;
    use std::fs;

    pub(super) fn is_companion(hub: &DeviceInfo, dev: &DeviceInfo) -> Option<bool> {
        let peer = fs::canonicalize(hub.sysfs_path().join("port/peer")).ok()?;
        let port = fs::canonicalize(dev.sysfs_path().join("port")).ok()?;
        Some(peer == port)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use nusb::DeviceInfo;
    use std::ffi::{c_char, c_void, CString};

    type IoObject = u32;
    const SERVICE_PLANE: &[u8] = b"IOService\0";

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IORegistryEntryIDMatching(id: u64) -> *const c_void;
        fn IOServiceGetMatchingService(port: u32, matching: *const c_void) -> IoObject;
        fn IORegistryEntryGetParentEntry(
            entry: IoObject,
            plane: *const c_char,
            parent: *mut IoObject,
        ) -> i32;
        fn IORegistryEntryGetRegistryEntryID(entry: IoObject, id: *mut u64) -> i32;
        fn IORegistryEntryFromPath(port: u32, path: *const c_char) -> IoObject;
        fn IORegistryEntryCreateCFProperty(
            entry: IoObject,
            key: *const c_void,
            alloc: *const c_void,
            options: u32,
        ) -> *const c_void;
        fn IOObjectRelease(obj: IoObject) -> i32;
    }

    /// Releases an io_object_t on drop.
    struct Obj(IoObject);
    impl Drop for Obj {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe { IOObjectRelease(self.0) };
            }
        }
    }

    fn by_id(id: u64) -> Option<Obj> {
        let entry = unsafe { IOServiceGetMatchingService(0, IORegistryEntryIDMatching(id)) };
        (entry != 0).then_some(Obj(entry))
    }

    fn parent(entry: &Obj) -> Option<Obj> {
        let mut parent = 0;
        let kr = unsafe {
            IORegistryEntryGetParentEntry(entry.0, SERVICE_PLANE.as_ptr().cast(), &mut parent)
        };
        (kr == 0 && parent != 0).then_some(Obj(parent))
    }

    fn id(entry: &Obj) -> Option<u64> {
        let mut id = 0;
        (unsafe { IORegistryEntryGetRegistryEntryID(entry.0, &mut id) } == 0).then_some(id)
    }

    fn string(entry: &Obj, key: &str) -> Option<String> {
        let key = CFString::new(key);
        let value = unsafe {
            IORegistryEntryCreateCFProperty(
                entry.0,
                key.as_concrete_TypeRef().cast(),
                std::ptr::null(),
                0,
            )
        };
        if value.is_null() {
            return None;
        }
        // The property is a CFString; wrap_under_create_rule releases it.
        let value = unsafe { CFString::wrap_under_create_rule(value as CFStringRef) };
        Some(value.to_string())
    }

    pub(super) fn is_companion(hub: &DeviceInfo, dev: &DeviceInfo) -> Option<bool> {
        // The hub's parent in the service plane is the port it is plugged into.
        let hub_port = parent(&by_id(hub.registry_entry_id())?)?;
        let path = CString::new(string(&hub_port, "UsbProtocolCompanion (3.x)")?).ok()?;
        let companion = unsafe { IORegistryEntryFromPath(0, path.as_ptr()) };
        if companion == 0 {
            return Some(false);
        }
        let companion = Obj(companion);
        let dev_port = parent(&by_id(dev.registry_entry_id())?)?;
        Some(id(&companion)? == id(&dev_port)?)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use nusb::DeviceInfo;

    pub(super) fn is_companion(_: &DeviceInfo, _: &DeviceInfo) -> Option<bool> {
        None
    }
}
