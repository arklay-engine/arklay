# Player bot

A restricted Lua route that drives a normal new Jill game through the same pad
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

# Edit bot/**.lua while the route runs: --watch re-executes the chunks in the
# same Lua state, so BOT_STATE and the engine's bot memory keep their progress.
target/test-release/arklay ~/.cache/arklay-biorand/re1.akpak --mod mods/bot --bot --watch

# --bot-risk 0.0..1.0 (default 0.25) is how much danger the planner may accept;
# the route reads it as bot:risk(). Values outside the range are an error.

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

The route starts a fresh Jill game. It continues through 117, 11A, the
east-wing return, the 105/10F shield-key detour, and the 10C/204/205 armor
path, and stops with a precise item/health report. The 10F gold-key trap is
resolved legally with the 105 wood emblem key: the pickup closes the bar, and
using the wood key at the 10F hollow reopens it while keeping the gold key for
the 105 fireplace. The shield key (0x35) is taken and the star crest is placed
in 11A. 205's sun crest is then blocked by the room-init item deletion, and
20D's Richard death scene blocks 20E/210 (the route has no serum), so the moon
crest and the 11B lock remain unreached. The 10C greenhouse's six plant
fountains stunlock the player in the only bed crossing, so that path uses the
118 herbicide at the pump, takes the armor key, and goes on to 205:

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
   restore the east-wing door. The lockpick item (IT_31) is registered as
   AOT_06 at `(0, 0, 16, 16)` with `SAT_UNDER | SAT_PLAYER`, an automatic
   probe of the player's own position at the room origin, outside the walkable
   floor. No Jill event in 106 fires it (its `aot_on`/`give_item` calls are in
   the CHRIS branches), so **the inventory item cannot be obtained by input**
   and the route does not fake it. The game's own handover is the Barry
   scene's `set(FG_SCENARIO, F_SCENARIO_7C, TRUE)` in `from_main_11`, and the
   door rule (`has_lockpick`) tests that flag; the route waits for the flag
   (or the item) and logs the missing item. If the first Barry scene never
   starts, the route takes the 203 stairs and returns once before retrying the
   camera cuts.
