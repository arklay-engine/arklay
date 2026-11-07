#!/usr/bin/env python3
"""Where a bot run's ticks go: locked vs in control, idle causes, retries.

Usage: scripts/bot-idle-report.py <stats.json> [<run.log>] [--top N]

Run the bot with `--bot-stats <stats.json>` and keep its log
(`... --bot-stats r/stats.json > r/run.log 2>&1`); the log defaults to
`run.log` beside the stats file. Needs the `timeline` and
`stalled_walk_ticks` fields the stats module writes (src/engine/bot/stats.rs).

Prints markdown:
  1. the tick budget: input locked, bot-driven menus, in control (walking,
     turning, combat, waiting);
  2. a goal-kind x activity table (the goal the bot held at each tick);
  3. the idle (waiting) ticks by goal kind and by the log line before the
     wait, plus the longest waits;
  4. retries: marks, approach sides that failed, goals that did not finish,
     door loops, with counts and ticks lost per cause.

A tick belongs to the goal between its `goal:` line and the next `goal:`,
`goal done:`, `goal paused:` or failing `mark` line.
"""
import bisect
import json
import os
import re
import sys
from collections import Counter, defaultdict

argv = sys.argv[1:]
top = 12
if "--top" in argv:
    i = argv.index("--top")
    top = int(argv[i + 1])
    del argv[i:i + 2]
stats_path = argv[0]
log_path = argv[1] if len(argv) > 1 else os.path.join(os.path.dirname(stats_path), "run.log")
d = json.load(open(stats_path))
total = d["ticks"]
act = d["activity"]
timeline = d["timeline"]

tick_re = re.compile(r"^\[bot\] tick (\d+) room (\w+): (.*)$")
events = []
for line in open(log_path, errors="replace"):
    m = tick_re.match(line.rstrip("\n"))
    if m:
        events.append((int(m[1]), m[2], m[3]))
ev_ticks = [e[0] for e in events]

LOCKED = ["doors", "scenes", "uncontrolled", "pickup"]
UI = ["saving", "menu", "item_box", "pickup_viewer", "examine"]
CONTROL = ["walking", "turning", "combat", "waiting"]


def pct(n, of=None):
    of = of or total or 1
    return f"{100 * n / of:.1f}%"


def table(headers, rows):
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join("---" for _ in headers) + "|"]
    for r in rows:
        out.append("| " + " | ".join(str(c) for c in r) + " |")
    return "\n".join(out)


def kind_of(label):
    m = re.match(r"(item box|passage check|[a-z]+)", label)
    k = m[1] if m else label.split()[0]
    if k == "door":
        p = re.search(r"\((\w+)", label)
        if p:
            return "door:" + p[1]
    return k


# Goal intervals: (start, end, kind, label, how it ended).
goals = []
cur = None
marks = []  # (tick, room, key, reason, attempt)


def close(tick, how):
    global cur
    if cur:
        goals.append((cur[0], tick, cur[1], cur[2], how))
        cur = None


for tick, room, msg in events:
    if msg.startswith("goal: "):
        close(tick, "superseded")
        label = msg[6:]
        cur = (tick, kind_of(label), label)
    elif msg.startswith("goal done: "):
        close(tick, "done")
    elif msg.startswith("goal paused: "):
        close(tick, "paused")
    else:
        m = re.match(r"mark (\S+): (.*) \(attempt (\d+)\)$", msg)
        if m:
            marks.append((tick, room, m[1], m[2], int(m[3])))
            if cur:
                close(tick, "marked")
close(total, "end")
goal_starts = [g[0] for g in goals]


def goal_at(tick):
    i = bisect.bisect_right(goal_starts, tick) - 1
    if i >= 0 and goals[i][0] <= tick < goals[i][1]:
        return goals[i]
    return None


def last_event_before(tick):
    i = bisect.bisect_right(ev_ticks, tick) - 1
    while i >= 0:
        if not events[i][2].startswith("scene "):
            return events[i]
        i -= 1
    return None


def norm(msg):
    msg = re.sub(r"\(.*?\)", "(..)", msg)
    msg = re.sub(r"0x[0-9A-Fa-f]+|\b\d+\b|\b[0-9A-F]{3}\b", "N", msg)
    return msg[:70]


# ------------------------------------------------------------ 1. budget
print(f"# Bot tick report: {os.path.basename(os.path.dirname(os.path.abspath(stats_path)))}")
print()
print(f"Outcome `{d['outcome']}`, **{total}** ticks, {d['rooms_reached']} room entries "
      f"({d['unique_rooms']} unique), {d['saves']} saves, {d['deaths']} death(s).")
print()
print("## 1. Tick budget")
print()
rows = []
for group, names in (("input locked by the game", LOCKED),
                     ("bot-driven menu / viewer / save UI", UI),
                     ("player in control", CONTROL)):
    n = sum(act.get(k, 0) for k in names)
    rows.append((f"**{group}**", f"**{n}**", f"**{pct(n)}**"))
    for k in names:
        rows.append((f"&nbsp;&nbsp;{k}", act.get(k, 0), pct(act.get(k, 0))))
