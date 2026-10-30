# RE2 deviations

RE2 play is approximate at this stage (Phase B of #244): the engine reuses RE1's player machine and takes small lookups from bio2's PC 1.10 executable. Every place where RE2 play knowingly differs from the original is listed here, so that an exact port (Phase C) has the list to work from.

## Player

- **Movement speed.** The player machine keeps RE1's walk and run speed curves. The original reads each frame's speed from the animation (`Spd_get`).
- **Turning.** The player machine keeps RE1's turn rate. The original turns by `0x28`, `0x20` or `0x10` units per tick while walking and by `0x38`, `0x30` or `0x18` while running, by health.
- **Hurt animations.** The clips are the healthy character's. The original picks the walk, run and idle clips by health (`D_life_u`).
- **The held weapon.** The player always holds the handgun, whose bank has the locomotion the knife's lacks.
- **No aiming, no combat, no other weapons.** RE2's weapon clips aren't wired to the weapon machine.
- **Stairs.** The action key at a set of stairs puts the player straight at its other end. The original plays the climb (`Sca_hit_stairs`).

## Collision

- **Shapes other than boxes.** Boxes follow `Sca_hit_box`. Triangles, diamonds, discs and rounded boxes test the original's outline, and push straight out of it, where the original has its own slides.
- **Curves and shape 10.** Shape 13 records (`Sca_hit_curve`, 8 in the game) and shape 10 records (50) don't block or change the player's height.
- **Objects and other bodies.** Only the room's SCA records block. `Sca_ck_hit`'s object models (`Omd_in_check`) don't.

## Doors

- **The reach and auto probes.** The bits of an area's `sat` byte are reduced to RE1's action-key, position and reach probes. The other triggers (the low nibble's non-player bits) are ignored.
- **Locked doors.** The key messages are the executable's wording, written from memory (`games/re2/language/en-US.txt`), and the door plays no lock sound.
- **The door scripts' other inputs.** The knock type isn't used.

## Scripts

- **Sleeps in an init procedure.** They don't wait: a room's init runs to its end inside the load.
- **Cutscenes.** There are none. `plc_dest` moves the player to the end of the walk at once, `member_cmp` always answers false and any `member_set` of a script character is dropped, so a script waiting on a character's motion moves on. `tests/real/re2_real.rs` runs every main procedure and names those still holding (103, none in the rooms from the start to the police station).
- **Script waits on input.** `sce_key_ck` and `sce_trg_ck` accept any key.
- **Unmodelled instructions.** They do nothing and log once. That includes enemies and NPCs (`sce_em_set`), `sce_bgm_control`, and damage, water and movement areas (`sce_at`'s other kinds).
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
