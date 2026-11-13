-- The zombie dog, `Em_20` (entity id 0x20).
--
-- The whole state machine and its tables. The engine runs the animation and
-- collision, `Oba_ck_em` and `Sca_ck_em` after `update`, and counts `STUCK`.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and `e:set_var`
-- are the fields of the original's work that the engine doesn't read.
--
-- A partner to chase (`splayer_work`) isn't ported: the dog only goes after the
-- player. Nor are the parts that a burst dog throws, its colours in flames, or
-- the fading of a dead dog's shadow.

local AREA <const> = 0 -- `+0x218`: the guide area it's heading for.
local ANNOY <const> = 1 -- `+0x21a`: how long it's chased without biting.
local TURN_NOW <const> = 2 -- `+0x21c`: turn around at the next chance.
local WAIT_CHASE <const> = 3 -- `+0x21d`: the run before it steers again.
local AIR <const> = 4 -- `+0x219`: it's off the ground.
local BIT <const> = 5 -- `+0x21e`: 1 a bite, 2 one that took the player down.
local BLEED <const> = 6 -- `+0x21f`: how much more blood.
local LANDED <const> = 7 -- `+0x220`.
local LEAPT <const> = 9 -- `+0x222`: this is a leap at the player.
local HURT <const> = 10 -- `+0x223`: bits of the last hurt.
local TURN_RATE <const> = 11 -- `+0x224`.
local AIM_X <const> = 12 -- `+0x228`: which side of the player it comes up on.
local AIM_Z <const> = 13 -- `+0x22a`.
local NO_BITE <const> = 14 -- `+0x22c`.
local RESTLESS <const> = 15 -- `+0x22d`.
local ROAM <const> = 16 -- `+0x22e`: it wanders about the player, not at them.
local LEAP_WAIT <const> = 17 -- `+0x22f`.
local STUCK <const> = 18 -- `+0x230`, which the engine counts.
local BURST <const> = 19 -- `+0x231`.

-- `Be_flg`.
local BE_NO_OBA <const> = 2
local BE_NO_SCA <const> = 8
local BE_LOW <const> = 0x4000000
local BE_HIGH <const> = 0x8000000
local BE_AIR <const> = 0x10000000

local KIND_HELD <const> = 0x2000

-- `fg_room_enemy`.
local ROOM_HEARD <const> = 0x20
local ROOM_HAS_KILL <const> = 0x40
local ROOM_ANSWERED <const> = 0x80

local LIFE_HARD <const> = {
    [0] = 119, 85, 85, 85, 119, 70, 85, 85, 70, 85, 59, 70, 59, 85, 59, 70,
}
local LIFE_EASY <const> = {
    [0] = 57, 57, 82, 65, 82, 65, 57, 59, 65, 57, 30, 57, 30, 57, 30, 69,
}
-- `BYTE_ARRAY_00527588`: `Type`'s first state.
local FIRST_STATE <const> = {
    [0] = 0, 1, 2, 4, 10, 12, 14, 13, 15,
}
local SNIFF_LENGTH <const> = { [0] = 7, 31, 15, 15, 15, 15, 7, 31 }
local PAUSE_SHORT <const> = { [0] = 10, 20, 30, 20, 30, 20, 10, 20 }
local PAUSE_LONG <const> = { [0] = 15, 30, 45, 60, 60, 45, 30, 15 }
local AIM_OFFSET <const> = { [0] = 1024, 0, -512, 0, -1024, 0, 512, 0 }
local TURN_RATES <const> = { [0] = 32, 64, 48, 64, 32, 64, 48, 64 }
local SCRATCH_CLIPS <const> = { [0] = 11, 26, 11, 26, 26, 26, 11, 26 }
local SHOVE_TURN <const> = { [0] = 128, -192, -128, 192, -256, 256, 256, -256 }
-- `DAT_005277e8`: a blood row's effect kind, row, spread and size. A kind's
-- `0x80` hangs it on the part's matrix, which the effects have no use for yet.
local BLOOD <const> = {
    [0] = { 0, 1, 0, 0x1000 },
    { 9, 0, 0x200, 0x800 },
    { 9, 1, 0x200, 0x800 },
    { 9, 2, 0x200, 0x800 },
    { 0, 1, 0x400, 0x1000 },
    { 0, 1, 0x200, 0x1000 },
    { 0, 21, 0, 0x400 },
    { 5, 3, 0, 0x1000 },
    { 5, 4, 0, 0x1000 },
    { 4, 15, 0, 0x1400 },
    { 4, 15, 0, 0xC00 },
    { 6, 0, 0, 0x1000 },
    { 4, 12, 0, 0x1000 },
}

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

local function has_room(e, bit)
    return e:room_enemy() & bit ~= 0
end

local function state(e, one, two, three)
    e:routines(1, one, two or 0, three or 0)
end

local function sub(e, two, three)
    e:set_routine(2, two)
    e:set_routine(3, three or 0)
end

local function spd_x(e)
    return i16(e:speed_x())
end

local function spd_y(e)
    local _, y = e:speed()
    return i16(y)
end

local function set_spd(e, x, y, z)
    local old_x, old_y, old_z = e:speed()
    e:set_speed(x or old_x, y or old_y, z or old_z)
end

local function add_gravity(e)
    set_spd(e, nil, spd_y(e) + 0x28)
end

-- `FUN_0045f5f0`.
local function speed_up(e, by, top)
    local speed = spd_x(e)
    if speed < top then
        speed = speed + by
        e:set_speed_x(speed > top and top or speed)
    end
end

-- `FUN_0045f630`.
local function slow_down(e, by, least)
    local speed = spd_x(e)
    if least < speed then
        speed = speed - by
        e:set_speed_x(speed < least and least or speed)
    end
end

-- `FUN_0045f670`: the run's clip leans into the turn.
local function lean_clip(e, turn)
    local clip = 2
    if i16(turn) > 0 then
        clip = 12
    elseif i16(turn) < 0 then
        clip = 13
    end
    if e:move_no() ~= clip then
        e:set_move_no(clip)
        if e:hokan() < 7 then
            e:set_hokan(0xF)
        end
    end
end

local function neck_or(e, bits)
    e:set_neck_flags(e:neck_flags() | bits)
end

local function neck_clear(e, bits)
    e:set_neck_flags(e:neck_flags() & ~bits)
end

local function hold_damage(e)
    e:set_damage_cnt(e:damage_cnt() | 0x80)
end

local function free_damage(e)
    e:set_damage_cnt(e:damage_cnt() & 0x7F)
end

local function run_be_flg(e)
    e:set_be_flg((e:be_flg() & ~BE_AIR) | BE_LOW)
end

-- `FUN_0045f5b0`.
local function neck(e)
    e:rot_neck()
end

