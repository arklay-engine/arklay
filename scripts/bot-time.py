#!/usr/bin/env python3
"""Report where a bot run's game time went.

  bot-time.py [options] LOG|BATCH-DIR ...

Reads bot logs (`[bot] tick T room R: ...` lines; a directory stands for its
seed-*.log files) and lays the run out as a timeline: the ticks between two
log lines go to the state the bot was in at the first of them. The state is
the first of these that holds:

  combat         between `combat start` and `combat end` (a combat equip or
                 reload menu inside it counts as combat too), or a `fight` goal
  menus          a `menu: LABEL: start ...` job until its next `menu:` line
                 (a `heal` job is healing), or a `combine` goal
  typewriter     from `typewriter slot N: save (...)` to `saved:`
  scenes         the last `scene` line says trans=true (a door transition),
                 film=true (a film), skip/skippable with no control (a cut
                 scene), or no control while no goal runs
  goal           the running goal, from `goal: LABEL` until `goal done:`,
                 `goal paused:`, a `mark` of the goal's own key (failed), a new
                 goal (superseded) or leaving the room. Door goals are walking,
                 split by the purpose in their label (unvisited room, item,
                 use, the item box, the typewriter, a door, ...); item pickups,
                 the item box, puzzle steps (interact, use, push, step, climb,
                 drawer, deliver, fetch, brew, ...), stair and passage checks
                 and waits are their own groups
  waits          after `frontier empty; waiting for ...` or `waiting for the
                 use probe` until the next goal
  idle           none of the above (no goal)

It prints, per run: the totals by activity group and detail, the top rooms
by time (with entries), the top goals (single instances, and summed by room
and label), the failed goals with the ticks they cost and why, and the time
per stage (the first hex digit of the room) with the sequence of stage
changes, so walking back and forth between areas shows. With several logs it
also prints the activity totals summed over them. Ticks are 30 per second.

Options:
  --top N     rows in each top list (default 15)
  --json      one JSON object instead of the text report
"""

import argparse
import glob
import json
import os
import re
import sys
from collections import defaultdict

TICKS_PER_SECOND = 30

LINE_RE = re.compile(r"\[bot\] tick (\d+) room ([0-9A-Fa-f]+): (.*)")
ROOM_ENTRY_RE = re.compile(r"room ([0-9A-Fa-f]+) pos=\(")
SLOT_RE = re.compile(r"\bslot (\d+)")
MARK_RE = re.compile(r"mark ([a-z]+):([0-9A-Fa-f]+):(\d+)?:? ?(.*)")
DOOR_RE = re.compile(r"door slot (\d+) -> ([0-9A-Fa-f]+)(?: \((.*)\))?(?: via .*)?$")
OUTCOME_RE = re.compile(r"\[bot\] outcome=(\S+) ticks=(\d+)")
COMBAT_ID_RE = re.compile(r"id=(\S+)")

PUZZLE_KINDS = {"interact", "use", "push", "step", "climb", "drawer",
                "deliver", "fetch", "brew", "open", "load", "terminal"}
CHECK_KINDS = {"stair", "passage"}


def goal_kind(label):
    """The goal kind: the first word, or two for `item box`, `stair check`."""
    if label.startswith("item box"):
        return "item box"
    if label.startswith("stair check"):
        return "stair check"
    if label.startswith("passage check"):
        return "passage check"
    if label.startswith("open doom"):
        return "open doom"
    if label.startswith("load combine"):
        return "load combine"
    return label.split(" ", 1)[0]


# The mark-key kind each goal kind fails under.
MARK_KIND = {"item box": "box", "stair check": "stair", "passage check": "passage",
             "fight": "interact"}


