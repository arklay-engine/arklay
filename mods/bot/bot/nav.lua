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
-- Leaning into a zone: hold forward, facing it, at most this long while the
-- action probe falls short of it (the walk keeps a skin off furniture that
-- the collision resolver lets the player press into).
local LEAN_TICKS = C.SECOND

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

-- Whether the action probe point (rx, rz) is inside `zone`: both edges
-- count, as in the engine's box test.
function BotNav.reach_in(zone, rx, rz)
    return rx >= zone.x and rx <= zone.x + zone.width
        and rz >= zone.z and rz <= zone.z + zone.depth
end

local function clamp(value, lo, hi)
    if value < lo then
        return lo
    end
    if value > hi then
        return hi
    end
    return value
end

-- Whether a step forward would carry the action probe (rx, rz) of a player
-- at (px, pz) into `zone`: the probe is outside it, at most C.APPROACH
-- short on each axis, and the zone point nearest the probe lies ahead of it
-- along the facing (an overshoot lies behind and walking makes it worse).
function BotNav.reach_short(zone, px, pz, rx, rz)
    if BotNav.reach_in(zone, rx, rz) then
        return false
    end
    local nx = clamp(rx, zone.x, zone.x + zone.width)
    local nz = clamp(rz, zone.z, zone.z + zone.depth)
    if BotNav.abs(nx - rx) > C.APPROACH or BotNav.abs(nz - rz) > C.APPROACH then
        return false
    end
    return (nx - rx) * (rx - px) + (nz - rz) * (rz - pz) > 0
end

-- Whether a player at (px, pz) stands within a reach and an approach of
-- `zone` on both axes together: close enough that facing it and leaning in
-- may bring the action probe inside, though no approach point is walkable.
function BotNav.within_reach(zone, px, pz)
    local gx = BotNav.abs(px - clamp(px, zone.x, zone.x + zone.width))
    local gz = BotNav.abs(pz - clamp(pz, zone.z, zone.z + zone.depth))
    return gx + gz <= C.REACH + C.APPROACH
end

