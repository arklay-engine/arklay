# RE2 deviations

RE2 play is being ported to exactness (Phase C of #244), checked tick by tick against the original with `scripts/oracle-re2.py` (`oracle/re2.md`). Every place where RE2 play knowingly still differs from the original is listed here, so that the rest of the port has the list to work from.

## Player

The pad's walking, running, backing, turning, hurt clips and scripted motions (`player2`, a port of `Player_move`), collision and `Sce_Damage` areas match the original on random walks around room 100 for both characters.

- **The held weapon.** Equipping from the inventory or the script's `weapon_chg` changes the hand and the walk's clips (`Pl_weapon_ch`), but `Plw_init_tbl`'s per-weapon set-up and `Weapon_cls` aren't ported.
- **The guns' stance only.** `Pl_move`'s routine 5 is `pl_mv05_w1` for Leon's and Claire's starting handguns, the magnums and the shotguns: raising, tilting, firing, reloading and lowering, with each weapon's timing, foot, kick, ammunition, flash, smoke, shell and shot rectangles. The knife, the custom handgun, the Colt, the bowgun, the grenade launchers and the heavier weapons don't raise. Firing is the original's, with `Shoot_camera`'s sparks when the gun lines up with the camera (checked by moving the camera on to the gun's line in both, since a real cut hardly ever lines up).
- **What a magnum or shotgun does to a zombie's body is only partly drawn.** `Em10Dm05`, `Em10Dm02` and their deaths run as the original does, a thrown arm falls and bleeds as `Parts_down` has it, and an arm the shove pulls off (`FUN_0045bd20`) follows the player's hands and comes away as it does in the original. Parts that `Be_flg` 0x4A blows apart (`Parts_bomb`) are just hidden. These aren't drawn: `Em10Dm02`'s body tilt, the torn body's and the stump's mesh swaps, the scaling of what's left where a part came off, and a thrown part's own lighting. `Esp_kill` doesn't put out the flames on a lost part. A thrown part's wall check passes its last position as `Sca_ck_info`'s previous one, which wasn't compared against the original. The water branches and `Routine_1` 9 and 0xC paths of these reactions aren't ported.
- **The licker is only on the floor.** `Em_22` matches the original on the floor: stalking, listening, tongue, slash, jump and bite, hurt, flinch and its deaths. A licker the scripts start on a wall or the ceiling doesn't spawn, and one that would climb stays down, because the body's `Super_matrix` and `Routine_1` 1, 7, 0xB, 0xD, 0x10-0x12 and 0x15-0x17, `Em22_Damage` 2 and 5 and `Em22_Die` 2-4 aren't ported. Rooms 10A, 10B, 110, 201, 204, 20E and 215 are the ones affected.
- **What a licker doesn't show.** The tongue (part 0x16, drawn by `Sa_parts_allmv`) is hidden, though it hits when it should. `Em22_Pldm` and `fg_system`'s decapitation aren't ported, so its killing bite plays as a plain death. Its dying and burning colours (`FUN_0046ed90`, `FUN_0046b840`) aren't drawn. A partner isn't a target. At some angles a claw (part 6) shows a small gap, so a finger looks detached; the cause isn't known.
- **The dropped magazine.** `Mag_set` and `Mag_down` match, but the part's place under the hand was only read from the original for Leon's weapons 2 and 5, and it's assumed for the rest. Claire's isn't checked.
- **The pose before play.** The first tick's blend starts from the clip's own frame rather than the pose `Pl_init` leaves, so raising the gun within the first few ticks of a game lands a few units off.
- **Stairs.** `pl_mv_07` climbs and descends them, but the shadow lies flat at the ground under the player rather than following the steps, and whether stairs are in reach goes by the floor the player's height is on rather than by `nFloor`, which nothing ports yet.
- **Falls and death.** `Pl_damage`'s falls (`Routine_1` 4 and 5) play as the plain hit, without their clips or getting up: those clips are `pSub1`'s, which only an enemy's grab sets, and no enemy that uses them is ported. `Pl_die` falls and cries as the original does, checked by forcing the routine in both; `Pl_dead` doesn't grow the shadow into its pool of blood, which `Die_vcut_set` culls anyway. `Pl_dead`'s own game over runs `Die` from falling below zero, not from `Pl_life_down`'s flag.
- **What the player's routine leaves out.** Water and `Lim_dir_ck`'s clip swap aren't ported. The neck follows `plc_neck` and `Pl_neck`'s aim at the nearest enemy, matching the original's flags and whether it has a target through all of `street_shoot.txt`. Its turn is usually within a few units of 4096, as far as 47 when the target's head moves fast, because the port places a head from the pose where the original uses its last drawn matrix. Only bodies arklay models can be looked at.
- **Climbing.** `Oma_pl_updown_ck` reads only the room's SCA records, not object models, and treats every shape as its box.

- **The game over.** `Die`'s timing, stops, music cut, white-out and `YOU DIED` text (`Die_spr_*`, with its `Rnd` draws) are the original's, on the room hidden as `Die_vcut_set` and `Bg_set_mode` hide it, and the game returns to the title. This is the original's behaviour with `censorship_off` clear. With it set the original shows the room's models from `Die_move_set_cut`'s spiralling camera under `Die_light_set`, with `Die_blood_move_main`'s blood; neither is ported. The text's corners are drawn as triangles without the original's depth order, and `Die_sprt_load`'s check for an ally's death isn't ported. `Game_loop`'s checks before starting `Die` (no other task running, the player's cut) are skipped.

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
- **Zombies.** `Em_Zombie` runs ids 0x10 to 0x1F: `Em10_Init`, standing, wandering, chasing, lying and getting up, on `Root_ck`'s line of sight and guide areas, sharing `Rnd` with the effects. Six zombies left alone in room 100 match the original tick for tick until the first one reaches the player, apart from one angle on the first tick, because the original's player has already been pushed out of the wall where arklay's hasn't. They push the player (`Oba_ck_em2`), grab, bite and are shoved off, and a kill runs the player's death in their hands, their letting the body down and the others coming over to eat it, all matching the original through 1,997 ticks of `street_shoot.txt`, the end of the game over. The `hard_mode`, `censorship_off`, `WORD_0098eb22 == 3` and `FUN_0050aaa0` branches of `Pl_life_down` are taken as off, and the inventory can still be opened during a grab. The handgun's shots hit them through `weapon2`: `Weapon_at_ck`, `Em_ck`, `Hit_scan_ck`, `Em10_Damage`, `Em10_Die` and the corpse match the original through a whole fight, damage, stagger, lost arms, blood, falling, death and the pool (`street_shoot.txt`). So do the magnums' and shotguns' in twelve fights, one for each gun at each aim, down to the arm pulled off in a shove. Those fights never reach `Em10_Damage`'s `Em10Dm05` and `Em10Dm02`, because each of these shots kills, so those two entries are ported but unchecked as damage. Other weapons' `Em10Dm03`, `Em10Dm04`, `Em10Dm06` to `Em10Dm08` aren't ported, and neither are `Water` checks on a dying zombie, `Sca_ck_hit`'s object models and its flag bit 2 corner case, or `FUN_0045b330` and `FUN_0045b870`'s body-part and burn extras. The parts' `Be_flg |= 0x8000` at death isn't set, and blood starts at its part but doesn't follow the matrix. The censored blood colours aren't used. `Em10_Init`'s skins are drawn and a burnt zombie's fire follows its body, but its darker colours, and the charring of its corpse, are a tint of the whole model, where the original's differ by part. `Esp_call`'s spit and a fire's `Esp_19` water check aren't ported, though their draws of `Rnd` are made, and neither are `Esp_05`'s splash or `Esp_07`'s `Game._1_1_`. Their sounds play from the room's enemy bank (`Sce_se_set`, `Snd_load_em`) and match the original's `SsPlay`s, type, slot, volume and pan, in `street_shoot.txt`, `street_reload.txt`, `street_walk.txt` and `bin_up_and_down.txt`, apart from a few moving zombies' pan or volume being one out: the port sounds them from the end of the tick's move, where the original can do so part way through. `Sce_em_set2` isn't ported, so a room with those enemies can pick the wrong bank.
- **Unmodelled instructions.** They do nothing and log once. That includes enemies other than zombies and water and movement areas (`sce_at`'s other kinds).
- **Scenario B.** It starts in room 104 with the scenario flag set. Nothing ties it to a finished Scenario A, and there is no zapping.

## Sound

- **The door's room load.** The original with sound holds `Set_room` for `Snd_bgm_ck`'s 91 ticks. Here the room loads when it does without sound, and only the music waits, so nothing else in the door moves. `Door_snd_trans`, which rewrites the table once seven of the item flags 0xF8 to 0xFF are set in a game with `fg_system`'s `0x1000000`, isn't ported: nothing sets that bit yet.
- **Music's stereo.** Tracks are mixed to mono and placed with the script's pan.
- **Sound effects.** `Snd_se_on`'s room-specific rules and loops aren't ported. `se_on` from an object's work uses the player's position.
- **Footsteps.** `Floor_check`'s zones choose the sound. Water, `+0x1e4`'s case and the core slots aren't ported, and `flr_set` does nothing.
- **Room 11D.** The retail PC data has no `room11d.sap`, though its script plays slot 33, so the conversion takes the PlayStation samples in the room file, at a fixed 22050 Hz.

## Effects

- **Which effects move.** `Esp_call`'s behaviours 0 to 4, 6, 0xB, 0xC and 0xD are ported, which covers room 100's fire and smoke. The others only play their frames in place and log once.
- **3D effects.** `Esp_trans_3d`'s sprites aren't drawn.
- **Attached effects.** An effect on a body's matrix is placed as if on the world's.
- **The sheets.** A room's effects use the low-resolution TIMs in its RDT, not `espdat1` and `espdat2`, and the shared ones `espcore.bin`'s six low-resolution TIMs (`Graphicsprdata == 0`), not `tex216.adt` and `tex316.adt`.
- **Randomness.** Effects draw from their own stream, not `Rnd`.
- **Collisions and pauses.** Effects don't hit water or SCA records, and `fg_stop` doesn't stop them.
- **Smoothing.** Effect quads aren't tweened at high frame rates.

## Game flow

- **Debug jumps.** The player arrives through the door back from a neighbour room. With no such door the player stands in the room's own first door, and with no door at all under the first camera. The cut is the first of the door's, the zone's and the rest that shows the player. The original has no such jump. In rooms 200 and 500 no cut shows the placement.

- **Pick-ups.** The item goes straight into the inventory with a "You picked up" message. There is no pick-up screen or item model.
- **Messages.** The ones the executable draws itself are written from memory, and every message uses RE1's Western font. A room's text is its RDT's English list (`rdt + 0x40`), where the executable reads the list before it (`rdt + 0x3C`), so a line with text reveals at the English length; a voiced line has none in either list, so its timing is the original's. `xa_on` plays its line and `wsleep` waits for it, for as long as the line plays with sound, and for `Xa_control`'s count without (half that), so a trace matches the reference, which runs without sound.
- **Start items.** `games/re2/game/new_game.lua` copies `Stage_init_item`'s table for Leon and Claire, with the lighter or lockpick in slot 2 rather than slot 10, and no other character's or mode's row.
- **Text screens.** The title, new-game choice, inventory and item box are plain text stand-ins (`games/re2/ui`), and the save screen has no background: RE2's own art is not converted.
- **The inventory.** Confirm equips a weapon or uses a healing item. There is no CHECK, MOVE, combining, map or file tab, and equipping fires only the handguns, magnums and shotguns.
- **Healing.** Herbs and first aid heal in RE1's thirds of the maximum, not RE2's amounts (`item_definitions`).
- **Item definitions.** Only weapons, ammo and healing items have a kind; everything else can't be used from the inventory. Key items work from the door or script that wants them.
- **Save labels.** The save screen names the room by its number, not its place.
- **Item stacks.** Ammo stacks to 255 and stacks into any stack of its own kind.
