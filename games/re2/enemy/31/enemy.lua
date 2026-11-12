-- G2, William Birkin's second form, `Em_31` (entity id 0x31).
--
-- `Em_31` runs `Routine_0` from `funcs_49032D` (`0x52a410`): 0 is the init
-- (`FUN_004903f0`) and 1 the move (`FUN_00490890`); the hit (2,
-- `FUN_00494440`) and the death (3, `FUN_00495bc0`) aren't ported yet. The
-- move runs by `Type & 0x3f`: 0 and 3 run `funcs_4908BB` (`0x52a480`) before
-- the clip and `funcs_4908D2` (`0x52a4a8`) after it, by `Routine_1`. Of
-- those, routine 7 (`0x4930b0`) is the change that room 509's scene starts:
-- it waits on clip 19 until the room's script raises `Sce_free0`'s bit 0,
-- then changes and climbs down to the floor for the fight: routine 1 walks
-- and picks what to do next, 2 swipes, 3 dodges, 4 grabs, carries and
-- throws, and 5 steps back.
--
-- Everything that lasts past a tick lives on the entity: `e:var` and
-- `e:set_var` are the fields of the original's work that the engine doesn't
-- read.

local CNT_218 <const> = 0 -- `+0x218`, which counts down from 15 while `FLAGS`' 2.
local CNT_219 <const> = 1 -- `+0x219`, how many times `+0x218` runs out before `FLAGS`' 2 clears.
local CNT_21A <const> = 2 -- `+0x21a`.
local CNT_21B <const> = 3 -- `+0x21b`: the ticks to the next point of life regained.
local FLAGS <const> = 4 -- `+0x21c`: 0x20 throbs parts 1 and 2.
local WAVE <const> = 5 -- `+0x21e`: the throb's place, mod 64.
local HURT <const> = 6 -- `+0x220` and `+0x221`: the eye's swell (`FUN_004966a0`).
local HURT_221 <const> = 7
local B227 <const> = 8 -- `+0x227`.
-- `+0x224`, a word: the points of life it may still regain, 2000 from the
-- init.
local REGEN <const> = 9
local CNT_228 <const> = 10 -- `+0x228`: the attacks routine 1 turns into routine 3 (`FUN_00491330`) in a row.
local CNT_22E <const> = 11 -- `+0x22e`: the ticks before a flame's hit bleeds again (`FUN_00494940`).
-- The weights of the morphs (`pSa_dat_head`'s words from 0xc): part 1's
-- one, part 2's two and part 4's one.
local W1 <const> = 12
local W2A <const> = 13
local W2B <const> = 14
local W4 <const> = 15
-- The `Size_y` of parts 3, 6, 7 and 8, which grow during the change, and
-- which of them still grow (their `Be_flg`'s 0x400), as a bit per part.
local SIZE <const> = { [3] = 16, [6] = 17, [7] = 18, [8] = 19 }
local GROWING <const> = 20
-- The eye's swell's step (`+0x222`) and its count of throbs (`+0x21f`).
local EYE_STEP <const> = 21
local EYE_COUNT <const> = 22
-- `+0x229`: a magnum's spurt (`FUN_00495470`), its count of drops (`+0x22a`)
-- and its turn (`+0x22c`).
local SPURT <const> = 23
local SPURT_N <const> = 25
local SPURT_DIR <const> = 26
-- `Atd[0]`'s `At_w`, `At_d`, `W` and `D`, which routine 6 grows.
local BOX <const> = 24

-- The init's life: normal, with `fg_system`'s `0x20`, and for the scene types.
local LIFE <const> = 700
local LIFE_EASY <const> = 600
local LIFE_SCENE <const> = 5000

-- The life it regains up to, by `Type`'s 1.
local REGAIN_TO <const> = { [0] = 500, 0 }

-- `BYTE_0052a430`: the throb's swell of part 1, by `WAVE`, as signed bytes.
local THROB <const> = {
    [0] = 0x0C, 0x17, 0x23, 0x2B, 0x33, 0x3C, 0x42, 0x47, 0x4B, 0x4F, 0x52, 0x56, 0x59, 0x5C, 0x5E, 0x60,
    0x61, 0x62, 0x63, 0x64, 0x63, 0x62, 0x61, 0x60, 0x5E, 0x5C, 0x59, 0x57, 0x55, 0x53, 0x50, 0x4E,
    0x4B, 0x48, 0x44, 0x41, 0x3E, 0x3A, 0x36, 0x31, 0x2C, 0x27, 0x22, 0x1D, 0x18, 0x13, 0x0D, 0x07,
    0x00, -0x06, -0x0F, -0x0B, -0x1A, -0x1E, -0x20, -0x22, -0x23, -0x23, -0x22, -0x1F, -0x19, -0x14, -0x0A, 0x00,
}

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function count_down(e, index)
    local value = e:var(index)
    if value ~= 0 then
        e:set_var(index, value - 1)
    end
end

-- `Sa_parts_allmv` on each morphing part, from its weights.
local function morph_1(e)
    e:morph(1, e:var(W1))
end

local function morph_2(e)
    e:morph(2, e:var(W2A), e:var(W2B))
end

local function morph_4(e)
    e:morph(4, e:var(W4))
end

-- `FUN_004903f0`. What it does to the parts' drawing (their colours, the
-- sizes but parts 3, 6, 7 and 8's `Size_y`) and its shadow aren't modelled.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_sce_flg(0)
    e:set_damage_cnt(0)
    e:set_life(e:easy() and LIFE_EASY or LIFE)
    e:set_d_life(0)
    e:set_neck_flags(0)
    for index = CNT_218, SPURT_DIR do
        e:set_var(index, 0)
    end
    e:set_var(BOX, 900)
    for part, index in pairs(SIZE) do
        e:set_var(index, 0x200)
        e:set_var(GROWING, e:var(GROWING) | (1 << part))
    end
    e:set_var(CNT_21B, 5)
    e:set_var(REGEN, 0x7D0)
    -- `Atd[0]`: `W` and `D` 900, `Ofs_y` -1500, `At_w` and `At_d` 900 and
    -- `At_h` 1500.
    e:set_body_box(-1500, 900, 1500, 900)
    e:set_body_ofs_x(0)
    e:set_be_flg(e:be_flg() | 0xC000004)
    e:clip(0, 0, 0)
    e:joint(0x100)
    morph_1(e)
    morph_2(e)
    e:set_routine(1, 7)
    e:clip(0x13, 0, 0)
    e:joint(0x100)
    e:set_var(W2A, 0x1000)
    morph_2(e)
    e:set_var(W4, 0x1000)
    morph_4(e)
    local low = e:kind() & 0xFFF
    if low == 2 then
        e:set_timer(0, 0x3C)
        e:set_life(LIFE_SCENE)
        e:set_be_flg(e:be_flg() | 2)
        e:set_var(GROWING, e:var(GROWING) & ~0x1C0)
        e:set_var(B227, (e:rnd() & 1) << 4)
        e:routines(1, 0, 3, 0)
    elseif low == 3 then
        e:set_life(LIFE_SCENE)
        e:set_be_flg(e:be_flg() | 10)
        e:set_var(GROWING, e:var(GROWING) & ~0x1C0)
        e:routines(1, 9, 0, 0)
    end
end

-- Grow a part's `Size_y` by `step` while it still grows; it stops past
-- 0xfff. Whether it grew.
local function grow(e, part, step)
    if e:var(GROWING) & (1 << part) == 0 then
        return false
    end
    local size = i16(e:var(SIZE[part]) + step)
    e:set_var(SIZE[part], size)
    if size > 0xFFF then
        e:set_var(GROWING, e:var(GROWING) & ~(1 << part))
    end
    return true
end

-- `Spd_get`, taken on the clip's second frame as well when it's on its first.
local function spd_first(e, hokan)
    e:spd()
    if e:move_cnt() == 0 then
        e:set_move_cnt(1)
        e:spd()
        if hokan then
            e:set_hokan(hokan)
        end
        e:set_move_cnt(e:move_cnt() - 1)
    end
end

local function add_w2(e, a, b)
    e:set_var(W2A, i16(e:var(W2A) + a))
    e:set_var(W2B, i16(e:var(W2B) + b))
end

-- `Routine_2` 2: clip 19 plays while part 2's two morphs sway.
local function change_swell(e)
    local count = e:move_cnt()
    if count > 5 and count < 0x23 then
        add_w2(e, (10 - count) * 8, count * 8 - 0x50)
    end
    if count > 0x23 and count < 0x3C then
        add_w2(e, count * 4 - 0x8C, (0x23 - count) * 4)
    end
    if count > 0x3C and count < 0x4B then
        add_w2(e, (0x3C - count) * 8, count * 8 - 0x1E0)
    end
    if count > 0x4B then
        add_w2(e, count * 2 - 0x96, (0x4B - count) * 2)
    end
    local odd = count & 1
    add_w2(e, odd * -0x78 + 0x3C, odd * 0x78 - 0x3C)
    morph_2(e)
    e:spd()
    if e:joint(0x100) then
        e:clip(0x14, 0, 0xF)
        e:set_routine(2, 3)
        e:set_timer(3, 0)
        e:set_timer(0, 0)
    end
    e:add_speed_xz()
end

-- `Routine_2` 3: clip 0x14, part 2's morph runs to its second target and
-- back, and part 3 grows; clip 0x15 once it has.
local function change_grow(e)
    spd_first(e, 0xF)
    e:joint(0x100)
    e:add_speed_xz()
    local timer = i16(e:timer(0))
    local flags = e:timer(3) & 0xFF
    if timer > 0x1E and flags & 1 == 0 then
        add_w2(e, (0x1E - timer) * 4, (timer - 0x1E) * 4)
        if e:var(W2B) > 0x1000 then
            e:set_var(W2B, 0x1000)
            e:set_var(W2A, 0)
            flags = flags | 1
        end
    end
    if flags & 1 ~= 0 and flags & 4 == 0 then
        e:set_var(W2B, i16(e:var(W2B) - 0x80))
        if e:var(W2B) < 0 then
            e:set_var(W2B, 0)
            flags = flags | 4
        end
    end
    e:set_timer(3, flags)
    local odd = e:move_cnt() & 1
    add_w2(e, odd * 0xF0 - 0x78, odd * -0xF0 + 0x78)
    morph_2(e)
    if timer == 0x29 then
        -- `blood_censor` is taken as off.
        e:effect(0, 3, 0x1FA0, i16(e:angle()), 3, 0, 0, 0)
    end
    if timer > 0x2D and grow(e, 3, (timer - 0x2D) * 2) and e:var(GROWING) & (1 << 3) == 0 then
        e:clip(0x15, 0, 0xF)
        e:set_routine(2, 4)
        e:set_timer(3, 0)
    end
    e:set_timer(0, timer + 1)
end

-- `Routine_2` 4: clip 0x15, then clip 0x16.
local function change_rise(e)
    spd_first(e)
    if e:joint(0x100) then
        e:clip(0x16, 0, 0xF)
        e:set_routine(2, 5)
        e:set_timer(3, 0)
        e:set_timer(0, 0)
        e:set_neck_flags(e:neck_flags() & 0xFD)
    end
    e:add_speed_xz()
end

-- `Routine_2` 5: part 4's morph runs back to its base. What it does to
-- `DWORD_0068968c` (the eye's light) isn't modelled.
local function change_eye(e)
    local was = e:timer(0)
    local timer = i16(was + 1)
    e:set_timer(0, timer)
    if was == 0x2D then
        e:set_timer(3, 0)
        e:set_routine(2, 6)
        e:set_neck_flags(e:neck_flags() | 2)
    end
    if timer > 0 and e:timer(3) & 1 == 0 then
        e:set_var(W4, i16(e:var(W4) - timer * 0x10))
        if e:var(W4) < 0 then
            e:set_var(W4, 0)
            e:set_timer(3, e:timer(3) | 1)
        end
    end
    morph_4(e)
end

-- `Routine_2` 6: clip 0x16, parts 8, 6 and 7 grow in turn with a spurt of
-- blood on each other frame.
local function change_sprout(e)
    local count = e:move_cnt()
    for _, part in ipairs({ { 8, 0xF }, { 6, 0x14 }, { 7, 0x28 } }) do
        local index, from = part[1], part[2]
        if count > from and grow(e, index, (count - from) * 0x20) and count & 1 ~= 0 then
            e:effect(0, 0x14, e:rnd() * 8 + 0x800, 0, index, -100, 100, 0)
        end
    end
    e:spd()
    if e:joint(0x100) then
        e:set_sce_free(0, e:sce_free(0) | 2)
        e:set_routine(2, 7)
        e:set_timer(0, 0)
        e:clip(0, 0, 0xFF)
    end
    e:add_speed_xz()
end

-- `Routine_2` 7: the scene's wait for `Sce_free0`'s bit 2, after which it
-- steps off on clip 15.
local function change_wait(e)
    local timer = e:timer(0)
    if i16(timer) > 0x1E then
        e:joint(0x10)
    end
    e:set_timer(0, timer + 1)
    if e:sce_free(0) & 4 ~= 0 then
        e:set_routine(2, 8)
        e:set_speed(200, 0, 0)
        e:set_timer(0, 0)
        e:clip(0xF, 0, 0)
        e:joint(0x100)
        e:sound(9)
    end
end

-- `Routine_2` 8 and 9: the fall, down to the floor.
local function fall(e)
    local timer = i16(e:timer(0))
    local _, y = e:pos()
    y = y + timer * 0x1E
    e:set_timer(0, timer + 1)
    e:set_pos_y(y)
    return y
end

-- Routine 7 (`0x4930b0`): the change, which room 509's scene starts. It
-- holds clip 19's first frame (its `Routine_2` 0 and 1 don't play it) until
-- the room's script raises `Sce_free0`'s bit 0, then changes (2 to 6), waits
-- for the script again (7), falls to the floor (8 and 9) and starts the
-- fight (10). What it does to the parts' drawing (their sizes but the four
-- in `SIZE`, and their `Be_flg`) isn't modelled.
local function change(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0x13, 0, 0)
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_routine(2, 1)
        e:set_timer(3, 0)
        e:set_timer(0, 0x96)
        e:set_timer(2, 0)
        sub = 1
    end
    if sub == 1 then
        if e:sce_free(0) & 1 ~= 0 then
            e:set_routine(2, 2)
        end
    elseif sub == 2 then
        change_swell(e)
    elseif sub == 3 then
        change_grow(e)
    elseif sub == 4 then
        change_rise(e)
    elseif sub == 5 then
        change_eye(e)
    elseif sub == 6 then
        change_sprout(e)
    elseif sub == 7 then
        change_wait(e)
    elseif sub == 8 then
        if fall(e) > -2000 then
            e:set_routine(2, 9)
        end
        e:add_speed_xz()
    elseif sub == 9 then
        if e:joint(0x100) then
            e:set_routine(2, 10)
            e:set_neck_flags(e:neck_flags() & 0xFD)
        end
        if fall(e) > -400 then
            e:set_pos_y(0)
        end
        if e:move_cnt() < 0xC then
            e:add_speed_xz()
        end
    elseif sub == 10 then
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 10)
        e:set_neck_flags(e:neck_flags() & 0xFD)
    end
    -- The throb of parts 0 to 2's sizes isn't modelled, only its place.
    e:set_timer(2, (e:timer(2) + 1) & 0x3F)
