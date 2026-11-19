-- RE 2's prologue (`Prologue`, 0x4CEFF0): the pictures and lines a new game
-- shows before it starts, as task 1 under `Title_game_init`'s step 3. The
-- title's script runs it (`ui/title.lua`) with its own fades.
--
-- The executable has three sequences: Leon's and Claire's first scenario
-- (`Prologue_mv00`, 0x4CF990), ported here, and the two a second scenario
-- starts with (`Prologue_mv01`, `Prologue_mv02`), which nothing reaches yet.
--
-- A sprite is a record of `intro_sprt0` (0x533600): a mode byte, a kind, a
-- depth, an entry of `intromap_gfx` (0x533490) or `intromap_sub` (0x533520),
-- a position and a colour. The mode's bits: 1 shown, 4 fading in, 8 fading
-- out, 0x10 scrolling up, 0x20 growing, 0x40 shrinking. Kind 0 is a piece
-- of a picture, kind 2 a picture that zooms about the screen's centre, and
-- kind 1 a line of the Japanese text, which Classic REbirth doesn't draw
-- (its sprites keep their modes all the same).

local M = {}

local KIND_PIECE, KIND_LINE, KIND_ZOOM = 0, 1, 2

-- `intromap_gfx`: the sheet (`ui/prologue/<sheet>`), the texels' size and
-- the size drawn (0 is 256). Entries 4 and 9 are the zooming pictures, whose
-- drawn size `Prologue_load` sets and the zoom changes.
local GFX = {
    [0] = { sheet = "open00_0", w = 256, h = 160 },
    { sheet = "open00_1", w = 64, h = 160 },
    { sheet = "open00_2", w = 256, h = 160 },
    { sheet = "open00_3", w = 64, h = 160 },
    { sheet = "open02_0", w = 189, h = 190 },
    { sheet = "open01_0", w = 256, h = 240 },
    { sheet = "open01_0", w = 256, h = 240 },
    { sheet = "open01_1", w = 64, h = 240 },
    { sheet = "open01_1", w = 64, h = 240 },
    { sheet = "open02_1", w = 189, h = 190 },
    { sheet = "open01_2", w = 256, h = 240 },
    { sheet = "open01_2", w = 256, h = 240 },
    { sheet = "open01_3", w = 64, h = 240 },
    { sheet = "open01_3", w = 64, h = 240 },
}

-- `intro_sprt0`'s 28 records as the executable holds them: the mode, the
-- kind, the depth, the entry, x and y.
local SPRITES = {
    [0] = { 6, KIND_LINE, 4, 0, 32, 180 },
    { 6, KIND_LINE, 4, 0, 32, 180 },
    { 0, KIND_PIECE, 11, 0, 0, 10 },
    { 0, KIND_PIECE, 11, 1, 256, 10 },
    { 0, KIND_PIECE, 11, 2, 0, 170 },
    { 0, KIND_PIECE, 11, 3, 256, 170 },
    { 6, KIND_LINE, 4, 1, 32, 180 },
    { 6, KIND_LINE, 4, 1, 32, 180 },
    { 6, KIND_LINE, 4, 2, 32, 180 },
    { 6, KIND_LINE, 4, 2, 32, 180 },
    { 0, KIND_ZOOM, 10, 4, 160, 120 },
    { 0, KIND_PIECE, 11, 5, 0, 0 },
    { 0, KIND_PIECE, 11, 6, 0, 0 },
    { 0, KIND_PIECE, 11, 7, 256, 0 },
    { 0, KIND_PIECE, 11, 8, 256, 0 },
    { 6, KIND_LINE, 4, 3, 32, 180 },
    { 6, KIND_LINE, 4, 3, 32, 180 },
    { 6, KIND_LINE, 4, 4, 32, 180 },
    { 6, KIND_LINE, 4, 4, 32, 180 },
    { 0, KIND_ZOOM, 10, 9, 160, 120 },
    { 0, KIND_PIECE, 11, 10, 0, 0 },
    { 0, KIND_PIECE, 11, 11, 0, 0 },
    { 0, KIND_PIECE, 11, 12, 256, 0 },
    { 0, KIND_PIECE, 11, 13, 256, 0 },
    { 6, KIND_LINE, 4, 5, 32, 180 },
    { 6, KIND_LINE, 4, 5, 32, 180 },
    { 6, KIND_LINE, 4, 6, 32, 180 },
    { 6, KIND_LINE, 4, 6, 32, 180 },
}
local COUNT = 28

