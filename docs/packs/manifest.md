# Manifest

`manifest.toml` sits at the top of a pack and says what the pack is.

```toml
[pack]
format = 1
id = "re1"
name = "Resident Evil"
version = "0.1.0"
kind = "base"
language = "en-US"
region = "USA"
start_room = "100"
```

The file is a small subset of TOML: comments, `key = value` with strings,
integers, booleans and arrays of strings (which may run over several lines
up to the closing `]`), and a single `[pack]` section.

| Key | Meaning |
| --- | --- |
| `format` | The manifest format: `1`. Required. |
| `id` | The pack's id. Mods name the game they are for by this id. Required. |
| `kind` | `"base"` for a game pack. Required. |
| `name`, `version` | Labels shown to players. Optional. |
| `description` | Longer text; `\n` breaks a line. Optional. |
| `authors`, `credits` | Who made the pack, and further acknowledgements, one string each. Optional. |
| `tags` | Free-form labels, one string each. `"official"` marks a pack made by `convert-game` and `"randomizer"` a generated one; the launcher's PLAY screen lists official games first. Optional. |
| `homepage` | The pack's home page URL. Optional. |
| `thumbnail` | The entry holding a 4:3 picture for the pack's card on the PLAY screen, such as `"ui/title.bmp"`. Optional. |
| `language` | The language of the pack's own room text, such as `"ja-JP"`. Optional. |
| `start_room` | The room a new game starts in, as a room id (`"100"`, `"G01"`). The player enters it as with `--room`. Optional. |
| `region` | `"USA"`, `"JPN"` or `"EUR"`, which room scripts can test with `if (USA)`. Default `"USA"`. |
| `lua` | The Lua hooks to run, in order. Left out or empty, every `lua/**.lua` file runs (see [Scripting](scripting.md#hooks)). |
| `engine` | A note about the engine build the pack is made for. Optional and not checked. |
| `convert_version`, `source_hash` | Written by `convert-game`, so the launcher can offer to rebuild an outdated pack. |

A pack has no key for its game: each model, room and sound file says what
format it is in by its own content.

A game pack without a manifest still loads, as `id` = the file name and
`kind = "base"`, with a warning.

## Generated games

A randomizer (BioRand) writes a game pack of its own, `biorand-<seed>`,
from a converted RE 1 pack. Its manifest is the base pack's with these keys
added. They belong to a game pack; a mod that sets one is refused.

```toml
seed = "42"
generator_version = "5"
plan_version = "3"
character = "1"
biorand_flags = ["DOOR_RANDOMIZER", "ITEM_RANDOMIZER"]
scenario_flags = ["122=1", "123=0"]
start_items = ["2:15", "65:1"]
chris_slots = 8
jill_slots = 8
```

| Key | Meaning |
| --- | --- |
| `seed` | The seed that generated the pack, as a string. It marks the pack as generated and raises the `RANDOMIZER` system flag. |
| `generator_version`, `plan_version` | The revisions of the generator and of its plan, as strings. The engine does not read them; the generator compares them to tell a stale pack. |
| `base` | The id of the pack the game was generated from, so that game's mods layer over it too. Left out, a seeded pack counts as `"re1"`'s, as BioRand generates only RE 1 games. On a pack without a `seed`, `base` changes nothing. |
| `character` | `"0"` (Chris) or `"1"` (Jill). Choosing a new game starts as that character, without the character select screen or the prologue film. RE 1's select screen chooses nothing else, so the new game's choice (difficulty, censorship) stays at its default. |
| `biorand_flags` | The system flags the pack raises, by name (below). A name that is not a system flag, or one listed twice, is refused. Left out, a seeded pack raises `DOOR_RANDOMIZER`, `ENEMY_RANDOMIZER` and the proof of concept's bits `0xF0` to `0xF2`. |
| `scenario_flags` | Scenario flags (bank 0) a new game sets (`=1`) or clears (`=0`), after the game's own start. A save keeps them. |
| `start_items` | The player's inventory on a new game, as `"<item>:<amount>"`, in place of what `game/new_game.lua` gives. At most 8; items past the character's slots are dropped. Rebecca's slots are left as the script sets them. |
| `chris_slots`, `jill_slots` | Chris's and Jill's inventory slot counts in RE 1 rooms, 1 to 8, in place of six and eight. They apply on a new game and on a continue. A save holds eight slots for the player and shares the last two with Rebecca's first two, as Jill's do, so Chris with seven or eight slots and Rebecca's items overlap there. |

### System flags

The system flags are bits `0xC0` and up of flag bank 5 (`FG_STATUS`), the
range BioRand reserves; the original scripts use only the bits below it.
Every new game and every continue raises the pack's flags and clears the
other system flags, since a save keeps none of bank 5. A pack that raises
none, which is every ordinary pack, leaves bank 5 alone.

| Name | Bit | Meaning |
| --- | --- | --- |
| `RANDOMIZER` | `0xC0` | The pack is generated (any `seed`). |
| `DOOR_RANDOMIZER` | `0xC1` | Doors are randomized. |
| `ENEMY_RANDOMIZER` | `0xC2` | Enemies are randomized. |
| `ITEM_RANDOMIZER` | `0xC3` | Items are randomized. |
| `LOCK_RANDOMIZER` | `0xC4` | Locks are randomized. |
| `CUTSCENES_DISABLED` | `0xC5` | Cutscenes are disabled. |
| `FORCE_TYRANT2` | `0xC6` | The rooftop Tyrant always appears. |
| `LAB` | `0xC7` | The game reaches the laboratory. |
| `BR_LOCK_FULL_BYTE` | `0xC8` | BioRand's 256 lock flags. |
| `BR_NEPTUNE_KILLABLE` | `0xC9` | Neptune can die and takes weapon damage. |
| `BR_YAWN_POISON` | `0xCA` | Yawn's bite gives ordinary poison. |
| `BR_HERB_MIXES` | `0xCB` | BioRand's extra blue herb mixes. |
| `BR_INK_FLAG` | `0xCC` | Ink reads scenario flag 122 for both characters. |
| `BR_NO_TITLE_DEMO` | `0xCD` | No attract demo from the title screen. |

Only the bits are supported so far. The engine sets them but reads none of
them: the behaviour each name describes (randomized doors, locks and
enemies, the BioRand patches) is not part of the engine. The script
compiler does not know the names either, so a room script tests one by its
bit: `ck(FG_STATUS, 0xC3, TRUE)`.

A mod's manifest has keys of its own: see [Mods](../mods.md#the-manifest).

`arklay pack info` prints a pack's manifest, the mods applied over it, and
the merged list of entries:

```sh
arklay pack info re1.akpak --mod mods/demo
```
