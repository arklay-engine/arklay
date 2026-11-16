#!/usr/bin/env python3
"""Summarise or compare bot-batch.sh output directories.

  bot-batch-summary.py summary <batch-dir>
  bot-batch-summary.py compare <batch-dir-A> <batch-dir-B>
  bot-batch-summary.py row <batch-dir> <commit> <change> <verdict>

Per run: completed (a `FINISH game completed...` line), deaths (lines with
`died:`), hits (`damage health A -> B` with A-B > 2), ticks. Hits are also
split by nearest enemy id and room.
"""
import glob, json, os, re, statistics, sys
from collections import Counter

DMG = re.compile(
    r"^\[bot\] tick (\d+) room (\w+): damage health (\d+) -> (\d+).*?"
    r"nearest=(?:slot=\d+ id=(\w+)|(none))")
FINISH = re.compile(r"FINISH (.*)")
OUTCOME = re.compile(r"\[bot\] outcome=(\S+) ticks=(\d+)")


def parse_log(path):
    run = dict(completed=False, finish="", outcome="", ticks=0, deaths=0, hits=0,
               by_enemy=Counter(), by_room=Counter(), by_source=Counter())
    last_tick = 0
    with open(path, errors="replace") as f:
        for line in f:
            if "died:" in line:
                run["deaths"] += 1
            m = DMG.match(line)
            if m:
                last_tick = int(m[1])
                if int(m[3]) - int(m[4]) > 2:
                    enemy = m[5] or "none"
                    run["hits"] += 1
                    run["by_enemy"][enemy] += 1
                    run["by_room"][m[2]] += 1
                    run["by_source"][f"{enemy}@{m[2]}"] += 1
                continue
            if "FINISH " in line:
                m = FINISH.search(line)
                run["finish"] = m[1][:80]
                if "game completed" in m[1]:
                    run["completed"] = True
                continue
            m = OUTCOME.search(line)
            if m:
                run["outcome"], run["ticks"] = m[1], int(m[2])
    if not run["ticks"]:
        run["ticks"] = last_tick
    return run


def load(batch):
    runs = {}
    for log in sorted(glob.glob(os.path.join(batch, "seed-*.log")),
                      key=lambda p: int(re.search(r"seed-(\d+)", p)[1])):
        seed = int(re.search(r"seed-(\d+)\.log", log)[1])
        run = parse_log(log)
        ex = log[:-4] + ".exit"
        run["exit"] = open(ex).read().strip() if os.path.exists(ex) else "?"
        runs[seed] = run
    return runs


