# Bot routes

One driver script per route through the game. Each one boots a new game,
attaches the bot with the story parameters that steer it down the route
(`mods/bot/params/re1.xml` declares them) and lets it play to the ending.

```sh
# Watch it play, in a window at the game's own pace. The host keys work
# (F2/F3 game speed, F12 screenshot, ...), and Escape opens the in-game menu,
# options included: the bot waits until it closes.
arklay script mods/bot/routes/re1_jill_best_ending.lua

# As fast as the machine allows, with no window.
arklay script --headless mods/bot/routes/re1_jill_best_ending.lua

# From stdin.
arklay script --headless - < mods/bot/routes/re1_jill_best_ending.lua

# Another pack: an installed pack's id, or a path.
arklay script --headless mods/bot/routes/re1_jill_best_ending.lua path/to/re1.akpak

# Every route (or the ones named), in parallel, as a table.
scripts/bot-routes.py [-p PACK] [-s SEEDS] [ROUTE ...]
```

Run them from the repository root: the bot is the mod layer `mods/bot`.

| Argument | |
|---|---|
| `PACK` | The RE1 game pack: an installed pack's id, or a path to an `.akpak` (default `re1`, the installed RE1 pack, found under the data roots' `games/` folders as the launcher finds it). |
| `SEED` | A bot seed: it varies the game's random stream, the same way in every run with that seed. Empty or absent: the game's own stream. |
| `STATS` | A file to write the full report to, as `--bot-stats` does. |

A route prints one JSON line on stdout: its name, the `outcome`
(`finished`, `death` or `max_ticks`), the checks it `missed` and the
`score` (see `bot_stats` in `lua-types/script.lua`). The bot's log goes to
stderr. The exit status is 1 when the route did not end as named: each
script checks the ending and the story flags that mark its branch. The
`re1_jill_*` routes play Jill's game and the `re1_chris_*` routes Chris's.

Who reaches the helicopter decides the ending, so every route sets each
parameter that decides it and has an effect on its way. Its ending then
follows from its own parameters, not from the defaults: a missed ending is
the bot's failure, not the route's. The other parameters pick a scene on the
way and keep their defaults otherwise; none of them changes who escapes.

- **Jill's game:** Barry's fate is `rope`, `cave_q1`, `cave_q2` and
  `cave_exit`, Chris's is `chris`. Without the rope, 308's scene never plays
  and the cave parameters do nothing; after Yes, No or No, Yes, `cave_exit`
  does nothing.
- **Chris's game:** Rebecca's fate is `rebecca_meet`, `rebecca_along`,
  `richard` and `rebecca`, Jill's is `jill` (`params/re1.xml` says what each
  does). Rebecca is lost only to a hunter attack Chris abandons, in 101 or
  206. 206's is lost whatever the timing, so the routes that lose her take
  it (`rebecca_meet=richard richard=leave rebecca=leave`). 101's plays only
  when Chris comes back more than 180 seconds after her radio call, which no
  parameter can force, so no route loses her there. Yawn's bite, Rebecca's
  serum run after it and Plant 42's V-JOLT follow from the game state.

| Route | Parameters | Ending |
|---|---|---|
| `re1_jill_best_ending` | `rope=wait cave_q1=yes cave_q2=yes cave_exit=follow chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_barry_ending` | `rope=wait cave_q1=yes cave_q2=yes cave_exit=follow chris=leave` | 5: Jill and Barry |
| `re1_jill_chris_ending` | `rope=wait cave_q1=no cave_q2=yes chris=save` | 3: Jill and Chris |
| `re1_jill_bad_ending` | `rope=leave chris=leave` | 2: Jill alone |
| `re1_jill_richard_dies` | `richard=leave rope=wait cave_q1=yes cave_q2=yes cave_exit=follow chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_barry_with_jill` | `rope=wait cave_q1=yes cave_q2=no chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_barry_ahead_no_no` | `rope=wait cave_q1=no cave_q2=no cave_exit=follow chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_cave_backtrack` | `rope=wait cave_q1=yes cave_q2=yes cave_exit=backtrack chris=save` | 3: Jill and Chris |
| `re1_jill_vjolt` | `vjolt=yes rope=wait cave_q1=yes cave_q2=yes cave_exit=follow chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_chemical_first` | `barry_early=chemical rope=wait cave_q1=yes cave_q2=yes cave_exit=follow chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_sandwich` | `sandwich=yes rope=wait cave_q1=yes cave_q2=yes cave_exit=follow chris=save` | 7: Jill, Chris and Barry |
| `re1_jill_rope_leave` | `rope=leave chris=save` | 3: Jill and Chris |
| `re1_chris_best_ending` | `rebecca_meet=save_room rebecca_along=yes rebecca=save jill=save` | 6: Chris, Jill and Rebecca |
| `re1_chris_rebecca_ending` | `rebecca_meet=save_room rebecca_along=yes rebecca=save jill=leave` | 4: Chris and Rebecca |
| `re1_chris_jill_ending` | `rebecca_meet=richard richard=leave rebecca=leave jill=save` | 3: Chris and Jill |
| `re1_chris_alone` | `rebecca_meet=richard richard=leave rebecca=leave jill=leave` | 1: Chris alone |
| `re1_chris_richard_saved` | `rebecca_meet=richard richard=save rebecca=save jill=save` | 6: Chris, Jill and Rebecca |
| `re1_chris_rebecca_206_saved` | `rebecca_meet=richard richard=leave rebecca=save jill=save` | 6: Chris, Jill and Rebecca |
| `re1_chris_rebecca_save_room` | `rebecca_meet=save_room rebecca_along=no rebecca=save jill=save` | 6: Chris, Jill and Rebecca |
| `re1_chris_ceiling_trap` | `sandwich=yes rebecca_meet=save_room rebecca_along=yes rebecca=save jill=save` | 6: Chris, Jill and Rebecca |

The Chris routes are not played through yet: none completes on seeds 1
and 2. Each one now plays past the mansion's story steps (Richard's serum
or death, Rebecca's piano, the emblems, 115's ceiling trap) into the
courtyard, the caves or the guardhouse. Where they stop:

- 20C/21A: the bot keeps rerunning 20C's ladder event (AOT 7, the way down
  to 21A and on to the 200 lift) as a passage check, until the loop rule
  ends the run (`alone`, `jill_ending`, `rebecca_save_room`).
- 400: Rebecca, the player while Plant 42 holds Chris, gets no item box
  (403's is off for her) and keeps pressing the gate to 304, a message
  while she plays (`best_ending`, `rebecca_ending`).
- 40C: after the skipped intro Chris can stand outside the room's walls,
  pushed out the far side of a wall box. The cause is not pinned down.
- 401: a fight at the desk can wedge Chris where no path plans out.
- Early deaths: on unlucky seeds Chris runs out of rounds in the mansion
  and saves rarely, so four deaths since the last save end the run.

The routes play with the port's default options (quick turn and tactical
reload on). A new route is a copy of one of these with other `params`,
and, for a randomised pack, another `PACK`.
