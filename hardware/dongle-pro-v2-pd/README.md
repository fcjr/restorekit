# Dongle-Pro v2 PD — KiCad hardware

Dongle-Pro v2 with a charger port. Plug a USB-C PD charger in next to the host cable and
the target Mac charges through the target port, at up to 20 V 3 A, while it
restores. Without a charger it behaves like the v2 and passes host 5 V.
32 x 55 mm, 4 layers, single-sided assembly, 25 BOM lines. Not built yet.
Nothing here has been on a bench.

The left 21 mm is the [v2](../dongle-pro-v2) layout almost untouched. Read its
README for the SuperSpeed path, the layer plan and the v2 firmware notes.

## What changed against the v2

| | Pro v2 | Pro v2 PD |
|---|---|---|
| Target VBUS | MT9700 switch, host 5 V, 0.68 A | TPS2121 power mux: host 5 V at 1.5 A or charger VBUS at 3.9 A |
| Charger port | none | third receptacle on the top edge beside the host, same part, power pins only |
| PD toward the charger | n/a | CH224Q sink, driven over I2C |
| Target VBUS pins | one pair (A4/B9) | all four |
| HUSB311 VBUS pin | open | on target VBUS |
| Power LED | from 3V3 | from host 5 V |
| Target VBUS bleeder | none | 5k (2x 10k), 2x 10 uF bulk |
| Target CC to the mux | direct | through 10k |
| Target SBU | direct to the level shifters | series 30 V FET, gate at 3.3 V |
| Target lanes | direct to the TVS arrays | 330 nF in series + 200k bleed on every pin |
| Length | 48 mm | 55 mm, the extra 7 mm is the lane protection |

## How the power path works

The TPS2121 gives the charger priority whenever charger VBUS is above about
4.5 V, whatever the host voltage is, and switches between the two in about
5 us. Each input has an off switch (its OV pin held above 1.06 V):

- Host 5 V is off until the MCU pulls `HOST_EN_N` low.
- Charger VBUS is off with the MCU unpowered, no matter what the charger does.
  `CHG_EN` lets it through only while it is under about 6.4 V. `CHG_HV` lets
  it through at any voltage.

The current limit follows the source by itself: 1.5 A on host power, 3.9 A on
the charger (the mux's status pin switches the limit resistor).

The CH224Q does the PD talking to the charger. It has its own Rd and 32 V
tolerant CC pins, which the CH32X035's PD pins do not. The MCU reads the
charger's source caps from it and tells it which voltage to ask for. The
HUSB311 is the source toward the Mac, same as the v2.

## Firmware

On top of the v2 notes:

- Second bit-banged I2C bus on PA4 (SCL) / PA5 (SDA) to the CH224Q, address
  0x22 or 0x23. The pull-ups are inside the CH224Q, so the bus is dead with no
  charger plugged in. Poll for an ACK to detect the charger.
- The CH224Q asks for 9 V on its own at plug-in (CFG1 strap). Write 0 to
  reg 0x0A to drop to 5 V before doing anything else.
- Read the charger's PDOs from 0x60..0x8F and offer the Mac the fixed ones,
  capped at 3 A. When the Mac requests one: write the voltage to 0x0A, wait
  for it, then send PS_RDY.
- Handover: host on (`HOST_EN_N` low), CH224Q at 5 V, then `CHG_EN` high. The mux moves to
  the charger by itself, at any charger voltage, and back to the host if the
  charger goes away. Both inputs can stay on.
- On host power the target gets 1.5 A at most. Advertise less than that.
- At a 5 V contract, offer the Mac no more than the host limit even with a
  charger in. Then pulling the charger is invisible to it. The real charging
  happens at 9 V and up, where an unplug is a detach whatever you do.
- Charger unplugged above 5 V: the Mac will most likely treat the sag as a
  detach. Assume the restore is lost.
- `CHG_EN` alone cannot put more than about 6.4 V on the target port, so the
  CH224Q's 9 V default is blocked in hardware. Raise `CHG_HV` only after the
  Mac has accepted a contract above 5 V, and drop it before going back to 5 V.
- On target detach turn `CHG_EN` off straight away. A 5k bleeder takes target
  VBUS from 20 V to under 0.8 V in about 0.3 s.
- Pins: PA6 `HOST_EN_N`, PA7 `CHG_EN`, PB0 `CHG_HV`. PB7 is free now.
- Debug: `clk dio 3v3 gnd` holes at 2.54 mm on the left edge take a pin
  header (not fitted), and there is a TC2030 Tag-Connect footprint below them
  with the other dongles' pinout. Its pin 6 (BOOT) is not connected here.

## Regenerating

Same as the v2: `gen/build.sh`, then `$KPY gen/fab.py`, `gen/panel.sh` for the
3-up panel. 0 DRC violations, 0 unconnected, schematic parity clean.

## Check before ordering

Everything on the v2 list except item 7, plus:

1. A Mac in DFU or restore mode takes a PD contract above 5 V and charges
   from it. This is the whole point and I have not seen it measured. What
   points to yes: the Lite firmware already gets a PD Request from the Mac
   on the DFU port, so it is a working sink there, and iBoot ships its own
   "charging over USB-C" screens, so charging does not need macOS.
2. CH224Q with an off-table CFG1 value is untested, so it uses the datasheet's
   6.8k (9 V). Confirm I2C comes up in that mode and the source caps read back.
3. TPS2121 at 3 A dissipates about 0.5 W in a 2 x 2.5 mm package with no
   thermal pad. Its charger input is tied to about 70 mm2 of bottom copper
   through four vias. Still a guess until measured. If it runs hot, cap what
   the firmware offers at 2.25 A (45 W at 20 V).
4. The only discharge on target VBUS is a 5k bleeder. Fine for detach, slow
   for a 20 V to 5 V step with a light load or a hard reset.
5. No TVS on charger VBUS. PD chargers ramp to 20 V after attach so it should
   not see a 20 V hot plug, but the mux's absolute max is 24 V.
6. The receptacles are hanxia HX TYPE-C 24P QCHT, rated 20 V 5 A in its
   datasheet. It is the part the footprint was drawn for. The v2 swapped to a
   JXTCONN part when this one went out of stock; that one is rated 5 V, so do
   not substitute it here.
7. The CH224Q's supply cap is the 25 V 1 uF and sees up to 21 V.
8. The target lanes now have a second set of AC caps in series with the Macs'
   own: about 130 nF per lane end to end, inside USB's 75 to 265 nF. Eight
   more 0402s sit in the pairs, on a path with two muxes and no redriver.
   My estimate for the dongle is 6 to 7 dB at 2.5 GHz against roughly 20 dB
   for the whole link, so short cables on both sides. Check it trains.
9. The SBU clamp FETs pass the 1.2 V serial through an AO3400A with 3.3 V on
   the gate. Worst-case threshold leaves about 0.6 V of gate drive. Check the
   serial on a scope.
10. TPS2121 status pin as the current limit switch is my idea, not a
   datasheet circuit. Measure both limits (1.5 A and 3.9 A nominal).
11. The 6.4 V trip on `CHG_EN` comes from 1% resistors and a 1.06 V
   reference: 6.1 to 6.6 V. Check 5.5 V passes and 9 V does not.
12. The target CH9441P reads CC through 10k. A 20 V short puts 1.5 mA into
   its 6 V pins' clamps.
13. CPL rotation for the TPS2121, CH224Q and the AO3400As.
