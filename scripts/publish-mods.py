#!/usr/bin/env python3
"""Build the hosted mod catalog from the authored mod directories.

Every `<build>/<id>/` holding a `manifest.toml` (see `scripts/import-mods.py`)
is built with `arklay mod build` into the hosted tree, which mirrors the
public mods URL (the R2 bucket's prefix):

    <out>/index.json                       the catalog the launcher reads
    <out>/<id>/<version>/<id>.akpak        the mod pack
    <out>/<id>/<version>/thumbnail.webp    its 4:3 preview, when it has one

Published files are immutable: a version's pack is never replaced by
different bytes, so CDN caches can keep them forever. Change a mod's
`version` in its manifest to publish new content (`1.1` becomes `1.1-2` for
a repackage of the same upstream release). `index.json` is the only file
rewritten in place and should be served with a short cache lifetime.

A pack is rebuilt only when its source directory changed since the last
publish (by file paths, sizes and modification times), so a re-run after
editing one mod's manifest only rebuilds that mod.

    scripts/publish-mods.py                       # every mod
    scripts/publish-mods.py re1-shdp re2-enhance
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_BUILD = Path("G:/arklay/mods-build")
DEFAULT_OUT = Path("G:/arklay/mods")
INDEX_FORMAT = 1
# Where the per-mod build stamps live, inside the build tree.
STAMPS = ".publish"


def run(command: list[str]) -> str:
    print("+", " ".join(f'"{part}"' if " " in part else part for part in command), flush=True)
    return subprocess.run(command, check=True, capture_output=True, text=True).stdout


def default_base(game: str) -> Path:
    local = os.environ.get("LOCALAPPDATA")
    root = Path(local) / "arklay" if local else Path.home() / ".local/share/arklay"
    return root / "games" / f"{game}.akpak"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def source_stamp(directory: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            stat = path.stat()
            digest.update(f"{path.relative_to(directory).as_posix()}\0{stat.st_size}\0{stat.st_mtime_ns}\n".encode())
    return digest.hexdigest()


def pack_info(arklay: Path, pack: Path) -> dict:
    """The manifest fields `arklay pack info` prints, so this script reads the
    manifest exactly as the engine parses it."""
    text = run([str(arklay), "pack", "info", str(pack)])
    info: dict = {"credits": []}
    key = None
    for line in text.splitlines():
        if line.startswith("manifest: "):
            match = re.match(r"manifest: (\S+) \((\w+)\)", line)
            if not match:
                sys.exit(f"{pack}: unexpected `pack info` line: {line}")
            info["id"], info["kind"] = match.group(1), match.group(2)
            continue
        if line.startswith(("layers:", "entries:")):
            break
        match = re.match(r"  (\w+): (.*)", line)
        if match:
            key, value = match.groups()
            if key == "credit":
                info["credits"].append(value)
            else:
                info[key] = value
        elif key == "description" and line.startswith("   "):
            info["description"] += "\n" + line.strip()
    return info


def manifest_value(text: str, key: str) -> str | None:
    match = re.search(rf'^\s*{key}\s*=\s*"([^"]*)"', text, re.MULTILINE)
    return match.group(1) if match else None


def publish(directory: Path, args: argparse.Namespace, stamps: dict) -> dict:
    manifest_text = (directory / "manifest.toml").read_text(encoding="utf-8")
    mod_id = manifest_value(manifest_text, "id")
    version = manifest_value(manifest_text, "version")
    game = manifest_value(manifest_text, "base")
    if mod_id != directory.name:
        sys.exit(f"{directory}: manifest id {mod_id!r} does not match the directory name")
    if not version or not game:
        sys.exit(f"{directory}: the manifest needs a `version` and a `base`")
    if not re.fullmatch(r"[A-Za-z0-9._+-]+", version):
        sys.exit(f"{directory}: version {version!r} is not usable in a URL path")

    print(f"=== {mod_id} {version}", flush=True)
    target = args.out / mod_id / version
    pack = target / f"{mod_id}.akpak"
    stamp = source_stamp(directory)
    previous = stamps.get(mod_id, {})
    base = getattr(args, f"{game}_base", None) or default_base(game)

    if pack.exists() and previous.get("stamp") == stamp and previous.get("version") == version:
        print("unchanged")
    else:
        with tempfile.TemporaryDirectory(dir=args.out) as temp:
            built = Path(temp) / pack.name
            command = [str(args.arklay), "mod", "build", str(directory), "--out", str(built)]
            if base.exists():
                command += ["--base", str(base)]
            else:
                print(f"warning: no {game} base pack at {base}; override paths are not checked")
            run(command)
            if pack.exists() and sha256(pack) != sha256(built):
                if not args.force:
                    sys.exit(
                        f"{mod_id}: {version} is already published with different content; "
                        "bump `version` in the manifest (or pass --force before anyone has downloaded it)"
                    )
                print(f"warning: replacing published {mod_id} {version}")
            target.mkdir(parents=True, exist_ok=True)
            shutil.move(built, pack)
        stamps[mod_id] = {"stamp": stamp, "version": version}

    info = pack_info(args.arklay, pack)
    entry = {
        "id": mod_id,
        "name": info.get("name", mod_id),
        "version": version,
        "game": game,
        "kind": info["kind"],
        "category": info.get("category"),
        "description": info.get("description"),
        "authors": [name.strip() for name in info["authors"].split(",")] if "authors" in info else [],
        "credits": info["credits"],
        "homepage": info.get("homepage"),
        "url": f"{mod_id}/{version}/{pack.name}",
        "size": pack.stat().st_size,
        "sha256": sha256(pack),
    }
    thumbnail = info.get("thumbnail")
    if thumbnail:
        source = directory / thumbnail
        if not source.exists():
            sys.exit(f"{mod_id}: thumbnail {thumbnail} is missing")
        hosted = target / f"thumbnail{source.suffix}"
        if not hosted.exists() or hosted.read_bytes() != source.read_bytes():
            shutil.copy2(source, hosted)
        entry["thumbnail"] = f"{mod_id}/{version}/{hosted.name}"
    return {key: value for key, value in entry.items() if value not in (None, [])}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ids", nargs="*", help="mods to rebuild (default: all); the index always lists every mod")
    parser.add_argument("--build", type=Path, default=DEFAULT_BUILD, help=f"mod source directories (default {DEFAULT_BUILD})")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help=f"hosted tree (default {DEFAULT_OUT})")
    parser.add_argument("--arklay", type=Path, default=REPO / "target/test-release" / ("arklay.exe" if os.name == "nt" else "arklay"))
    parser.add_argument("--re1-base", type=Path, help="RE1 base pack checking override paths")
    parser.add_argument("--re2-base", type=Path, help="RE2 base pack checking override paths")
    parser.add_argument("--force", action="store_true", help="replace a published version's pack")
    parser.add_argument("--skip", nargs="+", default=[], metavar="ID", help="leave these mods out of the index")
    args = parser.parse_args()

    directories = sorted(
        path.parent for path in args.build.glob("*/manifest.toml") if path.parent.name not in args.skip
    )
    known = {directory.name for directory in directories}
    unknown = [name for name in args.ids if name not in known]
    if unknown:
        sys.exit(f"no mod directory for: {', '.join(unknown)}")
    args.out.mkdir(parents=True, exist_ok=True)
    stamps_path = args.build / STAMPS / "stamps.json"
    stamps = json.loads(stamps_path.read_text()) if stamps_path.exists() else {}

    index_path = args.out / "index.json"
    previous = {}
    if index_path.exists():
        previous = {mod["id"]: mod for mod in json.loads(index_path.read_text(encoding="utf-8"))["mods"]}

    mods = []
    for directory in directories:
        if args.ids and directory.name not in args.ids:
            # Not asked for: keep its published listing, if it has one.
            if directory.name in previous:
                mods.append(previous[directory.name])
            continue
        mods.append(publish(directory, args, stamps))
        stamps_path.parent.mkdir(exist_ok=True)
        stamps_path.write_text(json.dumps(stamps, indent=2) + "\n")

    index = {
        "format": INDEX_FORMAT,
        "generated": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "mods": mods,
    }
    temp = index_path.with_suffix(".json.tmp")
    temp.write_text(json.dumps(index, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    temp.replace(index_path)
    total = sum(mod["size"] for mod in mods)
    print(f"wrote {index_path}: {len(mods)} mod(s), {total / (1 << 30):.2f} GiB")


if __name__ == "__main__":
    main()
