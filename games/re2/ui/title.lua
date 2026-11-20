-- RE2's title screen, as the original runs it: `Title` (0x5035B0) and the
-- steps it calls, `Title_init` (0x5038B0) and `Title_main` (0x503A20) with the
-- menu (`Title_sel_menu`, 0x504060), the words (`Moji_*`) and the logo
-- (`move_and_display_title_image`, 0x5065C0). Each tick runs what the
-- original's task 0 runs in one: the pad's repeat (`Pad_set`), the step, the
-- draw, and then the fade (`System_trans`).
--
-- The art is the original's (`ui/title/` in the pack): the background
-- `Title_bg.adt` and the three TIMs of `tmoji.bin`, uploaded as textures 0x12
-- (the menu's words), 0x13 and 0x14 (the logo's pieces). The words' places,
-- sizes and sheet rects are the executable's table at 0x5407D0.
--
-- The state keeps the executable's variables under their own numbers, so the
-- oracle's `ti`, `tlogo` and `tfade` fields compare one for one; the engine
-- reads them back with `api:set` (`publish` below).
--
-- What each row leads to: LOAD GAME takes the confirm's white flash and fade
-- and opens the load screen; ORIGINAL MODE and ARRANGE MODE lead to the
-- new-game choice (`ui/char_select.lua`); SPECIAL slides up to the extras
-- config.ini's `SPECIAL` opens (`args.special`, 15 by default), where 4TH
-- and TO-FU fade to their own title (`title_mv_state` 3 and 4, with
-- `title_o.adt`, `title_o2.adt` and the logos `spy_logo.tim` and
-- `tofulogo.tim` as texture 0x15) whose START isn't ported past its fade
-- (the title starts again) and whose EXIT goes back to `Title_init`,
-- GALLERY opens its character's choice (page 7), whose confirm fades to
-- the gallery (`ui/gallery.lua`), a task the title sleeps under until its
-- EXIT, with config.ini's `GALLERY0` to `GALLERY4` (`args.gallery0` to
-- `args.gallery4`, everything opened without them), and EX.BATTLE's
-- confirm does nothing; OPTION fades out into the
-- option screen, which runs here as a task of its own ("the option screen"
-- below) and gives the title back at `Title_init`. An idle title fades into
-- the attract demo after 0x708 ticks.
--
-- From the game's boot (`M.on_boot`) the title starts 189 ticks before
-- `Title_init`, with the warning's picture and the title's film ("the
-- boot" below).
--
-- args.folder_saves: a registered save screen lists the whole save folder.

local CrText = require("ui/cr_text")
local Prologue = require("ui/prologue")
local Gallery = require("ui/gallery")

local M = {}

-- Raw pad bits (`Pad_set`'s left column) and the mapped keys the title reads.
local PAD_UP, PAD_DOWN, PAD_LEFT, PAD_RIGHT = 0x1000, 0x4000, 0x8000, 0x2000
local PAD_START = 0x0800
local PAD_CONFIRM, PAD_CANCEL = 0x0080, 0x0040
local KEY_CONFIRM, KEY_CANCEL = 0x1000, 0x2000

-- `Snd_se_on`'s core-bank cues: 0x4000000 and 0x4010000 (the confirm's two),
-- 0x4040000 the cursor, 0x4050000 the cancel and 0x4060000 the decide.
local SE_CONFIRM_A, SE_CONFIRM_B, SE_CURSOR, SE_CANCEL, SE_DECIDE = 0, 1, 4, 5, 6

-- The words' table (0x5407D0): x, y, w, h, u, v and the blend, per entry.
local MOJI = {
    [0] = { 81, 123, 160, 15, 48, 29, 1 },
    { 32, 139, 256, 10, 0, 18, 1 },
    { 32, 154, 256, 16, 0, 207, 1 },
    { 32, 171, 170, 13, 0, 97, 1 },
    { 112, 187, 96, 16, 80, 44, 1 },
    { 40, 151, 128, 13, 0, 110, 1 },
    { 168, 151, 128, 13, 128, 110, 1 },
    { 32, 173, 256, 13, 0, 123, 1 },
    { 40, 173, 128, 13, 0, 138, 1 },
    { 40, 151, 128, 28, 0, 164, 1 },
    { 168, 151, 128, 16, 128, 164, 1 },
    { 32, 179, 144, 15, 0, 192, 1 },
    { 176, 179, 112, 15, 144, 192, 1 },
    { 32, 179, 86, 12, 170, 97, 1 },
    { 32, 172, 256, 13, 0, 151, 1 },
    { 32, 192, 256, 13, 0, 123, 1 },
    { 32, 211, 256, 13, 0, 62, 0 },
    { 100, 40, 75, 40, 0, 0, 1 },
    { 25, 80, 128, 56, 0, 40, 1 },
    { 153, 80, 128, 56, 0, 96, 1 },
    { 281, 80, 12, 56, 0, 152, 1 },
    { 150, 20, 128, 200, 128, 0, 1 },
    { 22, 76, 128, 80, 0, 0, 2 },
    { 150, 76, 128, 80, 0, 80, 2 },
    { 278, 76, 23, 80, 0, 160, 2 },
    { 82, 150, 160, 15, 48, 29, 1 },
    { 32, 168, 256, 13, 0, 151, 1 },
    { 32, 184, 256, 16, 0, 221, 1 },
    { 32, 203, 256, 13, 0, 123, 1 },
    { 0, 8, 256, 88, 0, 0, 1 },
    { 184, 8, 256, 88, 0, 88, 1 },
    { 0, 8, 256, 88, 0, 0, 1 },
    { 184, 8, 256, 88, 0, 88, 1 },
    { 96, 172, 32, 16, 64, 239, 1 },
    { 144, 172, 32, 16, 112, 239, 1 },
    { 192, 172, 32, 16, 160, 239, 1 },
    { 32, 195, 256, 13, 0, 123, 1 },
    { 104, 151, 48, 12, 0, 32, 1 },
    { 168, 151, 56, 12, 0, 44, 1 },
    { 123, 132, 74, 12, 170, 96, 1 },
}
local LICENSE = 16
local LOGO = 22

-- The menu's rows (`Title_sel_menu`'s cursor): LOAD GAME, ORIGINAL MODE,
-- ARRANGE MODE, SPECIAL and OPTION.
local ROW_LOAD, ROW_ORIGINAL, ROW_ARRANGE, ROW_SPECIAL, ROW_OPTION = 0, 1, 2, 3, 4

-- Pages of the menu (`0x691B82`): 0 the rows, 1 to 7 the choices below them;
-- 4 is the confirm's way out.
local MENU_ROWS, MENU_OUT, MENU_GALLERY = 0, 4, 7

-- `fg_system`'s bits the title reads or sets.
local FG_REPEAT = 0x80000000
local FG_ARRANGE = 0x40
local FG_FROM_LOGO = 0x2
local FG_EASY = 0x20
local FG_NEW_GAME = 0x20000
-- Up while the load screen has loaded nothing (`Title_main_load`).
local FG_NO_LOAD = 0x400

local s
local pages

local function u8(v)
    return v & 0xFF
end

local function i16(v)
    v = v & 0xFFFF
    return v >= 0x8000 and v - 0x10000 or v
end

local function new_state()
    return {
        -- Task 0's init step (+8) and sub-step (+9).
        init = 0,
        sub = 0,
        mv = 0,
        disp = 0,
        menu = 0,
        step = 0,
        sel = 0,
        cur = 0,
        pulse = 0,
        pulse_step = 1,
        slide = 0,
        dim = 0,
        b8c = { [0] = 0, 0, 0 },
        slide_flag = 0,
        -- The new-game choice: the character (`fg_status` bit 31), EASY
        -- (`0x691B8E`) and what the cancel puts back (`0x689F1C`), HARD,
        -- and the rows' slide flags (`Flg`, `0x689F18`).
        claire = false,
        easy = 0,
        saved_easy = 0,
        hard_mode = 0,
        char_flag = 0,
        diff_flag = 0,
        -- `SuperHardSwitch`: this install's config has it, so HARD shows.
        super_hard = true,
        -- SPECIAL's last column, for the up key from EXIT (0x5409E8).
        special_column = 2,
        demo = 0,
        kido = 0,
        add = 0,
        flash = 0,
        logo = 0,
        logo_count = 0,
        game_mode = 0,
        fg_system = 0,
        censorship_off = 0,
        -- `Fade_set`'s four slots (0x98F07C, 0x4C bytes each): the level,
        -- its step and the blend. Slot 3 is drawn behind the sprites.
        fades = {
            [0] = { level = -1, add = 0, mode = 0 },
            { level = -1, add = 0, mode = 0 },
            { level = -1, add = 0, mode = 0 },
            { level = -1, add = 0, mode = 0 },
        },
        -- The fades the frame draws: `System_trans` takes each level before
        -- this tick's step.
        drawn_fades = {},
        -- config.ini's `SPECIAL` (`Special`, 0x524EB8): which extras show.
        special = 15,
        -- Task 0's second sub-step (+0xA), the 4th Survivor's title's.
        sub2 = 0,
        -- `fg_status`'s bits the extras set (flag 1:6, `0x02000000`).
        fg_status = 0,
        -- The frame limiter's choice (`0x98F07A`): 0 the title's, 2 the
        -- game's.
        limiter = 0,
        -- `Pad_set`'s state; `raw` is the held word of the tick before, once
        -- there is one.
        rep_mask = 0,
        rep_delay = 0,
        rep_count = 1,
        -- The words' work (`Moji_set_work`): per entry its colour and y.
        work = {},
        -- What the frame draws, in the original's order.
        sprites = {},
        sleeping = false,
        exit = nil,
        any_saves = false,
    }
end

-- ---------------------------------------------------------------- the pad

-- `Pad_set` for the keys the title reads: the edges and the repeat flag.
local function pad_set(input)
    local raw = 0
    local held = input.held or 0
    if held & 0x10 ~= 0 then
        raw = raw | PAD_UP
    end
    if held & 0x20 ~= 0 then
        raw = raw | PAD_DOWN
    end
    if held & 0x40 ~= 0 then
        raw = raw | PAD_LEFT
    end
    if held & 0x80 ~= 0 then
        raw = raw | PAD_RIGHT
    end
    -- The decide key's level, which only the option screen reads.
    s.held_confirm = held & 1 ~= 0
    if input.start then
        raw = raw | PAD_START
    end
    -- The original's pad has run since boot, so a direction held as the
    -- title opens is no edge here.
    s.edge = ((s.raw or raw) ~ raw) & raw
    if input.up then
        s.edge = s.edge | PAD_UP
    end
    if input.down then
        s.edge = s.edge | PAD_DOWN
    end
    if input.left then
        s.edge = s.edge | PAD_LEFT
    end
    if input.right then
        s.edge = s.edge | PAD_RIGHT
    end
    -- The buttons' raw bits, for the idle count's reset.
    if input.confirm then
        s.edge = s.edge | PAD_CONFIRM
    end
    if input.cancel then
        s.edge = s.edge | PAD_CANCEL
    end
    s.raw = raw
    s.key_trg = 0
    if input.confirm then
        s.key_trg = s.key_trg | KEY_CONFIRM
    end
    if input.cancel then
        s.key_trg = s.key_trg | KEY_CANCEL
    end
    s.any = s.edge ~= 0 or s.key_trg ~= 0
    if s.edge & s.rep_mask ~= 0 then
        s.rep_count = s.rep_delay & 0xFF
        s.fg_system = s.fg_system | FG_REPEAT
    elseif s.rep_count == 0 then
        s.rep_count = (s.rep_delay >> 8) & 0xFF
        s.fg_system = s.fg_system | FG_REPEAT
    else
        if raw & s.rep_mask ~= 0 then
            s.rep_count = s.rep_count - 1
        end
        s.fg_system = s.fg_system & ~FG_REPEAT
    end
