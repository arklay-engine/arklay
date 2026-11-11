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
script checks the ending and the story flags that mark its branch.

| Route | Parameters | Ending |
|---|---|---|
| `re1_jill_best_ending` | the defaults | 7: Jill, Chris and Barry |
| `re1_jill_barry_ending` | `chris=leave` | 5: Jill and Barry |
| `re1_jill_chris_ending` | `cave_q1=no` | 3: Jill and Chris |
| `re1_jill_bad_ending` | `rope=leave chris=leave` | 2: Jill alone |
| `re1_jill_richard_dies` | `richard=leave` | 7 |
| `re1_jill_barry_with_jill` | `cave_q2=no` | 7 |
| `re1_jill_barry_ahead_no_no` | `cave_q1=no cave_q2=no` | 7 |
| `re1_jill_cave_backtrack` | `cave_exit=backtrack` | 3 |
| `re1_jill_vjolt` | `vjolt=yes` | 7 |
| `re1_jill_chemical_first` | `barry_early=chemical` | 7 |
| `re1_jill_sandwich` | `sandwich=yes` | 7 |
| `re1_jill_rope_leave` | `rope=leave` | 3 |

The routes play with the port's default options (quick turn and tactical
reload on). A new route is a copy of one of these with other `params`,
and, for a randomised pack, another `PACK`.
