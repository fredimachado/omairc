#!/usr/bin/env python3
"""Tests for bin/derive-build-versions.py."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "bin" / "derive-build-versions.py"


def run_derive(**kwargs: str | int) -> subprocess.CompletedProcess[str]:
    args = [
        sys.executable,
        str(SCRIPT),
        "--git-ref",
        str(kwargs["git_ref"]),
        "--sha",
        str(kwargs["sha"]),
        "--run-number",
        str(kwargs["run_number"]),
        "--run-attempt",
        str(kwargs["run_attempt"]),
        "--format",
        "json",
    ]
    return subprocess.run(args, check=False, capture_output=True, text=True)


class DeriveBuildVersionsTest(unittest.TestCase):
    def test_tag_release_is_unsuffixed(self) -> None:
        proc = run_derive(
            git_ref="refs/tags/v0.8.0",
            sha="abcdef1234567890abcdef1234567890abcdef12",
            run_number=237,
            run_attempt=2,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        values = json.loads(proc.stdout)
        self.assertEqual(
            values,
            {
                "release": "0.8.0",
                "display": "0.8.0",
                "artifact": "0.8.0",
                "arch_pkgrel": "1",
            },
        )

    def test_master_snapshot(self) -> None:
        proc = run_derive(
            git_ref="refs/heads/master",
            sha="abcdef1234567890abcdef1234567890abcdef12",
            run_number=237,
            run_attempt=1,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        values = json.loads(proc.stdout)
        self.assertEqual(values["release"], "0.8.0")
        self.assertEqual(values["display"], "0.8.0+master.gabcdef1")
        self.assertEqual(values["artifact"], "0.8.0-master.gabcdef1.a1")
        self.assertEqual(values["arch_pkgrel"], "1.237")

    def test_rerun_changes_only_artifact(self) -> None:
        first = json.loads(
            run_derive(
                git_ref="refs/heads/master",
                sha="abcdef1",
                run_number=42,
                run_attempt=1,
            ).stdout
        )
        second = json.loads(
            run_derive(
                git_ref="refs/heads/master",
                sha="abcdef1",
                run_number=42,
                run_attempt=3,
            ).stdout
        )
        self.assertEqual(first["display"], second["display"])
        self.assertNotEqual(first["artifact"], second["artifact"])
        self.assertEqual(second["artifact"], "0.8.0-master.gabcdef1.a3")

    def test_malformed_sha(self) -> None:
        proc = run_derive(
            git_ref="refs/heads/master",
            sha="not-a-sha",
            run_number=1,
            run_attempt=1,
        )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("invalid git sha", proc.stderr)

    def test_tag_mismatch(self) -> None:
        proc = run_derive(
            git_ref="refs/tags/v9.9.9",
            sha="abcdef1",
            run_number=1,
            run_attempt=1,
        )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("does not match version.pri", proc.stderr)

    def test_missing_version_file(self) -> None:
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "derive_build_versions", SCRIPT
        )
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)

        missing = ROOT / "tests" / "missing-version.pri"
        self.assertFalse(missing.is_file())
        with self.assertRaises(ValueError) as ctx:
            module.read_canonical_version(missing)
        self.assertIn("missing version file", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
