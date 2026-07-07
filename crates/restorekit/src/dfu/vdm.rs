/* SPDX-License-Identifier: Apache-2.0 */
//! Rust port of AsahiLinux/macvdmtool (Apache-2.0), which is itself based on
//! osy86/ThunderboltPatcher (Apache-2.0, Copyright 2019 osy86).
//!
//! Drives the host Apple Silicon Mac's USB-C port controller (the private
//! `AppleHPM` IOKit user client) to send Apple vendor USB-PD Vendor Defined
//! Messages that reboot the cabled target Mac into DFU (or back to normal).
//!
//! macOS + Apple Silicon + root only.

use std::ffi::{c_void, CString};
use std::os::raw::c_char;

use super::DfuTarget;
use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

type IOReturn = i32;
type KernReturn = i32;
type MachPort = u32;
type IoObject = u32;
type CFTypeRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFUUIDRef = *const c_void;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type Hresult = i32;
type Ulong = u32;

const KERN_SUCCESS: KernReturn = 0;
const K_CF_NUMBER_SINT32: isize = 3;
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// UUIDs (see AppleHPMLib.h and IOKit/IOCFPlugIn.h).
const APPLE_HPM_LIB_TYPE: [u8; 16] = [
    0x12, 0xA1, 0xDC, 0xCF, 0xCF, 0x7A, 0x47, 0x75, 0xBE, 0xE5, 0x9C, 0x43, 0x19, 0xF4, 0xCD, 0x2B,
];
const APPLE_HPM_LIB_INTERFACE: [u8; 16] = [
    0xC1, 0x3A, 0xCD, 0xD9, 0x20, 0x9E, 0x4B, 0x01, 0xB7, 0xBE, 0xE0, 0x5C, 0xD8, 0x83, 0xC7, 0xB1,
];
const IOCFPLUGIN_INTERFACE_ID: [u8; 16] = [
    0xC2, 0x44, 0xE8, 0x58, 0x10, 0x9C, 0x11, 0xD4, 0x91, 0xD4, 0x00, 0x50, 0xE4, 0xC6, 0x42, 0x6F,
];

#[repr(C)]
#[derive(Clone, Copy)]
struct CFUUIDBytes {
    bytes: [u8; 16],
}

#[repr(C)]
struct IOCFPlugInInterface {
    _reserved: *mut c_void,
    query_interface: unsafe extern "C" fn(*mut c_void, CFUUIDBytes, *mut *mut c_void) -> Hresult,
    add_ref: unsafe extern "C" fn(*mut c_void) -> Ulong,
    release: unsafe extern "C" fn(*mut c_void) -> Ulong,
    version: u16,
    revision: u16,
    probe: unsafe extern "C" fn(*mut c_void, CFDictionaryRef, IoObject, *mut i32) -> IOReturn,
    start: unsafe extern "C" fn(*mut c_void, CFDictionaryRef, IoObject) -> IOReturn,
    stop: unsafe extern "C" fn(*mut c_void) -> IOReturn,
}

/// Reverse-engineered vtable of the AppleHPM user-client plug-in.
#[repr(C)]
struct AppleHPMLib {
    _reserved: *mut c_void,
    query_interface: unsafe extern "C" fn(*mut c_void, CFUUIDBytes, *mut *mut c_void) -> Hresult,
    add_ref: unsafe extern "C" fn(*mut c_void) -> Ulong,
    release: unsafe extern "C" fn(*mut c_void) -> Ulong,
    field_20: u16,
    field_22: u16,
    read: unsafe extern "C" fn(*mut c_void, u64, u8, *mut c_void, u64, u32, *mut u64) -> IOReturn,
    write: unsafe extern "C" fn(*mut c_void, u64, u8, *const c_void, u64, u32) -> IOReturn,
    command: unsafe extern "C" fn(*mut c_void, u64, u32, u32) -> IOReturn,
    field_40: unsafe extern "C" fn() -> IOReturn,
    field_48: unsafe extern "C" fn() -> IOReturn,
    field_50: unsafe extern "C" fn() -> IOReturn,
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFTypeRef;
    fn IOServiceGetMatchingService(main_port: MachPort, matching: CFTypeRef) -> IoObject;
    fn IOServiceGetMatchingServices(
        main_port: MachPort,
        matching: CFTypeRef,
        existing: *mut IoObject,
    ) -> KernReturn;
    fn IOIteratorNext(iterator: IoObject) -> IoObject;
    fn IOObjectRelease(object: IoObject) -> KernReturn;
    fn IORegistryEntryGetName(entry: IoObject, name: *mut c_char) -> KernReturn;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> CFTypeRef;
    fn IOCreatePlugInInterfaceForService(
        service: IoObject,
        plugin_type: CFUUIDRef,
        interface_type: CFUUIDRef,
        the_interface: *mut *mut *mut IOCFPlugInInterface,
        the_score: *mut i32,
    ) -> KernReturn;
    fn IODestroyPlugInInterface(interface: *mut *mut IOCFPlugInInterface) -> KernReturn;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFNumberGetValue(number: CFTypeRef, the_type: isize, value_ptr: *mut c_void) -> bool;
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    #[allow(clippy::too_many_arguments)]
    fn CFUUIDGetConstantUUIDWithBytes(
        alloc: CFAllocatorRef,
        b0: u8,
        b1: u8,
        b2: u8,
        b3: u8,
        b4: u8,
        b5: u8,
        b6: u8,
        b7: u8,
        b8: u8,
        b9: u8,
        b10: u8,
        b11: u8,
        b12: u8,
        b13: u8,
        b14: u8,
        b15: u8,
    ) -> CFUUIDRef;
}