-- The colour a sprite fades in to. A colour is the sprite's weight over
-- the frame, of 0x80: at the full one a quarter of what is under it stays.
local FULL = 0x60

-- The height of a scene's bands.
local BAND = 24

-- The two full-screen pictures: `Work_buffer`'s and `Bg_buffer`'s.
local PICTURE_A, PICTURE_B = "ui/prologue/open10", "ui/prologue/open11"

-- The buttons that skip (`0x9F0`): the title's decide, cancel and START.
local function skip_pressed(keys)
    return keys.confirm or keys.cancel or keys.start
end

-- `Prologue_mv00`'s steps 1 to 5: at each count, in the routine's order,
-- what it does. `mode` is `Prologue_set_mode(no, clear)` over `mask`.
local STEPS = {
    {
        { 240, picture = PICTURE_A, init = true },
        { 420, xa = 1 },
        { 430, mask = 0x3, mode = 0x07, clear = true },
        { 793, mask = 0x3, mode = 0x0B },
        { 823, move = true, mask = 0x3C, mode = 0x17, next = true },
    },
    {
        { 64, xa = 2 },
        { 74, mask = 0xC0, mode = 0x07 },
        { 349, mask = 0xC0, mode = 0x0B },
        { 359, mask = 0x300, mode = 0x07 },
        { 875, mask = 0x300, mode = 0x0B },
        { 905, mask = 0x3C, mode = 0x1B },
        { 905, mask = 0x400, mode = 0x47, next = true },
    },
    {
        { 94, mask = 0x2800, mode = 0x07, xa = 3 },
        { 104, mask = 0x18000, mode = 0x07 },
        { 204, mask = 0x400, mode = 0x4B },
        { 321, mask = 0x18000, mode = 0x0B },
        { 351, picture = PICTURE_B, init = true, mask = 0x2800, mode = 0x0B, next = true },
    },
    {
        { 94, mask = 0, mode = 0, clear = true, xa = 4 },
        { 104, mask = 0x60000, mode = 0x07 },
        { 550, mask = 0x60000, mode = 0x0B },
        { 580, move = true, mask = 0x80000, mode = 0x27, next = true },
    },
    {
        { 96, mask = 0x500000, mode = 0x07, xa = 5 },
        { 106, mask = 0x3000000, mode = 0x07 },
        { 261, mask = 0x3000000, mode = 0x0B },
        { 271, mask = 0xC000000, mode = 0x07 },
        { 261, mask = 0x80000, mode = 0x2B },
        { 445, mask = 0xC000000, mode = 0x0B, leave = true },
    },
}

-- The count step 5 fades out on, and the one step 6 starts the films on.
local LEAVE_COUNT, FILM_COUNT = 445, 100
-- The count a skip comes back into its step with.
local SKIP_COUNT = 0x5A

local function u8(v)
    return v & 0xFF
end

-- A prologue as `Prologue_load` leaves it. `fades` is the title's:
-- `set(a0, add)`, `done(n)`, `off(n)`, `level(n, level)` and `add(n, add)`.
function M.new(fades)
    local p = {
        fades = fades,
        -- The task's steps (+8, +9, +10).
        task = { 0, 0, 0 },
        mv = 0,
        step = 0,
        count = 0,
        mask = 0,
        r = 1,
        fade_flags = 0,
        slot = 0,
        skip_ok = 1,
        skip = 0,
        frames = 0,
        scroll_wait = 0,
        zoom_wait = 0,
        saved_mv = 0,
        -- `Bg_set_mode`: whether the full-screen picture is drawn.
        background = false,
        picture = nil,
        sprites = {},
        sizes = { [4] = { 200, 200 }, [9] = { 20, 20 } },
        -- This tick's sound: a line of the narration (`Xa_play(1, n)`) or
        -- "stop", and the music's "start" (main 0x2B), "fade" or "stop".
        xa = nil,
        music = nil,
        done = false,
    }
    for i = 0, COUNT - 1 do
        local r = SPRITES[i]
        p.sprites[i] = { mode = r[1], kind = r[2], depth = r[3], map = r[4], x = r[5], y = r[6], colour = 0 }
    end
    return p
