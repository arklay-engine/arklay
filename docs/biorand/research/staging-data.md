# BioRand (staging) RE1: data-driven patches, mod output and config

Research notes for porting the RE1 part of BioRand into arklay. Source:
`~/src/biorand-classic`, branch `staging`. Paths below are relative to
`IntelOrca.Biohazard.BioRand/` unless stated. `Re1RdtPatcher.cs` is
catalogued elsewhere and is deliberately not repeated here. Cosmetic
features (enemy skins, voices, NPCs, player models, BGM, title call,
cutscene randomization) are out of scope.

All script offsets are **absolute byte offsets into the vanilla `.RDT`
file** (checked against `arklay scd export --list` listings of
`/home/ted/re1data/original/re1/JPN/STAGE*/ROOMxxxP.RDT`).

---

## 0. Which pipeline staging actually uses

Staging builds a Classic REbirth mod through the "classic" pipeline:

```
ClassicRandomizerFactory
  -> ClassicRandomizer (generic: Inventory/Door/Lock/Key/Item/Enemy randomizers)
  -> Re1ClassicRandomizerController (RE1 config, map tweaks, mod-general values)
  -> ModBuilder / ClassicMod (doors, items, enemies, inventories, general dict)
  -> RE1/Re1CrModBuilder (writes RDTs + supplementary files)
```

The older pipeline (`Re1Randomiser`, `Re1DoorHelper`, `Re1ItemHelper`,
`Re1EnemyHelper`, `DoorRandomiser`, `ItemRandomiser`, `EnemyRandomiser`,
`PlayGraph`, `Rdt.cs` `SetItem`/`SetDoorTarget`/`OverrideDoor` at
Rdt.cs:48-324) is still in the tree and reads the same `meta/re1/rdt.json`,
but **is not invoked by staging**. Several rdt.json fields are therefore
legacy-only: they are kept in the data but nothing in the staging path reads
them. The glossary marks each field.

Ordering in `ClassicRandomizer.Randomize` (ClassicRandomizer.cs:716-757):
Inventory -> Door (if `doors/random`) -> Lock (always runs) -> Key + Item (if
`items/random`) -> Enemy (if `enemies/random`) -> cosmetics.

---

## 1. rdt.json field glossary

`meta/re1/rdt.json` is 7452 lines, 122 room entries, `beginEndRooms`
start `106`, end `501`. The model is in `Map.cs` (551 lines).

### 1.1 Filtering (applies to every room, door, item, enemy entry)

`MapFilterable` (Map.cs:480-500): `tags`, `doorRando` (bool?), `player`
(int?), `scenario` (int?). An entry is dropped when a field is set and does
not equal the filter value. Re1CrModBuilder filters with
`new MapFilter(RandomDoors, Player, 0)` (Re1CrModBuilder.cs:40-49);
the controller filters the same way in `GetMap` (Re1ClassicRandomizerController.cs:141+).
So `doorRando: true` entries exist only when door rando is on, `doorRando: false`
only when it is off; `player: 0` = Chris only, `player: 1` = Jill only.

### 1.2 Map-level fields

| Field | Meaning | Consumer | Status |
|---|---|---|---|
| `beginEndRooms.start/end` | Start room (106) and goal room (501) of the play graph | DoorRandomizer, KeyRandomizer | staging |
| `items` (dict id -> def) | Item definitions, see 1.6 | everywhere | staging |
| `enemies` (groups) | Enemy groups, see 1.7 | EnemyRandomizer, Re1CrModBuilder.AddRequiredEsps | staging |
| `rooms` (dict "RRR" -> room) | Room entries, see 1.3 | everywhere | staging |

### 1.3 Room fields (`MapRoom`, Map.cs ~200-276)

| Field | Meaning | Consumer | Status |
|---|---|---|---|
| `name` | Display name, log only | logs | staging |
| `rdts` | RDT ids that implement this logical room (e.g. `["105","605"]` for both mansion visits). Every patch in the room is applied to every listed RDT. Door targets always use `Rdts[0]` (DoorRandomizer SetDoorTarget). | DoorRandomizer, Re1CrModBuilder Apply* | staging |
| `tags` | Room tags (`MapTags.cs`): `box`, `safe`, `save`, `prologue`, `segment-end`, `begin`, `end`, `connect-out`, `connect-back`, `lock-priority`; plus area tags used by the controller (`mansion`, `courtyard`, `caves`, `guardhouse`, `lab`, `mansion2able`, `plant42`...). `box`/`safe`/`save` ban enemies per config (`enemies/box`, `enemies/safe`, `enemies/save`). `exclude` (on 11C) is legacy-only. | DoorRandomizer, EnemyRandomizer, controller | staging |
| `doors` | `MapRoomDoor[]`, see 1.4 | | staging |
| `items` | `MapRoomItem[]`, see 1.5 | | staging |
| `enemies` | `MapRoomEnemies[]`, see 1.7 | | staging |
| `npcs`, `cutscenes`, actors' `offsets` | Cutscene/NPC actor data | CutsceneRandomizer / NPC randomizer | cosmetic, ignored |
| `doorRando` (`DoorRandoSpec[]`, Map.cs:230) | Legacy per-room door-rando spec | legacy Re1DoorHelper | legacy-only |
| `linkedRoom`, `requires` (room-level) | Legacy graph hints | PlayGraph | legacy-only |
| `flags` | not consumed in staging C# (grep finds no reader) | | unused |

### 1.4 Door fields (`MapRoomDoor`, Map.cs:278-327)

