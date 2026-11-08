---@meta arklay.script

-- Driver scripts: the programs `arklay script FILE` runs.
--
-- This file is the source of the "Driver scripts" page of the
-- documentation, and a LuaLS definition file for linting and completion in
-- an editor. `arklay mod lua-types --out DIR` writes every definition file
-- the engine ships; add DIR to your editor's `workspace.library`.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.

---A driver script is a Lua program that runs the engine step by step: it
---opens a game pack, boots a session, holds buttons, advances ticks and reads
---or writes the game state between them. It is the way to automate arklay,
---for tests and for agents, without a command-line flag for every case.
---
---```sh
---arklay script check.lua            # with a window showing every tick
---arklay script --headless check.lua # no window: CI, SSH, agents
---arklay script --headless - < check.lua
---arklay script --headless check.lua 106 30   # arg[1], arg[2]
---```
---
---```lua
---load_game_pack("games/re1.akpak")
---load_room(arg[1] or "106")
---hold("forward")
---tick(30)
---release()
---local x, y, z = game:player_pos()
---print(json({ room = game:room(), pos = { x, y, z }, health = game:health() }))
---capture("after.png")
---```
---
---## How a run behaves
---
---- **The script is the program.** Nothing happens until the script calls
---  for it, and the run ends when the script returns.
---- **One tick is one app update**, the unit `--ticks` counts: a fixed
---  30 Hz tick while a room plays, a 16 ms step on a UI screen.
---- **Deterministic.** The user's `arklay.toml` and installed mods are
---  ignored, and the run options are the oracle runs' unless the script sets
---  them in [`load_game_pack`](#load_game_pack). No audio device opens and no
---  film plays. In-game saves go to a scratch directory that is removed at
---  the end of the run, unless `save_dir` names one.
---- **Window or headless.** By default a window shows every tick at the
---  game's own pace; its keyboard and pads are ignored, so the frames are the
---  ones a `--headless` run draws. Closing the window fails the run. Where no
---  window can open, the run fails and says to pass `--headless`.
---- **Output.** Only [`print`](#print) writes to stdout. Engine messages and
---  warnings go to stderr.
---- **Errors are strict.** A Lua error, a failed engine call (a missing pack,
---  a bad room id) or an engine warning raised during a call fails the run
---  with exit status 1 and the error on stderr. [`strict(false)`](#strict)
---  lets warnings through; [`exit`](#exit) ends the run with a status of its
---  own. `--timeout SECONDS` fails a run that takes too long.
---- **Sandbox.** The base, `string`, `table`, `math` and `utf8` libraries
---  load; `io`, `os`, `package` and `debug` do not, and `dofile`, `loadfile`,
---  `load` and `collectgarbage` are removed. The engine calls below are the
---  only file access. The state may allocate up to 256 MiB.

-- # Globals

---### `arg`
---
---The script's arguments: `arg[0]` is the script's path (or `stdin`), and
---`arg[1]`, `arg[2]`, ... are the arguments after it on the command line.
---
---### `game`
---
---The running session's game state: the [hook API](hooks.md#HookApi) the
---mod hooks get as `api`, called the same way (`game:flag_get(0, 5)`,
---`game:give_item(0x0F)`, `game:player_pos()`).
---
---Call it with a colon: `game.flag_get(0, 5)` is an error. Calling it with
---no session running is an error too.
---
---Calls change the live state, and the player follows a position or angle
---a call wrote. Between a boot and the first tick, edits author the booted
---state the way `--setup-script` does: the first `tick` (or a `save_game` or
---`capture` before it) reruns the room's init scripts once if the edits
---changed the RE 2 flags or random stream. The getters (`room`,
---`player_pos`, `flag_get`, ...) do not count as edits.

---@type string[]
arg = {}

---@type HookApi
game = {}

-- # Booting

---Open a game pack, replacing any open one. A session starts with a boot
---call after it.
---
---```lua
---load_game_pack("re2.akpak", { difficulty = "hard", mods = { "mods/hd.akpak" } })
---```
---@param path string The game pack.
---@param options? { mods?: string[], language?: string, save_dir?: string, quick_turn?: boolean, tactical_reload?: boolean, authentic?: boolean, auto_aim?: boolean, blood?: boolean, difficulty?: "normal"|"easy"|"hard", arrange?: boolean } `mods` are the mod layers to apply in order (none by default); `language` the pack language to use; `save_dir` where in-game saves go (default: a scratch directory removed at exit). The rest are the run options, as the command-line flags of the same names set them: all off by default except `auto_aim` and `blood`, and `difficulty` defaults to `normal`. An unknown key is an error.
function load_game_pack(path, options) end

---Boot a room directly, like `arklay PACK --room ROOM`: a fresh game state
---with the player at the room's entry.
---@param room string Three hex digits: the stage digit plus the room, e.g. `"106"`; RE 2's letter stages are A-G, e.g. `"G01"`.
---@param options? { player?: integer, mansion2?: boolean } `player` is the digit (`0`-`9`) that selects the RDT variant (default 0); `mansion2` boots a mansion room on its second visit.
function load_room(room, options) end

---Start a new game through the session flow, like `--ui game`: the
---new-game items, flags and health, and the start room or `room`.
---@param options? { character?: integer, room?: string, player?: integer, mansion2?: boolean } `character` is `0` Chris/Leon or `1` Jill/Claire (RE 2: `2`/`3` for scenario B); `room` boots straight into that room instead of the start room, with `player` (default: the character) and `mansion2` as in `load_room`.
function new_game(options) end

---Continue from a save block (`savedatN.dat`), like `--load FILE`.
---@param path string
function load_save(path) end

-- # Input

---The buttons are the `--input` names: `up` (or `forward`), `down` (or
---`backward`), `left`, `right`, `confirm` (or `action`, `cross`), `cancel`
---(`circle`), `run`, `aim` (`square`), `fire`, `start` (`inventory`),
---`page_left` (`l1`), `page_right` (`r1`) and `f9` (`return_title`), plus
---`back` for `down`, and every other action by its `[bindings]` name (`quick_turn`, `reload`, `map`, `weapon_next`,
---...). An unknown name is an error.

---Hold buttons from the next tick on, until they are released.
---
---```lua
---hold("forward", "run")
---tick(40)
---release()
---```
---@param ... string
function hold(...) end

---Release held buttons; with no names, release them all.
---@param ... string
function release(...) end

---Hold buttons for the next tick only. A button pressed on two ticks in a
---row is one long press: tick once between two presses of the same button.
---@param ... string
function press(...) end

---Replace the held buttons with a raw key word (the `--input` key bits).
---@param word? integer The new held word; nil leaves it.
---@return integer previous The held word before the call.
function pad(word) end

-- # Time

---Advance the game: one app update per tick, with the buttons held now.
---@param count? integer Ticks to run (default 1).
function tick(count) end

---Tick until `condition` returns true, checking it before each tick. It is
---an error if the condition still fails after `max` ticks.
---
---```lua
---tick_until(function() return game:room() == "107" end, 300)
---```
---@param condition fun(): boolean
---@param max integer
---@return integer ticks The ticks it took (0 when the condition already held).
function tick_until(condition, max) end

---The app's current screen: `play` while a game session runs, else
---`title`, `select`, `load`, `save`, `movie`, `ending`, `epilogue`, or
---`none` before a pack is open or a session booted.
---@return string
function mode() end

-- # Output

---Write the arguments to stdout, separated by tabs, with a newline: the
---base library's `print`, on the only stream the script owns.
---@param ... any
function print(...) end

---Encode a value as JSON. A table with only the keys `1..n` is an array,
---any other an object with sorted keys; an empty table is `[]`. Functions
---and userdata have no JSON form.
---@param value any
---@return string
function json(value) end

---Write the current frame as a PNG, or a BMP when `path` ends in `.bmp`.
---@param path string
function capture(path) end

---Write the session's state as a save block (`savedatN.dat`), which
---`load_save` and `--load` read back: the `--save-out` seam.
---@param path string
function save_game(path) end

---Start writing the oracle trace (`oracle/trace-v1.md`) to `path`: one line
---per gameplay tick from the next tick on, as a `--ui game --trace` run
---writes. A trace already running is closed first.
---@param path string
---@param options? { all_frames?: boolean } `all_frames` also traces the frames of a door transition and the death screen, as a `--trace` run with `--input` does.
function trace_start(path, options) end

---Close the running trace. The run closes it at the end too.
function trace_stop() end

-- # Control

---Fail the run on engine warnings (the default), or let them through. A
---warning fails the engine call that raised it, with the first warning's
---text.
---@param on? boolean Default `true`.
function strict(on) end

---End the run now with an exit status. A `pcall` around it cannot catch it.
---@param code? integer `0..=255`; default 0.
function exit(code) end
