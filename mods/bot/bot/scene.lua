-- Scene, message and viewer handling for the player bot.
--
-- BotScene.handle(bot, scene) runs before the planner on every tick that no
-- menu job owns. It skips films, door transitions and skippable scenes,
-- pages messages, answers yes/no prompts (the room's <answer> facts through
-- BotPuzzle.answer, otherwise Yes), closes item viewers and mashes out of a
-- grab. It returns true when it consumed the tick, so the planner must not
-- move. Only pad/UI input; no room literals.

BotScene = BotScene or {}

local C = BotConst

local function state()
    BOT_STATE.scene = BOT_STATE.scene or {}
    return BOT_STATE.scene
end

-- A fresh action edge for a message window: the window consumes an edge
-- (held now, not held last tick) in a page wait, a finished-text wait or a
-- prompt. A message id with the fast-reveal bit also reveals faster while
-- the button is held, so hold it through the reveal.
local function page_message(bot, scene, s, tick)
    local held_last = s.msg_press_tick == tick - 1
    if scene.message_waiting then
        if not held_last then
            s.msg_press_tick = tick
            bot:press_action()
        end
    elseif scene.message and (scene.message & C.MESSAGE_FAST) ~= 0 then
        s.msg_press_tick = tick
        bot:press_action()
    end
end

-- A yes/no prompt: move the cursor to the wanted answer, then confirm with a
-- fresh edge. Left selects No, right selects Yes.
local function answer_prompt(bot, scene, s, tick)
    local want = "yes"
    if BotPuzzle and BotPuzzle.answer then
        want = BotPuzzle.answer(bot) or "yes"
    end
    if s.prompt_logged ~= tick - 1 then
        bot:log(string.format("prompt: answering %s (cursor on %s)", want,
            tostring(scene.prompt_choice)))
    end
    s.prompt_logged = tick
    if scene.prompt_choice ~= want then
        if s.prompt_move_tick ~= tick - 1 then
            s.prompt_move_tick = tick
            bot:set_input({ [want == "yes" and "right" or "left"] = true })
        end
        return
    end
    page_message(bot, scene, s, tick)
end

function BotScene.handle(bot, scene)
    local s = state()
    local tick = bot:tick()
    if not scene.transition then
        s.transition_start = nil
    end
    if scene.film then
        if tick % 2 == 0 then
            bot:pulse_confirm()
        end
        return true
    end
    if scene.transition then
        -- The transition's skip gate opens on animation frame 11, and a
        -- button already down when it begins cannot skip it. Release on the
        -- first tick, then press on frames 1, 3, 5, ... so the press on frame
        -- 11 is the first allowed one whatever tick it started on.
        if not s.transition_start then
            s.transition_start = tick
        end
        if (tick - s.transition_start) % 2 == 1 then
            bot:press_action()
        end
        return true
    end
    if scene.cutscene_skip then
        return true
    end
    if scene.skippable then
        -- A room's <watch> fact lets the scene play out: hands off the pad
        -- until the scene ends (or a message asks for a press).
        local watch = BotPuzzle and BotPuzzle.watching and BotPuzzle.watching(bot)
        if watch then
            if s.watch_logged ~= tick - 1 then
                bot:log("scene: watching, not skipping (" .. tostring(watch.label) .. ")")
            end
            s.watch_logged = tick
            return true
        end
        if tick % 2 == 0 then
            bot:press_action()
        end
        return true
    end
    if scene.pickup then
        -- The got-item viewer owns the frozen frame and closes on the
        -- action/confirm edge. Remember the close window: an action press
        -- buffered during the exit fade re-arms a reach-gated ground item.
        s.saw_pickup = true
        s.pickup_cooldown = tick + C.SECOND + C.SECOND // 2
        s.action_release_until = tick + 4
        if scene.message_takes_input then
            if scene.prompt then
                answer_prompt(bot, scene, s, tick)
            else
                page_message(bot, scene, s, tick)
            end
        elseif scene.reader then
            -- A document pick-up opens the FILE reader, which pages on the
            -- D-pad and closes on a cancel edge.
            if tick % 4 == 0 then
                bot:set_input({ cancel = true })
            end
        elseif tick % 2 == 0 then
            bot:press_action()
        else
            bot:pulse_confirm()
        end
        return true
    end
    if scene.message_takes_input then
        if scene.prompt then
            answer_prompt(bot, scene, s, tick)
        else
            page_message(bot, scene, s, tick)
        end
        return true
    end
    if scene.frozen then
        -- Item viewers and other full-screen UI close on action/confirm. Do
        -- not press during the pickup viewer's exit fade (see above).
        if tick < (s.pickup_cooldown or 0) then
            return true
        end
        if tick % 2 == 0 then
            bot:press_action()
        else
            bot:pulse_confirm()
        end
        return true
    end
    local player = bot:player()
    if player.being_attacked and player.being_attacked > 0 then
        -- Mash only for a real hold. The attacked byte is also the shared
        -- committed-action byte, and a plant bite can leave it set, so
        -- mashing on every nonzero value would random-walk the room. Holds
        -- raise `grabbed` or use player states 5/6; vault/stair sequences
        -- set the sequence bit, and their second press completes them.
        local held = player.grabbed or player.state == 5 or player.state == 6
            or (player.being_attacked & C.ATTACK_SEQUENCE) ~= 0
        if held then
            local dirs = { "left", "right", "up", "down" }
            bot:press_action()
            bot:set_input({ [dirs[tick % 4 + 1]] = true })
            return true
        end
        if not s.stale_attack_logged then
            s.stale_attack_logged = true
            bot:log(string.format(
                "ignore stale attacked=%d state=%d grabbed=%s health=%d",
                player.being_attacked, player.state, tostring(player.grabbed),
                player.health))
        end
    end
    return false
end

-- Whether the got-item viewer was seen since `BotScene.clear_pickup`.
function BotScene.saw_pickup()
    return state().saw_pickup == true
end

function BotScene.clear_pickup()
    state().saw_pickup = false
end

-- The tick before which an action press would leak into a closing viewer.
function BotScene.action_ready(bot)
    return bot:tick() >= (state().action_release_until or 0)
end

function BotScene.signature(scene)
    return string.format(
        "film=%s trans=%s skip=%s skippable=%s msg=%s input=%s prompt=%s "
            .. "pickup=%s frozen=%s ctrl=%s",
        tostring(scene.film), tostring(scene.transition), tostring(scene.cutscene_skip),
        tostring(scene.skippable), tostring(scene.message),
        tostring(scene.message_takes_input), tostring(scene.prompt),
        tostring(scene.pickup), tostring(scene.frozen), tostring(scene.controllable))
end
