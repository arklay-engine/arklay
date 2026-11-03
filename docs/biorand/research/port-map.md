# BioRand RE1 "classic" randomizer: port map for arklay

Research notes for porting BioRand's new RE1 randomization pipeline (C#) to a
Rust cargo package inside arklay. This is read-only research: no Rust yet.
Seed parity with C# is **not** a goal. Correctness and behaviour are.

## Sources and versions

| Abbreviation | Path | Revision |
|---|---|---|
| `BR/` | `~/src/biorand-classic/IntelOrca.Biohazard.BioRand/` | branch `staging`, HEAD `9f862154` ("Fix door rando with lock rando disabled") |
| `BC/` | `~/src/biorand-common/src/IntelOrca.Biohazard.BioRand.Common/` | **tag `v1.3.1`** (`14a79616`, 2025-09-11), see below |
| `BU/` | `~/src/biorand-classic/biohazard-utils/src/IntelOrca.Biohazard/` | submodule at staging |
| `meta/re1/` | `BR/meta/re1/` | data: `rdt.json`, `doors.csv`, `enemy.csv`, `bgm.json`, ... |

**Common library version.** `BR/IntelOrca.Biohazard.BioRand.csproj:10`
references `IntelOrca.Biohazard.BioRand.Common` 1.3.1. In `~/src/biorand-common`,
`main` is at 1.6.1; the tag `v1.3.1` carries `<Version>1.3.1</Version>`
(`BC/IntelOrca.Biohazard.BioRand.Common.csproj:6`; tag message "Fix edge case
with route finder around validation and consumable keys"). All `BC/` line
numbers below are from `git archive v1.3.1`. The 1.3.1 tree differs from `main`
in ways that matter:

- 1.3.1 has **no** `Rng.cs`, `EndlessBag.cs` or `Routing/DeadEndInfo.cs`. Those
  came later. So the `Rng` and `EndlessBag` used by staging are biorand-classic's
  own: `BR/Rng.cs`, `BR/EndlessBag.cs`.
- `RouteFinderOptions.DebugDeadendCallback` is `Action<object>` and receives the
  internal `State` (`BC/Routing/RouteFinderOptions.cs:5-9`). `main` passes a
  `DeadEndInfo` instead.
- `main` also adds graph serialization, an "advanced" flag and a "multiplier" on
  config items, and a `RouteSolver` two-way fix (`efa189e`). None of that is in
  1.3.1.

**Which pipeline RE1 uses.** The current entry points are the `biorand-classic`
CLI and agent: `biorand-classic/Program.cs:62-70`,
`Commands/GenerateCommand.cs:61`, `Commands/AgentCommand.cs:60`. They call
`ClassicRandomizerFactory.Default.Create(BioVersion.Biohazard1, …)`
(`BR/ClassicRandomizerFactory.cs:10-25`). That builds `ClassicRandomizer` with
`Re1ClassicRandomizerController`, plus `Re1CrModBuilder` for the output.
The old pipeline (`RE1/Re1Randomiser.cs` on top of `BaseRandomiser.cs`,
`DoorRandomiser.cs`, `ItemRandomiser.cs`, `EnemyRandomiser.cs`, `RandoConfig`,
`PlayGraph`, `IDoorHelper`, `IItemHelper`, `IEnemyHelper`, and
`Re1DoorHelper`/`Re1ItemHelper`/`Re1EnemyHelper`) is reached only from the
legacy WPF app (`biorand/MainWindow.xaml.cs:995`), `BaseRandomiser.cs:517`,
and the tests (`IntelOrca.Biohazard.BioRand.Tests/TestRandomizer.cs:135`).
The new pipeline touches the old code in three small places only:

- `EnemyRandomizer` borrows the CSV row type `EnemyRandomiser.EnemyPosition`
  (`BR/EnemyRandomizer.cs:36`, defined at `BR/EnemyRandomiser.cs:982-1031`).
- `Re1DoorHelper` reads `Re1Randomiser.MissingRooms`. That is old-pipeline only.
- `Re1ItemIds` and `Re1EnemyIds` are shared constant tables. The new pipeline
  uses them (`HelmetKey`, `SwordKey`, enemy ids in the mod builder).

---

## 1. Pipeline walkthrough (config + seed → mod)

### 1.1 Call sequence

`ClassicRandomizer.Randomize(RandomizerInput)` (`BR/ClassicRandomizer.cs:677-714`):

1. `RandomizeToMod(input)` (`:649-656`):
   1. `CreateModBuilder(input)` (`:658-675`) sets `Game="re1"`,
      `Name="BioRand | {profile} | {seed}"`, the description,
      `Seed` and `Configuration`. The stored configuration is the **unmodified
      input**, not the resolved copy.
   2. `new Context(ConfigurationDefinition, input.Configuration.Clone(), dataManager, seed)`.
      `ConfigurationDefinition` is rebuilt on every access (`:17`). It reads
      `rdt.json` and the data directories.
   3. `controller.ApplyConfigModifications(context, modBuilder)`
      (`RE1/Re1ClassicRandomizerController.cs:536-554`):
      - `ApplyRandomConfigOptions` (`:556-581`), rng `"config"`. Every
        `dropdown` item whose value is `"Random"` gets a random option other than
        `Random`, `Random (Segmented)` and `Always (Segmented)`. A value of
        `"Random (Segmented)"` gets a random option other than `Random`,
        `Random (Segmented)` and `Always`. Items are visited in definition order.
      - `UpdateConfigNeverAlways` (`:583-598`) resolves `"Default"` by player:
        `ink/enable` (Chris `Always`, Jill `Never`), `inventory/size` (`6`/`8`),
        `inventory/special/lockpick` (`Never`/`Always`). If ink is not `Always`,
        it forces `inventory/ink/min`, `inventory/ink/max` and
        `items/distribution/ink` to 0.
      - `ApplyModGeneral` (`:600-616`) writes `ModBuilder.General` (see §5).
   4. `Randomize(context, modBuilder)` (`BR/ClassicRandomizer.cs:716-757`):
      1. `controller.GetVariation(context)` (`Re1ClassicRandomizerController.cs:135-139`)
         sets `playerIndex = player=="Chris" ? 0 : 1` and calls
         `GetMap(context, playerIndex)` (`:141-524`), rng `"re1map"`. This is the
         RE1-specific map preparation, see §1.3.
      2. `InventoryRandomizer` always runs (rng `"inventory"`).
      3. `DoorRandomizer` runs if `doors/random` (rng `"sourceDoor"`).
      4. `LockRandomizer.Randomise` always runs. Its rng `"lock"` is only used if
         `locks/random`. It always writes door locks.
      5. If `items/random`: `KeyRandomizer` (rng `"key"`, which seeds
         `RouteFinder`), then `ItemRandomizer` (rng `"item"`).
      6. `EnemyRandomizer` runs if `enemies/random` (rng `"enemy"`).
      7. If `protagonist/random` or `npc/random`: `CharacterRandomizer`
         (rng `"character"`), then `CutsceneRandomizer` (rngs `"npc"`, `"voice"`).
      8. `RandomizeEnemySkins` always runs (rng `"enemyskin"`, `:791-826`).
      9. `MusicRandomizer` runs if `music/random` (rng `"music"`).
      10. `RandomizeTitleSound` runs if `title/call/random` (rng `"titlesound"`,
          `:828-844`).
   5. `generatedVariation.ModBuilder.Build()` returns a `ClassicMod`
      (`BR/ModBuilder.cs:75-95`).
2. `Re1CrModBuilder.Create(mod)` (`RE1/Re1CrModBuilder.cs:22-76`) turns the
   `ClassicMod` into a Classic Rebirth mod: patched RDTs, XMLs, skins, music,
   `generated.json`, `log.md`/`log.html`, `config.json`, `biorand.dat`.
   `Create7z()` packs it. The output assets are `mod_biorand_{seed}.7z` and the
   spoiler log `mod_biorand_{seed}.html`.

### 1.2 RNG forks

`Context.GetRng(key)` (`BR/ClassicRandomizer.cs:768-774`) computes 64-bit FNV-1a
over the key's **UTF-16LE** bytes (`MemoryMarshal.AsBytes(key.AsSpan())`;
FNV is in `BU/Extensions/MiscExtensions.cs:13-22`). The returned
`new Rng(seed ^ lo32 ^ hi32)` wraps `System.Random`. Every stage therefore has
its own deterministic stream, independent of earlier stages. Keys: `config`,
`re1map`, `inventory`, `sourceDoor`, `lock`, `key` (whose first draw,
`rng.Next(0, int.MaxValue)`, seeds `RouteFinder`'s own `System.Random`),
`item`, `enemy`, `character`, `npc`, `voice`, `enemyskin`, `music`,
`titlesound`.

For Rust: keep the idea (one named sub-stream per stage, derived from seed and
key) and use any good PRNG, for example a `rand_pcg` or `rand_chacha` stream
seeded from the hash. Parity is not needed, so `System.Random` need not be
reproduced.

`BR/Rng.cs` semantics worth keeping: `Next(min, max)` is max-exclusive and
**returns `min` when `max <= min`** instead of throwing (`:39-44`).
`NextDouble(min, max)` is linear. `NextOf(params T[])` throws on an empty array.
`Shuffle` (`BR/Extensions.cs:106-117`) is a Fisher–Yates that returns a new array.

### 1.3 RE1 map preparation (`Re1ClassicRandomizerController.GetMap`, `:141-524`)

This turns `rdt.json` into the working `Map` that all later stages mutate in place:

1. Load `meta/re1/rdt.json` and filter it with
   `map.For(new MapFilter(randomDoors, player, scenario 0))` (`:158-159`). This
   drops every room, door, item, enemy and cutscene whose `doorRando`, `player`
   or `scenario` is set and does not match. See §3.3.
2. Copy door entrances from `meta/re1/doors.csv` (`Target,X,Y,Z,D`, 246 rows)
   into `door.Entrance` for every room containing that RDT (`:162-182`).
3. If lockpick is `Always`, mark item 51 (Sword Key) and item 61 (Small Key)
   `Implicit` (`:185-190`). Doors keep those locks, but the key graph adds no
   requirement for them.
4. If cutscenes are enabled, `prologue`-tagged doors get `AllowedLocks = []`
   (`:193-203`).
5. Area groups are bit flags: `MANSION_1=1`, `MANSION_2=2`, `GUARDHOUSE=8`,
   `LAB=32`, `PLANT_42=64`, `SMALL_KEY=128`, `BATTERY=256`, `LAB_TYRANT=512`,
   `ALL=-1` (`:205-215`).
6. **Without door rando** (`:217-273`):
   - Guardhouse off: remove rooms tagged `guardhouse`, and null the courtyard
     "GATE TO GUARDHOUSE" target. Mansion 2 without the guardhouse throws
     `RandomizerUserException`.
   - Lab on: the fountain "DOOR TO HELIPORT" becomes `locked`, and the helipad
     "DOOR TO FOUNTAIN" becomes `unlock` with `IgnoreInGraph`. Lab off: remove
     `lab` rooms, null the "FOUNTAIN STAIRS" and "LIFT TO LAB" targets, and clear
     the heliport door kinds.
   - `softlock-safe` doors and their other sides get `AllowedLocks = []`
     (`:277-288`).
7. **Random locks** (`:290-445`):
   - Unless `locks/preserve`, strip vanilla `locked`/`unlock` kinds and `LockId`
     from doors where neither side has `AllowedLocks` set. `Requires2` is left
     in place; `LockRandomizer` overwrites it later.
   - `genericKeys` = item definitions with `discard: true` (Sword … Power Room
     key), shuffled.
   - With door rando, every door with `AllowedLocks == null` may take any
     generic key.
   - Without door rando:
     - With preserve: drop the Helmet key (mansion 2 key) from the generic keys.
     - Without preserve: set every `MANSION_2` item to `MANSION_1`.
     - If mansion 2 is on, pick one generic key (not the Sword key when the
       lockpick is on) as the mansion 2 key. Pick `rng.Next(2, 9)` rooms tagged
       `mansion2able`, tag their items `MANSION_2`, and lock every door of those
       rooms, both sides, with that key and fresh lock ids from 1..254
       (`:345-400`).
     - Each remaining generic key becomes a `DistributedLock` with a random
       4-bit mask over the areas `[mansion+courtyard, caves, guardhouse?, lab?]`
       (`:405-429`). Each door's `AllowedLocks` is the set of keys whose area
       tags match the door's room, or all keys when a key has no tags
       (`:431-443`, `:526-534`).
8. **Item groups** (`:447-521`):
   - Door rando: every item is `GROUP_ALL`.
   - Otherwise every item starts as `ALL`, and specific bits are then cleared so
     that only chosen locations can hold certain keys:
     - Mansion 2: only "KEY IN FIREPLACE" in `40C` keeps `PLANT_42`, and the
       mansion 2 key's group becomes `PLANT_42`. The Battery (39) becomes
       `BATTERY`, and only mansion 2 and lab items keep that bit.
     - Lab and tyrant 1: the Flare (42) becomes `LAB_TYRANT`, held only by
       tyrant items.
     - Segmented caves or lab: the "LADDER TO CAVES" or "FOUNTAIN STAIRS" door
       becomes `noreturn`, and its other side becomes `blocked`. **Segmentation
       works through no-return doors.** The route finder starts a fresh key
       inventory after each one (§2.13).
   - `segmentedGuardhouse` is hard-coded `false` (`:150`), so the
     guardhouse/small-key group branch is dead code.

### 1.4 Data flow

```
config (dict) + seed
   │ ApplyConfigModifications: resolve Random/Default, write General
   ▼
Map  (rdt.json → For(filter) → doors.csv entrances → GetMap mutations)
   │  every stage below MUTATES this one Map object in place
   ├─ InventoryRandomizer ──────────────► ModBuilder.Inventory[0]
   ├─ DoorRandomizer: rewires door.Target/Kind/LockId/AllowedLocks/Tags,
   │                  removes unused rooms ──► ModBuilder.SetDoorTarget(rdt:door → room,x,y,z,d)
   ├─ LockRandomizer: sets door.LockId/LockKey/Requires2 ──► ModBuilder.SetDoorLock(rdt:door → {id,type}|null)
   ├─ KeyRandomizer: Map → routing Graph → RouteFinder → ModBuilder.SetItem(globalId → key item)
   ├─ ItemRandomizer: weapons/ammo/health/ink into the remaining globalIds → ModBuilder.SetItem
   ├─ EnemyRandomizer: Map.Rooms[].Enemies + enemy.csv ──► ModBuilder.AddEnemy(EnemyPlacement)
   ├─ Character/Cutscene/Skins/Music/Title ──► Characters, Npcs, Voices, EnemySkins, Music, General.titleSound
   ▼
ClassicMod (serializable; written into the mod as generated.json)
   ▼
Re1CrModBuilder: patches RDT bytes (doors, items, enemies, ~45 room fixes in Re1RdtPatcher), init.xml, biorand.dat, ...
```

The routing graph is only an intermediate inside `KeyRandomizer`. The `Map` is
the single shared state. Later stages read what earlier ones wrote: kinds,
lock ids, `Requires2`, removed rooms, `segment-N` and `lock-priority` tags.

---

## 2. Per-file catalogue

LOC is `wc -l`. The proposed Rust modules assume a crate `arklay_biorand`
(see §6).

### 2.1 `ClassicRandomizer.cs` (852): orchestration and config definition

- **Purpose:** `IClassicRandomizer`. Builds the option schema (§4), runs the
  stages (§1.1), resolves enemy skins and the title sound, and derives the rngs.
- **Key logic:**
  - `CreateConfigDefinition` (`:43-623`) is data-driven from `map.Items`
    (weapons, health, ammo, ink), `map.Enemies` (distribution and min/max), and
    the data directories `re1/emd`, `re1/pld0`, `re1/pld1` and `bgm/*`. It then
    calls `controller.UpdateConfigDefinition`.
  - `RandomizeEnemySkins` (`:791-826`) shuffles the skin folders and keeps
    enabled ones whose `EM10xx.EMD` ids do not overlap an already chosen skin.
  - `RandomizeTitleSound` picks a `.ogg` or `.wav` from `title/`.
- **Depends on:** `DataManager`, `Map`, `ModBuilder`, all randomizers, `BC` config types.
- **Rust:** `pipeline.rs` (orchestration, `Context`, `rng_for(key)`) and
  `config/definition_re1.rs` (schema builder).

### 2.2 `ClassicRandomizerFactory.cs` (27), `IClassicRandomizer*.cs` (7 + 15 + 17)

- **Purpose:** factory (RE1 only; everything else throws
  `NotImplementedException`).
  - `IClassicRandomizerController` has three methods: `UpdateConfigDefinition`,
    `GetVariation` and `ApplyConfigModifications`.
  - `Variation(PlayerIndex, Map)` (`IClassicRandomizerController.cs:10-14`).
  - `IClassicRandomizerContext` provides `ConfigurationDefinition`,
    `Configuration`, `DataManager` and `GetRng`.
  - `IClassicRandomizerGeneratedVariation` adds `Variation` and `ModBuilder`.
- **Rust:** a `GameController` trait (or simply RE1 functions) in `pipeline.rs`.
  `Context { def, config, data, seed }` and `Generated { ctx, variation, out }`.

### 2.3 `RE1/Re1ClassicRandomizerController.cs` (627): RE1 map preparation and RE1 options

- **Purpose:** adds the RE1 options (`:8-133`), resolves Random and Default
  values (`:536-616`), and builds the per-run `Map` (`:141-524`, §1.3).
- **Notable:**
  - The `GetValueOrDefault` fallbacks in code often differ from the definition
    defaults. For example `progression/guardhouse` falls back to `"Never"` in
    code but defaults to `"Always"` in the definition. The CLI does not merge
    definition defaults into the input (`biorand-classic/Commands/GenerateCommand.cs:54`).
    **The launcher must always send a complete configuration**:
    `DefaultConfiguration + user overrides`.
  - Hard-coded room keys and door names: `"302"`, `"303"`, `"305"`, `"40C"`,
    "GATE TO GUARDHOUSE", "DOOR TO HELIPORT", "FOUNTAIN STAIRS", "LIFT TO LAB",
    "LADDER TO CAVES", "KEY IN FIREPLACE". Item ids 39 (Battery), 42 (Flare),
    51, 54 and 61.
- **Depends on:** `Map`, `MapTags`, `DoorKinds`, `Re1ItemIds`, `doors.csv`.
- **Rust:** `re1/controller.rs` (options and resolution) and `re1/map_prep.rs`.

### 2.4 `Map.cs` (551), `MapTags.cs` (31), `DoorKinds.cs` (41), `DoorLock.cs` (15)

- **Purpose:** the data model (§3) plus string constants.
  - Tags: `box`, `safe`, `save`, `prologue`, `segment-end`, `begin`, `end`,
    `connect-out`, `connect-back`, `lock-priority`. The data also uses room tags
    `mansion`, `courtyard`, `guardhouse`, `caves`, `lab`, `mansion2able` and
    `exclude`, and the door tag `softlock-safe`. The pipeline adds `segment-N`.
  - Door kinds: `dynamic`, `blocked`, `unblock`, `locked`, `unlock`, `oneway`,
    `noreturn`.
  - `DoorLock { Id, Type }`.
- **Rust:** `map/mod.rs` (serde model), `map/requirement.rs`, `map/filter.rs`.
  Make tags and kinds Rust enums with a serde fallback `Other(String)`.

### 2.5 `InventoryRandomizer.cs` (226): starting inventory

- **Purpose:** builds `ModBuilder.Inventory = [RandomInventory]` (`:241`).
- **Algorithm** (`:201-292`):
  1. Capacity = `rng.Next(inventory/min, inventory/max + 1)`.
  2. Knife: `Always`, `Never` or a coin flip; added with amount = `Max` (1).
  3. Primary weapon: a pool of `null` (if `inventory/primary/none`) plus every
     enabled `inventory/primary/{kind}` weapon. Shuffle and take the first.
     Ammo total = `round(NextDouble(minPct·Max, maxPct·Max))`. The weapon is
     loaded with `min(total, Max)`. The rest is extra ammo of a random
     compatible ammo type, split into stacks of that ammo's `Max` (`:280-289`,
     `:321-332`).
  4. Secondary weapon: same, excluding the primary's *group* (the second segment
     of the kind, for example `colt`).
  5. `ItemPool`: one group per prefix (`health/` and `ink`). Each item gets an
     amount from `rng.Next(min, max + 1)`. `TakeStack` round-robins over
     shuffled groups and takes `min(amount, Max)` per stack until the inventory
     is full (`:347-414`).
  6. `Build()` pads with empty entries up to the capacity.
