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
--   2. ground items in this room, nearest approach first: a visible item
--      here is always taken before the bot moves on;
--   3. a passage check when a goal of this room has no reachable approach
--      from here: a `door_check` stair zone, or an action-key event zone no
--      fact names (a script may start the climb from an event), each once
--      per room entry, done when the player warps;
--   4. the way out, ranked by distance cost (lowest first) over the doors
--      here and the first door of the route to every remote frontier goal:
--        cost = path length to the door's approach (UNPLANNED_COST when the
--               path planner reaches none)
--             + DOOR_COST per door step of the route (0 for a door here)
--             + for a remote goal, its path length in its own room from where
--               the bot stood there last (UNPLANNED_COST if none was found)
--             - UNVISITED_BONUS for a door into a room not yet entered
--             - ITEM_BONUS for a route to a remote frontier item
--             - GOAL_BONUS for a door into the --bot-goal-room, or
--               STAGE_BONUS for one into the goal room's stage;
--      ties go to the doors here, then frontier order. A locked door counts
--      only with the key it needs. So an unvisited door here beats an item
--      two rooms back, while an item one door away beats a far door;
--   5. nothing left: finish with the reason.
-- A goal that fails is marked with the inventory it failed with; it is tried
-- again (at most MAX_ATTEMPTS times) once the inventory changes. The goal
-- room, if any, comes from --bot-goal-room. Every room fact comes from the
-- world model and the room's bot.xml; this module holds no room literals.

BotPlanner = BotPlanner or {}

local C = BotConst

local MAX_ATTEMPTS = 3
local GOAL_TIMEOUT = 40 * C.SECOND
-- A run with no new progress (a room, an item, a flag fact) for this long
-- is stuck.
local STUCK_TICKS = 5 * 60 * C.SECOND
-- A room entered this many times means the planner is looping.
local MAX_ENTRIES = 40
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
-- A door into the --bot-goal-room, and one into the goal's stage.
local GOAL_BONUS = 30 * C.REACH
local STAGE_BONUS = 6 * C.REACH
-- An action-key event pressed as a passage gets this long.
local PASSAGE_TIMEOUT = 15 * C.SECOND

local function plan()
    BOT_STATE.plan = BOT_STATE.plan
        or { marks = {}, stairs = {}, locked = {}, entries = {}, lens = {}, progress = 0 }
    BOT_STATE.plan.lens = BOT_STATE.plan.lens or {}
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

local function item_goal(bot, action)
    local before = bot:item_count(action.item)
    local label = string.format("item 0x%02X slot %d", action.item, action.slot)
    return {
        kind = "item", slot = action.slot, label = label, item = action.item,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 3, gap = 15, side = 2 * C.SECOND,
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

    -- 2. Ground items here.
    local best, best_len = nil, nil
    local unreachable_here = false
    for _, action in ipairs(bot:actions()) do
        if action.kind == "item" and not blocked(bot, "item", room, action.slot) then
            local avoid = BotPuzzle.avoided(bot, action.slot)
            if avoid then
                -- Marked so remote planning stops heading back for it.
                mark(bot, "item", room, action.slot, "avoid: " .. avoid, true)
            elseif BotNav.is_origin_zone(action) then
                mark(bot, "item", room, action.slot,
                    "zone at the room origin: a scripted handover", true)
            else
                local len = BotNav.reach_length(bot, action) or UNPLANNED
                plan().lens[mark_key("item", room, action.slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                if not best_len or len < best_len then
                    best, best_len = action, len
                end
            end
        end
    end
    if best then
        return item_goal(bot, best)
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
        local p = plan()
        p.stairs[room] = p.stairs[room] or {}
        local stair, stair_len = nil, nil
        for _, action in ipairs(bot:actions()) do
            local passage = action.kind == "door_check"
                or (action.kind == "event" and action.flags >= C.ACTION_KEY
                    and not BotPuzzle.is_fact_slot(bot, action.slot))
            if passage and not p.stairs[room][action.slot]
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
            local usable
            if goal.kind == "item" then
                usable = not blocked(bot, "item", goal.room, goal.slot, true)
            else
                usable = not blocked(bot, "door", goal.room, goal.slot, true)
                    and (not goal.locked or key_opens(bot, goal.key))
            end
            local steps = usable and route(bot, world, room, goal.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local cost = door_len(first.slot) + #steps * DOOR_COST
                    + (plan().lens[mark_key(goal.kind, goal.room, goal.slot)] or 0)
                local why
                if goal.kind == "item" then
                    cost = cost - ITEM_BONUS
                    why = string.format("toward item 0x%02X in %s, %d door(s)",
                        goal.item, goal.room, #steps)
                else
                    cost = cost - toward_goal(goal.target)
                    if not goal.target_visited then
                        cost = cost - UNVISITED_BONUS
                    end
                    why = string.format("toward the %s -> %s door, %d door(s)",
                        goal.room, tostring(goal.target), #steps)
                end
                consider({ cost = cost, order = order, door = first, why = why })
            end
        end
    end
    if best_way then
        local goal = door_goal(bot, best_way.door, best_way.why)
        bot:log(string.format("way out: %s, cost %d", best_way.why, best_way.cost))
        return goal
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
    local world = bot:world()
    local here = world_room(world, room.id)
    if here and here.entries > MAX_ENTRIES then
        BotPlanner.finish(bot, string.format(
            "looping: entered %s %d times", room.id, here.entries))
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
        if not p.goal then
            BotPlanner.finish(bot, "frontier empty: nothing reachable is left")
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
        if goal.kind == "stair" then
            clear_unreachable(bot, room)
        end
        p.goal = nil
    elseif status == "failed" then
        local kind = (goal.kind == "camera") and "interact" or goal.kind
        mark(bot, kind, room, goal.slot, goal.job.reason or "failed")
        p.goal = nil
    end
end
