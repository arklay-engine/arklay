-- Combat for the player bot, driven by the game's knowledge file
-- (knowledge/<game>.xml, read as bot:knowledge()). Loads after constants.lua,
-- nav.lua and survival.lua (chunks run in path order).
--
-- Each tick that healing and saving leave free, BotTactics.step:
--   1. looks at the live monsters near the bot and their knowledge entries;
--   2. decides each engagement (BotTactics.decide): fight, or run past it;
--   3. for a fight, keeps the knowledge's distance band for the weapon
--      (BotTactics.spacing) by walking to a point the engine's spacing search
--      picks (bot:spacing_point) along an enemy-avoiding path, faces the
--      target, holds aim in the pose that matches the enemy's live status
--      bit (BotTactics.pose), and fires from the gun hold;
--   4. for a run, the planner keeps its goal and walks with avoid_enemies
--      (BotTactics.avoiding), so its path passes behind the monster. While
--      the bot sources ammunition (BotSurvival.sourcing) every walk avoids
--      the monsters and a monster only in a room crossed again is run past.
-- The fight-or-run rule, the pose rule and the spacing rule are pure
-- functions with unit tests (src/engine/bot.rs). No enemy id, weapon id or
-- coordinate is written here; every per-enemy fact is in the knowledge file.

BotTactics = BotTactics or {}

local C = BotConst
local abs = BotNav.abs

-- A fight that has not ended in this long stands down.
local MAX_TICKS = 25 * C.SECOND
-- The gun hold must show a lock (or the aim settle) within this long.
local LOCK_WAIT = 3 * C.SECOND
-- Ticks the aim is held before a shot without a lock: the raise.
local AIM_SETTLE = C.SECOND // 2
-- A reload that has not started or finished by now stands down.
local RELOAD_WAIT = 3 * C.SECOND
-- One reposition walk, then hold for SPACE_REST before moving again.
local SPACE_TICKS = C.SECOND + C.SECOND // 2
local SPACE_REST = 2 * C.SECOND
-- A walk this close to the spacing point is there.
local SPACE_TOLERANCE = C.APPROACH
-- The facing error accepted before raising the aim: 1/32 turn.
local FACE_BAND = C.FULL_TURN // 32
-- After a target is lost, walk on this long before engaging again.
local LOST_CALM = 6 * C.SECOND
-- Enemies are considered inside this distance when knowledge gives no band.
local NEAR = 8 * C.REACH
-- After a run decision the planner's walks avoid the monsters this long.
local RUN_HOLD = 2 * C.SECOND

local function state()
    BOT_STATE.tactics = BOT_STATE.tactics or {}
    return BOT_STATE.tactics
end

local function risk(bot)
    return bot:risk() or 0.25
end

local function angle_delta(a, b)
    local delta = (a - b) % C.FULL_TURN
    if delta > C.FULL_TURN // 2 then
        delta = delta - C.FULL_TURN
    end
    return delta
end

local POSE_BITS = { torso = C.STATUS_TORSO, down = C.STATUS_LOW, up = C.STATUS_HIGH }

