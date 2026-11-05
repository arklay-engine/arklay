-- Survival and combat for the player bot. Loads after constants.lua and
-- nav.lua (chunks run in path order), whose tables it captures.
--
-- BotSurvival.heal(bot, player): below a health threshold that scales with
-- bot:risk() (0.0 heals early, 1.0 heals late), use a held heal item through
-- the inventory menu. Never in a grab, a scene or a fight.
--
-- BotSurvival.critical(bot, player): health under a risk-scaled floor
-- (0.2 of max at risk 0, 0.05 at risk 1), no heal item held and damage taken
-- in the last 5 seconds. The rule for a critical tick (bot/init.lua):
--   1. save if possible: a typewriter in this room and a save the
--      BotSave decision allows (critical health skips its cooldown);
--   2. otherwise keep playing when a save exists: a death loads it;
--   3. only with no save at all does the run stop, with
--      "health critical with no heal item and no save while under attack".
--
-- BotSurvival.step(bot, scene, player): a conservative fight layer. It fights
-- only the common zombie classes with a held firearm, one target at a time,
-- and stands down for bosses, crowds, low health or an empty gun. Health
-- limits scale with bot:risk() too. It returns true when it owns the tick.
--
-- Weapons and heal items are chosen by their catalog kind from bot:menu(),
-- never by item id. No room literals.

BotSurvival = BotSurvival or {}

local C = BotConst
local abs = BotNav.abs

local MAX_TICKS = 14 * C.SECOND
local FIRE_CADENCE = 14
local LOCK_WAIT = 3 * C.SECOND
local RELOAD_WAIT = C.SECOND + C.SECOND // 2
local RETREAT_WAIT = 5 * C.SECOND
local HEAL_COOLDOWN = 2 * C.SECOND
-- One handgun round takes about 12 health off a zombie.
local DAMAGE_PER_ROUND = 12
local MIN_CLIP = 9
-- After losing a target, walk on for this long before engaging again.
local LOST_CALM = 10 * C.SECOND

local function state()
    BOT_STATE.combat = BOT_STATE.combat or {}
    return BOT_STATE.combat
end

local function risk(bot)
    return bot:risk() or 0.25
end

-- Heal below this fraction of max health: 0.6 at risk 0, 0.25 at risk 1.
local function heal_fraction(bot)
    return 0.6 - 0.35 * risk(bot)
end

-- Do not start a fight below this fraction: 0.45 at risk 0, 0.15 at risk 1.
local function fight_fraction(bot)
    return 0.45 - 0.3 * risk(bot)
end

