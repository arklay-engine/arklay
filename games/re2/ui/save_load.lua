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
-- | 0x1E | the overwrite choice, as Classic REbirth patches it: left and right (edges) move it between Overwrite and Cancel with the move cue; decide on Overwrite goes to 0x1F with the decide cue; decide on Cancel, or cancel, plays `4:5` and goes back to 2 |
-- | 0x1F | as 3, the new save taking the row, whose old file is deleted once the new one is written (the original deletes it in 0x1E, before the write); then 0x20 |
-- | 0x22, 0x20 | the label types a character on each tick the timer is 0, then 4 ticks (`2:22`) or 8 after a space or `_` (`2:24`); then 0x24 (0x21 from 0x20) with the timer at 60 |
-- | 0x24, 0x21 | waits: a press, or the timer run out, goes to 0x61 |
-- | 0x61, 99 | the exit |
--
-- The engine reads the work back for the trace's `sv` (`api:set("sv_*")`).
-- Classic REbirth patches the drawing, the names and the overwrite choice
-- (`oracle/re2.md`, "The save screen"): the layout here is measured from
-- the original's frames, and the overwrite is the patched one (the
-- executable's own is a three-way choice with an Update that keeps the
-- name). The load (10), the failed write (0x62), the keyboard paging
-- (`FUN_00432670`) and the name scroll aren't ported.

local CrText = require("ui/cr_text")

local M = {}

local STATE_FONT, STATE_LIST, STATE_BROWSE, STATE_WRITE = 0, 1, 2, 3
local STATE_REVEAL, STATE_WAIT, STATE_DONE, STATE_CANCEL = 0x22, 0x24, 0x61, 99
local STATE_ASK, STATE_OVERWRITE, STATE_RETYPE, STATE_REWAIT = 0x1E, 0x1F, 0x20, 0x21

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
-- The footer's count has its digits in cells as wide as the widest digit.
local DIGIT_W = 12
local BLUE = "#1E64FF"
-- The overwrite choice under the window: the question, the two answers a
-- row below it and `Cursor_disp`'s arrowhead left of the lit one.
local ASK_Y, ANSWER_Y = 0xCF, 0xDE
local ANSWER_X = { [0] = 0x38, 0xCC }
local ARROW_DX, ARROW_DY = -7, 4
local GREEN = "#00C80A"

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

-- What a row shows: a save's name with `^` back to `:`, and its `_` as
-- the `/` Classic REbirth prints for it.
local function label_of(name)
    return (name:gsub("%^", ":"):gsub("_", "/"))
end

local function newest_first(a, b)
    if a.timestamp ~= b.timestamp then
        return a.timestamp > b.timestamp
    end
    return a.name < b.name
end

