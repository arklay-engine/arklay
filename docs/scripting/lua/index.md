# Lua API

Lua runs in several places, and each place has its own API. Every API runs
in a sandbox: the base, `string` and `table` libraries are there, but `os`,
`io`, `package`, `debug`, `coroutine` and `math` are not, so a script has no
file, network or clock access. A script reaches other pack files only with
`require` (see [Shared modules](../../packs/scripting.md#shared-modules)).

| Page | Scripts | Pack files |
|---|---|---|
| [Lua hooks](hooks.md) | the `on_init`, `on_room_load`, `on_tick` and `on_message` functions of a pack's hooks, with the live game state | `lua/**.lua`, and `game/*.lua` |
| [Screen scripts](screen.md) | whole screens in Lua: the title, the menus, a save screen a pack registers with `api:register_save_screen` | `ui/<screen>.lua` |
| [GUI DOM](gui.md) | the scripting side of the engine's XML menu markup; a pack cannot supply a GUI script yet | |
| [Enemy scripts](enemy.md) | RE 1's per-tick `update(e)` for every enemy and character | `enemy/<id>/enemy.lua` |
| [Interactive screens](interactive.md) | RE 1's screens over the live room, such as the lab computer: a per-tick `display(api, s)` | `interactive/<screen>.lua` |
| [RE 2 enemy scripts](re2-enemy.md) | RE 2's per-tick `update(e)` | `enemy/<id>/enemy.lua` |
| [Driver scripts](script.md) | a program `arklay script FILE` runs, outside any pack, that boots the game and drives it tick by tick: for tests and automation. Its sandbox also has `math` and `utf8` | |

The pages are generated from the LuaLS definition files the engine ships,
which an editor can use for completion and linting:

```sh
arklay mod lua-types --out ~/arklay-lua-types
```

The same files are next to these pages, as
[`lua-types/*.lua`](https://github.com/arklay-engine/arklay/tree/main/lua-types)
and [`arklay-lua-types.zip`](../../arklay-lua-types.zip). Point the Lua
language server at them in a `.luarc.json` beside your scripts:

```json
{ "workspace.library": ["~/arklay-lua-types"] }
```
