-- The ivy, `Em_2E`: the plant 0x2e, and the poison ivy 0x39 that runs the same
-- code. `enemy/2e/enemy.lua` and `enemy/39/enemy.lua` load this.
--
-- An ivy grows three or four vines (`Em_2F`, `enemy/2f/enemy.lua`) at its
-- init and drives them through `+0x225` and `+0x227`: the vine's routine,
-- plus one, and which vines it is for. It walks toward the player, lashes
-- with its two whips, and closes on the player to hold and squeeze them.
-- `+0x231` is the side it walks on: its clips come in two sets of 13, and it
-- faces `Cdir_y + side * 0x800`.
--
-- The engine (`run_plant`) runs `Oba_ck_em` after `update`, or while
-- `vars[30]` (`+0x21a`) is set `FUN_00488980`, the push out of the other
-- enemies but not the player. Then it runs `Sca_ck_em`, poses the parts and
-- counts the ticks it didn't move in `vars[31]` (`+0x22d`).
--
-- State 5's spit is `Esp_call` kind 0x1D, whose `Esp_20` hits and poisons
-- the player (`esp2.rs`). A hit (`Routine_0` 2) and the death (3, then 7)
-- are ported for the weapons arklay fires, 2 to 8, 0xD, 0xF, 0x10 and 0x13,
-- with `censorship_off` and `hard_mode` clear, and so is the retreat at half
-- its life (state 7). Only
-- the player as the target is ported (`FUN_004881c0` can pick the partner
-- too), and neither are the other weapons' entries, the spit at close range
-- (state 6), states 8, 9 and 11, the head's twist on parts 11 to 13
-- (`FUN_004883d0`), the parts' colours (the flame's death fades them,
-- `FUN_00487b90`) and the dead ivy's shadow.

local M = {}

local TARGET <const> = 0 -- `+0x218`: 0 for the player.
local PARTNERS <const> = 1 -- `+0x219`: another ivy is in play.
local HELD <const> = 2 -- `+0x21c`.
local TWIST <const> = 3 -- `+0x21e`: the head's twist, which only the spit uses.
local T220 <const> = 4 -- `+0x220`: the wait before it may lash.
local T221 <const> = 5 -- `+0x221`: the wait before it may grab.
local T222 <const> = 6 -- `+0x222`.
local T223 <const> = 7 -- `+0x223`: the wait before it sends a vine.
local COMMAND <const> = 8 -- `+0x225`: the vines' routine, plus one.
local COMMAND_TO <const> = 9 -- `+0x227`: which vines.
local CHILDREN <const> = 10 -- `+0x226`: a bit for each vine.
local HALF <const> = 11 -- `+0x228`: half its life, where it backs off.
local TWIST_BY <const> = 12 -- `+0x22a`: which way the head twists, or 0.
local WHIP <const> = 13 -- `+0x21d`: what a lash takes off.
local T232 <const> = 14 -- `+0x232`: the wait after it turns round.
local TWISTED <const> = 15 -- `+0x22b`: the twist is done.
local F238 <const> = 16 -- `+0x238`.
local F239 <const> = 17 -- `+0x239`.
local T23A <const> = 18 -- `+0x23a`: the left vine's wait.
local T23B <const> = 19 -- `+0x23b`: the right's.
local BLOOD <const> = 20 -- `+0x23c`.
local F23D <const> = 21 -- `+0x23d`.
local HOLDS <const> = 22 -- `+0x23e`: which spit, added to the id's row.
local F23F <const> = 23 -- `+0x23f`.
local KEPT <const> = 24 -- `+0x224`: `+0x21c` while it holds.
local TWIST_RATE <const> = 25 -- `+0x22c`.
local SAVED <const> = 26 -- `+0x22e` to `+0x230`: last tick's routines.
local SIDE <const> = 29 -- `+0x231`.
local OBA <const> = 30 -- `+0x21a`: skip `Oba_ck_em`.
local STUCK <const> = 31 -- `+0x22d`.
local HOLD_COUNT <const> = 32 -- `+0x240`: the squeezes left in a hold.
local DEAD <const> = 33 -- `+0x233`: how far `FUN_004880c0` has gone.

local ROOM_SPAT <const> = 0x20
local ROOM_LASHING <const> = 0x40
local ROOM_HOLDING <const> = 0x80

local VINE <const> = 0x2F
local IVY <const> = 0x2E

-- `0x529d68` and `0x529d88`: life, by `Rnd`.
local LIFE <const> = { 97, 119, 97, 90, 97, 90, 119, 97, 90, 90, 89, 119, 69, 90, 89, 119 }
local LIFE_EASY <const> = { 89, 69, 89, 79, 99, 69, 79, 59, 69, 59, 59, 69, 59, 59, 59, 69 }
-- `0x529df8`: four sets of the vines' rows.
local VINE_SETS <const> = { { 2, 3, 5, 6 }, { 0, 1, 4, 7 }, { 0, 3, 6, 7 }, { 1, 2, 6, 7 } }
-- `0x529db8`: a vine's part, its command bits and its three offsets.
local VINE_ROWS <const> = {
    [0] = { 14, 2, 0, 200 },
    { 15, 2, 0, 0 },
    { 20, 1, 0, 300 },
    { 21, 1, 0, 0 },
    { 1, 2, 0, 0 },
    { 5, 1, 0, 0 },
    { 9, 2, -10, -100 },
    { 10, 1, -20, 150 },
}
-- `0x529e38`: the first `Routine_1`, by `Type`.
local START <const> = { [0] = 0, 1, 0, 1, 8, 8, 9, 11 }
-- `0x529e70`.
local WAITS <const> = { [0] = 15, 30, 60, 45, 15, 30, 60, 45 }
-- `0x529e94`: the wait after a spit, less 60.
local SPIT_WAITS <const> = { [0] = 30, 15, 0, 15, 15, 0, 15, 30 }
-- `0x529da8`: the whips' points, and how near the player must be.
local WHIP_POINTS <const> = { { 21, 1000 }, { 23, 800 }, { 15, 1000 }, { 17, 800 } }

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function dec(e, index)
    local value = e:var(index)
    if value ~= 0 then
        e:set_var(index, value - 1)
    end
