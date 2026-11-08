-- The bodies in room 205's walls, `Em_2D` (entity id 0x2D).
--
-- `Em_2D` (`0x483e30`) runs `Routine_0` from `0x529c34`: 0 is the init
-- (`FUN_00483eb0`) and 1 the move (`FUN_00484140`); 2 and 3 are the hits,
-- 4 jumps through a table that is all zero, and 7 is `nullsub`. The move runs
-- a check by `Routine_1`, then an action by `Routine_1` read again, from
-- `0x529c5c`/`0x529c7c` (kind 0) or `0x529c9c`/`0x529cbc` (kind 1), which hold
-- the same entries. The engine runs `Root_ck` before `update`, even while the
-- room is stopped, and `Oba_ck_em` after it. The model holds both kinds'
-- bodies (`Type & 1`): parts 1 to 7 and 8 to 14, and each kind hides the
-- other's.
--
-- Not ported yet: the grab (`Routine_1` 4, `FUN_00484640`) and the player's
-- side of it (`em_damage_table_real[0x1d]`, `FUN_00484a80`), the hits and their
-- blood (`Routine_0` 2 and 3) and the shadow (`Kage_work_set`).

local HOME_ANGLE <const> = 0 -- `+0x218`: `Cdir_y` at the init.
local HOME_X <const> = 1 -- `+0x21c`: `Pos_x` at the init.
local HOME_Z <const> = 2 -- `+0x220`: `Pos_z` at the init.
local SEEN <const> = 3 -- `+0x224`: 1 once it has woken; the next wake groans.

local TYPE_KIND <const> = 0x01 -- which body of the model it shows
local TYPE_LEADER <const> = 0x10 -- it counts the room's globals down

-- `0x529c24 + kind * 7`: the kind's own parts, as a mask of hidden parts.
local OWN_PARTS <const> = { [0] = 0x00FE, 0x7F00 }
local ALL_PARTS <const> = 0x7FFE
-- `0x529c26 + kind * 7`: the part the reach and the grab measure from.
local REACH_PART <const> = { [0] = 3, 10 }
local BE_HIDDEN <const> = 0x2

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function kind(e)
    return e:kind() & TYPE_KIND
end

-- `FUN_00484a10`: its own seven parts shown or hidden, and `Be_flg`'s 2,
-- which `Oba_ck_hit` skips a body on, cleared or set.
local function show(e, on)
    if on then
        e:set_hidden_parts(ALL_PARTS & ~OWN_PARTS[kind(e)])
        e:set_be_flg(e:be_flg() & ~BE_HIDDEN)
    else
        e:set_hidden_parts(ALL_PARTS)
        e:set_be_flg(e:be_flg() | BE_HIDDEN)
    end
end

local function go_home(e)
    local _, y = e:pos()
    e:set_pos(e:var(HOME_X), y, e:var(HOME_Z))
end

-- `FUN_00483eb0`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    e:set_speed(0, 0, 0)
    e:set_timer(0, e:rnd() & 0xF)
    local x, _, z = e:pos()
    e:set_var(HOME_ANGLE, e:angle())
    e:set_var(HOME_X, x)
    e:set_var(HOME_Z, z)
    e:set_var(SEEN, 0)
    e:set_d_life(0x82)
    e:set_life(250)
    e:clear_neck()
    -- The other kind's parts (`Be_flg` 0), then its own hidden.
    show(e, false)
    e:set_room_enemy(0)
    e:set_groan_wait(0)
    e:set_be_flg(e:be_flg() | 0x0C000000)
    e:set_body_ofs_x(0)
    e:set_body_box(0, 800, 500, 800)
    e:clip(0, 0, 0)
    e:joint(0x100)
    e:set_be_flg(e:be_flg() | 0x1404)
end

-- `FUN_004841c0`: under 2500 and within 0x400 of its facing, once armed, it
-- turns its head; under 3000 and within 0x2f8, armed, it comes out.
local function wake_check(e)
    local d = e:distance()
    local px, _, pz = e:player_pos()
    local near = e:goto_ck(px, pz, 0x400)
    local ahead = e:goto_ck(px, pz, 0x2F8)
    if d < 2500 and near == 0 and e:routine(2) >= 2 then
        e:routines(1, 1, 0, 0)
        show(e, true)
        go_home(e)
    elseif d < 3000 and ahead == 0 and e:routine(2) == 2 then
        e:set_routine(2, 3)
        show(e, true)
    else
        return
    end
    e:set_damage_cnt(0)
    if e:var(SEEN) == 1 then
        if e:groan_wait() == 0 then
            e:sound(4)
            e:set_groan_wait(0x3C)
        end
    else
        e:set_var(SEEN, 1)
    end
end

