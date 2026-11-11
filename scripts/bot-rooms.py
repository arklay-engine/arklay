#!/usr/bin/env python3
"""Record a bot game as scenes, one per room entry, and replay rooms from them.

A full game takes a minute; a room takes a second. To work on how the bot
handles a room (its monsters, most of all), record a game once and then
replay only that room, over many seeds, after each change.

  scripts/bot-rooms.py record [-s SEEDS] [-n NAME] [NAME=VALUE ...]
  scripts/bot-rooms.py list   [NAME ...]
  scripts/bot-rooms.py play   [NAME ...] [-r ROOM ...] [-e SCENE ...] [-s SEEDS]
                              [-m TICKS] [--from ROOM] [--to ROOM] [-k KIND ...]
                              [--steer] [--scenes] [--log | --trace]

record   Play a game per seed (default 1) and write a scene at every room
         entry to target/bot-rooms/NAME-s<seed>/: the save, the bot's memory
         and index.json. NAME defaults to `game`; NAME=VALUE are the bot's
         story parameters.
list     The rooms of the named recordings (default: all), with their
         entries and the ticks, hits and deaths the recorded game had there.
play     Replay the scenes of the rooms (-r, repeatable; `all` is every
         room) or the scenes named (-e, as `0303-207`), each over the seeds
         (default 1-4): the bot starts at the room's door with what it knew
         and had planned then, and the replay ends when it leaves the room,
         dies, or after -m ticks (default 3000). --from and --to keep only
         the entries from or to that room; -k KIND (repeatable, as `06`)
         only the scenes whose room held an enemy of that kind in an earlier
         replay (`play -r all` once to learn them). --steer has the bot walk to the
         room the recorded game went to next, whatever it would choose.
         --scenes prints a row per scene whatever the rooms. --log keeps
         each replay's bot log beside its result; --trace adds a line a
         tick to it, of the player and every monster.

play prints one row per room (or per scene with one room) and the totals:
replays, how many left the room (`out`) and by the door the recorded game
took (`same`), and over those that left the mean ticks; hits, damage, deaths,
shots and the shots that landed (a monster's health drops) over all of them. Results go to target/bot-rooms/play-<timestamp>/.

Common options:
  -p, --pack PATH  the RE1 game pack (default $BOT_PACK, else
                   target/bot-pack/re1.akpak)
  -j, --jobs N     runs at once (default: the CPU count)
      --no-build   skip `cargo build --profile test-release`
"""
import argparse
import concurrent.futures
import datetime
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "target", "bot-rooms")
BIN = os.path.join(ROOT, "target", "test-release", "arklay")


def seeds(text):
    out = []
    for part in text.split(","):
        part = part.strip()
        if "-" in part:
            low, high = part.split("-", 1)
            out += range(int(low), int(high) + 1)
        elif part:
            out.append(int(part))
    return out


def build(args):
    if args.no_build:
        return True
    done = subprocess.run(["cargo", "build", "--profile", "test-release"], cwd=ROOT,
                          stderr=subprocess.PIPE, text=True)
    for line in done.stderr.splitlines():
        if line.startswith(("error", "warning")) or "Finished" in line:
            print(line.strip(), file=sys.stderr)
    return done.returncode == 0


def script(name, *args, stderr=subprocess.DEVNULL):
    """Run a driver script headless; its first stdout line as JSON, or None."""
    done = subprocess.run([BIN, "script", "--headless", os.path.join("scripts", name),
                           *[str(arg) for arg in args]],
                          cwd=ROOT, stdout=subprocess.PIPE, stderr=stderr, text=True)
    try:
        return json.loads(done.stdout.splitlines()[0])
    except (IndexError, ValueError):
        return None


def recordings(names):
    """The recording directories named (all of them when none is)."""
    found = []
    for entry in sorted(os.listdir(OUT)) if os.path.isdir(OUT) else []:
        path = os.path.join(OUT, entry)
        if not os.path.isfile(os.path.join(path, "index.json")):
            continue
        if not names or any(entry == name or entry.startswith(name + "-s") for name in names):
            found.append(path)
    return found


