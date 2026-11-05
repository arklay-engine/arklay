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

`kendo_attacked.txt` runs from room 100 through the door into room 101 and through the scene where the zombies break the window and kill Kendo, with the player left to die afterwards. `--no-enemies --no-enemies-room 0` clears the enemies in room 100 only (the number is `ROOM`'s, stage and room as `game.u16(ROOM)` reads them), and arklay starts at the door with a `--setup-script` and the input moved 780 ticks earlier (`--offset -780`, `--room 100 --player 0`). `rseed=` is `Sce_rnd_set`'s state. A door keeps it, so to compare this route tick by tick, force its value and `Rnd`'s in arklay's start with a setup script's `api:set_re2_sce_rnd` and `api:set_re2_rnd`, so that arklay's tick 0 has what the original has at that point. `rseed=` only shows the low 16 bits of the state and `set_re2_sce_rnd` takes all 32, so read them from the original's `RANDOM_BASE`. The state steps twice between the setup script and tick 0, and room 100's load draws `Rnd` 77 times, so the values to force come from before that point. Check the first ticks' `rng=` and `rseed=` against the original before trusting a run, and compare `em` and `zm` too: Kendo chooses where to turn from `Sce_rnd`. A setup script that does this places the player at `-20524, 0, -18869`, angle 2064, with health 200, `api:set_re2_rnd(39135)`, `api:set_re2_sce_rnd(2060504824)` and `api:set_room_camera(8)`, and clears room 100's zombies as the original's `--no-enemies-room 0` does with `api:set_re2_flag(6, N)` for their dead flags 35 to 41 (with them gone, the room's load draws no `Rnd`). Tick 10, where the door starts, prints a different `rng` and `rseed` for that tick only. `em`'s and `mot`'s last three fields are `Spd` and aren't compared. The player's neck turn (`neck=`'s fifth field) can differ by one for a tick, where the original's settles a tick later. With that, `rng`, `rseed`, the player, Kendo and all the zombies match over 3820 ticks, and `pixels` has a mean error of 1.21; the worst frames are the frames of two cuts. With `--every 1`, the frames through the door match too: the original shows black from the tick after the door fires, because `Psx_main` presents nothing of its hold, and after the door it skips the room's first four frames (`Swap_Cbuff`'s `Movie_set(4)`), so the fade-in shows from its fifth tick.

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

`licker24_fight.txt` and `licker24_close.txt` fight the two `0x24` lickers of room 605 with weapon 2 (`--warp 605:-24500:0:-24000:0 --give 0:2:200 --give 1:0x14:60 --equip 2 --life 200`, 1800 and 1900 ticks). Leon drops in from the duct. In the first he holds his ground; in the second he runs east to the wall and fires as they come. Compare at `--offset 344` against an arklay run of `--room 605` with the input moved 344 ticks earlier and a setup script that places the player at `-24500, 0, -24000`, angle 0, gives the same life and items, equips the weapon and calls `api:set_re2_rnd(6114)`. The room's resting sparks (`Esp_tbl`'s 0x41) draw `Rnd` from the first tick, so they have to match too. `Rnd`, the player and both lickers match to the end of both runs: in `licker24_close.txt` one licker takes a shot and bites Leon five times. `pixels` over the same run has a mean error of 0.23. `pixels` runs arklay with `--no-mods`, so give it a pack converted with the scripts being tested, not a mod.

`crow_shots.txt` fights the eight perched crows of room 109 in scenario B (`--warp 109:-14000:0:-14800:1024 --flag 1:1 --equip 7 --give 0:7:200 --give 1:0x14:60`, 2400 ticks). Compare at `--offset 344` against an arklay run of `--room 109` with the input moved 344 ticks earlier and a setup script with the same place, life, items, `api:set_re2_flag(1, 1)` and `api:set_re2_rnd(6114)`. `Rnd`, the player and all the crows match to the end, and also with weapon 5 and with the wire crows and weapon 7 without the flag. With `:9` on the warp and `api:set_room_camera(9)` the same fight has them in view for `pixels`: the bodies and their poses match, with differences only along the edges of their wings.

