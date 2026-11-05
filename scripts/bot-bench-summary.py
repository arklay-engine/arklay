#!/usr/bin/env python3
"""Summarise one bot-bench run: tick buckets, saves, box visits, bounces.

Usage: scripts/bot-bench-summary.py <stats.json> [<run.log>]
The log defaults to the stats file with a .log suffix.
"""
import json, re, sys
from collections import defaultdict

path = sys.argv[1]
log = sys.argv[2] if len(sys.argv) > 2 else re.sub(r"\.json$", ".log", path)
d = json.load(open(path))
total = d["ticks"] or 1

act = d.get("activity") or {}
if act:
    print("  activity: " + ", ".join(
        f"{k} {v} ({100*v/total:.0f}%)" for k, v in sorted(act.items(), key=lambda kv: -kv[1])))

try:
    lines = open(log, errors="replace").read().splitlines()
except OSError:
    sys.exit(0)
tick_re = re.compile(r"^\[bot\] tick (\d+) room (\w+): (.*)$")
events = [(int(m[1]), m[2], m[3]) for m in map(tick_re.match, lines) if m]

# Ticks per goal kind: from one "goal:" line to the next.
goals = [(t, re.match(r"goal: (\w+(?: box)?)", msg)[1]) for t, _, msg in events
         if msg.startswith("goal: ")]
spent = defaultdict(int)
for (t, kind), nxt in zip(goals, goals[1:] + [(d["ticks"], None)]):
    spent[kind] += nxt[0] - t
if goals:
    spent["before first goal"] += goals[0][0]
    print("  goal time: " + ", ".join(
        f"{k} {v}" for k, v in sorted(spent.items(), key=lambda kv: -kv[1])))

saves = [(t, r, msg) for t, r, msg in events if re.search(r"typewriter slot \d+: save \(", msg)]
print(f"  saves {d.get('saves')}: " + "; ".join(
    f"{t} {r} {re.search(r'save \((.*)\)', m)[1]}" for t, r, m in saves))
boxes = [(t, r, msg) for t, r, msg in events if msg.startswith("goal: item box")]
print(f"  box visits {len(boxes)}: " + "; ".join(
    f"{t} {r} {re.search(r'\((.*)\)', m)[1] if '(' in m else m}" for t, r, m in boxes))
full = sorted({re.search(r"mark (item:\w+:\d+)", m)[1] for _, _, m in events
               if "inventory full" in m and m.startswith("mark ")})
print(f"  inventory-full marks {len(full)}: {' '.join(full)}")

entries = [(e["tick"], e["room"]) for e in d.get("room_entries", [])]
stays = [(r, nxt[0] - t) for (t, r), nxt in zip(entries, entries[1:])]
# A bounce: A -> B -> A with the stay in B short.
bounces = [(entries[i][1], entries[i + 1][1], stays[i + 1][1] if i + 1 < len(stays) else 0)
           for i in range(len(entries) - 2) if entries[i][1] == entries[i + 2][1]]
q3 = [b for b in bounces if b[2] < 300]
q9 = [b for b in bounces if b[2] < 900]
print(f"  bounces <300: {len(q3)} <900: {len(q9)}: " +
      " ".join(f"{a}>{b}({n})" for a, b, n in q9))
