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
    paint_steps_source = nil,
    milestone = nil,
    combat = {},
    next_phase = nil,
    heal = {},
}

-- Fields added after the first checked-in route. A --watch reload keeps the
-- old table, so fill any field that the previous chunk did not define.
BOT_STATE.saw_pickup = BOT_STATE.saw_pickup or false
BOT_STATE.did_117 = BOT_STATE.did_117 or false
BOT_STATE.did_11a = BOT_STATE.did_11a or false
BOT_STATE.puzzle_index = BOT_STATE.puzzle_index or 1
BOT_STATE.puzzle_failed = BOT_STATE.puzzle_failed or false
BOT_STATE.milestone = BOT_STATE.milestone or nil
BOT_STATE.combat = BOT_STATE.combat or {}
BOT_STATE.next_phase = BOT_STATE.next_phase or nil
BOT_STATE.heal = BOT_STATE.heal or {}
BOT_STATE.heal.cooldown = BOT_STATE.heal.cooldown or 0
BOT_STATE.heal.uses = BOT_STATE.heal.uses or 0
BOT_STATE.heal.last_health = BOT_STATE.heal.last_health or nil
BOT_STATE.crest = BOT_STATE.crest or {}
BOT_STATE.crest.index = BOT_STATE.crest.index or 1
BOT_STATE.crest.stair_203_done = BOT_STATE.crest.stair_203_done or false
BOT_STATE.crest.stair_nudge = BOT_STATE.crest.stair_nudge or 0
BOT_STATE.crest.clear_202_positioned = BOT_STATE.crest.clear_202_positioned or false
BOT_STATE.last_health = BOT_STATE.last_health or nil
BOT_STATE.stale_attack_logged = BOT_STATE.stale_attack_logged or false

-- 10C greenhouse progress. `herb_slots` is built from the room actions the
-- first time the phase runs; `herb_index` walks that list across ticks.
BOT_STATE.tenc = BOT_STATE.tenc or {}
BOT_STATE.tenc.stage = BOT_STATE.tenc.stage or 1
BOT_STATE.tenc.herb_slots = BOT_STATE.tenc.herb_slots or nil
BOT_STATE.tenc.herb_index = BOT_STATE.tenc.herb_index or 1
BOT_STATE.tenc.key_line_stage = BOT_STATE.tenc.key_line_stage or 1
BOT_STATE.tenc.retreat_reason = BOT_STATE.tenc.retreat_reason or nil

-- Combat is a conservative layer over the route: it only stops to shoot the
-- common zombies with the handgun, and stands down for bosses, crowds, low
-- health or an empty clip it cannot reload. The route's normal walk still owns
-- every other tick, and scenes/doors/messages are handled before this runs.
BOT_STATE.combat.active = BOT_STATE.combat.active or false
BOT_STATE.combat.target = BOT_STATE.combat.target or nil
BOT_STATE.combat.ticks = BOT_STATE.combat.ticks or 0
BOT_STATE.combat.cooldown = BOT_STATE.combat.cooldown or 0
BOT_STATE.combat.aim_ticks = BOT_STATE.combat.aim_ticks or 0
BOT_STATE.combat.reload_ticks = BOT_STATE.combat.reload_ticks or 0
BOT_STATE.combat.reload_started = BOT_STATE.combat.reload_started or false
BOT_STATE.combat.retreating = BOT_STATE.combat.retreating or false
BOT_STATE.combat.retreat_ticks = BOT_STATE.combat.retreat_ticks or 0
BOT_STATE.combat.shots = BOT_STATE.combat.shots or 0
BOT_STATE.combat.room = BOT_STATE.combat.room or nil
BOT_STATE.combat.disarmed = BOT_STATE.combat.disarmed or false
BOT_STATE.combat.last_health = BOT_STATE.combat.last_health or nil
BOT_STATE.combat.start_health = BOT_STATE.combat.start_health or nil
BOT_STATE.combat.start_clip = BOT_STATE.combat.start_clip or nil
BOT_STATE.combat.start_reserve = BOT_STATE.combat.start_reserve or nil

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
local ITEM_HANDGUN = 0x02
local ITEM_SHIELD_KEY = 0x35
local ITEM_BLUE_JEWEL_KEY = 0x21
local ITEM_ARMOR_KEY = 0x34
local ITEM_SUN_CREST = 0x2E
local ITEM_MOON_CREST = 0x2C
local SCENARIO_HAS_LOCKPICK = 0x7C
local F_SCENARIO_0B = 0x0B

-- Combat thresholds. The route only needs the 10A/109 zombies, so it engages
-- one or two plain zombies with the starting handgun and runs past everything
-- else (bosses, crowds, low health, no ammunition).
local COMBAT_RANGE = 4500
local COMBAT_ARC = 0x400
local COMBAT_MIN_HEALTH = 40
local COMBAT_MAX_TICKS = 420
local COMBAT_MAX_THREATS = 1
local COMBAT_FIRE_CADENCE = 14
local COMBAT_LOCK_WAIT = 90
local COMBAT_RELOAD_WAIT = 45
-- Without spare ammunition, keep enough rounds for a full zombie plus the
-- misses a reaction window causes: a handgun hit does 12, and the 10A/109
-- zombies have about 70 health.
local COMBAT_MIN_CLIP = 9
-- The handgun's aim cone excludes bodies closer than their collision radius
-- plus 200 units, so a zombie that walks right up to the player is unshootable.
-- Back away before that happens, then resume firing.
local COMBAT_RETREAT_START = 1800
local COMBAT_RETREAT_DONE = 2600
local COMBAT_RETREAT_WAIT = 150

-- Healing: between fights, one green herb below 60 health, at most once per
-- 60 controllable ticks. The host applies the menu's USE rule and consumes a
-- unit, so this cannot set health or inventory directly.
local HEAL_THRESHOLD = 60
local HEAL_COOLDOWN = 60

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

local function abs(value)
    if value < 0 then
        return -value
    end
    return value
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
            .. "items(ribbon=%d ammo=%d herb=%d star=%d radio=%d desk=%d crank=%d "
            .. "shield=%d blue=%d armor=%d sun=%d moon=%d) "
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
        bot:item_count(ITEM_SHIELD_KEY),
        bot:item_count(ITEM_BLUE_JEWEL_KEY),
        bot:item_count(ITEM_ARMOR_KEY),
        bot:item_count(ITEM_SUN_CREST),
        bot:item_count(ITEM_MOON_CREST),
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
    BOT_STATE.stale_attack_logged = false
    BOT_STATE.travel_line = false
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
        -- Mashes only for a real hold. The engine's attacked byte is also the
        -- shared committed-action byte, and a plant bite leaves it set (the
        -- 10C fountains never clear it), so mashing on every nonzero value
        -- would turn the rest of the room into a random-walk stunlock. The
        -- Plant 42/Yawn holds raise `grabbed`; the wasp and vine holds use
        -- player states 5/6. The vault/stair sequences use the same attacked
        -- byte with the 0x80 bit, and their second press is what completes the
        -- climb, so keep mashing for those too.
        local held = player.grabbed or player.state == 5 or player.state == 6
            or (player.being_attacked & 0x80) ~= 0
        if held then
            local dirs = { "left", "right", "up", "down" }
            bot:press_action()
            bot:set_input({ [dirs[tick % 4 + 1]] = true })
            return true
        end
        if not BOT_STATE.stale_attack_logged then
            BOT_STATE.stale_attack_logged = true
            log(bot, string.format(
                "ignore stale attacked=%d state=%d grabbed=%s health=%d",
                player.being_attacked, player.state, tostring(player.grabbed),
                player.health))
        end
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

-- A straight pad walk for a corridor narrower than the 512-unit A* grid.
-- The 10C south bed's free band (z 3072..3570) has no free grid row, so the
-- generic planner cannot cross it even though a body fits. This stays a
-- normal pad input: face the point, then hold forward until the tolerance.
local function walk_line(bot, x, z, tolerance, label)
    local distance = bot:distance(x, z)
    if distance <= tolerance then
        return "arrived"
    end
    local facing = bot:face_to(x, z)
    if facing.status == "facing" then
        local input = { up = true }
        if distance > RUN_MIN_DISTANCE then
            input.run = true
        end
        bot:set_input(input)
    end
    return "moving"
end

