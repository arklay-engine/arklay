-- The lab computer terminal of room 5060 (`ComputerLab.cpp`,
-- `display_computer_lab` at 0x00412390).
--
-- The terminal is the second of the original's interactive screens: the game
-- loop keeps running underneath it (the room scripts, the camera switch, the
-- entities and the render), and the engine's dispatcher calls `display` once
-- a frame, right after the room scripts, while scenario flag 0x20 is up and
-- system flag 0x1E selects it.
--
-- It is a first-person state: the player is parked 9000 units below the
-- floor, the room shows camera 4 (the monitor) or 5 (the keyboard), the RDT's
-- lights are swapped for a fixed set, and the two forearm props in enemy
-- slots 0 and 1 (`enemy/14`, `enemy/15`) are driven as the on-screen hands
-- through their `behavior_flags` command byte (bit 0x20 is their "done"
-- answer).
--
-- Everything on the monitor is drawn from the `umb00`-`umb02` and `umbrella`
-- texture pages as depth-keyed scene sprites that interleave with the arms'
-- triangles, exactly like the original's ordering table. This file is a
-- transcription of the reference port's state machine: the names follow
-- `ComputerLab.cpp`, and the comments there explain the many overlaps and
-- quirks kept here.
--
-- All state lives in `s`, which the engine keeps between calls; this file
-- keeps none. `s.desc` is the original's shared `g_TextureDesc`: several
-- draws rely on what an earlier one left in it. Integer fields keep the C
-- widths through the `u8`/`s16`/`s32` helpers wherever the C arithmetic
-- wraps or truncates.

-- # Integer helpers -----------------------------------------------------------

local function u8(value)
    return value & 0xff
end

local function s16(value)
    value = value & 0xffff
    if value >= 0x8000 then
        return value - 0x10000
    end
    return value
end

local function s32(value)
    value = value & 0xffffffff
    if value >= 0x80000000 then
        return value - 0x100000000
    end
    return value
end

-- C integer division truncates toward zero; Lua's `//` floors.
local function tdiv(a, b)
    local quotient = a // b
    if (a % b ~= 0) and ((a < 0) ~= (b < 0)) then
        quotient = quotient + 1
    end
    return quotient
end

