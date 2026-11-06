-- Inventory menu skills for the player bot.
--
-- Equip, heal and item use go through the pause menu exactly as a player
-- does it: START opens the menu, the d-pad moves the cursor to the item's
-- slot, confirm opens the item's submenu, the d-pad picks the option
-- (EQUIP/USE, CHECK, MOVE), confirm applies it and cancel backs out and
-- closes the menu. Every step is one button pulse with a release tick between
-- pulses, and every step waits on the engine's real menu state from
-- `bot:menu()` (the menu phase, the cursor byte, the submenu mode and
-- option). A job succeeds only when the engine's state says it happened: the
-- equipped item changed to the weapon, or the item's count dropped.
--
-- API (all take the bot handle):
--   BotMenu.equip(bot, item, label)     start an EQUIP job
--   BotMenu.use_heal(bot, item, label)  start a USE job on a heal item
--   BotMenu.use_item(bot, item, label)  start a USE job on any other item
--   BotMenu.combine(bot, slot, target, label)
--                                       start a MOVE job: the item in menu
--                                       slot `slot` onto slot `target`
--                                       (0-based), answering a herb mix's
--                                       yes/no prompt Yes; it succeeds when
--                                       the engine's inventory holds one
--                                       stack fewer
--   BotMenu.examine(bot, item, label)  start a CHECK job on a doom book:
--                                       turn the viewer's turntable to the
--                                       angle that opens it, confirm, and
--                                       wait for the book to become its
--                                       medal
--     Each returns true when the job started (or, for equip, the weapon is
--     already equipped) and false when the item is not held, is the wrong
--     kind, a job is already running, or the same job failed recently.
--   BotMenu.step(bot)    run one tick of the active job; true when the job
--                        owns this tick and the route must not run
--   BotMenu.busy()       a job is running
--   BotMenu.last()       the last finished job {kind, item, ok, reason, tick}
--
-- The job state is a global so a --watch reload keeps a job in flight.

BOT_MENU = BOT_MENU or {}
BOT_MENU.failed = BOT_MENU.failed or {}
BOT_MENU.stray_ticks = BOT_MENU.stray_ticks or 0

BotMenu = BotMenu or {}

local FIRST_SLOT_CURSOR = 8
-- Ticks between two pulses of the same or another button: one press tick,
-- then at least one release tick, so every press is a fresh edge.
local PRESS_GAP = 2
-- START is retried this rarely, so a slow open is not toggled shut.
local START_GAP = 20
local SECOND = BotConst.SECOND
local OPEN_TIMEOUT = 5 * SECOND
local STEP_TIMEOUT = 90
local JOB_TIMEOUT = 40 * SECOND
local SETTLE_TIMEOUT = 4 * SECOND
local EXAMINE_YAW_LOW = BotConst.EXAMINE_YAW.low
local EXAMINE_YAW_HIGH = BotConst.EXAMINE_YAW.high
local DOOM_BOOK_2 = BotConst.DOOM_BOOK_2
local WOLF_MEDAL = BotConst.WOLF_MEDAL
-- A failed job for the same item and kind is not retried for this long.
local RETRY_COOLDOWN = 10 * SECOND

local function log(bot, text)
    bot:log("menu: " .. text)
end

local function find_item(menu, item)
    for i = 1, #menu.items do
        local row = menu.items[i]
        if row.item == item then
            return row
        end
    end
    return nil
end

local function describe(menu)
    return string.format(
        "phase=%s screen=%s cursor=%s slot=%s mode=%s option=%s options=[%s] selected=0x%02X",
        tostring(menu.phase), tostring(menu.screen), tostring(menu.cursor),
        tostring(menu.slot), tostring(menu.mode), tostring(menu.option),
        table.concat(menu.options or {}, ","), menu.selected_item or 0)
end

-- Occupied inventory slots in the menu's list.
local function used_slots(menu)
    local n = 0
    for _, row in ipairs(menu.items or {}) do
        if row.item ~= 0 then
            n = n + 1
        end
    end
    return n
end