fn cfuuid(b: [u8; 16]) -> CFUUIDRef {
    unsafe {
        CFUUIDGetConstantUUIDWithBytes(
            std::ptr::null(),
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15],
        )
    }
}

/// FourCC command code, matching C multi-character constants (big-endian pack).
const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

/// The port controller connection, holding the opened plug-in. Exits DBMa mode
/// and destroys the plug-in on drop, mirroring macvdmtool's destructor.
struct Hpm {
    plugin: *mut *mut IOCFPlugInInterface,
    device: *mut *mut AppleHPMLib,
    chip: u64,
}

impl Hpm {
    fn read_register(&self, data_addr: u8) -> Result<[u8; 64]> {
        let mut buf = [0u8; 64];
        let mut rlen: u64 = 0;
        let ret = unsafe {
            ((**self.device).read)(
                self.device as *mut c_void,
                self.chip,
                data_addr,
                buf.as_mut_ptr() as *mut c_void,
                64,
                0,
                &mut rlen,
            )
        };
        if ret != 0 {
            return Err(Error::Vdm(format!(
                "readRegister(0x{data_addr:02x}) failed"
            )));
        }
        Ok(buf)
    }

    fn read_status_string(&self, data_addr: u8) -> Result<String> {
        let buf = self.read_register(data_addr)?;
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
    }

    /// Write args to data register 9, issue a command, return low nibble of the
    /// resulting register-9 status byte (0 == success).
    fn command(&self, cmd: u32, args: &[u8]) -> Result<u8> {
        if !args.is_empty() {
            let ret = unsafe {
                ((**self.device).write)(
                    self.device as *mut c_void,
                    self.chip,
                    9,
                    args.as_ptr() as *const c_void,
                    args.len() as u64,
                    0,
                )
            };
            if ret != 0 {
                return Err(Error::Vdm("writeRegister(9) failed".into()));
            }
        }
        let ret =
            unsafe { ((**self.device).command)(self.device as *mut c_void, self.chip, cmd, 0) };
        if ret != 0 {
            return Ok(0xff); // non-zero IOReturn: treat as command failure
        }
        let res = self.read_register(9)?;
        Ok(res[0] & 0x0f)
    }

    fn unlock(&self, key: u32) -> Result<()> {
        let args = key.to_le_bytes();
        if self.command(fourcc(b"LOCK"), &args)? != 0 {
            // Try a reset, then unlock again.
            if self.command(fourcc(b"Gaid"), &[])? != 0 {
                return Err(Error::Vdm("failed to unlock device (reset failed)".into()));
            }
            if self.command(fourcc(b"LOCK"), &args)? != 0 {
                return Err(Error::Vdm("failed to unlock device".into()));
            }
        }
        Ok(())
    }

    fn send_vdm(&self, vdm: &[u32]) -> Result<()> {
        let start = self.read_register(0x4d)?;
        let rxst = start[0];

        let mut args = Vec::with_capacity(1 + vdm.len() * 4);
        args.push((3u8 << 4) | (vdm.len() as u8));
        for w in vdm {
            args.extend_from_slice(&w.to_le_bytes());
        }
        if self.command(fourcc(b"VDMs"), &args)? != 0 {
            return Err(Error::Vdm("failed to send VDM".into()));
        }

        let mut reply = start;
        let mut got = false;
        for _ in 0..16 {
            reply = self.read_register(0x4d)?;
            if reply[0] != rxst {
                got = true;
                break;
            }
        }
        if !got {
            return Err(Error::Vdm("did not get a reply to VDM".into()));
        }

        // Reply layout: [status_byte, vdm_header (u32 LE), ...].
        let vdmhdr = u32::from_le_bytes([reply[1], reply[2], reply[3], reply[4]]);
        if vdmhdr != (vdm[0] | 0x40) {
            return Err(Error::Vdm(format!("VDM rejected (reply: 0x{vdmhdr:08x})")));
        }
        Ok(())
    }

