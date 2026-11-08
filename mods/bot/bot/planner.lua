-- The generic goal planner for the player bot.
--
-- Each tick that the scene handler, the menu skills, healing and combat do
-- not own, the planner runs the current goal, or picks one, highest priority
-- first:
--   1. room puzzle: the next <interact> of the room's bot.xml whose done
--      flag is clear (walk to its AOT zone, press, wait for the flag);
--      then a <use> fact that applies now (the item is held, the done flag
--      clear and the use-item probe live): stand where the probe arms the
--      item, USE it through the menu, wait for the done flag;
--      then inventory room (BotInventory): with the inventory at its limit
--      and a known pickup needing a slot, a MOVE that empties one; then the
--      nearest item box here (BotPlanner.nearest) when a deposit, the slot
--      reserve, a first look or a withdrawal is due (BotInventory.box_reason
--      with `here`);
--   2. ground items in this room, keys first, then the nearest approach:
--      a visible item here is taken before the bot moves on, unless the
--      inventory is full and it merges into no held stack. A second zone
--      of an item just taken here gets one approach. A drawer (a desk zone)
--      counts as its item: pressed and answered Yes until the item is
--      taken; a drawer whose lock bit is clear waits, marked, until the
--      desk key or the lockpick is held, and an empty one is skipped;
--   3. a passage check when a goal of this room has no reachable approach
--      from here: a `door_check` stair zone, or an action-key event zone no
--      fact names (a script may start the climb from an event), each once
--      per room entry, done when the player warps;
--   4. the way out. A pursued target (4a) keeps its course: the first door
--      of the route to it, until it is reached, real progress re-ranks or
--      the loop rule drops it. Otherwise rank by distance cost (lowest
--      first) over the doors here, the first door of the route to every
--      remote frontier goal, and to every room whose <use> fact
--      (bot:use_facts()) needs a held item:
--        cost = path length to the door's approach (UNPLANNED_COST when the
--               path planner reaches none)
--             + DOOR_COST per door step of the route (0 for a door here)
--             + for a remote goal, its path length in its own room from where
--               the bot stood there last (UNPLANNED_COST if none was found)
--             - UNVISITED_BONUS for a door into a room not yet entered
--             - ITEM_BONUS for a route to a remote frontier item
--             - KEY_BONUS when that item is a key a remembered locked
--               door needs, or AMMO_BONUS when it is the ammunition the
--               equipped gun needs (BotSurvival.ammo_need); KEY_BONUS is
--               the larger, so a needed key comes before ammunition and
--               herbs (BotPlanner.item_bonus)
--             - SOURCE_BONUS for a door into a room not yet entered, and
--             + MONSTER_COST per live monster last seen in each room the
--               route enters, both only while sourcing ammunition
--               (BotSurvival.sourcing: under the floor and no ammunition
--               the planner can reach, a pickup it has not given up on or
--               the remembered box)
--             + HURT_ROOM_COST per monster that hurt the bot in each room
--               the route enters, while it is hurt with no heal held
--             + BOSS_ROOM_COST for each room the route enters whose <fight>
--               is still ahead while the bot is not ready for it
--             - for a route to a held item's <use> room, HELD_USE_BONUS
--               and no door steps: only the first door's length counts;
--               so for a route back to a <fight> held back there as not
--               ready and ready now (fight_due)
--             - HEAL_BONUS for a route to a heal pickup or to the box for
--               one while hurt with none held (BotSurvival.needs_heal)
--             - for a route to a <fetch> of a cure for the player's
--               poison, CURE_BONUS (twice HELD_USE_BONUS), no door steps
--             - BOX_BONUS for a route to a remembered typewriter when a
--               save trip is due (BotSave.trip_reason), picked like a
--               remote box
--             - BOX_BONUS for a route to a remembered item box when a box
--               visit is due (BotInventory.box_reason); a remote box's whole
--               cost is its walking distance over the mental map instead
--               (BotPlanner.pick_remote_box: exit path, then each room's leg
--               from the arrival point, MAP_DOOR per door load)
--             - for a door into a room not yet entered, the bonus of an item
--               a <holds> fact places there that the bot lacks and needs
--               (BotPlanner.holds_bonus)
--             - GOAL_BONUS for a door into the --bot-goal-room, or
--               STAGE_BONUS for one into the goal room's stage;
--      ties go to the doors here, then frontier order. A locked door counts
--      only with the key it needs. The remote winner becomes the pursued
--      target. An exit with no planned approach from here gets a passage
--      check first;
--   5. nothing left: approach and time-out marks under MAX_ATTEMPTS get one
--      more try; then finish with the reason.
-- A goal that fails is marked with the inventory it failed with; it is tried
-- again (at most MAX_ATTEMPTS times; a key or <use> item without limit for
-- a full inventory) once the inventory changes. A <use>
-- that failed in its room is retried there, never travelled to.
--
-- The loop rule. Real progress is a change of the rooms entered, the items
-- taken, the doors unlocked or the inventory, or a puzzle step done. Without
-- it, a room entered more than MAX_REPEATS times marks the pursued target
-- (or the last exit taken from the room) looping, and more than twice that
-- finishes the run; a pursued target whose route from a newly entered room
-- is no shorter than the shortest seen MAX_STALLS times is marked looping.
-- A looping mark holds for LOOP_HOLD or until the inventory changes. The
-- limits are three times what the re1 bench run reaching 20D needs (at most
-- 2 entries of one room between progress, 1 stall).
-- The goal room, if any, comes from --bot-goal-room. Every room fact comes
-- from the world model and the rooms' bot.xml; this module holds no room
-- literals.

BotPlanner = BotPlanner or {}

local C = BotConst

