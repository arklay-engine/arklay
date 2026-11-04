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
| 22 | 6 or 8 slots for either character | Engine (`config.rs` inventory size); the manifest must set it | manifest **Done:** a generated pack's manifest sets `chris_slots`/`jill_slots` when they differ from six and eight; the engine applies them on every path into a game (`apply_slot_counts`), so a load needs nothing saved. The start inventory (`start_items`) and scenario flags 122–124 (`scenario_flags`) come from the manifest the same way, after `new_game.lua`. |
| 23 | Undo a Classic REbirth hook that kept the flamethrower in slots 7–8 | Gone: a Classic REbirth bug | |
| 24 | Weapon hit-scan heights for a swapped player model | Gone: cosmetic (player models) | |
| 25 | `biorand.dat` loader | Gone: the manifest carries the options | |
| H1–H4 | Old 8-slot inventory, with Rebecca sharing Chris's inventory | Gone; but check that Chris's slots 7–8 and Rebecca's don't overlap with an 8-slot Chris (an open question in staging too) | **Done:** in memory no overlap. The save lays out one 12-slot run, so a player holding more than six slots covers Rebecca's first saved slots; when any of those is in use, the save keeps all of Rebecca's slots in an `AKY\x04` trailer and a load restores them from it. A six-slot Chris and a vanilla Jill write no trailer, so their saves stay byte-identical. |
| H5 | Generic memory writer | Gone | |

## B. Room script patches (`Re1RdtPatcher`)

Gates: **R** `RANDOMIZER`, **D** `DOOR_RANDOMIZER`, **I** `ITEM_RANDOMIZER`,
**E** `ENEMY_RANDOMIZER`, **L** `LOCK_RANDOMIZER`, **F** `BR_LOCK_FULL_BYTE`
(raised with **L**). "Chris" or "Jill" means the patch applies only to
that player's script path.

