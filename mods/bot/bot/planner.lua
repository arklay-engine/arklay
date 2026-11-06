-- The generic goal planner for the player bot.
--
-- Each tick that the scene handler, the menu skills, healing and combat do
-- not own, the planner runs the current goal, or picks one, highest priority
-- first:
--   1. room puzzle: the next <interact> of the room's bot.xml whose done
--      flag is clear (walk to its AOT zone, press, wait for the flag);
--      then a <use> fact that applies now (the item is held, the done flag
--      clear and the use-item probe live): stand where the probe arms the
--      item, USE it through the menu, wait for the done flag;
--      then inventory room (BotInventory): with the inventory at its limit
--      and a known pickup needing a slot, a MOVE that empties one; then the
--      nearest item box here (BotPlanner.nearest) when a deposit, the slot
--      reserve, a first look or a withdrawal is due (BotInventory.box_reason
--      with `here`);
--   2. ground items in this room, keys first, then the nearest approach:
--      a visible item here is taken before the bot moves on, unless the
--      inventory is full and it merges into no held stack. A second zone
--      of an item just taken here gets one approach. A drawer (a desk zone)
--      counts as its item: pressed and answered Yes until the item is
--      taken; a drawer whose lock bit is clear waits, marked, until the
--      desk key or the lockpick is held, and an empty one is skipped;
--   3. a passage check when a goal of this room has no reachable approach
--      from here: a `door_check` stair zone, or an action-key event zone no
--      fact names (a script may start the climb from an event), each once
--      per room entry, done when the player warps;
--   4. the way out. A pursued target (4a) keeps its course: the first door
--      of the route to it, until it is reached, real progress re-ranks or
--      the loop rule drops it. Otherwise rank by distance cost (lowest
--      first) over the doors here, the first door of the route to every
--      remote frontier goal, and to every room whose <use> fact
--      (bot:use_facts()) needs a held item:
--        cost = path length to the door's approach (UNPLANNED_COST when the
--               path planner reaches none)
--             + DOOR_COST per door step of the route (0 for a door here)
--             + for a remote goal, its path length in its own room from where
--               the bot stood there last (UNPLANNED_COST if none was found)
--             - UNVISITED_BONUS for a door into a room not yet entered
--             - ITEM_BONUS for a route to a remote frontier item
--             - KEY_BONUS when that item is a key a remembered locked
--               door needs, or AMMO_BONUS when it is the ammunition the
--               equipped gun needs (BotSurvival.ammo_need); KEY_BONUS is
--               the larger, so a needed key comes before ammunition and
--               herbs (BotPlanner.item_bonus)
--             - SOURCE_BONUS for a door into a room not yet entered, and
--             + MONSTER_COST per live monster last seen in each room the
--               route enters, both only while sourcing ammunition
--               (BotSurvival.sourcing: under the floor and no ammunition
--               the planner can reach, a pickup it has not given up on or
--               the remembered box)
--             - USE_BONUS for a route to a held item's <use> room
--             - BOX_BONUS for a route to a remembered item box when a box
--               visit is due (BotInventory.box_reason); a remote box's whole
--               cost is its walking distance over the mental map instead
--               (BotPlanner.pick_remote_box: exit path, then each room's leg
--               from the arrival point, MAP_DOOR per door load)
--             - for a door into a room not yet entered, the bonus of an item
--               a <holds> fact places there that the bot lacks and needs
--               (BotPlanner.holds_bonus)
--             - GOAL_BONUS for a door into the --bot-goal-room, or
--               STAGE_BONUS for one into the goal room's stage;
--      ties go to the doors here, then frontier order. A locked door counts
--      only with the key it needs. The remote winner becomes the pursued
--      target. An exit with no planned approach from here gets a passage
--      check first;
--   5. nothing left: approach and time-out marks under MAX_ATTEMPTS get one
--      more try; then finish with the reason.
-- A goal that fails is marked with the inventory it failed with; it is tried
-- again (at most MAX_ATTEMPTS times) once the inventory changes. A <use>
-- that failed in its room is retried there, never travelled to.
--
-- The loop rule. Real progress is a change of the rooms entered, the items
-- taken, the doors unlocked or the inventory, or a puzzle step done. Without
-- it, a room entered more than MAX_REPEATS times marks the pursued target
-- (or the last exit taken from the room) looping, and more than twice that
-- finishes the run; a pursued target whose route from a newly entered room
-- is no shorter than the shortest seen MAX_STALLS times is marked looping.
-- A looping mark holds for LOOP_HOLD or until the inventory changes. The
-- limits are three times what the re1 bench run reaching 20D needs (at most
-- 2 entries of one room between progress, 1 stall).
-- The goal room, if any, comes from --bot-goal-room. Every room fact comes
-- from the world model and the rooms' bot.xml; this module holds no room
-- literals.

BotPlanner = BotPlanner or {}

local C = BotConst