end

local function side(e)
    return e:var(SIDE)
end

-- `Move_no` for clip `n` of its side, with `Hokan_flg` 0x3f.
local function side_clip(e, n, hokan)
    e:clip(side(e) * 13 + n, 0, hokan or 0x3F)
end

local function player_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

-- `FUN_004886b0`: `Goto00_ck`, facing its side.
local function goto_side(e, x, z, rate)
    local bx, _, bz = e:pos()
    return e:goto_ck_at(bx, bz, e:angle() + side(e) * 0x800, x, z, rate)
end

local function routines(e, one, two, three)
    e:set_routine(1, one)
    e:set_routine(2, two or 0)
    e:set_routine(3, three or 0)
end

-- `0x529fe0`, `FUN_00488570`'s rows: `Esp_call`'s kind, row and size.
local BLOOD_ROWS <const> = {
    [0] = { 0, 0x11, 0x1800 },
    { 0, 0x11, 0x1000 },
    { 5, 4, 0x1000 },
    { 5, 1, 0x1000 },
    { 0, 0x1D, 0x1000 },
    { 0, 5, 0x800 },
    { 0, 0x1D, 0x1800 },
    { 4, 0xF, 0xC00 },
    { 4, 0xF, 0x1400 },
    { 6, 0, 0x1000 },
    { 4, 0xC, 0x1800 },
}
-- `0x529e08`: the parts a spray of blood picks from.
local BLOOD_PARTS <const> = { [0] = 0, 1, 9, 14, 5, 10, 20, 17, 2, 21, 10, 0, 7, 15, 3, 9 }
-- `0x529f64`: the parts `FUN_004872e0` bleeds from.
local HIT_PARTS <const> = { [0] = 11, 10, 14, 20, 0, 1, 5, 21 }

-- `FUN_00488570`: a row's effect at a part, facing from the ivy to its
-- target (`+0x218`, only the player here).
local function blood(e, part, row)
    local effect = BLOOD_ROWS[row]
    local x, y, z = e:own_part(part)
    e:call_effect(effect[1], effect[2], effect[3], e:direction_to_player(), x, y, z)
end

-- `FUN_00487450`: `count` parts in a row of `BLOOD_PARTS` from a random one.
local function spray(e, row, count)
    local first = e:rnd() & 7
    for index = 0, count - 1 do
        blood(e, BLOOD_PARTS[first + index], row)
    end
end

-- The vines' command, and which of them it is for.
local function command(e, value, to)
    e:set_var(COMMAND, value)
    e:set_var(COMMAND_TO, to)
end

-- `FUN_00488450`: at half its life it backs off.
local function backs_off(e)
    local life = e:life()
    if e:var(HALF) < life or life < 0 or e:var(HELD) ~= 0 then
        return false
    end
    e:set_routine(1, 7)
    e:set_routine(2, 0)
    e:set_routine(3, 0)
    return true
end

-- `FUN_004853a0`.
local function hit_hard(e)
    if e:var(F238) == -0x8000 then
        if e:var(F23D) == 0 then
            e:routines(3, 0xA, 0, 0)
        else
            e:routines(7, 0, 0, 0)
        end
    end
end

-- `FUN_004888a0`: the turn toward the player from `part`, as it faces with
-- the neck's turn, and how far.
local function turn_from_part(e, part, rate)
    local x, _, z = e:own_part(part)
    local angle = e:angle()
    if part > 8 then
        angle = angle + e:neck_turn()
    end
    local px, pz = player_xz(e)
    local distance = e:hypot(x - px, z - pz)
    return e:goto_ck_at(x, z, angle + side(e) * 0x800, px, pz, rate), distance
end

-- `FUN_00485690`: what to do next, from how near the player is.
local function choose(e)
    local distance = e:distance()
    if e:player_life() < 0 then
        return
    end
    local room = e:room_enemy()
    if room & ROOM_HOLDING ~= 0 then
        e:set_var(T220, 0x1E)
        e:set_var(T221, 0x1E)
        return
    end
    local _, neck = e:part_rot(9)
    neck = i16(neck)
    if e:var(TARGET) == 0 and room & ROOM_SPAT ~= 0 and e:var(F239) & 0x80 == 0 and side(e) == 0 then
        -- The spit (state 6) isn't ported.
        return
    end
    -- Another ivy on its first side, with the player hurt, holds off.
    if e:var(PARTNERS) ~= 0 and room & ROOM_SPAT ~= 0 then
        return
    end
    if e:var(T221) == 0 then
        local turn = turn_from_part(e, 11, 1)
        local square, near = turn_from_part(e, 11, 0x200)
        if near < 0x5DD and square == 0 and (neck - 0x800) * turn <= 0
            and e:var(F239) & 0x80 == 0 and e:var(TARGET) == 0
            and e:pl_damage_cnt() & 0x80 == 0 then
            routines(e, 10)
            e:set_room_enemy(e:room_enemy() | ROOM_HOLDING)
            return
        end
    end
    local px, pz = player_xz(e)
    e:set_angle((e:angle() + neck) & 0xFFFF)
    local turn = goto_side(e, px, pz, 0x100)
    e:set_angle((e:angle() - neck) & 0xFFFF)
    if distance < 0xFA1 and turn == 0 and e:var(T220) == 0 and room & ROOM_LASHING == 0
        and e:pl_routine(0) == 1 and e:pl_routine(1) ~= 2 then
        routines(e, 4)
        return
    end
    e:set_angle((e:angle() + neck) & 0xFFFF)
    turn = goto_side(e, px, pz, 0x100)
    e:set_angle((e:angle() - neck) & 0xFFFF)
    if distance < 0x2329 and distance > 3999 and turn == 0 and e:var(T222) == 0
        and e:var(F239) & 0x80 == 0 then
        routines(e, 5)
        e:set_var(HOLDS, 0)
    end