-- `FUN_004842b0`: back into the wall, the wait until it is armed, and the
-- way out.
local function wait(e)
    local step = e:routine(2)
    if step == 0 then
        e:clip(0, 0, 0xF)
        e:joint(0x100)
        e:set_angle(e:var(HOME_ANGLE))
        e:set_routine(2, 1)
        go_home(e)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:set_speed(400, 0, 0)
        e:add_speed_xz_at(0x800)
    elseif step == 3 then
        -- It goes on with clip 0 from where step 0's frame left it.
        if e:joint(0x100) then
            e:routines(1, 1, 0, 0)
            go_home(e)
            return
        end
        if e:move_cnt() < 0xB then
            e:set_speed(0x32, 0, 0)
            e:add_speed_xz_at(0)
        end
        return
    elseif step ~= 1 then
        return
    end
    local timer = e:timer(0)
    e:set_timer(0, timer - 1)
    if timer == 0 then
        e:set_routine(2, 2)
    end
end

-- `FUN_004843d0`: the player within 900 of its arm.
local function reach_check(e)
    local px, _, pz = e:player_pos()
    local x, _, z = e:own_part(REACH_PART[kind(e)])
    if e:hypot(px - x, pz - z) < 900 then
        e:routines(1, 3, 0, 0)
    end
end

-- `FUN_00484430`: the head turns after the player, within 0x180 of home,
-- until the timer runs out.
local function turn(e)
    local step = e:routine(2)
    if step == 0 then
        e:set_routine(2, 1)
        e:clip(1 + (e:rnd() & 1), 0, 0xF)
        e:set_timer(0, (e:rnd() & 0x1F) + 0x3C)
        if e:rnd() & 1 ~= 0 then
            e:sound((e:rnd() & 1) == 0 and 1 or 0)
        end
    elseif step ~= 1 then
        return
    end
    local px, _, pz = e:player_pos()
    -- The compare is signed 16-bit, on the facing unmasked.
    local angle = (e:angle() + e:goto_ck(px, pz, 0x20)) & 0xFFFF
    local home = i16(e:var(HOME_ANGLE))
    if i16(angle) > home + 0x180 then
        angle = home + 0x180
    end
    if i16(angle) < home - 0x180 then
        angle = home - 0x180
    end
    e:set_angle(angle & 0xFFFF)
    e:joint(0x100)
    local timer = e:timer(0)
    e:set_timer(0, timer - 1)
    if timer == 0 then
        e:routines(1, 5, 0, 0)
    end
end

-- `FUN_00484530`: the player within 600 of its arm, late in the reach, and
-- no other body's grab just ended (`fg_room_enemy`).
local function grab_check(e)
    local px, _, pz = e:player_pos()
    local x, _, z = e:own_part(REACH_PART[kind(e)])
    if e:hypot(px - x, pz - z) < 600 and e:move_cnt() > 4 and e:room_enemy() == 0 then
        local held = e:pl_damage_cnt()
        if held & 0x80 == 0 then
            e:routines(1, 4, 0, 0)
            e:pl_set_damage_cnt(held | 0x80)
        else
            e:routines(1, 6, 0, 0)
        end
    end
end

-- `FUN_004845d0`: the reach, turning after the player.
local function reach(e)
    local step = e:routine(2)
    if step == 0 then
        e:clip(3, 0, 0xF)
        e:set_routine(2, 1)
    elseif step == 2 then
        e:routines(1, 5, 0, 0)
        return
    elseif step ~= 1 then
        return
    end
    e:goto_player(0x30)
    e:step(e:joint(0x100))
end

-- `FUN_00484780`: back into the wall, and the end.
local function retreat(e)
    local step = e:routine(2)
    if step == 0 then
        e:clip(4, 0, 0xF)
        e:set_routine(2, 1)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:sound(6)
    elseif step ~= 1 then
        return
    end
    if e:joint(0x100) then
        e:set_timer(0, (e:rnd() & 0x1F) + 0x1E)
        show(e, false)
        e:routines(1, 7, 0, 0)
        e:set_life(-1)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
    end
    e:set_speed(0x1E, 0, 0)
    e:add_speed_xz_at(0x800)
end

-- `FUN_00484850`: while another body has hold of the player. Each step ends
-- the tick.
local function hold(e)
    local step = e:routine(2)
    if step == 0 then
        e:clip(5, 0, 0xF)
        e:set_routine(2, 1)
    elseif step == 1 then
        e:joint(0x100)
        if e:pl_damage_cnt() & 0x80 == 0 then
            e:set_routine(2, 2)
        end
    elseif step == 2 then
        e:routines(1, 1, 0, 0)
    end
end

-- 4, the grab (`FUN_00484640`), isn't ported yet.
local CHECK <const> = { [0] = wake_check, [1] = reach_check, [3] = grab_check }
local ACTION <const> = { [0] = wait, [1] = turn, [3] = reach, [5] = retreat, [6] = hold }

-- `FUN_00484140`.
local function move(e)
    local check = CHECK[e:routine(1)]
    if check then
        check(e)
    end
    local action = ACTION[e:routine(1)]
    if action then
        action(e)
    end
end

function update(e)
    if e:kind() & TYPE_LEADER ~= 0 then
        local room_enemy = e:room_enemy()
        if room_enemy ~= 0 then
            e:set_room_enemy(room_enemy - 1)
        end
        local groan = e:groan_wait()
        if groan ~= 0 then
            e:set_groan_wait(groan - 1)
        end
    end
    local r0 = e:routine(0)
    if r0 == 0 then
        init(e)
    elseif r0 == 1 then
        move(e)
    end
end
