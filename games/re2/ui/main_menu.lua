-- RE2's status screen: its ITEMS page as the original draws it. The engine
-- runs the page (`src/game/status2.rs`): the slots, the command menu (EQUIP
-- or USE, CHECK, COMBN), the combine target and the tab bar. This script
-- draws what `api:re2_items()` reports, from the original's art in the pack
-- (`ui/status/`: the `st<n>_jp.tim` sheets, the `itemall.bin` icons and the
-- `itemdata.bin` CHECK pictures).
--
-- The layout is the original's primitive list (`0x4F8000` and the routines
-- it calls), read from its frames: every sprite's place, source rectangle
-- and colour, in the original's draw order. The cursor's pulse and the
-- condition panel's trace are stepped once a tick by `M.animate`, which the
-- engine calls on every tick the screen is up, its open and close fades
-- included, as the original's draw routines run on every frame.
--
-- The item names and the messages are drawn in the port's font, at the
-- original's place.
--
-- It keeps the engine's contract with the pause menu: `open`, `tick`,
-- `update`, `tick_monitor`, `view_closed`, `reselect`, `move_cursor`,
-- `present` and `draw`, publishing `cursor`, `slot` and `selected_item`, and
-- `event` and `event_id`.

local M = {}

local EVENT_NONE = 0
local PERSONAL_SLOT = 10
local PAGE_TABS, PAGE_ITEMS = 2, 3
local SUB_GRID, SUB_MENU, SUB_COMMAND, SUB_OPEN, SUB_CLOSE = 0, 1, 2, 3, 4
local COMMAND_COMBINE, COMMAND_CHECK = 1, 2
local FIRST_AMMO = 0x14
local CHECK_DONE = 13

-- The open's first drawn tick ([`re2_status_open_view`] in the engine) and
-- the close's first tick without the screen.
local OPEN_SHOWN, CLOSE_BLACK = 7, 7

-- The sheets' texture handles in the original's table (`0x67C7D8`): the
-- character's `st<n>_jp.tim` TIMs 0-7, then the three icon pages.
local ST, ICONS = 0x12, 0x1A

-- The page's frame (texture 0x12), drawn first, in the original's order.
-- `msg` marks the message box's pieces, which CHECK moves.
local FRAME_TABS = {
    { 108, 16, 52, 12, 0, 164 },
    { 160, 16, 49, 12, 3, 164 },
    { 209, 16, 49, 12, 3, 164 },
    { 258, 16, 52, 12, 3, 164 },
}
local MESSAGE_BOX = { 7, 170, 217, 222 }
local MESSAGE_FRAME = {
    { 7, 170, 83, 4, 0, 147 },
    { 90, 170, 128, 4, 0, 88 },
    { 7, 223, 83, 4, 0, 151 },
    { 90, 223, 128, 4, 0, 84 },
    { 7, 174, 5, 26, 123, 92 },
    { 7, 200, 5, 23, 123, 151 },
    { 213, 174, 5, 26, 123, 174 },
    { 213, 200, 5, 23, 123, 233 },
    { 218, 210, 8, 14, 107, 242 },
    { 226, 212, 16, 9, 80, 155 },
    { 242, 212, 16, 9, 80, 155 },
    { 258, 212, 16, 9, 80, 155 },
    { 274, 212, 16, 9, 80, 155 },
    { 290, 212, 16, 9, 80, 155 },
    { 306, 212, 16, 9, 80, 155 },
    { 322, 212, 16, 9, 80, 155 },
    { 338, 212, 16, 9, 80, 155 },
}
local FRAME = {
    { 11, 16, 39, 4, 56, 164 },
    { 11, 30, 39, 2, 56, 168 },
    { 11, 71, 39, 4, 56, 171 },
    { 7, 16, 4, 59, 106, 92 },
    { 49, 16, 4, 59, 106, 152 },
    { -9, 16, 16, 14, 107, 242 },
    { -9, 32, 16, 14, 107, 242 },
    { -9, 48, 16, 14, 107, 242 },
    { 53, 32, 7, 7, 56, 178 },
    { 53, 60, 7, 7, 56, 178 },
    { 60, 29, 99, 46, 0, 92 },
    { 173, 65, 80, 3, 0, 155 },
    { 173, 29, 80, 6, 0, 158 },
    { 168, 29, 5, 39, 91, 164 },
    { 253, 29, 5, 39, 96, 164 },
    { 161, 29, 11, 39, 50, 211 },
    { 260, 29, 50, 41, 0, 211 },
    { 220, 70, 90, 3, 0, 140 },
    { 220, 193, 90, 4, 0, 143 },
    { 220, 73, 5, 120, 113, 92 },
    { 305, 73, 5, 120, 118, 92 },
}
-- CHECK's picture panel, at its top `y`: a fill and its frame.
local CHECK_FILL = { 12, 36, 24 }
local MESSAGE_FILL = { 12, 12, 24 }

