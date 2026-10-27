-- Classic REbirth-style save/load screen.
--
-- The folder-based typewriter screen: every save file in the save directory
-- is a row (labelled with its base name, CR's `_`/`^` file-name stand-ins
-- shown as `/`/`:`), the selected row is a highlight band, and a detail line
-- shows the highlighted save's character, save count and location. Save mode puts a `Create New` row above the files; the last
-- row is `Exit`. A new save is named the way CR names one
-- (`Chris_00_2026-10-23 17^50.18.dat`).
--
-- The mod's `mod.lua` registers this entry with `api:register_save_screen`;
-- the engine knows nothing about the screen itself. The API it uses is the
-- general screen surface documented in docs/modding.md, and its strings are
-- the `UI_SAVE_CR_*` keys this mod ships under `language/`.

local VISIBLE_ROWS = 10 -- rows drawn at once (CR shows ten)
local ROW_Y = 49 -- the first row's y (CR's ink sits 2 px below it)
local ROW_PITCH = 15 -- row pitch, CR's 15 px
local TITLE_Y = 20 -- the centred title's y (CR's ink sits at 21..33)
local DETAIL_Y = 216 -- the detail line's y (CR's ink sits at 217..229)
-- The list panel and the selection band, measured from CR 1.1.3: a grey wash
-- over the full ten-row list and a dark band across the selected row.
local PANEL_X = 10
local PANEL_W = 300
local PANEL_Y = 45
local PANEL_H = 156
local BAND_H = 14
-- CR's character colours, over the sheet's grey ramp: a save row and the
-- detail line's name take the saved character's.
local BLUE = "#3F7DFA" -- Jill
local GREEN = "#00FF00" -- Chris

local mode = "load"
local saves = {}
local cursor = 0 -- row index (not save index); the final row is Exit
local scroll = 0 -- the first visible row
local message = nil -- an error line, cleared by any key
local leaving = false -- the load screen's fade-out is running
local fade_alpha = 0 -- its black-overlay opacity

local function clamp(value, low, high)
    if value < low then
        return low
    end
    if value > high then
        return high
    end
    return value
end

-- The row count: the save files, the save mode's `Create New` row and the
-- exit row.
local function new_rows()
    return mode == "save" and 1 or 0
end

local function row_count()
    return #saves + new_rows() + 1
end

-- The save file under the cursor, or nil for the `Create New` and exit rows.
local function selected()
    return saves[cursor + 1 - new_rows()]
end

-- Whether `row` is save mode's `Create New` row.
local function is_new_row(row)
    return mode == "save" and row == 0
end

-- Whether the cursor is on save mode's `Create New` row.
local function create_new()
    return is_new_row(cursor)
end

-- A language string, or `fallback` when the pack does not carry the key.
local function string_or(api, key, fallback)
    return api:string(key) or fallback
end

-- The pixel width of `text` at CR's metrics. CR draws every line with its
-- proportional width table (`tight`), on the Japanese sheet too.
local function text_width(api, text)
    return api:text_width(text, true)
end

-- Draw `text` at CR's metrics, white unless `colour` is given.
local function draw_text(api, x, y, text, colour)
    api:text(x, y, text, { colour = colour or "white", tight = true })
end

-- The x that centres `text` the way CR does: by its own measure, which can
-- differ from the drawn width (see `api:classic_rebirth_width`).
local function centre_x(api, text)
    return (320 - api:classic_rebirth_width(text)) // 2
end

-- Draw one centred line.
local function draw_line(api, text, y, colour)
    draw_text(api, centre_x(api, text), y, text, colour)
end

-- The colour of a loadable save's character.
local function character_colour(entry)
    return entry.character == 0 and GREEN or BLUE
end

-- Whether `name` is taken by a listed save. Save names compare
-- case-insensitively, like the save directory's own listing (Windows
-- filesystems may hand back either case).
local function name_used(name, except)
    local lower = string.lower(name)
    for _, entry in ipairs(saves) do
        if entry ~= except and string.lower(entry.name) == lower then
            return true
        end
    end
    return false
