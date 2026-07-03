use serde::Serialize;

/// A known Apple Silicon Mac model, keyed by chip ID + board ID as reported
/// in the DFU-mode USB serial string.
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

/// Generated from https://api.ipsw.me/v4/devices (cpid/bdid per board).
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
];

pub fn lookup(cpid: u16, bdid: u8) -> Option<&'static MacModel> {
    MAC_MODELS.iter().find(|m| m.cpid == cpid && m.bdid == bdid)
}

pub fn lookup_identifier(identifier: &str) -> Option<&'static MacModel> {
    MAC_MODELS
        .iter()
        .find(|m| m.identifier.eq_ignore_ascii_case(identifier))
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
}
