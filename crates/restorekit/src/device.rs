use std::fmt;
use std::time::{Duration, Instant};

use nusb::MaybeFuture;
use serde::Serialize;

use crate::error::{Error, Result};

/// A known model, keyed by chip ID + board ID as reported in the DFU-mode USB
/// serial string: a Mac from [`MAC_MODELS`], or an iPhone, iPad or other device
/// from libirecovery's device database (see [`lookup`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MacModel {
    pub cpid: u16,
    pub bdid: u8,
    /// Board config, e.g. "J313AP".
    pub board: &'static str,
    /// Model identifier, e.g. "MacBookAir10,1" — the key used by firmware APIs.
    pub identifier: &'static str,
    /// Marketing name, e.g. "MacBook Air (M1, Late 2020)".
    pub name: &'static str,
}

/// Generated from <https://api.ipsw.me/v4/devices> (cpid/bdid per board).
pub const MAC_MODELS: &[MacModel] = &[
    MacModel {
        cpid: 0x6000,
        bdid: 0x08,
        board: "J314sAP",
        identifier: "MacBookPro18,3",
        name: "MacBook Pro (M1 Pro, 14-inch, 2021)",
    },
    MacModel {
        cpid: 0x6000,
        bdid: 0x0a,
        board: "J316sAP",
        identifier: "MacBookPro18,1",
        name: "MacBook Pro (M1 Pro, 16-inch, 2021)",
    },
    MacModel {
        cpid: 0x6001,
        bdid: 0x04,
        board: "J375cAP",
        identifier: "Mac13,1",
        name: "Mac Studio (M1 Max)",
    },
    MacModel {
        cpid: 0x6001,
        bdid: 0x08,
        board: "J314cAP",
        identifier: "MacBookPro18,4",
        name: "MacBook Pro (M1 Max, 14-inch, 2021)",
    },
    MacModel {
        cpid: 0x6001,
        bdid: 0x0a,
        board: "J316cAP",
        identifier: "MacBookPro18,2",
        name: "MacBook Pro (M1 Max, 16-inch, 2021)",
    },
    MacModel {
        cpid: 0x6002,
        bdid: 0x0c,
        board: "J375dAP",
        identifier: "Mac13,2",
        name: "Mac Studio (M1 Ultra)",
    },
    MacModel {
        cpid: 0x6020,
        bdid: 0x02,
        board: "J474sAP",
        identifier: "Mac14,12",
        name: "Mac mini (M2 Pro, 2023)",
    },
    MacModel {
        cpid: 0x6020,
        bdid: 0x04,
        board: "J414sAP",
        identifier: "Mac14,9",
        name: "MacBook Pro (M2 Pro, 14-inch, 2023)",
    },
    MacModel {
        cpid: 0x6020,
        bdid: 0x06,
        board: "J416sAP",
        identifier: "Mac14,10",
        name: "MacBook Pro (M2 Pro, 16-inch, 2023)",
    },
    MacModel {
        cpid: 0x6021,
        bdid: 0x04,
        board: "J414cAP",
        identifier: "Mac14,5",
        name: "MacBook Pro (M2 Max, 14-inch, 2023)",
    },
    MacModel {
        cpid: 0x6021,
        bdid: 0x06,
        board: "J416cAP",
        identifier: "Mac14,6",
        name: "MacBook Pro (M2 Max, 16-inch, 2023)",
    },
    MacModel {
        cpid: 0x6021,
        bdid: 0x0a,
        board: "J475cAP",
        identifier: "Mac14,13",
        name: "Mac Studio (M2 Max, 2023)",
    },
    MacModel {
        cpid: 0x6022,
        bdid: 0x08,
        board: "J180dAP",
        identifier: "Mac14,8",
        name: "Mac Pro (2023)",
    },
    MacModel {
        cpid: 0x6022,
        bdid: 0x0a,
        board: "J475dAP",
        identifier: "Mac14,14",
        name: "Mac Studio (M2 Ultra, 2023)",
    },
    MacModel {
        cpid: 0x6030,
        bdid: 0x04,
        board: "J514sAP",
        identifier: "Mac15,6",
        name: "MacBook Pro (M3 Pro, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6030,
        bdid: 0x06,
        board: "J516sAP",
        identifier: "Mac15,7",
        name: "MacBook Pro (M3 Pro, 16-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6031,
        bdid: 0x44,
        board: "J514cAP",
        identifier: "Mac15,8",
        name: "MacBook Pro (M3 Max, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6031,
        bdid: 0x46,
        board: "J516cAP",
        identifier: "Mac15,9",
        name: "MacBook Pro (M3 Max, 16-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6032,
        bdid: 0x44,
        board: "J575dAP",
        identifier: "Mac15,14",
        name: "Mac Studio (2025)",
    },
    MacModel {
        cpid: 0x6034,
        bdid: 0x44,
        board: "J514mAP",
        identifier: "Mac15,10",
        name: "MacBook Pro (M3 Max, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6034,
        bdid: 0x46,
        board: "J516mAP",
        identifier: "Mac15,11",
        name: "MacBook Pro (M3 Max, 16-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6040,
        bdid: 0x02,
        board: "J773sAP",
        identifier: "Mac16,11",
        name: "Mac mini (M4 Pro, 2024)",
    },
    MacModel {
        cpid: 0x6040,
        bdid: 0x04,
        board: "J614sAP",
        identifier: "Mac16,8",
        name: "MacBook Pro (M4 Pro, 14-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6040,
        bdid: 0x06,
        board: "J616sAP",
        identifier: "Mac16,7",
        name: "MacBook Pro (M4 Pro, 16-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6041,
        bdid: 0x02,
        board: "J575cAP",
        identifier: "Mac16,9",
        name: "Mac Studio (2025)",
    },
    MacModel {
        cpid: 0x6041,
        bdid: 0x04,
        board: "J614cAP",
        identifier: "Mac16,6",
        name: "MacBook Pro (M4 Max, 14-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6041,
        bdid: 0x06,
        board: "J616cAP",
        identifier: "Mac16,5",
        name: "MacBook Pro (M4 Max, 16-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x02,
        board: "J873sAP",
        identifier: "Mac17,16",
        name: "Mac mini (M5 Pro, 2026)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x04,
        board: "K114cAP",
        identifier: "Mac17,11",
        name: "MacBook Pro (14-inch PROLED, M5 Max, 2026)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x06,
        board: "K116cAP",
        identifier: "Mac17,10",
        name: "MacBook Pro (16-inch PROLED, M5 Max, 2026)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x08,
        board: "J714sAP",
        identifier: "Mac17,9",
        name: "MacBook Pro (14-inch, M5 Pro)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x0a,
        board: "J714cAP",
        identifier: "Mac17,7",
        name: "MacBook Pro (14-inch, M5 Max)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x0c,
        board: "J716sAP",
        identifier: "Mac17,8",
        name: "MacBook Pro (16-inch, M5 Pro)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x0e,
        board: "J716cAP",
        identifier: "Mac17,6",
        name: "MacBook Pro (16-inch, M5 Max)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x10,
        board: "J775cAP",
        identifier: "Mac17,14",
        name: "Mac Studio (M5 Max, 2026)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x12,
        board: "J775dAP",
        identifier: "Mac17,15",
        name: "Mac Studio (M5 Ultra, 2026)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x14,
        board: "K114sAP",
        identifier: "Mac17,13",
        name: "MacBook Pro (14-inch PROLED, M5 Pro, 2026)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x16,
        board: "K116sAP",
        identifier: "Mac17,12",
        name: "MacBook Pro (16-inch PROLED, M5 Pro, 2026)",
    },
    // Intel Macs with the Apple T2 (CPID 0x8012). In DFU the T2 reports this
    // CPID/BDID; `identifier` is the T2 bridge product type (`iBridge2,x`) — the
    // key ipsw.me/idevicerestore use for a T2 (bridgeOS) restore, not the host
    // Mac's model — and `board`/`name` are the T2 board and the host Mac model.
    // Board IDs from AsahiLinux/libirecovery.
    MacModel {
        cpid: 0x8012,
        bdid: 0x0a,
        board: "J137AP",
        identifier: "iBridge2,1",
        name: "iMac Pro (2017)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x0b,
        board: "J680AP",
        identifier: "iBridge2,3",
        name: "MacBook Pro (15-inch, 2018/2019)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x0c,
        board: "J132AP",
        identifier: "iBridge2,4",
        name: "MacBook Pro (13-inch, 2018/2019, Four Thunderbolt 3 ports)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x0e,
        board: "J174AP",
        identifier: "iBridge2,5",
        name: "Mac mini (2018)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x0f,
        board: "J160AP",
        identifier: "iBridge2,6",
        name: "Mac Pro (2019)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x07,
        board: "J780AP",
        identifier: "iBridge2,7",
        name: "MacBook Pro (15-inch, 2019)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x17,
        board: "J140KAP",
        identifier: "iBridge2,8",
        name: "MacBook Air (Retina, 13-inch, 2018)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x18,
        board: "J213AP",
        identifier: "iBridge2,10",
        name: "MacBook Pro (13-inch, 2019, Two Thunderbolt 3 ports)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x37,
        board: "J140AAP",
        identifier: "iBridge2,12",
        name: "MacBook Air (Retina, 13-inch, 2019)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x3a,
        board: "J152FAP",
        identifier: "iBridge2,14",
        name: "MacBook Pro (16-inch, 2019)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x3f,
        board: "J230KAP",
        identifier: "iBridge2,15",
        name: "MacBook Air (Retina, 13-inch, 2020)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x3e,
        board: "J214KAP",
        identifier: "iBridge2,16",
        name: "MacBook Pro (13-inch, 2020, Four Thunderbolt 3 ports)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x22,
        board: "J185AP",
        identifier: "iBridge2,19",
        name: "iMac (Retina 5K, 27-inch, 2020)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x23,
        board: "J185FAP",
        identifier: "iBridge2,20",
        name: "iMac (Retina 5K, 27-inch, 2020)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x3b,
        board: "J223AP",
        identifier: "iBridge2,21",
        name: "MacBook Pro (13-inch, 2020, Two Thunderbolt 3 ports)",
    },
    MacModel {
        cpid: 0x8012,
        bdid: 0x38,
        board: "J215AP",
        identifier: "iBridge2,22",
        name: "MacBook Pro (16-inch, 2019)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x22,
        board: "J274AP",
        identifier: "Macmini9,1",
        name: "Mac mini (M1, Late 2020)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x24,
        board: "J293AP",
        identifier: "MacBookPro17,1",
        name: "MacBook Pro (M1, Late 2020)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x26,
        board: "J313AP",
        identifier: "MacBookAir10,1",
        name: "MacBook Air (M1, Late 2020)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x28,
        board: "J456AP",
        identifier: "iMac21,1",
        name: "iMac 24-inch (M1, Two Ports, 2021)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x2a,
        board: "J457AP",
        identifier: "iMac21,2",
        name: "iMac 24-inch (M1, Four Ports, 2021)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x24,
        board: "J473AP",
        identifier: "Mac14,3",
        name: "Mac mini (M2, 2023)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x28,
        board: "J413AP",
        identifier: "Mac14,2",
        name: "MacBook Air (M2, 2022)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x2a,
        board: "J493AP",
        identifier: "Mac14,7",
        name: "MacBook Pro (13-inch, M2, 2022)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x2e,
        board: "J415AP",
        identifier: "Mac14,15",
        name: "MacBook Air (15-inch, M2, 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x22,
        board: "J504AP",
        identifier: "Mac15,3",
        name: "MacBook Pro (M3, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x28,
        board: "J433AP",
        identifier: "Mac15,4",
        name: "iMac (Two Ports, 24-inch, 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x2a,
        board: "J434AP",
        identifier: "Mac15,5",
        name: "iMac (Four Ports, 24-inch, 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x30,
        board: "J613AP",
        identifier: "Mac15,12",
        name: "MacBook Air (13-inch, M3, 2024)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x32,
        board: "J615AP",
        identifier: "Mac15,13",
        name: "MacBook Air (15-inch, M3, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x22,
        board: "J604AP",
        identifier: "Mac16,1",
        name: "MacBook Pro (M4, 14-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x24,
        board: "J623AP",
        identifier: "Mac16,2",
        name: "iMac (Two Ports, 24-inch, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x26,
        board: "J624AP",
        identifier: "Mac16,3",
        name: "iMac (Four Ports, 24-inch, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x2a,
        board: "J773gAP",
        identifier: "Mac16,10",
        name: "Mac mini (M4, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x2c,
        board: "J713AP",
        identifier: "Mac16,12",
        name: "MacBook Air (13-inch, M4, 2025)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x2e,
        board: "J715AP",
        identifier: "Mac16,13",
        name: "MacBook Air (15-inch, M4, 2025)",
    },
    MacModel {
        cpid: 0x8140,
        bdid: 0x64,
        board: "J700AP",
        identifier: "Mac17,5",
        name: "MacBook Neo",
    },
    MacModel {
        cpid: 0x8142,
        bdid: 0x22,
        board: "J704AP",
        identifier: "Mac17,2",
        name: "MacBook Pro (14-inch, M5)",
    },
    MacModel {
        cpid: 0x8142,
        bdid: 0x24,
        board: "J813AP",
        identifier: "Mac17,3",
        name: "MacBook Air (13-inch, M5)",
    },
    MacModel {
        cpid: 0x8142,
        bdid: 0x26,
        board: "J815AP",
        identifier: "Mac17,4",
        name: "MacBook Air (15-inch, M5)",
    },
    MacModel {
        cpid: 0x8152,
        bdid: 0x22,
        board: "J804AP",
        identifier: "Mac18,1",
        name: "MacBook Pro (14-inch, M6, 2026)",
    },
    MacModel {
        cpid: 0x8152,
        bdid: 0x24,
        board: "J873gAP",
        identifier: "Mac18,5",
        name: "Mac mini (M6, 2026)",
    },
    MacModel {
        cpid: 0x8152,
        bdid: 0x26,
        board: "J833ctAP",
        identifier: "Mac18,2",
        name: "iMac (24-inch, M6, 2026)",
    },
];

