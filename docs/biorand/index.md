# BioRand

BioRand is ported to the new randomizer (`crates/arklay-biorand`) in
milestones; the plan is in [`plan.md`](plan.md). Today a
generated game randomizes the items, the start inventory, the doors
(`doors/random`, on by default; it needs `items/random`), the locks
(`locks/random`) and the enemies (`enemies/random`).

Conversion packs the randomizer's inputs into the base pack: each room's
`random.xml` as `room/<id>/random.xml`, and the map-wide files from
`games/re1/biorand/` as `biorand/map.xml`. The schema of both is documented in
`crates/arklay-biorand/src/map.rs`.

```sh
arklay biorand generate re1.akpak --seed 123456 [--player chris|jill|both|configured]
                        [--config config.json] [--out-dir DIR] [--force] [--jobs N]
arklay biorand verify biorand-123456.akpak [--base re1.akpak]
```

`generate` writes `biorand-<seed>.akpak` (for `--player both`, one pack per
player: `biorand-<seed>-chris.akpak` and `-jill.akpak`) beside the base pack
or in `--out-dir`, prints each path, and verifies each pack, deleting one that
fails. `--config` takes BioRand's configuration JSON, a flat object of option
ids and values (the ids are in `crates/arklay-biorand/src/config.rs`); options
it leaves out keep their defaults. Without `--player`, the configuration's
`player` decides, drawn from the seed when it is `Random` (the default). The
same seed, configuration and generator always make the same bytes. `verify`
plans the game again from what the pack records and checks it, and names
every problem it finds. Given the base pack (by default `re1.akpak` beside the
generated one), it also checks that nothing outside the planned bytes differs.
It also runs a static audit of the plan (a route walk, item, enemy, door and
lock checks); problems print as `audit: ...` and exit non-zero, and a route
that can strand a player who spends a consumable key wrongly prints a
`warning:` line.

A generated game is an ordinary base pack with id `biorand-<seed>`, the seed
padded to six digits. It is its base pack with the planned pickups written
into the room scripts' `item_aot_set`s and the planned enemies written in at
the rooms' `random_enemies()` markers (see below), and these entries added:

