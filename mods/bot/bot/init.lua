-- The RE1 Jill opening route for the Arklay player bot.
--
-- The script owns route policy only. It drives the same pad a human would:
-- skip the opening scenes, walk with the engine's collision-aware navigation,
-- face doors and press action. It cannot author flags, items or positions;
-- the one scripted-handover skill it uses is the room's own item action.
--
-- Route:
--   106 opening scene -> scripted walk-in to 105
--   105 dining room: the AOT_02 event starts the first Barry scene, then the
--       north door (AOT_00) leads to the tea room
--   104 tea room: walk west to camera 4, which starts the first-zombie FMV
--       (F_COMMON_02 is set after it); return to the south door (AOT_03)
--   105 return: skip the entry scene, cross the dining room to the east door
--   106 main hall: the return triggers Barry's lockpick scene (on_enter_3).
--       Walk to camera 3/4 and then camera 2/5; that plays the rest of the
--       hall scene and restores the east-wing door. The lockpick item is
--       registered at the room origin, outside the walkable floor; the route
--       runs that item action, closes the got-item viewer and waits until
--       has_item(0x31) is true.
--   107 gallery: take the ink ribbon (0x2F) from the south gallery when the
--       build registers it (jill_ink_ribbons on), then open the sword-key door
--       (AOT_01) to 108. Jill's lockpick substitutes for the sword key
--       (scenario flag 0x7C).
--   108 L passage: the handgun ammo (0x0B) zone is inside the solid table at
--       (21605,13970); its collision leaves the 600-unit reach short on every
--       side, so log the block and take the south door (AOT_01) to 109.
--   109 L passage: take the green herb (0x44), then open the west door
--       (AOT_04) to 10A and stop.
--
-- Every phase has a tick deadline. A blocked walk advances to the next
-- waypoint or door approach; a phase that cannot make progress fails with a
-- precise log instead of spinning until --bot-max-ticks.

-- The route state is a global so a `--watch` reload of this chunk keeps its
-- progress. Re-running the chunk reuses the same table instead of starting the
-- route over.
BOT_STATE = BOT_STATE or {
    phase = "boot",
    phase_ticks = 0,
    room = nil,
    room_ticks = 0,
    wp = 1,
    wp_retry = false,
    wp_tol = nil,
    nav = nil,
    press_ticks = 0,
    saw_scene = false,
    saw_film = false,
    scene_sig = nil,
    transition_start = nil,
    controllable_ticks = 0,
    locked_ticks = 0,
    tried_203 = false,
    cam_stage = "north",
    cam_wait = 0,
    cam_visited_34 = false,
    west_wait = 0,
    barry_scene_logged = false,
    barry_done_ticks = 0,
    saw_pickup = false,
    did_117 = false,
    did_11a = false,
    puzzle_index = 1,
    puzzle_failed = false,
    milestone = nil,
}

-- Fields added after the first checked-in route. A --watch reload keeps the
-- old table, so fill any field that the previous chunk did not define.
BOT_STATE.saw_pickup = BOT_STATE.saw_pickup or false
BOT_STATE.did_117 = BOT_STATE.did_117 or false
BOT_STATE.did_11a = BOT_STATE.did_11a or false
BOT_STATE.puzzle_index = BOT_STATE.puzzle_index or 1
BOT_STATE.puzzle_failed = BOT_STATE.puzzle_failed or false
BOT_STATE.milestone = BOT_STATE.milestone or nil

-- Flag banks: FG_COMMON is 1, FG_ROOM is 4, FG_STATUS is 5, FG_ITEM is 7.
local BANK_COMMON = 1
local BANK_STATUS = 5
local BANK_ITEM = 7

-- Short final approaches walk: running into a waypoint or door overshoots and
-- forces a turn-back. The Rust walk_to walks the last 400 units anyway; keep
-- this gate close to that so the route only asks for a run when it helps.
local RUN_MIN_DISTANCE = 600

local F_COMMON_02 = 0x02 -- 104 FMV seen
local F_COMMON_03 = 0x03 -- first 106 Barry scene started
local F_COMMON_05 = 0x05 -- 106 camera scenes done
local F_STATUS_1A = 0x1A
local F_STATUS_1C = 0x1C
local F_ITEM_LOCKPICK = 0x09
local ITEM_LOCKPICK = 0x31
local ITEM_SWORD_KEY = 0x33
local ITEM_INK_RIBBON = 0x2F
local ITEM_HANDGUN_AMMO = 0x0B
local ITEM_GREEN_HERB = 0x44
local ITEM_CRANK_KEY = 0x1D
local ITEM_STAR_CREST = 0x2D
local ITEM_DESK_KEY = 0x3D
local ITEM_RADIO_KEY = 0x4D
local SCENARIO_HAS_LOCKPICK = 0x7C

-- 117 large gallery: the seven painting switches must be pushed in the order
-- below. Each painting clears the room bits 0x18..0x1E and leaves only its own
-- bit, and the next one requires that bit (it is the picture chronology:
-- newborn, infant, lively boy, young man, tired middle-aged man, bold old man,
-- people mourning). Pushing the last one sets F_SCENARIO_03, opens the frame
-- and puts Jill beside the star crest.
local F_SCENARIO_03 = 0x03
local F_SCENARIO_69 = 0x69
local F_SCENARIO_6A = 0x6A
local F_SCENARIO_6B = 0x6B
local F_SCENARIO_6C = 0x6C
local F_LOCK_17 = 0x17
local F_ROOM_19 = 0x19
local F_ROOM_1A = 0x1A
local F_ROOM_1B = 0x1B
local F_ROOM_1C = 0x1C
local F_ROOM_1D = 0x1D
local F_ROOM_1E = 0x1E

-- A ground item pickup gets this many controllable ticks on one side before
-- the route walks around to the next side. The item viewer itself runs while
-- the scene is not controllable, so those ticks are not counted here.
local ACTION_SIDE_TICKS = 60
-- Overall budget for one action_nav call, in controllable ticks.
local ACTION_TIMEOUT_TICKS = 1200

local function log(bot, text)
    bot:log(text)
end

local function flag_common(bot, bit)
    return bot:flag(BANK_COMMON, bit)
end

local function item_action_slot(bot, item)
    local actions = bot:actions()
    for i = 1, #actions do
        if actions[i].kind == "item" and actions[i].item == item then
            return actions[i].slot
        end
    end
    return nil
end

local function action_by_slot(bot, slot)
    local actions = bot:actions()
    for i = 1, #actions do
        if actions[i].slot == slot then
            return actions[i]
        end
    end
    return nil
end

local function has_lockpick_action(bot)
    return item_action_slot(bot, ITEM_LOCKPICK) ~= nil
end

local function door_by_target(bot, target)
    local doors = bot:doors()
    for i = 1, #doors do
        if doors[i].target == target then
            return doors[i]
        end
    end
    return nil
end

local function door_action_kind(bot, target)
    local door = door_by_target(bot, target)
    if not door then
        return nil
    end
    local actions = bot:actions()
    for i = 1, #actions do
        if actions[i].slot == door.slot then
            return actions[i].kind
        end
    end
    return nil
end

