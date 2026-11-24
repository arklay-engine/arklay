-- The giant moth, `Em_3A` (entity id 0x3A).
--
-- `Em_3A` (`0x4acc70`) runs `Routine_0` from `0x52bbf8`: 0 is the init
-- (`FUN_004acd70`), 1 the move (`FUN_004acfa0`), 2 the hit (`FUN_004aea50`),
-- 3 the death (`FUN_004af4f0`) and 7 `FUN_004afa20`; 4 is `nullsub`. The move
-- runs `Routine_1` from `0x52bc18`. Room 613's moth (`Type` 0x10) waits on the
-- ceiling in `Routine_1` 10 until the room's area 3 raises `Sce_free0`. The
-- engine runs `Oba_ck_em` and `Sca_ck_em` with `0x400` after `update`.
--
-- Awake, it rests (`Routine_1` 0), picks its next move by the distance (1),
-- turns to the target bobbing (4), crawls toward it (5), takes off and flies
-- at it in a hop (11, and 2), hovers shedding scales (8), and grabs the player
-- to squeeze them (3), whose side of it is `0x4afac0`, the hook the init sets
-- at `0x9886e0` (`Mover::em3a_held`).
--
-- Not ported yet: `Routine_1` 6, 7, 9 and 12, the hit, the death, state 7,
-- the partner as the target, the wings' beat (`0x52bdf8`), the shadow
-- (`Kage_work_set`), and the parts' draw words the init sets (part 0's from
-- `+0x98`, and parts 13 to 16's `0x4000`).

local TARGET <const> = 0 -- `+0x218`: 1 while the partner is the target.
local WINGS <const> = 1 -- `+0x21e`: the wings' beat, by `FUN_004afbf0`'s table.
local HELD <const> = 2 -- `+0x220`: with `UP`, `Be_flg` takes 0x18000000 for 0xC000000.
local FLAGS <const> = 3 -- `+0x221`: 1 sheds scales as `SHED` counts; 0x80 beats the wings.
local SHOT <const> = 4 -- `+0x227`: a hit that wakes it.
local B228 <const> = 5 -- `+0x228`.
local UP <const> = 6 -- `+0x229`: 1 while it hangs from the ceiling.
local B22C <const> = 7 -- `+0x22c`: a count down; 0x3C while the player's `0x989eec` 0x20.
local B22D <const> = 8 -- `+0x22d`.
local B22F <const> = 9 -- `+0x22f`.
local SHED <const> = 10 -- `+0x230`: a count down; each odd tick sheds scales.
local B232 <const> = 11 -- `+0x232`: a count down.
local DUST <const> = 12 -- `+0x233`: the ticks of dust left.
local B23A <const> = 13 -- `+0x23a`: cleared while `Damage_cnt` is 0.
-- `+0x224` to `+0x226` and `+0x234` to `+0x238`: the routines and the speed as
-- the move left them.
local LAST_R1 <const> = 14
local LAST_R2 <const> = 15
local LAST_R3 <const> = 16
local LAST_SX <const> = 17
local LAST_SY <const> = 18
local LAST_SZ <const> = 19
-- Var 20 is the engine's `Ground` (`+0x1c2`, `e:ground`).
local HOP <const> = 21 -- `+0x222`: the hop's row of `HOPS`, and how far the hops have gone.
local BOB <const> = 22 -- `+0x223`: the turn a tick while it bobs, or the crawl's turn rate.
local LUNGE <const> = 23 -- `+0x22a`: the flight may end in the grab.
local STEER <const> = 24 -- `+0x22b`: the flight steers at the target.
local PEAK <const> = 25 -- `+0x23c`: half the height the flight rose from the floor.
local FROM_X <const> = 26 -- `+0x23e`: where the hop began.
local FROM_Z <const> = 27 -- `+0x240`.
local SAME <const> = 28 -- `+0x242`: the hops in a row that came down where they began.
local B243 <const> = 29 -- `+0x243`: a short rest.

local TYPE_CEILING <const> = 0x10

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

local function abs(value)
    return value < 0 and -value or value
end

