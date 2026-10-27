-- The save/load screen.
--
-- A port of the original's per-tick state machine (SaveLoadScreen.cpp):
-- scan the eight savedat*.dat slots, navigate the eight rows plus the exit
-- row with a blinking cursor, confirm an overwrite in save mode, write the
-- block and play the typed reveal (name/count/location, two bytes per
-- step), or load the chosen slot. It serves both modes: the title's load
-- path opens it in load mode, and a room typewriter opens it in save mode
-- over the frozen gameplay frame.
--
-- Port-only: in load mode the autosaves (autosave1.dat, autosave2.dat) the
-- interactive game writes on each door's room load follow the eight slots as
-- extra rows, newest first, and push the exit row down. With no autosave on
-- disk the screen is the original's exactly.
--
-- The column positions are the release's glyph grid's, picked like the
-- reference's GetSaveScreenLayout: the Japanese FONT.TIM's 14px cells
-- (name(3) / count(2) / location(8), a 15-cell row) or the USA fontus.tim's
-- 8px cells (name(5) \ count(2) \ location(19), a 28-cell row). The port's
-- session font follows the language, so the grid follows the font that is
-- actually drawing: api:glyph_width() is 14 on the Japanese sheet and 8 on
-- the Western one. The reveal string is assembled exactly like the
-- original's: the name cells, a separator, the two count digits, a
-- separator and the location cells. All text is the text table's
-- font-encoded bytes, drawn with api:glyphs.

local M = {}

local HEADER_Y = 13
local ROW_X, ROW_Y, ROW_HEIGHT = 55, 45, 16
local CONFIRM_X, CONFIRM_Y = 118, 209
local PROMPT_X, PROMPT_Y = 49, 193
local ERROR_Y, ERROR_Y2 = 209, 225
local SLOT_COUNT = 8

-- The slot-row art's cells: a blank, the separator glyph (0x38, a forward
-- slash in FONT.TIM and a backslash in fontus.tim) and the dash (0x3B, the
-- katakana long-vowel mark in FONT.TIM and '-' in fontus.tim). Both sheets
-- share these bytes, so the art belongs to the grid, not to a language.
local function art(cells)
    return (cells:gsub(".", { [" "] = "\0", ["/"] = "\x38", ["-"] = "\x3B" }))
end

-- The two releases' column tables (SaveLoadScreen.cpp's SaveScreenLayout),
-- keyed by glyph width.
M.LAYOUTS = {
    -- The Japanese screen (0x00435af0).
    [14] = {
        glyph_w = 14,
        count_x = 0x6F, -- cell 4 of the grid from ROW_X
        loc_x = 0x99, -- cell 7
        header_x = 0x61, -- the 9-cell "SAVE GAME" centred
        cursor_x = 0x29, -- one cell left of ROW_X
        confirm_stride = 56, -- 4 cells: " はい" -> "  いいえ"
        name_cells = 3,
        loc_cells = 8,
        -- Leading blank cells the original's location names carry.
        loc_indent = 0,
        filled = art("   /  /     /  "),
        empty = art("---/--/-----/--"),
    },
    -- The USA screen (0x00493310).
    [8] = {
        glyph_w = 8,
        count_x = 103, -- cell 6
        loc_x = 127, -- cell 9
        header_x = 124,
        cursor_x = 47,
        confirm_stride = 40, -- 5 cells: " YES" -> " NO"
        name_cells = 5,
        loc_cells = 19,
        -- The USA location names start with two blanks ("  M.Room 1F"),
        -- which the empty row's two blanks after the second separator
        -- mirror; the language files' labels do not carry them.
        loc_indent = 2,
        filled = art("     /  /"),
        empty = art("-----/--/  -----------------"),
    },
}
-- Without a font the screen draws no text; keep the Japanese table.
local DEFAULT_GLYPH_W = 14

local BLINK_TICKS = 5
local REVEAL_TICKS = 5
local REVEAL_STEP = 2
-- The black-overlay steps of the fade-in and the (faster) fade-out.
local FADE_STEP, FADE_OUT_STEP = 8, 16
-- The separator glyph and the blinking cursor: the reference's
-- DrawSaveCursor blits the arrowhead at the glyph grid's plain byte 0x02.
local SEPARATOR = 0x38
local CURSOR_GLYPH = string.char(0x02)
local PAD = 0xFB
local BLANK_CELL = string.char(0x00, PAD)
local TERMINATOR = 0x01

