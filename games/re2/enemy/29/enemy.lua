-- The cockroaches, `Em_29` (entity id 0x29): room 40B's swarm of 24, which
-- crawl over the walls of a round pipe and jump at the player.
--
-- `Em_29` runs `Routine_0` from `funcs_47BFDB`: 0 is the init
-- (`FUN_0047c0e0`), 1 the move (`FUN_0047c290`), and 2 (a hit) and 3 (the
-- tread) `FUN_0047cd20`; 4 and 7 are `nullsub_1`. The move runs `Routine_1`
-- from `funcs_47C2B2`: 0 the wait (`0x47c2c0`), 1 the crawl (`0x47c340`), 2
-- the jump (`0x47c550`), 3 on the player (`0x47c9b0`) and 4 the fall off the
-- player (`0x47caf0`). `update` is `Em_29` after its stop check; the engine
-- runs the stop check, `Damage_cnt` and `Oba_set_ofs` around it, and neither
-- `Oba_ck_em` nor `Sca_ck_em`.
--
-- `Em_29` also sets `Pos` to the attach point's offset before its stop check
-- while `Routine_0..2` read 1, 3, 1. Routine 3's sub 1 writes the same offset
-- on every tick it runs and nothing else moves the body, so that write
-- changes nothing and isn't ported.
--
-- Not ported yet: the tread, the hits and the deaths (`Routine_0` 2 and 3);
-- and the flame count (`+0x228`).

local ANGLE <const> = 0 -- `+0x218`: the angle round the pipe's axis.
local STEP <const> = 1 -- `+0x21a`: the last crawl's step round it.
local HEADING <const> = 2 -- `+0x21c`: the `Cdir_y` the crawl turns to.
local FLAGS <const> = 3 -- `+0x21e`: 1 holds an attach point, 2 attached, 4 trodden, 8 on the pipe.
local TX <const> = 4 -- `+0x220` to `+0x224`: the jump's target point.
local TY <const> = 5
local TZ <const> = 6
local SLOT <const> = 7 -- `+0x226`: the attach point, 0 to 31.
local TROD <const> = 8 -- `+0x227`: trodden once.
local FLAME <const> = 9 -- `+0x228`: the flame count.
local SVX <const> = 10 -- `Super_vector`, which `RotMatrix` turns into `pSuper`.
local SVY <const> = 11
local SVZ <const> = 12

-- `Em_29`'s swarm-wide words (`swarm`): `0x689590` the ticks with more than
-- two on the player, `0x689594` how many are on it, `0x689598` the attach
-- points taken, and `0x68959C` the player was killed.
local ON_TICKS <const> = 0
local ON_PLAYER <const> = 1
local TAKEN <const> = 2
local KILLED <const> = 3

-- The pipe: its axis runs along x at this z and y, and its radius.
local AXIS_Z <const> = -25200
local AXIS_Y <const> = -1800
local RADIUS <const> = 1800
-- The crawl's step round the pipe: `vz * ARC_NUM / ARC_DEN`, the arc's
-- length to its angle at the radius.
local ARC_NUM <const> = 0x64000
local ARC_DEN <const> = 0x113FA0
-- `Pos_x`'s limits while it is on the pipe.
local X_MAX <const> = -7300
local X_MIN <const> = -27000
local SPEED_STEP <const> = 0x14
local SPEED_MAX <const> = 0x8C
local TURN <const> = 0x20
local FLY_MAX <const> = 200
local CLIMB <const> = 100
local FLY_TICKS <const> = 0x5A
local REACH <const> = 200
-- `0x689590` past this when a roach lands: the player dies.
local KILL_TICKS <const> = 0xE1
-- Parts 2 and 3: the wings.
local WINGS <const> = (1 << 2) | (1 << 3)