`crow_held.txt` runs into the first crow with the gun raised and a turn held, so the peck lets go of the player straight into aiming and turning (same flags, items and setup script, 1500 ticks). A hold's last tick plays the crow's clip and then changes the player's bank (`FUN_00463210`'s release), and `Foot_set_pl`'s first turning tick after the blend reads that pose, so this shows whether the pose of a hold's end is right. `Rnd`, the player and all the crows match through tick 786. `docs/re2-deviations.md` has what follows.

`dog_shots.txt` fights the two dogs of room 212 (`--warp 212:-11250:0:-25692:3024 --flag 4:83 --equip 2 --give 0:2:200 --give 1:0x14:60`, 2400 ticks). Compare at `--offset 344` against an arklay run of `--room 212` with the input moved 344 ticks earlier and a setup script with the same place, life, flag, items and `api:set_re2_rnd(6114)`. `Rnd`, the player and both dogs match to the end. So do fights with weapons 3 and 5 to 8, standing still until killed, running and shooting, and rooms 21A (sleeping and eating, kinds 7 and 8), 20F (kind 4) and 216 (kind 2, flag 1:6). The exceptions are in `docs/re2-deviations.md`. The first trace row of a dog in 21A differs in its speed, which the original hasn't set yet. Weapons 4, 10, 11 and 14 can't be tested this way: arklay doesn't aim weapon 4 yet, and with the others the reference run ends early. `pixels --arklay-input` gives arklay its own moved input, and these frames differ by a mean of about 0.8, mostly in the player's shot and a blast's parts.

Mr. X (`Em_2A`, `enemy/2a/enemy.lua`) is in `zm` too. In scenario B, room 20D has him walk at the player from the far end: `--warp 20D:-26510:0:-24891:3752 --flag 1:1 --flag 4:78 --life 200` with no input, against an arklay run of `--room 20D --character 2` with a setup script that places the player, sets the life and both flags and calls `api:set_re2_rnd(6114)`. Compare at `--offset 344`. `Rnd`, the player and Mr. X match from his first step to the game over. With `mrx_shots.txt` (a shot every 30 ticks) or `mrx_shots_slow.txt` (every 60), `--equip 7 --give 0:7:200 --give 1:0x14:60`, and the same items, `api:set_equipped_weapon` and the moved input on arklay's side, the fights match to the end with weapons 2, 5 and 7: his flinch, and his drop to one knee below 20000 life. `--life 30` brings out his overhead blow and his kill by the neck, which drags the player. Room 20C drops him from the ceiling when the player walks in under it: `--warp 20C:-10500:0:-14200:0 --flag 1:1 --flag 4:78`. With no input, the drop, the fight and the death match. With `mrx_shots.txt` and weapon 7, his charge and the kill by the neck match too. Rooms 109 (`--warp 109:-4696:0:-1380:560 --flag 1:6`) and 10C (`--warp 10C:-25450:-3600:-15800:0:7 --flag 1:6`) match to the game over. Rooms 117, 507, 602, 604 (both characters), 612 and 30A, entered through their doors with the flags their scripts check, match while he waits there for the script; give Claire's rooms `--player 1`.

The plants (`Em_2E` for ids 0x2e and 0x39, `Em_2F` for the vines, id 0x2f) are in `zm` too. Room 110 places two poison ivies and a template vine they grow six vines from, under flag 1:6 only (in both scenarios and for both players: no other flag places them). Run `--warp 110:-13697:-7200:8942:3224:5 --flag 1:6 --life 200` with no input, against an arklay run of `--room 110` with a setup script that places the player, sets the life and the flag and calls `api:set_re2_rnd(6114)`. Compare at `--offset 344`. The room's load draws `Rnd` 21 times: four for each ivy, two for each vine it grows and one for the template. Every plant, the player and `Rnd` match for the 1000 ticks run, through the first ivy's grab; the vines are 1 or 2 units out from tick 418. With the player at `-14300, -7200, 9900` it's the same. The vines' positions come from the ivy's posed parts, so they show any difference in its neck turn first. Room 205 places two ivies and a template under flag 1:6 as well: run `--warp 205:-12000:0:-12000:0 --flag 1:6 --life 200` with no input. Its first ivy starts in state 5, the spit from range, and the stale `Dest` its slot keeps from an earlier room turns its first walk ticks (`docs/re2-deviations.md`). Room 207 places two 0x39 poison ivies and a template under the same flag: run `--warp 207:-20500:0:-13500:1024 --flag 1:6 --life 200` with no input. The original pushes the player to `z` -13012 before play begins, so arklay's setup script places the player there. The plants, the player and `Rnd` match to the end of the reference, through the first ivy's spit, its hit on the player at tick 259, the poison and its first life taken.

