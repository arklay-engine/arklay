-- RE2's gallery, as the original runs it: the task the title starts from
-- GALLERY's page (`Gallery`, 0x4BF760) and its menu, DATA GALLERY
-- (`GalleryMain`, 0x439A80, the first case of `GalleryLoop`, 0x43AE60).
-- The title's task sleeps under it.
--
-- The gallery draws at 640 x 480 with its own art (`ui/gallery/` in the
-- pack): the picture's five sheets, `title1` to `title5`, and the rows'
-- words, `moji`. The places below are the executable's, in those pixels;
-- the frame draws them at half size. With a 320 x 240 display the original
-- has a second layout with smaller art (`top32_1`, `top32_2` and
-- `moji3224`), which isn't ported.
--
-- The state keeps the original's work (`pGallery`) under its own numbers,
-- so the oracle's `gal` field compares one for one.
--
-- The menu's rows: ROUGH SKETCH (LEON) or ILLUSTRATION (CLAIRE), MOVIE, 3D
-- MODEL (CLAIRE only) and EXIT. EXIT ends the gallery and wakes the title.
-- A button on another row fades the menu out and, in the original, opens
-- that row's screen (`GalleryIllustrations`, `GalleryMovie`,
-- `Gallery3DArt`); those aren't ported, and the menu loads again instead.

local M = {}

-- `Gallery_Padd_trg`'s bits: up, down, and any button.
local PAD_UP, PAD_DOWN, PAD_BUTTONS = 0x1000, 0x4000, 0x0FFF

-- `GallerySeOn`'s core-bank cues.
local SE_CURSOR, SE_DECIDE = 4, 6

-- The work's flags (`+0x1478`): the fade's black is drawn, the fade goes
-- out rather than in, and the lit row's level is falling.
local FADING, FADING_OUT, FALLING = 0x01, 0x02, 0x20

-- A row that isn't lit.
local DIM = 0x40

-- The picture: each sheet's place and the corner of the sheet it shows.
local PICTURE = {
    { "title1", 0, 0, 256, 256, 0 },
    { "title2", 256, 0, 256, 256, 0 },
    { "title3", 0, 256, 256, 224, 0 },
    { "title4", 256, 256, 256, 224, 0 },
    { "title5", 512, 0, 128, 256, 0 },
    { "title5", 512, 256, 128, 224, 128 },
}

