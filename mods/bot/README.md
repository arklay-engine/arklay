# Player bot

A restricted Lua planner that drives a normal new Jill game through the same pad
a human would use. It cannot set flags, items or positions, and it cannot run
an item, equip, heal or room-action handler. Its only output is pad and UI
input (`set_input`, `press_action`, `pulse_confirm`, and the direction/run
buttons `walk_to`/`face_to` set). Equip, heal and item use go through the
inventory menu by those presses ([Menu skills](#menu-skills)); room items,
doors and stair checks are reached by walking into the zone and pressing
action. A unit test (`bot_ctx_has_no_state_changing_method_but_input`) lists
the Lua handle's registered methods and fails on any method that is not a
read, the bot's own bookkeeping, or input.

```sh
# Watch the bot in the normal window (bare --bot implies --ui game --character 1)
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot

# Edit bot/**.lua while the bot runs: --watch re-executes the chunks in the
# same Lua state, so BOT_STATE and the engine's bot memory keep their progress.
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot --watch

# --bot-risk 0.0..1.0 (default 0.25) is how much danger the planner may accept;
# the planner reads it as bot:risk(). Values outside the range are an error.

# --bot-goal-room ROOM finishes the run on entering ROOM (three hex digits).

# Headless: runs as fast as the simulation allows and reports ticks/rooms/reason
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot --bot-headless
```

A bot run saves at typewriters through the game's own save screen. Without
an explicit `--save-dir` it writes to a fresh directory of its own,
`<temp>/arklay-bot-saves-<pid>`, so it never overwrites the player's saves.

`--watch` reloads the bot in place when `bot/**.lua` changes: the chunks
re-run in the same Lua state, so `BOT_STATE` keeps its plan and the engine
keeps its remembered memory, navigation and tick count. Editing a bot file
does not restart the room or the run. A new game or restart rebuilds the bot
VM from the pack, so bot globals start fresh there. A changed
`room/<id>/bot.xml` invalidates the cached room metadata, which the planner sees
through `bot:room_meta()` on the next tick.

## Planner

The bot has no route. Each tick it reads the snapshot, the world model
(`bot:world()`) and the current room's facts (`bot:room_meta()`), and picks
the next goal. The Lua holds no room id, flag bit, item id, AOT slot or
coordinate. Room facts live in `room/<id>/bot.xml`, which the Rust side
reads ([Room metadata](#room-metadata)), and engine rules live in
`bot/constants.lua`.

Chunks run in path order, so the modules are global tables:

| File | Table | Role |
| --- | --- | --- |
| `bot/constants.lua` | `BotConst` | Engine rules: tick rate, action reach, the door lock encoding, the special key values, the message fast bit, the enemy status (aim) bits, the tank turn step, the gun hold and reload behaviours, the item-use flag bank and selector base, and the AOT probe and action-key flag bits. |
| `bot/init.lua` | | `on_bot_tick`: the priority order below, plus damage and room-entry logging. |
| `bot/menu.lua` | `BotMenu` | Equip, heal and item use through the inventory menu ([Menu skills](#menu-skills)). |
| `bot/nav.lua` | `BotNav` | Zone skills: approach points, `press_zone` (walk, face, press) and `visit_camera` (walk into a camera switch zone). |
| `bot/planner.lua` | `BotPlanner` | Goal choice, the pursued target, attempt marks, the loop rule, the door route and the finish. |
| `bot/puzzle.lua` | `BotPuzzle` | The `<interact>`/`<camera>` sequence, `<use>`, `<answer>` and `<avoid>` facts. |
| `bot/save.lua` | `BotSave` | Saving at typewriters, the latest-save tracking and the reload after a death ([Saving and reloading](#saving-and-reloading)). |
| `bot/scene.lua` | `BotScene` | Films, transitions, messages, yes/no prompts, pick-up viewers and the document reader. |
| `bot/survival.lua` | `BotSurvival` | Healing, the critical-health rule, the ammo reserve ([Ammunition](#ammunition)). |
| `bot/tactics.lua` | `BotTactics` | Combat from the knowledge file: fight or run, the aim pose, spacing and firing ([Combat](#combat)). |

Priorities, highest first:

1. A running menu job owns the pad. A death goes to `BotSave.on_death`; the
   typewriter's save screen and its fades go to `BotSave`. Scenes, films,
   transitions, messages and viewers go to `BotScene`. A yes/no prompt takes the `<answer>` that
   matches the last press or a set flag, and Yes otherwise. A document
   pick-up's reader is closed with cancel.
2. Survival. Heal below `max_health * (0.6 - 0.35 * risk)` with the largest
   held heal stack, never in a grab, a fight or a scene. Then a save at a
   typewriter here when the save decision allows it
   ([Saving and reloading](#saving-and-reloading)). Below
   `max_health * (0.2 - 0.15 * risk)`, with no heal item and damage taken
   in the last 5 seconds (critical), the bot saves if a typewriter here
   allows it, plays on if a save exists (a death loads it), and only with no
   save at all finishes with
   `health critical with no heal item and no save while under attack`.
   Then combat (`BotTactics.step`, [Combat](#combat)): each nearby monster
   is fought or run past; a run leaves the planner its goal and turns on
   `avoid_enemies` for its walks.
3. The room puzzle: the next `<interact>`/`<camera>` fact in document order
   (walk to the zone, press, wait for the done flag). The sequence position
   is the furthest fact whose flag was ever seen set, because a later step
   may clear an earlier step's flag. `<avoid>` slots are never used. Then
   the first `<use>` fact that applies: the item is held, its done flag is
   clear and its slot is a live use-item probe (`flag_bank_set`). The bot
   walks into the zone (inside it when the probe tests the player's
   position, bit `0x40`, else facing it), waits until the probe arms the
   item's use flag (bank 9, selector `item - 0x1B`), uses the item from the
   menu and waits up to 20 seconds for the done flag. A menu failure or a
   flag that never rises marks the fact.
4. Ground items here, keys (catalog kind `key`) first, then the nearest
   planned path. A visible item here is taken before the bot moves on,
   unless every inventory slot is full and the item does not merge into a
   held stack (`inventory full`, retried once the inventory changes). A
   zone at the room origin is a scripted handover and is marked
   permanently. A zone that holds the centre of a zone of the same item
   taken here may be a second AOT of that pickup: it gets one approach, and
   a press there with no pickup marks it permanently.
5. A passage check, when a door or item of this room has no planned
   approach: a stair zone (`door_check`), or an action-key event zone
   (flags `>= 0x80`) that no fact names, since a script may start the climb
   from an event. One press per slot per room entry, nearest first.
6. The way out. A pursued target keeps its course: the bot takes the first
   door of the route to it until it is reached, real progress re-ranks, or
   the loop rule drops it (`pursuing ...`). Otherwise one ranking by
   distance cost over the doors here and the first door of the route
   (breadth-first over the remembered doors) to each remote frontier item or
   door, and to each room whose `<use>` fact (`bot:use_facts()`) needs a
   held item with its done flag clear. The cost is the path length to the
   door's approach, plus 20 reach per door step of the route, plus, for a
   remote goal, its path length in its own room from where the bot last
   stood there (`REACH^3` when the planner found none), minus 15 reach for
   an unvisited room, 10 reach for a remote item, 60 more for the
   ammunition the gun needs ([Ammunition](#ammunition)) or for a seen key
   item that a remembered locked door needs, 60 for a held
   item's `<use>` room, and 200 reach for a door into the goal room or 6
   for one into its stage. The lowest cost wins and a remote winner becomes
   the pursued target; the pick is logged as `way out: <why>, cost N`. An
   exit with no planned approach from where the bot stands (another level
   of the room) gets a passage check first. A `<use>` that failed in its
   room is retried there, never travelled to. `planner.lua`'s header has
   the formula.
7. Nothing left: every approach or time-out mark under 3 attempts gets one
   more try (the bot may now stand elsewhere); then
   `frontier empty: nothing reachable is left` (with the count of goals
   marked looping). Entering the `--bot-goal-room` finishes with
   `goal room X reached`. Every finish lists the locked doors
   (`needs key 0x..`) and the blocked goals with their reason.

The loop rule (`BotPlanner.pursue`, `BotPlanner.repeat_check`). Real
progress is a change in the rooms entered, the items taken, the doors
unlocked or the inventory, or a puzzle or `<use>` step done; it re-ranks the
pursued target and resets the counts. Without it:

- a room entered more than 6 times marks the pursued target looping (with
  no target, the last exit the planner took from that room), and more than
  12 times finishes the run with `looping: entered X N times without
  progress`;
- a pursued target whose route from a newly entered room is no shorter
  (in doors) than the shortest seen counts a stall, and the third stall
  marks it looping.

A looping mark holds for 3 minutes of game time or until the inventory
changes, and counts as an attempt. The limits are three times what the
re1 bench run to 20D needs: at most 2 entries of one room between progress
events and 1 stall. Before the rule, that run entered 204 41 times in a
204/206/207 cycle between two targets whose remote costs disagreed room to
room; the 207 exit toward 10B has no planned approach from the level the
bot enters on, which the passage-first rule above now handles.

Doors follow the engine's `try_door` rules. A door is locked when its lock
byte has bit `0x80` and the bank-2 flag `lock & 0x3F` is clear. The key
values:

| Key | Meaning | Planner |
| --- | --- | --- |
| `0x00` | Only a room event raises the lock. | Recorded as `locked by a room event`; usable once the flag is set. |
| `0xFE` | Opens from the other side; a press raises the flag. | Pressed like an unlocked door. (`0xFE` is checked before the general key rule.) |
| `0xFF` | Locked for good. | Marked permanently. |
| `0x33` (sword key) | Jill turns it with the lockpick (item `0x31` or scenario flag bank 0 bit `0x7C`). | Tried when either is held. |
| any other | The key item. | Tried only with the key held; otherwise recorded as `needs key 0x..`. |

A failed goal is marked with its reason. It is retried when the inventory
changes, and an approach failure is also retried on the next entry into the
room (the bot may stand in another part of it). After 3 attempts the mark
holds.

Approach points sit outside each side of a zone, at a quarter, a half and
three quarters of the action reach, and a quarter reach inside the edge. On
a long side there are also points a quarter of the way in from each end.
Points the path planner reaches come first, by path length; then up to 6
unplanned points by distance, each with a short walk budget.

The path planner searches a 512-unit lattice first. When that finds no path
(a corridor narrower than the lattice rows, like the 10C bed passage), it
falls back to a 128-unit lattice whose blocked cells and connected parts
are cached per room and body. Both test every node and every edge against
the room collision with the player's radius, so neither cuts through a wall
thinner than its step; the fallback rejects a goal inside the collision or
in another part of the room. A start inside the body margin (a stair warp
can leave the player there) steps to the nearest free node first.

`--bot-goal-room ROOM` (three hex digits, needs `--bot`) is read as
`bot:goal_room()`. `scripts/bot-bench.sh` passes `20D` for the `re1`
scenario.

### Combat

`bot/tactics.lua` fights from `knowledge/<game>.xml`, found by the pack's
base game (the BioRand packs read `knowledge/re1.xml`). The file is strict
XML: an unknown attribute is an error, and `unknown` or `none` is `nil`.
Each `<weapon>` gives the cone (`near`, `side`), `cadence`, the shotgun's
`headshot` and `weak` distances; each `<enemy>` gives `policy`
(`fight`/`pass`/`avoid`), the poses (`stand`, `downed`, `up`), `reach`,
`crawl_reach`, `grabs`, `outrun`, `speed`, `health` and one `<keep>` band per
weapon. Every entry has a comment saying how it was measured.

- **Pose** (`BotTactics.pose`). The engine hits a monster only when its
  status bits share a bit with the aim (`src/combat.rs`,
  `apply_weapon_damage_with`): up `0x80`, neutral `0x40`, down `0x20`. Inside
  the weapon's headshot distance, with `up` allowed, the bot aims up; a
  monster showing only `0x20` (downed or crawling) takes the `downed` pose;
  otherwise `stand`, then whichever bit is set.
- **Fight or run** (`BotTactics.decide`). Run when the monster has no entry,
  its policy is not `fight`, or there is no gun. A monster closing in
  (within twice its reach) that cannot be outrun is fought with any rounds
  left. Otherwise run below `max_health * (0.45 - 0.3 * risk)`, or with too
  few rounds for it (`health / damage * (1 + 0.5 * (1 - risk))`) or for the
  crowd; fight a monster that cannot be outrun or one in a room the
  frontier crosses again; run past one that is only in the way.
- **Spacing** (`BotTactics.spacing`). Keep the `<keep>` band: back off to
  its middle when too close, to its far edge while reloading, and close in
  with a headshot weapon from just outside its headshot distance. A back-off
  that the monster's `speed` would close during the turn around, or one
  inside its reach, holds and shoots instead. The walks use
  `bot:spacing_point` and `walk_to` with `avoid_enemies`.

### Ammunition

`BotSurvival.ammo_need(bot)`: the equipped gun's rounds are its clip plus
the inventory count of its ammunition item (`bot:weapon().ammo`, the
engine's `weapon + 9` when the item catalog lists the weapon as a weapon and
that item as ammunition). The floor is `shots * (1 + 2 * (1 - risk))`,
rounded up: three kills at risk 0, two and a half at the default 0.25, one
at risk 1, where `shots` is the most hits one kill of a `fight` monster
takes with the weapon in the knowledge file (`health / damage`, rounded up:
9 for the re1 handgun, 4 for the shotgun). Below it, the way-out ranking
takes a reachable frontier item of that ammunition 60 reach cheaper, and
the planner logs `ammo low: R rounds under the floor F; N known ammo
item(s)`. Ammunition seen in a room is taken there anyway (step 4), so the
bonus matters for ammunition left behind (full inventory, a failed
approach).

### Lint

`engine::bot::lua_lint` (a unit test) scans `mods/bot/bot/*.lua`. Outside
comments and outside `constants.lua`, it fails on:

- a string literal that looks like a room id: exactly three characters, a
  decimal digit followed by two hex digits (`"10F"`, `"117"`, `"20D"`);
- a hex number literal of `0x10` or more;
- a decimal number literal of `100` or more.

`constants.lua` is the one exception. It holds at most 40 assignments in at
most 140 lines, and every assignment has a `--` comment, on the same line or
the line above, naming the engine rule. Room-id strings are banned there
too. Write larger values as expressions of constants (`30 * C.SECOND`,
`C.APPROACH // 2`).

### Saving and reloading

A typewriter is a room action of kind `typewriter` (the engine's room-action
handler `0x10`). The bot saves like a player: it walks into the zone and
presses action, answers the save prompt Yes, waits through the fade
(`scene.typewriter`), then drives the save screen (`scene.save_screen`):
d-pad to the first empty slot (or, with all eight full, the slots in turn),
confirm, accept the overwrite question, and wait for the write and the fade
back. `bot:saves()` reports only what the engine's save screen wrote.

The ink ribbon: the save prompt needs one only when the character's ink
ribbon option is on (`bot:saves().needs_ribbon`), and then the save spends
one. Jill's option (`jill_ink_ribbons`) is off by default, so by default Jill
saves without a ribbon, spends none, and no ribbon appears in the game. With
the option on she needs a held ribbon for every save, and the bot saves only
while it holds one.

The decision (`BotSave.wants`), scaled by `bot:risk()`:

- no save without a ribbon when one is needed;
- no save when nothing new happened since the last save or since the run
  started (rooms entered, items taken, doors unlocked);
- critical health saves at once (it skips only the cooldown);
- otherwise at most one save per `(20 + 100 * risk)` seconds;
- risk up to `0.5` saves at every typewriter it passes;
- a bolder bot saves only below half of max health, or in a risky room: a
  live monster in the room, or damage taken in the last 30 seconds.

The latest save is kept in bot memory (`save_count`, `save_slot`,
`save_room`, `save_tick`, `save_progress`). The engine also keeps a copy of
the bot's memory and world model from the tick each save is written.

After a death, with a save, the bot confirms through the DIED screen. The
game returns to the title; there `on_bot_screen(bot)` runs instead of
`on_bot_tick`: it picks LOAD GAME, moves the load screen's cursor to the
latest save's slot and confirms. The loaded session carries the run on: the
tick count, the statistics and the save history continue, the memory and the
world model go back to the copy from that save (the room path is kept), the
navigation cache is dropped and the Lua VM is rebuilt (`BOT_STATE` starts
fresh). Without a save the run finishes `dead, no save`; more than 3 deaths
since one save finish the run too. The stats JSON counts `saves`,
`reloads` and `deaths_before_reload`.

The bot reads the shipped save screen's state; a mod's registered save
screen or the modern screen shows the bot nothing, and the save or load
finishes with a reason.

### Menu skills

`bot/menu.lua` defines `BotMenu.equip(bot, item)`, `BotMenu.use_heal(bot,
item)` and `BotMenu.use_item(bot, item)`. Each starts a job and returns
`true`, or returns `false` (and logs) when the item is not held, is the wrong
kind, another job is running, or the same job failed in the last 300 ticks.
While a job runs, `on_bot_tick` hands it the tick before the scene handler,
and the job drives the menu with one button pulse at a time (one release tick
between pulses), each step waiting on `bot:menu()`:

1. `open`: pulse START (every 20 ticks at most) until the engine shows the
   menu requested or open; fail after 150 ticks.
2. wait through the fades and the open delay until `phase == "input"`.
3. `cursor`: d-pad toward `8 + 2 * slot` (right flips the column, up/down
   step rows), reading the cursor back after every press.
4. `select`: confirm on the item until `mode == "actions"`.
5. `option`: when the submenu is `ready`, d-pad to the `EQUIP`/`USE` row of
   `options`, then confirm.
6. `result`: success only when the engine's state shows it: `equipped` is
   the weapon, or the item's count dropped. A refusal message is dismissed
   with action and fails the job.
7. `close`: cancel out of the submenu and the panel (a key item's use closes
   the menu itself), then `settle` waits through the closing fade and the
   room's post-menu freeze without pressing, so no press leaks into the room.

The stats JSON's `menu` block counts, from the engine's menu state alone:
`opens` (inventory panel opened), `viewer_opens` (the same menu slot under a
pick-up viewer or item box), `open_ticks`, `cursor_moves`, `option_moves`,
`item_uses`, `heal_uses` and `equips`.


## API

`on_bot_tick(bot)` runs once per app update, before the game tick.

`on_bot_screen(bot)` (optional) runs instead on the title and the title's
load screen, where the bot loads its latest save after a death. Its handle
has `tick`, `risk`, `saves`, `memory`, `log`, `finish`, `set_input`,
`pulse_confirm` and `screen()`: `{kind = "title", stage, selection,
any_saves}` (`stage` `press`/`menu`/`fade_out`/`attract_fade`, `selection`
1 NEW GAME or 2 LOAD GAME) or `{kind = "load", readable, mode, stage,
cursor, exit_row, slots}`. A route without it finishes the run when it
leaves the game session.

| Call | Result |
| --- | --- |
| `bot:tick()` | Monotonic app-update counter. |
| `bot:room()` | `{id, stage, room, player_flag, mansion2, camera, controllable}`. |
| `bot:room_meta()` | The current room's authored `room/<id>/bot.xml` metadata, or `nil`. Read-only; see [Room metadata](#room-metadata). |
| `bot:world()` | The observed world model: rooms, doors, items, action presses and the frontier. Read-only; see [World model](#world-model). |
| `bot:risk()` | The `--bot-risk` setting, `0.0..1.0` (default `0.25`). Scales the heal, fight-or-run, save and critical-health thresholds. |
| `bot:saves()` | The run's saves, read-only: `{count, slot, room, tick, reloads, deaths, deaths_since_save, needs_ribbon, ribbons}`. `slot` (0-based), `room` and `tick` describe the latest save the engine's save screen wrote and are `nil` before the first. `needs_ribbon` is the character's ink ribbon rule and `ribbons` the held count. |
| `bot:goal_room()` | The `--bot-goal-room` id (uppercase, such as `"20D"`), or `nil`. |
| `bot:cameras()` | The current room's camera switch zones: `{from, to, x, z}` rows, `x`/`z` the zone's corner mean. Each cut group's header record is skipped. |
| `bot:player()` | `{x, y, z, angle, health, max_health, state, being_attacked, equipped, collision_flags, grabbed}`. `collision_flags` is the live health-status/collision byte (bit `0x10` skips shape-5 floor volumes). `grabbed` is the entity's grab zone flag. |
| `bot:scene()` | `{mode, skippable, film, transition, cutscene_skip, message, message_takes_input, message_waiting, prompt, prompt_choice, pickup, reader, frozen, controls_locked, controllable, dead, typewriter, save_screen}`. `prompt` is a yes/no prompt on screen and `prompt_choice` its highlighted answer (`"yes"`/`"no"`; left selects No, right Yes). `reader` is a document pick-up's FILE reader, which closes on cancel. `typewriter` is the fade into or out of the typewriter's save screen; `save_screen` (only while it is up) is `{mode, stage, cursor, exit_row, slots}`: `mode` `"save"`, `stage` `idle`/`confirm_overwrite`/`save_anim`/`error`/`fade_out`, `cursor` the highlighted row (0-based; rows `0..7` are the slots), `slots` whether each slot holds a save. |
| `bot:actions()` | Array of `{slot, kind, x, z, width, depth, item, quantity, next_room, key, lock, handler, flags}`. |
| `bot:doors()` | Array of `{slot, target, next_room, key, lock, x, z, width, depth}`; `target` is the destination room id. |
| `bot:enemies()` | Array of active monsters (`id < 0x20`, `health > 0`), nearest first: `{slot, id, x, y, z, angle, bearing, health, state, status_flags, action_behavior, action_state, hit_state, distance, visible}`. `bearing` is the absolute 12-bit angle from the player; `distance` is horizontal; `visible` is the weapon line-of-sight result. Boss ids are included. |
| `bot:weapon()` | `{equipped, weapon, clip, reserve, ammo, behavior, state, aim_state, target}`. `equipped` is the inventory item (or nil), `weapon` is `equipped_weapon_id`, `clip` is the rounds in the equipped weapon's stack, `reserve` is `item_count(weapon + 9)`, `ammo` is `bot:ammo_for(weapon)`, and `target` is the locked entity slot or nil. |
| `bot:knowledge()` | The game's combat knowledge, read-only: `{source, enemies, weapons}`. `enemies[id]` is `{name, policy, stand, downed, up, reach, crawl_reach, grabs, outrun, speed, health, keep}` with `keep[weapon] = {min, max, damage}`; `weapons[id]` is `{name, near, side, cadence, headshot, weak, status_bit}`. An `unknown` value is `nil`. With no file, both tables are empty. See [Combat](#combat). |
| `bot:spacing_point(slot, want)` | A point `want` from monster `slot` that is free of collision, at least `want` from every other monster, in a clear line from the player and not reached by walking past the monster: `{x, z}` (the nearest such point) or `nil`. |
| `bot:can_see(slot)` | Whether monster `slot` is in the current snapshot with a clear weapon sight line. |
| `bot:flag(bank, bit)` | A flag bit, read-only. |
| `bot:has_item(item)` / `bot:item_count(item)` | Inventory queries. |
| `bot:inventory()` | Read-only array of the live inventory slots: `{slot, item, quantity, kind}` rows, including zero-quantity weapon slots. `kind` is the catalog kind. |
| `bot:item_kind(item)` | The item catalog's kind (`"weapon"`, `"ammo"`, `"key"`, `"heal"`, ...) and whether a pickup merges into a held stack (two values). |
| `bot:ammo_for(weapon)` | The ammunition item `weapon` fires, or `nil` when the catalog does not list it as a weapon with an ammunition item. |
| `bot:use_facts()` | Every room's `<use>` facts from the pack's `room/<id>/bot.xml` files, by room id then document order: `{room, slot, item, bank, bit, label}`. Read once per pack. |
| `bot:menu()` | The pause menu as the engine shows it, read-only: `{open, requested, phase, accepts_input, screen, cursor, slot, mode, option, options, selected_item, ready, closing, viewing, capacity, equipped, items}`. `phase` is `closed`, `requested`, `fade_out`, `fade_in`, `opening`, `closing`, `screen`, `message` or `input`; `screen` names a sub-screen (`pickup`, `examine`, `item_box`, `map`, `file`); `cursor` is the menu's cursor byte (`0..6` tabs, `8 + 2 * slot` items); `mode` is `navigation`, `actions` or `move`; `option` is the highlighted submenu row (0-based) and `options` its labels (`EQUIP`/`USE`, `CHECK`, `MOVE`). `items` rows are `{slot, item, quantity, kind}` with the menu's 0-based `slot`. |
| `bot:memory()` | String map set by `remember`. |
| `bot:visited(room)` | Whether a room id such as `"106"` (or packed `0x106`) was seen. |
| `bot:remember(key, value)` | Store a string/number/boolean in memory; `nil` removes it. |
| `bot:log(text)` | Print `[bot] tick N room R: text`. |
| `bot:finish(reason)` | End the run and report `reason`. |
| `bot:set_input(table)` | Set the next tick's player/UI input. |
| `bot:press_action()` | Hold and edge the action button for one tick. |
| `bot:pulse_confirm()` | Edge UI confirm for one tick. |
| `bot:walk_to(x, z, opts)` | Collision-aware A* walk. Returns `{status, distance, angle_error}`. |
| `bot:face_to(x, z)` | Turn toward a point. Returns `{status, distance, angle_error}`. |
| `bot:distance(x, z)` | Horizontal distance. |

### World model

`bot:world()` returns a fresh table built only from what the bot observed
this run (the room snapshot, doors, actions, room items, inventory counts and
flags); it is reset with the bot's memory on a new game. Arrays are sorted by
room id, then slot, so iteration is deterministic.

```lua
{
  room = "106",
  rooms = {
    { id = "106", visited = true, entries = 2, last_entry = 812,
      doors = { { slot = 1, target = "107", target_visited = true, key = 0,
                  locked = false, unlocked = false, traversed = true,
                  present = true, x = 9000, z = 9000 } },
      items = { { slot = 4, item = 0x30, item_kind = "special", quantity = 1, present = false,
                  taken = true, taken_tick = 640, x = 12000, z = 14000 } },
      actions = { { slot = 3, kind = "event", presses = 1, last_press = 700,
                    message = 0x21, flags = { { bank = 4, bit = 0x1A } } } } },
    { id = "107", visited = false, entries = 0, doors = {}, items = {}, actions = {} },
  },
  frontier = {
    { kind = "door", room = "106", slot = 2, target = "108", target_visited = false,
      key = 0x30, locked = true, x = 3000, z = 3000 },
    { kind = "item", room = "107", slot = 3, item = 0x40, item_kind = "special",
      quantity = 1, x = 1000, z = 1000 },
  },
}
```

- `rooms` lists every room entered and every target of a seen door;
  `visited` is true once entered. `entries` counts room changes into it.
- A door is `locked` when it was locked the first time the bot saw it
  (lock bit `0x80` and its bank-2 flag clear), and `unlocked` once that flag
  is observed set. While the bot is in the room the record follows the
  door's lock byte: a door seen without bit `0x80` (a room script removed the
  lock) is no longer `locked`, and one that gains the bit while its flag is
  clear is `locked` again. It is `traversed` when the room changes to its target
  right after an action press on it, or when it is the old room's only door
  to the new room. `x`/`z` are the zone centre.
- A *press* is an action edge while the room is controllable, credited to
  the first action-key entry whose zone holds the reach point (as the engine
  picks it). `message` is the first message opened and `flags` the flags
  (bank 5 excepted, at most 8) that rose within 60 ticks of the latest press
  in the same room.
- A room item is `taken` when its slot stops being that item while the bot
  stays in the room, and either the item's inventory count rose that tick or
  the bot pressed the slot within 120 ticks. An item that vanished otherwise
  (a script, or absent on re-entry) is only `present = false`.
- `frontier` is every present, untraversed door and every present item not
  taken; `locked` there is the door's current state. Item rows (in `items`
  and the frontier) carry `item_kind`, the catalog kind.

### Room metadata

A room may author puzzle data next to its other files as
`room/<id>/bot.xml`. The merged pack resolves it like any other entry, so a
mod's copy overlays the base pack's. The file is optional; without one
`bot:room_meta()` returns `nil`. The parser rejects unknown elements and
attributes, malformed actions, and out-of-range banks, and names the file and
source line on error.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<bot format="1">
  <puzzle id="paintings" title="117 large gallery painting order">
    <step slot="3" bank="4" bit="0x1A" label="117 painting 1/7 newborn">
      <walk x="19500" z="8100"/>
      <face x="19500" z="7200"/>
    </step>
  </puzzle>
</bot>
```

`format` is `1` (the default when absent) or `2`. `puzzle` takes
`id` and optional `title`; `step` takes `slot`, `bank`, `bit` and `label`.
`bit` is the flag selector passed to `bot:flag`, so the examples use `0x1A`,
not bit number 26. A step has any number of `<walk x="..." z="..."/>` points
and at most one `<face x="..." z="..."/>`; coordinates are signed 32-bit
integers. `slot` and `bit` are bytes and `bank` must be below 10.

Format 2 adds fact elements. They describe the room, never coordinates or
walk paths, have no children, and are an error in a format-1 file:

| Element | Attributes | Meaning |
| --- | --- | --- |
| `<interact>` | `slot`, `bank`, `bit`, optional `label` | Press AOT `slot`; flag `bank`/`bit` set means done. |
| `<camera>` | `cut`, optional `bank`+`bit`, optional `label` | Walk into a camera switch zone that cuts to camera `cut` (a room event keyed on the camera). With a flag, it is done once the flag is set; without one, once the cut is entered on this room entry. It is one step of the same sequence as `<interact>`. |
| `<answer>` | `choice` (`yes` or `no`), `slot` and/or `bank`+`bit`, optional `label` | At the yes/no prompt opened by AOT `slot` (or identified by the flag), choose `choice`. At least one of `slot` or the flag is required; `bank` and `bit` go together. |
| `<avoid>` | `slot`, `reason` | The planner should not use AOT `slot`; `reason` is a non-empty note. |
| `<use>` | `slot`, `item`, `bank`, `bit`, optional `label` | Use `item` (non-zero) from the menu while standing in AOT `slot`, a use-item probe; flag `bank`/`bit` set means done. |

```xml
<bot format="2">
  <interact slot="4" bank="1" bit="0x22" label="push the statue"/>
  <answer choice="no" slot="6" label="do not drink"/>
  <avoid slot="7" reason="the plant stuns the player"/>
  <use slot="2" item="0x26" bank="1" bit="0xA9" label="herbicide at the pump"/>
</bot>
```

`bot:room_meta()` always has `interacts`, `uses`, `answers` and `avoids`
arrays (empty for a format-1 file): `{kind = "press", slot, bank, bit,
label}` or `{kind = "camera", cut, bank, bit, label}` in document order,
`{slot, item, bank, bit, label}` in document order, `{choice, slot, bank,
bit, label}` (absent fields are `nil`) and `{slot, reason}`.

`bot:room_meta()` returns a fresh Lua table; it is a read-only view in the
sense that it cannot write game state:

```lua
local meta = bot:room_meta()
if meta then
    for _, puzzle in ipairs(meta.puzzles) do
        if puzzle.id == "paintings" then
            for _, step in ipairs(puzzle.steps) do
                -- step.slot, step.bank, step.bit, step.label,
                -- step.walks = { {x=..., z=...}, ... }, step.face = {x=..., z=...}
            end
        end
    end
end
```

The planner reads only the format-2 facts; `puzzles` stays for format-1
files. The checked-in facts:

| Room | Facts |
| --- | --- |
| 105 | `<use>` the gold emblem (`0x20`) at slot 11, the fireplace; done flag bank 1 `0xAB`. |
| 10C | `<use>` the herbicide (`0x26`) at slot 2, the pump; done flag bank 1 `0xA9`. |
| 10F | `<use>` the wood emblem (`0x1F`) at slot 6, the hollow; done flag bank 1 `0xA7`. |
| 11A | Four `<use>` crests at slots 5, 3, 4 and 6 (star `0x2D`, sun `0x2E`, moon `0x2C`, wind `0x29`); done flags bank 0 `0x6A`, `0x69`, `0x6B`, `0x6C`. |
| 106 | `<camera cut="4">`, then `<camera cut="2" bank="1" bit="0x05">`: Barry's hall scene plays out as Jill walks into the north cut, then the middle cut; the flag marks it done and the east-wing door is restored. |
| 116 | `<avoid slot="1">`: lifting the shotgun without the broken one springs the 115 ceiling trap. |
| 117 | Seven `<interact>` paintings in picture order; each push keeps only its own bank-4 bit, the last sets scenario `0x03`. |

`set_input` keys are literal: `up down left right` (player directions plus the
UI edge/held flag), `held_up` etc. (UI held only), `up_pressed` etc. (UI edge
only), `run`, `run_pressed`, `aim`, `aim_pressed`, `fire`, `fire_pressed`,
`action`/`action_held`, `action_pressed`, `confirm`, `cancel`, `start`.
Unknown keys are a Lua error.

`walk_to` options: `{ run = true, tolerance = 250 }`, or `{ walk = true }` to
force a walk. The default tolerance is 300. `status` is one of `moving`,
`arrived`, `unreachable` or `blocked`. A blocked/unreachable goal is terminal
for that goal; call with a different point (or a different tolerance) to retry.
`angle_error` is signed 12-bit units, positive meaning turn right.

With `run = true`, `walk_to` latches the run: it starts only on a small heading
error at more than 800 units, keeps it through moderate errors as a run-turn,
and walks the final 400 units. A sharp corner stops for an in-place turn and
then resumes the run. Planning uses the engine's 18-unit collision skin, so a
path cannot graze a wall the movement resolver would push against.

The Lua sandbox has no `math` or `os` library; use plain arithmetic,
`string.format` and `table.concat`.
