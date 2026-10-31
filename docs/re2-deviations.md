# RE2 deviations

RE2 play is being ported to exactness (Phase C of #244), checked tick by tick against the original with `scripts/oracle-re2.py` (`oracle/re2.md`). Every place where RE2 play knowingly still differs from the original is listed here, so that the rest of the port has the list to work from.

## Player

The pad's walking, running, backing, turning, hurt clips and scripted motions (`player2`, a port of `Player_move`), collision and `Sce_Damage` areas match the original on random walks around room 100 for both characters.

- **The held weapon.** The player always holds the starting handgun, whose bank has the locomotion the knife's lacks.
- **The handguns' stance only.** `Pl_move`'s routine 5 is `pl_mv05_w1` for Leon's and Claire's starting handguns: raising, tilting, firing, reloading and lowering, with their ammunition. The other weapons don't raise. A shot hits nothing and has no sound, flash, shell or camera kick, and neither do the click or the reload; `Em_ck`'s lock-on and the lock-drop button have nothing to do without enemies.
- **The pose before play.** The first tick's blend starts from the clip's own frame rather than the pose `Pl_init` leaves, so raising the gun within the first few ticks of a game lands a few units off.
- **Stairs.** `pl_mv_07` climbs and descends them, but its shadow's path isn't ported, and whether stairs are in reach goes by the floor the player's height is on rather than by `nFloor`, which nothing ports yet.
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
- **Script characters.** The characters `_Spl` runs (ids 0x40 to 0x5B, from `sce_em_set`) take `work_set 3`, move on `Spl_Scenario` with their own speeds, turns and banks and stop on the records of `Sca_ck_em`'s mask, and room 101's scene with Kendo matches the original tick for tick. They have no `Spl_Auto` yet, so a character outside a script's `plc_*` stands still, and no neck (`plc_neck`), `Oba_ck_em`, footsteps, `parts_set`, shadow or room lighting of their own. Their `Spd` isn't compared: `Spl_Init`'s `Joint_move` leaves values in it that nothing reads before `Spl_sce*` sets them. `splayer_work` (`sce_em_set` slot 255, `work_set 2`) is one of them, with its own slot and stop bit: Sherry's scene in room 114 matches too, past the voice line's wait below. The other enemies aren't modelled: their `plc_dest` arrives at once, `member_cmp` on one always answers false and any `member_set` of one is dropped, so a script waiting on one's motion moves on. `tests/real/re2_real.rs` runs every main procedure and names those still holding (113, none in the rooms from the start to the police station).
- **Unmodelled instructions.** They do nothing and log once. That includes enemies and NPCs (`sce_em_set`), `sce_bgm_control`, and water and movement areas (`sce_at`'s other kinds).
- **Scenario B.** It starts in room 104 with the scenario flag set. Nothing ties it to a finished Scenario A, and there is no zapping.

## Game flow

- **Pick-ups.** The item goes straight into the inventory with a "You picked up" message. There is no pick-up screen or item model.
- **Messages.** The ones the executable draws itself are written from memory, and every message uses RE1's Western font. A room's text is its RDT's English list (`rdt + 0x40`), where the executable reads the list before it (`rdt + 0x3C`), so a line with text reveals at the English length; a voiced line has none in either list, so its timing is the original's. A voice line lasts as long as its message: `wsleep` doesn't wait for the audio, as the original does when the audio outlasts the message.
- **Start items.** `games/re2/game/new_game.lua` copies `Stage_init_item`'s table for Leon and Claire, with the lighter or lockpick in slot 2 rather than slot 10, and no other character's or mode's row.
- **Text screens.** The title, new-game choice, inventory and item box are plain text stand-ins (`games/re2/ui`), and the save screen has no background: RE2's own art is not converted.
- **The inventory.** Confirm equips a weapon or uses a healing item. There is no CHECK, MOVE, combining, map or file tab, and equipping changes nothing about the weapon the player holds.
- **Healing.** Herbs and first aid heal in RE1's thirds of the maximum, not RE2's amounts (`item_definitions`).
- **Item definitions.** Only weapons, ammo and healing items have a kind; everything else can't be used from the inventory. Key items work from the door or script that wants them.
- **Save labels.** The save screen names the room by its number, not its place.
- **Item stacks.** Ammo stacks to 255 and stacks into any stack of its own kind.
