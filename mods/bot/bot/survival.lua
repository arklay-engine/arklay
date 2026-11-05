-- Survival for the player bot: healing and the critical-health rule. Loads
-- after constants.lua (chunks run in path order), whose table it captures.
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
-- The fight itself lives in bot/tactics.lua (BotTactics); healing waits
-- while it runs.
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
local function heal_fraction(bot)
    return 0.6 - 0.35 * risk(bot)
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
    if player.grabbed or BotTactics.active() or s.heal_cooldown > 0 then
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

-- The tick the player last lost health (BotSurvival.critical tracks it).
function BotSurvival.hurt_tick()
    return state().hurt_tick
end

-- Whether a fight owns the pad (BotTactics, which loads after this file).
function BotSurvival.active()
    return BotTactics.active()
end
