# Arklay architecture

Arklay is a from-scratch engine for the classic Resident Evil games. A game
installation is migrated into an `.akpak` pack by `convert-game`; the runtime
opens the pack (an `.akpak` image or a directory of unpacked entries), loads
one room and drives it at a fixed 30 Hz. Everything game-specific lives in the
pack, so the engine has no hard-coded game data.

This document is the map: the modules and their responsibilities, the frame
data flow, the trust boundaries, the format inventory with each reader's
budget, the engine invariants and the test taxonomy. `docs/status.md` records
what is implemented and what the project knows it still approximates;
`docs/m17-deviations.md` records what the hardening milestone does not prove.

## Module map

Every module of `src/lib.rs`, with the subsystem that owns it and its
responsibility.

### Container and tooling

| Module | Owner | Responsibility |
|---|---|---|
| `pack` | Container | `.akpak` v1 reader/writer with raw and zstd-compressed entries, layered mod packs, exact-path and `AssetClass` stem lookup, atomic `PackWriter::write` |
| `format` | Container | Per-file RE1/RE2 dialect detection (RDT, EMD) by scored header heuristics, plus WAV codec and MPEG sniffs |
| `budget` | Container | Fixed per-format decode caps and checked allocation shared by every parser |
| `manifest` | Container | `manifest.toml` grammar (`format`, `kind`, `base`, `load_order`, Lua hooks) |
| `modding` | Tooling | Sibling `mods/` discovery and the `mod build` source-directory builder |
| `convert` | Tooling | `convert-game`: discover an install and migrate every asset category — rooms, audio, voice and films — into one pack; progress on stderr, per-file sizes with `--verbose` |
| `convert_re2` | Tooling | `convert-game` for the RE2 PC install: rooms of both discs, `roomcut.bin` backgrounds, enemies, players, voice, BGM, doors, films and demos |
| `adt` | Format | RE2 ADT background decompression (three-table Huffman with a 16 KiB history) and the 320x240 page reorganisation |
| `sap` | Format | RE2 `.sap` sound containers: the bit-mask header and the RIFF WAVs it fronts, one per slot |
| `adpcm` | Format | MS-ADPCM (WAVE tag 2) decoding to 16-bit PCM, used by `audio::parse_wav` |
| `se2` | Runtime | RE2 sound banks: pack names, EDH sound tables, the `SsLoadBanks` bank layering and `Snd_se_on` id to WAV and gain |
| `bgm2` | Runtime | RE2 room music table (`bgm_tbl`), its channel and track name tables, and the `.BGM` decision |
| `progress` | Tooling | One terminal-aware overall progress bar per command, spanning its internal phases (and periodic plain lines when redirected) |
| `atomic` | Tooling | Sibling-temp-then-rename single-file output used by every artifact writer |
| `verify` | Tooling | `arklay verify`: path-classified parse of every pack entry with a per-format report |
| `stats` | Tooling | Fixed-bucket frame-time histogram and the `--stats` report |
| `hash` | Tooling | `arklay hash`: the HD texture-pack filename hash and its per-class texture conversion |
| `trace` | Tooling | Oracle trace v1 emission (`oracle/trace-v1.md`) shared by the C++ reference and arklay |

### Runtime