-- `0x52be08`: the effects' rows, `{ kind, row, size }`, where kind's 0x80
-- starts it on the part's matrix.
local ROWS <const> = {
    [0] = { 0x00, 25, 0x2000 },
    { 0x00, 25, 0x2800 },
    { 0x00, 1, 0x1800 },
    { 0x85, 4, 0x1000 },
    { 0x85, 1, 0x1000 },
    { 0x20, 0, 0x2800 },
    { 0x21, 0, 0x2000 },
    { 0x85, 4, 0x1800 },
    { 0x84, 15, 0x1000 },
    { 0x84, 15, 0x1800 },
    { 0x86, 0, 0x1000 },
    { 0x84, 12, 0x2000 },
    { 0x00, 9, 0x2800 },
    { 0x85, 1, 0x1800 },
}
-- `0x52be60`: the points `FUN_004b0170` starts an effect at, `{ part, x, y, z }`.
local POINTS <const> = {
    [0] = { 3, 0, 0, 0 },
    { 3, -200, 0, -1400 },
    { 3, 400, 0, -800 },
    { 4, 0, 0, -500 },
    { 5, 0, 0, 0 },
    { 5, 200, 0, 1300 },
    { 5, -300, 0, 900 },
    { 6, 0, 0, 400 },
    { 3, -600, -200, -1300 },
    { 3, 500, -100, -1400 },
    { 5, 700, -300, 1500 },
    { 5, -400, -200, 1600 },
}

-- The target's place: the player, or with `TARGET` the partner.
local function target_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

-- `FUN_004b0080`: row `row` at `part`, toward the target.
local function effect_at_part(e, part, row)
    local r = ROWS[row]
    local mx, _, mz = e:pos()
    local tx, tz = target_xz(e)
    local dir = e:direction(mx, mz, tx, tz)
    if r[1] & 0x80 ~= 0 then
        e:effect(r[1] & 0x7F, r[2], r[3], dir, part, 0, 0, 0)
    else
        local px, py, pz = e:own_part(part)
        e:call_effect(r[1], r[2], r[3], dir, px, py, pz)
    end
end

-- `FUN_004b0270`: row `row` at the world point `(x, y, z)` and an offset
-- turned with the body.
local function effect_turned(e, row, x, y, z, ox, oy, oz)
    local r = ROWS[row & 0x7F]
    local cx, cz = e:cdir()
    local rx, ry, rz = e:rot_apply(cx, e:angle(), cz, ox, oy, oz)
    e:call_effect(r[1] & 0x7F, r[2], r[3], i16(e:angle()), i16(x + rx), i16(y + ry), i16(z + rz))
end

-- `FUN_004b0170`: row `row` at `POINTS[point]`, along the moth's facing; a
-- row of 0x80 or more starts it on the floor under the part, turned with the
-- body.
local function effect_at_point(e, row, point)
    local r = ROWS[row & 0x7F]
    local p = POINTS[point]
    if row & 0x80 == 0 then
        e:effect(r[1] & 0x7F, r[2], r[3], i16(e:angle()), p[1], p[2], p[3], p[4])
        return
    end
    local x, _, z = e:own_part(p[1])
    effect_turned(e, row, x, e:ground(), z, p[2], 0, p[4])
end

-- `FUN_004ad2f0`: the dust a take-off or a landing raises.
local function dust(e)
    for _, point in ipairs({ 1, 8, 3, 10, 6, 7 }) do
        effect_at_point(e, 0x86, point)
    end
end

-- `FUN_004b0310`: the body box for the ceiling (`on`) or the room.
local function box(e, on)
    if on then
        e:set_body_box(0, 800, 0x7D0, 4)
    else
        e:set_body_box(-800, 800, 1000, 800)
    end
end

-- `FUN_004afe50`: the target, the player unless the partner is nearer.
-- The partner isn't ported.
local function pick_target(e)
    e:set_var(TARGET, 0)
end

-- `FUN_004afeb0`: the target within `within`, faced within `rate`, alive and
-- not reeling.
local function in_reach(e, within, rate)
    local x, z = target_xz(e)
    return e:distance() < within
        and e:goto_ck(x, z, rate) == 0
        and e:player_life() >= 0
        and e:pl_damage_cnt() & 0x80 == 0
end

-- `FUN_0045fb00`: the player's hit, front or back.
local function player_hurt(e, behind)
    e:pl_set_routine(2, (behind and 1 or 0) + 2, 0, 0)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
end

-- `FUN_004aff20`: the flight's blow, 0xE (HARD 0x14): 2 when it kills, 1 when
-- it lands, 0 for none.
local function strike(e)
    if not in_reach(e, 0x73A, 0x180) then
        return 0
    end
    e:sound(4)
    if e:pl_life_down(e:hard_mode() and 0x14 or 0xE) == 2 then
        e:pl_set_routine(3, 0, 0, 0)
        e:pl_or_be_flg(2)
        return 2
    end
    player_hurt(e, e:cdir_player())
    return 1
end

