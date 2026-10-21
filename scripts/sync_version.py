"""Sync release versions without resolving or downloading Rust dependencies."""

import argparse
import json
from pathlib import Path
import re
import tomllib


def version_key(version):
    match = re.fullmatch(
        r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
        r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
        r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?",
        version,
    )
    if not match:
        raise ValueError(f"Invalid semantic version: {version}")
    prerelease = match[4]
    identifiers = []
    for part in prerelease.split(".") if prerelease else []:
        if part.isdigit() and len(part) > 1 and part.startswith("0"):
            raise ValueError(f"Invalid prerelease version: {version}")
        identifiers.append((0, int(part)) if part.isdigit() else (1, part))
    return (*map(int, match.group(1, 2, 3)), prerelease is None, identifiers)


def sync_version(root, tag, only_newer=False):
    version = tag.removeprefix("app-v")
    candidate = version_key(version)
    native_manifest = root / "native-app/Cargo.toml"
    current = tomllib.loads(native_manifest.read_text(encoding="utf-8"))["package"]["version"]
    if only_newer and candidate < version_key(current):
        return version

    updates = {}
    for directory in ("native-app", "src-tauri"):
        manifest = root / directory / "Cargo.toml"
        text = manifest.read_text(encoding="utf-8")
        package = tomllib.loads(text)["package"]
        text, count = re.subn(
            r'(\[package\]\s*\n(?:(?!\[)[^\n]*\n)*?version\s*=\s*")[^"]+("[^\n]*)',
            lambda match: match[1] + version + match[2],
            text,
            count=1,
        )
        if count != 1:
            raise ValueError(f"Missing package version in {manifest}")
        updates[manifest] = text

        lockfile = root / directory / "Cargo.lock"
        text, count = re.subn(
            r'(\[\[package\]\]\nname = "' + re.escape(package["name"])
            + r'"\nversion = ")[^"]+("\n)',
            lambda match: match[1] + version + match[2],
            lockfile.read_text(encoding="utf-8"),
        )
        if count != 1:
            raise ValueError(f"Expected one root package in {lockfile}")
        updates[lockfile] = text

    config = root / "src-tauri/tauri.conf.json"
    data = json.loads(config.read_text(encoding="utf-8"))
    if "version" not in data:
        raise ValueError(f"Missing version in {config}")
    data["version"] = version
    updates[config] = json.dumps(data, indent=2) + "\n"

    for path, text in updates.items():
        path.write_text(text, encoding="utf-8", newline="\n")
    return version


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("--only-newer", action="store_true")
    args = parser.parse_args()
    try:
        print(sync_version(Path(__file__).resolve().parents[1], args.tag, args.only_newer))
    except (ValueError, KeyError) as error:
        parser.exit(1, f"{error}\n")
