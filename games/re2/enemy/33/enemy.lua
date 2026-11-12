-- G3, William Birkin's third form, `Em_33` (entity id 0x33).
--
-- `Em_33` runs `Routine_0` from `0x52a5f0`: 0 is the init (`FUN_00496a80`)
-- and 1 the move (`FUN_00496c90`), which runs `Routine_1` from `0x52a610`.
-- Of those, routine 0 (`FUN_00496d00`) is room 509's scene: G3 waits off
-- the stage, walks in and rises as the room's script steps `Sce_free0`
-- through 1, 2 and 3, and starts the fight in routine 1. `Routine_0` 2 is the
-- hit (`FUN_00499210`) and 3 the death (`FUN_00499e00`).
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
local B221 <const> = 23 -- `+0x221`: it is off the floor (a jump or the death's climb); the swipe throws the player down rather than back.
-- `+0x21e` to `+0x220`: the routines the move left, which a hit goes back
-- to.
local SAVED_1 <const> = 24
local SAVED_2 <const> = 25
local SAVED_3 <const> = 26
local FLINCHED <const> = 27 -- `+0x234`: a flinch (routine 8 or 9) is under way.
-- `+0x23c` to `+0x23e`: a magnum's spurt (`FUN_00499750`), its count of
-- drops and its heading.
local SPURT <const> = 28
local SPURT_N <const> = 29
local SPURT_DIR <const> = 30
-- The grip of this frame (`pNow_seq`'s bits 20 to 23), which the player's
-- knock-back reads (the engine's `G3_GRIP`).
local GRIP <const> = 31

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
    for index = CNT_219, GRIP do
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
-- `DAT_0052a828`: the blood of `FUN_0049add0` and `FUN_0049af50`, by kind:
-- `Esp_call`'s kind, row and size, and whether it follows the part's matrix.
local BLOOD <const> = {
    [0] = { 0, 1, 0x1800 },
    { 0, 1, 0x2000 },
    { 5, 4, 0x1800, true },
    { 5, 1, 0x1000, true },
    { 4, 15, 0x1000, true },
    { 4, 15, 0x1800, true },
    { 1, 6, 0x2000 },
    { 6, 0, 0x1000, true },
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
local function blood(e, part, kind, ox)
    local row = BLOOD[kind]
    if not row then
        return
    end
    local dir = i16(e:direction_to_player())
    if row[4] then
        e:effect(row[1], row[2], row[3], dir, part, ox or 300, 0, 0)
        return
    end
    local x, y, z = e:drawn_part_point(part, 0, 0, 0)
    if not x then
        return
    end
    e:call_effect(row[1], row[2], row[3], dir, x + (ox or 0), y, z)
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
    if e:pl_damage_cnt() & 0x80 ~= 0 or e:player_life() < 0 or not ARM[side] then
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

-- `FUN_0049b230`: routine 5's hits, 15 life (5 for reaches 2 and 3). The
-- first (0) knocks the player down (routine 2, 4 or 5); the others knock
-- them back in G3's grip (`em_damage_table[0x33]`'s `Routine_1` 2 or 3),
-- with `Timer0` by the reach's side. 1 on a hit, -1 on a kill.
local function grip_hit(e, reach)
    if not arm_reaches(e, reach) then
        return 0
    end
    if e:pl_life_down(reach & 2 ~= 0 and 5 or 15, false) == 2 then
        e:pl_set_routine(3, 0, 0, 0)
        return -1
    end
    local same = e:cdir_player() and 1 or 0
    if reach == 0 then
        e:pl_set_routine(2, same + 4, 0, 0)
        e:pl_set_timer(0, 0)
    else
        e:pl_set_routine(5, same + 2, 0, 0)
        e:pl_hold()
        e:pl_set_timer(0, reach & 1)
    end
    return 1
end

-- `WORD_0052a594`: routine 5's blood, by `Timer2`: the part (its first two
-- entries are the table before it).
local GRIP_BLOOD <const> = { [0] = 0xFA, 0xD, 0x13, 6, 9, 6 }

-- Routine 5 (`FUN_00497f70`), in mode 2: the second swipes. `Routine_2` 0
-- turns to the player on clip 9 and grabs with a hit frame from 0x70000
-- (reach 3), or goes on to clip 12 at 0x80000 and back to the walk with the
-- right arm resting 30 ticks. `Routine_2` 1 strikes on each frame's reach
-- (bits 16 to 18), with sound 3, or 13 for the far reaches, and blood. 2
-- roars (clip 6) and waits 31 ticks on clip 0, then walks with the right
-- arm resting 60 ticks. The wall strikes (`FUN_00497df0`) aren't modelled.
local function swipe_2(e)
    local two = e:routine(2)
    if two == 0 then
        local three = e:routine(3)
        if three == 0 then
            e:set_routine(3, 1)
            e:clip(9, 0, 0xF)
            e:spd()
            e:joint(0x100)
            e:set_var(SIDE, facing(e, 0x20) & 0xFFFF)
            e:set_var(TURNS, 0)
            e:set_neck_flags(e:neck_flags() | 1)
        elseif three == 1 then
            e:spd()
            e:joint(0x100)
            e:add_speed_xz()
            e:set_angle(e:angle() + i16(e:var(SIDE)))
            if e:now_seq() & 0x70000 ~= 0 and grip_hit(e, 3) ~= 0 then
                e:sound(0xD)
                e:routines(1, 5, 1, 0)
                e:seq_sound()
                return
            end
            if e:now_seq() & 0x80000 ~= 0 then
                e:set_routine(3, 2)
                e:clip(0xC, 0, 0xF)
                e:spd()
                e:joint(0x100)
                e:seq_sound()
                return
            end
        elseif three == 2 then
            e:spd()
            if e:joint(0x100) then
                set_routine_1(e, 2)
                e:set_var(MODE, 0)
                e:set_var(CNT_21C, 0x1E)
                e:sound(10)
            end
            e:add_speed_xz()
        end
        e:seq_sound()
    elseif two == 1 then
        if e:routine(3) == 0 then
            e:set_routine(3, 1)
            e:set_timer(2, 4)
            e:set_timer(3, 4)
        end
        e:spd()
        if e:joint(0x100) then
            e:routines(1, 5, 2, 0)
        end
        e:add_speed_xz()
        e:seq_sound()
        local reach = (e:now_seq() >> 16) & 7
        if reach ~= 0 and i8(e:timer(2)) ~= reach and grip_hit(e, reach - 1) ~= 0 then
            e:sound(reach > 2 and 13 or 3)
            e:set_timer(2, reach)
            e:set_timer(3, 4)
        end
        if e:timer(3) ~= 0 then
            local index = i8(e:timer(2))
            local part = GRIP_BLOOD[index]
            if part then
                if index < 3 then
                    blood(e, part + 1, 1)
                    blood(e, part + 3, 1)
                else
                    blood(e, part, 1)
                end
            end
            e:set_timer(3, e:timer(3) - 1)
        end
    elseif two == 2 then
        local three = e:routine(3)
        if three == 0 then
            e:set_routine(3, 1)
            e:clip(6, 0, 0xF)
            e:spd()
            e:joint(0x100)
            e:sound(0xB)
        elseif three == 1 then
            e:spd()
            if e:joint(0x100) then
                e:set_routine(3, 2)
                e:set_timer(0, 0x1F)
                e:clip(0, 0, 0x3F)
                e:joint(0x40)
            end
            e:add_speed_xz()
        elseif three == 2 then
            e:joint(0x40)
            e:set_timer(0, e:timer(0) - 1)
            if e:timer(0) == 0 then
                set_routine_1(e, 2)
                e:set_var(MODE, 0)
                e:set_var(CNT_21C, 0x3C)
                player_dead_ck(e)
            end
        end
    end
end

-- Routine 8 (`FUN_00499050`): the flinch at 600 life or more (clip 4, sound
-- 10); at its frame 0x10000 it walks on, or starts routine 7 where the
-- guide areas allow it.
local function flinch(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_routine(2, 1)
        e:clip(4, 0, 0xF)
        e:spd()
        e:joint(0x100)
        e:sound(10)
        return
    end
    e:spd()
    if two == 1 then
        e:joint(0x100)
        if e:now_seq() & 0x10000 ~= 0 then
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
            local px, pz = player_xz(e)
            local x, _, z = e:pos()
            if ROUTINE_7[true][e:guide_at(px, pz) + e:guide_at(x, z) * 0xB] == 0 then
                e:set_var(FLINCHED, 0)
                e:routines(1, 7, 0, 0)
            else
                e:set_routine(2, 2)
                e:set_routine(3, 0)
            end
        end
    elseif two == 2 then
        if e:joint(0x100) then
            e:routines(1, 2, 0, 0)
            e:set_var(FLINCHED, 0)
        end
    end
    e:add_speed_xz()
end

-- Routine 9 (`FUN_00499170`): the flinch below 600 life (clip 6, sound 11),
-- after which it walks in mode 2.
local function flinch_low(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_var(MODE, 2)
        e:clip(6, 0, 0xF)
        e:spd()
        e:joint(0x100)
        e:sound(0xB)
        return
    end
    e:spd()
    if e:joint(0x100) then
        e:routines(1, 2, 0, 0)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:set_var(FLINCHED, 0)
    end
    e:add_speed_xz()
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
    elseif one == 5 then
        swipe_2(e)
    elseif one == 6 then
        finish(e)
    elseif one == 8 then
        flinch(e)
    elseif one == 9 then
        flinch_low(e)
    end
    -- `FUN_00496c90` keeps the routines for a hit to go back to.
    e:set_var(SAVED_1, e:routine(1))
    e:set_var(SAVED_2, e:routine(2))
    e:set_var(SAVED_3, e:routine(3))
end

-- `BYTE_0052a5c3`: the kind of a hit's blood by weapon; `BYTE_0052a5d8`:
-- the parts it comes from, by `Damage_no`; `BYTE_0052a5dc`: the parts of
-- the blood of weapons 4, 0xf and 0x12, by `Damage_no` and a `Rnd`.
local HIT_KIND <const> = { [0] = 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 2, 1, 0, 0 }
local HIT_PART <const> = { [0] = 0x18, 1, 3 }
-- `BYTE_0052a7cb` and `BYTE_0052a7df`: what each weapon takes from
-- `AT_EM_FLG`, which flinches it at 0, at 600 life or more and below.
local STUN_HIGH <const> = { [0] = 0, 10, 10, 10, 10, 20, 10, 10, 10, 10, 40, 10, 10, 10, 20, 4, 4, 40, 4, 10 }
local STUN_LOW <const> = { [0] = 10, 10, 10, 10, 10, 20, 10, 20, 40, 20, 40, 40, 10, 10, 40, 10, 20, 40, 4, 10 }

-- `DWORD_0052a77c`: what each weapon's hit adds. The magnums start the
-- spurt; the others' (9 to 0xb, 0xe, 0x10 and 0x11) aren't ported, and
-- 4, 0xf and 0x12's blood draws its `Rnd` only, as does the Beretta's
-- (`0x52a7c8`, `FUN_004995c0`: three of kind 2, which `blood` doesn't
-- model). The Colt's (`0x52a7b0`) is `nullsub_1`.
local function weapon_blood(e, weapon)
    if weapon == 5 or weapon == 6 then
        e:set_var(SPURT, 1)
    elseif weapon == 4 or weapon == 0xF or weapon == 0x12 then
        e:rnd()
    elseif weapon == 0x13 then
        for _ = 1, 3 do
            e:rnd()
        end
    end
end

local function flinch_to(e, one)
    e:set_var(AGAIN, 0)
    e:set_var(AT_EM_FLG, 0x28)
    e:set_var(FLINCHED, 1)
    e:routines(1, one, 0, 0)
    e:set_var(SAVED_1, one)
    e:set_var(SAVED_2, 0)
    e:set_var(SAVED_3, 0)
end

-- Routine 0 2's first tick (`FUN_00499260`): a hit. The eye blinks, a sound
-- and blood; each hit wears `AT_EM_FLG` down by the weapon's share, and
-- when it runs out it flinches (routine 8 at 600 life or more, 9 below in
-- mode 0); otherwise it goes on with what the move left on this tick.
local function hit(e)
    local weapon = e:routine(1)
    weapon_blood(e, weapon)
    e:set_var(LOOK, 2)
    e:set_var(LOOK_STEP, 0)
    if weapon ~= 0x10 then
        e:sound(2)
    end
    if weapon ~= 1 then
        local part = HIT_PART[e:damage_no() % 3]
        local kind = HIT_KIND[weapon] or 0
        blood(e, part, kind, 300)
        blood(e, part, kind, -100)
    end
    if e:player_life() > 0x13 and e:var(B221) == 0 and e:var(FLINCHED) == 0 then
        local stun = i16(e:var(AT_EM_FLG))
        if e:life() >= (e:easy() and 500 or 600) then
            stun = stun - (STUN_HIGH[weapon] or 0)
            e:set_var(AT_EM_FLG, stun & 0xFFFF)
            if stun < 1 then
                flinch_to(e, 8)
                return
            end
        else
            stun = stun - (STUN_LOW[weapon] or 0)
            e:set_var(AT_EM_FLG, stun & 0xFFFF)
            if stun < 1 then
                e:set_var(AGAIN, 0)
                e:set_var(AT_EM_FLG, 0x28)
                if e:var(MODE) == 0 then
                    flinch_to(e, 9)
                    return
                end
            end
        end
    end
    e:routines(1, e:var(SAVED_1), e:var(SAVED_2), e:var(SAVED_3))
    move(e)
end

-- Routine 0 2 (`FUN_00499210`): the hit, or (`FUN_004994f0`) a stagger
-- played to its end.
local function hurt(e)
    if e:routine(2) == 0 then
        hit(e)
        return
    end
    e:spd()
    if e:joint(0x100) then
        e:routines(1, 2, 0, 0)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:set_var(SAVED_1, 2)
        e:set_var(SAVED_2, 0)
        e:set_var(SAVED_3, 0)
    end
    e:add_speed_xz()
end

-- Routine 0 3 (`FUN_00499e00`): the death. It staggers (clip 4), turns to
-- its guide area's heading on clip 2 until a wall stops it facing within
-- 0x3ff, climbs it (clips 17, 18 twice and 19), leaps (clip 15 from 1500
-- or 1600 up, by the heading), falls back (clip 20, its dead flag), and
-- sinks to -13000. Each tick one `Rnd` may spill blood; off the floor, two
-- may.
local function death(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_routine(2, 1)
        e:clip(4, 0, 0xF)
        e:spd()
        e:joint(0x100)
        local x, _, z = e:pos()
        local heading = WALL_TURN[e:guide_at(x, z)] or 0
        e:set_timer(0, 0x2D)
        e:set_var(SIDE, heading)
        e:set_timer(2, heading & 0x3FF ~= 0 and 1 or 0)
        e:set_timer(3, 2)
        weapon_blood(e, e:routine(1))
        e:set_var(B221, 0)
        e:sound(10)
        e:set_neck_flags(e:neck_flags() | 1)
    elseif two == 1 then
        e:spd()
        if e:joint(0x100) then
            e:set_routine(2, 2)
            e:clip(2, 0, 0xF)
            e:spd()
            e:joint(0x100)
            e:set_be_flg(e:be_flg() | 4)
        else
            e:add_speed_xz()
        end
    elseif two == 2 then
        turn_to_side(e, 0x10)
        e:spd()
        e:joint(0x100)
        if e:sca_info() & 1 == 0 or abs(i16(e:var(SIDE)) - (e:angle() & 0xFFF)) > 0x3FF then
            e:seq_sound()
            e:add_speed_xz()
        else
            e:set_routine(2, 3)
            e:clip(0x11, 0, 0xF)
            e:spd()
            e:joint(0x100)
            e:sound(0xC)
        end
    elseif two == 3 then
        turn_to_side(e, 0x18)
        e:spd()
        e:joint(0x100)
        e:add_speed_xz()
        if e:now_seq() & 0x10000 ~= 0 then
            e:set_routine(2, 4)
            e:clip(0x12, 0, 0xF)
        end
    elseif two == 4 then
        if e:joint(0x100) then
            e:set_hokan(0xF)
        end
        if e:now_seq() & 0x10000 ~= 0 then
            local count = (e:timer(3) - 1) & 0xFF
            e:set_timer(3, count)
            if count == 0 then
                e:set_routine(2, 5)
                e:clip(0x13, 0, 0xF)
                e:set_be_flg(e:be_flg() | 0x4A)
            end
        end
    elseif two == 5 then
        e:spd()
        e:joint(0x100)
        if e:now_seq() & 0x10000 ~= 0 then
            e:set_routine(2, 6)
            e:set_timer(3, 0)
            local _, _, sz = e:speed()
            e:set_speed(0xDC, -1000, sz)
            e:set_var(B221, 1)
            e:set_be_flg(e:be_flg() | 0x400)
        end
    elseif two == 6 then
        if e:timer(3) == 0 and e:joint(0x100) then
            e:set_timer(3, 1)
        end
        e:add_speed_xyz()
        local high = e:timer(2) ~= 0
        local sx, sy, sz = e:speed()
        sy = i16(sy + 0xA0)
        e:set_speed(sx, sy, sz)
        local top = high and -1500 or -1600
        local _, y = e:pos()
        if sy > 0 and y > top then
            e:set_routine(2, 7)
            e:set_pos_y(top)
            e:clip(0xF, 0, 1)
            e:sound(high and 9 or 8)
        end
    elseif two == 7 then
        if e:joint(0x800) then
            e:set_routine(2, 8)
            local sx, _, sz = e:speed()
            e:set_speed(sx, -900, sz)
            e:clip(0x14, 0, 3)
            e:sound(6)
            e:set_died()
        end
    elseif two == 8 then
        e:joint(0x400)
        e:add_speed_xyz()
        local sx, sy, sz = e:speed()
        sy = i16(sy + 0xA0)
        e:set_speed(sx, sy, sz)
        if sy >= 0 then
            e:set_routine(2, 9)
            e:sound(0xE)
        end
    elseif two == 9 then
        if e:joint(0x400) then
            e:set_routine(2, 10)
        end
    elseif two == 10 then
        local _, y = e:pos()
        y = y - 0xB4
        e:set_pos_y(y)
        if y < -13000 then
            e:set_routine(2, 11)
        end
    elseif two == 11 then
        e:set_routine(2, 12)
    end
    if e:routine(2) > 0xB then
        return
    end
    if e:var(B221) == 0 then
        if e:rnd() & 0xF == 0xF then
            blood(e, HIT_PART[e:damage_no() % 3], 0)
        end
    else
        local _, y = e:pos()
        if y >= -9999 and (1 << (e:rnd() & 0xF)) & 0x17E4 ~= 0 then
            if (1 << (e:rnd() & 0xF)) & 0x17E4 ~= 0 then
                blood(e, 2, 9)
            end
        end
    end
end

-- `FUN_00499750`: a magnum's spurt out of G3's far side, toward where the
-- player was when it began, for three ticks: eight drops a tick along a
-- line from part 9's height, each other one scattered by two `Rnd`s, 20
-- `Rnd`s a tick in all.
local function spurt(e)
    local state = e:var(SPURT)
    if state == 1 then
        e:set_var(SPURT, 2)
        e:set_var(SPURT_N, 0)
        e:set_var(SPURT_DIR, e:direction_to_player())
    elseif state ~= 2 then
        return
    end
    local dir = i16(e:var(SPURT_DIR))
    local n = e:var(SPURT_N)
    local ax, ay, az = e:rot_apply(0, dir, 0, i16(n * -100), 200, 0)
    local step_y = e:damage_no() % 3 == 0 and 500 or -500
    local bx, _, bz = e:rot_apply(0, dir, 0, -100, step_y, 0)
    local x, _, z = e:pos()
    x, z = i16(x), i16(z)
    local _, part_y = e:drawn_part_point(9, 0, 0, 0)
    local y = i16((part_y or 0) + ay)
    for drop = 1, 8 do
        local k = drop - 1
        local px, pz, row, size
        if drop % 2 == 1 then
            px = i16(bx * k + x + ax)
            pz = i16(bz * k + z + az)
            size = 0x65C - n * 0x24 - e:rnd()
            row = 1
        else
            px = i16(bx * k - e:rnd() + x + 0x80 + ax)
            pz = i16(bz * k - e:rnd() + z + 0x80 + az)
            size = 0x65C - n * (drop == 8 and 0x1E or 0x24) - e:rnd()
            row = e:rnd() & 1 == 0 and 1 or 0
        end
        e:call_effect(0, row, size & 0xFFFF, dir, px, y, pz)
        if drop == 2 and n == 1 then
            e:call_effect(0, 2, 0x17D0, i16(dir + 0x800), px, y, pz)
            e:call_effect(0, 2, 0xC18, i16(dir + 0x800), px, y, pz)
        end
        local was = n
        n = (n + 1) & 0xFF
        if drop == 8 and was > 0x10 then
            e:set_var(SPURT, 0)
        end
    end
    e:set_var(SPURT_N, n)
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
    elseif zero == 2 then
        hurt(e)
    elseif zero == 3 then
        death(e)
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
    spurt(e)
    e:set_var(GRIP, (e:now_seq() >> 20) & 0xF)
end
