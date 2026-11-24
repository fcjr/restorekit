//! Take a running iPhone or iPad into recovery mode, from which [`restore`]
//! restores it. A Mac isn't reachable this way: put it in DFU with [`dfu`].
//!
//! [`restore`]: crate::restore
//! [`dfu`]: crate::dfu

use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_int};
use std::ptr;

use restorekit_sys as sys;

use crate::error::{Error, Result};

/// The label lockdownd logs with each request.
const LABEL: &CStr = c"restorekit";

/// Ask the running iPhone or iPad with this ECID to reboot into recovery mode.
///
/// The device is found by the `UniqueChipID` its lockdownd reports without
/// pairing, among the USB devices listed by the usbmuxd this process is
/// configured for (`USBMUXD_SOCKET_ADDRESS`, else the host's own). A running
/// device is served by the host's own: Apple's on macOS, Apple Mobile Device
/// Service on Windows, the system usbmuxd on Linux.
///
/// The request goes out without pairing first; iOS 14.5 and later refuse that,
/// so it is then sent in a paired session. Opening that session pairs the
/// device with this computer when the host has no pair record for it, or one
/// the device has since forgotten, as an erase does. The device asks whether
/// to trust this computer: this returns [`Error::TrustPending`], and a call
/// after the user taps Trust goes through. The pair record stays on the host.
///
/// Returns once the device has accepted the request. It then leaves USB and
/// comes back in recovery mode, which
/// [`device::wait_where`](crate::device::wait_where) can wait for.
pub fn enter(ecid: u64) -> Result<()> {
    let device = find(ecid)?;
    let code = Lockdown::connect(&device)?.enter_recovery();
    if code != sys::LOCKDOWN_E_SESSION_INACTIVE {
        return check(code);
    }
    let lockdown = match Lockdown::handshake(&device) {
        // A pair record from before the device was erased.
        Err(Error::Lockdown {
            code: sys::LOCKDOWN_E_INVALID_HOST_ID,
            ..
        }) => {
            Lockdown::connect(&device)?.pair()?;
            Lockdown::handshake(&device)?
        }
        lockdown => lockdown?,
    };
    check(lockdown.enter_recovery())
}

/// Open the USB device whose lockdownd reports this ECID.
fn find(ecid: u64) -> Result<Device> {
    udids()?
        .iter()
        .filter_map(|udid| Device::open(udid))
        .find(|device| {
            Lockdown::connect(device).is_ok_and(|lockdown| lockdown.unique_chip_id() == Some(ecid))
        })
        .ok_or(Error::RunningDeviceNotFound(ecid))
}

/// The UDIDs of the USB devices usbmuxd lists.
fn udids() -> Result<Vec<CString>> {
    let mut list: *mut *mut c_char = ptr::null_mut();
    let mut count: c_int = 0;
    if unsafe { sys::idevice_get_device_list(&mut list, &mut count) } != sys::IDEVICE_E_SUCCESS {
        return Err(Error::UsbmuxdUnreachable);
    }
    let udids = (0..count.max(0) as usize)
        .map(|i| unsafe { CStr::from_ptr(*list.add(i)) }.to_owned())
        .collect();
    unsafe { sys::idevice_device_list_free(list) };
    Ok(udids)
}

/// A USB device usbmuxd lists.
struct Device(sys::idevice_t);

impl Device {
    fn open(udid: &CStr) -> Option<Self> {
        let mut device = ptr::null_mut();
        if unsafe { sys::idevice_new(&mut device, udid.as_ptr()) } == sys::IDEVICE_E_SUCCESS {
            Some(Device(device))
        } else {
            None
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { sys::idevice_free(self.0) };
    }
}

/// A lockdownd client on a [`Device`].
struct Lockdown<'a> {
    client: sys::lockdownd_client_t,
    _device: PhantomData<&'a Device>,
}

type NewClient =
    unsafe extern "C" fn(sys::idevice_t, *mut sys::lockdownd_client_t, *const c_char) -> c_int;

impl<'a> Lockdown<'a> {
    /// Connect without pairing.
    fn connect(device: &'a Device) -> Result<Self> {
        Self::open(device, sys::lockdownd_client_new)
    }

    /// Connect in a paired session.
    fn handshake(device: &'a Device) -> Result<Self> {
        Self::open(device, sys::lockdownd_client_new_with_handshake)
    }

    fn open(device: &'a Device, new: NewClient) -> Result<Self> {
        let mut client = ptr::null_mut();
        check(unsafe { new(device.0, &mut client, LABEL.as_ptr()) })?;
        Ok(Lockdown {
            client,
            _device: PhantomData,
        })
    }

