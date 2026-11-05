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
-- (9). The ceiling and the wall, damage and death (`Routine_0` 2 to 7),
-- `FUN_004727e0`'s water and `FUN_00473530`'s blood aren't ported: a spider
-- that reaches them stands still.

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
local T221 <const> = 13 -- `+0x221`.
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
    -- `Water` (`+0x10c`) is taken as 0: arklay has no water yet.
    e:sound(STEPS[e:var(STEP)])
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
    for index = 0, 13 do
        e:set_var(index, 0)
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

function update(e)
    if e:var(T23D) ~= 0 then
        e:set_var(T23D, e:var(T23D) - 1)
    end
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 then
        move(e)
    end
end
