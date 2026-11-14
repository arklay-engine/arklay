-- The G embryos, `Em_27` (entity id 0x27): the small crawlers of rooms 303
-- and 309 that the G adult (`Em_28`, slot 0 there) carries and spits out.
--
-- `Em_27` (0x474210) runs `Root_ck` toward the player, then `Routine_0` from
-- `funcs_47425D` (0x5288f8): 0 is the init (`FUN_004742e0`), 1 the move
-- (`FUN_00474660`), 3 the death (`FUN_004767a0`) and 7 the fade (0x476af0);
-- 2 and 4 are `nullsub_1` (hits go through `em_damage_table`). The move runs
-- `Routine_1`'s pre from 0x528918 (only 1 has one, `FUN_00474690`), reads
-- `Routine_1` again, and runs its state from 0x528938: 0 nothing (the room's
-- template, `Type` 0), 1 the crawl (`FUN_004746d0`), 2 the jump
-- (`FUN_00474a50`), 3 the ride on the G adult (`FUN_00474b80`), 4 the latch
-- onto the player (`FUN_00475280`), 5 the slide (`FUN_00475da0`), 6 the way
-- out of the G adult (`FUN_00476060`), and 7, read past the table's end from
-- the death's jump table, the rest (0x4764e0). After the routine `Em_27`
-- shifts a six-deep history of `Cdir_y`, whose oldest the crawl walks by, and
-- runs `Em_seq_sound`; the engine then runs `Oba_set_ofs` (no `Oba_ck_em`)
-- and `Sca_ck_em` with `0x400`, and finds `Ground` under the body before
-- `update`.
--
-- The player's side of the latch is `em_damage_table[0x27]` (0x476c60,
-- `player2::em27_held`), which the init points at; `em_die_table`'s 0x476ed0
-- isn't reached.
--
-- `Type` 2, the embryos the G adult (`Em_28`) spits, take state 6 (`FUN_00476060`).
-- Not ported: `Type` 1 and state 3 (it stands hidden), the
-- shot's hits,
-- `Routine_0` 3's `Routine_1` 0x13, and the parts'
-- squash and colours as drawn (only their counts are kept, in vars).

local HISTORY <const> = 0 -- 0 to 5: `+0x218, +0x21a, +0x21c, +0x21e, +0x224, +0x226`.
local RGB <const> = 6 -- The parts' `Poly_rgb` (all ten alike).
local SIZE_Y <const> = 7 -- The parts' `Size_y` (all ten alike), from the squash on.
local GROUND <const> = 30 -- `Ground`, which the engine finds under it before the tick.
local AROUND <const> = 8 -- `+0x228`: the latch's turn about the player, from their facing.
local STRUGGLE <const> = 9 -- `+0x22a`: what the player's presses have left to shake it off.
local DIE_AFTER <const> = 10 -- `+0x22c`'s bit 0: its life ran out on the bite, so it dies once off.
local SIZE <const> = 11 -- `Size`, from the way in on.

-- `abStack_c`: the latch's sounds, by `Rnd & 3`.
local CHEW_SOUNDS <const> = { [0] = 0, 8, 10, 11 }

-- The parts' `Poly_rgb` as the model loads.
local RGB_LOADED <const> = 0x808080

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

local function speed_x(e)
    local sx = e:speed()
    return sx
end

local function set_speed_x(e, value)
    local _, sy, sz = e:speed()
    e:set_speed(i16(value), sy, sz)
end

local function goto_player(e, rate)
    local px, _, pz = e:player_pos()
    e:goto_point(px, pz, rate)
end

local function player_turn(e, rate)
    local px, _, pz = e:player_pos()
    return e:goto_ck(px, pz, rate)
end

