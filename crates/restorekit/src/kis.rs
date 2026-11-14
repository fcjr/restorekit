//! Identity of a Mac in DFU over Apple's Debug USB interface ("KIS", `05ac:1881`).
//!
//! A KIS-mode Mac is in the same bootrom DFU as a classic `05ac:1227` one, but its
//! USB serial is empty: the `CPID:… BDID:… ECID:…` string sits behind a KIS
//! request instead. This is a port of libirecovery's `irecv_kis_init` +
//! `irecv_kis_load_device_info` over nusb, so we can open exactly the device we
//! enumerated (libirecovery's `irecv_open_with_ecid(0)` takes whichever
//! restore-family device it finds first).
//!
//! macOS gates `05ac:1881` behind the `com.apple.private.debug-usb.access`
//! entitlement (and `deviceinterfaced` holds the device), so on a macOS host the
//! open fails and the identity stays unknown.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::MaybeFuture;

const PORTAL_CONFIG: u8 = 0x01;
const PORTAL_RSM: u8 = 0x10;

const INDEX_ENABLE_A: u16 = 0x0a;
const INDEX_ENABLE_B: u16 = 0x14;
const INDEX_GET_INFO: u16 = 0x100;
const ENABLE_A: u32 = 0x21;
const ENABLE_B: u32 = 0x01;

const HEADER_LEN: usize = 12;
/// Size of the device-info block the GET_INFO reply carries after its header.
const INFO_LEN: usize = 0x300;
/// Offset of the embedded USB device descriptor inside the info block.
const DESCRIPTOR_OFFSET: usize = 64;
/// `iSerialNumber` within a USB device descriptor.
const I_SERIAL_NUMBER: usize = 16;

const TIMEOUT: Duration = Duration::from_secs(2);

/// The identity string (`CPID:… ECID:…`) of the KIS device behind `info`, or
/// `None` if it can't be opened or read. Cached per enumeration, so polling
/// `list()` opens each KIS device once rather than on every call.
pub(crate) fn serial(info: &nusb::DeviceInfo) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<nusb::DeviceId, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(&info.id()) {
        return hit.clone();
    }
    let serial = read(info);
    cache.lock().unwrap().insert(info.id(), serial.clone());
    serial
}

fn read(info: &nusb::DeviceInfo) -> Option<String> {
    let dev = info.open().wait().ok()?;
    let iface = dev.claim_interface(0).wait().ok()?;
    let mut config = Portal::open(&iface, 0x01)?;
    let mut rsm = Portal::open(&iface, 0x03)?;
    config.request(PORTAL_CONFIG, INDEX_ENABLE_A, &[ENABLE_A], 1)?;
    config.request(PORTAL_CONFIG, INDEX_ENABLE_B, &[ENABLE_B], 1)?;
    let reply = rsm.request(PORTAL_RSM, INDEX_GET_INFO, &[], INFO_LEN / 4)?;
    serial_from_info(reply.get(HEADER_LEN..)?)
}

/// A KIS portal: a bulk OUT/IN endpoint pair.
struct Portal {
    out: nusb::Endpoint<Bulk, Out>,
    inp: nusb::Endpoint<Bulk, In>,
}

impl Portal {
    fn open(iface: &nusb::Interface, ep: u8) -> Option<Portal> {
        Some(Portal {
            out: iface.endpoint::<Bulk, Out>(ep).ok()?,
            inp: iface.endpoint::<Bulk, In>(ep | 0x80).ok()?,
        })
    }

    fn request(
        &mut self,
        portal: u8,
        index: u16,
        args: &[u32],
        reply_words: usize,
    ) -> Option<Vec<u8>> {
        let mut req = header(portal, index, args.len(), reply_words).to_vec();
        for a in args {
            req.extend_from_slice(&a.to_le_bytes());
        }
        self.out
            .transfer_blocking(req.into(), TIMEOUT)
            .status
            .ok()?;
        // Reply: header, payload words, then a size/status trailer. IN transfers
        // must be a whole number of max-size packets.
        let mps = self.inp.max_packet_size().max(1);
        let want = (HEADER_LEN + reply_words * 4 + 8).div_ceil(mps) * mps;
        let done = self.inp.transfer_blocking(Buffer::new(want), TIMEOUT);
        done.status.ok()?;
        Some(done.buffer.into_vec())
    }
}

/// The 12-byte KIS request header (libirecovery's `irecv_kis_request_init`).
fn header(portal: u8, index: u16, arg_count: usize, reply_words: usize) -> [u8; HEADER_LEN] {
    let req_size = (arg_count * 4) as u32;
    let mut h = [0u8; HEADER_LEN];
    // h[0..2]: sequence number, ignored by the device.
    h[2] = 0xa0; // protocol version
    h[3] = portal;
    h[4] = arg_count as u8;
    h[5] = (index & 0xff) as u8;
    h[6] = (((index >> 8) & 0x3) as u8) | (((reply_words << 2) & 0xfc) as u8);
    h[7] = ((reply_words >> 6) & 0xff) as u8;
    h[8..12].copy_from_slice(&req_size.to_le_bytes());
    h
}

/// Pull the serial string out of a GET_INFO info block: the embedded device
/// descriptor's `iSerialNumber` is a word offset to a USB string descriptor.
fn serial_from_info(info: &[u8]) -> Option<String> {
    let index = *info.get(DESCRIPTOR_OFFSET + I_SERIAL_NUMBER)? as usize;
    string_at(info, index * 4)
}

/// A USB string descriptor (`bLength`, `bDescriptorType` = 3, UTF-16LE) at `off`.
fn string_at(info: &[u8], off: usize) -> Option<String> {
    let len = *info.get(off)? as usize;
    if *info.get(off + 1)? != 3 || !len.is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = info
        .get(off + 2..off + 2 + len)?
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16(&units).ok()?;
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_libirecovery() {
        // GET_INFO on the RSM portal, 0xC0 reply words, no args.
        assert_eq!(
            header(PORTAL_RSM, INDEX_GET_INFO, 0, INFO_LEN / 4),
            [0, 0, 0xa0, 0x10, 0, 0x00, 0x01, 0x03, 0, 0, 0, 0]
        );
        // Config write: one arg, one reply word.
        assert_eq!(
            header(PORTAL_CONFIG, INDEX_ENABLE_A, 1, 1),
            [0, 0, 0xa0, 0x01, 1, 0x0a, 0x04, 0x00, 4, 0, 0, 0]
        );
    }

    #[test]
    fn parses_serial_from_info_block() {
        let serial = "CPID:8142 CPRV:01 BDID:0E ECID:001A2B3C4D5E6F70";
        let mut info = vec![0u8; INFO_LEN];
        let word = 0x40; // string at byte 0x100
        info[DESCRIPTOR_OFFSET + I_SERIAL_NUMBER] = word as u8;
        let off = word * 4;
        let utf16: Vec<u8> = serial.encode_utf16().flat_map(u16::to_le_bytes).collect();
        info[off] = utf16.len() as u8;
        info[off + 1] = 3;
        info[off + 2..off + 2 + utf16.len()].copy_from_slice(&utf16);
        assert_eq!(serial_from_info(&info).as_deref(), Some(serial));
    }

    #[test]
    fn rejects_non_string_descriptor() {
        let mut info = vec![0u8; INFO_LEN];
        info[DESCRIPTOR_OFFSET + I_SERIAL_NUMBER] = 0x40;
        info[0x100] = 4;
        info[0x101] = 1;
        assert!(serial_from_info(&info).is_none());
    }
}
