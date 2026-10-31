# RE2 deviations

RE2 play is being ported to exactness (Phase C of #244), checked tick by tick against the original with `scripts/oracle-re2.py` (`oracle/re2.md`). Every place where RE2 play knowingly still differs from the original is listed here, so that the rest of the port has the list to work from.

## Player

The pad's walking, running, backing, turning, hurt clips and scripted motions (`player2`, a port of `Player_move`), collision and `Sce_Damage` areas match the original on random walks around room 100 for both characters.

- **The held weapon.** The player always holds the starting handgun, whose bank has the locomotion the knife's lacks.
- **The handguns' stance only.** `Pl_move`'s routine 5 is `pl_mv05_w1` for Leon's and Claire's starting handguns: raising, tilting, firing, reloading and lowering, with their ammunition. The other weapons don't raise. A shot hits nothing and has no flash, shell or camera kick, and the click has no sound; `Em_ck`'s lock-on and the lock-drop button have nothing to do without enemies.
- **The pose before play.** The first tick's blend starts from the clip's own frame rather than the pose `Pl_init` leaves, so raising the gun within the first few ticks of a game lands a few units off.
- **Stairs.** `pl_mv_07` climbs and descends them, but the shadow lies flat at the ground under the player rather than following the steps, and whether stairs are in reach goes by the floor the player's height is on rather than by `nFloor`, which nothing ports yet.
- **Falls and death.** `Pl_damage`'s falls (`Routine_1` 4 and 5) play as the plain hit, without their `pSub1` clips or getting up, and `Pl_dead` leaves the player standing. `Pl_dead`'s own game over runs `Die` from falling below zero, not from `Pl_life_down`'s flag.
- **Sounds of hits.** `Pl_damage` plays none of its sounds.
- **What the player's routine leaves out.** Water and `Lim_dir_ck`'s clip swap aren't ported. The neck follows `plc_neck`, but `Pl_neck`'s aim at the nearest enemy waits for enemies.
- **Climbing.** `Oma_pl_updown_ck` reads only the room's SCA records, not object models, and treats every shape as its box.

- **The game over.** `Die`'s timing, stops, music cut and white-out are the original's, but what it shows isn't ported yet: `Die_move_set_cut`'s spiralling camera, `Die_light_set`, `Die_enemy_set`, `Die_snd_trans` and the `Die_spr_*` sprites and blood, whose `Rnd` draws are also missing, so the shared stream parts from the original 313 ticks into the sequence. The room's own camera stays up and the game returns to the title. `Game_loop`'s checks before starting `Die` (no other task running, the player's cut) are skipped.

## Collision

- **The floor.** A body's floor comes from its height. The original keeps it in `nFloor`, which rounds differently on a slope (`Sca_hit_slope`).
- **Objects and other bodies.** Only the room's SCA records block. `Sca_ck_hit`'s object models (`Omd_in_check`) and `Oba_ck_em2`'s other bodies don't.
- **Offsets.** `Sca_hit_*` add `Atd`'s `Ofs_x` and `Ofs_z`, which are zero for the player. Other bodies will need them.

## Doors

- **The reach and auto probes.** The bits of an area's `sat` byte are reduced to RE1's action-key, position and reach probes. The other triggers (the low nibble's non-player bits) are ignored.
- **Locked doors.** The key messages are the executable's wording, written from memory (`games/re2/language/en-US.txt`), and the door plays no lock sound.
- **The door scripts' other inputs.** The knock type isn't used.
- **The hold before a door.** `Psx_main` takes 8 off every channel of the frozen frame per tick. Here it's a black veil of that weight, which is the same at both ends and differs a little between.
- **Door timing.** The ticks are the original's for room 100's door. `Door_load`'s own sleeps depend on the door's sound bank, and no other door has been traced.

## Scripts

- **Sleeps in an init procedure.** They don't wait: a room's init runs to its end inside the load.
- **Script characters.** The characters `_Spl` runs (ids 0x40 to 0x5B, from `sce_em_set`) take `work_set 3`, move on `Spl_Scenario` with their own speeds, turns and banks and stop on the records of `Sca_ck_em`'s mask, and room 101's scene with Kendo matches the original tick for tick. They have no `Spl_Auto` yet, so a character outside a script's `plc_*` stands still, and no `Oba_ck_em` or footsteps of their own. `parts_set` only does its case 2, a part's `Be_flg`, which hides and shows it: its cases for a part's colour, attribute and rotation, in rooms 106 and 112's loops, do nothing. A `plc_neck` aimed at someone not modelled, such as a zombie, leaves the head where it was. Their `Spd` isn't compared: `Spl_Init`'s `Joint_move` leaves values in it that nothing reads before `Spl_sce*` sets them. `splayer_work` (`sce_em_set` slot 255, `work_set 2`) is one of them, with its own slot and stop bit: Sherry's scene in room 114 matches too. The enemies other than zombies aren't modelled: their `plc_dest` arrives at once, `member_cmp` on one always answers false and any `member_set` of one is dropped, so a script waiting on one's motion moves on. `tests/real/re2_real.rs` runs every main procedure and names those still holding (113, none in the rooms from the start to the police station).
- **Zombies.** `Em_Zombie` runs ids 0x10 to 0x1F: `Em10_Init`, standing, wandering, chasing, lying and getting up, on `Root_ck`'s line of sight and guide areas, sharing `Rnd` with the effects. Six zombies left alone in room 100 match the original tick for tick until the first one reaches the player, apart from one angle on the first tick, because the original's player has already been pushed out of the wall where arklay's hasn't. They push the player (`Oba_ck_em2`), grab, bite and are shoved off, and a kill runs the player's death in their hands, their letting the body down and the others coming over to eat it, all matching the original through 1,765 ticks of `street_shoot.txt`. The `hard_mode`, `censorship_off`, `WORD_0098eb22 == 3` and `FUN_0050aaa0` branches of `Pl_life_down` are taken as off, and the inventory can still be opened during a grab. The handgun's shots hit them through `weapon2`: `Weapon_at_ck`, `Em_ck`, `Hit_scan_ck`, `Em10_Damage`, `Em10_Die` and the corpse match the original through a whole fight, damage, stagger, lost arms, blood, falling, death and the pool (`street_shoot.txt`). Other weapons' `Em10Dm02` to `Em10Dm08` aren't ported, and neither are `Water` checks on a dying zombie, `Sca_ck_hit`'s object models and its flag bit 2 corner case, or `FUN_0045b330`, `FUN_0045b870` and `FUN_0045bd20`'s body-part and burn extras. A thrown arm is only hidden, not a part of its own, the parts' `Be_flg |= 0x8000` at death isn't set, and blood starts at its part but doesn't follow the matrix. The censored blood colours aren't used. `Pl_neck` doesn't aim at them. `Em10_Init`'s skins are drawn and a burnt zombie's fire follows its body, but its darker colours are a tint of the whole model, where the original's differ by part. `Esp_call`'s spit and a fire's `Esp_19` water check aren't ported, though their draws of `Rnd` are made, and neither are `Esp_05`'s splash or `Esp_07`'s `Game._1_1_`. Their moans use the player's sound banks' numbering: enemy banks aren't loaded.
- **Unmodelled instructions.** They do nothing and log once. That includes enemies other than zombies and water and movement areas (`sce_at`'s other kinds).
- **Scenario B.** It starts in room 104 with the scenario flag set. Nothing ties it to a finished Scenario A, and there is no zapping.

## Sound

- **The door's room load.** The original with sound holds `Set_room` for `Snd_bgm_ck`'s 91 ticks. Here the room loads when it does without sound, and only the music waits, so nothing else in the door moves. `Door_snd_trans`, which rewrites the table once seven of the item flags 0xF8 to 0xFF are set in a game with `fg_system`'s `0x1000000`, isn't ported: nothing sets that bit yet.
- **Music's stereo.** Tracks are mixed to mono and placed with the script's pan.
- **Sound effects.** `Snd_se_on`'s room-specific rules and loops aren't ported, nor `Sca_ck_line`'s muffling through walls, and enemies' banks aren't loaded. `se_on` from an object's work uses the player's position.
- **Footsteps.** `Floor_check`'s zones choose the sound. Water, `+0x1e4`'s case and the core slots aren't ported, and `flr_set` does nothing.
- **Room 11D.** The retail PC data has no `room11d.sap`, though its script plays slot 33, so the conversion takes the PlayStation samples in the room file, at a fixed 22050 Hz.

## Effects

- **Which effects move.** `Esp_call`'s behaviours 0 to 4, 6, 0xB, 0xC and 0xD are ported, which covers room 100's fire and smoke. The others only play their frames in place and log once.
- **3D effects.** `Esp_trans_3d`'s sprites aren't drawn.
- **Attached effects.** An effect on a body's matrix is placed as if on the world's.
- **The sheets.** A room's effects use the low-resolution TIMs in its RDT, not `espdat1` and `espdat2`. Of the shared sheets only page 0x1E's top 168 rows are converted.
- **Randomness.** Effects draw from their own stream, not `Rnd`.
- **Collisions and pauses.** Effects don't hit water or SCA records, and `fg_stop` doesn't stop them.
- **Smoothing.** Effect quads aren't tweened at high frame rates.

## Game flow

- **Debug jumps.** The player arrives through the door back from a neighbour room. With no such door the player stands in the room's own first door, and with no door at all under the first camera. The cut is the first of the door's, the zone's and the rest that shows the player. The original has no such jump. In rooms 200 and 500 no cut shows the placement.

- **Pick-ups.** The item goes straight into the inventory with a "You picked up" message. There is no pick-up screen or item model.
- **Messages.** The ones the executable draws itself are written from memory, and every message uses RE1's Western font. A room's text is its RDT's English list (`rdt + 0x40`), where the executable reads the list before it (`rdt + 0x3C`), so a line with text reveals at the English length; a voiced line has none in either list, so its timing is the original's. `xa_on` plays its line and `wsleep` waits for it, for as long as the line plays with sound, and for `Xa_control`'s count without (half that), so a trace matches the reference, which runs without sound.
- **Start items.** `games/re2/game/new_game.lua` copies `Stage_init_item`'s table for Leon and Claire, with the lighter or lockpick in slot 2 rather than slot 10, and no other character's or mode's row.
- **Text screens.** The title, new-game choice, inventory and item box are plain text stand-ins (`games/re2/ui`), and the save screen has no background: RE2's own art is not converted.
- **The inventory.** Confirm equips a weapon or uses a healing item. There is no CHECK, MOVE, combining, map or file tab, and equipping changes nothing about the weapon the player holds.
- **Healing.** Herbs and first aid heal in RE1's thirds of the maximum, not RE2's amounts (`item_definitions`).
- **Item definitions.** Only weapons, ammo and healing items have a kind; everything else can't be used from the inventory. Key items work from the door or script that wants them.
- **Save labels.** The save screen names the room by its number, not its place.
- **Item stacks.** Ammo stacks to 255 and stacks into any stack of its own kind.
