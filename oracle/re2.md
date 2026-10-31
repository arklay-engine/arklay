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

Tick 0 is the first tick after the room's load has settled: a few ticks after
the player comes alive (`pl.life != 0`), which is when `Pl_init` has run and the
start position's collision push has landed. `--start leon` or `--start claire`
takes a fixed route there through the title screen: Original Mode, the
character, Normal, then confirm every 60 ticks until play begins. That takes 907
ticks for both, and they aren't traced. A line holds the state at the end of its
tick, and a pad value given for tick `t` first shows in line `t + 1`.

`--no-enemies` clears bit 0 of every enemy's `be_flg` at each stop, so nothing
runs them or collides with them. That leaves the player alone to compare with
arklay until enemies are ported. `oracle/inputs-re2/` holds the scripts in use,
and `street_wander.txt` is a random walk through room 100.

`--keep-enemy SLOT` leaves that slot running, for a character arklay has. Room 101's scene wants `--no-enemies --keep-enemy 0`: the input `first_door.txt` over 4500 ticks, compared with `--fields room,cam,p.x,p.y,p.z,p.angle,p.beh,p.as,p.hp,mot,em`, matches from the room's first line. Its zombies have no state in arklay yet, and the street's would attack the route, so this doesn't show the scene with them.

`--flag BANK:INDEX` raises a bit of a flag bank (`ck` and `set`'s numbering) when play begins, before `--warp`; a `--setup-script`'s `api:set_re2_flag(bank, index)` is the same on arklay's side, and starts the room's scripts again. Claire's room 114 scene with Sherry wants `--start claire --no-enemies --flag 8:93 --warp 114:-26100:0:-24800:0xc00` with no input, and `--room 114 --player 1` with the flag, position, angle and health in a setup script. Over 1500 ticks, `p`, `cam`, `mot` and `es` match at `--offset 344`. The reference runs with `DirectSoundEnable` off, so a voice line's hold is `Xa_control`'s count, not wall-clock time.

`--give SLOT:ID:COUNT` puts an item in the inventory when play begins, for what
the start of a game has nothing to test with. `street_reload.txt` wants
`--give 3:0x14:20` and the same stack added to the end of arklay's
`games/re2/game/new_game.lua` for the comparison.

`--warp ROOM:X:Y:Z:ANGLE` takes the player from the street to any room the way
`Sce_Door` does, for what is far from the start. The new room's play begins at
tick 344, so an arklay run of that room with `--room`, `--player`, `--character`
and a `--setup-script` that places the player compares at `--offset 344`, with
the script's input moved 344 ticks earlier and the health set to the original's.
`stairs_up_and_down.txt`, `stairs_refused.txt` and `stairs_at_a_run.txt` want
`--warp 116:-14800:0:-24000:0xc00`.

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
| `mot` | the player's `routine_2`, `routine_3`, `Move_no`, `Move_cnt`, `Hokan_flg`, `D_life_u` and speed x, y, z |
| `em` | enemy slot 0 when it's a character (id 0x40 up): id, x, y, z, `cdir.y & 0xFFF`, `routine_0`-`routine_3`, `Move_no`, `Move_cnt`, `Hokan_flg`, `D_life_u` and speed x, y, z; `scripts/oracle.py` compares all but the speed |
| `es` | `splayer_work`, Sherry and the other second players, with `em`'s fields |

`rseed`, `fade` and `f` are placeholders so the line parses: `rseed` and the fade
have no RE2 equivalent recorded yet, and the frame isn't hashed. A screenshot of
any tick is available through `--shot-dir` and `--shot-every`.

## What compares today

`scenario` compares `room`, `cam`, the player's x, y, z, angle, `beh`, `as` and
health, and `mot` (`--fields`). The other fields describe things arklay doesn't
model RE2's way yet: its random stream isn't `Rnd`, and it has no enemies. Add
each one to the default in `RE2_FIELDS` when the milestone that makes it
comparable lands. arklay doesn't emit the `fg*` flag banks yet.

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