end

-- `Goto00_ck` toward the player: 0 once facing them within `rate`.
local function player_ck(e, rate)
    local x, _, z = e:player_pos()
    return e:goto_ck(x, z, rate)
end

-- Out of points to regain, or low on life, it goes to routine 6 with `Type`
-- 1, which isn't ported.
local function spent(e)
    if e:var(REGEN) == 0 or e:life() < 100 then
        e:set_kind(1)
        e:routines(1, 6, 0, 0)
        return true
    end
    return false
end

-- Routine 1 before the clip (`0x4909a0`): the fight's choice, by the
-- player's distance (`L_pl`) and whether it faces them. Close, it steps back
-- (5); from 0x9c4 to 0xd48 it swipes (2) once `CNT_21A` has run out; closer
-- it grabs (4); and a running player in reach may make it dodge (8, not
-- ported). With the player near death, two choices in three become 3's.
local function fight_ck(e)
    local far = e:distance()
    local grab = player_ck(e, 0x4C8)
    local near = player_ck(e, 0x270)
    local ahead = player_ck(e, 0x400)
    if spent(e) then
        return
    end
    if ahead == 0 and far < 0x5DC then
        e:routines(1, 5, 0, 0)
        e:set_timer(0, 0x28)
        return
    end
    local wait = e:var(CNT_21A)
    local hurt = e:pl_damage_cnt() ~= 0
    local chosen = false
    if wait == 0 and not hurt and ahead == 0 and far < 0xD48 and far > 0x9C4 then
        e:routines(1, 2, 0, 0)
        chosen = true
    end
    if far < 0x9C4 and grab == 0 and not hurt and wait < 10 then
        e:routines(1, 4, 0, 0)
        chosen = true
    end
    if wait ~= 0 and not hurt and near == 0 and far < 0xD48 and e:noise() & 4 ~= 0
        and e:rnd() & 3 == 0
    then
        e:routines(1, 8, 0, 0)
        chosen = true
    end
    local life = e:player_life()
    if life ~= 0 and life < 0x3C and chosen then
        local count = (e:var(CNT_228) + 1) & 0xFF
        e:set_var(CNT_228, count)
        if count < 3 then
            e:routines(1, 3, 0, 0)
            return
        end
        e:set_var(CNT_228, 0)
    end
