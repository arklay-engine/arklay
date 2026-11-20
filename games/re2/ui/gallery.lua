-- RE2's gallery, as the original runs it: the task the title starts from
-- GALLERY's page (`Gallery`, 0x4BF760), its menu, DATA GALLERY
-- (`GalleryMain`, 0x439A80), the pictures behind the menu's first row
-- (`GalleryIllustrations`, 0x438DA0), the movies behind its second
-- (`GalleryMovie`, 0x437F30) and CLAIRE's models (`Gallery3DArt`,
-- 0x4375B0): the screens of `GalleryLoop` (0x43AE60). The title's task
-- sleeps under it.
--
-- The gallery draws at 640 x 480 with its own art (`ui/gallery/` in the
-- pack): the menu's picture in five sheets, `title1` to `title5`, and its
-- rows' words, `moji`; the pictures, `rough01` to `rough60` and `illust01`
-- to `illust35`, with their panel, `left01` and `left02`, and its digits,
-- `gnum2`; the movies' words, `movie64`; the models' panel, `3dleft1` and
-- `3dleft2`, and its digits, `gnum`. The places below are the executable's,
-- in those pixels; the frame draws them at half size. With a 320 x 240
-- display the original has a second layout with smaller art (`top32_1`,
-- `top32_2`, `moji3224`, `left3224`, `movie32` and `3d3224`) and, in it
-- alone, a button that doubles a picture's size; neither is ported.
--
-- The state keeps the original's work (`pGallery`) under its own numbers,
-- so the oracle's `gal` and `galv` fields compare one for one.
--
-- The menu's rows: ROUGH SKETCH (LEON) or ILLUSTRATION (CLAIRE), MOVIE, 3D
-- MODEL (CLAIRE only) and EXIT. EXIT ends the gallery and wakes the title.
-- A button on another row fades the menu out and opens that row's screen:
-- the pictures, the movies, or CLAIRE's 3D models. The models' screen has
-- its panel and its count; the model itself isn't drawn.

local M = {}

-- `Gallery_Padd`'s bits: the directions, and any button.
local PAD_UP, PAD_RIGHT, PAD_DOWN, PAD_LEFT = 0x1000, 0x2000, 0x4000, 0x8000
local PAD_BUTTONS = 0x0FFF

-- `GallerySeOn`'s core-bank cues.
local SE_CURSOR, SE_DECIDE, SE_TURN = 4, 6, 10

-- The work's flags (`+0x1478`): the fade's black is drawn, the fade goes
-- out rather than in, a model is loaded, and the lit row's level is
-- falling.
local FADING, FADING_OUT, MODEL, FALLING = 0x01, 0x02, 0x10, 0x20

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

-- A panel's two sheets' places and sizes, and the sheets of the pictures'
-- panel and of the models': the upper, the lower, which has the lit keys
-- too, and the count's digits.
local PANEL = {
    { 0, 0, 154, 256 },
    { 0, 256, 154, 224 },
}
local PICTURE_SHEETS = { "left01", "left02", digits = "gnum2" }
local MODEL_SHEETS = { "3dleft1", "3dleft2", digits = "gnum" }

-- A panel's keys, lit while held, in the order the original asks for
-- them: the pad's bit, with the lit key's place and its corner in the
-- panel's lower sheet. Each is 100 x 25.
local KEYS = {
    { PAD_RIGHT, 18, 285, 154, 0 },
    { PAD_LEFT, 18, 321, 154, 25 },
    { PAD_UP, 18, 357, 154, 50 },
    { PAD_DOWN, 18, 393, 154, 75 },
}

-- The count's place, "01/ 60": the picture's number and the last one's,
-- with the digits' stroke between. A digit is 12 x 20, five to a row.
local NUMBER_X, LAST_X, COUNT_Y = 560, 600, 455
local DIGIT_W, DIGIT_H = 12, 20
local STROKE = { 35, 40 }