/// The model with this chip and board ID: a Mac from [`MAC_MODELS`], else
/// anything libirecovery's device database knows, such as an iPhone or iPad.
pub fn lookup(cpid: u16, bdid: u8) -> Option<&'static MacModel> {
    let matches = |m: &&MacModel| m.cpid == cpid && m.bdid == bdid;
    MAC_MODELS
        .iter()
        .find(matches)
        .or_else(|| known_devices().iter().find(matches))
}

pub fn lookup_identifier(identifier: &str) -> Option<&'static MacModel> {
    let matches = |m: &&MacModel| m.identifier.eq_ignore_ascii_case(identifier);
    MAC_MODELS
        .iter()
        .find(matches)
        .or_else(|| known_devices().iter().find(matches))
}

/// libirecovery's device database, read once, in [`MacModel`]'s shape.
fn known_devices() -> &'static [MacModel] {
    static DEVICES: std::sync::OnceLock<Vec<MacModel>> = std::sync::OnceLock::new();
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    DEVICES.get_or_init(|| {
        restorekit_sys::known_devices()
            .into_iter()
            .filter_map(|d| {
                Some(MacModel {
                    cpid: u16::try_from(d.chip_id).ok()?,
                    bdid: u8::try_from(d.board_id).ok()?,
                    board: leak(d.hardware_model.to_ascii_uppercase()),
                    identifier: leak(d.product_type),
                    name: leak(d.display_name),
                })
            })
            .collect()
    })
}

/// What to call a device with this model identifier: "Mac", "iPhone",
/// "iPad", and so on, or "device" when the identifier says nothing.
pub fn kind(identifier: &str) -> &'static str {
    const KINDS: &[(&str, &str)] = &[
        ("iPhone", "iPhone"),
        ("iPad", "iPad"),
        ("iPod", "iPod"),
        ("AppleTV", "Apple TV"),
        ("Watch", "Apple Watch"),
        ("AudioAccessory", "HomePod"),
        ("RealityDevice", "Apple Vision Pro"),
        ("iBridge", "Mac"),
        ("iMac", "Mac"),
        ("Mac", "Mac"),
    ];
    KINDS
        .iter()
        .find(|(prefix, _)| identifier.starts_with(prefix))
        .map_or("device", |(_, kind)| kind)
}

