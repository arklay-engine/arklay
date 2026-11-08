-- Room 200's computer console (`Computer200`'s draw, game mode 7), over the
-- cut's background and under the message window.
--
-- What one tick draws comes from `api:re2_computer()` (`computer2::View`).
-- The original's order, back to front:
--   1. `Sprite_Trans`'s black `TILE` (code 2, the average blend) over the
--      console's rectangle, in Marni's back table, so under every sprite;
--   2. the glyph cells, the last record first (same-otag primitives draw the
--      last added first);
--   3. the panel, record 3 (its cell area is transparent, so it never covers
--      a glyph);
--   4. the cursor, record 2, added first and so drawn last;
--   5. `Wframe_set`'s four `LINE_F2` at otag 6, in front of the sprites.
-- Every sprite is on room 200's object page with clut 0 at grey 0x80, so the
-- texture's own colours, which the PC renderer expands from 5 bits as
-- 8v + 7 (arklay's decoder gives v * 255 / 31), as the title's sheets.

local M = {}

local PAGE = "ui/computer/rc200"
local PANEL = { u = 0, v = 117, x = 12, y = 10, w = 252, h = 139 }
local CURSOR = { u = 224, v = 10 }
local CELL_W, CELL_H = 8, 10

local s = {}
M.state = s

function M.open(_api, _args) end

function M.update(_api, _input) end

-- A palette channel as the original's renderer expands it.
local function expand(c)
    local v = (c * 31 + 127) // 255
    return v == 0 and 0 or 8 * v + 7
end

-- The page's clut 0 at grey 0x80; colour 0 stays transparent.
local function lut_of(palette)
    local lut = {}
    for index = 1, 255 do
        local rgb = palette[index]
        if rgb and rgb ~= 0 then
            lut[index] = (expand((rgb >> 16) & 0xFF) << 16)
                | (expand((rgb >> 8) & 0xFF) << 8)
                | expand(rgb & 0xFF)
        end
    end
    return lut
end

local function sprite(api, d, u, v, w, h, x, y)
    api:page_lut(d.page, u, v, w, h, d.ox + x, d.oy + y, d.lut, false)
end

-- `LINE_F2` as the PC paints it: each edge on its own column and row, both
-- ends included, so a zero-length step is the one pixel.
local function frame(api, d, f)
    local x0, y0, x1, y1 = f[1], f[2], f[3], f[4]
    local w, h = x1 - x0 + 1, y1 - y0 + 1
    api:fill_rect(d.ox + x0, d.oy + y0, w, 1, 255, 255, 255)
    api:fill_rect(d.ox + x0, d.oy + y1, w, 1, 255, 255, 255)
    api:fill_rect(d.ox + x0, d.oy + y0, 1, h, 255, 255, 255)
    api:fill_rect(d.ox + x1, d.oy + y0, 1, h, 255, 255, 255)
end

function M.draw(api)
    local view = api:re2_computer()
    if view == nil then
        return
    end
    if s.page == nil then
        s.page = api:page(PAGE) or false
        s.lut = s.page and lut_of(api:page_palette(s.page, 0) or {}) or {}
    end
    local d = { page = s.page, lut = s.lut, ox = api:width() // 2 - 160, oy = api:height() // 2 - 120 }
    if view.drawn and d.page then
        api:fade_tile(0, 0, d.ox + PANEL.x, d.oy + PANEL.y, PANEL.w, PANEL.h)
        local cells = view.cells
        for i = #cells, 1, -1 do
            local c = cells[i]
            sprite(api, d, c[3], c[4], CELL_W, CELL_H, c[1], c[2])
        end
        sprite(api, d, PANEL.u, PANEL.v, PANEL.w, PANEL.h, PANEL.x, PANEL.y)
        if view.cursor then
            sprite(api, d, CURSOR.u, CURSOR.v, CELL_W, CELL_H, view.cursor[1], view.cursor[2])
        end
    end
    if view.frame then
        frame(api, d, view.frame)
    end
end

return M
