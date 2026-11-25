-- RE 2's prologue (`Prologue`, 0x4CEFF0): the pictures and lines a game
-- shows before it starts, as task 1 under `Title_game_init`'s step 3. The
-- title's script runs it (`ui/title.lua`) with its own fades.
--
-- `Prologue_mv` has a sequence for each scenario, by `fg_status`'s bits 31
-- (Claire) and 30 (the second scenario): Leon's and Claire's first
-- scenario (`Prologue_mv00`, 0x4CF990, entries 0 and 1), Leon's second
-- (`Prologue_mv01`, 0x4CFF90, entry 2) and Claire's (`Prologue_mv02`,
-- 0x4D0470, entry 3), and a skip's wait (`Prologue_mv03`, entry 4). A new
-- game always starts a first scenario; a second starts from the save a
-- main game's results write, whose place (0xF or 0x10) `Prologue_load`
-- takes as a scenario's start.
--
-- A sprite is a record of `intro_sprt0` (0x533600, the first scenarios'),
-- `intro_sprt1` (0x533750, Leon's second) or `intro_sprt2` (0x5337E0,
-- Claire's second): a mode byte, a kind, a depth, an entry of
-- `intromap_gfx` (0x533490) or `intromap_sub` (0x533520), a position and a
-- colour. The mode's bits: 1 shown, 4 fading in, 8 fading out, 0x10
-- scrolling up, 0x20 growing, 0x40 shrinking. Kind 0 is a piece of a
-- picture, kind 2 a picture that zooms about the screen's centre, and kind
-- 1 a line of the Japanese text, which Classic REbirth doesn't draw (its
-- sprites keep their modes all the same). The second scenarios' sprites
-- are all lines: what they show is their two full-screen pictures.

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
local SPRITES_A = {
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

-- A second scenario's lines, two records to a line: `intro_sprt1`'s 12
-- from `intromap_sub`'s entry 7, `intro_sprt2`'s 18 from entry 13, at
-- (32, 180), or (32, 196) for a line half as high.
local function lines(first, ys)
    local records = {}
    for i, y in ipairs(ys) do
        local record = { 6, KIND_LINE, 4, first + i - 1, 32, y }
        records[2 * i - 2] = record
        records[2 * i - 1] = record
    end
    return records
end
local SPRITES_LEON_B = lines(7, { 180, 180, 180, 180, 180, 180 })
local SPRITES_CLAIRE_B = lines(13, { 180, 180, 196, 180, 196, 180, 180, 196, 196 })

-- The colour a sprite fades in to. A colour is the sprite's weight over
-- the frame, of 0x80: at the full one a quarter of what is under it stays.
local FULL = 0x60

-- The height of a scene's bands.
local BAND = 24

-- The full-screen pictures `Prologue_load` reads into `Work_buffer` and
-- `Bg_buffer`: the first scenarios' zombie and helicopter, Leon B's
-- `open13` and Claire B's `open12`, and the second scenarios' `open14`.
local PICTURE_A, PICTURE_B = "ui/prologue/open10", "ui/prologue/open11"
local PICTURE_LEON_B, PICTURE_CLAIRE_B = "ui/prologue/open13", "ui/prologue/open12"
local PICTURE_B2 = "ui/prologue/open14"

-- The buttons that skip (the raw pad's `0x9F0`): decide, cancel, START and
-- the PC's map key.
local function skip_pressed(keys)
    return keys.confirm or keys.cancel or keys.start or keys.map
end

-- A sequence's steps from 1 to its last fade's: at each count, in the
-- routine's order, what it does. `mode` is `Prologue_set_mode(no, clear)`
-- over `mask`, `xa` the narration's line (`Xa_play(1, n)`), `picture` and
-- `init` the full-screen picture fading in (`BgToSurface`,
-- `Prologue_init`), `move` it fading out, `leave` the last fade, and
-- `next` the step's end. `Prologue_mv00`'s:
local STEPS_A = {
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

-- `Prologue_mv01`'s, Leon B's.
local STEPS_LEON_B = {
    {
        { 240, xa = 6 },
        { 250, mask = 0x3, mode = 0x07 },
        { 523, mask = 0x3, mode = 0x0B, next = true },
    },
    {
        { 1, xa = 7 },
        { 11, mask = 0xC, mode = 0x07 },
        { 190, xa = 8 },
        { 388, mask = 0xC, mode = 0x0B, picture = PICTURE_LEON_B, init = true, next = true },
    },
    {
        { 128, xa = 9 },
        { 138, mask = 0x30, mode = 0x07 },
        { 336, mask = 0x30, mode = 0x0B },
        { 396, move = true, next = true },
    },
    {
        { 60, xa = 10 },
        { 70, mask = 0xC0, mode = 0x07 },
        { 274, mask = 0xC0, mode = 0x0B, next = true },
    },
    {
        { 1, xa = 11 },
        { 11, mask = 0x300, mode = 0x07 },
        { 253, mask = 0x300, mode = 0x0B, picture = PICTURE_B2, init = true, next = true },
    },
    {
        { 128, xa = 12 },
        { 138, mask = 0xC00, mode = 0x07 },
        { 307, mask = 0xC00, mode = 0x0B },
        { 517, leave = true },
    },
}

-- `Prologue_mv02`'s, Claire B's.
local STEPS_CLAIRE_B = {
    {
        { 240, xa = 6 },
        { 250, mask = 0x3, mode = 0x07 },
        { 523, mask = 0x3, mode = 0x0B, next = true },
    },
    {
        { 1, xa = 7 },
        { 11, mask = 0xC, mode = 0x07 },
        { 438, mask = 0xC, mode = 0x0B, next = true },
    },
    {
        { 10, xa = 8 },
        { 20, mask = 0x30, mode = 0x07 },
        { 140, mask = 0x30, mode = 0x0B, next = true },
    },
    {
        { 1, xa = 9 },
        { 11, mask = 0xC0, mode = 0x07 },
        { 322, mask = 0xC0, mode = 0x0B, picture = PICTURE_CLAIRE_B, init = true, next = true },
    },
    {
        { 128, xa = 14 },
        { 337, move = true, next = true },
    },
    {
        { 90, xa = 10 },
        { 100, mask = 0x300, mode = 0x07 },
        { 336, mask = 0x300, mode = 0x0B, next = true },
    },
    {
        { 1, xa = 11 },
        { 11, mask = 0xC00, mode = 0x07 },
        { 266, mask = 0xC00, mode = 0x0B, next = true },
    },
    {
        { 1, xa = 12 },
        { 11, mask = 0x3000, mode = 0x07 },
        { 441, mask = 0x3000, mode = 0x0B, picture = PICTURE_B2, init = true },
        { 451, mask = 0xC000, mode = 0x07 },
        { 738, mask = 0xC000, mode = 0x0B },
        { 768, next = true },
    },
    {
        { 1, xa = 13 },
        { 11, mask = 0x30000, mode = 0x07 },
        { 385, mask = 0x30000, mode = 0x0B },
        { 595, leave = true },
    },
}

-- `Prologue_mv`'s entries: each sequence's sprites, steps, the step its
-- last fade is in (and its count), the step that starts the films (which
-- a skip comes back into), the second film (`Init_movie_work`) and the
-- main music `Prologue_load` loads.
local SEQUENCES = {
    [0] = { sprites = SPRITES_A, count = 28, steps = STEPS_A, leave = 5, leave_count = 445, films = 6, film2 = 0xF, music = 0x2B },
    [2] = { sprites = SPRITES_LEON_B, count = 12, steps = STEPS_LEON_B, leave = 6, leave_count = 517, films = 7, film2 = 0x10, music = 0x2C },
    [3] = { sprites = SPRITES_CLAIRE_B, count = 18, steps = STEPS_CLAIRE_B, leave = 9, leave_count = 595, films = 10, film2 = 0x10, music = 0x2C },
}
SEQUENCES[1] = SEQUENCES[0]

-- The count the films' step starts the first film on.
local FILM_COUNT = 100
-- The count a skip comes back into its step with.
local SKIP_COUNT = 0x5A

local function u8(v)
    return v & 0xFF
end

-- `Prologue_mv`'s entry for a game of `fg_status` (`status`): for a new
-- game (`place` nil) bit 31 (Claire) and bit 30 (the second scenario), and
-- for a load the save's place, 0xF a first scenario's start and 0x10 a
-- second's, or nil for any other place, which has no prologue.
function M.sequence(status, place)
    local claire = (status >> 31) & 1
    if place == nil then
        return claire | ((status >> 29) & 2)
    elseif place == 0xF then
        return claire
    elseif place == 0x10 then
        return claire | 2
    end
    return nil
end

-- A prologue as `Prologue_load` leaves it for `Prologue_mv`'s entry `mv`
-- (`M.sequence`). `fades` is the title's: `set(a0, add)`, `done(n)`,
-- `off(n)`, `level(n, level)` and `add(n, add)`.
function M.new(fades, mv)
    local seq = SEQUENCES[mv]
    local p = {
        fades = fades,
        seq = seq,
        -- The task's steps (+8, +9, +10).
        task = { 0, 0, 0 },
        -- `Prologue_mv`'s entry: 0 Leon A, 1 Claire A (both
        -- `Prologue_mv00`), 2 Leon B, 3 Claire B, 4 a skip's wait.
        mv = mv,
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
        -- The film a step started this tick (`Init_movie_work`), for the
        -- title to play; `film_on` until it says the film is over
        -- (`M.film_over`), and whether a press ended it (`0x9814B5`).
        film = nil,
        film_on = false,
        film_skipped = false,
        done = false,
    }
    for i = 0, seq.count - 1 do
        local r = seq.sprites[i]
        p.sprites[i] = { mode = r[1], kind = r[2], depth = r[3], map = r[4], x = r[5], y = r[6], colour = 0 }
    end
    return p
end

-- `Prologue_set_mode(no, clear)`: the sprites in the mask take `no`, and
-- with `clear` the others go off.
local function set_mode(p, no, clear)
    for i = 0, p.seq.count - 1 do
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

-- `Init_movie_work(id)`: the sequence waits for the film.
local function film(p, id)
    p.film = id
    p.film_on = true
    p.film_skipped = false
end

-- The film a step started is over, `skipped` when a press ended it.
function M.film_over(p, skipped)
    p.film_on = false
    p.film_skipped = skipped and true or false
end

-- `Prologue_exit` (0x4CF730): once the fade is over, the task ends. Until
-- then the task sleeps in it, and nothing else of the tick runs.
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

-- One count of the sequence (`Prologue_mv00`, `Prologue_mv01` or
-- `Prologue_mv02`). Returns without counting on when a step starts the
-- next.
local function sequence(p)
    if p.step == 0 then
        p.fades.level(3, 0)
        p.music = "start"
        p.step = 1
        p.slot = 3
        p.count = 1
        return
    end
    local seq = p.seq
    local actions = seq.steps[p.step]
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
        if p.step == seq.leave and p.count > seq.leave_count and p.fades.done(p.slot) then
            p.skip_ok = 1
            p.mask = 0
            set_mode(p, 0, true)
            p.step = seq.films
            p.count = 1
            return
        end
    elseif p.step == seq.films then
        if p.count == FILM_COUNT then
            -- The scenario's opening film.
            film(p, 3)
            p.step = seq.films + 1
            p.count = 1
            return
        end
    elseif p.step == seq.films + 1 then
        if not p.film_on then
            -- The first film is over: the second follows it unless a
            -- press ended the first.
            p.step = seq.films + 3
            p.count = 1
            if p.film_skipped then
                p.r = 3
                p.mv = 4
            else
                film(p, seq.film2)
            end
            return
        end
    elseif p.step == seq.films + 3 then
        if not p.film_on then
            p.r = 3
            p.mv = 4
        end
    end
    p.count = (p.count + 1) & 0xFFFF
end

-- `Prologue_mv03` (0x4CF170), a skip's wait: the lines go off, and when
-- slot 0's fade is over the sequence comes back in at its films' step.
local function skipped(p)
    for i = 0, p.seq.count - 1 do
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
    for i = 0, p.seq.count - 1 do
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

-- One tick of the task. `keys` are this tick's presses. A film the tick
-- started is left in `p.film` for the caller to play.
function M.tick(p, keys)
    p.running = true
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
            p.step = p.seq.films
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
        return
    end
    if p.mv == 4 then
        skipped(p)
    else
        sequence(p)
    end
    animate(p)
    p.frames = p.frames + 1
end

-- Play what this tick asked of the sound. `player` is 1 for Claire.
function M.sound(api, p, player)
    if p.music == "start" then
        api:music(p.seq.music)
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

-- The values of a trace's `pro`, `prs` and `prz`, from the task's first
-- tick: the tick that starts it leaves no task for a trace to read.
function M.publish(api, p)
    local on = p and p.running and not p.done
    api:set("pro_on", on and 1 or 0)
    if not on then
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
    api:set("pro_sprites", p.seq.count)
    -- Two sprites to a value: the mode, the colour and y of each.
    local function word(sp)
        return (sp.mode << 24) | (sp.colour << 16) | (sp.y & 0xFFFF)
    end
    for i = 0, p.seq.count - 1, 2 do
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
        for i = p.seq.count - 1, 0, -1 do
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
    if p.step <= p.seq.films then
        api:fill_rect(ox, oy, 320, BAND, 0, 0, 0)
        api:fill_rect(ox, oy + 240 - BAND, 320, BAND, 0, 0, 0)
    end
    fade(0)
end

return M
