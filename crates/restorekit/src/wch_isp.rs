//! Flash a CH32X035-based dongle (Dongle-Pro v2) through the chip's ROM USB
//! loader. The 62 KB of flash has no room for a second image, so an update is:
//! the firmware drops into ROM on `bootsel`, and the host erases, programs and
//! verifies it here, then resets it into the new firmware. The ROM is in mask
//! ROM, so an interrupted update leaves a board that still enters it (hold
//! `boot` while plugging in) and can be flashed again.
//!
//! Protocol (WCH's USB ISP, bulk EP 0x02 out / 0x82 in, one command per
//! packet): `[cmd, len_lo, len_hi, payload..]` out, `[cmd, 0, len_lo, len_hi,
//! payload..]` back. Program and verify data is XORed with an 8-byte key the
//! host and the loader both derive from the chip UID and chip ID.

use crate::error::{Error, Result};
use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::MaybeFuture;
use std::time::{Duration, Instant};

/// The ROM loader's USB IDs (WCH's two vendor IDs, one product ID).
const ISP_VIDS: [u16; 2] = [0x1a86, 0x4348];
const ISP_PID: u16 = 0x55e0;

const CMD_IDENTIFY: u8 = 0xa1;
const CMD_END: u8 = 0xa2;
const CMD_KEY: u8 = 0xa3;
const CMD_ERASE: u8 = 0xa4;
const CMD_PROGRAM: u8 = 0xa5;
const CMD_VERIFY: u8 = 0xa6;
const CMD_READ_CONFIG: u8 = 0xa7;

/// CH32X035: chip ID 0x56 in device family 0x23.
const CHIP_ID: u8 = 0x56;
const DEVICE_TYPE: u8 = 0x23;
/// The application region (62 KB; the top 2 KB is reserved).
pub(crate) const MAX_IMAGE: usize = 62 * 1024;
const SECTOR: usize = 1024;
/// Data bytes per program/verify packet: 64 - 3 header - 4 address - 1 pad.
const CHUNK: usize = 56;
/// Read protection off.
const RDPR_UNPROTECTED: u8 = 0xa5;

const TIMEOUT: Duration = Duration::from_secs(2);
const ERASE_TIMEOUT: Duration = Duration::from_secs(10);

struct Isp {
    out: nusb::Endpoint<Bulk, Out>,
    inp: nusb::Endpoint<Bulk, In>,
}

impl Isp {
    fn command(&mut self, cmd: u8, payload: &[u8], timeout: Duration) -> Result<Vec<u8>> {
        let mut packet = Vec::with_capacity(3 + payload.len());
        packet.push(cmd);
        packet.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        packet.extend_from_slice(payload);
        self.out
            .transfer_blocking(packet.into(), timeout)
            .into_result()
            .map_err(|e| isp_err(cmd, &e.to_string()))?;
        let reply = self
            .inp
            .transfer_blocking(Buffer::new(64), timeout)
            .into_result()
            .map_err(|e| isp_err(cmd, &e.to_string()))?;
        let reply: &[u8] = &reply;
        if reply.len() < 4 || reply[0] != cmd {
            return Err(isp_err(cmd, "malformed reply"));
        }
        let len = usize::from(u16::from_le_bytes([reply[2], reply[3]]));
        Ok(reply[4..(4 + len).min(reply.len())].to_vec())
    }

    /// Commands that answer with a status byte, 0 for success.
    fn checked(&mut self, cmd: u8, payload: &[u8], timeout: Duration) -> Result<()> {
        match self.command(cmd, payload, timeout)?.first() {
            Some(0) => Ok(()),
            Some(code) => Err(isp_err(cmd, &format!("status {code:#04x}"))),
            None => Err(isp_err(cmd, "empty reply")),
        }
    }
}

fn isp_err(cmd: u8, what: &str) -> Error {
    Error::Dongle(format!("ROM loader command {cmd:#04x}: {what}"))
}

/// The XOR key: eight copies of the UID's byte sum, the last plus the chip ID.
fn xor_key(uid: &[u8], chip_id: u8) -> [u8; 8] {
    let sum = uid.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    let mut key = [sum; 8];
    key[7] = key[7].wrapping_add(chip_id);
    key
}

/// Program or verify packets: address, a pad byte, then key-XORed data.
fn data_packets(image: &[u8], key: &[u8; 8]) -> Vec<Vec<u8>> {
    image
        .chunks(CHUNK)
        .enumerate()
        .map(|(i, chunk)| {
            let mut p = Vec::with_capacity(5 + chunk.len());
            p.extend_from_slice(&((i * CHUNK) as u32).to_le_bytes());
            p.push(0);
            p.extend(chunk.iter().enumerate().map(|(j, b)| b ^ key[j % 8]));
            p
        })
        .collect()
}

