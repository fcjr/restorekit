//! Take a running iPhone or iPad into recovery mode, from which [`restore`]
//! restores it. A Mac isn't reachable this way: put it in DFU with [`dfu`].
//!
//! [`restore`]: crate::restore
//! [`dfu`]: crate::dfu

use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::error::ErrorStack;
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkcs12::Pkcs12;
use openssl::pkcs7::{Pkcs7, Pkcs7Flags};
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::stack::Stack;
use openssl::x509::{X509Builder, X509NameBuilder, X509};
use restorekit_sys as sys;
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// The label lockdownd logs with each request.
const LABEL: &CStr = c"restorekit";

/// Ask the running iPhone or iPad with this ECID to reboot into recovery mode.
///
/// The device is found by the `UniqueChipID` its lockdownd reports without
/// pairing, among the USB devices listed by the host's usbmuxd: Apple's on
/// macOS, Apple Mobile Device Service on Windows, the system usbmuxd on Linux.
/// `USBMUXD_SOCKET_ADDRESS` picks another, unless it points at the embedded
/// usbmuxd a restore in this process (or a parent's
/// [`SharedUsbmuxd`](crate::SharedUsbmuxd)) serves, which only sees restore
/// mode.
///
/// The request goes out without pairing first; iOS 14.5 and later refuse that,
/// so it is then sent in a paired session. Opening that session pairs the
/// device with this computer when the host has no pair record for it, or one
/// the device has since forgotten, as an erase does. The device asks whether
/// to trust this computer: this returns [`Error::TrustPending`], and a call
/// after the user taps Trust goes through. The pair record stays on the host.
///
/// With a [`Supervisor`], a device that lists its certificate as a supervising
/// host pairs without asking: in those same cases the host pairs as its
/// supervisor and saves the new record. A device the identity doesn't
/// supervise pairs as it would without one.
///
/// Returns once the device has accepted the request. It then leaves USB and
/// comes back in recovery mode, which
/// [`device::wait_where`](crate::device::wait_where) can wait for.
pub fn enter(ecid: u64, supervisor: Option<&Supervisor>) -> Result<()> {
    let _host = HostUsbmuxd::select();
    let device = find(ecid)?;
    let code = Lockdown::connect(&device)?.enter_recovery();
    if code != sys::LOCKDOWN_E_SESSION_INACTIVE {
        return check(code);
    }
    if let Some(supervisor) = supervisor.filter(|_| !device.paired()) {
        supervisor.pair(&device)?;
    }
    let lockdown = match Lockdown::handshake(&device) {
        // A pair record from before the device was erased.
        Err(Error::Lockdown {
            code: sys::LOCKDOWN_E_INVALID_HOST_ID,
            ..
        }) => {
            match supervisor {
                Some(supervisor) => supervisor.pair(&device)?,
                None => Lockdown::connect(&device)?.pair()?,
            }
            Lockdown::handshake(&device)?
        }
        lockdown => lockdown?,
    };
    check(lockdown.enter_recovery())
}

/// A supervision identity: a certificate and its private key, made by
/// [`Supervisor::generate`] or by Apple Configurator. A supervised device whose
/// supervising host certificates include the certificate pairs with a host
/// holding the key without asking its user whether to trust that host.
pub struct Supervisor {
    certificate: X509,
    key: PKey<Private>,
}

impl Supervisor {
    /// A new identity: an RSA key and a self-signed certificate with this
    /// common name, valid for 20 years.
    pub fn generate(name: &str) -> Result<Self> {
        let build = || -> std::result::Result<Self, ErrorStack> {
            let key = PKey::from_rsa(Rsa::generate(2048)?)?;
            let mut subject = X509NameBuilder::new()?;
            subject.append_entry_by_nid(Nid::COMMONNAME, name)?;
            let subject = subject.build();
            let mut serial = BigNum::new()?;
            serial.rand(64, MsbOption::MAYBE_ZERO, false)?;
            // A day early, for a device whose clock runs behind this host's.
            let since = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .saturating_sub(Duration::from_secs(24 * 60 * 60));
            let serial = serial.to_asn1_integer()?;
            let not_before = Asn1Time::from_unix(since.as_secs() as i64)?;
            let not_after = Asn1Time::days_from_now(20 * 365)?;
            let mut certificate = X509Builder::new()?;
            certificate.set_version(2)?;
            certificate.set_serial_number(&serial)?;
            certificate.set_subject_name(&subject)?;
            certificate.set_issuer_name(&subject)?;
            certificate.set_pubkey(&key)?;
            certificate.set_not_before(&not_before)?;
            certificate.set_not_after(&not_after)?;
            certificate.sign(&key, MessageDigest::sha256())?;
            Ok(Supervisor {
                certificate: certificate.build(),
                key,
            })
        };
        build().map_err(unusable)
    }