local function fail(bot, reason)
    local p = bot:player()
    local room = bot:room()
    local scene = bot:scene()
    log(bot, string.format(
        "FAIL phase=%s reason=%s room=%s pos=(%d,%d) camera=%d health=%d/%d "
            .. "collision=%d common02=%s common03=%s common05=%s "
            .. "common64=%s common65=%s status1a=%s lockpick=%s "
            .. "scenario7c=%s scenario03=%s scenario69=%s scenario6a=%s "
            .. "scenario6b=%s scenario6c=%s lock17=%s "
            .. "items(ribbon=%d ammo=%d herb=%d star=%d radio=%d desk=%d crank=%d) "
            .. "scene(film=%s trans=%s skip=%s skippable=%s msg=%s ctrl=%s frozen=%s)",
        BOT_STATE.phase, reason, room.id, p.x, p.z, room.camera, p.health, p.max_health,
        p.collision_flags,
        tostring(flag_common(bot, F_COMMON_02)),
        tostring(flag_common(bot, F_COMMON_03)),
        tostring(flag_common(bot, F_COMMON_05)),
        tostring(flag_common(bot, 0x40)),
        tostring(flag_common(bot, 0x41)),
        tostring(bot:flag(BANK_STATUS, F_STATUS_1A)),
        tostring(bot:has_item(ITEM_LOCKPICK)),
        tostring(bot:flag(0, SCENARIO_HAS_LOCKPICK)),
        tostring(bot:flag(0, F_SCENARIO_03)),
        tostring(bot:flag(0, F_SCENARIO_69)),
        tostring(bot:flag(0, F_SCENARIO_6A)),
        tostring(bot:flag(0, F_SCENARIO_6B)),
        tostring(bot:flag(0, F_SCENARIO_6C)),
        tostring(bot:flag(2, F_LOCK_17)),
        bot:item_count(ITEM_INK_RIBBON),
        bot:item_count(ITEM_HANDGUN_AMMO),
        bot:item_count(ITEM_GREEN_HERB),
        bot:item_count(ITEM_STAR_CREST),
        bot:item_count(ITEM_RADIO_KEY),
        bot:item_count(ITEM_DESK_KEY),
        bot:item_count(ITEM_CRANK_KEY),
        tostring(scene.film), tostring(scene.transition), tostring(scene.cutscene_skip),
        tostring(scene.skippable), tostring(scene.message), tostring(scene.controllable),
        tostring(scene.frozen)
    ))
    bot:finish("failed: " .. reason)
end

local function reset_phase_data()
    BOT_STATE.wp = 1
    BOT_STATE.wp_retry = false
    BOT_STATE.wp_tol = nil
    BOT_STATE.nav = nil
    BOT_STATE.press_ticks = 0
    BOT_STATE.saw_scene = false
    BOT_STATE.saw_film = false
    BOT_STATE.controllable_ticks = 0
    BOT_STATE.locked_ticks = 0
    BOT_STATE.cam_stage = "north"
    BOT_STATE.cam_wait = 0
    BOT_STATE.cam_visited_34 = false
    BOT_STATE.west_wait = 0
    BOT_STATE.barry_scene_logged = false
    BOT_STATE.barry_done_ticks = 0
    BOT_STATE.saw_pickup = false
end

local function set_phase(bot, phase, note)
    if BOT_STATE.phase == phase then
        return
    end
    if note then
        log(bot, string.format("phase %s -> %s (%s)", BOT_STATE.phase, phase, note))
    else
        log(bot, string.format("phase %s -> %s", BOT_STATE.phase, phase))
    end
    BOT_STATE.phase = phase
    BOT_STATE.phase_ticks = 0
    reset_phase_data()
end

-- Scene handling. Returns true when it consumed the tick and the route must
-- not move. A cutscene skip needs a press/release edge, so presses alternate.
local function handle_scene(bot, scene)
    if scene.dead then
        fail(bot, "player died")
        return true
    end
    local tick = bot:tick()
    if not scene.transition then
        BOT_STATE.transition_start = nil
    end
    if scene.film then
        if tick % 2 == 0 then
            bot:pulse_confirm()
        end
        return true
    end
    if scene.transition then
        -- The transition's skip gate opens on animation frame 11, and a
        -- button already down when the transition begins cannot skip it. Use
        -- transition-relative parity: release on the first tick, then press on
        -- frames 1, 3, 5, ... so the press on frame 11 is the first allowed
        -- one whatever tick the transition started on.
        if not BOT_STATE.transition_start then
            BOT_STATE.transition_start = tick
        end
        if (tick - BOT_STATE.transition_start) % 2 == 1 then
            bot:press_action()
        end
        return true
    end
    if scene.cutscene_skip then
        return true
    end
    if scene.skippable then
        if tick % 2 == 0 then
            bot:press_action()
        end
        return true
    end
    if scene.message_takes_input then
        -- A page wait or a yes/no prompt needs a fresh press edge; holding
        -- the button every tick can leave the window waiting forever.
        if tick % 2 == 0 then
            bot:press_action()
        end
        return true
    end
    if scene.pickup then
        -- The got-item viewer owns the frozen frame. It closes on the
        -- action/confirm edge and awards the armed room item.
        BOT_STATE.saw_pickup = true
        if tick % 2 == 0 then
            bot:press_action()
        else
            bot:pulse_confirm()
        end
        return true
    end
    if scene.frozen then
        -- Item viewers and other full-screen UI close on action/confirm.
        if tick % 2 == 0 then
            bot:press_action()
        else
            bot:pulse_confirm()
        end
        return true
    end
    local player = bot:player()
    if player.being_attacked and player.being_attacked > 0 then
        local dirs = { "left", "right", "up", "down" }
        bot:press_action()
        bot:set_input({ [dirs[tick % 4 + 1]] = true })
        return true
    end
    return false
end

local function scene_signature(scene)
    return string.format(
        "film=%s trans=%s skip=%s skippable=%s msg=%s input=%s pickup=%s frozen=%s ctrl=%s",
        tostring(scene.film), tostring(scene.transition), tostring(scene.cutscene_skip),
        tostring(scene.skippable), tostring(scene.message),
        tostring(scene.message_takes_input), tostring(scene.pickup),
        tostring(scene.frozen), tostring(scene.controllable)
    )
end

local function walk_waypoints(bot, points, tolerance, label)
    local index = BOT_STATE.wp or 1
    if index > #points then
        return true
    end
    local point = points[index]
    local tol = BOT_STATE.wp_tol or tolerance
    local result = bot:walk_to(point[1], point[2], {
        run = bot:distance(point[1], point[2]) >= RUN_MIN_DISTANCE,
        tolerance = tol,
    })
    if result.status == "moving" then
        return false
    end
    if result.status == "arrived" then
        log(bot, string.format(
            "%s: waypoint %d/%d arrived at (%d,%d) d=%d",
            label, index, #points, point[1], point[2], result.distance
        ))
        BOT_STATE.wp = index + 1
        BOT_STATE.wp_retry = false
        BOT_STATE.wp_tol = nil
        return BOT_STATE.wp > #points
    end
    if not BOT_STATE.wp_retry then
        -- A terminal status is sticky for one goal. Retry it once with a
        -- tighter tolerance, then move on so one bad point cannot stall us.
        BOT_STATE.wp_retry = true
        BOT_STATE.wp_tol = 60
        log(bot, string.format(
            "%s: waypoint %d/%d %s at d=%d; retrying tighter",
            label, index, #points, result.status, result.distance
        ))
        return false
    end
    log(bot, string.format(
        "%s: waypoint %d/%d %s at d=%d; skipping",
        label, index, #points, result.status, result.distance
    ))
    BOT_STATE.wp = index + 1
    BOT_STATE.wp_retry = false
    BOT_STATE.wp_tol = nil
    return BOT_STATE.wp > #points
end

-- A generic fallback approach from each side of a door zone. The reach probe
-- is 600 units ahead, so a point 300 outside the zone puts the probe inside.
local function generic_door_points(door)
    local x, z, w, h = door.x, door.z, door.width, door.depth
    local cx, cz = x + w // 2, z + h // 2
    return {
        { x - 300, cz, cx, cz },
        { x + w + 300, cz, cx, cz },
        { cx, z - 300, cx, cz },
        { cx, z + h + 300, cx, cz },
    }
end

-- A precise read of a door's lock state for the failure logs. Bank 2 is the
-- lock bank; the low six bits of the door lock byte name its bit.
local function door_diag(bot, target)
    local door = door_by_target(bot, target)
    if not door then
        return string.format("%s door=missing scenario7c=%s lockpick=%s",
            target, tostring(bot:flag(0, SCENARIO_HAS_LOCKPICK)),
            tostring(bot:has_item(ITEM_LOCKPICK)))
    end
    local lock = door.lock or 0
    return string.format(
        "%s door(slot=%d key=0x%02X lock=0x%02X lockbit=%d lockflag=%s next=%d) "
            .. "scenario7c=%s lockpick=%s sword=%s",
        target, door.slot, door.key or 0, lock, lock % 64,
        tostring(bot:flag(2, lock % 64)), door.next_room or 0,
        tostring(bot:flag(0, SCENARIO_HAS_LOCKPICK)),
        tostring(bot:has_item(ITEM_LOCKPICK)),
        tostring(bot:has_item(ITEM_SWORD_KEY))
    )
end

