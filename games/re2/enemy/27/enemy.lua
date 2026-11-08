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
-- Not ported: `Type` 1 and 2 and states 3, 4 and 6 (they stand hidden), the
-- hits (`em_damage_table`'s 0x476c60, `em_die_table`'s 0x476ed0), the shot's
-- death (`Routine_0` 3's `Routine_2` 0) and `Routine_1` 0x13, and the parts'
-- squash and colours as drawn (only their counts are kept, in vars).

local HISTORY <const> = 0 -- 0 to 5: `+0x218, +0x21a, +0x21c, +0x21e, +0x224, +0x226`.
local RGB <const> = 6 -- The parts' `Poly_rgb` (all ten alike).
local SIZE_Y <const> = 7 -- The parts' `Size_y` (all ten alike), from the squash on.
local GROUND <const> = 30 -- `Ground`, which the engine finds under it before the tick.

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

-- `funcs_528938`, by `Routine_1` (3, 4 and 6 aren't ported).
local STATES <const> = { [1] = crawl, [2] = jump, [5] = slide, [7] = rest }

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

-- Every part's `Poly_rgb` less `step`. Only its colour, the low three bytes,
-- is kept: the code byte above them doesn't change what the fade reads.
local function rgb_sub(e, step)
    e:set_var(RGB, (e:var(RGB) - step) & 0xFFFFFF)
end

-- `0x4767c0` from `Routine_2` 1, where a tread starts it: the slide to a
-- stop, the squash, then the fade.
local function squash(e)
    local sub = e:routine(2)
    if sub == 0 then
        -- A shot's death; hits aren't ported.
        error("Em_27: the shot's death (Routine_2 0 of 0x4767c0) isn't ported")
    elseif sub == 1 then
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
        e:set_pos_y(math.min(y, ground))
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