-- `0x528da0`: the attach points, each a part of the player, a point on it and
-- a turn (`part, ofs_x, ofs_y, ofs_z, rot_x, rot_y, rot_z`). 29 to 31 repeat 0 to 2.
local POINTS <const> = {
    [0] = { 8, 245, -306, 10, 0, 2048, -1024 },
    { 8, 97, -316, -106, 9017, -546, -4165 },
    { 8, -43, -253, 50, 7998, -41, -4835 },
    { 1, 150, -569, -102, 460, 2000, 3426 },
    { 1, 207, -169, 90, 3906, 2160, 2976 },
    { 1, 134, -168, -194, 3546, 2160, 2846 },
    { 1, 182, 0, -40, 284, 2286, 3036 },
    { 2, 125, 353, 90, 143, 1460, 3046 },
    { 3, 60, 279, -30, 29, 2100, 3044 },
    { 5, 120, 256, 80, 269, 1580, 2882 },
    { 6, 47, 136, -100, 69, 2522, 3052 },
    { 9, -23, 230, -116, 3515, 3416, 2876 },
    { 10, 80, -30, -50, 285, 3356, 3486 },
    { 11, -30, -30, -50, 133, 3576, 3197 },
    { 12, -14, -20, -70, 0, 2866, 190 },
    { 13, 19, -4, 40, 1280, 1126, 1876 },
    { 13, 82, 206, 37, 1849, 36, 960 },
    { 0, -208, -674, 65, 2319, 1842, 1139 },
    { 0, -202, -314, -74, 1769, 1842, 999 },
    { 1, -177, -14, 162, 2689, 1842, 959 },
    { 2, 163, -14, -58, 3992, 3468, 3095 },
    { 3, -180, -194, -8, 4, 162, 2945 },
    { 4, -1, -354, -23, 64, 3678, 3088 },
    { 6, -213, -354, 47, 3851, 4058, 3049 },
    { 7, -23, -354, 47, 226, 4058, 3039 },
    { 10, 33, -354, 47, 4031, 4058, 3379 },
    { 13, -34, -214, 35, 55, 4058, 3119 },
    { 1, -153, -214, -237, 405, 3378, 3409 },
    { 1, 8, -214, 226, 545, 798, 2529 },
    { 8, 245, -306, 10, 0, 2048, -1024 },
    { 8, 97, -316, -106, 9017, -546, -4165 },
    { 8, -43, -253, 50, 7998, -41, -4835 },
}

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function abs(value)
    return value < 0 and -value or value
end

-- C's `/`, which rounds toward zero.
local function cdiv(a, b)
    local q = a // b
    if q < 0 and q * b ~= a then
        q = q + 1
    end
    return q
end

