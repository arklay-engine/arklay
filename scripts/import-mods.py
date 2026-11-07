#!/usr/bin/env python3
"""Import the hosted mods' content from their original archives.

Each mod of the arklay mod catalog is authored as a source directory,
`<build>/<id>/`, holding `manifest.toml`, an optional `thumbnail.webp` and the
pack entries in arklay's layout. `scripts/publish-mods.py` turns those
directories into the hosted `.akpak` files and `index.json`.

This script (re)fills a mod directory's *content* from the raw archives the
mod was released as, so the directories can be rebuilt from a clean source.
It never touches the hand-authored files (`manifest.toml`, `thumbnail.*`):
on a first import it writes a minimal manifest to start from, and on a
re-import it keeps the existing one.

A recipe either runs `arklay convert-mod` over the archive files it selects
(Classic REbirth's RE1 layout, which convert-mod maps onto pack entries) or
copies files onto entries by pattern (RE2's HD textures and MP4 films, which
need no conversion).

    scripts/import-mods.py                     # every recipe
    scripts/import-mods.py re1-shdp re2-enhance-movies
    scripts/import-mods.py --list

Archives are read with 7-Zip (`7z`), which handles .zip, .7z and .rar.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_RAW = Path("G:/arklay/raw")
DEFAULT_BUILD = Path("G:/arklay/mods-build")
DEFAULT_WORK = Path("G:/arklay/work")

# Files of a mod directory that are authored by hand and survive a re-import.
AUTHORED = ("manifest.toml", "thumbnail.webp", "thumbnail.png")


@dataclass
class Source:
    """Files taken from one archive: 7-Zip path patterns, archive-relative."""

    archive: str
    include: list[str]
    exclude: list[str] = field(default_factory=list)


@dataclass
class Copy:
    """Copy archive files whose relative path matches `pattern` (a
    case-insensitive regex over `/`-separated paths) to `entry`, a template
    over the match's groups."""

    pattern: str
    entry: str


@dataclass
class Recipe:
    id: str
    name: str
    version: str
    base: str
    load_order: int
    sources: list[Source]
    # Without copy rules the selected files are run through convert-mod.
    copies: list[Copy] = field(default_factory=list)