    /// Read the identity from PEM holding its certificate and unencrypted
    /// private key, as [`to_pem`](Self::to_pem) writes.
    pub fn from_pem(pem: &[u8]) -> Result<Self> {
        let invalid = |reason: &str| Error::SupervisionIdentity(reason.into());
        let certificate = X509::from_pem(pem).map_err(|_| invalid("no certificate"))?;
        // An empty passphrase fails an encrypted key instead of prompting.
        let key = PKey::private_key_from_pem_passphrase(pem, b"")
            .map_err(|_| invalid("no unencrypted private key"))?;
        if !certificate
            .public_key()
            .is_ok_and(|public| public.public_eq(&key))
        {
            return Err(invalid("the private key does not match the certificate"));
        }
        Ok(Supervisor { certificate, key })
    }

    /// Read the identity from a PKCS #12 archive protected by this password.
    ///
    /// The bundled OpenSSL has no legacy algorithms, so it can't read an
    /// archive encrypted with RC2, as macOS keychain exports are: convert one
    /// to PEM for [`from_pem`](Self::from_pem).
    pub fn from_pkcs12(der: &[u8], password: &str) -> Result<Self> {
        let invalid = |reason: &str| Error::SupervisionIdentity(reason.into());
        let archive = Pkcs12::from_der(der).map_err(|_| invalid("not a PKCS #12 archive"))?;
        let parsed = archive.parse2(password).map_err(|error| {
            let wrong = error
                .errors()
                .iter()
                .any(|error| error.reason() == Some("mac verify failure"));
            if wrong {
                invalid("wrong password")
            } else {
                unusable(error)
            }
        })?;
        let (Some(key), Some(certificate)) = (parsed.pkey, parsed.cert) else {
            return Err(invalid("the archive holds no certificate and key"));
        };
        if !certificate
            .public_key()
            .is_ok_and(|public| public.public_eq(&key))
        {
            return Err(invalid("the private key does not match the certificate"));
        }
        Ok(Supervisor { certificate, key })
    }

    /// The identity as PEM: its unencrypted private key, then its certificate.
    pub fn to_pem(&self) -> Result<Zeroizing<Vec<u8>>> {
        let key = Zeroizing::new(self.key.private_key_to_pem_pkcs8().map_err(unusable)?);
        let certificate = self.certificate.to_pem().map_err(unusable)?;
        Ok(Zeroizing::new([key.as_slice(), &certificate].concat()))
    }

    /// The identity as a PKCS #12 archive protected by this password, with
    /// OpenSSL's default AES-256 and PBKDF2 encryption.
    pub fn to_pkcs12(&self, password: &str) -> Result<Vec<u8>> {
        let mut builder = Pkcs12::builder();
        if let Some(name) = self.name() {
            builder.name(&name);
        }
        builder
            .pkey(&self.key)
            .cert(&self.certificate)
            .build2(password)
            .and_then(|archive| archive.to_der())
            .map_err(unusable)
    }

    /// The certificate in DER, as a device enrollment profile lists it among
    /// its supervising host certificates.
    pub fn certificate(&self) -> Result<Vec<u8>> {
        self.certificate.to_der().map_err(unusable)
    }

    /// The certificate's common name.
    pub fn name(&self) -> Option<String> {
        let entry = self
            .certificate
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()?;
        entry.data().to_string().ok()
    }

    /// Pair this host with the device as its supervisor. The device answers
    /// the certificate with a challenge, and accepts this host once it gets
    /// the challenge signed with the identity's key.
    ///
    /// Both requests go over one lockdownd client, which sends the response
    /// with the pair record the device challenged (our libimobiledevice patch)
    /// rather than a newly generated one.
    fn pair(&self, device: &Device) -> Result<()> {
        let lockdown = Lockdown::connect(device)?;
        let (code, response) = lockdown.pair_with(c"SupervisorCertificate", &self.certificate()?);
        if code != sys::LOCKDOWN_E_MC_CHALLENGE_REQUIRED {
            // Paired outright, as a device in Setup Assistant does, or
            // answered as to any host, as one this identity doesn't supervise does.
            return check(code);
        }
        let Some(challenge) = response.data(&[c"ExtendedResponse", c"PairingChallenge"]) else {
            return check(code);
        };
        check(
            lockdown
                .pair_with(c"ChallengeResponse", &self.sign(&challenge)?)
                .0,
        )
    }

