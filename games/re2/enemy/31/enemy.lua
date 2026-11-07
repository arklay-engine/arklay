-- G2, William Birkin's second form, `Em_31` (entity id 0x31).
--
-- `Em_31` runs `Routine_0` from `funcs_49032D` (`0x52a410`): 0 is the init
-- (`FUN_004903f0`) and 1 the move (`FUN_00490890`); the hit (2,
-- `FUN_00494440`) and the death (3, `FUN_00495bc0`) aren't ported yet. The
-- move runs by `Type & 0x3f`: 0 and 3 run `funcs_4908BB` (`0x52a480`) before
-- the clip and `funcs_4908D2` (`0x52a4a8`) after it, by `Routine_1`. Of
-- those, routine 7 (`0x4930b0`) is the change that room 509's scene starts:
-- it waits on clip 19 until the room's script raises `Sce_free0`'s bit 0,
-- then changes and climbs down to the floor for the fight (routine 1), which
-- isn't ported yet.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and
-- `e:set_var` are the fields of the original's work that the engine doesn't
-- read.

local CNT_218 <const> = 0 -- `+0x218`, which counts down from 15 while `FLAGS`' 2.
local CNT_219 <const> = 1 -- `+0x219`, how many times `+0x218` runs out before `FLAGS`' 2 clears.
local CNT_21A <const> = 2 -- `+0x21a`.
local CNT_21B <const> = 3 -- `+0x21b`: the ticks to the next point of life regained.
local FLAGS <const> = 4 -- `+0x21c`: 0x20 throbs parts 1 and 2.
local WAVE <const> = 5 -- `+0x21e`: the throb's place, mod 64.
local HURT <const> = 6 -- `+0x220` and `+0x221`: the eye's swell (`FUN_004966a0`).
local HURT_221 <const> = 7
local REACH <const> = 8 -- `+0x222`, a word: 2000.
local REGEN <const> = 9 -- `+0x224`, a word: the points of life it may still regain.
local ATTR <const> = 10 -- `+0x226`.
local CNT_22E <const> = 11 -- `+0x22e`.
-- The weights of the morphs (`pSa_dat_head`'s words from 0xc): part 1's
-- one, part 2's two and part 4's one.
local W1 <const> = 12
local W2A <const> = 13
local W2B <const> = 14
local W4 <const> = 15
-- The `Size_y` of parts 3, 6, 7 and 8, which grow during the change, and
-- which of them still grow (their `Be_flg`'s 0x400), as a bit per part.
local SIZE <const> = { [3] = 16, [6] = 17, [7] = 18, [8] = 19 }
local GROWING <const> = 20

-- The init's life: normal, with `fg_system`'s `0x20`, and for the scene types.
local LIFE <const> = 700
local LIFE_EASY <const> = 600
local LIFE_SCENE <const> = 5000

-- The life it regains up to, by `Type`'s 1.
local REGAIN_TO <const> = { [0] = 500, 0 }

-- `BYTE_0052a430`: the throb's swell of part 1, by `WAVE`, as signed bytes.
local THROB <const> = {
    [0] = 0x0C, 0x17, 0x23, 0x2B, 0x33, 0x3C, 0x42, 0x47, 0x4B, 0x4F, 0x52, 0x56, 0x59, 0x5C, 0x5E, 0x60,
    0x61, 0x62, 0x63, 0x64, 0x63, 0x62, 0x61, 0x60, 0x5E, 0x5C, 0x59, 0x57, 0x55, 0x53, 0x50, 0x4E,
    0x4B, 0x48, 0x44, 0x41, 0x3E, 0x3A, 0x36, 0x31, 0x2C, 0x27, 0x22, 0x1D, 0x18, 0x13, 0x0D, 0x07,
    0x00, -0x06, -0x0F, -0x0B, -0x1A, -0x1E, -0x20, -0x22, -0x23, -0x23, -0x22, -0x1F, -0x19, -0x14, -0x0A, 0x00,
}

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function count_down(e, index)
    local value = e:var(index)
    if value ~= 0 then
        e:set_var(index, value - 1)
    end
end

-- `Sa_parts_allmv` on each morphing part, from its weights.
local function morph_1(e)
    e:morph(1, e:var(W1))
end

local function morph_2(e)
    e:morph(2, e:var(W2A), e:var(W2B))
end

local function morph_4(e)
    e:morph(4, e:var(W4))
end

-- `FUN_004903f0`. What it does to the parts' drawing (their colours, the
-- sizes but parts 3, 6, 7 and 8's `Size_y`) and its shadow aren't modelled.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    e:set_life(e:easy() and LIFE_EASY or LIFE)
    e:set_d_life(0)
    e:set_neck_flags(0)
    for index = CNT_218, GROWING do
        e:set_var(index, 0)
    end
    for part, index in pairs(SIZE) do
        e:set_var(index, 0x200)
        e:set_var(GROWING, e:var(GROWING) | (1 << part))
    end
    e:set_var(CNT_21B, 5)
    e:set_var(REACH, 0x7D0)
    -- `Atd[0]`: `W` and `D` 900, `Ofs_y` -1500, `At_w` and `At_d` 900 and
    -- `At_h` 1500.
    e:set_body_box(-1500, 900, 1500, 900)
    e:set_body_ofs_x(0)
    e:set_be_flg(e:be_flg() | 0xC000004)
    e:clip(0, 0, 0)
    e:joint(0x100)
    morph_1(e)
    morph_2(e)
    e:set_routine(1, 7)
    e:clip(0x13, 0, 0)
    e:joint(0x100)
    e:set_var(W2A, 0x1000)
    morph_2(e)
    e:set_var(W4, 0x1000)
    morph_4(e)
    local low = e:kind() & 0xFFF
    if low == 2 then
        e:set_timer(0, 0x3C)
        e:set_life(LIFE_SCENE)
        e:set_be_flg(e:be_flg() | 2)
        e:set_var(GROWING, e:var(GROWING) & ~0x1C0)
        -- `+0x225`, the high byte of `REGEN`.
        e:set_var(REGEN, e:var(REGEN) | ((e:rnd() & 1) << 12))
        e:routines(1, 0, 3, 0)
    elseif low == 3 then
        e:set_life(LIFE_SCENE)
        e:set_be_flg(e:be_flg() | 10)
        e:set_var(GROWING, e:var(GROWING) & ~0x1C0)
        e:routines(1, 9, 0, 0)
    end
end

-- Grow a part's `Size_y` by `step` while it still grows; it stops past
-- 0xfff. Whether it grew.
local function grow(e, part, step)
    if e:var(GROWING) & (1 << part) == 0 then
        return false
    end
    local size = i16(e:var(SIZE[part]) + step)
    e:set_var(SIZE[part], size)
    if size > 0xFFF then
        e:set_var(GROWING, e:var(GROWING) & ~(1 << part))
    end
    return true
end

-- `Spd_get`, taken on the clip's second frame as well when it's on its first.
local function spd_first(e, hokan)
    e:spd()
    if e:move_cnt() == 0 then
        e:set_move_cnt(1)
        e:spd()
        if hokan then
            e:set_hokan(hokan)
        end
        e:set_move_cnt(e:move_cnt() - 1)
    end
end

local function add_w2(e, a, b)
    e:set_var(W2A, i16(e:var(W2A) + a))
    e:set_var(W2B, i16(e:var(W2B) + b))
end

-- `Routine_2` 2: clip 19 plays while part 2's two morphs sway.
local function change_swell(e)
    local count = e:move_cnt()
    if count > 5 and count < 0x23 then
        add_w2(e, (10 - count) * 8, count * 8 - 0x50)
    end
    if count > 0x23 and count < 0x3C then
        add_w2(e, count * 4 - 0x8C, (0x23 - count) * 4)
    end
    if count > 0x3C and count < 0x4B then
        add_w2(e, (0x3C - count) * 8, count * 8 - 0x1E0)
    end
    if count > 0x4B then
        add_w2(e, count * 2 - 0x96, (0x4B - count) * 2)
    end
    local odd = count & 1
    add_w2(e, odd * -0x78 + 0x3C, odd * 0x78 - 0x3C)
    morph_2(e)
    e:spd()
    if e:joint(0x100) then
        e:clip(0x14, 0, 0xF)
        e:set_routine(2, 3)
        e:set_timer(3, 0)
        e:set_timer(0, 0)
    end
    e:add_speed_xz()
end

-- `Routine_2` 3: clip 0x14, part 2's morph runs to its second target and
-- back, and part 3 grows; clip 0x15 once it has.
local function change_grow(e)
    spd_first(e, 0xF)
    e:joint(0x100)
    e:add_speed_xz()
    local timer = i16(e:timer(0))
    local flags = e:timer(3) & 0xFF
    if timer > 0x1E and flags & 1 == 0 then
        add_w2(e, (0x1E - timer) * 4, (timer - 0x1E) * 4)
        if e:var(W2B) > 0x1000 then
            e:set_var(W2B, 0x1000)
            e:set_var(W2A, 0)
            flags = flags | 1
        end
    end
    if flags & 1 ~= 0 and flags & 4 == 0 then
        e:set_var(W2B, i16(e:var(W2B) - 0x80))
        if e:var(W2B) < 0 then
            e:set_var(W2B, 0)
            flags = flags | 4
        end
    end
    e:set_timer(3, flags)
    local odd = e:move_cnt() & 1
    add_w2(e, odd * 0xF0 - 0x78, odd * -0xF0 + 0x78)
    morph_2(e)
    if timer == 0x29 then
        -- `blood_censor` is taken as off.
        e:effect(0, 3, 0x1FA0, i16(e:angle()), 3, 0, 0, 0)
    end
    if timer > 0x2D and grow(e, 3, (timer - 0x2D) * 2) and e:var(GROWING) & (1 << 3) == 0 then
        e:clip(0x15, 0, 0xF)
        e:set_routine(2, 4)
        e:set_timer(3, 0)
    end
    e:set_timer(0, timer + 1)
end

-- `Routine_2` 4: clip 0x15, then clip 0x16.
local function change_rise(e)
    spd_first(e)
    if e:joint(0x100) then
        e:clip(0x16, 0, 0xF)
        e:set_routine(2, 5)
        e:set_timer(3, 0)
        e:set_timer(0, 0)
        e:set_neck_flags(e:neck_flags() & 0xFD)
    end
    e:add_speed_xz()
end

-- `Routine_2` 5: part 4's morph runs back to its base. What it does to
-- `DWORD_0068968c` (the eye's light) isn't modelled.
local function change_eye(e)
    local was = e:timer(0)
    local timer = i16(was + 1)
    e:set_timer(0, timer)
    if was == 0x2D then
        e:set_timer(3, 0)
        e:set_routine(2, 6)
        e:set_neck_flags(e:neck_flags() | 2)
    end
    if timer > 0 and e:timer(3) & 1 == 0 then
        e:set_var(W4, i16(e:var(W4) - timer * 0x10))
        if e:var(W4) < 0 then
            e:set_var(W4, 0)
            e:set_timer(3, e:timer(3) | 1)
        end
    end
    morph_4(e)
end

-- `Routine_2` 6: clip 0x16, parts 8, 6 and 7 grow in turn with a spurt of
-- blood on each other frame.
local function change_sprout(e)
    local count = e:move_cnt()
    for _, part in ipairs({ { 8, 0xF }, { 6, 0x14 }, { 7, 0x28 } }) do
        local index, from = part[1], part[2]
        if count > from and grow(e, index, (count - from) * 0x20) and count & 1 ~= 0 then
            e:effect(0, 0x14, e:rnd() * 8 + 0x800, 0, index, -100, 100, 0)
        end
    end
    e:spd()
    if e:joint(0x100) then
        e:set_sce_free(0, e:sce_free(0) | 2)
        e:set_routine(2, 7)
        e:set_timer(0, 0)
        e:clip(0, 0, 0xFF)
    end
    e:add_speed_xz()
end

-- `Routine_2` 7: the scene's wait for `Sce_free0`'s bit 2, after which it
-- steps off on clip 15.
local function change_wait(e)
    local timer = e:timer(0)
    if i16(timer) > 0x1E then
        e:joint(0x10)
    end
    e:set_timer(0, timer + 1)
    if e:sce_free(0) & 4 ~= 0 then
        e:set_routine(2, 8)
        e:set_speed(200, 0, 0)
        e:set_timer(0, 0)
        e:clip(0xF, 0, 0)
        e:joint(0x100)
        e:sound(9)
    end
end

-- `Routine_2` 8 and 9: the fall, down to the floor.
local function fall(e)
    local timer = i16(e:timer(0))
    local _, y = e:pos()
    y = y + timer * 0x1E
    e:set_timer(0, timer + 1)
    e:set_pos_y(y)
    return y
end

-- Routine 7 (`0x4930b0`): the change, which room 509's scene starts. It
-- holds clip 19's first frame (its `Routine_2` 0 and 1 don't play it) until
-- the room's script raises `Sce_free0`'s bit 0, then changes (2 to 6), waits
-- for the script again (7), falls to the floor (8 and 9) and starts the
-- fight (10). What it does to the parts' drawing (their sizes but the four
-- in `SIZE`, and their `Be_flg`) isn't modelled.
local function change(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0x13, 0, 0)
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_routine(2, 1)
        e:set_timer(3, 0)
        e:set_timer(0, 0x96)
        e:set_timer(2, 0)
        sub = 1
    end
    if sub == 1 then
        if e:sce_free(0) & 1 ~= 0 then
            e:set_routine(2, 2)
        end
    elseif sub == 2 then
        change_swell(e)
    elseif sub == 3 then
        change_grow(e)
    elseif sub == 4 then
        change_rise(e)
    elseif sub == 5 then
        change_eye(e)
    elseif sub == 6 then
        change_sprout(e)
    elseif sub == 7 then
        change_wait(e)
    elseif sub == 8 then
        if fall(e) > -2000 then
            e:set_routine(2, 9)
        end
        e:add_speed_xz()
    elseif sub == 9 then
        if e:joint(0x100) then
            e:set_routine(2, 10)
            e:set_neck_flags(e:neck_flags() & 0xFD)
        end
        if fall(e) > -400 then
            e:set_pos_y(0)
        end
        if e:move_cnt() < 0xC then
            e:add_speed_xz()
        end
    elseif sub == 10 then
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 10)
        e:set_neck_flags(e:neck_flags() & 0xFD)
    end
    -- The throb of parts 0 to 2's sizes isn't modelled, only its place.
    e:set_timer(2, (e:timer(2) + 1) & 0x3F)