end

-- `Prologue_set_mode(no, clear)`: the sprites in the mask take `no`, and
-- with `clear` the others go off.
local function set_mode(p, no, clear)
    for i = 0, COUNT - 1 do
        if (p.mask >> i) & 1 ~= 0 then
            p.sprites[i].mode = no
        elseif clear then
            p.sprites[i].mode = 0
        end
    end
end

-- `Prologue_init` (0x4CF860): the fade in of the full-screen picture.
local function init(p)
    if p.fade_flags & 1 == 0 then
        p.fades.set(0x200 | p.slot, -0x100)
        p.fade_flags = p.fade_flags | 1
        p.r = 0
        p.background = true
    elseif p.fades.done(p.slot) then
        p.r = 1
        p.fade_flags = p.fade_flags ~ 1
    end
end

-- `Prologue_move` (0x4CF8E0): the fade out, after which the picture is off.
local function move(p)
    if p.fade_flags & 2 == 0 then
        p.fades.set(0x200 | p.slot, 0x100)
        p.fade_flags = p.fade_flags | 2
        p.r = 2
    elseif p.fades.done(p.slot) then
        if p.fade_flags & 4 == 0 then
            p.r = (p.fade_flags >> 3) & 1
            p.fade_flags = p.fade_flags & 0xF5
        else
            p.r = 3
            p.mask = 0
            p.fade_flags = p.fade_flags & 0xFD
            set_mode(p, 0, true)
        end
        p.background = false
    end
end

-- `Prologue_exit` (0x4CF730): once the fade is over, the task ends.
local function exit(p)
    if p.task[3] == 0 then
        if not p.fades.done(p.slot) then
            return
        end
        p.task[3] = 1
    end
    p.xa = "stop"
    p.music = "stop"
    p.fades.off(0)
    p.fades.off(3)
    p.fades.add(3, 0)
    p.fades.level(3, -1)
    p.background = false
    p.done = true
end

-- One count of `Prologue_mv00`. Returns without counting on when a step
-- starts the next.
local function sequence(p)
    if p.step == 0 then
        p.fades.level(3, 0)
        p.music = "start"
        p.step = 1
        p.slot = 3
        p.count = 1
        return
    end
    local actions = STEPS[p.step]
    if actions then
        for _, action in ipairs(actions) do
            if action[1] == p.count then
                if action.picture then
                    p.picture = action.picture
                end
                if action.init then
                    init(p)
                end
                if action.move then
                    move(p)
                    p.fade_flags = 8
                end
                if action.mask then
                    p.mask = action.mask
                    set_mode(p, action.mode, action.clear)
                end
                if action.xa then
                    p.xa = action.xa
                end
                if action.leave then
                    -- The last fade is slot 0's, over everything, and no
                    -- skip is taken under it.
                    p.music = "fade"
                    p.fade_flags = 8
                    p.slot = 0
                    p.skip_ok = 0
                    p.skip = 2
                    move(p)
                end
                if action.next then
                    p.step = p.step + 1
                    p.count = 1
                    return
                end
            end
        end
        if p.step == 5 and p.count > LEAVE_COUNT and p.fades.done(p.slot) then
            p.skip_ok = 1
            p.mask = 0
            set_mode(p, 0, true)
            p.step = 6
            p.count = 1
            return
        end
    elseif p.step == 6 then
        if p.count == FILM_COUNT then
            -- `Init_movie_work(3)`: with the films off it is over at once.
            p.film = 3
            p.step = 7
            p.count = 1
            return
        end
    elseif p.step == 7 then
        -- The first film is over: `Init_movie_work(0xF)`.
        p.film = 0xF
        p.step = 9
        p.count = 1
        return
    elseif p.step == 9 then
        p.r = 3
        p.mv = 4
    end
    p.count = (p.count + 1) & 0xFFFF
end

