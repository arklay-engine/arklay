#!/usr/bin/env python3
"""List the bot's wasted door trips: revisits of a room that do nothing there.

  bot-bounces.py [options] LOG|BATCH-DIR ...

Reads bot logs (`[bot] tick T room R: ...` lines; a directory stands for its
seed-*.log files) and splits each run into visits: a visit starts at a
`room R pos=(...) health=` line and ends at the next one. Inside a visit it
looks for purpose, anything that is not walking to a door:

  - a `goal done:` of any goal but a door, a stair check or a passage check
    (an item or box take, a use, an interact, a push, a drawer, a fight ...),
  - a `combat hit` (the bot fought there), a `saved:`, a `heal`, an
    `inventory:` box operation, a `menu:` operation other than a combat
    equip or reload, a `story: ... set/cleared` flag, a `terminal:` entry.

A visit with none of that is idle. A visit with a non-door goal begun but
never finished is futile (it tried, and left without it).

Consecutive visits without purpose form a chain that starts at the last
room where something was done. The chain keeps its simple path as a stack;
entering a room already on the path closes a cycle, and the rooms popped
were walked for nothing. A cycle closed inside a larger one merges into it,
so one wasted detour is one episode with its ticks counted once:

  bounce   A -> B -> A: B entered and left straight back the way it came,
  loop     a longer cycle (A -> B -> C -> B -> A, or A -> B -> C -> A): the
           round trip served nothing.

An episode whose rooms were all entered before is a wasted trip. One that
entered a room for the first time is exploring (counted apart, listed with
--first), and one in which a goal was begun and abandoned is futile (counted
apart unless --futile). Pass-throughs (a revisit left by another door on the
way somewhere) close no cycle and are not counted.

For each wasted trip the report prints the tick of its turnaround visit (the
room where the walk turned back), the rooms, the door used, how long it
stayed, the ticks lost (from starting the door goal that left the room the
walk comes back to, to coming back), the rooms walked, and the planner's
stated reasons: the door goal that took the bot in (`in`), the `target:`,
`way out:`, `goal:`, `mark`, `progress (...)` and `frontier empty` lines
inside the visit, and the door goal that took it out (`out`). It tags a
trip `relearn` when the turnaround logged a mark the run had already made
with the same reason (it walked back to find out again), and
`after-reload` when it came after a death's reload. The cause class:

  step-out     a `(step out to re-enter R)` door goal (forgive_approaches /
               forgive_events),
  hazard       a scripted way out took it in or out (a <leave>: 507's gas,
               30F's boulder),
  frontier     a `frontier empty` retry inside the visit,
  door-test    the target was the door into this room, now traversed, and
               the bot turned back,
  door-here    it came for a door of this room and did not take it (out of
               reach, a room event's message),
  target-here  it came for another target in this room and dropped it,
  route-flip   in and out for the same remote target (the route turned back),
  retarget     in for one target, out for another (re-ranked on entry),
  unvisited    out to an unvisited room,
  other        anything else; the reasons say what.

A reload after a death starts a new chain.

Options:
  --futile     count futile visits (a goal tried and abandoned) as wasted
               too; by default they are listed apart
  --first      also consider first visits (exploration bounces)
  -v           print every wasted trip with its reasons, and the futile
               ones (default: the first 40 wasted trips per run)
  -q           summary only
  --json       one JSON object per run on stdout
"""
import glob, json, os, re, sys
from collections import Counter

LINE = re.compile(r"^\[bot\] tick (\d+) room (\w+): (.*)$")
ENTRY = re.compile(r"^room (\w+) pos=\(")
DOOR_GOAL = re.compile(r"^goal: door slot (\d+) -> (\w+) \((.*)\)(?: via slot \d+)?$")
GOAL = re.compile(r"^goal: (.*)$")
DONE = re.compile(r"^goal done: (.*)$")
NAV_GOAL = re.compile(r"^(door slot|stair check|passage check)")
MARK = re.compile(r"^mark (\S+): (.*)$")
RELOAD = re.compile(r"^\[bot\] tick \d+ reloaded the save")
PURPOSE = (
    re.compile(r"^combat hit "),
    re.compile(r"^saved: "),
    re.compile(r"^heal item="),
    re.compile(r"^inventory: (deposited|withdrew|box)"),
    re.compile(r"^menu: (?!combat (equip|reload))"),
    re.compile(r"^story: .* (set|cleared)$"),
    re.compile(r"^terminal: "),
    re.compile(r"^drawer .*took"),
)
REASON = re.compile(r"^(target: |target dropped: |way out|frontier empty|goal: |mark |progress \(|loop count)")


