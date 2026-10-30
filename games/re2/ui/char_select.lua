-- The new-game choice, as text (see `ui/title.lua` for why): Leon or Claire,
-- in Scenario A or B. Up and down pick, confirm starts the game and cancel goes
-- back to the title. The exit's character carries the scenario in bit 1, which
-- is `--character`'s numbering.

local M = {}

local CHOICES = {
    { label = "Leon, Scenario A", character = 0 },
    { label = "Claire, Scenario A", character = 1 },
    { label = "Leon, Scenario B", character = 2 },
    { label = "Claire, Scenario B", character = 3 },
}

local LEFT, TOP, ROW_H = 88, 90, 20

local s = {}
M.state = s

function M.open(_api)
    s.selection = 1
end

function M.update(api, input)
    if input.up or input.left then
        s.selection = (s.selection - 2) % #CHOICES + 1
        api:sound("cursor")
    elseif input.down or input.right then
        s.selection = s.selection % #CHOICES + 1
        api:sound("cursor")
    elseif input.confirm then
        api:sound("decide")
        api:exit("new_game", CHOICES[s.selection].character)
    elseif input.cancel then
        api:sound("cancel")
        api:exit("title")
    end
end

function M.draw(api)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0)
    api:text(ox + LEFT, oy + 60, "New Game")
    for index, choice in ipairs(CHOICES) do
        api:text(ox + LEFT, oy + TOP + (index - 1) * ROW_H, choice.label)
    end
    api:fill_rect(ox + LEFT - 10, oy + TOP + (s.selection - 1) * ROW_H + 3, 6, 6, 255, 255, 255)
end

return M
