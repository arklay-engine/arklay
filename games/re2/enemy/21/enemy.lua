-- The crow, `Em_21` (entity id 0x21).
--
-- The whole state machine. The engine runs the animation and collision, and
-- `Oba_ck_em` and `Sca_ck_em` after `update`.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and `e:set_var`
-- are the fields of the original's work that the engine doesn't read.
--
-- A crow blown apart throws its parts as a zombie does, not with the original's
-- two kinds of flight.

local NEXT <const> = 0 -- `+0x218`: the step after the count, or how fast it speeds up.
local COUNT <const> = 1 -- `+0x219`.
local COUNT2 <const> = 2 -- `+0x21a`.
local COUNT3 <const> = 3 -- `+0x21b`.
local NO_ATTACK <const> = 4 -- `+0x21c`.
local AT_WALL <const> = 5 -- `+0x21d`: how long it's been against a wall.
local AT_BODY <const> = 6 -- `+0x21e`: how long it's been against the player.
local AREA <const> = 7 -- `+0x21f`: the guide area it's wandering to.
local AREA_TIME <const> = 8 -- `+0x220`: how long it goes there.
local ANGRY <const> = 9 -- `+0x221`: how long it stays up after a kill.
local FALL <const> = 10 -- `+0x222`: its speed falling, or the height it wants, or `Pos_x`.
local AWAY <const> = 11 -- `+0x224`: where it's thrown, or `Pos_z`.
local ROLL <const> = 12 -- `+0x228`: how it turns over as it falls.
local FLAGS <const> = 13 -- `+0x22a`.

-- `FLAGS`.
local FLAG_FLOWN <const> = 1
local FLAG_SIGHT <const> = 2
local FLAG_OWNER <const> = 4 -- It's the one that goes for the player.
local FLAG_CALLED <const> = 8
local FLAG_RELEASED <const> = 0x10
local FLAG_QUIET <const> = 0x20
local FLAG_BUSY <const> = 0x40 -- Another has the player.
local FLAG_KILLED <const> = 0x80

-- `Be_flg`.
local BE_NO_OBA <const> = 2
local BE_NO_SCA <const> = 8
local BE_PUSHED_Z <const> = 0x4000 -- `Sca_ck_em` moved it along z.
local BE_FLOATING <const> = 0x80000
local BE_HIGH <const> = 0x8000000

local KIND_ANGRY <const> = 0x40
local KIND_SCENE <const> = 0x4000
local ROOM_TAKEN <const> = 1

-- `hard_mode` doubles these, and is off here as it is for the zombies.
local LIFE <const> = 10
local PECK <const> = 5

local PLAYER_BODY <const> = 0xFE
local PERCH <const> = 0xFA
local TOP_SPEED <const> = 300
local BOX <const> = 350
local TOUCHING_SPEED <const> = 0xE6

-- Where `FUN_00462d20`'s calls return to, which its wall check reads as a
-- position.
local WALL_RETURN_4 <const> = 0x4607DD
local WALL_RETURN_5 <const> = 0x460948
local WALL_RETURN_6 <const> = 0x460A78

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

-- A byte's post-decrement: what it held.
local function count_down(e, var)
    local old = e:var(var)
    e:set_var(var, (old - 1) & 0xFF)
    return old
end

-- `FUN_00462d00`.
local function go(e, one)
    e:set_routine(1, one)
    e:set_routine(2, 0)
    e:set_routine(3, 0)
end

-- `FUN_00462ce0`.
local function state(e, zero, one)
    e:routines(zero, one, 0, 0)
end

local function flag(e, bit)
    return e:var(FLAGS) & bit ~= 0
end

local function set_flag(e, bit)
    e:set_var(FLAGS, e:var(FLAGS) | bit)
end

local function clear_flag(e, bit)
    e:set_var(FLAGS, e:var(FLAGS) & ~bit)
end

local function spd_x(e)
    return i16(e:speed_x())
end

local function spd_y(e)
    local _, y = e:speed()
    return i16(y)
end

local function set_spd_y(e, y)
    local x, _, z = e:speed()
    e:set_speed(x, y, z)
end

local function pos_y(e)
    local _, y = e:pos()
    return y
end

-- Where it stands on the floor.
local function floor_y(e)
    return i16(e:ground()) - PERCH
end

local function player_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

local function goto_player(e, rate)
    local x, z = player_xz(e)
    e:goto_point(x, z, rate)
end

local function goto_dest(e, rate)
    local x, z = e:dest()
    e:goto_point(x, z, rate)
end

local function player_ck(e, rate)
    local x, z = player_xz(e)
    return e:goto_ck(x, z, rate)
end

local function dest_ck(e, rate)
    local x, z = e:dest()
    return e:goto_ck(x, z, rate)
end

-- `FUN_004631e0`.
local function box(e, size)
    local ofs_y, _, at_h = e:body_box()
    e:set_body_box(ofs_y, size, at_h, size)
end

-- `FUN_00463160`.
local function feathers(e, count)
    local x, y, z = e:pos()
    for _ = 1, count do
        local at_x = i8(e:rnd()) * 4 + x
        local at_y = i8(e:rnd()) * 4 + y
        local at_z = i8(e:rnd()) * 4 + z
        e:call_effect(0xC, 0, 0x800, e:rnd() << 4, at_x, at_y, at_z)
    end
end

-- `FUN_004630c0`.
local function blood(e)
    local x, y, z = e:pos()
    local at_x = i8(e:rnd()) + x
    local at_y = i8(e:rnd()) + y
    local at_z = i8(e:rnd()) + z
    e:call_effect(0, 1, 0x2800, e:rnd() << 4, at_x, at_y, at_z)
end

-- `FUN_00463070`, which every crow in the room hears unless it's the one on the
-- player.
local function tell_all(e, bits)
    e:call_kin(FLAGS, FLAG_OWNER, bits, AREA_TIME, AREA)
end

local function new_area(e)
    e:set_var(AREA_TIME, e:rnd() & 0x7F)
    e:set_var(AREA, e:rnd_area() & 0xFF)
end

-- `FUN_00462c60`: it stays where its part 11 was before the clip stepped.
local function pin(e, x, z)
    local now_x, _, now_z = e:part_point(11, 0, 0, 0)
    if x and now_x then
        e:add_pos(x - now_x, 0, z - now_z)
    end
end

