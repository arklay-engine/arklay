-- The alligator, `Em_23` (entity id 0x23).
--
-- `Em_23` runs `Routine_0` from `funcs_4687AE` (`0x527e18`): 0 is the init
-- (`FUN_00468a10`) and 1 the move (`FUN_00468ce0`); the hit (2,
-- `FUN_00469cf0`) and the death (3, `FUN_0046a6d0`) aren't ported yet. The
-- move runs `funcs_468CEB` before its clip and `funcs_468D18` after it, by
-- `Routine_1`: 0 the entry (`FUN_00468d30`), 1 the rest (`FUN_00469010`), 2
-- the swim (`FUN_00469030`), 3 the turn (`FUN_00469060`) and 7 the wait at
-- the corridor's end (`FUN_004693b0`), with `FUN_00468e20` before 1 and 2.
-- Room 40A's script drives it: its scene raises `Sce_free0`, which starts the
-- entry, and moves it to the corridor.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and
-- `e:set_var` are the fields of the original's work that the engine doesn't
-- read.

local FLAGS <const> = 0 -- `+0x218`.
local LEG <const> = 1 -- `+0x21a`: 0 in the first leg of the corridor, 1 in the second, 2 at its end.
local SHIFT_X <const> = 2 -- `+0x21c`: what the turn still moves it along `x`.
local SHIFT_Z <const> = 3 -- `+0x21e`: what the turn still moves it back along `z`.
local TMP <const> = 4 -- `+0x220`: last tick's routines, for `Mot_play_init`.
local SNAP <const> = 5 -- `+0x224`: the snap of its jaws (`FUN_00469480`).
local TURNING <const> = 6 -- `+0x225`.
local TANK <const> = 7 -- `+0x226`: the gas tank in its jaws (`FUN_004697f0`).
local SHAKE <const> = 8 -- `+0x227`: the camera's shake (`FUN_00469c00`).
local JAW_A <const> = 9 -- `+0x228`.
local JAW_B <const> = 10 -- `+0x22a`.
local GRABBED <const> = 11 -- `+0x22c`: the ticks since its reach caught the player.
local HIT_WAIT <const> = 12 -- `+0x22e`.
local C232 <const> = 13 -- `+0x232`.
local C233 <const> = 14 -- `+0x233`.
local REST <const> = 15 -- `+0x238`: the rest before it swims.
local TIMER2 <const> = 16 -- `Timer2` (`+0x16a`).
local C231 <const> = 17 -- `+0x231`.
local NECK_X <const> = 18 -- `Neck_point_x` to `_z` (`+0x170`), which `Rot_neck` would turn the head to; the neck isn't ported.
local NECK_Y <const> = 19
local NECK_Z <const> = 20
local TIMER0 <const> = 21 -- `Timer0` (`+0x158`): the snap's count.
local BLOOD <const> = 22 -- `+0x234`: `FUN_0046b1c0` runs each tick while it's 1.

-- `Rot_neck_em`'s neck: `Neck_no` 5, whose part has a rate of 20 and a limit
-- of 560 (its `+0x9c` and `+0xa0`).
local NECK <const> = 5
local NECK_RATE <const> = 20
local NECK_LIMIT <const> = 560
-- `FUN_00469750`: the player within this of part 0 is caught.
local REACH <const> = 0x1450

local LIFE <const> = 25000
-- `WORD_00527dea` and `DAT_00527de8`: the life past 25000, normal and with
-- `fg_system`'s `0x20`; both are 300.
local LIFE_EXTRA <const> = 300

-- `fg_room`'s bits that room 40A's script raises.
local ROOM_BANK <const> = 5

-- The gas tank: room 40A's `obj_model_set` 2.
local TANK_OBJECT <const> = 2

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function routine_word(e)
    return e:routine(0) | e:routine(1) << 8 | e:routine(2) << 16
end