local function start(bot, kind, item, label, extra)
    label = label or kind
    if BOT_MENU.job then
        log(bot, string.format("%s 0x%02X refused: job %s 0x%02X is running",
            kind, item, BOT_MENU.job.kind, BOT_MENU.job.item))
        return false
    end
    local key = kind .. ":" .. item
    local failed_at = BOT_MENU.failed[key]
    if failed_at and bot:tick() - failed_at < RETRY_COOLDOWN then
        return false
    end
    local menu = bot:menu()
    local row = find_item(menu, item)
    if extra and extra.slot then
        row = menu.items[extra.slot + 1]
        if not row or row.item ~= item then
            row = nil
        end
    end
    if not row then
        log(bot, string.format("%s 0x%02X refused: not in the inventory", kind, item))
        return false
    end
    if kind == "equip" then
        if row.kind ~= "weapon" then
            log(bot, string.format("equip 0x%02X refused: kind %s is not a weapon",
                item, row.kind))
            return false
        end
        if bot:player().equipped == item then
            return true
        end
    else
        -- An empty gun still takes a clip in a combine, and an uncounted
        -- item (the cranks, quantity 0) is used without being used up. A
        -- doom book out of the box has quantity 0 too and still opens.
        if row.quantity <= 0 and not (kind == "combine" and row.kind == "weapon")
            and not ((kind == "use" or kind == "examine") and row.kind ~= "heal") then
            log(bot, string.format("%s 0x%02X refused: the stack is empty", kind, item))
            return false
        end
        if kind == "heal" and row.kind ~= "heal" then
            log(bot, string.format("heal 0x%02X refused: kind %s is not a heal",
                item, row.kind))
            return false
        end
    end
    BOT_MENU.job = {
        kind = kind,
        item = item,
        label = label,
        stage = "open",
        ticks = 0,
        stage_ticks = 0,
        last_press = -SECOND,
        last_start = -SECOND,
        before_count = bot:item_count(item),
        before_equipped = bot:player().equipped,
        before_used = used_slots(menu),
        slot = extra and extra.slot or nil,
        target = extra and extra.target or nil,
        ok = nil,
        reason = nil,
    }
    log(bot, string.format("%s: start %s 0x%02X (slot %d, count %d, equipped %s)",
        label, kind, item, row.slot, bot:item_count(item),
        tostring(bot:player().equipped)))
    return true
end

function BotMenu.equip(bot, item, label)
    return start(bot, "equip", item, label)
end

function BotMenu.use_heal(bot, item, label)
    return start(bot, "heal", item, label)
end

function BotMenu.use_item(bot, item, label)
    return start(bot, "use", item, label)
end

-- MOVE the item in menu slot `slot` onto the one in `target`. The caller
-- checks the pair with bot:combine_result first.
function BotMenu.combine(bot, slot, target, label)
    local row = bot:menu().items[slot + 1]
    if not row or row.item == 0 then
        log(bot, string.format("combine slot %d refused: the slot is empty", slot))
        return false
    end
    return start(bot, "combine", row.item, label, { slot = slot, target = target })
end

-- CHECK a doom book (Doom Book II 0x3F, Doom Book I 0x40) and open it: the
-- viewer turns it into its medal.
function BotMenu.examine(bot, item, label)
    if item ~= DOOM_BOOK_2 and item ~= DOOM_BOOK_2 + 1 then
        log(bot, string.format("examine 0x%02X refused: not a doom book", item))
        return false
    end
    return start(bot, "examine", item, label)
end

-- The item a CHECK job's book turns into.
local function examine_result(job)
    return job.item - DOOM_BOOK_2 + WOLF_MEDAL
end

function BotMenu.busy()
    return BOT_MENU.job ~= nil
end

function BotMenu.last()
    return BOT_MENU.last
end

local function set_stage(job, stage)
    job.stage = stage
    job.stage_ticks = 0
end

-- Record the outcome; the menu still has to be closed before the job ends.
local function conclude(bot, job, ok, reason)
    if job.ok == nil then
        job.ok = ok
        job.reason = reason
        log(bot, string.format("%s: %s 0x%02X %s (%s)", job.label, job.kind, job.item,
            ok and "done" or "FAILED", reason))
    end
    set_stage(job, "close")
end