-- The portrait (texture 0x13) and the condition word (texture 0x16, one
-- 11-row band per condition).
local PORTRAIT = {
    { 11, 20, 39, 10, 0, 72 },
    { 11, 31, 39, 20, 0, 86 },
    { 11, 51, 39, 20, 0, 107 },
}
local CONDITION_X, CONDITION_Y = 105, 54

-- The tab labels (texture 0x17, `0x4FEF70`): the page's tab is red on the
-- item page, white on the tab bar and while the screen closes.
local TAB_X, TAB_Y, TAB_W, TAB_H = 112, 17, 47, 10
local TAB_LIT, TAB_SELECTED, TAB_DIM = { 128, 64, 64 }, { 128, 128, 128 }, { 40, 40, 40 }

-- The slots: icons 40x30 from the grid's corner, the personal slot above.
local GRID_X, GRID_Y, SLOT_W, SLOT_H = 225, 73, 40, 30
local PERSONAL_X, PERSONAL_Y = 265, 35
-- The equipped item: its box (texture 0x1C) and its icon.
local EQUIP_BOX = { 173, 35, 80, 30, 40, 90 }
local EQUIP_X, EQUIP_Y = 193, 35
-- The counts (texture 0x14): `0x53EF58` per slot from the grid's frame at
-- (220, 70), and the equipped item's.
local COUNT_ORIGIN_X, COUNT_ORIGIN_Y = 220, 70
local COUNT_AT = {
    [0] = { 6, 24 }, { 46, 24 }, { 6, 54 }, { 46, 54 }, { 6, 84 }, { 46, 84 },
    { 6, 114 }, { 46, 114 }, { 6, 144 }, { 46, 144 }, { 46, -14 },
}
local EQUIP_COUNT_X, EQUIP_COUNT_Y = 194, 55
-- Byte 2 of each id's item-table record (`0x53DE28`): the count's CLUT row
-- in the low nibble (4 for none) and `0x80` for a percent sign. Every id
-- past 0x1F draws no count.
local COUNT_FLAGS = {
    [0] = 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x02, 0x00, 0x00, 0x80, 0x80,
    0x81, 0x00, 0x80, 0x00, 0x03, 0x03, 0x03, 0x81, 0x03, 0x01, 0x02, 0x83, 0x83, 0x03, 0x00, 0x00,
}
local INFINITE = 0xFF

