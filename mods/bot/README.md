# Player bot

A restricted Lua route that drives a normal new Jill game through the same pad
a human would use. It cannot set flags, items or positions.

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
VM from the pack, so route globals start fresh there.

## Route

The route starts a fresh Jill game and stops when it enters the east-wing
gallery (room 107):

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
   skip it. `F_COMMON_02` is set after the film. Return east to
   `(12000, 2700)` and open the 104 -> 105 door (AOT_03) from the north.
4. **105 return.** Skip the entry scene and cross the dining room to
   `(31300, 8300)` in one leg. Open the 105 -> 106 door (AOT_01) from the
   west.
5. **106 main hall.** The return triggers Barry's lockpick scene
   (`on_enter_3`) and registers the lockpick item. Skip the scene, then walk to
   camera 3/4 in the north hall (`(11000, 25000)`) and back to camera 2/5
   (`(11000, 23000)`). Those camera cuts play the rest of the hall scene and
   restore the east-wing door. The lockpick item is registered at the room
   origin, outside the walkable floor; the route runs that item's room action
   (`bot:use_action`) to open the got-item viewer, closes the viewer, and waits
   until `has_item(0x31)` is true. If the first Barry scene never starts, the
   route takes the 203 stairs and returns once before retrying the camera cuts.
6. **107 gallery.** With the lockpick in the inventory and the east-wing door
   a normal door again, walk to `(31000, 12500)` just outside the door zone,
   face east and press action. Entering 107 logs `east wing reached` and
   finishes the run.

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
got-item viewer. The run finishes only after `has_item(0x31)` is true. If the
viewer or the item never arrives, the route reports the lockpick state and
fails instead of spinning.

### Failure handling

- Every phase has a tick deadline. A timeout logs the phase, room, position,
  camera, `F_COMMON_02/03/05`, `F_STATUS_1A`, lockpick state and scene flags,
  then calls `bot:finish` with a `failed: ...` reason.
- A waypoint that reports `blocked` or `unreachable` is retried once with a
  tighter tolerance and then skipped, so one bad point cannot stall the run.
- Doors use a point 300 units outside the zone (the action reach probe is 600
  units ahead), face the zone, and press for up to 110 ticks. If the room does
  not change, the next side of the zone is tried.
- A grab mashes action plus alternating directions; an unmarked locked state
  does the same after 150 ticks.

## API

`on_bot_tick(bot)` runs once per app update, before the game tick.

| Call | Result |
| --- | --- |
| `bot:tick()` | Monotonic app-update counter. |
| `bot:room()` | `{id, stage, room, player_flag, mansion2, camera, controllable}`. |
| `bot:player()` | `{x, y, z, angle, health, max_health, state, being_attacked, equipped}`. |
| `bot:scene()` | `{mode, skippable, film, transition, cutscene_skip, message, message_takes_input, pickup, frozen, controls_locked, controllable, dead}`. |
| `bot:actions()` | Array of `{slot, kind, x, z, width, depth, item, quantity, next_room, key, lock, handler, flags}`. |
| `bot:doors()` | Array of `{slot, target, next_room, key, lock, x, z, width, depth}`; `target` is the destination room id. |
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
| `bot:use_action(slot)` | Run the item room action in `slot` for one tick (the same handler a normal action press runs). Restricted to `kind == "item"`; used for scripted handovers whose zone is outside the walkable floor. |
| `bot:walk_to(x, z, opts)` | Collision-aware A* walk. Returns `{status, distance, angle_error}`. |
| `bot:face_to(x, z)` | Turn toward a point. Returns `{status, distance, angle_error}`. |
| `bot:distance(x, z)` | Horizontal distance. |

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