print(table(["bucket", "ticks", "share"], rows))
print()
walk = act.get("walking", 0)
stalled = d["movement"].get("stalled_walk_ticks", 0)
ctrl = sum(act.get(k, 0) for k in CONTROL)
print(f"Of {ctrl} in-control ticks: walking {walk} (of which **{stalled}** left the "
      f"position unchanged: pushing into geometry), turning {act.get('turning', 0)}, "
      f"combat {act.get('combat', 0)}, **waiting (idle) {act.get('waiting', 0)}**.")
print()

# ------------------------------------------------ 2. goal x activity
by_goal = defaultdict(Counter)
for start, length, bucket in timeline:
    t, end = start, start + length
    while t < end:
        g = goal_at(t)
        if g:
            nxt, key = min(end, g[1]), g[2]
        else:
            i = bisect.bisect_right(goal_starts, t)
            nxt, key = min(end, goals[i][0] if i < len(goals) else end), "(no goal)"
        by_goal[key][bucket] += nxt - t
        t = nxt

print("## 2. Ticks by goal kind and activity")
print()
cols = ["walking", "turning", "combat", "waiting", "menu", "item_box", "pickup_viewer",
        "saving", "scenes", "doors", "uncontrolled", "pickup", "examine"]
rows = []
for key, c in sorted(by_goal.items(), key=lambda kv: -sum(kv[1].values())):
    tot = sum(c.values())
    if tot < 150:
        continue
    rows.append([key, tot] + [c.get(k, 0) or "" for k in cols])
print(table(["goal kind", "ticks"] + cols, rows))
print()

# Stalled walking by goal kind and room.
stall_goal, stall_room = Counter(), Counter()
room_at = {}
for tick, room, msg in events:
    room_at[tick] = room
room_ticks = sorted(room_at)
for start, length in d.get("stalled_runs", []):
    for t in range(start, start + length):
        g = goal_at(t)
        stall_goal[g[2] if g else "(no goal)"] += 1
        i = bisect.bisect_right(room_ticks, t) - 1
        stall_room[room_at[room_ticks[i]] if i >= 0 else "?"] += 1
if stall_goal:
    print("Stalled walking by goal kind: " + ", ".join(
        f"{k} {v}" for k, v in stall_goal.most_common(8)))
    print()
    print("Stalled walking by room: " + ", ".join(
        f"{k} {v}" for k, v in stall_room.most_common(10)))
    print()

# ------------------------------------------------------------ 3. idle
print("## 3. Idle (waiting) ticks")
print()
idle = Counter({k: c.get("waiting", 0) for k, c in by_goal.items()})
print(table(["goal kind", "idle ticks", "share of idle"],
            [(k, v, pct(v, act.get("waiting", 1)))
             for k, v in idle.most_common(top) if v]))
print()
waits = [(s, n) for s, n, b in timeline if b == "waiting"]
ctx = defaultdict(lambda: [0, 0])
for s, n in waits:
    e = last_event_before(s)
    key = norm(e[2]) if e else "(start)"
    ctx[key][0] += 1
    ctx[key][1] += n
print("Waiting segments by the last non-scene log line before them:")
print()
print(table(["log line before the wait", "segments", "ticks"],
            [(k, v[0], v[1]) for k, v in sorted(ctx.items(), key=lambda kv: -kv[1][1])[:top]]))
print()
hist = Counter()
for s, n in waits:
    hist["1" if n == 1 else "2-3" if n < 4 else "4-9" if n < 10 else "10-29" if n < 30
         else "30-99" if n < 100 else "100+"] += n
print("Waiting ticks by segment length: " + ", ".join(
    f"{k}: {hist[k]}" for k in ("1", "2-3", "4-9", "10-29", "30-99", "100+")))
print()
print(f"Longest waits ({len(waits)} segments in all):")
print()
rows = []
for s, n in sorted(waits, key=lambda w: -w[1])[:top]:
    e = last_event_before(s)
    g = goal_at(s)
    rows.append((s, e[1] if e else "", n, g[3][:60] if g else "(no goal)",
                 norm(e[2]) if e else ""))
print(table(["tick", "room", "ticks", "goal", "log line before"], rows))
print()
turn = Counter({k: c.get("turning", 0) for k, c in by_goal.items()})
print("`turning` ticks by goal kind: " + ", ".join(
    f"{k} {v}" for k, v in turn.most_common(6) if v))
print()

# --------------------------------------------------------- 4. retries
print("## 4. Retries")
print()


def reason_class(r):
    r = re.sub(r"\(common.*", "", r)
    if r.startswith("inventory full"):
        return "inventory full"
    if r.startswith("avoid:"):
        return "avoid: " + re.sub(r"\s+", " ", r[6:])[:40]
    r = re.sub(r"\(\w{3}\)", "(R)", r)
    r = re.sub(r"\b[0-9A-F]{3}\b", "R", r)
    return r[:60]


mark_rows = defaultdict(lambda: [0, 0, 0, 0])
for tick, room, key, reason, attempt in marks:
    spent = 0
    for g in reversed(goals):
        if g[1] == tick and g[4] == "marked":
            spent = g[1] - g[0]
            break
        if g[1] < tick - 1:
            break
    row = mark_rows[f"{key.split(':')[0]}: {reason_class(reason)}"]
    row[0 if attempt == 1 else 1] += 1
    row[2 if attempt == 1 else 3] += spent
