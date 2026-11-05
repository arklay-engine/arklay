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
--   107 gallery: the east door (AOT_00) is a normal door again.
--
-- Every phase has a tick deadline. A blocked walk advances to the next
-- waypoint or door approach; a phase that cannot make progress fails with a
-- precise log instead of spinning until --bot-max-ticks.

local STATE = {
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
    lockpick_used = false,
    barry_done_ticks = 0,
}

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
            .. "common02=%s common03=%s common05=%s status1a=%s lockpick=%s "
            .. "scene(film=%s trans=%s skip=%s skippable=%s msg=%s ctrl=%s frozen=%s)",
        STATE.phase, reason, room.id, p.x, p.z, room.camera, p.health, p.max_health,
        tostring(flag_common(bot, F_COMMON_02)),
        tostring(flag_common(bot, F_COMMON_03)),
        tostring(flag_common(bot, F_COMMON_05)),
        tostring(bot:flag(BANK_STATUS, F_STATUS_1A)),
        tostring(bot:has_item(ITEM_LOCKPICK)),
        tostring(scene.film), tostring(scene.transition), tostring(scene.cutscene_skip),
        tostring(scene.skippable), tostring(scene.message), tostring(scene.controllable),
        tostring(scene.frozen)
    ))
    bot:finish("failed: " .. reason)
end

local function reset_phase_data()
    STATE.wp = 1
    STATE.wp_retry = false
    STATE.wp_tol = nil
    STATE.nav = nil
    STATE.press_ticks = 0
    STATE.saw_scene = false
    STATE.saw_film = false
    STATE.controllable_ticks = 0
    STATE.locked_ticks = 0
    STATE.cam_stage = "north"
    STATE.cam_wait = 0
    STATE.cam_visited_34 = false
    STATE.west_wait = 0
    STATE.barry_scene_logged = false
    STATE.lockpick_used = false
    STATE.barry_done_ticks = 0
end

local function set_phase(bot, phase, note)
    if STATE.phase == phase then
        return
    end
    if note then
        log(bot, string.format("phase %s -> %s (%s)", STATE.phase, phase, note))
    else
        log(bot, string.format("phase %s -> %s", STATE.phase, phase))
    end
    STATE.phase = phase
    STATE.phase_ticks = 0
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
        STATE.transition_start = nil
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
        if not STATE.transition_start then
            STATE.transition_start = tick
        end
        if (tick - STATE.transition_start) % 2 == 1 then
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
    local index = STATE.wp or 1
    if index > #points then
        return true
    end
    local point = points[index]
    local tol = STATE.wp_tol or tolerance
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
        STATE.wp = index + 1
        STATE.wp_retry = false
        STATE.wp_tol = nil
        return STATE.wp > #points
    end
    if not STATE.wp_retry then
        -- A terminal status is sticky for one goal. Retry it once with a
        -- tighter tolerance, then move on so one bad point cannot stall us.
        STATE.wp_retry = true
        STATE.wp_tol = 60
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
    STATE.wp = index + 1
    STATE.wp_retry = false
    STATE.wp_tol = nil
    return STATE.wp > #points
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

