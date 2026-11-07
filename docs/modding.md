# Modding and authoring

An `.akpak` pack is a flat store of named entries; a mod is a second pack whose
entries layer over a base pack. Nothing about the engine is special-cased for a
particular mod: every loader reads through the layered `Pack`, so any entry
(room definition, background, `room/{id}/room.scd` scripts, BGM, UI art, Lua source,
...) can be shadowed by a later layer.

## The pack manifest

Every pack may carry a `manifest.toml` entry. `arklay convert-game` writes one
for the base pack; a mod source must have one. The grammar is a small TOML
subset: comments, `key = value` with strings, integers, booleans and string
arrays, and a single `[pack]` section.

```toml
[pack]
format = 1
id = "demo"
name = "Demo mod"
version = "0.1.0"
description = "One line or several.\nShown by `arklay pack info`."
kind = "mod"
base = "re1"
load_order = 10
language = "ja-JP"
lua = ["lua/demo.lua"]
```

| Key | Meaning |
| --- | --- |
| `format` | Manifest format; `1` today. Required. |
| `id` | Stable pack id. Required. |
| `name`, `version` | Human-facing labels. Optional. |
| `description` | Longer human-facing text (credits, a seed); `\n` breaks lines. Optional. |
| `kind` | `"base"` or `"mod"`. Required. |
| `base` | A mod's base pack id. Required for mods. |
| `load_order` | Layer order; lower applies first. Default 0. |
| `language` | The language of the pack's own room text (`ja-JP` for the Japanese install). Optional. |
| `engine` | Optional note about the target engine build. |
| `lua` | Ordered hook paths; empty means discover `lua/**.lua`. |
| `start_room` | A base pack's new-game room as a three-character room id (`"100"`, or a letter stage such as `"G01"`), in the selected character's player variant; the player enters like a `--room` boot. Without it a new game starts in RE1's main hall. Optional; a mod manifest may not set it. |
| `region` | A base pack's script region, `"USA"`, `"JPN"` or `"EUR"`: `convert-game` records the region whose room scripts the install ships, and the room scripts' `if (USA)`-style tests read it. Without it the scripts run as USA's. Optional; a mod manifest may not set it. |

A pack carries no game-format tag: each RDT, EMD and WAV entry is identified
by its own content (`src/format.rs`), so one pack may mix RE1 and RE2
files. Older manifests' `rdt` and `scd` keys are accepted and ignored.

A base pack without a manifest is tolerated as `id = <file stem>`,
`kind = "base"` with a warning; a mod without one is rejected by name.

## Layers at runtime

A game pack — the positional argument, `--mod`, and every pack command — may
be either an `.akpak` image or a directory of unpacked entries. A directory
pack maps every regular file below it to an entry named by its
directory-relative path (the layout `arklay extract` writes) and reads entries
on demand, so it does not load the whole game into memory. A directory mod must
hold `manifest.toml`; `arklay mod build` is optional when running directly.

```sh
# Explicit layers, repeatable; later load_order wins a shared entry. A mod may
# be an .akpak or a source directory.
arklay re1.akpak --mod mods/balance.akpak --mod mods/demo --room 100

# Discovery: every top-level *.akpak or mod directory in the pack's sibling
# mods/ and in the per-user <config dir>/mods directory is applied. On Windows
# that is %APPDATA%\arklay\mods; ARKLAY_CONFIG_DIR relocates both.
arklay re1.akpak --room 100

# A clean vanilla run: no discovery, no --mod
arklay re1.akpak --no-mods --room 100
```

`<config dir>/mods` holds per-user mods next to the config file. Only its
top-level entries are candidates: an `.akpak` file or a directory containing
`manifest.toml`. A nested directory is not scanned, so a mod's own asset tree
is never mistaken for another mod.

### Configuring discovered mods

`arklay.toml`'s `[mods]` table selects and orders discovered mods by their
manifest `id`:

```toml
[mods]
# When present, only these discovered ids apply (an empty list applies none).
enabled = ["hd", "demo"]
# Always excluded from discovery.
disabled = ["balance"]
# Application order: these ids apply first in this order, then the rest by
# (manifest load_order, id).
order = ["demo", "hd"]
```

`disabled` wins over `enabled`. An explicit `--mod` layer always applies, even
when the config excludes its id, and prints a warning; `--no-mods` ignores the
explicit layers and both discovery directories. A missing `[mods]` table keeps
the default: every discovered mod applies in `(load_order, id)` order. An
`enabled` id that was not discovered is reported as a warning.

Mods sort by the configured `order`, then by `(load_order, id)`; within one
layer the existing ASCII case-insensitive lookup wins. Duplicate mod ids and
duplicate layer paths are rejected, and a mod may only name a `base` matching
the base pack's id. The exception is a mod holding nothing but `htexture/`
entries: textures replace by pixel hash, so once enabled it applies to every game
pack, whatever its `base` says. With no layers at all the engine takes exactly the
single-pack path, so a run without `--mod` (and with no discovered mods) is
byte-identical to earlier releases.

`arklay list`, `arklay extract` and `arklay pack info` accept `--mod` too;
`pack info` prints the manifest, the applied layers and the merged entry table.
Their `--mod` paths accept directories, but they do not auto-discover the
sibling or per-user directories.

## Replacement formats

Mods are authored in WebP/PNG and OGG Vorbis while the pack stores the art and
audio the original shipped. Every loader therefore looks an asset up by *stem*
— the entry path without its extension — and the pack resolves the formats:

| class | priority, highest first |
| --- | --- |
| image | `webp` > `png` > `bmp` > `tim` > `pix` |
| audio | `ogg` > `wav` |
| movie | `mp4` > `avi` > `mpg` |

**Any entry in a mod whose stem matches a known asset is an override.** The
winning layer is still decided by `(load_order, id)` exactly as above, and only
then does the extension priority pick within that layer: `room/100/cut/00.png`
replaces the background `room/100/cut/00.bmp`, `ui/itemboxn.webp` replaces the
page `ui/itemboxn.tim`, `bgm/013.ogg` replaces `bgm/013.wav`, and a layer that
ships both `x.png` and `x.webp` gets the WebP. A higher layer's PNG beats a
lower layer's WebP, because the override is decided per layer first.

