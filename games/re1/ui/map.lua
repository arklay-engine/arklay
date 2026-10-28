-- The pause menu's map tab.
--
-- The tab is always available: the original's `menu_tab_map` runs the map
-- state machine without a scenario check (the radio's flag gates only the
-- radio tab). The browser composites the highest available layout's art
-- (`map/0e`, `00`, `0c` or `0d` - `map_tab_load_map` loads the
-- file of the mask's highest set bit, one page for every browser sprite):
-- the 160x72 base window at (33,32), the four layout markers (the current
-- layout's at full brightness, the others at half), the current area's name
-- row ("Mansion 1F" and friends, 80x16 at (57,104)) and the current floor's
-- number glyph (24x16 at (137,104)), plus the room-step arrows while a
-- neighbouring room is reachable. Every sprite's index-0 texels are
-- transparent. Up/down step the room index inside the current layout (only
-- when the neighbouring area is known), left/right step between the layouts
-- the owned maps unlock. Confirm switches to the full floor-plan view of the
-- current area; cancel returns to the browser and then closes the tab.
--
-- The full display is `map_display_update`'s: the browser stays up behind it
-- dimmed, the blue grid (`Map_blue`, two 80x104 halves side by side) is
-- half-blended over it and the floor plan over that. Both open as a thin
-- line that stretches across, overshoots in height and settles (the zoom
-- state machine, scaled about the menu's origin like `AddSprite_Ex`), and
-- close the same way in reverse. Once open, the plan's semi-transparent
-- texels add to the frame, the current room's marker pulses red, and a
-- scanline sweeps down the screen every 256 frames: one row of the grid and
-- the plan drawn again a pixel to the right (`g_MapFloorAnim`). While a
-- story objective points at an area (`MAP_MODE`: Mansion 1F for the serum,
-- Guardhouse B1 for Plant 42), that area's plan blinks a growing square
-- over it (`g_MapLabelDesc`, the plan art's 8x8 frames at row 104).
--
-- The screen plays the reference's own cues: the cursor blip only for a
-- move that happens, `map_zoom` as the plan opens or re-opens, `map_label`
-- as the objective square starts each blink, and the cancel cue.
--
-- The area/layout/room tables are the built-in values below; a pack that
-- carries `map/tables.bin` (an older conversion or a mod) overrides them.
-- A pack without the map art leaves the tab blank with a warning.
--
-- The screen publishes `zoomed` (0/1) for the engine's tests. The room-flags/SCD
-- bit that records a map pick-up is raised by the engine's pick-up rule.

local Panel = require("lib/status_panel")

local M = {}

-- The screen rect the browser's composite covers (the base window's 160x72
-- plus the name and glyph rows below it): the status panel's decorations
-- that overlap it, or the full display's quads, are re-drawn on top.
local BROWSER_RECT = { 33, 32, 160, 96 }

--- The viewer's rows: the zoom's vertical overshoot (`scaleY` 0x15DC) is cut
--- off here, as the reference shows it, rather than running over the frame.
local VIEWER_TOP, VIEWER_BOTTOM = 24, 128

local TABLES_ENTRY = "map/tables.bin"
-- The full display's blue grid backdrop (`Map_blue`).
local BACKDROP_STEM = "map/bg"
-- The four browser floor-plan stems, indexed by layout.
local TAB_STEMS = { [0] = "map/0d", "map/0c", "map/00", "map/0e" }
-- The eleven full-display floor-plan stems, indexed by area. The file
-- numbers are not in area order: the guardhouse's are `0a`/`0b` and the
-- laboratory's `06`..`09` (`g_MapFileNames`).
local AREA_STEMS = {
    [0] = "map/01", "map/02", "map/03", "map/04", "map/05", "map/0a",
    "map/0b", "map/06", "map/07", "map/08", "map/09",
}

-- The flag bank the visited rooms and owned maps live in, and the owned-map
-- bit base (`ROOM_FLAG_MAP_BASE`).
local BANK_ROOM_FLAGS = 8
local ROOM_FLAG_MAP_BASE = 0x7C

-- The objective flags (`map_update_objective_highlight`): an "outstanding"
-- bit in the second scenario bank and its "acknowledged" bit in the first.
local BANK_SCENARIO, BANK_SCENARIO2 = 0, 1
local OBJ_JILL_SERUM = 0x23
local OBJ_CHRIS_SERUM1, OBJ_CHRIS_SERUM2 = 0x2D, 0x2E
local OBJ_PLANT42 = 0x38
local ACK_PLANT42, ACK_SERUM1, ACK_SERUM2 = 0x48, 0x49, 0x4A

-- The browser's base map window (`g_MapBaseDesc`): the 160x72 top-left window
-- of the loaded layout art at (33,32).
local BASE_RECT = { x = 33, y = 32, w = 160, h = 72, u = 0x00, v = 0x00 }
-- The browser's sprite table (`g_MapSprites`), in draw order: the four layout
-- markers, the area-name row, the floor number glyph and the two room-step
-- arrows.
local SPRITES = {
    [0] = { x = 121, y = 32, w = 56, h = 64, u = 0xA0, v = 0x00 },
    { x = 73, y = 32, w = 80, h = 64, u = 0x00, v = 0x48 },
    { x = 49, y = 32, w = 40, h = 64, u = 0x50, v = 0x48 },
    { x = 121, y = 32, w = 28, h = 64, u = 0xC8, v = 0x48 },
    { x = 57, y = 104, w = 80, h = 16, u = 0x78, v = 0x48 },
    { x = 137, y = 104, w = 24, h = 16, u = 0xE8, v = 0x60 },
    { x = 145, y = 96, w = 8, h = 8, u = 0xA0, v = 0x40 },
    { x = 145, y = 120, w = 8, h = 8, u = 0xB0, v = 0x40 },
}

-- The `colorMul` of the browser's sprites, and of the markers of the layouts
-- the player is not standing in.
local MUL_FULL = 0x80
local MUL_HALF = 0x40

-- The browser's `colorMul` while the full display is up
-- (`map_display_update`): the base window, then every sprite.
local MUL_DIM_BASE = 0x40
local MUL_DIM_SPRITE = 0x30

-- The full display's scaled sprites (`g_MapZoomDesc`), positioned relative
-- to the menu's scaled-sprite origin (`g_SubpixelOffsetX/Y`): the plan's
-- 128x100 top rows, and the grid's 80x104 top-left window twice, either side
-- of x = 112. A grid half is 81 wide while the display opens or closes.
local ZOOM_X, ZOOM_Y = 112, 76
local PLAN_QUAD = { x = 0x10, y = 2, w = 128, h = 100, px = 80, py = 50 }
local GRID_QUADS = {
    { x = 0, y = 0, h = 104, px = 80, py = 52 },
    { x = 0, y = 0, h = 104, px = 0, py = 52 },
}
local GRID_W_OPEN, GRID_W_ZOOM = 80, 81
-- fix16.12 unity.
local FIX_ONE = 0x1000
-- The current room's red on the highlight page (`0x001f` BGR555).
local HIGHLIGHT_RED = 0xF8
-- The palette indices of the plan's outline and door/ladder symbols, and the
-- first of six per room (`load + 0x22`, `load + 0x2e`).
local SYMBOL_FIRST, SYMBOL_LAST = 7, 12
local ROOM_FIRST = 13
-- The scanline sweep (`g_MapFloorAnim`): drawn while its counter is below
-- 0xD0, one row per two frames from y = 24, each row a pixel right of the
-- layer it copies.
local SWEEP_END = 0xD0
-- The objective label (`g_MapLabelDesc`): an 8x8 frame of the plan art's
-- row 104 (`texV` 0xF0 on the 0x88-based page), eight frames across, drawn
-- at the first position for the Mansion 1F objective (`MAP_MODE` 2) and the
-- second for the guardhouse's (4). It starts ten frames after the plan
-- comes to rest (`DAT_00ac3694`).
local LABEL_V, LABEL_SIZE = 104, 8
local LABEL_POS = { [0] = { 0x38, 0x34 }, { 0x86, 0x43 } }
local LABEL_DELAY = 10

local BACKDROP_WARNING = "warning: no map backdrop %s: %s; the map tab stays blank"
local PAGE_WARNING = "warning: no map page %s: %s; the map tab stays blank"
local PLAN_WARNING = "warning: no map plan %s: %s; the map display stays blank"

local s

-- ---- tables ----------------------------------------------------------------

--- The built-in tables, 0-based like the original's arrays.
local function default_tables()
    local function list(...)
        local values = { ... }
        local out = {}
        for index = 1, #values do
            out[index - 1] = values[index]
        end
        return out
    end
    return {
        -- Visited-room bit base per map group (`(stage - 1) % 5`).
        stage_room_offsets = list(0, 32, 63, 82, 100, 0),
        -- Room count per map group.
        room_counts = list(32, 31, 19, 18, 24, 0),
        -- Area per `(layout, room index)`; 0xFF is no room.
        area = list(2, 0, 1, 0xFF, 4, 3, 0xFF, 0xFF, 6, 5, 0xFF, 0xFF, 10, 9, 8, 7),
        -- Layout per area.
        layouts = list(0, 0, 0, 1, 1, 2, 2, 3, 3, 3, 3, 0, 0, 0, 0, 0),
        -- Map group per area.
        groups = list(0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 4, 4, 0, 3, 2, 2),
        -- Room count per layout (the index of the trailing exit room).
        layout_room_count = list(0, 3, 2, 2),
        -- Floor-number offset per layout.
        layout_offset = list(4, 4, 4, 4),
    }
end

-- The `map/tables.bin` layout: the magic, then the seven tables.
local TABLE_FIELDS = {
    { "stage_room_offsets", 6 }, { "room_counts", 6 }, { "area", 16 }, { "layouts", 16 },
    { "groups", 16 }, { "layout_room_count", 4 }, { "layout_offset", 4 },
}
local TABLES_BYTES = 4 + 6 + 6 + 16 + 16 + 16 + 4 + 4

--- Parse `map/tables.bin`; nil and the reason when it is invalid.
function M.parse_tables(data)
    if #data < TABLES_BYTES then
        return nil, ("map table blob is %d bytes, need %d"):format(#data, TABLES_BYTES)
    end
    if data:sub(1, 4) ~= "MAP1" then
        return nil, "map table blob has no MAP1 magic"
    end
    local tables, cursor = {}, 5
    for _, entry in ipairs(TABLE_FIELDS) do
        local name, count = entry[1], entry[2]
        local values = {}
        for index = 0, count - 1 do
            values[index] = data:byte(cursor + index)
        end
        tables[name] = values
        cursor = cursor + count
    end
    return tables
end

local function load_tables(api)
    local data = api:pack_bytes(TABLES_ENTRY)
    if not data then
        -- A converted pack carries no tables entry; the built-ins are the
        -- documented values.
        return default_tables()
    end
    local tables, err = M.parse_tables(data)
    if not tables then
        api:log(("warning: invalid %s: %s; the map tab uses the built-in tables"):format(TABLES_ENTRY, err))
        return default_tables()
    end
    return tables
end

--- The area at a layout's room index (0xFF when the slot is empty).
local function area_at(layout, room)
    return s.tables.area[layout * 4 + room] or 0xFF
end

-- The map index an area's owned bit uses, or nil for the areas that are
-- never "known".
local MAP_INDEX = { [0] = 0, [1] = 1, [3] = 2, [4] = 3, [5] = 4, [6] = 4, [8] = 5, [9] = 5 }

--- Whether the area's map is owned (the `0x7C + index` room-flags bit).
local function area_known(api, area)
    local index = MAP_INDEX[area]
    return index ~= nil and api:flag(BANK_ROOM_FLAGS, ROOM_FLAG_MAP_BASE + index)
end

--- Whether a group's room has been visited.
local function room_visited(api, group, room)
    local base = s.tables.stage_room_offsets[group]
    if base == nil or room >= (s.tables.room_counts[group] or 0) then
        return false
    end
    return api:flag(BANK_ROOM_FLAGS, (base + room) & 0xFF)
end

--- The area a group's visited room belongs to.
local function room_area(group, room)
    if group == 1 then
        return (room < 0x1A or room == 0x1D) and 1 or 2
    elseif group == 2 then
        return (room < 0x06 or room == 0x10) and 3 or 4
    elseif group == 3 then
        return room < 0x0D and 5 or 6
    elseif group == 4 then
        if room < 0x02 or room == 0x16 then
            return 7
        elseif room < 0x05 then
            return 8
        elseif room < 0x13 then
            return 9
        end
        return 10
    end
    return 0
end

--- The area mask from the visited rooms and owned maps.
local function area_mask(api)
    local mask = 0
    for group = 0, 4 do
        for room = 0, s.tables.room_counts[group] - 1 do
            if room_visited(api, group, room) then
                mask = mask | (1 << room_area(group, room))
            end
        end
    end
    for area = 0, 10 do
        if area_known(api, area) then
            mask = mask | (1 << area)
        end
    end
    return mask
end

--- The layout-availability mask from the area mask.
local function layout_mask(api)
    local mask = area_mask(api)
    local layouts = 0
    for area = 0, 10 do
        if mask & (1 << area) ~= 0 then
            layouts = layouts | (1 << s.tables.layouts[area])
        end
    end
    return layouts
end

--- The area, map group and map room the display opens on for a room
--- (`menu_init_map_screen`): the room itself, but for the cameras that look
--- into a neighbouring area.
local function initial_position(group, room, cut)
    if group == 0 then
        if room == 0x07 and cut > 2 and cut < 7 then
            return 0, group, 0x1D
        elseif room == 0x0F and (cut == 3 or cut == 4) then
            return 0, group, 0x1E
        elseif room == 0x10 and cut == 0 then
            return 2, 1, 0x1E
        end
        return 0, group, room
    elseif group == 1 then
        if room == 0x0F and (cut == 2 or cut == 5) then
            return 1, group, 0x1D
        elseif room == 0x0C and cut > 3 and cut < 8 then
            return 0, 0, 0x1F
        end
        return room < 0x1A and 1 or 2, group, room
    elseif group == 2 then
        if room == 0x0B and cut > 4 and cut < 7 then
            return 4, group, 0x11
        elseif room == 0x0F and cut == 6 then
            return 4, group, 0x12
        end
        return room < 0x06 and 3 or 4, group, room
    elseif group == 3 then
        return room < 0x0D and 5 or 6, group, room
    end
    if room == 0x07 and cut < 4 then
        return 9, group, 0x16
    elseif room < 0x02 then
        return 7, group, room
    elseif room < 0x05 then
        return 8, group, room
    elseif room > 0x12 then
        return 10, group, room
    end
    return 9, group, room
end

--- The objective `MAP_MODE` (`map_update_objective_highlight`): 1 points
--- at Mansion 1F (the serum), 3 at Guardhouse B1 (Plant 42), 0 at nothing.
local function objective_mode(api)
    local function on(bank, bit)
        return api:flag(bank, bit)
    end
    local stage, _, _, player = api:room()
    if player == 1 then
        if on(BANK_SCENARIO2, OBJ_JILL_SERUM) and not on(BANK_SCENARIO, ACK_SERUM1) then
            return 1
        end
        return 0
    end
    if on(BANK_SCENARIO2, OBJ_CHRIS_SERUM1) and not on(BANK_SCENARIO, ACK_SERUM1) then
        return 1
    end
    if on(BANK_SCENARIO2, OBJ_CHRIS_SERUM2) and not on(BANK_SCENARIO, ACK_SERUM2) then
        -- Outside the mansion (stages 1 and 2) it shows only while the
        -- Plant 42 objective is outstanding too.
        if stage ~= 1 and stage ~= 2 then
            return on(BANK_SCENARIO2, OBJ_PLANT42) and 1 or 0
        end
        return 1
    end
    if on(BANK_SCENARIO2, OBJ_PLANT42) and not on(BANK_SCENARIO, ACK_PLANT42) then
        return 3
    end
    return 0
end

--- The initial area for a room.
local function initial_area(group, room, cut)
    return (initial_position(group, room, cut))
end

--- The area of the room *after* `room`, exactly as the original's
--- `g_MapAreaTable[layout * 4 + room + 1]` read resolves it: past a layout's
--- last slot the read continues into the next layout's first room, and
--- layout 3's lands on the layout table's own first byte (Mansion 1F).
local function next_area(layout, room)
    local index = room + 1
    if index < 4 then
        return area_at(layout, index)
    elseif index == 4 then
        return layout == 3 and 0 or area_at(layout + 1, 0)
    end
    return 0xFF
end

--- The right/left move flags of a layout position: bit 0 = a room to the
--- right exists, bit 1 = a room to the left, both gated by the neighbour's
--- area being reachable.
local function move_flags(api, layout, room)
    local mask = area_mask(api)
    local flags = 0
    local last = s.tables.layout_room_count[layout] or 0
    if (last - room) & 0xFF ~= 1 then
        local next = next_area(layout, room)
        if next ~= 0xFF and (mask & (1 << (next & 0x0F)) ~= 0 or area_known(api, next)) then
            flags = flags | 1
        end
    end
    if room ~= 0 then
        local prev = area_at(layout, room - 1)
        if prev ~= 0xFF and (mask & (1 << (prev & 0x0F)) ~= 0 or area_known(api, prev)) then
            flags = flags | 2
        end
    end
    return flags
end

M.initial_area = initial_area
M.area_stem = function(area)
    return AREA_STEMS[area]
end
M.area_at = function(layout, room)
    return area_at(layout, room)
end

-- ---- state -----------------------------------------------------------------

local function room_index()
    return s.room[s.layout]
end

--- Select the area under the current layout position.
local function refresh_area()
    local area = area_at(s.layout, room_index())
    if area ~= 0xFF then
        s.area = area
    end
end

--- Load one page once, warning on the first failure only. `warning` formats
--- the stem and the error.
local function page(api, stem, warning)
    local entry = s.pages[stem]
    if entry == nil then
        local handle, err = api:indexed_page(stem)
        if handle then
            entry = handle
        else
            api:log(warning:format(stem, err))
            entry = false
        end
        s.pages[stem] = entry
    end
    return entry or nil
end

function M.open(api)
    -- Drawn on black: a wider aspect gives the panel a wider canvas.
    api:set("canvas", 1)
    s = {
        tables = load_tables(api),
        -- The current layout (0..3) and the selected room index per layout.
        layout = 0,
        room = { [0] = 1, 1, 1, 3 },
        area = 0,
        -- The player's map group and room (`MAP_GROUP`, `MAP_ROOM`): the
        -- display's highlighted room.
        map_group = 0,
        map_room = 0,
        -- Whether the full floor plan is up (`MAP_STATE` 1), and its zoom
        -- state (`MAP_ZOOM_STATE`, 0 until its first frame).
        zoomed = false,
        zoom = 0,
        -- The zoom's fix16.12 scale, whether the quads sit at rest (the
        -- unscaled layout), whether the plan adds to the frame (the 0x51
        -- descriptor variant) and the countdown of the rest states.
        scale_x = 1,
        scale_y = 200,
        at_rest = false,
        additive = false,
        hold = 0,
        -- The "you are here" pulse (`MAP_BLINK_STATE`, its ramp, and
        -- `g_MapDotBlink`).
        pulse_state = 0,
        pulse_ramp = 0x50,
        pulse = 0,
        -- The scanline sweep's counter (`MAP_ARROW_ANIM`), and the value the
        -- frame draws (nil when the sweep is not running).
        sweep = 0,
        sweep_row = nil,
        -- The frame after the close: the display's last frame still dims the
        -- browser.
        closing = false,
        -- Blink phase for the cursor arrows (`MAP_HL_COUNT`).
        ticks = 0,
        -- The objective (`MAP_MODE`, raised by one as the tab opens: 0, 2
        -- or 4), the label's state (`MAP_HL_STATE`: 0 off, 1 waiting, 2..5
        -- the blink), its counter (`MAP_HL_FRAME`) and the frame to draw
        -- (nil when hidden).
        objective = 0,
        label_state = 0,
        label_count = 0,
        label_frame = nil,
        -- The original's tab action runs one frame after its confirm, so the
        -- browser stays blank until its first update.
        started = false,
        -- Loaded pages by stem (false once a load failed).
        pages = {},
    }
    M.state = s
    page(api, BACKDROP_STEM, BACKDROP_WARNING)
    local stage, room, cut = api:room()
    local group = (((stage or 1) - 1) & 0xFF) % 5
    s.area, s.map_group, s.map_room = initial_position(group, room or 0, cut or 0)
    s.layout = s.tables.layouts[s.area] or 0
    local mode = objective_mode(api)
    s.objective = mode ~= 0 and mode + 1 or 0
    -- The original overwrites each layout's index with the last room slot
    -- whose area matches the current one.
    for layout = 0, 3 do
        for slot = 0, 3 do
            if area_at(layout, slot) == s.area then
                s.room[layout] = slot
            end
        end
    end
    api:set("zoomed", 0)
end

--- The next layout a left (`step == 1`) or right (`step == -1`) press picks.
--- The D-pad branches read the raw pad bits: left steps to the next layout
--- and right to the previous one; with all four layouts the original wraps
--- in pairs.
local function step_layout(mask, current, left)
    if left then
        if mask & 8 ~= 0 then
            return ({ [0] = 3, 2, 2, 1 })[current]
        end
        local next = current + 1
        if current == 2 or mask & (1 << next) == 0 then
            return current
        end
        return next
    elseif mask & 8 ~= 0 then
        return ({ [0] = 0, 3, 1, 0 })[current]
    elseif current ~= 0 and mask & (1 << (current - 1)) ~= 0 then
        return current - 1
    end
    return current
end

--- One frame of the objective label (`map_display_draw`'s tail): grow the
--- square two counts a frame, shrink it one a frame, then hold it hidden
--- for twenty frames before the next blink.
local function step_label(api)
    s.label_frame = nil
    if s.label_state <= 1 or s.objective == 0 then
        return
    end
    local frame = (s.label_count // 3) & 7
    if s.label_state == 2 then
        api:sound("map_label")
        s.label_state = 3
    end
    if s.label_state == 3 then
        s.label_count = s.label_count + 2
        if s.label_count >= 0x17 then
            s.label_state = 4
            s.label_count = 0x17
        end
    elseif s.label_state == 4 then
        s.label_count = s.label_count - 1
        if s.label_count == 0 then
            s.label_state = 5
            s.label_count = 0x14
        end
    elseif s.label_state == 5 then
        frame = 0
        s.label_count = s.label_count - 1
        if s.label_count == 0 then
            s.label_state = 2
        end
    end
    s.label_frame = frame
end

--- One frame of the full display's zoom (`map_display_animate`): the
--- pulse, then the zoom state machine, then the label. Returns true when
--- the close finished and the browser is back.
local function animate(api)
    -- The pulse runs while the plan is the player's map group's.
    if s.tables.groups[s.area] == s.map_group then
        if s.pulse_state == 0 then
            s.pulse_ramp = 0x50
            s.pulse_state = 1
        elseif s.pulse_state == 1 then
            s.pulse_ramp = s.pulse_ramp + 2
            if s.pulse_ramp > 0xA0 then
                s.pulse_state = 2
            end
        else
            s.pulse_ramp = s.pulse_ramp - 2
            if s.pulse_ramp < 0x50 then
                s.pulse_state = 1
            end
        end
        s.pulse = (s.pulse_ramp >> 3) & 0x1F
    end
    local zoom = s.zoom
    if zoom == 0 then
        -- A one-pixel-wide sliver, 200/4096 tall.
        s.scale_x, s.scale_y = 1, 200
        s.additive = false
        api:sound("map_zoom")
        zoom = 1
    end
    if zoom == 1 then
        s.at_rest = false
        s.zoom = 2
    elseif zoom == 2 then
        -- Stretch across, three times wider a frame.
        s.scale_x = s.scale_x * 3
        if s.scale_x >= 0xFFF then
            s.scale_x = 0xFFF
            s.zoom = 3
        end
    elseif zoom == 3 then
        -- Open up past full height ...
        s.scale_y = s.scale_y + 900
        if s.scale_y >= 0x15DC then
            s.scale_y = 0x15DC
            s.zoom = 4
        end
    elseif zoom == 4 then
        -- ... and settle back to it.
        s.scale_y = s.scale_y - 300
        if s.scale_y <= FIX_ONE then
            s.scale_x, s.scale_y = FIX_ONE, FIX_ONE
            s.at_rest = true
            s.hold = 8
            s.zoom = 5
        end
    elseif zoom == 5 then
        s.hold = s.hold - 1
        if s.hold == 0 then
            s.additive = true
            s.hold = LABEL_DELAY
            s.zoom = 6
        end
    elseif zoom == 6 then
        -- Wait to start the objective label.
        if s.label_state == 1 then
            s.hold = s.hold - 1
            if s.hold == 0 then
                s.label_state = 2
            end
        end
    elseif zoom == 7 or zoom == 8 then
        if zoom == 7 then
            -- The close: back to the sliver layout at full width ...
            s.additive = false
            s.at_rest = false
            s.scale_x = 0xFFF
            s.sweep = 0
            if s.label_state ~= 0 then
                s.label_state = 1
            end
            s.zoom = 8
        end
        -- ... collapse to a line ...
        s.scale_y = s.scale_y - 0x28A
        if s.scale_y <= 1 then
            s.scale_y = 200
            s.zoom = 9
        end
    elseif zoom == 9 then
        -- ... and shrink it to a dot.
        s.scale_x = s.scale_x // 2
        if s.scale_x <= 1 then
            s.scale_x = 1
            s.label_frame = nil
            return true
        end
    end
    step_label(api)
    return false
end

--- One frame of the full display (`map_display_update` and its share of
--- `menu_map_state_machine`).
local function update_display(api, input)
    -- Cancel while it closes leaves the tab outright: the browser's own
    -- cancel test sees the same press.
    if input.cancel and s.zoom >= 7 then
        api:sound("cancel")
        api:exit()
        return
    end
    if animate(api) then
        s.zoomed = false
        s.zoom = 0
        s.closing = true
        s.sweep_row = nil
        -- The frame's own confirm test still sees the zoom state the close
        -- left (0), so its cue plays without effect. (A cancel never gets
        -- here: the close's cancel leaves the tab above.)
        if input.confirm then
            api:sound("map_zoom")
        end
        return
    end
    -- The sweep draws while the display rests at state 6, a counter step a
    -- frame; the byte wraps every 256 frames.
    if s.zoom == 6 then
        s.sweep_row = s.sweep
        s.sweep = (s.sweep + 1) & 0xFF
    else
        s.sweep_row = nil
    end
    if input.confirm and (s.zoom == 0 or s.zoom > 6) then
        -- Re-open from the close.
        if s.zoom == 8 then
            s.zoom = 3
        elseif s.zoom == 9 then
            s.zoom = 2
        end
        api:sound("map_zoom")
    end
    if input.cancel then
        s.zoom = 7
        api:sound("cancel")
    end
end

local function handle_input(api, input)
    if input.cancel then
        api:sound("cancel")
        api:exit()
    elseif input.confirm then
        -- The display's first frame is the next one; its texture load arms
        -- the objective label when the plan is the objective's area.
        s.zoomed = true
        s.zoom = 0
        s.pulse_state = 0
        if (s.objective == 2 and s.area == 0) or (s.objective == 4 and s.area == 6) then
            s.label_state = 1
        else
            s.label_state = 0
        end
    elseif input.up or input.down then
        local room = room_index()
        local flags = move_flags(api, s.layout, room)
        if input.up and flags & 1 ~= 0 then
            room = room + 1
        elseif input.down and flags & 2 ~= 0 then
            room = room - 1
        else
            return
        end
        s.room[s.layout] = room
        api:sound("cursor")
        refresh_area()
    elseif input.left or input.right then
        local mask = layout_mask(api)
        local next = step_layout(mask, s.layout, input.left)
        -- With all four layouts every press blips, even one that stays put.
        if next ~= s.layout or mask & 8 ~= 0 then
            api:sound("cursor")
        end
        if next ~= s.layout then
            s.layout = next
            refresh_area()
        end
    end
end

--- One frame: advance the blink phase, then take the frame's one menu input.
--- The first update is the browser's first drawn frame, whose counter step
--- is its first frame's `1`.
function M.update(api, input)
    if not s.started then
        s.started = true
    else
        s.ticks = s.ticks + 1
    end
    s.closing = false
    if s.zoomed then
        update_display(api, input)
    else
        handle_input(api, input)
    end
    api:set("zoomed", s.zoomed and 1 or 0)
end

-- ---- drawing ---------------------------------------------------------------

--- One `colorMul`-scaled channel: the texel times `colorMul * 2 / 255`,
--- clamped at 1.0 and rounded to nearest, so 0x80 is unity.
local function tint(channel, mul)
    local factor = mul * 2
    if factor > 255 then
        factor = 255
    end
    return (channel * factor + 127) // 255
end

local function rgb(colour)
    return colour >> 16, (colour >> 8) & 0xFF, colour & 0xFF
end

--- The browser lut of `mul`: index 0 transparent, the rest tinted.
local function browser_lut(api, art, mul)
    s.luts = s.luts or {}
    local key = art .. ":" .. mul
    if not s.luts[key] then
        local palette = api:page_palette(art)
        local lut = {}
        for index = 1, 255 do
            local r, g, b = rgb(palette[index])
            lut[index] = (tint(r, mul) << 16) | (tint(g, mul) << 8) | tint(b, mul)
        end
        s.luts[key] = lut
    end
    return s.luts[key]
end

--- Blit one browser sprite. The original's page search rejects a sprite
--- whose rect leaves the loaded page, which is what makes the wrapped
--- floor-glyph `texV` disappear. `whole` lets an HD page cover the whole
--- rect, for the labels it redraws in its own lettering.
local function blit(api, art, rect, mul, ox, oy, whole)
    local width, height = api:art_size(art)
    if rect.u + rect.w > width or rect.v + rect.h > height then
        return
    end
    api:page_lut(art, rect.u, rect.v, rect.w, rect.h, ox + rect.x, oy + rect.y, browser_lut(api, art, mul), whole)
end

local function with(rect, field, value)
    local copy = {}
    for key, item in pairs(rect) do
        copy[key] = item
    end
    copy[field] = value
    return copy
end

--- The map browser: the loaded layout art's base window, the marker of every
--- available layout (the current one at full brightness, the rest at half),
--- the current area's name row, the floor's number glyph and the arrows.
--- `dim` is the full display's dimmed copy behind the plan.
local function draw_browser(api, ox, oy, dim)
    -- The original loads one page for the whole browser: the file of the
    -- availability mask's highest set layout bit.
    local mask = layout_mask(api)
    local top = 0
    for layout = 3, 0, -1 do
        if mask & (1 << layout) ~= 0 then
            top = layout
            break
        end
    end
    local art = page(api, TAB_STEMS[top], PAGE_WARNING)
    if not art then
        return
    end
    local base_mul = dim and MUL_DIM_BASE or MUL_FULL
    local label_mul = dim and MUL_DIM_SPRITE or MUL_FULL
    blit(api, art, BASE_RECT, base_mul, ox, oy)
    for layout = 0, 3 do
        if mask & (1 << layout) ~= 0 then
            local mul = layout == s.layout and MUL_FULL or MUL_HALF
            blit(api, art, SPRITES[layout], dim and MUL_DIM_SPRITE or mul, ox, oy)
        end
    end
    blit(api, art, with(SPRITES[4], "v", (s.layout * 0x10 + 0x48) & 0xFF), label_mul, ox, oy, true)
    local offset = s.tables.layout_offset[s.layout] or 0
    local glyph_v = (((6 - room_index() - offset) & 0xFF) * 0x10) & 0xFF
    blit(api, art, with(SPRITES[5], "v", glyph_v), label_mul, ox, oy, true)
    -- `MAP_HL_COUNT`: bit 4 set for the dim half of its 64-frame cycle.
    local blink = ((s.ticks + 1) & 0x30) ~= 0
    local flags = move_flags(api, s.layout, room_index())
    for index, bit in ipairs({ 1, 2 }) do
        local sprite = SPRITES[5 + index]
        if flags & bit ~= 0 then
            blit(api, art, with(sprite, "u", (sprite.u + (blink and 8 or 0)) & 0xFF), label_mul, ox, oy)
        end
    end
end

--- A fix16.12 product rounded toward zero, as the original's
--- `(x + ((x >> 31) & 0xfff)) >> 12`.
local function fix(value)
    if value < 0 then
        return -((-value) >> 12)
    end
    return value >> 12
end

--- A zoom quad's screen rect (`AddSprite_Ex`): `quad` scaled about its
--- pivot at the zoom origin; nil once it has no pixels.
local function zoom_rect(quad, width, ox, oy)
    local x, y = ox + ZOOM_X + quad.x, oy + ZOOM_Y + quad.y
    local x0 = x - fix(quad.px * s.scale_x)
    local y0 = y - fix(quad.py * s.scale_y)
    local x1 = fix((width - quad.px) * s.scale_x) + x - 1
    local y1 = fix((quad.h - quad.py) * s.scale_y) + y - 1
    if x1 < x0 or y1 < y0 then
        return nil
    end
    return { x0, y0, x1 - x0 + 1, y1 - y0 + 1 }
end

--- A page's palette and STP set, loaded once.
local function colours(api, page)
    s.colours = s.colours or {}
    local entry = s.colours[page]
    if not entry then
        entry = { api:page_palette(page), api:page_stp(page) or {} }
        s.colours[page] = entry
    end
    return entry[1], entry[2]
end

--- Whether a palette entry is drawn: the PSX keys only a CLUT word of 0, so
--- a black entry with the STP bit still draws (adding nothing).
local function drawn(palette, stp, index)
    return palette[index] ~= 0 or stp[index] == true
end

--- The grid page's lookup: every drawn entry as it is.
local function grid_lut(api, page)
    if not s.grid_lut then
        local palette, stp = colours(api, page)
        local lut = {}
        for index = 0, 255 do
            if drawn(palette, stp, index) then
                lut[index] = palette[index]
            end
        end
        s.grid_lut = lut
    end
    return s.grid_lut
end

--- The plan page's lookups (`map_display_load_textures`), each keyed by
--- palette index: the plan's opaque entries, its semi-transparent (STP)
--- entries, and the highlight page, which blends whole and is drawn with
--- the red `colorMul` of `pulse`. Without the area's map the outline and
--- symbols and every unvisited room are cleared; with it an unvisited room
--- keeps only its door marker and border (its six entries' first and fifth).
--- The player's room loses its fill to the highlight page, which holds only
--- that room (fill in red) and the entries past the room table.
local function plan_luts(api, page)
    local palette, stp = colours(api, page)
    local group = s.tables.groups[s.area] or 0
    local count = s.tables.room_counts[group] or 0
    local known = area_known(api, s.area)
    local cleared, highlight = {}, {}
    if not known then
        for index = SYMBOL_FIRST, SYMBOL_LAST do
            cleared[index] = true
        end
    end
    for index = 1, SYMBOL_LAST do
        highlight[index] = false
    end
    for room = 0, count - 1 do
        local base = ROOM_FIRST + room * 6
        local visited = room_visited(api, group, room)
        local player = visited and group == s.map_group and room == s.map_room
        for entry = 0, 5 do
            local fill = entry >= 1 and entry <= 3
            if not visited then
                cleared[base + entry] = cleared[base + entry] or not known or fill
            elseif player and fill then
                cleared[base + entry] = true
            end
            if not player then
                highlight[base + entry] = false
            elseif fill then
                highlight[base + entry] = HIGHLIGHT_RED << 16
            end
        end
    end
    local red = s.pulse << 2
    local opaque, blended, glow = {}, {}, {}
    for index = 0, 255 do
        local colour = palette[index]
        if not cleared[index] and drawn(palette, stp, index) then
            if stp[index] then
                blended[index] = colour
            else
                opaque[index] = colour
            end
        end
        local lit = highlight[index]
        if lit == nil and drawn(palette, stp, index) then
            lit = colour
        end
        if lit then
            local r, g, b = rgb(lit)
            glow[index] = (tint(r, red) << 16) | (g << 8) | b
        end
    end
    return opaque, blended, glow
end

--- The label's lookup: every drawn entry of the plan page as it is.
local function label_lut(api, page)
    s.label_luts = s.label_luts or {}
    if not s.label_luts[page] then
        local palette, stp = colours(api, page)
        local lut = {}
        for index = 0, 255 do
            if drawn(palette, stp, index) then
                lut[index] = palette[index]
            end
        end
        s.label_luts[page] = lut
    end
    return s.label_luts[page]
end

--- One quad of a page through `lut`.
local function quad_lut(api, page, sx, sy, w, h, rect, lut, blend)
    api:page_lut(page, sx, sy, w, h, rect[1], rect[2], lut, false, {
        dw = rect[3],
        dh = rect[4],
        blend = blend,
    })
end

--- The full display: the dimmed browser, the grid's two halves half-blended
--- over it, the plan (its opaque entries over, its STP entries blended),
--- the highlight page, and while the display rests the sweep's row. Returns
--- the screen rect the quads cover (nil when nothing showed).
local function draw_display(api, ox, oy)
    draw_browser(api, ox, oy, true)
    local area = s.area < 10 and s.area or 10
    local plan = page(api, AREA_STEMS[area], PLAN_WARNING)
    local grid = page(api, BACKDROP_STEM, BACKDROP_WARNING)
    local blend = s.additive and "add" or "half"
    local bounds
    local function cover(rect)
        if rect == nil then
            return
        end
        local edges = { rect[1], rect[2], rect[1] + rect[3], rect[2] + rect[4] }
        if bounds == nil then
            bounds = edges
            return
        end
        for side = 1, 2 do
            if edges[side] < bounds[side] then
                bounds[side] = edges[side]
            end
            if edges[side + 2] > bounds[side + 2] then
                bounds[side + 2] = edges[side + 2]
            end
        end
    end
    api:clip(0, oy + VIEWER_TOP, api:width(), VIEWER_BOTTOM - VIEWER_TOP)
    local grid_w = s.at_rest and GRID_W_OPEN or GRID_W_ZOOM
    if grid then
        local lut = grid_lut(api, grid)
        for _, quad in ipairs(GRID_QUADS) do
            local rect = zoom_rect(quad, grid_w, ox, oy)
            if rect then
                quad_lut(api, grid, 0, 0, grid_w, quad.h, rect, lut, "half")
                cover(rect)
            end
        end
    end
    local opaque, blended, glow
    if plan then
        opaque, blended, glow = plan_luts(api, plan)
        local rect = zoom_rect(PLAN_QUAD, PLAN_QUAD.w, ox, oy)
        if rect then
            -- Only the additive blend keeps the PS1's per-texel STP split;
            -- the half blend covers every drawn texel.
            local solid = not s.additive and blend or nil
            quad_lut(api, plan, 0, 0, PLAN_QUAD.w, PLAN_QUAD.h, rect, opaque, solid)
            quad_lut(api, plan, 0, 0, PLAN_QUAD.w, PLAN_QUAD.h, rect, blended, blend)
            quad_lut(api, plan, 0, 0, PLAN_QUAD.w, PLAN_QUAD.h, rect, glow, blend)
            cover(rect)
        end
    end
    -- The sweep's rows: a texV past 0x8D is inside the 0x88-based page.
    local counter = s.sweep_row
    if counter ~= nil and counter < SWEEP_END then
        local row = counter >> 1
        local y = oy + row + 24
        if grid and (row - 0x78) & 0xFF > 0x8D then
            local lut = grid_lut(api, grid)
            for _, x in ipairs({ 113, 33 }) do
                quad_lut(api, grid, 0, row, GRID_W_OPEN, 1, { ox + x, y, GRID_W_OPEN, 1 }, lut)
            end
        end
        if plan and (row - 0x7C) & 0xFF > 0x8D then
            local rect = { ox + 49, y, PLAN_QUAD.w, 1 }
            quad_lut(api, plan, 0, row - 4, PLAN_QUAD.w, 1, rect, opaque)
            quad_lut(api, plan, 0, row - 4, PLAN_QUAD.w, 1, rect, blended, "add")
            quad_lut(api, plan, 0, row - 4, PLAN_QUAD.w, 1, rect, glow, "add")
        end
    end
    api:clip()
    if plan and s.label_frame ~= nil then
        local pos = LABEL_POS[s.objective // 3] or LABEL_POS[0]
        local rect = { ox + pos[1], oy + pos[2], LABEL_SIZE, LABEL_SIZE }
        quad_lut(api, plan, s.label_frame * LABEL_SIZE, LABEL_V, LABEL_SIZE, LABEL_SIZE, rect, label_lut(api, plan))
    end
    if bounds == nil then
        return nil
    end
    local top, bottom = bounds[2] - oy, bounds[4] - oy
    top = top > VIEWER_TOP and top or VIEWER_TOP
    bottom = bottom < VIEWER_BOTTOM and bottom or VIEWER_BOTTOM
    return { bounds[1] - ox, top, bounds[3] - bounds[1], bottom - top }
end

--- Draw the tab over the frozen menu frame the engine already drew.
function M.draw(api)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    local rect = BROWSER_RECT
    if s.zoomed and s.zoom ~= 0 then
        rect = draw_display(api, ox, oy) or BROWSER_RECT
    elseif s.closing then
        draw_browser(api, ox, oy, true)
    elseif s.started then
        draw_browser(api, ox, oy)
    end
    -- The original's depth puts the status screen's frame decorations in
    -- front of the map's sprites, so the panel's dithered right-hand edge
    -- shows over the composite.
    Panel.draw_frames_over(api, ox, oy, api:inventory_capacity(), rect)
end

return M
