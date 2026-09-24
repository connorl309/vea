#!/usr/bin/env python3
"""Generate a multi-unit KiCad 9 symbol for an ECP5 FPGA from a Lattice pinout CSV.

Units: Config (configuration and JTAG), Power, and one GPIO unit each for PL, PR, PT, PB.
If the library already has a symbol with the same name, only that symbol is replaced.

Two CSV shapes work:

  * The Lattice pinout file of a device family, for example fpga-ecp5_12u-pinout.csv. Its
    header starts with PAD, and it holds one column per package. Use --package to pick
    one. This shape also holds the Dual Function column, so the pin names carry the
    alternate function with no other input. Prefer this shape.
  * The older die pad extraction, whose header starts with PADN. It has no alternate
    function, so --trellis-pinout must supply one.

Take the pinout file of the density on the board. The alternate functions differ between
densities in one package.
"""

import argparse
import csv
import math
import re
from dataclasses import dataclass
from pathlib import Path

# KiCad wants pins on a 100 mil grid.
GRID = 2.54
PIN_LEN = GRID
# A wide estimate, so that the names on the two sides of a unit never touch.
NAME_CHAR_W = 1.3

BALL_RE = re.compile(r"^[A-Z]{1,2}\d{1,2}$")
GPIO_RE = re.compile(r"^P([LRTB])(\d+)([A-D])$")
POWER_RE = re.compile(r"^(GND|VCC|VCCAUX|VCCIO\d+)$")

CONFIG_PINS = [  # (name, side of the unit, electrical type)
    ("PROGRAMN", "left", "input"),
    ("INITN", "left", "bidirectional"),
    ("DONE", "left", "bidirectional"),
    ("CCLK", "left", "bidirectional"),
    ("CFG_0", "left", "input"),
    ("CFG_1", "left", "input"),
    ("CFG_2", "left", "input"),
    ("TCK", "right", "input"),
    ("TDI", "right", "input"),
    ("TDO", "right", "output"),
    ("TMS", "right", "input"),
]
CONFIG_INFO = {name: (side, etype) for name, side, etype in CONFIG_PINS}

GPIO_SIDES = ("L", "R", "T", "B")


@dataclass(frozen=True)
class Pin:
    number: str  # ball
    name: str
    etype: str
    # The plain pad name, without an alternate function. The sort order and the
    # differential pairs come from this name, so an annotation cannot disturb them.
    base: str = ""


class Q(str):
    """A string that KiCad needs in double quotes."""


def num(x: float) -> str:
    s = f"{x:.4f}".rstrip("0").rstrip(".")
    return "0" if s in ("", "-0") else s


def atom(a) -> str:
    if isinstance(a, Q):
        return '"' + a.replace("\\", "\\\\").replace('"', '\\"') + '"'
    return a


def dump(node: list, depth: int = 0) -> str:
    """Write a nested list in the layout of the KiCad symbol editor."""
    tabs = "\t" * depth
    split = next((i for i, x in enumerate(node) if isinstance(x, list)), len(node))
    head = " ".join(atom(x) for x in node[:split])
    if split == len(node):
        return f"{tabs}({head})"
    kids = [dump(k, depth + 1) for k in node[split:]]
    return "\n".join([f"{tabs}({head}", *kids, f"{tabs})"])


def ball_key(ball: str):
    row = re.match(r"[A-Z]+", ball).group()
    return (len(row), row, int(ball[len(row):]))


def read_pin_table(path: Path) -> list[dict]:
    """Return the rows of a Lattice pinout CSV.

    A comment line in these files is sometimes in quotes, so a leading # does not find
    every one. The header row is the first row whose first field is PAD or PADN, and the
    rows above it are the preamble.
    """
    with open(path, newline="") as f:
        rows = list(csv.reader(f))
    start = next((i for i, r in enumerate(rows) if r and r[0].strip() in ("PAD", "PADN")), None)
    if start is None:
        raise SystemExit(f"error: no PAD or PADN header row in {path}")
    header = [c.strip() for c in rows[start]]
    return [dict(zip(header, r)) for r in rows[start + 1:] if len(r) == len(header)]


