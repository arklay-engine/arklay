-- Survival for the player bot: healing and the critical-health rule. Loads
-- after constants.lua (chunks run in path order), whose table it captures.
--
-- BotSurvival.heal(bot, player): below a health threshold that scales with
-- bot:risk() (0.0 heals early, 1.0 heals late), or up to 0.85 before a
-- room's <fight> step starts, use a held heal item through
-- the inventory menu. Never in a grab or a scene; a fight is paused for it.
--
-- BotSurvival.cure(bot, player): poisoned (the engine's health status,
-- bot:player().poisoned / seriously_poisoned), use a held item the
-- knowledge file's <cure> list says clears that poison, before any heal. A
-- cure that only cures goes at once; one that also heals (a herb mix) waits
-- until health is under the heal threshold, so its healing is not wasted.
-- With no cure held it logs that once per poisoning and the heal rule
-- carries on.
--
-- BotSurvival.critical(bot, player): health under a risk-scaled floor
-- (0.2 of max at risk 0, 0.05 at risk 1), no heal item held and damage taken
-- in the last 5 seconds. The rule for a critical tick (bot/init.lua):
--   1. save if possible: a typewriter in this room and a save the
--      BotSave decision allows (critical health skips its cooldown, but
--      the save-safety rule refuses a dead-end save while a better save
--      exists);
--   2. otherwise keep playing when a save exists: a death loads it;
--   3. only with no save at all does the run stop, with
--      "health critical with no heal item and no save while under attack".
--
-- The fight itself lives in bot/tactics.lua (BotTactics); a heal under the
-- threshold goes first even while it runs.
--
-- Heal items are chosen by their catalog kind from bot:menu(), never by
-- item id. No room literals.

BotSurvival = BotSurvival or {}

local C = BotConst

local HEAL_COOLDOWN = 2 * C.SECOND

local function state()
    BOT_STATE.combat = BOT_STATE.combat or {}
    return BOT_STATE.combat
end

local function risk(bot)
    return bot:risk() or 0.25
end

-- Heal below this fraction of max health: 0.6 at risk 0, 0.25 at risk 1.
-- While the room's next step is a <fight> (a boss: 513's Tyrant is a race
-- of heals) and no combat runs yet, the bot heals up to
-- FIGHT_HEAL_FRACTION first; the menu pauses the game, so the heal costs
-- no fight time. Once the fight runs the usual threshold holds, so a full
-- heal is not spent on a scratch.
local FIGHT_HEAL_FRACTION = 0.85
local function heal_fraction(bot)
    local fraction = 0.6 - 0.35 * risk(bot)
    local step = not BotTactics.active() and BotPuzzle.next_interact(bot)
    if step and step.kind == "fight" and fraction < FIGHT_HEAL_FRACTION then
        fraction = FIGHT_HEAL_FRACTION
    end
    return fraction
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

-- The cure to use now: the first <cure> (knowledge order) for a poison the
-- player has, held (`held(item)` true), and either cure-only or `low`.
function BotSurvival.pick_cure(cures, player, held, low)
    for _, cure in ipairs(cures or {}) do
        local has = (cure.poison == "poison" and player.poisoned)
            or (cure.poison == "serious" and player.seriously_poisoned)
        if has and held(cure.item) and (low or not cure.heals) then
            return cure
        end
    end
    return nil
end

function BotSurvival.cure(bot, player)
    local s = state()
    if not (player.poisoned or player.seriously_poisoned) then
        s.poison_logged = nil
        return false
    end
    s.cure_cooldown = (s.cure_cooldown or 0) - 1
    if player.grabbed or BotTactics.active() or s.cure_cooldown > 0 then
        return false
    end
    local low = player.health < player.max_health * heal_fraction(bot)
    local cure = BotSurvival.pick_cure(bot:knowledge().cures, player,
        function(item) return bot:item_count(item) > 0 end, low)
    if not cure then
        if not s.poison_logged then
            s.poison_logged = true
            bot:log(string.format("poisoned (poison=%s serious=%s) health=%d/%d: "
                .. "no cure item held", tostring(player.poisoned),
                tostring(player.seriously_poisoned), player.health, player.max_health))
        end
        return false
    end
    s.cure_cooldown = HEAL_COOLDOWN
    if not BotMenu.use_heal(bot, cure.item, "cure") then
        return false
    end
    s.cures = (s.cures or 0) + 1
    bot:log(string.format("cure item=0x%02X poison=%s health=%d/%d uses=%d",
        cure.item, cure.poison, player.health, player.max_health, s.cures))
    return true
end

