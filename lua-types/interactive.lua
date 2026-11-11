---@meta arklay.interactive

-- RE 1 interactive screens: the `interactive/*.lua` scripts of the screens
-- the game loop keeps running underneath (the guardhouse panel, the lab
-- computer).
--
-- This file is the source of the "Interactive screens" page of the
-- documentation, and a LuaLS definition file for linting and completion in
-- an editor. `arklay mod lua-types --out DIR` writes every definition file
-- the engine ships; add DIR to your editor's `workspace.library`.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.

---The original has three screens that draw over the live room instead of
---freezing it: the passcode panel (room 4080), the lab computer (room 5060)
---and the slide projector (room 4070). While scenario flag `0x20` is up, the
---engine's dispatcher calls the one a system flag selects once a fixed tick,
---right after the room's scripts and before the camera switch and the entity
---update, as the original's `check_and_display_interactive_screen` does:
---
---| System flag | Pack entry |
---|---|---|
---| `0x1D` | `interactive/passcode_panel.lua` |
---| `0x1E` | `interactive/computer_lab.lua` |
---| `0x1F` | `interactive/slides.lua` |
---
---The first set flag wins; a pack without its script shows nothing. RE 1's
---pack carries `interactive/passcode_panel.lua` (room 408's numeric panel)
---and `interactive/computer_lab.lua`. The scripts run in the enemy
---scripts' VM, with the same sandbox, instruction budget and pack-scoped
---`require`.
---
---```lua
----- interactive/computer_lab.lua
---function display(api, s)
---    if s.desc == nil then
---        init_statics(s)            -- the first visit
---    end
---    if s.func == 0 then
---        init(s, api)
---    elseif s.func == 1 then
---        update(s, api)
---    end
---    api:effect_sprite(s.desc, 10, 0x17, 2)
---end
---```
---
---## State
---
---A screen keeps no state in the VM, which the engine may drop between any
---two calls. Its state is the table `s`: the engine keeps it between calls
---(and across visits) as part of the game state, so it may hold only
---integers, numbers, strings, booleans and nested tables of them, keyed by
---integers or strings. A table may appear in it only once: a table shared
---between two fields, or a cycle, is an error, since the next call would get
---two copies. Its fields `func`, `sub`, `sub2` and `sub3` are the original's
---shared state block: the dispatcher sets all four to `0` on the frame the
---gate rises, and `func` to `0` when a room loads.
---
---## Errors
---
---A screen whose `display` raises an error, runs out of its instruction
---budget or leaves `s` holding something it cannot keep is turned off until
---the game reloads its scripts, and the error is logged. The engine then
---backs out of the screen so the game does not hang with the gate up: it
---restores the camera, the room's lights, the message flags, the player's
---height and the enemies' command bytes as they were when the gate rose,
---frees the screen's textures and lowers scenario flag `0x20`. `s` keeps the
---value of the last call that finished. A selected screen with no script is
---backed out of the same way.
---
---## Integers
---
---Values are Lua's 64-bit integers. The API narrows each argument to its C
---field's width on the way in (a sprite's `screen_x` is 16-bit, its `tex_u`
---8-bit), so a script only has to wrap values it reads back itself. The
---sandbox has no `math` library; [`api:f32`](#ScreenApi.f32),
---[`api:sinf`](#ScreenApi.sinf) and [`api:cosf`](#ScreenApi.cosf) give the
---original's single-precision maths.

-- # The screen function

---One frame of the screen. Draws go through `api`; everything the screen
---remembers lives in `s`.
---@param api ScreenApi
---@param s table The screen's state table.
function display(api, s) end

-- # The API

-- source: src/interactive.rs#ScreenApi
---The live game state behind a scoped userdata, passed to `display`. Call
---its methods with Lua colon syntax (`api:rand()`).
---@class ScreenApi
---@field pad_pressed integer The remapped pad's press edges this frame (`g_PlayerDpadPressed >> 8`): `0x40` confirm, `0x80` cancel. Cancel is the raw pad bit run shares, so the cancel and run keys both raise it.
---@field pad_held integer The remapped pad's held buttons (`g_PlayerDpadHeld >> 8`), same bits.
---@field pad_edge integer The raw pad's press edges, high byte (`g_PlayerPadHeld >> 8`): `0x10` up, `0x20` right, `0x40` down, `0x80` left.
---@field raw_held integer The raw D-pad held (`g_button_pressed_id & 0xF000`): `0x1000` up, `0x2000` right, `0x4000` down, `0x8000` left.
---@field raw_edge integer The raw D-pad's press edges (`g_PlayerPadHeld & 0xF000`), same bits.
---@field player_character integer `0` Chris, `1` Jill.
---@field player_id integer The player entity's id (`g_playerEntity.id`): `1` is Jill.
---@field menu_choice integer The message window's choice byte (`g_menu_choice_id`): bit `0x80` while a message is up, then `0` for the first answer (yes) and nonzero for the other.
---@field camera integer The selected camera (`g_roomCameraId`), which a deferred switch may have moved ahead of the drawn one.
---@field fov integer The drawn camera's projection distance (589 without a camera).
---@field message_flags integer The message window's flag word (`g_message_flags`), read and write.
---@field player_y integer The player's Y, read and write; a write also moves the player's entity.
local ScreenApi = {}

-- # Game state

---Whether bit `bit` of flag bank `bank` is set.
---@param bank integer
---@param bit integer
---@return boolean
function ScreenApi:flag(bank, bit) end

---Set bit `bit` of flag bank `bank` (`Flg_on`).
---@param bank integer
---@param bit integer
function ScreenApi:flag_on(bank, bit) end

---Clear bit `bit` of flag bank `bank` (`Flg_off`).
---@param bank integer
---@param bit integer
function ScreenApi:flag_off(bank, bit) end

---The `behavior_flags` command byte of `g_EnemiesList[index]` (entity slot
---`index + 1`): the lab computer's forearms take their commands there.
---@param index integer
---@return integer
function ScreenApi:enemy_flags(index) end

---Set the `behavior_flags` byte of `g_EnemiesList[index]`.
---@param index integer
---@param value integer
function ScreenApi:set_enemy_flags(index, value) end

---The next value of the platform random stream (`rand()`), `0..=0x7FFF`.
---@return integer
function ScreenApi:rand() end

---Play sound `id` of sound bank `bank` with no position (`play_sfx`).
---@param bank integer
---@param id integer
function ScreenApi:sound(bank, id) end

---Play voice line `id` of the current stage's table and raise the
---voice-playing flag.
---@param id integer
function ScreenApi:voice(id) end

---Whether a voice line is playing.
---@return boolean
function ScreenApi:voice_playing() end

---Show room message `id` (`set_message_display`), masking `pause` out of
---the message flags until it closes. Refused while a message is up.
---[`menu_choice`](#ScreenApi.menu_choice) reads a yes/no answer.
---@param id integer
---@param pause integer
function ScreenApi:message(id, pause) end

---Cut to room camera `camera` now (`g_roomCameraId = camera;
---check_camera_switch(1)`). A camera without a switch-zone group leaves the
---zone tests on the group they were on, as the original's zone pointer does.
---@param camera integer
function ScreenApi:switch_camera(camera) end

---Set room camera `camera`'s projection distance, as the original's write
---into the RDT's camera record. It lasts until the room loads again.
---@param camera integer
---@param fov integer
function ScreenApi:set_camera_fov(camera, fov) end

---Turn room sprite (mask group) `id` of the current camera on or off
---(`RoomSpr_SetActive`/`RoomSpr_SetInactive`). A camera switch puts every
---group back on.
---@param id integer
---@param active boolean
function ScreenApi:set_room_sprite(id, active) end

---Room light `index` (0-2): position, colour, kind and radius.
---@param index integer
---@return integer x
---@return integer y
---@return integer z
---@return integer r
---@return integer g
---@return integer b
---@return integer kind
---@return integer radius
function ScreenApi:light(index) end

---Set room light `index` (0-2).
---@param index integer
---@param x integer
---@param y integer
---@param z integer
---@param r integer
---@param g integer
---@param b integer
---@param kind integer `0` point, otherwise directional.
---@param radius integer
function ScreenApi:set_light(index, x, y, z, r, g, b, kind, radius) end

---The room's ambient light.
---@return integer r
---@return integer g
---@return integer b
function ScreenApi:ambient() end

---Set the room's ambient light.
---@param r integer
---@param g integer
---@param b integer
function ScreenApi:set_ambient(r, g, b) end

-- # Textures

---Load the TIM pack entry `entry` into texture slot `slot` at VRAM texture
---page `tpage` (`LoadTexturePage`). The sprite calls search the slots for
---the page a descriptor names. An entry that is missing or not a TIM logs a
---warning and leaves the slot empty, so the sprites from it draw nothing.
---@param slot integer
---@param entry string
---@param tpage integer
---@return boolean loaded
function ScreenApi:load_texture(slot, entry, tpage) end

---Free texture slot `slot`. A room load frees every slot.
---@param slot integer
function ScreenApi:free_texture(slot) end

---Whether texture slot `slot` holds a page.
---@param slot integer
---@return boolean
function ScreenApi:texture_loaded(slot) end

-- # Drawing

---A sprite descriptor (`g_TextureDesc`): a plain table the script keeps,
---read field by field at each draw.
---@class SpriteDesc
---@field page integer The VRAM texture page; above 16 is the lower half.
---@field screen_x integer Game-space X (`0` is the screen centre).
---@field screen_y integer Game-space Y.
---@field pivot_x integer The pivot, which a scaled sprite scales about.
---@field pivot_y integer
---@field tex_u integer Texel U within the page (in the page's own pixels).
---@field tex_v integer Texel V.
---@field width integer
---@field height integer
---@field flags integer Bits 24-25 the colour depth (0 4-bit, 1 8-bit), bit 30 half alpha (with bits 28-29 the variant; variant 3 is opaque), bits 22-23 mirror V and U.
---@field r integer The colour multiplier, `0x80` neutral.
---@field g integer
---@field b integer
---@field scale_x integer `0x1000` is 1.
---@field scale_y integer

---`SubmitEffectSprite_Ex`: one unscaled sprite of `desc`, sorted among the
---scene's triangles at `key` (`key * 16 + 500` on the ordering table).
---`slot` and `count` are the texture slots searched for the page.
---@param desc SpriteDesc
---@param key integer
---@param slot integer
---@param count integer
function ScreenApi:effect_sprite(desc, key, slot, count) end

---`AddSprite_Ex`: one sprite of `desc` scaled about its pivot, drawn in the
---2D pass after the scene in `depth` order.
---@param desc SpriteDesc
---@param depth integer
---@param slot integer
---@param count integer
function ScreenApi:add_sprite(desc, depth, slot, count) end

---`PrintText8x8`: `text` in the 8x8 font at screen pixel `(x, y)`, in the
---overlay pass after every sprite. `colour`'s low nibble is the tint (`0`
---white, `1` green, `2` red, `3` grey, otherwise yellow). Its high nibble,
---the brightness, does not change the colour: the original only sorts the
---glyphs by it, and arklay ignores it. Unlike the original it leaves the
---script's descriptor alone; a script that relies on the fields the
---original leaves behind sets them itself.
---@param x integer
---@param y integer
---@param text string
---@param colour integer
function ScreenApi:print8x8(x, y, text, colour) end

---`PrintText8x14`: `text` in the message font at screen pixel `(x, y)`,
---over the scene. `colour`'s low nibble is the font's CLUT row (`0` white,
---`1` green, `2` red, `3` grey, otherwise yellow). The glyphs are opaque:
---the original's half-transparent flag is not modelled.
---@param x integer
---@param y integer
---@param text string
---@param colour integer
function ScreenApi:print8x14(x, y, text, colour) end

---A textured quad with free corners (`MarniDrawTrianglesPersp`): the
---`w`x`h` texels at `(u, v)` of slot `slot`'s page, sampled bilinearly and
---blended by texel alpha, at screen pixels top left, top right, bottom left
---and bottom right. `key` is its ordering-table key as stored (not scaled).
---Draws nothing when the slot is empty.
---@param slot integer
---@param key integer
---@param u integer
---@param v integer
---@param w integer
---@param h integer
---@param x0 integer
---@param y0 integer
---@param x1 integer
---@param y1 integer
---@param x2 integer
---@param y2 integer
---@param x3 integer
---@param y3 integer
function ScreenApi:smooth_quad(slot, key, u, v, w, h, x0, y0, x1, y1, x2, y2, x3, y3) end

-- # Single precision

---`value` rounded to the nearest `float`. Rounding the result of each
---`+ - * /` of floats gives the original's single-precision result exactly.
---@param value number
---@return number
function ScreenApi:f32(value) end

---The `float` sine of `value` (radians), as the original's `sinf`.
---@param value number
---@return number
function ScreenApi:sinf(value) end

---The `float` cosine of `value` (radians).
---@param value number
---@return number
function ScreenApi:cosf(value) end