def load_balls(path: Path, package: str | None = None) -> tuple[dict[str, str], dict[str, str]]:
    """Return ({ball: pad name}, {ball: alternate function}) for every ball with a pin."""
    rows = read_pin_table(path)
    if not rows:
        raise SystemExit(f"error: no pin rows in {path}")

    columns = list(rows[0])
    if "Pin/Ball Function" in columns:
        name_col, dual_col = "Pin/Ball Function", "Dual Function"
        if package is None:
            raise SystemExit(
                "error: this pinout holds several packages. Name one with --package. "
                "It has: " + ", ".join(c for c in columns if c.startswith(("CABGA", "CSFBGA", "TQFP")))
            )
    else:
        # The die pad extraction carries one package, in its last column, and no
        # alternate function.
        name_col, dual_col = "FNC", None
        package = package or columns[-1]

    if package not in columns:
        raise SystemExit(f"error: {path} has no column {package}")

    balls: dict[str, str] = {}
    duals: dict[str, str] = {}
    for row in rows:
        ball = row[package].strip()
        # A pad of another package has "-", "Unused" or a net name in this column.
        if not BALL_RE.match(ball):
            continue
        if ball in balls:
            raise SystemExit(f"error: ball {ball} appears twice in {path}")
        balls[ball] = row[name_col].strip()
        dual = row.get(dual_col, "").strip() if dual_col else ""
        # An empty alternate function is a dash in these files.
        if dual and dual != "-":
            duals[ball] = dual
    return balls, duals


def load_alt_functions(path: Path) -> dict[str, str]:
    """Return {ball: alternate function} from a Project Trellis pinout CSV.

    The pinout files from Lattice carry no alternate function. The Trellis database
    carries one for the clock inputs, the PLL inputs, the VREF pins and the DQS groups.
    """
    with open(path, newline="") as f:
        return {
            row["ball"].strip(): row["function"].strip()
            for row in csv.DictReader(f)
            if row.get("function", "").strip()
        }


