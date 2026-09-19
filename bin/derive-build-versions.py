#!/usr/bin/env python3
"""Derive release, display, artifact, and Arch pkgrel versions for CI builds."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION_PRI = ROOT / "version.pri"

SHORT_SHA_RE = re.compile(r"^[0-9a-fA-F]{7,40}$")
ARTIFACT_SAFE_RE = re.compile(r"^[0-9A-Za-z][0-9A-Za-z.+_-]*$")
DISPLAY_SAFE_RE = re.compile(r"^[0-9A-Za-z][0-9A-Za-z.+_-]*$")
PKGREL_RE = re.compile(r"^[0-9]+(?:\.[0-9]+)*$")


def read_canonical_version(version_pri: Path = VERSION_PRI) -> str:
    if not version_pri.is_file():
        raise ValueError(f"missing version file: {version_pri}")
    for line in version_pri.read_text(encoding="utf-8").splitlines():
        match = re.match(r"^VERSION\s*=\s*(\S+)\s*$", line)
        if match:
            version = match.group(1)
            if not version:
                raise ValueError("version.pri VERSION is empty")
            return version
    raise ValueError("version.pri must set VERSION")


def short_sha(sha: str) -> str:
    cleaned = sha.strip().lower()
    if not SHORT_SHA_RE.fullmatch(cleaned):
        raise ValueError(f"invalid git sha: {sha!r}")
    return cleaned[:7]


def is_tag_ref(git_ref: str) -> bool:
    return git_ref.startswith("refs/tags/v")


def tag_version(git_ref: str) -> str:
    return git_ref[len("refs/tags/v") :]


def validate_field(name: str, value: str, pattern: re.Pattern[str]) -> None:
    if not pattern.fullmatch(value):
        raise ValueError(f"{name} is not package-safe: {value!r}")


def derive(
    *,
    git_ref: str,
    sha: str,
    run_number: int,
    run_attempt: int,
    canonical_version: str,
) -> dict[str, str]:
    if run_number < 1:
        raise ValueError(f"run_number must be >= 1, got {run_number}")
    if run_attempt < 1:
        raise ValueError(f"run_attempt must be >= 1, got {run_attempt}")

    if is_tag_ref(git_ref):
        expected = tag_version(git_ref)
        if expected != canonical_version:
            raise ValueError(
                f"tag {git_ref} does not match version.pri {canonical_version}"
            )
        release = canonical_version
        display = release
        artifact = release
        arch_pkgrel = "1"
    else:
        digest = short_sha(sha)
        release = canonical_version
        display = f"{release}+master.g{digest}"
        artifact = f"{release}-master.g{digest}.a{run_attempt}"
        arch_pkgrel = f"1.{run_number}"

    validate_field("release", release, ARTIFACT_SAFE_RE)
    validate_field("display", display, DISPLAY_SAFE_RE)
    validate_field("artifact", artifact, ARTIFACT_SAFE_RE)
    validate_field("arch_pkgrel", arch_pkgrel, PKGREL_RE)

    return {
        "release": release,
        "display": display,
        "artifact": artifact,
        "arch_pkgrel": arch_pkgrel,
    }


def write_github_env(path: Path, values: dict[str, str]) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(f"OMAIRC_VERSION={values['release']}\n")
        handle.write(f"OMAIRC_DISPLAY_VERSION={values['display']}\n")
        handle.write(f"OMAIRC_ARTIFACT_VERSION={values['artifact']}\n")
        handle.write(f"OMAIRC_BUILD_VERSION={values['display']}\n")
        handle.write(f"OMAIRC_ARCH_PKGREL={values['arch_pkgrel']}\n")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--git-ref", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--run-number", type=int, required=True)
    parser.add_argument("--run-attempt", type=int, required=True)
    parser.add_argument(
        "--format",
        choices=("json", "github-env"),
        default="json",
    )
    parser.add_argument("--github-env", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        canonical = read_canonical_version()
        values = derive(
            git_ref=args.git_ref,
            sha=args.sha,
            run_number=args.run_number,
            run_attempt=args.run_attempt,
            canonical_version=canonical,
        )
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 1

    if args.format == "json":
        json.dump(values, sys.stdout, sort_keys=True)
        sys.stdout.write("\n")
    else:
        if args.github_env is None:
            print("--github-env is required with --format github-env", file=sys.stderr)
            return 1
        write_github_env(args.github_env, values)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
