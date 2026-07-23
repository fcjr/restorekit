# Dongle-Pro-Power — KiCad hardware

The [Dongle-Pro](../dongle-pro/) plus a third USB-C port that takes up to
100 W from a PD charger and re-sources it to the target Mac — charge and
restore over one cable, host optional for the charging part. See
[`../README.md`](../README.md) for the family overview and `./PRD.md` for the
spec, power architecture, and the confirm-before-layout checklist.

## Status

- **Schematic:** complete, generated (`gen/board.py`, 165 components), ERC 0
  errors. The power section is provisioned for the full 100 W (5 A parts and
  copper); the 60 W → 100 W step is firmware-only (SOP′ e-marker gate).
- **Layout:** not started. The plan is the Pro's board grown ~12 mm for the
  third receptacle and power section; all SS/impedance work carries over.
- **Firmware:** `--features power` builds and parks the power path safe. The
  PD source policy engine is M2 (PRD §5).

## Regenerating

```sh
cd hardware/dongle-pro-power
python3 gen/board.py            # netlist -> dongle-pro-power.kicad_sch
kicad-cli sch erc dongle-pro-power.kicad_sch
```

Parts live in the shared library (`lib -> ../dongle-lite/lib`).