-- Offsets from a side's middle to stand at along it, for a side `span`
-- long. A long side also gets a quarter of the way in from each end: a wall
-- or a prop can block the middle of a side. A short side gets points a
-- quarter approach in from each end instead, still in front of the zone: a
-- narrow zone in a corner has its middle inside the wall's grown box, and
-- only its open end can be stood at (102's item beside the east wall).
function BotNav.side_offsets(span)
    local q = span // 4
    if q >= C.APPROACH // 2 then
        return { 0, -q, q }
    end
    local e = span // 2 - C.APPROACH // 4
    if e > 0 then
        return { 0, -e, e }
    end
    return { 0 }
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
    local along = BotNav.side_offsets
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
            job.lean = 0
            job.ticks = 0
            job.side_presses = 0
            bot:log(string.format("%s: at approach %d/%d (%d,%d)",
                job.label, job.index, #job.points, point[1], point[2]))
        elseif result.status == "unreachable"
            and (bot:distance(point[1], point[2]) <= C.APPROACH
                or (not job.zone.inside and BotNav.within_reach(job.zone, here.x, here.z))) then
            -- The exact point is inside grown geometry, but the player is
            -- already beside it or within a reach of the zone: press from
            -- here, leaning in if the probe falls short (a side that gets
            -- nothing times out as usual).
            job.pressing = true
            job.lean = 0
            job.ticks = 0
            job.side_presses = 0
            bot:log(string.format("%s: near approach %d/%d (%d,%d) from (%d,%d)",
                job.label, job.index, #job.points, point[1], point[2], here.x, here.z))
        elseif result.status ~= "moving" then
            bot:log(string.format("%s: approach %d/%d (%d,%d) %s from (%d,%d); next side",
                job.label, job.index, #job.points, point[1], point[2], result.status,
                here.x, here.z))
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
    -- The probe falls short (the walk stopped a skin off the furniture in
    -- front of the zone): hold forward into it, as a player would, before
    -- pressing.
    if facing.status == "facing" and not job.zone.inside
        and (job.lean or 0) < LEAN_TICKS
        and BotNav.reach_short(job.zone, here.x, here.z, here.reach_x, here.reach_z) then
        if job.lean == 0 then
            bot:log(string.format("%s: leaning in from (%d,%d), probe (%d,%d)", job.label,
                here.x, here.z, here.reach_x, here.reach_z))
        end
        job.lean = (job.lean or 0) + 1
        bot:set_input({ up = true })
        facing = { status = "leaning" }
    end
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

-- Push skills. A <push> fact names a pushable object and the zone (an AOT
-- slot, usually an object-position probe) its centre must reach. The push
-- is cardinal: the engine starts it after forward is held into the object
-- for nine frames and snaps the facing to the nearest axis, and the object
-- creeps on while forward stays held. So the skill fixes one axis at a time:
-- stand behind the object on that axis, face along it and hold forward until
-- the centre is in the middle half of the zone on that axis. An axis whose
-- push stalls twice (a wall, another object, an unreachable side) is given
-- up for the other one.
--
-- A push with a direction (`dir = {x, z}`, a compass step from the fact)
-- clears the zone instead: the object goes that way only, until its
-- trailing face has passed the far quarter of the zone on that axis (a
-- shelf moved off the item it hides).

function BotNav.object_by_slot(bot, slot)
    for _, object in ipairs(bot:objects()) do
        if object.slot == slot then
            return object
        end
    end
    return nil
end

-- Signed distance from `value` to the middle half of [lo, lo + span]:
-- positive means push towards +, 0 means inside.
local function band_error(value, lo, span)
    local a, b = lo + span // 4, lo + span - span // 4
    if value < a then
        return a - value
    elseif value > b then
        return b - value
    end
    return 0
end

-- How far the trailing face (centre `value`, half size `half`) is from
-- passing the far quarter of [lo, lo + span] going in `step` (+1 or -1).
local function clear_error(value, half, lo, span, step)
    if step > 0 then
        local goal = lo + span - span // 4
        local face = value - half
        return face < goal and goal - face or 0
    end
    local goal = lo + span // 4
    local face = value + half
    return face > goal and goal - face or 0
end

-- How far `object`'s centre is from the middle half of `zone`, per axis;
-- with `dir`, how far its trailing face is from clearing the zone that way.
function BotNav.push_error(object, zone, dir)
    if dir then
        local ex = dir.x ~= 0 and clear_error(object.x, object.half_x, zone.x, zone.width, dir.x) or 0
        local ez = dir.z ~= 0 and clear_error(object.z, object.half_z, zone.z, zone.depth, dir.z) or 0
        return ex, ez
    end
    return band_error(object.x, zone.x, zone.width), band_error(object.z, zone.z, zone.depth)
end

-- A push that moves the object less than this in PUSH_STALL ticks stalled.
local PUSH_MOVED = C.APPROACH // 6
local PUSH_STALL = 4 * C.SECOND
-- Stalls on one axis before the axis is given up.
local PUSH_TRIES = 2
-- How far behind the object's face to stand, nearest last: forward then
-- walks the player into the face. A wall close behind the object leaves
-- room only for the nearer points (the probe reaches 470 ahead).
local PUSH_STANDS = { C.REACH, C.APPROACH * 3 // 2, C.APPROACH * 4 // 3 }
-- Where along the face to stand, in thirds of the object's half size across
-- the push: the middle first, then either side (a wall band over one end).
local PUSH_LATERALS = { 0, -1, 1 }

-- One tick of a push job. `job` fields set by the caller: object (slot),
-- zone, label, done(bot, job), timeout, and optionally dir. Returns "done", "busy" or "failed"
-- (with job.reason).
function BotNav.push_object(bot, job)
    job.total = (job.total or 0) + 1
    if job.done and job.done(bot, job) then
        return "done"
    end
    -- A tick the job did not run (a fight, a heal, a message) is no stall:
    -- walk back behind the object and measure the push afresh.
    local now = bot:tick()
    if job.last_tick and now - job.last_tick > 1 and job.phase == "push" then
        job.phase = "approach"
        job.walk_ticks = 0
    end
    job.last_tick = now
    if job.total > (job.timeout or 3 * WALK_TICKS) then
        job.reason = "timed out"
        return "failed"
    end
    local object = BotNav.object_by_slot(bot, job.object)
    if not object or not object.pushable then
        job.reason = string.format("object %d is not a pushable object here", job.object)
        job.permanent = true
        return "failed"
    end
    local ex, ez = BotNav.push_error(object, job.zone, job.dir)
    job.tries = job.tries or { x = 0, z = 0 }
    local function error_on(axis)
        return axis == "x" and ex or ez
    end
    if not job.axis or error_on(job.axis) == 0 or job.tries[job.axis] >= PUSH_TRIES then
        local order = BotNav.abs(ex) >= BotNav.abs(ez) and { "x", "z" } or { "z", "x" }
        job.axis = nil
        for _, axis in ipairs(order) do
            if error_on(axis) ~= 0 and job.tries[axis] < PUSH_TRIES then
                job.axis = axis
                break
            end
        end
        if not job.axis then
            job.reason = string.format("object %d stalled short of the zone on every axis",
                job.object)
            return "failed"
        end
        job.phase = "approach"
        job.walk_ticks = 0
        bot:log(string.format("%s: object at (%d,%d), pushing along %s by %d",
            job.label, object.x, object.z, job.axis, error_on(job.axis)))
    end
    local axis = job.axis
    local sign = error_on(axis) > 0 and 1 or -1
    local here = bot:player()
    job.stand = job.stand or { x = 1, z = 1 }
    job.lateral = job.lateral or { x = 1, z = 1 }
    local back = PUSH_STANDS[job.stand[axis]]
    local sx, sz, fx, fz, along, across
    if axis == "x" then
        local line = object.z + PUSH_LATERALS[job.lateral[axis]] * object.half_z // 3
        sx, sz = object.x - sign * (object.half_x + back), line
        fx, fz = here.x + sign * C.REACH, here.z
        along, across = object.x, BotNav.abs(here.z - line) > object.half_z // 2
    else
        local line = object.x + PUSH_LATERALS[job.lateral[axis]] * object.half_x // 3
        sx, sz = line, object.z - sign * (object.half_z + back)
        fx, fz = here.x, here.z + sign * C.REACH
        along, across = object.z, BotNav.abs(here.x - line) > object.half_x // 2
    end
    local function stalled(why)
        job.tries[axis] = job.tries[axis] + 1
        bot:log(string.format("%s: %s along %s (try %d/%d)", job.label, why, axis,
            job.tries[axis], PUSH_TRIES))
        job.phase = "approach"
        job.walk_ticks = 0
    end
    if job.phase == "approach" then
        job.walk_ticks = job.walk_ticks + 1
        local result = bot:walk_to(sx, sz, {
            run = bot:distance(sx, sz) >= C.RUN_MIN_DISTANCE,
            tolerance = C.APPROACH // 3,
            face = { sx + (fx - here.x), sz + (fz - here.z) },
            avoid_enemies = BotTactics.avoiding(),
        })
        if result.status == "arrived" then
            job.phase = "push"
            job.mark, job.mark_tick = along, bot:tick()
        elseif result.status ~= "moving" or job.walk_ticks > WALK_TICKS then
            if job.stand[axis] < #PUSH_STANDS then
                job.stand[axis] = job.stand[axis] + 1
                job.walk_ticks = 0
            elseif job.lateral[axis] < #PUSH_LATERALS then
                job.stand[axis] = 1
                job.lateral[axis] = job.lateral[axis] + 1
                job.walk_ticks = 0
            else
                job.stand[axis] = 1
                job.lateral[axis] = 1
                stalled("stand point " .. result.status)
            end
        end
        return "busy"
    end
    -- Pushing: a player knocked off the line walks back behind the object.
    if across then
        job.phase = "approach"
        job.walk_ticks = 0
        return "busy"
    end
    if BotNav.abs(along - job.mark) >= PUSH_MOVED then
        job.mark, job.mark_tick = along, bot:tick()
    elseif bot:tick() - job.mark_tick > PUSH_STALL then
        stalled("the object did not move")
        return "busy"
    end
    if bot:face_to(fx, fz).status == "facing" then
        bot:set_input({ up = true })
    end
    return "busy"
end
