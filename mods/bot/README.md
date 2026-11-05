# Player bot

A restricted Lua route that drives a normal new Jill game through the same pad
a human would use. It cannot set flags, items or positions. Its only legal
state-changing actions are normal input, the room's own item/stair actions, an
equip request that the host applies like the inventory menu, and a heal request
that runs the menu USE rule and consumes the item.

```sh
# Watch the bot in the normal window (bare --bot implies --ui game --character 1)
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot

# Edit bot/**.lua while the route runs: --watch re-executes the chunks in the
# same Lua state, so BOT_STATE and the engine's bot memory keep their progress.
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot --watch

# Headless: runs as fast as the simulation allows and reports ticks/rooms/reason
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot --bot-headless
```

`--watch` reloads the route in place when `bot/**.lua` changes: the chunks
re-run in the same Lua state, so `BOT_STATE` keeps its phase and the engine
keeps its remembered memory, navigation and tick count. Editing a route file
does not restart the room or the run. A new game or restart rebuilds the bot
VM from the pack, so route globals start fresh there. A changed
`room/<id>/bot.xml` invalidates the cached room metadata, which the route sees
through `bot:room_meta()` on the next tick.

## Route

The route starts a fresh Jill game. It used to stop when it entered room 10A;
it now continues through 117, 11A, the east-wing return, the 203 stair landing
and 10C. The 10C greenhouse's six plant fountains stunlock the player in the
only bed crossing, so the route attempts the reachable green herbs, tries the
armor-key corridor, and then retreats to 103 with a precise item/health report
instead of dying:

1. **106 opening.** Skip the live-action intro and the main-hall scene. The
   shipped opening ends by scripting Jill through the 106 -> 105 walk-in door.
   If a build leaves Jill controllable in 106 instead, the route walks to the
   106 -> 105 door at `(3300, 16900)` and opens it.
2. **105 dining room.** Walk to `(14000, 13000)`, south of the OBJ_02
   sideboard, then to `(6700, 13700)`. The AOT_02 event zone lies across the
   second leg, so the first Barry scene starts on the way; skip it. The scene
   moves Jill to the south-west, after which the same goal is a straight north
   walk to the 105 -> 104 door (AOT_00), leaving her facing it.
3. **104 tea room.** Walk west to `(4300, 4800)`, inside the diagonal
   camera 1 -> 4 switch band. Entering camera 4 starts the first-zombie FMV;
   skip it. `F_COMMON_02` is set after the film. On the way back the route
   optionally takes the handgun ammunition (0x0B, AOT_05/AOT_09 at
   `3300, 5600, 2000, 2000`) from the south side; this supplies the reserve
   the later zombie fights need. Return east to `(12000, 2700)` and open the
   104 -> 105 door (AOT_03) from the north.
4. **105 return.** Skip the entry scene and cross the dining room to
   `(31300, 8300)` in one leg. Open the 105 -> 106 door (AOT_01) from the
   west.
5. **106 main hall.** The return triggers Barry's lockpick scene
   (`on_enter_3`) and registers the lockpick item. Skip the scene, then walk to
   camera 3/4 in the north hall (`(11000, 25000)`) and back to camera 2/5
   (`(11000, 23000)`). Those camera cuts play the rest of the hall scene and
   restore the east-wing door. The lockpick item is registered at the room
   origin, outside the walkable floor; `action_nav` recognises that scripted
   handover, runs the item's room action (`bot:use_action`), closes the viewer,
   and waits until `has_item(0x31)` is true. If the first Barry scene never
   starts, the route takes the 203 stairs and returns once before retrying the
   camera cuts.
6. **107 gallery.** With the lockpick in the inventory and the east-wing door
   a normal door again, walk to `(31000, 12500)` just outside the door zone,
   face east and press action. In 107, the ink ribbon (0x2F) zone is in the
   south gallery. The port's default `jill_ink_ribbons = false` unregisters
   that item (the action slot is inert); with the option on, the route goes
   around the inner wall through `(14500, 11000)` and picks up three ribbons.
   Then open the sword-key door (AOT_01) to 108. The door's key is
   `ITEM_SWORD_KEY` (0x33), lock byte `0x89`; Jill's lockpick substitutes for
   the sword key because the 106 scene raises scenario flag `0x7C`, and the
   route logs `bot:flag(0, 0x7C)` plus the door's key/lock flag at navigation
   start and on a press timeout.
