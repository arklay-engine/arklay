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
-- The item names and the messages are drawn in the original's font (the
-- pack's font 00, converted from `font0.tim`), at the original's place.
--
-- It keeps the engine's contract with the pause menu: `open`, `tick`,
-- `update`, `tick_monitor`, `view_closed`, `reselect`, `move_cursor`,
-- `present` and `draw`, publishing `cursor`, `slot` and `selected_item`, and
-- `event` and `event_id`.

local M = {}

local EVENT_NONE = 0
local PERSONAL_SLOT = 10
local PAGE_TABS, PAGE_ITEMS, PAGE_MAP, PAGE_FILE = 2, 3, 4, 5
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
-- The first `FRAME_LEFT` pieces are the left column's, the rest the
-- right's.
local FRAME_LEFT = 11
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
-- The item box's EXIT, dim off it.
local TAB_DIM_BOX = { 64, 64, 64 }

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

-- The cursor (texture 0x15): three pieces over a slot, grey: the screen's
-- blink (`re2_items().blink`), which the engine steps (`0x4FE5F0`: up by 3
-- a frame until past 120, then down by 3 until under 50; held at 60 under
-- the menu). The item box's inventory cursor pulses here, `s.pulse`.
local CURSOR_X, CURSOR_Y = 223, 71
local PERSONAL_CURSOR_X, PERSONAL_CURSOR_Y = 263, 33
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

-- The item's name, where the message box's first line starts: its ink from
-- x 16, its cells' tops at y 175.
local NAME_X, NAME_Y = 16, 175

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

-- The MAP page (`0x4FA240`): it slides the item page's frame out over 14
-- ticks, its pieces in four groups (the tabs up, the message box down, the
-- left column left and the right column right, by these steps a tick),
-- over a tiled backdrop (texture 0x13), and draws the floor's map once it
-- is shown (`0x4FA990`).
local MAP_IN, MAP_OUT, MAP_SHOWN, MAP_FLOOR = 1, 2, 3, 4
local MAP_TICKS = 14
local SLIDE_TABS, SLIDE_BOX, SLIDE_LEFT, SLIDE_RIGHT = -19, 21, -22, 21
local TILE_U, TILE_V, TILE, TILE_COLUMNS, TILE_ROWS = 80, 22, 48, 7, 5
-- The map's pictures by their texture slots: the five TIMs of
-- `map<nn>.adt`, in order.
local MAP_SLOTS = { 0x1D, 0x21, 0x20, 0x1E, 0x1F }
local SIGNS = 0x1E
local SIGN_PIECES = {
    { 10, 16, 96, 32, 0, 0 },
    { 250, 21, 32, 48, 96, 0 },
}
-- The player's marker (texture 0x14, translucent), its colour the blink
-- (`0x691F88`, its way at `0x691F89`) plus 40: the engine steps it up by 2
-- a tick until past 0x50, then down by 2 until under 10, on each tick the
-- shown map's logic runs. It is the item cursor's pulse too.
local MARKER, MARKER_SIZE, MARKER_BASE = 0x14, 12, 40

-- The FILE page (`0x4F95B0`): the message box drops out of the screen as a
-- taller panel rises over the left column's lower half, for 12 ticks in and
-- 13 out. The panel's pieces (texture 0x12), from its top edge.
local FILE_IN, FILE_OUT, FILE_SHOWN = 1, 2, 3
local FILE_BOX, FILE_TOP, FILE_RISE = 182, 78, 286
local FILE_OUT_TICKS, FILE_BOX_STEP, FILE_TOP_STEP = 13, 14, 13
local FILE_PANEL = {
    { 7, 0, 83, 4, 0, 147 },
    { 90, -8, 128, 12, 0, 72 },
    { 7, 145, 83, 4, 0, 151 },
    { 90, 145, 128, 4, 0, 84 },
    { 7, 4, 5, 82, 123, 92 },
    { 213, 4, 5, 82, 123, 174 },
    { 7, 86, 5, 59, 123, 92 },
    { 213, 86, 5, 59, 123, 174 },
    { 218, 132, 8, 14, 107, 242 },
}
for k = 0, 7 do
    FILE_PANEL[#FILE_PANEL + 1] = { 226 + 16 * k, 134, 16, 9, 80, 155 }
end

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

function M.open(api, args)
    -- The item box's screen: the engine runs its page (`src/game/itembox2.rs`).
    if args ~= nil and args.mode == "box" then
        s.box = true
        api:set("canvas", 1)
        s.slots = api:inventory_capacity()
        s.page, s.last = nil, nil
        s.open, s.close = 0, nil
        s.pulse, s.pulse_falling = PULSE_START, false
        emit(api)
        return
    end
    -- A script's map (`Sce_84`, the status task's mode 4): the engine runs
    -- its page (`src/game/status2/script_map.rs`).
    if args ~= nil and args.mode == "map" then
        s.script_map = true
        api:set("canvas", 1)
        s.open, s.close = 0, nil
        emit(api)
        return
    end
    -- The get-item window draws with this script's art but runs no page.
    if args ~= nil and args.mode == "get" then
        s.get = true
        s.slots = api:inventory_capacity()
        s.trace, s.trace_mode = 0, 0
        return
    end
    api:set("canvas", 1)
    s.slots = api:inventory_capacity()
    s.page, s.last = nil, nil
    s.open, s.close = 0, nil
    s.trace, s.trace_mode = 0, 0
    s.target_pulse, s.target_falling, s.target_shown = TARGET_START, false, false
    s.menu_item = 0
    emit(api)
    refresh(api)
