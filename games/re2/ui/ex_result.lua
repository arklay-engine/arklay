-- RESULT's records: Extreme Battle's best times, which the title's RESULT
-- row opens (`FUN_00507AB0`, `title_mv_state` 7). The title starts this as
-- task 2 (`FUN_004DD4F0`), which suspends the title's task until it ends.
--
-- The task's routine (`BYTE_00698822`, through 0x53AB10):
--   0 `Load_ex_result` (0x4DD570): the background (`RESULT.adt`) and the
--     sheet (`RES_MOJI.TIM`, texture 0x17), a tick each, then slot 0's fade
--     in from black (`Fade_set(0x200, -0x200)`).
--   1 `FUN_004DD670`: waits for the fade.
--   2 `FUN_004DD690`: reads the records (`FUN_004DE720`), then
--     `FUN_004DDB40` shows them, a count (`WORD_00698820`) a tick: RESULT
--     on 0, the levels' labels on 0x1E (`FUN_004DDC30`), their times on
--     0x3C (`FUN_004DDD30`). Past 0x5A, a press of the raw pad's 0x8E0 fades
--     slot 0 out (`Fade_set(0x200, 0x400)`).
--   3 `FUN_004DD7D0`: once the fade is done, black (`Fade_adjust(0,
--     0x7FFF)`), the sprites cleared, the records written back
--     (`FUN_004DE690`), and routine 4, on which the task wakes the title.
-- Every tick that doesn't sleep in its routine draws the sprites
-- (`FUN_004DD8D0`).
--
-- After an Extreme Battle game (`BYTE_00989E90`, which `Result_exit`
-- raises and `Title`'s step 0 sends here) routine 2's sub-step 2 is the
-- after-game page (`FUN_004DDFF0`) in place of `FUN_004DDB40`: the level's
-- record against the run, a count a tick to 0xC9, with NEW RECORD and the
-- record taken from the run at 0xB4 when the run's total is the lower or
-- the level has none. Past 0xC9 a press of 0x8E0 fades the music
-- (`Snd_bgm_fade_ON(0x5A)`) and slot 0 out, sub-step 3 waits 0x5A ticks,
-- and routine 3 hands back as above (`FUN_004DE650`'s new game from
-- `INIT_TBL.DAT` is the game's, not the screen's).

local M = {}

-- The title's page for the sheet (texture 0x17).
M.PAGE = 9

-- The sheet's glyphs (0x53AA08): u, v, w, h.
local GLYPHS = {
    [0x00] = { 0, 0, 13, 14 }, [0x01] = { 15, 0, 13, 14 }, [0x02] = { 29, 0, 13, 14 },
    [0x03] = { 43, 0, 13, 14 }, [0x04] = { 58, 0, 11, 14 }, [0x05] = { 71, 0, 11, 14 },
    [0x06] = { 84, 0, 14, 14 }, [0x07] = { 113, 32, 12, 14 }, [0x08] = { 102, 0, 6, 14 },
    [0x09] = { 128, 32, 10, 14 }, [0x0A] = { 112, 0, 13, 14 }, [0x0B] = { 127, 0, 11, 14 },
    [0x0C] = { 140, 0, 14, 14 }, [0x0D] = { 154, 0, 13, 14 }, [0x0E] = { 168, 0, 14, 14 },
    [0x0F] = { 183, 0, 11, 14 }, [0x10] = { 140, 32, 14, 14 }, [0x11] = { 197, 0, 13, 14 },
    [0x12] = { 212, 0, 10, 14 }, [0x13] = { 225, 0, 12, 14 }, [0x14] = { 238, 0, 13, 14 },
    [0x15] = { 0, 14, 14, 14 }, [0x16] = { 14, 14, 14, 14 }, [0x17] = { 28, 14, 14, 14 },
    [0x18] = { 42, 14, 14, 14 }, [0x19] = { 156, 32, 10, 14 }, [0x1A] = { 56, 14, 14, 14 },
    [0x1B] = { 70, 14, 14, 14 }, [0x1C] = { 84, 14, 14, 14 }, [0x1D] = { 98, 14, 14, 14 },
    [0x1E] = { 112, 14, 14, 14 }, [0x1F] = { 126, 14, 14, 14 }, [0x20] = { 140, 14, 14, 14 },
    [0x21] = { 154, 14, 14, 14 }, [0x22] = { 168, 14, 14, 14 }, [0x23] = { 182, 14, 14, 14 },
    [0x24] = { 196, 14, 10, 14 }, [0x25] = { 213, 14, 8, 14 }, [0x26] = { 224, 14, 14, 14 },
    [0x27] = { 240, 14, 10, 14 }, [0x28] = { 152, 32, 10, 14 }, [0x29] = { 241, 32, 8, 14 },
    [0x2A] = { 0, 32, 41, 14 }, [0x2B] = { 56, 32, 41, 14 }, [0x2C] = { 0, 46, 41, 14 },
    [0x2D] = { 42, 48, 41, 14 }, [0x2E] = { 0, 76, 104, 36 }, [0x2F] = { 136, 80, 104, 20 },
    [0x30] = { 84, 48, 140, 8 }, [0x31] = { 0, 112, 255, 24 }, [0x32] = { 1, 62, 11, 11 },
    [0x33] = { 14, 62, 11, 11 }, [0x34] = { 29, 62, 11, 11 }, [0x35] = { 44, 62, 11, 11 },
    [0x36] = { 56, 62, 11, 11 }, [0x37] = { 71, 62, 11, 11 }, [0x38] = { 84, 62, 11, 11 },
    [0x39] = { 98, 62, 11, 11 }, [0x3A] = { 112, 62, 11, 11 }, [0x3B] = { 127, 62, 11, 11 },
    [0x3C] = { 240, 16, 10, 12 }, [0x3D] = { 0, 136, 100, 12 }, [0x3E] = { 112, 136, 102, 12 },
    [0x3F] = { 166, 32, 13, 14 }, [0x40] = { 140, 62, 11, 11 }, [0x41] = { 0, 152, 153, 20 },
}

-- The sprites' strings (0x53A910 by the offsets at 0x53A9A8), glyphs to
-- the end. 0 to 11 are the times, which `FUN_004DE430` writes over.
local STRINGS = {
    [0x0C] = { 0x2A }, [0x0D] = { 0x2B }, [0x0E] = { 0x2C }, [0x0F] = { 0x2D },
    [0x10] = { 0x0D, 0x0E, 0x29, 0x03, 0x00, 0x13, 0x00 },
    [0x11] = { 0x0B, 0x15, 0x1B }, [0x12] = { 0x0B, 0x15, 0x1C }, [0x13] = { 0x0B, 0x15, 0x1D },
    [0x14] = { 0x2E }, [0x15] = { 0x2F }, [0x16] = { 0x30 }, [0x17] = { 0x31 },
    [0x18] = { 0x3D }, [0x19] = { 0x3E }, [0x1A] = { 0x41 },
}
local TIME_START = { 0x1A, 0x1A, 0x27, 0x1A, 0x1A, 0x27, 0x1A, 0x1A }

-- The rows' places by the levels open (`BYTE_00989E91`): level 1's row
-- (0x53A9E0), level 2's (0x53A9E8), and each level's offsets of its second
-- and third times, its line and its total (0x53A9F0).
local LEVEL1_Y = { [0] = 96, 60, 48 }
local LEVEL2_Y = { [0] = 0, 146, 106 }
local ROWS = { [0] = { 18, 36, 48, 58 }, { 15, 30, 40, 48 }, { 12, 24, 32, 38 } }

-- `FUN_004DDB90`'s flags: shown, and fading in from black.
local SHOWN = 1
local FADE_IN = 4
local FADE_OUT = 8
local PULSE = 0x10
local DONE = 0x80
-- The sprites' slots (0x695EA0 to 0x696014, 12 bytes each).
local SLOTS = 32
-- The draw's depth (`AddPolyFT4(..., 10)`).
local DEPTH = 10

local function is_total(id)
    return id == 3 or id == 7 or id == 11
end

-- `FUN_004DE430`: a time (seconds) into time string `id`, as
-- hours:minutes:seconds, in the large digits for a level's total (3, 7
-- and 11) and the small for its three; none is dashes, and 100 hours or
-- more the largest it shows.
function M.time_string(strings, value, id)
    local s = strings[id]
    local total = is_total(id)
    local function put(a, b, c, d, e, f)
        s[1], s[2], s[4], s[5], s[7], s[8] = a, b, c, d, e, f
        if not total then
            s[3], s[6] = 0x3C, 0x3C
        end
    end
    if value == 0 then
        if total then
            put(0x3F, 0x3F, 0x3F, 0x3F, 0x3F, 0x3F)
        else
            put(0x40, 0x40, 0x40, 0x40, 0x40, 0x40)
        end
        return
    end
    local hours = value // 0xE10
    if hours > 99 then
        local nine, five = total and 0x23 or 0x3B, total and 0x1F or 0x37
        put(nine, nine, five, nine, five, nine)
        return
    end
    local rest = value - hours * 0xE10
    local minutes = rest // 0x3C
    local seconds = rest - minutes * 0x3C
    local base = total and 0x1A or 0x32
    put(base + hours // 10, base + hours % 10, base + minutes // 10, base + minutes % 10,
        base + seconds // 10, base + seconds % 10)
end

-- `records`: per level (1 to 3), `{ character, kept, times = { a, b, c } }`
-- (0x989E94, 16 bytes a level), as `FUN_004DE720` reads them from
-- 0x689F60; `levels` the levels open (`BYTE_00989E91`, 0 to 2).
-- `after`: the game just ended, `{ level, character, times = { a, b, c } }`
-- (`BYTE_00989E92`, `BYTE_00989E93` and 0x989EC4), or nil from the title's
-- RESULT row.
function M.new(fx, records, levels, after)
    local strings = {}
    for id = 0, 11 do
        strings[id] = { table.unpack(TIME_START) }
    end
    for id, glyphs in pairs(STRINGS) do
        strings[id] = glyphs
    end
    return {
        fx = fx,
        after = after,
        records = records,
        levels = levels,
        strings = strings,
        routine = 0,
        step = 0,
        sub = 0,
        count = 0,
        sprites = {},
        slots = {},
        background = false,
        ended = false,
        loads = {},
    }
end

-- `FUN_004DDB90`: the first free slot shows `id` at x, y. Of `flags`'s
-- low byte, 2 fades it in, 4 out, and 8 fades it in slowly and then pulses
-- (`PULSE`).
local function show(c, x, y, id, flags)
    for i = 1, SLOTS do
        if not c.slots[i] then
            local slot = { id = id, x = x, y = y, flags = SHOWN, colour = 0x80 }
            if flags & 1 ~= 0 then
                slot.flags = slot.flags | 2
            end
            if flags & 2 ~= 0 then
                slot.colour = 0
                slot.flags = slot.flags | FADE_IN
            elseif flags & 4 ~= 0 then
                slot.flags = slot.flags | FADE_OUT
            elseif flags & 8 ~= 0 then
                slot.colour = 0
                slot.flags = slot.flags | FADE_IN | PULSE
            end
            c.slots[i] = slot
            return
        end
    end
end

local function clear(c)
    c.slots = {}
end

-- `FUN_004DDC30`: each open level's label, after its character's name
-- where it has a record, from the last level up.
local function labels(c)
    local y = { [0] = LEVEL1_Y[c.levels], LEVEL2_Y[c.levels], 0xA0 }
    for level = c.levels, 0, -1 do
        local record = c.records[level + 1]
        if record.kept then
            show(c, 0x60, y[level], record.character + 0x0C, 0x102)
        end
        show(c, 0x20, y[level], 0x11 + level, 0x102)
    end
end

-- `FUN_004DDD30`: each open level's three times, its line and its total,
-- from the last level up.
local function times(c)
    local rows = ROWS[c.levels]
    for level = c.levels, 0, -1 do
        local record = c.records[level + 1]
        local first = level * 4
        local total = record.times[1] + record.times[2] + record.times[3]
        if level == 2 then
            for k = 0, 2 do
                M.time_string(c.strings, record.times[k + 1], first + k)
                show(c, 0xAC, 0xA2 + 0xC * k, first + k, 0x102)
            end
            show(c, 0x92, 0xC2, 0x16, 0x102)
            M.time_string(c.strings, total, first + 3)
            show(c, 0xA0, 200, first + 3, 0x102)
        else
            local y = level == 0 and LEVEL1_Y[c.levels] or LEVEL2_Y[c.levels]
            M.time_string(c.strings, record.times[1], first)
            show(c, 0xAC, y, first, 0x102)
            M.time_string(c.strings, record.times[2], first + 1)
            show(c, 0xAC, y + rows[1], first + 1, 0x102)
            M.time_string(c.strings, record.times[3], first + 2)
            show(c, 0xAC, y + rows[2], first + 2, 0x102)
            show(c, 0x92, y + rows[3], 0x16, 0x102)
            M.time_string(c.strings, total, first + 3)
            show(c, 0xA0, y + rows[4], first + 3, 0x102)
        end
    end
end

-- `FUN_004DDB40`: the count's reveals; whether the count is past them.
local function reveal(c)
    if c.count == 0 then
        show(c, 0x6C, 8, 0x14, 0x100)
    elseif c.count == 0x1E then
        labels(c)
    elseif c.count == 0x3C then
        times(c)
    end
    if c.count < 0x5B then
        c.count = c.count + 1
        return false
    end
    return true
end

-- `FUN_004DD8D0`: each slot's fade a step, and its glyphs drawn. A
-- fading slot's code is semi-transparent (0x2E), but the draw zeroes the
-- primitive's code before `AddPolyFT4` (0x4DDAE6), so every glyph is
-- opaque, its texels scaled by the colour. A fade out goes by 0x10 a step
-- and frees the slot under 8; a pulsing slot comes in by 4 to full, goes
-- out by 0xC to under 0x40 and in again.
local function draw(c)
    local sprites = {}
    for i = 1, SLOTS do
        local slot = c.slots[i]
        if slot and slot.flags & FADE_OUT ~= 0 then
            if slot.flags & PULSE == 0 then
                slot.colour = slot.colour - 0x10
                if slot.colour < 8 then
                    c.slots[i] = nil
                    slot = nil
                end
            else
                slot.colour = slot.colour - 0xC
                if slot.colour < 0x40 then
                    slot.flags = (slot.flags & ~FADE_OUT) | FADE_IN
                end
            end
        end
        if slot then
            if slot.flags & FADE_IN ~= 0 then
                local pulse = slot.flags & PULSE ~= 0
                slot.colour = slot.colour + (pulse and 4 or 8)
                if slot.colour > 0x7F then
                    if pulse then
                        slot.flags = (slot.flags & ~FADE_IN) | DONE | FADE_OUT
                    else
                        slot.flags = (slot.flags & ~FADE_IN) | DONE
                        slot.colour = 0x80
                    end
                end
            end
            local x = slot.x
            for _, glyph in ipairs(c.strings[slot.id]) do
                local g = GLYPHS[glyph]
                sprites[#sprites + 1] = {
                    page = M.PAGE, x = x, y = slot.y, u = g[1], v = g[2], w = g[3], h = g[4],
                    colour = slot.colour, depth = DEPTH,
                }
                x = x + g[3]
            end
        end
    end
    c.sprites = sprites
end

-- `FUN_004DDFF0`: the after-game page's count. The level's record shows
-- on the first rows and the run on the second; at 0xB4 a level opens
-- (or, at level 3, the last), and the record goes to the run when it is
-- better. Whether the count is past its end.
local function after_page(c)
    local a = c.after
    local level = a.level
    local record = c.records[level]
    local first = 0
    local run = a.times[1] + a.times[2] + a.times[3]
    local best = record.times[1] + record.times[2] + record.times[3]
    local label = level + 0x10
    local character = (a.character >= 0 and a.character <= 3) and a.character + 0x0C or (level - 1) & 0xFF
    if c.count == 0 then
        show(c, 0x6C, 8, 0x14, 0x100)
    elseif c.count == 0x1E then
        show(c, 0x20, 0x30, 0x18, 0x102)
        show(c, 0x20, 0x40, label, 0x102)
        if record.kept then
            show(c, 0x60, 0x40, record.character + 0x0C, 0x102)
        end
        M.time_string(c.strings, record.times[1], first)
        show(c, 0xAC, 0x30, first, 0x102)
        show(c, 0x20, 0x84, 0x19, 0x102)
        show(c, 0x20, 0x94, label, 0x102)
        show(c, 0x60, 0x94, character, 0x102)
        M.time_string(c.strings, a.times[1], 4)
        show(c, 0xAC, 0x84, 4, 0x102)
    elseif c.count == 0x3C then
        M.time_string(c.strings, record.times[2], 1)
        show(c, 0xAC, 0x40, 1, 0x102)
        M.time_string(c.strings, a.times[2], 5)
        show(c, 0xAC, 0x94, 5, 0x102)
    elseif c.count == 0x5A then
        M.time_string(c.strings, record.times[3], 2)
        show(c, 0xAC, 0x50, 2, 0x102)
        M.time_string(c.strings, a.times[3], 6)
        show(c, 0xAC, 0xA4, 6, 0x102)
    elseif c.count == 0x78 then
        show(c, 0x90, 0x5A, 0x16, 0x102)
        show(c, 0x90, 0xAE, 0x16, 0x102)
    elseif c.count == 0x96 then
        M.time_string(c.strings, best, 3)
        show(c, 0xA0, 0x62, 3, 0x102)
        M.time_string(c.strings, run, 7)
        show(c, 0xA0, 0xB6, 7, 0x10A)
    elseif c.count == 0xB4 then
        if level == 3 then
            show(c, 0x60, 0xCC, 0x1A, 0x109)
        elseif c.levels < level then
            show(c, 0x40, 0xCC, 0x17, 0x109)
            c.levels = c.levels + 1
        end
        if run < best or not record.kept then
            show(c, 0x28, 0xB0, 0x15, 0x109)
            record.character = a.character
            record.kept = true
            record.times = { a.times[1], a.times[2], a.times[3] }
        end
    end
    if c.count < 0xC9 then
        c.count = c.count + 1
        return false
    end
    return true
end

-- One tick of the task: `input.edge` is the raw pad's newly pressed bits
-- (0x9885FE).
function M.tick(c, input)
    local fx = c.fx
    local slept = false
    if c.routine == 0 then
        if c.step == 0 then
            clear(c)
            c.background = false
            c.loads[#c.loads + 1] = "background"
            c.step = 1
            slept = true
        elseif c.step == 1 then
            c.loads[#c.loads + 1] = "sheet"
            c.step = 2
            slept = true
        else
            fx.set(0x200, -0x200)
            c.background = true
            c.routine = 1
            c.step = 0
        end
    elseif c.routine == 1 then
        if fx.done(0) then
            c.routine = 2
        end
    elseif c.routine == 2 then
        if c.sub == 0 then
            c.sub = c.after and 2 or 1
        elseif c.sub == 1 then
            if reveal(c) and input.edge & 0x8E0 ~= 0 then
                fx.set(0x200, 0x400)
                c.routine = 3
            end
        elseif c.sub == 2 then
            if after_page(c) and input.edge & 0x8E0 ~= 0 then
                c.music = "fade"
                fx.set(0x200, 0x400)
                c.sub = 3
                c.count = 0x5A
            end
        else
            if fx.done(0) then
                clear(c)
                c.background = false
            end
            local over = c.count < 1
            c.count = c.count - 1
            if over then
                c.music = "stop"
                c.routine = 3
            end
        end
    elseif c.routine == 3 then
        if fx.done(0) then
            if c.after then
                c.background = false
            end
            fx.set(0x200, 0)
            fx.level(0, 0x7FFF)
            if not c.after then
                clear(c)
            end
            c.routine = 4
        end
    end
    if slept then
        c.sprites = {}
    else
        draw(c)
        if c.routine > 3 then
            c.ended = true
        end
    end
end

return M