7. **108 L passage.** The handgun ammo (0x0B) zone `(20700, 13200, 1000, 1000)`
   lies inside the solid table object at `(21605, 13970)` (half extents
   `650, 544, 1400`). The table collision stops the player at `x = 22627`, so
   the 600-unit action reach ends at `x = 22027`, 327 short of the zone; the
   north and south sides are farther. Pushing the table from the north or
   south moves the player into the table/cerberus and does not clear the zone,
   so the route logs the block and takes the south door (AOT_01) to 109. It
   does not wedge in the pocket north of the table or fail the whole run on a
   geometry blocker.
8. **109 L passage.** Take the green herb (0x44) at `(20406, 1524, 1800, 1800)`
   from the east side, then cross the L passage to the west door (AOT_04) and
   enter 10A. The route logs `10A reached` and keeps going.
9. **10A back passage -> 117 large gallery.** Walk south of the 10A zombie at
   `(8272, 2433)` and open the north-wall door (AOT_01) to 117. The gallery's
   inner wall splits it into a north and a south half joined only at the east
   end. The star crest (0x2D, AOT_09 at `(1300, 3300, 1100, 1600)`) is covered
   by the `AOT_08` painting event until the seven painting switches are pushed.
   They must run in picture order, because each push clears room bits
   `0x18..0x1E` and the next painting requires the bit the previous one left:
   newborn (AOT_03, north side), infant (AOT_05), lively boy (AOT_06), young
   man (AOT_04), tired middle-aged man (AOT_02, north side), bold old man
   (AOT_07), then people mourning (AOT_08, the crest zone). The last push sets
   `F_SCENARIO_03`, opens the frame and moves Jill to `(2773, 4147)`; the east
   approach at `(2900, 4100)` then picks up the crest. Walk back through the
   east gap and the south-wall door (AOT_00) to 10A.
10. **10A -> 11A roofed passage.** Walk down the west corridor past the zombie
    at `(3023, 6864)` and open the west-wall door (AOT_04) to 11A. In the
    first mansion 11A registers the four crest use-item hollows at
    `(6500, 27200, 2000, 1000)`, not a radio key action, so `bot:actions()` by
    item 0x4D finds nothing and the route logs that. If another flag set does
    register the radio key (the second-visit Chris item, whose model sits at
    `4785, 8990`), `action_nav` picks it up from its actual zone.
11. **11A -> 11B store room.** The door (AOT_01 at `(8000, 24500, 1500, 2100)`)
    is `DOOR_LOCKED | F_LOCK_17` (`0x97`) with key byte `0x00`. The room raises
    the lock only after all four crest hollows are filled; one star crest in
    the inventory is not enough and the radio key never matches. The route
    presses the known-good west approach, logs the door slot/key/lock/lock bit,
    the lock-bank flag, the four crest scenario bits, and stops in 11A rather
    than forcing the lock. If 11B does open, the route takes the desk key
    (0x3D, AOT_02 at `8100, 8500`) and crank key (0x1D, AOT_03 at
    `5000, 1200`), then optionally opens the first-house door (AOT_01 at
    `8800, 4300`) to 300.
12. **11A return to the east wing.** The crest/key extension leaves 11A back
    through 10A -> 109 -> 108 -> 107 -> 106 -> 105. Each leg is a normal
    `door_nav`; `enter_room` hands the arrival to the next named travel phase,
    so the outward room defaults cannot restart the opening route.
