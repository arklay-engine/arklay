-- The vines, `Em_2F` (entity id 0x2f).
--
-- A room places one with `Type` 0 as the template the ivy grows its own from
-- (`Set_enemy_work`), and some alone, with `Type` 0x20 and up. An ivy's vine
-- has the part it grows on, plus one, in `Type`'s low byte, and its ivy in
-- the `parent_` bindings: it sits on that part each tick, and takes its routine from the
-- command the ivy left in its `+0x225` and `+0x227`. The engine runs the
-- animation and `Oba_ck_em` and `Sca_ck_em` after `update`.
--
-- The vines alone (`Routine_1` 9) and the vines' damage and death
-- (`Routine_0` 2 and 3) aren't ported: no run reaches them yet.

local INDEX <const> = 0 -- `+0x218`: which of its ivy's vines, from 0.
local BITS <const> = 1 -- `+0x219`: the commands `+0x227` that are for it.
local YAW <const> = 5 -- `+0x220`: its turn from its ivy's.
local WAIT <const> = 6 -- `+0x222`.
local CHOSEN <const> = 7 -- `+0x223`.
local SPIN <const> = 8 -- `+0x224`: the turn `Em_2F`'s init chose.
local ROLL <const> = 9 -- `+0x226`: its roll from its ivy's.
local HIT <const> = 10 -- `+0x229`: its lash hit this swing.
local ALONE <const> = 13 -- `+0x22c`: placed alone, with no ivy.

-- The ivy's fields (`lib/ivy.lua`).
local IVY_TARGET <const> = 0
local IVY_COMMAND <const> = 8
local IVY_COMMAND_TO <const> = 9

local KIND_GROWN <const> = 0x2000
local KIND_TEMPLATE <const> = 0x4000

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function set_clip(e, word)
    e:clip(word & 0xFF, (word >> 8) & 0xFF, (word >> 16) & 0xFF)
end

-- `FUN_00489ed0`: on its ivy's part, turned as the ivy is and by its own.
local function place(e)
    local part = (e:kind() & 0xFF) - 1
    local x, y, z = e:parent_part(part)
    e:set_pos(x, y, z)
    e:set_angle((e:var(YAW) + e:parent_angle()) & 0xFFF)
    e:set_tilt(0, e:var(ROLL) & 0xFFF)
end

-- `FUN_00488a70`.
local function init(e)
    e:routines(1, 0, 0, 0)
    e:set_be_flg(e:be_flg() | 0x44A)
    e:set_life(-1)
    e:set_neck_flags(1)
    e:set_var(WAIT, 0)
    e:set_var(ROLL, 0)
    e:set_var(HIT, 0)
    e:set_var(11, 0)
    e:set_var(12, 0)
    e:set_body_ofs_x(0)
    e:set_body_box(-500, 500, 500, 500)
    local rnd = e:rnd()
    e:set_kind(e:kind() | KIND_GROWN)
    e:clip(0, rnd & 15, 7)
    if e:kind() & 0xFF == 0 then
        e:set_be_flg(e:be_flg() | 0x80400)
        e:set_kind(e:kind() | KIND_TEMPLATE)
        return
    end
    e:set_var(ALONE, 0)
    if e:kind() & 0xFF < 0x20 then
        e:set_kind(e:kind() | KIND_TEMPLATE)
        local turn = e:rnd() * 2
        e:set_angle(turn)
        e:set_var(SPIN, turn)
        place(e)
        return
    end
    -- Alone (`Routine_1` 9).
    e:set_tilt(0x400, 0)
    e:set_routine(1, 9)
    e:set_var(ALONE, 1)
    e:set_be_flg(e:be_flg() | 0x18010000)
    if e:kind() == 0x2021 then
        e:set_life(-1)
    else
        e:set_life(1)
        e:set_angle(0x800)
    end
end

-- `FUN_00488d40`: at rest on the ivy, `Routine_1` 0 to 2.
local function rest(e)
    if e:routine(2) == 0 then
        e:set_var(YAW, e:var(SPIN))
        e:set_routine(2, 1)
        e:set_var(ROLL, 0)
        local one = e:routine(1)
        set_clip(e, (((e:var(INDEX) << (one + 2)) + 0x700) * 0x100 + one) & 0xFFFFFFFF)
    end
    e:joint(0x200)
    place(e)
end

-- `FUN_00489030`: `Routine_1` 4, wrapped round the player the ivy holds.
local function wrap(e)
    local sub = e:routine(2)
    if sub == 0 then
        e:set_routine(2, 1)
        local a = e:rnd()
        local b = e:rnd()
        set_clip(e, (a & 7) * 0x100 + 0x70005 + (b & 1))
        -- The ivy's target `+0x218`: only the player (0) is ported.
        e:set_timer(0, e:direction_to_player())
        sub = 1
    end
    if sub == 1 then
        e:set_var(YAW, i16(e:timer(0) - e:parent_angle()))
        e:set_var(ROLL, i16(e:var(ROLL) - 0x100 - (e:rnd() & 0x1F)))
        if e:var(ROLL) < -0x3FF then
            e:set_routine(2, 2)
        end
    elseif sub == 2 then
        if e:parent_var(IVY_COMMAND_TO) ~= 0 then
            e:set_routine(2, 3)
            set_clip(e, e:var(INDEX) * 0x800 + 0x70001)
        end
    elseif sub == 3 then
        e:set_var(YAW, i16(e:timer(0) - e:parent_angle()))
        e:set_var(ROLL, i16(e:var(ROLL) + (e:rnd() & 0x1F) + 0x100))
        e:joint(0x200)
        if e:var(ROLL) >= 0 then
            e:set_routine(2, 4)
        end
    end
    e:joint(0x200)
    place(e)
end

local STATES <const> = { [0] = rest, rest, rest, nil, wrap }

-- `FUN_0048a190`: the command's state, or with `0x40` a `Routine_0`.
local function obey(e, state)
    if state & 0x40 == 0 then
        e:set_routine(1, state)
        e:set_routine(2, 0)
        e:set_routine(3, 0)
    else
        e:routines(2, state & 0xF, 0, 0)
    end
end

-- `FUN_00488c90`.
local function main(e)
    if e:kind() & 0xFF == 0 then
        return
    end
    local state = STATES[e:routine(1)]
    if state then
        state(e)
    end
    if e:var(ALONE) ~= 0 or not e:has_parent() then
        return
    end
    local command = e:parent_var(IVY_COMMAND)
    if command == 0 then
        return
    end
    local wanted = ((command & 0x7F) - 1) & 0xFF
    if e:routine(1) == wanted then
        return
    end
    if command & 0x80 ~= 0 then
        if e:parent_var(IVY_COMMAND_TO) == e:var(INDEX) then
            obey(e, wanted)
        end
    elseif e:parent_var(IVY_COMMAND_TO) & e:var(BITS) ~= 0 then
        obey(e, wanted)
    end
end

local ROUTINES <const> = { [0] = init, main }

-- `Em_2F`.
function update(e)
    if e:damage_cnt() & 0x7F ~= 0 then
        e:set_damage_cnt(e:damage_cnt() - 1)
    end
    if e:var(WAIT) ~= 0 then
        e:set_var(WAIT, e:var(WAIT) - 1)
    end
    local routine = ROUTINES[e:routine(0)]
    if routine then
        routine(e)
    end
end
