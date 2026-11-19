-- Classic REbirth's own print (its `SavePrint`), which the save screen's
-- rows and the option screen's sound page are set with: the message font,
-- each glyph a pixel closer than its width. The frames show some glyphs
-- apart from that (`{dx, advance less the font's, dy}`): `f` keeps its
-- width, `O` and `Q` are two pixels closer, `?` sits a pixel later and is
-- a pixel wider, `.` and `:` sit a pixel later, and the digits, `/` and `-`
-- sit a pixel later and a row higher (a save's label on the load screen,
-- `title_load_game`).
local M = {}

local GLYPHS = {
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
    GLYPHS[tostring(digit)] = { 1, -1, -1 }
end

-- One character of a line: a save's row is its file's name, which may
-- hold more than ASCII.
local CHARACTER = "[\0-\x7F\xC2-\xFD][\x80-\xBF]*"

local function glyph_width(api, glyph)
    local odd = GLYPHS[glyph]
    return api:text_width(glyph) + (odd and odd[2] or -1)
end

-- How wide `text` prints.
function M.width(api, text)
    local w = 0
    for glyph in text:gmatch(CHARACTER) do
        w = w + glyph_width(api, glyph)
    end
    return w
end

-- `text` from `x`, in `colour` (the font's own when nil).
function M.print(api, x, y, text, colour)
    local opts = { colour = colour }
    for glyph in text:gmatch(CHARACTER) do
        local odd = GLYPHS[glyph]
        api:text(x + (odd and odd[1] or 0), y + (odd and odd[3] or 0), glyph, opts)
        x = x + glyph_width(api, glyph)
    end
end

return M