6. **107 gallery.** With the lockpick flag raised and the east-wing door
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
    at `(17400, 14500, 1000, 1000)`, behind the north wall and the active
    OBJ_02 sideboard at `(18000, 14300)`. The north wall
    (`z 14454..16632`) grown by Jill's 372 radius plus the 18-unit nav skin
    leaves `z <= 14064`; the sideboard (half extents `700, 600`) grown by the
    same radius covers `x 16928..19072, z 13328..15272`. The only corner left
    is `(16928, 14064)`; its reach ends at `(17377, 14462)`, 23 short in x and
    38 in z of the zone's south-west corner `(17400, 14500)`. The sideboard
    cannot be pushed: its two-point floor probe uses the 300-unit body radius
    and the body already sits 154 units from the north wall, so the probe
    always reports the object blocked and the push counter parks at 10. The
    room moves the sideboard to `(16000, 14300)` only through its scripted
    gold-emblem event (`from_main`, raised by F_COMMON_AB): the 10F bar's gold
    emblem key (0x20) is used at the 105 fireplace after the room's
    SCE_USEITEM actions raise the item use flag (bank 9 bit 5) and F_ROOM_22
    (bank 4 bit 0x22).

    The route first takes the wood emblem key (0x1F) from the 105 fireplace
    while F_COMMON_AA is set by the 106 intro. Picking it clears AA, and the
    105 main's AA-false branch re-arms the fireplace SCE_USEITEM probes with
    `aot_delete(..., SCE_USEITEM, SAT_PLAYER)`; that is why the gold key can
    still be used at the fireplace later. The wood key is not used in 105.

    The detour then runs 105 -> 104 -> 10F: push OBJ_01 (the bookshelf at
    `17394,11190`) **east** with the normal forward+push input, use the green
    herb if the pack is full, take the music score (0x23) at slot 5, play the
    piano at `(10200,7300,600,1500)`, wait for F_COMMON_A5, then walk north
    through the now-open `x8356..9720` gap and pick the gold key at
    `(8100,18800,1500,1000)`. The pick-up is real: the run logs
    `gold=1 held=true`.

    The gold pick-up raises `testpickup(IT_20)` and runs `from_main_2_jill`,
    which sets OBJ_00 to about `(13380,12596)` (half extents 16000/400). That
    covers `x -2620..29380, z 12196..12996` and closes the only north-south
    gap, trapping the player at `(8996,13447)`. The escape is the wood key:
    with F_COMMON_A6 now set, the 10F main re-arms the wood use-item probes
    AOT_06/AOT_07 (`aot_delete(..., SCE_USEITEM, SAT_PLAYER)`), so the route
    walks back to the key hollow at `(9000,18500)`, faces north, waits for
    bank 9 bit 4 and F_ROOM_22, and uses the key from the inventory menu
    (`BotMenu.use_item(0x1F)`). The room's
    wood branch runs `from_main_3`/`from_main_4`, which tween OBJ_00 west and
    then to `0,0,0`, reopening the gap. The gold key stays in the inventory.

    The route returns through 104 to 105, walks to the fireplace approach
    `(4100,8100)`, waits for the gold use flag (bank 9 bit 5) and F_ROOM_22,
    and uses it from the menu (`BotMenu.use_item(0x20)`). The room's gold event
    sets F_COMMON_AB and
    tweens OBJ_02 to `(16000,14300)`. The shield key is then approached around
    the moved sideboard: `(14000,13000)` -> `(17100,13000)` ->
    `(17100,14060)`, then the 600 reach lands at `(17492,14514)` inside the
    key zone. The run logs `shield key held count=1`.

    If the wood key is missing or the hollow use does not fire, the fallback
    uses the gold key itself at the 10F hollow (the same from_main_3 escape).
    That reopens the bar but spends the key, so the 105 sideboard stays at
    `(18000,14300)` and the shield-key leg is forfeited; the route logs the
    precise reason. The blue jewel key (0x21) is absent while scenario `0x0B`
    is clear.
14. **106 -> 203 -> 202.** The 203 stair door lands on the balcony landing,
    which is sealed by the room collision. The route recognises the in-room
    `check_door` zones, walks to the west side of the east flight at
    `(19700, 25650)`, presses action, and lets the room's own climb clip warp
    the player onto the east corridor. It then walks around the balcony to the
    202 door. The climb is one real action press: facing east from
    `(19700, 25650)` puts the 600 reach inside the `door_check` zone
    `(20220, 23900, 3540, 3500)`, and the press starts the room's climb. A
    press that does not move the player is repeated every 40 ticks (at most 5)
    and then reported as a blocker. This replaces the old `bot:use_stair`, the
    same room handler a normal
    action press runs.