end

function M.tick(api)
    emit(api)
    if not s.box and not s.script_map then
        refresh(api)
    end
end

function M.update(api, _input)
    emit(api)
    if not s.box and not s.script_map then
        refresh(api)
    end
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
-- One tick of the item box's drawing state: the inventory cursor's pulse,
-- which `St_disp_cursol0` steps on each tick the page's routine 3 runs, on
-- the inventory and held at 60 off it.
local function animate_box(api)
    s.last = s.page
    s.page = api:re2_box()
    local last = s.last
    if s.page == nil or last == nil or not last.started or last.routine ~= 3 then
        return
    end
    if s.page.sub == 0 then
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
end

function M.animate(api, open, close)
    s.open, s.close = open, close
    if s.box then
        animate_box(api)
        return
    end
    if s.script_map then
        return
    end
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

-- One tick of the get-item window's drawing state, by the window's ticks
-- since its first and in its close: the trace steps on the ticks its
-- screen is drawn, as the status screen's does.
function M.animate_get(_api, open, close)
    s.open, s.close = open, close
    if shown() then
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
    -- The original culls a line left of the screen.
    if x < 0 then
        return
    end
    local w = TRACE_WEIGHT
    api:fill_rect(d.ox + x, d.oy + y0, 1, y1 - y0 + 1,
        floor(r * w), floor(g * w), floor(b * w), TRACE_ALPHA)
end