def door_purpose(label):
    m = DOOR_RE.match(label)
    why = (m.group(3) or "") if m else ""
    w = re.sub(r"^(toward|pursuing) ", "", why)
    if why.startswith("unvisited room"):
        return "unvisited room"
    if why.startswith("untraversed"):
        return "untraversed door"
    if why.startswith("reached the"):
        return "a door (reached)"
    if why.startswith("the boulder"):
        return "the boulder escape"
    if why.startswith("step out") or why.startswith("out to"):
        return "step out to retry"
    if w.startswith("the item box"):
        return "the item box"
    if w.startswith("the typewriter"):
        return "the typewriter"
    if w.startswith("the fight"):
        return "a fight"
    if re.match(r"the [0-9A-Fa-f]+ -> [0-9A-Fa-f]+ door", w):
        return "a door"
    for k in ("item", "use", "fetch", "deliver", "room", "interact", "push"):
        if w.startswith(k + " "):
            return k
    return w.split(" ", 1)[0] or "other"


def goal_activity(kind, label):
    if kind == "door":
        return "walk to door", "for " + door_purpose(label)
    if kind == "item":
        return "item pickup", "item"
    if kind == "item box":
        return "item box", "box visit"
    if kind == "fight":
        return "combat", "fight goal"
    if kind == "combine":
        return "healing/menus", "combine goal"
    if kind in ("stair check", "passage check"):
        return "checks", kind
    if kind == "wait":
        return "waits", "wait goal"
    if kind.split(" ")[0] in PUZZLE_KINDS or kind in ("open doom", "load combine"):
        return "puzzle steps", kind
    return "puzzle steps", kind


class Run:
    def __init__(self, path):
        self.path = path
        self.activity = defaultdict(int)  # (group, detail) -> ticks
        self.room_ticks = defaultdict(int)
        self.room_entries = defaultdict(int)
        self.goals = []
        self.stage_segments = []  # [stage, start, end]
        self.outcome = None
        self.total = 0
        self.completed = False
        self.finish = ""
        self.last_tick = 0

    def to_json(self, top):
        acts = sorted(self.activity.items(), key=lambda kv: -kv[1])
        groups = defaultdict(int)
        for (g, _), t in acts:
            groups[g] += t
        by_label = defaultdict(lambda: {"count": 0, "ticks": 0, "failed": 0})
        for g in self.goals:
            e = by_label[(g["room"], g["label"])]
            e["count"] += 1
            e["ticks"] += g["ticks"]
            if g["outcome"] == "failed":
                e["failed"] += 1
        failed = [g for g in self.goals if g["outcome"] == "failed"]
        fail_reasons = defaultdict(lambda: [0, 0])
        for g in failed:
            r = re.sub(r" \(attempt \d+\)$", "", g["reason"])
            r = re.sub(r"[0-9A-F]{2}(,[0-9A-F]{2})+\+?", "...", r)
            fail_reasons[r][0] += 1
            fail_reasons[r][1] += g["ticks"]
        stages = defaultdict(lambda: {"ticks": 0, "visits": 0})
        for s, a, b in self.stage_segments:
            stages[s]["ticks"] += b - a
            stages[s]["visits"] += 1
        return {
            "log": self.path,
            "outcome": self.outcome,
            "completed": self.completed,
            "finish": self.finish,
            "ticks": self.total,
            "groups": dict(sorted(groups.items(), key=lambda kv: -kv[1])),
            "activity": [{"group": g, "detail": d, "ticks": t} for (g, d), t in acts],
            "rooms": [{"room": r, "ticks": t, "entries": self.room_entries.get(r, 0)}
                      for r, t in sorted(self.room_ticks.items(), key=lambda kv: -kv[1])[:top]],
            "top_goals": sorted(self.goals, key=lambda g: -g["ticks"])[:top],
            "top_goal_labels": [dict(room=r, label=l, **v) for (r, l), v in
                                sorted(by_label.items(), key=lambda kv: -kv[1]["ticks"])[:top]],
            "failed_ticks": sum(g["ticks"] for g in failed),
            "failed_count": len(failed),
            "failed_reasons": [{"reason": r, "count": c, "ticks": t} for r, (c, t) in
                               sorted(fail_reasons.items(), key=lambda kv: -kv[1][1])[:top]],
            "failed_goals": sorted(failed, key=lambda g: -g["ticks"])[:top],
            "stages": dict(sorted(stages.items())),
            "stage_sequence": [{"stage": s, "start": a, "ticks": b - a}
                               for s, a, b in self.stage_segments],
        }


