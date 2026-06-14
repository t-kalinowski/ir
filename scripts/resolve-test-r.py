#!/usr/bin/env python3
"""Resolve the concrete rig install used by version-selection tests."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path, PureWindowsPath
from typing import Any


def die(message: str) -> None:
    raise SystemExit(message)


def run_rig(args: list[str]) -> str:
    result = subprocess.run(
        ["rig", *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        die(f"`rig {' '.join(args)}` exited with code {result.returncode}")
    return result.stdout


def clean_rig_output(text: str) -> str:
    return "\n".join(line for line in text.splitlines() if not line.startswith("[INFO]"))


def version_parts(value: str) -> tuple[int, int, int] | None:
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        return None
    return tuple(int(part) for part in value.split("."))


def oldrel_offset(spec: str) -> int:
    if spec == "oldrel":
        return 1
    if spec.startswith("oldrel/"):
        value = spec.split("/", 1)[1]
        if value and value.isdigit() and int(value) > 0:
            return int(value)
    die(f"unsupported test R spec: {spec}")


def installed_r() -> list[dict[str, Any]]:
    return json.loads(clean_rig_output(run_rig(["list", "--json"])))


def release_install(installed: list[dict[str, Any]]) -> dict[str, Any]:
    release = next(
        (
            install
            for install in installed
            if install.get("name") == "release"
            or "release" in install.get("aliases", [])
        ),
        None,
    )
    if release is None:
        die("rig does not report an installed release R")
    return release


def resolve_install(spec: str) -> tuple[str, str]:
    offset = oldrel_offset(spec)
    installed = installed_r()
    release = release_install(installed)

    release_parts = version_parts(release.get("version", ""))
    if release_parts is None or release_parts[1] < offset:
        die(f"cannot resolve {spec} relative to installed release R {release.get('version')}")

    target = (release_parts[0], release_parts[1] - offset)
    matches = [
        (parts, install)
        for install in installed
        for parts in [version_parts(install.get("version", ""))]
        if parts is not None and parts[:2] == target
    ]
    if not matches:
        die(f"R {target[0]}.{target[1]} from {spec} is not installed by rig")

    _, install = max(matches, key=lambda item: item[0])
    return install["name"], install["version"]


def rscript_from_r_binary(binary: str) -> str:
    if "\\" in binary or re.match(r"^[A-Za-z]:", binary):
        path = PureWindowsPath(binary)
        suffix = path.suffix if path.suffix.lower() == ".exe" else ""
        return str(path.with_name(f"Rscript{suffix}"))

    path = Path(binary)
    suffix = path.suffix if path.suffix.lower() == ".exe" else ""
    return str(path.with_name(f"Rscript{suffix}"))


def release_rscript() -> str:
    release = release_install(installed_r())
    binary = release.get("binary")
    if not isinstance(binary, str) or not binary:
        die("rig release R does not report a binary path")
    return rscript_from_r_binary(binary)


def release_date(name: str) -> str:
    output = run_rig(
        [
            "run",
            "-r",
            name,
            "-e",
            'cat(sprintf("%s-%s-%s\\n", R.version$year, R.version$month, R.version$day))',
        ]
    )
    match = re.search(r"\d{4}-\d{2}-\d{2}", output)
    if not match:
        die(f"could not read R release date for {name}")
    return match.group(0)


def main() -> None:
    if len(sys.argv) != 2:
        die("usage: scripts/resolve-test-r.py oldrel/N | --release-rscript")

    if sys.argv[1] == "--release-rscript":
        print(release_rscript())
        return

    name, version = resolve_install(sys.argv[1])
    print(name, version, release_date(name))


if __name__ == "__main__":
    main()