-- `FUN_0045f9a0`: one of `BLEED`'s drops of blood, or what a row has instead.
-- Row 4 bleeds from the player.
local function bleed(e, part, row)
    if e:var(BLEED) == 0 then
        return
    end
    local blood = BLOOD[row]
    local spread = blood[3]
    local draw = e:rnd()
    local dir
    if row == 4 then
        dir = ((draw & (spread * 2 - 1)) + e:player_angle()) - spread
        local x, y, z = e:player_root_point(0, 0, 0)
        if x then
            e:call_effect(blood[1], blood[2], blood[4], i16(dir), x, y, z)
        end
    else
        dir = ((draw & (spread * 2 - 1)) + e:angle()) - spread
        e:effect(blood[1], blood[2], blood[4], i16(dir), part & 0x7F, 0, 0, 0)
    end
    e:set_var(BLEED, (e:var(BLEED) - 1) & 0xFF)
end

local function bleed_chance(e)
    if e:var(BLEED) == 0 and e:rnd() & 0x3F == 0 then
        e:set_var(BLEED, SNIFF_LENGTH[e:rnd() & 7])
    end
end

-- `FUN_0045fb40`.
local function floor_lost(e)
    if not e:same_floor() then
        state(e, 9)
        e:set_room_enemy(e:room_enemy() & ~ROOM_HEARD)
    end
end

-- `FUN_0045fb70`.
local function alarm(e)
    if has_room(e, ROOM_HAS_KILL) then
        state(e, 0xB)
    end
end

-- `FUN_0045edd0`: the box the dog fills, standing or lying.
local function body_box(e, standing)
    local _, at_w, _, w = e:body_box()
    if standing then
        e:set_body_box(-1000, at_w, 1000, w)
        e:set_be_flg(e:be_flg() | BE_LOW | BE_HIGH)
    else
        e:set_body_box(-500, at_w, 500, w)
        e:set_be_flg((e:be_flg() & 0xE7FFFFFF) | BE_LOW)
    end
end

-- `FUN_0045fb00`.
local function player_hurt(e, behind)
    e:pl_set_routine(2, (behind and 1 or 0) + 2, 0, 0)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
end

-- `FUN_0045f6c0`: what the dog makes of the player.
local function look(e)
    local life = e:player_life()
    if life < 0 then
        return -2
    end
    if e:distance() > 3999 or not e:same_floor() then
        return 0
    end
    if not e:player_within(e:angle(), 0x100) then
        return -1
    end
    if e:pl_damage_cnt() & 0x80 ~= 0 then
        return -4
    end
    if life < 0xD and e:cdir_player() and i16(e:var(ANNOY)) < 600 then
        return -2
    end
    return 1
end

-- `FUN_0045f7f0`: the leap's jaws.
local function bite_ck(e)
    if e:var(NO_BITE) ~= 0 then
        return 0
    end
    if e:var(BIT) ~= 0 or e:pl_damage_cnt() & 0x80 ~= 0 or not e:same_floor() then
        return e:var(BIT)
    end
    local x, z = e:jaw()
    local px, _, pz = e:player_pos()
    local reach = e:player_life() < 0xD and 700 or 1000
    local _, pos_y = e:pos()
    if not e:point_within(x, z, px, pz, reach) or pos_y <= e:ground() - 0x708 then
        return e:var(BIT)
    end
    e:set_var(ANNOY, 0)
    local result = e:pl_life_down(0xC)
    local behind = e:cdir_player()
    if result == 2 and not behind and e:player_id() ~= 0xF then
        e:set_var(BIT, 2)
        e:pl_hold_back_die()
        e:set_speed_x(0)
        e:pl_set_damage_cnt(0xFF)
        e:pl_or_be_flg(10)
        e:set_player_angle(e:angle() + 0x800)
        hold_damage(e)
        return 2
    end
    sub(e, 3, 0)
    e:set_var(BIT, 1)
    if e:player_life() >= 0 then
        player_hurt(e, behind)
    else
        e:pl_set_routine(3, 0, 0, 0)
        e:set_be_flg(e:be_flg() | BE_NO_OBA)
    end
    return 1
end

-- `FUN_0045cea0`: the crouch before a leap.
local function crouch(e, up)
    e:set_speed_x(0x41)
    set_spd(e, nil, up)
    e:set_timer(0, 8)
    run_be_flg(e)
    e:clip(0x14, 0, 7)
    e:set_var(BIT, 0)
    e:set_var(BLEED, 0)
end

-- `FUN_0045cf60`: in the air.
local function in_air(e)
    e:add_speed_xyz()
    add_gravity(e)
    local hit = bite_ck(e)
    if spd_y(e) >= 0 and hit ~= 1 then
        return -1
    end
    if e:move_cnt() ~= 0 then
        e:joint(0x200)
    end
    return hit == 1 and 1 or 0
end

local function touch_down(e)
    e:set_pos_y(e:ground())
    e:set_var(AIR, 0)
    run_be_flg(e)
    e:sound(10)
end

local function grounded(e)
    local _, y = e:pos()
    return e:ground() <= y
end

-- `FUN_0045d010`.
local function landing(e)
    if e:routine(3) == 0 then
        e:add_speed_xyz()
        add_gravity(e)
        if grounded(e) then
            touch_down(e)
            e:set_routine(3, 1)
        end
        if e:move_cnt() ~= 8 then
            e:joint(0x100)
        end
    else
        e:add_speed_xz()
        slow_down(e, 0x40, 0xF0)
        return e:joint(0x100)
    end
    return false
end

-- `FUN_0045c4e0`: padding about the room.
local function wander(e)
    local hit = 0
    local routine = e:routine(2)
    if routine == 0 then
        e:set_routine(2, 1)
        e:set_speed_x(0x1E)
        neck_or(e, 2)
        set_spd(e, nil, nil, 0)
        run_be_flg(e)
        e:set_var(AIR, 0)
        e:set_var(RESTLESS, 0)
        free_damage(e)
    elseif routine == 1 then
        e:add_speed_xz()
        if e:sca_info() & 1 ~= 0 then
            local next = (e:rnd() & 0x1F ~= 0) and 3 or 2
            e:set_routine(2, next)
            e:set_var(AREA, e:rnd_area() & 0xFF)
            if next == 2 then
                e:set_timer(1, 0x5A)
                e:clip(0, 0, 0xF)
            end
        end
    elseif routine == 2 then
        e:set_timer(1, e:timer(1) - 1)
        if e:timer(1) == 0 then
            e:set_routine(2, 3)
            e:clip(1, 0, 0xF)
        end
    elseif routine == 3 then
        hit = e:root_ck_area(e:var(AREA))
        e:goto_dest(0x10)
        e:add_speed_xz()
        if e:sca_info() & 1 == 0 then
            e:set_routine(2, 1)
        end
    end
    e:joint(0x100)
    neck(e)
    bleed_chance(e)
    bleed(e, 4, 6)
    return hit
end

local function normal_00(e)
    local hit = wander(e)
    if e:same_floor() then
        if e:distance() < 4000 or hit == 1 or has_room(e, ROOM_HEARD) then
            state(e, 1)
        elseif e:noise() & 1 ~= 0 then
            state(e, 2)
        end
    end
    alarm(e)
