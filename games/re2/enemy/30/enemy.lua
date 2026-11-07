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
-- fight's walk, which chooses its attacks: the lunge and slash (2 and 7),
-- with the pull from a wall (6) and the rearing up (8), are ported; the step
-- in (3), the charge (4), the swipe (5) and 9 aren't yet.
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

-- The lunge's and the slash's clips (`0x48b080`'s tables), by `FLAGS`' 1:
-- `Move_no`, `Move_cnt`, `Hokan_flg`, and `Joint_move`'s rate.
local LUNGE <const> = { [0] = { 4, 0, 0xF, 0x100 }, { 4, 0x21, 3, 0x400 } }
local SLASH <const> = { [0] = { 0xB, 0, 0xF, 0x100 }, { 0xB, 0x11, 3, 0x400 } }
-- The frame before which each may break off: the lunge's and the slash's.
local BREAK_BEFORE <const> = { [0] = 0x1E, 0x14 }
-- The frame each swings again from (clip 8).
local AGAIN_AT <const> = { [0] = 0x2E, 0x1E }
-- The two points along part 6, the arm, that hit the player within 900, and
-- the one that strikes a wall (`Sca_ck_hit`, 400, mask `0x8000`, 0x96 lower).
local HIT_NEAR <const> = { 0x1F4, 0x183, 0 }
local HIT_FAR <const> = { 0x44C, 0x183, 0 }
local WALL <const> = { 0x320, 0x183, 0 }
local ARM <const> = 6

-- A point along part 6 as `FUN_00490060` composes it, on this tick's pose,
-- facing and place.
local function arm_point(e, point)
    return e:part_point(ARM, point[1], point[2], point[3])
end

-- `L_pos_ckXZM` on part 6.
local function arm_reaches(e, point)
    local x, _, z = arm_point(e, point)
    if x == nil then
        return false
    end
    local px, _, pz = e:player_pos()
    return e:hypot(x - px, z - pz) < 900
end

-- The hit: 30 life (45 in hard mode) and a fall on G1's own clips, backwards
-- from in front (2,4), forwards from behind (2,5), or turned round first by a
-- slash from behind that hasn't swung again (2,5,1 on clip 5).
local function hit_player(e)
    local dir = (e:direction_to_player() - 0x400) & 0xFFFF
    local front = ((e:player_angle() - dir + 0x400) & 0xFFF) < 0x800
    local again = e:timer(3) ~= 0
    if again then
        front = not front
        dir = (dir - 0x800) & 0xFFFF
    end
    e:pl_life_down(0x1E, true)
    local one = front and 5 or 4
    e:pl_set_routine(2, one, 0, 0)
    local turn = not front
    if e:timer(2) ~= 0 and not again then
        turn = false
        if not front then
            e:pl_set_routine(2, 5, 1, 0)
            e:pl_set_motion(5, 5, 3, 0x96, 0, 0)
        end
    end
    if turn then
        e:set_player_angle(dir + 0x800)
    end
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:pl_set_sub1()
    e:set_var(FLAGS, e:var(FLAGS) & 0xFFFE)
    e:set_timer(3, 1)
    e:sound(6)
    e:set_timer(1, 0xF)
end

-- Routines 2 and 7 (`0x48b080`): the lunge and the slash. It may break off
-- early if the player gets away or out of its arc, swings again once, and on
-- the clips' hit frames (`0x30000`) turns to the player and hits them with
-- its arm, or strikes a wall with it (routine 6).
local function swing(e)
    local sub = e:routine(2)
    local row = e:var(FLAGS) & 1
    if sub == 0 then
        local clip = LUNGE[row]
        e:clip(clip[1], clip[2], clip[3])
        if row ~= 0 then
            e:sound(3)
        end
        e:set_routine(2, 1)
        e:set_timer(2, 0)
        if e:routine(1) == 7 then
            e:set_timer(2, 1)
            clip = SLASH[row]
            e:clip(clip[1], clip[2], clip[3])
            e:set_var(FLAGS, e:var(FLAGS) & 0xFFDF)
        end
        e:spd()
        e:set_timer(3, 0)
        e:set_timer(1, 0)
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:hokan() ~= 0 then
        local x, y, z = e:speed()
        e:set_speed(x // 2, y, z // 2)
    end
    if e:joint(LUNGE[row][4]) then
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, (e:timer(1) + 0x1E) & 0xFF)
    end
    e:add_speed_xz()
    local slash = e:timer(2)
    local count = e:move_cnt()
    if count > 5 and count < BREAK_BEFORE[slash] and e:timer(3) == 0 then
        e:goto_player(0x20)
        if e:distance() > 0xED8 or player_ck(e, 0x52C) ~= 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(CNT_21A, 10)
            if e:rnd() & 3 == 0 then
                e:routines(1, 8, 0, 0)
            end
        end
    end
    if count == AGAIN_AT[slash] and (slash == 0 or e:timer(3) == 0) then
        if e:timer(3) == 0 or e:rnd() & 3 == 0 then
            e:clip(8, 0, LUNGE[e:var(FLAGS) & 1][3])
            e:set_timer(3, 1)
        end
    end
    if e:now_seq() & 0x30000 == 0 then
        return
    end
    e:goto_player(0x20)
    if e:timer(3) ~= 0 and player_ck(e, 0x400) ~= 0 then
        e:set_angle((e:angle() + 0x80) & 0xFFFF)
    end
    if (arm_reaches(e, HIT_NEAR) or arm_reaches(e, HIT_FAR)) and e:pl_damage_cnt() == 0 then
        hit_player(e)
    end
    local x, y, z = arm_point(e, WALL)
    if x ~= nil and e:sca_struck(x, y + 0x96, z, 400, 0x8000) then
        e:routines(1, 6, 0, 0)
        e:set_var(CNT_21A, 0x1E)
        -- Four sparks at the arm; their effects aren't ported, but the three
        -- `Rnd`s between them are drawn.
        e:rnd()
        e:rnd()
        e:rnd()
    end
end

-- Routine 6 (`0x48c060`): the arm struck a wall, and it pulls it free
-- (clip 7), turning to the player; then it walks again, or one time in two
-- goes to routine 8, and one time in two, facing an unhurt player, to 4's
-- charge (`Routine_2` 2). `FLAGS`' 0x20 makes its next lunge a slash.
local function pull_free(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_var(FLAGS, e:var(FLAGS) | 0x20)
        e:clip(7, 0, 2)
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    e:goto_player(0x10)
    e:spd()
    if e:joint(0x400) then
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 0x28)
        if e:rnd() & 1 == 0 then
            e:routines(1, 8, 0, 0)
        end
        if e:rnd() & 1 == 0 and e:pl_damage_cnt() == 0 and player_ck(e, 0x100) == 0 then
            e:routines(1, 4, 2, 0)
        end
    end
    e:add_speed_xz()
end

-- Routine 8 (`0x48c130`): it rears up (clip 17), turned toward the player,
-- and raises `FLAGS`' 1, which plays its walk and its swings from their
-- faster clips and frames; then it walks again.
local function rear(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0x11, 0, 7)
        e:set_routine(2, 1)
        e:goto_player(0x40)
        e:set_var(FLAGS, e:var(FLAGS) | 1)
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:joint(0x200) then
        e:set_var(FLAGS, e:var(FLAGS) | 1)
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 10)
    end
    e:add_speed_xz()
end

-- `funcs_48AB5B` (before the clip) and `funcs_48AB72` (after it), by
-- `Routine_1`.
local BEFORE <const> = { [1] = fight_ck }
local AFTER <const> = {
    [1] = walk,
    [2] = swing,
    [6] = pull_free,
    [7] = swing,
    [8] = rear,
    [10] = scene,
}

-- `FUN_0048ab30`.
local function move(e)
    if e:kind() & 0x3F > 4 then
        return
    end
    local before = BEFORE[e:routine(1)]
    if before then
        before(e)
    end
    -- The choice may have changed `Routine_1`, which the second table reads.
    local after = AFTER[e:routine(1)]
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
