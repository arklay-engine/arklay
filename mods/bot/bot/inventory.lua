-- Inventory management for the player bot: the item box and the combine.
--
-- Both go through the game's own screens with pad input. The item box is a
-- room action of kind "item_box" (the engine's room-action handler 0x08):
-- the planner walks into its zone and presses action, the lid opens and the
-- box screen comes up over the inventory panel (bot:menu().box). There the
-- bot moves the inventory cursor with the d-pad, confirms the slot (the
-- screen arms it), pages the box cursor up or down to the box slot and
-- confirms again: the engine swaps the two. Cancel closes the box and the
-- pause menu. The combine is the menu's MOVE (BotMenu.combine).
--
-- The policy (BotInventory.keep, .deposits, .withdrawals, .combine_pick)
-- works on plain tables so the unit tests can drive it:
--   keep:     the equipped item, a firearm the knowledge file fights with
--             and its ammunition (the ammo reserve), the loadout (below),
--             every key, every item
--             an open <use> fact needs (bot:use_facts(), done flag clear),
--             every heal item that restores health (the row's `heals`)
--             and one <cure> stack. Anything else may go in the box (a
--             red herb alone).
--   deposit:  when the inventory is at its limit and a known pickup
--             (a frontier item that merges into no held stack) needs room,
--             or, at a box in this room, when no more than RESERVE slots
--             are free and more pickups than that need one (the slot
--             reserve); then every item the keep rule does not hold goes in.
--   look:     before any box was opened, at a box in this room with a slot
--             free (or from anywhere while sourcing ammunition,
--             BotSurvival.sourcing): one visit to read what the shared
--             box holds;
--   withdraw: an item in the box that an open <use> fact needs, a key a
--             remembered locked door needs, the equipped gun's ammunition
--             below the reserve floor, or a heal item when none is held;
--             only into free slots. At a box in this room, also every
--             stack of that ammunition while the rounds are under TOP_UP
--             times the floor, and every heal stack (the heal top-up: a
--             boss such as 513's Tyrant is a race of heals), as long as
--             RESERVE slots stay free. First of all, from any box (a
--             trip is made for it), the weapon and ammunition a <fight>
--             ahead in this stage prefers (the loadout, the knowledge
--             file's `prefer`), into any free slot; so is the rearm: a
--             firearm in the box (not a `reserve` one) whose ammunition
--             is held, or in the box while sourcing ammunition, with that
--             ammunition (held, it stays).
--   combine:  only when the inventory is at its limit and a pickup needs
--             room: a pair whose MOVE (bot:combine_result) empties a slot,
--             of heal, ammo or weapon items only, never a key or an item
--             a <use> fact needs; heal pairs first, so herbs mix into
--             stronger heals that stay in the inventory.
-- What the box holds is remembered (bot:remember "box_items") each time its
-- screen is up, so the memory checkpoint a save keeps agrees with the
-- box the save holds. No room literals; only pad/UI input.

BotInventory = BotInventory or {}

local C = BotConst

local FIRST_SLOT_CURSOR = 8
-- One button pulse per this many ticks on the box screen, so each press is
-- a fresh edge and the screen's state is read back between presses.
local PRESS_GAP = 3
-- The box screen takes no input for its first few ticks (the menu reveal).
local OPEN_SETTLE = C.SECOND // 2
-- One deposit or withdrawal gives up after this long; the whole box visit
-- after the next.
local OP_TIMEOUT = 15 * C.SECOND
local SESSION_TIMEOUT = 60 * C.SECOND
-- After the box press, the lid animates before the screen comes up.
local LID_WAIT = 5 * C.SECOND
-- The close fade and the room's post-menu freeze.
local SETTLE_TIMEOUT = 4 * C.SECOND
-- Controllable ticks in a row that end a visit.
local CLEAR_TICKS = C.SECOND
-- Slots kept free for pickups at a box in this room (the slot reserve).
BotInventory.RESERVE = 1
-- At a box here, the equipped gun's ammunition is topped up under this many
-- times the reserve floor.
local TOP_UP = 2
-- The rounds of a weapon's own loading that leave its other loadings'
-- rounds (the launcher's acid and flame) out of the loadout.
local OWN_ROUNDS = 6
-- Kinds the combine may mix.
local MIX_KINDS = { heal = 1, ammo = 2, weapon = 2 }

local function state()
    BOT_STATE.inventory = BOT_STATE.inventory or {}
    return BOT_STATE.inventory
end

local function log(bot, text)
    bot:log("inventory: " .. text)
end

-- ---- policy (plain values) ---------------------------------------------

-- Why `row` ({item, kind, quantity}) stays in the inventory, or nil.
-- ctx: {equipped, guns = {[id] = true}, ammo = {[id] = true},
--       uses = {[id] = true}, cures = {[id] = true}}
function BotInventory.keep(row, ctx)
    local item = row.item
    if item == 0 then
        return "empty"
    end
    if row.kind == "key" then
        return "key"
    end
    if ctx.uses and ctx.uses[item] then
        return "a <use> fact needs it"
    end
    if item == ctx.equipped then
        return "equipped"
    end
    if row.kind == "weapon" and ctx.guns and ctx.guns[item] then
        return "firearm"
    end
    if row.kind == "ammo" and ctx.ammo and ctx.ammo[item] then
        return "ammo reserve"
    end
    if ctx.loadout and ctx.loadout[item] then
        return "a fight ahead prefers it"
    end
    if ctx.rearm and ctx.rearm[item] then
        return "its gun comes back out of the box"
    end
    if row.kind == "heal" and row.heals ~= false then
        return "heal"
    end
    if row.kind == "heal" and ctx.cures and ctx.cures[item] then
        return "cure"
    end
    return nil
end

-- The menu rows (bot:menu().items) the box should take, in slot order.
-- One <cure> stack is kept; a second one goes in.
function BotInventory.deposits(items, ctx)
    local out = {}
    local cure_kept = false
    for _, row in ipairs(items) do
        local why = row.item ~= 0 and BotInventory.keep(row, ctx)
        if why == "cure" then
            if cure_kept then
                why = nil
            end
            cure_kept = true
        end
        if row.item ~= 0 and not why then
            out[#out + 1] = row
        end
    end
    return out
end

-- Heal stacks a full inventory keeps when it must make room for a needed
-- pickup; the rest may go in the box (the "none held" withdrawal brings one
-- back).
BotInventory.SOFT_HEALS = 1

-- When nothing the keep rule lets go is held, the inventory is full and a
-- needed pickup (`needed`, a count: a key, an item a <use> fact needs, or
-- the ammunition a held gun lacks) has no slot, the softer keeps that may
-- go in, at most `needed` of them: first heal stacks past SOFT_HEALS (the
-- last in slot order; only for a key or <use> item, ctx.swap, or the
-- loadout, ctx.heals_may_go: ammunition alone is no reason to box a heal,
-- the first aid spray went in for 118's acid rounds and the bot met Yawn 2
-- with one herb), then a key no remembered locked door needs, then a
-- spare hand weapon (ctx.spare: held, not equipped, and no <fight> in this stage
-- prefers it), then, only to make room for a key or <use> item, boxed or
-- on the frontier (ctx.swap), a held firearm other than the equipped one
-- and the loadout, fewest rounds first (ctx.spare_guns: {[id] = rounds}),
-- the staple (the handgun, ctx.staple_guns) last.
function BotInventory.soft_deposits(items, ctx, needed)
    local out = {}
    if not ctx.full or (needed or 0) == 0 then
        return out
    end
    -- Spare heals first: a key whose door is not seen yet costs a trip back.
    local heals = {}
    for _, row in ipairs(items) do
        if row.item ~= 0 and BotInventory.keep(row, ctx) == "heal" then
            heals[#heals + 1] = row
        end
    end
    for i = #heals, BotInventory.SOFT_HEALS + 1, -1 do
        if not (ctx.swap or ctx.heals_may_go) then
            break
        end
        if #out >= needed then
            return out
        end
        out[#out + 1] = heals[i]
    end
    for _, row in ipairs(items) do
        if #out >= needed then
            return out
        end
        if row.item ~= 0 and BotInventory.keep(row, ctx) == "key"
            and not (ctx.keys and ctx.keys[row.item]) then
            out[#out + 1] = row
        end
    end
    for _, row in ipairs(items) do
        if #out >= needed then
            return out
        end
        if row.item ~= 0 and ctx.spare and ctx.spare[row.item] then
            out[#out + 1] = row
        end
    end
    local guns = {}
    for _, row in ipairs(ctx.swap and items or {}) do
        local rounds = row.item ~= 0 and ctx.spare_guns and ctx.spare_guns[row.item]
        if rounds and not (ctx.loadout and ctx.loadout[row.item])
            and not (ctx.uses and ctx.uses[row.item]) then
            guns[#guns + 1] = { row = row, rounds = rounds,
                staple = ctx.staple_guns and ctx.staple_guns[row.item] or false }
        end
    end
    table.sort(guns, function(a, b)
        if a.staple ~= b.staple then
            return b.staple
        end
        if a.rounds ~= b.rounds then
            return a.rounds < b.rounds
        end
        return a.row.slot < b.row.slot
    end)
    for _, gun in ipairs(guns) do
        if #out >= needed then
            return out
        end
        out[#out + 1] = gun.row
    end
    return out
end

-- The loadout withdrawal's reason: taken first, into any free slot, and
-- worth a trip to a remote box (a full inventory makes room for it the way
-- it does for a needed pickup).
local LOADOUT = "loadout: a fight ahead prefers it"
-- The rearm withdrawal's reason, handled like the loadout's: a firearm in
-- the box whose ammunition is held, or in the box while sourcing
-- ammunition (BotSurvival.sourcing_now), comes out with it.
local REARM = "rearm: a boxed gun's ammunition is at hand"
-- The cure withdrawal's reason (ctx.cure_now).
local CURE = "a cure for the poison"
BotInventory.CURE = CURE
-- The reasons that take any free slot and are worth a remote trip.
local FIRST = { [LOADOUT] = true, [REARM] = true, [CURE] = true }

-- The free slots a top-up withdrawal must leave.
local function slots_to_leave()
    return BotInventory.RESERVE
end

-- Why the box item `row` ({item, kind}) should come out, or nil.
-- ctx adds: keys = {[id] = true} (keys remembered locked doors need),
--           ammo_low = the equipped gun's ammunition below the floor,
--           heals = heal stacks held.
function BotInventory.wanted_back(row, ctx)
    local item = row.item
    if item == 0 then
        return nil
    end
    if ctx.cure_now and ctx.cure_now[item] then
        return CURE
    end
    if ctx.uses and ctx.uses[item] then
        return "a <use> fact needs it"
    end
    if ctx.keys and ctx.keys[item] then
        return "a locked door needs it"
    end
    if ctx.ammo_low and ctx.ammo_low == item then
        return "ammunition under the reserve floor"
    end
    if ctx.gun_ammo and ctx.gun_ammo[item] then
        return "ammunition for a held gun"
    end
    if ctx.loadout and ctx.loadout[item] then
        return LOADOUT
    end
    if ctx.rearm and ctx.rearm[item] then
        return REARM
    end
    if ctx.top_up and ctx.top_up == item and not ctx.reserve_only then
        return "ammunition top-up at the box"
    end
    if row.kind == "heal" and row.heals ~= false and (ctx.heals or 0) == 0 then
        return "no heal item held"
    end
    if row.kind == "heal" and row.heals ~= false and ctx.heal_top_up and not ctx.reserve_only then
        return "heal top-up at the box"
    end
    return nil
end

-- The top-ups at a box here take stacks only while RESERVE slots stay free.
local TOP_UPS = {
    ["ammunition top-up at the box"] = true,
    ["heal top-up at the box"] = true,
}

-- The box rows to take back, at most `free` of them, box order.
function BotInventory.withdrawals(box_items, ctx, free)
    local out = {}
    local heal_taken = false
    -- The loadout and the rearm first, so the heal top-up does not fill
    -- their slots.
    local rows = {}
    for _, row in ipairs(box_items) do
        if FIRST[BotInventory.wanted_back(row, ctx)] then
            rows[#rows + 1] = row
        end
    end
    for _, row in ipairs(box_items) do
        if not FIRST[BotInventory.wanted_back(row, ctx)] then
            rows[#rows + 1] = row
        end
    end
    for _, row in ipairs(rows) do
        if #out >= free then
            break
        end
        local why = BotInventory.wanted_back(row, ctx)
        -- A top-up takes the stacks only while the reserve stays free.
        if TOP_UPS[why] and free - #out <= slots_to_leave(why, ctx) then
            why = nil
        end
        -- One heal stack is enough to stop the "none held" rule.
        if why and not (why == "no heal item held" and heal_taken) then
            if why == "no heal item held" then
                heal_taken = true
            end
            out[#out + 1] = row
        end
    end
    return out
end

-- The slot reserve at a box here: deposit when no more than RESERVE slots
-- are free and more known pickups than that need one. ctx: {capacity,
-- used}; `deposits` the count the keep rule lets go; `pickups` the count
-- of known pickups needing a slot.
function BotInventory.reserve_due(ctx, deposits, pickups)
    local free = ctx.capacity - ctx.used
    return deposits > 0 and free <= BotInventory.RESERVE and pickups > free
end

-- The best MOVE that empties a slot: `{slot, target}` (0-based menu slots)
-- or nil. `result(slot, target)` is bot:combine_result.
-- Whether combining rows `a` and `b` loads a weapon with ammunition other
-- than its own (ctx.own_ammo: {[weapon] = ammo}): that turns it into
-- another item (the grenade launcher 0x07 with acid rounds is 0x08), which
-- the knowledge, the loadout and the rearm do not know as the weapon. The
-- empty launcher took 118's acid rounds to free a slot, and the bot then
-- boxed it and met 30C's Black Tiger without it.
local function loads_other(a, b, ctx)
    local own = ctx.own_ammo
    if not own then
        return false
    end
    local gun, ammo = a, b
    if b.kind == "weapon" then
        gun, ammo = b, a
    end
    return gun.kind == "weapon" and ammo.kind == "ammo" and own[gun.item] ~= ammo.item
end

function BotInventory.combine_pick(items, ctx, result)
    local best, best_rank = nil, nil
    for i, a in ipairs(items) do
        for j, b in ipairs(items) do
            if i ~= j and a.item ~= 0 and b.item ~= 0
                and MIX_KINDS[a.kind] and MIX_KINDS[b.kind]
                and not loads_other(a, b, ctx)
                and not (ctx.uses and (ctx.uses[a.item] or ctx.uses[b.item])) then
                local r = result(a.slot, b.slot)
                if r.frees and (r.result == "applied" or r.result == "herb") then
                    local rank = MIX_KINDS[a.kind] + MIX_KINDS[b.kind]
                    if not best_rank or rank < best_rank then
                        best, best_rank = { slot = a.slot, target = b.slot, item = a.item,
                            other = b.item }, rank
                    end
                end
            end
        end
    end
    return best
end

-- The box slot nearest `cursor` (forward or back, wrapping over `count`
-- slots) that satisfies `want(row)`, and the direction to page: +1 down
-- (next), -1 up (previous), 0 already there. nil when none does.
function BotInventory.box_target(rows, cursor, want)
    local count = #rows
    local best, best_dist, best_dir = nil, nil, 0
    for _, row in ipairs(rows) do
        if want(row) then
            local fwd = (row.slot - cursor) % count
            local back = (cursor - row.slot) % count
            local dist, dir = fwd, 1
            if back < fwd then
                dist, dir = back, -1
            end
            if dist == 0 then
                dir = 0
            end
            if not best_dist or dist < best_dist then
                best, best_dist, best_dir = row, dist, dir
            end
        end
    end
    return best, best_dir
end

-- ---- reads ---------------------------------------------------------------

local function used_slots(items)
    local n = 0
    for _, row in ipairs(items) do
        if row.item ~= 0 then
            n = n + 1
        end
    end
    return n
end

-- The policy context from the live state.
function BotInventory.context(bot)
    local ctx = { uses = {}, guns = {}, ammo = {}, keys = {}, cures = {}, heals = 0 }
    ctx.equipped = bot:player().equipped
    local knowledge = bot:knowledge()
    -- Poisoned with no cure for it held: the cures for it come out of the
    -- box first (ctx.cure_now; 408's wasps poisoned the bot and it walked
    -- 17k ticks poisoned with a blue herb boxed).
    local player = bot:player()
    local cure_for, cure_held = {}, false
    for _, cure in ipairs(knowledge and knowledge.cures or {}) do
        ctx.cures[cure.item] = true
        if (cure.poison == "poison" and player.poisoned)
            or (cure.poison == "serious" and player.seriously_poisoned) then
            cure_for[cure.item] = true
            cure_held = cure_held or bot:item_count(cure.item) > 0
        end
    end
    if next(cure_for) and not cure_held then
        ctx.cure_now = cure_for
    end
    ctx.own_ammo = {}
    for id, _ in pairs(knowledge and knowledge.weapons or {}) do
        ctx.guns[id] = true
        ctx.own_ammo[id] = bot:ammo_for(id)
        -- The reserve is kept only for a gun the bot holds: shells carried
        -- before the shotgun fill a slot a pickup needs, and the box gives
        -- them back once the gun is equipped (the reserve-floor withdrawal).
        local ammo = bot:ammo_for(id)
        if ammo and bot:has_item(id) then
            ctx.ammo[ammo] = true
            if bot:item_count(ammo) == 0 then
                ctx.gun_ammo = ctx.gun_ammo or {}
                ctx.gun_ammo[ammo] = true
            end
        end
    end
    if ctx.equipped and bot:ammo_for(ctx.equipped) then
        ctx.ammo[bot:ammo_for(ctx.equipped)] = true
    end
    for _, fact in ipairs(bot:use_facts()) do
        if BotPuzzle.use_open(bot, fact) then
            ctx.uses[fact.item] = true
        end
    end
    -- An item a wanted <deliver> needs is kept like a <use> item.
    for item, _ in pairs(BotPuzzle.wanted_items(bot)) do
        ctx.uses[item] = true
    end
    -- So is a doom book whose medal a <use> wants (305's fountain): the
    -- book becomes the medal once opened.
    for book = C.DOOM_BOOK_2, C.DOOM_BOOK_2 + 1 do
        if ctx.uses[book - C.DOOM_BOOK_2 + C.WOLF_MEDAL] then
            ctx.uses[book] = true
        end
    end
    for _, room in ipairs(bot:world().rooms) do
        for _, door in ipairs(room.doors) do
            if door.present and door.locked and not door.unlocked and not door.traversed then
                ctx.keys[door.key] = true
            end
        end
    end
    -- The loadout: the weapon (and its ammunition) a <fight> still ahead in
    -- this stage prefers, so a boss is met with it (513's Tyrant and the
    -- grenade launcher). The stage is the room id's first character.
    local here = bot:room().id
    local box = BotInventory.remembered(bot)
    for _, fight in ipairs(bot:fight_facts()) do
        local entry = knowledge and knowledge.enemies[fight.enemy]
        local prefer = entry and entry.prefer
        -- A preferred weapon neither held nor boxed (Brad's rocket
        -- launcher, dropped mid-fight) gives way to the enemy's opening
        -- one (`open`: the grenade launcher for 303's Tyrant).
        if prefer and entry.open and not bot:has_item(prefer) and not box[prefer] then
            prefer = entry.open
        end
        local row = prefer and knowledge.weapons[prefer]
        local pending = row and not (fight.bank and bot:flag(fight.bank, fight.bit))
        -- A firearm for a fight in a room already seen counts in any
        -- stage: a box trip into stage 4 deposited the launcher and its
        -- rounds that 30C's Black Tiger wanted, and the bot met the tiger
        -- with an empty handgun, twice. (A hand weapon waits for its
        -- fight's stage: the knife.)
        -- So does a fight whose `stage` attribute names this stage (303's
        -- Tyrant, met from the lab's escape lift), and a firearm for a
        -- fight not seen yet: 403's box left the launcher behind for 20C's
        -- Yawn 2, met from the guardhouse before the bot had seen 20C, and
        -- the handgun lost that fight four times.
        if pending and (fight.room:sub(1, 1) == here:sub(1, 1)
            or fight.stage == here:sub(1, 1)
            or not row.melee) then
            ctx.loadout = ctx.loadout or {}
            ctx.loadout[prefer] = true
            local ammo = bot:ammo_for(prefer)
            if ammo then
                ctx.loadout[ammo] = true
            end
            -- The same weapon loaded with other rounds (variant_of: the
            -- launcher with flame rounds is 0x09), held or boxed, is that
            -- weapon (the acid one went into 100's box as a spare gun and
            -- the bot met 309's hunters without it), and so are the other
            -- rounds held for it.
            -- The other rounds only while its own run short (OWN_ROUNDS):
            -- three kinds of grenade rounds held as loadout left no slot to
            -- deposit for 30A's hex crank, and the run ended there; but one
            -- explosive round left counted as held, the acid and flame
            -- rounds went into 30E's box, and the bot met the lab with one.
            local own = ammo and bot:item_count(ammo) >= OWN_ROUNDS
            for id, gun in pairs(knowledge.weapons) do
                if gun.variant_of == prefer then
                    local other = bot:ammo_for(id)
                    if bot:has_item(id) or box[id] then
                        ctx.loadout[id] = true
                    end
                    if other and not own and (bot:item_count(other) > 0 or box[other]) then
                        ctx.loadout[other] = true
                    end
                end
            end
        end
    end
    -- A hand weapon no <fight> in this stage prefers is spare: the last
    -- soft keep to go in (the knife, until the stage of 30C's web).
    for id, row in pairs(knowledge and knowledge.weapons or {}) do
        if row.melee and id ~= ctx.equipped and bot:has_item(id) then
            ctx.spare = ctx.spare or {}
            ctx.spare[id] = true
        end
    end
    -- Held firearms other than the equipped one and the staple (the
    -- handgun, whose ammunition is the most common), by rounds (the clip
    -- plus the ammunition held): the very last soft keeps, fewest rounds
    -- first. The staple, not equipped, goes after every other spare gun
    -- (ctx.staple_guns): 408's guardhouse key found the slots full of the
    -- equipped shotgun's kit, the loadout's grenade launcher and the
    -- handgun. The rearm takes it back once its ammunition is held.
    for id, row in pairs(knowledge and knowledge.weapons or {}) do
        if not row.melee and id ~= ctx.equipped and bot:has_item(id) then
            local ammo = bot:ammo_for(id)
            ctx.spare_guns = ctx.spare_guns or {}
            ctx.spare_guns[id] = bot:item_count(id) + (ammo and bot:item_count(ammo) or 0)
            if row.staple then
                ctx.staple_guns = ctx.staple_guns or {}
                ctx.staple_guns[id] = true
            end
        end
    end
    -- With a reserve gun (the launcher) or nothing equipped, one ordinary
    -- firearm is not spare: the swap boxed the shotgun and then the
    -- handgun behind the launcher, and the lab's zombies met an empty
    -- flame launcher and the knife. A loaded one that is not the staple
    -- (the shotgun) first, then the most rounds: kept for its rounds, the
    -- handgun met 308's two hunters with the shotgun boxed.
    local equipped = knowledge and ctx.equipped and knowledge.weapons[ctx.equipped]
    if not equipped or equipped.reserve or equipped.melee then
        local best, best_rank
        for id, rounds in pairs(ctx.spare_guns or {}) do
            local row = knowledge.weapons[id]
            local rank = (rounds > 0 and not row.staple) and 1 or 0
            if not row.reserve and (not best or rank > best_rank
                or (rank == best_rank and (rounds > ctx.spare_guns[best]
                    or (rounds == ctx.spare_guns[best] and id > best)))) then
                best, best_rank = id, rank
            end
        end
        if best then
            ctx.spare_guns[best] = nil
        end
    end
    -- The loadout keeps a hand weapon a fight in this stage prefers; one
    -- whose fight is in a later stage is spare until then (the knife,
    -- held for 30C's web from the first room, took a slot the shells
    -- needed), and the loadout takes it back out of the box there.
    for id, _ in pairs(ctx.loadout or {}) do
        if ctx.spare then
            ctx.spare[id] = nil
        end
    end
    ctx.ammo_low = BotSurvival.ammo_need(bot)
    -- The rearm: a firearm (not a reserve one) the remembered box holds,
    -- with its ammunition held (useless without it: the shells picked up
    -- after the shotgun went in for a key), or in the box while sourcing
    -- ammunition. The held ammunition stays (BotInventory.keep).
    if knowledge then
        local box = BotInventory.remembered(bot)
        local sourcing = BotSurvival.sourcing_now(bot)
        for id, row in pairs(knowledge.weapons) do
            local ammo = bot:ammo_for(id)
            local held = ammo and bot:item_count(ammo) > 0
            if not row.melee and not row.reserve and box[id] and not bot:has_item(id)
                and ammo and (held or (sourcing and box[ammo])) then
                ctx.rearm = ctx.rearm or {}
                ctx.rearm[id] = true
                ctx.rearm[ammo] = true
            end
        end
    end
    local weapon = bot:weapon()
    if weapon.ammo then
        local rounds, shots = BotSurvival.rounds(bot)
        local floor = BotSurvival.ammo_floor(shots, bot:risk() or 0.25)
        if floor > 0 and rounds < TOP_UP * floor then
            ctx.top_up = weapon.ammo
        end
    end
    local menu = bot:menu()
    for _, row in ipairs(menu.items or {}) do
        if row.kind == "heal" and row.heals ~= false and row.quantity > 0 then
            ctx.heals = ctx.heals + 1
        end
    end
    ctx.capacity = menu.capacity or #(menu.items or {})
    ctx.used = used_slots(menu.items or {})
    ctx.full = ctx.used >= ctx.capacity
    ctx.heal_top_up = true
    -- The rearm takes slots (the gun, and its ammunition when boxed): it
    -- waits while a key or <use> pickup needs the free ones (else the swap
    -- boxes the gun again: 100's shotgun went in and out for 250k ticks).
    if ctx.rearm then
        local box = BotInventory.remembered(bot)
        local slots = 0
        for item, _ in pairs(ctx.rearm) do
            if box[item] then
                slots = slots + 1
            end
        end
        local after = ctx.capacity - ctx.used - slots
        local _, progress = BotInventory.needed_pickups(bot, ctx)
        if progress > 0 and after < progress then
            ctx.rearm = nil
        end
    end
    if ctx.loadout then
        ctx.pickups = BotInventory.pickups_needing_room(bot)
    end
    return ctx
end

-- What the bot remembers the box holds: {[item] = true}.
function BotInventory.remembered(bot)
    local out = {}
    local text = bot:memory().box_items or ""
    for hex in string.gmatch(text, "%x+") do
        out[tonumber(hex, 16)] = true
    end
    return out
end

-- The <holds> items an open <use> fact needs that are off the frontier:
-- not held, not in the box and not a frontier pickup. They lie where only
-- a room-local step (a <climb>, say) reaches them, so from another room the
-- frontier misses them (11B's crank on its shelf).
local function held_elsewhere(bot)
    local open = {}
    for _, fact in ipairs(bot:use_facts()) do
        if BotPuzzle.use_open(bot, fact) then
            open[fact.item] = true
        end
    end
    local seen = {}
    for _, goal in ipairs(bot:world().frontier) do
        if goal.kind == "item" then
            seen[goal.item] = true
        end
    end
    local box = BotInventory.remembered(bot)
    local n = 0
    for _, fact in ipairs(bot:item_facts()) do
        if open[fact.item] and not seen[fact.item] and not box[fact.item]
            and not bot:has_item(fact.item) then
            seen[fact.item] = true
            n = n + 1
        end
    end
    return n
end

-- Known pickups that need a free slot: frontier items that merge into no
-- held stack, and the off-frontier <holds> items (held_elsewhere).
-- `skip(goal)` drops one the planner has given up on.
function BotInventory.pickups_needing_room(bot, skip)
    -- A due <fetch> hands over an item that needs a slot too.
    local n = #BotPuzzle.fetches_due(bot) + held_elsewhere(bot)
    for _, goal in ipairs(bot:world().frontier) do
        if goal.kind == "item" and not (skip and skip(goal)) then
            local _, merge = bot:item_kind(goal.item)
            if not (merge and bot:has_item(goal.item)) then
                n = n + 1
            end
        end
    end
    return n
end

-- Known pickups that need a free slot and that the run needs: a key, an
-- item a <use> fact needs, or the ammunition a held gun lacks. The second
-- result counts the keys, <use> items and fetches among them: progress
-- items worth a spare gun's slot.
function BotInventory.needed_pickups(bot, ctx, skip)
    local n = #BotPuzzle.fetches_due(bot) + held_elsewhere(bot)
    local progress = n
    for _, goal in ipairs(bot:world().frontier) do
        if goal.kind == "item" and not (skip and skip(goal)) then
            local kind, merge = bot:item_kind(goal.item)
            local needed = kind == "key" or ctx.uses[goal.item]
                or (ctx.gun_ammo and ctx.gun_ammo[goal.item])
                or ctx.ammo_low == goal.item
            if needed and not (merge and bot:has_item(goal.item)) then
                n = n + 1
                if kind == "key" or ctx.uses[goal.item] then
                    progress = progress + 1
                end
            end
        end
    end
    return n, progress
end

-- What a deposit puts in the box: the keep rule's leftovers, else the soft
-- keeps (BotInventory.soft_deposits) when a needed pickup has no slot.
function BotInventory.make_room(bot, ctx, items, skip)
    local deposits = BotInventory.deposits(items, ctx)
    if #deposits == 0 and ctx.full then
        local needed, progress = BotInventory.needed_pickups(bot, ctx, skip)
        -- A key or <use> item on the frontier is worth a spare gun's slot.
        if progress > 0 then
            ctx.swap = true
        end
        -- A box item the bot needs back (a key a locked door needs, an
        -- item a <use> fact needs) is a needed pickup too: a swap.
        for item, _ in pairs(BotInventory.remembered(bot)) do
            local kind, _, heals = bot:item_kind(item)
            local why = BotInventory.wanted_back({ item = item, kind = kind, heals = heals }, ctx)
            if why == "a <use> fact needs it" or why == "a locked door needs it"
                or why == "ammunition for a held gun" or FIRST[why] then
                needed = needed + 1
                if why ~= "ammunition for a held gun" and not FIRST[why] then
                    ctx.swap = true
                end
                if why == LOADOUT then
                    ctx.heals_may_go = true
                end
            end
        end
        deposits = BotInventory.soft_deposits(items, ctx, needed)
    end
    return deposits
end

-- Whether to open an item box only to see what it holds: no box has ever
-- been opened (`box_items`, the remembered contents, is nil), a slot is
-- free for what it may hold, and the box is in this room (`here`, a look on
-- the way) or the bot is sourcing ammunition (BotSurvival.sourcing, worth
-- a trip). Every box shares one store, so one look covers them all.
function BotInventory.look_in_box(ctx, box_items, sourcing, here)
    return (sourcing == true or here == true) and box_items == nil
        and ctx.used < ctx.capacity
end

-- Why the bot should visit an item box now, or nil: a deposit (full, a
-- pickup needs room and something may go in) or a withdrawal (the box
-- holds an item the bot needs and a slot is, or can be made, free).
-- `here` asks for a box in this room, which also takes the slot reserve,
-- a first look and the ammunition top-up (see the header); a trip to a
-- remote box is worth only the rest.
function BotInventory.box_reason(bot, skip, here)
    local ctx = BotInventory.context(bot)
    local items = bot:menu().items or {}
    local deposits = BotInventory.make_room(bot, ctx, items, skip)
    local pickups = BotInventory.pickups_needing_room(bot, skip)
    if ctx.full and #deposits > 0 and pickups > 0 then
        return "deposit: the inventory is full and a pickup needs room"
    end
    if here and BotInventory.reserve_due(ctx, #deposits, pickups) then
        return "deposit: keep a slot free for pickups"
    end
    if BotInventory.look_in_box(ctx, bot:memory().box_items, BotSurvival.sourcing_now(bot),
        here) then
        return here and "look in the box on the way: never opened"
            or "look in the box: sourcing ammunition and the box was never opened"
    end
    if not here then
        ctx.reserve_only = true
    end
    local free = ctx.capacity - ctx.used
    if free > 0 or #deposits > 0 then
        for item, _ in pairs(BotInventory.remembered(bot)) do
            local kind, _, heals = bot:item_kind(item)
            local why = BotInventory.wanted_back({ item = item, kind = kind, heals = heals }, ctx)
            if TOP_UPS[why] and free <= slots_to_leave(why, ctx) then
                why = nil
            end
            if why then
                return string.format("withdraw 0x%02X: %s", item, why)
            end
        end
    end
    return nil
end

-- Health a heal used to free a slot must have to restore: at least this
-- much missing, so the herb is not wasted.
BotInventory.MAKE_ROOM_HURT = 24

-- The heal item to use now to free a slot for a needed pickup in this room,
-- or nil: every slot used, a frontier pickup here (one the planner has not
-- given up on, `skip`) that merges into no held stack and that the run
-- needs (a key, a <use> item, or the ammunition of a gun held or in the
-- box), at least two heal stacks held (a cure is none), and MAKE_ROOM_HURT
-- health missing.
-- 219 left 12 grenade rounds for a full inventory at 56/96 health with
-- four heal stacks held.
function BotInventory.heal_for_room(bot, skip)
    local ctx = BotInventory.context(bot)
    local player = bot:player()
    local hurt = player.max_health - player.health
    if not ctx.full or hurt <= 0 then
        return nil
    end
    local heals = {}
    for _, row in ipairs(bot:menu().items or {}) do
        if row.kind == "heal" and row.heals ~= false and row.quantity > 0
            and not (ctx.cures and ctx.cures[row.item]) then
            heals[#heals + 1] = row
        end
    end
    if #heals < 2 then
        return nil
    end
    local box = BotInventory.remembered(bot)
    local guns_ammo = {}
    for id, _ in pairs(ctx.guns) do
        local ammo = bot:ammo_for(id)
        if ammo and (bot:has_item(id) or box[id]) then
            guns_ammo[ammo] = true
        end
    end
    local here = bot:room().id
    local wanted, needed = false, false
    for _, goal in ipairs(bot:world().frontier) do
        if goal.kind == "item" and goal.room == here and not (skip and skip(goal)) then
            local kind, merge = bot:item_kind(goal.item)
            local book = goal.item == C.DOOM_BOOK_2 or goal.item == C.DOOM_BOOK_2 + 1
            if not (merge and bot:has_item(goal.item))
                and (kind == "key" or ctx.uses[goal.item] or book or guns_ammo[goal.item]) then
                wanted = true
                needed = needed or kind == "key" or ctx.uses[goal.item] or book
            end
        end
    end
    -- A key, a <use> item or a doom book takes a heal at any hurt: 306's
    -- book lies in its zone only while the pushed box stands in the probe,
    -- and the room puts the box back once the bot leaves.
    if not wanted or (not needed and hurt < BotInventory.MAKE_ROOM_HURT) then
        return nil
    end
    -- The largest stack first, as BotSurvival.heal does.
    table.sort(heals, function(a, b)
        if a.quantity ~= b.quantity then
            return a.quantity > b.quantity
        end
        return a.slot < b.slot
    end)
    return heals[1].item
end

-- Why a combine is worth doing now, and the pick, or nil.
function BotInventory.combine_now(bot, skip)
    local ctx = BotInventory.context(bot)
    if not ctx.full or BotInventory.pickups_needing_room(bot, skip) == 0 then
        return nil
    end
    return BotInventory.combine_pick(bot:menu().items or {}, ctx, function(a, b)
        return bot:combine_result(a, b)
    end)
end

-- An empty weapon whose own ammunition is not held, loaded with the
-- rounds of another loading of it (knowledge variant_of: the grenade
-- launcher's acid and flame rounds), the most held first. Loading other
-- rounds turns it into that loading's item and loses the rounds it held,
-- so only an empty one is loaded: the flame rounds rode along unused while
-- hunters hit the bot with an empty launcher. Returns the combine pick
-- ({slot, target, item, other}, 0-based menu slots) or nil.
function BotInventory.load_now(bot)
    local k = bot:knowledge()
    local items = bot:menu().items or {}
    for _, gun in ipairs(k and items or {}) do
        local row = gun.kind == "weapon" and k.weapons[gun.item]
        local own = row and not row.melee and bot:ammo_for(gun.item)
        if own and gun.quantity == 0 and bot:item_count(own) == 0 then
            local base = row.variant_of or gun.item
            local best, best_count
            for id, other in pairs(k.weapons) do
                local ammo = id ~= gun.item and (id == base or other.variant_of == base)
                    and bot:ammo_for(id)
                local count = ammo and bot:item_count(ammo) or 0
                if count > 0 and (not best_count or count > best_count
                        or (count == best_count and ammo < best)) then
                    best, best_count = ammo, count
                end
            end
            for _, row_ammo in ipairs(best and items or {}) do
                if row_ammo.item == best then
                    for _, pair in ipairs({ { gun, row_ammo }, { row_ammo, gun } }) do
                        local r = bot:combine_result(pair[1].slot, pair[2].slot)
                        local loaded = r and (pair[1] == gun and r.cursor_quantity
                            or r.target_quantity) or 0
                        if r and r.result == "applied" and loaded > 0 then
                            return { slot = pair[1].slot, target = pair[2].slot,
                                item = pair[1].item, other = pair[2].item }
                        end
                    end
                end
            end
        end
    end
    return nil
end

-- ---- the box screen --------------------------------------------------------

local function remember_box(bot, view)
    local ids = {}
    for _, row in ipairs(view.items) do
        if row.item ~= 0 then
            ids[#ids + 1] = string.format("%02X", row.item)
        end
    end
    local text = table.concat(ids, ",")
    if bot:memory().box_items ~= text then
        bot:remember("box_items", text)
    end
end

-- The plan for one box visit: deposits first (they free the slots the
-- withdrawals need), then withdrawals.
local function plan_visit(bot, menu, view)
    local ctx = BotInventory.context(bot)
    local ops = {}
    local free = ctx.capacity - ctx.used
    local deposits = BotInventory.make_room(bot, ctx, menu.items)
    local pickups = BotInventory.pickups_needing_room(bot)
    -- The reserve is judged after this visit's withdrawals, so one visit
    -- both takes and makes room.
    local taking = #BotInventory.withdrawals(view.items, ctx, free)
    local after = { capacity = ctx.capacity, used = ctx.used + taking }
    if (ctx.full and pickups > 0) or BotInventory.reserve_due(after, #deposits, pickups) then
        for _, row in ipairs(deposits) do
            ops[#ops + 1] = { kind = "deposit", item = row.item }
            free = free + 1
        end
    end
    for _, row in ipairs(BotInventory.withdrawals(view.items, ctx, free)) do
        ops[#ops + 1] = { kind = "withdraw", item = row.item }
    end
    local parts = {}
    for _, op in ipairs(ops) do
        parts[#parts + 1] = string.format("%s 0x%02X", op.kind, op.item)
    end
    log(bot, string.format("box open: %d op(s) [%s], inventory %d/%d, box %s", #ops,
        table.concat(parts, ", "), ctx.used, ctx.capacity, bot:memory().box_items or "?"))
    return ops
end

local function inventory_slot(menu, want)
    for _, row in ipairs(menu.items) do
        if want(row) then
            return row
        end
    end
    return nil
end

-- The d-pad key that moves the box screen's inventory cursor toward
-- `target`: off the EXIT tab row first, then the column, then the row.
local function cursor_key(cursor, target)
    if cursor < FIRST_SLOT_CURSOR then
        return "down"
    end
    if (cursor & 2) ~= (target & 2) then
        return "right"
    end
    if target > cursor then
        return "down"
    end
    return "up"
end

local function pulse(bot, s, key)
    local tick = bot:tick()
    if tick - (s.last_press or -C.SECOND) < PRESS_GAP then
        return
    end
    s.last_press = tick
    bot:set_input({ [key] = true })
end

-- One tick of the current op on the box screen. Returns true when the op
-- is over (done or given up).
local function drive_op(bot, s, menu, view, op)
    op.ticks = (op.ticks or 0) + 1
    if op.ticks > OP_TIMEOUT then
        log(bot, string.format("%s 0x%02X gave up (mode %s, box %d, cursor %d)",
            op.kind, op.item, view.mode, view.box_cursor, view.cursor))
        return true
    end
    if not op.before then
        op.before = bot:item_count(op.item)
        op.held = bot:has_item(op.item)
    end
    -- The swap shows in the engine's inventory.
    if op.kind == "deposit" and (bot:item_count(op.item) < op.before
        or not bot:has_item(op.item)) then
        s.deposits = (s.deposits or 0) + 1
        log(bot, string.format("deposited 0x%02X", op.item))
        return true
    end
    if op.kind == "withdraw" and (bot:item_count(op.item) > op.before
        or (bot:has_item(op.item) and not op.held)) then
        s.withdrawals = (s.withdrawals or 0) + 1
        log(bot, string.format("withdrew 0x%02X", op.item))
        return true
    end
    if view.sliding then
        return false
    end
    local row
    if op.kind == "deposit" then
        row = inventory_slot(menu, function(r) return r.item == op.item end)
    else
        row = inventory_slot(menu, function(r) return r.item == 0 end)
    end
    if not row then
        log(bot, string.format("%s 0x%02X: no inventory slot for it", op.kind, op.item))
        return true
    end
    local target_cursor = FIRST_SLOT_CURSOR + 2 * row.slot
    if view.mode == "browse" then
        if view.cursor ~= target_cursor then
            pulse(bot, s, cursor_key(view.cursor, target_cursor))
        else
            pulse(bot, s, "confirm")
        end
        return false
    end
    -- Armed: the inventory slot is fixed; a wrong one backs out first.
    if view.cursor ~= target_cursor then
        pulse(bot, s, "cancel")
        return false
    end
    local box_row, dir = BotInventory.box_target(view.items, view.box_cursor, function(r)
        if op.kind == "deposit" then
            return r.item == 0
        end
        return r.item == op.item
    end)
    if not box_row then
        log(bot, string.format("%s 0x%02X: no box slot for it", op.kind, op.item))
        pulse(bot, s, "cancel")
        return true
    end
    if dir == 0 then
        pulse(bot, s, "confirm")
    else
        pulse(bot, s, dir > 0 and "down" or "up")
    end
    return false
end

-- Record a box visit's end for the planner's box goal.
local function end_visit(bot, s)
    s.visit = nil
    s.done_tick = bot:tick()
end

-- One tick before the scene handler: the item box screen, the lid before
-- it and the close after it own the pad. Returns true when it consumed the
-- tick.
function BotInventory.screen_step(bot)
    local s = state()
    local menu = bot:menu()
    local view = menu.box
    local tick = bot:tick()
    if view and menu.screen == "item_box" then
        remember_box(bot, view)
        if not s.visit then
            s.visit = { start = tick, ops = nil, index = 1 }
            s.expect = nil
        end
        local visit = s.visit
        if tick - visit.start < OPEN_SETTLE then
            return true
        end
        if not visit.ops then
            visit.ops = plan_visit(bot, menu, view)
        end
        local op = visit.ops[visit.index]
        if op and tick - visit.start < SESSION_TIMEOUT then
            if drive_op(bot, s, menu, view, op) then
                visit.index = visit.index + 1
            end
            return true
        end
        -- Done: cancel out (an armed slot first goes back to browse).
        visit.closing = true
        pulse(bot, s, "cancel")
        return true
    end
    if s.visit then
        -- The box is gone: wait out the close fade and the freeze.
        local visit = s.visit
        visit.settle = (visit.settle or 0) + 1
        local scene = bot:scene()
        -- The lid closes after the screen does, under a freeze: wait until
        -- the player has been controllable for a moment, or a press meant
        -- for the next goal reopens the box.
        if scene.controllable and not (menu.open or menu.requested) then
            visit.free = (visit.free or 0) + 1
        else
            visit.free = 0
        end
        if visit.free > CLEAR_TICKS or visit.settle > SETTLE_TIMEOUT then
            end_visit(bot, s)
            return false
        end
        return true
    end
    -- Between the box press and the screen: the lid animates.
    if s.expect and tick - s.expect < LID_WAIT then
        if menu.open or bot:scene().frozen or not bot:scene().controllable then
            return true
        end
    end
    return false
end

-- The planner pressed an item box zone: hold off the scene handler until
-- the screen shows (or LID_WAIT passes).
function BotInventory.expect_box(bot)
    state().expect = bot:tick()
end

-- Whether a box visit ended at or after `tick`.
function BotInventory.visited_since(tick)
    local done = state().done_tick
    return done ~= nil and done >= tick
end
