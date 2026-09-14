#!/usr/bin/env python3
"""Inject key events into a real evdev keyboard so Omakeycast can see them.

Omakeycast skips hl-virtual-keyboard, so wtype/ydotool chords never overlay.
The caller must be in group `input`. Prefer a device whose name contains
"keyboard" (on this laptop: AT Translated Set 2 keyboard).
"""

from __future__ import annotations

import argparse
import array
import fcntl
import glob
import os
import re
import struct
import sys
import time

EVENT_FORMAT = "llHHi"
EV_SYN = 0
EV_KEY = 1
SYN_REPORT = 0

SKIP_NAME = (
    "hl-virtual-keyboard|power[ -]?button|sleep[ -]?button|lid[ -]?switch|"
    "video[ -]?bus|consumer control|system control|wacom|touchpad|mouse|"
    "accelerometer|speaker|headphone|hdmi"
)

KEY_CODES = {
    "esc": 1,
    "escape": 1,
    "1": 2,
    "2": 3,
    "3": 4,
    "4": 5,
    "5": 6,
    "6": 7,
    "7": 8,
    "8": 9,
    "9": 10,
    "0": 11,
    "minus": 12,
    "-": 12,
    "tab": 15,
    "q": 16,
    "w": 17,
    "e": 18,
    "r": 19,
    "t": 20,
    "y": 21,
    "u": 22,
    "i": 23,
    "o": 24,
    "p": 25,
    "a": 30,
    "s": 31,
    "d": 32,
    "f": 33,
    "g": 34,
    "h": 35,
    "j": 36,
    "k": 37,
    "l": 38,
    "z": 44,
    "x": 45,
    "c": 46,
    "v": 47,
    "b": 48,
    "n": 49,
    "m": 50,
    "comma": 51,
    ",": 51,
    "dot": 52,
    ".": 52,
    "slash": 53,
    "/": 53,
    "equal": 13,
    "=": 13,
    "leftbrace": 26,
    "[": 26,
    "rightbrace": 27,
    "]": 27,
    "semicolon": 39,
    ";": 39,
    "apostrophe": 40,
    "'": 40,
    "grave": 41,
    "`": 41,
    "backslash": 43,
    "\\": 43,
    "space": 57,
    " ": 57,
    "enter": 28,
    "return": 28,
    "up": 103,
    "left": 105,
    "right": 106,
    "down": 108,
    "ctrl": 29,
    "shift": 42,
    "alt": 56,
    "super": 125,
    "meta": 125,
}

# Scancodes are physical, so the character a shifted key produces is whatever
# the compositor's layout says. These pairings assume US QWERTY.
SHIFTED_CHARS = {
    "!": "1",
    "@": "2",
    "#": "3",
    "$": "4",
    "%": "5",
    "^": "6",
    "&": "7",
    "*": "8",
    "(": "9",
    ")": "0",
    "_": "minus",
    "+": "equal",
    "{": "leftbrace",
    "}": "rightbrace",
    ":": "semicolon",
    '"': "apostrophe",
    "~": "grave",
    "|": "backslash",
    "<": "comma",
    ">": "dot",
    "?": "slash",
}


def eviocgname(length: int = 256) -> int:
    return 2 << 30 | length << 16 | ord("E") << 8 | 0x06


def device_name(path: str) -> str:
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    try:
        buf = array.array("B", [0] * 256)
        fcntl.ioctl(fd, eviocgname(256), buf, True)
        return buf.tobytes().split(b"\x00", 1)[0].decode("utf-8", "replace")
    finally:
        os.close(fd)


def find_keyboard() -> str:
    skip = re.compile(SKIP_NAME, re.IGNORECASE)
    for path in sorted(glob.glob("/dev/input/event*")):
        try:
            name = device_name(path)
        except OSError:
            continue
        if not skip.search(name) and "keyboard" in name.lower():
            return path
    raise SystemExit(
        "no evdev keyboard found (need group input); pass --device explicitly"
    )


def emit(fd: int, ev_type: int, code: int, value: int) -> None:
    now = time.time()
    sec = int(now)
    usec = int((now - sec) * 1_000_000)
    os.write(fd, struct.pack(EVENT_FORMAT, sec, usec, ev_type, code, value))


def sync(fd: int) -> None:
    emit(fd, EV_SYN, SYN_REPORT, 0)


def key(name: str) -> int:
    code = KEY_CODES.get(name.lower())
    if code is None:
        raise SystemExit(f"unknown key {name!r}")
    return code


def tap(fd: int, code: int, hold: float = 0.04) -> None:
    emit(fd, EV_KEY, code, 1)
    sync(fd)
    time.sleep(hold)
    emit(fd, EV_KEY, code, 0)
    sync(fd)


def tap_shifted(fd: int, code: int, hold: float = 0.03) -> None:
    """Shift+key at typing speed. chord() pads mods and would break cadence."""
    shift = key("shift")
    emit(fd, EV_KEY, shift, 1)
    sync(fd)
    time.sleep(0.01)
    try:
        tap(fd, code, hold=hold)
    finally:
        # Same hazard as chord(): a held Shift would outlive this process.
        emit(fd, EV_KEY, shift, 0)
        sync(fd)


def chord(fd: int, mods: list[str], name: str) -> None:
    mod_codes = [key(mod) for mod in mods]
    code = key(name)
    for mod_code in mod_codes:
        emit(fd, EV_KEY, mod_code, 1)
        sync(fd)
        time.sleep(0.02)
    try:
        time.sleep(0.03)
        tap(fd, code, hold=0.05)
        time.sleep(0.02)
    finally:
        # A modifier left down here would stay stuck for the whole session.
        for mod_code in reversed(mod_codes):
            emit(fd, EV_KEY, mod_code, 0)
            sync(fd)
            time.sleep(0.02)


def type_text(fd: int, text: str, delay: float) -> None:
    for char in text:
        shifted = SHIFTED_CHARS.get(char)
        if shifted is not None:
            tap_shifted(fd, key(shifted))
        elif char.isupper():
            tap_shifted(fd, key(char.lower()))
        else:
            tap(fd, key(char), hold=0.03)
        time.sleep(delay)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Inject evdev key events into a real keyboard"
    )
    parser.add_argument(
        "--device",
        default=os.environ.get("OMAIRC_EVDEV_DEVICE", ""),
        help="evdev path (default: first real keyboard)",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)
    chord_p = sub.add_parser("chord", help="ctrl+slash, alt+down, escape, …")
    chord_p.add_argument("spec")
    type_p = sub.add_parser("type", help="type characters one by one")
    type_p.add_argument("text")
    type_p.add_argument("--delay", type=float, default=0.08)
    args = parser.parse_args()

    path = args.device or find_keyboard()
    try:
        fd = os.open(path, os.O_WRONLY)
    except PermissionError:
        raise SystemExit(
            f"cannot write {path}: add this user to group input and log out"
        ) from None
    try:
        if args.cmd == "chord":
            parts = [part for part in args.spec.split("+") if part]
            if not parts:
                raise SystemExit("empty chord")
            chord(fd, parts[:-1], parts[-1])
        else:
            type_text(fd, args.text, args.delay)
    finally:
        os.close(fd)
    print(path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