/// Resolve a Mac model from the USB `bcdDevice` release number. A booted Mac
/// encodes its model identifier's numeric part there in BCD — e.g. `0x1701` →
/// "17,1" → `MacBookPro17,1`, `0x1606` → "16,6" → `Mac16,6`. The numeric part
/// is not unique across prefixes (`MacBookPro18,1` and `Mac18,1` both read
/// `0x1801`), so this returns `None` when it matches more than one model; use
/// [`model_from_usb`] to break the tie with the product ID.
pub fn model_from_device_version(bcd: u16) -> Option<&'static MacModel> {
    match models_with_number(bcd).as_slice() {
        [m] => Some(m),
        _ => None,
    }
}

/// Resolve a booted Mac's model from its USB product ID and `bcdDevice`. A
/// unique numeric part resolves on its own; a clash is broken by the identifier
/// prefix the product ID implies (see [`booted_pid_prefix`]), or `None` if that
/// doesn't pick exactly one.
pub fn model_from_usb(pid: u16, bcd: u16) -> Option<&'static MacModel> {
    let candidates = models_with_number(bcd);
    if let [m] = candidates.as_slice() {
        return Some(m);
    }
    let prefix = booted_pid_prefix(pid)?;
    let mut hits = candidates
        .into_iter()
        .filter(|m| identifier_prefix(m.identifier) == prefix);
    match (hits.next(), hits.next()) {
        (Some(m), None) => Some(m),
        _ => None,
    }
}

/// The model-identifier prefix of each booted-Mac product ID. Unverified
/// against hardware for anything but `0x1902`; only used to break clashes.
fn booted_pid_prefix(pid: u16) -> Option<&'static str> {
    Some(match pid {
        0x1901 => "Macmini",
        0x1902 => "MacBookPro",
        0x1903 => "MacBookAir",
        0x1904 => "iMac",
        0x1905 => "Mac",
        _ => return None,
    })
}

/// Split an identifier into its alphabetic prefix and numeric part.
fn identifier_prefix(identifier: &str) -> &str {
    identifier.trim_end_matches(|c: char| c.is_ascii_digit() || c == ',')
}

fn models_with_number(bcd: u16) -> Vec<&'static MacModel> {
    let bcd_byte = |b: u16| (b >> 4) * 10 + (b & 0xf);
    let number = format!("{},{}", bcd_byte(bcd >> 8), bcd_byte(bcd & 0xff));
    MAC_MODELS
        .iter()
        .filter(|m| m.identifier[identifier_prefix(m.identifier).len()..] == number)
        .collect()
}

/// Apple's USB vendor ID.
pub const APPLE_VID: u16 = 0x05ac;
/// Product ID presented by an Apple SoC in DFU mode.
pub const DFU_PID: u16 = 0x1227;
/// Product ID of a Mac in DFU over Apple's Debug USB interface ("KIS").
pub const KIS_PID: u16 = 0x1881;
/// Product ID presented by a Mac's USB-C controller in Port DFU mode.
pub const PORT_DFU_PID: u16 = 0xf014;
/// Product ID a Mac's USB-C port controller presents as a USB Billboard device
/// when an alternate mode (e.g. Thunderbolt) fails to enter over the link.
pub const BILLBOARD_PID: u16 = 0x7304;
/// Chip ID (`CPID`) of the Apple T2, reported in the DFU serial by Intel Macs.
/// A T2 restore reinstalls bridgeOS, not macOS (see the erase warning).
pub const T2_CPID: u16 = 0x8012;

/// The USB mode a connected Apple device is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsbMode {
    /// Normal SoC DFU mode.
    Dfu,
    /// Port DFU mode. The USB-C controller must be updated before the Mac
    /// reconnects in normal SoC DFU mode and the restore continues.
    #[serde(rename = "port-dfu")]
    PortDfu,
    /// DFU over Apple's Debug USB interface ("KIS", `05ac:1881`): the same
    /// bootrom DFU, but the chip identity sits behind a KIS request instead of
    /// the USB serial. macOS only lets Apple's own `deviceinterfaced` open it,
    /// so on a macOS host the request goes through that daemon.
    Kis,
    /// Recovery mode (iBoot).
    Recovery,
    /// WTF ("what's the firmware") mode.
    Wtf,
    /// Mid-restore ramdisk.
    Restore,
    /// A Mac booted into macOS. It exposes a USB gadget to a cabled host (the
    /// RemoteXPC/NCM sidecar) advertising its Apple serial number; its exact
    /// model and ECID are filled in separately by [`identify`].
    Booted,
    /// A Mac's USB-C port controller presenting a USB Billboard device — the
    /// fallback a port exposes after an alternate mode (e.g. Thunderbolt)
    /// fails to enter, as happens over a USB 2.0-only dongle. This is the port
    /// silicon talking, not the Mac's SoC: it carries no Apple serial, model,
    /// or ECID, only the controller's own UID and the alt-mode SVID it tried.
    /// Put the Mac in DFU to read its real identity.
    Billboard,
    /// Any other Apple USB device (e.g. an iPhone in normal mode).
    Other,
}

impl UsbMode {
    fn from_pid(pid: u16) -> UsbMode {
        match pid {
            DFU_PID => UsbMode::Dfu,
            PORT_DFU_PID => UsbMode::PortDfu,
            KIS_PID => UsbMode::Kis,
            0x1280..=0x1283 => UsbMode::Recovery,
            0x1222 => UsbMode::Wtf,
            0x12ac => UsbMode::Restore,
            // The RemoteXPC/NCM gadget a booted macOS exposes to a USB host,
            // by model family: Mac mini, MacBook Pro, MacBook Air, iMac, then
            // every MacNN,N (order unverified; see booted_pid_prefix).
            0x1901..=0x1905 => UsbMode::Booted,
            // The USB Billboard a Mac's USB-C port controller presents when an
            // alt mode (Thunderbolt) can't be entered over the link.
            BILLBOARD_PID => UsbMode::Billboard,
            _ => UsbMode::Other,
        }
    }
}

impl fmt::Display for UsbMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UsbMode::Dfu => "dfu",
            UsbMode::PortDfu => "port-dfu",
            UsbMode::Kis => "kis",
            UsbMode::Recovery => "recovery",
            UsbMode::Wtf => "wtf",
            UsbMode::Restore => "restore",
            UsbMode::Booted => "booted",
            UsbMode::Billboard => "billboard",
            UsbMode::Other => "other",
        })
    }
}

/// Restore-family chip identity parsed from the `CPID:… BDID:…` USB serial
/// string Apple SoCs expose in DFU/recovery/restore modes. The ECID lives on
/// [`Device`] itself, since it is also obtainable for a booted Mac.
#[derive(Debug, Clone, Serialize)]
pub struct Identity {
    pub cpid: u16,
    pub bdid: u8,
    /// iBoot version tag (`SRTG`), when present.
    pub srtg: Option<String>,
    /// Resolved model, if the (cpid, bdid) pair is known.
    pub model: Option<MacModel>,
}

