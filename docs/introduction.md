# Introduction

Arklay is an engine for classic fixed-camera survival horror games. It runs
RE 1 and RE 2 from game packs converted from the original data, and it loads
mods on top of them.

The engine itself holds no game. Everything a game needs (its rooms, models,
music, text and scripts) is in a **game pack**: a single `.akpak` file, or a
directory with the same files in it. `arklay convert-game` builds a pack from
your own installation of the original game, and the engine runs that pack.
A **mod** is a smaller pack that sits on top of a game pack: any file it
carries replaces the file at the same path in the pack below.

This book is for people who build packs and mods:

- **[Game packs](packs/index.md)**: the folders of a pack, one page per
  folder, with the file formats and the names that go in each.
- **[Mods](mods.md)**: how mods layer over a pack, how replacement art and
  audio is picked, HD texture packs, and building and converting mods.
- **[Scripting reference](scripting/index.md)**: the Lua API the engine
  gives scripts, and the `.akscr` room script language, with a page per
  instruction for RE 1 and RE 2.
- **[Internals](internals/index.md)**: the engine's architecture,
  performance budgets, status, and the recorded deviations from the
  original games.

## A first look at a pack

`arklay extract` unpacks a pack into a directory, and the engine runs that
directory as it runs the `.akpak`:

```sh
arklay convert-game "C:\GOG Games\Resident Evil" --out re1.akpak
arklay extract re1.akpak --out re1
arklay re1 --room 100
```

Edit any file in `re1/` and run it again to see the change. To keep your
changes apart from the game, put them in a mod instead (see [Mods](mods.md)).

The source is at
[github.com/arklay-engine/arklay](https://github.com/arklay-engine/arklay).
These pages are built from the `docs/` directory of the `main` branch, so
every commit to `main` updates them.