- **Note:** weapon and extra-ammo stacks are added without a capacity check, so
  the inventory can overflow its capacity. `Re1CrModBuilder` later truncates
  with `WithSize(12 or 8)` (`RE1/Re1CrModBuilder.cs:752-755`).
- **Rust:** `rando/inventory.rs`.

### 2.6 `DoorRandomizer.cs` (946): door randomizer (segments, bridges, loopbacks)

- **Purpose:** rewires the door graph out of "room pieces", split into N
  segments separated by no-return boss or multi-key doors. Unused rooms are
  removed.
- **Pieces** (`GetPieces`, `:205-246`): BFS over doors with `randomize:false`
  (fixed connections) groups rooms into a `RoomPiece` (`:716-780`). On the way
  it **clears `Target` on every randomizable door** (`:235-239`). A piece's
  `Doors` are its randomizable doors (`RoomPieceDoor`, `:782-861`).
- **Coupling** (`:59-66`, `IsCoupled` `:738-772`, `GroupDictionary`
  `:881-944`): if piece A has a `room(X)` or `flag(F)` requirement satisfied by
  piece B, they form a dependency group. `TakeRoom` (`:292-301`) only takes a
  piece together with its whole group, and only if every member is still
  available.
- **Counts:**
  - `numRooms = round(pieces × doors/rooms)`.
  - `numSegments = max(1, doors/segments)`.
  - Segment enders are pieces with a `segment-end` door, shuffled. With one
    segment their tags are removed. `numSegments-1` are used as segment ends;
    the rest are discarded (`:39-57`).
