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

`if (RANDOMIZER)`, `if (DOOR_RANDOMIZER)` and `if (ENEMY_RANDOMIZER)` test system
flags in bank 5 (`0xF0` to `0xF2`), which the engine sets from a manifest with a
`seed`. They are false in the vanilla game, and a room's retail-identical variant
build folds them to the `else` arm, so a guard does not change those bytes.
