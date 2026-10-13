#!/usr/bin/env python3
"""Install Omarchy-pinned Qt packages from Arch Linux Archive for pacman builds."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINS_PATH = ROOT / "packaging" / "arch" / "omarchy-qt-pins.json"
PACMAN_CONF = Path("/etc/pacman.conf")


def load_pins(path: Path = PINS_PATH) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    for key in ("archive_base", "install_order", "packages", "hold_in_pacman"):
        if key not in data:
            raise ValueError(f"{path}: missing {key!r}")
    return data


def package_url(data: dict, name: str) -> str:
    version = data["packages"][name]
    base = data["archive_base"].rstrip("/")
    letter = name[0]
    return f"{base}/{letter}/{name}/{name}-{version}-x86_64.pkg.tar.zst"


def install_pins(data: dict) -> None:
    urls = []
    for name in data["install_order"]:
        if name not in data["packages"]:
            raise ValueError(f"install_order lists unknown package {name!r}")
        urls.append(package_url(data, name))
    subprocess.run(
        ["pacman", "-U", "--noconfirm", *urls],
        check=True,
    )


def hold_packages(data: dict) -> None:
    hold = data["hold_in_pacman"]
    if not PACMAN_CONF.is_file():
        raise FileNotFoundError(f"missing {PACMAN_CONF}")
    hold_line = f"IgnorePkg = {' '.join(hold)}"
    text = PACMAN_CONF.read_text(encoding="utf-8")
    lines = text.splitlines()
    out: list[str] = []
    replaced = False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("#") and stripped.lstrip("#").strip().startswith("IgnorePkg"):
            if not replaced:
                out.append("# Omairc release-package: pin Qt to Omarchy.")
                out.append(hold_line)
                replaced = True
            continue
        if stripped.startswith("IgnorePkg"):
            if not replaced:
                existing = stripped.split("=", 1)[1].strip().split()
                merged = sorted(set(existing) | set(hold))
                out.append(f"IgnorePkg = {' '.join(merged)}")
                replaced = True
            continue
        out.append(line)
    if not replaced:
        out.extend(
            [
                "",
                "# Omairc release-package: pin Qt to Omarchy.",
                hold_line,
            ]
        )
    PACMAN_CONF.write_text("\n".join(out).rstrip() + "\n", encoding="utf-8")


def self_test() -> None:
    data = load_pins()
    for name in data["install_order"]:
        url = package_url(data, name)
        if not url.endswith(".pkg.tar.zst"):
            raise ValueError(f"bad archive url for {name}: {url}")
    if data.get("max_qt_symbol_minor") is None:
        raise ValueError("max_qt_symbol_minor is required")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=("install", "hold-pacman", "self-test"),
        help="install pinned Qt, append IgnorePkg, or validate pin metadata",
    )
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
        return 0
    data = load_pins()
    if args.command == "install":
        install_pins(data)
        return 0
    if args.command == "hold-pacman":
        hold_packages(data)
        return 0
    raise AssertionError(args.command)


if __name__ == "__main__":
    sys.exit(main())
