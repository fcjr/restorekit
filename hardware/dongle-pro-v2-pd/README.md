# Dongle-Pro v2 PD — KiCad hardware

Dongle-Pro v2 with a charger port. Plug a USB-C PD charger in next to the host cable and
the target Mac charges through the target port, at up to 20 V 3 A, while it
restores. Without a charger it behaves like the v2 and passes host 5 V.
32 x 48 mm, 4 layers, single-sided assembly, 20 BOM lines. Not built yet.
Nothing here has been on a bench.

The left 21 mm is the [v2](../dongle-pro-v2) layout almost untouched. Read its
README for the SuperSpeed path, the layer plan and the v2 firmware notes.

## What changed against the v2

| | Pro v2 | Pro v2 PD |
|---|---|---|
| Target VBUS | MT9700 switch, host 5 V, 0.68 A | TPS2121 power mux: host 5 V or charger VBUS, 3.5 A limit |
| Charger port | none | third receptacle on the top edge beside the host, same part, power pins only |
| PD toward the charger | n/a | CH224Q sink, driven over I2C |
| Target VBUS pins | one pair (A4/B9) | all four |
| HUSB311 VBUS pin | open | on target VBUS |
| Power LED | from 3V3 | from host 5 V |
| Target VBUS bleeder | none | 10k |

## How the power path works

The TPS2121 picks the higher of its two inputs. Each input has an off switch
(its OV pin held above 1.06 V):

- Host 5 V is off until the MCU pulls `HOST_EN_N` low.
- Charger VBUS is off until the MCU drives `CHG_EN` high. With the MCU
  unpowered it stays off no matter what the charger does.

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
- Handover: host on (`HOST_EN_N` low), `CHG_EN` high, then raise the charger
  above 5 V. The mux moves to the charger as soon as it is the higher input.
- Never leave both inputs on at a 5 V contract. The mux takes whichever is
  higher, by as little as 0 to 600 mV, so the Mac's 3 A can land on the host
  port. For a 5 V only charger pick one input and turn the other off.
- The current limit is 3.5 A on both inputs (the v2 had 0.68 A). On host
  power, keep what the HUSB311 advertises low.
- Charger unplugged above 5 V: the mux drops to host 5 V, but the Mac will
  most likely treat the sag as a detach. Assume the restore is lost.
- On target detach turn `CHG_EN` off straight away. A 10k bleeder takes target
  VBUS from 20 V to under 0.8 V in about 0.3 s.
- Pins: PA6 `HOST_EN_N`, PA7 `CHG_EN`. PB7 is free now.

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
4. The only discharge on target VBUS is a 10k bleeder. Fine for detach, slow
   for a 20 V to 5 V step with a light load or a hard reset.
5. No TVS on charger VBUS. PD chargers ramp to 20 V after attach so it should
   not see a 20 V hot plug, but the mux's absolute max is 24 V.
6. JLCPCB lists the receptacle as 5 V rated. That looks like a catalog default
   (USB-C contacts are specified for 20 V and more), but get the JXTCONN
   drawing before putting 20 V on it.
7. The CH224Q's supply cap is the 25 V 1 uF and sees up to 21 V.
8. 20 V now sits next to CC, SBU and the lane pins in the target receptacle.
   A crooked unplug can short it across. HUSB311 CC is good for 24 V; the
   CH9441P CC pins, the H5VU25U arrays and the SBU FETs (body diode straight
   into the MCU) are not rated for it. Find the CH9441P CC limit at least.
9. Source handover dips the output for about 100 us (slow switchover mode).
   Should be inside the Mac's debounce, not measured.
10. CH224Q DP/DM are left open and the footprint is EasyEDA's generic 2 x 2
   DFN-10. Check both against WCH's drawing.
11. CPL rotation for the TPS2121 and CH224Q.
