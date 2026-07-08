# RecoverKit Dongle - Product Requirements Document

Status: Draft v0.1
Owner: Frank Chiarulli Jr.
Last updated: 2026-07-07

## 1. Summary

The RecoverKit Dongle is the full three-port version of the RecoverKit Dongle
Lite. It does everything Lite does (force an Apple Silicon or T2 Mac into DFU
mode and hand it to a host for restore, on macOS, Windows, or Linux) and adds a
third port that powers the target from an external USB-C PD charger of up to
100 W. This covers restores where the target battery is dead or the host port
cannot keep the target alive through the restore.

This PRD only specifies the delta over Lite. The base platform (RP2040, target
PD PHY, USB 2.0 hub, host bus power, firmware, manufacturing rules, and Apple
serial mode: orientation-aware 1.2 V SBU UART with intelligent TX/RX swap) is
defined in `../dongle-lite/PRD.md` and is inherited unchanged here. Read that
first. The Power port carries no data or SBU pins, so it does not touch the
serial path.

Key requirement for the power subsystem: the dongle must be able to source
**whatever voltage and current the target requests** within the external
supply's limits, not a single fixed profile. That drives the design choices
below.

## 2. Goals (delta over Lite)

- Power the target from an external USB-C PD charger, up to 100 W (20 V / 5 A).
- Source to the target whatever PD profile it asks for (5 V, 9 V, 15 V, 20 V,
  and ideally PPS ranges), subject to what the attached charger can supply.
- Keep the dongle's own logic bus-powered from the host by default. The external
  charger powers the target, not the dongle logic.
- Keep the power subsystem optional to populate: an unpopulated board is
  electrically a Dongle Lite and must pass all Lite requirements.

## 3. Non-goals (delta over Lite)

- Not a general-purpose bench PSU. Output profiles are bounded by the charger
  and by the converter's rating.
- No power sharing or splitting across multiple targets.

## 4. Additional port

| Port | Role | Data | Power |
|------|------|------|-------|
| Power | Accepts an external USB-C PD charger | none | Sinks up to 100 W (20 V/5 A) to power the target |

The Power port carries VBUS, GND, and CC only. No data pins needed. A 16-pin
USB 2.0 receptacle is fine.

## 5. Additional functional requirements

- FR-P1: The dongle still owns the target CC line at all times (Lite FR2). The
  external charger negotiates only with the dongle, never directly with the
  target.
- FR-P2: The Power port runs a PD **sink** controller that dynamically
  negotiates with the attached charger.
- FR-P3: When the target requests a PD profile, the dongle sources it and
  delivers the matching voltage/current on the target VBUS, within the charger's
  and converter's limits. If a requested profile cannot be met, the dongle
  advertises only profiles it can actually supply so the target never negotiates
  a contract the hardware cannot honor.
- FR-P4: Host 5 V (dongle logic) and the external power path must never backfeed
  each other. Only one source drives target VBUS at a time, through reverse-
  blocking switching.
- FR-P5: With the Power port unpopulated or no charger attached, behavior is
  identical to Lite: vSafe5V signaling only, target on battery.

## 6. Power subsystem design

Two candidate approaches. The requirement to source "whatever the target wants"
is what separates them, and PPS narrows the gap.

### 6.1 Approach B: PD passthrough with a programmable load switch (recommended)

- The power-port PD sink negotiates from the charger exactly the profile the
  target wants -- fixed PDOs (5/9/15/20 V) or PPS (fine steps) -- then passes
  that VBUS to the target through a switched, current-limited power path.
- No DC-DC converter. Firmware keeps the sink contract (charger side) and the
  source contract (target side) in lockstep, so the dongle only advertises to
  the target what it can currently pull from the charger.

Spark-Analyzer is the reference: an FUSB302-based board that negotiates
5/9/15/20 V plus PPS (3.3-21 V in 20 mV steps) up to 100 W (20 V / 5 A) and
switches it to an output with a software current limit. Its power stage maps
onto our needs:

  - FUSB302MPX as the power-port sink PHY (the same C132291 we already use)
  - DMP3017SFG-7 P-FET as the output load switch with programmable current
    limit (the e-fuse / VBUS gate)
  - CC6904SO-10A hall-effect current sensor for measurement and limiting
  - TPS62175 buck for local 3.3 V (we already derive 3.3 V from host, so this is
    only relevant if the power port must run logic with the host absent)

Why this now covers "whatever the target wants": with PPS the dongle can source
almost any voltage the target asks for (20 mV steps), and for fixed requests it
mirrors the matching PDO. A converter is only needed for the rare case where the
target wants a voltage the charger cannot provide at all.

### 6.2 Approach A: buck-boost converter (fallback, full independence)