-- Each character's menu. `words`: the rows as drawn, each with the row
-- number that lights it and its place, its size and its corner in `moji`.
-- `rows`: where up, down and a button take the cursor from a row, and the
-- level the button's fade starts at; a row with no `to` is EXIT. `leave`:
-- the screen a fading row opens (`GalleryLoop`'s case).
local MENUS = {
    [false] = {
        words = {
            { 0, 198, 342, 244, 22, 0, 0 },
            { 1, 270, 386, 103, 22, 0, 22 },
            { 2, 280, 430, 79, 21, 0, 110 },
        },
        rows = {
            [0] = { up = 2, down = 1, to = 3, fade = 0xFF },
            [1] = { up = 0, down = 2, to = 4, fade = 0x80 },
            [2] = { up = 1, down = 0 },
        },
        leave = { [3] = 1, [4] = 2 },
    },
    [true] = {
        words = {
            { 0, 208, 326, 226, 22, 0, 44 },
            { 1, 270, 362, 103, 22, 0, 22 },
            { 5, 242, 398, 157, 22, 0, 66 },
            { 2, 280, 434, 79, 21, 0, 110 },
        },
        rows = {
            [0] = { up = 2, down = 1, to = 3, fade = 0x80 },
            [1] = { up = 0, down = 5, to = 6, fade = 0x80 },
            [5] = { up = 1, down = 2, to = 7, fade = 0x80 },
            [2] = { up = 5, down = 0 },
        },
        leave = { [3] = 1, [6] = 2, [7] = 3 },
    },
}

-- The task as `Gallery`'s first step leaves it, on the tick the title
-- starts it: step 1, with the work it has just cleared.
function M.new(claire)
    return {
        task = 1,
        screen = 0,
        step = 0,
        row = 0,
        pulse = 0,
        fade = 0,
        flags = 0,
        claire = claire and true or false,
    }
end

-- `GalleryDisp` (0x436F20): the fade's black over the frame, at the level
-- it has before this tick's step of 8.
local function disp(g)
    if g.flags & FADING == 0 then
        return
    end
    if g.fade < 1 then
        g.flags = g.flags & ~FADING
        return
    end
    g.black = g.flags & FADING_OUT ~= 0 and 0xFF - g.fade or g.fade
    g.fade = g.fade - 8
end

-- `GalleryMain`: `false` once EXIT is taken.
local function main(g, edge, cue)
    if g.step == 0 then
        -- The art is loaded and the menu fades in from the next tick.
        g.step = 1
        g.flags = (g.flags | FADING) & ~FADING_OUT
        g.fade = 0x80
        disp(g)
        return true
    end
    local menu = MENUS[g.claire]
    local at = g.row
    local row = menu.rows[at]
    local alive, dark = true, row == nil
    if row then
        if edge & PAD_UP ~= 0 then
            cue(SE_CURSOR)
            g.row = row.up
            g.pulse = 0x80
        end
        if edge & PAD_DOWN ~= 0 then
            cue(SE_CURSOR)
            g.row = row.down
            g.pulse = 0x80
        end
        if edge & PAD_BUTTONS ~= 0 then
            cue(SE_DECIDE)
            if row.to then
                g.row = row.to
                g.flags = g.flags | FADING | FADING_OUT
                g.fade = row.fade
            else
                alive = false
                -- CLAIRE's EXIT leaves its tick's rows unlit.
                dark = g.claire
            end
        end
    elseif g.fade < 1 then
        -- The faded row's screen starts from the next tick, and this one
        -- draws nothing. None of them is ported: the menu starts again.
        g.step = 0
        g.row = 0
        return true
    end
    -- The rows as the tick found the cursor: its row at the lit level (a
    -- move has just put that back to full), the others dim, and all of them
    -- black while a row fades out.
    g.words = {}
    for k, word in ipairs(menu.words) do
        g.words[k] = dark and 0 or (word[1] == at and g.pulse or DIM)
    end
    if g.flags & FALLING == 0 then
        g.pulse = g.pulse + 5
        if g.pulse > 0x7F then
            g.flags = g.flags | FALLING
            g.pulse = 0x80
        end
    else
        g.pulse = g.pulse - 5
        if g.pulse < 1 then
            g.flags = g.flags & ~FALLING
            g.pulse = 0
        end
    end
    disp(g)
    return alive
end

-- One tick of the task. `edge` is the pad's pressed bits and `cue` plays a
-- core-bank cue. The task is over (`g.task` 0) the tick after EXIT.
function M.tick(g, edge, cue)
    g.words = nil
    g.black = nil
    g.ticked = true
    if g.task == 2 then
        -- `GalleryClose`, and the title's task is signalled.
        g.task = 0
        return
    end
    if not main(g, edge, cue) then
        g.task = 2
    end
end

-- The trace's `gal`.
function M.publish(api, g)
    api:set("gal_on", g and g.task ~= 0 and 1 or 0)
    if g then
        api:set("gal_task", g.task)
        api:set("gal_screen", g.screen)
        api:set("gal_step", g.step)
        api:set("gal_row", g.row)
        api:set("gal_pulse", g.pulse)
        api:set("gal_fade", g.fade)
        api:set("gal_flags", g.flags)
    end
end

local art

function M.reset()
    art = nil
end

local function load(api)
    art = {}
    for _, stem in ipairs({ "title1", "title2", "title3", "title4", "title5", "moji" }) do
        local page = api:indexed_page("ui/gallery/" .. stem)
        if page then
            art[stem] = { page = page, palette = api:page_palette(page, 0) or {}, luts = {} }
        end
    end
end

-- A sheet's 5-bit level `v` as the original shows it, 8v + 7, and its 0
-- as 0.
local function expand(c)
    local v = (c * 31 + 127) // 255
    return v == 0 and 0 or 8 * v + 7
end

-- A sheet's colours at `level` (0x80 is its own), keyed by texel index.
-- Black is see-through unless the sheet is `opaque`.
local function lut_for(sheet, level, opaque)
    local lut = sheet.luts[level]
    if lut then
        return lut
    end
    lut = {}
    local function at(c)
        return (expand(c) * level + 64) >> 7
    end
    for index = 0, 255 do
        local rgb = sheet.palette[index]
        if rgb and (opaque or rgb ~= 0) then
            lut[index] = (at((rgb >> 16) & 0xFF) << 16) | (at((rgb >> 8) & 0xFF) << 8) | at(rgb & 0xFF)
        end
    end
    sheet.luts[level] = lut
    return lut
end

-- A piece of a sheet at half size, from its place and size at 640 x 480,
-- with the picture's corner at `ox, oy`. A piece with an odd side takes
-- one more texel, which is see-through in `moji`.
local function half(api, sheet, ox, oy, x, y, w, h, u, v, level, opaque)
    if not sheet then
        return
    end
    local dw, dh = (w + 1) // 2, (h + 1) // 2
    local lut = lut_for(sheet, level, opaque)
    local options = { dw = dw, dh = dh, blend = 255 }
    api:page_lut(sheet.page, u, v, dw * 2, dh * 2, ox + x // 2, oy + y // 2, lut, false, options)
end

-- The frame the last tick left, with the 320 x 240 picture's corner at
-- `ox, oy`: nothing on a tick that drew no menu, else the picture, the
-- rows and the fade's black.
function M.draw(api, g, ox, oy)
    if not g.words then
        return
    end
    if not art then
        load(api)
    end
    for _, sheet in ipairs(PICTURE) do
        half(api, art[sheet[1]], ox, oy, sheet[2], sheet[3], sheet[4], sheet[5], sheet[6], 0, 0x80, true)
    end
    for k, word in ipairs(MENUS[g.claire].words) do
        half(api, art.moji, ox, oy, word[2], word[3], word[4], word[5], word[6], word[7], g.words[k], false)
    end
    if g.black then
        api:darken(g.black)
    end
end

return M