    /// The challenge as DER PKCS #7 signed data, its bytes as they are.
    fn sign(&self, challenge: &[u8]) -> Result<Vec<u8>> {
        Stack::new()
            .and_then(|certs| {
                Pkcs7::sign(
                    &self.certificate,
                    &self.key,
                    &certs,
                    challenge,
                    Pkcs7Flags::BINARY,
                )
            })
            .and_then(|signed| signed.to_der())
            .map_err(unusable)
    }
}

/// Points this thread's libusbmuxd at the host's usbmuxd while held, whatever
/// `USBMUXD_SOCKET_ADDRESS` says for the rest of the process.
struct HostUsbmuxd;

impl HostUsbmuxd {
    fn select() -> Self {
        let address = host_socket_address();
        let address = address.as_deref().map_or(ptr::null(), CStr::as_ptr);
        unsafe { sys::libusbmuxd_set_thread_socket_address(1, address) };
        HostUsbmuxd
    }
}

impl Drop for HostUsbmuxd {
    fn drop(&mut self) {
        unsafe { sys::libusbmuxd_set_thread_socket_address(0, ptr::null()) };
    }
}

/// `USBMUXD_SOCKET_ADDRESS` unless it's an embedded usbmuxd's, or `None` for
/// the platform's default.
fn host_socket_address() -> Option<CString> {
    let address = std::env::var("USBMUXD_SOCKET_ADDRESS").ok()?;
    #[cfg(target_os = "linux")]
    if crate::usbmuxd::is_embedded_address(&address) {
        return None;
    }
    CString::new(address).ok()
}

fn unusable(error: ErrorStack) -> Error {
    Error::SupervisionIdentity(error.to_string())
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
struct Device {
    handle: sys::idevice_t,
    udid: CString,
}

impl Device {
    fn open(udid: &CStr) -> Option<Self> {
        let mut handle = ptr::null_mut();
        if unsafe { sys::idevice_new(&mut handle, udid.as_ptr()) } == sys::IDEVICE_E_SUCCESS {
            Some(Device {
                handle,
                udid: udid.to_owned(),
            })
        } else {
            None
        }
    }

    /// Whether the host has a pair record for the device.
    fn paired(&self) -> bool {
        let mut record = ptr::null_mut();
        let mut size = 0;
        if unsafe { sys::usbmuxd_read_pair_record(self.udid.as_ptr(), &mut record, &mut size) } != 0
        {
            return false;
        }
        // libusbmuxd allocates the record with malloc.
        unsafe { sys::plist_mem_free(record.cast()) };
        true
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { sys::idevice_free(self.handle) };
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
        check(unsafe { new(device.handle, &mut client, LABEL.as_ptr()) })?;
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

    /// [`pair`](Self::pair) with lockdownd's detailed pairing errors and this
    /// data as the pairing options, returning the device's response.
    fn pair_with(&self, key: &CStr, data: &[u8]) -> (c_int, Plist) {
        let mut response = ptr::null_mut();
        let code = unsafe {
            let options = Plist(sys::plist_new_dict());
            sys::plist_dict_set_item(
                options.0,
                c"ExtendedPairingErrors".as_ptr(),
                sys::plist_new_bool(1),
            );
            sys::plist_dict_set_item(
                options.0,
                key.as_ptr(),
                sys::plist_new_data(data.as_ptr().cast(), data.len() as u64),
            );
            sys::lockdownd_pair_with_options(self.client, ptr::null_mut(), options.0, &mut response)
        };
        (code, Plist(response))
    }
}

impl Drop for Lockdown<'_> {
    fn drop(&mut self) {
        unsafe { sys::lockdownd_client_free(self.client) };
    }
}

/// An owned libplist node, possibly none.
struct Plist(sys::plist_t);

impl Plist {
    /// The data at this path of dictionary keys.
    fn data(&self, path: &[&CStr]) -> Option<Vec<u8>> {
        let mut node = self.0;
        for key in path {
            if unsafe { sys::plist_get_node_type(node) } != sys::PLIST_DICT {
                return None;
            }
            node = unsafe { sys::plist_dict_get_item(node, key.as_ptr()) };
        }
        let mut length = 0;
        let bytes = unsafe { sys::plist_get_data_ptr(node, &mut length) };
        if bytes.is_null() || length == 0 {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), length as usize) }.to_vec())
    }
}