15. **202 -> 201 -> 101 -> 103 -> 10C.** The route takes the long south
    corridor at `z 3000` around 202's central block and re-arms the handgun
    after each kill, so the second corridor zombie cannot grab the route while
    it is disarmed. The 201 groundskeeper is fought normally. The 101 corridor
    zombie is bypassed instead of fought: the route runs the long north leg in
    a straight line at `x 2800`, but the zombie can still catch and grab it
    (about 20 health); the room policy logs the bypass and does not stop to
    shoot it. The east-wing return detours through 10B and 118 for the
    herbicide (0x26): 10A -> 10B (the `0xFE` far-side door opens on the second
    press) -> 118, where the herbicide is picked from `(4560,3369)`, then back
    through 10B (fighting the stair zombie on the way in, running past the
    restored one on the way out and taking the room's green herb), and on to
    109. In 10C the armor key (0x34, AOT_04 at `5000, 4500, 800, 2000`) is
    behind six plant fountains (`0x0F`). Their Manhattan trigger covers the
    whole 498-unit bed band (`z 3072..3570`), so the route uses the room's own
    pump solution: walk to `(16200, 4700)`, face north so the 600 reach lands
    in the pump's use-item zone `(15300, 5100, 1800, 600)`, wait for the
    herbicide use flag (bank 9 bit 11) and F_ROOM_20 (bank 4 bit 0x20), then
    use the herbicide from the menu (`BotMenu.use_item(0x26)`). The room's
    `from_main` event withers all six
    fountains and sets F_COMMON_A9. The key is then taken from `(6100,6300)`
    with the normal reach. The 10C green-herb actions are reach-gated and can
    re-arm each other's got-item viewers; the route swallows the viewer-close
    press and waits for the last viewer before moving on. Their awards are
    unreliable in this build (the run logs the viewer closing with `count=0`),
    so the route relies on the first-aid spray and the 10B/202/101 herbs for
    health rather than the greenhouse bed.
16. **Current stop: 20D (moon route).** The checked-in run now takes the 10F
    shield key, places the star crest in 11A, clears 10C and 204, and reaches
    205 and 20D. Final report: shield=1, armor=4 (the 204 door spends one),
    star hollow placed (1/4), sun=0, moon=0, 11B `lock17=false`, health 2.
    The run does not die; it stops with the exact blockers.

    205 sun crest: the live action slot 9 is `kind=other`/handler 0 at room
    entry. The init `item_aot_set(AOT_09_ITEM, ...)` is immediately followed
    by `aot_delete(AOT_09_ITEM, SCE_NONE, SAT_MANUAL | SAT_PLAYER)`; in this
    port the delete op sets the handler to the passed SCE (0), while the
    item model and `F_ITEM_24` remain. `item_action_slot(0x2E)` therefore
    finds nothing, and the route detects that before walking into the poison
    gas. The armor switch at `(9235,6736,1000,1000)` is still a live
    `kind=event` action, but `F_ROOM_20`/`F_ROOM_21` read false at entry
    (despite the room init setting them), so pressing it takes the room's
    `from_main_2` branch and leaves `F_ROOM_1D`/`F_SCENARIO_3A` clear; the
    camera-2 gas also drains health at collision flag 0x80. The route logs the
    dead item action and continues instead of dying at the switch.

    20D moon route: the room's Richard death entry scene sends the player back
    to 204 (the scene's final move parks at `(2982,12660)`, beside the 204
    door). The 20E door is armed for the story only by
    `item_ck(IT_42_KEY_SERUM)`, which this route does not hold, so 20E/210 and
    the moon crest are unreachable on this branch. The 20D travel phase and
    the 210 moon phases remain implemented for a run that arrives with the
    serum; the route now stops at 20D entry with the report rather than
    looping through the scene.

    The 11B store-room door stays locked (`F_LOCK_17` false) because only the
    star hollow is filled; sun and moon are missing. The route's finish reason
    carries the room path, health, shield/armor/sun/moon counts and the
    hollow/lock state.

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
item's zone is at the room origin and no Jill script fires it, so the route
leaves 106 on `F_SCENARIO_7C` (the flag the door rule tests) without the
inventory item, and records that in memory as `blocker_lockpick_item`. The finish
reason always names the furthest room/item milestone reached, such as
`11A reached; 11B store room door locked (...)`, so a partial run is still
self-describing.

### Interaction skills

`bot:inventory()` returns the live inventory as a read-only array of
`{ slot, item, quantity }` rows, including the zero-quantity weapon slots. The
route uses it to detect the full pack (the 106 Barry gift adds the acid
rounds and the empty weapon stacks still count) before the 105 wood-key
pickup, without guessing from item counts. It never writes the inventory.

`action_nav(bot, slot, label, custom, opts)` is the generic ground-item skill.
It finds the action by slot, treats a zone at the room origin as a scripted
handover that only the room script can fire (it waits for the item and then
reports a `BLOCKER`; it never runs the handler), and otherwise computes four
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
milestone. `opts.side_ticks` extends the per-side wait for reach-gated items,
`opts.single_press` sends only one action edge per side (the 10C herbs), and
`opts.require_count` refuses to treat a got-item viewer closing as success
unless the item count actually rose (the 10C armor key). The pickup viewer's
dismissal press is swallowed by the host until the button is released, so the
same press cannot immediately re-arm a reach-gated item.

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
- It equips the handgun through the inventory menu (`BotMenu.equip(0x02)`),
  faces the target, holds aim
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
  more health than running the long west-edge line. The 204 corridor zombie is
  bypassed for the same reason, though it can still catch the route once.
  `handle_scene` still owns every transition, message and film.

The `--watch` reload keeps `BOT_STATE.combat` with the rest of the route
state, so editing the policy mid-fight does not restart or reset the fight.

Headless evidence from the checked-in route: the 104 ammunition pickup gives
30 rounds of reserve, the 10A zombies are engaged and killed, and the
202/201/103 fights are won with the handgun. The first-aid spray is used at 56
health in 10A; later green herbs are used at 56 health in 10B and 202 and at
43 in 101. The 10F detour uses a green herb to free the music-score slot,
pushes the bookshelf east, takes the music score, plays the piano, picks the
gold key, then uses the 105 wood key at the key hollow to reopen the bar. The
run path is
`106 -> 105 -> 104 -> 105 -> 106 -> 107 -> 108 -> 109 -> 10A -> 117 ->
10A -> 11A -> 10A -> 10B -> 118 -> 10B -> 10A -> 109 -> 108 -> 107 -> 106 ->
105 -> 104 -> 10F -> 104 -> 105 -> 106 -> 203 -> 202 -> 201 -> 101 -> 103 ->
10C -> 103 -> 101 -> 201 -> 202 -> 203 -> 204 -> 205 -> 204 -> 20D`, with
**shield=1**, blue jewel=0, **armor=4**, star crest placed (1/4 hollows),
sun=0, moon=0 and `lock17=false`. The run finishes at 20D, health 2, no death
and no max-ticks. The 205 sun-crest action is dead at room init; the 20D
Richard death scene returns to 204 and the 20E serum branch is not armed. The
10C green-herb viewers still show `count=0`; the spray and the other-room
herbs are the reliable heals.

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

### Healing

`BotMenu.use_heal(bot, item)` runs a menu USE on a heal item (see
[Menu skills](#menu-skills)): it refuses an item that is not held, has no
units, or whose catalog kind is not `heal`, and reports success only when the
item's count dropped. The bot never writes health, status, flags or inventory
quantities itself.

The route calls it below 60 health, preferring the first-aid spray (0x41,
full heal) and falling back to a green herb (0x44), at most once per 60
controllable ticks and only when no fight is active. Scenes, menus and grabs
never reach the healing step; an active fight is allowed to finish first, so a
heal cannot interrupt a grab or a shooting sequence. The first-aid spray from
the new-game inventory is used at 56 health in 10A. The checked-in 10F branch
also uses a green herb at 76 health before the music-score pick-up: the pack is
full, and the menu USE both heals to 96 and frees the slot. The 10C/205/20D
continuation uses the 10B/202/101 herbs at 56/56/43 and still arrives at 205
with only 2 health; the armor-room gas cannot be tanked further. The 118
spray is a second-visit (`MANSION2`) item and is logged absent on this
first-mansion route.

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
- The 105 shield key action at `(17400,14500,1000,1000)` is unreachable while
  OBJ_02 is at `(18000,14300)`: the north wall's grown edge is `z=14064`, the
  sideboard's grown box starts at `x=16928`, and the south-west corner's 600
  probe ends 23 units short in x and 38 in z. The sideboard cannot be pushed
  either: its two-point floor probe uses its 300-unit body radius and it sits
  154 units from the north wall. The room's gold event is the only mover.
  After the event moves it to `(16000,14300)`, the route walks south around it
  and up its east side to `(17100,14060)`, where the reach lands inside the
  key zone; the pickup is real (`shield key held count=1`).
- The gold-emblem detour reaches and keeps the 10F key: take the 105 wood key
  first, push OBJ_01 east from its west face at `(15700,10800)`, use a green
  herb if the pack is full, take the music score at slot 5, play the piano at
  `(9700,8000)` facing the zone at `(10500,8000)`, wait for F_COMMON_A5, then
  take the gold key at `(8100,18800,1500,1000)` from `(8850,18500)`. The
  pickup's `from_main_2_jill` moves OBJ_00 to about `(13380,12596)` with half
  extents 16000/400, covering `x -2620..29380, z 12196..12996` and closing
  the only `x8356..9720` gap. That is the trap. With F_COMMON_A6 set the room
  re-arms the wood probes; the route returns to `(9000,18500)`, faces north,
  and uses the wood key, whose `from_main_3` tweens OBJ_00 back to `0,0,0`.
  The gold key survives for the 105 fireplace. Without the wood key the
  fallback spends the gold key on the same escape and forfeits the shield.
  The blue jewel key (scenario `0x0B` clear) remains absent in either case.
- The 203 landing is sealed by room collision; the route presses action
  facing the room's `check_door` stair zone instead of trying to walk through
  the wall. A stair that does not move the player is a precise
  `fail`, not an endless press.
- The 10C bed band is `z 3072..3570` (498 units after the 372 radius and
  18-unit nav skin). The plant fountains' Manhattan trigger covers the whole
  crossing, so the route first collects the 118 herbicide, stands at
  `(16200,4700)` so the 600 reach hits the pump zone, and uses the herbicide
  from the menu. The room's own withering event removes the fountains. The
  armor key is then taken from `(6100,6300)`. The green herbs run after the
  key; their overlapping reach-gated actions can re-arm each other, and the
  viewer-close action is swallowed until release. Their awards show `count=0`
  in this build, so the route does not depend on them. The armor key's action
  zone itself is standable at `(6100,6300)` (west wall grown edge `x=5634`),
  but the path to it crosses the stunlock without the herbicide.
- The 205 sun-crest item action is disabled at room init: slot 9 is
  `kind=other`/handler 0 after `aot_delete(AOT_09_ITEM, SCE_NONE, ...)`, even
  though `F_ITEM_24` is present. The armor switch is still `kind=event`, but
  `F_ROOM_20`/`F_ROOM_21` read false at entry, so pressing it runs
  `from_main_2` and leaves `F_ROOM_1D`/`F_SCENARIO_3A` clear while camera-2
  gas drains health. The route detects the dead item action before entering
  the gas and continues.
- The 20D Richard death entry scene returns the player to 204; the 20E door is
  armed only by `item_ck(IT_42_KEY_SERUM)`, which this route never holds. The
  route reports the 20D blocker and stops cleanly instead of looping through
  the scene.
- A locked 11B door is not a failed run: the route records the lock and stops
  in 11A with the lock, key, crest and inventory state in the finish reason.

## API

`on_bot_tick(bot)` runs once per app update, before the game tick.

| Call | Result |
| --- | --- |
| `bot:tick()` | Monotonic app-update counter. |
| `bot:room()` | `{id, stage, room, player_flag, mansion2, camera, controllable}`. |
| `bot:room_meta()` | The current room's authored `room/<id>/bot.xml` metadata, or `nil`. Read-only; see [Room metadata](#room-metadata). |
| `bot:world()` | The observed world model: rooms, doors, items, action presses and the frontier. Read-only; see [World model](#world-model). |
| `bot:risk()` | The `--bot-risk` setting, `0.0..1.0` (default `0.25`). Not used by the route yet. |
| `bot:player()` | `{x, y, z, angle, health, max_health, state, being_attacked, equipped, collision_flags, grabbed}`. `collision_flags` is the live health-status/collision byte (bit `0x10` skips shape-5 floor volumes). `grabbed` is the entity's grab zone flag. |
| `bot:scene()` | `{mode, skippable, film, transition, cutscene_skip, message, message_takes_input, pickup, frozen, controls_locked, controllable, dead}`. |
| `bot:actions()` | Array of `{slot, kind, x, z, width, depth, item, quantity, next_room, key, lock, handler, flags}`. |
| `bot:doors()` | Array of `{slot, target, next_room, key, lock, x, z, width, depth}`; `target` is the destination room id. |
| `bot:enemies()` | Array of active monsters (`id < 0x20`, `health > 0`), nearest first: `{slot, id, x, y, z, angle, bearing, health, state, status_flags, action_behavior, action_state, hit_state, distance, visible}`. `bearing` is the absolute 12-bit angle from the player; `distance` is horizontal; `visible` is the weapon line-of-sight result. Boss ids are included. |
| `bot:weapon()` | `{equipped, weapon, clip, reserve, behavior, state, aim_state, target}`. `equipped` is the inventory item (or nil), `weapon` is `equipped_weapon_id`, `clip` is the rounds in the equipped weapon's stack, `reserve` is `item_count(weapon + 9)`, and `target` is the locked entity slot or nil. |
| `bot:can_see(slot)` | Whether monster `slot` is in the current snapshot with a clear weapon sight line. |
| `bot:flag(bank, bit)` | A flag bit, read-only. |
| `bot:has_item(item)` / `bot:item_count(item)` | Inventory queries. |
| `bot:inventory()` | Read-only array of the live inventory slots: `{slot, item, quantity}` rows, including zero-quantity weapon slots. |
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
      items = { { slot = 4, item = 0x30, quantity = 1, present = false,
                  taken = true, taken_tick = 640, x = 12000, z = 14000 } },
      actions = { { slot = 3, kind = "event", presses = 1, last_press = 700,
                    message = 0x21, flags = { { bank = 4, bit = 0x1A } } } } },
    { id = "107", visited = false, entries = 0, doors = {}, items = {}, actions = {} },
  },
  frontier = {
    { kind = "door", room = "106", slot = 2, target = "108", target_visited = false,
      key = 0x30, locked = true, x = 3000, z = 3000 },
    { kind = "item", room = "107", slot = 3, item = 0x40, quantity = 1, x = 1000, z = 1000 },
  },
}
```

- `rooms` lists every room entered and every target of a seen door;
  `visited` is true once entered. `entries` counts room changes into it.
- A door is `locked` when it was locked the first time the bot saw it
  (lock bit `0x80` and its bank-2 flag clear), and `unlocked` once that flag
  is observed set. It is `traversed` when the room changes to its target
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
  taken; `locked` there is the door's current state.

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
| `<answer>` | `choice` (`yes` or `no`), `slot` and/or `bank`+`bit`, optional `label` | At the yes/no prompt opened by AOT `slot` (or identified by the flag), choose `choice`. At least one of `slot` or the flag is required; `bank` and `bit` go together. |
| `<avoid>` | `slot`, `reason` | The planner should not use AOT `slot`; `reason` is a non-empty note. |

```xml
<bot format="2">
  <interact slot="4" bank="1" bit="0x22" label="push the statue"/>
  <answer choice="no" slot="6" label="do not drink"/>
  <avoid slot="7" reason="the plant stuns the player"/>
</bot>
```

`bot:room_meta()` always has `interacts`, `answers` and `avoids` arrays
(empty for a format-1 file): `{slot, bank, bit, label}`,
`{choice, slot, bank, bit, label}` (absent fields are `nil`) and
`{slot, reason}`.

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
