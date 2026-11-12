# Dongle-Pro v2 — KiCad hardware

Cost-reduced Dongle-Pro: USB 3 passthrough with cable flip on both ports, for
less than the Lite's parts cost. 21 x 48 mm, 4 layers, single-sided assembly,
18 BOM lines. Not built yet. Nothing here has been on a bench.

## What changed against the Pro

| | Pro | Pro v2 |
|---|---|---|
| SuperSpeed path | GL3510 hub + HD3SS3220 + HD3SS3212 | no hub: two CH9441P lane muxes back to back |
| MCU | RP2354A + crystal + inductor | CH32X035G8U6 (48 MHz HSI, no crystal) |
| PD PHY | FUSB302B | HUSB311 (TCPCI) |
| USB 2.0 hub | n/a (inside GL3510) | CH334P |
| Target VBUS switch | AP22653 | MT9700-N, 10k -> 0.68 A |
| SBU level shift | 2x 74AVC1T45 + 1.2 V LDO | 2x HL2302 + five resistors |
| ESD | USBLC6 + TPD4E05U06 | H5VU25U everywhere |
| Debug | Tag-Connect | two pads (clk, dio) + 3v3/gnd pads |

The CH32X035 has its own PD PHY but it cannot send SOP'' Debug (no RST-2 in
K-code slot 2), so the HUSB311 does the Apple VDMs.

## How the SuperSpeed path works

The lanes never touch a hub. Each receptacle's two lane sets go into a CH9441P,
which reads that port's CC pins and connects the live set to its common side.
The two common sides are wired together down the middle of the board. USB 2.0
(DFU, and anything that falls back) goes through the CH334P: port 1 is the
target, port 2 is the MCU.

## Layout

| Layer | Use |
|---|---|
| F.Cu | parts, the A-row lane pairs, the mux-to-mux corridor |
| In1 (GND1) | solid GND, nothing routed |
| In2 (GND2) | GND, plus three short jumpers: both D+ joins at the receptacles and SBU2 |
| B.Cu | the B-row lane pairs near each receptacle, both USB 2.0 trunks, power, slow signals crossing under the corridor |

Lane pairs are 0.211 mm wide. A-row pairs and the corridor match to 0.00 mm,
B-row pairs to 0.03 mm (`gen/lengths.py`). Order with JLC04161H-7628 and
impedance control. Every bend is an arc. `gen/audit.py` lists what sits near
the lanes, how far each lane via is from a ground via, the narrowest copper on
each power net and how far each decoupling cap is from its pin.

Target VBUS is capped at 0.68 A and runs on 0.3 mm copper. A 10k bleeds it
down when the switch is off, and the HUSB311 VBUS pin is wired to it so the
PHY can see the rail.

## Regenerating

```sh
cd hardware/dongle-pro-v2
gen/build.sh        # schematic, netlist check, placement, routing, pours, silk, DRC summary
KPY=/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
$KPY gen/fab.py     # mfg-jlcpcb/: gerbers, drill, BOM, CPL, STEP
```

`gen/board.py` is the netlist, `gen/place.py` the placement, `gen/route.py`
every track. `gen/netcheck.py` diffs the exported netlist against `board.py`
pin by pin. Unlike the Pro, the checked-in board is exactly what the scripts
produce: 0 DRC violations, 0 unconnected, schematic parity clean apart from
field warnings.

## Firmware

Needs a port to `ch32-hal` and a TCPCI driver. Pin map is at the top of the MCU
block in `gen/board.py`. Things the port has to know:

- I2C to the HUSB311 is bit-banged on PB10/PB9 with internal pull-ups; INT_N on
  PB8 also uses the internal pull-up.
- SBU serial: USART2 for a normal cable, USART3 for a flipped one. Drive PB6
  high to enable the level shifters.
- Firmware update is WCH's ROM USB bootloader. Enter it from the app, or hold
  `boot` while plugging in.
- Set the HUSB311 to source (Rp) straight after reset. Unpowered or unconfigured
  it shows dead-battery Rd to the target.

## Check before ordering

1. M4/M5 restores really run at USB 3 (one vendor's claim so far).
2. HUSB311 sends and receives SOP'' Debug against a real Mac.
3. Two CH9441P in series with no redriver pass 5 Gbps, and the target one
   behaves with the HUSB311 driving CC. The target mux runs from +3V3 on both
   its VDD and VBUS pins, which the datasheet allows for VDD but does not show.
4. CH334P reports self-powered (no PSELF pin on this package). If it reports
   bus-powered, restores fail the way they did on the Lite.
5. The FET level shifter at 1.2 V. With the shifters off, each SBU line sees
   5.6k to ground.
6. Firmware fits in 62 KB.
7. Each receptacle uses one VBUS pin pair (A4/B9). The other pair is boxed in
   by the D+/D- and SBU escapes and is left open.
8. CPL rotations for the QFNs, SOT-23s and DFNs against JLCPCB's preview.