-- `Prologue_mv03` (0x4CF170), a skip's wait: the lines go off, and when
-- slot 0's fade is over the sequence comes back in at its films' step.
local function skipped(p)
    for i = 0, COUNT - 1 do
        if p.sprites[i].kind == KIND_LINE then
            p.sprites[i].mode = 0
        end
    end
    if p.skip == 1 then
        p.frames = 0
        if p.fades.done(0) then
            p.mask = 0
            set_mode(p, 0, true)
            p.skip = 2
            p.mv = p.saved_mv
            p.count = SKIP_COUNT
        end
    end
end

-- `Prologue_draw` (0x4D0AB0): the scroll's and the zoom's waits, the fades
-- of the colours, the scroll and the zoom, and the sprites the frame shows
-- (`shown`, by record).
local function animate(p)
    local scroll, zoom = 0, 0
    if p.scroll_wait == 0 then
        p.scroll_wait = 10
        scroll = -1
    else
        p.scroll_wait = p.scroll_wait - 1
    end
    if p.zoom_wait == 0 then
        p.zoom_wait = 3
        zoom = -1
    else
        p.zoom_wait = p.zoom_wait - 1
    end
    p.shown = {}
    for i = 0, COUNT - 1 do
        local sp = p.sprites[i]
        if sp.mode & 1 ~= 0 then
            if sp.kind == KIND_LINE then
                sp.colour = 0x80
                if sp.mode & 8 ~= 0 then
                    sp.mode = 0
                end
            else
                if sp.mode & 4 ~= 0 then
                    sp.colour = sp.colour + 2
                    if sp.colour >= FULL then
                        sp.colour = FULL
                        sp.mode = sp.mode & 0xF9
                    end
                end
                if sp.mode & 8 ~= 0 then
                    sp.colour = sp.colour - (sp.kind == KIND_ZOOM and 1 or 2)
                    if sp.colour <= 0 then
                        sp.colour = 0
                        sp.mode = sp.mode & 0xF7
                    end
                end
                if sp.kind == KIND_PIECE then
                    if sp.mode & 0x10 ~= 0 then
                        sp.y = sp.y + scroll
                        -- The second picture's last piece ends the scroll.
                        if sp.map == 3 and sp.y < 0x50 then
                            for k = 2, 5 do
                                p.sprites[k].mode = 0
                            end
                        end
                    end
                else
                    local size = p.sizes[sp.map]
                    if sp.mode & 0x20 ~= 0 then
                        size[1] = u8(size[1] - zoom * 2)
                        size[2] = u8(size[2] - zoom * 2)
                        if size[1] > 0xE6 then
                            sp.mode = 0
                        end
                    end
                    if sp.mode & 0x40 ~= 0 then
                        size[1] = u8(size[1] + zoom * 2)
                        size[2] = u8(size[2] + zoom * 2)
                        if size[1] < 10 then
                            sp.mode = 0
                        end
                    end
                end
                -- The sprite is drawn as this left it, also on the tick
                -- that puts it out.
                local shown = { depth = sp.depth, map = sp.map, colour = sp.colour }
                if sp.kind == KIND_ZOOM then
                    local size = p.sizes[sp.map]
                    shown.w, shown.h = size[1], size[2]
                    shown.x, shown.y = (0x140 - size[1]) // 2, (0xF0 - size[2]) // 2
                else
                    local g = GFX[sp.map]
                    shown.w, shown.h = g.w, g.h
                    shown.x, shown.y = sp.x, sp.y
                end
                p.shown[i] = shown
            end
        end
    end
end

-- One tick of the task. `keys` are this tick's presses.
function M.tick(p, keys)
    p.xa = nil
    p.music = nil
    p.film = nil
    if p.task[1] == 0 then
        p.task[1] = 1
    end
    if p.skip_ok ~= 0 and skip_pressed(keys) then
        if p.skip == 0 then
            p.skip = 1
            p.xa = "stop"
            p.music = "fade"
            p.fade_flags = 8
            p.fades.add(3, 1)
            p.slot = 0
            move(p)
            p.saved_mv = p.mv
            p.count = 0
            p.step = 6
            p.mv = 4
        elseif p.skip == 2 then
            p.skip = 3
            p.saved_mv = 4
            p.mv = 4
            p.r = 3
        end
    end
    if p.r == 0 then
        init(p)
    elseif p.r == 2 then
        move(p)
    elseif p.r == 3 then
        exit(p)
        if p.done then
            return
        end
    end
    if p.mv == 4 then
        skipped(p)
    else
        sequence(p)
    end
    animate(p)
    p.frames = p.frames + 1
