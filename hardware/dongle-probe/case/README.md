# dongle-probe case

Snap-fit two-piece enclosure (bottom tray + lid) generated with
[CadQuery](https://github.com/cadquery/cadquery).

```sh
uv run case.py
```

Outputs to `output/`:

- `dongle-probe-case.step` — full assembly
- `bottom.stl`, `lid.stl` — print-ready (lid pre-flipped flat side down)

Features:

- USB-C notch at the top end, with an outer overmold recess (the shell sits
  1.4 mm inside the board edge, so bulky cable overmolds can enter the notch)
- open bay at the bottom end for the 2x3 1.27 mm SWD header — the TC2030
  IDC socket seats from above — with JP1 (VTREF jumper) reachable through
  the same opening
- BOOT button hole, PWR/ACT LED windows, engraved labels (0.6 mm deep for a
  two-color filament swap on the lid's first layers)
- friction-fit lip with snap bumps, hold-down posts pressing the PCB into
  the tray

Print the tray cavity-up and the lid as exported; no supports needed.
