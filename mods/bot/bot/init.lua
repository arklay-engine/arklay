-- The Arklay player bot: a generic planner over the observed world.
--
-- The bot drives the same pad a human would. It reads the snapshot, the
-- world model (bot:world()) and the room facts (bot:room_meta()); it never
-- writes game state. Each tick, highest priority first:
--   1. a running inventory-menu job (bot/menu.lua) owns the pad;
--   2. scenes, films, door transitions, messages, prompts and viewers
--      (bot/scene.lua);
--   3. survival: cure a poison with a held <cure> item, then heal below a
--      risk-scaled threshold (bot/survival.lua);
--   4. saving at a typewriter here when the risk-scaled decision says so
--      (bot/save.lua), never one a reload would turn straight back into
--      the same death (critical health with no heal item or no rounds for
--      one kill) while a better save exists; at critical health with no
--      heal item, a save if that allows one, else play on when a save
--      exists, else stop;
--   5. combat (bot/tactics.lua): fight or run past each monster by the
--      knowledge file's entry, the risk setting, health and rounds;
--   6. the planner (bot/planner.lua): room puzzle facts, items, stairs,
--      doors, and the finish when nothing reachable is left or the
--      --bot-goal-room is entered.
-- A death loads the latest save through the title's load screen
-- (on_bot_screen below); without a save the run finishes "dead, no save".
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
    -- First, so the memory the engine checkpoints with a save holds it.
    BotSave.observe(bot)
    on_damage(bot, player)

    if BOT_STATE.room ~= room.id then
        enter_room(bot, room)
    end

    -- An inventory job owns the pad until the menu is closed again.
    if BotMenu.step(bot) then
        return
    end
    -- The item box screen, its lid and its close fade.
    if BotInventory.screen_step(bot) then
        return
    end

    local signature = BotScene.signature(scene)
    if signature ~= BOT_STATE.scene_sig then
        BOT_STATE.scene_sig = signature
        bot:log("scene " .. signature)
    end
    if scene.dead then
        BotSave.on_death(bot)
        return
    end
    BotSave.note_prompt(bot, scene)
    if BotSave.screen_step(bot, scene) then
        return
    end
    if scene.message then
        BotPlanner.note_message(bot, scene.message)
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

    if BotSurvival.cure(bot, player) or BotSurvival.heal(bot, player) then
        return
    end
    local critical = BotSurvival.critical(bot, player)
    if BotSave.step(bot, player, critical) then
        return
    end
    if critical and not BotSave.has_save(bot) then
        BotPlanner.finish(bot,
            "health critical with no heal item and no save while under attack")
        return
    end
    if BotTactics.step(bot, scene, player) then
        return
    end
    BotPlanner.step(bot)
end

-- The title and its load screen, where the bot loads its latest save after
-- a death (bot/save.lua).
function on_bot_screen(bot)
    BotSave.app_screen(bot)
end
