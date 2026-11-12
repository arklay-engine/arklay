-- The zombies, `Em_Zombie` (entity ids 0x10-0x1F): `Em10_Init`, `Em10_Move`,
-- `Em10_Damage`, `Em10_Die`, `Em10_Scenario` and the routine scripts hand them to.
--
-- Each id has a thin `enemy/{id}/enemy.lua` that calls `run` with what sets it
-- apart. The engine keeps their sight and routing, pushes and walls, and the parts
-- thrown off them, and runs the first before `update` and the rest after it.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and `e:set_var`
-- are the fields of the original's work that the engine doesn't read.

local WALK <const> = 0 -- The first byte at `+0x218`: the walks' clip.
local VOICE <const> = 1 -- The wait before the next moan.
local BITE <const> = 2 -- The wait before the next spit.
local AT_OBJ_NO <const> = 3 -- It has been staggered, and hits now knock it back.
local AT_SCE_NO <const> = 4 -- What's left before it next staggers.
local BLAST <const> = 5 -- Which of `die_thrown` (1) and `die_head` (2) took over.
local BLAST_GROUND <const> = 6 -- `Ground` when `Em10Dm02` started.
local RISING_FROM <const> = 7 -- `+0x22c`, four of them: what a hit while rising goes back to.
local RISE_SIDE <const> = 11 -- `+0x230`: which way up it's lying.

-- The motion tables.
local PLD <const> = 0
local WEAPON <const> = 1
local ROOM <const> = 2

-- The bowgun, `Routine_1` of a bolt's hit.
local BOW <const> = 0xC

-- `Type`'s bits.
local TYPE_BURNT <const> = 0x80
local TYPE_ON_FLOOR <const> = 0x2000
local TYPE_HELD <const> = 0x4000

-- `Be_flg`'s bits.
local BE_NO_SCA <const> = 8
local BE_HOLDING <const> = 0x1000
local BE_LOW <const> = 0x4000000
local BE_HIGH <const> = 0x8000000

-- The word at `+0x21a`.
local FLAG_DOWN <const> = 2
local FLAG_FACE_DOWN <const> = 4
local FLAG_THROWN <const> = 8
local FLAG_RISING <const> = 0x10
local FLAG_LOST_ARM <const> = 0x20
local FLAG_LOST_OTHER_ARM <const> = 0x40
local FLAG_HEAD_LEFT <const> = 0x80 -- `FUN_00453510`: Claire's stomp kicks the head off, and `FUN_0045b870` (zombie2.rs) sends it flying.
local FLAG_NECK_STILL <const> = 0x200
local FLAG_GUIDED <const> = 0x400
local FLAG_BURNING <const> = 0x1000
-- The walk's extra step (`FUN_00452ba0`, `FUN_00453240`): id 0x11's, and
-- `Em10_Init`'s under ARRANGE MODE's roll and under HARD.
local FLAG_QUICK_STEP <const> = 0x8000

-- `Now_seq`'s bits.
local SEQ_SOUND <const> = 0x8000000
local SEQ_BLOOD <const> = 0x10000

-- The player's `Routine_0` in a zombie's hands, and dying there.
local MODE_HELD <const> = 5
local MODE_HELD_DIE <const> = 6

-- `At_em_no` when nothing's touching.
local NO_BODY <const> = 0xFF

-- `Em10_Scenario` takes a burnt body's colours down this far.

-- `Em10_Life` and `Em10_Life_easy`.
local LIFE <const> = {
    [0] = 70, 84, 118, 65, 50, 85, 48, 65, 40, 73, 69, 56, 70, 55, 72, 55,
}
local LIFE_EASY <const> = {
    [0] = 39, 13, 27, 39, 54, 27, 39, 85, 27, 39, 27, 27, 39, 27, 39, 27,
}

-- `WORD_00526eb8`: a wander's stretches, whose top bit turns it away from the
-- player when they're far.
local STRETCHES <const> = {
    [0] = 0x00BE, 0x8032, 0x0096, 0x8040, 0x006E, 0x8040, 0x00B4, 0x8034,
    0x008C, 0x8034, 0x0078, 0x803A, 0x0096, 0x804E, 0x008C, 0x8032,
    0x0096, 0x803E, 0x0096, 0x8048, 0x006E, 0x8046, 0x0078, 0x804A,
    0x00B4, 0x8022, 0x006E, 0x8034, 0x00C8, 0x8042, 0x00B4, 0x8042,
}

-- `Em10_Init`'s walks, by two of `Rnd`: clip 0 or clip 2.
local WALKS <const> = { [0] = 0, 2, 0, 2, 0, 2, 2, 0 }

-- `FUN_00453e40`'s holds on the floor.
local FLOOR_HOLDS <const> = {
    [0] = 0x5A, 0x1E, 0xAA, 0xA0, 10, 0x28, 100, 0x32,
    0x6E, 0x14, 0x23, 0x46, 0x1E, 0xB4, 0x1E, 0x3C,
}

-- `Em10Dm00`'s `funcs_458795 + 0xb`: what a hit takes from `At_sce_no`, by weapon.
local STAGGER <const> = { [0] = 0, 9, 15, 15, 35 }

-- The clips of eating.
local EAT_CLIPS <const> = { [0] = 0x12, 0x13, 0x14, 0x12, 0x13, 0x14, 0x12, 0x13 }

local function i8(value)
    value = value & 0xFF
    if value >= 0x80 then
        return value - 0x100
    end
    return value
end

local function i16(value)
    value = value & 0xFFFF
    if value >= 0x8000 then
        return value - 0x10000
    end
    return value
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

local function drop_flag(e, flag)
    e:set_flags(e:flags() & ~flag)
end

local function raise_be_flg(e, bits)
    e:set_be_flg(e:be_flg() | bits)
end

local function raise_kind(e, bits)
    e:set_kind(e:kind() | bits)
end

local function hold_damage(e)
    e:set_damage_cnt(e:damage_cnt() | 0x80)
end

local function free_damage(e)
    e:set_damage_cnt(e:damage_cnt() & 0x7F)
end

local function hold_player(e)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
end

local function player_free(e)
    return e:pl_damage_cnt() & 0x80 == 0
end

local function lying_be_flg(e)
    e:set_be_flg((e:be_flg() & ~BE_HIGH) | BE_LOW)
end

-- `Routine_0` of 1, at this state.
local function state(e, one, two)
    e:routines(1, one, two, 0)
end

-- `Timer-- == 0`.
local function timer_out(e, which)
    local old = e:timer(which)
    e:set_timer(which, old - 1)
    return old == 0
end

-- `Rnd() ; Rnd() >> (first & 3)`, the two draws most choices make.
local function shifted(e)
    local shift = e:rnd() & 3
    return e:rnd() >> shift
end

local function rnd_dir(e)
    return e:rnd() << 4
end

local function body_box_standing(e)
    e:set_body_box(-0x5DC, 500, 0x5DC, 500)
end

local function body_box_crawling(e)
    e:set_body_box(-0x15E, 200, 0x15E, 200)
end

-- `Esp_call` at a point on a part.
local function effect(e, kind, row, size, dir, part, x, y, z)
    e:effect(kind, row, size, i16(dir), part, x, y, z)
end

-- `Esp_call` of the blood's type, on the part's matrix, which it follows.
local function blood(e, row, size, dir, part, x, y, z)
    e:effect(0, row, size, i16(dir), part, x, y, z)
end

-- A point on `pSin_parts_ptr[8]`, the head, for the blood it starts with.
local function head_point(e, x, y)
    local hx, hy, hz = e:part_offset(8)
    return hx + x, hy + y, hz
end

-- The first blood of `Em10Dm00`, `Em10Dm01` and `FUN_004587b0`: a shot at the head
-- or the body, on the side it came from.
local function hit_blood(e, size, offset, dir)
    if e:damage_no() % 3 == 0 then
        blood(e, 0, 0x17D0, dir, 1, 0, 800, 0)
    else
        local x, y, z = head_point(e, offset, 300)
        blood(e, 0, size, dir, 0, x, y, z)
    end
end

local function angle(e)
    return i16(e:angle())
end

-- `Snd_se_enem`, and the wait before the next.
local function moan(e, slot)
    e:sound(slot)
    e:set_var(VOICE, 0x96)
end

local function voice_free(e)
    return e:var(VOICE) == 0
end

-- One of two moans, by `Rnd`'s low bit.
local function moan_either(e, even, odd)
    if e:rnd() & 1 == 0 then
        moan(e, even)
    else
        moan(e, odd)
    end
end

-- The same, with no wait after it.
local function sound_either(e, even, odd)
    if e:rnd() & 1 == 0 then
        e:sound(even)
    else
        e:sound(odd)
    end
end

-- `Foot_set`'s pair, around a clip that shouldn't slide the body: the foot goes
-- back unless the blend into the clip has it moving.
local function joint_planted(e, late, stepping)
    local x, y, z = e:foot_position()
    local ended = e:joint_bank(WEAPON, late)
    if stepping then
        e:step(ended)
    end
    if e:hokan() == 0 then
        e:foot_set(x, y, z)
    end
end

local function joint_step(e, bank, late)
    e:step(e:joint_bank(bank, late))
end

local function walk_on(e, bank)
    e:spd_bank(bank)
    e:joint_bank(bank, 0x100)
    e:add_speed_xz()
end

-- The two reads of a clip's speed that start most walks: at the frame it starts
-- on, and at a random one.
local function speed_at_random(e, bank, mask)
    e:spd_bank(bank)
    e:set_move_cnt(e:rnd() & mask)
    e:spd_bank(bank)
end

-- `Timer3 * Timer1 * Timer0`: how far a hit has it leaning.
local function lean_now(e)
    return i16(e:timer(3) * i16(e:timer(1)) * i16(e:timer(0)))
end

-- `Em10_Damage`'s arm coming off: which of them, by the side of the shot. It comes
-- off at the elbow, and half the time the original also swaps the upper arm's mesh
-- for a stump, which isn't ported.
local function lose_arm(e)
    e:sound(9)
    local part
    if e:damage_flg() & 0x80 ~= 0 then
        raise_flag(e, FLAG_LOST_OTHER_ARM)
        part = 12
    else
        raise_flag(e, FLAG_LOST_ARM)
        part = 9
    end
    -- `Rnd() & 1` swaps which of the two ends bleed.
    e:rnd()
    blood(e, 0, 8000, rnd_dir(e), part, 0, 0, 0)
    blood(e, 0, 7000, rnd_dir(e), part + 1, 0, 0, 0)
    e:throw_part(part + 1, 10, -200, 0x32)
end