-- The aim pose for a shot, or nil when no pose can hit the enemy now.
--   entry: the enemy's knowledge row (or nil); weapon: the weapon's row (or
--   nil); status: the enemy's live status_flags; distance: horizontal.
-- The engine hits only when the enemy's status shares a bit with the aim
-- (torso 0x40, down 0x20, up 0x80). The order: a shotgun headshot close in
-- (up, then torso: both pop the head); a downed enemy (only the low bit) is
-- shot down at the legs; otherwise the entry's standing pose, then torso,
-- down and up. Up is tried only with a weapon the entry allows it for.
-- Returns the pose and why.
function BotTactics.pose(entry, weapon, status, distance)
    local up_ok = entry ~= nil and weapon ~= nil
        and (entry.up == "yes" or entry.up == weapon.name)
    local order, seen = {}, {}
    local function add(pose)
        if pose and not seen[pose] then
            seen[pose] = true
            order[#order + 1] = pose
        end
    end
    local why = "standing"
    local downed = status & C.STATUS_TORSO == 0 and status & C.STATUS_LOW ~= 0
        and status & C.STATUS_HIGH == 0
    if weapon and weapon.headshot and distance <= weapon.headshot and up_ok then
        why = "headshot range"
        add("up")
        add("torso")
    elseif downed then
        why = "downed"
        add(entry and entry.downed or "down")
    end
    add(entry and entry.stand)
    add("torso")
    add("down")
    add("up")
    for _, pose in ipairs(order) do
        if (pose ~= "up" or up_ok) and status & POSE_BITS[pose] ~= 0 then
            return pose, why
        end
    end
    return nil, "no aim bit matches"
end

-- The pad input for an aim pose.
function BotTactics.aim_input(pose)
    local input = { aim = true }
    if pose == "down" then
        input.down = true
    elseif pose == "up" then
        input.up = true
    end
    return input
end

-- Fight or run. `o`:
--   policy     the knowledge policy ("fight", "pass", "avoid") or nil
--   gun        a firearm with rounds is held
--   rounds     clip + reserve of that firearm
--   needed     rounds to bring the target down (its live health / damage)
--   crowd_needed  the same summed over every fightable monster near
--   health, max_health, risk
--   crossing   this room still has frontier work (the bot comes back)
--   distance, reach  the target's distance and attack reach (nil unknown)
--   outrun     Jill's run beats its chase (nil unknown)
--   sourcing   the bot is under its ammo floor with no ammunition it can
--              reach (BotSurvival.sourcing): the rounds left are for what
--              must be fought, so a monster only in a room crossed again
--              is run past
-- Returns "fight" or "run" and why. The rule: never fight what the file
-- says to pass or avoid, or knows nothing about, or without a firearm;
-- fight what is closing in (inside twice its reach) with whatever rounds
-- are left, even at low health when it cannot be outrun (or its speed is
-- unknown); below a risk-scaled health floor run; otherwise only with the rounds
-- for the target (a margin at low risk) and for the crowd, and then what
-- cannot be outrun and anything in a room the bot must cross again;
-- otherwise it is only in the way: run past it.
function BotTactics.decide(o)
    if not o.policy then
        return "run", "not in the knowledge file"
    end
    if o.policy ~= "fight" then
        return "run", "policy " .. o.policy
    end
    if not o.gun then
        return "run", "no firearm with rounds"
    end
    -- Inside twice its reach it gets the bot anyway: shoot what is left.
    -- At low health only what the bot cannot outrun (or may not).
    local closing = o.reach and o.distance <= 2 * o.reach and o.rounds > 0
    if closing and o.outrun ~= true then
        return "fight", "closing in, cannot outrun it"
    end
    if o.health <= o.max_health * (0.45 - 0.3 * o.risk) then
        return "run", "low health"
    end
    if closing then
        return "fight", "closing in"
    end
    -- A cautious bot wants a margin of rounds: half again at risk 0.
    local margin = 1 + 0.5 * (1 - o.risk)
    if o.rounds < o.needed * margin then
        return "run", "too few rounds"
    end
    if o.crowd_needed and o.rounds < o.crowd_needed then
        return "run", "too few rounds for the crowd"
    end
    if o.outrun == false then
        return "fight", "cannot outrun it"
    end
    if o.crossing and o.sourcing then
        return "run", "sourcing ammunition: only what must be fought"
    end
    if o.crossing then
        return "fight", "the room is crossed again"
    end
    return "run", "only in the way"
end

-- Where to stand. `o`: distance, min, max (the knowledge band), reloading,
-- headshot (the weapon's headshot distance or nil), crawl_reach (a downed
-- enemy's reach or nil), reach and speed (the enemy's attack reach and
-- chase speed, nil when unknown). Returns "back", "close" or "hold", and the
-- distance to walk to. Reloading backs off to the band's far edge; a target
-- inside the band's near edge (or a crawler's reach) backs off to the
-- middle; a shotgun target just past the band steps in to the middle, where
-- a torso or up shot pops the head; otherwise hold and shoot. A back-off
-- starts with turning around (tank controls), so it is taken only when the
-- enemy cannot close the gap to its reach in that time; otherwise the bot
-- holds and shoots, since a hit makes it flinch.
function BotTactics.spacing(o)
    local turn_ticks = C.FULL_TURN // 2 // C.TURN_STEP
    local room_to_turn = not o.speed
        or o.distance - (o.reach or 0) > o.speed * turn_ticks
    -- Inside its reach a back-off only shows it the bot's back: shoot.
    if o.reach and o.distance < o.reach then
        room_to_turn = false
    end
    local min = o.min
    if o.crawl_reach and o.crawl_reach + C.APPROACH > min then
        min = o.crawl_reach + C.APPROACH
    end
    local middle = (min + o.max) // 2
    if o.max > min and middle < min then
        middle = min
    end
    if o.reloading then
        if o.distance < o.max and room_to_turn then
            return "back", o.max
        end
        return "hold", o.distance
    end
    if o.distance < min then
        if room_to_turn then
            return "back", middle
        end
        return "hold", o.distance
    end
    -- Close in only from outside the headshot distance: inside it the shot
    -- already pops the head, and a walk there only delays it.
    if o.headshot and o.distance > o.max and o.distance >= o.headshot
        and o.distance <= o.max + (o.max - min) then
        return "close", middle
    end
    return "hold", o.distance