-- `Routine_0` to `Routine_3` from a word, as the original writes them.
local function set_routines(e, word)
    e:routines(word & 0xFF, (word >> 8) & 0xFF, (word >> 16) & 0xFF, (word >> 24) & 0xFF)
end

-- `Mot_play_init`: a change of routines starts the next clip afresh.
local function mot_play_init(e)
    local now = routine_word(e)
    if now ~= e:var(TMP) then
        e:set_move_no(0xFF)
    end
    e:set_var(TMP, now)
end

-- `Mot_play_sp`, on its own clips: the clip from its start if it isn't
-- playing, the frame's speed, and the step if the clip hasn't ended.
local function mot_play_sp(e, clip, hokan)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    e:spd()
    if e:joint(0x1000 // (hokan + 1)) then
        return 1
    end
    e:add_speed_xz()
    return 0
end

local function count_down(e, index)
    local value = e:var(index)
    if value ~= 0 then
        e:set_var(index, value - 1)
    end
end

-- `FUN_00468a10`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_life(LIFE + LIFE_EXTRA)
    e:set_neck_flags(0)
    e:clip(1, 0, 0)
    e:joint(0x100)
    -- `Atd[0]`, the head's box, 3000 ahead of the body; `Atd[1]` and
    -- `Atd[2]`, the body's, aren't modelled.
    e:set_body_box(0, 600, 2000, 600)
    e:set_body_ofs_x(3000)
    for index = FLAGS, BLOOD do
        e:set_var(index, 0)
    end
    e:set_be_flg(e:be_flg() | 0x1C00004C)
end

-- `FUN_00469430`: the player's touch starts the jaws' snap.
local function snap_ck(e)
    local tank = e:var(TANK)
    if e:player_touched() and not e:status(0x10) and e:pl_damage_cnt() == 0
        and e:var(SNAP) == 0 and e:neck_flags() & 4 == 0 and (tank == 0 or tank == 7) then
        e:set_var(SNAP, 1)
    end
end

-- `FUN_00468e20`: the corridor's legs, and the swim's start and stop.
local function swim_ck(e)
    snap_ck(e)
    local x, y, z = e:pos()
    local leg = e:var(LEG)
    if leg == 0 and x < -0x45D5 then
        e:set_var(LEG, 1)
        e:set_var(SHIFT_Z, 0)
        e:set_var(SHIFT_X, -0x45D5 - x)
        set_routines(e, 0x301)
        return
    end
    if leg == 1 then
        if z > -0x1C9B then
            e:set_var(LEG, 2)
            e:set_var(SHIFT_X, 0)
            e:set_var(SHIFT_Z, z + 0x1C9B)
            set_routines(e, 0x301)
            return
        end
        if e:var(GRABBED) ~= 0 then
            return
        end
        local _, _, tank_z = e:object_pos(TANK_OBJECT)
        if e:flag(ROOM_BANK, 1) and e:var(TANK) < 2 then
            e:set_neck_flags(e:neck_flags() | 4)
            e:set_var(NECK_X, x)
            e:set_var(NECK_Y, y - 800)
            e:set_var(NECK_Z, tank_z)
        end
        if tank_z - z - 9000 < -0x78 and e:var(TANK) == 0 then
            e:set_neck_flags(e:neck_flags() & 0xFB)
        end
        if e:flag(ROOM_BANK, 0) then
            local ahead = tank_z - z
            if ahead > 8999 or ahead < 0x2199 or e:var(TANK) ~= 0 then
                return
            end
            e:set_var(TANK, 1)
            if e:routine(1) == 2 then
                set_routines(e, 0x101)
            end
            return
        end
    end
    local tank = e:var(TANK)
    if tank == 0 or tank == 7 then
        if e:routine(1) == 1 then
            if e:var(GRABBED) ~= 0 then
                set_routines(e, 0x201)
            end
            if e:var(REST) == 0 and e:distance() > 100 then
                set_routines(e, 0x201)
            end
        elseif e:var(GRABBED) == 0 then
            if e:distance() < 0x5A or e:var(REST) ~= 0 then
                set_routines(e, 0x101)
            end
        end
    end
end

-- `FUN_00468d30`: the entry, once the room's scene raises `Sce_free0`.
local function enter(e)
    local sub = e:routine(2)
    if sub == 0 then
        if e:sce_free(0) ~= 0 then
            e:set_routine(2, 1)
        end
    elseif sub == 1 then
        e:set_status(0x110)
        e:set_var(FLAGS, e:var(FLAGS) | 2)
        if e:move_cnt() == 0x46 then
            e:cut_change(0)
        end
        if mot_play_sp(e, 6, 0) ~= 0 then
            e:cut_change(1)
            local x, y = e:pos()
            e:set_pos(x, y, -0x52DC)
            e:set_routine(2, 2)
        end
    elseif sub == 2 then
        if mot_play_sp(e, 3, 0) ~= 0 then
            e:clear_status(0x110)
            e:set_angle((e:angle() + 0x400) & 0xFFFF)
            e:set_var(REST, 0x6E)
            e:set_var(FLAGS, (e:var(FLAGS) & 0xFFFD) | 4)
            set_routines(e, 0x101)
        end
    end
end

-- `FUN_00469010`.
local function rest(e)
    if e:routine(2) == 0 then
        mot_play_sp(e, 0, 0x20)
    end
end

-- `FUN_00469030`.
local function swim(e)
    if e:routine(2) == 0 and mot_play_sp(e, 1, 4) ~= 0 then
        set_routines(e, 0x201)
    end
end

-- Both clips go twice as fast once its reach has caught the player.
local function play_fast(e, clip, hokan)
    local ended = mot_play_sp(e, clip, hokan)
    if e:var(GRABBED) ~= 0 then
        ended = ended + mot_play_sp(e, clip, hokan)
    end
    return ended
end

-- `FUN_00469060`: a quarter turn at a corner of the corridor.
local function turn(e)
    snap_ck(e)
    local sub = e:routine(2)
    if sub == 0 then
        -- The sound, and the turn's first step on the same tick.
        e:sound(4)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub == 1 then
        local ended = play_fast(e, 2, 0x10)
        e:set_var(TURNING, 1)
        if ended ~= 0 then
            e:set_routine(2, 2)
        end
    elseif sub == 2 then
        local ended = play_fast(e, 3, 0)
        if e:var(LEG) == 2 and e:move_no() == 3 and e:move_cnt() > 0x71 then
            e:set_var(FLAGS, e:var(FLAGS) | 0x10)
        end
        if ended ~= 0 then
            e:set_angle((e:angle() + 0x400) & 0xFFFF)
            e:set_var(TURNING, 0)
            if e:var(LEG) == 2 then
                set_routines(e, 0x701)
                e:set_var(HIT_WAIT, 0xFF)
            else
                set_routines(e, 0x101)
            end
        end
    end
    local x, y, z = e:pos()
    local shift_x = i16(e:var(SHIFT_X))
    if shift_x > 0 then
        x = x + 6
        e:set_var(SHIFT_X, shift_x - 6)
    end
    local shift_z = i16(e:var(SHIFT_Z))
    if shift_z > 0 then
        z = z - 6
        e:set_var(SHIFT_Z, shift_z - 6)
    end
    e:set_pos(x, y, z)
end

-- `FUN_004693b0`: at the corridor's end it rests, then goes for the player.
local function corner_wait(e)
    local sub = e:routine(2)
    if sub == 0 then
        snap_ck(e)
        e:set_var(HIT_WAIT, 0xFF)
        mot_play_sp(e, 0, 0x20)
        e:set_var(FLAGS, e:var(FLAGS) | 0x10)
        if e:move_cnt() > 0x42 then
            e:set_routine(2, 1)
        end
    elseif sub == 1 then
        local flags = (e:var(FLAGS) & 0xFFEF) | 2
        e:set_var(FLAGS, flags)
        if e:player_life() < 0 then
            set_routines(e, 0x601)
            return
        end
        set_routines(e, 0x401)
        e:set_var(FLAGS, flags | 4)
    end
end

-- `funcs_468CEB` and `funcs_468D18`, by `Routine_1`.
local BEFORE <const> = { [1] = swim_ck, [2] = swim_ck }
local AFTER <const> = { [0] = enter, rest, swim, turn, nil, nil, nil, corner_wait }

-- `FUN_00468ce0`.
local function move(e)
    local before = BEFORE[e:routine(1)]
    if before then
        before(e)
    end
    mot_play_init(e)
    local after = AFTER[e:routine(1)]
    if after then
        after(e)
    end
end

-- `FUN_00469c00`: a clip's `0x28000000` frame shakes the camera.
local function shake(e)
    local step = e:var(SHAKE)
    if step == 1 then
        e:set_var(TIMER2, 2)
        step = 2
        e:set_var(SHAKE, 2)
    end
    if step == 2 then
        e:set_var(TIMER2, e:var(TIMER2) - 1)
        if e:var(TIMER2) == 0 then
            e:set_var(SHAKE, 3)
        end
        return
    end
    if step == 3 then
        e:set_var(TIMER2, 5)
        e:set_status(4)
        step = 4
        e:set_var(SHAKE, 4)
    end
    if step == 4 then
        e:set_var(TIMER2, e:var(TIMER2) - 1)
        if e:var(TIMER2) == 0 then
            e:set_var(TIMER2, 10)
            e:clear_status(4)
            e:set_status(8)
            e:set_var(SHAKE, 5)
        end
        return
    end
    if step == 5 then
        e:set_var(TIMER2, e:var(TIMER2) - 1)
        if e:var(TIMER2) == 0 then
            e:clear_status(8)
            e:set_var(SHAKE, 0)
        end
    end
end

-- `FUN_00469b80` and `FUN_00469bc0`: a move of `total` over `count` steps,
-- speeding up or slowing down, this step's share.
-- C's division, toward 0, for the easings' negative totals.
local function cdiv(a, b)
    local q = a // b
    if q < 0 and q * b ~= a then
        q = q + 1
    end
    return q
end

-- `FUN_00469b80` and `FUN_00469bc0`: this step's share of a move of `total`
-- over `count` steps, speeding up or slowing down.
local function ease_in(count, step, total)
    if step == 0 then
        return 0
    end
    local square = count * count
    return cdiv(step * step * total, square) - cdiv((step - 1) * (step - 1) * total, square)
end

local function ease_out(count, step, total)
    if step == 0 then
        return total
    end
    step = count - step
    local square = count * count
    return cdiv((step + 1) * (step + 1) * total, square) - cdiv(step * step * total, square)
end

local function add_var16(e, index, value)
    e:set_var(index, i16(e:var(index) + value))
end

-- `FUN_0046b1c0`: three ticks of blood about the player's head.
local function blood(e)
    local count = e:var(C231)
    if count == 0 then
        e:set_var(BLOOD, 1)
    elseif count <= 3 then
        local px, py, pz = e:player_pos()
        local dir = e:direction_from_player()
        local i8 = function(v) v = v & 0xFF; return v >= 0x80 and v - 0x100 or v end
        local bx = i8(e:rnd()) + px
        local by = i8(e:rnd()) - 800 + py
        local bz = i8(e:rnd()) + pz
        local turn = i8(e:rnd()) * 0x10 + dir
        e:call_effect(0, 1, 0x2000, i16(turn), bx, by, bz)
        if count == 3 then
            e:set_var(BLOOD, 0)
            e:set_var(C231, 4)
            return
        end
    else
        return
    end
    e:set_var(C231, count + 1)
end

-- `FUN_00469630`: the jaws' snap throws the player back and hurts them.
local function knock(e)
    if not e:player_touched() or e:pl_damage_cnt() ~= 0 then
        return
    end
    e:pl_hold()
    local one = e:cdir_player() and 3 or 2
    e:pl_set_routine(5, one, 0, 0)
    local x, _, z = e:pos()
    local dir
    if z < -8999 then
        if x > -0x3CF0 then
            dir = e:cdir_player() and 0x800 or 0
        else
            dir = e:cdir_player() and 0xC00 or 0x400
        end
    else
        dir = e:cdir_player() and 0 or 0x800
    end
    e:player_goto_angle(dir, 0x600)
    -- `hard_mode` and `censorship_off` are clear: 40 before `Pl_life_down`'s
    -- own scaling.
    local down = 0x28
    local result = e:pl_life_down(down)
    if result & 1 ~= 0 then
        e:pl_set_routine_at(2, 4)
    end
    if result & 2 ~= 0 then
        e:pl_set_routine(3, 0, 0, 0)
    end
    e:set_var(REST, 0x46)
    e:set_var(C231, 0)
    blood(e)
end

-- `FUN_00469480`: the jaws open over four ticks and shut over four, then
-- snap over three more (the knock) and settle over three.
local function snap(e)
    local step = e:var(SNAP)
    if step == 1 then
        e:set_var(TIMER0, 0)
        step = 2
        e:set_var(SNAP, 2)
    end
    local t
    if step == 2 or step == 3 then
        t = e:var(TIMER0) + 1
        e:set_var(TIMER0, t)
        local ease = step == 2 and ease_in or ease_out
        add_var16(e, JAW_B, ease(4, t, 0x14))
        add_var16(e, JAW_A, ease(4, t, -100))
        if t == 4 then
            e:set_var(TIMER0, 0)
            e:set_var(SNAP, step + 1)
        end
    elseif step == 4 or step == 5 then
        t = e:var(TIMER0) + 1
        e:set_var(TIMER0, t)
        local ease = step == 4 and ease_in or ease_out
        add_var16(e, JAW_B, ease(3, t, -0x14))
        add_var16(e, JAW_A, ease(3, t, 100))
        if t == 3 then
            e:set_var(TIMER0, 0)
            e:set_var(SNAP, step == 4 and 5 or 0)
        end
        knock(e)
    end
end

-- `FUN_00469750`: the player within reach of part 0 is caught, and dies.
local function reach_ck(e)
    if e:pl_damage_cnt() ~= 0 then
        return
    end
    local x, _, z = e:own_part(0)
    local px, _, pz = e:player_pos()
    if e:hypot(px - x, pz - z) >= REACH then
        return
    end
    e:set_var(FLAGS, e:var(FLAGS) | 2)
    e:set_var(GRABBED, 8)
    e:pl_hold()
    e:pl_set_routine(5, 1, 0, 0)
    e:set_be_flg(e:be_flg() | 0x100A)
    e:pl_or_be_flg(0x1008)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:set_player_life(-1)
end

function update(e)
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        move(e)
    end
    mot_play_init(e)
    snap(e)
    if e:var(FLAGS) & 4 ~= 0 and e:neck_flags() & 4 == 0 then
        e:rot_neck_part(NECK, NECK_RATE, NECK_LIMIT)
    end
    e:set_jaw(e:var(JAW_B), e:var(JAW_A))
    -- On the init's tick, while the room loads, arklay's player isn't placed
    -- yet and the parts haven't been drawn; the original's player is at the
    -- door, out of reach.
    if e:var(FLAGS) & 2 == 0 and zero ~= 0 then
        reach_ck(e)
    end
    local seq = e:now_seq() & 0xF8000000
    if seq == 0x28000000 then
        e:set_var(SHAKE, 1)
    end
    shake(e)
    if seq == 0x48000000 then
        e:set_var(C232, 0x37)
    end
    e:seq_sound()
    count_down(e, C232)
    count_down(e, GRABBED)
    if e:var(BLOOD) ~= 0 then
        blood(e)
    end
    count_down(e, C233)
    count_down(e, HIT_WAIT)
    count_down(e, REST)
end