end

-- `funcs_4908BB` (before the clip) and `funcs_4908D2` (after it), by
-- `Routine_1`.
local BEFORE <const> = {}
local AFTER <const> = {
    [7] = change,
}

-- `FUN_00490890`.
local function move(e)
    local kind = e:kind() & 0x3F
    if kind ~= 0 and kind ~= 3 then
        return
    end
    local before = BEFORE[e:routine(1)]
    if before then
        before(e)
    end
    local after = AFTER[e:routine(1)]
    if after then
        after(e)
    end
end

-- `Em_31`'s throb of parts 1 and 2 while `FLAGS`' 0x20, with two `Rnd`s
-- for the eye (`DWORD_0068968c`) on 16 ticks of each 32.
local function throb(e)
    local wave = e:var(WAVE)
    e:set_var(W1, i16((0x100 - THROB[wave]) * 0x10))
    morph_1(e)
    local odd = wave & 1
    e:set_var(W2A, i16(e:var(W2A) + odd * 0x100 - 0x80))
    e:set_var(W2B, i16(e:var(W2B) - odd * 0x100 + 0x80))
    morph_2(e)
    wave = (wave + 1) & 0xFF
    e:set_var(WAVE, wave & 0x3F)
    if wave & 0x10 ~= 0 then
        e:rnd()
        e:rnd()
    end