local function menu_items(bot, kind)
    local rows = {}
    for _, row in ipairs(bot:menu().items or {}) do
        if row.kind == kind and row.quantity > 0 then
            rows[#rows + 1] = row
        end
    end
    return rows
end

function BotSurvival.heal(bot, player)
    local s = state()
    s.heal_cooldown = (s.heal_cooldown or 0) - 1
    if player.grabbed or s.active or s.heal_cooldown > 0 then
        return false
    end
    if player.health >= player.max_health * heal_fraction(bot) then
        return false
    end
    -- The largest stack first keeps the rarer single heals for later.
    local heals = menu_items(bot, "heal")
    table.sort(heals, function(a, b)
        if a.quantity ~= b.quantity then
            return a.quantity > b.quantity
        end
        return a.slot < b.slot
    end)
    local row = heals[1]
    s.heal_cooldown = HEAL_COOLDOWN
    if not row then
        return false
    end
    if not BotMenu.use_heal(bot, row.item, "heal") then
        return false
    end
    s.heals = (s.heals or 0) + 1
    bot:log(string.format("heal item=0x%02X health=%d/%d risk=%.2f uses=%d",
        row.item, player.health, player.max_health, risk(bot), s.heals))
    return true
end

-- Whether the run should stop rather than walk on: health is under a
-- risk-scaled floor, no heal item is held and the player was hurt within the
-- last few seconds. A bolder bot plays on lower.
function BotSurvival.critical(bot, player)
    local s = state()
    if s.player_health and player.health < s.player_health then
        s.hurt_tick = bot:tick()
    end
    s.player_health = player.health
    if player.health >= player.max_health * (0.2 - 0.15 * risk(bot)) then
        return false
    end
    if #menu_items(bot, "heal") > 0 then
        return false
    end
    return s.hurt_tick ~= nil and bot:tick() - s.hurt_tick <= 5 * C.SECOND
end

local function angle_delta(a, b)
    local delta = (a - b) % C.FULL_TURN
    if delta > C.FULL_TURN // 2 then
        delta = delta - C.FULL_TURN
    end
    return delta
end

-- The aim direction must match the monster's vertical status bit.
local function aim_input(target)
    local input = { aim = true }
    if target.status_flags & C.STATUS_LOW ~= 0 then
        input.down = true
    elseif target.status_flags & C.STATUS_HIGH ~= 0 then
        input.up = true
    end
    return input
end

-- The firearm to fight with: the equipped one if it still has rounds or
-- reserve, else the held weapon stack with the most rounds.
local function firearm(bot)
    local weapon = bot:weapon()
    if weapon.equipped and (weapon.clip > 0 or weapon.reserve > 0) then
        return weapon.equipped
    end
    local best = nil
    for _, row in ipairs(menu_items(bot, "weapon")) do
        if not best or row.quantity > best.quantity then
            best = row
        end
    end
    return best and best.item or nil
end

local function reset(s)
    s.active = false
    s.target = nil
    s.ticks = 0
    s.cooldown = 0
    s.aim_ticks = 0
    s.reload_ticks = 0
    s.reload_started = false
    s.last_health = nil
    s.retreating = false
    s.retreat_ticks = 0
    s.shots = 0
    s.unseen = 0
end

local function stop(bot, s, reason, disarm)
    if s.active then
        local player = bot:player()
        local weapon = bot:weapon()
        bot:log(string.format(
            "combat end: %s slot=%s shots=%d ticks=%d health=%d clip=%d reserve=%d",
            reason, tostring(s.target), s.shots, s.ticks, player.health,
            weapon.clip, weapon.reserve))
    end
    reset(s)
    if disarm then
        s.disarmed = true
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

function BotSurvival.step(bot, scene, player)
    if not scene.controllable or player.grabbed then
        return false
    end
    local s = state()
    local room = bot:room().id
    if s.room ~= room then
        reset(s)
        s.room = room
        s.disarmed = false
    end
    if s.disarmed or bot:tick() < (s.calm_until or 0) then
        return false
    end
    local enemies = bot:enemies()
    local arc = C.FULL_TURN // 4
    local threats, nearby = {}, 0
    for i = 1, #enemies do
        local enemy = enemies[i]
        if enemy.distance <= C.COMBAT_RANGE then
            nearby = nearby + 1
            if enemy.visible and abs(angle_delta(enemy.bearing, player.angle)) <= arc then
                threats[#threats + 1] = enemy
            end
        end
    end
    local target = s.active and enemy_by_slot(enemies, s.target) or nil
    if s.active and not target then
        -- Killed: stay armed for the room's next zombie.
        stop(bot, s, string.format("target down health=%s", tostring(s.last_health)), false)
        return false
    end
    if target and (target.distance > C.COMBAT_RANGE or not target.visible) then
        -- Sight flickers while turning; give the target a moment.
        s.unseen = (s.unseen or 0) + 1
        if s.unseen > C.SECOND then
            stop(bot, s, "target lost", false)
            s.calm_until = bot:tick() + LOST_CALM
            return false
        end
    elseif target then
        s.unseen = 0
    end
    target = target or threats[1]
    if not target then
        return false
    end
    if not s.active then
        if player.health <= player.max_health * fight_fraction(bot) then
            stop(bot, s, "low health", true)
            return false
        end
        if nearby > 1 then
            stop(bot, s, "more than one enemy nearby", true)
            return false
        end
        if not C.COMMON_ZOMBIES[target.id] then
            stop(bot, s, string.format("not a common zombie id=%#04x", target.id), true)
            return false
        end
        local gun = firearm(bot)
        if not gun then
            stop(bot, s, "no firearm with rounds", true)
            return false
        end
        local weapon = bot:weapon()
        local needed = (target.health + DAMAGE_PER_ROUND - 1) // DAMAGE_PER_ROUND
        if weapon.equipped == gun and weapon.reserve <= 0 and weapon.clip < MIN_CLIP
            and weapon.clip < needed then
            stop(bot, s, "too few rounds for a fresh zombie", true)
            return false
        end
        reset(s)
        s.active = true
        s.gun = gun
        s.target = target.slot
        s.last_health = target.health
        bot:log(string.format(
            "combat start slot=%d id=%#04x dist=%d health=%d player_health=%d gun=0x%02X",
            target.slot, target.id, target.distance, target.health, player.health, gun))
    end
    s.ticks = s.ticks + 1
    if s.ticks > MAX_TICKS then
        stop(bot, s, "timeout", true)
        return false
    end
    if s.last_health and target.health < s.last_health then
        bot:log(string.format("combat hit slot=%d health=%d->%d",
            target.slot, s.last_health, target.health))
    end
    s.last_health = target.health

    local weapon = bot:weapon()
    if weapon.equipped ~= s.gun then
        if not BotMenu.equip(bot, s.gun, "combat equip") then
            stop(bot, s, "cannot equip the firearm", true)
        end
        return true
    end
    if weapon.clip <= 0 then
        if weapon.reserve <= 0 then
            stop(bot, s, "out of ammunition", true)
            return false
        end
        s.reload_ticks = s.reload_ticks + 1
        if not s.reload_started then
            -- The tactical reload: aim + run with matching ammunition.
            bot:set_input({ aim = true, run = true })
            if weapon.behavior == C.RELOAD_BEHAVIOR then
                s.reload_started = true
                s.reload_ticks = 0
            elseif s.reload_ticks > RELOAD_WAIT then
                stop(bot, s, "tactical reload unavailable", true)
                return false
            end
            return true
        end
        bot:set_input({ aim = true })
        if s.reload_ticks > RELOAD_WAIT * 4 then
            stop(bot, s, "reload did not finish", true)
            return false
        end
        return true
    end
    s.reload_started = false
    s.reload_ticks = 0

    if s.retreating then
        if target.distance >= C.COMBAT_RETREAT_DONE then
            s.retreating = false
        else
            s.retreat_ticks = s.retreat_ticks + 1
            if s.retreat_ticks > RETREAT_WAIT then
                stop(bot, s, "cannot open distance", true)
                return false
            end
            bot:face_to(target.x, target.z)
            bot:set_input({ down = true })
            return true
        end
    elseif target.distance < C.COMBAT_RETREAT_START then
        s.retreating = true
        s.retreat_ticks = 0
        s.aim_ticks = 0
        bot:face_to(target.x, target.z)
        bot:set_input({ down = true })
        return true
    end

    local face = bot:face_to(target.x, target.z)
    if abs(face.angle_error) > C.FULL_TURN // 10 then
        s.aim_ticks = 0
        return true
    end
    bot:set_input(aim_input(target))
    s.aim_ticks = s.aim_ticks + 1
    local locked = weapon.target ~= nil or (weapon.aim_state % 4) == 3
    if not locked then
        if s.aim_ticks > LOCK_WAIT then
            stop(bot, s, "no weapon lock", true)
            return false
        end
        return true
    end
    -- A hit puts the monster in its reaction window, which discards more
    -- bullets; wait for it to clear.
    if target.hit_state ~= 0 then
        return true
    end
    if s.cooldown > 0 then
        s.cooldown = s.cooldown - 1
        return true
    end
    local shot = aim_input(target)
    shot.fire = true
    shot.fire_pressed = true
    bot:set_input(shot)
    s.cooldown = FIRE_CADENCE
    s.shots = s.shots + 1
    return true
end

-- The tick the player last lost health (BotSurvival.critical tracks it).
function BotSurvival.hurt_tick()
    return state().hurt_tick
end

function BotSurvival.active()
    return state().active == true
end
