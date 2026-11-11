-- The guardhouse's numeric panel, beside room 408's door to the drug
-- storehouse (409): the original's `display_passcode_panel` (0x0042a0d0,
-- `InteractiveScreen.cpp`).
--
-- The panel is the first of the original's interactive screens. Room 408's
-- init sets system flag 0x1D, which selects it, and the flag-bank action at
-- the panel (AOT 4) raises scenario flag 0x20; the engine's dispatcher then
-- calls `display` once a frame, right after the room scripts, until the
-- panel lowers the flag.
--
-- It cuts to camera 5, a close-up of the 3x3 key pad whose room sprites 1-9
-- are its lamps, and asks "Will you push the keys?" (room message 14). Yes
-- gives the player a cursor over the keys: a key flips its own lamp and its
-- orthogonal neighbours'. Once all nine are lit the lamps go out one by one,
-- the panel plays its success sound and raises scenario flag 0x21, which
-- 408's main reads to turn the door to 409 back into a door. No, or cancel
-- on the pad, puts the camera back.
--
-- All state lives in `s`, which the engine keeps between calls. `s.func` and
-- `s.sub` are the original's `g_labSlidesFuncIndex` and
-- `g_labSlidesAnimState`, `s.sub2` its `g_passcodePanelAnimationState`: the
-- dispatcher zeroes all three when the gate rises.

-- # Constants -----------------------------------------------------------------

local BANK_SCENARIO = 0
-- Scenario flags 0x1E/0x1F pick the initial lamps (room 400 sets one from
-- the play time).
local SCENARIO_VARIANT_A = 0x1E
local SCENARIO_VARIANT_B = 0x1F
-- The interactive-screen gate, which the panel lowers when it closes.
local SCENARIO_INTERACTIVE = 0x20
-- Raised when the panel is solved; 408's main re-arms the door to 409.
local SCENARIO_SOLVED = 0x21

-- The room camera that shows the pad, and the fov the original writes into
-- its record (RDT + 0x198) whatever the RDT shipped.
local PANEL_CAMERA = 5
local PANEL_FOV = 0xC0

-- Room message 14, "There's a panel with number keys. Will you push the
-- keys?", with pause word 0xFF.
local PANEL_MESSAGE = 0x0E
local PANEL_MESSAGE_PAUSE = 0xFF

-- `g_message_flags &= 0xfeb0` while the panel is up (the player, the
-- characters and the controls stop); `|= 0x014f` when it closes.
local MESSAGE_FLAGS_KEEP = 0xFEB0
local MESSAGE_FLAGS_RESTORE = 0x014F

-- `play_sfx(2, id)`: a key press, and the panel solved.
local SOUND_BANK = 2
local SE_KEY = 0x17
local SE_SOLVED = 0x18