-- `FUN_004742e0`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    e:set_life((e:rnd() & 1) + 1)
    e:set_kind(e:kind() | 0x2000)
    e:set_atd(0, 0, -200, 200, 200, 200)
    local _, sy, sz = e:speed()
    e:set_speed(0x50, sy, sz)
    e:set_be_flg(e:be_flg() | 0x4400002)
    e:clip(0, 0, 0)
    e:joint(0x100)
    -- `Kage_work_set`'s half-size is 400 by 300; arklay's is square.
    e:set_shadow(400)
    e:set_var(RGB, RGB_LOADED)
    local low = e:kind() & 0xFFF
    if low == 0 or low == 1 or low == 2 then
        -- The template, and the two that ride on the G adult (not ported:
        -- they stand hidden).
        if low == 1 then
            e:set_routine(1, 3)
        elseif low == 2 then
            e:set_routine(1, 6)
        end
        if low ~= 2 then
            e:set_kind(e:kind() | 0x4000)
        end
        e:set_be_flg(e:be_flg() & ~0x4000000)
        e:set_hidden_parts(0x3FF)
        e:set_shadow(0)
        if low == 2 then
            -- On the G adult's part 4, a point 0x32 above it.
            local ax, ay, az = e:parent_apply(4, 0, 0x32, 0)
            local px, py, pz = e:parent_part(4)
            e:set_pos(ax + px, ay + py, az + pz)
            e:set_var(SIZE, 0x800)
            e:set_damage_cnt(0x80)
            e:set_be_flg(e:be_flg() | 0x800)
        end
    elseif low == 3 then
        e:set_routine(1, 1)
        e:set_life((e:rnd() & 1) + 2)
    end
end

-- The top of states 1, 5 and 7: a player who steps on it within 400 squashes
-- it.
local function tread_check(e)
    if not (e:distance() < 400 and e:noise() & 6 ~= 0) then
        return false
    end
    e:routines(3, 1, 1, 0)
    e:set_damage_no(0)
    set_speed_x(e, 0)
    local x, y, z = e:pos()
    e:call_effect(0, 8, 0x1FA0, e:rnd() << 4, x, y, z)
    e:call_effect(0, 8, 0x17D0, e:rnd() << 4, x, y, z)
    e:call_effect(0, 8, 0x17D0, e:rnd() << 4, x, y, z)
    local draw = e:rnd()
    e:set_timer(0, 0)
    e:clip(9, draw & 7, 0)
    return true
end

-- `FUN_00474690`: the jump, this tick, at a player it faces within 3000.
local function pre_crawl(e)
    local turn = player_turn(e, 0x80)
    if e:distance() < 3000 and turn == 0 then
        e:routines(1, 2, 0, 0)
    end
end

