-- G3, William Birkin's third form, `Em_33` (entity id 0x33).
--
-- `Em_33` runs `Routine_0` from `0x52a5f0`: 0 is the init (`FUN_00496a80`)
-- and 1 the move (`FUN_00496c90`), which runs `Routine_1` from `0x52a610`.
-- Of those, routine 0 (`FUN_00496d00`) is room 509's scene: G3 waits off
-- the stage, walks in and rises as the room's script steps `Sce_free0`
-- through 1, 2 and 3, and starts the fight in routine 1. The fight, the hit (2,
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
local MODE <const> = 17 -- `+0x218`: 1 plays the walk's second clip and swaps the swipes for routine 5.
local SIDE <const> = 18 -- `+0x21a`, a word: the swipe's arm (0 right, 1 left), or routine 3's turn, or routine 2's heading off a wall.
-- `+0x225`, which `Em_33` counts while a wall holds it (the engine's
-- `G3_STUCK`).
local STUCK <const> = 19
local AGAIN <const> = 20 -- `+0x224`: the swipe swung twice, or 1 makes `FUN_00497ab0` turn.
local LAST_TURN <const> = 21 -- `+0x230`: routine 3's last way to turn, 1 or -1.
local TURNS <const> = 22 -- `+0x231`: how often in a row it turned that way.
local B221 <const> = 23 -- `+0x221`: the swipe throws the player down rather than back.

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

local function abs(value)
    return value < 0 and -value or value
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
    for index = CNT_219, B221 do
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

-- `Routine_2` 1: clip 10 plays and moves G3 until the script sets
-- `Sce_free0` to 2.
local function scene_walk(e)
    e:spd()
    e:joint(0x100)
    e:add_speed_xz()
    if e:sce_free(0) == 2 then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
    end
    e:seq_sound()
end

-- Every other tick, `weight` on even ticks of `Timer2` and 0xfff on odd ones.
local function flicker(e, weight)
    local tick = (e:timer(2) + 1) & 0xFF
    e:set_timer(2, tick)
    return tick & 1 == 0 and weight or 0xFFF
end

-- `FUN_004974c0`: the clip, and `Timer0` down by `step` to 0. The frame's
-- high word.
local function play_down(e, step)
    e:joint(0x100)
    local timer = i16(e:timer(0)) - step
    e:set_timer(0, timer < 0 and 0 or timer)
    return e:now_seq() >> 16
end

-- `FUN_00497510`: `Timer0` down by 0x200 to 0, as parts 9 and 6's first
-- weight.
local function fade_9_6(e)
    local timer = i16(e:timer(0)) - 0x200
    timer = timer < 0 and 0 or timer
    e:set_timer(0, timer)
    set_weight(e, 9, timer)
    set_weight(e, 6, timer)
end

-- `Size` wobbles by 30 each tick, on `Timer2`.
local function wobble(e)
    local tick = (e:timer(2) + 1) & 0xFF
    e:set_timer(2, tick)
    e:set_var(SIZE, i16(e:var(SIZE) + (tick & 1 ~= 0 and 0x1E or -0x1E)))
end

-- `Routine_2` 2: the rise. It waits for `Sce_free0` 3, then holds clip 5's
-- first frame for 31 ticks while parts 10 and 0x11 flicker (1, 2), plays it
-- (3, 4, 5) while the morphs of parts 0x11 and 10 run down, flickers parts
-- 9, 6, 2, 3, 0x12 and 0x13 while part 0xb's morph runs down (6), shrinks
-- the morphs of parts 2, 3, 0x12 and 0x13 and wobbles (7, 8), swells and
-- settles (9, 10), plays clip 6 (11) and after 30 ticks sets `Sce_free1`
-- to 1 (12).
local function scene_rise(e)
    local three = e:routine(3)
    if three == 0 then
        if e:sce_free(0) == 3 then
            e:set_routine(3, 1)
        end
    elseif three == 1 or three == 2 then
        if three == 1 then
            e:set_routine(3, 2)
            e:clip(5, 0, 0xF)
            e:set_var(LOOK, 1)
            e:set_var(LOOK_STEP, 1)
            e:set_timer(0, 0x1F)
            e:set_timer(2, 0)
        end
        local weight = flicker(e, 0xF00)
        set_weight(e, 10, weight)
        set_weight(e, 0x11, weight)
        e:joint(0x100)
        e:set_timer(0, e:timer(0) - 1)
        e:set_move_cnt(0)
        if e:timer(0) == 0 then
            e:set_routine(3, 3)
        end
    elseif three == 3 then
        local weight = flicker(e, 0xF00)
        set_weight(e, 10, weight)
        set_weight(e, 0x11, weight)
        e:joint(0x200)
        if e:now_seq() & 0x10000 ~= 0 then
            e:set_routine(3, 4)
            e:set_timer(0, 0xFFF)
        end
    elseif three == 4 then
        local high = play_down(e, 0x180)
        set_weight(e, 0x11, e:timer(0))
        set_weight(e, 10, flicker(e, 0xE00))
        if high == 2 then
            e:set_routine(3, 5)
            e:set_timer(0, 0xFFF)
        end
    elseif three == 5 then
        local high = play_down(e, 0x100)
        set_weight(e, 10, e:timer(0))
        if high == 3 then
            e:set_routine(3, 6)
            e:set_timer(0, 0xFFF)
            e:set_timer(1, 0xFFF)
        end
    elseif three == 6 then
        local weight = flicker(e, 0xD00)
        for _, part in ipairs({ 9, 6, 2, 3, 0x12, 0x13 }) do
            set_weight(e, part, weight)
        end
        local timer = i16(e:timer(1) - 0x100)
        e:set_timer(1, timer & 0xFFFF)
        if timer < 0 then
            e:set_routine(3, 7)
            e:set_timer(1, 0xFFF)
            e:set_timer(0, 0xFFF)
            e:set_timer(3, 4)
            set_weight(e, 0xB, 0)
        else
            set_weight(e, 0xB, timer)
        end
    elseif three == 7 then
        local tick = (e:timer(2) + 1) & 0xFF
        e:set_timer(2, tick)
        local timer = i16(e:timer(1) - 0x400)
        e:set_var(SIZE, i16(e:var(SIZE) + (tick & 1 ~= 0 and 0x1E or -0x1E)))
        if timer < 0 then
            timer = 0
        end
        e:set_timer(1, timer)
        for _, part in ipairs({ 2, 3, 0x12, 0x13 }) do
            set_weight(e, part, timer)
        end
        fade_9_6(e)
        e:joint(0x100)
        local count = (e:timer(3) - 1) & 0xFF
        e:set_timer(3, count)
        if count == 0 then
            e:set_routine(3, 8)
        end
    elseif three == 8 then
        wobble(e)
        fade_9_6(e)
        e:joint(0x100)
        if e:now_seq() & 0x40000 ~= 0 then
            e:set_routine(3, 9)
        end
    elseif three == 9 then
        local size = i16(e:var(SIZE) + 500)
        if size > 0x117F then
            size = 0x11FF
            e:set_routine(3, 10)
        end
        e:set_var(SIZE, size)
        fade_9_6(e)
        e:joint(0x100)
    elseif three == 10 then
        local size = i16(e:var(SIZE) - 5)
        e:set_var(SIZE, size < 0xFFF and 0xFFF or size)
        fade_9_6(e)
        if e:joint(0x100) then
            e:set_routine(3, 0xB)
            e:set_var(PULSE_OFF, 0)
            e:clip(6, 0, 0xF)
            e:sound(0xB)
        end
    elseif three == 0xB then
        if e:joint(0x100) then
            e:set_routine(3, 0xC)
            e:set_timer(0, 0x1E)
        end
    elseif three == 0xC then
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            e:set_sce_free(1, 1)
            e:set_routine(2, 3)
            e:set_routine(3, 0)
        end
    end
    e:seq_sound()
end

-- `Routine_2` 3: a tick later, the scene's end. `Type`'s high byte, the
-- first weights of the scene's parts, `Be_flg`'s 0x800 and the neck's 1
-- clear, and the fight starts in `Routine_1` 1.
local function scene_end(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        return
    end
    e:set_kind(e:kind() & 0xFF)
    e:set_be_flg(e:be_flg() & ~0x800)
    e:set_neck_flags(e:neck_flags() & ~1)
    for _, part in ipairs(SCENE_PARTS) do
        set_weight(e, part, 0)
    end
    e:set_var(LOOK, 0)
    e:set_var(LOOK_STEP, 1)
    e:routines(1, 1, 0, 0)
    e:set_damage_cnt(e:damage_cnt() & 0x7F)
end

-- Routine 0 (`FUN_00496d00`): room 509's scene.
local function scene(e)
    local two = e:routine(2)
    if two == 0 then
        scene_wait(e)
    elseif two == 1 then
        scene_walk(e)
    elseif two == 2 then
        scene_rise(e)
    elseif two == 3 then
        scene_end(e)
    end
end

-- `WORD_0052a5ac`: routine 2's heading off a wall, by guide area.
local WALL_TURN <const> = { [0] = 0x800, 0xA00, 0xC00, 0, 0x600, 0x400, 0xC00, 0xC00 }
-- `WORD_0052a598`: each swipe's part and reach: the right and left arms,
-- then the lunge's (4, part 6).
local ARM <const> = { [0] = { 0xD, 0x6A4 }, { 0x13, 0x5DC }, { 6, 0x320 }, { 9, 0x4B0 }, { 6, 0x4B0 } }
-- `DAT_0052a688` and `DAT_0052a701`: whether routine 7 may start, by the
-- player's guide area and G3's (11 each), at full life and below half.
local ROUTINE_7 <const> = {
    [false] = {
        [0] = 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 0,
        0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1,
        1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0, 1,
        1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1,
    },
    [true] = {
        [0] = 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0,
        0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1,
        0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1,
        1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1,
    },
}
-- `DAT_0052a828`: the blood of `FUN_0049add0`, by kind: `Esp_call`'s kind,
-- row and size.
local BLOOD <const> = {
    [0] = { 0, 1, 0x1800 }, { 0, 1, 0x2000 }, nil, nil, nil, nil, { 1, 6, 0x2000 },
}

local function player_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

-- `Goto00_ck` toward the player.
local function facing(e, rate)
    local x, z = player_xz(e)
    return e:goto_ck(x, z, rate)
end

local function set_routine_1(e, one)
    e:routines(1, one, 0, 0)
end

-- `FUN_0049add0` at a part as the last draw left it (`Workm.t`), facing the
-- player. Only kinds 0, 1 and 6 are modelled; their `Esp_call`s draw no
-- `Rnd`.
local function blood(e, part, kind)
    local row = BLOOD[kind]
    if not row then
        return
    end
    local x, y, z = e:drawn_part_point(part, 0, 0, 0)
    if not x then
        return
    end
    e:call_effect(row[1], row[2], row[3], e:direction_to_player(), x, y, z)
end

-- `FUN_00497820`: whether to swipe. Toward a player near death (below 20
-- life) who doesn't face its way it starts routine 6 within 0xdac; in mode
-- 0 it swipes with the arm on the player's side (routine 4), within 3000 on
-- the left and 4000 on the right, once that arm has rested, and both arms
-- resting frees one by a `Rnd` (-1); in mode 1 it starts routine 5 within
-- 3000. 1 when it starts one.
local function attack_ck(e)
    local ahead = facing(e, 0x300)
    local life = e:player_life()
    if life < 0 then
        return 0
    end
    local distance = e:distance()
    if life < 0x14 and not e:cdir_player() then
        if e:var(CNT_21C) ~= 0 or ahead ~= 0 or distance >= 0xDAC then
            return 0
        end
        set_routine_1(e, 6)
        return 1
    end
    if e:var(MODE) == 0 then
        if e:var(CNT_21C) ~= 0 and e:var(CNT_21D) ~= 0 then
            e:set_var(CNT_21C + (e:rnd() & 1), 0)
            return -1
        end
        local rest, reach
        if facing(e, 1) < 0 then
            rest, reach = e:var(CNT_21D), 3000
        else
            rest, reach = e:var(CNT_21C), 4000
        end
        if rest == 0 and ahead == 0 and distance < reach then
            set_routine_1(e, 4)
            return 1
        end
    elseif e:var(CNT_21C) == 0 and ahead == 0 and distance < 3000 then
        set_routine_1(e, 5)
        return 1
    end
    return 0
end

-- `FUN_00497ab0`: within 0x1194 of a player it doesn't face within 0x400,
-- it turns (routine 3) on 6 `Rnd`s in 16, or always after a second swing.
local function turn_ck(e)
    if e:distance() >= 0x1194 or e:player_life() < 0 then
        return
    end
    if facing(e, 0x400) ~= 0 then
        if (1 << (e:rnd() & 0xF)) & 0x1558 ~= 0 or e:var(AGAIN) ~= 0 then
            set_routine_1(e, 3)
        end
    end
end

-- `FUN_00498f80`: once `+0x22b` has run out, where the player's and G3's
-- guide areas allow it (by whether its life is below half), routine 7 on 3
-- `Rnd`s in 16.
local function routine_7_ck(e)
    if e:player_life() < 0 or e:var(CNT_22B) ~= 0 then
        return
    end
    local px, pz = player_xz(e)
    local x, _, z = e:pos()
    local low = e:life() < (e:easy() and LIFE_EASY or LIFE) >> 1
    local allowed = ROUTINE_7[low][e:guide_at(px, pz) + e:guide_at(x, z) * 0xB]
    if allowed == 0 and (1 << (e:rnd() & 0xF)) & 0x1A0 ~= 0 then
        set_routine_1(e, 7)
    end
end

-- `FUN_00497f50`: a dead player sends it back to routine 1.
local function player_dead_ck(e)
    if e:player_life() < 0 then
        set_routine_1(e, 1)
        e:set_be_flg(e:be_flg() | 0x4A)
    end
end

-- Routine 1 (`FUN_00497600`): a roar (sound 10) on clip 0 for 60 ticks.
local function roar(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(0, 0, 0xF)
        e:set_timer(0, 0x3C)
        e:sound(10)
        e:set_neck_flags(e:neck_flags() & ~1)
    end
    e:joint(0x100)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        set_routine_1(e, 2)
    end
end

-- `FUN_00499db0`: turn by `step` toward `SIDE` as a heading; true once within
-- `step`.
local function turn_to_side(e, step)
    local off = i16(e:var(SIDE) - (e:angle() & 0xFFF))
    if abs(off) < step then
        return true
    end
    if abs(off) > 0x7FF then
        off = -off
    end
    e:set_angle(e:angle() + (off < 1 and -step or step))
    return false
end

-- Routine 2 (`FUN_00497680`): the walk to the player (clip 1, or 2 in mode
-- 1), turning faster beyond 5000; held 20 ticks against a wall, it turns to
-- its area's heading for up to 45 ticks. Each tick it may swipe, turn or
-- start routine 7.
local function walk(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_routine(2, 1)
        e:set_speed(0, 0, 0)
        e:clip(1 + e:var(MODE), 0, 0xF)
        e:set_var(AGAIN, 0)
        e:set_var(STUCK, 0)
        e:set_neck_flags(e:neck_flags() & ~1)
        two = 1
    end
    if two == 1 then
        local px, pz = player_xz(e)
        e:root_ck(px, pz)
        if e:player_life() >= 0 then
            local dx, dz = e:dest()
            local rate = e:distance() > 5000 and 0x20 or 0x18
            e:set_angle(e:angle() + e:goto_ck(dx, dz, rate))
        end
        e:spd()
        if e:var(STUCK) > 0x14 then
            e:set_routine(2, 2)
        end
    else
        if two == 2 then
            e:set_routine(2, 3)
            local x, _, z = e:pos()
            e:set_var(SIDE, WALL_TURN[e:guide_at(x, z)] or 0)
            e:set_timer(0, 0x2D)
        end
        e:spd()
        local turned = turn_to_side(e, 0x10)
        if not turned then
            e:set_timer(0, e:timer(0) - 1)
        end
        if turned or e:timer(0) == 0 then
            e:set_routine(2, 1)
            e:set_var(STUCK, 0)
        end
    end
    e:joint(0x100)
    e:add_speed_xz()
    e:seq_sound()
    if attack_ck(e) == -1 then
        e:sound(10)
    end
    turn_ck(e)
    routine_7_ck(e)
end

-- Routine 3 (`FUN_00497970`): a turn toward the player at up to 0x40 a tick
-- for 32 ticks, on the walk played at twice the speed. The third turn the
-- same way in a row goes the other way.
local function turn(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(1 + e:var(MODE), 0, 0xF)
        local way = facing(e, 0x40)
        e:set_var(SIDE, way & 0xFFFF)
        local sign = way > 0 and 1 or -1
        if i8(e:var(LAST_TURN)) == sign then
            local turns = (e:var(TURNS) + 1) & 0xFF
            e:set_var(TURNS, turns)
            if turns > 2 then
                e:set_var(SIDE, -way & 0xFFFF)
                e:set_var(TURNS, 0)
            end
        end
        e:set_var(LAST_TURN, sign & 0xFF)
        e:set_timer(0, 0x20)
        e:set_var(AGAIN, 0)
        e:set_neck_flags(e:neck_flags() & ~1)
    end
    local step = i16(e:var(SIDE))
    e:set_angle(e:angle() + step)
    local ahead = facing(e, 0x40)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 or ahead == 0 or step == 0 then
        set_routine_1(e, 2)
        attack_ck(e)
        routine_7_ck(e)
    end
    e:joint(0x100)
    e:seq_sound()
    e:joint(0x100)
    e:seq_sound()
end

-- `FUN_0049b0b0`: the player within the arm's reach of its part (as the last
-- draw left it), and within 0xce4 of G3's height.
local function arm_reaches(e, side)
    if e:pl_damage_cnt() & 0x80 ~= 0 or e:player_life() < 0 then
        return false
    end
    local part, reach = ARM[side][1], ARM[side][2]
    local x, y, z = e:drawn_part_point(part, 0, 0, 0)
    if not x then
        return false
    end
    local px, pz = player_xz(e)
    local dx, dz = px - x, pz - z
    local _, my = e:pos()
    return dx * dx + dz * dz < reach * reach and abs(my - y) < 0xCE4
end

-- `FUN_0049b130`: the swipe's hit, 20 life; the player is knocked back on
-- G3's clips (`em_damage_table[0x33]`, `Routine_1` by whether they face its
-- way), or falls if `B221`. A kill sends them to routine 3. 1 on a hit, -1
-- on a kill.
local function swipe_hit(e, side)
    if not arm_reaches(e, side) then
        return 0
    end
    if e:pl_life_down(0x14, false) == 2 then
        e:pl_set_routine(3, 0, 0, 0)
        return -1
    end
    local same = e:cdir_player() and 1 or 0
    if e:var(B221) == 0 then
        e:pl_set_routine(5, same, 0, 0)
        e:pl_hold()
    else
        e:pl_set_routine(2, same + 2, 0, 0)
    end
    return 1
end

-- Routine 4's `Routine_2` 0 (`0x497b90`): the swipe's start, with the arm on
-- the player's side (clip 7 right, 8 left), and whether it may swing twice
-- (`Timer1`, 4 `Rnd`s in 16).
local function swipe_start(e)
    e:set_routine(2, 1)
    if e:routine(3) == 0 then
        e:set_var(SIDE, facing(e, 1) < 0 and 1 or 0)
        e:set_timer(1, (1 << (e:rnd() & 0xF)) & 0x5A0)
    end
    e:clip(7 + e:var(SIDE), 0, 0xF)
    e:spd()
    e:joint(0x100)
    e:set_timer(2, 0)
    e:set_timer(3, 0)
    e:set_neck_flags(e:neck_flags() | 1)
    e:set_var(TURNS, 0)
end

-- Routine 4's `Routine_2` 1 (`0x497c50`): the swing. At frame 0x20000 it may
-- swing again with the other arm (turning to the player, unless it faces
-- them within 0x440 and 12 `Rnd`s in 16 let it carry on); its hit frame
-- strikes the player (sound 3, blood for three ticks). At the end the arm
-- rests `(Rnd() & 7) + Timer2 + 10` ticks, unless it swung twice.
local function swing(e)
    e:spd()
    local ended = e:joint(0x100)
    local seq = e:now_seq()
    local side = e:var(SIDE)
    local carry = false
    if not ended then
        if e:routine(3) == 0 and e:timer(1) ~= 0 and seq & 0x20000 ~= 0 then
            if facing(e, 0x440) ~= 0 and (1 << (e:rnd() & 0xF)) & 0x5A0 == 0 then
                e:set_routine(3, 1)
                e:set_var(AGAIN, 1)
                carry = true
            end
            if not carry then
                e:goto_player(0x40)
                e:set_routine(2, 0)
                e:set_routine(3, 1)
                e:set_var(SIDE, side ~ 1)
            end
        end
    else
        set_routine_1(e, 2)
        if e:var(AGAIN) == 0 then
            e:set_var(CNT_21C + side, ((e:rnd() & 7) + e:timer(2) + 10) & 0xFF)
        end
        turn_ck(e)
        player_dead_ck(e)
        e:set_var(AGAIN, 0)
        e:set_var(CNT_22B, ((e:timer(2) + 0xF) * 4) & 0xFF)
    end
    e:add_speed_xz()
    e:seq_sound()
    if seq & 0x10000 ~= 0 and e:timer(2) == 0 then
        if swipe_hit(e, side) ~= 0 then
            e:sound(3)
            e:set_timer(2, 0xF)
            e:set_timer(3, 3)
        end
    end
end

-- Routine 4 (`FUN_00497b20`): the swipe. Its wall strike (`FUN_00497df0`)
-- and the blood at the player's part 8 aren't modelled.
local function swipe(e)
    local two = e:routine(2)
    if two == 0 then
        swipe_start(e)
    elseif two == 1 then
        swing(e)
    end
    local part = ARM[i16(e:var(SIDE))] and ARM[i16(e:var(SIDE))][1] + 1
    if part and e:timer(3) ~= 0 then
        blood(e, part, 1)
        blood(e, part + 2, 1)
        e:set_timer(3, e:timer(3) - 1)
    end
end

-- Routine 6's `Routine_2` 0: the lunge at a player near death (clip 6,
-- turning by up to 0x10 a tick until it faces them within 0x40, then clip
-- 11 and clip 12). Its hit frame within 0x4b0 of part 6 takes 5 life and
-- knocks them down (2, 2 or 3), or grabs a player below 20 who doesn't face
-- its way for the kill.
local function lunge(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_routine(3, 1)
        e:clip(6, 0, 0xF)
        e:spd()
        e:joint(0x100)
        e:add_speed_xz()
        e:set_neck_flags(e:neck_flags() | 1)
    elseif three == 1 then
        e:spd()
        e:joint(0x100)
        e:add_speed_xz()
        if e:now_seq() & 0x10000 ~= 0 then
            e:set_routine(3, 2)
            e:set_var(SIDE, 0)
            if facing(e, 0x40) ~= 0 then
                e:set_var(SIDE, facing(e, 0x10) & 0xFFFF)
            end
        end
    elseif three == 2 or three == 4 then
        e:set_routine(3, three + 1)
        e:clip(three == 2 and 0xB or 0xC, 0, 0xF)
        e:spd()
        e:joint(0x100)
    elseif three == 3 then
        e:set_angle(e:angle() + i16(e:var(SIDE)))
        e:spd()
        e:joint(0x100)
        e:add_speed_xz()
        if e:now_seq() & 0x10000 ~= 0 and arm_reaches(e, 4) then
            if e:player_life() < 0x14 and not e:cdir_player() then
                e:routines(1, 6, 1, 0)
                e:sound(4)
                return
            end
            if e:pl_life_down(5, false) == 2 then
                e:pl_set_routine(3, 0, 0, 0)
            else
                e:pl_set_routine(2, (e:cdir_player() and 1 or 0) + 2, 0, 0)
            end
            e:sound(0xD)
            blood(e, 6, 1)
        end
        if e:now_seq() & 0x20000 ~= 0 then
            e:set_routine(3, 4)
        end
    elseif three == 5 then
        e:spd()
        if e:joint(0x100) then
            set_routine_1(e, 2)
            e:set_var(MODE, 0)
            e:set_var(CNT_21C, 0x3C)
            e:sound(10)
            player_dead_ck(e)
        end
        e:add_speed_xz()
    end
end

-- Routine 6's `Routine_2` 1: the kill. The player dies in its grip (player
-- routine 6, on its second bank, turned to face it), and clip 11 plays on
-- with its blood, three `Rnd`s at frame 0x100000 and one at 0x200000.
local function kill(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_be_flg(e:be_flg() | 2)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:pl_hold()
        e:pl_set_routine(6, 0, 0, 0)
        e:pl_set_sub1()
        e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        e:pl_set_base()
        e:set_player_angle((e:angle() + 0x800) & 0xFFFF)
        e:pl_or_be_flg(0x4A)
        e:set_var(LOOK, 1)
        e:set_var(LOOK_STEP, 1)
    end
    e:spd()
    if e:joint(0x100) then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
    end
    e:add_speed_xz()
    e:seq_sound()
    local seq = e:now_seq()
    if seq & 0x100000 ~= 0 then
        if (1 << (e:rnd() & 0xF)) & 0x5FFA ~= 0 then
            blood(e, 6, e:rnd() & 1)
        end
        local _ = e:rnd()
    end
    if seq & 0x200000 ~= 0 then
        blood(e, 0xF, 1)
        blood(e, 0x15, 1)
        if (1 << (e:rnd() & 0xF)) & 0x17E4 ~= 0 then
            for _, part in ipairs({ 0xE, 0x10, 0x14, 0x16 }) do
                blood(e, part, 0)
            end
        end
    end
end

-- Routine 6's `Routine_2` 2: after the kill, clip 6 with a roar, then clip
-- 0 held.
local function after_kill(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_routine(3, 1)
        e:clip(6, 0, 0xF)
        e:spd()
        e:joint(0x100)
        e:sound(10)
    elseif three == 1 then
        e:spd()
        if e:joint(0x100) then
            e:set_routine(3, 2)
            e:clip(0, 0, 0xFF)
        end
        e:add_speed_xz()
    elseif three == 2 then
        e:joint(0xF)
    end
end

-- Routine 6 (`FUN_00498360`). Its jumps (`Routine_2` 3 to 5) aren't ported.
local function finish(e)
    local two = e:routine(2)
    if two == 0 then
        lunge(e)
    elseif two == 1 then
        kill(e)
    elseif two == 2 then
        after_kill(e)
    end
end

-- `FUN_00496c90`: the move, by `Routine_1`.
local function move(e)
    local one = e:routine(1)
    if one == 0 then
        scene(e)
    elseif one == 1 then
        roar(e)
    elseif one == 2 then
        walk(e)
    elseif one == 3 then
        turn(e)
    elseif one == 4 then
        swipe(e)
    elseif one == 6 then
        finish(e)
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

-- `FUN_0049aad0` on `LOOK` 1: every `(Rnd() & 7) * LOOK_STEP` ticks the
-- eye darts by `(Rnd() & 0xf) - 7`, across, up, both or neither by a third
-- `Rnd`.
local function look_dart(e)
    local count = e:var(LOOK_CNT)
    if count ~= 0 then
        e:set_var(LOOK_CNT, count - 1)
        return
    end
    local step = (e:rnd() & 0xF) - 7
    local which = e:rnd()
    if which & 1 ~= 0 then
        look(e, EYE_X, step)
    end
    if which & 2 ~= 0 then
        look(e, EYE_Y, step)
    end
    e:set_var(LOOK_CNT, ((e:rnd() & 7) * e:var(LOOK_STEP)) & 0xFF)
end

-- `FUN_0049aad0` on other `LOOK`s: a blink, part 0xb's morph closing over
-- 9 ticks and opening over 8, after which the eye looks around again.
local function blink(e)
    local step = e:var(LOOK_STEP)
    local count = e:var(LOOK_CNT)
    if step == 0 then
        e:set_var(LOOK_STEP, 1)
        step = 1
        count = 0
    end
    if step == 1 then
        count = count + 1
        if count > 8 then
            e:set_var(LOOK_STEP, 2)
            count = 8
        end
    elseif step == 2 then
        count = count - 1
        if count == 0 then
            e:set_var(LOOK, 0)
            e:set_var(LOOK_STEP, 0)
        end
    end
    e:set_var(LOOK_CNT, count)
    set_weight(e, 0xB, count << 9)
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
    local mode = e:var(LOOK)
    if mode == 0 then
        look_around(e)
    elseif mode == 1 then
        look_dart(e)
    else
        blink(e)
    end
    pulse(e)
end
