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
-- (the title starts again) and whose EXIT goes back to `Title_init`, and
-- EX.BATTLE's and GALLERY's confirms do nothing; OPTION fades out and, arklay having no option
-- screen, the title starts again (`docs/internals/deviations/re2.md`, "The title"). An
-- idle title fades into the attract demo after 0x708 ticks.
--
-- args.folder_saves: a registered save screen lists the whole save folder.

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
local MENU_ROWS, MENU_OUT = 0, 4

-- `fg_system`'s bits the title reads or sets.
local FG_REPEAT = 0x80000000
local FG_ARRANGE = 0x40
local FG_FROM_LOGO = 0x2
local FG_EASY = 0x20
local FG_NEW_GAME = 0x20000

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
        -- `Pad_set`'s state.
        raw = 0,
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
    if input.held_up then
        raw = raw | PAD_UP
    end
    if input.held_down then
        raw = raw | PAD_DOWN
    end
    if input.held_left then
        raw = raw | PAD_LEFT
    end
    if input.held_right then
        raw = raw | PAD_RIGHT
    end
    if input.start then
        raw = raw | PAD_START
    end
    s.edge = (s.raw ~ raw) & raw
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
-- mode's title (`title_mv_state` 3). EX.BATTLE's and GALLERY's confirms
-- aren't ported and do nothing.
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

-- The choices below the rows.
local function sub_menu(api)
    if s.menu == 5 or s.menu == 6 then
        character_menu()
    elseif s.menu == 1 or s.menu == 2 then
        difficulty_menu()
    elseif s.menu == 3 then
        special()
    end
end

-- ---------------------------------------------------------------- the steps

-- `Title_init` (0x5038B0). The boot reaches it with `fg_system` bit 2 set
-- (the warning screen's `Capcom_logo` sets it), so it takes that branch and
-- clears the bit: the screen white (fade slot 0 added at 0x7F80), the logo
-- off the screen, and step 5 next. Any later entry (back from OPTION) fades
-- in from black over 31 ticks (`Ti_kido` from 0x7C00 down by 0x400).
local function title_init()
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
            -- The demo's pad file is loaded and step 3 starts it.
            fade_set(0x200, 0)
            fade_adjust(0x7FFF)
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
        s.exit = "save_load"
    elseif s.disp == 3 and s.fg_system & 8 ~= 0 then
        -- The 4th Survivor's or To-fu's new game (`Title_game_init`'s
        -- extras branch) isn't ported: the title starts again from
        -- `Title_init`, as after OPTION (docs/internals/deviations/re2.md,
        -- "The title").
        s.fg_system = s.fg_system & ~(9 | FG_NEW_GAME)
        init_global()
        api:core_bank(0x10)
        s.mv = 1
    elseif s.disp == 3 then
        s.exit = s.attract and "attract" or "new_game"
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
        -- OPTION: step 4 starts the option task (`0x44A040`) and leaves the
        -- title to start again from `Title_init` once it ends. arklay has no
        -- option screen, so the title starts again on the next tick.
        s.disp = 1
        s.mv = 1
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

local function publish(api)
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
    api:set("ti_hard_mode", s.hard_mode)
    api:set("ti_censorship_off", s.censorship_off)
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
    s.any_saves = any_saves(api, args or {})
    pages = {
        [0] = api:indexed_page("ui/title/tmoji_0"),
        api:indexed_page("ui/title/tmoji_1"),
        api:indexed_page("ui/title/tmoji_2"),
    }
    s.background = api:picture("ui/title/title_bg")
    -- The title starts at `Title_init`: the boot's logos and warning before
    -- it aren't ported.
    s.mv = 1
    s.init = 1
    -- From the boot, the white branch of `Title_init`; the new-game choice's
    -- entry (`ui/char_select.lua`) fades in and opens on ORIGINAL MODE's.
    s.fg_system = args and args.choice and 0 or FG_FROM_LOGO
    s.open_choice = args and args.choice
    s.rep_mask, s.rep_delay = 0x5000, 0x20A
    -- config.ini's SPECIAL, read as the signed byte it is.
    local special = args and args.special or 15
    s.special = special >= 0x80 and special - 0x100 or special
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
    pad_set(input)
    if s.mv == 1 then
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
    for _, slot in ipairs(cues) do
        api:core_sound(slot)
    end
    publish(api)
    if s.exit then
        local exit = s.exit
        s.exit = nil
        if exit == "new_game" then
            api:exit("new_game", s.character or 0, s.choice)
        else
            api:exit(exit)
        end
    end
end

-- A palette channel as the original's renderer expands the sheets' 5-bit
-- colours: 8v + 7 (arklay's decoder gives v * 255 / 31).
local function expand(c)
    local v = (c * 31 + 127) // 255
    return v == 0 and 0 or 8 * v + 7
end

-- A sheet rect through the page's palette at the sprite's colour.
local function lut_for(page, colour)
    local key = colour
    page.luts = page.luts or {}
    local lut = page.luts[key]
    if lut then
        return lut
    end
    lut = {}
    for index = 1, 255 do
        local rgb = page.palette[index]
        if rgb and rgb ~= 0 then
            local r = (expand((rgb >> 16) & 0xFF) * colour) >> 7
            local g = (expand((rgb >> 8) & 0xFF) * colour) >> 7
            local b = (expand(rgb & 0xFF) * colour) >> 7
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
    -- Slot 3 goes on the back list (`AddTile`'s `is_back`): over the
    -- background, under every sprite.
    fade(3)
    local cache = M.cache or {}
    M.cache = cache
    for k = 0, 3 do
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
                local lut = lut_for(cache[sp.page], colour)
                local options = nil
                if sp.blend then
                    options = { blend = BLENDS[sp.blend] }
                end
                api:page_lut(pages[sp.page], sp.u, sp.v, sp.w, sp.h, ox + sp.x, oy + sp.y, lut, false, options)
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
