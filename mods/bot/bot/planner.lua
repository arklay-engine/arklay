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
--   2. ground items in this room, keys first, then the nearest approach:
--      a visible item here is taken before the bot moves on, unless the
--      inventory is full and it merges into no held stack. A second zone
--      of an item just taken here gets one approach;
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
--             - AMMO_BONUS when that item is the ammunition the equipped
--               gun needs (BotSurvival.ammo_need), or KEY_BONUS when it
--               is a key a remembered locked door needs
--             - USE_BONUS for a route to a held item's <use> room
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
-- A route to a seen key item that a remembered locked door needs.
local KEY_BONUS = 60 * C.REACH
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
-- A door to a room never entered.
local UNVISITED_BONUS = 15 * C.REACH
-- A route to a remote frontier item.
local ITEM_BONUS = 10 * C.REACH
-- A door into the --bot-goal-room, and one into the goal's stage. The goal
-- door is worth ten door steps: once its key is held it outranks every
-- other goal short of a long detour.
local GOAL_BONUS = 10 * DOOR_COST
local STAGE_BONUS = 6 * C.REACH
-- An action-key event pressed as a passage gets this long.
local PASSAGE_TIMEOUT = 15 * C.SECOND

local function plan()
    BOT_STATE.plan = BOT_STATE.plan
        or { marks = {}, stairs = {}, locked = {}, entries = {}, lens = {}, progress = 0 }
    BOT_STATE.plan.lens = BOT_STATE.plan.lens or {}
    BOT_STATE.plan.since = BOT_STATE.plan.since or {}
    BOT_STATE.plan.exits = BOT_STATE.plan.exits or {}
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
    if m.permanent or m.attempts >= MAX_ATTEMPTS then
        return true
    end
    if m.hold and bot:tick() >= m.hold then
        return false
    end
    if m.sig ~= inventory_sig(bot) then
        return false
    end
    local entry = (plan().entries or {})[room]
    if not remote and m.reason:find("approach", 1, true) and m.entry ~= entry then
        return false
    end
    return true
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
-- unlocked and the inventory. Walking between known rooms changes none.
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
local function door_passable(bot, room_id, door)
    if not door.present or not door.target then
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
    local action = BotNav.action_by_slot(bot, fact.slot)
    if not action then
        return nil, string.format("interact slot %d is not a live action", fact.slot)
    end
    local label = string.format("interact %d/%d slot %d (%s)", index, count,
        fact.slot, fact.label or "unnamed")
    return {
        kind = "interact", slot = fact.slot, label = label,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND,
            done = function(b) return b:flag(fact.bank, fact.bit) end,
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

local function door_goal(bot, door, why)
    local label = string.format("door slot %d -> %s (%s)", door.slot,
        tostring(door.target), why)
    return {
        kind = "door", slot = door.slot, target = door.target, label = label,
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
            -- The climb is a warp: one tick's jump after the press.
            done = function(b, job)
                local q = b:player()
                local last = job.last_pos
                return job.pressed and last ~= nil
                    and BotNav.abs(q.x - last[1]) + BotNav.abs(q.z - last[2])
                        > C.WARP_DISTANCE
            end,
        },
    }
end

-- A passage check here: a stair zone (`door_check`), or an action-key
-- event zone no fact names (a room may start its stair climb from an
-- event, as when a script turns the stair zone on from the event). One try
-- per slot per entry, nearest first.
local function passage_goal(bot, room)
    local p = plan()
    p.stairs[room] = p.stairs[room] or {}
    local stair, stair_len = nil, nil
    for _, action in ipairs(bot:actions()) do
        local passage = action.kind == "door_check"
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
    if t.kind == "use" then
        if bot:has_item(t.item) and not bot:flag(t.bank, t.bit)
            and not blocked(bot, "use", t.room, t.slot, true) then
            return { kind = "use", room = t.room, slot = t.slot }
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

