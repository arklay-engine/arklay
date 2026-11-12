-- Brewing at a drug store, from the room's facts:
--   <tap slot item label>             AOT `slot`'s event fills one held
--                                     empty bottle with `item` (Yes at
--                                     its prompt)
--   <brew item bottle bank bit [after_bank after_bit after_while] label>
--                                     make `item` here; done once the
--                                     flag is set or `item` is held
-- The recipe tree comes from the engine's combine table (bot:recipe),
-- searched outward from the tap items: a MOVE of item a onto item b that
-- leaves an empty `bottle` in a's slot makes what b's slot becomes. A
-- wrong mix (both slots turn into empty bottles) is never in the tree.
-- Each decision plans afresh from what is held, so a stray chemical in
-- the inventory is used where the tree needs it.
-- The facts carry every item id; this module holds none.

BotMix = BotMix or {}

local function meta(bot)
    return bot:room_meta() or {}
end

-- Whether a <brew> fact still wants its item made: not done, the item
-- not held, and its `after` flag (if any) in the state it waits for.
function BotMix.open(bot, fact)
    if bot:flag(fact.bank, fact.bit) or bot:has_item(fact.item) then
        return false
    end
    if fact.after_bank ~= nil then
        local set = bot:flag(fact.after_bank, fact.after_bit)
        if set == (fact.after_clear == true) then
            return false
        end
    end
    return true
end

