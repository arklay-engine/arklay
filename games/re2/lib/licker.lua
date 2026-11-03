-- The lickers, `Em_22` and `Em_24` (entity ids 0x22 and 0x24).
--
-- The state machine, its tables and its effects. The engine keeps the licker's
-- animation, collision, tongue blend and the turn of the surface it's on, and
-- runs them after `update`.
--
-- `Em_24` is a copy of `Em_22` with its own clips, life, starts and choices,
-- and without the climbing and the scripted drops. A variant table names what
-- differs (see `enemy/22` and `enemy/24`).
--
-- Everything that lasts past a tick lives on the entity: `e:var` and `e:set_var`
-- are the fields of the original's work that the engine doesn't read.

-- The variant of this tick's licker, set by `run`.
local V

local TARGET <const> = 0 -- `+0x228`: it has someone to go after.
local AWARE <const> = 1 -- `+0x219`: 1 has heard, 2 knows where.
local WAIT <const> = 2 -- `+0x22e`: the wait before the next attack.
local WEAPON <const> = 3 -- `+0x22f`: the weapon of the last hit.
local TURN <const> = 4 -- `+0x230`: which way it's turning along a wall.
local GUN_AREA <const> = 5 -- `+0x232`: where a shot can aim.
local COUNT <const> = 6 -- `+0x233`.
local AFTER <const> = 7 -- `+0x23c`: what runs after each tick's move.
local SLOWING <const> = 8 -- `+0x241`: the jump's slowing.
local GRAVITY <const> = 9 -- `+0x242`.
local LEFT_Y <const> = 10 -- `+0x244`: the height it left from.
local TIMER0 <const> = 11 -- `Timer0`.
local BITE_BEHIND <const> = 12 -- `FUN_00466100`'s turn: the side the bite came from.
local TIMER2 <const> = 13 -- `Timer2`: the side of the room it's heading for.
local TIMER3 <const> = 14 -- `Timer3`: the side of the room its wall faces.
local NEXT_SIDE <const> = 15 -- `+0x218`: the side it's going over to.
local WALL <const> = 16 -- `+0x220`: the collision record it's on, or 0.
local DEST_X <const> = 17 -- `+0x234`: where it's climbing to.
local DEST_Y <const> = 18
local DEST_Z <const> = 19

local AFTER_BITE <const> = 1 -- `FUN_00466100`: the jump's bite bleeds.
local AFTER_FLAMES <const> = 2 -- `FUN_00467370`: a burning weapon's flames.

-- `Be_flg`: no pushes, none given, no `Sca_ck_em`.
local BE_NO_OBA <const> = 2
local BE_NO_PUSH <const> = 4
local BE_NO_SCA <const> = 8
local BE_SUPER <const> = 0x2000000

-- `+0x22c`: the shot that hurt it wasn't a surprise, it stands on its hind legs,
-- its feet make no sound, it's reached the wall or the ceiling it was climbing to
-- (along the floor, then up), it fell off a wall, it's climbing, the next tick has
-- no `Sca_ck_em`, and it's in the air.
local FLAG_KNOWS <const> = 1
local FLAG_UP <const> = 2
local FLAG_QUIET <const> = 4
local FLAG_ARRIVED <const> = 0x10
local FLAG_ARRIVED_Y <const> = 0x20
local FLAG_FALLING <const> = 0x40
local FLAG_CLIMBING <const> = 0x80
local FLAG_NO_SCA <const> = 0x100
local FLAG_JUMPING <const> = 0x200
-- What `Em22_Damage` clears when the hit does more than hurt.
local FLAGS_KEPT <const> = 0xFD0F


-- `WORD_00527c46`, by `Routine_3`: how far the tongue reaches as it goes out.
local TONGUE_REACH <const> = {
    [0] = 3500, 4600, 4500, 4200, 3500, 3000, 2500, 2000,
}

-- `DAT_00527c90`, by weapon: how many hits of blood, which points they pick from
-- from the front and from behind, the mask, and the effect's kind, row and size.
local BLOOD <const> = {
    [0] = { 0, 0, 0, 0, 0, 6, 0x2000 },
    { 0, 0, 12, 12, 0, 6, 0x2000 },
    { 1, 0, 12, 12, 0, 6, 0x2000 },
    { 1, 0, 12, 12, 0, 6, 0x2000 },
    { 1, 0, 12, 12, 0, 6, 0x2000 },
    { 1, 0, 12, 12, 0, 6, 0x2000 },
    { 1, 0, 12, 12, 0, 6, 0x2000 },
    { 5, 0, 0, 24, 0, 6, 0x2000 },
    { 8, 0, 0, 24, 0, 6, 0x2000 },
    { 4, 0, 12, 12, 5, 4, 0x1400 },
    { 6, 0, 0, 24, 5, 4, 0x1800 },
    { 4, 0, 0, 24, 4, 9, 0x2800 },
    { 0, 0, 12, 12, 0, 6, 0x2000 },
    { 1, 0, 12, 12, 0, 6, 0x2000 },
    { 0, 0, 12, 12, 6, 0, 0x1000 },
    { 1, 0, 0, 24, 0, 6, 0x2000 },
    { 6, 0, 0, 24, 5, 4, 0x1800 },
    { 1, 0, 8, 12, 5, 6, 0x2000 },
    { 1, 0, 8, 12, 0, 6, 0x2000 },
    { 1, 0, 8, 12, 0, 6, 0x2000 },
}

-- `pVec`: the points on the body that bleed, and their parts.
local BLOOD_POINTS <const> = {
    [0] = { -300, 0, 0, 7 },
    { 0, -200, 0, 4 },
    { 0, 200, 200, 6 },
    { 0, -300, 0, 8 },
    { -300, 0, 0, 10 },
    { 300, 100, 0, 11 },
    { 0, 200, 200, 6 },
    { 0, -300, 0, 13 },
    { 300, -100, 0, 5 },
    { -200, -600, 0, 6 },
    { 300, 400, 0, 12 },
    { 0, 0, -100, 13 },
    { 0, 0, -400, 0 },
    { -400, 0, -300, 0 },
    { -300, 0, 200, 2 },
    { -300, 0, -200, 2 },
    { -500, -300, 200, 2 },
    { -200, -600, -400, 2 },
    { -200, 600, -200, 14 },
    { 0, 600, 200, 18 },
    { 0, 0, 0, 16 },
    { 0, 0, 0, 20 },
    { -400, 200, 0, 18 },
    { -100, 400, -200, 18 },
}

-- `FUN_00466100`'s points on the player, `y` and `z`.
local BITE_POINTS <const> = {
    [0] = { -400, 400 }, { -300, 250 }, { -200, 100 }, { -100, -50 },
}

-- `DAT_00527ba8`: the entries of its sixteen that catch, one a tick.
local FLAMES <const> = {
    [4] = { 0, 100, 100, 6 },
    [6] = { -100, 0, -100, 15 },
    [8] = { -200, 0, 200, 1 },
    [11] = { -300, -600, -200, 2 },
    [15] = { 0, 0, 0, 20 },
}

local mv

-- `Branch`.
local function branch(value, first, second, third)
    if value < first then
        return 0
    elseif value < second then
        return 1
    elseif value < third then
        return 2
    end
    return 3
end

-- The original's division, which goes toward zero.
local function quot(value, by)
    local q = value // by
    if q < 0 and q * by ~= value then
        q = q + 1
    end
    return q
end

local function has_flag(e, flag)
    return e:flags() & flag ~= 0
end

local function raise_flag(e, flag)
    e:set_flags(e:flags() | flag)
end

local function lower_flag(e, flag)
    e:set_flags(e:flags() & ~flag)
end

local function r2(e)
    return e:routine(2)
end

local function set_r2(e, value)
    e:set_routine(2, value)
end

local function r3(e)
    return e:routine(3)
end

local function set_r3(e, value)
    e:set_routine(3, value)
end

local function next_r3(e)
    local value = (e:routine(3) + 1) & 0xFF
    e:set_routine(3, value)
    return value
end

-- `Routine_3` moves on when the animation ends.
local function step_r3(e, ended)
    if ended then
        e:set_routine(3, e:routine(3) + 1)
    end
end