/// Wait for the ROM loader to enumerate where the dongle's MCU was.
fn find_loader(bus_id: &str, port_chain: &[u8], within: Duration) -> Result<nusb::DeviceInfo> {
    let deadline = Instant::now() + within;
    loop {
        let found = nusb::list_devices().wait().map_err(Error::from)?.find(|i| {
            ISP_VIDS.contains(&i.vendor_id())
                && i.product_id() == ISP_PID
                && i.bus_id() == bus_id
                && i.port_chain() == port_chain
        });
        if let Some(info) = found {
            return Ok(info);
        }
        if Instant::now() >= deadline {
            return Err(Error::Dongle(
                "the dongle did not show up in its ROM USB loader; hold its boot button \
                 while plugging it in, then run the update again"
                    .into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Flash `image` (a raw binary at flash offset 0) into the ROM loader that
/// appears at `bus_id`/`port_chain`, verify it, and reset into it.
/// `progress` gets (bytes written, total).
pub(crate) fn flash(
    bus_id: &str,
    port_chain: &[u8],
    image: &[u8],
    mut progress: impl FnMut(usize, usize),
) -> Result<()> {
    if image.is_empty() || image.len() > MAX_IMAGE {
        return Err(Error::Dongle(format!(
            "firmware image is {} bytes; it must be 1 to {MAX_IMAGE}",
            image.len()
        )));
    }
    let info = find_loader(bus_id, port_chain, Duration::from_secs(10))?;
    let dev = info.open().wait().map_err(Error::from)?;
    // macOS leaves a vendor-class device unconfigured.
    if dev.active_configuration().is_err() {
        dev.set_configuration(1).wait().map_err(Error::from)?;
    }
    let iface = dev.claim_interface(0).wait().map_err(Error::from)?;
    let mut isp = Isp {
        out: iface.endpoint::<Bulk, Out>(0x02).map_err(Error::from)?,
        inp: iface.endpoint::<Bulk, In>(0x82).map_err(Error::from)?,
    };

    let mut identify = vec![0, 0];
    identify.extend_from_slice(b"MCU ISP & WCH.CN");
    let id = isp.command(CMD_IDENTIFY, &identify, TIMEOUT)?;
    if id.get(..2) != Some(&[CHIP_ID, DEVICE_TYPE]) {
        return Err(Error::Dongle(format!(
            "the ROM loader reports chip {id:02x?}, not a CH32X035; not flashing it"
        )));
    }
    // [mask(2), RDPR, USER, DATA0, DATA1, WPR(4), reserved(4), loader version(4), UID(8)]
    let config = isp.command(CMD_READ_CONFIG, &[0x1f, 0x00], TIMEOUT)?;
    if config.len() < 26 {
        return Err(isp_err(CMD_READ_CONFIG, "short reply"));
    }
    if config[2] != RDPR_UNPROTECTED {
        return Err(Error::Dongle(
            "the dongle's flash is read-protected; clear that with wchisp first".into(),
        ));
    }
    let key = xor_key(&config[18..26], CHIP_ID);
    let reply = isp.command(CMD_KEY, &[0; 0x1e], TIMEOUT)?;
    let expect = key.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    if reply.first() != Some(&expect) {
        return Err(isp_err(CMD_KEY, "key mismatch"));
    }

    // Pad to whole 8-byte key cycles with erased-flash bytes.
    let mut padded = image.to_vec();
    padded.resize(image.len().div_ceil(8) * 8, 0xff);
    let sectors = padded.len().div_ceil(SECTOR) as u32;
    isp.checked(CMD_ERASE, &sectors.to_le_bytes(), ERASE_TIMEOUT)?;

    let packets = data_packets(&padded, &key);
    let total = image.len();
    for (i, p) in packets.iter().enumerate() {
        isp.checked(CMD_PROGRAM, p, TIMEOUT)?;
        progress(((i + 1) * CHUNK).min(total), total);
    }
    // An empty program packet at the end address flushes the last page.
    let mut flush = (padded.len() as u32).to_le_bytes().to_vec();
    flush.push(0);
    isp.checked(CMD_PROGRAM, &flush, TIMEOUT)?;

    for p in &packets {
        isp.checked(CMD_VERIFY, p, TIMEOUT).map_err(|_| {
            Error::Dongle("verifying the new firmware failed; run the update again".into())
        })?;
    }
    // Reset into the application. The loader may drop off before it answers.
    let _ = isp.command(CMD_END, &[1], TIMEOUT);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_uid_sum_with_chip_id_last() {
        let uid = [0xcd, 0xab, 0x63, 0xbb, 0xc2, 0xbd, 0xf2, 0x24];
        let sum = uid.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        let key = xor_key(&uid, CHIP_ID);
        assert_eq!(&key[..7], &[sum; 7]);
        assert_eq!(key[7], sum.wrapping_add(CHIP_ID));
    }

    #[test]
    fn packets_fill_a_full_speed_packet() {
        let image = [0x11u8; 120];
        let packets = data_packets(&image, &[0; 8]);
        assert_eq!(packets.len(), 3);
        assert_eq!(3 + packets[0].len(), 64);
        assert_eq!(&packets[1][..4], &56u32.to_le_bytes());
        assert_eq!(packets[2].len(), 5 + 8);
    }
}
