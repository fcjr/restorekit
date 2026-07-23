# RecoverKit Dongle Pro Power - Product Requirements Document

Status: Draft v0.1
Owner: Frank Chiarulli Jr.
Last updated: 2026-07-23

## 1. Summary

The Dongle-Pro-Power is the Dongle-Pro (`../dongle-pro/PRD.md`) plus a third
USB-C port that takes up to 100 W from a wall PD charger and re-sources it to
the target: the dongle advertises real PD source capabilities on the target
port and delivers whatever the Mac requests, up to the full input budget.
Killer use case: reviving Macs with dead batteries, which often cannot hold
DFU without external power — park the Mac on the dongle and it charges,
DFU-triggers, and restores over the same cable, host optional for charging.

Everything else — RP2354A platform, FUSB302B DFU trigger, GL3510 SuperSpeed
passthrough, SBU serial — is unchanged from the Pro.

## 2. Delta over Pro

| | Dongle-Pro | Dongle-Pro-Power |
|---|---|---|
| Ports | host + target | host + target + **power-in** (24-pin USB-C) |
| Power in | host 5 V only | host 5 V and/or PD charger up to 20 V / 5 A |
| Target VBUS | vSafe5V via AP22653 | vSafe5V **or** PD-sourced up to 20 V / 5 A |
| PD sink | — | STUSB4500 (autonomous, I²C-readable) |
| Source switch | — | 2× LM74800 + AOD4184A back-to-back pairs |
| Housekeeping | — | TPS54331 20 V→5.15 V buck, INA180 current sense |
| Host required | yes | only for data; charging/DFU work charger-only |
| iProduct / serial | `Dongle-Pro` / `DP-` | `Dongle-Pro-Power` / `DPP-` |
| Firmware | `--features pro` | `--features power` (implies `pro`) |
| Update asset | `dongle-pro-fw.bin` | `dongle-pro-power-fw.bin` (same tag) |

## 3. Architecture

```
 PD charger ─► J4 ─► STUSB4500 (sink, ≤20V/5A) ─► PWR_VBUS (20V rail)
                                                    │
     ┌── TPS54331 buck (5.15V, auto-EN ≥7.5V) ◄─────┤
     │                                              ▼
     │   host5V ─► AP22653 ─┐                 U12: LM74800 + Q1/Q2 (B2B)
     └── D20 ───────────────┴► VSAFE_SRC ─►   │  SRC20 ─ R26 10mΩ ─┐
                                U13: LM74800  │       INA180 ──► GP28 (ADC)
                                + Q3/Q4 (B2B) ▼                    ▼
                                └──────────► TGT_VBUS ◄────────────┘
                                              │ 100k/10k ──► GP29 (ADC)
                                              └ Q5 + 940Ω discharge (GP14)
```

- **Every semiconductor on TGT_VBUS is ≥26 V tolerant.** The AP22653 (5.5 V)
  now feeds `VSAFE_SRC` *behind* U13's 40 V FETs, and the USBLC6's VBUS pin
  moved to the 5 V rail. This is why the Pro can't do this with a bodge.
- **vSafe5V sources**: host path `host5V → AP22653 → U13` (4.93 V, keeps the
  Lite/Pro current limit and flow) or charger path `buck 5.15 V → D20 → U13`
  (4.85 V) — both inside vSafe5V. System logic ORs host/buck via D21/D22, so
  an off host port is never back-driven.
- **Provisioned for 100 W from day one**: 5 A-rated 24-pin receptacle, 7 mΩ
  FETs (≈175 mW each at 5 A), 10 mΩ sense (250 mW, 2512), OCP setpoint above
  5 A with firmware enforcing the advertised limit, 20 V copper sized for 5 A.
  The 60 W → 100 W step is firmware only (see §5).

## 4. CC ownership (unchanged rule, one more instance)

Three CC domains, one owner each: HD3SS3220 owns host CC (autonomous),
STUSB4500 owns power-in CC (autonomous sink), FUSB302B owns target CC — now
running the DFU VDMs *and* the PD source policy engine. No sharing, ever.

## 5. Firmware (M2 — the PD source policy engine)

v0 firmware (this commit) parks the power path safe: `SRC20_EN` (GP12) low,
`VSAFE_EN` (GP13) high, `VBUS_DISCHG` (GP14) low, `STUSB_ALERT` (GP15) input;
GP28/GP29 are the current/voltage ADCs. M2 adds, on the FUSB302B:

1. Read the STUSB4500's negotiated contract over I²C (addr 0x28).
2. Advertise SourceCapabilities mirrored minus overhead: 5 V/1.5 A plus
   (input V)/(input I − 250 mA), e.g. 20 V/4.75 A from a 100 W charger.
3. Request evaluation, Accept, source transition (U10/U13 off → discharge →
   U12 on → PS_RDY) within tSrcReady; reverse for contract loss.
4. Hard-reset handling: discharge to vSafe0V, restart at vSafe5V.
5. Interleave with the DFU VDM flow (pause contract, VBUS-cycle, resume).
6. OCP: ADC trip above the advertised current → SRC20_EN low + discharge.
7. **60 W gate**: cap advertisements at 3 A until SOP′ Discover Identity
   confirms a 5 A e-marked cable (Vconn from the FUSB302B's integrated
   switches, in-spec at 3.3 V). 100 W is then a firmware release, no respin.

## 6. Sourcing (verified 2026-07-23)

| Part | LCSC | Notes |
|---|---|---|
| STUSB4500QTR | C2678061 | ~$0.94, QFN-24 |
| LM74800QDRRRQ1 ×2 | C3215600 | ~$1.16, B2B ideal-diode/switch controller |
| AOD4184A ×5 | C99124 | 40 V 7 mΩ TO-252, ~$0.14, 17k stock |
| TPS54331DR | C9865 | 28 V-in 3 A buck |
| INA180A1IDBVR | C122228 | 26 V common-mode, 20 V/V |
| SS34 ×3 | C8678 | ORing Schottkys |
| TYPE-C 24P QT | C2681555 | same receptacle as J1/J2, 5 A-rated |

## 7. Confirm before layout

Carried in the Lite tradition — wired from datasheet memory, verify pin-level:

1. **LM74800 application wiring** (CAP/VS/VSNS/OV pin roles, EN/UVLO divider,
   charge-pump cap placement) against the TI datasheet, both instances.
2. **STUSB4500 DISCH / VBUS_VS_DISCH** wiring vs. the ST sink application
   schematic; NVM defaults (ship requesting 20 V/5 A, fallback 9 V/3 A?).
3. **TPS54331 compensation** (R32/C66) and soft-start values for 20 V→5.15 V.
4. R26 sense resistor needs a **2512 footprint** (placeholder is 0402) and
   input/output buck capacitors must be **25 V-rated**.
5. Add **SMBJ24A TVS** on PWR_VBUS and TGT_VBUS at layout (not in netlist).
6. Buck inductor L3 placeholder reuses the AOTA 3.3 µH footprint — pick a
   real 10 µH / ≥3 A part at layout.
7. Bench: connector/pin temperature at 5 A with the enclosure on; derate in
   firmware if needed.
