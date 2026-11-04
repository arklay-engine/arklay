-- Mr. X, Tyrant 1, `Em_2A` (entity id 0x2a).
--
-- The whole state machine. The engine runs the animation and collision, and
-- `Oba_ck_em` and `Sca_ck_em` after `update`. `MrX_init` swaps his main and
-- first sub banks, but for `Type` 7, and the engine's bank 0 is the swapped
-- one. The player's side of his blows, `FUN_004805e0`, is in the engine.
--
-- `hard_mode` is the title menu's and stays 0 here, as it does for the
-- zombies.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and
-- `e:set_var` are the fields of the original's work that the engine doesn't
-- read.

local TMP <const> = 0 -- `Tmp_routine` (`+0x218`), routines 0 to 2.
local SWUNG <const> = 1 -- `+0x22a`: a swing has just ended.
local CHARGED <const> = 2 -- `+0x22b`: the charge's wait.
local SHOVED <const> = 3 -- `+0x22c`: the shove's wait.
local PUNCHED <const> = 4 -- `+0x22d`: the big swing's wait.
local SOUNDS <const> = 5 -- `+0x224`: its sign keeps `Em_seq_sound` going.
local HAND_X <const> = 6 -- `+0x21c`: where the hand was last tick.
local HAND_Z <const> = 7 -- `+0x21e`.
local WEAPON <const> = 8 -- `+0x229`: the weapon that last hit him.
local BLEEDING <const> = 9 -- `+0x220`: the blood still to come, by weapon.
local BLEED_COUNT <const> = 10 -- `+0x228`.
local REACTION <const> = 11 -- `+0x1d2`: how he took the last hit.
local FLINCH_COUNT <const> = 12 -- `+0x230`.
local FLINCH_BITS <const> = 13 -- `+0x234`.

local BE_NO_OBA <const> = 2
local BE_SCENE <const> = 8
local KIND_SCENE <const> = 0x4000
local KIND_FAR <const> = 0x100
local KIND_NEAR <const> = 0x200

-- `BYTE_00529598`: the state a `Type` with `0x4000` starts in, by its low bits.
local SCENE_STATE <const> = { [0] = 9, 10, 11, 12, 13, 14, 15, 17 }

-- `MrX_init`'s lives past 20000 (`0x528f64`), by its offset there: normal, easy.
local function table_life(e, normal, easy)
    return 20000 + (e:easy() and easy or normal)
end

-- `MrX_init`'s rooms, `(scd_var_stage + 1) * 0x100 + scd_var_room`: the life
-- and which of `Type`'s distance bits it sets.
local ROOM_LIFE <const> = {
    [0x117] = { 300, 240, KIND_FAR },
    [0x10D] = { 220, 175, KIND_FAR },
    [0x507] = { 300, 240, KIND_FAR },
    [0x109] = { 400, 310, KIND_FAR },
    [0x20D] = { 300, 240, KIND_FAR },
    [0x20C] = { 400, 310, KIND_NEAR },
    [0x602] = { 350, 300, KIND_NEAR },
}

-- `0x529270`: where the blood comes from, `x, y, z`, the turn it leaves at, and
-- the part. The first 18 face the front, the rest the back.
local BLOOD <const> = {
    [0] = { 200, 400, -200, 16384, 0 },
    { 200, 600, 300, 16384, 0 },
    { 200, 900, 0, 16384, 0 },
    { 200, 700, -100, 16384, 0 },
    { 200, 1100, 500, 16384, 0 },
    { 200, 1100, -300, 16384, 0 },
    { 200, 100, 200, 16384, 8 },
    { 200, 200, -100, 16384, 0 },
    { 200, 0, 100, 16384, 8 },
    { 200, 0, 100, 16384, 0 },
    { 200, -100, 400, 16000, 8 },
    { 300, 200, 100, 16405, 11 },
    { 200, -300, 200, 16384, 8 },
    { 200, -300, -100, 16384, 8 },
    { 200, -200, -300, 16384, 8 },
    { 200, 0, -100, 16384, 8 },
    { 100, 200, 200, 16000, 13 },
    { 300, -58, -59, 16406, 9 },
    { -300, 500, 200, 18500, 0 },
    { -300, 700, -200, 18500, 0 },
    { -300, 900, 0, 18500, 0 },
    { -300, 1100, -400, 18500, 0 },
    { -300, 600, -500, 18500, 0 },
    { -300, 1100, 400, 18500, 0 },
    { -300, -200, -100, 18500, 8 },
    { -300, 0, 200, 18500, 8 },
    { -300, 100, -100, 18500, 8 },
    { -300, -100, 100, 18500, 0 },
    { -300, 100, -300, 18500, 0 },
    { -300, 300, 0, 18500, 0 },
    { -200, -300, 200, 18300, 8 },
    { -150, -200, -100, 18300, 8 },
    { -200, 0, -200, 18300, 8 },
    { -120, -200, 300, 18500, 8 },
    { -100, 300, -100, 18500, 10 },
    { -150, -100, -100, 18300, 9 },
}

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function abs(value)
    return value < 0 and -value or value
end

local function i32(value)
    value = value & 0xFFFFFFFF
    return value >= 0x80000000 and value - 0x100000000 or value
end

local function go(e, one)
    e:routines(1, one, 0, 0)
end

local function sub(e)
    return e:routine(2)
end

local function set_sub(e, two)
    e:set_routine(2, two)
end

local function next_sub(e, by)
    e:set_routine(2, (e:routine(2) + (by or 1)) & 0xFF)