def pct(values, q):
    """The q-th percentile (nearest rank) of values, or None."""
    if not values:
        return None
    ordered = sorted(values)
    rank = max(1, -(-q * len(ordered) // 100))
    return ordered[int(rank) - 1]


def stalled(run):
    """A run the bot ended on its own without the ending, or that timed out:
    a loop, a stuck room, an empty frontier, the tick cap or an error."""
    return not run["completed"] and "dead" not in run["finish"]


def aggregate(runs):
    done = [r for r in runs.values() if r["completed"]]
    hits = [r["hits"] for r in done]
    every = [r["hits"] for r in runs.values()]
    default = runs.get(0)
    agg = dict(
        median_hits_all=statistics.median(every) if every else None,
        p90_hits_all=pct(every, 90),
        median_ticks=statistics.median([r["ticks"] for r in done]) if done else None,
        stalls=sum(1 for r in runs.values() if stalled(r)),
        default_completed=None if default is None else default["completed"],
        runs=len(runs), completed=len(done),
        completion_rate=round(len(done) / len(runs), 3) if runs else 0,
        median_hits=statistics.median(hits) if hits else None,
        mean_hits=round(statistics.mean(hits), 2) if hits else None,
        deaths=sum(r["deaths"] for r in runs.values()),
        hits_all=sum(r["hits"] for r in runs.values()),
        by_enemy=sum((r["by_enemy"] for r in runs.values()), Counter()),
        by_room=sum((r["by_room"] for r in runs.values()), Counter()),
        by_source=sum((r["by_source"] for r in runs.values()), Counter()),
    )
    return agg


def fmt(x):
    return "-" if x is None else x


def summary(batch):
    runs = load(batch)
    if not runs:
        sys.exit(f"no seed-*.log in {batch}")
    print(f"{'seed':>5} {'done':>5} {'deaths':>6} {'hits':>5} {'ticks':>7}  finish")
    for seed, r in runs.items():
        tag = "yes" if r["completed"] else "NO"
        extra = r["finish"] or f"{r['outcome'] or 'no outcome'} ({r['exit']})"
        print(f"{seed:>5} {tag:>5} {r['deaths']:>6} {r['hits']:>5} {r['ticks']:>7}  {extra}")
    a = aggregate(runs)
    print(f"\ncompleted {a['completed']}/{a['runs']} ({a['completion_rate']:.0%}); "
          f"hits over completed runs: median {fmt(a['median_hits'])} "
          f"mean {fmt(a['mean_hits'])}; deaths total {a['deaths']}; "
          f"hits all runs {a['hits_all']}")
    for title, key, n in (("hits by enemy id", "by_enemy", 12),
                          ("hits by room", "by_room", 12),
                          ("hits by enemy@room", "by_source", 20)):
        print(f"\n{title} (all runs):")
        for k, v in a[key].most_common(n):
            print(f"  {k:<14} {v}")
    print("\nBATCH " + json.dumps(dict(
        dir=os.path.basename(os.path.normpath(batch)), runs=a["runs"],
        completed=a["completed"], completion_rate=a["completion_rate"],
        median_hits=a["median_hits"], mean_hits=a["mean_hits"],
        deaths=a["deaths"], hits_all=a["hits_all"],
        median_hits_all=a["median_hits_all"], p90_hits_all=a["p90_hits_all"],
        median_ticks=a["median_ticks"], stalls=a["stalls"],
        default_completed=a["default_completed"],
        hits_per_seed={str(s): r["hits"] for s, r in runs.items()}),
        separators=(",", ":")))


def row(batch, commit, change, verdict):
    """One docs/bot-scoreboard.md table row for a batch."""
    a = aggregate(load(batch))
    default = {None: "-", True: "yes", False: "NO"}[a["default_completed"]]
    print(f"| {os.path.basename(os.path.normpath(batch))} | {commit} | {change} | "
          f"{a['completed']}/{a['runs']} | {a['deaths']} | {fmt(a['median_hits_all'])} | "
          f"{fmt(a['p90_hits_all'])} | {fmt(a['median_ticks'])} | {a['stalls']} | "
          f"{default} | {verdict} |")


def compare(a_dir, b_dir):
    a, b = aggregate(load(a_dir)), aggregate(load(b_dir))
    print(f"A = {a_dir}\nB = {b_dir}\n")
    print(f"{'':<18} {'A':>8} {'B':>8} {'B-A':>8}")
    for label, key in (("runs", "runs"), ("completed", "completed"),
                       ("completion rate", "completion_rate"),
                       ("median hits", "median_hits"), ("mean hits", "mean_hits"),
                       ("deaths", "deaths"), ("hits (all runs)", "hits_all"),
                       ("median hits all", "median_hits_all"),
                       ("p90 hits all", "p90_hits_all"),
                       ("median ticks", "median_ticks"), ("stalls", "stalls")):
        x, y = a[key], b[key]
        d = round(y - x, 2) if x is not None and y is not None else None
        print(f"{label:<18} {fmt(x):>8} {fmt(y):>8} {fmt(d):>8}")
    for title, key in (("enemy id", "by_enemy"), ("room", "by_room"),
                       ("enemy@room", "by_source")):
        rows = []
        for k in set(a[key]) | set(b[key]):
            d = b[key][k] - a[key][k]
            if d:
                rows.append((d, k))
        rows.sort(key=lambda t: (-abs(t[0]), t[1]))
        print(f"\nhit changes by {title} (B-A, summed over each batch's runs):")
        for d, k in rows[:15]:
            print(f"  {k:<14} {a[key][k]:>4} -> {b[key][k]:>4}  ({d:+d})")
        if not rows:
            print("  none")
    print("\nnote: the batches should use the same number of seeds; "
          "counts are sums, medians/means are over completed runs.")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "summary":
        summary(sys.argv[2])
    elif len(sys.argv) == 4 and sys.argv[1] == "compare":
        compare(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 6 and sys.argv[1] == "row":
        row(*sys.argv[2:])
    else:
        sys.exit(__doc__)