end

-- Routine 1 after the clip (`0x490bd0`): the walk to `Dest`, which
-- `Root_ck` sets, turning faster while it doesn't face it (slower with
-- `Type`'s 1, on clip 2). Its stride is halved while the clip blends in, and
-- longer while it's out of the cut's view. Every other step's end groans.
local function walk(e)
    local sub = e:routine(2)
    local weak = e:kind() & 1 ~= 0
    if sub == 0 then
        e:clip(weak and 2 or 1, 0, 0x1F)
        e:set_routine(2, 1)
        e:set_timer(0, 0)
    elseif sub ~= 1 then
        return
    end
    local x, z = e:dest()
    local slow, fast = 0x20, 0x30
    if weak then
        slow, fast = 8, 0x10
    end
    e:goto_point(x, z, slow)
    if e:goto_ck(x, z, 0x338) ~= 0 then
        e:goto_point(x, z, fast)
    end
    e:spd()
    if e:hokan() > 0xF then
        -- `SAR`: toward minus infinity.
        e:set_speed_x(e:speed_x() // 2)
    end
    if not e:in_cut_view() then
        e:set_speed_x(e:speed_x() + 0x14)
    end
    if e:joint(0x80) then
        e:set_timer(0, e:timer(0) + 1)
        if weak then
            if e:rnd() & 1 ~= 0 then
                e:sound(9)
            end
        elseif e:timer(0) & 1 == 0 then
            e:sound(0xD)
        end
    end
    e:add_speed_xz()
end

-- The claws, part 6 (`FUN_00490060`), and the hand, part 11
-- (`FUN_004968b0`).
local CLAWS <const> = 6
local HAND <const> = 11

-- `L_pos_ckXZM`: whether a point, `x` and `z` as a part placed it, is within
-- `within` of the player, across the floor.
local function near(e, x, z, within)
    if x == nil then
        return false
    end
    local tx, _, tz = e:player_pos()
    return e:hypot(x - tx, z - tz) < within
end

-- `near` for a point on `part`, on this tick's pose and facing.
local function reaches(e, part, x, y, z, within)
    local px, _, pz = e:part_point(part, x, y, z)
    return near(e, px, pz, within)
end

-- `FUN_004943b0`: `FLAGS`' 0x40, with what it does to the parts' drawing.
local function mark(e)
    e:set_var(FLAGS, e:var(FLAGS) | 0x40)
end

-- The player's hit: `life` taken, a stagger (`Routine_1` 2 from the front, 3
-- from behind) on G2's clips, and the blood for `blood` ticks.
local function strike(e, life, back, sound, blood)
    e:pl_life_down(life, true)
    e:pl_or_neck_flags(0x12)
    e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
    e:pl_set_routine(2, 2 + back, 0, 0)
    e:pl_set_sub1()
    e:set_var(FLAGS, e:var(FLAGS) & 0xFFFE)
    e:set_timer(2, blood)
    e:sound(sound)
    mark(e)
end

-- The claws strike a wall (`Sca_ck_hit`, 600, mask `0x8000`, 1000 along
-- them): four sparks, with a `Rnd` between each, and a clang at most each
-- eight ticks.
local function wall(e, x, y, z)
    if x == nil or not e:sca_struck(x, y, z, 600, 0x8000) then
        return
    end
    local dir = i16(e:angle())
    local ox, oy, oz = 0, 1000, 0
    e:effect(1, 6, 0x1FA0, dir, CLAWS, ox, oy, oz)
    ox = ox + 0x40 - (e:rnd() & 0x7F)
    e:effect(1, 6, 0x1FA0, dir, CLAWS, ox, oy, oz)
    oy = oy + 0x80 - e:rnd()
    e:effect(1, 6, 0x1FA0, dir, CLAWS, ox, oy, oz)
    oz = oz + 0x40 - (e:rnd() & 0x7F)
    e:effect(1, 6, 0x1FA0, dir, CLAWS, ox, oy, oz)
    if e:timer(1) == 0 then
        e:sound(7)
        e:set_timer(1, 7)
    end
end

-- The clang's count (its shake of the screen, `fg_status`' 8, isn't
-- ported) and the blood of a hit: a spurt at the claws and one at the
-- player, three `Rnd`s and two a tick.
local function after_hit(e)
    if e:timer(1) ~= 0 then
        e:set_timer(1, e:timer(1) - 1)
    end
    if e:timer(2) == 0 then
        return
    end
    e:set_timer(2, e:timer(2) - 1)
    local ox = 0x100 - e:rnd() * 2
    local oy = 0x4E8 - e:rnd() * 2
    local oz = 0x100 - e:rnd() * 2
    e:effect(0, 0, 0x1000 + e:rnd() * 4, i16(e:angle()), CLAWS, ox, oy, oz)
    ox = 0x100 - e:rnd() * 2
    oz = 0x100 - e:rnd() * 2
    local size = 0x1000 + e:rnd() * 4
    -- At the player's root; the original's at their part 0.
    local x, y, z = e:player_root_point(ox, 0, oz)
    if x ~= nil then
        e:call_effect(0, 0, size, 0, x, y, z)
    end
