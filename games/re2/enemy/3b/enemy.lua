-- The giant moth's larvae, `Em_3B` (entity id 0x3B).
--
-- `Em_3B` (`0x4b0370`) runs `Routine_0` from `0x52bf20`: 0 is the init
-- (`FUN_004b03d0`), 1 the move (`FUN_004b06c0`), 3 the death
-- (`FUN_004b0e80`) and 7 `FUN_004b0fd0`; 2 and 4 are `nullsub`. The move runs
-- `Routine_1` from `0x52bf50`. The engine runs `Oba_ck_em` and `Sca_ck_em`
-- with `0x400` after `update`.
--
-- Only the wriggle in place (`Routine_1` 5, room 613's six on their table) is
-- ported: the crawl (0 to 4), the death and state 7 aren't yet, and neither
-- are the kinds that climb the walls or come from the moth, or the shadow
-- (`Kage_work_set`).

local CNT_219 <const> = 0 -- `+0x219`, a count down.
local CNT_21C <const> = 1 -- `+0x21c`: at its end the player's `0x989eec` 0x10 clears.
local TURN <const> = 2 -- `+0x21e`: the wriggle's turn a tick, 2 or -2.
local B21A <const> = 3 -- `+0x21a`.
local B21B <const> = 4 -- `+0x21b`.
local B224 <const> = 5 -- `+0x224`.

-- `0x52bf40`: `Routine_1` by `Type & 0xF`.
local START <const> = { [0] = 0, 1, 2, 3, 3, 3, 3, 4, 5, 0, 0, 0, 0, 0, 0, 0 }

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

-- `FUN_004b03d0`. The kinds the moth lays (0x6002), and those on a wall
-- (0x2003 to 0x2006) or that fall (0x6001), aren't ported: they face as the
-- default does.
local function init(e)
    e:routines(1, START[e:kind() & 0xF], 0, 0)
    e:set_damage_flg(0)
    e:set_d_life(0xD)
    e:set_life(1)
    e:set_be_flg(e:be_flg() | 0x4400002)
    e:set_body_box(-200, 100, 200, 200)
    e:set_var(B21A, 0)
    e:set_var(B21B, 0xF)
    e:set_var(CNT_21C, 0)
    e:clip(0, e:rnd() & 0xF, 0)
    e:joint(0x80)
    e:set_be_flg(e:be_flg() | 0x400000)
    if e:kind() == 0x2008 then
        e:set_var(B224, 1)
    end
    e:set_angle((e:rnd() & 0xFF) << 4)
    e:set_speed(0, 0, 0)
end

-- `FUN_004b0d20`: turning one way and then the other in place, `0x1000` over
-- 2 a tick each way. With the player's `Type` (`DAT_00989ffe`, the weapon
-- outside a script's move) 0x11, the body box reaches the floor.
local function wriggle(e)
    if e:routine(2) == 0 then
        e:set_routine(2, 1)
        e:set_speed(2, 0, 0)
        e:set_var(TURN, (e:rnd() & 1) ~= 0 and 2 or -2)
        e:set_timer(0, 0x1000)
    end
    e:set_angle(e:angle() + e:var(TURN))
    e:add_speed_xz()
    e:joint(0x80)
    e:set_timer(0, e:timer(0) - 2)
    if i16(e:timer(0)) < 0 then
        e:set_var(TURN, -e:var(TURN))
        e:set_timer(0, 0x1000)
    end
    local _, y = e:pos()
    if e:weapon() == 0x11 then
        e:set_body_box(y, 100, -y, 200)
    else
        e:set_body_box(-200, 100, 200, 200)
    end
end

local MOVES <const> = { [5] = wriggle }

-- `FUN_004b06c0`.
local function move(e)
    local left = e:var(CNT_21C)
    if left ~= 0 then
        left = (left - 1) & 0xFF
        e:set_var(CNT_21C, left)
        if left == 0 then
            e:clear_status(0x10)
        end
    end
    if e:var(CNT_219) ~= 0 then
        e:set_var(CNT_219, e:var(CNT_219) - 1)
    end
    local step = MOVES[e:routine(1)]
    if step then
        step(e)
    end
end

local STATES <const> = { [0] = init, [1] = move }

function update(e)
    if e:damage_cnt() & 0x7F ~= 0 then
        e:set_damage_cnt(e:damage_cnt() - 1)
    end
    local state = STATES[e:routine(0)]
    if state then
        state(e)
    end
end
