-- The young spiders, `Em_26` (entity id 0x26).
--
-- `Em_26` runs `Routine_0` from `funcs_473657`: 0 is the init
-- (`FUN_004736a0`), 1 the move (`FUN_00473830`) and 3 the death
-- (`FUN_00473f10`); the hit (2), 4 and 7 are `nullsub_1`. The move runs
-- `Routine_1`: 0 only poses (the room's template), 1 the walk (the wait
-- `0x4739d0` and the run `0x473ac0`), 2 the fall, 3 the jump and its bite and
-- 4 the swim. The engine runs `nFloor`, and unless the death took the work out
-- of play (`Kill_enemy_work`) `Oba_ck_em` and `Sca_ck_em` with `0x400`, after
-- `update`, and finds `Ground` under the body before it.
--
-- The giant spider's death (`FUN_004732a0`) sends them: it runs each one's
-- init at once, so it draws the init's `Rnd` itself and leaves it in `DRAWN`.
-- The player's partner (`splayer_work`, `fg_status`'s `0x10000000`) as a
-- target and the shadow's flags at the death aren't ported.

local T21C <const> = 0 -- `+0x21c`: the wait before it can jump again.
local WOBBLE <const> = 1 -- `+0x21e`: the turn it sways by, which changes sign each tick.
local BIT <const> = 2 -- `+0x221`: it has bitten in this jump.
local AIR <const> = 3 -- `+0x222`: it is in the air.
local RATE <const> = 4 -- `+0x223`: the run's turn toward the player, 0x40, or 0xC0 to run away.
local WATER <const> = 5 -- `Water` (`+0x10c`): `Sce_em_set`'s, or the work's before it was sent.
local DRAWN <const> = 6 -- The init's `Rnd` that the giant spider drew, plus 0x100, or 0.
local GROUND <const> = 30 -- `Ground`, which the engine finds under it before the tick.

-- `BYTE_00528870`: `Routine_1` by `Type`'s low byte.
local ROUTINE_BY_TYPE <const> = { [0] = 0, 0, 1, 4, 2, 2, 0, 0 }

-- `DAT_005288a8`'s rows for `FUN_00474110`: the effect's kind, its row, the
-- turn from the body's and its size.
local EFFECTS <const> = {
    [0] = { 0, 0x19, 0, 0x1000 },
    { 0, 1, 0x800, 0x800 },
    { 0x1A, 0, 0, 0x400 },
    { 5, 4, 0, 0x1000 },
    { 4, 0xF, 0, 0xC00 },
}

-- `0x528893`'s bytes by the weapon that killed it: the death's effect row.
local DEATH_EFFECTS <const> = { [10] = 3, [11] = 4 }

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

-- `FUN_00474110(e, 0, row)`: an effect at part 0, on the water's level for a
-- row with bit 7 when there's water above `Ground`.
local function effect(e, row)
    local entry = EFFECTS[row & 0x7F]
    local x, y, z = e:own_part(0)
    local water = i16(e:var(WATER))
    if row & 0x80 ~= 0 and water < i16(e:var(GROUND)) then
        y = water + 10
    end
    e:call_effect(entry[1], entry[2], entry[4], i16(e:angle() + entry[3]), x, y, z)
end

-- `Add_speedXYZ(e, 0, 0)`.
local function add_speed_xyz(e)
    local _, cz = e:cdir()
    local sx, sy, sz = e:speed()
    e:add_pos(e:rot_apply(0, e:angle(), cz, sx, sy, sz))
end

-- `FUN_004741d0`: 1 on `Ground`, 0 in the air, or the water's level once
-- it's within 10 of it.
local function under(e)
    local x, y, z = e:pos()
    local water = e:water_ck(x, z)
    if y < i16(e:var(GROUND)) then
        if y - 10 < water then
            return 0
        end
        return water
    end
    return 1
end

-- The sway: `Cdir_y` turns by `+0x21e`, which changes sign.
local function sway(e, more)
    local wobble = e:var(WOBBLE)
    e:set_angle((e:angle() + more + wobble) & 0xFFFF)
    e:set_var(WOBBLE, i8(-wobble))
end

local function bite_ready(e)
    return e:pl_damage_cnt() & 0x80 == 0
end

-- `FUN_004736a0`.
local function init(e)
    local drawn = e:var(DRAWN)
    e:set_life(1)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    -- `+0x218` to `+0x247`.
    for index = T21C, DRAWN do
        if index ~= WATER then
            e:set_var(index, 0)
        end
    end
    e:clip(0, 0, 0xF)
    e:set_be_flg(e:be_flg() | 0x4C00000)
    e:set_var(WOBBLE, 0x78)
    local draw = drawn ~= 0 and (drawn & 0xFF) or e:rnd()
    e:set_var(T21C, draw & 0x1F)
    e:set_body_ofs_x(0)
    e:set_body_box(-10, 10, 10, 10)
    e:set_speed(0, 0, 0)
    e:set_cdir(0, 0)
    e:set_damage_cnt(6)
    e:set_timer(0, 0)
    local kind = e:kind()
    local low = kind & 0xFF
    e:routines(1, ROUTINE_BY_TYPE[low] or 0, 0, 0)
    if low == 0 then
        e:set_life(-1)
        e:set_be_flg(e:be_flg() | 0x8044A)
        e:set_kind(kind | 0x4000)
    elseif low == 4 or low == 5 then
        e:set_speed(200, 0, 0)
        e:set_be_flg((e:be_flg() & 0xE3FFFFFF) | 2)
    end
    e:set_shadow(200)
    local x, _, z = e:pos()
    local water = e:water_ck(x, z)
    if drawn == 0 then
        -- `Sce_em_set` found it there.
        e:set_var(WATER, water)
    end
    if water ~= 0 then
        e:set_be_flg(e:be_flg() | 0x400)
    end
