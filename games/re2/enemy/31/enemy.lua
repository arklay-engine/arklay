-- G2, William Birkin's second form, `Em_31` (entity id 0x31).
--
-- `Em_31` runs `Routine_0` from `funcs_49032D` (`0x52a410`): 0 is the init
-- (`FUN_004903f0`) and 1 the move (`FUN_00490890`); the hit (2,
-- `FUN_00494440`) and the death (3, `FUN_00495bc0`) aren't ported yet. The
-- move runs by `Type & 0x3f`: 0 and 3 run `funcs_4908BB` (`0x52a480`) before
-- the clip and `funcs_4908D2` (`0x52a4a8`) after it, by `Routine_1`. Of
-- those, routine 7 (`0x4930b0`) is the change that room 509's scene starts:
-- it waits on clip 19 until the room's script raises `Sce_free0`'s bit 0.
-- The change itself and the fight aren't ported yet.
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

-- `FUN_004903f0`. What it does to the parts' drawing (their colours, their
-- sizes and `Be_flg`'s 0x400 on parts 3, 6, 7 and 8) and its shadow aren't
-- modelled.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    e:set_life(e:easy() and LIFE_EASY or LIFE)
    e:set_d_life(0)
    e:set_neck_flags(0)
    for index = CNT_218, W4 do
        e:set_var(index, 0)
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
        -- `+0x225`, the high byte of `REGEN`.
        e:set_var(REGEN, e:var(REGEN) | ((e:rnd() & 1) << 12))
        e:routines(1, 0, 3, 0)
    elseif low == 3 then
        e:set_life(LIFE_SCENE)
        e:set_be_flg(e:be_flg() | 10)
        e:routines(1, 9, 0, 0)
    end
end

-- Routine 7 (`0x4930b0`): the change, which room 509's scene starts. It
-- holds clip 19's first frame (its `Routine_2` 0 and 1 don't play it) until
-- the room's script raises `Sce_free0`'s bit 0. What `Routine_2` 0 does to
-- the parts' drawing isn't modelled, and the change itself isn't ported yet.
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
    if sub == 1 and e:sce_free(0) & 1 ~= 0 then
        e:set_routine(2, 2)
    end
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