def scenes(path):
    """A recording's scenes, each with the room the game went to next and
    what the stay cost it."""
    with open(os.path.join(path, "index.json")) as f:
        rows = json.load(f)["scenes"]
    for row, after in zip(rows, rows[1:] + [None]):
        row["dir"] = path
        row["to"] = after and after["room"]
        row["stay"] = after["tick"] - row["tick"] if after else 0
        # A fall in health over the stay; a rise is a heal, a reload or both.
        row["lost"] = max(0, row["health"] - after["health"]) if after else 0
    return rows


def kinds(row):
    """The enemy kinds a scene's room held, from the recording's enemies.json
    (which `play -r all` writes)."""
    path = os.path.join(row["dir"], "enemies.json")
    if path not in KINDS:
        try:
            with open(path) as f:
                KINDS[path] = json.load(f)
        except (OSError, ValueError):
            KINDS[path] = {}
    return KINDS[path].get(row["scene"], [])


KINDS = {}


def record(args):
    if not build(args):
        return 1
    jobs = []
    for seed in seeds(args.seed):
        out = os.path.join(OUT, f"{args.name}-s{seed}")
        os.makedirs(out, exist_ok=True)
        for old in os.listdir(out):
            os.remove(os.path.join(out, old))
        jobs.append((seed, out))

    def one(job):
        seed, out = job
        index = script("bot-room-record.lua", args.pack, out, seed or "", *args.params)
        if index is None:
            return f"seed {seed}: the recording failed"
        with open(os.path.join(out, "index.json"), "w") as f:
            json.dump(index, f)
        return (f"seed {seed}: {len(index['scenes'])} scenes, {index['ticks']} ticks, "
                f"{(index.get('reason') or 'not finished')[:60]} -> {os.path.relpath(out, ROOT)}")

    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        for line in pool.map(one, jobs):
            print(line)
    return 0


def list_rooms(args):
    rooms = {}
    for path in recordings(args.names):
        for row in scenes(path):
            room = rooms.setdefault(row["room"], {"entries": 0, "ticks": 0, "lost": 0})
            room["entries"] += 1
            room["ticks"] += row["stay"]
            room["lost"] += row["lost"]
    print(f"{'room':5} {'entries':>7} {'ticks':>8} {'health lost':>11}")
    for name, room in sorted(rooms.items(), key=lambda item: -item[1]["lost"]):
        print(f"{name:5} {room['entries']:>7} {room['ticks']:>8} {room['lost']:>11}")
    return 0


