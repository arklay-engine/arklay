# BioRand's RE1 patches, and how arklay takes them

Every change BioRand has made to RE1 beyond the randomized data itself, with
arklay's plan for each. Sources, with full detail and file:line references:
[`research/staging-data.md`](research/staging-data.md),
[`research/exe-history.md`](research/exe-history.md), and staging's
`IntelOrca.Biohazard.BioRand/RE1/Re1RdtPatcher.cs`.

**How arklay takes them**

- **Guard.** An `akscr` edit in the base game under a runtime system-flag
  guard (`if (DOOR_RANDOMIZER)` and so on, [`plan.md`](plan.md#runtime-guards-and-system-flags)).
  The retail build folds every guard away, so `AUTHENTIC` bytes hold.
- **Fix.** A plain bug fix, applied outside `AUTHENTIC` as arklay's other
  fixes are, because vanilla benefits too.
- **Engine.** Rust or Lua behaviour, behind a BioRand flag when it is
  randomizer-only.
- **Data.** Carried in `random.xml` and applied by the emitter, because it
  depends on what the generator chose.
- **Gone.** Not needed in arklay, with the reason.

Staging patches each mansion room twice (1xx and 6xx). Arklay's rooms hold
both visits (#328), so one edit covers both. Player-specific offsets become one
edit under the room's existing `CHRIS`/`JILL` branches.

## A. Executable patches

From `biorandmod/re1.cpp`. Exe patches #1–#25 are from staging; H1–H5 are
dropped history.

| # | Patch | Arklay | Gate |
|---|---|---|---|
| 1 | No attract demo from the title | Engine | `BR_NO_TITLE_DEMO` (a generated pack has no demo rooms) **Done:** `App::next_attract_entry` returns no reel when the manifest raises `BR_NO_TITLE_DEMO`. |
| 2 | Combining fuel or ammunition no longer truncates the count to 7 bits | Fix | not `AUTHENTIC`; check whether arklay inherited the truncation **Done:** arklay inherited the truncation (the ammunition transfer masked a count with `0x7F`); `GameState::combine_count` keeps it only under `--authentic`. |
| 3 | Heal items usable at full health | Engine, already in arklay | not `AUTHENTIC` **Done:** confirmed (`accepts_ineffective_heal`). |
| 4 | Blue herb usable when not poisoned | Engine, already in arklay | not `AUTHENTIC` **Done:** confirmed (same). |
| 5–6 | Neptune can die (Cerberus's death routine) and takes weapon damage | Engine | `BR_NEPTUNE_KILLABLE`; randomizer-only **Done:** state 3 of `enemy/0b/enemy.lua` (a RET in the original, so a lethal hit froze the shark) runs the shark's own death under flag `0xC9`: death event raised, turn, thrash, sink, then parked in state 4 instead of respawning. Weapon damage already reaches Neptune through the shared pipeline (it sets the aim bit within 10000 units); the pack's hit table is not checked. |
| 7 | Yawn's bite gives normal poison | Engine, changed: Yawn still poisons. Under door rando it is always ordinary poison a blue herb cures, with no trip to room 100. Without door rando the original Yawn poison and its room 100 rescue happen only once room 100 has been visited (the visited-room bits in bank 8); before that it is ordinary poison | `BR_YAWN_POISON`, with `DOOR_RANDOMIZER`; randomizer-only **Done:** `yawn_bite_poison` in `src/game.rs`, called from `enemy/0d/enemy.lua` through `e:yawn_poison_original()`; the ordinary case calls `poison_player`. The rescue is room 20E's test of bank 1 flag `0x43`, which the ordinary bite never raises. |
| 8–9 | Extra blue herb mixes (B+B=G, B+GB=GG, …) and the icon refresh after mixing | Engine: the mix table, gated | `BR_HERB_MIXES`; randomizer-only (two mixes are BioRand inventions). Arklay has no icon bug to work around **Done:** `items::combine_with_mixes` replaces the blue herb's recipes with classic's six (B+B=G, B+G=GB, B+GR=GRB, B+GG=GGB, B+GB=GG, B+GGB=GGG). The table is Rust data (`items.rs`), not Lua. |
| 10 | The locked drawer's hard-coded ink-ribbon case | Fix, or Engine if it changes vanilla behaviour | check against arklay's drawer handler **Done:** no change needed: arklay's desk handler has no ink-ribbon special case. |
| 11–13 | Ink controlled by scenario flag 122 for both characters | Engine: ink reads flag 122 when `BR_INK_FLAG` is set, the character rule otherwise | `BR_INK_FLAG` **Done:** `ink_ribbons_needed` / `GameState::needs_ink_ribbons`: scenario flag 122 under `BR_INK_FLAG`. |
| 14–15 | Chris may use the lockpick (flag 124); Jill may use the sword key | Engine, already in arklay; confirm both directions | flag 124 **Done:** confirmed: `has_lockpick` accepts flag 124 for either character, and Jill may turn a held sword key, both outside `--authentic`. |
| 16–21 | Whole lock byte is the flag index; 256 lock flags, saved; cleared on a new game | Engine, plus a save extension block for bank 2. Done: `GameState::door_lock` decodes the byte; the `AKY\x03` save trailer holds bank 2 bytes 8..32 when any is set; a new game starts cleared | `BR_LOCK_FULL_BYTE` |
| 22 | 6 or 8 slots for either character | Engine (`config.rs` inventory size); the manifest must set it | manifest. **Open (milestone 5):** no manifest field sets a slot count yet (only `debug.inventory_size`). Rebecca's inventory is separate in memory, but the save stores one 12-slot run, so an 8-slot Chris would overwrite Rebecca's first two saved slots (as Jill's 8 slots already do); fix with the manifest field |
| 23 | Undo a Classic REbirth hook that kept the flamethrower in slots 7–8 | Gone: a Classic REbirth bug | |
| 24 | Weapon hit-scan heights for a swapped player model | Gone: cosmetic (player models) | |
| 25 | `biorand.dat` loader | Gone: the manifest carries the options | |
| H1–H4 | Old 8-slot inventory, with Rebecca sharing Chris's inventory | Gone; but check that Chris's slots 7–8 and Rebecca's don't overlap with an 8-slot Chris (an open question in staging too) | **Done:** in memory no overlap, but `save.rs` lays out one 12-slot run, so an 8-slot Chris would overwrite Rebecca's first two saved slots, as Jill already does. Rebecca does not use item boxes, so only her saved stash is affected. |
| H5 | Generic memory writer | Gone | |

## B. Room script patches (`Re1RdtPatcher`)

Gates: **R** `RANDOMIZER`, **D** `DOOR_RANDOMIZER`, **I** `ITEM_RANDOMIZER`,
**E** `ENEMY_RANDOMIZER`. "Chris" or "Jill" means the patch applies only to
that player's script path.

| Patch | Rooms | What and why | Arklay |
|---|---|---|---|
| ConfigureOptions | 106 | Sets scenario flags 122 ink, 123 hard, 124 lockpick; for Jill without the lockpick option, removes Barry giving it | Engine sets the flags from the manifest. Guard (R): Barry's gift tests flag 124 |
| FixMapsAsItems | 107, 20B | The 1F and 2F map pickups were event AOTs; make them item AOTs (20B gains AOT 4) | Guard (I) |
| FixMapsAsItems | 300, 30F, 406 | Courtyard, caves and guardhouse maps: same | Guard (I) |
| FixDocuments | 20A | The fish-tank document on the desk becomes an item AOT, per player | Guard (I) |
| DisableDogWindows | 108 | No dogs through the windows (staging always applies it) | Guard (R) |
| DisableDogBoiler | 114 | No dog from the boiler room | Guard (R) |
| DisableHunterAmbushes | 203, 209 (second visit) | No Hunter ambush events | Guard (E) |
| RemoveBlood211 | 211 | Dogs and bees crash the original during the pick-up crouch near this blood event | Guard (E); check whether arklay crashes at all, and drop it if not |
| AddDoor207 | 207 | A new door AOT 4 (lock 21) to 206 | Guard (R) |
| FixDoor104 | 104 | Door 2's arrival position | Fix (the original's arrival point is wrong) |
| FixDoor106 | 106 | The main hall's front door is usable as a normal door | Guard (D); Jill needs a new door AOT 3 |
| FixDoorToWardrobe | 112 | The wardrobe door no longer waits on the story (the proof of concept already does this) | Guard (R) |
| FixPassCodeDoor | 201 | The pass-code door gets a lock flag and arrival; Jill without door rando starts it unlocked when the code is known | Guard (R) + data (its lock flag). Staging's lock id disagrees with `rdt.json` (209 against 17); arklay names it in `random.xml` |
| AllowRoughPassageDoorUnlock | 214 | The rough-passage AOT becomes a door, unlockable from this side, sharing the pass-code door's lock | Guard (R) |
| FixDrugStoreRoom | 409 (Chris) | Removes Rebecca's drug store event blocks | Guard (R) |
| FixDoorPlant42 | 40A, 40C, 408 | The bookcase stays open; 40C's AOT becomes a door; with door rando: the nest's double doors aren't blocked, no switch to the second mansion visit, the door stays blocked until plant 42 is dead, and the reload crash after killing it (Jill) is avoided | Guard (R) for the bookcase and door; Guard (D) for the rest. Check whether arklay has the reload crash |
| FixChrisPlant42 | 40C, 40F, 408, 409 (Chris) | No switch to Rebecca; the V-JOLT hand-back; Rebecca's cutscene and drug-store presence forced | Guard (R) |
| DiableJillSandwich | 115 | Jill's sandwich-room trap off with door rando | Guard (D) |
| ShotgunOnWallFix | 109, 115, 116 | Ceiling trap with any item: don't overwrite the pickup's quantity, don't place the shotgun back, lock both sandwich-room doors, remap its lock flags 0/1/2 to 15/16 and unlock them from the hall and the living room | Redesign: `picked()` (section C) for the trigger; Guard (I) for placing it back; data for the lock flags (named in `random.xml`, renumbered by the emitter) |
| DisableSerumDoorBlock | 20D | The serum event no longer blocks the door | Guard (R) |
| DisablePoisonChallenge | 20E | No forced poison challenge | Guard (R) |
| DisableBarryEvesdrop | 405 (Jill) | Barry's eavesdropping event off | Guard (R) |
| ChangeEnemies101 | 101 (Chris) | Shifts enemy ids by 2 to dodge 601's reserved ids | Gone: the emitter gives each spawn its own id |
| ClearEnemies309, 601, 706 | 309 (Jill), 201 and 206 second visit (Chris) | Removes scripted enemy sets that the enemy randomizer replaces | Guard (E): the sets go under `if (!ENEMY_RANDOMIZER)` beside the room's `random_enemies()` |
| AllowPartnerItemBoxes | 100, 403 | Rebecca uses item boxes (`#if false` in staging) | Gone, unless the 8-slot overlap check (H1–H4) needs it |
| EnableFountainHeliportDoors | 303, 305 | Doors both ways between the fountain and the heliport, with a lock flag and arrival cut | Guard (R) (the proof of concept already has 303's half) |
| FixPlayerChangeGlitch305 | 305 | Removes two player-change opcodes | Guard (R), or Fix if vanilla shows the glitch |
| ForceHelipadTyrant | 303 | Forces the rooftop Tyrant (`set 0:43`) | Guard on `FORCE_TYRANT2` |
| ForceLift302 | 300, 301, 302 | With door rando, the lifts start in place; removes player-change opcodes and the waterfall | Guard (D) |
| FixDoor500 | 500 | Killing Tyrant 1 no longer forces you out | Guard (R) |
| FixCutscene502 | 502 (Jill) | Conditional opcodes that avoid the cutscene softlock | Guard (R) |
| AddVariableHelper308 | 308 (Jill) | Sets `FG_ROOM[0]` when Barry's conditions hold, for the enemy condition to test | Gone: `random_enemies()` sits inside Barry's own branch |
| FixBookcase216, FixBookcase406 | the rooms past 216:2 and 406:2 | With door rando, keeps the bookcase moved after passing it, by clearing flag 79 or 69 in whatever room the door leads to | Redesign: Guard (D) in 216 and 406 themselves, so the flag doesn't depend on the far room |
| FixMoDiscReader507, 509, 510; FixMoDiscDoor508 | 507–510 | MO disc readers: the item AOT takes the reader's area, and "this reader is done" moves from the MO disc pickup flags (`FG_ITEM` 82/90/98) to scenario flags 112–114, which 508's door reads | Guard (I) |
| EnableItemForJill106, 113, 11B, 211, 21C, 401, 40F | those rooms (Jill) | Pickups that only exist in Chris's game now exist for Jill | Guard (I) (Jill branch) |

## C. Item-type tests

Staging rewrites the item type a script compares the pickup against
(`typeOffsets`). Arklay replaces each test with `picked(AOT)`, which tests
the slot's own collected flag in a randomized game
([`plan.md`](plan.md#item-puzzles-collected-flags-not-item-types)).

| Room | Test | Effect | Staging |
|---|---|---|---|
| 105 | `testpickup WOOD_EMBLEM` | Dining-room emblem swap | `typeOffsets` |
| 106 | `testpickup BERETTA` (Chris) | Jill's Beretta drives the cutscene | `typeOffsets` |
| 10D | `cmpb B_PICKED_ITEM == MAGNUM`, `== WIND_CREST` | Tiger statue's eyes | `typeOffsets` |
| 10F | `testpickup GOLD_EMBLEM` | Clock wall closes behind the emblem | `typeOffsets` (two door-rando variants) |
| 112 | `testpickup INK_RIBBON` | Removes the duplicate AOT | `typeOffsets` |
| 113 | `testpickup SMALL_KEY` | Bathtub drain AOT removed | `typeOffsets` |
| 116 | `testpickup SHOTGUN` | Ceiling trap | `typeOffsets` |
| 219 | `testpickup BATTERY` | Removes an AOT | `typeOffsets` (from the 719 file) |
| 40A | `testpickup V-JOLT book (69)` | Event 06 | `typeOffsets` |
| 106 | `testpickup HELMET_KEY` | Event 0C | **missed** |
| 308 | `testpickup FLAMETHROWER` | Lock 40 and event 00 | **missed** |
| 30B | `testpickup FLAMETHROWER` | Lock 39 and item flag 172 | **missed** |
| 30D | `testpickup FLAMETHROWER` ×2 | | **missed** |
| 50A | `testpickup GREEN_HERB (68)` | Event 09 | **missed** |

`testitem` tests the item being *used* (keys, cranks, jewels), which keeps
its identity, so the 99 of them need nothing. The 116 shotgun's
`testitem SHOTGUN` (putting the broken shotgun back) is one of these.

## D. Enemy conditions

Staging gates the enemies it adds with 25 room conditions in `rdt.json`
(listed in
[`research/staging-data.md` §2.3](research/staging-data.md#23-enemy-condition-25-entries-wrap-created-sce_em_sets)),
for example "not during Jill's prologue" in 106. Arklay drops them: each room's
`akscr` gets a `random_enemies()` placeholder inside the branch where its own
enemies are set ([`plan.md`](plan.md#enemy-placeholders)), and the 25
conditions are the guide to where each placeholder goes. Fixed enemies retyped
in place (104, 10E, 208, 20C, 210, 21C, 30A, 30B) stay data in `random.xml`.

## E. Disabled cutscenes (`cutscenes/disable`)

Staging clears 13 (Chris) or 18 (Jill) story flags in 106, removes event
blocks in 201, 206, 20A (Chris), 20C, 40C, 512, 513 and 514, retargets 513's
flag tests to the self-destruct flag, and adds Tyrant 1 to 513 when the
self-destruct starts. All of these become guards on `CUTSCENES_DISABLED`,
with the flag clears in the engine's new-game set-up. This is the last
milestone.

## F. Door and lock data

- Every listed door is rewritten even with locks and doors not randomized. In
  staging that unlocks 207:1 (the armor key door: a `rdt.json` gap, which
  looks like a bug), unlocks 10F:0 from inside and 50B:1, and locks 201:1 and
  305:0 from one side. Arklay writes only what the plan changes, and
  `random.xml` records each intended difference.
- Entrance overrides (106:3, 214, 303:8, 305:0) and the doors that differ by
  player or door rando are data in `random.xml`.
- Doors leading nowhere get lock 255, which is never raised.

## G. History

[`research/exe-history.md` §B–C](research/exe-history.md#b-historical-script-and-data-patches-re1)
lists 61 historical script and data patches (41 kept in staging, 13 replaced,
7 dropped) and the bug reports behind them. The dropped ones matter only as
hazards to re-check:

- 203 and 212 Barry and cutscene softlocks under door rando;
- the 302 battery lift corrupting the inventory;
- master's lock ids colliding modulo 64.

Each becomes a static check in the verifier where it can.
