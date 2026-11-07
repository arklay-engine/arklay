-- Combat for the player bot, driven by the game's knowledge file
-- (knowledge/<game>.xml, read as bot:knowledge()). Loads after constants.lua,
-- nav.lua and survival.lua (chunks run in path order).
--
-- Each tick that healing and saving leave free, BotTactics.step:
--   1. looks at the live monsters near the bot and their knowledge entries;
--   2. decides each engagement (BotTactics.decide): fight, or run past it;
--   3. for a fight, keeps the knowledge's distance band for the weapon
--      (BotTactics.spacing) by walking to a point the engine's spacing search
--      picks (bot:spacing_point) along an enemy-avoiding path, faces the
--      target, holds aim in the pose that matches the enemy's live status
--      bit (BotTactics.pose), and fires from the gun hold;
--   4. for a run, the planner keeps its goal and walks with avoid_enemies
--      (BotTactics.avoiding), so its path passes behind the monster. While
--      the bot sources ammunition (BotSurvival.sourcing) every walk avoids
--      the monsters and a monster only in a room crossed again is run past.
-- The fight-or-run rule, the pose rule and the spacing rule are pure
-- functions with unit tests (src/engine/bot.rs). No enemy id, weapon id or
-- coordinate is written here; every per-enemy fact is in the knowledge file.

BotTactics = BotTactics or {}

local C = BotConst
local abs = BotNav.abs

-- A fight with no hit on its target in this long stands down.
local MAX_TICKS = 25 * C.SECOND
-- The gun hold must show a lock (or the aim settle) within this long.
local LOCK_WAIT = 3 * C.SECOND
-- Ticks the aim is held before a shot without a lock: the raise.
local AIM_SETTLE = C.SECOND // 2
-- A reload that has not started or finished by now stands down.
local RELOAD_WAIT = 3 * C.SECOND
-- One reposition walk, then hold for SPACE_REST before moving again.
local SPACE_TICKS = C.SECOND + C.SECOND // 2
local SPACE_REST = 2 * C.SECOND
-- A walk this close to the spacing point is there.
local SPACE_TOLERANCE = C.APPROACH
-- A backward-walk retreat that has not reached its distance by now shoots.
local RETREAT_TICKS = 2 * C.SECOND
-- A monster showing no aim bit this long is not fought until it shows one.
local DORMANT_TICKS = C.SECOND // 2
-- The facing error accepted before raising the aim: 1/32 turn.
local FACE_BAND = C.FULL_TURN // 32
-- After a target is lost, walk on this long before engaging again.
local LOST_CALM = 6 * C.SECOND
-- Enemies are considered inside this distance when knowledge gives no band.
local NEAR = 8 * C.REACH
-- After a run decision the planner's walks avoid the monsters this long.
local RUN_HOLD = 2 * C.SECOND
-- A grabber straight ahead (inside AHEAD_ARC of the facing) counts as
-- closing this much past its band: the bot's own run toward it closes the
-- gap at the run's speed, so 10A's zombie, run past at 4600, was at 3000
-- nine ticks later, and the handgun's last shot came after its grab.
local AHEAD = 2 * C.REACH
local AHEAD_ARC = C.FULL_TURN // 8
-- Player state 2 is the generic hit reaction (player_script.rs): the
-- player is held and a raised aim or a reload is dropped.
local HIT_STATE = 2
-- The hunter (enemy/06) and its dodge-and-swipe behaviour.
local HUNTER = 0x06
local HUNTER_DODGE = 6

local function state()
    BOT_STATE.tactics = BOT_STATE.tactics or {}
    return BOT_STATE.tactics
end

local function risk(bot)
    return bot:risk() or 0.25
end

local function angle_delta(a, b)
    local delta = (a - b) % C.FULL_TURN
    if delta > C.FULL_TURN // 2 then
        delta = delta - C.FULL_TURN
    end
    return delta
end

local POSE_BITS = { torso = C.STATUS_TORSO, down = C.STATUS_LOW, up = C.STATUS_HIGH }