-- `Mot_play2`.
local function mot_play(e, clip, hokan, reverse)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    return e:joint(0x1000 // (hokan + 1), reverse)
end

-- `Mot_play2_sp`.
local function mot_play_sp(e, clip, hokan)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    e:spd()
    if e:joint(0x1000 // (hokan + 1)) then
        return true
    end
    e:add_speed_super(0, 0)
    return false
end

-- `Mot_play2_spxz`.
local function mot_play_spxz(e, clip, hokan)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    e:spd()
    if e:joint(0x1000 // (hokan + 1)) then
        return true
    end
    e:add_speed_xz()
    return false
end

-- `Em22_Atd_set_1`.
local function atd_set(e)
    e:set_atd(0, 0x2A0, 0, 0x250, 600, 1000)
    e:set_atd(1, -0x60, -0x180, 0x180, 0x180, 0x180)
    e:clear_back_atd()
end

-- `Em22_Atd_set_2`: on a wall or the ceiling.
local function atd_set_up(e)
    e:set_atd(0, 0x2A0, -200, 0x180, 600, 0x180)
    e:set_atd(1, -0x60, -0x180, 0x180, 0x180, 0x180)
    e:set_atd(2, 0x400, 0x70, 0x380, 0x180, 0x380)
end

-- `Gunarea_chg`.
local function gunarea_chg(e)
    local area = e:var(GUN_AREA)
    local be_flg = e:be_flg()
    if area == 0 then
        be_flg = (be_flg & 0xF3FFFFFF) | 0x10000000
    elseif area == 1 then
        be_flg = (be_flg & 0xFBFFFFFF) | 0x18000000
    elseif area == 2 then
        be_flg = (be_flg & 0xEBFFFFFF) | 0x8000000
    elseif area == 3 then
        be_flg = (be_flg & 0xEFFFFFFF) | 0xC000000
    elseif area == 4 then
        local weapon = e:weapon()
        if weapon == 7 or weapon == 8 then
            be_flg = (be_flg & 0xEFFFFFFF) | 0xC000000
        else
            be_flg = (be_flg & 0xE7FFFFFF) | 0x4000000
        end
    elseif area == 5 then
        be_flg = be_flg & 0xE3FFFFFF
    end
    e:set_be_flg(be_flg)
end

-- `FUN_00466100`.
local function bite_blood(e)
    local count = e:var(COUNT)
    if count & 1 == 0 then
        local point = BITE_POINTS[count >> 1]
        local turn = e:var(BITE_BEHIND) ~= 0 and 0x800 or 0
        local x, z = e:rotate(turn, 0x15E, point[2])
        local at_x, at_y, at_z = e:player_root_point(x, point[1], z)
        if at_x then
            e:call_effect(0, 2, 0x2400, 0, at_x, at_y, at_z)
        end
    end
    e:set_var(COUNT, count + 1)
    if count + 1 == 7 then
        e:set_var(AFTER, 0)
    end
end

-- `FUN_00467370`: the first tick only sets it going.
local function flames(e)
    local count = e:var(COUNT)
    if count ~= 0 then
        local flame = FLAMES[(count - 1) % 16]
        if flame then
            e:effect(6, 0, e:rnd() * 2 + 0x1000, 0, flame[4], flame[1], flame[2], flame[3])
        end
        if count == 0x10 then
            e:set_var(AFTER, 0)
            e:set_var(COUNT, 0x11)
            return
        end
    end
    e:set_var(COUNT, count + 1)
end

-- `FUN_004671f0`: the blood of a hit.
local function hit_blood(e, behind)
    local weapon = e:var(WEAPON)
    local blood = V.blood[weapon] or BLOOD[weapon] or BLOOD[0]
    local from = behind and blood[3] or blood[2]
    for _ = 1, blood[1] do
        local pick = e:rnd() & ((blood[4] - 1) & 0xFF)
        local point = BLOOD_POINTS[pick + from]
        e:effect(blood[5], blood[6], blood[7], 0, point[4], point[1], point[2], point[3])
    end
    e:set_var(COUNT, 0)
    if weapon == 0xE then
        e:set_var(AFTER, AFTER_FLAMES)
    end
end

-- `FUN_00463490`: whether it notices the player, who is the only target.
local function spots(e)
    if e:player_life() < 0 then
        return false
    end
    if e:routine(0) == 7 then
        e:set_var(AWARE, 0)
        return false
    end
    if has_flag(e, FLAG_KNOWS) then
        return true
    end
    if e:touching() then
        e:set_var(AWARE, 1)
        return true
    end
    local noise = e:noise() & 0xFF
    local heard
    if branch(e:distance(), 3000, 9000, 0x238C) == 0 then
        heard = noise & 0x17
    else
        heard = noise & 5
    end
    if heard == 0 then
        return false
    end
    e:set_var(AWARE, 1)
    return true
end

-- `FUN_004641f0`.
local function spots_close(e)
    if e:player_life() < 0 then
        return false
    end
    if e:routine(0) == 7 then
        e:set_var(AWARE, 0)
        return false
    end
    if has_flag(e, FLAG_KNOWS) then
        e:set_var(AWARE, 1)
        return true
    end
    if e:touching() then
        e:set_var(AWARE, 2)
        return true
    end
    local noise = e:noise() & 0xFF
    local near = branch(e:distance(), 3000, 5000, 7000)
    local noticed
    if near == 0 then
        noticed = e:pl_routine(1) ~= 0
    elseif near == 1 then
        noticed = noise & 0x17 ~= 0
    else
        noticed = noise & 5 ~= 0
    end
    if noticed then
        e:set_var(AWARE, 2)
    end
    return noticed
end

local function set_target(e, target)
    e:set_var(TARGET, target and 1 or 0)
end

-- `FUN_00463540`: turn away from what's ahead.
local function turn_at_wall(e, step)
    local here_x, here_y, here_z = e:pos()
    local angle = e:angle()
    local ahead_x, ahead_z = e:rotate(angle, 0x708, 0)
    local x, z, width, depth = e:first_wall(here_x + ahead_x, here_y, here_z + ahead_z, 400)
    if not x then
        e:set_var(TURN, 0)
        return
    end
    if e:var(TURN) == 0 then
        local along = quot((here_x - x) * depth, width > 1 and width or 1)
        local over = here_z - z
        local beyond = over - depth
        local side
        if over <= along then
            if -beyond == along or -along < beyond then
                side = 0
            else
                side = 1
            end
        elseif -along < beyond then
            side = 3
        else
            side = 2
        end
        local off = (angle & 0xFFF) - (((side - 2) * 0x400) & 0xFFF)
        if off > 0x800 or off < -0x800 then
            off = off - 0x1000
        end
        e:set_var(TURN, off == 0 and -step or step)
    end
    e:set_angle(angle + e:var(TURN))
end

local function idle(e)
    e:set_var(GUN_AREA, 4)
    local target = spots(e)
    set_target(e, target)
    if target then
        local aware = e:var(AWARE)
        if aware == 1 then
            e:go(0xE)
        elseif aware == 2 then
            e:go(0xF)
        end
        return
    end
    local state = r2(e)
    if state == 0 or state == 3 then
        local count = r3(e) + 1
        raise_flag(e, FLAG_UP)
        e:set_var(TURN, 0)
        e:clip(3, 0, 0x2F)
        set_r3(e, count)
        if count < 2 then
            set_r2(e, 1)
            state = 1
        else
            set_r2(e, 4)
            set_r3(e, 0)
            e:clip(1, 0, 0x1F)
            state = 4
        end
    end
    if state == 4 then
        turn_at_wall(e, 0xC)
        e:spd()
        if e:joint(0x80) and e:rnd() & 3 == 0 then
            set_r2(e, 0)
            set_r3(e, 0)
        end
        e:add_speed_xyz()
    elseif state == 1 then
        e:spd()
        if e:joint(0x55) then
            set_r2(e, 3)
        end
        e:add_speed_xz()
    elseif state == 2 then
        e:step(e:joint(0x400, true))
    end
end

local function turn_to(e)
    local state = r2(e)
    if state == 0 then
        lower_flag(e, FLAG_UP)
        e:clip(2, 0, 0xF)
        e:set_var(GUN_AREA, 3)
        set_r2(e, 1)
        state = 1
    end
    if state == 1 then
        e:goto_player(0x60)
        e:spd()
        local x, y, z = e:speed()
        e:set_speed(quot(x, 4), y, quot(z, 4))
        e:step(e:joint(0x100))
        e:add_speed_xyz()
        return
    end
    if state == 2 then
        e:clip(4, 0, 0xF)
        set_r2(e, 3)
        set_r3(e, 0)
        state = 3
    end
    if state == 3 then
        local count = r3(e) + 1
        set_r3(e, count)
        e:joint(0x100)
        if count > 4 then
            e:go(0xF)
        end
    end
end

local function get_up(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:clip(6, 0, 0xF)
        e:set_var(GUN_AREA, 4)
        set_r2(e, 1)
    elseif state ~= 1 then
        return
    end
    if e:var(TARGET) ~= 0 then
        e:goto_player(0x20)
    end
    if e:joint(0x100, true) then
        lower_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 3)
        e:go(5)
    end
end

local function get_down(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:clip(5, 0, 0xF)
        e:set_var(GUN_AREA, 4)
        set_r2(e, 1)
    elseif state ~= 1 then
        return
    end
    if e:joint(0x100) then
        e:go(0)
    end
end

-- `LAB_00463918`.
local function hesitate_step(e)
    local turn = 1
    if e:turn_to_player(0x200) == 0 then
        if e:distance_now() < 4000 then
            turn = e:turn_to_player(0x40)
        else
            turn = e:turn_to_player(0x10)
            set_r3(e, r3(e) + 1)
        end
        e:set_angle(e:angle() + turn)
        local _, y = e:speed()
        e:set_speed(-40, y, 0)
        e:add_speed_xz()
    else
        e:go(0x13)
    end
    e:joint(0x100)
    if turn == 0 then
        e:go(2)
    end
    if r3(e) > 0x20 then
        e:go(5)
    end
end

local function hesitate(e)
    local state = r2(e)
    local waiting = state == 1
    if state == 0 then
        lower_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 3)
        if e:var(TARGET) == 0 then
            e:clip(4, 0, 0xF)
            set_r2(e, 1)
            waiting = true
        end
    elseif state > 2 then
        return
    end
    if waiting then
        local target = spots(e)
        set_target(e, target)
        local count = r3(e) + 1
        set_r3(e, count)
        e:joint(0x100)
        if not target then
            if count >= 0x79 then
                e:go(4)
                e:clip(2, 0, 0xF)
            end
            return
        end
    end
    if state ~= 2 then
        e:clip(2, 0, 0xF)
        set_r2(e, 2)
    end
    hesitate_step(e)
end

local function back_off(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:clip(1, 0, 0xF)
        e:set_var(GUN_AREA, 4)
        set_r2(e, 1)
    elseif state ~= 1 then
        return
    end
    if e:var(WAIT) == 0 and e:distance() < 2000 then
        if e:turn_to_player(0x300) == 0 then
            e:go(9)
        else
            e:go(2)
        end
        return
    end
    if not V.em24 and e:distance() > 4000 then
        if e:player_life() > 99 then
            if e:rnd() & 7 < 3 then
                e:go(0xA)
            elseif e:rnd() & 2 == 0 then
                e:go(8)
            else
                e:go(0x14)
            end
        end
        if e:player_life() < 100 then
            if e:rnd() & 7 < 2 then
                e:go(0xA)
            else
                e:go(0x14)
            end
        end
    end
    e:spd_reversed()
    e:add_speed_xz()
    e:joint(0x100, true)
    if e:move_cnt() == 0x2E then
        e:go(0xF)
    end
end

-- The tongue's and the slash's end: `Pl_damage`, `Routine_1` 0 or 1.
local function hurt_player(e, down)
    local behind = e:cdir_player()
    e:pl_hold()
    e:pl_set_routine(2, behind and 1 or 0, 0, 0)
    if e:pl_life_down(down) & 2 ~= 0 then
        e:pl_set_routine(3, 0, 0, 0)
    end
    e:set_var(WAIT, 0x28)
    e:set_var(TIMER0, 1)
end

local function tongue_attack(e)
    local state = r2(e)
    if state == 0 then
        set_r3(e, 0)
        if has_flag(e, FLAG_UP) then
            e:goto_player(0x10)
            e:step(mot_play(e, 6, 0xF, true))
            state = 0xFF
        else
            set_r2(e, 1)
            state = 1
        end
    end
    if state == 1 then
        if e:tongue_owner() == 0 then
            e:set_tongue_owner(e:slot() + 2)
            lower_flag(e, FLAG_UP)
            e:tongue_out()
            e:clip(7, 0, 3)
            e:set_var(GUN_AREA, 3)
            e:set_var(TIMER0, 0)
            set_r2(e, 2)
            set_r3(e, 0)
            state = 2
        else
            e:go(0xF)
            state = 0xFF
        end
    end
    if state == 2 then
        e:spd()
        e:goto_player(8)
        e:step(e:joint(0x400))
        e:add_speed_xz()
        if e:move_cnt() == 0x11 then
            e:effect(0, 0x14, 0x1800, 0, 9, 200, 0, 0)
            e:sound(5)
        end
    end
    if state == 3 and e:tongue() == 0 then
        e:go(0xF)
    end
    local count = next_r3(e)
    if count > 0x11
        and count < 0x1A
        and e:var(TIMER0) == 0
        and e:pl_damage_cnt() == 0
        and e:turn_to_player(0x100) == 0
        and e:distance() < TONGUE_REACH[count - 0x12]
    then
        hurt_player(e, 0x19)
    end
end

local function slash(e)
    local state = r2(e)
    if state == 0 then
        set_r3(e, 0)
        if not has_flag(e, FLAG_UP) then
            e:step(mot_play(e, 6, 0xF, false))
            return
        end
        set_r2(e, 1)
    elseif state > 2 then
        return
    end
    if r2(e) == 1 then
        raise_flag(e, FLAG_UP)
        e:clip(V.clips.slash, 0, 3)
        e:set_var(GUN_AREA, 4)
        set_r2(e, 2)
        e:set_var(TIMER0, 0)
    end
    e:spd()
    e:goto_player(8)
    if e:joint(0x400) then
        e:go(0xF)
    end
    e:add_speed_xz()
    local count = next_r3(e)
    if e:pl_damage_cnt() == 0
        and e:var(TIMER0) == 0
        and count >= 2
        and count <= 6
        and e:turn_to_player(0x154) == 0
        and e:distance() <= 0xAEF
        and e:player_life() >= 0
    then
        local x, y, z = e:part_point(13, 0, 200, 0)
        if x then
            e:call_effect(0, 0, 0x2400, 0, x, y, z)
        end
        hurt_player(e, 0xF)
    end
end

-- `FUN_0047dbb0`.
local function angle_to_player(e)
    local off = e:angle_to_player()
    if off > 0x800 then
        return off - 0x1000
    end
    return off
end

-- The jump's bite, which takes the most of anything.
local function bite(e)
    e:set_var(AFTER, AFTER_BITE)
    e:set_var(COUNT, 0)
    e:set_var(TIMER0, 1)
    e:pl_hold()
    e:pl_life_down(e:player_life() > 0x6E and 100 or 0x50)
    e:sound(10)
    local behind = e:cdir_player()
    if e:player_life() < 0 then
        e:set_var(BITE_BEHIND, behind and 1 or 0)
        e:pl_set_routine(3, 0, 0, 0)
        e:pl_held_kill()
        e:set_player_life(-1)
    else
        if e:player_life() < 0x28 then
            e:set_player_life(0x28)
        end
        e:pl_set_routine(2, behind and 3 or 2, 0, 0)
    end
end

local function jump(e)
    local state = r2(e)
    if state == 0 then
        local _, y = e:pos()
        e:set_var(LEFT_Y, y)
        e:set_speed(0, 0, 0)
        e:set_var(SLOWING, 0)
        e:set_var(GRAVITY, 0)
        e:set_var(TIMER0, 0)
        if has_flag(e, FLAG_UP) then
            e:clip(6, 0, 3)
            set_r2(e, 1)
            state = 1
        else
            set_r2(e, 2)
            state = 2
        end
    end
    if state == 1 then
        e:step(e:joint(0x400, true))
    end
    if state == 2 then
        lower_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 3)
        e:set_var(SLOWING, -10)
        e:set_var(GRAVITY, 0)
        e:sound(8)
        set_r2(e, 3)
        state = 3
    end
    if state == 3 then
        e:step(mot_play(e, V.clips.leap, 0, false))
        e:goto_player(6)
    end
    if state == 4 then
        raise_flag(e, FLAG_JUMPING)
        local _, _, z = e:speed()
        e:set_speed(0x226, -0xF5, z)
        e:set_var(SLOWING, -8)
        e:set_var(GRAVITY, 0x23)
        e:add_pos(0, -0x72F, 0)
        set_r2(e, 5)
        state = 5
    end
    if state == 5 then
        e:step(mot_play(e, V.clips.flight, 0, false))
        e:goto_player(0x20)
        local count = e:move_cnt()
        local angle = angle_to_player(e)
        if e:pl_damage_cnt() == 0
            and count >= 0xB
            and count <= 0xE
            and angle >= -199
            and angle <= 0x2C7
            and e:distance() <= 0x897
            and e:var(TIMER0) == 0
        then
            bite(e)
        end
    end
    if state == 6 then
        e:clip(V.clips.flight, 0x1C, 0)
        local _, y = e:pos()
        if y > e:var(LEFT_Y) - 0x60F then
            lower_flag(e, FLAG_JUMPING)
            e:set_var(SLOWING, -60)
            e:set_pos_y(e:var(LEFT_Y))
            local x, _, z = e:speed()
            e:set_speed(x, 0, z)
            e:set_var(GRAVITY, 0)
            set_r2(e, 7)
            state = 7
        end
    end
    if state == 7 and mot_play(e, V.clips.land, 0, false) then
        e:go(0xF)
    end
    local _, up = e:speed()
    e:add_pos(0, up, 0)
    e:add_speed_xz()
    local x, y, z = e:speed()
    x = x + e:var(SLOWING)
    if x < 0 then
        x = 0
        e:set_var(SLOWING, 0)
    end
    e:set_speed(x, y + e:var(GRAVITY), z)
end

local function stalk_on(e)
    if e:distance() < 0x9C4 then
        e:go(0xF)
    end
    e:goto_player(0x40)
    e:spd()
    if e:joint(0x1000) then
        if e:distance() > 8000 then
            e:set_var(AWARE, 1)
            e:go(0)
            return
        end
        if e:distance() < 2000 then
            e:go(0xF)
            e:add_speed_xyz()
            return
        end
        local draw = e:rnd()
        set_r2(e, 2)
        set_r3(e, 0)
        if draw & 3 == 0 then
            e:clip(0, 0, 0)
        end
    end
    e:add_speed_xyz()
end

local function stalk(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:clip(6, 5, 3)
        e:set_var(GUN_AREA, 4)
        set_r2(e, 1)
        set_r3(e, 0)
        state = 1
    end
    if state == 1 then
        if e:joint(0x400) then
            e:clip(0, 0, 0)
            set_r2(e, 2)
            set_r3(e, 0)
        end
        e:goto_player(0x40)
    elseif state == 2 then
        stalk_on(e)
    elseif state == 3 then
        local count = r3(e) + 1
        set_r3(e, count)
        if count > 5 then
            e:clip(0, 0, 0)
            set_r2(e, 2)
            set_r3(e, 0)
        end
    end
end

local function listen(e)
    local state = r2(e)
    if state == 0 then
        lower_flag(e, FLAG_UP)
        e:clip(4, 0, 0xF)
        e:set_var(GUN_AREA, 3)
        set_r2(e, 1)
        set_r3(e, 0)
        e:set_var(TIMER0, 0)
    end
    if state <= 1 then
        if e:joint(0x100) then
            e:clip(4, 0, 3)
            set_r2(e, r2(e) + 1)
        end
    elseif state == 2 and e:joint(0x400) then
        e:go(4)
    end
    local count = next_r3(e)
    if count > 10 then
        local target = spots_close(e)
        set_target(e, target)
        if target then
            e:go(0xF)
        end
    end
end

local function decide_near(e, side)
    if e:var(WAIT) ~= 0 then
        e:go(6)
    elseif e:rnd() & 3 ~= 0 then
        e:go(side < 2 and 6 or 2)
    else
        e:go(9)
    end
end

local function decide_middle(e, side, life)
    if side == 0 then
        if e:var(WAIT) ~= 0 then
            e:go(0x13)
            return
        end
        if life > 99 then
            if e:rnd() & 7 < 3 then
                e:go(0xA)
            elseif e:rnd() & 2 ~= 0 then
                e:go(0x13)
            else
                e:go(8)
            end
        end
        if life < 100 then
            e:go(e:rnd() & 7 > 1 and 8 or 0xA)
        end
    elseif side == 1 then
        if e:var(WAIT) == 0 then
            e:go(8)
        elseif e:rnd() & 1 ~= 0 then
            e:go(6)
        else
            e:go(0x13)
        end
    else
        e:go(2)
    end
end

local function decide_far(e, side, life)
    if side == 0 then
        if has_flag(e, FLAG_KNOWS) then
            e:go(0x13)
            return
        end
        if life > 99 then
            e:go(e:rnd() & 7 > 4 and 0x13 or 0xA)
        end
        if life < 100 then
            e:go(e:rnd() & 7 > 1 and 0x13 or 0xA)
        end
    elseif side == 1 then
        e:go(e:rnd() & 3 ~= 0 and 0x13 or 2)
    else
        e:go(0x14)
    end
end

local function decide_farthest(e, side, life)
    if e:distance() < 10000 or e:var(AWARE) ~= 2 then
        e:set_var(AWARE, 2)
        if side < 3 then
            if life > 99 then
                e:go(e:rnd() & 7 > 4 and 0x14 or 0xA)
            end
            if life < 100 then
                e:go(e:rnd() & 7 > 1 and 0x14 or 0xA)
            end
        else
            e:go(0xC)
        end
    else
        e:go(0xC)
    end
end

-- `Em_24`'s choices (`FUN_0046da40`): the same ranges, without the player's
-- life in the near ones.
local function decide_24(e, side, range, life)
    local wait = e:var(WAIT) ~= 0
    if range == 0 then
        if side < 2 then
            if wait or e:rnd() & (side == 0 and 1 or 3) ~= 0 then
                e:go(6)
            else
                e:go(9)
            end
        elseif wait then
            e:go(6)
        else
            e:go(e:rnd() & 3 ~= 0 and 2 or 9)
        end
    elseif range == 1 then
        if side == 0 then
            e:go(wait and 0x13 or 8)
        elseif side == 1 then
            if not wait then
                e:go(8)
            else
                e:go(e:rnd() & 1 ~= 0 and 6 or 0x13)
            end
        else
            e:go(2)
        end
    elseif range == 2 then
        if side == 0 then
            if not has_flag(e, FLAG_KNOWS) and e:rnd() & 2 ~= 0 then
                e:go(e:rnd() & 7 > 5 and 8 or 0xA)
            else
                e:go(0x13)
            end
        elseif side == 1 then
            if e:rnd() & 3 == 0 then
                e:go(2)
            elseif e:rnd() & 1 ~= 0 and life < 0x65 then
                e:go(0xA)
            else
                e:go(0x13)
            end
        else
            e:go(6)
        end
    elseif e:distance() < 10000 or e:var(AWARE) ~= 2 then
        e:set_var(AWARE, 2)
        if side < 3 then
            e:go(e:rnd() & 7 > 4 and 0x14 or 0xA)
        else
            e:go(0xC)
        end
    else
        e:go(0xC)
    end
end

local function decide(e)
    local life = e:player_life()
    if life < 0 or e:routine(0) == 7 then
        e:go(0xE)
    elseif V.em24 then
        local off = e:angle_to_player()
        if off > 0x800 then
            off = 0x1000 - off
        end
        local side = branch(off, 0x100, 0x200, 0x600)
        decide_24(e, side, branch(e:distance(), 0x9C4, 0x1194, 0x1964), life)
    elseif e:kind() & 0x10 ~= 0 and e:life() < 0x32 and e:rnd() & 3 == 0 then
        e:go(1)
    else
        local off = e:angle_to_player()
        if off > 0x800 then
            off = 0x1000 - off
        end
        local side = branch(off, 0x100, 0x200, 0x600)
        local range = branch(e:distance(), 0x9C4, 0x1194, 0x1964)
        if range == 0 then
            decide_near(e, side)
        elseif range == 1 then
            decide_middle(e, side, life)
        elseif range == 2 then
            decide_far(e, side, life)
        else
            decide_farthest(e, side, life)
        end
    end
    if e:routine(1) ~= 0xF then
        mv(e)
    end
    lower_flag(e, FLAG_KNOWS)
end

local function chase(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 4)
        e:clip(1, 0, 0xF)
        set_r2(e, 1)
    elseif state ~= 1 then
        return
    end
    e:goto_player(0x10)
    e:spd()
    e:add_speed_xz()
    if e:joint(0x100) then
        e:go(0xF)
    end
    if e:distance() > 4000 then
        e:go(0x14)
        return
    end
    if e:var(WAIT) == 0 and e:distance() < 2000 then
        e:go(9)
    end
end

local function run_at(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 4)
        e:clip(V.clips.run_at, 0, 0xF)
        e:set_var(TIMER0, 0)
        e:set_timer1(0)
        e:set_var(TURN, 0)
        set_r2(e, 1)
    elseif state ~= 1 then
        return
    end
    if e:sca_info() & 1 == 0 then
        e:set_timer1(0)
    else
        e:set_timer1(e:timer1() + 1)
    end
    local stuck = e:timer1()
    if stuck < 6 then
        if e:rnd() & 2 == 0 then
            e:goto_dest(0x32)
        else
            e:goto_player(0x32)
        end
    else
        e:set_var(TIMER0, e:var(TIMER0) + 1)
        if stuck > 0x14 then
            if stuck == 0x15 then
                e:set_var(TURN, 0)
            end
            turn_at_wall(e, 0x32)
        else
            e:goto_dest(0x32)
        end
    end
    e:spd()
    e:joint(0x100)
    e:add_speed_xz()
    e:set_var(TIMER0, e:var(TIMER0) + 1)
    if e:var(TIMER0) > 0xB3 then
        e:go(0)
        return
    end
    if e:distance() < 0x898 then
        e:go(0xF)
    end
end

-- `Em22_Move`, by `Routine_1`.
-- What it takes to fall: `Spd_y` and the height, each tick.
local function fall(e)
    local x, speed, z = e:speed()
    speed = speed + 0x48
    e:set_speed(x, speed, z)
    e:add_pos(0, speed, 0)
end

-- It's about to land.
local function landing(e)
    local _, speed = e:speed()
    local _, y = e:pos()
    return speed + 0x48 + y >= 0
end

-- `FUN_00465bf0`'s answers that aren't a side of the room.
local AHEAD_CLEAR <const> = -1
local AHEAD_BLOCKED <const> = -2
local AHEAD_END <const> = -3

-- `FUN_00465bf0`: what's ahead of it along its wall.
local function along_wall(e)
    local x, y, z = e:pos()
    local ahead_x, _, ahead_z = e:super_rotate(e:angle(), 2200, 0, 0)
    e:set_var(TIMER2, ((e:direction(0, 0, ahead_x, ahead_z) + 0x100) >> 10) & 3)
    local at_x, at_z = x + ahead_x, z + ahead_z
    local floor = quot(-y, 1800)
    if #e:sca_list(at_x, at_z, floor, 0x200) == 0 then
        return AHEAD_END
    end
    local timer3 = e:var(TIMER3)
    local answer = AHEAD_CLEAR
    for _, record in ipairs(e:sca_list(at_x, at_z, floor, 0x600)) do
        local _, _, _, _, id = e:sca_rect(record)
        if id & 0x200 == 0 then
            answer = AHEAD_BLOCKED
        else
            local side = e:sca_side(record, at_x, at_z)
            if record == e:var(WALL) then
                local edge = e:sca_edge(record, timer3)
                if timer3 & 1 == 0 then
                    e:add_pos(edge - x, 0, 0)
                    x = edge
                else
                    e:add_pos(0, 0, edge - z)
                    z = edge
                end
            else
                e:set_var(WALL, record)
                if (timer3 ~ side) & 1 ~= 0 then
                    return side
                end
            end
        end
    end
    return answer
end

-- The end of a turn around a corner of its wall: its motion leaves it `by` along
-- the new side, on the edge of the record.
local function corner_end(e, side, turn, by)
    local by_x, by_z = e:rotate(e:var(TIMER2) * 0x400 + turn, by, 0)
    e:super_add(0, turn, 0)
    local x, _, z = e:pos()
    local edge = e:sca_edge(e:var(WALL), side)
    if side & 1 == 0 then
        e:add_pos(edge - x, 0, by_z)
    else
        e:add_pos(by_x, 0, edge - z)
    end
    e:set_var(TIMER3, side)
    e:set_be_flg(e:be_flg() & ~BE_NO_PUSH)
    set_r2(e, 0)
end

local function walk_wall(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:clip(1, 0, 0xF)
        e:set_var(GUN_AREA, 4)
        set_r2(e, 1)
        state = 1
    end
    if state == 1 then
        local ahead = along_wall(e)
        if spots(e) or ahead == AHEAD_BLOCKED then
            e:go(0xD)
            return
        end
        if ahead == AHEAD_CLEAR then
            e:spd()
            e:joint(0x100)
            e:add_speed_super(0, 0)
            return
        end
        e:set_be_flg(e:be_flg() | BE_NO_PUSH)
        if ahead == AHEAD_END then
            state = 5
        else
            e:set_var(NEXT_SIDE, ahead)
            state = 3
        end
        set_r2(e, state)
    end
    if state == 3 then
        e:clip(0x11, 0, 0xF)
        set_r2(e, 4)
        state = 4
    elseif state == 5 then
        e:clip(0x10, 0, 0xF)
        set_r2(e, 6)
        state = 6
    end
    if state == 4 then
        if e:joint(0x100) then
            local side = e:var(NEXT_SIDE)
            local timer2 = e:var(TIMER2)
            local turn = (timer2 * 2 - e:var(TIMER3) - side) * 0x400
            corner_end(e, side, turn, 0x69D)
            raise_flag(e, FLAG_NO_SCA)
        end
    elseif state == 6 then
        if e:joint(0x100) then
            corner_end(e, e:var(TIMER2) & 3, (e:var(TIMER2) - e:var(TIMER3)) * 0x400, 0x670)
        end
    end
end

-- Whether it turns the other way to come down: the original's division by three.
local function comes_down_reversed(e)
    local turn = e:var(TIMER3) - e:var(TIMER2)
    if turn > 1 or turn < -1 then
        turn = quot((turn * 0x55555555) // 0x100000000 - turn, 2)
    end
    return turn < 0
end

local function come_down(e)
    local state = r2(e)
    if state == 0 then
        e:set_var(TIMER0, 0)
        e:set_timer1(0)
        e:set_be_flg(e:be_flg() | BE_NO_PUSH | BE_NO_SCA)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        local reversed = comes_down_reversed(e)
        local _, y = e:pos()
        local ground = e:ground()
        if y < ground - 0x903 then
            e:clip(0x1B, 0, 0xF)
            if reversed then
                e:set_timer1(1)
            end
            set_r2(e, 3)
            atd_set(e)
            state = 3
        else
            local x, _, z = e:speed()
            if y > ground - 0x631 or y < ground - 0x66D then
                local left = ground - y - 0x64F
                e:set_timer1(left)
                e:set_speed(x, quot(left, 0x16), z)
            else
                e:set_speed(x, 0, z)
                e:set_pos_y(ground - 0x64F)
            end
            e:clip(reversed and 0x1A or 0xC, 0, 0xF)
            set_r2(e, 1)
            state = 1
        end
    end
    if state == 1 then
        local left = e:timer1()
        if left > 0x14 or left < -0x14 then
            local _, speed = e:speed()
            e:set_timer1(left - speed)
            e:add_pos(0, speed, 0)
        end
        if e:joint(0x100) then
            local by_x, by_z = e:rotate(e:var(TIMER3) << 10, 0x64F, 0)
            e:add_pos(by_x, 0, by_z)
            e:set_pos_y(0)
            e:init_super()
            e:set_angle(e:var(TIMER2) << 10)
            e:set_var(WALL, 0)
            set_r2(e, 2)
        end
    elseif state == 2 then
        e:set_move_cnt(0)
        mot_play_spxz(e, 3, 3)
        raise_flag(e, FLAG_UP)
        e:set_var(TIMER0, e:var(TIMER0) + 1)
        e:set_var(GUN_AREA, 4)
        e:set_be_flg(e:be_flg() & ~(BE_NO_PUSH | BE_NO_SCA))
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        if e:var(TIMER0) > 3 then
            e:go(0xE)
        end
    elseif state == 3 then
        e:spd()
        local reversed = e:timer1() ~= 0
        if e:joint(0x100, reversed) then
            set_r2(e, 0)
        end
        e:add_speed_super(reversed and 0x800 or 0, 0)
    end
end

-- The common turn from hanging to on its feet: `Pos_y` is the head's, and it
-- lands looking the way it fell.
local function drop_turn(e)
    e:add_pos(0, 0xA8C, 0)
    local x, _, z = e:super_rotate(e:angle(), 0x898, 0, 0)
    e:set_angle((e:direction(0, 0, x, z) + 0x100) & 0xC00)
    e:init_super()
end

local function hang(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_UP)
        e:set_var(WALL, 0)
        e:clip(3, 0, 0)
        e:set_var(GUN_AREA, 0)
        local x, _, z = e:speed()
        e:set_speed(x, 0, z)
        set_r2(e, 1)
        state = 1
    end
    local function drops()
        e:clip(V.clips.drop, 0, 0)
        e:set_var(TIMER0, e:var(TIMER0) + 1)
        set_r2(e, r2(e) + 1)
    end
    if state == 1 then
        e:joint(0x1000)
        if V.em24 and e:kind() & 0xF == 1 then
            -- `Em_24`'s first kind waits for its script.
            if e:sce_free(0) == 0 then
                e:set_var(TIMER0, e:var(TIMER0) + 1)
                return
            end
        elseif e:var(AWARE) ~= 2 then
            local target = spots(e)
            e:set_var(TARGET, target and 1 or 0)
            if not target then
                e:set_var(TIMER0, e:var(TIMER0) + 1)
                return
            end
        end
        e:set_var(TIMER0, 0)
        e:set_var(GUN_AREA, 5)
        e:clip(V.clips.let_go, 0, 0)
        set_r2(e, 2)
    elseif state == 2 then
        fall(e)
        if e:joint(0x1000) then
            drops()
        else
            e:set_var(TIMER0, e:var(TIMER0) + 1)
        end
    else
        if state == 3 then
            drop_turn(e)
            set_r2(e, 4)
            state = 4
        end
        if state == 4 then
            fall(e)
            e:clip(V.clips.drop, 0, 0)
            e:joint(0x1000)
            if landing(e) then
                drops()
            else
                e:set_var(TIMER0, e:var(TIMER0) + 1)
            end
        elseif state == 5 then
            e:set_var(GUN_AREA, 4)
            e:set_pos_y(0)
            if e:joint(0x1000) then
                e:go(0)
            end
        end
    end
end

-- `Routine_1` 0x17: on the ceiling until a script says.
local function hang_scripted(e)
    local state = r2(e)
    if state == 0 then
        e:set_var(WALL, 0)
        e:set_sce_free(0, 0)
        e:clip(3, 0, 0)
        e:joint(0x1000)
        raise_flag(e, FLAG_UP)
        e:set_kind(e:kind() | 0x4000)
        e:set_var(GUN_AREA, 0)
        local x, _, z = e:speed()
        e:set_speed(x, 0, z)
        set_r2(e, 1)
        state = 1
    end
    local function drops()
        e:clip(V.clips.drop, 0, 0)
        e:set_var(TIMER0, e:var(TIMER0) + 1)
        set_r2(e, r2(e) + 1)
    end
    if state == 1 then
        if e:sce_free(0) ~= 0 then
            e:set_kind(e:kind() & ~0x4000)
            e:set_var(TIMER0, 0)
            e:set_var(GUN_AREA, 5)
            e:clip(V.clips.let_go, 0, 0)
            set_r2(e, 2)
            return
        end
    elseif state == 2 then
        fall(e)
        if e:joint(0x1000) then
            drops()
            return
        end
    elseif state == 3 or state == 4 then
        if state == 3 then
            drop_turn(e)
            set_r2(e, 4)
        end
        fall(e)
        e:clip(V.clips.drop, 0, 0)
        e:joint(0x1000)
        if landing(e) then
            drops()
            return
        end
    elseif state == 5 then
        e:set_var(GUN_AREA, 4)
        e:set_pos_y(0)
        if e:joint(0x1000) then
            e:go(0)
        end
    end
    e:set_var(TIMER0, e:var(TIMER0) + 1)
end

-- `Routine_1` 0x10: down from the ceiling, at a script's word, and up again.
local function dropped_by_script(e)
    local state = r2(e)
    if state == 0 then
        e:set_move_cnt(0)
        mot_play_spxz(e, 3, 0)
        if e:sce_free(0) ~= 0 then
            e:set_kind(e:kind() & ~0x4000)
            e:set_sce_free(0, 0)
            set_r2(e, 1)
            local x, _, z = e:speed()
            e:set_speed(x, 0, z)
        end
    elseif state == 1 then
        fall(e)
        e:step(mot_play(e, 0x12, 0, false))
    elseif state == 2 or state == 3 then
        if state == 2 then
            drop_turn(e)
            e:clip(0x13, 0, 0)
            set_r2(e, 3)
        end
        fall(e)
        e:set_move_cnt(0)
        e:joint(0x100)
        if landing(e) then
            e:set_var(GUN_AREA, 3)
            atd_set(e)
            e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
            e:set_move_no(0)
            set_r2(e, 4)
        end
    elseif state == 4 then
        e:set_pos_y(0)
        e:step(mot_play(e, 0x13, 0, false))
    elseif state == 5 then
        mot_play_spxz(e, 3, 3)
        raise_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 4)
        if e:sce_free(0) ~= 0 then
            e:go(0xE)
        end
    end
end

-- `Routine_1` 0x11: on the ceiling, it drops as it is at a script's word.
local function dropped_at_word(e)
    local state = r2(e)
    if state == 0 then
        e:set_move_cnt(0)
        mot_play_spxz(e, 3, 0)
        if e:sce_free(0) ~= 0 then
            e:set_kind(e:kind() & ~0x4000)
            e:set_sce_free(0, 0)
            set_r2(e, 1)
            local x, _, z = e:speed()
            e:set_speed(x, 0, z)
            e:clip(0x13, 0, 0xF)
        end
    elseif state == 1 then
        fall(e)
        e:set_move_cnt(0)
        e:joint(0x100)
        if landing(e) then
            e:set_var(GUN_AREA, 3)
            e:sound(2)
            atd_set(e)
            e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
            e:set_move_no(0)
            set_r2(e, 2)
        end
    elseif state == 2 then
        e:set_pos_y(0)
        e:step(mot_play(e, 0x13, 0, false))
    elseif state == 3 then
        mot_play_spxz(e, 3, 3)
        raise_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 4)
        if e:sce_free(0) ~= 0 then
            e:go(0xE)
        end
    end
end

-- `Routine_1` 0x12: a script's walk: still, then its own steps, and each word
-- moves it on.
local function walk_by_script(e)
    local state = r2(e)
    if state == 0 then
        raise_flag(e, FLAG_QUIET)
        e:set_move_cnt(0)
        mot_play(e, 0, 0, false)
        if e:sce_free(0) ~= 0 then
            e:set_sce_free(0, 0)
            e:set_move_no(0xFF)
            set_r2(e, 1)
        end
    elseif state == 1 then
        mot_play_sp(e, 0, 0)
        if e:sce_free(0) ~= 0 then
            set_r2(e, 2)
        end
    end
end

-- `Routine_1` 0x16: on a wall until a script says, and down it.
local function leap_by_script(e)
    local state = r2(e)
    if state == 0 then
        e:set_be_flg(e:be_flg() | BE_NO_SCA)
        mot_play(e, 0xE, 0, false)
        e:set_move_no(0)
        e:set_move_cnt(0)
        if e:sce_free(0) ~= 0 then
            set_r2(e, 1)
        end
    elseif state == 1 then
        if e:move_cnt() > 4 then
            e:approach_y(0, 100)
        end
        if mot_play_sp(e, 0xE, 0) then
            e:go(5)
            e:set_kind(e:kind() & 0x0FFF)
            e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
        end
    end
end

-- `WORD_00527a58`: the turn up a surface, by its axis and the side it's going to.
local CLIMB_TURNS <const> = {
    [0] = { 0, 0, -1024 }, { -1024, 0, 0 }, { 0, 0, 1024 }, { 1024, 0, 0 },
    { 0, 0, 1024 }, { 1024, 0, 0 }, { 0, 0, -1024 }, { -1024, 0, 0 },
    { 0, -1024, 0 }, { 0, 0, -1024 }, { 0, 1024, 0 }, { 0, 0, 1024 },
    { 0, 1024, 0 }, { 0, 0, 1024 }, { 0, -1024, 0 }, { 0, 0, -1024 },
    { -1024, 0, 0 }, { 0, -1024, 0 }, { 1024, 0, 0 }, { 0, 1024, 0 },
    { 1024, 0, 0 }, { 0, 1024, 0 }, { -1024, 0, 0 }, { 0, -1024, 0 },
}

-- `FUN_004654f0`: whether there's a surface to climb onto ahead of it, and the
-- wall if it's one. On a wall, it looks for the ceiling or the floor.
local function surface_ahead(e)
    local x, y, z = e:pos()
    local by_x, by_y, by_z = e:super_rotate(e:angle(), 0x898, 0, 0)
    x, y, z = x + by_x, y + by_y, z + by_z
    if e:super_plane() == 0 then
        local wall = e:sca_list(x, z, e:floor(), 0x200)[1]
        return wall ~= nil, wall
    end
    local over, under = e:sca_hit(x, y, z, 0x600)
    if over < y then
        e:set_var(DEST_Y, over)
        return true
    end
    if y < under then
        e:set_var(DEST_Y, under)
        return true
    end
    return false
end

-- `FUN_00465610`: where the climb ends.
local function climb_to(e, wall)
    local x, y, z = e:pos()
    local side = 0
    local flags = e:flags()
    if wall then
        e:set_var(WALL, wall)
        local edge_x, _, edge_z = e:sca_onto_edge(wall, x, y, z)
        side = e:sca_face(wall, x, y, z)
        local by_x, by_z = e:rotate(side << 10, 0x8A2, 0)
        e:set_var(DEST_X, edge_x + by_x)
        e:set_var(DEST_Z, edge_z + by_z)
        flags = (flags & ~FLAG_ARRIVED) | FLAG_ARRIVED_Y
    else
        e:set_var(DEST_X, x)
        e:set_var(DEST_Z, z)
        local plane = e:super_plane()
        local to = e:var(DEST_Y)
        if plane == 1 then
            if y < to then
                side = 1
                e:set_var(DEST_Y, to - 0x898)
            else
                side = 3
                e:set_var(DEST_Y, to + 0x898)
            end
        elseif plane == 2 then
            if to <= y then
                e:set_var(DEST_Y, to + 0x898)
            else
                e:set_var(DEST_Y, to - 0x898)
                side = 2
            end
        end
        flags = (flags & ~FLAG_ARRIVED_Y) | FLAG_ARRIVED
    end
    e:set_flags(flags)
    e:set_var(NEXT_SIDE, side ~ 2)
    e:set_be_flg(e:be_flg() | BE_NO_SCA)
end

-- `FUN_00465790`.
local function climb_step(e)
    if not has_flag(e, FLAG_ARRIVED) then
        if e:approach(e:var(DEST_X), e:var(DEST_Z), 0x30) then
            raise_flag(e, FLAG_ARRIVED)
        end
    end
    if not has_flag(e, FLAG_ARRIVED_Y) then
        if e:approach_y(e:var(DEST_Y), 0x30) then
            raise_flag(e, FLAG_ARRIVED)
        end
    end
    e:goto_angle_super(e:var(NEXT_SIDE) << 10, 0x20)
end

-- `FUN_00465820`.
local function climb_end(e)
    local x, y, z = e:super_rotate(e:angle(), 0x898, -0x69D, 0)
    local here_x, _, here_z = e:pos()
    e:add_pos(e:var(DEST_X) + x - here_x, y, e:var(DEST_Z) + z - here_z)
    local turn = CLIMB_TURNS[e:super_axis() * 4 + e:var(NEXT_SIDE)]
    e:super_add(turn[1], turn[2], turn[3])
    e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
end

-- Each wall, by how far the point is from its nearest side.
local function nearest_wall(e)
    local x, _, z = e:pos()
    local best, nearest = 0x7FFFFFFF, nil
    for _, record in ipairs(e:sca_all(0xF, 0x200)) do
        local left, near, width, depth = e:sca_rect(record)
        local side = e:sca_side(record, x, z)
        local distance
        if side == 0 then
            distance = x - width - left
        elseif side == 1 then
            distance = near - z
        elseif side == 2 then
            distance = left - x
        else
            distance = z - depth - near
        end
        if distance < 0 then
            distance = -distance
        end
        if distance < best then
            best, nearest = distance, record
        end
    end
    return nearest
end

-- `Routine_1` 1: wounded, it goes up the nearest wall.
local function climb(e)
    local state = r2(e)
    if state == 0 then
        e:set_move_no(1)
        set_r2(e, 1)
        state = 1
    end
    if state == 1 then
        if not has_flag(e, FLAG_UP) then
            e:step(mot_play(e, 6, 0xF, false))
            return
        end
        set_r2(e, 2)
        state = 2
    end
    if state == 2 then
        raise_flag(e, FLAG_UP)
        e:set_var(GUN_AREA, 4)
        local record = nearest_wall(e)
        local left, near, width, depth = e:sca_rect(record)
        e:set_var(DEST_X, (width >> 1) + left)
        e:set_var(DEST_Z, (depth >> 1) + near)
        e:set_move_cnt(1)
        e:set_var(TIMER0, 0)
        set_r2(e, 3)
        state = 3
    end
    if state == 3 then
        e:goto_super(e:var(DEST_X), 0, e:var(DEST_Z), 0x40)
        if e:sca_info() & 1 ~= 0 then
            e:set_timer1(e:timer1() + 1)
        end
        if e:var(TIMER0) > 0xB3 then
            e:routines(1, 0, 0, 0)
        end
        local found, wall = surface_ahead(e)
        if found then
            climb_to(e, wall)
            raise_flag(e, FLAG_FALLING | FLAG_CLIMBING)
            e:set_be_flg(e:be_flg() | BE_NO_PUSH)
            set_r2(e, r2(e) + 1)
        end
        mot_play_sp(e, 0, 0xF)
    elseif state == 4 then
        if mot_play(e, 0x11, 0xF, false) then
            climb_end(e)
            lower_flag(e, FLAG_FALLING)
            set_r2(e, 5)
        else
            climb_step(e)
        end
    elseif state == 5 then
        local found, wall = surface_ahead(e)
        if found then
            lower_flag(e, FLAG_UP)
            e:set_be_flg(e:be_flg() & ~BE_NO_PUSH)
            e:set_var(GUN_AREA, 1)
            climb_to(e, wall)
            set_r2(e, 6)
        end
        mot_play_sp(e, 0, 0xF)
    elseif state == 6 then
        if mot_play(e, 0x11, 0xF, false) then
            climb_end(e)
            lower_flag(e, FLAG_CLIMBING)
            e:go(0x15)
        else
            climb_step(e)
        end
    end
end

-- `Routine_1` 0x15: across the ceiling, until it's over the player.
local function cross_ceiling(e)
    local state = r2(e)
    if state == 0 then
        e:set_move_no(1)
        set_r2(e, 1)
        e:set_var(TIMER0, 0)
        e:set_var(TURN, 0)
    elseif state ~= 1 then
        return
    end
    e:set_var(TIMER0, e:var(TIMER0) + 1)
    local x, y, z = e:player_pos()
    e:goto_super(x, y, z, 0x40)
    mot_play_sp(e, 0, 0xF)
    if e:distance() < 0x9C4 or e:var(TIMER0) > 0xFF then
        e:set_var(AWARE, 2)
        e:go(0xB)
    end
end

local MOVES <const> = {
    [0] = idle,
    [1] = climb,
    [2] = turn_to,
    [3] = get_up,
    [4] = get_down,
    [5] = hesitate,
    [6] = back_off,
    [7] = walk_wall,
    [8] = tongue_attack,
    [9] = slash,
    [0xA] = jump,
    [0xB] = hang,
    [0xC] = stalk,
    [0xD] = come_down,
    [0xE] = listen,
    [0xF] = decide,
    [0x10] = dropped_by_script,
    [0x11] = dropped_at_word,
    [0x12] = walk_by_script,
    [0x13] = chase,
    [0x14] = run_at,
    [0x15] = cross_ceiling,
    [0x16] = leap_by_script,
    [0x17] = hang_scripted,
}

-- The entries `Em_24`'s table leaves empty: it doesn't climb or drop by script.
local MISSING_24 <const> = {
    [1] = true, [7] = true, [0xD] = true, [0x10] = true, [0x11] = true,
    [0x12] = true, [0x15] = true, [0x16] = true,
}

mv = function(e)
    local routine = e:routine(1)
    local move = MOVES[routine]
    if move and not (V.em24 and MISSING_24[routine]) then
        move(e)
    end
end

-- It's on its feet on the floor, not on a wall or in the air.
local function on_floor(e)
    local _, y = e:pos()
    return e:var(WALL) == 0 and e:ground() <= y
end

-- `FUN_00467040`, and `Em_24`'s `FUN_0046ddc0`: no jumping or climbing case,
-- and no draw for the knock down.
local function pick_damage(e)
    if has_flag(e, FLAG_FALLING) then
        e:set_damage_no(0)
        return
    end
    if V.em24 and not on_floor(e) then
        e:set_damage_no(5)
        return
    end
    if not V.em24 and has_flag(e, FLAG_JUMPING) then
        e:set_damage_no(7)
        return
    end
    if not on_floor(e) then
        e:set_damage_no(has_flag(e, FLAG_CLIMBING) and 5 or 2)
        return
    end
    local behind = e:cdir_player()
    local weapon = e:var(WEAPON)
    if weapon == 1 or weapon == 2 or weapon == 3 or weapon == 0xD or weapon == 0x10
        or weapon == 0x12 or weapon == 0x13
    then
        e:set_damage_no(0)
    elseif (weapon >= 5 and weapon <= 9) or weapon == 0xB then
        if V.em24 or e:rnd() & 0xF > 4 then
            e:set_damage_no(behind and 4 or 3)
        else
            e:set_damage_no(1)
        end
    elseif weapon == 4 or weapon == 0xC or weapon == 0xE or weapon == 0xF then
        e:set_damage_no(1)
    elseif weapon == 0xA or weapon == 0x11 then
        local flg = e:damage_flg() & ~0x20
        if behind then
            flg = flg | 0x20
        end
        e:set_damage_flg(flg)
        e:set_damage_no(6)
    end
end

-- Case 0: it barely notices.
local function hurt(e)
    if e:life() < 0 then
        e:routines(3, 0, 0, 0)
        return
    end
    e:restore_routine()
    e:sound(7)
    hit_blood(e, false)
    if e:routine(1) ~= 0xFF then
        raise_flag(e, FLAG_KNOWS)
        mv(e)
        lower_flag(e, FLAG_KNOWS)
    end
end

-- `Spd_x`, as a speed along the shooter's facing.
local function along_shot(e, speed)
    local x, z = e:rotate(e:player_angle(), speed, 0)
    e:set_speed(x, 0, z)
end

-- `Goto00_ck` with the player as its subject: how far the player must turn to
-- face the licker. `FUN_004675f0`.
local function after_hurt(e)
    local state
    if e:distance() < 0x9C4 then
        if e:player_turn_here(0x140) == 0 then
            raise_flag(e, FLAG_KNOWS)
            state = 0x13
        elseif e:rnd() & 1 == 0 then
            state = 0xF
        else
            state = 0xC
        end
    elseif e:player_turn_here(0x1E0) == 0 then
        state = 0x14
    else
        state = 0xC
    end
    e:go(state)
end

-- Cases 1 and 6, which go by the side the shot came from.
local function knocked(e)
    local state = r3(e)
    if state == 0 then
        local behind = e:damage_flg() & 0x20 ~= 0
        e:sound(7)
        hit_blood(e, behind)
        e:set_speed(100, 0, 0)
        local shooter = e:player_angle()
        e:goto_angle(behind and shooter or shooter + 0x800, 0x80)
        lower_flag(e, FLAG_UP)
        set_r3(e, 1)
        e:clip(8, 0, 3)
    elseif state == 2 then
        if e:life() < 0 then
            e:routines(3, 0, 0, 0)
        else
            after_hurt(e)
        end
        return
    elseif state ~= 1 then
        return
    end
    local speed, y, z = e:speed()
    if speed > 0 then
        speed = speed - 6
        e:set_speed(speed, y, z)
    end
    local x, along = e:rotate(e:player_angle(), speed, 0)
    e:add_pos(x, 0, along)
    step_r3(e, e:joint(0x400))
end

local function flinch_slide(e)
    step_r3(e, e:joint(0x1000))
    if e:reach() > -800 then
        e:set_reach(e:reach() - 0x28)
    end
    e:set_var(GUN_AREA, 3)
    local x, _, z = e:speed()
    e:add_pos(x, 0, z)
end

-- Case 3: thrown on its back.
local function flinch(e)
    local state = r3(e)
    if state == 0 then
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:sound(7)
        hit_blood(e, false)
        along_shot(e, 0x32)
        e:goto_angle(e:player_angle() + 0x800, 0x80)
        local draw = e:rnd()
        raise_flag(e, FLAG_UP)
        e:set_var(TIMER0, draw & 0x11)
        e:clip(9, 0, 0)
        set_r3(e, 1)
        flinch_slide(e)
    elseif state == 1 then
        flinch_slide(e)
    elseif state == 2 then
        if e:life() < 0 then
            set_r3(e, 0)
            set_r2(e, r2(e) + 1)
            return
        end
        e:set_var(TIMER0, e:var(TIMER0) - 1)
        if e:var(TIMER0) < 1 then
            e:clip(0x16, 0, 3)
            e:set_var(TIMER0, 0)
            set_r3(e, 3)
        end
    elseif state == 3 then
        e:set_var(TIMER0, e:var(TIMER0) + 1)
        if e:reach() < -500 then
            e:set_reach(e:reach() + 0x50)
        end
        e:spd()
        if e:joint(0x400) then
            e:set_reach(-500)
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
            e:set_var(GUN_AREA, 3)
            after_hurt(e)
        end
        e:add_speed_xz()
    end
end

local function reared_slide(e)
    local ended = e:joint(0x400)
    e:set_var(TIMER0, e:var(TIMER0) + 1)
    step_r3(e, ended)
    if e:var(TIMER0) == 4 then
        along_shot(e, 0x32)
    end
    local x, _, z = e:speed()
    e:add_pos(x, 0, z)
end

local function reared_up(e)
    if e:joint(0x100, true) then
        e:set_var(GUN_AREA, 3)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        after_hurt(e)
    end
end

-- Case 4: back on its hind legs.
local function reared(e)
    local state = r3(e)
    if state == 0 then
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:sound(7)
        hit_blood(e, true)
        along_shot(e, 100)
        e:goto_angle(e:player_angle(), 0x80)
        raise_flag(e, FLAG_UP)
        e:set_var(TIMER0, 0)
        e:clip(0xA, 0, 3)
        set_r3(e, 1)
        reared_slide(e)
    elseif state == 1 then
        reared_slide(e)
    elseif state == 2 then
        if e:life() < 0 then
            set_r3(e, 0)
            set_r2(e, r2(e) + 1)
            return
        end
        raise_flag(e, FLAG_UP)
        e:clip(5, 0, 0xF)
        set_r3(e, 3)
        reared_up(e)
    elseif state == 3 then
        reared_up(e)
    end
end

-- Case 7: shot in the air.
local function shot_down(e)
    local state = r3(e)
    if state == 0 then
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:sound(7)
        hit_blood(e, false)
        raise_flag(e, FLAG_UP)
        e:set_speed(0, 0, 0)
        e:clip(V.clips.shot_down, 0, 0)
        e:add_pos(0, 0x546, 0)
        set_r3(e, 1)
        state = 1
    end
    if state == 1 then
        e:joint(0x1000)
        e:set_move_cnt(0)
        local _, y = e:pos()
        if e:var(LEFT_Y) <= y then
            atd_set(e)
            e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
            set_r3(e, 2)
            e:clip(V.clips.shot_down, 0, 0)
            state = 2
        end
    end
    if state == 2 then
        step_r3(e, e:joint(0x400))
    elseif state == 3 then
        if e:life() < 0 then
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
            e:set_var(GUN_AREA, 3)
            e:set_be_flg(e:be_flg() & ~(BE_NO_PUSH | BE_NO_SCA))
            set_r2(e, r2(e) + 1)
        else
            e:clip(V.clips.shot_up, 0, 0)
            set_r3(e, 4)
            step_r3(e, e:joint(0x1000))
        end
    elseif state == 4 then
        step_r3(e, e:joint(0x1000))
    elseif state == 5 then
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:set_var(GUN_AREA, 3)
        e:set_be_flg(e:be_flg() & ~(BE_NO_PUSH | BE_NO_SCA))
        after_hurt(e)
    end
    local ground = e:var(LEFT_Y)
    local _, y = e:pos()
    local x, speed, z = e:speed()
    if speed + 0x48 + y < ground then
        speed = speed + 0x30
        e:set_speed(x, speed, z)
        e:add_pos(x, speed, z)
    else
        e:set_pos_y(ground)
    end
end

-- The landing after a hit on a wall: flat out, and over, unless that ends it.
local function wall_hit_turns_over(e, state)
    if state == 5 then
        if e:life() < 0 then
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
            e:set_var(GUN_AREA, 3)
            e:set_be_flg(e:be_flg() & ~(BE_NO_PUSH | BE_NO_SCA))
            set_r2(e, r2(e) + 1)
            return
        end
        e:clip(0x16, 0, 0)
        set_r3(e, 6)
        step_r3(e, e:joint(0x1000))
    elseif state == 7 then
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
        e:set_var(GUN_AREA, 3)
        e:set_be_flg(e:be_flg() & ~(BE_NO_PUSH | BE_NO_SCA))
        after_hurt(e)
    else
        step_r3(e, e:joint(0x1000))
    end
end

-- It's down: the floor's boxes, and its head in the fall's last frame.
local function wall_hit_lands(e)
    e:clip(0x19, 0, 0)
    e:joint(0x400)
    local _, y = e:pos()
    if y >= 0 then
        e:set_var(WALL, 0)
        atd_set(e)
        e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
        set_r3(e, 4)
        e:clip(0x19, 0, 0)
        step_r3(e, e:joint(0x1000))
    end
end

-- Case 2: shot off a wall.
local function shot_off_wall(e)
    local state = r3(e)
    if state == 0 then
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:sound(7)
        hit_blood(e, false)
        local timer3 = e:var(TIMER3)
        local x, z = 0, 0
        if timer3 == 0 then
            x = 0x96
        elseif timer3 == 1 then
            z = -0x96
        elseif timer3 == 2 then
            x = -0x96
        else
            z = 0x96
        end
        e:set_speed(x, 0, z)
        local timer2 = e:var(TIMER2)
        if timer2 == 0 then
            timer2 = 4
        end
        e:clip(timer3 - timer2 < 0 and 0x17 or 0x18, 0, 0)
        raise_flag(e, FLAG_UP)
        set_r3(e, 1)
        e:set_var(TIMER0, 4)
        step_r3(e, e:joint(0x1000))
    elseif state == 2 or state == 3 then
        if state == 2 then
            e:add_pos(0, 0x4F5, 0)
            e:set_angle(((e:var(TIMER2) - 2) * 0x400) & 0xFFF)
            e:init_super()
            set_r3(e, 3)
        end
        wall_hit_lands(e)
    else
        wall_hit_turns_over(e, state)
    end
    local x, speed, z = e:speed()
    local _, y = e:pos()
    if speed + 0x48 + y < 0 then
        speed = speed + 0x30
        e:set_speed(x, speed, z)
        e:add_pos(x, speed, z)
    else
        e:set_pos_y(0)
    end
end

-- Case 5: shot as it climbs.
local function shot_climbing(e)
    local state = r3(e)
    if state == 0 or state == 1 then
        if state == 0 then
            e:set_damage_cnt(e:damage_cnt() | 0x80)
            e:sound(7)
            hit_blood(e, false)
            raise_flag(e, FLAG_UP)
            e:set_speed(0, 0, 0)
            e:clip(0x22, 0, 0)
            set_r3(e, 1)
            e:set_var(TIMER0, 4)
        end
        step_r3(e, e:joint(0x1000))
        if e:move_cnt() < 6 then
            local x, _, z = e:speed()
            e:set_speed(x, 0, z)
        end
    elseif state == 2 or state == 3 then
        if state == 2 then
            e:add_pos(0, 0x4F5, 0)
            local axis = e:super_axis()
            if axis == 2 then
                e:add_pos(-600, 0, 0)
            elseif axis == 3 then
                e:add_pos(600, 0, 0)
            elseif axis == 4 then
                e:add_pos(0, 0, -600)
            elseif axis == 5 then
                e:add_pos(0, 0, 600)
            end
            e:init_super()
            set_r3(e, 3)
        end
        wall_hit_lands(e)
    else
        wall_hit_turns_over(e, state)
    end
    local x, speed, z = e:speed()
    local _, y = e:pos()
    if speed + y + 0x48 < 0 then
        e:add_pos(x, speed, z)
        e:set_speed(x, speed + 0x30, z)
    else
        e:set_pos_y(e:ground())
    end
end

-- `Em22_Damage`.
local function damage(e)
    if e:routine(1) ~= 0xFF then
        e:set_var(WEAPON, e:routine(1))
        e:set_var(TARGET, 1)
        if e:tongue_owner() == e:slot() + 2 then
            e:set_tongue_owner(0)
        end
        e:set_var(AWARE, 2)
        e:set_routine(1, 0xFF)
        pick_damage(e)
        if e:damage_no() ~= 0 then
            e:set_flags(e:flags() & FLAGS_KEPT)
            e:set_be_flg(e:be_flg() & ~(BE_NO_OBA | BE_NO_PUSH | BE_NO_SCA))
        end
    end
    local kind = e:damage_no()
    if kind == 0 then
        hurt(e)
    elseif kind == 1 or kind == 6 then
        knocked(e)
    elseif kind == 2 then
        shot_off_wall(e)
    elseif kind == 5 and V.em24 then
        shot_down(e)
    elseif kind == 5 then
        shot_climbing(e)
    elseif kind == 3 then
        flinch(e)
    elseif kind == 4 then
        reared(e)
    elseif kind == 7 then
        shot_down(e)
    end
end

local function die_front(e)
    local state = r2(e)
    if state == 0 then
        if V.em24 then
            e:set_tongue_owner(0)
        end
        e:sound(8)
        hit_blood(e, false)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
        set_r2(e, 1)
        state = 1
    end
    if state == 1 then
        e:step(mot_play_sp(e, V.clips.die_front, 0))
        if e:reach() > -1000 then
            e:set_reach(e:reach() - 0x32)
        end
    end
    if state == 2 then
        set_r2(e, 3)
        state = 3
    end
    if state == 3 and mot_play(e, 0xB, 0xF, false) then
        e:set_fading(1)
        set_r2(e, 4)
    end
end

local function die_back(e)
    local state = r2(e)
    if state == 0 then
        e:sound(8)
        hit_blood(e, true)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
        set_r2(e, 1)
        state = 1
    end
    if state == 1 then
        e:step(mot_play_sp(e, V.clips.die_back, 0xF))
        if e:reach() < -100 then
            e:set_reach(e:reach() + 0x19)
        end
    elseif state == 2 then
        e:set_fading(1)
        set_r2(e, 3)
    end
end

local function die_in_air(e)
    local state = r2(e)
    if state == 0 then
        shot_down(e)
    elseif state == 1 then
        e:sound(8)
        set_r2(e, 2)
    elseif state == 2 and mot_play(e, 0xB, 0xF, false) then
        e:set_fading(1)
        set_r2(e, 3)
    end
end

-- A hit that ends it on a wall (cases 2 and 4): down as ever, and it stays there.
local function die_on_wall(e, shot)
    local state = r2(e)
    if state == 0 then
        shot(e)
    elseif state == 1 then
        e:sound(8)
        set_r2(e, 2)
    elseif state == 2 and mot_play(e, 0xB, 0xF, false) then
        e:set_fading(1)
        set_r2(e, 3)
    end
end

-- Case 3: while it was falling off a wall.
local function die_falling(e)
    local state = r2(e)
    if state == 0 then
        e:init_super()
        e:sound(8)
        hit_blood(e, false)
        e:set_move_no(0)
        set_r2(e, 1)
        state = 1
    end
    if state == 1 then
        e:step(mot_play(e, 9, 0, false))
    elseif state == 2 then
        set_r2(e, 3)
    elseif state == 3 and mot_play(e, 0xB, 0xF, false) then
        e:set_fading(1)
        set_r2(e, 4)
    end
end

-- `Em22_Die`.
local function die(e)
    if e:routine(1) ~= 0xFF then
        e:set_var(WEAPON, e:routine(1))
        e:set_routine(1, 0xFF)
        e:set_var(TARGET, 1)
        e:set_died()
        -- `FUN_00468320`, and `Em_24`'s `FUN_0046edc0`: off the floor is the
        -- fall from the air.
        if V.em24 then
            if on_floor(e) then
                e:set_damage_no(e:cdir_player() and 1 or 0)
            else
                e:set_damage_no(5)
            end
        elseif has_flag(e, FLAG_FALLING) then
            e:set_damage_no(3)
        elseif has_flag(e, FLAG_JUMPING) then
            e:set_damage_no(5)
        elseif on_floor(e) then
            e:set_damage_no(e:cdir_player() and 1 or 0)
        else
            e:set_damage_no(has_flag(e, FLAG_CLIMBING) and 4 or 2)
        end
    end
    local kind = e:damage_no()
    if kind == 0 then
        die_front(e)
    elseif kind == 1 then
        die_back(e)
    elseif kind == 2 then
        die_on_wall(e, shot_off_wall)
    elseif kind == 3 then
        die_falling(e)
    elseif kind == 4 then
        die_on_wall(e, shot_climbing)
    elseif kind == 5 then
        die_in_air(e)
    end
end

-- `Em22_Init` and `Em_24`'s `FUN_0046cfa0`.
local function init(e)
    e:set_damage_flg(0)
    e:set_damage_cnt(0)
    e:clip(1, 0, 0)
    e:joint(V.init_late)
    e:set_be_flg(e:be_flg() | BE_SUPER)
    e:clear_neck()
    e:set_tongue_owner(0)
    local easy = e:easy()
    local life_table = easy and V.life_easy or V.life
    local down = 0
    if e:common_a4() then
        down = easy and V.down_easy or V.down
    end
    e:set_life(life_table[e:rnd() & 0xF] - down)
    e:set_var(TARGET, 1)
    e:set_reach(-500)
    e:set_be_flg(e:be_flg() & ~(BE_NO_OBA | BE_NO_SCA))
    if V.em24 then
        -- `Sce_free0` to `Sce_free3`; the engine keeps the first two.
        e:set_sce_free(0, 0)
        e:set_sce_free(1, 0)
        e:set_var(TIMER0, 0)
    end
    local kind = e:kind()
    local starts = (kind & 0x4000 ~= 0 and V.starts_scripted) or V.starts
    e:routines(1, starts[kind & 0xF] or 0, 0, 0)
    e:init_super()
    local surface = (kind >> 8) & 0xF
    if surface == 1 then
        e:super_add(0, 0, 0x400)
        e:set_var(TIMER3, 0)
    elseif surface == 2 then
        e:super_add(0, 0, -0x400)
        e:set_var(TIMER3, 2)
    elseif surface == 3 then
        e:super_add(-0x400, 0, 0)
        e:set_var(TIMER3, 3)
    elseif surface == 4 then
        e:super_add(0x400, 0, 0)
        e:set_var(TIMER3, 1)
    elseif surface == 5 then
        e:super_add(0x800, 0x800, 0)
    end
    if surface >= 1 and surface <= 4 then
        e:set_be_flg(e:be_flg() | BE_NO_SCA)
        atd_set_up(e)
    elseif surface == 0 or surface == 5 then
        atd_set(e)
    end
    e:set_be_flg((e:be_flg() & 0xEFFFFFFF) | 0xC000000)
    e:init_parts()
end

local function run(e, v)
    V = v
    local wait = e:var(WAIT)
    if wait > 0 then
        e:set_var(WAIT, wait - 1)
    end
    local routine = e:routine(0)
    if routine == 0 then
        init(e)
    elseif routine == 1 or routine == 4 then
        mv(e)
    elseif routine == 2 then
        damage(e)
    elseif routine == 3 then
        die(e)
    end
    gunarea_chg(e)
    local after = e:var(AFTER)
    if after == AFTER_BITE then
        bite_blood(e)
    elseif after == AFTER_FLAMES then
        flames(e)
    end
end

return { run = run }