    /// Bring the controller into debug mode (DBMa), unlocking first if needed.
    fn enter_dbma(&self, key: u32, progress: &mut dyn FnMut(Event)) -> Result<()> {
        let connection = self.read_register(0x3f)?;
        if connection[0] & 1 == 0 {
            return Err(Error::Vdm(
                "no target detected on the DFU port (check the cable and port)".into(),
            ));
        }

        let status = self.read_status_string(0x03)?;
        if status == "DBMa" {
            return Ok(());
        }

        progress(Event::DfuTriggerStage {
            stage: "unlocking port controller".into(),
        });
        self.unlock(key)?;

        progress(Event::DfuTriggerStage {
            stage: "entering debug mode".into(),
        });
        if self.command(fourcc(b"DBMa"), &[0x01])? != 0 {
            return Err(Error::Vdm("failed to enter DBMa mode".into()));
        }
        let status = self.read_status_string(0x03)?;
        if status != "DBMa" {
            return Err(Error::Vdm(format!(
                "failed to enter DBMa mode (status: {status})"
            )));
        }
        Ok(())
    }
}

impl Drop for Hpm {
    fn drop(&mut self) {
        unsafe {
            // Exit debug mode; ignore errors (best effort, matches upstream).
            let _ = self.command(fourcc(b"DBMa"), &[0x00]);
            IODestroyPlugInInterface(self.plugin);
        }
    }
}

/// Read the host Mac's platform name (e.g. "J314sAP") and derive the unlock key
/// from its first four bytes, matching macvdmtool's GetUnlockKey.
fn unlock_key() -> Result<(u32, String)> {
    unsafe {
        let class = CString::new("IOPlatformExpertDevice").unwrap();
        let matching = IOServiceMatching(class.as_ptr());
        if matching.is_null() {
            return Err(Error::Vdm(
                "IOServiceMatching(IOPlatformExpertDevice) failed".into(),
            ));
        }
        let service = IOServiceGetMatchingService(0, matching);
        if service == 0 {
            return Err(Error::Vdm("could not find IOPlatformExpertDevice".into()));
        }
        let mut name = [0i8; 128];
        let kr = IORegistryEntryGetName(service, name.as_mut_ptr());
        IOObjectRelease(service);
        if kr != KERN_SUCCESS {
            return Err(Error::Vdm("IORegistryEntryGetName failed".into()));
        }
        let bytes: [u8; 4] = [name[0] as u8, name[1] as u8, name[2] as u8, name[3] as u8];
        let key = (bytes[0] as u32) << 24
            | (bytes[1] as u32) << 16
            | (bytes[2] as u32) << 8
            | (bytes[3] as u32);
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        let full: Vec<u8> = name[..end].iter().map(|&b| b as u8).collect();
        Ok((key, String::from_utf8_lossy(&full).into_owned()))
    }
}

/// Resolve a [`DfuTarget`] to the AppleHPM `RID` whose port [`find_device`]
/// should open. `None` means "let `find_device` pick the first DFU-capable
/// controller" — the historical [`DfuTarget::Auto`] behavior.
fn resolve_rid(target: &DfuTarget) -> Result<Option<i32>> {
    match target {
        DfuTarget::Auto => Ok(None),
        DfuTarget::Port(rid) => {
            if super::port::all_ports()
                .iter()
                .any(|p| p.dfu && p.rid == *rid)
            {
                Ok(Some(*rid))
            } else {
                Err(Error::DfuPortNotFound(*rid))
            }
        }
        DfuTarget::Ecid(e) => {
            let mut devices = crate::device::list()?;
            crate::device::identify(&mut devices);
            let dev = devices
                .iter()
                .find(|d| d.ecid == Some(*e))
                .ok_or(Error::EcidNotConnected(*e))?;
            super::port::dfu_rid_for_serial(&dev.serial)
                .map(Some)
                .ok_or(Error::EcidNotOnDfuPort(*e))
        }
    }
}