-- The aim pose for a shot, or nil when no pose can hit the enemy now.
--   entry: the enemy's knowledge row (or nil); weapon: the weapon's row (or
--   nil); status: the enemy's live status_flags; distance: horizontal.
-- The engine hits only when the enemy's status shares a bit with the aim
-- (torso 0x40, down 0x20, up 0x80). The order: a shotgun headshot close in
-- (up, then torso: both pop the head); a downed enemy (only the low bit) is
-- shot down at the legs; otherwise the entry's standing pose, then torso,
-- down and up. Up is tried only with a weapon the entry allows it for.
-- Returns the pose and why.
function BotTactics.pose(entry, weapon, status, distance)
    local up_ok = entry ~= nil and weapon ~= nil
        and (entry.up == "yes" or entry.up == weapon.name)
    local order, seen = {}, {}
    local function add(pose)
        if pose and not seen[pose] then
            seen[pose] = true
            order[#order + 1] = pose
        end
    end
    local why = "standing"
    local downed = status & C.STATUS_TORSO == 0 and status & C.STATUS_LOW ~= 0
        and status & C.STATUS_HIGH == 0
    if weapon and weapon.headshot and distance <= weapon.headshot and up_ok
        and entry.head then
        why = "headshot range"
        add("up")
        add("torso")
    elseif downed then
        why = "downed"
        add(entry and entry.downed or "down")
    end
    add(entry and entry.stand)
    add("torso")
    add("down")
    add("up")
    for _, pose in ipairs(order) do
        if (pose ~= "up" or up_ok) and status & POSE_BITS[pose] ~= 0 then
            return pose, why
        end
    end
    return nil, "no aim bit matches"
end

-- The pad input for an aim pose.
function BotTactics.aim_input(pose)
    local input = { aim = true }
    if pose == "down" then
        input.down = true
    elseif pose == "up" then
        input.up = true
    end
    return input
end

-- Fight or run. `o`:
--   policy     the knowledge policy ("fight", "pass", "avoid") or nil
--   gun        a firearm with rounds is held
--   rounds     clip + reserve of that firearm
--   needed     rounds to bring the target down (its live health / damage)
--   crowd_needed  the same summed over every fightable monster near
--   health, max_health, risk
--   crossing   this room still has frontier work (the bot comes back)
--   distance, reach  the target's distance and attack reach (nil unknown)
--   engage     the far edge of the weapon's distance band (nil unknown)
--   outrun     Jill's run beats its chase (nil unknown)
--   slow       its chase speed is known and below the backward walk
--   sourcing   the bot is under its ammo floor with no ammunition it can
--              reach (BotSurvival.sourcing): the rounds left are for what
--              must be fought, so a monster only in a room crossed again
--              is run past
--   no_band    the file lists bands for it, none for this gun
--   ahead      a grabber straight ahead: closing from AHEAD past its band
--   hurt_before  it already hurt the bot (BotTactics.note_hurt): passing it
--              again costs the same hit, so the rounds for the kill
--              suffice, with no margin
-- Returns "fight" or "run" and why. The rule: never fight what the file
-- says to pass or avoid, or knows nothing about, or without a firearm;
-- fight what is closing in (inside twice its reach, or inside the band's
-- far edge) with whatever rounds are left, even at low health when it cannot be outrun (or its speed is
-- unknown); below a risk-scaled health floor run (but shoot a slow one the
-- rounds cover); otherwise only with the rounds
-- for the target (a margin at low risk) and for the crowd, and then what
-- cannot be outrun and anything in a room the bot must cross again;
-- otherwise it is only in the way: run past it.
-- The verdict for a monster closing in that the bot cannot outrun: the
-- fight goes on at any health, since running only takes the hit anyway.
local MUST_FIGHT = "closing in, cannot outrun it"
-- The verdict for a slow monster at low health: like MUST_FIGHT, it is not
-- given up for health.
local SLOW_FIGHT = "low health: shoot a slow one rather than pass it"
-- The verdict for one that hurt the bot on an earlier pass, fought with
-- no margin of rounds.
local HURT_FIGHT = "it hurt the bot on an earlier pass"

-- Monsters that have hurt the bot, by "room:slot" (a monster's slot is
-- fixed in its room). Kept across saves and reloads: the next pass meets
-- the same monster in the same place.
local function hurt_key(room, slot)
    return room .. ":" .. slot
end

-- Record that the monster `enemy` (a bot:enemies() row, or nil) in this
-- room hurt the bot, when it is inside twice its reach.
function BotTactics.note_hurt(bot, enemy)
    if not enemy then
        return
    end
    local entry = bot:knowledge().enemies[enemy.id]
    if enemy.distance > 2 * (entry and entry.reach or C.REACH) then
        return
    end
    local s = state()
    s.hurt_by = s.hurt_by or {}
    local key = hurt_key(bot:room().id, enemy.slot)
    if not s.hurt_by[key] then
        bot:log(string.format("hurt by slot=%d id=%#04x: fight it on the next pass",
            enemy.slot, enemy.id))
    end
    s.hurt_by[key] = true
end

-- How many monsters hurt the bot in room `room_id` (BotTactics.note_hurt).
function BotTactics.hurt_count(room_id)
    local hurt_by = state().hurt_by
    local n = 0
    if hurt_by then
        local prefix = room_id .. ":"
        for key in pairs(hurt_by) do
            if key:sub(1, #prefix) == prefix then
                n = n + 1
            end
        end
    end
    return n
end

function BotTactics.decide(o)
    if not o.policy then
        return "run", "not in the knowledge file"
    end
    if o.policy ~= "fight" then
        return "run", "policy " .. o.policy
    end
    if not o.gun then
        return "run", "no firearm with rounds"
    end
    -- The file gives this monster bands, none for this gun: its hits do
    -- not tell (Black Tiger knocks the handgun's aim down), so do not open
    -- a fight with it, closing in or not.
    if o.no_band then
        return "run", "no band for this gun"
    end
    -- Inside twice its reach it gets the bot anyway: shoot what is left.
    -- So is one inside the band's far edge: a zombie closes on the bot
    -- about as fast as the handgun brings it down, so a fight opened at
    -- twice its reach (3000) ends in its grab, while one opened at the
    -- band's edge (4000) gets the two more shots that finish it first.
    -- At low health only what the bot cannot outrun (or may not).
    local engage = (o.engage or 0) + (o.ahead and AHEAD or 0)
    local closing = o.reach and o.rounds > 0
        and (o.distance <= 2 * o.reach or o.distance <= engage)
    if closing and o.outrun ~= true then
        return "fight", MUST_FIGHT
    end
    -- A cautious bot wants a margin of rounds: half again at risk 0.
    local margin = 1 + 0.5 * (1 - o.risk)
    local enough = o.rounds >= o.needed * margin
        or (o.hurt_before == true and o.rounds >= o.needed)
    if o.health <= o.max_health * (0.45 - 0.3 * o.risk) then
        -- One slower than the backward walk is shot from range when the
        -- rounds cover it: running past a zombie in a corridor ends in its
        -- grab, which low health does not survive.
        if o.slow and enough then
            return "fight", SLOW_FIGHT
        end
        return "run", "low health"
    end
    -- One it can outrun but not bring down with a margin is outrun: a
    -- fight that ends empty-handed costs the grabs it took on the way.
    if closing and o.sourcing and not enough then
        return "run", "closing in, too few rounds to finish it"
    end
    if closing then
        return "fight", "closing in"
    end
    if not enough then
        return "run", "too few rounds"
    end
    if o.hurt_before and o.rounds < o.needed * margin then
        return "fight", HURT_FIGHT
    end
    if o.crowd_needed and o.rounds < o.crowd_needed then
        return "run", "too few rounds for the crowd"
    end
    if o.outrun == false then
        return "fight", "cannot outrun it"
    end
    if o.crossing and o.sourcing then
        return "run", "sourcing ammunition: only what must be fought"
    end
    if o.crossing then
        return "fight", "the room is crossed again"
    end
    return "run", "only in the way"
end

-- Where to stand. `o`: distance, min, max (the knowledge band), reloading,
-- melee (a hand weapon: closes in past the band's far edge),
-- headshot (the weapon's headshot distance or nil), crawl_reach (a downed
-- enemy's reach or nil), reach and speed (the enemy's attack reach and
-- chase speed, nil when unknown). Returns "back", "retreat", "close" or
-- "hold", and the distance to walk to. Reloading backs off to the band's
-- far edge: the aim is down anyway. A target inside the band's near edge
-- (or a crawler's reach) is held and shot, never backed off from: leaving
-- the aim plays the holster (behaviour 0x17), which drifts the player a few
-- hundred units toward the target before a backward step lands, and the
-- shots it skips are the ones that would have finished the target, so a
-- zombie the bot backed off from caught it at once. A shotgun target just
-- past the band steps in to the middle, where a torso or up shot pops the
-- head; otherwise hold and shoot. The reloading back-off starts with
-- turning around (tank controls), so it is taken only when the enemy cannot
-- close the gap to its reach in that time. Before that, an enemy whose
-- chase speed is known and slower than the backward walk (o.back_speed) is
-- backpedalled from: "retreat" walks backwards to the same distance while
-- the bot keeps facing it, no turn needed.
function BotTactics.spacing(o)
    local turn_ticks = C.FULL_TURN // 2 // C.TURN_STEP
    local room_to_turn = not o.speed
        or o.distance - (o.reach or 0) > o.speed * turn_ticks
    local outpaced = o.back_speed ~= nil and o.speed ~= nil and o.speed < o.back_speed
    -- Inside its reach a back-off only shows it the bot's back (or is
    -- caught mid-step): shoot.
    if o.reach and o.distance < o.reach then
        room_to_turn = false
        outpaced = false
    end
    local min = o.min
    if o.crawl_reach and o.crawl_reach + C.APPROACH > min then
        min = o.crawl_reach + C.APPROACH
    end
    local middle = (min + o.max) // 2
    if o.max > min and middle < min then
        middle = min
    end
    if o.reloading then
        if o.distance < o.max and outpaced then
            return "retreat", o.max
        end
        if o.distance < o.max and room_to_turn then
            return "back", o.max
        end
        return "hold", o.distance
    end
    if o.distance < min then
        return "hold", o.distance
    end
    -- A hand weapon reaches only inside its band: walk in.
    if o.melee and o.distance > o.max then
        return "close", middle
    end
    -- Close in only from outside the headshot distance: inside it the shot
    -- already pops the head, and a walk there only delays it.
    if o.headshot and o.distance > o.max and o.distance >= o.headshot
        and o.distance <= o.max + (o.max - min) then
        return "close", middle
    end
    return "hold", o.distance
end

-- Whether the spot (gx, gz) lies behind the player at (px, pz) as seen from
-- the target at (tx, tz): within 1/8 turn of straight away from it, so a
-- backward walk facing the target reaches it. A sideways spot (a wall
-- straight behind) needs a turn, which a retreat does not take.
function BotTactics.behind(px, pz, tx, tz, gx, gz)
    -- Floats: the squared products overflow integers across a room.
    local ax, az = (gx - px) * 1.0, (gz - pz) * 1.0
    local bx, bz = (tx - px) * 1.0, (tz - pz) * 1.0
    local dot = ax * bx + az * bz
    -- cos(angle) <= -cos(1/8 turn), squared: dot^2 >= |a|^2 |b|^2 / 2.
    return dot < 0 and 2 * dot * dot >= (ax * ax + az * az) * (bx * bx + bz * bz)
end

-- A held weapon's rounds: its loaded quantity plus the matching ammunition
-- held, the same count the equipped gun's clip plus reserve gives.
local function held_rounds(bot, row)
    local ammo = bot:ammo_for(row.item)
    return row.quantity + (ammo and bot:item_count(ammo) or 0)
end

-- The gun to fight `entry` (a knowledge enemy, or nil) with `health` left
-- (nil: the knowledge maximum), from `guns`: rows {item, rounds, equipped}
-- with rounds > 0. The plain pick is the equipped gun, else the one with the
-- most rounds. It stands unless its rounds fall short of the kill (its keep
-- band's damage against that health) times `margin` (nil: 1; misses cost
-- rounds) while another gun's cover that: then the gun that covers it in
-- the fewest shots. With no gun covering the margin, the plain pick stands
-- if it covers the bare kill, else the fewest-shot gun that does; a plain
-- pick with no kill data gives way to a gun with data that covers it. A
-- reserve gun (row.reserve: the launcher's scarce rounds) is a switch only
-- when no other gun covers the bare kill. The enemy's `open` weapon, held
-- with rounds, is the pick while its `prefer`red one is not held. So a
-- stronger gun (the shotgun) is spent only on a fight the plain pick
-- cannot finish, and a fight with no kill data keeps the plain pick.
-- Returns the row or nil.
function BotTactics.pick_gun(guns, entry, health, margin, crowd)
    health = health or (entry and entry.health)
    margin = margin or 1
    local function shots(row)
        local band = entry and entry.keep and entry.keep[row.item]
        local damage = band and band.damage
        if not damage or damage <= 0 or not health or health < 0 then
            return nil
        end
        return health // damage + 1
    end
    local plain
    for _, row in ipairs(guns) do
        if not plain or (row.equipped and not plain.equipped)
            or (row.equipped == plain.equipped and row.rounds > plain.rounds) then
            plain = row
        end
    end
    -- The knowledge's preferred weapon, held with the rounds for the kill.
    -- A row's `base` (knowledge variant_of: the launcher loaded with other
    -- rounds) counts as that weapon for `prefer` and `open`.
    local function is(row, id)
        return id ~= nil and (row.item == id or row.base == id)
    end
    local prefer = entry and entry.prefer
    local prefer_held = false
    for _, row in ipairs(guns) do
        prefer_held = prefer_held or is(row, prefer)
        local n = is(row, prefer) and shots(row)
        if n and n <= row.rounds then
            return row
        end
    end
    -- Its opening weapon (`open`), while the preferred one is not held:
    -- any rounds it has go first (303's Tyrant, taken to 200 health by the
    -- launcher, where Brad drops the rocket launcher).
    local open = entry and entry.open
    if open and not prefer_held then
        for _, row in ipairs(guns) do
            if is(row, open) then
                return row
            end
        end
    end
    -- Two or more grabbers near (`crowd`, the count): a reserve gun that
    -- covers the kill goes first, so the fight is short. 308's two
    -- hunters met a handgun holding the rounds for both and swiped the
    -- bot to death while it took them down seven shots each.
    -- Only for one the bot cannot outrun: zombies are walked past.
    if entry and entry.grabs and entry.outrun ~= true and (crowd or 0) >= 2 then
        for _, row in ipairs(guns) do
            local n = row.reserve and shots(row)
            if n and n <= row.rounds then
                return row
            end
        end
    end
    local need = plain and shots(plain)
    local spare_plain = plain and not plain.reserve
    if need and need * margin <= plain.rounds and spare_plain then
        return plain
    end
    -- `spend`: a reserve gun (row.reserve, the launcher) may be picked.
    local function fewest(scale, spend)
        local best, best_shots = nil, nil
        for _, row in ipairs(guns) do
            local n = shots(row)
            if n and n * scale <= row.rounds and (spend or not row.reserve)
                and (not best_shots or n < best_shots) then
                best, best_shots = row, n
            end
        end
        return best
    end
    -- The plain pick has no kill data against a known enemy (the grenade
    -- launcher equipped for a boss, meeting a hunter): a gun with data
    -- that covers the kill replaces it.
    local covered = fewest(margin, false)
    if covered then
        return covered
    end
    -- A grabber no ordinary gun covers with the margin: a reserve gun that
    -- does goes before the bare kill (a reserve gun has rounds here only
    -- past the band's `spare`, or when no ordinary gun finishes it). With
    -- five handgun rounds for 401's two zombies the bot ran past both
    -- (too few rounds) and was grabbed three times; the launcher, holding
    -- five, then killed each with one round.
    if entry and entry.grabs then
        covered = fewest(margin, true)
        if covered then
            return covered
        end
    end
    if need and need <= plain.rounds and spare_plain then
        return plain
    end
    covered = fewest(1, false)
    if covered then
        return covered
    end
    -- Only a reserve gun covers it: the equipped one stands if it does.
    if need and need <= plain.rounds then
        return plain
    end
    return fewest(1, true) or plain
end

-- Rounds of a reserve gun (the launcher, every loading of it) kept from
-- ordinary monsters for a boss the bot has not met: it knows only that
-- bosses come, not which or where. Scaled by health: a bot near full health
-- runs past or uses the handgun; a hurt one spends what clears the room.
local GENERAL_RESERVE = 4

local function general_reserve(bot)
    local player = bot:player()
    local max = player.max_health or 0
    if max <= 0 then
        return GENERAL_RESERVE
    end
    return (GENERAL_RESERVE * player.health + max // 2) // max
end

-- The rounds a met <fight> still to be won (BotNav.fight_met) keeps from
-- weapon `base`: its `save`, else the kill at its `base` band's damage.
local function fight_rounds(bot, k, fight, base)
    local entry = k.enemies[fight.enemy]
    if not (entry and (entry.prefer == base or entry.open == base))
        or (fight.bank and bot:flag(fight.bank, fight.bit))
        or not BotNav.fight_met(bot, fight) then
        return nil
    end
    local band = entry.keep and entry.keep[base]
    local damage = band and band.damage
    local kill = damage and damage > 0 and entry.health
        and entry.health // damage + 1
    return entry.save or kill or 0
end

-- Rounds of weapon `base` (every loading of it: knowledge variant_of)
-- kept from the monster `enemy_id`: the met <fight>s still to be won that
-- prefer or open with it, plus the general reserve for a reserve gun. A
-- fight's own enemy spends them all (rationing 40C's Plant 42 for the later
-- bosses killed the bot there), so it gives 0. Bosses not met yet keep
-- nothing beyond the general reserve: the bot does not know them.
function BotTactics.saved_rounds(bot, k, base, enemy_id)
    local saved = 0
    for _, fight in ipairs(bot:fight_facts()) do
        local rounds = fight_rounds(bot, k, fight, base)
        if rounds then
            if fight.enemy == enemy_id then
                return 0
            end
            saved = saved + rounds
        end
    end
    local row = k.weapons[base]
    if row and row.reserve then
        saved = saved + general_reserve(bot)
    end
    return saved
end

-- Every round held for weapon `base`: each loading's loaded rounds and
-- its ammunition.
local function family_rounds(bot, k, base)
    local total = 0
    for id, row in pairs(k.weapons) do
        if id == base or row.variant_of == base then
            local ammo = bot:ammo_for(id)
            total = total + bot:item_count(id) + (ammo and bot:item_count(ammo) or 0)
        end
    end
    return total
end

-- The firearm to fight `enemy` (the nearest fightable monster, or nil)
-- with and its rounds, counting the ammunition held for each held weapon
-- with knowledge, so an emptied gun with reserve still counts; see
-- BotTactics.pick_gun. A hand weapon (the knife) counts only for an enemy
-- that prefers it, with rounds that never run out. `crowd` (optional):
-- every grabber of the same kind near the bot, `enemy` among them; an
-- ordinary gun finishes only the rounds for all of them.
local function firearm(bot, k, enemy, crowd)
    local weapon = bot:weapon()
    local entry = enemy and k.enemies[enemy.id]
    local function melee(item)
        local row = k.weapons[item]
        return row and row.melee
    end
    -- A hand weapon's rounds: enough for any kill (a hit takes at least 1).
    local hand_rounds = (enemy and enemy.health or 0) + 1
    local guns = {}
    if weapon.equipped and k.weapons[weapon.equipped] then
        if melee(weapon.equipped) then
            if entry and entry.prefer == weapon.equipped then
                guns[1] = { item = weapon.equipped, rounds = hand_rounds, equipped = true }
            end
        elseif weapon.clip + weapon.reserve > 0 then
            guns[1] = { item = weapon.equipped, rounds = weapon.clip + weapon.reserve,
                equipped = true, reserve = k.weapons[weapon.equipped].reserve == true,
                base = k.weapons[weapon.equipped].variant_of }
        end
    end
    for _, row in ipairs(bot:menu().items or {}) do
        if row.kind == "weapon" and k.weapons[row.item] and row.item ~= weapon.equipped then
            local rounds = held_rounds(bot, row)
            if melee(row.item) then
                rounds = entry and entry.prefer == row.item and hand_rounds or 0
            end
            if rounds > 0 then
                guns[#guns + 1] = { item = row.item, rounds = rounds, equipped = false,
                    reserve = k.weapons[row.item].reserve == true,
                    base = k.weapons[row.item].variant_of }
            end
        end
    end
    local margin = 1 + 0.5 * (1 - risk(bot))
    -- An ordinary gun holds the rounds for the kill, with the margin.
    -- With a crowd it holds the rounds for all of them: 509's four naked
    -- zombies met a handgun holding enough for one, the launcher's spare
    -- kept back, and grabbed the bot twenty times.
    local finish = false
    for _, row in ipairs(guns) do
        local band = entry and entry.keep and entry.keep[row.item]
        local damage = band and band.damage
        if not row.reserve and damage and damage > 0 and enemy and enemy.health then
            local shots = 0
            for _, e in ipairs(crowd or { enemy }) do
                shots = shots + e.health // damage + 1
            end
            if shots * margin <= row.rounds then
                finish = true
            end
        end
    end
    -- A reserve gun spends only the rounds the <fight>s ahead leave
    -- (BotTactics.saved_rounds).
    for i = #guns, 1, -1 do
        local row = guns[i]
        if row.reserve then
            local base = row.base or row.item
            -- A band's `spare`: the rounds it leaves held (a zombie
            -- gets a grenade only while the lab's fights keep theirs),
            -- only while an ordinary gun can finish it and it has not hurt
            -- the bot yet: running past costs a grab (401's zombies grabbed
            -- twice past a handgun holding five rounds, then five times
            -- past an empty one, the launcher holding six).
            local band = entry and entry.keep and entry.keep[row.item]
            local hurt = enemy and state().hurt_by
                and state().hurt_by[hurt_key(bot:room().id, enemy.slot)]
            local keep = band and finish and not hurt and band.spare or 0
            local spare = family_rounds(bot, k, base)
                - BotTactics.saved_rounds(bot, k, base, enemy and enemy.id)
                - keep
            if spare < row.rounds then
                row.rounds = spare
            end
            if row.rounds <= 0 then
                table.remove(guns, i)
            end
        end
    end
    -- The crowd rule spends only what the met bosses and the general
    -- reserve leave: 308's hunters took the rounds 513's Tyrant needed.
    local crowd_n = crowd and #crowd or 0
    for _, row in ipairs(guns) do
        if row.reserve and crowd_n >= 2 then
            local base = row.base or row.item
            if family_rounds(bot, k, base) - BotTactics.saved_rounds(bot, k, base) <= 0 then
                crowd_n = 0
            end
        end
    end
    local best = BotTactics.pick_gun(guns, entry, enemy and enemy.health, margin, crowd_n)
    if not best then
        return nil, 0
    end
    return best.item, best.rounds
end

local function rounds_for(enemy, entry, gun)
    local band = entry and entry.keep and entry.keep[gun]
    local damage = band and band.damage
    if not damage or damage <= 0 then
        return enemy.health
    end
    return enemy.health // damage + 1
end

local function crossing(bot)
    local here = bot:room().id
    local count = 0
    for _, row in ipairs(bot:world().frontier or {}) do
        if row.room == here then
            count = count + 1
        end
    end
    -- The door the bot leaves by is one of them.
    return count > 1
end

local function band_of(entry, gun)
    local band = entry and entry.keep and gun and entry.keep[gun]
    if band then
        return band.min, band.max
    end
    return 2 * C.REACH, 6 * C.REACH
end

local function reset(s)
    s.active = false
    s.target = nil
    s.ticks = 0
    s.since_hit = 0
    s.aim_ticks = 0
    s.reload_ticks = 0
    s.reload_started = false
    s.last_health = nil
    s.shots = 0
    s.unseen = 0
    s.space = nil
    s.space_ticks = 0
    s.space_rest = 0
    s.fired_tick = nil
    s.retreat_ticks = 0
    s.retreat_shots = nil
    s.must = nil
end

local function stop(bot, s, reason, calm)
    if s.active then
        local player = bot:player()
        local weapon = bot:weapon()
        bot:log(string.format(
            "combat end: %s slot=%s shots=%d ticks=%d health=%d clip=%d reserve=%d",
            reason, tostring(s.target), s.shots, s.ticks, player.health,
            weapon.clip, weapon.reserve))
    end
    reset(s)
    if calm then
        s.calm_until = bot:tick() + calm
    end
end

local function enemy_by_slot(enemies, slot)
    for i = 1, #enemies do
        if enemies[i].slot == slot then
            return enemies[i]
        end
    end
    return nil
end

-- Whether the planner's walks should plan around the monsters: a run
-- decision is in force, or the bot is sourcing ammunition.
function BotTactics.avoiding()
    local s = state()
    return s.sourcing == true or (s.avoid_until or 0) > (s.now or 0)
end

function BotTactics.active()
    return state().active == true
end

-- Ticks in a row `enemy` has shown no aim bit any pose can hit, counted
-- across choose and step, while it has not shown one since the bot entered
-- the room: a monster not woken yet. Once it has (`shootable`), a later
-- gap (a dog in mid-leap) is not dormancy and counts 0. A look away longer
-- than a second (it may have woken meanwhile) restarts the count.
function BotTactics.no_aim_ticks(s, enemy, shootable)
    s.no_aim = s.no_aim or {}
    local row = s.no_aim[enemy.slot]
    if shootable or (row and row.woke) then
        s.no_aim[enemy.slot] = { woke = true }
        return 0
    end
    if not row or s.now - row.last > C.SECOND then
        row = { since = s.now }
        s.no_aim[enemy.slot] = row
    end
    row.last = s.now
    return s.now - row.since
end

-- Whether knowledge enemy `entry` is fought with a hand weapon (the web
-- and the knife): it is fought only as a <fight> step's target, never with
-- a firearm.
local function by_hand(k, entry)
    local weapon = entry and entry.prefer and k.weapons[entry.prefer]
    return weapon ~= nil and weapon.melee == true
end

-- Decide every monster near the bot; returns the fight target or nil.
local function choose(bot, s, k, enemies, player)
    -- A <fight> step's target fought by hand is the fight once it is inside
    -- the weapon's band, whatever else is near.
    local focus = s.focus and enemy_by_slot(enemies, s.focus)
    local fentry = focus and k.enemies[focus.id]
    if focus and by_hand(k, fentry) and focus.visible then
        local gun = firearm(bot, k, focus)
        local band = gun and fentry.keep[gun]
        if band and k.weapons[gun].melee and focus.distance <= band.max then
            focus.why = MUST_FIGHT
            focus.must = true
            return focus, gun
        end
    end
    -- An enemy whose preferred weapon lies here as a pickup is no target
    -- until it is taken (BotTactics.step).
    local kept = {}
    for i = 1, #enemies do
        if not BotPuzzle.preferred_pickup(bot, enemies[i].id) then
            kept[#kept + 1] = enemies[i]
        end
    end
    enemies = kept
    local nearest
    for i = 1, #enemies do
        local entry = k.enemies[enemies[i].id]
        if entry and entry.policy == "fight" and not by_hand(k, entry)
            and (not nearest or enemies[i].distance < nearest.distance) then
            nearest = enemies[i]
        end
    end
    local crowd = nil
    local nentry = nearest and k.enemies[nearest.id]
    if nentry and nentry.grabs then
        crowd = {}
        -- The whole room's: 308's second hunter joined a fight opened on
        -- the first.
        for i = 1, #enemies do
            if enemies[i].id == nearest.id and (enemies[i].health or 0) >= 0 then
                crowd[#crowd + 1] = enemies[i]
            end
        end
    end
    local gun, rounds = firearm(bot, k, nearest, crowd)
    local near, fightable = {}, {}
    for i = 1, #enemies do
        local enemy = enemies[i]
        local entry = k.enemies[enemy.id]
        local _, far = band_of(entry, gun)
        if not by_hand(k, entry) and enemy.distance <= (far > NEAR and far or NEAR) then
            near[#near + 1] = enemy
            if entry and entry.policy == "fight" then
                fightable[#fightable + 1] = enemy
            end
        end
    end
    if #near == 0 then
        return nil
    end
    local crowd_needed = 0
    for _, enemy in ipairs(fightable) do
        crowd_needed = crowd_needed + rounds_for(enemy, k.enemies[enemy.id], gun)
    end
    local cross = nil
    local arc = C.FULL_TURN // 4
    local best = nil
    for _, enemy in ipairs(near) do
        local entry = k.enemies[enemy.id]
        -- Its band's far edge for this gun (decide's `engage`).
        local _, far = band_of(entry, gun)
        do
            if cross == nil then
                if not s.cross_tick or bot:tick() - s.cross_tick >= C.SECOND then
                    s.cross = crossing(bot)
                    s.cross_tick = bot:tick()
                end
                cross = s.cross
            end
            local verdict, why = BotTactics.decide({
                policy = entry and entry.policy,
                gun = gun ~= nil and rounds > 0,
                rounds = rounds,
                needed = rounds_for(enemy, entry, gun),
                crowd_needed = #fightable > 1 and crowd_needed or nil,
                health = player.health,
                max_health = player.max_health,
                risk = risk(bot),
                crossing = cross,
                sourcing = s.sourcing,
                distance = enemy.distance,
                reach = entry and entry.reach,
                engage = far,
                outrun = entry and entry.outrun,
                slow = entry ~= nil and entry.speed ~= nil and entry.speed < C.BACK_SPEED,
                hurt_before = s.hurt_by ~= nil and s.hurt_by[hurt_key(bot:room().id, enemy.slot)] == true,
                ahead = entry ~= nil and entry.grabs == true
                    and abs(angle_delta(enemy.bearing, player.angle)) <= AHEAD_ARC,
                no_band = entry ~= nil and entry.keep ~= nil and next(entry.keep) ~= nil
                    and gun ~= nil and entry.keep[gun] == nil,
            })
            if verdict == "run" then
                s.avoid_until = bot:tick() + RUN_HOLD
                s.run_logs = s.run_logs or {}
                if s.run_logs[enemy.slot] ~= why then
                    s.run_logs[enemy.slot] = why
                    bot:log(string.format("run past slot=%d id=%#04x dist=%d: %s",
                        enemy.slot, enemy.id, enemy.distance, why))
                end
            elseif BotTactics.no_aim_ticks(s, enemy, BotTactics.pose(entry, gun and k.weapons[gun],
                    enemy.status_flags, enemy.distance) ~= nil) >= DORMANT_TICKS then
                -- No aim bit for a while: a monster the room has not woken
                -- yet (a zombie still feeding shows only 0x01) or one out of
                -- every aim's line. No shot can hit it, so do not stand
                -- aiming. The wait lets a waking monster's first tick pass.
                -- The walk gives it a berth, as a run past does: 114's
                -- zombie woke beside the herb walk and grabbed.
                s.avoid_until = bot:tick() + RUN_HOLD
                s.run_logs = s.run_logs or {}
                local note = string.format("no aim bit (status=0x%02X)", enemy.status_flags)
                if s.run_logs[enemy.slot] ~= note then
                    s.run_logs[enemy.slot] = note
                    bot:log(string.format("hold fire slot=%d id=%#04x dist=%d: %s",
                        enemy.slot, enemy.id, enemy.distance, note))
                end
            elseif enemy.visible
                and (abs(angle_delta(enemy.bearing, player.angle)) <= arc
                    or enemy.distance <= 2 * (entry.reach or C.REACH)) then
                local focused = best and best.slot == s.focus
                if enemy.slot == s.focus or (not focused
                        and (not best or enemy.distance < best.distance)) then
                    best = enemy
                    best.why = why
                    best.must = why == MUST_FIGHT or why == SLOW_FIGHT or why == HURT_FIGHT
                end
            end
        end
    end
    return best, gun
end

-- Prefer monster `slot` (nil: none) over nearer ones when it is fought:
-- a <fight> step's target, a boss's body among its vines. Cleared on a
-- room change.
function BotTactics.focus(slot)
    state().focus = slot
end

-- Reload the empty equipped gun in the inventory menu: a MOVE of its
-- ammunition onto it (or of it onto the ammunition, whichever
-- bot:combine_result applies). The menu stops the game, so a monster that
-- hits faster than the aim-and-fire reload (a hunter's swipes, about every
-- 50 ticks, against a 40-tick reload each hit resets) cannot keep the clip
-- empty. One try per MENU_RELOAD_GAP; the field reload is the fallback.
local MENU_RELOAD_GAP = 4 * C.SECOND
local function menu_reload(bot, s, weapon)
    if s.menu_reload_tick and bot:tick() - s.menu_reload_tick < MENU_RELOAD_GAP then
        return false
    end
    -- No rounds of its own: the launcher takes its other rounds
    -- (BotInventory.load_now; the gun becomes that loading's item).
    if weapon.reserve <= 0 then
        local pick = weapon.clip <= 0 and BotInventory.load_now(bot)
        if not pick then
            return false
        end
        s.menu_reload_tick = bot:tick()
        return BotMenu.combine(bot, pick.slot, pick.target,
            string.format("combat load 0x%02X onto 0x%02X", pick.item, pick.other))
    end
    local ammo = bot:ammo_for(weapon.equipped)
    local gun_slot, ammo_slot
    for i, row in ipairs(bot:menu().items or {}) do
        if row.item == weapon.equipped and not gun_slot then
            gun_slot = i - 1
        elseif ammo and row.item == ammo and not ammo_slot then
            ammo_slot = i - 1
        end
    end
    if not gun_slot or not ammo_slot then
        return false
    end
    s.menu_reload_tick = bot:tick()
    local label = string.format("combat reload 0x%02X", weapon.equipped)
    for _, pair in ipairs({ { ammo_slot, gun_slot }, { gun_slot, ammo_slot } }) do
        local result = bot:combine_result(pair[1], pair[2])
        if result and result.result == "applied" then
            return BotMenu.combine(bot, pair[1], pair[2], label)
        end
    end
    return false
end

-- An acid glob in flight within DODGE_RADIUS (horizontally) is dodged: the
-- splash tests the player within 600 or 900 of where it lands (behaviours 22
-- and 55). Plant 42's drops from the ceiling onto where the bot stood (about
-- 30 ticks); the Black Tiger's flies at it (11). Standing to aim took every
-- one: 18 hits from 40C's plant in a run.
-- Five approach steps (1500): past the 900 splash with a margin.
local DODGE_RADIUS = 5 * C.APPROACH
local DODGE_STEP = DODGE_RADIUS + C.APPROACH

-- The dodge for this tick, or nil: run straight away from the nearest glob
-- still in the air, else across its line.
local function dodge(bot, s, player)
    local nearest, best
    for _, g in ipairs(bot:projectiles()) do
        local d = bot:distance(g.x, g.z)
        if g.y < 0 and d < DODGE_RADIUS and (not best or d < best) then
            nearest, best = g, d
        end
    end
    if not nearest then
        s.dodge = nil
        return false
    end
    local dx, dz = player.x - nearest.x, player.z - nearest.z
    if best < 50 then
        -- Right over the bot: away along its facing's side.
        dx, dz = C.RING[3][1], C.RING[3][2]
        best = C.RING[1][1]
    end
    local tries = {
        { dx, dz }, { -dz, dx }, { dz, -dx },
    }
    for _, dir in ipairs(tries) do
        local x = player.x + dir[1] * DODGE_STEP // best
        local z = player.z + dir[2] * DODGE_STEP // best
        local result = bot:walk_to(x, z, { run = true, tolerance = C.APPROACH, avoid_enemies = true })
        if result.status == "moving" then
            if not s.dodge then
                bot:log(string.format("dodge: glob at (%d,%d) %d away", nearest.x, nearest.z, best))
            end
            s.dodge = true
            return true
        end
    end
    return false
end

function BotTactics.step(bot, scene, player)
    local s = state()
    s.now = bot:tick()
    if not s.sourcing_tick or s.now - s.sourcing_tick >= C.SECOND then
        s.sourcing_tick = s.now
        local was = s.sourcing
        s.sourcing = BotSurvival.sourcing_now(bot)
        if s.sourcing ~= (was or false) then
            bot:log(s.sourcing and "sourcing ammunition: explore, avoid monsters, fight only what must be fought"
                or "sourcing ammunition: over")
        end
    end
    if not scene.controllable or player.grabbed then
        return false
    end
    if player.state == 1 and dodge(bot, s, player) then
        s.aim_ticks = 0
        return true
    end
    local room = bot:room().id
    -- A boss seen in its <fight>'s room is met (BotNav.fight_met): what the
    -- bot carries and saves now counts it.
    if s.met_tick ~= s.now // C.SECOND then
        s.met_tick = s.now // C.SECOND
        for _, fight in ipairs(bot:fight_facts()) do
            if fight.room == room and not BotNav.fight_met(bot, fight) then
                for _, e in ipairs(bot:enemies()) do
                    if e.id == fight.enemy and (e.health or 0) >= 0 then
                        bot:remember(BotNav.fight_met_key(room, fight.enemy), "1")
                        bot:log(string.format("met the fight in %s (enemy 0x%02X)", room, fight.enemy))
                        break
                    end
                end
            end
        end
    end
    if s.room ~= room then
        stop(bot, s, "left the room")
        s.room = room
        s.cross_tick = nil
        s.run_logs = {}
        s.no_aim = {}
        s.avoid_until = nil
        s.focus = nil
    end
    if bot:tick() < (s.calm_until or 0) and not s.active then
        return false
    end
    local k = bot:knowledge()
    local enemies = bot:enemies()
    local target = s.active and enemy_by_slot(enemies, s.target) or nil
    if s.active and not target then
        stop(bot, s, string.format("target down health=%s", tostring(s.last_health)))
        return false
    end
    -- The target's preferred weapon lies here as a pickup: the planner
    -- takes it first (303's Tyrant stays at 200 health until Brad's
    -- rocket launcher; shooting on spent the rounds and the health).
    if target and BotPuzzle.preferred_pickup(bot, target.id) then
        stop(bot, s, "its preferred weapon lies here: take it first")
        return false
    end
    if not s.active then
        local gun
        target, gun = choose(bot, s, k, enemies, player)
        if not target then
            return false
        end
        reset(s)
        s.active = true
        s.gun = gun
        s.target = target.slot
        s.must = target.must
        s.last_health = target.health
        s.avoid_until = nil
        bot:log(string.format(
            "combat start slot=%d id=%#04x dist=%d health=%d player_health=%d gun=0x%02X: %s",
            target.slot, target.id, target.distance, target.health, player.health, gun,
            target.why or "fight"))
    end
    local entry = k.enemies[target.id]
    local wrow = k.weapons[s.gun]
    s.ticks = s.ticks + 1
    s.since_hit = (s.since_hit or 0) + 1
    if s.since_hit > MAX_TICKS then
        stop(bot, s, "timeout", LOST_CALM)
        return false
    end
    -- A fight against what cannot be outrun is not given up for health.
    if not s.must and player.health <= player.max_health * (0.45 - 0.3 * risk(bot)) then
        stop(bot, s, "low health", LOST_CALM)
        return false
    end
    if not target.visible then
        s.unseen = s.unseen + 1
        if s.unseen > C.SECOND then
            stop(bot, s, "target lost", LOST_CALM)
            return false
        end
    else
        s.unseen = 0
    end
    if s.last_health and target.health < s.last_health then
        bot:log(string.format("combat hit slot=%d health=%d->%d",
            target.slot, s.last_health, target.health))
        -- A fight that lands hits is not timed out: 40C's plant, shot from
        -- past its vines' reach, took the shotgun's hits past MAX_TICKS,
        -- and the bot walked back into the vines between the fights.
        s.since_hit = 0
    end
    s.last_health = target.health

    -- A hit while fighting what never attacks (BotNav.harmless: 30C's web)
    -- came from something no fight here can answer: the fight stands down
    -- and the fight goal steps out of the room (BotNav.hold_fight).
    if player.state == HIT_STATE and BotNav.harmless(entry) then
        stop(bot, s, "hurt by something unseen", LOST_CALM)
        return false
    end
    -- A hit reaction holds the player and drops the aim or the reload:
    -- wait it out, then start them again.
    if player.state == HIT_STATE then
        s.reload_started = false
        s.reload_ticks = 0
        s.aim_ticks = 0
        return true
    end

    local weapon = bot:weapon()
    -- The gun loaded with other rounds (BotInventory.load_now) is the same
    -- gun under that loading's item; the load leaves nothing equipped.
    if weapon.equipped ~= s.gun and wrow and not bot:has_item(s.gun) then
        local base = wrow.variant_of or s.gun
        for id, row in pairs(k.weapons) do
            if (row.variant_of or id) == base and bot:has_item(id) then
                s.gun = id
                wrow = row
            end
        end
    end
    if weapon.equipped ~= s.gun then
        if not BotMenu.equip(bot, s.gun, "combat equip") then
            stop(bot, s, "cannot equip the firearm", LOST_CALM)
        end
        return true
    end
    local melee = wrow and wrow.melee
    if not melee and weapon.clip <= 0 and weapon.reserve <= 0 then
        if menu_reload(bot, s, weapon) then
            return true
        end
        stop(bot, s, "out of ammunition", LOST_CALM)
        return false
    end
    -- A target no aim can hit (not woken yet) is let go; choose skips it
    -- until it shows an aim bit.
    if BotTactics.no_aim_ticks(s, target, BotTactics.pose(entry, wrow, target.status_flags,
            target.distance) ~= nil) >= DORMANT_TICKS then
        stop(bot, s, string.format("no aim bit (status=0x%02X)", target.status_flags))
        return false
    end
    -- The cadence the engine shows: ticks from a fire press to the hold.
    if s.fired_tick and weapon.behavior == C.HOLD_BEHAVIOR and s.fired_left then
        bot:log(string.format("cadence %d ticks", bot:tick() - s.fired_tick))
        s.fired_tick = nil
    end
    if s.fired_tick and weapon.behavior ~= C.HOLD_BEHAVIOR then
        s.fired_left = true
    end

    -- Spacing first: a walk drops the aim.
    local min, max = band_of(entry, s.gun)
    local reloading = not melee and weapon.clip <= 0
    local move, want = BotTactics.spacing({
        distance = target.distance,
        min = min,
        max = max,
        melee = melee,
        reloading = reloading,
        -- Only a head that pops is worth walking in for (a hunter's swipe
        -- met the shotgun's walk in to the band's middle).
        headshot = entry and entry.head and wrow and wrow.headshot or nil,
        crawl_reach = target.status_flags & C.STATUS_TORSO == 0 and entry
            and entry.crawl_reach or nil,
        reach = entry and entry.reach,
        speed = entry and entry.speed,
        back_speed = C.BACK_SPEED,
    })
    -- A retreat walks backwards, still facing the target, to the distance
    -- spacing asked for, then the bot shoots again: a new retreat waits for
    -- a shot after the last one. It needs the engine's spacing spot (clear
    -- of the room and the other monsters, a straight walk away) to lie
    -- behind the bot; otherwise the bot holds and shoots.
    if move == "retreat" and not s.retreat_want and not s.reload_started
        and (not s.retreat_shots or s.shots > s.retreat_shots) then
        s.retreat_want = want
        s.retreat_ticks = 0
        s.space = nil
        bot:log(string.format("combat retreat to %d from slot=%d (dist=%d)",
            want, target.slot, target.distance))
    end
    if s.retreat_want then
        local ended = nil
        local point = nil
        if target.distance >= s.retreat_want - SPACE_TOLERANCE then
            ended = "at range"
        elseif s.retreat_ticks >= RETREAT_TICKS then
            ended = "timeout"
        elseif s.reload_started or (entry and entry.reach and target.distance < entry.reach) then
            ended = "inside its reach"
        else
            point = bot:spacing_point(target.slot, s.retreat_want)
            if not point or not BotTactics.behind(player.x, player.z, target.x, target.z,
                    point.x, point.z) then
                ended = "nothing clear behind"
            end
        end
        if not ended then
            s.retreat_ticks = s.retreat_ticks + 1
            bot:face_to(target.x, target.z)
            bot:set_input({ down = true })
            s.aim_ticks = 0
            return true
        end
        bot:log(string.format("combat retreat end: %s dist=%d ticks=%d",
            ended, target.distance, s.retreat_ticks))
        s.retreat_want = nil
        s.retreat_shots = s.shots
    end
    if move == "retreat" then
        move = "hold"
    end
    if s.space_rest > 0 then
        s.space_rest = s.space_rest - 1
        move = "hold"
    end
    if move ~= "hold" and not s.reload_started then
        if not s.space or s.space.move ~= move then
            local point = bot:spacing_point(target.slot, want)
            s.space = point and { move = move, x = point.x, z = point.z } or nil
            s.space_ticks = 0
            if point then
                bot:log(string.format("combat %s to %d from slot=%d (dist=%d) at (%d,%d)",
                    move, want, target.slot, target.distance, point.x, point.z))
            end
        end
        if s.space then
            s.space_ticks = s.space_ticks + 1
            local result = bot:walk_to(s.space.x, s.space.z, {
                run = bot:distance(s.space.x, s.space.z) >= C.RUN_MIN_DISTANCE,
                tolerance = SPACE_TOLERANCE,
                avoid_enemies = true,
            })
            if result.status == "moving" and s.space_ticks <= SPACE_TICKS then
                s.aim_ticks = 0
                return true
            end
        end
        s.space = nil
        s.space_rest = SPACE_REST
    else
        s.space = nil
    end

    -- Face the target before raising the aim.
    if s.aim_ticks == 0 then
        local face = bot:face_to(target.x, target.z)
        if abs(face.angle_error) > FACE_BAND then
            return true
        end
    end

    if reloading and not s.reload_started and menu_reload(bot, s, weapon) then
        return true
    end
    if reloading then
        -- An empty clip with ammunition held: aim and fire reloads.
        s.reload_ticks = s.reload_ticks + 1
        if weapon.behavior == C.RELOAD_BEHAVIOR then
            s.reload_started = true
        end
        local input = { aim = true }
        if not s.reload_started and weapon.behavior == C.HOLD_BEHAVIOR
            and s.reload_ticks % 4 == 0 then
            input.fire = true
            input.fire_pressed = true
        end
        bot:set_input(input)
        s.aim_ticks = s.aim_ticks + 1
        if s.reload_ticks > RELOAD_WAIT * (s.reload_started and 2 or 1) then
            stop(bot, s, "reload did not finish", LOST_CALM)
            return false
        end
        return true
    end
    if s.reload_started then
        bot:log(string.format("reloaded in %d ticks clip=%d", s.reload_ticks, weapon.clip))
    end
    s.reload_started = false
    s.reload_ticks = 0

    local pose, why = BotTactics.pose(entry, wrow, target.status_flags, target.distance)
    if pose ~= s.pose then
        bot:log(string.format("aim %s at slot=%d (%s, status=0x%02X dist=%d)",
            tostring(pose), target.slot, why, target.status_flags, target.distance))
        s.pose = pose
    end
    local input = BotTactics.aim_input(pose or s.held_pose or "torso")
    if pose then
        s.held_pose = pose
    end
    s.aim_ticks = s.aim_ticks + 1
    -- Keep turning onto the target while the aim is up.
    local face = bot:face_to(target.x, target.z)
    local locked = weapon.target ~= nil
    -- A monster of parts (Plant 42) takes the shot on its next free part
    -- while the target's own hit reaction runs; with no part free the shot
    -- meets nothing (nine shotgun shells into 40C's plant while every part
    -- held hit state 1).
    local reacting = target.hit_state ~= 0
    if entry and entry.parts then
        reacting = true
        for _, e in ipairs(enemies) do
            if e.id == target.id and e.health >= 0 and e.hit_state == 0 then
                reacting = false
            end
        end
    end
    -- A hunter's dodge (enemy/06 behaviour 6) hops clear of the shot from
    -- its first frame and swipes on landing: a round fired into the hop
    -- missed, and its pump cost the shot at the swipe's rise (0x80, up).
    local leaping = target.id == HUNTER and target.action_behavior == HUNTER_DODGE
        and target.action_state <= 1
    if not pose or leaping or weapon.behavior ~= C.HOLD_BEHAVIOR or reacting
        or (not locked and s.aim_ticks < AIM_SETTLE)
        or abs(face.angle_error) > FACE_BAND * 2 then
        -- Waiting out a parts monster's shared hit state is no lock trouble.
        if reacting and entry and entry.parts then
            s.aim_ticks = AIM_SETTLE
        end
        if s.aim_ticks > LOCK_WAIT * 3 then
            stop(bot, s, pose and "never ready to fire" or "no aim pose hits it", LOST_CALM)
            return false
        end
        bot:set_input(input)
        return true
    end
    input.fire = true
    input.fire_pressed = true
    bot:set_input(input)
    s.shots = s.shots + 1
    s.fired_tick = bot:tick()
    s.fired_left = false
    s.aim_ticks = AIM_SETTLE
    return true
end
