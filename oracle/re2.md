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
arklay, which is how the routes are checked; the zombie recordings run with them
left on. `oracle/inputs-re2/` holds the scripts in use, and `street_wander.txt`
is a random walk through room 100.

`--keep-enemy SLOT` leaves that slot running, for a character arklay has. Room 101's scene wants `--no-enemies --keep-enemy 0`: the input `first_door.txt` over 4500 ticks, compared with `--fields room,cam,p.x,p.y,p.z,p.angle,p.beh,p.as,p.hp,mot,em`, matches from the room's first line. A zombie left running would attack the player on the route, so this doesn't show the scene with them.

`kendo_attacked.txt` runs from room 100 through the door into room 101 and through the scene where the zombies break the window and kill Kendo, with the player left to die afterwards. `--no-enemies --no-enemies-room 0` clears the enemies in room 100 only (the number is `ROOM`'s, stage and room as `game.u16(ROOM)` reads them), and arklay starts at the door with a `--setup-script` and the input moved 780 ticks earlier (`--offset -780`, `--room 100 --player 0`). `rseed=` is `Sce_rnd_set`'s state. A door keeps it, so to compare this route tick by tick, force its value and `Rnd`'s in arklay's start with a setup script's `api:set_re2_sce_rnd` and `api:set_re2_rnd`, so that arklay's tick 0 has what the original has at that point. `rseed=` only shows the low 16 bits of the state and `set_re2_sce_rnd` takes all 32, so read them from the original's `RANDOM_BASE`. The state steps twice between the setup script and tick 0, and room 100's load draws `Rnd` 77 times, so the values to force come from before that point. Check the first ticks' `rng=` and `rseed=` against the original before trusting a run, and compare `em` and `zm` too: Kendo chooses where to turn from `Sce_rnd`. A setup script that does this places the player at `-20524, 0, -18869`, angle 2064, with health 200, `api:set_re2_rnd(39135)`, `api:set_re2_sce_rnd(2060504824)` and `api:set_room_camera(8)`, and clears room 100's zombies as the original's `--no-enemies-room 0` does with `api:set_re2_flag(6, N)` for their dead flags 35 to 41 (with them gone, the room's load draws no `Rnd`). Tick 10, where the door starts, prints a different `rng` and `rseed` for that tick only. `em`'s and `mot`'s last three fields are `Spd` and aren't compared. The player's neck turn (`neck=`'s fifth field) can differ by one for a tick, where the original's settles a tick later. With that, `rng`, `rseed`, the player, Kendo and all the zombies match over 3820 ticks, and `pixels` has a mean error of 1.21; the worst frames are the door's fade and the frames of two cuts.

`street_shoot.txt` aims the handgun in room 100 and fires until it's empty, with the zombies running. Over 2000 ticks, with the input long since ended, the zombies come for the player and kill them: damage, death and corpses, six grabs with bites and shove-offs, the player dying in a zombie's hands, the others eating the body and `Die`'s stops. Everything matches through the end of the game over at tick 1997 (`--fields rng,p.x,p.z,p.angle,p.beh,p.as,p.hp,zm0,zm1,zm2,zm3,zm4,zm5`), apart from slot 1's one angle on the first tick. The original then returns to the title. With `--sound`, `snd=` lists each tick's `SsPlay`s as `type:index:volume:pan`; without it the game has no audio device and plays none. arklay's trace has the same field, so sound is compared like the rest. `neck=` starts with the same flags, joint, whether it has a target and turn on both sides. `pixels` compares the game over's frames too: `YOU DIED` within a few levels of the original, and `die=` in the trace holds `Die`'s phase and the text's state.

`street_gun_low.txt`, `street_gun_level.txt` and `street_gun_up.txt` fire in room 100 at each aim. Run over 1500 ticks with weapon 5, 6, 7 or 8 equipped in both games and its ammunition given (`--equip WEAPON --give 0:WEAPON:MAGAZINE --give 1:AMMO:60`), all twelve fights match exactly on `rng`, the player and every zombie, including `Em10_Die`'s blown-off, torn and thrown deaths and the arm pulled off in `street_gun_up.txt` with weapon 5. `--equip` holds a weapon from the moment play is settled, and `--life` sets the player's life when play begins. The licker's id `0x22` is in `zm` too. Name the zombies `zm0` to `zm5` in `oracle.py compare --fields`: it silently ignores a bare `zm`. Frames from `pixels` differ by a mean of about 3.5, from the fires' animation phase and the pieces a blast throws (`Parts_bomb`).

The licker's behaviour moved from Rust to `enemy/22/enemy.lua` with byte-identical traces over 36 fights in room 211, and the zombie's to `lib/zombie.lua` the same way, over the street inputs here, room 101's attack, the feeding rooms and generated fights across the weapons and low life. When moving another enemy, compare traces of the old and new builds the same way: it needs no Wine and shows the first tick that differs. Run these on a variety of weapons and player life, and cover the damage reactions, since a short fight may not reach them.

The climbing licker was checked with `record` traces under weapon 2 and 200 life (`--equip 2 --life 200 --give 0:2:200 --give 1:0x14:60`), compared at `--offset 344` against arklay runs with the same input. These matched the original tick for tick, ignoring the speed fields:

- Room 204's ceiling licker, with `--warp 204:-14992:0:-12079:0 --flag 1:6` over 1300 ticks, fired at from tick 150.
- Room 10A's wall licker, with `--warp 10A:-19000:0:-6000:1024 --flag 1:1` over 2400 ticks, with Leon standing still. Six other 10A fights (aiming, shooting and walking past) also matched.
- Room 20E's wall licker, with `--warp 20E:-24200:0:-19000:0 --flag 1:1`, standing still. It matches for 2400 ticks; a walking run strays after about 600 ticks at `kara_rootck2` (`docs/re2-deviations.md`).

`pixels` with `--warp 10A:-25000:0:-12000:1024:8` (camera 8) shows the wall licker and its shadow; the mean error over 400 ticks of aiming and shooting is 1.3. Room 204 at camera 5 shows the ceiling licker. The pose matches there, but the shading differs, and so does the near-plane clip of an arm.

Room 200's entry scene, where the camera pans up the hall, plays from `--warp 200:-21700:0:-3700:1024 --flag 1:1 --life 200` with no input: the scene starts from cut 0. On arklay's side, `--room 200` with a setup script that places the player there, sets flag 1:1 and calls `api:set_room_camera(0)`. Compare at `--offset 344`. The player and the cuts match the original for the scene's 356 ticks, and `pixels` has a mean error of 0.28. The scene's `pos_set` puts Leon inside the entrance landing's box. The original's `Sca_ck_em` still checks the floor he came in on (`Floor`), not his height, so it pushes him out against his last position (`Old_x` and `Old_z`).

`licker_close.txt` and `licker_long.txt` fight the licker of room 211 with weapon 2 (`--warp 211:4000:0:-14400:2048 --flag 8:119 --give 0:2:90 --give 1:0x14:60`, 1844 and 3744 ticks). Compare at `--offset 344`, against an arklay run of `--room 211` with the input moved 344 ticks earlier and a `--setup-script` that places the player, calls `api:set_re2_flag(8, 119)` and `api:set_re2_rnd(6114)`: a door keeps the stream, so the script sets the value it has when the room's play begins (`Rnd` is only seeded at a new game or at a door that changes the player's model). These runs equip the weapon and its 90 rounds with `--equip` and `api:set_equipped_weapon`: the large magazine avoids a reload. Fights with weapons 5 to 8 and with a low starting life match there too. With the licker in view, use `--warp 211:-500:0:-14400:2048:5` and `api:set_room_camera(5)` for `pixels`: the cut doesn't follow a warp, and frames there differ by a mean of about 0.5, with the tongue in view too. A shot at the licker's tick 120 of that run (`50..700 aim`, `120..125 aim fire`) draws the tongue at tick 169; the same with no shot never does, and a run that lets it kill the player (`50..900 aim`, `70..75` and `200..205 aim fire`) matches through the bite and the white-out for 700 ticks. A pack is loaded whole into memory, so run these one at a time. A reload's dropped magazine (`Mag_set`, `Mag_down`) matches `Rnd`, position and rotation at every tick with weapon 5 in that room and magazine size 8.

