-- The guardhouse's number panel (room 408, src/engine/passcode_panel.rs):
-- a 3x3 pad whose keys each flip their own lamp and their orthogonal
-- neighbours'; lighting all nine solves it. bot:scene().panel reads it
-- ({active, x, y, lamps}: the cursor's column and row, the nine lamps row
-- by row). The bot works out which keys light the pad (each key at most
-- once; the order does not matter), walks the cursor to the first and
-- presses it, one pad edge every other tick, and plans again after each
-- press. The prompt that opens the pad is the scene handler's (Yes).

BotPanel = BotPanel or {}

local SIDE = 3
local KEYS = SIDE * SIDE

local last_input = -1

-- The lamps (booleans, 1-based row-major) after pressing key `key`
-- (0-based).
local function press(lamps, key)
    local out = {}
    for i = 1, KEYS do
        out[i] = lamps[i]
    end
    local row, col = key // SIDE, key % SIDE
    local function flip(r, c)
        if r >= 0 and r < SIDE and c >= 0 and c < SIDE then
            local i = r * SIDE + c + 1
            out[i] = not out[i]
        end
    end
    flip(row, col)
    flip(row - 1, col)
    flip(row + 1, col)
    flip(row, col - 1)
    flip(row, col + 1)
    return out
end

-- The first key (0-based) of the smallest key set that lights every
-- lamp, or nil when none does.
function BotPanel.next_key(lamps)
    local best, best_count = nil, nil
    for mask = 0, (1 << KEYS) - 1 do
        local state, count, first = lamps, 0, nil
        for key = 0, KEYS - 1 do
            if (mask >> key) & 1 == 1 then
                state = press(state, key)
                count = count + 1
                first = first or key
            end
        end
        local lit = true
        for i = 1, KEYS do
            lit = lit and state[i]
        end
        if lit and count > 0 and (not best_count or count < best_count) then
            best, best_count = first, count
        end
    end
    return best
end

-- One tick while the panel is up. Returns true while it owns the pad.
function BotPanel.step(bot, scene)
    local view = scene.panel
    if not view then
        return false
    end
    if not view.active then
        -- The prompt is the scene handler's; the success animation needs
        -- nothing.
        return not scene.message
    end
    if bot:tick() - last_input < 2 then
        return true
    end
    local key = BotPanel.next_key(view.lamps)
    if not key then
        return true
    end
    local row, col = key // SIDE, key % SIDE
    local edge
    if view.y < row then
        edge = "down_pressed"
    elseif view.y > row then
        edge = "up_pressed"
    elseif view.x < col then
        edge = "right_pressed"
    elseif view.x > col then
        edge = "left_pressed"
    else
        edge = "confirm"
        bot:log(string.format("panel: press key %d", key + 1))
    end
    bot:set_input({ [edge] = true })
    last_input = bot:tick()
    return true
end

return BotPanel