-- Pick the next goal, or nil plus the reason nothing is left.
local function choose(bot)
    local room = bot:room().id
    local world = bot:world()
    local here = world_room(world, room)

    -- 1. The room puzzle.
    local fact, index, count = BotPuzzle.next_interact(bot)
    if fact then
        if BotPuzzle.avoided(bot, fact.slot) then
            bot:log(string.format("interact slot %d is also an <avoid>; skipped", fact.slot))
        elseif not blocked(bot, "interact", room, fact.slot or ("camera" .. fact.cut)) then
            local goal, why = interact_goal(bot, fact, index, count)
            if goal then
                return goal
            end
            mark(bot, "interact", room, fact.slot or ("camera" .. fact.cut), why)
        end
    end

    -- 1b. A held item to use at a zone (<use> facts that apply now).
    for _, row in ipairs(BotPuzzle.uses(bot)) do
        local fact = row.fact
        if not blocked(bot, "use", room, fact.slot) then
            return use_goal(bot, fact, row.action)
        end
    end

    -- 2. Ground items here, keys first (a key opens a door; nothing else
    -- in the room is worth more), then the nearest planned path.
    local best, best_len, best_key = nil, nil, false
    local unreachable_here = false
    local full = inventory_full(bot)
    for _, action in ipairs(bot:actions()) do
        if action.kind == "item" and not blocked(bot, "item", room, action.slot) then
            local avoid = BotPuzzle.avoided(bot, action.slot)
            if not pickup_fits(bot, action.item, full) then
                -- No slot for it and no stack to merge into: retried once
                -- the inventory changes.
                mark(bot, "item", room, action.slot, "inventory full")
            elseif avoid then
                -- Marked so remote planning stops heading back for it.
                mark(bot, "item", room, action.slot, "avoid: " .. avoid, true)
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
                    best, best_len, best_key = action, len, key
                end
            end
        end
    end
    if best then
        return item_goal(bot, best, taken_twin(here, best))
    end

    -- Doors of this room the planner may open, scored.
    local doors_here = {}
    for _, door in ipairs(bot:doors()) do
        local fresh, row = untraversed(here, door.slot)
        if fresh and door.target and door.target ~= room
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
                local len = BotNav.reach_length(bot, door) or UNPLANNED
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
    -- zone (`door_check`), or an action-key event zone no fact names (a
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

    -- 4. Leave the room: rank every way out by its distance cost (see the
    -- header), the doors of this room and the first door of the route to a
    -- remote frontier item or door alike.
    local lengths = {}
    local function door_len(slot)
        if lengths[slot] == nil then
            local door = BotNav.door_by_slot(bot, slot)
            lengths[slot] = door and BotNav.reach_length(bot, door) or false
        end
        return lengths[slot] or UNPLANNED_COST
    end
    for _, row in ipairs(doors_here) do
        lengths[row.door.slot] = row.len ~= UNPLANNED and row.len or false
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
        return not blocked(bot, "door", goal.room, goal.slot, true)
            and (not goal.locked or key_opens(bot, goal.key))
    end

    -- 4a. A pursued remote target keeps its course until it is reached,
    -- real progress re-ranks, or the loop rule drops it.
    if p.target then
        local t = p.target
        local goal = target_goal(bot, world, t)
        if goal and goal.kind == "use" and goal.room == room then
            -- Here, and step 1b did not take it: its probe is not live.
            mark(bot, "use", room, t.slot, "use probe not live on arrival")
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
        local steps = goal and goal.room ~= room
            and (goal.kind == "use" or usable(goal))
            and route(bot, world, room, goal.room)
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
    local ammo, rounds, floor = BotSurvival.ammo_need(bot)
    if ammo then
        local known = 0
        for _, goal in ipairs(world.frontier) do
            if goal.kind == "item" and goal.item == ammo then
                known = known + 1
            end
        end
        bot:log(string.format("ammo low: %d rounds under the floor %d; %d known ammo item(s)",
            rounds, floor, known))
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
        if not row.visited then
            cost = cost - UNVISITED_BONUS
        end
        consider({
            cost = cost, order = order, door = row.door,
            why = row.visited and "untraversed" or "unvisited room",
        })
    end
    for _, goal in ipairs(world.frontier) do
        if goal.room ~= room then
            local steps = usable(goal) and route(bot, world, room, goal.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local cost = door_len(first.slot) + #steps * DOOR_COST
                    + (p.lens[mark_key(goal.kind, goal.room, goal.slot)] or 0)
                local why, label
                if goal.kind == "item" then
                    cost = cost - ITEM_BONUS
                    label = string.format("item 0x%02X in %s", goal.item, goal.room)
                    if ammo and goal.item == ammo then
                        cost = cost - AMMO_BONUS
                        label = label .. " (ammunition)"
                    elseif goal.item_kind == "key" and needed[goal.item] then
                        cost = cost - KEY_BONUS
                        label = label .. " (key for " .. needed[goal.item] .. ")"
                    end
                else
                    cost = cost - toward_goal(goal.target)
                    if not goal.target_visited then
                        cost = cost - UNVISITED_BONUS
                    end
                    label = string.format("the %s -> %s door", goal.room, tostring(goal.target))
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
                    cost = door_len(first.slot) + #steps * DOOR_COST - USE_BONUS,
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = "use", room = fact.room, slot = fact.slot,
                        item = fact.item, bank = fact.bank, bit = fact.bit,
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
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
    BotPuzzle.enter_room()
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
function BotPlanner.drop_goal(bot)
    local p = plan()
    if p.goal then
        bot:log("goal paused: " .. p.goal.label)
        p.goal = nil
    end
end

-- One controllable planner tick.
function BotPlanner.step(bot)
    local p = plan()
    local room = bot:room().id
    if bot:tick() - (p.progress or 0) > STUCK_TICKS then
        BotPlanner.finish(bot, string.format("stuck in room %s: no progress for %d ticks",
            room, STUCK_TICKS))
        return
    end
    if not p.goal then
        p.goal = choose(bot)
        if not p.goal and forgive_approaches(bot) > 0 then
            p.goal = choose(bot)
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
        bot:log("goal: " .. p.goal.label)
        BotScene.clear_pickup()
    end
    local goal = p.goal
    local status
    if goal.job.camera then
        status = BotNav.visit_camera(bot, goal.job)
    else
        status = BotNav.press_zone(bot, goal.job)
    end
    if goal.job.press_tick == bot:tick() then
        BotPuzzle.pressed(bot, goal.slot)
    end
    if status == "busy" and bot:tick() - goal.started > GOAL_TIMEOUT * 2 then
        status = "failed"
        goal.job.reason = "goal timed out"
    end
    if status == "done" then
        bot:log("goal done: " .. goal.label)
        progress(bot)
        if goal.kind == "interact" or goal.kind == "use" or goal.kind == "camera" then
            real_progress(bot, goal.label)
        else
            BotPlanner.observe_progress(bot, goal.label)
        end
        if goal.kind == "stair" then
            clear_unreachable(bot, room)
        end
        p.goal = nil
    elseif status == "failed" then
        local kind = (goal.kind == "camera") and "interact" or goal.kind
        mark(bot, kind, room, goal.slot, goal.job.reason or "failed", goal.job.permanent)
        p.goal = nil
    end
end