RECIPES = [
    Recipe(
        id="re1-shdp",
        name="Seamless HD Project",
        version="1.1",
        base="re1",
        load_order=10,
        sources=[
            # `bio1dc/` holds the Director's Cut camera angles' backgrounds;
            # ten of its names clash with the original angles' ones, so it is
            # left out, as the SHDP readme does for the original angles.
            Source("RE_SHDP_1.1.zip", ["hires\\*", "Mod_SHDP\\*"]),
            Source("RE_SHDP_-_Patch_1.1.zip", ["hires\\*", "JPN\\*"]),
        ],
    ),
    Recipe(
        id="re1-shdp-movies",
        name="Seamless HD Project Movies",
        version="1.1",
        base="re1",
        load_order=20,
        sources=[
            # The `Capcom clips` and `DM7 clips` folders hold alternative
            # takes; the films at the top of `movie/` are the pack's choice.
            Source("RE_SHDP_1.1.zip", ["movie\\*.mp4"], ["movie\\*\\*"]),
            Source("RE_SHDP_-_Patch_1.1.zip", ["movie\\*.mp4"]),
        ],
    ),
    Recipe(
        id="re1-enhance",
        name="RE-ENHANCE",
        version="2.0",
        base="re1",
        load_order=10,
        sources=[Source("RE-ENHANCE_RE1_v2.0_GOG.zip", ["hires\\*", "jpn\\PLAYERS\\*"])],
    ),
    Recipe(
        id="re1-enhance-movies",
        name="RE-ENHANCE Movies",
        version="2.0",
        base="re1",
        load_order=20,
        # `jpn/movie/*.avi` lands on the same films, where the MP4s win.
        sources=[Source("RE-ENHANCE_RE1_v2.0_GOG.zip", ["movie\\*.mp4"])],
    ),
    Recipe(
        id="re1-gemini-movies",
        name="Gemini Movie Pack",
        version="2018",
        base="re1",
        load_order=20,
        sources=[
            # The pack carries both the Japanese and the Western takes of the
            # opening, the prologue, two endings and the staff rolls, which
            # land on the same films; the Western ones match the English
            # base.
            Source(
                "RE_gemini_movie_pack.7z",
                ["movie\\*.mp4"],
                [f"movie\\{name}.mp4" for name in ("oj", "pj", "ed4", "ed5", "staf_b", "stfc_b", "stfj_b", "stfz_b")],
            )
        ],
    ),
    Recipe(
        id="re1-hq-audio",
        name="High Quality Audio",
        version="1.0",
        base="re1",
        load_order=30,
        sources=[
            Source(
                "RE1 PC - HQ Audio Mod (ORIGINAL) (simple install).zip",
                ["JPN\\sound\\*", "JPN\\voice\\*"],
            )
        ],
    ),
    Recipe(
        id="re2-shdp",
        name="Seamless HD Project",
        version="2.0",
        base="re2",
        load_order=10,
        sources=[
            # SHDP 2.0 is an update over TeamX's HD mod, which supplies the
            # textures SHDP leaves alone (most character skins and menus);
            # the BioRand hotfix fixes the flamethrower and Spark Shot ammo
            # icons on top.
            Source("Resident_Evil_2_HD_mod_v20220716.zip", ["hires\\*"]),
            Source("RE2_SHDP_2.0_update_for_TeamX_HD_patch.2.zip", ["hires\\*"]),
            Source("RE2_Sourcenext_hotfix_2_biorand.zip", ["hires\\*"]),
        ],
        copies=[Copy(r"hires/(.+)", "hires/{0}")],
    ),
    Recipe(
        id="re2-enhance",
        name="RE-ENHANCE",
        version="3.0",
        base="re2",
        load_order=10,
        sources=[Source("RE-ENHANCE_RE2_v3.0_GOG.zip", ["hires\\*"])],
        copies=[Copy(r"hires/(.+)", "hires/{0}")],
    ),
    Recipe(
        id="re2-enhance-movies",
        name="RE-ENHANCE Movies",
        version="3.0",
        base="re2",
        load_order=20,
        sources=[
            Source(
                "RE-ENHANCE_RE2_v3.0_GOG.zip",
                ["zmovie\\*.bin", "Pl0\\Zmovie\\*.bin", "Pl1\\Zmovie\\*.bin"],
            )
        ],
        # The films are MP4s named `.bin`; the `_BIOHAZARD` takes carry the
        # Japanese title and are not used.
        copies=[Copy(r"(?:pl[01]/)?zmovie/(?!.*_biohazard)([^/]+)\.bin", "movie/{0}.mp4")],
    ),
]


def remove_tree(path: Path) -> None:
    """Remove a directory tree, clearing read-only attributes (7-Zip keeps
    an archive's) and retrying once a moment later, as Windows briefly
    holds a just-written folder open (an indexer or antivirus scan)."""

    def retry(function, name, _error):
        os.chmod(name, stat.S_IWRITE)
        try:
            function(name)
        except OSError:
            time.sleep(1)
            function(name)

    shutil.rmtree(path, onexc=retry)


def run(command: list[str], **kwargs) -> None:
    print("+", " ".join(f'"{part}"' if " " in part else part for part in command), flush=True)
    subprocess.run(command, check=True, **kwargs)


def seven_zip() -> str:
    found = shutil.which("7z")
    if found:
        return found
    candidate = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "7-Zip/7z.exe"
    if candidate.exists():
        return str(candidate)
    sys.exit("7z not found; install 7-Zip")


def default_base(game: str) -> Path:
    local = os.environ.get("LOCALAPPDATA")
    root = Path(local) / "arklay" if local else Path.home() / ".local/share/arklay"
    return root / "games" / f"{game}.akpak"


def stage(recipe: Recipe, raw: Path, work: Path) -> Path:
    """Extract the recipe's files into a fresh staging directory, later
    sources overwriting earlier ones (a patch over its release)."""
    staging = work / recipe.id
    if staging.exists():
        remove_tree(staging)
    staging.mkdir(parents=True)
    for source in recipe.sources:
        archive = raw / source.archive
        if not archive.exists():
            sys.exit(f"{recipe.id}: missing archive {archive}")
        command = [seven_zip(), "x", "-y", "-bso0", "-bsp0", f"-o{staging}", str(archive)]
        command += source.include
        command += [f"-x!{pattern}" for pattern in source.exclude]
        run(command)
    return staging