| Patch | Rooms | What and why | Arklay |
|---|---|---|---|
| ConfigureOptions | 106 | Sets scenario flags 122 ink, 123 hard, 124 lockpick; for Jill without the lockpick option, removes Barry giving it | Engine sets the flags from the manifest. Guard (R): Barry's gift tests flag 124 |
| FixMapsAsItems | 107, 20B | The 1F and 2F map pickups were event AOTs; make them item AOTs (20B gains AOT 4) | Guard (I) **Done:** 107's item AOT 2 takes event AOT 5's area and is armed with the map SCE; 20B gains item AOT 4 (flag 17, item 0x4F, which `random.xml` now names; staging's says 0) in place of the lighter-gated event AOT 2. 20B's map has no item model, as in staging |
| FixMapsAsItems | 300, 30F, 406 | Courtyard, caves and guardhouse maps: same | Guard (I) **Done:** 300 skips the `aot_reset` that made AOT 4 an event; 30F's AOT 4 and 406's AOT 8 take the event AOT's area. 406 also turns AOT 8 into a message in Rebecca's branch, so she can't take the slot |
| FixDocuments | 20A | The fish-tank document on the desk becomes an item AOT, per player | Guard (I) **Done:** the five desk `item_aot_set`s of AOT 8 (flag 16) take the desk's area and the desk message is not set. Jill's third branch, where Barry hands the document over, is unchanged |
| DisableDogWindows | 108 | No dogs through the windows (staging always applies it) | Guard (R) |
| DisableDogBoiler | 114 | No dog from the boiler room | Guard (R) |
| DisableHunterAmbushes | 203, 209 (second visit) | No Hunter ambush events | Guard (E) **Done:** staging's 603 and 609 are rooms 103 and 109 on the second visit. Their ambush hunters (flags `F_ENEMY_3A`, `F_ENEMY_76`) are under `if (!ENEMY_RANDOMIZER)`, with a `random_enemies()` beside each |
| RemoveBlood211 | 211 | Dogs and bees crash the original during the pick-up crouch near this blood event | Guard (E) **Not done:** the crash is the original executable's, in the pick-up crouch; arklay's animation is its own and no crash path is known. Room 211 has a `random_enemies()` at the top of its init, and the blood event is unchanged. Watch for a report |
| AddDoor207 | 207 | A new door AOT 4 (lock 21) to 206 | Guard (L) **Done:** under `LOCK_RANDOMIZER` 207's message AOT 4 becomes staging's door to 206 (`LOCKED`, lock 0x15); the emitter writes the plan's bytes into it. Item-only games keep the message **Done (doors):** the guard is now `BR_LOCK_FULL_BYTE`, which random doors raise too: 207:4 is a door of the map, so door rando needs it |
| FixDoor104 | 104 | Door 2's arrival position | Fix (the original's arrival point is wrong) |
| FixDoor106 | 106 | The main hall's front door is usable as a normal door | Guard (D); Jill needs a new door AOT 3 **Done:** under `DOOR_RANDOMIZER` AOT 3 is one ordinary door for either player, naming 106; the emitter writes its target and arrival |
| FixDoorToWardrobe | 112 | The wardrobe door no longer waits on the story (the proof of concept already does this) | Guard (R) |
| FixPassCodeDoor | 201 | The pass-code door gets a lock flag and arrival; Jill without door rando starts it unlocked when the code is known | Guard (R) + data (its lock flag). Staging's lock id disagrees with `rdt.json` (209 against 17); arklay names it in `random.xml` **Done:** Guard (L): no pass-code panel (AOT 5), and the second visit keeps door 1 for either player (no `aot_delete`); its bytes are the plan's. Staging's Jill-only unlock when the code is known (no door rando) is not ported: the plan opens the pair from 214. The first visit's door stays non-interactive (SAT 0), as 214 exists only on the second. Staging's nop offsets for the second visit fall mid-instruction in the retail script; the intent is ported, not the offsets |
| AllowRoughPassageDoorUnlock | 214 | The rough-passage AOT becomes a door, unlockable from this side, sharing the pass-code door's lock | Guard (L) **Done:** Chris's use-item AOT 1 and Jill's AOT 5 become doors to 201 (`LOCK`, lock 0x11); Jill's original door AOT 0 and the `aot_delete`s that pick between it and AOT 5 are skipped |
| FixDrugStoreRoom | 409 (Chris) | Removes Rebecca's drug store event blocks | Guard (R) |
| FixDoorPlant42 | 40A, 40C, 408 | The bookcase stays open; 40C's AOT becomes a door; with door rando: the nest's double doors aren't blocked, no switch to the second mansion visit, the door stays blocked until plant 42 is dead, and the reload crash after killing it (Jill) is avoided | Guard (R) for the bookcase and door; Guard (D) for the rest. Check whether arklay has the reload crash **Done:** 40A keeps `F_SCENARIO_25` (the bookcase) under `RANDOMIZER`; 40C's message AOT 1 is a door to 40A under `RANDOMIZER`. Under `DOOR_RANDOMIZER`: 408's AOT 3 is the door to 40C before the first visit; 40C does not raise `F_SCENARIO_00` (the second mansion visit), its door 0 is off while plant 42 lives and on once it is dead (both players), and Jill's dead-plant set-up (the body, the music, Barry's scene) is skipped, which is staging's crash fix. Whether arklay has the reload crash is not checked; the guard follows staging |
| FixChrisPlant42 | 40C, 40F, 408, 409 (Chris) | No switch to Rebecca; the V-JOLT hand-back; Rebecca's cutscene and drug-store presence forced | Guard (R) |
| DiableJillSandwich | 115 | Jill's sandwich-room trap off with door rando | Guard (D) **Done:** Jill's first ceiling-trap branch (`F_COMMON_07` and `5F` clear, with Barry's rescue) is skipped; the second trap and the unlocked case stay |
| ShotgunOnWallFix | 109, 115, 116 | Ceiling trap with any item: don't overwrite the pickup's quantity, don't place the shotgun back, lock both sandwich-room doors, remap its lock flags 0/1/2 to 15/16 and unlock them from the hall and the living room | Redesign: `picked()` (section C) for the trigger; Guard (I) for placing it back; data for the lock flags (named in `random.xml`, renumbered by the emitter) **Partly done:** 116 keeps the slot's own amount, sets no put-back AOT 6 and ignores the shotgun at the wall under `ITEM_RANDOMIZER`. **Done (lock milestone):** under `BR_LOCK_FULL_BYTE` 115 mirrors `F_LOCK_00`/`01` onto flags 0x0F/0x10 (the map's locks for its doors, which the emitter writes) at the end of `init` and where event 1 opens door 0, and 109 and 116 raise 0x0F and 0x10 in `init`. `F_LOCK_02` is on no door, so staging's 2 → 16 is not ported, nor its Jill nop at 0x2342 (the trap's locks stay the original's) |
| DisableSerumDoorBlock | 20D | The serum event no longer blocks the door | Guard (R) |
| DisablePoisonChallenge | 20E | No forced poison challenge | Guard (R) |
| DisableBarryEvesdrop | 405 (Jill) | Barry's eavesdropping event off | Guard (R) |
| ChangeEnemies101 | 101 (Chris) | Shifts enemy ids by 2 to dodge 601's reserved ids | Gone: the emitter gives each spawn its own id |
| ClearEnemies309, 601, 706 | 309 (Jill), 201 and 206 second visit (Chris) | Removes scripted enemy sets that the enemy randomizer replaces | Guard (E) **Done:** the sets go under `if (!ENEMY_RANDOMIZER)` beside the room's `random_enemies()` |
| AllowPartnerItemBoxes | 100, 403 | Rebecca uses item boxes (`#if false` in staging) | Gone: the 8-slot overlap (H1–H4) is solved in the save |
| EnableFountainHeliportDoors | 303, 305 | Doors both ways between the fountain and the heliport, with a lock flag and arrival cut | Guard (R) (the proof of concept already has 303's half) **Done (no door-rando part):** 303:8 is `door-rando="false"` and 303:0 `randomize="false"`, so the heliport is outside the door layout and 305:0 is sealed or rewired by the plan like any door; the `R` half stays as it is |
| FixPlayerChangeGlitch305 | 305 | Removes two player-change opcodes | Guard (R), or Fix if vanilla shows the glitch |
| ForceHelipadTyrant | 303 | Forces the rooftop Tyrant (`set 0:43`) | Guard on `FORCE_TYRANT2` |
| ForceLift302 | 300, 301, 302 | With door rando, the lifts start in place; removes player-change opcodes and the waterfall | Guard (D) **Done:** 300, 301 and 302 set the lift flags (`F_SCENARIO_30`/`31`) at the start of `init`; 302 skips the waterfall (`F_SCENARIO_2F`: its message, object, effect and sound pans, one guard per block for the jump range) and the player-model changes on the lifts and the ladder |
| FixDoor500 | 500 | Killing Tyrant 1 no longer forces you out | Guard (R) **Done** as Guard (D), not (R): with door rando the lift stays a door after Tyrant 1 (both players), since it may be the only way out; item-only games keep the original's message |
| FixCutscene502 | 502 (Jill) | Conditional opcodes that avoid the cutscene softlock | Guard (R) |
| AddVariableHelper308 | 308 (Jill) | Sets `FG_ROOM[0]` when Barry's conditions hold, for the enemy condition to test | Gone **Done:** `random_enemies()` sits inside Barry's own branch |
| FixBookcase216, FixBookcase406 | the rooms past 216:2 and 406:2 | With door rando, keeps the bookcase moved after passing it, by clearing flag 79 or 69 in whatever room the door leads to | Redesign: Guard (D) in 216 and 406 themselves, so the flag doesn't depend on the far room **Done:** under `DOOR_RANDOMIZER` 216 and 406 raise their bookcase flag (`F_SCENARIO_4F`, `F_SCENARIO_45`) at the start of `init`, so the bookcase starts moved and a return through 216:2 or 406:2 never lands behind it. That also skips the push; staging's set in the far room depends on which room that is |
| FixMoDiscReader507, 509, 510; FixMoDiscDoor508 | 507–510 | MO disc readers: the item AOT takes the reader's area, and "this reader is done" moves from the MO disc pickup flags (`FG_ITEM` 82/90/98) to scenario flags 112–114, which 508's door reads | Guard (I) **Done** |
| EnableItemForJill106, 113, 11B, 211, 21C, 401, 40F | those rooms (Jill) | Pickups that only exist in Chris's game now exist for Jill | Guard (I) (Jill branch) **Done.** 21C's Jill variant has no item model, so its pickup is invisible; 401 also drops "There's a key in the cup." under the guard |

## C. Item-type tests

Staging rewrites the item type a script compares the pickup against
(`typeOffsets`). Arklay replaces each test with `picked(AOT)`, which in a randomized game
remaps a pickup of exactly that AOT (the engine's `B_PICKED_AOT` byte) to the
original item type, so the room's own test runs unchanged
([`plan.md`](plan.md#item-puzzles-collected-flags-not-item-types)).

| Room | Test | Effect | Staging | Arklay |
|---|---|---|---|---|
| 105 | `testpickup WOOD_EMBLEM` | Dining-room emblem swap | `typeOffsets` | Done: `picked(AOT_04_ITEM)` and `picked(AOT_07_ITEM)` (one per player) |
| 106 | `testpickup BERETTA` (Chris) | Jill's Beretta drives the cutscene | `typeOffsets` | Done: `picked(AOT_06_ITEM, testpickup(HANDGUN))`; AOT 6 is reused, the item picks flag `F_ITEM_1A` |
| 10D | `cmpb B_PICKED_ITEM == MAGNUM`, `== WIND_CREST` | Tiger statue's eyes | `typeOffsets` | Done: two `picked_as` above the long `if` (too far from it for `picked`) |
| 10F | `testpickup GOLD_EMBLEM` | Clock wall closes behind the emblem | `typeOffsets` (two door-rando variants) | Done: `picked(AOT_04_ITEM)`, both variants |
| 112 | `testpickup INK_RIBBON` | Removes the duplicate AOT | `typeOffsets` | Done: `picked(AOT_04_ITEM)` |
| 113 | `testpickup SMALL_KEY` | Bathtub drain AOT removed | `typeOffsets` | Done: `picked(AOT_01_ITEM)` |
| 116 | `testpickup SHOTGUN` | Ceiling trap | `typeOffsets` | Done: `picked(AOT_01_ITEM)`; the shotgun put-back stays Guard (I) |
| 219 | `testpickup BATTERY` | Removes an AOT | `typeOffsets` (from the 719 file) | Done: `picked(AOT_03_ITEM)` |
| 40A | `testpickup V-JOLT book (69)` | Event 06 | `typeOffsets` | Done: `picked(AOT_03_ITEM)` |
| 106 | `testpickup HELMET_KEY` | Event 0C | **missed** | Not converted: the test (`IT_54`) sits after a `return;` in `proc main` and never runs |
| 308 | `testpickup FLAMETHROWER` | Lock 40 and event 00 | **missed** | Done: `picked(AOT_03_ITEM)` |
| 30B | `testpickup FLAMETHROWER` | Lock 39 and item flag 172 | **missed** | Done: `picked(AOT_03_ITEM)` |
| 30D | `testpickup FLAMETHROWER` ×2 | | **missed** | Done: both, `picked(AOT_03_ITEM)` |
| 50A | `testpickup GREEN_HERB (68)` | Event 09 | **missed** | Done: `picked(AOT_01_ITEM)` (item IT_68, a file) |

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

**Done (milestone 11).** Staging's `DisableCutscenes` (`Re1CrModBuilder.cs`
276-446; the table is
[`research/staging-data.md` §3.8](research/staging-data.md#38-disablecutscenes-276-446-only-with-cutscenesdisable))
becomes guards on `CUTSCENES_DISABLED`, which the package raises with
`cutscenes/disable`. The retail build of every edited room is byte-identical
(`scripts/check-room-retail.sh`).

The rooms are staging's, by the mansion's second-visit ids: 60A is 10A on
the second visit, 601 is 101, 706 is 206, and 70C is 20C. (The first draft of
this section listed 201 and 20A; staging does not touch them. The Jill flag
list says one of its flags "also disables 115 sandwich rescue and 20A
cutscene".)

| Room | Player | Guard |
|---|---|---|
| 106 | both | New game: the first visit (F_COMMON_00 still clear) raises the flags below. 13 for Chris: bank 0 `101, 127, 192`, bank 1 `0, 2, 3, 36, 69, 72, 100, 167, 170, 171`. 18 for Jill: bank 0 `101, 127`, bank 1 `0, 2, 3, 5, 7, 69, 72, 86, 97, 103, 161, 170, 172, 173, 175, 192`. Staging's "clear" is the `set` opcode's value 0, which raises the bit. |
| 10A | Chris, second visit | The hunter and Rebecca scream: no `set F_COMMON_21` in the cutscene event. |
| 101 | Chris, second visit | Rebecca and the hunters of "Rebecca in trouble": every block in `init` after the music reset, and all of `main`. |
| 206 | Chris, second visit | The same, for the Rebecca and hunter scenes: the whole branch but the placeholders, and all of `main`. |
| 20C | Jill, second visit | Barry in the Yawn 2 room: AOT 3 always runs `on_aot_03_2` (staging patches event 0x2C in and drops two flag tests). |
| 40C | Jill | Barry at plant 42: the two raises of F_ROOM_0F are skipped. |
| 512 | both | The cell scene (Jill or Chris in the cell): the block under F_COMMON_4B is skipped. |
| 514 | both | Wesker's, Rebecca's and Barry's scenes: all of `init` after the doors. |
| 513 | both | The Wesker and Tyrant scene (below). |

**513.** In `init`, F_ROOM_07 ("self-destruct started") is raised at the end of
the door set-up, so the room always sets up as on a return visit: no Wesker
spawn and none of the scene's events. `main` sets F_ROOM_07 from
F_SCENARIO_37 every frame while the room runs, which is staging's retargeting
of the two flag tests to the self-destruct flag, and the Tyrant fight
therefore still starts the self-destruct. Staging retargets Jill's Wesker-route
test only; the Barry-route block is covered too, since otherwise the
self-destruct could not start. The Tyrant 1 (`F_ENEMY_70`) is spawned at the
top of `init` (staging adds it there, and nothing releases it otherwise); it
is not spawned once dead, as the flag keeps it from spawning (checked by
booting a save with the flag raised). In the first frame before the
self-destruct, the boss music starts, as staging's frame opcodes do (F_ROOM_0B
as the once flag).

**Why the 106 flags are a script block and not the engine's new-game
set-up.** Staging's flags are bank 0 and bank 1; the manifest's
`scenario_flags` holds bank 0 only, and the verifier reads that field. The
guarded block is also staging's mechanism exactly (a prepended `ConfigureOptions`-style
block under "the first-cutscene flag is clear"), it runs in whichever room
copy is the start, for either player, and needs no engine, manifest or
verifier change. It runs after the enemy placeholder, which therefore still
reads the unraised flags.

Command-`if`s test a system flag alone (`if (CUTSCENES_DISABLED)`), so a
guard on an existing condition is a nested `if (!CUTSCENES_DISABLED)`; each
block stays under the one-byte jump limit.

## F. Door and lock data

- Every listed door is rewritten even with locks and doors not randomized. In
  staging that unlocks 207:1 (the armor key door: a `rdt.json` gap, which
  looks like a bug), unlocks 10F:0 from inside and 50B:1, and locks 201:1 and
  305:0 from one side. Arklay writes only what the plan changes, and
  `random.xml` records each intended difference.
- Entrance overrides (106:3, 214, 303:8, 305:0) and the doors that differ by
  player or door rando are data in `random.xml`.
- Doors leading nowhere get lock 255, which is never raised. **Done**, and
  arklay also writes key 255 (staging leaves such a door's key 0, which the
  walk showed can leave it open).
- **Done (milestone 6):** under `BR_LOCK_FULL_BYTE` the emitter writes every
  `door_aot_set`: the plan's `(lock, key)` for a door it has, matched by
  room, AOT id and target room (106 AOT 3, 10F:3, 20E:3, 303:0, 40C:1 and
  40F:0 don't match the map and keep their original flag); `byte & 0x3F`
  when the original locks it, else 0, for the rest. The `0x40` Rebecca
  refusal becomes a Guard (F) message AOT in its room (27 doors in 101, 103,
  104, 106, 109, 10A, 10B, 201, 203, 204, 207, 303 and 408).

## G. History

[`research/exe-history.md` §B–C](research/exe-history.md#b-historical-script-and-data-patches-re1)
lists 61 historical script and data patches (41 kept in staging, 13 replaced,
7 dropped) and the bug reports behind them. The dropped ones matter only as
hazards to re-check:

- 203 and 212 Barry and cutscene softlocks under door rando;
- the 302 battery lift corrupting the inventory;
- master's lock ids colliding modulo 64.

Each becomes a static check in the verifier where it can.
