# Dongle Lite — PCB Design & Layout Brief

**For:** the PCB designer taking this from schematic to fabricated, cased board.
**Owner:** Frank Chiarulli Jr. · **Design rev:** A · **Status:** schematic complete, layout not started.

This is a hand-off spec. The schematic, netlist, and part libraries are done and
verified; your job is placement, routing, mechanical, and fab/assembly output.

---

## 1. What this board is

A small **two-port USB-C in-line dongle** that forces an Apple Silicon or T2 Mac
into DFU over USB-PD and hands it to a host computer for restore. One end plugs
toward the **host** PC, the other toward the **target** Mac. See
[`../dongle-lite/PRD.md`](../dongle-lite/PRD.md) for the product rationale; this
brief covers only the board.

Functional blocks (already grouped in the schematic): RP2040 MCU core, USB 2.0
hub, USB-PD PHY, SBU level-shift, power. Full block map is in the schematic and
the connection list.

## 2. Inputs you're given

| Item | File |
|------|------|
| Schematic (single sheet, KiCad 10) | `dongle-lite.kicad_sch` |
| Schematic PDF | `dongle-lite.pdf` |
| Bill of materials | [`BOM.md`](BOM.md) |
| Pin-by-pin connection list | [`WIRING.md`](WIRING.md) |
| Project symbols / footprints / 3D models | `lib/` (registered via `sym-lib-table` / `fp-lib-table`) |
| Netlist generator (authoritative) | `gen/board.py`, `gen/gen.py` |

Connectivity is verified by net-partition against design intent: **no merges, no
splits, ERC 0 errors** (2 intentional warnings for the SWD bring-up nets, cleared
once you add the header in §7).

## 3. Hard constraints

1. **KiCad 10.** Deliver a KiCad 10 project. Keep using the project-local `lib/`
   so LCSC part numbers and JLCPCB footprints travel with the repo.
2. **LCSC parts only, JLCPCB-assembled.** Every placed part must be orderable from
   LCSC and have a JLCPCB assembly footprint — no hand-sourced or hand-soldered
   parts. If an LCSC code goes out of stock, substitute a pin/footprint-compatible
   LCSC part and note the swap. Prefer JLCPCB **Basic** parts (no feeder fee).
3. **USB-C on both ends.** `J1` (host) and `J2` (target) sit at **opposite short
   ends** of the board, connectors facing outward, so the board is an in-line
   adapter.
4. **As small as possible.** Minimize the outline; area is the primary mechanical
   goal (see §5).
5. **Fits a case.** The board must drop into a simple enclosure (see §9).

## 4. Stackup & fab (JLCPCB)

- **4-layer** (needed for clean 90 Ω USB 2.0 pairs with a solid, continuous ground
  reference). Sig / GND / PWR / Sig is the default intent.
- JLCPCB 4-layer default stackup, 1 oz copper, 0.8–1.0 mm total thickness
  (0.8 mm helps the case stay thin — confirm against connector mid-mount specs).
- Design-rule target: JLCPCB standard (≥3.5 mil trace/space, ≥0.2 mm vias). Only
  drop to their advanced rules if the QFN-56 fan-out forces it — call it out if so.
- Controlled impedance: request JLCPCB's impedance-controlled option and use their
  stackup calculator for the 90 Ω differential geometry.

## 5. Mechanical / form factor

- **Shape:** narrow rectangle, USB-C at each short end, parts in between.
- **Width** is set by the USB-C receptacle (~9 mm body) plus case wall — target
  **≤ 14 mm** board width.
- **Length:** minimize. Two connectors + RP2040 (7×7) + hub (4×4) + PD + power.
  Use **both board sides** for placement to shorten it. Target envelope **≤ ~50 mm
  long**; shorter is better. State the final size you reach.
- **Connector setback:** recess each USB-C so the mating plug and the case wall
  clear — coordinate the receptacle's board-edge position with the case opening
  (§9). Both connectors' insertion axes should be collinear (straight-through dongle).
- Keep the two USB-C shells and the crystals away from board edges/mounting
  features per their keepouts.

## 6. Placement & routing rules

Route by block; keep each functional cluster tight (the schematic is already laid
out this way).

- **USB 2.0 data** (`HOST_DP/DM`, `MCU_DP/DM`, `TGT_DP/DM`): route as **90 Ω
  differential pairs**, length-matched within pair, short, with ground reference
  the whole run. Put the `USBLC6` ESD arrays (`D10`/`D11`) right at the connectors,
  D± facing the plug. No series resistors on the pairs (RP2040 USB is direct).
- **CC lines** (`HOST_CC1/2` → `R6/R7`; `TGT_CC1/2` → `U2`): keep short; `R6/R7`
  (5.1 k Rd) close to `J1`.
- **SBU** (`TGT_SBU1/2` → `U8/U9` → MCU): the 1.2 V shifter low side is noise-
  sensitive; keep `U8/U9` between `J2` and the MCU, short traces.
- **RP2040 core supply:** internal LDO — `VREG_VIN` (3.3 V) → `VREG_VOUT` (1.1 V) →
  `DVDD`; **no external inductor**. Decouple `VREG_VOUT` (`C4` 1 µF) and each `DVDD`
  (`C5` 100 nF) right at the pins.
- **Crystals** `Y1`/`Y2`: load caps at the pins, guard ground, no fast signals
  crossing underneath.