-- Straight-line waypoints for a long corridor the A* grid handles poorly or
-- where the route needs to outrun a chaser. Uses the same shared waypoint
-- index as walk_waypoints.
local function line_waypoints(bot, points, tolerance, label)
    local index = BOT_STATE.wp or 1
    if index > #points then
        return true
    end
    local point = points[index]
    local result = walk_line(bot, point[1], point[2], tolerance or 150, label)
    if result == "arrived" then
        log(bot, string.format(
            "%s: line waypoint %d/%d arrived at (%d,%d) d=%d",
            label, index, #points, point[1], point[2],
            bot:distance(point[1], point[2])))
        BOT_STATE.wp = index + 1
        return BOT_STATE.wp > #points
    end
    return false
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
            walk_tolerance = opts and opts.walk_tolerance or 120,
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
            tolerance = nav.walk_tolerance or 120,
        })
        if result.status == "arrived" then
            nav.pressing = true
            nav.ticks = 0
            log(bot, string.format(
                "%s: at approach %d/%d (%d,%d) d=%d",
                label, nav.index, #nav.points, point[1], point[2], result.distance
            ))
            -- Arm on the arrival tick instead of spending one more frame
            -- turning/standing: a room hazard can knock us out of reach
            -- between the two ticks.
            local facing = bot:face_to(point[3], point[4])
            if facing.status == "facing" then
                nav.pressed = true
                bot:press_action()
            end
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

-- Extended crest/key route ---------------------------------------------------
-- After the first 11A visit the route goes back through the east wing, then
-- through the main hall and west stairs to the armor key, the sun crest and
-- the moon crest. Every leg is a normal `door_nav` to the next room; the
-- `next_phase` hand-off lets `enter_room` resume the route without the old
-- room-id defaults (which only describe the outward opening route).
local TRAVEL_PHASES = {
    to_10a_from_11a = {
        target = "10A", next_phase = "to_109_from_10a",
        label = "door 11A->10A", deadline = 1200,
        custom = { { 4400, 3850, 5600, 3850 } },
    },
    to_109_from_10a = {
        target = "109", next_phase = "to_108_from_109",
        label = "door 10A->109", deadline = 1200,
        custom = { { 14900, 3700, 16200, 3700 } },
    },
    to_108_from_109 = {
        target = "108", next_phase = "to_107_from_108",
        label = "door 109->108", deadline = 2400,
        points = { { 20000, 5000 } },
        custom = { { 24400, 2800, 24400, 1650 } },
    },
    to_107_from_108 = {
        target = "107", next_phase = "to_106_from_107",
        label = "door 108->107", deadline = 3000,
        points = { { 23500, 20000 }, { 12000, 20000 } },
        custom = { { 3500, 4200, 1600, 4200 } },
    },
    to_106_from_107 = {
        target = "106", next_phase = "to_105_from_106",
        label = "door 107->106", deadline = 2400,
        points = { { 14000, 3000 }, { 9000, 3000 } },
        custom = { { 2950, 6750, 1300, 6750 } },
    },
    to_105_from_106 = {
        target = "105", next_phase = "to_105_items",
        label = "door 106->105", deadline = 1800,
        custom = { { 3250, 16900, 1975, 16900 } },
    },
    to_106_from_105 = {
        target = "106", next_phase = "to_203_from_106",
        label = "door 105->106", deadline = 1800,
        custom = { { 31300, 8300, 33000, 8300 } },
    },
    to_203_from_106 = {
        target = "203", next_phase = "to_202_from_203",
        label = "door 106->203", deadline = 1800,
        points = { { 17000, 15400 } },
        custom = { { 17000, 15400, 17000, 17500 } },
    },
    to_201_from_202 = {
        target = "201", next_phase = "to_101_from_201",
        label = "door 202->201", deadline = 4200,
        points = {
            { 16000, 3000 }, { 4608, 3000 }, { 3072, 5120 },
            { 3072, 11776 }, { 4096, 12800 }, { 4608, 13824 },
            { 6144, 13824 },
        },
        custom = { { 6150, 13800, 6150, 15650 } },
    },
    to_101_from_201 = {
        target = "101", next_phase = "to_103_from_101",
        label = "door 201->101", deadline = 4200,
        points = {
            { 15872, 4608 }, { 15872, 6656 }, { 14336, 8192 },
            { 3584, 8192 }, { 3072, 8704 }, { 3072, 12800 },
            { 4608, 13824 }, { 14336, 14336 }, { 15360, 11776 },
        },
        custom = { { 14208, 11000, 12600, 11000 } },
    },
    to_103_from_101 = {
        target = "103", next_phase = "to_10c_from_103",
        label = "door 101->103", deadline = 4200,
        points = {
            { 3600, 4500 }, { 3000, 6700 },
        },
        -- The corridor zombie chases this leg. A straight run at the west
        -- edge outruns it; the generic A* leg walked and was grabbed twice.
        line_points = {
            { 2800, 18000 }, { 6200, 19500 }, { 7000, 21000 },
            { 18000, 21000 }, { 19000, 20400 },
        },
        custom = { { 19100, 20350, 20400, 20350 } },
    },
    to_10c_from_103 = {
        target = "10C", next_phase = "to_10c_item",
        label = "door 103->10C", deadline = 2400,
        custom = { { 18700, 18900, 20250, 18900 } },
    },
    to_103_from_10c = {
        target = "103", next_phase = "to_101_from_103",
        label = "door 10C->103", deadline = 1200,
        custom = { { 13900, 8300, 15100, 8300 } },
    },
    to_101_from_103 = {
        target = "101", next_phase = "to_201_from_101",
        label = "door 103->101", deadline = 2400,
        custom = { { 2800, 24750, 1250, 24750 } },
    },
    to_201_from_101 = {
        target = "201", next_phase = "to_202_from_201",
        label = "door 101->201", deadline = 2400,
        custom = { { 7600, 3150, 6000, 3150 } },
    },
    to_202_from_201 = {
        target = "202", next_phase = "to_203_from_202",
        label = "door 201->202", deadline = 1800,
        custom = { { 18200, 3200, 18200, 1900 } },
    },
    to_203_from_202 = {
        target = "203", next_phase = "to_204_from_203",
        label = "door 202->203", deadline = 2400,
        custom = { { 30700, 8550, 32400, 8550 } },
    },
    to_204_from_203 = {
        target = "204", next_phase = "to_205_from_204",
        label = "door 203->204", deadline = 2400,
        custom = { { 31100, 20500, 32400, 20500 } },
    },
    to_205_from_204 = {
        target = "205", next_phase = "to_205_item",
        label = "door 204->205 (armor key)", deadline = 2400,
        -- 204's corridor is open only east of the big block (x >= 18126);
        -- the door zone reaches x 18800, so approach from the east.
        custom = { { 19300, 12100, 17600, 12100 } },
    },
    to_204_from_205 = {
        target = "204", next_phase = "to_20d_from_204",
        label = "door 205->204", deadline = 1800,
        custom = { { 16100, 7300, 17400, 7300 } },
    },
    to_20d_from_204 = {
        target = "20D", next_phase = "to_20e_from_20d",
        label = "door 204->20D (armor key)", deadline = 2400,
        custom = { { 12450, 3000, 12450, 1900 } },
    },
    to_20e_from_20d = {
        target = "20E", next_phase = "to_210_from_20e",
        label = "door 20D->20E", deadline = 2400,
        custom = { { 11700, 2650, 13150, 2650 } },
    },
    to_210_from_20e = {
        target = "210", next_phase = "to_210_item",
        label = "door 20E->210 (shield key)", deadline = 2400,
        custom = { { 8900, 3150, 10050, 3150 } },
    },
    to_20e_from_210 = {
        target = "20E", next_phase = "to_20d_from_20e",
        label = "door 210->20E", deadline = 3000,
        points = { { 4600, 20000 }, { 4600, 6000 } },
        custom = { { 4400, 4600, 4400, 3250 } },
    },
    to_20d_from_20e = {
        target = "20D", next_phase = "to_204_from_20d",
        label = "door 20E->20D", deadline = 1800,
        custom = { { 2500, 4400, 1350, 4400 } },
    },
    to_204_from_20d = {
        target = "204", next_phase = "to_203_from_204",
        label = "door 20D->204", deadline = 2400,
        custom = { { 3000, 15200, 3000, 13900 } },
    },
    to_203_from_204 = {
        target = "203", next_phase = "to_106_from_203",
        label = "door 204->203", deadline = 1800,
        custom = { { 3100, 3700, 1850, 3700 } },
    },
    to_106_from_203 = {
        target = "106", next_phase = "to_107_from_106",
        label = "door 203->106", deadline = 2400,
        custom = { { 17000, 23800, 17000, 22500 } },
    },
    to_107_from_106 = {
        target = "107", next_phase = "to_108_from_107",
        label = "door 106->107", deadline = 2400,
        custom = { { 30800, 11500, 32100, 11500 } },
    },
    to_108_from_107 = {
        target = "108", next_phase = "to_109_from_108",
        label = "door 107->108", deadline = 2400,
        points = { { 9000, 3000 }, { 14000, 3000 } },
        custom = { { 15100, 4200, 16550, 4200 } },
    },
    to_109_from_108 = {
        target = "109", next_phase = "to_10a_from_109",
        label = "door 108->109", deadline = 3000,
        points = { { 23500, 12000 }, { 23500, 20000 } },
        custom = { { 24800, 25200, 24800, 26700 } },
    },
    to_10a_from_109 = {
        target = "10A", next_phase = "to_11a_from_10a",
        label = "door 109->10A", deadline = 3000,
        points = { { 20000, 3000 }, { 9000, 5000 } },
        custom = { { 2900, 8550, 1300, 8550 } },
    },
    to_11a_from_10a = {
        target = "11A", next_phase = "place_crests_11a",
        label = "door 10A->11A", deadline = 2400,
        points = { { 2600, 8000 }, { 2600, 11850 } },
        custom = { { 2500, 11850, 1200, 11850 } },
    },
}

