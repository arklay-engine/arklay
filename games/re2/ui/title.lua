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
-- new-game choice (`ui/char_select.lua`); SPECIAL slides up to its extras,
-- whose modes aren't ported; OPTION fades out and, arklay having no option
-- screen, the title starts again (`docs/re2-deviations.md`, "The title"). An
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
        -- `Fade_set`'s slot 0: the level, its step and the blend.
        fade = { level = -1, add = 0, mode = 0 },
        -- The fade the frame draws: `System_trans` takes the level before
        -- this tick's step.
        drawn_fade = nil,
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

local function fade_set(a0, add)
    s.fade.mode = (a0 >> 8) & 3
    s.fade.add = i16(add)
    s.fade.level = add < 0 and i16(add + 0x8000) or 0
end

local function fade_adjust(level)
    s.fade.level = i16(level)
end

local function fade_done()
    return s.fade.level < 0
end

-- ---------------------------------------------------------------- the words

local function moji_set_work()
    for i = 0, #MOJI do
        local m = MOJI[i]
        s.work[i] = { r = 0x80, y = m[2] }
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

-- `Moji_Sel` (0x504000).
local function moji_sel()
    moji_license()
    moji_select_mode(s.cur, s.cur, 0, 0)
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

-- SPECIAL (`0x504900`) as `Special` 15 has it (the executable's start
-- value, every extra open): its row slides up by 37 over the others' fade,
-- and the five choices below it take the directions. EXIT and the cancel
-- slide back to the rows; the extras and the gallery (the confirm's step 4
-- and page 7) aren't ported, and their confirm does nothing.
local function special()
    local stepped = s.step
    if s.step == 0 then
        s.cur = 0
        slide_start()
        slide_in()
    elseif s.step == 1 then
        slide_in()
    elseif s.step == 2 then
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
        pulse()
        if s.key_trg & KEY_CANCEL ~= 0 then
            s.cur = SPECIAL_EXIT
            s.cue(SE_CANCEL)
            s.step = 3
        elseif confirm() and s.cur == SPECIAL_EXIT then
            s.cue(SE_CANCEL)
            s.step = 3
        end
    elseif s.step == 3 then
        if slide_out() then
            s.cur = SPECIAL_EXIT
            s.menu = MENU_ROWS
            s.step = 0
        end
    end
    if stepped == 2 then
        for row = 0, 4 do
            local r = SPECIAL_ROWS[row]
            draw_word_at(r[1], row == s.cur and u8(s.pulse + 0x20) or 0x20, r[2], r[3])
        end
    else
        -- While sliding, the draw's case is a register the caller left (ECX,
        -- which `Fade_status(0)` leaves 0): case 0, EXIT alone at 120, 160.
        draw_word_at(12, u8(s.pulse + 0x20), 120, 160)
    end
    moji_select_mode(3, 3, 0x25, s.slide_flag)
end

-- The choices below the rows. ORIGINAL MODE and ARRANGE MODE lead to the
-- new-game choice (`ui/char_select.lua` in arklay), which takes over from
-- the title.
local function sub_menu(api)
    if s.menu == 5 or s.menu == 6 then
        s.exit = "char_select"
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
    elseif s.disp == 3 then
        s.exit = s.attract and "attract" or "new_game"
    elseif s.disp == 4 then
        -- OPTION: step 4 starts the option task (`0x44A040`) and leaves the
        -- title to start again from `Title_init` once it ends. arklay has no
        -- option screen, so the title starts again on the next tick.
        s.disp = 1
        s.mv = 1
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
    api:set("ti_fade_level", s.fade.level)
    api:set("ti_fade_add", s.fade.add)
    api:set("ti_fade_mode", s.fade.mode)
    api:set("ti_kido", s.kido)
    api:set("ti_add", s.add)
    api:set("ti_fgsys", s.fg_system)
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
    s.fg_system = FG_FROM_LOGO
    s.rep_mask, s.rep_delay = 0x5000, 0x20A
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
    end
    if not s.sleeping and (s.mv == 2 or s.mv == 3) then
        title_image()
    end
    -- `System_trans`: the fade is drawn at its level, then steps.
    if s.fade.level >= 0 then
        s.drawn_fade = { level = s.fade.level, mode = s.fade.mode }
        s.fade.level = i16(s.fade.level + s.fade.add)
    else
        s.drawn_fade = nil
    end
    for _, slot in ipairs(cues) do
        api:core_sound(slot)
    end
    publish(api)
    if s.exit then
        local exit = s.exit
        s.exit = nil
        if exit == "new_game" then
            api:exit("new_game", s.character or 0)
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
    local cache = M.cache or {}
    M.cache = cache
    for k = 0, 2 do
        if pages[k] and not cache[k] then
            cache[k] = { palette = api:page_palette(pages[k], 0) or {} }
        end
    end
    -- The words (depth 10) behind the logo (depth 6).
    for pass = 1, 2 do
        for i = #s.sprites, 1, -1 do
            local sp = s.sprites[i]
            if (pass == 1) == (not sp.logo) and pages[sp.page] then
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
    local f = s.drawn_fade
    if f then
        local level = (f.level >> 7) & 0xFF
        if level ~= 0 then
            api:fade_tile(f.mode, level)
        end
    end
    if s.bands then
        api:fade_tile(2, s.bands, ox, oy, 320, 40)
        api:fade_tile(2, s.bands, ox, oy + 200, 320, 40)
    end
end

return M
