-- Room puzzle facts for the planner, read from the current room's bot.xml
-- (format 2) through bot:room_meta():
--   <interact slot bank bit label>  press AOT `slot`; its flag set = done
--   <use slot item bank bit label>  stand in AOT `slot`'s use-item zone and
--                                   USE `item` from the menu; flag = done
--   <answer choice slot|bank+bit>   the yes/no prompt answer
--   <avoid slot reason>             never press AOT `slot`
--   <avoid slot bank bit while reason>
--                                   not while the flag is `while` ("set" or
--                                   "clear"): a slot a scene blocks for now
--   <avoid slot unless_held reason> not while item `unless_held` is held
--                                   (a swap: lift one item while holding
--                                   the one that goes in its place); it
--                                   combines with the flag gate
--   <climb object slot bank bit [done] label>
--                                   push object `object` under AOT `slot`'s
--                                   item zone (a high shelf), climb it, take
--                                   the item, climb down; the item flag
--                                   (done="clear" for bank 7) = done
--   <watch bank bit [after_bank after_bit] label>
--                                   let a scene play, not skip it, while
--                                   the flag is clear (and the `after` flag
--                                   set): a scene a skip would break
-- The facts carry every room number; this module holds none.

BotPuzzle = BotPuzzle or {}

local function meta(bot)
    return bot:room_meta() or { interacts = {}, uses = {}, answers = {}, avoids = {} }
end

-- A <use> fact's done flag reads done: set, or clear with done="clear".
function BotPuzzle.use_done(bot, fact)
    return bot:flag(fact.bank, fact.bit) ~= (fact.done_clear == true)
end

-- A <use> fact is open: not done, its `after` flag (if any) set and its
-- `until` flag (if any) clear.
function BotPuzzle.use_open(bot, fact)
    return not BotPuzzle.use_done(bot, fact)
        and (fact.after_bank == nil or bot:flag(fact.after_bank, fact.after_bit))
        and (fact.until_bank == nil or not bot:flag(fact.until_bank, fact.until_bit))
end

-- The <use> facts that apply now, in document order: the item is held, the
-- done flag is clear, and the slot is a live use-item probe (a room deletes
-- the probe until its step applies, as 10F does before the gold pickup).
function BotPuzzle.uses(bot)
    local out = {}
    for _, fact in ipairs(meta(bot).uses or {}) do
        local action = BotNav.action_by_slot(bot, fact.slot)
        if bot:has_item(fact.item) and BotPuzzle.use_open(bot, fact)
            and action and action.kind == "flag_bank_set" then
            out[#out + 1] = { fact = fact, action = action }
        end
    end
    return out
end

-- Whether two zones {x, z, width, depth} overlap (edges count).
function BotPuzzle.zones_overlap(a, b)
    return a.x <= b.x + b.width and b.x <= a.x + a.width
        and a.z <= b.z + b.depth and b.z <= a.z + a.depth
end

-- The <use> fact of this room that waits only on its probe, or nil: the
-- item is held and the done flag clear, the slot is not a live use-item
-- probe yet, but its zone is here and overlaps `zone`, where the last step
-- acted. That step's room event may still be arming it (116's rack turns
-- AOT 5 into the probe once the lift its pickup started has played). A
-- probe elsewhere in the room waits on some other step. With `anywhere`
-- (just after entering the room) any probe here counts: the room's main
-- script may re-arm it a few ticks after the init set it as something else
-- (40A's bookshelf, an event in the init and the red book's probe in main).
function BotPuzzle.use_pending(bot, zone, anywhere)
    if not zone and not anywhere then
        return nil
    end
    for _, fact in ipairs(meta(bot).uses or {}) do
        local action = BotNav.action_by_slot(bot, fact.slot)
        if bot:has_item(fact.item) and BotPuzzle.use_open(bot, fact)
            and action and action.kind ~= "flag_bank_set"
            and (anywhere or BotPuzzle.zones_overlap(action, zone)) then
            return fact
        end
    end
    return nil
end

-- The key a sequence step's marks go under: its AOT slot, its camera cut,
-- its enemy for a <fight>, or its flag for a <wait>.
function BotPuzzle.step_key(fact)
    if fact.slot then
        return fact.slot
    end
    if fact.cut then
        return "camera" .. fact.cut
    end
    if fact.kind == "wait" then
        return string.format("wait%d:%d", fact.bank, fact.bit)
    end
    return "fight" .. tostring(fact.enemy)
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

-- Whether a gated <avoid> still holds: the `unless_held` item is not held,
-- and its flag (if any) is in its `while` state. `gate` is {bank, bit, set,
-- unless_held}; flags and the inventory are global, so this is answerable
-- from any room.
function BotPuzzle.gate_holds(bot, gate)
    if gate.unless_held and bot:has_item(gate.unless_held) then
        return false
    end
    return gate.bank == nil or bot:flag(gate.bank, gate.bit) == gate.set