local MAX_ATTEMPTS = 3
local GOAL_TIMEOUT = 40 * C.SECOND
-- After a goal is done here, a <use> fact of this room whose item is held
-- but whose probe is not live yet holds the next goal choice (and so the way
-- out and the finish) this long: the step's room event may still be arming
-- the probe.
local USE_ARM_TICKS = 3 * C.SECOND
-- A <fight> step holds the room this long before it gives up: a boss takes
-- many clips (40C's Plant 42).
local FIGHT_TIMEOUT = 5 * 60 * C.SECOND
-- A run with no new progress (a room, an item, a flag fact) for this long
-- is stuck.
local STUCK_TICKS = 5 * 60 * C.SECOND
-- The loop rule (see the header). A pursued remote target whose route from
-- a newly entered room is no shorter than the shortest seen counts a stall;
-- this many stalls mark it looping.
local MAX_STALLS = 3
-- A room entered more than this many times since the last real progress
-- marks the goal being pursued (or the last exit taken from the room) as
-- looping; more than twice as many finishes the run.
local MAX_REPEATS = 6
-- A goal marked looping is held off this long, or until the inventory
-- changes, whichever comes first.
local LOOP_HOLD = 3 * 60 * C.SECOND
-- A route to the ammunition the equipped gun needs (BotSurvival.ammo_need).
local AMMO_BONUS = 60 * C.REACH
-- A route to a room whose <use> fact needs a held item (bot:use_facts()).
local USE_BONUS = 60 * C.REACH
-- A route to a seen key item that a remembered locked door needs: more
-- than the ammunition bonus, so a needed key comes first.
local KEY_BONUS = 70 * C.REACH
-- A route for an item errand (<deliver>/<fetch>): more than a key, since
-- the room that wants the item may want it only for a while (a poisoned
-- partner who dies after some minutes).
local ERRAND_BONUS = 2 * KEY_BONUS
-- A route to a remembered item box room when a box visit is due.
local BOX_BONUS = 60 * C.REACH
-- The sort length of a goal the path planner reaches no approach to: last.
local UNPLANNED = 1 / 0
-- Ranks an unplanned stair check after every planned one, by distance.
local UNPLANNED_BASE = C.REACH * C.REACH
-- Way-out ranking (see the header), in path units. A door the path planner
-- reaches no approach to costs this much, so it ranks after every planned
-- one but still counts.
local UNPLANNED_COST = C.REACH * C.REACH * C.REACH
-- Each door step of a remote route: about one room crossing.
local DOOR_COST = 20 * C.REACH
-- The mental map's price of one door load, in walking units: the door film
-- and its fades take about 1.5 seconds (the bench's `doors` and `trans`
-- ticks over its room entries), which the run (about 200 units a tick)
-- would cover in this much distance.
local MAP_DOOR = 15 * C.REACH
-- A door to a room never entered.
local UNVISITED_BONUS = 15 * C.REACH
-- A route to a remote frontier item.
local ITEM_BONUS = 10 * C.REACH
-- A door into the --bot-goal-room, and one into the goal's stage. The goal
-- door is worth ten door steps: once its key is held it outranks every
-- other goal short of a long detour.
local GOAL_BONUS = 10 * DOOR_COST
local STAGE_BONUS = 6 * C.REACH
-- Sourcing ammunition (BotSurvival.sourcing: under the floor, none known):
-- a door into a room not yet entered is worth this much more, and each live
-- monster last seen in a room the route enters costs MONSTER_COST (at most
-- MONSTER_CAP of them a room).
local SOURCE_BONUS = 60 * C.REACH
local MONSTER_COST = DOOR_COST
local MONSTER_CAP = 3
-- The hurt rule: a pickup goal (an item or a drawer) that loses this share of the
-- maximum health, on ticks the planner drives (not a fight's), is dropped
-- and marked `hurt while trying`; the room's other pickups wait until the
-- inventory changes, and remote planning does not head back for it.
local HURT_SHARE = 0.25
-- How often the live monsters of this room are counted.
local MONSTER_EVERY = C.SECOND // 2
-- A door a room event turned into a message (BotPlanner.note_message) is
-- retried on a later entry into its room; remote planning heads back for it
-- only once nothing else is left, and not before it has waited this long
-- (doubled per attempt): the event may clear with time, on a room entry.
local EVENT_WAIT = 60 * C.SECOND
-- An action-key event pressed as a passage gets this long.
local PASSAGE_TIMEOUT = 15 * C.SECOND

local function plan()
    BOT_STATE.plan = BOT_STATE.plan
        or { marks = {}, stairs = {}, locked = {}, entries = {}, lens = {}, progress = 0 }
    BOT_STATE.plan.lens = BOT_STATE.plan.lens or {}
    -- The mental map's arrival points: arrivals[room][from] = {x, z}, where
    -- the bot stood on entering `room` from `from`.
    BOT_STATE.plan.arrivals = BOT_STATE.plan.arrivals or {}
    -- The mental map's measured legs: legs[room][from][slot] is the planned
    -- path length to that door from where the bot arrived in `room` from
    -- `from` (UNPLANNED_COST when the path planner reached no approach, as
    -- on another level of the room), so remote routes price it as the bot
    -- would there.
    BOT_STATE.plan.legs = BOT_STATE.plan.legs or {}
    -- The hurt rule: room -> the inventory signature it hurt the bot with.
    BOT_STATE.plan.hurt = BOT_STATE.plan.hurt or {}
    BOT_STATE.plan.since = BOT_STATE.plan.since or {}
    BOT_STATE.plan.exits = BOT_STATE.plan.exits or {}
    BOT_STATE.plan.monsters = BOT_STATE.plan.monsters or {}
    return BOT_STATE.plan
end

-- The inventory as a string: a failed goal is retried once this changes.
local function inventory_sig(bot)
    local ids = {}
    for _, row in ipairs(bot:inventory()) do
        if row.item ~= 0 and row.quantity > 0 then
            ids[#ids + 1] = string.format("%02X", row.item)
        end
    end
    table.sort(ids)
    local flag = C.LOCKPICK_FLAG
    return table.concat(ids, ",") .. (bot:flag(flag.bank, flag.bit) and "+" or "")
end

local function mark_key(kind, room, slot)
    return kind .. ":" .. room .. ":" .. tostring(slot)
end

local function mark(bot, kind, room, slot, reason, permanent)
    local p = plan()
    local key = mark_key(kind, room, slot)
    local m = p.marks[key] or { attempts = 0 }
    m.attempts = m.attempts + 1
    m.reason = reason
    m.sig = inventory_sig(bot)
    m.permanent = permanent or false
    m.entry = (p.entries or {})[room]
    m.kind, m.room, m.slot = kind, room, slot
    p.marks[key] = m
    bot:log(string.format("mark %s: %s (attempt %d)", key, reason, m.attempts))
end

-- Whether a marked goal is still off limits. A goal is tried again once
-- the inventory changes; a goal that failed on its approach is also tried
-- again on a later entry into its room (the bot may now stand in a part of
-- the room that reaches it), but `remote` planning from another room does
-- not head back for it.
local function blocked(bot, kind, room, slot, remote)
    local m = plan().marks[mark_key(kind, room, slot)]
    if not m then
        return false
    end
    if m.desk then
        -- A locked drawer: off limits, from this room or any other, until
        -- its lock bit is set or the desk key or the lockpick is held.
        if BotPlanner.desk_blocker(bot, m.desk) then
            return true
        end
        plan().marks[mark_key(kind, room, slot)] = nil
        return false
    end
    if m.event then
        -- A door a room event made a message: off limits on the entry it
        -- failed on; from another room until the empty-frontier retry
        -- forgives it (forgive_events).
        if remote then
            return not m.event.forgiven
        end
        return (plan().entries or {})[room] == m.event.entry
    end
    if m.gate then
        -- A gated <avoid>: off limits exactly while its flag says so, from
        -- this room or any other.
        if BotPuzzle.gate_holds(bot, m.gate) then
            return true
        end
        plan().marks[mark_key(kind, room, slot)] = nil
        return false
    end
    if m.permanent or m.attempts >= MAX_ATTEMPTS then
        return true
    end
    if m.hold and bot:tick() >= m.hold then
        return false
    end
    return BotPlanner.mark_holds(m, inventory_sig(bot), (plan().entries or {})[room], remote)
end

-- Whether a plain mark `m` (no desk, gate, permanent flag or hold) still
-- holds, given the inventory signature `sig` and this entry count of its
-- room. A mark lifts once the inventory changes; a failed approach also
-- lifts on a later entry into its room. Remote planning never heads back
-- for an item whose approach failed, even after the inventory changed: a
-- new item does not make an unreachable zone reachable. (A door keeps the
-- inventory rule: route() reads door marks too.)
function BotPlanner.mark_holds(m, sig, entry, remote)
    local approach = m.reason:find("approach", 1, true) ~= nil
    local hurt = m.reason:find("hurt while trying", 1, true) ~= nil
    if remote and (approach or hurt) and m.kind == "item" then
        return true
    end
    if m.sig ~= sig then
        return false
    end
    return remote or not approach or m.entry == entry
end

-- Whether the planner has given up on the frontier item `goal` for a
-- reason other than a full inventory (the inventory rules ask which pickups
-- still need a free slot).
local function gave_up(goal)
    local m = plan().marks[mark_key("item", goal.room, goal.slot)]
    if not m or m.reason:find("inventory full", 1, true) then
        return false
    end
    -- A locked drawer needs no slot until a key or lockpick lifts its
    -- mark (blocked() drops it then).
    if m.desk then
        return true
    end
    return m.permanent or m.attempts >= MAX_ATTEMPTS
end

-- Unreachable marks of this room are stale after a warp (a stair climb).
local function clear_unreachable(bot, room)
    for key, m in pairs(plan().marks) do
        if m.room == room and m.reason:find("approach", 1, true) then
            plan().marks[key] = nil
        end
    end
end

local function progress(bot, note)
    local p = plan()
    p.progress = bot:tick()
    if note then
        bot:log("progress: " .. note)
    end
end

-- What counts as real progress: rooms entered, items taken, doors
-- unlocked or newly there (a room event that turns a zone into a door, as
-- 411's lock switch does for 40E's door to 410) and the inventory. Walking
-- between known rooms changes none.
local function progress_sig(bot, world)
    local visited, taken, unlocked = 0, 0, 0
    for _, r in ipairs(world.rooms) do
        if r.visited then
            visited = visited + 1
        end
        for _, door in ipairs(r.doors) do
            if door.unlocked then
                unlocked = unlocked + 1
            end
            if door.present and door.target then
                unlocked = unlocked + 1
            end
        end
        for _, item in ipairs(r.items) do
            if item.taken then
                taken = taken + 1
            end
        end
    end
    return string.format("%d/%d/%d/%s", visited, taken, unlocked, inventory_sig(bot))
end

-- Real progress: the room repeat counts and the pursued target start over.
local function real_progress(bot, note)
    local p = plan()
    p.since = {}
    if p.target then
        bot:log(string.format("progress (%s): target %s re-ranked", note, p.target.label))
        p.target = nil
    end
end

-- The loop rule for one pursued target, `rec = {best, stalls}`, entered a
-- room whose route to it takes `steps` doors: a route no shorter than the
-- shortest seen is a stall. Returns true once the target is looping.
function BotPlanner.pursue(rec, steps)
    if rec.best == nil or steps < rec.best then
        rec.best = steps
    else
        rec.stalls = (rec.stalls or 0) + 1
    end
    return (rec.stalls or 0) >= MAX_STALLS
end

-- The room repeat rule: count one more entry of `room` since the last real
-- progress in `counts`. Returns "ok", "loop" (mark the pursued goal) or
-- "finish", and the count.
function BotPlanner.repeat_check(counts, room)
    counts[room] = (counts[room] or 0) + 1
    local n = counts[room]
    if n > 2 * MAX_REPEATS then
        return "finish", n
    end
    if n > MAX_REPEATS then
        return "loop", n
    end
    return "ok", n
end

-- Mark a goal looping: held off for LOOP_HOLD or until the inventory
-- changes, and counted as an attempt.
local function mark_loop(bot, kind, room, slot, why)
    mark(bot, kind, room, slot, "looping: " .. why)
    plan().marks[mark_key(kind, room, slot)].hold = bot:tick() + LOOP_HOLD
end

local function has_lockpick(bot)
    local flag = C.LOCKPICK_FLAG
    return bot:has_item(C.LOCKPICK) or bot:flag(flag.bank, flag.bit)
end

-- Why the drawer `desk` (a bot:actions() desk row) cannot be opened now, or
-- nil when a press opens it. check_desk opens a drawer whose bank-2 lock
-- bit is set; a clear one needs the desk key or the lockpick (the press
-- then asks to use it, and a second press opens the drawer). An empty
-- drawer refuses the press, so it is no goal.
function BotPlanner.desk_blocker(bot, desk)
    if desk.full == false then
        return "the drawer is empty"
    end
    if bot:flag(desk.lock_bank, desk.lock_bit) then
        return nil
    end
    if bot:has_item(C.DESK_KEY) or has_lockpick(bot) then
        return nil
    end
    return string.format("drawer locked: no desk key 0x%02X or lockpick", C.DESK_KEY)
end

-- The drawers of the action list, by the slot of the item each holds.
function BotPlanner.drawers(actions)
    local by_item = {}
    for _, action in ipairs(actions) do
        if action.kind == "desk" and action.item_slot ~= nil then
            by_item[action.item_slot] = action
        end
    end
    return by_item
end

-- Whether a door with key byte `key` can be opened now when it is locked,
-- and why not. Special keys (see the README's door rules): KEY_FAR_SIDE
-- opens on a press, KEY_NEVER never, KEY_NONE only by a room event.
local function key_opens(bot, key)
    if key == C.KEY_FAR_SIDE then
        return true
    end
    if key == C.KEY_NEVER then
        return false, "locked for good", true
    end
    if key == C.KEY_NONE then
        return false, "locked by a room event"
    end
    if key == C.SWORD_KEY and has_lockpick(bot) then
        return true
    end
    if bot:has_item(key) then
        return true
    end
    return false, string.format("needs key 0x%02X", key)
end

-- A live door's current lock state.
local function live_locked(bot, door)
    local lock = door.lock or 0
    return lock >= C.LOCK_LOCKED and not bot:flag(C.LOCK_BANK, lock % C.LOCK_SPAN)
end

local function world_room(world, id)
    for _, room in ipairs(world.rooms) do
        if room.id == id then
            return room
        end
    end
    return nil
end

-- Whether a remembered door (a world row) can be walked through.
-- The <door slot via> fact of door `slot` in room `room_id`, or nil: a door
-- the room parks at its origin until AOT `via` is pressed.
local function via_fact(bot, room_id, slot)
    for _, fact in ipairs(bot:door_facts()) do
        if fact.room == room_id and fact.slot == slot then
            return fact
        end
    end
    return nil
end

-- The live action the bot presses to open this room's `door`, when the door
-- is parked at the origin and a <door via> fact names it (406's hatch).
local function via_action(bot, door)
    if not BotNav.is_origin_zone(door) then
        return nil
    end
    local fact = via_fact(bot, bot:room().id, door.slot)
    return fact and BotNav.action_by_slot(bot, fact.via)
end

local function door_passable(bot, room_id, door)
    if not door.present or not door.target then
        return false
    end
    -- A door zone parked at the room origin is a scripted transfer (a
    -- cutscene's exit), not a door to walk through, unless a <door via>
    -- fact names the action that sets it around the player.
    if door.x == 0 and door.z == 0 and not via_fact(bot, room_id, door.slot) then
        return false
    end
    if blocked(bot, "door", room_id, door.slot, true) then
        return false
    end
    if door.locked and not door.unlocked then
        return (key_opens(bot, door.key))
    end
    return true
end

-- The remembered zone centre {x, z} of action `slot` in room `id`, or nil.
function BotPlanner.action_point(world, id, slot)
    for _, row in ipairs(world.rooms or {}) do
        if row.id == id then
            for _, action in ipairs(row.actions or {}) do
                if action.slot == slot and action.x then
                    return { x = action.x, z = action.z }
                end
            end
        end
    end
    return nil
end

-- The mental map. Every room the bot has seen is remembered with its doors
-- (zone centre x, z and target) and its actions (item boxes with their zone
-- centre), in bot:world(). A route is priced by walking distance through
-- it: from the bot to a door here (`opts.exit_len(slot)`, the planned path
-- length), MAP_DOOR for each door load, then in each room from where the bot
-- arrives to the next door, and last to the goal's point. The arrival point
-- in room T from room R is where the bot last stood on entering T from R
-- (`opts.arrivals[T][R]`), else T's door back to R. Remote legs are straight
-- lines; nothing is learned from earlier choices.

local function map_dist(ax, az, bx, bz)
    local dx, dz = ax - bx, az - bz
    return (dx * dx + dz * dz) ^ 0.5 // 1
end

-- The point where the bot arrives in room row `row` from room `from_id`, or
-- nil when the map holds none.
function BotPlanner.entry_point(row, from_id, arrivals)
    local seen = arrivals and arrivals[row.id] and arrivals[row.id][from_id]
    if seen then
        return seen.x, seen.z
    end
    for _, door in ipairs(row.doors or {}) do
        if door.target == from_id and door.present ~= false then
            return door.x, door.z
        end
    end
    return nil
end

-- Dijkstra from the bot in room `from` over the map. A node is a room
-- entered from a given room (its arrival point). Returns the tree that
-- BotPlanner.tree_route reads. `opts.passable(room_id, door)` filters the
-- doors; `opts.exit_len(slot)` prices the doors of `from`.
function BotPlanner.map_tree(world, from, opts)
    local rows = {}
    for _, row in ipairs(world.rooms or {}) do
        rows[row.id] = row
    end
    local nodes = { { room = from, cost = 0 } }
    local best = { [from .. "|"] = 1 }
    local done = {}
    while true do
        local pick = nil
        for i, node in ipairs(nodes) do
            if not done[i] and (not pick or node.cost < nodes[pick].cost) then
                pick = i
            end
        end
        if not pick then
            break
        end
        done[pick] = true
        local node = nodes[pick]
        local row = rows[node.room]
        for _, door in ipairs(row and row.doors or {}) do
            local target = door.target
            if target and target ~= node.room and door.x and opts.passable(node.room, door) then
                local leg
                local from_room = node.step and node.step.room
                local legs = opts.legs and from_room and opts.legs[node.room]
                    and opts.legs[node.room][from_room]
                if node.prev == nil then
                    leg = opts.exit_len(door.slot)
                elseif legs and legs[door.slot] then
                    leg = legs[door.slot]
                elseif node.x then
                    leg = map_dist(node.x, node.z, door.x, door.z)
                else
                    leg = 0
                end
                local cost = node.cost + leg + MAP_DOOR
                local key = target .. "|" .. node.room
                local seen = best[key]
                if not seen or (not done[seen] and cost < nodes[seen].cost) then
                    local x, z = nil, nil
                    if rows[target] then
                        x, z = BotPlanner.entry_point(rows[target], node.room, opts.arrivals)
                    end
                    local next_node = { room = target, cost = cost, x = x, z = z, prev = pick,
                        step = { room = node.room, slot = door.slot, target = target } }
                    if seen then
                        nodes[seen] = next_node
                    else
                        nodes[#nodes + 1] = next_node
                        best[key] = #nodes
                    end
                end
            end
        end
    end
    return { nodes = nodes, from = from }
end

-- The cheapest route in `tree` into room `to`, ending at `point` ({x, z},
-- or nil for the arrival): its cost and its {room, slot, target} steps, or
-- nil when the map reaches no entry of `to`.
function BotPlanner.tree_route(tree, to, point)
    local best, best_cost = nil, nil
    for i, node in ipairs(tree.nodes) do
        if node.room == to and node.prev ~= nil then
            local cost = node.cost
            if point and point.x and node.x then
                cost = cost + map_dist(node.x, node.z, point.x, point.z)
            end
            if not best_cost or cost < best_cost then
                best, best_cost = i, cost
            end
        end
    end
    if not best then
        return nil
    end
    local steps = {}
    local i = best
    while tree.nodes[i].prev ~= nil do
        table.insert(steps, 1, tree.nodes[i].step)
        i = tree.nodes[i].prev
    end
    return best_cost, steps
end

-- map_tree's options for the bot: the doors it may walk through, the
-- arrival points it remembers, and `exit_len(slot)` for the doors here
-- (default: the straight line from where it stands).
local function map_opts(bot, exit_len)
    return {
        passable = function(id, door) return door_passable(bot, id, door) end,
        exit_len = exit_len or function(slot)
            local door = BotNav.door_by_slot(bot, slot)
            return door and bot:distance(BotNav.centre(door)) or UNPLANNED_COST
        end,
        arrivals = plan().arrivals,
        legs = plan().legs,
    }
end

-- The route through the mental map from room `from` (the bot's room) into
-- room `to`, ending at `point` (a {x, z} row, or nil): the list of
-- {room, slot, target} steps and its walking cost, or nil. `tree` is a
-- map_tree from `from` to reuse.
local function map_route(bot, world, from, to, tree, point)
    if from == to then
        return {}, 0
    end
    tree = tree or BotPlanner.map_tree(world, from, map_opts(bot))
    local cost, steps = BotPlanner.tree_route(tree, to, point)
    return steps, cost
end

-- Breadth-first route over the remembered doors: the list of
-- {room, slot, target} steps from `from` to `to`, or nil.
local function route(bot, world, from, to)
    if from == to then
        return {}
    end
    local prev = { [from] = false }
    local queue = { from }
    local head = 1
    while queue[head] do
        local id = queue[head]
        head = head + 1
        local room = world_room(world, id)
        for _, door in ipairs(room and room.doors or {}) do
            local target = door.target
            if target and prev[target] == nil and door_passable(bot, id, door) then
                prev[target] = { room = id, slot = door.slot, target = target }
                if target == to then
                    local steps = {}
                    local step = prev[target]
                    while step do
                        table.insert(steps, 1, step)
                        step = prev[step.room]
                    end
                    return steps
                end
                queue[#queue + 1] = target
            end
        end
    end
    return nil
end

-- Goal constructors. Each goal has a kind, a label and a press job.
local function zone_of(row)
    return { x = row.x, z = row.z, width = row.width, depth = row.depth }
end

local function camera_goal(bot, fact, index, count)
    local label = string.format("step %d/%d camera %d (%s)", index, count,
        fact.cut, fact.label or "unnamed")
    return {
        kind = "camera", slot = "camera" .. fact.cut, label = label,
        job = {
            camera = fact.cut, label = label,
            done = function(b, job)
                if fact.bank then
                    return b:flag(fact.bank, fact.bit)
                end
                if job.reached then
                    BotPuzzle.visited(index)
                    return true
                end
                return false
            end,
        },
    }
end

local function interact_goal(bot, fact, index, count)
    if fact.kind == "camera" then
        return camera_goal(bot, fact, index, count)
    end
    if fact.kind == "fight" then
        local label = string.format("fight %d/%d enemy 0x%02X (%s)", index, count,
            fact.enemy, fact.label or "unnamed")
        return {
            kind = "interact", slot = "fight" .. fact.enemy, label = label,
            job = {
                fight = true, enemy = fact.enemy, label = label,
                timeout = FIGHT_TIMEOUT,
                done = function(b) return BotPuzzle.flag_done(b, fact) end,
            },
        }
    end
    local action = BotNav.action_by_slot(bot, fact.slot)
    if not action then
        return nil, string.format("interact slot %d is not a live action", fact.slot)
    end
    if fact.kind == "climb" then
        local label = string.format("climb %d/%d object %d to slot %d (%s)", index, count,
            fact.object, fact.slot, fact.label or "unnamed")
        return {
            kind = "interact", slot = fact.slot, label = label,
            job = {
                climb = true, object = fact.object, zone = zone_of(action), label = label,
                taken = function(b) return BotPuzzle.flag_done(b, fact) end,
            },
        }
    end
    if fact.kind == "push" then
        local label = string.format("push %d/%d object %d onto slot %d (%s)", index, count,
            fact.object, fact.slot, fact.label or "unnamed")
        return {
            kind = "interact", slot = fact.slot, label = label,
            job = {
                push = true, object = fact.object, zone = zone_of(action), label = label,
                dir = BotPuzzle.push_dir(fact), only_axis = fact.axis, step = index,
                done = function(b) return BotPuzzle.push_done(b, fact) end,
            },
        }
    end
    local label = string.format("interact %d/%d slot %d (%s)", index, count,
        fact.slot, fact.label or "unnamed")
    -- A body-probe zone fires on the player's position, not the reach point
    -- (40C's fireplace, SAT_UNDER): stand inside it.
    local zone = zone_of(action)
    zone.inside = (action.flags // C.PROBE_BODY) % 2 == 1
    return {
        kind = "interact", slot = fact.slot, label = label,
        job = {
            zone = zone, label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND,
            done = function(b) return BotPuzzle.flag_done(b, fact) end,
        },
    }
end

-- A <deliver> here with its item held: walk into its camera cut; the room
-- script takes the item there and sets the done flag.
local function deliver_goal(bot, fact)
    local label = string.format("deliver 0x%02X into camera %d (%s)", fact.item, fact.cut,
        fact.label or "unnamed")
    return {
        kind = "deliver", slot = "camera" .. fact.cut, label = label,
        job = {
            camera = fact.cut, label = label,
            done = function(b) return b:flag(fact.bank, fact.bit) end,
        },
    }
end

-- A <fetch> here that a wanted <deliver> needs: press its zone until the
-- item is held (or the offer is gone).
local function fetch_goal(bot, fact)
    local action = BotNav.action_by_slot(bot, fact.slot)
    if not action then
        return nil, string.format("fetch slot %d is not a live action", fact.slot)
    end
    local label = string.format("fetch 0x%02X at slot %d (%s)", fact.item, fact.slot,
        fact.label or "unnamed")
    return {
        kind = "fetch", slot = fact.slot, label = label,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND,
            done = function(b)
                return b:has_item(fact.item) or not BotPuzzle.fetch_offered(b, fact)
            end,
        },
    }
end

-- How long a used item may take to show its done flag once the menu closed:
-- the room event (a scene) runs in between.
local USE_SETTLE = 20 * C.SECOND

-- A <use> fact: stand where the room's use-item probe in `action` arms the
-- item (the reach point in the zone, or the player inside it for a body
-- probe), wait for the item's use flag, USE it through the menu, then wait
-- for the done flag while the room event plays.
local function use_goal(bot, fact, action)
    local label = string.format("use 0x%02X at slot %d (%s)", fact.item, fact.slot,
        fact.label or "unnamed")
    local zone = zone_of(action)
    zone.inside = (action.flags // C.PROBE_BODY) % 2 == 1
    local armed = fact.item - C.ITEM_USE_BASE
    return {
        kind = "use", slot = fact.slot, label = label,
        job = {
            zone = zone, label = label, pattern = "hold",
            side = 3 * C.SECOND, timeout = GOAL_TIMEOUT,
            done = function(b) return b:flag(fact.bank, fact.bit) end,
            hold = function(b, job)
                if not job.used and b:flag(C.ITEM_USE_BANK, armed)
                    and BotMenu.use_item(b, fact.item, label) then
                    job.used = b:tick()
                    job.settled = true
                end
            end,
            check = function(b, job)
                if not job.used or BotMenu.busy() then
                    return nil
                end
                local last = BotMenu.last()
                if last and last.item == fact.item and last.tick >= job.used and not last.ok then
                    return "menu use failed: " .. tostring(last.reason)
                end
                job.waited = (job.waited or 0) + 1
                if job.waited > USE_SETTLE then
                    return "used; the done flag stayed clear"
                end
                return nil
            end,
        },
    }
end

-- An item box visit: press its zone; BotInventory drives the box screen
-- the press opens, and the goal is done once that visit has ended.
local function box_goal(bot, action, why)
    local label = string.format("item box slot %d (%s)", action.slot, why)
    local look = bot:memory().box_items == nil
    local since = bot:tick()
    return {
        kind = "box", slot = action.slot, label = label, sig = inventory_sig(bot), look = look,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND, timeout = GOAL_TIMEOUT,
            done = function() return BotInventory.visited_since(since) end,
        },
    }
end

-- A MOVE through the menu that empties a slot (BotInventory.combine_now).
local function combine_goal(bot, pick)
    local label = string.format("combine 0x%02X onto 0x%02X (menu slots %d, %d)",
        pick.item, pick.other, pick.slot, pick.target)
    return {
        kind = "combine", slot = "mix", label = label,
        job = { menu = true, label = label, pick = pick },
    }
end

-- One tick of a combine goal: start the menu job, then read its outcome.
local function combine_step(bot, job)
    if not job.started then
        if not BotMenu.combine(bot, job.pick.slot, job.pick.target, job.label) then
            job.reason = "the menu job did not start"
            return "failed"
        end
        job.started = bot:tick()
        return "busy"
    end
    if BotMenu.busy() then
        return "busy"
    end
    local last = BotMenu.last()
    if last and last.kind == "combine" and last.tick >= job.started then
        if last.ok then
            return "done"
        end
        job.reason = "combine failed: " .. tostring(last.reason)
        return "failed"
    end
    job.reason = "the combine job left no result"
    return "failed"
end

-- `twin`: the slot of a taken item of the same id whose zone centre this
-- zone holds. It may be a second AOT of that pickup, so it gets one
-- approach: nothing there marks it for good.
local function item_goal(bot, action, twin)
    local before = bot:item_count(action.item)
    local label = string.format("item 0x%02X slot %d", action.item, action.slot)
    return {
        kind = "item", slot = action.slot, label = label, item = action.item,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 3, gap = 15, side = 2 * C.SECOND,
            check = twin and function(b, job)
                if job.index > 1 and job.pressed then
                    job.permanent = true
                    return string.format("a second zone of the item taken at slot %d", twin)
                end
                return nil
            end or nil,
            done = function(b, job)
                if b:item_count(action.item) > before then
                    return true
                end
                local now = BotNav.action_by_slot(b, action.slot)
                return job.pressed and (not now or now.kind ~= "item"
                    or now.item ~= action.item)
            end,
        },
    }
end

-- A drawer: press its zone, answer Yes to the key prompt (a locked drawer)
-- and to the take prompt (the scene handler answers both), until the item
-- is in the inventory or the drawer reads empty. Marks go on the item's
-- slot, so remote planning sees the same goal as the world model's item.
local function desk_goal(bot, desk)
    local item = desk.desk_item
    local before = bot:item_count(item)
    local label = string.format("drawer slot %d item 0x%02X slot %d", desk.slot, item,
        desk.item_slot)
    return {
        kind = "item", slot = desk.item_slot, press_slot = desk.slot, label = label,
        item = item,
        job = {
            -- The lid opens after a camera pan of about a second; a locked
            -- drawer needs one press for the key and one to open.
            zone = zone_of(desk), label = label, pattern = "edge",
            presses = 3, gap = 2 * C.SECOND, side = 8 * C.SECOND, timeout = GOAL_TIMEOUT,
            done = function(b, job)
                if b:item_count(item) > before then
                    b:log(string.format("%s: opened, took item 0x%02X (%d -> %d)", label, item,
                        before, b:item_count(item)))
                    return true
                end
                local now = BotNav.action_by_slot(b, desk.slot)
                if job.pressed and (not now or now.full == false) then
                    b:log(string.format("%s: opened, the drawer reads empty", label))
                    return true
                end
                return false
            end,
        },
    }
end

local function door_goal(bot, door, why)
    local label = string.format("door slot %d -> %s (%s)", door.slot,
        tostring(door.target), why)
    local via = via_action(bot, door)
    if via then
        -- Press the action that opens it; its event sets the door around
        -- the player (after a prompt the room's <answer> settles).
        local room_id = bot:room().id
        label = string.format("%s via slot %d", label, via.slot)
        return {
            kind = "door", slot = door.slot, target = door.target, label = label,
            was_locked = false,
            job = {
                zone = zone_of(via), label = label, pattern = "edge",
                presses = 2, gap = 20, side = 4 * C.SECOND, timeout = GOAL_TIMEOUT,
                done = function(b) return b:room().id ~= room_id end,
            },
        }
    end
    return {
        kind = "door", slot = door.slot, target = door.target, label = label,
        -- A locked door's press opens the key's message: not an event's.
        was_locked = live_locked(bot, door),
        job = {
            zone = zone_of(door), label = label, pattern = "door",
            side = 4 * C.SECOND - 10, timeout = GOAL_TIMEOUT,
        },
    }
end

local function stair_goal(bot, action)
    local label = string.format("%s check slot %d",
        action.kind == "event" and "passage" or "stair", action.slot)
    return {
        kind = "stair", slot = action.slot, label = label,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 40, side = 10 * C.SECOND,
            timeout = action.kind == "event" and PASSAGE_TIMEOUT or nil,
            -- The climb is a warp: one tick's jump after the press. A
            -- ladder (`stairs_zone`, started by the press or by leaning
            -- into it) carries the player over a few seconds instead: done
            -- once it has moved that far from the approach point and
            -- control is back.
            done = function(b, job)
                local q = b:player()
                local last = job.last_pos
                if job.pressed and last ~= nil
                    and BotNav.abs(q.x - last[1]) + BotNav.abs(q.z - last[2])
                        > C.WARP_DISTANCE then
                    return true
                end
                local at = job.arrived_at
                return at ~= nil and b:scene().controllable
                    and BotNav.abs(q.x - at[1]) + BotNav.abs(q.z - at[2]) > C.WARP_DISTANCE
            end,
        },
    }
end

-- A passage check here: a stair zone (`door_check`), a ladder
-- (`stairs_zone`), or an action-key
-- event zone no fact names (a room may start its stair climb from an
-- event, as when a script turns the stair zone on from the event). One try
-- per slot per entry, nearest first.
local function passage_goal(bot, room)
    local p = plan()
    p.stairs[room] = p.stairs[room] or {}
    local stair, stair_len = nil, nil
    for _, action in ipairs(bot:actions()) do
        local passage = action.kind == "door_check" or action.kind == "stairs_zone"
            or (action.kind == "event" and action.flags >= C.ACTION_KEY
                and not BotPuzzle.is_fact_slot(bot, action.slot))
        if passage and not p.stairs[room][action.slot]
            and not blocked(bot, "stair", room, action.slot)
            and not BotPuzzle.avoided(bot, action.slot) then
            local len = BotNav.reach_length(bot, action)
                or UNPLANNED_BASE + bot:distance(BotNav.centre(action))
            if not stair_len or len < stair_len then
                stair, stair_len = action, len
            end
        end
    end
    if stair then
        p.stairs[room][stair.slot] = true
        return stair_goal(bot, stair)
    end
    return nil
end

local function frontier_goal(world, target)
    for _, goal in ipairs(world.frontier) do
        if goal.kind == target.kind and goal.room == target.room and goal.slot == target.slot then
            return goal
        end
    end
    return nil
end

-- A pursued target's goal row while it is still worth pursuing, or nil: a
-- frontier door or item, or a <use> fact whose item is held and whose done
-- flag is clear.
local function target_goal(bot, world, t)
    if t.kind == "box" then
        if BotInventory.box_reason(bot, gave_up)
            and not blocked(bot, "box", t.room, t.slot, true) then
            return { kind = "box", room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "use" then
        if bot:has_item(t.item) and not bot:flag(t.bank, t.bit)
            and not blocked(bot, "use", t.room, t.slot, true) then
            return { kind = "use", room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "deliver" or t.kind == "fetch" then
        local fact = t.fact
        local live
        if t.kind == "deliver" then
            live = BotPuzzle.deliver_wanted(bot, fact) and bot:has_item(fact.item)
        else
            live = BotPuzzle.wanted_items(bot)[fact.item] and not bot:has_item(fact.item)
                and BotPuzzle.fetch_offered(bot, fact)
        end
        if live and not blocked(bot, t.kind, t.room, t.slot, true) then
            return { kind = t.kind, room = t.room, slot = t.slot }
        end
        return nil
    end
    return frontier_goal(world, t)
end

-- Every inventory slot holds an item.
local function inventory_full(bot)
    local used = 0
    for _, row in ipairs(bot:inventory()) do
        if row.item ~= 0 then
            used = used + 1
        end
    end
    return used >= (bot:menu().capacity or used + 1)
end

-- A pickup of `item` fits: a free slot, or a held stack it merges into.
local function pickup_fits(bot, item, full)
    if not full then
        return true
    end
    local _, merge = bot:item_kind(item)
    return merge and bot:has_item(item)
end

-- A live item zone that holds the centre of a zone of the same item the
-- bot took in this room is a second zone of that item (a pickup with two
-- AOTs): the slot of the taken one, or nil.
local function taken_twin(here, action)
    for _, item in ipairs(here and here.items or {}) do
        if item.taken and item.slot ~= action.slot and item.item == action.item
            and item.x >= action.x and item.x <= action.x + action.width
            and item.z >= action.z and item.z <= action.z + action.depth then
            return item.slot
        end
    end
    return nil
end

-- Is the door row `door` (a bot:doors() row) of this room untraversed?
local function untraversed(world_here, slot)
    for _, door in ipairs(world_here and world_here.doors or {}) do
        if door.slot == slot then
            return not door.traversed, door
        end
    end
    return true, nil
end

-- The way-out bonus of a route to the remote frontier item `item` (catalog
-- kind `kind`): ITEM_BONUS, plus KEY_BONUS for a key that a remembered
-- locked door needs (`needed[item]` names the door), else AMMO_BONUS for
-- the ammunition the equipped gun needs (`ammo`). Returns the bonus and a
-- label tag (nil for a plain item). A heal item gets the plain bonus.
function BotPlanner.item_bonus(item, kind, ammo, needed)
    if kind == "key" and needed and needed[item] then
        return ITEM_BONUS + KEY_BONUS, "key for " .. needed[item]
    end
    if ammo and item == ammo then
        return ITEM_BONUS + AMMO_BONUS, "ammunition"
    end
    return ITEM_BONUS, nil
end

-- The way-out bonus of a door into the unvisited room `id` for what its
-- <holds> facts (`holds`, bot:item_facts()) place there: the largest bonus
-- of an item the bot does not hold and still needs, one a <use> fact
-- (`uses`, bot:use_facts()) with its done flag clear needs
-- (ITEM_BONUS + USE_BONUS), or a needed key or the gun's ammunition
-- (BotPlanner.item_bonus). `held(item)` and `done(fact)` read the game.
-- Returns 0 and nil when nothing there is needed.
function BotPlanner.holds_bonus(id, holds, uses, held, done, kind_of, ammo, needed)
    local best, best_tag = 0, nil
    for _, fact in ipairs(holds) do
        if fact.room == id and not held(fact.item) then
            local bonus, tag = 0, nil
            for _, use in ipairs(uses) do
                if use.item == fact.item and not done(use) then
                    bonus, tag = ITEM_BONUS + USE_BONUS, "for " .. (use.label or "a use")
                end
            end
            if bonus == 0 then
                local extra, why = BotPlanner.item_bonus(fact.item, kind_of(fact.item), ammo, needed)
                if why then
                    bonus, tag = extra, why
                end
            end
            if bonus > best then
                best, best_tag = bonus, string.format("holds 0x%02X %s", fact.item, tag)
            end
        end
    end
    return best, best_tag
end

-- The row of `rows` with the shortest `len(row)` (nil: unplanned, after
-- every planned one), first in order on a tie, and its length.
function BotPlanner.nearest(rows, len)
    local best, best_len, best_sort = nil, nil, nil
    for _, row in ipairs(rows) do
        local n = len(row)
        local sort = n or UNPLANNED
        if not best or sort < best_sort then
            best, best_len, best_sort = row, n, sort
        end
    end
    return best, best_len
end

-- The remembered item box to head for from room `from`, over every other
-- room of `world`, by walking distance through the map
-- (BotPlanner.map_tree): returns {room, slot, cost, steps} or nil.
-- `opts` is map_tree's, plus `usable_box(room, action)`; `opts.tree`, when
-- given, is a map_tree already built from `from`.
function BotPlanner.pick_remote_box(world, from, opts)
    local tree = opts.tree or BotPlanner.map_tree(world, from, opts)
    local best = nil
    for _, r in ipairs(world.rooms) do
        for _, action in ipairs(r.id ~= from and r.actions or {}) do
            if action.kind == "item_box" and action.x and not (action.x == 0 and action.z == 0)
                and opts.usable_box(r.id, action) then
                local cost, steps = BotPlanner.tree_route(tree, r.id, action)
                if steps and steps[1] and (not best or cost < best.cost) then
                    best = { room = r.id, slot = action.slot, cost = cost, steps = steps }
                end
            end
        end
    end
    return best
end

-- The live item boxes of this room the planner may press.
function BotPlanner.boxes_here(bot)
    local room = bot:room().id
    local out = {}
    for _, action in ipairs(bot:actions()) do
        if action.kind == "item_box" and not BotNav.is_origin_zone(action)
            and not BotPuzzle.avoided(bot, action.slot)
            and not blocked(bot, "box", room, action.slot) then
            out[#out + 1] = action
        end
    end
    return out
end

-- Inventory work due in this room before a save: a MOVE that empties a
-- slot, or a box visit here. Returns why, or nil.
function BotPlanner.inventory_due_here(bot)
    if not blocked(bot, "combine", "inventory", "mix", true)
        and BotInventory.combine_now(bot, gave_up) then
        return "a combine is due"
    end
    if #BotPlanner.boxes_here(bot) > 0 then
        return BotInventory.box_reason(bot, gave_up, true)
    end
    return nil
end

-- Puzzle steps (<interact> and <use> goals) done this run.
function BotPlanner.steps_done()
    return plan().steps_done or 0
end

-- The room the pursued target's route enters next when a live monster was
-- last seen there, or nil (the cautious save rule's "risky room next").
function BotPlanner.risky_next(bot)
    local p = plan()
    local t = p.target
    if not t then
        return nil
    end
    local world = bot:world()
    local room = bot:room().id
    local goal = target_goal(bot, world, t)
    local steps = goal and goal.room ~= room and route(bot, world, room, goal.room)
    local next_room = steps and steps[1] and steps[1].target
    if next_room and (p.monsters[next_room] or 0) > 0 then
        return next_room
    end
    return nil
end

-- Pick the next goal, or nil plus the reason nothing is left.
local function choose(bot)
    local room = bot:room().id
    local world = bot:world()
    local here = world_room(world, room)

    -- 1. The room puzzle.
    local fact, index, count = BotPuzzle.next_interact(bot)
    if fact then
        if fact.slot and BotPuzzle.avoided(bot, fact.slot) then
            bot:log(string.format("interact slot %d is also an <avoid>; skipped", fact.slot))
        elseif fact.kind == "climb" and inventory_full(bot) and not BotNav.on_top(bot) then
            -- The climb is for an item: no free slot, no climb. Retried
            -- once the inventory changes (a box deposit makes room).
            if not blocked(bot, "interact", room, fact.slot) then
                mark(bot, "interact", room, fact.slot, "inventory full: " .. inventory_sig(bot))
            end
        elseif not blocked(bot, "interact", room, BotPuzzle.step_key(fact)) then
            local goal, why = interact_goal(bot, fact, index, count)
            if goal then
                return goal
            end
            mark(bot, "interact", room, BotPuzzle.step_key(fact), why)
        end
    end

    -- 1b. A held item to use at a zone (<use> facts that apply now).
    for _, row in ipairs(BotPuzzle.uses(bot)) do
        local fact = row.fact
        if not blocked(bot, "use", room, fact.slot) then
            return use_goal(bot, fact, row.action)
        end
    end

    -- 1c. Inventory room: a MOVE that empties a slot, then an item box
    -- here when a deposit or a withdrawal is due.
    if not blocked(bot, "combine", "inventory", "mix", true) then
        local pick = BotInventory.combine_now(bot, gave_up)
        if pick then
            return combine_goal(bot, pick)
        end
    end
    local boxes = BotPlanner.boxes_here(bot)
    local box_here = #boxes > 0 and BotInventory.box_reason(bot, gave_up, true)
    if box_here then
        -- The nearest box by its planned approach (every box shares one
        -- store).
        local box = BotPlanner.nearest(boxes, function(action)
            return BotNav.reach_length(bot, action)
        end)
        return box_goal(bot, box, box_here)
    end
    local box_why = BotInventory.box_reason(bot, gave_up)

    -- 1d. Item errands here: carry a held item a <deliver> wants into its
    -- camera cut, or take one a wanted <deliver> needs where a <fetch>
    -- offers it.
    for _, fact in ipairs(BotPuzzle.errands(bot)) do
        if fact.room == room and BotPuzzle.deliver_wanted(bot, fact)
            and bot:has_item(fact.item)
            and not blocked(bot, "deliver", room, "camera" .. fact.cut) then
            return deliver_goal(bot, fact)
        end
    end
    for _, fact in ipairs(BotPuzzle.fetches_due(bot)) do
        if fact.room == room and not blocked(bot, "fetch", room, fact.slot) then
            if not pickup_fits(bot, fact.item, inventory_full(bot)) then
                mark(bot, "fetch", room, fact.slot, "inventory full: " .. inventory_sig(bot))
            else
                local goal, why = fetch_goal(bot, fact)
                if goal then
                    return goal
                end
                mark(bot, "fetch", room, fact.slot, why)
            end
        end
    end

    -- 2. Ground items here, keys first (a key opens a door; nothing else
    -- in the room is worth more), then the nearest planned path.
    local best, best_len, best_key, best_desk = nil, nil, false, nil
    local unreachable_here = false
    local full = inventory_full(bot)
    local actions = bot:actions()
    local drawers = BotPlanner.drawers(actions)
    if plan().hurt[room] == inventory_sig(bot) then
        -- The hurt rule: this room hurt a goal with this inventory; its
        -- pickups wait until the inventory changes.
        actions = {}
    elseif plan().hurt[room] then
        plan().hurt[room] = nil
    end
    for _, action in ipairs(actions) do
        local desk = drawers[action.slot]
        if desk and (action.kind == "item" or action.kind == "item_dormant") then
            -- A drawer's item: the drawer below is the goal.
        elseif action.kind == "desk" and action.desk_item
            and not blocked(bot, "item", room, action.item_slot) then
            local why = BotPlanner.desk_blocker(bot, action)
            if why == "the drawer is empty" then
                -- Nothing to take; check_desk refuses the press.
            elseif not pickup_fits(bot, action.desk_item, full) then
                mark(bot, "item", room, action.item_slot,
                    "inventory full: " .. inventory_sig(bot))
            elseif why then
                mark(bot, "item", room, action.item_slot, why)
                plan().marks[mark_key("item", room, action.item_slot)].desk = {
                    lock_bank = action.lock_bank, lock_bit = action.lock_bit,
                }
            elseif not BotPuzzle.avoided(bot, action.slot)
                and not BotPuzzle.avoided(bot, action.item_slot) then
                local len = BotNav.reach_length(bot, action) or UNPLANNED
                plan().lens[mark_key("item", room, action.item_slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                local key = bot:item_kind(action.desk_item) == "key"
                if not best_len or (key and not best_key)
                    or (key == best_key and len < best_len) then
                    best, best_len, best_key, best_desk = action, len, key, action
                end
            end
        elseif action.kind == "item" and not blocked(bot, "item", room, action.slot) then
            local avoid, gate = BotPuzzle.avoided(bot, action.slot)
            if avoid then
                -- Marked so remote planning stops heading back for it.
                -- A gated avoid is lifted once its flag changes. Checked
                -- before the free slot: an inventory-full mark lifts on
                -- any inventory change and would send the bot back.
                mark(bot, "item", room, action.slot, "avoid: " .. avoid, gate == nil)
                plan().marks[mark_key("item", room, action.slot)].gate = gate
            elseif not pickup_fits(bot, action.item, full) then
                -- No slot for it and no stack to merge into: retried once
                -- the inventory changes.
                mark(bot, "item", room, action.slot, "inventory full: " .. inventory_sig(bot))
            elseif BotNav.is_origin_zone(action) then
                mark(bot, "item", room, action.slot,
                    "zone at the room origin: a scripted handover", true)
            else
                local len = BotNav.reach_length(bot, action) or UNPLANNED
                plan().lens[mark_key("item", room, action.slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                local key = bot:item_kind(action.item) == "key"
                if not best_len or (key and not best_key)
                    or (key == best_key and len < best_len) then
                    best, best_len, best_key, best_desk = action, len, key, nil
                end
            end
        end
    end
    if best_desk then
        return desk_goal(bot, best_desk)
    end
    if best then
        return item_goal(bot, best, taken_twin(here, best))
    end

    -- Doors of this room the planner may open, scored.
    local doors_here = {}
    for _, door in ipairs(bot:doors()) do
        local fresh, row = untraversed(here, door.slot)
        if fresh and door.target and door.target ~= room
            and (not BotNav.is_origin_zone(door) or via_action(bot, door))
            and not BotPuzzle.avoided(bot, door.slot)
            and not blocked(bot, "door", room, door.slot) then
            local ok, why, permanent = true, nil, false
            plan().locked[mark_key("door", room, door.slot)] = nil
            if live_locked(bot, door) then
                ok, why, permanent = key_opens(bot, door.key)
            end
            if not ok then
                if permanent then
                    if not blocked(bot, "door", room, door.slot) then
                        mark(bot, "door", room, door.slot, why, true)
                    end
                else
                    plan().locked[mark_key("door", room, door.slot)] = why
                end
            else
                local len = BotNav.reach_length(bot, via_action(bot, door) or door) or UNPLANNED
                local visited = row and row.target_visited or bot:visited(door.target)
                -- The length from where the bot stood, for remote ranking;
                -- a door with no planned approach costs UNPLANNED_COST
                -- there, and this room gets a passage check.
                plan().lens[mark_key("door", room, door.slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                if len == UNPLANNED and not visited then
                    unreachable_here = true
                end
                doors_here[#doors_here + 1] = {
                    door = door, len = len, visited = visited,
                }
            end
        end
    end

    -- 3. A passage check when part of this room is out of reach: a stair
    -- zone (`door_check`), a ladder (`stairs_zone`), or an action-key event zone no fact names (a
    -- room may start its stair climb from an event, as when a script turns
    -- the stair zone on from the event). One try per slot per entry.
    for _, m in pairs(plan().marks) do
        if m.room == room and m.reason:find("approach", 1, true) then
            unreachable_here = true
        end
    end
    if unreachable_here then
        local stair = passage_goal(bot, room)
        if stair then
            return stair
        end
    end

    -- 4-. The empty-frontier retry of an event door here (forgive_events):
    -- step out through the nearest door to a room already entered, so the
    -- way back is a new entry that lets the room's script decide again.
    if plan().reenter == room then
        local best_out, best_out_len = nil, nil
        for _, door in ipairs(bot:doors()) do
            if door.target and door.target ~= room and bot:visited(door.target)
                and not BotNav.is_origin_zone(door) and not live_locked(bot, door) then
                local len = BotNav.reach_length(bot, door)
                if len and (not best_out_len or len < best_out_len) then
                    best_out, best_out_len = door, len
                end
            end
        end
        if best_out then
            return door_goal(bot, best_out, "step out to re-enter " .. room)
        end
        plan().reenter = nil
    end

    -- 4. Leave the room: rank every way out by its distance cost (see the
    -- header), the doors of this room and the first door of the route to a
    -- remote frontier item or door alike.
    local lengths = {}
    local function door_len(slot)
        if lengths[slot] == nil then
            local door = BotNav.door_by_slot(bot, slot)
            local zone = door and (via_action(bot, door) or door)
            lengths[slot] = zone and BotNav.reach_length(bot, zone) or false
        end
        return lengths[slot] or UNPLANNED_COST
    end
    for _, row in ipairs(doors_here) do
        lengths[row.door.slot] = row.len ~= UNPLANNED and row.len or false
    end
    -- The mental map: which doors here the bot can reach from the part of
    -- the room it entered by.
    local mind = plan()
    local arrived = mind.last_from and mind.arrivals[room] and mind.arrivals[room][mind.last_from]
    if arrived and bot:distance(arrived.x, arrived.z) <= 2 * C.REACH then
        local legs = {}
        for _, door in ipairs(bot:doors()) do
            legs[door.slot] = door_len(door.slot)
        end
        mind.legs[room] = mind.legs[room] or {}
        mind.legs[room][mind.last_from] = legs
    end
    -- The mental map from where the bot stands, built once per choice.
    local tree = nil
    local function map()
        tree = tree or BotPlanner.map_tree(world, room, map_opts(bot, door_len))
        return tree
    end
    local goal_room = bot:goal_room()
    local function toward_goal(target)
        if not goal_room or not target then
            return 0
        end
        if target == goal_room then
            return GOAL_BONUS
        end
        if target:sub(1, 1) == goal_room:sub(1, 1) then
            return STAGE_BONUS
        end
        return 0
    end
    local p = plan()
    -- The way taken: a door of this room, and the remote target behind it.
    local function leave(way)
        -- The exit has no planned approach from this part of the room: a
        -- passage check here first (a stair to the part that reaches it).
        if not lengths[way.door.slot] then
            local stair = passage_goal(bot, room)
            if stair then
                bot:log(string.format("way out %s has no planned approach; %s first",
                    way.why, stair.label))
                return stair
            end
        end
        p.exits[room] = way.door.slot
        bot:log(string.format("way out: %s, cost %d", way.why, way.cost))
        return door_goal(bot, way.door, way.why)
    end
    local function usable(goal)
        if goal.kind == "item" then
            return not blocked(bot, "item", goal.room, goal.slot, true)
        end
        -- A door parked at the origin is a scripted transfer (see
        -- door_passable).
        return not (goal.x == 0 and goal.z == 0 and not via_fact(bot, goal.room, goal.slot))
            and not blocked(bot, "door", goal.room, goal.slot, true)
            and (not goal.locked or key_opens(bot, goal.key))
    end

    -- The ammo sourcing rule (BotSurvival.sourcing): under the floor with
    -- no ammunition the planner can reach. Turning it on re-ranks.
    local ammo, rounds, floor = BotSurvival.ammo_need(bot)
    local known = 0
    if ammo then
        for _, goal in ipairs(world.frontier) do
            if goal.kind == "item" and goal.item == ammo
                and not blocked(bot, "item", goal.room, goal.slot, true)
                and (goal.room == room or route(bot, world, room, goal.room)) then
                known = known + 1
            end
        end
        if BotInventory.remembered(bot)[ammo] then
            known = known + 1
        end
    end
    BotSurvival.note_known_ammo(known)
    local sourcing = BotSurvival.sourcing(ammo, known)
    if ammo then
        bot:log(string.format("ammo low: %d rounds under the floor %d; %d known ammo item(s)%s",
            rounds, floor, known, sourcing and "; sourcing" or ""))
    end
    if sourcing and not p.sourcing and p.target then
        bot:log("sourcing ammunition: target " .. p.target.label .. " re-ranked")
        p.target = nil
    end
    p.sourcing = sourcing
    -- While sourcing, the live monsters last seen along a route.
    local function monsters_in(id)
        local n = sourcing and p.monsters[id] or 0
        return (n > MONSTER_CAP and MONSTER_CAP or n) * MONSTER_COST
    end
    local function route_monsters(steps)
        local cost = 0
        for _, step in ipairs(steps) do
            cost = cost + monsters_in(step.target)
        end
        return cost
    end

    -- 4a. A pursued remote target keeps its course until it is reached,
    -- real progress re-ranks, or the loop rule drops it.
    if p.target then
        local t = p.target
        local goal = target_goal(bot, world, t)
        if goal and goal.kind == "item" and goal.room ~= room
            and not pickup_fits(bot, goal.item, full) then
            -- The inventory filled on the way: no slot for it there.
            goal = nil
        end
        if goal and (goal.kind == "use" or goal.kind == "box" or goal.kind == "deliver"
            or goal.kind == "fetch") and goal.room == room then
            -- Here, and step 1b/1c did not take it: its probe is not live,
            -- or the box is not.
            mark(bot, goal.kind, room, t.slot, goal.kind .. " not live on arrival")
            p.target = nil
            goal = nil
        end
        if goal and goal.room == room and goal.kind == "door" then
            for _, row in ipairs(doors_here) do
                if row.door.slot == goal.slot then
                    p.target = nil
                    return leave({ cost = door_len(goal.slot), door = row.door,
                        why = "reached " .. t.label })
                end
            end
        end
        local steps
        if goal and goal.room ~= room and goal.kind == "box" then
            -- A box was chosen by walking distance: keep the map's route.
            steps = map_route(bot, world, room, goal.room, map(),
                BotPlanner.action_point(world, goal.room, goal.slot))
        else
            steps = goal and goal.room ~= room
                and (goal.kind == "use" or goal.kind == "deliver" or goal.kind == "fetch"
                    or usable(goal))
                and route(bot, world, room, goal.room)
        end
        local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
        if first then
            return leave({ cost = door_len(first.slot), door = first,
                why = string.format("pursuing %s, %d door(s)", t.label, #steps) })
        end
        bot:log("target dropped: " .. t.label)
        p.target = nil
    end

    -- The keys the remembered locked doors need: key item -> a door name.
    local needed = {}
    for _, r in ipairs(world.rooms) do
        for _, door in ipairs(r.doors) do
            if door.present and door.locked and not door.unlocked and not door.traversed then
                needed[door.key] = needed[door.key]
                    or string.format("%s -> %s", r.id, tostring(door.target))
            end
        end
    end
    -- A door into an unvisited room that holds a needed item (<holds>).
    local holds = bot:item_facts()
    local use_list = bot:use_facts()
    local function holds_bonus(id)
        return BotPlanner.holds_bonus(id, holds, use_list,
            function(item) return bot:has_item(item) end,
            function(fact) return bot:flag(fact.bank, fact.bit) end,
            function(item) return bot:item_kind(item) end, ammo, needed)
    end
    local best_way = nil
    local function consider(way)
        if not best_way or way.cost < best_way.cost
            or (way.cost == best_way.cost and way.order < best_way.order) then
            best_way = way
        end
    end
    local order = 0
    for _, row in ipairs(doors_here) do
        order = order + 1
        local cost = door_len(row.door.slot) - toward_goal(row.door.target)
            + monsters_in(row.door.target)
        local why = row.visited and "untraversed" or "unvisited room"
        if not row.visited then
            local bonus, tag = holds_bonus(row.door.target)
            cost = cost - UNVISITED_BONUS - (sourcing and SOURCE_BONUS or 0) - bonus
            why = tag and (why .. " (" .. tag .. ")") or why
        end
        consider({
            cost = cost, order = order, door = row.door, why = why,
        })
    end
    -- The ammunition of the guns the bot holds: a remote trip for any other
    -- (rounds for a gun not found yet) only fills a slot a needed pickup
    -- wants, and one for a document unlocks nothing. Both are still taken
    -- when the bot is in their room.
    local gun_rounds = {}
    for id, _ in pairs((bot:knowledge() or {}).weapons or {}) do
        if bot:has_item(id) and bot:ammo_for(id) then
            gun_rounds[bot:ammo_for(id)] = true
        end
    end
    local function consider_frontier(allow_unfit)
        for _, goal in ipairs(world.frontier) do
            -- A remote item that would not fit is no trip: arriving full only
            -- marks it again. The box (box_why) or a combine makes room first.
            local fits = allow_unfit or goal.kind ~= "item"
                or pickup_fits(bot, goal.item, full)
            local useless = goal.kind == "item" and (goal.item_kind == "file"
                or goal.item_kind == "ammo" and not gun_rounds[goal.item])
            if goal.room ~= room and fits and not useless then
                local steps = usable(goal) and route(bot, world, room, goal.room)
                local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
                if first then
                    order = order + 1
                    local cost = door_len(first.slot) + #steps * DOOR_COST
                        + (p.lens[mark_key(goal.kind, goal.room, goal.slot)] or 0)
                        + route_monsters(steps)
                    local why, label
                    if goal.kind == "item" then
                        local bonus, tag = BotPlanner.item_bonus(goal.item, goal.item_kind, ammo,
                            needed)
                        cost = cost - bonus
                        label = string.format("item 0x%02X in %s", goal.item, goal.room)
                        if tag then
                            label = label .. " (" .. tag .. ")"
                        end
                    else
                        cost = cost - toward_goal(goal.target)
                        local tag = nil
                        if not goal.target_visited then
                            local bonus
                            bonus, tag = holds_bonus(goal.target)
                            cost = cost - UNVISITED_BONUS - (sourcing and SOURCE_BONUS or 0) - bonus
                        end
                        label = string.format("the %s -> %s door", goal.room, tostring(goal.target))
                        if tag then
                            label = label .. " (" .. tag .. ")"
                        end
                    end
                    why = string.format("toward %s, %d door(s)", label, #steps)
                    consider({
                        cost = cost, order = order, door = first, why = why,
                        target = { kind = goal.kind, room = goal.room, slot = goal.slot,
                            label = label, best = #steps, stalls = 0 },
                    })
                end
            end
        end
    end
    consider_frontier(false)

    -- A held item with a <use> fact in another room: go there.
    for _, fact in ipairs(bot:use_facts()) do
        if fact.room ~= room and bot:has_item(fact.item) and not bot:flag(fact.bank, fact.bit)
            and not plan().marks[mark_key("use", fact.room, fact.slot)] then
            local steps = route(bot, world, room, fact.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local label = string.format("use 0x%02X in %s (%s)", fact.item, fact.room,
                    fact.label or "unnamed")
                consider({
                    cost = door_len(first.slot) + #steps * DOOR_COST - USE_BONUS
                        + route_monsters(steps),
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = "use", room = fact.room, slot = fact.slot,
                        item = fact.item, bank = fact.bank, bit = fact.bit,
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
    end
    -- An item errand in another room: carry a held item to the <deliver>
    -- that wants it, or go to a <fetch> that offers one it needs.
    local wanted = BotPuzzle.wanted_items(bot)
    for _, fact in ipairs(BotPuzzle.errands(bot)) do
        local go
        if fact.kind == "deliver" then
            go = BotPuzzle.deliver_wanted(bot, fact) and bot:has_item(fact.item)
        else
            go = wanted[fact.item] and not bot:has_item(fact.item)
                and BotPuzzle.fetch_offered(bot, fact)
        end
        local slot = fact.kind == "deliver" and ("camera" .. fact.cut) or fact.slot
        if go and fact.room ~= room and not blocked(bot, fact.kind, fact.room, slot, true) then
            local steps = route(bot, world, room, fact.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local label = string.format("%s 0x%02X in %s (%s)", fact.kind, fact.item,
                    fact.room, fact.label or "unnamed")
                consider({
                    cost = door_len(first.slot) + #steps * DOOR_COST - ERRAND_BONUS
                        + route_monsters(steps),
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = fact.kind, room = fact.room, slot = slot,
                        item = fact.item, fact = fact,
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
    end
    -- A box visit is due and no box is here: go to the remembered box
    -- nearest by walking distance through the mental map.
    if box_why then
        local opts = map_opts(bot, door_len)
        opts.tree = map()
        opts.usable_box = function(id, action)
            return not blocked(bot, "box", id, action.slot, true)
        end
        local box = BotPlanner.pick_remote_box(world, room, opts)
        local first = box and BotNav.door_by_slot(bot, box.steps[1].slot)
        if first then
            order = order + 1
            local label = string.format("the item box in %s (%s)", box.room, box_why)
            consider({
                cost = box.cost - BOX_BONUS + route_monsters(box.steps),
                order = order, door = first,
                why = string.format("toward %s, %d door(s)", label, #box.steps),
                target = { kind = "box", room = box.room, slot = box.slot,
                    label = label, best = #box.steps, stalls = 0 },
            })
        end
    end
    if not best_way and full then
        -- Nothing else is left: the full-inventory trips are the only
        -- way on (an item may merge or a combine may free a slot there).
        consider_frontier(true)
    end
    if best_way then
        if best_way.target then
            p.target = best_way.target
            bot:log("target: " .. p.target.label)
        end
        return leave(best_way)
    end
    return nil
end

-- A summary of what is left for the finish reason.
local function leftovers(bot)
    local keys, others = {}, {}
    for key, why in pairs(plan().locked) do
        keys[#keys + 1] = key .. ": " .. why
    end
    for _, m in pairs(plan().marks) do
        local text = string.format("%s %s slot %s: %s", m.kind, m.room, tostring(m.slot), m.reason)
        if m.reason:find("needs key", 1, true) then
            keys[#keys + 1] = text
        else
            others[#others + 1] = text
        end
    end
    table.sort(keys)
    table.sort(others)
    local parts = {}
    if #keys > 0 then
        parts[#parts + 1] = "locked: " .. table.concat(keys, "; ")
    end
    if #others > 0 then
        parts[#parts + 1] = "blocked: " .. table.concat(others, "; ")
    end
    return table.concat(parts, " | ")
end

function BotPlanner.finish(bot, reason)
    -- The remembered door graph, for diagnosing why the frontier ran dry.
    for _, r in ipairs(bot:world().rooms or {}) do
        local parts = {}
        for _, door in ipairs(r.doors or {}) do
            parts[#parts + 1] = string.format("%d->%s k%02X%s%s%s", door.slot,
                tostring(door.target), door.key or 0, door.traversed and " T" or "",
                door.locked and not door.unlocked and " L" or "",
                door_passable(bot, r.id, door) and "" or " X")
        end
        bot:log(string.format("world %s: %s", r.id, table.concat(parts, ", ")))
    end
    -- The ammunition account: every ammunition pickup the world model saw,
    -- and what became of it.
    local marks = plan().marks
    for _, r in ipairs(bot:world().rooms or {}) do
        for _, item in ipairs(r.items or {}) do
            if item.item_kind == "ammo" then
                local m = marks[mark_key("item", r.id, item.slot)]
                local fate = item.taken and "taken"
                    or (item.present and "left" or "gone")
                bot:log(string.format("ammo account: %s slot %d item 0x%02X x%d %s%s",
                    r.id, item.slot, item.item, item.quantity or 0, fate,
                    m and not item.taken and (" (" .. m.reason .. ")") or ""))
            end
        end
    end
    local p = bot:player()
    local text = string.format("%s (room %s health %d/%d) %s", reason, bot:room().id,
        p.health, p.max_health, leftovers(bot))
    bot:log("FINISH " .. text)
    bot:finish(text)
end

-- Before the run finishes on an empty frontier: a goal that failed on its
-- approach or its time budget depends on where the bot stood, so each such
-- mark under MAX_ATTEMPTS gets another try (its attempts still count, so
-- this ends). Returns how many marks were forgiven.
local forgive_events

local function forgive_approaches(bot)
    local n = 0
    for key, m in pairs(plan().marks) do
        if not m.permanent and m.attempts < MAX_ATTEMPTS and not m.hold and m.sig ~= ""
            and (m.reason:find("approach", 1, true) or m.reason:find("timed out", 1, true)) then
            m.sig = ""
            m.entry = nil
            n = n + 1
            bot:log("frontier empty; retrying " .. key .. " (" .. m.reason .. ")")
        end
    end
    return n
end

-- Before the run finishes on an empty frontier: each door a room event
-- made a message is retried. Away from its room, at once: the walk back is
-- the wait. In its room, once it has waited EVENT_WAIT (doubled per
-- attempt) since it failed; standing there on the entry it failed on, the
-- bot then steps out (p.reenter): the room's script decides the door again
-- only on an entry. Returns the forgiven count and whether a mark is still
-- waiting (the bot then idles instead of finishing).
forgive_events = function(bot)
    local p = plan()
    local n, waiting = 0, false
    for key, m in pairs(p.marks) do
        if m.event and not m.event.forgiven then
            local wait = EVENT_WAIT * (1 << (m.attempts - 1))
            if m.room ~= bot:room().id or bot:tick() - m.event.tick >= wait then
                m.event.forgiven = true
                n = n + 1
                if m.room == bot:room().id and p.entries[m.room] == m.event.entry then
                    p.reenter = m.room
                end
                bot:log(string.format("frontier empty; retrying %s (%s) after %d ticks; %s",
                    key, m.reason, bot:tick() - m.event.tick,
                    p.reenter and "stepping out to re-enter its room" or "travelling back"))
            else
                if not m.event.waiting then
                    m.event.waiting = true
                    bot:log(string.format("frontier empty; waiting for %s (%s)", key, m.reason))
                end
                waiting = true
            end
        end
    end
    return n, waiting
end

-- Called once on every room change.
function BotPlanner.enter_room(bot, room)
    local p = plan()
    local from = p.goal
    if from and from.kind == "door" and from.target and from.target ~= room.id then
        bot:log(string.format("door %s led to %s instead of %s", from.label,
            room.id, from.target))
    end
    p.goal = nil
    p.entries[room.id] = (p.entries[room.id] or 0) + 1
    p.reenter = nil
    -- The mental map: where the bot arrives in this room from the last one.
    if p.last_room and p.last_room ~= room.id then
        local me = bot:player()
        p.arrivals[room.id] = p.arrivals[room.id] or {}
        p.arrivals[room.id][p.last_room] = { x = me.x, z = me.z }
    end
    p.last_from = p.last_room ~= room.id and p.last_room or nil
    p.last_room = room.id
    BotPuzzle.enter_room()
    p.last_done = { room = room.id, tick = bot:tick(), anywhere = true }
    progress(bot, "entered " .. room.id)
    p.stairs[room.id] = nil
    local goal = bot:goal_room()
    if goal and room.id == goal then
        BotPlanner.finish(bot, "goal room " .. goal .. " reached")
        return
    end
    BotPlanner.observe_progress(bot, "entered " .. room.id)
    -- The loop rules: a room entered again and again without real progress,
    -- and a pursued target whose route does not get shorter.
    local verdict, n = BotPlanner.repeat_check(p.since, room.id)
    if verdict == "finish" then
        BotPlanner.finish(bot, string.format(
            "looping: entered %s %d times without progress, every other goal marked",
            room.id, n))
        return
    end
    local t = p.target
    if n > 1 or (t and (t.stalls or 0) > 0) then
        bot:log(string.format("loop count: %s entered %d times since progress; target stalls %s",
            room.id, n, t and tostring(t.stalls or 0) or "-"))
    end
    if verdict == "loop" then
        if t then
            mark_loop(bot, t.kind, t.room, t.slot,
                string.format("%s entered %d times without progress", room.id, n))
            p.target = nil
        elseif p.exits[room.id] then
            mark_loop(bot, "door", room.id, p.exits[room.id],
                string.format("%s entered %d times without progress", room.id, n))
        end
        return
    end
    if t then
        local world = bot:world()
        local goal = target_goal(bot, world, t)
        local steps = goal and route(bot, world, room.id, goal.room)
        if steps and BotPlanner.pursue(t, #steps) then
            mark_loop(bot, t.kind, t.room, t.slot, string.format(
                "%d entries without a shorter route", t.stalls))
            p.target = nil
        end
    end
end

-- Compare the progress signature with the last one; on a change, the loop
-- counts and the pursued target start over.
function BotPlanner.observe_progress(bot, note)
    local p = plan()
    local sig = progress_sig(bot, bot:world())
    if sig ~= p.sig then
        p.sig = sig
        real_progress(bot, note)
    end
end

-- Give up the current goal without marking it, for a layer that takes the
-- pad for a while (a save at a typewriter); it is chosen again afterwards.
-- A message is up. If the current goal is a door the bot pressed, the
-- press opened a message instead of the door (a room event's lock).
function BotPlanner.note_message(bot, message)
    local goal = plan().goal
    if goal and goal.kind == "door" and not goal.was_locked and goal.job
        and goal.job.pressed and not goal.door_message then
        goal.door_message = message
    end
end

function BotPlanner.drop_goal(bot)
    local p = plan()
    if p.goal then
        bot:log("goal paused: " .. p.goal.label)
        p.goal = nil
    end
end

-- Whether the next goal waits for a use probe: within `window` ticks of
-- the last goal done in this room (`last_done` {room, tick, zone}), a <use>
-- fact here has its item held and its done flag clear but no live probe
-- yet, at a zone overlapping the one that goal acted on. Entering the room
-- counts as a goal done anywhere in it (`anywhere`).
function BotPlanner.awaiting_use(bot, last_done, window)
    if not last_done or last_done.room ~= bot:room().id
        or bot:tick() - last_done.tick > window then
        return nil
    end
    return BotPuzzle.use_pending(bot, last_done.zone, last_done.anywhere)
end

-- One controllable planner tick.
function BotPlanner.step(bot)
    local p = plan()
    local room = bot:room().id
    if not p.monster_tick or bot:tick() - p.monster_tick >= MONSTER_EVERY then
        p.monster_tick = bot:tick()
        p.monsters[room] = #bot:enemies()
    end
    if bot:tick() - (p.progress or 0) > STUCK_TICKS then
        BotPlanner.finish(bot, string.format("stuck in room %s: no progress for %d ticks",
            room, STUCK_TICKS))
        return
    end
    if not p.goal then
        local fact = BotPlanner.awaiting_use(bot, p.last_done, USE_ARM_TICKS)
        if fact then
            if not p.awaiting then
                bot:log("waiting for the use probe: " .. (fact.label or "use"))
            end
            p.awaiting = true
            return
        end
        p.awaiting = nil
        p.goal = choose(bot)
        if not p.goal and forgive_approaches(bot) > 0 then
            p.goal = choose(bot)
        end
        if not p.goal then
            local forgiven, waiting = forgive_events(bot)
            if forgiven > 0 then
                p.goal = choose(bot)
            elseif waiting then
                return
            end
        end
        if not p.goal then
            local loops = 0
            for _, m in pairs(p.marks) do
                if m.reason:find("looping", 1, true) and blocked(bot, m.kind, m.room, m.slot, true) then
                    loops = loops + 1
                end
            end
            BotPlanner.finish(bot, loops > 0 and string.format(
                "frontier empty: nothing reachable is left but %d goal(s) marked looping", loops)
                or "frontier empty: nothing reachable is left")
            return
        end
        p.goal.started = bot:tick()
        p.goal.health = bot:player().health
        p.goal.hurt = 0
        bot:log("goal: " .. p.goal.label)
        BotScene.clear_pickup()
    end
    local goal = p.goal
    local me = bot:player()
    if me.health < (goal.health or me.health) then
        goal.hurt = (goal.hurt or 0) + goal.health - me.health
    end
    goal.health = me.health
    if goal.kind == "item" and goal.hurt >= me.max_health * HURT_SHARE then
        mark(bot, "item", room, goal.slot, string.format("hurt while trying: %d health", goal.hurt))
        p.hurt[room] = inventory_sig(bot)
        p.goal = nil
        return
    end
    local status
    if goal.job.camera then
        status = BotNav.visit_camera(bot, goal.job)
    elseif goal.job.menu then
        status = combine_step(bot, goal.job)
    elseif goal.job.push then
        status = BotNav.push_object(bot, goal.job)
        if status == "done" then
            BotPuzzle.pushed(bot, goal.job.step)
        end
    elseif goal.job.climb then
        status = BotNav.climb_object(bot, goal.job)
    elseif goal.job.fight then
        status = BotNav.hold_fight(bot, goal.job)
    else
        status = BotNav.press_zone(bot, goal.job)
    end
    if goal.job.press_tick == bot:tick() then
        BotPuzzle.pressed(bot, goal.press_slot or goal.slot)
        if goal.kind == "box" then
            BotInventory.expect_box(bot)
        end
    end
    if goal.door_message then
        -- The press opened a message, not the door: a room event holds it.
        status = "failed"
        goal.job.reason = string.format("a room event made the door a message (%d)",
            goal.door_message)
        goal.event = true
    end
    if status == "busy" and bot:tick() - goal.started > (goal.job.timeout or GOAL_TIMEOUT * 2) then
        status = "failed"
        goal.job.reason = "goal timed out"
    end
    if status == "done" and goal.kind == "box" and goal.sig == inventory_sig(bot)
        and not (goal.look and bot:memory().box_items ~= nil) then
        -- A visit that moved nothing must not be chosen again until the
        -- inventory changes.
        status = "failed"
        goal.job.reason = "the box visit changed nothing"
    end
    if status == "done" then
        bot:log("goal done: " .. goal.label)
        p.last_done = { room = room, tick = bot:tick(), zone = goal.job.zone }
        progress(bot)
        if goal.kind == "interact" or goal.kind == "use" or goal.kind == "camera"
            or goal.kind == "deliver" or goal.kind == "fetch" then
            if goal.kind ~= "camera" then
                p.steps_done = (p.steps_done or 0) + 1
            end
            real_progress(bot, goal.label)
            -- A room event (an object moved off an item) may open what
            -- was unreachable here.
            if goal.kind ~= "camera" then
                clear_unreachable(bot, room)
            end
        else
            BotPlanner.observe_progress(bot, goal.label)
        end
        if goal.kind == "stair" then
            clear_unreachable(bot, room)
        end
        p.goal = nil
    elseif status == "failed" then
        local kind = (goal.kind == "camera") and "interact" or goal.kind
        -- A combine is about the inventory, not the room it was tried in.
        local where = goal.kind == "combine" and "inventory" or room
        mark(bot, kind, where, goal.slot, goal.job.reason or "failed", goal.job.permanent)
        if goal.event then
            local m = p.marks[mark_key(kind, where, goal.slot)]
            m.event = { entry = p.entries[room], tick = bot:tick(), forgiven = false }
        end
        p.goal = nil
    end
end