-- The folder's RE 2 saves, newest first (`Card_write` sorts by the file's
-- last-write time); an RE 1 block in the same folder isn't listed. In load
-- mode the RE 2 autosaves (port-only) follow as extra rows, newest first,
-- as RE 1's load screen lists them; `cnt0` counts the saves alone.
local function list_saves(api, load)
    local saves, autosaves = {}, {}
    for _, entry in ipairs(api:saves()) do
        if entry.loadable and entry.re2 then
            if not entry.autosave then
                saves[#saves + 1] = entry
            elseif load then
                autosaves[#autosaves + 1] = entry
            end
        end
    end
    table.sort(saves, newest_first)
    table.sort(autosaves, function(a, b)
        return a.autosave < b.autosave
    end)
    local cnt0 = #saves
    for _, entry in ipairs(autosaves) do
        saves[#saves + 1] = entry
    end
    return saves, cnt0
end

-- The list's rows past the folder's own: the new file in save mode, the
-- saves (and the autosaves in load mode), the leave row.
local function total_rows()
    return s.count - s.mode + 2
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
    elseif i < s.count - s.mode + 1 then
        return 1
    elseif i == s.count - s.mode + 1 then
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
        elseif row == 1 and s.mode == 0 then
            s.choice = 0
            s.picked = s.cursor + s.top
            s.state = STATE_ASK
            api:core_sound(CUE_DECIDE)
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

-- State 0x1E: the overwrite choice, the directions' edges first.
local function ask(api, input)
    if input.left or input.right then
        -- Left wraps from 0 to 1 and right from 1 to 0: either moves it.
        s.choice = 1 - s.choice
        api:core_sound(CUE_MOVE)
    elseif (input.confirm or input.start) and s.choice == 0 then
        -- The original deletes the row's file here (`RemoveSave`), a tick
        -- before it writes the new one; `write` deletes it once the new
        -- one is written, so a write that fails loses no save.
        s.state = STATE_OVERWRITE
        api:core_sound(CUE_DECIDE)
    elseif input.confirm or input.start or input.cancel then
        api:core_sound(CUE_CANCEL)
        s.state = STATE_BROWSE
    end
end

-- Whether the save `old` is a file of its own beside the new `file`. Two
-- names that differ only in case are one file where the folder folds case,
-- and that one must stay.
local function another_file(api, old, file)
    if old:lower() ~= file:lower() then
        return true
    end
    local found = 0
    for _, entry in ipairs(api:saves()) do
        if entry.name:lower() == file:lower() then
            found = found + 1
        end
    end
    return found > 1
end

-- States 3 and 0x1F: the write. The engine takes the ribbon and raises
-- the count on the same tick; a failed write goes on as a written one, as
-- the original's does (its rollback runs only while `string` is set,
-- which the save mode's 99 and 0x61 clear). `reveal` is the state that
-- types the label: 0x22 for a new file, 0x20 for a row's, whose old file
-- (`replaced`) goes once the new one is written.
local function write(api, reveal, replaced)
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
    local file = s.name .. ".dat"
    local ok, err = api:save(file, true)
    if not ok then
        api:log("warning: the save was not written: " .. tostring(err))
    elseif replaced and another_file(api, replaced.name, file) then
        local removed, why = api:remove(replaced.name)
        if not removed then
            api:log("warning: the old save was not removed: " .. tostring(why))
        end
    end
    s.timer = 0
    s.typed = 0
    s.state = reveal
end

-- States 0x22 and 0x20, which Classic REbirth gives 0x22's timers; `wait`
-- is the state that follows.
local function reveal(api, wait)
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
        s.state = wait
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
        s.saves, s.cnt0 = list_saves(api, s.mode == 1)
        s.count = #s.saves
        s.rows = total_rows() < 10 and total_rows() or 10
        s.listed = true
        s.state = STATE_BROWSE
    elseif state == STATE_BROWSE then
        browse(api, input)
    elseif state == STATE_WRITE then
        write(api, STATE_REVEAL)
    elseif state == STATE_REVEAL then
        reveal(api, STATE_WAIT)
    elseif state == STATE_WAIT or state == STATE_REWAIT then
        wait(input)
    elseif state == STATE_ASK then
        ask(api, input)
    elseif state == STATE_OVERWRITE then
        write(api, STATE_RETYPE, s.saves[s.picked])
    elseif state == STATE_RETYPE then
        reveal(api, STATE_REWAIT)
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
    s.cursor, s.top, s.rows, s.timer, s.choice, s.typed, s.cnt0, s.count = 0, 0, 0, 0, 0, 0, 0, 0
    s.blink, s.blink_choice = 0, 0
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
        if s.sub == 2 and s.state == STATE_ASK then
            -- `Card_Mess_disp` steps the answers' blink (the state word's
            -- third byte) as it draws them: 32 ticks, the first 15 lit,
            -- from the start again when the choice isn't the one it last
            -- drew (`Mwork_pc2`). The original keeps both from one screen
            -- to the next; each screen here starts them at 0.
            if s.blink_choice ~= s.choice then
                s.blink_choice = s.choice
                s.blink = 0x1F
            end
            s.blink = (s.blink + 1) & 0x1F
        end
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

-- `SavePrint`, Classic REbirth's print for this screen (`ui/cr_text.lua`).
local save_width, save_print = CrText.width, CrText.print

local function centred(api, y, text, colour)
    -- A line is centred as two pixels wider than it prints.
    save_print(api, 159 - (save_width(api, text) + 1) // 2, y, text, colour)
end

local function draw_footer(api, info)
    if not info then
        return
    end
    local name = (info.character & 1 == 1 and "Claire" or "Leon") .. (info.character & 2 ~= 0 and " B" or " A")
    local place = api:location_bytes(info.stage, info.room) or ""
    local slash = api:text_width("/")
    local w1 = api:text_width(name)
    local w2 = 2 * slash + 2 * DIGIT_W + api:glyphs_width(place)
    local x = (320 - w1 - w2) // 2
    api:text(x, FOOTER_Y, name, { colour = BLUE })
    x = x + w1
    api:text(x, FOOTER_Y, "/")
    x = x + slash
    -- The count's two digits, each centred in a cell.
    for digit in string.format("%02d", info.saves % 100):gmatch(".") do
        api:text(x + (DIGIT_W - api:text_width(digit) + 1) // 2, FOOTER_Y, digit)
        x = x + DIGIT_W
    end
    api:text(x, FOOTER_Y, "/")
    api:glyphs(x + slash, FOOTER_Y, place)
end

-- `Cursor_disp`: the debug print's `&`, a 6 by 9 arrowhead in three
-- greys, as the frames have it.
local function draw_arrow(api, x, y)
    for row = 0, 8 do
        local n = row < 5 and row or 8 - row
        api:fill_rect(x, y + row, n + 2, 1, 57, 49, 74)
        if n > 0 then
            api:fill_rect(x + 1, y + row, n - 1, 1, 206, 206, 189)
            api:fill_rect(x + n, y + row, 1, 1, 115, 115, 107)
        end
    end
end

local function draw_ask(api)
    centred(api, ASK_Y, "Proceed to overwrite this save file?")
    save_print(api, ANSWER_X[0], ANSWER_Y, "Overwrite", s.choice == 0 and GREEN or nil)
    save_print(api, ANSWER_X[1], ANSWER_Y, "Cancel", s.choice == 1 and GREEN or nil)
    if s.blink < 0xF then
        draw_arrow(api, ANSWER_X[s.choice] + ARROW_DX, ANSWER_Y + ARROW_DY)
    end
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
    -- A new file's label types in the first row with no bar; an
    -- overwritten row's types in its own row, under the bar.
    local typing = state == STATE_WRITE or state == STATE_REVEAL or state == STATE_WAIT
    local retyping = state == STATE_RETYPE or state == STATE_REWAIT
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
        elseif i <= s.count - s.mode then
            local entry = s.saves[i + s.mode]
            if retyping and i == s.picked then
                centred(api, y, s.label:sub(1, s.typed), BLUE)
            elseif entry then
                centred(api, y, label_of(entry.base), BLUE)
            end
        else
            centred(api, y, "Exit")
        end
    end
    if not typing then
        -- The bar is added over its row's text.
        api:fade_tile(1, BAR_GREY, BAR_X, ROW_Y + s.cursor * ROW_H, BAR_W, BAR_H)
    end
    if typing then
        if state ~= STATE_WRITE then
            draw_footer(api, s.new)
        end
    elseif retyping then
        draw_footer(api, s.new)
    elseif state == STATE_ASK then
        draw_ask(api)
    elseif state ~= STATE_OVERWRITE then
        local i = s.cursor + s.top
        if i > -s.mode and i <= s.count - s.mode then
            draw_footer(api, s.saves[i + s.mode])
        elseif i == s.count - s.mode + 1 and s.mode == 1 then
            centred(api, FOOTER_Y, "Do not load")
        end
    end
end

return M