def norm(s):
    """Reason text with numbers and ids folded, for grouping."""
    s = re.sub(r"\b0x[0-9A-Fa-f]+\b", "X", s)
    s = re.sub(r"-?\b\d+\b", "N", s)
    s = re.sub(r"\b[0-9A-F]{3}\b", "R", s)
    return s


def target_of(reason):
    """The target a door goal's reason names, or None."""
    if reason is None:
        return None
    m = re.match(r"(?:pursuing|toward) (.*?)(?:, \d+ door\(s\))?$", reason)
    if m:
        return m[1]
    m = re.match(r"reached (.*)$", reason)
    if m:
        return m[1]
    return None


class Visit:
    __slots__ = ("room", "tick", "end", "src", "reload", "purpose", "tried",
                 "reasons", "exits", "nav_start", "first", "frontier", "relearn",
                 "reloads")

    def __init__(self, room, tick, src, reload):
        self.room, self.tick, self.end, self.src = room, tick, tick, src
        self.reload = reload
        self.purpose = []      # what was done here
        self.tried = []        # non-door goals begun
        self.reasons = []      # target/way out/goal lines
        self.exits = []        # (tick, slot, dest, reason) door goals begun
        self.nav_start = None  # tick of the first nav goal of the last exit run
        self.first = True
        self.frontier = False
        self.relearn = []      # marks logged here that the run had made before
        self.reloads = 0       # reloads earlier in the run


def parse(path):
    visits, cur, reload = [], None, False
    seen = set()
    marked = {}     # mark key -> reason, as first logged
    reloads = 0
    with open(path, errors="replace") as f:
        for line in f:
            if line.startswith("[bot] tick ") and " reloaded the save" in line:
                reload = True
                reloads += 1
                continue
            m = LINE.match(line)
            if not m:
                continue
            tick, room, msg = int(m[1]), m[2], m[3]
            e = ENTRY.match(msg)
            if e:
                if cur:
                    cur.end = tick
                src = None if (reload or not cur) else cur.room
                cur = Visit(e[1], tick, src, reload)
                cur.first = e[1] not in seen
                cur.reloads = reloads
                seen.add(e[1])
                visits.append(cur)
                reload = False
                continue
            if cur is None:
                continue
            cur.end = tick
            d = DOOR_GOAL.match(msg)
            if d:
                cur.exits.append((tick, int(d[1]), d[2], d[3]))
                cur.reasons.append((tick, msg))
                continue
            g = GOAL.match(msg)
            if g:
                if not NAV_GOAL.match(g[1]):
                    cur.tried.append(g[1])
                cur.reasons.append((tick, msg))
                continue
            dn = DONE.match(msg)
            if dn:
                if not NAV_GOAL.match(dn[1]):
                    cur.purpose.append(f"done {dn[1]}")
                continue
            if REASON.match(msg):
                cur.reasons.append((tick, msg))
                mk = MARK.match(msg)
                if mk:
                    reason = re.sub(r" \(attempt \d+\)$", "", mk[2])
                    reason = re.sub(r": [0-9A-F,]+\+?$", "", reason)
                    if marked.get(mk[1]) == reason:
                        cur.relearn.append(mk[1])
                    marked.setdefault(mk[1], reason)
                if msg.startswith("frontier empty"):
                    cur.frontier = True
                continue
            for p in PURPOSE:
                if p.match(msg):
                    cur.purpose.append(msg[:60])
                    break
    return visits


def entry_reason(prev):
    """The door goal that took the bot out of visit `prev` (its last one)."""
    if prev and prev.exits:
        return prev.exits[-1]
    return None


