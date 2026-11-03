# BioRand

BioRand is ported to the new randomizer (`crates/arklay-biorand`) in
milestones; the plan is in [`plan.md`](plan.md). Today a
generated game randomizes the items and the start inventory; the doors, locks
and enemies stay the original's whatever the configuration asks.

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

A generated game is an ordinary base pack with id `biorand-<seed>`, the seed
padded to six digits. It is its base pack with the planned pickups written
into the room scripts' `item_aot_set`s, and these entries added:

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
  every key is, and every room's pickups.

A new generator or plan version, or a new conversion, makes the pack stale.
Saves belong to their pack: `re1` keeps `save_dir` itself, and every other
base pack, RE2 and generated games included, uses `<save_dir>/<pack id>/`.
Screen scripts' `<save_dir>` means that folder. A Chris with eight slots keeps
Rebecca's overlapped save slots in an extra save trailer (`AKY\x04`), so a
six-slot game's saves stay byte-identical to the original's.

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

  The test is not an `if (...) {} else {}` copy of the bodies: a command's
  `if` jumps by one byte, so a doubled body would not fit a long `main`.
- `picked_as(AOT, ITEM);` is the remap alone, for a test too far from its
  `if` to add 20 bytes (room 10D). It folds away in a retail build.