-- `FUN_004746d0`: the crawl, along the facing of six ticks ago, toward `Dest`.
local function crawl(e)
    if tread_check(e) then
        return
    end
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0, 0, 0xF)
        e:set_routine(2, 1)
        e:set_speed(speed_x(e), 0, 0)
        e:set_timer(1, 0)
        e:set_timer(2, 0)
        e:set_timer(0, (e:rnd() & 0x1F) + 0x14)
    elseif sub ~= 1 then
        return
    end
    local sx = i16(speed_x(e) - 10)
    if sx < 0x1E then
        sx = 0x1E
    end
    if sx > 0x78 then
        sx = sx - 0x1E
    end
    local left = e:timer(2)
    e:set_timer(2, (left - 1) & 0xFF)
    if left == 0 then
        local draw = (e:rnd() & 0xF) + 10
        e:set_timer(2, draw)
        e:set_timer(3, draw // 2)
    end
    if i8(e:timer(3)) < i8(e:timer(2)) then
        sx = sx + i8(e:timer(2)) * 3
    else
        sx = sx - 0x14
    end
    set_speed_x(e, sx)
    local dx, dz = e:dest()
    local turn = e:goto_ck(dx, dz, 0x10)
    if e:rnd() & 3 == 0 then
        goto_player(e, (e:rnd() & 1) ~= 0 and -0x20 or 0)
    end
    local t1 = i16(e:timer(1))
    if turn > 0 then
        t1 = i16(t1 + 4)
        if t1 < -0x40 then
            t1 = i16(t1 + 8)
        end
        if t1 < -0x80 then
            t1 = i16(t1 + 0x10)
        end
        if t1 < -0x100 then
            t1 = i16(t1 + 0x10)
        end
    end
    if turn < 0 then
        t1 = i16(t1 - 4)
        if t1 > -0x40 then
            t1 = i16(t1 - 8)
        end
        if t1 > 0x80 then
            t1 = i16(t1 - 0x10)
        end
        if t1 > 0x100 then
            t1 = i16(t1 - 0x10)
        end
    end
    e:set_timer(1, t1 & 0xFFFF)
    e:set_angle((e:angle() + t1) & 0xFFFF)
    local mx, _, mz = e:rot_apply(0, e:var(HISTORY), 0, i16(speed_x(e)), 0, 0)
    e:joint(0x100)
    e:add_pos(mx, 0, mz)
    if e:timer(0) ~= 0 then
        e:set_timer(0, e:timer(0) - 1)
    end
    if e:timer(0) == 0 then
        if e:rnd() & 1 ~= 0 then
            e:routines(1, 5, 0, 0)
            if e:rnd() & 1 ~= 0 then
                e:routines(1, 7, 0, 0)
            end
        end
        e:set_timer(0, (e:rnd() & 0x1F) + 0x14)
    end
end

-- `FUN_00474a50`: the jump at the player, which lands on `Ground` or latches
-- on.
local function jump(e)
    local sub = e:routine(2)
    if sub == 0 then
        local sx = i16(speed_x(e) + 200)
        e:clip(2, 0, 3)
        e:set_routine(2, 1)
        if sx > 300 then
            sx = 300
        end
        local _, _, sz = e:speed()
        e:set_speed(sx, -180, sz)
        e:set_timer(0, 0)
        e:set_timer(1, 0)
    elseif sub ~= 1 then
        return
    end
    goto_player(e, 0x28)
    e:joint(0x200)
    e:add_speed_xyz()
    local t = e:timer(0)
    local sx, sy, sz = e:speed()
    sy = i16(sy + t * 15)
    e:set_speed(sx, sy, sz)
    if sy > 0 and e:timer(1) == 0 then
        e:set_timer(1, 1)
        e:clip(3, 0, 7)
    end
    e:set_timer(0, (t + 1) & 0xFFFF)
    local ground = i16(e:var(GROUND))
    local _, y = e:pos()
    if y > ground then
        set_speed_x(e, speed_x(e) - 0x96)
        e:set_pos_y(ground)
        e:routines(1, 1, 0, 0)
        if e:rnd() & 1 ~= 0 then
            e:routines(1, 7, 0, 0)
        end
    end
    if e:distance() < 700 and e:pl_damage_cnt() == 0 then
        e:routines(1, 4, 0, 0)
    end
end

-- `FUN_00476720`: on the circle of `radius` about `x, z`, at `angle`.
local function place_about(e, x, z, radius, angle)
    local dx, _, dz = e:rot_apply(0, i16(angle), 0, radius, 0, 0)
    local _, y = e:pos()
    e:set_pos(x + dx, y, z + dz)
end

-- `FUN_00476720` about the player, at its turn from their facing.
local function place_round_player(e, radius)
    local px, _, pz = e:player_pos()
    place_about(e, px, pz, radius, e:var(AROUND) + e:player_angle())
end

-- `FUN_00476720` about the player's part 8 (the head), as `+0x5bc` and
-- `+0x5c4` hold it.
local function place_round_head(e, radius)
    local hx, _, hz = e:player_part_point(8, 0, 0, 0)
    if hx == nil then
        hx, _, hz = e:player_pos()
    end
    place_about(e, hx, hz, radius, e:var(AROUND) + e:player_angle())
end

-- The `Workm.t.y` of the player's part `part`, as the last tick left it.
local function player_part_y(e, part)
    local _, y = e:player_part_point(part, 0, 0, 0)
    if y == nil then
        _, y = e:player_pos()
    end
    return y
end

-- `|value|` as the latch compares it: an `int`'s, as a `ushort`.
local function abs16(value)
    if value < 0 then
        value = -value
    end
    return value & 0xFFFF
end

-- Blood (`Esp_call` row `row`) at its part 3, `(100, 200, 0)` on: a `Rnd`
-- for the way, one for the size.
local function blood(e, row, base)
    local dir = i16(e:angle() - e:rnd() * 4 + 0xA00)
    local size = e:rnd() * 2 + base
    e:effect(0, row, size, dir, 3, 100, 200, 0)
end

-- The end of states 3, 5 and 6 (`LAB_0047568d`): the player's presses
-- (`FUN_00475d70`: 1 for a press, 3 more for `0x989eee`'s 2) shake it off.
local function struggle(e)
    local presses = (e:gacha() and 1 or 0) + ((e:noise() & 2) ~= 0 and 3 or 0)
    local left = i16(e:var(STRUGGLE) - presses * 0x10)
    e:set_var(STRUGGLE, left & 0xFFFF)
    if left < 0 then
        e:set_angle((e:angle() + 0x800) & 0xFFFF)
        e:set_routine(2, 9)
    end
end

-- The player's `Routine_0` to 3 as `[5, 0, sub]` with `Routine_3` kept, and
-- this one's hold and `pSub1` clips on them.
local function take_hold(e, sub)
    e:pl_set_routine(5, 0, sub, e:pl_routine(3))
    e:pl_hold()
    e:pl_set_sub1()
end

-- `FUN_00475280`: the latch onto the player. It climbs to the hips (1),
-- winds round them (3), holds on (5, where the player is caught), climbs to
-- the head (6) and bites (7 and 8), then jumps off (9) and falls to the
-- crawl (10); presses shake it off from 3 to 6. A bite that kills goes in
-- (11 to 13). `fg_system`'s `0x100` (set by that bite) and `Size` and the
-- parts it hides are drawing only.
local function latch(e)
    if e:routine(2) < 9 then
        if e:room_enemy() & 1 ~= 0 or e:pl_damage_cnt() & 0x80 ~= 0 then
            e:set_routine(2, 9)
        end
    end
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(7, 0, 3)
        e:set_routine(2, 1)
        e:set_var(AROUND, (e:direction_from_player() - e:player_angle()) & 0xFFF)
        e:set_timer(0, ((e:rnd() & 1) * -0x80 + 0x40) & 0xFFFF)
        local draw = e:rnd()
        e:set_var(DIE_AFTER, 0)
        e:set_var(STRUGGLE, (draw & 0x3F) + 0x186)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        sub = 1
    end
    if sub == 1 then
        e:joint(0x200)
        place_round_player(e, 300)
        e:set_var(AROUND, (e:var(AROUND) + e:timer(0)) & 0xFFFF)
        goto_player(e, 0x400)
        local _, y = e:pos()
        y = y - 0x96
        e:set_pos_y(y)
        if abs16(player_part_y(e, 0) - y + 300) < 300 then
            e:set_routine(2, 2)
        end
        return
    end
    if sub == 2 then
        e:set_routine(2, 3)
        local draw = e:rnd()
        e:set_timer(1, 4)
        e:set_timer(2, 7)
        e:set_timer(0, (draw & 0x1F) + 0x3C)
        e:set_timer(3, 8)
        sub = 3
    end
    if sub == 3 then
        e:joint(0x200)
        place_round_player(e, 300)
        goto_player(e, 0x400)
        local t = e:timer(0)
        e:set_timer(0, (t - 1) & 0xFFFF)
        if t == 0 then
            e:set_routine(2, 4)
        end
        local step = e:timer(3)
        local _, y = e:pos()
        e:set_pos_y(y - 3)
        local turn = e:timer(1)
        e:set_var(AROUND, (e:var(AROUND) + turn) & 0xFFFF)
        local left = e:timer(2)
        e:set_timer(1, (i8(step) + turn) & 0xFFFF)
        e:set_timer(2, (left - 1) & 0xFF)
        if left == 0 then
            e:set_timer(2, 0xF)
            e:set_timer(3, (-step) & 0xFF)
        end
        struggle(e)
        return
    end
    if sub == 4 then
        local draw = e:rnd()
        e:set_routine(2, 5)
        e:set_timer(0, (draw & 0xF) + 0xF)
        if e:rnd() & 1 ~= 0 then
            e:sound(1)
        end
        e:set_timer(3, 0)
        sub = 5
    end
    if sub == 5 then
        e:joint(0x200)
        local t = e:timer(0)
        e:set_timer(0, (t - 1) & 0xFFFF)
        if t == 0 then
            if e:pl_routine(0) ~= 5 then
                e:pl_set_routine(5, 0, 0, e:pl_routine(3))
            end
            e:pl_hold()
            e:pl_set_sub1()
            e:set_timer(3, 0x84)
        end
        local hold = e:timer(3) & 0xFF
        if hold >= 0x80 then
            hold = (hold - 1) & 0xFF
            e:set_timer(3, hold)
            if hold & 0x7F == 0 then
                e:set_routine(2, 6)
            end
        end
        place_round_player(e, 300)
        goto_player(e, 0x400)
        struggle(e)
        return
    end
    if sub == 6 then
        e:set_timer(1, 0x140)
        local from = (0x340 - e:var(AROUND)) & 0xFFF
        if from < 0x280 then
            e:set_timer(1, 0)
            e:set_var(AROUND, 0x200)
        end
        local turn = e:timer(1)
        local around = e:var(AROUND) - turn
        if from < 0x801 then
            around = around + turn * 2
        end
        e:set_var(AROUND, around & 0xFFFF)
        local _, y = e:pos()
        y = y - 200
        e:set_pos_y(y)
        if abs16(player_part_y(e, 8) - y) < 200 then
            e:set_routine(2, 7)
        end
        place_round_player(e, 300)
        goto_player(e, 0x400)
        struggle(e)
        return
    end
    if sub == 7 then
        e:clip(8, 0, 7)
        e:set_routine(2, 8)
        local draw = e:rnd()
        e:set_life(e:life() - 1)
        e:set_timer(0, (draw & 0xF) + 0x2D)
        if e:life() < 0 then
            e:set_var(DIE_AFTER, 1)
        end
        if e:pl_life_down(10, false) == 2 then
            e:pl_hold_back_die()
            e:set_routine(2, 0xB)
            e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
            return
        end
        sub = 8
    end
    if sub == 8 then
        e:joint(0x200)
        place_round_head(e, 0x15E)
        goto_player(e, 0x400)
        local t = e:timer(0)
        local left = (t - 1) & 0xFFFF
        e:set_timer(0, left)
        if t == 0 then
            e:set_angle((e:angle() + 0x800) & 0xFFFF)
            e:set_routine(2, 9)
        end
        if left & 3 == 0 and i16(left) > 10 then
            blood(e, 1, 0x5F4)
        end
        if left & 7 == 0 then
            e:sound(CHEW_SOUNDS[e:rnd() & 3])
        end
        return
    end
    if sub == 9 then
        e:set_speed(0x78, 100, 0)
        e:set_timer(0, 0)
        e:set_routine(2, 10)
        e:clip(3, 0, 0)
        local draw = e:rnd()
        e:set_angle((e:angle() - draw * 2 + 0x900) & 0xFFFF)
        if e:var(DIE_AFTER) & 1 ~= 0 then
            draw = e:rnd()
            e:set_speed(200, 300, 0)
            e:set_angle((e:player_angle() - draw * 2 - 0x100) & 0xFFFF)
        end
        if e:pl_damage_cnt() & 0x80 == 0 then
            if e:pl_routine(0) == 5 then
                e:pl_set_routine_at(2, 2)
            else
                take_hold(e, 2)
            end
        end
        e:set_room_enemy(e:room_enemy() | 1)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        sub = 10
    end
    if sub == 10 then
        if e:move_cnt() == 1 then
            e:set_room_enemy(e:room_enemy() & 0xFE)
        end
        e:joint(0x200)
        e:add_speed_xyz()
        local t = e:timer(0)
        local sx, sy, sz = e:speed()
        e:set_speed(sx, i16(sy + t * 0xF), sz)
        e:set_timer(0, (t + 1) & 0xFFFF)
        local ground = i16(e:var(GROUND))
        local _, y = e:pos()
        if ground < y then
            e:set_pos_y(ground)
            e:routines(1, 1, 0, 0)
            if e:var(DIE_AFTER) & 1 ~= 0 then
                e:routines(3, 1, 0, 0)
                e:set_damage_no(0)
            end
        end
        return
    end
    if sub == 0xB then
        e:clip(0xF, 0, 0xF)
        e:set_routine(2, 0xC)
        e:set_timer(0, 0x78)
        e:set_timer(1, 0xFA)
        e:set_player_life(-1)
        e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        e:pl_set_routine_at(2, 4)
        local _, y = e:pos()
        e:set_pos_y(y - 100)
        e:hide_part(2)
        e:hide_part(3)
        e:set_var(SIZE, 0x1000)
        e:set_be_flg(e:be_flg() | 0x800)
        sub = 0xC
    end
    if sub == 0xC then
        e:set_var(SIZE, (e:var(SIZE) - 0xF) & 0xFFFF)
        e:joint(0x100)
        place_round_head(e, i16(e:timer(1)))
        goto_player(e, 0x400)
        local t = e:timer(0)
        local left = (t - 1) & 0xFFFF
        e:set_timer(0, left)
        if t == 0 then
            e:set_routine(2, 0xD)
        end
        if left & 3 == 0 then
            blood(e, 1, 0xDC4)
            blood(e, 1, 0x5F4)
            blood(e, 1, 0x5F4)
            if i16(left) < 0x1E then
                blood(e, 3, 0x9DC)
            end
        end
        if left & 7 == 0 then
            e:sound(CHEW_SOUNDS[e:rnd() & 3])
        end
        if left == 0x3C then
            e:pl_set_routine_at(2, 6)
            e:hide_part(1)
            e:sound(4)
        end
        if i16(left) < 0x3C then
            e:set_timer(1, (e:timer(1) - 2) & 0xFFFF)
            local _, y = e:pos()
            e:set_pos_y(y + 5)
        end
        if left == 5 then
            e:effect(0, 1, 0x1F40, i16(e:angle() + 0x800), 3, 100, 200, 0)
            for part = 0, 9 do
                e:hide_part(part)
            end
            e:sound(4)
        end
        return
    end
    if sub == 0xD then
        e:set_routine(2, 0xE)
        e:pl_set_routine(5, 0, 8, 0)
    end
end

-- `FUN_00475da0`: the slide to a stop, a look at the player, then a turn
-- away and back to the crawl.
local function slide(e)
    if tread_check(e) then
        return
    end
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(1, 0, 0xF)
        e:set_routine(2, 1)
        e:set_timer(0, 0)
        sub = 1
    end
    if sub == 1 then
        goto_player(e, 0x100)
        e:joint(0x100)
        e:add_speed_xz()
        local t = e:timer(0)
        set_speed_x(e, speed_x(e) + i16(t * -2))
        e:set_timer(0, (t + 1) & 0xFFFF)
        if speed_x(e) < 0 then
            e:set_routine(2, 2)
            local draw = e:rnd()
            e:set_timer(2, 0)
            e:set_timer(0, (draw & 0x1F) + 0x1E)
        end
        return
    end
    if sub == 2 then
        e:set_timer(2, 0)
        if player_turn(e, 0x200) ~= 0 then
            e:set_timer(2, 1)
        end
        if player_turn(e, 0x20) == 0 then
            e:set_timer(2, 0)
        end
        if e:timer(2) ~= 0 then
            goto_player(e, 0x80)
        end
        e:joint(0x100)
        local t = e:timer(0)
        e:set_timer(0, (t - 1) & 0xFFFF)
        if t == 0 then
            e:set_routine(2, 3)
            if e:rnd() & 1 ~= 0 then
                e:set_routine(1, 1)
                e:set_routine(2, 0)
            end
        end
        return
    end
    if sub == 3 then
        e:clip(0, 0, 0xF)
        e:set_routine(2, 4)
        e:set_timer(1, (e:rnd() + 0x1E) & 0xFFFF)
        sub = 4
    end
    if sub == 4 then
        e:joint(0x100)
        e:add_speed_xz()
        goto_player(e, -8)
        local sx = speed_x(e) + 10
        if sx > 100 then
            sx = 100
            e:routines(1, 1, 0, 0)
        end
        set_speed_x(e, sx)
    end
end

-- `0x4764e0`: the rest, which a player within 2500 cuts short.
local function rest(e)
    if tread_check(e) then
        return
    end
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0, 0, 0xF)
        e:set_routine(2, 1)
        e:set_timer(0, 0)
        sub = 1
    end
    if sub == 1 then
        goto_player(e, 0x40)
        e:joint(0x100)
        e:add_speed_xz()
        local t = e:timer(0)
        set_speed_x(e, speed_x(e) + i16(t * -5))
        e:set_timer(0, (t + 1) & 0xFFFF)
        if speed_x(e) < 0 then
            e:set_routine(2, 2)
            local draw = e:rnd()
            e:set_timer(2, 0)
            e:set_timer(0, (draw & 0x1F) + 0x3C)
        end
    elseif sub == 2 then
        if e:distance() < 0x9C4 then
            e:set_routine(2, 3)
            e:clip(0xD, 0, 7)
            goto_player(e, 0x100)
            return
        end
        e:joint(0x100)
        local t = e:timer(0)
        e:set_timer(0, (t - 1) & 0xFFFF)
        if t == 0 then
            e:routines(1, 1, 0, 0)
            set_speed_x(e, 0x50)
        end
    elseif sub == 3 then
        e:spd()
        if e:joint(0x200) then
            e:routines(1, 1, 0, 0)
            local _, sy = e:speed()
            e:set_speed(100, sy, 0)
            return
        end
        e:add_speed_xz()
    end
end

-- Every part's `Poly_rgb` less `step`. Only its colour, the low three bytes,
-- is kept: the code byte above them doesn't change what the fade reads.
local function rgb_sub(e, step)
    e:set_var(RGB, (e:var(RGB) - step) & 0xFFFFFF)
end

-- `FUN_00476060`: the way out of the G adult (`Type` 2): it rides a point
-- before part 4 of the adult's (`enemy_work[0]`'s) mouth, falls to the
-- ground and takes to the crawl. The parts' colours and their hiding aren't
-- kept, only their count in `RGB`.
local function mouth(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0xC, 0, 0)
        e:set_routine(2, 1)
        e:set_timer(0, 0x1E)
        e:set_timer(2, 0x5A)
        e:set_var(RGB, 0xC0C0C0)
        e:sound(7)
        sub = 1
    end
    if sub == 1 then
        local t0 = i16(e:timer(0))
        local vx, vy = t0, t0 // 5 + 0x3C
        if t0 < 0 and t0 % 5 ~= 0 then
            vy = vy + 1
        end
        local vz = 0x40 - (e:rnd() & 0x7F)
        local ax, ay, az = e:parent_apply(4, vx, vy, vz)
        local px, py, pz = e:parent_part(4)
        e:set_pos(ax + px, ay + py, az + pz)
        e:joint(0x100)
        e:set_timer(0, (t0 + 5) & 0xFFFF)
        local left = e:timer(2)
        e:set_timer(2, (left - 1) & 0xFF)
        if left == 0 then
            e:set_routine(2, 2)
        end
        return
    elseif sub == 2 then
        e:set_timer(0, 0)
        e:set_routine(2, 3)
        e:sound(7)
        sub = 3
    end
    if sub == 3 then
        e:joint(0x100)
        local t0 = i16(e:timer(0))
        local _, y = e:pos()
        y = y + t0
        e:set_timer(0, (t0 + 0x1E) & 0xFFFF)
        e:set_pos_y(y)
        local ground = i16(e:var(GROUND))
        if ground < y then
            e:set_pos_y(ground)
            e:set_routine(2, 4)
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
        end
        return
    end
    if sub == 4 then
        e:clip(0xB, 0, 3)
        e:set_timer(0, (e:rnd() & 0x1F) + 0x78)
        e:set_routine(2, 5)
        e:set_be_flg(e:be_flg() | 0x4000000)
        e:set_timer(1, 1)
        e:sound(5)
        sub = 5
    end
    if sub == 5 then
        local t1 = i16(e:timer(1))
        e:set_timer(1, (-t1) & 0xFFFF)
        local size = i16(e:var(SIZE) + t1 * 0x1E + 0x12)
        if size > 0x1000 then
            size = 0x1000
        end
        e:set_var(SIZE, size)
        local t0 = e:timer(0)
        if t0 == 0x3C then
            e:clip(9, 0, 3)
        end
        if t0 & 0x3F ~= 0 and e:rnd() & 0x1F == 0 then
            e:sound(5)
        end
        local dx = 0x10 - (e:rnd() & 0x1F)
        local dz
        set_speed_x(e, dx)
        e:add_pos(dx, 0, 0)
        dz = 0x10 - (e:rnd() & 0x1F)
        local sx, sy = e:speed()
        e:set_speed(sx, sy, dz)
        e:add_pos(0, 0, dz)
        e:set_angle((e:angle() + 0x80 - e:rnd()) & 0xFFFF)
        if i16(t0) < 0x5A then
            rgb_sub(e, 0x10101)
        end
        e:joint(0x400)
        e:set_timer(0, (t0 - 1) & 0xFFFF)
        if t0 == 0 then
            e:routines(1, 1, 0, 0)
            set_speed_x(e, 0x50)
            e:set_be_flg(e:be_flg() & ~0x800)
        end
        tread_check(e)
    end
