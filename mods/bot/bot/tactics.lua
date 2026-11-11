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
-- Live grabbers in one room that make it a death trap: in RE1 a zombie's
-- bite stuns, the rest take their turns, and the bot rarely gets out.
-- 509's four naked zombies, run past with the launcher's explosive rounds
-- kept for bosses, bit it from 96 to dead. So they are fought, the
-- reserve given up (BotTactics.trap).
local TRAP_CROWD = 3
local TRAP_FIGHT = "a death trap of grabbers: kill them"
-- Shots a trap zombie that make an ordinary gun fast enough for a trap.
local TRAP_FAST = 2

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
    -- A death trap (BotTactics.trap: its zombies still standing) the rounds cover is
    -- fought at any health and while sourcing: a run past it is bitten.
    if o.trap and o.trap > 0 and o.trap_covered then
        return "fight", TRAP_FIGHT
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
-- chase speed, nil when unknown), low_reach (how far a shot aimed down
-- lands, the knowledge's down_reach, or nil). Returns "back", "retreat", "close", "approach" or
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
    -- A downed target past where a low shot lands (o.low_reach): walk in
    -- ("approach") to the band's far edge, or just inside that reach.
    -- Raising the aim sets the bot back (240 to 720 measured): 214's
    -- fight opened at 3732, the draw put the bot at 4430, and it stood
    -- there and fired at the floor in front of the zombie. Nine in ten of
    -- the shots from past 4400 missed, eight handgun rounds a run. (A walk
    -- in far enough for the draw to end inside the reach was tried: over
    -- the zombie rooms it cost 12 more hits than this one, which shoots
    -- from where the second draw leaves it.)
    if o.low_reach and o.crawl_reach and not o.melee and o.distance > o.low_reach then
        local want = o.max < o.low_reach and o.max or o.low_reach - C.APPROACH
        return "approach", want > min and want or min
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
function BotTactics.pick_gun(guns, entry, health, margin, crowd, trap)
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
    -- Only for one the bot cannot outrun: zombies are walked past, unless
    -- they make a death trap (`trap`).
    if entry and entry.grabs and (entry.outrun ~= true or trap) and (crowd or 0) >= 2 then
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

-- Rounds of weapon `base` the <fight>s of the current stage still to be
-- won need, met or not (a fact's `stage`, else its room's): each one's
-- `save`, else the kill at its `base` band's damage. A death trap that an
-- ordinary gun can clear leaves them (505's zombies took the flame rounds
-- a stood-and-fought Tyrant then lacked). A boss the bot kites (a
-- knowledge <kite>) keeps none: any gun's rounds do for it from the run,
-- and the ordinary gun the grenades spare here is the one it then has.
-- 513's Tyrant, kited with the handgun alone, took 1700 ticks to the
-- launcher's 900 and hurt the bot in four of twenty fights, none lost;
-- 509's four naked zombies, taken on with the handgun to keep three
-- grenades for it, killed the bot in six of forty-eight runs.
local function stage_fight_rounds(bot, k, base)
    local stage = string.sub(bot:room().id, 1, 1)
    local total = 0
    for _, fight in ipairs(bot:fight_facts()) do
        local entry = k.enemies[fight.enemy]
        local here = (fight.stage or string.sub(fight.room, 1, 1)) == stage
        if here and entry and (entry.prefer == base or entry.open == base)
            and not entry.kite
            and not (fight.bank and bot:flag(fight.bank, fight.bit)) then
            local band = entry.keep and entry.keep[base]
            local damage = band and band.damage
            local kill = damage and damage > 0 and entry.health
                and entry.health // damage + 1
            total = total + (entry.save or kill or 0)
        end
    end
    return total
end

-- Rounds of weapon `base` (every loading of it: knowledge variant_of)
-- kept from the monster `enemy_id`: the met <fight>s still to be won that
-- prefer or open with it, plus the general reserve for a reserve gun. A
-- fight's own enemy spends them all (rationing 40C's Plant 42 for the later
-- bosses killed the bot there), so it gives 0. Bosses not met yet keep
-- nothing beyond the general reserve: the bot does not know them.
function BotTactics.saved_rounds(bot, k, base, enemy_id, swamped)
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
    if row and row.reserve and not swamped then
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
local function firearm(bot, k, enemy, crowd, trap)
    local weapon = bot:weapon()
    -- A launcher whose own loading is spent counts its other loadings'
    -- rounds: the menu reload loads them (BotInventory.load_now). 509's
    -- launcher fired its last flame round, then ran past three zombies
    -- holding 18 explosive rounds, "no firearm with rounds".
    local function family(item, own)
        local row = k.weapons[item]
        if own > 0 or not row or row.melee then
            return own
        end
        return family_rounds(bot, k, row.variant_of or item)
    end
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
        elseif family(weapon.equipped, weapon.clip + weapon.reserve) > 0 then
            guns[1] = { item = weapon.equipped,
                rounds = family(weapon.equipped, weapon.clip + weapon.reserve),
                equipped = true, reserve = k.weapons[weapon.equipped].reserve == true,
                base = k.weapons[weapon.equipped].variant_of }
        end
    end
    for _, row in ipairs(bot:menu().items or {}) do
        if row.kind == "weapon" and k.weapons[row.item] and row.item ~= weapon.equipped then
            local rounds = family(row.item, held_rounds(bot, row))
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
    local died = BotNav.died_in(bot, bot:room().id) > 0
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
                -- (A monster's health reads 0 on the tick the room is
                -- entered: the knowledge's then.)
                local health = e.health > 0 and e.health or (entry.health or 0)
                shots = shots + health // damage + 1
            end
            if shots * margin <= row.rounds then
                finish = true
            end
        end
    end
    -- A crowd of TRAP_CROWD grabbers that no ordinary gun holds the rounds
    -- for: the reserve gun goes first, and only a met boss keeps its share
    -- of its rounds (no general reserve). 509's four naked zombies met a
    -- handgun holding thirty rounds (up to nine a zombie) beside a launcher
    -- holding four grenades, kept for a boss not met yet.
    local swamped = not trap and not finish and crowd ~= nil and #crowd >= TRAP_CROWD
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
                - BotTactics.saved_rounds(bot, k, base, enemy and enemy.id, swamped)
                - keep
            -- A death trap spends everything: the bosses are no use dead.
            -- The launcher's last flame round goes first, then the reload
            -- loads the explosives. Only the kills of this stage's <fight>s
            -- still to be won stay, while an ordinary gun holds the rounds
            -- for the whole crowd: 505's zombies took the flame rounds 513's
            -- Tyrant then lacked, and the handgun lost that fight four times.
            if trap then
                spare = family_rounds(bot, k, base)
                row.rounds = spare
                if finish then
                    spare = spare - stage_fight_rounds(bot, k, base)
                end
            elseif died then
                -- So does a room the bot died in (BotNav.died_in): the
                -- same kit spent the same way dies there again.
                spare = family_rounds(bot, k, base)
            end
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
        if row.reserve and crowd_n >= 2 and not trap and not died then
            local base = row.base or row.item
            if family_rounds(bot, k, base) - BotTactics.saved_rounds(bot, k, base, nil, swamped) <= 0 then
                crowd_n = 0
            end
        end
    end
    local best = BotTactics.pick_gun(guns, entry, enemy and enemy.health, margin, crowd_n,
        trap or swamped)
    if not best then
        return nil, 0, guns
    end
    return best.item, best.rounds, guns
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
    s.react = nil
    s.react_clear = nil
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
-- Whether combat would open a fight with enemy `id` (knowledge health,
-- the room's live one when `health` is given): the gun firearm() picks,
-- with the rounds the reserve leaves, has a band for it. Seed 4 took 30C's
-- optional tiger fight holding grenades the Tyrant's reserve kept back, and
-- combat ran past the tiger for 9000 ticks.
function BotTactics.armed_for(bot, id, health)
    local k = bot:knowledge()
    local entry = k and k.enemies[id]
    if not (entry and entry.keep) then
        return false
    end
    local gun = firearm(bot, k, { id = id, slot = -1, health = health or entry.health })
    return gun ~= nil and entry.keep[gun] ~= nil and not k.weapons[gun].melee
end

-- Whether the bot holds a round for any gun the knowledge gives a band
-- against enemy `id`: in the gun, as its ammunition, or as another
-- loading's rounds of it (BotInventory.load_now loads those into an empty
-- one). The reserve counts: this asks whether a fight is possible at all.
function BotTactics.any_round_for(bot, id)
    local k = bot:knowledge()
    local entry = k and k.enemies[id]
    if not (entry and entry.keep) then
        return true
    end
    for _, row in ipairs(bot:menu().items or {}) do
        local gun = row.kind == "weapon" and k.weapons[row.item]
        if gun and not gun.melee then
            if entry.keep[row.item] and held_rounds(bot, row) > 0 then
                return true
            end
            local base = gun.variant_of or row.item
            for other_id, other in pairs(k.weapons) do
                local ammo = bot:ammo_for(other_id)
                if other_id ~= row.item and (other_id == base or other.variant_of == base)
                    and entry.keep[other_id] and ammo and bot:item_count(ammo) > 0 then
                    return true
                end
            end
        end
    end
    return false
end

function BotTactics.avoiding()
    local s = state()
    return s.sourcing == true or (s.avoid_until or 0) > (s.now or 0)
end

-- Whether a spit dodge holds the pad.
function BotTactics.dodging()
    return state().spit ~= nil
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
-- A hit on the fight's target at tick `now`, for a monster with a known
-- hit reaction (a knowledge <react>): when the reaction ends, and whether
-- its sidestep follows (its `hits`th hit inside `window` ticks of the first,
-- counted as the reaction ends: enemy/06 behavior_dispatch and act_swipe).
function BotTactics.note_hit(s, entry, now, hit_state)
    local react = entry and entry.react
    if not react then
        return
    end
    local r = s.react
    if not r or (react.window and now - r.first >= react.window) then
        r = { first = now, hits = 0 }
        s.react = r
    end
    r.hits = r.hits + 1
    r.clear = now + react.stun
    -- The reaction the stun is known for shows this hit state to its end.
    s.react_state = hit_state
    s.react_stun = r.clear
    if react.hits and r.hits >= react.hits and r.clear - r.first < react.window then
        r.clear = r.clear + react.evade
        s.react = nil
    end
    s.react_clear = r.clear
end

-- Whether to press fire now at a target in a known hit reaction: true when
-- the shot lands as the reaction (and the sidestep after it) ends, false
-- when it would land inside it, nil when the bot does not know (no <react>,
-- no `lands` for the weapon, or no reaction running). The hunter swipes
-- the tick after its reaction and the claw lands six ticks on: a shot
-- pressed only once the reaction is over lands with the claw. Only while
-- the monster shows the hit state it took the hit with (`hit_state`, its
-- now): a zombie that falls instead of staggering changes it within a tick
-- or two (its poise spent: 0 at once; knocked down from a crawl: 1 for
-- 56 ticks), and a shot timed to the stagger's end met the floor, 311 shots
-- of 2904 in the zombie rooms.
function BotTactics.fire_early(s, wrow, now, hit_state)
    local clear = s.react_clear
    if not clear or not (wrow and wrow.lands) or now > clear + 1 then
        return nil
    end
    if now < s.react_stun and hit_state ~= s.react_state then
        s.react_clear = nil
        return nil
    end
    -- The reaction's last tick still shows no aim bit: land two past it.
    return now + wrow.lands >= clear + 2
end

local function by_hand(k, entry)
    local weapon = entry and entry.prefer and k.weapons[entry.prefer]
    return weapon ~= nil and weapon.melee == true
end

-- Decide every monster near the bot; returns the fight target or nil.
-- Squared distance from (px, pz) to the segment (ax, az)-(bx, bz), in floats.
local function segment_dist2(px, pz, ax, az, bx, bz)
    local dx, dz = (bx - ax) * 1.0, (bz - az) * 1.0
    local len2 = dx * dx + dz * dz
    local t = 0
    if len2 > 0 then
        t = ((px - ax) * dx + (pz - az) * dz) / len2
        t = t < 0 and 0 or (t > 1 and 1 or t)
    end
    local ex, ez = px - (ax + t * dx), pz - (az + t * dz)
    return ex * ex + ez * ez
end

-- The death trap in this room: the live slow grabbers (knowledge grabs and
-- outrun, the zombies: their bite stuns and the others queue for theirs),
-- not a monster of parts, when one lies on the walk to the planner's goal
-- and TRAP_CROWD or more lie near enough to gather on it. Once seen, the room stays
-- a trap until its last one is down (they all come). Returns the list or nil.
local TRAP_RECHECK = C.SECOND
local TRAP_CONTACT = 2
local TRAP_GATHER = 4
function BotTactics.trap(bot, enemies)
    local s = state()
    local k = bot:knowledge()
    local room = bot:room().id
    local list = {}
    for _, e in ipairs(enemies or bot:enemies()) do
        local entry = k and k.enemies[e.id]
        if entry and entry.grabs and entry.outrun == true and not entry.parts
            and entry.policy == "fight" and not by_hand(k, entry) and (e.health or 0) >= 0 then
            list[#list + 1] = e
        end
    end
    if s.trap_room == room then
        return #list > 0 and list or nil
    end
    if #list < TRAP_CROWD then
        return nil
    end
    local goal = BOT_STATE.plan and BOT_STATE.plan.goal
    local zone = goal and goal.job and goal.job.zone
    if not zone then
        return nil
    end
    -- Where the walk ends: the job's approach point, else the zone's centre
    -- (often inside the desk or wall it is pressed from, with no path).
    local gx, gz = BotNav.centre(zone)
    local point = goal.job.points and goal.job.points[goal.job.index or 1]
    if point then
        gx, gz = point[1], point[2]
    end
    local now = bot:tick()
    local cache = s.trap_path
    if not cache or cache.room ~= room or cache.gx ~= gx or cache.gz ~= gz
        or now - cache.tick >= TRAP_RECHECK then
        local player = bot:player()
        local points = { { x = player.x, z = player.z } }
        local found = bot:path(gx, gz)
        if found.status == "ok" then
            for _, point in ipairs(found.points) do
                points[#points + 1] = point
            end
        end
        points[#points + 1] = { x = gx, z = gz }
        cache = { room = room, gx = gx, gz = gz, tick = now, points = points }
        s.trap_path = cache
    end
    -- One on the walk (inside TRAP_CONTACT reaches of it) bites first; the
    -- rest gather while its bite holds the bot, from up to TRAP_GATHER
    -- reaches away. 509's walk passed one zombie at 400 and drew two more
    -- from 3400 and 4900 while the first bit.
    local contact, on_path = false, 0
    for _, e in ipairs(list) do
        local reach = (k.enemies[e.id].reach or C.REACH) * 1.0
        local pts = cache.points
        local best = nil
        for i = 1, #pts - 1 do
            local d2 = segment_dist2(e.x, e.z, pts[i].x, pts[i].z, pts[i + 1].x, pts[i + 1].z)
            if not best or d2 < best then
                best = d2
            end
        end
        if best and best <= (TRAP_CONTACT * reach) ^ 2 then
            contact = true
        end
        if best and best <= (TRAP_GATHER * reach) ^ 2 then
            on_path = on_path + 1
        end
    end
    if not contact or on_path < TRAP_CROWD then
        return nil
    end
    s.trap_room = room
    bot:log(string.format("death trap: %d zombies on the way to %s (%d in the room): kill them",
        on_path, goal.label, #list))
    return list
end

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
    -- A death trap: TRAP_CROWD or more live grabbers anywhere in the room
    -- (they all come). The crowd is all of them, and the reserve gives way.
    local trap = BotTactics.trap(bot, enemies)
    if trap then
        crowd = trap
    end
    local gun, rounds, guns = firearm(bot, k, nearest, crowd, trap ~= nil)
    -- A trap's crowd: the gun that downs all of it, ranked an ordinary gun
    -- that kills fast (TRAP_FAST shots a zombie: the shotgun), then the
    -- reserve gun (the launcher: the bosses' rounds go only where nothing
    -- else clears the crowd fast; spent on every crowd, 303's Tyrant met
    -- an empty launcher four times), then the rest by fewest shots; else
    -- whatever the guns cover between them.
    local trap_covered = nil
    if trap and gun then
        local share, best_row, best_tier, best_need = 0, nil, nil, nil
        for _, row in ipairs(guns or {}) do
            local need = 0
            for _, e in ipairs(trap) do
                need = need + rounds_for(e, k.enemies[e.id], row.item)
            end
            if need > 0 then
                share = share + row.rounds / need
                if need <= row.rounds then
                    local tier = row.reserve and 1 or (need <= TRAP_FAST * #trap and 0 or 2)
                    if not best_tier or tier < best_tier
                            or (tier == best_tier and need < best_need) then
                        best_row, best_tier, best_need = row, tier, need
                    end
                end
            end
        end
        -- No gun downs the whole crowd: the one that downs the nearest in
        -- the fewest shots, while it has the rounds (three grenades and
        -- thirty handgun rounds met 509's four zombies: the handgun took
        -- them all on, the grenades stayed held, and the bot died).
        if not best_row then
            for _, row in ipairs(guns or {}) do
                local need = rounds_for(nearest, nentry, row.item)
                if need > 0 and need <= row.rounds and (not best_need or need < best_need) then
                    best_row, best_need = row, need
                end
            end
        end
        if best_row then
            gun, rounds = best_row.item, best_row.rounds
        end
        trap_covered = share >= 1
    end
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
    for _, enemy in ipairs(trap or fightable) do
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
                crowd_needed = (trap or #fightable > 1) and crowd_needed or nil,
                trap = trap and #trap or nil,
                trap_covered = trap_covered,
                health = player.health,
                max_health = player.max_health,
                risk = risk(bot),
                crossing = cross,
                sourcing = s.sourcing,
                distance = enemy.distance,
                reach = entry and entry.reach,
                engage = far,
                outrun = entry and entry.outrun,
                slow = entry ~= nil and entry.speed ~= nil and entry.speed < C.SPEED.back,
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
                    or enemy.distance <= 2 * (entry.reach or C.REACH)
                    or why == TRAP_FIGHT) then
                local focused = best and best.slot == s.focus
                if enemy.slot == s.focus or (not focused
                        and (not best or enemy.distance < best.distance)) then
                    best = enemy
                    best.why = why
                    best.must = why == MUST_FIGHT or why == SLOW_FIGHT or why == HURT_FIGHT
                        or why == TRAP_FIGHT
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

-- A spitter (a knowledge <spit>: 30C's Black Tiger) is dodged before it
-- spits. Its globs fly near twice the run speed and hit within glob_hit on
-- every tick of their flight (a hit from 3300 within 5 ticks), so a glob in the
-- air is too late to dodge. But they fly along the spitter's facing (one
-- glob straight, two at fan / 2 or fan off it), and the tiger locks its
-- facing from the bite's tell sub-state on, about 19 ticks before it spits:
-- the glob lines are known then. So while the tiger can spit (locked, mid
-- spit, or picking and lined up) and the bot stands within SPIT_SAFE of a
-- line, it runs to the quickest spot clear of every line (and of the bite),
-- then holds until the volley has passed. A dodged glob flies on along its
-- line, which is how the tiger's own spit wears 30C's web down.
-- A line closer than SPIT_SAFE starts a dodge; a spot is clear past
-- SPIT_CLEAR. The locked facing is exact, so the margins only cover a
-- glob's spread of particles.
local SPIT_SAFE = C.APPROACH // 2
local SPIT_CLEAR = C.APPROACH
-- (Both on top of the spitter's glob_hit; a spot also keeps out of its bite,
-- bite_keep.)
-- Escape spots are tried every C.APPROACH out to SPIT_RINGS of them, on 16
-- headings.
local SPIT_RINGS = 14
-- A walk, a strafe or a pick spits with no tell: the bot waits for the
-- spit itself while its escape takes this many ticks less than the globs'
-- flight, and moves at once otherwise.
local SPIT_SLACK = 4
-- The hold ends once the spit is this old and no glob is within
-- SPIT_NEAR of the bot (they fly glob_speed a tick), or once the tiger is back in
-- a bite's early sub-states with no spit thrown.
local SPIT_PASS = C.SECOND // 4
local SPIT_NEAR = 10 * C.APPROACH
local SPIT_TIMEOUT = 3 * C.SECOND

-- sin of an angle in 4096ths of a turn (the bot VM has no math library).
local function sin_turn(units)
    local half = C.FULL_TURN // 2
    local a = units % C.FULL_TURN
    if a >= half then
        a = a - C.FULL_TURN
    end
    local x = a * 6.2831853 / C.FULL_TURN
    local quarter = 1.5707963
    if x > quarter then
        x = 3.1415927 - x
    elseif x < -quarter then
        x = -3.1415927 - x
    end
    local x2 = x * x
    return x * (1 - x2 / 6 * (1 - x2 / 20 * (1 - x2 / 42 * (1 - x2 / 72))))
end

-- The unit heading of engine angle `units`: (cos, -sin).
local function heading(units)
    return sin_turn(units + C.FULL_TURN // 4), -sin_turn(units)
end

-- The nearest of 64 headings to direction (dx, dz), or `angle` for none.
local function spit_units(dx, dz, angle)
    if dx == 0 and dz == 0 then
        return angle % C.FULL_TURN
    end
    local best, best_dot = 0, nil
    for j = 0, 63 do
        local a = j * (C.FULL_TURN // 64)
        local hx, hz = heading(a)
        local dot = hx * dx + hz * dz
        if not best_dot or dot > best_dot then
            best, best_dot = a, dot
        end
    end
    return best
end

-- The glob lines of spitter `e` (its knowledge `entry`): {ox, oz, ux, uz}.
-- With `wide`, only the wide fan's three (the narrow fan, one volley in
-- four, is a risk taken when no spot clears both).
local function spit_lines(e, entry, wide)
    local lines = {}
    local half = entry.fan // 2
    local offs = { 0, half, -half, entry.fan, -entry.fan }
    if wide then
        offs = { 0, entry.fan, -entry.fan }
    end
    -- They fan out from glob_apex ahead on its facing.
    local fx, fz = heading(e.angle)
    local ox, oz = e.x + fx * entry.glob_apex, e.z + fz * entry.glob_apex
    for _, off in ipairs(offs) do
        local ux, uz = heading(e.angle + off)
        lines[#lines + 1] = { ox = ox, oz = oz, ux = ux, uz = uz, hit = entry.glob_hit }
    end
    return lines
end

-- The least distance from (x, z) to a glob line ahead of its origin,
-- past the glob's hit.
local function line_clearance(lines, x, z)
    local best = nil
    for _, l in ipairs(lines) do
        local dx, dz = x - l.ox, z - l.oz
        local along = dx * l.ux + dz * l.uz
        if along > -C.APPROACH then
            local perp = dx * l.uz - dz * l.ux
            if perp < 0 then
                perp = -perp
            end
            perp = perp - l.hit
            if not best or perp < best then
                best = perp
            end
        end
    end
    return best or C.FULL_TURN * C.FULL_TURN
end

-- Whether spitter `e`'s lines are live: every state but a bite's early
-- sub-states (before the tell, the facing still follows the bot and no
-- spit can come for about 20 ticks). A walk or a strafe spits the moment
-- the bot is lined up, with no tell, so their lines count at once. A
-- bite's end and an idle (behaviour 0) spit through the spitter's picker,
-- which spits only inside spit_range: out of it their lines are not live.
-- (A walk's are: it spits the moment it comes in range with the bot lined
-- up, so the bot keeps off them before.)
local function spit_live(e, entry)
    if (e.action_behavior == entry.tell_behavior or e.action_behavior == 0)
        and e.distance > entry.spit_range then
        return false
    end
    if e.action_behavior == entry.tell_behavior then
        return e.action_state >= entry.tell_state
    end
    return true
end

-- Whether spitter `e` bites (a bite cycle) with the bot inside its reach.
local function spit_biting(e, entry)
    return e.action_behavior == entry.tell_behavior and e.distance < entry.bite_keep + C.APPROACH
end

-- When spitter `e`'s next volley leaves: ticks from now until its globs
-- stand at glob_spawn (negative once they fly). The tell's lock leads the
-- spit by SPIT_LEAD (19 measured in 30C, one less for margin), the globs
-- show the tick after the spit starts, and a walk, strafe or pick may spit
-- now. A bite before its tell spits no sooner than SPIT_LEAD. Each phase is
-- timed from its first tick seen, so spit_dodge calls this every tick.
local SPIT_LEAD = 18
local function spit_due(s, e, entry, now)
    s.spit_phase = s.spit_phase or {}
    local phase = "walk"
    if e.action_behavior == entry.spit_behavior then
        phase = "spit"
    elseif e.action_behavior == entry.tell_behavior then
        phase = e.action_state >= entry.tell_state and "tell" or "bite"
    end
    local seen = s.spit_phase[e.slot]
    if not seen or seen.phase ~= phase then
        seen = { phase = phase, tick = now }
        s.spit_phase[e.slot] = seen
    end
    if phase == "spit" then
        return seen.tick + 1 - now
    elseif phase == "tell" then
        return seen.tick + SPIT_LEAD - now
    elseif phase == "bite" then
        return SPIT_LEAD
    end
    return 0
end

-- How far a run from the player to (x, z), `r` away on heading `units`,
-- keeps out of the globs of `lines` thrown `due` ticks from now: the least
-- distance past glob_hit + SPIT_CLEAR, negative for a hit. The bot turns in
-- place down to SPIT_DASH_TURN off, runs at the run speed and stands at the
-- spot; each glob leaves its line glob_spawn - glob_apex along it and flies
-- glob_speed a tick. A line crossed before its glob comes, or after it
-- passed, costs nothing: the static clearance (line_clearance) sent seeds
-- to spots their turn and run could not reach before the volley, and back
-- across lines a walk swept.
local SPIT_DASH_TURN = C.FULL_TURN // 8
local function spit_margin(player, units, r, x, z, e, entry, lines, due)
    local err = abs(angle_delta(units, player.angle % C.FULL_TURN))
    local t0 = 0
    if r > 0 and err > SPIT_DASH_TURN then
        t0 = (err - SPIT_DASH_TURN) / C.TURN_STEP
    end
    local run = r / C.SPEED.run
    local fx, fz = x - e.x, z - e.z
    local far = e.distance
    if fx * fx + fz * fz > far * far then
        far = (fx * fx + fz * fz) ^ 0.5
    end
    local keep = entry.glob_hit + SPIT_CLEAR
    local horizon = due + (far + keep - entry.glob_apex) / entry.glob_speed + 1
    local t = due > 0 and due or 0
    local least = nil
    while t <= horizon do
        local f = 0
        if run > 0 and t > t0 then
            f = (t - t0) / run
            if f > 1 then
                f = 1
            end
        end
        local px, pz = player.x + (x - player.x) * f, player.z + (z - player.z) * f
        local along = entry.glob_spawn - entry.glob_apex + entry.glob_speed * (t - due)
        for _, l in ipairs(lines) do
            local gx, gz = l.ox + l.ux * along - px, l.oz + l.uz * along - pz
            local d2 = gx * gx + gz * gz
            if not least or d2 < least then
                least = d2
            end
        end
        t = t + 1
    end
    if not least then
        return keep
    end
    return least ^ 0.5 - keep
end

-- The quickest reachable spot clear of `lines` and of the bite of `e`.
-- With `due` (spit_due), clear means out of the globs' way in time
-- (spit_margin); without it, past SPIT_CLEAR of every line. With `due` and
-- no clear spot, the reachable one (or the bot's own) that keeps furthest
-- from the globs, flagged `risky`: a fallback to the static clearance's
-- spots ran seed 1 of v3 down the walk-spit's line.
-- Out of spit_range, a spot the run cannot go on SPIT_ROOM past ranks
-- SPIT_CORNER ticks dearer: the tiger cannot spit yet, but its walk turns
-- after the bot, and the v4 start ran ahead of it into 30C's north-east
-- corner, where the walk's spit found it with no way out.
local SPIT_ROOM = 4 * C.APPROACH
local SPIT_CORNER = C.SECOND // 2
local function spit_escape(bot, player, e, entry, lines, due)
    local best, best_cost
    local fallback = {}
    for j = 0, 15 do
        local units = j * (C.FULL_TURN // 16)
        local ux, uz = heading(units)
        local turn = abs(angle_delta(units, player.angle % C.FULL_TURN)) // C.TURN_STEP
        for ring = 2, SPIT_RINGS do
            local r = ring * C.APPROACH
            local x, z = (player.x + ux * r) // 1, (player.z + uz * r) // 1
            -- The run there keeps out of the bite too (from inside it, it
            -- at least does not close on the tiger).
            local reach = entry.bite_keep
            if e.distance < reach then
                reach = e.distance
            end
            local fx, fz = x - e.x, z - e.z
            local bite_ok = fx * fx + fz * fz >= (entry.bite_keep + C.APPROACH) ^ 2
                and segment_dist2(e.x, e.z, player.x, player.z, x, z) >= reach * reach * 1.0
            local clear
            if due then
                local margin = spit_margin(player, units, r, x, z, e, entry, lines, due)
                clear = margin >= 0
                if bite_ok and not clear then
                    fallback[#fallback + 1] = { x = x, z = z, r = r, units = units,
                        margin = margin, cost = turn + r / C.SPEED.run }
                end
            else
                clear = line_clearance(lines, x, z) >= SPIT_CLEAR
            end
            if clear and bite_ok then
                local cost = turn + r / C.SPEED.run
                if not best_cost or cost < best_cost then
                    local path = bot:path(x, z)
                    if path.status == "ok" and path.length <= r * 1.3 then
                        local rank = cost
                        if e.distance > entry.spit_range then
                            local room = SPIT_ROOM + r
                            local on = bot:path((player.x + ux * room) // 1, (player.z + uz * room) // 1)
                            if on.status ~= "ok" or on.length > room * 1.3 then
                                rank = cost + SPIT_CORNER
                            end
                        end
                        if not best_cost or rank < best_cost then
                            best, best_cost = { x = x, z = z, cost = cost, units = units }, rank
                        end
                    end
                end
                break
            end
        end
    end
    if best or not due then
        return best
    end
    local stay = spit_margin(player, player.angle, 0, player.x, player.z, e, entry, lines, due)
    table.sort(fallback, function(p, q)
        return p.margin > q.margin
    end)
    for _, c in ipairs(fallback) do
        if c.margin <= stay then
            break
        end
        local path = bot:path(c.x, c.z)
        if path.status == "ok" and path.length <= c.r * 1.3 then
            return { x = c.x, z = c.z, cost = c.cost, units = c.units, risky = true }
        end
    end
    return { x = player.x, z = player.z, cost = 0, units = player.angle, risky = true }
end

-- Whether a spitter here may lock onto the bot before a swing or a shot
-- that roots it ends. A knife swing and its recovery hold the bot about 25
-- ticks, the bite's wait and turn take about 23 before the lock, and the
-- escape needs the 19 the lock leaves: a swing is safe only in the first
-- SPIT_SWING ticks of a spit (the spit runs about 40, then the bite's wait),
-- or while a walk or a pick has the bot clear of its lines.
local SPIT_SWING = 2 * C.SECOND // 3
function BotTactics.spit_holds_fire(bot, player)
    local k = bot:knowledge()
    local s = state()
    s.spit_start = s.spit_start or {}
    for _, e in ipairs(bot:enemies()) do
        local entry = k.enemies[e.id]
        if entry and entry.spit_behavior and (e.health or 0) >= 0 then
            if e.action_behavior == entry.spit_behavior then
                s.spit_start[e.slot] = s.spit_start[e.slot] or bot:tick()
                if bot:tick() - s.spit_start[e.slot] > SPIT_SWING then
                    return true
                end
            else
                s.spit_start[e.slot] = nil
                if e.action_behavior == entry.tell_behavior then
                    return true
                end
                if line_clearance(spit_lines(e, entry), player.x, player.z) < SPIT_CLEAR then
                    return true
                end
            end
        end
    end
    return false
end

-- Run straight for (x, z) on the pad: an in-place turn (twice the run-turn's
-- rate) while the heading is more than DASH_TURN off, else the run with the
-- turn held. walk_to's run latch waits out heading wobble, and the dodge's
-- 19 ticks cannot.
local DASH_TURN = C.FULL_TURN // 8
local DASH_BAND = C.FULL_TURN // 64
-- A target near straight behind keeps the turn's way: its error's sign
-- flips tick to tick there, and an alternating turn stands still.
local DASH_BEHIND = C.FULL_TURN * 3 // 8
local dash_way = 1
local function dash(bot, x, z)
    local f = bot:face_to(x, z)
    local err = f.angle_error or 0
    if abs(err) > DASH_BEHIND then
        err = abs(err) * dash_way
    elseif err ~= 0 then
        dash_way = err > 0 and 1 or -1
    end
    local go = abs(err) < DASH_TURN
    -- A held aim (a knife waiting to swing) roots the bot: let go of it.
    bot:set_input({ up = go, run = go, right = err > DASH_BAND, left = err < -DASH_BAND,
        aim = false, fire = false })
end

local function spit_dodge(bot, s, player)
    local k = bot:knowledge()
    local now = bot:tick()
    local d = s.spit
    if d then
        local e = enemy_by_slot(bot:enemies(), d.slot)
        local entry = e and k.enemies[e.id]
        if e and e.action_behavior == entry.spit_behavior and not d.spat then
            d.spat = now
        end
        local live = e and (spit_live(e, entry) or spit_biting(e, entry))
        local near = false
        for _, g in ipairs(bot:projectiles()) do
            if bot:distance(g.x, g.z) < SPIT_NEAR then
                near = true
            end
        end
        local flight = e and (e.distance - entry.glob_spawn) // entry.glob_speed or 0
        -- A walk, an idle or a strafe spits only as the bot comes in line,
        -- along its facing: once the bot stands clear of its lines, the hold
        -- is over (held on, the dodges chased a walk's lines into corners).
        local told = e and (e.action_behavior == entry.tell_behavior
            or e.action_behavior == entry.spit_behavior)
        local walk_clear = told == false and not d.spat and not near
            and line_clearance(spit_lines(e, entry, d.wide), player.x, player.z) >= SPIT_CLEAR
        local over = not e or (d.spat and now - d.spat >= SPIT_PASS + flight and not near)
            or (not d.spat and not live) or now - d.tick >= SPIT_TIMEOUT or walk_clear
        if over then
            bot:log(string.format("spit dodge over (%s) at (%d,%d) health=%d",
                d.spat and "the volley passed" or "no spit came", player.x, player.z, player.health))
            s.spit = nil
            s.aim_ticks = 0
            return false
        end
        -- The tiger turned (a walk keeps turning), or the run there is too
        -- slow for the volley: the spot is planned again.
        local lines = spit_lines(e, entry, d.wide)
        local due = spit_due(s, e, entry, now)
        local r = bot:distance(d.x, d.z)
        local units = spit_units(d.x - player.x, d.z - player.z, player.angle)
        -- Told or spat, the timing is known: the run is checked against
        -- it. A walk's lines sweep with its turn and may spit any tick: only
        -- a spot on them moves (planning against a spit now each tick ran
        -- v3's seed 1 down the corridor ahead of the walk).
        local slow = told and not d.risky
            and spit_margin(player, units, r, d.x, d.z, e, entry, lines, due) < 0
        if line_clearance(lines, d.x, d.z) < SPIT_SAFE or slow then
            local spot = spit_escape(bot, player, e, entry, lines, due)
            d.x, d.z, d.risky = spot.x, spot.z, spot.risky
        elseif told and d.risky then
            -- A risky run takes a clear spot once one shows.
            local spot = spit_escape(bot, player, e, entry, lines, due)
            if not spot.risky then
                d.x, d.z, d.risky = spot.x, spot.z, false
            end
        end
        if line_clearance(lines, player.x, player.z) < SPIT_CLEAR
            or bot:distance(d.x, d.z) > C.APPROACH then
            dash(bot, d.x, d.z)
            return true
        end
        bot:set_input({})
        return true
    end
    for _, e in ipairs(bot:enemies()) do
        local entry = k.enemies[e.id]
        local biting = entry and entry.spit_behavior and spit_biting(e, entry)
        local due = entry and entry.spit_behavior and spit_due(s, e, entry, now)
        if entry and entry.spit_behavior and (e.health or 0) >= 0
            and (biting or spit_live(e, entry)) then
            local lines = spit_lines(e, entry)
            local clear = line_clearance(lines, player.x, player.z)
            -- Inside the bite while it bites: out first, lines or not.
            if biting then
                clear = 0
            end
            if clear < SPIT_SAFE then
                -- Out of the globs' way in time, every line, then the wide
                -- fan's; else the static clearance's spots.
                local spot = spit_escape(bot, player, e, entry, lines, due)
                local wide = false
                if spot.risky then
                    local other = spit_escape(bot, player, e, entry, spit_lines(e, entry, true), due)
                    if not other.risky then
                        spot, wide = other, true
                    end
                end
                -- No tell (a walk, strafe or pick): the spit itself is the
                -- signal while the escape beats the globs' flight.
                local told = e.action_behavior == entry.tell_behavior
                    or e.action_behavior == entry.spit_behavior
                local flight = (e.distance - entry.glob_spawn) / entry.glob_speed
                -- (With no spot clear, too: the walk may not spit at all.)
                if not told and (spot.risky or spot.cost + SPIT_SLACK < flight) then
                    spot = nil
                    clear = nil
                end
                if spot then
                    s.spit = { slot = e.slot, tick = now, x = spot.x, z = spot.z, cost = spot.cost,
                        wide = wide, risky = spot.risky }
                    bot:log(string.format(
                        "spit dodge: slot=%d ab=%d as=%d dist=%d clear=%d: to (%d,%d) cost %.0f%s%s",
                        e.slot, e.action_behavior, e.action_state, e.distance, clear // 1,
                        spot.x, spot.z, spot.cost, wide and " (wide fan only)" or "",
                        spot.risky and " (no spot clears the globs)" or ""))
                    return spit_dodge(bot, s, player)
                end
                if clear and s.spit_stuck ~= now - 1 then
                    bot:log(string.format("spit dodge: slot=%d dist=%d: no clear spot", e.slot, e.distance))
                end
                if clear then
                    s.spit_stuck = now
                end
            end
        end
    end
    return false
end

-- Kiting (a knowledge <kite>, a room <kite> loop: 303's Tyrant). RANGE
-- sets the probes and margins: the room probe steps, the wall margin and
-- the run's look-ahead.
local RANGE = C.FIGHT_RANGE
local KITE_DASH = C.WARP_DISTANCE * 2
local KITE_TURN_WAIT = 2 * C.SECOND // 3
local KITE_REACH = C.REACH + C.APPROACH // 2
local KITE_FETCH_TICKS = 3 * C.SECOND
-- How long a kite runs on with no gun and nothing to fetch before it is lost.
local KITE_LOST = 20 * C.SECOND
local KITE_TURN_TICKS = 8
local KITE_STEER_MAX = C.FULL_TURN // 12
local KITE_QUICK_FROM = C.FULL_TURN * 5 // 16
local KITE_FLANK_TURN = C.FULL_TURN // 6
local KITE_FLANK_OFF = C.FULL_TURN // 14
-- The tyrant's stand after a rush's or a charge's swing (its repause).
local KITE_RECOVERY = C.SECOND // 2
-- The handgun's cadence: a stop's distances are set for it.
local KITE_FAST_CADENCE = 23
local KITE_LATE_SCALE = 3
local function manhattan(ax, az, bx, bz)
    return abs(ax - bx) + abs(az - bz)
end

local function norm(x, z)
    local len = (x * x + z * z) ^ 0.5
    if len < 1 then
        return 0, 0
    end
    return x / len, z / len
end

-- The engine angle of unit vector (ux, uz): the nearest of 64 headings.
local function angle_of(ux, uz)
    local best, best_dot = 0, -2
    for j = 0, 63 do
        local a = j * (C.FULL_TURN // 64)
        local hx, hz = heading(a)
        local dot = hx * ux + hz * uz
        if dot > best_dot then
            best, best_dot = a, dot
        end
    end
    return best
end

-- The straight run along unit (ux, uz) the room leaves, probed at steps.
local function kite_room(bot, player, ux, uz)
    local free = 0
    for _, r in ipairs({ RANGE // 2, RANGE, RANGE * 3 // 2 }) do
        local path = bot:path((player.x + ux * r) // 1, (player.z + uz * r) // 1)
        if path.status ~= "ok" or path.length > r * 1.15 then
            break
        end
        free = r
    end
    return free
end

-- A run across `e`'s facing line, to the side the bot is on.
local function kite_cross(bot, player, e)
    local fx, fz = heading(e.angle)
    local px, pz = player.x - e.x, player.z - e.z
    local side = fx * pz - fz * px
    local sign = side >= 0 and 1 or -1
    for _, sg in ipairs({ sign, -sign }) do
        local ux, uz = norm(-fz * sg + fx * 0.3, fx * sg + fz * 0.3)
        if kite_room(bot, player, ux, uz) >= RANGE // 2 then
            return ux, uz
        end
    end
    return nil
end

-- A ring loop (two or more room <kite> facts for one enemy: the corners of
-- a circuit of corridors around a block, 513's): the circuit's segments
-- with their start along it, and its length.
local function ring_build(corners)
    local ring = { segs = {}, length = 0 }
    for i, a in ipairs(corners) do
        local b = corners[i % #corners + 1]
        local len = ((b.x - a.x) ^ 2 + (b.z - a.z) ^ 2) ^ 0.5
        ring.segs[#ring.segs + 1] = { ax = a.x, az = a.z, bx = b.x, bz = b.z, len = len,
            at = ring.length, width = a.radius }
        ring.length = ring.length + len
    end
    return ring
end

-- The nearest point of the circuit to (x, z): how far along it, and how
-- far off it compared with the corridor's half width there.
local function ring_project(ring, x, z)
    local best_at, best_off, best_seg
    for _, g in ipairs(ring.segs) do
        local t = ((x - g.ax) * (g.bx - g.ax) + (z - g.az) * (g.bz - g.az)) / (g.len * g.len)
        t = t < 0 and 0 or (t > 1 and 1 or t)
        local px, pz = g.ax + (g.bx - g.ax) * t, g.az + (g.bz - g.az) * t
        local off = ((x - px) ^ 2 + (z - pz) ^ 2) ^ 0.5
        if not best_off or off < best_off then
            best_at, best_off, best_seg = g.at + g.len * t, off, g
        end
    end
    return best_at, best_off, best_seg
end

local function ring_point(ring, at)
    at = at % ring.length
    for _, g in ipairs(ring.segs) do
        if at <= g.at + g.len then
            local t = (at - g.at) / g.len
            return g.ax + (g.bx - g.ax) * t, g.az + (g.bz - g.az) * t
        end
    end
    local g = ring.segs[1]
    return g.ax, g.az
end

-- The run on a ring: along the circuit to a point RANGE * 2 / 3 ahead,
-- the way that leaves `e` behind (the longer way round to it). The way is
-- kept until the other is clearly longer, so a tyrant across the block
-- does not flip it each tick; off the circuit (a pocket) the run follows
-- the walk's path to a point farther on.
local KITE_RING_SWITCH = C.FIGHT_RANGE
local KITE_RING_BESIDE = C.FIGHT_RANGE // 3
local function ring_heading(bot, player, e, kk, ring)
    local sp, off, seg = ring_project(ring, player.x, player.z)
    local se = ring_project(ring, e.x, e.z)
    local ahead = (sp - se) % ring.length
    local behind = ring.length - ahead
    -- `ahead`: how far the bot is ahead of it along the circuit's order;
    -- `behind`: the other way round. The run goes the way it is ahead. One
    -- beside it (the corridor's width off, 513's entry) passes it either
    -- way, so the run keeps the way it faces: a turn stands still.
    local way = kk.ring_way
    if not way then
        local fx, fz = heading(player.angle)
        way = (fx * (seg.bx - seg.ax) + fz * (seg.bz - seg.az) >= 0) and 1 or -1
    end
    local front, back = behind, ahead
    if way < 0 then
        front, back = ahead, behind
    end
    if front + KITE_RING_SWITCH < back and front > KITE_RING_BESIDE then
        way = -way
        bot:log(string.format("kite: ring way %d (it is %d ahead, %d behind)", way, front // 1, back // 1))
    end
    kk.ring_way = way
    local tx, tz = ring_point(ring, sp + way * RANGE * 2 // 3)
    -- Off the corridor (a pocket: 513's ammunition), the walk's way there:
    -- a straight line back to the middle line ran at the tyrant.
    if off > seg.width then
        tx, tz = ring_point(ring, sp + way * RANGE * 2)
        local path = bot:path(tx // 1, tz // 1)
        for _, p in ipairs(path.status == "ok" and path.points or {}) do
            if bot:distance(p.x, p.z) > C.APPROACH then
                tx, tz = p.x, p.z
                break
            end
        end
    end
    return norm(tx - player.x, tz - player.z)
end

-- The run heading: the best of 24 by the swing off straight away from `e`
-- (weighted up as it closes), how far out of the deck's loop it leads, the
-- room ahead and, close to it, any part toward it.
local function kite_run_heading(bot, player, e, kite, loop, kk)
    if loop.ring then
        return ring_heading(bot, player, e, kk, loop.ring)
    end
    local ax, az = norm(player.x - e.x, player.z - e.z)
    local away = angle_of(ax, az)
    local d = manhattan(player.x, player.z, e.x, e.z)
    local close = d < kite.lower + 2 * C.APPROACH
    local best, best_cost, bx, bz
    for j = 0, 23 do
        local a = j * C.FULL_TURN // 24
        local ux, uz = heading(a)
        local dev = abs(angle_delta(a, away))
        local qx, qz = player.x + ux * RANGE * 2 // 3 - loop.x, player.z + uz * RANGE * 2 // 3 - loop.z
        local rq = (qx * qx + qz * qz) ^ 0.5
        local cost = dev * (close and 2 or 1) + (rq > loop.radius and (rq - loop.radius) or 0)
        local toward = -(ux * ax + uz * az)
        if close and toward > 0 then
            cost = cost + toward * C.FULL_TURN
        end
        if not best_cost or cost < best_cost then
            local room = kite_room(bot, player, ux, uz)
            cost = cost + (room >= RANGE and 0 or (RANGE - room) * 2)
            if not best_cost or cost < best_cost then
                best, best_cost, bx, bz = a, cost, ux, uz
            end
        end
    end
    return bx, bz
end

-- The room's kiting loop (cached per room) and the kited enemy here.
local function kite_target(bot, s, k)
    local room = bot:room().id
    if s.kite_room_id ~= room then
        s.kite_room_id = room
        local meta = bot:room_meta()
        local kites = meta and meta.kites or {}
        s.kite_loop = kites[1] or false
        local corners = {}
        for _, f in ipairs(kites) do
            if f.enemy == kites[1].enemy then
                corners[#corners + 1] = f
            end
        end
        if #corners > 1 then
            s.kite_loop = { enemy = kites[1].enemy, ring = ring_build(corners) }
        end
    end
    local loop = s.kite_loop
    if not loop then
        return nil
    end
    local entry = k.enemies[loop.enemy]
    if not (entry and entry.kite) then
        return nil
    end
    for _, row in ipairs(bot:enemies()) do
        if row.id == loop.enemy and (row.health or 0) >= 0 then
            return row, entry, loop
        end
    end
    return nil
end

local function kite_step(bot, s, player)
    local k = bot:knowledge()
    local e, entry, loop = kite_target(bot, s, k)
    if not e or e.action_behavior == entry.kite.entrance then
        return false
    end
    local kite = entry.kite
    if not s.kite then
        s.kite = { mode = "run", aim = 0 }
    end
    local kk = s.kite
    local now = bot:tick()
    -- A scene or a menu took the ticks since the last step: a quick turn
    -- it broke off is over (513's entry scene: the bot stood out the rest
    -- of it), and a ring's way round is chosen again.
    if kk.step_tick and now - kk.step_tick > 2 then
        if kk.mode == "turn" then
            kk.mode = "run"
        end
        kk.ring_way = nil
    end
    kk.step_tick = now
    local d = manhattan(player.x, player.z, e.x, e.z)
    -- On a ring the block between keeps it off longer than the straight
    -- gap: its walk is the shorter way round (the straight gap still
    -- counts beside it, a corridor's width off).
    if loop.ring then
        local sp = ring_project(loop.ring, player.x, player.z)
        local se = ring_project(loop.ring, e.x, e.z)
        local ahead = (sp - se) % loop.ring.length
        local round = loop.ring.length - ahead
        if ahead < round then
            round = ahead
        end
        if round > d then
            d = round // 1
        end
    end
    -- In its scene state (513's pod and Wesker) a room scene drives it:
    -- its behaviours are the scene's, so the bot only runs.
    local ab = e.action_behavior
    if kite.scene and e.state == kite.scene then
        ab = -1
    end
    local to_e = angle_of(norm(e.x - player.x, e.z - player.z))
    local facing_err = abs(angle_delta(player.angle % C.FULL_TURN, to_e))
    local weapon = bot:weapon()
    -- The last hit on it (its health drops; at 200 it springs back).
    if kk.health and e.health < kk.health then
        kk.last_hit = now
        bot:log(string.format("kite: hit health %d -> %d", kk.health, e.health))
    end
    kk.health = e.health
    do
        local w = bot:weapon()
        if kk.shot_gun == w.equipped and kk.shot_clip and w.clip < kk.shot_clip then
            bot:log(string.format("kite: shot 0x%02X clip %d d=%d beh=%d", w.equipped or 0,
                w.clip, d, w.behavior or -1))
        end
        kk.shot_gun, kk.shot_clip = w.equipped, w.clip
    end

    local function set_mode(mode, why)
        if kk.mode ~= mode then
            bot:log(string.format("kite: %s (%s) d=%d", mode, why, d))
        end
        kk.mode = mode
    end
    -- A quick turn: back + a run press until the turn starts, then wait it out.
    if kk.mode == "turn" then
        local turned = abs(angle_delta(player.angle % C.FULL_TURN, kk.turn_from))
        kk.turn_ticks = kk.turn_ticks + 1
        if player.locked then
            bot:set_input({})
            return true
        end
        if turned > C.FULL_TURN // 4 or kk.turn_ticks > KITE_TURN_WAIT then
            if turned <= C.FULL_TURN // 4 then
                bot:log("kite: quick turn did not start")
            end
            set_mode(kk.after_turn, "turned")
            kk.aim = 0
            kk.aim_start = now
        else
            bot:set_input({ down = true, run = true, run_pressed = true })
            return true
        end
    end
    local function quick_turn(next_mode)
        kk.turn_from = player.angle % C.FULL_TURN
        kk.turn_ticks = 0
        kk.after_turn = next_mode
        set_mode("turn", "to " .. next_mode)
        bot:set_input({ down = true, run = true, run_pressed = true })
        return true
    end
    -- A charge locks its facing for the swing: run across its line.
    if ab == kite.charge then
        local ux, uz = kite_cross(bot, player, e)
        set_mode("cross", string.format("ab=%d as=%d", ab, e.action_state))
        if ux then
            dash(bot, player.x + ux * KITE_DASH, player.z + uz * KITE_DASH)
        end
        return true
    end
    -- A rise, then the rush: the rush homes in (a turn of 0x80 a tick) and
    -- its swing still turns (0x48 a tick), so neither is outrun or run
    -- across. Its turn is wide, though (a circle about 4000 across at the
    -- swing's speed), so a run past its flank leaves the claw short. In the
    -- rise the bot takes up a spot by its flank, faces the run's way (the
    -- side with room), and runs once the swing starts.
    -- The killing weapon already up (or rising: it roots the bot anyway)
    -- when the rise starts: the rise stands nearly still, so the shot goes.
    if ab == kite.rise and not kk.flank_run and bot:has_item(entry.prefer or -1)
        and bot:weapon().equipped == entry.prefer and bot:weapon().behavior ~= 0 then
        local w = bot:weapon()
        local face = bot:face_to(e.x, e.z)
        local err = face.angle_error or 0
        local input = BotTactics.aim_input("torso")
        input.right = err > FACE_BAND
        input.left = err < -FACE_BAND
        if w.behavior == C.HOLD_BEHAVIOR and abs(err) <= FACE_BAND * 2 then
            input.fire = true
            input.fire_pressed = true
        end
        set_mode("finish", "the rise stands still")
        bot:set_input(input)
        return true
    end
    if ab == kite.rise or ab == kite.rush then
        local eu = ((player.x - e.x) ^ 2 + (player.z - e.z) ^ 2) ^ 0.5
        local swing = ab == kite.rush and (e.action_state >= 2 or d < kite.lower - kite.hold + kite.flank)
        -- The spot: kite.flank from it, KITE_FLANK_OFF off its facing line on
        -- one side, and the run from there KITE_FLANK_TURN off the line back
        -- to it, away from its facing line on that side. A spot near the
        -- facing line was hit one swing in three; past 20 degrees off it,
        -- none in 46 (303's bench).
        local function flank_at(side)
            local fx, fz = heading(e.angle)
            local px, pz = fz, -fx
            local oc, os = heading(KITE_FLANK_OFF)
            os = -os
            local ox, oz = fx * oc + px * os * side, fz * oc + pz * os * side
            local sx, sz = e.x + ox * kite.flank, e.z + oz * kite.flank
            local rc, rs = heading(KITE_FLANK_TURN)
            rs = -rs
            for _, turn in ipairs({ 1, -1 }) do
                local hx, hz = -ox * rc + oz * rs * turn, -oz * rc - ox * rs * turn
                if (hx * px + hz * pz) * side > 0 then
                    return { sx = sx, sz = sz, x = hx, z = hz, side = side }
                end
            end
        end
        if not kk.flank then
            local best, best_cost
            for _, side in ipairs({ 1, -1 }) do
                local c = flank_at(side)
                local room = kite_room(bot, player, c.x, c.z)
                local cost = (RANGE * 2 - room) + bot:distance(c.sx, c.sz)
                if not best_cost or cost < best_cost then
                    best, best_cost = c, cost
                end
            end
            kk.flank = best
            kk.rise_tick = now
            bot:log(string.format("kite: rise: flank side %d d=%d eu=%d", best.side, d, eu // 1))
        elseif not kk.flank_run then
            kk.flank = flank_at(kk.flank.side)
        end
        local f = kk.flank
        if kk.mode ~= "flank" then
            set_mode("flank", string.format("ab=%d as=%d eu=%d", ab, e.action_state, eu // 1))
        end
        if swing or kk.flank_run then
            if not kk.flank_run then
                local fe = angle_delta(player.angle % C.FULL_TURN, angle_of(f.x, f.z))
                bot:log(string.format("kite: flank run: eu=%d facing off %d as=%d rise %d ticks",
                    eu // 1, fe, e.action_state, now - (kk.rise_tick or now)))
            end
            kk.flank_run = kk.flank_run or now
            dash(bot, player.x + f.x * KITE_DASH, player.z + f.z * KITE_DASH)
            return true
        end
        -- Take up the spot, then face the run's way.
        if bot:distance(f.sx, f.sz) > C.APPROACH * 2 then
            dash(bot, f.sx, f.sz)
            return true
        end
        local face = bot:face_to(player.x + f.x * KITE_DASH, player.z + f.z * KITE_DASH)
        local err = face.angle_error or 0
        bot:set_input({ right = err > FACE_BAND, left = err < -FACE_BAND })
        return true
    end
    if kk.flank then
        kk.flank = nil
        kk.flank_run = nil
        set_mode("run", "rush over")
    end
    if kk.mode == "cross" then
        set_mode("run", "threat over")
    end
    -- With no gun the rest leave, the rounds kept for the fights ahead
    -- (a launcher's general reserve) are spent here: 513's flame rounds
    -- stayed held while the tyrant walked the bot round its ring.
    local gun = firearm(bot, k, e) or firearm(bot, k, e, nil, true)
    -- The rocket launcher dropped: go for it while safe. With no gun
    -- left, rounds lying here for a held one (513's handgun ammunition).
    local goal
    local function ammo_here(item)
        for _, row in ipairs(bot:menu().items or {}) do
            if row.kind == "weapon" and bot:ammo_for(row.item) == item then
                return true
            end
        end
        return false
    end
    for _, g in ipairs(bot:world().frontier or {}) do
        if g.kind == "item" and g.room == bot:room().id
            and ((entry.prefer and g.item == entry.prefer and not bot:has_item(entry.prefer))
                or (not gun and ammo_here(g.item))) then
            goal = g
        end
    end
    -- Fetch it while the tyrant is well off its way: run there, and press
    -- the action key once the reach point is on it.
    local fetch_ok = false
    if goal and kk.mode ~= "aim" and kk.mode ~= "lower" and kk.mode ~= "turn" then
        -- The run there (at the run's speed; the pickup pauses the room),
        -- the tyrant walking at the bot all the while: it stays out of its
        -- reach and a step, and at the pickup out of KITE_REACH past its
        -- reach unless it is behind (the run on is the escape: no turn).
        -- Its walk straight to the launcher (the old test) is no chase:
        -- that held the fetch off 1000 to 2000 ticks, and seed 4's
        -- self-destruct timer ran out first.
        local run = bot:distance(goal.x, goal.z)
        local ticks = run / C.SPEED.run
        local gx, gz = norm(goal.x - player.x, goal.z - player.z)
        local bx, bz, tx, tz = player.x, player.z, e.x, e.z
        local reach = (entry.reach or 0) + C.APPROACH
        local gap = ((player.x - e.x) ^ 2 + (player.z - e.z) ^ 2) ^ 0.5
        -- Facing away, a quick turn's stand first.
        local fx, fz = heading(player.angle)
        local stand = (fx * gx + fz * gz < 0) and KITE_TURN_TICKS or 0
        for t = 1, (ticks // 1) + stand do
            if t > stand then
                bx, bz = bx + gx * C.SPEED.run, bz + gz * C.SPEED.run
            end
            local cx, cz = norm(bx - tx, bz - tz)
            tx, tz = tx + cx * kite.speed, tz + cz * kite.speed
            local g = ((bx - tx) ^ 2 + (bz - tz) ^ 2) ^ 0.5
            if g < gap then
                gap = g
            end
        end
        local behind = (tx - goal.x) * gx + (tz - goal.z) * gz < 0
        local last = ((goal.x - tx) ^ 2 + (goal.z - tz) ^ 2) ^ 0.5
        fetch_ok = gap > reach and (behind or last > (entry.reach or 0) + KITE_REACH)
            and (kk.mode == "fetch" or ticks < KITE_FETCH_TICKS)
    end
    if fetch_ok then
        set_mode("fetch", "launcher clear")
        local rx, rz = player.reach_x - goal.x, player.reach_z - goal.z
        if rx * rx + rz * rz < KITE_REACH ^ 2 or bot:distance(goal.x, goal.z) < KITE_REACH then
            bot:face_to(goal.x, goal.z)
            bot:set_input({ action = true, action_pressed = (now % 2 == 0) })
            return true
        end
        dash(bot, goal.x, goal.z)
        return true
    elseif kk.mode == "fetch" then
        set_mode("run", "fetch off")
    end
    -- No gun with a round, and none to fetch here: the fight is lost. The
    -- room holds the bot until it ends, so the run would go on for good
    -- (513's tyrant walked the bot round its ring for 240000 ticks, the
    -- launcher's rounds in the box). The bot stands: the death loads the
    -- last save, and the met fight then asks the box for its loadout
    -- (BotInventory.context) before the bot comes back. Not while the
    -- killing weapon is still to come (`open`: 303's rocket launcher).
    local to_come = entry.prefer and entry.open and not bot:has_item(entry.prefer)
    if not gun and not goal and not to_come then
        kk.lost = kk.lost or now
        if now - kk.lost >= KITE_LOST then
            set_mode("lost", "no gun, and none to fetch")
            bot:face_to(e.x, e.z)
            return true
        end
    else
        kk.lost = nil
    end
    -- The stop's gun: the killing weapon when held; from past the charge's
    -- edge, combat's own pick (the launcher's rounds land while the gap is
    -- wide); nearer, the quickest gun with rounds (a known cadence: the
    -- handgun), since a slow gun's recoil and pump leave no time to run
    -- (303's shotgun: 33 ticks from the shot to the turn, the gap 7000 to
    -- 2600).
    local function stop_gun()
        if gun == entry.prefer or d >= kite.edge then
            return gun
        end
        local best, best_cadence
        for _, row in ipairs(bot:menu().items or {}) do
            local w = row.kind == "weapon" and k.weapons[row.item]
            if w and not w.melee and w.cadence and entry.keep and entry.keep[row.item]
                and held_rounds(bot, row) > 0 and (not best_cadence or w.cadence < best_cadence) then
                best, best_cadence = row.item, w.cadence
            end
        end
        return best or gun
    end
    -- A gun slower than the handgun drops its aim earlier: each tick of
    -- cadence past KITE_FAST_CADENCE costs KITE_LATE_SCALE ticks the tyrant
    -- walks before the run (the recoil outlasts the cadence's difference:
    -- the launcher's 27 ticks, 1440 more). One slow enough to push that
    -- to `shoot` (the shotgun's 37, with the pump; no cadence known) cannot
    -- fit an ordinary stop: the run would start inside the slash. It is
    -- fired from past the charge's edge, or while the tyrant stands out the
    -- recovery after a rush or a charge (KITE_RECOVERY).
    local function late(g)
        local w = g and k.weapons[g]
        if not w or g == entry.prefer then
            return 0
        end
        if not w.cadence then
            return kite.shoot
        end
        local extra = (w.cadence - KITE_FAST_CADENCE) * kite.speed * KITE_LATE_SCALE
        return extra > 0 and extra or 0
    end
    local function slow(g)
        return kite.lower + late(g) >= kite.shoot
    end
    if ab == kite.rush or ab == kite.charge then
        kk.attacking = true
    elseif kk.attacking then
        kk.attacking = nil
        kk.recovered = now
    end
    local recovering = kk.recovered and now - kk.recovered < KITE_RECOVERY
    -- The gun a stop would take now (nil: no stop fits).
    local stop = gun and stop_gun()
    if stop and slow(stop) and not (d >= kite.edge or (recovering and d >= kite.urgent)) then
        stop = nil
    end
    -- No stop without the sight line (bot:enemies' `visible`): on 513's
    -- ring the gap is measured the way round the central block, and a stop
    -- with the tyrant round a corner shot into the wall (five of the
    -- handgun's last rounds; it was left on 0 health, one hit short, and
    -- kited with no rounds until the run ended). Out of sight, the bot
    -- holds at the top of the band below, so it walks round into view:
    -- running on, the two circled the block on opposite sides.
    local hidden = not e.visible
    if hidden then
        stop = nil
    end
    if kk.mode == "aim" then
        -- The stop's gun while it has rounds (its loading and ammunition).
        local rounds = 0
        for _, row in ipairs(bot:menu().items or {}) do
            if row.item == kk.gun then
                rounds = held_rounds(bot, row)
            end
        end
        if kk.gun and rounds > 0 then
            gun = kk.gun
        end
        kk.gun = gun
        if weapon.equipped ~= gun and gun then
            BotMenu.equip(bot, gun, "kite equip")
            return true
        end
        -- The killing weapon is committed to: its shot ends the fight.
        -- A stop with no hit yet holds on closer: the hit is what keeps the
        -- rush off.
        -- (Only for a gun as quick as the handgun: a slower one's recoil
        -- leaves no room for it.)
        local lower_at = kite.lower
        if (kk.last_hit or -1) < (kk.aim_start or now) and late(gun) == 0 then
            lower_at = kite.hold
        end
        -- A slower gun drops its aim earlier: its recoil runs longer.
        lower_at = lower_at + late(gun)
        if slow(gun) then
            lower_at = kite.lower + kite.lower - kite.hold
        end
        if gun == entry.prefer then
            lower_at = kite.commit
        end
        if d < lower_at or not gun or (ab ~= kite.walk and ab ~= 0) then
            set_mode("lower", string.format("close: gun=%s at %d ab=%d", tostring(gun), lower_at, ab))
        else
            -- Reload only with the gun held or down: a menu opened while the
            -- last round's shot plays cancels it.
            if weapon.clip <= 0 and (weapon.behavior == C.HOLD_BEHAVIOR or weapon.behavior == 0)
                and menu_reload(bot, s, weapon) then
                return true
            end
            local face = bot:face_to(e.x, e.z)
            local err = face.angle_error or 0
            local input = BotTactics.aim_input("torso")
            input.right = err > FACE_BAND
            input.left = err < -FACE_BAND
            -- Only with the sight line clear (see `hidden` below).
            if weapon.behavior == C.HOLD_BEHAVIOR and abs(err) <= FACE_BAND * 2 and e.visible then
                input.fire = true
                input.fire_pressed = true
            end
            bot:set_input(input)
            return true
        end
    end
    if kk.mode == "hold" and stop and d >= kite.lower and (ab == kite.walk or ab == 0) then
        -- Held facing it out of sight: it came round into view, so the
        -- stop starts here (no run and turn back first).
        kk.gun = stop
        bot:log(string.format("kite: stop with gun %s from the hold", tostring(stop)))
        set_mode("aim", "in sight")
        kk.aim_start = now
        return true
    end
    if kk.mode == "hold" and (d < kite.lower or stop) then
        set_mode("lower", "hold over")
    end
    if kk.mode == "lower" then
        if weapon.behavior ~= 0 then
            bot:set_input({})
            return true
        end
        -- The way to run decides the turn: a quick turn when it lies
        -- behind, else a turn in place onto it.
        local ux, uz = kite_run_heading(bot, player, e, kite, loop, kk)
        local delta = abs(angle_delta(angle_of(ux, uz), player.angle % C.FULL_TURN))
        if delta > KITE_QUICK_FROM then
            return quick_turn("run")
        end
        set_mode("run", string.format("turn in place %d", delta))
        kk.free_turn = true
    end
    -- Far enough: stop, turn to it and shoot.
    -- The rush comes `cooldown` walking ticks after its last hit: two
    -- seconds short of it without one, a nearer stop.
    local shoot_from = kite.shoot
    if kite.cooldown and now - (kk.last_hit or now) > kite.cooldown - 2 * C.SECOND then
        shoot_from = kite.urgent
    end

    -- A slower gun's stop starts its extra farther out (inside the edge).
    if stop then
        shoot_from = shoot_from + late(stop)
        if shoot_from > kite.edge then
            shoot_from = kite.edge
        end
    end
    -- A stop needs the run after it: room straight away from the tyrant
    -- (a stop against the deck's edge leaves only a run past its side).
    local function escape()
        if loop.ring then
            local _, off, seg = ring_project(loop.ring, player.x, player.z)
            return off <= seg.width
        end
        local ax, az = norm(player.x - e.x, player.z - e.z)
        return kite_room(bot, player, ax, az) >= RANGE // 2
    end
    if stop and d >= shoot_from and (ab == kite.walk or ab == 0) and kk.mode == "run"
        and escape() then
        kk.gun = stop
        bot:log(string.format("kite: stop with gun %s (pick %s)", tostring(kk.gun), tostring(gun)))
        if facing_err > C.FULL_TURN // 4 then
            return quick_turn("aim")
        end
        set_mode("aim", "far")
        kk.aim_start = now
        return true
    end
    -- No gun: hold at the top of the band, run when it closes.
    -- (Without a charge, a gun too slow for an ordinary stop runs on to
    -- the edge, where its stop fits: 513's shotgun.)
    if not stop and d >= kite.shoot and kk.mode == "run" and (not gun or kite.charge or hidden) then
        set_mode("hold", gun and "out of sight" or "no gun")
        bot:face_to(e.x, e.z)
        return true
    end
    if kk.mode == "hold" then
        bot:face_to(e.x, e.z)
        return true
    end
    set_mode("run", "")
    -- The killing weapon, once held, is equipped on the run (the menu
    -- pauses the room; the draw after it does not): a stop with it then
    -- spends no ticks on the draw.
    if entry.prefer and bot:has_item(entry.prefer) and weapon.equipped ~= entry.prefer
        and d >= kite.shoot then
        BotMenu.equip(bot, entry.prefer, "kite ready")
        return true
    end
    -- An empty gun is reloaded on the run (the menu pauses the room), so
    -- the next stop's aim is not spent on it.
    if gun and weapon.equipped == gun and weapon.clip <= 0 and weapon.behavior == 0
        and menu_reload(bot, s, weapon) then
        return true
    end
    local ux, uz = kite_run_heading(bot, player, e, kite, loop, kk)
    -- Close to it, steer within the run's own turn (a turn in place stops
    -- the run), through the clear heading nearest the one wanted. A turn
    -- just planned (off the aim) runs free until it is done.
    local cur = player.angle % C.FULL_TURN
    local want = angle_of(ux, uz)
    -- On a ring, close to it with the way behind (513's entry: the scene
    -- leaves the bot facing the tyrant): a quick turn, not a turn in place
    -- (the bot stood seven ticks and took the slash).
    if loop.ring and d < kite.lower + 2 * C.APPROACH and abs(angle_delta(want, cur)) > KITE_QUICK_FROM
        and now - (kk.quick_tick or -KITE_TURN_WAIT) > KITE_TURN_WAIT then
        kk.quick_tick = now
        return quick_turn("run")
    end
    if kk.free_turn and abs(angle_delta(want, cur)) < DASH_TURN then
        kk.free_turn = nil
    end
    -- A heading wanted past a right angle away is turned onto in place: the
    -- run's turn could only swing it round through the tyrant's side.
    if abs(angle_delta(want, cur)) > C.FULL_TURN // 4 then
        kk.free_turn = true
    end
    if d < kite.lower + 2 * C.APPROACH and not kk.free_turn and not loop.ring then
        local best, best_cost
        for off = -4, 4 do
            local a = (cur + off * KITE_STEER_MAX // 4) % C.FULL_TURN
            local hx, hz = heading(a)
            local room = kite_room(bot, player, hx, hz)
            local cost = abs(angle_delta(want, a)) + (room >= RANGE and 0 or (RANGE - room))
            if not best_cost or cost < best_cost then
                best, best_cost = a, cost
            end
        end
        ux, uz = heading(best)
    end
    if goal and not loop.ring then
        -- Head for the launcher while the tyrant is not that way (a ring's
        -- run keeps to its way round: the turns between stood still).
        local gx, gz = norm(goal.x - player.x, goal.z - player.z)
        local ex, ez = norm(e.x - player.x, e.z - player.z)
        if gx * ex + gz * ez < 0.3 then
            ux, uz = gx, gz
        end
    end
    dash(bot, player.x + ux * KITE_DASH, player.z + uz * KITE_DASH)
    return true
end

-- A <fight> enemy down with its death flag still clear (s.await): the bot
-- stands until the flag is set, or DEATH_WAIT has passed. 30C's Black Tiger
-- set it about 220 ticks after the shot that killed it.
local DEATH_WAIT = 15 * C.SECOND
local function await_death(bot, s)
    local w = s.await
    if not w then
        return false
    end
    if bot:flag(w.bank, w.bit) or s.now >= w.till then
        bot:log(string.format("death of 0x%02X: %s", w.enemy,
            bot:flag(w.bank, w.bit) and "its flag is set" or "no flag; waited enough"))
        s.await = nil
        return false
    end
    bot:set_input({})
    return true
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
    if spit_dodge(bot, s, player) then
        return true
    end
    if kite_step(bot, s, player) then
        return true
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
        s.trap_room = nil
        s.kite = nil
        s.await = nil
    end
    if bot:tick() < (s.calm_until or 0) and not s.active then
        return await_death(bot, s)
    end
    local k = bot:knowledge()
    local enemies = bot:enemies()
    local target = s.active and enemy_by_slot(enemies, s.target) or nil
    if s.active and not target then
        local id = s.target_id
        stop(bot, s, string.format("target down health=%s", tostring(s.last_health)))
        -- A <fight> enemy of this room is won once its death flag is set,
        -- at the end of its death throes: the bot waits for it. Seed 1's
        -- cave backtrack shot 30C's Black Tiger down in passing eight
        -- times and left within 200 ticks each time; the flag stayed
        -- clear and the tiger was back on the next entry.
        for _, fight in ipairs(bot:fight_facts()) do
            if fight.room == room and fight.enemy == id and fight.bank
                and not bot:flag(fight.bank, fight.bit) then
                s.await = { bank = fight.bank, bit = fight.bit, enemy = id,
                    till = s.now + DEATH_WAIT }
                bot:log(string.format("waiting for the death of 0x%02X (flag %d:%d)",
                    id, fight.bank, fight.bit))
            end
        end
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
            return await_death(bot, s)
        end
        reset(s)
        s.active = true
        s.gun = gun
        s.target = target.slot
        s.target_id = target.id
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
    -- A target the bot has fired on is finished off: out of the line of
    -- fire it is faced with the aim up, ready for when it shows again
    -- (MAX_TICKS without a hit still ends it). Seed 2's hunter, shot from
    -- 79 health to 9, stepped out of sight in 201; combat ended, the calm
    -- held the next fight off and it killed the bot 20 ticks later.
    local engaged = s.shots > 0
    if not target.visible then
        s.unseen = s.unseen + 1
        if not engaged and s.unseen > C.SECOND then
            stop(bot, s, "target lost", LOST_CALM)
            return false
        end
        if engaged and s.unseen > C.SECOND // 2 and player.state ~= HIT_STATE
            and bot:weapon().equipped == s.gun then
            if s.unseen == C.SECOND // 2 + 1 then
                bot:log(string.format("combat: slot=%d health=%d out of sight: waiting to finish it",
                    target.slot, target.health))
            end
            local face = bot:face_to(target.x, target.z)
            local err = face.angle_error or 0
            bot:set_input({ aim = abs(err) < FACE_BAND * 4,
                right = err > FACE_BAND, left = err < -FACE_BAND })
            return true
        end
    else
        s.unseen = 0
    end
    if s.last_health and target.health < s.last_health then
        bot:log(string.format("combat hit slot=%d health=%d->%d",
            target.slot, s.last_health, target.health))
        BotTactics.note_hit(s, entry, s.now, target.hit_state)
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
        low_reach = entry and entry.down_reach or nil,
        reach = entry and entry.reach,
        speed = entry and entry.speed,
        back_speed = C.SPEED.back,
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
    -- The walk in on a downed target (spacing's "approach"): straight at
    -- it, no shot on the way. Given up after SPACE_TICKS without getting
    -- inside the low shot's reach (something in the way): then the bot
    -- shoots from where it is.
    if move == "approach" then
        s.approach_ticks = (s.approach_ticks or 0) + 1
        if not s.reload_started and s.approach_ticks <= SPACE_TICKS then
            bot:walk_to(target.x, target.z, {
                run = target.distance - want >= C.RUN_MIN_DISTANCE,
                tolerance = SPACE_TOLERANCE,
                avoid_enemies = true,
            })
            s.aim_ticks = 0
            return true
        end
        move = "hold"
    else
        s.approach_ticks = nil
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

    -- A knife swing roots the bot about 25 ticks, and a raised aim takes
    -- ticks to drop: neither while a spitter may lock onto it. Face the
    -- target, aim down. A gun fires: holding it too, seeds 1 to 15 dodged
    -- 30C's tiger for 9000 ticks without a shot and were bitten every fourth
    -- dodge.
    if melee and BotTactics.spit_holds_fire(bot, player) then
        s.aim_ticks = 0
        bot:face_to(target.x, target.z)
        return true
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
    -- A shot lands some ticks after its press: one pressed while the hit
    -- reaction runs out lands as it ends, before the monster's next move
    -- (BotTactics.fire_early). And none at a target out of sight: 308's
    -- hunter stepped behind the corner as the aim came up, the shell hit
    -- the wall and its pump held the bot for the swipe.
    local early = BotTactics.fire_early(s, wrow, s.now, target.hit_state)
    if early == true then
        reacting = false
        pose = pose or s.held_pose
    elseif early == false then
        reacting = true
    end
    if not pose or leaping or weapon.behavior ~= C.HOLD_BEHAVIOR or reacting
        or not target.visible
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
