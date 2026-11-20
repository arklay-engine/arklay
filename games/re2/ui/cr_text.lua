-- Classic REbirth's own print (its `SavePrint`), which the save screen's
-- rows and the option screen's sound page are set with: the message font,
-- each glyph a pixel closer than the width the executable's table gives it
-- (`width_tbl`, the font's own metrics). The frames show the digits and
-- some punctuation apart from that (`{dx, advance less the font's, dy}`):
-- each goes by its ink, which starts a pixel after the cursor and is as
-- wide as the advance, where the table has every digit 12 wide from a pixel
-- in; and the digits, `/` and `-` sit a row higher (a save's label on the
-- load screen, `title_load_game`).
local M = {}

local TABLE = {
    ["."] = { 1, -1, 0 },
    [":"] = { 0, -3, 0 },
    ["/"] = { 1, -1, -1 },
    ["-"] = { 0, -3, -1 },
    ["0"] = { 1, -1, -1 },
    ["1"] = { -3, -8, -1 },
    ["2"] = { 0, -3, -1 },
    ["3"] = { 0, -2, -1 },
    ["4"] = { 1, 0, -1 },
    ["5"] = { 0, -2, -1 },
    ["6"] = { 1, -2, -1 },
    ["7"] = { 0, -2, -1 },
    ["8"] = { 0, -2, -1 },
    ["9"] = { 1, -1, -1 },
}

-- The same places over a font whose widths are each glyph's ink and a
-- pixel (Classic REbirth's Western sheet, in a pack converted without
-- `font0.tim`): there `f` keeps its width, `O` and `Q` are two pixels
-- closer and `?` sits a pixel later and is a pixel wider, where the table
-- has them all a pixel closer than its own widths.
local INK = {
    f = { 0, 0, 0 },
    O = { 0, -2, 0 },
    Q = { 0, -2, 0 },
    ["?"] = { 1, 1, 0 },
    ["."] = { 1, -1, 0 },
    [":"] = { 1, -1, 0 },
    ["/"] = { 1, -1, -1 },
    ["-"] = { 1, -1, -1 },
}
for digit = 0, 9 do
    INK[tostring(digit)] = { 1, -1, -1 }
end

-- The widths that tell the table's font: every glyph whose entry differs
-- between the two, as the table has it. A mod's font with widths of its own
-- is printed as the Western sheet is.
local SIGN = { [":"] = 7, ["-"] = 14, ["?"] = 10, f = 9, O = 12, Q = 13 }
for digit = 0, 9 do
    SIGN[tostring(digit)] = 12
end

-- Whether the font's widths are the executable's table's.
function M.by_table(api)
    for glyph, width in pairs(SIGN) do
        if api:text_width(glyph) ~= width then
            return false
        end
    end
    return true
end

local function glyphs(api)
    return M.by_table(api) and TABLE or INK
end

-- One character of a line: a save's row is its file's name, which may
-- hold more than ASCII.
local CHARACTER = "[\0-\x7F\xC2-\xFD][\x80-\xBF]*"

local function glyph_width(api, glyph, odd)
    return api:text_width(glyph) + (odd and odd[2] or -1)
end

-- How wide `text` prints.
function M.width(api, text)
    local w, odd = 0, glyphs(api)
    for glyph in text:gmatch(CHARACTER) do
        w = w + glyph_width(api, glyph, odd[glyph])
    end
    return w
end

-- `text` from `x`, with `api:text`'s options (its colour).
function M.print(api, x, y, text, opts)
    local odds = glyphs(api)
    for glyph in text:gmatch(CHARACTER) do
        local odd = odds[glyph]
        api:text(x + (odd and odd[1] or 0), y + (odd and odd[3] or 0), glyph, opts)
        x = x + glyph_width(api, glyph, odd)
    end
end

return M