end

-- The avoid reason for `slot` in this room, or nil; a gated avoid counts
-- only while its gate holds, and its gate {bank, bit, set} is the second
-- result (nil for an avoid that always holds).
function BotPuzzle.avoided(bot, slot)
    for _, avoid in ipairs(meta(bot).avoids or {}) do
        if avoid.slot == slot then
            if avoid.bank == nil and avoid.unless_held == nil then
                return avoid.reason, nil
            end
            local gate = { bank = avoid.bank, bit = avoid.bit, set = avoid["while"] == "set",
                unless_held = avoid.unless_held }
            if BotPuzzle.gate_holds(bot, gate) then
                return avoid.reason, gate
            end
        end
    end
    return nil
end

-- The <watch> fact that holds now, or nil: its done flag is clear and its
-- `after` flag, if any, is set. The scene handler lets a skippable scene
-- play while one holds.
function BotPuzzle.watching(bot)
    for _, fact in ipairs(meta(bot).watches or {}) do
        if not bot:flag(fact.bank, fact.bit)
            and (fact.after_bank == nil or bot:flag(fact.after_bank, fact.after_bit)) then
            return fact
        end
    end
    return nil
end

-- Whether a step's done flag reads done: set, or clear for done="clear"
-- (bank 7's item bits clear when the item is taken). A step that names
-- the item slot it `reveals` is also done once that slot is a live item
-- zone: the item frontier takes it from there (40C's fireplace, which
-- turns slot 3 into the helmet key).
function BotPuzzle.flag_done(bot, fact)
    if fact.reveals then
        local action = BotNav.action_by_slot(bot, fact.reveals)
        if action and action.kind == "item" then
            return true
        end
    end
    return bot:flag(fact.bank, fact.bit) ~= (fact.done_clear == true)
end

-- The next <interact> in document order, with its position in the list, or
-- nil when the sequence is done. Interacts are a sequence, and a later step
-- may clear an earlier step's flag (a switch chain that keeps only the last
-- bit), so the sequence position is the furthest step whose flag was ever
-- seen set; the next step is the first one after it whose flag is clear.
-- A <push> flag is no sequence mark, only one more way the step reads done
-- (40D's crates each set or clear the same two bits), and a push done on
-- this room entry stays done for the entry: a later push may move the
-- object out of an earlier step's band (40D's third crate, lined up with
-- the ladder corridor and then pushed east).
--
-- A step with an `after` flag that is still clear holds the sequence: no
-- step is offered until the flag is set.
--
-- A <camera> step without a flag has nothing to persist: it counts as done
-- once its cut was entered during the current room entry (see
-- BotPuzzle.visited), so a later entry walks it again.
function BotPuzzle.next_interact(bot)
    local interacts = meta(bot).interacts or {}
    BOT_STATE.puzzle = BOT_STATE.puzzle or {}
    local room = bot:room().id
    local done = BOT_STATE.puzzle[room] or 0
    -- A climb is not done while the player still stands on the object.
    local on_top = BotNav.on_top(bot)
    for index, fact in ipairs(interacts) do
        -- A reveal lasts only for the room entry (119's switch hides its
        -- book again on the next one), so only the flag marks the sequence.
        if index > done and fact.bank and fact.kind ~= "push"
            and bot:flag(fact.bank, fact.bit) ~= (fact.done_clear == true)
            and not (fact.kind == "climb" and on_top) then
            done = index
        end
    end
    BOT_STATE.puzzle[room] = done
    if (BOT_STATE.pushed or 0) > done then
        done = BOT_STATE.pushed
    end
    local visits = BOT_STATE.puzzle_visits or {}
    for index = done + 1, #interacts do
        local fact = interacts[index]
        local finished
        if fact.kind == "push" then
            finished = BotPuzzle.push_done(bot, fact)
        elseif fact.kind == "climb" then
            finished = BotPuzzle.flag_done(bot, fact) and not on_top
        elseif fact.bank then
            finished = BotPuzzle.flag_done(bot, fact)
        else
            finished = visits[index] == true
        end
        if not finished then
            -- A step behind an `after` flag waits for it; the sequence holds
            -- (a <use> may set it, as 40A's red book moves the bookcase).
            if fact.after_bank and not bot:flag(fact.after_bank, fact.after_bit) then
                return nil, index - 1, #interacts
            end
            return fact, index, #interacts
        end
    end
    return nil, #interacts, #interacts
end

-- A <push> step is done while its object's centre sits in the middle half
-- of its zone (the room's object probe tests the object's position). A push
-- with a direction is done once the object has cleared the zone that way
-- (past its far edge with clear="full"; with until="stop" only once the push
-- stalls against a wall, which the planner records),
-- or once the zone is gone or its item was taken (the item it hid). A push
-- with a flag is also done once the flag is set.
function BotPuzzle.push_dir(fact)
    if fact.direction then
        return { x = fact.dx, z = fact.dz, full = fact.clear_full == true,
            until_stop = fact.until_stop == true }
    end
    return nil
end

-- Whether the world model saw the item of this room's `slot` taken.
local function item_taken(bot, slot)
    local room = bot:room().id
    for _, r in ipairs(bot:world().rooms) do
        if r.id == room then
            for _, item in ipairs(r.items) do
                if item.slot == slot then
                    return item.taken
                end
            end
        end
    end
    return false
end

function BotPuzzle.push_done(bot, fact)
    -- A push whose zone event takes the object away (a statue that falls)
    -- names the event's flag: set means done.
    if fact.bank and bot:flag(fact.bank, fact.bit) then
        return true
    end
    local dir = BotPuzzle.push_dir(fact)
    local object = BotNav.object_by_slot(bot, fact.object)
    local action = BotNav.action_by_slot(bot, fact.slot)
    if dir and (not action or (action.kind == "item" and item_taken(bot, fact.slot))) then
        return true
    end
    if not object or not action then
        return false
    end
    if dir and dir.until_stop then
        return false
    end
    local ex, ez = BotNav.push_error(object, action, dir, fact.axis)
    return ex == 0 and ez == 0
end

-- Record that the flagless step `index` of this room was done this entry.
function BotPuzzle.visited(index)
    BOT_STATE.puzzle_visits = BOT_STATE.puzzle_visits or {}
    BOT_STATE.puzzle_visits[index] = true
end

-- Record that push step `index` of this room finished on this entry.
function BotPuzzle.pushed(_bot, index)
    if index and index > (BOT_STATE.pushed or 0) then
        BOT_STATE.pushed = index
    end
end

-- A new room entry forgets the flagless steps and the pushes done.
function BotPuzzle.enter_room()
    BOT_STATE.puzzle_visits = {}
    BOT_STATE.pushed = nil
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

-- Item errands (<deliver>/<fetch>, bot:errand_facts(), every room). A
-- <deliver> is wanted while its done flag is clear and its `after` flag, if
-- any, is set; a <fetch> offers its item while its flag is set.
function BotPuzzle.deliver_wanted(bot, fact)
    return fact.kind == "deliver" and not bot:flag(fact.bank, fact.bit)
        and (fact.after_bank == nil or bot:flag(fact.after_bank, fact.after_bit))
end

function BotPuzzle.fetch_offered(bot, fact)
    return fact.kind == "fetch" and bot:flag(fact.bank, fact.bit)
end

-- Every room's errand facts (none for a test double without them).
function BotPuzzle.errands(bot)
    return bot.errand_facts and bot:errand_facts() or {}
end

-- The items a wanted <deliver> needs, {[item] = the deliver fact}, and a
-- <cure> for a poison the player has while none is held, {[item] = {cure}}
-- (the second serum on 100's shelf after Yawn's bite).
function BotPuzzle.wanted_items(bot)
    local out = {}
    for _, fact in ipairs(BotPuzzle.errands(bot)) do
        if BotPuzzle.deliver_wanted(bot, fact) then
            out[fact.item] = out[fact.item] or fact
        end
    end
    local player = bot:player()
    local cures = (bot:knowledge() or {}).cures or {}
    local held = false
    for _, cure in ipairs(cures) do
        if BotPuzzle.cures(cure, player) and bot:has_item(cure.item) then
            held = true
        end
    end
    if not held then
        for _, cure in ipairs(cures) do
            if BotPuzzle.cures(cure, player) then
                out[cure.item] = out[cure.item] or { cure = cure }
            end
        end
    end
    return out
end

-- Whether <cure> row `cure` clears a poison `player` has.
function BotPuzzle.cures(cure, player)
    return (cure.poison == "poison" and player.poisoned == true)
        or (cure.poison == "serious" and player.seriously_poisoned == true)
end

-- The <fetch> facts worth doing now: on offer, for an item a wanted
-- <deliver> needs and the bot does not hold.
function BotPuzzle.fetches_due(bot)
    local wanted = BotPuzzle.wanted_items(bot)
    local out = {}
    for _, fact in ipairs(BotPuzzle.errands(bot)) do
        if wanted[fact.item] and not bot:has_item(fact.item)
            and BotPuzzle.fetch_offered(bot, fact) then
            out[#out + 1] = fact
        end
    end
    return out
end
