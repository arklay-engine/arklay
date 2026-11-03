# BioRand RE1: executable patches and patch history

Read-only research for porting BioRand's RE1 gameplay patches into arklay.
This file covers three things:

- A. Every patch BioRand makes to the RE1 executable (`Bio.exe`, PC 1996) or
  to Classic REbirth (`ddraw.dll`).
- B. Every RE1 script/data patch BioRand ever made, across branches, and its
  status in `staging`.
- C. RE1 bug-fix commits whose fix is a game patch.

Scope is RE1 gameplay only. Cosmetic randomizers are named in one line at the
end.

## Sources

| Abbrev | Path | Revision |
|---|---|---|
| `BR/` | `~/src/biorand-classic/IntelOrca.Biohazard.BioRand/` | `staging` HEAD `9f862154` |
| `MOD/` | `~/src/biorand-classic/biorandmod/` (C++ `biorand.dll`) | `staging` |
| `master:` | same repo, branch `master` (`79557657`), the old 2023/24 "BaseRandomiser" pipeline | |
| arklay | `~/repos/arklay/src/game.rs` | used only to confirm vanilla door semantics |

`staging` is 308 commits ahead of `master` and 70 behind; they split at
`be0c7fd8` (2024-09-01). The branches `redesign`, `redesign-doors`,
`redesign-refactor`, `re1-support-2`, `re1-enemies`, `door-rando` and
`chris-island` have no commits that `staging` lacks, so their history is
already in the `staging` sweep. Branches with unmerged commits that matter:
`forcetyrant` (1), `re1-main-door` (1), `re1-support` (55; a pre-rebase copy of
the RE1 bring-up that was merged as `re1-support-2`, with no unique patches),
`store-all-items` (15; JSON item lists, no new patches), `new-routing-design`
(16; routing only), `new-item-randomizer` (1; WIP), `re1-sherry` (1; model
assets).

## How patches reach the game (three eras)

1. **2023 to 2024 (master, `BaseRandomiser`).** C# `PatchWriter`
   (`master:BR/PatchWriter.cs`) writes records of `{u32 address, u32 length,
   bytes}` to a `MemoryStream ExePatch`. `BaseRandomiser` saves it as
   `mod_biorand/biorand.dat` (`master:BR/BaseRandomiser.cs:934-935`). The old
   `biorand.dll` (`ModMain` in `MOD/biorand.cpp:304-328`, now commented out)
   reads the file at `DllMain` and then every 5 s on a thread, and applies each
   record with `WriteProcessMemory` at the **absolute** address. Classic
   REbirth addresses were written as absolute `0x10xxxxxx`, so they assumed
   `ddraw.dll` loads at its preferred base.
2. **2025-02 to 2025-03 (staging, new controller).** The same `PatchWriter`
   code moved into `Re1ClassicRandomizerController.WritePatchFile`
   (`5d001af7`, `9f754b17`).
3. **Since `c153eeb4` (2025-03-14, "Fix biorand.dll for RE 1 when in 7z").**
   The patches are compiled into the DLL: `MOD/re1.cpp` `RE1::Apply()`
   (`MOD/re1.cpp:92-107`) runs from the Classic REbirth export
   `Modsdk_post_init` (`MOD/biorand.cpp:339-356`). It detects RE1 by
   `byte[0x4C5E13] == 0x66`. Every patch in `Apply()` is **unconditional**.
   Only two values come from the seed, through a "dynamic tweaks" file
   (`MOD/re1.cpp:328-377`). The DLL reads `d:\horr\jpn\biorand.dat` through
   the game's own loader at `0x43E970` (so Classic REbirth's mod file system
   resolves it), and parses records of `{u32 kind, u32 length, bytes}`:
   - kind 1 `TWEAK_HITSCAN`: 24 bytes copied to `0x4AAD98`
     (`BR/RE1/Re1CrModBuilder.cs:936-965`).
   - kind 2 `TWEAK_INVENTORY_SIZE`: 1 byte, 6 or 8
     (`BR/RE1/Re1CrModBuilder.cs:1147-1151`).

   `DynamicTweaksBuilder` is at `BR/RE1/Re1CrModBuilder.cs:1320-1342`.
   Classic REbirth addresses go through `MemoryManager::CalculateCrAddress`
   (`MOD/mm.cpp`): `ddraw.dll` base + (addr - `0x10000000`).

Staging still contains the legacy `BR/RE1/Re1Randomiser.cs` (patch writers at
lines 805-990), which writes `{address, length, bytes}` records. The current
DLL reads that file as `{kind, length, bytes}` and acts only on kinds 1 and 2,
so the legacy pipeline's exe patches are dead in a staging build.

Options that change gameplay without an exe patch are written as **scenario
flags** by an `if (1:0) { set ... }` block appended to room 106's init script
(`BR/RE1/Re1RdtPatcher.cs:61-82`). Opcode `0x05 set` mode 0 raises a flag and
mode 1 clears it (arklay `GameState::apply_flag`, `game.rs:3717`):

| Scenario flag | Set when | Meaning after the DLL patches |
|---|---|---|
| `0:122` | `ink` (option `ink/enable`, default Chris on, Jill off) | ink ribbons needed to save, ink items allowed, ink model shown |
| `0:123` | `hard` (option `hard`, added `ad8d2d05`) | vanilla "new game+" flag: enemies are harder to kill. Vanilla also uses it to force ink for Jill; the DLL moves that to 122 |
| `0:124` | `lockpick` (option `inventory/special/lockpick`, default Chris Never, Jill Always) | vanilla "has lockpick" flag (arklay `SCENARIO_FLAG_HAS_LOCKPICK`). Jill: room 106 `Nop(0x31B02)` removes Barry handing it over when the option is off |

The option definitions are at
`BR/RE1/Re1ClassicRandomizerController.cs:31,40,117,126`, the defaults at
`:543-551`, and the hand-off to the mod as `General` values at `:606-614`.

## Door lock byte, before and after

