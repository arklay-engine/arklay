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
                 Plan (rooms, doors, locks, pickups, spawns, start
                     │      inventory, option flags, room renumbering)
                     ▼
   arklay::biorand ──► emit: a new base pack with rewritten room scripts and
                     │      renumbered rooms, and a manifest with the options
                     ▼
                 verify: reads the finished pack back and checks it
```

### The `arklay-biorand` package

A pure library: no SDL, no pack I/O, no engine types. Its inputs are the map
model (deserialized by the engine crate from the pack), the configuration and
a seed, and its output is the `Plan` (`plan::generate`). That keeps the engine crate's compile
cost unchanged while the generator is iterated on, and lets the generator's
tests run in a second.

| Module | Ports | Notes |
|---|---|---|
| `rng` | `Rng`, `EndlessBag` | Named forks per stage, as staging has. |
| `config` | `RandomizerConfigurationDefinition`, `RandomizerConfiguration`, both controllers' option lists | Exports the definition the launcher draws; reads values leniently, as staging does. |
| `map` | `Map.cs`, `MapTags`, `DoorKinds`, `DoorLock`, `ScdCondition` | Rooms keep insertion order (`IndexMap`). |
| `routing` | common's `Graph`, `GraphBuilder`, `RouteFinder`, `RouteSolver`, `Route`, `Edge`, `Requirement`, `DeadEndInfo` | The largest single piece. |
| `prepare` | `Re1ClassicRandomizerController.GetMap` | Applies the progression options to the map: the areas left out, the slot groups, the segmented caves and lab. |
| `inventory`, `doors`, `locks`, `keys`, `items`, `enemies` | the staging randomizers of the same names | Doors last, as it is the riskiest. |
| `plan` | `ClassicRandomizer.Randomize`, `ClassicMod` | Runs the stages in order; the plan is the contract with the engine crate. |

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
`& 0x3F` the flag in bank 2. Under `BR_LOCK_FULL_BYTE`, which a pack raises
with `LOCK_RANDOMIZER` (staging rewrites the bytes whenever it randomizes; here
an item-only game keeps the original bytes and reading), the whole byte is the
flag index: 0 is unlocked and 1–255 are lock flags. Bank 2 is already 256 bits
in arklay at run time, but a save keeps only its first 8 bytes, as the
original's BioCard does. So a save gains an `AKY\x03` trailer (after the height and RE2 trailers)
holding bank 2's bytes 8 to 31, as staging's `biorand.dll` added one. It is
written only when one of those bytes is non-zero, so retail saves are
unchanged and an older save loads with the high flags clear.

The package's lock stage (`locks.rs`, after the inventory) follows staging's
`LockRandomiser`: it frees the doors the map allows, keeps the box route and
the item rooms reachable, then draws locks over the remaining pairs at the
configured ratio with flags from 64 up (then 1–63), and keys for them from
the configuration's key pool. The plan lists every door's `(lock, key)`:
the key byte is an item, `0xFE` (opens from this side), `0xFF` (never opens
from this side) or 0 (says it is locked). Three deviations from staging, each
a staging bug the playthrough walk caught: a door that leads nowhere is
locked for good (`0xFF`, `0xFF`) rather than left open; a `no-unlock` pair is
never given a lock (staging can lock it with no way to open it); and a door
that lost its lock also loses the original's key requirement, so keys are not
placed for a lock that is gone.

The emitter writes every `door_aot_set` of every room script: a door the plan
has (matched by room, AOT id and target room) gets the plan's bytes in every
branch that declares it; any other door keeps its key and gets its original
flag (`byte & 0x3F` when `0x80` is set, else 0), so the scripts' own
`set(FG_LOCK, ...)` still open it. Room 115's scripted doors are the
exception, as the map gives them flags 0x0F and 0x10: 115 mirrors its
`F_LOCK_00`/`01` onto those under the guard, and 109 and 116 raise them from
their side (staging's `ShotgunOnWallFix` renumbering). `verify` checks every
door's bytes against the plan and that every planned locked door is declared.

The `0x40` Rebecca refusal goes: the doors that carry it (27 in 13 rooms) get
a script check instead. Under `BR_LOCK_FULL_BYTE`, while Rebecca is the
player (`B_CHARACTER_MODEL` 3), the door's AOT is replaced by a message AOT
that shows the engine's wrong-character message (0xD6).

### Room ids and segments

Under door randomization the generated game renumbers its rooms. Segment *n*
becomes stage *n*, and its rooms are numbered from 0 in the order the route
reaches them. Milestone 8 made the engine ready for that:

- **Ids.** `RoomId` takes any stage (1–255) and room (0–255). An id the
  original forms can spell keeps them: `10b` for a room folder, `10b1` for
  an RDT, and RE2's letter stages (`a00`–`g1f`). Any other id has the long
  form, two hex digits each: `09-40` for the room, `09-40-1` for one
  player's RDT. The long form of an id with a short one (`01-0b`) is
  refused, so every id has one spelling and every form round-trips.
- **Origins.** `<room origin="106">` in `room.xml` names the original room
  a room was copied from; a room without one is its own origin. The pack
  reads every room's origin once (`Pack::room_origins`), and engine
  behaviour keyed to an original room reads the origin
  (`RoomOrigins::of`, `GameState::origin`): the quirks (303's helicopter,
  the drug storehouse, the guardhouse save, the lab cues, the special
  lights, the object and item fix-ups, the shadow and depth tables), the
  music (`room_bgm` row and slot, RE2's music places), the sound and voice
  rows, the effect sheets and camera lights, the masks, the visited bits,
  the enemy and Lua room views, and the `B_STAGE`/`B_ROOM` state bytes the
  scripts read. A room plays the second mansion visit when its origin is a
  mansion room (`RoomOrigins::with_visit`). The room's files, its text keys
  (falling back to the origin's), door targets, saves and the debug menu
  use the room's own id. A table indexed by the original's 32 rooms per
  stage never reads past them: a room it does not hold gets none of its
  rows.
- **Door targets.** A script's door target byte names only an original room
  (`< 0x20` a room of the same stage, else the table row plus one in bits
  5–7 and the room in bits 0–4), and the retail scripts stay byte-identical.
  So `room.xml` names a door's other targets: `<doors><door aot="3"
  target="09-40"/></doors>` sends the door armed in AOT slot 3 to `09-40`,
  and `next="0x45"` limits an entry to the door whose byte is `0x45` (it
  wins over one without). `door_aot_set` arms the target in the door record
  (`Door::target`); the door animation still reads the byte. The player is
  the player's, and the visit the scenario flag's, through the target's
  origin.
- **Saves.** The block names a room by table row and room, so a room it
  cannot name is saved in the `AKY\x05` trailer, last after the others:
  stage, room, flags (bit 0 the second visit) and a reserved zero. The block
  then names the room's origin. Retail rooms never write it.
- **The map screen** is off under `DOOR_RANDOMIZER` (bank 5, `0xC1`): the
  map button and the MAP tab do nothing.

Milestone 9's emitter copies each renumbered room's folder to its new id
with `<room origin="...">` set, and adds a `<doors>` entry for every door
that leads to a renumbered room (by AOT, with `next` where a slot is armed
more than once). Room 303's death replay draws room 311 relative to the
origin, so 311 keeps its folder and its id.

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
instruction (`nop_wide`, tag `0xEA`, the placeholder's id). The emitter finds
the marker in the decoded script and rewrites it, in place, to an `evt_exec`
of an event script holding the plan's `sce_em_set`s; an unused marker stays a
no-op. A room
needing added enemies in more than one branch has more than one marker,
named (`random_enemies(jill)`), and its `random.xml` says which spots belong
to which. This replaces staging's 25 enemy conditions, its ClearEnemies
patches and room 308's helper flag.

### Emitting and verifying

As in the proof of concept, the generator works on the pack's compiled
scripts, not the `akscr` sources, so it needs neither the sources nor the
compiler and works on a room a mod replaced. It reads the map from the base
pack (`biorand/map.xml` and each room's `random.xml`), plans the game, and
writes one new base pack per planned player (`src/biorand/`):

- **Room scripts.** The emitter decodes each `room/<id>/room.scd` and, for
  every `item_aot_set` whose pickup flag the plan changes, sets the item type
  and amount at the decoded instruction's fields. An emptied slot holds item
  0, which the engine never builds or arms. Every `aot_reset`, `aot_delete`
  and `aot_on` that re-arms such an AOT with a pickup SCE (item, document,
  map) gets the SCE of its new item. Fields are written where they are, so
  nothing else in the script moves. A slot the plan leaves `Original` is not
  touched.
- **Script changes are data only.** What a randomized game needs from the
  room scripts (documents, maps and the MO disc readers as pickups, Jill's
  missing pickups) is in the shipped `akscr` sources, gated on
  `ITEM_RANDOMIZER`, so a retail build is byte-identical and the emitter
  writes no code.
- **Manifest.** `seed`, `generator_version` (the emitter's), `plan_version`
  (`arklay_biorand::plan::GENERATOR_VERSION`), `character`, `biorand_flags`,
  and the new game's setup: `scenario_flags`, `start_items` and any slot
  count other than the original (`chris_slots`, `jill_slots`). The engine
  applies the setup after `new_game.lua`, and the slot counts on every path
  into a game. A new version of either half makes an older pack stale.
- **Entries.** `biorand/request.json` (the seed, the players and the
  configuration, so the plan can be made again), `biorand/spoiler.json` and
  `biorand/spoiler.txt`. Every other entry is the base pack's, copied as it
  is stored.

Until their milestones land, the doors and enemies stay the original's:
`generate` defers `doors/random`, the engine side turns `enemies/random` off
and lists it as deferred in the spoiler, and the manifest raises neither
`DOOR_RANDOMIZER` nor `ENEMY_RANDOMIZER`. Locks are randomized with
`locks/random` (milestone 6), which raises `LOCK_RANDOMIZER` and
`BR_LOCK_FULL_BYTE`; without it the pack keeps the original lock bytes.

`verify` reads the finished pack back. It plans the game again from the
request and the map the pack carries, then checks the manifest's flags and
setup, the spoiler, that every room script decodes, and that every planned
pickup is declared by an `item_aot_set` in its room with the planned item and
amount (a slot no reachable `item_aot_set` declares could never be picked
up), and, with random locks, every door's lock and key bytes. Given the base pack, it also checks that every other byte matches: each
room script equals the base's with the planned fields written, and every
other entry is identical. `generate` runs it on every pack it writes and
deletes a pack that fails. Later milestones add the route checks: every room
reachable, every key before its lock, nothing waiting on a story flag the
route can't raise, and every guard a patch needs reached.

### How to test milestone 1

Milestone 5 is the first playable one: a full RE1 game with shuffled items,
on the original doors and locks. With `"enemies/random": true` in the
configuration (milestone 7) the enemies are randomized too. From a repository checkout, with
the game converted to `re1.akpak` (see the README):

```sh
cargo build --profile test-release
# Either player, or both (one pack each):
target/test-release/arklay biorand generate re1.akpak --seed 123456 --player chris
target/test-release/arklay biorand generate re1.akpak --seed 123456 --player both --out-dir games
# An optional configuration (BioRand's JSON; options it leaves out keep their defaults):
echo '{"inventory/size": "6", "hard": true}' > config.json
target/test-release/arklay biorand generate re1.akpak --seed 7 --player jill --config config.json --force
# Random locks (milestone 6):
echo '{"locks/random": true}' > locks.json
target/test-release/arklay biorand generate re1.akpak --seed 7 --player both --config locks.json --force
# Check a pack again (the base is found beside it, or given with --base):
target/test-release/arklay biorand verify games/biorand-123456-chris.akpak --base re1.akpak
# Read what it set up, and the spoiler:
target/test-release/arklay pack info games/biorand-123456-chris.akpak
target/test-release/arklay extract games/biorand-123456-chris.akpak --out target/scratch/seed
cat target/scratch/seed/biorand/spoiler.txt
# Play it:
target/test-release/arklay games/biorand-123456-chris.akpak
```

The launcher's BioRand screen (Main menu, BioRand; needs `re1.akpak` converted
by this build) generates `games/biorand-<seed>.akpak` and plays it. Type
digits for the seed, or pick New Seed; pick the player on the Player row;
Options opens the configuration (every non-cosmetic option staging has, by
page and group) and Spoiler Log reads the generated game's spoiler. Changing
an option, the player or the seed and choosing Generate & Play again rebuilds
the pack ("Will rebuild" shows beside it); the same ones reuse it. The options
are kept in `biorand.json` in the config directory. A generated game saves in
its own folder, `<save_dir>/<pack id>/`.

What to look for: the start inventory and slot count match the spoiler's;
pickups hold the spoiler's items; documents, maps and the MO disc pass codes
in randomized slots are picked up as items; emptied slots can't be picked
up; keys are where the spoiler says; with random locks, the spoiler's "Locked
doors" open with their keys from either side, and Rebecca is turned away from
the doors the original refuses her; the game is completable. Report a stuck
seed with its seed, player and configuration (`biorand/request.json`).

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
   ambushes and the crash guards. *Test milestone 2.* **Done:** `random_enemies()`
   is in the original-layout rooms; the emitter (`src/biorand/enemies.rs`)
   rewrites each marker to an `evt_exec` of an appended event script, retypes
   the fixed enemies, and flips the `ENEMY_RANDOMIZER` tests of rooms the plan
   does not change; the verifier checks every planned enemy. Created enemies
   do not add their effect sheets to the room (`esp`), so some effects are
   skipped with a warning; see [`modding.md`](../modding.md#enemies-random_enemies).
8. **Room ids.** Any stage and room number; room origins; the map screen off.
   **Done:** see [Room ids and segments](#room-ids-and-segments).
9. **Door randomization**: segments as stages, renumbered rooms, and the
   door patches (106, 207, the lifts, 303/305, plant 42, the bookcases, 500).
   *Test milestone 3.*
10. **Launcher and logs.** The configuration screen as in staging, seeds, the
    spoiler log and stale-pack rebuilds. **Done:** BIORAND has a Player row,
    an Options screen and a Spoiler Log screen. Options is generated from the
    package's definition (`arklay_biorand::config::definition`, read from the
    base pack's map): a page per definition page (Ctrl+Up/Down or LB/RB turn
    it), a heading per group, and per item a toggle (switch), choice
    (dropdown, with its Default and Random choices) or slider (range, percent),
    with the item's label and, in the pane beside the list, its description.
    R (X on a pad) resets every option to its default. The options are saved on
    every change to `biorand.json` beside `arklay.toml`, as staging's flat JSON
    (a file exported from staging loads, and keys the definition doesn't know
    are kept). Generate & Play writes `games/biorand-<seed>.akpak` with them.
    A pack is rebuilt unless it records this seed, the same options and
    player (`biorand/request.json`), generator and plan version, and base
    game; the launcher says "Already generated" or "Will rebuild" before it
    starts, and a stale pack picked from PLAY is rebuilt with its own recorded
    options. The Spoiler Log screen scrolls `biorand/spoiler.txt` and writes it
    to `games/biorand-<seed>-spoiler.txt`, whose path it shows (O opens the
    folder).
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