-- The open <brew> facts of this room, in document order.
function BotMix.brews(bot)
    local out = {}
    for _, fact in ipairs(meta(bot).brews or {}) do
        if BotMix.open(bot, fact) then
            out[#out + 1] = fact
        end
    end
    return out
end

-- Whether AOT `slot` is one of this room's taps.
function BotMix.is_tap(bot, slot)
    for _, tap in ipairs(meta(bot).taps or {}) do
        if tap.slot == slot then
            return true
        end
    end
    return false
end

-- Every way the taps (this room's, or `taps`) lead to each item, breadth
-- first: made[item] = a list of {tap = <tap>} or {a = item, b = item}
-- (MOVE a onto b), the shallowest first. An item reached in a round keeps
-- every pair of that round that makes it, so the plan can pick the one
-- that uses what is held.
-- The tree is the same for a fact and its taps on every call (the engine's
-- combine table does not change): it is built once per brew and tap set,
-- since the inventory rules ask for it every tick.
local recipe_cache = {}
function BotMix.recipes(bot, fact, taps)
    taps = taps or meta(bot).taps or {}
    local key = { tostring(fact.item), tostring(fact.bottle) }
    for _, tap in ipairs(taps) do
        key[#key + 1] = tostring(tap.slot) .. "=" .. tostring(tap.item)
    end
    key = table.concat(key, ",")
    local cached = recipe_cache[key]
    if cached then
        return cached
    end
    local made = BotMix.build_recipes(bot, fact, taps)
    recipe_cache[key] = made
    return made
end

function BotMix.build_recipes(bot, fact, taps)
    local made, order = {}, {}
    for _, tap in ipairs(taps) do
        if not made[tap.item] then
            made[tap.item] = { { tap = tap } }
            order[#order + 1] = tap.item
        end
    end
    local grew = true
    while grew and not made[fact.item] do
        grew = false
        local known = {}
        for i, item in ipairs(order) do
            known[i] = item
        end
        local fresh = {}
        for _, a in ipairs(known) do
            for _, b in ipairs(known) do
                local r = a ~= b and bot:recipe(a, b)
                if r and r.cursor == fact.bottle and r.target ~= fact.bottle
                    and (not made[r.target] or fresh[r.target]) then
                    if not made[r.target] then
                        made[r.target] = {}
                        fresh[r.target] = true
                        order[#order + 1] = r.target
                        grew = true
                    end
                    local list = made[r.target]
                    list[#list + 1] = { a = a, b = b }
                end
            end
        end
    end
    return made
end

-- The items the open brews of any room need held: their bottles and
-- every chemical of their recipe trees (item -> true). The inventory
-- keeps them, and a brew room's bottles count as needed pickups, so a box
-- makes room for them all in one trip.
function BotMix.needs(bot)
    local out = {}
    for _, fact in ipairs(bot:brew_facts()) do
        if BotMix.open(bot, fact) then
            out[fact.bottle] = true
            for item, _ in pairs(BotMix.recipes(bot, fact, fact.taps)) do
                out[item] = true
            end
        end
    end
    return out
end

-- The most bottles a brew holds at once, made from nothing held: the
-- slots it takes (a mix leaves its cursor bottle empty, so bottles are
-- never used up).
local peak_cache = {}
local function peak_bottles(bot, fact, made)
    -- Planned from nothing held, so the same for a tree every time.
    if peak_cache[made] then
        return peak_cache[made]
    end
    local ops = {}
    BotMix.plan(made, fact.item, {}, ops)
    local held, peak = 0, 0
    for _, op in ipairs(ops) do
        if op.kind == "fill" then
            held = held + 1
        else
            held = held - 1
        end
        if held > peak then
            peak = held
        end
    end
    peak_cache[made] = peak
    return peak
end

-- The free slots the open brews of any room still need: each one's
-- bottle peak less the bottles and recipe chemicals already held. With
-- `gated`, only the brews an `after` flag opened: a story step running now
-- (Rebecca's V-JOLT once Plant 42 has Chris), not one open all game.
function BotMix.slots_needed(bot, gated)
    local need = 0
    for _, fact in ipairs(bot:brew_facts()) do
        if BotMix.open(bot, fact) and (not gated or fact.after_bank ~= nil) then
            local made = BotMix.recipes(bot, fact, fact.taps)
            local held = BotMix.count(bot, fact.bottle)
            for item, _ in pairs(made) do
                held = held + BotMix.count(bot, item)
            end
            local short = peak_bottles(bot, fact, made) - held
            if short > 0 then
                need = need + short
            end
        end
    end
    return need
end

local function copy(t)
    local out = {}
    for k, v in pairs(t) do
        out[k] = v
    end
    return out
end

local function append(ops, more)
    for _, op in ipairs(more) do
        ops[#ops + 1] = op
    end
end

local function fills(ops)
    local n = 0
    for _, op in ipairs(ops) do
        if op.kind == "fill" then
            n = n + 1
        end
    end
    return n
end

-- The most chemicals `ops` hold at once, beyond the stock: the bottles
-- they need (a fill fills one, a mix leaves one of its two empty).
local function peak(ops)
    local held, most = 0, 0
    for _, op in ipairs(ops) do
        held = held + (op.kind == "fill" and 1 or -1)
        if held > most then
            most = held
        end
    end
    return most
end

-- The cheapest way to make `item` from the stock in `have` (item ->
-- count): its ops and the stock left, or nil when the taps cannot lead to
-- it. Cheapest is the fewest tap fills, then the fewest bottles at once,
-- then the fewest ops; each alternative recipe is tried on its own copy of
-- the stock, with either half made first. Making the half that is mixed
-- last, last, keeps a bottle free: V-JOLT is UMB No.13 and NP-003, and
-- NP-003 made first sat in a bottle while UMB No.13 took three more, one
-- more than Rebecca's free slots in Chris's game. `depth` bounds the
-- search (the tree is a handful of chemicals deep).
local function stock_key(have)
    local keys = {}
    for k, v in pairs(have) do
        if v > 0 then
            keys[#keys + 1] = tostring(k) .. "x" .. tostring(v)
        end
    end
    table.sort(keys)
    return table.concat(keys, ",")
end

local cheapest
local function cheapest_memo(made, item, have, depth, memo)
    local key = string.format("%d|%d|%s", item, depth, stock_key(have))
    local hit = memo[key]
    if hit then
        return hit[1], hit[2]
    end
    local ops, left = cheapest(made, item, have, depth, memo)
    memo[key] = { ops, left }
    return ops, left
end

cheapest = function(made, item, have, depth, memo)
    if (have[item] or 0) > 0 then
        local left = copy(have)
        left[item] = left[item] - 1
        return {}, left
    end
    if depth <= 0 then
        return nil
    end
    local best, best_left
    for _, how in ipairs(made[item] or {}) do
        local ops, left
        local tries = {}
        if how.tap then
            tries[1] = { { { kind = "fill", tap = how.tap, item = item } }, have }
        else
            for _, order in ipairs({ { how.a, how.b }, { how.b, how.a } }) do
                local first_ops, after_first = cheapest_memo(made, order[1], have, depth - 1, memo)
                local second_ops, after_second
                if first_ops then
                    second_ops, after_second = cheapest_memo(made, order[2], after_first,
                        depth - 1, memo)
                end
                if second_ops then
                    local both = {}
                    append(both, first_ops)
                    append(both, second_ops)
                    both[#both + 1] = { kind = "mix", a = how.a, b = how.b, item = item }
                    tries[#tries + 1] = { both, after_second }
                end
            end
        end
        for _, try in ipairs(tries) do
            ops, left = try[1], try[2]
            local better = not best or fills(ops) < fills(best)
            if not better and fills(ops) == fills(best) then
                better = peak(ops) < peak(best)
                    or (peak(ops) == peak(best) and #ops < #best)
            end
            if better then
                best, best_left = ops, left
            end
        end
    end
    return best, best_left
end

-- V-JOLT's tree is five chemicals deep; deeper only walks the recipes'
-- cycles (UMB No.4 from NP-003 and water, NP-003 from water and UMB No.2).
local PLAN_DEPTH = 6

-- Append to `ops` what making `item` takes beyond the stock in `have`
-- (item -> count; the plan's claims are taken out of it). False when the
-- taps cannot lead to it.
function BotMix.plan(made, item, have, ops)
    local best, left = cheapest_memo(made, item, have, PLAN_DEPTH, {})
    if not best then
        return false
    end
    append(ops, best)
    for k, _ in pairs(have) do
        have[k] = left[k]
    end
    return true
end

-- How many inventory slots hold `item` (a chemical stack holds one).
function BotMix.count(bot, item)
    local n = 0
    for _, row in ipairs(bot:inventory()) do
        if row.item == item then
            n = n + 1
        end
    end
    return n
end

-- Log why a brew cannot go on, once per reason.
local noted = {}
function BotMix.note(bot, fact, why)
    local key = string.format("%d:%s", fact.item, tostring(why))
    if noted[fact.item] ~= key then
        noted[fact.item] = key
        bot:log(string.format("brew 0x%02X waits: %s", fact.item, tostring(why)))
    end
end

-- The menu rows holding `item`, but not `skip`'s slot.
local function row_of(bot, item, skip)
    for _, row in ipairs(bot:menu().items or {}) do
        if row.item == item and row.slot ~= skip then
            return row
        end
    end
    return nil
end

-- The next step of a <brew>: {kind = "fill", tap, item} (press the tap
-- holding an empty bottle) or {kind = "mix", slot, target, a, b, item}
-- (0-based menu slots: MOVE a onto b). nil and the reason when the brew
-- cannot go on now: no empty bottle held, or no tap leads to the item.
function BotMix.next_step(bot, fact)
    local made = BotMix.recipes(bot, fact)
    if not made[fact.item] then
        return nil, string.format("no recipe from this room's taps makes 0x%02X", fact.item)
    end
    local have = {}
    for _, row in ipairs(bot:inventory()) do
        if row.item ~= 0 and row.item ~= fact.bottle then
            have[row.item] = (have[row.item] or 0) + 1
        end
    end
    local ops = {}
    BotMix.plan(made, fact.item, have, ops)
    if not ops[1] then
        return nil, "nothing left to make"
    end
    -- A mix whose two chemicals are held goes first: it moves the brew on
    -- and empties a bottle for the next fill.
    for _, op in ipairs(ops) do
        if op.kind == "mix" then
            local a = row_of(bot, op.a)
            local b = a and row_of(bot, op.b, a.slot)
            if a and b then
                op.slot, op.target = a.slot, b.slot
                return op
            end
        end
    end
    for _, op in ipairs(ops) do
        if op.kind == "fill" then
            if not bot:has_item(fact.bottle) then
                return nil, "no empty bottle held"
            end
            return op
        end
    end
    return nil, "the mix's chemicals are not both held"
end

return BotMix
