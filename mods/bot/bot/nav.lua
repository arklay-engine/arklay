-- Zone navigation skills for the player bot.
--
-- Everything the planner presses (a door, a ground item, a room interact,
-- a stair check) is an action zone: walk to a point just outside one side of
-- the zone, face the zone centre and press action. The approach points come
-- from the zone itself: each side at a half reach, a quarter reach and three
-- quarters of a reach outside the edge and a quarter reach inside it, at the
-- middle of the side and (long sides) a quarter in from each end; the points
-- the engine's path planner reaches come first, nearest path first. A side that does not fire is
-- abandoned for the next one. A zone that probes the player's own position
-- (`inside`) is entered instead: its centre and the quarter points inside.
-- No room literals; only pad input.

BotNav = BotNav or {}

local C = BotConst

-- Walking to one approach point gives up after this many planner ticks.
local WALK_TICKS = 15 * C.SECOND
-- A point the path planner does not reach gets a short try, and only the
-- nearest few of them are tried.
local UNPLANNED_WALK_TICKS = 5 * C.SECOND
local UNPLANNED_TRIES = 6

function BotNav.abs(value)
    if value < 0 then
        return -value
    end
    return value
end

function BotNav.action_by_slot(bot, slot)
    local actions = bot:actions()
    for i = 1, #actions do
        if actions[i].slot == slot then
            return actions[i]
        end
    end
    return nil
end

function BotNav.door_by_slot(bot, slot)
    local doors = bot:doors()
    for i = 1, #doors do
        if doors[i].slot == slot then
            return doors[i]
        end
    end
    return nil
end

-- A zone at the room origin no larger than a few units is a scripted
-- handover: outside the walkable floor, only the room script fires it.
function BotNav.is_origin_zone(zone)
    return zone.x == 0 and zone.z == 0 and zone.width <= 32 and zone.depth <= 32
end

function BotNav.centre(zone)
    return zone.x + zone.width // 2, zone.z + zone.depth // 2
end