`crow_shots.txt` fights the eight perched crows of room 109 in scenario B (`--warp 109:-14000:0:-14800:1024 --flag 1:1 --equip 7 --give 0:7:200 --give 1:0x14:60`, 2400 ticks). Compare at `--offset 344` against an arklay run of `--room 109` with the input moved 344 ticks earlier and a setup script with the same place, life, items, `api:set_re2_flag(1, 1)` and `api:set_re2_rnd(6114)`. `Rnd`, the player and all the crows match to the end, and also with weapon 5 and with the wire crows and weapon 7 without the flag. With `:9` on the warp and `api:set_room_camera(9)` the same fight has them in view for `pixels`: the bodies and their poses match, with differences only along the edges of their wings.

`dog_shots.txt` fights the two dogs of room 212 (`--warp 212:-11250:0:-25692:3024 --flag 4:83 --equip 2 --give 0:2:200 --give 1:0x14:60`, 2400 ticks). Compare at `--offset 344` against an arklay run of `--room 212` with the input moved 344 ticks earlier and a setup script with the same place, life, flag, items and `api:set_re2_rnd(6114)`. `Rnd`, the player and both dogs match to the end. So do fights with weapons 3 and 5 to 8, standing still until killed, running and shooting, and rooms 21A (sleeping and eating, kinds 7 and 8), 20F (kind 4) and 216 (kind 2, flag 1:6). The exceptions are in `docs/re2-deviations.md`. The first trace row of a dog in 21A differs in its speed, which the original hasn't set yet. Weapons 4, 10, 11 and 14 can't be tested this way: arklay doesn't aim weapon 4 yet, and with the others the reference run ends early. `pixels --arklay-input` gives arklay its own moved input, and these frames differ by a mean of about 0.8, mostly in the player's shot and a blast's parts.

