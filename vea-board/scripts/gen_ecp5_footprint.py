#!/usr/bin/env python3
"""Generate a KiCad 9 BGA footprint for an ECP5 package from a Lattice pinout CSV.

The footprint has one pad for each ball in the CSV. The ball layout follows JEDEC (top view):
the letters are the rows from top to bottom, and the numbers are the columns from left to right.
"""

import argparse
import re
import uuid
from pathlib import Path

from gen_ecp5_symbol import Q, ball_key, dump, load_balls, num

# The CSV does not give the physical dimensions of the package.
PITCH = 0.8
PAD_D = 0.5
BODY = 14.0
# Clearance from the package body to the courtyard. KLC uses 1 mm for BGA.
COURTYARD_GAP = 1.0
SILK_GAP = 0.11
FAB_CHAMFER = 1.0

FAB_W = 0.1
SILK_W = 0.12
CRTYD_W = 0.05


def split_ball(ball: str) -> tuple[str, int]:
    row, col = re.fullmatch(r"([A-Z]+)(\d+)", ball).groups()
    return row, int(col)


def ball_positions(balls) -> tuple[dict[str, tuple[float, float]], int, int]:
    """Return {ball: pad center}, the number of columns, and the number of rows.

    The grid is centered on the origin.
    """
    # Sort the row letters as A..Z and then AA, AB, and so on.
    rows = sorted({split_ball(b)[0] for b in balls}, key=lambda r: (len(r), r))
    ncols = max(split_ball(b)[1] for b in balls)
    x0 = (ncols - 1) / 2
    y0 = (len(rows) - 1) / 2
    pos = {}
    for b in balls:
        row, col = split_ball(b)
        pos[b] = ((col - 1 - x0) * PITCH, (rows.index(row) - y0) * PITCH)
    return pos, ncols, len(rows)


def make_uuid(name: str, item: str) -> list:
    # A fixed UUID for each item keeps the file the same when you run the script again.
    return ["uuid", Q(str(uuid.uuid5(uuid.NAMESPACE_URL, f"{name}/{item}")))]


def text_effects() -> list:
    return ["effects", ["font", ["size", "1", "1"], ["thickness", "0.15"]]]


def prop(fp: str, key: str, value: str, y: float, layer: str, hide: bool = False) -> list:
    node = ["property", Q(key), Q(value), ["at", "0", num(y), "0"], ["layer", Q(layer)]]
    if hide:
        node.append(["hide", "yes"])
    node += [make_uuid(fp, f"prop/{key}"), text_effects()]
    return node


def line(fp: str, tag: str, a, b, layer: str, width: float) -> list:
    return [
        "fp_line",
        ["start", num(a[0]), num(a[1])],
        ["end", num(b[0]), num(b[1])],
        ["stroke", ["width", num(width)], ["type", "solid"]],
        ["layer", Q(layer)],
        make_uuid(fp, tag),
    ]


def rect(fp: str, tag: str, half: float, layer: str, width: float) -> list:
    return [
        "fp_rect",
        ["start", num(-half), num(-half)],
        ["end", num(half), num(half)],
        ["stroke", ["width", num(width)], ["type", "solid"]],
        ["fill", "no"],
        ["layer", Q(layer)],
        make_uuid(fp, tag),
    ]


def poly(fp: str, tag: str, points, layer: str, width: float) -> list:
    return [
        "fp_poly",
        ["pts", *[["xy", num(x), num(y)] for x, y in points]],
        ["stroke", ["width", num(width)], ["type", "solid"]],
        ["fill", "no"],
        ["layer", Q(layer)],
        make_uuid(fp, tag),
    ]


def pad(fp: str, ball: str, x: float, y: float) -> list:
    return [
        "pad", Q(ball), "smd", "circle",
        ["at", num(x), num(y)],
        ["size", num(PAD_D), num(PAD_D)],
        ["layers", Q("F.Cu"), Q("F.Paste"), Q("F.Mask")],
        make_uuid(fp, f"pad/{ball}"),
    ]


def footprint_node(name: str, description: str, pads: dict[str, tuple[float, float]]) -> list:
    h = BODY / 2
    s = h + SILK_GAP
    c = h + COURTYARD_GAP
    ch = FAB_CHAMFER
    # The chamfer on the fab outline and the silk mark both show the A1 corner.
    fab_outline = [(-h + ch, -h), (h, -h), (h, h), (-h, h), (-h, -h + ch)]
    return [
        "footprint", Q(name),
        ["version", "20241229"],
        ["generator", Q("gen_ecp5_footprint")],
        ["generator_version", Q("9.0")],
        ["layer", Q("F.Cu")],
        ["descr", Q(description)],
        ["tags", Q(f"BGA {len(pads)} {PITCH:g} ECP5 Lattice caBGA")],
        prop(name, "Reference", "REF**", -c - 1, "F.SilkS"),
        prop(name, "Value", name, c + 1, "F.Fab"),
        prop(name, "Datasheet", "", 0, "F.Fab", hide=True),
        prop(name, "Description", description, 0, "F.Fab", hide=True),
        ["attr", "smd"],
        poly(name, "fab/outline", fab_outline, "F.Fab", FAB_W),
        rect(name, "silk/outline", s, "F.SilkS", SILK_W),
        line(name, "silk/a1_x", (-s - 1, -s), (-s, -s), "F.SilkS", SILK_W),
        line(name, "silk/a1_y", (-s, -s - 1), (-s, -s), "F.SilkS", SILK_W),
        rect(name, "crtyd/outline", c, "F.CrtYd", CRTYD_W),
        [
            "fp_text", "user", Q("${REFERENCE}"),
            ["at", "0", "0", "0"],
            ["layer", Q("F.Fab")],
            make_uuid(name, "fab/reference"),
            text_effects(),
        ],
        *[pad(name, b, x, y) for b, (x, y) in pads.items()],
        ["embedded_fonts", "no"],
    ]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("csv", type=Path, help="Lattice pinout CSV")
    ap.add_argument("--name", help="footprint name (default: made from the CSV ball grid)")
    ap.add_argument(
        "--lib", type=Path,
        help="footprint library directory (default: CustomFootprints.pretty next to the CSV)",
    )
    args = ap.parse_args()
    lib = args.lib or args.csv.parent / "CustomFootprints.pretty"

    # The symbol uses these ball names as pin numbers, so each pin gets a pad.
    balls = sorted(load_balls(args.csv), key=ball_key)
    pads, ncols, nrows = ball_positions(balls)
    if max(abs(v) for xy in pads.values() for v in xy) + PAD_D / 2 > BODY / 2:
        raise SystemExit(f"error: the ball grid is larger than the {BODY:g} mm body")

    grid = f"{ncols}x{nrows}"
    name = args.name or f"Lattice_caBGA-{len(balls)}_{BODY:g}x{BODY:g}mm_Layout{grid}_P{PITCH:g}mm"
    description = (
        f"Lattice caBGA-{len(balls)}, {BODY:g}x{BODY:g} mm body, {grid} balls, "
        f"{PITCH:g} mm pitch, {PAD_D:g} mm pads"
    )
    out = lib / f"{name}.kicad_mod"
    lib.mkdir(exist_ok=True)
    out.write_text(dump(footprint_node(name, description, pads)) + "\n")
    print(f"wrote {name}: {len(pads)} pads, {grid} grid -> {out}")


if __name__ == "__main__":
    main()