-- `v / 4096` toward zero (`CDQ; AND EDX,0xFFF; ADD; SAR 12`).
local function trunc0(v)
    if v >= 0 then
        return v // 4096
    end
    return -((-v) // 4096)
end

local function set_flags(e, set, clear)
    e:set_var(FLAGS, (e:var(FLAGS) | (set or 0)) & ~(clear or 0) & 0xFFFF)
end

-- `RotMatrix(&Super_vector, pSuper)`.
local function rot_super(e)
    e:set_super(e:var(SVX), e:var(SVY), e:var(SVZ))
end

-- `Em_init_super`: `Super_vector` cleared, and the identity from it.
local function init_super(e)
    e:set_var(SVX, 0)
    e:set_var(SVY, 0)
    e:set_var(SVZ, 0)
    e:init_super()
end

-- `Mot_play(e, clip, hokan, 0)`: 1 once the clip has ended.
local function mot_play(e, clip, hokan)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    return e:joint(0x1000 // (hokan + 1)) and 1 or 0
end

-- The point on the pipe at `ANGLE`, and its surface's turn (the end of
-- `FUN_0047d0a0` and of the crawl's step).
local function place_on_pipe(e)
    local angle = i16(e:var(ANGLE))
    local x = e:pos()
    local z = trunc0(e:rcos(angle) * RADIUS) + AXIS_Z
    local y = AXIS_Y - trunc0(e:rsin(angle) * RADIUS)
    e:set_pos(x, y, z)
    e:set_var(SVY, 0xC00)
    e:set_var(SVZ, i16(0xC00 - angle))
    rot_super(e)
end

-- `FUN_0047d0a0`: onto the pipe's wall, at the angle it is at from the axis.
local function on_pipe(e)
    local _, y, z = e:pos()
    e:set_var(ANGLE, e:direction(AXIS_Z, AXIS_Y, z, y))
    place_on_pipe(e)
end

local function show_wings(e, shown)
    local hidden = e:hidden_parts()
    e:set_hidden_parts(shown and (hidden & ~WINGS) or (hidden | WINGS))
end

-- `FUN_0047d150`: an attach point `Rnd` picks among the free ones, else the
-- first free one, else 0x20. The scan sets the index itself in the mask, not
-- its bit (`OR ESI,EAX` at 0x47d19b).
local function pick_point(e)
    local taken = e:swarm(TAKEN)
    for _ = 1, 32 do
        local index = e:rnd() >> 3
        local bit = 1 << index
        if taken & bit == 0 then
            e:set_swarm(TAKEN, taken | bit)
            return index
        end
    end
    for index = 0, 31 do
        if taken & (1 << index) == 0 then
            e:set_swarm(TAKEN, taken | index)
            return index
        end
    end
    return 0x20
end

-- `FUN_0047c0e0`.
local function init(e)
    e:set_kind(e:kind() | 0x2000)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_life(10)
    e:set_be_flg(e:be_flg() | 0x800000)
    for word = ON_TICKS, KILLED do
        e:set_swarm(word, 0)
    end
    e:clip(1, 0, 0)
    for index = ANGLE, FLAME do
        e:set_var(index, 0)
    end
    e:joint(0x100)
    show_wings(e, false)
    e:set_be_flg(e:be_flg() | 0x2000000)
    e:set_var(SVX, 0)
    e:set_var(SVY, 0xC00)
    e:set_var(SVZ, 0)
    -- `Atd[0]`: `At_w`, `At_h` and `W`; `Ofs_y` 0.
    e:set_body_box(0, 1, 100, 1)
    init_super(e)
    e:set_angle(e:rnd() << 4)
    local cx = e:cdir()
    e:set_cdir(cx, 0)
    on_pipe(e)
    e:set_be_flg(e:be_flg() | 0x1C000002)
    -- `Kage_work_set` with no size: no shadow.
    e:set_shadow(0)
end

-- `Routine_1` 0: the wait, clip 1 and then a frame or so of clip 0, until a
-- draw starts a crawl.
local function wait(e)
    set_flags(e, 8)
    local sub = e:routine(2)
    if sub == 0 then
        if mot_play(e, 1, 0) == 1 then
            if e:rnd() & 2 == 0 then
                e:routines(1, 1, 0, 0)
                return
            end
            e:set_routine(2, 1)
        end
    elseif sub == 1 then
        sub = sub - mot_play(e, 0, 0)
        e:set_routine(2, sub)
        if sub ~= 0 then
            if e:rnd() & 2 == 0 then
                e:routines(1, 1, 0, 0)
                return
            end
            e:set_routine(2, 0)
        end
    end
end

-- `Routine_1` 1: the crawl round the pipe and along it. It plays no clip.
local function crawl(e)
    set_flags(e, 8)
    local sub = e:routine(2)
    if sub == 0 then
        local _, y, z = e:pos()
        e:set_var(ANGLE, e:direction(AXIS_Z, AXIS_Y, z, y))
        local _, sy, sz = e:speed()
        e:set_speed(0, sy, sz)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub == 1 then
        if e:rnd() & 3 == 0 then
            local _, y = e:pos()
            if y < AXIS_Y and e:rnd() & 3 == 0 then
                e:routines(1, 2, 0, 0)
                return
            end
            e:routines(1, 0, 0, 0)
            return
        end
        if e:rnd() & 3 == 2 then
            e:sound(0)
        end
        e:set_var(HEADING, (e:rnd() * 8 - 0x400 + e:angle()) & 0xFFFF)
        e:set_routine(2, 2)
        sub = 2
    end
    if sub ~= 2 then
        return
    end
    local sx, sy, sz = e:speed()
    if i16(sx) < SPEED_MAX then
        sx = sx + SPEED_STEP
    end
    e:set_speed(sx, sy, sz)
    local turn = e:turn_to(e:var(HEADING), TURN)
    if turn == 0 then
        e:set_routine(2, 1)
    end
    e:set_angle((e:angle() + turn) & 0xFFFF)
    -- `Rot_vector(Cdir_y, (0, 0, Spd_x))`.
    local vx, vz = e:rotate(e:angle(), 0, sx)
    local step = i16(cdiv(i16(vz) * ARC_NUM, ARC_DEN))
    e:set_var(STEP, step)
    e:set_var(ANGLE, (e:var(ANGLE) + step) & 0xFFF)
    local x, y, z = e:pos()
    e:set_pos(x + i16(vx), y, z)
    place_on_pipe(e)
end

-- The attach point's place on the player (`CompM` of its part's `Workm` with
-- the point), or the player's own place without a pose of it.
local function target(e)
    local point = POINTS[e:var(SLOT)] or POINTS[0]
    local tx, ty, tz = e:player_part_point(point[1], point[2], point[3], point[4])
    if tx == nil then
        tx, ty, tz = e:player_pos()
    end
    return i16(tx), i16(ty), i16(tz)
end

-- `Routine_1` 2: the jump off the wall, and the flight at an attach point.
local function jump(e)
    set_flags(e, 8)
    local sub = e:routine(2)
    if sub == 0 then
        e:sound(3)
        show_wings(e, true)
        for timer = 0, 2 do
            e:set_timer(timer, 0)
        end
        e:set_speed(0, 0, 0)
        e:set_var(SLOT, pick_point(e))
        set_flags(e, 1)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub == 1 then
        if mot_play(e, 2, 0) == 1 then
            init_super(e)
            local px, _, pz = e:player_pos()
            e:goto_point(px, pz, 0x800)
            e:set_be_flg(e:be_flg() | 0x2000)
            e:set_timer(0, 0)
            e:set_routine(2, 2)
        end
        return
    end
    if sub ~= 2 then
        return
    end
    mot_play(e, 3, 0)
    local x, y, z = e:pos()
    local dz, dy = AXIS_Z - z, AXIS_Y - y
    local d = e:hypot(dy, dz)
    local beat = (e:timer(2) + 1) & 0xFF
    e:set_timer(2, beat)
    if beat == 0x10 then
        e:set_timer(2, 0)
        e:sound(3)
    end
    if d > RADIUS then
        if e:var(FLAGS) & 2 ~= 0 or i16(e:timer(0)) > FLY_TICKS - 1 then
            -- Back onto the wall.
            set_flags(e, 0, 2)
            on_pipe(e)
            show_wings(e, false)
            set_flags(e, 0, 1)
            e:set_swarm(TAKEN, e:swarm(TAKEN) & ~(1 << (e:var(SLOT) & 0x1F)) & 0xFFFFFFFF)
            e:routines(1, 0, 0, 0)
            e:set_be_flg(e:be_flg() & ~0x2000)
            return
        end
        -- It slides along the wall.
        local sx, _, sz = e:speed()
        e:set_speed(sx, 0, sz)
        z = -cdiv(dz * RADIUS, d) + AXIS_Z
        y = -cdiv(dy * RADIUS, d) + AXIS_Y
        e:set_pos(x, y, z)
    end
    if i16(e:timer(0)) < FLY_TICKS then
        e:set_timer(0, e:timer(0) + 1)
    end
    local tx, ty, tz = target(e)
    e:set_var(TX, tx)
    e:set_var(TY, ty)
    e:set_var(TZ, tz)
    if e:hypot(x - tx, z - tz) < REACH and abs(y - ty) < REACH then
        set_flags(e, 2, 8)
        e:set_swarm(ON_PLAYER, e:swarm(ON_PLAYER) + 1)
        if e:pl_damage_cnt() == 0 then
            -- `Pl_Em_damage` (`FUN_0047cf90`) from its start, with this
            -- body's second bank as the player's `pSub1`: each landing
            -- starts the struggle again.
            e:pl_set_routine(5, 0, 0, 0)
            e:pl_hold()
            e:pl_set_sub1()
            if e:swarm(ON_TICKS) > KILL_TICKS then
                -- The dword write: routines 3, 0, 0, 0, and `Die`.
                e:pl_set_routine(3, 0, 0, 0)
                e:set_player_life(-1)
                e:pl_set_damage_cnt(0x80)
                e:set_swarm(KILLED, 1)
            end
        end
        e:routines(1, 3, 0, 0)
        e:set_be_flg(e:be_flg() & ~0x2000)
        return
    end
    local sx, sy, sz = e:speed()
    if e:var(FLAGS) & 2 == 0 and i16(e:timer(0)) < FLY_TICKS then
        e:goto_point(tx, tz, 0x40)
        local climb = (ty - i16(sy)) - y
        if climb < CLIMB + 1 then
            sy = climb < -CLIMB and -CLIMB or climb
        else
            sy = CLIMB
        end
    else
        sy = i16(sy) + 1
    end
    e:set_pos(x, y + i16(sy), z)
    if i16(sx) < FLY_MAX then
        sx = sx + e:timer(1)
        e:set_timer(1, e:timer(1) + 1)
    end
    e:set_speed(sx, sy, sz)
    e:add_speed_xz()
end

-- `Routine_1` 3 (`0x47c9b0`): on the player, on its point's part, until
-- `Timer0` runs out; each new key press takes 8 off it rather than 1.
local function on_player(e)
    local point = POINTS[e:var(SLOT)] or POINTS[0]
    local sub = e:routine(2)
    if sub == 0 then
        e:sound(3)
        e:set_be_flg(e:be_flg() | 0x3000000)
        e:set_cdir(point[5], point[7])
        e:set_angle(point[6] & 0xFFFF)
        e:set_super_part(point[1])
        e:set_timer(0, (e:rnd() >> 4) + 0x5A)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub == 1 then
        e:set_pos(point[2], point[3], point[4])
        mot_play(e, 2, 0)
        local timer = i16(e:timer(0) - (e:key_trg() ~= 0 and 8 or 1))
        e:set_timer(0, timer & 0xFFFF)
        if timer < 0 and e:swarm(KILLED) == 0 then
            e:routines(1, 4, 0, 0)
            local on = e:swarm(ON_PLAYER) - 1
            e:set_swarm(ON_PLAYER, on)
            if on == 0 then
                -- The byte `Routine_0` and `Routine_1`, and the word after.
                e:pl_set_routine(1, 0, 0, 0)
                return
            end
        end
    end
    if e:swarm(ON_PLAYER) > 2 then
        e:set_swarm(ON_TICKS, e:swarm(ON_TICKS) + 1)
    end
end

-- `Routine_1` 4 (`0x47caf0`): the fall off the player, away from it, down
-- to the floor and back onto the pipe. Its turn on the way down steps away
-- from 0xC00, as the original's does.
local function fall_off(e)
    local sub = e:routine(2)
    if sub == 0 then
        local point = POINTS[e:var(SLOT)] or POINTS[0]
        e:set_be_flg(e:be_flg() & ~0x1000000)
        local x, y, z = e:player_part_point(point[1], point[2], point[3], point[4])
        if x == nil then
            x, y, z = e:player_pos()
        end
        e:set_super_part(nil)
        e:set_pos(x, y, z)
        local px, _, pz = e:player_pos()
        local sx, sz = e:rotate(e:direction(px, pz, i16(x), i16(z)), 0x32, 0)
        e:set_cdir(0, 0)
        e:set_angle(0)
        init_super(e)
        e:set_timer(0, 0)
        e:set_speed(sx, 0x14, sz)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub ~= 1 then
        return
    end
    e:add_speed_xz()
    local timer = i16(e:timer(0) + 2)
    e:set_timer(0, timer & 0xFFFF)
    local sx, sy, sz = e:speed()
    sy = i16(sy + timer)
    e:set_speed(sx, sy, sz)
    local x, y, z = e:pos()
    y = y + sy
    e:set_pos(x, y, z)
    local vz = i16(e:var(SVZ))
    if abs(vz - 0xC00) > 0x10 then
        vz = vz < 0xC00 and vz - 0x10 or vz + 0x10
        e:set_var(SVZ, i16(vz))
        rot_super(e)
    end
    local ground, ceiling = e:sca_ck_hit(x, y, z, 1, 0x400)
    ground = i16(ground)
    if ceiling < ground then
        ground = ceiling
    end
    if ground <= y then
        e:sound(1)
        e:set_pos(x, ground, z)
        on_pipe(e)
        show_wings(e, false)
        set_flags(e, 0, 1)
        e:set_swarm(TAKEN, e:swarm(TAKEN) & ~(1 << (e:var(SLOT) & 0x1F)) & 0xFFFFFFFF)
        e:routines(1, 0, 0, 0)
    end
end

-- `funcs_47C2B2`, by `Routine_1`.
local MOVES <const> = { [0] = wait, crawl, jump, on_player, fall_off }

function update(e)
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 then
        local move = MOVES[e:routine(1)]
        if move then
            move(e)
        end
    end
    -- 2 and 3 (`FUN_0047cd20`) aren't ported yet; 4 and 7 are `nullsub_1`.
    e:seq_sound()
    if e:var(FLAGS) & 8 ~= 0 then
        local x, y, z = e:pos()
        if x > X_MAX then
            x = X_MAX
        end
        if x < X_MIN then
            x = X_MIN
        end
        e:set_pos(x, y, z)
        -- The tread isn't ported yet.
    end
    if e:swarm(ON_PLAYER) < 3 then
        e:set_swarm(ON_TICKS, 0)
    end
    if e:swarm(KILLED) ~= 0 and e:var(FLAGS) & 1 == 0 then
        e:routines(1, 2, 0, 0)
    end
end