end

local function repeating()
    return s.fg_system & FG_REPEAT ~= 0
end

local function confirm()
    return s.key_trg & KEY_CONFIRM ~= 0 or s.edge & PAD_START ~= 0
end

-- ---------------------------------------------------------------- the fade

-- `Fade_set(a0, add)`: the slot is `a0`'s low byte, the blend its high one.
local function fade_set(a0, add)
    local fade = s.fades[a0 & 0xFF]
    fade.mode = (a0 >> 8) & 3
    fade.add = i16(add)
    fade.level = add < 0 and i16(add + 0x8000) or 0
end

-- `Fade_adjust(n, level, ...)`: slot `n` (0 when not given) at `level`.
local function fade_adjust(level, n)
    s.fades[n or 0].level = i16(level)
end

-- `Fade_status(n)`: the slot is done (its level negative).
local function fade_done(n)
    return s.fades[n or 0].level < 0
end

local function fade_off(n)
    s.fades[n].level = -1
end

-- The fades as the prologue's task uses them (`ui/prologue.lua`).
local PROLOGUE_FADES = {
    set = fade_set,
    done = fade_done,
    off = fade_off,
    level = function(n, level)
        fade_adjust(level, n)
    end,
    add = function(n, add)
        s.fades[n].add = add
    end,
}

-- `Init_global` (0x4C4D70) as the title sees it: the four fades off,
-- `fg_status` cleared and `fg_system` down to 0x1000899.
local function init_global()
    s.fg_status = 0
    s.fg_system = s.fg_system & 0x1000899
    for n = 0, 3 do
        fade_off(n)
    end
end

-- ---------------------------------------------------------------- the words

local function moji_set_work()
    for i = 0, #MOJI do
        local m = MOJI[i]
        s.work[i] = { r = 0x80, x = m[1], y = m[2] }
    end
    -- To-fu's title moves its name (word 21) by (+16, -4).
    if s.fg_system & 1 ~= 0 then
        s.work[21].x = s.work[21].x + 0x10
        s.work[21].y = s.work[21].y - 4
    end
end

local function add_sprt(entry, colour, y, blend)
    local m = MOJI[entry]
    s.sprites[#s.sprites + 1] = {
        page = 0,
        u = m[5],
        v = m[6],
        w = m[3],
        h = m[4],
        x = m[1],
        y = y,
        colour = colour,
        blend = blend,
    }
end

-- A word as the original draws it: once with its blend, and again a line
-- lower with blend 0.
local function draw_word(entry, colour, y)
    add_sprt(entry, colour, y, MOJI[entry][7])
    add_sprt(entry, colour, y + 1, 0)
end