-- `Em10Dm01`: knocked back.
local function dm01(e)
    local routine = e:routine(2)
    if routine == 0 then
        local back = e:damage_flg() & 0x20 ~= 0
        e:clip(back and 3 or 4, 0, 3)
        e:set_routine(2, 1)
        e:set_timer(1, 10)
        e:set_speed(back and 0 or -450, 0, 0)
        e:add_speed_xz()
        e:goto_dest(0x10)
        if voice_free(e) then
            moan_either(e, 0xD, 0xC)
        end
        local turn = 0
        e:set_timer(2, 1)
        if e:damage_flg() & 0x20 ~= 0 then
            turn = 0x800
            e:set_timer(2, 0xFF)
        end
        hit_blood(e, 0x17D0, i8(e:timer(2)) * -100, angle(e) - turn)
        if e:damage_no() % 3 == 0 then
            e:set_timer(2, 0)
        end
    elseif routine == 2 then
        local at_sce = e:var(AT_SCE_NO)
        state(e, 1, 0)
        if at_sce < 1 then
            local next = e:rnd()
            e:set_var(AT_OBJ_NO, 0)
            e:set_var(AT_SCE_NO, (next & 0x10) + 0xF)
        end
        if e:turn_to_player(0x200) == 0
            and e:distance() < 4000
            and player_free(e)
            and e:rnd() & 3 == 0
        then
            state(e, 0xB, 0)
            return
        end
        if e:distance() < 3000 and e:turn_to_player(900) ~= 0 and e:rnd() & 1 ~= 0 then
            state(e, 0xC, 0)
            return
        end
        if e:rnd() & 1 ~= 0 then
            state(e, 2, 0)
        end
        return
    elseif routine ~= 1 then
        return
    end
    e:spd_bank(WEAPON)
    joint_step(e, WEAPON, 0x400)
    e:add_speed_xz()
    if e:timer(1) ~= 0 and e:timer(2) ~= 0 then
        if e:timer(1) & 1 == 0 then
            local size = 8000
            if e:var(AT_OBJ_NO) ~= 0 then
                local x, y, z = e:speed()
                e:set_speed(x, y - 500, z)
                size = 0x13E8
            end
            local first = e:rnd()
            local second = e:rnd()
            -- `Em10Dm01`'s vector on part 0 is the speed (`&Spd_x`).
            local x, y, z = e:speed()
            blood(e, 0, size, angle(e) + i16((first - second) * 0x10), 0, x, y, z)
        end
        e:set_timer(1, e:timer(1) - 1)
    end
end