end

-- `funcs_528938`, by `Routine_1` (3 isn't ported).
local STATES <const> = { [1] = crawl, [2] = jump, [4] = latch, [5] = slide, [6] = mouth, [7] = rest }

-- `FUN_00474660`: the pre, then the state of `Routine_1` as the pre left it.
local function move(e)
    if e:routine(1) == 1 then
        pre_crawl(e)
    end
    local state = STATES[e:routine(1)]
    if state then
        state(e)
    end
end


-- `0x4767c0`'s `Routine_2` 0: a shot's death (`Routine_1` the weapon), with
-- clip 9 at a frame `Rnd` picks, blood (effect 0, row 8) on part 0's
-- `Workm` and its life at -1. Weapons 9, 10, 0x10 (the flamethrower) and
-- 0x11 burn it (effect 5, row 4), 0xB and 0xE (effect 4, row 0xF), on its
-- own matrix, and either keeps the squash from bleeding (`Timer3`); 0xC
-- stops it. Then the slide, this tick.
local function shot(e)
    local draw = e:rnd()
    e:clip(9, draw & 7, 0xF)
    e:set_routine(2, 1)
    e:set_timer(0, 0)
    e:effect(0, 8, 0x17D0, 0, 0, 0, 0, 0)
    e:set_life(-1)
    e:set_timer(3, 0)
    local weapon = e:routine(1)
    local x, y, z = e:pos()
    if weapon == 9 or weapon == 0xA or weapon == 0x10 or weapon == 0x11 then
        local dir = i16(e:angle())
        e:call_effect(5, 4, e:rnd() * 4 + 0xC18, dir, x, y, z)
        e:set_timer(3, 1)
    end
    if weapon == 0xB or weapon == 0xE then
        local dir = i16(e:angle())
        e:call_effect(4, 0xF, e:rnd() * 4 + 0xC18, dir, x, y, z)
        e:set_timer(3, 1)
    end
    if weapon == 0xC then
        local _, sy = e:speed()
        e:set_speed(0, sy, 0)
    end
    e:sound(3)
end

-- `0x4767c0` from `Routine_2` 1, where a tread starts it: the slide to a
-- stop, the squash, then the fade.
local function squash(e)
    local sub = e:routine(2)
    if sub == 0 then
        shot(e)
        sub = 1
    end
    if sub == 1 then
        e:joint(0x100)
        e:add_speed_xz()
        local t = e:timer(0)
        set_speed_x(e, speed_x(e) + i16(t * -0x14))
        e:set_timer(0, (t + 1) & 0xFFFF)
        if speed_x(e) < 0 then
            e:set_routine(2, 2)
            e:set_timer(0, e:rnd() & 7)
            e:set_var(SIZE_Y, 0x1000)
        end
    elseif sub == 2 then
        local t = e:timer(0)
        if t == 0x1E or t == 0x3C then
            e:set_hokan(7)
            e:set_move_no(e:move_no() + 1)
        end
        e:set_timer(0, (t + 1) & 0xFFFF)
        if (t + 1) & 0x1F == 0 and e:timer(3) == 0 then
            local ox = (e:rnd() >> 1) + 0x40
            local oz = 0x40 - (e:rnd() >> 1)
            e:effect(0, 8, 0x5F4, e:rnd() << 4, 0, ox, 200, oz)
        end
        local size = i16(e:var(SIZE_Y) - 0x28)
        e:set_var(SIZE_Y, size)
        if e:var(RGB) & 0xFF ~= 0x20 then
            rgb_sub(e, 0x10001)
        end
        if size < 0 then
            e:set_routine(2, 3)
        end
        e:joint(0x100)
    elseif sub == 3 then
        e:routines(7, 0, 0, 0)
    end
    local ground = i16(e:var(GROUND))
    local _, y = e:pos()
    if y < ground then
        y = y + 100
        if y > ground then
            y = ground
        end
        e:set_pos_y(y)
    end
end

-- 0x476af0: the parts fade out, then `Kill_enemy_work`.
local function fade(e)
    local sub = e:routine(1)
    if sub == 0 then
        e:set_life(-1)
        e:clip(9, 0, 0)
        e:set_routine(1, 1)
        e:set_timer(0, 0)
        e:joint(0x100)
        sub = 1
    end
    if sub == 1 then
        local t = (e:timer(0) + 1) & 0xFFFF
        e:set_timer(0, t)
        if t == 0x1F then
            rgb_sub(e, 0x103010)
        end
        if i16(t) > 0x1E then
            if e:var(RGB) & 0xFF ~= 0 then
                rgb_sub(e, 0x10001)
            end
            if e:var(RGB) & 0xFF00 ~= 0 then
                rgb_sub(e, 0x100)
            end
        end
        if e:var(RGB) & 0xFFFFFF == 0 then
            e:set_routine(1, 2)
        end
    elseif sub == 2 then
        e:kill()
    end
end

function update(e)
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 then
        move(e)
    elseif routine == 3 then
        local sub = e:routine(1)
        if sub == 0 then
            rest(e)
        elseif sub <= 0x12 then
            squash(e)
        end
        -- 0x13 (0x476c80) isn't ported.
    elseif routine == 7 then
        fade(e)
    end
    -- 2 and 4 are `nullsub_1`.
    for index = HISTORY, HISTORY + 4 do
        e:set_var(index, e:var(index + 1))
    end
    e:set_var(HISTORY + 5, e:angle())
    e:seq_sound()
end
