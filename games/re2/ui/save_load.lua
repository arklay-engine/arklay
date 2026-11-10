-- RE 2's save and load screen: `Mem_card` (0x4C57E0) running `Card_access`
-- (0x4C58A0) a tick at a time, as Classic REbirth draws it.
--
-- The typewriter's Yes opens it in save mode (mode 0) on the hand-off's
-- sixth tick, the title's LOAD GAME in load mode (mode 1). `open` is
-- `Mem_card`'s first call: `Cardaccess_init`'s step 0 sets fade 0 to fade
-- in from black. Every `update` after it is one `Card_access` call:
--
-- | step (`pCtcb+9`) | what it does |
-- |---|---|
-- | 3 | `Cardaccess_init` waits for the fade (0x1800 a tick), then `Task_sleep(2)` skips a tick |
-- | 2 | one state a tick (below), then `Card_Mess_disp` draws |
-- | 4 | `Cardaccess_exit` fades out (0x1800 a tick); the tick after the black the screen ends |
--
-- | state | what it does |
-- |---|---|
-- | 0 | the font and the folder; then 1 |
-- | 1 | the folder's saves counted (`cnt0`), the rows `min(10, cnt0 - mode + 2)`; then 2 |
-- | 2 | the list: a held direction moves the cursor every 4 ticks with the move cue (`4:4`), wrapping; decide picks the row (`FUN_00431D10`), cancel plays `4:5` and goes to 99 |
-- | 3 | the write: the ribbon and the count (the engine's write tick), the file; then 0x22 |
-- | 0x22 | the label types a character on each tick the timer is 0, then 4 ticks (`2:22`) or 8 after a space or `_` (`2:24`); then 0x24 with the timer at 60 |
-- | 0x24 | waits: a press, or the timer run out, goes to 0x61 |
-- | 0x61, 99 | the exit |
--
-- The engine reads the work back for the trace's `sv` (`api:set("sv_*")`).
-- Classic REbirth patches the drawing, the names and the overwrite choice
-- (`oracle/re2.md`, "The save screen"): the layout here is measured from
-- the original's frames. The overwrite (0x1E), the load (10), the keyboard
-- paging (`FUN_00432670`) and the name scroll aren't ported.

local M = {}

local STATE_FONT, STATE_LIST, STATE_BROWSE, STATE_WRITE = 0, 1, 2, 3
local STATE_REVEAL, STATE_WAIT, STATE_DONE, STATE_CANCEL = 0x22, 0x24, 0x61, 99

local FADE_STEP, FADE_FULL = 0x1800, 0x8000

-- `Card_Mess_disp`'s places in the 320x240 layout, as Classic REbirth puts
-- them (the original's 640x480 frames halved).
local TITLE_Y = 16
-- The message window (10, 45, 300 by 156, grey 0x21 over black) is part
-- of Classic REbirth's `type00` picture, which the pack carries.
local ROW_Y, ROW_H = 0x31, 15
local BAR_X, BAR_W, BAR_H, BAR_GREY = 0x0A, 0x12C, 0x0E, 0x50
local TEXT_DY = 0
local FOOTER_Y = 0xD8
local BLUE = "#3F7DFA"

-- The core bank's cues (`Snd_se_on(0x40n0000)`) and the reveal's keys
-- (`0x2220000`, `0x2240000`).
local CUE_MOVE, CUE_CANCEL, CUE_DECIDE = 4, 5, 6
local KEY, SPACE = 0x22, 0x24

-- `ScreenInput.held`'s directions.
local HELD_UP, HELD_DOWN, HELD_LEFT, HELD_RIGHT = 0x10, 0x20, 0x40, 0x80

local s = {}
M.state = s

local background

local function publish(api)
    api:set("sv_sub", s.sub)
    api:set("sv_step", s.step)
    api:set("sv_state", s.state)
    api:set("sv_mode", s.mode)
    api:set("sv_cursor", s.cursor)
    api:set("sv_top", s.top)
    if s.listed then
        -- `0x986394` and `cnt0` keep the last screen's until state 1.
        api:set("sv_rows", s.rows)
        api:set("sv_saves", s.cnt0)
    end
    api:set("sv_timer", s.timer)
    api:set("sv_choice", s.choice)
    api:set("sv_typed", s.typed)
end

-- `FUN_00509BE0`: the characters that type with the space's timing.
local function is_space(c)
    return c == " " or c == "_"
end

-- Classic REbirth's name for a save: the player and scenario, the count
-- and the wall clock's date and time (`Leon A_01_2026-11-10 19^59.02`).
function M.save_name(player, scenario_b, count, time)
    local name = (player == 1 and "Claire" or "Leon") .. (scenario_b and " B" or " A")
    local stamp = "0000-00-00 00^00.00"
    if time then
        stamp = string.format(
            "%04d-%02d-%02d %02d^%02d.%02d",
            time.year, time.month, time.day, time.hour, time.minute, time.second
        )
    end
    return string.format("%s_%02d_%s", name, count % 100, stamp)
end

-- What a row shows: a save's name with `^` back to `:`.
local function label_of(name)
    return (name:gsub("%^", ":"))
end

-- The folder's saves, newest first (`Card_write` sorts by the file's
-- last-write time).
local function list_saves(api)
    local saves = {}
    for _, entry in ipairs(api:saves()) do
        if entry.loadable and not entry.autosave then
            saves[#saves + 1] = entry
        end
    end
    table.sort(saves, function(a, b)
        if a.timestamp ~= b.timestamp then
            return a.timestamp > b.timestamp
        end
        return a.name < b.name
    end)
    return saves
end

local function total_rows()
    return s.cnt0 - s.mode + 2
end

local function start_exit()
    s.sub = 4
    s.step = 1
    s.level = 0
end

-- `FUN_00431D10`: the row under the cursor. 0 the new file, 1 a save, 4
-- the leave row, 99 nothing.
local function picked()
    local i = s.cursor + s.top
    if i == -s.mode then
        return 0
    elseif i < s.cnt0 - s.mode + 1 then
        return 1
    elseif i == s.cnt0 - s.mode + 1 then
        return 4
    end
    return 99
end

local function browse(api, input)
    if s.timer ~= 0 then
        s.timer = s.timer - 1
        return
    end
    local held = input.held or 0
    if held & HELD_DOWN ~= 0 then
        s.cursor = s.cursor + 1
        if s.cursor >= s.rows then
            s.cursor = s.rows - 1
            s.top = s.top + 1
            if total_rows() - s.rows < s.top then
                s.cursor = 0
                s.top = 0
            elseif s.top < 0 then
                s.top = 0
            end
        end
        s.timer = 3
        api:core_sound(CUE_MOVE)
    elseif held & HELD_UP ~= 0 then
        s.cursor = s.cursor - 1
        if s.cursor < 0 then
            s.cursor = 0
            s.top = s.top - 1
            if s.top < 0 then
                s.cursor = s.rows - 1
                s.top = total_rows() - s.rows
            end
        end
        s.timer = 3
        api:core_sound(CUE_MOVE)
    elseif held & (HELD_LEFT | HELD_RIGHT) ~= 0 then
        -- The name scroll, which a held direction keeps from a decide.
        return
    elseif input.confirm or input.start then
        local row = picked()
        if row == 0 then
            s.state = STATE_WRITE
            api:core_sound(CUE_DECIDE)
        elseif row == 4 then
            api:core_sound(CUE_CANCEL)
            s.state = STATE_CANCEL
        elseif row == 1 and s.mode == 1 then
            -- The load (state 10): not ported; it loads at once.
            api:core_sound(CUE_DECIDE)
            local entry = s.saves[s.mode + s.cursor - 1 + s.top + 1]
            if entry then
                local ok, err = api:load(entry.name)
                if not ok then
                    api:log("warning: failed to load " .. entry.name .. ": " .. tostring(err))
                end
            end
        end
    elseif input.cancel then
        api:core_sound(CUE_CANCEL)
        s.state = STATE_CANCEL
    end
end

-- State 3: the write. The engine takes the ribbon and raises the count on
-- the same tick; a failed write goes on as a written one, as the
-- original's does (its rollback runs only while `string` is set, which
-- the save mode's 99 and 0x61 clear).
local function write(api)
    local pending = api:pending() or { character = 0, saves = 0 }
    local character = api:character()
    s.name = M.save_name(character & 1, character & 2 ~= 0, pending.saves + 1, api:local_time())
    s.label = label_of(s.name)
    s.new = {
        character = character,
        saves = pending.saves + 1,
        stage = pending.stage,
        room = pending.room,
    }
    local ok, err = api:save(s.name .. ".dat", true)
    if not ok then
        api:log("warning: the save was not written: " .. tostring(err))
    end
    s.timer = 0
    s.typed = 0
    s.state = STATE_REVEAL
end

local function reveal(api)
    if s.timer ~= 0 then
        s.timer = s.timer - 1
        return
    end
    if s.typed < #s.name then
        s.typed = s.typed + 1
        if is_space(s.name:sub(s.typed, s.typed)) then
            s.timer = 8
            api:room_sound(SPACE)
        else
            s.timer = 4
            api:room_sound(KEY)
        end
    else
        s.state = STATE_WAIT
        s.timer = 0x3C
    end
end

local function wait(input)
    if input.confirm or input.cancel or input.start then
        s.timer = 0
        s.state = STATE_DONE
    elseif s.timer < 1 then
        s.state = STATE_DONE
    else
        s.timer = s.timer - 1
    end
end

-- One `Card_access` state.
local function step(api, input)
    local state = s.state
    if state == STATE_FONT then
        s.state = STATE_LIST
    elseif state == STATE_LIST then
        s.saves = list_saves(api)
        s.cnt0 = #s.saves
        s.rows = total_rows() < 10 and total_rows() or 10
        s.listed = true
        s.state = STATE_BROWSE
    elseif state == STATE_BROWSE then
        browse(api, input)
    elseif state == STATE_WRITE then
        write(api)
    elseif state == STATE_REVEAL then
        reveal(api)
    elseif state == STATE_WAIT then
        wait(input)
    elseif state == STATE_DONE or state == STATE_CANCEL then
        start_exit()
    end
end

function M.open(api)
    local err
    background, err = api:picture("ui/type00")
    if not background then
        api:log("warning: no save screen background: " .. tostring(err))
    end
    for key in pairs(s) do
        s[key] = nil
    end
    s.mode = api:mode() == "load" and 1 or 0
    s.state = STATE_FONT
    s.cursor, s.top, s.rows, s.timer, s.choice, s.typed, s.cnt0 = 0, 0, 0, 0, 0, 0, 0
    s.saves = {}
    -- `Cardaccess_init`'s step 0: fade 0 from black, a step already taken
    -- on this tick's draw.
    s.sub, s.step = 3, 1
    s.level = FADE_FULL - FADE_STEP
    s.skip = false
    publish(api)
    api:fade(s.level >> 7)
end

function M.update(api, input)
    if s.sub == 3 then
        if s.step == 1 then
            s.level = s.level - FADE_STEP
            if s.level <= 0 then
                s.level = 0
                s.step = 2
                s.skip = true
            end
        elseif s.skip then
            -- `Task_sleep(2)`: the task sleeps through this tick.
            s.skip = false
        else
            s.sub, s.step = 2, 0
            step(api, input)
        end
    elseif s.sub == 2 then
        step(api, input)
    elseif s.sub == 4 then
        if s.step == 1 then
            s.level = s.level + FADE_STEP
            if s.level >= FADE_FULL then
                s.level = FADE_FULL
                s.step = 2
            end
        else
            -- `Cardaccess_exit`'s step 2 returns without a sleep: `Mem_card`
            -- ends on this tick.
            api:exit(s.mode == 1 and "title" or "resume")
            return
        end
    end
    publish(api)
    api:fade(s.level >= FADE_FULL and 255 or s.level >> 7)
end

-- ---------------------------------------------------------------- drawing

local function centred(api, y, text, colour)
    local w = api:text_width(text)
    api:text(160 - w // 2, y, text, { colour = colour })
end

local function draw_footer(api, info)
    if not info then
        return
    end
    local name = (info.character & 1 == 1 and "Claire" or "Leon") .. (info.character & 2 ~= 0 and " B" or " A")
    local place = api:location_bytes(info.stage, info.room) or ""
    local rest = string.format("/%02d/", info.saves % 100)
    local w1 = api:text_width(name)
    local w2 = api:text_width(rest) + api:glyphs_width(place)
    local x = (320 - w1 - w2) // 2
    api:text(x, FOOTER_Y, name, { colour = BLUE })
    api:text(x + w1, FOOTER_Y, rest)
    api:glyphs(x + w1 + api:text_width(rest), FOOTER_Y, place)
end

function M.draw(api)
    api:clear()
    if background then
        api:draw_background(background, 0, 0, 255, 255, 255)
    end
    -- `Card_Mess_disp` draws after each state but the exit; states 0, 1,
    -- 0x61 and 99 draw nothing over the window.
    local state = s.state
    if s.sub ~= 2 or state == STATE_FONT or state == STATE_LIST or state == STATE_DONE or state == STATE_CANCEL then
        return
    end
    centred(api, TITLE_Y, s.mode == 1 and "Load" or "Save")
    local typing = state == STATE_WRITE or state == STATE_REVEAL or state == STATE_WAIT
    if not typing then
        api:fade_tile(1, BAR_GREY, BAR_X, ROW_Y + s.cursor * ROW_H, BAR_W, BAR_H)
    end
    for row = 0, s.rows - 1 do
        local i = row + s.top
        local y = ROW_Y + row * ROW_H + TEXT_DY
        if i == 0 and s.mode == 0 then
            if typing then
                if s.label then
                    centred(api, y, s.label:sub(1, s.typed), BLUE)
                end
            else
                centred(api, y, "Create New")
            end
        elseif i <= s.cnt0 - s.mode then
            local entry = s.saves[i + s.mode]
            if entry then
                centred(api, y, label_of(entry.base))
            end
        else
            centred(api, y, "Exit")
        end
    end
    if typing then
        if state ~= STATE_WRITE then
            draw_footer(api, s.new)
        end
    else
        local i = s.cursor + s.top
        if i > -s.mode and i <= s.cnt0 - s.mode then
            draw_footer(api, s.saves[i + s.mode])
        elseif i == s.cnt0 - s.mode + 1 and s.mode == 1 then
            centred(api, FOOTER_Y, "Do not load")
        end
    end
end

return M