local function finish(bot, job)
    BOT_MENU.job = nil
    BOT_MENU.last = {
        kind = job.kind, item = job.item, ok = job.ok and true or false,
        reason = job.reason, tick = bot:tick(),
    }
    if not job.ok then
        BOT_MENU.failed[job.kind .. ":" .. job.item] = bot:tick()
    end
end

-- One button pulse, if the previous one has been released for a tick.
local function pulse(bot, job, key)
    local tick = bot:tick()
    if tick - job.last_press < PRESS_GAP then
        return false
    end
    job.last_press = tick
    if key == "action" then
        bot:press_action()
    else
        bot:set_input({ [key] = true })
    end
    return true
end

-- The d-pad key that moves the 2-wide menu grid's cursor toward `target`.
-- Left/right flip the column; up/down step a row (and wrap at the ends).
local function cursor_key(cursor, target)
    if (cursor & 2) ~= (target & 2) then
        return "right"
    end
    if target > cursor then
        return "down"
    end
    return "up"
end

local function wanted_option(job)
    if job.kind == "equip" then
        return "EQUIP"
    end
    if job.kind == "combine" then
        return "MOVE"
    end
    if job.kind == "examine" then
        return "CHECK"
    end
    return "USE"
end

local function result_seen(bot, job)
    if job.kind == "equip" then
        return bot:player().equipped == job.item
    end
    if job.kind == "combine" then
        return used_slots(bot:menu()) < job.before_used
    end
    if job.kind == "examine" then
        return bot:has_item(examine_result(job))
    end
    return bot:item_count(job.item) < job.before_count
end

-- The menu is open and takes input: navigate it.
local function drive(bot, job, menu)
    if job.stage == "cursor" then
        if menu.mode ~= "navigation" then
            pulse(bot, job, "cancel")
            return
        end
        local row = find_item(menu, job.item)
        if job.slot then
            row = menu.items[job.slot + 1]
            if row and row.item ~= job.item then
                row = nil
            end
        end
        if not row then
            conclude(bot, job, false, "item left the inventory before the use")
            return
        end
        local target = FIRST_SLOT_CURSOR + 2 * row.slot
        if menu.cursor == target then
            set_stage(job, "select")
            return
        end
        pulse(bot, job, cursor_key(menu.cursor, target))
    elseif job.stage == "select" then
        if menu.mode == "actions" then
            set_stage(job, "option")
            return
        end
        if menu.selected_item ~= job.item then
            set_stage(job, "cursor")
            return
        end
        pulse(bot, job, "confirm")
    elseif job.stage == "option" then
        if menu.mode ~= "actions" then
            -- The submenu closed under us (a cancel or a refused confirm).
            set_stage(job, "cursor")
            return
        end
        if not menu.ready or menu.closing then
            return
        end
        local want = wanted_option(job)
        local index = nil
        for i = 1, #menu.options do
            if menu.options[i] == want then
                index = i - 1
            end
        end
        if index == nil then
            conclude(bot, job, false, string.format("the submenu offers no %s (%s)",
                want, describe(menu)))
            return
        end
        if menu.option == index then
            if pulse(bot, job, "confirm") then
                local next = "result"
                if job.kind == "combine" then
                    next = "move"
                elseif job.kind == "examine" then
                    next = "view"
                end
                set_stage(job, next)
            end
            return
        end
        pulse(bot, job, (menu.option or 0) < index and "down" or "up")
    elseif job.stage == "move" then
        -- The MOVE cursor: d-pad onto the target slot, then confirm.
        if menu.mode ~= "move" then
            set_stage(job, menu.mode == "actions" and "option" or "cursor")
            return
        end
        local target = FIRST_SLOT_CURSOR + 2 * job.target
        local at = menu.move_cursor
        if at == nil then
            return
        end
        if at == target then
            if pulse(bot, job, "confirm") then
                set_stage(job, "result")
            end
            return
        end
        pulse(bot, job, cursor_key(at, target))
    elseif job.stage == "result" then
        if result_seen(bot, job) then
            local why
            if job.kind == "equip" then
                why = string.format("equipped 0x%02X", job.item)
            elseif job.kind == "combine" then
                why = string.format("slots %d -> %d", job.before_used, used_slots(menu))
            elseif job.kind == "examine" then
                why = string.format("0x%02X became 0x%02X", job.item, examine_result(job))
            else
                why = string.format("count %d -> %d", job.before_count, bot:item_count(job.item))
            end
            conclude(bot, job, true, why)
        elseif job.stage_ticks > 60 then
            conclude(bot, job, false, string.format(
                "no effect after the %s confirm (count %d, equipped %s; %s)",
                wanted_option(job), bot:item_count(job.item),
                tostring(bot:player().equipped), describe(menu)))
        end
    elseif job.stage == "close" then
        -- Back out of the submenu, then close the panel, as a player would.
        if menu.closing or (menu.mode == "actions" and not menu.ready) then
            return
        end
        pulse(bot, job, "cancel")
    end
