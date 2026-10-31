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

`--keep-enemy SLOT` leaves that slot running, for a character arklay has. Room 101's scene wants `--no-enemies --keep-enemy 0`: the input `first_door.txt` over 4500 ticks, compared with `--fields room,cam,p.x,p.y,p.z,p.angle,p.beh,p.as,p.hp,mot,em`, matches from the room's first line. The zombies can't touch the player in arklay yet, and the street's would attack the route, so this doesn't show the scene with them.

`street_shoot.txt` aims the handgun in room 100 and fires until it's empty, with the zombies running. Over 2000 ticks, with the input long since ended, the zombies come for the player and kill them: damage, death and corpses, six grabs with bites and shove-offs, the player dying in a zombie's hands, the others eating the body and `Die`'s stops. Everything matches through the end of the game over at tick 1997 (`--fields rng,p.x,p.z,p.angle,p.beh,p.as,p.hp,zm0,zm1,zm2,zm3,zm4,zm5`), apart from slot 1's one angle on the first tick. The original then returns to the title. With `--sound`, `snd=` lists each tick's `SsPlay`s as `type:index:volume:pan`; without it the game has no audio device and plays none. arklay's trace has the same field, so sound is compared like the rest. `neck=` starts with the same flags, joint, whether it has a target and turn on both sides. `pixels` compares the game over's frames too: `YOU DIED` within a few levels of the original, and `die=` in the trace holds `Die`'s phase and the text's state.

`street_gun_low.txt`, `street_gun_level.txt` and `street_gun_up.txt` fire in room 100 at each aim. Run over 1500 ticks with weapon 5, 6, 7 or 8 equipped in both games and its ammunition given (`--give 0:WEAPON:MAGAZINE --give 1:AMMO:60`), all twelve fights match exactly on `rng`, the player and every zombie, including `Em10_Die`'s blown-off, torn and thrown deaths and the arm pulled off in `street_gun_up.txt` with weapon 5. `record` has no option to equip a weapon, so this needs a temporary patch in `oracle-re2.py` and `tick_re2_weapon`. Name the zombies `zm0` to `zm5` in `oracle.py compare --fields`: it silently ignores a bare `zm`. Frames from `pixels` differ by a mean of about 3.5, from the fires' animation phase and the pieces a blast throws (`Parts_bomb`).

`--fields rng,zm0,zm1,zm2,zm3,zm4,zm5` compares `Rnd` and each zombie's position, angle, routine, motion, `Type` and life, from the trace's `zm` field. With no input, room 100 matches from t=1 to t=193, when one reaches Leon.

`--sound` runs the game with `DirectSoundEnable` on, in a prefix of its own with Wine's audio going to a private PulseAudio sink: the audio is written beside the trace as a `.wav` and what the music loaded as a `.music` log. Only then does the original's `bgm` field say anything, so compare it with `--sound --fields room,bgm`: the word for the room, each sequence's state and hold, and the channels' tracks. The original's door takes the same ticks either way, but its room load comes 91 ticks later with sound, so the other fields of a `--sound` trace differ during a door. `first_door.txt` over 4500 ticks matches for Leon, and over 1200 for Claire, but for the one tick the original has unloaded the tracks.

`pixels` runs a scenario with screenshots and writes `cmp/tNNNNNN.png` for every `--every` ticks: the original, arklay and their difference times four. It prints the mean error and the worst frames. The original's screenshot can lag a tick, which shows as a large error on the tick of a cut and nothing after it. Its `--offset` is added to the original's tick to give arklay's, so a `--warp` scene that `compare` runs at `--offset 344` wants `--offset=-344` here, and it has to be a multiple of `--every` for both sides to capture the same tick: `--every 8` suits 344. Room 114's scene is at 0.18.

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
`--warp 116:-14800:0:-24000:0xc00`. `bin_up_and_down.txt` climbs the bin in room 118 and jumps back down, and wants `--no-enemies --warp 118:-24050:0:-6500:0`.

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
| `neck` | the player's `Neck_flg`, `Neck_no`, whether it aims at another work, and the joint's turn, rates, limits and rotation; arklay doesn't write it |
| `bgm` | `Bgm`, each of the three `Seq_ctr`'s state and hold, and the tracks of the five channels (`bgm/<track>`), `-` for none; it says something only with `--sound` |

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