13. **105 shield key and blue jewel key.** The shield key (0x35) action exists
    at `(17400, 14500, 1000, 1000)`, but the north wall and the sideboard leave
    no standable point inside the 600 reach. The north wall
    (`z 14454..16632`) grown by Jill's 372 radius plus the 18-unit nav skin
    leaves `z <= 14064`; the sideboard (`18000, 14300`, half extents
    `700, 600`) grown by the same radius covers `x 16928..19072,
    z 13328..15272`. The only corner left is `(16928, 14064)`; its reach ends
    at `(17377, 14462)`, 23 short in x and 38 short in z of the zone's
    south-west corner `(17400, 14500)`. The route walks to `(16900, 14060)`,
    presses for 60 ticks, logs the count, and continues to 106. The blue jewel
    key (0x21) is only registered when scenario flag `0x0B` is set; in the
    checked-in Jill route it is absent and is logged as such.
14. **106 -> 203 -> 202.** The 203 stair door lands on the balcony landing,
    which is sealed by the room collision. The route recognises the in-room
    `check_door` zones, walks to the west side of the east flight at
    `(19700, 25650)`, presses action, and lets the room's own climb clip warp
    the player onto the east corridor. It then walks around the balcony to the
    202 door. This uses `bot:use_stair(slot)`, the same room handler a normal
    action press runs.
15. **202 -> 201 -> 101 -> 103 -> 10C.** The route takes the long south
    corridor at `z 3000` around 202's central block and re-arms the handgun
    after each kill, so the second corridor zombie cannot grab the route while
    it is disarmed. The 201 groundskeeper is fought normally. The 101 corridor
    zombie is bypassed instead of fought: the route runs the long north leg in
    a straight line at `x 2800`, but the zombie can still catch and grab it
    (about 20 health); the room policy logs the bypass and does not stop to
    shoot it. In 10C the armor key (0x34, AOT_04 at
    `5000, 4500, 800, 2000`) is behind six plant fountains
    (`0x0F` at `8400/8300/8200, 5800/6100/6200` and `7900/8400/8200,
    4700/4900/4800`). Their Manhattan trigger is 3000, and a hit knocks the
    player out of the 498-unit-wide bed band (`z 3072..3570`) before the item
    reach animation can finish. The route takes the 8100 green-herb action from
    `(9300, 3280)` first (a 60-tick attempt budget), then either crosses the
    band west to the key from `(6100, 3280)` or retreats to 103 at 26 health
    with a report. It does not force the key or the crest route.
16. **Unreached crest/key phases.** The 205 sun crest, 20D/20E/210 moon crest,
    the return to 11A and the final crest-hollow report are implemented as
    normal travel/item phases, but cannot run until the 10C armor key is
    obtainable. The shield-key door to 210 is also blocked because the 105
    shield key is geometrically out of reach. The finish reason names the exact
    blocker and the held items.

Long legs run and only the final 400 units walk. `walk_to` uses pure pursuit
(the farthest path point within 1600 units that is in clear sight), keeps the
run latched through moderate heading errors, and only stops for a sharp turn,
so the route does not flicker between walk and run at every smoothed waypoint.
The A* grid is 512 units, so the path ends at the exact point the route named
when the last leg is clear; otherwise the bot would run past a door approach
and turn back. A scripted move (the 105 Barry scene) discards the cached path
and replans from the new position.

Door transitions release on the first tick the transition is seen and then
press on frames 1, 3, 5, ... . The engine's skip gate opens on frame 11, so
this hits the first allowed frame whatever tick the transition began on; every
door transition in the route is 11 ticks.

The east-wing door is restored by the camera 3/4 -> 2/5 scenes. The lockpick
item's zone is at the room origin, so the route uses `bot:use_action(slot)` on
that item action (the same handler a normal action press runs) and closes the
got-item viewer. It checks `has_item(0x31)` before leaving 106. The finish
reason always names the furthest room/item milestone reached, such as
`11A reached; 11B store room door locked (...)`, so a partial run is still
self-describing.

### Interaction skills

`action_nav(bot, slot, label, custom, opts)` is the generic ground-item skill.
It finds the action by slot, treats a zone at the room origin as the scripted
lockpick handover (running `bot:use_action`), and otherwise computes four
approach points 300 units outside the zone. It tries the nearest side first,
walks to it with `walk_to`, faces the zone centre, presses action, and waits
for `item_count` to rise or the got-item viewer to close. A side that does not
fire is abandoned after 60 controllable ticks, and a bounded timeout logs the
zone, count, position, health and collision flags. Optional `custom` points
are tried before the generic sides; the 107 ribbon and 109 herb use them for
the narrow gallery and the herb's wall corner. With `opts.optional = true` a
missing/unreachable item logs the same diagnostics and returns `"failed"` to
the phase instead of ending the run; the 117 star crest, 11A radio key and
11B desk/crank keys use that so a blocked optional item does not hide the next
milestone.