end

-- The CHECK viewer is up for an examine job: turn the book to its angle,
-- confirm once, and let the zoom, the message and the medal swap play.
local function drive_examine(bot, job, menu)
    local view = menu.examine
    if job.ok ~= nil or not view then
        -- Done (or failed): cancel leaves the viewer.
        if view and view.ready then
            pulse(bot, job, "cancel")
        end
        return
    end
    if bot:scene().message then
        pulse(bot, job, "action")
        return
    end
    if view.item == examine_result(job) or result_seen(bot, job) then
        conclude(bot, job, true, string.format("0x%02X became 0x%02X", job.item,
            examine_result(job)))
        return
    end
    if not view.ready then
        job.stage_ticks = 0
        return
    end
    if job.confirmed then
        -- Back on the turntable with the book unchanged: the angle missed.
        conclude(bot, job, false, string.format("the book stayed shut at yaw 0x%03X",
            view.yaw))
        return
    end
    local yaw = view.yaw % BotConst.FULL_TURN
    if yaw >= EXAMINE_YAW_LOW and yaw <= EXAMINE_YAW_HIGH then
        if job.turning then
            -- One release tick, so the confirm frame turns no further.
            job.turning = false
            return
        end
        if pulse(bot, job, "confirm") then
            job.confirmed = true
            log(bot, string.format("%s: confirm at yaw 0x%03X", job.label, yaw))
        end
        return
    end
    job.turning = true
    -- Left raises the yaw; right lowers it. Turn the short way round to the
    -- band's middle.
    local middle = (EXAMINE_YAW_LOW + EXAMINE_YAW_HIGH) // 2
    local ahead = (middle - yaw) % BotConst.FULL_TURN
    local key = ahead < BotConst.FULL_TURN // 2 and "left" or "right"
    bot:set_input({ [key] = true })
end

