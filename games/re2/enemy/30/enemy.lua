-- G1, William Birkin's first form, `Em_30` (entity id 0x30).
--
-- `Em_30` runs `Routine_0` from `funcs_48A474` (`0x52a248`): 0 is the
-- init (`FUN_0048a540`) and 1 the move
-- (`FUN_0048ab30`); the hit (2, `FUN_0048dc70`) and the death (3,
-- `FUN_0048f050`) aren't ported yet. The move runs by `Type & 0x3f`: 0 to 4
-- run `funcs_48AB5B` before the clip and `funcs_48AB72` after it, by
-- `Routine_1`, and 5 runs `FUN_0048ff00`. Of those, routine 10 is the first
-- encounter's scene in room 303 (`Type` 0), where it waits until the room's
-- event raises `Sce_free0`, then changes and goes on to routine 1, the
-- fight's walk, which chooses its attacks (2 to 5 and 7). The attacks aren't
-- ported yet.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and
-- `e:set_var` are the fields of the original's work that the engine doesn't
-- read.

local CNT_218 <const> = 0 -- `+0x218`, which counts down from 15 while `FLAGS`' 2.
local CNT_219 <const> = 1 -- `+0x219`, how many times `+0x218` runs out before `FLAGS`' 2 clears.
local CNT_21A <const> = 2 -- `+0x21a`.
local FLAGS <const> = 3 -- `+0x21c`: 0x40 keeps `Em_seq_sound` quiet.
local WAVE <const> = 4 -- `+0x21e`: a count of ticks, mod 64, that the parts' swell reads.
local CNT_22C <const> = 5 -- `+0x22c`.
local TEXTURE <const> = 6 -- `+0x224` to `+0x226`: the eye's texture's frames (`FUN_0048fba0`), not drawn.

-- The init's life: normal, with `fg_system`'s `0x20`, and in hard mode.
local LIFE <const> = 500
local LIFE_EASY <const> = 400

-- `Type`'s bit that the first encounter's scene holds until it's done.
local TYPE_SCENE <const> = 0x4000

-- `Type`'s low bits, and the `Routine_1` the init starts each at.
local START <const> = { [1] = 1, [2] = 11, [3] = 12, [4] = 13, [5] = 0 }

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

-- `Move_no`, `Move_cnt`, `Hokan_flg` and `Mplay_flg`, as the routines write
-- them: a clip from its start.
local function set_clip(e, clip)
    e:clip(clip, 0, 0x1F)
end

-- `Spd_get`, `Joint_move` with 0x80 and `Add_speedXZ`: whether the clip ended.
local function play(e)
    e:spd()
    local ended = e:joint(0x80)
    e:add_speed_xz()
    return ended
end

-- `Timer0`'s count down, as a word: whether it was 0, and what it is now.
local function timer0_down(e)
    local was = e:timer(0)
    e:set_timer(0, was - 1)
    return was == 0, i16(was - 1)
end

-- `Timer2` flips its sign, as a byte.
local function flip_timer2(e)
    e:set_timer(2, -e:timer(2))
end

-- `FUN_0048a540`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    e:set_life(e:easy() and LIFE_EASY or LIFE)
    e:set_neck_flags(0)
    e:set_d_life(0x82)
    for index = CNT_218, TEXTURE do
        e:set_var(index, 0)
    end
    -- `Atd[0]`: `W` and `D` 800, `Ofs_y` -1500, `At_w` and `At_d` 800 and
    -- `At_h` 1500.
    e:set_body_box(-1500, 800, 1500, 800)
    e:set_body_ofs_x(0)
    e:set_be_flg(e:be_flg() | 0xC000004)
    e:clip(0, 0, 0)
    e:joint(0x100)
    local kind = e:kind()
    local low = kind & 0xFFF
    if low == 1 then
        e:set_routine(1, 1)
        return
    end
    if low == 3 then
        e:set_routine(1, 12)
        e:set_kind(kind & 0xF000)
        return
    end
    e:set_routine(1, 10)
    kind = kind | TYPE_SCENE
    e:set_kind(kind)
    -- The parts start smaller (`Size_x` to `_z` 0xc28, 0xeb8 and 0xc28) and
    -- grow in the scene; arklay draws them at their own size.
    e:set_parts0_y(0xEB8)
    if START[low] then
        e:set_routine(1, START[low])
    end
    if low == 4 then
        e:set_kind(kind & 0xF000)
    end
end

