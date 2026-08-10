#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["reportlab>=4.2"]
# ///
"""Print product labels on a DYMO LabelWriter.

Two kinds, both sized for DYMO Large Shipping Labels 30256 (2-5/16" x 4", part
120300 / 10294), which the DYMO PPD calls PageSize w167h288:

    bag     goes on the outside: mark, dongle count, website
    start   goes inside: thanks, then the three steps to a first restore

Everything is drawn as vectors so the thermal head gets solid black instead of
a halftone. Run `labels.py <kind> --preview` to check a layout without burning
a label.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

from reportlab.lib.colors import black
from reportlab.pdfbase.pdfmetrics import stringWidth
from reportlab.pdfgen import canvas

PRINTER = "DYMO_LabelWriter_450_Twin_Turbo"
PAGE_SIZE = "w167h288"  # the 30256, as the DYMO PPD names it
SITE = "restorekit.org"

# The label is portrait, in points, the way it feeds: 2-5/16" across, 4" down.
W, H = 167.0, 288.0
MARGIN = 16.0

BODY = "Helvetica"
BOLD = "Helvetica-Bold"
MONO = "Courier-Bold"

STEPS = [
    (
        "INSTALL",
        "Get the cli or the desktop app for macOS, linux, or windows at restorekit.org",
        None,
    ),
    (
        "PLUG IN",
        "Host side into your computer, target side into the mac you're restoring.",
        None,
    ),
    ("RESTORE", "Then run:", "sudo restorekit restore"),
]


# --- drawing primitives ---


def draw_mark(c: canvas.Canvas, x: float, y: float, size: float) -> None:
    """The restorekit mark: rounded box + waveform, from apps/landing favicon.

    Drawn on the source SVG's 32-unit grid with y flipped, so the coordinates
    below match the SVG one-for-one.
    """
    c.saveState()
    c.translate(x, y + size)
    c.scale(size / 32.0, -size / 32.0)
    c.setStrokeColor(black)
    c.setLineCap(1)
    c.setLineJoin(1)

    c.setLineWidth(1.8)
    c.roundRect(7, 7, 18, 18, 3, stroke=1, fill=0)

    c.setLineWidth(2.1)
    wave = c.beginPath()
    wave.moveTo(4, 16)
    wave.lineTo(10, 16)
    wave.lineTo(12.2, 11)
    wave.lineTo(16, 21)
    wave.lineTo(19, 16)
    wave.lineTo(28, 16)
    c.drawPath(wave, stroke=1, fill=0)
    c.restoreState()


def draw_centered(c: canvas.Canvas, text: str, font: str, size: float, y: float) -> None:
    c.setFont(font, size)
    c.setFillColor(black)
    c.drawString((W - stringWidth(text, font, size)) / 2, y, text)


def draw_tracked(
    c: canvas.Canvas, text: str, font: str, size: float, y: float, tracking: float
) -> None:
    """Centered text with letter spacing, which reportlab has no helper for."""
    width = stringWidth(text, font, size) + tracking * (len(text) - 1)
    c.setFont(font, size)
    c.setFillColor(black)
    x = (W - width) / 2
    for ch in text:
        c.drawString(x, y, ch)
        x += stringWidth(ch, font, size) + tracking


def draw_rule(c: canvas.Canvas, y: float) -> None:
    c.setStrokeColor(black)
    c.setLineWidth(1.0)
    c.line(MARGIN + 8, y, W - MARGIN - 8, y)


def draw_footer(c: canvas.Canvas, size: float) -> None:
    draw_tracked(c, SITE, BODY, size, MARGIN, 1.4)


def wrap(text: str, font: str, size: float, width: float) -> list[str]:
    lines: list[str] = []
    line = ""
    for word in text.split():
        trial = f"{line} {word}".strip()
        if line and stringWidth(trial, font, size) > width:
            lines.append(line)
            line = word
        else:
            line = trial
    if line:
        lines.append(line)
    return lines


# --- labels ---


def draw_bag(c: canvas.Canvas, qty: int) -> None:
    mark = 62.0
    draw_mark(c, (W - mark) / 2, H - MARGIN - mark, mark)

    word_y = H - MARGIN - mark - 26
    draw_centered(c, "restorekit", BOLD, 25.0, word_y)
    draw_rule(c, word_y - 20)

    # The count, which is the whole point of the label.
    count_size = 78.0
    count_y = word_y - 50 - count_size * 0.72
    draw_centered(c, str(qty), BOLD, count_size, count_y)
    draw_tracked(c, "DONGLE" if qty == 1 else "DONGLES", BOLD, 17.0, count_y - 26, 2.6)

    draw_footer(c, 12.0)


def draw_start(c: canvas.Canvas, _qty: int) -> None:
    mark = 28.0
    draw_mark(c, (W - mark) / 2, H - MARGIN - mark, mark)

    top = H - MARGIN - mark
    draw_centered(c, "thanks for your purchase!", BODY, 10.5, top - 16)
    draw_tracked(c, "START HERE", BOLD, 14.0, top - 38, 2.2)
    draw_rule(c, top - 50)

    # Steps: a hanging number, then the title and wrapped body beside it.
    text_x = MARGIN + 14
    text_w = W - MARGIN - text_x
    size = 8.5
    y = top - 70

    for i, (title, body, cmd) in enumerate(STEPS, start=1):
        c.setFont(BOLD, 13.0)
        c.setFillColor(black)
        c.drawString(MARGIN, y - 1, str(i))
        c.setFont(BOLD, 11.0)
        c.drawString(text_x, y, title)
        y -= 13

        c.setFont(BODY, size)
        for line in wrap(body, BODY, size, text_w):
            c.drawString(text_x, y, line)
            y -= 10

        if cmd:
            # Commands go full width, shrunk to fit rather than clipped.
            cmd_size = min(10.0, 10.0 * (W - 2 * MARGIN) / stringWidth(cmd, MONO, 10.0))
            y -= 5
            draw_centered(c, cmd, MONO, cmd_size, y)
            y -= 10

        y -= 8

    draw_footer(c, 11.0)


DRAW = {"bag": draw_bag, "start": draw_start}


def render(path: Path, kind: str, qty: int, flip: bool) -> None:
    c = canvas.Canvas(str(path), pagesize=(W, H))
    c.setTitle(f"restorekit {kind} label")
    if flip:
        # The same label, other end out of the printer first.
        c.translate(W, H)
        c.rotate(180)
    DRAW[kind](c, qty)
    c.showPage()
    c.save()


# --- cli ---


def ask_qty() -> int:
    while True:
        raw = input("how many dongles are in the bag? ").strip()
        try:
            qty = int(raw)
        except ValueError:
            print("enter a whole number", file=sys.stderr)
            continue
        if qty < 1:
            print("must be at least 1", file=sys.stderr)
            continue
        return qty


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="kind", required=True)

    def options(p: argparse.ArgumentParser) -> argparse.ArgumentParser:
        p.add_argument("-n", "--copies", type=int, default=1, help="labels to print (default 1)")
        p.add_argument("-p", "--printer", default=PRINTER, help=f"CUPS queue (default {PRINTER})")
        p.add_argument(
            "-r",
            "--roll",
            default="Left",
            choices=["Left", "Right", "Auto"],
            help="twin turbo roll (default Left)",
        )
        p.add_argument("--flip", action="store_true", help="rotate 180 if it feeds out upside down")
        p.add_argument("--preview", action="store_true", help="open the PDF instead of printing")
        return p

    bag = options(sub.add_parser("bag", help="outside of the bag: mark, dongle count, website"))
    bag.add_argument("qty", nargs="?", type=int, help="dongles in the bag (prompted if omitted)")
    options(sub.add_parser("start", help="inside of the bag: how to get started"))

    return ap.parse_args()


def main() -> int:
    args = parse_args()
    if args.copies < 1:
        print("copies must be at least 1", file=sys.stderr)
        return 1

    qty = 0
    if args.kind == "bag":
        qty = args.qty if args.qty is not None else ask_qty()
        if qty < 1:
            print("quantity must be at least 1", file=sys.stderr)
            return 1

    stem = f"bag-{qty}" if args.kind == "bag" else args.kind
    out = Path(tempfile.mkdtemp(prefix="restorekit-label-")) / f"{stem}.pdf"
    render(out, args.kind, qty, args.flip)

    if args.preview:
        subprocess.run(["open", str(out)], check=True)
        print(out)
        return 0

    proc = subprocess.run(
        [
            "lp",
            "-d", args.printer,
            "-n", str(args.copies),
            "-o", f"PageSize={PAGE_SIZE}",
            "-o", f"InputSlot={args.roll}",
            "-o", "DymoPrintQuality=Text",
            "-o", "fit-to-page=false",
            str(out),
        ],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        return proc.returncode

    plural = "" if args.copies == 1 else "s"
    detail = f", {qty} dongles each" if args.kind == "bag" else ""
    print(f"{proc.stdout.strip()} — {args.copies} {args.kind} label{plural}{detail}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
