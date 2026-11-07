# Language

`language/` holds the game's text, one file per language, named by its
language tag:

```text
language/en-US.txt
language/de-DE.txt
language/fr-FR.txt
language/ja-JP.txt
```

Every string the engine shows comes from these files: global and room
messages, item names and descriptions, the save screen, the debug overlay and
the F9 prompt. The text of a room and of an item can
also sit in its own definition (see [Keys](#keys)). A mod may add a
language or replace single strings.

## Selecting a language

The session's starting language is, in order:

1. `--language <tag>`, e.g. `--language en-GB`;
2. the OS locale (`LC_ALL`, `LC_MESSAGES`, `LANGUAGE`, `LANG`; `fr_FR.UTF-8`
   becomes `fr-FR`, a bare `ja` stays `ja`);
3. `en-US`.

Each key then resolves through the fallback chain, independently:

1. the selected tag's file;
2. the same language in any region (`en-GB` finds `en-US`, a `ja` request finds
   `ja-JP`);
3. `en-US`;
4. a global or room message no file carries at all reads as its own registry
   id (`MESSAGE_3F`, `ROOM_101_TEXT_03`), so a missing message is visible
   instead of blank.

Item names and UI labels a chain does not carry read as empty, exactly like a
missing entry: an item without a name shows no name, and an optional save-screen suffix simply disappears.

## File format

One `KEY = "value"` line per string; blank lines and `#` comments (whole-line
or after the value) are ignored. Keys are upper-case ASCII with one of the
`MESSAGE_`/`ROOM_`/`ITEM_`/`UI_` prefixes, and each key appears once.

```text
# language/en-US.txt
ITEM_74_NAME = "Mansion Key"
MESSAGE_1B = "{scroll 2}They have escaped...{timed 48}"
ROOM_100_TEXT_2A = "\nVitamins and serums.{scroll 2}"   # a room message
```

| syntax | bytes | meaning |
|---|---|---|
| `\n` | `0x02` | line break |
| `\"`, `\\` | - | a literal quote or backslash |
| `\xNN` | `NN` | one raw glyph/operand byte |
| `{clear N}` | `0x03 N` | page break with settle N |
| `{scroll N}` | `0x04 N` | skip/tag block N |
| `{color N}` | `0x05 N` | CLUT tint N |
| `{string N}` | `0x06 N` | item-name substitution N |
| `{branch A B C}` | `0x08 A B C` | yes/no post-action |
| `{s}` | `0xFB` | glyph spacing |
| `{timed N}` | action byte | auto-dismiss after N frames (messages only) |

Any other character is encoded through the game font. Literal `0x01`/`0x07`
are rejected as `\xNN`: a value carries its stream's terminator itself.

## Keys

| key | meaning |
|---|---|
| `MESSAGE_<nn>` | global message `nn` (`00`..`3F`), including the boot narration |
| `ROOM_<room3>_TEXT_<nn>` | room message shared by the room's variants |
| `ROOM_<room3>_<VARIANT>_TEXT_<nn>` | a variant's own text: `<VARIANT>` is `CHRIS` or `JILL`, `MANSION1` or `MANSION2` (the mansion visit), or a player and a visit, e.g. `ROOM_100_JILL_MANSION2_TEXT_0A` |
| `ROOM_<room3>_NAME` | room display name (the F1 overlay, the save list) |
| `ROOM_<room3>_NAME_SAVE` | shorthand room name for the save screen, falling back to `NAME` |
| `ITEM_<nn>_NAME` | item name by item id |
| `ITEM_<nn>_NAME_UNCHECKED` | the generic name shown before the item is examined, only where it differs from `NAME` |
| `ITEM_<nn>_DESCRIPTION` | item description by item id |
| `UI_SAVE_*` | save-screen headers, exits, character names, slots, prompts, errors |
| `UI_PROMPT_RETURN_TITLE_*`, `UI_DEBUG_*` | the F9 prompt and the F1 debug tools (tool labels, hints, stage columns, ending names, flag group names) |

The item keys for an item with a definition come from the `<strings>` of its
`item/<nn>/item.xml` (see [Item definitions](items.md#the-definition)), and
the room name and message keys from the `<strings>` of each
`room/<id>/room.xml` (see [Room definitions](rooms.md#text)). A language
file may still set them, and its value wins over the definition's.

## Subtitles

A room script pairs a voice line with a message by playing the line and
then a message that ends `{timed N}`. A message raised while a voice
line plays, that times itself out and asks no yes/no question, is a subtitle.
The interactive window draws its current page with the native-resolution GUI
along the bottom of the screen, styled by the Language › Subtitles options,
instead of the message window. The page's glyphs are decoded
back to text; each authored line stays a line, trimmed and with runs of
spaces collapsed, and a line too wide for the screen wraps. If the GUI's faces
cannot spell a character, the message window draws the line instead.

A message with only timing and no text (`"{scroll 0}\n{scroll 2}{timed 70}"`)
shows no subtitle. To add subtitles, fill in the text and keep the
`{clear N}` and `{timed N}` timings.

A language file may name a message's speaker:

| key | meaning |
|---|---|
| `<message key>_SPEAKER` | the speaker of every page of the message, e.g. `ROOM_100_TEXT_03_SPEAKER = "Rebecca"` |
| `<message key>_SPEAKER_<page>` | the speaker of one page (0 is the first, in decimal), overriding the above, e.g. `ROOM_100_TEXT_04_SPEAKER_1 = "Rebecca"` |

The message key is the one the message resolves through (`MESSAGE_<nn>`, or
for a room message the most specific of `ROOM_<room3>_<PLAYER>_<VISIT>_TEXT_<nn>`,
`ROOM_<room3>_<PLAYER>_TEXT_<nn>`, `ROOM_<room3>_<VISIT>_TEXT_<nn>` and
`ROOM_<room3>_TEXT_<nn>` that exists). The name shows when Speaker Name
Display is on.

## Mods

A mod pack may ship `language/<tag>.txt` with any subset of keys. Every layer's
copy of a tag merges per key, later layers winning, so a mod can replace three
French strings without restating the file. Whole new tags work too: a mod may
add `language/pl-PL.txt`, and `--language pl-PL` (or a Polish OS locale) picks
it up. The font limits which characters it can show.

```text
mods/fr/
├── manifest.toml
└── language/
    └── fr-FR.txt       ITEM_33_NAME = "Clé du manoir"
```

```sh
arklay re1.akpak --mod mods/fr --language fr-FR --room 100
```

## Glyphs

A language can only show the characters its font has. Which font a language
draws, and how to add one, is in [Fonts](fonts.md).
