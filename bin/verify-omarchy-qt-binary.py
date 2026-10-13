#!/usr/bin/env python3
"""Fail if a Linux omairc binary needs a newer Qt symbol version than Omarchy ships."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINS_PATH = ROOT / "packaging" / "arch" / "omarchy-qt-pins.json"
QT_SYMBOL_RE = re.compile(r"Qt_6\.(\d+)")


def load_max_minor(path: Path = PINS_PATH) -> int:
    data = json.loads(path.read_text(encoding="utf-8"))
    minor = data.get("max_qt_symbol_minor")
    if minor is None:
        raise ValueError(f"{path}: missing max_qt_symbol_minor")
    return int(minor)


def qt_symbol_minors(binary: Path) -> set[int]:
    proc = subprocess.run(
        ["objdump", "-T", str(binary)],
        check=True,
        capture_output=True,
        text=True,
    )
    minors = set()
    for match in QT_SYMBOL_RE.finditer(proc.stdout):
        minors.add(int(match.group(1)))
    return minors


def verify(binary: Path, max_minor: int) -> list[str]:
    problems: list[str] = []
    minors = qt_symbol_minors(binary)
    for minor in sorted(minors):
        if minor > max_minor:
            problems.append(
                f"{binary} requires Qt_6.{minor} symbols; Omarchy pin allows up to Qt_6.{max_minor}"
            )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="path to omairc ELF binary")
    parser.add_argument(
        "--pins",
        type=Path,
        default=PINS_PATH,
        help="omarchy-qt-pins.json (default: packaging/arch/omarchy-qt-pins.json)",
    )
    args = parser.parse_args()
    if not args.binary.is_file():
        print(f"missing binary: {args.binary}", file=sys.stderr)
        return 2
    max_minor = load_max_minor(args.pins)
    problems = verify(args.binary, max_minor)
    for line in problems:
        print(line, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