print("### 4a. Marks (an AOT the planner gave up on for now)")
print()
print(table(["kind: reason", "first marks", "repeat marks (attempt>1)",
             "goal ticks before first", "goal ticks before repeats"],
            [(k, *v) for k, v in sorted(mark_rows.items(),
                                        key=lambda kv: -(kv[1][2] + kv[1][3]))[:top * 2]]))
print()
by_key = Counter(m[2] for m in marks)
print(f"Marks: {len(marks)} ({sum(1 for m in marks if m[4] > 1)} repeats); "
      f"{sum(1 for c in by_key.values() if c > 1)} keys marked more than once.")
print()

approach_re = re.compile(
    r"^(?P<label>.*?): (?:approach (?P<i>\d+)/(?P<n>\d+)(?: \(\d+,\d+\))? "
    r"(?P<what>unreachable|blocked|not reached in \d+ ticks|[a-z]+) (?:from \(.*?\); )?next side"
    r"|nothing on approach (?P<j>\d+); next side)")
job_last = {}
approach_rows = defaultdict(lambda: [0, 0])
samples = []
for tick, room, msg in events:
    if msg.startswith("goal: "):
        continue
    m = re.match(r"^(.*?): (?:zone=|at approach)", msg)
    if m:
        job_last[m[1]] = tick
        continue
    m = approach_re.match(msg)
    if m:
        label = m["label"]
        what = re.sub(r"\d+", "N", m["what"] or "pressed, nothing")
        lost = tick - job_last.get(label, tick)
        job_last[label] = tick
        k = "door" if label.startswith("door") else kind_of(label)
        approach_rows[(k, what)][0] += 1
        approach_rows[(k, what)][1] += lost
        samples.append((lost, tick, room, label[:70], what))
print("### 4b. Approach sides that failed (ticks since the job's previous event)")
print()
print(table(["goal kind", "outcome of the side", "sides", "ticks lost"],
            [(*k, *v) for k, v in sorted(approach_rows.items(), key=lambda kv: -kv[1][1])]))
print()
print("Costliest single failed sides:")
print()
print(table(["tick", "room", "ticks", "job", "outcome"],
            [(t, r, n, lab, w) for n, t, r, lab, w in sorted(samples, reverse=True)[:top]]))
print()

ab = defaultdict(lambda: [0, 0])
for start, end, kind, label, how in goals:
    # A door goal replaced by the next door goal is the route moving on.
    if kind.startswith("door") and how == "superseded":
        continue
    if how in ("paused", "superseded", "marked"):
        ab[(kind, how)][0] += 1
        ab[(kind, how)][1] += end - start
print("### 4c. Goals that ended without `goal done` (ticks spent on them)")
print()
print(table(["goal kind", "ended by", "goals", "ticks"],
            [(*k, *v) for k, v in sorted(ab.items(), key=lambda kv: -kv[1][1])[:top * 2]]))
print()
done_total = sum(e - s for s, e, k, l, h in goals if h == "done")
print(f"Goal ticks: {done_total} in goals that finished, "
      f"{sum(v[1] for v in ab.values())} in goals that did not (a pause for a menu "
      f"resumes later, so not all of that is waste).")
print()

rep = defaultdict(lambda: [0, 0])
seen = Counter()
for s, e, k, label, h in goals:
    seen[label] += 1
    if seen[label] > 1:
        rep[k][0] += 1
        rep[k][1] += e - s
print("### 4d. Goals started again with the same label (repeat starts, ticks in them)")
print()
print(table(["goal kind", "repeat starts", "ticks"],
            [(k, *v) for k, v in sorted(rep.items(), key=lambda kv: -kv[1][1])[:top]]))
print()

other = Counter()
for tick, room, msg in events:
    if msg.startswith("frontier empty; "):
        other["frontier empty: " + re.sub(r"\(.*", "", msg.split(";", 1)[1])[:12]] += 1
    elif msg.startswith("loop count"):
        other["loop count (room re-entered without progress)"] += 1
    elif "moved by a scene; replanning" in msg:
        other["approaches replanned after a scene moved the player"] += 1
    elif "leaning in" in msg:
        other["leaning in (probe fell short)"] += 1
    elif ": near approach" in msg:
        other["near approach (pressed from beside the point)"] += 1
print("### 4e. Other retry signals (counts)")
print()
print(table(["signal", "count"], other.most_common()))
print()
print(f"Stalled walking (pad up/down held, position unchanged): **{stalled}** ticks "
      f"({pct(stalled)} of the run).")
mn = d["menu"]
print()
print(f"Menu: {mn['opens']} START opens, {mn['viewer_opens']} viewer/box opens, "
      f"{mn['open_ticks']} ticks open, {mn['cursor_moves']} cursor moves, "
      f"{mn['item_uses']} item uses, {mn['box_deposits']} deposits, "
      f"{mn['box_withdrawals']} withdrawals, {mn['combines']} combines.")
