-- The pause menu's inventory, as text. RE1's screen (`games/re1/ui`) draws
-- from RE1's art and reads RE1's item ids, so RE2 has this stand-in until its
-- status art is converted: a list of the slots, the equipped weapon and the
-- player's condition. Confirm equips a weapon or uses an item, cancel or START
-- closes.
--
-- It keeps the engine's contract with that screen: `open`, `tick`, `update`,
-- `tick_monitor`, `view_closed`, `reselect`, `move_cursor`, `present` and
-- `draw`, publishing `cursor`, `slot` and `selected_item`, and `event` and
-- `event_id` (`1` closes the menu).

local Grid = require("lib/menu_grid")

local M = {}

local EVENT_NONE, EVENT_CLOSE = 0, 1

local FIRST_WEAPON, LAST_WEAPON = 0x01, 0x13

local LIST_X, LIST_Y, ROW_H = 24, 40, 18
local CONDITION_Y = 200

local s = {}
M.state = s

local function emit(api, kind)
    api:set("event", kind)
    api:set("event_id", 0)
end

local function refresh(api)
    local slot = Grid.slot_of_cursor(s.cursor) or 0
    s.selected_item = api:inventory(slot)
    api:select_item(s.selected_item)
    api:set("cursor", s.cursor)
    api:set("slot", slot)
    api:set("selected_item", s.selected_item)
    api:set("ready", true)
end

function M.open(api, _args)
    api:set("canvas", 1)
    s.slots = api:inventory_capacity()
    s.cursor = Grid.FIRST_SLOT_CURSOR
    s.show_cursor = true
    emit(api, EVENT_NONE)
    refresh(api)
end

function M.tick(api)
    emit(api, EVENT_NONE)
    refresh(api)
end

function M.tick_monitor(_api) end

function M.view_closed(_api) end

function M.reselect(api)
    refresh(api)
end

function M.move_cursor(api, cursor)
    s.cursor = cursor
    refresh(api)
end

function M.present(_api, show_cursor, _show_name, _backdrop)
    s.show_cursor = show_cursor
end

local function confirm(api)
    local item = s.selected_item
    if item == 0 then
        return
    end
    if item >= FIRST_WEAPON and item <= LAST_WEAPON then
        api:equip(api:equipped() ~= item and item or 0)
        api:sound("decide")
        return
    end
    if api:use_item(Grid.slot_of_cursor(s.cursor) or 0) then
        api:sound("decide")
    else
        api:sound("cancel")
    end
end

local function step(api, delta)
    local slot = Grid.slot_of_cursor(s.cursor) or 0
    s.cursor = Grid.cursor_of_slot((slot + delta) % s.slots)
    api:sound("cursor")
end

function M.update(api, input)
    emit(api, EVENT_NONE)
    if input.start or input.cancel then
        emit(api, EVENT_CLOSE)
    elseif input.up then
        step(api, -1)
    elseif input.down then
        step(api, 1)
    elseif input.confirm then
        confirm(api)
    end
    refresh(api)
end

local function draw_row(api, x, y, item, quantity, selected)
    if selected then
        api:fill_rect(x - 10, y + 3, 6, 6, 255, 255, 255)
    end
    if item == 0 then
        api:text(x, y, "-")
        return
    end
    api:glyphs(x, y, api:item_name_bytes(item) or "", { fit = 104 })
    if quantity > 0 then
        api:text(x + 108, y, tostring(quantity))
    end
    if item == api:equipped() then
        api:text(x + 132, y, "E")
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

function M.draw(api)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0)
    api:text(ox + LIST_X, oy + 16, "Items")
    local selected = Grid.slot_of_cursor(s.cursor)
    for slot = 0, s.slots - 1 do
        local item, quantity = api:inventory(slot)
        draw_row(api, ox + LIST_X, oy + LIST_Y + slot * ROW_H, item, quantity,
            s.show_cursor and slot == selected)
    end
    api:text(ox + LIST_X, oy + CONDITION_Y, "Condition: " .. condition(api:player_health()))
end

return M