end

local function normal_01(e)
    if e:routine(2) == 0 then
        neck_clear(e, 2)
        e:set_routine(2, 1)
        e:set_speed_x(0x1E)
        set_spd(e, nil, nil, 0)
        e:set_timer(2, 0)
        e:set_var(AIR, 0)
        e:set_var(RESTLESS, 0)
        free_damage(e)
        run_be_flg(e)
        e:set_var(AIM_X, AIM_OFFSET[e:rnd() & 7])
        e:set_var(AIM_Z, AIM_OFFSET[e:rnd() & 7])
    end
    local px, _, pz = e:player_pos()
    e:root_ck(px + e:var(AIM_X), pz + e:var(AIM_Z))
    local x, z = e:dest()
    e:set_angle(e:angle() + e:goto_ck(x, z, 0x10))
    e:add_speed_xz()
    local within = e:player_within(e:angle(), 0x80)
    if e:distance() < 3000 then
        local timer = e:timer(2)
        e:set_timer(2, timer + 1)
        if e:noise() & 4 ~= 0 then
            e:set_timer(2, timer + 0x15)
        end
        if within and 600 < e:distance() then
            state(e, 3)
        end
    else
        if e:timer(2) ~= 0 then
            e:set_timer(2, e:timer(2) - 1)
        end
        if within and e:noise() & 4 ~= 0 then
            e:set_timer(2, e:timer(2) + 0x14)
        end
    end
    if i8(e:timer(2)) > 0x40 or e:noise() & 1 ~= 0 or has_room(e, ROOM_HEARD) then
        state(e, 2)
        e:set_timer(2, 0)
    end
    e:joint(0x100)
    neck(e)
    bleed_chance(e)
    bleed(e, 4, 6)
    floor_lost(e)
    alarm(e)
end

local CHASE <const> = {}

-- `FUN_0045c8f0`.
CHASE[0] = function(e)
    e:set_routine(2, 1)
    e:clip(2, 0, 0xF)
    e:set_var(NO_BITE, 0)
    e:set_var(AIR, 0)
    e:set_var(RESTLESS, 0)
    neck_clear(e, 2)
    free_damage(e)
    e:set_be_flg((e:be_flg() & ~BE_AIR) | BE_LOW | BE_HIGH)
    e:set_var(TURN_RATE, 0x60)
    if e:same_id() ~= 0 then
        e:set_var(TURN_RATE, TURN_RATES[e:rnd() & 7])
    end
    if e:var(ROAM) ~= 0 then
        sub(e, 3, 0)
        e:set_var(AREA, e:rnd_area() & 0xFF)
        e:set_var(NO_BITE, 1)
        e:set_var(STUCK, 0)
        e:set_timer(1, 0)
    end
end

-- `FUN_0045c9b0`.
CHASE[1] = function(e)
    speed_up(e, 0x28, 0xF0)
    local turn = 0
    local life
    if e:var(WAIT_CHASE) == 0 then
        local x, _, z = e:player_pos()
        e:root_ck(x, z)
        local dest_x, dest_z = e:dest()
        turn = e:goto_ck(dest_x, dest_z, e:var(TURN_RATE))
        if e:var(STUCK) > 0xF and turn == 0 then
            turn = e:var(TURN_RATE)
        end
        e:set_angle(e:angle() + turn)
        life = e:player_life()
    else
        e:set_var(WAIT_CHASE, e:var(WAIT_CHASE) - 1)
        if e:var(WAIT_CHASE) == 0 or e:sca_info() & 1 ~= 0 then
            e:set_var(WAIT_CHASE, 0)
            e:set_var(LANDED, 0)
        end
        life = i16(e:life())
    end
    e:add_speed_xz()
    lean_clip(e, turn)
    local seen = look(e)
    if seen ~= 0 then
        if seen >= 0 then
            state(e, 3)
            return
        end
        e:set_var(ANNOY, i16(e:var(ANNOY) + 10))
        if seen == -2 then
            state(e, 8)
            return
        end
        if seen == -4 then
            e:set_var(TURN_NOW, 1)
        end
    end
    if e:var(ANNOY) > 0x1C2 then
        if life > 0xC then
            e:set_var(ANNOY, 0)
        end
        e:set_var(TURN_NOW, 1)
    end
    if e:var(TURN_NOW) ~= 0 then
        e:set_var(TURN_NOW, 0)
        sub(e, 2, 0)
    end
end