local background
local s
-- The running grid's column table.
local L = M.LAYOUTS[DEFAULT_GLYPH_W]

--- The save-count digits as the original's %02d line: tens, units, each in
--- its own cell with a 0xFB pad.
function M.count_bytes(saves)
    saves = saves % 100
    return string.char(0x0C + (saves // 10) % 10, PAD, 0x0C + saves % 10, PAD)
end

--- `src` (up to its terminator) as two-byte glyph cells, blank-padded to
--- at least `cells`: a plain glyph takes a 0xFB pad, an extended-page glyph
--- is already two bytes. The original's name and location tables are
--- authored this way (STR_CELL), because the reveal lengthens its slice two
--- bytes a step; the language files spell Latin text unpadded. Text longer
--- than its field is kept whole, as the filled rows draw it.
function M.cells(src, cells)
    local out = {}
    local i = 1
    while i <= #src do
        local byte = src:byte(i)
        if byte == TERMINATOR then
            break
        elseif byte == PAD or byte == 0xFF then
            i = i + 1
        elseif byte >= 0xF8 and byte <= 0xFA then
            out[#out + 1] = src:sub(i, i + 1)
            i = i + 2
        else
            out[#out + 1] = string.char(byte, PAD)
            i = i + 1
        end
    end
    for _ = #out + 1, cells do
        out[#out + 1] = BLANK_CELL
    end
    return table.concat(out)
end

--- The location field: the grid's leading blanks, then the label.
local function location_field(location)
    return string.rep(BLANK_CELL, L.loc_indent) .. M.cells(location or "", 0)
end

--- Assemble the reveal string: name, separator, count digits, separator,
--- location and its terminator (the original's slice takes the 0x01 too).
--- Also returns the byte offsets the count and the location start at.
function M.build_reveal(name, saves, location)
    local name_field = M.cells(name or "", L.name_cells)
    local loc_field = M.cells(location_field(location), L.loc_cells)
    local bytes = name_field
        .. string.char(SEPARATOR, PAD)
        .. M.count_bytes(saves)
        .. string.char(SEPARATOR, PAD)
        .. loc_field
        .. string.char(TERMINATOR)
    return bytes, #name_field, #name_field + 8
end

--- Whether the reveal has shown everything the original shows. The last
--- printed slice is one byte short of the string: the final pass's check
--- steps past the string before it draws the terminator-only slice.
local function reveal_finished(reveal)
    return reveal.length + REVEAL_STEP + REVEAL_STEP > #reveal.bytes + 1
end

--- Whether the slice drawn at the current length asks for the reference's
--- typewriter tick (play_sfx(sfxBank, 31)): the byte two back from the copy
--- end is not a space (spaces are 0x00).
local function reveal_cues(reveal)
    local index = reveal.length - REVEAL_STEP
    if index < 0 then
        return false
    end
    local byte = reveal.bytes:byte(index + 1)
    return byte ~= nil and byte ~= 0
end

local function reset_blink(visible)
    s.blink_state = visible or false
    s.blink_timer = BLINK_TICKS
end

--- The exit row's index (0-based): past the slot rows and the load mode's
--- autosave rows.
local function exit_row()
    return SLOT_COUNT + #s.autosaves
end

--- The autosave entry under the cursor, or nil on a slot or the exit row.
local function selected_autosave()
    return s.autosaves[s.cursor - SLOT_COUNT + 1]
end

--- Move the cursor, wrapping through the exit row.
local function move_cursor(delta)
    s.cursor = (s.cursor + delta) % (exit_row() + 1)
    reset_blink()
end

local function tick_blink()
    if s.blink_timer == 0 then
        s.blink_state = not s.blink_state
        s.blink_timer = BLINK_TICKS
    else
        s.blink_timer = s.blink_timer - 1
    end
end

--- Leave through cancel: back to gameplay in save mode, back to the title
--- in load mode.
local function leave()
    s.exit = s.save and "resume" or "title"
    s.stage = "fade_out"
end

--- Write the pending block to the cursor's slot and start the reveal.
---
--- `slot_cued` says the tick that arrived here already played the slot's
--- confirm cue - the navigation confirm plays it, while the overwrite
--- dialog's YES is silent. The reference plays play_sfx(sfxBank, 31) on the
--- confirm frame and, when the byte is not a space, again on its first
--- STATE_SAVE_ANIM_STEP1 frame; the port draws the first slice on the
--- write's own tick, so only the silent overwrite path adds that first
--- slice's tick here.
local function begin_save(api, slot_cued)
    local ok, info = api:save_slot(s.cursor)
    if not ok then
        api:log("warning: failed to save slot " .. (s.cursor + 1) .. ": " .. tostring(info))
        s.stage = "error"
        return
    end
    s.slots[s.cursor + 1] = info
    local name = api:ui_bytes(
        info.character & 1 == 0 and "UI_SAVE_CHARACTER_0_NAME" or "UI_SAVE_CHARACTER_1_NAME"
    ) or ""
    local location = api:location_bytes(info.stage, info.room) or ""
    local bytes, name_len, loc_base = M.build_reveal(name, info.saves, location)
    local reveal = {
        bytes = bytes,
        name_len = name_len,
        loc_base = loc_base,
        length = REVEAL_STEP,
        timer = REVEAL_TICKS,
    }
    if not slot_cued and reveal_cues(reveal) then
        api:sound("save_confirm")
    end
    s.reveal = reveal
    s.stage = "save_anim"
end

--- Confirm the current row.
local function confirm(api)
    if s.cursor == exit_row() then
        leave()
        return
    end
    local autosave = selected_autosave()
    if autosave then
        s.exit = "load_autosave"
        s.load_name = autosave.name
        s.stage = "fade_out"
        return
    end
    local occupied = s.slots[s.cursor + 1] ~= false
    if not s.save then
        -- An empty slot refuses.
        if occupied then
            s.exit = "load_game"
            s.stage = "fade_out"
        end
    elseif occupied then
        s.confirm_choice = 0
        reset_blink(true)
        s.stage = "confirm_overwrite"
    else
        -- The caller played the slot's confirm cue (the same bank slot as
        -- the first slice's typewriter tick).
        begin_save(api, true)
    end
end

function M.open(api)
    local err
    background, err = api:picture("ui/type00")
    if not background then
        api:log("warning: no save/load background: " .. err)
    end
    L = M.LAYOUTS[api:glyph_width() or DEFAULT_GLYPH_W] or M.LAYOUTS[DEFAULT_GLYPH_W]
    M.layout = L
    local save = api:mode() == "save"
    -- The loadable autosaves, newest first; save mode never writes them.
    local autosaves = {}
    if not save then
        for _, entry in ipairs(api:saves()) do
            if entry.autosave and entry.loadable then
                autosaves[#autosaves + 1] = entry
            end
        end
        table.sort(autosaves, function(a, b)
            return a.autosave < b.autosave
        end)
    end
    s = {
        save = save,
        slots = api:slot_info(),
        autosaves = autosaves,
        -- 0-7 are the slot rows, then the autosave rows, then the exit row.
        cursor = 0,
        stage = "idle",
        blink_timer = BLINK_TICKS,
        blink_state = false,
        -- 0 YES, 1 NO in the overwrite confirmation.
        confirm_choice = 0,
        reveal = nil,
        exit = nil,
        fade = 0,
    }
    M.state = s
end

local function finish(api)
    if s.exit == "load_game" then
        api:exit("load_game", s.cursor)
    elseif s.exit == "load_autosave" then
        local ok, err = api:load(s.load_name)
        if not ok then
            api:log("warning: failed to load " .. s.load_name .. ": " .. tostring(err))
            api:exit("title")
        end
    else
        api:exit(s.exit or "resume")
    end
end

local function step(api, input)
    if s.stage == "idle" then
        if input.up then
            api:sound("save_cursor")
            move_cursor(-1)
        end
        if input.down then
            api:sound("save_cursor")
            move_cursor(1)
        end
        if input.confirm then
            -- The exit row uses the cancel slot; the slots use the confirm
            -- slot (the original's play_sfx 29/31).
            api:sound(s.cursor == exit_row() and "cancel" or "save_confirm")
            confirm(api)
        elseif input.cancel then
            api:sound("cancel")
            leave()
        end
    elseif s.stage == "confirm_overwrite" then
        -- The original's overwrite dialog is silent.
        if input.left then
            s.confirm_choice = 0
            reset_blink()
        end
        if input.right then
            s.confirm_choice = 1
            reset_blink()
        end
        if input.confirm then
            if s.confirm_choice == 1 then
                s.stage = "idle"
                reset_blink()
            else
                -- The dialog's YES is silent in the reference, so the
                -- write's own tick plays the first slice's tick.
                begin_save(api, false)
            end
        elseif input.cancel then
            s.stage = "idle"
            reset_blink()
        end
    elseif s.stage == "save_anim" then
        local reveal = s.reveal
        if reveal.timer == 0 then
            if reveal_finished(reveal) then
                s.exit = "resume"
                s.stage = "fade_out"
            else
                reveal.length = reveal.length + REVEAL_STEP
                reveal.timer = REVEAL_TICKS
                if reveal_cues(reveal) then
                    api:sound("save_confirm")
                end
            end
        else
            reveal.timer = reveal.timer - 1
        end
    elseif s.stage == "error" then
        if input.confirm or input.cancel or input.any then
            s.stage = "idle"
            reset_blink()
        end
    end
end

function M.update(api, input)
    if s.stage == "fade_out" then
        s.fade = s.fade + FADE_OUT_STEP
        if s.fade >= 255 then
            s.fade = 255
            api:fade(s.fade)
            finish(api)
            return
        end
        api:fade(s.fade)
        return
    end
    s.fade = s.fade - FADE_STEP
    if s.fade < 0 then
        s.fade = 0
    end
    tick_blink()
    step(api, input)
    api:fade(s.fade)
    -- The in-game (typewriter) screen does not fade itself: the reference's
    -- LoadSaveGameState returns straight to check_typewriter_state, which
    -- fades the room back in from the black the save hand-off left. Only
    -- the title's load screen fades its own overlay.
    if s.stage == "fade_out" and s.save then
        finish(api)
    end
end

--- The layout origin: the original's 320x240 columns, centred.
local function origin(api)
    return api:width() // 2 - 160, api:height() // 2 - 120
end

-- The rows a frame shorter than 240 (a wider aspect's band) stacks the
-- screen on: the header, the slot rows at a tighter pitch, the exit row and
-- the prompt/confirm lines, measured from the band's top.
local COMPACT = {
    header = 4, row = 20, pitch = 14,
    prompt = 148, confirm = 164, error = 148, error2 = 164,
}

--- The vertical layout in the origin's coordinates: the original's rows
--- on a 240-row frame, the compact rows on a shorter one.
local function layout(api, oy)
    if api:height() >= 240 then
        return {
            header = HEADER_Y, row = ROW_Y, pitch = ROW_HEIGHT,
            exit = ROW_Y + exit_row() * ROW_HEIGHT,
            prompt = PROMPT_Y, confirm = CONFIRM_Y, error = ERROR_Y, error2 = ERROR_Y2,
        }
    end
    local rows = {}
    for key, y in pairs(COMPACT) do
        rows[key] = key == "pitch" and y or y - oy
    end
    rows.exit = rows.row + exit_row() * rows.pitch
    return rows
end

local function cursor(api, x, y)
    if api:has_font() then
        api:glyphs(x, y, CURSOR_GLYPH, { tight = false })
    else
        api:fill_rect(x, y, 8, 12, 255, 255, 255, 255)
    end
end

--- Draw `bytes` one glyph cell per grid column from `x`, the original's
--- fixed-advance PrintFormattedText. The Western sheet always draws at its
--- proportional advances (its dash is 7px), so the slot art and the
--- reveal's separators place every cell on the grid themselves.
local function grid(api, x, y, bytes)
    local column = 0
    local i = 1
    while i <= #bytes do
        local byte = bytes:byte(i)
        if byte == TERMINATOR then
            break
        elseif byte == PAD or byte == 0xFF then
            i = i + 1
        else
            local len = (byte >= 0xF8 and byte <= 0xFA) and 2 or 1
            if byte ~= 0 then
                api:glyphs(x + column * L.glyph_w, y, bytes:sub(i, i + len - 1), { tight = false })
            end
            column = column + 1
            i = i + len
        end
    end
end

local function min(a, b)
    return a < b and a or b
end

--- Draw the reveal's first `length` bytes field by field, so each lands on
--- the columns the filled rows use: the name at the row's start, the
--- separators and count digits on the grid, the location at its column.
local function draw_reveal(api, ox, y, reveal)
    local length = reveal.length
    local bytes = reveal.bytes
    api:glyphs(ox + ROW_X, y, bytes:sub(1, min(length, reveal.name_len)))
    if length > reveal.name_len then
        local x = ox + ROW_X + L.name_cells * L.glyph_w
        grid(api, x, y, bytes:sub(reveal.name_len + 1, min(length, reveal.loc_base)))
    end
    if length > reveal.loc_base then
        api:glyphs(ox + L.loc_x, y, bytes:sub(reveal.loc_base + 1, length))
    end
end

function M.draw(api)
    local ox, oy = origin(api)
    local rows = layout(api, oy)
    if background then
        -- A full-frame backdrop is stretched, so a replacement may be
        -- authored at any size.
        api:draw_background(background, 0, 0, 255, 255, 255)
    else
        api:clear()
        api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0, 255)
    end
    local row_y = oy + rows.row + s.cursor * rows.pitch
    if not (api:has_font() and api:has_text()) then
        cursor(api, ox + L.cursor_x, row_y)
        return
    end
    local function text(x, y, key)
        api:glyphs(ox + x, oy + y, api:ui_bytes(key))
    end
    local mode = s.save and 0 or 1
    -- A filled row: the slot art keeps its separators on the count and
    -- location columns; the name, count and location overlay its blanks.
    local function filled_row(y, info)
        grid(api, ox + ROW_X, oy + y, L.filled)
        text(ROW_X, y, "UI_SAVE_CHARACTER_" .. (info.character & 1) .. "_NAME")
        grid(api, ox + L.count_x, oy + y, M.count_bytes(info.saves))
        api:glyphs(ox + L.loc_x, oy + y, location_field(api:location_bytes(info.stage, info.room)))
    end

    -- The header is the original's formatted path, so the suffix's M is
    -- the sheet's letter M.
    text(L.header_x, rows.header, "UI_SAVE_HEADER_" .. mode)
    text(L.header_x, rows.header, "UI_SAVE_HEADER_SUFFIX")

    for index = 0, SLOT_COUNT - 1 do
        local y = rows.row + index * rows.pitch
        local info = s.slots[index + 1]
        if s.stage == "save_anim" and s.cursor == index then
            -- The reveal draws this row.
        elseif info then
            filled_row(y, info)
        else
            grid(api, ox + ROW_X, oy + y, L.empty)
        end
    end

    -- The autosave rows draw like filled slots.
    for index, info in ipairs(s.autosaves) do
        filled_row(rows.row + (SLOT_COUNT + index - 1) * rows.pitch, info)
    end

    text(ROW_X, rows.exit, "UI_SAVE_EXIT_" .. mode)
    text(ROW_X, rows.exit, "UI_SAVE_EXIT_SUFFIX")

    if s.reveal then
        draw_reveal(api, ox, row_y, s.reveal)
    end

    if s.stage == "confirm_overwrite" then
        text(PROMPT_X, rows.prompt, "UI_SAVE_OVERWRITE_PROMPT")
        text(CONFIRM_X, rows.confirm, "UI_SAVE_YES_NO")
    elseif s.stage == "error" then
        text(PROMPT_X, rows.error, "UI_SAVE_ERROR_0")
        text(PROMPT_X, rows.error2, "UI_SAVE_ERROR_1")
    end

    if s.stage == "confirm_overwrite" then
        -- The slot cursor stays visible while the confirm cursor blinks.
        cursor(api, ox + L.cursor_x, row_y)
        if not s.blink_state then
            cursor(api, ox + CONFIRM_X + s.confirm_choice * L.confirm_stride, oy + rows.confirm)
        end
    elseif not s.blink_state then
        cursor(api, ox + L.cursor_x, row_y)
    end
end

return M
