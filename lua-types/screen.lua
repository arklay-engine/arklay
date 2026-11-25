---@meta arklay.screen

-- Screen scripts: the Lua scripts that implement whole screens.
--
-- This file is the source of the "Screen scripts" page of the
-- documentation, and a LuaLS definition file for linting and completion in
-- an editor. `arklay mod lua-types --out DIR` writes every definition file
-- the engine ships; add DIR to your editor's `workspace.library`.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.

---A screen script draws and drives one full-screen menu. Nothing about a
---screen is built into the engine: the game's own screens are pack scripts,
---and a pack or mod can replace any of them or register its own save/load
---screen. A screen script runs only while its screen is open; it is not a
---[hook](hooks.md), which runs for the whole session.
---
---There are two kinds of screen:
---
---- **Engine screens** are the pack entries `ui/<id>.lua`: `title`,
---  `char_select`, `save_load`, `ending_result`, `ending_epilogue`,
---  `return_title`, `death`, `main_menu`, `map`, `file`,
---  `item_view` and `item_box` for RE 1, and `title`, `char_select`,
---  `main_menu` and `item_box` for RE 2. Shared code lives under `ui/lib/`.
---  An entry in the pack or a mod replaces the copy built into the engine,
---  which is used for any `ui/` entry the pack lacks. The engine opens each
---  screen when the game needs it and passes it an `args` table of its own.
---- **A registered save/load screen** is any pack entry a hook names with
---  [`api:register_save_screen(entry)`](hooks.md#HookApi.register_save_screen)
---  from `on_init`. It replaces the shipped `ui/save_load.lua` for the
---  title's LOAD GAME and for the typewriter. The checked-in
---  `mods/crloadsave/` is the reference, a Classic REbirth-style folder
---  screen:
---
---```
---mods/crloadsave/manifest.toml        lua = ["mod.lua"]
---mods/crloadsave/mod.lua              function on_init(api) api:register_save_screen("script/loadsave.lua") end
---mods/crloadsave/script/loadsave.lua  the screen itself
---mods/crloadsave/language/*.txt       its UI_SAVE_CR_* strings
---```
---
---The registration is pack-level. The app runs the pack's hooks once in a
---throwaway VM before the title appears, so the title's LOAD GAME and the
---load screen honour it, and every session's own VM runs `on_init` again
---for the typewriter. A registered screen also changes the title's
---existence probe: it counts every loadable file in the save directory
---rather than the eight `savedatN.dat` slots, since the script lists the
---directory itself.
---
---## The script and its entry points
---
---The engine runs the script's chunk once when the screen opens. If the
---chunk returns a table, the entry points are that table's fields; this is
---the form every engine screen uses:
---
---```lua
---local M = {}
---function M.open(api, args) end    -- once, before the first tick
---function M.update(api, input) end -- once per fixed 30 Hz tick
---function M.draw(api) end          -- once per presented frame
---return M
---```
---
---A chunk that returns nothing uses its own globals instead (the form
---`mods/crloadsave` uses). `on_open` is accepted as well as `open`:
---
---```lua
---function on_open(api) end
---function update(api, input) end
---function draw(api) end
---```
---
---`update` is required: a script without one fails to load, because the
---screen could never leave. `open` and `draw` are optional. The engine may
---also call other functions of the table, the signals listed under
---[`ScreenModule`](#ScreenModule); a script that does not define one ignores
---that signal.
---
---Each call gets a fresh [`ScreenApi`](#ScreenApi) userdata, valid only for
---that call: do not keep it in a variable for later.
---
---## Lifecycle
---
---1. The chunk runs, then `open(api, args)` runs once. An error in either
---   means the screen does not open, and the opener falls back (below).
---2. Every fixed tick, `update(api, input)` runs with the tick's
---   [input](#ScreenInput). After it returns, a successful
---   [`api:load`](#ScreenApi.load) continues the game from that save, and an
---   [`api:exit`](#ScreenApi.exit) closes the screen.
---3. Every presented frame, `draw(api)` runs. The drawing methods work only
---   here, and [`api:clip`](#ScreenApi.clip) lasts until `draw` returns.
---
---[`api:ticks`](#ScreenApi.ticks) counts the completed `update` calls, so it
---is 0 during the first one.
---
---## Layout space
---
---A screen draws in a layout space of [`api:width()`](#ScreenApi.width) x
---[`api:height()`](#ScreenApi.height) pixels. At the default 4:3 aspect it is
---320x240. A screen should centre its 320x240 art on
---`(width // 2 - 160, height // 2 - 120)` rather than assume that size,
---because a wider `aspect` (see `arklay.toml`) changes the space in one of
---two ways:
---
---- **The band.** By default a screen draws in the cropped band the window
---  shows: 320 wide and `320 * h / w` rows (180 at 16:9, 200 at 16:10). A
---  screen whose content is taller has to lay it out again: the shipped title
---  shows the bottom of its art, the save screen stacks its rows tighter and
---  the ending captions are pulled inside the band.
---  [`api:background`](#ScreenApi.background) keeps stretching over the
---  logical 320x240 quad centred in the space, so the band crops a
---  full-screen picture rather than squashing it.
---- **The canvas.** A screen drawn on black can call
---  `api:set("canvas", 1)`. The frame then becomes a full-height canvas of the
---  aspect's width (427x240 at 16:9, 384x240 at 16:10), so the screen keeps
---  every row and gains columns at its sides. The pause menu, the item box,
---  the file and map screens and the examine viewer use it.
---
---Model draws ([`api:draw_ivm`](#ScreenApi.draw_ivm),
---[`api:draw_character`](#ScreenApi.draw_character),
---[`api:draw_actor`](#ScreenApi.draw_actor)) project about the centre of
---the layout space, so a camera's `screen` offset stays relative to the
---centre of the 320x240 layout.
---
---Coordinates and sizes passed to the drawing methods are clamped to
---`-4096..=4096`.
---
---## The lent game state
---
---Most methods that read the game ([`api:flag`](#ScreenApi.flag),
---[`api:inventory`](#ScreenApi.inventory), ...) see the paused session
---read-only, and answer a neutral value (`false`, `0`, `nil`) when there is
---no session, as on the title. The pause menu (`main_menu`) changes the game:
---the engine lends it the session's state for `open`, `update` and its
---signals, and the methods under "Changing the game (lent)" work only there.
---From any other screen, or from `draw`, they raise an error.
---
---## Sandbox and errors
---
---A screen runs in its own VM with the hooks' sandbox: the base, `string`
---and `table` libraries only, no `dofile`, `loadfile`, `load` or
---`collectgarbage`, a 16 MiB allocation ceiling and an instruction budget per
---call. See [Sandbox and errors](hooks.md#sandbox-and-errors) on the hooks
---page.
---
---`require(name)` loads another screen module: every `ui/**.lua` entry of
---the pack and its mods, the built-in `ui/` copies, and the screen's own
---entry. `name` is a pack path with `.lua` optional: `/a/b` is from the pack
---root, `./b` and `../a/b` are relative to the requiring module's directory,
---and a bare `a/b` is tried as an exact entry first and relative second. Each
---module runs once per VM and its value is cached (`true` when it returns
---nothing); a cycle is an error. Each chunk gets its own environment that
---falls back to the VM's globals, so a module's globals stay its own.
---
---When a script fails to load or `open` fails, the opener falls back: a
---registered save/load screen falls back to the shipped `ui/save_load.lua`
---with a warning, and an engine screen takes its fallback action (back to
---the title or to gameplay) or does not open. A script that raises an error
---while running is logged once with a `[lua]` prefix and the screen leaves:
---back to gameplay from a save screen, back to the title from the load
---screen, and the opener's fallback from an engine screen. It is never
---called again.
---
---Decoded images drawn by entry name are cached per screen, up to 64 images
---and 64 MiB of pixels; art loaded by handle has its own cache of 64 live
---handles (item icons are exempt) and 64 MiB. Going over either budget is an
---error, so a screen that pages through much art should
---[`release`](#ScreenApi.release) what it no longer draws.

-- # Entry points

---The table a screen chunk returns. The engine calls the functions below if
---the table has them. `open`, `update` and `draw` are the entry points; the
---rest are signals the engine sends particular screens, each called with
---the `api` and any values shown, at the moment described.
-- calls: src/ui/lua_screen/mod.rs src/engine.rs
---@class ScreenModule
local ScreenModule = {}

---Called once when the screen opens, before the first tick. The legacy
---global form calls it `on_open`.
---@param api ScreenApi
---@param args ScreenArgs The opener's arguments; empty for a save/load screen.
function ScreenModule.open(api, args) end

---The older name of [`open`](#ScreenModule.open), used when a script
---defines no `open`.
---@param api ScreenApi
---@param args ScreenArgs
function ScreenModule.on_open(api, args) end

---Called once per fixed 30 Hz tick with that tick's input. Required.
---@param api ScreenApi
---@param input ScreenInput
function ScreenModule.update(api, input) end

---Called once per presented frame. The only call in which the drawing
---methods work.
---@param api ScreenApi
function ScreenModule.draw(api) end

---Called on a title screen when the load screen it left for comes back
---without loading a save. A title that has it is kept as it stands while
---the load screen is up and carries on from its next `update`; one without
---it is opened anew.
---@param api ScreenApi
function ScreenModule.on_resume(api) end

---Called on a title screen with [`on_resume`](#ScreenModule.on_resume) when
---the load screen it left for chose a save
---([`api:load`](#ScreenApi.load)): the title wakes as it stood and starts
---the game from that save when it is ready, by exiting with `continue`.
---Any other exit forgets the save. A title without it is dropped, and the
---game starts at once. For an RE 2 save, `place` is its save place and
---`status` its `fg_status`, by which RE 2's title chooses the prologue the
---load runs; both are nil for any other save.
---@param api ScreenApi
---@param place integer?
---@param status integer?
function ScreenModule.on_loaded(api, place, status) end

---Called on a title screen when a film it left for
---([`api:exit("film", id)`](#ScreenApi.exit)) is over: the title wakes as
---it stood and carries on from its next `update`. A film the pack lacks,
---or a run that plays no films, is over at once, before the next `update`,
---and never `played`. A title that has this plays the films of the new
---game it starts itself, and the engine adds none of its own to that game;
---the boot's opening film is [`on_boot`](#ScreenModule.on_boot)'s.
---@param api ScreenApi
---@param skipped boolean Whether a press ended the film.
---@param played boolean Whether the film started at all.
function ScreenModule.on_film(api, skipped, played) end

---Called on the title screen the game's boot opens, right after its
---`open` (once per run, and whenever a control script opens the title with
---`boot`): the title runs the boot itself, showing what the game
---shows before its title and asking for its own opening film
---([`api:exit("film", id)`](#ScreenApi.exit), with
---[`on_film`](#ScreenModule.on_film)). The engine adds no opening film of
---its own to a title that has this; a title without it gets the engine's.
---Not called on a title opened later, or by `--ui title`.
---@param api ScreenApi
function ScreenModule.on_boot(api) end

---Called on a title screen, or on the new-game choice's entry, with the
---run's music and effects volumes in percent (0 to 100): once as the
---screen opens, again whenever the settings change, and when a load takes
---them from its save while the screen is up (a save keeps the run's
---volumes, as the original's does, and a load gives them back as the game
---it continues starts; a title opened after it is told them). It is called
---as the screen opens in every run, a headless one too, so a screen that
---sets up its own interface from this hook does that on every open. For a
---title with an option screen of its own that shows and sets them. Such a screen publishes the two it
---shows as `music_volume` and `effects_volume`
---([`api:set`](#ScreenApi.set)) from this hook, and again whenever one
---changes: what it publishes from the hook is what it shows of what it was
---told, and when a published value changes to another from 0 to 100 the
---engine takes that as the run's new volume. In the interactive game the
---volumes are the player's settings, and a change is mixed with and saved;
---in a run with no settings (a headless one) they are the run's own, 100
---and 100 from the boot, and a change is only kept. A value the screen
---shows otherwise than it was told (a range of its own) is so no change
---until it moves. Both published values are forgotten as the hook is
---called, and nothing is taken of a volume the hook doesn't publish, or of
---a value outside 0 to 100. A screen without the hook may publish what it
---likes under the two names.
---
---The speaker mode and the sound quality come with them, and are taken
---back the same way from `speaker_mode` (1 or 2) and `sound_quality` (0 to
---2). In the interactive game they are the player's settings: mono mixes
---every sound to both speakers alike, and places a 3D sound by its whole
---distance with no pan; the quality is kept in the settings but doesn't
---change the mix. In a headless run they are the run's own, stereo and high
---from the boot.
---@param api ScreenApi
---@param music integer The music volume, 0 to 100.
---@param effects integer The effects volume, 0 to 100.
---@param speaker integer The speaker mode: 1 mono, 2 stereo.
---@param quality integer The sound quality: 0 high (44.1 kHz, 16 bits), 1 middle (22 kHz, 16 bits), 2 low (22 kHz, 8 bits).
function ScreenModule.on_volumes(api, music, effects, speaker, quality) end

---Called on a title screen in the interactive game with the player's
---bindings: as the screen opens, and again whenever they change (the in-game
---menu's controls, or the screen's own [`api:set_bindings`](#ScreenApi.set_bindings)).
---Both tables hold every action by its `[bindings]` name, each as
---`{ keys = {...}, pads = {...} }`: its keyboard keys and its gamepad buttons
---and axes by their `device:name` (`key:W`, `pad:a`, `axis:lefttrigger+`),
---in the order the settings list them. `bindings` is the settings',
---`defaults` the shipped layout's. A run with no settings (a headless one)
---never calls it. RE 2's title lists the bindings on its KEYCONFIG page.
---@param api ScreenApi
---@param bindings table<string, { keys: string[], pads: string[] }>
---@param defaults table<string, { keys: string[], pads: string[] }>
function ScreenModule.on_bindings(api, bindings, defaults) end

---The published view: what the screen shows a player, as a table of
---integers, numbers, strings, booleans and nested tables of them, for
---observers such as the player bot. It is the screen's contract with them:
---they read nothing else of the screen's state, so the script may keep that
---state however it likes. The engine calls it between ticks with no
---arguments; it must only read. A screen without one publishes nothing.
---
---RE 1's title publishes `stage` (`"press"`, `"menu"`, `"fade_out"` or
---`"attract_fade"`), `selection` (1 NEW GAME, 2 LOAD GAME) and `any_saves`
---(whether LOAD GAME can be chosen); the bot drives the title from those.
---RE 1's `main_menu` publishes its cursor, the slot and item under it, the
---submenu's mode, option and labels, its open and close animations and the
---MOVE cursor; its `item_box` publishes the box's mode, box cursor,
---inventory cursor and whether a page slides. The bot drives both from
---those.
---@return table view
function ScreenModule.view() end

---Signal to `main_menu`: one frame of the menu's own animation (a use
---confirmed on the previous frame, the CHECK slide, the submenu box, the
---health monitor), apart from its input handling in `update`. The game
---state is lent.
---@param api ScreenApi
function ScreenModule.tick(api) end

---Signal to RE 1's `main_menu`: per frame while the item viewer is up over
---the menu, so the status panel's monitor keeps sweeping under the model.
---@param api ScreenApi
function ScreenModule.tick_monitor(api) end

---Signal to RE 1's `main_menu`: the item viewer it opened has closed (or
---could not open).
---@param api ScreenApi
function ScreenModule.view_closed(api) end

---Signal to RE 1's `main_menu`: an examine replaced the slot under the
---cursor (the medal), so the menu should select it again.
---@param api ScreenApi
function ScreenModule.reselect(api) end

---Signal to RE 1's `main_menu`, before each `draw`: what this frame shows.
---@param api ScreenApi
---@param show_cursor boolean Whether the item cursor is drawn.
---@param show_name boolean Whether the selected item's name is drawn.
---@param backdrop boolean Whether the menu draws its own backdrop.
function ScreenModule.present(api, show_cursor, show_name, backdrop) end

---Signal to RE 2's `main_menu`: the status screen's open and close
---animation state for this frame.
---@param api ScreenApi
---@param open boolean
---@param close boolean
function ScreenModule.animate(api, open, close) end

---Signal to RE 2's `main_menu` opened in `get` mode (the get-item window):
---its open and close animation state for this frame.
---@param api ScreenApi
---@param open boolean
---@param close boolean
function ScreenModule.animate_get(api, open, close) end

---Signal to `item_view` opened for a pick-up: the pick-up is done and the
---viewer should play its exit.
---@param api ScreenApi
function ScreenModule.begin_exit(api) end

---Signal to `item_box`, before each `draw`: whether the cursor is shown this
---frame.
---@param api ScreenApi
---@param shown boolean
function ScreenModule.cursor_shown(api, shown) end

---Signal to the pause menu while the item box is open over it: after each
---frame of the box's input, the slot its cursor is on. None of the shipped
---menus handles it; a menu that draws its own item-box cursor can.
---@param api ScreenApi
---@param cursor integer The item box cursor's slot.
function ScreenModule.move_cursor(api, cursor) end

-- source: src/ui/lua_screen/methods.rs#Api src/ui/lua_screen/draw/methods.rs src/ui/lua_screen/game_api.rs src/ui/lua_screen/option_model.rs src/ui/lua_screen/bindings.rs
---The screen surface, passed to every entry point and signal. Only these
---methods exist; call them with Lua colon syntax (`api:width()`).
---
---Methods marked *draw only* raise `draw methods need draw(api)` when
---called outside `draw`; every other method works from any call. Methods
---under "Changing the game (lent)" need the lent game state.
---@class ScreenApi
local ScreenApi = {}

-- # The screen

---Which way the screen was opened: `"load"` for the title's load screen,
---`"save"` for a save (the typewriter, the next-cycle ending save), and
---`"screen"` for an engine screen opened by id.
---@return "load"|"save"|"screen"
function ScreenApi:mode() end

---The number of fixed ticks since the screen opened: 0 during the first
---`update`.
---@return integer
function ScreenApi:ticks() end

---The layout space's width: 320 at 4:3, 320 in the band, the aspect's width
---on the canvas. See "Layout space" above.
---@return integer
function ScreenApi:width() end

---The layout space's height: 240 at 4:3 and on the canvas, the band's rows
---otherwise.
---@return integer
function ScreenApi:height() end

---Publish a value the engine reads back. Each engine screen has its own
---keys (the pause menu's `cursor`, the item box's `selected_item`, ...);
---`canvas` set to 1 asks any screen's frame for the full-height canvas.
---Up to 96 keys; a new key past that raises an error, overwriting one never
---does. An unset key reads as 0.
---@param key string
---@param value integer|boolean|nil An integer, a boolean (stored as 1 or 0), or `nil` to clear the key. A float or any other type raises an error.
function ScreenApi:set(key, value) end

---Ask for the player's bindings to change, from a screen that shows them
---([`on_bindings`](#ScreenModule.on_bindings)). Each key is an action's
---`[bindings]` name and each value `{ keys = {...}, pads = {...} }`: a list
---given replaces the action's keyboard keys (`keys`) or its gamepad buttons
---and axes (`pads`), by their `device:name`; a list left out keeps that
---device's as it is, and the action's mouse buttons and wheel always stay.
---The engine applies the change after the tick and tells the screen the
---bindings again with `on_bindings`. In the interactive game the change is
---saved to `arklay.toml`, unless it would leave a menu action without a key
---of its own (then nothing changes and a warning is logged); in a run with no
---settings (a headless one) it is ignored. An unknown action, a name that
---isn't a source, a source on the other device or a reserved input (F1, F9,
---the pad's start) raises an error.
---@param changes table<string, { keys?: string[], pads?: string[] }>
function ScreenApi:set_bindings(changes) end

---Keep a value past this screen: the engine holds it for as long as it
---runs and gives it back to this script's next opening, in a session or
---on the title. It is for work the original keeps in static data, which a
---screen finds as the last one left it (the save list's name scroll, the
---option screen's rows); it is never saved, and a load doesn't change it.
---The keys are the script's own: another screen's script neither reads
---them nor counts against them. Up to 64 keys a script; a new key past
---that raises an error.
---@param key string
---@param value integer|boolean|nil An integer, a boolean (kept as 1 or 0), or `nil` to forget the key.
function ScreenApi:keep(key, value) end

---A value this script kept with [`api:keep`](#ScreenApi.keep).
---@param key string
---@return integer? value `nil` when nothing is kept under `key`.
function ScreenApi:kept(key) end

---Leave the screen once the current call returns, reporting `action`. With
---no action the screen leaves its default way: back to gameplay from a
---save screen, back to the title from the load screen, the opener's
---fallback from an engine screen. An unknown action, or `new_game`,
---`load_game` or `film` without `arg`, raises an error.
---@param action? ScreenExit
---@param arg? integer `new_game`: the character, clamped to `0..=255`. `load_game`: the slot, clamped to `0..=7`. `film`: RE 2's film id, `0..=255`. `attract`: the number of the reel to play (`demo/<hex>.dat`, `0..=255`), for a title that picks its own; without it the engine plays the pack's reels in turn.
---@param choice? NewGameChoice|{ claire?: boolean, gallery?: boolean } `new_game`: the options the title chose. `film`: `claire` for Claire's copy of the film, and `gallery` for the gallery's playing of it, which any skip button ends and which keeps the frame the title drew as it asked until the film starts.
function ScreenApi:exit(action, arg, choice) end

---Set the full-screen overlay drawn over the screen: black or white at
---`alpha`, or none. A black overlay also sets the
---[`fade`](#ScreenApi.fade) level. An unknown colour raises an error.
---@param colour? "black"|"white" `nil` draws no overlay at all.
---@param alpha? integer `0..=255`, default 0.
function ScreenApi:overlay(colour, alpha) end

---Set the screen's black-overlay opacity, from `0` (clear) to `255`
---(black), and drop any [`overlay`](#ScreenApi.overlay) colour.
---@param alpha integer Clamped to `0..=255`.
function ScreenApi:fade(alpha) end

---Write one `[lua]` line to stderr.
---@param text string
function ScreenApi:log(text) end

-- # Sound

---Queue a screen sound. Cues queue from `open`, `update` and the signals
---and play after the call; a cue raised while drawing is dropped, and a
---capture run stays silent. An unknown name is logged and ignored.
---@param name ScreenSound
function ScreenApi:sound(name) end

---Queue a sound from a numbered global sound bank, such as RE 1's ending
---bank `0x0E`. Like `sound`, the cue plays after the call.
---@param bank integer The bank number.
---@param slot integer The slot in that bank.
function ScreenApi:bank_sound(bank, slot) end

---Queue a sound from RE 2's core bank, the original's
---`Snd_se_on(0x4nn0000)` (the title's cues).
---@param slot integer The core-bank slot.
function ScreenApi:core_sound(slot) end

---Load RE 2's core bank `id` for the later `core_sound` cues, the
---original's `Snd_load_core(id)` (the 4th Survivor's title loads 0x12,
---To-fu's 0x13, and the title's own is 0x10).
---@param id integer The core bank.
function ScreenApi:core_bank(id) end

---Play a line of RE 2's voice bank that no stage owns, the original's
---`Xa_play(1, line)` (the prologue's narration), or stop the line with no
---argument (`Xa_stop`). It plays after the call, on the app's own screens
---only (the title and the screens it opens): an in-game screen's call is
---dropped, and a capture run stays silent. The line stops when the screen
---is replaced.
---@param line? integer The line, or `nil` to stop.
---@param player? integer `0` Leon's bank, `1` Claire's; any other raises an error. Default 0.
function ScreenApi:voice(line, player) end

---Play RE 2's main music `id` from its start, looping (`Snd_bgm_main`, then
---`Snd_bgm_ctr(0x1000000)`), or stop the music with no argument. Like
---[`api:voice`](#ScreenApi.voice) it is the app's own screens', and the
---music ends when the screen is replaced; a title asleep under its load
---screen keeps it.
---@param id? integer The main music's id, or `nil` to stop.
function ScreenApi:music(id) end

---Fade the music [`api:music`](#ScreenApi.music) started out
---(`Snd_bgm_fade_ON`), over about four seconds.
function ScreenApi:music_fade() end

---Queue a current-room bank sound, the original's `play_sfx(2, slot)`:
---slot `slot` of the loaded room's sound-table row, played after the call
---like [`api:sound`](#ScreenApi.sound). It plays only on in-game screens;
---a slot the room leaves empty stays silent.
---@param slot integer
function ScreenApi:room_sound(slot) end

-- # Text and language

---Whether the pack has a game font. Without one, text draws nothing and
---the text widths are 0; the screen keeps running and logs the missing font
---once.
---@return boolean
function ScreenApi:has_font() end

---Whether the session has a text table.
---@return boolean
function ScreenApi:has_text() end

---The session's selected language tag (`en-US`, `ja-JP`, ...).
---@return string? tag `nil` without a text table.
function ScreenApi:language() end

---A `language/<tag>.txt` string, decoded to characters.
---@param key string The string's key.
---@param tag? string A language tag such as `"en-US"`: the string from that language alone, without the fallback chain.
---@return string? text `nil` when no language (or the tagged one) carries the key, or without a text table.
function ScreenApi:string(key, tag) end

---The text table's font-encoded bytes for `key`, ready for
---[`api:glyphs`](#ScreenApi.glyphs).
---@param key string
---@return string? bytes `nil` without a text table.
function ScreenApi:ui_bytes(key) end

---The font-encoded name of an item.
---@param id integer The item id.
---@param examined? boolean With `true`, the menu's name: an unchecked item whose class the session has not examined yet keeps its unchecked name.
---@return string? bytes `nil` without a text table or a name.
function ScreenApi:item_name_bytes(id, examined) end

---The font-encoded shorthand location label of a save's room.
---@param stage integer The 1-based stage, as a save entry reports it.
---@param room integer
---@return string? bytes `nil` without a text table.
function ScreenApi:location_bytes(stage, room) end

---The save screen's name for a character, decoded.
---@param index integer `0` Chris, `1` Jill; only the low bit counts.
---@return string? name `nil` without a text table.
function ScreenApi:character_name(index) end

---The session font's left text margin.
---@return integer margin 0 without a font.
function ScreenApi:font_margin() end

---The session font's fixed glyph advance, the cell the original's text grid
---steps by: 14 for the Japanese sheet, 8 for the Western one. Screens with
---per-release column tables pick theirs by it, as the vanilla save screen
---does.
---@return integer? width `nil` without a font.
function ScreenApi:glyph_width() end

---How far [`api:grid_text`](#ScreenApi.grid_text) advances per cell.
---@return integer width 0 without a font.
function ScreenApi:grid_glyph_width() end

---The pixel width [`api:text`](#ScreenApi.text) draws `text` at.
---@param text string Characters, encoded like `api:text` encodes them.
---@param tight? boolean The metrics to measure with; defaults to the session's.
---@return integer width 0 without a font.
function ScreenApi:text_width(text, tight) end

---How far left of `x` [`api:text`](#ScreenApi.text) draws the cell of
---`text`'s first glyph (its left bearing), so that `x + indent` puts the
---whole cell at `x`.
---@param text string Characters, encoded like `api:text` encodes them.
---@param tight? boolean The metrics to measure with; defaults to the session's.
---@return integer indent 0 without a font or for an empty `text`.
function ScreenApi:text_indent(text, tight) end

---The width Classic REbirth's own text measure gives `text`, which CR
---centres its lines by. It can differ from the drawn width: CR measures a
---kanji from the font's right page as wider than it draws it, and `.` on the
---Japanese sheet as narrower.
---@param text string
---@return integer width 0 without a font.
function ScreenApi:classic_rebirth_width(text) end

---The pixel width [`api:glyphs`](#ScreenApi.glyphs) draws font-encoded
---`bytes` at with the same options (only `tight` matters).
---@param bytes string Font-encoded bytes.
---@param opts? DrawOptions
---@return integer width 0 without a font.
function ScreenApi:glyphs_width(bytes, opts) end

---Break font-encoded `bytes` at spaces into rows no wider than `max_width`
---pixels.
---@param bytes string Font-encoded bytes.
---@param max_width integer
---@param opts? DrawOptions `tight` picks the metrics.
---@return string[] rows The rows, font-encoded; the whole text as one row without a font.
function ScreenApi:wrap(bytes, max_width, opts) end

-- # Saves

---The save directory's files, sorted by name. The list is rescanned when
---the screen opens and after a save or remove.
---@return SaveEntry[]
function ScreenApi:saves() end

---Which of the eight numbered slots hold a save.
---@return boolean[] slots Eight booleans, slot 0 first.
function ScreenApi:save_slots() end

---The eight numbered slots' headers, slot 0 first.
---@return (SlotInfo|false)[] slots `false` for an empty slot.
function ScreenApi:slot_info() end

---The block a save would write.
---@return SlotInfo? info `nil` outside save mode.
function ScreenApi:pending() end

---Write the pending block to `<save_dir>/<name>`, consuming an ink ribbon
---when the opener asked for one. Save mode only. `name` is one plain file
---name with no directory part; the extension is the script's choice. An RE 2
---block is the paused session's state after the typewriter's write tick
---(the ribbon off, the save count up), which the engine applies to the
---session on the same tick; a pending block the opener passed in is not
---used for it.
---@param name string
---@param carry_on? boolean A failed write still counts as a save (the ribbon taken, the count raised), as RE 2's `Card_access` carries on after a failed write. Default false.
---@return boolean ok
---@return string? err Why it failed: an invalid name, not in save mode, or the write error.
function ScreenApi:save(name, carry_on) end

---Write the pending block to numbered slot `slot`, consuming an ink ribbon
---when the opener asked for one. Save mode only.
---@param slot integer `0..=7`.
---@return boolean ok
---@return SlotInfo|string result The written slot's header on success, else why it failed.
function ScreenApi:save_slot(slot) end

---Delete `<save_dir>/<name>`. A missing file is not an error.
---@param name string One plain file name.
---@return boolean ok
---@return string? err Why it failed.
function ScreenApi:remove(name) end

---Load `<save_dir>/<name>`: once this `update` returns, the screen closes
---and the game continues from that save, or, under a title with both
---[`on_resume`](#ScreenModule.on_resume) and
---[`on_loaded`](#ScreenModule.on_loaded), that title wakes to start it.
---Load mode only; the file must be a readable save.
---@param name string One plain file name.
---@return boolean ok
---@return string? err Why it failed.
function ScreenApi:load(name) end

---The local wall-clock time, for naming files.
---@return LocalTime? time `nil` when the platform cannot say.
function ScreenApi:local_time() end

-- # Pack entries

---A pack entry's raw bytes, for data a screen parses itself (the map's
---tables). An entry over 64 MiB raises an error.
---@param entry string The pack path.
---@return string? bytes `nil` when the pack has no such entry.
function ScreenApi:pack_bytes(entry) end

---Whether the pack has an image with that stem, in any image format.
---@param stem string
---@return boolean
function ScreenApi:has_image(stem) end

---A pack image's size, decoding it into the entry cache. A missing or
---invalid image raises an error.
---@param entry string
---@return integer width
---@return integer height
function ScreenApi:image_size(entry) end

-- # Drawing by entry

---*Draw only.* Draw the pack image `entry` over the whole screen, stretched
---over the logical 320x240 quad. A 320x240 full-colour image with default
---options is copied exactly; `colour` tints any background, and
---`brightness`, `clut` and `blend` apply to an indexed one. It ignores the
---clip rect. A missing image raises an error.
---
---An image `entry` resolves like every other asset, by its stem, highest
---priority first `webp > png > bmp > tim > pix`, so a mod's replacement wins
---and a `.tim` keeps its palette (4bpp/8bpp indexed, 16bpp full colour).
---@param entry string
---@param opts? DrawOptions
function ScreenApi:background(entry, opts) end

---*Draw only.* A filled rectangle.
---@param x integer
---@param y integer
---@param w integer
---@param h integer
---@param colour ScreenColour
---@param alpha? integer `0..=255`, default 255; below 255 it blends over what is already there.
function ScreenApi:rect(x, y, w, h, colour, alpha) end

---*Draw only.* Draw `text` through the game font, encoded in the session's
---text encoding, with the vanilla shadow.
---@param x integer
---@param y integer
---@param text string Characters (UTF-8).
---@param opts? DrawOptions `colour`, `brightness` and `tight` apply.
function ScreenApi:text(x, y, text, opts) end

---*Draw only.* Draw the whole pack image `entry` at `(x, y)`.
---@param entry string
---@param x integer
---@param y integer
---@param opts? DrawOptions
function ScreenApi:sprite(entry, x, y, opts) end

---*Draw only.* Draw the `sx, sy, sw, sh` region of the pack image `entry`,
---scaled into the `dx, dy, dw, dh` rectangle.
---@param entry string
---@param sx integer
---@param sy integer
---@param sw integer
---@param sh integer
---@param dx integer
---@param dy integer
---@param dw integer
---@param dh integer
---@param opts? DrawOptions
function ScreenApi:sprite_region(entry, sx, sy, sw, sh, dx, dy, dw, dh, opts) end

---*Draw only.* Draw already font-encoded bytes (from
---[`api:ui_bytes`](#ScreenApi.ui_bytes),
---[`api:location_bytes`](#ScreenApi.location_bytes) or assembled by the
---script): white, brightness 2 and the session's metrics unless `opts` says
---otherwise. `{tight = false}` draws on the fixed grid, and `fit` condenses
---a row wider than that many pixels.
---@param x integer
---@param y integer
---@param bytes string Font-encoded bytes.
---@param opts? DrawOptions
function ScreenApi:glyphs(x, y, bytes, opts) end

---*Draw only.* Draw raw bytes through the original's unshadowed
---`PrintText8x14` grid, in `opts.colour`.
---@param x integer
---@param y integer
---@param bytes string
---@param opts? DrawOptions
function ScreenApi:text_8x14(x, y, bytes, opts) end

---*Draw only.* Draw raw font-sheet cells through the sheet's own grid
---addressing (18 cells a row, 14 pixels high), advancing
---[`api:grid_glyph_width()`](#ScreenApi.grid_glyph_width) per cell, as the
---ending captions do. Nothing draws without a font.
---@param x integer
---@param y integer
---@param scale integer The brightness scale, `0..=255`.
---@param bytes string Sheet cell numbers.
function ScreenApi:grid_text(x, y, scale, bytes) end

-- # Art handles

---Load a full-colour image as a picture handle. Asking twice for the same
---art returns the same handle.
---@param stem string
---@param key? ScreenColour A colour drawn transparent.
---@param geometry? "free"|"shipped" `"free"` (default) lets a replacement be authored at any size; `"shipped"` pins it to the original sheet's size. Anything else raises an error.
---@return integer? handle
---@return string? err Why it could not load (missing art, the cache budget).
function ScreenApi:picture(stem, key, geometry) end

---Load a UI sheet as a page handle, indexed with its CLUT rows when the art
---is, else full colour.
---@param stem string
---@return integer? handle
---@return string? err
function ScreenApi:page(stem) end

---Load a page that is always indexed: a true-colour replacement is
---re-indexed colour for colour. For screens that composite by texel index
---([`api:page_lut`](#ScreenApi.page_lut)).
---@param stem string
---@return integer? handle
---@return string? err
function ScreenApi:indexed_page(stem) end

---The item's 40x30 inventory icon as a picture handle, from its folder's
---`item/<id>/item.bmp`. While the frame is captured, the HD names
---`htexture/item/<id>/item` and `htexture/data/item_all/<row>` win over the
---content hash. Icons do not count against the 64-handle budget.
---@param item integer
---@return integer? handle
---@return string? err When the item has no icon.
function ScreenApi:item_icon(item) end

---Load the item viewer's model for `item`: the item folder's `model.tmd`
---with its `model.bmp` page.
---@param item integer
---@return integer? handle A model handle.
---@return string? err When the item has no model or its file is missing.
function ScreenApi:item_model(item) end

---Load a character's model from the pack entry `entry`, as the model viewer
---loads one: an enemy's `.emd` (RE 2's with the texture page beside it), an
---RE 2 player's `.pld`, or a weapon's `.plw` on its player. For
---[`api:draw_character`](#ScreenApi.draw_character).
---
---`moves` moves parts to other pages of the model's texture, as RE 2's
---`Texture_Move` gives a zombie other clothes: each `{part, page}` puts the
---part's textured polygons on `page` with that page's palette row. Both
---numbers count from 0: `part` is the mesh's object, `page` a 128-texel
---column of the texture from its left, 0 to 15, with a palette row for each
---page up to it. A move the model can't take (no such part, a page past the
---texture's width or 15, or a row the texture hasn't) loads nothing and is
---the error.
---@param entry string A pack entry, such as `enemy/10/enemy_0.emd`.
---@param moves? integer[][] `{part, page}` pairs, in order.
---@return integer? handle A character model handle.
---@return string? err When the entry is missing, is no model, or can't take a move.
function ScreenApi:character_model(entry, moves) end

---RE 2's KEYCONFIG model: the player beside the option screen's key list,
---which the engine runs and the screen calls at the points the original's
---page does. The model's demonstration counters outlive every model the
---screen makes, as the original's do.
---
---- `"make", id, type, disc`: a fresh model of character `id` (0 to 15)
---  holding weapon `type` (a 16-bit word whose low 12 bits are a weapon up
---  to 0x13), from disc `disc`'s (0 or 1) player files or else the other
---  disc's, standing in its place facing the viewer. False when the pack
---  has no such player files, and every later call then does nothing.
---- `"start"`: the model back off to the left with its first step in.
---- `"walk"`: a step of the walk in.
---- `"walk_on"`: the walk's routine, which the page writes every tick of
---  the walk in.
---- `"place"`: the model in its place at once.
---- `"face"`: a step of the turn to face the viewer, then the idle.
---- `"turn_back"`: the next `"row"` turns the model back to face the
---  viewer first.
---- `"row", row`: the list's row `row` (0 to 10) acted out for a tick.
---- `"trace"`: the model as the trace's `kcm` shows it; nil with no model.
---- `"draw"` or `"draw", picture, x, y`: in a draw, over the room's picture
---  drawn already (`picture` and the `x`, `y` it was drawn at with
---  `draw_background`), the model's reflection and shadow in the mirror, a
---  strip of the picture at the mirror's left edge drawn again over the
---  reflection (none without `picture`), and the model in front.
---
---The arguments must be Lua integers, exactly as many as the operation
---takes; anything else raises an error before the model changes.
---@param op string
---@param ... integer
---@return boolean|string|nil
function ScreenApi:option_model(op, ...) end

---Load RE 2's FILE binders model (`ui/status/st_file.ts`), for
---[`api:draw_re2_binders`](#ScreenApi.draw_re2_binders).
---@return integer? handle A model handle.
---@return string? err When the pack has none.
function ScreenApi:re2_binders_model() end

---The number of objects an item model carries. A handle that is not one,
---a character's among them, raises an error.
---@param model integer
---@return integer
function ScreenApi:model_objects(model) end

---A handle's width and height (a model's texture page size). An invalid
---handle raises an error.
---@param handle integer
---@return integer width
---@return integer height
function ScreenApi:art_size(handle) end

---Drop art the screen no longer draws, returning its bytes and count to the
---budget. The handle is invalid afterwards and never handed out again; an
---unknown or already released handle does nothing.
---@param handle integer
function ScreenApi:release(handle) end

---An indexed page's CLUT row, keyed by texel index.
---@param page integer
---@param row? integer The CLUT row, default 0.
---@return table<integer, integer>? palette `0xRRGGBB` colours for indices `0..=255`; `nil` for a full-colour page.
function ScreenApi:page_palette(page, row) end

---The texel indices of an indexed page's CLUT row whose colour carries the
---PlayStation semi-transparency (STP) bit, as a set (`stp[index] == true`).
---@param page integer
---@param row? integer The CLUT row, default 0.
---@return table<integer, boolean>? stp `nil` for a full-colour page.
function ScreenApi:page_stp(page, row) end

-- # Drawing primitives

---*Draw only.* Clear the frame to black.
function ScreenApi:clear() end

---*Draw only.* A filled rectangle from colour channels.
---@param x integer
---@param y integer
---@param w integer
---@param h integer
---@param r integer
---@param g integer
---@param b integer
---@param a? integer Default 255. Each channel is clamped to `0..=255`.
function ScreenApi:fill_rect(x, y, w, h, r, g, b, a) end

---*Draw only.* A one-pixel opaque line as the original's line primitive
---draws it: a quad half a pixel either side of the segment, filled with the
---GPU's top-left rule, so the far end of each axis is excluded (a run from
---10 to 13 paints 10..12, a downward run at x=20 paints column 19), and each
---colour byte goes through the primitive's `byte / 256 * 255` step (0xD8
---draws 0xD7).
---@param x0 integer
---@param y0 integer
---@param x1 integer
---@param y1 integer
---@param r integer
---@param g integer
---@param b integer
function ScreenApi:line(x0, y0, x1, y1, r, g, b) end

---*Draw only.* Clip the rest of this `draw` to a rectangle; with any
---argument missing, lift the clip. The clip ends with the `draw` call.
---Backgrounds ignore it.
---@param x? integer
---@param y? integer
---@param w? integer
---@param h? integer
function ScreenApi:clip(x, y, w, h) end

---*Draw only.* Blend the whole frame towards black by `alpha`/255, the
---screens' black fade rect.
---@param alpha integer `0..=255`.
function ScreenApi:darken(alpha) end

---*Draw only.* RE 2's fade: a grey tile of `level` over the frame, or over
---the rectangle when all four of `x, y, w, h` are given, in a PlayStation
---blend mode.
---@param mode integer `0` average, `1` add, `2` subtract, `3` a quarter added.
---@param level integer The grey level, `0..=255`.
---@param x? integer
---@param y? integer
---@param w? integer
---@param h? integer
function ScreenApi:fade_tile(mode, level, x, y, w, h) end

---*Draw only.* Darken a rectangle by `alpha`/255.
---@param x integer
---@param y integer
---@param w integer
---@param h integer
---@param alpha integer `0..=255`.
function ScreenApi:blend_black_rect(x, y, w, h, alpha) end

-- # Drawing art

---*Draw only.* Draw a picture as a backdrop with its origin at `(x, y)`,
---tinted by `r, g, b`. Like [`api:background`](#ScreenApi.background) it
---ignores the clip.
---@param picture integer A picture handle.
---@param x integer
---@param y integer
---@param r? integer Default 255.
---@param g? integer Default 255.
---@param b? integer Default 255.
function ScreenApi:draw_background(picture, x, y, r, g, b) end

---*Draw only.* A picture region scaled into a rectangle at a brightness
---scale.
---@param picture integer A picture handle.
---@param sx integer
---@param sy integer
---@param sw integer
---@param sh integer
---@param dx integer
---@param dy integer
---@param dw integer
---@param dh integer
---@param scale integer The colour scale, `0..=255`.
---@param cell? integer[] `{x, y, w, h}`: the atlas cell the region lies in, so an HD replacement is looked up for that cell alone rather than the whole picture.
function ScreenApi:picture_sprite_scaled(picture, sx, sy, sw, sh, dx, dy, dw, dh, scale, cell) end

---*Draw only.* A page region through CLUT row `clut`, scaled into a
---rectangle and tinted.
---@param page integer A page handle.
---@param sx integer
---@param sy integer
---@param sw integer
---@param sh integer
---@param dx integer
---@param dy integer
---@param dw integer
---@param dh integer
---@param clut integer The CLUT row (capped at 255).
---@param scale integer The colour scale, `0..=255`.
---@param tint? ScreenTextColour Default white.
---@param blend? "opaque"|"half"|"variant"|"add"|integer Default `"opaque"`. `"variant"` is the original's variant-2 half alpha over the frame; `"half"` averages, `"add"` adds. An integer is an alpha, clamped to `0..=255`: the sprite goes over the frame at that weight, from the page itself (an HD replacement isn't drawn at a weight). The software renderer smooths one drawn at another size than its texels as the original's 640 x 480 frame shows it; the GPU renderer samples it once to a pixel.
function ScreenApi:page_sprite_scaled(page, sx, sy, sw, sh, dx, dy, dw, dh, clut, scale, tint, blend) end

---*Draw only.* A page region through CLUT row `clut` as RE 2's PC version
---draws a sprite. Each channel of an indexed page's texel is multiplied by
---twice the colour's (capped at 255) over 255 and rounded, so `0x80` is the
---texel as it is, less dims it and more does nothing. `widen` names how that version
---made the page's 15-bit colours 8-bit, which differs from sheet to sheet:
---`"bits"` repeats a level's top bits below it (level 20 is 165) and
---`"seven"` is eight times the level plus 7, with 0 left at 0 (level 10 is
---87); without it the colours are the pack's (level 20 is 164). A colour
---with a channel that isn't a 5-bit level's (a page of a mod's own) is kept
---whole either way. Index 0 is clear, and so is black without the
---PlayStation's semi-transparency bit; an empty rect, or a row the page
---hasn't, draws nothing. An HD replacement of the row is drawn in its own
---colours under `rgb` as a tint; a row with none takes the replacement of
---the page's first row, recoloured by the page's indices, so one made from
---the first row serves them all. A page that isn't indexed has no rows or
---levels: `clut` and `widen` don't apply, and it is drawn under `rgb` as the
---engine's tint (twice the colour over 255, not rounded).
---@param page integer A page handle.
---@param sx integer
---@param sy integer
---@param sw integer
---@param sh integer
---@param dx integer
---@param dy integer
---@param dw integer
---@param dh integer
---@param clut integer The CLUT row (capped at 255).
---@param rgb? integer[] `{r, g, b}`, each held to `0..=255`. Default `{0x80, 0x80, 0x80}`.
---@param widen? "bits"|"seven"
---@param blend? "opaque"|"half"|"variant"|"add"|integer As [`api:page_sprite_scaled`](#ScreenApi.page_sprite_scaled) takes it, but that `"half"` is this version's: the sprite at 128 of 255 over the frame, rounded, with an HD replacement drawn at that weight too. An integer alpha draws from the page itself, as there.
function ScreenApi:page_sprite_modulated(page, sx, sy, sw, sh, dx, dy, dw, dh, clut, rgb, widen, blend) end

---@class ScreenEffectCamera
---@field from integer[] `{x, y, z}` eye position.
---@field to integer[] `{x, y, z}` look-at point.
---@field fov integer The projection distance.

---Give the screen its own effect pool, spawning from the weapon effect
---sheets (the screen has no room of its own), viewed through `camera`. `zone`
---is the camera zone, as four `{x, z}` corners, where the effects may draw.
---Calling it again replaces the pool.
---@param camera ScreenEffectCamera
---@param zone integer[][]
function ScreenApi:effect_scene(camera, zone) end

---Spawn effect `type` with depth group `group` at `{x, y, z}` in the
---screen's pool (`effect_scene` first).
---@param type integer
---@param group integer
---@param x integer
---@param y integer
---@param z integer
---@return integer? slot The first slot taken, or nil when nothing spawned.
function ScreenApi:spawn_effect(type, group, x, y, z) end

---Remove every effect of `type` in depth group `group` from the screen's pool.
---@param type integer
---@param group integer
function ScreenApi:kill_effects(type, group) end

---Run one tick of the screen's effects. Call it once per `update`.
function ScreenApi:update_effects() end

---*Draw only.* Draw the screen's effects as they stood before the latest
---`update_effects`, one frame behind the way the original draws them.
function ScreenApi:draw_effects() end

---*Draw only.* A page region flipped per axis. With `whole`, an HD
---replacement covers the region's transparent texels too and its own alpha
---decides what shows, for art the HD page redraws rather than upscales (the
---ECG condition label): cut to the original's drawn texels, it would come
---out in pieces.
---@param page integer A page handle.
---@param sx integer
---@param sy integer
---@param sw integer
---@param sh integer
---@param dx integer
---@param dy integer
---@param dw integer
---@param dh integer
---@param clut integer The CLUT row (capped at 255).
---@param brightness integer `0..=255`.
---@param mirror_u boolean Flip horizontally.
---@param mirror_v boolean Flip vertically.
---@param whole? boolean
function ScreenApi:page_sprite_mirrored(page, sx, sy, sw, sh, dx, dy, dw, dh, clut, brightness, mirror_u, mirror_v, whole) end

---*Draw only.* A rectangle of an indexed page drawn through `lut`, a table
---of `0xRRGGBB` colours keyed by texel index; an index without an entry,
---index 0 included, is transparent. The screens composite remapped palettes
---with it (the map's tinted and room-highlighted plans). A page that is not
---indexed raises an error.
---@param page integer An indexed page handle.
---@param sx integer
---@param sy integer
---@param w integer
---@param h integer
---@param dx integer
---@param dy integer
---@param lut table<integer, integer>
---@param whole? boolean With `true`, a lookup that tints every index alike lets an HD replacement cover the whole rectangle, for art the HD page redraws. It does nothing with an alpha for `options.blend`, which never draws a replacement.
---@param options? LutOptions
function ScreenApi:page_lut(page, sx, sy, w, h, dx, dy, lut, whole, options) end

---*Draw only.* An unscaled page region modulated by `2 * ramp` and
---half-blended over the frame (the FILE reader's backdrop pieces).
---@param page integer A page handle.
---@param sx integer
---@param sy integer
---@param w integer
---@param h integer
---@param dx integer
---@param dy integer
---@param ramp integer `0..=255`.
function ScreenApi:page_ramp(page, sx, sy, w, h, dx, dy, ramp) end

---*Draw only.* Draw a model. Object `i` takes the joint chain
---`spec.objects[i]`, and every object past that list the one `spec.joints`
---chain. While the GPU renderer draws the frame, the model renders on the
---GPU, cut to the clip rect.
---@param model integer A model handle.
---@param spec IvmSpec
function ScreenApi:draw_ivm(model, spec) end

---*Draw only.* Draw a character's model standing at the origin, in its
---textures' own colours with no lights, as RE 2's gallery shows its models.
---Each of the mesh's objects takes its joint of the skeleton; the objects
---past the skeleton's joints draw only where `spec.parts` places them.
---While the GPU renderer draws the frame, the model renders on the GPU, cut
---to the clip rect.
---@param model integer A character model handle.
---@param spec CharacterSpec
---@return boolean drew `false` when the model has no such pose.
function ScreenApi:draw_character(model, spec) end

---*Draw only.* Draw an engine-owned actor through the screen's camera. The
---DIED screen offers `"player"`, the posed corpse.
---@param name string
---@param camera ScreenCamera `screen` is ignored.
---@return boolean drew `false` when the engine has no such actor.
function ScreenApi:draw_actor(name, camera) end

---*Draw only.* Draw RE 2's three FILE binders and their labels as the original does: the zoom's eye, the pull, the stack, the spin and a file's drop.
---@param model integer The handle from [`api:re2_binders_model`](#ScreenApi.re2_binders_model).
---@param page Re2Items The page whose binders these are: its `binder`, `slot`, `sub`, `command`, `zoom`, `view_zoom`, `spin` and `files`.
---@param top integer The panel's top row, which the projection follows.
function ScreenApi:draw_re2_binders(model, page, top) end

-- # Reading the game

---One bit of the paused session's flag banks.
---@param bank integer
---@param bit integer
---@return boolean set `false` outside a session.
function ScreenApi:flag(bank, bit) end

---Where the paused session stands; four `nil`s outside a session. A
---generated room reports its origin's stage and room.
---@return integer? stage
---@return integer? room
---@return integer? cut The camera cut.
---@return integer? player `0` Chris, `1` Jill.
function ScreenApi:room() end

---Whether the game's message window is up.
---@return boolean
function ScreenApi:message_active() end

---An inventory slot's item.
---@param slot integer The 0-based slot.
---@return integer id `0` past the held run or outside a session.
---@return integer quantity
function ScreenApi:inventory(slot) end

---The current character's number of inventory slots.
---@return integer slots Chris's six outside a session.
function ScreenApi:inventory_capacity() end

---An item-box slot's item.
---@param slot integer The 0-based slot.
---@return integer id `0` for an out-of-range slot or outside a session.
---@return integer quantity
function ScreenApi:item_box(slot) end

---The player's health.
---@return integer health
---@return integer max
---@return integer status The status byte; the poison bits are `0x22`. All three are 0 outside a session.
function ScreenApi:player_health() end

---The character byte's low two bits, which pick the portrait: `0` Chris,
---`1` Jill, `2`/`3` the alternates. In an RE 2 game bit 0 is Claire and
---bit 1 the scenario B.
---@return integer
function ScreenApi:character() end

---Whether the radio tab is live.
---@return boolean
function ScreenApi:has_radio() end

---Whether the run was started with `--authentic` (the original's item
---rules). Without a session, the run's setting answers.
---@return boolean
function ScreenApi:authentic() end

---The equipped item.
---@return integer item `0` for none.
function ScreenApi:equipped() end

---How many of an item the inventory holds in total.
---@param item integer
---@return integer
function ScreenApi:item_count(item) end

-- # Examining

---The item viewer's turntable pose after pre-multiplying a spin,
---re-extracted the reference way.
---@param pitch integer
---@param yaw integer
---@param roll integer
---@param sx integer The spin about x.
---@param sy integer The spin about y.
---@param sz integer The spin about z.
---@return integer pitch
---@return integer yaw
---@return integer roll
function ScreenApi:examine_spin(pitch, yaw, roll, sx, sy, sz) end

---The examine check for an item held in a pose.
---@param item integer
---@param pitch integer
---@param yaw integer
---@param roll integer
---@param zoom_spin integer
---@param quantity integer The slot's quantity.
---@return "description"|"zoom"|"message" kind `"description"`: show the plain description; `"zoom"`: a book whose pose matched, run the zoom spin; `"message"`: show a global message.
---@return integer? id The global message id, for `"message"`.
---@return boolean mark Whether the message marks the item's class examined.
---@return integer? select The item the message's `\i` names.
function ScreenApi:examine_check(item, pitch, yaw, roll, zoom_spin, quantity) end

---The global message an item's examine class shows.
---@param item integer
---@return integer? id `nil` when the class has none.
function ScreenApi:class_message(item) end

-- # Changing the game (lent)

---*Lent.* The next draw of the game's shared `rand()` stream, the one the
---room scripts use.
---@return integer value `0..=0x7FFF`.
function ScreenApi:rand() end

---*Lent.* Mirror the item under the cursor into the state byte the room
---scripts test.
---@param item? integer `0` or `nil` for none.
function ScreenApi:select_item(item) end

---*Lent.* Equip an item, or unequip.
---@param item? integer `0` or `nil` unequips.
function ScreenApi:equip(item) end

---*Lent.* USE the item in an inventory slot. On success the item is
---recorded as used and one is consumed (a stack picked up with quantity 0,
---such as a crank, never is), and it returns `true, healed, cured, closes`.
---A refusal returns `false, category`; an empty slot returns nothing.
---Outside authentic mode the full-health heal items, and a blue herb while
---the player is not poisoned, are consumed with no effect and return
---`true, false, false`; authentic mode refuses them.
---@param slot integer
---@return boolean? ok
---@return boolean|integer? healed On success, whether it healed. On a refusal, the use category the refusal message adds to `0xF7`.
---@return boolean? cured Whether it cured poison.
---@return boolean? closes Whether the use (a key, special, chemical or book) exits the whole menu.
function ScreenApi:use_item(slot) end

---*Lent.* MOVE the item in `slot` onto `target`, applying any recipe.
---@param slot integer
---@param target integer
---@return "applied"|"drug_store"|"herb_no_recipe"|"none" result `"drug_store"`: chemicals need the drug store; `"none"`: no recipe, or `target` past the held items.
---@return boolean herb With `"applied"`, whether it was a herb mix.
function ScreenApi:combine(slot, target) end

---*Lent.* The original's `menu_item_check_combine`. An ordinary recipe is
---applied at once; a valid herb recipe is held back for the `0xF5` yes/no
---prompt.
---@param slot integer
---@param target integer
---@return "applied"|"herb"|"drug_store"|"herb_no_recipe"|"none"
function ScreenApi:combine_check(slot, target) end

---*Lent.* Open the game's message window on a global message; nothing
---happens while one is already up.
---@param id integer
function ScreenApi:show_message(id) end

---*Lent.* The game's `g_menu_choice_id` byte: bit `0x80` is set while a
---message owns the frame, and bit 0 is the yes/no answer once confirmed
---(0 Yes, 1 No), the byte the original's scripts read.
---@return integer
function ScreenApi:message_choice() end

-- # RE 2 status screens

---RE 2's status screen while it is up. The engine runs it; the script only
---draws it.
---@return Re2Items? page `nil` when it is not up.
function ScreenApi:re2_items() end

---RE 2's get-item window while it is up.
---@return Re2ItemGet? window `nil` when it is not up.
function ScreenApi:re2_item_get() end

---RE 2's status screen portrait: its three rows' texels on the portrait sheet
---(the `st<n>_jp` sheet's TIM 1) for the character the player is.
---@return { u: integer, v: integer }[] rows top to bottom.
function ScreenApi:re2_portrait() end

---RE 2's item box page while its screen is up.
---@return Re2Box? page `nil` otherwise.
function ScreenApi:re2_box() end

---One of RE 2's item box slots.
---@param slot integer Taken modulo the box's 64 slots.
---@return integer id
---@return integer count
---@return integer mark
function ScreenApi:re2_box_slot(slot) end

---What RE 2's MAP page draws on the floor it shows. While the page isn't
---up that is the floor last chosen, on the page or by a script's map
---(`Sce_84`), which share it: 0 before either, and not the player's.
---@return Re2Map? map `nil` without a game.
function ScreenApi:re2_map() end

---What RE 2's status screen draws in its map mode: a script's map
---(`Sce_84`), which is the floor's rooms with the target's fill blinking and
---no marker, or the map key's, which is the MAP page's.
---@return (Re2Map|Re2MapBackdrop)? map `nil` while no such map is up, or on a tick that draws nothing.
function ScreenApi:re2_script_map() end

---What room 200's computer draws this tick (`Computer200`).
---@return Re2Computer? view `nil` while no computer runs.
function ScreenApi:re2_computer() end

---Whether RE 2's item's count shows the infinity sign by the extras word
---(the status screen's Konami code): weapons 1 to 0x13 only.
---@param item integer
---@return boolean
function ScreenApi:re2_infinite(item) end

---What RE 2's results after a cleared game draw this tick (`Result`).
---@return Re2Result? view `nil` while no results run.
function ScreenApi:re2_result() end

---The id and count of RE 2's `Item_work` slot.
---@param slot integer `10` is the personal item.
---@return integer id
---@return integer count
function ScreenApi:re2_slot(slot) end

---The tick's input, passed to `update`. The edge fields are true on the
---tick a button was pressed. `pad` and `held` are the original's pad words,
---with its bit layout: confirm `0x0001`, cancel `0x0002`, start `0x0008`,
---up `0x0010`, down `0x0020`, left `0x0040`, right `0x0080`, L1 `0x0400`,
---R1 `0x0800`.
---@class ScreenInput
---@field up boolean
---@field down boolean
---@field left boolean
---@field right boolean
---@field confirm boolean
---@field cancel boolean
---@field page_left boolean L1.
---@field page_right boolean R1.
---@field start boolean
---@field map boolean The map key.
---@field any boolean Any button was pressed.
---@field return_title boolean The return-to-title key (F9).
---@field pad integer The edge word of the fields above.
---@field held integer The held confirm, directions and L1/R1.
---@field keys? string[] The keyboard keys that went down on the tick, by their `device:name` (`key:Q`), whatever they are bound to, in the order they went down (up to eight with the buttons); reserved keys (F1, F9) and mouse buttons aren't listed.
---@field buttons? string[] The gamepad buttons and axes that went down on the tick (`pad:a`, `axis:lefttrigger+`), whatever they are bound to, in the order they went down; the pad's start isn't listed.
local ScreenInput = {}

---The options table the entry-drawn methods take. An unknown colour or
---blend raises an error.
---@class DrawOptions
---@field colour? ScreenTextColour Default white.
---@field row? integer For text: one of the text's colour rows, drawn in the font's own colour for it in place of `colour` when the font has one (RE 2's font has rows 1 to 4: green, red, grey and blue); over any other font `colour` applies.
---@field brightness? integer The font's brightness scale, `0..=30`, default 2.
---@field tight? boolean Classic REbirth's proportional widths. The Western sheet always uses them; the Japanese sheet uses CR's Japanese-mode table with `tight` and the original's full 14 px cells without it. Defaults to the session's metrics (fixed cells in a Japanese session).
---@field clut? integer An indexed image's CLUT row, default 0.
---@field blend? "opaque"|"half"|"half_variant"|"variant" The sprite blend mode, default `"opaque"`; `"variant"` is `"half_variant"`. A blended image draws the original art, never an HD replacement.
---@field fit? integer `api:glyphs` only: condense a row wider than this many pixels.
local DrawOptions = {}

---The options of [`api:page_lut`](#ScreenApi.page_lut).
---@class LutOptions
---@field dw? integer Stretch the rectangle over a destination this wide (nearest sampling, but see `blend`'s alpha).
---@field dh? integer Stretch it over a destination this high.
---@field blend? "opaque"|"half"|"variant"|"add"|integer `"opaque"` (default), `"half"` averages with the frame, `"variant"` is the original's variant-2 half alpha, `"add"` adds to it. An integer is an alpha, clamped to `0..=255`, as [`api:page_sprite_scaled`](#ScreenApi.page_sprite_scaled) takes it: the rectangle goes over the frame at that weight, from the page itself, and the software renderer smooths one stretched to another size as the original's 640 x 480 frame shows it.
local LutOptions = {}

---A new game's options, for `api:exit("new_game", character, choice)`. Each
---flag is set when `true` or a non-zero number; anything else but `nil` raises
---an error.
---@class NewGameChoice
---@field easy? boolean|integer
---@field hard_mode? boolean|integer
---@field censorship_off? boolean|integer
---@field extra_player? integer RE 2's extra modes (`Title_game_init`'s extras branch): `0xC` starts the 4th Survivor's game (HUNK) and `0xD` To-fu's (TOFU), in room 304 with no prologue, on the disc `character` picks. `nil` or `0` is the story's new game; any other value raises an error.
local NewGameChoice = {}

---One file of the save directory.
---@class SaveEntry
---@field path string The full path.
---@field name string The file name (`savedat1.dat`).
---@field base string The name without its extension (`savedat1`).
---@field loadable boolean Whether the file is a save the engine can load.
---@field timestamp number Unix seconds, with the filesystem's sub-second part.
---@field autosave? integer The autosave rotation index, `1` newest, for `autosave1.dat`/`autosave2.dat`.
---@field character? integer A loadable file's character: `0` Chris, `1` Jill.
---@field character_name? string The character's name, with a text table.
---@field saves? integer The save count.
---@field stage? integer The 1-based stage of the room the block was saved in.
---@field room? integer
---@field mansion2? boolean Whether the room was on its second mansion visit.
---@field re2? boolean Whether a loadable file is an RE 2 session's block (it carries the RE 2 state).
---@field location? string The resolved room label, with a text table.
local SaveEntry = {}

---A save block's header.
---@class SlotInfo
---@field character integer `0` Chris, `1` Jill.
---@field saves integer The save count.
---@field stage integer The 1-based stage.
---@field room integer
---@field mansion2 boolean Whether the room was on its second mansion visit.
---@field re2 boolean Whether the block is an RE 2 session's (it carries the RE 2 state).
local SlotInfo = {}

---The local wall-clock time.
---@class LocalTime
---@field year integer
---@field month integer
---@field day integer
---@field hour integer
---@field minute integer
---@field second integer
---@field millisecond integer
local LocalTime = {}

---A screen camera.
---@class ScreenCamera
---@field from integer[] `{x, y, z}`, the eye.
---@field to integer[] `{x, y, z}`, the point looked at.
---@field fov? integer The projection value, as a room camera's; 0 when absent.
---@field screen? integer[] `{x, y}`: offsets the projection centre from the layout's centre.
local ScreenCamera = {}

---One joint of a chain, in the engine's 12-bit angles. Absent fields are 0.
---@class ScreenJoint
---@field rx? integer
---@field ry? integer
---@field rz? integer
---@field tx? integer
---@field ty? integer
---@field tz? integer
local ScreenJoint = {}

---What [`api:draw_ivm`](#ScreenApi.draw_ivm) draws.
---@class IvmSpec
---@field joints? ScreenJoint[] The joint chain, root first, composed into one matrix; the identity when absent.
---@field objects? ScreenJoint[][] One chain per object, in object order.
---@field camera ScreenCamera
---@field light? ScreenLight Unlit when absent.
local IvmSpec = {}

---What [`api:draw_character`](#ScreenApi.draw_character) draws.
---@class CharacterSpec
---@field pose? CharacterPose A frame of one of the model's animations. When absent the model is as its file has it, every joint unturned.
---@field turn? integer[] `{x, y, z}`: the character's own rotation, in 12-bit angles.
---@field view CharacterView
---@field parts? integer[][] The parts past the skeleton's joints, in order, each `{parent, x, y, z}`: unturned at that place on part `parent`, numbered from 0. An enemy whose routine hangs parts on its skeleton needs them given here.
local CharacterSpec = {}

---A frame of a character's animation.
---@class CharacterPose
---@field bank? integer 0, the default, for the model's own animations; `n` for its `n`th further bank (an RE 2 enemy has up to two). This, `clip` and `frame` are Lua integers: a float raises an error, `1.0` as much as `0.5`, and a negative one is a frame the model lacks.
---@field clip? integer The clip, from 0.
---@field frame? integer The clip's frame, from 0.
---@field height? integer Scales the root's height, in 4.12 (`0x1000` is as stored).
---@field turn? table<integer, integer[]> Angles added to the frame's, `{x, y, z}` keyed by joint, numbered from 0. A joint the model lacks is skipped; a key that is not an integer raises an error.
local CharacterPose = {}

---A character's view: from a point towards another as RE 2's `Set_view`
---builds it (`from` and `to`), or straight down Z with the world's origin at
---`at` before the eye.
---@class CharacterView
---@field from? integer[] `{x, y, z}`, the eye.
---@field to? integer[] `{x, y, z}`, the point looked at.
---@field at? integer[] `{x, y, z}`: where the origin is in the view. With it, `from` and `to` are not read.
---@field fov integer The distance to the screen, in the pixels of the 320x240 layout: 2 or more, or the draw raises an error.
---@field screen? integer[] `{x, y}`: moves the drawn model by that many pixels from the layout's centre, each clamped to `-4096..=4096`. The view is not tilted for it.
local CharacterView = {}

---A model light, built as the original builds it.
---@class ScreenLight
---@field ambient? integer The 12-bit ambient level.
---@field sources? ScreenLightSource[] Up to three sources; missing ones are dark.
local ScreenLight = {}

---One directional light source.
---@class ScreenLightSource
---@field pos integer[] `{x, y, z}`: a direction, normalised.
---@field color integer[] `{r, g, b}`, each capped at the `0x80` full scale.
local ScreenLightSource = {}

---RE 2's status screen state, from [`api:re2_items`](#ScreenApi.re2_items).
---@class Re2Items
---@field page integer
---@field sub integer
---@field command integer
---@field step integer
---@field count integer
---@field cursor integer
---@field target integer
---@field tab integer
---@field command_cursor integer
---@field result integer COMBN's result id.
---@field follow boolean
---@field binder integer FILE's binder, 0 to 2.
---@field slot integer FILE's slot in the binder, 0 to 7.
---@field page_index integer The page of the file being read.
---@field zoom integer The binders' zoom.
---@field view_zoom integer The zoom the binders' last view was built with (the eye lags the zoom by a tick).
---@field spin integer The binders' spin about y, from their rest.
---@field file_x integer The page's x.
---@field files integer[] The 24 files held (`File_tag_bk`), 0xFF for none.
---@field blink integer The screen's shared blink level.
---@field blink_falling boolean The blink's way.
---@field panel_mode integer The condition panel: 0 idle, 1 a heal, 2 a cure, 3 both.
---@field panel_sweep integer
---@field panel_cure integer
---@field panel_done boolean
---@field poisoned boolean Whether the player is poisoned.
---@field partner_dx? integer While a partner follows the player (`fg_status`'s `0x10000000`): her panel's x offset from its place below the portrait (`0x691FA0`, 0 or less).
---@field file integer The file in FILE's binder and slot, 0xFF for none.
---@field shown_page integer The page whose picture the reader's page texture holds: a turn changes it on its slide out's last tick.
---@field file_page? Re2FilePageSprite[] In the reader's subs 0xD to 0x12: the page's sprites in paint order, by the sub the tick left.
local Re2Items = {}

---One sprite of a file page's draw, at 320 x 240.
---@class Re2FilePageSprite
---@field chunk? integer The file's chunk (`ui/status/file<nn>_<k>`): 1 the picture, `2 + page` a page.
---@field sheet? integer The status sheet (`st<n>_jp_<k>`), counted from texture 0x12.
---@field u integer
---@field v integer
---@field w integer
---@field h integer
---@field x integer
---@field y integer
---@field colour integer `0x80` draws the texels as they are; less dims them by `colour / 0x80`.
local Re2FilePageSprite = {}

---RE 2's get-item window, from [`api:re2_item_get`](#ScreenApi.re2_item_get).
---@class Re2ItemGet
---@field item integer
---@field poisoned boolean
---@field fly? { x: integer, y: integer, w: integer, h: integer } On the ticks the picture flies to its slot: its rectangle, from the window's origin.
---@field phase "open"|"ask"|"wait"|"fly"|"refuse"|"shown"|"close"|"file_fade"|"file_read"
---@field sub? integer A file's `St_get_file` sub, once it is filed.
---@field file? integer A file's number, once it is filed.
---@field file_page? Re2FilePageSprite[] In a file's phases, on the ticks the page draws: its sprites in paint order.
local Re2ItemGet = {}

---RE 2's item box page, from [`api:re2_box`](#ScreenApi.re2_box).
---@class Re2Box
---@field routine integer
---@field sub integer
---@field count integer
---@field cursor integer
---@field top integer
---@field scroll integer
---@field pulse integer
---@field started boolean Whether `St_init01` has started the page.
---@field arrows integer The arrows shown pressed this tick: bit 0 up, bit 1 down.
local Re2Box = {}

---Room 200's computer's view, from [`api:re2_computer`](#ScreenApi.re2_computer).
---@class Re2Computer
---@field frame? integer[] `{x0, y0, x1, y1}`: the rectangle `Wframe_set` drew this tick, edges inclusive.
---@field drawn boolean `Console_trans` and `Sprite_Trans` drew: the black tile, the glyphs and the panel.
---@field cells integer[][] The non-space glyphs, each `{x, y, u, v}` on the console page, in record order.
---@field cursor? integer[] `{x, y}`: the cursor's place, when it shows.
local Re2Computer = {}

---RE 2's results' view, from [`api:re2_result`](#ScreenApi.re2_result).
---@class Re2Result
---@field pieces { flags: integer, colour: integer }[] The executable's 33 pieces (`pieces[n + 1]` is piece `n`): bit 0 of `flags` shows it, 4 and 8 fade it in and out; `colour` is `0xRRGGBB`, `0x808080` the art's own.
---@field drawn boolean `Result_draw_quads` drew the pieces this tick.
---@field picture? integer The picture last loaded: `n` of `Result_load_bg`'s `ui/result/end1<n>`, or 9 for HARD's `sp_e`.
---@field picture_name? string That picture's name under `ui/result/`: `end1<n>` or `sp_e`.
---@field background boolean The background shows the picture (`Bg_set_mode(0, 0)`) rather than black.
---@field digits integer[] The clear time's ten glyphs, `HH:MM:SS` and the saves.
---@field fade? { level: integer, mode: integer } Fade slot 0 as `System_trans` drew it.
local Re2Result = {}

---RE 2's MAP page view, from [`api:re2_map`](#ScreenApi.re2_map).
---@class Re2Map
---@field floor integer
---@field map integer The map picture's number.
---@field rooms Re2MapRoom[] In draw order.
---@field marker? { x: integer, y: integer, u: integer, v: integer } The player's marker, when the player is on the floor.
---@field arrows { x: integer, y: integer, u: integer }[] The arrows to the floors below and above, in the order the original adds them (it draws the last first).
---@field doors { x: integer, y: integer, u: integer }[] The locked doors' icons, in the order the original adds them.
---@field blink integer The screen's blink level, which colours the marker.
---@field blink_falling boolean The way of the screen's blink, which colours the arrows and the icons.
local Re2Map = {}

---What [`api:re2_script_map`](#ScreenApi.re2_script_map) returns on a tick
---that draws the backdrop with no map (the map key's floor change): a table
---with no field.
---@class Re2MapBackdrop
local Re2MapBackdrop = {}

---One room of the MAP page.
---@class Re2MapRoom
---@field x integer
---@field y integer
---@field w integer
---@field h integer
---@field outline Re2MapSprite
---@field fill? Re2MapSprite A translucent fill.
local Re2MapRoom = {}

---A MAP page sprite.
---@class Re2MapSprite
---@field texture integer The texture slot.
---@field u integer
---@field v integer
---@field clut integer
local Re2MapSprite = {}

---The arguments the opener passes to `open`: keys of integers, booleans or
---strings, such as the pause menu's `mode` (`"box"`, `"get"`).
---@alias ScreenArgs table<string, integer|boolean|string>

---A drawing colour: `0xRRGGBB`, `"#RRGGBB"` or a tint name (`white`,
---`green`, `red`, `grey`/`gray`, `yellow`).
---@alias ScreenColour integer|string

---A text or sprite colour: a tint name (`white`, `green`, `red`,
---`grey`/`gray`, `yellow`), a CLUT row number, or `"#RRGGBB"`. A text
---`"#RRGGBB"` is a per-channel multiplier over the font sheet's grey ramp,
---so `"#1E64FF"` draws the sheet's white as a blue, and a name is its
---colour the same way. A row number is one of the text's colour rows, as a
---message's colour code is: white for 0, and for the others the font's own
---colour for the row when it has one, else green for 1 and 8, red for 2,
---grey for 3 and yellow for any other.
---@alias ScreenTextColour string|integer

---A screen sound name. `error` is the cancel cue.
---@alias ScreenSound "cursor"|"decide"|"cancel"|"error"|"title"|"select_cursor"|"select_confirm"|"save_cursor"|"save_confirm"|"page_turn"|"map_zoom"|"map_label"

---An [`api:exit`](#ScreenApi.exit) action: where the game goes next.
---`new_game` takes the character, `load_game` the slot. `continue` is a
---title's, after its [`on_loaded`](#ScreenModule.on_loaded): the game
---continues from the save the load screen chose, as after a prologue the
---title ran (RE 2's game starts three ticks after it) when its argument is
---1. `film` is a title's with
---[`on_film`](#ScreenModule.on_film): it takes one of RE 2's film ids,
---sleeps while the film plays and wakes after it. Without `on_film` it
---raises an error, and from a screen that isn't the title it is dropped.
---@alias ScreenExit "title"|"char_select"|"save_load"|"continue"|"attract"|"resume"|"quit"|"ending_result"|"ending_epilogue"|"ending_save"|"new_game"|"load_game"|"film"