def cause(v, into, out):
    """The class of a wasted turnaround visit `v`, from its reasons."""
    in_r = into[3] if into else None
    out_r = out[3] if out else None
    if (out_r or "").startswith("step out to re-enter") \
            or (in_r or "").startswith("step out to re-enter"):
        return "step-out"
    here = re.escape(v.room)
    t_in, t_out = target_of(in_r), target_of(out_r)
    if in_r and not t_in and in_r not in ("unvisited room", "untraversed") \
            and not in_r.startswith("unvisited room ("):
        return "hazard"     # a <leave> or a scripted way out took it in
    if out_r and not t_out and out_r not in ("unvisited room", "untraversed") \
            and not out_r.startswith("unvisited room ("):
        return "hazard"
    if v.frontier:
        return "frontier"
    if t_in and re.match(r"the \w+ -> " + here + r" door", t_in) and in_r.startswith("reached"):
        return "door-test"  # the target was the door into this room
    if t_in and re.match(r"the " + here + r" -> \w+ door", t_in):
        return "door-here"  # came for a door of this room, did not take it
    if t_in and re.search(r"\bin " + here + r"\b", t_in):
        return "target-here"  # came for a target in this room, dropped it
    if t_in and t_in == t_out:
        return "route-flip"
    if t_in and t_out and t_in != t_out:
        return "retarget"
    if out_r == "unvisited room":
        return "unvisited"
    return "other"


def analyse(path, futile=False, first=False):
    """Wasted episodes of one run.

    Consecutive visits without purpose form a chain, starting at the last
    room where something was done. A stack holds the rooms of the chain's
    current simple path; entering a room already on it closes a cycle (the
    rooms popped were walked for nothing). Cycles closed inside a larger one
    merge into it, so one wasted detour is one episode, its ticks counted
    once. The episode's turnaround is the visit that first led back."""
    visits = parse(path)
    episodes = []
    stack = []      # [visit index] of the chain's simple path
    open_eps = []   # episodes of the current chain, for merging

    def leave_tick(j):
        ex = visits[j].exits
        return ex[-1][0] if ex else visits[j].end

    for i, v in enumerate(visits):
        if v.reload or i == 0:
            stack, open_eps = [i], []
            continue
        pos = next((k for k in range(len(stack) - 1, -1, -1)
                    if visits[stack[k]].room == v.room), None)
        if pos is not None and pos < len(stack) - 1:
            popped = stack[pos + 1:]
            start = leave_tick(stack[pos])
            turn = popped[-1]
            ep = dict(start=start, end=v.tick, sidx=stack[pos], eidx=i, rooms=[visits[stack[pos]].room]
                      + [visits[j].room for j in popped] + [v.room],
                      turn=turn, first=any(visits[j].first for j in popped),
                      futile=any(visits[j].tried for j in popped),
                      tried=[g for j in popped for g in visits[j].tried],
                      relearn=[k for j in popped for k in visits[j].relearn])
            # merge the episodes this one contains
            inner = [e for e in open_eps if e["start"] >= start]
            if inner:
                ep["turn"] = inner[0]["turn"]
                ep["first"] = ep["first"] or any(e["first"] for e in inner)
                ep["futile"] = ep["futile"] or any(e["futile"] for e in inner)
                ep["tried"] = [g for e in inner for g in e["tried"]] + ep["tried"]
                ep["relearn"] = [k for e in inner for k in e["relearn"]] + ep["relearn"]
                open_eps = [e for e in open_eps if e["start"] < start]
                ep["inner"] = True
            open_eps.append(ep)
            episodes.append(ep)
            for e in inner:
                episodes.remove(e)
            stack = stack[:pos] + [i]
        else:
            stack.append(i)
        if v.purpose:
            stack, open_eps = [i], []
    trips = []
    for ep in episodes:
        t = visits[ep["turn"]]
        prev = visits[ep["turn"] - 1]
        into = entry_reason(prev)
        out = entry_reason(t)
        walk = [visits[j].room for j in range(ep["sidx"], ep["eidx"] + 1)]
        kind = "bounce" if len(walk) == 3 else "loop"
        nxt = visits[ep["turn"] + 1] if ep["turn"] + 1 < len(visits) else None
        c = cause(t, into, out)
        if ep["futile"]:
            c = "futile:" + c
        trips.append(dict(
            kind=kind, futile=ep["futile"], explore=ep["first"],
            wasted=(first or not ep["first"]) and (futile or not ep["futile"]),
            tick=t.tick, room=t.room, src=t.src, dst=nxt.room if nxt else None,
            door=out[1] if out else None, stay=t.end - t.tick,
            lost=ep["end"] - ep["start"], cause=c,
            into=into[3] if into else None, out=out[3] if out else None,
            tried=ep["tried"], reasons=[r for _, r in t.reasons],
            chain=walk, relearn=ep["relearn"], after_reload=t.reloads > 0))
    trips.sort(key=lambda t: t["tick"])
    end = visits[-1].end if visits else 0
    return dict(log=path, visits=len(visits), end_tick=end, trips=trips)