-- A word at a place of its own (the choices' rows).
local function draw_word_at(entry, colour, x, y)
    local m = MOJI[entry]
    for k, blend in ipairs({ m[7], 0 }) do
        s.sprites[#s.sprites + 1] = {
            page = 0, u = m[5], v = m[6], w = m[3], h = m[4],
            x = x, y = y + k - 1, colour = colour, blend = blend,
        }
    end
end

-- `Moji_License` (0x506580): the copyright line, opaque (it clears the
-- sprite's code, and with it the blend).
local function moji_license()
    add_sprt(LICENSE, 0x60, MOJI[LICENSE][2], nil)
end

-- `Moji_SelectMode` (0x505D40): the five rows, `mode` the lit one, `no` the
-- one that slides by `dis` while the others fade.
local function moji_select_mode(mode, no, dis, flg)
    local n = flg == 0 and 8 - s.slide or s.slide
    for row = 0, 4 do
        local colour = 0x20
        local y = MOJI[row][2]
        if s.slide ~= 0 then
            if row == no then
                local k = n * n * n * n
                if flg == 0 then
                    y = ((k * dis) >> 12) - dis + y
                else
                    y = y - ((k * dis) >> 12)
                end
            else
                local d = 0x80 - s.dim
                colour = u8(0x20 - ((d + ((d >> 31) & 3)) >> 2))
            end
        end
        if row == mode and s.slide ~= 8 then
            colour = u8(s.pulse + 0x20)
        end
        if colour ~= 0 then
            draw_word(row, colour, y)
        end
    end
end

local draw_difficulty

-- `Moji_Sel` (0x504000): the rows, or once a new game is chosen the
-- difficulty's.
local function moji_sel()
    moji_license()
    if s.sel ~= 1 then
        moji_select_mode(s.cur, s.cur, 0, 0)
        return
    end
    draw_difficulty(s.cur)
    if s.game_mode == 0 then
        moji_select_mode(1, 1, 6, 0)
    else
        moji_select_mode(2, 2, 0x16, 0)
    end
end

local function pulse()
    s.pulse = u8(s.pulse + s.pulse_step)
    if s.pulse == 0 or s.pulse > 0x30 then
        s.pulse_step = -s.pulse_step
    end
end

-- ---------------------------------------------------------------- the logo

-- `move_and_display_title_image` (0x5065C0).
local function title_image()
    if s.flash ~= 0 then
        s.flash = s.flash - 2
    end
    if s.logo_count >= 0 then
        s.logo_count = s.logo_count - 1
        local n = s.logo_count
        s.logo = i16((n * n * 0x2E) // 0x1E4 - 0x2E)
    end
    local y = s.logo + 0x4C
    local x = MOJI[LOGO][1]
    local list = s.sprites
    list[#list + 1] = { page = 1, u = 0, v = 0, w = 0x80, h = 0x50, x = x, y = y, logo = true }
    list[#list + 1] = { page = 1, u = 0, v = 0x50, w = 0x80, h = 0x30, x = 0x96, y = y, logo = true }
    list[#list + 1] = { page = 2, u = 0, v = 0, w = 0x80, h = 0x20, x = 0x96, y = y + 0x30, logo = true }
    list[#list + 1] = { page = 2, u = 0, v = 0x20, w = 0x80, h = 0x50, x = 0x116, y = y, logo = true }
end

-- ---------------------------------------------------------------- the menu

-- `Title_sel_menu` (0x504060): the rows.
local function sel_menu()
    if s.step == 0 then
        s.slide = 0
        s.pulse_step = 2
        s.pulse = 0
        s.step = 1
    elseif s.step ~= 1 then
        return
    end
    if repeating() then
        if s.edge & PAD_DOWN ~= 0 and s.cur < 4 then
            s.cur = s.cur + 1
            s.cue(SE_CURSOR)
        end
        if s.edge & PAD_UP ~= 0 and s.cur ~= 0 then
            s.cur = s.cur - 1
            s.cue(SE_CURSOR)
        end
    end
    pulse()
    moji_select_mode(s.cur, 0, 0, 0)
    if not confirm() then
        return
    end
    if s.cur == ROW_LOAD then
        s.sel = 0
        s.step = 0
        s.menu = MENU_OUT
        return
    elseif s.cur == ROW_ORIGINAL then
        s.cue(SE_DECIDE)
        s.menu = 5
        s.game_mode = 0
    elseif s.cur == ROW_ARRANGE then
        s.cue(SE_DECIDE)
        s.fg_system = s.fg_system | FG_ARRANGE
        s.cur = 0
        s.b8c[1] = 0
        s.step = 0
        s.menu = 6
        s.game_mode = 1
        s.censorship_off = 1
        return
    elseif s.cur == ROW_SPECIAL then
        s.cue(SE_DECIDE)
        s.menu = 3
    else
        s.sel = 2
        s.menu = MENU_OUT
        s.step = 0
        return
    end
    s.fg_system = s.fg_system & ~FG_ARRANGE
    s.cur = 0
    s.b8c[1] = 0
    s.censorship_off = 0
    s.step = 0
end

-- A choice's slide in under its row (steps 0 and 1 of `0x504240`,
-- `0x5044D0` and `0x504900`): the other rows fade over 8 ticks.
local function slide_in()
    s.dim = u8(s.dim - 0x10)
    s.b8c[0] = u8(s.b8c[0] + 0x10)
    s.slide = s.slide + 1
    if s.slide == 8 then
        s.dim = 0
        s.step = s.step + 1
        s.b8c[0] = 0x80
    end
end

local function slide_start()
    s.slide = 0
    s.dim = 0x80
    s.b8c[0] = 0
    s.pulse = 0
    s.pulse_step = 2
    s.slide_flag = 0
    s.step = 1
end

-- The slide back out (the choices' step 3): `true` once it's over.
local function slide_out()
    s.dim = u8(s.dim + 0x10)
    s.b8c[0] = u8(s.b8c[0] - 0x10)
    s.slide = s.slide - 1
    s.slide_flag = 1
    s.pulse = 0
    if s.slide == 0 then
        s.dim = 0x80
        s.b8c[0] = 0
        return true
    end
    return false
end

-- SPECIAL's choices (`0x505FC0`'s case 4): words 9 to 13 (4TH, TO-FU,
-- EX.BATTLE, EXIT and GALLERY), each at its own place.
local SPECIAL_ROWS = {
    [0] = { 9, 40, 147 },
    { 10, 168, 147 },
    { 11, 8, 173 },
    { 12, 120, 190 },
    { 13, 184, 173 },
}
local SPECIAL_EXIT = 3

-- `0x504900`'s draw case for `Special` (read signed): 0 EXIT alone, 1 with
-- GALLERY, 2 and 3 with EX.BATTLE, 4 to 7 with 4TH, and anything else
-- (this install's 15) every extra.
local function special_case()
    local v = s.special
    if v == 0 or v == 1 then
        return v
    elseif v == 2 or v == 3 then
        return 2
    elseif v >= 4 and v <= 7 then
        return 3
    end
    return 4
end

-- Step 0's cursor for each case.
local SPECIAL_OPEN = { [0] = 3, 4, 2, 0, 0 }

-- `0x505FC0`'s cases 0 to 3: the rows each shows, as { row, word, x, y }.
local SPECIAL_LAYOUTS = {
    [0] = { { 3, 12, 120, 160 } },
    { { 4, 13, 126, 156 }, { 3, 12, 120, 179 } },
    { { 2, 11, 8, 156 }, { 3, 12, 120, 179 }, { 4, 13, 184, 156 } },
    { { 0, 9, 32, 147 }, { 2, 11, 128, 152 }, { 3, 12, 120, 190 }, { 4, 13, 124, 173 } },
}

local function special_draw(case)
    if case == 4 then
        for row = 0, 4 do
            local r = SPECIAL_ROWS[row]
            draw_word_at(r[1], row == s.cur and u8(s.pulse + 0x20) or 0x20, r[2], r[3])
        end
        return
    end
    for _, r in ipairs(SPECIAL_LAYOUTS[case]) do
        local lit = r[1] == s.cur
        if case == 0 then
            lit = true
        elseif case == 1 then
            -- GALLERY is lit unless EXIT is chosen.
            lit = (r[1] == 4) == (s.cur ~= 3)
        end
        draw_word_at(r[2], lit and u8(s.pulse + 0x20) or 0x20, r[3], r[4])
    end
end

-- Step 2's moves for the cases other than 4, each with the cursor cue.
local function special_moves(case)
    local down, up = s.edge & PAD_DOWN ~= 0, s.edge & PAD_UP ~= 0
    local left, right = s.edge & PAD_LEFT ~= 0, s.edge & PAD_RIGHT ~= 0
    local function go(to)
        s.cur = to
        s.cue(SE_CURSOR)
    end
    if case == 0 then
        s.cur = SPECIAL_EXIT
    elseif case == 1 then
        if down and s.cur == 4 then
            go(3)
        elseif up and s.cur == 3 then
            go(4)
        end
    elseif case == 2 then
        if down and s.cur ~= 3 then
            go(3)
        elseif up and s.cur == 3 then
            go(2)
        end
        if left and s.cur == 4 then
            go(2)
        elseif right and s.cur == 2 then
            go(4)
        end
    else
        if down and s.cur == 4 then
            go(3)
        elseif down and (s.cur == 0 or s.cur == 2) then
            go(4)
        end
        if up and s.cur == 3 then
            go(4)
        elseif up and s.cur == 4 then
            go(0)
        end
        if left and s.cur == 2 then
            go(0)
        elseif right and s.cur == 0 then
            go(2)
        end
    end
end

-- `Special` 15's moves (the default case).
local function special_moves_all()
    if s.edge & PAD_DOWN ~= 0 then
        local to = ({ [0] = 2, 4, 3, nil, 3 })[s.cur]
        if to then
            s.cur = to
            s.cue(SE_CURSOR)
        end
    elseif s.edge & PAD_UP ~= 0 then
        local to
        if s.cur == 2 then
            to = 0
        elseif s.cur == SPECIAL_EXIT then
            to = s.special_column ~= 2 and 4 or 2
        elseif s.cur == 4 then
            to = 1
        end
        if to then
            s.cur = to
            s.cue(SE_CURSOR)
        end
    end
    local left, right = s.edge & PAD_LEFT ~= 0, s.edge & PAD_RIGHT ~= 0
    if left and s.cur == 4 then
        s.cur = 2
        s.cue(SE_CURSOR)
    end
    if right and s.cur == 2 then
        s.cur = 4
        s.cue(SE_CURSOR)
    end
    if left and s.cur == 1 then
        s.cur = 0
        s.cue(SE_CURSOR)
    end
    if right and s.cur == 0 then
        s.cur = 1
        s.cue(SE_CURSOR)
    end
    if s.cur == 2 or s.cur == 4 then
        s.special_column = s.cur
    end
end

-- SPECIAL (`0x504900`): its row slides up by 37 over the others' fade, and
-- the choices `Special` opens show below it and take the directions. EXIT
-- and the cancel slide back to the rows. A confirm on 4TH or TO-FU fades
-- slot 2 to black (steps 4 and 5) and, on the tick it is done, starts that
-- mode's title (`title_mv_state` 3). GALLERY's confirm opens its
-- character's choice (page 7) on the next tick. EX.BATTLE's confirm isn't
-- ported and does nothing.
local function special()
    local stepped = s.step
    local case = special_case()
    if s.step == 0 then
        s.cur = SPECIAL_OPEN[case]
        slide_start()
        slide_in()
    elseif s.step == 1 then
        slide_in()
    elseif s.step == 2 then
        if case == 4 then
            special_moves_all()
        else
            special_moves(case)
        end
        pulse()
        if s.key_trg & KEY_CANCEL ~= 0 then
            s.cur = SPECIAL_EXIT
            s.cue(SE_CANCEL)
            s.step = 3
        elseif confirm() then
            if s.cur == SPECIAL_EXIT then
                s.cue(SE_CANCEL)
                s.step = 3
            elseif s.cur == 0 or s.cur == 1 then
                s.cue(SE_DECIDE)
                s.step = 4
            elseif s.cur == 4 then
                s.cue(SE_DECIDE)
                s.menu = MENU_GALLERY
                s.step = 0
            end
        end
    elseif s.step == 3 then
        if slide_out() then
            s.cur = SPECIAL_EXIT
            s.menu = MENU_ROWS
            s.step = 0
        end
    elseif s.step == 4 or s.step == 5 then
        if s.step == 4 then
            fade_set(0x202, 0x800)
            fade_adjust(0x200, 2)
            s.step = 5
        end
        if fade_done(2) then
            fade_set(0x202, 0)
            fade_adjust(0x7FFF, 2)
            if s.cur == 0 then
                s.fg_system = (s.fg_system & ~1) | 8
            else
                s.fg_system = s.fg_system | 9
            end
            -- `FUN_004B73C0`: the new game's work, with flag 1:6.
            s.fg_status = 0x02000000
            s.mv = 3
            s.menu = 0
            s.step = 0
            return
        end
    end
    if stepped == 2 or stepped >= 4 then
        special_draw(case)
    else
        -- While sliding, the draw's case is a register the caller left (ECX,
        -- which `Fade_status(0)` leaves 0): case 0, EXIT alone at 120, 160.
        draw_word_at(12, u8(s.pulse + 0x20), 120, 160)
    end
    moji_select_mode(3, 3, 0x25, s.slide_flag)
end

-- The new-game choice's rows slide from ORIGINAL MODE (row 1, by 6) or
-- ARRANGE MODE (row 2, by 0x16), as `0x689F25` says.
local function game_rows(flag)
    if s.game_mode == 0 then
        moji_select_mode(1, 1, 6, flag)
    else
        moji_select_mode(2, 2, 0x16, flag)
    end
end

-- `0x505C00`: the character row, LEON and CLAIRE (words 37 and 38), then
-- EXIT (word 7), the lit one the character's or EXIT.
local function draw_characters()
    local lit = s.cur == 0 and (s.claire and 2 or 1) or 3
    for k, entry in ipairs({ 37, 38, 7 }) do
        local m = MOJI[entry]
        draw_word_at(entry, k == lit and u8(s.pulse + 0x20) or 0x20, m[1], m[2])
    end
end

-- `Moji_Game_mode` (0x505EA0): NORMAL and EASY (words 5 and 6), EXIT (7)
-- and, with `SuperHardSwitch`, HARD (8), EXIT moving beside it.
draw_difficulty = function(cur)
    local lit
    local rows = { 5, 6, 7 }
    local exit_at = { MOJI[7][1], MOJI[7][3], MOJI[7][5], MOJI[7][6] }
    if s.super_hard then
        rows[4] = 8
        exit_at = { 0xA8, 0x80, 0x80, 0x8A }
        lit = cur + 1
        if lit == 1 and s.easy == 0 then
            lit = 0
        end
    else
        exit_at = { 0x20, 0x100, 0, 0x7B }
        lit = cur == 0 and (s.easy == 1 and 1 or 0) or 2
    end
    for k, entry in ipairs(rows) do
        local m = MOJI[entry]
        local colour = k - 1 == lit and u8(s.pulse + 0x20) or 0x20
        local x, w, u, v = m[1], m[3], m[5], m[6]
        if entry == 7 then
            x, w, u, v = exit_at[1], exit_at[2], exit_at[3], exit_at[4]
        end
        for j, blend in ipairs({ m[7], 0 }) do
            s.sprites[#s.sprites + 1] = {
                page = 0, u = u, v = v, w = w, h = m[4],
                x = x, y = m[2] + j - 1, colour = colour, blend = blend,
            }
        end
    end
end

-- The character's choice (`0x504240`, pages 5 and 6): LEON or CLAIRE with
-- the directions, or EXIT below them. The confirm on the character goes on
-- to the difficulty (page 1 or 2) with no slide; EXIT and the cancel slide
-- back to the rows.
local function character_menu()
    if s.step == 0 then
        slide_start()
        s.char_flag = 0
        s.claire = false
        slide_in()
    elseif s.step == 1 then
        slide_in()
    elseif s.step == 2 then
        if s.edge & PAD_DOWN ~= 0 and s.cur == 0 then
            s.cur = 1
            s.cue(SE_CURSOR)
        elseif s.edge & PAD_UP ~= 0 and s.cur ~= 0 then
            s.cur = s.cur - 1
            s.cue(SE_CURSOR)
        elseif s.cur == 0 then
            if s.edge & PAD_LEFT ~= 0 and s.claire then
                s.claire = false
                s.cue(SE_CURSOR)
            end
            if s.edge & PAD_RIGHT ~= 0 and not s.claire then
                s.claire = true
                s.cue(SE_CURSOR)
            end
        end
        pulse()
        if s.key_trg & KEY_CANCEL ~= 0 then
            s.cue(SE_CANCEL)
            s.step = 3
        elseif confirm() then
            if s.cur == 1 then
                s.cue(SE_CANCEL)
                s.step = 3
            else
                s.cue(SE_DECIDE)
                s.menu = s.menu - 4
            end
        end
    elseif s.step == 3 then
        s.char_flag = 1
        if slide_out() then
            s.cur = 1
            s.menu = MENU_ROWS
            s.step = 0
        end
    end
    draw_characters()
    game_rows(s.char_flag)
end

-- The difficulty (`0x5044D0`, pages 1 and 2): NORMAL or EASY with the
-- directions, HARD below NORMAL with `SuperHardSwitch`, and EXIT. The
-- confirm on a difficulty starts the game through the confirm's flash and
-- fade (page 4, `Ti_Sel_mode` 1); EXIT and the cancel slide back to the rows.
local function difficulty_menu()
    if s.step == 0 then
        slide_start()
        s.saved_easy = s.easy
        s.diff_flag = 0
        slide_in()
    elseif s.step == 1 then
        slide_in()
    elseif s.step == 2 then
        local left, right = s.edge & PAD_LEFT ~= 0, s.edge & PAD_RIGHT ~= 0
        local down, up = s.edge & PAD_DOWN ~= 0, s.edge & PAD_UP ~= 0
        if s.super_hard then
            if down then
                if s.cur == 0 and s.easy == 0 then
                    s.cur = 2
                    s.cue(SE_CURSOR)
                end
                if s.cur == 0 and s.easy == 1 then
                    s.cur = 1
                    s.cue(SE_CURSOR)
                end
            end
            if up then
                if s.cur == 1 then
                    s.cur = 0
                    s.easy = 1
                    s.cue(SE_CURSOR)
                end
                if s.cur == 2 then
                    s.cur = 0
                    s.easy = 0
                    s.cue(SE_CURSOR)
                end
            end
        else
            if down and s.cur == 0 then
                s.cur = 1
                s.cue(SE_CURSOR)
            end
            if up and s.cur ~= 0 then
                s.cur = s.cur - 1
                s.cue(SE_CURSOR)
            end
        end
        if s.cur == 0 then
            if left and s.easy == 1 then
                s.easy = 0
                s.cue(SE_CURSOR)
            end
            if right and s.easy == 0 then
                s.easy = 1
                s.cue(SE_CURSOR)
            end
        elseif s.super_hard then
            if left and s.cur == 1 then
                s.cur = 2
                s.cue(SE_CURSOR)
            end
            if right and s.cur == 2 then
                s.cur = 1
                s.cue(SE_CURSOR)
            end
        end
        pulse()
        if s.key_trg & KEY_CANCEL ~= 0 then
            s.cue(SE_CANCEL)
            s.step = 3
            s.easy = s.saved_easy
        end
        if confirm() then
            if s.cur == 1 then
                s.cue(SE_CANCEL)
                s.step = 3
                s.easy = 0
            else
                if s.super_hard and s.cur == 2 then
                    s.hard_mode = 1
                elseif s.easy == 1 then
                    s.fg_system = s.fg_system | FG_EASY
                end
                s.fg_system = s.fg_system | FG_NEW_GAME
                s.sel = 1
                s.menu = MENU_OUT
            end
        end
    elseif s.step == 3 then
        s.diff_flag = 1
        if slide_out() then
            s.cur = (s.menu == 2) and 2 or 1
            s.menu = MENU_ROWS
            s.step = 0
        end
    end
    draw_difficulty(s.cur)
    game_rows(s.diff_flag)
end

-- GALLERY's word (the table's last entry, `p` at 0x691EC4).
local GALLERY_WORD = 39

-- GALLERY's character (`0x505020`, page 7): the word GALLERY over LEON and
-- CLAIRE with the directions, and EXIT below them; the rows aren't drawn
-- under it. It counts the slide's 8 ticks in and out as the other choices
-- do, though nothing it draws moves or fades with them. EXIT and the
-- cancel go back to the rows with the cursor on ORIGINAL MODE. The confirm
-- on a character fades slot 2 to black (steps 4 and 5) and, on the tick it
-- is done, puts the page back at step 2 and starts the gallery's task with
-- that character (`fg_status` bit 31), under which the title's sleeps
-- until the gallery's EXIT.
local function gallery()
    if s.step == 0 then
        slide_start()
        s.claire = false
        s.cur = 0
        slide_in()
    elseif s.step == 1 then
        slide_in()
    elseif s.step == 2 then
        if s.edge & PAD_DOWN ~= 0 and s.cur == 0 then
            s.cur = 1
            s.cue(SE_CURSOR)
        elseif s.edge & PAD_UP ~= 0 and s.cur ~= 0 then
            s.cur = s.cur - 1
            s.cue(SE_CURSOR)
        elseif s.cur == 0 then
            if s.edge & PAD_LEFT ~= 0 and s.claire then
                s.claire = false
                s.cue(SE_CURSOR)
            elseif s.edge & PAD_RIGHT ~= 0 and not s.claire then
                s.claire = true
                s.cue(SE_CURSOR)
            end
        end
        pulse()
        if s.key_trg & KEY_CANCEL ~= 0 then
            s.cue(SE_CANCEL)
            s.step = 3
        elseif confirm() then
            if s.cur == 1 then
                s.cue(SE_CANCEL)
                s.step = 3
            else
                s.cue(SE_DECIDE)
                s.step = 4
            end
        end
    elseif s.step == 3 then
        if slide_out() then
            s.cur = 1
            s.menu = MENU_ROWS
            s.step = 0
        end
    elseif s.step == 4 or s.step == 5 then
        if s.step == 4 then
            fade_set(0x202, 0x800)
            fade_adjust(0x200, 2)
            s.step = 5
        end
        if fade_done(2) then
            fade_set(0x202, 0)
            fade_adjust(0x7FFF, 2)
            s.step = 2
            fade_off(2)
            -- `Task_execute(1, Gallery)`: the task's first step runs on
            -- this tick, and its menu from the next.
            s.gal = Gallery.new(s.claire, s.gallery)
            -- `Movie_set(1)`: this frame isn't presented, so the screen
            -- keeps the fade's last one.
            s.frame = "held"
        end
    end
    draw_characters()
    draw_word(GALLERY_WORD, 0x20, MOJI[GALLERY_WORD][2])
end

-- The choices below the rows.
local function sub_menu(api)
    if s.menu == 5 or s.menu == 6 then
        character_menu()
    elseif s.menu == 1 or s.menu == 2 then
        difficulty_menu()
    elseif s.menu == 3 then
        special()
    elseif s.menu == MENU_GALLERY then
        gallery()
    end
end

-- ---------------------------------------------------------------- the steps

-- `Title_init` (0x5038B0). The boot reaches it with `fg_system` bit 2 set
-- (the warning screen's `Capcom_logo` sets it), so it takes that branch and
-- clears the bit: the screen white (fade slot 0 added at 0x7F80), the logo
-- off the screen, and step 5 next. Any later entry (back from OPTION) fades
-- in from black over 31 ticks (`Ti_kido` from 0x7C00 down by 0x400).
-- The title's generator (`0x50B910`), which picks Leon or Claire for its
-- film and its attract demo: `x * 5 + 0x19573BA7`, a pick being the high
-- half of `x * n`. Its first value is the clock's (`timeGetTime`'s bits,
-- reversed), or a run's own (`load_game_pack`'s `title_seed`). The value is
-- one of the executable's statics (`0x689FB0`), kept past this script.
local function title_rnd(api, n)
    local x = api:kept("rnd")
    if x then
        x = (x * 5 + 0x19573BA7) & 0xFFFFFFFF
    elseif s.seed then
        x = s.seed & 0xFFFFFFFF
    else
        local time = api:local_time()
        local ms = time and ((time.hour * 60 + time.minute) * 60 + time.second) * 1000 + time.millisecond or 0
        local reversed = 0
        for bit = 0, 31 do
            reversed = reversed | ((ms >> bit) & 1) << (31 - bit)
        end
        x = (reversed * 5 + 0x19573BA7) & 0xFFFFFFFF
    end
    api:keep("rnd", x)
    return (x * n) >> 32
end

-- What the title's statics hold as it hands over to its attract demo, for
-- the title that starts after it to find (`api:keep`): `Init_global` leaves
-- them, and `Title_init` sets them again 122 ticks on.
local DEMO_KEPT = {
    "mv", "disp", "menu", "step", "sel", "cur", "pulse", "slide", "demo", "flash", "logo", "logo_count", "kido",
    "add",
}

local function title_init()
    -- `Title_bg_reload` (0x503880), on both branches: the option screen
    -- leaves its own picture behind.
    s.background = s.title_background
    s.limiter = 0
    s.fg_system = s.fg_system & 0xFFF7FFBF
    s.censorship_off = 0
    s.sel = 0
    s.cur = 0
    s.pulse = 0
    s.pulse_step = 1
    s.disp = 0
    s.menu = 0
    s.step = 0
    s.demo = 0
    s.b8c[1] = 0
    s.b8c[2] = 0
    s.easy = 0
    s.logo = i16(0xFFD2)
    s.flash = 0
    s.logo_count = -1
    if s.fg_system & FG_FROM_LOGO == 0 then
        fade_set(0x200, 0)
        fade_adjust(0x7C00)
        s.add = 0xFC00
        s.kido = 0x7C00
        s.disp = 1
    else
        s.fg_system = s.fg_system & ~FG_FROM_LOGO
        fade_set(0x100, 0)
        fade_adjust(0x7F80)
        s.logo = 0xF0
        s.disp = 5
    end
    moji_set_work()
    s.mv = s.mv + 1
    if s.open_choice then
        -- As ORIGINAL MODE's confirm leaves it (`Title_sel_menu`).
        s.open_choice = nil
        s.menu = 5
        s.game_mode = 0
        s.cur = 0
        s.b8c[1] = 0
        s.censorship_off = 0
        s.step = 0
    end
end

-- ------------------------------------------------------- the option screen

-- OPTION's screen: task 1 (`0x44A040`), which runs while the title's task
-- sleeps. Its step 0 is `Config_set_data` (0x44AD40), step 1 `Config_main`
-- (0x44A0B0) with its draw (0x44B180), and step 2 the exit (0x44AED0).
-- `Config_main`'s states: 0 the fade in, 1 the menu, 3 and 7 MONITOR
-- TUNING's picture, 4 the SOUND page and 8 the wait after a change there, 6,
-- 0x16 and 0x17 the way out. KEYCONFIG (states 2 and 9 to 0x15, the keys'
-- page with the player's model) isn't ported: its row takes no decide
-- (oracle/re2-deviations.md, "The title"; #807).
--
-- The sheets are textures 0x1B (`moji1.tim`, the words) and 0x1A
-- (`parts.tim`, the arrows); the volumes' digits are the message font's.
-- Classic REbirth sets the sound page's rows with its own print in place
-- of the sheet's words, each value ending where the volumes' right arrow
-- starts: the recordings are where its words and their places come from.
local OPT_WORDS, OPT_PARTS = 4, 5
local OPT_GREY, OPT_GREEN = { 0x80, 0x80, 0x80 }, { 0x00, 0xA0, 0x20 }
-- The cursor's row in Classic REbirth's print, and in the digits, which
-- take the sheets' green at the font's level.
local OPT_TEXT_GREEN, OPT_DIGIT_GREEN = { colour = "#00C80A", row = 1 }, "#00FF40"

-- The sound settings, which outlast a visit: the speaker mode (1 monaural, 2
-- stereo), the quality (`Config_get_squal`: 0 is 44.1 kHz at 16 bits) and
-- the two volumes. The volumes are arklay's music and effects volumes when
-- the engine has settings to give (`M.on_volumes`); the rest is the
-- screen's own (oracle/re2-deviations.md, "The title").
local sound = { speaker = 2, quality = 0, bgm = 100, se = 100 }

-- Whether the engine gave the volumes, which are then published for it
-- to take a change from (`music_volume`, `effects_volume`).
local volumes_given = false

-- A volume as the page holds it: 1 to 100.
local function option_volume_range(value)
    if value < 1 then
        return 1
    end
    return value > 100 and 100 or value
end

-- The screen's work, which is static in the original and so outlasts a
-- visit too: a second one opens with the cursor's row until `Config_main`
-- clears it, and with the blink counts where the first left them.
local option_work = {
    step = 0,
    state = 0,
    cursor = 0,
    type = 0,
    wait = 0,
    -- The arrows' blink counts: the mode's, the quality's, the two
    -- volumes' and EXIT's.
    blink = { mode = 0, quality = 0, bgm = 0, se = 0, exit = 0 },
}

-- The work and the sound values are the executable's statics, which last as
-- long as the process: they are kept past this script (`api:keep`), for the
-- title's next opening to find.
local OPT_KEPT = {
    { option_work, "cursor", "opt_cursor" },
    { option_work, "type", "opt_type" },
    { option_work, "wait", "opt_wait" },
    { option_work.blink, "mode", "opt_blink_mode" },
    { option_work.blink, "quality", "opt_blink_quality" },
    { option_work.blink, "bgm", "opt_blink_bgm" },
    { option_work.blink, "se", "opt_blink_se" },
    { option_work.blink, "exit", "opt_blink_exit" },
    { sound, "speaker", "snd_speaker" },
    { sound, "quality", "snd_quality" },
    { sound, "bgm", "snd_bgm" },
    { sound, "se", "snd_se" },
}

local function option_restore(api)
    for _, kept in ipairs(OPT_KEPT) do
        local value = api:kept(kept[3])
        if value then
            kept[1][kept[2]] = value
        end
    end
end

local function option_keep(api)
    for _, kept in ipairs(OPT_KEPT) do
        api:keep(kept[3], kept[1][kept[2]])
    end
end

local function option_new()
    option_work.step, option_work.state = 0, 0
    return option_work
end

local function option_sprt(page, x, y, u, v, w, rgb)
    s.sprites[#s.sprites + 1] = { page = page, u = u, v = v, w = w, h = 14, x = x, y = y, rgb = rgb }
end

local function option_colour(on)
    return on and OPT_GREEN or OPT_GREY
end

-- A row's blink: its arrows show the count's first 20 ticks while the row
-- is the cursor's, and the count stands while it isn't. The original's
-- period is 31 ticks, which EXIT's keeps; Classic REbirth's rows have 30.
local function option_blink(count)
    local o = s.opt
    local shown = o.blink[count] < 0x14
    o.blink[count] = o.blink[count] + 1
    if o.blink[count] > (count == "exit" and 0x1E or 0x1D) then
        o.blink[count] = 0
    end
    return shown
end

local function option_arrow(x, y, right)
    option_sprt(OPT_PARTS, x, y, right and 0xDF or 0xD1, 199, 14, OPT_GREEN)
end

-- `Config_draw_sexit` (0x44BEA0): EXIT, with the arrows on the sound page.
local function option_exit(x, y, on)
    local o = s.opt
    option_sprt(OPT_WORDS, x, y, 0, 0xD2, 0x35, option_colour(on))
    if on and (o.state == 4 or o.state == 8) and option_blink("exit") then
        option_arrow(x - 14, y, false)
        option_arrow(x + 0x35, y, true)
    end
end

-- Where the sound page's values end, and the volumes' digit cells.
local OPT_VALUE_END = 0x10E
local OPT_DIGITS = { 0xE4, 0xF2, 0x100 }

-- A row of the sound page: its name from x 0x32, and its value's word up
-- to `OPT_VALUE_END` with the arrows about it while the blink shows them.
local function option_row(count, y, name, value, on)
    local texts = s.opt_texts
    texts[#texts + 1] = { x = 0x32, y = y, text = name, on = on }
    texts[#texts + 1] = { y = y, text = value, on = on, arrows = on and option_blink(count) }
end

-- `Config_draw_bgmvol` (0x44C050) and `Config_draw_sevol` (0x44C2E0): the
-- name, the value's digits in their cells in the row's colour, and the
-- arrows towards what the volume can still reach.
local function option_volume(count, y, name, value, on)
    local texts = s.opt_texts
    texts[#texts + 1] = { x = 0x32, y = y, text = name, on = on }
    if value // 100 ~= 0 then
        texts[#texts + 1] = { cell = OPT_DIGITS[1], y = y, text = "1", on = on }
    end
    if value // 10 ~= 0 then
        texts[#texts + 1] = { cell = OPT_DIGITS[2], y = y, text = tostring(value // 10 % 10), on = on }
    end
    texts[#texts + 1] = { cell = OPT_DIGITS[3], y = y, text = tostring(value % 10), on = on }
    if on and option_blink(count) then
        if value > 1 then
            option_arrow(0xD6, y, false)
        end
        if value < 100 then
            option_arrow(OPT_VALUE_END, y, true)
        end
    end
end

-- Where the print puts a digit for the original's cell: the original's
-- sprite is the font's whole cell, and the print draws a digit's cell a
-- pixel before its place, the bearing the executable's table gives every
-- digit (`by_table`, `CrText.by_table`). A font without the table (Classic
-- REbirth's Western sheet, in a pack converted without `font0.tim`) trims a
-- glyph to its ink: by half of what its width leaves of the cell, but the
-- 6's, a pixel less.
function M.option_digit_x(cell, digit, width, by_table)
    if by_table then
        return cell + 1
    end
    return cell + (15 - width) // 2 - (digit == "6" and 1 or 0)
end

local OPT_SPEAKERS = { "MONO", "STEREO" }
local OPT_QUALITIES = { [0] = "HIGH", "MEDIUM", "LOW" }

-- The draw (0x44B180), by the state `Config_main` left.
local function option_draw()
    local o = s.opt
    local state = o.state
    if state == 1 or state == 3 then
        option_sprt(OPT_WORDS, 0x61, 0x4B, 0x3A, 0, 0x7E, option_colour(o.cursor == 0))
        option_sprt(OPT_WORDS, 0x41, 0x63, 0x3A, 0x0E, 0xBD, option_colour(o.cursor == 1))
        option_sprt(OPT_WORDS, 0x40, 0x7B, 0x36, 0xD2, 0x46, option_colour(o.cursor == 2))
        -- The mode beside SOUND (`Config_draw_monoraul`, 0x44BD80, and
        -- `Config_draw_stereo`, 0x44BE10), in Classic REbirth's print too.
        s.opt_texts[#s.opt_texts + 1] = { y = 0x7B, text = OPT_SPEAKERS[sound.speaker], on = o.cursor == 2 }
        option_exit(0x85, 0x93, o.cursor == 3)
    elseif state == 4 or state == 8 then
        option_sprt(OPT_WORDS, 0x7D, 0x32, 0x36, 0xD2, 0x46, OPT_GREEN)
        -- `Config_draw_sound` (0x44BFE0): the mode, the quality, the two
        -- volumes and EXIT.
        option_row("mode", 0x50, "MODE", OPT_SPEAKERS[sound.speaker], o.type == 0)
        option_row("quality", 0x63, "QUALITY", OPT_QUALITIES[sound.quality], o.type == 1)
        option_volume("bgm", 0x76, "BGM VOLUME", sound.bgm, o.type == 2)
        option_volume("se", 0x89, "SE.VOLUME", sound.se, o.type == 3)
        option_exit(0x84, 0x9C, o.type == 4)
    elseif state == 7 then
        option_exit(0xF5, 0xCA, true)
    end
end

-- A volume a step down or up: one, or ten with the decide key held, kept
-- within 1 to 100.
local function option_step(value, way)
    value = value + way * (s.held_confirm and 10 or 1)
    if value < 1 then
        return 1
    end
    return value > 100 and 100 or value
end

-- The SOUND page (state 4): the rows on the edges, the values on the raw
-- pad's left and right, each change followed by state 8's five ticks.
local function option_sound()
    local o = s.opt
    local trg = s.edge
    if trg & PAD_UP ~= 0 then
        if o.type == 0 then
            return
        end
        s.cue(SE_CURSOR)
        o.type = o.type - 1
    end
    if trg & PAD_DOWN ~= 0 then
        if o.type == 4 then
            return
        end
        s.cue(SE_CURSOR)
        o.type = o.type + 1
    end
    if trg & PAD_CANCEL ~= 0 then
        s.cue(SE_CANCEL)
        o.state = 1
        o.type = 0
    end
    if trg & PAD_CONFIRM ~= 0 and o.type == 4 then
        s.cue(SE_DECIDE)
        o.state = 1
        o.type = 0
    end
    local function changed()
        o.state = 8
        o.wait = 5
    end
    if s.raw & PAD_LEFT ~= 0 then
        if o.type == 0 then
            s.cue(SE_CURSOR)
            sound.speaker = sound.speaker == 2 and 1 or 2
            changed()
        elseif o.type == 1 then
            sound.quality = ({ [0] = 2, 0, 1 })[sound.quality]
            s.cue(SE_CURSOR)
            changed()
        elseif o.type == 2 and sound.bgm > 1 then
            s.cue(SE_CURSOR)
            sound.bgm = option_step(sound.bgm, -1)
            changed()
        elseif o.type == 3 and sound.se > 1 then
            s.cue(SE_CURSOR)
            sound.se = option_step(sound.se, -1)
            changed()
        end
    end
    if s.raw & PAD_RIGHT ~= 0 then
        if o.type == 0 then
            s.cue(SE_CURSOR)
            sound.speaker = sound.speaker == 1 and 2 or 1
            changed()
        elseif o.type == 1 then
            sound.quality = ({ [0] = 1, 2, 0 })[sound.quality]
            s.cue(SE_CURSOR)
            changed()
        elseif o.type == 2 and sound.bgm < 100 then
            s.cue(SE_CURSOR)
            sound.bgm = option_step(sound.bgm, 1)
            changed()
        elseif o.type == 3 and sound.se < 100 then
            s.cue(SE_CURSOR)
            sound.se = option_step(sound.se, 1)
            changed()
        end
    end
end

-- The way out: slot 0 fades to black by 0x400 a tick.
local function option_leave()
    s.opt.state = 0x16
    fade_set(0x200, 0x400)
end

-- `Config_main`: one state a tick, then the draw of the state it left.
local function option_main(api)
    local o = s.opt
    local trg = s.edge
    if o.state == 0 then
        if fade_done(0) then
            o.state = 1
        end
        o.cursor = 0
    elseif o.state == 1 then
        o.type = 0
        if trg & PAD_START ~= 0 then
            s.cue(SE_CANCEL)
            option_leave()
        end
        if trg & PAD_UP ~= 0 and o.cursor > 0 then
            s.cue(SE_CURSOR)
            o.cursor = o.cursor - 1
        end
        if trg & PAD_DOWN ~= 0 and o.cursor < 3 then
            s.cue(SE_CURSOR)
            o.cursor = o.cursor + 1
        end
        -- The decide adds the row to the state as it stands, which after a
        -- START on the same tick is a state the original has no case for;
        -- here START's way out stands. KEYCONFIG's row takes no decide.
        if trg & PAD_CONFIRM ~= 0 and o.state == 1 and o.cursor ~= 0 then
            if o.cursor == 3 then
                s.cue(SE_CANCEL)
                o.state = 6
            else
                s.cue(SE_DECIDE)
                o.state = o.cursor + 2
            end
        end
        if trg & PAD_CANCEL ~= 0 then
            s.cue(SE_CANCEL)
            option_leave()
        end
    elseif o.state == 3 then
        s.background = api:picture("ui/option/color2")
        o.state = 7
    elseif o.state == 4 then
        option_sound()
    elseif o.state == 6 then
        option_leave()
    elseif o.state == 7 then
        if trg & (PAD_CONFIRM | PAD_CANCEL) ~= 0 then
            s.cue(trg & PAD_CONFIRM ~= 0 and SE_DECIDE or SE_CANCEL)
            o.state = 1
            s.background = api:picture("ui/option/room")
        end
    elseif o.state == 8 then
        o.wait = o.wait - 1
        if o.wait == 0 then
            o.state = 4
        end
    elseif o.state == 0x16 then
        -- `Movie_set(2)`: this frame and the next aren't presented, so the
        -- screen keeps the fade's last one.
        if fade_done(0) then
            o.state = 0x17
            s.frame = "held"
        end
    elseif o.state == 0x17 then
        o.state = 0
        o.step = 2
        s.frame = "held"
        return
    end
    option_draw()
end

-- One tick of the option task.
local function option_task(api)
    local o = s.opt
    if o.step == 0 then
        -- `Config_set_data`: black on both buffers, the room's picture and
        -- the fade in by 0x1800 a tick. The pad's repeat is the status
        -- screen's mask with its own delays, which the title keeps after.
        s.rep_mask, s.rep_delay = 0xF01C, 0x40C
        s.limiter = 2
        s.background = api:picture("ui/option/room")
        fade_set(0x200, -0x1800)
        o.step = 1
        -- The picture is the draw's (`Bg_draw`), which this tick hasn't.
        s.frame = "black"
    elseif o.step == 1 then
        option_main(api)
        option_keep(api)
    else
        -- The exit: the task ends, and the title's wakes on the next tick.
        s.opt = nil
        s.frame = "black"
    end
end

-- `Title_main`'s step 5 (0x503B20): the logo's slide from the top and the
-- white tiles' level, then the menu.
local function title_enter()
    if s.menu == 0 then
        s.flash = 0x36
        s.logo_count = 0x16
        s.logo = 0
    end
    s.disp = 1
    s.menu = 0
    -- The tiles (0x540790 and 0x5407B0): two bands taken away at the white's
    -- level, drawn on this tick only, in front of the fade.
    if s.flash > 0 then
        s.bands = s.flash
    end
end

-- `Title_main`'s step 1, the menu (`Title_main_select`): the fade in, the
-- demo's count, the rows, and the confirm's flash and fade out.
local function title_select(api)
    local sub = s.sub
    if sub == 1 or sub == 2 or sub == 5 then
        if not fade_done() then
            moji_sel()
            title_image()
            s.sleeping = true
            return
        end
        if sub == 1 then
            fade_set(0x200, 0x80)
            fade_adjust(0x200)
            s.sub = 2
            -- The original carries on into step 2's check on the same tick.
            if not fade_done() then
                moji_sel()
                title_image()
                s.sleeping = true
                return
            end
        end
        fade_set(0x200, 0)
        fade_adjust(0x7FFF)
        s.sub = sub == 5 and 6 or 3
        s.sleeping = true
        return
    end
    if sub == 3 or sub == 6 then
        s.disp = s.sel + 2
        moji_license()
        s.sub = 0
        return
    end
    -- The rows.
    s.kido = (s.kido + s.add) & 0xFFFF
    fade_set(0x200, 0)
    fade_adjust(s.kido)
    if fade_done() then
        s.add = 0
        s.kido = 0xFFFF
    end
    s.demo = s.demo + 1
    if s.demo > 0x707 then
        if s.demo == 0x708 then
            s.add = 0x100
            s.kido = 0
        elseif fade_done() then
            -- The demo's pad file is loaded and step 3 starts it: Leon's
            -- or Claire's (`pl<n>\pld\pdemo<n><cycle>.dat`), of the three
            -- the title plays in turn (`0x989E7F`). The pack has them as
            -- `demo/<cycle * 2 + n>.dat`.
            fade_set(0x200, 0)
            fade_adjust(0x7FFF)
            local cycle = api:kept("demo_cycle") or 0
            s.reel = cycle * 2 + title_rnd(api, 2)
            api:keep("demo_cycle", (cycle + 1) % 3)
            s.attract = true
            s.disp = 3
        end
        if s.edge & 0x9FF ~= 0 then
            s.cue(SE_CANCEL)
            s.demo = 0
            s.add = 0xF000
            s.step = 0
        end
        moji_license()
        s.sub = 0
        return
    end
    if s.edge ~= 0 then
        s.demo = 0
    end
    if s.menu == MENU_ROWS then
        sel_menu()
        moji_license()
        s.sub = 0
        return
    elseif s.menu ~= MENU_OUT then
        sub_menu(api)
        moji_license()
        s.sub = 0
        return
    end
    if s.sel == 2 then
        -- OPTION: the decide cue and a fade to black.
        s.cue(SE_DECIDE)
        fade_set(0x200, 0x1800)
        fade_adjust(0x200)
        s.sub = 5
        if not fade_done() then
            title_image()
            moji_sel()
            s.sleeping = true
            return
        end
        return
    end
    s.cue(SE_CONFIRM_A)
    s.cue(SE_CONFIRM_B)
    fade_set(0x100, -0x800)
    s.sub = 1
    if not fade_done() then
        moji_sel()
        title_image()
        s.sleeping = true
    end
end

local function title_main(api)
    if s.disp == 1 then
        title_select(api)
    elseif s.disp == 5 then
        title_enter()
    elseif s.disp == 2 then
        -- `Title_main_load` (0x505320): its first tick starts `Mem_card`
        -- (the load screen) and sleeps under it; the tick after that task
        -- ends without a load it reloads the picture and starts the title
        -- again from `Title_init`. A load doesn't come back here.
        if s.sub == 0 then
            s.sub = 1
            s.limiter = 2
            s.exit = "save_load"
        elseif s.loaded then
            -- A save was read (`fg_system`'s 0x400 down): step 3 is the
            -- hand-over to the game, on the next tick.
            s.fg_system = s.fg_system & ~FG_NEW_GAME
            s.disp = 3
            s.sub = 0
        else
            s.fg_system = s.fg_system & ~FG_NO_LOAD
            s.background = s.title_background
            s.mv = 1
            s.sub = 0
        end
    elseif s.disp == 3 and s.loaded then
        s.exit = "continue"
    elseif s.disp == 3 and s.fg_system & 8 ~= 0 then
        -- The 4th Survivor's or To-fu's new game (`Title_game_init`'s
        -- extras branch) isn't ported: the title starts again from
        -- `Title_init`, as after OPTION (oracle/re2-deviations.md,
        -- "The title").
        s.fg_system = s.fg_system & ~(9 | FG_NEW_GAME)
        init_global()
        api:core_bank(0x10)
        s.mv = 1
    elseif s.disp == 3 then
        s.exit = s.attract and "attract" or "new_game"
        if not s.attract and s.prologue_art then
            -- `Title_game_init`'s step 2 starts the prologue's task on the
            -- next tick (0x506750) and the game waits for it to end.
            s.exit = nil
            s.game_init = 0
        end
        s.character = s.claire and 1 or 0
        -- What the game starts with besides the character: the difficulty
        -- confirm's EASY bit and `hard_mode`, and ARRANGE MODE's
        -- `censorship_off`, which the new game's setup keeps in its saved work.
        s.choice = {
            easy = s.fg_system & FG_EASY ~= 0,
            hard_mode = s.hard_mode ~= 0,
            censorship_off = s.censorship_off ~= 0,
        }
    elseif s.disp == 4 then
        -- OPTION (`Title_main_option`, 0x505400): step 4 starts the option
        -- task, which runs from this tick while the title's sleeps, and
        -- leaves the title to start again from `Title_init` once it ends.
        s.disp = 1
        s.mv = 1
        s.opt = option_new()
    end
end

-- `Title_game_init` (0x506750) from the tick after the hand-over, for a
-- new game: its step 2 starts the prologue's task (`Prologue`, 0x4CEFF0),
-- which runs from the next tick, and its step 3 waits for that task's end,
-- the tick the game is started on.
local function game_init(api, input)
    s.game_init = s.game_init + 1
    if s.game_init == 1 then
        s.init, s.sub = 0, 2
        fade_off(0)
        s.background = nil
        s.pro = Prologue.new(PROLOGUE_FADES, s.claire)
        return
    end
    s.sub = 3
    Prologue.tick(s.pro, input)
    Prologue.sound(api, s.pro, s.claire and 1 or 0)
    if s.pro.done then
        s.exit = "new_game"
    elseif s.pro.film then
        -- The title sleeps while the film plays (`M.on_film`).
        s.exit = "film"
    end
end

-- ------------------------------------------- the 4th Survivor's title

-- `FUN_005064E0(c, c21, semi)`: the logo's four pieces (words 17 to 20, on
-- texture 0x15, depth 11) at `c` when it isn't 0, then the name (word 21,
-- depth 12) at `c21`. Their code has no semi-transparent bit, so both are
-- opaque, `semi` or not.
local function survivor_logo(c, c21)
    local function add(entry, colour, depth)
        local m = MOJI[entry]
        local w = s.work[entry]
        s.sprites[#s.sprites + 1] = {
            page = 3, u = m[5], v = m[6], w = m[3], h = m[4],
            x = w.x, y = w.y, colour = colour, depth = depth,
        }
    end
    if c ~= 0 then
        for entry = 17, 20 do
            add(entry, c, 11)
        end
    end
    add(21, c21, 12)
end

-- `FUN_00506440`: START and EXIT (words 14 and 15), the chosen one lit.
local function survivor_rows()
    for row = 0, 1 do
        local entry = 14 + row
        draw_word(entry, row == s.cur and u8(s.pulse + 0x20) or 0x20, s.work[entry].y)
    end
end

-- `Load_title_survivor` (0x505460, `title_mv_state` 3), all in one tick:
-- the mode's core bank (0x12, or To-fu's 0x13), its logo's sheet as
-- texture 0x15, slot 3 at 0x7C00 over its background (`title_o.adt`, or
-- `title_o2.adt`), and the counts for the logo's build-up.
local function survivor_load(api)
    local tofu = s.fg_system & 1 ~= 0
    api:core_bank(tofu and 0x13 or 0x12)
    pages[3] = api:indexed_page(tofu and "ui/title/tofulogo" or "ui/title/spy_logo")
    fade_off(0)
    fade_off(1)
    fade_off(2)
    fade_set(0x203, 0)
    fade_adjust(0x7C00, 3)
    s.background = api:picture(tofu and "ui/title/title_o2" or "ui/title/title_o")
    moji_set_work()
    s.mv = 4
    s.cur = 0
    s.add = 0
    s.kido = 0
    s.pulse = 0
    s.pulse_step = 2
    s.disp = 0
    s.menu = 0
    s.step = 0
    s.dim = 0
    s.b8c[0] = 0
    s.logo = 0xF0
    s.flash = 0
    s.logo_count = -1
    fade_off(0)
    s.sub = 0
end

-- `FUN_00505670`'s `Ti_Disp_add` 0: the name fades in over 64 ticks (page
-- 0), the logo over 128 (page 1), then slot 3 lifts off the background
-- (page 2) and the decide cue plays. A button skips it all.
local function survivor_build()
    fade_adjust(0x7C00, 3)
    if s.menu == 0 then
        s.dim = u8(s.dim + 2)
        survivor_logo(0, s.dim)
        if s.dim >= 0x80 then
            s.dim = 0x80
            s.menu = s.menu + 1
        end
    elseif s.menu == 1 then
        s.b8c[0] = u8(s.b8c[0] + 1)
        survivor_logo(s.b8c[0], 0x80)
        if s.b8c[0] >= 0x80 then
            s.b8c[0] = 0x80
            s.add = 0xFF00
            s.kido = 0x5A00
            s.menu = s.menu + 1
        end
    elseif s.menu == 2 then
        survivor_logo(0x80, 0x80)
        s.kido = (s.kido + s.add) & 0xFFFF
        fade_adjust(s.kido, 3)
        if fade_done(3) then
            s.cue(SE_DECIDE)
            s.add = 0
            s.kido = 0xFFFF
            s.disp = 1
        end
    end
    if s.edge & 0x9FF ~= 0 then
        s.cue(SE_DECIDE)
        s.disp = s.disp + 1
        s.add = 0xF000
        if s.kido == 0 then
            s.kido = 0x5A00
        end
    end
end

-- `FUN_00505670`'s `Ti_Disp_add` 1, by task 0's +0xA: the rows (0, then
-- 5), START's flash and fade (1, 2) and EXIT's fade (2), then the hand-off
-- (3, 4): START to the new game, EXIT back to `Title_init`.
local function survivor_menu(api)
    local sub = s.sub2
    if sub == 3 or sub == 4 then
        if sub == 3 and s.cur ~= 0 then
            s.sub2 = 4
            api:core_bank(0x10)
        end
        -- `Title_bg_reload`.
        s.background = api:picture("ui/title/title_bg")
        if s.cur == 0 then
            s.mv = 2
            s.disp = 3
        else
            s.mv = 1
        end
        s.sub2 = 0
        return
    end
    if sub ~= 1 and sub ~= 2 and sub ~= 5 then
        s.kido = (s.kido + s.add) & 0xFFFF
        fade_adjust(s.kido, 3)
        if fade_done(3) then
            s.add = 0
            s.kido = 0xFFFF
        end
        if confirm() then
            if not fade_done(3) then
                s.add = 0xF000
                sub = 5
            elseif s.cur == 0 then
                s.fg_system = s.fg_system | FG_NEW_GAME
                s.cue(SE_CONFIRM_A)
                s.cue(SE_CONFIRM_B)
                fade_set(0x100, -0x800)
                s.sub2 = 1
                sub = 1
            else
                s.cue(SE_CANCEL)
                s.fg_system = s.fg_system & 0xFFFFFFF6
                init_global()
                fade_set(0x200, 0x1800)
                fade_adjust(0x200)
                s.sub2 = 2
                sub = 2
            end
        else
            sub = 5
        end
    end
    if sub == 5 then
        pulse()
        if s.edge & PAD_UP ~= 0 and s.cur == 1 then
            s.cue(SE_CURSOR)
            s.cur = 0
        end
        if s.edge & PAD_DOWN ~= 0 and s.cur == 0 then
            s.cue(SE_CURSOR)
            s.cur = 1
        end
        s.dim = s.dim + 0x10 >= 0x80 and 0x80 or s.dim + 0x10
        s.b8c[0] = s.b8c[0] + 0x10 >= 0x80 and 0x80 or s.b8c[0] + 0x10
        survivor_logo(s.b8c[0], s.dim)
        survivor_rows()
        s.sub2 = 0
        return
    end
    if sub == 1 then
        if not fade_done(0) then
            survivor_logo(0x80, 0x80)
            survivor_rows()
            s.sleeping = true
            return
        end
        fade_set(0x200, 0x80)
        fade_adjust(0x200)
        s.sub2 = 2
    end
    -- Sub-step 2.
    if not fade_done(0) then
        survivor_logo(0x80, 0x80)
        survivor_rows()
        s.sleeping = true
        return
    end
    fade_set(0x200, 0)
    fade_adjust(0x7FFF)
    s.sub2 = 3
    s.sleeping = true
end

-- `FUN_00505670` (`title_mv_state` 4). A skip on the logo's last tick
-- leaves `Ti_Disp_add` at 2, which the original's two-entry table sends
-- into EX.BATTLE's title; that isn't ported, and the title holds.
local function survivor_main(api)
    if s.disp == 0 then
        survivor_build()
    elseif s.disp == 1 then
        survivor_menu(api)
    end
end

-- ---------------------------------------------------------------- the screen

-- ---------------------------------------------------------------- the boot

-- The boot before the title. `Init_main` (0x4C4000) starts task 1 on
-- `Load_disclaimer` (0x4C4FF0), which shows the warning's picture
-- (`gw2.adt`) under a fade in and says so (`0x98F1B9` at 1); the main task
-- then goes on to `Capcom_logo` (0x503680), which loads the core bank,
-- sleeps 0x78 ticks and asks the warning to go (2); it fades out and ends
-- (0), and `Capcom_logo` plays the title's film and hands over to
-- `Title_init`: 189 ticks in all.
local BOOT_HOLD = 0x78

-- Task 1, `Load_disclaimer`, a tick: each step falls into the next.
local function boot_warning()
    local b = s.boot
    if b.task == 0 then
        -- `BYTE_0098F1B7`: the picture is drawn from this tick on.
        s.background = s.warning
        fade_set(0x200, -0x400)
        b.task = 4
    end
    if b.task == 4 then
        if not fade_done(0) then
            return
        end
        b.flag = 1
        b.task = 5
    end
    if b.task == 5 then
        if b.flag ~= 2 then
            return
        end
        fade_set(0x200, 0x400)
        b.task = 6
    end
    if b.task == 6 and fade_done(0) then
        s.background = nil
        b.flag = 0
        b.task = nil
    end
end

-- The title's film (`Init_movie_work(0)`): Leon's or Claire's, by the
-- title's generator.
local function boot_film(api)
    return { id = 0, claire = title_rnd(api, 2) == 1 }
end

-- `Capcom_logo`'s end: the title's task starts at `Title_init` on the
-- next tick. `Title_bg_reload`'s picture is first drawn then.
local function boot_end()
    s.frame = "black"
    s.mv = 1
    s.sub = 0
    s.boot = nil
end

-- Task 0 through the boot, a tick: `Init_main`'s steps, then
-- `Capcom_logo`'s (`s.sub`), with `Task_sleep` as `b.sleep`.
local function boot_main(api)
    local b = s.boot
    b.sleep = b.sleep - 1
    if b.sleep > 0 then
        return
    end
    b.sleep = 1
    if b.step == 0 then
        s.init = 1
        b.step = 1
    elseif b.step == 1 or b.step == 2 then
        if b.step == 1 then
            b.flag = 0
            b.task = 0
            s.init = 2
            b.step = 2
        end
        if b.flag ~= 0 then
            -- The rest of `Init_main`: the fonts and the effects' sheets,
            -- and the main task is the title's from the next tick, come
            -- from the logo.
            s.fg_system = s.fg_system | FG_FROM_LOGO
            b.step = 3
        end
    elseif b.step == 3 then
        -- The title's task's first tick: `Title`'s step 0 sets the pad's
        -- repeat (`Pad_rep_set`), after this tick's pad. Then
        -- `Capcom_logo`'s steps 0 and 4.
        s.rep_mask, s.rep_delay = 0x5000, 0x20A
        api:core_bank(0x10)
        s.mv = 0
        s.init = 1
        s.sub = 5
        b.step = 5
        b.sleep = BOOT_HOLD
    elseif b.step == 5 then
        b.flag = 2
        s.sub = 6
        b.step = 6
    elseif b.step == 8 then
        -- The film played: the task's first tick after it.
        boot_end()
    elseif b.step == 6 and s.fg_system & FG_FROM_LOGO == 0 then
        -- Step 9, not from the logo (after a game, or a demo a key
        -- ended): the title's picture, and no film.
        boot_end()
    else
        s.sub = 7
        b.step = 7
        if b.flag == 0 then
            -- The film plays on the boot and then after every sixth demo
            -- played to its end (`0x689F24` counts them).
            local films = api:kept("films") or 0
            api:keep("films", (films + 1) % 6)
            if films ~= 0 then
                boot_end()
                return
            end
            -- Step 8 waits for the film (`fg_system`'s 0x200), which is
            -- over on this tick in a run that plays none (`M.on_film`).
            s.film = boot_film(api)
            s.exit = "film"
            s.frame = "black"
            s.sub = 8
            b.step = 8
        end
    end
end

local function boot(api)
    boot_main(api)
    local b = s.boot
    if b and b.task then
        boot_warning()
    elseif b and b.acks and b.flag == 2 then
        -- Task 1 after a demo (`0x4C0820`) only answers the logo's wait.
        b.flag = 0
    end
end

-- The title after its attract demo (`Game_loop`'s `0x4BFE5D`): the title's
-- task starts from `Capcom_logo` again, with its statics as the hand-over
-- left them. A demo played to its end comes back as from the logo
-- (`fg_system`'s 2: the film on its turn, and the title's entry); one a
-- key ended comes back as a game does, to the title's picture.
local function demo_return(api, key)
    for _, name in ipairs(DEMO_KEPT) do
        s[name] = api:kept("demo_" .. name) or s[name]
    end
    s.fades[0].mode = 2
    s.background = nil
    s.init, s.sub = 0, 0
    s.fg_system = key and 0 or FG_FROM_LOGO
    s.boot = { step = 3, sleep = 1, flag = 0, acks = not key }
end

local function publish(api)
    Prologue.publish(api, s.pro)
    Gallery.publish(api, s.gal)
    if volumes_given then
        api:set("music_volume", sound.bgm)
        api:set("effects_volume", sound.se)
    end
    api:set("ti_mv", s.mv)
    api:set("ti_disp", s.disp)
    api:set("ti_init", s.init)
    api:set("ti_sub", s.sub)
    api:set("ti_menu", s.menu)
    api:set("ti_step", s.step)
    api:set("ti_sel", s.sel)
    api:set("ti_cur", s.cur)
    api:set("ti_pulse", s.pulse)
    api:set("ti_slide", s.slide)
    api:set("ti_demo", s.demo)
    api:set("ti_flash", s.flash)
    api:set("ti_logo", s.logo)
    api:set("ti_logo_count", s.logo_count)
    api:set("ti_fade_level", s.fades[0].level)
    api:set("ti_fade_add", s.fades[0].add)
    api:set("ti_fade_mode", s.fades[0].mode)
    for _, n in ipairs({ 2, 3 }) do
        api:set("ti_fade" .. n .. "_level", s.fades[n].level)
        api:set("ti_fade" .. n .. "_add", s.fades[n].add)
        api:set("ti_fade" .. n .. "_mode", s.fades[n].mode)
    end
    -- The trace's `tx`: task 0's +0xA, the page bytes, the slide's two
    -- counts, EX.BATTLE's level page (not ported) and `fg_status`'s bits.
    api:set("ti_sub2", s.sub2)
    api:set("ti_page2", 0)
    api:set("ti_dim", s.dim)
    api:set("ti_b8c", s.b8c[0])
    api:set("ti_ef2", 0)
    api:set("ti_ef3", 0)
    -- EX.BATTLE's level (`0x989E91`), which the boot leaves at 2.
    api:set("ti_level", 2)
    api:set("ti_fgstatus", s.fg_status)
    api:set("ti_kido", s.kido)
    api:set("ti_add", s.add)
    api:set("ti_fgsys", s.fg_system)
    -- The frame limiter (`0x98F07A`): the title's own 60 ticks a second, or
    -- the game's 30 under the option screen and the load screen, until
    -- `Title_init` runs again.
    api:set("game_active", s.limiter == 2)
    api:set("ti_hard_mode", s.hard_mode)
    api:set("ti_censorship_off", s.censorship_off)
    -- The trace's `opt`, while the option task runs.
    local o = s.opt
    api:set("opt_on", o and 1 or 0)
    if o then
        api:set("opt_step", o.step)
        api:set("opt_state", o.state)
        api:set("opt_cursor", o.cursor)
        api:set("opt_type", o.type)
        api:set("opt_wait", o.wait)
        api:set("opt_speaker", sound.speaker)
        api:set("opt_quality", sound.quality)
        api:set("opt_bgm", sound.bgm)
        api:set("opt_se", sound.se)
        api:set("opt_blink_mode", o.blink.mode)
        api:set("opt_blink_quality", o.blink.quality)
        api:set("opt_blink_bgm", o.blink.bgm)
        api:set("opt_blink_se", o.blink.se)
        api:set("opt_blink_exit", o.blink.exit)
    end
end

local function any_saves(api, args)
    for _, entry in ipairs(api:saves()) do
        if entry.loadable and (args.folder_saves or entry.autosave) then
            return true
        end
    end
    if not args.folder_saves then
        for _, present in ipairs(api:save_slots()) do
            if present then
                return true
            end
        end
    end
    return false
end

function M.open(api, args)
    s = new_state()
    M.state = s
    option_restore(api)
    s.any_saves = any_saves(api, args or {})
    pages = {
        [0] = api:indexed_page("ui/title/tmoji_0"),
        api:indexed_page("ui/title/tmoji_1"),
        api:indexed_page("ui/title/tmoji_2"),
        [OPT_WORDS] = api:indexed_page("ui/option/moji1"),
        [OPT_PARTS] = api:indexed_page("ui/option/parts"),
    }
    s.title_background = api:picture("ui/title/title_bg")
    s.background = s.title_background
    Prologue.reset()
    Gallery.reset()
    s.prologue_art = api:has_image("ui/prologue/open10")
    -- The title starts at `Title_init`, or, from the boot, 189 ticks before
    -- it with the warning (`M.on_boot`).
    s.mv = 1
    s.init = 1
    -- From the boot, the white branch of `Title_init`; the new-game choice's
    -- entry (`ui/char_select.lua`) fades in and opens on ORIGINAL MODE's.
    s.fg_system = args and args.choice and 0 or FG_FROM_LOGO
    s.open_choice = args and args.choice
    s.rep_mask, s.rep_delay = 0x5000, 0x20A
    s.seed = args and args.seed
    if args and args.demo then
        demo_return(api, args.demo == "key")
    end
    -- config.ini's SPECIAL, read as the signed byte it is.
    local special = args and args.special or 15
    s.special = special >= 0x80 and special - 0x100 or special
    -- config.ini's GALLERY0 to GALLERY4, what play has opened of the
    -- gallery: all of it without them.
    if args and args.gallery0 then
        s.gallery = {}
        for n = 0, 4 do
            s.gallery[n + 1] = args["gallery" .. n] or 0
        end
    end
    moji_set_work()
    publish(api)
end

function M.update(api, input)
    local cues = {}
    s.cue = function(slot)
        cues[#cues + 1] = slot
    end
    s.sprites = {}
    s.bands = nil
    s.sleeping = false
    s.opt_texts = {}
    -- What the frame shows besides the tick's draw: "black" on a tick with
    -- no draw at all, "held" while the last presented frame stays up.
    s.frame = nil
    local shown = {}
    for n = 0, 3 do
        shown[n] = s.drawn_fades[n]
    end
    pad_set(input)
    if s.boot then
        boot(api)
    elseif s.game_init then
        game_init(api, input)
    elseif s.opt then
        -- The option task's ticks: the title's task sleeps.
        option_task(api)
    elseif s.gal then
        -- The gallery's ticks: the title's task sleeps, through the tick
        -- the gallery closes on, which shows the title's picture alone.
        Gallery.tick(api, s.gal, s.edge, s.raw, s.cue)
        s.sleeping = true
        if s.gal.film then
            -- A movie's film: the gallery runs on once it is over.
            s.film = { id = s.gal.film, claire = s.claire, gallery = true }
            s.exit = "film"
        end
        if s.gal.task == 0 then
            s.gal = nil
        end
    elseif s.mv == 1 then
        title_init()
    elseif s.mv == 2 then
        title_main(api)
    elseif s.mv == 3 then
        survivor_load(api)
    elseif s.mv == 4 then
        survivor_main(api)
    end
    if not s.sleeping and (s.mv == 2 or s.mv == 3) then
        title_image()
    end
    -- OPTION's confirm starts the option task on its own tick.
    if s.opt and s.opt.step == 0 then
        option_task(api)
    end
    -- `System_trans`: each fade is drawn at its level, then steps.
    for n = 0, 3 do
        local fade = s.fades[n]
        if fade.level >= 0 then
            s.drawn_fades[n] = { level = fade.level, mode = fade.mode }
            fade.level = i16(fade.level + fade.add)
        else
            s.drawn_fades[n] = nil
        end
    end
    if s.frame == "held" then
        for n = 0, 3 do
            s.drawn_fades[n] = shown[n]
        end
    end
    for _, slot in ipairs(cues) do
        api:core_sound(slot)
    end
    publish(api)
    if s.exit then
        local exit = s.exit
        s.exit = nil
        if exit == "new_game" then
            api:exit("new_game", s.character or 0, s.choice)
        elseif exit == "attract" then
            for _, name in ipairs(DEMO_KEPT) do
                api:keep("demo_" .. name, s[name])
            end
            api:exit("attract", s.reel)
        elseif exit == "film" then
            -- The boot's film, the gallery's, or the one the prologue asks
            -- for.
            local film = s.film or { id = s.pro.film, claire = s.claire }
            s.film = nil
            Gallery.open_film(s.gallery, film.claire, film.id)
            api:exit("film", film.id, { claire = film.claire, gallery = film.gallery })
        else
            api:exit(exit)
        end
    end
end

-- The load screen came back: the title's task, asleep under it as it
-- stood, wakes on its next tick (`Title_main_load`'s second step). The
-- screen leaves fade 3 off in the mode `Cardaccess_init` set it to, and,
-- without a load, `fg_system`'s 0x400 up for that step to find and clear.
local function resume(api, loaded)
    local fade = s.fades[3]
    fade.level, fade.add, fade.mode = -1, 0, 1
    s.loaded = loaded
    if not loaded then
        s.fg_system = s.fg_system | FG_NO_LOAD
    end
    publish(api)
end

function M.on_resume(api)
    resume(api, false)
end

function M.on_loaded(api)
    resume(api, true)
end

-- arklay's music and effects volumes, in percent, as the title opens and
-- whenever the settings change: the SOUND page's two volumes are these,
-- held to the page's 1 to 100, and the engine takes a change of the page's
-- from what is published.
function M.on_volumes(api, music, effects)
    volumes_given = true
    sound.bgm = option_volume_range(music)
    sound.se = option_volume_range(effects)
    api:set("music_volume", sound.bgm)
    api:set("effects_volume", sound.se)
end

-- The title is the boot's first: it starts with `Init_main`, before the
-- warning ("the boot").
function M.on_boot(api)
    s.warning = api:has_image("ui/title/warning") and api:picture("ui/title/warning") or nil
    s.background = nil
    s.mv = 0
    s.init = 0
    s.fg_system = 0
    -- No key repeats until the title's task sets its own.
    s.rep_mask, s.rep_delay = 0, 0
    s.boot = { step = 0, sleep = 1, flag = 0 }
    -- The process starts: the generator has no value yet, the film plays
    -- and the demos start from their first.
    api:keep("rnd", nil)
    api:keep("films", nil)
    api:keep("demo_cycle", nil)
    publish(api)
end

-- The film the title left for is over, or never played. The prologue's
-- sequence finds that on its next tick. The boot's `Capcom_logo` ends on
-- the tick it asked when the film never played, and on its next tick
-- after one that did.
function M.on_film(api, skipped, played)
    if played then
        -- `Pad_set` ran through the film: a direction pressed under it
        -- and still held is no edge on the tick the title wakes.
        s.raw = nil
    end
    if s.boot then
        if not played then
            boot_end()
            publish(api)
        end
    elseif s.pro then
        Prologue.film_over(s.pro, skipped)
    end
end

-- A palette channel as the original's renderer expands the sheets' 5-bit
-- colours: 8v + 7 (arklay's decoder gives v * 255 / 31).
local function expand(c)
    local v = (c * 31 + 127) // 255
    return v == 0 and 0 or 8 * v + 7
end

-- A sheet rect through the page's palette at the sprite's colour: one
-- level for the three channels, or the option screen's own three.
local function lut_for(page, colour, tint)
    local cr, cg, cb = colour, colour, colour
    if tint then
        -- A channel past 0x80 is the sheet's own colour, no brighter.
        local function level(c)
            return c > 0x80 and 0x80 or c
        end
        cr, cg, cb = level(tint[1]), level(tint[2]), level(tint[3])
    end
    local key = (cr << 16) | (cg << 8) | cb
    page.luts = page.luts or {}
    local lut = page.luts[key]
    if lut then
        return lut
    end
    lut = {}
    for index = 1, 255 do
        local rgb = page.palette[index]
        if rgb and rgb ~= 0 then
            local r = (expand((rgb >> 16) & 0xFF) * cr) >> 7
            local g = (expand((rgb >> 8) & 0xFF) * cg) >> 7
            local b = (expand(rgb & 0xFF) * cb) >> 7
            r = r > 255 and 255 or r
            g = g > 255 and 255 or g
            b = b > 255 and 255 or b
            lut[index] = (r << 16) | (g << 8) | b
        end
    end
    page.luts[key] = lut
    return lut
end

local BLENDS = { [0] = "half", "add", "sub", "quarter" }

function M.draw(api)
    api:fill_rect(0, 0, api:width(), api:height(), 0, 0, 0)
    local ox, oy = api:width() // 2 - 160, api:height() // 2 - 120
    if s.frame == "black" then
        return
    end
    if s.gal and s.gal.ticked then
        Gallery.draw(api, s.gal, ox, oy)
        return
    end
    if s.background then
        api:draw_background(s.background, ox, oy)
    end
    local function fade(n)
        local f = s.drawn_fades[n]
        if f then
            local level = (f.level >> 7) & 0xFF
            if level ~= 0 then
                api:fade_tile(f.mode, level)
            end
        end
    end
    if s.pro and not s.pro.done then
        Prologue.draw(api, s.pro, ox, oy, fade)
        return
    end
    -- Slot 3 goes on the back list (`AddTile`'s `is_back`): over the
    -- background, under every sprite.
    fade(3)
    local cache = M.cache or {}
    M.cache = cache
    for k = 0, OPT_PARTS do
        if pages[k] and (not cache[k] or cache[k].page ~= pages[k]) then
            cache[k] = { page = pages[k], palette = api:page_palette(pages[k], 0) or {} }
        end
    end
    -- By depth, the farthest first: the 4th Survivor's name (12) and logo
    -- (11), the words (10), then the title's logo (6). A depth's sprites go
    -- last-added first, as `AddPrimitiveFront` lists them.
    local depths = {}
    for _, sp in ipairs(s.sprites) do
        depths[sp.depth or (sp.logo and 6 or 10)] = true
    end
    local order = {}
    for depth in pairs(depths) do
        order[#order + 1] = depth
    end
    table.sort(order, function(a, b)
        return a > b
    end)
    for _, depth in ipairs(order) do
        for i = #s.sprites, 1, -1 do
            local sp = s.sprites[i]
            if (sp.depth or (sp.logo and 6 or 10)) == depth and pages[sp.page] then
                local colour = sp.colour or 0x80
                local lut = lut_for(cache[sp.page], colour, sp.rgb)
                local options = nil
                if sp.blend then
                    options = { blend = BLENDS[sp.blend] }
                end
                api:page_lut(pages[sp.page], sp.u, sp.v, sp.w, sp.h, ox + sp.x, oy + sp.y, lut, false, options)
            end
        end
    end
    -- The option screen's sound page: the names and the values' words in
    -- Classic REbirth's print, the digits in their cells, and the arrows
    -- about a word.
    for _, t in ipairs(s.opt_texts or {}) do
        local parts = cache[OPT_PARTS]
        if t.cell then
            local x = M.option_digit_x(t.cell, t.text, api:text_width(t.text), CrText.by_table(api))
            api:text(ox + x, oy + t.y, t.text, { colour = t.on and OPT_DIGIT_GREEN or nil })
        else
            -- The print starts a pixel before the sheet's word did.
            local x = t.x or OPT_VALUE_END - CrText.width(api, t.text)
            CrText.print(api, ox + x - 1, oy + t.y, t.text, t.on and OPT_TEXT_GREEN or nil)
            if t.arrows and parts then
                local lut = lut_for(parts, 0x80, OPT_GREEN)
                api:page_lut(parts.page, 0xD1, 199, 14, 14, ox + x - 14, oy + t.y, lut, false)
                api:page_lut(parts.page, 0xDF, 199, 14, 14, ox + OPT_VALUE_END, oy + t.y, lut, false)
            end
        end
    end
    -- Slots 0 to 2 in front, slot 0 on top.
    fade(2)
    fade(1)
    fade(0)
    if s.bands then
        api:fade_tile(2, s.bands, ox, oy, 320, 40)
        api:fade_tile(2, s.bands, ox, oy + 200, 320, 40)
    end
end

return M
