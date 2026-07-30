# dongle-probe

SWD probe for flashing dongle-lite and dongle-pro boards over their TC2030 pads.
The TC2030-IDC-NL cable plugs straight into J2, no adapter wiring.

RP2354A + USB-C, runs the stock [debugprobe](https://github.com/raspberrypi/debugprobe)
firmware built for Pico 2 (`DEBUG_ON_PICO=ON`): GP2 = SWCLK, GP3 = SWDIO, GP25 = activity
LED. Hold BOOT while plugging in and drop the UF2 on the mass storage device.

J2 is a shrouded 2.54 mm 2x3 box header; the TC2030-IDC-NL cable ends in a
6-pin 0.1" IDC socket whose polarizing bump only fits the key notch one way.
It's wired 1:1 with the target TC2030 pads:

| Pin | Signal |
|-----|--------|
| 1 | VTREF (bridge JP1 to power the target at 3.3V) |
| 2 | SWDIO |
| 3 | nRESET (GP6, unused by stock firmware) |
| 4 | SWCLK |
| 5 | GND |
| 6 | BOOT (GP7, unused by stock firmware) |

Pin 6 pairs with dongle boards that route their TC2030 pin 6 to the BOOTSEL
net: hold GP7 low, pulse GP6, and the target wakes up in USB boot mode.
Boards with pin 6 unrouted just ignore it.

Designed with [pcb](https://github.com/diodeinc/pcb). Schematic source is
`dongle-probe.zen` plus `components/`, symbols and footprints reused from
dongle-lite in `lib/`.

```sh
pcb build              # validate
pcb layout dongle-probe.zen   # regenerate layout/ and open KiCad
```

The routed board lives in `layout/layout.kicad_pcb` (don't re-run
`pcb layout` unless you want to re-place and re-route from scratch). The
`gen_*.py` scripts are KiCad-python helpers that were used to place and
route it; they need KiCad's bundled interpreter:

```sh
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3 gen_place.py
```

## Manufacturing

`mfg-jlcpcb/` has the full JLCPCB package: gerber zip, `bom.csv`,
`cpl.csv` (all LCSC parts), and `README-JLCPCB.md` with ordering notes.

## Case

`case/` has a snap-fit printed enclosure (CadQuery, `uv run case.py`).