end

-- `0x4739d0`: a wait, and the run's way toward the player or away.
local function wait(e)
    local sub = e:routine(3)
    if sub == 0 then
        e:set_routine(3, 1)
        local draw = e:rnd()
        local bit = e:var(BIT)
        e:set_var(BIT, 0)
        e:set_timer(0, (draw + 0x1E) & (bit * 0x1E + 0x3F))
        local away = (1 << (e:rnd() & 0xF)) & 0x1111 ~= 0
        e:set_var(RATE, away and 0xC0 or 0x40)
    end
    if sub <= 1 then
        local left = e:timer(0)
        e:set_timer(0, (left - 1) & 0xFFFF)
        if left == 0 then
            e:set_routine(2, 1)
            e:set_routine(3, 0)
        end
    end
    if under(e) == 0 then
        e:routines(1, 2, 0, 0)
    end
end

-- `0x473ac0`: the run, swaying, which water stops.
local function run(e)
    local sub = e:routine(3)
    if sub == 0 then
        e:set_routine(3, 1)
        local _, sy = e:speed()
        local speed = (e:rnd() & 0x3F) + 0x5A
        e:set_speed(speed, sy, 0)
        e:set_timer(0, e:rnd() & 0x4E)
    end
    if sub <= 1 then
        local px, _, pz = e:player_pos()
        e:goto_point(px, pz, i8(e:var(RATE)))
        sway(e, 0)
        local left = e:timer(0)
        e:set_timer(0, (left - 1) & 0xFFFF)
        if left == 0 then
            e:set_routine(2, 0)
            e:set_routine(3, 0)
        end
    end
    e:add_speed_xz_at(i8(e:var(WOBBLE)) & 0xFFFF)
    local x, _, z = e:pos()
    if e:water_ck(x, z) ~= 0 then
        local ox, oy, oz = e:old_pos()
        e:set_pos(i16(ox), i16(oy), i16(oz))
        return
    end
    if under(e) == 0 then
        e:routines(1, 2, 0, 0)
    end
end

-- `funcs_473888`, by `Routine_2`.
local WALKS <const> = { [0] = wait, run }

-- `Routine_1` 1: the walk, the jump at the player once it faces them, and
-- the death under a player who steps on it.
local function walk(e)
    local step = WALKS[e:routine(2)]
    if step then
        step(e)
    end
    local px, _, pz = e:player_pos()
    local turn = e:goto_ck(px, pz, 0x100)
    local wait_left = e:var(T21C)
    if wait_left ~= 0 then
        wait_left = wait_left - 1
        e:set_var(T21C, wait_left)
    end
    if e:distance() < 2000 and turn == 0 and wait_left == 0 and bite_ready(e) then
        e:routines(1, 3, 0, 0)
    end
    local moving = e:pl_routine(1)
    if e:distance() < 500
        and e:noise() & 6 ~= 0
        and e:floor() == e:player_floor()
        and e:pl_routine(0) == 1
        and moving >= 1 and moving <= 4
    then
        e:routines(3, 0, 0, 0)
    end
    e:joint(0x400)
end

