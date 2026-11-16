-- The G adult, `Em_28` (entity id 0x28): the grown G of room 303 and 309 in
-- scenario A, which spits the G embryos (`Em_27`) and bites.
--
-- `Em_28` (0x476ef0) runs `Root_ck` toward the player, counts `Damage_cnt` and
-- its own counters down, runs `Routine_0` from `funcs_476F4F` (0x5289d8): 0 is
-- the init (`FUN_004771d0`), 1 the move (`FUN_00477870`), 2 the hit
-- (`FUN_00479b00`), 3 the death (`FUN_0047b5b0`) and 4 the rest of a hit
-- (`FUN_0047bc80`). Then it counts its timers down, draws `Rnd` twice for the
-- eye's next target when the frame asks for a sound (`Now_seq` `0x8000000`),
-- animates the eye, runs the wobble (`Routine_3`'s 1), the spark trail
-- (`FUN_004797a0`) and the death's break-up (`FUN_0047ab00`), turns the neck,
-- and plays the frame's sound. The engine then runs `Oba_set_ofs`, places
-- `Atd[1]` and `Atd[2]` on parts 3 and 0x12, and runs `Oba_ck_hit` and
-- `Sca_ck_em` (`0x400`).
--
-- `FUN_00477870` switches on `Type`: 0 is the adult proper, which chooses its
-- state with a pre (`funcs_47789B`, 0x528a40: 0, 1 and 9 have one) and runs
-- the state from `funcs_4778B2` (0x528a68). The init gives a `Type` 1 the
-- state 1 and `Type` 0 (the room's other adult, with `Routine_3`'s 1 set), and
-- a `Type` 0 `Type`'s `0x4000` and the birth (state 8).
--
-- Not ported: the other `Type`s of `FUN_00477870`, the hit and the death
-- (`Routine_0` 2 to 4), the birth (state 8), the spark trail, and the effects, whose `Rnd` draws are kept where the move
-- draws them but not the effects themselves. The neck's turn is a third of
-- `Rot_neck_em_n`'s on each of parts 2 to 4 (`Neck::apply`); the wobble
-- places every part (`FUN_004e0070`) as well as part 1's children.

local COOL <const> = 0 -- `+0x218`: the wait before it spits again.
local COUNT <const> = 1 -- `+0x21a`: the ticks it has been turned away from the player.
local WAIT <const> = 2 -- `+0x21c`: a wait the states set and `FUN_004779a0` counts down.
local LOCK <const> = 3 -- `+0x21d`: counts down every tick; a bite waits on it.
local B21E <const> = 4 -- `+0x21e`.
local B21F <const> = 5 -- `+0x21f`: bit 1 the wobble, 0x10 and 0x20 the states 0x16 and 2's.
local MARK <const> = 6 -- `+0x220` (`Id`): at least 1 once the adult is hurt.
local HURT <const> = 7 -- `+0x222` (`At_obj_no`): the hurt marker, `0x82` while a move goes on.
local SINGLE <const> = 8 -- `+0x228`: counts down.
local PHASE <const> = 9 -- `+0x221` (`Sc_id`): where the wobble is in its table.
local SWAY <const> = 10 -- Part 1's morph weight, which the wobble moves about.

-- `BYTE_005289f8`: the wobble's sine, as signed bytes.
local WOBBLE <const> = {
    [0] = 12, 23, 35, 43, 51, 60, 66, 71, 75, 79, 82, 86, 89, 92, 94, 96, 97, 98, 99, 100,
    99, 98, 97, 96, 94, 92, 89, 87, 85, 83, 80, 78, 75, 72, 68, 65, 62, 58, 54, 49,
    44, 39, 34, 29, 24, 19, 13, 7, 0, -6, -15, -11, -26, -30, -32, -34, -35, -35, -34, -31,
    -25, -20, -10, 0,
}

local LIFE <const> = 600
local LIFE_EASY <const> = 400
local LIFE_HARD <const> = 1000

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

-- `Goto00_ck` at the player.
local function face(e, rate)
    local px, _, pz = e:player_pos()
    return e:goto_ck(px, pz, rate)
end

-- `Goto00` at the player.
local function turn_to_player(e, rate)
    local px, _, pz = e:player_pos()
    e:goto_point(px, pz, rate)
end

-- `FUN_004771d0`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    local life = LIFE
    if e:easy() then
        life = LIFE_EASY
    end
    if e:hard_mode() then
        life = LIFE_HARD
    end
    e:set_life(life)
    e:set_d_life(0x82)
    e:set_neck_flags(0)
    e:set_be_flg(e:be_flg() | 0x400000)
    e:set_var(COOL, 10)
    e:set_var(COUNT, 0)
    e:set_var(WAIT, (e:rnd() >> 1) + 0x78)
    e:set_var(LOCK, 0)
    e:set_var(B21E, 0)
    e:set_var(B21F, 0)
    e:set_var(MARK, 6)
    e:set_var(HURT, 0)
    e:set_var(SINGLE, 0)
    e:set_var(PHASE, 0)
    e:set_var(SWAY, 0)
    -- `Atd[0]` to `Atd[2]`: 0x514 (800 for the others) wide, 1500 high from
    -- -1500; `Eff_at_r` 800 and `nOba` 3.
    e:set_atd(0, 0, -1500, 0x514, 1500, 0x514)
    e:set_be_flg(e:be_flg() | 0xC000000)
    e:clip(1, 0, 0)
    e:joint(0x100)
    e:set_be_flg(e:be_flg() | 4)
    local low = e:kind() & 0xFFF
    if low == 1 then
        e:set_var(B21F, e:var(B21F) | 1)
        e:set_routine(1, 1)
        e:set_kind(0)
        return
    end
    e:set_kind(e:kind() | 0x4000)
    e:set_routine(1, 8)
end

-- `FUN_004778f0`, the pre of state 0.
local function pre_0(e)
    face(e, 0x400)
    face(e, 0x338)
    e:routines(1, 1, 0, 0)
end

-- The bite's and the turn's pre, `FUN_004779a0` (state 1) and `FUN_004795f0`
-- (state 9): `walk` is 1 or 9.
local function pre_walk(e, walk)
    local near = e:distance()
    local wide, aimed, any
    if walk == 1 then
        wide = face(e, 0x4C8)
    else
        wide = face(e, 0x5F4)
    end
    aimed = face(e, 0x338)
    any = face(e, 0x400)
    if any ~= 0 then
        e:set_var(COUNT, (e:var(COUNT) + (walk == 1 and 1 or 2)) & 0xFFFF)
    end
    if walk == 1 then
        if e:var(WAIT) ~= 0 then
            e:set_var(WAIT, e:var(WAIT) - 1)
        end
        if wide ~= 0 and near < 6000 and e:var(COUNT) > 0x3C then
            e:routines(1, 2, 0, 0)
        end
        -- The point 2000 ahead: a spit goes where nothing blocks it.
        if e:enemies() < 5 and e:var(COOL) == 0 then
            local x, _, z = e:pos()
            local ahead, _, ahead_z = e:rot_apply(0, e:angle(), 0, 2000, 0, 0)
            local ground = e:ground()
            local product = (ground * 0x6E5D4C3B) // 0x100000000
            local v = product - ground
            local floor = ((v // 1024) - (v < 0 and -1 or 0)) & 0x1F
            local info = e:sca_ck_info(x + ahead, z + ahead_z, floor, 0x400, 400)
            if info == 0 then
                e:routines(1, 6, 0, 0)
            end
        end
        if aimed == 0 then
            if near < 0xDAC and e:pl_damage_cnt() == 0 and e:var(LOCK) == 0 then
                e:routines(1, 3, 0, 0)
            end
            if near < 2000 then
                e:routines(1, 9, 0, 0)
            end
        end
    else
        if wide ~= 0 and near < 5000 and e:var(COUNT) > 0x5A then
            e:routines(1, 2, 0, 0)
        end
        if aimed == 0 and near < 0xDAC and e:pl_damage_cnt() == 0 and e:var(LOCK) == 0
            and e:timer(0) < 0x1E then
            e:routines(1, 3, 0, 0)
        end
        if near > 0xDAC then
            e:routines(1, 1, 0, 0)
        end
    end
end

-- Starts the clip of a state's first step.
local function start(e, clip, count, hokan, neck_on)
    e:clip(clip, count, hokan)
    if neck_on then
        e:set_neck_flags(e:neck_flags() | 2)
    else
        e:set_neck_flags(e:neck_flags() & ~2)
    end
end

-- 0x477940: the stand.
local function state_0(e)
    local sub = e:routine(2)
    if sub == 0 then
        local draw = e:rnd()
        e:set_routine(2, 1)
        e:clip(0, draw & 0x1F, 0x3F)
        e:set_neck_flags(e:neck_flags() & ~2)
    elseif sub ~= 1 then
        return
    end
    e:joint(0x40)
end

-- 0x477b30: the walk toward `Dest`.
local function state_1(e)
    local sub = e:routine(2)
    if sub == 0 then
        start(e, 1, 0, 0x3F, false)
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    local dx, dz = e:dest()
    local spd = e:speed()
    if spd > 0xF then
        e:goto_point(dx, dz, 8)
    end
    if spd > 10 then
        e:goto_point(dx, dz, 8)
    end
    e:spd()
    e:joint(0x40)
    if not e:in_cut_view() then
        local sx, sy, sz = e:speed()
        e:set_speed(i16(sx + 0x32), sy, sz)
    end
    e:add_speed_xz()
end

-- The hurt marker the bite, the turn and the retreat keep up.
local function keep_hurt(e)
    if i8(e:var(HURT)) < 3 then
        e:set_var(HURT, e:var(HURT) | 0x82)
    end
    if i8(e:var(MARK)) < 2 then
        e:set_var(MARK, 1)
    end
end

-- 0x477c00: the turn away.
local function state_2(e)
    keep_hurt(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        e:set_var(B21F, e:var(B21F) | 0x20)
        start(e, 8, 0, 0x1F, true)
        e:set_var(COUNT, 0)
    elseif sub ~= 1 then
        return
    end
    turn_to_player(e, -0x20)
    e:spd()
    if not e:joint(0x80) then
        e:add_speed_xz()
        return
    end
    e:set_angle((e:angle() + 0x800) & 0xFFFF)
    e:routines(1, 1, 0, 0)
    e:set_var(WAIT, (e:rnd() >> 1) + 0x78)
    e:set_var(B21F, e:var(B21F) & 0xDF)
    e:set_var(HURT, e:var(HURT) & 0x7F)
end

-- 0x477cf0: the bite.
local function state_3(e)
    keep_hurt(e)
    local sub = e:routine(2)
    e:set_var(LOCK, 0xF)
    if sub == 0 then
        start(e, 4, 0, 0x3F, true)
        e:set_routine(2, 1)
        e:set_timer(0, 0)
        e:set_timer(1, 0)
    elseif sub ~= 1 then
        if sub == 2 then
            e:set_var(LOCK, 0x1E)
        end
        return
    end
    e:spd()
    if e:joint(0x40) then
        e:set_var(HURT, e:var(HURT) & 0x7F)
        e:set_angle((e:angle() + 0x400) & 0xFFFF)
        e:routines(1, 1, 0, 0)
        turn_to_player(e, 0x10)
        e:set_var(WAIT, (e:rnd() >> 1) + 0x78)
    end
    e:add_speed_xz()
    local count = e:move_cnt()
    if count > 5 and count < 0x1E then
        turn_to_player(e, 8)
        if e:distance_now() > 0x1194 or face(e, 0x4C8) ~= 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(LOCK, 0xF)
            e:set_var(HURT, e:var(HURT) & 0x7F)
        end
    end
    e:set_angle((e:angle() + 0x200) & 0xFFFF)
    turn_to_player(e, 4)
    e:set_angle((e:angle() - 0x200) & 0xFFFF)
    if (count - 0x36) & 0xFFFF < 0x16 then
        e:set_angle((e:angle() + 0x200) & 0xFFFF)
        turn_to_player(e, 0x20)
        e:set_angle((e:angle() - 0x200) & 0xFFFF)
        if e:timer(0) == 0 then
            local ax, _, az = e:own_part(0xC)
            local bx, _, bz = e:own_part(10)
            local px, _, pz = e:player_pos()
            if (e:hypot(ax - px, az - pz) < 800 or e:hypot(bx - px, bz - pz) < 1000)
                and e:pl_damage_cnt() == 0 then
                local result = e:pl_life_down(e:hard_mode() and 0x2D or 0x1E, false)
                if result == 2 then
                    e:pl_set_routine(3, 0, 0, 0)
                else
                    local behind = ((e:direction_from_player() - e:player_angle()) & 0xFFF) < 0x800
                    e:pl_set_routine(5, behind and 1 or 0, 0, 0)
                    e:pl_hold()
                end
                e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
                e:pl_set_sub1()
                e:set_timer(0, 1)
                e:set_timer(1, 4)
                e:sound(6)
            end
        end
    end
    if e:timer(0) ~= 0 and e:timer(1) ~= 0 then
        e:set_timer(1, e:timer(1) - 1)
    end
end

-- `Set_enemy_work('\'')`: an embryo (`Type` 2) at part 4 of the adult's
-- feet's height, turned from its facing by `turn` less a draw.
local function release(e, turn)
    local slot = e:spawn(0x27, 2, {})
    if slot == nil then
        return
    end
    local x, _, z = e:own_part(4)
    local _, y = e:pos()
    local draw = e:rnd()
    e:place_spawn(x, y, z, (e:angle() - draw + turn) & 0xFFFF, {}, false)
end

-- 0x478100: the spit, three embryos, the first from the room's template.
local function state_6(e)
    keep_hurt(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(5, 0, 0x3F)
        local flags = e:var(B21F)
        e:set_routine(2, 1)
        if flags & 4 == 0 then
            e:free_work(1)
            e:set_var(B21F, flags | 4)
        end
        release(e, 0x80)
        e:set_timer(0, 0x78)
        e:set_neck_flags(e:neck_flags() | 2)
    elseif sub ~= 1 then
        return
    end
    local t0 = e:timer(0)
    if t0 == 100 and e:enemies() < 5 then
        release(e, 0x880)
    end
    if t0 == 0x5A and e:enemies() < 5 and e:rnd() & 1 ~= 0 then
        release(e, 0x480)
    end
    if e:joint(0x40) then
        e:set_hokan(0x3F)
        e:rnd()
        e:rnd()
    end
    t0 = i16(e:timer(0))
    if t0 < 0x3C and t0 & 0xF == 0 then
        e:rnd()
        e:rnd()
        e:effect(1, 5, 0x1000, i16(e:angle()), 4, 0, 0, 0)
    end
    e:set_timer(0, (t0 - 1) & 0xFFFF)
    if t0 == 0 then
        e:set_var(HURT, e:var(HURT) & 0x7F)
        e:routines(1, 1, 0, 0)
        local draw = e:rnd()
        local cool = draw + 300
        e:set_var(COOL, cool)
        if e:enemies() < 4 then
            e:set_var(COOL, cool >> 1)
        end
        if e:distance() < 4000 and e:pl_damage_cnt() == 0 then
            e:routines(1, 3, 0, 0)
        end
    end
end

-- 0x478020: the stagger after a hit, onto 0x478090.
local function state_4(e)
    local sub = e:routine(2)
    if sub == 0 then
        start(e, 2, 0, 0x3F, true)
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:joint(0x40) then
        e:routines(1, 5, 0, 0)
    end
    e:add_speed_xz()
end

-- 0x478090.
local function state_5(e)
    local sub = e:routine(2)
    if sub == 0 then
        start(e, 3, 0, 0x3F, false)
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:joint(0x40) then
        e:routines(1, 1, 0, 0)
    end
    e:add_speed_xz()
end

-- 0x4783e0.
local function state_7(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(10, 0, 0x3F)
        e:set_routine(2, 1)
        e:set_var(WAIT, (e:rnd() >> 1) + 0x78)
        e:set_neck_flags(e:neck_flags() & ~2)
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:joint(0x40) then
        e:routines(1, 1, 0, 0)
    end
    e:add_speed_xz()
end

-- 0x4796c0: the retreat, the walk backward.
local function state_9(e)
    local sub = e:routine(2)
    if sub == 0 then
        start(e, 1, 0, 0x3F, false)
        e:set_routine(2, 1)
        local draw = e:rnd()
        e:set_speed(10, 0, 0)
        e:set_timer(0, (draw & 0x1F) + 0x82)
    elseif sub ~= 1 then
        return
    end
    local spd = e:speed()
    if spd > 0xF then
        turn_to_player(e, 4)
    end
    if spd > 10 then
        turn_to_player(e, 4)
    end
    e:joint(0x40, true)
    e:add_speed_xz_at(0x800)
    local left = e:timer(0)
    e:set_timer(0, left - 1)
    if left == 0 then
        e:routines(1, 1, 0, 0)
    end
end

local PRE = { [0] = pre_0 }
local STATES = {
    [0] = state_0,
    state_1,
    state_2,
    state_3,
    state_4,
    state_5,
    state_6,
    state_7,
    nil,
    state_9,
}

-- `FUN_00477870`, `Type` 0.
local function move(e)
    local state = e:routine(1)
    if state == 0 then
        pre_0(e)
    elseif state == 1 then
        pre_walk(e, 1)
    elseif state == 9 then
        pre_walk(e, 9)
    end
    local run = STATES[e:routine(1)]
    if run then
        run(e)
    end
end

function update(e)
    local hurt = e:var(HURT) & 0x7F
    if hurt ~= 0 then
        e:set_var(HURT, e:var(HURT) - 1)
    end
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 then
        move(e)
    end
    if e:var(COOL) ~= 0 then
        e:set_var(COOL, e:var(COOL) - 1)
    end
    if e:var(LOCK) ~= 0 then
        e:set_var(LOCK, e:var(LOCK) - 1)
    end
    if e:var(SINGLE) ~= 0 then
        e:set_var(SINGLE, e:var(SINGLE) - 1)
    end
    if e:now_seq() & 0x8000000 ~= 0 then
        e:rnd()
        e:rnd()
    end
    -- The wobble (`Routine_3`'s 1): part 1's first morph weight swings about
    -- where it was, and the body is posed on it.
    if e:var(B21F) & 1 ~= 0 then
        local phase = e:var(PHASE)
        local weights = { e:morph_weights(1) }
        weights[1] = i16((weights[1] or 0) - 5 * WOBBLE[phase])
        e:morph_all(1, table.unpack(weights))
        e:set_var(PHASE, (phase + 1) & 0x3F)
    end
    -- `Rot_neck_em_n` on `Neck_no` 4 (rate 0x20, limit 0x32c, from `Init`):
    -- the posing turns parts 2 to 4 a third of it each.
    if e:var(B21F) & 0x40 == 0 then
        e:rot_neck_part(4, 0x20, 0x32C)
    end
    e:seq_sound()
end