def play(args):
    if not build(args):
        return 1
    picked = []
    for path in recordings(args.names):
        for row in scenes(path):
            # A game's last scene has no room after it: replayed only when
            # named (-e), which is how the room a run got stuck in is studied.
            named = args.scene and row["scene"] in args.scene
            if (not row["to"] or row["to"] == row["room"]) and not named:
                continue
            if args.room and "all" not in args.room and row["room"] not in args.room:
                continue
            if args.scene and row["scene"] not in args.scene:
                continue
            if args.came_from and row.get("from") != args.came_from:
                continue
            if args.to and row["to"] != args.to:
                continue
            picked.append(row)
    if args.enemy:
        # The kinds each scene's room held are in the last play's results.
        picked = [row for row in picked if set(args.enemy) & set(kinds(row))]
    if not (args.room or args.scene):
        print("name the rooms (-r) or the scenes (-e) to replay", file=sys.stderr)
        return 2
    if not picked:
        print("no recorded scene matches", file=sys.stderr)
        return 2
    out = os.path.join(OUT, "play-" + datetime.datetime.now().strftime("%Y%m%d-%H%M%S"))
    os.makedirs(out, exist_ok=True)
    jobs = [(row, seed) for row in picked for seed in seeds(args.seed)]

    def one(job):
        row, seed = job
        stem = os.path.join(row["dir"], row["scene"])
        tag = f"{os.path.basename(row['dir'])}-{row['scene']}-s{seed}"
        steer = row["to"] if args.steer else ""
        if args.log or args.trace:
            with open(os.path.join(out, tag + ".log"), "w") as log:
                result = script("bot-room-play.lua", args.pack, stem, seed, args.max,
                                steer, "trace" if args.trace else "log", stderr=log)
        else:
            result = script("bot-room-play.lua", args.pack, stem, seed, args.max, steer)
        result = result or {"ticks": 0, "hits": 0, "damage": 0, "died": False, "error": True}
        result.update(recording=os.path.basename(row["dir"]), scene=row["scene"],
                      room=row["room"], came_from=row.get("from"), to=row["to"], seed=seed,
                      recorded_ticks=row["stay"], recorded_lost=row["lost"])
        return result

    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        results = list(pool.map(one, jobs))
    with open(os.path.join(out, "results.json"), "w") as f:
        json.dump(results, f, indent=1)
    # Remember the enemy kinds each scene's room held, for -k.
    seen = {}
    for row, result in zip((row for row, _ in jobs), results):
        seen.setdefault(row["dir"], {}).setdefault(row["scene"], set()).update(
            result.get("enemies") or {})
    for path, found in seen.items():
        name = os.path.join(path, "enemies.json")
        KINDS.pop(name, None)
        known = {}
        if os.path.isfile(name):
            with open(name) as f:
                known = json.load(f)
        known.update({scene: sorted(set(known.get(scene, [])) | ids)
                      for scene, ids in found.items()})
        with open(name, "w") as f:
            json.dump(known, f)

    by_room = len({row["room"] for row in picked}) > 1 and not args.scenes
    groups = {}
    for result in results:
        key = result["room"] if by_room else (
            f"{result['recording']} {result['scene']} {result['came_from']}->{result['to']}")
        groups.setdefault(key, []).append(result)

    def line(name, rows):
        left = [row for row in rows if row.get("exit")]
        same = sum(1 for row in left if row["exit"] == row["to"])
        ticks = sum(row["ticks"] for row in left) / len(left) if left else 0
        return (f"{name:42} {len(rows):>4} {len(left):>4} {same:>4} {ticks:>7.0f} "
                f"{sum(row['hits'] for row in rows):>5} {sum(row['damage'] for row in rows):>6} "
                f"{sum(1 for row in rows if row['died']):>6} "
                f"{sum(row.get('shots', 0) for row in rows):>5} "
                f"{sum(row.get('landed', 0) for row in rows):>6}")

    lines = [f"{'room' if by_room else 'scene':42} {'runs':>4} {'out':>4} {'same':>4} {'ticks':>7} "
             f"{'hits':>5} {'damage':>6} {'deaths':>6} {'shots':>5} {'landed':>6}"]
    lines += [line(name, rows) for name, rows in sorted(groups.items())]
    lines.append(line("total", results))
    by_kind = {}
    for result in results:
        for kind, count in (result.get("enemy_hits") or {}).items():
            by_kind[kind] = by_kind.get(kind, 0) + count
    if by_kind:
        lines.append("hits by the nearest enemy's kind: " + ", ".join(
            f"{kind} {count}" for kind, count in sorted(by_kind.items(), key=lambda item: -item[1])))
    text = "\n".join(lines)
    print(text)
    with open(os.path.join(out, "summary.txt"), "w") as f:
        f.write(text + "\n")
    print(f"-> {os.path.relpath(out, ROOT)}", file=sys.stderr)
    return 0


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("command", nargs="?")
    parser.add_argument("names", nargs="*")
    parser.add_argument("-p", "--pack", default=os.environ.get(
        "BOT_PACK", os.path.join(ROOT, "target", "bot-pack", "re1.akpak")))
    parser.add_argument("-s", "--seed")
    parser.add_argument("-j", "--jobs", type=int, default=os.cpu_count() or 1)
    parser.add_argument("-n", "--name", default="game")
    parser.add_argument("-r", "--room", action="append")
    parser.add_argument("-e", "--scene", action="append")
    parser.add_argument("-m", "--max", type=int, default=3000)
    parser.add_argument("--from", dest="came_from")
    parser.add_argument("--to")
    parser.add_argument("-k", "--enemy", action="append")
    parser.add_argument("--scenes", action="store_true")
    parser.add_argument("--steer", action="store_true")
    parser.add_argument("--log", action="store_true")
    parser.add_argument("--trace", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("-h", "--help", action="store_true")
    args = parser.parse_args()
    if args.help or args.command not in ("record", "list", "play"):
        print(__doc__.strip())
        return 0 if args.help else 2
    args.pack = os.path.abspath(args.pack)
    if not os.path.isfile(args.pack):
        print(f"pack not found: {args.pack} (see -p)", file=sys.stderr)
        return 2
    if args.command == "record":
        args.params = [name for name in args.names if "=" in name]
        args.seed = args.seed or "1"
        return record(args)
    if args.command == "list":
        return list_rooms(args)
    args.seed = args.seed or "1-4"
    return play(args)


if __name__ == "__main__":
    sys.exit(main())
