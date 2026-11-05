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
--   deposit:  only when the inventory is at its limit and a known pickup
--             (a frontier item that merges into no held stack) needs room;
--             then every item the keep rule does not hold goes in.
--   look:     sourcing ammunition (BotSurvival.sourcing) before any box
--             was opened: one visit to read what the shared box holds;
--   withdraw: an item in the box that an open <use> fact needs, a key a
--             remembered locked door needs, the equipped gun's ammunition
--             below the reserve floor, or a heal item when none is held;
--             only into free slots.
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
        local ammo = bot:ammo_for(id)
        if ammo then
            ctx.ammo[ammo] = true
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
    for _, room in ipairs(bot:world().rooms) do
        for _, door in ipairs(room.doors) do
            if door.present and door.locked and not door.unlocked and not door.traversed then
                ctx.keys[door.key] = true
            end
        end
    end
    ctx.ammo_low = BotSurvival.ammo_need(bot)
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
    local n = 0
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

-- Whether to open an item box only to see what it holds: the bot is
-- sourcing ammunition (BotSurvival.sourcing), no box has ever been opened
-- (`box_items`, the remembered contents, is nil) and a slot is free for
-- what it may hold. Every box shares one store, so one look covers them all.
function BotInventory.look_in_box(ctx, box_items, sourcing)
    return sourcing == true and box_items == nil and ctx.used < ctx.capacity
end

-- Why the bot should visit an item box now, or nil: a deposit (full, a
-- pickup needs room and something may go in) or a withdrawal (the box
-- holds an item the bot needs and a slot is, or can be made, free).
function BotInventory.box_reason(bot, skip)
    local ctx = BotInventory.context(bot)
    local items = bot:menu().items or {}
    local deposits = BotInventory.deposits(items, ctx)
    if ctx.full and #deposits > 0 and BotInventory.pickups_needing_room(bot, skip) > 0 then
        return "deposit: the inventory is full and a pickup needs room"
    end
    if BotInventory.look_in_box(ctx, bot:memory().box_items, BotSurvival.sourcing_now(bot)) then
        return "look in the box: sourcing ammunition and the box was never opened"
    end
    if ctx.used < ctx.capacity or #deposits > 0 then
        for item, _ in pairs(BotInventory.remembered(bot)) do
            local why = BotInventory.wanted_back({ item = item, kind = bot:item_kind(item) }, ctx)
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
    if ctx.full and BotInventory.pickups_needing_room(bot) > 0 then
        for _, row in ipairs(BotInventory.deposits(menu.items, ctx)) do
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