PNG, WebP and OGG Vorbis are decoded by the pure-Rust `png`, `image-webp` and
`lewton` crates behind the default-on `hd` feature (the same switch `cargo
build --no-default-features` turns off, keeping that build's dependency list to
`zstd`). A build without `hd` still resolves replacements by stem but refuses
to decode those formats, naming the feature. BMP, TIM and WAV stay the port's
own decoders with the same size budgets as the shipped formats. `mod build`
packs replacement sources verbatim and stores them raw, and `convert-game`
needs no change: it reads the install's own BMP/TIM/WAV/PIX sources, which
never carry a replacement format.

A film replacement is either an MP4 or an AVI holding H.264 video (any
profile OpenH264 decodes, e.g. High) with optional AAC-LC audio, or a Cinepak
AVI like the shipped RE1 films. RE2's shipped films are MPEG-1 program streams
(`movie/<name>.mpg`), so an MP4 or AVI at the same stem replaces one. The decoder is picked from the container and the
codec, so an H.264 AVI can sit at the shipped `movie/<name>.avi` path or an MP4
next to it at `movie/<name>.mp4`; there is no hash lookup. The film plays at
its own frame rate on the 30 Hz clock and its audio is resampled to the
mixer's 22050 Hz. The GPU renderer scales the film's own frames to the window
(letterboxed to their aspect); the software renderer box-filters them onto the
320x240 frame. Colour follows the stream's signalled matrix (BT.709 or
BT.601, limited or full range) and assumes BT.601 limited range when there is
none. The audio track is optional; an MP4 edit list's encoder priming is
skipped. H.264 is decoded by Cisco's OpenH264 (built from source by the
`openh264` crate) and AAC by `symphonia`, both behind the `hd` feature. A film
that does not open logs a warning and the shipped film it shadows plays
instead; a damaged frame later on holds the previous picture.