/// Find a DFU-capable port controller and open it. With `target_rid == Some`,
/// opens exactly that controller; otherwise the device tree declares which
/// AppleHPM `RID`s carry the DFU/debug VDM path in `uart-hpm-rids` (see
/// [`super::port`]) and we pick the first matching controller, falling back to
/// `RID == 0` when that property is absent.
fn find_device(target_rid: Option<i32>) -> Result<Hpm> {
    let dfu_rids = super::port::dfu_capable_rids();
    let wanted = |rid: i32| match target_rid {
        Some(t) => rid == t,
        None => dfu_rids.contains(&rid),
    };
    unsafe {
        let class = CString::new("AppleHPM").unwrap();
        let matching = IOServiceMatching(class.as_ptr());
        if matching.is_null() {
            return Err(Error::Vdm("IOServiceMatching(AppleHPM) failed".into()));
        }
        let mut iter: IoObject = 0;
        if IOServiceGetMatchingServices(0, matching, &mut iter) != KERN_SUCCESS {
            return Err(Error::Vdm("IOServiceGetMatchingServices failed".into()));
        }

        let rid_key = CString::new("RID").unwrap();
        let mut chosen: Option<Hpm> = None;

        loop {
            let device = IOIteratorNext(iter);
            if device == 0 {
                break;
            }

            let cf_key = CFStringCreateWithCString(
                std::ptr::null(),
                rid_key.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            );
            let prop = IORegistryEntryCreateCFProperty(device, cf_key, std::ptr::null(), 0);
            if !cf_key.is_null() {
                CFRelease(cf_key);
            }
            if prop.is_null() {
                IOObjectRelease(device);
                continue;
            }
            let mut rid: i32 = -1;
            CFNumberGetValue(
                prop,
                K_CF_NUMBER_SINT32,
                &mut rid as *mut i32 as *mut c_void,
            );
            CFRelease(prop);

            // Skip controllers we don't want, and stop once we've opened one.
            if chosen.is_some() || !wanted(rid) {
                IOObjectRelease(device);
                continue;
            }

            // A DFU-capable port: open its plug-in.
            let mut plugin: *mut *mut IOCFPlugInInterface = std::ptr::null_mut();
            let mut score: i32 = 0;
            let kr = IOCreatePlugInInterfaceForService(
                device,
                cfuuid(APPLE_HPM_LIB_TYPE),
                cfuuid(IOCFPLUGIN_INTERFACE_ID),
                &mut plugin,
                &mut score,
            );
            IOObjectRelease(device);
            if kr != KERN_SUCCESS || plugin.is_null() {
                return Err(Error::Vdm(
                    "IOCreatePlugInInterfaceForService failed".into(),
                ));
            }

            let mut device_iface: *mut c_void = std::ptr::null_mut();
            let iid = CFUUIDBytes {
                bytes: APPLE_HPM_LIB_INTERFACE,
            };
            let res = ((**plugin).query_interface)(plugin as *mut c_void, iid, &mut device_iface);
            if res != 0 || device_iface.is_null() {
                IODestroyPlugInInterface(plugin);
                return Err(Error::Vdm("QueryInterface(AppleHPMLib) failed".into()));
            }

            chosen = Some(Hpm {
                plugin,
                device: device_iface as *mut *mut AppleHPMLib,
                chip: 0,
            });
        }
        IOObjectRelease(iter);

        chosen.ok_or_else(|| {
            Error::Vdm("no AppleHPM DFU port found (is a target cabled to the DFU port?)".into())
        })
    }
}

fn preflight() -> Result<()> {
    if !cfg!(target_arch = "aarch64") {
        return Err(Error::UnsupportedHost(
            "host is not an Apple Silicon Mac".into(),
        ));
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err(Error::NeedsRoot);
    }
    Ok(())
}

fn connect(target: &DfuTarget, progress: &mut dyn FnMut(Event)) -> Result<Hpm> {
    preflight()?;
    let target_rid = resolve_rid(target)?;
    let (key, mac_type) = unlock_key()?;
    progress(Event::DfuTriggerStage {
        stage: format!("host: {mac_type}"),
    });
    let hpm = find_device(target_rid)?;
    hpm.enter_dbma(key, progress)?;
    Ok(hpm)
}

/// Reboot the cabled target Mac into DFU mode. `target` selects which port to
/// drive when the host has several DFU-capable ports (see [`DfuTarget`]).
pub fn enter_dfu(target: &DfuTarget, progress: ProgressFn) -> Result<()> {
    let hpm = connect(target, progress)?;
    progress(Event::DfuTriggerStage {
        stage: "rebooting target into DFU".into(),
    });
    hpm.send_vdm(&[0x05ac_8012, 0x106, 0x8001_0000])?;
    Ok(())
}

/// Reboot the cabled target Mac into normal mode (undo a DFU trigger).
pub fn reboot(target: &DfuTarget, progress: ProgressFn) -> Result<()> {
    let hpm = connect(target, progress)?;
    progress(Event::DfuTriggerStage {
        stage: "rebooting target".into(),
    });
    hpm.send_vdm(&[0x05ac_8012, 0x105, 0x8000_0000])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_matches_c_multichar() {
        assert_eq!(fourcc(b"DBMa"), 0x4442_4D61);
        assert_eq!(fourcc(b"LOCK"), 0x4C4F_434B);
        assert_eq!(fourcc(b"VDMs"), 0x56444D73);
    }
}
