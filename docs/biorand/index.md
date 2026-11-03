# BioRand

BioRand generation is being ported to the new randomizer
(`crates/arklay-biorand`). Until the port lands, the launcher's BioRand screen,
`arklay biorand generate` and `arklay biorand verify` refuse with a message. The
plan is in [`plan.md`](plan.md).

Conversion packs the randomizer's inputs into the base pack: each room's
`random.xml` as `room/<id>/random.xml`, and the map-wide files from
`games/re1/biorand/` as `biorand/map.xml`. The schema of both is documented in
`crates/arklay-biorand/src/map.rs`.

A generated game is an ordinary base pack, `games/biorand-<seed>.akpak`, with id
`biorand-<seed>` and the seed padded to six digits. Its manifest records `seed`,
`generator_version` and `character`. A new generator version or conversion makes
the pack stale. Saves belong to their pack: `re1` keeps `save_dir` itself, and
every other base pack, RE2 and generated games included, uses
`<save_dir>/<pack id>/`. Screen scripts' `<save_dir>` means that folder.

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
