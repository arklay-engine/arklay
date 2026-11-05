-- Room puzzle facts for the planner, read from the current room's bot.xml
-- (format 2) through bot:room_meta():
--   <interact slot bank bit label>  press AOT `slot`; its flag set = done
--   <use slot item bank bit label>  stand in AOT `slot`'s use-item zone and
--                                   USE `item` from the menu; flag = done
--   <answer choice slot|bank+bit>   the yes/no prompt answer
--   <avoid slot reason>             never press AOT `slot`
-- The facts carry every room number; this module holds none.

BotPuzzle = BotPuzzle or {}

local function meta(bot)
    return bot:room_meta() or { interacts = {}, uses = {}, answers = {}, avoids = {} }
end

-- The <use> facts that apply now, in document order: the item is held, the
-- done flag is clear, and the slot is a live use-item probe (a room deletes
-- the probe until its step applies, as 10F does before the gold pickup).
function BotPuzzle.uses(bot)
    local out = {}
    for _, fact in ipairs(meta(bot).uses or {}) do
        local action = BotNav.action_by_slot(bot, fact.slot)
        if bot:has_item(fact.item) and not bot:flag(fact.bank, fact.bit)
            and action and action.kind == "flag_bank_set" then
            out[#out + 1] = { fact = fact, action = action }
        end
    end
    return out
end

-- Whether AOT `slot` is named by any fact of this room.
function BotPuzzle.is_fact_slot(bot, slot)
    local m = meta(bot)
    for _, list in ipairs({ m.interacts, m.uses, m.answers, m.avoids }) do
        for _, fact in ipairs(list or {}) do
            if fact.slot == slot then
                return true
            end
        end
    end
    return false
end

-- The avoid reason for `slot` in this room, or nil.
function BotPuzzle.avoided(bot, slot)
    for _, avoid in ipairs(meta(bot).avoids or {}) do
        if avoid.slot == slot then
            return avoid.reason
        end
    end
    return nil
end

-- The next <interact> in document order, with its position in the list, or
-- nil when the sequence is done. Interacts are a sequence, and a later step
-- may clear an earlier step's flag (a switch chain that keeps only the last
-- bit), so the sequence position is the furthest step whose flag was ever
-- seen set; the next step is the first one after it whose flag is clear.
--
-- A <camera> step without a flag has nothing to persist: it counts as done
-- once its cut was entered during the current room entry (see
-- BotPuzzle.visited), so a later entry walks it again.
function BotPuzzle.next_interact(bot)
    local interacts = meta(bot).interacts or {}
    BOT_STATE.puzzle = BOT_STATE.puzzle or {}
    local room = bot:room().id
    local done = BOT_STATE.puzzle[room] or 0
    for index, fact in ipairs(interacts) do
        if index > done and fact.bank and bot:flag(fact.bank, fact.bit) then
            done = index
        end
    end
    BOT_STATE.puzzle[room] = done
    local visits = BOT_STATE.puzzle_visits or {}
    for index = done + 1, #interacts do
        local fact = interacts[index]
        local finished
        if fact.bank then
            finished = bot:flag(fact.bank, fact.bit)
        else
            finished = visits[index] == true
        end
        if not finished then
            return fact, index, #interacts
        end
    end
    return nil, #interacts, #interacts
end

-- Record that the flagless step `index` of this room was done this entry.
function BotPuzzle.visited(index)
    BOT_STATE.puzzle_visits = BOT_STATE.puzzle_visits or {}
    BOT_STATE.puzzle_visits[index] = true
end

-- A new room entry forgets the flagless steps.
function BotPuzzle.enter_room()
    BOT_STATE.puzzle_visits = {}
end

-- Record the slot the planner just pressed, so a prompt it opens can be
-- matched to an <answer slot=...>.
function BotPuzzle.pressed(bot, slot)
    BOT_STATE.last_press = { room = bot:room().id, slot = slot, tick = bot:tick() }
end

-- The answer for the prompt on screen: "yes", "no" or nil (no fact).
function BotPuzzle.answer(bot)
    local last = BOT_STATE.last_press
    local room = bot:room().id
    for _, fact in ipairs(meta(bot).answers or {}) do
        local slot_ok = fact.slot == nil
            or (last and last.room == room and last.slot == fact.slot)
        local flag_ok = fact.bank == nil or bot:flag(fact.bank, fact.bit)
        if slot_ok and flag_ok then
            return fact.choice
        end
    end
    return nil
end