-- `Em10Dm00`.
local function dm00(e)
    if e:var(AT_OBJ_NO) == 1 then
        dm01(e)
        return
    end
    local weapon = e:routine(1)
    local walk = e:var(WALK)
    if e:routine(2) == 0 then
        if e:move_no() ~= walk then
            e:clip(walk, 0, 0xF)
        end
        if voice_free(e) then
            moan_either(e, 0xC, 0xB)
        end
        e:set_timer(3, 0x18)
        local count = e:move_cnt()
        e:set_move_cnt(0)
        e:spd_bank(PLD)
        e:set_move_cnt(count)
        e:spd_bank(PLD)
        e:set_routine(2, 1)
        e:set_timer(0, 0)
        e:set_timer(1, 1)
        local at_sce = i8(e:var(AT_SCE_NO) - (STAGGER[weapon] or 0))
        e:set_var(AT_SCE_NO, at_sce)
        if e:kind() & 0x40 == 0 and at_sce < 1 and e:life() > 0x50 then
            e:set_var(AT_SCE_NO, (e:rnd() & 0xF) + 0x10)
        end
        local turn = 0
        if e:damage_flg() & 0x20 ~= 0 then
            e:set_timer(1, 0xFFFF)
            turn = 0x800
        end
        hit_blood(e, 0x17D0, i16(e:timer(1)) * -100, angle(e) - turn)
        e:goto_dest(0x10)
    end
    local routine = e:routine(2)
    if routine == 1 then
        e:goto_dest(0x10)
        e:spd_bank(PLD)
        local x, y, z = e:speed()
        e:set_speed(x // 2, y, z // 2)
        e:joint_bank(PLD, 0x100)
        e:set_lean(lean_now(e) * -8)
        local old = i16(e:timer(0))
        e:set_timer(0, old + 1)
        if old > 2 then
            e:set_routine(2, 2)
            e:set_timer(0, 0x10)
        end
    elseif routine == 2 then
        e:goto_dest(0x10)
        e:spd_bank(PLD)
        e:joint_bank(PLD, 0x100)
        e:set_lean(-lean_now(e))
        if timer_out(e, 0) then
            e:set_routine(2, 3)
        end
        if e:distance() < 0x514
            and player_free(e)
            and e:same_floor()
            and not e:partner()
        then
            local facing = e:angle()
            local flags = e:flags()
            if flags & FLAG_LOST_ARM == 0 and e:player_within(facing + 0x100, 0x120) then
                state(e, 3, 0)
            end
            if flags & FLAG_LOST_OTHER_ARM == 0 and e:player_within(facing - 0x100, 0x120) then
                state(e, 3, 0)
            end
        end
    elseif routine == 3 then
        if e:turn_to_player(0x200) == 0
            and e:distance() < 4000
            and player_free(e)
            and e:rnd() & 0xF == 0
        then
            state(e, 0xB, 0)
            return
        end
        if e:turn_to_player(0x400) ~= 0 and e:rnd() & 1 ~= 0 then
            state(e, 0xC, 0)
            return
        end
        if e:rnd() & 1 ~= 0 then
            state(e, 2, 0)
            return
        end
        state(e, 1, 0)
        if e:move_no() == walk then
            state(e, 1, 1)
            local next = e:rnd()
            e:set_timer(0, 0x1E)
            e:set_timer(3, (next & 0xF) * 2)
            next = e:rnd()
            e:set_timer(2, (next & 0x1F) + 0x1E)
            next = e:rnd()
            e:set_timer(1, next + 300)
        end
    end
    if e:move_no() == walk then
        e:add_speed_xz()
    end
end

-- `FUN_00457d10`: a hit while crawling.
local function dm_crawling(e)
    local routine = e:routine(2)
    if routine == 0 then
        local count = e:rnd() & 7
        e:set_routine(2, 1)
        e:clip(6, count, 3)
        blood(e, 0, 6000, 0, 0, 0, 0, 0)
        if voice_free(e) then
            moan(e, 0xC)
        end
    elseif routine == 2 then
        e:set_kind(1)
        e:routines(1, 0, 0, 0)
        return
    elseif routine ~= 1 then
        return
    end
    joint_step(e, WEAPON, 0x400)
end

-- `FUN_00457e60`: a hit while getting up, which only rocks it.
local function dm_rising(e)
    local first = e:routine(2) == 0
    if first then
        e:set_routine(2, 1)
        e:set_timer(0, 0)
        e:set_timer(1, 1)
        e:set_timer(2, 0)
        if e:damage_flg() & 0x20 ~= 0 then
            e:set_timer(1, 0xFFFF)
        end
        blood(e, 0, 0x17D0, angle(e), 8, 0, 300, 0)
        if e:last_routine(0) == 1 then
            for i = 0, 3 do
                e:set_var(RISING_FROM + i, e:last_routine(i))
            end
        end
        e:set_hokan(0xF)
        if voice_free(e) then
            moan(e, 0xC)
        end
        e:set_timer(3, 0x18)
    end
    local routine = e:routine(2)
    local ended = false
    if routine == 1 then
        ended = e:joint_bank(WEAPON, 0x100)
        if not ended then
            -- The first tick falls into this step with the blood's vector still
            -- in `local_48`, so its `RotMatrix` also turns 300 about y.
            e:set_lean(lean_now(e) * -8, first and 300 or 0)
            local old = i16(e:timer(0))
            e:set_timer(0, old + 1)
            if old >= 3 then
                e:set_routine(2, 2)
                e:set_timer(0, 0x10)
            end
        end
    elseif routine == 2 then
        ended = e:joint_bank(WEAPON, 0x100)
        if not ended then
            e:set_lean(-lean_now(e))
            if timer_out(e, 0) then
                e:set_routine(2, 3)
            end
        end
    elseif routine == 3 then
        ended = e:joint_bank(WEAPON, 0x100)
    end
    if ended then
        drop_flag(e, FLAG_RISING | FLAG_DOWN)
        e:routines(
            e:var(RISING_FROM),
            e:var(RISING_FROM + 1),
            e:var(RISING_FROM + 2) + 1,
            e:var(RISING_FROM + 3)
        )
        e:set_kind(e:kind() & ~TYPE_ON_FLOOR)
        raise_be_flg(e, BE_LOW | BE_HIGH)
        body_box_standing(e)
    end
end

local function become_corpse(e)
    drop_flag(e, FLAG_FACE_DOWN)
    e:routines(7, 0, 0, 0)
end

-- What a body left crawling is hit as.
local function become_crawler(e)
    body_box_crawling(e)
    lying_be_flg(e)
    e:set_kind(0x2001)
end

-- `Em10Dm05`'s legs: torn off, and sometimes the hips with them.
local function dm05_torn(e)
    local legs = e:rnd() & 1
    e:set_timer(3, legs)
    if legs ~= 0 then
        e:hide_part(3)
        e:hide_part(4)
        e:sound(2)
        if e:rnd() & 1 ~= 0 then
            blood(e, 0, 6000, 0, 5, 0, 0, 0)
            effect(e, 8, 0, 0x1000, rnd_dir(e), 5, 0, 0, 0)
            blood(e, 0, 6000, 0, 6, 0, 0, 0)
            e:hide_part(5)
            e:hide_part(6)
            e:hide_part(7)
        end
    end
    blood(e, 0, 5000, 0, 3, 0, 0, 0)
    blood(e, 0, 6000, e:rnd() << 2, 3, 0, 400, 0)
    blood(e, 0, 4000, rnd_dir(e), 3, 0, 800, 0)
    effect(e, 8, 0, 0x1000, rnd_dir(e), 3, 0, 0, 0)
end

-- `Em10Dm05`: blown off its legs, or just down. It carries on crawling.
local function dm05(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:clip(1, 0, 0xF)
        speed_at_random(e, WEAPON, 7)
        e:set_routine(2, 1)
        hold_damage(e)
        -- The bowgun's bolt tears nothing off, and leaves `Timer3`.
        if e:routine(1) ~= BOW then
            dm05_torn(e)
        end
        if voice_free(e) then
            moan(e, 0xC)
        end
        e:set_died()
    elseif routine == 2 then
        if e:routine(0) == 3 then
            if e:rnd() & 3 == 0 and e:timer(3) ~= 0 then
                become_corpse(e)
                return
            end
            e:set_life(10)
        end
        if e:partner() then
            become_corpse(e)
            return
        end
        become_crawler(e)
        local next = e:rnd()
        e:routines(1, next & 1 == 0 and 2 or 0, 0, 0)
        free_damage(e)
        return
    elseif routine ~= 1 then
        return
    end
    e:spd_bank(WEAPON)
    joint_step(e, WEAPON, 0x100)
    e:add_speed_xz()
end

-- The end of `Em10Dm02`: the body falls from where the pose has it and bounces,
-- twice.
local function dm02_bounce(e, count)
    local bounces = e:timer(2)
    if bounces ~= 0 then
        local speed = e:timer(1)
        local _, y = e:pos()
        e:set_pos_y(y + i16(speed))
        e:set_timer(1, speed + (count > 0x22 and 0x37 or 5))
    end
    local floor = e:var(BLAST_GROUND) - e:root_y()
    local _, y = e:pos()
    if floor - 200 < y and bounces ~= 0 then
        e:set_timer(1, -(i16(e:timer(1)) // 8))
        e:set_pos_y(floor - 300)
        e:set_timer(2, bounces - 1)
        e:sound(4)
    end
end

local function die_head_walks(e)
    if timer_out(e, 0) then
        drop_flag(e, FLAG_FACE_DOWN)
        e:set_routine(2, 1)
        e:clip(1, 0, 0xF)
    end
    e:goto_dest(8)
    walk_on(e, PLD)
end

-- `FUN_00459460`: the head blown off. One in two walks on first.
local function die_head(e)
    local routine = e:routine(2)
    if routine == 0 then
        local fall = e:rnd() & 1
        e:set_timer(2, fall)
        drop_flag(e, FLAG_FACE_DOWN)
        if fall ~= 0 then
            raise_flag(e, FLAG_FACE_DOWN)
        end
        e:set_routine(2, 1)
        e:clip(1 + fall, fall * 10, 3)
        hold_damage(e)
        local x, y, z = head_point(e, 0, 300)
        local dir = angle(e)
        blood(e, 0, 8000, dir, 0, x, y, z)
        effect(e, 8, 0, 0x1B58, e:direction_to_player(), 0, x, y - 400, z)
        blood(e, 2, 0x1F40, angle(e), 0, x, y - 400, z)
        e:set_neck_flags(1)
        e:hide_part(8)
        e:sound(2)
        e:set_died()
        e:set_speed_x(0xB)
        if e:rnd() & 1 ~= 0 then
            e:set_routine(2, 3)
            return
        end
        joint_planted(e, 0x100, true)
    elseif routine == 1 then
        joint_planted(e, 0x100, true)
    elseif routine == 2 then
        e:routines(7, 0, 0, 0)
    elseif routine == 3 then
        e:set_routine(2, 4)
        e:clip(e:var(WALK), 0, 0xF)
        e:set_timer(0, (e:rnd() & 0x3F) + 0x1E)
        die_head_walks(e)
    elseif routine == 4 then
        die_head_walks(e)
    end
end

-- `FUN_00456a00`: a grenade's smoke off the body, and off the head's part 3
-- while it's drawn. The darker colours it gives ten of the parts aren't ported.
local function blast_smoke(e)
    effect(e, 5, 3, 0x2710, angle(e), 0, 0, 200, 0)
    if e:hidden_parts() & (1 << 3) == 0 then
        effect(e, 5, 3, 0x13E8, angle(e), 3, 0, 200, 0)
    end
end

-- The grenade's rounds and the rocket's: what `FUN_00456a00` is called for.
local function blasted(e)
    local weapon = e:routine(WEAPON)
    return (weapon == 9 or weapon == 0x11) and e:kind() & TYPE_BURNT == 0
end

-- `FUN_00459780`: thrown down where it stands, sliding away from the shot. A bolt
-- (`Routine_1` of 0xC) bleeds only from the part it's in.
local function die_thrown(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:set_neck_flags(e:neck_flags() | 1)
        hold_damage(e)
        local dir = i16(e:direction_to_player())
        e:set_timer(0, dir)
        local count = (e:rnd() & 3) + 1
        if e:routine(1) == BOW then
            local part = e:bolt()
            e:set_timer(2, part)
            blood(e, 0, 0x17D0, dir, part, 0, 0, 0)
            count = 0
        else
            blood(e, 0, 0x1FA0, dir, 0, 0, -300, 0)
            blood(e, 0, 0x17D0, dir, 0, 0, -100, 0)
        end
        for _ = 1, count do
            local x = (0x80 - e:rnd()) * 2
            local y = (0x1C - e:rnd()) * 2
            local z = (0x80 - e:rnd()) * 2
            local turn = dir + e:rnd() * -4 + 0x200
            local size = e:rnd() * 4 + 0x600
            effect(e, 9, 2, size, turn, 0, x, y, z)
        end
        local front = e:cdir_player()
        drop_flag(e, FLAG_FACE_DOWN)
        e:set_timer(2, front and 0 or 1)
        if not front then
            raise_flag(e, FLAG_FACE_DOWN)
        end
        e:set_routine(2, 1)
        e:clip(front and 1 or 2, 0xF, 0xF)
        sound_either(e, 0xB, 0xD)
        e:set_died()
        e:set_speed_x(400)
        local x, y = e:speed()
        e:set_speed(x, y, 0)
    elseif routine == 2 then
        e:routines(7, 0, 0, 0)
        return
    elseif routine ~= 1 then
        return
    end
    local count = e:move_cnt()
    local start = e:timer(2) * 10
    if e:routine(1) ~= BOW
        and start + 4 <= count
        and count <= start + 0x10
        and count & 1 ~= 0
    then
        local dir = rnd_dir(e)
        local size = e:rnd() * 8 + 0xFD0
        local x, y, z = head_point(e, 0, 500)
        blood(e, 0, size, dir, 0, x, y, z)
    end
    local x, y, z = e:foot_position()
    joint_step(e, WEAPON, 0x100)
    e:add_speed_xz_at(e:timer(2) << 11)
    local speed = e:speed_x() - 0x32
    e:set_speed_x(speed > 0 and speed or 0)
    if speed < 1 then
        e:foot_set(x, y, z)
    end
end

-- `Em10Dm02`: thrown back off its feet, to land crawling.
local function dm02(e)
    local blast = e:var(BLAST)
    if blast == 1 then
        die_thrown(e)
        return
    elseif blast == 2 then
        die_head(e)
        return
    end
    local routine = e:routine(2)
    if routine == 0 then
        if e:shot_high() then
            e:set_var(BLAST, 2)
            die_head(e)
            return
        end
        if e:partner() then
            e:set_var(BLAST, 1)
            die_thrown(e)
            return
        end
        if e:rnd() & 3 ~= 0 and e:routine(0) == 3 then
            e:set_var(BLAST, 1)
            die_thrown(e)
            return
        end
        local back = e:damage_flg() & 0x20 ~= 0
        e:clip(back and 3 or 4, 0, 3)
        e:set_routine(2, 1)
        e:set_timer(0, 0)
        e:set_timer(3, 0)
        e:set_speed(back and 100 or -250, 0, 0)
        e:add_speed_xz()
        blood(e, 0, 6000, rnd_dir(e), 0, 0, 0, 0)
        local dir = rnd_dir(e)
        effect(e, 8, 0, e:rnd() * 2 + 4000, dir, 0, 0, 0, 0)
        for _ = 1, (e:rnd() & 3) + 2 do
            dir = rnd_dir(e)
            effect(e, 9, 2, e:rnd() + 0xC00, dir, 0, 0, 0, 0)
        end
        if voice_free(e) then
            moan_either(e, 0xD, 0xC)
        end
        e:sound(9)
        local _, y = e:pos()
        e:set_var(BLAST_GROUND, y)
        e:set_died()
        hold_damage(e)
    elseif routine == 2 then
        drop_flag(e, FLAG_FACE_DOWN)
        free_damage(e)
        e:set_pos_y(e:var(BLAST_GROUND))
        raise_flag(e, 1)
        if e:routine(0) == 3 then
            e:set_life(10)
        end
        become_crawler(e)
        e:routines(1, 0, 0, 0)
        e:clip(5, 0, 0xF)
        e:joint_bank(WEAPON, 0x100)
        return
    elseif routine ~= 1 then
        return
    end
    e:spd_bank(WEAPON)
    joint_step(e, WEAPON, 0x400)
    e:add_speed_xz()
    local count = e:move_cnt()
    if count ~= 0 and count & 3 == 0 then
        local dir = rnd_dir(e)
        local size = e:rnd() * 8 + 6000
        local x, y, z = e:speed()
        blood(e, 0, size, dir, 0, x, y, z)
    end
    if e:routine(2) ~= 1 then
        return
    end
    if count == 0x14 then
        e:set_timer(1, 10)
        e:set_timer(2, 2)
        blood(e, 3, 0x1000, rnd_dir(e), 1, 0, 0, 0)
        e:sound(9)
    end
    if count > 0x13 then
        dm02_bounce(e, count)
    end
end

-- `Em10Dm04`: a bolt that didn't go into a leg (part 3 or 6, which is `Em10Dm05`'s)
-- pushes it back along the bolt's way, slowing, then it walks again. Its states 2 to 5,
-- which tip the root over, have no way in from here and aren't ported, nor is part 0's
-- `Be_flg` bit 0x40 it clears.
local function dm04(e)
    local routine = e:routine(2)
    if routine == 0 then
        local part, yaw = e:bolt()
        e:set_timer(2, part)
        if part == 3 or part == 6 then
            e:set_damage_no(3)
            return
        end
        e:set_timer(0, yaw)
        e:set_routine(2, 1)
        e:clip(e:var(WALK), 0, 0xF)
        if voice_free(e) then
            moan_either(e, 0xD, 0xC)
        end
        e:set_speed(300, 0, 0)
        e:set_timer(1, 0)
        e:set_timer(3, 0x32)
        local dir = e:direction_to_player()
        blood(e, 0, e:rnd() * 8 + 6000, dir, part, 0, 0, 0)
    elseif routine == 6 then
        e:routines(1, e:rnd() & 1 ~= 0 and 2 or 1, 0, 0)
        return
    elseif routine ~= 1 then
        return
    end
    local speed = e:speed_x()
    local x, z = e:rotate(e:timer(0), speed, 0)
    e:joint(0x100)
    e:set_speed_x(speed - i16(e:timer(1)))
    e:add_pos(x, 0, z)
    e:set_timer(1, e:timer(1) + i8(e:timer(3)))
    if i16(e:speed_x()) < 0 then
        e:set_routine(2, 6)
    end
end

-- `Em10_Damage`, by `DWORD_00526ed4`. The other weapons' branches, which light and
-- freeze it, aren't ported.
local function em10_damage(e)
    e:let_go()
    if e:kind() & 1 ~= 0 then
        dm_crawling(e)
        return
    end
    if has_flag(e, FLAG_RISING) then
        dm_rising(e)
        return
    end
    local weapon = e:routine(1)
    local threshold = 0
    if e:kind() & 0x40 ~= 0 and weapon ~= 1 then
        threshold = 0x17
    end
    if e:routine(2) == 0 and e:var(AT_SCE_NO) <= threshold then
        if e:var(AT_OBJ_NO) == 1 or weapon == 1 then
            e:set_var(AT_OBJ_NO, 1)
            state(e, 5, 0)
            e:set_var(AT_SCE_NO, (e:rnd() & 0xF) + 0x10)
            local dir = angle(e)
            if e:damage_no() % 3 ~= 0 then
                local x, y, z = head_point(e, 0, 500)
                blood(e, 0, 0x1000, dir, 0, x, y, z)
            else
                blood(e, 0, 0x17D0, dir, 1, 0, 800, 0)
            end
            return
        end
        e:set_var(AT_OBJ_NO, 1)
    end
    if has_flag(e, FLAG_DOWN) then
        state(e, 5, 6)
        e:set_timer(3, 1)
        return
    end
    if e:flags() & (FLAG_LOST_ARM | FLAG_LOST_OTHER_ARM) == 0
        and e:d_life(1) >= 0x80
        and e:damage_flg() & 0xC0 ~= 0
    then
        lose_arm(e)
    end
    local no = e:damage_no()
    if weapon == BOW then
        -- Its row has no call for 2, 5 and 8.
        if no % 3 == 0 then
            dm05(e)
        elseif no % 3 == 1 then
            dm04(e)
        end
        return
    end
    local blast = weapon == 7 or weapon == 8
    if blast and (no == 0 or no == 3 or no == 6) then
        dm05(e)
    elseif blast and no == 1 then
        dm02(e)
    elseif (blast and no == 4) or (weapon == 8 and no == 7) then
        dm01(e)
    else
        dm00(e)
    end
end

-- The end of a dying state's first tick: `Em_dead_flg_on` and the last moan.
local function death_cry(e)
    sound_either(e, 0xB, 0xD)
    e:set_died()
end

-- `FUN_004587b0`.
local function die_standing(e)
    local routine = e:routine(2)
    if routine == 0 then
        local fall = e:rnd() & 1
        if e:flags() & 0x2000 ~= 0 then
            fall = 0
        end
        e:set_timer(2, fall)
        drop_flag(e, FLAG_FACE_DOWN)
        if fall ~= 0 then
            raise_flag(e, FLAG_FACE_DOWN)
        end
        e:set_routine(2, 1)
        e:clip(1 + fall, fall * 10, 0xF)
        e:set_neck_flags(e:neck_flags() | 1)
        hold_damage(e)
        hit_blood(e, 0x1FA0, 0, angle(e))
        death_cry(e)
        e:set_timer(3, 0)
        -- The flame and acid rounds' branches, and the spark shot's and the
        -- flamethrower's, aren't ported.
        if blasted(e) then
            blast_smoke(e)
        end
        e:set_speed_x(0xB)
    elseif routine == 2 then
        e:routines(7, 0, 0, 0)
        if e:rnd() & 3 ~= 0
            or has_flag(e, FLAG_FACE_DOWN)
            or e:timer(3) ~= 0
            or e:partner()
        then
            return
        end
        e:set_life(1)
        body_box_crawling(e)
        lying_be_flg(e)
        e:set_kind(0x2001)
        local next = e:rnd()
        e:routines(1, next & 7 ~= 0 and 2 or 0, 0, 0)
        free_damage(e)
        return
    elseif routine ~= 1 then
        return
    end
    if e:damage_no() % 3 ~= 0 then
        local count = e:move_cnt()
        local start = e:timer(2) * 10
        if start + 4 <= count and count <= start + 0x10 and count & 1 ~= 0 then
            local size = e:rnd() * 8 + 0xFD0
            local dir = rnd_dir(e)
            local x, y, z = head_point(e, 0, 500)
            blood(e, 0, size, dir, 0, x, y, z)
        end
    end
    joint_planted(e, 0x100, true)
end

-- `FUN_00458bf0`.
local function die_crawling(e)
    local routine = e:routine(2)
    if routine == 0 then
        drop_flag(e, FLAG_FACE_DOWN)
        hold_damage(e)
        e:clip(7, 0, 0xF)
        e:set_routine(2, 1)
        e:set_neck_flags(e:neck_flags() | 1)
        blood(e, 0, 0x1FA0, angle(e), 0, 0, 300, 0)
        death_cry(e)
    elseif routine == 2 then
        e:routines(7, 0, 0, 0)
        return
    elseif routine ~= 1 then
        return
    end
    joint_step(e, WEAPON, 0x100)
end

-- The turn of a spurt from a blast: the facing, a little either way.
local function blast_turn(e)
    local first = e:rnd()
    local second = e:rnd()
    return angle(e) + i16((first - second) * 0x10)
end

local function die_torn_falls(e)
    e:spd_bank(WEAPON)
    joint_step(e, WEAPON, 0x400)
    e:add_speed_xz()
    if e:timer(1) ~= 0 then
        if e:timer(1) & 1 == 0 then
            local x, y, z = e:speed()
            for _ = 1, 2 do
                blood(e, 0, 8000, blast_turn(e), 0, x, y, z)
            end
        end
        e:set_timer(1, e:timer(1) - 1)
    end
end

local function die_torn_walks(e)
    if timer_out(e, 0) then
        drop_flag(e, FLAG_FACE_DOWN)
        e:set_routine(2, 3)
        e:clip(1, 0, 7)
    end
    e:goto_dest(8)
    walk_on(e, PLD)
end

-- `FUN_00458d40`: the custom shotgun takes the head and arms off and tears the
-- body. The torn body's own mesh isn't ported.
local function die_torn(e)
    if e:var(BLAST) == 1 then
        die_thrown(e)
        return
    end
    local routine = e:routine(2)
    if routine == 0 then
        local weapon = e:routine(WEAPON)
        local blast = weapon == 9 or weapon == 0x11
        if e:rnd() & 3 ~= 0 and not e:shot_high() and not blast then
            e:set_var(BLAST, 1)
            die_thrown(e)
            return
        end
        if blasted(e) then
            blast_smoke(e)
        end
        local shift = e:rnd() & 7
        if (e:rnd() >> shift) & 3 ~= 0 and weapon == 9 then
            e:set_var(BLAST, 1)
            die_thrown(e)
            return
        end
        e:keep_arms()
        drop_flag(e, FLAG_FACE_DOWN)
        local back = e:damage_flg() & 0x20 ~= 0
        e:set_timer(3, back and 1 or 0)
        if not back then
            raise_flag(e, FLAG_FACE_DOWN)
        end
        e:clip(back and 3 or 4, 0, 3)
        e:set_routine(2, 1)
        e:set_timer(1, 10)
        e:set_speed(back and 0xFA or -0x1C2, 0, 0)
        e:add_speed_xz()
        e:set_neck_flags(1)
        hold_damage(e)
        e:hide_part(8)
        blood(e, 0, 3000, rnd_dir(e), 8, 0, 0, 0)
        if not has_flag(e, FLAG_LOST_ARM) then
            e:throw_kept_arm(9, 100, 0, 0x1E)
        else
            e:hide_part(9)
            e:hide_part(10)
            e:hide_part(11)
        end
        if not has_flag(e, FLAG_LOST_OTHER_ARM) and e:rnd() & 1 == 0 then
            e:throw_kept_arm(12, 100, 0, 0x1E)
        end
        blood(e, 0, 8000, blast_turn(e), 0, 0, -500, 0)
        local dir = rnd_dir(e)
        effect(e, 8, 0, e:rnd() * 4 + 16000, dir, 0, 0, -500, 0)
        e:sound(2)
        e:set_died()
        e:set_speed_x(0xB)
        die_torn_falls(e)
    elseif routine == 1 then
        die_torn_falls(e)
    elseif routine == 2 then
        if e:rnd() & 1 ~= 0 then
            e:set_routine(2, 5)
            return
        end
        e:set_routine(2, 3)
        local side = e:timer(3) & 0xFF
        e:clip(side ~= 0 and 1 or 2, 0xA + side * 0xF, 7)
        joint_planted(e, 0x200, true)
    elseif routine == 3 then
        joint_planted(e, 0x200, true)
    elseif routine == 4 then
        e:routines(7, 0, 0, 0)
    elseif routine == 5 then
        e:set_routine(2, 6)
        e:clip(e:var(WALK), 0, 0xF)
        e:set_timer(0, (e:rnd() & 0x1F) + 0xF)
        die_torn_walks(e)
    elseif routine == 6 then
        die_torn_walks(e)
    end
end

-- The frame it's too late by, then the clips either side of it.
local FALLS <const> = {
    [0] = { 0x37, 0x16, 0x01 }, { 0x28, 0x17, 0x01 }, { 0x00, 0x01, 0x01 },
}

-- `FUN_00459b60`: shot dead while getting up. The clip carries on from the rise's
-- if it's early enough, by which way up it is.
local function die_rising(e)
    local routine = e:routine(2)
    if routine == 0 then
        local side = e:var(RISE_SIDE)
        raise_flag(e, FLAG_FACE_DOWN)
        if side == 1 then
            drop_flag(e, FLAG_FACE_DOWN)
        end
        local falls = FALLS[side < 2 and side or 2]
        if e:move_cnt() < falls[1] then
            e:set_timer(3, 0)
            e:clip(falls[2], 0, 0x1F)
        else
            drop_flag(e, FLAG_FACE_DOWN)
            e:set_timer(3, 1)
            e:clip(falls[3], 0x1E, 0x1F)
        end
        hold_damage(e)
        e:set_routine(2, 1)
        e:set_neck_flags(e:neck_flags() | 1)
        local x, y, z = head_point(e, 0, 300)
        blood(e, 0, 0x1FA0, angle(e), 0, x, y, z)
        death_cry(e)
        e:set_speed_x(0xB)
    elseif routine == 2 then
        e:joint_bank(WEAPON, 0x80)
        if e:hokan() == 0 then
            e:set_routine(2, 3)
        end
        return
    elseif routine == 3 then
        e:routines(7, 0, 0, 0)
        return
    elseif routine ~= 1 then
        return
    end
    if e:joint_bank(WEAPON, 0x80) then
        e:set_routine(2, e:timer(3) ~= 0 and 3 or 2)
    end
end

-- `funcs_458795`: `Em10_Die`'s routine by `Routine_1`, then `Damage_no`. The
-- original's empty cells are never reached; a weapon not here stands to die.
local DIES <const> = (function()
    local S, D5, H, D2, T, TH, C = die_standing, dm05, die_head, dm02, die_torn, die_thrown, die_crawling
    local magnum = { D5, H, S, D5, H, S, D5, H, S }
    local rows = {
        [5] = magnum,
        [6] = magnum,
        [7] = { D5, D2, S, D5, S, S, D5, S, S },
        [8] = { D5, T, S, D5, TH, S, D5, S, S },
        [9] = { D5, T, T, S, S, S, S, S, S },
        [0xC] = { S, TH, S, S, TH, S, S, TH, S },
        [0x11] = { D5, T, S, D5, TH, S, D5, S, S },
        [0x13] = { C, C, C, C, C, C, C, C, C },
    }
    return rows
end)()

-- `Em10_Die`.
local function em10_die(e)
    e:let_go()
    drop_flag(e, FLAG_NECK_STILL)
    if e:kind() & 1 ~= 0 then
        die_crawling(e)
    elseif has_flag(e, FLAG_RISING) then
        die_rising(e)
    elseif not has_flag(e, FLAG_DOWN) then
        local row = DIES[e:routine(WEAPON)]
        local die = row and row[e:damage_no() + 1] or die_standing
        die(e)
    else
        blood(e, 0, 8000, 0, 0, 0, 0, 0)
        sound_either(e, 0xB, 0xD)
        -- The other weapons' calls here aren't ported.
        if blasted(e) then
            blast_smoke(e)
        end
        e:routines(7, 9, 0, 0)
    end
end

-- `FUN_00459e10`.
local function scene_stand(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:clip(0, 0, 0xF)
        e:set_routine(2, 1)
    elseif routine ~= 1 then
        return
    end
    joint_planted(e, 0x100, false)
end

-- `FUN_00459e70`: on to `Dest_x`, `Dest_z`.
local function scene_walk(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:set_routine(2, 1)
        e:clip(e:var(WALK), 0, 0xF)
    elseif routine ~= 1 then
        return
    end
    e:goto_dest(0x24)
    walk_on(e, PLD)
end

-- `FUN_00459ef0`: at the body `Sce_free3` names, and take hold of it.
local function scene_grab(e)
    local routine = e:routine(2)
    if routine == 0 or routine == 1 then
        if routine == 0 then
            e:clip(10, 0, 0xF)
            e:set_routine(2, 1)
            local x, _, z = e:scene_target()
            e:goto_point(x, z, 0x400)
            e:set_timer(0, 0)
            e:sound(10)
            hold_damage(e)
        end
        e:set_timer(0, e:timer(0) + 1)
        if e:timer(0) & 1 == 0 then
            e:set_speed_x(e:speed_x() // 4)
        else
            e:spd_bank(WEAPON)
        end
        joint_step(e, WEAPON, 0x100)
        e:add_speed_xz()
        if e:move_cnt() == 0x11 then
            e:set_routine(2, 2)
        end
    elseif routine == 2 or routine == 3 then
        if routine == 2 then
            e:clip(0x18, 0, 0xF)
            e:set_routine(2, 3)
            e:set_bank(WEAPON)
            e:grab_body()
            raise_be_flg(e, BE_HOLDING)
        end
        e:null_pos_set(0)
        joint_step(e, WEAPON, 0x100)
    elseif routine == 4 then
        e:routines(8, 3, 2, 0)
    end
end

-- `FUN_0045a070`: over the body `Sce_free3` names, eating it.
local function scene_eat(e)
    local routine = e:routine(2)
    if routine == 0 or routine == 1 then
        if routine == 0 then
            e:set_routine(2, 1)
            e:clip(0x15, e:rnd() & 7, 0xF)
            hold_damage(e)
        end
        local x, _, z = e:scene_target()
        e:goto_point(x, z, 0x10)
        e:step(e:joint_bank(WEAPON, 0x100, true))
    elseif routine == 2 or routine == 3 then
        if routine == 2 then
            e:set_routine(2, 3)
            e:clip(EAT_CLIPS[e:rnd() & 7], 0, 7)
        end
        joint_planted(e, 0x200, true)
        if e:now_seq() & SEQ_BLOOD ~= 0 then
            blood(e, 0, 0x17B8, rnd_dir(e), 8, 200, 0, 0)
        end
    elseif routine == 4 then
        local clip = e:rnd() & 7
        e:set_routine(2, 1)
        e:clip(EAT_CLIPS[clip], 3, 0)
    end
end

-- `FUN_00459df0`, the routine scripts hand a zombie to. Its table's last entry,
-- `FUN_0045a240`, no room's script asks for.
local function em10_scene(e)
    local which = e:routine(1)
    if which == 0 then
        scene_stand(e)
    elseif which == 1 then
        scene_walk(e)
    elseif which == 2 then
        scene_grab(e)
    elseif which == 3 then
        scene_eat(e)
    end
end

-- `Half_x` and `Half_z` grow while `Timer2` counts down.
local function spread_pool(e)
    local left = e:timer(2)
    if left ~= 0 then
        e:set_timer(2, left - 1)
        e:set_pool(e:pool() + 5)
    end
end

local function corpse_wait(e)
    spread_pool(e)
    if timer_out(e, 3) then
        e:set_routine(1, 1)
    end
end

-- `Em10_Scenario`, the state a body stays in: the pool under it spreads and a few
-- of them keep twitching.
local function em10_corpse(e)
    local joint = false
    local step = e:routine(1)
    if step == 0 then
        e:clip(has_flag(e, FLAG_FACE_DOWN) and 0x16 or 0x17, 0, 7)
        e:set_neck_flags(e:neck_flags() | 1)
        e:set_life(-1)
        e:set_routine(1, 1)
        raise_be_flg(e, 2)
        e:set_timer(2, 0x78)
        e:set_timer(0, 0x28)
        e:set_speed_x(0)
        e:set_died()
        if e:rnd() % 3 ~= 0 then
            e:set_routine(1, 8)
            return
        end
        e:set_timer(1, 0)
        spread_pool(e)
        joint = true
    elseif step == 1 then
        spread_pool(e)
        joint = true
    elseif step == 5 or step == 10 then
        joint = true
    elseif step == 2 then
        local next = e:rnd()
        e:set_routine(1, 3)
        e:set_timer(3, (next & 0xF) + 1)
        if not timer_out(e, 0) then
            corpse_wait(e)
        else
            e:set_routine(1, 4)
            e:set_timer(0, 0x1E)
        end
    elseif step == 3 then
        corpse_wait(e)
    elseif step == 4 then
        if timer_out(e, 3) then
            e:set_routine(1, 5)
        end
    elseif step == 6 then
        local next = e:rnd()
        e:set_routine(1, 4)
        e:set_timer(3, (next & 0x3F) + 0x3C)
        if timer_out(e, 0) then
            e:set_routine(1, 7)
        end
    elseif step == 8 then
        spread_pool(e)
        if e:timer(2) == 0 then
            e:set_routine(1, 7)
        end
    elseif step == 9 then
        e:clip(has_flag(e, FLAG_FACE_DOWN) and 0x16 or 7, 0, 7)
        e:set_routine(1, 10)
        joint = true
    elseif step == 0xB then
        e:set_routine(1, 0)
    end
    if joint and e:joint_bank(WEAPON, 0x200) then
        e:set_routine(1, e:routine(1) + 1)
    end
    e:set_timer(1, e:timer(1) + 1)
    if e:kind() & TYPE_BURNT ~= 0 and e:timer(1) & 3 == 0 then
        e:char_parts()
    end
end

-- `FUN_00456780`: fire on the body and, unless `Routine_1` is 0x10, darker
-- parts.
local function light_flames(e)
    local first_row = e:rnd() & 1 == 0
    if e:routine(1) ~= 0x10 then
        e:burn_parts()
    end
    if first_row then
        e:light_flame(10000, 0, 0, 0, 0)
        e:light_flame(5000, 3, 0, 200, 0)
        e:light_flame(5096, 6, 0, -200, 0)
    else
        e:light_flame(10000, 0, 0, 200, 0)
        e:light_flame(5000, 8, 0, 0, 0)
        e:light_flame(5096, 3, 0, 200, 0)
    end
end

local function lie_down(e, clip)
    e:clip(clip, 0, 0)
    state(e, 7, 0)
end

-- `Em10_Init`.
local function init(e, v)
    e:routines(1, 0, 0, 0)
    e:set_damage_cnt(0)
    e:set_d_life(0xD)
    e:set_sce_flg(0)
    e:set_life(LIFE[shifted(e) & 0xF])
    if e:easy() then
        e:set_life(LIFE_EASY[shifted(e) & 0xF])
    elseif e:enemies() < 4 then
        e:set_life(e:life() + 0xF)
    end
    e:set_neck_flags(0)
    e:set_var(WALK, WALKS[shifted(e) & 7])
    e:set_flags(0)
    e:set_var(AT_OBJ_NO, 0)
    e:set_var(AT_SCE_NO, (e:rnd() & 0xF) + 0x10)
    if v.life then
        raise_flag(e, FLAG_QUICK_STEP)
        e:set_life(v.life)
    end
    -- ARRANGE MODE: one in three takes the quick step and staggers later.
    if e:censorship_off() and shifted(e) % 3 == 0 then
        raise_flag(e, FLAG_QUICK_STEP)
        e:set_var(AT_SCE_NO, (e:rnd() & 0xF) + 0x20)
    end
    -- HARD: five times the life up to stage 4 (rooms 1xx to 4xx), three
    -- times after, in a 16-bit multiply; and every one takes the quick step.
    if e:hard_mode() then
        e:set_life(i16(e:life() * (e:room_code() < 0x500 and 5 or 3)))
        raise_flag(e, FLAG_QUICK_STEP)
        e:set_var(AT_SCE_NO, (e:rnd() & 0xF) + 0x20)
    end
    body_box_standing(e)
    raise_be_flg(e, BE_LOW | BE_HIGH)
    if v.parts0_y then
        e:set_parts0_y(v.parts0_y)
    end
    e:clip(1, 0, 0)
    local kind = e:kind() & 0x3F
    if kind == 2 then
        raise_flag(e, FLAG_FACE_DOWN)
        lie_down(e, 0x16)
    end
    if kind == 4 then
        lie_down(e, 0x17)
    end
    if kind == 5 then
        raise_flag(e, FLAG_FACE_DOWN)
        lie_down(e, 0x16)
        e:set_kind(0x4002)
        e:set_life(-1)
    end
    if kind == 7 then
        lie_down(e, 0x17)
        e:set_kind(0x4004)
        e:set_life(-1)
    end
    kind = e:kind()
    if kind & 0x3F == 6 then
        state(e, 1, 0)
    end
    if kind & 0x3F == 8 then
        e:clip(0x12, 0, 0)
        state(e, 8, 0)
    end
    if kind & 1 ~= 0 then
        e:clip(0x17, 0, 0)
        raise_kind(e, TYPE_ON_FLOOR)
        lying_be_flg(e)
        body_box_crawling(e)
        e:set_life(quot(e:life(), 2))
    end
    kind = e:kind() & 0x3F
    if kind == 1 then
        state(e, 2, 0)
    elseif kind == 3 then
        state(e, 0, 0)
    end
    e:joint_bank(WEAPON, 0x100)
    if e:kind() & 0x3F == 10 then
        hold_damage(e)
        e:set_flags((e:flags() & ~FLAG_FACE_DOWN) | FLAG_DOWN)
        e:set_body_box(-0x15E, 0, 0x15E, 0)
        state(e, 0xF, 0)
        e:set_be_flg((e:be_flg() & ~BE_HIGH) | BE_LOW | BE_NO_SCA)
        e:clip(0, 0, 0)
        e:joint_bank(ROOM, 0x100)
    end
    for _ = 1, v.init_draws or 0 do
        e:rnd()
    end
    if v.skins then
        local model_type = e:model_type()
        local skin = 0
        if model_type & 0xC0 == 0 then
            local shift = e:rnd() & 3
            local high = e:rnd()
            local low = e:rnd()
            skin = (low + (high << shift)) % 3 + 1
        end
        if model_type & 0x3F ~= 0 then
            skin = model_type & 0x3F
        end
        e:set_skin(skin)
    end
    if e:kind() & TYPE_BURNT ~= 0 then
        light_flames(e)
    end
    e:set_hidden_parts(e:hidden_parts() | 1 << 0xF | 1 << 0x10)
end

-- The two checks of a reach, either side of the facing. Bits 0x20 and 0x40 at
-- `+0x21a` are an arm the zombie has lost.
local function reach_for_player(e, to)
    local facing = e:angle()
    if not has_flag(e, FLAG_LOST_ARM) and e:player_within(facing + 0x100, 0x100) then
        state(e, to, 0)
        hold_player(e)
    end
    if not has_flag(e, FLAG_LOST_OTHER_ARM) and e:player_within(facing - 0x100, 0x100) then
        state(e, to, 0)
        hold_player(e)
    end
end

-- `FUN_00452630`.
local function look_stand(e)
    local distance = e:distance()
    local wide = e:turn_to_player(0x400)
    local close = e:turn_to_player(800)
    local in_sight = e:in_sight()
    local noise = e:noise() & 0xFF
    if distance < 5000 and wide == 0 and in_sight then
        state(e, 1, 0)
    end
    if distance < 3000 and close ~= 0 and noise & 0x15 ~= 0 and in_sight and e:rnd() & 1 == 0 then
        state(e, 0xC, 0)
    end
    if distance < 2000 and wide ~= 0 and in_sight and noise & 0x17 ~= 0 and e:rnd() & 1 ~= 0 then
        state(e, 0xC, 0)
    end
end

-- `FUN_00452700`.
local function mv_stand(e)
    if e:routine(2) == 0 then
        local count = e:rnd() & 0x1F
        e:set_routine(2, 1)
        e:clip(0, count, 0xF)
        e:set_timer(0, e:rnd() + 0x96)
        e:set_timer(1, e:rnd() + 300)
    end
    local routine = e:routine(2)
    if routine == 1 then
        local x, y, z = e:foot_position()
        e:joint_bank(WEAPON, 0x100)
        if timer_out(e, 1) then
            if voice_free(e) and e:rnd() & 1 ~= 0 then
                moan(e, 0xB)
            end
            e:set_timer(1, e:rnd() + 300)
        end
        if e:hokan() == 0 then
            e:foot_set(x, y, z)
        end
        if e:timer(1) == 0 and e:rnd() & 1 == 0 then
            e:set_routine(2, 2)
        end
        if e:timer(0) ~= 0 then
            e:set_timer(0, e:timer(0) - 1)
            return
        end
        if e:distance() < 0x1D4C and e:in_sight() then
            state(e, 1, 0)
        end
    elseif routine == 2 or routine == 3 then
        if routine == 2 then
            e:set_routine(2, 3)
            e:clip(e:var(WALK), 0, 0xF)
            e:set_timer(0, e:rnd() + 0x96)
            -- `Rnd_area`, whose choice nothing reads.
            if e:has_guides() then
                e:rnd()
            end
            raise_flag(e, FLAG_GUIDED)
        end
        e:goto_dest(0x10)
        walk_on(e, PLD)
        e:set_timer(0, e:timer(0) - 1)
        if e:timer(0) == 0 then
            drop_flag(e, FLAG_GUIDED)
            e:set_routine(2, 0)
        end
        if e:distance() < 3000 or (e:distance() < 0x1D4C and e:in_sight()) then
            drop_flag(e, FLAG_GUIDED)
            state(e, 1, 0)
        end
    end
end

-- The end of `FUN_00452930` and `FUN_00453010`.
local function look_close(e, narrow)
    local distance = e:distance()
    if e:player_id() == 0xF then
        if e:var(BITE) == 0 and narrow == 0 and distance < 2000 and e:pl_damage_cnt() == 0 then
            state(e, 0xE, 0)
        end
    elseif distance < 0x4B0 and player_free(e) and e:same_floor() then
        reach_for_player(e, 3)
    end
    -- The player's body, once `Life` holds its `FUN_0045b070` mark.
    if e:player_life() == -0x8000 and narrow == 0 and distance < 1000 then
        raise_kind(e, TYPE_HELD)
        state(e, 8, 6)
    end
end

-- `Sce_free0`'s scripted wall, once the zombie is against one: its high
-- byte (`+0x1D5`) holds the script's bits, its low twelve the way to face.
local function wall_check(e)
    if (e:sce_free(0) & 0xFFFF) >> 8 & 0xC0 ~= 0 and e:sca_info() & 1 ~= 0 then
        state(e, 0xA, 0)
    end
end

-- The first test of `FUN_00452930` and `FUN_00453010`: on the partner's list
-- (`L_spl`'s top bits), within 2000 of her and facing her within 0x100, the
-- zombie spits at her.
local function spit_at_partner(e)
    local l_spl = e:l_spl()
    return e:var(BITE) == 0
        and l_spl & 0xC0000000 ~= 0
        and l_spl & 0x3FFFFFFF < 2000
        and e:turn_to(e:dir_spl(), 0x100) == 0
end

-- `FUN_00452930`.
local function look_walk(e)
    local distance = e:distance()
    local wide = e:turn_to_player(0x400)
    local narrow = e:turn_to_player(0x200)
    local noise = e:noise() & 0xFF
    if spit_at_partner(e) then
        state(e, 0xE, 0)
        return
    end
    if e:var(BITE) == 0 then
        if narrow == 0 and distance < 2000 and not e:same_floor() and e:pl_damage_cnt() == 0 then
            state(e, 0xE, 0)
        end
    end
    wall_check(e)
    if distance < 0xDAC and wide ~= 0 and noise & 0x15 ~= 0 and e:rnd() & 3 == 0 then
        state(e, 0xC, 0)
    end
    if distance < 0x9C4 and wide ~= 0 and noise & 0x17 ~= 0 and e:rnd() & 1 == 0 then
        state(e, 0xC, 0)
    end
    look_close(e, narrow)
end

local function stretch(e)
    local index = e:timer(3)
    if index < 0 then
        index = 0
    end
    return STRETCHES[index & 0x1F]
end

-- A burning zombie's walk runs on: `Type`'s `0x80` or `FLAG_QUICK_STEP` skips every
-- third frame.
local function burnt_clip(e)
    local burnt = e:kind() & TYPE_BURNT ~= 0 or has_flag(e, FLAG_QUICK_STEP)
    if burnt and e:move_cnt() % 3 == 2 then
        local seq = e:now_seq()
        if seq & SEQ_SOUND ~= 0 and seq >> 28 < 2 then
            e:sound(seq >> 28)
        end
        return true
    end
    return false
end

-- The burns' end of `FUN_00452ba0`: true when they kill it. What that death does
-- waits for `Em10_Die`.
local function burn_walk(e)
    if e:kind() & TYPE_BURNT == 0 and not has_flag(e, FLAG_BURNING) then
        return false
    end
    if e:flags() & 0x7 == 0 and e:flags() & 0x800 ~= 0 then
        local shift = e:rnd() & 3
        local side = e:rnd() >> shift & 1
        e:set_angle(e:angle() + 0x40 - side * 0x80)
        e:set_life(i16(e:life() - 1))
        if e:life() < 0 then
            e:routines(3, 10, 0, 0)
            return true
        end
    end
    if has_flag(e, FLAG_BURNING) then
        e:set_angle(e:angle() + 0x40 - (e:rnd() >> 1))
    end
    return false
end

-- `FUN_00452ba0`.
local function mv_walk(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(e:var(WALK), 0, 0xF)
        e:set_timer(3, (e:rnd() & 0xF) * 2)
        local extra = e:rnd()
        e:set_timer(0, extra + (stretch(e) & 0x7FFF))
        e:set_timer(2, (e:rnd() & 0x1F) + 0x1E)
        speed_at_random(e, PLD, 0x1F)
        e:set_timer(1, (e:rnd() >> 3) + 100)
    elseif e:routine(2) ~= 1 then
        return
    end
    if timer_out(e, 0) then
        local next = e:timer(3) + 1
        e:set_timer(3, next > 0x1F and 0 or next)
        local extra = e:rnd()
        e:set_timer(0, (extra & 0x1F) + (stretch(e) & 0x7FFF))
        local lurch = e:rnd()
        e:set_angle(e:angle() + (lurch & 1) * 0x20 - 0x10)
    end
    local rate = 8
    if e:distance() >= 0x1389 then
        rate = 8 - (stretch(e) >> 15) * 0x10
    end
    e:goto_dest(rate)
    if timer_out(e, 2) then
        e:set_timer(2, (e:rnd() & 0xF) + 0x1E)
        e:goto_dest(0x10)
        if voice_free(e) then
            if e:rnd() & 0x1F == 0 then
                moan(e, 10)
            elseif e:rnd() & 0x1F == 0 then
                moan(e, 0xB)
            end
        end
    end
    if e:distance() < 3000 then
        e:goto_dest(0x10)
    end
    e:spd_bank(PLD)
    e:joint_bank(PLD, 0x100)
    if burnt_clip(e) then
        e:goto_dest(8)
        e:skip_frame()
    end
    e:add_speed_xz()
    e:set_timer(1, e:timer(1) - 1)
    if e:timer(1) == 0 then
        e:set_timer(1, (e:rnd() >> 3) + 100)
        if e:rnd() & 3 == 0 and e:distance() < 5000 then
            state(e, 2, 0)
        end
    end
    if burn_walk(e) then
        return
    end
    if e:distance() < 2000 and e:turn_to_player(0x200) == 0 and player_free(e) then
        state(e, 2, 0)
    end
    if e:noise() & 4 ~= 0 and e:distance() < 0x9C4 and e:turn_to_player(0x52C) == 0 then
        state(e, 2, 0)
    end
end

-- `FUN_00453010`.
local function look_chase(e)
    local distance = e:distance()
    local wide = e:turn_to_player(0x400)
    local narrow = e:turn_to_player(0x200)
    if spit_at_partner(e) then
        state(e, 0xE, 0)
        return
    end
    if e:var(BITE) == 0
        and narrow == 0
        and distance < 2000
        and not e:same_floor()
        and e:pl_damage_cnt() == 0
    then
        state(e, 0xE, 0)
    end
    wall_check(e)
    if distance < 3000 and wide ~= 0 and e:noise() & 0x17 ~= 0 and e:rnd() & 1 == 0 then
        state(e, 0xC, 0)
    end
    look_close(e, narrow)
end

-- `FUN_00453240`.
local function mv_chase(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(e:var(WALK) + 4, 0, 0xF)
        e:set_timer(0, (e:rnd() & 0x3F) + 0xB4)
        if voice_free(e) then
            moan_either(e, 10, 0xB)
        end
        speed_at_random(e, PLD, 0x1F)
        local turn = e:rnd()
        e:set_timer(2, 0)
        e:set_timer(1, (turn & 0x1F) + 0x2D)
    elseif e:routine(2) ~= 1 then
        return
    end
    if e:timer(2) == 0 then
        e:goto_dest(0x10)
    else
        e:goto_dest(-0x10)
        e:set_timer(2, e:timer(2) - 1)
    end
    if e:sca_info() & 1 ~= 0 and timer_out(e, 1) then
        e:set_timer(1, (e:rnd() & 0x1F) + 0x2D)
        e:set_timer(2, (e:rnd() & 7) + 5)
    end
    e:spd_bank(PLD)
    e:joint_bank(PLD, 0x100)
    if burnt_clip(e) then
        e:skip_frame()
    end
    e:add_speed_xz()
    if e:kind() & TYPE_BURNT ~= 0 or has_flag(e, FLAG_BURNING) then
        if e:flags() & 7 == 0 and e:flags() & 0x800 ~= 0 then
            e:set_life(i16(e:life() - 1))
            if e:life() < 0 then
                e:routines(3, 10, 0, 0)
                return
            end
        end
        if has_flag(e, FLAG_BURNING) then
            e:set_angle(e:angle() + 0x40 - (e:rnd() >> 1))
        end
    end
    if timer_out(e, 0) or not player_free(e) then
        state(e, 1, 0)
    end
    if e:distance() < 0x1389 and e:turn_to_player(0x400) == 0 then
        return
    end
    if i16(e:timer(0)) < 0x78 then
        state(e, 1, 0)
    end
end

-- `FUN_00453510`'s arm coming off in the shove: `Rnd`'s roll, and the side.
local function maybe_lose_arm(e)
    if shifted(e) & 0xF ~= 0 then
        return
    end
    local behind = e:cdir_player()
    if e:rnd() & 2 ~= 0 then
        raise_flag(e, FLAG_LOST_ARM)
        e:pull_part(12, behind)
    else
        raise_flag(e, FLAG_LOST_OTHER_ARM)
        e:pull_part(9, behind)
    end
end

-- The shove's end for a crawler: which of two ends it gets goes by the player's
-- `Id`, and it bleeds on odd frames either way.
local function stomped(e)
    local count = e:move_cnt()
    local id = e:player_id()
    if (id & 1 == 0 and id < 0xC) or id == 0xC or id == 0xD then
        if count == 0x16 then
            e:hide_part(8)
            effect(e, 8, 0, 0x1000, angle(e), 0, 0, -600, 0)
            e:sound(2)
        end
    elseif count == 0x1B then
        raise_flag(e, FLAG_HEAD_LEFT)
        e:sound(8)
    end
    if count > 0x16 and count < 0x2A and count & 1 ~= 0 then
        blood(e, 0, 0x1000, e:rnd() * 4 - 0x200 + angle(e), 8, 0, 100, 0)
    end
end

-- `FUN_00453510`'s stumble ending on the floor.
local function falls_after_shove(e)
    e:routines(1, 5, 1, 0)
    local sce = e:rnd() & 0xF
    raise_kind(e, TYPE_ON_FLOOR)
    e:set_var(AT_SCE_NO, sce + 0x10)
    e:set_timer(2, 1)
    e:clip(2, 0x14, 10)
    e:set_flags((e:flags() & ~FLAG_THROWN) | 0x206)
    lying_be_flg(e)
    hold_damage(e)
    e:set_var(AT_OBJ_NO, 0)
    e:set_timer(3, 0)
    sound_either(e, 0xD, 0xC)
    e:set_timer(0, 1)
end

-- `FUN_00453510`: the zombie takes hold of the player and bites. The bite's frame
-- and damage go by whether it's crawling, and are worse from `v.tough`.
local function mv_grab(e, v)
    local low = e:kind() & 1 ~= 0
    local bite_frame = low and 1 or 0x10
    local damage
    if v.tough then
        damage = low and 10 or 0x1E
    else
        damage = low and 5 or 0x14
    end
    local clip = low and 0xE or 0xB
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:clip(clip, 0, 0xF)
        e:goto_player(0x800)
        e:set_bank(WEAPON)
        e:pl_set_base()
        local behind = e:cdir_player()
        e:pl_set_routine(MODE_HELD, (behind and 1 or 0) + (low and 2 or 0), 0, 0)
        e:pl_hold()
        hold_player(e)
        e:set_damage_cnt(0xF)
        raise_be_flg(e, BE_HOLDING)
        e:pl_or_be_flg(BE_HOLDING)
        if voice_free(e) then
            moan_either(e, 0xB, 0xA)
        end
    end
    local routine = e:routine(2)
    if routine == 1 then
        e:null_pos_set(0)
        joint_step(e, WEAPON, 0x100)
    elseif routine == 2 or routine == 3 then
        if routine == 2 then
            e:set_routine(2, 3)
            e:set_timer(0, 0x94)
            e:clip(clip + 1, 0, 0)
        end
        e:null_pos_set(0)
        e:joint_bank(WEAPON, 0x100)
        local shaken = e:gacha() and 5 or 0
        e:set_timer(0, e:timer(0) - (shaken + 2))
        if i16(e:timer(0)) < 0 then
            e:set_routine(2, 4)
            e:pl_set_routine_at(2, 4)
        end
        if e:move_cnt() == bite_frame then
            e:sound(3)
            local result = e:pl_life_down(damage, low)
            if result == 1 then
                e:set_routine(2, 4)
                e:pl_set_routine_at(2, 4)
                -- The player's `Timer2`: one more blood as they go.
                e:pl_set_timer(2, 1)
            elseif result == 2 then
                state(e, 6, 0)
                local behind = e:cdir_player()
                e:pl_set_routine(MODE_HELD_DIE, behind and 1 or 0, 0, 0)
            end
        end
    elseif routine == 4 or routine == 5 then
        if routine == 4 then
            e:set_routine(2, 5)
            e:set_timer(0, 0)
            e:set_timer(2, 0)
            e:clip(clip + 2, 0, 7)
        end
        if e:flags() & (FLAG_LOST_ARM | FLAG_LOST_OTHER_ARM) == 0
            and e:move_cnt() == 3
            and not low
        then
            maybe_lose_arm(e)
        end
        e:null_pos_set(0)
        joint_step(e, WEAPON, 0x200)
        if low then
            stomped(e)
        elseif e:move_cnt() == 7 then
            e:set_routine(2, 7)
        end
    elseif routine == 6 then
        drop_flag(e, FLAG_FACE_DOWN)
        e:routines(7, 0, 0, 0)
        e:set_life(-1)
        lying_be_flg(e)
        hold_damage(e)
    elseif routine == 7 or routine == 8 then
        if routine == 7 then
            e:spd_bank(WEAPON)
            e:spd_bank(WEAPON)
            e:set_routine(2, 8)
        end
        e:spd_bank(WEAPON)
        local ended = e:joint_bank(WEAPON, 0x200)
        e:set_speed_x(e:speed_x() - 0x1E)
        e:step(ended)
        e:add_speed_xz()
        if e:move_cnt() == 0x16 and shifted(e) & 0xF ~= 2 then
            falls_after_shove(e)
            return
        end
        local count = e:move_cnt()
        if count > 6 and count < 0x19 and e:at_em_no() ~= NO_BODY then
            e:knock(e:at_em_no())
        end
    elseif routine == 9 then
        state(e, 0, 0)
    end
end

-- `FUN_00453c50`.
local function look_crawl(e)
    if e:distance() < 0x514 and player_free(e) and e:same_floor() then
        reach_for_player(e, 1)
    end
end

-- `FUN_00453d40`.
local function mv_crawl(e)
    if e:routine(2) == 0 then
        e:clip(5, 0, 0xF)
        e:set_routine(2, 1)
        e:set_timer(0, (e:rnd() & 7) + 7)
        speed_at_random(e, WEAPON, 0xF)
    elseif e:routine(2) ~= 1 then
        return
    end
    if e:speed_x() > 0x14 then
        e:goto_dest(0x18)
        if timer_out(e, 0) then
            e:goto_dest(0x18)
            e:set_timer(0, (e:rnd() & 7) + 7)
        end
    end
    walk_on(e, WEAPON)
end

-- `FUN_004545c0`.
local function look_flat(e)
    if e:turn_to_player(0x200) == 0
        and e:distance() < 0x514
        and player_free(e)
        and e:in_sight()
        and e:same_floor()
    then
        state(e, 1, 0)
        hold_player(e)
    end
end

-- `FUN_00454650`.
local function mv_flat(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_move_no(0x17)
    end
end

local function floor_timer(e)
    if timer_out(e, 1) then
        e:set_routine(2, 6)
    end
end

local function floor_hold(e)
    if e:speed_x() == 0 then
        joint_step(e, WEAPON, 0x100)
    end
    floor_timer(e)
end

-- `FUN_00453e40`: on the floor, then back up.
local function mv_floor(e)
    if e:routine(2) == 0 then
        local side = e:rnd() & 1
        drop_flag(e, FLAG_FACE_DOWN)
        e:set_timer(2, side)
        if side ~= 0 then
            raise_flag(e, FLAG_FACE_DOWN)
        end
        raise_flag(e, FLAG_NECK_STILL | FLAG_DOWN)
        raise_kind(e, TYPE_ON_FLOOR)
        e:set_routine(2, 1)
        lying_be_flg(e)
        e:clip(side == 0 and 1 or 2, side * 5 + 10, 0xF)
        hold_damage(e)
        -- Down, it is no longer staggered: its next hit staggers it again.
        e:set_var(AT_OBJ_NO, 0)
        e:set_timer(3, 0)
        if voice_free(e) then
            moan_either(e, 0xD, 0xC)
        end
        e:set_timer(0, 0)
        e:set_speed_x(0xB)
    end
    local routine = e:routine(2)
    if routine == 1 then
        local x, y, z = e:foot_position()
        joint_step(e, WEAPON, 0x100)
        local ofs_y, at_w, at_h, w = e:body_box()
        if at_w > 0x14 then
            at_w = at_w - 10
            if w > 200 then
                w = w - 10
            end
            e:set_body_box(ofs_y, at_w, at_h, w)
        end
        if e:hokan() == 0 or e:timer(0) ~= 0 then
            e:foot_set(x, y, z)
        end
    elseif routine == 2 then
        local side = e:timer(2) & 1
        e:set_routine(2, 3)
        e:clip(side == 0 and 0x17 or 0x16, 0, 0xF)
        local hold = shifted(e) & 0xF
        local extra = e:rnd()
        e:set_timer(1, FLOOR_HOLDS[hold] + (extra & 0xF))
        free_damage(e)
        e:set_body_box(-0x15E, 0, 0x15E, 200)
        e:set_speed_x(e:rnd() & 3)
        if e:kind() & TYPE_BURNT ~= 0 then
            e:set_speed_x(0)
            e:set_timer(1, FLOOR_HOLDS[shifted(e) & 0xF] >> 1)
        end
        floor_hold(e)
    elseif routine == 3 then
        floor_hold(e)
    elseif routine == 4 or routine == 5 then
        if routine == 4 then
            local wait = e:rnd()
            e:set_routine(2, 5)
            e:set_timer(0, (wait & 0xF) + 3)
        end
        if timer_out(e, 0) then
            e:set_routine(2, 3)
        end
        floor_timer(e)
    elseif routine == 6 or routine == 7 then
        if routine == 6 then
            local side = e:timer(2) & 1
            raise_flag(e, FLAG_RISING)
            e:set_var(RISE_SIDE, has_flag(e, FLAG_FACE_DOWN) and 0 or 1)
            e:clip(side == 0 and 8 or 9, 0, 0xF)
            e:set_routine(2, 7)
            if e:timer(3) == 1 then
                blood(e, 0, 8000, angle(e), 0, 0, 0, 0)
            end
            if voice_free(e) and e:rnd() & 3 == 0 then
                moan(e, 0xB)
            end
            -- `+0x21b`'s 0x02: the head follows the player again as it rises.
            drop_flag(e, FLAG_NECK_STILL)
            e:set_speed_x(0xB)
        end
        local ofs_y, at_w, at_h, w = e:body_box()
        if at_w < 500 then
            at_w = at_w + 10
            if w < 500 then
                w = w + 10
            end
        end
        if ofs_y > -0x5DC then
            at_h = at_h + 10
            ofs_y = ofs_y - 10
        end
        e:set_body_box(ofs_y, at_w, at_h, w)
        if e:joint_bank(WEAPON, 0x100) then
            drop_flag(e, FLAG_RISING | FLAG_DOWN)
            e:set_routine(2, 8)
        end
        if e:move_cnt() == 0x37 then
            raise_be_flg(e, BE_LOW | BE_HIGH)
        end
    elseif routine == 8 then
        e:set_kind(e:kind() & ~TYPE_ON_FLOOR)
        drop_flag(e, FLAG_RISING | FLAG_DOWN)
        free_damage(e)
        state(e, 1, 0)
        raise_be_flg(e, BE_LOW | BE_HIGH)
        body_box_standing(e)
    end
end

local function lying_wait(e)
    if e:kind() & TYPE_HELD == 0 then
        e:set_routine(2, 2)
    end
end

-- `FUN_004543c0`.
local function mv_lying(e)
    local face_down = has_flag(e, FLAG_FACE_DOWN)
    local routine = e:routine(2)
    if routine == 0 then
        e:clip(face_down and 0x16 or 0x17, 0, 0)
        e:joint_bank(WEAPON, 0x100)
        e:set_routine(2, 1)
        hold_damage(e)
        e:set_neck_flags(e:neck_flags() | 1)
        lying_wait(e)
    elseif routine == 1 then
        lying_wait(e)
    elseif routine == 2 or routine == 3 then
        if routine == 2 then
            local count = e:rnd() & 0xF
            if face_down then
                e:clip(9, e:rnd() & 0xF, 0xF)
            else
                e:clip(8, count, 0xF)
            end
            e:set_routine(2, 3)
            if voice_free(e) then
                if e:rnd() & 1 ~= 0 then
                    moan(e, 0xC)
                elseif e:rnd() & 1 == 0 then
                    moan(e, 10)
                else
                    moan(e, 0xB)
                end
            end
        end
        joint_step(e, WEAPON, 0x100)
    elseif routine == 4 then
        free_damage(e)
        state(e, 1, 0)
        raise_be_flg(e, BE_LOW | BE_HIGH)
        e:set_neck_flags(e:neck_flags() & ~1)
    end
end

-- `FUN_00454520`: lets the dead player's body down, then eats it.
local function mv_drop_body(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:clip(0x18, 0, 0xF)
        e:set_routine(2, 1)
        e:pl_set_base()
        moan_either(e, 0xB, 0xA)
        hold_damage(e)
    elseif routine == 2 then
        raise_kind(e, TYPE_HELD)
        state(e, 8, 0)
        return
    elseif routine ~= 1 then
        return
    end
    e:null_pos_set(0)
    joint_step(e, WEAPON, 0x100)
end

-- `FUN_00454670`: eating, or over the player's body.
local function mv_eat(e)
    local routine = e:routine(2)
    if routine == 0 or routine == 1 then
        if routine == 0 then
            e:set_routine(2, 1)
            e:clip(EAT_CLIPS[e:rnd() & 7], 0, 7)
            hold_damage(e)
        end
        joint_planted(e, 0x200, true)
        if e:now_seq() & SEQ_BLOOD ~= 0 then
            blood(e, 0, 0x13D0, rnd_dir(e), 8, 200, 0, 0)
        end
        if e:kind() & TYPE_HELD == 0 then
            e:set_routine(2, 3)
        end
    elseif routine == 2 then
        local clip = e:rnd() & 7
        e:set_routine(2, 1)
        e:clip(EAT_CLIPS[clip], 3, 0)
    elseif routine == 3 or routine == 4 then
        if routine == 3 then
            raise_flag(e, FLAG_RISING)
            e:set_var(RISE_SIDE, 2)
            e:set_routine(2, 4)
            e:clip(0x15, 0, 0xF)
            free_damage(e)
            if voice_free(e) then
                if e:rnd() & 1 == 0 then
                    if e:rnd() & 1 == 0 then
                        if e:rnd() & 1 ~= 0 then
                            e:sound(10)
                        end
                    else
                        e:sound(0xB)
                    end
                end
                e:set_var(VOICE, 0x96)
            end
        end
        if e:joint_bank(WEAPON, 0x100) then
            drop_flag(e, FLAG_RISING)
            e:set_routine(2, 5)
        end
    elseif routine == 5 then
        state(e, 1, 0)
        drop_flag(e, FLAG_RISING)
        free_damage(e)
    elseif routine == 6 or routine == 7 then
        if routine == 6 then
            e:set_routine(2, 7)
            e:clip(0x15, e:rnd() & 7, 0xF)
        end
        e:goto_player(0x10)
        e:step(e:joint_bank(WEAPON, 0x100, true))
    elseif routine == 8 then
        e:set_routine(2, 0)
    end
end

-- `FUN_00454900`: thrown off the player.
local THROWN_CLIPS <const> = { [0] = 3, 3, 4, 0xD }

local function mv_thrown(e)
    local routine = e:routine(2)
    if routine == 0 then
        local side = e:rnd() & 1
        local back = e:timer(3)
        e:set_timer(2, side)
        local count = e:rnd() & 3
        local index = back * 2 + side
        if index < 0 then
            index = 0
        end
        e:clip(THROWN_CLIPS[index & 3], count, 0)
        if back ~= 0 and side ~= 0 then
            e:set_move_cnt((e:rnd() & 3) + 0x14)
        end
        e:set_timer(0, back << 11)
        e:set_routine(2, 1)
        local _, y = e:speed()
        e:set_speed(400, y, 0)
        e:set_flags((e:flags() & ~FLAG_RISING) | FLAG_THROWN)
        e:set_timer(1, 0)
        if voice_free(e) and e:rnd() & 1 ~= 0 then
            moan(e, 0xC)
        end
    elseif routine == 2 then
        drop_flag(e, FLAG_THROWN)
        state(e, 1, 0)
        return
    elseif routine ~= 1 then
        return
    end
    e:add_speed_xz_at(e:timer(0))
    e:set_speed_x(e:speed_x() - 0x1E)
    if e:speed_x() < 0 then
        e:set_timer(1, e:timer(1) + 1)
        e:set_speed_x(0)
    end
    if e:timer(1) == 1 and shifted(e) & 7 ~= 0 then
        e:routines(1, 5, 1, 0)
        e:rnd()
        drop_flag(e, FLAG_THROWN | FLAG_FACE_DOWN)
        local back = e:timer(3)
        e:set_timer(2, back)
        if back ~= 0 then
            raise_flag(e, FLAG_FACE_DOWN)
        end
        raise_flag(e, FLAG_NECK_STILL | FLAG_DOWN)
        raise_kind(e, TYPE_ON_FLOOR)
        lying_be_flg(e)
        local side = back < 0 and 0 or back & 1
        e:clip(side == 0 and 1 or 2, (back & 0xFF) * 5 + 10, 0xF)
        hold_damage(e)
        e:set_timer(3, 0)
        if voice_free(e) then
            moan_either(e, 0xD, 0xC)
        end
        e:set_timer(0, 0)
        return
    end
    joint_step(e, WEAPON, 0x100)
end

-- `FUN_00454b90`: at a wall a script chose, on the room's clips.
local WALL_CLIPS <const> = { [0] = 0, 2, 4 }

local function mv_wall(e)
    e:set_sce_free(1, e:sce_free(1) | 0x4000)
    local routine = e:routine(2)
    if routine == 0 or routine == 1 then
        if routine == 0 then
            local count = e:rnd() & 3
            e:set_routine(2, 1)
            e:clip(e:var(WALK), count, 0xF)
            e:set_timer(2, 0)
            if e:sce_free(0) & 0x4000 ~= 0 then
                e:set_timer(2, (e:rnd() & 1) + 1)
            end
        end
        e:joint_bank(PLD, 0x100)
        local turn = e:turn_to(e:sce_free(0) & 0xFFF, 0x20)
        e:set_angle(e:angle() + turn)
        if turn == 0 then
            e:set_routine(2, 2)
        end
    elseif routine == 2 or routine == 3 then
        local table_clip = WALL_CLIPS[e:timer(2) % 3]
        if routine == 2 then
            local count = e:rnd() & 7
            local side = e:rnd() & 1
            e:set_routine(2, 3)
            e:clip(side + table_clip, count, 0xF)
            e:set_timer(0, (e:rnd() & 0x7F) + 0x3C)
        end
        if e:joint_bank(ROOM, 0x100) then
            local count = e:rnd() & 7
            local side = e:rnd() & 1
            e:clip(side + table_clip, count, 0xF)
        end
        local free = e:sce_free(1) & 0x7FFF
        if e:now_seq() & SEQ_BLOOD ~= 0 then
            free = free | 0x8000
        end
        e:set_sce_free(1, free)
        if timer_out(e, 0) then
            e:set_routine(2, 4)
        end
    elseif routine == 4 then
        state(e, 1, 0)
    end
end

-- `FUN_00454d60`: falling to the floor toward the player.
local function mv_fall(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:clip(10, 0, 0xF)
        e:set_routine(2, 1)
        e:goto_dest(0x80)
        raise_kind(e, TYPE_ON_FLOOR)
        e:set_flags((e:flags() & ~FLAG_FACE_DOWN) | FLAG_DOWN)
        lying_be_flg(e)
        e:set_timer(0, 0)
        hold_damage(e)
        e:set_timer(3, 0)
        if voice_free(e) then
            moan_either(e, 0xB, 10)
        end
    elseif routine == 2 then
        e:routines(1, 5, 2, 0)
        e:rnd()
        e:set_timer(2, 0)
        return
    elseif routine ~= 1 then
        return
    end
    e:set_timer(0, e:timer(0) + 1)
    if e:timer(0) & 1 == 0 then
        e:set_speed_x(e:speed_x() // 4)
    else
        e:spd_bank(WEAPON)
    end
    joint_step(e, WEAPON, 0x100)
    e:add_speed_xz()
    if e:move_cnt() < 0x12 then
        e:goto_dest(0x10)
    elseif e:turn_to_player(0x100) == 0
        and e:distance() < 0x708
        and player_free(e)
        and e:same_floor()
    then
        state(e, 1, 0)
        e:set_kind((e:kind() & 0xFFC1) | 1)
        hold_player(e)
    end
end

-- `FUN_00454f30`: the reach comes in the lunge's last step, `Routine_2`
-- (`+0x6`) of 3, not on a clip.
local function look_lunge(e)
    if e:routine(2) == 3
        and e:distance() < 0x514
        and player_free(e)
        and e:move_cnt() < 0xB
        and e:same_floor()
        and e:player_id() ~= 0xF
    then
        reach_for_player(e, 3)
    end
end

-- `FUN_00455010`.
local function mv_lunge(e)
    local routine = e:routine(2)
    if routine == 0 or routine == 1 then
        if routine == 0 then
            e:clip(0x19, 0, 7)
            e:set_routine(2, 1)
            e:goto_player(0x20)
            e:set_timer(0, e:turn_to_player(0xBE))
            if voice_free(e) then
                moan(e, 10)
            end
            e:set_speed_x(0xB)
        end
        if e:turn_to_player(0x140) == 0 then
            e:set_timer(0, 0)
            e:set_routine(2, 2)
        end
        e:set_angle(e:angle() + e:timer(0))
        if e:joint_bank(WEAPON, 0x200) then
            e:set_routine(2, 2)
        end
    elseif routine == 2 or routine == 3 then
        if routine == 2 then
            e:clip(0x1B, 0, 7)
            e:set_routine(2, 3)
        end
        e:goto_player(0x20)
        e:spd_bank(WEAPON)
        if e:joint_bank(WEAPON, 0x200) then
            state(e, 1, 0)
        end
        e:add_speed_xz()
        if e:move_cnt() == 0x19 then
            state(e, 1, 0)
        end
    end
end

-- `FUN_00455170`: begun again, as a script's `Type` asks.
local function mv_rise_again(e, v)
    e:routines(1, 0, 0, 0)
    e:set_damage_cnt(0)
    e:set_sce_flg(0)
    e:set_life(LIFE[e:rnd() & 0xF])
    e:set_var(WALK, e:rnd() & 1 == 0 and 0 or 2)
    e:set_flags(0)
    e:rnd()
    body_box_standing(e)
    e:set_be_flg(BE_LOW | BE_HIGH | 1)
    e:clip(1, 0, 0)
    e:joint_bank(WEAPON, 0x100)
    if e:kind() & 0x3F == 6 then
        state(e, 1, 0)
    end
    if v.skins and e:model_type() & 0xC0 == 0 then
        e:set_skin(e:rnd() % 3 + 1)
    end
    e:set_hidden_parts(1 << 0xF | 1 << 0x10)
    e:reset_part_colours()
end

-- `FUN_00455620`: `Esp_call`'s spit isn't ported.
local function mv_vomit(e)
    if e:routine(2) == 0 then
        e:clip(0x11, 0, 0xF)
        e:set_routine(2, 1)
        e:set_speed_x(0xB)
    elseif e:routine(2) ~= 1 then
        return
    end
    if e:move_cnt() == 10 then
        e:sound(5)
    end
    if e:joint_bank(WEAPON, 0x100) then
        state(e, 0, 0)
        e:set_var(BITE, 0x3C)
    end
end

-- `FUN_004556b0`: the room's clip, at a script's word.
local function mv_room_clip(e)
    local routine = e:routine(2)
    if routine == 0 then
        e:clip(0, 0, 0)
        e:joint_bank(ROOM, 0x100)
        if e:sce_free(0) & 1 ~= 0 then
            e:set_routine(2, 1)
            e:clip(0, 0, 0)
        end
    elseif routine == 1 then
        e:spd_bank(ROOM)
        if e:joint_bank(ROOM, 0x100) then
            e:set_kind(e:kind() & ~TYPE_HELD)
            free_damage(e)
            e:set_timer(3, 0)
            e:set_timer(2, 0)
            e:routines(1, 5, 6, 0)
            e:set_be_flg(e:be_flg() & ~BE_NO_SCA)
        end
        e:add_speed_xz()
    end
end

local CRAWLING_KINDS <const> = {
    [1] = true, [3] = true, [5] = true, [7] = true, [9] = true, [0xB] = true, [0xD] = true,
}
local LOOKS <const> = { [0] = look_stand, look_walk, look_chase, [0xC] = look_lunge }
local MOVES <const> = {
    [0] = mv_stand,
    mv_walk,
    mv_chase,
    mv_grab,
    [5] = mv_floor,
    [6] = mv_drop_body,
    [7] = mv_lying,
    [8] = mv_eat,
    [9] = mv_thrown,
    [0xA] = mv_wall,
    [0xB] = mv_fall,
    [0xC] = mv_lunge,
    [0xD] = mv_rise_again,
    [0xE] = mv_vomit,
    [0xF] = mv_room_clip,
}

-- `Em10_Move`: what `Type` decides, then the state's motion.
local function mv(e, v)
    local which = e:routine(1)
    if CRAWLING_KINDS[e:kind() & 0x3F] then
        if which == 0 then
            look_crawl(e)
        elseif which == 2 then
            look_flat(e)
        end
        which = e:routine(1)
        if which == 0 then
            mv_crawl(e)
        elseif which == 1 then
            mv_grab(e, v)
        elseif which == 2 then
            mv_flat(e)
        end
        return
    end
    local look = LOOKS[which]
    if look then
        look(e)
    end
    local move = MOVES[e:routine(1)]
    if move then
        move(e, v)
    end
end

local ROUTINES <const> = {
    [0] = init,
    mv,
    em10_damage,
    em10_die,
    -- `Spl_Scenario`, on the model's own bank.
    function(e)
        e:scenario()
    end,
    [7] = em10_corpse,
    [8] = em10_scene,
}

local function run(e, v)
    local voice = e:var(VOICE)
    if voice ~= 0 then
        e:set_var(VOICE, voice - 1)
    end
    local bite = e:var(BITE)
    if bite ~= 0 then
        e:set_var(BITE, bite - 1)
    end
    local routine = ROUTINES[e:routine(0)]
    if routine then
        routine(e, v)
    end
end

return { run = run }
