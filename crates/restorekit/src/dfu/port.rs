//! macOS Apple Silicon USB-C port topology.
//!
//! Two questions, answered from read-only IORegistry (no root):
//!
//! - **Which port controller(s) can send DFU VDMs?** Apple's device tree
//!   declares this in `uart-hpm-rids` — a bitmask of the HPM `RID`s that carry
//!   the debug harness (UART/SWD/DFU), which all ride the same USB-PD VDM path.
//!   [`vdm`](super::vdm) sends the trigger to one of these controllers.
//! - **Which physical port is a given USB device on, and is it a DFU port?**
//!   The DFU controller's `port-number` matches a `usb-drd` USB controller,
//!   whose `locationID` is the base every device on that port enumerates under.

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};

type IoObject = u32;
type CfTypeRef = *const c_void;

const UTF8: u32 = 0x0800_0100;
const CF_NUMBER_SINT64: isize = 4;
// IORegistryEntrySearchCFProperty options.
const ITERATE_RECURSIVELY: u32 = 0x1;
const ITERATE_PARENTS: u32 = 0x2;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CfTypeRef;
    fn IOServiceGetMatchingServices(port: u32, matching: CfTypeRef, iter: *mut IoObject) -> i32;
    fn IOIteratorNext(iter: IoObject) -> IoObject;
    fn IOObjectRelease(obj: IoObject) -> i32;
    fn IORegistryGetRootEntry(port: u32) -> IoObject;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: CfTypeRef,
        allocator: CfTypeRef,
        options: u32,
    ) -> CfTypeRef;
    fn IORegistryEntrySearchCFProperty(
        entry: IoObject,
        plane: *const c_char,
        key: CfTypeRef,
        allocator: CfTypeRef,
        options: u32,
    ) -> CfTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CfTypeRef);
    fn CFGetTypeID(cf: CfTypeRef) -> usize;
    fn CFStringCreateWithCString(alloc: CfTypeRef, s: *const c_char, enc: u32) -> CfTypeRef;
    fn CFStringGetTypeID() -> usize;
    fn CFStringGetCString(s: CfTypeRef, buf: *mut c_char, size: isize, enc: u32) -> bool;
    fn CFNumberGetValue(n: CfTypeRef, ty: isize, val: *mut c_void) -> bool;
    fn CFDataGetTypeID() -> usize;
    fn CFDataGetLength(d: CfTypeRef) -> isize;
    fn CFDataGetBytePtr(d: CfTypeRef) -> *const u8;
}

fn cfstr(s: &str) -> CfTypeRef {
    let c = CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) }
}

/// Interpret a CF value as a little-endian `u32`: a `CFData` blob (device-tree
/// properties like `port-number` / `uart-hpm-rids`) or a `CFNumber`.
unsafe fn as_u32(value: CfTypeRef) -> Option<u32> {
    if value.is_null() {
        return None;
    }
    let id = CFGetTypeID(value);
    if id == CFDataGetTypeID() {
        let len = CFDataGetLength(value);
        let ptr = CFDataGetBytePtr(value);
        (len >= 4 && !ptr.is_null())
            .then(|| u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]))
    } else {
        let mut v: i64 = 0;
        CFNumberGetValue(value, CF_NUMBER_SINT64, &mut v as *mut i64 as *mut c_void)
            .then_some(v as u32)
    }
}