end

-- Routine 2 (`0x490d50`): the swipe, clip 6 then clip 7, which a hurt
-- player starts in on. Early on it breaks off if the player gets away or out
-- of its arc; on the clips' hit frames (`0x30000`) it turns to them and hits
-- them with its claws (15 life), or strikes a wall.
local function swipe(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(6, 0, 0xF)
        e:set_routine(2, 1)
        e:spd()
        e:set_timer(3, 0)
        e:set_timer(2, 0)
        e:set_timer(1, 0)
        if e:player_life() > 0x3C then
            e:set_move_cnt(0xF)
            e:spd()
        end
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:hokan() ~= 0 then
        local x, y, z = e:speed()
        e:set_speed(x // 2, y, z // 2)
    end
    if e:joint(0x100) then
        e:set_var(FLAGS, e:var(FLAGS) & 0xFF7F)
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 0x14)
    end
    e:add_speed_xz()
    local count = e:move_cnt()
    if count > 5 and count < 0x13 and e:timer(3) == 0 then
        e:goto_player(8)
        if e:distance() > 0xED8 or player_ck(e, 0x590) ~= 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(CNT_21A, 10)
        end
    end
    if e:move_cnt() == 0x28 and e:distance() < 3000 and e:timer(3) == 0 then
        e:clip(7, 0, 0xF)
        e:set_timer(3, 1)
        if e:pl_damage_cnt() & 0x80 ~= 0 and e:player_life() > 0 then
            e:pl_set_damage_cnt(0)
        end
    end
    if e:now_seq() & 0x30000 ~= 0 then
        e:set_var(FLAGS, e:var(FLAGS) | 0x80)
        e:goto_player(0x20)
        if e:timer(3) ~= 0 and player_ck(e, 0x400) ~= 0 then
            e:set_angle((e:angle() + 0x80) & 0xFFFF)
        end
        if (reaches(e, CLAWS, 0, -300, 0, 0x4B0) or reaches(e, CLAWS, 0, 400, 0, 0x4B0))
            and e:pl_damage_cnt() == 0
        then
            local dir = (e:direction_to_player() - 0x400) & 0xFFFF
            local front = ((e:player_angle() - dir + 0x400) & 0xFFF) < 0x800
            if e:timer(3) ~= 0 then
                front = not front
            end
            -- 20 in hard mode or uncensored.
            strike(e, 0xF, front and 1 or 0, 3, 6)
        end
        wall(e, e:part_point(CLAWS, 0, 1000, 0))
    end
    after_hit(e)
end

-- Routine 3 before the clip (`0x491330`): while it dodges, a player in reach
-- running across it starts a grab (4), and from frame 0x1a it walks again
-- once they're out of its arc or 5000 away.
local function dodge_ck(e)
    if spent(e) then
        return
    end
    local far = e:distance()
    local ahead = player_ck(e, 0x400)
    local count = e:move_cnt()
    if ahead == 0 then
        if e:pl_damage_cnt() == 0 and count > 10 and far < 0x5DC then
            local run = i16(e:pl_speed())
            local same = e:cdir_player()
            if (run > 0 and not same) or (run < 0 and same) then
                e:routines(1, 4, 0, 0)
                e:set_var(CNT_228, 0)
            end
        end
        if far < 0x1389 then
            return
        end
    end
    if count > 0x19 then
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 10)
    end
end

-- Routine 3 after the clip (`0x491420`): the dodge, clip 0x17, then a
-- swipe, or one time in four clip 6 to its frame 0x12 (`Routine_2` 3), or
-- the walk if the player is far.
local function dodge(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(0x17, 0, 0xF)
        e:set_routine(2, 1)
        e:set_timer(0, 0x3C)
    elseif sub == 3 then
        e:spd()
        e:joint(0x100)
        e:add_speed_xz()
        if e:move_cnt() == 0x12 then
            e:routines(1, 1, 0, 0)
            e:set_var(CNT_21A, 0xF)
        end
        return
    elseif sub ~= 1 then
        return
    end
    if e:hokan() ~= 0 then
        e:goto_player(0x20)
    end
    e:spd()
    if e:joint(0x100) then
        if e:distance() < 0xD48 then
            if e:rnd() & 3 == 0 then
                e:clip(6, 0, 0xF)
                e:set_routine(2, 3)
            else
                e:routines(1, 2, 0, 0)
            end
            return
        end
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 0xF)
    end
    e:add_speed_xz()
end

-- `Routine_2` 1 of routine 4: clip 5. On its hit frames its hand catches a
-- player within 800 and carries them, until the frames end. Then, with the
-- player above 20 life, it throws (5), or one time in a few swipes; below,
-- the kill (2 to 4); and having caught nothing, walks.
local function grab_reach(e)
    e:spd()
    if e:joint(0x100) then
        e:pl_set_damage_cnt(e:pl_damage_cnt() & 0x7F)
        e:set_routine(2, 2)
        if e:player_life() > 0x14 then
            e:set_routine(2, 5)
            e:set_timer(2, 0)
            local shift = e:rnd() & 3
            if (e:rnd() >> shift) & 3 == 0 then
                e:routines(1, 2, 0, 0)
            end
        end
        if e:timer(3) == 0 then
            e:routines(1, 1, 0, 0)
            e:set_var(CNT_21A, 0xF)
        end
    end
    e:add_speed_xz()
    -- The hand's place before the turn below.
    local hx, _, hz = e:part_point(HAND, 0, 0, 0)
    local hold = e:now_seq() & 0x30000 ~= 0
    if hold and e:timer(3) == 0 then
        e:goto_player(0x80)
        if near(e, hx, hz, 800) and e:pl_damage_cnt() == 0 then
            e:pl_set_routine(5, 0, 0, 0)
            e:pl_or_neck_flags(0x12)
            e:pl_set_damage_cnt(0x80)
            e:pl_hold()
            e:pl_set_sub1()
            e:set_var(FLAGS, e:var(FLAGS) & 0xFFFE)
            e:set_timer(3, 1)
            e:sound(8)
        end
    end
    if hold and e:timer(3) ~= 0 then
        if hx ~= nil then
            e:set_player_xz(hx, hz)
        end
        if e:move_cnt() & 3 == 0 then
            -- At the player's root; the original's at their parts 4 and 7.
            local px, py, pz = e:player_root_point(0, 0, 0)
            if px ~= nil then
                e:call_effect(3, 0xB, 0x17D0, i16(e:angle()), px, py, pz)
                e:call_effect(3, 0xB, 0x17D0, i16(e:angle()), px, py, pz)
            end
        end
    end
    if not hold and e:timer(3) ~= 0 then
        if e:timer(2) == 0 then
            e:pl_set_routine(1, 0, 0, 0)
            e:pl_set_damage_cnt(e:pl_damage_cnt() & 0x7F)
            e:set_be_flg(e:be_flg() & ~0x1000)
            e:pl_clear_be_flg(0x1000)
        end
        e:goto_player(0x40)
    end
end

