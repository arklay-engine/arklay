-- The giant spider, `Em_25` (entity id 0x25).
--
-- `Em_25` runs `Routine_0` from `funcs_46F049`: 0 is the init
-- (`FUN_0046f0c0`) and 1 the move (`FUN_0046f370`), which runs the state
-- table of the spider's kind, `+0x222`: 0 on the floor, 1 on the ceiling and
-- 3 on a wall, from `Type`'s low nibble. The engine runs `Oba_ck_em`,
-- `Sca_ck_em` (with `0x600`) and `nFloor` after `update`, and leaves the
-- place part 12 had at the end of the last tick in `FOOT`.
--
-- Ported: the init, and on the floor the rest (`Routine_1` 0), the walks
-- (1 to 3), the turn on the spot (4), the turns toward the player (5 and 6),
-- the lunge and its bite (7), the spit (8) and the leap aside from a noise
-- (9); and on the floor a hit (`Routine_0` 2), the death (3) and the body
-- (7), for weapons 2 to 8. The ceiling and the wall, the young spiders
-- (`Em_26`) a death sends out, `FUN_004727e0`'s water and `FUN_00473530`'s
-- blood aren't ported: a spider that reaches them stands still.

local COOLDOWN <const> = 0 -- `+0x218`: the rest waits for it before an attack.
local ADD_DIR <const> = 1 -- `+0x21a`: the turn `Add_speedXZ` adds.
local KIND <const> = 2 -- `+0x222`: 0 floor, 1 ceiling, 3 wall.
local TO_PARTNER <const> = 3 -- `+0x237`: it aims at `splayer_work`.
local STEP <const> = 4 -- `+0x238`: which footstep sound is next.
local T232 <const> = 5 -- `+0x232`.
local T21C <const> = 6 -- `+0x21c`.
local T21E <const> = 7 -- `+0x21e`.
local LAST <const> = 8 -- `+0x233` to `+0x235`: `Routine_1` to `Routine_3` while `+0x21c` is 0.
local T23D <const> = 11 -- `+0x23d`.
local LOST_LEGS <const> = 12 -- `+0x220`: a bit for each leg lost.
local T221 <const> = 13 -- `+0x221`: legs left to lose, from 8.
local WATER <const> = 14 -- `Water` (`+0x10c`): the level a hit or a death found it in.
local HIT_BY <const> = 15 -- `+0x23c`: the weapon (`Routine_1`) of the last hit or death.
local REVIVED <const> = 16 -- `+0x224`: it has died once, and its body takes a last shot.
local FLOAT <const> = 17 -- `+0x23a`: below 0 once its body lies in water.
local BOB <const> = 18 -- `+0x23e`: how the body in water moves (`FUN_004728b0`).
local F23F <const> = 19 -- `+0x23f`.
local T225 <const> = 20 -- `+0x225`: `FUN_004727e0`'s state.
local T239 <const> = 21 -- `+0x239`: no body to shoot again; nothing on the floor sets it.
local FOOT <const> = 32 -- x and z of part 12 (leg 1's foot) as the last tick ended.

-- `PTR_DAT_005284ac` and, with `fg_system`'s `0x20`, `PTR_DAT_005284cc`.
local LIFE <const> = {
    0x63, 0x63, 0x63, 0x63, 0x77, 0x63, 0x63, 0x77,
    0x63, 0x63, 0x63, 0x77, 0x63, 0x59, 0x63, 0x63,
}
local LIFE_EASY <const> = {
    0x4F, 0x59, 0x63, 0x4F, 0x31, 0x59, 0x4F, 0x45,
    0x59, 0x45, 0x59, 0x59, 0x63, 0x45, 0x31, 0x4F,
}

-- `0x5285e4`'s rows for `Routine_1` 1 to 3: the clip, the turn toward the
-- target and what's added to the foot's step.
local WALKS <const> = {
    [1] = { 2, 0x30, 0x64 },
    [2] = { 3, 0x10, 0x3C },
    [3] = { 3, 0x10, 0x14 },
}

-- `BYTE_00528844`: the footsteps' sounds, in turn.
local STEPS <const> = { [0] = 0, 8, 9 }

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

-- `L_pl`, or `L_spl` & 0x3fffffff when it aims at the partner. Only the
-- player is ported.
local function distance(e)
    return e:distance()
end

-- `FUN_00472aa0`: `Goto00_ck` toward the target. Only the floor's is ported.
local function goto_target(e, rate)
    local x, _, z = e:player_pos()
    return e:goto_ck(x, z, rate)
end

-- `FUN_00473430`: a footstep on the frames that call for one.
local function footstep(e)
    if e:now_seq() & 0x100000 == 0 then
        return
    end
    -- `Water` (`+0x10c`) is only found by a hit or a death.
    e:sound(e:var(WATER) ~= 0 and 10 or STEPS[e:var(STEP)])
    e:set_var(STEP, (e:var(STEP) + 1) % 3)
end

-- `FUN_004729c0(1)`: the step is how far leg 1's foot, part 12, has moved
-- across the floor since the last tick, posed and turned as it is now.
local function foot_step(e)
    if e:var(LOST_LEGS) & (1 << 4) ~= 0 then
        return 0
    end
    local x, _, z = e:part_point(12, 0, 0, 0)
    if x == nil then
        return 0
    end
    local dx = i16(x - e:var(FOOT))
    local dz = i16(z - e:var(FOOT + 1))
    return e:hypot(dx, dz)
end

-- `FUN_0046f0c0`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_be_flg(e:be_flg() | 0xC00000)
    local life
    if e:easy() then
        life = LIFE_EASY[(e:rnd() & 0xF) + 1]
    else
        life = LIFE[(e:rnd() & 0xF) + 1]
    end
    -- `hard_mode` doubles it; it's taken as clear.
    e:set_life(life + (e:rnd() & 3))
    e:set_d_life(6)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    -- `+0x218` to `+0x247`.
    for index = 0, 21 do
        if index ~= WATER then
            e:set_var(index, 0)
        end
    end
    e:clip(1, 0, 0)
    e:joint(0x100)
    e:set_neck_flags(0)
    e:set_shadow(0x320)
    e:set_var(T221, 8)
    e:set_body_ofs_x(0)
    e:set_body_box(-0x578, 0x3E8, 0x578, 0x320)
    local kind = e:kind() & 0xF
    if kind == 0 then
        e:set_var(KIND, 0)
        e:set_var(ADD_DIR, 0)
        e:set_be_flg(e:be_flg() | 0xC000000)
        return
    end
    -- The ceiling (2 and 3) and the walls (4 to 11) aren't ported.
    if kind == 2 or kind == 3 then
        e:set_var(KIND, 1)
    elseif kind >= 4 and kind <= 11 then
        e:set_var(KIND, 3)
    end
    e:set_var(ADD_DIR, 0x800)
end

-- `FUN_0046f520`: the rest's clip and its count.
local function rest_wait(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0, 0, 3)
        e:set_routine(2, 1)
        e:set_timer(0, (e:rnd() + 10) & 0x1F)
    elseif sub ~= 1 then
        return
    end
    e:joint(0x400)
    local left = e:timer(0)
    e:set_timer(0, (left - 1) & 0xFFFF)
    if left == 0 then
        e:routines(1, 1, 0, 0)
        if e:rnd() & 1 ~= 0 then
            e:routines(1, 4, 0, 0)
        end
    end
end

-- `Routine_1` 0 on the floor (`0x46f420`).
local function rest(e)
    rest_wait(e)
    local near = distance(e) & 0xFFFF
    local facing = goto_target(e, 0x80)
    goto_target(e, 0x100)
    local wide = goto_target(e, 0x400)
    if wide ~= 0 and near < 8000 then
        e:routines(1, 5, 0, 0)
    end
    if e:noise() & 1 ~= 0 then
        e:routines(1, 9, 0, 0)
        if e:rnd() & 1 ~= 0 then
            e:routines(1, 5, 0, 0)
        end
    end
    if e:var(COOLDOWN) ~= 0 then
        return
    end
    if near < 7500 and facing == 0 and near > 3000 then
        e:routines(1, 7, 0, 0)
    end
    if near < 6000 and facing == 0 and near > 1000 then
        if (1 << (e:rnd() & 0xF)) & 0x340 ~= 0 then
            e:routines(1, 8, 0, 0)
            return
        end
        e:routines(1, 7, 0, 0)
    end
end

-- `Routine_1` 1 to 3 on the floor (`0x46f5a0`): a walk toward the target.
local function walk(e)
    local row = WALKS[e:routine(1)]
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        e:clip(row[1], 0, 3)
        local x, y = e:speed()
        e:set_speed(x, y, 0)
        e:set_timer(0, e:rnd() & 0x2E)
    elseif sub ~= 1 then
        e:add_speed_xz_at(e:var(ADD_DIR))
        return
    end
    e:set_angle((e:angle() + goto_target(e, row[2])) & 0xFFFF)
    e:joint(0x400)
    footstep(e)
    local step = foot_step(e)
    e:set_timer(1, step)
    local _, y, z = e:speed()
    e:set_speed(i16(step + row[3]), y, z)
    local left = e:timer(0)
    e:set_timer(0, (left - 1) & 0xFFFF)
    if left == 0 or (e:var(KIND) == 0 and distance(e) < 1500) then
        e:routines(1, 0, 0, 0)
    end
    e:add_speed_xz_at(e:var(ADD_DIR))
end

-- `Routine_1` 4 (`0x46f6d0`): a turn on the spot.
local function turn(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(3, 0, 3)
        e:set_routine(2, 1)
        e:set_timer(1, 0x18)
        e:set_timer(0, (e:rnd() + 10) & 0x3F)
        if e:rnd() & 1 ~= 0 then
            e:set_timer(1, 0xFFE8)
        end
        if e:var(KIND) ~= 0 then
            e:set_timer(1, (-e:timer(1)) & 0xFFFF)
        end
    elseif sub ~= 1 then
        return
    end
    e:set_angle((e:angle() + e:timer(1)) & 0xFFFF)
    e:joint(0x400)
    footstep(e)
    local left = e:timer(0)
    e:set_timer(0, (left - 1) & 0xFFFF)
    if left == 0 then
        e:routines(1, 0, 0, 0)
    end
end

-- `Joint_move`'s answer as the `Routine_2` step the states add it as.
local function joint_step(e)
    return e:joint(0x400) and 1 or 0
end

-- The target's place: `pPl`, or `splayer_work` when it aims at the partner.
-- Only the player is ported.
local function target_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

-- `0x46f790`'s table on the stack, for `Routine_1` 5 and 6: the clip and the
-- turn toward the target that counts as facing it.
local AIMS <const> = {
    [5] = { 2, 0x80 },
    [6] = { 3, 0x20 },
}

-- `Routine_1` 5 and 6 (`0x46f790`): a turn toward the target, until it faces
-- it or `Timer1` runs out. Only the rest enters 5; nothing enters 6 on the
-- floor.
local function aim(e)
    local row = AIMS[e:routine(1)]
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        e:clip(row[1], 0, 3)
        e:set_timer(1, (e:rnd() & 0x1F) + 0x3C)
    elseif sub == 2 then
        e:routines(1, 0, 0, 0)
        return
    elseif sub ~= 1 then
        return
    end
    local turn = goto_target(e, row[2])
    e:set_angle((e:angle() + turn) & 0xFFFF)
    local left = e:timer(1)
    e:set_timer(1, (left - 1) & 0xFFFF)
    if left == 0 or turn == 0 then
        e:set_routine(2, 2)
    end
    e:joint(0x400)
    footstep(e)
end

-- `FUN_00473100(e, 0, reach)`: the bite on the player within `reach`, unless
-- a hit reaction is running (`+0x1d3`'s `0x80`). `Pl_life_down(20)`
-- (`hard_mode`'s 30 is taken as off), then `FUN_004b0050` (the player's
-- `Routine` 3) when it kills, or `FUN_0045fb00` (2, and 2 or 3 by `Cdir_ck`)
-- as the dog's bite does.
local function bite_player(e, reach)
    if distance(e) >= reach or e:pl_damage_cnt() & 0x80 ~= 0 then
        return false
    end
    if e:pl_life_down(20) == 2 then
        e:pl_set_routine(3, 0, 0, 0)
        return true
    end
    local behind = e:cdir_player()
    e:pl_set_routine(2, (behind and 1 or 0) + 2, 0, 0)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    return true
end

-- `FUN_00473080(e, reach, height)`: the bite, with the player within `height`
-- up or down, and its sound. The partner's (`fg_status`'s `0x10000000`) isn't
-- ported.
local function bite(e, reach, height)
    local _, y = e:pos()
    local _, py = e:player_pos()
    local up = y - py
    if up < 0 then
        up = -up
    end
    if up < height and bite_player(e, reach) then
        e:sound(1)
        return true
    end
    return false
end

-- `Routine_1` 7 (`0x46f870`): a step forward, a turn to face the target, a
-- lunge at it that bites in reach, and the landing.
local function lunge(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(4, 0, 3)
        e:set_routine(2, 1)
        local x, y = e:speed()
        e:set_speed(x, y, 0)
        sub = 1
    end
    if sub == 1 then
        e:set_routine(2, sub + joint_step(e))
        e:set_speed_x(100)
        e:add_speed_xz_at(0)
        footstep(e)
        return
    end
    if sub == 2 then
        e:clip(5, 0, 3)
        local draw = e:rnd()
        e:set_routine(2, 3)
        e:set_timer(0, (draw & 0x1F) + 0x14)
        sub = 3
    end
    if sub == 3 then
        local x, z = target_xz(e)
        local turn = e:goto_ck(x, z, 0x40)
        e:set_angle((e:angle() + turn) & 0xFFFF)
        e:joint(0x400)
        if turn == 0 then
            e:set_routine(2, 4)
        end
        local left = e:timer(0)
        e:set_timer(0, (left - 1) & 0xFFFF)
        if left == 0 then
            e:set_routine(2, 6)
        end
        footstep(e)
        return
    end
    if sub == 4 then
        e:clip(5, 0, 3)
        e:set_routine(2, 5)
        local draw = e:rnd()
        e:set_speed_x(300)
        e:set_timer(0, (draw & 0x1F) + 10)
        sub = 5
    end
    if sub == 5 then
        local x, z = target_xz(e)
        e:goto_point(x, z, 0x10)
        e:add_speed_xz_at(e:var(ADD_DIR))
        e:joint(0x400)
        local left = e:timer(0)
        e:set_timer(0, (left - 1) & 0xFFFF)
        if left == 0 or e:at_em_no() ~= 0xFF or e:sca_info() & 1 ~= 0
            or e:goto_ck(x, z, 0x200) ~= 0 then
            e:set_routine(2, 6)
        end
        if bite(e, 0x5DC, 0x10CC) then
            e:set_routine(2, 6)
        end
        footstep(e)
        return
    end
    if sub == 6 then
        e:clip(7, 0, 3)
        e:set_speed_x(100)
        e:set_routine(2, 7)
        sub = 7
    end
    if sub == 7 then
        e:add_speed_xz_at(e:var(ADD_DIR))
        e:set_routine(2, sub + joint_step(e))
        footstep(e)
        return
    end
    if sub == 8 then
        e:routines(1, 0, 0, 0)
        e:set_var(COOLDOWN, 0xF)
    end
    footstep(e)
end

-- `FUN_00472f70(e, row, dir)` with `row` from 0 to 0x7f: `Esp_call` 0x1d
-- (the spit) from part 0's matrix, 1000 ahead and 100 aside. Its
-- `Sca_ck_hit` first only sets the hit globals, which nothing here reads.
-- The rows with the top bit, which start from the body, are the ceiling's.
local function spit_out(e, row, dir)
    e:effect(0x1D, row, 0x1000, dir, 0, 1000, 0, -100)
end

-- `Routine_1` 8 (`0x46fae0`): the spit, then back to the rest with
-- `+0x218` at 15. `Routine_2` 3 (`FUN_0046fbb0`) isn't entered on the floor
-- and isn't ported.
local function spit(e)
    local sub = e:routine(2)
    if sub == 0 then
        local dir = i16(e:var(ADD_DIR) + e:angle())
        e:clip(0xB, 0, 3)
        e:set_routine(2, 1)
        spit_out(e, e:var(KIND) + 8, dir)
        sub = 1
    end
    if sub == 1 then
        e:set_routine(2, sub + joint_step(e))
        footstep(e)
        e:seq_sound()
        return
    end
    if sub == 2 then
        e:routines(1, 0, 0, 0)
        e:set_var(COOLDOWN, 0xF)
    end
    footstep(e)
    e:seq_sound()
end

-- `Routine_1` 9 (`0x46fcc0`): a leap aside from a noise, at 400 to 463 a
-- tick, 0x380 to 0x47f off the way to the target, or that and half a turn,
-- as it turns to face the target. `Timer0` keeps the way it leaps.
local function leap(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(8, 0, 3)
        e:set_routine(2, 1)
        local x, z = target_xz(e)
        local at_x, _, at_z = e:pos()
        local dir = e:direction(at_x, at_z, x, z)
        local side = (e:rnd() & 1) << 11
        dir = (dir + side + 0x380 + e:rnd()) & 0xFFFF
        e:set_timer(0, dir)
        local ahead = (e:rnd() & 0x3F) + 400
        local sx, sz = e:rotate(dir, ahead, 0)
        e:set_speed(sx, 0, sz)
    elseif sub == 2 then
        e:routines(1, 0, 0, 0)
        e:set_var(T21E, 0)
        return
    elseif sub ~= 1 then
        return
    end
    e:set_angle((e:angle() + goto_target(e, 0x60)) & 0xFFFF)
    e:add_speed_xz_at((i16(e:var(ADD_DIR)) + i16(e:timer(0))) & 0xFFFF)
    e:set_routine(2, 1 + joint_step(e))
end

-- `funcs_46F38E[0]` (`FUN_0046f400`), by `Routine_1`.
local FLOOR <const> = {
    [0] = rest,
    [1] = walk,
    [2] = walk,
    [3] = walk,
    [4] = turn,
    [5] = aim,
    [6] = aim,
    [7] = lunge,
    [8] = spit,
    [9] = leap,
}

-- `FUN_004735a0`: `Be_flg`'s height bits for the kind.
local function heights(e)
    local flags = e:be_flg() & 0xE3FFFFFF
    local kind = e:var(KIND)
    if kind == 0 then
        flags = flags | 0xC000000
    elseif kind == 1 then
        flags = flags | 0x10000000
    end
    -- Kind 3's depend on the player's height; not ported.
    e:set_be_flg(flags)
end

-- `FUN_0046f370`.
local function move(e)
    -- `FUN_004733d0`: the partner (`fg_status`'s `0x10000000`) isn't ported.
    e:set_var(TO_PARTNER, 0)
    local state = e:var(KIND) == 0 and FLOOR[e:routine(1)]
    if state then
        state(e)
    end
    heights(e)
    if e:var(COOLDOWN) ~= 0 then
        e:set_var(COOLDOWN, e:var(COOLDOWN) - 1)
    end
    if e:var(T232) ~= 0 then
        e:set_var(T232, e:var(T232) - 1)
    end
    if e:var(T21C) == 0 then
        e:set_var(T21E, 0)
        e:set_var(LAST, e:routine(1))
        e:set_var(LAST + 1, e:routine(2))
        e:set_var(LAST + 2, e:routine(3))
    else
        e:set_var(T21E, (e:var(T21E) + 1) & 0xFFFF)
    end
end

-- `DAT_00528800`'s rows that the floor's hits and deaths use: `Esp_call`'s
-- kind, row and size. None of them has the top bit, which would place it on
-- the part's matrix instead.
local BLOOD_ROWS <const> = {
    [0] = { 0, 0x19, 0x2000 },
    { 0, 0x19, 0x2800 },
    { 9, 0x0A, 0x0800 },
    { 0, 0x09, 0x1000 },
}

-- `FUN_00472df0`: blood at a part, where `Joint_trans` left it. The way is
-- `Direction_ck` from the body to itself. On the floor nothing is added to
-- the part's place.
local function blood(e, part, row)
    local effect = BLOOD_ROWS[row]
    if effect == nil then
        return
    end
    local x, _, z = e:pos()
    local dir = e:direction(x, z, x, z)
    local px, py, pz = e:own_part(part)
    e:call_effect(effect[1], effect[2], effect[3], dir, px, py, pz)
end

-- `Water_ck` under the body, kept once found.
local function find_water(e)
    if e:var(WATER) == 0 then
        local x, _, z = e:pos()
        e:set_var(WATER, e:water_ck(x, z))
    end
end

-- `FUN_004731a0`: a leg comes off, two parts thrown with `Parts_bomb`, while
-- more than two are left; otherwise blood on the body.
local function lose_leg(e)
    if e:var(HIT_BY) == 0xE then
        return
    end
    local leg = e:rnd() & 7
    if e:var(T221) > 2 and e:var(LOST_LEGS) & (1 << leg) == 0 then
        local first = leg * 2 + 3
        e:set_var(T221, e:var(T221) - 1)
        e:set_var(LOST_LEGS, e:var(LOST_LEGS) | (1 << leg))
        for part = first, first + 1 do
            e:blow_part(part, (e:angle() + 0x800) & 0xFFFF, 600, -120, 10)
        end
        blood(e, first, 1)
        return
    end
    blood(e, 0x13, 1)
end

-- `FUN_00472450`: the body, part 19, falls away, with blood.
local function lose_body(e)
    e:set_neck_flags(e:neck_flags() | 1)
    e:blow_part(0x13, (e:angle() + 0x800) & 0xFFFF, 0, -240, 0xF)
    blood(e, 0x13, 1)
    for _ = 1, 4 do
        blood(e, 0x13, 2)
    end
end

-- `FUN_004732a0`: the young spiders (`Em_26`) a death sends out. `Em_26`
-- isn't ported, so only the draw before the call, for how many, is made.
local function send_young(e)
    e:rnd()
end

-- `BYTE_005286f4`: where a handgun's hit sends it, by `Rnd & 7`.
local REACTIONS <const> = { [0] = 1, 9, 1, 9, 1, 5, 1, 5 }

-- `FUN_00471170` on the floor (`0x471190`): the flinch (clip 1), then the
-- leap aside (9) for a knife or a hit in the first band, and otherwise a
-- walk, a leap or a turn by `Rnd`.
local function flinch(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        e:clip(1, 0, 3)
        if e:var(HIT_BY) ~= 1 then
            blood(e, 0, 0)
        end
    elseif sub == 2 then
        if e:routine(1) == 1 or e:damage_no() // 3 == 0 then
            e:routines(1, 9, 0, 0)
            return
        end
        e:routines(1, REACTIONS[e:rnd() & 7], 0, 0)
        return
    elseif sub ~= 1 then
        return
    end
    e:set_routine(2, e:routine(2) + joint_step(e))
end

-- `FUN_00471840`: weapon 4's hit bleeds from the body and a leg, and takes
-- a leg off once a region's `D_life` has run out.
local LEG_ROWS <const> = { [0] = 0, 1, 2, 0x13 }
local function flinch_bleeding(e)
    if e:routine(2) == 0 then
        blood(e, LEG_ROWS[e:rnd() & 3], 3)
        local leg = e:rnd() & 7
        if e:var(LOST_LEGS) & (1 << leg) == 0 then
            blood(e, leg * 2 + 3, 3)
        end
        if e:d_life(0) >= 0x80 or e:d_life(1) >= 0x80 or e:d_life(2) >= 0x80 then
            lose_leg(e)
            e:set_d_life(6)
        end
    end
    flinch(e)
end

-- `FUN_00471340` on the floor (`0x471360`): the magnums' and shotguns' hits
-- take a leg off, then it leaps aside.
local function flinch_hard(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        e:clip(1, 0, 3)
        lose_leg(e)
    elseif sub == 2 then
        e:routines(1, 9, 0, 0)
        return
    elseif sub ~= 1 then
        return
    end
    e:set_routine(2, e:routine(2) + joint_step(e))
end

-- `DWORD_00528694`, by weapon: only the weapons arklay fires, and only the
-- floor's half of each.
local HURTS <const> = {
    [2] = flinch,
    [3] = flinch,
    [4] = flinch_bleeding,
    [5] = flinch_hard,
    [6] = flinch_hard,
    [7] = flinch_hard,
    [8] = flinch_hard,
}

-- `FUN_00471110`: `Routine_0` 2.
local function hurt(e)
    if e:routine(2) == 0 then
        e:set_var(HIT_BY, e:routine(1))
    end
    find_water(e)
    local entry = e:var(KIND) == 0 and HURTS[e:routine(1)]
    if entry then
        entry(e)
    end
end

-- The end of a fall into water: the body floats (`FUN_004728b0`'s 3).
local function lands_in_water(e)
    if e:var(WATER) == 0 then
        return
    end
    -- Weapons 9, 10 and 16 splash here (row 7); arklay doesn't fire them.
    e:set_var(FLOAT, 0xFF)
    if e:var(BOB) == 0 then
        e:set_var(BOB, 3)
        e:set_var(F23F, 1)
    end
end

-- `FUN_00472130`: a body shot again by a handgun throws the last of it.
local function die_again(e)
    e:clip(0xD, 0, 7)
    e:joint(0x200)
    send_young(e)
    blood(e, 0, 0)
    e:routines(7, 0, 0, 0)
    e:set_be_flg(e:be_flg() | 0x42)
    if e:var(FLOAT) >= 0x80 then
        e:set_pos_y(i16(e:var(WATER)) + 0x46)
    end
end

-- `FUN_00471c10`: the handguns' death. On the floor (`0x471c40`) it falls on
-- its back (clip 12), and its body takes one more shot with a life of 1.
local function die_plain(e)
    if e:var(REVIVED) ~= 0 then
        die_again(e)
        return
    end
    if e:var(KIND) ~= 0 then
        return
    end
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(0xC, 0, 7)
        if e:var(HIT_BY) ~= 1 then
            blood(e, 0, 0)
        end
        e:set_be_flg(e:be_flg() | 0x42)
    end
    e:joint(0x200)
    if e:now_seq() & 0x80000 == 0 then
        return
    end
    e:set_be_flg(e:be_flg() & 0xE3FFFFFF)
    lands_in_water(e)
    e:routines(7, 0, 0, 0)
    if e:var(T239) == 0 then
        e:set_life(1)
        e:set_var(REVIVED, 1)
    end
end

-- `FUN_004721a0`: the magnums' and shotguns' death throws its body off.
local function die_blown(e)
    if e:var(REVIVED) ~= 0 then
        e:clip(0xD, 0, 7)
        e:joint(0x200)
        send_young(e)
        blood(e, 0x13, 1)
        lose_body(e)
        e:set_be_flg(e:be_flg() | 0x42)
        e:routines(7, 0, 0, 0)
        if e:var(FLOAT) >= 0x80 then
            e:set_pos_y(i16(e:var(WATER)) + 0x46)
        end
        return
    end
    if e:var(KIND) ~= 0 then
        return
    end
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_be_flg(e:be_flg() | 2)
        e:set_var(FLOAT, 1)
        e:clip(0xC, 0, 7)
        blood(e, 0x13, 1)
        lose_body(e)
        send_young(e)
    end
    e:joint(0x200)
    if e:now_seq() & 0x40000 == 0 then
        return
    end
    lands_in_water(e)
    e:routines(7, 0, 0, 0)
end

-- `DWORD_00528724`, by weapon: only the weapons arklay fires.
local DIES <const> = {
    [2] = die_plain,
    [3] = die_plain,
    [4] = die_plain,
    [5] = die_blown,
    [6] = die_blown,
    [7] = die_blown,
    [8] = die_blown,
}

-- `BYTE_005287f8`: the body's bob in the water, by `Timer1 >> 3 & 7`.
local BOBS <const> = { [0] = 0, 8, 16, 32, 32, 16, 8, 0 }

-- `FUN_004728b0`: a body in water bobs there (`+0x23e`'s 3). Its rise to
-- the water (1 and 2) and its rest on `Ground` out of water follow
-- `FUN_00471e50`, the ceiling's fall, and aren't ported.
local function bob(e)
    local water = i16(e:var(WATER))
    if e:var(BOB) ~= 3 or water == 0 then
        return
    end
    local timer = e:timer(1)
    e:set_timer(1, (timer + 1) & 0xFFFF)
    e:set_pos_y(BOBS[(timer >> 3) & 7] + 0x46 + water)
end

-- `FUN_00471bb0`: `Routine_0` 3.
local function die(e)
    if e:routine(2) == 0 then
        e:set_var(HIT_BY, e:routine(1))
    end
    find_water(e)
    local entry = DIES[e:routine(1)]
    if entry then
        entry(e)
    end
    bob(e)
    e:set_died()
end

-- `0x472730`: `Routine_0` 7, the body. It twitches (clip 13) every 30 to 61
-- ticks, floats when in water, and can be shot at low, and high too when
-- it's above the player.
local function dead(e)
    if e:routine(1) == 0 then
        if e:joint(0x200) then
            e:set_routine(1, 1)
            e:set_timer(2, (e:rnd() & 0x1F) + 0x1E)
        end
    else
        local left = (e:timer(2) - 1) & 0xFF
        e:set_timer(2, left)
        if left == 0 then
            e:clip(0xD, 0, 7)
            e:set_routine(1, 0)
        end
    end
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_var(T225, 2)
        e:set_timer(1, 0)
    end
    if e:var(FLOAT) >= 0x80 then
        bob(e)
    end
    e:set_be_flg(e:be_flg() | 0x4000000)
    local _, y = e:pos()
    local _, player_y = e:player_pos()
    if y < player_y then
        e:set_be_flg(e:be_flg() | 0x8000000)
    end
end

function update(e)
    if e:var(T23D) ~= 0 then
        e:set_var(T23D, e:var(T23D) - 1)
    end
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 then
        move(e)
    elseif routine == 2 then
        hurt(e)
    elseif routine == 3 then
        die(e)
    elseif routine == 7 then
        dead(e)
    end
    -- 4 is `nullsub_1`; 5 and 6 are empty.
end