local MAX_ATTEMPTS = 3
local GOAL_TIMEOUT = 40 * C.SECOND
-- After a goal is done here, a <use> fact of this room whose item is held
-- but whose probe is not live yet holds the next goal choice (and so the way
-- out and the finish) this long: the step's room event may still be arming
-- the probe.
local USE_ARM_TICKS = 3 * C.SECOND
-- A <fight> step holds the room this long before it gives up: a boss takes
-- many clips (40C's Plant 42).
local FIGHT_TIMEOUT = 5 * 60 * C.SECOND
-- How long a <wait> step stands still for its room event.
local WAIT_TIMEOUT = 60 * C.SECOND
-- A run with no new progress (a room, an item, a flag fact) for this long
-- is stuck.
local STUCK_TICKS = 5 * 60 * C.SECOND
-- The loop rule (see the header). A pursued remote target whose route from
-- a newly entered room is no shorter than the shortest seen counts a stall;
-- this many stalls mark it looping.
local MAX_STALLS = 3
-- A room entered more than this many times since the last real progress
-- marks the goal being pursued (or the last exit taken from the room) as
-- looping; more than twice as many finishes the run.
local MAX_REPEATS = 6
-- A goal marked looping is held off this long, or until the inventory
-- changes, whichever comes first.
local LOOP_HOLD = 3 * 60 * C.SECOND
-- How long an empty frontier waits before it ends the run (a won fight's
-- scene, then the ending, can still be coming).
local EMPTY_FRONTIER_GRACE = 40 * C.SECOND
-- Revisits of rooms left before the story flags changed allowed between
-- two real progress changes (BotPlanner.revisit_room).
local MAX_REVISITS = 8
-- A route to the ammunition the equipped gun needs (BotSurvival.ammo_need).
local AMMO_BONUS = 60 * C.REACH
-- A route to a frontier item that a <use> fact needs (item_bonus).
local USE_BONUS = 60 * C.REACH
-- A route to a seen key item that a remembered locked door needs: more
-- than the ammunition bonus, so a needed key comes first.
local KEY_BONUS = 70 * C.REACH
-- A route for an item errand (<deliver>/<fetch>): more than a key, since
-- the room that wants the item may want it only for a while (a poisoned
-- partner who dies after some minutes).
local ERRAND_BONUS = 2 * KEY_BONUS
-- A route to a room whose <use> fact needs a held item (bot:use_facts()):
-- the item makes progress only there, so the route's length is not
-- counted (305's fountain is a dozen doors from the mansion, where Doom
-- Book I becomes the Eagle Medal) and it outranks exploring.
local HELD_USE_BONUS = 2 * ERRAND_BONUS
-- A route to a <fetch> of the cure for a poison the player has (the second
-- serum after Yawn's bite): the poison drains health the whole way, so it
-- outranks every other target and the route's length is not counted.
local CURE_BONUS = 2 * HELD_USE_BONUS
-- A route to a heal pickup, or to the box for one, while the bot is hurt
-- with none held (BotSurvival.needs_heal): ahead of every target but a cure.
local HEAL_BONUS = CURE_BONUS - KEY_BONUS
-- A route to a remembered item box room when a box visit is due.
local BOX_BONUS = 60 * C.REACH
-- A route into a room whose <fight> is still ahead while the bot is not
-- ready for it (BotSurvival.fight_ready): a boss meets the bot on entry
-- (30C's Black Tiger killed it walking in at 5 health), so only a route
-- with no other way left takes it.
local BOSS_ROOM_COST = CURE_BONUS
-- The sort length of a goal the path planner reaches no approach to: last.
local UNPLANNED = 1 / 0
-- Ranks an unplanned stair check after every planned one, by distance.
local UNPLANNED_BASE = C.REACH * C.REACH
-- Way-out ranking (see the header), in path units. A door the path planner
-- reaches no approach to costs this much, so it ranks after every planned
-- one but still counts.
local UNPLANNED_COST = C.REACH * C.REACH * C.REACH
-- Each door step of a remote route: about one room crossing.
local DOOR_COST = 20 * C.REACH
-- The mental map's price of one door load, in walking units: the door film
-- and its fades take about 1.5 seconds (the bench's `doors` and `trans`
-- ticks over its room entries), which the run (about 200 units a tick)
-- would cover in this much distance.
local MAP_DOOR = 15 * C.REACH
-- A door to a room never entered.
local UNVISITED_BONUS = 15 * C.REACH
-- A route to a remote frontier item.
local ITEM_BONUS = 10 * C.REACH
-- A door into the --bot-goal-room, and one into the goal's stage. The goal
-- door is worth ten door steps: once its key is held it outranks every
-- other goal short of a long detour.
local GOAL_BONUS = 10 * DOOR_COST
local STAGE_BONUS = 6 * C.REACH
-- Sourcing ammunition (BotSurvival.sourcing: under the floor, none known):
-- a door into a room not yet entered is worth this much more, and each live
-- monster last seen in a room the route enters costs MONSTER_COST (at most
-- MONSTER_CAP of them a room).
local SOURCE_BONUS = 60 * C.REACH
local MONSTER_COST = DOOR_COST
local MONSTER_CAP = 3
-- Hurt with no heal held (BotSurvival.needs_heal): each monster that hurt
-- the bot in a room the route enters (BotTactics.note_hurt, at most
-- MONSTER_CAP a room) costs this much, so the way to a heal goes round the
-- rooms where a cerberus already bit.
local HURT_ROOM_COST = 2 * DOOR_COST
-- The hurt rule: a pickup goal (an item or a drawer) that loses this share of the
-- maximum health, on ticks the planner drives (not a fight's), is dropped
-- and marked `hurt while trying`; the room's other pickups wait until the
-- inventory changes, and remote planning does not head back for it.
local HURT_SHARE = 0.25
-- How often the live monsters of this room are counted.
local MONSTER_EVERY = C.SECOND // 2
-- A door a room event turned into a message (BotPlanner.note_message) is
-- retried on a later entry into its room; remote planning heads back for it
-- only once nothing else is left, and not before it has waited this long
-- (doubled per attempt): the event may clear with time, on a room entry.
local EVENT_WAIT = 60 * C.SECOND

-- A room whose only ways out are doors parked at its origin (310's lift)
-- may carry the player through one by itself once its event ends: with
-- nothing else to do, the bot idles this long after entering before the
-- frontier counts as empty.
local TRANSFER_WAIT = 30 * C.SECOND
-- An action-key event pressed as a passage gets this long.
local PASSAGE_TIMEOUT = 15 * C.SECOND

local function plan()
    BOT_STATE.plan = BOT_STATE.plan
        or { marks = {}, stairs = {}, locked = {}, entries = {}, lens = {}, progress = 0 }
    BOT_STATE.plan.lens = BOT_STATE.plan.lens or {}
    -- The mental map's arrival points: arrivals[room][from] = {x, z}, where
    -- the bot stood on entering `room` from `from`.
    BOT_STATE.plan.arrivals = BOT_STATE.plan.arrivals or {}
    -- The mental map's measured legs: legs[room][from][slot] is the planned
    -- path length to that door from where the bot arrived in `room` from
    -- `from` (UNPLANNED_COST when the path planner reached no approach, as
    -- on another level of the room), so remote routes price it as the bot
    -- would there.
    BOT_STATE.plan.legs = BOT_STATE.plan.legs or {}
    -- The hurt rule: room -> the inventory signature it hurt the bot with.
    BOT_STATE.plan.hurt = BOT_STATE.plan.hurt or {}
    BOT_STATE.plan.since = BOT_STATE.plan.since or {}
    BOT_STATE.plan.exits = BOT_STATE.plan.exits or {}
    BOT_STATE.plan.monsters = BOT_STATE.plan.monsters or {}
    return BOT_STATE.plan
end

-- The inventory as a string: a failed goal is retried once this changes.
local function inventory_sig(bot)
    local ids = {}
    for _, row in ipairs(bot:inventory()) do
        if row.item ~= 0 then
            ids[#ids + 1] = string.format("%02X", row.item)
        end
    end
    table.sort(ids)
    local flag = C.LOCKPICK_FLAG
    return table.concat(ids, ",") .. (bot:flag(flag.bank, flag.bit) and "+" or "")
end

local function mark_key(kind, room, slot)
    return kind .. ":" .. room .. ":" .. tostring(slot)
end

-- The <reach> fact of door `slot` in room `room`, or nil (bot:reach_facts()).
local function reach_fact(bot, room, slot)
    for _, fact in ipairs(bot:reach_facts()) do
        if fact.room == room and fact.slot == slot then
            return fact
        end
    end
    return nil
end

-- Whether a <reach> fact keeps door `slot` of room `room` out of reach now:
-- its flag is not in its state, or (entry="same") the bot is not in the
-- room on the entry the flag got there. False for a door with no fact.
local function reach_closed(bot, room, slot)
    local fact = reach_fact(bot, room, slot)
    if not fact then
        return false
    end
    if bot:flag(fact.bank, fact.bit) ~= fact.set then
        return true
    end
    if fact.same_entry then
        if room ~= bot:room().id then
            return true
        end
        -- In the state already on entering: an entry after the one it got
        -- there (20C seen from 20B, not from the pit).
        return (plan().reach_entry or {})[room .. ":" .. slot] == true
    end
    return false
end

-- Called on every room change: whether each same-entry <reach> fact of
-- this room is in its state already on entering.
local function note_reaches(p, bot, room_id)
    p.reach_entry = p.reach_entry or {}
    for _, fact in ipairs(bot:reach_facts()) do
        if fact.room == room_id and fact.same_entry then
            p.reach_entry[room_id .. ":" .. fact.slot] =
                bot:flag(fact.bank, fact.bit) == fact.set
        end
    end
end

local function mark(bot, kind, room, slot, reason, permanent)
    local p = plan()
    local key = mark_key(kind, room, slot)
    local m = p.marks[key] or { attempts = 0 }
    m.attempts = m.attempts + 1
    m.reason = reason
    m.sig = inventory_sig(bot)
    m.permanent = permanent or false
    m.entry = (p.entries or {})[room]
    m.kind, m.room, m.slot = kind, room, slot
    -- A door with a <reach> fact: the mark lifts once the fact's flag
    -- changes (blocked), not on a puzzle step anywhere (clear_unreachable).
    local reach = kind == "door" and reach_fact(bot, room, slot)
    m.reach_state = reach and (bot:flag(reach.bank, reach.bit) == reach.set) or nil
    p.marks[key] = m
    bot:log(string.format("mark %s: %s (attempt %d)", key, reason, m.attempts))
end

-- Puzzle steps held for a free slot (a <push reveals>, its `inventory full`
-- mark set needed): each counts as a pickup needing room,
-- so a box visit frees one (BotInventory.pickups_needing_room). Only while
-- the inventory is as it was marked: once it changes the step is tried
-- again, and a done step's mark goes stale with the item it handed over.
-- With `any_sig`, every such step still held, whatever the inventory: the
-- slots the pickups that no step or use needs must leave free.
function BotPlanner.steps_needing_room(bot, any_sig)
    local n = 0
    local sig = inventory_sig(bot)
    for _, m in pairs(plan().marks) do
        if m.kind == "interact" and m.needed and (any_sig or m.sig == sig)
            and m.reason:find("inventory full", 1, true) then
            n = n + 1
        end
    end
    return n
end

-- Whether an `inventory full` mark on `item` waits for the inventory to
-- change however often it was tried, beyond a key or <use> item (m.needed):
-- a heal while the bot is hurt with none held (408's wasps stung it to 1
-- with the mansion's green herbs given up), or the ammunition of a gun
-- held or in the box (35 shells were left on full inventories while the
-- bot ran dry).
local function full_mark_needed(bot, item)
    local kind, _, heals = bot:item_kind(item)
    if kind == "heal" then
        return heals == true and BotSurvival.needs_heal(bot)
    end
    if kind ~= "ammo" then
        return false
    end
    local k = bot:knowledge()
    local box = BotInventory.remembered(bot)
    for id, _ in pairs(k and k.weapons or {}) do
        if bot:ammo_for(id) == item and (bot:has_item(id) or box[id]) then
            return true
        end
    end
    return false
end

-- Whether a marked goal is still off limits. A goal is tried again once
-- the inventory changes; a goal that failed on its approach is also tried
-- again on a later entry into its room (the bot may now stand in a part of
-- the room that reaches it), but `remote` planning from another room does
-- not head back for it.
local function blocked(bot, kind, room, slot, remote)
    local m = plan().marks[mark_key(kind, room, slot)]
    if not m then
        return false
    end
    if m.reach_state ~= nil then
        -- A door with a <reach> fact, marked with its flag in one state:
        -- tried again once the flag is in the other.
        local reach = reach_fact(bot, room, slot)
        if reach and (bot:flag(reach.bank, reach.bit) == reach.set) ~= m.reach_state then
            plan().marks[mark_key(kind, room, slot)] = nil
            return false
        end
    end
    if m.desk then
        -- A locked drawer: off limits, from this room or any other, until
        -- its lock bit is set or the desk key or the lockpick is held.
        if BotPlanner.desk_blocker(bot, m.desk) then
            return true
        end
        plan().marks[mark_key(kind, room, slot)] = nil
        return false
    end
    if m.event then
        -- A door a room event made a message: off limits on the entry it
        -- failed on; from another room until the empty-frontier retry
        -- forgives it (forgive_events).
        if remote then
            return not m.event.forgiven
        end
        return (plan().entries or {})[room] == m.event.entry
    end
    if m.gate then
        -- A gated <avoid>: off limits exactly while its flag says so, from
        -- this room or any other.
        if BotPuzzle.gate_holds(bot, m.gate) then
            return true
        end
        plan().marks[mark_key(kind, room, slot)] = nil
        return false
    end
    if m.lock_bit and bot:flag(C.LOCK_BANK, m.lock_bit) then
        -- A door locked for good from this side lifts once its lock flag
        -- is set: opened from the far side (507's door 0 opens 505's
        -- door 2).
        plan().marks[mark_key(kind, room, slot)] = nil
        return false
    end
    -- A full inventory is no failure of a needed pickup (a key or a <use>
    -- item, m.needed: 401's red book, three passes with every slot used):
    -- it waits for the inventory to change however often it was tried.
    local needed = m.needed or (m.item and full_mark_needed(bot, m.item))
    if m.permanent or (m.attempts >= MAX_ATTEMPTS and not needed) then
        return true
    end
    if m.hold and bot:tick() >= m.hold then
        return false
    end
    return BotPlanner.mark_holds(m, inventory_sig(bot), (plan().entries or {})[room], remote)
end

-- Whether a plain mark `m` (no desk, gate, permanent flag or hold) still
-- holds, given the inventory signature `sig` and this entry count of its
-- room. A mark lifts once the inventory changes; a failed approach also
-- lifts on a later entry into its room. Remote planning never heads back
-- for an item whose approach failed, even after the inventory changed: a
-- new item does not make an unreachable zone reachable. (A door keeps the
-- inventory rule: route() reads door marks too.)
function BotPlanner.mark_holds(m, sig, entry, remote)
    local approach = m.reason:find("approach", 1, true) ~= nil
    local hurt = m.reason:find("hurt while trying", 1, true) ~= nil
    if remote and (approach or hurt) and m.kind == "item" then
        return true
    end
    if m.sig ~= sig then
        return false
    end
    return remote or not approach or m.entry == entry
end

-- Whether the planner has given up on the frontier item `goal` for a
-- reason other than a full inventory (the inventory rules ask which pickups
-- still need a free slot).
local function gave_up(goal)
    local m = plan().marks[mark_key("item", goal.room, goal.slot)]
    if not m or m.reason:find("inventory full", 1, true) then
        return false
    end
    -- A locked drawer needs no slot until a key or lockpick lifts its
    -- mark (blocked() drops it then).
    if m.desk then
        return true
    end
    return m.permanent or m.attempts >= MAX_ATTEMPTS
end

-- Unreachable marks of this room are stale after a warp (a stair climb).
-- With `doors`, so are the door marks of every room: a puzzle step in one
-- room can open the way to a door in another (301's refilled pool stops
-- 302's waterfall, the way to its door to 307, which had used up its
-- attempts before). A door whose goal timed out counts as unreachable
-- too: 307's door to 30B ran out its time budget cycling approaches
-- blocked by the chasm, twice, and stayed given up after the hex crank
-- turned the bridge (sandwich=yes ended on an empty frontier).
local function clear_unreachable(bot, room, doors)
    for key, m in pairs(plan().marks) do
        if m.reach_state ~= nil then
            -- A door with a <reach> fact: its flag decides (blocked).
        elseif ((m.room == room or (doors and m.kind == "door"))
                and m.reason:find("approach", 1, true))
            or (doors and m.kind == "door" and not m.permanent
                and m.reason:find("timed out", 1, true)) then
            plan().marks[key] = nil
        elseif doors and m.event and m.room == room then
            -- So may a door of this room a room event had made a message:
            -- 302's battery powers the elevator whose call showed 132.
            plan().marks[key] = nil
        elseif doors and m.room == room and m.reason:find("hurt while trying", 1, true) then
            -- And an item the bot was hurt going for: the step may have
            -- ended what hurt it. 513's master key was marked while the
            -- Tyrant still stood, two ticks before it fell, and the key
            -- the lab's exit needs stayed on the floor.
            plan().marks[key] = nil
        end
    end
end

local function progress(bot, note)
    local p = plan()
    p.progress = bot:tick()
    if note then
        bot:log("progress: " .. note)
    end
end

-- What counts as real progress: rooms entered, items taken, doors
-- unlocked or newly there (a room event that turns a zone into a door, as
-- 411's lock switch does for 40E's door to 410) and the inventory. Walking
-- between known rooms changes none.
local function progress_sig(bot, world)
    local visited, taken, unlocked = 0, 0, 0
    for _, r in ipairs(world.rooms) do
        if r.visited then
            visited = visited + 1
        end
        for _, door in ipairs(r.doors) do
            if door.unlocked then
                unlocked = unlocked + 1
            end
            if door.present and door.target then
                unlocked = unlocked + 1
            end
        end
        for _, item in ipairs(r.items) do
            if item.taken then
                taken = taken + 1
            end
        end
    end
    return string.format("%d/%d/%d/%s", visited, taken, unlocked, inventory_sig(bot))
end

-- Real progress: the room repeat counts and the pursued target start over.
local function real_progress(bot, note)
    local p = plan()
    p.since = {}
    if p.target then
        bot:log(string.format("progress (%s): target %s re-ranked", note, p.target.label))
        p.target = nil
    end
end

-- The loop rule for one pursued target, `rec = {best, stalls}`, entered a
-- room whose route to it takes `steps` doors: a route no shorter than the
-- shortest seen is a stall. Returns true once the target is looping.
function BotPlanner.pursue(rec, steps)
    if rec.best == nil or steps < rec.best then
        rec.best = steps
    else
        rec.stalls = (rec.stalls or 0) + 1
    end
    return (rec.stalls or 0) >= MAX_STALLS
end

-- The room repeat rule: count one more entry of `room` since the last real
-- progress in `counts`. Returns "ok", "loop" (mark the pursued goal) or
-- "finish", and the count.
function BotPlanner.repeat_check(counts, room)
    counts[room] = (counts[room] or 0) + 1
    local n = counts[room]
    if n > 2 * MAX_REPEATS then
        return "finish", n
    end
    if n > MAX_REPEATS then
        return "loop", n
    end
    return "ok", n
end

-- Mark a goal looping: held off for LOOP_HOLD or until the inventory
-- changes, and counted as an attempt.
local function mark_loop(bot, kind, room, slot, why)
    mark(bot, kind, room, slot, "looping: " .. why)
    plan().marks[mark_key(kind, room, slot)].hold = bot:tick() + LOOP_HOLD
end

local function has_lockpick(bot)
    local flag = C.LOCKPICK_FLAG
    return bot:has_item(C.LOCKPICK) or bot:flag(flag.bank, flag.bit)
end

-- Why the drawer `desk` (a bot:actions() desk row) cannot be opened now, or
-- nil when a press opens it. check_desk opens a drawer whose bank-2 lock
-- bit is set; a clear one needs the desk key or the lockpick (the press
-- then asks to use it, and a second press opens the drawer). An empty
-- drawer refuses the press, so it is no goal.
function BotPlanner.desk_blocker(bot, desk)
    if desk.full == false then
        return "the drawer is empty"
    end
    if bot:flag(desk.lock_bank, desk.lock_bit) then
        return nil
    end
    if bot:has_item(C.DESK_KEY) or has_lockpick(bot) then
        return nil
    end
    return string.format("drawer locked: no desk key 0x%02X or lockpick", C.DESK_KEY)
end

-- The drawers of the action list, by the slot of the item each holds.
function BotPlanner.drawers(actions)
    local by_item = {}
    for _, action in ipairs(actions) do
        if action.kind == "desk" and action.item_slot ~= nil then
            by_item[action.item_slot] = action
        end
    end
    return by_item
end

-- Whether a door with key byte `key` can be opened now when it is locked,
-- and why not. Special keys (see the README's door rules): KEY_FAR_SIDE
-- opens on a press, KEY_NEVER never, KEY_NONE only by a room event.
local function key_opens(bot, key)
    if key == C.KEY_FAR_SIDE then
        return true
    end
    if key == C.KEY_NEVER then
        return false, "locked for good", true
    end
    if key == C.KEY_NONE then
        return false, "locked by a room event"
    end
    if key == C.SWORD_KEY and has_lockpick(bot) then
        return true
    end
    if bot:has_item(key) then
        return true
    end
    return false, string.format("needs key 0x%02X", key)
end

-- A live door's current lock state.
local function live_locked(bot, door)
    local lock = door.lock or 0
    return lock >= C.LOCK_LOCKED and not bot:flag(C.LOCK_BANK, lock % C.LOCK_SPAN)
end

local function world_room(world, id)
    for _, room in ipairs(world.rooms) do
        if room.id == id then
            return room
        end
    end
    return nil
end

-- Whether a remembered door (a world row) can be walked through.
-- The <door slot via> fact of door `slot` in room `room_id`, or nil: a door
-- the room parks at its origin until AOT `via` is pressed.
local function via_fact(bot, room_id, slot)
    for _, fact in ipairs(bot:door_facts()) do
        if fact.room == room_id and fact.slot == slot
            and (fact.gate_bank == nil
                or bot:flag(fact.gate_bank, fact.gate_bit) == fact.gate_set) then
            return fact
        end
    end
    return nil
end

-- The plan().revisit_seen key of a <revisit> fact (bot:revisit_facts()).
local function revisit_key(fact)
    return string.format("%s|%d|%d|%s", fact.room, fact.bank, fact.bit, tostring(fact.set))
end

-- Whether the bot's map has room `id` as visited.
local function visited_room(world, id)
    for _, r in ipairs(world.rooms or {}) do
        if r.id == id then
            return r.visited == true
        end
    end
    return false
end

-- Whether an <avoid> fact holds door `slot` of room `room_id` shut now:
-- an ungated one always, a gated one while its gate holds (302's north
-- elevator before the power is on). Remote planning reads it; in the room
-- BotPuzzle.avoided does.
local avoid_cache = { tick = nil, facts = nil }
local function avoid_holds(bot, room_id, slot)
    -- Routes ask once per door: read the facts once per tick.
    if avoid_cache.tick ~= bot:tick() then
        avoid_cache.tick, avoid_cache.facts = bot:tick(), bot:avoid_facts()
    end
    for _, fact in ipairs(avoid_cache.facts) do
        if fact.room == room_id and fact.slot == slot
            and BotPuzzle.gate_holds(bot, { bank = fact.bank, bit = fact.bit,
                set = fact["while"] == "set", unless_held = fact.unless_held }) then
            return true
        end
    end
    return false
end

-- The live action the bot presses to open this room's `door`: a <door via>
-- fact names it, and the door is parked at the origin (406's hatch) or the
-- fact's gate holds wherever the door sits (302's ladder, 301's elevator).
local function via_action(bot, door)
    local fact = via_fact(bot, bot:room().id, door.slot)
    if not fact or (fact.gate_bank == nil and not BotNav.is_origin_zone(door)) then
        return nil
    end
    return BotNav.action_by_slot(bot, fact.via)
end

-- Whether room `room_id`, entered from room `from`, carries the player out
-- through its door `slot` by itself (a <ride> fact: 515's lift).
local function rides_out(bot, room_id, slot, from)
    for _, fact in ipairs(bot:ride_facts()) do
        if fact.room == room_id and fact.slot == slot and fact.from == from then
            return true
        end
    end
    return false
end

-- `from`: the room the route enters `room_id` from, when known.
local function door_passable(bot, room_id, door, from)
    if not door.present or not door.target then
        return false
    end
    -- A door zone parked at the room origin is a scripted transfer (a
    -- cutscene's exit), not a door to walk through, unless a <door via>
    -- fact names the action that sets it around the player. A <ride>
    -- passes through it on a route entering the room from its `from` room;
    -- in the ride room itself the bot only waits for it.
    if door.x == 0 and door.z == 0 and not via_fact(bot, room_id, door.slot) then
        return from ~= nil and room_id ~= bot:room().id
            and rides_out(bot, room_id, door.slot, from)
    end
    if blocked(bot, "door", room_id, door.slot, true) then
        return false
    end
    -- Nor is a door a <reach> fact keeps out of reach (20C's door to 21A
    -- from anywhere but the pit).
    if reach_closed(bot, room_id, door.slot) then
        return false
    end
    -- A door an <avoid> holds shut is no way through either, while its
    -- gate holds: 214's door to 201 under rope=leave, which reads message
    -- 128 until 201's key panel has the pass number, was the first door of
    -- every route back into the mansion.
    if avoid_holds(bot, room_id, door.slot) then
        return false
    end
    if door.locked and not door.unlocked then
        return (key_opens(bot, door.key))
    end
    return true
end

-- The remembered zone centre {x, z} of action `slot` in room `id`, or nil.
function BotPlanner.action_point(world, id, slot)
    for _, row in ipairs(world.rooms or {}) do
        if row.id == id then
            for _, action in ipairs(row.actions or {}) do
                if action.slot == slot and action.x then
                    return { x = action.x, z = action.z }
                end
            end
        end
    end
    return nil
end

-- The mental map. Every room the bot has seen is remembered with its doors
-- (zone centre x, z and target) and its actions (item boxes with their zone
-- centre), in bot:world(). A route is priced by walking distance through
-- it: from the bot to a door here (`opts.exit_len(slot)`, the planned path
-- length), MAP_DOOR for each door load, then in each room from where the bot
-- arrives to the next door, and last to the goal's point. The arrival point
-- in room T from room R is where the bot last stood on entering T from R
-- (`opts.arrivals[T][R]`), else T's door back to R. Remote legs are straight
-- lines; nothing is learned from earlier choices.

local function map_dist(ax, az, bx, bz)
    local dx, dz = ax - bx, az - bz
    return (dx * dx + dz * dz) ^ 0.5 // 1
end

-- The point where the bot arrives in room row `row` from room `from_id`, or
-- nil when the map holds none.
function BotPlanner.entry_point(row, from_id, arrivals)
    local seen = arrivals and arrivals[row.id] and arrivals[row.id][from_id]
    if seen then
        return seen.x, seen.z
    end
    for _, door in ipairs(row.doors or {}) do
        if door.target == from_id and door.present ~= false then
            return door.x, door.z
        end
    end
    return nil
end

-- Dijkstra from the bot in room `from` over the map. A node is a room
-- entered from a given room (its arrival point). Returns the tree that
-- BotPlanner.tree_route reads. `opts.passable(room_id, door, from)` filters the
-- doors; `opts.exit_len(slot)` prices the doors of `from`.
function BotPlanner.map_tree(world, from, opts)
    local rows = {}
    for _, row in ipairs(world.rooms or {}) do
        rows[row.id] = row
    end
    local nodes = { { room = from, cost = 0 } }
    local best = { [from .. "|"] = 1 }
    local done = {}
    while true do
        local pick = nil
        for i, node in ipairs(nodes) do
            if not done[i] and (not pick or node.cost < nodes[pick].cost) then
                pick = i
            end
        end
        if not pick then
            break
        end
        done[pick] = true
        local node = nodes[pick]
        local row = rows[node.room]
        for _, door in ipairs(row and row.doors or {}) do
            local target = door.target
            if target and target ~= node.room and door.x
                and opts.passable(node.room, door, node.step and node.step.room) then
                local leg
                local from_room = node.step and node.step.room
                local legs = opts.legs and from_room and opts.legs[node.room]
                    and opts.legs[node.room][from_room]
                if node.prev == nil then
                    leg = opts.exit_len(door.slot)
                elseif legs and legs[door.slot] then
                    leg = legs[door.slot]
                elseif node.x then
                    leg = map_dist(node.x, node.z, door.x, door.z)
                else
                    leg = 0
                end
                local cost = node.cost + leg + MAP_DOOR
                local key = target .. "|" .. node.room
                local seen = best[key]
                if not seen or (not done[seen] and cost < nodes[seen].cost) then
                    local x, z = nil, nil
                    if rows[target] then
                        x, z = BotPlanner.entry_point(rows[target], node.room, opts.arrivals)
                    end
                    local next_node = { room = target, cost = cost, x = x, z = z, prev = pick,
                        step = { room = node.room, slot = door.slot, target = target } }
                    if seen then
                        nodes[seen] = next_node
                    else
                        nodes[#nodes + 1] = next_node
                        best[key] = #nodes
                    end
                end
            end
        end
    end
    return { nodes = nodes, from = from }
end

-- The cheapest route in `tree` into room `to`, ending at `point` ({x, z},
-- or nil for the arrival): its cost and its {room, slot, target} steps, or
-- nil when the map reaches no entry of `to`.
function BotPlanner.tree_route(tree, to, point)
    local best, best_cost = nil, nil
    for i, node in ipairs(tree.nodes) do
        if node.room == to and node.prev ~= nil then
            local cost = node.cost
            if point and point.x and node.x then
                cost = cost + map_dist(node.x, node.z, point.x, point.z)
            end
            if not best_cost or cost < best_cost then
                best, best_cost = i, cost
            end
        end
    end
    if not best then
        return nil
    end
    local steps = {}
    local i = best
    while tree.nodes[i].prev ~= nil do
        table.insert(steps, 1, tree.nodes[i].step)
        i = tree.nodes[i].prev
    end
    return best_cost, steps
end

-- map_tree's options for the bot: the doors it may walk through, the
-- arrival points it remembers, and `exit_len(slot)` for the doors here
-- (default: the straight line from where it stands).
local function map_opts(bot, exit_len)
    return {
        passable = function(id, door, from) return door_passable(bot, id, door, from) end,
        exit_len = exit_len or function(slot)
            local door = BotNav.door_by_slot(bot, slot)
            return door and bot:distance(BotNav.centre(door)) or UNPLANNED_COST
        end,
        arrivals = plan().arrivals,
        legs = plan().legs,
    }
end

-- The route through the mental map from room `from` (the bot's room) into
-- room `to`, ending at `point` (a {x, z} row, or nil): the list of
-- {room, slot, target} steps and its walking cost, or nil. `tree` is a
-- map_tree from `from` to reuse.
local function map_route(bot, world, from, to, tree, point)
    if from == to then
        return {}, 0
    end
    tree = tree or BotPlanner.map_tree(world, from, map_opts(bot))
    local cost, steps = BotPlanner.tree_route(tree, to, point)
    return steps, cost
end

-- Breadth-first route over the remembered doors: the list of
-- {room, slot, target} steps from `from` to `to`, or nil.
local function route(bot, world, from, to)
    if from == to then
        return {}
    end
    local prev = { [from] = false }
    local queue = { from }
    local head = 1
    while queue[head] do
        local id = queue[head]
        head = head + 1
        local room = world_room(world, id)
        for _, door in ipairs(room and room.doors or {}) do
            local target = door.target
            if target and prev[target] == nil
                and door_passable(bot, id, door, prev[id] and prev[id].room or nil) then
                prev[target] = { room = id, slot = door.slot, target = target }
                if target == to then
                    local steps = {}
                    local step = prev[target]
                    while step do
                        table.insert(steps, 1, step)
                        step = prev[step.room]
                    end
                    return steps
                end
                queue[#queue + 1] = target
            end
        end
    end
    return nil
end

-- Goal constructors. Each goal has a kind, a label and a press job.
local function zone_of(row)
    return { x = row.x, z = row.z, width = row.width, depth = row.depth }
end

local function camera_goal(bot, fact, index, count)
    local label = string.format("step %d/%d camera %d (%s)", index, count,
        fact.cut, fact.label or "unnamed")
    return {
        kind = "camera", slot = "camera" .. fact.cut, label = label,
        job = {
            camera = fact.cut, label = label,
            done = function(b, job)
                if fact.bank then
                    return b:flag(fact.bank, fact.bit)
                end
                if job.reached then
                    BotPuzzle.visited(index)
                    return true
                end
                return false
            end,
        },
    }
end

local function interact_goal(bot, fact, index, count)
    if fact.kind == "camera" then
        return camera_goal(bot, fact, index, count)
    end
    if fact.kind == "fight" then
        local label = string.format("fight %d/%d enemy 0x%02X (%s)", index, count,
            fact.enemy, fact.label or "unnamed")
        return {
            kind = "interact", slot = "fight" .. fact.enemy, label = label,
            job = {
                fight = true, enemy = fact.enemy, label = label,
                timeout = FIGHT_TIMEOUT,
                done = function(b) return BotPuzzle.flag_done(b, fact) end,
            },
        }
    end
    if fact.kind == "wait" then
        local label = string.format("wait %d/%d for flag %d:0x%02X (%s)", index, count,
            fact.bank, fact.bit, fact.label or "unnamed")
        return {
            kind = "interact", slot = BotPuzzle.step_key(fact), label = label,
            job = {
                wait = true, label = label, timeout = WAIT_TIMEOUT,
                done = function(b) return BotPuzzle.flag_done(b, fact) end,
            },
        }
    end
    local action = BotNav.action_by_slot(bot, fact.slot)
    if not action then
        return nil, string.format("interact slot %d is not a live action", fact.slot)
    end
    if fact.kind == "climb" then
        local label = string.format("climb %d/%d object %d to slot %d (%s)", index, count,
            fact.object, fact.slot, fact.label or "unnamed")
        return {
            kind = "interact", slot = fact.slot, label = label,
            job = {
                climb = true, object = fact.object, zone = zone_of(action), label = label,
                taken = function(b) return BotPuzzle.flag_done(b, fact) end,
                -- An earlier <push> step of this entry placed the object.
                placed = BotPuzzle.placed_before(index),
            },
        }
    end
    if fact.kind == "push" then
        local label = string.format("push %d/%d object %d onto slot %d (%s)", index, count,
            fact.object, fact.slot, fact.label or "unnamed")
        return {
            kind = "interact", slot = fact.slot, label = label,
            job = {
                push = true, object = fact.object, zone = zone_of(action), label = label,
                dir = BotPuzzle.push_dir(fact), only_axis = fact.axis, step = index,
                done = function(b) return BotPuzzle.push_done(b, fact) end,
            },
        }
    end
    local label = string.format("interact %d/%d slot %d (%s)", index, count,
        fact.slot, fact.label or "unnamed")
    -- A body-probe zone fires on the player's position, not the reach point
    -- (40C's fireplace, SAT_UNDER): stand inside it.
    local zone = zone_of(action)
    zone.inside = (action.flags // C.PROBE_BODY) % 2 == 1
    return {
        kind = "interact", slot = fact.slot, label = label,
        job = {
            zone = zone, label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND,
            done = function(b) return BotPuzzle.flag_done(b, fact) end,
        },
    }
end

-- A <deliver> here with its item held: walk into its camera cut; the room
-- script takes the item there and sets the done flag.
local function deliver_goal(bot, fact)
    local label = string.format("deliver 0x%02X into camera %d (%s)", fact.item, fact.cut,
        fact.label or "unnamed")
    return {
        kind = "deliver", slot = "camera" .. fact.cut, label = label,
        job = {
            camera = fact.cut, label = label,
            done = function(b) return b:flag(fact.bank, fact.bit) end,
        },
    }
end

-- A <fetch> here that a wanted <deliver> needs: press its zone until the
-- item is held (or the offer is gone).
local function fetch_goal(bot, fact)
    local action = BotNav.action_by_slot(bot, fact.slot)
    if not action then
        return nil, string.format("fetch slot %d is not a live action", fact.slot)
    end
    local label = string.format("fetch 0x%02X at slot %d (%s)", fact.item, fact.slot,
        fact.label or "unnamed")
    return {
        kind = "fetch", slot = fact.slot, label = label,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND,
            done = function(b)
                return b:has_item(fact.item) or not BotPuzzle.fetch_offered(b, fact)
            end,
        },
    }
end

-- How long a used item may take to show its done flag once the menu closed:
-- the room event (a scene) runs in between.
local USE_SETTLE = 20 * C.SECOND

-- A <use> fact: stand where the room's use-item probe in `action` arms the
-- item (the reach point in the zone, or the player inside it for a body
-- probe), wait for the item's use flag, USE it through the menu, then wait
-- for the done flag while the room event plays.
local function use_goal(bot, fact, action)
    local label = string.format("use 0x%02X at slot %d (%s)", fact.item, fact.slot,
        fact.label or "unnamed")
    local zone = zone_of(action)
    zone.inside = (action.flags // C.PROBE_BODY) % 2 == 1
    local armed = fact.item - C.ITEM_USE_BASE
    return {
        kind = "use", slot = fact.slot, label = label,
        job = {
            zone = zone, label = label, pattern = "hold",
            side = 3 * C.SECOND, timeout = GOAL_TIMEOUT,
            done = function(b) return BotPuzzle.use_done(b, fact) end,
            hold = function(b, job)
                if not job.used and b:flag(C.ITEM_USE_BANK, armed)
                    and BotMenu.use_item(b, fact.item, label) then
                    job.used = b:tick()
                    job.settled = true
                end
            end,
            check = function(b, job)
                if not job.used or BotMenu.busy() then
                    return nil
                end
                local last = BotMenu.last()
                if last and last.item == fact.item and last.tick >= job.used and not last.ok then
                    return "menu use failed: " .. tostring(last.reason)
                end
                job.waited = (job.waited or 0) + 1
                if job.waited > USE_SETTLE then
                    -- The item went in, so the room took it: a turning
                    -- piece (30F's hex crank) steps once a use. Try again
                    -- now; MAX_ATTEMPTS still bounds a use that does nothing.
                    job.retry = true
                    return "used; the done flag stayed clear"
                end
                return nil
            end,
        },
    }
end

-- A <brew> step (BotMix.next_step): fill an empty bottle at a tap (press
-- its event zone; the scene handler answers Yes at its prompt) or MOVE one
-- chemical onto another through the menu.
local function brew_goal(bot, fact, step)
    if step.kind == "fill" then
        local action = BotNav.action_by_slot(bot, step.tap.slot)
        if not action then
            return nil, string.format("tap slot %d is not a live action", step.tap.slot)
        end
        local before = BotMix.count(bot, step.item)
        local bottles = BotMix.count(bot, fact.bottle)
        local label = string.format("brew 0x%02X: fill 0x%02X at slot %d (%s)", fact.item,
            step.item, step.tap.slot, step.tap.label or "unnamed")
        return {
            kind = "brew", slot = step.tap.slot, label = label,
            job = {
                zone = zone_of(action), label = label, pattern = "edge",
                presses = 2, gap = 20, side = 4 * C.SECOND,
                -- A spent bottle ends it too: the award has landed by
                -- the time the job runs again, and a press that reached a
                -- neighbouring tap made a chemical the next plan uses.
                done = function(b)
                    return BotMix.count(b, step.item) > before
                        or BotMix.count(b, fact.bottle) < bottles
                end,
            },
        }
    end
    local label = string.format("brew 0x%02X: mix 0x%02X onto 0x%02X (menu slots %d, %d)",
        fact.item, step.a, step.b, step.slot, step.target)
    return {
        kind = "brew", slot = "mix", label = label,
        job = {
            menu = "combine", label = label,
            start = function(b) return BotMenu.combine(b, step.slot, step.target, label) end,
        },
    }
end

-- An item box visit: press its zone; BotInventory drives the box screen
-- the press opens, and the goal is done once that visit has ended.
local function box_goal(bot, action, why)
    local label = string.format("item box slot %d (%s)", action.slot, why)
    local look = bot:memory().box_items == nil
    local since = bot:tick()
    return {
        kind = "box", slot = action.slot, label = label, sig = inventory_sig(bot), look = look,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 20, side = 4 * C.SECOND, timeout = GOAL_TIMEOUT,
            done = function() return BotInventory.visited_since(since) end,
        },
    }
end

-- A MOVE through the menu that empties a slot (BotInventory.combine_now).
local function combine_goal(bot, pick)
    local label = string.format("combine 0x%02X onto 0x%02X (menu slots %d, %d)",
        pick.item, pick.other, pick.slot, pick.target)
    return {
        kind = "combine", slot = "mix", label = label,
        job = {
            menu = "combine", label = label,
            start = function(b) return BotMenu.combine(b, pick.slot, pick.target, label) end,
        },
    }
end

-- A heal used through the menu to free a slot for a pickup here
-- (BotInventory.heal_for_room).
local function heal_room_goal(bot, item)
    local label = string.format("use heal 0x%02X to make room", item)
    return {
        kind = "heal", slot = "room", label = label,
        job = {
            menu = "heal", label = label,
            start = function(b) return BotMenu.use_heal(b, item, label) end,
        },
    }
end

-- A held doom book is opened in the menu's CHECK viewer, which turns it
-- into its medal (Doom Book II the Wolf Medal, Doom Book I the Eagle).
local DOOM_BOOKS = { C.DOOM_BOOK_2, C.DOOM_BOOK_2 + 1 }

local function examine_goal(bot, item)
    local label = string.format("open doom book 0x%02X", item)
    return {
        kind = "examine", slot = item, label = label,
        job = {
            menu = "examine", label = label,
            start = function(b) return BotMenu.examine(b, item, label) end,
        },
    }
end

-- One tick of a menu goal: start the menu job, then read its outcome.
local function menu_step(bot, job)
    if not job.started then
        if not job.start(bot) then
            job.reason = "the menu job did not start"
            return "failed"
        end
        job.started = bot:tick()
        return "busy"
    end
    if BotMenu.busy() then
        return "busy"
    end
    local last = BotMenu.last(job.menu)
    if last and last.tick >= job.started then
        if last.ok then
            return "done"
        end
        job.reason = job.menu .. " failed: " .. tostring(last.reason)
        return "failed"
    end
    job.reason = "the " .. job.menu .. " job left no result"
    return "failed"
end

-- `twin`: the slot of a taken item of the same id whose zone centre this
-- zone holds. It may be a second AOT of that pickup, so it gets one
-- approach: nothing there marks it for good.
local function item_goal(bot, action, twin)
    local before = bot:item_count(action.item)
    local label = string.format("item 0x%02X slot %d", action.item, action.slot)
    return {
        kind = "item", slot = action.slot, label = label, item = action.item,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 3, gap = 15, side = 2 * C.SECOND,
            check = twin and function(b, job)
                if job.index > 1 and job.pressed then
                    job.permanent = true
                    return string.format("a second zone of the item taken at slot %d", twin)
                end
                return nil
            end or nil,
            done = function(b, job)
                if b:item_count(action.item) > before then
                    return true
                end
                local now = BotNav.action_by_slot(b, action.slot)
                return job.pressed and (not now or now.kind ~= "item"
                    or now.item ~= action.item)
            end,
        },
    }
end

-- A drawer: press its zone, answer Yes to the key prompt (a locked drawer)
-- and to the take prompt (the scene handler answers both), until the item
-- is in the inventory or the drawer reads empty. Marks go on the item's
-- slot, so remote planning sees the same goal as the world model's item.
local function desk_goal(bot, desk)
    local item = desk.desk_item
    local before = bot:item_count(item)
    local label = string.format("drawer slot %d item 0x%02X slot %d", desk.slot, item,
        desk.item_slot)
    return {
        kind = "item", slot = desk.item_slot, press_slot = desk.slot, label = label,
        item = item,
        job = {
            -- The lid opens after a camera pan of about a second; a locked
            -- drawer needs one press for the key and one to open.
            zone = zone_of(desk), label = label, pattern = "edge",
            presses = 3, gap = 2 * C.SECOND, side = 8 * C.SECOND, timeout = GOAL_TIMEOUT,
            done = function(b, job)
                if b:item_count(item) > before then
                    b:log(string.format("%s: opened, took item 0x%02X (%d -> %d)", label, item,
                        before, b:item_count(item)))
                    return true
                end
                local now = BotNav.action_by_slot(b, desk.slot)
                if job.pressed and (not now or now.full == false) then
                    b:log(string.format("%s: opened, the drawer reads empty", label))
                    return true
                end
                return false
            end,
        },
    }
end

local function door_goal(bot, door, why)
    local label = string.format("door slot %d -> %s (%s)", door.slot,
        tostring(door.target), why)
    local via = via_action(bot, door)
    if via then
        -- Press the action that opens it; its event sets the door around
        -- the player (after a prompt the room's <answer> settles).
        local room_id = bot:room().id
        label = string.format("%s via slot %d", label, via.slot)
        -- A body probe (302's elevator call) fires on the player standing
        -- in it: walk in rather than press from its edge.
        local zone = zone_of(via)
        zone.inside = (via.flags // C.PROBE_BODY) % 2 == 1
        return {
            kind = "door", slot = door.slot, target = door.target, label = label,
            was_locked = false,
            job = {
                zone = zone, label = label, pattern = "edge",
                presses = 2, gap = 20, side = 4 * C.SECOND, timeout = GOAL_TIMEOUT,
                -- Done once the room changes, or once the event took the
                -- player to the door without leaving (305's fountain stairs
                -- end on its floor, where the gate no longer holds): the
                -- door is then an ordinary one.
                done = function(b)
                    return b:room().id ~= room_id or not via_action(b, door)
                end,
            },
        }
    end
    return {
        kind = "door", slot = door.slot, target = door.target, label = label,
        -- A locked door's press opens the key's message: not an event's.
        was_locked = live_locked(bot, door),
        job = {
            zone = zone_of(door), label = label, pattern = "door",
            side = 4 * C.SECOND - 10, timeout = GOAL_TIMEOUT,
        },
    }
end

local function stair_goal(bot, action)
    local label = string.format("%s check slot %d",
        action.kind == "event" and "passage" or "stair", action.slot)
    return {
        kind = "stair", slot = action.slot, label = label,
        job = {
            zone = zone_of(action), label = label, pattern = "edge",
            presses = 2, gap = 40, side = 10 * C.SECOND,
            timeout = action.kind == "event" and PASSAGE_TIMEOUT or nil,
            -- The climb is a warp: one tick's jump after the press. A
            -- ladder (`stairs_zone`, started by the press or by leaning
            -- into it) carries the player over a few seconds instead: done
            -- once it has moved that far from the approach point and
            -- control is back.
            done = function(b, job)
                local q = b:player()
                local last = job.last_pos
                if job.pressed and last ~= nil
                    and BotNav.abs(q.x - last[1]) + BotNav.abs(q.z - last[2])
                        > C.WARP_DISTANCE then
                    return true
                end
                local at = job.arrived_at
                return at ~= nil and b:scene().controllable
                    and BotNav.abs(q.x - at[1]) + BotNav.abs(q.z - at[2]) > C.WARP_DISTANCE
            end,
        },
    }
end

-- A passage check here: a stair zone (`door_check`), a ladder
-- (`stairs_zone`), or an action-key
-- event zone no fact names (a room may start its stair climb from an
-- event, as when a script turns the stair zone on from the event). One try
-- per slot per entry, nearest first.
local function passage_goal(bot, room)
    local p = plan()
    p.stairs[room] = p.stairs[room] or {}
    local stair, stair_len = nil, nil
    -- A slot that is a door to another room is no passage within this one
    -- (307's ladder up to 302: the check climbed it on every entry, and
    -- the route back down came straight back).
    local exits = {}
    for _, door in ipairs(bot:doors()) do
        if door.target and door.target ~= room then
            exits[door.slot] = true
        end
    end
    for _, action in ipairs(bot:actions()) do
        local passage = action.kind == "door_check" or action.kind == "stairs_zone"
            or (action.kind == "event" and action.flags >= C.ACTION_KEY
                and not BotPuzzle.is_fact_slot(bot, action.slot))
        if passage and not exits[action.slot] and not p.stairs[room][action.slot]
            and not blocked(bot, "stair", room, action.slot)
            and not BotPuzzle.avoided(bot, action.slot) then
            local len = BotNav.reach_length(bot, action)
                or UNPLANNED_BASE + bot:distance(BotNav.centre(action))
            if not stair_len or len < stair_len then
                stair, stair_len = action, len
            end
        end
    end
    if stair then
        p.stairs[room][stair.slot] = true
        return stair_goal(bot, stair)
    end
    return nil
end

local function frontier_goal(world, target)
    for _, goal in ipairs(world.frontier) do
        if goal.kind == target.kind and goal.room == target.room and goal.slot == target.slot then
            return goal
        end
    end
    return nil
end

-- The memory key of a <fight> seen won in its own room (fight_due).
local function fight_won_key(room, enemy)
    return string.format("fight_won:%s:%d", room, enemy)
end

-- A pursued target's goal row while it is still worth pursuing, or nil: a
-- frontier door or item, or a <use> fact whose item is held and whose done
-- flag is clear.
-- Whether to head back to the room of the <fight> fact `fight`: it is
-- still ahead, a step leading into it was held back there (an interact
-- mark `not ready for the fight`), and the bot is ready now (a heal
-- fetched from the box, rounds found). Fight steps are room-local, so
-- without this a frontier emptied elsewhere never returns to them (40C's
-- Plant 42 after its heal trip to 403). A held-back fight won while the
-- bot was away is due once as well (see below).
local function fight_due(bot, fight)
    if fight.bank and bot:flag(fight.bank, fight.bit) then
        -- Won while the bot was away from a fight it was held back from:
        -- due once, for the steps the win opens. In Barry's chemical
        -- branch his flamethrower finishes Plant 42 after the bot has
        -- left 40C short of rounds, and the fireplace's helmet key (the
        -- step after the fight) was never fetched.
        local memory = bot:memory()
        return memory[BotNav.fight_met_key(fight.room, fight.enemy)] ~= nil
            and memory[fight_won_key(fight.room, fight.enemy)] == nil
    end
    -- An optional fight the bot is not armed for is passed, not due (30C's
    -- Black Tiger, dodged while the knife cut the web: the bot went back
    -- from 30D to fight it with the shotgun).
    if fight.optional and not BotPuzzle.armed_for(bot, fight.enemy) then
        return false
    end
    -- Forced from an empty frontier (forgive_fight): due, ready or not.
    local forced = plan().forced_fight
    if forced and forced.room == fight.room
        and ((plan().entries or {})[fight.room] or 0) < forced.entry then
        return true
    end
    -- A fight step that found its enemy and left it alive counts too: the
    -- bot left the room short of rounds or health, or a reload undid the
    -- fight and the marks with it (40C's Plant 42 after a death in 302).
    local held_back = bot:memory()[BotNav.fight_met_key(fight.room, fight.enemy)] ~= nil
    for _, m in pairs(plan().marks) do
        if m.kind == "interact" and m.room == fight.room
            and m.reason:find("not ready for the fight", 1, true) then
            held_back = true
        end
    end
    if not (held_back and BotSurvival.fight_ready(bot, fight)) then
        return false
    end
    -- An earlier fight of the room's sequence not ready holds it: the web's
    -- knife was at hand, the Black Tiger before it was not, and the bot
    -- walked 30B and 30C until the loop rule ended the run.
    for _, other in ipairs(bot:fight_facts()) do
        if other.room == fight.room and other.enemy == fight.enemy then
            break
        end
        if other.room == fight.room and not (other.bank and bot:flag(other.bank, other.bit))
            and not (other.optional and not BotPuzzle.armed_for(bot, other.enemy))
            and not BotSurvival.fight_ready(bot, other) then
            return false
        end
    end
    return true
end

local function target_goal(bot, world, t)
    if t.kind == "fight" then
        if fight_due(bot, t.fact) then
            return { kind = "fight", room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "box" then
        if BotInventory.box_reason(bot, gave_up)
            and not blocked(bot, "box", t.room, t.slot, true) then
            return { kind = "box", room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "save" then
        if BotSave.trip_reason(bot) then
            return { kind = "save", room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "use" then
        if bot:has_item(t.item) and BotPuzzle.use_open(bot, t.fact or t)
            and not blocked(bot, "use", t.room, t.slot, true) then
            return { kind = "use", room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "deliver" or t.kind == "fetch" then
        local fact = t.fact
        local live
        if t.kind == "deliver" then
            live = BotPuzzle.deliver_wanted(bot, fact) and bot:has_item(fact.item)
        else
            live = BotPuzzle.wanted_items(bot)[fact.item] and not bot:has_item(fact.item)
                and BotPuzzle.fetch_offered(bot, fact)
        end
        if live and not blocked(bot, t.kind, t.room, t.slot, true) then
            return { kind = t.kind, room = t.room, slot = t.slot }
        end
        return nil
    end
    if t.kind == "revisit" then
        -- A room entered again for the story flags changed since
        -- (BotPlanner.revisit_room): done on entering it.
        return { kind = "revisit", room = t.room, slot = t.slot }
    end
    return frontier_goal(world, t)
end

-- Every inventory slot holds an item.
local function inventory_full(bot)
    local used = 0
    for _, row in ipairs(bot:inventory()) do
        if row.item ~= 0 then
            used = used + 1
        end
    end
    return used >= (bot:menu().capacity or used + 1)
end

-- A pickup of `item` fits: a free slot, or a held stack it merges into.
local function pickup_fits(bot, item, full)
    local kind, merge = bot:item_kind(item)
    if not full then
        -- A puzzle step held for a free slot keeps it: an item that no key,
        -- <use> or step needs leaves it (30E's herbs took the slot a box
        -- visit freed for 306's Doom Book, again and again).
        local held = BotPlanner.steps_needing_room(bot, true)
        if held == 0 or kind == "key" or kind == "file" or (merge and bot:has_item(item)) then
            return true
        end
        local used = 0
        for _, row in ipairs(bot:inventory()) do
            if row.item ~= 0 then
                used = used + 1
            end
        end
        local free = (bot:menu().capacity or used) - used
        return free > held or BotInventory.context(bot).uses[item] == true
    end
    -- A document goes to the FILE tab, not a slot.
    return kind == "file" or (merge and bot:has_item(item))
end

-- A live item zone that holds the centre of a zone of the same item the
-- bot took in this room is a second zone of that item (a pickup with two
-- AOTs): the slot of the taken one, or nil.
local function taken_twin(here, action)
    for _, item in ipairs(here and here.items or {}) do
        if item.taken and item.slot ~= action.slot and item.item == action.item
            and item.x >= action.x and item.x <= action.x + action.width
            and item.z >= action.z and item.z <= action.z + action.depth then
            return item.slot
        end
    end
    return nil
end

-- Is the door row `door` (a bot:doors() row) of this room untraversed?
local function untraversed(world_here, slot)
    for _, door in ipairs(world_here and world_here.doors or {}) do
        if door.slot == slot then
            return not door.traversed, door
        end
    end
    return true, nil
end

-- The way-out bonus of a route to the remote frontier item `item` (catalog
-- kind `kind`): ITEM_BONUS, plus KEY_BONUS for a key that a remembered
-- locked door needs (`needed[item]` names the door), else AMMO_BONUS for
-- the ammunition the equipped gun needs (`ammo`). Returns the bonus and a
-- label tag (nil for a plain item). A heal item gets the plain bonus,
-- unless `hurt` (the item heals and the bot needs a heal): HEAL_BONUS.
function BotPlanner.item_bonus(item, kind, ammo, needed, hurt, guns)
    if hurt and kind == "heal" then
        return ITEM_BONUS + HEAL_BONUS, "heal: hurt with none held"
    end
    if kind == "key" and needed and needed[item] then
        return ITEM_BONUS + KEY_BONUS, "key for " .. needed[item]
    end
    if ammo and item == ammo then
        return ITEM_BONUS + AMMO_BONUS, "ammunition"
    end
    -- Under the floor, a gun the bot has none of (`guns`,
    -- BotInventory.new_guns) brings its clip.
    if ammo and guns and guns[item] then
        return ITEM_BONUS + AMMO_BONUS, "a gun, the ammunition low"
    end
    return ITEM_BONUS, nil
end

-- The way-out bonus of a door into the unvisited room `id` for what its
-- <holds> facts (`holds`, bot:item_facts()) place there: the largest bonus
-- of an item the bot does not hold and still needs, one a <use> fact
-- (`uses`, bot:use_facts()) with its done flag clear needs
-- (ITEM_BONUS + USE_BONUS), or a needed key or the gun's ammunition
-- (BotPlanner.item_bonus). `held(item)` and `done(fact)` read the game.
-- Returns 0 and nil when nothing there is needed.
function BotPlanner.holds_bonus(id, holds, uses, held, done, kind_of, ammo, needed)
    local best, best_tag = 0, nil
    for _, fact in ipairs(holds) do
        if fact.room == id and not held(fact.item) then
            local bonus, tag = 0, nil
            for _, use in ipairs(uses) do
                if use.item == fact.item and not done(use) then
                    bonus, tag = ITEM_BONUS + USE_BONUS, "for " .. (use.label or "a use")
                end
            end
            if bonus == 0 then
                local extra, why = BotPlanner.item_bonus(fact.item, kind_of(fact.item), ammo, needed)
                if why then
                    bonus, tag = extra, why
                end
            end
            if bonus > best then
                best, best_tag = bonus, string.format("holds 0x%02X %s", fact.item, tag)
            end
        end
    end
    return best, best_tag
end

-- The row of `rows` with the shortest `len(row)` (nil: unplanned, after
-- every planned one), first in order on a tie, and its length.
function BotPlanner.nearest(rows, len)
    local best, best_len, best_sort = nil, nil, nil
    for _, row in ipairs(rows) do
        local n = len(row)
        local sort = n or UNPLANNED
        if not best or sort < best_sort then
            best, best_len, best_sort = row, n, sort
        end
    end
    return best, best_len
end

-- The remembered item box to head for from room `from`, over every other
-- room of `world`, by walking distance through the map
-- (BotPlanner.map_tree): returns {room, slot, cost, steps} or nil.
-- `opts` is map_tree's, plus `usable_box(room, action)`; `opts.tree`, when
-- given, is a map_tree already built from `from`.
function BotPlanner.pick_remote_box(world, from, opts)
    local tree = opts.tree or BotPlanner.map_tree(world, from, opts)
    local best = nil
    for _, r in ipairs(world.rooms) do
        for _, action in ipairs(r.id ~= from and r.actions or {}) do
            if action.kind == (opts.kind or "item_box") and action.x and not (action.x == 0 and action.z == 0)
                and opts.usable_box(r.id, action) then
                local cost, steps = BotPlanner.tree_route(tree, r.id, action)
                if steps and steps[1] and (not best or cost < best.cost) then
                    best = { room = r.id, slot = action.slot, cost = cost, steps = steps }
                end
            end
        end
    end
    return best
end

-- The live item boxes of this room the planner may press.
function BotPlanner.boxes_here(bot)
    local room = bot:room().id
    local out = {}
    for _, action in ipairs(bot:actions()) do
        if action.kind == "item_box" and not BotNav.is_origin_zone(action)
            and not BotPuzzle.avoided(bot, action.slot)
            and not blocked(bot, "box", room, action.slot) then
            out[#out + 1] = action
        end
    end
    return out
end

-- Inventory work due in this room before a save: a MOVE that empties a
-- slot, or a box visit here. Returns why, or nil.
function BotPlanner.inventory_due_here(bot)
    if not blocked(bot, "combine", "inventory", "mix", true)
        and BotInventory.combine_now(bot, gave_up) then
        return "a combine is due"
    end
    if #BotPlanner.boxes_here(bot) > 0 then
        return BotInventory.box_reason(bot, gave_up, true)
    end
    return nil
end

-- Puzzle steps (<interact> and <use> goals) done this run. Kept in the
-- memory (bot:remember), which a reload puts back as it was at the save:
-- the save rule's milestones count them, and a count in the planner's
-- state restarted at 0 with the rebuilt VM, so no milestone after a
-- reload beat the one saved and the bot never saved again (90k ticks lost
-- to each later death).
function BotPlanner.steps_done(bot)
    return tonumber(bot:memory().steps_done or "") or 0
end

-- The room the pursued target's route enters next when a live monster was
-- last seen there, or nil (the cautious save rule's "risky room next").
function BotPlanner.risky_next(bot)
    local p = plan()
    local t = p.target
    if not t then
        return nil
    end
    local world = bot:world()
    local room = bot:room().id
    local goal = target_goal(bot, world, t)
    local steps = goal and goal.room ~= room and route(bot, world, room, goal.room)
    local next_room = steps and steps[1] and steps[1].target
    if next_room and (p.monsters[next_room] or 0) > 0 then
        return next_room
    end
    return nil
end

-- Whether this room entry fights its <fight> ready or not (forgive_fight).
local fight_forced

-- Whether `item` is the preferred weapon of a <fight> in this room still
-- to be won (BotPuzzle.preferred_pickup's weapon).
function BotPlanner.preferred_here(bot, item)
    local k = bot:knowledge()
    local room = bot:room().id
    for _, fight in ipairs(bot:fight_facts()) do
        local entry = k and k.enemies[fight.enemy]
        if fight.room == room and entry and entry.prefer == item
            and not (fight.bank and bot:flag(fight.bank, fight.bit)) then
            return true
        end
    end
    return false
end

-- Pick the next goal, or nil plus the reason nothing is left.
local function choose(bot)
    local room = bot:room().id
    local world = bot:world()
    local here = world_room(world, room)

    -- A fight of this room seen won: no trip back for it (fight_due).
    for _, fight in ipairs(bot:fight_facts()) do
        if fight.room == room and fight.bank and bot:flag(fight.bank, fight.bit) then
            local key = fight_won_key(room, fight.enemy)
            if bot:memory()[key] == nil then
                bot:remember(key, "1")
            end
        end
    end

    -- 0. A <leave> whose flag is set: out through its door first (507's
    -- gas drains health until the room changes).
    local leave = BotPuzzle.leave(bot)
    if leave then
        local door = BotNav.door_by_slot(bot, leave.slot)
        if door and not blocked(bot, "door", room, door.slot) then
            return door_goal(bot, door, leave.label or "leave")
        end
    end

    -- 1. The room puzzle.
    local fact, index, count = BotPuzzle.next_interact(bot)
    local held_full = false
    if fact then
        if fact.slot and BotPuzzle.avoided(bot, fact.slot) then
            bot:log(string.format("interact slot %d is also an <avoid>; skipped", fact.slot))
        elseif fact.kind == "climb" and fact.bank == C.ITEM_BANK and inventory_full(bot)
            and not BotNav.on_top(bot) then
            -- The climb is for an item: no free slot, no climb. Retried
            -- once the inventory changes (a box deposit makes room). A climb
            -- to an event zone (507's duct) needs no slot.
            if not blocked(bot, "interact", room, fact.slot) then
                mark(bot, "interact", room, fact.slot, "inventory full: " .. inventory_sig(bot))
            end
        elseif fact.kind == "push" and fact.reveals and inventory_full(bot) then
            -- So does a push puzzle that reveals an item: 306's box went
            -- into the book's probe with no slot for the book, and on the
            -- next entry the box stood back against the wall with the slab
            -- already cranked out, which no push gets past. The room's
            -- <use> steps wait with it (the crank comes after the push).
            held_full = true
            if not blocked(bot, "interact", room, BotPuzzle.step_key(fact)) then
                mark(bot, "interact", room, BotPuzzle.step_key(fact),
                    "inventory full: " .. inventory_sig(bot))
                -- Never given up for a full inventory, like a key: 306's
                -- Doom Book was lost after three passes with every slot
                -- used, and with it the Wolf Medal and the way to the lab.
                plan().marks[mark_key("interact", room, BotPuzzle.step_key(fact))].needed = true
            end
        elseif not blocked(bot, "interact", room, BotPuzzle.step_key(fact)) then
            if fact.kind == "push" and fact.reveals then
                -- The slot is free now: the step no longer holds one.
                local key = mark_key("interact", room, BotPuzzle.step_key(fact))
                local m = plan().marks[key]
                if m and m.reason:find("inventory full", 1, true) then
                    plan().marks[key] = nil
                end
            end
            -- A boss fight ahead waits for health and rounds, a hazard
            -- (<leave>) for health; the mark
            -- lifts once the inventory changes (a heal, ammunition).
            local fight = not BotTactics.active() and BotPuzzle.fight_ahead(bot, index)
            local ready, not_ready = true, nil
            if fight and not fight_forced(bot, room) then
                ready, not_ready = BotSurvival.fight_ready(bot, fight)
            elseif not BotTactics.active() and BotPuzzle.leave_ahead(bot) then
                -- So do steps into a hazard that drains health (507's gas).
                ready, not_ready = BotSurvival.health_ready(bot, "the room's hazard")
            end
            local goal, why = nil, not_ready
            if ready then
                goal, why = interact_goal(bot, fact, index, count)
            end
            if goal then
                return goal
            end
            if fight and not ready then
                -- Held back from the fight: remembered past a reload too
                -- (fight_due; marks live in the VM a reload rebuilds).
                bot:remember(BotNav.fight_met_key(room, fight.enemy), "1")
            end
            mark(bot, "interact", room, BotPuzzle.step_key(fact), why)
        end
    end

    -- 1b. A held item to use at a zone (<use> facts that apply now).
    for _, row in ipairs(BotPuzzle.uses(bot)) do
        local fact = row.fact
        if held_full then
            -- Marked like the push, so the use draws the bot back here
            -- once the inventory changes.
            if not blocked(bot, "use", room, fact.slot) then
                mark(bot, "use", room, fact.slot, "inventory full: " .. inventory_sig(bot))
            end
        elseif not blocked(bot, "use", room, fact.slot) then
            return use_goal(bot, fact, row.action)
        end
    end

    -- 1b'. A <brew> here (BotMix): fill bottles at the room's taps and
    -- mix them until the brew's item is held. With no empty bottle held
    -- the ground items below come first (the room's bottles).
    -- A failed step is marked on its tap slot (or "mix") like any goal.
    for _, fact in ipairs(BotMix.brews(bot)) do
        local step, why = BotMix.next_step(bot, fact)
        local goal
        if step then
            goal, why = brew_goal(bot, fact, step)
        end
        if goal and blocked(bot, "brew", room, goal.slot) then
            goal, why = nil, "its next step is marked"
        end
        if goal then
            return goal
        end
        BotMix.note(bot, fact, why)
    end

    -- 1c. Inventory room: a MOVE that empties a slot, a heal used for
    -- one (BotInventory.heal_for_room), then an item box here when a
    -- deposit or a withdrawal is due.
    if not blocked(bot, "combine", "inventory", "mix", true) then
        local pick = BotInventory.combine_now(bot, gave_up)
        if pick then
            return combine_goal(bot, pick)
        end
    end
    if not blocked(bot, "combine", "inventory", "load", true) then
        local pick = BotInventory.load_now(bot)
        if pick then
            local goal = combine_goal(bot, pick)
            goal.slot = "load"
            goal.label = "load " .. goal.label
            goal.job.label = goal.label
            return goal
        end
    end
    if not blocked(bot, "heal", "inventory", "room", true) then
        local heal = BotInventory.heal_for_room(bot, gave_up)
        if heal then
            return heal_room_goal(bot, heal)
        end
    end
    for _, book in ipairs(DOOM_BOOKS) do
        -- has_item, not item_count: a book out of the box has quantity 0.
        if bot:has_item(book) and not blocked(bot, "examine", "inventory", book, true) then
            return examine_goal(bot, book)
        end
    end
    local boxes = BotPlanner.boxes_here(bot)
    local box_here = #boxes > 0 and BotInventory.box_reason(bot, gave_up, true)
    if box_here then
        -- The nearest box by its planned approach (every box shares one
        -- store).
        local box = BotPlanner.nearest(boxes, function(action)
            return BotNav.reach_length(bot, action)
        end)
        return box_goal(bot, box, box_here)
    end
    local box_why = BotInventory.box_reason(bot, gave_up)

    -- 1d. Item errands here: carry a held item a <deliver> wants into its
    -- camera cut, or take one a wanted <deliver> needs where a <fetch>
    -- offers it.
    for _, fact in ipairs(BotPuzzle.errands(bot)) do
        if fact.room == room and BotPuzzle.deliver_wanted(bot, fact)
            and bot:has_item(fact.item)
            and not blocked(bot, "deliver", room, "camera" .. fact.cut) then
            return deliver_goal(bot, fact)
        end
    end
    for _, fact in ipairs(BotPuzzle.fetches_due(bot)) do
        if fact.room == room and not blocked(bot, "fetch", room, fact.slot) then
            if not pickup_fits(bot, fact.item, inventory_full(bot)) then
                mark(bot, "fetch", room, fact.slot, "inventory full: " .. inventory_sig(bot))
            else
                local goal, why = fetch_goal(bot, fact)
                if goal then
                    return goal
                end
                mark(bot, "fetch", room, fact.slot, why)
            end
        end
    end

    -- 2. Ground items here, keys first (a key opens a door; nothing else
    -- in the room is worth more), then the nearest planned path.
    local best, best_len, best_key, best_desk = nil, nil, false, nil
    local unreachable_here = false
    local full = inventory_full(bot)
    local actions = bot:actions()
    local drawers = BotPlanner.drawers(actions)
    if plan().hurt[room] == inventory_sig(bot) then
        -- The hurt rule: this room hurt a goal with this inventory; its
        -- pickups wait until the inventory changes.
        actions = {}
    elseif plan().hurt[room] then
        plan().hurt[room] = nil
    end
    for _, action in ipairs(actions) do
        local desk = drawers[action.slot]
        if desk and (action.kind == "item" or action.kind == "item_dormant") then
            -- A drawer's item: the drawer below is the goal.
        elseif action.kind == "desk" and action.desk_item
            and not blocked(bot, "item", room, action.item_slot) then
            local why = BotPlanner.desk_blocker(bot, action)
            if why == "the drawer is empty" then
                -- Nothing to take; check_desk refuses the press.
            elseif not pickup_fits(bot, action.desk_item, full) then
                mark(bot, "item", room, action.item_slot,
                    "inventory full: " .. inventory_sig(bot))
            elseif why then
                mark(bot, "item", room, action.item_slot, why)
                plan().marks[mark_key("item", room, action.item_slot)].desk = {
                    lock_bank = action.lock_bank, lock_bit = action.lock_bit,
                }
            elseif not BotPuzzle.avoided(bot, action.slot)
                and not BotPuzzle.avoided(bot, action.item_slot) then
                local len = BotNav.reach_length(bot, action) or UNPLANNED
                plan().lens[mark_key("item", room, action.item_slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                local key = bot:item_kind(action.desk_item) == "key"
                if not best_len or (key and not best_key)
                    or (key == best_key and len < best_len) then
                    best, best_len, best_key, best_desk = action, len, key, action
                end
            end
        elseif action.kind == "item" and not blocked(bot, "item", room, action.slot) then
            local avoid, gate = BotPuzzle.avoided(bot, action.slot)
            if avoid then
                -- Marked so remote planning stops heading back for it.
                -- A gated avoid is lifted once its flag changes. Checked
                -- before the free slot: an inventory-full mark lifts on
                -- any inventory change and would send the bot back.
                mark(bot, "item", room, action.slot, "avoid: " .. avoid, gate == nil)
                plan().marks[mark_key("item", room, action.slot)].gate = gate
            elseif not pickup_fits(bot, action.item, full) then
                -- No slot for it and no stack to merge into: retried once
                -- the inventory changes.
                mark(bot, "item", room, action.slot, "inventory full: " .. inventory_sig(bot))
                -- A key or an item a <use> fact needs is never given up for
                -- a full inventory (401's red book, passed three times).
                local ctx = BotInventory.context(bot)
                local m = plan().marks[mark_key("item", room, action.slot)]
                m.item = action.item
                -- So is a hand weapon a met fight needs (30C's knife).
                if bot:item_kind(action.item) == "key" or ctx.uses[action.item]
                    or (ctx.required and ctx.required[action.item]) then
                    m.needed = true
                end
            elseif BotNav.is_origin_zone(action) then
                mark(bot, "item", room, action.slot,
                    "zone at the room origin: a scripted handover", true)
            else
                local len = BotNav.reach_length(bot, action) or UNPLANNED
                plan().lens[mark_key("item", room, action.slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                local key = bot:item_kind(action.item) == "key"
                if not best_len or (key and not best_key)
                    or (key == best_key and len < best_len) then
                    best, best_len, best_key, best_desk = action, len, key, nil
                end
            end
        end
    end
    if best_desk then
        return desk_goal(bot, best_desk)
    end
    if best then
        return item_goal(bot, best, taken_twin(here, best))
    end

    -- Doors of this room the planner may open, scored.
    local doors_here = {}
    for _, door in ipairs(bot:doors()) do
        -- An ungated <avoid> on a door holds from every room: marked for
        -- good, remote planning never heads for it either (500's lift).
        local avoid, gate = BotPuzzle.avoided(bot, door.slot)
        if avoid and not gate and not blocked(bot, "door", room, door.slot, true) then
            mark(bot, "door", room, door.slot, "avoided: " .. avoid, true)
        end
        local fresh, row = untraversed(here, door.slot)
        if fresh and door.target and door.target ~= room
            and (not BotNav.is_origin_zone(door) or via_action(bot, door))
            and not BotPuzzle.avoided(bot, door.slot)
            and not reach_closed(bot, room, door.slot)
            and not blocked(bot, "door", room, door.slot) then
            local ok, why, permanent = true, nil, false
            plan().locked[mark_key("door", room, door.slot)] = nil
            if live_locked(bot, door) then
                ok, why, permanent = key_opens(bot, door.key)
            end
            if not ok then
                if permanent then
                    if not blocked(bot, "door", room, door.slot) then
                        mark(bot, "door", room, door.slot, why, true)
                        plan().marks[mark_key("door", room, door.slot)].lock_bit =
                            door.lock % C.LOCK_SPAN
                    end
                else
                    plan().locked[mark_key("door", room, door.slot)] = why
                end
            else
                local len = BotNav.reach_length(bot, via_action(bot, door) or door) or UNPLANNED
                local visited = row and row.target_visited or bot:visited(door.target)
                -- The length from where the bot stood, for remote ranking;
                -- a door with no planned approach costs UNPLANNED_COST
                -- there, and this room gets a passage check.
                plan().lens[mark_key("door", room, door.slot)] =
                    len == UNPLANNED and UNPLANNED_COST or len
                if len == UNPLANNED and not visited then
                    unreachable_here = true
                end
                doors_here[#doors_here + 1] = {
                    door = door, len = len, visited = visited,
                }
            end
        end
    end

    -- 3. A passage check when part of this room is out of reach: a stair
    -- zone (`door_check`), a ladder (`stairs_zone`), or an action-key event zone no fact names (a
    -- room may start its stair climb from an event, as when a script turns
    -- the stair zone on from the event). One try per slot per entry.
    for _, m in pairs(plan().marks) do
        if m.room == room and m.reason:find("approach", 1, true) then
            unreachable_here = true
        end
    end
    if unreachable_here then
        local stair = passage_goal(bot, room)
        if stair then
            return stair
        end
    end

    -- 4-. The empty-frontier retry of an event door here (forgive_events):
    -- step out through the nearest door to a room already entered, so the
    -- way back is a new entry that lets the room's script decide again.
    if plan().reenter == room then
        local best_out, best_out_len = nil, nil
        for _, door in ipairs(bot:doors()) do
            if door.target and door.target ~= room and bot:visited(door.target)
                and not BotNav.is_origin_zone(door) and not live_locked(bot, door) then
                local len = BotNav.reach_length(bot, door)
                if len and (not best_out_len or len < best_out_len) then
                    best_out, best_out_len = door, len
                end
            end
        end
        if best_out then
            return door_goal(bot, best_out, "step out to re-enter " .. room)
        end
        plan().reenter = nil
    end

    -- 4. Leave the room: rank every way out by its distance cost (see the
    -- header), the doors of this room and the first door of the route to a
    -- remote frontier item or door alike.
    local lengths = {}
    local function door_len(slot)
        if lengths[slot] == nil then
            local door = BotNav.door_by_slot(bot, slot)
            local zone = door and (via_action(bot, door) or door)
            lengths[slot] = zone and BotNav.reach_length(bot, zone) or false
        end
        return lengths[slot] or UNPLANNED_COST
    end
    for _, row in ipairs(doors_here) do
        lengths[row.door.slot] = row.len ~= UNPLANNED and row.len or false
    end
    -- The mental map: which doors here the bot can reach from the part of
    -- the room it entered by.
    local mind = plan()
    -- The arrival point is taken again at the entry's first choice: on the
    -- room change the player can still stand where it left the last room
    -- (106's stair to 203 reads 106's landing, (17670, 15739), with the bot
    -- at (17100, 25300) by 203's door back down). The legs below were then
    -- never recorded, the map priced 203's far doors from that point as a
    -- short walk, and every box trip from the hall went up to 203 and
    -- straight back down until the loop rule marked it looping.
    if mind.arrival_fix == room and mind.last_from and mind.arrivals[room] then
        local me = bot:player()
        mind.arrivals[room][mind.last_from] = { x = me.x, z = me.z }
    end
    mind.arrival_fix = nil
    local arrived = mind.last_from and mind.arrivals[room] and mind.arrivals[room][mind.last_from]
    if arrived and bot:distance(arrived.x, arrived.z) <= 2 * C.REACH then
        local legs = {}
        for _, door in ipairs(bot:doors()) do
            legs[door.slot] = door_len(door.slot)
        end
        mind.legs[room] = mind.legs[room] or {}
        -- New to the map, or a door here reachable from this arrival then
        -- and not now (or the other way: 106's stair lands in 203 by the
        -- far doors or by the one back down): the pursued target's stall
        -- count restarts (the stall check below), as the door counts before
        -- were measured on a map that had this room wrong.
        local old = mind.legs[room][mind.last_from]
        local changed = old == nil
        for slot, len in pairs(legs) do
            if old and (old[slot] == nil or (old[slot] >= UNPLANNED_COST) ~= (len >= UNPLANNED_COST)) then
                changed = true
            end
        end
        if changed then
            mind.map_learned = room
        end
        mind.legs[room][mind.last_from] = legs
    end
    -- The mental map from where the bot stands, built once per choice.
    local tree = nil
    local function map()
        tree = tree or BotPlanner.map_tree(world, room, map_opts(bot, door_len))
        return tree
    end
    local goal_room = bot:goal_room()
    local function toward_goal(target)
        if not goal_room or not target then
            return 0
        end
        if target == goal_room then
            return GOAL_BONUS
        end
        if target:sub(1, 1) == goal_room:sub(1, 1) then
            return STAGE_BONUS
        end
        return 0
    end
    local p = plan()
    -- The way taken: a door of this room, and the remote target behind it.
    local function leave(way)
        -- The exit has no planned approach from this part of the room: a
        -- passage check here first (a stair to the part that reaches it).
        if not lengths[way.door.slot] then
            local stair = passage_goal(bot, room)
            if stair then
                bot:log(string.format("way out %s has no planned approach; %s first",
                    way.why, stair.label))
                return stair
            end
        end
        p.exits[room] = way.door.slot
        bot:log(string.format("way out: %s, cost %d", way.why, way.cost))
        return door_goal(bot, way.door, way.why)
    end
    local function usable(goal)
        if goal.kind == "item" then
            return not blocked(bot, "item", goal.room, goal.slot, true)
        end
        -- A door parked at the origin is a scripted transfer (see
        -- door_passable).
        return not (goal.x == 0 and goal.z == 0 and not via_fact(bot, goal.room, goal.slot))
            and not blocked(bot, "door", goal.room, goal.slot, true)
            and not avoid_holds(bot, goal.room, goal.slot)
            and not reach_closed(bot, goal.room, goal.slot)
            and (not goal.locked or key_opens(bot, goal.key))
    end

    -- The ammo sourcing rule (BotSurvival.sourcing): under the floor with
    -- no ammunition the planner can reach. Turning it on re-ranks.
    local ammo, rounds, floor = BotSurvival.ammo_need(bot)
    local known = 0
    if ammo then
        -- The gun's other loadings' rounds count (the launcher's acid and
        -- flame rounds load into it): sourcing past them in 30E's box sent
        -- the bot into the lab with one round.
        local ammos = { [ammo] = true }
        local weapons = bot:knowledge().weapons
        local held = weapons[bot:weapon().weapon]
        local base = held and (held.variant_of or bot:weapon().weapon)
        for id, row in pairs(base and weapons or {}) do
            local other = (row.variant_of or id) == base and bot:ammo_for(id)
            if other then
                ammos[other] = true
            end
        end
        -- A gun the bot has none of comes with its clip
        -- (BotInventory.new_guns).
        local guns = BotInventory.new_guns(bot)
        for _, goal in ipairs(world.frontier) do
            if goal.kind == "item" and (ammos[goal.item] or guns[goal.item])
                and not blocked(bot, "item", goal.room, goal.slot, true)
                and (goal.room == room or route(bot, world, room, goal.room)) then
                known = known + 1
            end
        end
        local box = BotInventory.remembered(bot)
        for id in pairs(ammos) do
            if box[id] then
                known = known + 1
            end
        end
    end
    BotSurvival.note_known_ammo(known)
    local sourcing = BotSurvival.sourcing(ammo, known)
    if ammo then
        bot:log(string.format("ammo low: %d rounds under the floor %d; %d known ammo item(s)%s",
            rounds, floor, known, sourcing and "; sourcing" or ""))
    end
    if sourcing and not p.sourcing and p.target then
        bot:log("sourcing ammunition: target " .. p.target.label .. " re-ranked")
        p.target = nil
    end
    p.sourcing = sourcing
    -- While sourcing, the live monsters last seen along a route; while
    -- hurt with no heal held, the monsters that hurt the bot there.
    local hurt_now = BotSurvival.needs_heal(bot)
    local boss_rooms = {}
    for _, fight in ipairs(bot:fight_facts()) do
        if not (fight.bank and bot:flag(fight.bank, fight.bit))
            and BotNav.fight_met(bot, fight)
            and not BotSurvival.fight_ready(bot, fight) then
            boss_rooms[fight.room] = true
        end
    end
    local function monsters_in(id)
        local n = sourcing and p.monsters[id] or 0
        local cost = (n > MONSTER_CAP and MONSTER_CAP or n) * MONSTER_COST
        if boss_rooms[id] then
            cost = cost + BOSS_ROOM_COST
        end
        if hurt_now then
            local h = BotTactics.hurt_count(id)
            cost = cost + (h > MONSTER_CAP and MONSTER_CAP or h) * HURT_ROOM_COST
        end
        return cost
    end
    local function route_monsters(steps)
        local cost = 0
        for _, step in ipairs(steps) do
            cost = cost + monsters_in(step.target)
        end
        return cost
    end

    -- 4a. A pursued remote target keeps its course until it is reached,
    -- real progress re-ranks, or the loop rule drops it. Getting hurt with
    -- no heal held drops it once, so a heal can be ranked first.
    local hurt = BotSurvival.needs_heal(bot)
    -- Poisoned with no cure held: a cure pickup ranks like a heal.
    local cure_now = BotInventory.context(bot).cure_now
    if not hurt then
        p.hurt_dropped = nil
    elseif p.target and not p.target.heal and not p.hurt_dropped then
        p.hurt_dropped = true
        bot:log("target dropped: hurt with no heal held: " .. p.target.label)
        p.target = nil
    end
    if p.target then
        local t = p.target
        local goal = target_goal(bot, world, t)
        if goal and goal.kind == "item" and goal.room ~= room and not t.unfit
            and not pickup_fits(bot, goal.item, full) then
            -- The inventory filled on the way: no slot for it there. (Not
            -- for a trip chosen full, the only way on: dropping it on the
            -- first entry walked 302 and 304 back and forth between two
            -- such trips until the loop rule ended the run.)
            goal = nil
        end
        if goal and (goal.kind == "use" or goal.kind == "box" or goal.kind == "deliver"
            or goal.kind == "fetch") and goal.room == room then
            -- Here, and step 1b/1c did not take it: its probe is not live,
            -- or the box is not.
            mark(bot, goal.kind, room, t.slot, goal.kind .. " not live on arrival")
            p.target = nil
            goal = nil
        end
        if goal and goal.kind == "fight" and goal.room == room then
            -- Here: the room's own steps take the fight. Its readiness
            -- marks are lifted, as fight_due found the bot ready: a third
            -- `not ready` mark held the step for good, and seed 1's cave
            -- backtrack walked to 30C and away again eight times, killing
            -- the Black Tiger in passing, until the flame rounds ran out.
            for key, m in pairs(p.marks) do
                if m.kind == "interact" and m.room == room
                    and m.reason:find("not ready for the fight", 1, true) then
                    p.marks[key] = nil
                end
            end
            p.target = nil
            goal = nil
        end
        if goal and goal.kind == "save" and goal.room == room then
            -- Here: BotSave.step decides at the typewriter.
            BotSave.trip_arrived(bot)
            p.target = nil
            goal = nil
        end
        if goal and goal.kind == "revisit" and goal.room == room then
            -- Entered: its init has read the changed flags.
            p.target = nil
            goal = nil
        end
        if goal and goal.room == room and goal.kind == "door" then
            for _, row in ipairs(doors_here) do
                if row.door.slot == goal.slot then
                    p.target = nil
                    return leave({ cost = door_len(goal.slot), door = row.door,
                        why = "reached " .. t.label })
                end
            end
        end
        local steps
        if goal and goal.room ~= room and (goal.kind == "box" or goal.kind == "save") then
            -- A box was chosen by walking distance: keep the map's route.
            steps = map_route(bot, world, room, goal.room, map(),
                BotPlanner.action_point(world, goal.room, goal.slot))
            -- The loop rule's stall check for this room entry (enter_room).
            if steps and p.stall_room == room then
                p.stall_room = nil
                if p.map_learned == room then
                    t.best, t.stalls = nil, 0
                end
                if BotPlanner.pursue(t, #steps) then
                    mark_loop(bot, t.kind, t.room, t.slot, string.format(
                        "%d entries without a shorter route", t.stalls))
                    p.target = nil
                    t, goal, steps = nil, nil, nil
                end
            end
        else
            steps = goal and goal.room ~= room
                and (goal.kind == "use" or goal.kind == "deliver" or goal.kind == "fetch"
                    or goal.kind == "fight" or goal.kind == "revisit" or usable(goal))
                and route(bot, world, room, goal.room)
        end
        local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
        if first then
            return leave({ cost = door_len(first.slot), door = first,
                why = string.format("pursuing %s, %d door(s)", t.label, #steps) })
        end
        if t then
            bot:log("target dropped: " .. t.label)
        end
        p.target = nil
    end

    -- The keys the remembered locked doors need: key item -> a door name.
    local needed = {}
    for _, r in ipairs(world.rooms) do
        for _, door in ipairs(r.doors) do
            if door.present and door.locked and not door.unlocked and not door.traversed then
                needed[door.key] = needed[door.key]
                    or string.format("%s -> %s", r.id, tostring(door.target))
            end
        end
    end
    -- A door into an unvisited room that holds a needed item (<holds>).
    local holds = bot:item_facts()
    local use_list = bot:use_facts()
    local function holds_bonus(id)
        return BotPlanner.holds_bonus(id, holds, use_list,
            function(item) return bot:has_item(item) end,
            function(fact) return not BotPuzzle.use_open(bot, fact) end,
            function(item) return bot:item_kind(item) end, ammo, needed)
    end
    local best_way = nil
    local function consider(way)
        if not best_way or way.cost < best_way.cost
            or (way.cost == best_way.cost and way.order < best_way.order) then
            best_way = way
        end
    end
    local order = 0
    for _, row in ipairs(doors_here) do
        order = order + 1
        local cost = door_len(row.door.slot) - toward_goal(row.door.target)
            + monsters_in(row.door.target)
        local why = row.visited and "untraversed" or "unvisited room"
        if not row.visited then
            local bonus, tag = holds_bonus(row.door.target)
            cost = cost - UNVISITED_BONUS - (sourcing and SOURCE_BONUS or 0) - bonus
            why = tag and (why .. " (" .. tag .. ")") or why
        end
        consider({
            cost = cost, order = order, door = row.door, why = why,
        })
    end
    -- The ammunition of the guns the bot holds: a remote trip for any other
    -- (rounds for a gun not found yet) only fills a slot a needed pickup
    -- wants, and one for a document unlocks nothing. Both are still taken
    -- when the bot is in their room.
    local gun_rounds = {}
    for id, _ in pairs((bot:knowledge() or {}).weapons or {}) do
        if bot:has_item(id) and bot:ammo_for(id) then
            gun_rounds[bot:ammo_for(id)] = true
        end
    end
    local new_guns = BotInventory.new_guns(bot)
    local function consider_frontier(allow_unfit)
        for _, goal in ipairs(world.frontier) do
            -- A remote item that would not fit is no trip: arriving full only
            -- marks it again. The box (box_why) or a combine makes room first.
            local fits = allow_unfit or goal.kind ~= "item"
                or pickup_fits(bot, goal.item, full)
            -- A room whose pickups wait on the hurt rule (step 2) is no
            -- item trip until the inventory changes: 109 and 114 walked
            -- back and forth for 114's herbs until the loop rule ended it.
            if goal.kind == "item" and p.hurt[goal.room] == inventory_sig(bot) then
                fits = false
            end
            local useless = goal.kind == "item" and (goal.item_kind == "file"
                or goal.item_kind == "ammo" and not gun_rounds[goal.item])
            if goal.room ~= room and fits and not useless then
                local steps = usable(goal) and route(bot, world, room, goal.room)
                local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
                if first then
                    order = order + 1
                    local cost = door_len(first.slot) + #steps * DOOR_COST
                        + (p.lens[mark_key(goal.kind, goal.room, goal.slot)] or 0)
                        + route_monsters(steps)
                    local why, label
                    local heal_target = nil
                    if goal.kind == "item" then
                        local _, _, heals = bot:item_kind(goal.item)
                        heals = (hurt and heals == true)
                            or (cure_now ~= nil and cure_now[goal.item] == true)
                        local bonus, tag = BotPlanner.item_bonus(goal.item, goal.item_kind, ammo,
                            needed, heals, new_guns)
                        local left = plan().marks[mark_key("item", goal.room, goal.slot)]
                        if not tag and left and left.reason:find("inventory full", 1, true) then
                            -- An item an open <use> needs, left behind on a
                            -- full inventory, weighs as a <holds> one does:
                            -- 217's MO disk lost to every nearer pickup.
                            -- One per open use: three computers, three disks.
                            local open, use_label = 0, nil
                            for _, use in ipairs(use_list) do
                                if use.item == goal.item and BotPuzzle.use_open(bot, use) then
                                    open = open + 1
                                    use_label = use_label or use.label
                                end
                            end
                            if open > bot:item_count(goal.item) then
                                bonus = ITEM_BONUS + USE_BONUS
                                tag = "for " .. (use_label or "a use")
                            end
                        end
                        heal_target = heals and goal.item_kind == "heal" or nil
                        cost = cost - bonus
                        label = string.format("item 0x%02X in %s", goal.item, goal.room)
                        if tag then
                            label = label .. " (" .. tag .. ")"
                        end
                    else
                        cost = cost - toward_goal(goal.target)
                        local tag = nil
                        if not goal.target_visited then
                            local bonus
                            bonus, tag = holds_bonus(goal.target)
                            cost = cost - UNVISITED_BONUS - (sourcing and SOURCE_BONUS or 0) - bonus
                        end
                        label = string.format("the %s -> %s door", goal.room, tostring(goal.target))
                        if tag then
                            label = label .. " (" .. tag .. ")"
                        end
                    end
                    why = string.format("toward %s, %d door(s)", label, #steps)
                    consider({
                        cost = cost, order = order, door = first, why = why,
                        target = { kind = goal.kind, room = goal.room, slot = goal.slot,
                            label = label, best = #steps, stalls = 0, heal = heal_target,
                            unfit = allow_unfit or nil },
                    })
                end
            end
        end
    end
    consider_frontier(false)

    -- A held item with a <use> fact in another room: go there.
    for _, fact in ipairs(bot:use_facts()) do
        local m = plan().marks[mark_key("use", fact.room, fact.slot)]
        -- A looping mark is held off only for its LOOP_HOLD (11A's moon
        -- crest, marked looping once on a stair detour, was never carried
        -- back and the run ended on an empty frontier).
        if fact.room ~= room and bot:has_item(fact.item) and BotPuzzle.use_open(bot, fact)
            and (not m or ((m.reason:find("inventory full", 1, true)
                    or m.reason:find("looping", 1, true))
                and not blocked(bot, "use", fact.room, fact.slot, true))) then
            local steps = route(bot, world, room, fact.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local label = string.format("use 0x%02X in %s (%s)", fact.item, fact.room,
                    fact.label or "unnamed")
                consider({
                    cost = door_len(first.slot) - HELD_USE_BONUS + route_monsters(steps),
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = "use", room = fact.room, slot = fact.slot,
                        item = fact.item, bank = fact.bank, bit = fact.bit, fact = fact,
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
    end
    -- A boss fight held back in another room, now ready (fight_due).
    for _, fight in ipairs(bot:fight_facts()) do
        if fight.room ~= room and fight_due(bot, fight) then
            local steps = route(bot, world, room, fight.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local label = string.format("the fight in %s (enemy 0x%02X), now ready",
                    fight.room, fight.enemy)
                consider({
                    cost = door_len(first.slot) - HELD_USE_BONUS + route_monsters(steps),
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = "fight", room = fight.room, slot = "fight", fact = fight,
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
    end
    -- A room to enter again now that a flag has changed (<revisit>): 50B,
    -- whose cell door to 512 shows only in the self-destruct.
    local seen = plan().revisit_seen or {}
    for _, fact in ipairs(bot:revisit_facts()) do
        if fact.room ~= room and not seen[revisit_key(fact)]
            and bot:flag(fact.bank, fact.bit) == fact.set and visited_room(world, fact.room) then
            local steps = route(bot, world, room, fact.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local label = string.format("room %s again (%s)", fact.room,
                    fact.label or "a flag changed")
                consider({
                    cost = door_len(first.slot) - HELD_USE_BONUS + route_monsters(steps),
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = "revisit", room = fact.room, slot = "revisit",
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
    end
    -- An item errand in another room: carry a held item to the <deliver>
    -- that wants it, or go to a <fetch> that offers one it needs.
    local wanted = BotPuzzle.wanted_items(bot)
    for _, fact in ipairs(BotPuzzle.errands(bot)) do
        local go
        if fact.kind == "deliver" then
            go = BotPuzzle.deliver_wanted(bot, fact) and bot:has_item(fact.item)
        else
            go = wanted[fact.item] and not bot:has_item(fact.item)
                and BotPuzzle.fetch_offered(bot, fact)
        end
        local slot = fact.kind == "deliver" and ("camera" .. fact.cut) or fact.slot
        if go and fact.room ~= room and not blocked(bot, fact.kind, fact.room, slot, true) then
            local steps = route(bot, world, room, fact.room)
            local first = steps and steps[1] and BotNav.door_by_slot(bot, steps[1].slot)
            if first then
                order = order + 1
                local label = string.format("%s 0x%02X in %s (%s)", fact.kind, fact.item,
                    fact.room, fact.label or "unnamed")
                local cure = fact.kind == "fetch" and wanted[fact.item].cure
                consider({
                    cost = (cure and door_len(first.slot) - CURE_BONUS
                        or door_len(first.slot) + #steps * DOOR_COST - ERRAND_BONUS)
                        + route_monsters(steps),
                    order = order, door = first,
                    why = string.format("toward %s, %d door(s)", label, #steps),
                    target = { kind = fact.kind, room = fact.room, slot = slot,
                        item = fact.item, fact = fact,
                        label = label, best = #steps, stalls = 0 },
                })
            end
        end
    end
    -- A box visit is due and no box is here: go to the remembered box
    -- nearest by walking distance through the mental map.
    if box_why then
        local opts = map_opts(bot, door_len)
        opts.tree = map()
        opts.usable_box = function(id, action)
            return not blocked(bot, "box", id, action.slot, true)
        end
        local box = BotPlanner.pick_remote_box(world, room, opts)
        local first = box and BotNav.door_by_slot(bot, box.steps[1].slot)
        if first then
            order = order + 1
            local label = string.format("the item box in %s (%s)", box.room, box_why)
            local heal = (hurt and box_why:find("no heal item held", 1, true) ~= nil)
                or box_why:find(BotInventory.CURE, 1, true) ~= nil or nil
            consider({
                cost = box.cost - (heal and HEAL_BONUS or BOX_BONUS) + route_monsters(box.steps),
                order = order, door = first,
                why = string.format("toward %s, %d door(s)", label, #box.steps),
                target = { kind = "box", room = box.room, slot = box.slot,
                    label = label, best = #box.steps, stalls = 0, heal = heal },
            })
        end
    end
    -- A save trip is due: the remembered typewriter nearest by walking.
    local save_why = BotSave.trip_reason(bot)
    if save_why then
        local opts = map_opts(bot, door_len)
        opts.tree = map()
        opts.kind = "typewriter"
        opts.usable_box = function(id, action)
            return not blocked(bot, "save", id, action.slot, true)
        end
        local desk = BotPlanner.pick_remote_box(world, room, opts)
        local first = desk and BotNav.door_by_slot(bot, desk.steps[1].slot)
        if first then
            order = order + 1
            local label = string.format("the typewriter in %s (%s)", desk.room, save_why)
            consider({
                cost = desk.cost - BOX_BONUS + route_monsters(desk.steps),
                order = order, door = first,
                why = string.format("toward %s, %d door(s)", label, #desk.steps),
                target = { kind = "save", room = desk.room, slot = desk.slot,
                    label = label, best = #desk.steps, stalls = 0 },
            })
        end
    end
    if not best_way and full then
        -- Nothing else is left: the full-inventory trips are the only
        -- way on (an item may merge or a combine may free a slot there).
        consider_frontier(true)
    end
    if best_way then
        if best_way.target then
            p.target = best_way.target
            bot:log("target: " .. p.target.label)
        end
        return leave(best_way)
    end
    return nil
end

-- A summary of what is left for the finish reason.
local function leftovers(bot)
    local keys, others = {}, {}
    for key, why in pairs(plan().locked) do
        keys[#keys + 1] = key .. ": " .. why
    end
    for _, m in pairs(plan().marks) do
        local text = string.format("%s %s slot %s: %s", m.kind, m.room, tostring(m.slot), m.reason)
        if m.reason:find("needs key", 1, true) then
            keys[#keys + 1] = text
        else
            others[#others + 1] = text
        end
    end
    table.sort(keys)
    table.sort(others)
    local parts = {}
    if #keys > 0 then
        parts[#parts + 1] = "locked: " .. table.concat(keys, "; ")
    end
    if #others > 0 then
        parts[#parts + 1] = "blocked: " .. table.concat(others, "; ")
    end
    return table.concat(parts, " | ")
end

function BotPlanner.finish(bot, reason)
    -- Where the bot stood idle in control (on_bot_tick), longest first.
    local idle = {}
    for key, n in pairs(BOT_STATE.idle or {}) do
        idle[#idle + 1] = { key = key, n = n }
    end
    table.sort(idle, function(a, b) return a.n > b.n end)
    bot:log(string.format("idle total %d ticks", BOT_STATE.idle_total or 0))
    for i = 1, #idle < 25 and #idle or 25 do
        bot:log(string.format("idle %d %s", idle[i].n, idle[i].key))
    end
    -- The remembered door graph, for diagnosing why the frontier ran dry.
    for _, r in ipairs(bot:world().rooms or {}) do
        local parts = {}
        for _, door in ipairs(r.doors or {}) do
            parts[#parts + 1] = string.format("%d->%s k%02X%s%s%s", door.slot,
                tostring(door.target), door.key or 0, door.traversed and " T" or "",
                door.locked and not door.unlocked and " L" or "",
                door_passable(bot, r.id, door) and "" or " X")
        end
        bot:log(string.format("world %s: %s", r.id, table.concat(parts, ", ")))
    end
    -- The ammunition account: every ammunition pickup the world model saw,
    -- and what became of it.
    local marks = plan().marks
    for _, r in ipairs(bot:world().rooms or {}) do
        for _, item in ipairs(r.items or {}) do
            if item.item_kind == "ammo" then
                local m = marks[mark_key("item", r.id, item.slot)]
                local fate = item.taken and "taken"
                    or (item.present and "left" or "gone")
                bot:log(string.format("ammo account: %s slot %d item 0x%02X x%d %s%s",
                    r.id, item.slot, item.item, item.quantity or 0, fate,
                    m and not item.taken and (" (" .. m.reason .. ")") or ""))
            end
        end
    end
    local p = bot:player()
    local text = string.format("%s (room %s health %d/%d) %s", reason, bot:room().id,
        p.health, p.max_health, leftovers(bot))
    bot:log("FINISH " .. text)
    bot:finish(text)
end

-- Before the run finishes on an empty frontier: a goal that failed on its
-- approach or its time budget depends on where the bot stood, so each such
-- mark under MAX_ATTEMPTS gets another try (its attempts still count, so
-- this ends). Returns how many marks were forgiven.
local forgive_events

local function forgive_approaches(bot)
    local p = plan()
    local room = bot:room().id
    local n = 0
    for key, m in pairs(p.marks) do
        if not m.permanent and m.attempts < MAX_ATTEMPTS and not m.hold and m.sig ~= ""
            and (m.reason:find("approach", 1, true) or m.reason:find("timed out", 1, true)) then
            if m.room == room and m.entry == (p.entries or {})[room]
                and m.reason:find("every approach failed", 1, true) then
                -- Planned unreachable on this very entry: a retry from the
                -- same spot plans the same, so the bot steps out and back
                -- (an approach mark lifts on the next entry). 305's drained
                -- fountain opens the way to 310 only on a later entry; three
                -- retries in 20 ticks gave the door up for good.
                p.reenter = room
                bot:log("frontier empty; stepping out to retry " .. key .. " (" .. m.reason .. ")")
            else
                m.sig = ""
                m.entry = nil
                bot:log("frontier empty; retrying " .. key .. " (" .. m.reason .. ")")
            end
            n = n + 1
        end
    end
    return n
end

-- Before the run finishes on an empty frontier: a fight here the bot was
-- held back from (`not ready for the fight`) is fought now, ready or not,
-- for the rest of this room entry (p.forced_fight). Nothing else is left
-- to make it readier, and 303's heliport locks every door behind the
-- flare: holding back there only ends the run. Returns whether one was.
local function forgive_fight(bot)
    local p = plan()
    local room = bot:room().id
    local forgiven = false
    local remote_key, remote
    for key, m in pairs(p.marks) do
        if m.reason:find("not ready for the fight", 1, true) then
            if m.room == room then
                p.marks[key] = nil
                p.forced_fight = { room = room, entry = (p.entries or {})[room] }
                forgiven = true
                bot:log("frontier empty; fighting anyway: " .. key .. " (" .. m.reason .. ")")
            elseif not remote_key or key < remote_key then
                remote_key, remote = key, m
            end
        end
    end
    -- A fight held back in another room is walked to and fought on the next
    -- entry (fight_due): 20C's Yawn 2, short of rounds, ended two runs on an
    -- empty frontier in 216.
    if not forgiven and remote and not (p.forced_fight and p.forced_fight.room == remote.room) then
        for key, m in pairs(p.marks) do
            if m.room == remote.room and m.reason:find("not ready for the fight", 1, true) then
                p.marks[key] = nil
            end
        end
        p.forced_fight = { room = remote.room, entry = ((p.entries or {})[remote.room] or 0) + 1 }
        forgiven = true
        bot:log("frontier empty; going to fight anyway: " .. remote_key .. " (" .. remote.reason .. ")")
    end
    return forgiven
end

-- The story flags (the scenario and common banks, 0 and 1) as a string of
-- the set bits: what a room's init reads to pick its variant.
local STORY_BANK_COUNT = 2
local BANK_BITS = 1 << 8
local function story_sig(bot)
    local parts = {}
    for bank = 0, STORY_BANK_COUNT - 1 do
        for sel = 0, BANK_BITS - 1 do
            if bot:flag(bank, sel) then
                parts[#parts + 1] = bank * BANK_BITS + sel
            end
        end
    end
    return table.concat(parts, ",")
end

-- Before the run finishes on an empty frontier: a room of this stage left
-- before the story flags last changed is entered again (nearest first by
-- doors), since its init may now play a scene or open a door. 514 starts
-- the self-destruct, which unlocks the lab's exit and 50B's cell door,
-- only when entered after the Tyrant's death; with every other goal of
-- the lab done before that fight, nothing led back there and the run
-- ended in 513. A room is revisited only in a story state it was never
-- entered or left in, once per signature, so this ends: flags a room's
-- script sets and clears as the bot walks (the caves' 0x30 and 0x31, in
-- 301 and 302) bring back states already seen, and with barry_early=chemical
-- the bot circled 300, 301 and 302 on them until the run timed out.
-- Returns whether a revisit target was set.
-- At most MAX_REVISITS revisits run between two real progress changes
-- (progress_sig): on the richard=leave run, scenario flags that 301's and
-- 302's scripts kept setting and clearing gave every room of stage 2 a new
-- signature again and again, and the revisits walked the mansion from tick
-- 173k until the run timed out with the real blocker (the square crank,
-- boxed at 403 behind a box trip marked looping) never named.
function BotPlanner.revisit_room(bot)
    local p = plan()
    local room = bot:room().id
    local world = bot:world()
    local progress = progress_sig(bot, world)
    if not p.revisit_run or p.revisit_run.sig ~= progress then
        p.revisit_run = { sig = progress, n = 0 }
    end
    if p.revisit_run.n >= MAX_REVISITS then
        if not p.revisit_run.logged then
            p.revisit_run.logged = true
            bot:log(string.format("frontier empty; %d revisits without progress, no more",
                p.revisit_run.n))
        end
        return false
    end
    local sig = story_sig(bot)
    local seen = p.story_seen or {}
    local function seen_in(id)
        return seen[id] ~= nil and seen[id][sig] == true
    end
    p.revisited = p.revisited or {}
    local stage = string.sub(room, 1, 1)
    local best, best_steps = nil, nil
    for _, r in ipairs(world.rooms or {}) do
        if r.visited and r.id ~= room and string.sub(r.id, 1, 1) == stage
            and not seen_in(r.id) and not p.revisited[r.id .. "|" .. sig] then
            local steps = route(bot, world, room, r.id)
            if steps and steps[1] and (not best or #steps < #best_steps
                or (#steps == #best_steps and r.id < best)) then
                best, best_steps = r.id, steps
            end
        end
    end
    if not best then
        return false
    end
    p.revisited[best .. "|" .. sig] = true
    p.revisit_run.n = p.revisit_run.n + 1
    local label = string.format("room %s again (story flags changed since it was left)", best)
    p.target = { kind = "revisit", room = best, slot = "revisit", label = label,
        best = #best_steps, stalls = 0 }
    bot:log("frontier empty; entering " .. label)
    return true
end

-- Called on every room change: each <revisit> fact of this room whose
-- flag is in its state now is done (its init has read the flag).
local function note_revisits(p, bot, room_id)
    p.revisit_seen = p.revisit_seen or {}
    for _, fact in ipairs(bot:revisit_facts()) do
        if fact.room == room_id and bot:flag(fact.bank, fact.bit) == fact.set then
            p.revisit_seen[revisit_key(fact)] = true
        end
    end
end

-- Called on every room change: the story flags as the bot leaves the last
-- room and enters this one (BotPlanner.revisit_room).
local function note_story(p, bot, room_id)
    local sig = story_sig(bot)
    p.story_seen = p.story_seen or {}
    for _, id in pairs({ last = p.last_room, here = room_id }) do
        p.story_seen[id] = p.story_seen[id] or {}
        p.story_seen[id][sig] = true
    end
end

fight_forced = function(bot, room)
    local f = plan().forced_fight
    return f ~= nil and f.room == room and f.entry == (plan().entries or {})[room]
end

-- Before the run finishes on an empty frontier: each door a room event
-- made a message is retried. Away from its room, at once: the walk back is
-- the wait. In its room, once it has waited EVENT_WAIT (doubled per
-- attempt) since it failed; standing there on the entry it failed on, the
-- bot then steps out (p.reenter): the room's script decides the door again
-- only on an entry. Returns the forgiven count and whether a mark is still
-- waiting (the bot then idles instead of finishing).
forgive_events = function(bot)
    local p = plan()
    local n, waiting = 0, false
    for key, m in pairs(p.marks) do
        if m.event and not m.event.forgiven then
            local wait = EVENT_WAIT * (1 << (m.attempts - 1))
            if m.room ~= bot:room().id or bot:tick() - m.event.tick >= wait then
                m.event.forgiven = true
                n = n + 1
                if m.room == bot:room().id and p.entries[m.room] == m.event.entry then
                    p.reenter = m.room
                end
                bot:log(string.format("frontier empty; retrying %s (%s) after %d ticks; %s",
                    key, m.reason, bot:tick() - m.event.tick,
                    p.reenter and "stepping out to re-enter its room" or "travelling back"))
            else
                if not m.event.waiting then
                    m.event.waiting = true
                    bot:log(string.format("frontier empty; waiting for %s (%s)", key, m.reason))
                end
                waiting = true
            end
        end
    end
    return n, waiting
end

-- Called once on every room change.
function BotPlanner.enter_room(bot, room)
    local p = plan()
    local from = p.goal
    if from and from.kind == "door" and from.target and from.target ~= room.id then
        bot:log(string.format("door %s led to %s instead of %s", from.label,
            room.id, from.target))
    end
    p.goal = nil
    p.entries[room.id] = (p.entries[room.id] or 0) + 1
    p.reenter = nil
    p.entered_tick = bot:tick()
    p.transfer_wait = nil
    p.stall_room = nil
    p.map_learned = nil
    -- The mental map: where the bot arrives in this room from the last one.
    if p.last_room and p.last_room ~= room.id then
        local me = bot:player()
        p.arrivals[room.id] = p.arrivals[room.id] or {}
        p.arrivals[room.id][p.last_room] = { x = me.x, z = me.z }
        p.arrival_fix = room.id
    end
    p.last_from = p.last_room ~= room.id and p.last_room or nil
    note_story(p, bot, room.id)
    note_revisits(p, bot, room.id)
    note_reaches(p, bot, room.id)
    p.last_room = room.id
    BotPuzzle.enter_room()
    p.last_done = { room = room.id, tick = bot:tick(), anywhere = true }
    progress(bot, "entered " .. room.id)
    p.stairs[room.id] = nil
    local goal = bot:goal_room()
    if goal and room.id == goal then
        BotPlanner.finish(bot, "goal room " .. goal .. " reached")
        return
    end
    BotPlanner.observe_progress(bot, "entered " .. room.id)
    -- The loop rules: a room entered again and again without real progress,
    -- and a pursued target whose route does not get shorter.
    local verdict, n = BotPlanner.repeat_check(p.since, room.id)
    if verdict == "finish" then
        BotPlanner.finish(bot, string.format(
            "looping: entered %s %d times without progress, every other goal marked",
            room.id, n))
        return
    end
    local t = p.target
    if n > 1 or (t and (t.stalls or 0) > 0) then
        bot:log(string.format("loop count: %s entered %d times since progress; target stalls %s",
            room.id, n, t and tostring(t.stalls or 0) or "-"))
    end
    if verdict == "loop" then
        if t then
            mark_loop(bot, t.kind, t.room, t.slot,
                string.format("%s entered %d times without progress", room.id, n))
            p.target = nil
        elseif p.exits[room.id] then
            mark_loop(bot, "door", room.id, p.exits[room.id],
                string.format("%s entered %d times without progress", room.id, n))
        end
        return
    end
    if t then
        local world = bot:world()
        local goal = target_goal(bot, world, t)
        -- Measured on the route the pursuit walks. A box or a typewriter
        -- goes by the map's walking distance, measured in choose
        -- (p.stall_room) with the room's planned exit lengths: built here,
        -- before the approaches are planned, the first leg is a straight
        -- line from the doorway, and on the richard=leave run that picked
        -- other exits than the pursuit walked, so trips to box 118 and 403
        -- that lost a door at every room (17, 16, ... 13) still stalled
        -- three times and were marked looping. (Fewest doors marked every
        -- box looping, the way over the 2F corridors taking more doors.)
        if goal and (t.kind == "box" or t.kind == "save") then
            p.stall_room = room.id
            return
        end
        local steps = goal and route(bot, world, room.id, goal.room)
        if steps and BotPlanner.pursue(t, #steps) then
            mark_loop(bot, t.kind, t.room, t.slot, string.format(
                "%d entries without a shorter route", t.stalls))
            p.target = nil
        end
    end
end

-- Compare the progress signature with the last one; on a change, the loop
-- counts and the pursued target start over.
function BotPlanner.observe_progress(bot, note)
    local p = plan()
    local sig = progress_sig(bot, bot:world())
    if sig ~= p.sig then
        p.sig = sig
        real_progress(bot, note)
    end
end

-- Give up the current goal without marking it, for a layer that takes the
-- pad for a while (a save at a typewriter); it is chosen again afterwards.
-- A message is up. If the current goal is a door the bot pressed, the
-- press opened a message instead of the door (a room event's lock).
function BotPlanner.note_message(bot, message)
    local goal = plan().goal
    if goal and goal.kind == "door" and not goal.was_locked and goal.job
        and goal.job.pressed and not goal.door_message then
        goal.door_message = message
    end
end

-- Whether the current goal walks out of the room (a door or a stair).
function BotPlanner.leaving()
    local goal = plan().goal
    return goal ~= nil and (goal.kind == "door" or goal.kind == "stair")
end

-- A tick the goal did not get (a spit dodge held the pad): its time
-- budget is not spent. 30C's tiger spits every few seconds, and the dodges
-- timed the web door out while the bot stood next to it.
function BotPlanner.hold_clock()
    local goal = plan().goal
    if goal and goal.started then
        goal.started = goal.started + 1
    end
end

function BotPlanner.drop_goal(bot)
    local p = plan()
    if p.goal then
        bot:log("goal paused: " .. p.goal.label)
        p.goal = nil
    end
end

-- Whether the next goal waits for a use probe: within `window` ticks of
-- the last goal done in this room (`last_done` {room, tick, zone}), a <use>
-- fact here has its item held and its done flag clear but no live probe
-- yet, at a zone overlapping the one that goal acted on. Entering the room
-- counts as a goal done anywhere in it (`anywhere`).
function BotPlanner.awaiting_use(bot, last_done, window)
    if not last_done or last_done.room ~= bot:room().id
        or bot:tick() - last_done.tick > window then
        return nil
    end
    return BotPuzzle.use_pending(bot, last_done.zone, last_done.anywhere)
end

-- One controllable planner tick.
function BotPlanner.step(bot)
    local p = plan()
    local room = bot:room().id
    if not p.monster_tick or bot:tick() - p.monster_tick >= MONSTER_EVERY then
        p.monster_tick = bot:tick()
        p.monsters[room] = #bot:enemies()
    end
    if bot:tick() - (p.progress or 0) > STUCK_TICKS then
        BotPlanner.finish(bot, string.format("stuck in room %s: no progress for %d ticks",
            room, STUCK_TICKS))
        return
    end
    if not p.goal then
        local fact = BotPlanner.awaiting_use(bot, p.last_done, USE_ARM_TICKS)
        if fact then
            if not p.awaiting then
                bot:log("waiting for the use probe: " .. (fact.label or "use"))
            end
            p.awaiting = true
            return
        end
        p.awaiting = nil
        p.goal = choose(bot)
        if not p.goal and forgive_approaches(bot) > 0 then
            p.goal = choose(bot)
        end
        if not p.goal then
            local forgiven, waiting = forgive_events(bot)
            if forgiven > 0 then
                p.goal = choose(bot)
            elseif waiting then
                -- A room left before the story changed is entered again
                -- before the wait for a message door (BotPlanner.revisit_room):
                -- 508's lamps freed 50F's power panel, and the bot idled at
                -- 50C's dark elevator door, retrying it with ever longer
                -- waits until the run timed out.
                while not p.goal and BotPlanner.revisit_room(bot) do
                    p.goal = choose(bot)
                end
                if not p.goal then
                    return
                end
            end
        end
        if not p.goal and forgive_fight(bot) then
            p.goal = choose(bot)
        end
        if not p.goal and bot:tick() - (p.entered_tick or 0) < TRANSFER_WAIT then
            for _, door in ipairs(bot:doors()) do
                if door.target and door.target ~= room and BotNav.is_origin_zone(door) then
                    if not p.transfer_wait then
                        p.transfer_wait = true
                        bot:log(string.format("frontier empty; waiting for door slot %d -> %s at the room origin",
                            door.slot, door.target))
                    end
                    return
                end
            end
        end
        -- A just-won fight's scene can still be coming (303's Tyrant falls
        -- off the heliport, then the ending plays): an empty frontier waits
        -- a while before it ends the run, and resets once a goal shows.
        if p.goal then
            p.empty_since = nil
        else
            p.empty_since = p.empty_since or bot:tick()
            if bot:tick() - p.empty_since < EMPTY_FRONTIER_GRACE then
                return
            end
        end
        -- Each call takes one room off the list, so this ends.
        while not p.goal and BotPlanner.revisit_room(bot) do
            p.goal = choose(bot)
            if p.goal then
                p.empty_since = nil
            end
        end
        if not p.goal then
            local loops = {}
            for key, m in pairs(p.marks) do
                if m.reason:find("looping", 1, true) and blocked(bot, m.kind, m.room, m.slot, true) then
                    loops[#loops + 1] = key
                end
            end
            table.sort(loops)
            BotPlanner.finish(bot, #loops > 0 and string.format(
                "frontier empty: nothing reachable is left but %d goal(s) marked looping: %s",
                #loops, table.concat(loops, ", "))
                or "frontier empty: nothing reachable is left")
            return
        end
        p.goal.started = bot:tick()
        p.goal.health = bot:player().health
        p.goal.hurt = 0
        bot:log("goal: " .. p.goal.label)
        BotScene.clear_pickup()
    end
    local goal = p.goal
    local me = bot:player()
    if me.health < (goal.health or me.health) then
        goal.hurt = (goal.hurt or 0) + goal.health - me.health
    end
    goal.health = me.health
    -- Not a weapon a fight here prefers: that pickup is how the fight ends
    -- (Brad's rocket launcher on 303's deck, with the Tyrant on the bot).
    if goal.kind == "item" and goal.hurt >= me.max_health * HURT_SHARE
        and not BotPlanner.preferred_here(bot, goal.item) then
        mark(bot, "item", room, goal.slot, string.format("hurt while trying: %d health", goal.hurt))
        p.hurt[room] = inventory_sig(bot)
        p.goal = nil
        return
    end
    -- A door an <avoid> holds shut since the goal was chosen (its gate came
    -- to hold on the way: 308's door 1 once Barry goes ahead, for a
    -- backtrack) is dropped, not walked through.
    -- (A door avoided already when chosen is reached another way, through
    -- a <door via>: 302's elevator.)
    if goal.kind == "door" then
        local avoid = BotPuzzle.avoided(bot, goal.slot)
        if goal.avoided_at_start == nil then
            goal.avoided_at_start = avoid ~= nil
        end
        if avoid and not goal.avoided_at_start then
            bot:log(string.format("goal dropped: door slot %d is avoided now: %s", goal.slot, avoid))
            p.goal = nil
            return
        end
    end
    local status
    if goal.job.camera then
        status = BotNav.visit_camera(bot, goal.job)
    elseif goal.job.menu then
        status = menu_step(bot, goal.job)
    elseif goal.job.push then
        status = BotNav.push_object(bot, goal.job)
        if status == "done" then
            BotPuzzle.pushed(bot, goal.job.step)
        end
    elseif goal.job.climb then
        status = BotNav.climb_object(bot, goal.job)
    elseif goal.job.fight then
        status = BotNav.hold_fight(bot, goal.job)
    elseif goal.job.wait then
        -- A <wait> step: stand still until a timed room event sets its flag.
        status = goal.job.done(bot) and "done" or "busy"
    else
        status = BotNav.press_zone(bot, goal.job)
    end
    if goal.job.press_tick == bot:tick() then
        BotPuzzle.pressed(bot, goal.press_slot or goal.slot)
        if goal.kind == "box" then
            BotInventory.expect_box(bot)
        end
    end
    if goal.door_message then
        -- The press opened a message, not the door: a room event holds it.
        status = "failed"
        goal.job.reason = string.format("a room event made the door a message (%d)",
            goal.door_message)
        goal.event = true
    end
    if status == "busy" and bot:tick() - goal.started > (goal.job.timeout or GOAL_TIMEOUT * 2) then
        status = "failed"
        goal.job.reason = "goal timed out"
    end
    if status == "done" and goal.kind == "box" and goal.sig == inventory_sig(bot)
        and not (goal.look and bot:memory().box_items ~= nil) then
        -- A visit that moved nothing must not be chosen again until the
        -- inventory changes.
        status = "failed"
        goal.job.reason = "the box visit changed nothing"
    end
    if status == "done" then
        bot:log("goal done: " .. goal.label)
        p.last_done = { room = room, tick = bot:tick(), zone = goal.job.zone }
        progress(bot)
        if goal.kind == "interact" or goal.kind == "use" or goal.kind == "camera"
            or goal.kind == "deliver" or goal.kind == "fetch" then
            if goal.kind ~= "camera" then
                bot:remember("steps_done", tostring(BotPlanner.steps_done(bot) + 1))
            end
            real_progress(bot, goal.label)
            if goal.kind == "use" then
                -- A retried turning piece (see use_goal) did its step.
                p.marks[mark_key("use", room, goal.slot)] = nil
            end
            -- A room event (an object moved off an item) may open what
            -- was unreachable here.
            if goal.kind ~= "camera" then
                clear_unreachable(bot, room, true)
            end
        else
            BotPlanner.observe_progress(bot, goal.label)
        end
        if goal.kind == "stair" then
            clear_unreachable(bot, room)
        end
        p.goal = nil
    elseif status == "failed" then
        local kind = (goal.kind == "camera") and "interact" or goal.kind
        -- A combine or a book opened is about the inventory, not the room.
        local where = (goal.kind == "combine" or goal.kind == "examine") and "inventory" or room
        mark(bot, kind, where, goal.slot, goal.job.reason or "failed", goal.job.permanent)
        if goal.job.retry then
            p.marks[mark_key(kind, where, goal.slot)].hold = bot:tick()
        end
        if goal.event then
            local m = p.marks[mark_key(kind, where, goal.slot)]
            m.event = { entry = p.entries[room], tick = bot:tick(), forgiven = false }
        end
        if goal.job.reenter then
            p.reenter = room
        end
        p.goal = nil
    end
end
