-- The generic goal planner for the player bot.
--
-- Each tick that the scene handler, the menu skills, healing and combat do
-- not own, the planner runs the current goal, or picks one, highest priority
-- first:
--   1. room puzzle: the next <interact> of the room's bot.xml whose done
--      flag is clear (walk to its AOT zone, press, wait for the flag);
--   2. ground items in this room, nearest approach first;
--   3. a stair check (an in-room `door_check` zone) when a door or item of
--      this room has no reachable approach from here;
--   4. the nearest frontier item in another room (travel there);
--   5. the nearest untraversed door the bot can open, preferring doors to
--      rooms not yet visited; a locked door only with the key it needs;
--   6. nothing left: finish with the reason.
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

local function plan()
    BOT_STATE.plan = BOT_STATE.plan
        or { marks = {}, stairs = {}, locked = {}, entries = {}, progress = 0 }
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
    local label = string.format("stair check slot %d", action.slot)
    return {
        kind = "stair", slot = action.slot, label = label,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 40, side = 10 * C.SECOND,
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
                doors_here[#doors_here + 1] = {
                    door = door, len = len, visited = visited,
                }
            end
        end
    end

    -- 3. A stair check when part of this room is out of reach.
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
            if action.kind == "door_check" and not p.stairs[room][action.slot]
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

    -- 4. The nearest frontier item in another room.
    local best_route, best_item = nil, nil
    for _, goal in ipairs(world.frontier) do
        if goal.kind == "item" and goal.room ~= room
            and not blocked(bot, "item", goal.room, goal.slot, true) then
            local steps = route(bot, world, room, goal.room)
            if steps and #steps > 0 and (not best_route or #steps < #best_route) then
                best_route, best_item = steps, goal
            end
        end
    end
    if best_route then
        local door = BotNav.door_by_slot(bot, best_route[1].slot)
        if door then
            return door_goal(bot, door, string.format(
                "toward item 0x%02X in %s, %d door(s)", best_item.item,
                best_item.room, #best_route))
        end
    end

    -- 5. Doors: here first by (unvisited target, path length), then the
    -- nearest frontier door elsewhere by route length.
    table.sort(doors_here, function(a, b)
        if a.visited ~= b.visited then
            return not a.visited
        end
        if a.len ~= b.len then
            return a.len < b.len
        end
        return a.door.slot < b.door.slot
    end)
    local first = doors_here[1]
    if first and not first.visited then
        return door_goal(bot, first.door, "unvisited room")
    end
    local remote, remote_door = nil, nil
    for _, goal in ipairs(world.frontier) do
        if goal.kind == "door" and goal.room ~= room and not goal.target_visited
            and not blocked(bot, "door", goal.room, goal.slot, true)
            and (not goal.locked or key_opens(bot, goal.key)) then
            local steps = route(bot, world, room, goal.room)
            if steps and #steps > 0 and (not remote or #steps < #remote) then
                remote, remote_door = steps, goal
            end
        end
    end
    if remote then
        local door = BotNav.door_by_slot(bot, remote[1].slot)
        if door then
            return door_goal(bot, door, string.format(
                "toward the %s -> %s door, %d door(s)", remote_door.room,
                tostring(remote_door.target), #remote))
        end
    end
    if first then
        return door_goal(bot, first.door, "untraversed")
    end
    for _, goal in ipairs(world.frontier) do
        if goal.kind == "door" and goal.room ~= room
            and not blocked(bot, "door", goal.room, goal.slot, true)
            and (not goal.locked or key_opens(bot, goal.key)) then
            local steps = route(bot, world, room, goal.room)
            if steps and #steps > 0 and (not remote or #steps < #remote) then
                remote, remote_door = steps, goal
            end
        end
    end
    if remote then
        local door = BotNav.door_by_slot(bot, remote[1].slot)
        if door then
            return door_goal(bot, door, string.format(
                "toward the untraversed %s -> %s door", remote_door.room,
                tostring(remote_door.target)))
        end
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