-- `Routine_2` 6 of routine 4: the throw, clip 9 from frame 1 and clip 11
-- from its frame 10 unless it has hit; on the hit frames its claws hit the
-- player within 800 (15 life) or strike a wall.
local function throw(e)
    e:spd()
    if e:joint(0x200) then
        local hit = e:timer(3) ~= 0
        e:set_var(FLAGS, e:var(FLAGS) & 0xFF7F)
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 0x1E)
        if hit then
            e:set_var(CNT_21A, (e:rnd() & 7) + 10)
        end
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
    end
    e:add_speed_xz()
    -- The claws' points before the turn below.
    local ax, _, az = e:part_point(CLAWS, 0, 0, 0)
    local bx, _, bz = e:part_point(CLAWS, 0, 500, 0)
    local wx, wy, wz = e:part_point(CLAWS, 0, 1000, 0)
    if e:now_seq() & 0x30000 ~= 0 then
        e:goto_player(0x20)
        if (near(e, ax, az, 800) or near(e, bx, bz, 800)) and e:pl_damage_cnt() == 0 then
            -- 25 in hard mode.
            strike(e, 0xF, 0, 4, 5)
            e:set_timer(3, 1)
        end
        wall(e, wx, wy, wz)
    end
    after_hit(e)
    if e:move_cnt() == 10 and e:timer(3) == 0 then
        e:clip(0xB, 0, 7)
        e:set_timer(3, 1)
    end
end

-- `Routine_2` 2 to 4 of routine 4: the kill of a player it caught near
-- death. It turns to them, takes 500 life and their death in its claws
-- (`em_die_table[0x31]`, player routine 6) on clip 9, with blood at the claws
-- each fourth frame from frame 8, then holds clip 0x17. `fg_system`'s
-- `0x100` isn't ported.
local function kill(e, sub)
    if sub == 2 then
        e:clip(9, 0, 7)
        e:set_routine(2, 3)
        e:goto_player(0x800)
        e:pl_hold()
        e:pl_set_base()
        e:pl_set_routine(6, 0, 0, 0)
        e:pl_set_sub1()
        e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:set_be_flg(e:be_flg() | 0x1000)
        e:pl_or_be_flg(0x1000)
        e:set_player_angle((e:angle() + 0x800) & 0xFFFF)
        e:pl_life_down(500, false)
        sub = 3
    end
    if sub == 3 then
        e:null_pos_set(0)
        if e:joint(0x200) then
            e:clip(0x17, 0, 0xF)
            e:set_routine(2, 4)
        end
        local count = e:move_cnt()
        if count == 6 then
            e:sound(5)
            mark(e)
        end
        if count >= 7 and count & 3 == 0 then
            local ox = 0x100 - e:rnd() * 2
            local oy = 0x148 - e:rnd()
            local oz = 0x100 - e:rnd() * 2
            e:effect(0, 0, 0x1000 + e:rnd() * 4, i16(e:angle()), CLAWS, ox, oy, oz)
        end
    elseif sub == 4 then
        if e:joint(0x100) then
            e:set_hokan(0xF)
        end
    end
end

-- Routine 4 (`0x491540`): the grab (`Routine_2` 0 and 1), the kill (2 to 4)
-- and the throw (5 and 6). `FLAGS`' 0x80 is up throughout.
local function grab(e)
    local sub = e:routine(2)
    e:set_var(FLAGS, e:var(FLAGS) | 0x80)
    if sub == 0 then
        e:clip(5, 0, 0xF)
        e:spd()
        e:set_routine(2, 1)
        e:goto_player(0x20)
        e:set_timer(3, 0)
        e:set_timer(2, 0)
        sub = 1
    end
    if sub == 1 then
        grab_reach(e)
    elseif sub >= 2 and sub <= 4 then
        kill(e, sub)
    elseif sub == 5 then
        e:clip(9, 1, 7)
        e:set_routine(2, 6)
        e:set_timer(3, 0)
        e:spd()
        e:goto_player(0x20)
        e:set_timer(2, 0)
        e:set_timer(1, 0)
        throw(e)
    elseif sub == 6 then
        throw(e)
    end
end

-- Routine 5 before the clip (`0x491ec0`): while it steps back, it walks
-- again once the player is more than 0x9c4 away, or grabs a running player
-- in reach near the step's end.
local function back_ck(e)
    local far = e:distance()
    player_ck(e, 0x5F4)
    player_ck(e, 0x338)
    local ahead = player_ck(e, 0x400)
    if spent(e) then
        return
    end
    if ahead == 0 and far > 0x9C4 then
        e:routines(1, 1, 0, 0)
    end
    if e:pl_damage_cnt() == 0 and i16(e:timer(0)) < 10 and ahead == 0 and far < 0x9C4
        and e:noise() & 4 ~= 0
    then
        e:routines(1, 4, 0, 0)
    end
end

