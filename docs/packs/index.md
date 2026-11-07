# Game packs

A game pack holds everything one game needs. The engine has no game data of
its own: it reads rooms, models, music, text and scripts from the pack, by
path.

A pack is one of two things:

- an **`.akpak` file**: a single compressed archive of named entries;
- a **directory**: the same entries as ordinary files, each named by its path
  below the directory.

The engine runs both the same way, and reads a directory's files only when it
needs them. Every path in a pack is lower case and uses `/`.

```sh
arklay list re1.akpak                        # every entry with its size
arklay extract re1.akpak --out re1           # unpack into a directory
arklay verify re1                            # parse every entry it knows
arklay re1 --room 100                        # run the directory
```

## The folders

```text
<pack>/
├── manifest.toml     the pack's id and settings
├── bgm/              music tracks
├── data/             effect sprites every room shares
├── demo/             attract-mode demo recordings
├── door/             door, stairs and lift transitions
├── effect/           effect sprite sheets
├── enemy/            enemies and other characters: models, scripts, sounds
├── font/             font sheets and their metrics
├── game/             new-game state, music tables, flag names
├── item/             one folder per item
├── language/         the text, one file per language
├── lib/              shared Lua modules
├── lua/              Lua hooks
├── map/              the map screen's pages
├── movie/            films
├── player/           the playable characters
├── room/             one folder per room
├── se/               named sound effects
├── sound/            sound banks
├── shadow/           the player's shadow
├── ui/               menu art and the menu screens
└── voice/            spoken lines
```

| Folder | Page |
|---|---|
| `manifest.toml` | [Manifest](manifest.md) |
| `game/`, `interactive/`, `lib/`, `lua/`, `ui/*.lua` | [Scripting](scripting.md) |
| `bgm/` | [BGM](bgm.md) |
| `demo/` | [Demos](demos.md) |
| `door/` | [Doors](doors.md) |
| `effect/`, `data/` | [Effects](effects.md) |
| `enemy/` | [Enemies](enemies.md) |
| `font/` | [Fonts](fonts.md) |
| `item/` | [Items](items.md) |
| `language/` | [Language](language.md) |
| `movie/` | [Movies](movies.md) |
| `player/` | [Players](players.md) |
| `room/` | [Rooms](rooms.md) |
| `voice/` | [Voice](voice.md) |

The folders without a page of their own are described in
[Other folders](#other-folders) below.

## Names

Numbers in paths are lower-case hexadecimal with a fixed number of digits,
so that the files sort in order:

| What | Digits | Example |
|---|---|---|
| a room | 3 or 4 | `room/10b/`, `room/1000/` |
| a camera, model, effect or animation in a room | 2 | `room/100/cut/0a.bmp` is camera 10 |
| an item, enemy, player, door, film, font, demo | 2 | `item/0b/`, `enemy/20/`, `door/1f.dor` |
| a music track | 3 | `bgm/013.ogg` |

The engine finds files by their **stem**, the path without its extension.
Where a file can be in more than one format, the pack may hold any of them
at that stem, and the highest one in this order wins:

| Kind | Formats, highest first |
|---|---|
| image | `webp`, `png`, `bmp`, `tim`, `pix` |
| audio | `ogg`, `wav` |
| film | `mp4`, `avi`, `mpg` |

So `room/100/cut/00.png` and `room/100/cut/00.bmp` are both camera 0's
background, and the PNG wins if both are there. Each folder's page lists
the formats its files may be in.

## File formats

| Extension | What it is |
|---|---|
| `.toml` | the manifest |
| `.xml` | room, item, effect and font definitions |
| `.lua` | scripts |
| `.akscr`, `.scd` | room scripts, as text and compiled |
| `.txt` | language files |
| `.webp`, `.png`, `.bmp`, `.tim`, `.pix` | images: backgrounds, masks, textures, menu art |
| `.ogg`, `.wav` | audio: music, sound effects, voice |
| `.mp4`, `.avi`, `.mpg` | films |
| `.emd`, `.emw`, `.emr`, `.edd`, `.tmd` | RE 1 format models and animations |
| `.pld`, `.plw` | RE 2 format character models and weapons |
| `.dor`, `.do2` | RE 1 and RE 2 format door transitions |
| `.dat` | demo recordings |
| `.edh`, `.vb`, `.esp`, `.etm`, `.bin` | sound tables and effect sprite data |

### BMP pages

Magenta (`#FF00FF`) is the transparent colour of every BMP. An 8-bit BMP
whose palette entry 0 is magenta is an **indexed page**: the engine keeps its
pixel indices and palette order, index 0 is transparent, and each palette
entry's reserved byte is its semi-transparency bit. Pages that are coloured
by palette index (the map, effect sheets, model textures) are indexed pages.
Any other BMP is read as true colour.

## Other folders

### `se/`: sound effects

Named sound effects, played by name: `se/dlbeep.ogg`, `se/ft_wda.ogg`. The
name is free; the scripts and tables that play a sound name it. Formats:
`ogg`, `wav`.

### `sound/`: sound banks

Sound effects grouped into numbered banks, one file per slot:
`sound/<kind>/<bank>/<slot>`, bank and slot in two hex digits.

| Folder | Bank |
|---|---|
| `sound/door/<bank>/` | a door's sounds |
| `sound/arms/<bank>/` | a weapon's sounds |
| `sound/room/<room>/` | a room's sounds |
| `sound/step/<bank>/` | a footstep set |
| `sound/enemy/<bank>/` | an enemy's sounds |
| `sound/core/<bank>/` | sounds every room shares |

A bank may carry a sound table, `bank.edh`, beside its slots: it gives each
slot's volume. Formats: `ogg`, `wav`.

### `ui/`: menus and screens

Menu art and the screens' scripts (see [Scripting](scripting.md#screens)).
A full-screen backdrop (`ui/title`, `ui/sel_back`, `ui/type00`) may be any
size; it is stretched to the screen. A sheet that is cut into pieces by
position (`ui/status`, `ui/itemboxn`) keeps its size. Formats: `webp`,
`png`, `bmp`, `tim`.

### `map/`: map pages

`map/<nn>` are the floor plans and `map/bg` the backdrop of the full
floor-plan view. The map screen colours rooms by palette index, so a page
may use at most 255 opaque colours. Formats: `webp`, `png`, `bmp`, `tim`.

### `shadow/`

`shadow/kage` is the player's ground shadow. In an image with alpha, the
alpha is the shadow's strength. Formats: `webp`, `png`, `bmp`, `tim`.

### `hires/`

HD textures, found by a hash of the texture they replace. See
[HD texture packs](../mods.md#hd-texture-packs).