-- Walk to a point just outside a door zone, face the zone, and press action.
-- `custom` lists phase-specific approaches first (the known-good side); the
-- generic sides follow as retries. Returns false while working; the room
-- change is observed by enter_room.
local function door_nav(bot, target, custom, label)
    local nav = BOT_STATE.nav
    if not nav or nav.target ~= target then
        local door = door_by_target(bot, target)
        if not door then
            fail(bot, "no door to room " .. target)
            return false
        end
        local kind = door_action_kind(bot, target)
        if kind ~= "door" then
            log(bot, string.format("door to %s is %s, not a door; %s",
                target, tostring(kind), door_diag(bot, target)))
            -- Remember the failed target so the message is logged once.
            BOT_STATE.nav = {
                target = target,
                points = {},
                index = 1,
                pressing = false,
                ticks = 0,
                exhausted = true,
            }
            return false
        end
        local points = {}
        for i = 1, #custom do
            points[#points + 1] = custom[i]
        end
        local generic = generic_door_points(door)
        for i = 1, #generic do
            points[#points + 1] = generic[i]
        end
        nav = {
            target = target,
            points = points,
            index = 1,
            pressing = false,
            ticks = 0,
            exhausted = false,
        }
        BOT_STATE.nav = nav
        log(bot, string.format("door %s: navigating with %d approaches; %s",
            target, #points, door_diag(bot, target)))
    end

    if nav.index > #nav.points then
        if not nav.exhausted then
            nav.exhausted = true
            log(bot, string.format("%s: every approach failed; %s",
                label, door_diag(bot, target)))
        end
        return false
    end
    local point = nav.points[nav.index]

    if not nav.pressing then
        local result = bot:walk_to(point[1], point[2], {
            run = bot:distance(point[1], point[2]) >= RUN_MIN_DISTANCE,
            tolerance = 120,
        })
        if result.status == "arrived" then
            nav.pressing = true
            nav.ticks = 0
            log(bot, string.format(
                "%s: at approach %d/%d (%d,%d) d=%d",
                label, nav.index, #nav.points, point[1], point[2], result.distance
            ))
        elseif result.status ~= "moving" then
            log(bot, string.format(
                "%s: approach %d/%d %s d=%d; next side",
                label, nav.index, #nav.points, result.status, result.distance
            ))
            nav.index = nav.index + 1
            nav.pressing = false
        end
        return false
    end

    nav.ticks = nav.ticks + 1
    local facing = bot:face_to(point[3], point[4])
    if facing.status == "facing" then
        bot:press_action()
    end
    if nav.ticks % 30 == 0 then
        log(bot, string.format(
            "%s: pressing %d ticks face=%s angle_error=%d",
            label, nav.ticks, facing.status, facing.angle_error
        ))
    end
    if nav.ticks > 110 then
        log(bot, string.format("%s: press timeout on approach %d; %s; next side",
            label, nav.index, door_diag(bot, target)))
        nav.index = nav.index + 1
        nav.pressing = false
        nav.ticks = 0
    end
    return false
end

-- Four approach points 300 units outside an action zone. The action reach
-- probe is 600 units ahead, so the probe lands 300 units inside the zone.
local function generic_action_points(action)
    local x, z, w, h = action.x, action.z, action.width, action.depth
    local cx, cz = x + w // 2, z + h // 2
    return {
        { x - 300, cz, cx, cz },
        { x + w + 300, cz, cx, cz },
        { cx, z - 300, cx, cz },
        { cx, z + h + 300, cx, cz },
    }
end

-- Try the closest approach first. A room may have a wall on one side (the
-- 107 ribbon is behind the gallery's inner wall) or an enemy near another, so
-- the fixed west/east/north/south order is only a fallback.
local function sort_points_by_distance(bot, points)
    for i = 2, #points do
        local point = points[i]
        local distance = bot:distance(point[1], point[2])
        local j = i - 1
        while j >= 1
            and bot:distance(points[j][1], points[j][2]) > distance do
            points[j + 1] = points[j]
            j = j - 1
        end
        points[j + 1] = point
    end
    return points
end

-- Walk to reach of a ground item action, face it, and press action until the
-- item count rises or the got-item viewer closes. The item viewer runs while
-- the scene is not controllable, so the route's handle_scene closes it before
-- this function runs again. If a side does not fire, walk to the next side.
--
-- A zone at the room origin is a scripted handover (the 106 lockpick): its
-- zone is outside the walkable floor, so use the room item action directly.
local function action_nav(bot, slot, label, custom, opts)
    local action = action_by_slot(bot, slot)
    local nav = BOT_STATE.nav
    if not nav or nav.kind ~= "action" or nav.slot ~= slot then
        if not action or action.kind ~= "item" then
            log(bot, string.format(
                "%s: item action slot %s is not present (already taken?)",
                label, tostring(slot)
            ))
            return true
        end
        local scripted = action.x == 0 and action.z == 0
            and action.width <= 32 and action.depth <= 32
        nav = {
            kind = "action",
            slot = slot,
            item = action.item,
            label = label,
            before = bot:item_count(action.item),
            scripted = scripted,
            optional = opts and opts.optional or false,
            points = {},
            index = 1,
            pressing = false,
            pressed = false,
            used = false,
            ticks = 0,
            total = 0,
        }
        BOT_STATE.nav = nav
        BOT_STATE.saw_pickup = false
        if scripted then
            log(bot, string.format(
                "%s: scripted handover slot %d item=0x%02X count=%d",
                label, slot, action.item, nav.before
            ))
        else
            local points = {}
            if custom then
                for i = 1, #custom do
                    points[#points + 1] = custom[i]
                end
            end
            local generic = sort_points_by_distance(bot, generic_action_points(action))
            for i = 1, #generic do
                points[#points + 1] = generic[i]
            end
            nav.points = points
            log(bot, string.format(
                "%s: item=0x%02X slot=%d zone=(%d,%d,%d,%d) count=%d "
                    .. "with %d approaches",
                label, action.item, slot, action.x, action.z,
                action.width, action.depth, nav.before, #nav.points
            ))
        end
    end

    nav.total = nav.total + 1
    local count = bot:item_count(nav.item)
    if count > nav.before then
        log(bot, string.format(
            "%s: item=0x%02X count %d -> %d",
            label, nav.item, nav.before, count
        ))
        BOT_STATE.nav = nil
        return true
    end

    action = action_by_slot(bot, slot)
    if nav.pressed and (not action or action.kind ~= "item") then
        log(bot, string.format(
            "%s: action slot %d cleared after the press (count=%d)",
            label, slot, count
        ))
        BOT_STATE.nav = nil
        return true
    end
    if nav.pressed and BOT_STATE.saw_pickup then
        log(bot, string.format(
            "%s: got-item viewer closed (count=%d)", label, count
        ))
        BOT_STATE.nav = nil
        return true
    end

    if nav.scripted then
        if not nav.used then
            nav.used = true
            nav.pressed = true
            log(bot, string.format(
                "%s: using scripted item action slot %d", label, slot
            ))
            bot:use_action(slot)
            return false
        end
        if nav.total > ACTION_TIMEOUT_TICKS then
            fail(bot, string.format(
                "%s: scripted handover slot %d did not award item 0x%02X "
                    .. "(count=%d action=%s pickup=%s)",
                label, slot, nav.item, count,
                tostring(action and action.kind or "gone"),
                tostring(BOT_STATE.saw_pickup)
            ))
        end
        return false
    end

    if nav.index > #nav.points then
        local message = string.format(
            "%s: every approach failed for item 0x%02X slot %d "
                .. "(count=%d zone=(%d,%d,%d,%d) action=%s pickup=%s)",
            label, nav.item, slot, count,
            action and action.x or -1, action and action.z or -1,
            action and action.width or -1, action and action.depth or -1,
            tostring(action and action.kind or "gone"),
            tostring(BOT_STATE.saw_pickup)
        )
        if nav.optional then
            log(bot, message .. "; optional item skipped")
            BOT_STATE.nav = nil
            return "failed"
        end
        fail(bot, message)
        return false
    end
    local point = nav.points[nav.index]

    if not nav.pressing then
        local result = bot:walk_to(point[1], point[2], {
            run = bot:distance(point[1], point[2]) >= RUN_MIN_DISTANCE,
            tolerance = 120,
        })
        if result.status == "arrived" then
            nav.pressing = true
            nav.ticks = 0
            log(bot, string.format(
                "%s: at approach %d/%d (%d,%d) d=%d",
                label, nav.index, #nav.points, point[1], point[2], result.distance
            ))
        elseif result.status ~= "moving" then
            local p = bot:player()
            log(bot, string.format(
                "%s: approach %d/%d %s d=%d pos=(%d,%d) health=%d collision=%d; next side",
                label, nav.index, #nav.points, result.status, result.distance,
                p.x, p.z, p.health, p.collision_flags
            ))
            nav.index = nav.index + 1
            nav.pressing = false
        end
        if nav.total > ACTION_TIMEOUT_TICKS then
            local message = string.format(
                "%s: timed out walking to item 0x%02X slot %d (count=%d)",
                label, nav.item, slot, count
            )
            if nav.optional then
                log(bot, message .. "; optional item skipped")
                BOT_STATE.nav = nil
                return "failed"
            end
            fail(bot, message)
        end
        return false
    end

    nav.ticks = nav.ticks + 1
    local facing = bot:face_to(point[3], point[4])
    if facing.status == "facing" then
        nav.pressed = true
        bot:press_action()
    end
    if nav.ticks % 30 == 0 then
        local p = bot:player()
        log(bot, string.format(
            "%s: pressing %d ticks face=%s angle_error=%d count=%d "
                .. "pos=(%d,%d) health=%d collision=%d attacked=%d",
            label, nav.ticks, facing.status, facing.angle_error, count,
            p.x, p.z, p.health, p.collision_flags, p.being_attacked
        ))
    end
    if nav.ticks > ACTION_SIDE_TICKS then
        log(bot, string.format(
            "%s: no pickup on approach %d; next side (count=%d)",
            label, nav.index, count
        ))
        nav.index = nav.index + 1
        nav.pressing = false
        nav.ticks = 0
    end
    if nav.total > ACTION_TIMEOUT_TICKS then
        local message = string.format(
            "%s: timed out picking up item 0x%02X slot %d "
                .. "(count=%d approach=%d/%d zone=(%d,%d,%d,%d))",
            label, nav.item, slot, count, nav.index, #nav.points,
            action and action.x or -1, action and action.z or -1,
            action and action.width or -1, action and action.depth or -1
        )
        if nav.optional then
            log(bot, message .. "; optional item skipped")
            BOT_STATE.nav = nil
            return "failed"
        end
        fail(bot, message)
    end
    return false
end

-- The 117 gallery paintings, in the only order that satisfies the script's
-- room-bit requires. Each entry walks to an approach on the reachable side of
-- the wall (north for AOT_02/03, south for AOT_04..07) and presses action.
local PAINT_STEPS = {
    { slot = 3, bank = 4, bit = F_ROOM_1A,
      label = "117 painting 1/7 newborn",
      points = { { 19500, 8100 } }, face = { 19500, 7200 } },
    { slot = 5, bank = 4, bit = F_ROOM_1C,
      label = "117 painting 2/7 infant",
      points = { { 24000, 8100 }, { 24000, 5300 }, { 16000, 5300 } },
      face = { 16000, 6100 } },
    { slot = 6, bank = 4, bit = F_ROOM_1D,
      label = "117 painting 3/7 lively boy",
      points = { { 10500, 5300 } }, face = { 10500, 6100 } },
    { slot = 4, bank = 4, bit = F_ROOM_1B,
      label = "117 painting 4/7 young man",
      points = { { 19500, 5300 } }, face = { 19500, 6100 } },
    { slot = 2, bank = 4, bit = F_ROOM_19,
      label = "117 painting 5/7 tired middle-aged man",
      points = { { 24000, 5300 }, { 24000, 8100 }, { 16000, 8100 } },
      face = { 16000, 7200 } },
    { slot = 7, bank = 4, bit = F_ROOM_1E,
      label = "117 painting 6/7 bold old man",
      points = { { 24000, 8100 }, { 24000, 5300 }, { 7250, 5300 } },
      face = { 7250, 6100 } },
    { slot = 8, bank = 0, bit = F_SCENARIO_03,
      label = "117 painting 7/7 people mourning",
      points = { { 2900, 4100 } }, face = { 1850, 4100 } },
}

-- Press one painting zone and wait for its room bit. A press starts the
-- picture message and then the yes/no switch prompt; handle_scene closes both
-- while the phase is not called. The bit is the proof the push landed: the
-- script clears 0x18..0x1E first and sets only this painting's bit afterwards.
local function press_flag_action(bot, step)
    if bot:flag(step.bank, step.bit) then
        return true
    end
    local action = action_by_slot(bot, step.slot)
    if not action then
        log(bot, string.format("%s: action slot %d is not present",
            step.label, step.slot))
        return true
    end
    local nav = BOT_STATE.nav
    if not nav or nav.kind ~= "paint" or nav.slot ~= step.slot then
        nav = { kind = "paint", slot = step.slot, ticks = 0,
            last_press = -1000, presses = 0 }
        BOT_STATE.nav = nav
    end
    nav.ticks = nav.ticks + 1
    if nav.ticks - nav.last_press < 20 then
        return false
    end
    if nav.presses >= 4 then
        log(bot, string.format(
            "%s: flag %d:0x%02X still clear after %d presses; "
                .. "painting sequence stopped",
            step.label, step.bank, step.bit, nav.presses))
        BOT_STATE.puzzle_failed = true
        BOT_STATE.nav = nil
        return false
    end
    local facing = bot:face_to(step.face[1], step.face[2])
    if facing.status ~= "facing" then
        return false
    end
    nav.presses = nav.presses + 1
    nav.last_press = nav.ticks
    log(bot, string.format("%s: press %d (slot=%d, waiting for flag %d:0x%02X)",
        step.label, nav.presses, step.slot, step.bank, step.bit))
    bot:press_action()
    return false
end

local function run_117_puzzle(bot)
    local index = BOT_STATE.puzzle_index or 1
    if index > #PAINT_STEPS then
        return true
    end
    local step = PAINT_STEPS[index]
    if bot:flag(step.bank, step.bit) then
        log(bot, string.format("%s: flag already set", step.label))
        BOT_STATE.puzzle_index = index + 1
        BOT_STATE.wp = 1
        BOT_STATE.wp_retry = false
        BOT_STATE.wp_tol = nil
        return false
    end
    if walk_waypoints(bot, step.points, 150, step.label) then
        if press_flag_action(bot, step) then
            if BOT_STATE.puzzle_failed then
                return true
            end
            log(bot, string.format("%s: flag 0x%02X set", step.label, step.bit))
            BOT_STATE.puzzle_index = index + 1
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
        elseif BOT_STATE.puzzle_failed then
            return true
        end
    end
    return false
end

local function enter_room(bot, room, tick)
    local p = bot:player()
    log(bot, string.format(
        "room %s at tick %d pos=(%d,%d) camera=%d health=%d/%d "
            .. "common02=%s common03=%s common05=%s common64=%s common65=%s "
            .. "lockpick=%s scenario7c=%s scenario03=%s scenario69=%s "
            .. "scenario6a=%s scenario6b=%s scenario6c=%s lock17=%s "
            .. "items(ribbon=%d ammo=%d herb=%d star=%d radio=%d desk=%d crank=%d)",
        room.id, tick, p.x, p.z, room.camera, p.health, p.max_health,
        tostring(flag_common(bot, F_COMMON_02)),
        tostring(flag_common(bot, F_COMMON_03)),
        tostring(flag_common(bot, F_COMMON_05)),
        tostring(flag_common(bot, 0x40)),
        tostring(flag_common(bot, 0x41)),
        tostring(bot:has_item(ITEM_LOCKPICK)),
        tostring(bot:flag(0, SCENARIO_HAS_LOCKPICK)),
        tostring(bot:flag(0, F_SCENARIO_03)),
        tostring(bot:flag(0, F_SCENARIO_69)),
        tostring(bot:flag(0, F_SCENARIO_6A)),
        tostring(bot:flag(0, F_SCENARIO_6B)),
        tostring(bot:flag(0, F_SCENARIO_6C)),
        tostring(bot:flag(2, F_LOCK_17)),
        bot:item_count(ITEM_INK_RIBBON),
        bot:item_count(ITEM_HANDGUN_AMMO),
        bot:item_count(ITEM_GREEN_HERB),
        bot:item_count(ITEM_STAR_CREST),
        bot:item_count(ITEM_RADIO_KEY),
        bot:item_count(ITEM_DESK_KEY),
        bot:item_count(ITEM_CRANK_KEY)
    ))
    BOT_STATE.room = room.id
    BOT_STATE.room_ticks = 0
    BOT_STATE.nav = nil
    BOT_STATE.wp = 1
    BOT_STATE.wp_retry = false
    BOT_STATE.wp_tol = nil

    if room.id == "106" and has_lockpick_action(bot) then
        log(bot, "Barry lockpick item registered")
    end

    if room.id == "106" then
        if BOT_STATE.phase == "boot" then
            set_phase(bot, "opening")
        elseif BOT_STATE.phase == "to_106" or BOT_STATE.phase == "back_from_203"
            or bot:visited("104") then
            set_phase(bot, "barry", "returned to the main hall")
        else
            set_phase(bot, "opening")
        end
    elseif room.id == "105" then
        if bot:visited("104") and flag_common(bot, F_COMMON_02) then
            set_phase(bot, "to_106", "returned from the tea room")
        else
            set_phase(bot, "to_104", "dining room")
        end
    elseif room.id == "104" then
        set_phase(bot, "walk_104", "tea room")
    elseif room.id == "203" then
        set_phase(bot, "back_from_203", "stairs landing")
    elseif room.id == "107" then
        log(bot, "east wing reached")
        set_phase(bot, "to_107_item", "gallery")
    elseif room.id == "108" then
        -- The handgun ammo zone (20700,13200,1000,1000) lies inside the solid
        -- table object at (21605,13970) (half extents 650,544,1400). The table
        -- collision stops the player at x=22627, so the 600-unit reach ends at
        -- x=22027, 327 short of the zone; the north and south sides are
        -- farther. A south push attempt moves the player into the table/dog
        -- and does not clear the zone, so log the block and go on to 109.
        log(bot, "108 handgun ammo blocked by the solid table: zone "
            .. "(20700,13200,1000,1000) reach ends short on every side")
        set_phase(bot, "to_109", "L passage")
    elseif room.id == "109" then
        set_phase(bot, "to_109_item", "L passage west")
    elseif room.id == "10A" then
        log(bot, "10A reached")
        BOT_STATE.milestone = BOT_STATE.milestone or "10A"
        if not BOT_STATE.did_117 then
            set_phase(bot, "to_117", "back passage")
        elseif not BOT_STATE.did_11a then
            set_phase(bot, "to_11a", "roofed passage")
        else
            bot:finish("10A reached; east wing already tried")
        end
    elseif room.id == "117" then
        log(bot, "117 large gallery reached")
        BOT_STATE.did_117 = true
        BOT_STATE.milestone = "117"
        set_phase(bot, "to_117_puzzle", "large gallery")
    elseif room.id == "11A" then
        log(bot, "11A roofed passage reached")
        BOT_STATE.did_11a = true
        BOT_STATE.milestone = "11A"
        set_phase(bot, "to_11a_item", "roofed passage")
    elseif room.id == "11B" then
        log(bot, "11B store room reached")
        BOT_STATE.milestone = "11B"
        set_phase(bot, "to_11b_desk", "store room")
    elseif room.id == "300" then
        log(bot, "300 first house reached")
        BOT_STATE.milestone = "300"
        bot:finish("300 first house reached (11B -> 300 door)")
    else
        fail(bot, "unexpected room " .. room.id)
    end
end

local function phase_opening(bot)
    -- The shipped opening scene normally scripts the walk into 105 by itself.
    -- If this build leaves the player in 106, use the 106 -> 105 door once the
    -- scene has clearly ended.
    if BOT_STATE.phase_ticks > 900 and BOT_STATE.controllable_ticks > 30 then
        set_phase(bot, "goto_105_from_106", "opening ended in 106")
        return
    end
    if BOT_STATE.phase_ticks > 1500 then
        fail(bot, "opening scene did not leave 106")
    end
end

local function phase_goto_105_from_106(bot)
    local custom = { { 3300, 16900, 2000, 16900 } }
    door_nav(bot, "105", custom, "door 106->105")
    if BOT_STATE.phase_ticks > 900 then
        fail(bot, "could not open the 106 -> 105 door")
    end
end

local function phase_to_104(bot)
    -- The A* planner only knows the room collision, not the OBJ_02 sideboard
    -- at (18000, 14300), so a single goal hugs the north wall and wedges on
    -- it. This one waypoint stays south of the sideboard; after it the AOT_02
    -- zone is straight ahead, and the scene's scripted move leaves a clean
    -- north walk to the door.
    local points = {
        { 14000, 13000 },
        { 6700, 13700 },
    }
    if walk_waypoints(bot, points, 120, "105 dining room") then
        local custom = { { 6700, 13700, 6700, 16000 } }
        door_nav(bot, "104", custom, "door 105->104")
    end
    if BOT_STATE.phase_ticks > 1500 then
        fail(bot, "could not reach the 105 -> 104 door")
    end
end

local function phase_walk_104(bot)
    if flag_common(bot, F_COMMON_02) then
        set_phase(bot, "return_104", "first-zombie FMV done")
        return
    end
    -- The 1 -> 4 switch zone is the diagonal band between (500,3600) and
    -- (8200,8600). The old (4300, 4400) goal sat just south of it and only
    -- worked because the path overshot into the band; aim well inside it.
    local points = {
        { 4300, 4800 },
    }
    if walk_waypoints(bot, points, 200, "104 tea room") then
        BOT_STATE.west_wait = BOT_STATE.west_wait + 1
        if BOT_STATE.west_wait > 600 then
            if flag_common(bot, F_COMMON_02) then
                set_phase(bot, "return_104", "FMV flag set late")
            else
                fail(bot, "camera 4 did not start the first-zombie FMV")
            end
        end
    end
    if BOT_STATE.phase_ticks > 1500 then
        fail(bot, "could not trigger the 104 FMV")
    end
end

local function phase_return_104(bot)
    -- Straight back east along the south wall. The zombie is north of this
    -- corridor, so the direct route does not pass it.
    local points = {
        { 12000, 2700 },
    }
    if walk_waypoints(bot, points, 150, "104 return") then
        -- The reach probe is 600 ahead, so z = 2700 already covers the door
        -- zone (z <= 2300) and leaves more clearance from the south wall.
        local custom = { { 12000, 2700, 12000, 0 } }
        door_nav(bot, "105", custom, "door 104->105")
    end
    if BOT_STATE.phase_ticks > 900 then
        fail(bot, "could not return to 105 from the tea room")
    end
end

local function phase_to_106(bot)
    -- One goal at the door approach; the old (24000, 10000) point was inside
    -- the long table and reported unreachable every run.
    local points = {
        { 31300, 8300 },
    }
    if walk_waypoints(bot, points, 120, "105 return") then
        local custom = { { 31300, 8300, 33000, 8300 } }
        door_nav(bot, "106", custom, "door 105->106")
    end
    if BOT_STATE.phase_ticks > 1500 then
        fail(bot, "could not reach the 105 -> 106 door")
    end
end

local function phase_barry(bot)
    -- The room init gives the lockpick item and starts on_enter_3 as soon as
    -- 106 is entered with F_COMMON_02 set. Let that scene settle before the
    -- camera walk. If it never starts, try the 203 round trip once.
    if BOT_STATE.saw_scene and not BOT_STATE.barry_scene_logged then
        BOT_STATE.barry_scene_logged = true
        log(bot, "Barry lockpick scene started")
    end
    if bot:has_item(ITEM_LOCKPICK) then
        set_phase(bot, "barry_done", "lockpick already held")
        return
    end
    -- The tick only reaches this phase while the scene is already over, so a
    -- seen-and-finished scene is enough: do not spend 120 ticks idling.
    if BOT_STATE.saw_scene then
        set_phase(bot, "barry_camera", "walking the hall camera cuts")
        return
    end
    if BOT_STATE.phase_ticks > 120 then
        set_phase(bot, "goto_203", "Barry scene did not start")
        return
    end
    if BOT_STATE.phase_ticks > 900 then
        if BOT_STATE.tried_203 then
            set_phase(bot, "barry_camera", "203 round trip did not start Barry")
        else
            set_phase(bot, "goto_203", "Barry scene timed out")
        end
    end
end

local function phase_goto_203(bot)
    BOT_STATE.tried_203 = true
    local custom = { { 17000, 15400, 17000, 17500 } }
    door_nav(bot, "203", custom, "door 106->203")
    if BOT_STATE.phase_ticks > 900 then
        fail(bot, "could not take the 203 stairs")
    end
end

local function phase_back_from_203(bot)
    local custom = { { 17000, 24000, 17000, 22500 } }
    door_nav(bot, "106", custom, "door 203->106")
    if BOT_STATE.phase_ticks > 900 then
        fail(bot, "could not return to 106 from 203")
    end
end

local function phase_barry_camera(bot)
    if flag_common(bot, F_COMMON_05) then
        set_phase(bot, "barry_done", "hall camera scenes done")
        return
    end
    if bot:has_item(ITEM_LOCKPICK) then
        set_phase(bot, "barry_done", "lockpick acquired")
        return
    end

    local camera = bot:room().camera
    if BOT_STATE.cam_stage == "north" then
        -- The camera 4 switch zone starts at z ~24305 on this line, so the
        -- old 22000 point was pure detour. One goal just inside the zone is
        -- enough for the cut.
        local points = {
            { 11000, 25000 },
        }
        if walk_waypoints(bot, points, 250, "106 north hall") then
            BOT_STATE.cam_stage = "north_wait"
            BOT_STATE.cam_wait = 0
        end
    elseif BOT_STATE.cam_stage == "north_wait" then
        BOT_STATE.cam_wait = BOT_STATE.cam_wait + 1
        if camera == 3 or camera == 4 then
            log(bot, string.format("camera %d reached; heading to camera 2/5", camera))
            BOT_STATE.cam_visited_34 = true
            BOT_STATE.cam_stage = "mid"
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
            BOT_STATE.cam_wait = 0
        elseif BOT_STATE.cam_wait > 240 then
            BOT_STATE.cam_stage = "north"
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
            BOT_STATE.cam_wait = 0
        end
    elseif BOT_STATE.cam_stage == "mid" then
        local points = { { 11000, 23000 } }
        if walk_waypoints(bot, points, 250, "106 mid hall") then
            BOT_STATE.cam_stage = "mid_wait"
            BOT_STATE.cam_wait = 0
        end
    elseif BOT_STATE.cam_stage == "mid_wait" then
        BOT_STATE.cam_wait = BOT_STATE.cam_wait + 1
        if flag_common(bot, F_COMMON_05) then
            set_phase(bot, "barry_done", "hall camera scenes done")
            return
        end
        if BOT_STATE.cam_wait > 600 then
            log(bot, "camera 2/5 did not set F_COMMON_05; retrying camera 3/4")
            BOT_STATE.cam_stage = "north"
            BOT_STATE.cam_visited_34 = false
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
            BOT_STATE.cam_wait = 0
        end
    end

    if BOT_STATE.phase_ticks > 2400 then
        if BOT_STATE.tried_203 then
            fail(bot, "hall camera cuts did not restore the east-wing door")
        else
            set_phase(bot, "goto_203", "camera cuts timed out")
        end
    end
end

local function room_flag_list(bot)
    local parts = {}
    for bit = 0, 40 do
        if bot:flag(4, bit) then
            parts[#parts + 1] = string.format("%02x", bit)
        end
    end
    return table.concat(parts, ",")
end

local function phase_barry_done(bot)
    BOT_STATE.barry_done_ticks = BOT_STATE.barry_done_ticks + 1
    local has_lockpick = bot:has_item(ITEM_LOCKPICK)
    local kind = door_action_kind(bot, "107")
    if BOT_STATE.barry_done_ticks % 60 == 1 then
        local p = bot:player()
        log(bot, string.format(
            "barry item wait %d pos=(%d,%d) roombits=[%s] lockpick=%s door107=%s",
            BOT_STATE.barry_done_ticks, p.x, p.z, room_flag_list(bot),
            tostring(has_lockpick), tostring(kind)
        ))
    end
    if has_lockpick then
        log(bot, "lockpick acquired")
        if kind == "door" then
            set_phase(bot, "to_107", "lockpick acquired and east-wing door unlocked")
        elseif BOT_STATE.barry_done_ticks > 600 then
            log(bot, "lockpick acquired; east-wing door did not become a door")
            bot:finish("lockpick acquired")
        end
        return
    end
    if not has_lockpick then
        local slot = item_action_slot(bot, ITEM_LOCKPICK)
        if slot then
            -- The lockpick zone is 0,0,16,16 at the room origin, outside the
            -- walkable floor: action_nav recognises that scripted handover and
            -- runs the room item action.
            action_nav(bot, slot, "lockpick handover")
        else
            log(bot, "lockpick item action is not present")
        end
    end
    if BOT_STATE.barry_done_ticks > 2000 then
        fail(bot, "lockpick did not arrive after the hall scene")
    end
end

local function phase_to_107(bot)
    local kind = door_action_kind(bot, "107")
    if kind ~= "door" then
        set_phase(bot, "barry_done", "east-wing door is not ready")
        return
    end
    local points = {
        { 31000, 12500 },
    }
    if walk_waypoints(bot, points, 120, "106 east wing") then
        local custom = { { 31000, 12500, 33000, 12500 } }
        door_nav(bot, "107", custom, "door 106->107")
    end
    if BOT_STATE.phase_ticks > 900 then
        fail(bot, "could not open the 106 -> 107 east-wing door")
    end
end

local function phase_to_107_item(bot)
    local slot = item_action_slot(bot, ITEM_INK_RIBBON)
    if not slot then
        local count = bot:item_count(ITEM_INK_RIBBON)
        if count > 0 then
            log(bot, string.format("107 ink ribbon already held count=%d", count))
        else
            local action = action_by_slot(bot, 3)
            log(bot, string.format(
                "107 ink ribbon action slot 3 is %s handler=%s; item skipped "
                    .. "or already taken (jill_ink_ribbons option)",
                tostring(action and action.kind or "missing"),
                tostring(action and action.handler or "nil")
            ))
        end
        set_phase(bot, "to_108", "no ink ribbon action")
        return
    end
    -- The item zone is south of the gallery's inner wall (box 1) and west of
    -- box 3; a direct A* goal wedges on that corner. Go around box 3 on its
    -- east side first, then walk west along the south gallery.
    local points = {
        { 14500, 11000 },
        { 14500, 13700 },
    }
    if walk_waypoints(bot, points, 150, "107 ink ribbon east") then
        if action_nav(bot, slot, "107 ink ribbon", {
            { 15000, 13700, 13780, 13730 },
        }) then
            log(bot, string.format("107 ink ribbon count=%d",
                bot:item_count(ITEM_INK_RIBBON)))
            set_phase(bot, "to_108", "ink ribbon taken")
            return
        end
    end
    if BOT_STATE.phase_ticks > 1800 then
        fail(bot, "could not take the 107 ink ribbon")
    end
end

local function phase_to_108(bot)
    -- The gallery statue at (12500,4600) is not in the room collision, so a
    -- straight goal to the east door wedges on it. Pass north of it first.
    local points = {
        { 9000, 3000 },
        { 14000, 3000 },
    }
    if walk_waypoints(bot, points, 150, "107 gallery north") then
        -- The door is on the east wall; the west approach is the nearest one.
        local custom = { { 15100, 4200, 16650, 4200 } }
        door_nav(bot, "108", custom, "door 107->108")
    end
    if BOT_STATE.phase_ticks > 1200 then
        fail(bot, "could not open the 107 -> 108 sword-key door; "
            .. door_diag(bot, "108"))
    end
end

local function phase_to_109(bot)
    -- Get back east of the ammo table, then run down the east corridor to the
    -- south door. A straight door goal from the table's north side would walk
    -- back into the table.
    local points = {
        { 23500, 12000 },
        { 23500, 20000 },
    }
    if walk_waypoints(bot, points, 150, "108 east corridor") then
        -- The door is on the south wall; approach from the north.
        local custom = { { 24800, 25200, 24800, 26700 } }
        door_nav(bot, "109", custom, "door 108->109")
    end
    if BOT_STATE.phase_ticks > 1200 then
        fail(bot, "could not open the 108 -> 109 door; " .. door_diag(bot, "109"))
    end
end

local function phase_to_109_item(bot)
    local slot = item_action_slot(bot, ITEM_GREEN_HERB)
    if not slot then
        local count = bot:item_count(ITEM_GREEN_HERB)
        if count > 0 then
            log(bot, string.format("109 green herb count=%d", count))
        else
            log(bot, "109 green herb action not present; assuming already taken")
        end
        set_phase(bot, "to_10a", "no herb action")
        return
    end
    if action_nav(bot, slot, "109 green herb", {
        { 22700, 2600, 21306, 2424 },
    }) then
        log(bot, string.format("109 green herb count=%d",
            bot:item_count(ITEM_GREEN_HERB)))
        set_phase(bot, "to_10a", "green herb taken")
        return
    end
    if BOT_STATE.phase_ticks > 1800 then
        fail(bot, "could not take the 109 green herb")
    end
end

local function phase_to_10a(bot)
    -- The door is on the west wall; approach from the east.
    local custom = { { 2900, 8550, 1300, 8550 } }
    door_nav(bot, "10A", custom, "door 109->10A")
    if BOT_STATE.phase_ticks > 1200 then
        fail(bot, "could not open the 109 -> 10A door; " .. door_diag(bot, "10A"))
    end
end

local function crest_flags_placed(bot)
    local count = 0
    if bot:flag(0, F_SCENARIO_69) then count = count + 1 end
    if bot:flag(0, F_SCENARIO_6A) then count = count + 1 end
    if bot:flag(0, F_SCENARIO_6B) then count = count + 1 end
    if bot:flag(0, F_SCENARIO_6C) then count = count + 1 end
    return count
end

-- The 11A -> 11B door is `DOOR_LOCKED | F_LOCK_17` with no key. In the first
-- mansion the room raises that lock bit only after all four crests are placed
-- in the roofed passage's use-item hollows; the radio key is a second-visit
-- Chris item and is not a key for this door. Report whatever state the room
-- actually left behind instead of forcing the lock.
local function finish_11b_locked(bot)
    local crests = crest_flags_placed(bot)
    log(bot, string.format(
        "11B door stays locked: %s crests=%d/4 star=%s radio=%s lock17=%s",
        door_diag(bot, "11B"), crests,
        tostring(bot:has_item(ITEM_STAR_CREST)),
        tostring(bot:has_item(ITEM_RADIO_KEY)),
        tostring(bot:flag(2, F_LOCK_17))
    ))
    BOT_STATE.milestone = "11A"
    bot:finish(string.format(
        "11A reached; 11B store room door locked "
            .. "(F_LOCK_17 false, key 0x00, crest hollows %d/4, star crest %s)",
        crests, bot:has_item(ITEM_STAR_CREST) and "held" or "missing"))
end

local function phase_to_117(bot)
    local points = {
        -- Stay south of the 10A zombie at (8272,2433) on the long west leg.
        { 14000, 4500 },
        { 8000, 4500 },
        { 3000, 4500 },
        { 3000, 2700 },
    }
    if walk_waypoints(bot, points, 150, "10A back passage") then
        -- The door is in the north wall band; approach from the south.
        local custom = { { 3000, 2700, 3000, 1700 } }
        door_nav(bot, "117", custom, "door 10A->117")
    end
    if BOT_STATE.phase_ticks > 1500 then
        fail(bot, "could not open the 10A -> 117 door; " .. door_diag(bot, "117"))
    end
end

local function phase_to_117_puzzle(bot)
    if bot:flag(0, F_SCENARIO_03) then
        set_phase(bot, "to_117_item", "painting sequence already done")
        return
    end
    if BOT_STATE.puzzle_failed then
        set_phase(bot, "to_117_item", "painting sequence failed")
        return
    end
    if run_117_puzzle(bot) then
        if BOT_STATE.puzzle_failed then
            set_phase(bot, "to_117_item", "painting sequence failed")
        else
            set_phase(bot, "to_117_item", "painting sequence done")
        end
        return
    end
    if BOT_STATE.phase_ticks > 4000 then
        log(bot, string.format(
            "117 painting sequence timed out at step %d; giving up",
            BOT_STATE.puzzle_index or 1))
        BOT_STATE.puzzle_failed = true
        set_phase(bot, "to_117_item", "painting timeout")
    end
end

local function phase_to_117_item(bot)
    local count = bot:item_count(ITEM_STAR_CREST)
    if count > 0 then
        log(bot, string.format("117 star crest count=%d", count))
        BOT_STATE.milestone = "117 star crest"
        set_phase(bot, "return_117", "star crest already held")
        return
    end
    if not bot:flag(0, F_SCENARIO_03) then
        log(bot, "117 star crest blocked: the painting sequence did not set "
            .. "F_SCENARIO_03, so the AOT_08 painting event still covers AOT_09")
        set_phase(bot, "return_117", "star crest blocked")
        return
    end
    local slot = item_action_slot(bot, ITEM_STAR_CREST)
    if not slot then
        log(bot, string.format(
            "117 star crest action not present; count=%d", count))
        set_phase(bot, "return_117", "no star crest action")
        return
    end
    -- The final painting move leaves Jill at (2773,4147), just east of the
    -- crest zone; the east approach is inside reach already. The west wall's
    -- grown edge is x=2710, so the approach stays at x=2900.
    local result = action_nav(bot, slot, "117 star crest", {
        { 2900, 4100, 1850, 4100 },
    }, { optional = true })
    if result == true then
        log(bot, string.format("117 star crest count=%d",
            bot:item_count(ITEM_STAR_CREST)))
        BOT_STATE.milestone = "117 star crest"
        set_phase(bot, "return_117", "star crest taken")
        return
    end
    if result == "failed" then
        log(bot, string.format(
            "117 star crest unreachable; count=%d", bot:item_count(ITEM_STAR_CREST)))
        set_phase(bot, "return_117", "star crest unreachable")
        return
    end
    if BOT_STATE.phase_ticks > 1800 then
        fail(bot, "could not take the 117 star crest")
    end
end

local function phase_return_117(bot)
    -- Go back through the east gap in the gallery's inner wall, then to the
    -- south-wall door to 10A.
    local points = {
        { 24000, 5000 },
        { 24000, 10000 },
        { 3200, 10500 },
    }
    if walk_waypoints(bot, points, 150, "117 return") then
        local custom = { { 3200, 10500, 3200, 11600 } }
        door_nav(bot, "10A", custom, "door 117->10A")
    end
    if BOT_STATE.phase_ticks > 2400 then
        fail(bot, "could not return to 10A from 117; " .. door_diag(bot, "10A"))
    end
end

local function phase_to_11a(bot)
    -- The door is on the west wall of the south corridor. Keep west of the
    -- 10A zombie at (3023,6864) on the way down.
    local points = {
        { 2600, 8000 },
        { 2600, 11850 },
    }
    if walk_waypoints(bot, points, 150, "10A west passage") then
        local custom = { { 2500, 11850, 1300, 11850 } }
        door_nav(bot, "11A", custom, "door 10A->11A")
    end
    if BOT_STATE.phase_ticks > 1800 then
        fail(bot, "could not open the 10A -> 11A door; " .. door_diag(bot, "11A"))
    end
end

local function phase_to_11a_item(bot)
    local slot = item_action_slot(bot, ITEM_RADIO_KEY)
    local count = bot:item_count(ITEM_RADIO_KEY)
    if not slot then
        log(bot, string.format(
            "11A radio key action not present (Jill first mansion registers the "
                .. "crest use-item zones here); count=%d", count))
        set_phase(bot, "to_11b", "no radio key action")
        return
    end
    local points = {
        { 4500, 12000 },
        { 4500, 23000 },
        { 6000, 25000 },
    }
    if walk_waypoints(bot, points, 150, "11A north passage") then
        local result = action_nav(bot, slot, "11A radio key", nil, { optional = true })
        if result == true then
            log(bot, string.format("11A radio key count=%d",
                bot:item_count(ITEM_RADIO_KEY)))
            set_phase(bot, "to_11b", "radio key taken")
            return
        end
        if result == "failed" then
            log(bot, string.format(
                "11A radio key unreachable; count=%d",
                bot:item_count(ITEM_RADIO_KEY)))
            set_phase(bot, "to_11b", "radio key unreachable")
            return
        end
    end
    if BOT_STATE.phase_ticks > 3000 then
        log(bot, "11A radio key timed out; continuing to the store-room door")
        set_phase(bot, "to_11b", "radio key timeout")
    end
end

local function phase_to_11b(bot)
    local p = bot:player()
    local points
    if p.z > 24000 then
        points = {
            { 6000, 25000 },
            { 7700, 25550 },
        }
    else
        points = {
            { 4500, 12000 },
            { 4500, 23000 },
            { 6000, 24500 },
            { 7700, 25550 },
        }
    end
    if walk_waypoints(bot, points, 150, "11A store room door") then
        -- The door is on the east wall; approach from the west.
        local custom = { { 7700, 25550, 8750, 25550 } }
        door_nav(bot, "11B", custom, "door 11A->11B")
        local nav = BOT_STATE.nav
        if nav and nav.target == "11B"
            and (nav.exhausted or (nav.index > 1 and not nav.pressing)) then
            -- The known-good west approach timed out. The room's lock bit and
            -- crest flags are already in the log; stop in 11A rather than
            -- trying the wall-side approaches or forcing the lock.
            finish_11b_locked(bot)
            return
        end
    end
    if BOT_STATE.phase_ticks > 2400 then
        log(bot, string.format(
            "11B door approach did not finish; pos=(%d,%d) %s",
            p.x, p.z, door_diag(bot, "11B")))
        BOT_STATE.milestone = "11A"
        bot:finish("11A reached; 11B door approach unreachable")
    end
end

local function phase_to_11b_desk(bot)
    local slot = item_action_slot(bot, ITEM_DESK_KEY)
    if not slot then
        log(bot, string.format(
            "11B desk key action not present; count=%d",
            bot:item_count(ITEM_DESK_KEY)))
        set_phase(bot, "to_11b_crank", "no desk key action")
        return
    end
    local result = action_nav(bot, slot, "11B desk key", {
        { 8800, 8200, 8750, 9500 },
    }, { optional = true })
    if result == true then
        log(bot, string.format("11B desk key count=%d",
            bot:item_count(ITEM_DESK_KEY)))
        set_phase(bot, "to_11b_crank", "desk key taken")
        return
    end
    if result == "failed" then
        log(bot, string.format(
            "11B desk key unreachable; count=%d",
            bot:item_count(ITEM_DESK_KEY)))
        set_phase(bot, "to_11b_crank", "desk key unreachable")
        return
    end
    if BOT_STATE.phase_ticks > 1800 then
        log(bot, "11B desk key timed out; continuing to the crank key")
        set_phase(bot, "to_11b_crank", "desk key timeout")
    end
end

local function phase_to_11b_crank(bot)
    local slot = item_action_slot(bot, ITEM_CRANK_KEY)
    if not slot then
        log(bot, string.format(
            "11B crank key action not present; count=%d",
            bot:item_count(ITEM_CRANK_KEY)))
        set_phase(bot, "to_300", "no crank key action")
        return
    end
    local result = action_nav(bot, slot, "11B crank key", {
        { 5650, 3100, 5650, 2000 },
    }, { optional = true })
    if result == true then
        log(bot, string.format("11B crank key count=%d",
            bot:item_count(ITEM_CRANK_KEY)))
        set_phase(bot, "to_300", "crank key taken")
        return
    end
    if result == "failed" then
        log(bot, string.format(
            "11B crank key unreachable; count=%d",
            bot:item_count(ITEM_CRANK_KEY)))
        set_phase(bot, "to_300", "crank key unreachable")
        return
    end
    if BOT_STATE.phase_ticks > 1800 then
        log(bot, "11B crank key timed out; continuing to the 300 door")
        set_phase(bot, "to_300", "crank key timeout")
    end
end

local function phase_to_300(bot)
    -- The first-house door is on the east wall; approach from the west.
    local custom = { { 8500, 5900, 9100, 5900 } }
    door_nav(bot, "300", custom, "door 11B->300")
    local nav = BOT_STATE.nav
    if nav and nav.target == "300" and nav.exhausted then
        log(bot, "11B -> 300 first-house door did not open: " .. door_diag(bot, "300"))
        BOT_STATE.milestone = "11B"
        bot:finish("11B reached; 300 first-house door did not open")
        return
    end
    if BOT_STATE.phase_ticks > 1500 then
        log(bot, "11B -> 300 door timed out: " .. door_diag(bot, "300"))
        BOT_STATE.milestone = "11B"
        bot:finish("11B reached; 300 first-house door timed out")
    end
end

function on_bot_tick(bot)
    local tick = bot:tick()
    local room = bot:room()
    local scene = bot:scene()
    local player = bot:player()

    if BOT_STATE.room ~= room.id then
        enter_room(bot, room, tick)
        if room.id == "10A" then
            return
        end
    end

    BOT_STATE.phase_ticks = BOT_STATE.phase_ticks + 1
    BOT_STATE.room_ticks = BOT_STATE.room_ticks + 1

    local signature = scene_signature(scene)
    if signature ~= BOT_STATE.scene_sig then
        BOT_STATE.scene_sig = signature
        local p = bot:player()
        log(bot, string.format(
            "scene %s pos=(%d,%d) camera=%d", signature, p.x, p.z, room.camera
        ))
    end
    if scene.skippable or scene.cutscene_skip or scene.film then
        BOT_STATE.saw_scene = true
    end
    if scene.film then
        BOT_STATE.saw_film = true
    end

    if BOT_STATE.phase == "barry_done" and BOT_STATE.phase_ticks > 4000 and not scene.controllable then
        fail(bot, "pickup viewer did not close")
        return
    end

    if handle_scene(bot, scene) then
        return
    end
    if not scene.controllable then
        BOT_STATE.controllable_ticks = 0
        BOT_STATE.locked_ticks = BOT_STATE.locked_ticks + 1
        if BOT_STATE.locked_ticks > 150 then
            -- A grab or an unmarked lock: mash so it cannot stall the route.
            local dirs = { "left", "right", "up", "down" }
            bot:press_action()
            bot:set_input({ [dirs[tick % 4 + 1]] = true })
        end
        return
    end
    BOT_STATE.controllable_ticks = BOT_STATE.controllable_ticks + 1
    BOT_STATE.locked_ticks = 0

    local phase = BOT_STATE.phase
    if phase == "opening" then
        phase_opening(bot)
    elseif phase == "goto_105_from_106" then
        phase_goto_105_from_106(bot)
    elseif phase == "to_104" then
        phase_to_104(bot)
    elseif phase == "walk_104" then
        phase_walk_104(bot)
    elseif phase == "return_104" then
        phase_return_104(bot)
    elseif phase == "to_106" then
        phase_to_106(bot)
    elseif phase == "barry" then
        phase_barry(bot)
    elseif phase == "goto_203" then
        phase_goto_203(bot)
    elseif phase == "back_from_203" then
        phase_back_from_203(bot)
    elseif phase == "barry_camera" then
        phase_barry_camera(bot)
    elseif phase == "barry_done" then
        phase_barry_done(bot)
    elseif phase == "to_107" then
        phase_to_107(bot)
    elseif phase == "to_107_item" then
        phase_to_107_item(bot)
    elseif phase == "to_108" then
        phase_to_108(bot)
    elseif phase == "to_109" then
        phase_to_109(bot)
    elseif phase == "to_109_item" then
        phase_to_109_item(bot)
    elseif phase == "to_10a" then
        phase_to_10a(bot)
    elseif phase == "to_117" then
        phase_to_117(bot)
    elseif phase == "to_117_puzzle" then
        phase_to_117_puzzle(bot)
    elseif phase == "to_117_item" then
        phase_to_117_item(bot)
    elseif phase == "return_117" then
        phase_return_117(bot)
    elseif phase == "to_11a" then
        phase_to_11a(bot)
    elseif phase == "to_11a_item" then
        phase_to_11a_item(bot)
    elseif phase == "to_11b" then
        phase_to_11b(bot)
    elseif phase == "to_11b_desk" then
        phase_to_11b_desk(bot)
    elseif phase == "to_11b_crank" then
        phase_to_11b_crank(bot)
    elseif phase == "to_300" then
        phase_to_300(bot)
    else
        fail(bot, "unknown phase " .. tostring(phase))
    end
end