-- `FUN_004600e0`: at rest on its perch.
local function perched(e)
    local two = e:routine(2)
    if two == 0 then
        local draw = e:rnd()
        e:set_routine(2, 7)
        e:set_var(COUNT, draw % 100)
    elseif two == 1 then
        local draw = e:rnd()
        e:set_var(COUNT, 5)
        e:set_routine(2, 10)
        e:clip(0, draw % 5, 7)
    elseif two == 2 then
        e:clip(1, 0, 7)
        e:set_routine(2, 8)
    elseif two == 3 then
        e:clip(1, 0, 7)
        e:set_routine(2, 9)
    elseif two == 4 then
        e:clip(1, 0, 7)
        e:set_routine(2, 0xB)
    elseif two == 5 then
        local draw = e:rnd()
        e:set_routine(2, 8)
        e:clip(2, draw % 7, 7)
    elseif two == 6 then
        e:clip(10, 0, 7)
        local draw = e:rnd()
        local x, _, z = e:pos()
        e:set_routine(2, 0xD)
        e:set_var(AWAY, z)
        e:set_var(COUNT, draw % 5 + 0xF)
        e:set_var(FALL, x)
    elseif two == 7 then
        if e:var(COUNT) == 0 then
            local choice = e:rnd() % 7
            e:set_var(NEXT, choice)
            if choice == 6 then
                e:set_routine(2, e:rnd() & 0xF ~= 0 and 0 or 6)
            else
                e:set_routine(2, choice)
            end
        end
        e:set_var(COUNT, (e:var(COUNT) - 1) & 0xFF)
    elseif two == 9 then
        if e:joint(0x200, true) then
            e:set_routine(2, 0)
        end
    elseif two == 10 or two == 8 then
        if two == 10 and count_down(e, COUNT) == 0 and not flag(e, FLAG_QUIET) and e:rnd() & 1 ~= 0 then
            e:sound(6)
        end
        if e:joint(0x200) then
            e:set_routine(2, 0)
        end
    elseif two == 0xB then
        if e:joint(0x200) then
            e:set_routine(2, 0xC)
            e:set_var(COUNT, e:rnd() % 100)
        end
    elseif two == 0xC then
        if e:var(COUNT) == 0 then
            e:set_routine(2, 3)
        end
        e:set_var(COUNT, (e:var(COUNT) - 1) & 0xFF)
    elseif two == 0xD then
        local x, _, z = e:part_point(11, 0, 0, 0)
        e:joint(0x200, true)
        pin(e, x, z)
        if e:move_cnt() == e:var(COUNT) then
            e:set_routine(2, 0xE)
            e:clip(10, 0x27 - e:var(COUNT), 7)
        end
    elseif two == 0xE then
        local x, _, z = e:part_point(11, 0, 0, 0)
        if e:joint(0x200) then
            local now_x, _, now_z = e:pos()
            e:add_pos(i16(e:var(FALL)) - now_x, 0, i16(e:var(AWAY)) - now_z)
            e:set_routine(2, 0)
        else
            pin(e, x, z)
        end
    end
end

-- `FUN_00460050`.
local function think_perched(e)
    local distance = e:distance()
    if e:player_life() < 0 and i8(e:var(ANGRY)) <= 0 and distance <= 0x707 then
        return
    end
    local near = distance < 0x709
    local noise = e:noise()
    if noise & 4 ~= 0 and distance < 0x1519 then
        near = true
    end
    if noise & 1 ~= 0 then
        near = true
    end
    local waited = false
    if flag(e, FLAG_FLOWN) then
        local timer = e:timer(0)
        e:set_timer(0, timer - 1)
        waited = timer == 0
    end
    if waited or near then
        go(e, 1)
    end
end

