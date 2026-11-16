# Mods

A mod is a pack that sits on top of a game pack. It has the same folders as
a game pack (see [Game packs](packs/index.md)), but carries only the files
it changes: any file in a mod replaces the file at the same path in the
packs below it, and a file at a new path adds to them. Nothing in the
engine is special to one mod. Every loader reads through the stack of
packs, so a mod can replace a room's background, a room's scripts, an
enemy's behaviour, a menu, the music, or the text.

## A first mod

A mod is a directory with a `manifest.toml` (see
[The manifest](#the-manifest)) and the files it replaces:

```text
mods/mansion-hd/
├── manifest.toml
├── room/
│   └── 100/
│       └── cut/
│           └── 00.png        a 640x480 main hall background
├── bgm/
│   └── 013.ogg               new main hall music
└── language/
    └── en-US.txt             ROOM_100_NAME = "GRAND HALL"
```

```toml
[pack]
format = 1
id = "mansion-hd"
name = "Mansion HD"
version = "1.0.0"
kind = "mod"
base = "re1"
```

Run the game with it, then build it into one file to share:

```sh
arklay re1.akpak --mod mods/mansion-hd --room 100
arklay mod build mods/mansion-hd --out mansion-hd.akpak
```

The Arklay repository has three example mods:

- [`mods/demo/`](https://github.com/arklay-engine/arklay/tree/main/mods/demo):
  a manifest, a whole-room script override for room 100, a Lua hook and a
  generated 320x240 background;
- [`mods/hd/`](https://github.com/arklay-engine/arklay/tree/main/mods/hd):
  the replacement formats: a 2x PNG background, a menu page shipped as both
  PNG and WebP, and an OGG Vorbis music track;
- [`mods/crloadsave/`](https://github.com/arklay-engine/arklay/tree/main/mods/crloadsave):
  a Lua save and load screen in the style of Classic REbirth.

## The manifest

A mod's `manifest.toml` has the same form as a game pack's (see
[Manifest](packs/manifest.md)), with `kind = "mod"` and the game it is for:

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
lua = ["lua/demo.lua"]
```

| Key | Meaning |
| --- | --- |
| `format` | `1`. Required. |
| `id` | The mod's id, by which `arklay.toml` enables, disables and orders it. Required. |
| `kind` | `"mod"`. Required. |
| `base` | The id of the game pack the mod is for. Required. |
| `name`, `version` | Labels shown in the launcher's mod list. Optional. |
| `description` | Longer text, such as credits; `\n` breaks a line. Optional. |
| `load_order` | Where the mod goes among the others: lower applies first, so a higher one wins a file both carry. Default `0`. |
| `lua` | The mod's Lua hooks, in order. Left out or empty, every `lua/**.lua` file of the mod runs. |
| `category` | The [mod catalog](#hosted-mods)'s category, e.g. `"High Definition"`, `"Audio"`, `"Cosmetic"`. Optional. |
| `authors` | The people or teams who made the content. Optional. |
| `credits` | Further acknowledgements (sources, contributors, thanks), one string each. Optional. |
| `tags` | Free-form labels, one string each. Optional. |
| `homepage` | The mod's home page URL. Optional. |
| `thumbnail` | The entry holding a 4:3 preview picture (`"thumbnail.webp"`), shown in MODS > AVAILABLE. Optional. |

A string array may run over several lines up to its closing `]`, which keeps
long credit lists readable:

```toml
credits = [
    "bserv: Barry's photo, inventory avatars and menus",
    "TeamX: the PC HD mods and their mask tools",
]
```

A mod may not set `start_room` or `region`. A mod without a manifest is
refused.

## Applying mods

A mod, like a game pack, is either an `.akpak` file or a directory. A
directory mod must hold `manifest.toml`, and it runs as it is: `arklay mod
build` is only needed to share it as one file.

A mod `.akpak` is read on demand: opening it reads only its header, table
of contents and paths, and each entry is read from the file when it is asked
for, so an HD or film pack of several gigabytes costs no memory until its art
is drawn. A game pack is read whole (it serves every room load) and is capped
at 2 GiB; a mod may be up to 64 GiB, and any one entry up to 1 GiB.

```sh
# Explicit layers, repeatable; later load_order wins a shared entry. A mod may
# be an .akpak or a source directory.
arklay re1.akpak --mod mods/balance.akpak --mod mods/demo --room 100

# Discovery: every mod found in the mods/ directories below is applied
arklay re1.akpak --room 100

# A clean vanilla run: no discovery, no --mod
arklay re1.akpak --no-mods --room 100
```

Without `--no-mods`, the engine and the launcher look for mods in these
directories, in order:

1. the `mods/` directory beside the game pack;
2. the `mods/` directory of each data root: on Windows
   `%LOCALAPPDATA%\arklay\mods`, `%APPDATA%\arklay\mods` and the one beside
   `arklay.exe`; on Linux `$XDG_DATA_HOME/arklay/mods`,
   `$XDG_CONFIG_HOME/arklay/mods` and the one beside the executable
   (`ARKLAY_DATA` replaces the data roots);
3. `<config dir>/mods`, if it is not already one of them.

Only a directory's top-level entries are candidates: an `.akpak` file or a
directory containing `manifest.toml`. A nested directory is not scanned, so
a mod's own folders are never mistaken for another mod.

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
the base pack's id. The exception is a mod holding nothing but `hires/`
entries: textures replace by pixel hash, so once enabled it applies to every game
pack, whatever its `base` says. (The folder was called `htexture/` in
0.1.0; a pack that still uses that name reads as `hires/`.) With no layers at all the engine takes exactly the
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

The film formats and codecs are listed in [Movies](packs/movies.md).

### What each class accepts

| class | entry stems (one example) | formats | size |
| --- | --- | --- | --- |
| room background | `room/100/cut/00` | webp, png, bmp, tim, pix | any size: stretched over the 320x240 frame |
| camera mask | `room/100/mask/00` | webp, png, bmp, tim, pix | the shipped page's; black is a hole |
| full-frame UI backdrop | `ui/title`, `ui/sel_back`, `ui/type00`, `ui/chris02`, `ui/jill02`, `ui/clis01`, `ui/jill01`, `ui/rc1121` | webp, png, bmp, tim | any size: stretched |
| UI sheets | `ui/status`, `ui/blue`, `ui/statface`, `ui/staitem`, `ui/itemboxn`, `ui/select_b`, `ui/select_k`, `ui/t_press`, `ui/t_start`, `ui/died`, `ui/en05`, `ui/en07` | webp, png, bmp, tim | the shipped page's; an RGBA replacement uses its own alpha |
| item art | `item/item_mix`, `item/medal` | webp, png, bmp, tim | the shipped sheet's; black is a hole |
| item definition | `item/0b/item.xml` | xml (see [Items](packs/items.md#the-definition)) | — |
| item icon | `item/0b/item`, `item/44/item`, `item/6f/item` | webp, png, bmp, tim | the shipped 40x30 icon's; black is a hole |
| item viewer model | `item/0b/model.tmd` | tmd | the shipped mesh |
| item viewer texture | `item/0b/model.bmp` | bmp (8-bit indexed, full 256-entry palette) | the shipped 256x256 page; HD replacements use `hires/item/0b/model` |
| enemy / player art | (inside `enemy/<id>/enemy*.emd`, `player/<id>/skin/<skin>.emd`) | — | override the whole model entry |
| player character | `player/<id>/skin/<skin>.emd` (`00` own model, `01`/`02` alternate outfits), `player/<id>/weapon/<item>.emw` (weapon id `00` no weapon … `0a`, `6f` Ingram, `70` Minimi), `player/<id>/sound/<sound>.wav` (`00`-`03` hurt sounds, `04`-`07` their echoing copies); `<id>` is `00` Chris, `01` Jill, `02` Barry, `03` Rebecca | emd, emw; sounds as for `se/` | — |
| scripted-character held weapon | `enemy/<id>/weapon/<behavior>.tmd` (entity id `20`-`2e`, the behavior byte `01`-`06`); a missing entry holds nothing | tmd | textured from the character's own page |
| doors | (inside `door/<type>.dor`, the RDT door-type byte `00`-`21`) | — | override the whole entry |
| font sheets | `font/00` (Western), `font/01` (Japanese), or a new two-digit hex id | webp, png, bmp, tim | the shipped sheet's; at most 255 opaque colours |
| font metrics | `font/00.xml`, `font/01.xml`, or a new font's `font/<id>.xml` | xml: `<font format="1" script advance cellWidth margin textY menuY>` with a `<glyph cell advance indent>` per non-default cell (see `games/re1/font/`); the optional `yesX noX cursorX` (all three) draw a question's yes/no on its last line, as the RE2 pack's font does | — |
| map pages | `map/00` … `map/0e`, `map/bg` (the full floor-plan view's backdrop) | webp, png, bmp, tim | the shipped page's; at most 255 opaque colours |
| FILE pages | `item/<id>/file_bg` (the book backdrop), `item/<id>/file_fg_<page>_<language>` (`en-US`, `de-DE`, `fr-FR`, `ja-JP`), e.g. `item/6e/file_fg_03_en-US`, where `<id>` is the document's item (`5f` + the document number); `ui/file_book_0`, `ui/file_book_1`, `ui/file_arrows` | webp, png, bmp, tim | the shipped page's |
| room effect sheets | `effect/00` … `effect/20` | webp, png, bmp, tim | the shipped page's (the room's own sprites composite onto it) |
| room definition | `room/100/room.xml` | xml (see [Rooms](packs/rooms.md#the-room-definition)); a raw RDT named for its variant, `room/100/jill-mansion1.rdt`, still loads | — |
| room scripts | `room/100/room.scd` | scd | — |
| room object / item model | `room/100/object/00.tmd`, `room/100/item/00.tmd` | tmd | the shipped mesh |
| room model texture | `room/100/object/00.bmp`; HD replacements use `hires/room/100/object/00` | bmp (8-bit indexed, full 256-entry palette), tim | the shipped page's |
| room effect sprite | `room/100/effect/00.xml` + `.bmp` | xml; bmp (16-wide CLUT sheet), tim | the shipped sprite's |
| room player animation | `room/100/anim/00.emr` + `.edd` | emr, edd | — |
| player shadow | `shadow/kage` | webp, png, bmp, tim | the shipped page's; an RGBA page's alpha is the coverage |
| BGM | `bgm/013`, `bgm/100` | ogg, wav | any |
| sound effect | `se/a_mcn03`, `se/ft_wda` | ogg, wav | any |
| voice | `voice/01_09` | ogg, wav | any |
| RE2 sound bank slot | `sound/room/10a/01`, `sound/door/36/00`, `sound/step/35/00` | ogg, wav | any |
| RE2 sound table | `sound/enemy/25/bank.edh`, `room/1000/sound.edh` | edh | — |
| film | `movie/00`, `movie/17`, `movie/0f` (the film id); RE2 `movie/opn1stl`, `movie/staff` (the film name) | mp4, avi, mpg | any size up to 4096x4096, any frame rate |

The stems are the shipped entries' paths without their extensions, as
`arklay list` prints them. Each folder's page in [Game packs](packs/index.md)
gives its naming, and [Names](packs/index.md#names) the numbering they share.

### When a replacement does not fit

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

`arklay mod build --base <base.akpak>` counts a format override as an override:
`room/100/cut/00.png` over the base's `.bmp` is not reported as a new file.

## Building a mod

`arklay mod build <dir> --out <pack.akpak>` builds a mod from a directory:
`manifest.toml` is required and packed, every other file maps to a pack entry
equal to its directory-relative path, and a `<stem>.s` source in a room
folder (`room/<room>/`) is assembled into
`<stem>.scd` beside it (the `.s` itself is not packed), and a `<stem>.akscr`
script is compiled the same way. A room script laid out like the checked-in
ones (`room/103/room.akscr`) or named by three hex digits (`room/103.akscr`)
builds the room's `room/103/room.scd`: one runtime build for every player,
visit and region, which picks the variant's lines as it runs (see
[RE 1](scripting/room/re1/index.md#mods)).
`--base <base.akpak>`
checks the declared base id and reports override paths the base does not hold
as a warning, never an error; a replacement whose stem matches a base entry of
its own class (see [Replacement formats](#replacement-formats)) is an override, not a new file. PNG, WebP
and OGG sources are packed verbatim and stored raw, exactly like the formats
that carry their own compression.

### Watching a mod while it runs

`--watch` keeps a running interactive game in sync with the applied source
directories. Start it on an interactive boot (`--ui game`, `--ui title`,
`--room` or a continue); it is rejected with `--capture`, `--trace`, `--input`,
`--save-out` and `--bot-headless`, which have no interactive loop to reload
into.

```sh
# Edit mods/bot/bot/init.lua (or a lua/**.lua hook) while the game runs.
arklay re1.akpak --mod mods/bot --bot --watch
```

The watcher polls the base pack and every layered source directory. It scans
every `.lua` file and every `bot.xml` file below them, but only these classes
are reloaded:

- **`lua/**.lua` hooks** load into a fresh VM. Hook globals reset, exactly as
  when the session opened; `on_init` then `on_room_load` run over the live game
  state and current room. The room's init script is not re-run, so the room,
  session and inventory stay where they are. The app's cached save/load screen
  registration is refreshed from the reloaded `on_init` too.
- **`bot/**.lua` route** re-executes its chunks in the same Lua state. Globals
  survive, and the runtime keeps its tick counter and remembered
  memory/navigation. A route should keep its progress in a global, as the
  checked-in `mods/bot` route does with `BOT_STATE = BOT_STATE or {...}`. A
  chunk that fails to run leaves the previous `on_bot_tick` callable.
- **`room/<id>/bot.xml` metadata** invalidates the bot runtime's cached room
  metadata. The next tick reparses the file and `bot:room_meta()` returns the
  new puzzles. The room, its script and its assets are not reloaded. An invalid
  edit is reported where the bot tick is, and the previous parse is not kept.

A room's metadata lives next to its other files and is optional; the merged
pack resolves it like any other entry. The first format is:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<bot format="1">
  <puzzle id="paintings" title="117 large gallery painting order">
    <step slot="3" bank="4" bit="0x1A" label="117 painting 1/7 newborn">
      <walk x="19500" z="8100"/>
      <face x="19500" z="7200"/>
    </step>
  </puzzle>
</bot>
```

`puzzle` takes `id` and optional `title`; `step` takes `slot`, `bank`, `bit`
and `label`, any number of `<walk x="..." z="..."/>` points, and at most one
`<face .../>`. `bit` is the flag selector passed to `bot:flag` (for example
`0x1A`, not bit number 26). `slot` and `bit` are bytes, `bank` must be below
10, and coordinates are signed 32-bit integers. Unknown elements and attributes
are errors naming the source line, so a typo in a hand-edited file fails
loudly. `bot:room_meta()` exposes the parsed data as a read-only Lua table, or
`nil` when the room has no metadata; it cannot write game state.

Room XML, RDT/room scripts, room assets, screen scripts, enemy scripts and
`.akpak` layers are not reloaded: they need a room reload or a restart. The
pack's entry list is also built when the pack opens, so editing an existing
file reloads, a newly added file is not seen until a restart, and deleting a
`bot/` chunk is a reload error that leaves the previous route running. With
`--watch` off, none of this runs and the game is byte-identical to a run
without the flag.

## HD texture packs

### Names

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

### Using them

A mod replaces textures by these names under `hires/`. On first use every image
(`webp`, `png` or `bmp`) anywhere below `hires/` is indexed by the first 8 hex
digits of its file name, so `hires/bgd/DD17F52F.webp`,
`hires/anything/DD17F52F.png` and `hires/DD17F52F_3B.webp` all replace the
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
and each camera's mask sits immediately before its background. `hires` reads
those lists and pairs a mask with the background hash that follows it. Together
with the 131 that name themselves, this resolves 407 of the 431 masks RE-ENHANCE
v2.0 ships.

The rest belong to rooms or cameras the lists do not cover, so `hires` pairs
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
pack. Matching runs once, when the pack's `hires/` table is first built, and
only for a pack with a `mask/` directory. It is spread over every core and takes
about 0.7 to 0.9 seconds on 8 threads; most of that is hashing each camera's page
and background.

Some 2D art stays software: the fonts, and the raw save screen (`TYPE00.TIM`), whose
hash matches no file in the RE1 HD pack. The title screen finds a match (#132).

A texture with no replacement is reported once on stderr, with every hash that was
tried:

```text
hires: no replacement for 256x256 model page, CLUT row 1: 3D4A4499 ...
```

A complete pack should report nothing while playing. Missing reports mean either the
pack lacks the texture or its name was hashed from different pixels.

## Converting Classic REbirth mods

`arklay convert-mod <mod>` turns a Classic REbirth (CR) mod, a `.zip` or an
extracted directory, into a mod pack. A CR mod is the game's own files with
overrides, so each file is mapped by the folder it sits in onto the entry
`convert-game` gives the file it replaces:

| CR file | Pack entry |
| --- | --- |
| `hires/**` (HD backgrounds, masks, skins, `cache/ROOM####.dat`) | `hires/**` |
| `movie/*.mp4`, `movie/*.avi` | `movie/<film id>.<ext>`; an AVI beside an MP4 of the same film is dropped |
| `STAGEn/ROOM####.RDT` | the room's folder, named for its variant: `room/<room>/<variant>.rdt` (`STAGE6/ROOM6031.RDT` is `room/103/jill-mansion2.rdt`, the original's stages 6 and 7 being the second mansion visit); the engine loads a raw RDT as well as the converted XML |
| `STAGEn/RC*.pak`, `objspr/OSP*.pak` | `room/<room>/cut/*.bmp`, `room/<room>/mask/*.bmp`, which every variant of the room reads |
| `ENEMY/*.EMD`, `PLAYERS/*.EMW` | `enemy/<id>/enemy*.emd`, `player/<id>/skin/*`, every `player/<id>/weapon/*` the original's weapon table plays the file for |
| `PLAYERS/WS*.TMD` | every `enemy/<id>/weapon/<behavior>.tmd` holding the mesh |
| `sound/*.WAV`, `voice/*.WAV` | `bgm/*`, `se/*`, every `player/<id>/sound/*`, `enemy/<id>/sound/*` slot and `voice/*` record playing the file |
| `DATA/*` (UI art, item icons, demos, effects) | `ui/*`, `item/*`, `demo/*`, `shadow/*` |
| `ITEM_M1/*.DOR`, `effspr/*.TIM` | `door/<type>.dor`, `effect/<id>.tim` |
| `TEXT/ROOM_###.xml` | `ROOM_###_TEXT_##` keys in `language/en-US.txt` |
| BioRand `init.xml` | `game/new_game.lua` (see [Scripting](packs/scripting.md#the-new-game-state)) |
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

## Hosted mods

The launcher's MODS screen has two tabs. **Installed** lists the discovered
mods to enable, disable and reorder. **Available** lists a hosted mod catalog,
as a grid of pictures or a list (V), filtered by game and category (F) and
sorted (G); R fetches it again. A mod's details show its picture,
description, size, authors and credits, and install it. An install downloads
the pack to the primary data root's `mods` folder (on Windows
`%LOCALAPPDATA%\arklay\mods`). There it is checked against the catalog's size
and SHA-256 and against its own manifest before it replaces anything. An
installed mod whose catalog version differs shows **Update**, on both tabs.
Remove deletes the pack. A mod installed from a source folder is left alone:
two layers may not share an id, so its details say to remove the folder first.

The catalog is `ARKLAY_MOD_CATALOG`, else `[mods] catalog` in `arklay.toml`
(a URL or a local path to an `index.json`), else Arklay's own hosted catalog,
`https://pub-c49dad06a9fe4f2baacb873a181858b9.r2.dev/index.json`. Opening
MODS fetches it; `catalog = "none"` (or `ARKLAY_MOD_CATALOG=none`) turns the
hosted catalog off. To use another catalog:

```toml
[mods]
catalog = "https://example.org/mods/index.json"
```

The catalog's `index.json` lists each mod's manifest fields plus where the
pack is, how big it is and its SHA-256. Links are relative to the index, so a
hosted tree moves as a whole (see `src/catalog.rs` for the full format):

```json
{
  "format": 1,
  "mods": [{
    "id": "re1-shdp", "name": "Seamless HD Project", "version": "1.1",
    "game": "re1", "kind": "mod", "category": "High Definition",
    "authors": ["MoArtis", "Kayael", "Doze_D", "FrankWesker"],
    "credits": ["..."], "homepage": "https://www.reshdp.com/re1/",
    "url": "re1-shdp/1.1/re1-shdp.akpak", "size": 529808808, "sha256": "...",
    "thumbnail": "re1-shdp/1.1/thumbnail.webp"
  }]
}
```

### Publishing the catalog

Each hosted mod is authored as a mod source directory, `<build>/<id>/`, with
its `manifest.toml` (name, version, category, authors, credits, homepage), a
640x480 `thumbnail.webp` and its entries. Two scripts maintain them:

- `scripts/import-mods.py [id...]` (re)fills a mod directory's content from
  the archives the mod was released as, per the recipes in the script:
  `convert-mod` over the files it selects, or plain copies. It keeps the
  hand-written `manifest.toml` and thumbnail, and writes a minimal manifest
  on a first import.
- `scripts/publish-mods.py [id...]` builds every directory with
  `arklay mod build` into the hosted tree and writes `index.json`:
  `<id>/<version>/<id>.akpak` and its `thumbnail.webp`. A published version is
  never replaced by different bytes, so caches can keep the packs for good.
  To publish changed content, change the manifest's `version`: the upstream
  release's version (`1.1`), with a suffix for a repackage of the same
  release (`1.1-2`). Only `index.json` is rewritten in place, so serve it with
  a short cache lifetime. `--skip` leaves mods out of the index.

The defaults are `G:\arklay\raw` (the original archives),
`G:\arklay\mods-build` (the mod directories) and `G:\arklay\mods` (the hosted
tree, which mirrors the public URL); each script's `--help` lists the
overrides.

## Determinism

A capture or headless run is deterministic. Lua hooks run in a fixed order on
fixed ticks, and `api:log` writes to stderr only, so a `--capture` BMP with
hooks still matches between runs; a run with no `--mod` and no discovered mods
matches the pre-M15 captures exactly.
