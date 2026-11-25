-- Extreme Battle's character choice (`ExBattle`, 0x4EA320), task 1 under
-- `Title_game_init`'s step 1 while the title's task sleeps: four players
-- on a turning stage, Ada, Leon, Claire and Chris (slots 0 to 3, players
-- 0xE, 0, 1 and 0xB), with the chosen one's name and start items typed
-- out below them. The title's script runs it (`ui/title.lua`) with its
-- own fades and cues.
--
-- The task's routine is `0x694260`'s low byte, through `funcs_4EA35E`:
-- 0 `ExBattle_init` (0x4EA390), 1 `ExBattle_fade_in` (0x4EB930), 2
-- `ExBattle_02` (0x4EAD00, the keys), 3 `ExBattle_03` (0x4EADD0, the
-- confirm's flash), 4 `ExBattle_fade_out` (0x4EB960) and 5
-- `ExBattle_exit` (0x4EAE80). Every tick that doesn't sleep in its routine
-- runs `ExPl_animate`, `ExPl_move` (the stage's draw) and the text
-- (`FUN_004EBDB0`) after it.
--
-- What isn't drawn: the four players' models (`Ex_load_pld`'s
-- `PL<id>CH.PLD` and their weapons, `ExPl_animate`'s motions and
-- `ExPl_move`'s lights and sort). Their places and turns are run, and the
-- trace's `exb` has them.

local M = {}

-- Pages the title loads for the choice (`ExBattle_load`): `SELECT_W.TIM`
-- as texture 0x18 (the frame, the arrows and the font) and `Select_i.tim`
-- as 0x19 (the start items' pictures).
M.PAGE_W, M.PAGE_I = 7, 8

-- `Snd_se_on`'s cues from core bank 0x15: the cursor, the decide and the
-- text's.
local SE_CURSOR, SE_DECIDE, SE_TEXT = 4, 6, 0x10

-- Raw pad bits (`Pad_set`'s held word, `0x9885FC`).
local PAD_LEFT, PAD_RIGHT = 0x8000, 0x2000

-- The players' slots (`0x53B538`, 0x14 bytes each): x, z, `Cdir_y` and
-- `Id` (y and the other two angles are 0), and `L_pl` by slot
-- (`FUN_004EA990`).
local SLOTS = {
    [0] = { x = 0, z = -2250, cdir = 0, id = 0xE, l_pl = 0x800 },
    { x = 2250, z = 0, cdir = 3584, id = 0x0, l_pl = 0x400 },
    { x = 0, z = 2250, cdir = 3904, id = 0x1, l_pl = 0 },
    { x = -2250, z = 0, cdir = 2048, id = 0xB, l_pl = 0xC00 },
}

-- `FUN_004EB9A0`'s targets for a cursor: each slot's place on the circle
-- (`0x53C2E0`, `+0x1F4`) and its facing (`0x53C320`, `+0x15E`).
local TURNS = {
    [0] = { 1024, 0, 3072, 2048 },
    { 2048, 1024, 0, 3072 },
    { 3072, 2048, 1024, 0 },
    { 0, 3072, 2048, 1024 },
}
local FACINGS = {
    [0] = { 3584, 3904, 2048, 0 },
    { 0, 3584, 3904, 2048 },
    { 2048, 0, 3584, 3904 },
    { 3904, 2048, 0, 3584 },
}
-- The circle's radius (`FUN_004EBA10`).
local RADIUS = 0x8CA

-- The text, by the cursor (`aAdaWong`, 0xD2 bytes a character, 0x1E a
-- line): the name, then the start items. `[nnn]` waits nnn ticks, `]` ends
-- the line. Ada's and Chris's seventh line is skipped.
local TEXT = {
    [0] = { "Ada Wong]", "[007Colt.S.A.A.]", "[005MAC11]", "[005Bow Gun]", "[005Ink Ribbon]", "[005Mixed Herb", " " },
    { "Leon S.Kennedy]", "[007H&K VP70.]", "[005Reminton M1100-P.]", "[005Desert Eagle 50A.E.]", "[005Ink Ribbon]", "[005F.Aid Spray]", "[005Blue Herb" },
    { "Claire Redfield]", "[007M79 Grenade Launcher", "[005Grenade Flame Rounds", "[005Grenade Acid Rounds]", "[005Ink Ribbon]", "[005F.Aid Spray]", "[005Blue Herb" },
    { "Chris Redfield]", "[007Beretta-M92FS.]", "[005Remington M1100.]", "[005Rocket Launcher]", "[005Ink Ribbon]", "[005F.Aid Spray", " " },
}
local COLUMNS, ROWS = 20, 7

-- The frame (`0x53B4A8`, records 0 to 7, opaque) and the two arrows
-- (records 8 and 9, added at the pulse's level): x, y, w, h, u, v.
local FRAME = {
    { 0, 159, 256, 9, 0, 176 },
    { 256, 159, 64, 9, 64, 144 },
    { 0, 0, 256, 24, 0, 120 },
    { 256, 0, 64, 24, 0, 144 },
    { 0, 162, 256, 80, 0, 176 },
    { 256, 162, 64, 24, 64, 144 },
    { 256, 186, 64, 24, 128, 144 },
    { 256, 210, 64, 32, 192, 144 },
}
local ARROWS = { { 115, 10, 20, 240, 30 }, { 115, 10, 20, 240, 50 } }
-- The arrows' x by the cursor (`0x53C3B8`).
local ARROW_X = { [0] = { 65, 155 }, { 40, 170 }, { 60, 160 }, { 40, 170 } }

-- The start items' pictures (`FUN_004EB280`): six places (`0x53C3A0`),
-- the pictures by the cursor (`0x53C3C8`, 0xFF none) and each picture's
-- size and place in the sheet (`0x53C340`).
local ICON_AT = { { 184, 176 }, { 224, 176 }, { 264, 176 }, { 184, 206 }, { 224, 206 }, { 264, 206 } }
local ICONS = {
    [0] = { 4, 13, 0xFF, 6, 12, 15 },
    { 0, 5, 7, 12, 11, 14 },
    { 3, 9, 10, 12, 11, 14 },
    { 8, 2, 12, 1, 0xFF, 11 },
}
local ICON_UV = {
    [0] = { 40, 30, 0, 30 },
    { 80, 30, 40, 30 },
    { 40, 30, 120, 30 },
    { 40, 30, 160, 30 },
    { 40, 30, 200, 30 },
    { 40, 30, 0, 60 },
    { 40, 30, 40, 60 },
    { 40, 30, 80, 60 },
    { 40, 30, 120, 60 },
    { 40, 30, 160, 60 },
    { 40, 30, 200, 60 },
    { 40, 30, 0, 90 },
}
-- Pictures past the table's twelve read on into the bytes after it; none
-- of the cursors names one but 13, 14 and 15, whose records follow.
ICON_UV[12] = { 40, 30, 40, 90 }
ICON_UV[13] = { 40, 30, 80, 90 }
ICON_UV[14] = { 40, 30, 120, 90 }
ICON_UV[15] = { 40, 30, 160, 90 }

-- The font's cells (`FUN_004EC080`): 8 by 10 from the sheet's top left,
-- 32 to a row from the space, and the caret's cell.
local CELL_W, CELL_H = 8, 10
local CARET_U, CARET_V = 0xE0, 0x0A

local function u8(v)
    return v & 0xFF
end

local function i16(v)
    v = v & 0xFFFF
    return v >= 0x8000 and v - 0x10000 or v
end

-- The text's work (`DWORD_006949EC`): the cells, the column and row, the
-- caret's count (+3), the wait (+0x11C) and the line in hand (+0x120,
-- +0x124: the line and the place in it).
local function new_text()
    -- The wait isn't part of `FUN_004EC010`'s clear: its first value is
    -- what the work's memory held, 50 in every recording.
    local t = { col = 0, row = 0, caret = 0, wait = 50, line = nil, at = 1, cells = {}, caret_on = 0 }
    for i = 0, COLUMNS * ROWS - 1 do
        t.cells[i] = 0x20
    end
    return t
end

-- `Task_execute(1, ExBattle)`: the task, which runs from the tick it is
-- started on. `fx` holds the title's fades (`set`, `adjust`, `done`) and
-- its cue.
function M.new(fx)
    return {
        fx = fx,
        task8 = 0,
        task9 = 0,
        routine = 0,
        text_step = 0,
        cursor = 0,
        pulse = 0,
        way = 0,
        flags = 0,
        done = 0,
        text = nil,
        works = {},
        sprites = {},
        exit = false,
        ended = false,
    }
end

-- `FUN_004EB9A0`: a move's targets for every player, the text from its
-- start, and the turn's flag.
local function turn_to(c)
    for slot = 0, 3 do
        local w = c.works[slot]
        w.target = TURNS[c.cursor][slot + 1]
        w.step = 0
        w.facing = FACINGS[c.cursor][slot + 1]
    end
    c.text_step = 0
    c.flags = c.flags | 1
end

-- `FUN_004EBA10`: each player a step round the circle (0x40, the near way
-- through 0x1000) and a sixteenth of its turn, and the flag down once one
-- is there.
local function turn(c, api)
    for slot = 0, 3 do
        local w = c.works[slot]
        if w.angle == w.target then
            if w.angle == 0x1000 then
                w.angle = 0
                w.target = 0
            end
            c.flags = c.flags & 0xFE
        else
            if w.angle > 0xBFF and w.target == 0 then
                w.target = 0x1000
            end
            if w.angle == 0 and w.target > 0xBFF then
                w.angle = 0x1000
            end
            if w.angle < w.target then
                w.angle = w.angle + 0x40
            else
                w.angle = w.angle - 0x40
            end
            -- `>> 12`, the arithmetic shift: a floor.
            w.x = (api:rsin(w.angle) * RADIUS) // 4096
            w.z = (api:rcos(w.angle) * RADIUS) // 4096
        end
        if w.cdir == w.facing then
            if w.cdir == 0x1000 then
                w.cdir = 0
                w.facing = 0
            end
            w.step = 0
        else
            if w.facing == 0 and w.cdir > 3000 then
                w.facing = 0x1000
            end
            if w.cdir == 0 and w.facing > 3000 then
                w.cdir = 0x1000
            end
            if w.step == 0 then
                local d = w.facing - w.cdir
                -- C's division by 16, toward zero.
                w.step = i16(d >= 0 and d // 16 or -((-d) // 16))
            end
            w.cdir = i16(w.cdir + w.step)
        end
    end
end

-- `Ex_Mess_disp(1, line, &done)`: a character of the line a tick, its
-- cue with it; `[nnn` waits nnn ticks first; `]` ends the line, and the
-- end of the line raises `done`.
local function type_line(c, line)
    local t = c.text
    if c.done ~= 0 then
        return
    end
    if t.line ~= line then
        t.line = line
        t.at = 1
        t.wait = 1
    end
    t.wait = u8(t.wait - 1)
    if t.wait ~= 0 then
        return
    end
    t.wait = 1
    local wrote = false
    while true do
        local ch = line:byte(t.at)
        if ch == nil then
            t.line = nil
            c.done = 1
            return
        end
        if ch == 0x5B then
            -- `[`: the three digits, as the bytes' sum leaves them.
            local d1, d2, d3 = line:byte(t.at + 1), line:byte(t.at + 2), line:byte(t.at + 3)
            t.wait = u8(d1 * 100 + 0x40 + d2 * 10 + 0x20 + d3 - 0x30)
            t.at = t.at + 4
            return
        end
        if ch == 0x5D then
            t.col = 0
            t.row = t.row + 1
        else
            t.cells[t.col + t.row * COLUMNS] = ch
            wrote = true
            t.caret = 0x10
            t.col = t.col + 1
            if t.col > 0x13 then
                t.col = 0
                t.row = t.row + 1
            end
        end
        c.fx.cue(SE_TEXT)
        t.at = t.at + 1
        if wrote then
            return
        end
    end
end

-- `FUN_004EBDB0`: the text's steps, the name and the start items one
-- after the other, and its draw.
local function text(c)
    local step = c.text_step
    if step == 0 then
        -- `FUN_004EC010`: the cells cleared, the column and row at 0, the
        -- caret's count 0.
        local line, at, wait = c.text and c.text.line, c.text and c.text.at, c.text and c.text.wait
        c.text = new_text()
        if wait then
            c.text.line, c.text.at, c.text.wait = line, at, wait
        end
        c.text_step = 1
        c.done = 0
    elseif step >= 1 and step <= 7 then
        type_line(c, TEXT[c.cursor][step])
        if c.done ~= 0 then
            if step == 6 and (c.cursor == 3 or c.cursor == 0) then
                c.text_step = 8
            else
                c.text_step = step + 1
            end
            c.done = 0
        end
    elseif c.done ~= 0 then
        c.done = 0
    end
    -- `FUN_004EC080(1)`: the caret's count and its blink.
    local t = c.text
    local caret = u8(t.caret + 1)
    t.caret = caret & 0x1F
    t.caret_on = caret & 0x10
end

-- `ExBattle_init`: the music, the core bank, the sheets, the players in
-- their slots, the cursor on Leon and the fade in from black.
local function init(c, api)
    c.limiter = 2
    c.music = 0x39
    api:core_bank(0x15)
    c.load = true
    for slot = 0, 3 do
        local s = SLOTS[slot]
        c.works[slot] = {
            id = s.id,
            x = s.x,
            z = s.z,
            cdir = s.cdir,
            -- The work's turn, its target, its facing's target and step
            -- are no part of the init: the recordings have them as the
            -- cursor on Leon leaves them, standing.
            angle = TURNS[1][slot + 1],
            target = TURNS[1][slot + 1],
            facing = s.cdir,
            step = 0,
            r2 = 0,
            move_no = 0,
            move_cnt = 0,
            hokan = 0,
            mplay = 0,
        }
    end
    c.pulse = 0
    c.cursor = 1
    c.way = 0
    c.flags = 0
    c.fx.set(0x200, -0xC00)
    c.routine = 1
end

-- `ExBattle_02`: the confirm, or a sideways key while the four stand.
local function keys(c, api, input)
    if c.flags & 1 == 0 then
        if input.confirm then
            c.routine = 3
            c.fx.cue(SE_DECIDE)
            return
        end
        local skip = false
        if input.raw & PAD_LEFT ~= 0 then
            if (c.cursor == 1 and c.flags & 2 ~= 0) or (c.cursor == 0 and c.flags & 4 ~= 0) then
                skip = true
            else
                c.cursor = c.cursor - 1
                if c.cursor < 0 then
                    c.cursor = 3
                end
                turn_to(c)
                c.fx.cue(SE_CURSOR)
            end
        end
        if not skip and input.raw & PAD_RIGHT ~= 0 and (c.cursor ~= 2 or c.flags & 4 == 0) then
            c.cursor = c.cursor + 1
            if c.cursor > 3 then
                c.cursor = 0
            end
            turn_to(c)
            c.fx.cue(SE_CURSOR)
        end
    end
    turn(c, api)
end

-- `FUN_004EB120`'s draw: the frame, the arrows while the four stand (but
-- after the confirm), and the start items' pictures (`FUN_004EB280`).
local function draw(c)
    local sprites = {}
    c.sprites = sprites
    local function add(page, x, y, w, h, u, v, colour, blend, depth)
        sprites[#sprites + 1] = { page = page, x = x, y = y, w = w, h = h, u = u, v = v, colour = colour, blend = blend, depth = depth }
    end
    for _, f in ipairs(FRAME) do
        add(M.PAGE_W, f[1], f[2], f[3], f[4], f[5], f[6], 0x80, nil, 12)
    end
    if c.routine < 3 then
        if c.flags & 1 ~= 0 then
            c.pulse = 0
            c.way = 0
        else
            if c.way == 0 then
                c.pulse = u8(c.pulse + 8)
                if c.pulse == 0x80 then
                    c.way = 1
                end
            else
                c.pulse = u8(c.pulse - 8)
                if c.pulse == 0 then
                    c.way = 1 - c.way
                end
            end
            for n = 1, 2 do
                local barred
                if n == 1 then
                    barred = (c.cursor == 1 and c.flags & 2 ~= 0) or (c.cursor == 0 and c.flags & 4 ~= 0)
                else
                    barred = c.cursor == 2 and c.flags & 4 ~= 0
                end
                if not barred then
                    local a = ARROWS[n]
                    add(M.PAGE_W, ARROW_X[c.cursor][n], a[1], a[2], a[3], a[4], a[5], c.pulse, 1, 12)
                end
            end
        end
    end
    for k = 1, 6 do
        local icon = ICONS[c.cursor][k]
        if icon ~= 0xFF then
            local uv = ICON_UV[icon]
            add(M.PAGE_I, ICON_AT[k][1], ICON_AT[k][2], uv[1], uv[2], uv[3], uv[4], 0x80, nil, 10)
        end
    end
    -- The text (`FUN_004EC180`, depth 6, added): the caret, then the cells
    -- that aren't spaces, 8 by 10 from (14, 166).
    local t = c.text
    if t then
        if t.caret_on ~= 0 then
            add(M.PAGE_W, t.col * 8 + 0xE, t.row * 10 + 0xA8, CELL_W, CELL_H, CARET_U, CARET_V, 0x80, 1, 6)
        end
        for i = 0, COLUMNS * ROWS - 1 do
            local ch = t.cells[i]
            if ch ~= 0x20 then
                local n = ch - 0x20
                add(M.PAGE_W, 14 + (i % COLUMNS) * 8, 166 + (i // COLUMNS) * 10, CELL_W, CELL_H,
                    (n & 0x1F) * 8, (n >> 5) * 10, 0x80, 1, 6)
            end
        end
    end
end

-- `ExPl_animate`, `ExPl_move` and the text, after a routine that didn't
-- sleep.
local function after(c)
    for slot = 0, 3 do
        local w = c.works[slot]
        -- `ExPl_animate`'s first pass: motion 0 from its start.
        if w.r2 == 0 and w.id ~= 0xE then
            w.move_no, w.move_cnt, w.hokan, w.mplay = 0, 0, 0, 0
            w.r2 = 1
        end
    end
    draw(c)
    text(c)
end

-- One tick of the task. `input.confirm` is the confirm's edge (`Key_trg`
-- 0x1000 or START's), `input.raw` the held word.
function M.tick(api, c, input)
    if c.ended then
        return
    end
    c.music = nil
    c.load = false
    if c.task8 ~= 1 then
        c.routine = 0
        c.text_step = 0
        c.task8 = 1
    end
    local slept = false
    if c.routine == 0 then
        init(c, api)
    elseif c.routine == 1 then
        if c.fx.done(0) then
            c.fx.set(0x200, 0)
            c.routine = 2
        end
    elseif c.routine == 2 then
        keys(c, api, input)
    elseif c.routine == 3 then
        if c.task9 ~= 1 then
            local w = c.works[c.cursor]
            if c.cursor == 0 then
                w.move_no = 1
                w.r2 = 1
            else
                w.move_no = 2
                w.r2 = 3
            end
            c.fx.set(0x100, -0x400)
            c.task9 = 1
        end
        if not c.fx.done(0) then
            after(c)
            slept = true
        else
            c.fx.set(0x200, 0xC00)
            c.routine = 4
            c.task9 = 0
        end
    elseif c.routine == 4 then
        if c.fx.done(0) then
            c.fx.set(0x200, 0)
            c.fx.adjust(0x7FFF)
            c.routine = 5
        end
    elseif c.routine == 5 then
        -- `ExBattle_exit`: the music fades, the player is the cursor's,
        -- and the title's task wakes on the next tick.
        c.music = "fade"
        c.player = SLOTS[c.cursor].id
        c.ended = true
        c.sprites = {}
        return
    end
    if not slept then
        after(c)
    end
end

-- `FUN_004E3A20`, Extreme Battle's placement, from `Rnd`'s state `rnd`:
-- `VSync() & 0x1FF` draws first, none on the PC (its `VSync` returns 0),
-- then four of eight bits, each by a draw's low bit, the rest set high
-- once four can no longer be had. The bits (`DWORD_0098EBD0`'s low byte;
-- `BYTE_0098EE9C` takes their complement) and `Rnd`'s state after.
function M.placement(rnd)
    local function draw()
        local high = ((rnd * 2) >> 8) & 0xFF
        local low = (rnd + high) & 0xFF
        rnd = (high << 8) | low
        return low
    end
    local bits, count, n = 0, 0, 0
    while true do
        if draw() & 1 ~= 0 then
            bits = bits | (1 << n)
            count = count + 1
            if count == 4 then
                return bits, rnd
            end
        end
        n = n + 1
        if n - count > 3 then
            return (bits | (0xF0 << count)) & 0xFF, rnd
        end
        if n > 7 then
            return bits, rnd
        end
    end
end

-- The trace's `exb` and `exbm`, as `scripts/oracle-re2.py` has them.
function M.trace(c)
    if not c or c.ended then
        return nil, nil
    end
    local t = c.text
    local text = t and string.format("%d,%d,%d,%d", t.col, t.row, t.caret, t.wait) or "-"
    local players, motions = {}, {}
    for slot = 0, 3 do
        local w = c.works[slot]
        if w then
            players[#players + 1] = string.format("%d,%d,%d,%d,%d,%d,%d", w.angle, w.target, w.x, w.z,
                w.cdir & 0xFFFF, w.facing & 0xFFFF, w.step)
            motions[#motions + 1] = string.format("%d,%02x%02x%02x%02x", w.r2, w.move_no, w.move_cnt, w.hokan, w.mplay)
        end
    end
    local exb = string.format("%d,%d,%d,%d,%d,%d,%d,%d,%d/%s/%s", c.task8, c.task9, c.routine, c.text_step,
        c.cursor, c.pulse, c.way, c.flags, c.done, text, #players > 0 and table.concat(players, ";") or "-")
    return exb, #motions > 0 and table.concat(motions, ";") or "-"
end

return M