The door record (`door_aot_set`, opcode `0x0C`) has a `LockId` byte, a
`LockType` byte and a `Free` byte (`BU/Script/Opcodes/DoorAotSeOpcode.cs:55-85`).

**Vanilla** (arklay `try_door`, `game.rs:7655-7723`; `Door::lock` doc,
`game.rs:1120-1122`):

- `LockId` bit `0x80`: the door is lock-checked. If clear, the door opens.
- `LockId` bit `0x40`: "one character". It refuses entry when
  `player_flag & 3 == 3` (Rebecca), and is inert for Chris and Jill.
- `LockId & 0x3F`: the lock flag index in flag bank 2 ("LocksFlags": 64 bits,
  save bytes 84..91 in arklay's layout). Once the flag is raised the door
  opens.
- `LockType`: the key item id, or `0xFE` (254) or `0xFF` (255).
  - 254: the door unlocks from this side. It shows the "unlocked from the
    other side" message and raises the flag.
  - 255: locked, cannot be opened from here. A script must raise the flag
    (pass-code door, scripted locks).
  - For key type 51 (sword key), Jill substitutes the lockpick when scenario
    flag 124 is set. Chris cannot, and Jill is refused the sword key itself.
- `Free` = 129 (`0x81`) in every door BioRand creates. This is arklay's
  `sub_type` probe byte: `0x80` needs the action button, low bit 1 is a probe
  mask.
- The cap of 64 flags is why `07ace78e` ("Reduce lock limit to 64 as allowed
  by game", 2025-03-17) limited the lock randomizer to `0..62 | 0x80`.

**BioRand after `afd56f00`** ("Increase lock limit to 254", 2025-04-02;
`MOD/re1.cpp:270-290`):

- The `0x40` test is neutralised and the `0x80` test becomes "any non-zero
  byte". The `& 0x3F` masks on the check path and the set path are NOPed, so
  **the whole byte is the flag index**: 0 means unlocked, 1..255 are lock flags.
- Bank-2 lock storage moves to `_customData.flag_locks`, a `uint32_t[8]`
  (256 bits, 32 bytes). Nine code sites that held the bank-2 base pointer are
  rewritten.
- The new storage is saved through the Classic REbirth hooks `Modsdk_save` and
  `Modsdk_load` (`MOD/biorand.cpp:358-372`) as a 44-byte blob:
  `"BIORAND\0"`, `u32 version = 1`, then the 32 flag bytes
  (`MOD/re1.cpp:63-76`, `:109-141`).
- On load, a magic or version mismatch shows a message box ("Save data is
  incompatible with BioRand") and starts with empty flags. A `jmp` hook at
  `0x48DB4A` (`PostGameInit`) zeroes the flags on new game.
- Randomized lock ids are `1..254` (`BR/LockRandomizer.cs:282`,
  `BR/DoorRandomizer.cs:29`). Id 255 is used as "flag never set" for doors
  that lead nowhere (`BR/DoorRandomizer.cs:149`).
- `LockType` values written by `LockRandomizer.SetDoorLocks`
  (`BR/LockRandomizer.cs:345-372`):
  - `0` "UNLOCKED": the door is `noUnlock`, or has a lock id but no key. The
    exe behaviour for a non-zero id with type 0 was not verified.
  - `255` LOCKED: the door has no target or its kind is `locked`.
  - `254` UNLOCK: the door kind is `unlock`.
  - otherwise the key item id.
- History of vanilla lock ids under the new format:
  - `afd56f00` OR'ed `0x80` into every vanilla `lockId` so the old flags
    stayed distinct ("All original locks had 0x80 (some had 0x40 but we can
    ignore those)").
  - Script `set 2:n` targets were then shifted by +128: `FixCrestDoor` 11A
    (`fa3e1037`), sandwich room 115 (`77ccf4c9`).
  - `9e494022` ("Fix lock ids", 2025-07-04) dropped the OR. Vanilla ids are
    now their low-6-bit value, and the script remaps changed with them (see
    B).
- **Pass-code door.** `master:Re1DoorHelper` used `209` = `0xD1` = `0x80 |
  0x40 | 17`: vanilla flag 17, so `set 2:(209-192)`. Staging's
  `Re1RdtPatcher` uses `PassCodeDoorLockId = 128 | 81` = 209 as a **full flag
  index**, with `set 2:209` (`BR/RE1/Re1RdtPatcher.cs:16`, `:296-329`,
  `:424-441`). But `meta/re1/rdt.json` gives the 214 to 201 doors
  `lockId: 17`, and `ApplyDoors` overwrites `LockId` for every door listed in
  the mod (`BR/RE1/Re1CrModBuilder.cs:478-507`). Worth checking which value
  ends up in the room.

**master-era door rando (2022 to 2024).** Lock ids came from
`Enumerable.Range(0,125).Select(x => x+128)`, so 128..252
(`master:BR/DoorRandomiser.cs:120-125`), on an unpatched exe. Ids at or above
192 also carry the `0x40` bit, and ids alias modulo 64 on the flag index. This
is a latent collision source in the old RE1 door rando (inferred, not
reported in any commit).

## A. Executable and Classic REbirth patches

Staging lines are from `MOD/re1.cpp`. "Since" is the commit that first added
the patch. "Gate" is the config that enables it; "always" means
unconditional.

| # | Address(es) | Original code / data | Patch | Gate | Gameplay meaning | Since / file:line |
|---|---|---|---|---|---|---|
| 1 | `0x48E031` (7 bytes) | title-screen idle path that starts the attract demo | 7x `NOP` | always | no title demo (it would load demo rooms under the mod) | `a70480c5` 2023-09-15; `re1.cpp:157-161` |
| 2 | `0x4483BD`, `0x44842D` (4 bytes each) | `and bx, 0x7F` on the quantity during an item combine | 4x `NOP` each | always | combining flamethrower fuel or ammo no longer truncates the count to 7 bits | `249ae907` 2023-08-22; `re1.cpp:163-172` |
| 3 | `0x447A39` (2) | `jge 0x447AA2`: refuse a heal item when HP >= max | `NOP NOP` | always | heal items can be used at full HP. Needed when randomized healing is forced on the player, and to empty slots | `a660ad5f` 2023-08-22; `re1.cpp:174-186` |
| 4 | `0x447AAD`, `0x447AE7` (2 each) | `je`: refuse the blue herb (and its mixes) when not poisoned | `NOP NOP` each | always | blue herb is always usable. `0x447AE7` came with the DLL move (`c153eeb4`); `0x447AAD` was added in `2751dd27` (2025-03-16, "Fix using blue herb when not needed") | `re1.cpp:181-185` |
| 5 | `0x4AA0EC` (u32) | Neptune's entry in the enemy death-routine pointer table: none (Neptune cannot die) | `0x004596D0` (Cerberus's death routine) | always | Neptune can be killed when enemy rando puts it in a normal room | `6af9334b` 2023-08-25; `re1.cpp:188-209` |
| 6 | `0x4AF908`, `0x4B0268` + `11*120 + 6 + w*12`, w = 0..9 (u16) | Neptune row of the two weapon-damage tables (12-byte entries, damage at +6): 0 | damage `{16,14,32,40,130,20,100,200,100,900}` for weapons 0..9 | always | Neptune takes damage. Two tables, presumably normal and hard/NG+ | same as #5 |
| 7 | `0x45B8C0+6` (1) | `80 0D 90 52 C3 00 20` = `or byte [0xC35290], 0x20` (status `ST_POISON_YAWN`) on a Yawn bite | immediate `0x02` (`ST_POISON`) | always since `c153eeb4`; on master only when `RandomDoors` | Yawn poison becomes normal poison, curable by blue herb instead of serum (#566). Needed once Yawn can appear anywhere | `4f27c071` 2023-09-16; `re1.cpp:211-217` |
| 8 | `0x4CCC08[28]` (pointer) | mix-table pointer array; slot 28 is the blue herb's mix list | points at a 6-entry table: B+B->G, B+G->GB, B+GR->GRB, B+GG->GGB, B+GB->GG, B+GGB->GGG (`{with, result, remain=0, MIX_COMMON}`) | always | extra blue-herb mixes. The last two were added in `0a388bfd` (2025-07-01). B+B->G and B+GB->GG are BioRand inventions | `c153eeb4`; `re1.cpp:51-61`, `:219-239` |
| 9 | `0x448380` (5) | post-mix texture update ("some daft thing") | `E9 BB BD FC FF` = `jmp 0x414140` (refresh all inventory item textures) | always | correct icons after the new mixes | `86f933f5` 2025-08-07; `re1.cpp:223-226` |
| 10 | `0x4563C0..0x4563F7` (55) | locked-drawer handler's hard-coded ink-ribbon special case | `NOP` range | always | randomized items in lockpick drawers work for Jill (room 40A, lock 35) | `8f548507` 2025-04-01; `re1.cpp:241-244` |
| 11 | `0x456659..0x45666B`, `0x456A90..0x456A9B` | typewriter: "if player is Chris, ink is required" | `NOP` ranges | always | ink requirement no longer depends on character | `ccae801d` 2025-04-03; `re1.cpp:248-250` |
| 12 | `0x418C30`, `0x4190D5`, `0x45666C`, `0x456A9C` (1 each) | scenario flag index `123` ("NG+": hard enemies and Jill ink) | `122` | always | ink is controlled by flag 122 for both characters: allow ink items (`0x418C30`), show the ink model (`0x4190D5`), typewriter (`0x45666C`, `0x456A9C`). Flag 123 is left meaning hard mode only | `ad8d2d05` 2025-08-02; `re1.cpp:252-258` |
| 13 | CR `0x10072A62` (1) | Classic REbirth's "use ink ribbon" check reads flag 123 | `122` | always | same split inside Classic REbirth | `ad8d2d05`; `re1.cpp:260-261` |
| 14 | `0x455BA4..0x455BAF` | door code: lockpick substitutes for the sword key only if the player is Jill | `NOP` range | always (flag 124 decides) | Chris can use the lockpick when flag 124 is set | `d46cfdc1` 2025-04-03; `re1.cpp:264-268` |
| 15 | `0x455BCB..0x455BD5` | door code: Jill is refused the sword/small key ("need lockpick" path) | `NOP` range | always | Jill can use the sword key (and small key) when she has no lockpick | `d46cfdc1` |
| 16 | `0x455B4C` (1) | immediate `0x40` of the lock-byte Rebecca test | `0` | always | `0x40` bit ignored | `afd56f00` 2025-04-02; `re1.cpp:272` |
| 17 | `0x455B73` (1) | immediate `0x80` of the "is locked" test | `0xFF` | always | any non-zero lock byte is locked | `re1.cpp:273` |
| 18 | `0x455B7A..0x455B7D`, `0x455C81..0x455C83` | `and ..., 0x3F` before the flag test / flag set | `NOP` | always | the full byte is the flag index (check and key-turn set) | `re1.cpp:274-275` |
| 19 | `0x417F83`, `0x418066`, `0x4520B5`, `0x4520D4`, `0x455B83`, `0x455C87`, `0x455FDF`, `0x4562CB`, `0x456483` (u32 each) | absolute pointer to bank-2 lock flags (64 bits) | `&_customData.flag_locks` (256 bits) | always | room for 255 lock flags. Covers doors and item/drawer locks, and apparently the script flag-bank table, since `set 2:n` in scripts still works on the new storage | `re1.cpp:277-287` |
| 20 | `0x48DB4A` (5) | game-init path | `jmp PostGameInit` (zero `_customData`) | always | new game clears the extended lock flags | `re1.cpp:289`, `:379-382` |
| 21 | save/load hooks | n/a | `Modsdk_save` and `Modsdk_load` persist the 44-byte blob | always | extended lock flags survive save/load; old saves are refused | `afd56f00`; `re1.cpp:109-141` |
| 22 | `0x40B461`, `0x40B476`, `0x40B483`, `0x414103`, `0x414022`, `0x4142CC` (5 each) | 5-byte instruction that yields "this player has 8 slots" (true for Jill) | `call thunk`. The 16-byte thunk is `mov eax,&_inventorySize; cmp byte[eax],8; xor eax,eax; jl +1; inc eax; ret`, so eax = (size >= 8) | `inventory/size` -> tweak kind 2 (default Chris 6, Jill 8) | per-seed 6 or 8 slots for either character | `7d0eeea2` 2025-07-10; `re1.cpp:292-319` |
| 23 | `0x4063A0` (5) | Classic REbirth hook at this address (its purpose is unknown to the author) | restore the original bytes `53 A0 F6 86 C3` (`push ebx; mov al,[0x00C386F6]`) | always | with the CR hook, the flamethrower was not removed when placed on the cave latch from slot 7 or 8 (Chris with 8 slots) | `4d4b4b71` 2025-07-26; `re1.cpp:321-325` |
| 24 | `0x4AAD98` (12 x i16) | per-character weapon hit-scan heights: Chris 6 values, then Jill 6 (`-2026,-1656,-2530,-2280,-2040,-1800, -1917,-1617,-2190,-1940,-2003,-1720`) | values from the chosen character's `weapons.csv`, or the other vanilla character's | tweak kind 1, always written | aiming matches a swapped player model. Master swapped Chris and Jill when `ChangePlayer && SwapCharacters` | `f6f6e91a` 2023-07-01; `BR/RE1/Re1CrModBuilder.cs:936-965`, `re1.cpp:366-369` |
| 25 | (loader) | game file loader at `0x43E970(char*,void*,int)` | used to read `biorand.dat` | n/a | infrastructure | `671169d7`; `re1.cpp:144-148` |
| H1 | 5 sites `0x40B461 0x40B476 0x40B483 0x414103 0x414022`, then `0x4142CC` | as #22 | `B0 01 90 90 90` (`mov al,1`): always 8 slots | master: global UI checkbox `MaxInventorySize` | Chris gets 8 slots. **Replaced** by #22 | `393f44a7` 2023-08-29; `0x4142CC` added in `912b74a4` 2023-10-01 ("issues with Chris 8 slot stacking") |
| H2 | `0x41B208` (10) | partner inventory pointer set-up on partner swap | `C7 05 48 8E AA 00 14 88 C3 00` = `mov dword [0xAA8E48], 0xC38814` | as H1 | Rebecca shares Chris's inventory, so the 8-slot inventory does not overlap hers. **Dropped** in `c153eeb4` and not re-added | `94868283` 2023-08-31 |
| H3 | CR `0x100505A3` (7) | Classic REbirth's inventory-size check | `B8 01 00 00 00 90 90` (`mov eax,1`) | as H1 | CR side of 8 slots. **Dropped** in `c153eeb4` | `393f44a7` |
| H4 | CR `0x1006F0C2+3` (1) | CR inventory slot-count immediate | `8` | as H1 | CR side of 8 slots. **Dropped** in `c153eeb4` | `912b74a4` |
| H5 | 2023-25 biorand.dat writer | n/a | generic `{addr,len,data}` writer with 5 s hot reload | n/a | **replaced** by compiled-in patches plus `{kind,len,data}` tweaks | `7b5a0223` 2023-01-07 ("Allow biorand DLL to write to any memory"); `c153eeb4` |

Notes:

- The "original code" descriptions come from BioRand's source comments and
  commit titles. No disassembly was done here. Items 1, 5, 6, 10, 19 and 22 to
  23 name the routine's purpose, not the exact instruction, except where the
  comment quotes it (2, 3, 4, 7).
- The old 8-slot patch (H1 to H4) also needed RDT patches in rooms 100 and
  403 to let Rebecca use item boxes, because the inventory was shared (B, row
  `AllowPartnerItemBoxes`). The current thunk (#22) has no partner swap, yet
  `init.xml` still writes a 12-entry Chris/Rebecca array
  (`BR/RE1/Re1CrModBuilder.cs:741-786`). Whether Chris's slots 7 and 8 alias
  Rebecca's in staging is an **open question**. The `0x4063A0` fix (#23)
  suggests slots 7 and 8 already had issues.
- arklay already has equivalents of #3 and #4 (heal and blue herb at full HP,
  outside `--authentic`: `game.rs:5601`, `:15898-16012`), of #14 and #15
  (`has_lockpick` and the sword-key paths, `game.rs:6878-6884`, `:7686-7699`)
  and a configurable inventory size (`config.rs:466`). It has nothing for #5
  and #6, #7, #8 and #9, #10, #11 to #13, #16 to #21, or #24.

## B. Historical script and data patches (RE1)

Room ids are `SRR`, stage 1-based (RdtId(0,0x16) = 116). "BM" means the patch
is applied to both mansion copies (1xx and 6xx, 2xx and 7xx). Status means:

- **kept**: an equivalent is in staging `BR/RE1/Re1RdtPatcher.cs` (line given),
  `Re1CrModBuilder` or `meta/re1/rdt.json`.
- **replaced**: different bytes or mechanism with the same goal.
- **dropped**: gone.

Data-file formats over time:

- `IntelOrca.Biohazard/data/re1/re1_rdt.json` (2022-11/12): doors, items,
  `requires`, `lock`. No byte patches.
- `IntelOrca.Biohazard{,.BioRand}/data/re1/rdt.json` (2022-12 to 2025-03) adds:
  - `doorRando[]: {player, category, cutscene, nop: [...]}`: NOPs applied when
    door rando is on.
  - `enemies[]: {nop}`: NOPs applied when enemies are randomized at that
    difficulty.
  - `doors[]: {create, texture, special, entrance{x,y,z,d,floor,cut}}`:
    convert an `aot_set` to a door, or override the spawn point.
  - `items[]: {nop}`.
- `BR/meta/re1/rdt.json` (staging) keeps `items[].typeOffsets` (bytes patched
  to the new item type), `doors[].entrance`, one `enemies[].nop` (108) and
  `lockId`/`lockKey`/`allowedLocks`/`kind`.
- `9ac20a8d` (2025-03-10, "Remove old door rando blocks") and `2749d51e`
  ("Remove old correct rdt.json") deleted the `doorRando.nop` blocks.

| Room | Description | Era / commit | Status in staging |
|---|---|---|---|
| 100, 403 | NOP partner check on the item box (`100:0x10C92`, `403:0x1F920`) so Rebecca can use boxes (needed for the shared 8-slot inventory) | `94868283` 2023-08-31 | **dropped**: `#if false` in `9987af16` 2025-07-17 (`Re1RdtPatcher.cs:575-589`); the reason is not stated. Probably moot after H2 was dropped |
| 101 (Chris) | enemy id hack for 601 reserved ids | staging only | kept `:528-540` |
| 104 | door 2 spawn fix `NextX/Y/Z = 12700/-7200/3300` | master `Re1DoorHelper.End` (non-door-rando) | kept `FixDoor104 :237-244` (now always) |
| 105 | `typeOffsets 0x2831F` (P0), `0x2848B` (P1): wood emblem type check | `53c1eb50` 2025-03-06 | kept (json) |
| 106 | **ink via flag 123**: `set 0:123` in 106 init | `c72ffb49` 2025-03-08, `ccae801d` | **replaced** by flag 122 plus DLL #12 (`ad8d2d05`) `:61-82` |
| 106 | Jill extra items: `Nop(0x2FC06)`, `Nop(0x31862)` | `c72ffb49` | replaced (`eda136fe`), then by `EnableMoreJillItems` |
| 106 | `EnableMoreJillItems`: `Patch(0x2FC02+1..3 = 7,52,0)`, `Nop(0x2FC06)`, same at `0x2FFBC`, `Nop(0x31862)` | 2025-03..07 | **replaced** by `EnableItemForJill106 :898-909` (`c546d005` "Fix ink in main hall for Jill") |
| 106 (Jill) | `Nop(0x31B02)`: Barry does not give the lockpick | `9d73bcfb` 2025-07-05 | kept `:72-76` (when the lockpick option is off) |
| 106 | lockpick flag `set 0:124` per option | `d46cfdc1` 2025-04-03 | kept `:61-82` |
| 106 | create the front door 3 (`texture 3`, `special 6`, entrance 17200,0,3000,-1024) in door rando | `fa5b20ec` 2024-02-05 (json `create`) | **replaced** by `FixDoor106 :246-286` (`1581fba4`) |
| 106 | WIP: append main hall door 5 to the init SCD; NOP `0x2FC22-0x2FD84`, `0x2FD8E-0x2FDD8` | `a98d65ef` (`re1-main-door`, unmerged) | **dropped** (superseded by `fa5b20ec`) |
| 107, 20B (BM) | maps as items: move the item aot, repoint enable/disable (`0x349C4`, `0x349CA`), new item aot in 20B, `Nop(0xCD68, 0xCE52)` | `8ec6a743` 2025-07-15 | kept `:84-126` |
| 300, 30F, 406 | maps as items (`300:Nop 0x15E66-0x15E78`, `30F:Nop 0x35B04`) | `8ec6a743` | kept `:128-152` |
| 108 | NOP dog-window ambush `0x19754-0x197EE` | `82f860a3` 2024-02-05 (json `enemies.nop`) | kept `DisableDogWindows :174-178`, plus json `enemies.nop` |
| 10D | `typeOffsets 0x26E7` (red eye), `0x270F` (blue eye): tiger statue item checks | `c4cc392b` 2022-12-16 ("Fix Tiger puzzle", `ItemRandomiser.PatchRE1Stuff`) | kept (json `typeOffsets`; legacy code `BR/ItemRandomiser.cs:1231-1244`) |
| 10F | `typeOffsets 0x30C45`/`0x30BB9`: item behind the sliding door | `53c1eb50` | kept (json) |
| 112 (BM) | NOP `0x17864-0x17866`, `0x17884-0x17886`: enable the wardrobe/closet door | `fd54b35f` 2023-09-01 (json, door rando only) | kept `FixDoorToWardrobe :288-293` (now always) |
| 112, 113, 116, 219, 40A | `typeOffsets` (ink `0x17969`, bathtub `0x1CBC5`, shotgun frame `0x1FEFD`, battery `0x14B51`, V-JOLT book `0x39383`) | `53c1eb50`, `8ec6a743` | kept (json) |
| 113, 11B, 211, 21C, 401, 40F (Jill) | enable small-key pickups for Jill (`113:0x1CAD8.. 0x1CBBA`, `11B:0x4A7E..0x4B4C`, `211:0x10C1C, 0x10C42`, plus added opcodes in 21C and 401, `40F:0x1F6FE`) | `7180cfa0` 2025-07-30 | kept `:911-968` |
| 114 | NOP dog-boiler ambush `0x24B80-0x24C1C` | `55eedfdf` 2025-03-08 | kept `:180-184` |
| 115 (BM, Jill) | NOP `0x22F4-0x231E`: Barry rescue, door rando | `06728d9f` 2025-08-03 | kept `DiableJillSandwich :443-450` |
| 115 | sandwich-room locks: `Patch(0x22BC+2, 1)`, `Patch(0x22DE+2, 1)`, later `+128`; `Nop(0x2342)` (Jill) | `e0241ef4` 2025-03-08, `77ccf4c9` 2025-04-27 | **replaced** (`9e494022`): remap every `set` in 115 (index 0 -> 15, 1/2 -> 16) and add `set 2:15` in 109/609 and `set 2:16` in 116/516 (`:472-487`). `Nop(0x2342)` kept |
| 116 | `Nop(0x1FE16)`: shotgun pickup always had amount 7 | `21183ad5` 2023-09-07 | kept `:458` (re-added `a5fe82a9` "multiple uses of item") |
| 116 | `Patch(0x1FE62+2..9 = 0)`: cannot put the shotgun back | `e0241ef4` | kept `:461-464` |
| 116 | rewrite every `testpickup` (`0x15`) opcode whose type equals the original item | `9997272e` 2023-01-20 (#184) | **replaced** by explicit `typeOffsets` (`53c1eb50`); the legacy code is still in `BR/ItemRandomiser.cs:1210-1222` |
| 11A | `FixCrestDoor`: `Patch(0x35DC+2, 0x35F0+2 = 128|23)` | `fa3e1037` 2025-06-15 | **dropped** in `9e494022` (vanilla ids no longer OR'ed with 0x80) |
| 201 (BM) | pass-code door: `LockId 209`, `LockType 255`, `Free 129`, spawn 11200/28000; Jill non-door-rando adds `if 1:0x25 { set 2:17 }`; `Nop(0x41A34)`; mansion 2 `Nop(0x41A92)` (P0) or `0x41A5C-0x41A68` (P1) | `df3205cb` 2023-11-28, `ddaa3db2` 2023-11-30 (the first version checked `0:0x46` and was wrong for Jill) | kept `:295-329` (lock id 209 again; see the lock section) |
| 203 (Jill) | NOP `0x23698-0x236EA`: Barry cutscene softlock in door rando (#119) | `213dac2c` 2023-01-06 | **dropped** (`9ac20a8d`) |
| 205, 306, 307, 30F, 405, 406, 60F, 61A, ... | crashes from enemies during cutscenes | 2023 | json enemy conditions/excludes, not byte patches (see C) |
| 206 / 207 | 207: convert `aot_set` to door 4 (broken door, `Re1UnkA 3`, `UnkC 7`, later `UnkC 2`, `LockId 149 -> 21|0x80 -> 21`, `LockType 255`); 206: entrance override 9180,0,11280 | `dd755c2a` 2023-09-01; `971911e9`, `ddefaba0`, `9e494022` | kept `AddDoor207 :210-235` (lock 21) |
| 20A (BM) | fish-tank document on the desk | `ed545bf5` 2025-07-18 | kept `:154-172` |
| 20C, 40C, 512, 513, 514, 601, 60A, 706 | disable later cutscenes (for example `40C:Nop 0x64C8/0x64D4`; `20C:Patch 0x96EA+14=0x2C`; `513` enemy set-up and flag `55`) | `bdfd01a4`, `cc79e182` 2025-03-08 | kept in `Re1CrModBuilder.DisableCutscenes` (`Re1CrModBuilder.cs:276-411`) |
| 20D | NOP `0x2914C`, `0x29246`, `0x291AE`: door block during the serum challenge | `c2d30654` 2025-07-05 | kept `:490-502` |
| 20E | NOP the poison challenge (Chris `0x10724, 0x1073A-0x10740, 0x1075A-0x107FC, 0x107F2-0x107FC`; Jill `0x10724-0x1072A, 0x10744-0x10780, 0x1078A-0x10794`) | `e0241ef4` | kept `:504-520` |
| 211 | NOP `0x10BB2-0x10C0A` (blood pool pickup): dogs and bees crash the game if they attack during the bend-down animation | `0367597b` 2025-07-27 | kept `:199-208` |
| 212 (Jill) | NOP `0x2474E-0x2475A`: force the cutscene in door rando | `fb1a7bf3` 2023-09-03 | **dropped** (`9ac20a8d`) |
| 214 (BM) | `ConvertToDoor(id 1 or 5, 0, LockType 254, lock 209)`: rough passage unlocks the pass-code door from the far side; Jill `Nop 0x19F3A, 0x1A016, 0x1A01C`; json entrance overrides | `df3205cb` | kept `:423-441` |
| 302 | NOP `0x18DA2`, `0x18DE0`: battery lift can corrupt the inventory (#141) | `92cfe72c` 2023-05-02 | **replaced** by `41bf8f83` |
| 302 | NOP the odd change-player opcodes `0x192C4, 0x192D0, 0x192DC, 0x1944A, 0x19450` (#475) | `41bf8f83` 2023-09-01 | kept `ForceLift302 :647-673` |
| 300, 301, 302 | force the lift state: `set 0:48`/`0:49` | `1581fba4` 2025-07-25 | kept `:652-658` |
| 302 | disable the waterfall (`0x18E40-0x18E58, 0x18EB4-0x18ED6, 0x18F18-0x18F2A, 0x19008-0x191E2`) | `62a6c6cb` 2025-07-29 | kept `:667-671` |
| 303 / 305 | fountain/heliport doors: `ConvertToDoor(8, 11)`, `Nop 303:0x111BE, 0x111C0`, `305:0x3E9AE`, cut fix, `LockId 2`, `LockType 255` | `f9288e75` 2025-03-05, `1530b21f` | kept `EnableFountainHeliportDoors :591-629` |
| 303 | force Tyrant 2: `set 0:43` | `c1bb0a2a` 2023-09-01 (door rando only) | kept `ForceHelipadTyrant :638-645` (option `forceTyrant`) |
| 303, 601 | NOP `303:0x11342` (70466), `601:0x24D58` (150872) to force the tyrant | `5e2ab491` (`forcetyrant`, unmerged, 2022-12-29) | **dropped** (replaced by `set 0:43`) |
| 305 | NOP `0x3EEEC`, `0x3EEF8`: player sometimes changes (#475) | `41bf8f83` (json, door rando) | kept `FixPlayerChangeGlitch305 :631-636` (deleted in `9ac20a8d`, re-added `36ba4cd7` 2025-08-07) |
| 308 (Jill) | NOP `0x1AFD8-0x1B074`: Barry cutscene crash (#121) | `91b09290` 2023-01-06 | **replaced** by `AddVariableHelper308 :703-719` (`dc33a4a1`): set `FG_ROOM[0]` when the Barry conditions hold, and gate enemies on it |
| 309 (Jill), 601, 706 (Chris) | NOP enemy sets that attack Barry or Rebecca during cave/lab cutscenes (`309:0x2D83C, 0x2D852`; `601:0x24D0A..0x24E00`; `706:0x3792A, 0x37954`) | `90e9561c`, `550d2863` 2025-07-14/16 | kept `:542-572` |
| 405 (Jill) | `Nop(0x194A2)`: Barry eavesdrop cutscene; snakes crash during it (#678) | `b5d99243` 2023-12-01 | kept `:522-526` (`1581fba4` fixed a wrong room 305 -> 405) |
| 406 / 216 | keep bookcases moved through doors (`set 0:79`, `set 0:69`) | `29df8019` 2025-07-29 | kept `:721-751` |
| 408 | NOP `0x30ACC, 0x30ACE, 0x30AEC, 0x30AEE` (199372..199406): double doors to the Plant 42 room (#86) | `29c74df6` 2022-12-20 (json, door rando) | kept in `FixDoorPlant42 :359-361` |
| 408, 409, 40C, 40F (Chris) | no Rebecca switch at Plant 42 (`40C:0x66DC-0x6712`), V-JOLT switch-back (`40F:0x1F782`), force the Rebecca cutscenes (`408:0x30B4E, 0x30B90`; `409:0x16856`) | `f0aee4ff` 2025-07-14 | kept `:402-421` |
| 409 (Chris) | NOP `0x166D4-0x16742`, `0x168AA-0x16918` (drug store) | `23cc2194` 2025-03-07 | kept `:331-336` |
| 40A / 40C | keep the bookcase open (`40A:0x393BC`); `ConvertToDoor(1,5)`; no switch to mansion 2 (`0x676C`/`0x66AC`); block the door until Plant 42 is dead (`Patch 0x67AE/0x66EE` to check `3:96==1`); NOP `0x65FA-0x6676` (crash on re-entering after a kill) | `9e728e38`, `27f59bee`, `90e69fad` 2025-07-25/27 | kept `:338-400` |
| 500 | NOP `0xCF1A-0xCF5E` (P0) / `0xCF1A-0xCF90` (P1): softlock when the escape sequence is active (#529) | `bf82913a` 2023-08-05 | **replaced** by `FixDoor500 :675-687` (`86902d65`, narrower: `0xCF42-0xCF54` / `0xCF70-0xCF82`, "prevent leaving due to Tyrant 1 being killed") |
| 502 | NOP `0x4934-0x4964, 0x4970-0x497A` (P0) and `0x4984-0x4A72`, later `0x498E-0x499C, 0x49A8, 0x4A00` (P1): softlock if the cutscene plays (#106) | `315679a5` 2023-01-06, `f659f5b2` | **replaced** by `FixCutscene502 :689-701` (`eae49ec0`, conditional opcodes, Jill) |
| 507, 509, 510 | MO-disc readers (patched checks, Jill variants `509:0x1D13A`, `510:0x253FA`) | `3831f5a6`, `f7bcc14e` 2025-07-29 | kept `:753-865` |
| 508 | NOP `0x442, 0x450, 0x480` (door rando) | `f659f5b2` 2023-09-01 | **replaced** by `FixMoDiscDoor508 :867-896`: item-flag checks become scenario flags 112/113/114 |
| 603, 609 | NOP hunter ambushes (`603:0x1ABA2-0x1ABD4`; `609:0x1CB8E-0x1CBC0` / `0x1CB52-0x1CB84`) | `b4da817c` 2023-10-16 (json), `7b2f9fba` 2025-07-16 | kept `:186-197` |
| (doors) | `c09b79fc`: duplicate a door under `if 0:0` / `else` around enemy placement | 2023-10-23 | **dropped** (`ede81346`) |
| (locks) | OR `0x80` into vanilla lock ids | `afd56f00` | **dropped** (`9e494022`) |
| (locks) | master door rando lock ids 128..252; `LockType 254` on the unlock side, 255 on the far side | `34afad9b`, `387933d7` 2022-11 | **replaced** by `LockRandomizer`/`DoorRandomizer` (1..254, DLL #16 to #21) |

That makes 61 rows: 41 kept, 13 replaced, 7 dropped. The cutscene-crash row
is data only.

## C. RE1 bug-fix commits fixed by a game patch

Exe patches:

| Commit | Date | Message | What was patched |
|---|---|---|---|
| `249ae907` | 2023-08-22 | Fix flamethrower ammo combine | A#2 |
| `a660ad5f` | 2023-08-22 | Allow herb use when at full health | A#3 |
| `6af9334b` | 2023-08-25 | Fix hurting and killing a Neptune | A#5, A#6 |
| `912b74a4` | 2023-10-01 | Fix RE 1, issues with Chris 8 slot stacking | H1 site `0x4142CC`, H4 |
| `4f27c071` | 2023-09-16 | Fix #566: RE 1, Yawn poison can not be healed with blue herb | A#7 |
| `f6f6e91a` | 2023-07-01 | Fix RE 1, weapon hit scan | A#24 |
| `2751dd27` | 2025-03-16 | Fix using blue herb when not needed | A#4 `0x447AAD` |
| `8f548507` | 2025-04-01 | Fix room 40A drawer for Jill | A#10 |
| `4d4b4b71` | 2025-07-26 | Fix flamethrower not removed for Chris when in 7/8 | A#23 |
| `86f933f5` | 2025-08-07 | Fix item mixing texture | A#9 |
| `ad8d2d05` | 2025-08-02 | Split hard mode and ink ribbon flag | A#12, A#13. Before this, ink on flag 123 also made enemies NG+ hard |

Script and data patches:

| Commit | Date | Message | What was patched |
|---|---|---|---|
| `c4cc392b` | 2022-12-16 | Fix Tiger puzzle | 10D item-type bytes `0x26E7`, `0x270F` |
| `29c74df6` | 2022-12-20 | Fix #86: double doors to plant 42 room not randomized | 408 NOPs |
| `315679a5` | 2023-01-06 | Fix #106: room 502, possible softlock if cutscene plays | 502 NOPs |
| `213dac2c` | 2023-01-06 | Fix #119: room 203, Barry cutscene can softlock | 203 NOP |
| `91b09290` | 2023-01-06 | Fix #121: room 308 Barry cutscene can crash game | 308 NOP |
| `9997272e` | 2023-01-20 | Fix #184: room 116, broken shotgun cannot be placed | `testpickup` type rewrite |
| `92cfe72c` | 2023-05-02 | Fix #141: room 302 battery lift can corrupt inventory | 302 NOPs |
| `bf82913a` | 2023-08-05 | Fix #529: room 500, softlock when escape sequence active | 500 NOPs |
| `41bf8f83` | 2023-09-01 | Fix #475: room 305, player sometimes changes | 302 and 305 NOPs |
| `c1bb0a2a` | 2023-09-01 | force Tyrant 2 fight in door randos | 303 `set 0:43` |
| `21183ad5` | 2023-09-07 | room 116, pickup always has amount 7 | 116 `Nop(0x1FE16)` |
| `b4da817c` | 2023-10-16 | room 609, crash due to hunter ambush | 609 NOPs |
| `df3205cb`, `ddaa3db2` | 2023-11-28/30 | pass code door unlock (other side; Jill) | 201 and 214 door rewrite |
| `b5d99243` | 2023-12-01 | Fix #678: room 405, crash due to snakes during door cutscene | 405 `Nop(0x194A2)` |
| `e0241ef4` | 2025-03-08 | Fix shotgun on wall issues | 116/516 `0x1FE62`, 115 locks, 20E poison challenge |
| `fa3e1037` | 2025-06-15 | Fix crest door always locked | 11A lock-id patch (lock-limit side effect) |
| `77ccf4c9` | 2025-04-27 | Fix unlock doors and trap doors | 115 +128 patches; 109/609/116/516 `set 2:128/129` |
| `9e494022` | 2025-07-04 | Fix lock ids | drop `0x80` OR, 115 remap to 15/16, remove FixCrestDoor |
| `9d73bcfb` | 2025-07-05 | Fix Jill getting lockpick from Barry when she isn't supposed to | 106 `Nop(0x31B02)` |
| `341c7b6d` | 2025-07-05 | Fix lockpick and ink when cutscenes are enabled | 106 flag block always appended |
| `c2d30654` | 2025-07-05 | Disable blocked door in serum challenge | 20D NOPs |
| `f0aee4ff` | 2025-07-14 | Fix Chris, V-JOLT and Rebecca switching | 40C, 40F, 408, 409 NOPs |
| `90e9561c` | 2025-07-14 | Fix Barry cave cutscenes | 309 NOPs |
| `7b2f9fba` | 2025-07-16 | Fix crash due to hunter ambushes | 603 and 609 NOPs |
| `550d2863` | 2025-07-16 | Fix hunter Rebecca attack and voices | 601 and 706 NOPs |
| `a5fe82a9` | 2025-07-23 | Fix multiple uses of item from living room | 116 `Nop(0x1FE16)` |
| `1581fba4` | 2025-07-25 | Fix various doors in door rando | 106 door, 405 room-id fix, 300-302 lift flags |
| `27f59bee` | 2025-07-27 | Block plant 42 door until its dead in door rando | 40C patches |
| `90e69fad` | 2025-07-27 | Fix crash with plant 42 | 40C NOP `0x65FA-0x6676` |
| `0367597b` | 2025-07-27 | Remove blood from 211 to stop crash | 211 NOP |
| `86902d65` | 2025-07-27 | Fix a few rooms in door rando | 500 NOPs |
| `dc33a4a1` | 2025-07-28 | Fix room 308, enemies and cutscene | 308 helper flag |
| `29df8019` | 2025-07-29 | Fix bookcases moving back | 216, 406 flags |
| `62a6c6cb` | 2025-07-29 | Disable waterfall in door rando | 302 NOPs |
| `3831f5a6`, `f7bcc14e` | 2025-07-29 | Fix MO disc reading (and for Jill) | 507-510 |
| `c546d005` | 2025-07-30 | Fix ink in main hall for Jill | 106 |
| `7180cfa0` | 2025-07-30 | Add small key item pickups for Jill | 113, 11B, 211, 21C, 401, 40F |
| `06728d9f` | 2025-08-03 | Disable Barry rescue in door rando | 115 NOP |
| `eae49ec0` | 2025-08-03 | Fix Barry death cutscene 502 | 502 opcodes |
| `36ba4cd7` | 2025-08-07 | Fix player change in 305 | 305 NOPs (restored) |

Related fixes made only with data constraints, not byte patches. These say
which hazards the patches do not cover:

- Enemy exclusions in cutscene rooms: `c73b11cd` (#102, 405), `a28731a2`
  (#104, 208), `25cae347` (#143, 10E dog in closet), `7c20175b` (#234, 20E
  Yawn), `72879442` (#579, 20E Neptune), `13043370` (#168, 309), `ee92663a`
  (#165, 400 statue), `a28d6b24` (107 Yawn), `e28f92b8` (#673, 40F V-JOLT
  hurt), `07005806` (#705, 100), `2c32d063` (502), and the 2023-09/2024-01
  "crash during cutscene" series (#575, #583, #585, #586, #593, `4bed563c`,
  `755b68f0`, `e290f1c3`, `99d5d404`, `93222edd`).
- Enemy limits: `826a0e19` (#70, Chimera), `e5ee060d` (#105, Tyrant 2 swap),
  `a141aca0` (too many enemies crash).
- `noUnlock` on ladders and lifts (a locked one softlocks): `4765f6b7` 71A,
  `14fa605e` 30F, `cbe02d7e` 302.
- `39947eb1` (#756): music-notes amount must be 0 or Jill softlocks in 10F.
- `a212d91a` (#221): Rebecca needs spare slots for serum/V-JOLT.
- `cb9239cd` (#311): the Rebecca inventory length must be 12 (6+6).

Counts: 11 exe fix commits and 40 script/data fix rows (covering 42 commits).

## Cosmetic features (out of scope)

Enemy skins, NPC skins and models, protagonist skin, face card and inventory
face, player-animation height fix, voices and cutscene voice rando, BGM/music,
title call, room sound banks (`FixRoomSounds`), background textures, cutscene
NPC randomization.