| Field | Meaning (from consumer) | Consumer | Status |
|---|---|---|---|
| `id` | AOT id of the door (`door_aot_set` +0x01). Key into `_mod.Doors[(rdtId,id)]`. | Re1CrModBuilder.ApplyDoors:478-508 | staging |
| `altId` | Second AOT id for the same logical door (door duplicated in script); gets the same rewrite | ApplyDoors | staging |
| `target` | `"RRR:id"` - logical destination door. `null` = door leads nowhere (treated as locked-from-this-side, lockType 255 in SetDoorLocks). | DoorRandomizer, LockRandomizer.SetDoorLocks:344-373 | staging |
| `entrance` `{x,y,z,d,floor,cut}` | Where the player spawns when arriving *through* this door. Overridden by `meta/re1/doors.csv` (`Target,X,Y,Z,D`) in GetMap. `cut` is stored but **Re1CrModBuilder never writes the camera** (door +0x0D/cut untouched). | DoorRandomizer.SetDoorTarget, controller GetMap | staging |
| `entranceId` | Which door id of the target room to use for the entrance | DoorRandomizer | staging |
| `randomize` | `false` = this connection is fixed; rooms joined by such doors form one "room piece" | DoorRandomizer | staging |
| `kind` (`DoorKinds.cs`) | `dynamic`, `blocked`, `unblock`, `locked`, `unlock`, `oneway`, `noreturn`. In SetDoorLocks `locked` -> lockType 255, `unlock` -> 254. `blocked` = unusable from this side; `unblock` = this side opens a blocked partner; `noreturn`/`oneway` drive segmenting. | DoorRandomizer, LockRandomizer, KeyRandomizer graph | staging |
| `lock` | Legacy lock descriptor | legacy | legacy-only |
| `lockId` (byte) | Lock flag index written to door +0x0E. 0 = no lock. | LockRandomizer.SetDoorLocks, ApplyDoors | staging |
| `lockKey` (int) | Item id that opens the door -> door +0x18 lockType = key id. Only written when a lockId exists (see 207:1 below). | SetDoorLocks | staging |
| `noUnlock` | Door can never be unlocked from this side: SetDoorLocks forces lockType 0; DoorRandomizer seals such doors as `blocked` instead of `locked`. | SetDoorLocks, DoorRandomizer | staging |
| `allowedLocks` | List of key item ids the lock randomizer may put on this door; `[]` = never lock it. Set at runtime by the controller for prologue, softlock-safe, preserve-mode doors. | LockRandomizer, controller | staging |
| `requires` | Legacy item requirement list | legacy PlayGraph | legacy-only |
| `requires2` | Requirement strings `item(N)`, `flag(NAME)`, `room(KEY)` parsed by `MapRequirement` | KeyRandomizer graph | staging |
| `requiresRoom` | Legacy | | legacy-only |
| `isBridgeEdge` | Marks an edge that must not be cut by segmenting | DoorRandomizer | staging |
| `ignoreInGraph` | Edge excluded from the key graph | KeyRandomizer | staging |
| `noReturn` | Legacy alias of kind noreturn | legacy | legacy-only |
| `condition`, `create`, `texture`, `special`, `offsets`, `cut` | Legacy door creation/override (Rdt.OverrideDoor). Not present on any staging door. | legacy | legacy-only |

### 1.5 Item fields (`MapRoomItem`, Map.cs:339-360)

| Field | Meaning | Consumer | Status |
|---|---|---|---|
| `globalId` | Item pickup flag (FG_ITEM index, `item_aot_set` +0x16). **This is the key**: `_mod.Items[globalId]`. ApplyItems rewrites every `item_aot_set` whose GlobalId matches, in every RDT of the room. | Re1CrModBuilder.ApplyItems:510-541 | staging |
| `id` | AOT id. Used only to find `aot_delete`(0x13)/`aot_on`(0x24) with `Data[0]==id` and SCE byte `0x0D` (document) and turn it into `0x04` (item). | ApplyItems | staging |
| `itemId` / `type` | Vanilla item type (log / original key identification) | KeyRandomizer, logs | staging |
| `amount` | Vanilla amount | logs | staging |
| `typeOffsets` (string[], hex) | Byte offsets in the RDT where the item **type** is hardcoded in script (usually `testpickup` operand). After items are assigned each offset is overwritten with the new type byte. | Re1CrModBuilder.ApplyPostPatches:448-476 | staging |
| `lockId` | Lock id the item itself is tied to (crank/jewel/emblem slots 30-35); reserved so the lock randomizer does not reuse it | LockRandomizer reserved set | staging |
| `link` | Item location linked to another (same pickup appearing twice) | KeyRandomizer/ItemRandomizer | staging |
| `priority` | Placement priority (low for optional/hidden/doc) | ItemRandomizer | staging |
| `requires2` | Requirements to reach this pickup (graph) | KeyRandomizer | staging |
| `requires`, `requiresRoom` | Legacy | | legacy-only |
| `allowDocuments` | Location may receive a document | ItemRandomizer | staging |
| `document` | Location is a document slot; skipped if `items/documents` off; keys only if `items/documents/keys` | ItemRandomizer, KeyRandomizer | staging |
| `hidden` | Hidden pickup; keys only if `items/hidden/keys` | KeyRandomizer, ItemRandomizer priority | staging |
| `optional` | Not part of key graph; low priority | KeyRandomizer, ItemRandomizer | staging |
| `group` | Bitmask restricting which item groups may land here (see controller groups §4.3) | KeyRandomizer | staging |
| `nop`, `offsets` | Legacy item rewrite | | legacy-only (not in data) |

### 1.6 Item definition fields (`MapItemDefinition`)

| Field | Meaning | Consumer |
|---|---|---|
| `name` | Display name | logs |
| `kind` | `weapon/...`, `ammo/...`, `health/...`, `ink`, `key`, `document`... drives every randomizer | all |
| `ammo` | Ammo item id(s) for a weapon | Inventory/ItemRandomizer |
| `max` | Max stack | ItemRandomizer stack options |
| `group` | Group bits | KeyRandomizer |
| `amount` | Key quantity placed (default 1) | KeyRandomizer |
| `discard` | Generic consumable key (51-60): amount = number of distinct-lockId doors in the segment subtree requiring it; used as the generic-key pool for lock rando | KeyRandomizer, controller |
| `implicit` | Player is assumed to have it; dropped from requirements (lockpick-Always makes 51 Sword and 61 Small Key implicit) | KeyRandomizer, controller |