- **Power:** `+5V` from `J1` VBUS → `U5` (3V3) → `U6` (1V2); `U10` load switch on
  target VBUS. Bulk caps at the LDO in/out. Star/again solid ground.
- Decoupling: one 100 nF per IC supply pin at the pin.

## 7. Debug & bring-up headers (please add)

The schematic exposes these nets for bring-up; **add accessible footprints** (they
currently have no connector — this is the source of the 2 ERC warnings):

- **SWD (required):** `SWDIO`, `SWCLK`, `+3V3`, `GND`. To keep the board tiny, use
  a **Tag-Connect TC2030-IDC** footprint (no populated connector, just pads/legs)
  rather than a pin header. A 2×3 1.27 mm Cortex-Debug header is the fallback if
  there's room.
- **BOOTSEL:** either keep `SW1`, **or** replace it with a `QSPI_SS`-to-GND **test
  pad** (recommended — see §11). A blank board auto-enters the USB bootloader and
  firmware has a `bootsel` command, so a pad is enough.
- **Test points** (small pads, top or bottom): `+5V`, `+3V3`, `+1V2`, `+1V1`,
  `TGT_VBUS`, `GND`, `RUN`, `FUSB_INT`. Label on silk.

All debug/test features must still fit the "as small as possible" goal — prefer
pads and Tag-Connect over connectors.

## 8. Silkscreen & branding

- **Logo:** place **Frank's / RestoreKit logo on the top silkscreen** (logo asset
  to be provided — import as a KiCad graphic/bitmap on the silk layer). Keep it
  clear of pads and the case openings.
- **Port labels:** clearly mark **"HOST"** at `J1` and **"TARGET"** at `J2` on the
  silk so orientation is obvious in use.
- **LED labels:** `PWR` (D1) and `STAT` (D2).
- Reference designators on silk where they fit; board name + rev + a date/version
  string in a corner; keep silk off pads.

## 9. Enclosure / case

The board must go in a **simple two-part enclosure**:

- **Openings** at both short ends for the two USB-C receptacles (align the case
  wall to the connector face; leave clearance for the mating plug boot).
- **LED windows / light pipes** on top for `D1`/`D2` (or a light-transmissive
  case), positioned to the LED locations.
- **Retention:** design for one of these and state which you chose —
  (a) **2× M2 mounting holes** with matching case bosses, or
  (b) a **slide-in shell** where the PCB edges seat in side grooves (no screws;
  best for smallest size). Option (b) needs a defined board-edge tolerance and
  keepout along both long edges.
- **Board thickness** consistent with the case and the USB-C mid-mount height.
- Deliver a **STEP of the board with components** (KiCad 3D export) so the case
  can be modeled to it. If you also produce the enclosure, deliver its STEP + a
  print-ready model (STL/STEP) and note the intended process (3D print / injection).
- Keep tall parts (USB-C shells, crystals, electrolytics if any) within the
  internal case height; flag anything that doesn't fit.

## 10. Deliverables

1. KiCad 10 project: routed `.kicad_pcb` + updated `.kicad_sch` (with debug header),
   passing **DRC 0 errors** and **ERC 0 errors / 0 warnings**.
2. Gerbers + drill (JLCPCB set), **BOM CSV** and **CPL/pick-and-place CSV** in
   JLCPCB format, generated from this project.
3. Fab notes: stackup, impedance request, any advanced rules used.
4. **Board STEP** with 3D models; enclosure model + fit notes.
5. A short readme of final size, layer count, and any part substitutions.

## 11. Acceptance criteria

- Passes DRC/ERC clean; matches the netlist in `WIRING.md` exactly (no adds/drops).
- USB pairs impedance-controlled ~90 Ω, length-matched.
- Both USB-C on opposite ends, in-line; board within the size envelope (§5).
- SWD debug reachable; test points present and labeled.
- Logo + HOST/TARGET labels on silk.
- Drops into the case with connector openings and LED windows aligned; STEP files
  provided.
- 100% LCSC parts, JLCPCB-assemblable, Basic-preferred; substitutions noted.

## 12. Open items to confirm before/at layout

These need a datasheet check (LCSC auto-generated symbols don't carry the intent —
same list as the schematic README):

1. **RP2040 core supply** — internal LDO, `VREG_VOUT` (1.1 V) → `DVDD`, decoupled
   per the RP2040 hardware design guide. No external inductor (unlike RP2350).
2. **RP2040 supplies** — `IOVDD`, `USB_VDD`, `ADC_AVDD`, `VREG_VIN` tied to 3.3 V;
   `TESTEN` (pin 19) to GND. Confirm against the datasheet.
3. **CH334F strapping** — `PSELF`→GND, `RESET#`/`OVCUR#` pull direction, and
   whether the hub needs the external 12 MHz crystal (`Y2`) vs. an internal source.
   `V5` = 5 V in, `VDD33` = internal 3.3 V out (own rail).
4. **USBLC6-2SC6** pin pairing: I/O1 = {1,6}, I/O2 = {3,4}, GND = 2, VBUS = 5.
5. **AP22653 ILIM (`R8`, 47 k)** — sets the current limit; recalc for the chosen
   target-VBUS limit.
6. **No series resistors** on the USB data pairs (RP2040 USB is direct to the hub).
7. **Firmware** already targets RP2040 (`embassy-rp`, RP2040 boot2) — no retarget
   needed. Keep the SWD header usable for bring-up.