end

-- CR's name for the pending save: `<character>_<saves>_<local time>.dat`,
-- e.g. `Chris_00_2026-10-23 17^50.18.dat`. CR names the file with the English
-- character name in every language, so the name is read from `en-US`. Nil
-- when the engine cannot supply the parts.
local function classic_rebirth_name(api)
    local pending = api:pending()
    local time = api:local_time()
    if not pending or not time then
        return nil
    end
    local character = api:string(string.format("UI_SAVE_CHARACTER_%d_NAME", pending.character & 1), "en-US")
    if not character then
        return nil
    end
    local date = string.format(
        "%d-%02d-%02d %02d^%02d.%02d",
        time.year,
        time.month,
        time.day,
        time.hour,
        time.minute,
        time.second
    )
    return string.format("%s_%02d_%s.dat", character, pending.saves, date)
end

-- The name a new save takes: CR's, else the first free `save_NN.dat`; nil
-- when every candidate is taken. `replacing` is the row a save over will
-- delete, whose name is free to reuse.
local function new_name(api, replacing)
    local name = classic_rebirth_name(api)
    if name and not name_used(name, replacing) then
        return name
    end
    for index = 1, 9999 do
        name = string.format("save_%02d.dat", index)
        if not name_used(name, replacing) then
            return name
        end
    end
    if not name_used("save.dat", replacing) then
        return "save.dat"
    end
    return nil
end

-- A save file's row label: its base name with CR's file-name stand-ins
-- shown as the characters they replace (`_` for `/`, `^` for `:`).
local function row_label(entry)
    return (string.gsub(entry.base, "[_^]", { ["_"] = "/", ["^"] = ":" }))
end

