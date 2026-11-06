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
        -- Just inside each edge too: a long zone may reach into floor
        -- that is walled off, and only its near end is walkable (30B's
        -- corridor end runs on under the boulder).
        local i = C.APPROACH
        if zone.width > 4 * i then
            candidates[#candidates + 1] = { zone.x + i, cz }
            candidates[#candidates + 1] = { zone.x + zone.width - i, cz }
        end
        if zone.depth > 4 * i then
            candidates[#candidates + 1] = { cx, zone.z + i }
            candidates[#candidates + 1] = { cx, zone.z + zone.depth - i }
        end
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

-- The sides of a zone where a press got nothing, by room and zone: a later
-- job on the same zone tries that side's points last (401's drawer answers
-- from one side only, and every visit used to spend its whole budget on
-- points along the same dead side). Geometry, so it survives a reload.
local dead_sides = {}

local function zone_key(bot, zone)
    return string.format("%s:%d,%d,%d,%d", bot:room().id, zone.x, zone.z, zone.width, zone.depth)
end

-- The side of `zone` approach point `point` stands on ("inside" within it).
local function side_of(zone, point)
    if point[2] > zone.z + zone.depth then
        return "north"
    elseif point[2] < zone.z then
        return "south"
    elseif point[1] < zone.x then
        return "west"
    elseif point[1] > zone.x + zone.width then
        return "east"
    end
    return "inside"
end

-- `points` with the ones on a side a press got nothing on (dead_sides)
-- moved last.
local function live_first(bot, zone, points)
    local dead = dead_sides[zone_key(bot, zone)]
    if not dead then
        return points
    end
    local live, rest = {}, {}
    for _, point in ipairs(points) do
        if dead[side_of(zone, point)] then
            rest[#rest + 1] = point
        else
            live[#live + 1] = point
        end
    end
    for _, point in ipairs(rest) do
        live[#live + 1] = point
    end
    return live
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
        job.points = live_first(bot, job.zone, BotNav.approach_points(bot, job.zone))
        job.index = 1
        job.pressing = false
        job.arrived_at = nil
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
        job.arrived_at = nil
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
            job.arrived_at = { here.x, here.z }
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
            job.arrived_at = { here.x, here.z }
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
        local key = zone_key(bot, job.zone)
        dead_sides[key] = dead_sides[key] or {}
        dead_sides[key][side_of(job.zone, point)] = true
        -- The points still to try: the other sides first.
        local left = {}
        for i = job.index + 1, #job.points do
            left[#left + 1] = job.points[i]
        end
        left = live_first(bot, job.zone, left)
        for i, p in ipairs(left) do
            job.points[job.index + i] = p
        end
        job.index = job.index + 1
        job.pressing = false
        job.arrived_at = nil
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
-- shelf moved off the item it hides), or with `dir.full` its far edge.

function BotNav.object_by_slot(bot, slot)
    for _, object in ipairs(bot:objects()) do
        if object.slot == slot then
            return object
        end
    end
    return nil
end

-- The error an until="stop" push reports: farther than any room.
local PUSH_ONWARD = 1 << 16

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
-- passing the far quarter (with `full`, the far edge) of [lo, lo + span]
-- going in `step` (+1 or -1).
local function clear_error(value, half, lo, span, step, full)
    local margin = full and 0 or span // 4
    if step > 0 then
        local goal = lo + span - margin
        local face = value - half
        return face < goal and goal - face or 0
    end
    local goal = lo + margin
    local face = value + half
    return face > goal and goal - face or 0
end

-- How far `object`'s centre is from the middle half of `zone`, per axis
-- (only along `axis` when given); with `dir`, how far its trailing face is
-- from clearing the zone that way, and with `dir.until_stop` always some
-- way further (the push runs until it stalls).
function BotNav.push_error(object, zone, dir, axis)
    if dir and dir.until_stop then
        return dir.x * PUSH_ONWARD, dir.z * PUSH_ONWARD
    end
    if dir then
        local ex = dir.x ~= 0
            and clear_error(object.x, object.half_x, zone.x, zone.width, dir.x, dir.full) or 0
        local ez = dir.z ~= 0
            and clear_error(object.z, object.half_z, zone.z, zone.depth, dir.z, dir.full) or 0
        return ex, ez
    end
    local ex, ez = band_error(object.x, zone.x, zone.width), band_error(object.z, zone.z, zone.depth)
    return axis == "z" and 0 or ex, axis == "x" and 0 or ez
end

-- A push that moves the object less than this in PUSH_STALL ticks stalled.
local PUSH_MOVED = C.APPROACH // 6
local PUSH_STALL = 4 * C.SECOND
-- Stalls on one axis before the axis is given up.
local PUSH_TRIES = 2
-- How far behind the object's face to stand, nearest last: forward then
-- walks the player into the face. A wall close behind the object leaves
-- room only for the nearer points (the probe reaches 470 ahead; 406's
-- crate has a strip only 335 deep beside it).
local PUSH_STANDS = { C.REACH, C.APPROACH * 3 // 2, C.APPROACH * 4 // 3, C.APPROACH }
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
    local ex, ez = BotNav.push_error(object, job.zone, job.dir, job.only_axis)
    job.tries = job.tries or { x = 0, z = 0 }
    local function error_on(axis)
        return axis == "x" and ex or ez
    end
    if job.axis and error_on(job.axis) == 0 then
        -- The object moved: a stand point the other axis could not reach
        -- before may be reachable now (11B's crate, pushed east out of the
        -- corner before it can be pushed north).
        job.tries = { x = 0, z = 0 }
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
        if job.dir and job.dir.until_stop then
            -- Pushed into a wall: the step's goal.
            bot:log(string.format("%s: object stopped at (%d,%d)", job.label, object.x, object.z))
            return "done"
        end
        stalled("the object did not move")
        return "busy"
    end
    if bot:face_to(fx, fz).status == "facing" then
        bot:set_input({ up = true })
    end
    return "busy"
end

local CLIMB_HEIGHT = C.CLIMB_HEIGHT
local CLIMB_STAND = C.CLIMB_STAND
local CLIMB_INSET = C.CLIMB_INSET
-- Ticks to wait for a vault press to lift (or drop) the player.
local CLIMB_WAIT = 2 * C.SECOND
-- Presses per phase before it fails.
local CLIMB_PRESSES = 4

function BotNav.on_top(bot)
    return bot:player().y <= -CLIMB_HEIGHT
end

-- Whether the box of `object` (its centre and half extents) overlaps `zone`.
local function covers(object, zone)
    return object.x - object.half_x < zone.x + zone.width
        and zone.x < object.x + object.half_x
        and object.z - object.half_z < zone.z + zone.depth
        and zone.z < object.z + object.half_z
end

-- Where to climb `object` to reach `zone` beyond it: the stand point
-- behind the face away from the zone, on the axis the zone lies further
-- along (in half extents), across at the middle of the object's and the
-- zone's common span; and a point ahead (toward the zone) to face.
function BotNav.climb_point(object, zone)
    local zx, zz = zone.x + zone.width // 2, zone.z + zone.depth // 2
    local dx, dz = zx - object.x, zz - object.z
    local along_x = BotNav.abs(dx) * object.half_z > BotNav.abs(dz) * object.half_x
    local function across(centre, half, lo, size)
        local a, b = centre - half + CLIMB_INSET, centre + half - CLIMB_INSET
        if lo > a then a = lo end
        if lo + size < b then b = lo + size end
        if a > b then
            return centre
        end
        return (a + b) // 2
    end
    if along_x then
        local sign = dx > 0 and 1 or -1
        local z = across(object.z, object.half_z, zone.z, zone.depth)
        return { x = object.x - sign * (object.half_x + CLIMB_STAND), z = z },
            { x = zx, z = z }, { x = object.x - sign * 4 * object.half_x, z = z }
    end
    local sign = dz > 0 and 1 or -1
    local x = across(object.x, object.half_x, zone.x, zone.width)
    return { x = x, z = object.z - sign * (object.half_z + CLIMB_STAND) },
        { x = x, z = zz }, { x = x, z = object.z - sign * 4 * object.half_z }
end

-- The reachable spot nearest the player by path between `min` and `max`
-- from `target` ({x, z}), from rings of points around it. Nil when none is
-- reachable.
function BotNav.stand_near(bot, target, min, max)
    local best, best_length
    local inner = min + (max - min) // 3
    for _, r in ipairs({ max - C.APPROACH, (inner + max) // 2, inner }) do
        if r > min then
            for _, dir in ipairs(C.RING) do
                local x = target.x + r * dir[1] // C.RING[1][1]
                local z = target.z + r * dir[2] // C.RING[1][1]
                local path = bot:path(x, z)
                if path.status == "ok" and (not best_length or path.length < best_length) then
                    best, best_length = { x = x, z = z }, path.length
                end
            end
        end
    end
    return best
end

-- One tick of a fight job: a <fight> step holds the room until its enemy's
-- done flag is set (a boss killed). Walks toward the job's target (the
-- tactics layer prefers it, BotTactics.focus) until it is FIGHT_RANGE away,
-- inside the knowledge band where the tactics layer engages, then faces it:
-- the tactics layer engages only what is in front. Returns "done" or
-- "busy"; the planner's job.timeout ends it.
local FIGHT_RANGE = C.FIGHT_RANGE
function BotNav.hold_fight(bot, job)
    if job.done(bot) then
        return "done"
    end
    -- The target: the part first picked while it lives, else the one of
    -- the job's id with the most health (a boss's body over its escort).
    -- A monster of parts (knowledge parts="yes", Plant 42) is the other way
    -- round: its vines first, the weakest live part, since shooting them
    -- is what ends it (Barry burns the plant) and the body soaked up eight
    -- launcher rounds in a run.
    local k = bot:knowledge()
    local entry = k and k.enemies[job.enemy]
    local weakest = entry ~= nil and entry.parts == true
    local target
    for _, e in ipairs(bot:enemies()) do
        if e.id == job.enemy and e.health > 0 then
            if e.slot == job.slot then
                target = e
                break
            end
            if not target or (weakest and e.health < target.health)
                or (not weakest and e.health > target.health) then
                target = e
            end
        end
    end
    if not target then
        return "busy"
    end
    if target.slot ~= job.slot then
        job.slot = target.slot
        BotTactics.focus(target.slot)
        bot:log(string.format("%s: target slot %d (health %d)", job.label, target.slot,
            target.health))
    end
    -- A target fought by hand (the web, with the knife) is walked up to a
    -- reachable spot inside the knife's band; any other from the fight
    -- range.
    local k = bot:knowledge() or {}
    local entry = k.enemies and k.enemies[job.enemy]
    local hand = entry and entry.prefer and k.weapons[entry.prefer]
    local band = hand and hand.melee and bot:has_item(entry.prefer) and entry.keep[entry.prefer]
    if band then
        if target.distance > band.max then
            job.stand = job.stand or BotNav.stand_near(bot, target, band.min, band.max)
            if job.stand then
                local result = bot:walk_to(job.stand.x, job.stand.z, {
                    tolerance = C.APPROACH // 2, face = { x = target.x, z = target.z },
                })
                if result.status == "moving" then
                    return "busy"
                end
                if result.status ~= "arrived" then
                    job.stand = nil
                end
            end
        end
        bot:face_to(target.x, target.z)
        return "busy"
    end
    local range = FIGHT_RANGE
    if target.distance > range + C.APPROACH then
        local p = bot:player()
        local x = target.x + (p.x - target.x) * range // target.distance
        local z = target.z + (p.z - target.z) * range // target.distance
        local result = bot:walk_to(x, z, {
            tolerance = C.APPROACH, face = { x = target.x, z = target.z },
        })
        if result.status == "moving" then
            return "busy"
        end
    end
    bot:face_to(target.x, target.z)
    return "busy"
end

-- One tick of a climb job: an item zone (a high shelf) reachable only from
-- on top of a pushable object. `job` fields set by the caller: object
-- (slot), zone, label, taken(bot) (the item is gone), timeout. Push the
-- object toward the zone until it covers the zone and stalls (against the
-- wall under the shelf), climb it from the side away from the zone, press
-- toward the zone until the item is taken (the scene handler answers the
-- pickup prompt), then climb down the far side. With job.placed (an earlier
-- <push> step of the entry left the object there) an object already over
-- the zone is climbed at once. Returns "done", "busy" or "failed" (with
-- job.reason).
function BotNav.climb_object(bot, job)
    job.total = (job.total or 0) + 1
    local top = BotNav.on_top(bot)
    local taken = job.taken(bot)
    if taken and not top then
        return "done"
    end
    if job.total > (job.timeout or 6 * WALK_TICKS) then
        job.reason = "timed out"
        return "failed"
    end
    local object = BotNav.object_by_slot(bot, job.object)
    if not object then
        job.reason = string.format("object %d is not here", job.object)
        job.permanent = true
        return "failed"
    end
    local now = bot:tick()
    -- A press phase: press, wait for `moved` (or the wait to run out), press
    -- again, at most CLIMB_PRESSES times.
    local function press_until(phase, aim, moved)
        if job.phase ~= phase then
            job.phase, job.presses, job.press_tick = phase, 0, nil
            bot:log(string.format("%s: %s", job.label, phase))
        end
        if moved() then
            return true
        end
        if job.press_tick and now - job.press_tick < CLIMB_WAIT then
            return false
        end
        if job.presses >= CLIMB_PRESSES then
            job.reason = string.format("%s: no effect after %d presses", phase, job.presses)
            return nil
        end
        if bot:face_to(aim.x, aim.z).status == "facing" then
            bot:press_action()
            job.presses, job.press_tick = job.presses + 1, now
        end
        return false
    end
    local stand, ahead, behind = BotNav.climb_point(object, job.zone)
    if top then
        local aim = taken and behind or ahead
        local result = press_until(taken and "climb down" or "take from the top", aim,
            function() return not taken and job.taken(bot) or (taken and not BotNav.on_top(bot)) end)
        if result == nil then
            return "failed"
        end
        return "busy"
    end
    if not job.pushed and job.placed and covers(object, job.zone) then
        job.pushed = true
        job.walk_ticks = 0
    end
    if not job.pushed then
        job.shove = job.shove or {
            object = job.object, zone = job.zone, label = job.label,
            timeout = 3 * WALK_TICKS,
            done = function(b, push)
                local o = BotNav.object_by_slot(b, job.object)
                return o ~= nil and covers(o, job.zone)
                    and (push.tries ~= nil and push.tries.x + push.tries.z > 0)
            end,
        }
        local status = BotNav.push_object(bot, job.shove)
        if status == "busy" then
            return "busy"
        end
        if not covers(object, job.zone) then
            job.reason = job.shove.reason or "the push left the object short of the zone"
            job.permanent = job.shove.permanent
            return "failed"
        end
        job.pushed = true
        job.walk_ticks = 0
        bot:log(string.format("%s: object at (%d,%d) covers the zone; climb from (%d,%d)",
            job.label, object.x, object.z, stand.x, stand.z))
    end
    if job.phase ~= "climb up" then
        job.walk_ticks = (job.walk_ticks or 0) + 1
        local result = bot:walk_to(stand.x, stand.z, {
            tolerance = C.APPROACH // 3, face = ahead,
            avoid_enemies = BotTactics.avoiding(),
        })
        if result.status ~= "arrived" then
            if result.status ~= "moving" or job.walk_ticks > WALK_TICKS then
                job.reason = "climb stand point " .. result.status
                return "failed"
            end
            return "busy"
        end
    end
    if press_until("climb up", ahead, function() return BotNav.on_top(bot) end) == nil then
        return "failed"
    end
    return "busy"
end