def copy_entries(recipe: Recipe, staging: Path, out: Path) -> int:
    rules = [(re.compile(rule.pattern + r"\Z", re.IGNORECASE), rule.entry) for rule in recipe.copies]
    count = 0
    for path in sorted(staging.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(staging).as_posix()
        for pattern, template in rules:
            match = pattern.match(relative)
            if match:
                entry = template.format(*match.groups()).lower()
                target = out / entry
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(path, target)
                count += 1
                break
    return count


def minimal_manifest(recipe: Recipe) -> str:
    return (
        "[pack]\n"
        "format = 1\n"
        f'id = "{recipe.id}"\n'
        f'name = "{recipe.name}"\n'
        f'version = "{recipe.version}"\n'
        'kind = "mod"\n'
        f'base = "{recipe.base}"\n'
        f"load_order = {recipe.load_order}\n"
    )


def import_recipe(recipe: Recipe, args: argparse.Namespace) -> None:
    print(f"=== {recipe.id}", flush=True)
    target = args.build / recipe.id
    fresh = args.build / f"{recipe.id}.importing"
    if fresh.exists():
        remove_tree(fresh)
    staging = stage(recipe, args.raw, args.work)
    base = getattr(args, f"{recipe.base}_base") or default_base(recipe.base)

    if recipe.copies:
        fresh.mkdir(parents=True)
        count = copy_entries(recipe, staging, fresh)
        print(f"copied {count} file(s)")
        if count == 0:
            sys.exit(f"{recipe.id}: no file matched the copy rules")
    else:
        with tempfile.TemporaryDirectory(dir=args.work) as temp:
            pack = Path(temp) / f"{recipe.id}.akpak"
            run(
                [
                    str(args.arklay), "convert-mod", str(staging),
                    "--base", str(base),
                    "--id", recipe.id, "--name", recipe.name, "--version", recipe.version,
                    "--out", str(pack), "--force",
                ]
            )
            run([str(args.arklay), "extract", str(pack), "--out", str(fresh)])
        # convert-mod's manifest only seeds a first import.
        (fresh / "manifest.toml").unlink(missing_ok=True)
    remove_tree(staging)

    # Keep the authored files; seed a manifest on the first import.
    for name in AUTHORED:
        if (target / name).exists():
            shutil.copy2(target / name, fresh / name)
    if not (fresh / "manifest.toml").exists():
        (fresh / "manifest.toml").write_text(minimal_manifest(recipe), encoding="utf-8")

    if target.exists():
        old = args.build / f"{recipe.id}.old"
        if old.exists():
            remove_tree(old)
        target.rename(old)
        fresh.rename(target)
        remove_tree(old)
    else:
        fresh.rename(target)
    print(f"imported {target}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ids", nargs="*", help="recipes to import (default: all)")
    parser.add_argument("--raw", type=Path, default=DEFAULT_RAW, help=f"original archives (default {DEFAULT_RAW})")
    parser.add_argument("--build", type=Path, default=DEFAULT_BUILD, help=f"mod source directories (default {DEFAULT_BUILD})")
    parser.add_argument("--work", type=Path, default=DEFAULT_WORK, help=f"scratch space (default {DEFAULT_WORK})")
    parser.add_argument("--arklay", type=Path, default=REPO / "target/test-release" / ("arklay.exe" if os.name == "nt" else "arklay"))
    parser.add_argument("--re1-base", type=Path, help="RE1 base pack (default: the launcher's games/re1.akpak)")
    parser.add_argument("--re2-base", type=Path, help="RE2 base pack (default: the launcher's games/re2.akpak)")
    parser.add_argument("--list", action="store_true", help="list the recipes and exit")
    args = parser.parse_args()

    if args.list:
        for recipe in RECIPES:
            print(f"{recipe.id:22} {recipe.base}  {recipe.name} {recipe.version}")
        return
    by_id = {recipe.id: recipe for recipe in RECIPES}
    unknown = [name for name in args.ids if name not in by_id]
    if unknown:
        sys.exit(f"unknown recipe(s): {', '.join(unknown)}; see --list")
    args.build.mkdir(parents=True, exist_ok=True)
    args.work.mkdir(parents=True, exist_ok=True)
    for recipe in [by_id[name] for name in args.ids] or RECIPES:
        import_recipe(recipe, args)


if __name__ == "__main__":
    main()