- `manifest.toml`: the base's, with `seed`, `generator_version`,
  `plan_version`, `character`, `biorand_flags` (the system flags it raises),
  and the new game's setup: `scenario_flags` (bank-0 flags raised or cleared,
  as `"122=1"`), `start_items` (the player's inventory, as `"item:amount"`,
  in place of what `new_game.lua` gives) and `chris_slots`/`jill_slots`
  (inventory slot counts other than the original's six and eight). Only a
  base pack may carry them.
- `biorand/request.json`: the seed, the players and the configuration asked
  for, so `verify` can plan the game again.
- `biorand/spoiler.json` and `biorand/spoiler.txt`: the start inventory, where
  every key is, the locked doors, the room ids and doors (with random
  doors), and every room's pickups.

With random doors the rooms are renumbered: segment *n* of the route is
stage *n*, numbered in the order the route reaches its rooms. Each room the
game uses is its base folder copied to `room/<new id>/`, with
`<room origin="...">` and a `<doors>` entry for each door whose target byte
would not reach its planned room (see [Room definitions](#room-definitions)). Each planned
door's `door_aot_set` holds the arrival at its far door (position, facing
and camera cut) and keeps its animation and sound; a door leading out of
the game is sealed (lock and key 255). Room 311 keeps its folder and id.
Rooms the game leaves out are not in the pack, and every room's
`random.xml` is at `biorand/rooms/<dir>.xml` instead. The manifest's
`start_room` is the main hall's new id, and the spoiler lists the room ids
and the doors.

A new generator or plan version, or a new conversion, makes the pack stale.
Saves belong to their pack: `re1` keeps `save_dir` itself, and every other
base pack, RE2 and generated games included, uses `<save_dir>/<pack id>/`.
Screen scripts' `<save_dir>` means that folder. A Chris with eight slots keeps
Rebecca's overlapped save slots in an extra save trailer (`AKY\x04`), so a
six-slot game's saves stay byte-identical to the original's. Likewise a save
in a room the block cannot name (a long id) keeps it in the `AKY\x05`
trailer, and the block names the room's origin.

Under `DOOR_RANDOMIZER` the map screen is off.

## Script guards

Each system flag is tested by name, as in `if (RANDOMIZER)` or
`if (!ITEM_RANDOMIZER)`. They are bank 5 bits `0xC0` to `0xCD`, in the range
BioRand reserves: `0xCE` and `0xCF` are reserved for more engine behaviour, and
`0xD0` to `0xFF` are spare. The flags are `RANDOMIZER`, `DOOR_RANDOMIZER`,
`ENEMY_RANDOMIZER`, `ITEM_RANDOMIZER`, `LOCK_RANDOMIZER`, `CUTSCENES_DISABLED`,
`FORCE_TYRANT2` and `LAB`, then the engine-behaviour flags
`BR_LOCK_FULL_BYTE`, `BR_NEPTUNE_KILLABLE`, `BR_YAWN_POISON`, `BR_HERB_MIXES`,
`BR_INK_FLAG` and `BR_NO_TITLE_DEMO`. The engine raises `RANDOMIZER` for a
manifest with a `seed`, and every name in its `biorand_flags` list. A seeded
manifest without that list (the proof of concept's packs) also raises
`DOOR_RANDOMIZER` and `ENEMY_RANDOMIZER`. The flags are clear in the vanilla
game. A room's retail-identical variant build folds each guard to the branch
that the clear flag takes, so a guard does not change those bytes.

A system flag is tested alone: `if (!CUTSCENES_DISABLED && ck(...))` is an error
(a variant test and a flag test cannot mix with other conditions). Nest the
tests, and keep each guarded block under a command `if`'s one-byte jump.

## Item puzzles: `picked()`

A room that reacts to the item type just picked up (`testpickup(IT_..)` or
`cmpb(B_PICKED_ITEM, CMP_EQ, IT_..)`) breaks when item randomization puts
another item in the slot. `if (picked(AOT_04_ITEM))` names the slot's item
AOT instead; the compiler finds that AOT's `item_aot_set` for the item type and
the pickup flag.

- `picked(AOT)` stands for `testpickup(<the AOT's item type>)`. It is an error
  if no `item_aot_set` sets the AOT (so a non-item AOT fails), or if it sets
  it with several items or flags.
- `picked(AOT, <original>)` keeps the room's own test, `testpickup(ITEM)` or
  `cmpb(B_PICKED_ITEM, CMP_EQ, ITEM)`, which a retail build emits as written.
  Its item also picks the `item_aot_set` when the room reuses an AOT index for
  several items (room 106's AOT 6).
- A retail variant build (`--room`) is exactly the original test, as with
  every guard. The runtime build puts the following before the `if`, which
  makes the original test see a randomized pickup as the original item:

  ```
  if (ITEM_RANDOMIZER && cmpb(B_PICKED_AOT, CMP_EQ, <AOT> + 1)) {
      setb(B_PICKED_ITEM, <original item>, 0);
  }
  ```

  `B_PICKED_AOT` is an Arklay state byte (20): the engine sets it to the
  picked item AOT's id plus one (0 is no pickup) whenever it sets
  `B_PICKED_ITEM`, and clears it with it after the frame. So the remap fires
  for a pickup of exactly that AOT, and never for another pickup in the room,
  collected or not. (`B_FWD_AOT` cannot do this: it lasts the one frame of the
  action press, and the pickup runs frames later after the prompt.) An AOT
  that the room reuses for several items (room 106's AOT 6) also tests the
  item's own pickup flag, `ck(FG_ITEM, <flag>, TRUE)`.

  The test is not an `if (...) {} else {}` copy of the bodies, which would
  double each body in the script.
- `picked_as(AOT, ITEM);` is the remap alone, for a test far from its `if`
  (room 10D). It folds away in a retail build.


## Enemies: `random_enemies()`

A room script says where the enemy randomizer's new enemies go with
`random_enemies();`, inside the branch where the script sets its own enemies,
so story and player conditions come for free. The room's own `enemy(...)`
commands go under `if (!ENEMY_RANDOMIZER)`:

```
if (ck(FG_SCENARIO, F_SCENARIO_05, FALSE))
{
    if (!ENEMY_RANDOMIZER)
    {
        enemy(...);
    }
    random_enemies();
}
```

- A retail variant build (`--room`) compiles `random_enemies()` to nothing and
  folds the guard, so a room's retail bytes do not change.
- The runtime (base pack) build compiles it to a four-byte no-op marker,
  `nop_wide` with operands `0xEA` and the placeholder's id. The unnamed
  placeholder's id is 0; `random_enemies(name)` is for a room that needs enemies
  in more than one branch, and its id is `placeholder_id(name)` (FNV-1a of the
  name folded to 16 bits, never 0). A spot of the room's `random.xml` names its
  marker with its `placeholder` attribute; a spot with none belongs to the
  unnamed marker. A marker name is unique within a room.
- The generator rewrites each marker the plan creates enemies at, in place, to
  an `evt_exec` of a new event script at the end of the event table. That
  script is an `evt_single` of an `enemy` per created enemy (its type, pose,
  position, facing, kill flag and free slot) and `evt_finish`. A marker the plan
  does not use stays a no-op. Enemies the plan retypes in place (the fixed
  ones `random.xml` lists with an id) get their new type and the state staging
  gives them. A created enemy keeps its planned slot unless a fixed enemy that
  stays (a prop or a boss) holds it, and then takes the lowest free one. A
  retype never touches a person (an `enemy` of type 0x20 or above): a slot
  can be an enemy for one character and a person for the other (30A's slot 1
  is Chris's hunter and Jill's Barry), and a retyped Barry left Enrico's
  scene waiting for good.
- A kind with weight 0 in the configuration is never placed. Staging falls
  back to the lowest-weight kind when a room supports no kind with a weight;
  here such a room gets no enemies, so a configuration of zombies only has
  zombies only.
- A game that randomizes enemies raises `ENEMY_RANDOMIZER`, so the original
  `enemy(...)` commands under the guard are skipped. A room with enemy rules
  in `random.xml` that the plan gives no enemies is therefore empty, as in
  staging, which clears every such room before it places anything:
  `enemies/rooms` is the share of rooms with enemies. In a room with no
  enemy rules the generator flips the sense of every `ENEMY_RANDOMIZER` test,
  so the room runs as if the flag were clear and its original enemies stay.
- Effect sheets: a created enemy's effect sprites are not added to rooms that
  do not declare them. The engine skips such an effect with a warning
  (`effect sprite N is not declared by this room`) and the enemy plays
  without it. This is cosmetic.