`door_nav` keeps its four-side approach. For a keyed door it logs the door
slot, key byte, lock byte, the lock-bank bit and the scenario flag when the
press times out; a key turn that only raises the lock flag makes the next
press transition. The 11A -> 11B test uses the known-good west approach and
stops after its first timeout when the lock is `F_LOCK_17`, reporting the
crest bits instead of trying wall-side approaches for a door that cannot open.

### Combat

`combat_step(bot, scene, player)` runs after scene/menu handling and only on a
controllable, non-grabbed tick. It is a conservative first combat layer, not a
replacement for the route:

- The nearest active monster in front (within 4500 units and 0x400 of the
  player's facing) is a threat. A room with more than one monster within that
  range is treated as a crowd and the route runs past.
- It fights only common zombies (`0x00`, `0x01`, `0x11`) with the handgun
  (`0x02`). Bosses, hunters and every other id stand the policy down. It never
  equips anything the inventory does not hold, and never changes ammo, health
  or flags.
- If health is at or below 40, the handgun is missing, the clip and reserve
  are both empty, or the clip is too small to finish a fresh zombie without
  spare ammunition, it returns `false` and the route's normal movement
  continues.
- It equips the handgun through `bot:equip(0x02)`, faces the target, holds aim
  until the reticle locks, and fires one pulse every 14 ticks. It follows the
  target's `status_flags` 0x20/0x80 vertical bit with up/down aim, because the
  damage pipeline only connects when the aim direction matches the monster's
  status bit.
- A hit puts the zombie into its reaction window; more bullets during that
  window are discarded, so the policy waits for `hit_state` to clear.
- The handgun's near cone excludes bodies closer than their collision radius
  plus 200 units. If the zombie closes inside 1800 units, the policy releases
  aim, backs away to 2600, then resumes firing.
- An empty clip with reserve uses the tactical reload (aim + run) when the
  build supports it. If the reload does not start or finish in its window, the
  policy disengages instead of standing in front of the monster.
- A kill, a target that leaves range/sight, a timeout (420 combat ticks), low
  health or an empty gun ends the fight and disarms combat for the rest of
  that room. A room change re-arms it. The 202 dining room re-arms after each
  kill because it has two corridor zombies, and uses a full-circle threat arc
  so the second one cannot grab from behind. The 101 corridor zombie is
  explicitly bypassed: fighting a 97-health zombie with the 7-round clip costs
  more health than running the long west-edge line. `handle_scene` still owns
  every transition, message and film.

The `--watch` reload keeps `BOT_STATE.combat` with the rest of the route
state, so editing the policy mid-fight does not restart or reset the fight.

Headless evidence from the checked-in route: the 104 ammunition pickup gives
30 rounds of reserve, the 10A zombies are engaged and killed, the one 109 green
herb is used at 56 health, and the 202/201 groundskeeper fights are won with the
handgun. The 101 corridor zombie is bypassed and the 202 corridor is crossed at
`z 3000`; the checked-in run still takes one grab in 101 and reaches 10C with
31 health. The 8100 green-herb press is knocked out of reach by the nearest
fountain before the reach animation finishes; after the 60-tick attempt health
is 27, below the 26 key-lane floor, so the route retreats through the
10C -> 103 door and finishes with `103 retreat; 10C armor key blocked: ...` at
about 15 health. The room path is
`106 -> 105 -> 104 -> 105 -> 106 -> 107 -> 108 -> 109 -> 10A -> 117 ->
10A -> 11A -> 10A -> 109 -> 108 -> 107 -> 106 -> 105 -> 106 -> 203 -> 202 ->
201 -> 101 -> 103 -> 10C -> 103`, with 0 shield keys, 0 blue jewels, 0 armor
keys, 1 star crest, and the 11B lock still closed. The 105 shield key is
pressed from `(16900,14060)` and logs the 23/38-unit reach shortfall. This is
the current blocker, not a finish of the crest collection.

### Healing

`bot:use_heal(item)` queues a legal one-tick heal request. The Lua snapshot
only accepts the item when it is in the inventory, has at least one unit, and
the catalog kind is `Heal`; it then returns `true` (accepted). The host validates
the same conditions again, calls `game.use_item(item)` (the menu USE rules),
and on `UseResult::Used` consumes one unit through
`select_item` + `use_selected_item`, before the gameplay tick. The bot never
writes health, status, flags or inventory quantities itself.

The route calls it below 60 health with a green herb (0x44), at most once per
60 controllable ticks, and only when no fight is active. Scenes, menus and
grabs never reach the healing step; an active fight is allowed to finish
first, so a heal cannot interrupt a grab or a shooting sequence. The
checked-in run uses the one 109 green herb after the first 10A fight. Later
zombie fights are supplied by the 104 handgun ammunition pickup.

### Failure handling

- Every phase has a tick deadline. A timeout logs the phase, room, position,
  camera, health, collision flags, `F_COMMON_02/03/05`, `F_COMMON_40/41`,
  `F_STATUS_1A`, lockpick state, scenario `0x7C`, scenario
  `0x03/0x69/0x6A/0x6B/0x6C`, lock `0x17`, the ribbon/ammo/herb and
  star/radio/desk/crank counts, and the scene flags, then calls `bot:finish`
  with a `failed: ...` reason.
- A waypoint that reports `blocked` or `unreachable` is retried once with a
  tighter tolerance and then skipped, so one bad point cannot stall the run.
  The 117 painting approaches sit well clear of the inner wall's grown
  rectangle: the 512-unit A* snap would otherwise put the goal node inside the
  wall and report the point unreachable.
- Doors use a point 300 units outside the zone (the action reach probe is 600
  units ahead), face the zone, and press for up to 110 ticks. If the room does
  not change, the next side of the zone is tried. A keyed door logs its key,
  lock bank bit and the lockpick scenario flag.
- A grab mashes action plus alternating directions; an unmarked locked state
  does the same after 150 ticks.
- The 108 ammo is a known geometry blocker: its zone is inside a solid table
  and the reach cannot enter from any side. The route logs it and continues to
  109 instead of treating the room as a failed run.
- The 105 shield key action exists at `(17400,14500,1000,1000)` but every
  standable point is outside the 600 reach: the north wall's grown edge is
  `z=14064`, the sideboard's grown box starts at `x=16928`, and the
  south-west corner's probe ends 23 units short in x and 38 in z. The route
  walks to `(16900,14060)`, presses, logs the count, and continues. The blue
  jewel key action is absent while scenario `0x0B` is clear.
- The 203 landing is sealed by room collision; the route uses the room's
  `check_door` stair zones through `bot:use_stair(slot)` instead of trying to
  walk through the wall. A stair that does not move the player is a precise
  `fail`, not an endless press.
- The 10C bed band is `z 3072..3570` (498 units after the 372 radius and
  18-unit nav skin). The plant fountains' 3000 Manhattan trigger covers the
  whole crossing, and each hit knocks the player out of the band before the
  herb reach finishes. The route spends at most 60 ticks on the 8100 green
  herb, then tries the key lane; at 26 health it retreats to 103 and reports
  the position, health, held keys/crests and hollow count instead of dying.
  The armor key's action zone itself is standable at `(6100,6300)` (west wall
  grown edge `x=5634`), but the path to it crosses the stunlock.
