# Arklay

Arklay is an engine for classic fixed-camera survival horror games. It runs
RE 1 and RE 2 from game packs converted from the original data, and it loads
mods on top of them.

This book documents the engine for people who build packs and mods:

- **Game packs and mods**: how a pack is laid out, the manifest, the
  replacement formats and how mods layer over a pack.
- **Lua API**: the functions and tables the engine gives Lua hooks, screen
  scripts and enemy scripts.
- **.akscr scripts**: the room script language, version 1 for RE 1 and
  version 2 for RE 2.
- **Engine internals**: the architecture, performance budgets, status matrix
  and the recorded deviations from the original games.

The source is at
[github.com/arklay-engine/arklay](https://github.com/arklay-engine/arklay).
These pages are built from the `docs/` directory of the `main` branch, so
every commit to `main` updates them.
