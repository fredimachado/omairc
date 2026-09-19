#!/usr/bin/env python3
from __future__ import annotations

import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: gen-omaircversion-h.py <version> <output-path>", file=sys.stderr)
        return 1

    version = sys.argv[1]
    out = Path(sys.argv[2])
    wanted = (
        "#ifndef OMAIRC_VERSION_H\n"
        "#define OMAIRC_VERSION_H\n"
        f'#define OMAIRC_VERSION "{version}"\n'
        "#endif\n"
    )

    if out.is_file() and out.read_text(encoding="utf-8") == wanted:
        return 0

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(wanted, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