    /// The ECID lockdownd reports, or `None` when the service on the lockdown
    /// port is something else, such as a device in restore mode.
    fn unique_chip_id(&self) -> Option<u64> {
        unsafe {
            let mut kind: *mut c_char = ptr::null_mut();
            let lockdownd = sys::lockdownd_query_type(self.client, &mut kind)
                == sys::LOCKDOWN_E_SUCCESS
                && !kind.is_null()
                && CStr::from_ptr(kind) == c"com.apple.mobile.lockdown";
            sys::plist_mem_free(kind.cast());
            if !lockdownd {
                return None;
            }

            let mut node: sys::plist_t = ptr::null_mut();
            let mut ecid = None;
            if sys::lockdownd_get_value(
                self.client,
                ptr::null(),
                c"UniqueChipID".as_ptr(),
                &mut node,
            ) == sys::LOCKDOWN_E_SUCCESS
                && sys::plist_get_node_type(node) == sys::PLIST_INT
            {
                let mut value = 0;
                sys::plist_get_uint_val(node, &mut value);
                ecid = Some(value);
            }
            sys::plist_free(node);
            ecid
        }
    }

    fn enter_recovery(&self) -> c_int {
        unsafe { sys::lockdownd_enter_recovery(self.client) }
    }

    /// Ask the device to pair with a new pair record, which the host saves
    /// once the device accepts it.
    fn pair(&self) -> Result<()> {
        check(unsafe { sys::lockdownd_pair(self.client, ptr::null_mut()) })
    }
}

impl Drop for Lockdown<'_> {
    fn drop(&mut self) {
        unsafe { sys::lockdownd_client_free(self.client) };
    }
}

/// A lockdownd result as this crate's result.
fn check(code: c_int) -> Result<()> {
    match code {
        sys::LOCKDOWN_E_SUCCESS => Ok(()),
        sys::LOCKDOWN_E_PASSWORD_PROTECTED => Err(Error::DeviceLocked),
        sys::LOCKDOWN_E_PAIRING_DIALOG_RESPONSE_PENDING => Err(Error::TrustPending),
        sys::LOCKDOWN_E_USER_DENIED_PAIRING => Err(Error::TrustDenied),
        code => Err(Error::Lockdown {
            code,
            message: unsafe { CStr::from_ptr(sys::lockdownd_strerror(code)) }
                .to_string_lossy()
                .into_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockdown_results_map_to_errors() {
        // libimobiledevice's name for each code pins the constant.
        let name = |code| unsafe { CStr::from_ptr(sys::lockdownd_strerror(code)) };
        assert_eq!(
            name(sys::LOCKDOWN_E_PASSWORD_PROTECTED),
            c"Password protected"
        );
        assert_eq!(
            name(sys::LOCKDOWN_E_USER_DENIED_PAIRING),
            c"User denied pairing"
        );
        assert_eq!(
            name(sys::LOCKDOWN_E_PAIRING_DIALOG_RESPONSE_PENDING),
            c"Pairing dialog response pending"
        );
        assert_eq!(name(sys::LOCKDOWN_E_INVALID_HOST_ID), c"Invalid HostID");
        assert_eq!(name(sys::LOCKDOWN_E_SESSION_INACTIVE), c"Session inactive");

        assert!(check(sys::LOCKDOWN_E_SUCCESS).is_ok());
        assert!(matches!(
            check(sys::LOCKDOWN_E_PASSWORD_PROTECTED),
            Err(Error::DeviceLocked)
        ));
        assert!(matches!(
            check(sys::LOCKDOWN_E_PAIRING_DIALOG_RESPONSE_PENDING),
            Err(Error::TrustPending)
        ));
        assert!(matches!(
            check(sys::LOCKDOWN_E_USER_DENIED_PAIRING),
            Err(Error::TrustDenied)
        ));
        // What enter() pairs again on.
        assert!(matches!(
            check(sys::LOCKDOWN_E_INVALID_HOST_ID),
            Err(Error::Lockdown { code: -21, .. })
        ));
        assert!(matches!(
            check(sys::LOCKDOWN_E_SESSION_INACTIVE),
            Err(Error::Lockdown { code: -23, message }) if message == "Session inactive"
        ));
    }

    #[test]
    fn an_absent_ecid_is_not_found() {
        // No device has ECID 0. Without a usbmuxd, as on the Linux and Windows
        // CI runners, the lookup stops before listing devices.
        assert!(matches!(
            enter(0),
            Err(Error::RunningDeviceNotFound(0) | Error::UsbmuxdUnreachable)
        ));
    }
}