-- Where each picture's corner goes (`tbl_sketches`, 0x522F50, and
-- `tbl_illustrations`, 0x523130), in the order of their files.
local PLACES = {
    [false] = {
        stem = "rough",
        { 270, 13 }, { 162, 67 }, { 253, 18 }, { 253, 15 }, { 282, 12 }, { 264, 11 },
        { 234, 8 }, { 213, 10 }, { 226, 9 }, { 266, 14 }, { 177, 58 }, { 242, 11 },
        { 217, 4 }, { 239, 10 }, { 257, 13 }, { 243, 12 }, { 226, 9 }, { 220, 5 },
        { 285, 4 }, { 267, 6 }, { 233, 10 }, { 317, 53 }, { 316, 47 }, { 294, 39 },
        { 251, 9 }, { 289, 8 }, { 280, 34 }, { 276, 2 }, { 245, 28 }, { 195, 43 },
        { 155, 56 }, { 199, 4 }, { 162, 66 }, { 164, 60 }, { 221, 24 }, { 205, 7 },
        { 278, 16 }, { 297, 15 }, { 236, 16 }, { 249, 21 }, { 230, 17 }, { 271, 17 },
        { 239, 22 }, { 161, 85 }, { 207, 5 }, { 166, 107 }, { 201, 7 }, { 158, 59 },
        { 154, 62 }, { 160, 75 }, { 155, 62 }, { 154, 50 }, { 156, 62 }, { 154, 64 },
        { 155, 69 }, { 155, 67 }, { 219, 0 }, { 155, 69 }, { 154, 67 }, { 157, 69 },
    },
    [true] = {
        stem = "illust",
        { 183, 6 }, { 287, 2 }, { 202, 6 }, { 265, 10 }, { 296, 8 }, { 303, 6 },
        { 312, 55 }, { 286, 7 }, { 286, 10 }, { 209, 61 }, { 209, 61 }, { 212, 12 },
        { 212, 12 }, { 212, 12 }, { 249, 3 }, { 280, 28 }, { 210, 33 }, { 211, 58 },
        { 231, 48 }, { 154, 87 }, { 217, 25 }, { 211, 43 }, { 233, 3 }, { 173, 31 },
        { 236, 5 }, { 219, 5 }, { 199, 3 }, { 154, 59 }, { 154, 86 }, { 267, 7 },
        { 213, 5 }, { 202, 72 }, { 331, 9 }, { 218, 21 }, { 218, 11 },
    },
}

-- The movies' words, each with its place, its size and its corner in
-- `movie64`: the two arrows beside the number and the two beside the ways
-- out, MOVIE, the button and PLAY, and each character's ways up and down
-- (ROUGH SKETCH and TO HOME, ILLUSTRATION and 3D MODEL).
local MOVIE = {
    left = { 167, 230, 11, 19, 0, 20 },
    right = { 223, 230, 11, 19, 0, 39 },
    up = { 316, 219, 19, 11, 0, 58 },
    down = { 316, 250, 19, 11, 0, 69 },
    word = { 243, 233, 59, 14, 0, 80 },
    button = { 282, 398, 16, 16, 0, 165 },
    play = { 314, 400, 48, 13, 0, 151 },
    ways = {
        [false] = { { 350, 218, 139, 14, 0, 94 }, { 379, 248, 81, 14, 0, 108 } },
        [true] = { { 356, 218, 127, 14, 0, 122 }, { 370, 248, 90, 14, 0, 136 } },
    },
}

-- The number's two digits' places. A digit is 12 x 21 in the sheet, ten
-- in a row, and is drawn 20 high: its eleventh row is left out.
local MOVIE_DIGITS = { 182, 205 }
local MOVIE_DIGIT_Y = 230

-- The film of each of a character's sixteen movies (`gmovie_tbl0`,
-- 0x523248, and `gmovie_tbl1`, 0x523258), an id of the character's films.
local FILMS = {
    [false] = { 0, 3, 15, 16, 10, 5, 4, 6, 7, 11, 8, 9, 12, 17, 13, 14 },
    [true] = { 0, 3, 15, 16, 10, 5, 4, 11, 6, 7, 8, 9, 12, 17, 13, 14 },
}

-- config.ini's `GALLERY0` to `GALLERY4` with every picture, film and model
-- open, as a finished game leaves them.
local ALL_OPEN = { 0xFFFFFFFF, 0xFFFFFFF0, 0xFFFFFFFE, 0xFFFFFFFF, 0xFFFFFFFF }

