---@meta arklay.hooks

-- Lua hooks: the scripts a pack carries under `lua/**.lua`.
--
-- This file is the source of the "Lua hooks" page of the
-- documentation, and a LuaLS definition file for linting and completion in
-- an editor. `arklay mod lua-types --out DIR` writes every definition file
-- the engine ships; add DIR to your editor's `workspace.library`.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.

---A pack opts into scripting by carrying `lua/**.lua` entries. The manifest's
---`lua` list selects and orders them; without one, every `lua/` entry runs in
---lowercased-path order. In a layered run each layer contributes its own list
---(the base first, then each mod in applied order), so a mod names its hooks
---in its own manifest.
---
---Each chunk is compiled and run once when the session opens. The engine then
---calls the global functions below if a chunk defines them:
---
---```lua
---local granted = false
---
---function on_init(api)
---    api:register_save_screen("script/mine.lua")  -- before any room
---end
---
---function on_room_load(api)
---    api:log("entered " .. api:room())
---    api:flag_set(0, 2, true)          -- bank 0, bit 2, set
---end
---
---function on_tick(api, tick)           -- tick is the completed fixed-tick count
---    if tick >= 30 and not granted then
---        granted = true
---        api:give_item(0x0F)
---    end
---end
---
---function on_message(api, id)          -- raised by the preceding tick
---    return id + 1                     -- nil/nothing keeps the original id
---end
---```
---
---## Sandbox and errors
---
---The VM loads the base, `string` and `table` libraries only: `os`, `io`,
---`package`, `debug`, `coroutine` and `math` do not exist, so there is no
---filesystem, network, clock or scheduling access. The base library's
---`dofile`, `loadfile`, `load` and `collectgarbage` globals are set to nil
---before any chunk runs, so a hook cannot load a file (or a binary chunk) or
---drive the collector, and the state carries a 16 MiB allocation ceiling. An
---instruction-budget hook aborts a script that runs away (the call errors out
---within a few million instructions). Every load or call error is logged once
---with a `[lua]` prefix and disables only the failing hook or chunk; the game
---session always continues. A pack with no `lua/` entries loads no VM, and
---runs stay byte-identical.
---
---Hooks cannot spawn enemies: the hook API has no such operation by
---construction. Entity behaviour is the separate enemy-script surface.

-- # Hook functions

