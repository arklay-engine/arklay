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
| `bot/inventory.lua` | `BotInventory` | The item box and combine rules and the box screen driver ([Inventory room](#inventory-room)). |
| `bot/menu.lua` | `BotMenu` | Equip, heal, item use and combine through the inventory menu ([Menu skills](#menu-skills)). |
| `bot/nav.lua` | `BotNav` | Zone skills: approach points, `press_zone` (walk, face, press), `visit_camera` (walk into a camera switch zone) `push_object` (push a room object onto a zone, one axis at a time, or off it in a fact's direction) and `climb_object` (push an object under a high item, climb it, take the item, climb down). |
| `bot/planner.lua` | `BotPlanner` | Goal choice, the pursued target, attempt marks, the loop rule, the door route and the finish. |
| `bot/puzzle.lua` | `BotPuzzle` | The `<interact>`/`<camera>`/`<push>`/`<climb>` sequence, `<use>`, `<answer>` and `<avoid>` facts. |
| `bot/save.lua` | `BotSave` | Saving at typewriters, the latest-save tracking and the reload after a death ([Saving and reloading](#saving-and-reloading)). |
| `bot/scene.lua` | `BotScene` | Films, transitions, messages, yes/no prompts, pick-up viewers and the document reader. |
| `bot/survival.lua` | `BotSurvival` | Healing, the critical-health rule, the ammo reserve ([Ammunition](#ammunition)). |
| `bot/tactics.lua` | `BotTactics` | Combat from the knowledge file: fight or run, the aim pose, spacing and firing ([Combat](#combat)). |

Priorities, highest first:

1. A running menu job owns the pad, then an open item box screen
   (`BotInventory.screen_step`, [Inventory room](#inventory-room)). A death goes to `BotSave.on_death`; the
   typewriter's save screen and its fades go to `BotSave`. Scenes, films,
   transitions, messages and viewers go to `BotScene`. A yes/no prompt takes the `<answer>` that
   matches the last press or a set flag, and Yes otherwise. A document
   pick-up's reader is closed with cancel.
2. Survival. Poisoned (`bot:player().poisoned` or `seriously_poisoned`), use
   the first held `<cure>` of the knowledge file for that poison: a cure-only
   item at once, a herb mix that also heals only below the heal threshold;
   with none held, log it once per poisoning. Then heal below
   `max_health * (0.6 - 0.35 * risk)` with the largest held stack whose menu
   row `heals`, never in a grab or a scene; a fight is paused for it (the
   menu stops the game), so a fight under the threshold does not play on
   to the fight's low-health floor. Then a save at a
   typewriter here when the save decision allows it
   ([Saving and reloading](#saving-and-reloading)). Below
   `max_health * (0.2 - 0.15 * risk)`, with no heal item and damage taken
   in the last 5 seconds (critical), the bot saves if a typewriter here
   allows it (the save-safety rule refuses that save while a better one
   exists), plays on if a save exists (a death loads it), and only with no
   save at all finishes with
   `health critical with no heal item and no save while under attack`.
   Then combat (`BotTactics.step`, [Combat](#combat)): each nearby monster
   is fought or run past; a run, or sourcing ammunition
   ([Ammunition](#ammunition)), leaves the planner its goal and turns on
   `avoid_enemies` for its walks.
3. The room puzzle: the next `<interact>`/`<camera>`/`<push>` fact in
   document order (walk to the zone, press, wait for the done flag; or push
   the object onto its zone, below). The sequence position
   is the furthest fact whose flag was ever seen set, because a later step
   may clear an earlier step's flag. `<avoid>` slots are never used. Then
   the first `<use>` fact that applies: the item is held, its done flag is
   clear and its slot is a live use-item probe (`flag_bank_set`). The bot
   walks into the zone (inside it when the probe tests the player's
   position, bit `0x40`, else facing it), waits until the probe arms the
   item's use flag (bank 9, selector `item - 0x1B`), uses the item from the
   menu and waits up to 20 seconds for the done flag. A menu failure or a
   flag that never rises marks the fact. For 3 seconds after a goal is done
   here, no next goal is chosen while a `<use>` fact here has its item held
   and its done flag clear, and its slot, not yet a live probe, overlaps
   the zone that goal acted on (`BotPuzzle.use_pending`). The step's room
   event may still be arming the probe: 116's rack turns AOT 5 into one
   only after the lift that the shotgun pickup starts, and 10F's hollow
   arms after the gold emblem pickup. The log line is `waiting for the use
   probe`. Then the item errands (`BotPuzzle.errands`): a `<deliver>` here
   whose item is held, whose `after` flag is set and whose done flag is
   clear walks into a switch zone that cuts to camera `cut` and waits for
   the flag; then a `<fetch>` here whose done flag is set and whose item is
   not held presses AOT `slot` (marked `inventory full` when the item does
   not fit). An item a wanted `<deliver>` needs is kept like a `<use>` item,
   and a due `<fetch>` counts as a pickup needing a free slot.
4. Inventory room ([Inventory room](#inventory-room)): with every slot
   full and a known pickup needing one, a MOVE that empties a slot; then an
   item box here when a deposit or a withdrawal is due.
   Ground items here, keys (catalog kind `key`) first, then the nearest
   planned path. A visible item here is taken before the bot moves on,
   unless every inventory slot is full and the item does not merge into a
   held stack (`inventory full`, retried once the inventory changes). A
   drawer (a `desk` action, `check_desk`) counts as the item it holds
   (`item_slot`): the bot presses the drawer and answers Yes to the key and
   take prompts until the item is held or the drawer reads empty (`full`
   false). A drawer whose bank-2 lock bit (`lock_bank`, `lock_bit`) is
   clear is marked `drawer locked` until the desk key or the lockpick is
   held; an empty drawer is skipped. Any other item zone at the room
   origin is a scripted handover and is marked permanently. A zone that holds the centre of a zone of the same item
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
   held item with its done flag clear, and to each room with a due errand
   (`bot:errand_facts()`): a `<fetch>` while its `<deliver>` is wanted, a
   `<deliver>` while its item is held. An errand route gets 140 reach off
   its cost (twice a key's), since the room may want the item only for a
   while. The cost is the path length to the
   door's approach, plus 20 reach per door step of the route, plus, for a
   remote goal, its path length in its own room from where the bot last
   stood there (`REACH^3` when the planner found none), minus 15 reach for
   an unvisited room, 10 reach for a remote item, 70 more for a seen key
   item that a remembered locked door needs or else 60 more for the
   ammunition the gun needs ([Ammunition](#ammunition)), so a needed key
   comes before ammunition and herbs (`BotPlanner.item_bonus`), 60 for a held
   item's `<use>` room, the same item and `<use>` bonuses (70 reach) for a
   door into a room not yet entered whose `<holds>` fact names an item the
   bot lacks and a pending `<use>` needs, or the remote item bonus for a
   needed key or ammunition there (`BotPlanner.holds_bonus`, logged as
   `(holds 0xNN for ...)`), and
   200 reach for a door into the goal room or 6 for one into its stage; while sourcing ammunition, 60 more for a room not
   yet entered and one door step per monster seen on the route
   ([Ammunition](#ammunition)). The lowest cost wins and a remote winner becomes
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
room (the bot may stand in another part of it). Remote planning never
heads back to an item whose approach failed, even after the inventory
changed. An `<avoid>` item is marked as avoided before the free-slot check,
so a full inventory does not turn it into a goal that lifts on the next
inventory change. After 3 attempts the mark holds.

An item goal that costs a quarter of the maximum health while the planner
is trying it (`HURT_SHARE`) is dropped and marked `hurt while trying`:
like an approach failure, remote planning does not head back for it, and
the room's other pickups wait until the inventory changes, so the bot does
not walk back into what hurt it.

Approach points sit outside each side of a zone, at a quarter, a half and
three quarters of the action reach, and a quarter reach inside the edge. On
a long side there are also points a quarter of the way in from each end. On
a short side (`BotNav.side_offsets`, under two approaches long) they sit a
quarter approach in from each end. A narrow zone beside a wall has its
middle inside the wall's grown box, so only its open end can be stood at:
102's broken shotgun (x 6500..7000) is taken from x 6575.
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
scenario and `300` for `re1-full` (run it with `--only re1-full`): the
courtyard behind 11B, whose door opens once all four crests are in the 11A
wall (lock `0x17`), so entering 300 is the mansion's completion.

### Combat

`bot/tactics.lua` fights from `knowledge/<game>.xml`, found by the pack's
base game (the BioRand packs read `knowledge/re1.xml`). The file is strict
XML: an unknown attribute is an error, and `unknown` or `none` is `nil`.
Each `<weapon>` gives the cone (`near`, `side`), `cadence`, the shotgun's
`headshot` and `weak` distances; each `<enemy>` gives `policy`
(`fight`/`pass`/`avoid`), the poses (`stand`, `downed`, `up`), `reach`,
`crawl_reach`, `grabs`, `outrun`, `speed`, `health` and one `<keep>` band per
weapon. Every entry has a comment saying how it was measured.
Each `<cure item poison heals>` names an item that clears a poison
(`poison`: the `0x02` status, `serious`: `0x20`) and whether it also
restores health (`yes`/`no`); a unit test checks the rows against the item
catalog's heal effects.

- **Pose** (`BotTactics.pose`). The engine hits a monster only when its
  status bits share a bit with the aim (`src/combat.rs`,
  `apply_weapon_damage_with`): up `0x80`, neutral `0x40`, down `0x20`. Inside
  the weapon's headshot distance, with `up` allowed, the bot aims up; a
  monster showing only `0x20` (downed or crawling) takes the `downed` pose;
  otherwise `stand`, then whichever bit is set.
- **Gun** (`BotTactics.pick_gun`). The plain pick is the equipped firearm
  with rounds, else the held one with the most (clip plus the ammunition
  held for it). Against the nearest monster the knowledge file fights, it
  stands unless its rounds fall short of the kill (`<keep>` damage against
  the monster's health) while another gun's cover it. The gun that covers
  the kill in the fewest shots is then equipped, so the shotgun's shells go
  to a fight the handgun cannot finish.
- **Fight or run** (`BotTactics.decide`). Run when the monster has no entry,
  its policy is not `fight`, or there is no gun. A monster closing in
  (within twice its reach, or within the far edge of the gun's `<keep>`
  band) that cannot be outrun is fought with any rounds left: a zombie
  closes about as fast as the handgun brings it down, so the fight has to
  open at the band's edge to end before its grab. Otherwise run below
  `max_health * (0.45 - 0.3 * risk)`, or with too few rounds for it
  (`(health // damage + 1) * (1 + 0.5 * (1 - risk))`: a monster dies below
  zero health, so a hit that leaves it at exactly 0 does not kill) or for the
  crowd; fight a monster that cannot be outrun or one in a room the
  frontier crosses again (not while sourcing ammunition); run past one that
  is only in the way.
- **Spacing** (`BotTactics.spacing`). Hold and shoot inside the `<keep>`
  band and inside its near edge too: lowering the aim plays the holster,
  which drifts the player a few hundred units toward the monster, and
  re-raising it costs about 14 ticks, so a back-off from a close zombie
  handed it the grab. Back off to the band's far edge only while reloading
  (the aim is down anyway), and close in with a headshot weapon from just
  outside its headshot distance. A reload back-off that the monster's
  `speed` would close during the turn around, or one inside its reach,
  holds instead. The walks use
  `bot:spacing_point` and `walk_to` with `avoid_enemies`.

### Ammunition

`BotSurvival.ammo_need(bot)`: the equipped gun's rounds are its clip plus
the inventory count of its ammunition item (`bot:weapon().ammo`, the
engine's `weapon + 9` when the item catalog lists the weapon as a weapon and
that item as ammunition). The floor is `shots * (1 + 2 * (1 - risk))`,
rounded up: three kills at risk 0, two and a half at the default 0.25, one
at risk 1, where `shots` is the most hits one kill of a `fight` monster
takes with the weapon in the knowledge file (`health // damage + 1`:
9 for the re1 handgun, 4 for the shotgun). Below it, the way-out ranking
takes a reachable frontier item of that ammunition 60 reach cheaper, and
the planner logs `ammo low: R rounds under the floor F; N known ammo
item(s)`. Ammunition seen in a room is taken there anyway (step 4), so the
bonus matters for ammunition left behind (full inventory, a failed
approach).

The sourcing rule (`BotSurvival.sourcing`). Under the floor, with no
ammunition of that kind the planner can still reach (a frontier pickup it
has not given up on, with a route, or the remembered item box holding it),
the bot is sourcing ammunition (`ammo low: ...; sourcing`):

- the way-out ranking takes a door into a room not yet entered
  `SOURCE_BONUS` (60 reach) cheaper, and adds `MONSTER_COST` (one door
  step) for each live monster last seen in a room the route enters (at most
  3 a room), so exploration comes first and quiet routes are preferred;
  turning sourcing on drops the pursued target;
- every walk plans around the monsters (`avoid_enemies`);
- `BotTactics.decide` runs past a monster it would fight only because the
  room is crossed again; one that closes in or cannot be outrun is still
  fought;
- with a free slot and no item box ever opened (`box_items` unset), a box
  visit is due to look in the shared box (`look in the box: ...`). In the
  vanilla re1 pack the first box holds two handgun clips.

At finish the log lists every ammunition pickup the world model saw and
its fate (`ammo account: ROOM slot N item 0x.. xQ taken|left|gone (mark)`).

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
back. `bot:saves()` reports only what the engine's save screen wrote. The
modern save/load screen (the Save Screen option) publishes the same table:
its rows are listed newest first, so it adds `rows`, the slot each row
writes or loads, and its question opens on No, so it adds `answer`. The
bot's presses reach it as the keys a player's would.

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
- save safety (`BotSave.dead_end`): no save the reload would turn straight
  back into the same death, that is health under the critical floor
  (`max_health * (0.2 - 0.15 * risk)`) with no heal item held or with fewer
  rounds than one kill of the hardest `fight` monster takes. Such a save is
  made only when there is no other choice (`BotSave.no_choice`): no save
  exists yet, or the latest save is no better in health, rounds and heals
  (recorded with each save as `save_health`, `save_rounds`, `save_heals`);
- no save while inventory work is due in this room
  (`BotPlanner.inventory_due_here`: a combine, or a visit to an item box
  here), so the box or the mix comes first and the save holds its result
  (`inventory work first: ...`);
- critical health saves at once (it skips only the cooldown), when save
  safety allows it;
- otherwise at most one save per `(20 + 100 * risk)` seconds;
- risk up to `0.5` saves only for a reason: below half of max health (`low
  health`); below three quarters when the room the pursued target's route
  enters next had a live monster (`a risky room is next: ROOM`); or when a
  milestone is new since the last save (a key item taken, a door unlocked,
  a `<use>` fact done, or a puzzle goal the planner finished:
  `BotSave.milestones`) and the last save is at least
  `(2 + 4 * risk)` minutes old (`a milestone since the last save`), or at
  once while no save exists (`the first milestone of the run`);
- a bolder bot saves only below half of max health, or in a risky room: a
  live monster in the room, or damage taken in the last 30 seconds.

The latest save is kept in bot memory (`save_count`, `save_slot`,
`save_room`, `save_tick`, `save_progress`, `save_health`, `save_rounds`,
`save_heals`, `save_milestones`). The engine also keeps a copy of
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
item)`, `BotMenu.use_item(bot, item)` and `BotMenu.combine(bot, slot,
target)`. Each starts a job and returns
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
   `options` (`MOVE` for a combine), then confirm. A combine then d-pads the
   MOVE cursor (`bot:menu().move_cursor`) onto `8 + 2 * target` and
   confirms; a herb mix's yes/no prompt is answered Yes (right, then
   action).
6. `result`: success only when the engine's state shows it: `equipped` is
   the weapon, the item's count dropped, or (a combine) fewer slots are
   used. A refusal message is dismissed with action and fails the job; a
   combine with no effect 60 ticks after its confirm or prompt fails.
7. `close`: cancel out of the submenu and the panel (a key item's use closes
   the menu itself), then `settle` waits through the closing fade and the
   room's post-menu freeze without pressing, so no press leaks into the room.

The stats JSON's `menu` block counts, from the engine's menu state alone:
`opens` (inventory panel opened), `viewer_opens` (the same menu slot under a
pick-up viewer or item box), `open_ticks`, `cursor_moves`, `option_moves`,
`item_uses`, `heal_uses`, `equips`, `combines` (a MOVE that changed the
stacks), `box_deposits` and `box_withdrawals` (box slots that filled or
emptied; a swap counts one of each). Box swaps and combines are not item
uses. A menu job whose START landed as a zone press opened a pick-up viewer
or the item box instead fails at once and gives the pad back to the scene
handler, rather than waiting out its timeout over that screen.

The stats JSON's `activity` block puts every tick in one bucket, the first
that applies: `saving` (typewriter or save screen), `item_box`,
`pickup_viewer`, `examine`, `menu` (the bare inventory panel), `doors` (a
room transition), `scenes` (films, skippable scenes, messages), `pickup`,
`combat` (aim or fire held, or grabbed), `walking` (up or down held),
`turning` (left or right only), `uncontrolled` and `waiting` (in control,
no input). `room_activity` has the same buckets per room.
`scripts/bot-bench.sh` prints them with `scripts/bot-bench-summary.py`,
which also lists the time per goal kind, the saves with their reasons, the
box visits, the inventory-full marks and the quick bounces (A to B and
back with under 300 or 900 ticks in B).

### Inventory room

The inventory has 8 slots (`bot:menu().capacity`); the item box has 48,
shared by every box. `bot/inventory.lua` decides from the world model and
drives the box screen with pad input only.

- Keep (never deposit): keys, an item an open `<use>` fact needs, the
  equipped weapon, a firearm the knowledge file lists, the ammunition of
  those guns (the reserve), and heal items. Everything else may go in.
- Deposit when every slot is used and a known frontier pickup needs a
  free slot (it merges into no held stack, and the planner has not given
  up on it for a reason other than the full inventory). At a box in this
  room, also when no more than one slot (`BotInventory.RESERVE`) is free,
  counted after the visit's withdrawals, and more known pickups than that
  need one (`deposit: keep a slot free for pickups`); a trip to a remote
  box is made only for the full inventory.
- Look in the box when no box was ever opened and a slot is free: at a box
  in this room on the way (`look in the box on the way: never opened`), or
  from anywhere while sourcing ammunition ([Ammunition](#ammunition)).
- Withdraw an item the bot remembers in the box (`bot:memory().box_items`,
  written whenever the box screen is up) when a `<use>` fact needs it, a
  remembered locked door needs it, it is the equipped gun's ammunition
  under the reserve floor, or it is a heal and none is held (one stack).
  At most the free slots, after the deposits. At a box in this room, also
  every stack of the equipped gun's ammunition while its rounds are under
  twice the reserve floor, as long as one slot stays free (the top-up; no
  trip is made for it).
- Combine first, when every slot is used and a pickup needs room: the best
  pair of heal, ammo or weapon stacks that `bot:combine_result` says the
  engine applies and that empties a slot, heals first (rank 1, guns and
  ammo rank 2), never an item a `<use>` fact needs. Mixing herbs keeps the
  healing; a full herb stack stays reachable.

A box visit is a planner goal: press the nearest live `item_box` zone here
(by planned approach length, `BotPlanner.nearest`), or route to a
remembered one (`BOX_BONUS`). A remote box is priced by walking distance
over the bot's mental map (`BotPlanner.pick_remote_box`, a shortest-path
tree from `BotPlanner.map_tree`): the path length from here to each exit,
then inside each room from the point the bot arrived at through that door
(`bot:world()` keeps each door's and box's zone centre) to the next door
or the box, plus `MAP_DOOR` (15 reach) per door load. A leg inside a room
is the path length the bot measured from that arrival point when it last
chose a goal near it, else the straight line. The price comes from the
map, never from which box the bot chose before. The screen driver waits out the
lid, then per operation: in browse mode d-pad the inventory cursor to the
slot and confirm; in armed mode page the box cursor with up/down to the
target slot (nearest way round) and confirm (the engine swaps). A
deposit is done when the item's count drops, a withdrawal when it rises.
Then cancel closes the box and the menu, and the visit ends once the
player has been controllable for a second. A visit that moved nothing is
marked and not chosen again until the inventory changes, except a first
look (the contents are now remembered).


## API

`on_bot_tick(bot)` runs once per app update, before the game tick.

`on_bot_screen(bot)` (optional) runs instead on the title and the title's
load screen, where the bot loads its latest save after a death. Its handle
has `tick`, `risk`, `saves`, `memory`, `log`, `finish`, `set_input`,
`pulse_confirm` and `screen()`: `{kind = "title", stage, selection,
any_saves}` (`stage` `press`/`menu`/`fade_out`/`attract_fade`, `selection`
1 NEW GAME or 2 LOAD GAME) or `{kind = "load", readable, mode, stage,
cursor, exit_row, slots, rows, answer}`. A route without it finishes the run when it
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
| `bot:player()` | `{x, y, z, angle, reach_x, reach_z, health, max_health, state, being_attacked, equipped, collision_flags, grabbed, poisoned, seriously_poisoned}`. `collision_flags` is the live health-status/collision byte (bit `0x10` skips shape-5 floor volumes). `poisoned` and `seriously_poisoned` are the engine's health-status poison bits (`0x02`, cured by a blue herb; `0x20`, cured by the serum). `grabbed` is the entity's grab zone flag. `reach_x`/`reach_z` is the forward action probe (`probe_actions`, a reach ahead of the player): a zone press fires only when it is inside the zone, and `press_zone` holds forward into furniture while it falls short. |
| `bot:scene()` | `{mode, skippable, film, transition, cutscene_skip, message, message_takes_input, message_waiting, prompt, prompt_choice, pickup, reader, frozen, controls_locked, controllable, dead, typewriter, save_screen}`. `prompt` is a yes/no prompt on screen and `prompt_choice` its highlighted answer (`"yes"`/`"no"`; left selects No, right Yes). `reader` is a document pick-up's FILE reader, which closes on cancel. `typewriter` is the fade into or out of the typewriter's save screen; `save_screen` (only while it is up) is `{mode, stage, cursor, exit_row, slots, rows, answer}`: `mode` `"save"`, `stage` `idle`/`confirm_overwrite`/`save_anim`/`error`/`fade_out`, `cursor` the highlighted row (0-based; rows `0..7` are the slots), `slots` whether each slot holds a save. The modern screen also sets `rows`, the 0-based slot each row writes or loads (`-1` for none; save mode's first row is the new save, writing the first free slot), and `answer`, the open question's highlighted answer (`"yes"`/`"no"`; left selects Yes); `cursor` is `-1` while no row is highlighted. |
| `bot:actions()` | Array of `{slot, kind, x, z, width, depth, item, quantity, next_room, key, lock, handler, flags}`. An item AOT whose flags lack the player bit (`0x01`) cannot be picked up yet (a pickup a room event arms later): its `kind` is `item_dormant` and `item`/`quantity` are `nil`. |
| `bot:doors()` | Array of `{slot, target, next_room, key, lock, x, z, width, depth}`; `target` is the destination room id. |
| `bot:objects()` | The room's active objects (blocks, statues, shelves): `{slot, x, z, half_x, half_z, pushable}`, `x`/`z` the live centre and `half_*` the collision half-extents. `pushable` is solid and without the not-pushable flag. The walk and path planner treat every solid object as an obstacle, a box 128 units inside the object on each side, rebuilt when an object moves. |
| `bot:enemies()` | Array of active monsters (`id < 0x20`, `health > 0`), nearest first: `{slot, id, x, y, z, angle, bearing, health, state, status_flags, action_behavior, action_state, hit_state, distance, visible}`. `bearing` is the absolute 12-bit angle from the player; `distance` is horizontal; `visible` is the weapon line-of-sight result. Boss ids are included. |
| `bot:weapon()` | `{equipped, weapon, clip, reserve, ammo, behavior, state, aim_state, target}`. `equipped` is the inventory item (or nil), `weapon` is `equipped_weapon_id`, `clip` is the rounds in the equipped weapon's stack, `reserve` is `item_count(weapon + 9)`, `ammo` is `bot:ammo_for(weapon)`, and `target` is the locked entity slot or nil. |
| `bot:knowledge()` | The game's combat knowledge, read-only: `{source, enemies, weapons, cures}`. `enemies[id]` is `{name, policy, stand, downed, up, reach, crawl_reach, grabs, outrun, speed, health, keep}` with `keep[weapon] = {min, max, damage}`; `weapons[id]` is `{name, near, side, cadence, headshot, weak, status_bit}`. `cures` is an array of `{item, poison, heals}` from `<cure>` rows, in file order: `poison` is `"poison"` or `"serious"` and `heals` whether the item also restores health. An `unknown` value is `nil`. With no file, the tables are empty. See [Combat](#combat). |
| `bot:spacing_point(slot, want)` | A point `want` from monster `slot` that is free of collision, at least `want` from every other monster, in a clear line from the player and not reached by walking past the monster: `{x, z}` (the nearest such point) or `nil`. |
| `bot:can_see(slot)` | Whether monster `slot` is in the current snapshot with a clear weapon sight line. |
| `bot:flag(bank, bit)` | A flag bit, read-only. |
| `bot:has_item(item)` / `bot:item_count(item)` | Inventory queries. |
| `bot:inventory()` | Read-only array of the live inventory slots: `{slot, item, quantity, kind}` rows, including zero-quantity weapon slots. `kind` is the catalog kind. |
| `bot:item_kind(item)` | The item catalog's kind (`"weapon"`, `"ammo"`, `"key"`, `"heal"`, ...) and whether a pickup merges into a held stack (two values). |
| `bot:ammo_for(weapon)` | The ammunition item `weapon` fires, or `nil` when the catalog does not list it as a weapon with an ammunition item. |
| `bot:use_facts()` | Every room's `<use>` facts from the pack's `room/<id>/bot.xml` files, by room id then document order: `{room, slot, item, bank, bit, label}`. Read once per pack. |
| `bot:item_facts()` | Every room's `<holds>` facts, the same way: `{room, item, label}`. |
| `bot:errand_facts()` | Every room's `<deliver>` and `<fetch>` facts, the same way: `{room, kind, cut, slot, item, bank, bit, after_bank, after_bit, label}`, `kind` `"deliver"` (with `cut`) or `"fetch"` (with `slot`); `after_bank`/`after_bit` are `nil` without an `after` flag. |
| `bot:menu()` | The pause menu as the engine shows it, read-only: `{open, requested, phase, accepts_input, screen, cursor, slot, mode, option, options, selected_item, ready, closing, viewing, capacity, equipped, items}`. `phase` is `closed`, `requested`, `fade_out`, `fade_in`, `opening`, `closing`, `screen`, `message` or `input`; `screen` names a sub-screen (`pickup`, `examine`, `item_box`, `map`, `file`); `cursor` is the menu's cursor byte (`0..6` tabs, `8 + 2 * slot` items); `mode` is `navigation`, `actions` or `move`; `option` is the highlighted submenu row (0-based) and `options` its labels (`EQUIP`/`USE`, `CHECK`, `MOVE`). `items` rows are `{slot, item, quantity, kind, heals}` with the menu's 0-based `slot`; `heals` is the catalog's word that a USE restores health (false for a blue herb, the serum or a red herb alone). `move_cursor` is the MOVE cursor byte while `mode == "move"`. While the item box screen is up, `box = {mode, box_cursor, cursor, sliding, items}`: `mode` is `browse` or `armed`, `box_cursor` the box slot (0..47), `cursor` the inventory cursor byte, `sliding` true while a page slides (no input is read), and `items` the box rows `{slot, item, quantity, kind}`. |
| `bot:combine_result(slot, target)` | Read-only: what a MOVE of menu slot `slot` onto `target` would do, from the engine's recipe: `{result, cursor_item, cursor_quantity, target_item, target_quantity, frees}`. `result` is `applied`, `herb` (a herb mix; the menu asks first), `no_recipe`, `herb_no_recipe` or `drug_store`; `frees` is true when the combine empties a slot. Nothing is changed. |
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

- The model ignores the ticks of a door load between the player's
  placement in the new room and the room's own load (`SessionSnapshot.booted`
  false): on a stage change the engine shows the old room's doors and actions
  under the new room id for those ticks.
- `rooms` lists every room entered and every target of a seen door;
  `visited` is true once entered. `entries` counts room changes into it.
- A door is `locked` when it was locked the first time the bot saw it
  (lock bit `0x80` and its bank-2 flag clear), and `unlocked` once that flag
  is observed set. While the bot is in the room the record follows the
  door's lock byte: a door seen without bit `0x80` (a room script removed the
  lock) is no longer `locked`, and one that gains the bit while its flag is
  clear is `locked` again. It is `traversed` when the room changes to its target
  right after an action press on it, or when it is the old room's only door
  to the new room. On arrival, the new room's door back to the old room (the
  one nearest the player, if several) is `traversed` too: the bot came through
  its far side, so it is not a door to explore. `x`/`z` are the zone centre.
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
| `<interact>` | `slot`, `bank`, `bit`, optional `done`, optional `label` | Press AOT `slot`; flag `bank`/`bit` set means done. With `done="clear"` the flag clearing means done instead: bank 7's item bits are set while the item is in the room and clear when it is taken. |
| `<camera>` | `cut`, optional `bank`+`bit`, optional `label` | Walk into a camera switch zone that cuts to camera `cut` (a room event keyed on the camera). With a flag, it is done once the flag is set; without one, once the cut is entered on this room entry. It is one step of the same sequence as `<interact>`. |
| `<answer>` | `choice` (`yes` or `no`), `slot` and/or `bank`+`bit`, optional `label` | At the yes/no prompt opened by AOT `slot` (or identified by the flag), choose `choice`. At least one of `slot` or the flag is required; `bank` and `bit` go together. |
| `<avoid>` | `slot`, `reason`, optional `bank`+`bit`+`while`, optional `unless_held` | The planner should not use AOT `slot`; `reason` is a non-empty note. With `bank`, `bit` and `while` (`set` or `clear`), only while that flag is in that state. With `unless_held`, not while that item is held (a swap: lift one item only while holding the one that goes in its place). |
| `<push>` | `object`, `slot`, optional `direction`, optional `bank` + `bit`, optional `label` | Push room object `object` (a `bot:objects()` slot) until its centre is in the middle half of AOT `slot`'s zone, usually an object-position probe. It is one step of the `<interact>` sequence, done while the object stays there. The push is cardinal: one axis at a time, standing behind the object (600, then 450, then 400 units behind its face; on the face's middle, then a third of its half size to either side) and holding forward; an axis that stalls twice is given up for the other. With `direction` (`north` = +z, `south`, `east` = +x, `west`) the object is pushed off the zone instead: that way only, until its trailing face passes the zone's far quarter. Such a step is also done once the zone is gone or the item it names was taken. With `bank` and `bit` (both or neither) the step is also done once that flag is set: a zone event that takes the object away (a statue that falls). A push the bot stops for a fight or a heal walks back behind the object before it measures a stall. |
| `<use>` | `slot`, `item`, `bank`, `bit`, optional `label` | Use `item` (non-zero) from the menu while standing in AOT `slot`, a use-item probe; flag `bank`/`bit` set means done. |
| `<climb>` | `object`, `slot`, `bank`, `bit`, optional `done`, optional `label` | Take an item off a high shelf: push room object `object` until it covers AOT `slot`'s item zone (and has stalled against the wall at least once), stand behind its face away from the zone, press action to climb it, press facing the zone to take the item, then face away and press to climb down. It is one step of the `<interact>` sequence, done once flag `bank`/`bit` reads done (`done="clear"` for a bank-7 item bit) and the player is off the object. |
| `<watch>` | `bank`, `bit`, optional `after_bank` + `after_bit`, optional `label` | Let a skippable scene play instead of skipping it while flag `bank`/`bit` is clear and, if given, the `after` flag is set: a scene a skip breaks. |
| `<holds>` | `item` (non-zero), optional `label` | The room holds `item`: what is where, never when to go. Read by `bot:item_facts()` for every room. |
| `<deliver>` | `item` (non-zero), `cut`, `bank`, `bit`, optional `after_bank` + `after_bit`, optional `label` | Bring `item` here and walk into camera `cut`, where a room event takes it; flag `bank`/`bit` set means done. With an `after` flag, wanted only once it is set. Read by `bot:errand_facts()` for every room. |
| `<fetch>` | `item` (non-zero), `slot`, `bank`, `bit`, optional `label` | Press AOT `slot` to take `item`, offered while flag `bank`/`bit` is set; fetched only while a `<deliver>` of the same item is wanted. Read by `bot:errand_facts()` for every room. |

Every fact element also takes `character="jill"` or `character="chris"`: the
fact then holds for that character only (player flag bit 0: 1 Jill, 0 Chris).
The engine drops the other character's facts before `bot:room_meta()` and
`bot:use_facts()` see them, so a room whose two variants number their AOT
slots differently lists each character's facts side by side.

```xml
<bot format="2">
  <interact slot="4" bank="1" bit="0x22" label="push the statue"/>
  <answer choice="no" slot="6" label="do not drink"/>
  <avoid slot="7" reason="the plant stuns the player"/>
  <use slot="2" item="0x26" bank="1" bit="0xA9" label="herbicide at the pump"/>
</bot>
```

`bot:room_meta()` always has `interacts`, `uses`, `answers`, `avoids` and
`watches` arrays (empty for a format-1 file): `{kind = "press", slot, bank, bit,
label}`, `{kind = "camera", cut, bank, bit, label}` or `{kind = "push",
object, slot, label}` in document order,
`{slot, item, bank, bit, label}` in document order, `{choice, slot, bank,
bit, label}` (absent fields are `nil`), `{slot, reason, bank, bit,
while, unless_held}` and `{bank, bit, after_bank, after_bit, label}`.

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
| 105 | Jill: `<use>` the gold emblem (`0x20`) at slot 11, the fireplace; done flag bank 1 `0xAB`. Chris: the same at slot 8; done flag `0xA8`. The fireplace hollow's message (Jill slot 13, Chris slot 10) is avoided. |
| 10C | `<use>` the herbicide (`0x26`) at slot 2, the pump; done flag bank 1 `0xA9`. `<avoid slot="4" bank="1" bit="0xA9" while="clear">`: the plant fountains stand across the way to the armor key until the herbicide. |
| 102 | `<holds item="0x1C">`: the broken shotgun lies here (the room's door from 101 takes the armor key). |
| 210 | `<interact slot="4" bank="7" bit="0x81" done="clear">`: the moon crest has no player bit; the nest event (AOT 4) hands it over. |
| 202 | `<push object="0" slot="3" bank="0" bit="0x0B">`: the statue along the balcony until it falls; the fall sets scenario `0x0B` and puts the blue jewel (`0x21`) in 105. |
| 10D | `<use>` the blue jewel (`0x21`) at slot 2, the tiger's eye; done flag bank 0 `0x0F` (the wind crest pickup arms). `<use>` the red jewel (`0x22`) at slot 2; done flag bank 0 `0x0E` (the magnum). |
| 10F | `<push object="1" slot="5" direction="east">`: the bookshelf off the music score's pickup. `<use>` the music score (`0x23`) at slot 13, the piano; done flag bank 1 `0xA5` (the bar counter moves off the gold emblem). `<use>` the wood emblem (`0x1F`) at slot 6, the hollow; done flag bank 1 `0xA7`. |
| 11A | Four `<use>` crests at slots 5, 3, 4 and 6 (star `0x2D`, sun `0x2E`, moon `0x2C`, wind `0x29`); done flags bank 0 `0x6A`, `0x69`, `0x6B`, `0x6C`. |
| 106 | `<camera cut="4">`, then `<camera cut="2" bank="1" bit="0x05">`: Barry's hall scene plays out as Jill walks into the north cut, then the middle cut; the flag marks it done and the east-wing door is restored. |
| 116 | `<avoid slot="1" unless_held="0x1C">`: lifting the shotgun without the broken one springs the 115 ceiling trap, so the rack is used only while the broken shotgun is held. `<use>` the broken shotgun (`0x1C`) at slot 5, the rack's probe (armed once the shotgun is lifted); done flag bank 7 `0x01` (the broken one hangs on the rack; room main also clears common `0x20`). |
| 11B | `<climb object="0" slot="3" bank="7" bit="35" done="clear">`: the square crank lies on a shelf over the crate's corner; room main keeps its pickup an item only while the player stands on the crate (`FG_STATUS` 24 clear). |
| 117 | Seven `<interact>` paintings in picture order; each push keeps only its own bank-4 bit, the last sets scenario `0x03`. |
| 301 | `<use>` the square crank (`0x1D`) at slot 4; done flag scenario `0x2F`, which the drain event sets. `<watch>` the same flag: the drain scene is not skipped. Using a crank from the menu leaves the player in the crank turn, whose completion the event waits on. |
| 205 | `<push>` armor object 1 onto vent slot 1 and object 2 onto vent slot 2, then `<interact slot="3" bank="0" bit="0x3A">` (the switch) and `<answer choice="yes" slot="3">`. With both vents covered Yes opens the sun crest (item slot 9); with one open it releases the gas. |
| 20D | Jill: `<deliver item="0x42" cut="3" bank="1" bit="0x61" after_bank="1" after_bit="0x62">`: poisoned Richard waits (`0x62`) for the serum; walking into camera 3 with it hands it over and sets `0x61` (so does his death after 420 seconds). Until then the door to 20E is a message. |
| 100 | Jill: `<fetch slot="5" item="0x42" bank="1" bit="0x23">`: the serum on the shelf, offered once Richard's scene sets `0x23`. |
| 219 | `<interact slot="4" bank="7" bit="0x39" done="clear">`: the shelf event hands over the battery, whose own pickup zone sits at the room origin. |

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
