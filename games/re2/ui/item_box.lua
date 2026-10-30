-- The item box, as text, over `ui/main_menu.lua`'s inventory list (see there
-- for why). Up and down walk the inventory; confirm moves to the box, where up
-- and down walk the box and confirm swaps the two slots. Cancel steps back, and
-- START leaves.
--
-- It keeps the engine's contract with RE1's screen: the `cursor` argument, and
-- publishing `cursor`, plus `swap`, `swap_box` and `swap_slot` on the frame of
-- a swap.

local Grid = require("lib/menu_grid")

local M = {}

local SLOTS_TOTAL = 48
local ROWS = 8
local LIST_X, LIST_Y, ROW_H = 186, 40, 18

local s = {}
M.state = s

function M.open(api, args)
    s.cursor = args.cursor or Grid.FIRST_SLOT_CURSOR
    s.slots = api:inventory_capacity()
    s.box = 0
    s.in_box = false
    api:set("cursor", s.cursor)
    api:set("swap", false)
end

function M.cursor_shown(_api, _shown) end

local function step_inventory(api, delta)
    local slot = Grid.slot_of_cursor(s.cursor) or 0
    s.cursor = Grid.cursor_of_slot((slot + delta) % s.slots)
    api:sound("cursor")
end

local function step_box(api, delta)
    s.box = (s.box + delta) % SLOTS_TOTAL
    api:sound("cursor")
end

local function swap(api)
    local slot = Grid.slot_of_cursor(s.cursor) or 0
    if api:item_box(s.box) == 0 and api:inventory(slot) == 0 then
        return
    end
    api:sound("decide")
    api:set("swap", true)
    api:set("swap_box", s.box)
    api:set("swap_slot", slot)
    s.in_box = false
end

function M.update(api, input)
    api:set("swap", false)
    if input.start or (input.cancel and not s.in_box) then
        api:sound("cancel")
        api:exit()
        return
    end
    if s.in_box then
        if input.cancel then
            api:sound("cancel")
            s.in_box = false
        elseif input.up then
            step_box(api, -1)
        elseif input.down then
            step_box(api, 1)
        elseif input.confirm then
            swap(api)
        end
    elseif input.up then
        step_inventory(api, -1)
    elseif input.down then
        step_inventory(api, 1)
    elseif input.confirm then
        api:sound("decide")
        s.in_box = true
    end
    api:set("cursor", s.cursor)
end

function M.draw(api)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:text(ox + LIST_X, oy + 16, "Item box")
    local first = s.box - ROWS // 2
    if first > SLOTS_TOTAL - ROWS then
        first = SLOTS_TOTAL - ROWS
    end
    if first < 0 then
        first = 0
    end
    for row = 0, ROWS - 1 do
        local slot = first + row
        local item, quantity = api:item_box(slot)
        local y = oy + LIST_Y + row * ROW_H
        if s.in_box and slot == s.box then
            api:fill_rect(ox + LIST_X - 10, y + 3, 6, 6, 255, 255, 255)
        end
        if item == 0 then
            api:text(ox + LIST_X, y, "-")
        else
            api:glyphs(ox + LIST_X, y, api:item_name_bytes(item) or "", { fit = 104 })
            if quantity > 0 then
                api:text(ox + LIST_X + 108, y, tostring(quantity))
            end
        end
    end
end

return M