-- Routine 5 after the clip (`0x491fd0`): the walk played backwards, facing
-- the player, for 0x28 ticks; then a swipe or a grab if they're ahead and
-- near, or the walk.
local function back_off(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(e:kind() & 1 ~= 0 and 2 or 1, 0, 0x1F)
        e:set_routine(2, 1)
        e:set_timer(0, 0x28)
    elseif sub ~= 1 then
        return
    end
    e:goto_player(0x20)
    e:spd_reversed()
    if e:hokan() > 0x14 then
        e:set_speed_x(e:speed_x() // 2)
    end
    e:joint(0x80, true)
    e:add_speed_xz()
    local was = e:timer(0)
    e:set_timer(0, was - 1)
    if was == 0 then
        e:routines(1, 1, 0, 0)
        if player_ck(e, 0x400) == 0 and e:distance() < 3000 then
            e:routines(1, 2, 0, 0)
            if e:rnd() & 1 ~= 0 or e:player_life() < 1 then
                e:routines(1, 4, 0, 0)
            end
        end
    end
end

-- Routine 6 (`0x492390`): out of points to regain or below 100 life, it
-- mutates (clip 0xe), its life held at 9000. Part 2's morph shudders and
-- swells, part 1's swells to 0x1000 (and starts `FLAGS`' 0x20, the throb),
-- its box grows to 0x44c, and blood spurts from parts 2 and 3. Then it raises
-- `FLAGS`' 0x100, which lets the next killing shot kill it, and walks, with
-- the life it had, at most 100.
local function mutate(e)
    local sub = e:routine(2)
    e:set_var(FLAGS, e:var(FLAGS) | 0x80)
    e:set_var(CNT_219, 0)
    local dir = i16(e:angle())
    if sub == 0 then
        e:clip(0xE, 0, 0xF)
        e:set_routine(2, 1)
        e:effect(0, 0, 0x13D0, dir, 3, 400, 100, 0)
        e:set_neck_flags(e:neck_flags() | 2)
        e:set_timer(1, e:life())
        e:sound(0xC)
    elseif sub ~= 1 then
        e:set_life(9000)
        return
    end
    local count = e:move_cnt()
    if count == 0x1E or count == 0x23 or count == 0x32 then
        e:effect(0, 0, 0x13D0, dir, 3, 300, 0, 0)
    end
    if count > 5 and count < 0x19 then
        local vy = -0x2C - e:rnd() * 2
        local vz = 0x80 - e:rnd()
        e:effect(0, 0, 0x17B8, i16(e:rnd() << 4), 2, 400, vy, vz)
        vy = -0x2C - e:rnd() * 2
        vz = 0x80 - e:rnd()
        e:effect(0, 1, 0x17B8, i16(e:rnd() << 4), 2, 400, vy, vz)
    end
    local box = e:var(BOX)
    if box < 0x44C then
        box = box + 5
        e:set_var(BOX, box)
        e:set_body_box(-1500, box, 1500, box)
    end
    local odd = count & 1
    e:set_var(W2B, i16(e:var(W2B) - odd * 0x80))
    if count > 0x3C and count < 0x5C then
        add_w2(e, 0x80, 0x40)
    end
    morph_2(e)
    e:set_var(W2B, i16(e:var(W2B) + odd * 0x80))
    if count > 0x14 then
        local w = i16(e:var(W1) + 0x40)
        if w > 0x1000 then
            w = 0x1000
            e:set_var(FLAGS, e:var(FLAGS) | 0x20)
        end
        e:set_var(W1, w)
        morph_1(e)
    end
    if count > 0x14 and count < 0x3E and count & 3 == 0 then
        local vx = 0x16C - (e:rnd() & 0x7F)
        local vz = 0x16C - (e:rnd() & 0x7F)
        e:effect(0, 0, 0x13D0, dir, 2, vx, 200, vz)
        vz = -(e:rnd() & 0x7F) - 0xEC
        e:effect(0, 0, 0x13D0, dir, 2, vx, 200, vz)
    end
    e:spd()
    if not e:joint(0x100) then
        e:add_speed_xz()
        e:set_life(9000)
        return
    end
    e:set_neck_flags(e:neck_flags() & 0xFD)
    e:routines(1, 1, 0, 0)
    e:set_var(CNT_21A, 10)
    e:set_var(FLAGS, (e:var(FLAGS) & 0xFF7F) | 0x100)
    local life = i16(e:timer(1))
    e:set_life(life > 100 and 100 or life)
end

-- `funcs_4908BB` (before the clip) and `funcs_4908D2` (after it), by
-- `Routine_1`.
local BEFORE <const> = {
    [1] = fight_ck,
    [3] = dodge_ck,
    [5] = back_ck,
}
local AFTER <const> = {
    [1] = walk,
    [2] = swipe,
    [3] = dodge,
    [4] = grab,
    [5] = back_off,
    [6] = mutate,
    [7] = change,
}

-- Routine 1 before the clip with `Type` 1 (`0x490b30`): close and facing it
-- steps back (5); within 3000 and in its arc, once `CNT_21A` has run out, it
-- strikes (2).
local function weak_ck(e)
    local far = e:distance()
    player_ck(e, 0x5F4)
    local near = player_ck(e, 0x270)
    local ahead = player_ck(e, 0x400)
    if ahead == 0 and far < 0x5DC then
        e:routines(1, 5, 0, 0)
    end
    if e:var(CNT_21A) == 0 and e:pl_damage_cnt() == 0 and near == 0 and far < 3000 then
        e:routines(1, 2, 0, 0)
    end
end

-- Routine 5 before the clip with `Type` 1 (`FUN_00491f90`): it walks again
-- once the player is more than 0x9c4 away and in its arc.
local function weak_back_ck(e)
    local far = e:distance()
    if player_ck(e, 0x400) == 0 and far > 0x9C4 then
        e:routines(1, 1, 0, 0)
    end
end

-- Routines 2 and 4 with `Type` 1 (`FUN_004920e0`): a strike, clip 0xc or
-- 0xd, turning slowly to the player. On its hit frames the claws (part 6 as
-- its last draw left it) hit a player within 800 for twice 5 life (10 in
-- hard mode), a plain hit, or their death; with blood for two ticks.
local function weak_strike(e)
    local sub = e:routine(2)
    if sub == 0 then
        local clip = 0xC + (e:rnd() & 1)
        e:set_routine(2, 1)
        e:set_timer(2, 0)
        e:clip(clip, 0, 0xF)
    elseif sub ~= 1 then
        return
    end
    e:goto_player(8)
    e:spd()
    if e:joint(0x100) then
        e:set_var(FLAGS, e:var(FLAGS) & 0xFF7F)
        e:routines(1, 1, 0, 0)
        e:set_var(CNT_21A, 0x14)
    end
    e:add_speed_xz()
    if e:now_seq() & 0x30000 ~= 0 then
        e:set_var(FLAGS, e:var(FLAGS) | 0x80)
        e:goto_player(0x10)
        local ax, _, az = e:drawn_part_point(CLAWS, 0, 0, 0)
        local bx, _, bz = e:drawn_part_point(CLAWS, 0, 500, 0)
        if (near(e, ax, az, 800) or near(e, bx, bz, 800)) and e:pl_damage_cnt() == 0 then
            e:pl_life_down(5, false)
            local killed = e:pl_life_down(5, false) == 2
            e:pl_set_routine(killed and 3 or 2, 0, 0, 0)
            e:pl_set_damage_cnt(e:pl_damage_cnt() | 0x80)
            e:pl_set_sub1()
            e:set_timer(2, 2)
            e:sound(6)
            mark(e)
        end
    end
    if e:timer(2) ~= 0 then
        e:set_timer(2, e:timer(2) - 1)
        local ox = 0x100 - e:rnd() * 2
        local oy = 0x274 - e:rnd()
        local oz = 0x100 - e:rnd() * 2
        e:effect(0, 0, 0xC18 + e:rnd() * 4, i16(e:angle()), CLAWS, ox, oy, oz)
        ox = 0x100 - e:rnd() * 2
        oz = 0x100 - e:rnd() * 2
        local size = 0x1000 + e:rnd() * 4
        local x, y, z = e:player_root_point(ox, 0, oz)
        if x ~= nil then
            e:call_effect(0, 0, size, 0, x, y, z)
        end
    end
end

-- `funcs_4908EB` and `funcs_490902`: the same with `Type` 1, after routine
-- 6.
local WEAK_BEFORE <const> = {
    [1] = weak_ck,
    [5] = weak_back_ck,
}
local WEAK_AFTER <const> = {
    [1] = walk,
    [2] = weak_strike,
    [4] = weak_strike,
    [5] = back_off,
    [6] = mutate,
}

-- `FUN_00490890`.
local function move(e)
    local kind = e:kind() & 0x3F
    local tables
    if kind == 0 or kind == 3 then
        tables = { BEFORE, AFTER }
    elseif kind == 1 then
        tables = { WEAK_BEFORE, WEAK_AFTER }
    else
        return
    end
    local before = tables[1][e:routine(1)]
    if before then
        before(e)
    end
    local after = tables[2][e:routine(1)]
    if after then
        after(e)
    end
end

-- `Em_31`'s throb of parts 1 and 2 while `FLAGS`' 0x20, with two `Rnd`s
-- for the eye (`DWORD_0068968c`) on 16 ticks of each 32.
local function throb(e)
    local wave = e:var(WAVE)
    e:set_var(W1, i16((0x100 - THROB[wave]) * 0x10))
    morph_1(e)
    local odd = wave & 1
    e:set_var(W2A, i16(e:var(W2A) + odd * 0x100 - 0x80))
    e:set_var(W2B, i16(e:var(W2B) - odd * 0x100 + 0x80))
    morph_2(e)
    wave = (wave + 1) & 0xFF
    e:set_var(WAVE, wave & 0x3F)
    if wave & 0x10 ~= 0 then
        e:rnd()
        e:rnd()
    end
end

-- `FUN_00495470`: a magnum's spurt of blood out of the far side, eight drops
-- a tick in a line away from the player, for three ticks, as G1's.
local function spurt(e)
    local state = e:var(SPURT)
    if state == 1 then
        e:set_var(SPURT, 2)
        e:set_var(SPURT_N, 0)
        e:set_var(SPURT_DIR, e:direction_to_player())
    elseif state ~= 2 then
        return
    end
    local dir = e:var(SPURT_DIR)
    local count = e:var(SPURT_N)
    local ox, oy, oz = e:rot_apply(0, dir, 0, i16(count * -100), 200, 0)
    local sx, sz
    if e:damage_no() % 3 == 0 then
        sx, _, sz = e:rot_apply(0, dir, 0, -100, 500, 0)
    else
        sx, _, sz = e:rot_apply(0, dir, 0, -100, -500, 0)
    end
    local bx, _, bz = e:pos()
    local _, part_y = e:drawn_part_point(2, 0, 0, 0)
    local y = i16((part_y or 0) + oy)
    local function drop(x, z, step)
        local size = (0x65C - step * count) - e:rnd()
        e:call_effect(0, 1, size & 0xFFFF, i16(dir), i16(x), y, i16(z))
        count = count + 1
    end
    local function wide_drop(x_at, z_at, step)
        local x = bx - e:rnd() + x_at + 0x80 + ox
        local z = bz - e:rnd() + z_at + 0x80 + oz
        local size = (0x65C - step * count) - e:rnd()
        local row = e:rnd() & 1 == 0 and 1 or 0
        e:call_effect(0, row, size & 0xFFFF, i16(dir), i16(x), y, i16(z))
        if count == 1 then
            e:call_effect(0, 2, 0x17D0, i16(dir + 0x800), i16(x), y, i16(z))
            e:call_effect(0, 2, 0xC18, i16(dir + 0x800), i16(x), y, i16(z))
        end
        count = count + 1
    end
    drop(bx + ox, bz + oz, 0x24)
    wide_drop(sx, sz, 0x24)
    drop(sx * 2 + bx + ox, sz * 2 + bz + oz, 0x24)
    wide_drop(sx * 3, sz * 3, 0x24)
    drop(sx * 4 + bx + ox, sz * 4 + bz + oz, 0x24)
    wide_drop(sx * 5, sz * 5, 0x24)
    drop(sx * 6 + bx + ox, sz * 6 + bz + oz, 0x24)
    local last = count
    wide_drop(sx * 7, sz * 7, 0x1E)
    e:set_var(SPURT_N, count)
    if last > 0x10 then
        e:set_var(SPURT, 0)
    end
end

-- `FUN_004966a0`: the eye (part 4's morph) swells after a hit (`HURT` 1)
-- to 0x1000, with two `Rnd`s as it gets there, then shrinks back and stops;
-- once G2 is dead (`HURT` 2) it swells to 0xc00 and throbs, 500 a tick for
-- four ticks each way, with two `Rnd`s each tick. What it does to
-- `DWORD_0068968c` (the eye's light) isn't modelled.
local function eye(e)
    local state = e:var(HURT)
    if state == 0 then
        return
    end
    local sub = e:var(HURT_221)
    local weight = e:var(W4)
    local step = e:var(EYE_STEP)
    local count = e:var(EYE_COUNT)
    if state == 1 then
        if sub == 0 then
            weight = i16(weight + 0x400)
            if weight > 0x1000 then
                weight = 0x1000
                sub = 1
                step = 0
                e:rnd()
                e:rnd()
            end
        elseif sub == 1 then
            weight = i16(weight - step)
            step = i16(step + 100)
            if weight < 0 then
                weight = 0
                e:set_var(HURT, 0)
                sub = 0
            end
        end
    else
        if sub == 0 then
            weight = i16(weight + 0x400)
            if weight >= 0xC01 then
                weight = 0xC00
                step = 0
                sub = 1
                count = 3
            end
        elseif sub == 1 or sub == 2 then
            local way = sub == 1 and -1 or 1
            weight = i16(weight + way * 500)
            step = i16(step - way * 100)
            local was = count
            count = (count - 1) & 0xFF
            if was == 0 then
                sub = 3 - sub
                count = 3
            end
        end
        e:rnd()
        e:rnd()
    end
    e:set_var(W4, weight)
    morph_4(e)
    e:set_var(HURT_221, sub)
    e:set_var(EYE_STEP, step)
    e:set_var(EYE_COUNT, count)
end

-- The blood of a hit, at part 1 as its last draw left it, turned toward
-- the player: two drops 200 out and two further by its speed, higher for a
-- shot at the head (`Damage_no`'s third), and `Snd_se_enem` 2. With `Type`
-- 2 (room 500, not ported) it bleeds at parts 5 and 6 instead.
local function hit_blood(e, weapon)
    if weapon == 0x10 or weapon == 0xC then
        return
    end
    local dir = e:direction_to_player()
    local px, py, pz = e:drawn_part_point(1, 0, 0, 0)
    if px == nil then
        px, py, pz = e:pos()
    end
    local reach = i16(e:speed_x() + 0x4B)
    local ax, ay, bx, by = 200, -200, reach * 4, 0
    if e:damage_no() % 3 == 0 then
        ax, ay, bx, by = 0x96, 700, reach * 2, 700
    end
    local x, y, z = e:rot_apply(0, dir, 0, ax, ay, 0)
    x, y, z = i16(x + px), i16(y + py), i16(z + pz)
    local fx, fy, fz = e:rot_apply(0, dir, 0, i16(bx), by, 0)
    fx, fy, fz = i16(fx + px), i16(fy + py), i16(fz + pz)
    dir = i16(dir)
    e:call_effect(0, 1, e:rnd() * 8 + 0x17D0, dir, x, y, z)
    y = i16(y + 0x40 - (e:rnd() & 0x3F))
    e:call_effect(0, 0, e:rnd() * 8 + 0x17D0, dir, x, y, z)
    e:call_effect(0, 0, e:rnd() * 8 + 0x17D0, dir, fx, fy, fz)
    -- The original moves the first pair's height here and passes the second.
    e:rnd()
    e:call_effect(0, 0, e:rnd() * 8 + 0x17D0, dir, fx, fy, fz)
    if weapon ~= 1 then
        e:sound(2)
    end
end

-- `FUN_00494940`, a flame's (and 0xa's) blood at parts 2, 0, 5, 15, 13,
-- 10 and 3 (`+0x1a0` to `+0x24c` into the parts, which are 0xac long): six
-- of kind 5, row 4 and one of kind 4, row 0xf, each its own size from a
-- `Rnd`, turned as G2 faces.
local FLAME_BLOOD <const> = {
    { 2, 5, 4, 0x3AF8 },
    { 0, 5, 4, 0x2770 },
    { 5, 5, 4, 0x1000 },
    { 15, 5, 4, 0x1FA0 },
    { 13, 5, 4, 0x17D0 },
    { 10, 5, 4, 0x1000 },
    { 3, 4, 0xF, 0x17D0 },
}

local function flame_blood(e)
    local dir = i16(e:angle())
    for _, at in ipairs(FLAME_BLOOD) do
        e:effect(at[2], at[3], e:rnd() * 4 + at[4], dir, at[1], 0, 0, 0)
    end
end

-- `FUN_00495ad0`: the flinch, backwards or forwards by whether it faces the
-- way the player does (clip 3 or 4), to its frame 15; then it walks.
local function flinch(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:clip(e:cdir_player() and 3 or 4, 0, 3)
        e:set_routine(2, 1)
        e:sound(9)
        e:set_var(FLAGS, e:var(FLAGS) | 0x200)
        e:set_var(CNT_228, (e:var(CNT_228) + 1) & 0xFF)
    elseif sub ~= 1 then
        return
    end
    e:spd()
    if e:joint(0x400) then
        e:routines(1, 1, 0, 0)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
    end
    e:add_speed_xz()
    if e:move_cnt() == 0xF then
        e:routines(1, 1, 0, 0)
        e:set_var(FLAGS, e:var(FLAGS) & 0xFDFF)
        e:set_damage_cnt(e:damage_cnt() & 0x7F)
    end
end

-- `0x52a537`: what each weapon adds to `CNT_219`, which flinches it at 12.
local FLINCH_ADD <const> = {
    [0] = 3, 1, 4, 4, 3, 10, 20, 6, 10, 8, 20, 8, 3, 2, 20, 1, 0, 20, 2,
    0, -- 0x13 (`0x52a54a`): the Beretta adds nothing.
}

-- Routine 0 2 (`FUN_00494440`): a hit. Below 200 life it raises `FLAGS`' 4
-- and starts `CNT_219` at 0x32. Each hit adds the weapon's share to
-- `CNT_219`, which `CNT_218` takes down by one every 15 ticks; at 12 or more
-- it flinches, and otherwise it goes on with what it was doing on this tick,
-- as it does during an attack (`FLAGS`' 0x80). A hit during a flinch goes on
-- with it. A flame (0x10) bleeds at most once in 0x2d ticks (`+0x22e`).
-- The other weapons' effects (9, 0xa, 0xb, 0xe and 0x11) aren't ported.
local function hurt(e)
    if e:routine(2) ~= 0 then
        flinch(e)
        return
    end
    local weapon = e:routine(1)
    hit_blood(e, weapon)
    e:set_var(HURT, 1)
    e:set_var(HURT_221, 0)
    if weapon == 5 or weapon == 6 then
        e:set_var(SPURT, 1)
    end
    if weapon == 0x10 and e:var(CNT_22E) == 0 then
        flame_blood(e)
        e:set_var(CNT_22E, 0x2D)
    end
    local flags = e:var(FLAGS)
    if flags & 4 == 0 and e:life() < 200 then
        e:set_var(CNT_219, 0x32)
        flags = flags | 4
        e:set_var(FLAGS, flags)
    end
    if flags & 0x200 ~= 0 then
        e:restore_routine()
        flinch(e)
        return
    end
    if flags & 1 == 0 then
        e:set_var(CNT_219, (e:var(CNT_219) + (FLINCH_ADD[weapon] or 0)) & 0xFF)
        if flags & 2 == 0 then
            e:set_var(CNT_218, 0xF)
            e:set_var(FLAGS, flags | 2)
        end
    end
    if e:var(FLAGS) & 0x80 ~= 0 or e:var(CNT_219) < 0xC then
        e:restore_routine()
        move(e)
        return
    end
    e:set_var(CNT_219, 0)
    e:set_var(CNT_218, 0)
    e:set_var(FLAGS, e:var(FLAGS) & 0xFFFD)
    flinch(e)
end

-- Blood from part 3 (and part 2) as the death plays: `vy` up, a random
-- turn and row, and `size`.
local function death_blood(e, part, vx, vy, vz, size)
    local dir = i16(e:rnd() << 4)
    local row = e:rnd() & 1
    e:effect(0, row, size, dir, part, vx, vy, vz)
end

-- Routine 0 3 (`FUN_00495bc0`): a shot that takes its life below zero.
-- Before routine 6 has raised `FLAGS`' 0x100 it only leaves it 10 life and
-- goes on. After, it bleeds, raises its dead flag, falls (clip 10) with blood
-- from part 3 and, from frame 0x51, from part 2 as well, and lies. Its
-- shadow's fade isn't modelled.
local function death(e)
    local sub = e:routine(2)
    if sub == 0 then
        local weapon = e:routine(1)
        local dir = e:direction_to_player() & 0xFF
        local px, py, pz = e:drawn_part_point(1, 0, 0, 0)
        if px == nil then
            px, py, pz = e:pos()
        end
        local x, y, z = e:rot_apply(0, dir, 0, 100, e:damage_no() % 3 == 0 and 700 or 0, 0)
        x, y, z = i16(x + px), i16(y + py), i16(z + pz)
        if weapon ~= 0x10 then
            e:call_effect(0, 1, e:rnd() * 8 + 0x17D0, dir, x, y, z)
            y = i16(y + 0x40 - (e:rnd() & 0x3F))
            e:call_effect(0, 0, e:rnd() * 8 + 0x17D0, dir, x, y, z)
        end
        if weapon == 5 or weapon == 6 then
            e:set_var(SPURT, 1)
        end
        -- A flame's blood here isn't held back by `+0x22e`.
        if weapon == 0x10 then
            flame_blood(e)
        end
        if e:var(FLAGS) & 0x100 == 0 then
            e:set_life(10)
            e:restore_routine()
            return
        end
        e:set_damage_cnt(e:damage_cnt() | 0x80)
        e:clip(10, 0, 3)
        e:set_routine(2, 1)
        e:set_var(FLAGS, e:var(FLAGS) | 0x11)
        e:set_var(HURT, 2)
        e:set_var(HURT_221, 0)
        e:set_died()
        e:set_neck_flags(e:neck_flags() | 2)
        sub = 1
    end
    if sub == 1 then
        local count = e:move_cnt()
        if count < 0xF then
            local vx = 0x40 - (e:rnd() >> 1)
            local vz = 0x40 - (e:rnd() >> 1)
            death_blood(e, 3, vx, 0, vz, 0xC00)
        end
        if count > 0x50 and count < 0x6D then
            if count == 0x51 then
                e:sound(9)
            end
            local vx = 0x40 - (e:rnd() >> 1)
            local vz = 0x40 - (e:rnd() >> 1)
            death_blood(e, 3, vx, -200, vz, 0xC00)
            vx = 0x100 - e:rnd() * 2
            local vy = i16(0xD4 - e:rnd() * 4)
            vz = 0x100 - e:rnd() * 2
            death_blood(e, 2, vx, vy, vz, 0x17B8)
        end
        e:spd()
        if e:joint(0x400) then
            e:set_var(FLAGS, e:var(FLAGS) & 0xFFDF)
            e:set_routine(2, 2)
            e:set_be_flg(e:be_flg() | 2)
            e:set_var(HURT, 0)
            e:set_var(HURT_221, 0)
            e:set_timer(0, 0xD2)
        end
        e:add_speed_xz()
    elseif sub == 2 then
        if e:timer(0) ~= 0 then
            e:set_timer(0, e:timer(0) - 1)
        end
    end
end

function update(e)
    local flags = e:var(FLAGS)
    if flags & 2 ~= 0 then
        local count = e:var(CNT_218)
        e:set_var(CNT_218, (count - 1) & 0xFF)
        if count == 0 then
            local times = e:var(CNT_219)
            if times ~= 0 then
                times = times - 1
                e:set_var(CNT_219, times)
            end
            e:set_var(CNT_218, 0xF)
            if times == 0 then
                e:set_var(FLAGS, flags & 0xFFFD)
            end
        end
    end
    -- It regains a point of life every eight ticks, up to 500, while it has
    -- points to regain (`REGEN`, which runs down while its life is 500 or
    -- less).
    local life = e:life()
    if life < 0x1F5 and e:var(REGEN) ~= 0 then
        e:set_var(REGEN, e:var(REGEN) - 1)
    end
    if not e:easy() then
        local was = e:var(CNT_21B)
        e:set_var(CNT_21B, (was - 1) & 0xFF)
        if was == 0 and e:var(REGEN) ~= 0 then
            e:set_var(CNT_21B, 7)
            if life > 0 and life < REGAIN_TO[e:kind() & 1] then
                e:set_life(life + 1)
            end
        end
    end
    count_down(e, CNT_21A)
    count_down(e, CNT_22E)
    if e:var(FLAGS) & 0x20 ~= 0 then
        throb(e)
    end
    local zero = e:routine(0)
    if zero == 0 then
        init(e)
    elseif zero == 1 then
        move(e)
    elseif zero == 2 then
        hurt(e)
    elseif zero == 3 then
        death(e)
    end
    spurt(e)
    eye(e)
    e:seq_sound()
end