- **Segments** (`CreateSegments`, `:248-275`): a linked list. Segment 1 starts
  at the `begin` piece and the last ends at the `end` piece. The middle ends are
  dequeued from the used segment enders, and each segment's end is the next
  segment's start (`:69-82`). A segment's `Unused` pool is filled with the
  coupled companions of its start and end pieces.
- **Distribution** (`Distribute`, `:277-290`): an `EndlessBag` over segments
  (shuffled round-robin) deals each piece group to a segment's `Unused` pool, in
  three passes (`:84-99`):
  1. all `box` rooms;
  2. "hub" rooms: shuffle, then a stable sort by door count descending, take
     `numSegments × 4`;
  3. random non-segment-end pieces up to `numRooms - included`.
- **Per segment** (`RandomizeSegment`, `:308-369`):
  1. `UseRoom(Start)`, then repeat `ConnectAnotherDoor(ensureFreeDoor: true)`
     (`:371-451`) until it fails. Each step picks a shuffled unconnected door of
     a used piece and a shuffled available door of an unused piece, subject to:
     - Until the segment `HasBox` and `HasItems` (a requirement-free item), the
       source door must be requirement-free and the target piece must be a
       single room. This keeps the early tree simple.
     - The frontier must stay open: after the connection, at least one
       connectable door must remain, not counting doors that must connect back.
     - Source doors of kind `unblock` need a lockable target. The pair is then
       locked one-sidedly so it cannot be entered from the wrong side.
     - Skip the pair if the source room already has a door to the target room
       ("stops key rando complaining").
     - **Connect-back** (`ConnectBackDoors` `:453-474`,
       `FindConnectBackDoors` `:476-497`): if the target piece has
       `connect-back` doors (rooms behind one-way drops), each must be wired to
       a door on the ancestor chain (`Parent` pointers, back to the segment
       start) with a fresh lock id. These are one-sided "unlock from this side"
       shortcuts that guarantee a way back.
     - `Connect` (`:801-834`) seals both doors and sets the target's `Parent`.
       If the source door has a vanilla `LockId`, the lock is mirrored onto the
       target door. If a `lockId` is passed, the source becomes `unlock` and the
       target `locked`, both with `AllowedLocks=[]`.
  2. **Segment-end bridge** (`:318-326`, `PickUnconnectedDoorForSegmentEnd`
     `:499-542`): BFS from the start gives each unconnected door a depth. Pick a
     random door at the **maximum depth**, tag it `lock-priority`, and connect
     it to a random free, connectable, non-segment-end door of the `End` piece.
     If no door is found, throw "Segment could not be connected up to the end
     room".
  3. `ConnectAnotherDoor(false)` repeats to attach the remaining unused pieces.
     It prefers targets with more open doors and drops the free-door rule.
  4. **Loopbacks** (`:333-349`): for each remaining unconnected lockable door,
    connect it to another unconnected door that is free, not connect-out,
    lockable, and in a piece not already connected to it. The door gets a lock:
    it can be opened from this side and is "locked" from the other.
  5. **Dead ends** (`:351-360`): seal the rest. Tag every room `segment-{n}`.
  6. Unused pieces flow on to the next segment. The last segment returns them
     to `_allRooms`, and they are removed from `map.Rooms` (`:101-124`).
- **Locks per segment** (`DistributeLocks`, `:544-592`, only if `locks/random`):
  each segment gets one dedicated generic key, and the remaining keys go to
  `rng.Next(1, min(4, segments+1))` random segments each. A door's
  `AllowedLocks` becomes `AllowedLocks ∩ segment keys`, sorted.
- **Finalize** (`:131-202`):
  - A segment-end door becomes `noreturn` and its partner `blocked`.
  - Unconnected doors become `blocked` (if `noUnlock`) or `locked` with
    `LockId=255`, with no requirements.
  - Write `door.Target = "{room}:{id}"`.
  - For each RDT of the room (both mansion copies),
    `ModBuilder.SetDoorTarget(rdt:id, {Room = target.Rdts[0], entrance x,y,z,d})`.
    An unconnected door targets itself. A missing entrance throws.
- **Correctness notes:**
  - It relies on `Queue.Dequeue` and `First()` throwing (not enough segment
    enders, lock ids exhausted). The data has 5 `segment-end` doors, which
    matches the 1..6 range of `doors/segments`.
  - The target room is always `Room.Rdts[0]` (`:799`), so door order in `rdts`
    matters. Some rooms list `6xx` first, for example `"110": ["610","110"]`.
    Under door rando every RDT copy of a door gets the same target. **Verify**
    how this interacts with mansion 1/2 RDT selection in arklay.
  - It uses reference identity throughout (`HashSet<MapRoom>`, `RoomPiece`,
    `RoomPieceDoor`). In Rust use indices: `PieceId`, `DoorRef { piece, idx }`.
  - `Segment.Used` is a `List` with `Contains`, so it is O(n²). That is fine at
    this size.
- **Rust:** `rando/doors/{mod.rs, pieces.rs, segment.rs, connect.rs}`.

### 2.7 `LockRandomizer.cs` (484): lock assignment and writing locks

- **Purpose:** decides which door pairs are locked and with which key, then
  always writes every door's lock to the mod.
- **Algorithm:**
  - `GetDoors` (`:36-60`): every door with an `id`, as
    `DoorInfo(roomKey:id)`.
  - `GetDoorPairs` (`:62-79`): match A with the door whose identity equals
    `A.Target`. Unmatched doors are not paired.
  - `locks/preserve` (`:16-24`): doors that already have a `LockId` get
    `AllowedLocks=[]`, which makes the pair *restricted*.
  - `KeepBoxRouteClear` (`:81-145`): enumerate **all simple paths** from the
    start room over doors with no requirements that are not segment-end. Keep
    maximal paths whose tail is a `box` room. Drop paths that leave the start's
    free component and come back (`ContainsButNotAtStart`). Pick one at random,
    then clear and forbid locks on every door of it and on the other sides.
  - `KeepRouteItemful` (`:154-260`): from the start room and the target of every
    `noreturn` door, BFS through doors that cannot be locked until at least 2
    non-optional, requirement-free items are reachable. Whenever the BFS runs
    dry, force a random lockable candidate door, and its other side, to stay
    unlocked, then continue. `AllowedLocks == null` counts as lockable because
    `null?.Length == 0` is false (`:206`).
  - `RandomizeLocks` (`:262-324`):
    - `lockLimit = round(pairs × locks/ratio)`. Restricted pairs (empty
      `AllowedLocks` intersection) keep their locks and count toward the limit.
    - Lock ids come from 1..254, minus the restricted pairs' ids and the items'
      `lockId` values.
    - The other pairs are shuffled and then **stable-sorted with
      `lock-priority` first**, so each segment's bridge door is locked first.
    - Below the limit, a pair gets a lock: its existing id or a new one, and
      `NextOf(AllowedLocks)` as the key. **Unblock workaround:** if one side is
      `unblock`, the other side gets key 255 (locked, no item requirement).
    - Above the limit, the lock is cleared.
    - `AssignDoorLock` (`:326-342`) sets `LockId`, `LockKey` and
      `Requires2 = ["item(k)"]` (or none for 255).
  - `SetDoorLocks` (`:344-373`): `type` is chosen by the first rule that
    matches. With `lockId = door.LockId ?? 0`:

    | Condition | `type` |
    |---|---|
    | `noUnlock` | 0 |
    | no target | 255 |
    | kind `locked` | 255 |
    | kind `unlock` | 254 |
    | `LockKey` set | `LockKey` |
    | otherwise | 0 |

    Then `SetDoorLock(rdt:id, lockId==0 ? null : {lockId, type})` for every
    RDT of the room.
- **Correctness notes:**
  - `KeepBoxRouteClear` builds immutable path lists breadth-first, which is
    exponential in the worst case. It is fine on RE1's sparse graph. In Rust,
    write it as a DFS with a path budget, or compute shortest paths to box rooms.
  - `GetDoorPairs` uses `dict.First()` after removals, which relies on .NET
    `Dictionary` keeping insertion order when nothing is added.
  - `pairs.Except(restricted)` relies on `DoorPair` equality by identity
    string.
- **Rust:** `rando/locks.rs`.

### 2.8 `KeyRandomizer.cs` (312): key placement through routing

- **Purpose:** places key items so the game can be completed, using the common
  `RouteFinder`. Then sets each placed key's amount.