-- `Routine_1` 2: the fall, to `Ground` or into water.
local function fall(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        local sx, _, sz = e:speed()
        e:set_speed(sx, 0x28, sz)
        e:set_be_flg(e:be_flg() | 2)
        e:set_var(AIR, 1)
    end
    add_speed_xyz(e)
    local sx, sy, sz = e:speed()
    e:set_speed(sx, i16(sy) < 800 and sy + 0x28 or 800, sz)
    e:set_damage_cnt(0xFF)
    local landed = under(e)
    if landed ~= 0 then
        e:set_damage_cnt(0)
        e:set_var(AIR, 0)
        e:set_be_flg((e:be_flg() & ~2) | 0x4000000)
        if landed < 0 then
            e:set_pos_y(landed + 10)
            e:set_routine(1, 4)
            effect(e, 0x82)
            e:set_be_flg(e:be_flg() | 0x400)
        else
            e:set_pos_y(i16(e:var(GROUND)))
            e:set_routine(1, 1)
            e:set_be_flg(e:be_flg() & ~0x400)
        end
        e:set_routine(2, 0)
        e:set_routine(3, 0)
    end
    e:joint(0x400)
end

-- `FUN_00474050(e, 0)`: the bite on a player within its reach and up to 2999
-- below it, once a jump. `FUN_0048a140` starts the player's hit reaction
-- (`Routine` 2, by `Cdir_ck`), and `Pl_life_down(1, 1)` never kills.
local function bite(e)
    local _, y = e:pos()
    local _, py = e:player_pos()
    local below = py - y
    if e:player_at_w() + 10 <= e:distance()
        or below < 0
        or below > 2999
        or e:var(BIT) ~= 0
        or not bite_ready(e)
    then
        return false
    end
    local behind = e:cdir_player()
    e:pl_set_routine(2, behind and 1 or 0, 0, 0)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:pl_life_down(1, true)
    effect(e, 1)
    e:set_var(BIT, 1)
    e:set_angle((e:angle() + 0x800) & 0xFFFF)
    local _, sy, sz = e:speed()
    e:set_speed(0x5A, sy, sz)
    return true
end

-- `Routine_1` 3: the jump, which bites, and lands on `Ground` or drowns.
local function jump(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        local draw = e:rnd()
        local _, _, sz = e:speed()
        e:set_speed((draw & 0x3F) + 0x5A, -300, sz)
        e:set_be_flg(e:be_flg() | 0x8000000)
        e:set_var(BIT, 0)
        e:set_var(AIR, 1)
    end
    add_speed_xyz(e)
    local sx, sy, sz = e:speed()
    e:set_speed(sx, sy + 0x28, sz)
    bite(e)
    local landed = under(e)
    if landed == 0 then
        return
    end
    e:set_be_flg(e:be_flg() & ~0x8000000)
    e:set_var(AIR, 0)
    if landed < 0 then
        effect(e, 0x82)
        e:set_pos_y(landed + 10)
        e:set_life(-1)
        e:routines(3, 0, 0, 0)
        e:set_be_flg(e:be_flg() | 0x400)
        return
    end
    e:routines(1, 1, 0, 0)
    e:set_var(T21C, (e:rnd() & 0x1F) + 0x1E)
    e:set_pos_y(i16(e:var(GROUND)))
    e:set_be_flg(e:be_flg() & ~0x400)
end

-- `Routine_1` 4: the swim on the water's level, with ripples, until it
-- reaches land or drowns.
local function swim(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_timer(0, 0x1C2)
        e:set_timer(1, ((e:rnd() & 1 ~= 0 and 0x40 or 0) - 0x20) & 0xFFFF)
        e:set_timer(2, e:rnd() & 0x1F)
        e:set_be_flg(e:be_flg() | 0xC000000)
    end
    local px, _, pz = e:player_pos()
    local turn = e:goto_ck(px, pz, 0x100)
    sway(e, i16(e:timer(1)))
    e:joint(0x400)
    local ripple = (e:timer(2) + 1) & 0xFF
    e:set_timer(2, ripple)
    if ripple & 0xF == 0 then
        effect(e, 0x82)
        e:set_timer(2, (e:timer(2) + (e:rnd() & 7)) & 0xFF)
    end
    if e:distance() < 2000 and turn == 0 and bite_ready(e) then
        e:routines(1, 3, 0, 0)
    end
    e:set_pos_y(i16(e:var(WATER)) + 10)
    if under(e) >= 0 then
        e:routines(1, 1, 0, 0)
        return
    end
    local left = (e:timer(0) - 1) & 0xFFFF
    e:set_timer(0, left)
    if left == 0 then
        e:set_life(-1)
        e:routines(3, 0, 0, 0)
    end
end

-- `FUN_00473830`, by `Routine_1`.
local MOVES <const> = {
    [0] = function(e)
        e:joint(0x400)
    end,
    walk,
    fall,
    jump,
    swim,
}

-- `FUN_00473ff0`: the body sinks, turning, and goes once it's under.
local function sink(e)
    local cx, cz = e:cdir()
    e:set_angle((e:angle() + 0x20) & 0xFFFF)
    e:set_cdir(cx, (cz + 0x40) & 0xFFFF)
    e:set_be_flg(e:be_flg() | 2)
    local _, y = e:pos()
    y = y + 0x1E
    e:set_pos_y(y)
    e:joint(0x400)
    if i16(e:var(GROUND)) <= y or y < i16(e:var(WATER)) then
        e:kill()
    end
end

-- `FUN_00473f70`: its sound and an effect, and the work goes the tick after.
local function die_here(e)
    if e:routine(2) ~= 0 then
        e:kill()
        return
    end
    e:set_routine(2, 1)
    -- On the floor the shadow's flags change; that isn't ported.
    if e:var(AIR) ~= 0 then
        e:set_be_flg(e:be_flg() | 0x400)
    end
    e:sound(7)
    effect(e, DEATH_EFFECTS[e:routine(1)] or 0)
end

-- `FUN_00473f10`.
local function die(e)
    local x, _, z = e:pos()
    if e:water_ck(x, z) < i16(e:var(GROUND)) and e:var(AIR) == 0 then
        sink(e)
    else
        die_here(e)
    end
end

function update(e)
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 then
        local move = MOVES[e:routine(1)]
        if move then
            move(e)
        end
    elseif routine == 3 then
        die(e)
    end
    -- 2, 4 and 7 are `nullsub_1`.
end
