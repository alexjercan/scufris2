#!/usr/bin/env python3
"""Check Scufris product and surface protocol version consistency."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

import tomllib

PRODUCT_PATTERNS = {
    "iOS marketing": (
        Path("surfaces/ios/project.yml"),
        re.compile(
            r"^\s*MARKETING_VERSION:\s*([0-9]+\.[0-9]+\.[0-9]+)\s*$", re.MULTILINE
        ),
    ),
}
PROTOCOL_PATTERNS = {
    "Rust control": (
        Path("shared/control/src/service.rs"),
        re.compile(r"^pub const SERVICE_VERSION: u32 = ([0-9]+);$", re.MULTILINE),
    ),
    "TypeScript agent": (
        Path("agent/extensions/scufris/service/protocol.ts"),
        re.compile(r"^export const SERVICE_VERSION = ([0-9]+);$", re.MULTILINE),
    ),
    "Swift surface": (
        Path("surfaces/ios/Sources/Protocol.swift"),
        re.compile(r"^let scufrisProtocolVersion = ([0-9]+)$", re.MULTILINE),
    ),
}


def _match(root: Path, name: str, path: Path, pattern: re.Pattern[str]) -> str:
    matches = pattern.findall((root / path).read_text(encoding="utf-8"))
    if len(matches) != 1:
        raise ValueError(
            f"{name}: expected one version in {path}, found {len(matches)}"
        )
    return matches[0]


def collect_versions(root: Path) -> tuple[dict[str, str], dict[str, str]]:
    package = json.loads((root / "package.json").read_text(encoding="utf-8"))
    package_lock = json.loads((root / "package-lock.json").read_text(encoding="utf-8"))
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    product = {
        "npm package": package["version"],
        "npm lock": package_lock["version"],
        "npm root lock": package_lock["packages"][""]["version"],
        "Cargo workspace": cargo["workspace"]["package"]["version"],
    }
    product.update(
        {
            name: _match(root, name, path, pattern)
            for name, (path, pattern) in PRODUCT_PATTERNS.items()
        }
    )
    protocol = {
        name: _match(root, name, path, pattern)
        for name, (path, pattern) in PROTOCOL_PATTERNS.items()
    }
    return product, protocol


def check_versions(root: Path, expected_tag: str | None = None) -> list[str]:
    product, protocol = collect_versions(root)
    errors: list[str] = []
    product_version = next(iter(product.values()))
    for name, version in product.items():
        if version != product_version:
            errors.append(f"{name} is {version}, expected {product_version}")
    protocol_version = next(iter(protocol.values()))
    for name, version in protocol.items():
        if version != protocol_version:
            errors.append(f"{name} protocol is {version}, expected {protocol_version}")
    if expected_tag is not None and expected_tag != f"v{product_version}":
        errors.append(f"tag is {expected_tag}, expected v{product_version}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[2]
    )
    parser.add_argument("--tag")
    args = parser.parse_args()
    try:
        errors = check_versions(args.root, args.tag)
    except (
        KeyError,
        OSError,
        ValueError,
        json.JSONDecodeError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(f"version check failed: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(f"version check failed: {error}", file=sys.stderr)
        return 1
    product, protocol = collect_versions(args.root)
    print(
        f"product {next(iter(product.values()))}; "
        f"surface protocol {next(iter(protocol.values()))}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
