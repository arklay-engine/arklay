-- G1, William Birkin's first form, `Em_30` (entity id 0x30).
--
-- `Em_30` runs `Routine_0` from `funcs_48A474` (`0x52a248`): 0 is the
-- init (`FUN_0048a540`), 1 the move (`FUN_0048ab30`), 2 the hit
-- (`FUN_0048dc70`) and 3 the death (`FUN_0048f050`). The move runs by
-- `Type & 0x3f`: 0 to 4 run `funcs_48AB5B` before the clip and
-- `funcs_48AB72` after it, by `Routine_1`, and 5 runs `FUN_0048ff00`. Of
-- those, routine 10 is the first encounter's scene in room 303 (`Type` 0),
-- where it waits until the room's event raises `Sce_free0`, then changes and
-- goes on to routine 1, the fight's walk, which chooses its attacks: the
-- lunge and slash (2 and 7), with the pull from a wall (6) and the rearing up
-- (8), the step in (3), the charge (4) and the back off (5). Routine 9 is
-- where it limps away after its death, and falls.
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
-- `Root_ck`'s target while `FLAGS`' 8: the point of `ESCAPE`. `Em_30` reads
-- it from its table; the engine reads these two fields (`src/zombie2.rs`).
local ROOT_X <const> = 7
local ROOT_Z <const> = 8
local ESCAPE <const> = 9 -- `+0x21b`: routine 9's nearest point.
local HURT <const> = 10 -- `+0x220`: 1 hit, 2 dead; `+0x221` goes to 0 with it.
local HURT_221 <const> = 11
local SPURT <const> = 12 -- `+0x227`: a magnum's spurt of blood, 1 to start and 2 while it runs.
local SPURT_N <const> = 13 -- `+0x228`: the spurt's drops so far.
local SPURT_DIR <const> = 14 -- `+0x22a`: the way it spurts.
-- The eye's swell (`Em30_core_me_sa`): part 4's scale (its `pSa_dat_head`'s
-- word at 4), `+0x222` and `+0x21f`.
local EYE <const> = 15
local EYE_STEP <const> = 16
local EYE_COUNT <const> = 17

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
    for index = CNT_218, EYE_COUNT do
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
local function arm_reaches_within(e, point, within)
    local x, _, z = arm_point(e, point)
    if x == nil then
        return false
    end
    local px, _, pz = e:player_pos()
    return e:hypot(x - px, z - pz) < within
end

local function arm_reaches(e, point)
    return arm_reaches_within(e, point, 900)
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

-- Routine 3 (`0x48b6d0`): the step in, a short swing (clip 2) turning to the
-- player. On its hit frames two points along part 6, as the last draw left
-- it, hit an unhurt player within 800: 15 life (20 in hard mode or
-- uncensored) and the fall forwards, turned round first (2,5,1 on clip 5).
local STEP_NEAR <const> = { 0x1F4, 0x183, 0 }
local STEP_FAR <const> = { 0x3E8, 0x183, 0 }

local function drawn_arm_reaches(e, point, within)
    local x, _, z = e:drawn_part_point(ARM, point[1], point[2], point[3])
    if x == nil then
        return false
    end
    local px, _, pz = e:player_pos()
    return e:hypot(x - px, z - pz) < within
end

local function step_in(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(2, e:var(FLAGS) & 1 ~= 0 and 10 or 0, 0xF)
        e:spd()
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    e:goto_player(8)
    e:spd()
    if e:joint(0x100) then
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 0xF)
    end
    e:add_speed_xz()
    if e:now_seq() & 0x30000 == 0 then
        return
    end
    e:goto_player(0x10)
    if not (drawn_arm_reaches(e, STEP_NEAR, 800) or drawn_arm_reaches(e, STEP_FAR, 800)) then
        return
    end
    if e:pl_damage_cnt() ~= 0 then
        return
    end
    e:pl_life_down(0xF, true)
    e:pl_set_motion(5, 5, 3, 0x96, 0, 0, 0x12)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:pl_set_routine(2, 5, 1, 0)
    e:pl_set_sub1()
    e:set_var(FLAGS, e:var(FLAGS) & 0xFFFE)
    e:sound(6)
end