end

-- `FUN_004855c0`: after each step of the walk.
local function after_step(e)
    local distance = e:distance()
    e:seq_sound()
    -- The player behind its side: it turns round.
    local px, pz = player_xz(e)
    local bx, _, bz = e:pos()
    local turn = e:goto_ck_at(bx, bz, e:angle() + (side(e) + 1) * 0x800, px, pz, 0x200)
    if distance < 3000 and turn == 0 and e:var(T232) == 0 then
        e:set_routine(1, 2)
        e:set_routine(2, 0)
        e:set_routine(3, 0)
    end
    if e:player_life() < 0 then
        routines(e, 0)
    end
    choose(e)
    backs_off(e)
    hit_hard(e)
end

-- `FUN_00485a10`: turned round, it walks on its other side.
local function turned(e)
    local was = side(e)
    e:set_var(SIDE, was ~ 1)
    e:set_body_ofs_x(was ~= 1 and -800 or 800)
    e:set_var(T232, 0x3C)
    routines(e, 1)
    e:set_neck_flags(e:neck_flags() & ~2)
    backs_off(e)
    if e:pl_routine(0) == 1 and e:pl_routine(1) == 2 then
        if (1 << (e:rnd() & 0xF)) & 0xBD0 ~= 0 then
            e:set_routine(1, 5)
            e:set_var(HOLDS, 1)
        end
    end
    e:set_var(T222, 0)
    e:set_var(T220, 0)
    e:set_var(T221, 0)
end

-- State 0, `FUN_00485250`: waits for the player to come near.
local function wait(e)
    local two = e:routine(2)
    if two == 0 then
        if e:kind() == 0x6002 then
            if e:sce_free() == 0 then
                return
            end
            e:set_kind(2)
        end
        e:set_routine(2, 1)
        e:set_var(COMMAND, 1)
        e:set_var(COMMAND_TO, 3)
        side_clip(e, 0)
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
        e:set_var(OBA, 1)
        two = 1
    elseif two == 2 then
        local timer = i16(e:timer(0) - 1)
        e:set_timer(0, timer)
        if timer == 0 then
            routines(e, 1)
            e:set_var(STUCK, 0)
        end
    end
    if two == 1 and e:distance() < 10000 and e:player_life() >= 0 then
        e:set_routine(2, 2)
        e:set_timer(0, 0x1E)
        e:set_neck_flags(e:neck_flags() & ~2)
        e:set_var(COMMAND, 3)
        e:set_var(COMMAND_TO, 3)
    end
    e:joint(0x40)
    backs_off(e)
    hit_hard(e)
end

-- State 1, `FUN_004853d0`: the walk toward the player.
local function walk(e)
    local px, pz = player_xz(e)
    local two = e:routine(2)
    if two == 0 then
        if e:kind() == 0x6002 then
            if e:sce_free() == 0 then
                return
            end
            e:set_kind(2)
        end
        e:set_routine(2, 1)
        side_clip(e, 1)
        e:spd()
        e:joint(0x40)
        e:set_neck_flags(e:neck_flags() & ~2)
        e:set_var(COMMAND, 3)
        e:set_var(COMMAND_TO, 3)
        e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
        e:set_be_flg(e:be_flg() & ~4)
        e:set_var(OBA, 0)
        e:set_var(STUCK, 0)
        after_step(e)
        return
    end
    if two == 1 then
        e:root_ck(px, pz)
        local dx, dz = e:dest()
        e:set_angle((e:angle() + goto_side(e, dx, dz, 6)) & 0xFFFF)
        e:spd()
        e:joint(0x40)
        e:add_speed_xz()
        if e:var(T223) == 0 then
            e:set_routine(2, 2)
        end
        if e:var(STUCK) > 0x78 then
            routines(e, 3)
            after_step(e)
            return
        end
    else
        if two == 2 then
            e:set_routine(2, 3)
            e:set_timer(0, 0x5A)
        end
        e:root_ck(px, pz)
        local dx, dz = e:dest()
        e:set_angle((e:angle() + goto_side(e, dx, dz, 6)) & 0xFFFF)
        e:set_var(T220, 0)
        e:spd()
        e:joint(0x40)
        local timer = i16(e:timer(0) - 1)
        e:set_timer(0, timer)
        if timer == 0 then
            e:set_routine(2, 1)
            e:set_var(STUCK, 0)
        end
    end
    after_step(e)
end