`plant_shots.txt` fights the plants of rooms 110 and 207 with a shot every 30 ticks: add `--equip W --give 0:W:200 --give 1:0x14:60` to the runs above, with 2300 ticks. Compare at `--offset 344` against arklay's `--room` run with the input moved 344 ticks earlier and the same setup script plus `api:give_item(W, 200)`, `api:give_item(0x14, 60)` and `api:set_equipped_weapon(W)`. With weapons 2, 5, 6, 7 and 8, the plants' routines, motion and life and the player's routine and life match through hits, the retreat, a hold (weapons 2 and 6 in room 110), the deaths, the parts thrown and the vines' fall, to the end of 1956 ticks. `Rnd` matches too, except with weapon 6 in room 110: a spent case lands at tick 425 there, the original draws `Rnd` once more on the next tick, and it is one draw out from then on. Positions are exact with weapons 5 and 7 in both rooms and with 6 and 8 in room 207, and otherwise within 7 to 16 units (32 for weapon 6 in room 110). The details are in `docs/re2-deviations.md`. With weapon 3 the reference crashes before play begins in both rooms.

The giant spider (`Em_25`, id 0x25, `enemy/25/enemy.lua`) is in `zm` too. Room 302 with flag 1:6 places two floor spiders: run `--warp 302:-6611:0:-3724:0 --flag 1:6 --life 200` with no input over 2400 ticks, against an arklay run of `--room 302` with a setup script that places the player at `-1801, 0, -3724` (the original pushes the player there before play begins), angle 0, sets health 200 and flag 1:6 and calls `api:set_re2_rnd(6114)`, over 2056 ticks. Compare at `--offset 344` with `--fields rng,p.x,p.y,p.z,p.angle,p.beh,p.as,p.hp,zm0,zm1`. The room's load draws `Rnd` twice for each spider, and `Sca_ck_em` pushes the first 600 units out of a wall. With the player that far away the spiders only rest, walk and turn, and `Rnd`, the player and both spiders match for all 2056 ticks. Without the flag the room places a wall spider (`Type` 9) instead, whose states aren't ported.

The floor spiders' attack states want the player within 8000 units. Run `--warp 302:-13000:0:-12000:0 --flag 1:6 --life 200` with no input over 1400 ticks, and arklay's `--room 302` with a setup script that places the player at `-13000, 0, -12000` (the original leaves the player there), angle 0, sets health 200 and flag 1:6 and calls `api:set_re2_rnd(6114)`, over 1056 ticks. Compare at `--offset 344` with the same fields. The spiders, about 5300 units from the player, turn toward the player (`Routine_1` 5), lunge (7) and spit (8), and `Rnd`, the player and both spiders match for all 1056 ticks. With the player at `-13500, 0, -8500` (both sides), the second spider starts about 2400 units away. Its lunges stop where the bodies' pushes leave it, about 1770 units from the player, outside the bite's 1500, and that run matches for all 1056 ticks too. The noise wants a shot: run `--warp 302:-6611:0:-3724:0 --flag 1:6 --life 200 --give 0:2:90 --give 1:0x14:60 --equip 2` with `oracle/inputs-re2/spider_noise.txt` (a shot every 20 ticks from 420 to 800, facing away from the spiders), and arklay as in the first recipe, also calling `api:give_item(2, 90)`, `api:give_item(0x14, 60)` and `api:set_equipped_weapon(2)`, with the input 344 ticks earlier. A spider resting when a shot goes off leaps aside (9) or turns (5), and all 1056 ticks match. Between them the three runs enter state 5 13 times, 7 32 times, 8 7 times and 9 8 times. A run where the player walks into the spider (`--warp 302:-13500:0:-8500:2048`, `up` held from tick 360) differs at its tick 22: the original draws `Rnd` twice that arklay doesn't, while both spiders are in states that draw nothing. The player walks on the spot against something there, so the draws are most likely the player's, and that run isn't one of this recipe's.