-- Run one tick of the active job. Returns true when the job owns the tick.
function BotMenu.step(bot)
    local job = BOT_MENU.job
    local menu = bot:menu()
    if not job then
        -- An inventory panel left open with nothing over it (no pick-up
        -- viewer, item box or message) is closed with cancel.
        if menu.open and menu.phase == "input" and menu.screen == nil then
            BOT_MENU.stray_ticks = BOT_MENU.stray_ticks + 1
            if BOT_MENU.stray_ticks > 30 then
                if BOT_MENU.stray_ticks % 2 == 0 then
                    bot:set_input({ cancel = true })
                end
                return true
            end
        else
            BOT_MENU.stray_ticks = 0
        end
        return false
    end
    job.ticks = job.ticks + 1
    job.stage_ticks = job.stage_ticks + 1
    local tick = bot:tick()

    -- A pick-up viewer (or item box) came up instead of the bare panel: a
    -- zone press landed as the job pressed START. The job cannot drive that
    -- screen; give the tick back to the scene handler and fail the job, so
    -- the route asks again once the viewer is gone.
    if menu.screen == "pickup" or menu.screen == "item_box" then
        if job.ok == nil then
            conclude(bot, job, false, "a " .. menu.screen .. " screen opened over the menu")
        end
        finish(bot, job)
        return false
    end

    if job.ticks > JOB_TIMEOUT and job.stage ~= "settle" then
        if job.ok == nil then
            conclude(bot, job, false, "job timed out; " .. describe(menu))
        end
        if not menu.open then
            set_stage(job, "settle")
        elseif job.ticks > JOB_TIMEOUT + RETRY_COOLDOWN then
            -- The panel would not close: give the tick back to the route
            -- with the failure on record rather than holding it forever.
            log(bot, string.format("%s: menu still open after the timeout; %s",
                job.label, describe(menu)))
            finish(bot, job)
            return false
        end
    end

    if job.stage == "open" then
        if menu.open or menu.requested then
            set_stage(job, "wait_input")
            return true
        end
        -- A scene that holds the controls (513's entry unequips the gun
        -- and plays the pod breaking) ignores START: give the tick to the
        -- scene handler, which may skip it, without pressing or counting;
        -- JOB_TIMEOUT still bounds the job.
        if not bot:scene().controllable then
            job.stage_ticks = 0
            return false
        end
        if job.stage_ticks > OPEN_TIMEOUT then
            local scene = bot:scene()
            local player = bot:player()
            job.ok = false
            job.reason = string.format(
                "START did not open the menu (controllable=%s attacked=%d message=%s)",
                tostring(scene.controllable), player.being_attacked or 0,
                tostring(scene.message))
            log(bot, string.format("%s: %s 0x%02X FAILED (%s)", job.label, job.kind,
                job.item, job.reason))
            finish(bot, job)
            return false
        end
        if tick - job.last_start >= START_GAP and pulse(bot, job, "start") then
            job.last_start = tick
        end
        return true
    end

    if job.stage == "settle" then
        -- Wait for the close fade and the room's post-menu freeze without
        -- pressing anything: a press buffered there reaches the room tick.
        if menu.open or menu.requested then
            job.stage_ticks = 0
            return true
        end
        local scene = bot:scene()
        local other_scene = scene.film or scene.transition or scene.skippable
            or scene.cutscene_skip or scene.pickup or scene.message_takes_input
        if scene.controllable or other_scene or job.stage_ticks > SETTLE_TIMEOUT then
            finish(bot, job)
            return false
        end
        return true
    end

    if not menu.open then
        if menu.requested then
            return true
        end
        -- The menu closed: by our cancel, or by itself after a key item use.
        if job.ok == nil then
            if result_seen(bot, job) then
                conclude(bot, job, true, "menu closed after the effect")
            elseif job.kind == "use" and job.stage == "result" then
                -- A key item used at its zone closes the menu itself; an
                -- uncounted one (the cranks, quantity 0) stays held.
                conclude(bot, job, true, "menu closed by the use")
            else
                conclude(bot, job, false, "menu closed before the " .. wanted_option(job))
            end
        end
        set_stage(job, "settle")
        return true
    end

    if menu.phase == "message" then
        local scene = bot:scene()
        if job.kind == "combine" and job.stage == "result" and scene.prompt then
            -- A herb mix asks first: right selects Yes, then a fresh edge
            -- confirms it.
            if pulse(bot, job, scene.prompt_choice == "yes" and "action" or "right") then
                -- The mix applies a few ticks after the answer.
                job.stage_ticks = 0
            end
            return true
        end
        -- A refusal ("You can't use it here") or a prompt: dismiss it with
        -- the action button, like the message window expects.
        -- A combine's prompt text is no refusal: its result stage decides.
        if job.stage == "result" and job.ok == nil and job.kind ~= "combine"
            and not result_seen(bot, job) then
            job.refused = true
        end
        pulse(bot, job, "action")
        return true
    end
    if job.refused and job.ok == nil then
        conclude(bot, job, false, string.format("the menu refused the %s",
            wanted_option(job)))
    end
    if job.kind == "examine" and menu.screen == "examine" then
        drive_examine(bot, job, menu)
        return true
    end
    if job.kind == "examine" and job.stage == "view" and menu.phase == "screen" then
        -- The CHECK slide runs before the viewer starts.
        return true
    end
    if menu.phase == "screen" then
        -- A sub-screen the job did not ask for (CHECK viewer, map): back out.
        pulse(bot, job, "cancel")
        return true
    end
    if not menu.accepts_input then
        -- Fades and the open delay take no input.
        return true
    end
    if job.stage == "wait_input" then
        set_stage(job, "cursor")
    end
    if job.stage_ticks > STEP_TIMEOUT and job.stage ~= "close" and job.ok == nil then
        conclude(bot, job, false, string.format("stage %s stalled; %s", job.stage,
            describe(menu)))
    end
    drive(bot, job, menu)
    return true
end