-- Candidate standing points around `zone`, sorted: the ones the engine's
-- path planner reaches first, by path length, then the rest by straight
-- distance (the 512-unit planning grid can miss a point next to a wall that
-- the walk still reaches). Each row is {x, z, fx, fz, length, planned}.
function BotNav.approach_points(bot, zone)
    local cx, cz = BotNav.centre(zone)
    -- Outside the edge at three depths, plus one just inside it (the zone
    -- may cover the only walkable floor, as when a door zone holds the
    -- entry point): the forward reach still lands inside the zone.
    local offsets = { C.APPROACH, C.APPROACH // 2, C.APPROACH + C.APPROACH // 2,
        -(C.APPROACH // 2) }
    -- Along a long side, also try a quarter of the way in from each end: a
    -- wall or a prop can block the middle of a side.
    local function along(span)
        local q = span // 4
        if q >= C.APPROACH // 2 then
            return { 0, -q, q }
        end
        return { 0 }
    end
    local planned, rest = {}, {}
    local candidates = {}
    if zone.inside then
        offsets = {}
        candidates = {
            { cx, cz },
            { zone.x + zone.width // 4, cz }, { zone.x + zone.width - zone.width // 4, cz },
            { cx, zone.z + zone.depth // 4 }, { cx, zone.z + zone.depth - zone.depth // 4 },
        }
    end
    for _, d in ipairs(offsets) do
        for _, a in ipairs(along(zone.depth)) do
            candidates[#candidates + 1] = { zone.x - d, cz + a }
            candidates[#candidates + 1] = { zone.x + zone.width + d, cz + a }
        end
        for _, a in ipairs(along(zone.width)) do
            candidates[#candidates + 1] = { cx + a, zone.z - d }
            candidates[#candidates + 1] = { cx + a, zone.z + zone.depth + d }
        end
    end
    for _, p in ipairs(candidates) do
        local path = bot:path(p[1], p[2])
        if path.status == "ok" then
            planned[#planned + 1] = { p[1], p[2], cx, cz, path.length, true }
        else
            rest[#rest + 1] = { p[1], p[2], cx, cz, bot:distance(p[1], p[2]), false }
        end
    end
    local function order(a, b)
        if a[5] ~= b[5] then
            return a[5] < b[5]
        end
        if a[1] ~= b[1] then
            return a[1] < b[1]
        end
        return a[2] < b[2]
    end
    table.sort(planned, order)
    table.sort(rest, order)
    for i = 1, UNPLANNED_TRIES do
        planned[#planned + 1] = rest[i]
    end
    return planned
end

-- The shortest planned approach length to `zone`, or nil when the path
-- planner reaches no approach point (the walk may still get there).
function BotNav.reach_length(bot, zone)
    local point = BotNav.approach_points(bot, zone)[1]
    return point and point[6] and point[5] or nil
end

-- One tick of a zone press job. `job` fields set by the caller:
--   zone      {x, z, width, depth}
--   label     log label
--   done      function(bot, job) -> true once the press worked
--   pattern   "door" (press two ticks, release two), "edge" (one edge per
--             `gap` ticks, at most `presses` per side) or "hold" (no press:
--             stand still, facing the zone, and call `hold(bot, job)` each
--             tick; a use-item probe needs only the reach point in the zone)
--   side      ticks to spend pressing on one side
--   timeout   overall budget in ticks
--   check     optional function(bot, job) -> a failure reason, or nil; while
--             it sets `job.settled`, the job stays put and spends no side or
--             overall budget (a menu use whose room event is running)
-- Returns "done", "busy" or "failed" (with job.reason).
function BotNav.press_zone(bot, job)
    if not job.points then
        job.points = BotNav.approach_points(bot, job.zone)
        job.index = 1
        job.pressing = false
        job.ticks = 0
        job.total = 0
        job.side_presses = 0
        job.last_press = -C.SECOND * C.SECOND
        bot:log(string.format("%s: zone=(%d,%d,%d,%d) %d reachable approaches",
            job.label, job.zone.x, job.zone.z, job.zone.width, job.zone.depth,
            #job.points))
    end
    job.total = job.total + 1
    if job.done and job.done(bot, job) then
        return "done"
    end
    if job.check then
        local why = job.check(bot, job)
        if why then
            job.reason = why
            return "failed"
        end
        if job.settled then
            job.total = job.total - 1
            return "busy"
        end
    end
    -- A scripted move (a scene that places the player) makes the approach
    -- list stale: plan it again from the new position.
    local here = bot:player()
    if job.last_pos and BotNav.abs(here.x - job.last_pos[1])
        + BotNav.abs(here.z - job.last_pos[2]) > C.WARP_DISTANCE then
        bot:log(string.format("%s: moved by a scene; replanning approaches", job.label))
        job.points = BotNav.approach_points(bot, job.zone)
        job.index = 1
        job.pressing = false
        job.walk_ticks = 0
    end
    job.last_pos = { here.x, here.z }
    if job.total > (job.timeout or 40 * C.SECOND) then
        job.reason = "timed out"
        return "failed"
    end
    local point = job.points[job.index]
    if not point then
        job.reason = #job.points == 0 and "no reachable approach"
            or "every approach failed"
        return "failed"
    end
    if not job.pressing then
        job.walk_ticks = (job.walk_ticks or 0) + 1
        if job.walk_ticks > (point[6] and WALK_TICKS or UNPLANNED_WALK_TICKS) then
            bot:log(string.format("%s: approach %d/%d not reached in %d ticks; next side",
                job.label, job.index, #job.points, WALK_TICKS))
            job.index = job.index + 1
            job.walk_ticks = 0
            return "busy"
        end
        local result = bot:walk_to(point[1], point[2], {
            run = bot:distance(point[1], point[2]) >= C.RUN_MIN_DISTANCE,
            tolerance = C.APPROACH // 2 - 30,
            face = { point[3], point[4] },
            avoid_enemies = BotTactics.avoiding(),
        })
        if result.status == "arrived" then
            job.pressing = true
            job.ticks = 0
            job.side_presses = 0
            bot:log(string.format("%s: at approach %d/%d (%d,%d)",
                job.label, job.index, #job.points, point[1], point[2]))
        elseif result.status ~= "moving" then
            bot:log(string.format("%s: approach %d/%d %s; next side",
                job.label, job.index, #job.points, result.status))
            job.index = job.index + 1
            job.walk_ticks = 0
            return "busy"
        else
            return "busy"
        end
    end
    job.ticks = job.ticks + 1
    if job.pattern == "hold" then
        local facing = job.zone.inside and { status = "facing" }
            or bot:face_to(point[3], point[4])
        if facing.status == "facing" and job.hold then
            job.hold(bot, job)
        end
    end
    local facing = job.pattern ~= "hold" and bot:face_to(point[3], point[4])
        or { status = "hold" }
    if facing.status == "facing" and BotScene.action_ready(bot) then
        if job.pattern == "door" then
            if bot:tick() % 4 < 2 then
                bot:press_action()
                job.pressed = true
            end
        elseif job.side_presses < (job.presses or 1)
            and job.ticks - job.last_press >= (job.gap or 20) then
            job.side_presses = job.side_presses + 1
            job.last_press = job.ticks
            job.pressed = true
            job.press_tick = bot:tick()
            bot:press_action()
        end
    end
    if job.ticks > (job.side or 4 * C.SECOND) then
        bot:log(string.format("%s: nothing on approach %d; next side",
            job.label, job.index))
        job.index = job.index + 1
        job.pressing = false
        job.walk_ticks = 0
        job.last_press = -C.SECOND * C.SECOND
    end
    return "busy"
end

-- One tick of a camera visit job: walk into a camera switch zone that cuts
-- to `job.camera`, nearest reachable zone first, until the room's camera is
-- that cut; then wait (a scene usually starts) for `job.done`. Returns
-- "done", "busy" or "failed" (with job.reason).
function BotNav.visit_camera(bot, job)
    job.total = (job.total or 0) + 1
    if bot:room().camera == job.camera then
        job.reached = true
    end
    if job.done and job.done(bot, job) then
        return "done"
    end
    if job.reached then
        -- In the cut: give the room a moment to start its scene.
        job.wait = (job.wait or 0) + 1
        if job.wait > 10 * C.SECOND then
            job.reason = "entered the cut; its flag stayed clear"
            return "failed"
        end
        if bot:room().camera ~= job.camera then
            job.reached = false
        end
        return "busy"
    end
    if not job.points then
        job.points = {}
        for _, zone in ipairs(bot:cameras()) do
            if zone.to == job.camera then
                local path = bot:path(zone.x, zone.z)
                if path.status == "ok" then
                    job.points[#job.points + 1] = { zone.x, zone.z, path.length }
                end
            end
        end
        table.sort(job.points, function(a, b) return a[3] < b[3] end)
        job.index = 1
        job.walk_ticks = 0
        bot:log(string.format("%s: %d reachable switch zone(s) into cut %d",
            job.label, #job.points, job.camera))
    end
    local point = job.points[job.index]
    if not point then
        job.reason = #job.points == 0 and "no reachable approach"
            or "no switch zone changed the camera"
        return "failed"
    end
    job.walk_ticks = job.walk_ticks + 1
    local result = bot:walk_to(point[1], point[2], {
        run = bot:distance(point[1], point[2]) >= C.RUN_MIN_DISTANCE,
        tolerance = C.APPROACH // 2,
        avoid_enemies = BotTactics.avoiding(),
    })
    if result.status ~= "moving" or job.walk_ticks > WALK_TICKS then
        bot:log(string.format("%s: switch zone %d/%d %s; camera %d",
            job.label, job.index, #job.points, result.status, bot:room().camera))
        job.index = job.index + 1
        job.walk_ticks = 0
    end
    return "busy"
end