-- The trace (`0x4FFAB0`): the idle wave's last 32 columns fading behind its
-- head, or a heal's or a cure's sweep.
local function draw_trace(api, d, page, cond, dx)
    local mode = s.trace_mode
    local panel_x = PANEL_X + dx
    if mode == 1 or mode == 3 then
        for i = TRACE_TAIL - 1, 0, -1 do
            local at = page.panel_sweep - i
            if at >= HEAL_FIRST and at <= HEAL_LAST then
                local fade = 0xFF - 8 * i
                local r = mode == 3 and fade or 0x10
                trace_line(api, d, panel_x + at, PANEL_Y + HEAL_TOP, PANEL_Y + HEAL_BOTTOM, r, fade, 0x10)
            end
        end
        return
    elseif mode == 2 then
        for i = TRACE_TAIL - 1, 0, -1 do
            local at = page.panel_cure - i
            if at >= CURE_FIRST and at <= CURE_LAST then
                api:fill_rect(d.ox + panel_x + CURE_LEFT, d.oy + PANEL_Y + at,
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
            trace_line(api, d, panel_x + 12 + at, y0, y1,
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

-- How far the MAP page has slid the item page's frame out, 0 to 14.
local function map_slide(page)
    if page.page ~= PAGE_MAP then
        return 0
    elseif page.sub == MAP_IN then
        return page.command
    elseif page.sub == MAP_SHOWN or page.sub == MAP_FLOOR then
        return MAP_TICKS
    elseif page.sub == MAP_OUT then
        return MAP_TICKS - page.command
    end
    return 0
end

-- How far the FILE page has moved the message box down, and where its
-- panel's top is (nil when it isn't drawn). The tick the slide out ends,
-- back on the tab bar, still draws its last step.
local function file_slide(page, last)
    if page.page == PAGE_FILE then
        local c = page.command
        if page.sub == FILE_IN then
            return c * c + 3 * c, FILE_RISE - c * c - 5 * c
        elseif page.sub == FILE_OUT then
            return FILE_BOX - FILE_BOX_STEP * c, FILE_TOP + FILE_TOP_STEP * c
        elseif page.sub >= FILE_SHOWN then
            return FILE_BOX, FILE_TOP
        end
    elseif last ~= nil and last.page == PAGE_FILE and last.sub == FILE_OUT then
        local c = FILE_OUT_TICKS
        return FILE_BOX - FILE_BOX_STEP * c, FILE_TOP + FILE_TOP_STEP * c
    end
    return 0, nil
end

-- FILE's three binders (`0x500470`) and their labels: 3D models from
-- `st_file.ts`, which the engine projects as the original does, in the
-- panel whose top is `top`. `shown` is the page whose binders these are.
local function file_binders(api, d, shown, top)
    if d.binders == nil then
        d.binders = api:re2_binders_model() or false
    end
    if d.binders then
        api:draw_re2_binders(d.binders, shown, top)
    end
end

-- The reader's subs: 10 lists the binder's files, 0xB moves the screen's
-- parts out and 0xC back, 0xD to 0x12 show the page.
local FILE_LIST, FILE_READ_IN, FILE_READ_OUT, FILE_PAGE, FILE_PREV_IN = 10, 0xB, 0xC, 0xD, 0x12
-- The panel's fall a step of the reader's slide (`0x691FAE`).
local FILE_FALL = 22
-- The list's name of the file in the binder's slot (`St_disp_name`).
local FILE_NAME_X, FILE_NAME_Y, FILE_NAMES = 16, 203, 0x68

-- How far a file's opening has moved the screen's parts out, 0 to 14, on
-- the moves MAP's slide makes, and the panel down by `FILE_FALL` a step:
-- sub 0xB's count, and 0xC's 14 less its count (it steps before the draw).
local function reader_slide(page)
    if page.page ~= PAGE_FILE then
        return 0
    elseif page.sub == FILE_READ_IN then
        return page.command
    elseif page.sub == FILE_READ_OUT then
        return MAP_TICKS - page.command
    end
    return 0
end

-- Whether `page` is in the reader's subs from `first` to 0x12.
local function reading(page, first)
    return page ~= nil and page.page == PAGE_FILE and page.sub >= first and page.sub <= FILE_PREV_IN
end

-- File `file`'s chunk `k` (`ui/status/file<nn>_<k>`): 1 the picture, `2 +
-- page` a page.
local function file_chunk(api, d, file, k)
    d.files = d.files or {}
    local chunks = d.files[file]
    if chunks == nil then
        chunks = {}
        d.files[file] = chunks
    end
    if chunks[k] == nil then
        chunks[k] = api:page(("ui/status/file%02x_%d"):format(file, k)) or false
    end
    return chunks[k] or nil
end

-- A file's page (`St_disp_file`, `St_disp_file2`): the sprites the engine
-- lists, in paint order, the picture and the page from the file's chunks
-- and the arrows and the end marker from the sheets.
local function draw_file_page(api, d, file, sprites)
    for _, p in ipairs(sprites) do
        local handle
        if p.chunk ~= nil then
            handle = file_chunk(api, d, file, p.chunk)
        else
            handle = d.pages[ST + p.sheet]
        end
        if handle ~= nil then
            local c = p.colour
            local rgb = c ~= 0x80 and { c, c, c } or nil
            api:page_sprite_scaled(handle, p.u, p.v, p.w, p.h, d.ox + p.x, d.oy + p.y, p.w, p.h, 0, 255, tint(rgb))
        end
    end
end

local function map_art(api, d, number)
    d.maps = d.maps or {}
    local pages = d.maps[number]
    if pages == nil then
        pages = {}
        for k, slot in ipairs(MAP_SLOTS) do
            pages[slot] = api:page(("ui/status/map%02d_%d"):format(number, k - 1))
        end
        d.maps[number] = pages
    end
    return pages
end

-- A translucent sprite (`0x440280` with the primitive's semi-transparency
-- on): every texel is averaged with the frame, the STP bit or not; colour
-- 0 is clear. `rgb` modulates it.
local function translucent(api, d, handle, row, rgb, x, y, w, h, u, v)
    if handle == nil then
        return
    end
    d.luts = d.luts or {}
    local key = ("%d:%d:%d"):format(handle, row, rgb or 128)
    local lut = d.luts[key]
    if lut == nil then
        local palette = api:page_palette(handle, row) or {}
        local stp = api:page_stp(handle, row) or {}
        local m = rgb or 128
        lut = {}
        for index = 1, 255 do
            local c = palette[index]
            if c ~= nil and (c ~= 0 or stp[index]) then
                local r = min(255, ((c >> 16) & 0xFF) * m // 128)
                local g = min(255, ((c >> 8) & 0xFF) * m // 128)
                local b = min(255, (c & 0xFF) * m // 128)
                lut[index] = (r << 16) | (g << 8) | b
            end
        end
        d.luts[key] = lut
    end
    api:page_lut(handle, u, v, w, h, d.ox + x, d.oy + y, lut, false, { blend = "half" })
end

-- The floor's map (`0x4FA990`): the rooms' fills, the signs, the rooms'
-- outlines and the player's marker, in the original's order.
local function draw_map(api, d, view)
    view = view or api:re2_map()
    if view == nil then
        return
    end
    local pages = map_art(api, d, view.map)
    for _, room in ipairs(view.rooms) do
        local fill = room.fill
        if fill ~= nil then
            translucent(api, d, pages[fill.texture], fill.clut, nil,
                room.x, room.y, room.w, room.h, fill.u, fill.v)
        end
    end
    for _, p in ipairs(SIGN_PIECES) do
        local handle = pages[SIGNS]
        if handle ~= nil then
            api:page_sprite_scaled(handle, p[5], p[6], p[3], p[4], d.ox + p[1], d.oy + p[2], p[3], p[4], 0, 255)
        end
    end
    for _, room in ipairs(view.rooms) do
        local o = room.outline
        local handle = pages[o.texture]
        if handle ~= nil then
            api:page_sprite_scaled(handle, o.u, o.v, room.w, room.h, d.ox + room.x, d.oy + room.y,
                room.w, room.h, o.clut, 255)
        end
    end
    local marker = view.marker
    if marker ~= nil then
        local blink = s.page and s.page.blink or PULSE_START
        translucent(api, d, d.pages[MARKER], 0, blink + MARKER_BASE,
            marker.x, marker.y, MARKER_SIZE, MARKER_SIZE, marker.u, marker.v)
    end
end

-- The get-item window (`0x4FD560`'s draw list): the item page's frame,
-- portrait, slots and trace, CHECK's panel at the window's origin with the
-- item's still picture (`0x4FFF70`) and, while it flies, the picture's copy
-- shrinking onto the slot, under the still. No tab is lit and no cursor or name is drawn.
local GET_X, GET_Y = 7, 78
local GET_PICTURE = { 0x10, 0x0A, 0x70, 0x48 }

local function picture(api, d, item, x, y, w, h)
    local pictures = d.checks
    if pictures[item] == nil then
        pictures[item] = api:page(("ui/status/check_%02x"):format(item)) or false
    end
    local handle = pictures[item]
    if handle then
        api:page_sprite_scaled(handle, 0, 0, 112, 72, d.ox + x, d.oy + y, w, h, 0, 255)
    end
end

local function draw_get(api)
    local get = api:re2_item_get()
    if get == nil then
        return
    end
    local d = art(api)
    d.ox, d.oy = api:width() // 2 - 160, api:height() // 2 - 120
    -- A file draws its page on the engine's black, with no item screen.
    if get.phase == "file_fade" or get.phase == "file_read" then
        if get.file_page ~= nil then
            draw_file_page(api, d, get.file, get.file_page)
        end
        return
    end
    api:fill_rect(0, 0, api:width(), api:height(), 8, 8, 32)
    pieces(api, d, FRAME_TABS, 0, 0)
    fill(api, d, MESSAGE_BOX[1], MESSAGE_BOX[2], MESSAGE_BOX[3], MESSAGE_BOX[4], MESSAGE_FILL)
    pieces(api, d, MESSAGE_FRAME, 0, 0)
    pieces(api, d, FRAME, 0, 0)
    check_panel(api, d, GET_Y)
    for tab = 0, 3 do
        sprite(api, d, ST + 5, TAB_X + 49 * tab, TAB_Y, TAB_W, TAB_H, 0, TAB_H * tab, TAB_DIM)
    end
    for _, p in ipairs(PORTRAIT) do
        sprite(api, d, ST + 1, p[1], p[2], p[3], p[4], p[5], p[6])
    end
    local page = { poisoned = get.poisoned }
    local cond = condition(api, page)
    sprite(api, d, ST + 4, CONDITION_X, CONDITION_Y, 46, 11, 0, 11 * cond)
    slot_icon(api, d, PERSONAL_SLOT, PERSONAL_X, PERSONAL_Y)
    for slot = s.slots - 1, 0, -1 do
        slot_icon(api, d, slot, GRID_X + SLOT_W * (slot % 2), GRID_Y + SLOT_H * (slot // 2))
    end
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
    draw_trace(api, d, page, cond, 0)
    -- The still picture covers its flying copy.
    local fly = get.fly
    if fly ~= nil then
        picture(api, d, get.item, GET_X + fly.x, GET_Y + fly.y, fly.w, fly.h)
    end
    local p = GET_PICTURE
    picture(api, d, get.item, GET_X + p[1], GET_Y + p[2], p[3], p[4])
end

-- The item box's screen (`St_main01`'s draw list): the box's panel
-- (`St_disp_itembox`, `0x4FCBC0`) on the left, from `itembox.tim` (texture
-- 0x1D) at (7, 14), and the item page's inventory, counts and message box
-- on the right, with the equipped item's panel where the personal slot's
-- was and EXIT above it. The primitives are drawn by depth, the deepest
-- first: the rows' icons, the rows' bands (averaged), the names, the
-- panel's frame and lines, its arrows and scroll bar.
local B = {}
B.X, B.Y = 7, 14
B.TEXTURE = 0x1D
-- `St_init_itembox`'s sprites (`0x53DB88`, read from the end) at their
-- places from the panel's origin (`0x53DC00` to `0x53DC60`): the frame's
-- sixteen pieces.
B.PIECES = {
    { 199, 142, 8, 6, 0, 214 },
    { 199, 4, 8, 6, 0, 202 },
    { 199, 11, 8, 130, 0, 60 },
    { 196, 6, 2, 142, 10, 60 },
    { 4, 6, 2, 142, 8, 60 },
    { 134, 127, 62, 20, 62, 40 },
    { 134, 6, 62, 20, 0, 40 },
    { 6, 127, 128, 20, 0, 20 },
    { 6, 6, 128, 20, 0, 0 },
    { 211, 0, 39, 12, 0, 228 },
    { 208, 0, 3, 153, 15, 60 },
    { 0, 0, 3, 153, 12, 60 },
    { 131, 149, 77, 4, 0, 244 },
    { 131, 0, 77, 4, 0, 240 },
    { 3, 149, 128, 4, 0, 252 },
    { 3, 0, 128, 4, 0, 248 },
}
-- The bands behind the rows but the chosen one (`0x53DBEC`): their tops,
-- 20 high from x 6 to 0xC5, averaged.
B.BANDS = { 26, 46, 87, 107 }
B.BAND = 48
-- The chosen row's lines, and the box's end after its last slot.
B.LINE_IDLE = { 0x30, 0x20, 8 }
B.END = { 0x80, 0x20, 0x20 }
-- The equipped item's panel, moved right by 0x34 onto the personal slot's
-- place (`St_disp_equip` in item box mode), and its count by 0x36.
B.EQUIP_DX, B.EQUIP_COUNT_DX = 0x34, 0x36
-- The cursor's pieces from the slot by its two-slot mark (`0x53EFAC`): the
-- right, the middle and the left.
B.CURSOR = {
    [0] = { 25, 3, 3 },
    { 65, 25, 3 },
    { 25, -15, -37 },
}

function B.box_art(api, d)
    if d.box == nil then
        d.box = api:page("ui/status/itembox_0") or false
    end
    return d.box
end

function B.box_sprite(api, d, x, y, w, h, u, v)
    local handle = B.box_art(api, d)
    if handle then
        api:page_sprite_scaled(handle, u, v, w, h, d.ox + B.X + x, d.oy + B.Y + y, w, h, 0, 255)
    end
end

-- A row's icon (`0x4FCF8E`): the 39x30 cell squeezed onto the quad from
-- x 0x92 to 0xAB and 19 down, which the PC fills with its far edges, and a
-- two-slot weapon's right half beside it.
function B.box_icon(api, d, item, mark, x, y)
    local texture, u, v = icon_cell(item)
    local handle = d.pages[texture]
    if handle ~= nil then
        api:page_sprite_scaled(handle, u, v, 39, 30, d.ox + x, d.oy + y, 26, 20, 0, 255)
    end
    if mark ~= 0 then
        local half, hu, hv = half_cell(item)
        handle = d.pages[half]
        if handle ~= nil then
            api:page_sprite_scaled(handle, hu, hv, 40, 30, d.ox + x + 0x18, d.oy + y, 26, 20, 0, 255)
        end
    end
end

function B.box_name(api, d, item, x, y)
    local bytes = api:item_name_bytes(item == 0 and 100 or item)
    if bytes ~= nil then
        api:glyphs(d.ox + x, d.oy + y, bytes)
    end
end

function B.box_line(api, d, y, rgb)
    api:line(d.ox + B.X + 6, d.oy + y, d.ox + B.X + 0xC4 + 1, d.oy + y, rgb[1], rgb[2], rgb[3])
end

-- A slot's two-slot mark: 1 on a two-slot weapon's left half, 2 on its
-- right (`itembox2::marks`).
function B.inventory_mark(api, slot)
    local index = 0
    while index < s.slots do
        local item = api:re2_slot(index)
        if is_wide(item) and index + 1 < s.slots and api:re2_slot(index + 1) == item then
            if slot == index then
                return 1
            elseif slot == index + 1 then
                return 2
            end
            index = index + 2
        else
            index = index + 1
        end
    end
    return 0
end

function B.draw_box(api)
    local page = s.page or api:re2_box()
    if page == nil or not page.started then
        return
    end
    local last = s.last
    local ran_page = last ~= nil and last.started and last.routine == 3
    local d = art(api)
    d.ox, d.oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:fill_rect(0, 0, api:width(), api:height(), 8, 8, 32)
    local top, scroll = page.top, page.scroll

    -- The inventory's side: the message box, the grid's and the equipped
    -- item's frames, the icons, the counts.
    fill(api, d, MESSAGE_BOX[1], MESSAGE_BOX[2], MESSAGE_BOX[3], MESSAGE_BOX[4], MESSAGE_FILL)
    pieces(api, d, MESSAGE_FRAME, 0, 0)
    for index = FRAME_LEFT + 1, #FRAME do
        local p = FRAME[index]
        if index <= FRAME_LEFT + 4 then
            sprite(api, d, ST, p[1] + B.EQUIP_DX, p[2], p[3], p[4], p[5], p[6])
        elseif index > FRAME_LEFT + 6 then
            sprite(api, d, ST, p[1], p[2], p[3], p[4], p[5], p[6])
        end
    end
    -- EXIT and the piece under it, lit while its routine runs.
    sprite(api, d, ST, 255, 14, 55, 12, 0, 164)
    sprite(api, d, ST + 5, 259, 15, 47, 10, 0, 30, page.routine == 2 and TAB_SELECTED or TAB_DIM_BOX)
    for slot = s.slots - 1, 0, -1 do
        slot_icon(api, d, slot, GRID_X + SLOT_W * (slot % 2), GRID_Y + SLOT_H * (slot // 2))
    end
    sprite(api, d, ICONS + 2, EQUIP_BOX[1] + B.EQUIP_DX, EQUIP_BOX[2], EQUIP_BOX[3], EQUIP_BOX[4], EQUIP_BOX[5], EQUIP_BOX[6])
    local equip = equipped_slot(api)
    if equip ~= nil then
        local item, quantity = api:re2_slot(equip)
        local texture, u, v = icon_cell(item)
        local x = EQUIP_X + B.EQUIP_DX
        if is_wide(item) then
            sprite(api, d, texture, x - 20, EQUIP_Y, SLOT_W, SLOT_H, u, v)
            local half, hu, hv = half_cell(item)
            sprite(api, d, half, x + 20, EQUIP_Y, SLOT_W, SLOT_H, hu, hv)
        else
            sprite(api, d, texture, x, EQUIP_Y, SLOT_W, SLOT_H, u, v)
        end
        count(api, d, item, quantity, EQUIP_COUNT_X + B.EQUIP_COUNT_DX, EQUIP_COUNT_Y)
    end
    for slot = 0, s.slots - 1 do
        local item, quantity = api:re2_slot(slot)
        local at = COUNT_AT[slot]
        if item ~= 0 and not (slot % 2 == 1 and is_wide(item)) then
            count(api, d, item, quantity, COUNT_ORIGIN_X + at[1], COUNT_ORIGIN_Y + at[2])
        end
    end

    -- The box's rows: icons, then the bands over them, the names and the
    -- counts.
    for row = 4, 0, -1 do
        local item, _, mark = api:re2_box_slot((top + row) % 64)
        B.box_icon(api, d, item, mark, B.X + 0x92, B.Y + scroll + 20 * row + 0x1B)
    end
    for _, y in ipairs(B.BANDS) do
        api:fade_tile(0, B.BAND, d.ox + B.X + 6, d.oy + B.Y + y, 0xC5 - 6, 20)
    end
    for row = 4, 0, -1 do
        local item, quantity = api:re2_box_slot((top + row) % 64)
        B.box_name(api, d, item, B.X + 7, B.Y + scroll + 20 * row + 0x1D)
        if item ~= 0 then
            count(api, d, item, quantity, COUNT_ORIGIN_X - 67, COUNT_ORIGIN_Y - 18 + 20 * row + scroll)
        end
    end
    -- A scroll's incoming row: its name and count; its place is held by
    -- the panel's blank row.
    local incoming = page.sub == 2 and -1 or page.sub == 3 and 5 or nil
    if incoming ~= nil then
        local item, quantity = api:re2_box_slot((top + incoming) % 64)
        B.box_name(api, d, item, B.X + 7, B.Y + scroll + 20 * incoming + 0x1D)
        if item ~= 0 then
            count(api, d, item, quantity, COUNT_ORIGIN_X - 67, COUNT_ORIGIN_Y - 18 + 20 * incoming + scroll)
        end
    end

    -- The panel's lines and frame.
    local lines = page.sub == 0 and B.LINE_IDLE
        or { page.pulse + 0x50, page.pulse + 0x40, page.pulse & 0x1F }
    B.box_line(api, d, B.Y + 0x42, lines)
    B.box_line(api, d, B.Y + 0x56, lines)
    local chosen = (top + 2) % 64
    if chosen < 5 or chosen == 63 then
        B.box_line(api, d, B.Y + scroll - 20 * ((top - 4) % 64) + 0x4F3, B.END)
    end
    for k = #B.PIECES, 1, -1 do
        local p = B.PIECES[k]
        B.box_sprite(api, d, p[1], p[2], p[3], p[4], p[5], p[6])
    end

    -- The arrows, pressed on a tick up or down moves the box, and the
    -- scroll bar's five pieces, two rows a slot.
    for row = 0, 4 do
        B.box_sprite(api, d, 200, 12 + ((top + row) % 64) * 2, 6, 2, 0, 190)
    end
    B.box_sprite(api, d, 94, 5, 20, 20, 18, page.arrows & 1 ~= 0 and 60 or 80)
    B.box_sprite(api, d, 94, 127, 20, 20, 18, page.arrows & 2 ~= 0 and 100 or 120)
    if incoming ~= nil then
        local item, _, mark = api:re2_box_slot((top + incoming) % 64)
        local w = mark ~= 0 and 0x30 or 0x18
        local handle = B.box_art(api, d)
        if handle then
            api:page_sprite_scaled(handle, 40, 60, mark ~= 0 and 80 or 40, 30,
                d.ox + B.X + 0x92, d.oy + B.Y + scroll + 20 * incoming + 0x1B, w + 2, 20, 0, 255)
        end
    end

    -- The inventory's cursor and the item's name, from the page's routine.
    if ran_page then
        local slot = page.cursor
        local mark = B.inventory_mark(api, slot)
        local offsets = B.CURSOR[mark] or B.CURSOR[0]
        local x = COUNT_ORIGIN_X + SLOT_W * (slot % 2)
        local y = COUNT_ORIGIN_Y + 1 + SLOT_H * (slot // 2)
        local rgb = { s.pulse, s.pulse, s.pulse }
        sprite(api, d, ST + 3, x + offsets[3], y, 22, 34, 0, 30, rgb)
        sprite(api, d, ST + 3, x + offsets[2], y, 40, 34, 2, 30, rgb)
        sprite(api, d, ST + 3, x + offsets[1], y, 22, 34, 22, 30, rgb)
        if page.sub ~= 5 then
            local item = api:re2_slot(slot)
            if item ~= 0 then
                api:glyphs(d.ox + NAME_X, d.oy + NAME_Y, api:item_name_bytes(item) or "")
            end
        end
    end
end

-- A script's map (`0x4FB800` and `St_disp_wall`): the background, the
-- backdrop's tiles and the floor's rooms, the target's fill blinking, with no frame, tab or
-- marker.
local function draw_script_map(api)
    api:fill_rect(0, 0, api:width(), api:height(), 8, 8, 32)
    -- The tick that finds the fade in done draws nothing over the
    -- background.
    local view = api:re2_script_map()
    if view == nil then
        return
    end
    local d = art(api)
    d.ox, d.oy = api:width() // 2 - 160, api:height() // 2 - 120
    for row = 0, TILE_ROWS - 1 do
        for column = 0, TILE_COLUMNS - 1 do
            sprite(api, d, ST + 1, TILE * column, TILE * row, TILE, TILE, TILE_U, TILE_V)
        end
    end
    draw_map(api, d, view)
end

function M.draw(api)
    if s.box then
        B.draw_box(api)
        return
    end
    if s.script_map then
        draw_script_map(api)
        return
    end
    if s.get then
        draw_get(api)
        return
    end
    local page = s.page or api:re2_items()
    if page == nil then
        return
    end
    local last = s.last
    local d = art(api)
    d.ox, d.oy = api:width() // 2 - 160, api:height() // 2 - 120
    -- The page the tick started on: its logic draws this frame.
    local started = last or page
    -- The reader's background is black from its load (`Bg_set_mode(2, 0)`)
    -- to sub 0xC's last tick, on the ticks those subs run.
    if reading(started, FILE_READ_OUT) then
        api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0)
    else
        api:fill_rect(0, 0, api:width(), api:height(), 8, 8, 32)
    end
    -- The reader's subs 0xD to 0x12 draw only the page, on the ticks they
    -- run; the load tick (sub 0xB's last) draws nothing of the screen.
    if reading(page, FILE_PAGE) then
        if reading(started, FILE_PAGE) and page.file_page ~= nil then
            draw_file_page(api, d, page.file, page.file_page)
        end
        return
    end
    -- The MAP page's backdrop, on each tick its logic runs.
    if started.page == PAGE_MAP then
        for row = 0, TILE_ROWS - 1 do
            for column = 0, TILE_COLUMNS - 1 do
                sprite(api, d, ST + 1, TILE * column, TILE * row, TILE, TILE, TILE_U, TILE_V)
            end
        end
    end
    local k = map_slide(page) + reader_slide(page)
    local T, B, L, R = SLIDE_TABS * k, SLIDE_BOX * k, SLIDE_LEFT * k, SLIDE_RIGHT * k
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
    pieces(api, d, FRAME_TABS, 0, T)
    local check = on_items and (is_check(page) or (last ~= nil and page.sub == SUB_GRID
        and is_check(last) and last.step == CHECK_DONE))
    local bx, by = 0, 0
    if on_items and is_check(page) then
        bx, by = check_box(page)
    end
    local file_dy, file_top = file_slide(page, last)
    if file_top ~= nil then
        file_top = file_top + FILE_FALL * reader_slide(page)
    end
    by = by + B + file_dy
    -- The binders are in the 3D list, which draws under every sprite: the
    -- message box covers them as it comes back up.
    if file_top ~= nil then
        file_binders(api, d, page.page == PAGE_FILE and page or last, file_top)
    end
    fill(api, d, MESSAGE_BOX[1] + bx, MESSAGE_BOX[2] + by, MESSAGE_BOX[3] + bx, MESSAGE_BOX[4] + by, MESSAGE_FILL)
    pieces(api, d, MESSAGE_FRAME, bx, by)
    for index, p in ipairs(FRAME) do
        sprite(api, d, ST, p[1] + (index <= FRAME_LEFT and L or R), p[2], p[3], p[4], p[5], p[6])
    end
    if file_top ~= nil then
        pieces(api, d, FILE_PANEL, 0, file_top)
    end
    local panel_y = check and (is_check(page) and check_panel_y(page) or CHECK_BOTTOM) or nil
    if panel_y ~= nil then
        check_panel(api, d, panel_y)
    end

    -- The tabs.
    for tab = 0, 3 do
        sprite(api, d, ST + 5, TAB_X + 49 * tab, TAB_Y + T, TAB_W, TAB_H, 0, TAB_H * tab,
            tab ~= page.tab and TAB_DIM or page.page >= PAGE_ITEMS and TAB_LIT or TAB_SELECTED)
    end

    -- The portrait, the condition and the slots' icons.
    for _, p in ipairs(PORTRAIT) do
        sprite(api, d, ST + 1, p[1] + L, p[2], p[3], p[4], p[5], p[6])
    end
    local cond = condition(api, page)
    sprite(api, d, ST + 4, CONDITION_X + L, CONDITION_Y, 46, 11, 0, 11 * cond)
    slot_icon(api, d, PERSONAL_SLOT, PERSONAL_X + R, PERSONAL_Y)
    for slot = s.slots - 1, 0, -1 do
        slot_icon(api, d, slot, GRID_X + R + SLOT_W * (slot % 2), GRID_Y + SLOT_H * (slot // 2))
    end

    -- The equipped item and the counts.
    sprite(api, d, ICONS + 2, EQUIP_BOX[1] + R, EQUIP_BOX[2], EQUIP_BOX[3], EQUIP_BOX[4], EQUIP_BOX[5], EQUIP_BOX[6])
    local equip = equipped_slot(api)
    if equip ~= nil then
        local item, quantity = api:re2_slot(equip)
        local texture, u, v = icon_cell(item)
        if is_wide(item) then
            sprite(api, d, texture, EQUIP_X + R - 20, EQUIP_Y, SLOT_W, SLOT_H, u, v)
            local half, hu, hv = half_cell(item)
            sprite(api, d, half, EQUIP_X + R + 20, EQUIP_Y, SLOT_W, SLOT_H, hu, hv)
        else
            sprite(api, d, texture, EQUIP_X + R, EQUIP_Y, SLOT_W, SLOT_H, u, v)
        end
        count(api, d, item, quantity, EQUIP_COUNT_X + R, EQUIP_COUNT_Y)
    end
    for slot = 0, s.slots - 1 do
        local item, quantity = api:re2_slot(slot)
        local at = COUNT_AT[slot]
        if item ~= 0 and not (slot % 2 == 1 and is_wide(item)) then
            count(api, d, item, quantity, COUNT_ORIGIN_X + R + at[1], COUNT_ORIGIN_Y + at[2])
        end
    end

    -- The trace, the cursor and CHECK's picture.
    draw_trace(api, d, page, cond, L)
    local cursor_shown = s.show_cursor and not closing and ran_items
    local cx, cy, tx, ty = slide(api, page)
    if cursor_shown then
        draw_cursor(api, d, page.cursor, page.blink, cx, cy)
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

    -- The list's name, on the ticks sub 10 runs (its decide and cancel
    -- ticks too).
    if started.page == PAGE_FILE and started.sub == FILE_LIST and page.page == PAGE_FILE
        and page.file ~= nil and page.file ~= 0xFF then
        api:glyphs(d.ox + FILE_NAME_X, d.oy + FILE_NAME_Y, api:item_name_bytes(FILE_NAMES + page.file) or "")
    end

    -- The shown map, on the ticks its logic leaves it shown.
    if started.page == PAGE_MAP and started.sub == MAP_SHOWN
        and page.page == PAGE_MAP and page.sub == MAP_SHOWN then
        draw_map(api, d)
    end
end

return M
