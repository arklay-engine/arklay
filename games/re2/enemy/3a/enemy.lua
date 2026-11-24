-- The giant moth, `Em_3A` (entity id 0x3A).
--
-- `Em_3A` (`0x4acc70`) runs `Routine_0` from `0x52bbf8`: 0 is the init
-- (`FUN_004acd70`), 1 the move (`FUN_004acfa0`), 2 the hit (`FUN_004aea50`),
-- 3 the death (`FUN_004af4f0`) and 7 `FUN_004afa20`; 4 is `nullsub`. The move
-- runs `Routine_1` from `0x52bc18`. Room 613's moth (`Type` 0x10) waits on the
-- ceiling in `Routine_1` 10 until the room's area 3 raises `Sce_free0`. The
-- engine runs `Oba_ck_em` and `Sca_ck_em` with `0x400` after `update`.
--
-- Not ported yet: the move's other routines, the hit, the death, state 7, the
-- player's side of the moth's hold (`0x4afac0`, which the init hooks in at
-- `0x9886e0`), the partner as the target, the shadow (`Kage_work_set`), and
-- the parts' draw words the init sets (part 0's from `+0x98`, and parts 13 to
-- 16's `0x4000`).

local TARGET <const> = 0 -- `+0x218`: 1 while the partner is the target.
local WINGS <const> = 1 -- `+0x21e`: the wings' beat, by `FUN_004afbf0`'s table.
local HELD <const> = 2 -- `+0x220`: with `UP`, `Be_flg` takes 0x18000000 for 0xC000000.
local FLAGS <const> = 3 -- `+0x221`: 1 sheds scales as `SHED` counts; 0x80 beats the wings.
local SHOT <const> = 4 -- `+0x227`: a hit that wakes it.
local B228 <const> = 5 -- `+0x228`.
local UP <const> = 6 -- `+0x229`: 1 while it hangs from the ceiling.
local B22C <const> = 7 -- `+0x22c`: a count down; 0x3C while the player's `0x989eec` 0x20.
local B22D <const> = 8 -- `+0x22d`.
local B22F <const> = 9 -- `+0x22f`.
local SHED <const> = 10 -- `+0x230`: a count down; each odd tick sheds scales.
local B232 <const> = 11 -- `+0x232`: a count down.
local DUST <const> = 12 -- `+0x233`: the ticks of dust left.
local B23A <const> = 13 -- `+0x23a`: cleared while `Damage_cnt` is 0.
-- `+0x224` to `+0x226` and `+0x234` to `+0x238`: the routines and the speed as
-- the move left them.
local LAST_R1 <const> = 14
local LAST_R2 <const> = 15
local LAST_R3 <const> = 16
local LAST_SX <const> = 17
local LAST_SY <const> = 18
local LAST_SZ <const> = 19
local TURN <const> = 20 -- `+0x158`, `Timer0`: the wake's facing to come back to.

local TYPE_CEILING <const> = 0x10

local function i16(value)
    value = value & 0xFFFF
    return value >= 0x8000 and value - 0x10000 or value
end

local function i8(value)
    value = value & 0xFF
    return value >= 0x80 and value - 0x100 or value
end

-- `0x52be08`: the effects' rows, `{ kind, row, size }`, where kind's 0x80
-- starts it on the part's matrix.
local ROWS <const> = {
    [0] = { 0x00, 25, 0x2000 },
    { 0x00, 25, 0x2800 },
    { 0x00, 1, 0x1800 },
    { 0x85, 4, 0x1000 },
    { 0x85, 1, 0x1000 },
    { 0x20, 0, 0x2800 },
    { 0x21, 0, 0x2000 },
    { 0x85, 4, 0x1800 },
    { 0x84, 15, 0x1000 },
    { 0x84, 15, 0x1800 },
    { 0x86, 0, 0x1000 },
    { 0x84, 12, 0x2000 },
    { 0x00, 9, 0x2800 },
    { 0x85, 1, 0x1800 },
}
-- `0x52be60`: the points `FUN_004b0170` starts an effect at, `{ part, x, y, z }`.
local POINTS <const> = {
    [0] = { 3, 0, 0, 0 },
    { 3, -200, 0, -1400 },
    { 3, 400, 0, -800 },
    { 4, 0, 0, -500 },
    { 5, 0, 0, 0 },
    { 5, 200, 0, 1300 },
    { 5, -300, 0, 900 },
    { 6, 0, 0, 400 },
    { 3, -600, -200, -1300 },
    { 3, 500, -100, -1400 },
    { 5, 700, -300, 1500 },
    { 5, -400, -200, 1600 },
}

-- The target's place: the player, or with `TARGET` the partner.
local function target_xz(e)
    local x, _, z = e:player_pos()
    return x, z
end

-- `FUN_004b0080`: row `row` at `part`, toward the target.
local function effect_at_part(e, part, row)
    local r = ROWS[row]
    local mx, _, mz = e:pos()
    local tx, tz = target_xz(e)
    local dir = e:direction(mx, mz, tx, tz)
    if r[1] & 0x80 ~= 0 then
        e:effect(r[1] & 0x7F, r[2], r[3], dir, part, 0, 0, 0)
    else
        local px, py, pz = e:own_part(part)
        e:call_effect(r[1], r[2], r[3], dir, px, py, pz)
    end
end