-- `pad_edge` bits (the raw D-pad's press edges) and `pad_pressed` bits.
local PAD_UP = 0x10
local PAD_RIGHT = 0x20
local PAD_DOWN = 0x40
local PAD_LEFT = 0x80
local PRESSED_CONFIRM = 0x40
local PRESSED_CANCEL = 0x80

-- `s_passcodePanelInitialStates` (0x004ba960): the lamps at init, picked by
-- the variant flags and the character.
local INITIAL_STATES = {
    { 0, 0, 0, 0, 0, 0, 0, 0, 1 },
    { 1, 0, 0, 1, 1, 0, 1, 0, 1 },
    { 0, 1, 0, 1, 1, 1, 0, 0, 1 },
    { 1, 0, 1, 0, 1, 1, 0, 0, 1 },
}

-- `s_passcodePanelAnimationDelays` (0x004ba9a0): the success animation's
-- lamp order (0-based) interleaved with the frames before the next lamp
-- goes out. Lua tables are 1-based: entry `i` is the C table's `i - 1`.
local SUCCESS_SEQUENCE = { 0, 30, 1, 25, 2, 16, 5, 8, 8, 6, 7, 4, 6, 2, 3, 1, 4, 0 }

-- The frames the success animation waits before it starts, and after the
-- last lamp before it signals the room.
local SUCCESS_WAIT = 30

-- # The state machine ---------------------------------------------------------

-- `passcode_panel_init` (0x0042a0f0).
local function init(s, api)
    s.func = s.func + 1
    s.sub = 0
    s.saved_camera = api.camera
    api:set_camera_fov(PANEL_CAMERA, PANEL_FOV)
    api:switch_camera(PANEL_CAMERA)
    api.message_flags = api.message_flags & MESSAGE_FLAGS_KEEP
    s.cursor_x = 0
    s.cursor_y = 0
    -- The variant flags are scenario flags, not the system bank the
    -- dispatcher picks the screen from; the character is the player
    -- entity's id (1 is Jill).
    local jill = api.player_id == 1
    local table = INITIAL_STATES[1]
    if api:flag(BANK_SCENARIO, SCENARIO_VARIANT_A) then
        table = jill and INITIAL_STATES[3] or INITIAL_STATES[1]
    elseif api:flag(BANK_SCENARIO, SCENARIO_VARIANT_B) then
        table = jill and INITIAL_STATES[4] or INITIAL_STATES[2]
    end
    s.lamps = {}
    for i = 1, 9 do
        s.lamps[i] = table[i]
    end
    s.active = false
    api:message(PANEL_MESSAGE, PANEL_MESSAGE_PAUSE)
end

-- `passcode_panel_finish` (0x0042a490).
local function finish(s, api)
    api.message_flags = api.message_flags | MESSAGE_FLAGS_RESTORE
    api:switch_camera(s.saved_camera or 0)
    s.active = false
    api:flag_off(BANK_SCENARIO, SCENARIO_INTERACTIVE)
end

-- `passcode_panel_wait_message` (0x0042a1f0): Yes opens the pad; No (any
-- other answer) closes the panel.
local function wait_message(s, api)
    local choice = api.menu_choice
    if choice & 0x80 ~= 0 then
        return
    end
    if choice ~= 0 then
        s.func = 2
        s.sub = 0
    else
        s.sub = 1
        s.active = true
    end
end

-- `passcode_panel_input` (0x0042a230).
local function input(s, api)
    local edge = api.pad_edge
    if edge & PAD_UP ~= 0 and s.cursor_y > 0 then
        s.cursor_y = s.cursor_y - 1
    end
    if edge & PAD_DOWN ~= 0 and s.cursor_y < 2 then
        s.cursor_y = s.cursor_y + 1
    end
    if edge & PAD_LEFT ~= 0 and s.cursor_x > 0 then
        s.cursor_x = s.cursor_x - 1
    end
    if edge & PAD_RIGHT ~= 0 and s.cursor_x < 2 then
        s.cursor_x = s.cursor_x + 1
    end
    local pressed = api.pad_pressed
    if pressed & PRESSED_CONFIRM ~= 0 then
        s.sub = s.sub + 1
    end
    if pressed & PRESSED_CANCEL ~= 0 then
        s.func = s.func + 1
        s.sub = 0
    end
end

-- `passcode_panel_toggle` (0x0042a2d0): the key under the cursor flips
-- itself and its orthogonal neighbours.
local function toggle(s, api)
    api:sound(SOUND_BANK, SE_KEY)
    local row, column = s.cursor_y, s.cursor_x
    local index = row * 3 + column + 1
    local lamps = s.lamps
    lamps[index] = lamps[index] ~ 1
    if row > 0 then
        lamps[index - 3] = lamps[index - 3] ~ 1
    end
    if row < 2 then
        lamps[index + 3] = lamps[index + 3] ~ 1
    end
    if column > 0 then
        lamps[index - 1] = lamps[index - 1] ~ 1
    end
    if column < 2 then
        lamps[index + 1] = lamps[index + 1] ~ 1
    end
    local lit = 0
    for i = 1, 9 do
        lit = lit + lamps[i]
    end
    if lit == 9 then
        s.sub = 3
        s.sub2 = 0
        s.timer = SUCCESS_WAIT
        s.sequence = 0
        s.active = false
    else
        s.sub = 1
    end
end

-- `passcode_panel_animation_start` (0x0042a3b0).
local function animation_start(s)
    s.timer = s.timer - 1
    if s.timer == 0 then
        s.sub2 = 1
        s.timer = 1
        s.sequence = 0
    end
end

-- `passcode_panel_animation_step` (0x0042a3e0): put the lamps out in the
-- original's order. `s.sequence` is the C table index.
local function animation_step(s)
    s.timer = s.timer - 1
    if s.timer ~= 0 then
        return
    end
    if s.sequence < #SUCCESS_SEQUENCE then
        s.lamps[SUCCESS_SEQUENCE[s.sequence + 1] + 1] = 0
    end
    s.sequence = s.sequence + 1
    if s.sequence < #SUCCESS_SEQUENCE then
        s.timer = SUCCESS_SEQUENCE[s.sequence + 1]
    else
        s.timer = 0
    end
    s.sequence = s.sequence + 1
    if s.timer == 0 then
        s.sub2 = s.sub2 + 1
        s.timer = SUCCESS_WAIT
    end
end

-- `passcode_panel_animation_finish` (0x0042a450).
local function animation_finish(s, api)
    s.timer = s.timer - 1
    if s.timer == 0 then
        s.func = s.func + 1
        api:sound(SOUND_BANK, SE_SOLVED)
        api:flag_on(BANK_SCENARIO, SCENARIO_SOLVED)
    end
end

-- `passcode_panel_update` (0x0042a1d0).
local function update(s, api)
    if s.sub == 0 then
        wait_message(s, api)
    elseif s.sub == 1 then
        input(s, api)
    elseif s.sub == 2 then
        toggle(s, api)
    elseif s.sub == 3 then
        if s.sub2 == 0 then
            animation_start(s)
        elseif s.sub2 == 1 then
            animation_step(s)
        else
            animation_finish(s, api)
        end
    end
end

-- # Drawing -------------------------------------------------------------------

-- `passcode_panel_draw` (0x0042a4c0): the key labels in the 8x14 font while
-- the pad takes input. The frame is `$` glyphs; each key is its digit and a
-- `$`, green (CLUT row 1) under the cursor and grey (row 3) elsewhere.
local function draw(s, api)
    if not s.active then
        return
    end
    api:print8x14(0x7B, 0x8C, "$$$$$$$$$", 0)
    api:print8x14(0x7B, 0xC4, "$$$$$$$$$", 0)
    for row = 0, 2 do
        local y = (row + 0x0B) * 0x0E
        api:print8x14(0x7B, y, "$$", 0)
        api:print8x14(0xBB, y, "$", 0)
        for column = 0, 2 do
            local colour = 3
            if s.cursor_x == column and s.cursor_y == row then
                colour = 1
            end
            api:print8x14(column * 0x10 + 0x8B, y, string.char(row * 3 + column + 0x31) .. "$", colour)
        end
    end
end

-- `passcode_panel_update_room_sprites` (0x0042a610): every frame, lamp `i`
-- keeps room sprite `i` on or off.
local function update_room_sprites(s, api)
    for i = 1, 9 do
        api:set_room_sprite(i, s.lamps[i] ~= 0)
    end
end

-- # The screen function -------------------------------------------------------

function display(api, s)
    -- The dispatcher zeroes the block in a state it already keeps; the
    -- first visit's table starts empty.
    s.func = s.func or 0
    s.sub = s.sub or 0
    s.sub2 = s.sub2 or 0
    if s.func == 0 then
        init(s, api)
    elseif s.func == 1 then
        update(s, api)
    elseif s.func == 2 then
        finish(s, api)
    end
    if s.lamps ~= nil then
        draw(s, api)
        update_room_sprites(s, api)
    end
end
