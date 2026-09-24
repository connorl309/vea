#!/usr/bin/env python3
"""Generate an ECP5 pinout CSV from the Project Trellis database.

This script reads iodb.json, which holds the bank, the DQS group and the alternate
function of every ball. That file is the data that nextpnr-ecp5 itself uses, so a pin
that this file accepts is a pin that place and route accepts.

The Lattice pinout file of a density, for example fpga-ecp5_12u-pinout.csv, holds the
same ball map with more detail: the power pins, the configuration pins, the differential
pairs and a Dual Function column. Prefer the Lattice file to build a schematic symbol.
Use this file to learn what the toolchain accepts, and --verify to prove that the two
agree.

The alternate functions differ between densities in the same package. The LFE5U-12F has
two PLLs and the LFE5U-45F has four, so four balls carry a PLL input on the 45F and no
PLL input on the 12F. Generate this file for the density of the board.
"""

import argparse
import csv
import json
import re
import sys
from pathlib import Path

# A user I/O pad name, for example PL47C. The power and configuration balls of a Lattice
# pinout do not match this, and the Trellis database does not hold them.
GPIO_RE = re.compile(r"^P[LRTB]\d+[A-D]$")

sys.path.insert(0, str(Path(__file__).resolve().parent))

DEFAULT_DB = "/usr/local/share/trellis/database/ECP5"

COLUMNS = ["ball", "bank", "function", "dqs", "row", "col", "pio"]


def load_pinout(db_root: Path, device: str, package: str) -> list[dict]:
    iodb = json.loads((db_root / device / "iodb.json").read_text())

    packages = iodb["packages"]
    if package not in packages:
        raise SystemExit(
            f"{device} has no package {package}. It has: {', '.join(sorted(packages))}"
        )

    # pio_metadata keys off the die location, not off the ball, so index it first.
    meta = {(m["row"], m["col"], m["pio"]): m for m in iodb["pio_metadata"]}

    rows = []
    for ball, loc in packages[package].items():
        m = meta.get((loc["row"], loc["col"], loc["pio"]), {})
        rows.append(
            {
                "ball": ball,
                "bank": m.get("bank", ""),
                "function": m.get("function", ""),
                "dqs": m.get("dqs", ""),
                "row": loc["row"],
                "col": loc["col"],
                "pio": loc["pio"],
            }
        )

    # Sort by ball the way a person reads a ball grid: letter row, then number.
    rows.sort(key=lambda r: (r["ball"][0], int(r["ball"][1:])))
    return rows


def verify(rows: list[dict], lattice_csv: Path, package: str) -> int:
    """Compare the Trellis balls against a Lattice pinout file. Return the fault count.

    The two sources must agree on which balls are user I/O, on the bank of each one and on
    the alternate function of each one. A difference means that the Lattice file is for
    another density or another package, and a ball taken from it can be wrong.

    The alternate function is the check that matters most. A density changes the PLL count,
    so it changes which balls reach a PLL, while the ball map and the banks stay the same.
    """
    from gen_ecp5_symbol import load_balls  # Both scripts read the same CSV shapes.

    lat_names, lat_duals = load_balls(lattice_csv, package)
    lat_table = {r["ball"]: r for r in read_lattice_banks(lattice_csv, package)}

    trellis = {r["ball"]: r for r in rows}
    # A Lattice pinout also lists power and configuration balls. Only the user I/O balls
    # are in the Trellis database, so compare against those.
    lat_io = {b for b, n in lat_names.items() if GPIO_RE.match(n)}

    faults = 0
    for ball in sorted(lat_io - set(trellis)):
        print(f"  only in {lattice_csv.name}: {ball} ({lat_names[ball]})")
        faults += 1
    for ball in sorted(set(trellis) - lat_io):
        print(f"  only in the Trellis database: {ball}")
        faults += 1
    checked = 0
    for ball in sorted(lat_io & set(trellis)):
        want, got = lat_table[ball]["bank"], str(trellis[ball]["bank"])
        if want != got:
            print(f"  bank differs for {ball}: {lattice_csv.name} says {want}, Trellis says {got}")
            faults += 1

        # Both sources join several names with a slash, as in D0/MOSI/IO0. Compare the
        # sets of names, and let the Lattice field hold more names than the Trellis one.
        fn = trellis[ball].get("function")
        if not fn:
            continue
        checked += 1
        want = {x.strip() for x in fn.split("/") if x.strip()}
        got = {x.strip() for x in lat_duals.get(ball, "").split("/") if x.strip()}
        if not want <= got:
            shown = lat_duals.get(ball) or "nothing"
            print(f"  function differs for {ball}: Trellis says {fn}, "
                  f"{lattice_csv.name} says {shown}")
            faults += 1
    print(f"  compared {checked} alternate functions")
    return faults


def read_lattice_banks(path: Path, package: str) -> list[dict]:
    """Return [{ball, bank}] from a Lattice pinout CSV."""
    from gen_ecp5_symbol import read_pin_table

    out = []
    for row in read_pin_table(path):
        ball = row.get(package, "").strip()
        if ball and ball not in ("-", "Unused"):
            out.append({"ball": ball, "bank": row.get("Bank", "").strip()})
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--device", default="LFE5U-12F", help="for example LFE5U-12F")
    ap.add_argument("--package", default="CABGA256", help="for example CABGA256")
    ap.add_argument("--db", type=Path, default=Path(DEFAULT_DB), help="Trellis database")
    ap.add_argument("-o", "--output", type=Path, help="CSV to write; default is stdout")
    ap.add_argument(
        "--verify", type=Path, metavar="LATTICE_CSV",
        help="compare the balls and the banks against a Lattice pinout file, then stop. "
             "The exit status is 1 if the two disagree",
    )
    args = ap.parse_args()

    rows = load_pinout(args.db, args.device, args.package)

    if args.verify:
        print(f"{args.device} {args.package}: Trellis database against {args.verify.name}")
        faults = verify(rows, args.verify, args.package)
        if faults:
            raise SystemExit(f"{faults} difference(s) found")
        print(f"  {len(rows)} user I/O balls agree, banks agree")
        return

    out = args.output.open("w", newline="") if args.output else None
    try:
        writer = csv.DictWriter(out or __import__("sys").stdout, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)
    finally:
        if out:
            out.close()

    if args.output:
        named = sum(1 for r in rows if r["function"])
        print(f"{args.output}: {len(rows)} balls, {named} with an alternate function")


if __name__ == "__main__":
    main()
