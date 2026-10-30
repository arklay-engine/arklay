# The RE2 oracle

RE2 has no reference source to build a tracer into, so `scripts/oracle-re2.py`
traces the original executable from outside and writes the same
`oracle-trace v1` file (`trace-v1.md`) as the RE1 reference.

It needs a retail install holding `bio2 1.10.exe`, Classic Rebirth's `ddraw.dll`
and `data/`, named by `--game` or `ARKLAY_RE2_ROOT`, plus `wine`, `Xvfb` and, for
screenshots, ImageMagick's `import`. The game runs on its own virtual display,
with a Wine prefix under `--work`, and nothing is written to the install.

```sh
# Record the original, run arklay on the same input, compare
scripts/oracle-re2.py scenario --input oracle/inputs-re2/street_walk.txt \
    --ticks 500 --out target/oracle-re2/street --pack re2.akpak

# Only record
scripts/oracle-re2.py record --ticks 500 --trace ref.trace

# Check that nothing but ticks decides what happens
scripts/oracle-re2.py determinism --ticks 1000 --out target/oracle-re2/det

# List the code Classic Rebirth changes
scripts/oracle-re2.py patches
```

`scenario` exits non-zero at any difference, as `oracle.py compare` does.

## How it works

The tool adopts the game's process and stops it with a breakpoint in `Pad_set`,
just after the call that reads the pad (`0x004D0F58`). Every tick passes there
once, and it's the only code the tool touches. At each stop it reads a line of
state from memory and puts the scripted pad value in `EAX`, so input goes through
the game's own mapping, edge detection and key repeat.

Ticks are counted, not timed. `determinism` records the same input twice, the
second time with a delay on every tick, and needs the two traces to be identical.

## Ticks and input

Tick 0 is the first tick with a live player (`pl.life != 0`). `--start leon` or
`--start claire` takes a fixed route there through the title screen: Original
Mode, the character, Normal, then confirm every 60 ticks until play begins. That
takes 904 ticks for both, and they aren't traced.

Input scripts have the format of `oracle/inputs`, counted from tick 0. This
game's raw pad bits are `up` `0x1000`, `right` `0x2000`, `down` `0x4000`, `left`
`0x8000`, `confirm`/`action`/`fire` `0x0080`, `cancel`/`run` `0x0040`, `aim`
`0x0008` and `start` `0x0800`.

## The trace

| Field | Source |
|---|---|
| `room` | `current_stage + 1`, `current_room` |
| `rng` | `Rnd`'s state, the low 16 bits of `0x00988610` |
| `p` | `pl`: `m.t` for x, y and z, `cdir.y & 0xFFF`, `routine_0`, `routine_1`, `life` |
| `cam` | `current_cut` |
| `e` | each `enemies[n]` with bit 0 of `be_flg` set, as slot `n + 1`: id, position, angle, `routine_0`-`routine_2`, `life`, then three zeros |
| `pad` | the pad value given this tick |
| `fgsys`, `fgstat`, `fgstop`, `fgscn`, `fgcom`, `fgroom` | `fg_system`, `fg_status`, `fg_stop`, and the scenario, common and room flag banks |

`rseed`, `fade` and `f` are placeholders so the line parses: `rseed` and the fade
have no RE2 equivalent recorded yet, and the frame isn't hashed. A screenshot of
any tick is available through `--shot-dir` and `--shot-every`.

## What compares today

`scenario` compares `room`, `cam` and the player's x, y, z, angle and health
(`--fields`). The other fields describe things arklay doesn't model RE2's way yet:
its `beh` and `as` are RE1's player machine, its random stream isn't `Rnd`, and it
has no enemies. Add each one to the default in `RE2_FIELDS` when the milestone
that makes it comparable lands. arklay doesn't emit the `fg*` flag banks yet.

## Classic Rebirth

The game runs with Classic Rebirth. The tool turns off Classic Rebirth's gameplay options in its own
copy of `config.ini` (quick turn, tactical reload, auto aim, cutscene skip, debug
mode). With those off, `patches` shows no changes to `Pl_move`, the weapon
routines or `Game_loop`. What remains in the player, door, room and enemy code is
calls redirected for loading files, drawing and sound.

## Limits

- Runs start from a new game in room 100. There is no way yet to boot a room or
  load a save directly, so other rooms are reached by playing to them.
- Scenario B has no route yet.
- `fg_status`'s top bit differs between runs on the title screen, before tick 0.
  The tool doesn't trace those ticks.
