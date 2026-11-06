-- Engine rules the planner depends on, and nothing else.
--
-- This is the one module the bot.lua lint (src/engine/bot/lua_lint.rs)
-- exempts from its number rule: every other bot/*.lua file may not hold a hex
-- literal of 0x10 or more or a decimal literal of 100 or more, and no file
-- may hold a room-id string. Each constant here names the engine rule it
-- reflects. Room facts (slots, flags, items, coordinates) do not belong here;
-- they go in room/<id>/bot.xml.

BotConst = {}

-- The game advances 30 updates per second; tick budgets are in seconds.
BotConst.SECOND = 30

-- The action probe sits this far ahead of the player (RE1 reach offset).
BotConst.REACH = 600
-- Stand half a reach outside a zone so the probe lands inside it.
BotConst.APPROACH = 300
-- How close a <fight> step walks to an enemy that never comes to the player
-- (40C's Plant 42): inside the handgun's firing band.
BotConst.FIGHT_RANGE = 4500
-- walk_to walks the last 400 units; a shorter run overshoots the goal.
BotConst.RUN_MIN_DISTANCE = 600
-- Climbing an object (a crate): the vault lifts the player off the floor
-- (y 0) onto its top (11B's crate top is y -1800); a y this far up means on
-- top.
BotConst.CLIMB_HEIGHT = 500
-- How far behind the object's face to stand for the vault press: the
-- vault's probe reaches about 470 ahead, so the player stands just short
-- of it, where a push leaves it.
BotConst.CLIMB_STAND = 420
-- How far inside the object's ends to stand, so the vault probe hits its
-- face and the player lands on its top.
BotConst.CLIMB_INSET = 200
-- A position jump this large without a room change is a scripted warp.
BotConst.WARP_DISTANCE = 1000
-- Angles are 12-bit: a full turn is 0x1000 units.
BotConst.FULL_TURN = 0x1000

-- Door lock byte: bit 0x80 means locked, the low six bits name the bank-2
-- lock flag that opens it.
BotConst.LOCK_BANK = 2
-- See LOCK_BANK: the locked bit of a door's lock byte.
BotConst.LOCK_LOCKED = 0x80
-- See LOCK_BANK: lock % LOCK_SPAN is the lock flag.
BotConst.LOCK_SPAN = 0x40
-- Door key 0x00 on a locked door: only a room event raises the lock.
BotConst.KEY_NONE = 0x00
-- Door key 0xFE: "opens from the other side"; a press raises its lock flag.
BotConst.KEY_FAR_SIDE = 0xFE
-- Door key 0xFF: locked for good.
BotConst.KEY_NEVER = 0xFF
-- try_door: Jill's lockpick substitutes for the sword key (0x33) once she
-- holds the lockpick item (0x31) or scenario flag 0x7C is set.
BotConst.SWORD_KEY = 0x33
-- See SWORD_KEY: the lockpick item.
BotConst.LOCKPICK = 0x31
-- See SWORD_KEY: the scenario bank and bit has_lockpick tests.
BotConst.LOCKPICK_FLAG = { bank = 0, bit = 0x7C }
-- check_desk: a drawer whose bank-2 lock bit is clear opens only with the
-- desk key (0x3D) held or the lockpick (see LOCKPICK, LOCKPICK_FLAG).
BotConst.DESK_KEY = 0x3D

-- Bank 9 holds the per-item use flags: item `id` is armed at selector
-- id - ITEM_USE_BASE, and the menu USE of a key, special or chemical item
-- works only while its flag is set (GameState::use_item).
BotConst.ITEM_USE_BANK = 9
-- See ITEM_USE_BANK: the first item id with a use flag.
BotConst.ITEM_USE_BASE = 0x1B
-- Action flag 0x40: the zone probes the player's own position, not the
-- reach point (probe_actions).
BotConst.PROBE_BODY = 0x40
-- Action flag 0x80: the zone fires only on the action-press edge.
BotConst.ACTION_KEY = 0x80

-- A message id with bit 0x80 reveals twice as fast while action is held.
BotConst.MESSAGE_FAST = 0x80
-- being_attacked bit 0x80: a vault/stair sequence that a second press ends.
BotConst.ATTACK_SEQUENCE = 0x80
-- The damage pipeline (apply_weapon_damage_with) hits a monster only when
-- its status bits share one with the aim: down held aims 0x20 ...
BotConst.STATUS_LOW = 0x20
-- ... no direction aims 0x40 (the torso) ...
BotConst.STATUS_TORSO = 0x40
-- ... and up held aims 0x80.
BotConst.STATUS_HIGH = 0x80
-- player::TURN_STEP: an in-place turn rotates 0x60 a tick, so turning
-- around takes FULL_TURN / 2 / TURN_STEP ticks.
BotConst.TURN_STEP = 0x60
-- player::BACK_SPEED: the backward walk covers 0x40 a tick and keeps the
-- player facing forward, so it opens a gap on anything slower without the
-- turn-around a forward walk needs.
BotConst.BACK_SPEED = 0x40
-- Weapon behaviour 0x13 is the gun hold: a fire press is taken only here.
BotConst.HOLD_BEHAVIOR = 0x13
-- Weapon behaviour 0x18 is the reload running.
BotConst.RELOAD_BEHAVIOR = 0x18
-- Sixteen compass directions as unit vectors scaled by the first entry's
-- x (the bot Lua has no math library): the ring BotNav.stand_near searches.
BotConst.RING = {
    { 1000, 0 }, { 924, 383 }, { 707, 707 }, { 383, 924 },
    { 0, 1000 }, { -383, 924 }, { -707, 707 }, { -924, 383 },
    { -1000, 0 }, { -924, -383 }, { -707, -707 }, { -383, -924 },
    { 0, -1000 }, { 383, -924 }, { 707, -707 }, { 924, -383 },
}
-- The save/load screen lists eight numbered slots (rows 0..7), then the
-- load mode's autosave rows, then the exit row (ui/save_load.lua).
BotConst.SAVE_SLOTS = 8
-- Doom Book II; Doom Book I is the next id. Opened in the CHECK viewer, a
-- book becomes its medal (ui/item_view.lua).
BotConst.DOOM_BOOK_2 = 0x3F
-- The medal Doom Book II becomes: item - DOOM_BOOK_2 + WOLF_MEDAL.
BotConst.WOLF_MEDAL = 0x24
-- A doom book opens when the turntable's yaw is near 0x400 (the examine
-- check's window is 0x270..0x590); the bot turns into this narrower band so
-- the release tick cannot leave the window.
BotConst.EXAMINE_YAW_LOW = 0x340
-- The band's upper end (see EXAMINE_YAW_LOW).
BotConst.EXAMINE_YAW_HIGH = 0x4C0
-- A 12-bit angle's full turn: the viewer's yaw wraps at it.
BotConst.ANGLE_FULL = 0x1000