- A locked 11B door is not a failed run: the route records the lock and stops
  in 11A with the lock, key, crest and inventory state in the finish reason.

## API

`on_bot_tick(bot)` runs once per app update, before the game tick.

| Call | Result |
| --- | --- |
| `bot:tick()` | Monotonic app-update counter. |
| `bot:room()` | `{id, stage, room, player_flag, mansion2, camera, controllable}`. |
| `bot:room_meta()` | The current room's authored `room/<id>/bot.xml` metadata, or `nil`. Read-only; see [Room metadata](#room-metadata). |
| `bot:player()` | `{x, y, z, angle, health, max_health, state, being_attacked, equipped, collision_flags, grabbed}`. `collision_flags` is the live health-status/collision byte (bit `0x10` skips shape-5 floor volumes). `grabbed` is the entity's grab zone flag. |
| `bot:scene()` | `{mode, skippable, film, transition, cutscene_skip, message, message_takes_input, pickup, frozen, controls_locked, controllable, dead}`. |
| `bot:actions()` | Array of `{slot, kind, x, z, width, depth, item, quantity, next_room, key, lock, handler, flags}`. |
| `bot:doors()` | Array of `{slot, target, next_room, key, lock, x, z, width, depth}`; `target` is the destination room id. |
| `bot:enemies()` | Array of active monsters (`id < 0x20`, `health > 0`), nearest first: `{slot, id, x, y, z, angle, bearing, health, state, status_flags, action_behavior, action_state, hit_state, distance, visible}`. `bearing` is the absolute 12-bit angle from the player; `distance` is horizontal; `visible` is the weapon line-of-sight result. Boss ids are included. |
| `bot:weapon()` | `{equipped, weapon, clip, reserve, behavior, state, aim_state, target}`. `equipped` is the inventory item (or nil), `weapon` is `equipped_weapon_id`, `clip` is the rounds in the equipped weapon's stack, `reserve` is `item_count(weapon + 9)`, and `target` is the locked entity slot or nil. |
| `bot:can_see(slot)` | Whether monster `slot` is in the current snapshot with a clear weapon sight line. |
| `bot:flag(bank, bit)` | A flag bit, read-only. |
| `bot:has_item(item)` / `bot:item_count(item)` | Inventory queries. |
| `bot:memory()` | String map set by `remember`. |
| `bot:visited(room)` | Whether a room id such as `"106"` (or packed `0x106`) was seen. |
| `bot:remember(key, value)` | Store a string/number/boolean in memory; `nil` removes it. |
| `bot:log(text)` | Print `[bot] tick N room R: text`. |
| `bot:finish(reason)` | End the run and report `reason`. |
| `bot:set_input(table)` | Set the next tick's player/UI input. |
| `bot:press_action()` | Hold and edge the action button for one tick. |
| `bot:pulse_confirm()` | Edge UI confirm for one tick. |
| `bot:equip(item)` | Queue an equip for the next tick; returns `false` and logs when the item is not held or is not a weapon. The host applies the same `set_equipped` the menu does before the tick. |
| `bot:use_heal(item)` | Queue a heal for the next tick; returns `false` and logs when the item is not held, has no units, or is not a `Heal` catalog item. The host runs the menu `use_item` rule and consumes one unit through `select_item` + `use_selected_item` before the tick. |
| `bot:use_action(slot)` | Run the item room action in `slot` for one tick (the same handler a normal action press runs). Restricted to `kind == "item"`; used for scripted handovers whose zone is outside the walkable floor. |
| `bot:use_stair(slot)` | Run the `door_check` room action in `slot` for one tick, the in-room stair/climb handler a normal action press runs. Combined with `press_action`, this is how 203's sealed landing is climbed. |
| `bot:walk_to(x, z, opts)` | Collision-aware A* walk. Returns `{status, distance, angle_error}`. |
| `bot:face_to(x, z)` | Turn toward a point. Returns `{status, distance, angle_error}`. |
| `bot:distance(x, z)` | Horizontal distance. |

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

`format` is required to be `1` when present (the default). `puzzle` takes
`id` and optional `title`; `step` takes `slot`, `bank`, `bit` and `label`.
`bit` is the flag selector passed to `bot:flag`, so the examples use `0x1A`,
not bit number 26. A step has any number of `<walk x="..." z="..."/>` points
and at most one `<face x="..." z="..."/>`; coordinates are signed 32-bit
integers. `slot` and `bit` are bytes and `bank` must be below 10.

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

The checked-in route uses this metadata for the 117 painting order. It keeps
the hardcoded `PAINT_STEPS` table as a fallback and logs which source it is
using once per source change.

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