impl Drop for Plist {
    fn drop(&mut self) {
        unsafe { sys::plist_free(self.0) };
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
        assert_eq!(
            name(sys::LOCKDOWN_E_MC_CHALLENGE_REQUIRED),
            c"MC challenge required"
        );

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
            enter(0, None),
            Err(Error::RunningDeviceNotFound(0) | Error::UsbmuxdUnreachable)
        ));
    }

    #[test]
    fn the_host_usbmuxd_is_chosen_for_this_thread_only() {
        // Nothing listens on port 1, so a lookup that goes there can't list.
        let refused = c"127.0.0.1:1";
        std::thread::spawn(move || {
            unsafe { sys::libusbmuxd_set_thread_socket_address(1, refused.as_ptr()) };
            assert!(matches!(udids(), Err(Error::UsbmuxdUnreachable)));
            let other = std::thread::spawn(udids).join().unwrap();
            let host = {
                let _host = HostUsbmuxd::select();
                udids()
            };
            // Another thread, and this one inside the guard, reach the host's.
            assert_eq!(other.is_ok(), host.is_ok());
            unsafe { sys::libusbmuxd_set_thread_socket_address(0, ptr::null()) };
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_generated_identity_reads_back_from_its_pem() {
        let supervisor = Supervisor::generate("Example School").unwrap();
        let read = Supervisor::from_pem(&supervisor.to_pem().unwrap()).unwrap();
        assert_eq!(read.name().as_deref(), Some("Example School"));
        assert_eq!(
            read.certificate().unwrap(),
            supervisor.certificate().unwrap()
        );
        assert!(read.certificate.not_before() < Asn1Time::days_from_now(0).unwrap());
    }

    #[test]
    fn an_identity_moves_in_a_password_protected_pkcs12_archive() {
        let supervisor = Supervisor::generate("Example School").unwrap();
        let archive = supervisor.to_pkcs12("correct horse").unwrap();
        let read = Supervisor::from_pkcs12(&archive, "correct horse").unwrap();
        assert_eq!(read.name().as_deref(), Some("Example School"));
        assert_eq!(
            read.certificate().unwrap(),
            supervisor.certificate().unwrap()
        );
        assert!(read.key.public_eq(&supervisor.key));
        assert!(matches!(
            Supervisor::from_pkcs12(&archive, "wrong"),
            Err(Error::SupervisionIdentity(reason)) if reason == "wrong password"
        ));
        assert!(matches!(
            Supervisor::from_pkcs12(b"not an archive", "correct horse"),
            Err(Error::SupervisionIdentity(reason)) if reason == "not a PKCS #12 archive"
        ));
    }

    #[test]
    fn a_supervision_identity_is_its_certificate_and_own_unencrypted_key() {
        let supervisor = Supervisor::generate("Example School").unwrap();
        let other = Supervisor::generate("Elsewhere").unwrap();
        let certificate = supervisor.certificate.to_pem().unwrap();
        let unusable = |pem: &[&[u8]]| match Supervisor::from_pem(&pem.concat()) {
            Err(Error::SupervisionIdentity(reason)) => reason,
            _ => panic!("accepted"),
        };
        assert_eq!(
            unusable(&[&other.key.private_key_to_pem_pkcs8().unwrap(), &certificate]),
            "the private key does not match the certificate"
        );
        // Fails rather than asking for the passphrase.
        let encrypted = supervisor
            .key
            .private_key_to_pem_pkcs8_passphrase(openssl::symm::Cipher::aes_256_cbc(), b"secret")
            .unwrap();
        assert_eq!(
            unusable(&[&encrypted, &certificate]),
            "no unencrypted private key"
        );
        assert_eq!(
            unusable(&[&supervisor.key.private_key_to_pem_pkcs8().unwrap()]),
            "no certificate"
        );
    }

    #[test]
    fn a_signed_challenge_carries_the_challenge_unchanged() {
        let supervisor = Supervisor::generate("Example School").unwrap();
        // Text signing would rewrite these line endings.
        let challenge = b"\x00line\nnext\r\nlast\n";
        let signed = Pkcs7::from_der(&supervisor.sign(challenge).unwrap()).unwrap();

        let mut store = openssl::x509::store::X509StoreBuilder::new().unwrap();
        store.add_cert(supervisor.certificate.clone()).unwrap();
        let mut content = Vec::new();
        signed
            .verify(
                &Stack::new().unwrap(),
                &store.build(),
                None,
                Some(&mut content),
                Pkcs7Flags::BINARY,
            )
            .unwrap();
        assert_eq!(content, challenge);
    }
}