-- Routine 4 (the original's `0x48b8c0`): the charge. It closes in, walking
-- backwards on its clip (1, or 12 with `FLAGS`' 1) at half speed past 0x960;
-- from 0x7d1 to 0xce4 away and facing the player, its swing (clip 6) comes
-- when `Timer0` runs out, or when `Timer1` does once it has faced them in
-- that range, else it walks again (or rears up). On the swing's hit frames
-- its arm strikes a wall (radius 200, at the arm's origin, on the frames with
-- `0x10000`), which sends it to routine 6, or two points along the arm kill
-- a player within 600 (`Pl_life_down` 500, the player to 6,0,0,0 on G1's
-- clips). After a kill it waits, swings again over the body, and walks off.
local CHARGE_NEAR <const> = { 0x12C, 0x183, 0 }
local CHARGE_FAR <const> = { 0x44C, 0x183, 0 }

local function charge_swing(e)
    e:spd()
    if e:joint(0x100) then
        e:set_routine(2, 4)
        if e:timer(3) == 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(CNT_21A, 10)
            if e:rnd() & 1 == 0 then
                e:routines(1, 8, 0, 0)
            end
        end
    end
    e:add_speed_xz()
    local seq = e:now_seq()
    if seq & 0x30000 == 0 then
        return
    end
    if seq & 0x10000 ~= 0 then
        local x, y, z = e:part_point(ARM, 0, 0, 0)
        if x ~= nil and e:sca_struck(x, y, z, 200, 0x8000) then
            e:routines(1, 6, 0, 0)
            e:set_var(CNT_21A, 0x1E)
            -- Four sparks at the arm, not ported; the three `Rnd`s between
            -- them are drawn.
            e:rnd()
            e:rnd()
            e:rnd()
            return
        end
    end
    if not (arm_reaches_within(e, CHARGE_NEAR, 600) or arm_reaches_within(e, CHARGE_FAR, 600)) then
        return
    end
    if e:player_life() < 0 then
        return
    end
    -- `fg_system`'s `0x100` goes with the player's `MODE_HELD_DIE`.
    e:pl_life_down(500, false)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:pl_set_routine(6, 0, 0, 0)
    e:pl_hold()
    e:pl_set_sub1()
    e:set_var(FLAGS, e:var(FLAGS) & 0xFFFE)
    e:set_timer(3, 1)
    e:sound(1)
end

local function charge(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(e:var(FLAGS) & 1 ~= 0 and 0xC or 1, 0, 0x1F)
        e:set_routine(2, 1)
        e:set_timer(0, 0x3C)
        e:set_timer(1, 0x5A)
        e:set_timer(3, 0)
        sub = 1
    end
    if sub == 1 then
        e:goto_player(0x20)
        e:spd_reversed()
        if e:hokan() > 0x14 then
            e:set_speed_x(e:speed_x() // 2)
        end
        e:joint(0x80, true)
        local far = e:distance()
        if far > 0x960 then
            e:set_speed_x(e:speed_x() // 2)
        end
        if e:timer(0) ~= 0 then
            e:set_timer(0, e:timer(0) - 1)
        end
        local skip = false
        if e:timer(0) == 0 then
            if far < 0x7D1 then
                skip = true
            elseif far < 0xCE4 and player_ck(e, 0x100) == 0 then
                e:set_routine(2, 2)
            end
        end
        if not skip and far > 2000 and far < 0xCE4 and player_ck(e, 0x100) == 0 then
            e:set_timer(3, 1)
        end
        local was = e:timer(1)
        e:set_timer(1, was - 1)
        if was == 0 then
            if e:timer(3) == 0 then
                e:routines(1, 1, 0, 0)
                if e:rnd() & 1 == 0 then
                    e:routines(1, 8, 0, 0)
                end
            else
                e:set_routine(2, 2)
            end
            e:set_var(CNT_21A, 10)
        end
        e:add_speed_xz()
    elseif sub == 2 or sub == 3 then
        if sub == 2 then
            e:clip(6, e:var(FLAGS) & 1 ~= 0 and 10 or 0, 0xF)
            e:spd()
            e:set_routine(2, 3)
            e:set_timer(3, 0)
        end
        charge_swing(e)
    elseif sub == 4 or sub == 5 then
        if sub == 4 then
            e:clip(1, 0, 0x1F)
            e:set_routine(2, 5)
            e:set_timer(0, 0x3C)
        end
        e:goto_player(0x20)
        e:spd_reversed()
        if e:hokan() > 0x14 then
            e:set_speed_x(e:speed_x() // 2)
        end
        e:joint(0x80, true)
        local was = e:timer(0)
        e:set_timer(0, was - 1)
        if was == 0 then
            e:set_routine(2, 6)
        end
        e:add_speed_xz()
    elseif sub == 6 or sub == 7 then
        if sub == 6 then
            e:clip(6, 0, 0xF)
            e:set_routine(2, 7)
            e:set_timer(3, 0)
        end
        e:spd()
        if e:joint(0x100) then
            e:set_routine(2, 8)
        end
        if e:now_seq() & 0x20000 ~= 0 and e:timer(3) == 0 then
            -- Its blood at the player's parts 0 and 8 isn't ported.
            e:sound(1)
            e:set_timer(3, 1)
        end
        e:add_speed_xz()
    elseif sub == 8 or sub == 9 then
        if sub == 8 then
            e:set_routine(2, 9)
            e:clip(0x13, 0, 0x1F)
        end
        e:spd()
        e:set_hokan(0x1C)
        if e:joint(0x80) then
            e:set_routine(2, e:routine(2) + 1)
        end
        e:add_speed_xz()
    elseif sub == 10 or sub == 11 then
        if sub == 10 then
            e:clip(1, 0, 0x1F)
            e:set_routine(2, 0xB)
        end
        local x, z = e:dest()
        e:goto_point(x, z, -0x20)
        e:spd()
        e:joint(0x80)
        e:add_speed_xz()
    end
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

-- `FUN_0048ab30`, which the hit runs when it doesn't flinch.
local move

-- Routine 5 before the clip (`0x48bf30`): it gives up backing off once the
-- player is more than 2500 away and it faces them.
local function back_off_ck(e)
    local far = e:distance()
    player_ck(e, 0x5F4)
    player_ck(e, 0x338)
    if player_ck(e, 0x400) == 0 and far > 0x9C4 then
        e:routines(1, 1, 0, 0)
    end
end

-- Routine 5 after the clip (`0x48bfa0`): for 60 ticks it backs off on its
-- walk played backwards, turned to the player, at half speed while the clip
-- blends in; then it walks again.
local function back_off(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(e:var(FLAGS) & 1 ~= 0 and 0xC or 1, 0, 0x1F)
        e:set_routine(2, 1)
        e:set_timer(0, 0x3C)
    elseif sub ~= 1 then
        return
    end
    e:goto_player(0x20)
    e:spd_reversed()
    if e:hokan() > 0x14 then
        e:set_speed_x(e:speed_x() // 2)
    end
    e:joint(0x80, true)
    e:add_speed_xz()
    if timer0_down(e) then
        e:routines(1, 1, 0, 0)
    end
end

-- Room 303's (`scd_var_room` 3) points for routine 9, where `Root_ck` sends
-- it, and the areas (`Hani_ck`: `x`, `z`, width, depth) where it turns to
-- each one's heading and falls once it meets a wall. The other rooms' tables
-- (two points) aren't here.
local ESCAPE_POINTS <const> = {
    [0] = { -25000, -25400 },
    { -12200, -25300 },
    { -17800, -19700 },
    { -23600, -14500 },
    { -12300, -14500 },
}
local ESCAPE_AREAS <const> = {
    [0] = { -25500, -26500, 4100, 2700, 0xC00 },
    { -14100, -26600, 3000, 2500, 0xC00 },
    { -19200, -21400, 2700, 3800, 0 },
    { -25300, -15600, 3900, 2400, 0x400 },
    { -14500, -15600, 3400, 2700, 0x400 },
}

local function in_area(x, z, area)
    -- Unsigned, so a point below the corner is out too.
    return (x - area[1]) & 0xFFFFFFFF <= area[3] and (z - area[2]) & 0xFFFFFFFF <= area[4]
end

-- Routine 9 (`0x48c1c0`): after its death it limps (clip 14) toward the
-- nearest point, and in one of the areas it turns to the area's heading and
-- walks on; when it faces that way and a wall stops it (`Sca_info`'s 1), it
-- falls (clip 16) and its life goes to -1. Its life stays 0 until then.
local function escape(e)
    local sub = e:routine(2)
    local room3 = e:room_code() & 0xFF == 3
    if sub == 0 then
        e:clip(0xE, 0, 0x1F)
        e:set_routine(2, 1)
        if room3 then
            local x, _, z = e:pos()
            local best = 0x7FFFFFFF
            for index = 4, 0, -1 do
                local point = ESCAPE_POINTS[index]
                local far = e:hypot(x - point[1], z - point[2])
                if far < best then
                    e:set_var(ESCAPE, index)
                    best = far
                end
            end
            local point = ESCAPE_POINTS[e:var(ESCAPE) & 0xF]
            e:set_var(ROOT_X, point[1])
            e:set_var(ROOT_Z, point[2])
        end
        e:set_var(FLAGS, e:var(FLAGS) | 8)
        sub = 1
    end
    if sub == 1 then
        if room3 then
            local x, _, z = e:pos()
            for index = 4, 0, -1 do
                local area = ESCAPE_AREAS[index]
                if in_area(x, z, area) then
                    e:set_routine(2, 2)
                    e:set_timer(1, area[5])
                end
            end
        end
        local x, z = e:dest()
        e:goto_point(x, z, 0x10)
        play(e)
        e:set_life(0)
    elseif sub == 2 then
        local heading = e:timer(1)
        e:goto_angle(heading, 0x20)
        -- `Goto01_ck`: whether it faces the heading within 0x10.
        local off = ((heading - e:angle()) + 0x10) & 0xFFF
        if off < 0x20 and e:sca_info() & 1 ~= 0 then
            e:set_routine(2, 3)
            e:set_be_flg(e:be_flg() | 8)
        end
        play(e)
        e:set_life(0)
    elseif sub == 3 or sub == 4 then
        if sub == 3 then
            e:clip(0x10, 0, 0x1F)
            e:set_routine(2, 4)
            e:set_life(-1)
        end
        e:spd()
        if e:joint(0x80) then
            e:set_routine(2, 5)
        end
        e:add_speed_xz()
    end
end

-- The blood of a hit (`FUN_0048dc70`'s and `FUN_0048f050`'s start): two
-- drops beside part 1, turned toward the player, higher for a shot at the
-- head, and `Snd_se_enem` 8. The other weapons' blood (9 to 0x11) isn't
-- here: arklay's player doesn't fire them yet. A magnum's starts its spurt.
local function hit_blood(e, weapon)
    local dir = e:direction_to_player()
    local vy = i16(0x100 - e:rnd() * 2)
    local vz = 0x80 - e:rnd()
    if e:damage_no() % 3 == 0 then
        vy = i16(0x3BC - e:rnd() * 2)
    end
    local x, y, z = e:rot_apply(0, dir, 0, 100, vy, vz)
    local px, py, pz = e:drawn_part_point(1, 0, 0, 0)
    if px == nil then
        px, py, pz = e:pos()
    end
    x, y, z = i16(x + px), i16(y + py), i16(z + pz)
    if weapon ~= 0x10 then
        e:call_effect(0, 1, e:rnd() * 8 + 0x17D0, i16(dir), x, y, z)
        y = i16(y + 0x40 - (e:rnd() & 0x3F))
        e:call_effect(0, 0, e:rnd() * 8 + 0x17D0, i16(dir), x, y, z)
        if weapon ~= 1 then
            e:sound(8)
        end
    end
    if weapon == 5 or weapon == 6 then
        e:set_var(SPURT, 1)
    end
end

-- `FUN_0048e9f0`: a magnum's spurt of blood out of the far side, eight drops
-- a tick in a line away from the player, for three ticks.
local function spurt(e)
    local state = e:var(SPURT)
    if state == 1 then
        e:set_var(SPURT, 2)
        e:set_var(SPURT_N, 0)
        e:set_var(SPURT_DIR, e:direction_to_player())
    elseif state ~= 2 then
        return
    end
    local dir = e:var(SPURT_DIR)
    local count = e:var(SPURT_N)
    local ox, oy, oz = e:rot_apply(0, dir, 0, i16(count * -100), 200, 0)
    local sx, sz
    if e:damage_no() % 3 == 0 then
        sx, _, sz = e:rot_apply(0, dir, 0, -100, 500, 0)
    else
        sx, _, sz = e:rot_apply(0, dir, 0, -100, -500, 0)
    end
    local bx, _, bz = e:pos()
    local _, part_y = e:drawn_part_point(2, 0, 0, 0)
    local y = i16((part_y or 0) + oy)
    local function drop(x, z, step)
        local size = (0x65C - step * count) - e:rnd()
        e:call_effect(0, 1, size & 0xFFFF, i16(dir), i16(x), y, i16(z))
        count = count + 1
    end
    local function wide_drop(x_at, z_at, step)
        local x = bx - e:rnd() + x_at + 0x80 + ox
        local z = bz - e:rnd() + z_at + 0x80 + oz
        local size = (0x65C - step * count) - e:rnd()
        local row = e:rnd() & 1 == 0 and 1 or 0
        e:call_effect(0, row, size & 0xFFFF, i16(dir), i16(x), y, i16(z))
        if count == 1 then
            e:call_effect(0, 2, 0x17D0, i16(dir + 0x800), i16(x), y, i16(z))
            e:call_effect(0, 2, 0xC18, i16(dir + 0x800), i16(x), y, i16(z))
        end
        count = count + 1
    end
    drop(bx + ox, bz + oz, 0x24)
    wide_drop(sx, sz, 0x24)
    drop(sx * 2 + bx + ox, sz * 2 + bz + oz, 0x24)
    wide_drop(sx * 3, sz * 3, 0x24)
    drop(sx * 4 + bx + ox, sz * 4 + bz + oz, 0x24)
    wide_drop(sx * 5, sz * 5, 0x24)
    drop(sx * 6 + bx + ox, sz * 6 + bz + oz, 0x24)
    local last = count
    wide_drop(sx * 7, sz * 7, 0x1E)
    e:set_var(SPURT_N, count)
    if last > 0x10 then
        e:set_var(SPURT, 0)
    end
end

-- `Sa_parts_allmv` on part 4, the eye: its child, part 5, which carries the
-- arm, sits under it at a place blended from its own toward the swollen
-- eye's by the eye's weight (`em030.emd`'s `pSa_dat`, tables 0 and 1; the
-- same in `em130.emd`). The eye's own shape isn't drawn swollen.
local EYE_CHILD <const> = 5
local EYE_CHILD_AT <const> = { 0, 558, 0 }
local EYE_CHILD_SWOLLEN <const> = { 0, 426, 0 }

local function eye_morph(e, weight)
    if weight == 0 then
        e:set_joint_offset(EYE_CHILD)
        return
    end
    local at = {}
    for axis = 1, 3 do
        -- `LoadAverage12`, with 0xfff for the rest.
        at[axis] = (EYE_CHILD_AT[axis] * (0xFFF - weight) + EYE_CHILD_SWOLLEN[axis] * weight) // 0x1000
    end
    e:set_joint_offset(EYE_CHILD, at[1], at[2], at[3])
end

-- `Em30_core_me_sa`: the eye swells after a hit (`HURT` 1) until its scale
-- passes 0x1000, then shrinks back and stops; once G1 is dead (`HURT` 2) it
-- swells to 0xc00 and throbs, 500 a tick for four ticks each way. Each tick
-- but those of a hit's swell and shrink draws two `Rnd`s, for the eye's
-- jitter (`Em30_me_tate_d`, `Em30_me_yoko_a`).
local function eye(e)
    local state = e:var(HURT)
    if state == 0 then
        return
    end
    local sub = e:var(HURT_221)
    local scale = e:var(EYE)
    local step = e:var(EYE_STEP)
    local count = e:var(EYE_COUNT)
    local jitter = true
    if state == 1 then
        jitter = false
        if sub == 0 then
            scale = i16(scale + 0x400)
            if scale > 0x1000 then
                scale = 0x1000
                sub = 1
                step = 0
                jitter = true
            end
        elseif sub == 1 then
            scale = i16(scale - step)
            step = i16(step + 100)
            if scale < 0 then
                scale = 0
                e:set_var(HURT, 0)
                sub = 0
            end
        end
    elseif sub == 0 then
        scale = i16(scale + 0x400)
        if scale >= 0xC01 then
            scale = 0xC00
            step = 0
            sub = 1
            count = 3
        end
    elseif sub == 1 or sub == 2 then
        local way = sub == 1 and -1 or 1
        scale = i16(scale + way * 500)
        step = i16(step - way * 100)
        local was = count
        count = (count - 1) & 0xFF
        if was == 0 then
            sub = 3 - sub
            count = 3
        end
    end
    if jitter then
        e:rnd()
        e:rnd()
    end
    eye_morph(e, scale)
    e:set_var(HURT_221, sub)
    e:set_var(EYE, scale)
    e:set_var(EYE_STEP, step)
    e:set_var(EYE_COUNT, count)
end

-- `FUN_0048dfb0`: the flinch, backwards or forwards by whether it faces the
-- way the player does (clip 10 or 9); then it rears up (routine 8).
local function flinch(e)
    local sub = e:routine(2)
    if sub == 0 then
        local same = e:cdir_player()
        e:set_var(FLAGS, e:var(FLAGS) | 0x80)
        e:clip(same and 9 or 10, 0, 3)
        e:set_routine(2, 1)
        if e:routine(1) == 0xB then
            e:sound(0)
        end
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:joint(0x400) then
        e:set_var(FLAGS, e:var(FLAGS) & ~0x80)
        e:routines(1, 8, 0, 0)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:sound(7)
    end
    e:add_speed_xz()
end

-- `0x52a3b3`: what each weapon adds to `CNT_219`, which flinches it at 12.
local FLINCH_ADD <const> = {
    [0] = 0, 1, 4, 4, 4, 10, 20, 6, 7, 10, 10, 20, 6, 4, 20, 1, 0, 20, 2,
}

-- Routine 0 2 (`FUN_0048dc70`): a hit. Below 200 life it raises `FLAGS`' 4
-- (and from the next tick its fast clips) and starts `CNT_219` at 0x32. Each
-- hit adds the weapon's share to `CNT_219`, which `CNT_218` takes down by one
-- every 15 ticks; at 12 or more it flinches, and otherwise it goes on with
-- what it was doing on this tick. A hit during the flinch goes on with it.
local function hurt(e)
    if e:routine(2) ~= 0 then
        flinch(e)
        return
    end
    local weapon = e:routine(1)
    hit_blood(e, weapon)
    e:set_var(HURT, 1)
    e:set_var(HURT_221, 0)
    local flags = e:var(FLAGS)
    -- Hard mode's 350 isn't modelled.
    if flags & 4 == 0 and e:life() < 200 then
        e:set_var(CNT_219, 0x32)
        flags = flags | 4
        e:set_var(FLAGS, flags)
    end
    if flags & 0x80 ~= 0 then
        e:restore_routine()
        flinch(e)
        return
    end
    if flags & 1 == 0 then
        local add = FLINCH_ADD[weapon] or 0
        local count = e:var(CNT_219)
        if weapon == 9 then
            count = count + (add >> 1)
            if e:damage_no() < 3 then
                count = count + (add >> 1)
            end
        else
            count = count + add
        end
        e:set_var(CNT_219, count & 0xFF)
        if flags & 2 == 0 then
            e:set_var(CNT_218, 0xF)
            e:set_var(FLAGS, flags | 2)
        end
    end
    if e:var(CNT_219) < 0xC then
        e:restore_routine()
        move(e)
        return
    end
    e:set_var(FLAGS, e:var(FLAGS) & 0xFFFD)
    e:set_var(CNT_219, 0)
    e:set_var(CNT_218, 0)
    flinch(e)
end

-- The two points along the arm that hit the player as it rises from its
-- death, within 900.
local RISE_NEAR <const> = { 500, 0x183, 0 }
local RISE_FAR <const> = { 0x44C, 0x183, 0 }

-- The rise's blow: 30 life (45 in hard mode) and a fall on G1's clips,
-- forwards from behind (2,5) or backwards from in front (2,4), turned to it
-- first unless it swung again.
local function rise_hit(e)
    local to = e:direction_to_player()
    local dir = (to - 0x400) & 0xFFFF
    local front = ((e:player_angle() - dir + 0x400) & 0xFFF) < 0x800
    if e:timer(3) ~= 0 then
        front = not front
        dir = (to - 0xC00) & 0xFFFF
    end
    e:pl_life_down(0x1E, true)
    e:pl_set_routine(2, front and 5 or 4, 0, 0)
    if not front then
        e:set_player_angle((dir + 0x800) & 0xFFFF)
    end
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:pl_set_sub1()
    e:sound(6)
end

-- `FUN_0048f2b0`: the death. It falls back or forwards (clip 10 or 9),
-- raises its dead flag and `FLAGS`' 0x11, lies (clip 17), gets up toward the
-- player (clip 4 from frame 0x21), may swing again (clip 8) and on the hit
-- frames hits a player within reach; then it limps away (routine 9). Its
-- life stays 0 throughout.
local function death(e)
    e:set_life(0)
    local sub = e:routine(2)
    if sub == 0 then
        local same = e:cdir_player()
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:set_var(FLAGS, e:var(FLAGS) | 0x11)
        e:clip(same and 9 or 10, 0, 3)
        e:set_routine(2, 1)
        e:set_var(HURT, 2)
        e:set_var(HURT_221, 0)
        e:set_died()
        local weapon = e:routine(1)
        if weapon == 5 or weapon == 6 then
            e:set_var(SPURT, 1)
        end
        sub = 1
    end
    if sub == 1 then
        e:spd()
        if e:joint(0x400) then
            e:clip(0x11, 0, 7)
            e:set_routine(2, 2)
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
        end
        e:add_speed_xz()
    elseif sub == 2 then
        e:goto_player(0x10)
        e:spd()
        e:add_speed_xz()
        if e:joint(0x200) then
            e:clip(4, 0x21, 3)
            e:set_routine(2, 3)
            e:spd()
            e:set_timer(3, 0)
            e:sound(3)
        end
    elseif sub == 3 then
        e:spd()
        if e:joint(0x400) then
            e:routines(1, 9, 0, 0)
        end
        e:add_speed_xz()
        if e:move_cnt() == 0x2E and e:timer(3) == 0 then
            e:clip(8, 0, 3)
            e:set_timer(3, 1)
        end
        if e:now_seq() & 0x30000 ~= 0 then
            e:goto_player(0x10)
            if e:timer(3) ~= 0 and player_ck(e, 0x400) ~= 0 then
                e:set_angle((e:angle() + 0x80) & 0xFFFF)
            end
            if (arm_reaches(e, RISE_NEAR) or arm_reaches(e, RISE_FAR)) and e:pl_damage_cnt() == 0 then
                rise_hit(e)
            end
        end
    end
end

-- Routine 0 3 (`FUN_0048f050`): a shot that takes its life below zero. Shot
-- again once dead (`FLAGS`' 0x10), it bleeds and goes on where it was, in
-- its death or limping away.
local function dying(e)
    if e:var(FLAGS) & 0x10 ~= 0 and e:routine(2) == 0 then
        local weapon = e:routine(1)
        hit_blood(e, weapon)
        e:restore_routine()
        if e:routine(0) == 1 and e:routine(1) == 9 then
            escape(e)
            return
        end
    end
    death(e)
end

-- `funcs_48AB5B` (before the clip) and `funcs_48AB72` (after it), by
-- `Routine_1`.
local BEFORE <const> = { [1] = fight_ck, [5] = back_off_ck }
local AFTER <const> = {
    [1] = walk,
    [2] = swing,
    [3] = step_in,
    [4] = charge,
    [5] = back_off,
    [6] = pull_free,
    [7] = swing,
    [8] = rear,
    [9] = escape,
    [10] = scene,
}

-- `FUN_0048ab30`.
function move(e)
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
    elseif zero == 2 then
        hurt(e)
    elseif zero == 3 then
        dying(e)
    end
    spurt(e)
    eye(e)
    if e:var(FLAGS) & 0x40 == 0 then
        e:seq_sound()
    end
end
