-- RE2's results after a cleared game (`Result`, 0x4B57C0): what
-- `Result_draw_quads` (0x4B69B0) and `System_trans` draw each tick. The task
-- itself runs in the engine (`result2.rs`); `api:re2_result()` gives what it
-- left: which pieces show and their colours, the picture, the time's digits
-- and fade slot 0.
--
-- The art is the original's: `end00.adt`'s four sheets (textures 0x12 to
-- 0x15, `ui/result/end00_<k>`), and the pictures `Result_load_bg` loads
-- (`ui/result/end1<n>`) and HARD's (`ui/result/sp_e`). The pieces'
-- strings, places and glyphs are the executable's tables: `text_result_0`
-- (0x52D178), `text_result` (0x52D308) and `glyph_map` (0x52CF08).
--
-- Every piece and digit is a `POLY_FT4` on the same depth (10), and Marni
-- draws a depth's primitives last-added first: the digits under the pieces,
-- the last piece under the first. A piece's colour modulates its sheet
-- (`0x80` the art's own).

local M = {}

-- `glyph_map`: the sheet (texture 0x12 + n), its rect, and the size drawn.
-- Glyphs 0x36 to 0x38 are drawn smaller than their rect.
local G = {}
local function glyph(index, sheet, u, v, w, h, dw, dh)
    G[index] = { sheet = sheet, u = u, v = v, w = w, h = h, dw = dw or w, dh = dh or h }
end
do
    -- 0x00 to 0x26: the letters, the digits and the signs, one row of 14.
    local row = {
        { 0, 13 }, { 15, 13 }, { 29, 13 }, { 43, 13 }, { 58, 11 }, { 71, 11 }, { 84, 14 },
    }
    for i, g in ipairs(row) do
        glyph(i - 1, 3, g[1], 0, g[2], 14)
    end
    glyph(7, 3, 113, 32, 12, 14)
    local rest = {
        { 102, 6 }, { 112, 13 }, { 127, 11 }, { 140, 14 }, { 154, 13 }, { 168, 14 },
        { 183, 11 }, { 197, 13 }, { 212, 10 }, { 225, 12 }, { 238, 13 },
    }
    for i, g in ipairs(rest) do
        glyph(7 + i, 3, g[1], 0, g[2], 14)
    end
    for i = 0, 13 do
        glyph(0x13 + i, 3, i * 14, 14, 14, 14)
    end
    glyph(0x21, 3, 196, 14, 10, 14)
    glyph(0x22, 3, 213, 14, 8, 14)
    glyph(0x23, 3, 224, 14, 14, 14)
    glyph(0x24, 3, 240, 14, 10, 14)
    glyph(0x25, 3, 152, 32, 10, 14)
    glyph(0x26, 3, 241, 32, 8, 14)
    glyph(0x27, 3, 0, 32, 42, 14)
    glyph(0x28, 3, 56, 32, 42, 14)
    glyph(0x29, 3, 0, 32, 56, 14)
    glyph(0x2A, 3, 56, 32, 56, 14)
    glyph(0x2B, 2, 0, 0, 254, 48)
    glyph(0x2C, 2, 0, 48, 254, 32)
    glyph(0x2D, 2, 0, 80, 254, 32)
    glyph(0x2E, 2, 0, 112, 254, 32)
    glyph(0x2F, 1, 0, 160, 254, 32)
    glyph(0x30, 1, 0, 192, 254, 32)
    glyph(0x31, 1, 0, 224, 254, 31)
    glyph(0x32, 1, 0, 0, 254, 64)
    glyph(0x33, 1, 0, 64, 254, 32)
    glyph(0x34, 1, 0, 96, 254, 32)
    glyph(0x35, 1, 0, 128, 254, 32)
    glyph(0x36, 0, 0, 0, 112, 72, 84, 54)
    glyph(0x37, 0, 0, 72, 112, 72, 84, 54)
    glyph(0x38, 0, 0, 144, 112, 72, 84, 54)
    glyph(0x39, 3, 128, 32, 10, 14)
    glyph(0x3A, 3, 140, 32, 14, 14)
    glyph(0x3B, 3, 156, 32, 10, 14)
    glyph(0x3C, 3, 0, 48, 42, 14)
    glyph(0x3D, 3, 42, 48, 56, 14)
end

-- `text_result`: each string's glyphs.
local STRINGS = {
    [0] = { 0x02, 0x0D, 0x0C, 0x06, 0x0F, 0x00, 0x11, 0x12, 0x0A, 0x00, 0x11, 0x08, 0x0D, 0x0C, 0x10, 0x22, 0x22 },
    { 0x02, 0x0A, 0x04, 0x00, 0x0F, 0x26, 0x06, 0x00, 0x0B, 0x04 },
    { 0x27 }, { 0x28 }, { 0x29 }, { 0x2A },
    { 0x0F, 0x00, 0x0C, 0x09, 0x08, 0x0C, 0x06 },
    { 0x10 }, { 0x00 }, { 0x01 }, { 0x02 }, { 0x03 },
    { 0x11, 0x0D, 0x11, 0x00, 0x0A, 0x26, 0x11, 0x08, 0x0B, 0x04 },
    { 0x0C, 0x12, 0x0B, 0x01, 0x04, 0x0F, 0x26, 0x0D, 0x05, 0x26, 0x10, 0x00, 0x13, 0x04, 0x10 },
    { 0x14, 0x04, 0x00, 0x0E, 0x0D, 0x0C, 0x26, 0x06, 0x04, 0x11 },
    { 0x02, 0x07, 0x04, 0x02, 0x09, 0x26, 0x08, 0x11, 0x04, 0x0B, 0x26, 0x01, 0x0D, 0x15, 0x22, 0x22 },
}
for i = 16, 29 do
    STRINGS[i] = { 0x2B + i - 16 }
end
STRINGS[30] = { 0x3C }
STRINGS[31] = { 0x3D }

-- `text_result_0`: each piece's string and place.
local PIECES = {
    [0] = { 16, 32, 90 }, { 17, 32, 97 }, { 18, 32, 180 }, { 19, 32, 180 },
    { 7, 240, 76 }, { 8, 240, 76 }, { 9, 240, 76 }, { 10, 240, 76 }, { 11, 240, 76 },
    { 2, 224, 108 }, { 3, 224, 108 }, { 4, 219, 108 }, { 5, 219, 108 },
    { 0, 60, 25 }, { 6, 30, 76 }, { 1, 30, 108 }, { 12, 30, 140 }, { 13, 30, 172 },
    { 27, 28, 80 }, { 28, 122, 80 }, { 29, 216, 80 },
    { 21, 32, 150 }, { 22, 32, 150 }, { 14, 99, 30 }, { 15, 71, 200 },
    { 20, 32, 180 }, { 20, 32, 180 }, { 23, 32, 70 }, { 24, 32, 100 },
    { 25, 32, 180 }, { 26, 32, 180 }, { 30, 224, 108 }, { 31, 217, 108 },
}

-- The time's digits: their places (piece 13 shows them, in its colour).
local DIGIT_X = { 195, 209, 223, 233, 247, 261, 271, 285, 233, 247 }
local DIGIT_Y = { 140, 140, 140, 140, 140, 140, 140, 140, 172, 172 }
local TIME_PIECE = 13

local s = {}
M.state = s

function M.open(_api, _args) end

function M.update(_api, _input) end

local function sheet(api, n)
    s.sheets = s.sheets or {}
    if s.sheets[n] == nil then
        s.sheets[n] = api:page("ui/result/end00_" .. n) or false
    end
    return s.sheets[n]
end

local function rgb(colour)
    return { colour & 0xFF, (colour >> 8) & 0xFF, (colour >> 16) & 0xFF }
end

function M.draw(api)
    local view = api:re2_result()
    if view == nil then
        return
    end
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    if view.background and view.picture_name then
        s.pictures = s.pictures or {}
        local p = view.picture_name
        if s.pictures[p] == nil then
            s.pictures[p] = api:picture("ui/result/" .. p) or false
        end
        if s.pictures[p] then
            api:draw_background(s.pictures[p], ox, oy)
        end
    end
    if view.drawn then
        -- The quads in the order `Result_draw_quads` adds them.
        local quads = {}
        for n = 0, #PIECES do
            local piece = view.pieces[n + 1]
            if piece and piece.flags & 1 ~= 0 then
                local place = PIECES[n]
                local x = place[2]
                for _, g in ipairs(STRINGS[place[1]]) do
                    quads[#quads + 1] = { g = g, x = x, y = place[3], colour = piece.colour }
                    x = x + G[g].dw
                end
            end
        end
        local time = view.pieces[TIME_PIECE + 1]
        if time and time.flags & 1 ~= 0 then
            for i = 10, 1, -1 do
                quads[#quads + 1] = { g = view.digits[i], x = DIGIT_X[i], y = DIGIT_Y[i], colour = time.colour }
            end
        end
        for i = #quads, 1, -1 do
            local q = quads[i]
            local g = G[q.g]
            local page = g and sheet(api, g.sheet)
            if page then
                api:page_sprite_modulated(page, g.u, g.v, g.w, g.h, ox + q.x, oy + q.y, g.dw, g.dh, 0, rgb(q.colour), "seven")
            end
        end
    end
    local fade = view.fade
    if fade then
        local level = (fade.level >> 7) & 0xFF
        if level ~= 0 then
            api:fade_tile(fade.mode, level)
        end
    end
end

return M