function on_open(api)
    mode = api:mode()
    saves = {}
    for _, entry in ipairs(api:saves()) do
        -- The engine rotates the autosaves itself; a typewriter save never
        -- overwrites one.
        if mode ~= "save" or not entry.autosave then
            saves[#saves + 1] = entry
        end
    end
    -- CR lists the most recently written file first, by its sub-second
    -- modification time.
    table.sort(saves, function(a, b)
        if a.timestamp ~= b.timestamp then
            return a.timestamp > b.timestamp
        end
        return a.name < b.name
    end)
    cursor = 0
    scroll = 0
    message = nil
    leaving = false
    fade_alpha = 0
end

-- Leave the screen. A save screen hands straight back to the room, which
-- fades itself in (the vanilla screen does not fade there either); the title's
-- load screen fades to black first, like the original.
local function leave(api)
    if mode == "save" then
        api:exit()
    else
        leaving = true
    end
end

-- Report that the save directory has no free file name.
local function no_name(api)
    api:sound("cancel")
    message = string_or(api, "UI_SAVE_CR_NO_FREE_NAME", "No free save name")
end

-- Write the pending block to a fresh name and leave on success.
local function save_new(api)
    local name = new_name(api)
    if not name then
        no_name(api)
        return
    end
    api:sound("save_confirm")
    local ok, err = api:save(name)
    if ok then
        leave(api)
    else
        api:sound("cancel")
        message = err
    end
end

-- Replace the selected row: delete its file (when it is a save), write a new
-- one and leave on success.
local function save_over(api, entry)
    local name = new_name(api, entry.loadable and entry or nil)
    if not name then
        no_name(api)
        return
    end
    api:sound("save_confirm")
    -- CR overwrites by deleting the old file and writing a new one. Only an
    -- exact name match is overwritten in place: a case variant is a second
    -- file on a case-sensitive file system.
    if entry.loadable and entry.name ~= name then
        api:remove(entry.name)
    end
    local ok, err = api:save(name)
    if ok then
        leave(api)
    else
        api:sound("cancel")
        message = err
    end
end

function update(api, input)
    if leaving then
        fade_alpha = fade_alpha + 16
        api:fade(fade_alpha)
        if fade_alpha >= 255 then
            api:exit()
        end
        return
    end

    if message then
        if input.any then
            message = nil
        end
        return
    end

    if input.up then
        cursor = cursor - 1
        if cursor < 0 then
            cursor = row_count() - 1
        end
        api:sound("save_cursor")
    elseif input.down then
        cursor = cursor + 1
        if cursor > row_count() - 1 then
            cursor = 0
        end
        api:sound("save_cursor")
    elseif input.confirm then
        local entry = selected()
        if create_new() then
            save_new(api)
        elseif not entry then
            api:sound("cancel")
            leave(api)
        elseif mode == "load" then
            if entry.loadable then
                api:sound("save_confirm")
                local ok, err = api:load(entry.name)
                if not ok then
                    api:sound("cancel")
                    message = err
                end
            else
                api:sound("cancel")
            end
        else
            save_over(api, entry)
        end
    elseif input.cancel then
        api:sound("cancel")
        leave(api)
    end

    -- Keep the cursor inside the visible window.
    if cursor < scroll then
        scroll = cursor
    end
    if cursor >= scroll + VISIBLE_ROWS then
        scroll = cursor - VISIBLE_ROWS + 1
    end
    scroll = clamp(scroll, 0, clamp(row_count() - VISIBLE_ROWS, 0, row_count() - 1))
end

function draw(api)
    -- The vanilla typewriter photo; a pack without it gets a black screen.
    if not pcall(api.background, api, "ui/type00.bmp") then
        api:rect(0, 0, 320, 240, 0x000000)
    end

    local title
    if mode == "save" then
        title = string_or(api, "UI_SAVE_CR_SAVE", "Save")
    else
        title = string_or(api, "UI_SAVE_CR_LOAD", "Load")
    end
    draw_line(api, title, TITLE_Y)

    local rows = row_count()
    api:rect(PANEL_X, PANEL_Y, PANEL_W, PANEL_H, 0x797979, 76)

    -- The selection band follows the cursor.
    local band_y = ROW_Y + (cursor - scroll) * ROW_PITCH
    api:rect(PANEL_X, band_y, PANEL_W, BAND_H, 0x000000, 191)

    for index = 0, VISIBLE_ROWS - 1 do
        local row = scroll + index
        if row >= rows then
            break
        end
        local y = ROW_Y + index * ROW_PITCH
        local entry = saves[row + 1 - new_rows()]
        if is_new_row(row) then
            draw_line(api, string_or(api, "UI_SAVE_CR_CREATE_NEW", "Create New"), y)
        elseif entry then
            -- CR draws a save file's name in its character's colour and the
            -- exit row white. An autosave is labelled as one, with its
            -- rotation index.
            local label = row_label(entry)
            if entry.autosave then
                label = string_or(api, "UI_SAVE_CR_AUTOSAVE", "Autosave") .. " " .. entry.autosave
            end
            draw_line(api, label, y, entry.loadable and character_colour(entry) or "grey")
        else
            draw_line(api, string_or(api, "UI_SAVE_CR_EXIT", "Exit"), y)
        end
    end

    local entry = selected()
    if create_new() then
        -- The `Create New` row has no save to describe.
    elseif entry and entry.loadable then
        -- CR draws the character name in its colour and the count/location
        -- white, the whole line centred by CR's own measure.
        local name = entry.character_name or "?"
        local rest = string.format("/%02d/%s", entry.saves or 0, entry.location or "")
        local x = centre_x(api, name .. rest)
        draw_text(api, x, DETAIL_Y, name, character_colour(entry))
        draw_text(api, x + text_width(api, name), DETAIL_Y, rest)
    else
        local detail
        if entry then
            detail = string_or(api, "UI_SAVE_CR_UNLOADABLE", "Not loadable")
        elseif mode == "save" then
            detail = string_or(api, "UI_SAVE_CR_DO_NOT_SAVE", "Do not save")
        else
            detail = string_or(api, "UI_SAVE_CR_DO_NOT_LOAD", "Do not load")
        end
        draw_line(api, detail, DETAIL_Y)
    end

    if message then
        draw_line(api, message, 224, "red")
    end
end