end

local function count_down(e, var)
    if e:var(var) ~= 0 then
        e:set_var(var, e:var(var) - 1)
    end
end

local function timer(e, index)
    return i16(e:timer(index))
end

local function or_be_flg(e, bits)
    e:set_be_flg(e:be_flg() | bits)
end

local function clear_be_flg(e, bits)
    e:set_be_flg(e:be_flg() & ~bits)
end

-- `Mot_play_sp`, on bank 0: the clip from its start if it isn't playing, the
-- frame's speed, and the step if the clip hasn't ended.
local function mot_play_sp(e, clip, hokan, reverse)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    e:spd()
    if e:joint(0x1000 // (hokan + 1), reverse) then
        return 1
    end
    e:add_speed_xz()
    return 0
end

-- `Mot_play`: the same without the speed.
local function mot_play(e, clip, hokan)
    if e:move_no() ~= clip then
        e:clip(clip, 0, hokan)
    end
    return e:joint(0x1000 // (hokan + 1)) and 1 or 0
end

-- `FUN_0047dbb0`: the turn to the player, signed.
local function to_player(e)
    local turn = (e:direction_to_player() - e:angle()) & 0xFFF
    if turn > 0x800 then
        turn = turn - 0x1000
    end
    return turn
end

local function player_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

local function goto_player(e, rate)
    local x, z = player_xz(e)
    e:goto_point(x, z, rate)
end

-- `FUN_0047e000`: which way round `pSca_hit_data`'s middle lies.
local function toward_wall(e)
    local x, z = e:hit_middle()
    if not x then
        return 1
    end
    local bx, _, bz = e:pos()
    local turn = i16((e:direction(bx, bz, x, z) - e:angle()) & 0xFFF)
    if turn > 0x800 then
        turn = turn - 0x1000
    end
    return turn < 1 and 1 or -1
end

-- `FUN_0047e310`: the blow's damage, and the fall if it kills.
local function hurt_player(e, down)
    if e:pl_life_down(down) & 2 ~= 0 then
        e:pl_set_routine(3, 0, 0, 0)
    end
end

-- Part 12's or 15's place as `Game_loop` left it.
local function hand(e, part)
    return e:hand(part == 15 and 1 or 0)
end

-- `Esp_call(0x2400)` at a hand, kept above the floor.
local function spark(e, part, clamp)
    local x, y, z = hand(e, part)
    if clamp and y < i16(e:ground()) - 3000 then
        y = i16(e:ground()) - 3000
    end
    e:call_effect(0, 0, 0x2400, 0, x, y, z)
    return x, y, z
end

-- `FUN_0047e270`: the hand within `reach` of the player on a frame of the
-- blow, once.
local function hand_hit(e, part, from, to, reach)
    local x, _, z = hand(e, part)
    local px, pz = player_xz(e)
    local away = e:hypot(px - x, pz - z)
    e:set_var(HAND_X, i16(x))
    e:set_var(HAND_Z, i16(z))
    local count = e:move_cnt()
    if count < from or count > to or timer(e, 0) ~= 0 or away > reach then
        return false
    end
    e:set_timer(0, 1)
    return true
end

-- `FUN_0047e590`: the sweep of the hand since last tick past the player, once.
local function sweep_hit(e, part, from, to, half, reach)
    local old_x, old_z = e:var(HAND_X), e:var(HAND_Z)
    local x, _, z = hand(e, part)
    e:set_var(HAND_X, i16(x))
    e:set_var(HAND_Z, i16(z))
    local count = e:move_cnt()
    if count < from or count > to then
        return false
    end
    local bx, _, bz = e:pos()
    local now = e:hypot(bx - x, bz - z)
    local before = e:hypot(bx - old_x, bz - old_z)
    if timer(e, 0) ~= 0 or e:distance() >= (now + before + 500 + reach) >> 1 then
        return false
    end
    local low = i16(e:direction(bx, bz, old_x, old_z))
    local high = i16(e:direction(bx, bz, x, z))
    if high < low then
        low, high = high, low
    end
    local span = (high - low) & 0xFFF
    if span > 0x7FF then
        low = low - 0x1000 + span
        span = 0x1000 - span
    end
    local at = (e:direction_to_player() + (half - low)) & 0xFFF
    if at ~= 0 and at < span + half * 2 then
        e:set_timer(0, 1)
        return true
    end
    return false
end

-- `FUN_0047e330`: the blow throws the player, back or round, with his
-- `pSub1` clips. `old_x` and `old_z` are where the hand was last tick.
local function throw_player(e, old_x, old_z)
    e:pl_set_routine(5, 0, 0, 0)
    local px, pz = e:player_old()
    local dir = e:direction(old_x, old_z, px, pz)
    if ((dir - e:player_angle()) + 0x400) & 0xFFF < 0x800 then
        e:player_goto_angle(e:angle(), 0x200)
        e:pl_set_routine_at(1, 1)
    else
        e:player_goto_angle((e:angle() - 0x800) & 0xFFF, 0x200)
    end
    e:pl_hold()
end

-- `Cdir_ck(player, em)`, the other way round from `e:cdir_player()`.
local function player_faces_same_way(e)
    return ((e:angle() - e:player_angle()) + 0x400) & 0xFFF < 0x800
end

-- `FUN_0047ebf0`: `Pl_damage`'s stagger, from in front or behind.
local function stagger_player(e)
    e:pl_set_routine(2, player_faces_same_way(e) and 1 or 0, 0, 0)
    e:pl_hold()
end

-- `FUN_0047dbf0`: the look that picks the next blow.
local function look(e)
    if e:pl_routine(0) == 5 and e:pl_routine(1) == 1 then
        return
    end
    local life = e:player_life()
    if life < 0 then
        return
    end
    local dir = e:direction_to_player()
    local turn = to_player(e)
    local off = abs(turn)
    local away = e:distance()
    local free = e:pl_damage_cnt() == 0
    if off < 0x2AA and away < 2000 and life < 0x47 then
        local dx, dz = e:rotate(dir, e:player_at_w() + 300, 0)
        local px, pz = player_xz(e)
        local x, z = i16(dx) + px, i16(dz) + pz
        if free and #e:sca_list(x, z, e:player_floor(), 0x8000, 0) ~= 0 then
            go(e, 6)
            return
        end
    end
    if life < 0x33 and e:var(PUNCHED) == 0 and off < 0x2AA and away < 0x898 then
        if free then
            go(e, 3)
        end
        return
    end
    local swung = e:var(SWUNG)
    if swung ~= 0 and e:var(SHOVED) == 0 and off < 0x2AA and away < 0x898 then
        if free then
            go(e, 6)
        end
        return
    end
    local swing = false
    local flag = true
    if life < 0x29 then
        if swung == 0 and away <= 1999 then
            if free and off < 500 then
                swing = true
            else
                e:goto_dest(0x20)
            end
        end
    elseif swung == 0 and away <= 0x897 then
        if free and off < 800 and (off > 0x23 or away < 0x640) then
            swing = true
            if e:var(SOUNDS) & 0x40 ~= 0 and e:rnd() & 2 ~= 0 then
                e:set_var(SOUNDS, e:var(SOUNDS) & ~0x20)
                flag = false
            end
        else
            e:goto_dest(0x20)
        end
    end
    if swing then
        if flag then
            e:set_var(SOUNDS, e:var(SOUNDS) | 0x20)
        end
        go(e, turn < 1 and 5 or 4)
        return
    end
    if e:var(CHARGED) == 0 and e:kind() & 0x300 ~= KIND_FAR and e:routine(1) ~= 2
        and life > 0x50 and e:rnd() & 0x1F == 8 and free
        and away < 0xE74 and away > 0xC80 and off < 200 then
        go(e, 8)
    end
end

-- The walks' turn off a wall they're stuck on.
local function walk(e, clip, rate, reset)
    mot_play_sp(e, clip, 0xF)
    local side = e:hit_side()
    if side then
        if e:turn_to(((side + 2) * 0x400) & 0xFFF, 0x50) == 0 then
            e:set_timer(0, e:timer(0) + 1)
        elseif timer(e, 0) < 0x10 then
            e:set_timer(0, 0)
        end
    end
    e:goto_dest(rate)
    if timer(e, 0) > 0x3C then
        e:set_angle((e:angle() + toward_wall(e) * 0x20) & 0xFFFF)
        if timer(e, 0) > reset then
            e:set_timer(0, 0)
        end
    end
end

-- `FUN_0047da70`: walking after the player.
local function walking(e)
    if e:player_life() < 0 then
        go(e, 0x12)
        return
    end
    look(e)
    local far = e:kind() & 0x300 == KIND_FAR and 0x1964 or 0x157C
    if e:distance() > far then
        go(e, 2)
        return
    end
    if sub(e) == 0 then
        e:set_timer(0, 0)
        e:set_timer(1, 0)
        e:set_move_cnt(0)
        set_sub(e, 1)
    elseif sub(e) ~= 1 then
        return
    end
    walk(e, 1, 0x30, 0x5C)
end

-- `FUN_0047df00`: the longer stride, far from the player.
local function striding(e)
    look(e)
    if e:player_life() >= 0 and e:distance() < 0xED8 then
        go(e, 1)
        return
    end
    if sub(e) == 0 then
        e:set_timer(0, 0)
        e:set_timer(1, 0)
        set_sub(e, 1)
    elseif sub(e) ~= 1 then
        return
    end
    walk(e, 8, 0x20, 0x30)
end

-- `FUN_0047e070`: the big swing, which can kill.
local function big_swing(e)
    local two = sub(e)
    if two == 0 then
        set_sub(e, 1)
        e:set_timer(0, 0)
        two = 1
    end
    if two == 1 then
        local count = e:move_cnt()
        if count ~= 0 and count < 0x34
            and (abs(to_player(e)) > 0x352 or e:distance() > 0x1068) then
            e:set_var(PUNCHED, 0x1E)
            go(e, 1)
            return
        end
        next_sub(e, mot_play_sp(e, 2, 0xF))
        goto_player(e, 4)
        local old_x, old_z = e:var(HAND_X), e:var(HAND_Z)
        if hand_hit(e, 12, 0x38, 0x3B, 0x352) then
            e:sound(2)
            throw_player(e, old_x, old_z)
            spark(e, 12, true)
            hurt_player(e, 0x32)
            if e:player_life() < 0 then
                e:set_player_life(-1)
                e:pl_held_kill()
                e:pl_set_routine(5, 2, 0, 0)
                go(e, 0x10)
            end
        end
    elseif two == 3 then
        goto_player(e, 4)
        e:set_move_cnt(0)
        mot_play(e, 2, 0xF)
        e:set_timer(0, e:timer(0) - 1)
        if timer(e, 0) == 0 then
            go(e, 1)
        end
    elseif two == 2 then
        go(e, 1)
    end
end

-- `FUN_0047e440` and `FUN_0047e720`: the swings from either side.
local function swing(e, clip, part)
    if sub(e) == 0 then
        e:set_timer(0, 0)
        set_sub(e, 1)
    elseif sub(e) ~= 1 then
        return
    end
    if mot_play_sp(e, clip, 3) ~= 0 then
        e:set_var(SWUNG, 0x23)
        go(e, 1)
    end
    goto_player(e, 8)
    local old_x, old_z = e:var(HAND_X), e:var(HAND_Z)
    if sweep_hit(e, part, 5, 0xE, 600, e:player_at_w() * 2) then
        e:sound(2)
        throw_player(e, old_x, old_z)
        spark(e, part, true)
        hurt_player(e, part == 12 and 0x1E or 0x14)
    end
    if part == 12 then
        e:set_speed_z(100)
    end
end

-- `FUN_0047e870`: the shove into a wall.
local function shove(e)
    local two = sub(e)
    if two == 0 then
        e:set_timer(0, 0)
        set_sub(e, 1)
        or_be_flg(e, 4)
    elseif two == 2 then
        e:set_var(SHOVED, 0x32)
        go(e, 1)
        clear_be_flg(e, 4)
        return
    elseif two ~= 1 then
        return
    end
    next_sub(e, mot_play_sp(e, 0xC, 0xF))
    goto_player(e, 8)
    if hand_hit(e, 12, 0x2F, 0x32, 0x226) then
        e:sound(2)
        e:pl_set_routine(2, e:cdir_player() and 1 or 0, 0, 0)
        local x, y, z = spark(e, 12, true)
        e:call_effect(0, 0, 0x2B00, 0, x, y, z)
        hurt_player(e, 0x28)
    end
end

-- `FUN_0047e9e0`: the backhand, and its slower second half.
local function backhand(e)
    local two = sub(e)
    if two == 0 then
        e:set_timer(0, 0)
        set_sub(e, 1)
        two = 1
    end
    if two == 2 then
        e:set_move_no(0xF)
        if mot_play_sp(e, 0xF, 0, true) ~= 0 then
            go(e, 1)
        end
        goto_player(e, 8)
        if e:move_cnt() ~= 0x16 then
            return
        end
        e:sound(2)
        local x, _, z = e:pos()
        e:player_goto_point(x, z, 0x100)
        stagger_player(e)
        local hx, hy, hz = hand(e, 15)
        e:call_effect(0, 0, 0x2400, 0, hx, hy, hz)
        hurt_player(e, 10)
        return
    elseif two ~= 1 then
        return
    end
    if mot_play_sp(e, 0xF, 0xF) ~= 0 then
        go(e, 1)
    end
    goto_player(e, 8)
    if hand_hit(e, 15, 0xE, 0x13, 0x3B6) then
        e:sound(2)
        local x, _, z = e:pos()
        e:player_goto_point(x, z, 0x100)
        e:pl_set_routine(2, 0, 0, 0)
        spark(e, 15, true)
        hurt_player(e, 10)
        e:set_var(SOUNDS, e:var(SOUNDS) | 0x20)
    end
end

-- `FUN_0047ec30`: the charge from a distance.
local function charge(e)
    local two = sub(e)
    if two == 0 then
        set_sub(e, 1)
        e:set_var(CHARGED, 200)
        e:set_timer(0, 0)
    elseif two == 2 then
        go(e, 1)
        return
    elseif two ~= 1 then
        return
    end
    next_sub(e, mot_play_sp(e, 3, 0xF))
    goto_player(e, 0x18)
    local old_x, old_z = e:var(HAND_X), e:var(HAND_Z)
    if hand_hit(e, 12, 0x15, 0x19, 0x2EE) then
        e:sound(2)
        throw_player(e, old_x, old_z)
        spark(e, 12, false)
        hurt_player(e, 0x1E)
    end
end

local function set_pos(e, x, y, z)
    local bx, by, bz = e:pos()
    e:add_pos(x - bx, y - by, z - bz)
end

-- `FUN_0047ed50`: `Type` 0x4000, low bits 0: a scene that sets `fg_status`'s
-- `0x100` and moves him twice.
local function scene_drop(e)
    local two = sub(e)
    if two == 0 then
        e:set_sce_free(0, 0)
        set_sub(e, 1)
        e:set_timer(0, 10)
        or_be_flg(e, 0x408)
    elseif two == 2 then
        if mot_play_sp(e, 0xE, 0) ~= 0 then
            go(e, 1)
            clear_be_flg(e, 0x408)
        end
        if e:move_cnt() ~= 0x33 then
            return
        end
        local left = e:timer(0)
        if left == 0 then
            e:set_kind(e:kind() & ~KIND_SCENE)
            set_pos(e, -25650, -14401, -4200)
            e:set_angle(0xC20)
            return
        end
        e:set_timer(0, left - 1)
        e:add_pos(0, -1000, 0)
        if left - 1 == 0 then
            e:set_sce_free(1, 1)
        end
        e:set_move_cnt(0x32)
        return
    elseif two ~= 1 then
        return
    end
    if e:sce_free(0) ~= 0 then
        e:set_status(0x100)
        set_pos(e, -23867, -11400, 504)
        e:set_angle(0x800)
        next_sub(e)
    end
end

-- `FUN_0047ee60`: standing for a script, which steps it on with `Sce_free0`'s
-- bits.
local function scene_wait(e)
    local two = sub(e)
    if two == 0 then
        e:set_sce_free(0, 0)
        set_sub(e, 1)
        e:set_neck_flags(e:neck_flags() | 4)
        or_be_flg(e, BE_SCENE)
        two = 1
    end
    if two == 1 then
        if e:sce_free(0) & 1 == 0 then
            return
        end
        set_sub(e, 2)
        two = 2
    end
    if two == 2 then
        mot_play_sp(e, 1, 0)
        if e:sce_free(0) & 2 ~= 0 then
            e:clip(0, 0, 0x1F)
            next_sub(e)
        end
    elseif two == 3 then
        e:set_move_no(0)
        e:joint_bank(2, 0x80)
        if e:sce_free(0) & 4 ~= 0 then
            next_sub(e)
        end
    elseif two == 4 then
        mot_play_sp(e, 1, 0x1F)
    end
end

-- The climb out of the floor of `FUN_0047ef40` and `FUN_0047f110`: clip 0xe
-- held on frame 0x33 while the body rises `1000` a tick to `top`.
local function rising(e, top)
    e:set_move_no(0xE)
    e:set_move_cnt(0x33)
    e:joint(0x1000)
    local _, y = e:pos()
    e:set_pos_y(y + 1000)
    if y + 1000 == top then
        next_sub(e)
        return true
    end
    return false
end

-- `FUN_0047ef40`: rising through the floor when the script says.
local function scene_rise(e)
    local two = sub(e)
    if two == 0 then
        e:set_sce_free(0, 0)
        e:set_sce_free(1, 0)
        set_sub(e, 1)
        two = 1
    end
    if two == 1 then
        if e:sce_free(0) == 0 then
            return
        end
        e:set_kind(e:kind() & ~KIND_SCENE)
        e:set_pos_y(-4000)
        set_sub(e, 2)
        e:clip(0xE, 0x33, 0)
        rising(e, 0)
    elseif two == 2 then
        rising(e, 0)
    elseif two == 3 then
        e:set_move_no(0xE)
        if e:joint(0x1000) then
            next_sub(e)
        end
    elseif two == 4 then
        if e:sce_free(1) ~= 0 then
            go(e, 1)
        end
    end
end

-- `FUN_0047f030`: walking and the shove for a script.
local function scene_walk(e)
    local two = sub(e)
    if two == 0 then
        e:set_sce_free(0, 0)
        set_sub(e, 1)
        two = 1
    end
    if two == 1 then
        local free = e:sce_free(0)
        if free & 8 ~= 0 then
            set_sub(e, 5)
            return
        end
        if free == 0 then
            return
        end
        set_sub(e, 2)
        two = 2
    end
    if two == 2 then
        mot_play_sp(e, 1, 0xF)
        if e:sce_free(0) & 2 == 0 then
            return
        end
        set_sub(e, 3)
        two = 3
    end
    if two == 3 then
        mot_play(e, 0xC, 0x1F)
        e:set_move_cnt(1)
        if e:sce_free(0) & 4 == 0 then
            return
        end
        e:set_move_cnt(0)
        set_sub(e, 4)
        two = 4
    end
    if two == 4 then
        next_sub(e, mot_play_sp(e, 0xC, 0xF))
    elseif two == 5 then
        if e:sce_free(0) & 8 ~= 0 then
            e:set_kind(e:kind() & ~KIND_SCENE)
            go(e, 1)
        end
    end
end

-- A step of the room's own clips: `Spd_get` and `Joint_move` on bank 2.
local function room_step(e, clip, late)
    e:set_move_no(clip)
    e:spd_bank(2)
    local ended = e:joint_bank(2, late)
    e:add_speed_xz()
    return ended
end

local function room_clip(e, clip, count, hokan)
    e:set_bank(2)
    e:clip(clip, count, hokan)
end

-- `FUN_0047f110`: the long scene on the room's clips, stepped by `Sce_free0`.
local function scene_long(e)
    local two = sub(e)
    local free = e:sce_free(0)
    if two == 0 then
        e:set_sce_free(0, 0)
        free = 0
        e:set_timer(0, 0)
        e:set_pos_y(-25000)
        or_be_flg(e, 0xA)
        set_sub(e, 1)
        two = 1
    end
    if two == 1 then
        if free ~= 1 then
            return
        end
        e:set_pos_y(-24000)
        e:clip(0xE, 0x33, 0)
        set_sub(e, 2)
        rising(e, -18000)
    elseif two == 2 then
        rising(e, -18000)
    elseif two == 3 then
        e:set_move_no(0xE)
        if e:joint(0x1000) then
            next_sub(e)
        end
    elseif two == 4 or two == 5 then
        if two == 4 then
            if free ~= 2 then
                return
            end
            set_sub(e, 5)
        end
        mot_play_sp(e, 1, 0xF)
        if free == 3 then
            next_sub(e)
        end
    elseif two == 6 then
        next_sub(e, mot_play_sp(e, 9, 0xF))
    elseif two == 7 or two == 8 then
        if two == 7 then
            if free ~= 4 then
                return
            end
            room_clip(e, 1, 0, 0xF)
            set_sub(e, 8)
        end
        if room_step(e, 1, 0x100) then
            next_sub(e)
        end
    elseif two == 9 then
        if free == 5 then
            e:set_angle((e:angle() + 0x800) & 0xFFFF)
            set_sub(e, 10)
            room_clip(e, 1, 0, 0)
            e:joint_bank(2, 0x1000)
        end
    elseif two == 10 or two == 11 then
        if two == 10 then
            mot_play_sp(e, 8, 3)
            if e:sce_free(0) ~= 6 then
                return
            end
            room_clip(e, 2, 0, 3)
            set_sub(e, 11)
        end
        e:set_move_no(2)
        if e:sce_free(0) == 7 then
            room_clip(e, 3, 0, 3)
            next_sub(e)
        end
        e:spd_bank(2)
        if e:joint_bank(2, 0x400) then
            e:set_move_cnt(0x19)
            return
        end
    elseif two == 12 then
        e:set_move_no(3)
        e:spd_bank(2)
        if e:joint_bank(2, 0x400) then
            next_sub(e)
        end
        e:add_speed_xz()
    elseif two == 13 or two == 14 then
        if two == 13 then
            if free ~= 8 then
                return
            end
            e:set_angle((e:angle() + 0x800) & 0xFFFF)
            room_clip(e, 4, 0, 0)
            set_sub(e, 14)
        end
        e:joint_bank(2, 0x1000)
        if e:sce_free(0) ~= 8 then
            return
        end
        room_clip(e, 4, 0, 0)
        next_sub(e)
    elseif two == 15 then
        next_sub(e)
    elseif two == 16 then
        e:set_move_no(4)
        e:spd_bank(2)
        if e:joint_bank(2, 0x1000) then
            room_clip(e, 0, 0, 3)
            next_sub(e)
        end
        e:add_speed_xz()
    elseif two == 17 then
        if room_step(e, 0, 0x400) then
            next_sub(e)
        end
    end
end

-- `FUN_0047f500`: the other long scene on the room's clips.
local function scene_other(e)
    local two = sub(e)
    local free = e:sce_free(0)
    if two == 0 then
        e:set_sce_free(0, 0)
        free = 0
        or_be_flg(e, 0xA)
        set_sub(e, 1)
        two = 1
    end
    if two == 1 then
        if free ~= 1 then
            return
        end
        set_sub(e, 2)
        two = 2
    end
    if two == 2 then
        mot_play_sp(e, 1, 0xF)
        if e:sce_free(0) ~= 2 then
            return
        end
        set_sub(e, 3)
        two = 3
    end
    if two == 3 then
        next_sub(e, mot_play_sp(e, 2, 0xF))
    elseif two == 4 or two == 5 then
        if two == 4 then
            if free ~= 6 then
                return
            end
            set_sub(e, 5)
        end
        mot_play_sp(e, 1, 0xF)
        if e:sce_free(0) == 3 then
            room_clip(e, 1, 0, 7)
            next_sub(e)
        end
    elseif two == 6 or two == 7 then
        if two == 6 then
            if room_step(e, 1, 0x200) then
                next_sub(e)
            end
        end
        if e:sce_free(0) == 4 then
            e:set_angle((e:angle() + 0x800) & 0xFFFF)
            next_sub(e)
            room_clip(e, 1, 0, 0)
            e:joint_bank(2, 0x1000)
        end
    elseif two == 8 or two == 9 then
        if two == 8 then
            mot_play_sp(e, 1, 0xF)
            if e:sce_free(0) ~= 5 then
                return
            end
            room_clip(e, 4, 0, 0)
            set_sub(e, 9)
        end
        e:set_move_no(4)
        e:spd_bank(2)
        if not e:joint_bank(2, 0x1000) then
            e:add_speed_xz()
            return
        end
        room_clip(e, 0, 10, 0)
        e:spd_bank(2)
        next_sub(e)
    elseif two == 10 or two == 11 then
        if two == 10 then
            e:set_angle((e:angle() + 0x400) & 0xFFFF)
            set_sub(e, 11)
        end
        e:set_move_no(0)
        e:spd_bank(2)
        if e:joint_bank(2, 0x1000) then
            next_sub(e)
        end
        e:add_speed_xz()
    end
end

-- `FUN_0047f780`: waiting up in room 20c until the script drops him.
local function scene_fall(e)
    local two = sub(e)
    if two == 0 then
        e:set_sce_free(0, 0)
        or_be_flg(e, BE_SCENE)
        set_sub(e, 1)
        e:set_move_cnt(0)
        two = 1
    end
    if two == 1 then
        if e:sce_free(0) ~= 1 then
            return
        end
        e:set_move_cnt(0x1A)
        e:set_move_no(0x10)
        e:spd()
        set_sub(e, 2)
        two = 2
    end
    if two == 2 then
        e:set_move_no(0x10)
        next_sub(e, mot_play_sp(e, 0x10, 7))
    elseif two == 3 then
        e:routines(1, 1, 0, 0)
        e:set_kind(e:kind() & ~KIND_SCENE)
        clear_be_flg(e, BE_SCENE)
    end
end

-- `FUN_0047f840`: the kill, lifting the player by the neck.
local function killing(e)
    local dir = e:direction_to_player()
    local ahead, side
    if player_faces_same_way(e) then
        ahead, side = 0x640, 300
    else
        ahead, side = 2000, 0x10E
        local _, py = e:player_pos()
        e:approach_y(py + 0x82, 1)
    end
    local two = sub(e)
    local function hold(rate)
        local bx, _, bz = e:pos()
        local dx, dz = e:rotate(e:angle(), ahead, side)
        e:player_approach(i16(dx) + bx, i16(dz) + bz, rate)
    end
    if two == 0 then
        e:set_timer(0, 0)
        or_be_flg(e, 0xA)
        next_sub(e)
        two = 1
    end
    if two == 1 then
        e:set_move_no(2)
        next_sub(e, mot_play_sp(e, 2, 3))
        hold(300)
        local _, y = e:pos()
        e:set_pos_y(y + 0x19)
        e:goto_angle((dir + 0x60) & 0xFFF, 0x30)
        e:set_timer(0, e:timer(0) + 1)
        if timer(e, 0) > 6 then
            e:set_move_cnt(9)
            next_sub(e)
        end
    elseif two == 2 then
        e:set_move_no(0xC)
        local ended = mot_play(e, 0xC, 3)
        next_sub(e, ended)
        if e:move_cnt() == 0x3F then
            next_sub(e)
        end
        hold(0xFA)
        local count = e:move_cnt()
        if count == 0x32 then
            e:sound(2)
            local x, y, z = hand(e, 12)
            e:call_effect(0, 0, 0x2800, 0, x, y, z)
        end
        if count == 0x2F then
            e:sound(2)
        end
    elseif two == 3 then
        e:goto_angle((dir + 0x60) & 0xFFF, 0x30)
        next_sub(e, mot_play(e, 0xD, 3))
        if e:move_cnt() == 0x95 then
            e:set_move_cnt(0x74)
        end
    end
end

-- `FUN_0047fac0`: `Type` 7, standing still for a script.
local function scene_still(e)
    local two = sub(e)
    if two == 0 then
        -- `Attribute |= 0x80` (`+0x10`), which nothing here reads.
        e:set_sce_free(0, 0)
        set_sub(e, 1)
        mot_play_sp(e, 0, 0)
        or_be_flg(e, BE_SCENE)
        return
    end
    if two == 1 then
        if e:sce_free(0) == 0 then
            return
        end
        next_sub(e)
    elseif two ~= 2 then
        return
    end
    mot_play_sp(e, 0, 0)
    if e:sce_free(0) == 2 then
        next_sub(e)
    end
end

-- `FUN_0047fb40`: the player is dead; he turns away and walks, a frame on
-- every other draw buffer.
local function walking_off(e)
    local px, pz = player_xz(e)
    local bx, _, bz = e:pos()
    local away = e:direction(px, pz, bx, bz)
    if sub(e) == 0 then
        if e:draw_buffer() ~= 0 then
            mot_play_sp(e, 1, 0xF)
        end
        e:goto_angle(away, 8)
    end
end

local NORMAL <const> = {
    [1] = walking,
    [2] = striding,
    [3] = big_swing,
    [4] = function(e)
        swing(e, 6, 12)
    end,
    [5] = function(e)
        swing(e, 7, 15)
    end,
    [6] = shove,
    [7] = backhand,
    [8] = charge,
    [9] = scene_drop,
    [10] = scene_wait,
    [11] = scene_rise,
    [12] = scene_walk,
    [13] = scene_long,
    [14] = scene_other,
    [15] = scene_fall,
    [16] = killing,
    [17] = scene_still,
    [18] = walking_off,
}

-- `FUN_00480010`: `count` drops of blood at the table's points, from the
-- front or back half when `flags` has 2, and from a third of it by the last
-- reaction when it has 1.
local function blood(e, count, kind, row, size, flags)
    local base, range = 0, 0x24
    if flags & 2 ~= 0 then
        if e:cdir_player() then
            base = 0x12
        end
        range = 0x12
    end
    if flags & 1 ~= 0 then
        range = 6
        base = base + (e:var(REACTION) % 3) * 6
    end
    for _ = 1, count do
        local point = BLOOD[e:rnd() % range + base]
        local dir = i16(point[4] + e:angle())
        e:effect(kind, row, size, dir, point[5], point[1], point[2], point[3])
    end
end

-- `FUN_0047fea0`: the blood each weapon draws. The grenades' and the others'
-- that keep bleeding over the ticks after (`0xa`, `0xe`, `0xf` and `0x12`)
-- aren't in the weapon table yet.
local function bleed(e)
    e:set_var(BLEED_COUNT, 0)
    local weapon = e:var(WEAPON)
    if weapon == 2 or weapon == 3 or weapon == 4 or weapon == 0xD then
        blood(e, 1, 0, 6, 0x1000, 3)
    elseif weapon == 5 or weapon == 6 then
        blood(e, 1, 0, 6, 0x1600, 3)
    elseif weapon == 7 then
        blood(e, 4, 0, 6, 0x1500, 3)
        blood(e, 2, 0, 6, 0x1400, 2)
    elseif weapon == 8 then
        blood(e, 4, 0, 6, 0x1500, 3)
        blood(e, 4, 0, 6, 0x1400, 2)
    elseif weapon == 9 then
        blood(e, 5, 5, 4, 0x1000, 3)
    elseif weapon == 0xB then
        blood(e, 5, 4, 9, 0x1500, 3)
    elseif weapon == 0x10 then
        blood(e, 5, 5, 4, 0x1600, 3)
    end
end

-- `FUN_004822f0`: one hit in four, at a random place among them.
local function flinch_ck(e)
    local count = e:var(FLINCH_COUNT) & 0xFFFFFFFF
    local left = (count - 1) & 0xFFFFFFFF
    if count > 0x20 or left == 0 then
        e:set_var(FLINCH_COUNT, 4)
        e:set_var(FLINCH_BITS, i32(0x80000000 >> (e:rnd() % 4)))
    else
        e:set_var(FLINCH_COUNT, i32(left))
    end
    local bits = e:var(FLINCH_BITS) & 0xFFFFFFFF
    e:set_var(FLINCH_BITS, i32(bits << 1))
    return bits & 0x80000000 ~= 0
end

-- `FUN_00480370`.
local function react(e)
    if e:life() < 20000 then
        e:set_var(REACTION, 3)
    elseif not flinch_ck(e) then
        e:set_var(REACTION, 4)
    elseif e:damage_flg() & 0x20 == 0 then
        e:set_var(REACTION, 0)
    else
        e:set_var(REACTION, 1)
    end
end

-- `FUN_004803d0`: a flinch from in front.
local function flinch(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_routine(3, 1)
        e:set_timer(0, e:rnd() & 1)
    elseif three == 2 then
        go(e, 1)
        return
    elseif three ~= 1 then
        return
    end
    local ended = mot_play_sp(e, timer(e, 0) + 4, 0xF)
    e:set_routine(3, e:routine(3) + ended)
    goto_player(e, 8)
end

-- `FUN_00480440`: a flinch from behind.
local function flinch_back(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_routine(3, mot_play_sp(e, 9, 0xF))
    elseif three == 1 then
        go(e, 1)
    end
end

-- `FUN_00480480`: down on one knee, which takes him out of the room, until a
-- script stands him up.
local function knocked_down(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_died()
        e:set_kind(e:kind() | KIND_SCENE)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:set_sce_free(0, 0)
        e:set_routine(3, 1)
        three = 1
    end
    if three == 1 then
        e:set_routine(3, e:routine(3) + mot_play_sp(e, 10, 0xF))
    elseif three == 2 then
        or_be_flg(e, BE_NO_OBA)
        if e:sce_free(0) ~= 0 then
            e:set_sce_free(0, 0)
            e:set_routine(3, 3)
        end
    elseif three == 3 then
        e:set_move_cnt(0)
        mot_play(e, 0xB, 0)
        if e:sce_free(0) ~= 0 then
            e:set_routine(3, 4)
        end
    elseif three == 4 then
        e:set_routine(3, e:routine(3) + mot_play(e, 0xB, 0))
    end
end

-- `FUN_00480560`: no flinch; on with what he was doing.
local function carry_on(e)
    local tmp = e:var(TMP)
    e:routines(tmp & 0xFF, (tmp >> 8) & 0xFF, (tmp >> 16) & 0xFF, 0)
    if tmp & 0xFF ~= 1 then
        go(e, 1)
        return
    end
    local normal = NORMAL[e:routine(1)]
    if normal then
        normal(e)
    end
end

local REACTIONS <const> = {
    [0] = flinch,
    [1] = flinch_back,
    [3] = knocked_down,
    [4] = carry_on,
}

-- `FUN_0047fe50`.
local function hurt(e)
    if e:routine(1) ~= 0xFF then
        e:set_var(WEAPON, e:routine(1))
        bleed(e)
        e:set_routine(1, 0xFF)
        react(e)
    end
    local reaction = REACTIONS[e:var(REACTION)]
    if reaction then
        reaction(e)
    end
end

-- `MrX_init`.
local function init(e)
    local kind = e:kind()
    e:routines(1, 1, 0, 0)
    e:set_var(SOUNDS, 0)
    if kind & KIND_SCENE ~= 0 then
        e:set_routine(1, SCENE_STATE[kind & 0xF])
    end
    local high = (kind >> 8) & 3
    local life
    if high == 0 then
        life = 0x4F1A
    elseif high == 1 then
        e:set_var(SOUNDS, 6)
        life = table_life(e, 250, 180)
    elseif high == 2 then
        e:set_var(SOUNDS, 0x4B)
        life = table_life(e, 450, 340)
    else
        e:set_var(SOUNDS, 0x1A)
        life = table_life(e, 300, 220)
    end
    local room = ROOM_LIFE[e:room_code()]
    if room then
        life = table_life(e, room[1], room[2])
        local other = room[3] == KIND_FAR and KIND_NEAR or KIND_FAR
        e:set_kind((kind & ~other) | room[3])
    end
    e:set_life(life)
    e:set_be_flg(1 | 0x1C000000)
    e:set_damage_flg(0)
    e:set_var(HAND_X, 0)
    e:set_var(HAND_Z, 0)
    e:set_var(BLEED_COUNT, 0)
    e:set_var(BLEEDING, 0)
    e:set_var(SWUNG, 0)
    e:set_var(CHARGED, 0)
    e:set_var(SHOVED, 0)
    e:set_var(PUNCHED, 0)
    -- Neither `MrX_init` nor `Sce_em_set` clears `+0x230` or `+0x1d2`: they
    -- hold what the slot's last body left. That is nearly always past 0x20
    -- for the count, so the first hit draws a new order, as here. The
    -- reaction stays 0, which only moves the first hit's blood.
    e:set_var(FLINCH_COUNT, -1)
    e:clip(1, 0, 0)
    e:joint(0x100)
    e:set_body_box(-1700, 700, 1700, 700)
    e:set_body_ofs_x(0)
end

local function routine_word(e)
    return e:routine(0) | e:routine(1) << 8 | e:routine(2) << 16
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
        -- `Mot_play_init`: a new state starts its clip afresh.
        local now = routine_word(e)
        if now ~= e:var(TMP) then
            e:set_move_no(0xFF)
        end
        e:set_var(TMP, now)
    elseif zero == 2 then
        hurt(e)
    end
    count_down(e, SWUNG)
    count_down(e, CHARGED)
    count_down(e, SHOVED)
    count_down(e, PUNCHED)
    if e:var(SOUNDS) & 0x80 == 0 then
        e:seq_sound()
    end
end
