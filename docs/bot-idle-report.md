# Bot idle and retry report

Measured on the commit this report ships with (branch `t3/f2186cba`), one full
game per risk setting, `--bot-max-ticks 400000`. Both runs finish the game.
Numbers are ticks (the engine's 30 per second); a bucket is one tick in exactly
one activity (see `src/engine/bot/stats.rs`).

Re-run it:

```sh
cargo build --profile test-release
mkdir -p out/r25 out/s25
target/test-release/arklay target/bot-pack/re1.akpak --mod mods/bot --bot --bot-headless \
  --bot-max-ticks 400000 --bot-risk 0.25 --bot-stats out/r25/stats.json --save-dir out/s25 \
  > out/r25/run.log 2>&1
python3 scripts/bot-idle-report.py out/r25/stats.json   # markdown to stdout; log defaults to run.log beside it
```

A run takes 2 to 3 minutes. The two risk runs are independent; run them side by
side. `scripts/bot-idle-report.py` needs the `timeline` and `stalled_runs`
stats fields added with it.

## Headline

| | risk 0.25 | risk 0.35 |
|---|---|---|
| total ticks | **189,451** | **178,059** |
| input locked by the game (doors, scenes, scripted) | 33,451 (17.7%) | 31,377 (17.6%) |
| bot-driven UI (pick-up viewer, menu, item box, save) | 45,142 (23.8%) | 40,261 (22.6%) |
| in control | 110,858 (58.5%) | 106,421 (59.8%) |
| of which walking | 75,953 | 75,301 |
| of which turning in place | 12,855 | 12,603 |
| of which combat | 12,090 | 9,323 |
| **of which idle (no input, in control)** | **9,960 (5.3%)** | **9,194 (5.2%)** |
| walking that did not move the player (stalled) | 4,483 (2.4%) | 4,513 (2.5%) |
| ticks lost in failed approach sides | ~7,700 (4.1%) | ~7,800 (4.4%) |
| marks / repeat marks (attempt > 1) | 178 / 54 | 158 / 50 |

"Idle" is the `waiting` bucket: the player has control, the pad is untouched.
Idle plus stalled walking is 14.4k ticks (7.6%) at risk 0.25. The two runs
agree on every share within a point, so the figures are stable, not one
run's luck.

What the locked and UI ticks are:

- **Pick-up viewer, 24.0k ticks (12.7%): engine-fixed.** 138 viewers, median 179
  and maximum 185 ticks. The engine runs Intro (about 83 ticks, no input),
  Message (about 32), Exit (about 65): `src/engine.rs:5574`
  (`step_pickup_view`). The bot cannot shorten it; only fewer pick-ups can.
- **Doors, 14.5k (7.7%):** 608 transitions, median 20 ticks. 609 room entries
  for 105 distinct rooms.
- **Uncontrolled, 11.6k (6.1%):** 1,145 stretches, median 4 ticks. Almost all
  are game scripts (the 154-tick event stretches in rooms 10D, 210, 506, 515;
  the 595-tick ending). Not the bot's.
- **Scenes, 7.3k:** films and skippables; the bot already skips them.

So 58% of the run is controllable, and 5.3% of the run is the bot standing
still while it could act. The rest of the avoidable time is not "idle" but is
lost walking, pressing and retrying (below).

## Where idle ticks go

Idle ticks by the goal the bot held:
door goals 4.0k (door:pursuing 2,610, door:toward 594, door:unvisited 459,
door:reached 362), item 1,826, item box 1,214, scripted `wait` 590, no goal 512,
interact 368, use 332, passage check 320, drawer 258.

Idle ticks by segment length (r25): 1 tick 806, 2-3 ticks 1,829, 4-9 ticks 846,
10-29 ticks 2,186, 30-99 ticks 2,598, 100+ ticks 1,695. The tail (30+ ticks,
4.3k, 43%) is almost all pressing sides that do nothing (cause 1 and 2 below)
plus one scripted 591-tick wait.

The log line before each idle stretch says what the bot was doing:
`at approach` of a door (708 stretches, 3.1k ticks), of an item (126, 1.3k),
`inventory: withdrew/deposited` (74, 1.2k: closing the box), `prompt: answering
yes` (25, 0.4k), `goal done: interact` (27, 0.6k: the 591-tick scripted wait).
Full tables: appendix.

## Ranked causes and fixes

Ticks are risk 0.25 unless noted. "Expected" is a measured upper bound, not a
promise.

### 1. A blocked or dead approach costs the same again for every remaining side (3k to 5.8k ticks)

Failed approach sides: door `blocked` 937 (r25) / **3,585** (r35); door `pressed,
nothing` 1,621 (14 sides of about 110 ticks); door `unreachable` 597.

Cause:

- `walk_to` reports `blocked` after `MAX_REPLANS` (3) replans of
  `STUCK_TICKS` (12) each, about 50 to 100 ticks of pushing
  (`src/engine/bot.rs:1271-1276`, `2239-2262`). `blocked_goal` is per goal point.
- `press_zone` then moves to the next side (`mods/bot/bot/nav.lua:363-369`) and
  walks into the same obstruction. Room 307 to 30B: 9 sides blocked from
  (4462,13584), (4026,13584), (4474,13584), ... all on the same z, 100 ticks
  each, then the door is marked `every approach failed` (log tick 140937 to
  141881, 944 ticks; r35: three such visits, 3,585 ticks).
- The door pattern has no early exit: the `settled` rule excludes it
  (`nav.lua:416-420`), so each side runs its full `side = 4*SECOND-10` (110
  ticks, `planner.lua:1192`). Room 301 door 1 to 302: 7 sides of 110 ticks at
  (3150,12350)-ish, 770 ticks, then marked (tick 79216 to 80243).

Fix:

- Remember where a side was blocked. When a later side is blocked within about
  400 units of an earlier blocked spot in the same job, fail the job at once
  with `blocked at (x,z)` and mark the door. Also feed the spot to the nav as a
  keep-out so the next plan routes around, or stops offering the door.
- Give the door pattern a settle: after two press cycles with no message, no
  transition and no `reach` overlap, call the side dead after about 30 ticks
  (the locked-door message arrives within 10).

Expected: about 2.5k at risk 0.25, about 5.8k at risk 0.35 (1.3 to 3%).
Risk: a door that needs a longer wait (a scripted event holding it) is skipped
early; the existing `frontier empty; retrying` loop (`planner.lua:2424-2486`)
retries it later, so it costs a revisit, not a deadlock. The root cause of the
307 obstruction (the static map says passable, the engine disagrees at
z=13584) is open: it deserves a one-room trace.

### 2. The "inventory full" pick-up viewer is not read as a failure (up to 3.3k ticks, 1.7%)

When a pick-up does not fit, the engine opens the viewer with message 194
(`MESSAGE_INVENTORY_FULL`, 0xC2) and closes without taking the item. Nothing in
the Lua reads message 194 (`grep` finds none); `scene.lua:102-134` just closes
the viewer. The job then presses again.

- Room 219, interact 4 (battery from the shelf): 4 sides, each ~577 ticks (two
  viewer loops of ~290), 2,333 ticks, then `goal timed out` for 2,401 (tick
  113479 to 115880). On the next entry the same shelf took 200 ticks.
- Room 118, item 0x0C slot 4: two 194 viewers of ~215 ticks before a side took
  it (455 ticks).
- 10 such viewers in the r25 run; none in r35 (a different route).

Fix: in `BotScene.handle`'s pick-up branch record `s.full_seen = tick` when
`scene.message == 194` (or whenever the message is the inventory-full id). In
`press_zone` (`nav.lua`), a `check` that fails the job with `inventory full`
when `full_seen` is newer than the job's start; the planner already knows how
to park an `inventory full` mark until the inventory changes
(`planner.lua:272`, `377`) and to run the box deposit first. The interact goals
(`planner.lua:1578-1589`) need the same gate that items have
(`pickup_fits`, `planner.lua:1357`); 219's shelf item has a `bank == ITEM_BANK`
fact the planner does not test.

Expected: about 2.8k at risk 0.25 (219: 2.3k; 118: 0.25k, plus the repeat
viewers) and 0 at 0.35. Risk: low; the fail path already exists.

### 3. Pressing from a standstill: idle at the approach (about 4.5k idle, 2.5k to 3k saveable)

Door: arrival to transition start is median 4, mean 6.2 ticks over 575
doors (3.6k total). Item: median 15 ticks, 1.7k over 95 pick-ups. These show as
1-to-9-tick idle stretches.

Cause: `press_zone` walks to the point (`tolerance = APPROACH/2-30`), stops,
faces, then presses on `tick % 4 < 2` (`nav.lua:398-410`); the item pattern
adds `gap = 20` and waits for the `action_ready` release window
(`scene.lua:188`). A player holds forward and presses as the probe crosses the
zone edge.

Fix: when `walk_to` is within about one stride of the point and the probe
already overlaps the zone, set `up` and press action in the same tick instead
of stopping first (the door probe does not need a standstill; the leaning
branch already holds `up`). Press on the arrival tick rather than waiting for
the `% 4` phase. Expected: door 575 x about 3 = 1.7k, items 95 x about 8 = 0.8k.
Risk: a press before facing locks wrong; keep the existing `facing.status ==
"facing"` gate and only drop the full stop.

### 4. Failed pressing sides on items and uses (about 1.5k)

`pressed, nothing` on `use` 442 ticks (305 Wolf Medal: a 105-tick scripted
wait counted, then a 90-tick side of `near approach` that pressed from beside
the point), passage check 348 (20A, 202 ticks), item 455 (cause 2). The `hold`
pattern and `near approach` branch (`nav.lua:340-362`) press from where the walk
stopped; a side that gets nothing in 90 ticks is retried from a new point.
Fix: let the `use`/`hold` probe test `reach_short` before spending the side,
and skip to the next side the moment the probe lies outside the zone
(`BotNav.within_reach`, already computed). Expected: about 0.4k. Low priority.

### 5. Stalled walking: 4.5k ticks (2.4%), not yet diagnosed

Up or down held, position unchanged. By goal: push 1,539, passage check 693,
door:pursuing 601, stair 505, climb 270, interact 199, item 146. By room: 40D
772, 207 678, 301 391, 507 378, 11B 260, 400 183. The push rooms (40D, 207) are
expected to stall some (the crate resists), and stair and climb entries lock the
player for their animation; the rest is real shoving against geometry (301,
507). The stats cannot split these. Next step: record the player `state` and
`locked` byte in the stalled run (one extra field in `stalled_runs`), then
bucket "stall while a locked action runs" apart from "stall against
geometry". Until then no fix is proposed.

### Out of reach of the bot, or not worth it

- **Pick-up viewer 24k:** only fewer pick-ups help. The run makes 138 pick-ups
  and then 43 deposits and 26 withdrawals (39 box visits, 6.4k ticks in the box
  UI plus about 4k of walking and 14 box goals paused for a typewriter). A
  pick-up costs about 180 (viewer) + 100 (approach) + its share of the box
  trip, so each pick-up of an item the route never needs costs about 300 to 400
  ticks. A "needed or ammunition or heal" filter would cut perhaps 20 to 40
  pick-ups (6k to 14k ticks). It changes the game the bot plays (less
  ammunition), so it is a policy call, not a bug fix; measure with the bench.
- **Doors 14.5k, 609 room entries for 105 rooms:** 30 bounces under 300 ticks
  and 99 under 900 (`scripts/bot-bench-summary.py`). Route ordering, not
  idling. Rooms with the most entries: 10A 28, 10B 28, 207 24, 400 22.
- **Scripted waits:** room 20C `wait for flag 1:0x25 (Barry's rope)` is a
  591-tick game timer; 301's door stand of 222 ticks is cause 1.
- **Turning 12.9k (6.8%):** 3,880 stretches; 2,767 are 1 to 2 ticks (corners).
  `steer.rs` already prefers arcs to in-place turns (`steer.rs:45-47`, tuned
  on the bench). 6.7k are turn stretches inside walks. Not a retry problem.

## Retry summary (r25)

- **Marks:** 178, of which 54 repeat attempts on 42 keys. Of the repeats 36 are
  `inventory full` re-evaluations on room entry, which cost no ticks (planner
  skip, `planner.lua:2200`); 62 first marks are the same kind. The ticks sit in
  4b-4c, not in the mark count.
- **Failed approach sides:** 7.7k ticks: interact 2.3k (cause 2), door 3.2k
  (cause 1), stair 0.5k, item 0.7k, use 0.6k, passage check 0.4k.
- **Goals that ended in a mark:** 14.9k ticks, mostly doors marked after the
  walk (unvisited 3.7k, pursuing 3.1k, reached 3.1k, toward 1.7k), interact 2.4k
  (the 219 shelf). A marked door goal's cost is the walk to the room's far side
  plus its failed sides; cause 1 covers the repeatable part.
- **`goal timed out`:** 2 (interact 219/4 at 2,401 ticks, stair 20A/2 at 451).
- **Other signals:** `near approach` 74, `leaning in` 53, replanned after a
  scene moved the player 33, `loop count` (room re-entered without progress)
  28, `frontier empty; retrying` 18.
- **Menu / box:** 112 START opens, 177 viewer or box opens, 43 deposits, 26
  withdrawals, 52 combines (2.8k ticks), 31 item uses. Box goals: 39 visits,
  25 of them goal restarts with the same label (14 paused for a save).

## Method notes and limits

- The Rust stats module already bucketed every tick; it did not say what the
  bot was doing. The new `timeline` (run-length rows `[start, length, bucket]`)
  and `stalled_runs` fields let `scripts/bot-idle-report.py` join each tick to
  the goal active at that tick from the log's `goal:`, `goal done:`, `goal
  paused:` and `mark` lines.
- A failed side's cost is the ticks since the job's previous log line, so the
  walking to the point counts. A mark's cost is the goal's age when marked.
- A paused or superseded goal is not always waste (a goal paused for a save
  resumes). Door goals replaced by the next door goal are excluded from 4c.
- Runs are deterministic: re-running a risk setting gives the same tick count.

# Appendix: full output of `scripts/bot-idle-report.py`

## Risk 0.25

Outcome `finished`, **189451** ticks, 609 room entries (105 unique), 21 saves, 1 death(s).

### 1. Tick budget

| bucket | ticks | share |
|---|---|---|
| **input locked by the game** | **33451** | **17.7%** |
| &nbsp;&nbsp;doors | 14505 | 7.7% |
| &nbsp;&nbsp;scenes | 7304 | 3.9% |
| &nbsp;&nbsp;uncontrolled | 11642 | 6.1% |
| &nbsp;&nbsp;pickup | 0 | 0.0% |
| **bot-driven menu / viewer / save UI** | **45142** | **23.8%** |
| &nbsp;&nbsp;saving | 4261 | 2.2% |
| &nbsp;&nbsp;menu | 9771 | 5.2% |
| &nbsp;&nbsp;item_box | 6368 | 3.4% |
| &nbsp;&nbsp;pickup_viewer | 23994 | 12.7% |
| &nbsp;&nbsp;examine | 748 | 0.4% |
| **player in control** | **110858** | **58.5%** |
| &nbsp;&nbsp;walking | 75953 | 40.1% |
| &nbsp;&nbsp;turning | 12855 | 6.8% |
| &nbsp;&nbsp;combat | 12090 | 6.4% |
| &nbsp;&nbsp;waiting | 9960 | 5.3% |

Of 110858 in-control ticks: walking 75953 (of which **4483** left the position unchanged: pushing into geometry), turning 12855, combat 12090, **waiting (idle) 9960**.

### 2. Ticks by goal kind and activity

| goal kind | ticks | walking | turning | combat | waiting | menu | item_box | pickup_viewer | saving | scenes | doors | uncontrolled | pickup | examine |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| door:pursuing | 59432 | 37646 | 3572 | 2241 | 2610 | 1138 |  | 183 | 585 | 744 | 7939 | 2774 |  |  |
| item | 30981 | 5047 | 1363 | 988 | 1826 | 1753 |  | 19281 |  | 229 |  | 494 |  |  |
| door:toward | 19237 | 9007 | 2643 | 1440 | 594 | 266 |  | 102 |  | 191 | 3966 | 1028 |  |  |
| door:unvisited | 14210 | 5928 | 930 | 3200 | 459 | 802 |  | 183 | 195 | 426 | 1307 | 780 |  |  |
| item box | 10041 | 660 | 215 |  | 1214 |  | 6164 |  |  |  |  | 1788 |  |  |
| interact | 10028 | 2516 | 518 | 471 | 368 | 308 |  | 1892 |  | 2372 | 138 | 1445 |  |  |
| door:reached | 9385 | 3858 | 399 | 1442 | 362 | 502 |  | 912 |  | 676 | 900 | 334 |  |  |
| (no goal) | 7496 | 634 | 163 | 1 | 512 |  | 204 |  | 3481 | 1386 | 104 | 1011 |  |  |
| use | 5811 | 1826 | 390 | 441 | 332 | 1391 |  | 97 |  | 554 | 43 | 737 |  |  |
| push | 5714 | 4438 | 657 | 287 | 67 |  |  |  |  | 21 |  | 244 |  |  |
| passage check | 3226 | 1652 | 990 |  | 320 | 54 |  |  |  | 119 |  | 91 |  |  |
| combine | 2981 |  |  |  | 75 | 2793 |  |  |  |  |  | 113 |  |  |
| stair | 2392 | 1403 | 582 |  | 37 |  |  |  |  |  |  | 370 |  |  |
| fight | 2166 | 246 | 45 | 1432 | 20 | 358 |  |  |  | 21 |  | 44 |  |  |
| drawer | 2054 | 221 | 66 | 147 | 258 | 153 |  | 731 |  | 328 |  | 150 |  |  |
| climb | 1067 | 507 | 213 |  | 43 | 12 |  | 173 |  | 91 |  | 28 |  |  |
| open | 912 |  |  |  | 6 | 148 |  |  |  |  |  | 10 |  | 748 |
| wait | 729 |  |  |  | 590 | 12 |  | 94 |  | 33 |  |  |  |  |
| fetch | 533 | 46 | 30 |  | 25 | 81 |  | 346 |  |  |  | 5 |  |  |
| door:step | 368 | 18 | 26 |  | 226 |  |  |  |  |  | 21 | 77 |  |  |
| step | 283 | 143 | 23 |  | 1 |  |  |  |  | 56 | 26 | 34 |  |  |
| door:untraversed | 233 | 59 | 27 |  | 7 |  |  |  |  | 18 | 41 | 81 |  |  |

Stalled walking by goal kind: push 1539, passage check 693, door:pursuing 601, stair 505, climb 270, interact 199, item 146, door:unvisited 138

Stalled walking by room: 40D 772, 207 678, 301 391, 507 378, 11B 260, 400 183, 306 163, 203 143, 202 110, 411 92

### 3. Idle (waiting) ticks

| goal kind | idle ticks | share of idle |
|---|---|---|
| door:pursuing | 2610 | 26.2% |
| item | 1826 | 18.3% |
| item box | 1214 | 12.2% |
| door:toward | 594 | 6.0% |
| wait | 590 | 5.9% |
| (no goal) | 512 | 5.1% |
| door:unvisited | 459 | 4.6% |
| interact | 368 | 3.7% |
| door:reached | 362 | 3.6% |
| use | 332 | 3.3% |
| passage check | 320 | 3.2% |
| drawer | 258 | 2.6% |

Waiting segments by the last non-scene log line before them:

| log line before the wait | segments | ticks |
|---|---|---|
| door slot N -> N (..)): at approach N/N (..) | 316 | 1449 |
| item N slot N: at approach N/N (..) | 126 | 1337 |
| door slot N -> N (..), N door(..)): at approach N/N (..) | 264 | 1039 |
| inventory: withdrew N | 44 | 733 |
| door slot N -> N (..): at approach N/N (..) | 128 | 642 |
| goal done: interact N/N slot N (..) | 27 | 632 |
| inventory: deposited N | 30 | 496 |
| prompt: answering yes (..) | 25 | 396 |
| damage health N -> N pos=(..) state=N grabbed=false nearest=slot=N id= | 18 | 208 |
| hurt by slot=N id=N: fight it on the next pass | 18 | 207 |
| interact N/N slot N (..): at approach N/N (..) | 45 | 201 |
| use N at slot N (..): near approach N/N (..) from (..) | 6 | 191 |

Waiting ticks by segment length: 1: 806, 2-3: 1829, 4-9: 846, 10-29: 2186, 30-99: 2598, 100+: 1695

Longest waits (1983 segments in all):

| tick | room | ticks | goal | log line before |
|---|---|---|---|---|
| 103969 | 20C | 591 | (no goal) | goal done: interact N/N slot N (..) |
| 80262 | 301 | 222 | door slot 1 -> 302 (step out to re-enter 301) | door slot N -> N (..): at approach N/N (..) |
| 55299 | 20A | 133 | passage check slot 2 | prompt: answering yes (..) |
| 124961 | 301 | 111 | door slot 1 -> 302 (pursuing use 0x27 in 302 (the battery fo | door slot N -> N (..), N door(..)): at approach N/N (..) |
| 46764 | 301 | 109 | door slot 1 -> 302 (unvisited room) | door slot N -> N (..): at approach N/N (..) |
| 79218 | 301 | 109 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 124692 | 301 | 109 | door slot 1 -> 302 (pursuing use 0x27 in 302 (the battery fo | door slot N -> N (..), N door(..)): at approach N/N (..) |
| 79490 | 301 | 108 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 79800 | 301 | 102 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 80070 | 301 | 101 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 79355 | 301 | 98 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 124832 | 301 | 96 | door slot 1 -> 302 (pursuing use 0x27 in 302 (the battery fo | door slot N -> N (..), N door(..)): at approach N/N (..) |

`turning` ticks by goal kind: door:pursuing 3572, door:toward 2643, item 1363, passage check 990, door:unvisited 930, push 657

### 4. Retries

### 4a. Marks (an AOT the planner gave up on for now)

| kind: reason | first marks | repeat marks (attempt>1) | goal ticks before first | goal ticks before repeats |
|---|---|---|---|---|
| item: inventory full | 62 | 36 | 4385 | 2941 |
| item: avoid:  the plant fountains reach the herb unti | 6 | 0 | 2532 | 0 |
| interact: goal timed out | 1 | 0 | 2401 | 0 |
| item: zone at the room origin: a scripted handover | 15 | 0 | 2284 | 0 |
| door: every approach failed | 7 | 0 | 2180 | 0 |
| door: a room event made the door a message (R) | 10 | 2 | 1369 | 347 |
| door: locked for good | 4 | 0 | 715 | 0 |
| stair: goal timed out | 1 | 0 | 451 | 0 |
| item: avoid:  the plant fountains stand across the wa | 1 | 0 | 422 | 0 |
| interact: not ready for the fight: no heal held and the box holds 0x4A | 1 | 0 | 422 | 0 |
| door: avoided: the lift back up to R: the lab needs nothing from b | 1 | 0 | 263 | 0 |
| stair: every approach failed | 4 | 6 | 32 | 158 |
| door: avoided: the door to R is always message R | 1 | 0 | 162 | 0 |
| item: every approach failed | 6 | 9 | 44 | 106 |
| use: use not live on arrival | 1 | 0 | 132 | 0 |
| item: a second zone of the item taken at slot 1 | 0 | 1 | 0 | 132 |
| item: avoid:  the shotgun rack springs the 115 ceilin | 1 | 0 | 56 | 0 |
| item: avoid:  Barry stands in front of the fireplace  | 1 | 0 | 0 | 0 |
| interact: not ready for the fight: health 48/96 and no heal held | 1 | 0 | 0 | 0 |

Marks: 178 (54 repeats); 42 keys marked more than once.

### 4b. Approach sides that failed (ticks since the job's previous event)

| goal kind | outcome of the side | sides | ticks lost |
|---|---|---|---|
| interact | pressed, nothing | 4 | 2333 |
| door | pressed, nothing | 14 | 1621 |
| door | blocked | 9 | 937 |
| door | unreachable | 57 | 597 |
| stair | unreachable | 14 | 506 |
| item | pressed, nothing | 1 | 455 |
| use | pressed, nothing | 2 | 442 |
| passage check | pressed, nothing | 3 | 348 |
| item | unreachable | 93 | 223 |
| use | blocked | 1 | 166 |
| passage check | unreachable | 50 | 54 |
| use | unreachable | 12 | 12 |
| interact | unreachable | 2 | 2 |

Costliest single failed sides:

| tick | room | ticks | job | outcome |
|---|---|---|---|---|
| 115876 | 219 | 604 | interact 1/1 slot 4 (take the battery from the shelf) | pressed, nothing |
| 115272 | 219 | 577 | interact 1/1 slot 4 (take the battery from the shelf) | pressed, nothing |
| 114658 | 219 | 576 | interact 1/1 slot 4 (take the battery from the shelf) | pressed, nothing |
| 114082 | 219 | 576 | interact 1/1 slot 4 (take the battery from the shelf) | pressed, nothing |
| 107766 | 118 | 455 | item 0x0C slot 4 | pressed, nothing |
| 160823 | 305 | 281 | use 0x24 at slot 6 (the Wolf Medal into the fountain) | pressed, nothing |
| 109103 | 204 | 245 | door slot 0 -> 203 (toward the 201 -> 215 door, 3 door(s)) | unreachable |
| 55357 | 20A | 202 | passage check slot 2 | pressed, nothing |
| 80243 | 301 | 190 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | pressed, nothing |
| 100761 | 301 | 189 | stair check slot 3 | unreachable |
| 53857 | 301 | 189 | stair check slot 3 | unreachable |
| 158179 | 30F | 166 | use 0x1E at slot 6 (the hex crank: turn the passages) | blocked |

### 4c. Goals that ended without `goal done` (ticks spent on them)

| goal kind | ended by | goals | ticks |
|---|---|---|---|
| item box | paused | 14 | 4034 |
| door:unvisited | marked | 17 | 3709 |
| door:pursuing | marked | 11 | 3055 |
| door:reached | marked | 20 | 3054 |
| interact | marked | 1 | 2401 |
| door:toward | marked | 11 | 1698 |
| interact | superseded | 4 | 1125 |
| passage check | marked | 9 | 511 |
| use | superseded | 1 | 509 |
| item | marked | 16 | 282 |
| stair | marked | 2 | 130 |
| step | paused | 1 | 115 |
| combine | paused | 1 | 72 |
| step | superseded | 1 | 50 |
| door:reached | paused | 1 | 6 |

Goal ticks: 70235 in goals that finished, 20751 in goals that did not (a pause for a menu resumes later, so not all of that is waste).

### 4d. Goals started again with the same label (repeat starts, ticks in them)

| goal kind | repeat starts | ticks |
|---|---|---|
| item | 42 | 9694 |
| door:pursuing | 51 | 7975 |
| item box | 25 | 7412 |
| passage check | 21 | 2382 |
| stair | 12 | 1938 |
| door:toward | 11 | 1021 |
| combine | 7 | 882 |
| door:unvisited | 4 | 409 |
| interact | 2 | 369 |
| step | 1 | 118 |
| door:reached | 1 | 6 |

### 4e. Other retry signals (counts)

| signal | count |
|---|---|
| near approach (pressed from beside the point) | 74 |
| leaning in (probe fell short) | 53 |
| approaches replanned after a scene moved the player | 33 |
| loop count (room re-entered without progress) | 29 |
| frontier empty:  retrying do | 13 |
| frontier empty:  retrying it | 5 |
| frontier empty:  waiting for | 4 |
| frontier empty:  stepping ou | 2 |
| frontier empty:  retrying st | 2 |
| frontier empty:  retrying in | 1 |

Stalled walking (pad up/down held, position unchanged): **4483** ticks (2.4% of the run).

Menu: 112 START opens, 177 viewer/box opens, 40881 ticks open, 406 cursor moves, 31 item uses, 43 deposits, 26 withdrawals, 52 combines.


## Risk 0.35

Outcome `finished`, **178059** ticks, 558 room entries (105 unique), 19 saves, 1 death(s).

### 1. Tick budget

| bucket | ticks | share |
|---|---|---|
| **input locked by the game** | **31377** | **17.6%** |
| &nbsp;&nbsp;doors | 13445 | 7.6% |
| &nbsp;&nbsp;scenes | 6767 | 3.8% |
| &nbsp;&nbsp;uncontrolled | 11165 | 6.3% |
| &nbsp;&nbsp;pickup | 0 | 0.0% |
| **bot-driven menu / viewer / save UI** | **40261** | **22.6%** |
| &nbsp;&nbsp;saving | 3843 | 2.2% |
| &nbsp;&nbsp;menu | 9217 | 5.2% |
| &nbsp;&nbsp;item_box | 4828 | 2.7% |
| &nbsp;&nbsp;pickup_viewer | 21625 | 12.1% |
| &nbsp;&nbsp;examine | 748 | 0.4% |
| **player in control** | **106421** | **59.8%** |
| &nbsp;&nbsp;walking | 75301 | 42.3% |
| &nbsp;&nbsp;turning | 12603 | 7.1% |
| &nbsp;&nbsp;combat | 9323 | 5.2% |
| &nbsp;&nbsp;waiting | 9194 | 5.2% |

Of 106421 in-control ticks: walking 75301 (of which **4513** left the position unchanged: pushing into geometry), turning 12603, combat 9323, **waiting (idle) 9194**.

### 2. Ticks by goal kind and activity

| goal kind | ticks | walking | turning | combat | waiting | menu | item_box | pickup_viewer | saving | scenes | doors | uncontrolled | pickup | examine |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| door:pursuing | 53366 | 34967 | 3587 | 1046 | 2380 | 436 |  | 183 | 602 | 669 | 7216 | 2280 |  |  |
| item | 29325 | 4850 | 1318 | 891 | 1717 | 1723 |  | 17999 |  | 205 |  | 622 |  |  |
| door:toward | 18560 | 8851 | 2425 | 1095 | 509 | 257 |  | 281 |  | 395 | 3597 | 1150 |  |  |
| door:unvisited | 12864 | 6413 | 922 | 1908 | 478 | 575 |  | 183 |  | 417 | 1287 | 681 |  |  |
| door:reached | 11410 | 6078 | 521 | 977 | 361 | 511 |  | 912 |  | 632 | 920 | 498 |  |  |
| item box | 8271 | 566 | 237 |  | 1084 |  | 4796 |  |  |  |  | 1588 |  |  |
| interact | 7382 | 2462 | 490 | 300 | 185 | 151 |  | 451 |  | 1823 | 138 | 1382 |  |  |
| (no goal) | 7230 | 749 | 187 | 1 | 441 | 13 | 32 | 176 | 3241 | 1311 | 136 | 943 |  |  |
| use | 5774 | 1928 | 392 | 248 | 353 | 1413 |  | 97 |  | 554 | 43 | 746 |  |  |
| push | 5713 | 4438 | 656 | 287 | 67 |  |  |  |  | 21 |  | 244 |  |  |
| combine | 3421 |  |  |  | 103 | 3177 |  |  |  |  |  | 141 |  |  |
| fight | 3039 | 268 | 56 | 2082 | 27 | 481 |  |  |  | 57 |  | 68 |  |  |
| passage check | 2608 | 1246 | 833 |  | 289 | 54 |  |  |  | 119 |  | 67 |  |  |
| drawer | 2417 | 214 | 74 | 488 | 258 | 175 |  | 730 |  | 328 |  | 150 |  |  |
| stair | 2388 | 1402 | 583 |  | 37 |  |  |  |  |  |  | 366 |  |  |
| climb | 1066 | 506 | 214 |  | 43 | 12 |  | 173 |  | 90 |  | 28 |  |  |
| open | 910 |  |  |  | 6 | 146 |  |  |  |  |  | 10 |  | 748 |
| wait | 729 |  |  |  | 590 | 12 |  | 94 |  | 33 |  |  |  |  |
| fetch | 533 | 46 | 30 |  | 25 | 81 |  | 346 |  |  |  | 5 |  |  |
| door:step | 365 | 17 | 25 |  | 225 |  |  |  |  |  | 21 | 77 |  |  |
| step | 283 | 143 | 23 |  | 1 |  |  |  |  | 56 | 26 | 34 |  |  |
| door:untraversed | 233 | 59 | 27 |  | 7 |  |  |  |  | 18 | 41 | 81 |  |  |

Stalled walking by goal kind: push 1539, passage check 573, door:pursuing 529, stair 506, climb 269, item 247, interact 208, use 176

Stalled walking by room: 40D 770, 207 565, 301 390, 507 377, 11B 256, 203 173, 306 163, 400 122, 202 112, 411 92

### 3. Idle (waiting) ticks

| goal kind | idle ticks | share of idle |
|---|---|---|
| door:pursuing | 2380 | 25.9% |
| item | 1717 | 18.7% |
| item box | 1084 | 11.8% |
| wait | 590 | 6.4% |
| door:toward | 509 | 5.5% |
| door:unvisited | 478 | 5.2% |
| (no goal) | 441 | 4.8% |
| door:reached | 361 | 3.9% |
| use | 353 | 3.8% |
| passage check | 289 | 3.1% |
| drawer | 258 | 2.8% |
| door:step | 225 | 2.4% |

Waiting segments by the last non-scene log line before them:

| log line before the wait | segments | ticks |
|---|---|---|
| door slot N -> N (..)): at approach N/N (..) | 277 | 1380 |
| item N slot N: at approach N/N (..) | 116 | 1224 |
| door slot N -> N (..), N door(..)): at approach N/N (..) | 249 | 985 |
| door slot N -> N (..): at approach N/N (..) | 132 | 751 |
| goal done: interact N/N slot N (..) | 28 | 633 |
| inventory: withdrew N | 38 | 631 |
| inventory: deposited N | 26 | 429 |
| prompt: answering yes (..) | 26 | 399 |
| use N at slot N (..): near approach N/N (..) from (..) | 6 | 191 |
| waiting for the use probe: broken shotgun on the rack | 5 | 176 |
| item N slot N: near approach N/N (..) from (..) | 19 | 145 |
| hurt by slot=N id=N: fight it on the next pass | 9 | 126 |

Waiting ticks by segment length: 1: 777, 2-3: 1739, 4-9: 757, 10-29: 1723, 30-99: 2393, 100+: 1805

Longest waits (1856 segments in all):

| tick | room | ticks | goal | log line before |
|---|---|---|---|---|
| 107713 | 20C | 591 | (no goal) | goal done: interact N/N slot N (..) |
| 82970 | 301 | 221 | door slot 1 -> 302 (step out to re-enter 301) | door slot N -> N (..): at approach N/N (..) |
| 55039 | 20A | 133 | passage check slot 2 | prompt: answering yes (..) |
| 26016 | 208 | 111 | door slot 0 -> 209 (reached the 208 -> 209 door) | door slot N -> N (..): at approach N/N (..) |
| 117609 | 301 | 111 | door slot 1 -> 302 (pursuing use 0x27 in 302 (the battery fo | door slot N -> N (..), N door(..)): at approach N/N (..) |
| 46826 | 301 | 109 | door slot 1 -> 302 (unvisited room) | door slot N -> N (..): at approach N/N (..) |
| 82010 | 301 | 109 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 117340 | 301 | 109 | door slot 1 -> 302 (pursuing use 0x27 in 302 (the battery fo | door slot N -> N (..), N door(..)): at approach N/N (..) |
| 82303 | 301 | 108 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 82592 | 301 | 102 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 82858 | 301 | 101 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |
| 82168 | 301 | 98 | door slot 1 -> 302 (pursuing item 0x44 in 304, 2 door(s)) | door slot N -> N (..)): at approach N/N (..) |

`turning` ticks by goal kind: door:pursuing 3587, door:toward 2425, item 1318, door:unvisited 922, passage check 833, push 656

### 4. Retries

### 4a. Marks (an AOT the planner gave up on for now)

| kind: reason | first marks | repeat marks (attempt>1) | goal ticks before first | goal ticks before repeats |
|---|---|---|---|---|
| item: inventory full | 50 | 29 | 7484 | 2053 |
| door: every approach failed | 4 | 2 | 1777 | 2159 |
| item: avoid:  the plant fountains reach the herb unti | 6 | 0 | 2328 | 0 |
| door: a room event made the door a message (R) | 12 | 3 | 1697 | 406 |
| item: zone at the room origin: a scripted handover | 13 | 2 | 1483 | 532 |
| item: a second zone of the item taken at slot 1 | 0 | 1 | 0 | 474 |
| stair: goal timed out | 1 | 0 | 451 | 0 |
| item: avoid:  the plant fountains stand across the wa | 1 | 0 | 388 | 0 |
| interact: not ready for the fight: no heal held and the box holds 0x4A | 1 | 0 | 360 | 0 |
| door: avoided: the lift back up to R: the lab needs nothing from b | 1 | 0 | 265 | 0 |
| door: locked for good | 4 | 0 | 227 | 0 |
| item: every approach failed | 7 | 10 | 54 | 112 |
| door: avoided: the door to R is always message R | 1 | 0 | 162 | 0 |
| stair: every approach failed | 3 | 3 | 18 | 132 |
| use: use not live on arrival | 1 | 0 | 132 | 0 |
| item: avoid:  the shotgun rack springs the 115 ceilin | 1 | 0 | 56 | 0 |
| item: avoid:  Barry stands in front of the fireplace  | 1 | 0 | 0 | 0 |
| interact: not ready for the fight: health 44/96 and no heal held | 1 | 0 | 0 | 0 |

Marks: 158 (50 repeats); 36 keys marked more than once.

### 4b. Approach sides that failed (ticks since the job's previous event)

| goal kind | outcome of the side | sides | ticks lost |
|---|---|---|---|
| door | blocked | 33 | 3585 |
| door | pressed, nothing | 15 | 1651 |
| item | unreachable | 105 | 581 |
| stair | unreachable | 14 | 502 |
| door | unreachable | 46 | 459 |
| use | pressed, nothing | 2 | 442 |
| passage check | pressed, nothing | 3 | 348 |
| use | blocked | 1 | 166 |
| passage check | unreachable | 26 | 22 |
| use | unreachable | 12 | 12 |

Costliest single failed sides:

| tick | room | ticks | job | outcome |
|---|---|---|---|---|
| 71077 | 114 | 432 | item 0x44 slot 4 | unreachable |
| 149158 | 305 | 281 | use 0x24 at slot 6 (the Wolf Medal into the fountain) | pressed, nothing |
| 132893 | 307 | 252 | door slot 2 -> 30B (unvisited room) | blocked |
| 130097 | 307 | 252 | door slot 2 -> 30B (reached the 307 -> 30B door) | blocked |
| 135444 | 307 | 239 | door slot 2 -> 30B (reached the 307 -> 30B door) | blocked |
| 55097 | 20A | 202 | passage check slot 2 | pressed, nothing |
| 103713 | 301 | 189 | stair check slot 3 | unreachable |
| 53597 | 301 | 189 | stair check slot 3 | unreachable |
| 146515 | 30F | 166 | use 0x1E at slot 6 (the hex crank: turn the passages) | blocked |
| 3291 | 10F | 161 | use 0x23 at slot 13 (music score at the piano) | pressed, nothing |
| 171252 | 510 | 143 | door slot 1 -> 511 (unvisited room) | unreachable |
| 135987 | 307 | 142 | door slot 2 -> 30B (reached the 307 -> 30B door) | blocked |

### 4c. Goals that ended without `goal done` (ticks spent on them)

| goal kind | ended by | goals | ticks |
|---|---|---|---|
| door:unvisited | marked | 19 | 4461 |
| door:reached | marked | 16 | 4374 |
| door:pursuing | marked | 12 | 3525 |
| item box | paused | 8 | 2065 |
| interact | superseded | 4 | 1051 |
| door:toward | marked | 10 | 950 |
| door:reached | paused | 2 | 846 |
| item | marked | 18 | 640 |
| use | superseded | 1 | 509 |
| passage check | marked | 5 | 475 |
| door:pursuing | paused | 3 | 133 |
| stair | marked | 2 | 126 |
| step | paused | 1 | 115 |
| item | paused | 1 | 91 |
| step | superseded | 1 | 50 |
| door:toward | paused | 1 | 29 |
| door:unvisited | paused | 1 | 2 |

Goal ticks: 69266 in goals that finished, 19442 in goals that did not (a pause for a menu resumes later, so not all of that is waste).

### 4d. Goals started again with the same label (repeat starts, ticks in them)

| goal kind | repeat starts | ticks |
|---|---|---|
| item | 41 | 9455 |
| door:pursuing | 47 | 6988 |
| item box | 21 | 5592 |
| stair | 12 | 1938 |
| passage check | 15 | 1774 |
| door:toward | 11 | 1390 |
| door:unvisited | 4 | 1325 |
| door:reached | 2 | 1182 |
| combine | 8 | 960 |
| step | 1 | 118 |

### 4e. Other retry signals (counts)

| signal | count |
|---|---|
| near approach (pressed from beside the point) | 66 |
| leaning in (probe fell short) | 44 |
| approaches replanned after a scene moved the player | 23 |
| loop count (room re-entered without progress) | 22 |
| frontier empty:  retrying do | 11 |
| frontier empty:  retrying it | 5 |
| frontier empty:  waiting for | 4 |
| frontier empty:  retrying st | 2 |
| frontier empty:  stepping ou | 2 |

Stalled walking (pad up/down held, position unchanged): **4513** ticks (2.5% of the run).

Menu: 104 START opens, 158 viewer/box opens, 36418 ticks open, 360 cursor moves, 33 item uses, 42 deposits, 27 withdrawals, 49 combines.

