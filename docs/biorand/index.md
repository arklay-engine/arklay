# BioRand

The launcher's BioRand screen, or `arklay biorand generate <base pack> --seed N`,
makes a standalone game from the installed RE1 pack: about a dozen rooms from
`118` to the helipad `303`, with its own door graph, pickups, enemies and player
(Jill or Chris). It is a proof of concept, and the room choice is simple.

The result is an ordinary base pack, `games/biorand-<seed>.akpak` with id
`biorand-<seed>` and the seed padded to six digits, that appears in the Play
list. Its manifest records `seed`, `generator_version` and `character`. The same
seed and generator version give the same bytes. A new generator version or
conversion makes the pack stale, and the launcher rebuilds it from the seed.
Saves belong to their pack: `re1` keeps `save_dir` itself, and every other base
pack, RE2 and generated games included, uses `<save_dir>/<pack id>/`. Screen
scripts' `<save_dir>` means that folder.

Generation replaces the SCD of each room it uses and never patches bytes. A
`verify` pass then reads those scripts back from the finished pack, and a pack
that fails is deleted.

## `random.xml`

Each room that can be randomized has a `random.xml` beside its `room.akscr`.
The generator reads only this file for the route, not the script:

```xml
<random>
  <room name="B1 WEST" tags="visit2"/>
  <doors>
    <door id="0" target="21A:1" kind="normal">
      <entrance x="27500" y="0" z="12600" d="2048" cut="7"/>
    </door>
  </doors>
  <items>
    <item id="2" name="GREEN HERB/1"/>
  </items>
  <enemies exclude="4">
    <spot x="12869" y="0" z="4204" d="87"/>
  </enemies>
</random>
```

`id` is the AOT number of the door or item in the script. `target` is the door
the original leads to, as `room:door`: a note for whoever edits the file. An
`entrance` is where a player arriving through the door stands and the camera they
see. An item keeps its slot's world model, and only the pickup changes. `exclude`
and a spot's `include` hold enemy ids in hex, and `<enemies fixed="true"/>` keeps
the room's own enemies. The `<room>` tags are `start`, `goal` and `visit2`, for a
room whose scripts run only on the second mansion visit.

The verifier refuses a pack whose door or `random.xml` item depends on a story
flag, because the route can't wait for the story. Guard such a script for the
randomizer.

## Script guards

`if (RANDOMIZER)`, `if (DOOR_RANDOMIZER)` and `if (ENEMY_RANDOMIZER)` test system
flags in bank 5 (`0xF0` to `0xF2`), which the engine sets from a manifest with a
`seed`. They are false in the vanilla game, and a room's retail-identical variant
build folds them to the `else` arm, so a guard does not change those bytes.

## Checks

`arklay biorand verify <pack>` checks from the pack's scripts that every room and
key can be reached, each key before its lock, and the flare and goal room too. A
bot plays a real pack through the real app, from `118` to the Tyrant's death and
the ending:

```
ARKLAY_BIORAND_PACK=games/biorand-123456.akpak \
  cargo test --profile test-release --lib biorand_playthrough -- --ignored --nocapture
```

It stands the player in place rather than walking, keeps their health up and
removes enemies after watching them, so it does not show that a game is
walkable or fair.