---Called once the pack's scripts have loaded and before any room. It is where
---a pack registers things the engine reads outside a room, such as the
---save/load screen ([`api:register_save_screen`](#HookApi.register_save_screen)).
---
---The app runs it once in a throwaway VM and scratch state before the title
---appears; every session's own VM runs it again over its live state.
---@param api HookApi
function on_init(api) end

---Called once a room's state exists (RDT, init script and room edits
---applied) and before its first tick. It runs again on every room entry,
---including a door transition; the Lua globals themselves persist for the
---session.
---
---The room-load camera scan runs after it, so `api:set_room_camera` only
---sticks when the room's init script holds the camera lock; otherwise the cut
---is re-homed from camera 0 at the player's position, exactly like the
---original's `room_set`.
---@param api HookApi
function on_room_load(api) end

---Called after every fixed 30 Hz room tick, before that tick's audio
---dispatch.
---@param api HookApi
---@param tick integer `game.frame`, the number of completed room ticks.
function on_tick(api, tick) end

---Called when the tick raised a message, with its id word (see the `.akscr`
---pages). The window's bytes are resolved on the next tick, so the returned
---id is the one displayed.
---@param api HookApi
---@param id integer The message id word.
---@return integer? id The replacement id word, clamped to `0..=65535`; `nil` keeps the original.
function on_message(api, id) end

-- source: src/lua.rs#GameState
---The live game state behind a scoped userdata, passed to every hook. Only
---these methods exist; call them with Lua colon syntax (`api:room()`).
---
---The state-authoring methods (`set_*`, `clear_inventory`, `item_box_*`)
---exist for the CLI's `--setup-script`/`--save-out` oracle seam: a mod may
---use them too, but they are deliberately coarse. `set_room` only rewrites
---the identity the save block serialises; it does not load another room.
---@class HookApi
local HookApi = {}

-- # Reading state

---The three-digit room id.
---@return string room
function HookApi:room() end

---Whether the run was started with `--authentic` (the original's item rules).
---@return boolean
function HookApi:authentic() end

---Whether a flag bit is set.
---@param bank integer
---@param bit integer
---@return boolean
function HookApi:flag_get(bank, bit) end

---The total held quantity of an item.
---@param item integer The item id.
---@return integer
function HookApi:item_count(item) end

-- # Flags

---Set or clear a flag bit.
---@param bank integer
---@param bit integer
---@param value? boolean Defaults to `true`, so `api:flag_set(0, 1)` sets the bit.
function HookApi:flag_set(bank, bit, value) end

---Overwrite a bank's raw bytes, for state the named methods do not reach.
---@param bank integer
---@param bytes string|integer[] A string, or a sequence table of byte values.
function HookApi:set_flag_bank(bank, bytes) end

---Write one BioCard state-image byte.
---@param index integer
---@param value integer
function HookApi:set_state_byte(index, value) end

---Name a game bank's bit for the debug menu's FLAGS screen
---(`game/flags.lua`). A bit can take one name per player.
---@param bank integer
---@param bit integer
---@param name string
---@param description? string
function HookApi:set_flag_name(bank, bit, name, description) end

---Set or clear a FILE-tab document's collected bit.
---@param item integer A file document's item id, `0x5F..=0x6E`.
---@param value? boolean Defaults to `true`.
function HookApi:set_file_collected(item, value) end

---Set or clear a key, special or chemical item's use flag.
---@param item integer
---@param value? boolean Defaults to `true`.
function HookApi:set_item_use_flag(item, value) end

-- # Inventory and item box

---Add items through the normal merge rules.
---@param item integer
---@param quantity? integer Defaults to one.
function HookApi:give_item(item, quantity) end

---Clear the whole inventory slot holding the item.
---@param item integer
function HookApi:item_remove(item) end

---Replace a player slot's stack. A slot past the held run appends; the
---inventory compacts so no empty slot survives.
---@param slot integer Errors when out of range for the character.
---@param id integer The item id; `0` drops the stack.
---@param quantity? integer Defaults to one.
function HookApi:set_item_slot(slot, id, quantity) end

---Replace RE 2's eleventh slot, the lighter's or the lockpick's.
---@param id integer The item id; `0` empties it.
---@param quantity? integer Defaults to one.
function HookApi:set_personal_item(id, quantity) end

---Replace one of Rebecca's six partner slots.
---@param slot integer `0..6`.
---@param id integer The item id; `0` empties it.
---@param quantity? integer Defaults to one.
function HookApi:set_partner_slot(slot, id, quantity) end

---Empty the inventory and clear the equipped and selected item.
function HookApi:clear_inventory() end

---Equip an item, deriving the weapon id from it.
---@param id integer? The item id; `nil` or `0` unequips.
function HookApi:set_equipped(id) end

---Set the weapon id the firing model uses, without touching the equipped
---item.
---@param id integer
function HookApi:set_equipped_weapon(id) end

---Replace one of the 48 item-box slots.
---@param slot integer `0..48`.
---@param id integer
---@param quantity? integer Defaults to one.
function HookApi:item_box_set(slot, id, quantity) end

---Empty the whole item box.
function HookApi:item_box_clear() end

---Replace one of RE 2's 64 item box slots.
---@param slot integer `0..64`.
---@param id integer
---@param count integer
---@param mark? integer `3` for a two-slot weapon; defaults to `0`.
function HookApi:set_re2_box_slot(slot, id, count, mark) end

-- # Player

---Set the player's health and its BioCard copy.
---@param hp integer
function HookApi:set_health(hp) end

---Set the player's maximum health.
---@param hp integer
function HookApi:set_max_health(hp) end

---Set the health-status flags, poison bits included.
---@param byte integer
function HookApi:set_health_status(byte) end

---Set the player entity's position. In an RE 2 room this follows the same
---request as a script's `pos_set`, so it holds during a session too.
---@param x integer
---@param y integer
---@param z integer
function HookApi:set_player_pos(x, y, z) end

---Set the player's 12-bit yaw.
---@param degrees integer
function HookApi:set_player_angle(degrees) end

---Select the character: identity, model and entity id. The variant selects
---the room RDT, so a script that changes it usually also picks a room booted
---for that character.
---@param id integer `0` Chris, `1` Jill.
function HookApi:set_character(id) end

-- # Room

---Rewrite the room identity the save block records (authoring only). The
---session keeps running the room it booted, like a save written while the
---block is edited.
---@param stage integer
---@param room integer
---@param mansion2? boolean Names a mansion room's second visit.
function HookApi:set_room(stage, room, mansion2) end

---Select the room camera cut.
---@param cut integer
function HookApi:set_room_camera(cut) end

---Deactivate a running event slot.
---@param slot integer
function HookApi:kill_event(slot) end

---Request a room or global message by its id word.
---@param id integer
function HookApi:message(id) end

-- # Music

---Define one preset of up to three numeric music tracks. The track id names
---`bgm/<id:03x>`; the group id is a logical preset, not a file. An empty group
---is silent. More than three tracks, or group `0xFF` (reserved for "no
---group"), raises an error.
---@param group integer
---@param tracks { [1]: integer, [2]: boolean? }[] Up to three `{ track, loop }` pairs.
function HookApi:set_music_group(group, tracks) end

---Replace a room's up-to-four group variants. A stage or room outside the
---music table, or more than four groups, raises an error.
---@param stage integer The 1-based stage digit, `1..=5`.
---@param room integer
---@param groups integer[]
---@param options? { mansion2: boolean? } `mansion2` sets a mansion room's second visit's row.
function HookApi:set_room_music(stage, room, groups, options) end

---Write the saved per-room music state byte (variant, mode and channel
---mask).
---@param stage integer The 1-based stage digit, `1..=5`.
---@param room integer
---@param state integer
---@param options? { mansion2: boolean? } `mansion2` writes the second visit's byte.
function HookApi:set_room_music_state(stage, room, state, options) end

-- # RE 2 state

---Set one bit of RE 2's flag banks, numbered as `ck` and `set` number them.
---In a `--setup-script`, the room's scripts start again if any bit changed.
---@param bank integer
---@param index integer
---@param value? boolean Defaults to `true`.
function HookApi:set_re2_flag(bank, index, value) end

---Set where RE 2's `Rnd` stream stands. Only a new game seeds it and a door
---does not, so this is for a room reached any other way than the original
---was.
---@param value integer
function HookApi:set_re2_rnd(value) end

---Set `Sce_rnd_set`'s random state, like `set_re2_rnd`.
---@param value integer
function HookApi:set_re2_sce_rnd(value) end

---The oracle's `--no-enemies`: switch off every enemy slot at the top of each
---tick (bit 0 of `be_flg`) except the kept pairs, so a long route compares
---the player alone.
---@param keep? string[] `"ROOM:SLOT"` pairs to keep, the room in three hex digits, e.g. `"101:0"`.
function HookApi:set_re2_no_enemies(keep) end

---Set the `Dest` the next zombie placed in that slot of that room starts
---with, which the original takes from whatever its new work's memory held.
---@param pair string `"ROOM:SLOT"`, the room in three hex digits.
---@param x integer
---@param z integer
function HookApi:set_re2_enemy_dest(pair, x, z) end

-- # Screens and logging

---Run `entry` as the save/load screen instead of the ported original.
---@param entry string? A pack path; `nil` restores the original.
function HookApi:register_save_screen(entry) end

---Write one `[lua]` line to stderr.
---@param text string
function HookApi:log(text) end
