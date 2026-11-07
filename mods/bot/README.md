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

# --bot-seed N (u32, needs --bot) seeds the game's random number generator so
# each seed plays a different game (hunter health, enemy choices, poison).
# The same seed, code and --bot-risk replay identically; without the flag the
# game's own stream is untouched. A reload after a death gets a fresh stream
# derived from the seed. Compare changes over many seeds: scripts/bot-batch.sh.

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
| `bot/terminal.lua` | `BotTerminal` | The lab computer terminal (`bot:scene().computer`): log in, pick the first door still locked, type the command. The credentials and door lock bits are `TERMINAL` in `bot/constants.lua`. |

Priorities, highest first:

1. The lab computer terminal, while it is up (`BotTerminal.step`): one
   keyboard edge every other tick logs in, picks the menu row of the first
   `TERMINAL.door_bits` lock still clear and types the command that
   releases it. A running menu job owns the pad, then an open item box screen
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
   to the fight's low-health floor. While a room's `<fight>` step is still
   ahead (`BotPuzzle.fight_ahead`: the step or one leading into it, such as
   20C's wake-up press; no fight running yet), the threshold is
   `0.85 * max_health`, so the bot walks into a boss fight healed. The
   steps that lead into it wait (`BotSurvival.fight_ready`, marked `not
   ready for the fight`, lifted once the inventory changes) while health
   is under that with no heal held, while no heal is held and the
   remembered box holds one (a firearm fight; the box's `no heal item
   held` withdrawal fetches it), while no held firearm has rounds at all
   (`no firearm with rounds`, whatever the `<keep>` bands know: 30C's
   Black Tiger met with an empty handgun), or while the held rounds'
   damage (each gun's rounds times its `<keep>` damage) is under the
   enemy's health.
   Fight steps are seen only in their own room, so once a held-back
   fight is ready (a heal fetched from the box), the planner heads back
   to its room from anywhere (`toward the fight in 40C ... now ready`,
   weighted like a held item's `<use>`). A fight counts as held back
   once its step was held back or once the step found its enemy and left
   it alive; both are kept in the memory (`fight_met:ROOM:ENEMY`), which a
   reload puts back as it was at the save, while the planner's marks are
   lost with the rebuilt VM (40C's Plant 42, never returned to after a
   death). A held-back fight in the room the bot stands in is fought
   anyway once nothing else is left (`frontier empty; fighting anyway`,
   for the rest of that room entry): 303's heliport locks every door
   behind the flare, and holding back there only ended the run. Then a save at a
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
3. A `<leave>` fact whose flag is set: out through its door before
   anything else (507's gas drains health until the room changes). While
   a `<leave>` flag is still clear (`BotPuzzle.leave_ahead`) the heal
   threshold is `0.85 * max_health` and the puzzle waits as for a fight
   (`not ready for the room's hazard`), so the gas finds the bot healed. Then
   the room puzzle: the next `<interact>`/`<camera>`/`<push>` fact in
   document order (walk to the zone, press, wait for the done flag; or push
   the object onto its zone, below). The sequence position
   is the furthest fact whose flag was ever seen set, because a later step
   may clear an earlier step's flag. A `<push>` flag is no position mark,
   only one more way that step reads done; a push finished on this room
   entry stays done until the bot leaves, so a later push may move the
   object out of an earlier step's band. `<avoid>` slots are never used. Then
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
   arms after the gold emblem pickup. Entering a room counts as such a goal
   done anywhere in it: 40A's init sets the bookshelf as an event and its
   main script re-arms the red book's probe a few ticks later. The log line
   is `waiting for the use probe`. Then the item errands (`BotPuzzle.errands`): a `<deliver>` here
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
   file (catalog kind `file`, the lab's pass codes) goes to the FILE tab
   and needs no slot. A
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
   approach: a stair zone (`door_check`), a ladder (`stairs_zone`, done
   once the climb has carried the player a warp's distance from the
   approach point), or an action-key event zone (flags `>= 0x80`) that no
   fact names, since a script may start the climb from an event. One press
   per slot per room entry, nearest first. A slot that is a door to
   another room is no passage (307's ladder up to 302 was climbed on
   every entry while its door to 30B was out of reach). A puzzle step
   done here lifts this room's door marks of a room event's message too
   (302's battery powers the elevator whose call showed message 132).
6. The way out. A pursued target keeps its course: the bot takes the first
   door of the route to it until it is reached, real progress (a room
   entered for the first time, an item taken, a door unlocked or newly made a
   door by a room event, the inventory changed) re-ranks, or the loop rule
   drops it (`pursuing ...`). Otherwise one ranking by
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
   comes before ammunition and herbs (`BotPlanner.item_bonus`), for a held
   item's `<use>` room, 280 reach and none of the door steps or the remote
   path, only the first door's length (the medal is carried across the
   mansion to 305's fountain rather than left for a nearer frontier), the same item and `<use>` bonuses (70 reach) for a
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
   more try (the bot may now stand elsewhere). Within 30 seconds of
   entering a room with a door parked at its origin, the bot then idles
   (`waiting for door slot N -> X at the room origin`): the room's event
   may carry the player through it (310's lift down to 500). Then
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
| `0xFF` | Locked for good from this side. | Marked permanently, until its lock flag is set (opened from the far side: 507's door 0 opens 505's door 2). |
| `0x33` (sword key) | Jill turns it with the lockpick (item `0x31` or scenario flag bank 0 bit `0x7C`). | Tried when either is held. |
| any other | The key item. | Tried only with the key held; otherwise recorded as `needs key 0x..`. |

A failed goal is marked with its reason. It is retried when the inventory
changes, and an approach failure is also retried on the next entry into the
room (the bot may stand in another part of it). Remote planning never
heads back to an item whose approach failed, even after the inventory
changed. An `<avoid>` item is marked as avoided before the free-slot check,
so a full inventory does not turn it into a goal that lifts on the next
inventory change. After 3 attempts the mark holds, except an `inventory
full` mark on a key or an item a `<use>` fact needs: that waits for the
inventory to change however often it was tried (401's red book, passed
three times with every slot used), one on a heal item while the bot is
hurt with no heal held (`BotSurvival.needs_heal`: the mansion's green
herbs, given up while the inventory was full, when 408's wasps had stung
it to 1), and one on the ammunition of a gun held or in the box (a run
left 35 shells on full inventories and ran dry). A done
puzzle step (an `<interact>`, `<use>`, `<deliver>` or `<fetch>`) clears
the approach marks of its room and the door approach marks of every room:
301's refilled pool stops 302's waterfall, and the door to 307 beyond it
had used up its attempts before.

An item goal that costs a quarter of the maximum health while the planner
is trying it (`HURT_SHARE`) is dropped and marked `hurt while trying`:
like an approach failure, remote planning does not head back for it, and
the room's other pickups wait until the inventory changes, so the bot does
not walk back into what hurt it. The preferred weapon of a `<fight>` in the
room still to be won is never dropped this way (`BotPlanner.preferred_here`):
taking it is how the fight ends (Brad's rocket launcher, with the Tyrant on
the bot).

Approach points sit outside each side of a zone, at a quarter, a half and
three quarters of the action reach, and a quarter reach inside the edge. On
a long side there are also points a quarter of the way in from each end. On
a short side (`BotNav.side_offsets`, under two approaches long) they sit a
quarter approach in from each end. A narrow zone beside a wall has its
middle inside the wall's grown box, so only its open end can be stood at:
102's broken shotgun (x 6500..7000) is taken from x 6575.
Points the path planner reaches come first, by path length; then up to 6
unplanned points by distance, each with a short walk budget. A point where
a press got nothing (`nothing on approach N`) marks its side of the zone
(north, south, east, west or inside) dead, by room and zone, and a later
job on the same zone tries that side's points last: 401's drawer answers
from one side only, and each visit used to spend its whole time budget on
points along the same dead side.

The path planner searches a 512-unit lattice first. When that finds no path
(a corridor narrower than the lattice rows, like the 10C bed passage), it
falls back to a 128-unit lattice whose blocked cells and connected parts
are cached per room and body. Both test every node and every edge against
the room collision with the player's radius, so neither cuts through a wall
thinner than its step; the fallback rejects a goal inside the collision or
in another part of the room. A start inside the body margin (a stair warp
can leave the player there) steps to the nearest free node first. A goal
no node sees (the end of a dead end narrower than the step, like the strip
beside 406's crate) is reached through one bend: the planner walks out
from it along an axis, up to 4096 units, to the first point a node sees.

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
`crawl_reach`, `grabs`, `outrun`, `speed`, `health`, the optional `parts`
and `prefer`, and one `<keep>` band per weapon. A band's optional `spare`
is the rounds of that weapon (every loading) it leaves held: a reserve gun
opens the fight only with more (a zombie gets one grenade only while six
more are held). A reserve gun also keeps, from any monster but a met
boss's own, the rounds the met bosses still to be won need (`save`, else
the kill) and a general reserve for bosses not met yet: four rounds at
full health, fewer as health drops (`BotTactics.saved_rounds`). Every entry has a comment saying how it was
measured. `parts="yes"` marks a monster made of several entities (Plant 42's
body and vines): a hit holds only the part it struck, so the bot fires on
while its target is still reacting instead of waiting for the reaction to
end. A `<fight>` step's target is preferred over the nearest monster
(`BotTactics.focus`, cleared on a room change).
`prefer="0x03"` names the weapon to fight that monster with whenever it is
held with the rounds for a kill, over the equipped gun (`BotTactics.pick_gun`;
the hunter takes the shotgun). A fight against a monster closing in that
cannot be outrun is not given up for low health.
While the preferred weapon lies in the room as a pickup, the monster is no
target: combat stops (`its preferred weapon lies here: take it first`) and
the `<fight>` goal ends, so the item frontier takes it (303's Tyrant stays
at 200 health until Brad's rocket launcher is fired).
`open="0x07"` names the weapon that opens the fight while the `prefer`red
one is not held: any rounds it has go first (`BotTactics.pick_gun`), and
the loadout takes it instead of a preferred weapon neither held nor boxed
(303's Tyrant: four grenade hits bring it to the 200 health where Brad
drops the rocket launcher). It needs a `<keep>` band.
`staple="yes"` on a `<weapon>` marks the firearm whose ammunition is the
most common (the handgun): a full inventory never boxes it.
`melee="yes"` on a `<weapon>` marks a hand weapon (the knife): it has no
rounds and never runs dry, and it is held only for a monster that `prefer`s
it (the web over 30C's door). Such a monster is fought only as a `<fight>`
step's target, and only inside its band: the step waits until the weapon
is held (the item frontier takes it first), walks to a reachable spot
inside the band, and the combat layer swings from there. A hit reaction
(player state 2) drops the aim and any reload; the bot waits it out and
starts them again. An empty clip with its ammunition held is reloaded in
the inventory menu first (a MOVE of the ammunition onto the gun; the menu
stops the game, so a hunter swiping about every 50 ticks cannot keep
resetting the 40-tick reload), at most once every 4 seconds; the aim and
fire reload is the fallback.
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
  the monster's health) times the risk margin below while another gun's
  cover that. The gun that covers the kill in the fewest shots is then
  equipped, so the shotgun's shells go to a fight the handgun cannot
  finish; with none covering the margin the plain pick stands if it covers
  the bare kill. A plain pick with no `<keep>` damage against a known
  monster (the grenade launcher equipped for a boss, meeting a hunter)
  gives way to a gun whose damage covers it. A `reserve="yes"` weapon (the
  grenade and rocket launchers) is kept for the fights that prefer it: it
  is picked only when no other gun covers even the bare kill, so hunters do
  not spend the rounds the Tyrant needs.
- **Fight or run** (`BotTactics.decide`). Run when the monster has no entry,
  its policy is not `fight`, or there is no gun, or the gun has no `<keep>`
  band while the entry has bands for others (`no band for this gun`). A monster closing in
  (within twice its reach, or within the far edge of the gun's `<keep>`
  band) that cannot be outrun is fought with any rounds left: a zombie
  closes about as fast as the handgun brings it down, so the fight has to
  open at the band's edge to end before its grab. Otherwise run below
  `max_health * (0.45 - 0.3 * risk)`, except from a monster slower than
  the backward walk (`speed` under `BACK_SPEED`) that the rounds cover
  with the margin below: that one is shot from range, since running past
  a zombie in a corridor ends in its grab. Run with too few rounds for it
  (`(health // damage + 1) * (1 + 0.5 * (1 - risk))`: a monster dies below
  zero health, so a hit that leaves it at exactly 0 does not kill) or for the
  crowd; fight a monster that cannot be outrun or one in a room the
  frontier crosses again (not while sourcing ammunition); run past one that
  is only in the way. A monster that already hurt the bot from within twice
  its reach (`BotTactics.note_hurt`, by room and slot, kept across reloads:
  a zombie by a door grabs on every pass) needs only the bare kill's
  rounds, no margin, and is fought (`it hurt the bot on an earlier
  pass`).
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

Below half health with no heal held (`BotSurvival.needs_heal`), a heal
pickup or a box heal ranks `HEAL_BONUS` (a cure fetch less one key)
cheaper, and the pursued target is dropped once per hurt episode
(`target dropped: hurt with no heal held`). Each monster that hurt the
bot in a room a route enters (`BotTactics.hurt_count`, at most 3 a room)
then adds `HURT_ROOM_COST` (two door steps), so the way to the heal goes
round a room where a cerberus already bit. A route into a room whose
`<fight>` is still ahead while the bot is not ready for it
(`BotSurvival.fight_ready`) adds `BOSS_ROOM_COST` (a cure fetch's bonus),
so it is taken only with no other way left: 30C's Black Tiger killed a bot
that walked in unvisited at 5 health.

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
  a `<use>` fact done, or a puzzle goal the planner finished, counted in
  the memory so a reload restores it with the save: `BotSave.milestones`) and the last save is at least
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
- Soft deposits, when a slot is still short: heals beyond the first stack
  (only for a key, a `<use>` item or the loadout, never for ammunition
  alone: a first aid spray boxed for acid rounds left the bot one herb
  for Yawn 2), keys no remembered door needs, then held melee weapons that are not
  equipped and that no fight in this stage prefers (the knife goes in
  until 30C's stage, where the loadout takes it back out). Only to make room for a key
  or `<use>` item (one on the frontier, or a remembered box item a `<use>`
  fact or a locked door needs back) do spare guns go too, fewest rounds
  first, the `staple` gun (the handgun, whose ammunition is the most
  common) after every other, never one the loadout or a `<use>` needs
  (408's guardhouse key found the slots full of the equipped shotgun's
  kit, the loadout's launcher and the handgun). The rearm brings the
  handgun back once its clips are held.
- Heal to make room (`BotInventory.heal_for_room`, `use heal 0xNN to make
  room`): every slot used, a known pickup in this room the run needs (a
  key, a `<use>` item, or the ammunition of a gun held or boxed) that
  merges into no held stack, at least two heal stacks held (a cure is
  none), and at least `MAKE_ROOM_HURT` (24) health missing: the largest
  heal stack is used, as `BotSurvival` heals. 219 left twelve grenade
  rounds behind at 56/96 health with four heal stacks held.
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
  twice the reserve floor, and every heal stack, as long as one slot stays
  free (the top-ups; no trip is made for them).
- Loadout: the `prefer` weapon (a hand weapon too: 30C's knife) of each
  pending `<fight>` the bot has met (`BotNav.fight_met`: it entered the
  fight's room and fought or held back, or saw the boss there), and its
  ammunition, are kept, and withdrawn first into free slots, from any box:
  a trip is made for them, and a full inventory makes room for them the
  way it does for a needed pickup (`loadout: a fight ahead prefers it`). A
  boss not met yet asks nothing: the bot does not peek at what is ahead.
  Meeting one is remembered past a reload (a player who died to it knows it
  is there). Another loading of the preferred launcher keeps its rounds
  while the preferred one runs short, or always when its band hits the
  enemy harder (flame on the Black Tiger). With the preferred weapon
  neither held nor boxed, a held or boxed gun with a band for the enemy
  stands in. A met fight is not ready while the box holds its preferred
  weapon (or, with no held gun banded for it, any banded one), and a met
  fight still to be won makes the boxed heal worth a trip. A fight
  step whose preferred weapon is not held waits while a frontier pickup of
  it is in the room (`BotPuzzle.preferred_pickup`), so it is taken first.
- Cure: poisoned with no cure for that poison held, the `<cure>` items
  for it come out of the box the way the loadout does (`a cure for the
  poison`), and the trip to a remote box for them is weighted like one for
  a heal (`HEAL_BONUS`), and so is a route to a frontier pickup of one: a
  run walked 17k ticks poisoned by 408's wasps past 304's blue herb.
- Rearm: a firearm in the remembered box (not a `reserve` one) comes out
  with its ammunition, the way the loadout does
  (`rearm: a boxed gun's ammunition is at hand`), when that ammunition is
  held (it then stays held rather than going in after the gun), or when
  it is in the box while sourcing ammunition
  ([Ammunition](#ammunition)). So the shotgun put in to make room for a
  key comes back once shells are picked up, and the shotgun put away
  empty comes back with its boxed shells when the handgun runs dry. The
  rearm counts the slots it takes (the gun, and its ammunition when
  boxed) and waits while a key or `<use>` pickup needs the free slots;
  otherwise the swap would put the gun straight back.
- Combine first, when every slot is used and a pickup needs room: the best
  pair of heal, ammo or weapon stacks that `bot:combine_result` says the
  engine applies and that empties a slot, heals first (rank 1, guns and
  ammo rank 2), never an item a `<use>` fact needs, and never a weapon
  with ammunition other than its own (`bot:ammo_for`): that turns it
  into another item (the launcher with acid rounds is `0x08`), which the
  knowledge, the loadout and the rearm do not know. Mixing herbs keeps the
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
| `bot:scene()` | `{mode, skippable, film, transition, cutscene_skip, message, message_takes_input, message_waiting, prompt, prompt_choice, pickup, reader, frozen, controls_locked, controllable, dead, ending, typewriter, save_screen, computer}`. `ending` is the scripted death variant, the escape scene ending the game (it raises `dead` too); the bot then finishes with `game completed`. `computer` (only while the lab terminal is up) is `{stage, sub, cell, menu_row, typed}`: the screen's state (`login`/`menu`/`command`/`unlocked`), its field (`user`/`secret`), the keyboard cursor cell, the door menu row and how many characters the field holds. `prompt` is a yes/no prompt on screen and `prompt_choice` its highlighted answer (`"yes"`/`"no"`; left selects No, right Yes). `reader` is a document pick-up's FILE reader, which closes on cancel. `typewriter` is the fade into or out of the typewriter's save screen; `save_screen` (only while it is up) is `{mode, stage, cursor, exit_row, slots, rows, answer}`: `mode` `"save"`, `stage` `idle`/`confirm_overwrite`/`save_anim`/`error`/`fade_out`, `cursor` the highlighted row (0-based; rows `0..7` are the slots), `slots` whether each slot holds a save. The modern screen also sets `rows`, the 0-based slot each row writes or loads (`-1` for none; save mode's first row is the new save, writing the first free slot), and `answer`, the open question's highlighted answer (`"yes"`/`"no"`; left selects Yes); `cursor` is `-1` while no row is highlighted. |
| `bot:actions()` | Array of `{slot, kind, x, z, width, depth, item, quantity, next_room, key, lock, handler, flags}`. An item AOT whose flags lack the player bit (`0x01`) cannot be picked up yet (a pickup a room event arms later): its `kind` is `item_dormant` and `item`/`quantity` are `nil`. |
| `bot:doors()` | Array of `{slot, target, next_room, key, lock, x, z, width, depth}`; `target` is the destination room id. |
| `bot:objects()` | The room's active objects (blocks, statues, shelves): `{slot, x, z, half_x, half_z, pushable}`, `x`/`z` the live centre and `half_*` the collision half-extents. `pushable` is solid and without the not-pushable flag. The walk and path planner treat every solid object as an obstacle, a box 128 units inside the object on each side, rebuilt when an object moves. |
| `bot:enemies()` | Array of active monsters (`id < 0x20`, `health > 0`), nearest first: `{slot, id, x, y, z, angle, bearing, health, state, status_flags, action_behavior, action_state, hit_state, distance, visible}`. `bearing` is the absolute 12-bit angle from the player; `distance` is horizontal; `visible` is the weapon line-of-sight result. Boss ids are included. |
| `bot:weapon()` | `{equipped, weapon, clip, reserve, ammo, behavior, state, aim_state, target}`. `equipped` is the inventory item (or nil), `weapon` is `equipped_weapon_id`, `clip` is the rounds in the equipped weapon's stack, `reserve` is `item_count(weapon + 9)`, `ammo` is `bot:ammo_for(weapon)`, and `target` is the locked entity slot or nil. |
| `bot:knowledge()` | The game's combat knowledge, read-only: `{source, enemies, weapons, cures}`. `enemies[id]` is `{name, policy, stand, downed, up, reach, crawl_reach, grabs, outrun, speed, health, parts, keep}` with `keep[weapon] = {min, max, damage}`; `weapons[id]` is `{name, near, side, cadence, headshot, weak, status_bit, melee}`. `cures` is an array of `{item, poison, heals}` from `<cure>` rows, in file order: `poison` is `"poison"` or `"serious"` and `heals` whether the item also restores health. An `unknown` value is `nil`. With no file, the tables are empty. See [Combat](#combat). |
| `bot:spacing_point(slot, want)` | A point `want` from monster `slot` that is free of collision, at least `want` from every other monster, in a clear line from the player and not reached by walking past the monster: `{x, z}` (the nearest such point) or `nil`. |
| `bot:can_see(slot)` | Whether monster `slot` is in the current snapshot with a clear weapon sight line. |
| `bot:flag(bank, bit)` | A flag bit, read-only. |
| `bot:has_item(item)` / `bot:item_count(item)` | Inventory queries. |
| `bot:inventory()` | Read-only array of the live inventory slots: `{slot, item, quantity, kind}` rows, including zero-quantity weapon slots. `kind` is the catalog kind. |
| `bot:item_kind(item)` | The item catalog's kind (`"weapon"`, `"ammo"`, `"key"`, `"heal"`, ...) and whether a pickup merges into a held stack (two values). |
| `bot:ammo_for(weapon)` | The ammunition item `weapon` fires, or `nil` when the catalog does not list it as a weapon with an ammunition item. |
| `bot:use_facts()` | Every room's `<use>` facts from the pack's `room/<id>/bot.xml` files, by room id then document order: `{room, slot, item, bank, bit, label}`. Read once per pack. |
| `bot:item_facts()` | Every room's `<holds>` facts, the same way: `{room, item, label}`. |
| `bot:fight_facts()` | Every room's `<fight>` steps, the same way: `{room, enemy, bank, bit}` (the flag its death sets, or nil). |
| `bot:ride_facts()` | Every room's `<ride>` facts: `{room, slot, from, label}`. |
| `bot:door_facts()` | Every room's `<door>` facts: `{room, slot, via, label}`, plus `gate_bank`, `gate_bit` and `gate_set` for a gated one. |
| `bot:avoid_facts()` | Every room's `<avoid>` facts: `{room, slot, reason}`, plus `bank`, `bit` and `while` for a gated one and `unless_held`. |
| `bot:errand_facts()` | Every room's `<deliver>` and `<fetch>` facts, the same way: `{room, kind, cut, slot, item, bank, bit, after_bank, after_bit, label}`, `kind` `"deliver"` (with `cut`) or `"fetch"` (with `slot`); `after_bank`/`after_bit` are `nil` without an `after` flag. |
| `bot:menu()` | The pause menu as the engine shows it, read-only: `{open, requested, phase, accepts_input, screen, cursor, slot, mode, option, options, selected_item, ready, closing, viewing, capacity, equipped, items}`. `phase` is `closed`, `requested`, `fade_out`, `fade_in`, `opening`, `closing`, `screen`, `message` or `input`; `screen` names a sub-screen (`pickup`, `examine`, `item_box`, `map`, `file`); `cursor` is the menu's cursor byte (`0..6` tabs, `8 + 2 * slot` items); `mode` is `navigation`, `actions` or `move`; `option` is the highlighted submenu row (0-based) and `options` its labels (`EQUIP`/`USE`, `CHECK`, `MOVE`). `items` rows are `{slot, item, quantity, kind, heals}` with the menu's 0-based `slot`; `heals` is the catalog's word that a USE restores health (false for a blue herb, the serum or a red herb alone). `move_cursor` is the MOVE cursor byte while `mode == "move"`. While the item box screen is up, `box = {mode, box_cursor, cursor, sliding, items}`: `mode` is `browse` or `armed`, `box_cursor` the box slot (0..47), `cursor` the inventory cursor byte, `sliding` true while a page slides (no input is read), and `items` the box rows `{slot, item, quantity, kind}`. While the CHECK viewer is up, `examine = {item, yaw, ready}`: the item on the turntable, its yaw (`0..0x1000`, left raises it) and whether the turntable takes the d-pad and confirm. |
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
| `<interact>` | `slot`, `bank`, `bit`, optional `done`, optional `after_bank` + `after_bit` (and `after_while="clear"`), optional `reveals`, optional `opens`, optional `label` | Press AOT `slot`; flag `bank`/`bit` set means done. A body-probe zone (flag `0x40`, which fires on the player's position) is stood in instead of pressed. With `done="clear"` the flag clearing means done instead: bank 7's item bits are set while the item is in the room and clear when it is taken. With an `after` flag the step is offered only while that flag is set; until then the sequence holds and no later step is offered (a `<use>` may set it). With `after_while="clear"` the step waits for that flag to clear instead: 508's reader lights a lamp only once its pass code's file (a bank-7 item bit) is taken. With `reveals`, the step is also done once that AOT slot is a live item or event zone, which the item frontier or the next step then takes. With `opens`, it is also done once that AOT slot is a live door: an event that turns a door back from a message and sets no flag (513's Barry branch). |
| `<fight>` | `enemy`, `bank`, `bit`, optional `after_bank` + `after_bit` (and `after_while="clear"`), optional `stage`, optional `label` | With an `after` flag the step holds the sequence as for `<interact>`. With `stage` (one room-id character), the box loadout readies the fight's weapon from that stage too (303's Tyrant, reached by the lab's escape lift). Stay in the room and fight entity id `enemy` until flag `bank`/`bit` is set: walk to 4500 units from the part with the most health (for an enemy with `parts="yes"`, the weakest live part: Plant 42's vines before its body), face it and let the combat layer shoot (5-minute timeout). One step of the `<interact>` sequence, for a boss that never comes to the player. An enemy fought with a hand weapon (`melee`) holds the sequence until that weapon is held, and is walked up to inside the weapon's band instead. |
| `<wait>` | `bank`, `bit`, optional `after_bank` + `after_bit`, optional `label` | Stand still in the room until flag `bank`/`bit` is set: a timed room event that must play out before the player leaves (1-minute timeout). With an `after` flag it is offered only once that flag is set. One step of the `<interact>` sequence. |
| `<camera>` | `cut`, optional `bank`+`bit`, optional `label` | Walk into a camera switch zone that cuts to camera `cut` (a room event keyed on the camera). With a flag, it is done once the flag is set; without one, once the cut is entered on this room entry. It is one step of the same sequence as `<interact>`. |
| `<answer>` | `choice` (`yes` or `no`), `slot` and/or `bank`+`bit`, optional `label` | At the yes/no prompt opened by AOT `slot` (or identified by the flag), choose `choice`. At least one of `slot` or the flag is required; `bank` and `bit` go together. |
| `<avoid>` | `slot`, `reason`, optional `bank`+`bit`+`while`, optional `unless_held` | The planner should not use AOT `slot`; `reason` is a non-empty note. With `bank`, `bit` and `while` (`set` or `clear`), only while that flag is in that state; remote planning reads every room's avoids (`bot:avoid_facts()`) and never heads for a door one holds shut. With `unless_held`, not while that item is held (a swap: lift one item only while holding the one that goes in its place). |
| `<push>` | `object`, `slot`, optional `direction` (and `clear="full"` or `until="stop"`) or `axis`, optional `bank` + `bit`, optional `after_bank` + `after_bit`, optional `reveals`, optional `label` | Push room object `object` (a `bot:objects()` slot) until its centre is in the middle half of AOT `slot`'s zone, usually an object-position probe. It is one step of the `<interact>` sequence, done while the object stays there. The push is cardinal: one axis at a time, standing behind the object (600, then 450, 400 and 300 units behind its face; on the face's middle, then a third of its half size to either side) and holding forward; an axis that stalls twice is given up for the other. With `direction` (`north` = +z, `south`, `east` = +x, `west`) the object is pushed off the zone instead: that way only, until its trailing face passes the zone's far quarter, or with `clear="full"` its far edge (an object pushed out of another one's way). With `until="stop"` it is pushed that way until it stalls against a wall, whatever the zone. Without a direction, `axis` (`x` or `z`) lines the centre up with the zone on that axis only. Such a step is also done once the zone is gone or the item it names was taken. With `bank` and `bit` (both or neither) the step is also done once that flag is set: a zone event that takes the object away (a statue that falls). A push the bot stops for a fight or a heal walks back behind the object before it measures a stall. With an `after` flag the step holds the sequence until that flag is set, as for `<interact>` (306's box waits for the crank). With `reveals` (the item AOT slot the puzzle makes live), a full inventory holds the step until a slot is free (306's box resets once the slab is cranked). |
| `<use>` | `slot`, `item`, `bank`, `bit`, optional `done`, optional `after_bank` + `after_bit`, optional `until_bank` + `until_bit`, optional `label` | Use `item` (non-zero) from the menu while standing in AOT `slot`, a use-item probe; flag `bank`/`bit` set means done, or clear with `done="clear"`. With an `after` flag the use is open only while that flag is set, and with an `until` flag only while that flag is clear, so one item can take one zone twice (301's crank drains the pool, then fills it after Plant 42). A use whose menu closes by itself counts as done when the item stays held: the cranks are picked up with quantity 0 and never used up. A use that goes in but leaves its done flag clear is tried again at once, up to three times, for a piece that turns one step a use (30F's crank). |
| `<climb>` | `object`, `slot`, `bank`, `bit`, optional `done`, optional `label` | Take an item off a high shelf, or press an event zone only reachable from on top (507's air shaft): push room object `object` until it covers AOT `slot`'s zone (and has stalled against the wall at least once; an object an earlier `<push>` step of the entry left over the zone is climbed at once), stand behind its face away from the zone, press action to climb it, press facing the zone to take the item, then face away and press to climb down. A climb whose flag is not a bank-7 item bit needs no free inventory slot. The engine's vault only starts facing along the object's yaw, so the face away from the zone must be that one. It is one step of the `<interact>` sequence, done once flag `bank`/`bit` reads done (`done="clear"` for a bank-7 item bit) and the player is off the object. |
| `<watch>` | `bank`, `bit`, optional `after_bank` + `after_bit`, optional `label` | Let a skippable scene play instead of skipping it while flag `bank`/`bit` is clear and, if given, the `after` flag is set: a scene a skip breaks. |
| `<door>` | `slot`, `via` (another slot), optional `bank` + `bit` + `while`, optional `label` | Door AOT `slot` is parked at the room origin (or, with a gate, held elsewhere) until the player presses AOT `via`, whose event sets the door around the player. With a gate the fact holds only while flag `bank`/`bit` is `while` (`set` or `clear`), and the door is routed through `via` even when it is not at the origin. Routes may pass through it (any other door zone at the origin is a scripted transfer and never routed), and leaving by it presses `via`; the room's `<answer>` settles a prompt. The via goal is done once the room changes or the fact no longer holds (305's stairs event leaves the player beside the door). A body-probe `via` (flag `0x40`, as 302's elevator call) is walked into instead of pressed, and a `via` zone is never taken for a passage check. Read by `bot:door_facts()` for every room. |
| `<keep_out>` | `slot`, `reason`, optional `bank` + `bit` + `while` | Every walk plans around AOT `slot`'s zone as if it were a wall, while the gate holds (always without one): a floor probe that springs a trap (507's plate starts the gas). The engine reads it: the zone joins the path planner's object boxes. |
| `<leave>` | `slot`, `bank`, `bit`, optional `label` | Once flag `bank`/`bit` is set, the bot's next goal is door AOT `slot`, before the room's puzzle and pickups: a room event that keeps hurting until the room changes. |
| `<ride>` | `slot`, `from` (a room id), optional `label` | Door AOT `slot` sits at the room origin, a scripted transfer, but it is the way out when the room was entered from `from`: a lift whose event picks the far door by where the player came from. The route planner takes it only on a path that enters this room from `from`, and the room's own event then carries the player through. Read by `bot:ride_facts()`. |
| `<holds>` | `item` (non-zero), optional `label` | The room holds `item`: what is where, never when to go. Read by `bot:item_facts()` for every room. |
| `<deliver>` | `item` (non-zero), `cut`, `bank`, `bit`, optional `after_bank` + `after_bit`, optional `label` | Bring `item` here and walk into camera `cut`, where a room event takes it; flag `bank`/`bit` set means done. With an `after` flag, wanted only once it is set. Read by `bot:errand_facts()` for every room. |
| `<fetch>` | `item` (non-zero), `slot`, `bank`, `bit`, optional `label` | Press AOT `slot` to take `item`, offered while flag `bank`/`bit` is set; fetched only while a `<deliver>` of the same item is wanted, or while the item is a knowledge `<cure>` for a poison the player has and no such cure is held (100's second serum after Yawn's bite). Read by `bot:errand_facts()` for every room. |

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

`bot:room_meta()` always has `interacts`, `uses`, `answers`, `avoids`,
`watches` and `leaves` arrays (empty for a format-1 file); `leaves` rows are
`{slot, bank, bit, label}`. The others: `{kind = "press", slot, bank, bit,
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
| 10C | `<use>` the herbicide (`0x26`) at slot 2, the pump; done flag bank 1 `0xA9`. `<avoid slot="4" bank="1" bit="0xA9" while="clear">`: the plant fountains stand across the way to the armor key until the herbicide, and `<avoid>` slots 5..10 the same for the six herbs in their reach. |
| 102 | `<holds item="0x1C">`: the broken shotgun lies here (the room's door from 101 takes the armor key). |
| 210 | `<interact slot="4" bank="7" bit="0x81" done="clear">`: the moon crest has no player bit; the nest event (AOT 4) hands it over. Yawn (`0x0D`) comes for the bot while it does; it is passed (`policy="pass"`): a handgun fight cost the same 82 health as passing it, and 9 rounds. |
| 202 | `<push object="0" slot="3" bank="0" bit="0x0B">`: the statue along the balcony until it falls; the fall sets scenario `0x0B` and puts the blue jewel (`0x21`) in 105. |
| 10D | `<use>` the blue jewel (`0x21`) at slot 2, the tiger's eye; done flag bank 0 `0x0F` (the wind crest pickup arms). `<use>` the red jewel (`0x22`) at slot 2; done flag bank 0 `0x0E` (the magnum). |
| 10F | `<push object="1" slot="5" direction="east">`: the bookshelf off the music score's pickup. `<use>` the music score (`0x23`) at slot 13, the piano; done flag bank 1 `0xA5` (the bar counter moves off the gold emblem). `<use>` the wood emblem (`0x1F`) at slot 6, the hollow; done flag bank 1 `0xA7`. |
| 11A | Four `<use>` crests at slots 5, 3, 4 and 6 (star `0x2D`, sun `0x2E`, moon `0x2C`, wind `0x29`); done flags bank 0 `0x6A`, `0x69`, `0x6B`, `0x6C`. |
| 106 | `<camera cut="4">`, then `<camera cut="2" bank="1" bit="0x05">`: Barry's hall scene plays out as Jill walks into the north cut, then the middle cut; the flag marks it done and the east-wing door is restored. |
| 116 | `<avoid slot="1" unless_held="0x1C">`: lifting the shotgun without the broken one springs the 115 ceiling trap, so the rack is used only while the broken shotgun is held. `<use>` the broken shotgun (`0x1C`) at slot 5, the rack's probe (armed once the shotgun is lifted); done flag bank 7 `0x01` (the broken one hangs on the rack; room main also clears common `0x20`). |
| 11B | `<climb object="0" slot="3" bank="7" bit="35" done="clear">`: the square crank lies on a shelf over the crate's corner; room main keeps its pickup an item only while the player stands on the crate (`FG_STATUS` 24 clear). |
| 119 | Second mansion visit only. `<interact slot="2" bank="7" bit="67" done="clear" reveals="1">` with `<answer choice="yes" slot="2">`: the init sets room bit 10 on every entry, which hides Doom Book I (AOT 1) and the magnum rounds (AOT 4); the switch's Yes clears it and both become items. A doom book taken out of the item box has quantity 0, so it counts as held and is opened anyway. The planner then opens the book in the menu's CHECK viewer (a held doom book always gets a `BotMenu.examine` job), which turns it into the Eagle Medal (`0x25`). |
| 117 | Seven `<interact>` paintings in picture order; each push keeps only its own bank-4 bit, the last sets scenario `0x03`. |
| 300 | `<door slot="1" via="5">` while scenario `0x31` is set: the north elevator, parked at the room origin, rides down to 302 from AOT 5 on its platform; without it the shaft is walled off. |
| 301 | `<use>` the square crank (`0x1D`) at slot 4; done flag scenario `0x2F`, which the drain event sets. `<watch>` the same flag: the drain scene is not skipped. Using a crank from the menu leaves the player in the crank turn, whose completion the event waits on. The drained pool is crossed by its two ladders (AOT 2 and 3, passage checks) to the door to 302. `<avoid slot="7" bank="0" bit="0x2F" while="set">`: the elevator call back down to 302 on the landing the 302 door brings you to. Once the north elevator stands at 300 (scenario `0x31`, after 302's battery) a second `<use>` of the crank (`done="clear"`, `after`) fills the pool again, which stops 302's waterfall; with the pool full `<door slot="1" via="7">` takes the elevator down to 302. |
| 302 | `<avoid slot="7">`: the elevator call on the platform the 301 door lands on; it rides straight back up to 301. `<avoid slot="0" bank="0" bit="0x32" while="clear">`: door 0 to 300 is the north elevator's far end, called only with the power on (scenario `0x32`). `<use>` the battery (`0x27`) at slot 4 turns the power on (scenario `0x32`), and `<door slot="0" via="8">` (while scenario `0x31` is clear) rides the north elevator up to 300. `<door slot="3" via="10">` while scenario `0x2F` is clear: with the pool full the waterfall is gone and AOT 10 at the ladder asks to climb down to 307 (`<answer>` Yes). |
| 305 | `<use slot="6" item="0x24" bank="0" bit="28">` and `<use slot="5" item="0x25" bank="0" bit="27">`: the Wolf and Eagle medals into the fountain; with both in, the main drains it (scenario 29). `<door slot="2" via="14">`: the passage to 30F, a door parked at the origin that the AOT 14 event at the west end fires. `<door slot="1" via="9" bank="4" bit="29" while="clear">`: once drained, the stairs zone AOT 9 (a touch event: the reach point fires it, no press needed) runs event_00 down to the fountain floor, which sets room 29; the via goal is then done and door 1 to 310 is an ordinary door. |
| 310 | No facts. The lift: entered from 305, event_01 rides down and turns on door 1 to 500 at the room origin, which takes the player through by itself; the bot waits for it (step 7). |
| 306 | Doom Book II (AOT 6) is live only while box object 2 stands in object probe AOT 5. The box starts against the north wall, so: `<push slot="5" axis="x">` puts it under the wall slab, `<use slot="1" item="0x1E" bank="0" bit="60">` cranks the slab out and it shoves the box off the wall, then `<push slot="7" axis="x">` and `<push slot="5">`, both `after` scenario 60, take it west of the slab, south and east into AOT 5. The book is taken from the floor and opened for its medal. |
| 307 | `<use>` the hex crank (`0x1E`) at slot 4 turns the bridge (common `0x47`), which opens door 2 to 30B. In the BioRand pack slot 3 is a handover item at the room origin. |
| 30A | `<interact slot="2">` (Jill): the body probe in front of Enrico runs his scene (common `0x48`), which hands over the hex crank. |
| 30B | `<interact slot="10" reveals="5">`: the corridor's west end sets a room flag that the main clears at once, so the step is done when it arms AOT 5. `<interact slot="5">` then sets the boulder rolling (scenario `0x3D`), and the init opens the way to 30C. |
| 30C | `<fight enemy="0x13" bank="3" bit="138">`: the web over door 1 to 30D, cut with the knife picked up in the room (AOT 2). The init deletes the door while the web stands, and the main arms it again once enemy bit 138 is set. Before it, `<fight enemy="0x04" bank="3" bit="137" label="Black Tiger">`: the spider's knockback breaks the web cut, so it is killed first with the grenade launcher (its knowledge `prefer`). `<avoid slot="1" bank="3" bit="138" while="clear">`: the bot still saw door 1 while the web stood, walked into the web and stood stuck there until the tiger's poison killed it. |
| 30F | Two `<use slot="6" item="0x1E">` facts: the hex crank turns the passages one step a use, cycling scenario bits 74 and 75 through (0,0), (1,0), (0,1), (1,1). Only (1,1) joins the 30D door to door 1 (306). The first fact waits for bit 75, the second for bit 74 after 75; a use whose done flag stays clear is retried at once. `<door slot="0" via="3">`: the passage to 305 from the AOT 3 event at the east end. |
| 500 | `<avoid slot="1">`: the lift back up to 310. The lab needs nothing from before it, and an ungated `<avoid>` on a door is marked for good, so remote planning never routes through it either. |
| 504 | `<interact slot="7" bank="4" bit="0">`, then `<interact slot="7" bank="0" bit="102" reveals="1">`, each with `<answer choice="yes" slot="7">`: the switch lowers the first panel, then the one in front of the lab key (AOT 1). |
| 506 | Two `<interact slot="1">` facts with `<answer choice="yes" slot="1">`: the terminal releases lock 37 (503's door to 504), then lock 38 (508's door to 509). `BotTerminal` drives the screen. |
| 507 | The gas room. `<keep_out slot="8">` while room bit 3 is clear: the plate starts the gas, and the way to the box's north face runs past it. Three `<push object="2" slot="8">` steps: south and west `until="stop"` (crates take the box's east side), then north `axis="z"` onto the plate, done at room bit 36. `<climb object="2" slot="9" bank="4" bit="7">`: AOT 9, the air shaft, is armed only while the player stands on the box (main-state bit 24); its Yes crawls through to the west half. `<leave slot="0" bank="4" bit="7">`: the gas has set the fast health drain by then, which ends with the room. `<use slot="12" item="0x28">` on the next entry from 505: the MO disk prints pass code 02. |
| 508 | Four `<interact slot="4">` facts on scenario bits 52, 53, 54 and 80: one press enters every pass code taken so far. Each lamp step waits, with `after_while="clear"`, for its code's file (item bits 82, 98, 90), and the last opens door 3 to 50B. |
| 509, 510 | `<use slot="3" item="0x28" bank="7" bit="82">` (509) and `bit="90"` (510), `done="clear"`: the MO disk prints pass codes 01 and 03. The disk is used up, so the lab needs three. |
| 50C | `<interact slot="5" bank="0" bit="1" after_bank="0" after_bit="45">` with `<answer choice="yes" slot="5">`: the switch calls the elevator once the power is on, which turns door 4 to 515 from a message (129) into a door. |
| 50F | `<interact slot="2" bank="0" bit="46">` with `<answer choice="yes" slot="2">`: the power panel supplies the blacked-out areas. |
| 511 | `<interact slot="1" bank="0" bit="45" after_bank="0" after_bit="46">` with `<answer choice="yes" slot="1">`: the connection switch wires the elevator's circuit. It connects without 50F's supply too; the gate makes 45 mean both halves are on, so 50C waits for 45 only. |
| 513 | `<fight enemy="0x0C" bank="3" bit="112" label="the Tyrant">`, then one exit per branch of common bit `0x5B`. Clear (Wesker's branch): the Tyrant's death (enemy bit 112) shows the master key (AOT 2) on the floor, and `<interact slot="1" bank="4" bit="6" after_bank="4" after_bit="3" opens="0">` with `<answer choice="yes" slot="1">` has the control device turn door 0 from a message (159) back into a door; room bit 3 is set only on the entry where the Tyrant dies. Set (Barry's branch): door 0 is message 131 and AOT 1 message 160 for the whole entry, and the death arms AOT 3 as Barry's event, whose end makes door 0 a door and sets no flag: `<interact slot="3" bank="1" bit="0x5B" done="clear" opens="0">`, ahead of the control device. The bot fights the Tyrant with the grenade launcher (`prefer`), which the loadout rule takes from 50E's box on the way. |
| 515 | Two `<ride>` facts: the lift's doors are both at the origin. Entered from 514 it rides up to 50C (slot 0); entered from 50C, down to 514 (slot 1). |
| 501 | `<use slot="4" item="0x27" bank="1" bit="0x46">`: the battery (from 50A) powers the escape lift; its scene starts the 180-second self-destruct countdown. `<avoid slot="5">`: the pulled-out battery's message. |
| 303 | `<use slot="6" item="0x2A" bank="4" bit="0">`: the flare (AOT 2) used on the deck calls the helicopter. Without scenario `0x2B` (the partner's self-destruct scenes in 505/514) the helicopter takes the player away and the game ends; with it, `<fight enemy="0x10" bank="3" bit="0x59" after_bank="4" after_bit="0" stage="5">`: the Tyrant comes up; the grenade launcher (the knowledge `open`, carried up from the lab by the loadout) takes it to 200 health, Brad drops the rocket launcher (AOT 1, the knowledge `prefer`, taken while combat holds off), and its death starts the ending. The doors are locked behind the flare, so the fight is fought even when not ready (`fighting anyway`). |
| 402 | `<interact slot="3" bank="0" bit="0x43">`: pulling the bath plug makes AOT 1 the key `0x3B` (to 411). |
| 406 | `<push object="1" slot="5" direction="north" clear="full">` (the west crate deep into the recess behind it), `<push object="0" slot="5" direction="west">` (the east crate off the hatch, pushed from the 335-unit strip beside it), then `<interact slot="5" bank="0" bit="0x45">` and `<answer choice="yes" slot="5">`: Yes opens the ladder door down to 40D, whose init sets scenario `0x45`. `<door slot="2" via="5">`: on later entries door 2 is parked at the origin again, and the route goes down by pressing the hatch. |
| 408 | `<avoid slot="1">`: the door to 409 is reset to message 143 on every entry (and deleted once scenario `0x21` is set), so it is never retried; each retry crossed the room's wasps. |
| 40D | Six `<push>` steps make the bridge over the water to 40E. Objects 3 and 1 go `direction="south"` into AOT 2's edge zone (done flags scenario `0x39`, then `0x38`; the second clears the first). Object 2 stands against the room's north wall: `axis="x"` onto the ladder corridor's AOT 10, `south` `until="stop"`, `east` `until="stop"`, then `south` into the water. Bit `0x39` is set again only by the third crate. |
| 40A | `<interact slot="5" bank="7" bit="0x4E" done="clear">` with `<answer choice="yes" slot="5">`: the bookshelf hands over document `0x69`. `<use>` the red book (`0x3E`, from 401) at slot 5, done flag scenario `0x24`: the bookcase slides aside (scenario `0x25`). Then `<interact slot="7" bank="0" bit="0x44" after_bank="0" after_bit="0x25">`: the gap behind it turns on door 2 to 40C, whose init sets scenario `0x44`. |
| 40C | `<fight enemy="0x08" bank="3" bit="96">`: Plant 42 (Barry's branch), its body in slot 1 with five vines. Once the vines are shot Barry's event burns it, and its death sets enemy bit 96. Then `<interact slot="2" bank="7" bit="0x58" done="clear" after_bank="3" after_bit="96" reveals="3">`: AOT 2 at the fireplace is a body probe that raises system bit 62, and the main script turns AOT 3 into the helmet key while that bit and enemy bit 96 are set. |
| 411 | `<interact slot="1" bank="0" bit="0x23">` with `<answer choice="yes" slot="1">`: the drain switch empties the aqua ring (40E/40F/411 load dry). Then `<interact slot="2" bank="2" bit="36">` with `<answer choice="yes" slot="2">`: the lock switch, which turns 40E's door 2 to 410 into a door. |
| 205 | `<push>` armor object 1 onto vent slot 1 and object 2 onto vent slot 2, then `<interact slot="3" bank="0" bit="0x3A">` (the switch) and `<answer choice="yes" slot="3">`. With both vents covered Yes opens the sun crest (item slot 9); with one open it releases the gas. |
| 20D | Jill: `<deliver item="0x42" cut="3" bank="1" bit="0x61" after_bank="1" after_bit="0x62">`: poisoned Richard waits (`0x62`) for the serum; walking into camera 3 with it hands it over and sets `0x61` (so does his death after 420 seconds). Until then the door to 20E is a message. |
| 100 | Jill: `<fetch slot="5" item="0x42" bank="1" bit="0x23">`: the serum on the shelf, offered once Richard's scene sets `0x23`. A second `<fetch>` with bit `0x24`: the shelf's other serum, which 20E's collapse scene offers after Yawn's bite; the bot takes it as its cure. |
| 20C | Jill: `<interact slot="2" bank="0" bit="0x27">` plays Yawn's entrance, then `<fight enemy="0x12">` Yawn 2 (enemy bit `0x3E`), whose death arms the hole (AOT 3); `<interact slot="3">` there (common `0x26`) drops Jill into the pit, and `<wait bank="1" bit="0x25">` holds until Barry comes back with the rope 600 frames later and sets the pass number. Leaving first by the ladder loses the timer. |
| 201 | Jill: `<interact slot="5" bank="0" bit="0x46">` after common `0x25`: the key panel shares door 1's zone, and room init switches the door to 214 off until the pass number is entered. |
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
