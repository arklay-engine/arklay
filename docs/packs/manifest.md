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

A mod's manifest has keys of its own: see [Mods](../mods.md#the-manifest).

`arklay pack info` prints a pack's manifest, the mods applied over it, and
the merged list of entries:

```sh
arklay pack info re1.akpak --mod mods/demo
```