/// A connected Apple USB device, in whatever mode it is in.
///
/// Macs in a restore-family mode carry a parsed [`Identity`] (chip/board/model)
/// plus the ECID, all from the USB serial string. A Mac booted into macOS
/// enumerates as [`UsbMode::Booted`] with its name and Apple serial but no
/// [`Identity`]; its ECID can still be filled in by [`identify`]. Older targets
/// may not enumerate at all while booted.
#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub mode: UsbMode,
    /// USB product string, e.g. "Apple Mobile Device (DFU Mode)".
    pub product: String,
    /// Raw USB serial string (empty if the device exposes none). For a
    /// [`UsbMode::Kis`] device, the identity string read over KIS instead.
    pub serial: String,
    /// The device's ECID, if known: parsed from the serial in restore modes, or
    /// filled in for a booted Mac by [`identify`].
    pub ecid: Option<u64>,
    /// The hardware serial number (e.g. "C02XX…"), when the device exposes one:
    /// iBoot's `SRNM` in recovery mode, or the raw USB serial when booted. DFU
    /// mode usually omits it.
    pub srnm: Option<String>,
    /// Restore-family chip identity; present in DFU/recovery/restore, else None.
    pub identity: Option<Identity>,
    /// Exact Apple marketing name (e.g. "MacBook Pro (13-inch, M1, 2020)"),
    /// looked up from the serial by [`identify`]; falls back to the model's
    /// generic name in [`Device::display_name`] when absent.
    pub marketing_name: Option<String>,
    /// The host port this device is cabled to, filled in by [`identify`].
    /// `None` when undeterminable (non-macOS host, or the topology couldn't be
    /// read).
    pub port: Option<Port>,
    /// For a [`UsbMode::Billboard`] device: the preferred alternate-mode SVID
    /// its port controller advertised (e.g. `0x8087` Thunderbolt), read from
    /// the Billboard capability descriptor by [`identify`]. `None` until read,
    /// or when the device can't be opened.
    pub billboard_svid: Option<u16>,
    /// Raw USB descriptor identifiers (VID/PID, release, class triple) — the
    /// low-level facts `lsusb`/`system_profiler` show, surfaced for any device.
    pub usb: UsbId,
    /// Whether the OS driver lets restorekit open this device. Always true on
    /// macOS/Linux; on Windows, false until WinUSB is bound.
    pub driver_ready: bool,
    /// The bus entry this was read from, to find it again: in KIS mode
    /// `serial` is the identity read over KIS, not the USB serial.
    #[serde(skip)]
    pub(crate) usb_device: Option<nusb::DeviceId>,
}

/// Raw USB device-descriptor identifiers, straight off the wire.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct UsbId {
    /// `idVendor` (always `0x05ac` for the Apple devices restorekit lists).
    pub vid: u16,
    /// `idProduct`.
    pub pid: u16,
    /// `bcdDevice` — the device release number (BCD, e.g. `0x2100` = "21.00").
    pub device_version: u16,
    /// `bDeviceClass` (e.g. `0x11` Billboard, `0x00` per-interface, `0xff`
    /// vendor-specific).
    pub class: u8,
    /// `bDeviceSubClass`.
    pub subclass: u8,
    /// `bDeviceProtocol`.
    pub protocol: u8,
}

impl UsbId {
    /// A short name for the USB device class, when it's one restorekit expects.
    pub fn class_name(&self) -> Option<&'static str> {
        match self.class {
            0x00 => Some("per-interface"),
            0x09 => Some("hub"),
            0x11 => Some("billboard"),
            0xef => Some("misc"),
            0xff => Some("vendor-specific"),
            _ => None,
        }
    }

    /// `bcdDevice` formatted the way descriptors read it, e.g. `0x2100` → "21.00".
    pub fn version_str(&self) -> String {
        format!(
            "{:x}.{:02x}",
            self.device_version >> 8,
            self.device_version & 0xff
        )
    }

    /// One-line summary: `05ac:7304 (class 0x11 billboard, rev 21.00)`.
    pub fn summary(&self) -> String {
        let class = match self.class_name() {
            Some(name) => format!("class 0x{:02x} {name}", self.class),
            None => format!("class 0x{:02x}", self.class),
        };
        format!(
            "{:04x}:{:04x} ({class}, rev {})",
            self.vid,
            self.pid,
            self.version_str()
        )
    }
}

/// The host USB-C port a [`Device`] is cabled to.
#[derive(Debug, Clone, Serialize)]
pub struct Port {
    /// Whether this is the host's DFU-capable port — the one restorekit
    /// triggers DFU on. If `false`, move the cable to a DFU port to restore.
    pub dfu: bool,
    /// The port's physical location label from the firmware, e.g. "left-back",
    /// when the hardware provides one.
    pub location: Option<String>,
    /// The AppleHPM `RID` addressing this port — the value `dfu`/`reboot` take
    /// via `--port`.
    pub rid: i32,
}

impl Device {
    /// Whether `info` is the bus entry this device was read from. Matches by
    /// serial only for a device built without one.
    pub(crate) fn is_usb(&self, info: &nusb::DeviceInfo) -> bool {
        match self.usb_device {
            Some(id) => info.id() == id,
            None => {
                info.vendor_id() == self.usb.vid
                    && info.serial_number() == Some(self.serial.as_str())
            }
        }
    }

    pub fn in_dfu(&self) -> bool {
        matches!(self.mode, UsbMode::Dfu | UsbMode::PortDfu | UsbMode::Kis)
    }

    /// Whether restorekit can restore this device right now (normal, Port, or
    /// KIS DFU mode with a readable identity).
    pub fn restorable(&self) -> bool {
        self.in_dfu() && self.identity.is_some()
    }

    /// Whether a restore can start from where this device is now: a mode
    /// [`restorable`](Self::restorable) takes, or recovery mode for an iPhone
    /// or iPad, which the restore engine takes as is. A Mac in recovery goes
    /// to DFU first.
    pub fn ready_to_restore(&self) -> bool {
        self.restorable()
            || (self.mode == UsbMode::Recovery && self.is_ios() && self.ecid.is_some())
    }

    /// A Mac in Debug USB DFU whose identity couldn't be read. It's in DFU, but
    /// restorekit can't restore it.
    pub fn kis_unreadable(&self) -> bool {
        self.mode == UsbMode::Kis && self.identity.is_none()
    }

    /// A Debug USB (`05ac:1881`) device that isn't in DFU: it answered, but the
    /// chip behind it isn't running the boot ROM, so there's no identity. Seen
    /// with a booted device whose port was switched to Debug USB over USB-PD.
    pub fn debug_usb_not_in_dfu(&self) -> bool {
        self.mode == UsbMode::Other && self.usb.pid == KIS_PID
    }

    /// ECID formatted as idevicerestore expects it (hex with 0x prefix).
    pub fn ecid_hex(&self) -> Option<String> {
        self.ecid.map(|e| format!("0x{e:x}"))
    }

    /// Model identifier (e.g. "MacBookAir10,1"), if the model is known.
    pub fn identifier(&self) -> Option<&'static str> {
        self.identity.as_ref()?.model.map(|m| m.identifier)
    }

    /// What to call this device: "Mac", "iPhone", "iPad", and so on (see
    /// [`kind`]), or "device" when its model is unknown.
    pub fn kind(&self) -> &'static str {
        self.identifier().map_or("device", kind)
    }

    /// Whether this is an iPhone, iPad or iPod, by its model.
    pub fn is_ios(&self) -> bool {
        matches!(self.kind(), "iPhone" | "iPad" | "iPod")
    }

    /// iBoot version (`SRTG`), if the serial carried one.
    pub fn srtg(&self) -> Option<&str> {
        self.identity.as_ref()?.srtg.as_deref()
    }

    /// Whether this is a Mac's USB-C port controller presenting a billboard —
    /// a mode with no readable SoC identity (see [`UsbMode::Billboard`]).
    pub fn is_billboard(&self) -> bool {
        self.mode == UsbMode::Billboard
    }

    /// Whether this is an Intel Mac's Apple T2 (a bridgeOS restore target). A T2
    /// DFU restore only reinstalls bridgeOS, not macOS — see the erase warning.
    pub fn is_t2(&self) -> bool {
        self.identity.as_ref().is_some_and(|i| i.cpid == T2_CPID)
    }

    /// For a billboard device, a human label for the alternate mode the port
    /// tried to enter, from its advertised SVID. `None` if not yet read.
    pub fn billboard_alt_mode(&self) -> Option<String> {
        self.billboard_svid.map(|svid| match svid {
            0x8087 => format!("SVID 0x{svid:04x} (Thunderbolt)"),
            0x05ac => format!("SVID 0x{svid:04x} (Apple)"),
            _ => format!("SVID 0x{svid:04x}"),
        })
    }

    /// Human-readable name for display: the exact Apple marketing name when
    /// known, else the resolved model's generic name, else the USB product name.
    pub fn display_name(&self) -> String {
        if let Some(name) = &self.marketing_name {
            return name.clone();
        }
        match &self.identity {
            Some(Identity { model: Some(m), .. }) => m.name.to_string(),
            Some(i) => format!("Unknown Mac (CPID:{:04x} BDID:{:02x})", i.cpid, i.bdid),
            None => self.product.clone(),
        }
    }
}