def parse(path):
    run = Run(path)
    goal = None          # dict for the running goal
    combat = None        # detail string while in combat
    menu = None          # label of the running menu job
    saving = False
    waiting = None
    scene = "ctrl"       # ctrl, trans, film, cut, noctrl
    last_tick = 0
    last_room = None
    cur_room = None
    stage = None

    def state():
        if combat is not None:
            if menu is not None and menu.startswith("combat"):
                return ("combat", "menu " + menu)
            return ("combat", combat)
        if menu is not None:
            if menu.startswith("heal") or menu.startswith("cure"):
                return ("healing/menus", "heal")
            return ("healing/menus", re.sub(r" 0x[0-9A-Fa-f]+.*", "", menu))
        if saving:
            return ("typewriter save", "save")
        if scene == "trans":
            return ("scenes/transitions", "door transition")
        if scene == "film":
            return ("scenes/transitions", "film")
        if scene == "cut":
            return ("scenes/transitions", "cut scene")
        if goal is not None:
            return goal["activity"]
        if waiting is not None:
            return ("waits", waiting)
        if scene == "noctrl":
            return ("scenes/transitions", "no control")
        return ("idle/no goal", "no goal")

    def end_goal(tick, outcome, reason=""):
        nonlocal goal
        if goal is None:
            return
        goal["end"] = tick
        goal["ticks"] = tick - goal["start"]
        goal["outcome"] = outcome
        goal["reason"] = reason
        del goal["activity"]
        del goal["slot"]
        run.goals.append(goal)
        goal = None

    with open(path, "r", errors="replace") as f:
        for line in f:
            if not line.startswith("[bot] tick "):
                if line.startswith("[bot] outcome="):
                    m = OUTCOME_RE.match(line)
                    if m:
                        run.outcome = m.group(1)
                        run.total = int(m.group(2))
                continue
            m = LINE_RE.match(line)
            if not m:
                continue
            tick = int(m.group(1))
            room = m.group(2).upper()
            msg = m.group(3)
            dt = tick - last_tick
            if dt > 0:
                run.activity[state()] += dt
                if last_room is not None:
                    run.room_ticks[last_room] += dt
            last_tick = tick
            last_room = room
            c = msg[:6]

            if c == "scene ":
                if msg.startswith("scene film="):
                    if "trans=true" in msg:
                        scene = "trans"
                    elif "film=true" in msg:
                        scene = "film"
                    elif "ctrl=true" in msg:
                        scene = "ctrl"
                    elif "skip=true" in msg or "skippable=true" in msg:
                        scene = "cut"
                    else:
                        scene = "noctrl"
                continue
            if c[:5] == "room " and ROOM_ENTRY_RE.match(msg):
                r = ROOM_ENTRY_RE.match(msg).group(1).upper()
                if goal is not None:
                    if goal["kind"] == "door":
                        dm = DOOR_RE.match(goal["label"])
                        if dm and dm.group(2).upper() == r:
                            end_goal(tick, "done")
                        else:
                            end_goal(tick, "left room", "entered " + r)
                    else:
                        end_goal(tick, "left room", "entered " + r)
                combat = None
                menu = None
                saving = False
                waiting = None
                run.room_entries[r] += 1
                cur_room = r
                s = r[0]
                if s != stage:
                    if run.stage_segments:
                        run.stage_segments[-1][2] = tick
                    run.stage_segments.append([s, tick, tick])
                    stage = s
                continue
            if c == "goal: ":
                label = msg[6:]
                end_goal(tick, "superseded", "goal: " + label)
                kind = goal_kind(label)
                sm = SLOT_RE.search(label)
                goal = {"room": room, "label": label, "kind": kind, "start": tick,
                        "slot": sm.group(1) if sm else None,
                        "activity": goal_activity(kind, label)}
                waiting = None
                saving = False
                continue
            if c == "goal d" and msg.startswith("goal done: "):
                if goal is not None and msg[11:] == goal["label"]:
                    end_goal(tick, "done")
                continue
            if c == "goal p" and msg.startswith("goal paused: "):
                if goal is not None and msg[13:] == goal["label"]:
                    end_goal(tick, "paused")
                continue
            if c[:5] == "mark ":
                if goal is None:
                    continue
                mm = MARK_RE.match(msg)
                if not mm:
                    continue
                kind, mroom, mslot, reason = mm.groups()
                if mroom.upper() != goal["room"]:
                    continue
                want = MARK_KIND.get(goal["kind"], goal["kind"].split(" ")[0])
                if kind != want:
                    continue
                if goal["slot"] is not None and mslot is not None and mslot != goal["slot"]:
                    continue
                end_goal(tick, "failed", reason)
                continue
            if c == "combat":
                if msg.startswith("combat start"):
                    im = COMBAT_ID_RE.search(msg)
                    combat = "enemy " + (im.group(1) if im else "?")
                elif msg.startswith("combat end"):
                    combat = None
                continue
            if c == "menu: ":
                text = msg[6:]
                i = text.find(": start ")
                if i >= 0:
                    menu = text[:i]
                else:
                    menu = None
                continue
            if c == "typewr":
                if ": save (" in msg:
                    saving = True
                continue
            if c == "saved:":
                saving = False
                continue
            if c == "fronti":
                if msg.startswith("frontier empty; waiting"):
                    waiting = "frontier empty"
                continue
            if c == "waitin":
                waiting = "use probe"
                continue
            if c == "FINISH" or c == "finish":
                run.finish = msg.split(" ", 1)[1] if " " in msg else msg
                if "game completed" in msg:
                    run.completed = True
                continue

    end = run.total or last_tick
    if end > last_tick:
        run.activity[state()] += end - last_tick
        if last_room is not None:
            run.room_ticks[last_room] += end - last_tick
    end_goal(end, "unfinished")
    if run.stage_segments:
        run.stage_segments[-1][2] = end
    run.total = end
    run.last_tick = last_tick
    return run