`--fields rng,zm0,zm1,zm2,zm3,zm4,zm5` compares `Rnd` and each zombie's position, angle, routine, motion, `Type` and life, from the trace's `zm` field. With no input, room 100 matches from t=1 to t=193, when one reaches Leon.

Rooms 610 and 11B place zombies that can see the player's start. Run `--warp 610:-19500:0:-27000:0` or `--warp 11B:8000:0:-19500:0` with no input, against arklay's `--room 610` or `--room 11B` with a setup script that places the player, sets health 200 and calls `api:set_re2_rnd(6114)`, and compare at `--offset 344`. Before play begins, the original pushes the player in room 610 to `-19771, 0, -26129`, so arklay's setup script places the player there: from `-19500, 0, -27000` the first zombie is beyond the stand's 5000 units. Both rooms then differ from tick 0. The original's zombies that see the player (within 5000 units, facing it and in sight) leave the stand on the first tick, and arklay's leave it two ticks later, so `Rnd` and their routines differ from then on. Under `--room` the player's neck has no position until the end of arklay's tick 1, so `Root_ck`'s sight check fails on ticks 0 and 1, as for Marvin in room 202. In a test build that gave the zombies a neck point from the room's load (the player's position plus `-61, -2514, -1`, where arklay's pose puts it), `Rnd`, the player and every zombie matched over 1056 ticks in both rooms. Neither room calls `sce_em_set2`, and the zombies match with `Dest` started at 0.

A real door into room 11B is room 11A's door 1, which puts the player at `-9500, 0, -20400`, angle 1024. Run `--warp 11A:9150:0:-2600:3072 --flag 6:156 --flag 6:157 --life 200` with the input `oracle/inputs-re2/door_11b.txt` (`344 up`, `354`, `364 action`, `368`) over 1200 ticks: the player stands against the door, and the action press at tick 364 opens it. The two flags are the dead flags of room 11A's zombies, which would otherwise reach the player first. On arklay's side, run `--room 11A` with a setup script that places the player there with angle 3072, sets health 200, calls `api:set_re2_flag(6, 156)`, `api:set_re2_flag(6, 157)` and `api:set_re2_rnd(6114)`, and the same input 344 ticks earlier, over 856 ticks. Compare at `--offset 344`. Room 11B's load comes at tick 399 and its zombies are placed at 400. Then play stays stopped until tick 704, the last stopped tick (`fg_stop` `0xEF000000`), and runs from 705. `Rnd`, the player, `mot` and all six zombies match to tick 1097, apart from the door's own tick (364), whose `rng` is known to differ. With `--flag 0:25` on the original and `api:set_re2_flag(0, 25)` in the setup script, the room places its other set of zombies, and those match over all 856 ticks too. `--fields pneck` compares where the player's head is (see the trace table). The original keeps room 11A's position, `9150, -2513, -2668`, from the door until tick 703, places the head in room 11B on tick 704, and then has it within 44 units of arklay's until both agree exactly from tick 711. arklay has no position from the load until the end of tick 705, because its zombies run before `Player_move` in a tick. So on room 11B's first tick of play, `Root_ck` finds a neck in the original and none in arklay. In these two runs no zombie's routine depended on that tick's sight. With the default set, zombie 2 used to leave its wander (routine 1,0,3) for the walk (1,1,1), drawing `Rnd`, on tick 1098 in the original and on tick 1099 in arklay. `Root_ck`'s turn (`+0x150`) and `Dest` (`+0x1c4`, `+0x1c6`) matched, and so did the player's neck. Zombie 2's own neck didn't: the original's joint `+0x5C` is the head on the body after `Oba_ck_em` and `Sca_ck_em` have pushed it, and every part's `+0x5C` and `+0x68` matches arklay's pose after the push. arklay took the head before the push. On tick 1097 the push moved it 18 and -14 units, and from there the original's line to the player was clear a tick earlier. arklay now places the neck after the pushes, and `Rnd`, the player, `mot` and all six zombies match over all 856 ticks, apart from the door's tick, with either set. A door with a standing zombie that faces its entry point within 5000 units, on the first tick of play, would show whether the gap matters. Room 60C's door into room 610 doesn't: its entry point, `-24424, 0, -14288`, is more than 5000 units from every zombie there.