-- A locked door with a key the route did not obtain must stop here, not be
-- forced. A raised lock-bank flag means the room already opened it.
local function locked_door_blocker(bot, target)
    local door = door_by_target(bot, target)
    if not door then
        return nil
    end
    local lock = door.lock or 0
    local key = door.key or 0
    if lock < 0x80 or key == 0 then
        return nil
    end
    if bot:flag(2, lock % 64) or bot:has_item(key) then
        return nil
    end
    return string.format(
        "locked door %s needs key 0x%02X (lock=0x%02X bit=%d flag=%s); key not held",
        target, key, lock, lock % 64, tostring(bot:flag(2, lock % 64))
    )
end

local function phase_travel(bot, spec)
    if spec.points and not BOT_STATE.travel_line then
        if not walk_waypoints(bot, spec.points, spec.tolerance or 150, spec.label) then
            if BOT_STATE.phase_ticks > spec.deadline then
                fail(bot, spec.label .. " waypoint timed out; "
                    .. door_diag(bot, spec.target))
            end
            return
        end
        if spec.line_points then
            BOT_STATE.travel_line = true
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
            return
        end
    end
    if spec.line_points then
        if not line_waypoints(bot, spec.line_points, spec.tolerance or 150,
            spec.label) then
            if BOT_STATE.phase_ticks > spec.deadline then
                fail(bot, spec.label .. " line waypoint timed out; "
                    .. door_diag(bot, spec.target))
            end
            return
        end
    end
    local blocker = locked_door_blocker(bot, spec.target)
    if blocker then
        log(bot, blocker)
        if spec.target == "210" then
            -- The sun-crest route reaches 20E before the shield-key door.
            -- Stop here with the crest in hand instead of forcing the lock or
            -- looping back through the east wing for a key that is not held.
            local p = bot:player()
            log(bot, string.format(
                "205 sun crest reached; moon crest blocked at 20E pos=(%d,%d) "
                    .. "health=%d sun=%d shield=%d armor=%d",
                p.x, p.z, p.health, bot:item_count(ITEM_SUN_CREST),
                bot:item_count(ITEM_SHIELD_KEY), bot:item_count(ITEM_ARMOR_KEY)))
            BOT_STATE.milestone = "205 sun crest"
            bot:finish(string.format(
                "205 sun crest taken; moon crest blocked: %s "
                    .. "(sun=%d shield=%d armor=%d)",
                blocker, bot:item_count(ITEM_SUN_CREST),
                bot:item_count(ITEM_SHIELD_KEY), bot:item_count(ITEM_ARMOR_KEY)))
            return
        end
        fail(bot, blocker)
        return
    end
    BOT_STATE.next_phase = spec.next_phase
    door_nav(bot, spec.target, spec.custom or {}, spec.label)
    if BOT_STATE.phase_ticks > spec.deadline then
        fail(bot, spec.label .. " timed out; " .. door_diag(bot, spec.target))
    end
end

-- The 203 stair landing is an in-room `check_door` zone, not a door. Pressing
-- action in it starts the room's climb clip and warps the player onto the
-- balcony/east corridor without a room change. The route walks to the west
-- side of the east flight, faces into it, then uses the same handler a normal
-- action press runs. Success is the player's own position jump.
local function stair_action_slot(bot, min_x)
    local actions = bot:actions()
    for i = 1, #actions do
        if actions[i].kind == "door_check" and actions[i].x >= min_x then
            return actions[i].slot
        end
    end
    return nil
end

local function stair_nav(bot, slot, label, approach)
    local action = action_by_slot(bot, slot)
    local nav = BOT_STATE.nav
    if not nav or nav.kind ~= "stair" or nav.slot ~= slot then
        if not action or action.kind ~= "door_check" then
            log(bot, string.format(
                "%s: stair check slot %s is not present (kind=%s)",
                label, tostring(slot), tostring(action and action.kind or "missing")))
            return true
        end
        local p = bot:player()
        nav = {
            kind = "stair", slot = slot, label = label,
            x = p.x, y = p.y, z = p.z,
            pressed = false, ticks = 0,
        }
        BOT_STATE.nav = nav
        log(bot, string.format(
            "%s: stair check slot=%d zone=(%d,%d,%d,%d) pos=(%d,%d,%d)",
            label, slot, action.x, action.z, action.width, action.depth,
            p.x, p.y, p.z))
    end
    local p = bot:player()
    if nav.pressed then
        local moved = abs(p.x - nav.x) + abs(p.y - nav.y) + abs(p.z - nav.z)
        if moved > 1000 then
            log(bot, string.format(
                "%s: stair warp landed pos=(%d,%d,%d) moved=%d",
                label, p.x, p.y, p.z, moved))
            BOT_STATE.nav = nil
            return true
        end
        nav.ticks = nav.ticks + 1
        if nav.ticks > 600 then
            fail(bot, string.format(
                "%s: stair warp did not move the player (pos=(%d,%d,%d))",
                label, p.x, p.y, p.z))
        end
        return false
    end
    local result = bot:walk_to(approach[1], approach[2], {
        run = bot:distance(approach[1], approach[2]) >= RUN_MIN_DISTANCE,
        tolerance = 120,
    })
    if result.status == "arrived" then
        local facing = bot:face_to(approach[3], approach[4])
        if facing.status == "facing" then
            nav.pressed = true
            nav.ticks = 0
            log(bot, string.format(
                "%s: pressing stair check at (%d,%d) d=%d",
                label, approach[1], approach[2], result.distance))
            bot:use_stair(slot)
            bot:press_action()
        end
    elseif result.status ~= "moving" then
        fail(bot, string.format(
            "%s: stair approach (%d,%d) %s d=%d",
            label, approach[1], approach[2], result.status, result.distance))
    end
    if BOT_STATE.phase_ticks > 1800 then
        fail(bot, label .. ": stair approach timed out")
    end
    return false
end

local function phase_clear_202(bot)
    if not BOT_STATE.crest.clear_202_positioned then
        if not walk_waypoints(bot, { { 20000, 3000 } }, 150, "202 south approach") then
            if BOT_STATE.phase_ticks > 1200 then
                fail(bot, "202 south approach timed out")
            end
            return
        end
        BOT_STATE.crest.clear_202_positioned = true
    end
    local target = nil
    local enemies = bot:enemies()
    for i = 1, #enemies do
        local enemy = enemies[i]
        local common = enemy.id == 0x00 or enemy.id == 0x01 or enemy.id == 0x11
        local dx, dz = enemy.x - 16416, enemy.z - 2384
        if common and dx * dx + dz * dz <= 3500 * 3500 then
            target = enemy
            break
        end
    end
    if not target then
        set_phase(bot, "to_201_from_202", "202 south zombie cleared")
        return
    end
    if BOT_STATE.combat.disarmed then
        BOT_STATE.combat.disarmed = false
    end
    local p = bot:player()
    bot:face_to(target.x, target.z)
    if BOT_STATE.phase_ticks % 60 == 0 then
        log(bot, string.format(
            "202 south zombie wait slot=%d dist=%d health=%d player_health=%d clipped=%s",
            target.slot, target.distance, target.health, p.health,
            tostring(BOT_STATE.combat.disarmed)))
    end
    if BOT_STATE.phase_ticks > 900 then
        log(bot, "202 south zombie not cleared; running past")
        set_phase(bot, "to_201_from_202", "south zombie still up")
    end