/// Parse the restore-family USB serial string.
///
/// Example:
/// `CPID:8103 CPFM:03 SCEP:01 BDID:26 ECID:000C60A812345678 IBFL:3C SRTG:[iBoot-7429.61.2]`
///
/// Fields are space-separated `KEY:VALUE`; numeric values are hex, `SRTG` and
/// `SRNM` are bracketed. Missing optional fields are tolerated; CPID/BDID/ECID
/// are required. Returns `(cpid, bdid, ecid, srtg, srnm)`.
#[allow(clippy::type_complexity)]
pub fn parse_serial(serial: &str) -> Option<(u16, u8, u64, Option<String>, Option<String>)> {
    let mut cpid = None;
    let mut bdid = None;
    let mut ecid = None;
    let mut srtg = None;
    let mut srnm = None;

    for field in serial.split_whitespace() {
        let (key, value) = field.split_once(':')?;
        let bracketed = || {
            value
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string()
        };
        match key {
            "CPID" => cpid = u16::from_str_radix(value, 16).ok(),
            "BDID" => bdid = u8::from_str_radix(value, 16).ok(),
            "ECID" => ecid = u64::from_str_radix(value, 16).ok(),
            "SRTG" => srtg = Some(bracketed()),
            "SRNM" => srnm = Some(bracketed()),
            _ => {}
        }
    }

    Some((cpid?, bdid?, ecid?, srtg, srnm))
}

/// Port DFU reports the target Mac's chip and board IDs packed into its wide
/// `BDID` field. Decode it the same way libirecovery does when selecting the
/// device model; the separate `CPID` belongs to the USB-C controller.
#[allow(clippy::type_complexity)]
fn parse_port_dfu_serial(serial: &str) -> Option<(u16, u8, u64, Option<String>, Option<String>)> {
    let mut packed_bdid = None;
    let mut ecid = None;

    for field in serial.split_whitespace() {
        let Some((key, value)) = field.split_once(':') else {
            continue;
        };
        match key {
            "BDID" => packed_bdid = u64::from_str_radix(value, 16).ok(),
            "ECID" => ecid = u64::from_str_radix(value, 16).ok(),
            _ => {}
        }
    }

    let packed_bdid = packed_bdid?;
    let cpid = ((packed_bdid >> 8) & 0xffff) as u16;
    let bdid = ((packed_bdid >> 24) & 0xff) as u8;
    Some((cpid, bdid, ecid?, None, None))
}

/// Some Macs (booted / "other" mode) expose a `CPID-ECID` USB serial like
/// `00008103-000445E11462001E` — 8 hex CPID, a dash, then the 16-hex ECID.
/// Pull the ECID out of it when the richer `parse_serial` finds nothing.
fn ecid_from_usb_serial(serial: &str) -> Option<u64> {
    let (cpid, ecid) = serial.split_once('-')?;
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    if hex(cpid, 8) && hex(ecid, 16) {
        u64::from_str_radix(ecid, 16).ok()
    } else {
        None
    }
}

/// Describe one USB device from the caller's own nusb enumeration, the way
/// [`list`] describes every Apple device on the bus. For callers that track
/// devices by where they are attached and need the identity of a particular
/// one. A KIS identity is cached per arrival; see [`forget_absent`]. A
/// non-Apple device is described as [`UsbMode::Other`] without being opened.
///
/// Blocking: reading a KIS device does USB I/O, and right after it arrives
/// retries for up to a few seconds while the host finishes attaching it. Don't
/// call it from a hotplug callback or an async task.
pub fn from_usb(info: &nusb::DeviceInfo) -> Device {
    let apple = info.vendor_id() == APPLE_VID;
    let mut mode = if apple {
        UsbMode::from_pid(info.product_id())
    } else {
        UsbMode::Other
    };
    let serial = match mode {
        UsbMode::Kis => match crate::kis::identify(info) {
            crate::kis::Read::Serial(s) => Some(s),
            // Debug USB without DFU behind it isn't a restore mode.
            crate::kis::Read::NotInDfu => {
                mode = UsbMode::Other;
                None
            }
            crate::kis::Read::Failed => None,
        },
        _ => info.serial_number().map(str::to_string),
    }
    .unwrap_or_default();
    let parsed = if !apple {
        None
    } else if mode == UsbMode::PortDfu {
        parse_port_dfu_serial(&serial)
    } else {
        parse_serial(&serial)
    };
    let ecid = parsed
        .as_ref()
        .map(|(_, _, ecid, _, _)| *ecid)
        .or_else(|| apple.then(|| ecid_from_usb_serial(&serial)).flatten());
    // The hardware serial from iBoot's SRNM (recovery), else the raw USB serial
    // for a booted Mac (which already is the Apple serial).
    let srnm = parsed
        .as_ref()
        .and_then(|(_, _, _, _, srnm)| srnm.clone())
        .or_else(|| (mode == UsbMode::Booted && !serial.is_empty()).then(|| serial.clone()));
    let identity = parsed.map(|(cpid, bdid, _, srtg, _)| Identity {
        cpid,
        bdid,
        srtg,
        model: lookup(cpid, bdid).copied(),
    });
    Device {
        driver_ready: driver_ready(info, mode),
        mode,
        product: info.product_string().unwrap_or("Apple device").to_string(),
        serial,
        ecid,
        srnm,
        identity,
        marketing_name: None,
        port: None,
        billboard_svid: None,
        usb: UsbId {
            vid: info.vendor_id(),
            pid: info.product_id(),
            device_version: info.device_version(),
            class: info.class(),
            subclass: info.subclass(),
            protocol: info.protocol(),
        },
        usb_device: Some(info.id()),
    }
}

/// Whether the OS driver lets restorekit reach this device: WinUSB, or Apple's
/// own DFU (`WUDFRd`) and KIS drivers, which libirecovery's Windows backend
/// talks to directly. Never poke a normal device.
#[cfg(target_os = "windows")]
fn driver_ready(info: &nusb::DeviceInfo, mode: UsbMode) -> bool {
    let apple = info
        .driver()
        .is_some_and(|d| d.eq_ignore_ascii_case("WUDFRd") || d.eq_ignore_ascii_case("AppleKIS"));
    match mode {
        UsbMode::Other => true,
        _ => apple || info.open().wait().is_ok(),
    }
}
#[cfg(not(target_os = "windows"))]
fn driver_ready(_info: &nusb::DeviceInfo, _mode: UsbMode) -> bool {
    true
}

/// Drop the identities [`from_usb`] cached for devices that have left the bus.
/// Pass the id of every device attached now, after each enumeration; [`list`]
/// does this itself. A caller describing devices one at a time must call it.
pub fn forget_absent(present: &[nusb::DeviceId]) {
    crate::kis::forget_absent(present);
}

/// List every connected Apple USB device, in any mode. Cheap enumeration only;
/// call [`identify`] to fill in the richer fields (model, ECID, marketing name,
/// DFU-port).
pub fn list() -> Result<Vec<Device>> {
    let infos: Vec<nusb::DeviceInfo> = nusb::list_devices()
        .wait()
        .map_err(Error::from)?
        .filter(|i| i.vendor_id() == APPLE_VID)
        .collect();
    let ids: Vec<nusb::DeviceId> = infos.iter().map(nusb::DeviceInfo::id).collect();
    forget_absent(&ids);
    Ok(infos.iter().map(from_usb).collect())
}

/// Which device to pick.
#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// The only device in DFU mode — the implied restore target; it is an
    /// error if several are connected. An iPhone or iPad in recovery mode
    /// doesn't count (see [`Device::ready_to_restore`]).
    One,
    /// The device with this ECID, in any mode that exposes one.
    Ecid(u64),
}