end

function update(e)
    local flags = e:var(FLAGS)
    if flags & 2 ~= 0 then
        local count = e:var(CNT_218)
        e:set_var(CNT_218, (count - 1) & 0xFF)
        if count == 0 then
            local times = e:var(CNT_219)
            if times ~= 0 then
                times = times - 1
                e:set_var(CNT_219, times)
            end
            e:set_var(CNT_218, 0xF)
            if times == 0 then
                e:set_var(FLAGS, flags & 0xFFFD)
            end
        end
    end
    -- It regains a point of life every eight ticks, up to 500, while it has
    -- points to regain (`REGEN`, which runs down while its life is 500 or
    -- less).
    local life = e:life()
    if life < 0x1F5 and e:var(REGEN) ~= 0 then
        e:set_var(REGEN, e:var(REGEN) - 1)
    end
    if not e:easy() then
        local was = e:var(CNT_21B)
        e:set_var(CNT_21B, (was - 1) & 0xFF)
        if was == 0 and e:var(REGEN) ~= 0 then
            e:set_var(CNT_21B, 7)
            if life > 0 and life < REGAIN_TO[e:kind() & 1] then
                e:set_life(life + 1)
            end
        end
    end
    count_down(e, CNT_21A)
    count_down(e, CNT_22E)
    if e:var(FLAGS) & 0x20 ~= 0 then
        throb(e)
    end
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        move(e)
    end
    e:seq_sound()
end
