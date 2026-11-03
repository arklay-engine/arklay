# BioRand in arklay: plan

This ports the RE1 half of BioRand's new randomizer (the `staging` branch of
`biorand/classic`) into arklay: door, lock, item and enemy randomization,
with BioRand's configuration in arklay's launcher. It replaces the proof of
concept in `src/biorand` (33408ab), keeping its pack, manifest, launcher and
guard plumbing.

The milestone tracker is the checklist in the pull request's description.
[`patches.md`](patches.md) lists every patch BioRand made to RE1 and how
arklay takes each one. The research it rests on is in [`research/`](research/):

- [`research/port-map.md`](research/port-map.md): the staging pipeline,
  class by class, and the routing library from `biorand/common`.
- [`research/staging-data.md`](research/staging-data.md): `rdt.json`'s
  fields, staging's data-driven script patches, the mod builder's output and
  the configuration options.
- [`research/exe-history.md`](research/exe-history.md): the executable patches,
  every RE1 patch in BioRand's history, and the bug fixes behind them.

## Scope

In scope: door randomization with segments, lock randomization, item
randomization (keys, weapons, ammunition, healing, documents, the start
inventory) and enemy randomization, with every gameplay option staging
has: progression, hard mode, ink, lockpick, inventory size, disabled
cutscenes and the choice of Chris or Jill.

Out of scope for now: the cosmetic options (enemy skins, voices, NPCs, player
models, music and the title call). Their option ids stay reserved in the
configuration so a saved configuration from staging loads unchanged.

## Decisions

| Question | Decision |
|---|---|
| Same game as staging for a seed? | No. A faithful port of the algorithms with its own random stream; `GENERATOR_VERSION` pins arklay's output. |
| Where does the code live? | A new workspace package, `crates/arklay-biorand`, with no dependency on the engine crate. The engine crate depends on it. |
| Which algorithms? | Staging's `ClassicRandomizer` pipeline and the routing of `biorand/common` at its latest release (1.6.1, which fixes two routing bugs in the 1.3.1 staging pins). Not the old `Re1Randomiser` pipeline. |
| Configuration | The same option ids, labels, descriptions, kinds and defaults as staging, built from a definition the package exports, so the launcher draws it. |
| Room data | Each room's `random.xml` is generated once from staging's `rdt.json`, then owned and edited by hand in arklay. |
| Script changes | Structural patches are written into the base game's `akscr` under runtime guards on reserved system flags. The generator changes only data: door targets, locks, pickups and enemies. |
| Item puzzles | Scripts that test which item *type* was picked up test the pickup's collected flag instead, under the item guard (see below). |
| Locks | The whole lock byte is the flag index in a randomized game. Rebecca's one-character doors become script refusals. |
| Room ids | Door randomization renumbers rooms: segment *n* is stage *n*. Arklay allows any number of stages and up to 256 rooms in a stage. |
| Map screen | Off under door randomization. |
| Music and sound | Each room keeps its original music and sound table rows wherever it ends up. This may change later. |
| RE2 saves | Every base pack but `re1` saves under `<save_dir>/<pack id>/`; RE2's saves move without migration. |
| Testing | Static analysis of generated games only: the verifier and a seed sweep. No playthrough bots. |

## Architecture

```
launcher / CLI ──► config + seed
                     │
                     ▼
   arklay-biorand ──► Map (from the pack's random.xml and biorand/*.xml)
                     │   inventory → doors → locks → keys → items → enemies
                     ▼
                 Generated (the plan: rooms, doors, locks, pickups, spawns,
                     │      start inventory, option flags, room renumbering)
                     ▼
   arklay::biorand ──► emit: a new base pack with rewritten room scripts and
                     │      renumbered rooms, and a manifest with the options
                     ▼
                 verify: reads the finished pack back and checks it
```

### The `arklay-biorand` package

A pure library: no SDL, no pack I/O, no engine types. Its inputs are the map
model (deserialized by the engine crate from the pack), the configuration and
a seed, and its output is `Generated`. That keeps the engine crate's compile
cost unchanged while the generator is iterated on, and lets the generator's
tests run in a second.

| Module | Ports | Notes |
|---|---|---|
| `rng` | `Rng`, `EndlessBag` | Named forks per stage, as staging has. |
| `config` | `RandomizerConfigurationDefinition`, `RandomizerConfiguration`, both controllers' option lists | Exports the definition the launcher draws; reads values leniently, as staging does. |
| `map` | `Map.cs`, `MapTags`, `DoorKinds`, `DoorLock`, `ScdCondition` | Rooms keep insertion order (`IndexMap`). |
| `routing` | common's `Graph`, `GraphBuilder`, `RouteFinder`, `RouteSolver`, `Route`, `Edge`, `Requirement`, `DeadEndInfo` | The largest single piece. |
| `prepare` | `Re1ClassicRandomizerController.GetMap` | Applies the progression options to the map. |
| `inventory`, `doors`, `locks`, `keys`, `items`, `enemies` | the staging randomizers of the same names | Doors last, as it is the riskiest. |
| `generated` | `ClassicMod` | The contract with the engine crate. |