-- Routine 10 (`0x48c470`): the first encounter's scene. It waits with clip
-- 14 until the room's event raises `Sce_free0`'s bit 0, plays clip 18, holds
-- clip 15's first frame for 90 ticks, plays it, holds clip 19's first frame
-- for 90 ticks and plays it, and goes on to routine 1. What it does to its
-- parts (their swell, their growth, its arm's model, the eye's texture) and
-- its blood at clip 15's frame 15 aren't drawn.
local function scene(e)
    local sub = e:routine(2)
    if sub == 0 then
        set_clip(e, 0xE)
        e:set_timer(0, 0x78)
        e:set_neck_flags(e:neck_flags() | 2)
        if e:sce_free(0) & 1 ~= 0 then
            e:set_routine(2, 1)
        end
    elseif sub == 1 then
        play(e)
        if timer0_down(e) then
            e:set_routine(2, 2)
            set_clip(e, 0x12)
            e:set_timer(3, 0)
            e:set_timer(0, 0x50)
            e:set_timer(1, 5)
            e:set_timer(2, 1)
        end
    elseif sub == 2 then
        e:spd()
        if e:joint(0x80) then
            e:set_routine(2, 3)
            set_clip(e, 0xF)
            e:set_timer(0, 0x5A)
            e:set_timer(1, 5)
            e:set_timer(2, 1)
        end
        e:add_speed_xz()
    elseif sub == 3 then
        local was_zero, now = timer0_down(e)
        if was_zero then
            e:set_routine(2, 4)
            e:set_var(TEXTURE, 0)
        end
        if now < 0x28 then
            -- Its arm shakes, wider each tick.
            e:set_timer(1, e:timer(1) + 5)
            flip_timer2(e)
        end
    elseif sub == 4 then
        e:set_parts0_y(0x1000)
        flip_timer2(e)
        e:spd()
        if e:joint(0x80) then
            e:set_routine(2, 5)
            set_clip(e, 0x13)
            e:set_timer(0, 0x5A)
            e:set_timer(3, 10)
            e:set_var(WAVE, 0)
            e:set_neck_flags(e:neck_flags() & 0xFD)
        end
        e:add_speed_xz()
    elseif sub == 5 then
        if timer0_down(e) then
            e:set_routine(2, 6)
            e:set_var(FLAGS, e:var(FLAGS) | 0x40)
            e:set_var(TEXTURE, 0)
        end
        if i16(e:timer(1)) > 0 then
            e:set_timer(1, e:timer(1) - 10)
            flip_timer2(e)
        end
        if i16(e:timer(0)) < 0x46 then
            e:set_timer(3, e:timer(3) + 1)
        end
    elseif sub == 6 then
        e:spd()
        e:set_hokan(0x1C)
        e:joint(0x80)
        if e:move_cnt() == 0x32 then
            e:set_routine(2, 7)
            set_clip(e, 0x13)
            e:set_var(TEXTURE, 0)
        end
        e:add_speed_xz()
    elseif sub == 7 then
        if play(e) then
            e:set_kind(e:kind() & ~TYPE_SCENE)
            e:set_var(FLAGS, e:var(FLAGS) & ~0x40)
            e:routines(1, 1, 0, 0)
        end
    end
end

-- `Goto00_ck` toward the player: 0 once facing them within `rate`.
local function player_ck(e, rate)
    local x, _, z = e:player_pos()
    return e:goto_ck(x, z, rate)
end

-- Routine 1 before the clip (`0x48ae20`): the fight's choice of what to do
-- next, by the player's distance (`L_pl`) and whether it faces them. Close
-- and facing, it swipes (5) or, unless it has just attacked, may step in (3);
-- from 0x708 to 3000 it lunges (2), or with `FLAGS`' 1 or 0x20 slashes (7);
-- and while the player is hurt it goes for them (4).
local function fight_ck(e)
    local far = e:distance()
    player_ck(e, 0x5F4)
    local near = player_ck(e, 0x270)
    local ahead = player_ck(e, 0x338)
    if ahead == 0 and far < 0x5DC then
        e:routines(1, 5, 0, 0)
        if e:var(CNT_21A) < 10 and e:rnd() & 1 ~= 0 then
            e:routines(1, 3, 0, 0)
        end
    end
    if e:var(CNT_21A) == 0 and e:pl_damage_cnt() == 0 then
        if ahead == 0 and far < 3000 and far > 0x708 then
            e:routines(1, 2, 0, 0)
            if e:var(FLAGS) & 1 ~= 0 and e:rnd() & 3 ~= 0 then
                e:routines(1, 7, 0, 0)
            end
            if e:var(FLAGS) & 0x20 ~= 0 then
                e:routines(1, 7, 0, 0)
            end
        end
        if near == 0 and far < 0x9C4 and far > 0x708 and e:rnd() & 3 == 0 then
            e:routines(1, 7, 0, 0)
        end
        if ahead == 0 and far < 0x708 then
            e:routines(1, 3, 0, 0)
        end
    end
    if e:pl_damage_cnt() ~= 0 and far < 3000 then
        e:routines(1, 4, 0, 0)
    end
end

-- Routine 1 after the clip (`0x48af70`): the walk to `Dest`, which
-- `Root_ck` sets, turning faster while it doesn't face it. Its stride is
-- halved while the clip blends in, and longer while it's out of the cut's
-- view. A step's end sometimes groans (`Snd_se_enem` 7).
local function walk(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(e:var(FLAGS) & 1 ~= 0 and 0xC or 1, 0, 0x1F)
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    local x, z = e:dest()
    e:goto_point(x, z, 0x10)
    if e:goto_ck(x, z, 0x338) ~= 0 then
        e:goto_point(x, z, 0x20)
    end
    e:spd()
    if e:hokan() > 0xF then
        -- `SAR`: toward minus infinity.
        e:set_speed_x(e:speed_x() // 2)
    end
    if not e:in_cut_view() then
        e:set_speed_x(e:speed_x() + 0x28)
    end
    if e:joint(0x80) and e:rnd() & 3 == 0 then
        e:sound(7)
    end
    e:add_speed_xz()
end

-- `funcs_48AB5B` (before the clip) and `funcs_48AB72` (after it), by
-- `Routine_1`.
local BEFORE <const> = { [1] = fight_ck }
local AFTER <const> = { [1] = walk, [10] = scene }

-- `FUN_0048ab30`.
local function move(e)
    if e:kind() & 0x3F > 4 then
        return
    end
    local routine = e:routine(1)
    local before = BEFORE[routine]
    if before then
        before(e)
    end
    local after = AFTER[routine]
    if after then
        after(e)
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
                flags = flags & 0xFFFD
            end
        end
    end
    if flags & 4 ~= 0 then
        flags = flags | 1
    end
    e:set_var(FLAGS, flags)
    count_down(e, CNT_21A)
    count_down(e, CNT_22C)
    e:set_var(WAVE, (e:var(WAVE) + 1) & 0x3F)
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        move(e)
    end
    if e:var(FLAGS) & 0x40 == 0 then
        e:seq_sound()
    end
end
