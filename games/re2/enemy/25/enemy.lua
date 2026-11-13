-- The giant spider, `Em_25` (entity id 0x25).
--
-- `Em_25` runs `Routine_0` from `funcs_46F049`: 0 is the init
-- (`FUN_0046f0c0`) and 1 the move (`FUN_0046f370`), which runs the state
-- table of the spider's kind, `+0x222`: 0 on the floor, 1 on the ceiling and
-- 3 on a wall, from `Type`'s low nibble, and 2 between a wall and the
-- ceiling. The engine runs `Oba_ck_em`,
-- `Sca_ck_em` (with `0x600`) and `nFloor` after `update`, and leaves the
-- place part 12 had at the end of the last tick in `FOOT`.
--
-- Ported: the init, and on the floor the rest (`Routine_1` 0), the walks
-- (1 to 3), the turn on the spot (4), the turns toward the player (5 and 6),
-- the lunge and its bite (7), the spit (8) and the leap aside from a noise
-- (9). On the ceiling (kind 1) the rest, the walks and the way onto a wall
-- (10), the way round another spider (11), the lunge (7) and the floor's
-- others; on a wall (kinds 2 and 3) the way down (1), the wait (2), the
-- crawl (3), the climb (4), the way onto the ceiling (5), the lunge (6) and
-- the spit (7). A hit (`Routine_0` 2), the death (3) and the body (7), for
-- weapons 2 to 8, 0xD, 0xF, 0x10 and 0x13, on each kind. Another spider in play is its partner
-- (`Ck_em_id`, `+0x240`), and its hits and deaths send young spiders
-- (`Em_26`, `send_young`). `FUN_004732a0`'s `fg_system` `0x1000000` draw,
-- `FUN_004727e0`'s water, `FUN_00473530`'s blood, the player's partner
-- (`splayer_work`) as a target and the spit's `Routine_2` 3
-- (`FUN_0046fbb0`) aren't ported.

local COOLDOWN <const> = 0 -- `+0x218`: the rest waits for it before an attack.
local ADD_DIR <const> = 1 -- `+0x21a`: the turn `Add_speedXZ` adds.
local KIND <const> = 2 -- `+0x222`: 0 floor, 1 ceiling, 2 between, 3 wall.
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
local SCE <const> = 22 -- `+0x223`: a fall from a wall or the ceiling is under way.
local WALL_DIR <const> = 23 -- `+0x226`: the side of the wall's record it's on, then its turn there.
local RECORD <const> = 24 -- `+0x228`: the wall's record, from 1, or 0.
local TOP <const> = 25 -- `+0x22c`: the height a climb stops at.
local FACING <const> = 26 -- `+0x230`: which way the wall faces, by quarters.
local NEXT <const> = 27 -- `+0x231`: the state after the way down, and a hit's climb.
local BETWEEN <const> = 28 -- `+0x236`: between a wall and the ceiling, with `fg_room_enemy`'s 0x20.
local RISE <const> = 29 -- `+0x244`: how far a body has sunk into water.
local GROUND <const> = 30 -- `Ground`, which the engine finds under it before the tick.
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

local function abs(value)
    return value < 0 and -value or value
end

-- C's division, which truncates toward zero.
local function tdiv(a, b)
    local q = a // b
    if q < 0 and q * b ~= a then
        q = q + 1
    end
    return q
end

-- `FUN_00472be0`: `Spd`, turned by `Cdir` and the angles given, added to the
-- place.
local function move_by(e, ax, ay, az)
    local cx, cz = e:cdir()
    local sx, sy, sz = e:speed()
    e:add_pos(e:rot_apply(cx + ax, e:angle() + ay, cz + az, sx, sy, sz))
end

-- `Add_speedXYZ`: `Spd`, turned by `Cdir_y` and `Cdir_z` and the angles
-- given, added to the place.
local function add_speed_xyz(e, ay, az)
    local _, cz = e:cdir()
    local sx, sy, sz = e:speed()
    e:add_pos(e:rot_apply(0, e:angle() + ay, cz + az, sx, sy, sz))
end

-- `L_pl`, or `L_spl` & 0x3fffffff when it aims at the partner. Only the
-- player is ported.
local function distance(e)
    return e:distance()
end

-- `FUN_00472aa0`: `Goto00_ck` toward the target, or off the ceiling or a
-- wall toward the point opposite it.
local function goto_target(e, rate)
    local x, _, z = e:player_pos()
    if e:var(KIND) ~= 0 then
        local at_x, _, at_z = e:pos()
        x = at_x - (x - at_x)
        z = at_z - (z - at_z)
    end
    return e:goto_ck(x, z, rate)
end

-- `FUN_00472d80`: `Goto00_ck` toward the player, as the body would turn on
-- the floor facing the way the wall does.
local function toward_player(e, rate)
    local x, _, z = e:pos()
    local px, _, pz = e:player_pos()
    return e:goto_ck_at(x, z, (e:var(FACING) & 3) << 10, px, pz, rate)
end

-- `FUN_004e3250`: a record's edge on a side, or the side itself past 3.
local function edge(e, record, side)
    if side > 3 then
        return side
    end
    return e:sca_edge(record, side)
end

-- `FUN_00472b10`: the first record of `Sca_ck_info`'s list whose side faces
-- the body's way, from 1, and whose floor isn't below the body's height.
-- Each record it looks at is kept, with its side.
local function find_wall(e, list)
    local _, y = e:pos()
    local high = tdiv(y, -1800) & 0xFFFFFFFF
    for _, record in ipairs(list) do
        local x, _, z = e:pos()
        local side = e:sca_side(record, x, z)
        e:set_var(RECORD, record)
        e:set_var(WALL_DIR, (side << 10) & 0xFFFF)
        local floor = select(6, e:sca_rect(record))
        if side << 10 == (e:angle() + 0x200) & 0xC00 and high <= floor then
            return side + 1
        end
    end
    return 0
end

-- `FUN_00472c70`: whether the body is within 2500 of the wall record's end
-- that `dir` turns it toward.
local function near_end(e, dir)
    local record = e:var(RECORD)
    if record == 0 then
        return false
    end
    local facing = e:var(FACING)
    local x, _, z = e:pos()
    if facing & 1 == 0 then
        if i16(dir) == 0x400 then
            return edge(e, record, 3) - 0x9C4 < z
        end
        return z < edge(e, record, 1) + 0x9C4
    end
    if (facing * 0x400 - 0x400 + dir) & 0xFFFF == 0x400 then
        return x < edge(e, record, 2) + 0x9C4
    end
    return edge(e, record, 0) - 0x9C4 < x
end

-- `FUN_00472d20`: whether the player is within 3000 of a record's side.
local function player_near_edge(e, record, side)
    if record == 0 then
        return false
    end
    local at = edge(e, record, side)
    local px, _, pz = e:player_pos()
    local along = side & 1 == 0 and px or pz
    return abs(along - at) < 3000