- Sink the highest useful profile (typically 20 V) as a raw rail, then a
  programmable ~100 W buck-boost generates any target voltage independent of the
  charger's PDOs.

Pros: honors any target request regardless of charger profiles. Cons: a real
100 W DC-DC stage (inductor, controller, thermals, area, cost).

Decision: default to Approach B (passthrough + PPS, per Spark-Analyzer). It is
cheaper and simpler, and PPS makes it flexible enough for target restores, which
use standard voltages that common chargers and PPS already provide. Fall back to
Approach A only if bench testing finds a target/charger combination B cannot
satisfy.

### 6.3 Why the power-port controller is a second FUSB302B (not a fixed sink)

Because FR-P2/FR-P3 require **dynamic** negotiation on both ends. The power port
must renegotiate the charger at runtime as the target's needs change, and the
target side must present a PDO set computed from what is currently available. A
fixed-function sink trigger (CH224K, HUSB238) only requests one preset voltage
and cannot renegotiate, so it does not meet the requirement. A second FUSB302B
gives full firmware control of the sink contract and reuses the same driver
already written for the target-side PHY. This is the case where a second
FUSB302B is justified; on Lite (no power) and on a fixed-output design it would
not be.

## 7. Architecture (delta over Lite)

```
   (Lite platform: Host, Target, RP2040, hub, FUSB302B #1 source, serial)

  Power USB-C ---CC--> FUSB302B #2 (sink) <--I2C--> RP2040
     (up to 20V/5A) --> [ Approach A: buck-boost, output set by RP2040 ]
                             |
  Host 5V --> load switch ---+--(back-to-back reverse-blocking FETs, one active)--> Target VBUS
                             |
        RP2040 controls source selection and converter setpoint; FUSB302B #1
        advertises to the target only the PDOs currently deliverable.
```

## 8. Additional BOM (delta over Lite, LCSC / JLCPCB)

All parts LCSC-orderable and JLCPCB-assemblable. Verify stock and footprints at
M1.

| Function | Suggested part | LCSC (verify) |
|----------|----------------|---------------|
| PD PHY, power port (sink, dynamic) | FUSB302BMPX | C132291 |
| USB-C receptacle, 16-pin (Power) | USB-C 2.0 16P | C165948 |
| ESD protection (power port) | USBLC6-2SC6 (or CC/VBUS TVS) | C7519 |
| Output load switch (Approach B) | DMP3017SFG-7 P-FET | verify (per Spark-Analyzer) |
| Current sensor | CC6904SO-10A hall-effect | verify (per Spark-Analyzer) |
| Reverse-blocking on host 5 V vs power path | back-to-back P-FETs | verify |
| Input/output bulk capacitors | 25 V+ rated | verify |
| Buck-boost controller (Approach A fallback only) | evaluate at M1 | verify |
| Power inductor, 100 W class (Approach A only) | evaluate at M1 | verify |

## 9. Additional test plan (delta over Lite)

1. Power port PD sink negotiates 20 V from a 100 W charger; verify input rail.
2. Approach A: sweep converter output 5 to 20 V, verify regulation and current
   limit; verify back-to-back FET switching shows no backfeed between host 5 V
   and the power path.
3. Target requests each standard PDO; verify the dongle sources the matching
   voltage and the restore completes on a dead-battery target.
4. Regression: unpopulate the power subsystem, confirm the board behaves exactly
   as Dongle Lite (host cable only, restore end to end).

## 10. Milestones (delta over Lite)

- Full-dongle work starts after Dongle Lite reaches M3 (proven base platform).
- MP1: power subsystem schematic (Approach A), controller + FET selection from
  LCSC.
- MP2: layout with the power stage, thermal review, JLCPCB DRC clean.
- MP3: first article, power bring-up per section 9.
- MP4: dead-battery restore validated across a range of target Macs.

## 11. Open questions

- Final buck-boost controller selection with confirmed LCSC stock and a JLCPCB
  footprint, rated for 100 W with acceptable thermals in a small enclosure.
- Whether PPS support is required, or whether the fixed 5/9/15/20 V set is
  enough for target restores. This decides how far Approach A must stretch and
  whether Approach B is ever viable.
- Thermal design and enclosure venting at sustained 100 W.
- Current sensing and per-profile limits: report to host over CDC0 or keep
  internal.

## 12. References

See `../dongle-lite/PRD.md` section 13, plus:

- Spark-Analyzer (tooyipjee) - open KiCad FUSB302-based USB-C PD programmable
  supply: negotiates 5/9/15/20 V + PPS up to 100 W and switches it to an output
  with a software current limit. Reference for the power-port sink + load-switch
  + current-sense stage (parts: FUSB302MPX, DMP3017SFG-7, CC6904SO-10A,
  TPS62175).
  - https://github.com/tooyipjee/Spark-Analyzer