/// Pick the target from `devices`. `Ok(None)` means "not there (yet)";
/// `Err` means the target can never match (ambiguous).
fn select(devices: Vec<Device>, target: Target) -> Result<Option<Device>> {
    match target {
        Target::One => {
            let mut dfu: Vec<Device> = devices.into_iter().filter(|d| d.restorable()).collect();
            match dfu.len() {
                0 | 1 => Ok(dfu.pop()),
                n => Err(Error::MultipleDfuDevices(n)),
            }
        }
        Target::Ecid(e) => Ok(devices.into_iter().find(|d| d.ecid == Some(e))),
    }
}

/// Find the target among the currently connected devices.
pub fn find(target: Target) -> Result<Device> {
    select(list()?, target)?.ok_or(match target {
        Target::Ecid(e) => Error::EcidNotFound(e),
        Target::One => Error::NoDfuDevice,
    })
}

/// Poll until the target is connected, or the timeout elapses.
pub fn wait(target: Target, timeout: Duration) -> Result<Device> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(dev) = select(list()?, target)? {
            return Ok(dev);
        }
        if Instant::now() >= deadline {
            return Err(Error::WaitTimeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Enrich the devices [`list`] returned with everything else we can determine —
/// the same facts Apple Configurator surfaces:
///
/// - **Port** (macOS): which host port each device is on and whether it's the
///   DFU-capable one (`port`), from read-only IORegistry topology.
/// - **Model** (booted Macs): the product ID and `bcdDevice` release number
///   encode the model identifier (see [`model_from_usb`]); no device open needed.
/// - **Marketing name** (booted Macs): the exact Apple name from the serial's
///   config code (network, cached per serial).
/// - **ECID** (booted Macs): advertised in a platform-capability descriptor in
///   the USB BOS descriptor (the value macOS also exposes as
///   `UsbAppleDeviceECID`), read with a standard `GET_DESCRIPTOR` request.
///
/// Best-effort: a device we can't open keeps `ecid == None` (the ECID is then
/// free from the USB serial once the Mac enters DFU or recovery); the model
/// still resolves without opening. Never errors; leaves known fields untouched.
pub fn identify(devices: &mut [Device]) {
    // DFU-port detection applies to every device (macOS only).
    #[cfg(target_os = "macos")]
    crate::dfu::port::mark_ports(devices);

    // Billboard devices carry no SoC identity, but their port controller does
    // advertise which alternate mode it tried — read it for display.
    enrich_billboards(devices);

    // The rest enriches booted Macs, which alone need it.
    let needs = |d: &Device| {
        d.mode == UsbMode::Booted
            && (d.identity.is_none() || d.ecid.is_none())
            && !d.serial.is_empty()
    };
    if !devices.iter().any(needs) {
        return;
    }
    let Ok(infos) = nusb::list_devices().wait() else {
        return;
    };
    let apple: Vec<nusb::DeviceInfo> = infos.filter(|i| i.vendor_id() == APPLE_VID).collect();
    for d in devices.iter_mut() {
        if !needs(d) {
            continue;
        }
        let Some(info) = apple
            .iter()
            .find(|i| i.serial_number() == Some(d.serial.as_str()))
        else {
            continue;
        };
        // Model comes from the product ID and bcdDevice — no open needed.
        if d.identity.is_none() {
            if let Some(m) = model_from_usb(info.product_id(), info.device_version()) {
                d.identity = Some(Identity {
                    cpid: m.cpid,
                    bdid: m.bdid,
                    srtg: None,
                    model: Some(*m),
                });
            }
        }
        // Exact Apple marketing name from the serial's config code (network,
        // cached per serial); display falls back to the model name without it.
        d.marketing_name = apple_marketing_name(&d.serial);
        if d.ecid.is_none() {
            d.ecid = read_ecid(info);
        }
    }
}

/// The exact Apple marketing name for a Mac serial, from Apple's config-code
/// service — the same source Apple Configurator uses. Best-effort and cached
/// per serial (a machine's name never changes): returns `None` on any network
/// or parse failure, or for serial formats without a positional config code, so
/// callers fall back to the local model name.
fn apple_marketing_name(serial: &str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(serial) {
        return hit.clone();
    }
    let name = fetch_marketing_name(serial);
    cache
        .lock()
        .unwrap()
        .insert(serial.to_string(), name.clone());
    name
}

fn fetch_marketing_name(serial: &str) -> Option<String> {
    // Config code: the last 4 chars of a 12-char serial, last 3 of an 11-char
    // one (both start at index 8). Randomized/other formats have no positional
    // code — skip them.
    let cc = match serial.len() {
        11 | 12 => serial.get(8..)?,
        _ => return None,
    };
    let body = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .user_agent(concat!("restorekit/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?
        .get(format!("https://support-sp.apple.com/sp/product?cc={cc}"))
        .send()
        .ok()?
        .text()
        .ok()?;
    parse_config_code_name(&body)
}

/// Extract the marketing name from the config-code service's XML response.
/// Returns `None` for the "CPU Name" placeholder the service echoes for an
/// unknown code (a valid name always contains "Mac").
fn parse_config_code_name(body: &str) -> Option<String> {
    let start = body.find("<configCode>")? + "<configCode>".len();
    let end = start + body[start..].find("</configCode>")?;
    let name = body[start..end].trim();
    name.contains("Mac").then(|| name.to_string())
}

/// Apple's platform-capability UUID whose payload is the device's ECID.
const APPLE_ECID_UUID: [u8; 16] = [
    0x0a, 0x37, 0x4c, 0xe4, 0x76, 0x23, 0x47, 0xc9, 0x88, 0x0e, 0x1a, 0xc3, 0x15, 0x13, 0x47, 0x6f,
];

/// Read the device's BOS descriptor. Windows asks the parent hub for it, so no
/// interface needs claiming there.
fn read_bos(dev: &nusb::Device) -> Option<Vec<u8>> {
    dev.get_descriptor(0x0f, 0, 0, Duration::from_millis(500))
        .wait()
        .ok()
}

/// The ECID a booted Mac advertises in a platform capability of its BOS
/// descriptor. Opens the device; `None` when it isn't an Apple device, can't
/// be opened or advertises none.
pub fn read_ecid(info: &nusb::DeviceInfo) -> Option<u64> {
    if info.vendor_id() != APPLE_VID {
        return None;
    }
    ecid_from_bos(&read_bos(&info.open().wait().ok()?)?)
}

/// Fill in each billboard device's advertised alternate-mode SVID by opening it
/// and reading its Billboard capability descriptor. Best-effort: a device we
/// can't open (or one whose descriptor omits the capability) keeps
/// `billboard_svid == None`.
fn enrich_billboards(devices: &mut [Device]) {
    let needs = |d: &Device| d.mode == UsbMode::Billboard && d.billboard_svid.is_none();
    if !devices.iter().any(needs) {
        return;
    }
    let Ok(infos) = nusb::list_devices().wait() else {
        return;
    };
    let apple: Vec<nusb::DeviceInfo> = infos.filter(|i| i.vendor_id() == APPLE_VID).collect();
    for d in devices.iter_mut() {
        if !needs(d) {
            continue;
        }
        let Some(info) = apple.iter().find(|i| {
            i.product_id() == BILLBOARD_PID && i.serial_number() == Some(d.serial.as_str())
        }) else {
            continue;
        };
        d.billboard_svid = info
            .open()
            .wait()
            .ok()
            .and_then(|dev| read_bos_billboard_svid(&dev));
    }
}

fn read_bos_billboard_svid(dev: &nusb::Device) -> Option<u16> {
    billboard_svid_from_bos(&read_bos(dev)?)
}

/// Parse a BOS descriptor and return the ECID from Apple's platform-capability
/// descriptor (`bDevCapabilityType` = 5 PLATFORM, matching [`APPLE_ECID_UUID`],
/// with an 8-byte little-endian ECID payload).
fn ecid_from_bos(bos: &[u8]) -> Option<u64> {
    // 5-byte BOS header, then a sequence of capability descriptors.
    let mut i = 5;
    while i + 3 <= bos.len() {
        let len = bos[i] as usize;
        if len < 3 || i + len > bos.len() {
            break;
        }
        let cap = &bos[i..i + len];
        // DEVICE_CAPABILITY (0x10), PLATFORM (0x05), 4-byte header + 16-byte
        // UUID + 8-byte payload.
        if cap[1] == 0x10 && cap[2] == 0x05 && len >= 28 && cap[4..20] == APPLE_ECID_UUID {
            return Some(u64::from_le_bytes(cap[20..28].try_into().unwrap()));
        }
        i += len;
    }
    None
}

/// Parse a BOS descriptor and return the preferred alternate-mode SVID from its
/// USB Billboard capability descriptor (`bDevCapabilityType` = 0x0D BILLBOARD).
///
/// The billboard capability has a fixed 44-byte prefix — including a 1-byte
/// `bPreferredAlternateMode` index at offset 5 — followed by an array of 4-byte
/// alternate-mode entries (`wSVID`, `bAlternateMode`, `iAlternateModeString`).
/// We return the `wSVID` of the preferred entry.
fn billboard_svid_from_bos(bos: &[u8]) -> Option<u16> {
    let mut i = 5;
    while i + 3 <= bos.len() {
        let len = bos[i] as usize;
        if len < 3 || i + len > bos.len() {
            break;
        }
        let cap = &bos[i..i + len];
        // DEVICE_CAPABILITY (0x10), BILLBOARD (0x0D).
        if cap[1] == 0x10 && cap[2] == 0x0d {
            let preferred = *cap.get(5)? as usize;
            let base = 44 + preferred * 4;
            let svid = u16::from_le_bytes([*cap.get(base)?, *cap.get(base + 1)?]);
            return Some(svid);
        }
        i += len;
    }
    None
}

/// Poll until `pred` matches a connected device, or the timeout elapses.
/// E.g. "this ECID, in DFU mode": `wait_where(t, |d| d.ecid == Some(e) && d.in_dfu())`.
pub fn wait_where(timeout: Duration, mut pred: impl FnMut(&Device) -> bool) -> Result<Device> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(dev) = list()?.into_iter().find(&mut pred) {
            return Ok(dev);
        }
        if Instant::now() >= deadline {
            return Err(Error::WaitTimeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_m1_air() {
        let m = lookup(0x8103, 0x26).unwrap();
        assert_eq!(m.board, "J313AP");
        assert_eq!(m.identifier, "MacBookAir10,1");
    }

    #[test]
    fn lookup_unknown() {
        assert!(lookup(0xffff, 0xff).is_none());
    }

    #[test]
    fn lookup_falls_back_to_libirecovery() {
        let m = lookup(0x8030, 0x10).unwrap();
        assert_eq!(m.identifier, "iPhone12,8");
        assert_eq!(m.board, "D79AP");
        assert_eq!(lookup_identifier("iphone12,8").unwrap().cpid, 0x8030);
        // MAC_MODELS wins over libirecovery's entry for the same Mac.
        assert_eq!(lookup(0x8103, 0x26).unwrap().board, "J313AP");
    }

    #[test]
    fn kinds() {
        assert_eq!(kind("iPhone12,8"), "iPhone");
        assert_eq!(kind("iPad17,1"), "iPad");
        assert_eq!(kind("iMac24,1"), "Mac");
        assert_eq!(kind("MacBookAir10,1"), "Mac");
        assert_eq!(kind("iBridge2,1"), "Mac");
        assert_eq!(kind("Something1,1"), "device");
    }

    #[test]
    fn lookup_by_identifier_case_insensitive() {
        assert_eq!(lookup_identifier("macmini9,1").unwrap().board, "J274AP");
    }

    #[test]
    fn no_duplicate_keys() {
        let mut keys: Vec<_> = MAC_MODELS.iter().map(|m| (m.cpid, m.bdid)).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len());
    }

    #[test]
    fn model_from_device_version_decodes_bcd() {
        // 0x1701 → "17,1" → MacBookPro17,1 (a booted M1 MacBook Pro).
        assert_eq!(
            model_from_device_version(0x1701).map(|m| m.identifier),
            Some("MacBookPro17,1")
        );
        // Newer Macs use the "Mac16,6"-style identifier.
        assert_eq!(
            model_from_device_version(0x1606).map(|m| m.identifier),
            Some("Mac16,6")
        );
        // Two-digit minor: 0x1415 → "14,15" → Mac14,15 (15" MacBook Air M2).
        assert_eq!(
            model_from_device_version(0x1415).map(|m| m.identifier),
            Some("Mac14,15")
        );
        assert_eq!(model_from_device_version(0x9999), None);
    }

    #[test]
    fn device_version_clashes_need_the_pid() {
        // Numeric parts shared by more than one identifier. A new clash fails
        // here until model_from_usb is checked to resolve it.
        let mut numbers: Vec<&str> = MAC_MODELS
            .iter()
            .map(|m| &m.identifier[identifier_prefix(m.identifier).len()..])
            .collect();
        numbers.sort();
        let mut clashes: Vec<&str> = numbers
            .windows(2)
            .filter(|w| w[0] == w[1])
            .map(|w| w[0])
            .collect();
        clashes.dedup();
        assert_eq!(clashes, ["18,1", "18,2"]);

        assert_eq!(model_from_device_version(0x1801), None);
        let id = |pid, bcd| model_from_usb(pid, bcd).map(|m| m.identifier);
        assert_eq!(id(0x1902, 0x1801), Some("MacBookPro18,1"));
        assert_eq!(id(0x1905, 0x1801), Some("Mac18,1"));
        assert_eq!(id(0x1902, 0x1802), Some("MacBookPro18,2"));
        assert_eq!(id(0x1905, 0x1802), Some("Mac18,2"));
        assert_eq!(id(0x1901, 0x1801), None);
        // A unique number resolves whatever the PID says.
        assert_eq!(id(0x1902, 0x1606), Some("Mac16,6"));
    }

    #[test]
    fn model_from_usb_round_trips_every_model() {
        let bcd = |n: u16| (n / 10) << 4 | (n % 10);
        for m in MAC_MODELS {
            let prefix = identifier_prefix(m.identifier);
            let (major, minor) = m.identifier[prefix.len()..].split_once(',').unwrap();
            let v = bcd(major.parse().unwrap()) << 8 | bcd(minor.parse().unwrap());
            let pid = (0x1901..=0x1905)
                .find(|&p| booted_pid_prefix(p) == Some(prefix))
                .unwrap_or(0);
            assert_eq!(
                model_from_usb(pid, v).map(|m| m.identifier),
                Some(m.identifier)
            );
        }
    }

    #[test]
    fn usb_id_summary_formats_billboard() {
        let id = UsbId {
            vid: 0x05ac,
            pid: 0x7304,
            device_version: 0x2100,
            class: 0x11,
            subclass: 0,
            protocol: 0,
        };
        assert_eq!(id.version_str(), "21.00");
        assert_eq!(id.class_name(), Some("billboard"));
        assert_eq!(id.summary(), "05ac:7304 (class 0x11 billboard, rev 21.00)");

        let dfu = UsbId {
            vid: 0x05ac,
            pid: 0x1227,
            device_version: 0x0110,
            class: 0,
            subclass: 0,
            protocol: 0,
        };
        assert_eq!(
            dfu.summary(),
            "05ac:1227 (class 0x00 per-interface, rev 1.10)"
        );
    }

    #[test]
    fn billboard_svid_from_bos_reads_preferred() {
        // BOS header (5 bytes) + one Billboard capability with two alt modes,
        // preferred index 1 (Thunderbolt, SVID 0x8087).
        let mut cap = vec![
            0u8,  // bLength, patched below
            0x10, // DEVICE CAPABILITY
            0x0d, // BILLBOARD
            0,    // iAdditionalInfoURL
            2,    // bNumberOfAlternateModes
            1,    // bPreferredAlternateMode -> index 1
            0, 0, // VCONNPower
        ];
        cap.extend_from_slice(&[0u8; 32]); // bmConfigured
        cap.extend_from_slice(&[0x21, 0x01]); // bcdVersion 1.21
        cap.push(0); // bAdditionalFailureInfo
        cap.push(0); // bReserved
        cap.extend_from_slice(&[0xac, 0x05, 0x00, 0x00]); // alt 0: SVID 0x05ac (Apple)
        cap.extend_from_slice(&[0x87, 0x80, 0x00, 0x00]); // alt 1: SVID 0x8087 (Thunderbolt)
        cap[0] = cap.len() as u8;

        let mut bos = vec![0x05, 0x0f, 0, 0, 1]; // BOS header, 1 capability
        bos.extend_from_slice(&cap);
        assert_eq!(billboard_svid_from_bos(&bos), Some(0x8087));
        assert_eq!(billboard_svid_from_bos(&bos[..5]), None);
    }

    #[test]
    fn parses_config_code_name() {
        // Real response for the M1 MacBook Pro's config code (cc=Q05P).
        let ok = "<?xml version=\"1.0\"?><root><name>CPU Name</name>\
            <configCode>MacBook Pro (13-inch, M1, 2020)</configCode><locale>en_US</locale></root>";
        assert_eq!(
            parse_config_code_name(ok).as_deref(),
            Some("MacBook Pro (13-inch, M1, 2020)")
        );
        // Unknown code: the service echoes "CPU Name" — treated as no result.
        let placeholder = "<root><configCode>CPU Name</configCode></root>";
        assert_eq!(parse_config_code_name(placeholder), None);
        assert_eq!(parse_config_code_name("<root></root>"), None);
    }

    #[test]
    fn parses_ecid_from_bos_descriptor() {
        // Real BOS descriptor from a booted MacBook Pro (ECID 0x445E11462001E).
        let bos = [
            0x05u8, 0x0f, 0x46, 0x00, 0x04, // BOS header, 4 capabilities
            0x07, 0x10, 0x02, 0x02, 0x00, 0x00, 0x00, // USB 2.0 extension
            0x1c, 0x10, 0x05, 0x00, // platform capability, len 28
            0x0a, 0x37, 0x4c, 0xe4, 0x76, 0x23, 0x47, 0xc9, 0x88, 0x0e, 0x1a, 0xc3, 0x15, 0x13,
            0x47, 0x6f, // Apple ECID UUID
            0x1e, 0x00, 0x62, 0x14, 0xe1, 0x45, 0x04, 0x00, // ECID (little-endian)
            0x0a, 0x10, 0x03, 0x00, 0x0e, 0x00, 0x01, 0x0a, 0xff, 0x07, // another capability
        ];
        assert_eq!(ecid_from_bos(&bos), Some(0x445E11462001E));
        assert_eq!(ecid_from_bos(&[0x05, 0x0f, 0x05, 0x00, 0x00]), None);
    }

    #[test]
    fn parses_full_serial() {
        let s = "CPID:8103 CPFM:03 SCEP:01 BDID:26 ECID:000C60A812345678 IBFL:3C SRTG:[iBoot-7429.61.2] SRNM:[C02XX1234567]";
        let (cpid, bdid, ecid, srtg, srnm) = parse_serial(s).unwrap();
        assert_eq!(cpid, 0x8103);
        assert_eq!(bdid, 0x26);
        assert_eq!(ecid, 0x000C60A812345678);
        assert_eq!(srtg.as_deref(), Some("iBoot-7429.61.2"));
        assert_eq!(srnm.as_deref(), Some("C02XX1234567"));
    }

    #[test]
    fn parses_port_dfu_identity() {
        let s = "SDOM:01 CPID:0022 BDID:0000000004604000 ECID:794E1EC5141BD7C0 PREV:29";
        let (cpid, bdid, ecid, srtg, srnm) = parse_port_dfu_serial(s).unwrap();
        assert_eq!(cpid, 0x6040);
        assert_eq!(bdid, 0x04);
        assert_eq!(ecid, 0x794E1EC5141BD7C0);
        assert_eq!(lookup(cpid, bdid).unwrap().identifier, "Mac16,8");
        assert!(srtg.is_none());
        assert!(srnm.is_none());

        let neo = "CPID:0022 BDID:0000000064814000 ECID:0123456789ABCDEF";
        let (cpid, bdid, ..) = parse_port_dfu_serial(neo).unwrap();
        assert_eq!((cpid, bdid), (0x8140, 0x64));
        assert_eq!(lookup(cpid, bdid).unwrap().identifier, "Mac17,5");
    }

    #[test]
    fn recognizes_port_dfu_pid() {
        assert_eq!(UsbMode::from_pid(PORT_DFU_PID), UsbMode::PortDfu);
        assert_eq!(UsbMode::PortDfu.to_string(), "port-dfu");
    }

    #[test]
    fn recognizes_booted_restore_and_recovery_pids() {
        for pid in 0x1901..=0x1905 {
            assert_eq!(UsbMode::from_pid(pid), UsbMode::Booted);
        }
        assert_eq!(UsbMode::from_pid(0x12ac), UsbMode::Restore);
        assert_eq!(UsbMode::from_pid(0x1283), UsbMode::Recovery);
        assert_eq!(UsbMode::from_pid(0x12ab), UsbMode::Other);
    }

    #[test]
    fn missing_required_field_fails() {
        assert!(parse_serial("CPID:8103 BDID:26").is_none());
    }

    #[test]
    fn ecid_from_cpid_ecid_serial() {
        assert_eq!(
            super::ecid_from_usb_serial("00008103-000445E11462001E"),
            Some(0x000445E11462001E),
        );
        // Apple hardware serials and other shapes don't match.
        assert_eq!(super::ecid_from_usb_serial("C02DQ6TPQ05P"), None);
        assert_eq!(super::ecid_from_usb_serial("00008103-XYZ"), None);
    }

    fn dev(mode: UsbMode, ecid: u64) -> Device {
        Device {
            mode,
            product: "Apple Mobile Device".into(),
            serial: String::new(),
            ecid: Some(ecid),
            srnm: None,
            identity: Some(Identity {
                cpid: 0x8103,
                bdid: 0x26,
                srtg: None,
                model: lookup(0x8103, 0x26).copied(),
            }),
            marketing_name: None,
            port: None,
            billboard_svid: None,
            usb: UsbId {
                vid: APPLE_VID,
                pid: DFU_PID,
                device_version: 0,
                class: 0,
                subclass: 0,
                protocol: 0,
            },
            driver_ready: true,
            usb_device: None,
        }
    }

    #[test]
    fn device_accessors() {
        let d = dev(UsbMode::Dfu, 0x1234);
        assert!(d.restorable());
        assert_eq!(d.identifier(), Some("MacBookAir10,1"));
        assert_eq!(d.ecid_hex().as_deref(), Some("0x1234"));
        assert!(dev(UsbMode::PortDfu, 2).restorable());
        assert!(!dev(UsbMode::Recovery, 1).restorable());
    }

    #[test]
    fn kis_mode_is_dfu_and_restorable_once_identified() {
        assert_eq!(UsbMode::from_pid(KIS_PID), UsbMode::Kis);
        assert_eq!(UsbMode::Kis.to_string(), "kis");
        let mut d = dev(UsbMode::Kis, 3);
        assert!(d.in_dfu());
        assert!(d.restorable());
        // A macOS host can't read KIS identity: in DFU, but not restorable.
        d.identity = None;
        d.ecid = None;
        assert!(d.in_dfu());
        assert!(!d.restorable());
    }

    #[test]
    fn select_one_only_matches_dfu() {
        assert!(select(vec![], Target::One).unwrap().is_none());
        assert!(select(vec![dev(UsbMode::Recovery, 1)], Target::One)
            .unwrap()
            .is_none());
        let picked = select(
            vec![dev(UsbMode::Recovery, 1), dev(UsbMode::PortDfu, 2)],
            Target::One,
        )
        .unwrap()
        .unwrap();
        assert_eq!(picked.ecid, Some(2));
        assert!(matches!(
            select(
                vec![dev(UsbMode::Dfu, 1), dev(UsbMode::Dfu, 2)],
                Target::One
            ),
            Err(Error::MultipleDfuDevices(2))
        ));
    }

    #[test]
    fn select_ecid_matches_any_mode() {
        let devices = vec![dev(UsbMode::Recovery, 1), dev(UsbMode::Dfu, 2)];
        let found = select(devices.clone(), Target::Ecid(1)).unwrap().unwrap();
        assert_eq!(found.mode, UsbMode::Recovery);
        assert!(select(devices, Target::Ecid(3)).unwrap().is_none());
    }
}