-- A float to an integer, truncating toward zero (`(int)x`).
local function trunc(x)
    if x >= 0 then
        return (x // 1) | 0
    end
    return -((-x) // 1) | 0
end

-- A float to a 16-bit integer as Rust's `as i16` does it: towards zero,
-- saturating, NaN to 0 (the C cast is undefined out of range).
local function f2i16(x)
    if x ~= x then
        return 0
    elseif x >= 32767 then
        return 32767
    elseif x <= -32768 then
        return -32768
    end
    return trunc(x)
end

local function min(a, b)
    if a < b then
        return a
    end
    return b
end

-- A 1-based list as a 0-based one, like the C arrays.
local function zero(list)
    local out = {}
    for i, value in ipairs(list) do
        out[i - 1] = value
    end
    return out
end

-- # Constants -----------------------------------------------------------------

local BANK_SCENARIO = 0
local BANK_LOCKS = 2
local BANK_ROOM_FLAGS = 8
local SCENARIO_INTERACTIVE = 0x20
-- The lab map's room-flag bit (`ROOM_FLAG_MAP_BASE + MAP_INDEX_LABORATORY`).
local LAB_MAP_FLAG = 0x81

local CAMERA_MONITOR = 4
local CAMERA_KEYBOARD = 5
-- Bit 7 of `pending_camera`: a deferred switch is waiting.
local CAMERA_PENDING = 0x80

-- The texture slots `cl_load_textures` fills, and their pages.
local SLOT_UMB00, SLOT_UMB01, SLOT_UMB02, SLOT_UMBRELLA = 0x17, 0x18, 0x19, 0x1c

-- The three lights the terminal installs over the room's own (`s_labLights`,
-- 0x004b3f20), and its ambient.
local LAB_LIGHTS = zero({
    { x = 14300, y = -1950, z = 9650, radius = 6735 },
    { x = 4450, y = -2650, z = 12000, radius = 7055 },
    { x = 14700, y = -1600, z = 5600, radius = 7495 },
})
local LAB_AMBIENT = 0x708

-- One row of art inside a window: `width << 3` by `height << 4` pixels.
local function row(tex_u, tex_v, width, height, page)
    return { tex_u = tex_u, tex_v = tex_v, width = width, height = height, page = page }
end

-- The row lists a window points at; the login window's row 2 width is
-- patched to reveal the password line (`s.login_row2_width`).
local ROWS = {
    login = zero({ row(0x00, 0x90, 32, 2, 9), row(0x00, 0xe0, 7, 1, 7), row(0x00, 0xf0, 0, 1, 7) }),
    panel_a = zero({ row(0x00, 0xb0, 32, 1, 9), row(0x00, 0x00, 0, 0, 7), row(0x60, 0xf0, 13, 1, 7) }),
    side = zero({ row(0x38, 0xe0, 3, 1, 7), row(0x50, 0xe0, 3, 1, 7), row(0xc8, 0xe0, 7, 1, 7) }),
    menu = zero({
        row(0xc8, 0xf0, 6, 1, 7),
        row(0x00, 0x00, 0, 0, 7),
        row(0xe0, 0x68, 4, 1, 7),
        row(0xe0, 0x78, 4, 1, 7),
    }),
    panel_b = zero({
        row(0x00, 0xc0, 21, 1, 9),
        row(0x00, 0xd0, 21, 1, 9),
        row(0x00, 0x00, 0, 0, 7),
        row(0x00, 0xf0, 12, 1, 7),
    }),
}
local ROW_COUNT = { login = 3, panel_a = 3, side = 3, menu = 4, panel_b = 4 }

-- The expected inputs, as virtual-keyboard codes (1 = A): JOHN, ADA, MOLE.
local EXPECT_USER = { 10, 15, 8, 14 }
local EXPECT_PASS = { 1, 4, 1 }
local EXPECT_CMD = { 13, 15, 12, 5 }

-- Key code to forearm and reach (0x004b40a8): bit 7 picks the left arm
-- (enemy slot 1), the low bits the reach command.
local ARM_FOR_KEY = zero({
    0x83, 0x82, 0x81, 0x01, 0x02, 0x02, 0x02, 0x03,
    0x83, 0x82, 0x81, 0x01, 0x01, 0x02, 0x02, 0x03,
    0x83, 0x82, 0x81, 0x01, 0x01, 0x01, 0x02, 0x03,
    0x83, 0x82, 0x81, 0x01, 0x01, 0x01, 0x03, 0x03,
})

-- A drawable quad of `cl_draw_quads`: `flags` bit 0 tiles `rep` copies down
-- instead of across, bits 6-7 are the mirror bits.
local function quad(x, y, tex_u, tex_v, width, height, flags, rep)
    return {
        x = x, y = y, tex_u = tex_u, tex_v = tex_v,
        width = width, height = height, flags = flags, rep = rep,
    }
end

-- The 9-slice window frame (0x004b40c8); six fields are rewritten from the
-- window's size before each draw.
local FRAME_QUADS = zero({
    quad(0, 0, 0x50, 0x20, 16, 16, 0x00, 0),
    quad(0, 0, 0x70, 0x20, 16, 16, 0x00, 0),
    quad(0, 0, 0x50, 0x40, 16, 16, 0x00, 0),
    quad(0, 0, 0x70, 0x40, 16, 16, 0x00, 0),
    quad(16, 0, 0x60, 0x20, 16, 16, 0x00, 0),
    quad(16, 0, 0x60, 0x40, 16, 16, 0x00, 0),
    quad(0, 16, 0x50, 0x30, 16, 16, 0x01, 0),
    quad(0, 16, 0x70, 0x30, 16, 16, 0x01, 0),
})

-- The virtual keyboard's frame (0x004b4128).
local KEYBOARD_FRAME = zero({
    quad(16, 16, 0x10, 0x20, 192, 96, 0x00, 0),
    quad(8, 8, 0x00, 0x48, 16, 8, 0x00, 12),
    quad(200, 8, 0x00, 0x48, 8, 8, 0x00, 0),
    quad(8, 16, 0x08, 0x50, 8, 16, 0x00, 0),
    quad(8, 32, 0x08, 0x60, 8, 16, 0x01, 5),
    quad(8, 112, 0x08, 0x70, 8, 8, 0x00, 0),
    quad(16, 112, 0x20, 0x80, 16, 8, 0x00, 11),
    quad(192, 112, 0x30, 0x80, 24, 8, 0x00, 0),
    quad(208, 8, 0x50, 0x80, 8, 16, 0x01, 6),
    quad(208, 104, 0x50, 0x80, 8, 8, 0x00, 0),
})

-- The full-screen monitor pictures by `msgFlags & 3` (0x004b4194); index 0
-- is the never-drawn overlap slot.
local MSG_PICTURES = zero({
    quad(0, 0, 0x00, 0x00, 0, 0, 0x00, 0),
    quad(0, 0, 0x00, 0x00, 120, 104, 0x00, 0),
    quad(0, 0, 0x78, 0x00, 120, 104, 0x00, 0),
    quad(0, 0, 0x00, 0x68, 120, 104, 0x00, 0),
})

-- The status banners by `statusIdx` (0x004b41bc); index 4 is two quads.
local STATUS_BANNERS = zero({
    quad(0, 0, 0x00, 0x00, 0, 0, 0x00, 0),
    quad(0, 0, 0x78, 0x68, 100, 32, 0x00, 0),
    quad(0, 0, 0x78, 0x88, 100, 32, 0x00, 0),
    quad(0, 0, 0x78, 0xa8, 100, 32, 0x00, 0),
    quad(0, 0, 0x78, 0xc8, 100, 24, 0x00, 0),
    quad(0, 24, 0x78, 0xa0, 100, 8, 0x00, 0),
})

-- `msgFlags & 0x10` (0x004b4220) and `msgFlags & 0x20` (0x004b4238).
local STRIP_A = zero({
    quad(12, 29, 0x84, 0x17, 42, 3, 0x00, 0),
    quad(12, 32, 0x84, 0x14, 20, 3, 0x00, 0),
})
local STRIP_B = zero({
    quad(12, 39, 0x84, 0x17, 30, 3, 0x00, 0),
    quad(12, 42, 0x8a, 0x17, 30, 3, 0x00, 0),
    quad(12, 47, 0x84, 0x14, 16, 3, 0x00, 0),
})

-- The "door released" marker origins and quads, one per door.
local DOOR_ORIGIN = zero({ { x = 50, y = 20 }, { x = 28, y = 8 } })
local DOOR_MARKER = zero({
    quad(0, 8, 0x68, 0xe0, 20, 8, 0x01, 3),
    quad(12, 4, 0x68, 0xe0, 12, 12, 0x00, 0),
})

-- The two Umbrella-mark wedge quads (0x004c4420), in ring order: `(x, y, u,
-- v)` per corner, +y up, origin at the wedge's base.
local WEDGE_QUADS = zero({
    zero({
        { x = -16, y = 40, u = 32, v = 24 },
        { x = 16, y = 40, u = 64, v = 24 },
        { x = 16, y = 0, u = 64, v = 64 },
        { x = -16, y = 0, u = 32, v = 64 },
    }),
    zero({
        { x = -16, y = 40, u = 0, v = 24 },
        { x = 16, y = 40, u = 32, v = 24 },
        { x = 16, y = 0, u = 32, v = 64 },
        { x = -16, y = 0, u = 0, v = 64 },
    }),
})

-- `2 * pi` as the original's float.
local TAU = 6.2831854820251465

-- # State ---------------------------------------------------------------------

local LOGO_SLOTS = 9

-- The terminal's statics, as they stand before the first visit.
local function init_statics(s)
    s.func, s.sub, s.sub2, s.sub3 = s.func or 0, s.sub or 0, s.sub2 or 0, s.sub3 or 0
    s.col, s.row = 0, 0
    s.timer_a, s.timer_b = 0, 0
    s.win = {}
    for i = 0, 3 do
        s.win[i] = {
            enabled = 0, row_count = 0, x = 0, y = 0, cols = 0, rows = 0,
            row_data = "login", anim_cmd = 0, anim_state = 0,
            alpha = 0, visible = 0, big = 0,
        }
    end
    s.text = {}
    for i = 0, 1 do
        local text = {}
        for j = 0, 8 do
            text[j] = 0
        end
        s.text[i] = { enabled = 0, win = 0, x = 0, y = 0, text = text, masked = 0 }
    end
    s.typer = { active = 0, state = 0, char_idx = 0, timer = 0, line = 0 }
    s.login_row2_width = 0
    s.input_enabled = 0
    s.caret_win, s.caret_base_x, s.caret_base_y = 0, 0, 0
    s.caret_mode, s.caret_pos, s.text_rec = 0, 0, 0
    s.blink_counter, s.blink_hold = 0, 0
    s.led_flags, s.msg_flags, s.status_idx, s.door_sel = 0, 0, 0, 0
    s.col_r, s.col_g, s.col_b = 0, 0, 0
    s.pad_repeat_latch, s.pad_repeat_timer = 0, 0
    s.pending_camera, s.pending_cam_delay = 0, 0
    s.simple_mode, s.intro_logo_state = 0, 0
    s.logo = {}
    for i = 0, LOGO_SLOTS - 1 do
        s.logo[i] = {
            active = 0, behaviour = 0, sub = 0, x = 0, y = 0,
            counter = 0, phase = 0, tex_u = 0, tex_v = 0,
            width = 0, height = 0, depth_base = 0,
            pivot_x = 0, pivot_y = 0, scale_x = 0, scale_y = 0, angle = 0,
        }
    end
    s.desc = {
        page = 0, clut_y = 0, pivot_x = 0, pivot_y = 0,
        screen_x = 0, screen_y = 0, tex_u = 0, tex_v = 0,
        width = 0, height = 0, flags = 0x01000000,
        r = 0x80, g = 0x80, b = 0x80,
        scale_x = 0x1000, scale_y = 0x1000,
    }
end

local function set_state_word(s, packed)
    s.func = packed & 0xff
    s.sub = (packed >> 8) & 0xff
    s.sub2 = (packed >> 16) & 0xff
    s.sub3 = (packed >> 24) & 0xff
end

local function set_colour(desc, level)
    desc.r, desc.g, desc.b = level, level, level
end

-- The byte at `index` of a field record from its text on: past the nine text
-- bytes the original reads on into the `masked` byte.
local function text_byte(text, index)
    if index <= 8 then
        return text.text[index]
    end
    if index == 9 then
        return text.masked
    end
    return 0
end

-- Whether the field, as `strcmp` sees it up to its first zero, is `expect`.
local function word_is(text, expect)
    local n = #expect
    for i = 0, n - 1 do
        if text.text[i] ~= expect[i + 1] then
            return false
        end
    end
    return n == 9 or text.text[n] == 0
end

-- # Host helpers --------------------------------------------------------------

local function set_arm(api, slot, value)
    api:set_enemy_flags(slot, value)
end

local function arm(api, slot)
    return api:enemy_flags(slot)
end

local function is_jill(api)
    return api.player_character ~= 0
end

-- `play_sfx(2, id)`: a room-bank sound with no position.
local function sfx(api, id)
    api:sound(2, id)
end

-- # The pad -------------------------------------------------------------------

-- `cl_keyboard_input` (0x00414010): the 8x4 key grid. Returns -2 EXIT, -1
-- ENTER, -3 BACKSPACE, a key code, or 0 when nothing was committed.
local function keyboard_input(s, api)
    local w3 = s.win[3]
    w3.enabled = w3.enabled & 0x7f
    local cell = s.row * 8 + s.col
    local result = cell
    if api.pad_pressed & 0x40 ~= 0 then
        if cell == 0 then
            result = -2
        elseif cell == 0x17 then
            result = -1
        elseif cell == 0x1d then
            result = -3
        elseif cell > 0x16 then
            result = cell - 1
        end
        w3.enabled = w3.enabled | 0x80
        return result
    end
    local dpad = api.raw_held & 0xf000
    if dpad == s.pad_repeat_latch and s.pad_repeat_latch ~= 0 then
        if s.pad_repeat_timer ~= 0 then
            s.pad_repeat_timer = s.pad_repeat_timer - 1
            return 0
        end
        s.pad_repeat_timer = 0xf
    else
        s.pad_repeat_timer = 8
    end
    s.pad_repeat_latch = api.raw_edge & 0xf000
    local old_row, old_col = s.row, s.col
    if dpad & 0x1000 ~= 0 and s.row > 0 then
        s.row = s.row - 1
    end
    if dpad & 0x4000 ~= 0 and s.row < 3 then
        s.row = s.row + 1
    end
    if dpad & 0x8000 ~= 0 then
        s.col = s.col - 1
        if s.col < 0 then
            s.col = 7
        end
    end
    if dpad & 0x2000 ~= 0 then
        s.col = s.col + 1
        if cell == 0x1d then
            s.col = s.col + 1
        end
        if s.col > 7 then
            s.col = 0
        end
    end
    cell = s.col + s.row * 8
    if cell == 0x1f then
        s.row = s.row - 1
    end
    if cell == 0x1e then
        s.col = s.col - 1
    end
    if s.col ~= old_col or s.row ~= old_row then
        w3.enabled = w3.enabled | 0x80
    end
    return 0
end

-- # Fields --------------------------------------------------------------------

-- `cl_text_begin` (0x00414230).
local function text_begin(s, rec)
    local text = s.text[rec]
    for i = 0, 8 do
        text.text[i] = 0
    end
    s.caret_win = text.win
    s.caret_pos = 0
    s.blink_counter = 0
    s.text_rec = rec
    s.caret_base_x = text.x
    s.caret_base_y = text.y
end

-- `cl_text_append` (0x004141b0): the caret stops at 7, so the eighth
-- character is overwritten in place.
local function text_append(s, key, rec)
    if key == -1 or key == -2 then
        return
    end
    local pos = s.caret_pos
    local buffer = s.text[rec].text
    if key == -3 then
        if s.caret_pos ~= 0 then
            -- At 7 the caret only steps back once the eighth cell is empty.
            if s.caret_pos < 7 or buffer[pos] == 0 then
                s.caret_pos = s.caret_pos - 1
            end
        end
        buffer[s.caret_pos] = 0
        return
    end
    buffer[pos] = u8(key)
    if s.caret_pos < 7 then
        s.caret_pos = s.caret_pos + 1
    end
end

-- # Fade and camera -----------------------------------------------------------

-- `cl_fade_step` (0x00415410): ramp the colour multiplier down, install
-- `msg_on_fade_out` at the darkest point, then ramp it back up.
local function fade_step(s, step, msg_on_fade_out)
    if s.timer_a == 2 then
        return true
    end
    if s.timer_b == 0 then
        s.col_r = u8(s.col_r - step)
        s.col_g = u8(s.col_g - step)
        s.col_b = u8(s.col_b - step)
        if s.col_r == 0 or s.col_g == 0 or s.col_b == 0 then
            s.col_r, s.col_g, s.col_b = 0, 0, 0
            s.msg_flags = msg_on_fade_out
            s.timer_b = s16(s.timer_b + 1)
            return false
        end
    else
        s.col_r = u8(s.col_r + step)
        s.col_g = u8(s.col_g + step)
        s.col_b = u8(s.col_b + step)
        if s.col_r >= 0x80 or s.col_g >= 0x80 or s.col_b >= 0x80 then
            s.col_r, s.col_g, s.col_b = 0x80, 0x80, 0x80
            s.timer_b = s16(s.timer_b + 1)
            return true
        end
    end
    return false
end

-- `cl_camera_step` (0x004154d0): the deferred camera switch.
local function camera_step(s, api)
    if s.pending_camera & CAMERA_PENDING == 0 then
        return
    end
    if s.pending_cam_delay ~= 0 then
        s.pending_cam_delay = s.pending_cam_delay - 1
        return
    end
    api:switch_camera(s.pending_camera & ~CAMERA_PENDING & 0xff)
    s.pending_cam_delay = 0
    s.pending_camera = s.pending_camera & ~CAMERA_PENDING & 0xff
end

-- # Textures ------------------------------------------------------------------

-- `cl_load_textures` (0x00477fa0).
local function load_textures(api)
    api:load_texture(SLOT_UMB00, "ui/umb00.tim", 7)
    api:load_texture(SLOT_UMB01, "ui/umb01.tim", 9)
    api:load_texture(SLOT_UMB02, "ui/umb02.tim", 0xb)
end

-- `cl_load_intro_texture` (0x00478070): umb02 makes way for the photos.
local function load_intro_texture(api)
    api:free_texture(SLOT_UMB02)
    api:load_texture(SLOT_UMBRELLA, "ui/umbrella.tim", 0)
end

local function free_textures(api)
    api:free_texture(SLOT_UMB00)
    api:free_texture(SLOT_UMB01)
    api:free_texture(SLOT_UMB02)
    api:free_texture(SLOT_UMBRELLA)
end

-- # The Umbrella boot logo (0x00488a60) -----------------------------------------

local function logo_alloc(s)
    for i = 0, LOGO_SLOTS - 1 do
        local slot = s.logo[i]
        if slot.active == 0 then
            slot.active = 1
            slot.sub = 0
            return i
        end
    end
    return nil
end

local function logo_spawn_wedge(s, parent, behaviour)
    local child = logo_alloc(s)
    if child == nil then
        return
    end
    local source = s.logo[parent]
    local slot = s.logo[child]
    slot.behaviour = behaviour
    slot.x = source.x
    slot.y = source.y
    slot.angle = source.angle
    slot.parent = parent
end

local function logo_spin(slot)
    slot.angle = slot.angle + 15
    if slot.angle == 0x168 then
        slot.angle = 0
    end
end

-- The shared fly-out (0x00489030).
local function logo_move_out(slot)
    if slot.x <= -0x2a0000 then
        slot.sub = slot.sub + 1
        slot.x = -0x2a0000
        return
    end
    slot.x = s32(slot.x - 0x30000)
    slot.scale_x = s16(slot.scale_x - 0xd0)
    slot.scale_y = s16(slot.scale_y - 0xd0)
end

-- The parent slot's angle and phase, read before this slot's update.
local function logo_parent(s, slot)
    if slot.parent == nil then
        return nil
    end
    local parent = s.logo[slot.parent]
    return { angle = parent.angle, phase = parent.phase }
end

local function logo_wedge_init(slot, tex_u)
    slot.sub = slot.sub + 1
    slot.tex_u = tex_u
    slot.tex_v = 0x18
    slot.width = 0x20
    slot.height = 0x28
    slot.pivot_x = 0x10
    slot.pivot_y = 0x28
    slot.scale_x = 0x1000
    slot.scale_y = 0x1000
    slot.depth_base = 5
end

local function logo_hub_spawn(s, index)
    local slot = s.logo[index]
    if slot.angle % 0x5a ~= 0 then
        return
    end
    slot.counter = s16(slot.counter - 1)
    if slot.counter == 0 then
        slot.sub = slot.sub + 1
        slot.counter = 3
        slot.phase = 0
        logo_spawn_wedge(s, index, 2)
        return
    end
    logo_spawn_wedge(s, index, 1)
end

local function logo_hub(s, index)
    local slot = s.logo[index]
    local sub = slot.sub
    if sub == 0 then
        slot.sub = slot.sub + 1
        slot.tex_u = 0x20
        slot.tex_v = 0x18
        slot.width = 0x20
        slot.height = 0x28
        slot.pivot_x = 0x10
        slot.pivot_y = 0x28
        slot.scale_x = 0x1000
        slot.scale_y = 0x1000
        slot.angle = 0
        slot.depth_base = 5
        slot.counter = 3
    elseif sub == 1 then
        if slot.angle == 0 then
            slot.counter = s16(slot.counter - 1)
            if slot.counter == 0 then
                slot.sub = slot.sub + 1
                slot.counter = 4
                logo_hub_spawn(s, index)
                local word = logo_alloc(s)
                if word ~= nil then
                    s.logo[word].behaviour = 3
                    s.logo[word].parent = index
                end
            end
        end
    elseif sub == 2 then
        logo_hub_spawn(s, index)
    elseif sub == 3 then
        if slot.angle == 0 then
            slot.counter = s16(slot.counter - 1)
            if slot.counter == 0 then
                slot.sub = slot.sub + 1
                slot.phase = 1
            end
        end
    elseif sub == 4 then
        logo_move_out(slot)
    elseif sub == 5 then
        if (slot.angle + 15) % 0x5a == 0 then
            slot.phase = 2
            slot.active = 0
            slot.sub = 0
        end
    end
    logo_spin(slot)
end

-- Behaviours 1 and 2: the wedges; behaviour 1 also seeds a follower once the
-- hub reaches 270 degrees.
local function logo_wedge(s, index, seeds)
    local slot = s.logo[index]
    local parent = logo_parent(s, slot)
    local sub = slot.sub
    if sub == 0 then
        logo_wedge_init(slot, seeds and 0x20 or 0)
    elseif sub == 1 then
        if seeds then
            if parent ~= nil and parent.angle == 0x10e then
                slot.sub = slot.sub + 1
                slot.phase = 0
                logo_spawn_wedge(s, index, 2)
            end
        elseif parent ~= nil and slot.angle - parent.angle == -0x2d then
            slot.sub = slot.sub + 1
        end
    elseif sub == 2 then
        logo_spin(slot)
        if parent ~= nil and parent.phase == 1 then
            slot.sub = slot.sub + 1
            if seeds then
                slot.phase = 1
            end
        end
    elseif sub == 3 then
        logo_spin(slot)
        logo_move_out(slot)
    else
        logo_spin(slot)
        if parent ~= nil and parent.phase == 2 then
            slot.phase = 2
            slot.active = 0
            slot.sub = 0
        end
    end
end

-- Behaviour 3: the UMBRELLA wordmark, which ends the sequence.
local function logo_wordmark(s, index, voice_playing)
    local slot = s.logo[index]
    local parent = logo_parent(s, slot)
    local sub = slot.sub
    if sub == 0 then
        slot.sub = slot.sub + 1
        slot.tex_u = 0x18
        slot.tex_v = 0
        slot.width = 0
        slot.height = 0x18
        slot.pivot_x = 0
        slot.pivot_y = 0
        slot.scale_x = 0x1000
        slot.scale_y = 0x1000
        slot.x = 0x3b0000
        slot.y = -0x370000
        slot.angle = 0
        slot.depth_base = 4
    elseif sub == 1 then
        slot.x = s32(slot.x - 0x12000)
        local xi = s16(slot.x // 0x10000)
        slot.width = s16(0x3b - xi)
        if xi < -0x1e then
            slot.sub = slot.sub + 1
            slot.x = -0x1e0000
        end
    elseif sub == 2 then
        if parent ~= nil and parent.phase == 2 then
            slot.sub = slot.sub + 1
            slot.x = -0x360000
            slot.width = 0x70
            slot.tex_u = 0
            slot.counter = 0x5a
        end
    elseif sub == 3 then
        if slot.counter > 0 then
            slot.counter = slot.counter - 1
        end
        if slot.counter < 1 and not voice_playing then
            slot.sub = slot.sub + 1
            slot.x = 0x20000
            slot.y = s32(slot.y + 0xc0000)
            slot.width = 0x70
            slot.tex_u = 0
            slot.pivot_x = 0x38
            slot.pivot_y = 0x0c
            slot.scale_x = 0x1000
            slot.scale_y = 0x1000
        end
    else
        if slot.scale_x < 0xaab then
            slot.scale_x = 0xaaa
            slot.scale_y = 0xaaa
        else
            slot.scale_x = slot.scale_x - 0x32
            slot.scale_y = slot.scale_y - 0x32
        end
        slot.y = s32(slot.y + 0x10000)
        if s32(slot.y & 0xffff0000) > -0x120001 then
            slot.y = -0x120000
            slot.active = 0
            s.intro_logo_state = 2
        end
    end
end

-- One wedge: `v' = 2 * (q rotated about Z) + (px, py, pz)`, projected through
-- the monitor camera's distance, in the original's single precision.
local function logo_draw_wedge(api, slot, which, fov)
    if not api:texture_loaded(SLOT_UMB02) then
        return
    end
    local function f32(x)
        return api:f32(x)
    end
    local sc = slot.scale_x
    local px = f32(tdiv(sc - 0x1000, 0x15) + 0.0)
    local py = f32(tdiv(0x1000 - sc, 0x1a) + 0x50 + 0.0)
    local pz = f32(tdiv(0x1000 - sc, 2) + 0x4b0 + 0.0)
    if pz <= 1.0 then
        return
    end
    local turns = f32(f32(slot.angle * 0xb + 0.0) * (1.0 / 4096.0))
    local th = f32(turns * TAU)
    local sn, cs = api:sinf(th), api:cosf(th)
    local f = f32(fov + 0.0)
    local wedge = WEDGE_QUADS[which]
    local ring = {}
    for i = 0, 3 do
        local corner = wedge[i]
        local qx = f32(corner.x * 2.0)
        local qy = f32(corner.y * 2.0)
        local vx = f32(f32(f32(qx * cs) + f32(qy * sn)) + px)
        local vy = f32(f32(f32(-qx * sn) + f32(qy * cs)) + py)
        local iz = f32(f / pz)
        ring[i] = {
            x = f2i16(f32(160.0 + f32(vx * iz))),
            y = f2i16(f32(120.0 - f32(vy * iz))),
        }
    end
    -- Ring order 0, 1, 2, 3 is top left, top right, bottom right, bottom left
    -- of the texture; the quad wants TL, TR, BL, BR.
    api:smooth_quad(
        SLOT_UMB02,
        (slot.depth_base + 0x80) * 0x10 + 500,
        wedge[0].u, wedge[0].v, 32, 40,
        ring[0].x, ring[0].y,
        ring[1].x, ring[1].y,
        ring[3].x, ring[3].y,
        ring[2].x, ring[2].y
    )
end

-- The logo's draw pass: the wordmark is a scaled sprite off umb02, the wedges
-- rotated quads.
local function logo_draw(s, api)
    local desc = s.desc
    desc.clut_y = 0
    desc.flags = 0x01000040
    local fov = api.fov
    for i = 0, LOGO_SLOTS - 1 do
        local slot = s.logo[i]
        if slot.active ~= 0 then
            if slot.behaviour ~= 3 then
                logo_draw_wedge(api, slot, slot.behaviour == 2 and 1 or 0, fov)
            else
                desc.page = 0x0b
                desc.clut_y = 0x1ed
                desc.pivot_x = slot.pivot_x
                desc.pivot_y = slot.pivot_y
                desc.tex_u = slot.tex_u
                desc.tex_v = slot.tex_v
                desc.width = slot.width & 0xffff
                desc.height = slot.height & 0xffff
                desc.scale_x = slot.scale_x
                desc.scale_y = slot.scale_y
                desc.screen_x = s16(slot.x // 0x10000)
                desc.screen_y = s16(slot.y // 0x10000)
                set_colour(desc, 0x80)
                api:add_sprite(desc, 10, SLOT_UMB02, 1)
            end
        end
    end
end

-- `cl_intro_logo_update` (0x00488a60).
local function intro_logo_update(s, api)
    if s.intro_logo_state == 0 then
        for i = 0, LOGO_SLOTS - 1 do
            s.logo[i].active = 0
            s.logo[i].sub = 0
        end
        local hub = logo_alloc(s)
        if hub ~= nil then
            s.logo[hub].behaviour = 0
            s.logo[hub].x = 0
            s.logo[hub].y = -0x2b0000
        end
        api:voice(0xb7)
        s.intro_logo_state = s.intro_logo_state + 1
    end
    for i = 0, LOGO_SLOTS - 1 do
        local slot = s.logo[i]
        if slot.active ~= 0 then
            local behaviour = slot.behaviour
            if behaviour == 0 then
                logo_hub(s, i)
            elseif behaviour == 1 then
                logo_wedge(s, i, true)
            elseif behaviour == 2 then
                logo_wedge(s, i, false)
            elseif behaviour == 3 then
                logo_wordmark(s, i, api:voice_playing())
            end
        end
    end
    logo_draw(s, api)
end

-- # Window animation ----------------------------------------------------------

local function windows_update(s)
    for i = 0, 3 do
        local w = s.win[i]
        if w.enabled ~= 0 and w.anim_cmd & 1 ~= 0 then
            if w.anim_state == 0 then
                w.anim_state = w.anim_state + 1
                if w.anim_cmd & 0x80 ~= 0 then
                    w.alpha = (w.big == 0) and 0xff or 0x80
                else
                    w.alpha = 0
                end
                w.visible = 1
            else
                local step = (w.big == 0) and 16 or 8
                local target = (w.big == 0) and 0xff or 0x80
                if w.anim_cmd & 0x80 == 0 then
                    w.alpha = w.alpha + step
                    if w.alpha >= target then
                        w.alpha = 0x80
                        w.visible = 0
                        w.anim_cmd = 0
                        w.anim_state = 0
                    end
                else
                    w.alpha = w.alpha - step
                    if w.alpha < 1 then
                        w.alpha = 0
                        w.anim_cmd = 0
                        w.anim_state = 0
                    end
                end
            end
        end
    end
end

local function window_setup(s, index, x, y, cols, rows, count, data)
    local w = s.win[index]
    w.x = x
    w.y = y
    w.cols = cols
    w.rows = rows
    w.row_count = count
    w.row_data = data
    w.alpha = 0x80
    w.visible = 0
    w.anim_state = 0
end

-- # Typing --------------------------------------------------------------------

-- The hands type what is on screen, one character a reach.
local function typing_update(s, api)
    local typer = s.typer
    if typer.active == 0 then
        return
    end
    local line = s.text[typer.line & 1]
    local state = typer.state
    if state == 0 then
        typer.state = typer.state + 1
        typer.char_idx = 0
    elseif state == 1 then
        typer.state = typer.state + 1
        local index = typer.char_idx
        local code = text_byte(line, index)
        if code == 0 then
            typer.state = 3
            set_arm(api, 0, 0x46)
            return
        end
        local reach = ARM_FOR_KEY[code & 31]
        local command = (reach & 0x3f) | 0x40
        set_arm(api, (reach & 0x80 ~= 0) and 1 or 0, command)
        if is_jill(api) then
            local next = ARM_FOR_KEY[text_byte(line, index + 2) & 31]
            typer.timer = ((next ~ reach) & 0x80 == 0) and 13 or 6
        else
            typer.timer = (api:rand() & 0xf) + 10
        end
    elseif state == 2 then
        typer.timer = typer.timer - 1
        if typer.timer ~= 0 then
            return
        end
        typer.char_idx = typer.char_idx + (is_jill(api) and 1 or 0) + 1
        if text_byte(line, typer.char_idx) == 0 then
            typer.state = 3
            set_arm(api, 0, 0x46)
            return
        end
        typer.state = 1
    else
        if arm(api, 0) & 0x20 ~= 0 then
            typer.active = 0
            typer.state = 0
        end
    end
end

-- # Draws ---------------------------------------------------------------------

-- `cl_draw_quads` (0x00414ad0): `quads[first..last]` back to front, `rep`
-- copies of each.
local function draw_quads(s, api, base_x, base_y, flags, quads, first, last, page, key_b)
    local desc = s.desc
    desc.page = page
    desc.pivot_x = 0
    desc.pivot_y = 0
    desc.clut_y = tdiv(page - 7, 2) + 0x1eb
    for i = last, first, -1 do
        local q = quads[i]
        desc.flags = ((q.flags & 0xc0) << 16) | (((flags & 0x82) | 2) << 23)
        desc.screen_x = s16(q.x + base_x)
        desc.screen_y = s16(q.y + base_y)
        desc.tex_u = q.tex_u
        desc.tex_v = q.tex_v
        desc.width = q.width & 0xffff
        desc.height = q.height & 0xffff
        desc.r, desc.g, desc.b = s.col_r, s.col_g, s.col_b
        local reps = q.rep
        if reps < 1 then
            reps = 1
        end
        local key = key_b
        if flags & 0x40 ~= 0 then
            key = key_b + 0x3ff
        end
        if q.flags & 1 == 0 then
            desc.screen_x = s16(desc.screen_x + s16((reps - 1) * q.width))
            for _ = 1, reps do
                api:effect_sprite(desc, key, SLOT_UMB00, 2)
                desc.screen_x = s16(desc.screen_x - q.width)
            end
        else
            desc.screen_y = s16(desc.screen_y + s16((reps - 1) * q.height))
            for _ = 1, reps do
                api:effect_sprite(desc, key, SLOT_UMB00, 2)
                desc.screen_y = s16(desc.screen_y - q.height)
            end
        end
    end
end

-- `cl_draw_keyboard` (0x00414300).
local function draw_keyboard(s, api)
    local saved_r, saved_g, saved_b = s.col_r, s.col_g, s.col_b
    local w3 = s.win[3]
    if w3.enabled & 0x7f ~= 0 and w3.alpha ~= 0 then
        local bright = u8(w3.alpha)
        local desc = s.desc
        desc.flags = 0x01000000
        desc.page = 9
        desc.width = 0x18
        desc.height = 0x18
        desc.clut_y = 0x1ec
        desc.pivot_x = 0
        desc.pivot_y = 0
        s.col_r, s.col_g, s.col_b = bright, bright, bright
        local col, row_ = s.col, s.row
        desc.screen_x = s16(col * 0x18 - 0x58)
        desc.screen_y = s16(row_ * 0x18 + 2)
        desc.tex_u = u8(col * 0x18 + 0x10)
        desc.tex_v = u8(row_ * 0x18 + 0x20)
        set_colour(desc, bright)
        local cell = row_ * 8 + col
        if cell == 0x17 then
            desc.height = 0x30
        end
        if cell == 0x1d then
            desc.width = 0x30
        end
        if s.blink_counter & 0x10 == 0 or s.blink_hold ~= 0 then
            api:effect_sprite(desc, 10, SLOT_UMB00, 2)
        end
        if w3.enabled & 0x80 ~= 0 then
            s.blink_hold = 0x14
        end
        if s.blink_hold ~= 0 then
            s.blink_hold = s.blink_hold - 1
        end
        draw_quads(s, api, -0x68, -0x0e, 0x8a, KEYBOARD_FRAME, 0, 9, 9, 10)
    end
    s.col_r, s.col_g, s.col_b = saved_r, saved_g, saved_b
end

-- One fixed photo of the simple-mode windows.
local function photo(desc, flags, alpha, x, y, u, v, width, height)
    desc.screen_x = x
    desc.screen_y = y
    desc.page = 0
    desc.clut_y = 0x1e0
    desc.tex_u = u
    desc.tex_v = v
    desc.width = width
    desc.height = height
    desc.pivot_x = 0
    desc.pivot_y = 0
    desc.flags = flags
    set_colour(desc, alpha)
end

-- `cl_draw_windows` (0x004144c0): windows 2, 1, 0; in simple mode each index
-- draws one fixed photo instead.
local function draw_windows(s, api)
    local saved_r, saved_g, saved_b = s.col_r, s.col_g, s.col_b
    local desc = s.desc
    for idx = 0, 2 do
        local w = s.win[2 - idx]
        if w.enabled ~= 0 and w.alpha ~= 0 then
            local alpha = u8(w.alpha)
            if s.simple_mode == 0 then
                local fw = s16((w.cols + 1) * 0x10)
                local fh = s16((w.rows + 1) * 0x10)
                local frame = {}
                for i = 0, 7 do
                    local q = FRAME_QUADS[i]
                    frame[i] = quad(q.x, q.y, q.tex_u, q.tex_v, q.width, q.height, q.flags, q.rep)
                end
                frame[1].x = fw
                frame[7].x = fw
                frame[2].y = fh
                frame[5].y = fh
                frame[3].x = fw
                frame[3].y = fh
                frame[4].rep = u8(w.cols)
                frame[5].rep = u8(w.cols)
                frame[6].rep = u8(w.rows)
                frame[7].rep = u8(w.rows)
                s.col_r, s.col_g, s.col_b = alpha, alpha, alpha
                local key = (4 - idx) * 0x80
                local blend = u8((((w.visible == 0) and 0x00 or 0x80) | 0x0b) + (5 - idx) * 10)
                draw_quads(s, api, w.x, w.y, blend, frame, 0, 7, 10, key)

                desc.page = 9
                desc.clut_y = 0x1ec
                desc.width = 0x10
                desc.height = 0x10
                desc.pivot_x = 0
                desc.pivot_y = 0
                desc.flags = (((w.visible == 0) and 0xc0000000 or 0) + 0x41000000) & 0xffffffff
                set_colour(desc, alpha)
                local row_left = w.x
                desc.screen_y = s16(w.y + 0x10)
                local rows = ROWS[w.row_data]
                for i = 0, min(w.row_count, ROW_COUNT[w.row_data]) - 1 do
                    local art = rows[i]
                    local width = art.width
                    if w.row_data == "login" and i == 2 then
                        width = s.login_row2_width
                    end
                    desc.screen_x = s16(row_left + 0x10)
                    if width ~= 0 then
                        desc.tex_u = art.tex_u
                        desc.tex_v = art.tex_v
                        desc.width = (width << 3) & 0xffff
                        desc.height = (art.height << 4) & 0xffff
                        desc.page = art.page
                        desc.clut_y = ((art.page - 7) >> 1) + 0x1eb
                        api:effect_sprite(desc, idx * -0x80 + 0x210, SLOT_UMB00, 2)
                    end
                    local pad = w.cols * 2 - width
                    if width < w.cols * 2 then
                        desc.tex_u = 0xe0
                        desc.tex_v = 0x30
                        desc.width = 8
                        desc.height = 0x10
                        desc.page = 9
                        desc.clut_y = 0x1ec
                        desc.screen_x = s16(row_left + 0x10 + s16(width * 8))
                        for j = 0, pad - 1 do
                            api:effect_sprite(desc, (j + idx * -8 + 0x22) * 0x10, SLOT_UMB00, 2)
                            desc.screen_x = s16(desc.screen_x + 8)
                        end
                    end
                    desc.screen_y = s16(desc.screen_y + s16(desc.height))
                end
            else
                local flags = (w.visible == 0) and 0 or 0x40000000
                if idx == 0 then
                    photo(desc, flags, alpha, -0x96, -0x69, 0x59, 0x5e, 0xa3, 0x50)
                    api:effect_sprite(desc, 1000, SLOT_UMBRELLA, 1)
                    desc.tex_u = 0x80
                    desc.tex_v = 0xaf
                    desc.screen_x = 0x0d
                    desc.width = 0x7e
                    api:effect_sprite(desc, 1000, SLOT_UMBRELLA, 1)
                elseif idx == 1 then
                    photo(desc, flags, alpha, -0x10, -0x39, 0x56, 0x0a, 0x59, 0x50)
                    api:effect_sprite(desc, 800, SLOT_UMBRELLA, 1)
                else
                    photo(desc, flags, alpha, 0x10, -0x28, 0xb3, 0x01, 0x4b, 0x5c)
                    api:effect_sprite(desc, 600, SLOT_UMBRELLA, 1)
                end
            end
        end
    end
    s.col_r, s.col_g, s.col_b = saved_r, saved_g, saved_b
end

-- `PrintText8x8("*", x, y, 0xF0, 0)`: one masked character at screen pixel
-- `(x, y)`, in the overlay pass after every sprite. It leaves its own
-- descriptor fields behind.
local function print_mask(s, api, x, y)
    api:print8x8(x, y, "*", 0xf0)
    local desc = s.desc
    desc.flags = 0x40
    desc.width = 8
    desc.height = 8
    desc.page = 30
    desc.clut_y = 0x1e0
    set_colour(desc, 128)
    desc.pivot_x = 0
    desc.pivot_y = 0
end

-- `cl_draw_text` (0x00414cd0): the typed characters; a masked field prints
-- `*` through the 8x8 text layer.
local function draw_text(s, api)
    local desc = s.desc
    desc.page = 9
    desc.clut_y = 0x1ec
    desc.width = 0x10
    desc.height = 0x10
    desc.pivot_x = 0
    desc.pivot_y = 0
    for r = 0, 1 do
        local t = s.text[r]
        local w = s.win[t.win & 3]
        local flags = (((w.visible == 0) and 0xc0000000 or 0) + 0x41000000) & 0xffffffff
        desc.flags = flags
        if not (t.enabled == 0 or w.enabled == 0 or w.anim_cmd ~= 0 or w.alpha == 0) then
            desc.screen_x = s16(t.x * 8 + 0x10 + w.x)
            desc.screen_y = s16((t.y + 1) * 0x10 + w.y)
            for i = 0, 7 do
                local sx, sy = desc.screen_x, desc.screen_y
                if t.text[i] ~= 0 then
                    local glyph = u8(t.text[i] - 1)
                    desc.tex_v = glyph & 0x10
                    desc.tex_u = u8(glyph * 0x10)
                    if t.masked == 0 then
                        api:effect_sprite(desc, 10, SLOT_UMB00, 2)
                    else
                        print_mask(s, api, sx + 0xa4, sy + 0x7c)
                        desc.tex_u = 0xb0
                        desc.tex_v = 0x0f
                        desc.page = 9
                        desc.clut_y = 0x1ec
                        desc.width = 0x10
                        desc.height = 0x10
                        desc.flags = flags
                        desc.screen_x = sx
                        desc.screen_y = sy
                    end
                end
                desc.screen_x = s16(desc.screen_x + 0x10)
            end
        end
    end
end

-- `cl_draw_caret` (0x00414ee0): the blinking block caret of a field, or the
-- highlight bar over a menu line.
local function draw_caret(s, api)
    if s.input_enabled == 0 then
        return
    end
    local w = s.win[s.caret_win & 3]
    if w.enabled == 0 then
        return
    end
    local desc = s.desc
    desc.flags = 0x01000040
    desc.page = 9
    desc.clut_y = 0x1ec
    desc.width = 0x10
    s.blink_counter = u8(s.blink_counter + 1)
    desc.pivot_x = 0
    desc.height = 0x10
    desc.pivot_y = 0
    if s.caret_mode == 0 then
        desc.tex_u = 0xb0
        desc.tex_v = 0x10
        desc.screen_x = s16(s16((s.caret_pos * 2 + 2 + s.caret_base_x) * 8) + w.x)
        desc.screen_y = s16(w.y + (s.caret_base_y + 1) * 0x10)
        local text = s.text[s.text_rec & 1]
        local filled = (text.text[s.caret_pos] or 0) ~= 0
        if s.blink_counter & 0x10 ~= 0 then
            if filled then
                return
            end
            desc.tex_v = 0x0f
        end
        if s.caret_pos == 7 and filled then
            desc.tex_v = 0x1f
            desc.height = 1
            desc.screen_y = s16(desc.screen_y + 0x0f)
        end
        api:effect_sprite(desc, 0x0f, SLOT_UMB00, 2)
        return
    end
    if s.blink_counter & 0x10 ~= 0 then
        return
    end
    local width, step, bias
    if s.caret_win == 0 then
        width, step, bias = 0x3b, 0x10, 0x0e
    elseif s.caret_win == 1 then
        width, step, bias = 0x49, 0x12, 0x0c
    else
        return
    end
    desc.flags = 0x40000000
    desc.screen_x = s16(s.caret_base_x * 8 + 8 + w.x)
    desc.clut_y = 0x1e0
    desc.width = width
    desc.height = 0x11
    desc.screen_y = s16((s.caret_pos + s.caret_base_y) * step + w.y + bias)
    desc.tex_v = 0xc6
    desc.page = 0
    desc.tex_u = 0
    desc.pivot_x = 0
    desc.pivot_y = 0
    set_colour(desc, 0x80)
    api:effect_sprite(desc, 200, SLOT_UMBRELLA, 1)
end

-- `cl_draw_status` (0x004151c0).
local function draw_status(s, api)
    if s.status_idx ~= 0 then
        local index = min(s.status_idx, 5)
        local count = (s.status_idx == 4) and 2 or 1
        local last = min(index + count, 6) - 1
        draw_quads(s, api, -0x30, -0x3d, 0x7c, STATUS_BANNERS, index, last, 7, 1000)
    end
    if s.msg_flags & 3 == 0 then
        return
    end
    if s.msg_flags & 3 == 2 then
        if s.msg_flags & 0x10 ~= 0 then
            draw_quads(s, api, -0x3c, -0x5f, 0x54, STRIP_A, 0, 1, 7, 0x5dc)
        end
        if s.msg_flags & 0x20 ~= 0 then
            draw_quads(s, api, -0x3c, -0x5f, 0x54, STRIP_B, 0, 2, 7, 0x5dc)
        end
    end
    local picture = s.msg_flags & 3
    draw_quads(s, api, -0x3c, -0x5f, (s.msg_flags & 0xf2) | 0x72, MSG_PICTURES, picture, picture, 7, 0x640)
end

-- `cl_draw_door_picture` (0x004152a0).
local function draw_door_picture(s, api)
    if s.door_sel & 0x80 == 0 then
        return
    end
    local desc = s.desc
    desc.screen_x = -0x28
    desc.screen_y = -0x5c
    desc.clut_y = 0x1e0
    desc.width = 0x54
    desc.height = 0x61
    desc.page = 0
    desc.flags = 0
    desc.tex_u = 0
    desc.pivot_x = 0
    desc.tex_v = 0
    desc.pivot_y = 0
    if s.door_sel & 0x7f == 0 then
        desc.tex_v = 100
        desc.screen_x = -0x26
        desc.screen_y = -0x5d
    end
    api:effect_sprite(desc, 0x578, SLOT_UMBRELLA, 1)
end

-- `cl_draw_leds` (0x00415340): the bezel lamps; the second flickers.
local function draw_leds(s, api)
    local desc = s.desc
    desc.flags = 0x41000040
    desc.page = 7
    desc.clut_y = 0x1eb
    desc.width = 6
    desc.height = 4
    desc.pivot_x = 0
    desc.pivot_y = 0
    if s.led_flags & 1 ~= 0 then
        desc.tex_u = 0xe8
        desc.tex_v = 0x88
        desc.screen_x = 0x6f
        desc.screen_y = -0x28
        api:effect_sprite(desc, 0x4b4, SLOT_UMB00, 2)
    end
    if s.led_flags & 2 ~= 0 and api:rand() & 1 ~= 0 then
        desc.tex_u = 0xe8
        desc.tex_v = 0x98
        desc.screen_x = 0x6f
        desc.screen_y = -0x23
        api:effect_sprite(desc, 4, SLOT_UMB00, 2)
    end
end

-- The full-screen monitor image, scaled by the descriptor's scale for the
-- CRT power-on stretch.
local function draw_monitor(s, api)
    local desc = s.desc
    desc.page = 7
    desc.clut_y = 0x1eb
    desc.pivot_x = 0x3c
    desc.pivot_y = 0x34
    desc.screen_y = -0x2b
    desc.screen_x = 0
    desc.tex_u = 0
    desc.height = 0x68
    desc.width = 0x78
    desc.tex_v = 0x68
    api:add_sprite(desc, 3, SLOT_UMB00, 1)
end

-- # Stage 0: boot -------------------------------------------------------------

local function stage_boot(s, api)
    local desc = s.desc
    if s.sub2 == 0 then
        local step = s.sub3 & 3
        if step == 0 then
            s.sub3 = s.sub3 + 1
            desc.scale_y = 100
            desc.scale_x = 0
            s.led_flags = 1
            s.msg_flags = 0x83
            s.timer_a = 0
        elseif step == 1 then
            s.timer_a = s16(s.timer_a + 0x800)
            desc.flags = 0x01000000
            set_colour(desc, 0x80)
            desc.scale_x = s.timer_a
            desc.scale_y = 100
            if s.timer_a > 0xfff then
                s.sub3 = s.sub3 + 1
                s.timer_a = 100
            end
        elseif step == 2 then
            s.timer_a = s16(s.timer_a + 0x800)
            if s.timer_a > 0x1000 then
                s.timer_a = 0x1000
                s.timer_b = 0x5a
                s.sub3 = s.sub3 + 1
                s.msg_flags = 0
            end
            desc.flags = 0x01000000
            desc.scale_x = 0x1000
            desc.scale_y = s.timer_a
        else
            s.timer_b = s.timer_b - 1
            desc.flags = ((s.timer_b & 1 == 0) and 0x40000000 or 0) + 0x01000000
            set_colour(desc, u8(tdiv(s.timer_b, 2)))
            desc.scale_x = 0x1000
            desc.scale_y = 0x1000
            if s.timer_b == 0x3c then
                sfx(api, 0x17)
            end
            if s.timer_b < 0x1e then
                s.led_flags = 3
            end
            if s.timer_b == 0 then
                s.intro_logo_state = 0
                s.led_flags = 1
                s.sub2 = s.sub2 + 1
                s.sub3 = 0
                if api.pad_held & 0xc0 ~= 0 then
                    s.msg_flags = 1
                    s.sub3 = 1
                    s.timer_a = 0x1e
                end
            end
        end
        draw_monitor(s, api)
        return
    end
    -- `cl_boot_logo` (0x00412870).
    local sub3 = s.sub3
    if sub3 == 0 then
        intro_logo_update(s, api)
        if s.intro_logo_state == 2 then
            s.msg_flags = 1
            s.sub3 = s.sub3 + 1
            s.timer_a = 0x1e
        end
    elseif sub3 == 1 then
        s.timer_a = s.timer_a - 1
        if s.timer_a == 0 then
            s.sub3 = s.sub3 + 1
            s.timer_a = 0x1e
            s.timer_b = 0
        end
    elseif sub3 == 2 then
        if fade_step(s, 8, 2) then
            s.timer_a = s.timer_a - 1
            if s.timer_a == 0 then
                s.sub = s.sub + 1
                s.sub2 = 0
            end
        end
    end
end

-- # Stage 1: login ------------------------------------------------------------

local function login_init(s, api)
    load_intro_texture(api)
    s.simple_mode = 0
    s.sub2 = s.sub2 + 1
    s.sub3 = 0
    window_setup(s, 0, -0x90, -0x68, 0x10, 4, 3, "login")
    s.win[0].enabled = 1
    s.pad_repeat_timer = 0
    s.login_row2_width = 0
    s.text[0].enabled = 0
    s.win[0].anim_cmd = 1
    s.win[3].anim_cmd = 1
    s.text[0].win = 0
    s.text[0].masked = 0
    s.text[1].masked = 0
    s.win[3].enabled = 1
    s.text[0].x = 7
    s.text[0].y = 2
    text_begin(s, 0)
    s.status_idx = 0
    s.col = 1
    s.row = 0
    s.pending_cam_delay = 0
    set_arm(api, 0, 0x47)
    set_arm(api, 1, 0x47)
    s.pending_camera = CAMERA_PENDING | CAMERA_KEYBOARD
end

-- Steps 1 and 3 of the login, also the command prompt's typing.
local function login_typing(s, api)
    s.win[3].row_count = 0
    local key = keyboard_input(s, api)
    if key == -1 then
        s.sub3 = s.sub3 + 1
        return
    end
    if key == -2 or api.pad_pressed & 0x80 ~= 0 then
        s.sub3 = s.sub3 + 1
        s.win[3].row_count = 1
        return
    end
    if key ~= 0 then
        text_append(s, key, s.timer_a & 1)
        if s.caret_pos ~= 7 then
            s.blink_counter = 0
        end
    end
end

local function login_step(s, api, step)
    if step == 0 then
        if s.win[0].anim_cmd == 0 and s.win[3].anim_cmd == 0 then
            s.sub3 = s.sub3 + 1
            s.timer_a = 0
            s.text[0].enabled = 1
            s.input_enabled = 1
            s.caret_mode = 0
        end
    elseif step == 1 or step == 3 then
        login_typing(s, api)
    elseif step == 2 then
        if s.win[3].row_count ~= 0 then
            set_state_word(s, 2)
            return
        end
        s.sub3 = s.sub3 + 1
        s.text[1].y = 3
        s.text[1].enabled = 1
        s.login_row2_width = 0xc
        s.text[1].x = 0xc
        s.text[1].masked = 1
        s.text[1].win = 0
        s.timer_a = 1
        text_begin(s, 1)
    elseif step == 4 then
        if s.win[3].row_count ~= 0 then
            set_state_word(s, 2)
            return
        end
        s.sub3 = s.sub3 + 1
        s.input_enabled = 0
        s.pending_cam_delay = 0
        s.win[0].anim_cmd = 0x81
        s.msg_flags = 2
        s.pending_camera = CAMERA_PENDING | CAMERA_MONITOR
        s.win[3].anim_cmd = 0x81
        set_arm(api, 0, 0x40)
        set_arm(api, 1, 0x40)
    elseif step == 5 then
        if s.win[0].anim_cmd == 0 and s.win[3].anim_cmd == 0 then
            s.sub2 = s.sub2 + 1
            s.sub3 = 0
            s.text[0].enabled = 0
            s.text[1].enabled = 0
            s.win[0].enabled = 0
            s.win[3].enabled = 0
        end
    elseif step == 6 then
        s.sub3 = s.sub3 + 1
        s.typer.line = 0
        s.typer.active = 1
    elseif step == 7 then
        if s.typer.active ~= 0 then
            return
        end
        s.typer.line = s.typer.line + 1
        if s.typer.line == 2 then
            s.sub3 = s.sub3 + 1
            return
        end
        s.typer.active = 1
    elseif step == 8 then
        if word_is(s.text[0], EXPECT_USER) and word_is(s.text[1], EXPECT_PASS) then
            s.sub3 = 4
            s.timer_a = 0x5a
            return
        end
        s.sub3 = 3
        s.status_idx = 4
        s.timer_a = 0x5a
        sfx(api, 0x19)
    elseif step == 9 then
        if s.timer_a > 0 then
            s.timer_a = s.timer_a - 1
        end
        if s.timer_a == 0 and arm(api, 0) & 0x20 ~= 0 and arm(api, 1) & 0x20 ~= 0 then
            s.sub2 = 0
            s.sub3 = 0
        end
        if s.timer_a == 0x4b then
            set_arm(api, 0, 0x44)
            set_arm(api, 1, 0x44)
        end
    else
        if s.timer_a > 0 then
            s.timer_a = s.timer_a - 1
        end
        if s.timer_a == 0 then
            s.sub = s.sub + 1
            s.sub2 = 0
        end
        if s.timer_a > 0x32 then
            draw_quads(s, api, -0x3c, -0x5f, 3, STRIP_A, 0, 0, 7, 10)
            return
        end
        s.msg_flags = s.msg_flags | 0x10
    end
end

local function stage_login(s, api)
    if s.sub2 == 0 then
        login_init(s, api)
    elseif s.sub2 == 1 then
        login_step(s, api, min(s.sub3, 10))
    else
        login_step(s, api, 6 + min(s.sub3, 4))
    end
end

-- # Stage 2: the door menu ----------------------------------------------------

local function menu_step(s, api, step)
    if step == 0 then
        if s.timer_a ~= 0 then
            s.timer_a = s.timer_a - 1
            if s.timer_a == 0 then
                s.win[1].enabled = 1
                s.win[1].anim_cmd = 1
            end
            return
        end
        if s.win[0].anim_cmd == 0 and s.win[1].anim_cmd == 0 then
            s.sub3 = s.sub3 + 1
            set_arm(api, 0, 0x47)
            set_arm(api, 1, 0x47)
            s.input_enabled = 1
        end
    elseif step == 1 then
        if api.pad_pressed & 0x40 ~= 0 then
            if s.caret_pos == 0 then
                s.door_sel = 0
                s.sub3 = 4
            elseif s.caret_pos == 1 then
                s.door_sel = 1
                s.sub3 = 4
            elseif s.caret_pos == 2 then
                s.sub3 = 2
                s.input_enabled = 0
            end
        end
        if api.pad_pressed & 0x80 ~= 0 then
            s.sub3 = 2
            s.input_enabled = 0
        end
    elseif step == 2 then
        s.sub3 = s.sub3 + 1
        window_setup(s, 0, 0x10, -0x28, 3, 4, 4, "menu")
        s.caret_base_x = 0
        s.win[0].enabled = 1
        s.input_enabled = 1
        s.caret_win = 0
        s.caret_pos = 0
        s.caret_mode = 1
        s.blink_counter = 0
        s.text_rec = 4
        s.win[0].alpha = 0x80
        s.caret_base_y = 2
        s.timer_b = 1
        set_arm(api, 0, 0x40)
    elseif step == 3 then
        local leave = api.pad_pressed & 0x80 ~= 0
        if api.pad_pressed & 0x40 ~= 0 then
            if s.caret_pos == 0 then
                set_state_word(s, 2)
                return
            end
            leave = true
        end
        if not leave then
            return
        end
        s.sub3 = 1
        s.caret_base_x = 0
        s.caret_win = 1
        s.caret_base_y = 0
        s.caret_pos = 0
        s.caret_mode = 1
        s.blink_counter = 0
        s.win[0].enabled = 0
        s.input_enabled = 1
        set_arm(api, 0, 0x47)
        s.text_rec = 5
        set_arm(api, 1, 0x47)
        s.timer_b = 2
    elseif step == 4 then
        s.sub3 = s.sub3 + 1
        s.input_enabled = 0
        s.msg_flags = 0x12
        s.win[1].anim_cmd = 0x81
        s.timer_a = 0x0c
        s.pending_camera = CAMERA_PENDING | CAMERA_MONITOR
    elseif step == 5 then
        if s.timer_a ~= 0 then
            s.timer_a = s.timer_a - 1
            if s.timer_a == 0 then
                s.win[2].anim_cmd = 0x81
            end
            return
        end
        if s.win[1].anim_cmd == 0 and s.win[2].anim_cmd == 0 then
            s.sub3 = s.sub3 + 1
            s.win[1].enabled = 0
            s.win[2].enabled = 0
            s.timer_a = 0x5a
            if s.door_sel == 0 then
                set_arm(api, 0, 0x46)
                s.timer_b = 1
                return
            end
            set_arm(api, 0, 0x48)
            s.timer_b = 2
        end
    elseif step == 6 then
        if arm(api, 0) & 0x20 == 0 then
            return
        end
        s.timer_b = s.timer_b - 1
        if s.timer_b ~= 0 then
            set_arm(api, 0, 0x46)
            return
        end
        s.sub3 = s.sub3 + 1
        s.led_flags = s.led_flags | 2
        set_arm(api, 1, 0x47)
    elseif step == 7 then
        s.timer_a = s.timer_a - 1
        if s.timer_a ~= 0 then
            s.status_idx = ((s.timer_a & 0xffff) & 0x10 == 0) and 1 or 0
            return
        end
        if s.door_sel == 1 then
            s.sub = 4
            s.sub2 = 0
            return
        end
        s.sub3 = s.sub3 + 1
        s.timer_a = 0x1e
        s.timer_b = 0
        s.status_idx = 0
    else
        s.timer_a = s.timer_a - 1
        if s.timer_a == 0 then
            s.timer_a = 0x1e
            s.timer_b = s.timer_b + 1
            if s.timer_b == 3 then
                s.sub = 3
                s.sub2 = 0
                s.timer_a = 0x1e
                s.timer_b = 0
                s.msg_flags = 0x32
            end
        end
        if s.timer_b ~= 0 then
            local count = min(s.timer_b, 3)
            draw_quads(s, api, -0x3c, -0x5f, 0x40, STRIP_B, 0, count - 1, 7, 0)
        end
    end
end

local function stage_menu(s, api)
    if s.sub2 == 0 then
        s.simple_mode = 1
        s.sub2 = s.sub2 + 1
        s.sub3 = 0
        window_setup(s, 1, -0x10, -0x38, 4, 3, 3, "side")
        s.win[1].enabled = 0
        s.win[1].anim_cmd = 0
        window_setup(s, 2, -0x90, -0x68, 0x10, 3, 3, "panel_a")
        s.caret_base_x = 0
        s.caret_base_y = 0
        s.caret_pos = 0
        s.win[2].enabled = 1
        s.win[2].anim_cmd = 1
        s.blink_counter = 0
        s.input_enabled = 0
        s.text[0].enabled = 0
        s.text[1].enabled = 0
        s.pending_cam_delay = 0
        s.status_idx = 0
        s.caret_win = 1
        s.caret_mode = 1
        s.text_rec = 5
        s.pending_camera = CAMERA_PENDING | CAMERA_KEYBOARD
        s.timer_a = 0x0c
        s.timer_b = 2
        return
    end
    if s.input_enabled ~= 0 then
        if api.pad_edge & 0x10 ~= 0 and s.caret_pos ~= 0 then
            s.blink_counter = 0
            s.caret_pos = s.caret_pos - 1
        end
        if api.pad_edge & 0x40 ~= 0 and s.caret_pos < s.timer_b then
            s.blink_counter = 0
            s.caret_pos = s.caret_pos + 1
        end
    end
    menu_step(s, api, min(s.sub3, 8))
end

-- # Stages 3 and 4: the command prompt and the unlocked screen ----------------

local function command_step(s, api, step)
    if step == 0 then
        s.sub3 = s.sub3 + 1
        s.win[2].anim_cmd = 0x81
        s.win[3].anim_cmd = 0x81
        s.input_enabled = 0
        s.msg_flags = 0x32
        set_arm(api, 0, 0x40)
        set_arm(api, 1, 0x40)
    elseif step == 1 then
        if s.win[2].anim_cmd ~= 0 or s.win[3].anim_cmd ~= 0 then
            return
        end
        s.sub3 = s.sub3 + 1
        s.win[2].enabled = 0
        s.win[3].anim_cmd = 0
        s.win[3].enabled = 0
        if s.win[3].row_count == 0 then
            s.typer.active = 1
            s.typer.line = 0
        else
            set_arm(api, 1, 0x46)
        end
        s.pending_cam_delay = 2
        s.pending_camera = CAMERA_PENDING | CAMERA_MONITOR
    elseif step == 2 then
        if s.win[3].row_count == 0 then
            if s.typer.active == 0 then
                s.sub3 = s.sub3 + 1
            end
            return
        end
        if arm(api, 1) & 0x20 ~= 0 then
            set_state_word(s, 0x201)
        end
    elseif step == 3 then
        if not word_is(s.text[0], EXPECT_CMD) then
            s.sub3 = 4
            s.timer_a = 0x5a
            s.status_idx = 2
            sfx(api, 0x19)
            return
        end
        s.sub3 = 5
        s.timer_a = 0x5a
        s.status_idx = 3
        s.led_flags = s.led_flags | 2
    elseif step == 4 then
        if s.timer_a > 0 then
            s.timer_a = s.timer_a - 1
        end
        if s.timer_a == 0 and arm(api, 0) & 0x20 ~= 0 and arm(api, 1) & 0x20 ~= 0 then
            s.sub = 2
            s.sub2 = 0
        end
        if s.timer_a == 0x4b then
            set_arm(api, 0, 0x44)
            set_arm(api, 1, 0x44)
        end
    elseif step == 5 then
        if s.timer_a > 0 then
            s.timer_a = s.timer_a - 1
        end
        if s.timer_a == 0 then
            s.sub = s.sub + 1
            s.sub2 = 0
            s.led_flags = 1
            s.text[0].enabled = 0
        end
    elseif step == 6 then
        -- `cl_cmd_unlock`: the point of the whole sequence.
        s.sub2 = s.sub2 + 1
        s.sub3 = 0
        s.pending_camera = CAMERA_PENDING | CAMERA_MONITOR
        s.status_idx = 0
        s.led_flags = 1
        s.door_sel = s.door_sel | 0x80
        s.timer_a = 0x5a
        api:flag_on(BANK_ROOM_FLAGS, LAB_MAP_FLAG)
        local lock_bit = 0x26 - ((s.door_sel & 0x7f == 0) and 1 or 0)
        if api:flag(BANK_LOCKS, lock_bit) then
            s.timer_a = 0x20
            return
        end
        api:flag_on(BANK_LOCKS, lock_bit)
        set_arm(api, 0, 0x45)
        set_arm(api, 1, 0x45)
    elseif step == 7 then
        s.timer_a = s.timer_a - 1
        if s.timer_a == 0 then
            s.sub2 = s.sub2 + 1
            s.timer_a = 0x10
        end
    else
        if s.sub3 == 0 then
            s.sub3 = 1
            s.timer_b = 0x96
        end
        s.timer_b = s.timer_b - 1
        if s.timer_b ~= 0 and api.pad_pressed & 0xc0 == 0 then
            local desc = s.desc
            desc.page = 0
            desc.flags = 0
            desc.screen_x = -0x38
            desc.screen_y = -0x40
            desc.tex_u = 3
            desc.tex_v = 0xd7
            desc.clut_y = 0x1e0
            desc.width = 0x72
            desc.pivot_x = 0
            desc.pivot_y = 0
            desc.height = 0x29
            api:effect_sprite(desc, 1000, SLOT_UMBRELLA, 1)
            return
        end
        set_state_word(s, 0x201)
        s.win[0].enabled = 0
        s.door_sel = 0
    end
end

local function stage_command(s, api)
    if s.sub2 == 0 then
        s.simple_mode = 0
        s.timer_a = s.timer_a - 1
        if s.timer_a ~= 0 then
            return
        end
        s.sub2 = s.sub2 + 1
        s.sub3 = 0
        s.led_flags = 1
        window_setup(s, 2, -0x90, -0x68, 0x10, 4, 4, "panel_b")
        s.win[2].enabled = 1
        s.win[2].anim_cmd = 1
        s.win[3].anim_cmd = 1
        s.text[0].enabled = 1
        s.text[0].masked = 1
        s.text[0].win = 2
        s.text[0].x = 0x0c
        s.text[0].y = 3
        text_begin(s, 0)
        s.win[3].enabled = 1
        s.caret_mode = 0
        s.input_enabled = 0
        s.pad_repeat_timer = 0
        s.pending_camera = CAMERA_PENDING | CAMERA_KEYBOARD
        s.col = 1
        s.row = 0
        s.timer_a = 0
        sfx(api, 0x19)
        set_arm(api, 0, 0x47)
        set_arm(api, 1, 0x47)
        return
    end
    if s.sub2 == 1 then
        if s.win[2].anim_cmd ~= 0 or s.win[3].anim_cmd ~= 0 then
            return
        end
        s.input_enabled = 1
        if s.sub3 ~= 0 then
            s.sub2 = s.sub2 + 1
            s.sub3 = 0
            return
        end
        login_typing(s, api)
        return
    end
    command_step(s, api, min(s.sub3, 8))
end

local function stage_unlocked(s, api)
    command_step(s, api, 6 + min(s.sub2, 2))
    local door = (s.door_sel & 0x7f ~= 0) and 1 or 0
    if s.timer_a & 0x10 ~= 0 then
        local origin = DOOR_ORIGIN[door]
        draw_quads(s, api, origin.x - 0x3c, origin.y - 0x5f, 5, DOOR_MARKER, door, door, 10, 0x4b0)
    end
end

-- # Top level -----------------------------------------------------------------

-- `computer_lab_init` (0x004123d0).
local function init(s, api)
    set_state_word(s, 1)
    local lights = {}
    for i = 0, 2 do
        local x, y, z, r, g, b, kind, radius = api:light(i)
        lights[i] = { x = x, y = y, z = z, r = r, g = g, b = b, kind = kind, radius = radius }
    end
    local ar, ag, ab = api:ambient()
    s.saved = {
        -- `g_roomCameraId`, which a deferred switch may have moved ahead of
        -- the drawn cut.
        camera = api.camera,
        player_y = api.player_y,
        lights = lights,
        ambient = { r = ar, g = ag, b = ab },
    }
    api:switch_camera(CAMERA_MONITOR)
    api.message_flags = api.message_flags & 0xfff8
    api.message_flags = api.message_flags & 0xfeff
    api.player_y = -9000
    for i = 0, 2 do
        local light = LAB_LIGHTS[i]
        api:set_light(i, light.x, light.y, light.z, 0x78, 0x78, 0x78, 0, light.radius)
    end
    api:set_ambient(LAB_AMBIENT, LAB_AMBIENT, LAB_AMBIENT)
    s.door_sel = 0
    s.input_enabled = 0
    for i = 0, 3 do
        local w = s.win[i]
        w.enabled = 0
        w.anim_cmd = 0
        w.alpha = 0
        w.big = 0
    end
    s.win[3].big = 1
    for i = 0, 1 do
        s.text[i].enabled = 0
    end
    s.col_r, s.col_g, s.col_b = 0x80, 0x80, 0x80
    set_arm(api, 0, 0x40)
    set_arm(api, 1, 0x40)
    load_textures(api)
    s.simple_mode = 0
end

-- `computer_lab_update` (0x004125b0).
local function update(s, api)
    local stage = min(s.sub, 4)
    if stage == 0 then
        stage_boot(s, api)
    elseif stage == 1 then
        stage_login(s, api)
    elseif stage == 2 then
        stage_menu(s, api)
    elseif stage == 3 then
        stage_command(s, api)
    else
        stage_unlocked(s, api)
    end
    windows_update(s)
    typing_update(s, api)
end

-- `cl_finish_start` (0x00413a00): close the windows and type the log-out
-- with the hands.
local function finish_start(s)
    if s.sub3 == 0 then
        s.sub3 = 1
        for i = 0, 3 do
            local w = s.win[i]
            w.anim_cmd = 0x81
            if w.enabled == 0 then
                w.anim_cmd = 0
            end
        end
        local text = s.text[0].text
        text[0], text[1], text[2], text[3], text[4] = 0x13, 0x08, 0x15, 0x14, 0
        s.typer.active = 1
        s.typer.line = 0
        s.input_enabled = 0
        s.pending_cam_delay = 0
        s.pending_camera = CAMERA_PENDING | CAMERA_MONITOR
        return
    end
    if s.sub3 == 1 then
        if s.typer.active == 0 then
            s.timer_b = 0
            s.sub3 = s.sub3 + 1
            s.led_flags = 3
        end
        return
    end
    if s.sub3 ~= 2 then
        return
    end
    fade_step(s, 8, 0)
    local idle = true
    for i = 0, 3 do
        if s.win[i].anim_cmd ~= 0 then
            idle = false
        end
    end
    if s.timer_b ~= 0 and idle then
        for i = 0, 3 do
            s.win[i].enabled = 0
        end
        s.sub2 = s.sub2 + 1
        s.desc.scale_x = 0x1000
        s.desc.scale_y = 0x1000
        s.msg_flags = 0
        s.timer_a = 0x1000
        s.led_flags = 0
    end
end

-- `computer_lab_finish` (0x00413920).
local function finish(s, api)
    local desc = s.desc
    if s.sub == 0 then
        local step = min(s.sub2, 2)
        if step == 0 then
            finish_start(s)
        elseif step == 1 then
            s.timer_a = s16(s.timer_a - 2000)
            desc.scale_x = 0x1000
            desc.scale_y = s.timer_a
            if s.timer_a < 0x65 then
                s.sub2 = s.sub2 + 1
                s.timer_a = 0x1000
            end
        else
            s.timer_a = s16(s.timer_a - 0x800)
            desc.flags = 0x01000000
            set_colour(desc, 0x80)
            desc.scale_x = s.timer_a
            desc.scale_y = 0x60
            if s.timer_a < 1 then
                desc.scale_x = 0
                s.sub = s.sub + 1
                s.timer_a = 0x1e
            end
        end
        typing_update(s, api)
        windows_update(s)
        if s.sub2 ~= 0 then
            set_colour(desc, 0x80)
            desc.flags = 0x05000000
            desc.page = 7
            desc.clut_y = 0x1eb
            desc.screen_x = 0
            desc.tex_u = 0
            desc.pivot_x = 0x3c
            desc.height = 0x68
            desc.tex_v = 0x68
            desc.pivot_y = 0x34
            desc.screen_y = -0x2b
            desc.width = 0x78
            api:add_sprite(desc, 3, SLOT_UMB00, 1)
        end
        return
    end
    s.timer_a = s.timer_a - 1
    if s.timer_a ~= 0 then
        return
    end
    -- `computer_lab_finish`'s order: the lights, the arms and the pages, then
    -- the camera, the gate and the player.
    local saved = s.saved
    s.saved = nil
    if saved ~= nil then
        for i = 0, 2 do
            local light = saved.lights[i]
            api:set_light(i, light.x, light.y, light.z, light.r, light.g, light.b, light.kind, light.radius)
        end
        api:set_ambient(saved.ambient.r, saved.ambient.g, saved.ambient.b)
    end
    set_arm(api, 0, 0x80)
    set_arm(api, 1, 0x80)
    s.func = 0
    api.message_flags = api.message_flags | 0x0107
    free_textures(api)
    if saved ~= nil then
        api:switch_camera(saved.camera)
    end
    api:flag_off(BANK_SCENARIO, SCENARIO_INTERACTIVE)
    if saved ~= nil then
        api.player_y = saved.player_y
    end
end

-- # The published view ---------------------------------------------------------
--
-- `s.view` is what the terminal shows a player, for observers such as the
-- player bot (see "The published view" in `lua-types/interactive.lua`). It is
-- the screen's whole contract with them: nothing outside this file reads any
-- other field of `s`, so the state machine above may change freely as long as
-- this keeps meaning the same.

-- How many characters field record `rec` holds: its text up to the first
-- zero byte.
local function typed_length(s, rec)
    local record = s.text and s.text[rec]
    local text = record and record.text
    if text == nil then
        return 0
    end
    local length = 0
    while length < 9 and text[length] ~= nil and text[length] ~= 0 do
        length = length + 1
    end
    return length
end

-- Whether window `index` is still animating (opening, closing, scrolling).
local function window_busy(s, index)
    local window = s.win and s.win[index]
    return window ~= nil and (window.anim_cmd or 0) ~= 0
end

-- The view of the `func`/`sub`/`sub2`/`sub3` block: `sub` 1 is the login
-- (its steps 1 and 3 type the user and the password), 2 the door menu (step
-- 1 picks a row, step 3 asks whether to leave), 3 the command prompt and 4
-- the unlocked screen, which a press dismisses.
local function view(s)
    local func, sub, sub2, sub3 = s.func or 0, s.sub or 0, s.sub2 or 0, s.sub3 or 0
    local input = (s.input_enabled or 0) ~= 0
    local stage, field, typed = "busy", "", 0
    if func == 1 then
        if sub == 1 and sub2 == 1 and sub3 == 1 then
            stage, field, typed = "login", "user", typed_length(s, 0)
        elseif sub == 1 and sub2 == 1 and sub3 == 3 then
            stage, field, typed = "login", "secret", typed_length(s, 1)
        elseif sub == 2 and sub2 >= 1 and sub3 == 1 and input then
            stage = "menu"
        elseif sub == 2 and sub2 >= 1 and sub3 == 3 then
            stage = "exit"
        elseif sub == 3 and sub2 == 1 and sub3 == 0
            and not window_busy(s, 2) and not window_busy(s, 3) then
            stage, field, typed = "command", "user", typed_length(s, 0)
        elseif sub >= 4 and sub2 >= 2 then
            stage = "unlocked"
        end
    end
    return {
        stage = stage,
        field = field,
        typed = typed,
        cell = (s.row or 0) * 8 + (s.col or 0),
        menu_row = s.caret_pos or 0,
    }
end

-- `display_computer_lab` (0x00412390): one frame of the terminal.
function display(api, s)
    if s.desc == nil then
        init_statics(s)
    end
    if s.func == 0 then
        init(s, api)
    elseif s.func == 1 then
        update(s, api)
    elseif s.func == 2 then
        finish(s, api)
    end
    draw_caret(s, api)
    draw_text(s, api)
    draw_windows(s, api)
    draw_keyboard(s, api)
    draw_door_picture(s, api)
    draw_status(s, api)
    draw_leds(s, api)
    camera_step(s, api)
    s.view = view(s)
end
