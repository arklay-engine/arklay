-- The alligator, `Em_23` (entity id 0x23).
--
-- `Em_23` runs `Routine_0` from `funcs_4687AE` (`0x527e18`): 0 is the init
-- (`FUN_00468a10`), 1 the move (`FUN_00468ce0`) and 2 the hit
-- (`FUN_00469cf0`), of which the shrug, the flinch and the gas tank's blast
-- are ported, but not going under at the corridor's end; the death
-- (3, `FUN_0046a6d0`) isn't ported yet. The
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
local NECK_X <const> = 18 -- `Neck_point_x` to `_z` (`+0x170`), which `Rot_neck` turns the head to.
local NECK_Y <const> = 19
local NECK_Z <const> = 20
local TIMER0 <const> = 21 -- `Timer0` (`+0x158`): the snap's count.
local BLOOD <const> = 22 -- `+0x234`: what runs each tick: 1 `FUN_0046b1c0`, 2 `FUN_0046b670`, 3 `FUN_0046b2b0`.
local TANK_T <const> = 23 -- `+0x15a`: the count of `FUN_004697f0`'s steps.
-- The gas tank's place on part 6, once the jaws hold it: the original's
-- object work 1 `Pos_x` to `_z` with its parent matrix on that part.
local TANK_X <const> = 24
local TANK_Y <const> = 25
local TANK_Z <const> = 26
local HIT_BY <const> = 27 -- `+0x230`: the weapon (`Routine_1`) of the last hit.
local HURT <const> = 28 -- `+0x1d2`: `FUN_00469d40`'s pick of `funcs_469D34`.
local BLOOD_END <const> = 29 -- `DWORD_0068950c`: the shotguns' blood's last tick.
local BLOOD_POINT <const> = 30 -- `DWORD_00689508`: weapon 4's point of `BLOOD_POINTS`.
local BLAST_DIR <const> = 31 -- `DWORD_00689518`: where the blast throws it.
local BLAST_PUSH <const> = 32 -- `WORD_00689522`: what the blast still adds to `Spd_x`.
local DIVE_ENDS <const> = 33 -- `+0x16b`: the ends of clip 1 since the dive.
local LAST_VAR <const> = DIVE_ENDS

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
-- `fg_stop`'s 0x1000000, which holds the player's input.
local STOP_BANK <const> = 2
local STOP_INPUT <const> = 7

-- The gas tank: room 40A's `obj_model_set` 1 (the original's object work 1, `DAT_0098a84c`).
local TANK_OBJECT <const> = 1

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

-- `Mot_play`: as `Mot_play_sp`, but without the clip's speed.
local function mot_play(e, clip)
    if e:move_no() ~= clip then
        e:clip(clip, 0, 0)
    end
    return e:joint(0x1000) and 1 or 0
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
    for index = FLAGS, LAST_VAR do
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

