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
- **Locked doors.** A locked door stays shut, silently. Keys and their messages are R8's.
- **The door scripts' other inputs.** The knock type isn't used.

## Scripts

- **Sleeps in an init procedure.** They don't wait: a room's init runs to its end inside the load.
