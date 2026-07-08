# Dongle Lite - BOM sourcing notes

Resolves the parts left open in `AGENT_HANDOFF.md` / `PRD.md` section 8. Every
line maps to an in-stock LCSC part with a JLCPCB assembly footprint. Re-check
stock at order time. Prefer JLCPCB Basic parts to avoid per-part loading fees;
Extended parts are fine but add a one-time feeder fee each.

## Corrections to the anchored BOM

- **USB 2.0 hub CH334F**: the handoff listed `C425949`, which is wrong. The
  correct LCSC/JLCPCB code is **C5187527** (WCH CH334F, QFN-24 4x4, ~$0.38, in
  stock). C425949 is a different part - do not order it.

## Newly resolved parts (the net-new work over the reference designs)

| Function | MPN | LCSC | Package | Notes |
|----------|-----|------|---------|-------|
| USB 2.0 hub (1 up, up to 4 down; we use 2) | CH334F | **C5187527** | QFN-24 (4x4) | Pin-compatible family with FE1.1s. STT is fine for us; MTT available. Downstream port 1 -> RP2040, port 2 -> target D+/D-. Likely an Extended part - budget one feeder fee. |
| 3.3 V LDO from host 5 V | RT9013-33GB | **C47773** | SOT-23-5 | 500 mA, ~250 mV dropout, has EN, OCP + OTP. Big stock, ~$0.07. Better than AMS1117 (lower Iq, lower dropout, enable pin). Logic budget < 250 mA, so ample headroom. |
| Target VBUS load switch | TPS22918DBVR | **C131941** | SOT-23-6 | 2 A, EN, configurable rise time (tames inrush). vSafe5V signaling only - no real current. Simple default. |

Load-switch alternative if we want hard protection on the VBUS facing the Mac:
**AP22653** (LCSC C2158037, SOT-26) adds an adjustable current limit (resistor-
set) plus reverse-current blocking. Not required for Lite (the target is a sink
in DFU and never sources VBUS back), but it's the safer choice and is the same
family the full dongle's power path wants. TPS22918 is the cheaper default.

## Commodity parts (confirmed in stock, JLCPCB-assemblable)

| Function | MPN | LCSC | Notes |
|----------|-----|------|-------|
| QSPI boot flash | W25Q32JVSSIQ | **C2834491** | SOIC-8, per AltmodeFriend |
| XOSC crystal | 12 MHz 3225 4-pad | **C9002** | X322512MSB4SI, common RP2040 pick; + 2x 22 pF |

## Already anchored (from the references; unchanged)

| Function | MPN | LCSC |
|----------|-----|------|
| PD PHY (target source) | FUSB302BMPX | C132291 |
| SBU level translators (2x) | 74AVC1T45GW,125 | C282330 |
| 1.2 V LDO (translator low side) | TLV70212 | C81462 |
| USB-C receptacle (Host + Target) | HRO TYPE-C-31-M-12 | C165948 |
| ESD array (per USB port) | USBLC6-2SC6 | C7519 |
| MCU | RP2040 | C2040 |

Passives (22R USB series, 5.1k CC pulldowns, 4.7k I2C pullups, 0.1u/1u/470p
decoupling, boot button, LEDs) are all JLCPCB Basic commons; pick at schematic
capture per the Central Scrutinizer v3.1 production BOM values.

## Status

All BOM lines the handoff flagged "verify / still to pick" are now resolved to
concrete in-stock LCSC codes. Nothing here has been ordered - this is sourcing
only. Final stock + Basic/Extended re-check happens at M1 BOM finalization,
which is gated on the M0 bench result per the PRD.