-- Scenario B (`fg_status`'s 0x40000000) after a scenario A that raised
-- `fg_zapping`'s 0x10 (script flag bank 29).
local ZAPPING_BANK <const> = 29
local function zapping(e)
    return e:status(0x40000000) and e:flag(ZAPPING_BANK, 0x10)
end

-- `FUN_004691c0`: at the corridor's end it takes the player under, in cut 7
-- (0xE while zapping); `fg_system`'s `0x100` is
-- `pl_held_kill`, and its `0x4000000`, which starts `Die`, is the end of
-- `pl_hold_back_die`.
local function kill(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_status(0x100)
        e:cut_change(zapping(e) and 0xE or 7)
        e:hide_object(TANK_OBJECT)
        e:set_var(FLAGS, (e:var(FLAGS) | 2) & 0xFFFB)
        e:pl_set_routine(5, 0, 0, 0)
        e:set_player_life(-1)
        e:pl_hold_back_die()
        local _, y, z = e:pos()
        local x = -0x5A26
        e:set_pos(x, y, z)
        e:set_var(JAW_B, 0)
        e:set_var(JAW_A, 0)
        e:set_var(SNAP, 0)
        e:pl_hold()
        e:set_player_xz(x + 0x2993, -0x393)
        e:set_be_flg(e:be_flg() | 0x100A)
        e:pl_or_be_flg(0x1008)
        e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub == 1 then
        mot_play_sp(e, 4, 0)
        if e:move_cnt() == 0xD then
            e:pl_held_kill()
            e:pl_set_base()
            e:set_routine(2, 2)
            e:pl_set_timer(0, 1)
            e:set_player_angle(e:angle())
        end
    elseif sub == 2 then
        e:null_pos_set(0)
        if mot_play_sp(e, 5, 0) ~= 0 then
            e:pl_or_be_flg(0x80000)
            e:set_routine(2, 3)
        end
    elseif sub == 3 then
        e:null_pos_set(0)
        mot_play(e, 0xB)
    end
end

-- `funcs_468CEB` and `funcs_468D18`, by `Routine_1`.
local BEFORE <const> = { [1] = swim_ck, [2] = swim_ck }
local AFTER <const> = { [0] = enter, rest, swim, turn, kill, nil, nil, corner_wait }

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
    -- own scaling, or 20 once a hit has taken it below 25000 (`+0x218`'s
    -- 0x20).
    local down = e:var(FLAGS) & 0x20 ~= 0 and 0x14 or 0x28
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

-- `Approach_one`: `value` a step toward `target`, and not past it.
local function approach_one(value, target, step)
    local below = value < target
    local moved = below and value + step or value - step
    if (moved < target) ~= below or moved == target then
        return target
    end
    return moved
end

-- The jaws' roll this step of `FUN_004697f0`: `ease` of `count` steps to
-- `b` on `+0x22a` and `a` on `+0x228`.
local function tank_jaws(e, ease, count, b, a)
    local t = e:var(TANK_T)
    add_var16(e, JAW_B, ease(count, t, b))
    add_var16(e, JAW_A, ease(count, t, a))
    t = t + 1
    e:set_var(TANK_T, t)
    return t
end

local function tank_approach(e, x_step, y_step)
    if x_step then
        e:set_var(TANK_X, approach_one(e:var(TANK_X), 0x6D6, x_step))
    end
    if y_step then
        e:set_var(TANK_Y, approach_one(e:var(TANK_Y), -0xD2, y_step))
    end
end

local function tank_next(e, done)
    if done then
        e:set_var(TANK_T, 0)
        e:set_var(TANK, e:var(TANK) + 1)
    end
end

-- `FUN_004697f0`: the gas tank (`+0x226`). Once its swim reaches the tank
-- (1), it raises `fg_room`'s 0x1d and opens its jaws over 13 ticks, pushing
-- the tank ahead of it, then takes it in them (3): the tank's place is kept
-- on part 6 from then, and the jaws close (3, 4) and open a little (5, 6) as
-- it settles 1750 ahead and 210 up, which it keeps (7).
local function tank(e)
    local step = e:var(TANK)
    if step == 1 then
        e:set_var(TANK_T, 0)
        e:set_flag(ROOM_BANK, 0x1D, true)
        step = 2
        e:set_var(TANK, 2)
    end
    if step == 2 then
        local t = tank_jaws(e, ease_in, 0xD, 0xF0, -0x140)
        local _, _, z = e:pos()
        local tx, ty, tz = e:object_pos(TANK_OBJECT)
        if tz - z < 0x1FA4 then
            tz = z + 0x1FA4
            e:set_object_pos(TANK_OBJECT, tx, ty, tz)
        end
        if t == 0xD then
            e:set_var(TANK_T, 0)
            e:set_neck_flags(e:neck_flags() & 0xFB)
            local lx, ly, lz = e:part_local(6, tx, ty, tz)
            e:set_var(TANK_X, lx)
            e:set_var(TANK_Y, ly)
            e:set_var(TANK_Z, lz)
            e:set_var(TANK, 3)
        end
    elseif step == 3 then
        local t = tank_jaws(e, ease_in, 6, -0x8C, -0x3C)
        tank_approach(e, 0x46, nil)
        tank_next(e, t == 6)
    elseif step == 4 then
        local t = tank_jaws(e, ease_out, 6, -0x8C, -0x3C)
        tank_approach(e, 0x91, 0x19)
        tank_next(e, t == 6)
    elseif step == 5 then
        local t = tank_jaws(e, ease_in, 7, 0x4B, 100)
        tank_approach(e, 100, 0x37)
        tank_next(e, t == 7)
    elseif step == 6 then
        local t = tank_jaws(e, ease_out, 7, 0x4B, 100)
        tank_next(e, t == 7)
    elseif step == 7 then
        tank_approach(e, 100, 0x1E)
    end
end

-- The tank in its jaws, placed on part 6 as the last draw left it.
local function hold_tank(e)
    if e:var(TANK) >= 3 then
        local x, y, z = e:drawn_part_point(6, i16(e:var(TANK_X)), i16(e:var(TANK_Y)), i16(e:var(TANK_Z)))
        e:set_object_pos(TANK_OBJECT, x, y, z)
    end
end

-- `DAT_00527ea0`: where a hit bleeds, on the body's parts: the point on the
-- part, the blood's turn and the part.
local BLOOD_POINTS <const> = {
    [0] = { 2300, -300, 400, -2500, 7 },
    { 1900, -500, 400, -2500, 7 },
    { 1200, -600, 800, -2500, 7 },
    { 500, -200, 1200, -2500, 7 },
    { 1600, -600, 100, -2500, 7 },
    { 1800, -400, -300, -2500, 7 },
    { 2600, 0, -300, -2500, 7 },
    { 800, -400, -900, -2500, 7 },
    { 1700, 500, -900, -2500, 6 },
    { 200, 100, 500, -2500, 4 },
    { 600, 300, 0, -2500, 10 },
    { 700, 0, 1300, -2500, 6 },
    { 1000, 700, 800, -2500, 6 },
    { 2000, 900, -100, -2500, 6 },
    { 2000, 200, 0, -2500, 6 },
    { 3800, 500, 0, -2500, 6 },
    { 3200, 300, 500, -2500, 6 },
    { 3300, 300, 200, -2500, 6 },
    { 3100, 300, -300, -2500, 6 },
    { 900, 500, -1000, -2500, 5 },
    { 600, 1200, 800, -2500, 3 },
    { -400, 800, 200, -2500, 8 },
    { 700, -400, 1200, -2500, 5 },
    { 1300, 0, 1300, -2500, 6 },
}

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

-- `FUN_0046b890`: blood (effect 0, `row`) at a point on `part` as the last
-- draw left it. `blood_censor` is taken as off.
local function blood_at(e, row, size, dir, part, x, y, z)
    local wx, wy, wz = e:drawn_part_point(part, x, y, z)
    e:call_effect(0, row, size, i16(dir), i16(wx), i16(wy), i16(wz))
end

-- `FUN_0046b080`: `count` bloods of row 6 at points of `BLOOD_POINTS`, the
-- first eight or, `wide`, all of them.
local function bleed(e, count, size, wide)
    for _ = 1, count do
        local index = wide and e:rnd() % 0x18 or e:rnd() & 7
        local point = BLOOD_POINTS[index]
        local dir = i8(e:rnd()) * 0x10 + point[4]
        blood_at(e, 6, size, dir, point[5], point[1], point[2], point[3])
    end
end

-- `FUN_0046b110`: weapon 4's blood, near one point a burst keeps.
local function bleed_weapon4(e)
    if e:var(C233) == 0 then
        e:set_var(BLOOD_POINT, e:rnd() & 7)
    end
    e:set_var(C233, 4)
    local point = BLOOD_POINTS[e:var(BLOOD_POINT)]
    local x = e:rnd() * 2 + point[1] - 0x7F
    local y = e:rnd() * 2 + point[2] - 0x7F
    local z = e:rnd() * 2 + point[3] - 0x7F
    blood_at(e, 6, 0x2800, point[4], point[5], i16(x), i16(y), i16(z))
end

-- `FUN_0046b670`: the shotguns' blood, two a tick over the ticks after the
-- hit until `+0x231` reaches `last`.
local function bleed_shotgun(e, last)
    if e:var(C231) == 0 then
        e:set_var(BLOOD, 2)
        e:set_var(C231, 1)
        e:set_var(BLOOD_END, last)
        return
    end
    bleed(e, 1, 0x3000, true)
    bleed(e, 1, 0x3000, true)
    if e:var(C231) == e:var(BLOOD_END) then
        e:set_var(BLOOD, 0)
    end
    e:set_var(C231, (e:var(C231) + 1) & 0xFF)
end

-- `FUN_0046b540`: four of effect `kind`'s `row` about the head (part 7),
-- each from a point a little either way across and turned at random.
local HEAD_BURSTS <const> = {
    { 0, -400, 0, 0x2000 },
    { 300, -300, 900, 0x2000 },
    { -600, -200, -1000, 0x2000 },
    { -400, -400, 200, 0x2800 },
}

local function head_burst(e, kind, row)
    for _, point in ipairs(HEAD_BURSTS) do
        local x = i8(e:rnd()) * 2 + point[1]
        local dir = i8(e:rnd()) << 4
        local wx, wy, wz = e:drawn_part_point(7, x, point[2], point[3])
        e:call_effect(kind, row, point[4], i16(dir), i16(wx), i16(wy), i16(wz))
    end
end

-- `FUN_0046af30`: a hit's blood, by the weapon. The blasts' rows also take
-- the colours of 0x17 parts down to 0x60 (`FUN_0046b840`), which isn't
-- ported, and the spark shot's (0xE, `FUN_0046b750`) isn't either.
local function hit_blood(e)
    e:set_var(C231, 0)
    local weapon = e:var(HIT_BY)
    if weapon == 2 or weapon == 3 or weapon == 5 or weapon == 6 or weapon == 0xD or weapon == 0xF then
        bleed(e, 1, 0x2800, false)
    elseif weapon == 4 then
        bleed_weapon4(e)
    elseif weapon == 7 then
        bleed_shotgun(e, 4)
    elseif weapon == 8 then
        bleed_shotgun(e, 6)
    elseif weapon == 9 or weapon == 0xA or weapon == 0x10 or weapon == 0x11 or weapon == 0x13 then
        head_burst(e, 5, 4)
    elseif weapon == 0xB then
        head_burst(e, 4, 9)
    elseif weapon == 0x12 then
        bleed(e, 4, 0x2800, false)
    end
end

-- `FUN_00469d40`: what a hit does. While the player lives: with the tank in
-- its jaws, anything but the knife sets it off (1); at the corridor's end in
-- cut 8, hurt below 25000, it goes under (2, or 3 while zapping); otherwise a
-- quarter of hits, not while it turns or soon after the last, make it flinch
-- (4). The rest it shrugs off (0).
local function hurt_kind(e)
    if e:life() < 25000 then
        e:set_var(FLAGS, e:var(FLAGS) | 0x20)
    end
    if e:player_life() >= 0 then
        if e:var(TANK) == 7 then
            if e:var(HIT_BY) ~= 1 then
                return 1
            end
        elseif e:var(FLAGS) & 0x10 ~= 0 and e:cut() == 8 and e:life() < 25000 then
            e:set_var(SNAP, 0)
            e:set_var(JAW_B, 0)
            e:set_var(JAW_A, 0)
            return zapping(e) and 3 or 2
        elseif e:var(TURNING) == 0 and e:var(HIT_WAIT) == 0 and e:rnd() & 3 == 0 then
            e:set_var(HIT_WAIT, (e:rnd() & 0x3F) + 0x78)
            return 4
        end
    end
    return 0
end

-- `FUN_00469e20`: it bleeds and carries on with what it was doing, this tick.
local function shrug(e)
    if e:var(C232) == 0 and e:var(C233) == 0 then
        e:sound(5)
    end
    hit_blood(e)
    e:restore_routine()
    if e:routine(0) == 1 then
        local before = BEFORE[e:routine(1)]
        if before then
            before(e)
        end
        local after = AFTER[e:routine(1)]
        if after then
            after(e)
        end
    end
end

-- `FUN_0046a650`: it flinches (clip 10), then rests.
local function flinch(e)
    local sub = e:routine(2)
    if sub == 0 then
        if e:var(C232) == 0 then
            e:sound(0)
        end
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        hit_blood(e)
        e:set_routine(2, 1)
    elseif sub ~= 1 then
        return
    end
    if mot_play_sp(e, 10, 3) ~= 0 then
        e:routines(1, 1, 0, 0)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
    end
end

-- An effect at a point on `part`, as the last draw left it (`Esp_call` on
-- the part's `Workm`).
local function effect_at(e, kind, row, size, dir, part, x, y, z)
    local wx, wy, wz = e:drawn_part_point(part, x, y, z)
    e:call_effect(kind, row, size, i16(dir), i16(wx), i16(wy), i16(wz))
end

-- `FUN_0046b2b0`: the blast's blood and fire from the jaws and the head, a
-- burst a tick for its first eight ticks; then it counts to 0x40 and stops.
local function blast_blood(e)
    local count = e:var(C231)
    if count == 0 then
        e:set_var(BLOOD, 3)
        e:set_var(C231, 1)
        return
    end
    if count <= 8 then
        if count == 1 then
            local x = i8(e:rnd()) * 2 + 300
            local y = i8(e:rnd()) * 2
            local z = i8(e:rnd()) * 4 + 200
            effect_at(e, 0x26, 3, 0x2000, 0, 6, x, y, z)
        end
        local x = i8(e:rnd()) * 4 + 300
        local y = i8(e:rnd()) * 2
        local z = i8(e:rnd()) * 2 + 200
        effect_at(e, 0, 6, 0x4000, i8(e:rnd()) << 4, 7, x, y, z)
        x = i8(e:rnd()) * 4 + 300
        y = i8(e:rnd()) * 2
        z = i8(e:rnd()) * 2 + 200
        effect_at(e, 0, 3, 0x4000, i8(e:rnd()) << 4, 7, x, y, z)
        x = i8(e:rnd()) * 2 + 300
        y = i8(e:rnd()) * 2
        z = i8(e:rnd()) * 4 + 200
        effect_at(e, 8, 2, 0x4000, i8(e:rnd()) << 4, 6, x, y, z)
        effect_at(e, 8, 2, 0x4000, i8(e:rnd()) << 4, 6, x, y, z)
    end
    if count == 0x40 then
        e:set_var(BLOOD, 0)
    end
    e:set_var(C231, (count + 1) & 0xFF)
end

-- `FUN_0046abd0`: the tank's six blasts (effect 3, row 0xF), placed on the
-- tank as if it weren't turned.
local TANK_BLASTS <const> = {
    { 0x2000, 0, -0xAF0, 0 },
    { 0x3000, 300, -0x834, 0 },
    { 0x3000, 0, -0x2BC, 0 },
    { 0x2000, 0, 0, 300 },
    { 0x2000, 0, -0x578, -200 },
    { 0x3000, -500, -400, 200 },
}
local function tank_blasts(e)
    local tx, ty, tz = e:object_pos(TANK_OBJECT)
    for _, blast in ipairs(TANK_BLASTS) do
        e:call_effect(3, 0xF, blast[1], 0, i16(tx + blast[2]), i16(ty + blast[3]), i16(tz + blast[4]))
    end
end

-- `FUN_00469e90`: the tank goes off in its jaws. It dies (`Em_dead_flg_on`),
-- is thrown back on clip 9 a little to the side of where it faced, its head
-- blown and the camera shaken, and lies dead; `fg_room`'s 0x1e is raised.
local function explode(e)
    local sub = e:routine(2)
    if sub == 2 then
        e:set_var(FLAGS, e:var(FLAGS) & 0xFB)
        e:set_routine(2, 3)
        return
    elseif sub == 3 then
        e:set_kind(e:kind() | 0x4000)
        e:set_flag(ROOM_BANK, 0x1E, true)
        e:set_var(JAW_B, 0)
        e:set_var(JAW_A, 0)
        e:set_var(SNAP, 0)
        e:clear_status(0x100)
        return
    elseif sub == 0 then
        e:sound(4)
        e:set_died()
        e:set_status(0x100)
        local x, _, z = e:pos()
        local cut = e:cut()
        if (cut == 6 or cut == 0xD) and z < -0x33F4 then
            e:set_var(FLAGS, e:var(FLAGS) | 0x40)
            e:pl_or_be_flg(0x80400)
            e:cut_change(5)
        else
            e:set_var(FLAGS, e:var(FLAGS) & 0xBF)
        end
        e:set_status(4)
        e:set_var(FLAGS, e:var(FLAGS) | 2)
        e:set_var(SHAKE, 1)
        e:set_var(TANK, 0xD)
        e:set_var(JAW_B, 0)
        e:set_var(JAW_A, 0)
        e:set_var(SNAP, 0)
        e:set_be_flg(e:be_flg() | 0x80A)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        -- The tank's `Workm` back to the world's, and `Om_explosion_set`;
        -- the head's `FUN_0046b700`. The parts they throw aren't ported.
        blast_blood(e)
        tank_blasts(e)
        e:set_var(BLAST_DIR, (e:angle() + 0xA0) & 0xFFFF)
        -- `Timer3`: the ticks the burst tank is still drawn.
        e:set_timer(3, 9)
        local angle = e:angle()
        if angle == 0xC00 then
            if z > -3000 then
                z = -3000
            end
            if e:move_no() == 2 then
                angle = (e:move_cnt() << 9) // 0x96 + 0xC00
            elseif e:move_no() == 3 then
                angle = angle + (e:move_cnt() << 9) // 0x96 + 0x200
            end
            e:set_angle(angle & 0xFFFF)
        end
        if x > -0x5AFF then
            z = z - 1000
        end
        local _, y = e:pos()
        e:set_pos(x, y, z)
        e:set_routine(2, 1)
        e:set_var(BLAST_PUSH, 0x28)
        e:set_speed_x(-500)
    elseif sub ~= 1 then
        return
    end
    if mot_play_sp(e, 9, 0) ~= 0 then
        if e:var(FLAGS) & 0x40 ~= 0 then
            e:set_var(FLAGS, e:var(FLAGS) & 0xBF)
            e:cut_change(zapping(e) and 0xD or 6)
            e:pl_clear_be_flg(0x80400)
        end
        e:set_routine(2, e:routine(2) + 1)
    end
    if e:move_cnt() == 0x1F then
        e:set_status(4)
        e:set_var(SHAKE, 1)
    end
    if e:timer(3) < 1 then
        e:hide_object(TANK_OBJECT)
    else
        e:set_timer(3, e:timer(3) - 1)
    end
    local push = e:var(BLAST_PUSH)
    e:set_speed_x(i16(e:speed_x() + push))
    if push ~= 0 then
        e:set_var(BLAST_PUSH, push - 2)
    end
    e:add_speed_xz_at(0x800)
    local x, y, z = e:pos()
    if e:var(BLAST_DIR) == 0xCA0 then
        if x ~= -0x64C8 then
            x = approach_one(x, -0x64C8, 200)
        end
    elseif z ~= 600 then
        z = approach_one(z, 600, 200)
    end
    e:set_pos(x, y, z)
    e:goto_angle(e:var(BLAST_DIR), 0x4B)
end

-- `FUN_0046a2a0`: hurt at the corridor's end, it dives. It waits out the
-- camera's `fg_status` 0x80, turns a quarter (or, while it turns, is put at
-- the end, facing back), and plays clip 7 in cut 9, then clip 1 five times;
-- after the fifth it is back in cut 8 and gone (`Type`'s 0x4000). The
-- player's input is held throughout (`fg_stop`'s 0x1000000). The splash's
-- sound (`0x20e0001`, the room's bank) isn't ported.
local function dive(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_flag(STOP_BANK, STOP_INPUT, true)
        e:set_status(0x110)
        e:set_var(FLAGS, e:var(FLAGS) | 2)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:set_routine(2, 1)
        return
    end
    if sub == 1 then
        if e:status(0x80) then
            return
        end
        e:pl_or_be_flg(0x80400)
        e:cut_change(9)
        if e:var(TURNING) == 0 then
            e:set_angle((e:angle() - 0x400) & 0xFFFF)
        else
            local _, y = e:pos()
            e:set_pos(-0x4E9D, y, -0x388)
            e:set_angle(0xC00)
        end
        e:set_routine(2, 2)
        sub = 2
    end
    if sub == 2 then
        if mot_play_sp(e, 7, 0) ~= 0 then
            e:set_var(DIVE_ENDS, 0)
            e:set_routine(2, 3)
        end
    elseif sub == 3 then
        local ends = (e:var(DIVE_ENDS) + mot_play_sp(e, 1, 7)) & 0xFF
        e:set_var(DIVE_ENDS, ends)
        if ends == 4 then
            e:set_var(FLAGS, e:var(FLAGS) | 8)
        elseif ends == 5 then
            e:set_routine(2, 4)
            e:cut_change(8)
            e:clear_status(0x110)
            e:set_flag(STOP_BANK, STOP_INPUT, false)
            e:set_kind(e:kind() | 0x4000)
            e:pl_clear_be_flg(0x80400)
            mot_play_sp(e, 8, 0)
        end
    end
end

-- `FUN_0046a430`: the dive while zapping. It dies (`Em_dead_flg_on`) and
-- turns its head toward the corridor's end, holds the player's input, waits
-- while the player aims, is put at the end if it was turning, and plays clip
-- 8 in cut 0xE while it eases to `z` 0. Then it is back in cut 8, gone
-- (`Type`'s 0x4000), and stays in routine 1,6, which does nothing. Its new
-- boxes (`Atd` 1 and 3) aren't ported.
local function zap_dive(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_died()
        e:set_flag(STOP_BANK, STOP_INPUT, true)
        e:set_status(0x110)
        e:set_var(FLAGS, e:var(FLAGS) | 2)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:set_routine(3, 0)
        e:set_neck_flags(e:neck_flags() | 4)
        e:set_var(NECK_X, -0x2420)
        e:set_var(NECK_Y, -0x708)
        e:set_var(NECK_Z, 0x3CA)
        e:set_routine(2, 1)
        sub = 1
    end
    if sub == 1 then
        if e:status(0x80) then
            return
        end
        if e:var(TURNING) ~= 0 then
            local _, y = e:pos()
            e:set_pos(-0x4E9D, y, -0x388)
            e:set_angle(0x1000)
        end
        e:pl_or_be_flg(0x80400)
        e:cut_change(zapping(e) and 0xE or 7)
        e:set_routine(2, 2)
        sub = 2
    end
    if sub ~= 2 then
        return
    end
    if e:routine(3) == 0 then
        local x, y, z = e:pos()
        z = approach_one(z, 0, 0x14)
        e:set_pos(x, y, z)
        if z == 0 then
            e:set_routine(3, 1)
        end
    end
    if mot_play_sp(e, 8, 0) ~= 0 then
        e:set_kind(e:kind() | 0x4000)
        e:cut_change(8)
        e:clear_status(0x110)
        e:set_flag(STOP_BANK, STOP_INPUT, false)
        e:pl_clear_be_flg(0x80400)
        set_routines(e, 0x601)
    end
end

-- `funcs_469D34`, by `FUN_00469d40`'s pick.
local HURTS <const> = { [0] = shrug, explode, dive, zap_dive, flinch }

-- `FUN_00469cf0`: `Routine_0` 2.
local function hurt(e)
    if e:routine(1) ~= 0xFF then
        e:set_var(HIT_BY, e:routine(1))
        e:set_routine(1, 0xFF)
        e:set_var(HURT, hurt_kind(e))
        if e:move_cnt() > 1 then
            e:set_move_cnt(e:move_cnt() - 2)
        end
    end
    local entry = HURTS[e:var(HURT)]
    if entry then
        entry(e)
    end
end

function update(e)
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        move(e)
    elseif zero == 2 then
        hurt(e)
    end
    mot_play_init(e)
    tank(e)
    snap(e)
    if e:var(FLAGS) & 4 ~= 0 then
        if e:neck_flags() & 4 == 0 then
            e:rot_neck_part(NECK, NECK_RATE, NECK_LIMIT)
        else
            e:rot_neck_point(NECK, NECK_RATE, NECK_LIMIT, i16(e:var(NECK_X)), i16(e:var(NECK_Y)), i16(e:var(NECK_Z)))
        end
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
    local callback = e:var(BLOOD)
    if callback == 1 then
        blood(e)
    elseif callback == 2 then
        bleed_shotgun(e, e:var(BLOOD_END))
    elseif callback == 3 then
        blast_blood(e)
    end
    count_down(e, C233)
    count_down(e, HIT_WAIT)
    count_down(e, REST)
    hold_tank(e)
end
