#!/usr/bin/env python3
"""Tests for bin/derive-build-versions.py."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "bin" / "derive-build-versions.py"


def load_module():
    spec = importlib.util.spec_from_file_location("derive_build_versions", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


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
    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_module()

    def test_tag_release_is_unsuffixed(self) -> None:
        proc = run_derive(
            git_ref="refs/tags/v0.9.0",
            sha="abcdef1234567890abcdef1234567890abcdef12",
            run_number=237,
            run_attempt=2,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        values = json.loads(proc.stdout)
        self.assertEqual(
            values,
            {
                "release": "0.9.0",
                "display": "0.9.0",
                "artifact": "0.9.0",
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
        self.assertEqual(values["release"], "0.9.0")
        self.assertEqual(values["display"], "0.9.0+master.gabcdef1")
        self.assertEqual(values["artifact"], "0.9.0-master.gabcdef1.a1")
        self.assertEqual(values["arch_pkgrel"], "0.237")

    def test_pull_request_snapshot(self) -> None:
        proc = run_derive(
            git_ref="refs/pull/235/merge",
            sha="abcdef1234567890abcdef1234567890abcdef12",
            run_number=237,
            run_attempt=1,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        values = json.loads(proc.stdout)
        self.assertEqual(values["display"], "0.9.0+pr235.gabcdef1")
        self.assertEqual(values["artifact"], "0.9.0-pr235.gabcdef1.a1")
        self.assertEqual(values["arch_pkgrel"], "1")

    def test_non_master_branch_snapshot(self) -> None:
        proc = run_derive(
            git_ref="refs/heads/cursor/master-snapshot-versions-df37",
            sha="abcdef1",
            run_number=12,
            run_attempt=1,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        values = json.loads(proc.stdout)
        self.assertEqual(
            values["display"],
            "0.9.0+cursor-master-snapshot-versions-df37.gabcdef1",
        )
        self.assertEqual(
            values["artifact"],
            "0.9.0-cursor-master-snapshot-versions-df37.gabcdef1.a1",
        )
        self.assertEqual(values["arch_pkgrel"], "1")

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
        self.assertEqual(second["artifact"], "0.9.0-master.gabcdef1.a3")

    def test_arch_snapshot_ordering(self) -> None:
        release = "0.9.0"
        early = self.module.derive(
            git_ref="refs/heads/master",
            sha="abcdef1",
            run_number=1,
            run_attempt=1,
            canonical_version=release,
        )
        late = self.module.derive(
            git_ref="refs/heads/master",
            sha="abcdef1",
            run_number=237,
            run_attempt=1,
            canonical_version=release,
        )
        tagged = self.module.derive(
            git_ref="refs/tags/v0.9.0",
            sha="abcdef1",
            run_number=237,
            run_attempt=1,
            canonical_version=release,
        )

        early_version = self.module.arch_package_version(release, early["arch_pkgrel"])
        late_version = self.module.arch_package_version(release, late["arch_pkgrel"])
        release_version = self.module.arch_package_version(
            release, tagged["arch_pkgrel"]
        )

        self.assertLess(self.module.compare_arch_versions(early_version, late_version), 0)
        self.assertLess(self.module.compare_arch_versions(late_version, release_version), 0)
        self.assertLess(self.module.compare_arch_versions(early_version, release_version), 0)

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

    def test_unsupported_ref(self) -> None:
        proc = run_derive(
            git_ref="refs/tags/beta-0.9.0",
            sha="abcdef1",
            run_number=1,
            run_attempt=1,
        )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("unsupported git ref", proc.stderr)

    def test_missing_version_file(self) -> None:
        missing = ROOT / "tests" / "missing-version.pri"
        self.assertFalse(missing.is_file())
        with self.assertRaises(ValueError) as ctx:
            self.module.read_canonical_version(missing)
        self.assertIn("missing version file", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