- **Graph construction** (`:23-134`):
  - Keys: each `map.Items` entry whose kind is `key/<kind>` becomes
    `graphBuilder.Key(name, group, KeyKind.Parse(kind))`. The kinds are
    `reusuable` (sic; it must match the enum spelling `Reusuable`) and
    `consumable`.
  - Nodes: a `START` room node, then one room node per map room in dictionary
    order, then `Door(START, beginRoom)`.
  - Doors are skipped if they have `IgnoreInGraph`, no target, or kind
    `blocked`/`locked` (those are only passable from the other side, which is
    added from there). Requirements become key, flag-node or room-node
    requirements; `Implicit` item requirements are dropped. If the opposite door
    is `unblock`, the requirements are dropped ("the graph library hates
    unblock[key] <--> [key]"). Edge kind by door kind:

    | Door kind | Edge |
    |---|---|
    | `oneway` | `OneWay` |
    | `noreturn` | `NoReturn` |
    | `unblock`, `unlock` | `BlockedDoor` (`UnlockTwoWay`) |
    | other | `Door` (`TwoWay`) |

  - Items: skip `optional`; skip `document` unless docs-as-keys; skip `hidden`
    unless hidden keys. `graphBuilder.Item(label, group, room, reqs)`.
  - Flags: lazily created room nodes, with `Door(room, flag, reqs)`.
- **Routing:** `graph.GenerateRoute(seed, {DebugDeadendCallback})`. After 1000
  dead ends it throws `RandomizerUserException("Failed to find route…")`
  (`:136-147`). **`route.AllNodesVisited` is never checked.** A partial route is
  accepted silently if fewer than 1000 dead ends happened.
- **Amounts** (`:149-195`): `amount = def.Amount ?? 1`. For `discard` keys, the
  amount is the number of distinct lock ids among edges requiring `item(type)`
  in the *segment subtree* containing the item. The segment tree
  (`GetSegmentTree` `:237-310`) splits rooms at `noreturn` doors. The game then
  discards the key after its last use. Finally
  `ModBuilder.SetItem(globalId, Item(type, amount))`.
- **Correctness notes:**
  - `ToDictionary` throws on duplicate global ids. Duplicates exist for Jill
    (global ids 14 and 117 appear in two rooms each), but those items are
    `optional` and never key slots.
  - `itemIdToKey[int.Parse(...)]` throws on an unknown item id. This can happen
    if no mansion 2 key was found: `FirstOrDefault` returns a default
    `KeyValuePair` with key 0, which yields `item(0)` (`Re1ClassicRandomizerController.cs:347-351`).
- **Rust:** `rando/keys.rs`, using `routing::*`.

### 2.9 `ItemRandomizer.cs` (191): weapons, ammo, health, ink distribution

- **Purpose:** fills every item slot not taken by a key.
- **Algorithm** (`:11-107`):
  - `ItemSlotCollection` (`:109-171`) takes every room item with a `globalId`,
    deduplicated by `globalId` (first occurrence), minus the ones already
    assigned, shuffled. Documents are skipped when they are disabled.
    `optional`, low-priority documents (docs replaced but not keys) and
    low-priority hidden items go to a *low* queue; the rest to a *normal* queue.
    (`hidden` can never be `Disabled`; that branch is dead code.)
  - Weapons: shuffle all `weapon/*` definitions. Skip any that are disabled
    (`items/weapon/enabled/{kind}`) or whose group is already present, either in
    the starting inventory or already placed. One weapon per group:
    handgun, shotgun, colt, flamethrower, grenadelauncher, rocketlauncher,
    knife. Place each into `DequeueNormalFirst()` with
    `rng.Next(initialMin, initialMax+1)` ammo.
  - Weights: `items/distribution/{kind}` for every definition. Only ammo,
    health and ink have this key, so everything else defaults to 0 and is
    excluded. Ammo whose weapon was neither placed nor in the inventory gets
    weight 0.
  - `count = ceil(w / Σw × slots)` per type, stable-sorted ascending so rarer
    types are placed first. Each item goes into a random slot (`DequeueAny`
    picks normal or low in proportion to the remaining counts) with
    `rng.Next(stackMin, min(Max, stackMax)+1)`.
  - Remaining slots become `Item(0,0)`, which removes the item.
- **Correctness notes:** `Σw == 0` gives NaN, and `(int)NaN` is
  `int.MinValue` on x64, so the loop simply does not run. Rust `as i32` gives 0,
  which is also harmless. Guard it explicitly.
- **Rust:** `rando/items.rs`.

### 2.10 `EnemyRandomizer.cs` (317): enemy types, counts and difficulty

- **Purpose:** picks one enemy type per room, then retypes the fixed enemies or
  creates new ones at positions from `enemy.csv`.
- **Room selection** (`GetRoomSelection`, `:145-255`):
  - `EnemyInfo` per enemy entry, with weight `enemies/distribution/{key}`,
    `min = max(1, enemies/min/{key})` and `max = min(16, enemies/max/{key})`.
  - Rooms with `enemies` become `RoomRdt`s. With mansion 2 progression
    (`separateRdts`), each RDT is a separate entry; otherwise each room takes
    the RDTs not seen yet.
  - `SupportsEnemy` (`:257-281`): the first non-`npc` spec's `includeTypes`
    (any of the entry's ids) or `excludeTypes`. Rooms tagged with a banned tag
    (box, safe or save, unless allowed) support nothing.
  - Shuffle. `enemyRoomTotal = round(enemies/rooms × total)`. Remove rooms that
    support nothing, trim to the total, then **stable-sort by number of
    supported types ascending** (constrained rooms first).
  - `NumRooms = ceil(total × w/Σw)` per type. Each room takes a random
    supported type with `NumRooms > 0`, falling back to its lowest-weight
    supported type, and decrements.
- **Placement** (`:37-112`):
  - The type is `NextOf(entry.Id)`; for example zombie is one of 0, 1 or 17.
  - Free per-room ids are 0..15 minus the ids of the room's specs.
  - A spec with an `id` (and not `npc`) gets an `EnemyPlacement` that retypes
    the existing enemy, with a random pose and the entry's ESPs.
  - A spec without an id: `max` is capped by difficulty,
    `max = min(max, max(1, round(max / (4 - maxDifficulty))))`, which divides
    `max` by 4, 3 or 2 for difficulty 0, 1 or 2. Then
    `n = rng.Next(min, max+1)` positions are drawn from an `EndlessBag` of
    `enemy.csv` rows for the room's RDTs (filtered by Include/Exclude; those
    columns are empty in the data). Each position creates an enemy with a
    `Create=true` placement per RDT: next free id, a global "kill id" from an
    `EndlessBag` over 0..254 minus fixed global ids, a pose, x/y/z/d, the ESPs
    and the spec's `condition`.
  - Pose: weighted by `poses[].probability`; 0 if there are no poses.
- **Correctness notes:**
  - The kill-id bag is *endless*, so ids repeat once it has cycled through all
    255. In theory two created enemies can then share a kill flag.
  - `maxDifficulty = 4` would divide by zero. The data only uses 0 to 2.
  - When several created placements land in one RDT, `Re1CrModBuilder` applies
    only the first condition it finds (`RE1/Re1CrModBuilder.cs:578-588`).
  - `MapRoomEnemies` fields such as `KeepState`, `KeepAi`, `Restricted`,
    `IgnoreRatio`, `ExcludeOffsets` and `Nop` are **not read** by the new
    pipeline (`nop` is read by the RDT patcher only).
- **Rust:** `rando/enemies.rs`.

### 2.11 `ScdCondition.cs` (472): enemy and patch condition expressions

- **Purpose:** parses `"!1:0 && !1:2"` or `"!(7:0 && 7:1)"` into
  `Flag(group, index, value)`, `Variable` or `And`, with an optional top-level
  `Negated` (`:358-412`). It emits RE1 SCD `IfelCk`/`Ck`/`ElseCk`/`EndIf`
  wrappers in blocks of 192 bytes or less (`:31-131`).
- The new pipeline only stores the string in `EnemyPlacement.Condition`. Only
  the mod builder compiles it.
- **Rust:** `map/condition.rs` (parser and AST only). How arklay evaluates it
  (natively, or by compiling to its script format) is an engine-side decision.

### 2.12 `Rng.cs` (117), `EndlessBag.cs` (50), `Extensions.cs` (259, relevant parts only)

- **`Rng`:**
  - `Next(min, max)` returns `min` when `max <= min`.
  - `NextDouble(min, max)`, `NextOf`, `NextFork`, `NextGaussian`.
  - A probability `Table<T>`, unused by the new pipeline.
- **`EndlessBag<T>`:** shuffle all items into a queue, dequeue, and reshuffle
  when the queue is empty. `Next(n)` draws n. It throws if the bag is empty.
- **`Extensions`:**
  - `Shuffle` (new array, Fisher–Yates), `Random`, `ToQueue`, `RemoveMany`.
  - `DistinctBy` (keeps the first of each group).
  - `Choose` (filter-map).
- **Rust:** `rng.rs` (`Rng` wrapper with these semantics) and `bag.rs`.

### 2.13 `BC/Routing/*` (biorand-common 1.3.1): the key-placement engine

| File | LOC | Purpose |
|---|---|---|
| `Node.cs` | 31 | `Node { Id, Group, Kind(Default\|Item), Label }`; equality by Id |
| `Key.cs`, `KeyKind.cs` | 20 + 9 | `Key { Id, Group, Kind(Reusuable\|Consumable\|Removable), Label }`. `operator !=` recurses infinitely (`Key.cs:17`); never use it |
| `Requirement.cs` | 53 | Either a `Key` or a `Node`, plus an internal `IsSoft` (an inferred "passed through this node" requirement). `!=` also recurses |
| `Edge.cs` | 78 | `Edge(Source, Destination, Requires[], Kind)`; `EdgeKind` = `TwoWay`, `UnlockTwoWay`, `OneWay`, `NoReturn` (semantics in doc comments, `:39-69`); ordered by (Source, Destination) |
| `GraphBuilder.cs` | 152 | Id allocator; `Key`, `Room`, `Item` (adds a TwoWay room→item edge), `Door`, `BlockedDoor`, `OneWay`, `NoReturn`. `Edge()` (`:47-119`) deduplicates identical edges and reconciles **inverse** duplicates: an inverse TwoWay without requirements is replaced, conflicting requirements throw, NoReturn/OneWay inverses throw. The branch conditions at `:96-110` mix `\|\|` oddly; port the intent and add tests |
| `Graph.cs` | 249 | Immutable keys, nodes and edges. `EdgeMap`/`InverseEdgeMap` sorted. `Start = nodes.First()`. `GetApplicableEdgesFrom(n)` = outgoing edges plus incoming TwoWay edges. `Subgraphs` (components ignoring one-way edges). Mermaid export |
| `Route.cs` | 110 | Result: `AllNodesVisited`, `ItemToKey` (one-to-many), `Log`, `GetItemContents`, `Solve()` |
| `RouteFinder.cs` | 742 | The solver (below) |
| `RouteSolver.cs` | 179 | Checks a placement by greedy play-through (below) |
| `RouteFinderOptions.cs` | 10 | `DebugDepthLimit` (`int?`), `DebugDeadendCallback` (`Action<object>`) |
| `RouteFinderException.cs`, `GraphException.cs`, `RouteSolverResult.cs` | 9 + 8 + 12 | Exceptions; flags `NodesRemaining`, `PotentialSoftlock` |
| `DependencyGraphBuilder.cs` | 83 | Alternative builder; **not used** by classic RE1 |
| `Collections/MultiSet.cs` | 192 | `MultiSet` and `ImmutableMultiSet` (counts); `Remove` of a missing item is a no-op |
| `Collections/ImmutableOneToManyDictionary.cs` | 66 | item→key plus reverse key→items. The indexer throws on a missing key |
| `Collections/OneToManyDictionary.cs` | 48 | Mutable version |

**`RouteFinder` in plain words** (`BC/Routing/RouteFinder.cs`). It is a
randomized depth-first search with backtracking over immutable `State`
snapshots (`:536-740`).

- **State:** `Visited` nodes; `Next` (the frontier: edges with at least one end
  unvisited); `OneWay` (one-way and no-return exits found but not taken);
  `SpareItems` (visited item nodes with no key yet); `Keys` (a multiset of keys
  held); `ItemToKey`; `Parent` (for one-way forks); `Log`.
- **`DoSubgraph(start, fork)`** (`:39-59`): compute
  `GetGuaranteedRequirements(start)`, the reusable keys and nodes that any path
  to `start` must already have collected (`:343-458`, a fixpoint
  intersection over predecessor requirements). Then begin a new state: `Clear`
  for no-return (previous frontier and spare items are forgotten) or `Fork` for
  one-way (keeps a parent to rejoin). Visit `start`, then `Fulfill`.
- **`Fulfill`** (`:61-115`):
  1. `Expand` (`:147-179`): repeatedly take frontier edges whose requirements
     are met. Edges are sorted by (Source, Destination), and the first
     satisfied one is taken each time. Consumable keys are removed when used.
     Visiting a node adds its key, if it is an item with a key, or marks it as a
     spare slot, and adds its applicable edges to the frontier. One-way and
     no-return edges are deferred to `OneWay`.
  2. `FollowOneWayExits` (`:258-270`): take one random `OneWay` edge as a fork
     subgraph. `VisitNode` rejoins the parent (`Join`, `:607-637`) when it
     reaches a node a parent has visited.
  3. For each locked frontier edge, compute the missing keys (`GetRequiredKeys`,
     `:181-205`). This adds extra consumable copies when the same keys would
     also open another frontier edge. Removable keys need the minimum count
     along any path (`GetRemovableKeyCount`, `:315-341`). The edge list is
     shuffled, then sorted by how many of its reusable keys are already placed
     (ascending).
  4. For each edge, up to 10 tries: `FindAvailableSlots` (`:230-256`) draws
     shuffled spare items whose `Group` contains every bit of the key's
     `Group` (`(itemGroup & keyGroup) == keyGroup`). Place the keys, recurse into
     `Fulfill`, and reject the result if `RouteSolver` reports
     `PotentialSoftlock`. Return the first full solution (empty `Next` and
     `OneWay`); otherwise remember the best one (most keys placed).
  5. If the frontier still has locked edges, invoke
     `DebugDeadendCallback(state)` and **return the pre-attempt state** (this is
     the backtrack). Otherwise follow all no-return exits in random order, each
     as a fresh subgraph.
- **Termination:** there is no global budget besides the caller's dead-end
  counter and `DebugDepthLimit`. `depth >= null` is false in C#.
- **Ordering:** all randomness is applied after sorting by node id, so results
  do not depend on hash-set iteration order. In Rust use
  `BTreeSet<EdgeId>`/`BTreeSet<NodeId>`, or sorted `Vec`s.

**`RouteSolver`** (`:16-179`): a greedy simulation. Expand edges with no key
requirements, then use every reachable non-consumable key edge. If only
consumable edges remain, check that the keys held can pay for **all** of them
at once; if not, report `PotentialSoftlock`. Quirk: `Visit` adds only
`GetEdgesFrom` (outgoing) for new nodes (`:152`), while `Begin` uses applicable
edges, so `NodesRemaining` can be over-reported. That is harmless for
validation, but the Rust port should use applicable edges consistently. It is
fixed upstream later (`efa189e`).

**Rust:** `routing/{node.rs, graph.rs, builder.rs, finder.rs, solver.rs,
multiset.rs}`. Use persistent data structures (`im` or `rpds`) or `Rc` plus
cheap clones for `State`. C# allocates a new state per operation; a naive Rust
`clone()` of `HashSet`s per step would be much slower.

### 2.14 `ModBuilder.cs` (189), `Item.cs` (47), `RandomInventory.cs` (49), `EnemyPlacement.cs` (18): output model

See §5. `SetItem` uses `Dictionary.Add`, which **throws on a duplicate global
id** (`ModBuilder.cs:55-58`). That is the guard against two stages writing the
same slot. `ItemRandomizer` avoids it through `AssignedItemGlobalIds`.
**Rust:** `output.rs`.

### 2.15 `RE1/Re1CrModBuilder.cs` (1343) and `RE1/Re1RdtPatcher.cs` (1048): CR mod output

- **Purpose:** consumes a `ClassicMod`. Reads the vanilla `STAGE1-7/ROOMxxxp.RDT`
  files, plus copies of missing mansion 2 rooms from stage 6 (`:221-268`,
  `g_missingRooms` `:1300`). Then:
  - `Re1RdtPatcher`: about 45 reflection-dispatched `[Patch]` methods with
    hard-coded script offsets. Option flags are written at `0:122`/`123`/`124`
    for ink, hard mode and lockpick (`Re1RdtPatcher.cs:62-83`); there are many
    room fixes, mansion 2 duplication, and so on.
  - Optional cutscene disabling (`:276-446`).
  - `TypeOffsets` item byte patches (`:448-476`).
  - `ApplyDoors` (`:478-508`) sets `DoorAotSe` target, next x/y/z/d, lock id and
    lock type. No lock entry means `0/0`.
  - `ApplyItems` (`:510-541`) sets the item AOT type and amount, and turns a
    document `aot_on` SCE (0x0D) into an item (0x04).
  - `ApplyEnemies` (`:543-723`) NOPs removable enemies not reserved by the map,
    retypes fixed ones (with a state fix for snake and web spinner), creates
    `SceEmSet` opcodes wrapped in conditions, and injects missing ESPs harvested
    from other rooms.
  - `init.xml` holds two `<Player>` inventories, Jill first: Jill 8 slots,
    Chris 12; the unused player gets a knife only (`:741-786`).
  - Skins, voices, music, title call (`SOUND/BIO01.WAV`, `EVIL01.WAV`).
  - `biorand.dat` dynamic tweaks: kind 2 is the inventory size byte (`:1147-1151`).
- **For arklay:** this layer is RDT byte patching. arklay already converts a CR
  mod's `init.xml`, `bgm_tbl.xml`, RDTs and so on (`src/convert_mod.rs`). There
  are two paths:
  - (a) Port only up to `ClassicMod`, and have arklay apply `ClassicMod`
    natively: door targets, locks, items and enemy placements through its own
    room data and script hooks.
  - (b) Also port RDT patching.

  (a) is far less work, but the ~45 `Re1RdtPatcher` fixes become a checklist of
  arklay-side behaviours. Keep that porting decision out of the randomizer crate.

### 2.16 Other new-pipeline stages (out of the requested scope; listed for completeness)

- `CharacterRandomizer.cs` (80) fills `Characters[id] = {path, weapon}` for
  playable and NPC characters from `re1/pld0|pld1/*` (EndlessBag).
- `CutsceneRandomizer.cs` (535) produces `Npcs` (RDT plus enemy offset → type)
  and `Voices`.
- `MusicRandomizer.cs` (70) maps the `bgm.json` tag lists to `bgm/<game>/<tag>/*`.
- `ModLogBuilder.cs` (288) writes the spoiler markdown.

### 2.17 Not used by the new pipeline (old pipeline only)

- `PlayGraph.cs` (290): the old DGML play graph (`PlayNode`, `PlayEdge`,
  `LockKind`, `DoorRandoCategory`).
- `RandoConfig.cs` (374): the old bit-packed seed config.
- `IDoorHelper.cs` (9), `IItemHelper.cs` (29), `IEnemyHelper.cs` (21).
- `RE1/Re1DoorHelper.cs` (240), `RE1/Re1ItemHelper.cs` (396),
  `RE1/Re1EnemyHelper.cs` (315): the old RE1 implementations of those
  interfaces. They do contain useful RE1 facts, such as item sizes and enemy
  type limits.
- `GameDataReader.cs` (82): the old pipeline only.

`GameData.cs` (19) is still used, as an RDT list, by `Re1CrModBuilder`.
`Re1ItemIds.cs` (84; 78 constants) and `Re1EnemyIds.cs` (47; 41 constants) are
shared tables. In Rust they become `re1/ids.rs`. The randomizer needs
`HelmetKey=54`, `SwordKey=51` and `CombatKnife=1`. The rest serve the mod
builder.

### 2.18 biohazard-utils types used by the new pipeline (not catalogued)

The randomizer stages need only these:

- `RdtId` (`BU/RdtId.cs`): stage (0-based), room, optional variant.
  - Text form `"{stage+1:X}{room:X2}{variant:X}"`, for example `"106"` is stage
    0, room 6. Stage 15 prints as `G`, and the parser accepts hex digits 0-9 and
    A-G.
  - Ordered by (stage, room, variant).
- `RdtItemId` (`BU/RtdItemId.cs`): `"{rdt}:{id}"`.
- `BioVersion`.

The mod builder also uses `Rdt`/`Rdt1`, `RandomizedRdt` (`BR/Rdt.cs`), the
opcodes `SceEmSetOpcode`, `IDoorAotSetOpcode`, item AOT and `UnknownOpcode`,
`OpcodeV1`, `EmbeddedEffect(List)`, `ScriptDecompiler` and the `.EMD`/`.TIM`
model types. arklay has its own equivalents for all of these.

---

## 3. The Map data model (`BR/Map.cs`) and `rdt.json`

### 3.1 Loading

- `DataManager.GetJson<Map>` (`BR/DataManager.cs:107-122`) uses
  `System.Text.Json` with **`ReadCommentHandling = Skip`**: `rdt.json` contains
  `//` comments, for example line 1247. It uses the **camelCase** naming policy
  and is case-sensitive. It strips a UTF-8 BOM and uses `RdtIdConverter` for
  `RdtId` values.
- Each call deserializes a fresh `Map`; there is no cache.
- Unknown JSON properties are ignored. For example `"name"` on some enemy specs
  is not a model field.
- **Rust:** serde with `#[serde(rename_all = "camelCase")]` and
  `#[serde(default)]`. Strip comments before parsing (a small
  comment-stripping pre-pass, or the `json_comments` crate), because
  `serde_json` rejects comments.

`meta/re1/rdt.json` (7452 lines) has these top-level keys: `beginEndRooms` (1),
`items` (76), `enemies` (10 groups, 11 entries), `characters` (17) and `rooms`
(122). Across all rooms there are 283 door entries, 200 item entries, 109 rooms
with enemies, 36 with cutscenes and 8 with flags.

### 3.2 Types

`Map` (`:12-157`):

| Field | JSON | Meaning |
|---|---|---|
| `BeginEndRooms: MapStartEnd[]?` | `beginEndRooms` | `[{start:"106", end:"501"}]`. Filterable. `FirstOrDefault()` is used as the start room for routing, lock clearing and the segment tree |
| `Items: Dictionary<int, MapItemDefinition>` | `items` (object keyed by the item id as a string) | Item catalogue. **Insertion order matters**: config pages and weapon shuffles iterate it |
| `Enemies: MapEnemyGroup[]` | `enemies` | Enemy catalogue for the config UI and the enemy rando |
| `Characters: MapCharacter[]` | `characters` | Character catalogue (character and cutscene rando) |
| `Rooms: Dictionary<string, MapRoom>` | `rooms` | Rooms keyed by `"106"`, `"10C/BACK"`, `"301/END"`, … The setter copies each key into `MapRoom.Key` (`:20-31`). **Insertion order matters** for graph node order, door pairing and `GetPieces` |

Methods: `GetItem`, `GetRoom(RdtId|string)`, `GetRoomsContaining(RdtId)` (rooms
whose `rdts` contain the RDT; several rooms can share one RDT), `For(filter)`,
`GetOtherSide(door)` (the door in `door.TargetRoom` with
`Id == door.TargetId`), and the literal parsers `ParseLiteral` (`"0x…"` hex or
decimal) and `ParseNopArray` (`"lo-hi"` ranges expanded against the opcode
offsets).

`MapStartEnd : MapFilterable` (`:159-163`): `start`, `end` (room keys).

`MapItemDefinition` (`:165-180`):

| Field | Meaning |
|---|---|
| `name` | Display name, also the config label |
| `kind` | Path-like category that drives everything. Values: `weapon/knife`, `weapon/handgun`, `weapon/shotgun`, `weapon/colt/dumdum`, `weapon/colt/magnum`, `weapon/flamethrower`, `weapon/grenadelauncher/{acid,explosive,flame}`, `weapon/rocketlauncher`, `ammo/…` (same tails), `key/consumable`, `key/reusuable`, `ink`, `health/{fas,r,g,b,gr,gg,gb,grb,ggg,ggb}`, `map`, `document`. The weapon *group* is the second segment |
| `ammo: int[]?` | Compatible ammo item ids for a weapon |
| `max` | Max stack or weapon capacity. Initial-ammo and stack ranges use it |
| `group` | Key group bit mask (route placement constraint). 0 means anywhere |
| `amount: int?` | Fixed amount when placed as a key (0 for cranks and similar) |
| `discard` | "Generic" key the game discards after its last door. These are the keys lock rando may assign. Amount = number of doors using it |
| `implicit` | Runtime only (set for lockpick doors): no graph requirement |

`MapEnemyGroup` (`:182-188`): `name`, `background`, `foreground` (UI colours),
`entries[]`.

`MapEnemyGroupEntry` (`:190-197`): `key` (config key, for example `zombie`),
`name`, `id: int[]` (enemy type ids; one is chosen at random), `esp: int[]?`
(required effect ids), `poses[]`.

`MapEnemyPose` (`:199-204`): `name`, `pose` (state byte), `probability`
(default 1.0).

`MapCharacter` (`:206-214`): `id`, `name`, `actor`, `playable`, `weapon`
(the NPC holds a weapon), `flexible` (default true; eligible for the default
swap pool).

`MapRoom : MapFilterable` (`:216-276`):

| Field | Meaning |
|---|---|
| `key` | Not in the JSON; set from the dictionary key |
| `name` | Display name |
| `linkedRoom`, `requires: int[]?` | Legacy. **Not copied by `For()`**, so they are always null downstream |
| `rdts: RdtId[]` | RDT files of this logical room, usually both mansion copies (`["106","606"]`). Writes go to all of them. Door rando uses `rdts[0]` as the door identity |
| `doors[]`, `items[]`, `flags[]` (`MapEdge`: named graph flags with requirements), `enemies[]`, `npcs[]`, `cutscenes[]` | See below. `npcs` is empty in RE1 data |
| `doorRando: DoorRandoSpec[]?` | **Hides** the base `MapFilterable.DoorRando` bool (`new` property, `:230`), so rooms are never filtered by `doorRando`. Unused in RE1 data. `DoorRandoSpec` (`:413-420`) is legacy |
| `tags` (inherited) | Room tags |

`AllEdges` = doors, then flags, then items.

`MapRoomDoor : MapEdge` (`:278-327`). Fields read by the new pipeline: `id`,
`name`, `target` (`"ROOMKEY:doorId"`, or a room key alone for a fixed exit
without a door id, for example `"301"`), `kind` (§2.4), `lockId: byte?`,
`lockKey: int?` (key item id), `requires2`, `allowedLocks: int[]?` (`null` =
not decided, `[]` = may not be locked, otherwise the candidate keys),
`noUnlock` (cannot be a one-sided lock; a dead end becomes `blocked`, not
`locked`), `randomize: bool?` (`false` = fixed connection in door rando),
`entrance` (from inline JSON or `doors.csv`), `ignoreInGraph`, `tags`, and the
filter fields. Computed: `TargetRoom` and `TargetId`, by splitting `target` on
`:`. Legacy and unread: `condition`, `create`, `texture`, `special`, `altId`,
`offsets`, `cut`, `entranceId`, `lock`, `noReturn`, `isBridgeEdge`,
`requires`, `requiresRoom`.

`MapRoomDoorEntrance` (`:329-337`): `x`, `y`, `z`, `d`, `floor`, `cut`. Only
x/y/z/d reach the output; `DoorTarget.Cut` is never set.

`MapRoomItem : MapEdge` (`:339-360`):

| Field | Meaning |
|---|---|
| `id` | AOT id |
| `globalId: short?` | The slot identity in the output. Shared between both mansion RDTs and sometimes between two room entries |
| `type`, `amount` | Vanilla contents |
| `lockId` | Item-box or drawer lock id; reserved from the lock-id pools |
| `group` | Placement bit mask |
| `optional`, `document`, `hidden` | Priority and eligibility flags |
| `typeOffsets: string[]?` | Extra script bytes holding the item type, patched by the mod builder |
| `requires2` | Inherited requirements |

Unread here: `nop`, `offsets`, `itemId`, `link`, `priority`, `requires`,
`requiresRoom`, `allowDocuments`.

`MapRoomEnemies : MapFilterable` (`:362-382`). Read: `id` (a fixed enemy to
retype; null means "clear the room and create new ones"), `globalId`, `kind`
(`"npc"` excludes the spec), `includeTypes`, `excludeTypes` (enemy type ids),
`maxDifficulty`, `condition` (an `ScdCondition` string). Unread: `nop` (the RDT
patcher only), `excludeOffsets`, `keepState`, `keepAi`, `keepPositions`,
`ignoreRatio`, `y`, `restricted`, `randomPlacements`.

`MapRoomNpcs`, `MapRoomCutscene`, `MapRoomCutsceneActor` (`:384-411`) serve the
cutscene and NPC rando. Actor `offsets` are mixed string and number arrays.

`MapEdge : MapFilterable` (`:422-427`): `name`, `requires2: string[]?`.
`Requirements` parses each entry.

`MapRequirement` (`:429-470`): regex `^([a-z]+)\(([^()]*)\)$`, with kind
`flag`, `item` or `room` (case-insensitive enum parse) and a string value. Data
examples: `item(52)`, `flag(COURTYARD_LIFT_POWER)`, `room(…)`. An invalid
requirement throws.

`MapFilterable` (`:480-500`): `tags: string[]`, `doorRando: bool?`,
`player: int?`, `scenario: int?`. `IsIncludedInFilter` keeps the entry if every
non-null field equals the filter value.

`MapFilter` (`:502-514`): `DoorRando`, `Player`, `Scenario` (always 0 for RE1).

### 3.3 `For(filter)` semantics (`:131-145`, `:260-275`)

- `For` filters `beginEndRooms`, rooms (by the room's own filter fields; in
  practice only `player` applies because of the `doorRando` hiding above), and
  inside each room the doors, items, flags, enemies, npcs and cutscenes.
- The same door can appear twice with `doorRando: false` and `doorRando: true`
  variants. For example `40C` door 0 is a vanilla `unblock` door in one variant
  and a `segment-end` in the other; `302` "LADDER TO CAVES" has two variants.
- `For` **shares** the item catalogue and the door, item and enemy objects. It
  copies only the arrays. That is safe only because every `GetJson` call
  returns a fresh object graph.
- **Rust:** make `filter()` produce an owned `Map`.

### 3.4 Rust/serde shape

The shape is plain structs with `Option`s and serde defaults:

- `Map.items`: `IndexMap<u8, ItemDef>`. The JSON keys are strings, so parse
  them as integers.
- `rooms`: `IndexMap<String, Room>`. Remove entries with **`shift_remove`**,
  never `swap_remove`, to keep the .NET order semantics.
- `rdts: Vec<RdtId>`, with `RdtId` implementing `Deserialize` from a string.
- Mixed literal arrays (`nop`, `offsets`, actor `offsets`):
  `#[serde(untagged)] enum Literal { Int(i64), Str(String) }`.
- `allowedLocks`: keep the `Option<Vec<u8>>` distinction between `None` and
  `Some([])`.
- `kind`: `Option<DoorKind>`.
- `tags`: `Vec<String>`, or a small tag enum with a catch-all.

The stages use references between rooms (`GetOtherSide`, targets). Use room keys
or a `(room_idx, door_idx)` index built after loading, not pointers.

### 3.5 Companion data files

- `meta/re1/doors.csv` (`Target,X,Y,Z,D`; 246 rows, `Target = "RDT:doorId"`):
  door entrance positions. Every door that door rando may connect needs one.
- `meta/re1/enemy.csv` (`Room,X,Y,Z,D,Include,Exclude`; 799 rows; Include and
  Exclude are `|`-separated enemy keys and empty in the data): candidate spawn
  positions.
- `BR/Csv.cs` is a simple header-mapped CSV parser with RFC-style quotes; use
  the `csv` crate.

---

## 4. Configuration definition (the launcher UI schema)

### 4.1 Schema shape (`BC/RandomizerConfigurationDefinition.cs`, v1.3.1)

```
RandomizerConfigurationDefinition { pages: Page[] }
Page      { label: string, groups: Group[] }
Group     { label: string, warning?: string, items: GroupItem[] }
GroupItem { id?: string, label?: string, description?: string,
            category?: GroupItemCategory, type?: string, size?: int,
            min?: double, max?: double, step?: double,
            options?: string[], default?: any }
GroupItemCategory { label?: string, textColor?: string, backgroundColor?: string }
```

- `AllItems` flattens pages → groups → items in order.
- `GetDefault()` returns a `RandomizerConfiguration` with `id → default` for
  every item.
- In v1.3.1 there is no `advanced` or `multiplier` field; those came later.
- `size` is never set by RE1.
- A group label `""` means an untitled group.

**Item types used by RE1:**

| `type` | Value type | Fields used | UI |
|---|---|---|---|
| `switch` | bool | `default` | Toggle |
| `dropdown` | string | `options[]`, `default` | Select. Special option strings: `Random`, `Random (Segmented)`, `Always (Segmented)`, `Default` (resolved by `ApplyConfigModifications`; §1.1) |
| `range` | number (int-like or fractional) | `min`, `max`, `step`, `default` | Slider or number. `step` is 1 for counts and 0.01 for weights |
| `percent` | number (fraction; 1.0 = 100%) | `min`, `max`, `step`, `default` | Slider shown as a percentage. **`max` can exceed 1**: starting ammo goes to 8 (800%) |

`category` adds a coloured chip on an item: item distribution items are tagged
Ammo (`#66f`/`#fff`), Health (`#696`/`#fff`) or Ink (`#000`/`#fff`)
(`ClassicRandomizer.cs:583-622`); enemy items get their group's colours.

### 4.2 Values (`BC/RandomizerConfiguration.cs`)

- The configuration is a flat `Dictionary<string, object>` keyed by the item
  `id`. The JSON form is a flat object.
- `FromJson` and `FromDictionary` normalize JSON values (`:174-215`): null
  entries are dropped; booleans stay booleans; strings stay strings; a number
  with an integral value becomes `int` (or `long`), otherwise `double`.
- `GetValueOrDefault<T>(key, default)` (`:142-152`) uses
  `Convert.ChangeType(value, T)`:
  - `int → double` works.
  - **`double → int` rounds half to even** (2.5 becomes 2).
  - `0`/`1 → bool` works.
  - A string → number parses (culture-sensitive).
  - Anything else throws.

  Port this as a small `ConfigValue` enum (`Bool`, `Int`, `Float`, `Str`) with
  lenient `as_*` accessors.
- `Clone()` and `operator +` (merge right over left).

### 4.3 RE1 pages, groups and items (in order)

These are built in `ClassicRandomizer.cs:43-581`, then
`Re1ClassicRandomizerController.UpdateConfigDefinition` (`:8-133`) inserts more.
`(d)` marks the default; `«…»` marks entries generated from data.

**Page "General"**

- Group `""`:
  - `title/call/random` switch (d true)
  - `player` dropdown `[Chris, Jill, Random]` (d Random)
  - `cutscenes/disable` switch (false)
  - `ink/enable` dropdown `[Default, Never, Always, Random]` (d Always)
  - `hard` switch (false)

  The last four are appended by the controller.
- Group "Progression (non-door randomizer)", inserted at index 1:
  - `progression/guardhouse` `[Random, Always, Never]` (d Always)
  - `progression/mansion2` `[Random, Always, Never]` (d Always)
  - `progression/caves` `[Always, Always (Segmented)]` (d Always (Segmented))
  - `progression/lab` `[Random, Random (Segmented), Always, Always (Segmented), Never]` (d Always (Segmented))
  - `progression/tyrant1` `[Random, Always, Never]` (d Random)
  - `progression/tyrant2` same options (d Random)
- Group "Door Randomizer":
  - `doors/random` switch (true)
  - `doors/segments` range 1..6 (d 3)
  - `doors/rooms` percent 0..1 step 0.1 (d 0.4)
- Group "Door Locks":
  - `locks/random` switch (false)
  - `locks/preserve` switch (false)
  - `locks/ratio` percent 0..1 step 0.01 (d 0.25)

**Page "Inventory"**

- Group "Main":
  - `inventory/weapon/knife` dropdown `[Random, Never, Always]` (d Always)
  - `inventory/min` range 0..8 step 1 (d 4)
  - `inventory/max` range 0..8 (d 8)
  - `inventory/special/lockpick` `[Default, Random, Never, Always]` (d Default), added by the controller
  - `inventory/size` `[Default, Random, 6, 8]` (d Default), added by the controller
- Groups "Primary Weapon" and "Secondary Weapon":
  - `inventory/{primary|secondary}/none` switch (true)
  - one switch per non-knife weapon, `inventory/{slot}/«kind»` (true), labelled with the item name
- Group "Extras" (warning: "A random selection is chosen until inventory is full."):
  - `inventory/{primary|secondary}/ammo/min` percent 0..8 step 0.1 (d 1)
  - `inventory/{primary|secondary}/ammo/max` percent 0..8 (d 2)
  - per `health/*` item: `inventory/«kind»/min` range 0..10 (d 0) and `…/max` (d 2)
  - `inventory/ink/min` range 0..32 (d 0)
  - `inventory/ink/max` range 0..32 (d 3)

**Page "Items"**

- Group `""`:
  - `items/random` (true)
  - `items/hidden/keys` (true)
  - `items/documents` (true)
  - `items/documents/keys` (true)
- Group "Weapons": `items/weapon/enabled/«kind»` switch (true) for every weapon, knife included.
- Group "Weapons (Initial Ammo)", for weapons with `max > 0`:
  - `items/weapon/initial/min/«kind»` range 0..max (d 0)
  - `items/weapon/initial/max/«kind»` range 0..max (d max)
- Group "Stack", for `ammo/*` and `ink`:
  - `items/stack/min/«kind»` range 1..max (d 1)
  - `items/stack/max/«kind»` range 1..max (d 15)
- Group "Item Distribution", for ammo, health and ink:
  - `items/distribution/«kind»` range 0..1 step 0.01 (d 0.5), with `category`

**Page "Enemies"**

- Group `""`:
  - `enemies/random` (true)
  - `enemies/safe` (false)
  - `enemies/save` (true)
  - `enemies/box` (true)
  - `enemies/rooms` percent 0..1 step 0.01 (d 0.5)
- Group "Room Distribution": `enemies/distribution/«entry.key»` range 0..1 step 0.01 (d 0.5), with the group's category.
- Group "Min/Max per Room":
  - `enemies/min/«key»` range 1..16 (d 1)
  - `enemies/max/«key»` range 1..16 (d 6)
- Group "Skins": `enemies/skin/«folder»` switch (true) for each folder under data `re1/emd/`, labelled `ToActorString()`.

**Page "Protagonist"**

- `protagonist/random` (false)
- Group "Characters": `protagonist/character/«folder»` (true) for `re1/pld0/*` and `re1/pld1/*`, sorted by label.

**Page "NPCs"**

- `npc/random` (false)
- Group "Characters": `npc/character/«folder»` (true), same list.

**Page "Music"**

- `music/random` (false)
- Group "Games": `music/game/«dir»` (true) for each `bgm/*` folder, label upper-cased, sorted.

The data-driven entries come from `rdt.json`, so the launcher can build the
whole UI from `rdt.json` plus a directory listing of the BioRand data pack. The
schema can also be generated in Rust and serialized to JSON in the shape above,
so the launcher renders it generically.

---

## 5. Output model (`ClassicMod`, `BR/ModBuilder.cs:98-188`)

`ClassicMod.ToJson()` uses camelCase, indented, relaxed escaping, and converters
for `RdtId` (a string such as `"106"`) and `RdtItemId` (a string such as
`"106:2"`, including as a dictionary key). The mod carries it as
`generated.json`, and `ClassicMod.FromJson` can rebuild it. **This is the
natural contract between the Rust randomizer and arklay.**

| Field | Type | Producer | Meaning |
|---|---|---|---|
| `game` | string | pipeline | `"re1"` |
| `name`, `description` | string | pipeline | `"BioRand \| profile \| seed"`; the multi-line description |
| `general` | `{string: any}` | controller and title | See below |
| `inventory` | `RandomInventory[]` | Inventory | Exactly one entry: the chosen player's start inventory |
| `doors` | `{"rdt:doorId": DoorTargetLock}` | Door, Lock | Per-RDT door overrides |
| `items` | `{globalId: Item}` | Key, Item | Every randomized item slot. `{type:0, amount:0}` = empty |
| `enemyPlacements` | `EnemyPlacement[]` | Enemy | Retypes and new spawns |
| `music` | `{bgmPath: MusicSourceFile}` | Music | `{path, game, tag}` |
| `characters` | `{charId: CharacterReplacement}` | Character | `{path, weapon?}` (weapon omitted when 0) |
| `enemySkins` | string[] | skins | Chosen `re1/emd/<skin>` folders, without id overlaps |
| `npcs` | `NpcReplacement[]` | Cutscene | `{rdtId, offset, type}`: retype the enemy opcode at that offset |
| `voices` | `{string: string}` | Cutscene | Voice clip target → source |
| `seed` | int? | pipeline | |
| `configuration` | object | pipeline | The **unresolved** input config |

**`general`** keys (`Re1ClassicRandomizerController.cs:603-615`, plus
`ClassicRandomizer.cs:843`):

| Key | Type | Value |
|---|---|---|
| `player` | int | 0 = Chris, 1 = Jill |
| `hard` | bool | |
| `randomDoors` | bool | |
| `randomItems` | bool | |
| `randomEnemies` | bool | |
| `cutscenesDisabled` | bool | |
| `forceTyrant` | bool | `progression/tyrant2 == Always`: forces the helipad Tyrant |
| `lockpick` | bool | |
| `ink` | bool | Ink ribbons required to save |
| `inventorySize` | int | 6 or 8 |
| `titleSound` | string | Optional: data path of the title call clip |

`Re1RdtPatcher` reads `player`, `randomEnemies`, `randomItems`, `randomDoors`,
`ink`, `hard`, `lockpick` and `forceTyrant` (`Re1RdtPatcher.cs:18-25`).

**`RandomInventory`** (`BR/RandomInventory.cs`): `entries: [{type: byte,
count: byte, part: byte}]` and `special: Entry?` (always null in RE1).
Inventory amounts are stored as **bytes** (`(byte)item.Amount`,
`InventoryRandomizer.cs:318`), so flamethrower fuel above 255 would wrap. Max
is 240, so this is fine today.

**`DoorTargetLock`** (`:171-177`): `target?: DoorTarget`, `lock?: DoorLock`.
Both are omitted when null. A door entry with only `lock: null` means "write
lock 0/0". `ApplyDoors` treats a present entry with no lock as an explicit
unlock (`RE1/Re1CrModBuilder.cs:496-505`). Note that `SetDoorLock(…, null)` is
called for **every** door with an id, so every such door is in the dictionary.

- **`DoorTarget`** (`:179-188`): `room: RdtId`, `x`, `y`, `z`, `d`, `cut?`
  (never set).
- **`DoorLock`**: `id` (lock flag 1..255; 255 is used for "permanently locked"
  dead ends) and `type`:

  | `type` | Meaning |
  |---|---|
  | 0 | Unlocked |
  | 254 | Unlockable from this side |
  | 255 | Locked, cannot be opened here |
  | other | The key item id required |

**`Item`** (`BR/Item.cs`): `{type: byte, amount: ushort}`.

**`EnemyPlacement`** (`BR/EnemyPlacement.cs`):

| Field | Meaning |
|---|---|
| `rdtId` | |
| `globalId` | Kill flag id |
| `id` | Per-room enemy slot 0..15 |
| `type` | Enemy type |
| `pose` | State byte |
| `x`, `y`, `z`, `d` | Position (create only) |
| `create` | `false` = retype the existing enemy with that `id` |
| `esp: int[]` | Effect ids the room must have |
| `condition?` | An `ScdCondition` string guarding creation |

The `Re1CrModBuilder` artefacts derived from these are covered in §2.15
(`init.xml`, `biorand.dat`, patched RDTs, `log.md`/`html`, `config.json`). arklay
either reproduces them or consumes `ClassicMod` directly.

---

## 6. Port plan

### 6.1 Package layout

The repository is a single package today (no `[workspace]` in `Cargo.toml`).
To keep the main crate's compile cost unchanged (AGENTS.md), add a workspace
with a new member, for example `biorand/` (package `arklay-biorand`, lib
`arklay_biorand`). Its dependencies would be only `serde`, `serde_json`,
`indexmap`, a PRNG crate (`rand_pcg` or `rand_chacha`, or a hand-rolled
xoshiro), `csv` (or a tiny parser), and optionally `im` for the route finder.
It has no dependency on the `arklay` crate. arklay, or a launcher binary,
depends on it, or consumes its JSON output.

```
arklay_biorand/
  lib.rs            pub fn randomize(input, data) -> Result<ClassicMod, RandoError>; pub fn definition(data) -> ConfigDefinition
  rng.rs, bag.rs    Rng (named sub-streams), EndlessBag, shuffle
  config/{definition.rs, values.rs, definition_re1.rs}
  map/{mod.rs, requirement.rs, filter.rs, condition.rs, csv.rs}
  routing/{node.rs, graph.rs, builder.rs, finder.rs, solver.rs, multiset.rs}
  re1/{ids.rs, controller.rs, map_prep.rs}
  rando/{inventory.rs, doors/, locks.rs, keys.rs, items.rs, enemies.rs, cosmetics.rs}
  output.rs         ClassicMod + serde (byte-compatible with generated.json)
  pipeline.rs
```

### 6.2 Suggested order, with rough Rust LOC (code + unit tests)

| # | Step | Depends on | Rust LOC (code / tests) |
|---|---|---|---|
| 1 | `RdtId`/`RdtItemId` (or reuse arklay's), `rng`, `bag`, shuffle | none | 150 / 80 |
| 2 | `map` serde model, comment stripping, `For` filter, `MapRequirement`, `GetOtherSide`, CSV loaders; test by loading the real `rdt.json` and checking counts (122 rooms, 76 items) | 1 | 550 / 150 |
| 3 | `config` values (lenient coercion, banker's rounding) and definition types, plus the RE1 definition builder; snapshot-test the JSON | 2 | 650 / 120 |
| 4 | `output` (`ClassicMod` serde round-trip compatible with C# `generated.json`) | 1 | 220 / 80 |
| 5 | `routing`: builder (with the inverse-edge rules), graph, solver, finder; port the biorand-common routing tests (`~/src/biorand-common/test/…`) | none | 1300 / 500 |
| 6 | RE1 controller: random/default resolution, `General`, `map_prep` | 2, 3, 4 | 450 / 150 |
| 7 | `InventoryRandomizer` | 6 | 200 / 60 |
| 8 | `LockRandomizer` (`SetDoorLocks` first: it runs even without lock rando) | 6 | 400 / 120 |
| 9 | `KeyRandomizer` (graph from the map, amounts, segment tree) | 5, 8 | 280 / 150 |
| 10 | `ItemRandomizer` | 9 | 180 / 60 |
| 11 | `EnemyRandomizer` | 6 | 280 / 80 |
| 12 | `DoorRandomizer` (largest and most fragile; do it after the vanilla-layout pipeline works end to end) | 8, 9 | 850 / 250 |
| 13 | Cosmetic stages: skins, title, music, characters, cutscene/NPC/voice (optional; the cutscene stage alone is about 450) | 6 | 650 / 80 |
| 14 | `pipeline.rs` with a retry-on-failure loop and invariant checks | all | 150 / 200 (property tests over many seeds) |

Core without the cosmetic stages is about **5,700 LOC of code and 2,000 of
tests**. With cosmetics it is about 6,400 + 2,100. Applying `ClassicMod` inside
arklay is separate engine work and is not counted.

Milestones:

1. Steps 1–11 with `doors/random = false` give a vanilla-layout key, item and
   enemy rando.
2. Step 12 adds door rando.

### 6.3 .NET behaviours to preserve (correctness, not parity)

- **Insertion order.** `Dictionary` enumeration in insertion order (with only
  removals afterwards) drives the order of `map.Rooms`, `map.Items`,
  `GetDoorPairs`, graph node ids and config page order. Use `IndexMap` with
  `shift_remove`.
- **Rounding.** `Math.Round` is half-to-even: `doors/rooms`, `locks/ratio`,
  `enemies/rooms`, the difficulty cap, inventory ammo, and the
  `Convert.ChangeType` double→int coercion. Use `f64::round_ties_even`
  (MSRV 1.88 is fine).
- **LINQ semantics:**
  - `OrderBy`/`OrderByDescending` are **stable**; Rust `sort_by` is too.
    `Array.Sort` in `RouteFinder` is unstable, which does not matter.
  - `Except`, `Intersect` and `Distinct` are *set* operations that keep the
    first sequence's order and drop duplicates.
  - `First`, `Single` and dictionary indexers throw; decide `Result` versus
    panic deliberately.
  - `FirstOrDefault` on a `KeyValuePair` returns `(0, null)` (the knife lookup
    and the mansion 2 key).
  - Deferred evaluation: `Segment.UnconnectedDoors` and `AvailableDoors` are
    lazy and are re-read after mutations inside loops (`DoorRandomizer.cs:334-349`).
    Rust must collect snapshots, or iterate indices, in the same places.
- **Reference identity.** `MapRoom`, `MapRoomDoor`, `RoomPiece` and
  `RoomPieceDoor` are compared by reference in hash sets and `==`. Use arena
  indices.
- **Exceptions as control flow and failure:**
  - `DoorRandomizer` throws on structural failure.
  - `KeyRandomizer` throws after 1000 dead ends.
  - `ModBuilder.SetItem` throws on a duplicate slot.
  - `Queue.Dequeue` throws on exhaustion.

  **Nothing retries**: a bad seed is an error to the user. The Rust port should
  return `Result<_, RandoError>`. Optionally retry with a derived seed, and
  report the seed actually used.
- **Silent partial routes.** `KeyRandomizer` never checks
  `route.AllNodesVisited`. In Rust, check it (and run `RouteSolver` on the final
  route) and fail or retry rather than ship an unbeatable seed.
- **The `null` versus `[]` distinction** on `allowedLocks` (and on
  `requires2`) is semantic; keep `Option<Vec<_>>`.
- **Floating-point edge cases.** `(int)Math.Ceiling(NaN)` is `int.MinValue`
  when a weight sum is 0. Guard against it explicitly.
- **Mutation order.** Stages communicate through the mutated `Map`:
  `DoorRandomizer` sets kinds, tags and lock ids that `LockRandomizer` reads;
  `LockRandomizer`'s `Requires2` feeds `KeyRandomizer`. Keep one `&mut Map`
  threaded through the stages in the same order.
- **Config coercion.** A missing key falls back to the code default, which is
  not the definition default. Merge definition defaults into the input in the
  launcher, or in `randomize()`, to make behaviour explicit.

### 6.4 Risks

1. **`RouteFinder` complexity and performance.** It is recursive backtracking
   with 10 retries per edge per level and full immutable-state snapshots, and it
   runs `RouteSolver` at every recursion. Rust needs persistent collections or
   careful cloning. Add depth and dead-end budgets and fuzz it over many seeds.
2. **Subtle graph-builder rules.** The inverse-edge reconciliation, the
   "unblock" requirement drop in `KeyRandomizer`, and the key-255 workaround in
   `LockRandomizer` exist to keep the graph library consistent. Port them
   exactly and cover them with tests.
3. **`KeepBoxRouteClear`.** Enumerating every simple path can grow
   exponentially. Bound it.
4. **Door-rando data assumptions.**
   - Every connectable door needs an entrance (CSV or inline).
   - There must be at least `segments - 1` segment-end pieces.
   - The target RDT is `rdts[0]`; the mansion-copy behaviour needs checking
     against arklay's stage-6 handling.
5. **The data pack.** The config UI and the cosmetic stages need BioRand's data
   tree (`re1/emd`, `re1/pld*`, `bgm`, `title`), not just `meta/re1`. Decide
   what the arklay launcher ships or downloads. `rdt.json`, `doors.csv` and
   `enemy.csv` are small and can be embedded (`include_str!`).
6. **Applying the output in arklay.** `ClassicMod` describes edits in RDT terms:
   `rdt:doorId`, item `globalId`, enemy slot ids, script offsets for `npcs`, and
   `typeOffsets`. arklay must map these onto its room model. The ~45
   `Re1RdtPatcher` fixes (`Re1RdtPatcher.cs:62-1000`) are gameplay requirements
   for randomized seeds, such as the plant 42 door, MO disc readers and
   lockpick handling. Without them, some seeds softlock in-engine even though
   the route is valid. This is the largest hidden scope, and it is outside the
   randomizer crate.
7. **Upstream drift.** biorand-common `main` (1.6.x) has route-finder fixes
   (`224aa88` dead-end diagnostics, `efa189e` two-way solver fix). Since parity
   is not required, review those and consider porting the fixed behaviour
   instead of the 1.3.1 one.
8. **Kill-id reuse.** The endless global-id bag can reuse kill flags across
   rooms once it has used all 255 ids. Decide whether arklay needs unique ids,
   and fail or extend the pool if so.