-- `FUN_004ad1b0`: a hit while it sheds scales turns it to `Routine_1` 6,
-- unless `Type`'s 1.
local function shot_turn(e)
    if e:kind() & 1 == 0 and e:var(SHOT) ~= 0 and e:var(FLAGS) & 1 ~= 0 then
        e:routines(1, 6, 0, 0)
    end
end

-- The grab, from where the flight or the turn lets it.
local function to_grab(e)
    e:set_damage_cnt(e:damage_cnt() | 0x80)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:routines(1, 3, 0, 0)
end

-- `FUN_004acd70`.
local function init(e)
    e:set_be_flg(e:be_flg() | 0xC00000)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_speed(0, 0, 0)
    local life = e:easy() and 100 or 150
    if e:hard_mode() then
        life = 200
    end
    e:set_life(life)
    e:set_d_life(life // 3)
    e:set_be_flg(e:be_flg() | 0xC000000)
    box(e, false)
    e:set_var(FLAGS, 1)
    e:set_var(B22D, 2)
    e:set_var(B22F, 0x1E)
    if e:kind() & TYPE_CEILING ~= 0 then
        e:set_routine(1, 10)
        e:clip(8, 0, 0)
        e:joint(0x100)
        e:set_var(UP, 1)
        e:set_var(HELD, 1)
        e:set_sce_free(0, 0)
    else
        e:clip(0, 0, 0)
        e:joint(0x100)
    end
end

-- `FUN_004ae530`, `Routine_1` 10's first step: up against the ceiling.
local function ceiling_hang(e)
    e:set_routine(2, 1)
    e:set_routine(3, 0)
    e:set_be_flg(e:be_flg() | 4)
    local _, roll = e:cdir()
    e:set_cdir(0, roll)
    box(e, true)
    local _, sy = e:speed()
    e:set_speed(0x30F, sy, 0)
    e:add_speed_xz()
    e:set_cdir(0, -0x400)
    e:clip(8, 0, 0)
    e:joint(0x100)
end

-- `FUN_004ae5a0`: until the room's scene word, or a hit.
local function ceiling_wait(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_var(SHOT, 0)
    end
    if e:sce_free(0) ~= 0 or e:var(SHOT) ~= 0 then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
    end
    e:joint(0x100)
end

-- `FUN_004adfa0`: it drops off the ceiling, rolling level and spinning
-- either way, and lands on the floor to `Routine_1` 0 (or, out of its move,
-- `Routine_2` 2 of the routine it's in).
local function ceiling_drop(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        local _, sy, sz = e:speed()
        e:set_speed(-0x46, 0, sz)
        e:set_timer(0, e:angle())
        e:clip(4, 0, 0xF)
        e:set_timer(2, (e:rnd() & 1) ~= 0 and 0x40 or 0xC0)
    end
    local pitch, roll = e:cdir()
    roll = i16(roll)
    if roll < 0 then
        roll = roll + 0x80
    else
        roll = 0
        e:set_var(UP, 0)
    end
    e:set_cdir(pitch, roll)
    e:set_angle(e:angle() + i8(e:timer(2)))
    local sx, sy, sz = e:speed()
    if i16(sy) < 0x1F4 then
        e:set_speed(sx, sy + 0xA, sz)
    end
    e:set_cdir(pitch, 0)
    e:add_speed_xyz_at(i16(e:timer(0)) - i16(e:angle()))
    e:set_cdir(pitch, roll)
    local x, y, z = e:pos()
    local ground = e:ground()
    if y + 0x30F >= ground then
        e:set_pos(x, ground - 0x30F, z)
        e:set_var(HELD, 0)
        e:sound(7)
        if i16(roll) >= 0 then
            if e:routine(0) == 1 then
                e:routines(1, 0, 0, 0)
            else
                e:set_routine(2, 2)
                e:set_routine(3, 0)
            end
            e:set_be_flg(e:be_flg() & ~4)
            box(e, false)
            e:set_var(B228, 0)
            e:set_var(SHOT, 0)
            e:set_pos(x, ground, z)
            e:clip(0, 0, 0xF)
        end
    end
    e:joint(0x100)
    e:seq_sound()
end

local CEILING <const> = { [0] = ceiling_hang, ceiling_wait, ceiling_drop }

-- `FUN_004ae510`.
local function ceiling(e)
    local step = CEILING[e:routine(2)]
    if step then
        step(e)
    end
end

-- `FUN_004ad040`: a rest of `Rnd() & 0x1F` + 0x1E ticks (0x14 after
-- `+0x243`).
local function rest(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        if e:var(B243) == 0 then
            e:set_timer(0, (e:rnd() & 0x1F) + 0x1E)
        else
            e:set_timer(0, 0x14)
        end
        e:set_var(B243, 0)
        e:clip(0, 0, 0xF)
    end
    e:joint(0x100)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        e:routines(1, 1, 0, 0)
    end
end

-- `0x52bc4c` and `0x52bc54`: what `choose` picks far off, and near.
local FAR <const> = { [0] = 5, 4, 5, 4, 5, 4, 5, 4 }
local NEAR <const> = { [0] = 8, 4, 4, 4, 4, 4, 4, 4 }

-- `FUN_004ad0c0`: past 10000 a crawl or a turn; within 6000, once `B22C` is
-- out, a turn to fly at the target, or a hover; else a crawl or a turn. One
-- that doesn't shed scales flies off (9) instead.
local function choose(e)
    local d = e:distance()
    if d <= 10000 and d <= 6000 and e:var(B22C) == 0 then
        e:set_var(HOP, 2)
        e:set_routine(1, 4)
        if e:var(FLAGS) & 1 ~= 0 then
            e:set_routine(1, NEAR[e:rnd() & 7])
        end
        e:set_var(STEER, 0)
        e:set_var(LUNGE, 1)
    else
        e:set_var(HOP, d > 10000 and 0 or 1)
        e:set_routine(1, FAR[e:rnd() & 7])
        if e:var(FLAGS) & 1 == 0 then
            e:set_routine(1, 9)
        end
        e:set_var(STEER, 1)
        e:set_var(LUNGE, 0)
    end
    e:set_routine(2, 0)
    e:set_routine(3, 0)
    local _, roll = e:cdir()
    e:set_cdir(0, roll)
    shot_turn(e)
    e:joint(0x100)
end

-- `0x52bc6c`: a hop's rows by `HOP`, `{ speed, rise, lift }`: the speed ahead
-- once it's up, the upward speed it rises to first, and the flight's lift.
local HOPS <const> = {
    [0] = { 500, -140, 10 },
    { 360, -340, 30 },
    { 180, -320, 40 },
    { 10, -200, 40 },
}

-- `FUN_004ad220`: the take-off.
local function take_off(e)
    local row = HOPS[e:var(HOP)]
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_speed(0, -20, 0)
        e:set_timer(0, row[3])
        local _, roll = e:cdir()
        e:set_cdir(0, roll)
        e:set_timer(3, 0)
        e:clip(1, 0, 7)
        e:set_var(HELD, 1)
        local x, _, z = e:pos()
        e:set_var(FROM_X, i16(x))
        e:set_var(FROM_Z, i16(z))
        dust(e)
    end
    local sx, sy, sz = e:speed()
    sy = i16(sy - 0x14)
    e:set_speed(sx, sy, sz)
    local x, y, z = e:pos()
    e:set_pos(x, y + sy // 16 - 0x36, z)
    if sy < row[2] then
        e:set_routine(2, 1)
        e:set_routine(3, 0)
        e:set_speed(row[1], sy, sz)
        e:clip(4, 0, 7)
    end
end

-- The flight's steering and its blow (the start of `FUN_004ae630` and
-- `FUN_004ad350`): it strikes once a hop, turning off by 0x80 either way.
local function fly_steer(e)
    if e:timer(3) == 0 and e:var(STEER) ~= 0 then
        local x, z = target_xz(e)
        e:set_angle(e:angle() + e:goto_ck(x, z, 4))
        local hit = strike(e)
        e:set_timer(3, hit)
        if hit ~= 0 then
            e:set_angle(e:angle() + ((e:rnd() & 1) ~= 0 and 0x80 or -0x80))
        end
    end
    local sx, sy, sz = e:speed()
    e:set_speed(sx, i16(sy + e:timer(0)), sz)
    e:add_speed_xyz()
end

-- The flight near the top of its rise: a turn back up to the ceiling (7)
-- after a wall, or the grab.
local function fly_reach(e)
    local _, sy = e:speed()
    if abs(i16(sy)) >= 2 * i16(e:timer(0)) then
        return
    end
    if e:var(B228) ~= 0 and e:sca_info() & 1 ~= 0 then
        e:routines(1, 7, 0, 0)
    end
    if e:var(LUNGE) ~= 0 and in_reach(e, 0x73A, 0x180) then
        to_grab(e)
    end
end

-- `FUN_004ae630`: `Routine_1` 11's flight, up to half its rise and down onto
-- the floor.
local function hop_fly(e)
    fly_steer(e)
    local three = e:routine(3)
    local _, y = e:pos()
    local _, sy = e:speed()
    if three == 0 then
        if i16(sy) >= 0 then
            e:set_routine(3, 1)
            e:set_var(PEAK, i16((y - e:ground()) // 2))
        end
    elseif three == 1 then
        if y >= e:var(PEAK) then
            e:set_timer(0, -i16(e:timer(0)))
            e:set_routine(3, 2)
        end
    elseif three == 2 or three == 3 then
        if y + 0xC3 >= e:ground() then
            e:set_routine(2, 2)
            e:set_routine(3, 0)
            return
        end
        if three == 2 and i16(sy) <= i16(e:timer(0)) then
            e:set_routine(3, 3)
            local sx, _, sz = e:speed()
            e:set_speed(sx, abs(i16(e:timer(0))), sz)
            e:set_timer(0, 0)
        end
    end
    if e:routine(3) < 2 then
        fly_reach(e)
    end
end

-- `FUN_004ad350`: `Routine_1` 2's flight, until it comes down to the floor.
local function fly(e)
    fly_steer(e)
    local _, y = e:pos()
    local _, sy = e:speed()
    if i16(sy) >= 0 and y + 0x30F >= e:ground() then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
        return
    end
    fly_reach(e)
end

-- `FUN_004ad490`: the landing's bounce.
local function land(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        local x, _, z = e:pos()
        e:set_pos(x, e:ground(), z)
        e:clip(0, 0, 7)
        dust(e)
        local sx, _, sz = e:speed()
        e:set_speed(sx, -0x50, sz)
        e:sound(7)
    end
    e:add_speed_xyz()
    local sx, sy, sz = e:speed()
    sx = i16(sx - 0xA)
    if sx <= 0xA then
        sx = 5
    end
    sy = i16(sy + 0x1E)
    e:set_speed(sx, sy, sz)
    local x, y, z = e:pos()
    if sy > 0 and y >= e:ground() then
        e:set_pos(x, e:ground(), z)
        e:set_routine(2, 3)
        e:set_routine(3, 0)
        e:set_var(HELD, 0)
        e:sound(7)
    end
end

-- `FUN_004ad540`: `Routine_1` 2's wait after landing.
local function landed(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_timer(0, (e:rnd() & 0xF) + 0xF)
    end
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        e:routines(1, e:player_life() < 0 and 9 or 1, 0, 0)
    end
    shot_turn(e)
end

-- `FUN_004ae830`: `Routine_1` 11's crawl after it lands. Three hops in a row
-- that come down within 800 of where they began turn it to `Routine_1` 12.
local function hop_crawl(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_routine(3, 1)
        e:set_timer(0, 0xF)
        e:clip(3, 0, 7)
        local sx, sy = e:speed()
        e:set_speed(sx, sy, 0)
        local x, _, z = e:pos()
        if abs(x - e:var(FROM_X)) < 0x320 and abs(z - e:var(FROM_Z)) < 0x320 then
            local same = (e:var(SAME) + 1) & 0xFF
            e:set_var(SAME, same)
            if same >= 3 then
                e:routines(1, 12, 0, 0)
                return
            end
        else
            e:set_var(SAME, 0)
        end
        three = 1
    end
    if three == 1 then
        local _, sy, sz = e:speed()
        e:set_speed(e:now_seq() & 0x10000 ~= 0 and 0x4B or 0x25, sy, sz)
        e:add_speed_xz()
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 or e:sca_info() & 1 ~= 0 then
            e:set_routine(3, 2)
            e:clip(0, 0, 7)
            e:set_timer(0, (e:rnd() & 0xF) + 0xF)
        end
        e:seq_sound()
    elseif three == 2 then
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            e:routines(1, e:player_life() < 0 and 9 or 1, 0, 0)
        end
    end
    shot_turn(e)
end

local HOP_STEPS <const> = { [0] = take_off, hop_fly, land, hop_crawl }
local FLIGHT_STEPS <const> = { [0] = take_off, fly, land, landed }

-- `FUN_004ae5f0`, `Routine_1` 11.
local function hop(e)
    local step = HOP_STEPS[e:routine(2)]
    if step then
        step(e)
    end
    e:joint(0x200)
    e:seq_sound()
end

-- `FUN_004ad1e0`, `Routine_1` 2.
local function flight(e)
    local step = FLIGHT_STEPS[e:routine(2)]
    if step then
        step(e)
    end
    e:joint(0x100)
    e:seq_sound()
end

-- `0x52bc8c`: the pitch of a bob, by its tick.
local BOBS <const> = { [0] = 0, 12, 24, 12, 0, -12, -24, -12 }

-- `FUN_004ad9a0`, `Routine_1` 4: it turns to the target bobbing, in bursts,
-- for 60 ticks or until it faces it, then hops (or flies off with the player
-- dead). One that doesn't shed scales grabs a target in reach.
local function turn(e)
    local tx, tz = target_xz(e)
    local rate = e:goto_ck(tx, tz, 0x30)
    if e:player_life() < 0 then
        rate = -rate
    end
    local two = e:routine(2)
    if two == 0 then
        e:set_routine(2, 1)
        e:set_var(BOB, rate & 0xFF)
        e:set_timer(1, 0x3C)
        local _, roll = e:cdir()
        e:set_cdir(0, roll)
        two = 1
    end
    if two == 1 then
        e:set_routine(2, 2)
        e:clip(3, 0, 0xF)
        e:set_timer(0, (e:rnd() & 0xF) + 0xF)
        two = 2
    end
    if two == 2 then
        e:set_angle(e:angle() + i8(e:var(BOB)))
        local left = e:timer(0)
        local _, roll = e:cdir()
        e:set_cdir(BOBS[left & 7], roll)
        left = (left - 1) & 0xFFFF
        e:set_timer(0, left)
        if left == 0 then
            e:set_routine(2, 3)
        end
    elseif two == 3 then
        e:set_routine(2, 4)
        e:clip(0, 0, 0xF)
        e:set_timer(0, (e:rnd() & 0xF) + 0xF)
        local _, roll = e:cdir()
        e:set_cdir(0, roll)
        two = 4
    end
    if two == 4 then
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            e:set_routine(2, 1)
        end
    end
    e:set_timer(1, e:timer(1) - 1)
    if e:timer(1) == 0 or rate & 0xFFFF == 0 then
        if e:var(FLAGS) & 1 ~= 0 and e:player_life() >= 0 then
            e:set_routine(1, 11)
        else
            e:set_routine(1, 9)
        end
        e:set_routine(2, 0)
        e:set_routine(3, 0)
    end
    if e:var(FLAGS) & 1 == 0 and e:var(B22C) == 0 and in_reach(e, 0x73A, 0x180) then
        to_grab(e)
    end
    shot_turn(e)
    e:joint(0x100)
    e:seq_sound()
end

-- `FUN_004adc80`: a tick of the crawl along `Root_ck`'s way, bobbing, slower
-- off the clip's stepping frames.
local function crawl_step(e)
    local px, _, pz = e:player_pos()
    e:root_ck(px, pz)
    local dx, dz = e:dest()
    local rate = e:goto_ck(dx, dz, i8(e:var(BOB)))
    if e:player_life() < 0 then
        rate = -rate
    end
    e:set_angle(e:angle() + rate)
    local _, roll = e:cdir()
    e:set_cdir(BOBS[((e:timer(0) & 0xFF) >> 1) & 7], roll)
    local speed = e:var(FLAGS) & 1 ~= 0 and 0x32 or 0x4B
    if e:now_seq() & 0x10000 == 0 then
        speed = speed >> 1
    end
    local _, sy, sz = e:speed()
    e:set_speed(speed, sy, sz)
    e:add_speed_xz()
end

-- `FUN_004adb70`, `Routine_1` 5: a crawl toward the target for half the
-- distance at 50 a tick, then a hop, or past 10000 (6000 on a second crawl)
-- a new choice.
local function crawl(e)
    local d = e:distance()
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(2, 0, 0xF)
        e:set_speed(0x32, 0, 0)
        e:set_timer(0, (d >> 1) // 50)
        e:set_var(BOB, 0x10)
        local _, roll = e:cdir()
        e:set_cdir(0, roll)
    end
    crawl_step(e)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        e:set_routine(1, 11)
        local row = e:var(HOP)
        if row == 0 then
            if d > 10000 then
                e:set_routine(1, 1)
            end
        elseif row == 1 then
            if d <= 6000 then
                e:set_var(STEER, 0)
                e:set_var(LUNGE, 1)
            else
                e:set_routine(1, 1)
            end
        end
        e:set_var(HOP, (row + 1) & 0xFF)
        e:set_routine(2, 0)
        e:set_routine(3, 0)
    end
    shot_turn(e)
    e:joint(0x100)
    e:seq_sound()
end

-- `FUN_004ae0f0`, `Routine_1` 8: it rises off the floor and hovers, sinking
-- and rising for six beats as its scales fall, and comes down again.
local function hover(e)
    local two = e:routine(2)
    if two == 2 then
        local x, y, z = e:pos()
        if y < e:ground() then
            e:set_pos(x, y + 0x6F, z)
        end
        if e:joint(0x100) then
            local px, _, pz = e:pos()
            e:set_pos(px, e:ground(), pz)
            e:set_var(HELD, 0)
            e:routines(1, 1, 0, 0)
        end
        return
    end
    if two == 0 then
        e:set_routine(2, 1)
        local sx, _, sz = e:speed()
        e:set_speed(sx, 0, sz)
        e:set_var(HELD, 1)
        e:set_timer(1, 6)
        e:set_timer(2, 0xF)
        e:set_timer(3, -8)
        e:clip(4, 0, 0xF)
        local _, roll = e:cdir()
        e:set_cdir(0, roll)
        dust(e)
    end
    local x, y, z = e:pos()
    if e:routine(3) == 0 then
        y = y - 0x6F
        e:set_pos(x, y, z)
        if y <= e:ground() - 0x30F then
            e:set_routine(3, 1)
        end
    else
        e:set_pos(x, y + i8(e:timer(3)), z)
        local beat = e:timer(2)
        if beat == 0xF or beat == 7 then
            e:sound(5)
        end
        beat = (beat - 1) & 0xFF
        e:set_timer(2, beat)
        if beat == 0 then
            e:set_timer(2, 0xF)
            e:set_timer(3, -i8(e:timer(3)))
            e:set_timer(1, e:timer(1) - 1)
            if e:timer(1) == 0 then
                e:set_routine(2, 2)
                e:clip(0, 0, 0xF)
            end
        end
    end
    e:joint(0x100)
    if i16(e:timer(1)) < 6 then
        local px, py, pz = e:pos()
        local ox = i16(0x400 - ((e:rnd() & 0xFF) << 4))
        local oy = i16(e:ground() - ((e:rnd() & 0x1F) << 3) - 0x32)
        local oz = i16(((e:rnd() & 0xFF) - 0x60) << 4)
        effect_turned(e, 5, i16(px), i16(py), i16(pz), ox, oy, oz)
    end
end

-- `FUN_004ad5c0`: the grab's start. The player goes into `Routine_0` 5 on
-- this body's clips, and it closes in by a 26th of the gap a tick (to 500
-- from behind, else 1000).
local function grab_start(e)
    e:set_be_flg(e:be_flg() | 2)
    e:set_damage_cnt(e:damage_cnt() | 0x80)
    e:set_routine(2, 1)
    e:set_routine(3, 0)
    e:pl_hold()
    e:pl_set_routine(5, 0, 0, 0)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:clip(9, 0, 0xF)
    local gap = e:distance() - (e:cdir_player() and 500 or 1000)
    local _, roll = e:cdir()
    e:set_cdir(0, roll)
    e:set_var(SAME, 0)
    local _, sy, sz = e:speed()
    -- `/ 26` toward 0.
    local step = gap >= 0 and gap // 26 or -((-gap) // 26)
    e:set_speed(step, sy, sz)
end

-- `0x52bca8`: the parts the squeeze sheds scales from.
local SQUEEZE_PARTS <const> = { [0] = 12, 9, 9, 12 }

-- `FUN_004ad690`: it closes in and lifts; then it squeezes, the life going
-- every fourth tick (HARD 1, else 3, from `Timer3`'s 20) while mashing wears
-- `Timer0` (20) down. Either running out lets go.
local function grab_squeeze(e)
    if e:routine(3) == 0 then
        if e:joint(0x100) then
            e:set_routine(3, 1)
            e:set_timer(0, 0x14)
            e:set_timer(2, 0)
            e:set_timer(3, 0x14)
            e:clip(0xA, 0, 0)
            e:joint(0x100)
            e:pl_set_routine_at(2, 1)
        end
        e:add_speed_xz()
        local px, _, pz = e:player_pos()
        e:goto_point(px, pz, 0x18)
        if e:var(FLAGS) & 1 == 0 then
            local x, y, z = e:pos()
            e:set_pos(x, y - 0x37, z)
        end
        return
    end
    e:set_timer(0, e:timer(0) - (e:gacha() and 1 or 0))
    if i16(e:timer(0)) <= 0 or i8(e:timer(3)) <= 0 then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
        e:pl_set_routine_at(1, 3)
        return
    end
    if e:rnd() & 7 == 0 then
        effect_at_part(e, SQUEEZE_PARTS[e:rnd() & 3], 2)
    end
    e:joint(0x100)
    e:seq_sound()
    local beat = e:timer(2)
    e:set_timer(2, beat + 1)
    local x, y, z = e:pos()
    e:set_pos(x, y + (beat & 2 ~= 0 and 0x20 or -0x20), z)
    if ((beat + 1) & 0xFF) % 4 ~= 0 then
        return
    end
    local down = e:hard_mode() and 1 or 3
    e:set_timer(3, e:timer(3) - down)
    if e:pl_life_down(down) == 2 then
        e:pl_set_routine(3, 0, 0, 0)
        e:set_routine(2, 2)
        e:set_routine(3, 0)
    end
end

-- `FUN_004ad830`: the drop back to the floor after the grab, and a rest.
local function grab_drop(e)
    local three = e:routine(3)
    if three == 2 then
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(B22C, 0x3C)
        end
        e:joint(0x100)
        return
    end
    if three == 0 then
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:set_routine(3, 1)
        local _, _, sz = e:speed()
        e:set_speed(-100, 0x28, sz)
        e:set_timer(0, e:angle())
        e:rnd()
        e:set_timer(2, 0x80)
        e:clip(4, 0, 0xF)
    end
    local sx, sy, sz = e:speed()
    if i16(sy) < 0x1F4 then
        sy = sy + 0x28
        e:set_speed(sx, sy, sz)
    end
    e:set_angle(e:angle() + i8(e:timer(2)))
    e:add_speed_xyz_at(i16(e:timer(0)) - i16(e:angle()))
    local x, y, z = e:pos()
    if i16(sy) >= 0 and y + 0x30F >= e:ground() then
        e:set_pos(x, e:ground(), z)
        e:set_be_flg(e:be_flg() & ~2)
        e:set_var(HELD, 0)
        e:sound(7)
        e:clear_status(0x20)
        if e:var(FLAGS) & 1 ~= 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(B22C, 0x78)
            e:clip(4, 0, 0)
        else
            e:set_routine(3, 2)
            e:set_timer(0, 0x1E)
            e:clip(0, 0, 0)
        end
    end
    e:joint(0x100)
end

local GRAB <const> = { [0] = grab_start, grab_squeeze, grab_drop }

-- `FUN_004ad5a0`, `Routine_1` 3.
local function grab(e)
    local step = GRAB[e:routine(2)]
    if step then
        step(e)
    end
end

local MOVES <const> = {
    [0] = rest,
    [1] = choose,
    [2] = flight,
    [3] = grab,
    [4] = turn,
    [5] = crawl,
    [8] = hover,
    [10] = ceiling,
    [11] = hop,
}

-- `FUN_004acfa0`.
local function move(e)
    pick_target(e)
    if e:var(B22C) ~= 0 then
        e:set_var(B22C, e:var(B22C) - 1)
    end
    local step = MOVES[e:routine(1)]
    if step then
        step(e)
    end
    e:set_var(LAST_R1, e:routine(1))
    e:set_var(LAST_R2, e:routine(2))
    e:set_var(LAST_R3, e:routine(3))
    local sx, sy, sz = e:speed()
    e:set_var(LAST_SX, sx)
    e:set_var(LAST_SY, sy)
    e:set_var(LAST_SZ, sz)
    if e:status(0x20) then
        e:set_var(B22C, 0x3C)
    end
end

local STATES <const> = { [0] = init, [1] = move }

-- `FUN_004afbf0`: the scales a shedding moth drops on its odd ticks, and the
-- wings' beat, which isn't ported.
local function shed(e)
    local left = e:var(SHED)
    if left ~= 0 then
        left = left - 1
        e:set_var(SHED, left)
        if e:var(FLAGS) & 1 ~= 0 and left & 1 ~= 0 then
            effect_at_point(e, 3, e:rnd() & 7)
        end
    end
end

-- `FUN_004acc70`.
function update(e)
    if e:damage_cnt() & 0x7F ~= 0 then
        e:set_damage_cnt(e:damage_cnt() - 1)
    else
        e:set_var(B23A, 0)
    end
    if e:var(B232) ~= 0 then
        e:set_var(B232, e:var(B232) - 1)
    end
    local state = STATES[e:routine(0)]
    if state then
        state(e)
    end
    shed(e)
    if e:var(DUST) ~= 0 then
        local pick = e:rnd() & 3
        if pick ~= 0 then
            effect_at_part(e, pick - 1, 0xA)
        else
            effect_at_point(e, 0xA, e:rnd() & 3)
            effect_at_point(e, 0xA, (e:rnd() & 3) + 4)
        end
        e:set_var(DUST, e:var(DUST) - 1)
    end
    local flags = e:be_flg() & ~0x1C000000
    if e:var(HELD) == 0 and e:var(UP) == 0 then
        flags = flags | 0xC000000
    else
        flags = flags | 0x18000000
    end
    e:set_be_flg(flags)
end