-- The cursor (texture 0x15): three pieces over a slot, grey `pulse`.
local CURSOR_X, CURSOR_Y = 223, 71
local PERSONAL_CURSOR_X, PERSONAL_CURSOR_Y = 263, 33
-- `0x4FE5F0`: up by 3 a frame until past 120, then down by 3 until under
-- 50; held at 60 off the grid, going on the way it was when it comes back.
local PULSE_START, PULSE_HIGH, PULSE_LOW, PULSE_HELD, PULSE_STEP = 50, 120, 50, 60, 3
-- COMBN's target cursor (`0x4FE7C0`) pulses on its own, from 80 when the
-- screen opens: up by 3 until past 124, then down by 3 until under 70, a
-- step each time the page's logic draws it.
local TARGET_START, TARGET_HIGH, TARGET_LOW = 80, 124, 70
-- The steps that draw it: choosing the target, the question ("Will you mix
-- the herb?") and the slide after a combine, over its ten ticks.
local STEP_TARGET, STEP_SLIDE, STEP_QUESTION, SLIDE_TICKS = 0, 1, 11, 10

-- The command menu (texture 0x12), its labels (textures 0x18, 0x19) and
-- where it rests; the labels sit 4 right of it.
local MENU_X, MENU_SHUT, MENU_STEP = 162, 227, 13
local CHECK_MENU_X, CHECK_MENU_STEP = 242, 8
local MENU_PIECES = {
    { 70, 56, 13, 0, 176 },
    { 83, 56, 21, 0, 189 },
    { 104, 56, 21, 0, 189 },
    { 125, 56, 21, 0, 189 },
}
local LABEL_LIT = { 64, 64, 128 }
local LABEL_EQUIP, LABEL_USE, LABEL_CHECK, LABEL_COMBINE = 0, 16, 32, 48

-- The item's name, under the message box's top edge.
local NAME_X, NAME_Y = 15, 175

-- The condition panel's trace (`0x4FFAB0`), from the panel's corner.
local PANEL_X, PANEL_Y = 58, 29
local TRACE_LEN, TRACE_TAIL = 80, 32
-- Per condition the trace's colour and the step it fades by per line
-- (`0x53F2CC`), and its wave: a (y, length) pair per column (`0x53EC38`).
local TRACE_COLOURS = {
    [0] = { 0x20, 0xFF, 0x20, 0x01, 0x08, 0x01 },
    { 0xFF, 0xFF, 0x20, 0x08, 0x08, 0x01 },
    { 0xFF, 0x80, 0x20, 0x08, 0x04, 0x01 },
    { 0xFF, 0x20, 0x20, 0x08, 0x01, 0x01 },
    { 0xFF, 0x20, 0xFF, 0x08, 0x01, 0x08 },
}
local WAVES = {
    [0] = {
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 14, 0, 13, 0, 12, 0, 12, 0, 13, 2, 15, 3, 18, 2,
        20, 0, 16, 4, 8, 8, 5, 3, 4, 0, 5, 3, 8, 7, 15, 4, 19, 5, 24, 3, 27, 0, 25, 2, 21, 4, 16, 5, 14, 2, 13, 0, 14, 2, 16, 3, 19, 0, 19, 0,
        18, 0, 16, 2, 14, 2, 13, 0, 12, 0, 13, 0, 14, 1, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
    },
    {
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 14, 0, 13, 0, 12, 0, 12, 0, 14, 0, 13, 2, 15, 3, 18, 2,
        20, 0, 16, 4, 8, 8, 6, 2, 5, 0, 6, 2, 8, 5, 13, 2, 15, 0, 16, 4, 20, 2, 22, 0, 21, 0, 16, 5, 15, 0, 14, 0, 14, 0, 13, 0, 13, 0, 14, 0,
        15, 0, 15, 0, 15, 0, 14, 0, 14, 0, 14, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
    },
    {
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 16, 0, 16, 0, 17, 0, 17, 0, 17, 0, 16, 0, 15, 0, 14, 0,
        14, 0, 14, 0, 15, 0, 15, 0, 15, 0, 14, 0, 11, 3, 10, 0, 9, 0, 10, 4, 13, 3, 16, 3, 19, 0, 20, 0, 19, 0, 18, 0, 16, 2, 14, 2, 13, 0, 12, 0,
        12, 0, 12, 0, 13, 0, 14, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
    },
    {
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 14, 0,
        14, 0, 13, 0, 13, 0, 14, 0, 15, 0, 15, 0, 15, 0, 16, 0, 17, 2, 17, 0, 17, 0, 14, 3, 10, 4, 9, 0, 10, 2, 12, 3, 15, 0, 16, 0, 16, 0, 16, 0,
        16, 0, 15, 0, 14, 0, 13, 0, 13, 0, 14, 0, 14, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
    },
    {
        15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 14, 1, 13, 1, 12, 1, 12, 1,
        13, 2, 15, 3, 18, 2, 20, 1, 16, 4, 8, 8, 5, 3, 4, 1, 5, 3, 8, 7, 15, 4, 19, 5, 24, 2, 26, 1, 25, 1, 15, 10, 14, 1, 15, 1, 16, 2, 18, 1,
        17, 1, 10, 7, 9, 1, 10, 2, 12, 4, 16, 1, 17, 1, 18, 1, 18, 1, 18, 1, 17, 1, 16, 1, 15, 1, 15, 1, 15, 1, 15, 1, 15, 1, 12, 3, 10, 2, 9, 1,
        10, 6, 16, 3, 19, 1, 19, 1, 17, 2, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0, 15, 0,
    },
}
-- The heal's and the cure's sweeps (panel modes 1 and 3, and 2).
local HEAL_FIRST, HEAL_LAST, HEAL_TOP, HEAL_BOTTOM = 11, 0x5C, 4, 0x23
local CURE_FIRST, CURE_LAST, CURE_LEFT, CURE_RIGHT = 3, 0x23, 0x0B, 0x5C
-- The original draws the trace a hi-res pixel wide at three quarters'
-- strength: half of a 320-wide column.
local TRACE_ALPHA, TRACE_WEIGHT = 128, 3 / 4

local s = {
    slots = 8,
    show_cursor = true,
    show_name = true,
    -- `M.animate`'s state: the page this tick and the last.
    page = nil,
    last = nil,
    open = 0,
    close = nil,
    pulse = PULSE_START,
    pulse_falling = false,
    trace = 0,
    trace_mode = 0,
    target_pulse = TARGET_START,
    target_falling = false,
    target_shown = false,
    menu_item = 0,
    art = nil,
}
M.state = s

local function emit(api)
    api:set("event", EVENT_NONE)
    api:set("event_id", 0)
end

local function refresh(api)
    local page = api:re2_items()
    local cursor = page and page.cursor or 0
    local item = api:re2_slot(cursor)
    api:select_item(item)
    api:set("cursor", cursor)
    api:set("slot", cursor)
    api:set("selected_item", item)
    api:set("ready", true)
end

function M.open(api, _args)
    api:set("canvas", 1)
    s.slots = api:inventory_capacity()
    s.page, s.last = nil, nil
    s.open, s.close = 0, nil
    s.pulse, s.pulse_falling = PULSE_START, false
    s.trace, s.trace_mode = 0, 0
    s.target_pulse, s.target_falling, s.target_shown = TARGET_START, false, false
    s.menu_item = 0
    emit(api)
    refresh(api)
end

function M.tick(api)
    emit(api)
    refresh(api)
end

function M.update(api, _input)
    emit(api)
    refresh(api)
end

function M.tick_monitor(_api) end

function M.view_closed(_api) end

function M.reselect(api)
    refresh(api)
end

function M.move_cursor(api, _cursor)
    refresh(api)
end

function M.present(_api, show_cursor, show_name, _backdrop)
    s.show_cursor = show_cursor
    s.show_name = show_name
end

local function shown()
    return s.open >= OPEN_SHOWN and (s.close == nil or s.close < CLOSE_BLACK)
end

-- Whether the page's logic draws COMBN's target cursor on a tick that
-- starts at `page`: the logic runs the step the last tick left.
local function draws_target(page)
    if page == nil or page.sub ~= SUB_COMMAND or page.command ~= COMMAND_COMBINE then
        return false
    end
    return page.step == STEP_TARGET or page.step == STEP_QUESTION
        or (page.step == STEP_SLIDE and page.count < SLIDE_TICKS)
end

-- One tick of the original's drawing state: the cursors' pulses and the
-- trace step on the frames the screen is drawn.
function M.animate(api, open, close)
    s.open, s.close = open, close
    s.last = s.page
    s.page = api:re2_items()
    s.target_shown = false
    if not shown() or s.page == nil then
        return
    end
    local page = s.page
    if draws_target(s.last) then
        s.target_shown = true
        if s.target_falling then
            s.target_falling = s.target_pulse >= TARGET_LOW
            s.target_pulse = s.target_pulse - PULSE_STEP
        else
            s.target_falling = s.target_pulse > TARGET_HIGH
            s.target_pulse = s.target_pulse + PULSE_STEP
        end
    end
    if page.page == PAGE_ITEMS and page.sub == SUB_GRID then
        if s.pulse_falling then
            s.pulse_falling = s.pulse >= PULSE_LOW
            s.pulse = s.pulse - PULSE_STEP
        else
            s.pulse_falling = s.pulse > PULSE_HIGH
            s.pulse = s.pulse + PULSE_STEP
        end
    else
        s.pulse = PULSE_HELD
    end
    -- The menu's labels are set when it opens, from the item then under
    -- the cursor.
    if page.sub ~= SUB_GRID and (s.last == nil or s.last.sub == SUB_GRID) then
        s.menu_item = api:re2_slot(page.cursor)
    end
    -- A heal or a cure restarts the trace once its sweep is done.
    if page.panel_mode ~= 0 then
        s.trace_mode = page.panel_mode
        s.trace = 0
    else
        s.trace_mode = 0
        s.trace = (s.trace + 1) % 0x80
    end
end

local function min(a, b)
    return a < b and a or b
end

local function floor(x)
    return x // 1 | 0
end

local function art(api)
    if s.art then
        return s.art
    end
    local sheet = api:character() == 1 and 1 or 0
    local pages = {}
    for k = 0, 7 do
        pages[ST + k] = api:page(("ui/status/st%d_jp_%d"):format(sheet, k))
    end
    for k = 0, 2 do
        pages[ICONS + k] = api:page(("ui/status/itemall_%d"):format(k))
    end
    s.art = { pages = pages, checks = {} }
    return s.art
end

-- The PlayStation's `rgb / 128` modulation as a tint.
local function tint(rgb)
    if rgb == nil then
        return nil
    end
    return ("#%02X%02X%02X"):format(
        min(255, rgb[1] * 2),
        min(255, rgb[2] * 2),
        min(255, rgb[3] * 2)
    )
end

local function sprite(api, d, texture, x, y, w, h, u, v, rgb, clut)
    local handle = d.pages[texture]
    if handle == nil then
        return
    end
    api:page_sprite_scaled(handle, u, v, w, h, d.ox + x, d.oy + y, w, h, clut or 0, 255, tint(rgb))
end

local function pieces(api, d, list, dx, dy)
    for _, p in ipairs(list) do
        sprite(api, d, ST, p[1] + dx, p[2] + dy, p[3], p[4], p[5], p[6])
    end
end

local function fill(api, d, x0, y0, x1, y1, rgb)
    api:fill_rect(d.ox + x0, d.oy + y0, x1 - x0 + 1, y1 - y0 + 1, rgb[1], rgb[2], rgb[3])
end

-- An icon's cell: 48 a page, six columns of 40x30 (three on the last).
local function icon_cell(item)
    local page, cell = item // 48, item % 48
    local columns = page == 2 and 3 or 6
    return ICONS + page, 40 * (cell % columns), 30 * (cell // columns)
end

-- The right half of a two-slot weapon (ids 0x0E-0x12, on an odd slot) is
-- its own cell on the last page.
local function half_cell(item)
    local cell = item % 48 - 10
    return ICONS + 2, 40 * (cell % 3), 30 * (cell // 3)
end

local function is_wide(item)
    return item // 48 == 0 and item % 48 >= 14 and item % 48 <= 18
end

local function slot_icon(api, d, slot, x, y)
    local item = api:re2_slot(slot)
    local texture, u, v = icon_cell(item)
    if slot % 2 == 1 and is_wide(item) then
        texture, u, v = half_cell(item)
    end
    sprite(api, d, texture, x, y, SLOT_W, SLOT_H, u, v)
end

-- A count (`0x4FE3B0`): its digits from the left, no leading zeros, then
-- a percent sign; nothing for an item that shows no count.
local function count(api, d, item, quantity, x, y)
    local flags = COUNT_FLAGS[item] or 0x04
    local clut = flags & 0x0F
    if clut >= 4 then
        return
    end
    local glyphs = {}
    if quantity == INFINITE and flags & 0x10 ~= 0 then
        glyphs = { 11, 12 }
    else
        local hundreds, tens, units = quantity // 100, quantity // 10 % 10, quantity % 10
        if hundreds ~= 0 then
            glyphs[#glyphs + 1] = hundreds
        end
        if hundreds ~= 0 or tens ~= 0 then
            glyphs[#glyphs + 1] = tens
        end
        glyphs[#glyphs + 1] = units
        if flags & 0x80 ~= 0 then
            glyphs[#glyphs + 1] = 10
        end
    end
    for k, glyph in ipairs(glyphs) do
        local u, v = 8 * glyph, 38
        if glyph >= 8 then
            u, v = 8 * (glyph - 8), 47
        end
        sprite(api, d, ST + 2, x + 7 * (k - 1), y, 8, 9, u, v, nil, clut)
    end
end

local function equipped_slot(api)
    local item = api:equipped()
    if item == 0 then
        return nil
    end
    for slot = 0, s.slots - 1 do
        if api:re2_slot(slot) == item then
            return slot
        end
    end
    return nil
end

local function cursor_at(slot)
    if slot == PERSONAL_SLOT then
        return PERSONAL_CURSOR_X, PERSONAL_CURSOR_Y
    end
    return CURSOR_X + SLOT_W * (slot % 2), CURSOR_Y + SLOT_H * (slot // 2)
end

local function is_check(page)
    return page.sub == SUB_COMMAND and page.command == COMMAND_CHECK
end

local function is_target(page)
    return page.sub == SUB_COMMAND and page.command == COMMAND_COMBINE and page.step == 0
end

-- CHECK's moves by its step and count: the message box slides down and
-- out, zigzagging, while the picture panel rises over the grid, and back.
local CHECK_BOX = {
    [1] = function(c) return -6 * c, 8 * c end,
    [2] = function(c) return -30 + 6 * c, 40 + 8 * c end,
    [3] = function(_) return 0, 80 end,
    [4] = function(c) return -6 * c, 80 - 8 * c end,
    [5] = function(c) return -30 + 6 * c, 40 - 8 * c end,
}
local function check_box(page)
    local step = page.step
    if step > 7 then
        step = step - 7
    end
    local move = CHECK_BOX[step]
    if move == nil or page.step == 7 or page.step == CHECK_DONE then
        return 0, 0
    end
    return move(page.count)
end

local CHECK_BOTTOM, CHECK_TOP, CHECK_RISE = 258, 78, 15
local function check_panel_y(page)
    local step, c = page.step, page.count
    if step == 3 then
        return CHECK_BOTTOM - CHECK_RISE * c
    elseif step == 4 then
        return CHECK_BOTTOM - CHECK_RISE * (7 + c)
    elseif step >= 5 and step <= 9 then
        return CHECK_TOP
    elseif step == 10 then
        return CHECK_TOP + CHECK_RISE * c
    elseif step == 11 then
        return CHECK_TOP + CHECK_RISE * (7 + c)
    end
    return CHECK_BOTTOM
end

local function check_menu_x(page)
    local ticks
    if page.step == 0 then
        return MENU_X
    elseif page.step == 1 then
        ticks = page.count
    elseif page.step == 2 then
        ticks = 5 + page.count
    elseif page.step == 3 then
        ticks = 10 + page.count
    else
        return CHECK_MENU_X
    end
    return min(CHECK_MENU_X, MENU_X + CHECK_MENU_STEP * (ticks - 1))
end

-- Where the command menu is drawn this tick, or nil. The tick the menu
-- shuts, and the tick CHECK ends, still draw it where it was going.
local function menu_x(page, last)
    if page.sub == SUB_OPEN then
        if page.command == 0 then
            return nil
        end
        return MENU_SHUT - MENU_STEP * page.command
    elseif page.sub == SUB_CLOSE then
        return MENU_X + MENU_STEP * page.command
    elseif page.sub == SUB_MENU then
        return MENU_X
    elseif page.sub == SUB_COMMAND then
        if page.command == COMMAND_CHECK then
            return check_menu_x(page)
        end
        return MENU_X
    elseif last ~= nil and page.sub == SUB_GRID then
        if last.sub == SUB_CLOSE then
            return MENU_SHUT
        elseif is_check(last) and last.step == CHECK_DONE then
            return CHECK_MENU_X
        elseif last.sub == SUB_COMMAND then
            return MENU_X
        end
    end
    return nil
end

local function draw_menu(api, d, page, x)
    for _, p in ipairs(MENU_PIECES) do
        sprite(api, d, ST, x, p[1], p[2], p[3], p[4], p[5])
    end
    -- The labels are the item's the menu opened on (a reload's slide moves
    -- the cursor onto the weapon under USE): EQUIP for the weapons, 1 to
    -- 0x13, USE for the rest.
    local item = s.menu_item
    local first = (item ~= 0 and item < FIRST_AMMO) and LABEL_EQUIP or LABEL_USE
    local lit = page.command_cursor
    sprite(api, d, ST + 7, x + 4, 126, 48, 16, 0, 16)
    sprite(api, d, ST + 6, x + 4, 126, 48, 16, 0, LABEL_COMBINE, lit == 1 and LABEL_LIT or nil)
    sprite(api, d, ST + 6, x + 4, 105, 48, 16, 0, LABEL_CHECK, lit == 2 and LABEL_LIT or nil)
    sprite(api, d, ST + 6, x + 4, 84, 48, 16, 0, first, lit == 3 and LABEL_LIT or nil)
end

local function check_panel(api, d, y)
    fill(api, d, 7, y, 215, y + 87, CHECK_FILL)
    sprite(api, d, ST, 7, y, 83, 4, 0, 147)
    sprite(api, d, ST, 90, y - 8, 128, 12, 0, 72)
    sprite(api, d, ST, 7, y + 85, 83, 4, 0, 151)
    sprite(api, d, ST, 90, y + 85, 128, 4, 0, 84)
    sprite(api, d, ST, 7, y + 4, 5, 82, 123, 92)
    sprite(api, d, ST, 213, y + 4, 5, 82, 123, 174)
end

local function check_picture(api, d, page, y)
    local item = api:re2_slot(page.cursor)
    local pictures = d.checks
    if pictures[item] == nil then
        pictures[item] = api:page(("ui/status/check_%02x"):format(item)) or false
    end
    local handle = pictures[item]
    if handle then
        api:page_sprite_scaled(handle, 0, 0, 112, 72, d.ox + 57, d.oy + y + 10, 112, 72, 0, 255)
    end
end

-- The condition (`0x502530`): poisoned, or by the life left against the
-- player's `Max_life` (`0x98A052`), 200 for both characters.
local MAX_LIFE = 200
local function condition(api, page)
    local health = api:player_health()
    if page.poisoned then
        return 4
    elseif health > MAX_LIFE // 2 then
        return 0
    elseif health > 40 then
        return 1
    elseif health > 20 then
        return 2
    end
    return 3
end

local function trace_line(api, d, x, y0, y1, r, g, b)
    local w = TRACE_WEIGHT
    api:fill_rect(d.ox + x, d.oy + y0, 1, y1 - y0 + 1,
        floor(r * w), floor(g * w), floor(b * w), TRACE_ALPHA)
end

-- The trace (`0x4FFAB0`): the idle wave's last 32 columns fading behind its
-- head, or a heal's or a cure's sweep.
local function draw_trace(api, d, page, cond)
    local mode = s.trace_mode
    if mode == 1 or mode == 3 then
        for i = TRACE_TAIL - 1, 0, -1 do
            local at = page.panel_sweep - i
            if at >= HEAL_FIRST and at <= HEAL_LAST then
                local fade = 0xFF - 8 * i
                local r = mode == 3 and fade or 0x10
                trace_line(api, d, PANEL_X + at, PANEL_Y + HEAL_TOP, PANEL_Y + HEAL_BOTTOM, r, fade, 0x10)
            end
        end
        return
    elseif mode == 2 then
        for i = TRACE_TAIL - 1, 0, -1 do
            local at = page.panel_cure - i
            if at >= CURE_FIRST and at <= CURE_LAST then
                api:fill_rect(d.ox + PANEL_X + CURE_LEFT, d.oy + PANEL_Y + at,
                    CURE_RIGHT - CURE_LEFT + 1, 1,
                    floor(0x10 * TRACE_WEIGHT), floor(0x10 * TRACE_WEIGHT),
                    floor(8 * i * TRACE_WEIGHT), TRACE_ALPHA)
            end
        end
        return
    end
    local colour, wave = TRACE_COLOURS[cond], WAVES[cond]
    for i = TRACE_TAIL - 1, 0, -1 do
        local at = s.trace - i
        if at >= 0 and at < TRACE_LEN then
            local y0 = wave[2 * at + 1] + PANEL_Y + 2
            local y1 = y0 + wave[2 * at + 2]
            trace_line(api, d, PANEL_X + 12 + at, y0, y1,
                colour[1] - i * colour[4], colour[2] - i * colour[5], colour[3] - i * colour[6])
        end
    end
end

-- A slide's step a tick (`0x502B30`), 3 a tick down a row. The item's
-- cursor crosses a column at 4 a tick, 2 onto a two-slot weapon.
local function cursor_step(api, cursor, target)
    local dx
    if is_wide(api:re2_slot(target)) then
        dx = cursor % 2 == 1 and -2 or 2
    else
        dx = 4 * (target % 2 - cursor % 2)
    end
    return dx, 3 * (target // 2 - cursor // 2)
end

-- The target's cursor, toward the item's: the parity test overwrites the
-- flags of the two-slot test, so an odd target always steps 2 left, half
-- way when the item is left of it, and an even one 4 right under an odd
-- item.
local function target_step(cursor, target)
    local dx
    if target % 2 == 1 then
        dx = -2
    else
        dx = cursor % 2 == 1 and 4 or 0
    end
    return dx, 3 * (cursor // 2 - target // 2)
end

-- The cursors' offsets in COMBN's slide: the item's cursor moves onto the
-- target when it follows it, the target's cursor onto the item when not.
local function slide(api, page)
    if page.sub ~= SUB_COMMAND or page.command ~= COMMAND_COMBINE or page.step ~= STEP_SLIDE then
        return 0, 0, 0, 0
    end
    local c = page.count
    if page.follow then
        local dx, dy = cursor_step(api, page.cursor, page.target)
        return dx * c, dy * c, 0, 0
    end
    local dx, dy = target_step(page.cursor, page.target)
    return 0, 0, dx * c, dy * c
end

local function draw_cursor(api, d, slot, grey, dx, dy)
    local x, y = cursor_at(slot)
    x, y = x + dx, y + dy
    local rgb = { grey, grey, grey }
    sprite(api, d, ST + 3, x, y, 22, 34, 0, 30, rgb)
    sprite(api, d, ST + 3, x, y, 40, 34, 2, 30, rgb)
    sprite(api, d, ST + 3, x + 22, y, 22, 34, 22, 30, rgb)
end

local function draw_target(api, d, slot, grey, dx, dy)
    local x, y = cursor_at(slot)
    x, y = x + dx, y + dy
    local rgb = { grey, grey, grey }
    sprite(api, d, ST + 3, x + 2, y + 2, 22, 30, 0, 0, rgb)
    sprite(api, d, ST + 3, x + 22, y + 2, 22, 30, 20, 0, rgb)
end

function M.draw(api)
    local page = s.page or api:re2_items()
    if page == nil then
        return
    end
    local last = s.last
    local d = art(api)
    d.ox, d.oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:fill_rect(0, 0, api:width(), api:height(), 8, 8, 32)
    local closing = s.close ~= nil and s.close >= 1
    local on_items = page.page == PAGE_ITEMS
    -- The item page's logic draws the cursor and the name, on the page the
    -- tick started on.
    local ran_items = (last or page).page == PAGE_ITEMS

    -- The command menu, under the frame.
    local x = on_items and menu_x(page, last) or nil
    if x ~= nil then
        draw_menu(api, d, page, x)
    end

    -- The frame, the message box (moved by CHECK) and CHECK's panel.
    pieces(api, d, FRAME_TABS, 0, 0)
    local check = on_items and (is_check(page) or (last ~= nil and page.sub == SUB_GRID
        and is_check(last) and last.step == CHECK_DONE))
    local bx, by = 0, 0
    if on_items and is_check(page) then
        bx, by = check_box(page)
    end
    fill(api, d, MESSAGE_BOX[1] + bx, MESSAGE_BOX[2] + by, MESSAGE_BOX[3] + bx, MESSAGE_BOX[4] + by, MESSAGE_FILL)
    pieces(api, d, MESSAGE_FRAME, bx, by)
    pieces(api, d, FRAME, 0, 0)
    local panel_y = check and (is_check(page) and check_panel_y(page) or CHECK_BOTTOM) or nil
    if panel_y ~= nil then
        check_panel(api, d, panel_y)
    end

    -- The tabs.
    for tab = 0, 3 do
        sprite(api, d, ST + 5, TAB_X + 49 * tab, TAB_Y, TAB_W, TAB_H, 0, TAB_H * tab,
            tab ~= page.tab and TAB_DIM or page.page >= PAGE_ITEMS and TAB_LIT or TAB_SELECTED)
    end

    -- The portrait, the condition and the slots' icons.
    for _, p in ipairs(PORTRAIT) do
        sprite(api, d, ST + 1, p[1], p[2], p[3], p[4], p[5], p[6])
    end
    local cond = condition(api, page)
    sprite(api, d, ST + 4, CONDITION_X, CONDITION_Y, 46, 11, 0, 11 * cond)
    slot_icon(api, d, PERSONAL_SLOT, PERSONAL_X, PERSONAL_Y)
    for slot = s.slots - 1, 0, -1 do
        slot_icon(api, d, slot, GRID_X + SLOT_W * (slot % 2), GRID_Y + SLOT_H * (slot // 2))
    end

    -- The equipped item and the counts.
    sprite(api, d, ICONS + 2, EQUIP_BOX[1], EQUIP_BOX[2], EQUIP_BOX[3], EQUIP_BOX[4], EQUIP_BOX[5], EQUIP_BOX[6])
    local equip = equipped_slot(api)
    if equip ~= nil then
        local item, quantity = api:re2_slot(equip)
        local texture, u, v = icon_cell(item)
        if is_wide(item) then
            sprite(api, d, texture, EQUIP_X - 20, EQUIP_Y, SLOT_W, SLOT_H, u, v)
            local half, hu, hv = half_cell(item)
            sprite(api, d, half, EQUIP_X + 20, EQUIP_Y, SLOT_W, SLOT_H, hu, hv)
        else
            sprite(api, d, texture, EQUIP_X, EQUIP_Y, SLOT_W, SLOT_H, u, v)
        end
        count(api, d, item, quantity, EQUIP_COUNT_X, EQUIP_COUNT_Y)
    end
    for slot = 0, s.slots - 1 do
        local item, quantity = api:re2_slot(slot)
        local at = COUNT_AT[slot]
        if item ~= 0 and not (slot % 2 == 1 and is_wide(item)) then
            count(api, d, item, quantity, COUNT_ORIGIN_X + at[1], COUNT_ORIGIN_Y + at[2])
        end
    end

    -- The trace, the cursor and CHECK's picture.
    draw_trace(api, d, page, cond)
    local cursor_shown = s.show_cursor and not closing and ran_items
    local cx, cy, tx, ty = slide(api, page)
    if cursor_shown then
        draw_cursor(api, d, page.cursor, s.pulse, cx, cy)
    end
    if panel_y ~= nil then
        check_picture(api, d, page, panel_y)
    end

    -- COMBN's target and the item's name follow the last tick's page.
    local shows = last or page
    if cursor_shown and s.target_shown then
        draw_target(api, d, page.target, s.target_pulse, tx, ty)
    end
    if s.show_name and not closing and ran_items then
        local slot = nil
        if shows.sub == SUB_GRID and not (last ~= nil and last.sub ~= SUB_GRID) then
            slot = page.cursor
        elseif is_target(shows) then
            slot = page.target
        end
        local item = slot and api:re2_slot(slot) or 0
        if item ~= 0 then
            api:glyphs(d.ox + NAME_X, d.oy + NAME_Y, api:item_name_bytes(item) or "")
        end
    end
end

return M