end

local function menu_weapons(bot)
    local rows = {}
    for _, row in ipairs(bot:menu().items or {}) do
        if row.kind == "weapon" and row.quantity > 0 then
            rows[#rows + 1] = row
        end
    end
    return rows
end

-- The firearm to fight with and its rounds: the equipped one if it has
-- rounds, else the held weapon stack (with knowledge) with the most.
local function firearm(bot, k)
    local weapon = bot:weapon()
    if weapon.equipped and k.weapons[weapon.equipped]
        and weapon.clip + weapon.reserve > 0 then
        return weapon.equipped, weapon.clip + weapon.reserve
    end
    local best = nil
    for _, row in ipairs(menu_weapons(bot)) do
        if k.weapons[row.item] and (not best or row.quantity > best.quantity) then
            best = row
        end
    end
    if not best then
        return nil, 0
    end
    return best.item, best.quantity
end

local function rounds_for(enemy, entry, gun)
    local band = entry and entry.keep and entry.keep[gun]
    local damage = band and band.damage
    if not damage or damage <= 0 then
        return enemy.health
    end
    return (enemy.health + damage - 1) // damage
end

local function crossing(bot)
    local here = bot:room().id
    local count = 0
    for _, row in ipairs(bot:world().frontier or {}) do
        if row.room == here then
            count = count + 1
        end
    end
    -- The door the bot leaves by is one of them.
    return count > 1
end

local function band_of(entry, gun)
    local band = entry and entry.keep and gun and entry.keep[gun]
    if band then
        return band.min, band.max
    end
    return 2 * C.REACH, 6 * C.REACH
end

local function reset(s)
    s.active = false
    s.target = nil
    s.ticks = 0
    s.aim_ticks = 0
    s.reload_ticks = 0
    s.reload_started = false
    s.last_health = nil
    s.shots = 0
    s.unseen = 0
    s.space = nil
    s.space_ticks = 0
    s.space_rest = 0
    s.fired_tick = nil
end

local function stop(bot, s, reason, calm)
    if s.active then
        local player = bot:player()
        local weapon = bot:weapon()
        bot:log(string.format(
            "combat end: %s slot=%s shots=%d ticks=%d health=%d clip=%d reserve=%d",
            reason, tostring(s.target), s.shots, s.ticks, player.health,
            weapon.clip, weapon.reserve))
    end
    reset(s)
    if calm then
        s.calm_until = bot:tick() + calm
    end
end

local function enemy_by_slot(enemies, slot)
    for i = 1, #enemies do
        if enemies[i].slot == slot then
            return enemies[i]
        end
    end
    return nil
end

-- Whether the planner's walks should plan around the monsters: a run
-- decision is in force, or the bot is sourcing ammunition.
function BotTactics.avoiding()
    local s = state()
    return s.sourcing == true or (s.avoid_until or 0) > (s.now or 0)
end

function BotTactics.active()
    return state().active == true
end

-- Decide every monster near the bot; returns the fight target or nil.
local function choose(bot, s, k, enemies, player)
    local gun, rounds = firearm(bot, k)
    local near, fightable = {}, {}
    for i = 1, #enemies do
        local enemy = enemies[i]
        local entry = k.enemies[enemy.id]
        local _, far = band_of(entry, gun)
        if enemy.distance <= (far > NEAR and far or NEAR) then
            near[#near + 1] = enemy
            if entry and entry.policy == "fight" then
                fightable[#fightable + 1] = enemy
            end
        end
    end
    if #near == 0 then
        return nil
    end
    local crowd_needed = 0
    for _, enemy in ipairs(fightable) do
        crowd_needed = crowd_needed + rounds_for(enemy, k.enemies[enemy.id], gun)
    end
    local cross = nil
    local arc = C.FULL_TURN // 4
    local best = nil
    for _, enemy in ipairs(near) do
        local entry = k.enemies[enemy.id]
        do
            if cross == nil then
                if not s.cross_tick or bot:tick() - s.cross_tick >= C.SECOND then
                    s.cross = crossing(bot)
                    s.cross_tick = bot:tick()
                end
                cross = s.cross
            end
            local verdict, why = BotTactics.decide({
                policy = entry and entry.policy,
                gun = gun ~= nil and rounds > 0,
                rounds = rounds,
                needed = rounds_for(enemy, entry, gun),
                crowd_needed = #fightable > 1 and crowd_needed or nil,
                health = player.health,
                max_health = player.max_health,
                risk = risk(bot),
                crossing = cross,
                sourcing = s.sourcing,
                distance = enemy.distance,
                reach = entry and entry.reach,
                outrun = entry and entry.outrun,
            })
            if verdict == "run" then
                s.avoid_until = bot:tick() + RUN_HOLD
                s.run_logs = s.run_logs or {}
                if s.run_logs[enemy.slot] ~= why then
                    s.run_logs[enemy.slot] = why
                    bot:log(string.format("run past slot=%d id=%#04x dist=%d: %s",
                        enemy.slot, enemy.id, enemy.distance, why))
                end
            elseif enemy.visible
                and (abs(angle_delta(enemy.bearing, player.angle)) <= arc
                    or enemy.distance <= 2 * (entry.reach or C.REACH)) then
                if not best or enemy.distance < best.distance then
                    best = enemy
                    best.why = why
                end
            end
        end
    end
    return best, gun
end

function BotTactics.step(bot, scene, player)
    local s = state()
    s.now = bot:tick()
    if not s.sourcing_tick or s.now - s.sourcing_tick >= C.SECOND then
        s.sourcing_tick = s.now
        local was = s.sourcing
        s.sourcing = BotSurvival.sourcing_now(bot)
        if s.sourcing ~= (was or false) then
            bot:log(s.sourcing and "sourcing ammunition: explore, avoid monsters, fight only what must be fought"
                or "sourcing ammunition: over")
        end
    end
    if not scene.controllable or player.grabbed then
        return false
    end
    local room = bot:room().id
    if s.room ~= room then
        stop(bot, s, "left the room")
        s.room = room
        s.cross_tick = nil
        s.run_logs = {}
        s.avoid_until = nil
    end
    if bot:tick() < (s.calm_until or 0) and not s.active then
        return false
    end
    local k = bot:knowledge()
    local enemies = bot:enemies()
    local target = s.active and enemy_by_slot(enemies, s.target) or nil
    if s.active and not target then
        stop(bot, s, string.format("target down health=%s", tostring(s.last_health)))
        return false
    end
    if not s.active then
        local gun
        target, gun = choose(bot, s, k, enemies, player)
        if not target then
            return false
        end
        reset(s)
        s.active = true
        s.gun = gun
        s.target = target.slot
        s.last_health = target.health
        s.avoid_until = nil
        bot:log(string.format(
            "combat start slot=%d id=%#04x dist=%d health=%d player_health=%d gun=0x%02X: %s",
            target.slot, target.id, target.distance, target.health, player.health, gun,
            target.why or "fight"))
    end
    local entry = k.enemies[target.id]
    local wrow = k.weapons[s.gun]
    s.ticks = s.ticks + 1
    if s.ticks > MAX_TICKS then
        stop(bot, s, "timeout", LOST_CALM)
        return false
    end
    if player.health <= player.max_health * (0.45 - 0.3 * risk(bot)) then
        stop(bot, s, "low health", LOST_CALM)
        return false
    end
    if not target.visible then
        s.unseen = s.unseen + 1
        if s.unseen > C.SECOND then
            stop(bot, s, "target lost", LOST_CALM)
            return false
        end
    else
        s.unseen = 0
    end
    if s.last_health and target.health < s.last_health then
        bot:log(string.format("combat hit slot=%d health=%d->%d",
            target.slot, s.last_health, target.health))
    end
    s.last_health = target.health

    local weapon = bot:weapon()
    if weapon.equipped ~= s.gun then
        if not BotMenu.equip(bot, s.gun, "combat equip") then
            stop(bot, s, "cannot equip the firearm", LOST_CALM)
        end
        return true
    end
    if weapon.clip <= 0 and weapon.reserve <= 0 then
        stop(bot, s, "out of ammunition", LOST_CALM)
        return false
    end
    -- The cadence the engine shows: ticks from a fire press to the hold.
    if s.fired_tick and weapon.behavior == C.HOLD_BEHAVIOR and s.fired_left then
        bot:log(string.format("cadence %d ticks", bot:tick() - s.fired_tick))
        s.fired_tick = nil
    end
    if s.fired_tick and weapon.behavior ~= C.HOLD_BEHAVIOR then
        s.fired_left = true
    end

    -- Spacing first: a walk drops the aim.
    local min, max = band_of(entry, s.gun)
    local reloading = weapon.clip <= 0
    local move, want = BotTactics.spacing({
        distance = target.distance,
        min = min,
        max = max,
        reloading = reloading,
        headshot = wrow and wrow.headshot,
        crawl_reach = target.status_flags & C.STATUS_TORSO == 0 and entry
            and entry.crawl_reach or nil,
        reach = entry and entry.reach,
        speed = entry and entry.speed,
    })
    if s.space_rest > 0 then
        s.space_rest = s.space_rest - 1
        move = "hold"
    end
    if move ~= "hold" and not s.reload_started then
        if not s.space or s.space.move ~= move then
            local point = bot:spacing_point(target.slot, want)
            s.space = point and { move = move, x = point.x, z = point.z } or nil
            s.space_ticks = 0
            if point then
                bot:log(string.format("combat %s to %d from slot=%d (dist=%d) at (%d,%d)",
                    move, want, target.slot, target.distance, point.x, point.z))
            end
        end
        if s.space then
            s.space_ticks = s.space_ticks + 1
            local result = bot:walk_to(s.space.x, s.space.z, {
                run = bot:distance(s.space.x, s.space.z) >= C.RUN_MIN_DISTANCE,
                tolerance = SPACE_TOLERANCE,
                avoid_enemies = true,
            })
            if result.status == "moving" and s.space_ticks <= SPACE_TICKS then
                s.aim_ticks = 0
                return true
            end
        end
        s.space = nil
        s.space_rest = SPACE_REST
    else
        s.space = nil
    end

    -- Face the target before raising the aim.
    if s.aim_ticks == 0 then
        local face = bot:face_to(target.x, target.z)
        if abs(face.angle_error) > FACE_BAND then
            return true
        end
    end

    if reloading then
        -- An empty clip with ammunition held: aim and fire reloads.
        s.reload_ticks = s.reload_ticks + 1
        if weapon.behavior == C.RELOAD_BEHAVIOR then
            s.reload_started = true
        end
        local input = { aim = true }
        if not s.reload_started and weapon.behavior == C.HOLD_BEHAVIOR
            and s.reload_ticks % 4 == 0 then
            input.fire = true
            input.fire_pressed = true
        end
        bot:set_input(input)
        s.aim_ticks = s.aim_ticks + 1
        if s.reload_ticks > RELOAD_WAIT * (s.reload_started and 2 or 1) then
            stop(bot, s, "reload did not finish", LOST_CALM)
            return false
        end
        return true
    end
    if s.reload_started then
        bot:log(string.format("reloaded in %d ticks clip=%d", s.reload_ticks, weapon.clip))
    end
    s.reload_started = false
    s.reload_ticks = 0

    local pose, why = BotTactics.pose(entry, wrow, target.status_flags, target.distance)
    if pose ~= s.pose then
        bot:log(string.format("aim %s at slot=%d (%s, status=0x%02X dist=%d)",
            tostring(pose), target.slot, why, target.status_flags, target.distance))
        s.pose = pose
    end
    local input = BotTactics.aim_input(pose or s.held_pose or "torso")
    if pose then
        s.held_pose = pose
    end
    s.aim_ticks = s.aim_ticks + 1
    -- Keep turning onto the target while the aim is up.
    local face = bot:face_to(target.x, target.z)
    local locked = weapon.target ~= nil
    if not pose or weapon.behavior ~= C.HOLD_BEHAVIOR or target.hit_state ~= 0
        or (not locked and s.aim_ticks < AIM_SETTLE)
        or abs(face.angle_error) > FACE_BAND * 2 then
        if s.aim_ticks > LOCK_WAIT * 3 then
            stop(bot, s, pose and "never ready to fire" or "no aim pose hits it", LOST_CALM)
            return false
        end
        bot:set_input(input)
        return true
    end
    input.fire = true
    input.fire_pressed = true
    bot:set_input(input)
    s.shots = s.shots + 1
    s.fired_tick = bot:tick()
    s.fired_left = false
    s.aim_ticks = AIM_SETTLE
    return true
end
