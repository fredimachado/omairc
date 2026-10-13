#!/usr/bin/env python3

import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class OmarchyQtBinaryTests(unittest.TestCase):
    def test_pins_self_test(self) -> None:
        subprocess.run(
            [str(ROOT / "bin/arch-install-omarchy-qt-pins.py"), "self-test"],
            check=True,
            cwd=ROOT,
        )

    def test_verify_rejects_qt_6_12_system_binary(self) -> None:
        system = Path("/usr/bin/omairc")
        if not system.is_file():
            self.skipTest("/usr/bin/omairc not installed")
        proc = subprocess.run(
            [str(ROOT / "bin/verify-omarchy-qt-binary.py"), str(system)],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if "Qt_6.12" not in subprocess.check_output(
            ["objdump", "-T", str(system)], text=True
        ):
            self.skipTest("system omairc is not linked against Qt 6.12")
        self.assertEqual(proc.returncode, 1)
        self.assertIn("Qt_6.12", proc.stderr)

    def test_verify_accepts_local_build_when_present(self) -> None:
        local = ROOT / "build" / "omairc"
        if not local.is_file():
            self.skipTest("build/omairc missing")
        subprocess.run(
            [str(ROOT / "bin/verify-omarchy-qt-binary.py"), str(local)],
            check=True,
            cwd=ROOT,
        )


if __name__ == "__main__":
    unittest.main()
