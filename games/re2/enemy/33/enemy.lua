-- G3, William Birkin's third form, `Em_33` (entity id 0x33).
--
-- `Em_33` runs `Routine_0` from `0x52a5f0`: 0 is the init (`FUN_00496a80`)
-- and 1 the move (`FUN_00496c90`), which runs `Routine_1` from `0x52a610`.
-- Of those, routine 0 (`FUN_00496d00`) is room 509's scene: its
-- `Routine_2` 0 waits off the stage until the room's script raises
-- `Sce_free0` to 1. The rest of the scene, the fight, the hit (2,
-- `FUN_00499210`) and the death (3, `FUN_00499e00`) aren't ported yet.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and
-- `e:set_var` are the fields of the original's work that the engine doesn't
-- read, and the eye's globals at `0x6896e4`.

local CNT_219 <const> = 0 -- `+0x219`: the ticks of blood left (`FUN_0049add0`), which nothing sets yet.
local CNT_21C <const> = 1 -- `+0x21c`.
local CNT_21D <const> = 2 -- `+0x21d`.
local CNT_223 <const> = 3 -- `+0x223`.
local CNT_22B <const> = 4 -- `+0x22b`.
-- `+0x228` to `+0x22a`: what the eye does (`FUN_0049aad0`), its step and
-- its count.
local LOOK <const> = 5
local LOOK_STEP <const> = 6
local LOOK_CNT <const> = 7
-- `+0x22c` to `+0x22f`: part 2's pulse (`FUN_0049ad10`), whether it's
-- stopped, and its place.
local PULSE <const> = 8
local PULSE_OFF <const> = 9
local PULSE_W <const> = 10
-- `0x6896e6` and `0x6896e7`: where the eye looks, across and up, from -15
-- to 15.
local EYE_X <const> = 11
local EYE_Y <const> = 12
local SIZE <const> = 13 -- `Size` (`+0x176`), which isn't drawn.
local AT_EM_FLG <const> = 14 -- `+0x226`.
-- The first weights of the morphs (`pSa_dat_head`'s words from 0xc) of
-- part 0xb and part 2.
local W_B <const> = 15
local W2 <const> = 16

-- The init's life: normal and with `fg_system`'s `0x20`.
local LIFE <const> = 900
local LIFE_EASY <const> = 750

-- `BYTE_0052a5e8`: the parts whose first morph routine 0 sets.
local SCENE_PARTS <const> = { 2, 3, 6, 9, 0xA, 0x11, 0x12, 0x13 }

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

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

-- `Sa_parts_allmv` on part 2, from its first weight and its pulse
-- (`FUN_0049ad10`): the pulse's place below 0x1000 is the third weight's
-- 0x1000 less it, and above it the second's.
local function morph_2(e)
    local place = i16(e:var(PULSE_W))
    if place < 0x1000 then
        e:morph(2, e:var(W2), 0, 0x1000 - place)
    else
        e:morph(2, e:var(W2), place - 0x1000, 0)
    end
end

-- `FUN_0049b530`: a part's first weight, and `Sa_parts_allmv` on it.
local function set_weight(e, part, weight)
    if part == 2 then
        e:set_var(W2, weight)
        morph_2(e)
    else
        if part == 0xB then
            e:set_var(W_B, weight)
        end
        e:morph(part, weight)
    end
end

-- `FUN_0049ac90` and `FUN_0049acd0`: move where the eye looks by `step`,
-- held to 15 either way. 1 when it passes 15, -1 when it passes -15.
local function look(e, index, step)
    local at = i8(e:var(index) + step)
    if at > 15 then
        e:set_var(index, 15)
        return 1
    elseif at < -15 then
        e:set_var(index, -15)
        return -1
    end
    e:set_var(index, at)
    return 0
end

-- `FUN_00496a80`. Its shadow, its neck (`Neck_no` 3) and what it does to
-- the parts' drawing aren't modelled.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    for index = CNT_219, W2 do
        e:set_var(index, 0)
    end
    e:set_speed(0, 0, 0)
    e:set_life(e:easy() and LIFE_EASY or LIFE)
    e:set_var(AT_EM_FLG, 0x28)
    e:set_be_flg(e:be_flg() | 0x1C000000)
    e:set_neck_flags(0)
    -- `Atd[0]`: `W` and `D` 1200, `Ofs_y` -2000, `At_w` and `At_d` 1100 and
    -- `At_h` 2000.
    e:set_body_box(-2000, 0x44C, 2000, 0x4B0)
    e:set_body_ofs_x(0)
    e:clip(0, 0, 0)
    e:joint(0x100)
    local kind = e:kind()
    if kind == 1 then
        e:set_routine(1, 1)
    elseif kind == 0x6002 then
        e:routines(4, 0, 0, 0)
    end
    set_weight(e, 0xB, 0)
end

-- Routine 0's `Routine_2` 0: off the stage on clip 10 until the room's
-- script raises `Sce_free0` to 1.
local function scene_wait(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_sce_free(1, 0)
        e:clip(10, 0, 0xF)
        e:spd()
        e:joint(0x100)
        e:set_var(PULSE_OFF, 1)
        e:set_be_flg(e:be_flg() | 0x800)
        e:set_var(SIZE, 0xD98)
        for _, part in ipairs(SCENE_PARTS) do
            set_weight(e, part, 0xFFF)
        end
        set_weight(e, 0xB, 0xFFF)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
    end
    if e:sce_free(0) == 1 then
        e:set_routine(2, 1)
        e:set_routine(3, 0)
    end
end

-- Routine 0 (`FUN_00496d00`): room 509's scene.
local function scene(e)
    local two = e:routine(2)
    if two == 0 then
        scene_wait(e)
    end
end

-- `FUN_00496c90`: the move, by `Routine_1`.
local function move(e)
    local one = e:routine(1)
    if one == 0 then
        scene(e)
    end
end

-- `FUN_0049aad0` on `LOOK` 0: the eye looks across, from -15 to 15 and
-- back, by up to 3 a tick, resting up to 91 ticks at -15, and up or down by
-- 2 a tick as G3 turns toward the player or away. Drawing where it looks
-- (`Em33_eye`'s texture) isn't modelled.
local function look_around(e)
    local step = e:var(LOOK_STEP)
    if step == 0 then
        e:set_var(LOOK_STEP, 1)
        e:set_var(EYE_X, -15)
        step = 1
    end
    if step == 1 then
        if look(e, EYE_X, e:rnd() & 3) > 0 then
            e:set_var(LOOK_STEP, 2)
        end
    elseif step == 2 then
        if look(e, EYE_X, -(e:rnd() & 3)) < 0 then
            e:set_var(LOOK_STEP, 3)
            e:set_var(LOOK_CNT, (e:rnd() & 3) * 30 + 1)
        end
    elseif step == 3 then
        local count = (e:var(LOOK_CNT) - 1) & 0xFF
        e:set_var(LOOK_CNT, count)
        if count == 0 then
            e:set_var(LOOK_STEP, 1)
        end
    end
    local x, _, z = e:player_pos()
    look(e, EYE_Y, e:goto_ck(x, z, 1) > 0 and 2 or -2)
end

-- `FUN_0049ad10`: part 2's pulse, up by 0x200 a tick to 0x2800 and down by
-- 0x800 a tick to 0, while `PULSE_OFF` is clear.
local function pulse(e)
    if e:var(PULSE_OFF) ~= 0 then
        return
    end
    local state = e:var(PULSE)
    local place = i16(e:var(PULSE_W))
    if state == 1 then
        place = place + 0x200
        if place > 0x2800 then
            place = 0x2800
            e:set_var(PULSE, 2)
        end
    elseif state == 2 and place - 0x800 >= 0 then
        place = place - 0x800
    else
        e:set_var(PULSE, 1)
        place = 0
    end
    e:set_var(PULSE_W, place & 0xFFFF)
    morph_2(e)
end

function update(e)
    count_down(e, CNT_21C)
    count_down(e, CNT_21D)
    count_down(e, CNT_22B)
    count_down(e, CNT_223)
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        move(e)
    end
    if e:var(LOOK) == 0 then
        look_around(e)
    end
    pulse(e)
end
