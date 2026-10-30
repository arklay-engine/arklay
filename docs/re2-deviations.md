# RE2 deviations

RE2 play is being ported to exactness (Phase C of #244), checked tick by tick against the original with `scripts/oracle-re2.py` (`oracle/re2.md`). Every place where RE2 play knowingly still differs from the original is listed here, so that the rest of the port has the list to work from.

## Player

The pad's walking, running, backing, turning, hurt clips and scripted motions (`player2`, a port of `Player_move`), collision and `Sce_Damage` areas match the original on random walks around room 100 for both characters.

- **The held weapon.** The player always holds the starting handgun, whose bank has the locomotion the knife's lacks.
- **No aiming, no combat, no other weapons.** `Pl_move`'s routine 5 and RE2's weapon clips aren't wired up, and neither is the weapon machine.
- **Stairs.** The action key at a set of stairs puts the player straight at its other end. The original plays the climb (`Sca_hit_stairs`).
- **Falls and death.** `Pl_damage`'s falls (`Routine_1` 4 and 5) play as the plain hit, without their `pSub1` clips or getting up, and `Pl_dead` leaves the player standing with no game over.
- **Sounds of hits.** `Pl_damage` plays none of its sounds.
- **What the player's routine leaves out.** The neck, `Oma_*updown`, water and `Lim_dir_ck`'s clip swap aren't ported.

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
- **Script characters.** `plc_dest` and `plc_motion` move the player as the original does, but a non-player's arrive at once, `member_cmp` on one always answers false and any `member_set` of one is dropped, so a script waiting on a character's motion moves on. `tests/real/re2_real.rs` runs every main procedure and names those still holding (92, none in the rooms from the start to the police station).
- **Script waits on input.** `sce_key_ck` and `sce_trg_ck` accept any key.
- **Unmodelled instructions.** They do nothing and log once. That includes enemies and NPCs (`sce_em_set`), `sce_bgm_control`, and water and movement areas (`sce_at`'s other kinds).
- **Scenario B.** It starts in room 104 with the scenario flag set. Nothing ties it to a finished Scenario A, and there is no zapping.

## Game flow

- **Pick-ups.** The item goes straight into the inventory with a "You picked up" message. There is no pick-up screen or item model.
- **Messages.** The ones the executable draws itself are written from memory, and every message uses RE1's Western font. Rooms' yes/no prompts lose their choices.
- **Start items.** `games/re2/game/new_game.lua` is written from memory, not read from `Stage_init_item`.
- **Text screens.** The title, new-game choice, inventory and item box are plain text stand-ins (`games/re2/ui`), and the save screen has no background: RE2's own art is not converted.
- **The inventory.** Confirm equips a weapon or uses a healing item. There is no CHECK, MOVE, combining, map or file tab, and equipping changes nothing about the weapon the player holds.
- **Healing.** Herbs and first aid heal in RE1's thirds of the maximum, not RE2's amounts (`item_definitions`).
- **Item definitions.** Only weapons, ammo and healing items have a kind; everything else can't be used from the inventory. Key items work from the door or script that wants them.
- **Save labels.** The save screen names the room by its number, not its place.
- **Item stacks.** Ammo stacks to 255 and stacks into any stack of its own kind.