-- The held heal stacks that restore health (the menu row's `heals`, from
-- the catalog): a blue herb, the serum or a red herb alone is no heal.
local function heal_rows(bot)
    local heals = {}
    for _, row in ipairs(menu_items(bot, "heal")) do
        if row.heals ~= false then
            heals[#heals + 1] = row
        end
    end
    return heals
end

function BotSurvival.heal(bot, player)
    local s = state()
    s.heal_cooldown = (s.heal_cooldown or 0) - 1
    -- A fight does not hold off a heal: the menu pauses the game, and a
    -- fight played on under the threshold ends at the low-health floor,
    -- one grab lower (two bites).
    if player.grabbed or s.heal_cooldown > 0 then
        return false
    end
    if player.health >= player.max_health * heal_fraction(bot) then
        return false
    end
    -- The largest stack first keeps the rarer single heals for later.
    local heals = heal_rows(bot)
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

-- The critical-health floor as a fraction of max health: 0.2 at risk 0,
-- 0.05 at risk 1.
function BotSurvival.critical_fraction(r)
    return 0.2 - 0.15 * r
end

-- The held heal stacks that restore health, counted.
function BotSurvival.heal_count(bot)
    return #heal_rows(bot)
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
    if player.health >= player.max_health * BotSurvival.critical_fraction(risk(bot)) then
        return false
    end
    if #heal_rows(bot) > 0 then
        return false
    end
    return s.hurt_tick ~= nil and bot:tick() - s.hurt_tick <= 5 * C.SECOND
end

-- The tick the player last lost health (BotSurvival.critical tracks it).
function BotSurvival.hurt_tick()
    return state().hurt_tick
end

-- Whether a fight owns the pad (BotTactics, which loads after this file).
function BotSurvival.active()
    return BotTactics.active()
end

-- The ammo reserve rule. A gun keeps enough rounds (its clip plus the
-- inventory count of the ammunition the engine loads into it) for
-- `1 + 2 * (1 - risk)` kills of the hardest `fight` monster of the knowledge
-- file: three kills at risk 0, one at risk 1. Below that floor the planner
-- ranks a reachable frontier item of the gun's ammunition first; with none
-- reachable it sources ammunition (BotSurvival.sourcing, below). The item
-- kinds come from the engine (bot:weapon().ammo, the catalog), never ids.

-- The most hits one kill of a `fight` monster takes with `weapon`, from the
-- knowledge file's health and <keep> damage, or nil when nothing is known.
function BotSurvival.shots_per_kill(knowledge, weapon)
    local most = nil
    for _, enemy in pairs(knowledge and knowledge.enemies or {}) do
        local keep = enemy.keep and enemy.keep[weapon]
        local damage = keep and keep.damage
        if enemy.policy == "fight" and enemy.health and damage and damage > 0 then
            local shots = enemy.health // damage + 1
            if not most or shots > most then
                most = shots
            end
        end
    end
    return most
end

-- The rounds to keep for a gun whose kill takes `shots` hits, at `risk`.
function BotSurvival.ammo_floor(shots, risk)
    if not shots then
        return 0
    end
    local want = shots * (1 + 2 * (1 - risk))
    local floor = want // 1
    if floor < want then
        floor = floor + 1
    end
    return floor
end

-- The ammunition the equipped gun needs now: the item id, the rounds held
-- and the floor, or nil when the gun has no ammunition item or enough.
function BotSurvival.ammo_need(bot)
    local weapon = bot:weapon()
    if not weapon.ammo then
        return nil
    end
    local rounds = (weapon.clip or 0) + bot:item_count(weapon.ammo)
    local floor = BotSurvival.ammo_floor(
        BotSurvival.shots_per_kill(bot:knowledge(), weapon.weapon), risk(bot))
    if rounds >= floor then
        return nil
    end
    return weapon.ammo, rounds, floor
end

-- The equipped gun's rounds (clip plus its ammunition held) and the hits
-- one kill of the hardest `fight` monster takes with it (nil unknown). A
-- bot with no firearm has 0 rounds.
function BotSurvival.rounds(bot)
    local weapon = bot:weapon()
    if not weapon.ammo then
        return 0, nil
    end
    return (weapon.clip or 0) + bot:item_count(weapon.ammo),
        BotSurvival.shots_per_kill(bot:knowledge(), weapon.weapon)
end

-- The ammo sourcing rule. `need` is BotSurvival.ammo_need's item (nil when
-- the rounds are at the floor), `known` the ammunition of that kind the
-- planner can still reach: frontier pickups it has not given up on and a
-- remembered item box that holds it. Under the floor with none known, the
-- bot is sourcing: the planner ranks the rooms not yet entered (and an item
-- box never opened) first, every walk plans around the monsters, and
-- combat fights only what it must (BotTactics.decide's `sourcing`).
function BotSurvival.sourcing(need, known)
    return need ~= nil and (known or 0) == 0
end

-- The planner's latest count of reachable known ammunition (set each time
-- it ranks the ways out), and the live sourcing state from it.
function BotSurvival.note_known_ammo(n)
    state().known_ammo = n
end

function BotSurvival.sourcing_now(bot)
    local known = state().known_ammo
    if known == nil then
        return false
    end
    return BotSurvival.sourcing(BotSurvival.ammo_need(bot), known)
end