/// Interpret a CF value as a string: either a `CFString` or a `CFData` blob
/// holding text (how the device tree stores `port-location`).
unsafe fn as_string(value: CfTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let id = CFGetTypeID(value);
    if id == CFDataGetTypeID() {
        let len = CFDataGetLength(value).max(0) as usize;
        let ptr = CFDataGetBytePtr(value);
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, len);
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
        std::str::from_utf8(&bytes[..end]).ok().map(str::to_string)
    } else if id == CFStringGetTypeID() {
        let mut buf = [0i8; 128];
        CFStringGetCString(value, buf.as_mut_ptr(), buf.len() as isize, UTF8)
            .then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Read a property directly off an entry, as a `u32`.
unsafe fn prop_u32(entry: IoObject, key: &str) -> Option<u32> {
    let k = cfstr(key);
    let v = IORegistryEntryCreateCFProperty(entry, k, std::ptr::null(), 0);
    CFRelease(k);
    let out = as_u32(v);
    if !v.is_null() {
        CFRelease(v);
    }
    out
}

/// Search an entry and its ancestors (IOService plane) for a property.
unsafe fn search_u32(entry: IoObject, key: &str) -> Option<u32> {
    let (k, plane) = (cfstr(key), CString::new("IOService").unwrap());
    let v = IORegistryEntrySearchCFProperty(
        entry,
        plane.as_ptr(),
        k,
        std::ptr::null(),
        ITERATE_RECURSIVELY | ITERATE_PARENTS,
    );
    CFRelease(k);
    let out = as_u32(v);
    if !v.is_null() {
        CFRelease(v);
    }
    out
}

unsafe fn search_string(entry: IoObject, key: &str) -> Option<String> {
    let (k, plane) = (cfstr(key), CString::new("IOService").unwrap());
    let v = IORegistryEntrySearchCFProperty(
        entry,
        plane.as_ptr(),
        k,
        std::ptr::null(),
        ITERATE_RECURSIVELY | ITERATE_PARENTS,
    );
    CFRelease(k);
    let out = as_string(v);
    if !v.is_null() {
        CFRelease(v);
    }
    out
}

/// Run `f` for each service matching `class`, releasing each node after.
unsafe fn for_each_service(class: &str, mut f: impl FnMut(IoObject)) {
    let matching = IOServiceMatching(CString::new(class).unwrap().as_ptr());
    if matching.is_null() {
        return;
    }
    let mut iter: IoObject = 0;
    if IOServiceGetMatchingServices(0, matching, &mut iter) != 0 {
        return;
    }
    loop {
        let node = IOIteratorNext(iter);
        if node == 0 {
            break;
        }
        f(node);
        IOObjectRelease(node);
    }
    IOObjectRelease(iter);
}

/// RIDs of the HPM controllers that can send DFU VDMs, from `uart-hpm-rids`
/// (a bitmask). Falls back to `[0]` when the property is absent — the port
/// index [`vdm`](super::vdm) historically hardcoded.
pub(crate) fn dfu_capable_rids() -> Vec<i32> {
    read_uart_hpm_rids().unwrap_or_else(|| vec![0])
}

fn read_uart_hpm_rids() -> Option<Vec<i32>> {
    unsafe {
        let root = IORegistryGetRootEntry(0);
        if root == 0 {
            return None;
        }
        let k = cfstr("uart-hpm-rids");
        let plane = CString::new("IODeviceTree").unwrap();
        let v = IORegistryEntrySearchCFProperty(
            root,
            plane.as_ptr(),
            k,
            std::ptr::null(),
            ITERATE_RECURSIVELY,
        );
        CFRelease(k);
        IOObjectRelease(root);
        let mask = as_u32(v);
        if !v.is_null() {
            CFRelease(v);
        }
        let mask = mask?;
        (mask != 0).then(|| (0..32).filter(|b| mask & (1 << b) != 0).collect())
    }
}

/// One host USB-C port: the `locationID` base (top byte) every device on it
/// enumerates under, its firmware location name, and whether it's DFU-capable.
/// A USB device is on this port iff `device_location & 0xff00_0000 == base`.
struct HostPort {
    base: u32,
    location: Option<String>,
    dfu: bool,
}

/// Every USB-C port on this host. Cached — host topology is invariant.
fn host_ports() -> &'static [HostPort] {
    use std::sync::OnceLock;
    static PORTS: OnceLock<Vec<HostPort>> = OnceLock::new();
    PORTS.get_or_init(resolve_host_ports)
}

fn resolve_host_ports() -> Vec<HostPort> {
    let dfu_rids = dfu_capable_rids();
    unsafe {
        // Each Type-C controller → (port-number, location, is-dfu).
        let mut controllers: Vec<(u32, Option<String>, bool)> = Vec::new();
        for_each_service("AppleHPM", |hpm| {
            if let (Some(rid), Some(pn)) = (prop_u32(hpm, "RID"), search_u32(hpm, "port-number")) {
                let location = search_string(hpm, "port-location");
                controllers.push((pn, location, dfu_rids.contains(&(rid as i32))));
            }
        });
        // Match each to the USB controller with the same port-number → its
        // locationID base.
        let mut ports = Vec::new();
        for_each_service("AppleUSBHostController", |ctrl| {
            let (Some(loc), Some(pn)) = (
                prop_u32(ctrl, "locationID"),
                search_u32(ctrl, "port-number"),
            ) else {
                return;
            };
            if let Some((_, location, dfu)) = controllers.iter().find(|(w, _, _)| *w == pn) {
                ports.push(HostPort {
                    base: loc & 0xff00_0000,
                    location: location.clone(),
                    dfu: *dfu,
                });
            }
        });
        ports
    }
}

/// Physical location of the host's DFU-capable port, e.g. "left-back".
pub(crate) fn dfu_port_location() -> Option<String> {
    host_ports()
        .iter()
        .find(|p| p.dfu)
        .and_then(|p| p.location.clone())
}

/// Map every connected Apple USB device's serial to its `locationID`.
fn locations_by_serial() -> HashMap<String, u32> {
    let mut out = HashMap::new();
    unsafe {
        for_each_service("IOUSBHostDevice", |dev| {
            let k = cfstr("kUSBSerialNumberString");
            let v = IORegistryEntryCreateCFProperty(dev, k, std::ptr::null(), 0);
            CFRelease(k);
            let serial = as_string(v);
            if !v.is_null() {
                CFRelease(v);
            }
            if let (Some(serial), Some(loc)) = (serial, prop_u32(dev, "locationID")) {
                out.insert(serial, loc);
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    /// Prints the host's USB-C port topology — no target needed.
    /// `cargo test -p restorekit port_topology -- --ignored --nocapture`
    #[test]
    #[ignore = "reads live host IORegistry"]
    fn port_topology() {
        eprintln!("dfu_capable_rids: {:?}", super::dfu_capable_rids());
        for p in super::host_ports() {
            eprintln!(
                "port: base={:#010x} location={:?} dfu={}",
                p.base, p.location, p.dfu
            );
        }
    }
}

/// Set each device's [`Port`](crate::device::Port) from its `locationID`.
pub(crate) fn mark_ports(devices: &mut [super::super::device::Device]) {
    use crate::device::Port;
    let ports = host_ports();
    if ports.is_empty() {
        return; // couldn't resolve topology — leave unknown
    }
    let locs = locations_by_serial();
    for d in devices.iter_mut() {
        if let Some(loc) = locs.get(&d.serial) {
            if let Some(p) = ports.iter().find(|p| loc & 0xff00_0000 == p.base) {
                d.port = Some(Port {
                    dfu: p.dfu,
                    location: p.location.clone(),
                });
            }
        }
    }
}