Jill's prologue (`movie/01`) removes 177.8 s to 188.5 s of the film, so
playback jumps to 188.5 s. Decoding resumes from the keyframe at or before that
point and catches up from there, so a keyframe exactly at 188.5 s avoids a
brief freeze at the jump (for example ffmpeg's `-force_key_frames 188.5`, or
the closest frame boundary). Keep GOPs closed (x264's default) so the restart
is exact.

### What each class accepts

| class | entry stems (one example) | formats | size |
| --- | --- | --- | --- |
| room background | `room/100/cut/00` | webp, png, bmp, tim, pix | any size: stretched over the 320x240 frame |
| camera mask | `room/100/mask/00` | webp, png, bmp, tim, pix | the shipped page's; black is a hole |
| full-frame UI backdrop | `ui/title`, `ui/sel_back`, `ui/type00`, `ui/chris02`, `ui/jill02`, `ui/clis01`, `ui/jill01`, `ui/rc1121` | webp, png, bmp, tim | any size: stretched |
| UI sheets | `ui/status`, `ui/blue`, `ui/statface`, `ui/staitem`, `ui/itemboxn`, `ui/select_b`, `ui/select_k`, `ui/t_press`, `ui/t_start`, `ui/died`, `ui/en05`, `ui/en07` | webp, png, bmp, tim | the shipped page's; an RGBA replacement uses its own alpha |
| item art | `item/item_mix`, `item/medal` | webp, png, bmp, tim | the shipped sheet's; black is a hole |
| item definition | `item/0b/item.xml` | xml (see [Item definitions](#item-definitions)) | — |
| item icon | `item/0b/item`, `item/44/item`, `item/6f/item` | webp, png, bmp, tim | the shipped 40x30 icon's; black is a hole |
| item viewer model | `item/0b/model.tmd` | tmd | the shipped mesh |
| item viewer texture | `item/0b/model.bmp` | bmp (8-bit indexed, full 256-entry palette) | the shipped 256x256 page; HD replacements use `htexture/item/0b/model` |
| enemy / player art | (inside `enemy/<id>/enemy*.emd`, `player/<id>/skin/<skin>.emd`) | — | override the whole model entry |
| player character | `player/<id>/skin/<skin>.emd` (`00` own model, `01`/`02` alternate outfits), `player/<id>/weapon/<item>.emw` (weapon id `00` no weapon … `0a`, `6f` Ingram, `70` Minimi), `player/<id>/sound/<sound>.wav` (`00`-`03` hurt sounds, `04`-`07` their echoing copies); `<id>` is `00` Chris, `01` Jill, `02` Barry, `03` Rebecca | emd, emw; sounds as for `se/` | — |
| scripted-character held weapon | `enemy/<id>/weapon/<behavior>.tmd` (entity id `20`-`2e`, the behavior byte `01`-`06`); a missing entry holds nothing | tmd | textured from the character's own page |
| doors | (inside `door/<type>.dor`, the RDT door-type byte `00`-`21`) | — | override the whole entry |
| font sheets | `font/00` (Western), `font/01` (Japanese), or a new two-digit hex id | webp, png, bmp, tim | the shipped sheet's; at most 255 opaque colours |
| font metrics | `font/00.xml`, `font/01.xml`, or a new font's `font/<id>.xml` | xml: `<font format="1" script advance cellWidth margin textY menuY>` with a `<glyph cell advance indent>` per non-default cell (see `games/re1/font/`); the optional `yesX noX cursorX` (all three) draw a question's yes/no on its last line, as the RE2 pack's font does | — |
| map pages | `map/00` … `map/0e`, `map/bg` (the full floor-plan view's backdrop) | webp, png, bmp, tim | the shipped page's; at most 255 opaque colours |
| FILE pages | `item/<id>/file_bg` (the book backdrop), `item/<id>/file_fg_<page>_<language>` (`en-US`, `de-DE`, `fr-FR`, `ja-JP`), e.g. `item/6e/file_fg_03_en-US`, where `<id>` is the document's item (`5f` + the document number); `ui/file_book_0`, `ui/file_book_1`, `ui/file_arrows` | webp, png, bmp, tim | the shipped page's |
| room effect sheets | `effect/00` … `effect/20` | webp, png, bmp, tim | the shipped page's (the room's own sprites composite onto it) |
| room definition | `room/100/room.xml` | xml (see [Room definitions](#room-definitions)); a raw RDT named for its variant, `room/100/jill-mansion1.rdt`, still loads | — |
| room scripts | `room/100/room.scd` | scd | — |
| room object / item model | `room/100/object/00.tmd`, `room/100/item/00.tmd` | tmd | the shipped mesh |
| room model texture | `room/100/object/00.bmp`; HD replacements use `htexture/room/100/object/00` | bmp (8-bit indexed, full 256-entry palette), tim | the shipped page's |
| room effect sprite | `room/100/effect/00.xml` + `.bmp` | xml; bmp (16-wide CLUT sheet), tim | the shipped sprite's |
| room player animation | `room/100/anim/00.emr` + `.edd` | emr, edd | — |
| player shadow | `shadow/kage` | webp, png, bmp, tim | the shipped page's; an RGBA page's alpha is the coverage |
| BGM | `bgm/013`, `bgm/100` | ogg, wav | any |
| sound effect | `se/a_mcn03`, `se/ft_wda` | ogg, wav | any |
| voice | `voice/01_09` | ogg, wav | any |
| RE2 sound bank slot | `sound/room/10a/01`, `sound/door/36/00`, `sound/step/35/00` | ogg, wav | any |
| RE2 sound table | `sound/enemy/25/bank.edh`, `room/1000/sound.edh` | edh | — |
| film | `movie/00`, `movie/17`, `movie/0f` (the film id); RE2 `movie/opn1stl`, `movie/staff` (the film name) | mp4, avi, mpg | any size up to 4096x4096, any frame rate |

The stems are the shipped entries' names without their extensions: a room's
files sit in its folder `room/<room>` (the three-digit lower-case room id,
see [Room definitions](#room-definitions)), with cameras and model, effect
and animation files numbered by two lower-case hex digits
(`RoomId::cut_stem`), `bgm` uses
the numeric track id `{id:03x}` (`music::bgm_entry`), `voice` uses the 1-based
stage row and the lowercase hex id (`voice::entry_path`), `door`, `movie`,
`effect`, `map`, `file` and `font` use two lower-case hex digits of the door
type, film id, room effect-sheet index, map number, document (item
`0x5F + doc`) and page, and font id, RE2 sound banks use `sound/<kind>/<bank id>/<slot>`
with two lower-case hex digits for the slot (`se2::slot_stem`), and everything else is the literal name `arklay list` prints. Music tracks, including the former
`Se_*` group layers, are all `bgm/<track id>`; the converter owns the only
name-to-id table.

A font is a sheet and a `font/<id>.xml` description sharing an id; a sheet
without a description is not drawn. The description declares the script the
sheet's cells are encoded in, so a mod may add a font under a new id and the
sessions of that script pick the lowest id. The earlier `font/font`,
`font/latin` and `font/jpn` names are no longer read: rename a Western sheet
to `font/00` and a Japanese one to `font/01`.

### BMP transparency

Magenta (`#FF00FF`) is the transparent colour of every BMP. An 8-bit BMP
whose palette entry 0 is magenta is an indexed page: its texel indices and
palette order are kept as they are, index 0 is transparent, and each palette
entry's reserved byte is its PSX semi-transparency (STP) bit. `convert-game`
writes the map, FILE, effect and item model pages this way, so screens that
recolour by palette index and the HD texture hashes keep working. Any other
BMP is read as true colour.

### Room definitions

`convert-game` writes each room as a folder `room/<room>` (`<room>` is the
three-digit lower-case room id, e.g. `room/10b`). One folder holds every
variant of the room: both players and, for the mansion (stages 1 and 2),
both visits. The second visit is the same room played after the guardhouse
sets scenario flag 0 (the original game numbered it as stages 6 and 7 and
duplicated the rooms; the port does not). Every file in it is found by its
name, so nothing in a room refers to a path.

For RE1 the repository owns the room definitions: `room.xml` and the effect
definitions (`effect/<slot>.xml`) are checked in under
`games/re1/room/<room>/`, and `room.scd` is built from the room's script,
`room.akscr` beside them (room 103's is
`games/re1/room/103/room.akscr`). The script itself is not packed. Every
release takes the same definitions. Rooms 110, 119, 200, 20C and 213 to 21C
only exist on the second visit: the original ships empty stubs for their
first visit (the French and German RDTs carry leftover data in them), and
the room's folder is its second visit's room. The French and German extra
rooms 313 and 412, which no door leads to, are not converted. The install's
`ROOM*.RDT` files say which rooms exist, and supply only the game art: the
object and item models, the effect sheets and the player animation. The
camera backgrounds and mask pages come from the install's `RC*.pak` and
`objspr` files, and every variant shares them.

```text
room/100/room.xml        the room: lights, cameras and masks, zones, collision, strings
room/100/room.scd        the scripts the room runs: init, per-frame and events
room/100/cut/00.bmp      camera 0's background
room/100/mask/00.bmp     camera 0's mask page
room/100/object/00.tmd   object model 00, with its texture object/00.bmp
room/100/item/00.tmd     item model 00, with its texture item/00.bmp
room/100/item/03.tmd     the item model Jill's slot 0 reads instead (see <slots>)
room/100/effect/00.xml   effect sprite 00, with its sheet effect/00.bmp
room/100/anim/00.emr     Chris's player animation, with anim/00.edd
room/100/anim/01.emr     Jill's player animation
```

Camera and file numbers are two lower-case hex digits counting from zero
(`cut/0a.bmp` is camera 10). A room is self-contained: copying or renaming its
folder moves the whole room to another id or another pack, and the definition
carries no id of its own. The folder has no folders per player or visit: what
differs between variants is said in `room.xml`.

`room.xml` defines every variant at once. An element only some variants have
carries `when`: space-separated terms that must all hold, each `CHRIS`,
`JILL`, `MANSION1` or `MANSION2`, with `!` before one to negate it. The room
is read for the variant being entered, its visit fixed when it loads, and an
element whose `when` fails is not there:

```xml
<room format="3">
  <ambient when="MANSION1" r="696" g="680" b="504"/>
  <ambient when="MANSION2" r="888" g="888" b="888"/>
  ...
  <walkZones>
    <walk when="MANSION2" x1="7000" z1="11600" x2="11000" z2="15600" field8="0x0041" links="1 2"/>
    ...
  </walkZones>
</room>
```

A list (lights, walk zones, camera switches, footsteps) is the elements that
hold, in order. The `objectSlots` and `itemSlots` attributes of `<room>`
take no `when`: they are the largest slot count any variant declares.

The object and item models, the effect sprites and the player animation are
read by slot: the scripts name an item model by its slot, and the room's
effects are its effect slots. Slot `n` reads file `n` of its kind's folder
(`item/03.tmd` for slot 3), unless `<slots>` names other files for the
variant. Where the variants' models, effects or animation differ, each
differing file has a number of its own, and `<slots>` lists the file each
slot of a variant reads, by slot, with `-` for an empty slot:

```xml
<room format="3">
  ...
  <slots>
    <item when="JILL">03 01 02</item>
    <item>00 01 02</item>
    <effect when="MANSION2"/>
    <anim when="JILL">01</anim>
    <anim>00</anim>
  </slots>
</room>
```

Jill's item slot 0 reads `item/03` where Chris's reads `item/00`, the second
visit has no effects, and Jill's player animation is `anim/01`. The first
`<object>`, `<item>`, `<effect>` or `<anim>` that holds for the variant
gives the whole list of its kind, so one without `when` after the others is
the room's default; a kind with none reads every file of its folder in
order. `convert-game` places each variant's RDT art at the files its slots
name, so variants share a file their slots name alike.

The room's text sits last in `room.xml`, in the same `<strings>` shape as an
item's: a `<language lang="…">` per language holding a `name` (the debug menu
and save list name), a `save_name` (the save screen's shorthand label), and
then the room's messages, each keyed by its decimal message id, in the
language file syntax (`{scroll 0}`, `\n` for a line break):

```xml
<room format="3">
  ...
  <strings>
    <language lang="en-US">
      <string key="name">SAVE ROOM</string>
      <string key="save_name">M. Room 1F</string>
      <string key="0">{scroll 0}\n{scroll 2}{timed 70}</string>
      <string key="1" when="JILL">{scroll 0}\n{scroll 2}{timed 105}</string>
      ...
      <string key="42">\nVitamins and serums.{scroll 2}</string>
    </language>
  </strings>
</room>
```

A message's subtitle speaker goes beside it as `<id>_speaker`, or
`<id>_speaker_<page>` for one page. They become the `ROOM_<room3>_NAME`,
`ROOM_<room3>_NAME_SAVE`, `ROOM_<room3>_TEXT_<nn>` and
`ROOM_<room3>_TEXT_<nn>_SPEAKER[_<page>]` language keys (message `42` is
`TEXT_2A`). A string with a `when` (plain terms only: one player, one
visit or both) is that variant's own text, under the key with the variant
after the room id: `ROOM_<room3>_JILL_TEXT_<nn>`, `ROOM_<room3>_MANSION2_TEXT_<nn>`,
`ROOM_<room3>_JILL_MANSION2_TEXT_<nn>`. A variant reads the most specific
key that exists (player and visit, player, visit, then the room's own). A
language file's own key still wins. English
is a language like any other: a room has no message text outside
`<strings>`, and a message no language has reads as its key. RE1's names and
message texts are in its checked-in `room.xml` files; RE2's rooms have none
yet. Format 2 rooms kept the RDT's encoded messages in a `<messages>` block;
format 3 has none, and a format 2 room still loads with its block ignored, so
its text must be in `<strings>` (or a language file) to show.

An RE2 room's `room.xml` carries `game="re2"`, its cameras' own lighting and
its `<sca>` collision records; the RDT sections the engine does not read yet
(scripts, masks, models) are not converted.

Without `<slots>`, model slots are probed in order from `00`; a model
texture that does not round-trip as an 8-bit BMP is kept as a `.tim`. When
the room declares more object or item slots than it has files (trailing
empty slots), the `<room>` element carries `objectSlots` or `itemSlots`.
Effect slots are read from `00` up to the first missing `.xml`. An effect sheet is an 8-bit BMP with one
16-colour palette row per CLUT row: texel indices `0`-`15`, magenta for
palette entry 0 and each entry's reserved byte its STP bit.

The XML is the format the loader reads, so a hand edit (a moved light, a
retimed message, a reshaped collision box) needs no tool. An unknown element or
attribute is refused rather than ignored. Integers are decimal or `0x` hex.
The camera switch zones are grouped by their source camera, but each carries a
flat `zoneIndex`: the index scripts and the SCD `zone` opcodes use, which must
stay contiguous from 0. Packed flag words are split into named attributes:

- a collision `<box>` has its `shape` (the low byte of the RDT kind), a
  signed `step` height (a multiple of 100), the `cf1`/`cf2`/`cf4`/`cf8`
  collision flags (the `CF_*` bits `inst_cfg` toggles) and any remaining high
  bits as `otherBits`;
- a mask `<sprite>` has its texture `page`, `blend`, `colorMode` and
  `otherBits`, and a square `size` or explicit `width` and `height`;
- a `<walk>` zone lists the zones it connects to as `links="1 7"`.

A room runs `room.scd`, a mod's own when it ships one. A mod that only edits
`room.xml` keeps the base scripts. A mod that ships a raw RDT still loads: an
RDT is one variant's, so it is named for it (`room/<id>/jill-mansion2.rdt`,
`room/<id>/chris.rdt` outside the mansion), and a `room/<id>/room.rdt` serves
every variant without one. When both a `room.xml` and an RDT define a room,
the one from the later layer wins, and the XML wins a tie. An RDT room runs
its embedded scripts unless a `room.scd` comes from the same or a later
layer.

An HD pack can name a model's page `htexture/room/<id>/object/<slot>` (or
`item/<slot>`, covering the whole page) instead of by content hash. A page
with more than one CLUT row is only replaced by its per-row content hashes.
The hash names still work, so an HD pack made for RDT rooms keeps replacing
the same textures.

Effect sprites are composited onto the room's `effect/NN` sheet at their page
row (plus one variant page per extra CLUT row), and the composite is what the
content hash covers. An HD pack can instead name the pieces:
`htexture/room/<id>/effect/<slot>` covers sprite slot `<slot>`'s rows of its
page under CLUT row 0, `htexture/room/<id>/effect/<slot>_<row>` its rows of
the variant page for CLUT row `<row>` (1 to 3), and `htexture/effect/NN`
covers the sheet under the sprites. The GPU renderer then builds an HD page at
the largest scale of the named images: its base is the named sheet (with the
unnamed sprites' original texels over it), else the composite's
content-hash replacement, else the original composite stretched, and each
named sprite image replaces that sprite's whole rectangle, alpha included. A
page with no named image is replaced by its content hash as before. The names
work for rooms loaded from an RDT too.

### Item art

Everything about one item lives in its own folder, `item/<id>/`, where `<id>`
is the lower-case two-digit hex item id (`0b`, `44`, `6f`):

```text
item/0b/item.xml               the item definition (see below)
item/0b/item.bmp               the 40x30 inventory icon
item/0b/model.tmd              the inventory 3D model
item/0b/model.bmp              its 256x256 indexed texture page
item/6e/file_bg.bmp            a document's book backdrop
item/6e/file_fg_03_en-US.bmp   a document's page, per language
```

The original keys an item's art by its *image type*: one `DATA/ITEM_ALL.PIX`
row and one `ITEM_M2/I<imageType>V.IVM` serve every item that shares a class.
`convert-game` splits those per item id and does not pack the shared
`ITEM_ALL` sheet or the `.ivm` files. An item that shares its image type with
another gets its own copy, so editing one item's art never changes its class.
Documents are the items `0x5f` onwards (document `n` is item `0x5f + n`), so
their FILE pages live in those items' folders.

The files are independently optional:

- no `item.bmp`: the item has no inventory icon;
- `model.tmd` without `model.bmp`: the model is untextured;
- no `model.tmd`: the item has no model, as the item viewer and the examine
  screen expect.

The texture page keeps the IVM's exact 256x256 indices, palette and
semi-transparency bits, so the content-hash HD names of an existing pack still
match.

For HD packs the per-item names are under `htexture/`:

```text
htexture/item/0b/item.webp     the inventory icon, covering its 40x30 cell
htexture/item/0b/model.webp    the model's 256x256 page
```

These are preferred over the older names, which keep working unchanged:
`htexture/data/item_all/<row:02>` (`<row>` is `imageType - 1`, the file
Classic REbirth's `hires/Data/ITEM_ALL/<NN>.webp` becomes through
`convert-mod`) and `htexture/data/item_m2/<name>` (the `ITEM_M2` TIM). The
content hash remains the final fallback, so an HD pack that predates both
named routes still replaces the art it did before.

A mod authors an item's base art by shipping the entry itself
(`item/0b/item.bmp`, `item/0b/model.tmd`, `item/0b/model.bmp`) in a mod
source directory. To blank one item's icon, ship a fully transparent 40x30
image.

### Item definitions

`item/<id>/item.xml` describes how the item behaves and what it is called.
The engine embeds a definition for every original item, and a pack's
`item.xml` replaces that item's definition whole (a mod restates every field
it keeps). A definition that does not parse is reported and the item is left
undefined.

```xml
<item kind="heal" stack="1">
    <effects>
        <effect kind="heal" amount="third" />
        <effect kind="cure_poison" />
    </effects>
    <strings>
        <language lang="en-US">
            <string key="name">Mixed Herbs</string>
            <string key="description">It's a mixture of green\nand blue herbs.</string>
        </language>
    </strings>
</item>
```

- `kind`: `weapon`, `ammo`, `bottle`, `chemical`, `special`, `key`, `book`,
  `heal`, `file` or `other` (the default). It picks what *Use* does from the
  inventory and which refusal message an unusable item shows.
- `stack`: the most the item holds, a weapon's loaded rounds or an ammo
  box's count; omitted (0) for an item whose quantity is not shown.
- `merge="true"`: picking the item up adds to an existing stack of the same
  item instead of taking a new slot.
- `<effect kind="heal" amount="third|two_thirds|full" />`: restore that much
  health on use.
- `<effect kind="cure_poison" />` and `<effect kind="cure_serious_poison" />`:
  clear the two poison states on use.
- `<strings>`: per-language `name`, `name_unchecked` (the name shown before
  the item is examined) and `description`, in the language files' escape
  syntax (`\n`, `{color 1}`). Each language feeds the
  `ITEM_<id>_*` keys of the same tag, and a `language/<tag>.txt` key still
  overrides it (see [Language files](#language-files)).

Unknown elements and attributes are errors, so a typo fails `arklay verify`
instead of being ignored silently.

### Mismatch behaviour

A replacement is the winning entry, so it is validated exactly like a shipped
entry, with these extra rules:

- **Format.** A replacement that does not decode fails like a corrupt shipped
  entry: a required asset (a room background, the menu or font art a screen
  opens with) aborts that load with the entry's own path in the message, and an
  optional one logs a warning and leaves its layer empty.
- **Size.** Fixed-geometry art is addressed by UVs or sprite rectangles, so a
  page replacement must keep the dimensions of the entry it replaces; a
  mismatch is refused with a message naming the replacement's size, the shipped
  size and the reason. Full-frame pictures (room backgrounds and the `ui/*`
  backdrops) may be any size: they are stretched over the frame, which is what
  an HD replacement wants. `.pix` is headerless, so it takes the shipped
  entry's dimensions and is refused when it shadows nothing.
- **Colours.** An indexed consumer (the font sheets and the map pages, whose
  layouts the engine addresses by palette index) accepts a true-colour
  replacement with at most 255 distinct opaque colours and re-indexes it colour
  for colour; more colours is an error naming the count. The other sheet
  classes take a true-colour replacement as-is.
- **Layering.** A replacement the engine cannot classify (a fifth format, an
  `.ogg` for an image class) is not a lookup candidate at all, so it stays an
  ordinary new entry and the shipped asset is drawn.

### The checked-in example

`mods/hd/` is the reference replacement mod: a 640x480 PNG `room/100/cut/00`
(a 2x background), a 128x208 `ui/itemboxn` shipped as both PNG and WebP (so the
layer's own priority picks the WebP), and an OGG Vorbis `bgm/013`.

```sh
arklay mod build mods/hd --out hd.akpak
arklay verify hd.akpak                 # the png and ogg rows
arklay re1.akpak --mod hd.akpak --room 100 --ticks 120 --capture hd.bmp
arklay re1.akpak --mod hd.akpak --ui box --capture hd-box.bmp
```

`arklay mod build --base <base.akpak>` counts a format override as an override:
`room/100/cut/00.png` over the base's `.bmp` is not reported as a new file.

### HD texture-pack names

The RE1 HD texture pack (`bio1hd.asi`, run by Classic REbirth) names every
replacement `hires/<class>/<hash>.webp`. `<hash>` is the CRC-32 (IEEE / zlib) of the
texture's texels as the game uploads them, in 32-bit BGRA, written as 8 uppercase hex
digits. `arklay hash FILE` reproduces the conversion and prints the name for every
texture in an original file:

```sh
arklay hash JPN/STAGE1/RC1000.pak        # bgd/DD17F52F.webp
arklay hash JPN/objspr/OSP00001.pak      # mask/C5157461.webp
arklay hash JPN/ENEMY/EM1002.EMD         # skin/474AD7E1.webp, one line per TIM and CLUT row
arklay hash JPN/DATA/SEL_BACK.PIX        # bgd/B87E761B.webp
arklay hash JPN/DATA/BLUE.TIM            # misc/4F8DC05B.webp
```

The game's conversion, which the hash depends on:

- A 15-bit texel widens each channel to 8 bits. Room backgrounds (`RC*.pak`) and
  masks (`OSP*.pak`) bit-replicate (`v << 3 | v >> 2`). Model, menu and `DATA/*.PIX`
  textures shift (`v << 3`).
- Pure black becomes `0x000001` (blue 1) so it stays distinct from transparent `0`. A
  mask's colour 0 is transparent.
- `bgd` hashes the left 256 x 240 texels with alpha forced to `0xFF`. `mask` hashes
  the left 256 x 256 texels, with opaque texels at alpha `0xFF`. `misc`, `skin` and
  `skin2` hash the whole texture, with every texel but `0` and `1` at alpha `0xFF`. A
  window larger than the texture is padded with zero texels.
- A screen-border background (the caves, 316 x 236) is the exception. The game leaves
  the four spare rows of the 256 x 240 window black, so the renderer also tries that
  window with the spare rows hashed as the black texel (`1`), alpha `0xFF`. That
  candidate lets an unmodified pack replace the cave cameras. `arklay hash` hashes a
  316 x 236 `bgd` the same way. A cave room's `.pak` claims 320 x 240, though, and only
  the room's script sets the screen-border flag, so pass `--border` to re-wrap it at
  316 x 236 first, as the engine does: `arklay hash --border STAGE3/RC30B0.pak` prints
  `bgd/11103E64.webp`.

The class comes from how the game uses the texture, so `arklay hash` infers it from
the file: a `.pak` is `bgd` or `mask`, models (`EMD`, `EMW`, `IVM`, ...) are `skin`,
rooms (`RDT`) are `skin2`, and other 320x240 (or 316x236) images are `bgd`; anything
else is `misc`. `misc`, `skin` and `skin2` share one hash rule, so `--class` only
changes the directory for those.

The hash covers the texture the game builds at runtime, so a screen composited from
several sources has no single source file. Classic REbirth's save screen is its own
`type.png` with the menu panel baked in, which hashes to `bgd/D1D144A8.webp`. The raw
`JPN/DATA/TYPE00.TIM` does not. PNG/BMP/WebP input is treated as that kind of
artwork: alpha 0 is transparent, and every other pixel keeps its colour, with black
remapped to `0x000001`.

### HD texture packs

A mod replaces textures by these names under `htexture/`. On first use every image
(`webp`, `png` or `bmp`) anywhere below `htexture/` is indexed by the first 8 hex
digits of its file name, so `htexture/bgd/DD17F52F.webp`,
`htexture/anything/DD17F52F.png` and `htexture/DD17F52F_3B.webp` all replace the
texture hashing to `DD17F52F`; the subdirectories are only for organising files. When
two files share a hash, a bare `<hash>` name wins over a suffixed `<hash>_XX` one.

The lookup is the last step of loading a texture. The usual overrides by path decide
which pixels load, and then the hash of those pixels picks the replacement, so an HD
pack never changes which texture a room shows, and it composes with any mod that
replaces the originals. If the pixels a mod supplies hash to no file, the texture is
drawn as loaded.

Replacements are drawn by the GPU renderer (the default; `--renderer
software` opts out) at their own resolution. The software renderer is 320x240
and keeps the originals.

- A background or mask image is sampled from the replacement in place of the
  original. The replacement covers the texels its hash window covers: the whole
  background, or 256 x 256 mask texels.
- A door transition's page is drawn by the GPU renderer as a scene, so the DOR
  texture takes its page replacement (its single CLUT row) like a model page.
  The frame's fade stays on the overlay.
- A model page keeps its original indices, which decide transparency and the STP
  blend. Each CLUT row's colours come from that row's replacement. A 128-texel-wide
  page is stretched to 256 before hashing, as the game uploads it, so its
  replacement covers 128 x 256 texels.
- The 2D screens (files, item pictures, the status screen and item box, maps,
  `DATA/*.PIX` screens such as the select and epilogue backgrounds) are replaced per
  draw. Each draw is still clipped and positioned as in software, so the HD art covers
  exactly the original's texels, and text or highlights drawn over it stay on top.
  A menu page is hashed unstretched with black kept as `0x000001`, in a 256 x 256
  window, or at its own size when smaller. A `.PIX` screen is hashed like a `bgd`
  but with shift widening.
- A page drawn through a palette lookup (the map's backdrop, browser and floor
  plans, with their visited and current rooms) takes the replacement of its
  unmodified page, CLUT row 0. The GPU keeps the original's indices and recolours
  each HD pixel by the index under it, as the lookup does: an index the lookup
  drops stays clear, one it tints tints the HD pixel, and one it swaps for another
  colour (the red current room) takes that colour at the HD pixel's brightness. The
  PC game instead uploads the map page with its CLUT already edited for the rooms
  visited, so its hashes change with every visit and no pack can name them all.

Some masks cannot be named from their own texels: their hash depends on what the
loader's surface held before the upload. An RE-ENHANCE pack ships the loader's
`cache/ROOM*.dat` lists, little-endian words of each room's hashes in load order,
and each camera's mask sits immediately before its background. `htexture` reads
those lists and pairs a mask with the background hash that follows it. Together
with the 131 that name themselves, this resolves 407 of the 431 masks RE-ENHANCE
v2.0 ships.

The rest belong to rooms or cameras the lists do not cover, so `htexture` pairs
them by shape. Each mask under a `mask/` directory that no camera uses yet is
reduced to one texel per original texel (a texel is drawn when most of the HD
texels over it are at least half opaque). Its coverage is then compared with the
mask page of every camera still missing one, within the original page's extent. A
mask whose intersection over union with one camera is at least 0.9, and at least
0.05 ahead of every other camera, is paired with that camera's background. Cameras
with identical pages share a match, and a camera whose page is identical to one that
already has its mask takes that mask. Cameras whose coverage matches but whose page
art differs still compete, so a mask that fits both is left unpaired. On the 131 masks that name themselves, the rule
picks the right camera for 130 and abstains on the last. It never picks a wrong
one. It pairs all 24 remaining masks, so all 431 resolve from an unmodified
`hires/` tree. Three cameras (`306_002`, `406_005`, `407_001`) have no mask in the
pack. Matching runs once, when the pack's `htexture/` table is first built, and
only for a pack with a `mask/` directory. It is spread over every core and takes
about 0.7 to 0.9 seconds on 8 threads; most of that is hashing each camera's page
and background.

Some 2D art stays software: the fonts, and the raw save screen (`TYPE00.TIM`), whose
hash matches no file in the RE1 HD pack. The title screen finds a match (#132).

A texture with no replacement is reported once on stderr, with every hash that was
tried:

```text
htexture: no replacement for 256x256 model page, CLUT row 1: 3D4A4499 ...
```

A complete pack should report nothing while playing. Missing reports mean either the
pack lacks the texture or its name was hashed from different pixels.

## Language files

A mod may ship `language/<tag>.txt` with any subset of strings. Every layer's
copy of a tag merges per key, later layers winning, so a translation mod can
replace a handful of strings without restating the file; a whole new tag works
too. The runtime picks the file through `--language`, the OS locale and the
`en-US` fallback; the format, the tag vocabulary and the key registry live in
[`language.md`](language.md).

```sh
# A French-language mod overriding one item name and one message
mkdir -p mods/fr/language
cat > mods/fr/manifest.toml <<'EOF'
[pack]
format = 1
id = "fr"
kind = "mod"
base = "re1"
EOF
cat > mods/fr/language/fr-FR.txt <<'EOF'
ITEM_33_NAME = "Clé du manoir"
MESSAGE_1B = "{scroll 2}Ils se sont échappés…{timed 48}"
EOF
arklay mod build mods/fr --out fr.akpak
arklay re1.akpak --mod fr.akpak --language fr-FR --room 100
```

## Mod source directories

`arklay mod build <dir> --out <pack.akpak>` builds a mod from a directory:
`manifest.toml` is required and packed, every other file maps to a pack entry
equal to its directory-relative path, and a `<stem>.s` source in a room
folder (`room/<room>/`) is assembled into
`<stem>.scd` beside it (the `.s` itself is not packed), and a `<stem>.akscr`
script is compiled the same way. A room script laid out like the checked-in
ones (`room/103/room.akscr`) or named by three hex digits (`room/103.akscr`)
builds the room's `room/103/room.scd`: one runtime build for every player,
visit and region, which picks the variant's lines as it runs (see
[RE 1 scripts](akscr/re1.md#mods)).
`--base <base.akpak>`
checks the declared base id and reports override paths the base does not hold
as a warning, never an error; a replacement whose stem matches a base entry of
its own class (the previous section) is an override, not a new file. PNG, WebP
and OGG sources are packed verbatim and stored raw, exactly like the formats
that carry their own compression.

The checked-in `mods/demo/` is the reference mod:

```sh
arklay mod build mods/demo --out demo.akpak
arklay re1.akpak --mod demo.akpak --room 100 --capture demo.bmp
```

It carries a manifest, a whole-room script override for room 100, a Lua hook
and a generated 320x240 background, but no game assets.

The checked-in `mods/hd/` is the replacement-format reference: a 2x PNG room
background, a UI page shipped as both PNG and WebP, and an OGG Vorbis BGM
track, with no game assets either.

## Room music

The room-BGM tables are static game data, loaded once when the session starts
from `game/music.lua` before the first room:

```lua
local api = ...

api:set_music_group(0x0B, {
    { 0x013, true },   -- bgm/013, looping
})

api:set_room_music(1, 0x00, { 0x0B })
api:set_room_music(1, 0x00, { 0x0F, 0x0A }, { mansion2 = true })
```

A mansion room plays its own music on the second visit: `{ mansion2 = true }`
sets the second visit's row instead. A room row holds up to four group ids; the saved state byte's low three bits
select one. Each group has up to three track ids and a loop flag per channel;
all tracks are `bgm/<track id:03x>` in the pack. There are no `Bgm_*`/`Se_*`
names in the runtime or Lua: the converter maps the original `Sound/` files to
numeric ids.

The shipped new-game state lives in `game/initial_state.lua`. It writes the
224 per-room state bytes (`set_room_music_state`, which takes the same
`{ mansion2 = true }`: the second visit keeps its own state bytes, as the
original's saves do), the two starting item-box
stacks, the room-item bank and the remembered pick-up quantities. Save loads
restore their own `room_bgm` bytes and do not run this file.

Both files are whole-file replacements, like `game/new_game.lua`: a mod that
ships one must reproduce every group, room row or state byte it wants to keep.

## Flag names

The debug menu's FLAGS screen names the game banks' bits (scenario, common,
lock, enemy and item) from the pack's `game/flags.lua`, loaded when the menu
opens:

```lua
local api = ...

api:set_flag_name(1, 0x02, "KENNETH_ZOMBIE_SEEN", "The zombie eating Kenneth has been seen.")
api:set_flag_name(1, 0xA0, "J_BARRY_DINING_ENTRY", "Jill: ...")
api:set_flag_name(1, 0xA0, "C_MUSIC_SCORE_GIVEN", "Chris: ...")
```

A bit can take a name per player; the screen shows them as `J_.. / C_..`.
The shipped file is generated when the arklay binary is built, from the
`F_SCENARIO_`, `F_COMMON_`, `F_LOCK_`, `F_ENEMY_` and `F_ITEM_` defines of
`games/re1/inc/flags.akinc` (each define's trailing `//` comment is its
description), so the names are written once, where the room scripts use them.
A bit still named by its number (`F_COMMON_A4`) is left out. A mod can ship
its own `game/flags.lua`; it replaces the whole file. The engine's own banks
are named by the engine.

## The new-game start inventory

The items a new game starts with come from the Lua chunk `game/new_game.lua`,
not from the engine. The base pack carries the shipped one
(`games/re1/game/new_game.lua`); a mod replaces the start items by shipping its
own entry at the same path. The chunk runs once per new game, after
`game/initial_state.lua`, as `local api, character = ...` with `character` `0`
for Chris and `1` for Jill. The player's inventory is already empty when it runs
(for Jill the two partner slots her eight-slot run overlaps are cleared too),
so it only needs `api:set_item_slot` and `api:set_partner_slot`:

```lua
local api, character = ...
if character == 0 then
    api:set_item_slot(0, 0x01, 0)        -- knife
    api:set_item_slot(1, 0x02, 15)       -- beretta
    api:set_partner_slot(0, 0x41, 1)     -- Rebecca's first aid spray
else
    api:set_item_slot(0, 0x01, 0)
end
```

A chunk that fails to load or errors is logged and the shipped items are used
instead, so a broken mod never starts the player empty-handed.

## Converting Classic REbirth mods

`arklay convert-mod <mod>` turns a Classic REbirth (CR) mod, a `.zip` or an
extracted directory, into a mod pack. A CR mod is the game's own files with
overrides, so each file is mapped by the folder it sits in onto the entry
`convert-game` gives the file it replaces:

| CR file | Pack entry |
| --- | --- |
| `hires/**` (HD backgrounds, masks, skins, `cache/ROOM####.dat`) | `htexture/**` |
| `movie/*.mp4`, `movie/*.avi` | `movie/<film id>.<ext>`; an AVI beside an MP4 of the same film is dropped |
| `STAGEn/ROOM####.RDT` | the room's folder, named for its variant: `room/<room>/<variant>.rdt` (`STAGE6/ROOM6031.RDT` is `room/103/jill-mansion2.rdt`, the original's stages 6 and 7 being the second mansion visit); the engine loads a raw RDT as well as the converted XML |
| `STAGEn/RC*.pak`, `objspr/OSP*.pak` | `room/<room>/cut/*.bmp`, `room/<room>/mask/*.bmp`, which every variant of the room reads |
| `ENEMY/*.EMD`, `PLAYERS/*.EMW` | `enemy/<id>/enemy*.emd`, `player/<id>/skin/*`, every `player/<id>/weapon/*` the original's weapon table plays the file for |
| `PLAYERS/WS*.TMD` | every `enemy/<id>/weapon/<behavior>.tmd` holding the mesh |
| `sound/*.WAV`, `voice/*.WAV` | `bgm/*`, `se/*`, every `player/<id>/sound/*`, `enemy/<id>/sound/*` slot and `voice/*` record playing the file |
| `DATA/*` (UI art, item icons, demos, effects) | `ui/*`, `item/*`, `demo/*`, `shadow/*` |
| `ITEM_M1/*.DOR`, `effspr/*.TIM` | `door/<type>.dor`, `effect/<id>.tim` |
| `TEXT/ROOM_###.xml` | `ROOM_###_TEXT_##` keys in `language/en-US.txt` |
| BioRand `init.xml` | `game/new_game.lua` (the start inventory above) |
| `bgm_tbl.xml` (BioRand room music) | `game/music.lua`: the shipped table plus a `set_room_music` per listed room (its rows past the laboratory are the second mansion visit's); an empty `<Room />` keeps the room's music |
| BioRand `type.png` | `ui/type00.png` |

A folder holding a CR `manifest.txt` with a `[MOD]` section is a mod folder:
its paths are relative to the data root, its files win over the game-root
overlay, its `Name` becomes the pack name and its `description.txt` the
manifest `description`. Without one the name comes from the input file name.
The id is the name lowered to `a-z0-9-` and the version is the first dotted
number in the description or the file name; `--id`, `--name` and `--version`
override them.

Left out: the CR runtime and its configuration (`.dll`, `.exe`, `.asi`, `.ini`,
`.conf`, `.cfg`, `.txt`, logs), `savedata/`, the Director's Cut `_bio1dc/`
textures, BioRand's `biorand.dat`, and every converted file identical to the
base pack's entry. BioRand's room sound `tables/` are skipped because each
enemy carries its own sounds, so those tables are no longer needed. Files no
entry reads are listed as warnings.

The base pack (`--base`, else `games/re1.akpak` under the data roots) decides
the room layout (an RE2 base keeps a folder per RDT, so a background is
copied to each player's folder) and the unchanged-file pruning; without one
the room folders are RE1's and the pruning is skipped with a warning. `--root <dir>` converts one folder of an archive that
bundles several games.

Without `--out` the pack is written to the per-user mods directory as
`<id>.akpak`, where launches discover it. An existing pack there is replaced
only when it holds the same mod id (a re-conversion, such as a new BioRand
seed); `--force` replaces any file.

```sh
arklay convert-mod RE-ENHANCE_RE1_v2.0_GOG.zip
arklay convert-mod path/to/biorand/mod_biorand --out biorand.akpak
arklay convert-mod re-hd.zip --root re1hd --base re1.akpak
```

## Room scripts

A room's init, main and event scripts are written as `.akscr` scripts, the
structured source form of the original games' room bytecode. A mod replaces
a room's scripts as a whole. [`.akscr` scripts](akscr/index.md) explains
where scripts live and how they are built, and [RE 1 scripts](akscr/re1.md)
is the full reference for RE 1: the language, the room variants, the names,
every instruction and the `.s` assembler.

## Lua scripts

A pack can carry Lua for four jobs. Each has its own sandboxed API, documented
on its own page, which is generated from the LuaLS definition files in
[`docs/lua/`](https://github.com/arklay-engine/arklay/tree/main/docs/lua) (add
that directory to your editor's `workspace.library` for completion):

- [Lua hooks](lua/hooks.md): `lua/**.lua` chunks whose `on_init`,
  `on_room_load`, `on_tick` and `on_message` functions run with the live game
  state.
- [Screen scripts](lua/screen.md): whole screens in Lua, the engine's own
  `ui/<id>.lua` screens and a save/load screen a pack registers with
  `api:register_save_screen`.
- [GUI DOM](lua/gui.md): the scripting surface of the XML menu markup.
- [Enemy scripts](lua/enemy.md) and [RE 2 enemy scripts](lua/re2-enemy.md):
  the per-tick `update(e)` behaviour of every enemy and character, from the
  `enemy/<id>/` folders.

## Determinism

A capture or headless run is deterministic. Lua hooks run in a fixed order on
fixed ticks, and `api:log` writes to stderr only, so a `--capture` BMP with
hooks still matches between runs; a run with no `--mod` and no discovered mods
matches the pre-M15 captures exactly.