end

-- The main music `Prologue_load` loads for the first scenarios.
local MUSIC = 0x2B

-- Play what this tick asked of the sound. `player` is 1 for Claire.
function M.sound(api, p, player)
    if p.music == "start" then
        api:music(MUSIC)
    elseif p.music == "fade" then
        api:music_fade()
    elseif p.music == "stop" then
        api:music()
    end
    if p.xa == "stop" then
        api:voice()
    elseif p.xa then
        api:voice(p.xa, player)
    end
end

-- The values of a trace's `pro`, `prs` and `prz`.
function M.publish(api, p)
    api:set("pro_on", (p and not p.done) and 1 or 0)
    if not p or p.done then
        return
    end
    api:set("pro_task0", p.task[1])
    api:set("pro_task1", p.task[2])
    api:set("pro_task2", p.task[3])
    api:set("pro_mv", p.mv)
    api:set("pro_step", p.step)
    api:set("pro_count", p.count)
    api:set("pro_r", p.r)
    api:set("pro_flags", p.fade_flags)
    api:set("pro_slot", p.slot)
    api:set("pro_skip_ok", p.skip_ok)
    api:set("pro_skip", p.skip)
    api:set("pro_frames", p.frames)
    api:set("pro_scroll", p.scroll_wait)
    api:set("pro_zoom", p.zoom_wait)
    api:set("pro_mask", p.mask)
    api:set("pro_sprites", COUNT)
    -- Two sprites to a value: the mode, the colour and y of each.
    local function word(sp)
        return (sp.mode << 24) | (sp.colour << 16) | (sp.y & 0xFFFF)
    end
    for i = 0, COUNT - 1, 2 do
        api:set("pro_s" .. (i // 2), word(p.sprites[i]) | (word(p.sprites[i + 1]) << 32))
    end
    api:set("pro_z0", (p.sizes[4][1] << 8) | p.sizes[4][2])
    api:set("pro_z1", (p.sizes[9][1] << 8) | p.sizes[9][2])
end

local pages = {}

local function page(api, sheet)
    if pages[sheet] == nil then
        pages[sheet] = api:page("ui/prologue/" .. sheet) or false
    end
    return pages[sheet]
end

-- Forget the handles: a screen that opens again loads its own.
function M.reset()
    pages = {}
end

-- The frame: the picture under slot 3's fade, then the sprites from the
-- farthest depth. The PC's renderer puts a sprite's texels over the frame
-- at its colour's weight, as the recordings show: a picture fading in takes
-- the one under it away, down to a quarter.
-- `fade(n)` draws a slot of the title's.
function M.draw(api, p, ox, oy, fade)
    if p.background and p.picture then
        p.pictures = p.pictures or {}
        if p.pictures[p.picture] == nil then
            p.pictures[p.picture] = api:picture(p.picture) or false
        end
        if p.pictures[p.picture] then
            api:draw_background(p.pictures[p.picture], ox, oy)
        end
    end
    fade(3)
    for _, depth in ipairs({ 11, 10 }) do
        for i = COUNT - 1, 0, -1 do
            local sp = p.shown and p.shown[i]
            if sp and sp.depth == depth and sp.colour > 0 then
                local g = GFX[sp.map]
                local sheet = page(api, g.sheet)
                if sheet then
                    local alpha = (sp.colour * 255 + 0x40) // 0x80
                    if alpha > 255 then
                        alpha = 255
                    end
                    api:page_sprite_scaled(sheet, 0, 0, g.w, g.h, ox + sp.x, oy + sp.y, sp.w, sp.h, 0, 255, nil, alpha)
                end
            end
        end
    end
    -- `fg_status`'s 0x10, up from `Prologue_load` to the films: a scene's
    -- black bands over the picture and the sprites.
    if p.step < 7 then
        api:fill_rect(ox, oy, 320, BAND, 0, 0, 0)
        api:fill_rect(ox, oy + 240 - BAND, 320, BAND, 0, 0, 0)
    end
    fade(0)
end

return M
