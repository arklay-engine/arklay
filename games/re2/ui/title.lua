-- The title screen, as text. RE1's (`games/re1/ui`) draws from RE1's art; RE2
-- has this stand-in until its own is converted. Up and down pick New Game or
-- Load Game, and confirm goes there.
--
-- args.folder_saves: a registered save screen lists the whole save folder, so
-- every loadable file counts as a save rather than only the eight slots.

local M = {}

local LEFT, TOP, ROW_H = 120, 120, 20

local s = {}
M.state = s

local function any_saves(api, args)
    for _, entry in ipairs(api:saves()) do
        if entry.loadable and (args.folder_saves or entry.autosave) then
            return true
        end
    end
    if not args.folder_saves then
        for _, present in ipairs(api:save_slots()) do
            if present then
                return true
            end
        end
    end
    return false
end

function M.open(api, args)
    s.any_saves = any_saves(api, args)
    s.selection = s.any_saves and 2 or 1
end

function M.update(api, input)
    if s.any_saves and (input.up or input.down or input.left or input.right) then
        s.selection = 3 - s.selection
        api:sound("cursor")
    elseif input.confirm or input.start then
        api:sound("decide")
        api:exit(s.selection == 2 and "save_load" or "char_select")
    end
end

function M.draw(api)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0)
    api:text(ox + LEFT - 16, oy + 70, "Resident Evil 2")
    api:text(ox + LEFT, oy + TOP, "New Game")
    if s.any_saves then
        api:text(ox + LEFT, oy + TOP + ROW_H, "Load Game")
    end
    api:fill_rect(ox + LEFT - 10, oy + TOP + (s.selection - 1) * ROW_H + 3, 6, 6, 255, 255, 255)
end

return M