-- `FUN_0045caf0`: wheeling around.
CHASE[2] = function(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_timer(0, 0)
    end
    local x, _, z = e:player_pos()
    e:root_ck(x, z)
    local dest_x, dest_z = e:dest()
    local turn = e:goto_ck(dest_x, dest_z, 0x60)
    e:add_speed_xz_at(-e:timer(0))
    lean_clip(e, turn)
    if e:routine(3) == 1 then
        slow_down(e, 6, 10)
        if spd_x(e) < 0xB then
            e:set_routine(3, e:routine(3) + 1)
        end
    else
        speed_up(e, 6, 0xF0)
    end
    e:set_timer(0, e:timer(0) + turn)
    e:set_angle(e:angle() + turn)
    local turned = i16(e:timer(0))
    if turned < 0 then
        turned = -turned
    end
    if turned > 0x7FF or turn == 0 then
        e:set_routine(2, 1)
    end
end

-- `FUN_0045cbc0`: circling at a distance, one of a pack.
CHASE[3] = function(e)
    if e:var(LEAP_WAIT) ~= 0 then
        e:set_var(LEAP_WAIT, e:var(LEAP_WAIT) - 1)
    end
    speed_up(e, 0x28, 0xF0)
    local hit = e:root_ck_area(e:var(AREA))
    local dest_x, dest_z = e:dest()
    local toward = e:goto_ck(dest_x, dest_z, 0x40)
    local px, _, pz = e:player_pos()
    local at_player = e:goto_ck(px, pz, 0x80)
    if e:timer(1) == 0 then
        if e:var(STUCK) > 0xF then
            e:set_timer(1, 0x1E)
            if toward == 0 then
                e:set_timer(2, -(at_player // 2))
            else
                e:set_timer(2, -toward)
            end
        end
    else
        toward = i8(e:timer(2))
        e:set_timer(1, e:timer(1) - 1)
    end
    e:set_angle(e:angle() + toward)
    e:add_speed_xz()
    lean_clip(e, toward)
    local pick = e:routine(3) == 0
    if not pick and hit == 1 then
        local x, _, z = e:pos()
        pick = e:point_within(x, z, dest_x, dest_z, 1000)
    end
    if pick then
        e:set_routine(3, 1)
        local old = e:var(AREA)
        local area = e:rnd_area() & 0xFF
        if old == area then
            area = (area + 1) & 0xFF
        end
        local count = e:area_count()
        e:set_var(AREA, count > 0 and area % count or area)
    end
    if e:var(LEAP_WAIT) == 0 then
        local seen = look(e)
        if seen ~= 0 or at_player == 0 then
            state(e, 3)
            e:set_var(LEAP_WAIT, PAUSE_LONG[e:rnd() & 7] + 0x1E)
        end
    end
end

local function normal_02(e)
    CHASE[e:routine(2)](e)
    e:joint(0x100)
    neck(e)
    floor_lost(e)
    alarm(e)
end

local LEAP <const> = {}

-- `FUN_0045cde0`.
LEAP[0] = function(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        crouch(e, -0x118)
        neck_or(e, 2)
    end
    e:add_speed_xz()
    e:joint(0x200)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        sub(e, 1, 0)
        e:set_speed_x(0x118)
        e:set_var(AIR, 1)
        e:set_var(LEAPT, 1)
        e:set_be_flg((e:be_flg() & ~BE_LOW) | BE_AIR)
        e:sound(3)
        e:set_room_enemy(e:room_enemy() | ROOM_HEARD)
    end
end

-- `FUN_0045cef0`.
LEAP[1] = function(e)
    if in_air(e) < 0 then
        if e:var(BIT) == 2 then
            state(e, 7)
            return
        end
        sub(e, 2, 0)
        e:clip(0x15, 0, 0xF)
        e:joint(0x100)
        e:set_var(ANNOY, i16(e:var(ANNOY) + 0x14))
        e:set_var(LEAPT, 0)
    end
end

-- `FUN_0045cfd0`.
LEAP[2] = function(e)
    if landing(e) then
        e:routines(1, 2, 0, 0)
        e:set_var(WAIT_CHASE, 0)
        e:set_var(BIT, 0)
    end
end

-- `FUN_0045d0e0`: it bit, and goes on past.
LEAP[3] = function(e)
    local routine = e:routine(3)
    if routine == 0 then
        e:set_speed_x(spd_x(e) >> 2)
        e:set_routine(3, 1)
        e:clip(0x16, 0, 0xF)
        e:set_timer(0, 0)
        e:set_var(BIT, 0)
        e:set_var(BLEED, 4)
    end
    if routine <= 1 then
        e:add_speed_xyz_at(0x800)
        add_gravity(e)
        if grounded(e) then
            touch_down(e)
            e:set_routine(3, 2)
        end
        if e:move_cnt() < 0x11 then
            e:joint(0x100)
        else
            e:set_speed_x(0)
        end
    elseif routine == 2 and e:joint(0x100) then
        state(e, 2)
        e:set_var(WAIT_CHASE, SNIFF_LENGTH[e:rnd() & 7])
    end
    if i16(e:timer(0)) < 0x800 then
        e:set_angle(e:angle() + 0x80)
        e:set_timer(0, e:timer(0) + 0x80)
    end
end

local function normal_03(e)
    LEAP[e:routine(2)](e)
    neck(e)
    bleed(e, 4, 0)
end

-- How near the player can come before a sleeping dog wakes.
local function wake_distance(e)
    if e:pl_routine(0) == 1 and e:pl_routine(1) == 2 then
        return 5000
    end
    return 0xDAC
end

local function wake(e)
    e:set_routine(2, 2)
    e:clip(8, 0, 0xF)
end

local SLEEP <const> = {}

SLEEP[0] = function(e)
    if wake_distance(e) < e:distance() then
        if e:noise() & 1 ~= 0 or has_room(e, ROOM_HEARD) then
            if has_room(e, ROOM_ANSWERED) then
                e:set_timer(0, PAUSE_LONG[e:rnd() & 7])
                e:set_routine(2, 1)
            else
                wake(e)
                e:set_room_enemy(e:room_enemy() | ROOM_ANSWERED)
            end
        end
    else
        wake(e)
    end
    e:joint(0x100)
end

SLEEP[1] = function(e)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        wake(e)
    end
    e:joint(0x100)
end

SLEEP[2] = function(e)
    if e:joint(0x100) then
        sub(e, 3, 0)
        e:set_kind(e:kind() & ~KIND_HELD)
        e:set_be_flg(e:be_flg() | BE_HIGH)
    end
end

SLEEP[3] = function(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_timer(0, PAUSE_SHORT[e:rnd() & 7])
        e:clip(0, 0, 0x1F)
    end
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        e:clip(1, 0, 0xF)
        state(e, 1)
    end
    e:joint(0x80)
    neck(e)
end

local function normal_04(e)
    SLEEP[e:routine(2)](e)
end

-- A growl.
local function normal_05(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(3, 0, 3)
        neck_clear(e, 2)
        e:set_var(BLEED, 0x20)
    end
    if e:joint(0x400) then
        if e:move_no() == 4 then
            e:routines(1, 2, 0, 0)
        else
            local next = e:move_no() + 1
            e:clip(next, 0, 3)
            if next == 4 then
                e:sound(4)
            end
        end
    end
    neck(e)
    bleed(e, 4, 6)
end

local function normal_06(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_speed_x(0x32)
        e:clip(9, 0, 3)
        neck_clear(e, 2)
    end
    if e:move_cnt() < 0xF then
        if 7 < e:move_cnt() then
            slow_down(e, 0x20, 0x1E)
        end
        e:add_speed_xz_at(0x800)
        local x, _, z = e:player_pos()
        e:goto_point(x, z, 0x40)
    end
    if e:joint(0x400) then
        e:routines(1, 2, 0, 0)
    end
    neck(e)
end

-- `FUN_0045d780`: done with the kill.
local function feeding_done(e)
    if e:routine(3) == 0 then
        e:spd()
        local over = e:joint(0x80)
        e:add_speed_xz()
        if over then
            e:set_routine(3, 1)
            e:set_timer(0, 2)
            e:clip(0x19, 0, 0x1F)
            e:spd()
            e:joint(0x80)
        end
    else
        e:spd()
        if e:joint(0x80) then
            e:set_timer(0, e:timer(0) - 1)
            if e:timer(0) == 0 then
                state(e, 0xC)
            end
        end
    end
    e:seq_sound()
end

-- The kill, standing over the player.
local function normal_07(e)
    if e:routine(2) ~= 0 then
        feeding_done(e)
        return
    end
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_var(LANDED, 0)
        e:set_speed_x(0)
        e:set_timer(2, 0xC)
        e:clip(0x17, 0, 0x1F)
        e:joint(0x80)
        e:set_var(BLEED, 0)
        e:pl_hold()
        e:pl_set_base()
        e:pl_set_routine(6, 0, 0, 0)
        e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        e:set_player_angle(e:angle() + 0x800)
    else
        e:pl_set_base()
        e:null_pos_set(0)
        if e:joint(0x80) then
            sub(e, 1, 0)
            e:clip(0x18, 0, 0x1F)
            e:spd()
            e:joint(0x80)
            e:set_var(BLEED, 0)
            return
        end
        if e:move_cnt() == 0x62 then
            e:set_var(BLEED, 0x12)
        end
    end
    local timer = e:timer(2)
    local partner_id = e:player_id() & 1 ~= 0
    if timer == 0 or partner_id then
        if e:var(LANDED) == 0 then
            e:set_var(LANDED, 1)
            e:pl_held_kill()
            e:set_room_enemy(e:room_enemy() | ROOM_HAS_KILL)
        end
        e:add_speed_xyz()
        add_gravity(e)
        if grounded(e) then
            e:set_pos_y(e:ground())
            set_spd(e, nil, 0)
            e:set_var(AIR, 0)
        end
    end
    if timer ~= 0 or partner_id then
        e:set_timer(2, e:timer(2) - 1)
    end
    bleed(e, 4, 0)
    bleed(e, 8, 4)
    bleed(e, 8, 4)
    e:seq_sound()
end

-- It's lost the player: off to another area.
local function normal_08(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_var(AREA, e:rnd_area() & 0xFF)
        e:set_timer(0, PAUSE_SHORT[e:rnd() & 7] + 0x1E)
        e:set_var(ANNOY, i16(e:var(ANNOY) + 300))
    end
    e:root_ck_area(e:var(AREA))
    local x, z = e:dest()
    local turn = e:goto_ck(x, z, e:var(TURN_RATE))
    e:set_angle(e:angle() + turn)
    e:add_speed_xz()
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        state(e, 2)
        e:set_var(WAIT_CHASE, 0)
    end
    lean_clip(e, turn)
    e:joint(0x100)
    neck(e)
    floor_lost(e)
end

local function normal_09(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_timer(0, 0x3C)
        e:clip(0, 0, 0xF)
    end
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        state(e, 0)
        e:clip(1, 0, 0xF)
    end
    e:joint(0x100)
    neck(e)
end

local function normal_0a(e)
    wander(e)
    if e:same_floor() and (e:distance() < 4000 or has_room(e, ROOM_HEARD) or e:noise() & 1 ~= 0) then
        state(e, 2)
    end
end

-- It's killed the player, or a partner has: it goes to look.
local function normal_0b(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:set_routine(2, 1)
        e:set_speed_x(0x1E)
        set_spd(e, nil, nil, 0)
        neck_clear(e, 2)
        e:set_var(AIR, 0)
        e:set_var(RESTLESS, 0)
        e:clip(1, e:rnd() & 0x3F, 0xF)
    end
    if routine <= 1 then
        local px, _, pz = e:player_pos()
        e:root_ck(px, pz)
        e:goto_dest(0x10)
        e:add_speed_xz()
        if (e:player_within(e:angle(), 0x100) and e:distance() < 0x9C4) or e:at_em_no() ~= 0xFF then
            e:set_routine(2, 2)
            e:clip(0, 0, 0xF)
        end
    end
    e:joint(0x100)
    neck(e)
end

-- Eating.
local function normal_0c(e)
    if e:routine(2) ~= 0 then
        SLEEP[3](e)
        return
    end
    local routine = e:routine(3)
    if routine == 0 then
        e:set_routine(3, 1)
        e:set_var(AIR, 0)
        e:set_var(RESTLESS, 0)
        e:clip(0xE, e:rnd() & 0x3F, 0x1F)
        e:spd()
        e:joint(0x80)
    elseif routine == 1 then
        if e:player_life() >= 0 then
            if wake_distance(e) < e:distance() then
                if e:noise() & 1 ~= 0 then
                    if has_room(e, ROOM_ANSWERED) then
                        e:set_routine(3, 2)
                        e:set_timer(0, PAUSE_LONG[e:rnd() & 7])
                    else
                        sub(e, 1, 0)
                        e:set_room_enemy(e:room_enemy() | ROOM_ANSWERED)
                    end
                end
            else
                sub(e, 1, 0)
            end
        end
    elseif routine == 2 then
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            sub(e, 1, 0)
        end
    end
    local seq = e:now_seq()
    if seq & 0x30000 ~= 0 then
        e:set_var(BLEED, 1)
        bleed(e, 4, 0)
        if seq & 0x20000 ~= 0 and e:rnd() & 3 == 0 then
            e:set_var(BLEED, 1)
            bleed(e, 4, (e:rnd() & 1) + 1)
        end
    end
    e:seq_sound()
    e:spd()
    e:joint(0x80)
    e:add_speed_xz()
end

-- `FUN_0045dd20`.
local function scratching(e)
    local routine = e:routine(3)
    if routine == 0 then
        e:set_routine(3, 1)
        neck_clear(e, 2)
        e:clip(SCRATCH_CLIPS[e:rnd() & 7], 0, 0xF)
        e:spd()
        e:joint(0x100)
        e:set_timer(2, (e:rnd() & 3) + 1)
    elseif routine ~= 1 then
        e:seq_sound()
        return
    end
    e:spd()
    if e:joint(0x100) then
        local left = e:timer(2) - 1
        e:set_timer(2, left)
        if left == 0 then
            sub(e, 1, 0)
        else
            e:clip(SCRATCH_CLIPS[e:rnd() & 7], 0, 0xF)
        end
    end
    if e:move_no() == 0xB then
        e:add_speed_xz()
    end
    e:seq_sound()
end

-- `FUN_0045de20`.
local function circling(e)
    local routine = e:routine(3)
    if routine == 0 then
        e:set_routine(3, 1)
        e:set_speed_x(0xF0)
        e:set_timer(0, PAUSE_SHORT[e:rnd() & 7])
        local rate = (e:rnd() & 1 ~= 0) and 0x7F or -0x7F
        if e:rnd() & 1 ~= 0 then
            rate = -rate
        end
        e:set_timer(1, rate)
        e:clip(2, 0, 0xF)
        routine = 1
    end
    if routine == 1 then
        e:set_angle(e:angle() + i16(e:timer(1)))
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            e:set_routine(3, 2)
        end
    elseif routine == 2 then
        local angle = (i16(e:timer(1)) + e:angle()) & 0xFFF
        e:set_angle(angle)
        if angle >= 0x780 and angle <= 0x880 then
            if (1 << (e:rnd() & 0xF)) & 0x1B68 == 0 then
                e:set_routine(3, 3)
                e:set_timer(1, 0)
            else
                state(e, 0xF, 0, 1)
            end
        end
    elseif routine == 3 then
        if e:sca_info() & 1 ~= 0 then
            sub(e, 0, 0)
        end
    end
    e:add_speed_xz()
    lean_clip(e, e:timer(1))
    e:joint(0x100)
end

local function normal_0d(e)
    if e:routine(2) == 0 then
        scratching(e)
    else
        circling(e)
    end
    neck(e)
end

-- `FUN_0045dfb0`: a scripted leap's crouch.
local function scripted_crouch(e)
    local routine = e:routine(3)
    if routine == 0 then
        hold_damage(e)
        neck_or(e, 2)
        e:set_var(AIR, 0)
        e:set_var(RESTLESS, 0)
        return
    end
    if routine == 1 then
        e:set_routine(3, 2)
        crouch(e, -0x140)
        neck_clear(e, 2)
    elseif routine ~= 2 then
        return
    end
    e:add_speed_xz()
    e:joint(0x200)
    e:set_timer(0, e:timer(0) - 1)
    if e:timer(0) == 0 then
        sub(e, 1, 0)
        e:set_speed_x(0x118)
        e:set_var(AIR, 1)
        e:sound(3)
    end
end

-- `switchD_0045e30d::caseD_1`.
local function scripted_air(e)
    e:set_var(NO_BITE, 1)
    if in_air(e) < 0 then
        sub(e, 2, 0)
        e:clip(0x15, 0, 0xF)
        e:joint(0x100)
    end
end

local function normal_0e(e)
    local routine = e:routine(2)
    if routine == 0 then
        scripted_crouch(e)
    elseif routine == 1 then
        scripted_air(e)
    elseif routine == 2 and landing(e) then
        e:routines(1, 2, 0, 0)
        free_damage(e)
        e:set_var(WAIT_CHASE, 0)
        e:set_var(NO_BITE, 0)
    end
    neck(e)
end

local function normal_0f(e)
    local routine = e:routine(2)
    if routine == 0 then
        scripted_crouch(e)
    elseif routine == 1 then
        scripted_air(e)
        if e:sca_info() & 1 ~= 0 then
            sub(e, 3, 0)
            e:clip(0x16, 0, 0xF)
            e:joint(0x100)
            e:set_speed_x(-(spd_x(e) >> 1))
            e:set_timer(0, 0)
            e:set_sce_free(0, 1)
        end
    elseif routine == 2 then
        if landing(e) then
            e:routines(1, 0xD, 1, 0)
            free_damage(e)
        end
    elseif routine == 3 then
        if e:routine(3) == 0 then
            e:add_speed_xyz_at(0x800)
            add_gravity(e)
            if grounded(e) then
                touch_down(e)
                e:set_routine(3, 1)
            end
            if e:move_cnt() < 0x11 then
                e:joint(0x100)
            else
                e:set_speed_x(0)
            end
        elseif e:joint(0x100) then
            e:routines(1, 0xD, 1, 0)
            free_damage(e)
        end
        if i16(e:timer(0)) < 0x400 then
            e:set_angle(e:angle() + 0x100)
            e:set_timer(0, e:timer(0) + 0x100)
        end
    end
    neck(e)
end

-- `switchD_0045e447::caseD_0`: down from wherever it was, onto the player's floor.
local function fall_to_floor(e)
    local _, y = e:pos()
    local ofs_y = e:body_box()
    if y + i16(ofs_y) <= y then
        e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
    end
    if landing(e) then
        e:routines(e:routine(0), 2, 0, 0)
        free_damage(e)
        e:set_var(WAIT_CHASE, 0)
        e:set_var(NO_BITE, 0)
        e:take_player_floor()
    end
end

local function normal_10(e)
    local routine = e:routine(2)
    if routine == 0 then
        if e:routine(3) & 0xF == 0 then
            e:set_routine(3, 1)
            crouch(e, -0x140)
            hold_damage(e)
            neck_clear(e, 2)
            e:set_be_flg(e:be_flg() | BE_NO_SCA)
            e:set_var(ROAM, 0)
        end
        e:add_speed_xz()
        e:joint(0x200)
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            sub(e, 1, 0)
            e:set_speed_x(0x118)
            e:set_var(AIR, 1)
            e:sound(3)
        end
    elseif routine == 1 then
        scripted_air(e)
    elseif routine == 2 then
        fall_to_floor(e)
    end
end

local NORMAL <const> = {
    [0] = normal_00,
    normal_01,
    normal_02,
    normal_03,
    normal_04,
    normal_05,
    normal_06,
    normal_07,
    normal_08,
    normal_09,
    normal_0a,
    normal_0b,
    normal_0c,
    normal_0d,
    normal_0e,
    normal_0f,
    normal_10,
}

-- `Em20_Hurt`, and `FUN_0045ee20`'s last steps.
local hurt_start

local function hurt_fall(e)
    e:add_speed_xyz_at(e:timer(0))
    add_gravity(e)
    if grounded(e) then
        e:set_pos_y(e:ground())
        e:set_timer(1, 0x200)
        e:set_var(AIR, 0)
        e:set_var(LANDED, 1)
        body_box(e, false)
        local draw = e:rnd()
        if (1 << (draw & 0xF)) & 0x404 == 0
            or i8(e:var(HURT)) < 0
            or i16(e:life()) < 0
            or e:var(RESTLESS) ~= 0
        then
            e:sound(0xB)
            sub(e, 2, 0)
        else
            e:sound(10)
            sub(e, 3, 0)
        end
    end
    if e:move_cnt() ~= 3 then
        e:joint(0x200)
    end
end

local function hurt_down(e)
    if e:var(LANDED) == 0 then
        e:add_speed_xz_at(e:timer(0))
        slow_down(e, 10, 0)
    end
    if e:joint(i16(e:timer(1))) then
        local routine = e:routine(3)
        if routine == 0 then
            e:set_routine(3, 1)
            e:set_var(HURT, (e:var(HURT) & 0xC2) | 2)
            e:clip(0x12, 0, e:hokan())
        elseif routine == 1 then
            e:set_routine(3, 2)
            e:clip(7, 0, 0xF)
            e:set_timer(1, 0x100)
        elseif routine == 2 then
            body_box(e, true)
            free_damage(e)
            e:set_kind(e:kind() & ~KIND_HELD)
            e:set_var(HURT, 0)
            if e:var(RESTLESS) ~= 0 then
                e:routines(1, 0xD, 1, 0)
                return
            end
            local px, _, pz = e:player_pos()
            if e:goto_ck(px, pz, 0x200) == 0 then
                e:routines(1, 5, 0, 0)
            else
                e:routines(1, 6, 0, 0)
            end
        end
    end
end

local function hurt_up(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:clip(0x16, 0xF, 3)
        free_damage(e)
    end
    if e:joint(0x400) then
        body_box(e, true)
        e:set_var(HURT, 0)
        e:routines(1, 2, 0, 0)
    end
end

local function hurt_dead(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        local draw = e:rnd()
        local life = e:easy() and LIFE_EASY[draw & 0xF] or LIFE_HARD[draw & 0xF]
        e:set_life((e:rnd() & 3) + life)
        e:set_d_life(0xD)
        e:set_be_flg(e:be_flg() & 0xE3FFFFFF)
        hold_damage(e)
    end
end

local AGAIN <const> = {}

-- `FUN_0045e900`: hit again before it's got over the last one.
AGAIN[0] = function(e)
    local one = e:routine(1)
    if one == 0xF or one == 0x12 then
        e:set_var(BLEED, e:var(BLEED) + 3)
        if e:rnd() & 3 == 0 then
            e:set_var(BLEED, e:var(BLEED) + 1)
            local row = (e:rnd() & 1) + 2
            bleed(e, e:rnd() & 0xF, row)
        end
    else
        e:set_var(BLEED, 1)
        bleed(e, 0, 0)
        e:sound(1)
    end
    if e:var(HURT) & 0x7F == 1 then
        e:set_timer(1, 0)
        sub(e, e:var(AIR) == 0 and 2 or 1, 0)
        return
    end
    sub(e, 3, 1)
    if e:var(HURT) & 0x40 == 0 then
        e:clip(0x12, 0, e:hokan())
        e:sound(1)
        e:set_var(HURT, e:var(HURT) | 0x40)
    end
    e:set_timer(2, 0)
    e:set_var(LANDED, 1)
    e:set_timer(1, 0)
end

-- `FUN_0045e9f0`.
AGAIN[1] = function(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:add_speed_xz_at(e:timer(0) - 0x100 + e:rnd() * 2)
        e:set_angle(e:angle() + e:rnd() - 0x80)
    end
    hurt_fall(e)
end

-- `FUN_0045ea50`.
AGAIN[2] = function(e)
    if e:var(LANDED) == 0 then
        e:add_speed_xz_at(e:timer(0))
        slow_down(e, 10, 0)
        if e:routine(3) == 0 then
            e:set_routine(3, 1)
            e:set_angle(e:angle() + e:rnd() - 0x80)
        end
    end
    if e:joint(i16(e:timer(1))) then
        sub(e, 3, 0)
        e:set_var(HURT, (e:var(HURT) & 0xC2) | 2)
        e:set_timer(2, 0)
        e:clip(0x12, 0, e:hokan())
    end
end

-- `FUN_0045eaf0`.
AGAIN[3] = function(e)
    if e:timer(2) == 0 then
        e:set_timer(2, 1)
        if e:routine(1) ~= 0xC then
            e:set_speed_x(0x28)
            e:add_speed_xz_at(e:timer(0))
            e:set_angle(e:angle() + e:rnd() - 0x80)
        end
    end
    hurt_down(e)
    if e:routine(3) > 1 then
        e:set_var(HURT, e:var(HURT) & 0xBF)
    end
end

hurt_start = function(e)
    local turn = (e:player_angle() & 0xFFF) - (e:angle() & 0xFFF)
    e:set_timer(0, turn + SHOVE_TURN[e:rnd() & 7])
    e:set_kind(e:kind() & ~KIND_HELD)
    neck_or(e, 2)
    if e:routine(0) == 2 then
        e:sound(1)
    end
    if e:var(HURT) ~= 0 and e:var(RESTLESS) == 0 then
        e:set_var(HURT, e:var(HURT) | 0x80)
        AGAIN[0](e)
        return
    end
    if e:var(AIR) == 0 then
        sub(e, 2, 0)
        e:set_speed_x(0xF0)
        body_box(e, false)
    else
        e:set_speed_x(spd_x(e) >> 1)
        sub(e, 1, 0)
    end
    e:set_var(LANDED, 0)
    e:set_var(HURT, 1)
    e:clip(0x11, 0, 7)
    e:set_timer(1, 0x200)
    e:joint(0x200)
    e:set_var(BLEED, 1)
    bleed(e, 0, 0)
end

local HURT_STEPS <const> = { [0] = hurt_start, hurt_fall, hurt_down, hurt_up, hurt_dead }

local function hurt_steps(e)
    local step = HURT_STEPS[e:routine(2)]
    if step then
        step(e)
    end
end

-- `switchD_0045ed5d::caseD_0` (0x45ED70): a blast throws it into the air,
-- then `hurt_start`.
local function blast_start(e)
    if e:be_flg() & (BE_HIGH | BE_AIR) ~= 0 and e:var(AIR) == 0 then
        e:set_var(AIR, 1)
        set_spd(e, nil, -140)
        e:set_speed_x(0xF0)
    end
    hurt_start(e)
end

-- `0x5276e0` has four cells: its `Routine_2` 4 reads the next table's first
-- (`0x5276f0`), the blast's start.
AGAIN[4] = blast_start

-- `switchD_0045e447::caseD_4`: a blast throws it into the air.
local function hurt_blast(e)
    if e:routine(2) == 0 then
        blast_start(e)
    else
        hurt_steps(e)
    end
end

local function hurt_bleeding(e, count)
    local done = 0
    e:set_var(BLEED, count)
    while done < e:var(BLEED) do
        local draw = e:rnd()
        bleed(e, draw & 0xF, 8)
        done = done + 1
    end
end

local function hurt(e)
    if i8(e:var(HURT)) < 0 then
        local again = AGAIN[e:routine(2)]
        if again then
            again(e)
        else
            hurt_blast(e)
        end
        bleed(e, e:rnd() & 0xF, 0)
        bleed(e, e:rnd() & 0xF, 0)
        bleed(e, e:rnd() & 0xF, 5)
        return
    end
    local one = e:routine(1)
    if one == 0 then
        fall_to_floor(e)
    elseif one == 4 or one == 0xF or one == 0x12 then
        hurt_blast(e)
    elseif one == 9 or one == 0x11 or one == 0x13 then
        hurt_bleeding(e, 2 - e:damage_no() // 3)
        e:set_routine(1, 1)
        hurt_steps(e)
    elseif one == 0xA or one == 0x10 then
        hurt_bleeding(e, 4 - e:damage_no() // 3)
        hold_damage(e)
        e:set_routine(1, 1)
        hurt_steps(e)
    elseif one == 0xB then
        local part = e:rnd() & 0xF
        e:set_var(BLEED, 1)
        bleed(e, part, 9)
        e:set_routine(1, 1)
        hurt_steps(e)
    elseif one == 0xE then
        if e:routine(2) == 0 then
            hurt_start(e)
            e:set_var(BLEED, 8)
            bleed(e, e:rnd() & 0xF, 0xB)
            e:set_timer(3, 0)
        end
        hurt_steps(e)
        local count = i8(e:timer(3) + 1)
        e:set_timer(3, count)
        if count % 3 == 0 then
            bleed(e, e:rnd() & 0xF, 0xB)
            bleed(e, e:rnd() & 0xF, 0xB)
        end
    elseif one >= 0x14 then
        HURT_STEPS[one - 0x14](e)
    else
        hurt_steps(e)
    end
end

-- `FUN_0045ee70`.
local function die_start(e)
    e:set_died()
    local was = e:var(HURT)
    e:set_var(HURT, 0)
    hurt_start(e)
    if e:var(BURST) == 0 then
        e:sound(7)
    end
    if was & 0x3F == 2 then
        e:set_speed_x(0)
    end
end

-- `switchD_0045ee60::caseD_2`.
local function die_down(e)
    if e:var(LANDED) == 0 then
        e:add_speed_xz_at(e:timer(0))
        slow_down(e, 10, 0)
    end
    if e:joint(i16(e:timer(1))) then
        if e:routine(3) == 0 then
            e:set_routine(3, 1)
            e:clip(0x12, 0, e:hokan())
        else
            e:routines(7, 0, 0, 0)
        end
    end
end

-- `switchD_0045ee60::caseD_4`: killed in the air, it can knock the player over.
local function die_leap(e)
    if e:var(AIR) ~= 0 and e:var(LEAPT) ~= 0 then
        if e:rnd() & 5 ~= 0 and e:damage_no() < 3 then
            e:set_died()
            sub(e, 1, 0)
            neck_or(e, 1)
            player_hurt(e, e:cdir_player())
            e:set_var(BLEED, 0x12)
            e:clip(0x11, 0, 7)
            e:joint(0x200)
            return
        end
    end
    e:set_routine(1, 1)
    die_start(e)
end

-- `switchD_0045ee60::caseD_5`.
local function die_leap_fall(e)
    e:add_speed_xyz()
    if e:var(AIR) == 0 then
        if e:joint(0x200) then
            if e:routine(3) == 0 then
                e:set_routine(3, 1)
                e:clip(0x12, 0, e:hokan())
            else
                e:routines(7, 0, 0, 0)
            end
        end
    else
        if e:routine(3) == 0 and e:distance() < 0x578 then
            e:set_speed_x(0)
            e:set_routine(3, 1)
        end
        add_gravity(e)
        if grounded(e) then
            e:set_pos_y(e:ground())
            set_spd(e, nil, 0)
            e:set_var(AIR, 0)
            e:set_routine(3, 0)
        end
        if e:move_cnt() ~= 3 then
            e:joint(0x200)
        end
    end
end

-- `FUN_0045f200`: a headshot.
local function die_burst(e)
    if e:routine(1) == 9 and e:damage_no() > 2 then
        die_start(e)
        return
    end
    e:set_var(BURST, 1)
    die_start(e)
    neck_or(e, 1)
    if e:routine(1) == 9 then
        e:set_var(BLEED, 1)
        bleed(e, e:rnd() & 0xF, 7)
    end
    e:set_var(BLEED, 0x12)
end

-- `FUN_0045f2a0`. Its bleeding also takes the colours of parts 0 to 16 to
-- 0x202020, which isn't ported.
local function die_many_wounds(e)
    die_start(e)
    if e:damage_no() < 3 or e:routine(1) == 0x10 then
        e:set_var(BLEED, 6)
        for _ = 1, 6 do
            bleed(e, e:rnd() & 0xF, 7)
        end
    end
end

-- `FUN_0045f310`.
local function die_burning(e)
    die_start(e)
    if e:damage_no() < 3 then
        e:set_var(BLEED, 2)
        bleed(e, e:rnd() & 7, 9)
        bleed(e, (e:rnd() & 7) + 8, 10)
    end
end

-- `FUN_0045f3f0`, the spark shot's death (`FUN_0045f390`'s first step). The
-- colour 0x3F203F it gives every part isn't ported.
local function die_acid(e)
    die_start(e)
    e:set_var(BLEED, 0x14)
    e:set_timer(3, 0)
    bleed(e, e:rnd() & 0xF, 0xB)
    bleed(e, e:rnd() & 0xF, 0xC)
    bleed(e, 1, 0xC)
end

local function bleeding_out(e)
    bleed(e, 3, 0)
    bleed(e, 2, 0)
    bleed(e, 2, (e:rnd() & 1) + 1)
end

-- `FUN_0045ee20`, by the weapon that did it.
local function die(e)
    local one = e:routine(1)
    local two = e:routine(2)
    if one == 7 or one == 8 then
        if two == 0 then
            die_leap(e)
        elseif two == 1 then
            die_leap_fall(e)
        end
        bleeding_out(e)
    elseif one == 5 or one == 6 or one == 9 or one == 0x11 or one == 0x13 then
        if two == 0 then
            die_burst(e)
        elseif two == 1 then
            hurt_fall(e)
        elseif two == 2 then
            die_down(e)
        elseif two == 3 then
            hurt_dead(e)
        end
        bleeding_out(e)
    elseif one == 0xE then
        if two == 0 then
            die_acid(e)
        elseif two == 1 then
            hurt_fall(e)
        elseif two == 2 then
            die_down(e)
        elseif two == 3 then
            hurt_dead(e)
        end
        local count = (e:timer(3) + 1) & 0xFF
        e:set_timer(3, count)
        if count & 1 ~= 0 then
            bleed(e, e:rnd() & 0xF, 0xB)
            bleed(e, e:rnd() & 0xF, 0xB)
        end
    elseif two == 0 then
        if one == 0xA or one == 0x10 then
            die_many_wounds(e)
        elseif one == 0xB then
            die_burning(e)
        else
            die_start(e)
        end
    elseif two == 1 then
        hurt_fall(e)
    elseif two == 2 then
        die_down(e)
    elseif two == 3 then
        hurt_dead(e)
    elseif two == 4 then
        die_leap(e)
    elseif two == 5 then
        die_leap_fall(e)
    end
end

-- `Em20_Init`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    local draw = e:rnd()
    local life = e:easy() and LIFE_EASY[draw & 0xF] or LIFE_HARD[draw & 0xF]
    e:set_life((e:rnd() & 3) + life)
    e:set_d_life(0xD)
    e:set_body_box(-1000, 450, 1000, 600)
    e:set_body_ofs_x(500)
    e:set_neck_flags(0)
    e:set_speed(0, 0, 0)
    local kind = e:kind()
    e:set_routine(1, FIRST_STATE[kind & 0x7F] or 0)
    if kind == 0x2003 then
        e:clip(6, 0, 0xF)
        e:set_be_flg(e:be_flg() | BE_LOW)
    elseif kind > 0x2002 then
        e:set_be_flg(e:be_flg() | BE_LOW | BE_HIGH)
    elseif kind == 7 or kind == 8 then
        if kind == 7 then
            e:rnd()
            e:set_timer(2, (e:rnd() & 3) + 1)
            neck_clear(e, 2)
            e:set_routine(3, 1)
        end
        e:clip(0, 0, 0xF)
        e:set_var(RESTLESS, 1)
        e:set_be_flg(e:be_flg() | BE_HIGH)
    else
        if kind == 0 or kind == 1 or kind == 4 then
            e:clip(1, e:rnd() & 0x3F, 0xF)
        elseif kind == 2 or kind == 6 then
            e:clip(2, e:rnd() & 0xF, 0xF)
        elseif kind == 9 then
            e:clip(1, e:rnd() & 0x3F, 0xF)
            e:set_var(ROAM, 1)
        end
        e:set_be_flg(e:be_flg() | BE_LOW | BE_HIGH)
    end
    e:joint(0x100)
end

function update(e)
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        local normal = NORMAL[e:routine(1)]
        if normal then
            normal(e)
        end
        if e:now_seq() & 0x40000 ~= 0 then
            e:sound(8)
        end
    elseif zero == 2 then
        hurt(e)
    elseif zero == 3 then
        die(e)
    elseif zero == 7 then
        if e:routine(1) == 0 then
            e:set_be_flg(e:be_flg() | BE_NO_OBA)
            e:set_routine(1, 1)
        end
    end
end
