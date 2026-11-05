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
-- walk_to walks the last 400 units; a shorter run overshoots the goal.
BotConst.RUN_MIN_DISTANCE = 600
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

-- A message id with bit 0x80 reveals twice as fast while action is held.
BotConst.MESSAGE_FAST = 0x80
-- being_attacked bit 0x80: a vault/stair sequence that a second press ends.
BotConst.ATTACK_SEQUENCE = 0x80
-- The damage pipeline needs aim down for monster status bit 0x20 ...
BotConst.STATUS_LOW = 0x20
-- ... and aim up for status bit 0x80.
BotConst.STATUS_HIGH = 0x80
-- Weapon behaviour 0x18 is the tactical reload running.
BotConst.RELOAD_BEHAVIOR = 0x18
-- Enemy ids of the common zombie classes, the only ones the bot fights.
BotConst.COMMON_ZOMBIES = { [0x00] = true, [0x01] = true, [0x11] = true }
-- The handgun's near cone excludes bodies within their radius plus 200, so
-- back off inside this distance ...
BotConst.COMBAT_RETREAT_START = 1800
-- ... until the target is this far away.
BotConst.COMBAT_RETREAT_DONE = 2600
-- Monsters beyond this distance are outside the handgun's lock range.
BotConst.COMBAT_RANGE = 4500