def fmt_t(t):
    s = t // TICKS_PER_SECOND
    return f"{t:>7} {s // 60:>3}:{s % 60:02d}"


def pct(t, total):
    return f"{100.0 * t / total:5.1f}%" if total else "    -"


def short(s, n=70):
    return s if len(s) <= n else s[:n - 3] + "..."


def print_run(j, top):
    total = j["ticks"]
    print(f"== {j['log']}")
    print(f"outcome={j['outcome']} completed={'yes' if j['completed'] else 'no'} "
          f"ticks={total} ({total // TICKS_PER_SECOND // 60}m{total // TICKS_PER_SECOND % 60:02d}s)")
    if j["finish"]:
        print("finish: " + short(j["finish"], 110))
    print("\nactivity                          ticks   m:ss      %")
    by_group = defaultdict(list)
    for a in j["activity"]:
        by_group[a["group"]].append(a)
    for g, gt in j["groups"].items():
        print(f"  {g:<30}{fmt_t(gt)} {pct(gt, total)}")
        for a in by_group[g]:
            if len(by_group[g]) > 1 or a["detail"] != g:
                print(f"    {short(a['detail'], 28):<28}{fmt_t(a['ticks'])} {pct(a['ticks'], total)}")
    print(f"\ntop rooms                         ticks   m:ss      %  entries")
    for r in j["rooms"]:
        print(f"  {r['room']:<30}{fmt_t(r['ticks'])} {pct(r['ticks'], total)}  {r['entries']:>5}")
    print("\ntop goals (single)        ticks  start  room outcome     label")
    for g in j["top_goals"]:
        print(f"  {g['ticks']:>21} {g['start']:>6}  {g['room']:<4} {g['outcome']:<11} {short(g['label'], 60)}")
    print("\ntop goals (summed by room and label)  ticks count failed room label")
    for g in j["top_goal_labels"]:
        print(f"  {g['ticks']:>36} {g['count']:>5} {g['failed']:>6} {g['room']:<4} {short(g['label'], 60)}")
    print(f"\nfailed goals: {j['failed_count']}, {j['failed_ticks']} ticks "
          f"({pct(j['failed_ticks'], total).strip()})")
    print("  by reason                                       count  ticks")
    for r in j["failed_reasons"]:
        print(f"    {short(r['reason'], 46):<46}{r['count']:>6} {r['ticks']:>6}")
    print("  costliest                ticks  start  room label: reason")
    for g in j["failed_goals"]:
        print(f"  {g['ticks']:>21} {g['start']:>6}  {g['room']:<4} "
              f"{short(g['label'], 45)}: {short(g['reason'], 50)}")
    print("\nstages                            ticks   m:ss      %  visits")
    for s, v in j["stages"].items():
        print(f"  {s:<30}{fmt_t(v['ticks'])} {pct(v['ticks'], total)}  {v['visits']:>5}")
    seq = j["stage_sequence"]
    print(f"\nstage sequence ({len(seq)} segments, {max(0, len(seq) - len(j['stages']))} returns):")
    parts = [f"{s['stage']}({s['ticks'] / 1000:.1f}k@{s['start']})" for s in seq]
    line = " "
    for p in parts:
        piece = (" -> " if line.strip() else " ") + p
        if len(line) + len(piece) > 100:
            print(line)
            line = "  " + p
        else:
            line += piece
    if line.strip():
        print(line)
    print()


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                 epilog=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("paths", nargs="+", metavar="LOG|BATCH-DIR")
    ap.add_argument("--top", type=int, default=15)
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    logs = []
    for p in a.paths:
        if os.path.isdir(p):
            found = sorted(glob.glob(os.path.join(p, "seed-*.log")),
                           key=lambda s: int(re.sub(r"\D", "", os.path.basename(s)) or 0))
            if not found:
                print(f"bot-time: no seed-*.log in {p}", file=sys.stderr)
            logs.extend(found)
        elif os.path.isfile(p):
            logs.append(p)
        else:
            print(f"bot-time: no such file or directory: {p}", file=sys.stderr)
            return 2
    if not logs:
        return 2
    runs = [parse(p).to_json(a.top) for p in logs]
    combined = defaultdict(int)
    for j in runs:
        for x in j["activity"]:
            combined[(x["group"], x["detail"])] += x["ticks"]
    total = sum(j["ticks"] for j in runs)
    if a.json:
        out = {"runs": runs, "total_ticks": total,
               "combined": [{"group": g, "detail": d, "ticks": t} for (g, d), t in
                            sorted(combined.items(), key=lambda kv: -kv[1])]}
        json.dump(out, sys.stdout, indent=1)
        print()
        return 0
    for j in runs:
        print_run(j, a.top)
    if len(runs) > 1:
        print(f"== combined over {len(runs)} runs: {total} ticks")
        groups = defaultdict(int)
        for (g, _), t in combined.items():
            groups[g] += t
        for g, gt in sorted(groups.items(), key=lambda kv: -kv[1]):
            print(f"  {g:<30}{fmt_t(gt)} {pct(gt, total)}")
            for (g2, d), t in sorted(combined.items(), key=lambda kv: -kv[1]):
                if g2 == g and d != g:
                    print(f"    {short(d, 28):<28}{fmt_t(t)} {pct(t, total)}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except BrokenPipeError:
        sys.exit(0)
