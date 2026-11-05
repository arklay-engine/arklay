-- Inventory management for the player bot: the item box and the combine.
--
-- Both go through the game's own screens with pad input. The item box is a
-- room action of kind "item_box" (the engine's room-action handler 0x08):
-- the planner walks into its zone and presses action, the lid opens and the
-- box screen comes up over the inventory panel (bot:menu().box). There the
-- bot moves the inventory cursor with the d-pad, confirms the slot (the
-- screen arms it), pages the box cursor up or down to the box slot and
-- confirms again: the engine swaps the two. Cancel closes the box and the
-- pause menu. The combine is the menu's MOVE (BotMenu.combine).
--
-- The policy (BotInventory.keep, .deposits, .withdrawals, .combine_pick)
-- works on plain tables so the unit tests can drive it:
--   keep:     the equipped item, a firearm the knowledge file fights with
--             and its ammunition (the ammo reserve), every key, every item
--             an open <use> fact needs (bot:use_facts(), done flag clear),
--             and every heal item. Anything else may go in the box.
--   deposit:  when the inventory is at its limit and a known pickup
--             (a frontier item that merges into no held stack) needs room,
--             or, at a box in this room, when no more than RESERVE slots
--             are free and more pickups than that need one (the slot
--             reserve); then every item the keep rule does not hold goes in.
--   look:     before any box was opened, at a box in this room with a slot
--             free (or from anywhere while sourcing ammunition,
--             BotSurvival.sourcing): one visit to read what the shared
--             box holds;
--   withdraw: an item in the box that an open <use> fact needs, a key a
--             remembered locked door needs, the equipped gun's ammunition
--             below the reserve floor, or a heal item when none is held;
--             only into free slots. At a box in this room, also every
--             stack of that ammunition while the rounds are under TOP_UP
--             times the floor, as long as RESERVE slots stay free.
--   combine:  only when the inventory is at its limit and a pickup needs
--             room: a pair whose MOVE (bot:combine_result) empties a slot,
--             of heal, ammo or weapon items only, never a key or an item
--             a <use> fact needs; heal pairs first, so herbs mix into
--             stronger heals that stay in the inventory.
-- What the box holds is remembered (bot:remember "box_items") each time its
-- screen is up, so the memory checkpoint a save keeps agrees with the
-- box the save holds. No room literals; only pad/UI input.

BotInventory = BotInventory or {}

local C = BotConst

local FIRST_SLOT_CURSOR = 8
-- One button pulse per this many ticks on the box screen, so each press is
-- a fresh edge and the screen's state is read back between presses.
local PRESS_GAP = 3
-- The box screen takes no input for its first few ticks (the menu reveal).
local OPEN_SETTLE = C.SECOND // 2
-- One deposit or withdrawal gives up after this long; the whole box visit
-- after the next.
local OP_TIMEOUT = 15 * C.SECOND
local SESSION_TIMEOUT = 60 * C.SECOND
-- After the box press, the lid animates before the screen comes up.
local LID_WAIT = 5 * C.SECOND
-- The close fade and the room's post-menu freeze.
local SETTLE_TIMEOUT = 4 * C.SECOND
-- Controllable ticks in a row that end a visit.
local CLEAR_TICKS = C.SECOND
-- Slots kept free for pickups at a box in this room (the slot reserve).
BotInventory.RESERVE = 1
-- At a box here, the equipped gun's ammunition is topped up under this many
-- times the reserve floor.
local TOP_UP = 2
-- Kinds the combine may mix.
local MIX_KINDS = { heal = 1, ammo = 2, weapon = 2 }

local function state()
    BOT_STATE.inventory = BOT_STATE.inventory or {}
    return BOT_STATE.inventory
end

local function log(bot, text)
    bot:log("inventory: " .. text)
end

-- ---- policy (plain values) ---------------------------------------------

-- Why `row` ({item, kind, quantity}) stays in the inventory, or nil.
-- ctx: {equipped, guns = {[id] = true}, ammo = {[id] = true},
--       uses = {[id] = true}}
function BotInventory.keep(row, ctx)
    local item = row.item
    if item == 0 then
        return "empty"
    end
    if row.kind == "key" then
        return "key"
    end
    if ctx.uses and ctx.uses[item] then
        return "a <use> fact needs it"
    end
    if item == ctx.equipped then
        return "equipped"
    end
    if row.kind == "weapon" and ctx.guns and ctx.guns[item] then
        return "firearm"
    end
    if row.kind == "ammo" and ctx.ammo and ctx.ammo[item] then
        return "ammo reserve"
    end
    if row.kind == "heal" then
        return "heal"
    end
    return nil
end

-- The menu rows (bot:menu().items) the box should take, in slot order.
function BotInventory.deposits(items, ctx)
    local out = {}
    for _, row in ipairs(items) do
        if row.item ~= 0 and not BotInventory.keep(row, ctx) then
            out[#out + 1] = row
        end
    end
    return out
end

-- Heal stacks a full inventory keeps when it must make room for a needed
-- pickup; the rest may go in the box (the "none held" withdrawal brings one
-- back).
BotInventory.SOFT_HEALS = 1

-- When nothing the keep rule lets go is held, the inventory is full and a
-- needed pickup (`needed`, a count: a key, an item a <use> fact needs, or
-- the ammunition a held gun lacks) has no slot, the softer keeps that may
-- go in, at most `needed` of them: first heal stacks past SOFT_HEALS (the
-- last in slot order), then a key no remembered locked door needs.
function BotInventory.soft_deposits(items, ctx, needed)
    local out = {}
    if not ctx.full or (needed or 0) == 0 then
        return out
    end
    -- Spare heals first: a key whose door is not seen yet costs a trip back.
    local heals = {}
    for _, row in ipairs(items) do
        if row.item ~= 0 and BotInventory.keep(row, ctx) == "heal" then
            heals[#heals + 1] = row
        end
    end
    for i = #heals, BotInventory.SOFT_HEALS + 1, -1 do
        if #out >= needed then
            return out
        end
        out[#out + 1] = heals[i]
    end
    for _, row in ipairs(items) do
        if #out >= needed then
            return out
        end
        if row.item ~= 0 and BotInventory.keep(row, ctx) == "key"
            and not (ctx.keys and ctx.keys[row.item]) then
            out[#out + 1] = row
        end
    end
    return out
end

-- Why the box item `row` ({item, kind}) should come out, or nil.
-- ctx adds: keys = {[id] = true} (keys remembered locked doors need),
--           ammo_low = the equipped gun's ammunition below the floor,
--           heals = heal stacks held.
function BotInventory.wanted_back(row, ctx)
    local item = row.item
    if item == 0 then
        return nil
    end
    if ctx.uses and ctx.uses[item] then
        return "a <use> fact needs it"
    end
    if ctx.keys and ctx.keys[item] then
        return "a locked door needs it"
    end
    if ctx.ammo_low and ctx.ammo_low == item then
        return "ammunition under the reserve floor"
    end
    if ctx.gun_ammo and ctx.gun_ammo[item] then
        return "ammunition for a held gun"
    end
    if ctx.top_up and ctx.top_up == item and not ctx.reserve_only then
        return "ammunition top-up at the box"
    end
    if row.kind == "heal" and (ctx.heals or 0) == 0 then
        return "no heal item held"
    end
    return nil
end

-- The box rows to take back, at most `free` of them, box order.
function BotInventory.withdrawals(box_items, ctx, free)
    local out = {}
    local heal_taken = false
    for _, row in ipairs(box_items) do
        if #out >= free then
            break
        end
        local why = BotInventory.wanted_back(row, ctx)
        -- A top-up takes the stacks only while the reserve stays free.
        local top_up = why == "ammunition top-up at the box"
        if top_up and free - #out <= BotInventory.RESERVE then
            why = nil
        end
        -- One heal stack is enough to stop the "none held" rule.
        if why and not (row.kind == "heal" and heal_taken
            and not (ctx.uses and ctx.uses[row.item])) then
            if row.kind == "heal" then
                heal_taken = true
            end
            out[#out + 1] = row
        end
    end
    return out
end

-- The slot reserve at a box here: deposit when no more than RESERVE slots
-- are free and more known pickups than that need one. ctx: {capacity,
-- used}; `deposits` the count the keep rule lets go; `pickups` the count
-- of known pickups needing a slot.
function BotInventory.reserve_due(ctx, deposits, pickups)
    local free = ctx.capacity - ctx.used
    return deposits > 0 and free <= BotInventory.RESERVE and pickups > free
end

-- The best MOVE that empties a slot: `{slot, target}` (0-based menu slots)
-- or nil. `result(slot, target)` is bot:combine_result.
function BotInventory.combine_pick(items, ctx, result)
    local best, best_rank = nil, nil
    for i, a in ipairs(items) do
        for j, b in ipairs(items) do
            if i ~= j and a.item ~= 0 and b.item ~= 0
                and MIX_KINDS[a.kind] and MIX_KINDS[b.kind]
                and not (ctx.uses and (ctx.uses[a.item] or ctx.uses[b.item])) then
                local r = result(a.slot, b.slot)
                if r.frees and (r.result == "applied" or r.result == "herb") then
                    local rank = MIX_KINDS[a.kind] + MIX_KINDS[b.kind]
                    if not best_rank or rank < best_rank then
                        best, best_rank = { slot = a.slot, target = b.slot, item = a.item,
                            other = b.item }, rank
                    end
                end
            end
        end
    end
    return best
end

-- The box slot nearest `cursor` (forward or back, wrapping over `count`
-- slots) that satisfies `want(row)`, and the direction to page: +1 down
-- (next), -1 up (previous), 0 already there. nil when none does.
function BotInventory.box_target(rows, cursor, want)
    local count = #rows
    local best, best_dist, best_dir = nil, nil, 0
    for _, row in ipairs(rows) do
        if want(row) then
            local fwd = (row.slot - cursor) % count
            local back = (cursor - row.slot) % count
            local dist, dir = fwd, 1
            if back < fwd then
                dist, dir = back, -1
            end
            if dist == 0 then
                dir = 0
            end
            if not best_dist or dist < best_dist then
                best, best_dist, best_dir = row, dist, dir
            end
        end
    end
    return best, best_dir
end

-- ---- reads ---------------------------------------------------------------

local function used_slots(items)
    local n = 0
    for _, row in ipairs(items) do
        if row.item ~= 0 then
            n = n + 1
        end
    end
    return n
end

-- The policy context from the live state.
function BotInventory.context(bot)
    local ctx = { uses = {}, guns = {}, ammo = {}, keys = {}, heals = 0 }
    ctx.equipped = bot:player().equipped
    local knowledge = bot:knowledge()
    for id, _ in pairs(knowledge and knowledge.weapons or {}) do
        ctx.guns[id] = true
        -- The reserve is kept only for a gun the bot holds: shells carried
        -- before the shotgun fill a slot a pickup needs, and the box gives
        -- them back once the gun is equipped (the reserve-floor withdrawal).
        local ammo = bot:ammo_for(id)
        if ammo and bot:has_item(id) then
            ctx.ammo[ammo] = true
            if bot:item_count(ammo) == 0 then
                ctx.gun_ammo = ctx.gun_ammo or {}
                ctx.gun_ammo[ammo] = true
            end
        end
    end
    if ctx.equipped and bot:ammo_for(ctx.equipped) then
        ctx.ammo[bot:ammo_for(ctx.equipped)] = true
    end
    for _, fact in ipairs(bot:use_facts()) do
        if not bot:flag(fact.bank, fact.bit) then
            ctx.uses[fact.item] = true
        end
    end
    -- An item a wanted <deliver> needs is kept like a <use> item.
    for item, _ in pairs(BotPuzzle.wanted_items(bot)) do
        ctx.uses[item] = true
    end
    for _, room in ipairs(bot:world().rooms) do
        for _, door in ipairs(room.doors) do
            if door.present and door.locked and not door.unlocked and not door.traversed then
                ctx.keys[door.key] = true
            end
        end
    end
    ctx.ammo_low = BotSurvival.ammo_need(bot)
    local weapon = bot:weapon()
    if weapon.ammo then
        local rounds, shots = BotSurvival.rounds(bot)
        local floor = BotSurvival.ammo_floor(shots, bot:risk() or 0.25)
        if floor > 0 and rounds < TOP_UP * floor then
            ctx.top_up = weapon.ammo
        end
    end
    local menu = bot:menu()
    for _, row in ipairs(menu.items or {}) do
        if row.kind == "heal" and row.quantity > 0 then
            ctx.heals = ctx.heals + 1
        end
    end
    ctx.capacity = menu.capacity or #(menu.items or {})
    ctx.used = used_slots(menu.items or {})
    ctx.full = ctx.used >= ctx.capacity
    return ctx
end

-- What the bot remembers the box holds: {[item] = true}.
function BotInventory.remembered(bot)
    local out = {}
    local text = bot:memory().box_items or ""
    for hex in string.gmatch(text, "%x+") do
        out[tonumber(hex, 16)] = true
    end
    return out
end

-- Known pickups that need a free slot: frontier items that merge into no
-- held stack. `skip(goal)` drops one the planner has given up on.
function BotInventory.pickups_needing_room(bot, skip)
    -- A due <fetch> hands over an item that needs a slot too.
    local n = #BotPuzzle.fetches_due(bot)
    for _, goal in ipairs(bot:world().frontier) do
        if goal.kind == "item" and not (skip and skip(goal)) then
            local _, merge = bot:item_kind(goal.item)
            if not (merge and bot:has_item(goal.item)) then
                n = n + 1
            end
        end
    end
    return n
end

-- Known pickups that need a free slot and that the run needs: a key, an
-- item a <use> fact needs, or the ammunition a held gun lacks.
function BotInventory.needed_pickups(bot, ctx, skip)
    local n = #BotPuzzle.fetches_due(bot)
    for _, goal in ipairs(bot:world().frontier) do
        if goal.kind == "item" and not (skip and skip(goal)) then
            local kind, merge = bot:item_kind(goal.item)
            local needed = kind == "key" or ctx.uses[goal.item]
                or (ctx.gun_ammo and ctx.gun_ammo[goal.item])
                or ctx.ammo_low == goal.item
            if needed and not (merge and bot:has_item(goal.item)) then
                n = n + 1
            end
        end
    end
    return n
end

-- What a deposit puts in the box: the keep rule's leftovers, else the soft
-- keeps (BotInventory.soft_deposits) when a needed pickup has no slot.
function BotInventory.make_room(bot, ctx, items, skip)
    local deposits = BotInventory.deposits(items, ctx)
    if #deposits == 0 and ctx.full then
        local needed = BotInventory.needed_pickups(bot, ctx, skip)
        -- A box item the bot needs back (a key a locked door needs, an
        -- item a <use> fact needs) is a needed pickup too: a swap.
        for item, _ in pairs(BotInventory.remembered(bot)) do
            local why = BotInventory.wanted_back({ item = item, kind = bot:item_kind(item) }, ctx)
            if why == "a <use> fact needs it" or why == "a locked door needs it"
                or why == "ammunition for a held gun" then
                needed = needed + 1
            end
        end
        deposits = BotInventory.soft_deposits(items, ctx, needed)
    end
    return deposits
end

-- Whether to open an item box only to see what it holds: no box has ever
-- been opened (`box_items`, the remembered contents, is nil), a slot is
-- free for what it may hold, and the box is in this room (`here`, a look on
-- the way) or the bot is sourcing ammunition (BotSurvival.sourcing, worth
-- a trip). Every box shares one store, so one look covers them all.
function BotInventory.look_in_box(ctx, box_items, sourcing, here)
    return (sourcing == true or here == true) and box_items == nil
        and ctx.used < ctx.capacity
end

-- Why the bot should visit an item box now, or nil: a deposit (full, a
-- pickup needs room and something may go in) or a withdrawal (the box
-- holds an item the bot needs and a slot is, or can be made, free).
-- `here` asks for a box in this room, which also takes the slot reserve,
-- a first look and the ammunition top-up (see the header); a trip to a
-- remote box is worth only the rest.
function BotInventory.box_reason(bot, skip, here)
    local ctx = BotInventory.context(bot)
    local items = bot:menu().items or {}
    local deposits = BotInventory.make_room(bot, ctx, items, skip)
    local pickups = BotInventory.pickups_needing_room(bot, skip)
    if ctx.full and #deposits > 0 and pickups > 0 then
        return "deposit: the inventory is full and a pickup needs room"
    end
    if here and BotInventory.reserve_due(ctx, #deposits, pickups) then
        return "deposit: keep a slot free for pickups"
    end
    if BotInventory.look_in_box(ctx, bot:memory().box_items, BotSurvival.sourcing_now(bot),
        here) then
        return here and "look in the box on the way: never opened"
            or "look in the box: sourcing ammunition and the box was never opened"
    end
    if not here then
        ctx.reserve_only = true
    end
    local free = ctx.capacity - ctx.used
    if free > 0 or #deposits > 0 then
        for item, _ in pairs(BotInventory.remembered(bot)) do
            local why = BotInventory.wanted_back({ item = item, kind = bot:item_kind(item) }, ctx)
            if why == "ammunition top-up at the box" and free <= BotInventory.RESERVE then
                why = nil
            end
            if why then
                return string.format("withdraw 0x%02X: %s", item, why)
            end
        end
    end
    return nil
end

-- Why a combine is worth doing now, and the pick, or nil.
function BotInventory.combine_now(bot, skip)
    local ctx = BotInventory.context(bot)
    if not ctx.full or BotInventory.pickups_needing_room(bot, skip) == 0 then
        return nil
    end
    return BotInventory.combine_pick(bot:menu().items or {}, ctx, function(a, b)
        return bot:combine_result(a, b)
    end)
end

-- ---- the box screen --------------------------------------------------------

local function remember_box(bot, view)
    local ids = {}
    for _, row in ipairs(view.items) do
        if row.item ~= 0 then
            ids[#ids + 1] = string.format("%02X", row.item)
        end
    end
    local text = table.concat(ids, ",")
    if bot:memory().box_items ~= text then
        bot:remember("box_items", text)
    end
end

-- The plan for one box visit: deposits first (they free the slots the
-- withdrawals need), then withdrawals.
local function plan_visit(bot, menu, view)
    local ctx = BotInventory.context(bot)
    local ops = {}
    local free = ctx.capacity - ctx.used
    local deposits = BotInventory.make_room(bot, ctx, menu.items)
    local pickups = BotInventory.pickups_needing_room(bot)
    -- The reserve is judged after this visit's withdrawals, so one visit
    -- both takes and makes room.
    local taking = #BotInventory.withdrawals(view.items, ctx, free)
    local after = { capacity = ctx.capacity, used = ctx.used + taking }
    if (ctx.full and pickups > 0) or BotInventory.reserve_due(after, #deposits, pickups) then
        for _, row in ipairs(deposits) do
            ops[#ops + 1] = { kind = "deposit", item = row.item }
            free = free + 1
        end
    end
    for _, row in ipairs(BotInventory.withdrawals(view.items, ctx, free)) do
        ops[#ops + 1] = { kind = "withdraw", item = row.item }
    end
    local parts = {}
    for _, op in ipairs(ops) do
        parts[#parts + 1] = string.format("%s 0x%02X", op.kind, op.item)
    end
    log(bot, string.format("box open: %d op(s) [%s], inventory %d/%d", #ops,
        table.concat(parts, ", "), ctx.used, ctx.capacity))
    return ops
end

local function inventory_slot(menu, want)
    for _, row in ipairs(menu.items) do
        if want(row) then
            return row
        end
    end
    return nil
end

-- The d-pad key that moves the box screen's inventory cursor toward
-- `target`: off the EXIT tab row first, then the column, then the row.
local function cursor_key(cursor, target)
    if cursor < FIRST_SLOT_CURSOR then
        return "down"
    end
    if (cursor & 2) ~= (target & 2) then
        return "right"
    end
    if target > cursor then
        return "down"
    end
    return "up"
end

local function pulse(bot, s, key)
    local tick = bot:tick()
    if tick - (s.last_press or -C.SECOND) < PRESS_GAP then
        return
    end
    s.last_press = tick
    bot:set_input({ [key] = true })
end

-- One tick of the current op on the box screen. Returns true when the op
-- is over (done or given up).
local function drive_op(bot, s, menu, view, op)
    op.ticks = (op.ticks or 0) + 1
    if op.ticks > OP_TIMEOUT then
        log(bot, string.format("%s 0x%02X gave up (mode %s, box %d, cursor %d)",
            op.kind, op.item, view.mode, view.box_cursor, view.cursor))
        return true
    end
    if not op.before then
        op.before = bot:item_count(op.item)
        op.held = bot:has_item(op.item)
    end
    -- The swap shows in the engine's inventory.
    if op.kind == "deposit" and (bot:item_count(op.item) < op.before
        or not bot:has_item(op.item)) then
        s.deposits = (s.deposits or 0) + 1
        log(bot, string.format("deposited 0x%02X", op.item))
        return true
    end
    if op.kind == "withdraw" and (bot:item_count(op.item) > op.before
        or (bot:has_item(op.item) and not op.held)) then
        s.withdrawals = (s.withdrawals or 0) + 1
        log(bot, string.format("withdrew 0x%02X", op.item))
        return true
    end
    if view.sliding then
        return false
    end
    local row
    if op.kind == "deposit" then
        row = inventory_slot(menu, function(r) return r.item == op.item end)
    else
        row = inventory_slot(menu, function(r) return r.item == 0 end)
    end
    if not row then
        log(bot, string.format("%s 0x%02X: no inventory slot for it", op.kind, op.item))
        return true
    end
    local target_cursor = FIRST_SLOT_CURSOR + 2 * row.slot
    if view.mode == "browse" then
        if view.cursor ~= target_cursor then
            pulse(bot, s, cursor_key(view.cursor, target_cursor))
        else
            pulse(bot, s, "confirm")
        end
        return false
    end
    -- Armed: the inventory slot is fixed; a wrong one backs out first.
    if view.cursor ~= target_cursor then
        pulse(bot, s, "cancel")
        return false
    end
    local box_row, dir = BotInventory.box_target(view.items, view.box_cursor, function(r)
        if op.kind == "deposit" then
            return r.item == 0
        end
        return r.item == op.item
    end)
    if not box_row then
        log(bot, string.format("%s 0x%02X: no box slot for it", op.kind, op.item))
        pulse(bot, s, "cancel")
        return true
    end
    if dir == 0 then
        pulse(bot, s, "confirm")
    else
        pulse(bot, s, dir > 0 and "down" or "up")
    end
    return false
end

-- Record a box visit's end for the planner's box goal.
local function end_visit(bot, s)
    s.visit = nil
    s.done_tick = bot:tick()
end

-- One tick before the scene handler: the item box screen, the lid before
-- it and the close after it own the pad. Returns true when it consumed the
-- tick.
function BotInventory.screen_step(bot)
    local s = state()
    local menu = bot:menu()
    local view = menu.box
    local tick = bot:tick()
    if view and menu.screen == "item_box" then
        remember_box(bot, view)
        if not s.visit then
            s.visit = { start = tick, ops = nil, index = 1 }
            s.expect = nil
        end
        local visit = s.visit
        if tick - visit.start < OPEN_SETTLE then
            return true
        end
        if not visit.ops then
            visit.ops = plan_visit(bot, menu, view)
        end
        local op = visit.ops[visit.index]
        if op and tick - visit.start < SESSION_TIMEOUT then
            if drive_op(bot, s, menu, view, op) then
                visit.index = visit.index + 1
            end
            return true
        end
        -- Done: cancel out (an armed slot first goes back to browse).
        visit.closing = true
        pulse(bot, s, "cancel")
        return true
    end
    if s.visit then
        -- The box is gone: wait out the close fade and the freeze.
        local visit = s.visit
        visit.settle = (visit.settle or 0) + 1
        local scene = bot:scene()
        -- The lid closes after the screen does, under a freeze: wait until
        -- the player has been controllable for a moment, or a press meant
        -- for the next goal reopens the box.
        if scene.controllable and not (menu.open or menu.requested) then
            visit.free = (visit.free or 0) + 1
        else
            visit.free = 0
        end
        if visit.free > CLEAR_TICKS or visit.settle > SETTLE_TIMEOUT then
            end_visit(bot, s)
            return false
        end
        return true
    end
    -- Between the box press and the screen: the lid animates.
    if s.expect and tick - s.expect < LID_WAIT then
        if menu.open or bot:scene().frozen or not bot:scene().controllable then
            return true
        end
    end
    return false
end

-- The planner pressed an item box zone: hold off the scene handler until
-- the screen shows (or LID_WAIT passes).
function BotInventory.expect_box(bot)
    state().expect = bot:tick()
end

-- Whether a box visit ended at or after `tick`.
function BotInventory.visited_since(tick)
    local done = state().done_tick
    return done ~= nil and done >= tick
end