-- State 2, `FUN_00485920`: turns round to walk on its other side.
local function turn(e)
    if e:routine(2) ~= 0 then
        -- `0x485ad0`.
        if e:routine(3) == 0 then
            e:set_routine(3, 1)
            e:set_neck_flags(e:neck_flags() | 2)
            e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
            side_clip(e, 5, 0)
        end
        e:set_body_ofs_x(i16(e:body_ofs_x() + (side(e) ~= 0 and 0x2B or -0x2B)))
        if e:joint(0x40) then
            e:set_var(T220, WAITS[e:rnd() & 7])
            turned(e)
        end
        return
    end
    if e:routine(3) == 0 then
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_routine(3, 1)
        side_clip(e, 7)
        e:spd()
        e:joint(0x40)
        e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
        e:set_be_flg(e:be_flg() & ~4)
        e:set_var(OBA, 1)
        local sx = e:speed()
        e:set_body_ofs_x(i16(e:body_ofs_x() - sx))
        return
    end
    e:spd()
    if not e:joint(0x40) then
        e:add_speed_xz()
        local sx = e:speed()
        e:set_body_ofs_x(i16(e:body_ofs_x() - sx))
        return
    end
    turned(e)
end

-- State 3, `FUN_00485b60`: stuck, it turns on the spot.
local function stuck(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        local px, pz = player_xz(e)
        local by = -goto_side(e, px, pz, 1)
        if by == 0 then
            by = (e:rnd() & 1) ~= 0 and 1 or -1
        end
        e:set_timer(1, 0x400)
        e:set_timer(0, by * 6)
    end
    e:set_timer(1, i16(e:timer(1) - 6))
    e:set_angle((e:angle() + i16(e:timer(0))) & 0xFFFF)
    e:spd()
    e:joint(0x40)
    if i16(e:timer(1)) < 0 then
        routines(e, 1)
    end
    after_step(e)
end

-- `FUN_004882a0`: a whip's point near the player hits them.
local function whip_hits(e, part, within)
    local x, _, z = e:own_part(part)
    local px, pz = player_xz(e)
    if not e:point_within(px, pz, x, z, within) or e:pl_damage_cnt() & 0x80 ~= 0
        or not e:same_floor() then
        return false
    end
    if e:player_life() >= 0 then
        local result = e:pl_life_down(e:var(WHIP), e:var(F238) < 0)
        if result == 1 then
            e:set_room_enemy(e:room_enemy() | ROOM_SPAT)
        end
        if result ~= 2 then
            local behind = e:cdir_player() and 1 or 0
            e:pl_set_routine(2, (behind ~ side(e)) + 2, 0, 0)
            e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        end
    end
    e:sound(1)
    return true
end

-- `FUN_00488230`.
local function whip_hit(e)
    for _, point in ipairs(WHIP_POINTS) do
        if whip_hits(e, point[1], point[2]) then
            return 1
        end
    end
    return 0
end

-- `FUN_00485e10`: after a lash.
local function lashed(e)
    e:set_var(T220, WAITS[e:rnd() & 7])
    e:set_var(T222, WAITS[e:rnd() & 7] + 0x1E)
    e:set_var(T221, e:timer(2) ~= 0 and 0xF or 0)
    routines(e, 1)
    e:set_body_ofs_x(side(e) ~= 0 and -800 or 800)
    backs_off(e)
end

local function whip_point_blocked(e, part)
    local x, y, z = e:own_part(part)
    return e:sca_touch(x, y, z, 0x28, 0x400)
end

-- State 4, `FUN_00485c30`: the lash.
local WHIP_STEPS <const> = {
    [0] = function(e)
        e:set_routine(2, 1)
        side_clip(e, 2)
        e:set_neck_flags(e:neck_flags() & ~2)
        e:set_var(WHIP, 0xF)
        e:spd()
        e:joint(0x40)
        local _, sy = e:speed()
        e:set_speed(0, sy, 0)
        e:set_room_enemy(e:room_enemy() | ROOM_LASHING)
        e:set_timer(2, 0)
        e:set_var(T223, 0x5A)
        e:set_var(OBA, 0)
    end,
    function(e)
        e:spd()
        local done = e:joint(0x40)
        e:add_speed_xz()
        local sx = e:speed()
        e:set_body_ofs_x(i16(e:body_ofs_x() - sx))
        local seq = e:now_seq()
        e:seq_sound()
        if seq & 0x10000 ~= 0 and e:timer(2) == 0 then
            e:set_timer(2, whip_hit(e))
        end
        if seq & 0x20000 ~= 0 then
            e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
            e:set_var(T23A, (e:var(T23A) + 1) & 0xFF)
            e:set_var(T23B, (e:var(T23B) + 1) & 0xFF)
            if whip_point_blocked(e, 0x11) or whip_point_blocked(e, 0x17) then
                e:set_routine(2, 2)
                e:set_routine(3, 0)
            end
        end
        if done then
            lashed(e)
        end
    end,
    function(e)
        if e:routine(3) == 0 then
            e:set_routine(3, 1)
            side_clip(e, 6)
        end
        e:spd()
        if e:joint(0x40) then
            lashed(e)
        end
        e:add_speed_xz()
        if e:now_seq() & 0x10000 ~= 0 and e:timer(2) == 0 then
            e:set_timer(2, whip_hit(e))
        end
    end,
}

local function lash(e)
    local step = WHIP_STEPS[e:routine(2)]
    if step then
        step(e)
    end
    hit_hard(e)
end

-- State 10, `FUN_004869d0`: the hold.
local function hold_start(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_var(OBA, 1)
        e:set_timer(0, 0x1E)
        e:set_timer(1, 0x2D)
        e:set_var(HOLD_COUNT, 0x83)
        e:set_var(COMMAND_TO, 3)
        e:set_var(T223, 0x5A)
        e:set_var(KEPT, e:var(HELD))
        side_clip(e, 8)
        e:set_var(HELD, 0)
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_var(COMMAND, 5)
        e:pl_set_routine(5, 0, 0, 0)
        e:pl_hold()
        e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        e:pl_or_be_flg(0x42)
        e:pl_set_base()
        e:sound(0xD)
    end
    local px, pz = player_xz(e)
    local bx, _, bz = e:pos()
    local by = e:goto_ck_at(px, pz, e:player_angle(), bx, bz, 0x100)
    if by ~= 0 then
        e:set_player_angle(e:player_angle() + by)
    end
    if e:joint(0x40) then
        e:set_routine(2, 1)
        e:set_routine(3, 0)
        e:set_player_angle((side(e) ~ 1) * 0x800 + e:angle())
    end
end

local function hold_squeeze(e)
    local three = e:routine(3)
    if three == 0 then
        e:set_routine(3, 1)
        e:set_var(COMMAND, 3)
        e:set_var(COMMAND_TO, 3)
        side_clip(e, 9)
        three = 1
    end
    if three == 1 then
        local timer = i16(e:timer(1) - 1)
        e:set_timer(1, timer)
        if timer == 0 then
            e:set_routine(3, 2)
            e:set_var(HOLD_COUNT, (e:var(HOLD_COUNT) - 1) & 0xFF)
        end
    elseif three == 2 or three == 3 then
        if three == 2 then
            -- The poison spit (`Routine_1` 6) isn't ported.
            e:set_routine(3, 3)
            e:set_timer(2, 3)
            e:set_timer(3, 1)
            e:sound(3)
        end
        local t3 = (e:timer(3) - 1) & 0xFF
        e:set_timer(3, t3)
        if t3 == 0 then
            e:set_timer(3, 4)
            local t2 = (e:timer(2) - 1) & 0xFF
            e:set_timer(2, t2)
            if t2 == 0 then
                e:set_routine(3, 1)
                e:set_timer(1, 0x19)
                local result = e:pl_life_down(0x28, false)
                if result == 2 then
                    -- The player's death in the hold isn't ported.
                    e:set_routine(2, 2)
                    e:set_routine(3, 0)
                else
                    e:pl_set_routine_at(2, 2)
                    e:pl_set_routine_at(3, 0)
                    if result ~= 0 then
                        e:set_room_enemy(e:room_enemy() | ROOM_SPAT)
                    end
                end
            end
        end
    end
    if e:joint(0x40) then
        e:set_hokan(0x3F)
    end
    if e:gacha() then
        e:set_timer(0, i16(e:timer(0) - 1))
    end
    if (i16(e:timer(0)) < 0 and e:player_life() >= 0) or e:var(HOLD_COUNT) & 0x7F == 0 then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
        e:pl_set_routine_at(1, 2)
        e:pl_set_routine_at(2, 0)
        e:pl_set_routine_at(3, 0)
        e:set_var(HOLD_COUNT, 0)
    end
end

local function hold_end(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        side_clip(e, 11)
        e:set_var(HELD, e:var(KEPT))
    end
    if e:joint(0x40) then
        routines(e, 1)
        e:set_var(T220, 0x1E)
        e:set_var(T222, 0x1E)
        e:set_var(T221, 0x2D)
        e:set_room_enemy(e:room_enemy() & ~ROOM_HOLDING)
        e:set_var(OBA, 0)
        e:set_be_flg(e:be_flg() & ~0x42)
    end
end

local HOLD_STEPS <const> = { [0] = hold_start, hold_squeeze, hold_end }

local function hold(e)
    local step = HOLD_STEPS[e:routine(2)]
    if step then
        step(e)
    end
    e:set_var(T23A, 1)
    e:set_var(T23B, 1)
end

local function twist(e, by)
    e:set_var(TWIST_BY, by)
    e:set_var(TWISTED, 0)
    e:set_var(TWIST_RATE, 0x20)
end

-- `FUN_00485f20`'s end: back to walking, after a wait.
local function spat(e)
    e:set_var(T222, SPIT_WAITS[e:rnd() & 7] + 0x3C)
    routines(e, 1)
    e:set_speed_x(0)
    e:set_speed_z(0)
end

-- `Esp_call` 0x1d: the spit, from part 11's last matrix, along the head.
-- Its row is `+0x224`, which `spit`'s start set from the id and `+0x23e`.
local function spit_out(e)
    local _, neck = e:part_rot(9)
    local x, y, z = e:own_part(11)
    local dir = i16(neck + side(e) * 0x800 + e:angle())
    e:call_effect(0x1D, e:var(KEPT) & 0xFF, 0xC00, dir, x, y, z)
    e:sound(3)
end

-- `FUN_00485f20`: `Routine_1` 5, the spit from range.
local function spit(e)
    local two = e:routine(2)
    if two == 0 then
        e:set_var(T223, 0x5A)
        e:set_var(KEPT, (e:id() == IVY and 0 or 8) + e:var(HOLDS))
        e:set_be_flg(e:be_flg() & ~4)
        e:set_var(OBA, 0)
        side_clip(e, 0, 0x3F)
        if e:var(HELD) == 0 then
            e:set_routine(2, 2)
            e:set_routine(3, 0)
            twist(e, 1)
        else
            e:set_routine(2, 1)
            e:set_routine(3, 0)
        end
    elseif two == 1 then
        e:joint(0x40)
        local seq = e:now_seq()
        if e:routine(3) == 0 then
            if seq & 0x10000 ~= 0 then
                e:set_routine(3, 1)
                spit_out(e)
            end
        elseif seq & 0x20000 ~= 0 then
            spat(e)
        end
    elseif two == 2 then
        local three = e:routine(3)
        local done = e:var(TWISTED) ~= 0
        if (three == 0 and done) or three == 1 then
            e:set_routine(3, 2)
            twist(e, 2)
            spit_out(e)
        elseif three == 2 and done then
            spat(e)
        end
        e:joint(0x40)
    elseif two == 3 then
        spat(e)
    end
end

-- `FUN_004865c0`: `Routine_1` 7, backing off at half its life, its head
-- twisted round until `twist_step` is done.
local function retreat(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        side_clip(e, 0)
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_var(TWIST_BY, 1)
        e:set_var(TWISTED, 0)
        e:set_var(TWIST_RATE, 8)
        e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
        e:set_var(OBA, 0)
    else
        if e:rnd() & 7 == 0 then
            blood(e, e:rnd() % 3 + 0xB, 4)
        end
        if e:var(TWISTED) ~= 0 then
            e:routines(1, 1, 0, 0)
            e:set_damage_cnt(e:damage_cnt() & 0x7F)
        end
    end
    e:joint(0x40)
end

local STATES <const> = { [0] = wait, walk, turn, stuck, lash, spit, [7] = retreat, [10] = hold }

-- `FUN_00488160` and `FUN_00488190`, from `FUN_00488140`: the head twists
-- one way and back for the spit.
local function twist_step(e)
    local by = e:var(TWIST_BY)
    if e:var(TWISTED) ~= 0 then
        return
    end
    if by == 1 then
        local value = e:var(TWIST) + e:var(TWIST_RATE)
        e:set_var(HELD, 1)
        if value >= 0x100 then
            e:set_var(TWISTED, 1)
            value = 0x100
        end
        e:set_var(TWIST, value)
    elseif by == 2 then
        local value = e:var(TWIST) - e:var(TWIST_RATE)
        e:set_var(HELD, 1)
        if value < 1 then
            e:set_var(TWISTED, 1)
            value = 0
            e:set_var(HELD, 0)
        end
        e:set_var(TWIST, value)
    end
end

-- `FUN_00488490`: sends a vine at the player when it's near.
local function send_vine(e)
    local px, pz = player_xz(e)
    local by = e:goto_ck(px, pz, 0x100)
    if e:player_life() < 0 then
        return
    end
    if e:distance() > 0x5DC then
        e:set_var(T223, 0x5A)
        return
    end
    if e:var(T223) ~= 0 then
        e:set_var(T223, e:var(T223) - 1)
        return
    end
    local right = by >= 0 and 1 or 0
    if e:var(T23A + right) == 0 and e:room_enemy() & ROOM_HOLDING == 0 then
        e:set_var(COMMAND_TO, right + 1)
        e:set_var(COMMAND, 4)
        e:set_var(T23A + right, 0xF)
    end
end

-- The vines take the hit too (state 5), unless `+0x23d` is set.
local function shake_vines(e)
    if e:var(F23D) == 0 then
        command(e, 6, 3)
    end
end

-- A hit's blood: on the head or the body for every third `Damage_no`.
local function hit_part(e)
    if e:damage_no() % 3 == 0 and e:var(F23D) == 0 then
        return (e:rnd() & 1) * 4 + 3
    end
    return 10
end

-- `caseD_2` (`0x487080`): blood and a sound, then back to the routine it
-- was in, or to walking.
local function resume(e)
    shake_vines(e)
    local weapon = e:routine(1)
    if weapon ~= 1 then
        blood(e, hit_part(e), 1)
    end
    if weapon ~= 0x10 then
        e:sound(2)
    end
    e:spd()
    local one = e:var(SAVED)
    if one == 0 then
        e:routines(1, 1, 0, 0)
        e:set_neck_flags(e:neck_flags() & ~2)
        command(e, 3, 3)
        return
    end
    e:routines(1, one, e:var(SAVED + 1), e:var(SAVED + 2))
end

-- `caseD_5` (`0x4872e0`).
local function bleed(e)
    shake_vines(e)
    local weapon = e:routine(1)
    blood(e, HIT_PARTS[e:rnd() & 7], 0)
    if weapon == 0x12 then
        blood(e, HIT_PARTS[e:rnd() & 7], 1)
    end
    e:sound(2)
    e:routines(1, e:var(SAVED), e:var(SAVED + 1), e:var(SAVED + 2))
end

-- `caseD_6` (`0x487160`): the flinch, then walking.
local function flinch(e)
    if e:var(F23D) ~= 0 then
        resume(e)
        return
    end
    if e:routine(2) ~= 0 then
        e:spd()
        if not e:joint(0x400) then
            e:add_speed_xz()
            return
        end
        e:routines(1, e:var(F23D) == 0 and 1 or 9, 0, 0)
        e:set_body_ofs_x(side(e) ~= 0 and -800 or 800)
        return
    end
    e:set_routine(2, 1)
    e:set_routine(3, 0)
    e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
    e:clip(side(e) * 13 + 3 + (e:cdir_player() and 1 or 0), 0, 0)
    e:spd()
    e:joint(0x400)
    -- `Spd_base`, so the next `Spd_get` takes frame 1's whole place.
    e:set_speed_base(0, 0, 0)
    shake_vines(e)
    blood(e, hit_part(e), 0)
    e:sound(0xB)
    e:set_be_flg(e:be_flg() & ~4)
end

-- `caseD_11` (`0x487580`), the flamethrower's: six of row 2's blood, at most
-- once in 0x3c ticks (`+0x23f`), then as `caseD_2`.
local function scorch(e)
    if e:routine(2) == 0 and e:var(F23F) == 0 then
        e:set_var(F23F, 0x3C)
        spray(e, 2, 6)
    end
    resume(e)
end

-- `funcs_487037` (`0x529ec4`), by weapon, with `censorship_off` and
-- `hard_mode` clear. Only the weapons arklay fires are ported.
local HURTS <const> = {
    resume, resume, resume, bleed, flinch, flinch, flinch, flinch,
    [0xD] = resume, -- `0x529ef8`: the Colt's, the handguns'.
    [0xF] = bleed, -- `0x529f00`: the SMG's, 4's.
    [0x10] = scorch, -- `0x529f04`: the flamethrower's.
    [0x13] = flinch, -- `0x529f10`: the Beretta's, the table's last, 0x11's.
}

-- `FUN_00487010`: `Routine_0` 2.
local function hurt(e)
    local entry = HURTS[e:routine(1)]
    if entry then
        entry(e)
    end
    send_vine(e)
    twist_step(e)
    e:rot_neck_joint(9, (e:angle() + side(e) * 0x800) & 0xFFFF, 0xC, 0x200)
end

-- `FUN_00487640`: lets go of the player it holds and starts its fall.
local function die_start(e)
    if e:var(HOLD_COUNT) ~= 0 then
        e:pl_set_routine_at(1, 2)
        e:pl_set_routine_at(2, 0)
        e:pl_set_routine_at(3, 0)
        e:set_var(HOLD_COUNT, 0)
    end
    if e:var(F23D) ~= 0 then
        e:routines(7, 0, 0, 0)
        return
    end
    e:set_routine(2, 1)
    side_clip(e, 12)
    e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
    e:set_neck_flags(e:neck_flags() | 2)
    e:set_be_flg(e:be_flg() | 4)
end

-- `FUN_004876c0`: the fall, its vines with it, until `Routine_0` 7.
local function dying(e)
    e:spd()
    local done = e:joint(0x40)
    if e:neck_flags() & 1 ~= 0 then
        -- `J_seq` builds the neck's matrix from the clip, and `Rot_neck_em`
        -- no longer turns it.
        e:set_neck_turn(0)
    end
    if done then
        e:routines(7, 0, 0, 0)
    end
    e:add_speed_xz()
    e:seq_sound()
    shake_vines(e)
    if e:now_seq() & 0x10000 ~= 0 then
        command(e, 0x41, 3)
    end
end

-- `FUN_00487600`.
local function die_plain(e)
    if e:routine(2) ~= 0 then
        dying(e)
        return
    end
    die_start(e)
    if e:routine(1) ~= 1 then
        blood(e, 10, 1)
    end
end

-- `FUN_00487740`: the shotguns and magnums blow its head or body apart.
local function die_blown(e)
    if e:routine(2) ~= 0 then
        dying(e)
        return
    end
    local weapon = e:routine(1)
    die_start(e)
    e:set_neck_flags(e:neck_flags() | 1)
    local first, last = 12, 14
    if e:damage_no() % 3 == 0 and e:var(F23D) == 0 then
        first, last = 6, 9
    end
    blood(e, first - 1, 0)
    if e:damage_no() < 3 or weapon == 6 or weapon == 8 then
        -- `FUN_00487800`.
        for part = first, last - 1 do
            e:blow_part(part, e:angle(), 300, -150)
        end
    end
end

-- `FUN_00487bf0`: the death of weapons 0x11 and 0x13. `FUN_004878f0` blows
-- its head or body apart as `die_blown` always does, with the second row of
-- blood, then the vines get 0x46 and one side's parts (0x14 to 0x19 or 0xe to
-- 0x13) go too. Both ivies' models (`EM02E`, `EM039`) have 26 parts, so every
-- part named here, and the blood's at the side's first, is there.
local function die_torn(e)
    if e:routine(2) ~= 0 then
        dying(e)
        return
    end
    die_start(e)
    e:set_neck_flags(e:neck_flags() | 1)
    local first, last = 12, 14
    if e:damage_no() % 3 == 0 and e:var(F23D) == 0 then
        first, last = 6, 9
    end
    blood(e, first - 1, 0)
    blood(e, first - 1, 3)
    for part = first, last - 1 do
        e:blow_part(part, e:angle(), 300, -150)
    end
    command(e, 0x46, 3)
    if e:rnd() & 1 == 0 then
        first, last = 0x14, 0x1A
    else
        first, last = 0xE, 0x14
    end
    for part = first, last - 1 do
        e:blow_part(part, e:angle(), 300, -150)
    end
    blood(e, first, 0)
end

-- `FUN_00487980`: the death of weapons 0xa and 0x10, burnt. Two sprays of
-- row 3's blood and the fall, the vines' command 7 and `+0x238` 1; with
-- `+0x23d` it burns 0x78 ticks where it stands, then `Routine_0` 7.
-- `FUN_00487b90`, which fades the 26 parts' `Poly_rgb` a step each tick,
-- isn't ported: the script can't set a part's colour.
local function die_burnt(e)
    local two = e:routine(2)
    if e:var(F23D) == 0 then
        if two ~= 0 then
            dying(e)
            return
        end
        spray(e, 3, 6)
        spray(e, 3, 6)
        e:set_be_flg(e:be_flg() & ~4)
        die_start(e)
        e:set_room_enemy(e:room_enemy() & ~ROOM_LASHING)
    else
        if two ~= 0 then
            local left = (e:timer(0) - 1) & 0xFFFF
            e:set_timer(0, left)
            if left == 0 then
                e:routines(7, 0, 0, 0)
            end
            return
        end
        e:set_routine(2, 1)
        e:set_timer(0, 0x78)
        spray(e, 3, 6)
        spray(e, 3, 6)
    end
    command(e, 7, 3)
    e:set_var(F238, 1)
end

-- `0x529f6c`, by weapon. Only the weapons arklay fires are ported.
local DIES <const> = {
    die_plain, die_plain, die_plain, die_plain, die_blown, die_blown, die_blown, die_blown,
    [0xD] = die_plain, -- `0x529fa0`: the Colt's, the handguns'.
    [0xF] = die_plain, -- `0x529fa8`: the SMG's, the handguns'.
    [0x10] = die_burnt, -- `0x529fac`: the flamethrower's, 0xa's.
    [0x13] = die_torn, -- `0x529fb8`: the Beretta's, the table's last, 0x11's.
}

-- `FUN_004875c0`: `Routine_0` 3.
local function die(e)
    local entry = DIES[e:routine(1)]
    if entry then
        entry(e)
    end
    e:rot_neck_joint(9, (e:angle() + side(e) * 0x800) & 0xFFFF, 0xC, 0x200)
    e:set_died()
end

-- `0x4880a0`: `Routine_0` 7, dead. `FUN_004880c0` grows its shadow, which
-- isn't drawn, over 0x5a ticks.
local function dead(e)
    e:set_var(F238, 0)
    e:set_var(F239, 0)
    e:set_be_flg(e:be_flg() | 0x42)
    local done = e:var(DEAD)
    if done == 0 then
        e:set_var(DEAD, 1)
        e:set_timer(3, 0x5A)
    elseif done ~= 1 then
        return
    end
    local left = (e:timer(3) - 1) & 0xFF
    e:set_timer(3, left)
    if left == 0 then
        e:set_var(DEAD, 2)
    end
end

-- `FUN_00484c90`.
local function init(e)
    local start = START[e:kind() & 0xF] or 0
    e:set_damage_flg(0)
    e:routines(1, start, 0, 0)
    e:set_speed(0, 0, 0)
    local life
    if e:easy() then
        life = LIFE_EASY[(e:rnd() & 0xF) + 1]
        life = life + (e:rnd() & 3)
        if e:common_a4() then
            life = life - 0x14
        end
    else
        life = LIFE[(e:rnd() & 0xF) + 1]
        life = life + (e:rnd() & 3)
        if e:common_a4() then
            life = life - 0x32
        end
    end
    e:set_life(life)
    e:set_d_life(0xD)
    e:set_var(HALF, life >> 1)
    e:set_be_flg(e:be_flg() | 0xC000000)
    e:set_neck_flags(0)
    e:set_body_ofs_x(800)
    e:set_body_box(-1500, 700, 0x5DC, 800)
    e:set_var(PARTNERS, e:other_of(IVY) and 1 or 0)
    e:set_var(T223, 0x5A)
    local set = VINE_SETS[(e:rnd() & 3) + 1]
    local chosen = e:rnd() & 3
    local count = e:kind() & 0x10 ~= 0 and 3 or 4
    for index = 0, count - 1 do
        local row = VINE_ROWS[set[index + 1]]
        local slot = e:spawn(VINE, row[1] + 1, {
            [0] = index,
            row[2],
            row[3],
            row[4],
            [7] = index == chosen and 1 or 0,
        })
        if slot then
            e:set_var(CHILDREN, e:var(CHILDREN) | (1 << index))
        end
    end
    e:set_kind(e:kind() & 0xFF0F)
    local kind = e:kind()
    if kind == 6 then
        e:set_be_flg((e:be_flg() & 0xE7FFFFFF) | 0x800000)
        e:set_body_ofs_x(100)
        return
    end
    if kind < 0x6004 or kind > 0x6005 then
        e:clip(0, 0, 0)
        e:joint(0x40)
        e:set_be_flg(e:be_flg() | 0x800000)
        return
    end
    e:clip(0xC, 0x5A, 0)
    e:joint(0x40)
    e:set_damage_cnt(e:damage_cnt() | 0x80)
    e:set_be_flg(e:be_flg() | 0x408 | 0x800000)
end

-- `FUN_00485130`.
local function main(e)
    for _, index in ipairs({ T220, T221, T222, T23A, T23B, T232 }) do
        dec(e, index)
    end
    e:set_var(TARGET, 0)
    local state = STATES[e:routine(1)]
    if state then
        state(e)
    end
    send_vine(e)
    twist_step(e)
    e:rot_neck_joint(9, (e:angle() + side(e) * 0x800) & 0xFFFF, 0xC, 0x200)
    e:set_var(SAVED, e:routine(1))
    e:set_var(SAVED + 1, e:routine(2))
    e:set_var(SAVED + 2, e:routine(3))
    if e:room_enemy() & ROOM_SPAT ~= 0 and e:player_life() > 0 then
        e:set_room_enemy(e:room_enemy() & ~ROOM_SPAT)
    end
end

local ROUTINES <const> = { [0] = init, main, hurt, die, [7] = dead }

-- `Em_2E`.
function M.update(e)
    if e:var(COMMAND) ~= 0 then
        e:set_var(COMMAND, 0)
        e:set_var(COMMAND_TO, 0)
    end
    if e:damage_cnt() & 0x7F ~= 0 then
        e:set_damage_cnt(e:damage_cnt() - 1)
    end
    dec(e, F23F)
    local routine = ROUTINES[e:routine(0)]
    if routine then
        routine(e)
    end
    local blood_left = e:var(BLOOD)
    if blood_left ~= 0 then
        e:set_var(BLOOD, blood_left - 1)
        blood(e, BLOOD_PARTS[e:rnd() & 0xF], 9)
    end
end

return M