def logs_of(args):
    out = []
    for a in args:
        if os.path.isdir(a):
            out += sorted(glob.glob(os.path.join(a, "seed-*.log")),
                          key=lambda p: int(re.search(r"seed-(\d+)", p)[1]))
        else:
            out.append(a)
    return out


def main(argv):
    opts = {a for a in argv if a.startswith("-")}
    paths = logs_of([a for a in argv if not a.startswith("-")])
    if not paths:
        print(__doc__)
        return 2
    futile, first = "--futile" in opts, "--first" in opts
    total = Counter()
    total_lost = Counter()
    total_cause = Counter()
    runs = []
    for p in paths:
        r = analyse(p, futile, first)
        runs.append(r)
        if "--json" in opts:
            print(json.dumps(r))
            continue
        w = [t for t in r["trips"] if t["wasted"]]
        fut = [t for t in r["trips"] if t["futile"] and not t["explore"] and not t["wasted"]]
        expl = [t for t in r["trips"] if t["explore"] and not t["wasted"]]
        kinds = Counter(t["kind"] for t in w)
        causes = Counter(t["cause"] for t in w)
        lost = sum(t["lost"] for t in w)
        for t in w:
            total[t["kind"]] += 1
            total_lost[t["kind"]] += t["lost"]
            total_cause[t["cause"]] += 1
            total_lost["cause:" + t["cause"]] += t["lost"]
            if t["relearn"]:
                total["relearn"] += 1
            if t["after_reload"]:
                total["after-reload"] += 1
        total["futile"] += len(fut)
        total["explore"] += len(expl)
        print(f"== {p}: {r['visits']} visits, end tick {r['end_tick']}; "
              f"wasted {len(w)} ({', '.join(f'{k} {n}' for k, n in kinds.most_common()) or 'none'}), "
              f"{lost} ticks lost; futile {len(fut)}; exploring {len(expl)}")
        if causes:
            print("   causes: " + ", ".join(f"{k} {n}" for k, n in causes.most_common()))
            rl = sum(1 for t in w if t["relearn"])
            ar = sum(1 for t in w if t["after_reload"])
            print(f"   relearned a mark: {rl}; after a reload: {ar}")
        if "-q" in opts:
            continue
        show = w if "-v" in opts else w[:40]
        for t in show:
            print(f"  tick {t['tick']:>6} {t['kind']:<6} {t['src']} -> {t['room']} -> {t['dst']}"
                  f" (door slot {t['door']}) stay {t['stay']} lost {t['lost']} [{t['cause']}]"
                  f" walk {'>'.join(t['chain'])}"
                  + (" relearn " + ",".join(sorted(set(t["relearn"]))) if t["relearn"] else "")
                  + (" after-reload" if t["after_reload"] else ""))
            print(f"      in : {t['into']}")
            for rr in t["reasons"]:
                if not rr.startswith("goal: door"):
                    print(f"      .. {rr[:150]}")
            print(f"      out: {t['out']}")
        if len(show) < len(w):
            print(f"  ... {len(w) - len(show)} more (-v)")
        if fut and "-v" in opts:
            print("  futile revisits (a goal tried, nothing done):")
            for t in fut:
                print(f"  tick {t['tick']:>6} {t['kind']:<6} {t['src']} -> {t['room']} -> {t['dst']}"
                      f" lost {t['lost']} walk {'>'.join(t['chain'])} tried {t['tried'][:2]}")
    if "--json" not in opts and len(paths) > 1:
        print(f"== total over {len(paths)} runs: "
              + ", ".join(f"{k} {n}" for k, n in total.most_common())
              + f"; ticks lost {total_lost['bounce'] + total_lost['loop']}")
        print("   causes: " + ", ".join(f"{k} {n} ({total_lost['cause:' + k]} ticks)"
                                       for k, n in total_cause.most_common()))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