end

local function phase_to_202_from_203(bot)
    if not BOT_STATE.crest.stair_203_done then
        local slot = stair_action_slot(bot, 20000)
        if slot then
            if not stair_nav(bot, slot, "203 east flight", { 19700, 25650, 21990, 25650 }) then
                return
            end
        else
            log(bot, "203 east stair check not present; trying the room floor directly")
        end
        BOT_STATE.crest.stair_203_done = true
        -- The warp lands only just past the stair block's collision, inside
        -- the grown radius the planner uses. Walk east clear of it before
        -- planning, otherwise every straight segment starts blocked.
        BOT_STATE.crest.stair_nudge = 25
    end
    if BOT_STATE.crest.stair_nudge > 0 then
        BOT_STATE.crest.stair_nudge = BOT_STATE.crest.stair_nudge - 1
        bot:face_to(26000, 25650)
        bot:set_input({ up = true })
        return
    end
    -- The climb lands east of the stair block. Go east of it, then south down
    -- the balcony corridor into the open west strip and the 202 door.
    local points = {
        { 26000, 25000 },
        { 26000, 16000 },
        { 3000, 16900 },
    }
    if walk_waypoints(bot, points, 150, "203 balcony") then
        local custom = { { 3000, 16900, 1850, 16900 } }
        BOT_STATE.next_phase = "clear_202"
        door_nav(bot, "202", custom, "door 203->202")
    end
    if BOT_STATE.phase_ticks > 3000 then
        fail(bot, "door 203->202 timed out; " .. door_diag(bot, "202"))
    end
end

-- The 117 gallery paintings, in the only order that satisfies the script's
-- room-bit requires. Each entry walks to an approach on the reachable side of
-- the wall (north for AOT_02/03, south for AOT_04..07) and presses action.
-- These are the fallback when the room's authored `bot.xml` metadata is absent;
-- `paint_steps` below prefers the same steps from `bot:room_meta()`.
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

