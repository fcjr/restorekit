# Dongle-Pro v2 PD — KiCad hardware

Dongle-Pro v2 with a charger port. Plug a USB-C PD charger in next to the host cable and
the target Mac charges through the target port, at up to 20 V 3 A, while it
restores. Without a charger it behaves like the v2 and passes host 5 V.
32 x 48 mm, 4 layers, single-sided assembly, 22 BOM lines. Not built yet.
Nothing here has been on a bench.

The left 21 mm is the [v2](../dongle-pro-v2) layout almost untouched. Read its
README for the SuperSpeed path, the layer plan and the v2 firmware notes.

## What changed against the v2

| | Pro v2 | Pro v2 PD |
|---|---|---|
| Target VBUS | MT9700 switch, host 5 V, 0.68 A | TPS2121 power mux: host 5 V at 1.2 A or charger VBUS at 3.6 A |
| Charger port | none | third receptacle on the top edge beside the host, same part, power pins only |
| PD toward the charger | n/a | CH224Q sink, driven over I2C |
| Target VBUS pins | one pair (A4/B9) | all four |
| HUSB311 VBUS pin | open | on target VBUS |
| Power LED | from 3V3 | from host 5 V |
| Target VBUS bleeder | none | 5k (2x 10k), 2x 10 uF bulk |
| Target CC to the mux | direct | through 100k |
| Target SBU | direct to the level shifters | 1k, TVS channel, 1k on the MCU side |

## How the power path works

The TPS2121 gives the charger priority whenever charger VBUS is above about
4.5 V, whatever the host voltage is, and switches between the two in about
5 us. Each input has an off switch (its OV pin held above 1.06 V):

- Host 5 V is off until the MCU pulls `HOST_EN_N` low.
- Charger VBUS is off with the MCU unpowered, no matter what the charger does.
  `CHG_EN` lets it through only while it is under about 6.4 V. `CHG_HV` lets
  it through at any voltage.

The current limit follows the source by itself: 1.2 A on host power, 3.6 A on
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
- On host power the target gets 1.2 A at most. Advertise less than that.
- Charger unplugged at a 5 V contract: the mux is back on the host in
  microseconds, but the limit drops to 1.2 A with it. If the Mac was drawing
  more it browns out. Above 5 V the Mac will most likely treat the sag as a
  detach either way.
- `CHG_EN` alone cannot put more than about 6.4 V on the target port, so the
  CH224Q's 9 V default is blocked in hardware. Raise `CHG_HV` only after the
  Mac has accepted a contract above 5 V, and drop it before going back to 5 V.
- On target detach turn `CHG_EN` off straight away. A 5k bleeder takes target
  VBUS from 20 V to under 0.8 V in about 0.3 s.
- Pins: PA6 `HOST_EN_N`, PA7 `CHG_EN`, PB0 `CHG_HV`. PB7 is free now.

## Regenerating

Same as the v2: `gen/build.sh`, then `$KPY gen/fab.py`, `gen/panel.sh` for the
3-up panel. 0 DRC violations, 0 unconnected, schematic parity clean.

## Check before ordering

Everything on the v2 list except item 7, plus:

1. A Mac in DFU or restore mode takes a PD contract above 5 V and charges
   from it. This is the whole point and I have not seen it measured.
2. CH224Q with an off-table CFG1 value is untested, so it uses the datasheet's
   6.8k (9 V). Confirm I2C comes up in that mode and the source caps read back.
3. TPS2121 at 3 A dissipates about 0.5 W in a 2 x 2.5 mm package. Check the
   temperature.
4. The only discharge on target VBUS is a 5k bleeder. Fine for detach, slow
   for a 20 V to 5 V step with a light load or a hard reset.
5. No TVS on charger VBUS. PD chargers ramp to 20 V after attach so it should
   not see a 20 V hot plug, but the mux's absolute max is 24 V.
6. JLCPCB lists the receptacle as 5 V rated. That looks like a catalog default
   (USB-C contacts are specified for 20 V and more), but get the JXTCONN
   drawing before putting 20 V on it.
7. The CH224Q's supply cap is the 25 V 1 uF and sees up to 21 V.
8. 20 V sits next to CC, SBU and the lane pins in the target receptacle, and a
   crooked unplug can short across. CC and SBU are protected now (see the
   table). The lane pins are not: the CH9441P switch pins are 3.6 V parts
   behind 5 V TVS arrays. The fix is a 330 nF cap plus a discharge resistor
   on the connector side of each of the eight lane pins, and there is no room
   for the resistors on the through-hole row. Caps without them would hold
   20 V after a short and dump it into the next Mac plugged in, which is
   worse than losing the dongle's mux.
9. The SBU protection resistors run hot if the short is held: about 0.2 W in
   an 0402 at 20 V. Fine for an unplug, not for a jammed connector. During
   the short about 1.5 mA per pin still goes into the MCU's clamp diodes;
   the CH32X035 datasheet gives no injection rating.
10. The 1k in each SBU drain and the 1k at the pin sit in the serial path.
   Low level is about 0.26 V against a 1.2 V rail. Check it on a scope.
11. TPS2121 status pin as the current limit switch is my idea, not a
   datasheet circuit, and 100k is the top of the allowed range. Measure both
   limits.
12. Charger side bulk is 10 uF + 1 uF nominal against USB's 10 uF sink limit.
   DC bias should bring it under, not measured.
13. The 6.4 V trip on `CHG_EN` comes from 1% resistors and a 1.06 V
   reference: 6.1 to 6.6 V. Check 5.5 V passes and 9 V does not.
14. The target CH9441P now reads CC through 100k. Its input current is not in
   the datasheet. If it picks the wrong lane, that resistor is why.
15. CPL rotation for the TPS2121 and CH224Q.