-- The task as `Gallery`'s first step leaves it, on the tick the title
-- starts it: step 1, with the work it has just cleared. `open` is the five
-- words of what play has opened (`ALL_OPEN` without them). `clear` is the
-- colour the renderer clears the next frame to, which stays as the last
-- screen set it.
function M.new(claire, open)
    return {
        task = 1,
        screen = 0,
        step = 0,
        row = 0,
        pulse = 0,
        fade = 0,
        flags = 0,
        index = 0,
        tiles = 0,
        clear = 0,
        open = open or ALL_OPEN,
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

local load, picture

-- The menu's sheets, and the pictures' own.
local MENU_ART = { "title1", "title2", "title3", "title4", "title5", "moji" }
local VIEW_ART = { "gnum2", "left01", "left02" }
local MOVIE_ART = { "movie64" }
local MODEL_ART = { "gnum", "3dleft1", "3dleft2" }

-- The bit of the fifth word that opens each of the thirty-eight models
-- (kind 4's table, 0x524F8C): the first two share one, and six of the
-- zombies another.
local MODEL_BITS = {
    0, 0, 3, 4, 5, 1, 2, 1, 1, 1, 1, 6, 7, 1, 8, 9, 10, 11, 12,
    13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
}

-- `GalleryMain`: `false` once EXIT is taken, or when its art can't load.
local function main(api, g, edge, cue)
    if g.step == 0 then
        -- The art is loaded and the menu fades in from the next tick. A
        -- sheet that can't be made a texture ends the gallery there.
        if not load(api, MENU_ART) then
            return false
        end
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
        -- draws nothing.
        g.screen = menu.leave[at]
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

-- Whether play has opened picture `index` (`FUN_00442ed0`, with the bit
-- tables at 0x524F0C and 0x524F48): LEON's sketches have a bit each from
-- the first word's top, and CLAIRE's illustrations from the third's, where
-- her first five share one.
local function opened(g, index)
    local bit, word = index, 1
    if g.claire then
        bit, word = index < 5 and 0 or index - 4, 3
    end
    return (g.open[word + (bit >> 5)] or 0) & (0x80000000 >> (bit & 31)) ~= 0
end

-- `GalleryIllustrations`: the pictures, one at a time. `false` when the
-- panel's art can't load. Here the work's row (`+8`) is 1 while a picture
-- waits to be loaded.
local function pictures(api, g, edge, held, cue)
    local count = #PLACES[g.claire]
    if g.step == 0 then
        -- The panel's art is loaded, and the first picture is the first
        -- file, opened or not. Nothing is drawn.
        if not load(api, VIEW_ART) then
            return false
        end
        g.tiles = 0
        g.index = 0
        g.step = 1
        g.row = 1
        g.flags = (g.flags | FADING) & ~FADING_OUT
        g.fade = 0xFF
        disp(g)
        return true
    end
    if g.step == 1 then
        if edge & PAD_UP ~= 0 then
            -- To the menu.
            cue(SE_DECIDE)
            g.step = 2
            g.flags = g.flags | FADING | FADING_OUT
            g.fade = 0xFF
        elseif edge & PAD_DOWN ~= 0 then
            -- To the movies.
            cue(SE_DECIDE)
            g.step = 4
            g.flags = g.flags | FADING | FADING_OUT
            g.fade = 0xFF
        elseif edge & (PAD_RIGHT | PAD_LEFT) ~= 0 then
            -- The next opened picture that way, round the ends: the same
            -- one again when no other is.
            local way = edge & PAD_RIGHT ~= 0 and 1 or -1
            local from = g.index
            cue(SE_TURN)
            repeat
                g.index = (g.index + way) % count
            until opened(g, g.index) or g.index == from
            g.row = 1
            g.tiles = 0
        end
    elseif g.fade < 1 then
        -- The menu, or the movies, from the next tick; this one draws
        -- nothing.
        g.screen = g.step == 4 and 2 or 0
        g.step = 0
        g.row = 0
        g.tiles = 0
        return true
    end
    -- The frame is cleared to the colour the tick before asked for: white,
    -- and black once a fade out's level is under 0x20. So the screen's
    -- first frame is still cleared to black, and a fade out's last three.
    -- The tick a picture is loaded on doesn't draw it.
    local view = {
        index = g.index,
        last = count - 1,
        clear = g.clear,
        picture = g.row == 0 and g.tiles > 0,
        sheets = PICTURE_SHEETS,
    }
    local out = (g.step == 2 or g.step == 4) and g.fade < 0x20
    g.clear = out and 0 or 0xFF
    if g.row ~= 0 then
        g.tiles = picture(api, g)
        g.row = 0
    end
    for _, key in ipairs(KEYS) do
        if held & key[1] ~= 0 then
            view.key = key
            break
        end
    end
    g.view = view
    disp(g)
    return true
end

-- Whether play has opened a character's movie `index`: a bit each from
-- the fourth word's top, LEON's sixteen and then CLAIRE's (`FUN_00442ed0`
-- with kind 3, whose table at 0x524F6C is each index's own number).
local function film_opened(g, index)
    local bit = (g.claire and 16 or 0) + index
    return (g.open[4] or 0) & (0x80000000 >> bit) ~= 0
end

-- `GalleryMovie`: the movies, by number. `false` when its art can't load.
local function movies(api, g, edge, held, cue)
    if g.step == 0 then
        -- The art is loaded, and the first movie is number 0, opened or
        -- not. Nothing is drawn.
        if not load(api, MOVIE_ART) then
            return false
        end
        g.tiles = 0
        g.index = 0
        g.step = 1
        g.row = 0
        g.flags = (g.flags | FADING) & ~FADING_OUT
        g.fade = 0xFF
        disp(g)
        return true
    end
    if g.step == 1 then
        if edge & PAD_BUTTONS ~= 0 then
            -- `Init_movie_work`: the movie's film, which the title plays.
            cue(SE_DECIDE)
            g.film = FILMS[g.claire][g.index + 1]
        elseif edge & PAD_UP ~= 0 then
            -- To the pictures.
            cue(SE_DECIDE)
            g.step = 3
            g.flags = g.flags | FADING | FADING_OUT
            g.fade = 0xFF
        elseif edge & PAD_DOWN ~= 0 then
            -- To the menu, or CLAIRE's models.
            cue(SE_DECIDE)
            g.step = 2
            g.flags = g.flags | FADING | FADING_OUT
            g.fade = 0xFF
        elseif edge & (PAD_RIGHT | PAD_LEFT) ~= 0 then
            -- The next opened movie that way, round the ends.
            local way = edge & PAD_RIGHT ~= 0 and 1 or -1
            local from = g.index
            cue(SE_CURSOR)
            repeat
                g.index = (g.index + way) % 16
            until film_opened(g, g.index) or g.index == from
            g.tiles = 0
        end
    elseif g.fade < 1 then
        -- The pictures (step 3), or the menu for LEON and the models for
        -- CLAIRE (step 2), from the next tick; this one draws nothing.
        g.screen = g.step == 3 and 1 or (g.claire and 3 or 0)
        g.step = 0
        g.row = 0
        g.tiles = 0
        return true
    end
    -- The frame is cleared to black. One thing is lit: the arrow of the
    -- first held of right, left, up and down, or with none of them the
    -- button on the tick one is pressed.
    g.clear = 0
    local list = { index = g.index }
    if held & PAD_RIGHT ~= 0 then
        list.lit = "right"
    elseif held & PAD_LEFT ~= 0 then
        list.lit = "left"
    elseif held & PAD_UP ~= 0 then
        list.lit = "up"
    elseif held & PAD_DOWN ~= 0 then
        list.lit = "down"
    elseif edge & PAD_BUTTONS ~= 0 then
        list.lit = "button"
    end
    g.list = list
    disp(g)
    return true
end

-- Whether play has opened model `index` (`FUN_00442ed0` with kind 4).
local function model_opened(g, index)
    return (g.open[5] or 0) & (0x80000000 >> MODEL_BITS[index + 1]) ~= 0
end

-- `Gallery3DArt`: CLAIRE's models, one at a time beside a panel. `false`
-- when its art can't load. Here the work's row (`+8`) is 1 while a model
-- waits to be loaded.
local function models(api, g, edge, held, cue)
    if g.step == 0 then
        -- The panel's art is loaded, and the first model is number 0,
        -- opened or not. Nothing is drawn.
        if not load(api, MODEL_ART) then
            return false
        end
        g.tiles = 0
        g.index = 0
        g.step = 1
        g.row = 1
        g.flags = (g.flags | FADING) & ~(FADING_OUT | MODEL)
        g.fade = 0xFF
        disp(g)
        return true
    end
    if g.step == 1 then
        if edge & PAD_UP ~= 0 then
            -- To the movies.
            cue(SE_DECIDE)
            g.step = 2
            g.flags = g.flags | FADING | FADING_OUT
            g.fade = 0xFF
        elseif edge & PAD_DOWN ~= 0 then
            -- To the menu.
            cue(SE_DECIDE)
            g.step = 4
            g.flags = g.flags | FADING | FADING_OUT
            g.fade = 0xFF
        elseif edge & (PAD_RIGHT | PAD_LEFT) ~= 0 then
            -- The next opened model that way, round the ends. The buttons
            -- do nothing.
            local way = edge & PAD_RIGHT ~= 0 and 1 or -1
            local from = g.index
            cue(SE_CURSOR)
            repeat
                g.index = (g.index + way) % #MODEL_BITS
            until model_opened(g, g.index) or g.index == from
            g.row = 1
            g.tiles = 0
        end
    elseif g.fade < 1 then
        -- The movies (step 2) or the menu (step 4) from the next tick;
        -- this one draws nothing.
        g.screen = g.step == 2 and 2 or 0
        g.step = 0
        g.row = 0
        g.tiles = 0
        g.flags = g.flags & ~MODEL
        return true
    end
    -- The frame is cleared to black. The tick a model is loaded on
    -- (`GalleryLoadModel`: the screen's first, and a turn's) doesn't show
    -- it.
    local view = {
        index = g.index,
        last = #MODEL_BITS - 1,
        clear = g.clear,
        model = g.row == 0 and g.flags & MODEL ~= 0,
        sheets = MODEL_SHEETS,
    }
    g.clear = 0
    if g.row ~= 0 then
        g.flags = g.flags | MODEL
        g.row = 0
    end
    for _, key in ipairs(KEYS) do
        if held & key[1] ~= 0 then
            view.key = key
            break
        end
    end
    g.view = view
    disp(g)
    return true
end

-- `Init_movie_work` opens the film it plays, the title's and the
-- gallery's as play's: in `open`, the title's words of what is opened
-- (none when everything is), the bit of film `id`'s place in LEON's table,
-- whoever's film it is, in the half of the character it plays for.
-- CLAIRE's number 7 so opens her number 9, and that her number 8.
function M.open_film(open, claire, id)
    if not open then
        return
    end
    for index, film in ipairs(FILMS[false]) do
        if film == id then
            open[4] = (open[4] or 0) | (0x80000000 >> ((claire and 16 or 0) + index - 1))
        end
    end
end

-- One tick of the task. `edge` is the pad's pressed bits and `held` its
-- held ones, and `cue` plays a core-bank cue. The task is over (`g.task`
-- 0) the tick after EXIT. `g.film` is the film a movie asked for on this
-- tick, for the title to play.
function M.tick(api, g, edge, held, cue)
    g.words = nil
    g.view = nil
    g.list = nil
    g.film = nil
    g.black = nil
    g.ticked = true
    if g.task == 2 then
        -- `GalleryClose`, and the title's task is signalled.
        g.task = 0
        return
    end
    local alive
    if g.screen == 1 then
        alive = pictures(api, g, edge, held, cue)
    elseif g.screen == 2 then
        alive = movies(api, g, edge, held, cue)
    elseif g.screen == 3 then
        alive = models(api, g, edge, held, cue)
    else
        alive = main(api, g, edge, cue)
    end
    if not alive then
        g.task = 2
    end
end

-- The trace's `gal` and `galv`.
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
        api:set("gal_index", g.index)
        api:set("gal_tiles", g.tiles)
    end
end

local art = {}

function M.reset()
    art = {}
end

-- A sheet of the pack's, once: nil when the pack lacks it.
local function sheet_of(api, stem)
    local sheet = art[stem]
    if sheet == nil then
        local page = api:indexed_page("ui/gallery/" .. stem)
        sheet = page and { page = page, palette = api:page_palette(page, 0) or {}, luts = {} } or false
        art[stem] = sheet
    end
    return sheet or nil
end

-- A screen's sheets: `false` when the pack lacks one.
load = function(api, stems)
    for _, stem in ipairs(stems) do
        if not sheet_of(api, stem) then
            return false
        end
    end
    return true
end

-- The file of a character's picture `index`.
local function picture_stem(claire, index)
    local n = index + 1
    return PLACES[claire].stem .. (n < 10 and "0" or "") .. n
end

-- `GalleryLoadIllust`: the picture is loaded, and the count of the 256 x
-- 256 textures the original cuts it into is the work's; none for a file
-- the pack lacks, which shows no picture.
picture = function(api, g)
    local sheet = sheet_of(api, picture_stem(g.claire, g.index))
    if not sheet then
        return 0
    end
    local w, h = api:art_size(sheet.page)
    return ((w + 255) // 256) * ((h + 255) // 256)
end

-- A sheet's 5-bit level `v` as the original shows it, 8v + 7, and its 0
-- as 0.
local function expand(c)
    local v = (c * 31 + 127) // 255
    return v == 0 and 0 or 8 * v + 7
end

-- A sheet's colours at `level` (0x80 is its own) under the fade's `black`
-- (an alpha, 0 for none), keyed by texel index. Black is see-through unless
-- the sheet is `opaque`. The fade is in the colours, so each of the
-- original's pixels fades before the frame halves them, as its black
-- rectangle has it. A sheet keeps the lookups of one fade level.
local function lut_for(sheet, level, opaque, black)
    if sheet.black ~= black then
        sheet.black = black
        sheet.luts = {}
    end
    local lut = sheet.luts[level]
    if lut then
        return lut
    end
    lut = {}
    local function at(c)
        return (((expand(c) * level + 64) >> 7) * (255 - black) + 127) // 255
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

-- One side of a piece at half size: the runs of its texels from `from`
-- that share pixels, as `{ pixel, texel, texels, weight }`. A piece that
-- starts or ends halfway through a pixel has a run of the one texel there
-- at half weight.
local function runs(at, size, from)
    local out = {}
    local head = at & 1
    local body = (size - head) & ~1
    if head == 1 then
        out[#out + 1] = { at // 2, from, 1, 1 }
    end
    if body > 0 then
        out[#out + 1] = { (at + head) // 2, from + head, body, 2 }
    end
    if head + body < size then
        out[#out + 1] = { (at + size - 1) // 2, from + size - 1, 1, 1 }
    end
    return out
end

-- The weight of a part by its two runs' (4 is a whole pixel).
local WEIGHT = { 64, 128, [4] = 255 }

-- A piece of a sheet at half size, from its place and size at 640 x 480,
-- with the picture's corner at `ox, oy`. Each pixel is the mean of the
-- piece's texels in it; where the piece covers half a pixel, or a quarter
-- at a corner, its texels go over the frame at that weight.
local function half(api, sheet, ox, oy, x, y, w, h, u, v, level, opaque, black)
    local lut = lut_for(sheet, level, opaque, black)
    local rows = runs(y, h, v)
    for _, col in ipairs(runs(x, w, u)) do
        for _, row in ipairs(rows) do
            local options = { dw = (col[3] + 1) // 2, dh = (row[3] + 1) // 2, blend = WEIGHT[col[4] * row[4]] }
            api:page_lut(sheet.page, col[2], row[2], col[3], row[3], ox + col[1], oy + row[1], lut, false, options)
        end
    end
end

-- The pictures' frame, or the models': the cleared frame, the picture,
-- the panel with the held key lit, and the count. A model isn't drawn.
local function draw_view(api, g, ox, oy, black)
    local view = g.view
    local low, digits = art[view.sheets[2]], art[view.sheets.digits]
    local clear = (view.clear * (255 - black) + 127) // 255
    api:fill_rect(ox, oy, 320, 240, clear, clear, clear)
    local sheet = view.picture and art[picture_stem(g.claire, view.index)]
    if sheet then
        local place = PLACES[g.claire][view.index + 1]
        local w, h = api:art_size(sheet.page)
        half(api, sheet, ox, oy, place[1], place[2], w, h, 0, 0, 0x80, false, black)
    end
    for k, part in ipairs(PANEL) do
        half(api, art[view.sheets[k]], ox, oy, part[1], part[2], part[3], part[4], 0, 0, 0x80, false, black)
    end
    local key = view.key
    if key then
        -- A key starts halfway down a pixel, whose upper half is the
        -- panel's row above it. That row is drawn alone first, so that the
        -- key's first goes over it and not over the panel's two rows
        -- already halved. The keys have no see-through texel.
        local above = key[3] - 1
        local lut = lut_for(low, 0x80, false, black)
        local options = { dw = 50, dh = 1, blend = 255 }
        api:page_lut(low.page, key[2], above - PANEL[2][2], 100, 1, ox + key[2] // 2, oy + above // 2, lut, false, options)
        half(api, low, ox, oy, key[2], key[3], 100, 25, key[4], key[5], 0x80, false, black)
    end
    -- `GalleryDrawPrim`: a number from 1, as two digits.
    local function digit(x, d)
        local u, v = (d % 5) * DIGIT_W, (d // 5) * DIGIT_H
        half(api, digits, ox, oy, x, COUNT_Y, DIGIT_W, DIGIT_H, u, v, 0x80, false, black)
    end
    local function number(x, n)
        digit(x, (n + 1) // 10)
        digit(x + DIGIT_W, (n + 1) % 10)
    end
    number(NUMBER_X, view.index)
    half(api, digits, ox, oy, NUMBER_X + 2 * DIGIT_W, COUNT_Y, DIGIT_W, DIGIT_H, STROKE[1], STROKE[2], 0x80, false, black)
    number(LAST_X, view.last)
end

-- The movies' frame: the words dim or at their own level over black, with
-- the lit one at its own, and the movie's number from 00.
local function draw_list(api, g, ox, oy, black)
    local list = g.list
    local sheet = art.movie64
    local function word(place, level, x)
        half(api, sheet, ox, oy, x or place[1], place[2], place[3], place[4], place[5], place[6], level, false, black)
    end
    api:fill_rect(ox, oy, 320, 240, 0, 0, 0)
    for _, name in ipairs({ "left", "right", "up", "down", "button" }) do
        word(MOVIE[name], list.lit == name and 0x80 or DIM)
    end
    for _, way in ipairs(MOVIE.ways[g.claire]) do
        word(way, 0x80)
    end
    word(MOVIE.word, 0x80)
    word(MOVIE.play, 0x80)
    for k, d in ipairs({ list.index // 10, list.index % 10 }) do
        word({ 0, MOVIE_DIGIT_Y, 12, 10, d * 12, 0 }, 0x80, MOVIE_DIGITS[k])
        word({ 0, MOVIE_DIGIT_Y + 10, 12, 10, d * 12, 11 }, 0x80, MOVIE_DIGITS[k])
    end
end

-- The frame the last tick left, with the 320 x 240 picture's corner at
-- `ox, oy`: nothing on a tick that drew none, else the menu's picture and
-- rows, the pictures' screen or the movies', under the fade's black.
function M.draw(api, g, ox, oy)
    local black = g.black or 0
    if g.view then
        draw_view(api, g, ox, oy, black)
    end
    if g.list then
        draw_list(api, g, ox, oy, black)
    end
    if not g.words then
        return
    end
    for _, sheet in ipairs(PICTURE) do
        half(api, art[sheet[1]], ox, oy, sheet[2], sheet[3], sheet[4], sheet[5], sheet[6], 0, 0x80, true, black)
    end
    for k, word in ipairs(MENUS[g.claire].words) do
        local level = g.words[k]
        half(api, art.moji, ox, oy, word[2], word[3], word[4], word[5], word[6], word[7], level, false, black)
    end
end

return M