-- Resolve the paintings puzzle from the room metadata, with the hardcoded
-- table as a fallback. The Rust host parses `room/117/bot.xml`; the Lua route
-- still owns execution and state. Returns the step list and a source label so
-- the route can log which table it is using once.
local function paint_steps(bot)
    local meta = bot:room_meta()
    if meta then
        for _, puzzle in ipairs(meta.puzzles or {}) do
            if puzzle.id == "paintings" then
                local steps = {}
                for _, step in ipairs(puzzle.steps or {}) do
                    local points = {}
                    for _, walk in ipairs(step.walks or {}) do
                        points[#points + 1] = { walk.x, walk.z }
                    end
                    steps[#steps + 1] = {
                        slot = step.slot,
                        bank = step.bank,
                        bit = step.bit,
                        label = step.label,
                        points = points,
                        face = step.face and { step.face.x, step.face.z } or nil,
                    }
                end
                if #steps > 0 then
                    return steps, "room/117/bot.xml paintings puzzle"
                end
            end
        end
    end
    return PAINT_STEPS,
        "hardcoded PAINT_STEPS (no room/117/bot.xml paintings puzzle)"
end

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
    local facing
    if step.face then
        facing = bot:face_to(step.face[1], step.face[2])
        if facing.status ~= "facing" then
            return false
        end
    end
    nav.presses = nav.presses + 1
    nav.last_press = nav.ticks
    log(bot, string.format("%s: press %d (slot=%d, waiting for flag %d:0x%02X)",
        step.label, nav.presses, step.slot, step.bank, step.bit))
    bot:press_action()
    return false
end

local function run_117_puzzle(bot)
    local steps, source = paint_steps(bot)
    if BOT_STATE.paint_steps_source ~= source then
        BOT_STATE.paint_steps_source = source
        log(bot, "117 paintings: using " .. source)
    end
    local index = BOT_STATE.puzzle_index or 1
    if index > #steps then
        return true
    end
    local step = steps[index]
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

local function crest_hollows(bot)
    local count = 0
    local bits = { F_SCENARIO_69, F_SCENARIO_6A, F_SCENARIO_6B, F_SCENARIO_6C }
    for i = 1, #bits do
        if bot:flag(0, bits[i]) then
            count = count + 1
        end
    end
    return count
end

local function held_crest_names(bot)
    local held = {}
    if bot:item_count(ITEM_SUN_CREST) > 0 then held[#held + 1] = "sun" end
    if bot:item_count(ITEM_MOON_CREST) > 0 then held[#held + 1] = "moon" end
    if bot:item_count(ITEM_STAR_CREST) > 0 then held[#held + 1] = "star" end
    if bot:item_count(0x29) > 0 then held[#held + 1] = "wind" end
    if #held == 0 then
        return "none"
    end
    return table.concat(held, ",")
end

local function finish_11a_crests(bot, reason)
    local hollows = crest_hollows(bot)
    local opened = bot:flag(2, F_LOCK_17)
    local held = held_crest_names(bot)
    log(bot, string.format(
        "11A crest check: hollows=%d/4 held=%s lock17=%s 11B=%s "
            .. "sun=%d moon=%d star=%d wind=%d (%s)",
        hollows, held, tostring(opened), opened and "open" or "locked",
        bot:item_count(ITEM_SUN_CREST), bot:item_count(ITEM_MOON_CREST),
        bot:item_count(ITEM_STAR_CREST), bot:item_count(0x29), reason
    ))
    BOT_STATE.milestone = "11A crests"
    bot:finish(string.format(
        "11A crests: hollows %d/4, held %s, 11B %s (%s)",
        hollows, held, opened and "open" or "locked", reason
    ))
end

-- In 11A the room itself tests each crest in the inventory and plays its
-- placement event. The route only waits on the resulting scenario flag; it
-- never calls use_action, sets a flag or forces F_LOCK_17.
local function phase_place_crests_11a(bot)
    local expected = {
        { item = ITEM_SUN_CREST, bit = F_SCENARIO_6A, name = "sun" },
        { item = ITEM_MOON_CREST, bit = F_SCENARIO_6B, name = "moon" },
        { item = ITEM_STAR_CREST, bit = F_SCENARIO_69, name = "star" },
        { item = 0x29, bit = F_SCENARIO_6C, name = "wind" },
    }
    local pending = {}
    for i = 1, #expected do
        local crest = expected[i]
        if bot:item_count(crest.item) > 0 and not bot:flag(0, crest.bit) then
            pending[#pending + 1] = crest.name
        end
    end
    if #pending == 0 then
        finish_11a_crests(bot, "placement flags set")
        return
    end
    if BOT_STATE.phase_ticks > 2400 then
        finish_11a_crests(bot, "placement timeout waiting for "
            .. table.concat(pending, ","))
    end
end

local function phase_to_105_items(bot)
    local crest = BOT_STATE.crest
    if crest.index == 1 then
        local count = bot:item_count(ITEM_SHIELD_KEY)
        if count > 0 then
            log(bot, string.format("105 shield key already held count=%d", count))
            crest.index = 2
            return
        end
        local slot = item_action_slot(bot, ITEM_SHIELD_KEY)
        if not slot then
            log(bot, "105 shield key action not present; count=0")
            crest.index = 2
            return
        end
        local result = false
        if walk_waypoints(bot, { { 14000, 13000 } }, 150, "105 dining room") then
            -- Exact geometry: the north wall (x414..35604, z14454..16632)
            -- grown by Jill's 372 radius plus the 18-unit nav skin leaves
            -- z <= 14064. The sideboard (18000,14300, half extents 700/600)
            -- grown by the same radius covers x 16928..19072, z 13328..15272.
            -- The only corner left is (16928,14064); its 600 reach ends at
            -- (17377,14462), 23 short in x and 38 short in z of the zone's
            -- south-west corner (17400,14500). Every other side is farther,
            -- so this logs the block and continues without forcing the item.
            result = action_nav(bot, slot, "105 shield key", {
                { 16900, 14060, 17900, 14600 },
            }, { optional = true })
        end
        if result == true then
            log(bot, string.format("105 shield key count=%d",
                bot:item_count(ITEM_SHIELD_KEY)))
            crest.index = 2
            return
        end
        if result == "failed" then
            log(bot, string.format(
                "105 shield key unreachable; count=%d",
                bot:item_count(ITEM_SHIELD_KEY)))
            crest.index = 2
            return
        end
        if BOT_STATE.phase_ticks > 2400 then
            fail(bot, "105 shield key timed out; count="
                .. bot:item_count(ITEM_SHIELD_KEY))
        end
        return
    end
    if crest.index == 2 then
        local count = bot:item_count(ITEM_BLUE_JEWEL_KEY)
        if count > 0 then
            log(bot, string.format("105 blue jewel key already held count=%d", count))
            crest.index = 3
            return
        end
        local slot = item_action_slot(bot, ITEM_BLUE_JEWEL_KEY)
        if not slot then
            log(bot, string.format(
                "105 blue jewel key action not present (scenario 0x0B=%s); count=%d",
                tostring(bot:flag(0, F_SCENARIO_0B)), count))
            crest.index = 3
            return
        end
        local result = action_nav(bot, slot, "105 blue jewel key", {
            { 11700, 4750, 12500, 4750 },
        }, { optional = true })
        if result == true then
            log(bot, string.format("105 blue jewel key count=%d",
                bot:item_count(ITEM_BLUE_JEWEL_KEY)))
            crest.index = 3
            return
        end
        if result == "failed" then
            log(bot, string.format(
                "105 blue jewel key unreachable (scenario 0x0B=%s); count=%d",
                tostring(bot:flag(0, F_SCENARIO_0B)),
                bot:item_count(ITEM_BLUE_JEWEL_KEY)))
            crest.index = 3
            return
        end
        if BOT_STATE.phase_ticks > 2400 then
            fail(bot, "105 blue jewel key timed out; count="
                .. bot:item_count(ITEM_BLUE_JEWEL_KEY))
        end
        return
    end
    set_phase(bot, "to_106_from_105", "105 items done")
end

local function finish_10c_retreat(bot, reason)
    local p = bot:player()
    local hollows = crest_hollows(bot)
    log(bot, string.format(
        "10C retreat: %s room=%s pos=(%d,%d) health=%d/%d armor=%d "
            .. "shield=%d blue=%d sun=%d moon=%d hollows=%d/4",
        reason, bot:room().id, p.x, p.z, p.health, p.max_health,
        bot:item_count(ITEM_ARMOR_KEY), bot:item_count(ITEM_SHIELD_KEY),
        bot:item_count(ITEM_BLUE_JEWEL_KEY), bot:item_count(ITEM_SUN_CREST),
        bot:item_count(ITEM_MOON_CREST), hollows))
    BOT_STATE.milestone = "103"
    bot:finish(string.format(
        "103 retreat; 10C armor key blocked: %s (health=%d, armor=%d, "
            .. "shield=%d, blue=%d, sun=%d, moon=%d, hollows=%d/4)",
        reason, p.health, bot:item_count(ITEM_ARMOR_KEY),
        bot:item_count(ITEM_SHIELD_KEY), bot:item_count(ITEM_BLUE_JEWEL_KEY),
        bot:item_count(ITEM_SUN_CREST), bot:item_count(ITEM_MOON_CREST),
        hollows))
end

local function phase_retreat_10c(bot)
    local custom = { { 13900, 8300, 15100, 8300 } }
    BOT_STATE.next_phase = "report_10c_retreat"
    door_nav(bot, "103", custom, "door 10C->103 retreat")
    if BOT_STATE.phase_ticks > 2400 then
        finish_10c_retreat(bot, "10C->103 retreat door timed out")
    end
end

local function phase_report_10c_retreat(bot)
    finish_10c_retreat(bot, BOT_STATE.tenc.retreat_reason
        or "reached 103 without the armor key")
end

local function greenhouse_herb_actions(bot)
    local actions = bot:actions()
    local herbs = {}
    for i = 1, #actions do
        local action = actions[i]
        if action.kind == "item" and action.item == ITEM_GREEN_HERB then
            herbs[#herbs + 1] = action
        end
    end
    -- Take the 8100 pair first: the player's knockback from (8200,4800)
    -- lands around x 9000..9500, and the 8500 press point is still inside the
    -- zone's x range, so action_nav can fire from the knocked-back band
    -- without walking further west. The 7000 pair is behind that fountain's
    -- barricade and is tried only after the first heal.
    -- Insertion sort by x descending, then z descending.
    for i = 2, #herbs do
        local action = herbs[i]
        local j = i - 1
        while j >= 1
            and (herbs[j].x < action.x
                or (herbs[j].x == action.x and herbs[j].z < action.z)) do
            herbs[j + 1] = herbs[j]
            j = j - 1
        end
        herbs[j + 1] = action
    end
    return herbs
end

local function greenhouse_herb_approach(action)
    -- Stand east of the zone, outside the fountain's actual lunge reach. The
    -- 600 probe from (9300,3280) points at the zone centre and lands at
    -- (8596,2836), inside the 8100 zone. Walking further west is what gets
    -- bitten and knocked back before the reach can arm.
    local east = action.x + action.width + 400
    return { east, 3280, action.x + action.width // 2, 2000 + action.depth // 2 }
end

local function phase_to_10c_item(bot)
    local p = bot:player()
    if p.health <= 26 then
        log(bot, string.format(
            "10C health=%d at pos=(%d,%d); retreating to 103", p.health, p.x, p.z))
        BOT_STATE.tenc.retreat_reason = string.format(
            "green herbs unreachable; health=%d below the 26 key-lane floor",
            p.health)
        set_phase(bot, "retreat_10c", "health too low for the plants")
        return
    end
    local count = bot:item_count(ITEM_ARMOR_KEY)
    if count > 0 then
        log(bot, string.format("10C armor key already held count=%d", count))
        set_phase(bot, "to_103_from_10c", "armor key already held")
        return
    end
    local slot = item_action_slot(bot, ITEM_ARMOR_KEY)
    if not slot then
        log(bot, "10C armor key action not present; count=0")
        set_phase(bot, "to_103_from_10c", "no armor key action")
        return
    end

    local tenc = BOT_STATE.tenc
    -- The 10C collision splits the room into a north corridor, the east gap
    -- between the two central beds, and the narrow band z 3054..3588 south of
    -- the big bed. The only legal crossing from the 103 door to the west half
    -- is the north corridor, then down between the beds. Do not walk the
    -- direct line through the north bed: the A* grid sees the bed, but the
    -- old route still lingered in the fountains' Manhattan trigger radius.
    if tenc.stage == 1 then
        local points = {
            { 13000, 7800 },
            { 10200, 7800 },
            { 10200, 3600 },
        }
        if walk_waypoints(bot, points, 150, "10C south bed") then
            tenc.stage = 2
        end
        if BOT_STATE.phase_ticks > 900 then
            fail(bot, "10C south-bed approach timed out")
        end
        return
    end

    -- Pick every green herb on the south bed before the armor-key run. An
    -- herb the room refuses is left behind; the key is still the priority.
    if tenc.stage == 2 then
        if not tenc.herb_slots then
            tenc.herb_slots = greenhouse_herb_actions(bot)
            tenc.herb_index = 1
            tenc.herb_deadline = BOT_STATE.phase_ticks + 60
            log(bot, string.format("10C green herbs: %d actions on the south bed",
                #tenc.herb_slots))
        end
        if BOT_STATE.phase_ticks > (tenc.herb_deadline or 0) then
            local pl = bot:player()
            log(bot, string.format(
                "10C green herbs still unreachable pos=(%d,%d) health=%d; "
                    .. "leaving them for the key run",
                pl.x, pl.z, pl.health))
            tenc.stage = 3
            tenc.key_line_stage = 1
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
            return
        end
        local herb = tenc.herb_slots[tenc.herb_index]
        if not herb then
            tenc.stage = 3
            tenc.key_line_stage = 1
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
            return
        end
        local now = action_by_slot(bot, herb.slot)
        if not now or now.kind ~= "item" or now.item ~= ITEM_GREEN_HERB then
            tenc.herb_index = tenc.herb_index + 1
            return
        end
        local approach = greenhouse_herb_approach(now)
        local active = BOT_STATE.nav and BOT_STATE.nav.kind == "action"
            and BOT_STATE.nav.slot == herb.slot
        if not active
            and bot:distance(approach[1], approach[2]) > 520 then
            -- The bed band is narrower than the nav grid; walk it straight
            -- instead of asking A* for a row that does not exist.
            local line = walk_line(bot, approach[1], approach[2], 100,
                "10C herb bed")
            if BOT_STATE.phase_ticks % 30 == 0 then
                local pl = bot:player()
                log(bot, string.format(
                    "10C herb bed walk pos=(%d,%d) angle=%d state=%d attacked=%d "
                        .. "line=%s d=%d",
                    pl.x, pl.z, pl.angle, pl.state, pl.being_attacked, line,
                    bot:distance(approach[1], approach[2])))
            end
            if line ~= "arrived" then
                if BOT_STATE.phase_ticks > 1800 then
                    fail(bot, "10C herb-bed walk timed out")
                end
                return
            end
        end
        local result = action_nav(bot, herb.slot, "10C green herb", {
            approach,
        }, { optional = true, walk_tolerance = 500 })
        if result == true then
            log(bot, string.format("10C green herbs picked slot=%d count=%d",
                herb.slot, bot:item_count(ITEM_GREEN_HERB)))
            tenc.herb_index = tenc.herb_index + 1
            return
        end
        if result == "failed" then
            log(bot, string.format(
                "10C green herb slot=%d unreachable; continuing", herb.slot))
            tenc.herb_index = tenc.herb_index + 1
            return
        end
        if BOT_STATE.phase_ticks > 1800 then
            fail(bot, "10C green herb pickup timed out")
        end
        return
    end

    -- The key can only be reached from the east: the west wall's grown edge
    -- is x=5634. Stand at (6100,6300) (south of the north bed, outside its
    -- collision) and face west; the reach point (5500,6300) is inside the
    -- action zone. This is also the closest standable point to the safe west
    -- wall and keeps the press short.
    if tenc.stage == 3 then
        if tenc.key_line_stage == 1 then
            -- Get south of the planters first. The lane point stays east of
            -- the fountains' 3000-unit Manhattan trigger, then the west leg
            -- runs straight along the narrow band.
            if walk_line(bot, 10200, 3300, 100, "10C key lane east") ~= "arrived" then
                if BOT_STATE.phase_ticks > 1200 then
                    set_phase(bot, "retreat_10c", "key-lane east walk timed out")
                end
                return
            end
            tenc.key_line_stage = 2
        end
        if tenc.key_line_stage == 2 then
            if walk_line(bot, 6100, 3280, 100, "10C key line") ~= "arrived" then
                if BOT_STATE.phase_ticks > 1200 then
                    set_phase(bot, "retreat_10c", "key-line walk timed out")
                end
                return
            end
            tenc.key_line_stage = 3
            BOT_STATE.wp = 1
            BOT_STATE.wp_retry = false
            BOT_STATE.wp_tol = nil
        end
        if walk_waypoints(bot, { { 6100, 6300 } }, 150,
            "10C key corridor") then
            tenc.stage = 4
        end
        if BOT_STATE.phase_ticks > 1200 then
            set_phase(bot, "retreat_10c", "key corridor timed out")
        end
        return
    end

    local result = action_nav(bot, slot, "10C armor key", {
        { 6100, 6300, 5500, 6300 },
    }, { optional = true })
    if result == true then
        log(bot, string.format("10C armor key count=%d",
            bot:item_count(ITEM_ARMOR_KEY)))
        set_phase(bot, "to_103_from_10c", "armor key taken")
        return
    end
    if result == "failed" then
        log(bot, string.format("10C armor key unreachable; count=%d",
            bot:item_count(ITEM_ARMOR_KEY)))
        BOT_STATE.tenc.retreat_reason = "armor key action unreachable from the bed band"
        set_phase(bot, "retreat_10c", "armor key unreachable")
        return
    end
    if BOT_STATE.phase_ticks > 1800 then
        BOT_STATE.tenc.retreat_reason = "armor key pickup timed out"
        set_phase(bot, "retreat_10c", "armor key pickup timed out")
    end
end

local function phase_to_205_item(bot)
    local count = bot:item_count(ITEM_SUN_CREST)
    if count > 0 then
        log(bot, string.format("205 sun crest already held count=%d", count))
        set_phase(bot, "to_204_from_205", "sun crest already held")
        return
    end
    local slot = item_action_slot(bot, ITEM_SUN_CREST)
    if not slot then
        log(bot, "205 sun crest action not present; count=0")
        set_phase(bot, "to_204_from_205", "no sun crest action")
        return
    end
    -- The two armor-room pillars are object collision, which the A* grid does
    -- not see. Pass between them at z=7000 rather than straight west.
    if walk_waypoints(bot, { { 10000, 7000 } }, 150, "205 armor room") then
        local result = action_nav(bot, slot, "205 sun crest", {
            { 3900, 7350, 2500, 7350 },
        }, { optional = true })
        if result == true then
            log(bot, string.format("205 sun crest count=%d",
                bot:item_count(ITEM_SUN_CREST)))
            set_phase(bot, "to_204_from_205", "sun crest taken")
            return
        end
        if result == "failed" then
            log(bot, string.format("205 sun crest unreachable; count=%d",
                bot:item_count(ITEM_SUN_CREST)))
            set_phase(bot, "to_204_from_205", "sun crest unreachable")
            return
        end
    end
    if BOT_STATE.phase_ticks > 2400 then
        fail(bot, "205 sun crest timed out; count="
            .. bot:item_count(ITEM_SUN_CREST))
    end
end

local function phase_to_210_item(bot)
    local count = bot:item_count(ITEM_MOON_CREST)
    if count > 0 then
        log(bot, string.format("210 moon crest already held count=%d", count))
        set_phase(bot, "to_20e_from_210", "moon crest already held")
        return
    end
    local slot = item_action_slot(bot, ITEM_MOON_CREST)
    if not slot then
        log(bot, "210 moon crest action not present; count=0")
        set_phase(bot, "to_20e_from_210", "no moon crest action")
        return
    end
    -- Keep to the west wall on the way north: the Yawn spawns near
    -- (11000,10000) and must not be fought.
    if walk_waypoints(bot, { { 4600, 8000 }, { 4600, 20000 } }, 150, "210 attic west") then
        local result = action_nav(bot, slot, "210 moon crest", {
            { 4608, 22740, 4608, 24000 },
        }, { optional = true })
        if result == true then
            log(bot, string.format("210 moon crest count=%d",
                bot:item_count(ITEM_MOON_CREST)))
            set_phase(bot, "to_20e_from_210", "moon crest taken")
            return
        end
        if result == "failed" then
            log(bot, string.format("210 moon crest unreachable; count=%d",
                bot:item_count(ITEM_MOON_CREST)))
            set_phase(bot, "to_20e_from_210", "moon crest unreachable")
            return
        end
    end
    if BOT_STATE.phase_ticks > 3000 then
        fail(bot, "210 moon crest timed out; count="
            .. bot:item_count(ITEM_MOON_CREST))
    end
end

local function enter_room(bot, room, tick)
    local p = bot:player()
    log(bot, string.format(
        "room %s at tick %d pos=(%d,%d) camera=%d health=%d/%d "
            .. "common02=%s common03=%s common05=%s common64=%s common65=%s "
            .. "lockpick=%s scenario7c=%s scenario03=%s scenario69=%s "
            .. "scenario6a=%s scenario6b=%s scenario6c=%s lock17=%s "
            .. "items(ribbon=%d ammo=%d herb=%d star=%d radio=%d desk=%d crank=%d "
            .. "shield=%d blue=%d armor=%d sun=%d moon=%d)",
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
        bot:item_count(ITEM_CRANK_KEY),
        bot:item_count(ITEM_SHIELD_KEY),
        bot:item_count(ITEM_BLUE_JEWEL_KEY),
        bot:item_count(ITEM_ARMOR_KEY),
        bot:item_count(ITEM_SUN_CREST),
        bot:item_count(ITEM_MOON_CREST)
    ))
    BOT_STATE.room = room.id
    BOT_STATE.room_ticks = 0
    BOT_STATE.nav = nil
    BOT_STATE.wp = 1
    BOT_STATE.wp_retry = false
    BOT_STATE.wp_tol = nil

    -- A travel phase names the phase that owns the room it just entered. The
    -- outward route below only describes the opening trip; using this hand-off
    -- first keeps a return leg from falling back into a stale room default.
    if BOT_STATE.next_phase then
        local next_phase = BOT_STATE.next_phase
        BOT_STATE.next_phase = nil
        set_phase(bot, next_phase, "entered " .. room.id)
        return
    end

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
        set_phase(bot, "to_104_ammo", "first-zombie FMV done")
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

local function phase_to_104_ammo(bot)
    local slot = item_action_slot(bot, ITEM_HANDGUN_AMMO)
    if not slot then
        log(bot, "104 handgun ammo action not present; continuing")
        set_phase(bot, "return_104", "no ammo action")
        return
    end
    local result = action_nav(bot, slot, "104 handgun ammo", {
        { 4300, 5300, 4300, 5900 },
    }, { optional = true })
    if result == true then
        log(bot, string.format("104 handgun ammo count=%d",
            bot:item_count(ITEM_HANDGUN_AMMO)))
        set_phase(bot, "return_104", "ammo taken")
        return
    end
    if result == "failed" then
        log(bot, "104 handgun ammo unreachable; continuing")
        set_phase(bot, "return_104", "ammo unreachable")
        return
    end
    if BOT_STATE.phase_ticks > 900 then
        log(bot, "104 handgun ammo timed out; continuing")
        set_phase(bot, "return_104", "ammo timeout")
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
        set_phase(bot, "to_10a_from_11a", "no radio key action; return east")
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
            set_phase(bot, "to_10a_from_11a", "radio key done; return east")
            return
        end
        if result == "failed" then
            log(bot, string.format(
                "11A radio key unreachable; count=%d",
                bot:item_count(ITEM_RADIO_KEY)))
            set_phase(bot, "to_10a_from_11a", "radio key unreachable; return east")
            return
        end
    end
    if BOT_STATE.phase_ticks > 3000 then
        log(bot, "11A radio key timed out; continuing to the store-room door")
        set_phase(bot, "to_10a_from_11a", "radio key timeout; return east")
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

-- Combat policy -------------------------------------------------------------
-- The nearest visible monster in front of the player is a threat. The route
-- fights only common zombies with the handgun; bosses, crowds, low health and
-- an empty clip it cannot reload all make it stand down for the rest of the
-- room, so the normal route can run past instead.

local function angle_delta(a, b)
    local delta = (a - b) % 4096
    if delta > 2048 then
        delta = delta - 4096
    end
    return delta
end

local function is_common_zombie(enemy)
    return enemy.id == 0x00 or enemy.id == 0x01 or enemy.id == 0x11
end

-- The damage pipeline only accepts a shot when the monster's vertical status
-- bit matches the player's aim direction. Follow the target's own 0x20/0x80
-- bit so shots at a zombie on a step or in a bend-down reaction can connect.
local function combat_aim_input(target)
    local input = { aim = true }
    if target.status_flags & 0x20 ~= 0 then
        input.down = true
    elseif target.status_flags & 0x80 ~= 0 then
        input.up = true
    end
    return input
end

local function combat_enter_room(combat, room_id)
    if combat.room == room_id then
        return
    end
    combat.room = room_id
    combat.active = false
    combat.target = nil
    combat.ticks = 0
    combat.cooldown = 0
    combat.aim_ticks = 0
    combat.reload_ticks = 0
    combat.reload_started = false
    combat.shots = 0
    combat.disarmed = false
    combat.last_health = nil
    combat.retreating = false
    combat.retreat_ticks = 0
end

local function combat_end(bot, combat, reason, disarm)
    if not combat.active then
        if disarm then
            combat.disarmed = true
        end
        return
    end
    local player = bot:player()
    local weapon = bot:weapon()
    log(bot, string.format(
        "combat end: %s slot=%s shots=%d ticks=%d health=%d clip=%d reserve=%d",
        reason, tostring(combat.target), combat.shots, combat.ticks,
        player.health, weapon.clip, weapon.reserve
    ))
    combat.active = false
    combat.target = nil
    combat.ticks = 0
    combat.cooldown = 0
    combat.aim_ticks = 0
    combat.reload_ticks = 0
    combat.reload_started = false
    combat.last_health = nil
    combat.retreating = false
    combat.retreat_ticks = 0
    -- One fight per room is enough: once the target is lost (killed, out of
    -- range or out of sight), let the route run past anything else here.
    combat.disarmed = true
end

local function combat_start(bot, combat, enemy, player, weapon)
    combat.active = true
    combat.target = enemy.slot
    combat.ticks = 0
    combat.cooldown = 0
    combat.aim_ticks = 0
    combat.reload_ticks = 0
    combat.reload_started = false
    combat.shots = 0
    combat.last_health = enemy.health
    combat.retreating = false
    combat.retreat_ticks = 0
    combat.start_health = player.health
    combat.start_clip = weapon.clip
    combat.start_reserve = weapon.reserve
    log(bot, string.format(
        "combat start slot=%d id=%#04x dist=%d health=%d state=%d "
            .. "player_health=%d clip=%d reserve=%d",
        enemy.slot, enemy.id, enemy.distance, enemy.health, enemy.state,
        player.health, weapon.clip, weapon.reserve
    ))
end

-- Between fights: one green herb below 60 health. The host validates the item
-- again and applies the menu USE rule, so this only queues a legal action.
-- The cooldown is in controllable ticks; a grab, scene or menu never reaches
-- this function, and an active fight is allowed to finish first.
local function step_healing(bot, player)
    local heal = BOT_STATE.heal
    if heal.cooldown > 0 then
        heal.cooldown = heal.cooldown - 1
    end
    if player.grabbed or player.health >= HEAL_THRESHOLD then
        return false
    end
    if BOT_STATE.combat and BOT_STATE.combat.active then
        return false
    end
    if heal.cooldown > 0 or bot:item_count(ITEM_GREEN_HERB) <= 0 then
        return false
    end
    local health = player.health
    if not bot:use_heal(ITEM_GREEN_HERB) then
        heal.cooldown = HEAL_COOLDOWN
        return false
    end
    heal.cooldown = HEAL_COOLDOWN
    heal.uses = heal.uses + 1
    heal.last_health = health
    log(bot, string.format(
        "heal green herb health=%d uses=%d cooldown=%d",
        health, heal.uses, heal.cooldown))
    return true
end

local function combat_step(bot, scene, player)
    if not scene.controllable or player.grabbed then
        return false
    end
    local room = bot:room()
    local combat = BOT_STATE.combat
    if not combat then
        combat = {}
        BOT_STATE.combat = combat
    end
    combat_enter_room(combat, room.id)
    if combat.disarmed then
        return false
    end
    -- 101's corridor zombie is not on the route's critical path: the route
    -- can pass it on the west side and save the health for the 10C plants.
    if room.id == "101" then
        log(bot, "combat: bypassing the 101 corridor zombie to save health")
        combat.disarmed = true
        return false
    end

    local enemies = bot:enemies()
    local wide_arc = room.id == "202"
    local threats = {}
    local nearby = 0
    for i = 1, #enemies do
        local enemy = enemies[i]
        if enemy.distance <= COMBAT_RANGE then
            nearby = nearby + 1
            if enemy.visible
                and (wide_arc
                    or abs(angle_delta(enemy.bearing, player.angle)) <= COMBAT_ARC) then
                threats[#threats + 1] = enemy
            end
        end
    end
    local nearest = threats[1]
    if combat.active and combat.target then
        local exists = false
        for i = 1, #enemies do
            if enemies[i].slot == combat.target then
                exists = true
            end
        end
        if not exists then
            local health = combat.last_health
            combat_end(bot, combat, string.format(
                "target down health=%s", tostring(health)), false)
            if room.id == "202" then
                -- The dining room has a second corridor zombie; keep the
                -- policy armed so it cannot hit the route while it is
                -- disarmed and crossing the south approach.
                combat.disarmed = false
            end
            return false
        end
    end
    if not nearest then
        combat_end(bot, combat, "no visible threat in front", false)
        return false
    end

    -- Keep the current target while it lives and stays in range and sight.
    local target = nil
    if combat.active and combat.target then
        for i = 1, #enemies do
            if enemies[i].slot == combat.target then
                target = enemies[i]
            end
        end
        if not target
            or target.distance > COMBAT_RANGE
            or not target.visible
            or (not wide_arc
                and abs(angle_delta(target.bearing, player.angle)) > COMBAT_ARC) then
            local health = combat.last_health
            combat_end(bot, combat, string.format(
                "target lost health=%s", tostring(health)), false)
            if room.id == "202" then
                combat.disarmed = false
            end
            return false
        end
    end

    if player.health <= COMBAT_MIN_HEALTH then
        combat_end(bot, combat, "low health", true)
        return false
    end
    if nearby > COMBAT_MAX_THREATS then
        combat_end(bot, combat, "too many enemies nearby", true)
        return false
    end
    if not is_common_zombie(nearest) then
        combat_end(bot, combat, string.format(
            "boss or hard enemy id=%#04x", nearest.id), true)
        return false
    end
    if not bot:has_item(ITEM_HANDGUN) then
        combat_end(bot, combat, "no handgun", true)
        return false
    end

    if not combat.active then
        local weapon_now = bot:weapon()
        local needed = (nearest.health + 11) // 12
        local finishing = nearest.health <= 24 and weapon_now.clip >= needed
        if weapon_now.reserve <= 0 and weapon_now.clip < COMBAT_MIN_CLIP and not finishing then
            -- No spare ammunition for a fresh zombie: skip it entirely rather
            -- than starting a fight that will only anger it.
            combat.disarmed = true
            return false
        end
    end

    if not combat.active then
        combat_start(bot, combat, nearest, player, bot:weapon())
        target = nearest
    end
    target = target or nearest
    combat.ticks = combat.ticks + 1
    if combat.ticks > COMBAT_MAX_TICKS then
        combat_end(bot, combat, "timeout", true)
        return false
    end

    if combat.last_health ~= nil and target.health < combat.last_health then
        log(bot, string.format(
            "combat hit slot=%d health=%d->%d clip=%d reserve=%d",
            target.slot, combat.last_health, target.health,
            bot:weapon().clip, bot:weapon().reserve
        ))
    end
    combat.last_health = target.health

    local weapon = bot:weapon()
    if weapon.equipped ~= ITEM_HANDGUN or weapon.weapon ~= ITEM_HANDGUN then
        if not bot:equip(ITEM_HANDGUN) then
            combat_end(bot, combat, "cannot equip the handgun", true)
        end
        return true
    end

    if weapon.clip <= 0 then
        if weapon.reserve <= 0 then
            combat_end(bot, combat, "out of ammunition", true)
            return false
        end
        if not combat.reload_started then
            -- The Classic REbirth tactical reload: AIM + RUN with a partly
            -- filled clip and matching ammunition. If the build does not run
            -- it, stand down rather than standing in front of the zombie.
            combat.reload_ticks = combat.reload_ticks + 1
            bot:set_input({ aim = true, run = true })
            if weapon.behavior == 0x18 then
                combat.reload_started = true
                combat.reload_ticks = 0
                combat.aim_ticks = 0
                log(bot, string.format(
                    "combat reload start clip=%d reserve=%d",
                    weapon.clip, weapon.reserve))
            elseif combat.reload_ticks > COMBAT_RELOAD_WAIT then
                combat_end(bot, combat, "tactical reload unavailable", true)
                return false
            end
            return true
        end
        bot:set_input({ aim = true })
        combat.reload_ticks = combat.reload_ticks + 1
        if combat.reload_ticks > COMBAT_RELOAD_WAIT * 4 then
            combat_end(bot, combat, "reload did not finish", true)
            return false
        end
        return true
    end
    combat.reload_started = false
    combat.reload_ticks = 0

    if weapon.reserve <= 0 then
        -- A nearly dead target is worth finishing even with a low clip; a
        -- fresh one is not, because a reaction window can eat a miss or two.
        local needed = (target.health + 11) // 12
        local finishing = target.health <= 24 and weapon.clip >= needed
        if weapon.clip < COMBAT_MIN_CLIP and not finishing then
            combat_end(bot, combat, "low clip without reserve", true)
            return false
        end
    end

    -- Back out of the handgun's near blind spot, then resume the fight.
    if combat.retreating then
        if target.distance >= COMBAT_RETREAT_DONE then
            combat.retreating = false
            combat.retreat_ticks = 0
            log(bot, string.format(
                "combat retreat done dist=%d", target.distance))
        else
            combat.retreat_ticks = combat.retreat_ticks + 1
            if combat.retreat_ticks > COMBAT_RETREAT_WAIT then
                combat_end(bot, combat, "cannot open distance", true)
                return false
            end
            bot:face_to(target.x, target.z)
            bot:set_input({ down = true })
            return true
        end
    elseif target.distance < COMBAT_RETREAT_START then
        combat.retreating = true
        combat.retreat_ticks = 0
        combat.aim_ticks = 0
        log(bot, string.format(
            "combat retreat start dist=%d", target.distance))
        bot:face_to(target.x, target.z)
        bot:set_input({ down = true })
        return true
    end

    -- Face the target before raising the gun; once aligned, hold aim until
    -- the reticle locks, then fire one pulse per cadence window.
    local face = bot:face_to(target.x, target.z)
    if abs(face.angle_error) > 0x180 then
        combat.aim_ticks = 0
        return true
    end
    bot:set_input(combat_aim_input(target))
    combat.aim_ticks = combat.aim_ticks + 1

    local locked = weapon.target ~= nil or (weapon.aim_state % 4) == 3
    if not locked then
        if combat.aim_ticks > COMBAT_LOCK_WAIT then
            combat_end(bot, combat, "no weapon lock", true)
            return false
        end
        return true
    end
    -- A hit puts the monster into its reaction; more bullets during that
    -- window are discarded by the damage pipeline, so wait for it to clear.
    if target.hit_state ~= 0 then
        return true
    end
    if combat.cooldown > 0 then
        combat.cooldown = combat.cooldown - 1
        return true
    end
    local shot = combat_aim_input(target)
    shot.fire = true
    shot.fire_pressed = true
    bot:set_input(shot)
    combat.cooldown = COMBAT_FIRE_CADENCE
    combat.shots = combat.shots + 1
    log(bot, string.format(
        "combat shot=%d slot=%d health=%d dist=%d clip=%d reserve=%d "
            .. "state=%d status=%#04x action_behavior=%d action_state=%d hit_state=%d",
        combat.shots, target.slot, target.health, target.distance,
        weapon.clip, weapon.reserve, target.state, target.status_flags,
        target.action_behavior, target.action_state, target.hit_state
    ))
    return true
end

function on_bot_tick(bot)
    local tick = bot:tick()
    local room = bot:room()
    local scene = bot:scene()
    local player = bot:player()

    if BOT_STATE.last_health and player.health < BOT_STATE.last_health then
        local nearest = nil
        local enemies = bot:enemies()
        for i = 1, #enemies do
            if not nearest or enemies[i].distance < nearest.distance then
                nearest = enemies[i]
            end
        end
        log(bot, string.format(
            "damage health %d -> %d pos=(%d,%d) attacked=%d state=%d "
                .. "collision=%d grabbed=%s nearest=%s",
            BOT_STATE.last_health, player.health, player.x, player.z,
            player.being_attacked, player.state, player.collision_flags,
            tostring(player.grabbed),
            nearest and string.format("slot=%d id=%#04x dist=%d pos=(%d,%d)",
                nearest.slot, nearest.id, nearest.distance,
                nearest.x, nearest.z) or "none"))
    end
    BOT_STATE.last_health = player.health

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

    if step_healing(bot, player) then
        return
    end

    if combat_step(bot, scene, player) then
        return
    end

    local phase = BOT_STATE.phase
    if phase == "clear_202" then
        phase_clear_202(bot)
        return
    end
    if phase == "to_202_from_203" then
        phase_to_202_from_203(bot)
        return
    end
    local travel = TRAVEL_PHASES[phase]
    if travel then
        phase_travel(bot, travel)
        return
    end
    if phase == "to_105_items" then
        phase_to_105_items(bot)
    elseif phase == "to_10c_item" then
        phase_to_10c_item(bot)
    elseif phase == "retreat_10c" then
        phase_retreat_10c(bot)
    elseif phase == "report_10c_retreat" then
        phase_report_10c_retreat(bot)
    elseif phase == "to_205_item" then
        phase_to_205_item(bot)
    elseif phase == "to_210_item" then
        phase_to_210_item(bot)
    elseif phase == "place_crests_11a" then
        phase_place_crests_11a(bot)
    elseif phase == "opening" then
        phase_opening(bot)
    elseif phase == "goto_105_from_106" then
        phase_goto_105_from_106(bot)
    elseif phase == "to_104" then
        phase_to_104(bot)
    elseif phase == "walk_104" then
        phase_walk_104(bot)
    elseif phase == "to_104_ammo" then
        phase_to_104_ammo(bot)
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
