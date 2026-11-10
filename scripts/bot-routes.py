#!/usr/bin/env python3
"""Play the bot's routes (mods/bot/routes/*.lua) headless and tabulate them.

  scripts/bot-routes.py [options] [ROUTE ...]

  ROUTE            a route name or a part of one (default: every route)
  -p, --pack PATH  the RE1 game pack (default: $BOT_PACK, else
                   target/bot-pack/re1.akpak)
  -s, --seed LIST  bot seeds, as "1", "1,2,5" or "1-4"; 0 is the game's own
                   random stream (default 0)
  -j, --jobs N     runs at once (default: the CPU count). The compute columns
                   are wall-clock: use -j 1 to measure them
  -n, --name NAME  the output directory's name (default: a timestamp)
  -o, --out DIR    the output root (default target/bot-routes)
  -t, --timeout S  per-run timeout in seconds (default 1800)
      --no-build   skip `cargo build --profile test-release`

Each run is `arklay script --headless ROUTE.lua PACK SEED STATS`: its result
line goes to <out>/<name>/<route>-s<seed>.out, the bot's log to .log and the
full report (as --bot-stats writes it) to .json. The table is also written to
<out>/<name>/summary.txt, and the scores to summary.json.

The score of a playthrough is: the ticks it took (30 a second; `time` is the
game's own clear time), the hits (health drops of more than 2), the deaths,
the room loads, and the bot's compute per tick (average, 99th percentile and
worst, in milliseconds, with the ticks over 16 and 33 ms: a 30 Hz tick has
33.3 ms for the whole frame).
"""
import argparse
import concurrent.futures
import datetime
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROUTES = os.path.join(ROOT, "mods", "bot", "routes")
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


def run_one(route, seed, pack, out, timeout):
    """Play one route; the row for the table."""
    stem = os.path.join(out, f"{route}-s{seed}")
    command = [BIN, "script", "--headless", "--timeout", str(timeout),
               os.path.join("mods", "bot", "routes", route + ".lua"), pack,
               # Seed 0 is no seed: the route's tonumber() of "" is nil.
               str(seed) if seed else "", stem + ".json"]
    with open(stem + ".out", "w") as stdout, open(stem + ".log", "w") as stderr:
        code = subprocess.run(command, cwd=ROOT, stdout=stdout, stderr=stderr).returncode
    row = {"route": route, "seed": seed, "exit": code}
    try:
        with open(stem + ".out") as f:
            row.update(json.loads(f.readline()))
    except (OSError, ValueError):
        row["outcome"] = "error"
        with open(stem + ".log", errors="replace") as f:
            lines = f.read().strip().splitlines()
        row["reason"] = lines[-1][:200] if lines else "no output"
    return row


def ms(micros):
    return "-" if micros is None else f"{micros / 1000:.1f}"


def table(rows):
    head = ("route", "seed", "result", "end", "ticks", "time", "hits", "deaths",
            "loads", "avg ms", "p99 ms", "max ms", ">16ms", ">33ms")
    body = []
    for row in rows:
        score = row.get("score") or {}
        compute = score.get("compute") or {}
        over = compute.get("over_ms") or {}
        if row["exit"] == 0:
            result = "ok"
        elif score.get("completed"):
            result = "MISSED"
        else:
            result = (row.get("outcome") or "error").upper()
        body.append((row["route"], row["seed"], result, score.get("ending") or "-",
                     score.get("ticks", "-"), score.get("game_time") or "-",
                     score.get("hits", "-"), score.get("deaths", "-"),
                     score.get("room_loads", "-"), ms(compute.get("avg_us")),
                     ms(compute.get("p99_us")), ms(compute.get("max_us")),
                     over.get("16", "-"), over.get("33", "-")))
    cells = [head] + [tuple(str(cell) for cell in line) for line in body]
    widths = [max(len(line[i]) for line in cells) for i in range(len(head))]
    lines = []
    for line in cells:
        lines.append("  ".join(cell.ljust(widths[i]) if i == 0 else cell.rjust(widths[i])
                               for i, cell in enumerate(line)).rstrip())
    for row in rows:
        for miss in row.get("missed") or []:
            lines.append(f"{row['route']} s{row['seed']}: missed {miss}")
        if row["exit"] != 0 and row.get("reason"):
            lines.append(f"{row['route']} s{row['seed']}: {row['reason'][:300]}")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("routes", nargs="*")
    parser.add_argument("-p", "--pack", default=os.environ.get(
        "BOT_PACK", os.path.join(ROOT, "target", "bot-pack", "re1.akpak")))
    parser.add_argument("-s", "--seed", default="0")
    parser.add_argument("-j", "--jobs", type=int, default=os.cpu_count() or 1)
    parser.add_argument("-n", "--name",
                        default=datetime.datetime.now().strftime("%Y%m%d-%H%M%S"))
    parser.add_argument("-o", "--out", default=os.path.join(ROOT, "target", "bot-routes"))
    parser.add_argument("-t", "--timeout", type=int, default=1800)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("-h", "--help", action="store_true")
    args = parser.parse_args()
    if args.help:
        print(__doc__.strip())
        return 0

    known = sorted(name[:-4] for name in os.listdir(ROUTES) if name.endswith(".lua"))
    picked = [name for name in known
              if not args.routes or any(want in name for want in args.routes)]
    if not picked:
        print(f"no route matches {args.routes}; the routes are: {', '.join(known)}",
              file=sys.stderr)
        return 2
    if not os.path.isfile(args.pack):
        print(f"pack not found: {args.pack} (see -p)", file=sys.stderr)
        return 2
    if not args.no_build:
        build = subprocess.run(["cargo", "build", "--profile", "test-release"], cwd=ROOT,
                               stderr=subprocess.PIPE, text=True)
        for line in build.stderr.splitlines():
            if line.startswith(("error", "warning")) or "Finished" in line:
                print(line.strip())
        if build.returncode != 0:
            return 1
    out = os.path.join(args.out, args.name)
    os.makedirs(out, exist_ok=True)
    jobs = [(route, seed) for route in picked for seed in seeds(args.seed)]
    print(f"{len(jobs)} run(s), {args.jobs} at once -> {out}", file=sys.stderr)
    rows = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [pool.submit(run_one, route, seed, os.path.abspath(args.pack), out,
                               args.timeout) for route, seed in jobs]
        for future in concurrent.futures.as_completed(futures):
            row = future.result()
            rows.append(row)
            print(f"  {row['route']} s{row['seed']}: exit {row['exit']}", file=sys.stderr)
    rows.sort(key=lambda row: (row["route"], row["seed"]))
    text = table(rows)
    print(text)
    with open(os.path.join(out, "summary.txt"), "w") as f:
        f.write(text + "\n")
    with open(os.path.join(out, "summary.json"), "w") as f:
        json.dump(rows, f, indent=1)
    return 0 if all(row["exit"] == 0 for row in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