| Module | Owner | Responsibility |
|---|---|---|
| `engine` | Runtime | Entry points, room loading, the fixed 30 Hz game loop, transitions, headless simulate/capture seams, SDL window/renderer |
| `game` | Runtime | `GameState`, script hosts, entities, items, flags, room actions, effects tick, save integration |
| `state` | Runtime | `RoomId`, `RoomState` (cuts, zones, collisions, lights), `Image` |
| `controls` | Runtime | The logical action map: every action the game reads and the physical keyboard, mouse and gamepad bindings to it |
| `speed` | Runtime | The F2/F3 gameplay speed multiplier on the fixed-tick interval (1x..3x) |
| `process_stats` | Runtime | The process's memory and CPU use for the F6 debug HUD |
| `screenshot` | Runtime | F12 screenshots: the presented frame cropped to its band, encoded as PNG and written to the data root's `screenshots/` |
| `combat` | Runtime | Shared combat tables (weapon hit ranges and damage) and the weapon-damage pipeline |
| `weapons` | Runtime | The player weapon runtime: aim, fire, reload and the held-weapon FX |
| `player` | Runtime | Player locomotion, input mapping, collision, reach probes, camera cuts |
| `player_script` | Runtime | The player's scripted-animation states |
| `rdt` | Format | RDT room parser (header pointers, cameras, zones, collisions, embedded model/TIM pairs) |
| `rdt2` | Format | RE2 RDT room reader into a `RoomSource`: cameras (RID), camera switch zones (RVD), collision (SCA) and per-camera lights (LIT) |
| `room` | Format | `room/<id>/` room folders: the `room.xml` reader/writer, the RE1 and RE2 RDT-to-folder conversions (models, effects, animation and scripts by slot), the checked-in RE1 definitions and the art `convert-game` takes from an RDT beside them, the room strings' language layers, and the loader that picks the XML or a raw RDT (`room.rdt`, or a variant's own `jill-mansion2.rdt`); `<slots>` names the files a variant's model, effect and animation slots read |
| `scd` | Runtime/Format | SCD opcode tables, reader, IR, disassembler/decompiler, assembler, command and event VMs |
| `scd2` | Format | RE2 SCD reader: init and main procedure lists, the RE2 opcode table, and `scd export` to `.s` and structured `.akscr` |
| `door` | Format/Runtime | `.dor` container parser and the door-animation VM |
| `door2` | Format/Runtime | RE2 `.do2` parser (VAB sounds, MD1 mesh, TIM) and its SCD door VM, framed like `door` |
| `transition` | Runtime | The door-transition timeline (screen space, masks, camera, `.dor` stepper hand-off) |
| `stairs` | Runtime | `stairs_height_update` and `set_stairs_zone` room actions |
| `objects` | Runtime | RDT-embedded object TMD/TIM pairs, runtime object records and transforms |
| `enemy` | Runtime | Entities: the shared character/monster runtime, the pack-script host, movement/pathfinding/SCA helpers, the scripted-character and monster machines |
| `effects` | Runtime | The 64-slot billboard effect pool, per-room sprite tables, packed texture pages, behaviours |
| `items` | Runtime | The original's item tables: image classes, icon rows, model names, combine rules |
| `item_catalog` | Runtime | Item definitions (`item/<id>/item.xml`): kind, stack, merge, heal/cure effects and per-language strings; embedded defaults overridden per item by the pack |
| `item_art` | Runtime | Per-item inventory art: icon/model/texture entry names, the TMD+page loader, icon baking and named HD keys |
| `examine` | Runtime | The item viewer's game rules: the examine check, turntable spin and viewer camera |
| `demo` | Runtime | The attract-demo reels (`demo/<hex>.dat`) and their recorded pad words |
| `message` | Runtime | The seven-state message window and its byte grammar |
| `save` | Runtime | The 0x800-byte BioCard block, slot I/O, the folder scan and explicit-path actions the screen API uses, `Apply_to`/`from_state` round-trip |
| `config` | Runtime | The user configuration file (`arklay.toml`): `save_dir`, `language`, `[bindings]`, `[game]`, `[graphics]` and `[mods]`; the default `<config dir>/save` and `<config dir>/mods` locations |
| `audio` | Runtime | WAV parsing and the single SDL3 software mixer used for SE, voice, BGM and film audio |
| `bgm` | Runtime | The three-channel music state machine, ramps and channel hand-off |
| `sfx` | Runtime | Per-room sound-name tables, enemy sound-set resolution and footstep resolution |
| `voice` | Runtime | Voice pack-entry naming (`voice/<row>_<id>.wav`) and the stage-to-row selector |
| `voice_table` | Tooling | The shipped per-stage voice-line name tables `convert-game` renames into pack entries |
| `music` | Runtime | Room music tables loaded from the pack, track ids and path naming |
| `movie` | Runtime | The film table, `movie_on` hand-off and the playback session |
| `ending` | Runtime | Ending-row selection and its film chain |
| `lua` | Runtime | Sandboxed Lua 5.4 hooks, always compiled in |

### Presentation and UI

