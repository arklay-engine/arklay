-- The Arklay player bot: a generic planner over the observed world.
--
-- The bot drives the same pad a human would. It reads the snapshot, the
-- world model (bot:world()) and the room facts (bot:room_meta()); it never
-- writes game state. Each tick, highest priority first:
--   1. a running inventory-menu job (bot/menu.lua) owns the pad;
--   2. scenes, films, door transitions, messages, prompts and viewers
--      (bot/scene.lua);
--   3. survival: heal below a risk-scaled threshold, then fight a common
--      zombie in front (bot/survival.lua);
--   4. the planner (bot/planner.lua): room puzzle facts, items, stairs,
--      doors, and the finish when nothing reachable is left or the
--      --bot-goal-room is entered.
-- Module tables are globals because chunks run in path order and a --watch
-- reload re-runs them in the same Lua state. No module holds a room id, flag
-- bit, item id, slot number or coordinate; engine rules live in
-- bot/constants.lua and room facts in room/<id>/bot.xml.

BOT_STATE = BOT_STATE or {}

local function on_damage(bot, player)
    local last = BOT_STATE.last_health
    BOT_STATE.last_health = player.health
    if not last or player.health >= last then
        return
    end
    local enemies = bot:enemies()
    local nearest = enemies[1]
    bot:log(string.format(
        "damage health %d -> %d pos=(%d,%d) state=%d grabbed=%s nearest=%s",
        last, player.health, player.x, player.z, player.state,
        tostring(player.grabbed),
        nearest and string.format("slot=%d id=%#04x dist=%d", nearest.slot,
            nearest.id, nearest.distance) or "none"))
end

local function enter_room(bot, room)
    local p = bot:player()
    bot:log(string.format("room %s pos=(%d,%d) camera=%d health=%d/%d",
        room.id, p.x, p.z, room.camera, p.health, p.max_health))
    BOT_STATE.room = room.id
    BotPlanner.enter_room(bot, room)
end

function on_bot_tick(bot)
    local room = bot:room()
    local scene = bot:scene()
    local player = bot:player()
    on_damage(bot, player)

    if BOT_STATE.room ~= room.id then
        enter_room(bot, room)
    end

    -- An inventory job owns the pad until the menu is closed again.
    if BotMenu.step(bot) then
        return
    end

    local signature = BotScene.signature(scene)
    if signature ~= BOT_STATE.scene_sig then
        BOT_STATE.scene_sig = signature
        bot:log("scene " .. signature)
    end
    if scene.dead then
        BotPlanner.finish(bot, "player died")
        return
    end
    if BotScene.handle(bot, scene) then
        return
    end
    if not scene.controllable then
        BOT_STATE.locked_ticks = (BOT_STATE.locked_ticks or 0) + 1
        if BOT_STATE.locked_ticks > 5 * BotConst.SECOND then
            -- A grab or an unmarked lock: mash so it cannot stall the run.
            local dirs = { "left", "right", "up", "down" }
            bot:press_action()
            bot:set_input({ [dirs[bot:tick() % 4 + 1]] = true })
        end
        return
    end
    BOT_STATE.locked_ticks = 0

    if BotSurvival.heal(bot, player) then
        return
    end
    if BotSurvival.critical(bot, player) then
        BotPlanner.finish(bot, "health critical with no heal item while under attack")
        return
    end
    if BotSurvival.step(bot, scene, player) then
        return
    end
    BotPlanner.step(bot)
end
