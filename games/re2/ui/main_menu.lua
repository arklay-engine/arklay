-- The status screen, as text. The engine runs the original's item page
-- (`src/game/status2.rs`): the slots, the command menu (EQUIP or USE, CHECK,
-- COMBN), the combine target and the tab bar. This script only draws what
-- `api:re2_items()` reports, until RE2's status art is converted.
--
-- It keeps the engine's contract with the pause menu: `open`, `tick`,
-- `update`, `tick_monitor`, `view_closed`, `reselect`, `move_cursor`,
-- `present` and `draw`, publishing `cursor`, `slot` and `selected_item`, and
-- `event` and `event_id`.

local M = {}

local EVENT_NONE = 0
local PERSONAL_SLOT = 10
local SUB_MENU, SUB_COMMAND = 1, 2
local COMMAND_COMBINE, COMMAND_USE = 1, 3
local FIRST_AMMO = 0x14
local PAGE_TABS = 2
local TABS = { "FILE", "MAP", "ITEMS", "EXIT" }

local GRID_X, GRID_Y, COLUMN_W, ROW_H = 24, 56, 140, 18
local CONDITION_Y = 210

local s = { slots = 8, show_cursor = true }
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

function M.present(_api, show_cursor, _show_name, _backdrop)
    s.show_cursor = show_cursor
end

local function slot_position(ox, oy, slot)
    if slot == PERSONAL_SLOT then
        return ox + GRID_X + COLUMN_W * 2, oy + GRID_Y
    end
    return ox + GRID_X + (slot % 2) * COLUMN_W, oy + GRID_Y + (slot // 2) * ROW_H
end

local function draw_slot(api, ox, oy, slot, marker)
    local x, y = slot_position(ox, oy, slot)
    if marker then
        api:fill_rect(x - 10, y + 3, 6, 6, marker, marker, 255)
    end
    local item, quantity = api:re2_slot(slot)
    if item == 0 then
        api:text(x, y, "-")
        return
    end
    api:glyphs(x, y, api:item_name_bytes(item) or "", { fit = 92 })
    if item < 0x20 and item ~= 1 then
        api:text(x + 96, y, tostring(quantity))
    end
    if item == api:equipped() then
        api:text(x + 120, y, "E")
    end
end

local function condition(health)
    if health > 120 then
        return "Fine"
    elseif health > 60 then
        return "Caution"
    end
    return "Danger"
end

local function draw_menu(api, ox, oy, page)
    local item = api:re2_slot(page.cursor)
    local labels = {
        [COMMAND_USE] = item < FIRST_AMMO and "EQUIP" or "USE",
        [2] = "CHECK",
        [COMMAND_COMBINE] = "COMBN",
    }
    local x, y = ox + GRID_X + COLUMN_W * 2, oy + GRID_Y + 2 * ROW_H
    for row, command in ipairs({ COMMAND_USE, 2, COMMAND_COMBINE }) do
        local ry = y + (row - 1) * ROW_H
        if page.sub == SUB_MENU and command == page.command_cursor then
            api:fill_rect(x - 10, ry + 3, 6, 6, 255, 255, 255)
        end
        api:text(x, ry, labels[command])
    end
end

function M.draw(api)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0)
    local page = api:re2_items() or { page = 3, sub = 0, cursor = 0, target = 0, tab = 2 }
    for tab, label in ipairs(TABS) do
        local x = ox + GRID_X + (tab - 1) * 64
        if page.page == PAGE_TABS and page.tab == tab - 1 then
            api:fill_rect(x - 10, oy + 19, 6, 6, 255, 255, 255)
        end
        api:text(x, oy + 16, label)
    end
    local targeting = page.sub == SUB_COMMAND and page.command == COMMAND_COMBINE and page.step == 0
    for slot = 0, s.slots - 1 do
        local marker = nil
        if s.show_cursor and page.page ~= PAGE_TABS then
            if targeting and slot == page.target then
                marker = 96
            elseif slot == page.cursor then
                marker = 255
            end
        end
        draw_slot(api, ox, oy, slot, marker)
    end
    local personal = nil
    if s.show_cursor and page.page ~= PAGE_TABS then
        if targeting and page.target == PERSONAL_SLOT then
            personal = 96
        elseif page.cursor == PERSONAL_SLOT then
            personal = 255
        end
    end
    draw_slot(api, ox, oy, PERSONAL_SLOT, personal)
    if page.sub == SUB_MENU or page.sub == SUB_COMMAND then
        draw_menu(api, ox, oy, page)
    end
    api:text(ox + GRID_X, oy + CONDITION_Y, "Condition: " .. condition(api:player_health()))
end

return M