| Module | Owner | Responsibility |
|---|---|---|
| `render` | Presentation | Deterministic software rasterizer: framebuffer, models, masks, shadows, effect billboards, lighting |
| `raster` | Presentation | The reference's 3D triangle pipeline reproduced step for step, for llvmpipe-exact pixels |
| `gpu` | Presentation | Interactive SDL_GPU renderer for the gameplay scene (the default; software fallback); never used by captures, traces or tests of record |
| `htexture` | Presentation | HD texture replacement by content hash (`htexture/`), including the loader cache-list mask pairing and RE2 per-page model skins |
| `video` | Presentation | The `[graphics]` options, the 4:3 viewport fit and the F8 window steps |
| `tween` | Presentation | Presentation tweening for render rates above the 30 Hz simulation |
| `pacer` | Presentation | When the interactive loop presents a frame |
| `model` | Presentation | Shared decoded model types (`Texture8`, mesh/skeleton vocabulary) |
| `anim` | Presentation | Fixed-point skeletal animation and clip playback |
| `tmd` | Format | PSX TMD mesh parser |
| `tim` | Format | PSX TIM decoder: 16bpp direct, 8bpp/4bpp indexed with CLUT |
| `emd` | Format | EMD character/room model containers and EMW weapon animations |
| `emd2` | Format | RE2 EMD/PLD/PLW model containers (MD1 meshes, packed-angle EDD/EMR banks, weapon composites onto the player) |
| `model_viewer` | Runtime | Posed, animated model in a room with no AI (`--model`), placed in front of each camera or at `--model-pos` |
| `ivm` | Format | Item-view model (texture page + TMD) parser |
| `avi` | Format | RIFF/AVI demuxer for the shipped Cinepak films |
| `mp4` | Format | Minimal ISO-BMFF demuxer for HD (H.264/AAC) film replacements |
| `media` | Format | Container-independent H.264/AAC film description shared by MP4 and AVI |
| `hdfilm` | Presentation | HD film decoding: H.264 through OpenH264 on a worker thread, AAC through Symphonia |
| `cinepak` | Format | Cinepak (`cvid`) frame decoder |
| `mpeg` | Format | MPEG-1 program stream demuxer, MPEG-1 video and MP2 audio decoders for the RE2 films |
| `bmp` | Format | BMP encode/decode, including the black-key room-mask decode |
| `assets` | Format | Replacement-format resolution (`webp > png > bmp > tim > pix`, `ogg > wav`) and the PNG/WebP/OGG decoders behind the default-on `hd` feature; an image page is an indexed `Texture8` or an RGBA `Image` |
| `lzw` | Format | RE1 `.pak` LZW decoder |
| `shadow` | Presentation | The player's baked ground shadow: decode and placement |
| `font` | Presentation | Glyph decoding, per-glyph metrics measured from the sheet's ink, and tinted drawing; draws the synthetic missing-glyph box |
| `charset` | Presentation | The sheets' byte <-> character maps (Japanese and Western): greedy cell encoding, substitutes and the missing marker |
| `language` | Presentation | `language/<tag>.txt` grammar, the layered catalog, the fallback chain and OS locale detection |
| `text` | Presentation | The runtime string facade: messages, item names/descriptions, save strings, room labels and `UI_*` |
| `mask` | Presentation | Room mask (foreground overlay) sprites and depth ordering |
| `ui` | UI | `Screen` contract, title, character select, pause menu, item box, FILE, map, item viewer, save/load (the ported original and the Lua screen surface, `ui/lua_screen.rs`, selected by the pack's own `api:register_save_screen`); the port-only F1 debug tools (`ui/debug_menu/`, on the `gui` screens apart from the room and inventory grids) and the F9 return-to-title prompt (`ui/return_title.rs`) |
| `gui` | UI | The native-resolution menus (system menu, options, load/save, F1 debug tools): `gui/dom` is a retained widget tree built from `assets/ui/*.xml`, with a layout, focus and input routing, hints, and Lua scripting; `system_menu`, `save_screen` and `ui/debug_menu` drive it and turn its outputs into engine requests |

### GUI screen markup

Each modern screen is one XML document in `assets/ui/`. Its root is a `<screen>`, and `components.xml` defines the shared parts.

- `standard-screen` gives every screen the same title (with breadcrumbs from the screens below it), a caption, a `header` slot, the content, a `footer` slot and the hint bar. Cancel goes back.
- `option-category`, `option-detail` and `save-row` are the other shared components.

**Elements**

- Containers: `stack` (`orientation`, and `gap` as a theme token), `split`, `list`, `tabs`/`tab`.
- Content: `text` (with a `variant`), `image`, `badge`, `separator`, `spacer`, `facts`, `hint-bar`.
- Interactive rows: `item`, `choice`, `toggle`, `slider`, `link` and `info`.
- Any other hyphenated name is a custom widget that the host registers.
- Sizes come from the theme. Only `width`/`height` overrides appear in the markup.

**Attributes**

- An attribute value is a literal, a `{path}` binding into the store (`item.*` inside a `<template>`, `focus.*` for the focused row), or `@key` for a GUI string.
- `items="{path}"` repeats a `<template>` once per list entry.
- `def="{path}"` fills an option row from a map with keys such as `label`, `choices`, `value` and `default`.

**Actions**

- `back`, `open:<screen>` and `reset` are built in.
- `confirm`, `note` and `confirm-if` ask in a dialog first.
- Any other `name:arg` goes to the screen's Lua script, or to the host as a typed output.
- A `<command key="..." label="..." action="..."/>` binds a key and shows its hint while the focus is inside its parent.

**Tabs**

- Tab keys follow the nesting depth: outer strips use TabPrev/TabNext, inner strips use SubPrev/SubNext.

**Scripting**

A screen with `script="name.lua"` gets a fresh sandboxed Lua state each time it opens. The script's `on_open(ui)` can:

- call `ui:get`/`ui:set` on the store;
- `ui:find`, `ui:create` and `node:append` nodes;
- register handlers with `ui:on("action:<name>", fn)`;
- `ui:emit` actions to the host.

The full table is in `src/gui/dom/lua.rs`.

### Strings and languages

The engine has no text tables: every string resolves through
`language/<tag>.txt` files packed by `convert-game` and merged per key across
mod layers. `language::Catalog` resolves the selected tag (the `--language`
flag, then the OS locale, then `en-US`), `charset` encodes the value into the
font's byte stream, and `text::Text` is the facade the message machine, the
menus and the overlays call. The format, the key registry, the fallback chain
and the mod story live in [`language.md`](language.md).

## The fixed 30 Hz tick and the frame data flow

The original runs at 30 Hz and so does the port: `TICK_MS = 1000/30` and the
interactive loop accumulates real time and runs whole ticks, latching one
input edge per tick. Rendering happens once per accumulated frame; a tick that
opens a door hands control to the transition player until it finishes.

While a door transition owns the screen the port steps at
`TRANSITION_TICK_MS = 16 ms` instead: the original clears its game-active flag
for the transition, which drops the frame limiter from the 33 ms gameplay
interval to the 16 ms one, and the door interpreter advances one animation
frame per platform frame. Gameplay pacing resumes when the transition ends.
The door animation's skip gate reads the confirm or cancel/run button's press
edge, not its held level, so the press that activated the door must be
released before another press can skip the animation.
Film skip input is likewise polled once per platform frame, not once per 30 Hz
tick. The reference leaves its frame limiter through the film gate while a
film owns the screen, so its 100-update grace expires within milliseconds; the
port's first poll latches the pad and any later unmasked press skips the film.
Films are paced by the wall clock, with the mixer's device-playback cursor
(everything written to the stream minus what is still queued) accepted only
within the original's -0.5 s/+0.25 s tolerance; the session decodes forward to
the frame due now and keeps a two second audio lead queued so the cursor never
starves.

The confirm button skips the loading narration before the first room and,
like Classic REbirth 1.1.4's hidden `CutSkip`, a letterboxed cutscene (bank-5
`MSF_SCREEN_INTENSITY`). The cutscene skip never jumps the script: it fades
to black over ten ticks with the scene still running, then runs the scene's
ticks back to back under a black frame (up to 12 ms of them per 30 Hz
gameplay tick) with the play timer held, until the letterbox flag drops, and
fades back in over ten ticks. The mixer is parked with the voice and
one-shots stopped; the music keeps streaming from it, with the scene's volume
and pan, while the scene's BGM loads, stops and restarts stay queued for the
first tick after the skip (a restart of a still-playing track is dropped, so
the music carries on). A line waiting on a press is pressed through, but a
timed line (a voiced line's subtitle) runs out on its own clock: scenes fork
their actors' animations onto events that pace themselves with sleeps, and
dropping the lines early would end the main event while the forks still hold
the room. A press while a scene's line waits on the player pages the line
instead, and a line the scene raises on its last tick stays up. A yes/no
prompt ends the skip with the fade-in for the player to answer; a door the
scene opens, a film, the death sequence or a modal screen ends it at once so
the player gets them, and a scene that carries on past its door can be
skipped again. Both skips are on only in the interactive loops, so scripted
`--input` and `--trace` runs play every scene out like the reference.

Whenever a room load completes, gameplay re-arms the
original's entry fade: a black veil at accumulator `0x7FFF` whose alpha steps
255, 207, ..., 15 over six fixed ticks (a slow `0xFF5D` counter while scenario
flag `0x7D` is set).

One gameplay tick (`engine::tick_room`) runs this fixed order:

1. Publish the remapped D-pad held/pressed words so `ck_bits` sees this
   frame's pad.
2. Advance the special-room-light state and clear the per-frame item-use
   flags.
3. Desk/item-box flow and deferred key consumption.
4. Command VM `run_main` over the current block, then the event VM step.
5. Apply scripted room edits (collision boxes, `obj_xfm` lights).
6. NPC/entity update (`game.tick_entities`).
7. Mirror scripts' player-entity writes onto the visible player, mirror the
   live health-status byte into the player's collision-callback byte, then run
   the player state machine: a scripted/held state runs `player_script::update`,
   otherwise the state-1 entry checks (`player_script::control_gate`: death,
   hit pre-emption, poison) run before `weapons::update` (the aim/fire/reload
   machine when a weapon is raised, otherwise the locomotion fall-through into
   `player::update_with_room`). States 2 and 5-7 are the original's hit and
   animation windows: the attacker-written `animFrameId` selects a handler and
   the reaction clips play from `GameState::player_damage`, the damage scratch
   pair repointed at the last loaded enemy model's second EDD chunk
   (`ClipSource::Damage`). Once an enemy writes the player's reaction
   (`GameState::player_damage_owner`), only that enemy's model repoints it,
   so a room can mix enemy types.
8. Effect pool update (`game.tick_effects`).
9. Room action probe and object update (probes read the frame's final state).
10. Camera cut switch and mask/shadow bookkeeping.
11. The game-over machine (`tick_death`): notice a dead player, raise the
    dead flag and advance the delay/fade; once the fade completes the DIED
    screen owns the tick and draws its own frame.

### Death frame order

The game-over machine runs at the end of the gameplay tick, after the room
tick noticed the health drop. Its trigger sets the 90-frame delay (or arms the
white fade at once in the two Yawn rooms and the water tank); the delay
expires into the same fade, which ramps 2 alpha a frame to white and opens the
DIED screen when the 16-bit accumulator wraps negative (~128 frames). The
attract-demo and scripted-death-variant flags skip the screen and return to
the title directly.

While the DIED screen owns the tick the room stays frozen; one screen tick
runs:

1. The display state machine: 0 arms the white flash (accumulator `0x7FFF`,
   counter `-384`), 1 waits for it to clear, 2 ramps the black overlay 3 a
   frame and, past `0x3F`, draws and steps the wave amplitude down from 59,
   3 holds the settled strip for 49 frames, 4 arms the black fade-out
   (counter `0x180`), 5 re-waves the strip while the fade runs and finishes.
2. The fade accumulator advances once per tick (the arm tick only draws).
3. The corpse's facing spins 8 units; the confirm button skips to the title.

The frame is painted back to front in the original's ordering-table order:
the `died.tim` static backdrop (four mirrored 160x120 quadrants of the page's
lower region, colour-keyed on black), the corpse (the player mesh posed at the
fall's terminal frame through the fixed camera built from the death position),
the black overlay, the 256-column sine strip (phase `0x100` per column,
amplitude `param^2`, baseline from the same amplitude), and the screen's own
white/black fade on top. The first death screen also hides the player's head
joint through the shared grab/death one-shot.

### Weapon frame order

`weapons::update` takes the tick whenever the aim input is held with a weapon
equipped and no locked action owns the player; otherwise it calls
`player::update_with_room` unchanged, so the locomotion machine is untouched.
While the machine owns the tick, one 30 Hz step runs:

1. The behavior dispatch on `WeaponState.behavior` (0x12 aim, 0x13 raise/hold,
   0x14 auto-aim fire, 0x15/0x16 quick-fire and reverse recover, 0x17 holster,
   0x18 reload, 0x19 empty click, 0x1a lock-on fire; the knife's 0x12 aim,
   0x13 hold, 0x14 swing, 0x15 holster, 0x16 turn). The handler advances the
   weapon `W*.EMW` clip through the same `AnimPlayer` clock as the body
   (`Joint_move`'s blend step 0x400/0x200/0x100 and its wrap-completion flag),
   selects the per-weapon motion id, and runs the reticle/lock-on scan
   (`player_reticle_enemy`, the all-quadrant aim-cone test) where the original
   does. The 0x13 hold-input handler also runs the optional Classic REbirth
   tactical reload check after the aim-held test and before the fire, empty-clip
   and lock-on branches: with `[game] tactical_reload`, a held RUN (not the
   press edge) sends a partial-clip reloadable gun (ids 2..=5, matching
   ammunition item held) to 0x18.
2. The fire cycle: the ammo-decrement/muzzle-billboard frame, the damage and
   sound frame (`apply_weapon_damage_with` for the slots the original damages
   at a fire frame), the big muzzle flash on the player matrix, the second
   flash at the hand joint and the shotgun's second damage frame. The reload
   cycle instead runs the per-weapon FX routine (volley latch, ejected shell,
   ammo transfer from the matching stack).
3. The shared tail: the posed hand joint's world matrix is stored into
   `joint_worlds[0]` so the effects pass resolves `Attach::Joint(0, 14)`, and
   the flamethrower refreshes the aim pitch byte the damage cone reads.

The room tick's own tail runs the beretta's detached magazine
(`weapons::update_detached_clip`, the original's `player_update_detached_joint`)
after the collision pass and before the render pass's joint-world store. The
reload's frame-0xa volley gate arms it (`fire_reset_joint15_recoil`); state 0
seats the clip on joint 14's world pose, state 1 applies gravity with the
dust/floor tests, and state 2 skitters the bounce until it drops back past the
floor and switches itself off. The player EMD's own 16th mesh object draws
through that world pose while `velZ` is set, and the whole-entity gate
(`zoneFlags & 0x7f`) pauses both the physics and the draw.

The weapon bank's `W*.EMW` clips pose the body (its skeleton is the player
EMD's) and the EMW's single mesh object draws at joint 14 with the player's
own texture page, exactly like the original's weapon-slot TMD; the scripted
characters' `WS*.TMD` held meshes draw the same way off their character
texture page. The fire data (fire frames, sound ids, FX offsets, end frames,
special frame windows, aim heights), the weapon-file table and the held-weapon
table live in `src/weapons.rs`. The converter applies both tables once, so the
runtime only formats entries: `player/<id>/weapon/<item>.emw` (item `00` is the
no-weapon locomotion set) and `enemy/<id>/weapon/<behavior>.tmd`, a missing
entry meaning nothing is held. Shared files are packed under every user and
share one blob.

The shared damage layer rides this order without adding a tick step. A fired
weapon calls `GameState::apply_weapon_damage_with` from the weapon machine's
fire frame (the tests use the same entry): the pipeline scans the active enemy
list in the original's slot order, runs the per-class detector (knife reach,
gun cone, projectile origin), applies the line-of-sight rule below weapon slot
5, looks the hit record up for the active playthrough, subtracts the damage
with a 16-bit wrap, composes `hit_state`, runs the post-hit callback (which
dispatches the per-enemy reaction FX), and writes the `state = 3` / surviving
`state = 2` contract with `ignore`/`action_behavior`/`action_state` zeroed. A
killing blow also raises the spawn record's `death_event_id` in the enemy
bank. Monster scripts consume states 2/3 on their next update and run their
own reaction/death handlers, exactly like every other piece of scripted
behaviour. A door transition snapshots the outgoing room's live enemies into
the 16-slot TTL table (keyed by room and spawn-type byte, force spawns
skipped, death events permanent) and clears the list before the destination
loads; `spawn_enemy` restores a matching snapshot when the record lacks the
force byte.

After the tick comes `render_frame`: the 320x240 `Framebuffer` is cleared with
the room's fade/ambient state, the cut background and masks are blitted, the
object/item/effect billboards are depth-sorted and drawn, the player and NPC
models are transformed and rasterised, and the shadow is composited. The
interactive path presents the framebuffer through SDL; the `--capture` path
runs the same renderer under the offscreen/software driver and writes a BMP
through an atomic sibling-temp rename; `--stats` performs the render and
prints timings without opening a display.

Headless seams (`engine::simulate_room*`, `simulate_door`,
`simulate_typewriter`, `render_game_frame`) boot the same state, run the same
tick and render the same frames as the interactive loop, with audio stubbed
and film requests drained. The real-asset tests and the full-game soak are
built entirely on those seams.

## Trust boundaries

Arklay's input class determines what the code may assume:

- **Pack and mod input is untrusted.** A pack can come from a third party; the
  reader validates the TOC with checked arithmetic, rejects unsafe paths
  (absolute, `..`, backslashes, NUL, empty, duplicates) and every parser is
  expected to reject arbitrary bytes with `Err`, never a panic, abort,
  unbounded allocation or unbounded loop. `arklay verify` classifies by path
  and runs every entry through its format's reader; the torture matrix and the
  cargo-fuzz targets hammer the same entry points. Budgets live in
  `src/budget.rs`: `MAX_PACK_ENTRIES`, `MAX_ENTRY_BYTES`,
  `MAX_DECODE_ALLOC`, `MAX_PIXELS`, `MAX_LZW_OUTPUT`, `MAX_RECORDS`, plus
  per-format caps; an over-cap input reports the format, the limit and the
  observed value.
- **The game installation is semi-trusted.** `convert-game` may assume the
  shipped file layout, but a missing or surprising file is a reported warning
  or a category error, never a panic; conversion skips what it does not know.
  The shared combat tables are the engine's built-ins; conversion never reads
  the executable, so a converted pack carries no `data/combat.bin`.
- **The packed combat tables are untrusted.** `combat::CombatTables::parse`
  checks the fixed blob size, magic and version against
  `MAX_COMBAT_TABLE_BYTES` and fails cleanly; a pack without the entry (or a
  mod's replacement) falls back to the built-in documented tables. The
  torture matrix and `fuzz/combat` hammer the parser.
- **Save blocks are semi-trusted.** The block is fixed 0x800 bytes; a short or
  malformed slot is refused, and a save never writes outside its save
  directory.
- **Lua is sandboxed.** `lua/**.lua` runs in one 16 MiB-limited VM with the
  documented hook API and no filesystem, package or `os` access; a failing
  hook logs and disables itself rather than failing the session. Enemy scripts
  (`enemy/{id:02x}/enemy.lua` plus shared `lib/**.lua`) run in a second,
  identical sandbox owned by the session; they must hold no state of their own,
  so resetting that VM between two updates cannot change behaviour. A screen
  script (the entry a pack registers with `api:register_save_screen`) runs in a
  third sandbox with the same limits; its only file access is the host's save
  API, which validates every name to one component under the save directory. A
  script that cannot load falls back to the ported original screen.

## Format inventory

Each row names the reader, the policy cap applied at the trust boundary and
where the format is exercised. Caps are policy numbers chosen from the shipped
corpus with headroom and recorded in `docs/performance.md`; they are not
claims about the original.

| Format | Entry point | Cap | Tests |
|---|---|---|---|
| `.akpak` pack | `pack::Pack::from_bytes` | `MAX_PACK_ENTRIES` (1M), `MAX_ENTRY_BYTES` (1 GiB, also the zstd decompression cap) | `pack` unit tests, `tests/torture`, `fuzz/pack` |
| `manifest.toml` | `manifest::Manifest::parse` | line/key/value length caps | `manifest` unit tests, `tests/torture`, `fuzz/manifest` |
| RDT room | `rdt::parse` | section counts bounded by remaining bytes; `MAX_RECORDS` | `rdt` unit tests, `tests/real/m16_game_real.rs`, `tests/soak_real.rs`, `tests/torture`, `fuzz/rdt` |
| SCD scripts | `scd::reader::parse` | instructions per block, `MAX_SCD_EVENTS` event-table cap | `scd` unit tests, `fuzz/scd` |
| SCD assembler | `scd::asm::assemble` | input size, line count, `MAX_ASM_OUTPUT` (64 MiB) | `scd` unit tests, `tests/cli.rs`, `fuzz/scd_asm` |
| RE2 SCD scripts | `scd2::reader::parse` | `MAX_SCD_INSTRUCTIONS` per procedure list; decompiler nesting capped (`MAX_NESTING`, `MAX_PARSE_NESTING`) | `scd2` unit tests, `tests/real/re2_real.rs` |
| `.dor` door | `door::parse` | bytecode bounded by input; `MAX_SCRIPTS` | `door` unit tests, `tests/torture`, `fuzz/dor`, `tests/real/doors_scan.rs` |
| TIM | `tim::decode`, `decode_8bpp`, `decode_4bpp` | `MAX_PIXELS` (4096x4096), CLUT entry cap, `budget::alloc` | `tim` unit tests, `tests/torture`, `fuzz/tim` |
| TMD | `tmd::parse` | objects/vertices/normals/primitives, `try_reserve_exact` | `tmd` unit tests, `fuzz/tmd` |
| EMD/EMW | `emd::parse`, `parse_emw`, `parse_room_anim` | skeletons/keyframes/clips | `emd` unit tests, `fuzz/emd` |
| RE2 EMD/PLD/PLW | `emd2::parse_emd`, `parse_pld`, `parse_plw` | chunk offsets, objects/vertices/keyframes bounded by input | `emd2` unit tests, `tests/real/re2_real.rs` |
| IVM | `ivm::parse` | objects/vertices/texture caps | `ivm` unit tests, `fuzz/ivm` |
| AVI | `avi::Avi::parse` | streams/chunks/frames bounded by the file size | `avi` unit tests, `fuzz/avi`, `tests/real/m14_real.rs` |
| Cinepak | `cinepak::Decoder::decode` | canvas `MAX_PIXELS`, 32 strips | `cinepak` unit tests, `fuzz/cinepak`, `tests/real/m14_real.rs` |
| MPEG-1 film | `mpeg::Film::open` | 4096 dimension cap, `MAX_PIXELS`, `budget::MAX_AVI_FRAMES` pictures | `mpeg` unit tests, `tests/real/re2_real.rs` |
| LZW | `lzw::decode` | `MAX_LZW_OUTPUT` (64 MiB), reset-before-full guard | `lzw` unit tests, `fuzz/lzw` |
| BMP | `bmp::decode`, `decode_mask` | dimensions bounded by input | `bmp` unit tests, `tests/torture`, `fuzz/bmp` |
| Save block | `save::SaveFile::from_bytes` | fixed 0x800 layout | `save` unit tests, `tests/real/save_real.rs`, `tests/soak_real.rs`, `fuzz/save` |
| Mask table | `mask::parse` | sprite/group counts | `mask` unit tests, `tests/real/mask_render_real.rs`, `fuzz/mask` |
| WAV | `audio::parse_wav` | sample count bounded by the `data` chunk | `audio` unit tests, `fuzz/wav`, `tests/real/m13_real.rs` |
| Lua source | `lua::LuaVm::load`, `lua::verify_source` | 16 MiB VM, source-size cap | `lua` unit tests, `fuzz/lua`, `arklay verify` |
| Map tables | `ui/map.lua` `parse_tables` (size/magic also checked by `arklay verify`) | fixed blob size/magic | `ui::lua_screen` screen tests |
| Combat tables | `combat::CombatTables::parse` | fixed blob size/magic/version, `MAX_COMBAT_TABLE_BYTES` (64 KiB) | `combat` unit tests, `tests/real/combat_real.rs`, `tests/torture`, `fuzz/combat` |
| BioCard prefix | `save::BIOCARD_PREFIX` (hardcoded) | fixed 0x200 constant | `save` unit tests |
| room `cut`/`mask` image | `assets::decode_picture` (`bmp`/`png`/`webp`/`tim`/`pix`) | BMP/TIM/PNG/WebP caps, `MAX_PIXELS` | `arklay verify`, `tests/soak_real.rs`, `tests/real/hd_real.rs` |
| PNG replacement | `assets::decode_image_bytes("png", ..)` | `MAX_PIXELS` (4096x4096), decoder allocation cap | `assets` unit tests, `tests/torture`, `fuzz/png` |
| WebP replacement | `assets::decode_image_bytes("webp", ..)` | `MAX_PIXELS`, decoder memory cap | `assets` unit tests, `tests/torture`, `fuzz/webp` |
| OGG Vorbis replacement | `assets::decode_audio_bytes("ogg", ..)` | `MAX_WAV_SAMPLES` (134M samples) | `assets` unit tests, `tests/torture`, `fuzz/ogg`, `tests/real/hd_real.rs` |

`arklay verify` maps entry paths to these readers, requires every RDT camera's
`room/<id>/cut/<camera>.bmp` to be present, and counts unknown entries as
*opaque*; `--strict` upgrades an unknown extension to a failure. The
status matrix in `docs/status.md` records the counts a fresh verify reports for
the shipped packs.

## The GPU renderer

The GPU renderer (the default; `--renderer software` or
`[graphics] renderer = "software"` opts out) draws the gameplay scene with
SDL_GPU. It is interactive only: captures, traces, `--stats`, the oracle and
every golden draw through the software rasterizer, which stays the reference.
Without a usable device (the shaders are SPIR-V, so it needs Vulkan) the
window warns and falls back to software; a failure while drawing or
presenting does the same mid-game.

- **Presentation.** `vsync` (on by default) selects SDL_GPU's FIFO present
  mode; off prefers IMMEDIATE, then MAILBOX, and the frame pacer sets the
  rate. The software backend applies the same option through
  `SDL_SetRenderVSync`.
- **Resolutions.** Every frame is the 320x240 one; only its presentation
  varies. The window's client area is `window_width` x `window_height`
  (F8 sets whole multiples of the presented frame), or the
  whole display in borderless fullscreen, and the frame fills the largest
  centred viewport of its shape in it (`video::fit`). The table below is
  what each stage draws at for a viewport of `W`x`H`:

  | Stage | Software | GPU |
  |---|---|---|
  | Backgrounds, masks, models, shadows, effects | 320x240 | `W`x`H` times `render_scale`%, `msaa` samples, resolved and filtered bilinearly to `W`x`H` |
  | Sprites, UI, messages, fades | 320x240 | the scene's resolution, each frame pixel's texel (the 2D pass) |
  | To the screen | the 320x240 frame scaled to `W`x`H` | the 2D pass's target scaled to `W`x`H` |

  **Aspect.** `[graphics] aspect` (`--aspect`, F1 DISPLAY) picks the
  presented shape; `"4:3"` is the default and the only one traces, the oracle
  and goldens use. A wider aspect crops at presentation and changes nothing
  that runs: the room, its masks, collision and cameras stay in the 320x240
  frame, and the window shows a 320-wide band of it (`video::View`, 180 rows at
  16:9). In a room the band follows the player's projected chest row with a
  dead zone and a few rows per tick, and snaps on a cut or room change
  (`video::follow`), cutscenes included: a wider aspect does not draw the
  cutscene letterbox bars, so the band has no reason to centre on them.
  Films and the loading narration show the centred band.
  The message window and the other overlays draw in the band
  (`Framebuffer::with_screen`), keeping their distance from its bottom. Lua
  screens lay out in the band, or ask for a full-height canvas of the aspect's
  width (427x240 at 16:9) that replaces the frame while they are up (the pause
  menu and its sub-screens). A door transition (drawn on black) presents the
  whole 4:3 frame, pillarboxed. Both backends accept the canvas. The GPU's 2D
  pass draws it at its own width, while the scene targets keep covering the
  4:3 frame. The F1 debug menu keeps the room's band and draws into a
  separate layer of the canvas's size (`GameSession::draw_ui_layer`): on the
  GPU a 2D pass drawn into a clear target; in software drawn over black and
  over white, the difference becoming a premultiplied alpha. Both backends
  blend the layer over the fitted rectangle after the frame (`ui.frag` on the
  GPU).

  A 320x240 frame reaches the viewport with nearest sampling at a whole
  zoom (always, windowed) and with sharp (pixel-art) sampling at a fractional
  one, so every source pixel keeps one width and only the screen pixel that
  straddles two blends them (`SDL_SCALEMODE_PIXELART` in software, the same
  formula in `composite.frag`). A capture never reads these options: it
  draws in software through a fixed 960x720 window, so captures and traces
  are the same at every setting.
- **What it draws.** `render::GameplayScene` is the one description of a
  frame both renderers consume. `gpu::scene` turns it into GPU input: a static
  vertex buffer per TMD (rigid skinning by a per-vertex joint index), per-draw
  uniforms (joint rows, the cut camera, the latched lights, tint and blend),
  and screen-space quads for the background, masks, shadows and effect
  billboards. The fragment shader looks the CLUT row up itself, with STP in
  the palette's alpha, so the semi-transparent texels of an ABE packet blend
  in a second pass.
- **Visibility.** A depth buffer replaces the painter's sort. Masks write
  depth at their painter's key (converted to the same view-Z depth), shadows
  and effects test against it, and opaque mesh texels write it.
- **Residency.** Uploads are keyed by the source's address plus a
  fingerprint of their full contents and released after 300 scene frames
  unused. Entering a new room retires the previous one: its first frame keeps
  what it draws (the player, shared pages) and everything else is released at
  once. Leaving gameplay (the title, a film, the ending screens) releases
  everything, and a recording that fails forgets the entries it created.
- **2D.** The backends are separate: under the GPU the framebuffer records
  its 2D primitives (sprites, glyphs, rects, fades, lines, backgrounds, image
  copies) as a list of items instead of writing pixels (`draw2d`,
  `Framebuffer::record`), and the scene is an item of the list where the
  frame shows it (`Framebuffer::show_scene`; the DIED screen's corpse is a
  scene with no background, so what the script drew under it shows). The
  GPU draws the items in order into a target at the scene's resolution. A
  sprite's texels are decoded into the frame's atlas and sampled per frame
  pixel with the software's integer nearest mapping, so at one target pixel
  per frame pixel the pass draws the software frame's pixels, its blends
  within one unit (`gpu::tests::gpu_2d_pass_matches_the_software_frame`). The
  pack's HD art replaces the items its original draw recorded, kept to their
  opaque texels, and the examine viewer's model renders into a clear target
  of its own. A frozen frame (the room under a menu) starts from the items
  drawn up to its mark. The software 3D rasterizer is never reached from the
  GPU backend: a scene the GPU fails to record or present switches the whole
  run to the software backend.
- **Known differences** from the software frame: per-pixel depth instead of
  per-packet sorting, hardware near-plane clipping instead of dropping a
  crossing packet, and float shading instead of the GTE's truncation. `engine::tests::gpu_scene_matches_software_in_real_rooms` bounds the
  difference on real rooms.

## Engine invariants

These hold across every milestone and are the regression gate:

- **Deterministic frames.** For a fixed pack, room, tick count and input the
  rendered frame is byte-identical across runs and platforms; the RNG streams,
  animation phases and camera state are all driven by the fixed tick, never by
  wall time.
- **Index-stable entity slots.** Entity slot 0 is the player; characters and
  monsters occupy their scripted slots and are never compacted. Every id the
  original dispatches (`0x00..=0x2E`) allocates; an id whose pack script or
  model is missing parks inert.
- **A fixed save layout.** `SaveFile` is the original 0x800-byte block field
  for field; capture, serialization, parsing and apply are round-trip stable
  (`tests/real/save_real.rs`, the soak).
- **No behaviour change without a golden update.** A change that alters a
  deterministic capture, state hash, save byte or pack byte for a valid input
  is a bug until a milestone explicitly re-baselines its golden set.
- **Untrusted input fails cleanly.** Every parser returns `Err` for arbitrary
  bytes; the torture matrix, the memory caps and the fuzz targets are the
  proof.

## Test taxonomy and commands

- Unit tests inside `src`: fast, synthetic. `cargo test --lib`.
- Always-run integration tests (`tests/*.rs`, unignored subset): synthetic
  packs, CLI spawning, torture matrix, stats plumbing, the synthetic
  transition + save round-trip. `cargo test`.
- Release-profile green run: `cargo test --profile test-release`. The
  `test-release` profile is `release` without thin LTO: the tests run at
  release speed, the link skips the pass only a shipped binary needs.
- Lean build (no `hd` decoders): `cargo check --no-default-features`.
- Documents: `cargo doc --no-deps`.
- Real-asset suite (ignored; needs both env vars): one binary,
  `tests/real/main.rs`, holding every real-asset module.
  `ARKLAY_RE1_ROOT=/path/to/re1 ARKLAY_RE1_PACK=/path/re1.akpak cargo test
  --profile test-release --test real -- --ignored` runs all of them; a name
  filter targets one module, e.g. `--test real stairs_real::`. The corpus
  audit, the performance budgets and the capture/movie/UI goldens live here.
- Full-game soak (ignored, release-only, opt-in): `ARKLAY_SOAK=1 cargo test
  --profile test-release --test soak_real -- --ignored --nocapture`. The soak
  keeps its own binary because its counting allocator measures process-wide
  live bytes, so it must run alone; without `ARKLAY_SOAK=1` it skips with a
  printed note. No routine pass runs it.
- Performance budgets (release): `cargo test --profile test-release --test
  perf_real -- --ignored --nocapture`; the test has its own binary so the
  other real-asset tests cannot take its cores. `ARKLAY_PERF_BUDGET_MS=<ms>`
  overrides the per-tick p95 budget.
- Fuzzing (nightly, separate package):
  `cd fuzz && cargo +nightly fuzz run <target> -- -max_total_time=300`.
  The stable compile gate is `cargo check --manifest-path fuzz/Cargo.toml`.
- The check tiers (`AGENTS.md`): `./scripts/check.sh` is the session tier
  (fmt, clippy, the unit and fast integration tests in `test-release`, five
  minutes cold) run once before pushing; `--ci` mirrors the Linux CI job;
  `--full` adds every ignored test, the real-asset suite and the asset-gated
  unit tests, when the asset variables are set; `--soak` adds the release
  soak. `--full` and `--soak` are release-cycle checks.

CI runs fmt, clippy `--all-targets --all-features -D warnings`, the debug and
`test-release` tests, the `--no-default-features` check, the doc build and the
Linux artifact smoke on Ubuntu; tests and the release artifact on Windows; and
a `cargo check --all-features` MSRV job on 1.88.0. The soak never runs in CI:
hosted runners have no game install, so it stays a release check on a machine
with the assets. A scheduled, non-gating fuzz workflow runs each target for
300 seconds, and the fuzz package's stable compile gate runs in CI and
`scripts/check.sh --ci`.