The engine crate keeps `src/biorand/` for what touches packs: reading the map
from `random.xml`, emitting scripts, renumbering rooms, the manifest, verifying
and the launcher screen.

### Room data: `random.xml`

The proof of concept's `random.xml` grows into a full room entry of staging's
map: each door's target, kind, lock, key, requirements, entrance and tags;
each item's pickup flag, type, amount, group, requirements and whether it is a
document or hidden; and each enemy group's exclusions, spots and difficulty.
When added enemies appear is not data: the room's script says it (see
[enemy placeholders](#enemy-placeholders)). Map-wide data (item definitions, enemy definitions, begin and end
rooms) goes in `games/re1/biorand/`. A one-off converter writes them from
`rdt.json`; from then on they are edited by hand, and each room's file
documents any difference from staging that a fix needed.

Mansion rooms are now one room for both visits (#328), so a room entry covers
both. A room whose second visit staging treats as a separate door-rando room
(the `mansion2able` tag) is an open item for the door milestone.

### Runtime guards and system flags

The proof of concept added `if (RANDOMIZER)`-style guards: an `akscr` test of
a system flag in bank 5 that compiles to an ordinary `ck` at run time, and that
a room's retail build folds to the `else` arm so retail bytes hold. The
engine raises the flags from the pack manifest at the start of a game and
again when a save loads, so saves need not carry them.

Bank 5 is 256 bits in arklay; the original game uses only low bits (room 205
tests `0x40`, the highest seen). BioRand reserves bits `0xC0` to `0xFF`:

| Bit | Name | Raised when |
|---|---|---|
| `0xC0` | `RANDOMIZER` | any generated game |
| `0xC1` | `DOOR_RANDOMIZER` | `doors/random` |
| `0xC2` | `ENEMY_RANDOMIZER` | `enemies/random` |
| `0xC3` | `ITEM_RANDOMIZER` | `items/random` |
| `0xC4` | `LOCK_RANDOMIZER` | `locks/random` |
| `0xC5` | `CUTSCENES_DISABLED` | `cutscenes/disable` |
| `0xC6` | `FORCE_TYRANT2` | `progression/tyrant2` is Always |
| `0xC7` | `LAB` | the game reaches the laboratory |
| `0xC8`–`0xCF` | engine behaviour | see [`patches.md`](patches.md#a-executable-patches): full lock byte, killable Neptune, Yawn's poison, extra herb mixes, ink for both characters, no title demo |
| `0xD0`–`0xFF` | spare | |

The proof of concept used `0xF0` to `0xF2`; they move. Staging's option flags
in the scenario bank (122 ink, 123 hard, 124 lockpick) keep their banks and
meaning, because the original's scripts and engine already read 123 and 124;
the engine sets them from the manifest instead of a script in room 106.

### Item puzzles: collected flags, not item types

Some rooms react to *which item type* was just picked up:
`cmpb(B_PICKED_ITEM, CMP_EQ, IT_05_WEAPON_MAGNUM)` at the tiger statue, or
`testpickup SHOTGUN` in the ceiling trap room. Staging rewrites the type
operand to the new item's type (`typeOffsets`). That breaks when two slots
hold the same type, and it misses several checks (see
[`patches.md`](patches.md#c-item-type-tests)).

Newer RE games react to the slot's collected flag instead. Arklay does the
same. Every `item_aot_set` already names its pickup flag in `FG_ITEM`. `akscr`
gets one condition for it:

```
if (picked(AOT_04_ITEM))
```

The compiler lowers it to the room's original `cmpb`/`testpickup` in the
retail build. In other builds it puts a remap before that same test: under
`ITEM_RANDOMIZER`, when the AOT's pickup flag is set and something was picked
this frame, the picked item id becomes the original item. The pickup flag is
read from the room's own `item_aot_set` for that AOT, so the generator never
touches these tests and a slot can hold anything. A command `if` cannot be an
OR and jumps by one byte, so duplicating the test's bodies for a flag branch
overflows long `main`s (room 10D); remapping the id keeps one copy.

The flag is a level and the original test a pulse: the picked item id is
cleared after every frame, so a script that polls it in `proc main` sees the
pickup for one frame, where a collected flag stays set for good. The remap
therefore also requires a pickup this frame (`B_PICKED_ITEM != 0`). It still
misfires when another item in the same room is picked up after this slot was
collected. An exact pulse needs the engine to say which AOT was picked; that
is the follow-up if a room shows it. `picked_as(AOT, ITEM);` is the remap on
its own, for a test too far from its `if` (room 10D).

### Locks

A door's lock byte means `0x80` checked, `0x40` refused for Rebecca and
`& 0x3F` the flag in bank 2. Under `LOCK_RANDOMIZER` (and whenever
`RANDOMIZER` is set, as staging always rewrites locks), the whole byte is the
flag index: 0 is unlocked and 1–255 are lock flags. Bank 2 is already 256 bits
in arklay at run time, but a save keeps only its first 8 bytes, as the
original's BioCard does. So a save gains an `AKY\x03` trailer (after the height and RE2 trailers)
holding bank 2's bytes 8 to 31, as staging's `biorand.dll` added one. It is
written only when one of those bytes is non-zero, so retail saves are
unchanged and an older save loads with the high flags clear. Scripts that raise a lock flag by number (`set(FG_LOCK, ...)`)
use the room's lock names, which the emitter renumbers with the door.

The `0x40` Rebecca refusal goes: the few doors that carry it get a script
check in their rooms instead. While Rebecca is the player, the door's AOT is
replaced by a message AOT that shows the original's refusal.

### Room ids and segments

Under door randomization the generated game renumbers its rooms. Segment *n*
becomes stage *n*, and its rooms are numbered from 0 in the order the route
reaches them. That needs two engine changes, both in their own milestone:

- `RoomId` takes any stage and room numbers 0–255. The id's text form grows
  to fit, and RE2's letter stages are unchanged.
- Engine behaviour keyed to an original room (room 303's helicopter, 302's
  lift, second-visit-only rooms, the music and sound tables) is keyed to the
  room's *origin*, which `room.xml` records. A renumbered room keeps the
  original's behaviour, music and sounds.

The map screen is off under `DOOR_RANDOMIZER`.

### Enemy placeholders

Staging wraps the enemies it adds in a condition string from `rdt.json`
("not during Jill's prologue"), and patches rooms to clear the script's own
enemies or to raise helper flags for those conditions. In arklay the room's
`akscr` says where added enemies go:

```
if (ck(FG_SCENARIO, F_SCENARIO_05, FALSE))
{
    if (!ENEMY_RANDOMIZER)
    {
        sce_em_set(...);   // the original's enemies
    }
    random_enemies();
}
```

`random_enemies()` sits inside the branch the script already uses for its
own enemies, so the cutscene and story guards come for free. The retail build
compiles it to nothing; the base pack's build compiles it to a no-op marker
instruction. The emitter finds the marker in the decoded script and replaces
it with the plan's `sce_em_set`s; an unused marker stays a no-op. A room
needing added enemies in more than one branch has more than one marker,
named (`random_enemies(jill)`), and its `random.xml` says which spots belong
to which. This replaces staging's 25 enemy conditions, its ClearEnemies
patches and room 308's helper flag.

### Emitting and verifying

As in the proof of concept, the generator works on the pack's compiled
scripts, not the `akscr` sources, so it needs neither the sources nor the
compiler and works on a room a mod replaced. It never patches raw bytes: the
emitter decodes each used room's script, sets the fields the plan names (door
targets, entrances, locks and keys, pickups, enemy sets wrapped in their
placeholders), encodes it again and writes a new base pack. `verify` reads the
finished pack back and checks that every room is reachable, every key comes
before its lock, nothing waits on a story flag the route can't raise, and every
guard a patch needs is reached. A pack that fails is deleted.

## Milestones

The pull request's description holds the live checklist. In order:

1. **Foundations.** This plan, the patch catalogue and the research. The proof
   of concept rebased onto main: the rocket stopgap dropped (main has the real
   rocket), and its two uncommitted fixes already on main.
2. **Package and data.** `crates/arklay-biorand` with `rng`, `config` and
   `map`; the `rdt.json` converter; `random.xml` for every room; the map-wide
   files; the configuration definition matching staging.
3. **Routing.** Port common's route finder, with its unit tests.
4. **Engine flags.** The reserved system flags from the manifest; the
   executable-patch behaviours; the full lock byte; `picked()` in `akscr`.
5. **Item and key randomization** on the original layout: inventory, keys and
   items; the item puzzles; maps and documents as items; the MO disc readers;
   Jill's missing pickups. *Test milestone 1:* a full RE1 game with shuffled
   items.
6. **Lock randomization**, with the scripted locks (115, 201/214) and the
   Rebecca refusals.
7. **Enemy randomization**, with the enemy placeholders, the cleared rooms, the
   ambushes and the crash guards. *Test milestone 2.*
8. **Room ids.** Any stage and room number; room origins; the map screen off.
9. **Door randomization**: segments as stages, renumbered rooms, and the
   door patches (106, 207, the lifts, 303/305, plant 42, the bookcases, 500).
   *Test milestone 3.*
10. **Launcher and logs.** The configuration screen as in staging, seeds, the
    spoiler log and stale-pack rebuilds.
11. **Static analysis and the cutscene option.** The verifier's full checks,
    a seed sweep over the configuration space, and `cutscenes/disable`.
    *Test milestone 4:* ready for main.

## Known issues

- **Consumable keys can still softlock (to fix later).** The route finder is
  meant to rule out every softlock, but since common 1.6.1 its solver never
  reports `PotentialSoftlock`, so a finished route only guarantees that *some*
  order of play completes the game. A player who spends a consumable key on
  the wrong lock first can be stranded: in 1 of 200 RE1 placements in
  `tests/re1_keys.rs` (Chris, seed 16, the Battery in a drawer that needs the
  Small Key), and the greedy solver leaves nodes unvisited in 3 of 100 RE2
  routes. Staging has the same weakness. The fix belongs in the route finder
  itself (restore the potential-softlock check for consumable keys), not a
  reroll around it.
