# Scripting

Much of a pack's behaviour is Lua: the menus, the new-game state and any
hooks a pack adds. This page says where those scripts go in a pack and when
they run. The [Scripting reference](../scripting/index.md) documents what
they can call.

| Where | What | Runs |
|---|---|---|
| `ui/<screen>.lua` | [screens](#screens) | while the screen is open |
| `interactive/<screen>.lua` | [interactive screens](#interactive-screens) | each tick, while the room shows one |
| `game/initial_state.lua`, `game/new_game.lua` | [the new-game state](#the-new-game-state) | once, when a new game starts |
| `game/music.lua` | the room music tables (see [BGM](bgm.md#room-music)) | once, when a game session starts |
| `game/flags.lua` | [flag names](#flag-names) for the debug menu | when the debug menu's FLAGS screen opens |
| `lua/**.lua` | [hooks](#hooks) | on events: start, room load, each tick, each message |
| `lib/**.lua` | [shared modules](#shared-modules) | when another script `require`s them |

Rooms and enemies have scripts of their own, described with them: see
[Rooms](rooms.md#scripts) and [Enemies](enemies.md#the-script).

Every Lua script runs in a sandbox: it sees only the API the engine gives
it, cannot open files and has no `os` or `io` library. A mod can replace any
script by shipping a file at the same path.

## Screens

The title, the inventory, the item box, the map, the save screen and the
other menus are Lua screens under `ui/`: `ui/title.lua`, `ui/main_menu.lua`,
`ui/item_box.lua`, `ui/map.lua`, `ui/save_load.lua`, `ui/death.lua` and so
on. Each returns a table with `on_open`, `update` and `draw` functions.
Helpers shared by several screens go in `ui/lib/` (`ui/lib/fade.lua`,
`ui/lib/menu_grid.lua`). A hook can register its own save screen with
`api:register_save_screen`. See [Screen scripts](../scripting/lua/screen.md).

## Interactive screens

A few of the original's screens draw over the live room instead of pausing
it: the room keeps running, and the screen moves its camera and props. RE 1's
numeric panel in room 408 is `interactive/passcode_panel.lua` and its lab
computer in room 506 is `interactive/computer_lab.lua`. While scenario
flag `0x20` is up, the engine calls the script that a system flag selects
once a tick, between the room's scripts and its camera switch. It runs in the
enemy scripts' VM and keeps its state in a table the engine stores with the
game. See [Interactive screens](../scripting/lua/interactive.md).

## The new-game state

Two chunks set up a new game. Each replaces the whole file: a mod that ships
one restates everything in it that it wants to keep.

`game/initial_state.lua` writes the per-room state bytes (which also pick
each room's music), the item box's starting contents, the room-item bank
and the remembered pick-up quantities. Loading a save restores those from
the save instead and does not run it.

`game/new_game.lua` gives the starting inventory. It runs after
`game/initial_state.lua` as `local api, character = ...`, where `character`
is the number of the character the player chose. The inventory is empty
when it runs:

```lua
-- game/new_game.lua
local api, character = ...

if character == 0 then
    api:set_item_slot(0, 0x01, 0)        -- slot 0: item 01, no count
    api:set_item_slot(1, 0x41, 1)        -- slot 1: item 41, one of it
    api:set_partner_slot(0, 0x02, 15)    -- the partner's slot 0: item 02, 15 rounds
else
    api:set_item_slot(0, 0x01, 0)
    api:set_item_slot(1, 0x02, 15)
end
```

A chunk that fails to load or raises an error is logged and the pack's own
start items are used instead, so a broken mod never starts the player
empty-handed.

## Flag names

The debug menu's FLAGS screen (F1 while playing) names the bits of the
scenario, common, lock, enemy and item flag banks from `game/flags.lua`:

```lua
local api = ...

-- bank, bit, name, description
api:set_flag_name(1, 0x02, "KENNETH_ZOMBIE_SEEN", "The zombie eating Kenneth has been seen.")
api:set_flag_name(1, 0xA0, "J_BARRY_DINING_ENTRY", "Jill: ...")
api:set_flag_name(1, 0xA0, "C_MUSIC_SCORE_GIVEN", "Chris: ...")
```

A bit can have a name per player; the screen shows them as `J_.. / C_..`. A
mod's `game/flags.lua` replaces the whole file.

## Hooks

Hooks are Lua chunks that react to the game: `on_init` when the session
starts, `on_room_load` after each room loads, `on_tick` after each fixed tick
and `on_message` when a message opens. They are how a mod adds behaviour
without replacing a room or a screen:

```lua
-- lua/demo.lua
function on_room_load(api)
    api:log("entered " .. api:room())
end
```

A pack's hooks are the files its manifest's `lua` list names, in that order.
Without a list, every `lua/**.lua` file runs, in path order. Each mod runs
its own hooks after the layers below it. See
[Lua hooks](../scripting/lua/hooks.md).

## Shared modules

`require(name)` loads another Lua file of the pack and returns what it
returns. `name` is a pack path, with `.lua` optional:

| Form | Resolves |
|---|---|
| `/lib/fade` | from the top of the pack: `lib/fade.lua` |
| `./fade`, `../lib/fade` | from the requiring file's folder |
| `lib/fade` | as a path from the top first, then from the requiring file's folder |

`lib/` holds modules any script may share.

## Editor support

The engine ships LuaLS definition files for every API, for completion and
linting in an editor:

```sh
arklay mod lua-types --out ~/arklay-lua-types
```

Point the Lua language server at them in a `.luarc.json` beside your
scripts:

```json
{ "workspace.library": ["~/arklay-lua-types"] }
```