-- `FUN_004603c0`.
local function about_to_fly(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_var(COUNT, e:rnd() & 7)
    elseif two ~= 1 then
        return
    end
    if count_down(e, COUNT) == 0 then
        go(e, e:kind() & KIND_ANGRY ~= 0 and 3 or 2)
    end
end

local function speed_up(e, by)
    if spd_x(e) < TOP_SPEED then
        e:set_speed_x(spd_x(e) + by)
    end
end

-- `FUN_00460420`: up off its perch.
local function take_off(e)
    local two = e:routine(2)
    if two == 0 then
        local frame = e:rnd() & 7
        if e:rnd() & 1 ~= 0 then
            e:sound(1)
        end
        if e:rnd() & 7 == 0 then
            feathers(e, 1)
        end
        e:set_var(AT_WALL, 0)
        e:clip(7, frame, 7)
        local draw = e:rnd()
        set_flag(e, FLAG_FLOWN)
        set_spd_y(e, -10)
        e:set_speed_x((draw & 0x1F) + 100)
        e:set_var(COUNT, 9 - frame)
        e:set_var(NEXT, 1)
    elseif two == 1 then
        e:set_var(COUNT, 6)
        e:set_var(NEXT, 2)
        set_spd_y(e, -0x50)
    elseif two == 2 then
        if e:rnd() & 1 ~= 0 then
            e:sound(1)
        end
        set_spd_y(e, -0x3C)
        e:set_var(COUNT, 4)
        e:set_var(NEXT, 3)
    elseif two == 3 then
        set_spd_y(e, -0x64)
        e:set_var(COUNT, 9)
        e:set_var(NEXT, 4)
    elseif two == 4 then
        if e:rnd() & 7 == 0 then
            feathers(e, 1)
        end
        e:set_var(COUNT, 0xFF)
        set_spd_y(e, -0x50)
    elseif two ~= 5 then
        return
    end
    e:set_routine(2, 5)
    local count = (e:var(COUNT) - 1) & 0xFF
    e:set_var(COUNT, count)
    if count == 0 then
        e:set_routine(2, e:var(NEXT))
    end
    local x, z = player_xz(e)
    if e:rnd() & 1 ~= 0 then
        local at_x, _, at_z = e:pos()
        x, z = x - at_x, z - at_z
    end
    e:goto_point(x, z, 0x20)
    e:add_speed_xyz()
    speed_up(e, 1)
    if e:joint(0x200) then
        go(e, 4)
    end
end

-- `FUN_00460600`: off the perch, already after the player.
local function take_off_angry(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_kind(e:kind() & ~KIND_ANGRY)
        if e:rnd() & 1 ~= 0 then
            e:sound(1)
        end
        local frame = e:rnd() & 7
        e:set_var(AT_WALL, 0)
        e:clip(6, frame, 7)
        local draw = e:rnd()
        set_spd_y(e, 0)
        set_flag(e, FLAG_FLOWN)
        e:set_speed_x((draw & 7) + 0x3C)
        e:set_var(COUNT, 0xC)
        e:set_var(COUNT2, 0)
        e:set_var(NEXT, 1)
    elseif two == 1 then
        e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        local draw = e:rnd()
        set_spd_y(e, 0x50)
        e:set_var(COUNT, 0xFF)
        e:set_speed_x((draw & 7) + 0x50)
        e:set_var(COUNT2, 0x20)
    elseif two == 3 then
        go(e, 4)
        return
    elseif two ~= 2 then
        return
    end
    e:set_routine(2, 2)
    goto_player(e, e:var(COUNT2))
    e:add_speed_xyz()
    speed_up(e, 1)
    if e:joint(0x200) then
        e:set_routine(2, 3)
    end
    if count_down(e, COUNT) == 0 then
        e:set_routine(2, e:var(NEXT))
    end
end

-- `FUN_00462d20`: against a wall.
local function wall_ck(e, return_to)
    local angle = e:angle() & 0xFFF
    if e:sca_info() & 1 == 0 then
        e:set_var(AT_WALL, 0)
        return
    end
    if e:var(AT_WALL) == 0 and spd_x(e) > TOUCHING_SPEED then
        local side = e:wall_side(return_to)
        local along
        if side == 2 then
            along = angle >= 0xF81 or angle <= 0x7F
        else
            along = angle > ((side * 0x400 + 0x780) & 0xFFF)
                and angle < ((side * 0x400 - 0x780) & 0xFFF)
        end
        if along then
            go(e, 10)
        end
    end
    if e:var(AT_WALL) > 10 then
        go(e, e:rnd() & 0x7F == 0 and 7 or 8)
    end
    if spd_x(e) > 100 then
        e:set_speed_x(spd_x(e) - 4)
    end
    e:set_var(AT_WALL, (e:var(AT_WALL) + 1) & 0xFF)
end

-- `FUN_00462e30`: whether to go for the player.
local function attack_ck(e)
    if e:player_life() < 0 then
        return
    end
    local flags = e:var(FLAGS)
    if flags & FLAG_CALLED ~= 0 and flags & FLAG_SIGHT ~= 0 then
        go(e, 0xD)
        e:set_var(FLAGS, flags & 0xB7)
        e:set_var(NO_ATTACK, 0)
        return
    end
    if e:var(NO_ATTACK) ~= 0 then
        e:set_var(NO_ATTACK, e:var(NO_ATTACK) - 1)
        return
    end
    local distance = e:distance()
    local free = e:room_enemy() & ROOM_TAKEN == 0
    if distance < 0x1C20 and distance > 900 and free and flags & FLAG_SIGHT ~= 0 and player_ck(e, 0x100) == 0 then
        e:set_room_enemy(e:room_enemy() | ROOM_TAKEN)
        set_flag(e, FLAG_OWNER)
        go(e, e:rnd() & 0xF == 0 and 0xB or 0xD)
        return
    end
    if e:at_em_no() ~= PLAYER_BODY then
        e:set_var(AT_BODY, 0)
        return
    end
    if e:var(AT_BODY) == 0 and spd_x(e) > TOUCHING_SPEED and free and player_ck(e, 0x100) == 0 then
        go(e, 0xC)
    end
    if e:var(AT_BODY) > 10 then
        go(e, 8)
        new_area(e)
    end
    if spd_x(e) > 100 then
        e:set_speed_x(spd_x(e) - 4)
    end
    e:set_var(AT_BODY, (e:var(AT_BODY) + 1) & 0xFF)
end

-- `FUN_00462fb0`.
local function released_ck(e)
    if flag(e, FLAG_RELEASED) then
        box(e, BOX)
        e:set_var(AREA_TIME, e:rnd() & 0x9D)
        e:set_var(AREA, e:rnd_area() & 0xFF)
        e:set_var(NO_ATTACK, 0x1E)
        go(e, 4)
        e:set_var(FLAGS, e:var(FLAGS) & 0xA7)
    end
end

-- What every state in the air ends its thinking with.
local function think_in_air(e, return_to)
    wall_ck(e, return_to)
    if e:player_life() < 0 and i8(e:var(ANGRY)) < 1 then
        -- `FUN_00463000`.
        if e:distance() < 0x708 then
            go(e, 7)
        end
        return
    end
    attack_ck(e)
    released_ck(e)
end

-- `FUN_00460770`.
local function think_flying(e)
    if e:distance() > 0x1C20 then
        go(e, e:rnd() & 1 == 0 and 8 or 7)
    end
    if e:rnd() & 0xF == 0 then
        e:set_timer(0, e:rnd() & 0x3F)
        go(e, e:player_ground() - 0xE10 < pos_y(e) and 5 or 6)
    end
    think_in_air(e, WALL_RETURN_4)
end

-- `FUN_00460810`.
local function flying(e)
    local two = e:routine(2)
    if two == 0 then
        if e:rnd() & 1 == 0 then
            e:clip(4, 0, 7)
            if e:rnd() & 7 == 0 then
                feathers(e, 1)
            end
            e:set_var(NEXT, 3)
        else
            e:clip(3, 0, 7)
            e:set_var(NEXT, 2)
        end
        local draw = e:rnd()
        set_spd_y(e, 0)
        e:set_var(COUNT, draw & 0x1F)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    goto_dest(e, 0x20)
    e:add_speed_xyz()
    speed_up(e, e:var(NEXT))
    e:joint(0x200)
    if count_down(e, COUNT) == 0 then
        e:set_routine(2, 0)
    end
end

-- `FUN_00460900`, and `FUN_00460a30` the other way.
local function think_climbing(e)
    if pos_y(e) < e:player_ground() - 0x12C0 then
        go(e, 4)
    end
    local timer = e:timer(0)
    e:set_timer(0, timer - 1)
    if timer == 0 then
        go(e, 4)
    end
    think_in_air(e, WALL_RETURN_5)
end

local function think_diving(e)
    if e:player_ground() - 0x708 < pos_y(e) then
        go(e, 4)
    end
    local timer = e:timer(0)
    e:set_timer(0, timer - 1)
    if timer == 0 then
        go(e, 4)
    end
    think_in_air(e, WALL_RETURN_6)
end

-- `FUN_00460980` and `FUN_00460ab0`.
local function changing_height(e, clip, up, every, gain)
    local two = e:routine(2)
    if two == 0 then
        e:set_var(COUNT, 0)
        e:clip(clip, 0, 7)
        set_spd_y(e, up)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    if count_down(e, COUNT) == 0 then
        e:set_var(COUNT, every)
    end
    goto_dest(e, 0x20)
    e:add_speed_xyz()
    speed_up(e, gain)
    e:joint(0x200)
end

local function climbing(e)
    changing_height(e, 4, -40, 9, 2)
end

local function diving(e)
    changing_height(e, 3, 40, 0xF, 1)
end

-- `FUN_00460b60`: down to the floor.
local function landing(e)
    local two = e:routine(2)
    if two == 0 or two == 1 then
        if two == 0 then
            e:clip(3, 0, 7)
            e:set_var(AT_WALL, 0)
            e:set_var(COUNT, 0)
            e:set_speed_x(0x32)
            set_spd_y(e, 0x78)
            e:set_routine(2, 1)
        end
        if count_down(e, COUNT) == 0 then
            if e:rnd() & 1 ~= 0 then
                e:sound(1)
            end
            e:set_var(COUNT, 0xF)
        end
        e:add_speed_xyz()
        if pos_y(e) - i16(e:ground()) + PERCH > -300 then
            e:set_routine(2, 2)
        end
        e:joint(0x200)
        return
    elseif two == 6 then
        goto_dest(e, 0x60)
        e:add_speed_xyz()
        e:set_pos_y(floor_y(e))
        if e:joint(0x200) then
            if e:player_life() < 0 and i8(e:var(ANGRY)) < 1 then
                go(e, 0)
                return
            end
            e:set_timer(0, e:rnd() * 2)
            go(e, 2)
        end
        return
    elseif two == 2 then
        e:clip(10, 0, 7)
        if e:rnd() & 1 ~= 0 then
            e:sound(1)
        end
        e:set_speed_x(0x32)
        set_spd_y(e, 0xF)
        e:set_var(COUNT, 0xF)
        e:set_var(NEXT, 3)
        e:set_routine(2, 5)
    elseif two == 3 then
        if e:rnd() & 1 ~= 0 then
            e:sound(1)
        end
        e:set_var(NEXT, 4)
        e:set_var(COUNT, 5)
        e:set_routine(2, 5)
    elseif two == 4 then
        e:set_speed_x(10)
        set_spd_y(e, 0)
        e:set_pos_y(floor_y(e))
        e:set_var(COUNT, 0xFF)
        e:set_routine(2, 5)
    elseif two ~= 5 then
        return
    end
    local count = (e:var(COUNT) - 1) & 0xFF
    e:set_var(COUNT, count)
    if count == 0 then
        e:set_routine(2, e:var(NEXT))
    end
    e:add_speed_xyz()
    if pos_y(e) > floor_y(e) then
        e:set_pos_y(floor_y(e))
    end
    if e:joint(0x200) then
        e:set_speed_x(10)
        e:set_routine(2, 6)
        e:clip(8, 0, 7)
    end
end

-- `FUN_00460de0`: away from a wall.
local function veering(e)
    local two = e:routine(2)
    if two == 0 then
        if e:rnd() & 7 == 0 then
            feathers(e, 1)
        end
        e:clip(4, 0, 7)
        local draw = e:rnd()
        e:set_var(COUNT2, 0)
        e:set_var(COUNT, draw & 0x3F)
        e:set_var(AT_WALL, 0)
        set_spd_y(e, 0)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    if count_down(e, COUNT2) == 0 then
        e:set_var(COUNT2, 0xE)
    end
    goto_dest(e, 0x60)
    e:set_angle(e:angle() + i8(e:var(NEXT)))
    e:add_speed_xyz()
    e:joint(0x200)
    if dest_ck(e, 0x60) == 0 or count_down(e, COUNT) == 0 then
        go(e, 4)
    end
end

-- `FUN_00460ee0`: on the floor, turning.
local function turning_on_floor(e)
    local two = e:routine(2)
    if two == 0 then
        e:clip(8, 0, 7)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    goto_dest(e, 0x60)
    if e:joint(0x200) then
        e:set_timer(0, e:rnd() & 0xF)
        go(e, 0)
    end
end

-- The fall the wall and the player knock it into: 1 once it's landed.
local function dropping(e)
    local two = e:routine(2)
    if two == 1 then
        if e:joint(0x200) then
            e:set_routine(2, 2)
        end
    end
    if two <= 2 then
        e:set_var(FALL, i16(e:var(FALL) + 0x14))
        local y = pos_y(e) + e:var(FALL)
        e:set_pos_y(y)
        return y > floor_y(e)
    end
    return false
end

local function dropped(e)
    e:sound(5)
    e:set_pos_y(floor_y(e))
    e:clip(0xB, 0, 7)
end

-- `FUN_00460f60`: flown into a wall.
local function hit_wall(e)
    local two = e:routine(2)
    if two == 0 then
        e:sound(3)
        e:clip(9, 0, 7)
        e:set_speed(0, 0, select(3, e:speed()))
        e:set_var(FALL, 0)
        e:set_var(AT_WALL, 0)
        e:set_routine(2, 1)
        two = 1
    end
    if two <= 2 then
        if dropping(e) then
            dropped(e)
            local draw = e:rnd()
            e:set_tilt(0, 0)
            e:set_var(COUNT, draw & 0xF)
            e:set_routine(2, 3)
        end
    elseif two == 3 then
        if count_down(e, COUNT) == 0 then
            e:set_routine(2, 4)
        end
        e:joint(0x200)
        e:set_move_cnt(e:move_cnt() - 1)
    elseif two == 4 then
        if e:joint(0x200) then
            state(e, 1, 9)
        end
    end
end

-- `FUN_004610c0`.
local function think_diving_at(e)
    if player_ck(e, 0x60) ~= 0 or not flag(e, FLAG_SIGHT) then
        go(e, 4)
    end
    released_ck(e)
end

-- `FUN_00461100`: straight at the player's head, not stopping.
local function diving_at(e)
    local two = e:routine(2)
    if two == 0 then
        e:clip(4, 0, 7)
        local height = i16(e:player_ground() - 0xA8C)
        e:set_var(FALL, height)
        e:set_var(NEXT, height - pos_y(e) < 0 and 0 or 1)
        set_spd_y(e, 0)
        e:set_var(COUNT, 0)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    if count_down(e, COUNT) == 0 then
        e:sound(1)
        e:set_var(COUNT2, 0xE)
    end
    goto_player(e, 0x60)
    speed_up(e, 5)
    e:joint(0x200)
    local height = i16(e:var(FALL))
    local y = pos_y(e)
    if e:var(NEXT) == 0 then
        if y > height then
            e:set_pos_y(y - 0x28)
        end
    elseif y < height then
        e:set_pos_y(y + 0x28)
    end
    if e:at_em_no() ~= PLAYER_BODY then
        e:add_speed_xyz()
        return
    end
    local off = height - pos_y(e)
    if off < 0x168 and off > -0x168 then
        if spd_x(e) <= TOUCHING_SPEED then
            new_area(e)
            go(e, 8)
        else
            go(e, 0xC)
        end
    end
end

-- `FUN_00461280`: into the player, and down.
local function hit_player(e)
    local two = e:routine(2)
    if two == 0 then
        e:pl_set_routine(2, e:cdir_player() and 1 or 0, 0, 0)
        e:sound(4)
        e:set_speed(0, 0, select(3, e:speed()))
        e:set_var(AT_BODY, 0)
        e:set_var(FALL, 0)
        e:clip(9, 0, 7)
        e:set_routine(2, 1)
        two = 1
    end
    if two <= 2 then
        if dropping(e) then
            dropped(e)
            local draw = e:rnd()
            e:set_routine(2, 3)
            e:set_var(COUNT, draw & 0xF)
        end
    elseif two == 3 then
        if count_down(e, COUNT) == 0 then
            e:set_routine(2, 4)
        end
        e:joint(0x200)
        e:set_move_cnt(e:move_cnt() - 1)
    elseif two == 4 then
        if e:joint(0x200) then
            if dest_ck(e, 0x400) == 0 then
                go(e, 9)
                return
            end
            e:set_timer(0, e:rnd() & 0xF)
            go(e, 0)
        end
    end
end

-- `FUN_00461440`.
local function think_closing(e)
    if player_ck(e, 0xC0) ~= 0 or not flag(e, FLAG_SIGHT) then
        box(e, BOX)
        go(e, 4)
    end
    released_ck(e)
end

-- `FUN_00461490`: down to the player's height to land on them.
local function closing(e)
    local two = e:routine(2)
    if two == 0 then
        e:clip(4, 0, 7)
        local height = i16(e:player_ground() + (e:scenario_claire() and 0xB4 or 0) - 0xB40)
        e:set_var(FALL, height)
        e:set_var(NEXT, height - pos_y(e) < 0 and 0 or 1)
        e:set_speed(0, 0, 0)
        e:set_tilt(0, 0)
        e:set_var(COUNT, 0)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    if count_down(e, COUNT) == 0 then
        e:sound(1)
        e:set_var(COUNT2, 0xE)
    end
    if flag(e, FLAG_OWNER) then
        goto_player(e, 0x60)
        speed_up(e, 4)
    else
        goto_player(e, 0x180)
        speed_up(e, 0xC)
    end
    e:joint(0x200)
    local height = i16(e:var(FALL))
    local y = pos_y(e)
    if height - y < -0x3C then
        e:set_pos_y(y - 0x3C)
    elseif height - y >= 0x3D then
        e:set_pos_y(y + 0x3C)
    end
    local distance = e:distance()
    if distance < 900 then
        box(e, 100)
    end
    if distance < 0x28A then
        local off = height - pos_y(e)
        if off < 100 and off > -100 then
            go(e, 0xE)
        end
    else
        e:add_speed_xyz()
    end
end

-- `FUN_00461640`: on the player, pecking.
local function pecking(e)
    local alive = e:player_life() >= 1
    e:set_var(NEXT, alive and 1 or 0)
    local two = e:routine(2)
    local leave = false
    if two == 0 then
        if e:player_life() < 0 then
            leave = true
        else
            e:set_speed(0, 0, 0)
            e:set_tilt(0, 0)
            if flag(e, FLAG_OWNER) then
                tell_all(e, FLAG_CALLED)
                tell_all(e, FLAG_BUSY)
                e:pl_set_routine(5, 0, 0, 0)
                e:pl_hold()
            end
            e:set_pos_y(i16(e:var(FALL)))
            goto_player(e, 0x800)
            e:clip(5, 0, 7)
            e:set_var(COUNT, 100)
            e:set_var(COUNT2, 0)
            e:set_speed_x(300)
            e:set_routine(2, 1)
            two = 1
        end
    end
    if two == 1 then
        local bite = true
        if not flag(e, FLAG_OWNER) then
            if e:rnd() & 7 == 0 then
                feathers(e, 1)
            end
            bite = false
        end
        if bite then
            local x, y, z = e:pos()
            e:call_effect(0, 1, 0x1000, i16(e:angle()), x, y + 500, z)
            if e:rnd() & 3 == 0 then
                feathers(e, 1)
            end
            e:sound(2)
            local result = e:pl_life_down(PECK, e:var(NEXT) ~= 0)
            if result == 1 then
                leave = true
            elseif result == 2 then
                tell_all(e, FLAG_KILLED)
                leave = true
            end
        end
        if not leave then
            e:set_var(COUNT3, 4)
            e:set_routine(2, 2)
            two = 2
        end
    end
    if not leave and two >= 2 and two <= 4 then
        e:joint(0x200)
        local count = i8(e:var(COUNT) + (e:gacha() and -3 or 0) - 1)
        e:set_var(COUNT, count & 0xFF)
        if count < 0 then
            leave = true
        elseif two == 2 then
            if count_down(e, COUNT3) == 0 then
                e:set_var(FALL, 0x96)
                local draw = (e:rnd() & 7) + 2
                e:set_routine(2, 3)
                e:set_var(COUNT2, draw)
                e:set_var(COUNT3, draw)
            end
        elseif two == 3 then
            local fall = i16(e:var(FALL))
            e:set_var(FALL, fall - 0xF)
            e:set_pos_y(pos_y(e) - fall)
            if count_down(e, COUNT2) == 0 then
                e:set_routine(2, 4)
            end
        else
            local fall = i16(e:var(FALL) + 0xF)
            e:set_var(FALL, fall)
            e:set_pos_y(pos_y(e) + fall)
            if count_down(e, COUNT3) == 0 then
                e:clip(5, 0, 7)
                e:set_routine(2, 1)
                e:set_var(COUNT2, 7)
            end
        end
    elseif two == 5 then
        leave = true
    end
    if leave then
        new_area(e)
        go(e, 4)
        if flag(e, FLAG_OWNER) then
            box(e, BOX)
            tell_all(e, FLAG_RELEASED)
            e:pl_set_routine_at(2, 3)
        end
    end
end

local THINK <const> = {
    [0] = think_perched,
    [4] = think_flying,
    [5] = think_climbing,
    [6] = think_diving,
    -- `FUN_00460dd0`, `FUN_00461270` and `FUN_00461630` are all `FUN_00462fb0`.
    [8] = released_ck,
    [11] = think_diving_at,
    [12] = released_ck,
    [13] = think_closing,
    [14] = released_ck,
}

local ACT <const> = {
    [0] = perched,
    about_to_fly,
    take_off,
    take_off_angry,
    flying,
    climbing,
    diving,
    landing,
    veering,
    turning_on_floor,
    hit_wall,
    diving_at,
    hit_player,
    closing,
    pecking,
}

-- `FUN_0045ff30`.
local function normal(e)
    if e:var(AREA_TIME) == 0 and e:rnd() & 0x1F == 0 then
        new_area(e)
    end
    local wander
    if e:player_life() < 0 then
        wander = e:var(ANGRY) ~= 0
        if wander then
            e:set_var(ANGRY, e:var(ANGRY) - 1)
        end
    else
        wander = not flag(e, FLAG_BUSY)
    end
    if wander and e:var(AREA_TIME) ~= 0 then
        e:root_ck_area(e:var(AREA))
        e:set_var(AREA_TIME, e:var(AREA_TIME) - 1)
    else
        local x, z = player_xz(e)
        e:root_ck(x, z)
    end
    local think = THINK[e:routine(1)]
    if think then
        think(e)
    end
    local act = ACT[e:routine(1)]
    if act then
        act(e)
    end
end

-- The direction of a body thrown from the player.
local function thrown_away(e)
    return e:direction_from_player()
end

-- `FUN_00461a00`: hit, and down to the floor.
local function hurt_fall(e)
    local two = e:routine(2)
    if two == 0 then
        e:sound(3)
        e:clip(9, 0, 7)
        set_spd_y(e, 0)
        e:set_var(AT_WALL, 0)
        e:set_var(FALL, 0)
        e:set_var(AWAY, i16(thrown_away(e) - e:angle()))
        e:set_speed_x(100)
        e:set_var(ROLL, e:rnd() & 1 == 0 and -200 or 200)
        blood(e)
        feathers(e, (e:rnd() & 1) + 2)
        e:set_var(COUNT, 3)
        e:set_var(COUNT2, 3)
        e:set_routine(2, 1)
        two = 1
    end
    if two == 1 then
        if e:joint(0x200) then
            e:set_routine(2, 2)
        end
    end
    if two == 1 or two == 2 then
        if count_down(e, COUNT) == 0 then
            feathers(e, 1)
        end
        if count_down(e, COUNT2) == 0 then
            feathers(e, 1)
        end
        e:set_var(FALL, i16(e:var(FALL) + 0x14))
        e:set_pos_y(pos_y(e) + e:var(FALL))
        e:add_speed_xz_at(e:var(AWAY))
        local roll = e:var(ROLL)
        local _, tilt = e:tilt()
        tilt = i16(tilt)
        if (roll < 0 and tilt > -0x800) or (roll >= 0 and tilt < 0x800) then
            e:set_tilt(0, tilt + roll)
        end
        if pos_y(e) > floor_y(e) then
            dropped(e)
            local draw = e:rnd()
            e:set_var(COUNT, draw & 0xF)
            e:set_tilt(0, 0)
            if e:life() < 0 then
                e:set_var(COUNT, 0xB)
                e:set_died()
                e:set_routine(2, 5)
            else
                e:set_routine(2, 3)
            end
        end
    elseif two == 3 then
        if count_down(e, COUNT) == 0 then
            e:set_routine(2, 4)
        end
        e:joint(0x200)
        e:set_move_cnt(e:move_cnt() - 1)
    elseif two == 4 then
        if e:joint(0x200) then
            if dest_ck(e, 0x400) == 0 then
                e:set_timer(0, e:rnd() & 0xF)
                state(e, 1, 0)
            else
                state(e, 1, 9)
            end
        end
    elseif two == 5 then
        e:joint(0x200)
        if count_down(e, COUNT) == 0 then
            state(e, 7, 0)
        end
    end
end

-- `FUN_00461f90`: a part on its own, which `Parts_down` then takes.
local function part_flying(e, part, dir)
    local at = dir + e:rnd() * 4
    local along = (e:rnd() & 0x7F) + 100
    e:fling_part(part, at, along, 0x14, e:rnd() << 4)
end

-- `FUN_00461d00`: blown apart.
local function hurt_burst(e)
    set_spd_y(e, 0)
    local dir = e:direction_from_player()
    local bits = e:rnd()
    local throw = function(bit, first, last)
        for part = first, last do
            if bits & bit == 0 then
                part_flying(e, part, dir)
            else
                e:blow_part(part, dir)
            end
        end
    end
    e:blow_part(0, dir)
    throw(1, 1, 2)
    throw(2, 3, 5)
    throw(4, 6, 8)
    e:blow_part(9, dir)
    e:blow_part(10, dir)
    throw(8, 11, 11)
    throw(0x10, 12, 12)
    blood(e)
    blood(e)
    blood(e)
    feathers(e, (e:rnd() & 3) + 8)
    e:set_died()
    state(e, 7, 1)
end

-- `FUN_00461ff0`: killed in the air.
local function hurt_killed(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_died()
        e:sound(3)
        box(e, 100)
        e:set_life(-1)
        blood(e)
        feathers(e, (e:rnd() & 3) + 3)
        local x, y, z = e:speed()
        e:set_speed(x // 2, y, z // 2)
        e:clip(9, 0, 7)
        e:set_routine(2, 1)
        two = 1
    end
    if two == 1 then
        if e:joint(0x200) then
            e:set_routine(2, 2)
        end
    end
    if two == 1 or two == 2 then
        local x, y, z = e:speed()
        y = i16(y)
        e:add_pos(i16(x), y, i16(z))
        set_spd_y(e, y + 0x14)
        if pos_y(e) > floor_y(e) then
            e:set_pos_y(floor_y(e))
            e:set_tilt(0, 0)
            e:set_var(COUNT, 8)
            e:set_routine(2, 3)
        end
        if e:sca_info() & 1 ~= 0 then
            if e:be_flg() & BE_PUSHED_Z == 0 then
                local roll = (e:rnd() & 1) * 0x800 + 0x400
                e:set_angle(0)
                e:set_tilt(e:rnd() << 4, roll)
            else
                local pitch = (e:rnd() & 1) * 0x800 + 0x400
                e:set_angle(e:rnd() << 4)
                e:set_tilt(pitch, 0)
            end
            e:clip(4, 0, 0)
            e:set_move_cnt(3)
            e:joint(0x200)
            state(e, 7, 2)
        end
    elseif two == 3 then
        local x, y, z = e:speed()
        e:add_pos(i16(x), 0, i16(z))
        e:set_speed(x // 2, y, z // 2)
        if count_down(e, COUNT) == 0 then
            e:clip(0xB, 0, 7)
            e:set_var(COUNT, 0xB)
            e:set_routine(2, 4)
        end
    elseif two == 4 then
        e:joint(0x200)
        if count_down(e, COUNT) == 0 then
            state(e, 7, 0)
        end
    end
end

-- `FUN_00462240`: thrown across the room.
local function hurt_blast(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_died()
        e:sound(3)
        box(e, 10)
        e:set_life(-1)
        blood(e)
        feathers(e, (e:rnd() & 3) + 5)
        e:set_angle(e:player_angle())
        e:set_speed_x(600)
        e:clip(9, 0, 7)
        e:set_routine(2, 1)
        two = 1
    end
    if two == 1 then
        if e:joint(0x200) then
            e:set_routine(2, 2)
        end
    end
    if two == 1 or two == 2 then
        e:add_speed_xz()
        local y = spd_y(e)
        e:set_pos_y(pos_y(e) + y)
        set_spd_y(e, y + 0x14)
        if pos_y(e) > floor_y(e) then
            e:sound(5)
            e:set_pos_y(floor_y(e))
            set_spd_y(e, 0)
            e:set_var(COUNT, 0x10)
            e:set_routine(2, 3)
        end
        if e:sca_info() & 1 ~= 0 then
            e:sound(3)
            blood(e)
            e:set_angle(e:angle() + 0x800)
            set_spd_y(e, 0)
            e:set_speed_x(0x14)
            e:set_routine(2, 4)
        end
    elseif two == 3 then
        e:add_speed_xz()
        e:set_speed_x(spd_x(e) // 2)
        if count_down(e, COUNT) == 0 then
            state(e, 7, 3)
        end
    elseif two == 4 then
        local y = spd_y(e)
        e:set_pos_y(pos_y(e) + y)
        set_spd_y(e, y + 0x14)
        e:add_speed_xz()
        if pos_y(e) > floor_y(e) then
            box(e, BOX)
            e:sound(5)
            e:set_pos_y(floor_y(e))
            set_spd_y(e, 0)
            state(e, 7, 3)
        end
    end
end

-- `DAT_00527978`: what each weapon does to it, by `Routine_1`.
local HURT <const> = {
    [0] = hurt_fall, hurt_fall, hurt_fall, hurt_fall, hurt_fall,
    hurt_burst, hurt_burst,
    hurt_blast, hurt_blast,
    hurt_burst, hurt_burst, hurt_burst,
    hurt_killed,
    hurt_fall, hurt_fall, hurt_fall, hurt_fall,
    hurt_burst,
    hurt_fall,
}

-- `FUN_00462460`.
local function scene_perched(e)
    perched(e)
    local three = e:routine(3)
    local bits
    if three == 0 or three == 1 then
        if three == 0 then
            set_flag(e, FLAG_QUIET)
            e:set_routine(3, 1)
        end
        bits = e:sce_free(0)
    elseif three == 2 or three == 3 then
        if three == 2 then
            clear_flag(e, FLAG_QUIET)
            e:set_routine(3, 3)
        end
        bits = e:sce_free(0) & 1 == 0 and e:noise() or 1
    else
        return
    end
    if bits & 1 ~= 0 then
        e:set_sce_free(0, 0)
        state(e, 1, 1)
    end
end

-- The Beretta's row: `DAT_00527978` has 0x13 (to `Routine_1` 0x12), so 0x13
-- reads the next table's first, `funcs_462454[0]` (`0x5279c4`). A crow it
-- hits or kills perches where it is, its `Routine_0` still 2 or 3, until the
-- scene's `Sce_free0` bit 0 sends it flying (1, 1). The Colt's (0xD) falls.
HURT[0x13] = scene_perched

-- The way it's told to go in `FUN_00462500`: a point, then the next step.
local function scene_leg(e, x, z)
    e:goto_point(x, z, 0x60)
    e:add_speed_xyz()
    e:joint(0x200)
    return count_down(e, COUNT) == 0
end

-- `FUN_00462500`.
local function scene_flight(e)
    local two = e:routine(2)
    if two == 0 then
        if e:sce_free(0) & 2 ~= 0 then
            e:set_sce_free(0, 0)
            box(e, 0x32)
            e:clip(4, 0, 7)
            e:set_speed_x(300)
            set_spd_y(e, -0x14)
            e:set_var(COUNT, 3)
            e:set_routine(2, 1)
            e:sound(1)
        end
    elseif two == 1 then
        if scene_leg(e, -0x30D4, -0x3234) then
            e:sound(1)
            e:set_var(COUNT, 0x3C)
            e:set_var(COUNT2, 5)
            e:set_routine(2, 2)
        end
    elseif two == 2 then
        if scene_leg(e, -0xF4F, -0x3000) then
            e:set_var(AREA_TIME, 0xFF)
            e:set_var(AREA, 0)
            box(e, BOX)
            state(e, 1, 4)
        end
    end
end

-- `FUN_00462650`.
local function scene_held(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_routine(2, 1)
        e:set_kind(e:kind() | 0x4000)
        e:set_be_flg(e:be_flg() | BE_FLOATING | BE_NO_SCA)
    elseif two == 2 then
        e:add_speed_xyz()
        e:joint(0x200)
        e:set_speed_x(e:speed_x() - 10)
        if count_down(e, COUNT) == 0 then
            e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
            state(e, 1, 4)
        end
        return
    elseif two ~= 1 then
        return
    end
    if e:sce_free(0) & 4 ~= 0 then
        e:set_sce_free(0, 0)
        e:set_kind(e:kind() & ~0x4000)
        e:clip(4, 0, 7)
        e:set_speed_x(300)
        set_spd_y(e, 0)
        e:set_be_flg(e:be_flg() & ~BE_FLOATING)
        e:set_var(COUNT, 6)
        e:set_routine(2, 2)
    end
end

local SCENE <const> = { [0] = scene_perched, scene_flight, scene_held }

-- `FUN_00462430`.
local function scene(e)
    local x, _, z = e:player_pos()
    e:root_ck(x, z)
    local run = SCENE[e:routine(1)]
    if run then
        run(e)
    end
end

-- The pool grows under it while `Timer1` runs.
local function pool_grows(e)
    local left = e:timer(1)
    if left ~= 0 then
        left = left - 1
        e:set_timer(1, left)
        e:set_shadow(400 + (0x3C - left) * 10)
    end
    return left
end

-- `FUN_00462720`, `Routine_1` 0: dead on the floor, twitching in its blood.
local function die_on_floor(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_damage_cnt(0x80)
        e:clip(0xB, 0, 7)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
        local count = (e:rnd() & 0x1F) + 0x19
        e:set_var(COUNT, count)
        e:set_timer(0, count)
        e:set_shadow(400)
        e:set_timer(1, 0x3C)
        e:set_routine(2, 1)
    elseif two == 2 then
        local timer = i16(e:timer(0))
        local moves = timer < 0x5B
        if not moves then
            local odd = (e:var(COUNT2) + 1) & 0xFF
            e:set_var(COUNT2, odd)
            moves = odd & 1 == 0
        end
        if moves then
            if e:move_cnt() == 0 and e:rnd() & 1 ~= 0 and timer < 0x5A then
                e:sound(3)
            end
            e:joint(0x200)
            e:set_var(COUNT, (e:var(COUNT) - 1) & 0xFF)
        end
        if e:var(COUNT) == 0 then
            local later = timer + (e:rnd() & 0xF) + 10
            e:set_timer(0, later)
            if later < 0x97 then
                e:set_routine(2, 1)
                e:set_var(COUNT, later & 0xFF)
            else
                e:set_routine(2, 3)
            end
        end
        return
    elseif two ~= 1 then
        return
    end
    if count_down(e, COUNT) == 0 then
        if i16(e:timer(0)) < 0x5B then
            if e:rnd() & 1 == 0 then
                e:set_var(COUNT, 7)
                e:set_move_cnt(0xB)
            else
                e:set_move_cnt(0)
                e:set_var(COUNT, 0xC)
            end
        else
            e:set_var(COUNT, 6)
            e:set_move_cnt(0xB)
        end
        e:set_routine(2, 2)
    end
    pool_grows(e)
end

-- `Routine_1` 1: nothing left of it.
local function die_gone(e)
    if e:routine(2) == 0 then
        e:set_damage_cnt(0x80)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
        e:set_shadow(0)
        e:set_routine(2, 1)
    end
end

-- `Routine_1` 2: stuck to a wall, dying, then off it.
local function die_on_wall(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_damage_cnt(0x80)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
        local count = (e:rnd() & 0x1F) + 0x19
        e:set_var(COUNT, count)
        e:set_timer(0, count)
        e:set_shadow(0)
        e:set_timer(1, 0x3C)
        e:set_routine(2, 1)
        two = 1
    end
    if two == 1 then
        if count_down(e, COUNT) == 0 then
            local timer = i16(e:timer(0))
            e:clip(4, 0, 7)
            e:set_routine(2, 2)
            if timer < 0x79 then
                e:set_var(COUNT, ((e:rnd() & 7) + 1) * 5)
            else
                e:set_var(COUNT, ((e:rnd() & 1) + 2) * 5)
            end
        end
    elseif two == 2 then
        local timer = i16(e:timer(0))
        local moves = timer < 0x65
        if not moves then
            local odd = (e:var(COUNT2) + 1) & 0xFF
            e:set_var(COUNT2, odd)
            moves = odd & 1 == 0
        end
        if moves then
            e:joint(0x200)
            if timer < 100 then
                e:set_move_cnt(e:move_cnt() + 1)
            end
            e:set_var(COUNT, (e:var(COUNT) - 1) & 0xFF)
        end
        if e:var(COUNT) == 0 then
            local later = timer + (e:rnd() & 0xF) + 10
            e:set_timer(0, later)
            if later < 0x97 then
                e:set_routine(2, 1)
                e:set_var(COUNT, later & 0xFF)
            else
                set_spd_y(e, 0)
                e:set_routine(2, 3)
            end
        end
    elseif two == 3 then
        local y = spd_y(e)
        e:set_pos_y(pos_y(e) + y)
        set_spd_y(e, y + 0x14)
        if pos_y(e) > floor_y(e) then
            box(e, BOX)
            e:sound(5)
            e:set_pos_y(floor_y(e))
            e:clip(0xB, 0, 7)
            e:set_tilt(0, 0)
            e:set_var(COUNT, 0xB)
            e:set_routine(2, 4)
        end
    elseif two == 4 then
        e:joint(0x200)
        if count_down(e, COUNT) == 0 then
            e:set_routine(2, 5)
        end
    end
end

-- `Routine_1` 3.
local function die_thrown(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_damage_cnt(0x80)
        e:clip(0xB, 0, 7)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
        e:set_shadow(400)
        e:set_timer(1, 0x3C)
        e:set_var(COUNT, 0xB)
        e:set_routine(2, 1)
    elseif two ~= 1 then
        return
    end
    if e:var(COUNT) ~= 0 then
        e:set_var(COUNT, e:var(COUNT) - 1)
        e:joint(0x200)
    end
    if e:timer(1) == 0 then
        e:set_routine(2, 2)
        return
    end
    pool_grows(e)
end

local DIE <const> = { [0] = die_on_floor, die_gone, die_on_wall, die_thrown }

-- `FUN_0045fd50`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_speed(0, 0, 0)
    e:set_life(LIFE)
    e:set_sce_free(0, 0)
    e:set_sce_free(1, 0)
    e:clip(0, 0, 0)
    e:joint(0x200)
    e:set_neck_flags(0)
    e:set_body_box(-350, BOX, 0x212, BOX)
    e:set_be_flg(e:be_flg() | BE_HIGH)
    local kind = e:kind()
    if kind & KIND_SCENE ~= 0 then
        set_flag(e, FLAG_FLOWN)
        e:set_kind(kind & ~KIND_SCENE)
        e:routines(4, kind & 0xF, 0, 0)
        scene(e)
    end
    if e:kind() & KIND_ANGRY ~= 0 then
        e:set_damage_cnt(0x80)
        e:set_be_flg(e:be_flg() | BE_NO_SCA)
    end
end

-- `FUN_0045fd00`: the higher it is, the smaller its shadow.
local function shadow(e)
    local above = i16(e:ground()) - pos_y(e)
    if above - PERCH > 0 then
        e:set_shadow(360000 // ((above + 0x60E) >> 1))
    else
        e:set_shadow(0x190)
    end
end

-- `FUN_00463020`.
local function last_checks(e)
    local one = e:routine(1)
    if flag(e, FLAG_OWNER) and (one < 0xB or one > 0xE) then
        e:set_room_enemy(e:room_enemy() & ~ROOM_TAKEN)
        clear_flag(e, FLAG_OWNER)
    end
    if flag(e, FLAG_KILLED) then
        e:set_var(ANGRY, 0x78)
        clear_flag(e, FLAG_KILLED)
    end
end

function update(e)
    if e:line_to_player(0x8400) then
        set_flag(e, FLAG_SIGHT)
    else
        clear_flag(e, FLAG_SIGHT)
    end
    local ofs_y = e:distance() < 900 and -350 or 0
    local _, at_w, at_h, w = e:body_box()
    e:set_body_box(ofs_y, at_w, at_h, w)
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        normal(e)
    elseif zero == 2 or zero == 3 then
        local x, z = player_xz(e)
        e:root_ck(x, z)
        local hurt = HURT[e:routine(1)]
        if hurt then
            hurt(e)
        end
    elseif zero == 4 then
        scene(e)
    elseif zero == 7 then
        local dying = DIE[e:routine(1)]
        if dying then
            dying(e)
        end
    end
    if e:routine(0) ~= 7 then
        shadow(e)
    end
    last_checks(e)
end