-- Walk to a point just outside a door zone, face the zone, and press action.
-- `custom` lists phase-specific approaches first (the known-good side); the
-- generic sides follow as retries. Returns false while working; the room
-- change is observed by enter_room.
local function door_nav(bot, target, custom, label)
    local nav = STATE.nav
    if not nav or nav.target ~= target then
        local door = door_by_target(bot, target)
        if not door then
            fail(bot, "no door to room " .. target)
            return false
        end
        local kind = door_action_kind(bot, target)
        if kind ~= "door" then
            log(bot, string.format("door to %s is %s, not a door", target, tostring(kind)))
            -- Remember the failed target so the message is logged once.
            STATE.nav = {
                target = target,
                points = {},
                index = 1,
                pressing = false,
                ticks = 0,
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
        }
        STATE.nav = nav
        log(bot, string.format("door %s: navigating with %d approaches", target, #points))
    end

    if nav.index > #nav.points then
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
        log(bot, string.format("%s: press timeout on approach %d; next side", label, nav.index))
        nav.index = nav.index + 1
        nav.pressing = false
        nav.ticks = 0
    end
    return false
end

local function enter_room(bot, room, tick)
    local p = bot:player()
    log(bot, string.format(
        "room %s at tick %d pos=(%d,%d) camera=%d health=%d/%d "
            .. "common02=%s common03=%s common05=%s lockpick=%s",
        room.id, tick, p.x, p.z, room.camera, p.health, p.max_health,
        tostring(flag_common(bot, F_COMMON_02)),
        tostring(flag_common(bot, F_COMMON_03)),
        tostring(flag_common(bot, F_COMMON_05)),
        tostring(bot:has_item(ITEM_LOCKPICK))
    ))
    STATE.room = room.id
    STATE.room_ticks = 0
    STATE.nav = nil
    STATE.wp = 1
    STATE.wp_retry = false
    STATE.wp_tol = nil

    if room.id == "107" then
        log(bot, "east wing reached")
        bot:finish("east wing reached")
        return
    end

    if room.id == "106" and has_lockpick_action(bot) then
        log(bot, "Barry lockpick item registered")
    end

    if room.id == "106" then
        if STATE.phase == "boot" then
            set_phase(bot, "opening")
        elseif STATE.phase == "to_106" or STATE.phase == "back_from_203"
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
    else
        fail(bot, "unexpected room " .. room.id)
    end
end

local function phase_opening(bot)
    -- The shipped opening scene normally scripts the walk into 105 by itself.
    -- If this build leaves the player in 106, use the 106 -> 105 door once the
    -- scene has clearly ended.
    if STATE.phase_ticks > 900 and STATE.controllable_ticks > 30 then
        set_phase(bot, "goto_105_from_106", "opening ended in 106")
        return
    end
    if STATE.phase_ticks > 1500 then
        fail(bot, "opening scene did not leave 106")
    end
end

local function phase_goto_105_from_106(bot)
    local custom = { { 3300, 16900, 2000, 16900 } }
    door_nav(bot, "105", custom, "door 106->105")
    if STATE.phase_ticks > 900 then
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
    if STATE.phase_ticks > 1500 then
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
        STATE.west_wait = STATE.west_wait + 1
        if STATE.west_wait > 600 then
            if flag_common(bot, F_COMMON_02) then
                set_phase(bot, "return_104", "FMV flag set late")
            else
                fail(bot, "camera 4 did not start the first-zombie FMV")
            end
        end
    end
    if STATE.phase_ticks > 1500 then
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
    if STATE.phase_ticks > 900 then
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
    if STATE.phase_ticks > 1500 then
        fail(bot, "could not reach the 105 -> 106 door")
    end
end

local function phase_barry(bot)
    -- The room init gives the lockpick item and starts on_enter_3 as soon as
    -- 106 is entered with F_COMMON_02 set. Let that scene settle before the
    -- camera walk. If it never starts, try the 203 round trip once.
    if STATE.saw_scene and not STATE.barry_scene_logged then
        STATE.barry_scene_logged = true
        log(bot, "Barry lockpick scene started")
    end
    if bot:has_item(ITEM_LOCKPICK) then
        set_phase(bot, "barry_done", "lockpick already held")
        return
    end
    -- The tick only reaches this phase while the scene is already over, so a
    -- seen-and-finished scene is enough: do not spend 120 ticks idling.
    if STATE.saw_scene then
        set_phase(bot, "barry_camera", "walking the hall camera cuts")
        return
    end
    if STATE.phase_ticks > 120 then
        set_phase(bot, "goto_203", "Barry scene did not start")
        return
    end
    if STATE.phase_ticks > 900 then
        if STATE.tried_203 then
            set_phase(bot, "barry_camera", "203 round trip did not start Barry")
        else
            set_phase(bot, "goto_203", "Barry scene timed out")
        end
    end
end

local function phase_goto_203(bot)
    STATE.tried_203 = true
    local custom = { { 17000, 15400, 17000, 17500 } }
    door_nav(bot, "203", custom, "door 106->203")
    if STATE.phase_ticks > 900 then
        fail(bot, "could not take the 203 stairs")
    end
end

local function phase_back_from_203(bot)
    local custom = { { 17000, 24000, 17000, 22500 } }
    door_nav(bot, "106", custom, "door 203->106")
    if STATE.phase_ticks > 900 then
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
    if STATE.cam_stage == "north" then
        -- The camera 4 switch zone starts at z ~24305 on this line, so the
        -- old 22000 point was pure detour. One goal just inside the zone is
        -- enough for the cut.
        local points = {
            { 11000, 25000 },
        }
        if walk_waypoints(bot, points, 250, "106 north hall") then
            STATE.cam_stage = "north_wait"
            STATE.cam_wait = 0
        end
    elseif STATE.cam_stage == "north_wait" then
        STATE.cam_wait = STATE.cam_wait + 1
        if camera == 3 or camera == 4 then
            log(bot, string.format("camera %d reached; heading to camera 2/5", camera))
            STATE.cam_visited_34 = true
            STATE.cam_stage = "mid"
            STATE.wp = 1
            STATE.wp_retry = false
            STATE.wp_tol = nil
            STATE.cam_wait = 0
        elseif STATE.cam_wait > 240 then
            STATE.cam_stage = "north"
            STATE.wp = 1
            STATE.wp_retry = false
            STATE.wp_tol = nil
            STATE.cam_wait = 0
        end
    elseif STATE.cam_stage == "mid" then
        local points = { { 11000, 23000 } }
        if walk_waypoints(bot, points, 250, "106 mid hall") then
            STATE.cam_stage = "mid_wait"
            STATE.cam_wait = 0
        end
    elseif STATE.cam_stage == "mid_wait" then
        STATE.cam_wait = STATE.cam_wait + 1
        if flag_common(bot, F_COMMON_05) then
            set_phase(bot, "barry_done", "hall camera scenes done")
            return
        end
        if STATE.cam_wait > 600 then
            log(bot, "camera 2/5 did not set F_COMMON_05; retrying camera 3/4")
            STATE.cam_stage = "north"
            STATE.cam_visited_34 = false
            STATE.wp = 1
            STATE.wp_retry = false
            STATE.wp_tol = nil
            STATE.cam_wait = 0
        end
    end

    if STATE.phase_ticks > 2400 then
        if STATE.tried_203 then
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
    STATE.barry_done_ticks = STATE.barry_done_ticks + 1
    local has_lockpick = bot:has_item(ITEM_LOCKPICK)
    local kind = door_action_kind(bot, "107")
    if STATE.barry_done_ticks % 60 == 1 then
        local p = bot:player()
        log(bot, string.format(
            "barry item wait %d pos=(%d,%d) roombits=[%s] lockpick=%s door107=%s",
            STATE.barry_done_ticks, p.x, p.z, room_flag_list(bot),
            tostring(has_lockpick), tostring(kind)
        ))
    end
    if has_lockpick then
        log(bot, "lockpick acquired")
        if kind == "door" then
            set_phase(bot, "to_107", "lockpick acquired and east-wing door unlocked")
        elseif STATE.barry_done_ticks > 600 then
            log(bot, "lockpick acquired; east-wing door did not become a door")
            bot:finish("lockpick acquired")
        end
        return
    end
    if not STATE.lockpick_used then
        local slot = item_action_slot(bot, ITEM_LOCKPICK)
        if slot then
            STATE.lockpick_used = true
            log(bot, "using the lockpick item action in slot " .. tostring(slot))
            bot:use_action(slot)
        else
            log(bot, "lockpick item action is not present")
        end
    end
    if STATE.barry_done_ticks > 2000 then
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
    if STATE.phase_ticks > 900 then
        fail(bot, "could not open the 106 -> 107 east-wing door")
    end
end

function on_bot_tick(bot)
    local tick = bot:tick()
    local room = bot:room()
    local scene = bot:scene()
    local player = bot:player()

    if STATE.room ~= room.id then
        enter_room(bot, room, tick)
        if room.id == "107" then
            return
        end
    end

    STATE.phase_ticks = STATE.phase_ticks + 1
    STATE.room_ticks = STATE.room_ticks + 1

    local signature = scene_signature(scene)
    if signature ~= STATE.scene_sig then
        STATE.scene_sig = signature
        local p = bot:player()
        log(bot, string.format(
            "scene %s pos=(%d,%d) camera=%d", signature, p.x, p.z, room.camera
        ))
    end
    if scene.skippable or scene.cutscene_skip or scene.film then
        STATE.saw_scene = true
    end
    if scene.film then
        STATE.saw_film = true
    end

    if STATE.phase == "barry_done" and STATE.phase_ticks > 4000 and not scene.controllable then
        fail(bot, "pickup viewer did not close")
        return
    end

    if handle_scene(bot, scene) then
        return
    end
    if not scene.controllable then
        STATE.controllable_ticks = 0
        STATE.locked_ticks = STATE.locked_ticks + 1
        if STATE.locked_ticks > 150 then
            -- A grab or an unmarked lock: mash so it cannot stall the route.
            local dirs = { "left", "right", "up", "down" }
            bot:press_action()
            bot:set_input({ [dirs[tick % 4 + 1]] = true })
        end
        return
    end
    STATE.controllable_ticks = STATE.controllable_ticks + 1
    STATE.locked_ticks = 0

    local phase = STATE.phase
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
    else
        fail(bot, "unknown phase " .. tostring(phase))
    end
end