-- `FUN_004b0170`: row `row` at `POINTS[point]`, along the moth's facing. A
-- row of 0x80 or more starts it on the floor under the part, turned with the
-- body (`FUN_004b0270`), which isn't ported.
local function effect_at_point(e, row, point)
    local r = ROWS[row & 0x7F]
    local p = POINTS[point]
    e:effect(r[1] & 0x7F, r[2], r[3], e:angle(), p[1], p[2], p[3], p[4])
end

-- `FUN_004b0310`: the body box for the ceiling (`on`) or the room.
local function box(e, on)
    if on then
        e:set_body_box(0, 800, 0x7D0, 4)
    else
        e:set_body_box(-800, 800, 1000, 800)
    end
end

-- `FUN_004afe50`: the target, the player unless the partner is nearer.
-- The partner isn't ported.
local function pick_target(e)
    e:set_var(TARGET, 0)
end

-- `FUN_004acd70`.
local function init(e)
    e:set_be_flg(e:be_flg() | 0xC00000)
    e:routines(1, 0, 0, 0)
    e:set_damage_flg(0)
    e:set_speed(0, 0, 0)
    local life = e:easy() and 100 or 150
    if e:hard_mode() then
        life = 200
    end
    e:set_life(life)
    e:set_d_life(life // 3)
    e:set_be_flg(e:be_flg() | 0xC000000)
    box(e, false)
    e:set_var(FLAGS, 1)
    e:set_var(B22D, 2)
    e:set_var(B22F, 0x1E)
    if e:kind() & TYPE_CEILING ~= 0 then
        e:set_routine(1, 10)
        e:clip(8, 0, 0)
        e:joint(0x100)
        e:set_var(UP, 1)
        e:set_var(HELD, 1)
        e:set_sce_free(0, 0)
    else
        e:clip(0, 0, 0)
        e:joint(0x100)
    end
end

-- `FUN_004ae530`, `Routine_1` 10's first step: up against the ceiling.
local function ceiling_hang(e)
    e:set_routine(2, 1)
    e:set_routine(3, 0)
    e:set_be_flg(e:be_flg() | 4)
    local _, roll = e:cdir()
    e:set_cdir(0, roll)
    box(e, true)
    local _, sy = e:speed()
    e:set_speed(0x30F, sy, 0)
    e:add_speed_xz()
    e:set_cdir(0, -0x400)
    e:clip(8, 0, 0)
    e:joint(0x100)
end

-- `FUN_004ae5a0`: until the room's scene word, or a hit.
local function ceiling_wait(e)
    if e:routine(3) == 0 then
        e:set_routine(3, 1)
        e:set_var(SHOT, 0)
    end
    if e:sce_free(0) ~= 0 or e:var(SHOT) ~= 0 then
        e:set_routine(2, 2)
        e:set_routine(3, 0)
    end
    e:joint(0x100)
end

local CEILING <const> = { [0] = ceiling_hang, ceiling_wait }

-- `FUN_004ae510`.
local function ceiling(e)
    local step = CEILING[e:routine(2)]
    if step then
        step(e)
    end
end

local MOVES <const> = { [10] = ceiling }

-- `FUN_004acfa0`.
local function move(e)
    pick_target(e)
    if e:var(B22C) ~= 0 then
        e:set_var(B22C, e:var(B22C) - 1)
    end
    local step = MOVES[e:routine(1)]
    if step then
        step(e)
    end
    e:set_var(LAST_R1, e:routine(1))
    e:set_var(LAST_R2, e:routine(2))
    e:set_var(LAST_R3, e:routine(3))
    local sx, sy, sz = e:speed()
    e:set_var(LAST_SX, sx)
    e:set_var(LAST_SY, sy)
    e:set_var(LAST_SZ, sz)
    if e:status(0x20) then
        e:set_var(B22C, 0x3C)
    end
end

local STATES <const> = { [0] = init, [1] = move }

-- `FUN_004afbf0`: the scales a shedding moth drops on its odd ticks, and the
-- wings' beat, which isn't ported.
local function shed(e)
    local left = e:var(SHED)
    if left ~= 0 then
        left = left - 1
        e:set_var(SHED, left)
        if e:var(FLAGS) & 1 ~= 0 and left & 1 ~= 0 then
            effect_at_point(e, 3, e:rnd() & 7)
        end
    end
end

-- `FUN_004acc70`.
function update(e)
    if e:damage_cnt() & 0x7F ~= 0 then
        e:set_damage_cnt(e:damage_cnt() - 1)
    else
        e:set_var(B23A, 0)
    end
    if e:var(B232) ~= 0 then
        e:set_var(B232, e:var(B232) - 1)
    end
    local state = STATES[e:routine(0)]
    if state then
        state(e)
    end
    shed(e)
    if e:var(DUST) ~= 0 then
        local pick = e:rnd() & 3
        if pick ~= 0 then
            effect_at_part(e, pick - 1, 0xA)
        else
            effect_at_point(e, 0xA, e:rnd() & 3)
            effect_at_point(e, 0xA, (e:rnd() & 3) + 4)
        end
        e:set_var(DUST, e:var(DUST) - 1)
    end
    local flags = e:be_flg() & ~0x1C000000
    if e:var(HELD) == 0 and e:var(UP) == 0 then
        flags = flags | 0xC000000
    else
        flags = flags | 0x18000000
    end
    e:set_be_flg(flags)
end
