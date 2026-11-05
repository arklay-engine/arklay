-- Saving at typewriters and loading the latest save after a death.
--
-- A typewriter is a room action of kind "typewriter" (the room-action
-- handler the engine runs for it). The bot saves the way a player does:
-- walk into the zone and press action, answer the save prompt Yes (the
-- scene handler's default), wait through the fade, then drive the save
-- screen with the d-pad and confirm. It never writes a save or loads one by
-- a call; bot:saves() only reports what the engine's save screen wrote.
--
-- The save decision (BotSave.wants), scaled by bot:risk():
--   - no save without an ink ribbon when the character needs one
--     (bot:saves().needs_ribbon; Jill by default saves free);
--   - no save when nothing new happened since the last one, or since the run
--     started (progress: rooms entered, items taken, doors unlocked;
--     BotSave.progress), so the new game's first room is no save;
--   - save safety: no save that a reload would turn straight back into the
--     same death. Health under BotSurvival's critical floor with no heal
--     item held, or with too few rounds for one kill of the hardest `fight`
--     monster (BotSave.dead_end), is a dead end: the reload starts there
--     with nothing to fight or heal with, and a death since such a save
--     repeats until the run gives up. A dead end is saved only when there
--     is no other choice (BotSave.no_choice): no save exists yet, or the
--     latest save is no better (no more health, rounds or heals; recorded
--     as save_health, save_rounds and save_heals when it is written);
--   - no save while inventory work is due in this room (a combine, or a
--     visit to an item box here: BotPlanner.inventory_due_here), so the
--     save holds the inventory after it;
--   - critical health (BotSurvival's floor, no heal item, hurt just now)
--     saves at once, once the save-safety rule allows it;
--   - otherwise at most one save per cooldown, 20 s at risk 0 to 120 s at
--     risk 1 (BotSave.cooldown);
--   - risk up to BOLD_RISK saves below LOW_HEALTH of max health, below
--     RISKY_HEALTH when the room the pursued target's route enters next had
--     a live monster (BotPlanner.risky_next), or when a milestone (a key item taken, a
--     door unlocked, a <use> fact done: BotSave.milestones) is new since
--     the last save and the last save is at least BotSave.milestone_gap
--     old (2 minutes at risk 0, 3 at the default 0.25); with no save yet,
--     at the first milestone;
--   - a bolder bot saves only below LOW_HEALTH of max health, or in a risky
--     room: a live monster in the room, or damage taken in the last
--     RISKY_DAMAGE ticks.
-- The latest save is tracked in bot memory (save_count, save_slot,
-- save_room, save_tick, save_progress, save_health, save_rounds,
-- save_heals) the tick the engine reports it.
--
-- After a death: with a save, skip the DIED screen with confirm; the game
-- returns to the title, where on_bot_screen picks LOAD GAME, moves the load
-- screen's cursor to the latest save's slot and confirms. Without a save the
-- run finishes "dead, no save". More than MAX_DEATHS deaths on one save
-- finish the run too. No room literals; only pad/UI input.

BotSave = BotSave or {}

local C = BotConst

-- Risk above this saves only when health is low or the room is risky.
local BOLD_RISK = 0.5
local LOW_HEALTH = 0.5
-- A cautious bot saves before a room with a live monster only under this.
local RISKY_HEALTH = 0.75
-- Damage this recent makes a room risky.
local RISKY_DAMAGE = 30 * C.SECOND
-- The walk to the typewriter and the press, and the save screen itself.
local JOB_TIMEOUT = 40 * C.SECOND
local SCREEN_TIMEOUT = 60 * C.SECOND
-- After the prompt, the save must land within this long.
local SAVE_WAIT = 30 * C.SECOND
-- A typewriter that failed is not tried again for this long.
local RETRY = 60 * C.SECOND
-- How often the decision is evaluated while a typewriter is in reach.
local DECIDE_EVERY = C.SECOND
-- Deaths on one save before the run gives up.
local MAX_DEATHS = 3
-- One pad edge per this many ticks on the screens.
local PRESS_GAP = 4
-- The title's LOAD GAME entry.
local LOAD_GAME = 2

local function state()
    BOT_STATE.save = BOT_STATE.save or { failed = {} }
    return BOT_STATE.save
end

-- Save cooldown in ticks: 20 s at risk 0, 120 s at risk 1.
function BotSave.cooldown(risk)
    return (2 + 10 * risk) * 10 * C.SECOND
end

-- The least age of the last save (or the run start) for a milestone save:
-- 2 minutes at risk 0, 6 at risk 1.
function BotSave.milestone_gap(risk)
    return (2 + 4 * risk) * 60 * C.SECOND
end

-- Whether a typewriter save is possible now: a ribbon is held, or the
-- character needs none.
function BotSave.can_save(saves)
    return not saves.needs_ribbon or (saves.ribbons or 0) > 0
end

-- The save decision, from plain values (see the header):
--   risk, can_save, health, max_health, critical, risky,
--   progress, saved_progress (the last save's, or the run start's),
--   since_save (ticks since the last save, nil before the first).
-- Returns the decision and its reason.
function BotSave.wants(o)
    if not o.can_save then
        return false, "no ink ribbon"
    end
    if o.saved_progress and o.progress <= o.saved_progress then
        return false, "nothing new since the last save"
    end
    if o.dead_end and not o.no_choice then
        return false, "a reload would start in the same death: " .. tostring(o.dead_end)
    end
    if o.inventory_due then
        return false, "inventory work first: " .. tostring(o.inventory_due)
    end
    if o.critical then
        return true, "critical health"
    end
    if o.since_save and o.since_save < BotSave.cooldown(o.risk) then
        return false, "saved recently"
    end
    if o.risk <= BOLD_RISK then
        if o.health < o.max_health * LOW_HEALTH then
            return true, "low health"
        end
        if o.risky_next and o.health < o.max_health * RISKY_HEALTH then
            return true, "a risky room is next: " .. tostring(o.risky_next)
        end
        if o.milestone and not o.since_save then
            return true, "the first milestone of the run (no save yet)"
        end
        if o.milestone and o.since_save >= BotSave.milestone_gap(o.risk) then
            return true, "a milestone since the last save"
        end
        if o.milestone then
            return false, "a milestone, but the last save is recent"
        end
        return false, "no milestone since the last save"
    end
    if o.health < o.max_health * LOW_HEALTH then
        return true, "low health"
    end
    if o.risky then
        return true, "risky room"
    end
    return false, "a bold bot skips a save while healthy in a calm room"
end

-- The save-safety test (see the header), from plain values: health,
-- max_health, risk, heals (heal stacks held), rounds (the equipped gun's
-- clip plus its ammunition) and shots (hits for one kill of the hardest
-- `fight` monster, nil unknown). Returns why a save here is a dead end, or
-- nil.
function BotSave.dead_end(o)
    if o.health >= o.max_health * BotSurvival.critical_fraction(o.risk) then
        return nil
    end
    if (o.heals or 0) == 0 then
        return "critical health with no heal item"
    end
    if (o.rounds or 0) < (o.shots or 1) then
        return "critical health without the rounds for one kill"
    end
    return nil
end

-- Whether a dead-end save is still the best there is: no save exists, or
-- the latest one (BotSave.latest) is no better than `now` ({health, rounds,
-- heals}) in health, rounds and heals. A save from before these were
-- recorded counts as better.
function BotSave.no_choice(latest, now)
    if not latest then
        return true
    end
    if not (latest.health and latest.rounds and latest.heals) then
        return false
    end
    return latest.health <= now.health and latest.rounds <= now.rounds
        and latest.heals <= now.heals
end

-- What the save-safety rule reads from the live state.
local function condition(bot, player)
    local rounds, shots = BotSurvival.rounds(bot)
    return {
        health = player.health,
        max_health = player.max_health,
        risk = bot:risk() or 0.25,
        heals = BotSurvival.heal_count(bot),
        rounds = rounds,
        shots = shots,
    }
end

-- Progress for the "nothing new" test: rooms entered, items taken and doors
-- unlocked in the world model.
function BotSave.progress(world)
    local n = 0
    for _, r in ipairs(world.rooms or {}) do
        if r.visited then
            n = n + 1
        end
        for _, item in ipairs(r.items or {}) do
            if item.taken then
                n = n + 1
            end
        end
        for _, door in ipairs(r.doors or {}) do
            if door.unlocked then
                n = n + 1
            end
        end
    end
    return n
end

-- Milestones for the cautious save rule: key items taken, doors unlocked,
-- <use> facts done and `steps` (the puzzle goals the planner finished).
-- `kind(item)` is bot:item_kind, `done(fact)` reads a fact's done flag.
function BotSave.milestones(world, kind, facts, done, steps)
    local n = steps or 0
    for _, r in ipairs(world.rooms or {}) do
        for _, item in ipairs(r.items or {}) do
            if item.taken and kind(item.item) == "key" then
                n = n + 1
            end
        end
        for _, door in ipairs(r.doors or {}) do
            if door.unlocked then
                n = n + 1
            end
        end
    end
    for _, fact in ipairs(facts or {}) do
        if done(fact) then
            n = n + 1
        end
    end
    return n
end

local function milestones(bot)
    return BotSave.milestones(bot:world(), function(item) return (bot:item_kind(item)) end,
        bot:use_facts(), function(fact) return bot:flag(fact.bank, fact.bit) end,
        BotPlanner.steps_done())
end

-- The latest-save tracking: given bot:memory() and bot:saves(), the memory
-- entries to remember for a save not yet recorded, or nil. `now` (optional)
-- is the save-safety condition when it was written: {health, rounds, heals}.
function BotSave.track(memory, saves, progress, now)
    local seen = tonumber(memory.save_count or "0") or 0
    if (saves.count or 0) <= seen then
        return nil
    end
    return {
        save_count = saves.count,
        save_slot = saves.slot,
        save_room = saves.room,
        save_tick = saves.tick,
        save_progress = progress,
        save_health = now and now.health,
        save_rounds = now and now.rounds,
        save_heals = now and now.heals,
        save_milestones = now and now.milestones,
    }
end

-- The latest save from memory: {count, slot, room, tick, progress} or nil.
function BotSave.latest(memory)
    local count = tonumber(memory.save_count or "0") or 0
    if count == 0 then
        return nil
    end
    return {
        count = count,
        slot = tonumber(memory.save_slot or ""),
        room = memory.save_room,
        tick = tonumber(memory.save_tick or ""),
        progress = tonumber(memory.save_progress or ""),
        health = tonumber(memory.save_health or ""),
        rounds = tonumber(memory.save_rounds or ""),
        heals = tonumber(memory.save_heals or ""),
        milestones = tonumber(memory.save_milestones or ""),
    }
end

-- The slot a new save goes to (0-based): the first empty one, else the
-- slots in turn by save count.
function BotSave.pick_slot(slots, count)
    for i = 1, C.SAVE_SLOTS do
        if not slots[i] then
            return i - 1
        end
    end
    return (count or 0) % C.SAVE_SLOTS
end

function BotSave.has_save(bot)
    return bot:saves().slot ~= nil
end

-- Record a save the engine just reported. Runs first on every session tick
-- so the memory checkpoint the engine keeps with the save includes it.
function BotSave.observe(bot)
    local s = state()
    if not s.start_progress then
        s.start_progress = BotSave.progress(bot:world())
        s.start_milestones = milestones(bot)
    end
    local saves = bot:saves()
    local memory = bot:memory()
    if (saves.count or 0) <= (tonumber(memory.save_count or "0") or 0) then
        return
    end
    local now = condition(bot, bot:player())
    now.milestones = milestones(bot)
    local entries = BotSave.track(memory, saves, BotSave.progress(bot:world()), now)
    for key, value in pairs(entries) do
        bot:remember(key, value)
    end
    s.job = nil
    s.screen_ticks = nil
    s.target = nil
    s.last_save_tick = bot:tick()
    bot:log(string.format("saved: slot %s room %s tick %s (saves %d, ribbons %s)",
        tostring(saves.slot), tostring(saves.room), tostring(saves.tick),
        saves.count, tostring(saves.ribbons)))
end

local function typewriter_here(bot)
    local s = state()
    local room = bot:room().id
    for _, action in ipairs(bot:actions()) do
        if action.kind == "typewriter" and not BotNav.is_origin_zone(action)
            and not BotPuzzle.avoided(bot, action.slot) then
            local failed = s.failed[room .. ":" .. action.slot]
            if not failed or bot:tick() - failed > RETRY then
                return action
            end
        end
    end
    return nil
end

local function risky(bot)
    if #bot:enemies() > 0 then
        return true
    end
    local hurt = BotSurvival.hurt_tick and BotSurvival.hurt_tick()
    return hurt ~= nil and bot:tick() - hurt <= RISKY_DAMAGE
end

-- The row a save screen shows `slot` on: the slot itself where the rows are
-- the slots in order, else the row `view.rows` maps to it (nil for none).
local function slot_row(view, slot)
    if not view.rows then
        return slot
    end
    for i, row_slot in ipairs(view.rows) do
        if row_slot == slot then
            return i - 1
        end
    end
    return nil
end

-- Drive the typewriter's save screen: move to the target slot, confirm,
-- accept the overwrite question, and wait for the write and the fade.
local function drive_save_screen(bot, view)
    local s = state()
    local tick = bot:tick()
    s.screen_ticks = (s.screen_ticks or 0) + 1
    if s.screen_ticks == 1 then
        s.target = BotSave.pick_slot(view.slots, bot:saves().count)
        bot:log(string.format("save screen: writing slot %d", s.target))
    end
    if s.screen_ticks > SCREEN_TIMEOUT or s.screen_error then
        if view.stage == "idle" and tick % PRESS_GAP == 0 then
            bot:set_input({ cancel = true })
        end
        return
    end
    if tick % PRESS_GAP ~= 0 then
        return
    end
    local row = slot_row(view, s.target)
    if row == nil then
        bot:log(string.format("save screen: no row writes slot %d; leaving", s.target))
        s.screen_error = true
        return
    end
    if view.stage == "idle" then
        if view.cursor < row then
            bot:set_input({ down_pressed = true })
        elseif view.cursor > row then
            bot:set_input({ up_pressed = true })
        else
            bot:pulse_confirm()
        end
    elseif view.stage == "confirm_overwrite" then
        -- The classic dialog opens on YES; a screen that says otherwise
        -- reports the highlighted answer.
        if view.answer == "no" then
            bot:set_input({ left_pressed = true })
        else
            bot:pulse_confirm()
        end
    elseif view.stage == "error" then
        bot:log("save screen: the write failed; leaving")
        s.screen_error = true
        bot:pulse_confirm()
    end
end

-- One tick before the scene handler: the save screen and the typewriter's
-- fades own the pad. Returns true when it consumed the tick.
function BotSave.screen_step(bot, scene)
    if scene.save_screen and scene.save_screen.mode == "save" then
        drive_save_screen(bot, scene.save_screen)
        return true
    end
    local s = state()
    if s.screen_ticks then
        -- The screen closed without a save (BotSave.observe clears this on
        -- a save): do not walk straight back to the same typewriter.
        if s.job then
            s.failed[bot:room().id .. ":" .. s.job.slot] = bot:tick()
        end
        s.screen_ticks = nil
        s.screen_error = nil
        s.job = nil
    end
    return scene.typewriter == true
end

-- One controllable tick: run the save job, or start one at a typewriter
-- here when BotSave.wants says so. Returns true when it owns the tick.
function BotSave.step(bot, player, critical)
    local s = state()
    local tick = bot:tick()
    local job = s.job
    if job then
        if job.prompted then
            -- The prompt was answered and the screen never came, or a save
            -- is taking too long.
            if tick - job.prompted > SAVE_WAIT then
                bot:log("save: no save after the prompt; giving up")
                s.failed[bot:room().id .. ":" .. job.slot] = tick
                s.job = nil
            end
            return false
        end
        local status = BotNav.press_zone(bot, job)
        if status == "failed" then
            bot:log("save: " .. tostring(job.reason))
            s.failed[bot:room().id .. ":" .. job.slot] = tick
            s.job = nil
            return false
        end
        return true
    end
    -- A fight in progress finishes first.
    if BotSurvival.active() or tick - (s.decided or -DECIDE_EVERY) < DECIDE_EVERY then
        return false
    end
    s.decided = tick
    local action = typewriter_here(bot)
    if not action then
        return false
    end
    local saves = bot:saves()
    local latest = BotSave.latest(bot:memory())
    local now = condition(bot, player)
    local saved_milestones = latest and latest.milestones or s.start_milestones or 0
    local wants, why = BotSave.wants({
        inventory_due = BotPlanner.inventory_due_here(bot),
        risky_next = BotPlanner.risky_next(bot),
        milestone = milestones(bot) > saved_milestones,
        dead_end = BotSave.dead_end(now),
        no_choice = BotSave.no_choice(latest, now),
        risk = bot:risk() or 0.25,
        can_save = BotSave.can_save(saves),
        health = player.health,
        max_health = player.max_health,
        critical = critical,
        risky = risky(bot),
        progress = BotSave.progress(bot:world()),
        saved_progress = latest and latest.progress or s.start_progress,
        since_save = latest and latest.tick and tick - latest.tick,
    })
    local note = string.format("%s:%d:%s:%s", bot:room().id, action.slot,
        tostring(wants), why)
    if note ~= s.note then
        s.note = note
        bot:log(string.format("typewriter slot %d: %s (%s)", action.slot,
            wants and "save" or "skip", why))
    end
    if not wants then
        return false
    end
    BotPlanner.drop_goal(bot)
    s.job = {
        slot = action.slot,
        zone = { x = action.x, z = action.z, width = action.width, depth = action.depth },
        label = string.format("typewriter slot %d", action.slot),
        pattern = "edge",
        presses = 2,
        gap = 20,
        timeout = JOB_TIMEOUT,
        done = function(b)
            local scene = b:scene()
            return scene.prompt or scene.typewriter or scene.save_screen ~= nil
        end,
    }
    return true
end

-- Mark the running job as prompted the tick the save prompt appears, so the
-- walk stops and the scene handler answers it.
function BotSave.note_prompt(bot, scene)
    local job = state().job
    if job and not job.prompted and (scene.prompt or scene.typewriter
        or scene.save_screen) then
        job.prompted = bot:tick()
    end
end

-- The player is dead. Without a save, finish; with one, skip the DIED
-- screen so the game returns to the title.
function BotSave.on_death(bot)
    local saves = bot:saves()
    if not saves.slot then
        BotPlanner.finish(bot, "dead, no save")
        return
    end
    if (saves.deaths_since_save or 0) > MAX_DEATHS then
        BotPlanner.finish(bot, string.format(
            "dead %d times since the last save (slot %d, room %s)",
            saves.deaths_since_save, saves.slot, tostring(saves.room)))
        return
    end
    local s = state()
    if not s.death_logged then
        s.death_logged = true
        bot:log(string.format("died: loading the save in slot %d (room %s, tick %s)",
            saves.slot, tostring(saves.room), tostring(saves.tick)))
    end
    if bot:tick() % PRESS_GAP == 0 then
        bot:pulse_confirm()
    end
end

-- on_bot_screen: the title and its load screen, after a death.
function BotSave.app_screen(bot)
    local screen = bot:screen()
    local saves = bot:saves()
    local tick = bot:tick()
    BOT_STATE.screen_ticks = (BOT_STATE.screen_ticks or 0) + 1
    if not saves.slot then
        bot:finish("dead, no save")
        return
    end
    if BOT_STATE.screen_ticks > SCREEN_TIMEOUT then
        bot:finish("stuck on the " .. screen.kind .. " screen")
        return
    end
    if screen.kind ~= BOT_STATE.screen_kind then
        BOT_STATE.screen_kind = screen.kind
        bot:log(string.format("on the %s screen; loading slot %d", screen.kind, saves.slot))
    end
    if tick % PRESS_GAP ~= 0 then
        return
    end
    if screen.kind == "title" then
        if screen.stage == "press" or screen.stage == "attract_fade" then
            bot:pulse_confirm()
        elseif screen.stage == "menu" then
            if not screen.any_saves then
                bot:finish("dead: the title offers no LOAD GAME")
            elseif screen.selection ~= LOAD_GAME then
                bot:set_input({ right_pressed = true })
            else
                bot:pulse_confirm()
            end
        end
    elseif screen.kind == "load" then
        if not screen.readable then
            bot:finish("dead: the load screen shows nothing the bot can read")
            return
        end
        if screen.stage ~= "idle" then
            return
        end
        local row = slot_row(screen, saves.slot)
        if not screen.slots[saves.slot + 1] or row == nil then
            bot:finish(string.format("dead: the save in slot %d is gone", saves.slot))
        elseif screen.cursor < row then
            bot:set_input({ down_pressed = true })
        elseif screen.cursor > row then
            bot:set_input({ up_pressed = true })
        else
            bot:pulse_confirm()
        end
    end
end