end

-- `FUN_00472ba0`: a turn across the wall, of `Cdir_y` or `Cdir_x` by the
-- way it faces, kept in `+0x226` too.
local function shift(e, turn)
    local facing = e:var(FACING)
    if facing & 1 ~= 0 then
        e:set_angle((e:angle() + turn) & 0xFFFF)
    else
        if facing & 2 == 0 then
            turn = -turn
        end
        local cx, cz = e:cdir()
        e:set_cdir(cx + turn, cz)
    end
    e:set_var(WALL_DIR, (e:var(WALL_DIR) + turn) & 0xFFFF)
end

-- `FUN_00470fe0`: the way a wall's spit flies.
local function wall_aim(e)
    local turn = i16(e:var(WALL_DIR))
    local facing = e:var(FACING)
    if facing == 0 then
        turn = -turn
    end
    return (facing * 0x400 - turn // 4 + turn) & 0xFFF
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
    for index = 0, RISE do
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
    -- `Ck_em_id`: another spider in play is its partner (`+0x240`).
    e:set_parent(e:other_of(0x25))
    local kind = e:kind() & 0xF
    if kind == 0 then
        e:set_var(KIND, 0)
        e:set_var(ADD_DIR, 0)
        e:set_be_flg(e:be_flg() | 0xC000000)
        return
    end
    if kind == 2 or kind == 3 then
        e:set_var(KIND, 1)
        e:set_var(ADD_DIR, 0x800)
        e:set_cdir(0, 0x800)
        e:set_body_box(0, 0x3E8, 0x578, 0x320)
        e:set_be_flg(e:be_flg() | 0x10000000)
        local _, y = e:pos()
        e:set_var(TOP, y + 0x4E2)
    elseif kind >= 4 and kind <= 11 then
        e:set_var(KIND, 3)
        e:set_var(ADD_DIR, 0x800)
        local kind_byte = e:kind() & 0xFF
        local facing = ((kind_byte - 4) >> 1) & 0xFF
        e:set_var(FACING, facing)
        e:set_angle((facing << 10) & 0xFFFF)
        -- `DAT_00528558`.
        e:set_body_box(0, 0x3E8, 0x578, 0x1E)
        local x, _, z = e:pos()
        local _, list = e:sca_ck_info(x, z, e:floor(), 0x400, 800)
        find_wall(e, list)
        if kind_byte & 0xF0 == 0 then
            e:set_var(TOP, -0xF78)
        else
            e:set_var(TOP, ((kind_byte & 0xF0) >> 4) * -0x708 + 0x4E2)
        end
        e:set_var(T225, 1)
    end
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

-- `FUN_00472f70(e, row, dir)`: `Esp_call` 0x1d (the spit) from part 0's
-- matrix, 1000 ahead and 100 aside, or for a row with the top bit, from
-- that point turned by `Cdir` from part 0's place. Its `Sca_ck_hit` first
-- only sets the hit globals, which nothing here reads.
local function spit_out(e, row, dir)
    if row & 0x80 == 0 then
        e:effect(0x1D, row, 0x1000, dir, 0, 1000, 0, -100)
        return
    end
    local cx, cz = e:cdir()
    local x, y, z = e:rot_apply(cx, e:angle(), cz, 1000, 0, -100)
    local px, py, pz = e:own_part(0)
    e:call_effect(0x1D, row & 0x7F, 0x1000, i16(dir), x + px, y + py, z + pz)
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

-- `Routine_1` 0 on the ceiling (`0x46fe30`).
local function ceiling_rest(e)
    rest_wait(e)
    local near = distance(e) & 0xFFFF
    local facing = goto_target(e, 0x80)
    goto_target(e, 0x100)
    goto_target(e, 0x400)
    if e:noise() & 1 ~= 0 then
        e:routines(1, 9, 0, 0)
        if e:rnd() & 1 ~= 0 then
            e:routines(1, 5, 0, 0)
        end
    end
    if e:var(COOLDOWN) ~= 0 then
        return
    end
    if near < 7500 and near > 3000 then
        if facing == 0 and (1 << (e:rnd() & 0xF)) & 0x340 ~= 0 then
            e:routines(1, 7, 0, 0)
        else
            e:routines(1, 1, 0, 0)
        end
    end
    if near < 3000 and facing == 0 then
        if (1 << (e:rnd() & 0xF)) & 0x340 ~= 0 then
            e:routines(1, 8, 0, 0)
            return
        end
        e:routines(1, 7, 0, 0)
    end
end

-- `Routine_1` 1 to 3 on the ceiling (`0x46ff30`): the floor's walk, then
-- at the edge of a wall's record (`0x400` and not `0x200`) the way down it
-- (10), when the wall faces its way, neither of the wall's ends is near and
-- the player is near the wall. With another spider between a wall and the
-- ceiling, or `+0x232` still counting, it turns instead (4).
local function ceiling_walk(e)
    walk(e)
    local x, _, z = e:pos()
    if e:sca_ck_info(x, z, e:floor(), 0x200, 800) ~= 0 then
        return
    end
    local flags, list = e:sca_ck_info(x, z, e:floor(), 0x400, 800)
    if flags == 0 then
        return
    end
    -- The partner on a wall within 2500 is kept away from (11).
    if e:has_parent() and e:parent_routine(0) ~= 7 and e:parent_var(KIND) >= 2 then
        local px, _, pz = e:parent_pos()
        if e:hypot(x - px, z - pz) < 0x9C4 then
            e:routines(1, 11, 0, 0)
            return
        end
    end
    if e:var(T232) ~= 0 or e:room_enemy() & 0x20 ~= 0 then
        e:routines(1, 4, 0, 0)
        return
    end
    local found = find_wall(e, list)
    e:set_var(FACING, found)
    if found == 0 then
        return
    end
    e:set_var(FACING, found - 1)
    if not near_end(e, 0x400) and not near_end(e, -0x400)
        and player_near_edge(e, e:var(RECORD), e:var(FACING)) then
        e:routines(1, 10, 0, 0)
        e:set_var(BETWEEN, 1)
        e:set_room_enemy(e:room_enemy() | 0x20)
    end
end

-- `Routine_1` 7 on the ceiling (`0x4700c0`): a step, a turn to face away
-- from the target, and a run that spits down at it every 12 ticks.
local function ceiling_lunge(e)
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
        e:add_speed_xz_at(e:var(ADD_DIR))
        footstep(e)
        return
    end
    if sub == 2 then
        e:clip(5, 0, 3)
        local draw = e:rnd()
        e:set_routine(2, 3)
        e:set_timer(0, (draw & 0x1F) + 0x2D)
        sub = 3
    end
    if sub == 3 then
        local turn = goto_target(e, 0x40)
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
        e:set_timer(2, 1)
        e:set_speed_x(300)
        e:set_timer(0, (draw & 0x1F) + 10)
        sub = 5
    end
    if sub == 5 then
        e:set_angle((e:angle() + goto_target(e, 0x10)) & 0xFFFF)
        e:add_speed_xz_at(e:var(ADD_DIR))
        e:joint(0x400)
        local shot = (e:timer(2) - 1) & 0xFF
        e:set_timer(2, shot)
        if shot == 0 then
            e:sound(3)
            spit_out(e, 0x8A, e:angle())
            e:set_timer(2, 0xC)
        end
        local left = e:timer(0)
        e:set_timer(0, (left - 1) & 0xFFFF)
        if left == 0 or goto_target(e, 0x200) ~= 0 or e:sca_info() & 1 ~= 0
            or e:at_em_no() ~= 0xFF then
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
        e:set_var(COOLDOWN, 0x3C)
    end
    footstep(e)
end

-- `Routine_1` 11 on the ceiling (`0x4704c0`): a walk away from the partner
-- for 60 ticks.
local function avoid(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(2, 0, 3)
        e:set_timer(0, 0x3C)
        local px, _, pz = e:parent_pos()
        e:set_timer(1, e:goto_ck(px, pz, 0x40) & 0xFFFF)
    end
    e:set_angle((e:angle() + e:timer(1)) & 0xFFFF)
    e:joint(0x400)
    footstep(e)
    -- `FUN_004729c0(1)`, and `WORD_005285f8`.
    e:set_speed_x(foot_step(e) + 0x64)
    local left = (e:timer(0) - 1) & 0xFFFF
    e:set_timer(0, left)
    if left == 0 then
        e:routines(1, 0, 0, 0)
    end
    if e:sca_info() & 1 == 0 then
        e:add_speed_xz_at(e:var(ADD_DIR))
    end
end

local KIND_2

-- `Routine_1` 10 on the ceiling (`0x470320`): a turn to the wall, then over
-- its edge onto it (kind 2), where it carries on at once.
local function to_wall(e)
    local sub = e:routine(2)
    if sub == 0 then
        local wall = e:var(WALL_DIR)
        local angle = e:angle() & 0xFFF
        e:set_angle(angle)
        local diff = (wall - angle) & 0xFFFF
        e:set_routine(2, 1)
        e:set_timer(1, 0x18)
        e:set_timer(0, diff & 0xFFF)
        if diff & 0x800 ~= 0 then
            e:set_timer(1, 0xFFE8)
            e:set_timer(0, (angle - wall) & 0xFFF)
        end
        e:clip(3, 0, 3)
        sub = 1
    end
    if sub == 1 then
        if i16(e:timer(0)) < 1 then
            e:set_routine(2, 2)
            e:set_angle(e:var(WALL_DIR))
        else
            e:set_angle((e:angle() + e:timer(1)) & 0xFFFF)
            e:set_timer(0, (e:timer(0) - 0x18) & 0xFFFF)
        end
        e:joint(0x400)
        footstep(e)
        return
    end
    if sub == 2 then
        e:set_routine(2, 3)
        local _, y = e:speed()
        e:set_speed(0x19, y, 0)
        -- `DAT_00528538`.
        e:set_body_box(-0x64, 0x3E8, 0x578, 0x1E)
        e:clip(0xE, 0, 3)
        sub = 3
    end
    if sub == 3 then
        e:add_speed_xz_at(e:var(ADD_DIR))
        if e:joint(0x400) then
            e:routines(1, 0, 0, 0)
            e:set_var(KIND, 2)
            e:set_speed_x(0x1C2)
            e:add_speed_xz_at(e:var(ADD_DIR))
            local _, y = e:pos()
            e:set_pos_y(y + 0x4E2)
            e:set_var(TOP, y + 0x4E2)
            e:set_var(BETWEEN, 0)
            e:set_room_enemy(e:room_enemy() & 0xDF)
            KIND_2[0](e)
        end
    end
    footstep(e)
end

-- `FUN_004706a0`: settle on the wall: lean onto it (`Cdir_z` or `Cdir_x`),
-- the wall's body box and the rest's clip.
local function settle(e)
    if e:var(FACING) & 1 == 0 then
        e:set_cdir(0, 0x400)
    else
        e:set_cdir(e:angle(), 0)
    end
    -- `DAT_00528558`.
    e:set_body_box(0, 0x3E8, 0x578, 0x1E)
    e:set_var(WALL_DIR, 0)
    e:set_var(T225, 1)
    e:clip(0, 0, 3)
    e:joint(0x400)
end

-- `Routine_1` 0 for kind 2 (`0x4705a0`): settle, then down the wall (1)
-- while the player is more than 3500 below, as far as the water or their
-- height, or wait (2).
local function wall_arrive(e)
    settle(e)
    local x, y, z = e:pos()
    local _, py = e:player_pos()
    local below = py - y
    if below > 0xDAC then
        local water = e:water_ck(x, z)
        local steps
        if water < i16(e:var(GROUND)) then
            steps = i16(tdiv(water - y - 0x898, 100))
            if steps < 0 then
                steps = 0
            end
        else
            steps = i16(tdiv(below - 0xDAC, 100))
        end
        e:set_timer(0, steps & 0xFFFF)
        if steps ~= 0 then
            local draw = e:rnd()
            e:routines(1, 1, 0, 0)
            e:set_timer(0, (steps + (draw & 3)) & 0xFFFF)
            e:set_var(NEXT, 2)
            return
        end
    end
    e:routines(1, 2, 1, 0)
    e:set_timer(0, (e:rnd() & 0xF) + 0x1E)
end

-- `Routine_1` 0 on a wall (`0x471040`): settle and watch (2).
local function wall_start(e)
    settle(e)
    e:routines(1, 2, 0, 0)
end

-- `Routine_1` 1 on a wall (`0x470710`): down it for `Timer0` ticks, then on
-- to `+0x231`.
local function descend(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_speed(100, 0, 0)
        e:clip(2, 0, 3)
    end
    if e:timer(0) ~= 0 then
        move_by(e, 0, 0, 0)
        e:set_timer(0, e:timer(0) - 1)
    end
    e:joint(0x400)
    footstep(e)
    if e:timer(0) == 0 then
        e:routines(1, e:var(NEXT), 0, 0)
        e:set_timer(0, (e:rnd() & 0xF) + 0xF)
    end
end

-- `Routine_1` 2 for kind 2 (`0x4707c0`): a wait, then the spit (7) with the
-- player near or the crawl (3).
local function hang(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(0, 0, 3)
    end
    e:joint(0x400)
    local left = (e:timer(0) - 1) & 0xFFFF
    e:set_timer(0, left)
    if left == 0 then
        if distance(e) < 3000 and (1 << (e:rnd() & 0xF)) & 0x340 ~= 0 then
            e:routines(1, 7, 0, 0)
            return
        end
        e:routines(1, 3, 0, 0)
    end
end

-- `Routine_1` 2 on a wall (`0x471070`): it waits for the player within 7200
-- or near the wall, then spits (7) or crawls (3).
local function watch(e)
    e:joint(0x400)
    if distance(e) < 0x1C20 or player_near_edge(e, e:var(RECORD), e:var(FACING)) then
        if distance(e) < 3000 then
            e:routines(1, 7, 0, 0)
            return
        end
        e:routines(1, 3, 0, 0)
    end
end

-- `Routine_1` 3 on a wall (`0x470860`): a crawl across it, turning toward the
-- player, until it has turned a quarter (the lunge, 6), with pauses.
local function crawl(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        local turn = toward_player(e, 0x20)
        e:set_timer(1, turn & 0xFFFF)
        if turn == 0 then
            e:routines(1, 7, 0, 0)
            e:joint(0x400)
            footstep(e)
            return
        end
        local way = turn >= 1 and 0x400 or -0x400
        if e:var(FACING) == 0 then
            way = -way
        end
        e:set_timer(0, way & 0xFFFF)
        sub = 1
    end
    if sub == 1 then
        e:set_routine(2, 2)
        local draw = e:rnd()
        e:clip(2, 0, 3)
        e:set_timer(2, (draw & 0xF) + 0xF)
        sub = 2
    end
    if sub == 2 then
        shift(e, i16(e:timer(1)))
        if (e:var(WALL_DIR) ~ e:timer(0)) & 0xFFF == 0 then
            e:routines(1, 6, 0, 0)
        else
            local left = (e:timer(2) - 1) & 0xFF
            e:set_timer(2, left)
            if left == 0 then
                e:set_routine(2, 3)
                local draw = e:rnd()
                e:clip(0, 0, 3)
                e:set_timer(2, (draw & 0xF) + 1)
            end
        end
    elseif sub == 3 then
        local left = (e:timer(2) - 1) & 0xFF
        e:set_timer(2, left)
        if left == 0 then
            e:set_routine(2, 1)
        end
    end
    e:joint(0x400)
    footstep(e)
end

-- `WORD_00528664`: the climb's turns.
local CLIMBS <const> = { [0] = 0x40, -0x40, 0x40, -0x40, 0x20, -0x40, 0x40, -0x20 }

-- `Routine_1` 4 on a wall (`0x4709a0`): a turn across the wall until it
-- faces up, then up to `+0x22c` (5), unless another spider is between a
-- wall and the ceiling.
local function climb(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        local turn = 0x40
        if e:var(NEXT) == 0 then
            turn = CLIMBS[e:rnd() & 7]
        end
        e:set_timer(1, turn & 0xFFFF)
        e:set_var(BETWEEN, 1)
        sub = 1
    end
    if sub == 1 then
        e:set_routine(2, 2)
        if e:var(NEXT) == 0 then
            e:set_timer(2, (e:rnd() & 0xF) + 0xF)
        else
            e:set_timer(2, 0xFF)
        end
        if abs(i16(e:timer(1))) == 0x20 then
            e:clip(2, 0, 3)
        else
            e:clip(1, 0, 3)
        end
        sub = 2
    end
    if sub == 2 then
        local turn = i16(e:timer(1))
        shift(e, turn)
        local size = abs(turn)
        if (e:var(WALL_DIR) - 0x800 + size) & 0xFFF <= size * 2 then
            if e:room_enemy() & 0x20 == 0 then
                e:set_routine(2, 5)
            else
                e:set_routine(2, 4)
                e:clip(0, 0, 3)
            end
        end
        local left = (e:timer(2) - 1) & 0xFF
        e:set_timer(2, left)
        if left == 0 then
            e:set_routine(2, 3)
            local draw = e:rnd()
            e:clip(0, 0, 3)
            e:set_timer(2, (draw & 0xF) + 0xF)
        end
    elseif sub == 3 then
        local left = (e:timer(2) - 1) & 0xFF
        e:set_timer(2, left)
        if left == 0 then
            e:set_routine(2, 1)
        end
    elseif sub == 4 or sub == 5 then
        if sub == 5 or e:room_enemy() & 0x20 == 0 then
            e:set_routine(2, 6)
            e:set_var(WALL_DIR, 0x800)
            e:set_var(BETWEEN, 1)
            e:set_room_enemy(e:room_enemy() | 0x20)
            e:set_speed(100, 0, 0)
            e:clip(2, 0, 3)
        end
    elseif sub == 6 then
        move_by(e, 0, 0, 0)
        local _, y = e:pos()
        if y <= e:var(TOP) then
            e:set_pos_y(e:var(TOP))
            e:routines(1, 5, 0, 0)
        end
    end
    e:joint(0x400)
    footstep(e)
end

-- `Routine_1` 5 on a wall (`0x470b90`): over the top onto the ceiling
-- (kind 1).
local function to_ceiling(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        if e:var(FACING) & 1 == 0 then
            e:set_speed(0x19, 0, 0)
        else
            e:set_speed(0, 0x19, 0)
        end
        -- `DAT_00528538`.
        e:set_body_box(-0x64, 0x3E8, 0x578, 0x1E)
        e:clip(0, 0, 3)
    elseif sub ~= 1 then
        if sub == 2 then
            add_speed_xyz(e, 0, e:var(WALL_DIR))
            if e:joint(0x400) then
                e:set_var(KIND, 1)
                e:set_var(ADD_DIR, 0x800)
                if e:var(FACING) & 1 == 0 then
                    e:set_angle((e:angle() + 0x800) & 0xFFFF)
                else
                    e:set_angle((e:cdir() + 0x800) & 0xFFFF)
                end
                e:set_cdir(0, 0x800)
                e:set_var(T232, 0x96)
                local _, y = e:pos()
                e:set_pos_y(y - 0x1DB)
                e:set_speed_x(0x4C9)
                e:add_speed_xz_at(0x800)
                -- `DAT_00528518`, with `Ofs_y` at 0.
                e:set_body_box(0, 0x3E8, 0x578, 0x320)
                e:routines(1, 0, 0, 0)
                e:set_var(BETWEEN, 0)
                e:set_room_enemy(e:room_enemy() & 0xDF)
                e:set_var(T225, 0)
            end
        end
        footstep(e)
        return
    end
    -- The partner in play on the ceiling within 2500 holds it here.
    local wait = false
    if e:has_parent() and e:parent_routine(0) ~= 7 then
        local x, _, z = e:pos()
        local px, _, pz = e:parent_pos()
        wait = e:parent_var(KIND) == 1 and e:hypot(x - px, z - pz) <= 0x9C4
    end
    if not wait then
        e:set_routine(2, 2)
        e:clip(0xE, 0, 3)
    end
    e:joint(0x400)
    footstep(e)
end

-- `Routine_1` 6 on a wall (`0x470d50`): a lunge across it that bites the
-- player in reach, to the wall's end, then the climb (4).
local function wall_lunge(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        local draw = e:rnd()
        e:set_speed(300, 0, 0)
        e:clip(4, 0, 3)
        e:set_timer(0, (draw & 0xF) + 0x1E)
        sub = 1
    end
    if sub == 1 then
        if e:joint(0x400) then
            e:set_routine(2, 2)
            e:clip(5, 0, 3)
        end
    elseif sub == 2 then
        move_by(e, 0, 0, 0)
        e:joint(0x400)
        if near_end(e, e:var(WALL_DIR)) or e:at_em_no() ~= 0xFF or e:sca_info() & 1 ~= 0 then
            e:set_routine(2, 3)
            e:clip(7, 0, 3)
        end
        if bite(e, 2000, 0x10CC) then
            e:set_routine(2, 3)
            e:clip(7, 0, 3)
        end
    elseif sub == 3 then
        if e:joint(0x400) then
            e:routines(1, 4, 0, 0)
            e:set_var(NEXT, 0)
        end
    end
    footstep(e)
end

-- `Routine_1` 7 on a wall (`0x470eb0`): a turn toward the player, then the
-- spit, then the climb (4). `Routine_2` 3 (`FUN_0046fbb0`) isn't entered.
local function wall_spit(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        e:set_timer(0, 0x10)
        e:clip(1, 0, 3)
        sub = 1
    end
    if sub == 1 then
        local turn = toward_player(e, 0x40)
        shift(e, turn)
        local fire = turn == 0
        if not fire then
            local left = (e:timer(0) - 1) & 0xFFFF
            e:set_timer(0, left)
            fire = left == 0
        end
        if fire then
            e:set_routine(2, 2)
            e:clip(0xB, 0, 3)
            local row = ((distance(e) < 3000 and 1 or 0) - 0x78) & 0xFF
            spit_out(e, row, wall_aim(e))
        end
        e:joint(0x400)
        e:seq_sound()
        return
    end
    if sub == 2 and e:joint(0x400) then
        e:routines(1, 4, 0, 0)
        e:set_var(NEXT, 0)
    end
    e:seq_sound()
end

-- `funcs_46F38E[1]` (`FUN_0046fe10`), by `Routine_1`.
local CEILING <const> = {
    [0] = ceiling_rest,
    [1] = ceiling_walk,
    [2] = ceiling_walk,
    [3] = ceiling_walk,
    [4] = turn,
    [5] = aim,
    [6] = aim,
    [7] = ceiling_lunge,
    [8] = spit,
    [9] = leap,
    [10] = to_wall,
    [11] = avoid,
}

-- `funcs_46F38E[2]` (`FUN_00470580`), the wall a spider came down onto from
-- the ceiling.
KIND_2 = {
    [0] = wall_arrive,
    [1] = descend,
    [2] = hang,
    [3] = crawl,
    [4] = climb,
    [5] = to_ceiling,
    [6] = wall_lunge,
    [7] = wall_spit,
}

-- `funcs_46F38E[3]` (`FUN_00471020`), the wall a spider starts on.
local WALL <const> = {
    [0] = wall_start,
    [1] = descend,
    [2] = watch,
    [3] = crawl,
    [4] = climb,
    [5] = to_ceiling,
    [6] = wall_lunge,
    [7] = wall_spit,
}

local STATES <const> = { [0] = FLOOR, CEILING, KIND_2, WALL }

-- `FUN_004735a0`: `Be_flg`'s height bits for the kind, on a wall by how far
-- the player is below.
local function heights(e)
    local flags = e:be_flg() & 0xE3FFFFFF
    local kind = e:var(KIND)
    if kind == 0 then
        flags = flags | 0xC000000
    elseif kind == 1 then
        flags = flags | 0x10000000
    elseif kind == 2 or kind == 3 then
        local _, y = e:pos()
        local _, py = e:player_pos()
        local below = py - y
        if below < 0x834 then
            flags = flags | 0x1C000000
        elseif below < 0x1069 then
            flags = flags | 0x18000000
        else
            flags = flags | 0x10000000
        end
    end
    e:set_be_flg(flags)
end

-- `FUN_0046f370`.
local function move(e)
    -- `FUN_004733d0`: the partner (`fg_status`'s `0x10000000`) isn't ported.
    e:set_var(TO_PARTNER, 0)
    local states = STATES[e:var(KIND)]
    local state = states and states[e:routine(1)]
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

-- `DAT_00528800`'s rows that the hits and deaths use: `Esp_call`'s kind, row
-- and size, and whether the kind's byte has the top bit (4, 5 and 7: the
-- flames' and the blasts'), which places it on the part's matrix instead.
local BLOOD_ROWS <const> = {
    [0] = { 0, 0x19, 0x2000 },
    { 0, 0x19, 0x2800 },
    { 9, 0x0A, 0x0800 },
    { 0, 0x09, 0x1000 },
    { 5, 0x03, 0x1400, true },
    { 5, 0x04, 0x1000, true },
    [7] = { 4, 0x0F, 0x1400, true },
}

-- `FUN_00472df0`: blood at a part, where `Joint_trans` left it. The way is
-- `Direction_ck` from the body to itself. A row with the top bit is called
-- on the part's `Workm`, 100 above it. Otherwise on the floor nothing is
-- added to the part's place, on the ceiling 800 below it and on a wall 400
-- that way.
local function blood(e, part, row)
    local effect = BLOOD_ROWS[row]
    if effect == nil then
        return
    end
    local x, _, z = e:pos()
    local dir = e:direction(x, z, x, z)
    if effect[4] then
        e:effect(effect[1], effect[2], effect[3], dir, part, 0, -100, 0)
        return
    end
    local px, py, pz = e:own_part(part)
    local kind = e:var(KIND)
    if kind == 1 then
        py = py + 800
    elseif kind == 2 or kind == 3 then
        local dx, dz = e:rotate(dir, 400, 0)
        px = px + dx
        pz = pz + dz
    end
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

-- `FUN_004732a0`: the young spiders (`Em_26`) a hit or a death sends out,
-- `(Rnd & mask) + base` of them, from its place, while fewer than 18 are in
-- play. `Set_enemy_work` takes back a dead one or copies the last, and finds
-- none in a room that places none. Each is `kind`, with bit 0 in water, and
-- its routine runs at once: its init's `Rnd` is drawn here, after its facing's.
local function send_young(e, kind, mask, base)
    local count = (e:rnd() & mask) + base
    if e:var(HIT_BY) == 0xE then
        return
    end
    local others = e:count_of(0x26)
    if count + others >= 0x12 then
        count = (0x11 - others) & 0xFF
    end
    if e:var(WATER) & 0xFFFF ~= 0 then
        kind = kind | 1
    end
    local x, y, z = e:pos()
    for _ = 1, count do
        if e:spawn(0x26, kind, {}) == nil then
            return
        end
        local angle
        if e:var(KIND) < 2 then
            angle = e:rnd() << 4
        else
            angle = (((e:var(FACING) & 0xFF) << 8) | e:rnd()) * 4 - 0x200
        end
        local drawn = e:rnd() | 0x100
        -- `Em_26`'s `DRAWN`.
        e:place_spawn(x, y, z, angle & 0xFFFF, { [6] = drawn })
    end
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

-- A hit between a wall and the ceiling doesn't stop it: the state it was in
-- carries on at once.
local function carry_on(e)
    e:routines(1, e:var(LAST), e:var(LAST + 1), e:var(LAST + 2))
end

-- `FUN_00471170` on the ceiling (`0x471240`): the floor's flinch.
local function ceiling_flinch(e)
    if e:var(BETWEEN) == 0 then
        flinch(e)
        return
    end
    carry_on(e)
    if e:var(HIT_BY) ~= 1 then
        blood(e, 0, 0)
    end
    move(e)
end

-- `FUN_00471170` on a wall (`0x4712c0`): the climb, with the turn of 0x40.
local function wall_flinch(e)
    if e:var(HIT_BY) ~= 1 then
        blood(e, 1, 0)
    end
    if e:var(BETWEEN) == 0 then
        e:set_var(NEXT, 1)
        e:routines(1, 4, 0, 0)
        return
    end
    carry_on(e)
    move(e)
end

-- `FUN_00471170`, by kind.
local FLINCHES <const> = { [0] = flinch, ceiling_flinch, wall_flinch, wall_flinch }
local function flinch_by_kind(e)
    FLINCHES[e:var(KIND)](e)
end

-- `FUN_00471840`: weapon 4's hit bleeds from the body and a leg, and takes
-- a leg off once a region's `D_life` has run out. Its rows are the bytes of
-- `DWORD_00528724`'s first cell, which the deaths' table never runs.
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
    flinch_by_kind(e)
end

-- `0x471450`: the fall from the ceiling begins, with a leg off.
local function fall_start(e)
    e:set_routine(2, 1)
    e:set_speed(0, 0x28, 0)
    e:set_timer(0, 0x800)
    e:clip(1, 0, 3)
    e:set_var(SCE, 1)
    e:set_var(ADD_DIR, 0)
    e:set_be_flg(e:be_flg() & 0xE3FFFFFF)
    e:set_damage_cnt(e:damage_cnt() | 0x80)
    lose_leg(e)
    send_young(e, 0x2004, 1, 1)
end

-- The landing's sound: 6 into water, 5 onto the floor.
local function landing_sound(e)
    e:sound(e:var(FLOAT) >= 0x80 and 6 or 5)
end

-- `0x4714d0`: down to `Ground`, turning over on the way.
local function fall(e)
    add_speed_xyz(e, 0, e:timer(0))
    local x, y, z = e:speed()
    e:set_speed(x, y + 0x28, z)
    if e:timer(0) ~= 0 then
        local cx, cz = e:cdir()
        e:set_cdir(cx, cz + 0x200)
        e:set_timer(0, e:timer(0) - 0x200)
    end
    local ground = i16(e:var(GROUND))
    local _, at = e:pos()
    if ground <= at then
        e:set_pos_y(ground)
        e:set_be_flg((e:be_flg() & 0xEFFFFFFF) | 0xC000000)
        if e:timer(0) == 0 then
            e:set_routine(2, 2)
            e:clip(9, 0, 3)
        end
        landing_sound(e)
    end
    e:joint(0x400)
end

-- `0x471580`: up on the floor, it carries on there (kind 0).
local function fall_end(e)
    if e:joint(0x400) then
        e:set_var(SCE, 0)
        e:set_var(KIND, 0)
        e:set_be_flg(e:be_flg() | 0xC000000)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:routines(1, 0, 0, 0)
    end
end

-- `0x4716c0`: the fall from a wall begins, away from it.
local function wall_fall_start(e)
    fall_start(e)
    e:joint(0x400)
    e:set_speed(0x1E0, 0, 0)
    local facing = e:var(FACING)
    if facing & 1 == 0 then
        e:set_timer(0, (-e:cdir()) & 0xFFFF)
        e:set_timer(1, 0)
    else
        e:set_timer(0, 0)
        e:set_timer(1, (facing * 0x400 - e:angle()) & 0xFFFF)
    end
    e:set_be_flg(e:be_flg() & 0xE3FFFFFF)
end

-- The way it faces once it's down off a wall.
local function off_wall(e)
    local facing = e:var(FACING)
    e:set_angle((-(facing * 0x400 * (facing & 1) + e:var(WALL_DIR))) & 0xFFFF)
    e:set_cdir(0, 0)
end

-- `0x471760`: off the wall to `Ground`.
local function wall_fall(e)
    move_by(e, i16(e:timer(0)), i16(e:timer(1)), 0)
    local x, y, z = e:speed()
    e:set_speed(x + 0x28, y, z)
    local ground = i16(e:var(GROUND))
    local _, at = e:pos()
    if ground <= at + 400 then
        e:set_pos_y(ground)
        off_wall(e)
        e:set_be_flg((e:be_flg() & 0xEFFFFFFF) | 0xC000000)
        e:set_routine(2, 2)
        e:clip(9, 0, 3)
        e:joint(0x400)
        -- `DAT_00528518`.
        e:set_body_box(-0x578, 0x3E8, 0x578, 0x320)
        e:set_var(T225, 0)
        landing_sound(e)
    end
end

-- `funcs_47142C` and `funcs_47167C`, by `Routine_2`.
local FALLS <const> = { [0] = fall_start, fall, fall_end }
local WALL_FALLS <const> = { [0] = wall_fall_start, wall_fall, fall_end }

-- `FUN_004715d0`: an odd `Type` on the ceiling doesn't fall.
local function ceiling_flinch_hard(e)
    if e:var(BETWEEN) ~= 0 then
        lose_leg(e)
        carry_on(e)
        move(e)
        return
    end
    if e:kind() & 1 == 0 then
        FALLS[e:routine(2)](e)
        return
    end
    if e:routine(2) == 0 then
        send_young(e, 0x2004, 1, 1)
    end
    flinch_hard(e)
end

-- `0x471610`: an odd `Type` on a wall climbs instead.
local function wall_flinch_hard(e)
    if e:var(BETWEEN) ~= 0 then
        lose_leg(e)
        carry_on(e)
        move(e)
        return
    end
    if e:kind() & 1 == 0 then
        WALL_FALLS[e:routine(2)](e)
        return
    end
    lose_leg(e)
    send_young(e, 0x2004, 1, 1)
    e:set_var(NEXT, 1)
    e:routines(1, 4, 0, 0)
end

-- `FUN_00471340`, by kind.
local HARD <const> = { [0] = flinch_hard, ceiling_flinch_hard, wall_flinch_hard, wall_flinch_hard }
local function flinch_hard_by_kind(e)
    HARD[e:var(KIND)](e)
end

-- Whether a leg's part (3 to 18) is still on the body for the flames'
-- draws: its `Be_flg` has bit 0 and none of `0x4a`, which `FUN_004731a0`
-- sets on a leg it takes off (and `Parts_bomb` clears to 0 once it's down).
local function leg_part_on(e, part)
    if e:hidden_parts() & (1 << part) ~= 0 then
        return false
    end
    return e:var(LOST_LEGS) & (1 << ((part - 3) >> 1)) == 0
end

-- The flames' draws of `FUN_00471b10` and `FUN_00472580`: `count` legs'
-- parts by `(Rnd & 0xF) + 3`, each still on the body bleeding `rows`.
local function burn_legs(e, count, rows)
    for _ = 1, count do
        local part = (e:rnd() & 0xF) + 3
        if leg_part_on(e, part) then
            for _, row in ipairs(rows) do
                blood(e, part, row)
            end
        end
    end
end

-- `FUN_00471b10`: the flamethrower's hit doesn't stop it. Every 30 ticks
-- (`+0x23d`, which `update` counts down) five of its legs' parts burn
-- (row 5); then `Routine_0` goes back to 1 with `+0x233` to `+0x235`, and
-- the move runs this tick.
local function hurt_flame(e)
    if e:var(T23D) == 0 then
        burn_legs(e, 5, { 5 })
        e:set_var(T23D, 0x1E)
    end
    e:routines(1, e:var(LAST), e:var(LAST + 1), e:var(LAST + 2))
    move(e)
end

-- `DWORD_00528694`, by weapon: only the weapons arklay fires.
local HURTS <const> = {
    [2] = flinch_by_kind,
    [3] = flinch_by_kind,
    [4] = flinch_bleeding,
    [5] = flinch_hard_by_kind,
    [6] = flinch_hard_by_kind,
    [7] = flinch_hard_by_kind,
    [8] = flinch_hard_by_kind,
    [0xD] = flinch_by_kind, -- `0x5286c8`: the Colt's, the handguns'.
    [0xF] = flinch_bleeding, -- `0x5286d0`: the SMG's, weapon 4's `FUN_00471840`.
    [0x10] = hurt_flame, -- `0x5286d4`: the flamethrower's `FUN_00471b10`.
    [0x13] = flinch_by_kind, -- `0x5286e0`: the Beretta's, the table's last.
}

-- `FUN_00471110`: `Routine_0` 2.
local function hurt(e)
    if e:routine(2) == 0 then
        e:set_var(HIT_BY, e:routine(1))
    end
    find_water(e)
    local entry = HURTS[e:routine(1)]
    if entry then
        entry(e)
    end
end

-- A death by weapon 9, 10 or 0x10 (the flamethrower) that ends in water
-- puts its flames out there (row 7, on part 0): `HIT_BY > 8` and below 0xB,
-- or 0x10.
local function flames_out(e)
    local weapon = e:var(HIT_BY)
    if weapon == 9 or weapon == 0xA or weapon == 0x10 then
        blood(e, 0, 7)
    end
end

-- The end of a fall into water: the body floats (`FUN_004728b0`'s 3).
local function lands_in_water(e)
    if e:var(WATER) == 0 then
        return
    end
    flames_out(e)
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
    send_young(e, 0x2002, 3, 1)
    blood(e, 0, 0)
    e:routines(7, 0, 0, 0)
    e:set_be_flg(e:be_flg() | 0x42)
    if e:var(FLOAT) >= 0x80 then
        e:set_pos_y(i16(e:var(WATER)) + 0x46)
    end
end

-- `FUN_004734c0`: whether it has come down `dy` short of the water (-1),
-- which it floats on, or of `Ground` (1).
local function land(e, dy, on_ground, on_water)
    local water = i16(e:var(WATER))
    local ground = i16(e:var(GROUND))
    local _, y = e:pos()
    if water < ground then
        if water <= y + dy then
            e:set_var(FLOAT, 0xFF)
            e:set_pos_y(water + on_water)
            return -1
        end
    elseif ground <= y + dy then
        e:set_var(FLOAT, 1)
        e:set_pos_y(ground + on_ground)
        return 1
    end
    return 0
end

-- `WORD_00528790`: where the ripples of a fall into water start, from part 0.
local RIPPLES <const> = {
    { 0, 0 }, { 250, 500 }, { 250, -500 }, { 500, 1000 },
    { 500, -1000 }, { 750, 500 }, { 750, -500 }, { 1000, 0 },
}

-- `FUN_00471e50`: a fall has landed. It's no longer between a wall and the
-- ceiling, and in water it makes ripples and sinks (`FUN_004728b0`'s 1).
local function splash(e)
    if e:var(BETWEEN) ~= 0 then
        e:set_room_enemy(e:room_enemy() & 0xDF)
    end
    local float = e:var(FLOAT)
    if float == 0 or float >= 0x80 then
        local px, _, pz = e:own_part(0)
        local water = i16(e:var(WATER))
        for _, ripple in ipairs(RIPPLES) do
            e:call_effect(0x1A, 0, 0x2000, i16(e:angle()), px + ripple[1], water, pz + ripple[2])
        end
        e:set_var(BOB, 1)
        e:set_var(F23F, 1)
    end
end

-- `0x471ef0`: the end of a fall's death, and its body takes one more shot
-- with a life of 1.
local function fallen(e)
    e:joint(0x400)
    if e:now_seq() & 0x80000 == 0 then
        return
    end
    e:routines(7, 0, 0, 0)
    e:set_be_flg(e:be_flg() & 0xE3FFFFFF)
    if e:var(T239) == 0 then
        e:set_life(1)
        e:set_var(REVIVED, 1)
    end
end

-- `0x471d20`: the death on the ceiling drops it, on its back, to the floor.
local function ceiling_die(e)
    local sub = e:routine(2)
    if sub == 1 then
        e:joint(0x200)
        add_speed_xyz(e, 0, 0)
        local x, y, z = e:speed()
        e:set_speed(x, y + 0x28, z)
        if land(e, 0, 0, 0) ~= 0 then
            e:set_routine(2, 2)
            e:set_be_flg((e:be_flg() & 0xE7FFFFFF) | 0x4000000)
            e:set_var(KIND, 0)
            e:set_be_flg(e:be_flg() | 0x42)
            landing_sound(e)
            splash(e)
            if e:var(FLOAT) >= 0x80 then
                flames_out(e)
            end
        end
        return
    end
    if sub == 2 then
        fallen(e)
        return
    end
    local _, y = e:pos()
    e:set_pos_y(y + 800)
    e:set_routine(2, 1)
    e:set_speed(0, 0x28, 0)
    local cx = e:cdir()
    e:set_cdir(cx, 0)
    e:clip(0xC, 9, 0)
    if e:var(HIT_BY) ~= 1 then
        blood(e, 0, 0)
    end
end

-- `0x471f70`'s start: away from the wall.
local function wall_die_start(e)
    e:set_routine(2, 1)
    e:set_timer(0, 0x800)
    e:clip(1, 9, 0)
    e:joint(0x400)
    e:set_var(SCE, 1)
    e:set_var(ADD_DIR, 0)
    e:set_speed(0x1E0, 0, 0)
    local facing = e:var(FACING)
    if facing & 1 == 0 then
        e:set_timer(1, 0)
        e:set_timer(0, (-e:cdir()) & 0xFFFF)
    else
        e:set_timer(0, 0)
        e:set_timer(1, (facing * 0x400 - e:angle()) & 0xFFFF)
    end
end

-- `0x472020`: off the wall to the floor, on its back.
local function wall_drop(e)
    move_by(e, i16(e:timer(0)), i16(e:timer(1)), 0)
    local x, y, z = e:speed()
    e:set_speed(x + 0x28, y, z)
    if land(e, 400, 0, 0) == 0 then
        return
    end
    off_wall(e)
    e:set_routine(2, 2)
    e:set_var(KIND, 0)
    e:set_be_flg(e:be_flg() | 0x42)
    e:clip(0xC, 9, 0)
    e:joint(0x200)
    -- `DAT_00528518`.
    e:set_body_box(-0x578, 0x3E8, 0x578, 0x320)
    e:set_var(T225, 0)
    landing_sound(e)
    splash(e)
    if e:var(FLOAT) >= 0x80 then
        flames_out(e)
    end
end

-- `0x471f50`: the death on a wall.
local function wall_die(e)
    local sub = e:routine(2)
    if sub == 0 then
        wall_die_start(e)
        if e:var(HIT_BY) ~= 1 then
            blood(e, 0, 0)
        end
    elseif sub == 2 then
        fallen(e)
    else
        wall_drop(e)
    end
end

-- `FUN_00471c10`: the handguns' death. On the floor (`0x471c40`) it falls on
-- its back (clip 12), and its body takes one more shot with a life of 1.
local function die_plain(e)
    if e:var(REVIVED) ~= 0 then
        die_again(e)
        return
    end
    local kind = e:var(KIND)
    if kind == 1 then
        ceiling_die(e)
        return
    elseif kind ~= 0 then
        wall_die(e)
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
        send_young(e, 0x2002, 3, 6)
        blood(e, 0x13, 1)
        lose_body(e)
        e:set_be_flg(e:be_flg() | 0x42)
        e:routines(7, 0, 0, 0)
        if e:var(FLOAT) >= 0x80 then
            e:set_pos_y(i16(e:var(WATER)) + 0x46)
        end
        return
    end
    local kind = e:var(KIND)
    if kind == 1 then
        -- `0x4722a0`.
        if e:routine(2) == 0 then
            blood(e, 0x13, 1)
            lose_body(e)
            send_young(e, 0x2004, 3, 6)
        end
        ceiling_die(e)
        return
    elseif kind ~= 0 then
        -- `0x4722f0`.
        local sub = e:routine(2)
        if sub == 0 then
            wall_die_start(e)
            lose_body(e)
            blood(e, 0x13, 1)
            send_young(e, 0x2004, 3, 6)
        elseif sub == 2 then
            fallen(e)
        else
            wall_drop(e)
        end
        return
    end
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_be_flg(e:be_flg() | 2)
        e:set_var(FLOAT, 1)
        e:clip(0xC, 0, 7)
        blood(e, 0x13, 1)
        lose_body(e)
        send_young(e, 0x2002, 3, 6)
    end
    e:joint(0x200)
    if e:now_seq() & 0x40000 == 0 then
        return
    end
    lands_in_water(e)
    e:routines(7, 0, 0, 0)
end

-- `FUN_00472580`: weapon 0xA's and the flamethrower's death. On its first
-- tick, unless its body is being shot again, it burns: row 4 (5 in water) on
-- the body, part 0, then four draws of its legs' parts, each still on it
-- taking that row and row 5. Then it dies as the handguns' do. Its
-- `FUN_00473490(e, 0x202f2f)`, the 20 parts' `Poly_rgb` charred, isn't
-- ported (the script has no way to set them).
local function die_flame(e)
    if e:routine(2) == 0 and e:var(REVIVED) == 0 then
        local row = e:var(WATER) & 0xFFFF ~= 0 and 5 or 4
        blood(e, 0, row)
        burn_legs(e, 4, { row, 5 })
    end
    die_plain(e)
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
    [0xD] = die_plain, -- `0x528758`: the Colt's, the handguns'.
    [0xF] = die_plain, -- `0x528760`: the SMG's, the handguns'.
    [0x10] = die_flame, -- `0x528764`: the flamethrower's, weapon 0xA's.
    [0x13] = die_plain, -- `0x528770`: the Beretta's, the table's last.
}

-- `BYTE_005287f8`: the body's bob in the water, by `Timer1 >> 3 & 7`.
local BOBS <const> = { [0] = 0, 8, 16, 32, 32, 16, 8, 0 }

-- `FUN_004728b0`: a body that fell into water sinks and comes back up
-- (`+0x23e`'s 1 and 2), then bobs there (3). Out of water it lies on
-- `Ground`.
local function bob(e)
    local state = e:var(BOB)
    if state == 0 then
        return
    end
    local water = i16(e:var(WATER))
    if water == 0 then
        e:set_var(FLOAT, 1)
        e:set_var(F23F, 0)
        e:set_pos_y(i16(e:var(GROUND)))
        return
    end
    if state == 3 then
        local timer = e:timer(1)
        e:set_timer(1, (timer + 1) & 0xFFFF)
        e:set_pos_y(BOBS[(timer >> 3) & 7] + 0x46 + water)
        return
    end
    if state == 1 then
        e:set_var(BOB, 2)
        local x, _, z = e:speed()
        e:set_speed(x, 0x8C, z)
        e:set_var(RISE, 0)
    elseif state ~= 2 then
        return
    end
    local x, sink, z = e:speed()
    local rise = i16(e:var(RISE) + sink)
    e:set_var(RISE, rise)
    sink = sink - 0xF
    local y = rise + 0x46 + water
    e:set_pos_y(y)
    if sink < 1 then
        if sink < -0x46 then
            sink = -0x46
        end
        if y <= water then
            e:set_pos_y(water)
            e:set_var(BOB, 3)
        end
    elseif i16(e:var(GROUND)) <= y then
        sink = 0
        e:set_pos_y(i16(e:var(GROUND)))
    end
    e:set_speed(x, sink, z)
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