Item ids (staging rdt.json): 1-10 weapons, 11-18 ammo, 27 V-JOLT, 28 Broken
Shotgun, 29 Square Crank, 30 Hex Crank, 31 Wood Emblem, 32 Gold Emblem, 33
Blue Jewel, 34 Red Jewel, 35 Music Notes, 38 Chemical, 39 Battery, 40 MO
Disc, 41 Wind Crest, 42 Flare, 43 Slides, 44 Moon Crest, 45 Star Crest, 46
Sun Crest, 47 Ink, 48 Lighter, 49 Lockpick, 51 Sword key, 52 Armor key, 53
Shield key, 54 Helmet key, 55 Master key, 56 Special key, 57 Door key 002,
58 003, 59 Control room key, 60 Power room key, 61 Small key, 62 Red book,
63/64 Doom books, 65 FAS, 67-75 herbs, maps 78/80/81/82, documents 95-109.

### 1.7 Enemy fields (`MapRoomEnemies`, Map.cs:362-382) and groups

| Field | Meaning | Consumer | Status |
|---|---|---|---|
| `id` | Existing `sce_em_set` id to retype in place (fixed enemy) | EnemyRandomizer, ApplyEnemies | staging |
| `globalId` | Kill flag of a fixed enemy; reserved from the kill-id bag | EnemyRandomizer | staging |
| `kind` | `npc` = reserved id, never touched | EnemyRandomizer, ApplyEnemies | staging |
| `includeTypes` / `excludeTypes` | Allowed/banned Re1EnemyIds for the room (first non-npc entry) | EnemyRandomizer | staging |
| `excludeOffsets` | Positions to skip | EnemyRandomizer | staging |
| `keepAi`, `keepPositions` | Keep vanilla state byte / positions | EnemyRandomizer | staging |
| `y` | Override Y for created enemies | EnemyRandomizer | staging |
| `maxDifficulty` (d) | Caps count: `max = max(1, round(max/(4-d)))` | EnemyRandomizer | staging |
| `condition` | ScdCondition wrapping the **created** `sce_em_set` block (first placement's condition) | ApplyEnemies | staging |
| `randomPlacements` | Use `meta/re1/enemy.csv` positions (Room,X,Y,Z,D,Include,Exclude) | EnemyRandomizer | staging |
| `nop` | Legacy nop range | legacy Re1EnemyHelper | legacy-only (one use: 108) |
| `restricted`, `ignoreRatio`, `keepState` | Legacy | | legacy-only |

Enemy group fields (`MapEnemyGroup`): `name`, `background`, `foreground`
(colours for UI), `entries[]{key, name, id[], esp[], poses[{name, pose, probability}]}`.
`id` = Re1EnemyIds the entry may produce; `esp` = embedded effect ids the
RDT must carry (copied in by AddRequiredEsps); `poses.pose` is written into
the `sce_em_set` State byte, chosen by `probability`.

| Group | ids | esp |
|---|---|---|
| zombie | 0,1,17 | 3,4 (+poses) |
| cerberus | 2 | 4 |
| spider | 3 | 30 |
| blacktiger | 4 | 30 |
| crow | 5 | 28 |
| hunter | 6 | 3,4 |
| wasp | 7 | - |
| chimera | 9 | 3,4,29,31 |
| snake | 10 | 4 |
| neptune | 11 | 23,37 |
| tyrant | 12 | - |

Re1EnemyIds: 0 Zombie, 1 ZombieNaked, 2 Cerberus, 3 WebSpinner, 4
BlackTiger, 5 Crow, 6 Hunter, 7 Wasp, 8 Plant42, 9 Chimera, 10 Snake, 11
Neptune, 12 Tyrant1, 13 Yawn1, 14 Plant42Roots, 15 Plant42Vines, 16 Tyrant2,
17 ZombieResearcher, 18 Yawn2, 19 Cobweb, 32+ NPCs.

### 1.8 ScdCondition (ScdCondition.cs: Generate1 31-131, Parse 358-408)

- Term `G:I` = flag group G index I is set; `!G:I` = clear. Terms joined only
  by `&&`; whole expression may be `!( ... )`. `$var==`/`!=` parses but throws
  NotImplemented for RE1.
- RE1 emission, positive: `if(0x01,len) ck(0x04 G I 1) ... <children> endif(0x03)`.
  Negated `!(...)`: `if ck... else_ck(0x02) <children>`. Children split into
  chunks of <=192 bytes, each wrapped.
- RE1 flag groups: 0 FG_SCENARIO, 1 FG_COMMON, 2 FG_LOCK, 3 FG_ENEMY, 4
  FG_ROOM, 5 FG_STATUS, 6 FG_RUN, 7 FG_ITEM (index = item globalId), 8 FG_MAP,
  9 FG_9.

### 1.9 RandomizedRdt primitives (Rdt.cs)

- `Nop(a,b)` (Rdt.cs:341-365): every opcode whose **start** offset lies in
  `[a,b]` becomes opcode `0x0E` with every data byte also `0x0E`.
- `Patches` `(offset, byte)`: applied in `Save()` (506-561) after opcodes are
  re-serialised.
- `AdditionalOpcodes` are prepended to InitSCD procedure 0;
  `AdditionalFrameOpcodes` to MainSCD procedure 0 (PrependOpcodes 593-633,
  AppendFrameOpcodes 635-713).

---

## 2. Every script-modifying room entry

### 2.1 `typeOffsets` (13 patches, applied by ApplyPostPatches)

Applied when items are randomized and the item's `globalId` is in
`_mod.Items`; the byte is set to the new item **type**, in **every** RDT of
the room (both mansion visits, both player files where the offset applies).

| # | Room / item (gid) | Offset | Filter | Vanilla opcode at offset-1 (or -3) | Why |
|---|---|---|---|---|---|
| 1 | 105 WOOD EMBLEM (gid 42) | 0x2831F | player 0 | `testpickup WOOD_EMBLEM` @0x2831E -> `set FG_COMMON 167,1` | Dining-room emblem swap: script detects you took the wood emblem by type |
| 2 | 105 WOOD EMBLEM (gid 42) | 0x2848B | player 1 | same @0x2848A | Jill file |
| 3 | 106 GUN ON FLOOR (gid 26) | 0x2FDDD | player 0 | `testpickup BERETTA` -> `set FG_ROOM 3,0` | Chris finds Jill's Beretta in main hall; flag drives the cutscene |
| 4 | 10D BLUE EYE (gid 174) | 0x270F | both, 10D/60D | `cmpb 19,0,41` @0x270C | Tiger statue: var 19 holds the type picked, compared to Wind Crest |
| 5 | 10D RED EYE (gid 175) | 0x26E7 | both, 10D/60D | `cmpb 19,0,5` @0x26E4 | Tiger statue: compared to Colt Python (MAGNUM) |
| 6 | 10F BEHIND SLIDING DOOR (gid 21) | 0x30C45 | player 0 | `testpickup GOLD_EMBLEM` -> `set FG_COMMON 163,0` + `evt_exec event_0A` | Gold emblem behind fireplace clock: take triggers the wall closing |
| 7 | 10F BEHIND SLIDING DOOR (gid 21) | 0x30BB9 | player 1 | same | Jill file |
| 8 | 112 INK (gid 212) | 0x17969 | both | `testpickup INK_RIBBON` -> `aot_delete 5` | Removes the duplicate AOT after pickup |
| 9 | 113 IN BATHTUB (gid 3) | 0x1CBC5 | both | `testpickup SMALL_KEY` -> `aot_delete 4` | Bathtub drain key; removes the "drain" AOT |
| 10 | 116 SHOTGUN FRAME (gid 0) | 0x1FEFD | both | `testpickup SHOTGUN` -> `evt_exec event_00` | Shotgun-on-wall ceiling trap trigger |
| 11 | 219 BATTERY (gid 57) | 0x14B51 | file 719 (219 is a copy, see g_missingRooms) | `testpickup BATTERY` -> `aot_delete 4` | Guardhouse/courtyard battery |
| 12 | 40A V-JOLT BOOK (gid 78) | 0x39383 | both | `testpickup ITEM_69` -> `evt_exec event_06` | Lab book pickup triggers event |
| 13 | (count includes the 10F doorRando variants: one entry `doorRando:false`, another `doorRando:true, optional:true`, same offsets) | | | | |

(Row 13 is a note: there are 12 distinct offsets in 13 typeOffsets entries.)

### 2.2 Item-type puzzle details

**10D tiger statue (both visits, both players).** Init places two
`item_aot_set`s on the statue: id 131 = Wind Crest (blue eye), id 4 =
Colt Python x6 (red eye). Main SCD tests `FG_ROOM 24` and scenario flags
14/15, then compares the picked-up type held in variable 19 with `cmpb 19,0,<type>`
(0x26E4 red: 5, 0x270C blue: 41). On match: `aot_delete 2`, sets the room
flag, `cutnext 1`, `evt_exec`. typeOffsets rewrite the compared type so the
correct branch fires for the randomized item. Note: the statue's *insertion*
check (testitem of the jewel being *used*) is not type-patched; jewels are
keys and keep their identity.

**116 / 115 shotgun.** 116 init has `item_aot_set` gid 0 (Shotgun). Main has
`testpickup SHOTGUN` @0x1FEFC -> `evt_exec event_00` (ceiling trap), patched
by typeOffsets 0x1FEFD. There is also an **unpatched** `testitem ITEM_SHOTGUN`
@0x1FF0C (checks the item being *used* - placing the Broken Shotgun back).
115 (the adjoining room) has dynamic door locks 15/16 set by script during
the trap; they are kept as vanilla.

**105 dining room / 10F gold emblem.** Wood emblem pickup sets FG_COMMON 167
(swap state); gold emblem pickup in 10F sets FG_COMMON 163,0 and runs
event_0A (wall slides shut). Both type-checked, both patched.

**205 armoury.** No typeOffsets and no rdt.json script field; the
armor-room puzzle (statues / gas) is handled by Re1RdtPatcher (catalogued
separately) or not at all.

**Unpatched type checks** (testpickup with a hardcoded type and no
typeOffsets; if the randomized item differs the script branch never fires).
Cross-check against Re1RdtPatcher:

| RDT | Offset | Opcode | Effect |
|---|---|---|---|
| 1060 | 0x2FE02 | `testpickup ITEM_54` (Helmet key) | evt_exec event_0C |
| 3080 | 0x1B098 | `testpickup FLAMETHROWER` | `set FG_LOCK 40,1` + event_00 |
| 30B0/30B1 | 0x2E91A | `testpickup FLAMETHROWER` | `set FG_LOCK 39`, `set FG_ITEM 172` |
| 30D0 | 0x0502, 0x051E | `testpickup FLAMETHROWER` | |
| 50A0/50A1 | 0x28EE8 | `testpickup ITEM_68` (green herb) | evt_exec event_09 |

About 99 `testitem` (0x10) opcodes exist; they test the item being *used*
(keys, cranks, jewels), which keep their type, so they need no patch.
`cmpb 16,...` checks are player checks, not item checks.

### 2.3 Enemy `condition` (25 entries; wrap created `sce_em_set`s)

Applies only when `enemies/random` and the room gets created placements.

| Room | Condition | Player | Likely why |
|---|---|---|---|
| 100 | `1:34` | 1 | only after event flag (Jill) |
| 101 | `!(!1:34 && 1:35)` | 0 | suppress during scripted state |
| 106 | `!1:0 && !1:2 && !1:3` | 0 | not during prologue (Chris) |
| 106 | `!1:5 && !7:13` | 1 | not during prologue (Jill) / before item 13 taken |
| 109 | `!(!1:32 && 1:33)` | 1 | |
| 115 | `!(7:0 && 7:1 && 1:32)` | 0 | shotgun trap (items gid 0/1) state |
| 115 | `!(7:0 && 7:1)` | 1 | same, Jill |
| 206 | `1:103 && 1:104` | 0 | |
| 20A | `!(1:8 && 1:98 && 1:7)` | 1 | |
| 20E | `1:67` | both | |
| 304 | `!(!0:0 && 0:24)` | both | |
| 307 | `!(!1:92 && !1:72)` | 1 | |
| 308 | `4:0` | 1 | |
| 309 | `!1:72` | 1 | |
| 405 | `!(1:171 && !0:0)` | 0 | |
| 405 | `!(1:175 && !0:0)` | 1 | |
| 406 | `!(1:178 && !1:174 && 1:176 && 1:177)` | 1 | |
| 408 | `!(!0:0 && 1:170)` | 0 | |
| 409 | `!(!0:0 && 1:58)` | 0 | |
| 500 | `!(!0:55 && !1:75)` | 0 | |
| 500 | `0:55` | 1 | after lab self-destruct start |
| 502 | `1:75` | 0 | |
| 508 | `!(1:172 && !1:75)` | both | |
| 50C | `0:45` | both | |
| 511 | `!(!1:90 && 0:43)` | 0 | |

Flag 0:0 is FG_SCENARIO 0 (scenario/character); 0:55 is the self-destruct
timer flag (also used by DisableCutscenes for 513).

### 2.4 Fixed enemies retyped in place (`id` set)

104 id0 gid28; 10E id0; 208 id0 gid23; 20C id0 gid62 includeTypes
[11,12,18] (Yawn2 room -> Neptune/Tyrant/Yawn2); 210 id0 gid24 includeTypes
[4,11,12,13] (Yawn1 attic); 21C id0 gid107, id1 gid108; 30A id1 gid80, id2
gid81; 30B id0 gid122, id1 gid123. NPC-kind ids are reserved.

### 2.5 Legacy-only `nop`

108 enemy `nop: ["0x19754-0x197EE"]`, read only by the legacy Re1EnemyHelper.
**Not applied in staging.** (Staging instead nops removable enemies by type,
§3.4.)

### 2.6 Door entries that change behaviour

Filter-duplicated doors (different entry by `doorRando` / `player`):
106:3, 200:0/1, 20C, 300:1, 302:3, 308:0/1/2, 30B:0, 408:3, 40C:0, 505:2,
507 (STORE):0.

`entrance` overrides in JSON: 106:3 (doorRando true), 214 doors 0/1/1/5,
303:8, 305:0. doors.csv overrides 303:8 again. 303:8 does not exist in
vanilla; Re1RdtPatcher creates it (Re1RdtPatcher.cs:608).

**Lock differences even with locks and doors NOT randomized** (because
`LockRandomizer.SetDoorLocks` always runs and ApplyDoors always writes
LockId/LockType for every listed door):

| Door | Vanilla (+0x0E / +0x18) | Written | Effect |
|---|---|---|---|
| 10F:0 | 6 / Sword(51) | 6 / 0 | unlocked from inside |
| 201:1 | 0xFF / 0 | 17 / 255 | locked from this side |
| 207:1 | 0xD0 (lock 16 + 0xC0) / Armor(52) | 0 / 0 | **Armor key door becomes unlocked**: JSON has `lockKey` 52 but no `lockId`; graph still requires item 52. Likely a bug. |
| 305:0 | 0 / 0 | 2 / 255 | locked from this side |
| 50B:1 | 0xAC (lock 44 + 0x80) / Master(55) | 0 / 0 | kind `unblock`, requires room(513) |
| all listed doors | 0x40 / 0x80 bits in +0x0E | stripped | lock id written as plain 8-bit index |

---

## 3. What Re1CrModBuilder writes (RE1/Re1CrModBuilder.cs, 1343 lines)

### 3.1 Files in the mod

| File | Source | Where |
|---|---|---|
| `manifest.txt` | `[MOD]\r\nName = ...\r\nModule = biorand.dll` | ClassicRebirthModBuilder.cs:16-26 |
| `description.txt` | optional, CRLF-normalised | ClassicRebirthModBuilder.cs:45-54 |
| `biorand.dll` | CR module (biorandmod/re1.cpp) | Create() 57-76 |
| `config.json` | the RandomizerConfiguration (if that type) | Create() |
| `generated.json` | `_mod.ToJson()` (doors, items, enemies, inventories, general) | Create() |
| `log.md`, `log.html` | human log | Create() |
| `STAGEn/ROOMrrrP.RDT` | patched RDTs, written to `OriginalPath` | WriteRdts 137-166 |
| `misc.xml` | copy of meta/re1/misc.xml (message strings) | AddMiscXml |
| `tables/room_XXX.xml` | per-room sound tables from sounds.xml, enemy block picked from the first enemy placement's type | AddSoundXml 1159-1255 |
| `init.xml` | starting inventories | AddInventoryXml 741-786 |
| `biorand.dat` | dynamic tweaks | AddDynamicTweaks 1147-1151 |
| `scripts/`, `scripts_modded/` | only with env `BIORAND_DEBUG_SCRIPTS` | WriteRdts |

Write order (Write() 121-135): WriteRdts, AddMiscXml, AddSoundXml,
AddInventoryXml, cosmetics, AddDynamicTweaks.

### 3.2 RDT loading (GetGameData 221-268)

Loads STAGE1..7 files matching `ROOMxxxP.RDT` (case-insensitive) for the
chosen player. For `g_missingRooms` (1300-1316: 110, 119, 200, 20C, 213,
214, 215, 216, 217, 218, 219, 21A, 21B, 21C) the vanilla file is a stub <16
bytes, so the file of stage+5 (6xx/7xx) is copied and saved under the 1xx/2xx
name. `OriginalPath = STAGE{s+1}/ROOM{id}{P}.RDT`.

Per-RDT pipeline (WriteRdts): `Re1RdtPatcher.Patch` -> `DisableCutscenes`
(if `cutscenes/disable`) -> `ApplyPostPatches` -> `ApplyDoors` ->
`ApplyItems` -> `ApplyEnemies` -> `ApplyNpcs` (cosmetic) -> `Save`.

### 3.3 Doors (ApplyDoors 478-508)

For every `door_aot_set` (0x0C, 26 bytes) whose `(rdtId, id)` is in
`_mod.Doors`:

- If `Target` set: `+0x0F` = target room byte `((stage+1)<<5)|room` (stage
  bits 0 = same stage), `+0x10/12/14/16` NextX/Y/Z/D = target entrance. Camera
  (`+0x0D`) not touched. Target room = target logical room's `Rdts[0]`.
- If `Lock` set: `+0x0E` LockId = lock.Id, `+0x18` LockType = lock.Type.
  **Else LockId = 0, LockType = 0.** So every door in `_mod.Doors` has its
  vanilla lock overwritten.

Door layout: +0 op, +1 id, +2 x, +4 z, +6 w, +8 d, +0x0A special, +0x0B unkB,
+0x0C animation, +0x0D unkC/cut, +0x0E lockId, +0x0F target, +0x10 nextX,
+0x12 nextY, +0x14 nextZ, +0x16 nextD, +0x18 lockType, +0x19 free.
LockType: 0 unlocked, 255 locked (opens from other side), 254 unlock (this
side unlocks), else key item id.

Lock data source: `LockRandomizer.SetDoorLocks` (LockRandomizer.cs:344-373),
always: `noUnlock` -> 0; `target==null` -> 255; kind `locked` -> 255; kind
`unlock` -> 254; `lockKey` -> key; else 0. `lockId 0` => null lock.

Lock ids: random locks take new ids from 1..254 minus reserved (vanilla
lockIds in use, item lockIds 30-35). Door rando sealed doors: `noUnlock` ->
kind `blocked`; otherwise lockId 255 + kind `locked`. Connect() mirrors lock
data to the opposite side; the Unblock-side workaround gives the partner
type 255.

### 3.4 Lock flags and biorand.dll

Vanilla: lock byte = `0x80 | id` with `0x40` = "Rebecca cannot enter" (e.g.
0x92 = lock 18; 0xCB = lock 11 + 0x40; 0xD0 = lock 16 + 0xC0). The exe masks
these bits off and indexes a small flag array.

biorand.dll `IncreaseLockLimit` (biorandmod/re1.cpp):
- Disables the 0x40 Rebecca test (0x455B4C).
- 0x455B73 -> 0xFF ("0 means unlocked, not ~0x80").
- Nops the bit-stripping masks at 0x455B7A-7D and 0x455C81-83.
- Relocates the lock flags to `CustomData.flag_locks[8]` (256 bits), patched
  references at 0x417F83, 0x418066, 0x4520B5, 0x4520D4, 0x455B83, 0x455C87,
  0x455FDF, 0x4562CB, 0x456483. Saved in the savegame (magic `BIORAND`, v1);
  PostGameInit hook at 0x48DB4A.

Result: staging lock ids are full 8-bit indices 1..254 (255 used for
sealed). arklay needs a 256-bit lock flag array and must treat `+0x0E` as a
plain index for randomized RDTs.

Other dll patches (gameplay-relevant): DisableDemo, FixFlamethrowerCombine,
FixWasteHeal, FixNeptuneDamage, FixYawnPoison, UpdateMixes (blue herb mix
table 28), RemoveDrawerInkHack, ChangeInkToFlag122 (scenario flag 122 =
"ink required" for both characters; 123 stays NG+/hard), EnableChrisLockpick
(nops 0x455BA4-BAF and 0x455BCB-BD5 so Chris can use the lockpick and Jill
the sword key), UpdateInventorySize (reads `_inventorySize`), DoDynamicTweaks
(reads `d:\horr\jpn\biorand.dat`).

### 3.5 Items (ApplyItems 510-541, ApplyPostPatches 448-476)

- Every `item_aot_set` (0x18) whose `+0x16` GlobalId is in `_mod.Items`:
  `+0x0A` Type and `+0x0B` Amount rewritten. Nothing else (position, size,
  take animation unchanged).
- For every `aot_delete` (0x13) / `aot_on` (0x24) in the same RDT with
  `Data[0]==item.id && Data[1]==0x0D` (SCE_DOCUMENT): Data[1] = 0x04
  (SCE_ITEM), so a document slot can hold an item. RDTs containing such
  opcodes: 10E, 20A, 307 (p1), 507, 40A, 502 (p1), 509, 60E, 510, 715, 70A,
  706, 70C (p1).
- typeOffsets bytes, §2.1.
- Leftover slots get `Item(0,0)` from ItemRandomizer (type 0 = nothing).

item_aot_set layout: +1 id, +2 x, +4 y, +6 w, +8 h, +0x0A type, +0x0B
amount, +0x0C..0x13 unknown, +0x14/0x15 unknown, +0x16 globalId, +0x17
unknown, +0x18 takeAnimation, +0x19 unknown.

### 3.6 Enemies (ApplyEnemies 543-723, only if `enemies/random`)

1. For each RDT whose room has a non-empty `enemies` list: nop every
   `sce_em_set` (0x1B, 22 bytes) whose type is in CanRemoveEnemy (Zombie,
   ZombieNaked, Cerberus, WebSpinner, BlackTiger, Crow, Hunter, Wasp, Chimera,
   Snake, Neptune, Tyrant1, Plant42Vines, ZombieResearcher) and whose id is
   not reserved (fixed/NPC).
2. Placements with `Create`: new `sce_em_set` Length 22, Type, State = pose,
   KillId = globalId (from bag 0..254 minus fixed globalIds), Unk04=1,
   Unk05=2, D, X, Y, Z, Id. All wrapped in the first placement's ScdCondition
   and prepended to InitSCD.
3. Placements on an existing id: retype; State = 0 for Snake/WebSpinner, else
   `State & 0x80`.
4. AddRequiredEsps: missing embedded ESP effects (from the group `esp` list)
   are copied in from any RDT that has them.

EnemyRandomizer: one enemy type per room; positions from enemy.csv; with
mansion2 Always, 1xx and 6xx RDTs are randomized separately.

### 3.7 Initial inventory (AddInventoryXml 741-786)

`init.xml`: `<Init><Player>` Jill node (8 entries) then Chris node (12
entries), each `<Entry id count>`. The non-chosen character gets only
`CombatKnife x1`; the chosen one gets `inventories[0].WithSize(12 or 8)`.

### 3.8 DisableCutscenes (276-446, only with `cutscenes/disable`)

Injected flag clears (opcode 0x05 `set g,i,0`) in room 106 AdditionalOpcodes,
wrapped in `ScdCondition "1:0"`:

- Chris (13 flags): (1,0) (1,2) (1,3) (1,36) (1,69) (1,72) (1,100) (1,167)
  (1,170) (1,171) (0,101) (0,127) (0,192).
- Jill: (0,101) (0,127) (1,0) (1,2) (1,3) (1,5) (1,7) (1,69) (1,72) (1,86)
  (1,97) (1,103) (1,161) (1,170) (1,172) (1,173) (1,175) (1,192).

Nops / byte patches:

| Player | RDT | Change |
|---|---|---|
| Chris | 60A | nop 0xF702 |
| Chris | 601 | nop 0x24CAA-0x24E16, 0x24E1E-0x24E8A |
| Chris | 706 | nop 0x3785C-0x3796A, 0x37972-0x379F6 |
| Chris | 512 | nop 0xAF4C-0xAF8E |
| Chris | 514 | nop 0xEFCE-0xF048 |
| Chris | 513 | patch 0x1C4AB=0, 0x1C4AC=55, 0x1C6E9=0, 0x1C6EA=55; nop 0x1C484, 0x1C6B0 |
| Jill | 40C | nop 0x64C8, 0x64D4 |
| Jill | 20C, 70C | patch 0x96F8=0x2C; nop 0x9704, 0x970A |
| Jill | 514 | nop 0xEFCE-0xF0C6 |
| Jill | 512 | nop 0xAF4C-0xAF74 |
| Jill | 513 | nop 0x1C484; patch 0x1C4AB=0, 0x1C4AC=55, 0x1C7C9=0, 0x1C7CA=55; nop 0x1C724 |
| both | 513 | add `sce_em_set` Type 0x0C (Tyrant1), KillId 112, Unk04=1, Unk05=2, D 3072, X 10700, Y 0, Z 7000, Id 1; frame opcodes under `0:55 && 4:11`: `set 4,11,0`, raw `0x16 [00]`, `0x16 [01]`, `0x15 [02]` |

The 513 byte pairs `(0,55)` retarget a flag check to FG_SCENARIO 55
(self-destruct).

### 3.9 Dynamic tweaks (biorand.dat, DynamicTweaksBuilder 1320-1342)

Records `[u32 kind][u32 len][data]`:
- kind 1 TWEAK_HITSCAN: 12 shorts
  `-2026,-1656,-2530,-2280,-2040,-1800,-1917,-1617,-2190,-1940,-2003,-1720`
  (FixWeaponHitScan 936-965), written by the dll to 0x4AAD98.
- kind 2 TWEAK_INVENTORY_SIZE: 1 byte (6 or 8).

### 3.10 Mod "general" values (ApplyModGeneral, controller 600-616)

`player`, `hard`, `randomDoors`, `randomItems`, `randomEnemies`,
`cutscenesDisabled`, `forceTyrant` (tyrant2 == Always), `lockpick`, `ink`,
`inventorySize`. Re1CrModBuilder reads player, inventorySize, randomDoors,
randomEnemies, cutscenesDisabled (51-55).

---

## 4. Config options

### 4.1 RE1-specific (Re1ClassicRandomizerController.cs:8-133)

| Id | Label / type | Options / range | Default | Notes |
|---|---|---|---|---|
| `player` | dropdown | Chris, Jill, Random | Random | GetVariation: 0 if Chris else 1 |
| `cutscenes/disable` | switch | | false | "Disables all cutscenes keeping the randomizer completely gameplay focused." |
| `ink/enable` | "Requires Ink Ribbons to Save", dropdown | Default, Never, Always, Random | Always | desc: Default = Chris yes, Jill no |
| `hard` | "Hard Mode", switch | | false | "equivalent to new game+" |
| `progression/guardhouse` | dropdown | Random, Always, Never | Always | group "Progression (non-door randomizer)" inserted at index 1 |
| `progression/mansion2` | dropdown | Random, Always, Never | Always | requires guardhouse (throws otherwise) |
| `progression/caves` | dropdown | Always, Always (Segmented) | Always (Segmented) | |
| `progression/lab` | dropdown | Random, Random (Segmented), Always, Always (Segmented), Never | Always (Segmented) | |
| `progression/tyrant1` | dropdown | Random, Always, Never | Random | |
| `progression/tyrant2` | dropdown | Random, Always, Never | Random | Always -> forceTyrant |
| `inventory/special/lockpick` | dropdown (Inventory/Main) | Default, Random, Never, Always | Default | |
| `inventory/size` | dropdown | Default, Random, 6, 8 | Default | desc: Chris 6, Jill 8 |

ApplyConfigModifications (536-554): Random dropdowns resolved; ink Default
-> Always for Chris, Never for Jill; anything not Always zeroes
`inventory/ink/min/max` and `items/distribution/ink`; size Default -> 6
Chris / 8 Jill; lockpick Default -> Never Chris / Always Jill. Lockpick
Always makes items 51 and 61 implicit.

### 4.2 Shared (ClassicRandomizer.cs)

| Id | Type | Range / step | Default |
|---|---|---|---|
| `doors/random` | switch | | true |
| `doors/segments` | range | 1-6 | 3 |
| `doors/rooms` | percent | 0-1 step 0.1 | 0.4 |
| `locks/random` | switch | | false |
| `locks/preserve` | switch | | false |
| `locks/ratio` | percent | step 0.01 | 0.25 |
| `inventory/weapon/knife` | dropdown | Random, Never, Always | Always |
| `inventory/min` | range | 0-8 | 4 |
| `inventory/max` | range | 0-8 | 8 |
| `inventory/{primary,secondary}/none` | switch | | true |
| `inventory/{primary,secondary}/{weaponKind}` | switch (knife excluded) | | true |
| `inventory/{p}/ammo/min` | percent | 0-8 step 0.1 | 1 |
| `inventory/{p}/ammo/max` | percent | 0-8 step 0.1 | 2 |
| `inventory/{healthKind}/min` | range | 0-10 | 0 |
| `inventory/{healthKind}/max` | range | 0-10 | 2 |
| `inventory/ink/min` | range | 0-32 | 0 |
| `inventory/ink/max` | range | 0-32 | 3 |
| `items/random` | switch | | true |
| `items/hidden/keys` | switch | | true |
| `items/documents` | switch | | true |
| `items/documents/keys` | switch | | true |
| `items/weapon/enabled/{kind}` | switch | | true |
| `items/weapon/initial/min/{kind}` | range | 0..max | 0 |
| `items/weapon/initial/max/{kind}` | range | 0..max | max |
| `items/stack/min/{kind}` | range | 1..max | 1 |
| `items/stack/max/{kind}` | range | 1..max | 15 (ammo, ink) |
| `items/distribution/{kind}` | percent | 0-1 step 0.01 | 0.5 (ammo, health, ink) |
| `enemies/random` | switch | | true |
| `enemies/safe` | switch | | false |
| `enemies/save` | switch | | true |
| `enemies/box` | switch | | true |
| `enemies/rooms` | percent | | 0.5 |
| `enemies/distribution/{key}` | percent | | 0.5 |
| `enemies/min/{key}` | range | 1-16 | 1 |
| `enemies/max/{key}` | range | 1-16 | 6 |

Cosmetic ids (skipped): `enemies/skin/*`, `protagonist/random`,
`protagonist/character/*`, `npc/random`, `npc/character/*`, `music/random`,
`music/game/*`, `title/call/random`.

### 4.3 Map tweaks done by GetMap (141-524)

- Group bits: ALL -1, MANSION_1 1, MANSION_2 2, GUARDHOUSE 8, LAB 32,
  PLANT_42 64, SMALL_KEY 128, BATTERY 256, LAB_TYRANT 512.
- Not cutscene-disabled: prologue doors get `allowedLocks []`.
- No door rando:
  - guardhouse Never: guardhouse rooms removed, 302 "GATE TO GUARDHOUSE"
    target null.
  - lab on: 305 "DOOR TO HELIPORT" kind locked, allowedLocks []; 303 "DOOR TO
    FOUNTAIN" kind unlock + ignoreInGraph.
  - lab Never: lab rooms removed; 305 "FOUNTAIN STAIRS" target null,
    requires2 []; heliport door kinds cleared; 303 "LIFT TO LAB" target null.
  - softlock-safe doors and partners: allowedLocks [].
  - Groups: all items -1; mansion2: plant42 "KEY IN FIREPLACE" and the
    mansion-2 key group 64; battery (39) group 256 on mansion2 and lab
    items; lab + tyrant1: flare (42) group 512.
  - caves Segmented: 302 "LADDER TO CAVES" noreturn, other side blocked.
    lab Segmented: 305 "FOUNTAIN STAIRS" noreturn, other side blocked.
- `locks/random`: unless preserve, vanilla locked/unlock kinds and lockIds
  are cleared where both sides have allowedLocks null. Generic keys = items
  with `discard`. Door rando: every door may take any generic key. Otherwise:
  preserve drops the helmet key from the pool; mansion2 on picks a mansion-2
  key (not the sword key when lockpick on), tags `rng.Next(2,9)`
  `mansion2able` rooms as group 2, gives their doors a fresh lockId (1..254
  free), `lockKey` = that key and `requires2 item(key)` both sides. Area
  tag sets `[mansion,courtyard]`, `[caves]`, `[guardhouse]`, `[lab]` are
  assigned per lock via a random 4-bit mask.

---

## 5. Counts

- typeOffsets: 13 entries, 12 distinct (room, offset) byte patches.
- Enemy conditions: 25.
- Legacy-only nop: 1 (108), not applied.
- DisableCutscenes: ~20 nop/patch operations over 9 RDTs + 1 created Tyrant
  + 2 injected flag-clear blocks (13 and 18 flags).
- Lock changes vs vanilla with locks off: 5 doors + 0x40/0x80 bits stripped
  on every listed door.
- Unpatched type-checked pickups: 6 sites in 5 RDT families.
- Document -> item AOT conversions: 13 RDTs.