`--sound` runs the game with `DirectSoundEnable` on, in a prefix of its own with Wine's audio going to a private PulseAudio sink: the audio is written beside the trace as a `.wav` and what the music loaded as a `.music` log. Only then does the original's `bgm` field say anything, so compare it with `--sound --fields room,bgm`: the word for the room, each sequence's state and hold, and the channels' tracks. The original's door takes the same ticks either way, but its room load comes 91 ticks later with sound, so the other fields of a `--sound` trace differ during a door. `first_door.txt` over 4500 ticks matches for Leon, and over 1200 for Claire, but for the one tick the original has unloaded the tracks.

`pixels` runs a scenario with screenshots and writes `cmp/tNNNNNN.png` for every `--every` ticks: the original, arklay and their difference times four. It prints the mean error and the worst frames. On the tick a cut changes and the tick after, the original shows the frame from before the change again, and the new cut first shows on the tick after that. Its `--offset` is added to the original's tick to give arklay's, so a `--warp` scene that `compare` runs at `--offset 344` wants `--offset=-344` here, and it has to be a multiple of `--every` for both sides to capture the same tick: `--every 8` suits 344. Room 114's scene is at 0.18.

`--flag BANK:INDEX` raises a bit of a flag bank (`ck` and `set`'s numbering) when play begins, before `--warp`; a `--setup-script`'s `api:set_re2_flag(bank, index)` is the same on arklay's side, and starts the room's scripts again. Claire's room 114 scene with Sherry wants `--start claire --flag 8:93 --warp 114:-26100:0:-24800:0xc00` with no input, and `--room 114 --player 1` with the flag, position, angle, health and `api:set_re2_rnd(6114)` in a setup script. Leave the enemies on: the zombie in the corridor walks a scripted clip, `plc_motion(1, 0, 5)`, whose `Sce_flg` bit 1 pins its foot. Over 1500 ticks, `p`, `cam`, `mot`, `es`, `zm` and `rng` match at `--offset 344`, and `pixels` every 86 ticks has a mean error of 0.53. The reference runs with `DirectSoundEnable` off, so a voice line's hold is `Xa_control`'s count, not wall-clock time.

Leon's room 202 scene with Marvin wants `--start leon --warp 202:-9350:0:-16100:2048` with no input, and `--room 202 --player 0` with the position, angle, health 200 and `api:set_re2_rnd(6114)` in a setup script. Over 1250 ticks the trace matches at `--offset 344`, and both bodies' part matrices match the original's (`+0x48` and `+0x5c` of each part). `pixels` every 43 ticks had a mean error of 2.54 before #383. One frame is a cut the original shows a frame late; the rest was shading, which #383's RE2 light change addresses. Each body's light rows are those `Set_Light_data` (`0x502f00`) leaves at `0x689808` when it's called from `0x4c0d96`, and a breakpoint after that call reads them per body. Two light rows triangulate the point the body was lit from: the first part's world translation, the hips, for the player as for every character. With the light colours zeroed in the LIT records (`[[0x98861c]+0x2c] + cut * 0x28 + 4`, nine bytes) both engines' ambient-only frames agree within a few percent; with them on, the original's face was about 1.36 times as bright as arklay's at the close camera before #383. RE2 bodies are now lit in their own space, as Marni does, which brings that ratio to 1.09 (#383), and the shirt goes from 1.23 to 1.01. The last ten percent on the face is not yet explained.

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
| `neck` | the player's `Neck_flg`, `Neck_no`, whether it aims at another work, and the joint's turn, rates, limits and rotation; arklay writes the first five |
| `pneck` | where the player's head is in the world, the neck joint's `+0x5C`, which `Root_ck` aims at; `-` when arklay hasn't posed it. Compared only when `--fields` names it |
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
