-- The lab computer terminal (room 506, games/re1/ui/computer_lab.lua):
-- log in, pick a cell door and type the command that releases it.
--
-- The screen is a 4x8 keyboard: cell `code` for the letters A..V (code 1
-- is A), cell `code + 1` for W..Z, ENTER at cell 0x17, BACKSPACE at 0x1D;
-- the two cells right of BACKSPACE are covered. bot:scene().computer reads
-- its state machine ({stage, sub, cell, menu_row, typed}); the bot answers
-- with one UI edge every other tick.
--
-- The credentials and the lock bits each door row releases are in
-- bot/constants.lua (TERMINAL): the bot releases the first door still
-- locked (in RE1, 503's door to 504, then 508's door to 509).

BotTerminal = BotTerminal or {}

local COLS = 8
local T = BotConst.TERMINAL
local CELL_ENTER = T.enter
local CELL_BACKSPACE = T.backspace

local last_input = -1

-- The keyboard cell of letter `ch` ("A".."Z").
local function letter_cell(ch)
    local code = ch:byte() - ("A"):byte() + 1
    return code <= 22 and code or code + 1
end

-- The pad edge that moves the cursor from `cell` toward `target`, or nil
-- once it is there. Columns wrap; the bottom row's covered cells are
-- avoided by leaving that row before moving sideways.
local function step_toward(cell, target)
    local row, col = cell // COLS, cell % COLS
    local trow, tcol = target // COLS, target % COLS
    if col ~= tcol then
        if row == 3 then
            return "up_pressed"
        end
        local right = (tcol - col) % COLS
        return right <= COLS - right and "right_pressed" or "left_pressed"
    end
    if row < trow then
        return "down_pressed"
    elseif row > trow then
        return "up_pressed"
    end
    return nil
end

-- The edge that types `word` into the field holding `typed` chars, then
-- submits it.
local function type_word(view, word)
    local target
    if view.typed > #word then
        target = CELL_BACKSPACE
    elseif view.typed == #word then
        target = CELL_ENTER
    else
        target = letter_cell(word:sub(view.typed + 1, view.typed + 1))
    end
    return step_toward(view.cell, target) or "confirm"
end

-- The door menu row to release: the first door still locked.
local function wanted_row(bot)
    for index, bit in ipairs(T.door_bits) do
        if not bot:flag(BotConst.LOCK_BANK, bit) then
            return index - 1
        end
    end
    return 2
end

-- One tick while the terminal is up. Returns true while it owns the pad.
function BotTerminal.step(bot, scene)
    local view = scene.computer
    if not view then
        return false
    end
    if bot:tick() - last_input < 2 then
        return true
    end
    local edge
    if view.stage == "login" then
        if view.sub == "user" then
            edge = type_word(view, T.user)
        elseif view.sub == "secret" then
            edge = type_word(view, T.password)
        end
    elseif view.stage == "menu" then
        local row = wanted_row(bot)
        if view.menu_row < row then
            edge = "down_pressed"
        elseif view.menu_row > row then
            edge = "up_pressed"
        else
            edge = "confirm"
        end
    elseif view.stage == "command" then
        if view.sub == "user" then
            edge = type_word(view, T.command)
        end
    elseif view.stage == "unlocked" then
        edge = "confirm"
    end
    if edge then
        if edge == "confirm" then
            bot:log(string.format("terminal: %s/%s confirm at cell %d (typed %d)",
                view.stage, view.sub, view.cell, view.typed))
        end
        bot:set_input({ [edge] = true })
        last_input = bot:tick()
    end
    return true
end

return BotTerminal