`--fields rng,zm0,zm1,zm2,zm3,zm4,zm5` compares `Rnd` and each zombie's position, angle, routine, motion, `Type` and life, from the trace's `zm` field. With no input, room 100 matches from t=1 to t=193, when one reaches Leon.

`--sound` runs the game with `DirectSoundEnable` on, in a prefix of its own with Wine's audio going to a private PulseAudio sink: the audio is written beside the trace as a `.wav` and what the music loaded as a `.music` log. Only then does the original's `bgm` field say anything, so compare it with `--sound --fields room,bgm`: the word for the room, each sequence's state and hold, and the channels' tracks. The original's door takes the same ticks either way, but its room load comes 91 ticks later with sound, so the other fields of a `--sound` trace differ during a door. `first_door.txt` over 4500 ticks matches for Leon, and over 1200 for Claire, but for the one tick the original has unloaded the tracks.

`pixels` runs a scenario with screenshots and writes `cmp/tNNNNNN.png` for every `--every` ticks: the original, arklay and their difference times four. It prints the mean error and the worst frames. On the tick a cut changes and the tick after, the original shows the frame from before the change again, and the new cut first shows on the tick after that. Its `--offset` is added to the original's tick to give arklay's, so a `--warp` scene that `compare` runs at `--offset 344` wants `--offset=-344` here, and it has to be a multiple of `--every` for both sides to capture the same tick: `--every 8` suits 344. Room 114's scene is at 0.18.

`--flag BANK:INDEX` raises a bit of a flag bank (`ck` and `set`'s numbering) when play begins, before `--warp`; a `--setup-script`'s `api:set_re2_flag(bank, index)` is the same on arklay's side, and starts the room's scripts again. Claire's room 114 scene with Sherry wants `--start claire --flag 8:93 --warp 114:-26100:0:-24800:0xc00` with no input, and `--room 114 --player 1` with the flag, position, angle, health and `api:set_re2_rnd(6114)` in a setup script. Leave the enemies on: the zombie in the corridor walks a scripted clip, `plc_motion(1, 0, 5)`, whose `Sce_flg` bit 1 pins its foot. Over 1500 ticks, `p`, `cam`, `mot`, `es`, `zm` and `rng` match at `--offset 344`, and `pixels` every 86 ticks has a mean error of 0.53. The reference runs with `DirectSoundEnable` off, so a voice line's hold is `Xa_control`'s count, not wall-clock time.

Leon's room 202 scene with Marvin wants `--start leon --warp 202:-9350:0:-16100:2048` with no input, and `--room 202 --player 0` with the position, angle, health 200 and `api:set_re2_rnd(6114)` in a setup script. Over 1250 ticks the trace matches at `--offset 344`, and both bodies' part matrices match the original's (`+0x48` and `+0x5c` of each part). `pixels` every 43 ticks has a mean error of 2.54. One frame is a cut the original shows a frame late; the rest is shading. Each body's light rows are those `Set_Light_data` (`0x502f00`) leaves at `0x689808` when it's called from `0x4c0d96`, and a breakpoint after that call reads them per body. Two light rows triangulate the point the body was lit from: the first part's world translation, the hips, for the player as for every character. With the light colours zeroed in the LIT records (`[[0x98861c]+0x2c] + cut * 0x28 + 4`, nine bytes) both engines' ambient-only frames agree within a few percent; with them on, Marvin's face is still about half as bright in arklay, and the cause isn't found.

Marvin turns in room 202 once `4:7` and `4:9` are set: `--warp 202:-21000:0:-21000:1024 --flag 4:7 --flag 4:9` in the original, and `--room 202 --player 0` with that position, angle, health 200, `api:set_re2_flag(4, 7)`, `api:set_re2_flag(4, 9)` and `api:set_re2_rnd(6114)` in a setup script. The original turns him while the room loads: his routine stays at 4,0,1 until the room's first tick, when his `Id` is already 30. arklay turns him on its first tick, with his position within two units. His grab on the player starts 14 ticks later than the original's, because under `--room` the player's neck has no position on the first tick and the first sight check fails; from the grab on, the bites and the release match tick for tick.

`--give SLOT:ID:COUNT` puts an item in the inventory when play begins, for what
the start of a game has nothing to test with. `street_reload.txt` wants
`--give 3:0x14:20` and the same stack added to the end of arklay's
`games/re2/game/new_game.lua` for the comparison.

`--warp ROOM:X:Y:Z:ANGLE[:CUT]` takes the player from the street to any room the way
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
health, and `mot` (`--fields`). The enemy and presentation fields are opt-in:
`rng` for the shared random stream, `e`, `em` and `es` for the enemy slots,
`neck` for the player's neck, and the `snd` and `die` traces for sound and the
game over. arklay doesn't emit the `fg*` flag banks yet. Add each new field to
the default in `RE2_FIELDS` when the milestone that makes it comparable lands.

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