def pair_key(pin: Pin):
    m = GPIO_RE.match(pin.base)
    return (int(m.group(2)), (ord(m.group(3)) - ord("A")) // 2)


def split_halves(pins: list[Pin]):
    k = (len(pins) + 1) // 2
    # Both pins of a differential pair stay on the same side.
    while 0 < k < len(pins) and pair_key(pins[k - 1]) == pair_key(pins[k]):
        k += 1
    return pins[:k], pins[k:]


def build_units(balls: dict[str, str], alt: dict[str, str] | None = None):
    """Return [(unit title, left pins, right pins)]."""
    alt = alt or {}
    config_ball: dict[str, str] = {}
    gpio: dict[str, list[Pin]] = {s: [] for s in GPIO_SIDES}
    power: list[Pin] = []
    unknown = []
    for ball, fnc in balls.items():
        m = GPIO_RE.match(fnc)
        if m:
            # The alternate function goes into the name, so that the schematic shows
            # which balls can take a clock and which balls reach a PLL.
            label = f"{fnc}/{alt[ball]}" if ball in alt else fnc
            gpio[m.group(1)].append(Pin(ball, label, "bidirectional", fnc))
        elif fnc in CONFIG_INFO:
            config_ball[fnc] = ball
        elif POWER_RE.match(fnc):
            power.append(Pin(ball, fnc, "power_in"))
        else:
            unknown.append(f"{fnc} ({ball})")
    if unknown:
        raise SystemExit("error: no unit for these pin functions: " + ", ".join(unknown))

    def config(side):
        return [Pin(config_ball[n], n, t) for n, s, t in CONFIG_PINS if s == side]

    power.sort(key=lambda p: (p.name, ball_key(p.number)))
    units = [
        ("Config", config("left"), config("right")),
        ("Power", [p for p in power if p.name != "GND"], [p for p in power if p.name == "GND"]),
    ]
    for side in GPIO_SIDES:
        pins = sorted(gpio[side], key=lambda p: (int(GPIO_RE.match(p.base).group(2)), p.base[-1]))
        units.append((f"GPIO P{side}", *split_halves(pins)))
    return units


def font(hide: bool = False) -> list:
    effects = ["effects", ["font", ["size", "1.27", "1.27"]]]
    if hide:
        effects.append(["hide", "yes"])
    return effects


def prop(name: str, value: str, y: float = 0, hide: bool = False) -> list:
    return ["property", Q(name), Q(value), ["at", "0", num(y), "0"], font(hide)]


def pin_node(p: Pin, x: float, y: float, angle: int) -> list:
    return [
        "pin", p.etype, "line",
        ["at", num(x), num(y), str(angle)],
        ["length", num(PIN_LEN)],
        ["name", Q(p.name), font()],
        ["number", Q(p.number), font()],
    ]


def unit_node(sym_name: str, index: int, title: str, left: list[Pin], right: list[Pin]) -> list:
    rows = max(len(left), len(right))
    longest = lambda pins: max((len(p.name) for p in pins), default=0)
    text_w = (longest(left) + longest(right)) * NAME_CHAR_W + GRID
    half = math.ceil(text_w / (2 * GRID)) * GRID
    node = [
        "symbol", Q(f"{sym_name}_{index}_1"),
        ["unit_name", Q(title)],
        [
            "rectangle",
            ["start", num(-half), num(GRID)],
            ["end", num(half), num(-GRID * rows)],
            ["stroke", ["width", "0.254"], ["type", "default"]],
            ["fill", ["type", "background"]],
        ],
    ]
    for i, p in enumerate(left):
        node.append(pin_node(p, -half - PIN_LEN, -GRID * i, 0))
    for i, p in enumerate(right):
        node.append(pin_node(p, half + PIN_LEN, -GRID * i, 180))
    return node


def symbol_node(name: str, description: str, units) -> list:
    node = [
        "symbol", Q(name),
        ["exclude_from_sim", "no"],
        ["in_bom", "yes"],
        ["on_board", "yes"],
        # All units share the top edge, so the fields sit just above every unit.
        prop("Reference", "U", 3.81),
        prop("Value", name, 6.35),
        prop("Footprint", "", hide=True),
        prop("Datasheet", "", hide=True),
        prop("Description", description, hide=True),
        prop("ki_keywords", "FPGA ECP5 Lattice", hide=True),
    ]
    for i, (title, left, right) in enumerate(units, start=1):
        node.append(unit_node(name, i, title, left, right))
    node.append(["embedded_fonts", "no"])
    return node


def form_end(text: str, start: int) -> int:
    """Return the index after the s-expression that starts at text[start]."""
    depth = 0
    in_str = False
    i = start
    while i < len(text):
        c = text[i]
        if in_str:
            if c == "\\":
                i += 1
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    raise SystemExit("error: unbalanced parentheses in the symbol library")


def merge(lib_text: str, symbol_text: str, name: str) -> str:
    # The closing quote keeps this from matching the unit sub-symbols.
    start = lib_text.find(f'(symbol "{name}"')
    if start >= 0:
        line_start = lib_text.rfind("\n", 0, start) + 1
        return lib_text[:line_start] + symbol_text + lib_text[form_end(lib_text, start):]
    close = lib_text.rstrip().rfind(")")
    return lib_text[:close] + symbol_text + "\n" + lib_text[close:]


HEADER = (
    "(kicad_symbol_lib\n"
    "\t(version 20241209)\n"
    '\t(generator "gen_ecp5_symbol")\n'
    '\t(generator_version "9.0")\n'
)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("csv", type=Path, help="Lattice pinout CSV")
    ap.add_argument(
        "--package", default="CABGA256",
        help="package column to read, for a pinout that holds several (default: CABGA256)",
    )
    ap.add_argument(
        "--trellis-pinout", type=Path,
        help="CSV from gen_ecp5_pinout.py. Use it only for a pinout with no Dual Function "
             "column, because the alternate function then has no other source",
    )
    ap.add_argument("--name", default="ECP5U-xx-BGA256", help="symbol name")
    ap.add_argument(
        "--out", type=Path,
        help="symbol library to update (default: CustomSymbols.kicad_sym next to the CSV)",
    )
    args = ap.parse_args()
    out = args.out or args.csv.parent / "CustomSymbols.kicad_sym"

    balls, duals = load_balls(args.csv, args.package)
    # The pinout file is the first source of an alternate function. A Trellis CSV fills
    # the gap for a file that has no Dual Function column.
    alt = duals or (load_alt_functions(args.trellis_pinout) if args.trellis_pinout else {})
    units = build_units(balls, alt)
    description = f"Lattice ECP5 FPGA {args.name}, generated from {args.csv.name}"
    symbol_text = dump(symbol_node(args.name, description, units), 1)

    lib_text = out.read_text() if out.exists() else HEADER + ")\n"
    out.write_text(merge(lib_text, symbol_text, args.name))

    total = 0
    for i, (title, left, right) in enumerate(units, start=1):
        print(f"unit {i}: {title:9s} {len(left) + len(right):3d} pins ({len(left)} left, {len(right)} right)")
        total += len(left) + len(right)
    print(f"wrote {args.name}: {total} pins -> {out}")


if __name__ == "__main__":
    main()
