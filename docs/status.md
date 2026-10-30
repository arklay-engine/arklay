# Arklay status

A milestone-by-milestone feature table, the parser/format coverage a fresh
`arklay verify` reports, the verification-spine checklist with what is still
missing, the known `TODO(parity)` sites and their dispositions, and the
completed enemy milestone's closing state. RE1 numbers below were measured on
2026-10-18 with the workspace install (`/path/to/re1`) and its converted pack;
the RE2 rows were measured against the retail RE2 install on 2026-10-28.

## Milestones

| Milestone | Scope | State |
|---|---|---|
| M0 First Light | Convert room 1000, decode cuts, display | done |
| M1 Whole-game conversion + music | All rooms/categories into a pack; BGM playback | done |
| M2 Player | Player model, locomotion, collision, camera cuts | done |
| M3 SCD reading | SCD reader, VM skeletons, `scd export` | done |
| M4 SCD semantics | Full command/event opcode semantics | done |
| M5 Doors, masks, footsteps | `.dor` transitions, room masks, footstep zones | done |
| M6 Game flow and UI | Fonts, messages, inventory, menus, save/load | done |
| M9 Scripted characters | NPC models, animation, escort/walk behaviours (no enemies) | done |
| M10 Effects | Billboards, effect pool, sprite sheets, fire handler | done |
| M11 World interactions | Pushables, mirrors, item-box lid, ladders | done |
| M12 World items | Floor items, sparkles, desks, item-view models | done |
| M13 Voice and audio | Dialogue, per-room BGM state machine, 3D SE dispatch | done |
| M14 FMV | AVI/Cinepak, `movie_on` hand-off, opening/ending flow | done |
| M15 Mods and authoring | Manifest, layered packs, text-SCD assembler, Lua hooks, demo pack | done |
| M16 Fidelity sweep | Exact fixed-point presentation, scripting/UI parity, harness leftovers | done |
| M17 Hardening | Budgets, torture, fuzz, `verify`, atomic CLI, `--stats`, soak, CI, docs | done |
| E1 Enemy scripting | `enemy` namespace, sandboxed enemy-script VM from the pack, monster spawn/model dispatch, SpiderWeb (0x13) | done |
| E2 NPC port | Character states 0/1/8/9 in `lib/npc.lua` + per-id data scripts; native driver kept as the differential oracle | done |
| E3 Roots + shared helpers | Plant 42's roots (0x0e) spawn-kind/phase machine; named scratch words, joint-scale render, per-entity SCA record, enemy sound bank, additive model tint, direct RNG draw | done |
| E4 Shared damage layer | The shared weapon-damage pipeline with hit detectors, hit records and reactions over the built-in combat tables; player-hurt helpers; saved-enemy door snapshots | done |
| E5 Adder + Wasp | The adder (0x0a): four-state machine over the four spawn-kind decisions and seven behaviours, the ballistic drop, the joint-1 bite reach, the tail-hiding flinch and both death paths with the retreat's self-respawn. The wasp (0x07): the nest/ignore/behaviour three-level machine, hover drift, contact sting and cutscene grab, the per-character player poses, the big-wasp variant and the nest respawn cycle. Per-frame joint world matrices and joint-anchored billboards, the range/LOS and path-keep helpers they need | done |
| E6 Crow + spiders | The crow (0x05): the 14-behaviour flight machine, perching/take-off scatter, the altitude envelope and off-map death, obstacle swerve/latch steering, the floor-step channel, the stored-speed vector and the player animation/mash accessors. The WebSpinner (0x03) and BlackTiger (0x04): the three spawn-kind pickers and twelve behaviours, the three web shooters, the shared Rust web-clone arena with its own seven-state machine, room teardown and render pass, the two-volume SCA profile, 32-bit joint flags, the leg-reach probe and the per-type web-joint registries | done |
| E7 Cerberus + Chimera | The hound (0x02): the 16-slot state/behaviour block read through two bases, the five sub-tables and the wall-follow switch, all ten behaviours, the forward/turn collision probes with the `position`-word save/restore, the head-track steering and the bite→tumble/maul forks. The chimera (0x09): the four-state table with the hit/death layers, the seventeen behaviours (three doubling as the variant brains), the dive-arc drop/swoop/knockdown/death drivers, the ceiling attachment and the grab with its mash meter | done |
| E8 Zombies | The zombies (ids `0x00` standard, `0x01` naked, `0x11` green): the five overlapping dispatch tables with their exact alias bases, the wander/chase/eating behaviours, the pushback and get-up machines, the eleven-sub-state grab/bite/vomit attack, the four death variants with the corpse hold and the 1-in-4 revival, the severed-limb and head-explosion gore state contract, and the shared `lib/zombie.lua` driver behind three thin per-id scripts carrying the collision records | done |
| E9 Hunter | The hunter (id `0x06`): the nine-slot state table, the eight-variant AI layer (the plain/pack-follower/pouncer/coward/scripted-walk-in decisions), the twelve AI behaviours, the nine-slot action layer, the seven death paths, the 39-slot script-controlled table with its four self-re-entrant sub-range wrappers, the pack partner slot, the leap/dodge/scream/grab chains and the exact direct-RNG order | done |
| E10a Set-piece smalls | The monster plant (id `0x0f`): the five overlapping tables (state/behaviour/variant/sub/step) with the three step-chain entry bases, the head-joint SCA retarget, the full grab→drain→release chain over the shared player-held window, the bite and strike chains with both vine stop tables, the exact draw order of the die thrash and poison pulse, and the room-shared lunge-side register. The computer arms (ids `0x14`/`0x15`): the terminal command protocol, the 16.16 fixed-point integration, the per-arm reach/step/command chains, the voice handshake and the documented command-8 fall-off. The Forest zombie (`0x16`, absent from this install) stays a documented inert stub. The player-held reaction framework, `e:srand`/`e:lfsr_step`, `e:play_voice`/`e:play_sfx`, `e:set_sca_hit_point`, `e:raise_flag` and the boss scratch views land with it; Neptune, Yawn and the Tyrant stay pending | done |
| E10b Plant 42 | Plant 42 (id `0x08`): the state/action machine (init, state check, damaged, die, the SCD table), the sixteen action handlers with the sweep/spit/grab-hold-release-kill/wither chains, the range-gate selector, the ambient room effects and the shared vine pool with the last-vine award. The custom skeleton animator (`e:plant42_advance`: sub-frame blend, blend-before-use, hold-gated frame advance), the companion arena (the flower body and root ball clones with their own machines, render objects and scale, the room-shared body every split-vine record reads) and the room-40C0 set-ups (fallen core, poison arena, Chris hold, boss fight, poison body) land with it. The state-6 attacked-flag window (knockdown / grabbed / thrown, the capture-matrix slide) and the state-2 generic hit reaction join the player-held framework; `e:plant42_capture_setup`/`e:plant42_hold_player`, `e:apply_matrix_lv`, `e:vector_normal`, `e:set_player_pos`/`e:set_player_speed`/`e:add_player_speed` and the `p42_*` scratch views are the new surface. Neptune, Yawn and the two Tyrants remain for the later E10 groups | done |
| E10c Yawn | Yawn (ids `0x0d` first fight, `0x12` rematch): the five-state head machine (init, state check, damaged, die, wait), the nine actions (idle, move, bite with the per-form damage window and the first-fight poison, rear/form toggle, the swallow grab with its capture matrix and player window, the scripted entry crawl, the emerge/hiss ceiling burst, the flee path with its three segment batches, the wall-stuck reposition), the two decision trees and the exact direct-RNG order. The head's own fixed-point chain animator (pose init, `<<9` chain accumulation, the joint-13 anchor, the `reverse` swallow playback, chain follow/align and the post-move collision drag), the thirteen-slot segment machinery (`e:yawn_spawn_segments`/`e:yawn_segment_tick` with cross-slot damage routing and cleanup), the cross-slot scalar accessors, the swallow capture/player-held state-7 window and the segment render pass (mesh skip, body shadows, `& 0x7f` gate) land with it; `enemy/12/enemy.lua` is the thin alias. Neptune and the two Tyrants remain for the later E10 groups | done |
| E10d Tyrant | The Tyrant (ids `0x0c` lab, `0x10` heliport; one shared `enemy/0c/enemy.lua` machine plus the thin `enemy/10/enemy.lua` alias): the ten-slot state table (init, think+act, the hit reaction's `+0x174` state restore, the forced state and the SCD state), the two behaviour dispatch bases (state 1 adds the +2 bias, state 3 runs it unbent with no pathfinding), the sixteen behaviours (restrained slab, SCD yield, 60-frame pause, waypoint walk with the walk/run root-motion sets, swipe/slash/thrust/backhand with their reach windows, damage frames and kill latches, the impale grab, charge and rush with their save/restore collision probes, the eruption entrance, rise, and the two per-id decision layers) and the eighteen-slot SCD table (pod → glass break with its 53-entry shard draw order → the scripted Wesker impale → idle, the walk/strike/face/animation handlers, and the heliport rocket death that launches five joints as free bodies and stops at `health = -1` / `SysFlags 0x1F`). The root-motion extractor (`e:tyrant_root_motion`) turns the claw chains into the frame's speed and slides the entity; the ribbon history (`e:tyrant_trail_alloc`/`e:tyrant_trail_arm`/`e:tyrant_trail_update`, the nine matrix/midpoint/near-far slots with the `0x8000` arm bit), the two claw ghosts (`e:tyrant_ghosts_tick`, the signed-byte 3000..6000 scale pair) and the limb physics (`e:tyrant_limb_launch`/`e:tyrant_limb_update`) keep the original's exact state; the exposed heart rides joint 1 through the companion arena with its own beat ramp and rocket-death drop. The player-side Tyrant stagger table (backhand/swipe stagger and the knock-back with the wall slam and reverse get-up) runs in the shared state-6 window; `e:latch_player_attacker`, `e:snap_grab_slot`, `e:add_entity_anim_offset`, the `ty_*` scratch views, `e:angle_quadrant`, `e:store_camera_speed` and the travelled-distance third return of `e:resolve_collision` are the new surface. The corpus census now finds twenty-one scripted monster ids (adding `0x0c` and `0x10`) with no script errors, and the real-asset tests boot ROOM5130's pod Tyrant with its heart and ROOM3030's suspended heliport Tyrant | done |
| E11a Player weapons | The player weapon runtime: the aim/fire/reload state machine (0x12–0x1a guns, the knife's own aim/hold/swing/holster/turn) driven by the new aim/fire pad keys, the per-weapon `W*.EMW` clip selection and the knife/gun/projectile fire paths through `apply_weapon_damage_with`, the clip/reserve ammo model (`weapon_autoaim_check`, the reload transfer and the volley latch), the fire data tables (fire frames, sounds, muzzle and secondary flash offsets, end frames, special frame windows, aim heights), the held weapon drawn at joint 14 with the player/character texture page, the muzzle/secondary flashes and shells attached to the weapon joints, the state-8 held-weapon joint refresh, and the flamethrower enemy-bank cues | done |
| E11b Player death | The player damage/death state machine: the state-1 entry checks (the HP<0 death trigger with the back-shot facing flip and the cannot-die guard, the hit pre-emption into the generic reaction, the poison status drain), the death fall (state 3: the scream, the body clip, the slide, the blood pool that reuses the ground quad and grows 16 a frame for 148 frames, and the blocked state), the game-over machine (the 90-frame delay, the room-variant immediate fades for the two Yawn rooms and the water tank, the scripted-death rooms that skip the pool, the white-out and the black fade-out), and the DIED screen (`src/ui/death.rs`: the `died.tim` mirrored static backdrop, the fixed camera over the corpse with the 8-unit-per-frame spin, the 256-column sine-wavy strip with its exact phase/amplitude ramps, the black overlay ramp and the confirm skip), returning to the title; `ui/died.tim` is packed and `--ui death` joins the debug boot list | done |
| E12 Enemy verification | The closing gate over the whole arc: the corpus spawn census re-measured across all 348 rooms with twenty-one scripted monster ids and zero script errors, the per-id real-asset boots (`enemy_real`), the real weapon/damage kills (`weapons_real`/`combat_real`), the shipped DIED-screen run (`death_real`), the NPC/actor/walk/entities/fire regression suites, `verify` on the reconverted pack (the new `emw`/`tmd` rows green, `tim` 124/124), and the status/deviations close-out | done |
| M19 Config and Lua screens | The `arklay.toml` user config and the `<config dir>/save` default location; the general Lua screen surface (`on_init` + `api:register_save_screen`, the save list, save/load/remove, the pad edges, the screen sounds and immediate-mode drawing) and the Classic REbirth-style save/load screen as the `mods/crloadsave` mod | done |
| R0 RE2 data | `convert-game` for the RE2 PC release: rooms, backgrounds, models, audio and films into `re2.akpak`; per-file format detection replaces the pack-wide dialect; `verify` classifies the RE2 formats | done |
| R1 RE2 first light | RE2 rooms: the `rdt2` reader (cameras, RVD switch zones, SCA collision), letter stages A-G, the base manifest's `start_room`, the collision debug overlay, and the TeamX RE2 HD backgrounds | done |
| R2 RE2 models | RE2 EMD, PLD and PLW through the shared model and animation path, the `--model` viewer, per-camera LIT lights, HD model skins, and the `re2` pack conventions | done |
| R3 RE2 scripts | The RE2 SCD reader (`scd2`): init and main procedure lists, the RE2 opcode table, `scd export` to `.s` and structured `.akscr`, both round-tripping every retail room byte for byte | done |
| R4 RE2 doors and masks | The DO2 parser and door VM (`door2`) played through the door transition (`--door`), RE2 mask tables and mask pages (`convert-game` packs `room/<id>/mask/<nn>.bmp`), their TeamX HD names, the FLR footstep zones, and `verify` parsing DO2s | done |
| R5 RE2 audio | MS-ADPCM decoding (`adpcm`), matching ffmpeg up to its rounding, the door, arms, room, enemy and core `.sap` banks split into per-slot WAVs with their EDH sound tables, the RE2 SE dispatch (`se2`), and the room music table (`bgm2`) | done |
| R6 RE2 FMV | The in-house MPEG-1 decoder (`mpeg`: program stream demux, MPEG-1 video, MP2 audio) behind the movie path, RE2 films played by name (`--fmv opn1stl`, `r109`, `staff`), and `verify` parsing MPEG; all 26 retail films play end to end with ffprobe's frame counts | done |

The other RE2 milestones (R7 onwards) are still open and tracked by the
milestone issue #244 and its sub-issues.

Enemy behaviour now lives in the pack as `enemy/{id:02x}/enemy.lua` scripts (with
shared `lib/**.lua` modules) behind the same sandbox as the mod hooks;
the scripts are stateless and all their mutable state sits on the Rust entity.
The first ported monster is the spider web (id `0x13`), which validates spawn,
model, script load/reset, damage reaction and the death flag end to end. The
scripted characters (`0x20..=0x2E`) now run their whole driver from
`lib/npc.lua` with per-id data scripts: the state-0 init, state-1 idle
behaviours, state-8 scripted-action handlers (weapon fire, scripted turns and
walks) and the state-9 follow/pathfind driver, each checked against the native
oracle field for field, tick for tick. The native driver stays as the fallback
and differential oracle. The second monster is Plant 42's root mass (id
`0x0e`), whose `enemy/0e/enemy.lua` transcribes the dormant, retracted and full
writhe spawn kinds, the four phase handlers that double as state-table slots,
the exact groan-cue timer draw order and the shrink/tint/shadow cadence; E3
also paid the shared helper backlog it needed: named Rust accessors for the
scratch words that sit over adjacent bytes and the stored-position dword, the
`pad_ca` joint world-matrix scale wired into the renderer, a per-entity SCA
record consumed by the shared separation pass, the per-room enemy sound bank
behind `e:play_enemy_sound`/`e:play_3d_sound`, the additive model tint with
its mesh hook, and `e:random()` for the monsters that consume the platform
stream directly. The roots' damage reactions (the state-2 reset and the
inert states 3-5) are transcribed; the combat layer that enters them landed
with E4. Every monster id the corpus spawns is now scripted — the full
roster, the player weapons and the death/game-over flow are summarised in the
milestone table and the per-milestone notes below. The attract demo plays the
recorded reels from the title's idle timer, and the ending screens and
next-cycle save run end to end under `--ending`, though normal play cannot
reach them yet (see below). The DC-only Forest zombie and the documented
render-contract items remain deferred; every one is listed below or in the
milestone deviations files.

E4 added the shared damage/death layer those scripts consume. The combat
tables — the per-weapon hit ranges, both playthroughs' hit records, the
hit-joint lists, the per-type hit reactions, the reference type's init
profile and collision records, the shared player collision records and the
entity-model path table — are the engine's built-in documented values; a pack
that carries its own `data/combat.bin` (an older conversion or a mod)
overrides them.
`GameState::apply_weapon_damage` runs the original's per-shot pipeline in
slot order (knife/gun/projectile detectors, the two arcade cones, the
line-of-sight layers below weapon slot 5, first/second-playthrough record
selection, the 16-bit health wrap, the `hit_state` composition, the per-weapon
post-hit callback and per-type reaction FX) and writes the `state = 3` /
surviving `state = 2` contract with the action bytes zeroed; a killing blow
raises the spawn record's death flag. The player-side helpers
(`apply_player_hurt` with the clamp/lethal rules, the poison status/timer, the
grab pose) are exposed as `e:hurt_player`, `e:poison_player`,
`e:grab_player`, the player read/write properties and the animation override
methods. Door transitions now snapshot the outgoing room's live enemies into
the 16-slot TTL table and restore a matching snapshot in `spawn_enemy` when
the record lacks the force byte, so enemies left mid-fight persist across a
door and death events stay permanent. The declared joint-attached gore/tints
and the knife-hand joint origin are documented approximations until the
joint-matrix layer lands (see the module docs in `src/combat.rs`).

E5 ports the adder (id `0x0a`) as `enemy/0a/enemy.lua`. The script transcribes the
four-entry state table, the four spawn-kind decisions (floor, ceiling-near,
ceiling ambush with its pose/fuse trigger and 100-unit teleport, hidden) and
the seven behaviours (chase with its per-frame wobble draws, the quadratic
ceiling drop, turn away, the joint-1 bite with its frame-0xB hit window,
victory, idle, emerge), plus the flinch and the two death paths: the knife
writhe that ends in the blood pool and the death flag, and the firearm retreat
that sprays blood, hides every joint, parks out of sight and resets to state 0
with a fresh health roll and a ceiling spawn kind (kind 1 in the water-gate
room, 2 elsewhere). The shared helpers it needed landed with it: per-frame
joint world matrices composed from the posed EMD skeleton with the full
rotation triple and the `pad_ca` scale, exposed as `e:joint_world_x/y/z` and
the square `e:reach_test`; joint-anchored billboards through a new
`Attach::Joint` effect target; the flat visual/alert range checks and the
facing test; the path-result keep byte; the room-collision result code behind
`e:resolve_collision`; the player-pair touch flag from `e:separate`; and the
cross-enemy special-weapon check.

E5 also ports the wasp (id `0x07`) as `enemy/07/enemy.lua`, the last of the four
simplest monsters. The script transcribes the four-entry state table and the
three-level control (the `ignore` drift gate, the nest state machine encoded in
`behavior_flags`, and the seven-entry behaviour table with `action_state`
sub-states): the dormant nest's dwell roll and proximity arm, the 24-frame
takeoff spiral, the hover cruise (the one-in-seven wing blink and wingbeat cue,
the 0x23..0x96 bob ramp, two randomised altitude flips a frame and the
touch/range/aim/altitude attack gate), the contact sting with its
per-playthrough damage and the big wasp's scripted kill, the victory circle,
the pin-and-sting grab (per-character clip and hit frame, the poison coin, the
self-killing payout) and the emerge. Death forks on the weapon in `hit_state`:
a big or heavy-weapon kill is removed at once, anything else leaves a corpse
that crawls 20 frames and respawns at the fixed nest coordinates, every tenth
respawn as the big variant. The helpers it added: the wasp's typed scratch
views (`e.wasp_drift`/`touch`/`climb`/`timer`/`bob`/`distance`/`collision`/
`speed`/`dwell`/`anim_done`/`big`/`respawns`/`sound_latch`), the +0x16E
path-result keep behind `e:wasp_path_keep`/`e.wasp_path_bit`,
`e:apply_anim_vertex` for the grab's root-motion snap, `e:player_facing_entity`
and the player-side `e.player_move_speed`/`e.second_playthrough` reads. The
corpus census now finds four scripted monster ids (`0x07`, `0x0a`, `0x0e`,
`0x13`) and no script errors, and the real-asset tests boot the seven ceiling
adders of ROOM3010 and the seven dormant wasp nests of ROOM4080 through init.

E6 ports the crow (id `0x05`) as `enemy/05/enemy.lua`. The script transcribes the
nine-entry state table (init, driver, recoil, death and the five SCD-flight
slots), the four anim helpers (hold, step, banking turn, grab) and all
fourteen flight behaviours: the perched countdown and its three idle clips,
the wingbeat's climb/sink arc, the dive and its rolled ceiling bound, the
random-rate banking turn, the strike run and dive attack with their
distance-scaled climb velocities, the descending glide and climb-out (both
steered by the pathfinder bit and handing to the peck at head height), the
approach, the peck that bites through the shared player helpers and latches
on, the level-out, the grab with its signed mash-scaled struggle counter and
180-degree release, the victory descent and the staggered take-off. Death
forks on the room resolve's floor step: real floor fades the shadow yellow
and plays the landing, an off-map step bursts every joint and removes the
entity. The helpers the crow needed landed with it: the stored `Add_speedXZ`
velocity with `e:advance_speed`, the `entity_swerve_around_obstacle`
steering behind `e:swerve` with its quadrant fix-up and latch countdown, the
floor/step branch of the room collision (returned as the second value of
`e:resolve_collision`), the crow's typed scratch views and its own
swerve/latch/struggle/alt-bias/floor-step fields, and the player's animation
id/frame reads plus `e:player_mashing()` for the grab shake. The corpus
census now finds five scripted monster ids (`0x05`, `0x07`, `0x0a`, `0x0e`,
`0x13`) and no script errors.

E6 also ports the WebSpinner (id `0x03`) as `enemy/03/enemy.lua` and the Black
Tiger (id `0x04`) as `enemy/04/enemy.lua`, closing the milestone. The WebSpinner
transcribes the ten-slot state table (six states, the last four aliasing the
three spawn-kind behaviour pickers), the twelve behaviours (the two idle
speeds, three walks, the strafe, two approaches, the lunge with its direct
damage and facing reaction, the spit, the ceiling web drop, the no-op and the
circle) and both web shooters; the Black Tiger the four-slot state table, its
single picker with the post-attack cooldown and chase counter, the seven
behaviours (idle, walk, strafe, approach, circle, the two-fang bite and the
three-glob spit) and its three shooters. Both scripts drive the shared
web-clone subsystem: a Rust arena of full entity copies outside the enemy
list, spawned with a quarter-turn fan per thread, walked by their own
seven-state machine (swing, ballistic flight, sticky contact, dangle), draining
the player's health on contact, resolved against the room with the spider's
inherited SCA radius and reset with the room. The clone render pass draws the
live threads after the player with the frozen spawn pose, and spent threads
keep their web-coloured ground shadow. The original draws each thread as a 2D
sprite of the spider's joint-bound web object; the port submits the spider's
whole mesh at the clone's transform instead (the thread art cannot be
isolated from the model), so a thread reads as a small spider rather than a
strand until that object split lands. The helpers the spiders needed landed
with them: 32-bit joint visibility with `e:set_joint_visible` and the
minimal `e:arm_joint_effect`, the second SCA volume behind `e:set_sca2` and
the nested volume-pair separation, the `e:leg_reach` stride probe (both chain
shapes) over the same-call posed skeleton, the per-type web-joint registries
in the game state, the clone chain head on the entity, the ballistic step, and
the spiders' typed scratch views. The corpus census now finds seven scripted
monster ids (`0x03`, `0x04`, `0x05`, `0x07`, `0x0a`, `0x0e`, `0x13`) and no
script errors, and the real-asset tests boot ROOM30C0's Black Tiger and
ROOM4040's two WebSpinners through their scripts.

E8 ports the zombies (ids `0x00`, `0x01`, `0x11`) as `lib/zombie.lua`
with three thin per-id scripts (`enemy/00/enemy.lua`, `enemy/01/enemy.lua`, `enemy/11/enemy.lua`) that carry
only the variant's collision record. The shared driver transcribes the five
overlapping dispatch tables with their exact alias bases — the 22-slot states
block read through the behaviour base `states + 10`, the 28-slot
damage/action/move block read through `+0`, `+8` and `+16`, and the separate
14-slot SCD action table — so the aliasing is the original's one array, not
copies with matching values. It carries the full machine: the two spawn rolls
(health `17..99` from the table and the frame-seed threshold/stagger budgets),
the dead/floor/vomit spawn kinds, the wander/chase/eating behaviours with the
weave draws and the wander-turn stuck latch, the six damage reactions (the
short push back with its one-in-eight arm severing, the two staggers, the
knock-down, the leg explosion that crawls away or dies, and the long shove),
the eight action behaviours (idle head turns, slow walk, chase walk, fast
facing, bend-down eating, the falling lunge, vomit, falldown), the pushed-back
idle/stagger pair with the leg-chain crawl speed, the eleven-sub-state attack
(the grab anchor, the 105-frame bite loop with its mash shortening and
per-playthrough damage, the release's withdraw/head-bite/vomit forks, the
head explosion and headless death), the four death variants (plain fall with
the one-in-four revival, headshot, prone lay-down, magnum pushback), the
corpse hold and parking, and the scripted roar/headshot/dying/vomit handlers.
The gore state contract is complete: the joint flag bytes (active bit, the
`0x28` arm/explosion, the `0x0C` severed arm, the `0x88` vomiting head), the
`0xCC`/`0x40` head tests, the blood scratch the vomit arms and the
`blood_splatter_physics` fall with its wall/floor billboards and impact cue
all run; the per-joint colour tints and the async joint-object chunks stay
with the renderer's documented deferral, and the Director's Cut-only branches
(third SCA record, double-step nibbles, behaviour 11) stay dormant with no DC
mode in the port. The helpers the zombies needed landed with them: the state
dword/mirror aliases and the typed `+0x170`..`+0x188` scratch views, the
`checkAngularViewAndDistance` wedge and `check_line_of_sight` ray, the
`entity_update_wander_turn` out-parameter steering, the entity
`check_room_collision_two_point` prone probe with its per-end bits and
rollback, the radius-zero boundary point query, the per-joint blood scratch
and `zombie_body_part_physics` leg stride, the player-state/attack-word
accessors and the mash reduction. The corpus census now finds ten scripted
monster ids (`0x00`, `0x01`, `0x03`, `0x04`, `0x05`, `0x07`, `0x0a`, `0x0e`,
`0x11`, `0x13`) and no script errors, and the real-asset tests boot the 1F
dining room's zombies through their scripts.

E9 ports the hunter (id `0x06`) as `enemy/06/enemy.lua`, the last of the common
roster monsters. The script transcribes the three stacked dispatch layers: the
nine-slot state table (spawn, AI driver, action layer, death, a bare return and
the script-controlled state the spawn's control bit forces), the twelve-entry
AI behaviour table, the nine-entry action table and the seven-entry death
table. The AI layer carries the eight-variant decision block (the plain
hunter's one-frame kind rewrite, the pack follower's scripted formation
mirror, the health-gated pouncer with its low/mid pounce-chance tables and the
wounded-player gate, the walk-in variants that drop their kind bit, and the
coward that clears its own bit), the shared leap roll (the plain flavour reads
the frame seed, the pouncer flavour draws the stream), the path-latch/re-pause
driver and all twelve behaviours: the variant-selected stand/walk idle, the
chase with its wander steering and footfall cues, the 90-frame approach, the
nop, the swing, the aimed pounce, the dodge, the four-sub-state hold with its
head-joint tracker, the formation jump-down, the scream with the one-per-room
howl latch, the sidestep with its signed direction byte, and the scripted
walk-in with its stage/room/camera row selection, per-frame shuffle, mid-walk
howl and entry-yaw snap. The action layer seeds from the hit-state roll (with
the pending-grab cancel and the status/flag overrides), runs the swipe with
its joint-9 reach window and per-playthrough damage, the lunge, the
player-dragging recovery and the chain re-roll; the leap attack with its
ballistic arc, randomised hover, flurry re-roll, dive drag and leap-flag
pounce hand-over; the pounce chain with its three charge rates and swipe
re-arm; the claw flurry with its per-frame arm billboard; and the
player-holding loop. The death layer has the shared fall driver (the health
pin, the corpse-quad tint and shrink, the death event and the script release),
the standard fall with the howl and second cry, the thrash, the settle, the
collapse and the mid-pounce crash. The script-controlled table is spelled once
with all 39 slots, including the four wrappers that re-enter the same array at
the dodge/pounce/bite/swipe sub-range bases indexed by `action_state`, plus
the walk/run waypoint handlers, the scripted death with its target tracking,
the stalk window and the scripted bite that latches the enemy-list base
entity. The pack partner slot is picked at init (the enemy-list head, or the
next slot when the hunter is the head) and read through named partner
accessors. The helpers the hunter needed landed with it: the named `+0xC2`
through `+0x18A` scratch views and the partner slot, the hunter-flavoured
wander-turn steering over its own control byte and path-latch counter, the
partner and cross-slot joint/state accessors, the mouth-joint tracking and
body-recenter helpers over the posed skeleton, the player drag and the grab
one-shot, the shared `player_distance_z` scratch the line-of-sight helper
writes into mid-frame, the per-room howl latch, the intro row and the
outgoing-room camera id the same-stage door load stores. The corpus census now
finds thirteen scripted monster ids (`0x00`..`0x07`, `0x09`, `0x0a`, `0x0e`,
`0x11`, `0x13`) and no script errors, and the real-asset tests boot ROOM30A0's
scripted hunter pair through their scripts.

E10a opens the boss roster with the two smallest set pieces. The monster plant
(id `0x0f`) is `enemy/0f/enemy.lua`: the five overlapping tables are spelled once
(the five-entry state table, the seven-entry behaviour table indexed by the
`ignore` byte, the ten-entry variant table, the four-entry per-variant
approach table with its genuine NULL slot guarded, and the twelve-case step
chain entered at three bases), the grab/bite/strike chains run the drain over
the new player-held window (the plant writes player state 6 with window index
`0x0F`, the Rust window advances the grab clip and the drain flags the release
that spins a back grab and returns control), the vine reveal/hide tables walk
the fifteen segment bits exactly (including the spent cursor wrapping the
timer), and the die thrash's discard-first four-draw burst and the poison
pulse's two-timer tint ramp keep the original's draw order. The computer arms
(ids `0x14`/`0x15`) are `enemy/14/enemy.lua`/`enemy/15/enemy.lua`: the terminal's
`behavior_flags` command protocol (parked, new-command and done bits), the
16.16 fixed-point integration with truncating division and arithmetic shifts,
the per-arm reach offsets and step chains, the right arm's voice handshake
(queued through the new `e:play_voice` and polled through `e.voice_playing`),
the left arm's success gesture, lever pull and hanging drop, and the
documented command-8 fall-off that re-runs reach A forever on the right arm.
The Forest zombie (`0x16`) stays a documented stub: this install ships no
`0x16` spawn records and no model, so a modded spawn parks inert with the
missing-model warning rather than a fabricated script. The shared pieces the
group landed are the player-held window in `player_script.rs`, the per-frame
`e:set_sca_hit_point` SCA retarget (the separation pass now adds a world-space
bias to the primary volume), the `e:srand`/`e:lfsr_step` randomness specials,
`e:raise_flag`, `e:play_sfx`, `e:play_voice`/`e.voice_playing`,
`GameState.monster_plant_sides` and the typed `mp_*`/`arm_*` scratch views
over the shared bytes. The corpus census now finds sixteen scripted monster
ids (adding `0x0f`, `0x14` and `0x15`) with no script errors, and the
real-asset tests boot ROOM10C0's six-vine bed and ROOM5060's parked arm pair
through their scripts. Plant 42, Neptune, Yawn and the two Tyrants remain for
the later E10 groups.

E10b is Plant 42 (id `0x08`), `enemy/08/enemy.lua` over three new Rust pieces. The
script spells the state dispatch (init / state check / damaged / die / the
separate SCD table), the sixteen action handlers, the two range-gate selectors
and the ambient effects exactly as the original, and drives the companion
clones and the player-hold window through named methods. The custom skeleton
animator (`enemy/custom_anim.rs`) is the plant's own clock: it always poses,
sub-frame blends `(0x1000 / blendStep) / (timing_control + 1)` once the blend
counter has run out, consumes the counter before use and advances the frame
only when the hold expires; it also carries the per-axis `ScaleMatrixCols` and
the fixed-orientation matrix derivation. The companion arena
(`enemy/companion.rs`) holds the flower body and root ball as full entity
clones outside the enemy list: `e:plant42_spawn` builds them (consuming the two
clone draws in order, including the poison-body tint and the room-40C0 parked
pose), `e:plant42_tick` runs their machines with the original's draw order and
the body's `+0x70` vine pool is the boss's real health pool, and the render
pass draws each clone's own mesh object (16 the flower, 17 the root ball) at
its transform and scale. The room-40C0 set-ups (fallen core, poison arena,
Chris hold with the fixed 3x3 matrix, boss fight, poison body) select through
`behavior_flags` at init. On the player side the state-6 attacked-flag window
(knockdown / grabbed / thrown, with the thrown slide reading the shared capture
matrix's `t[0]`) and the state-2 generic hit reaction the acid spit triggers
now run in `player_script.rs`, so a spit, sweep, grab or drop can no longer
freeze the player; `e:plant42_capture_setup`/`e:plant42_hold_player`,
`e:apply_matrix_lv`, `e:vector_normal`, `e:set_player_pos`,
`e:set_player_speed`/`e:add_player_speed` and the `p42_*` scratch views are the
new Lua surface. The corpus census now finds seventeen scripted monster ids
(adding `0x08`) with no script errors, and the real-asset tests boot ROOM40C0's
boss, split vines and companions through their scripts. Neptune, Yawn and the
two Tyrants remain for the later E10 groups.

E10c is Yawn (ids `0x0d`/`0x12`), `enemy/0d/enemy.lua` plus the thin `enemy/12/enemy.lua`
alias over two new Rust pieces. The script spells the five-state head machine
(init, state check, damaged, die, the `0x80` park), the nine actions — idle,
move, the bite with its per-form damage window and the first fight's poison,
the rear-up form toggle, the swallow grab, the scripted entry crawl, the
emerge/hiss with the ten-puff ceiling burst, the flee path with its three
segment batches, and the wall-stuck reposition — the two decision trees
(`behavior_flags & 7`: free-roaming vs scripted) and the selector's whole-word
writes exactly as the original. The head's own fixed-point chain animator
(`enemy/custom_anim.rs`, the `YawnPose`) poses joints 0-2 from the animation,
stacks yaw/roll onto the previous joint for 3-14, accumulates the chain in
`<<9` space, anchors joint 13 back on its own previous world position, adds
the leftover XZ delta to the entity and rebuilds joints 0/2 in the post-move
pass;
the render recomposes the first three joints from their transforms the way the
original's render walk does. The thirteen-slot segment machinery
(`enemy/yawn.rs`) builds the twelve body slots from the head's record
(`e:yawn_spawn_segments`, linked to the joints they mirror and counted into
`game.enemy_count`), runs their whole per-frame branch in Rust
(`e:yawn_segment_tick`: mirror, visual range, ceiling bit, head damage routing
with the heavy/light split, room collision with the chain drag and the joint-3
carry, the switch-zone tail and the dead-head park), writes the head's batched
segment statuses (`e:yawn_set_status_range`) and recolours the body shadows at
the dissolve's phase 8. The render pass skips the segment meshes (the original
returns early for the sub-type-1 clones) while keeping their ground quads, and
the shadow gate now honours the `& 0x7f` switch-zone mask the death park sets.
The swallow's player side is the state-7 window in `player_script.rs` (the
carried animation, then the head-joint capture hold every frame, releasing on
the head's death), and the cross-slot scalar accessors (`e:entity_status`,
`e:entity_health`, `e:entity_pos`, `e:entity_angle` and the setters) plus the
`yawn_*` scratch views are the new Lua surface. The corpus census now finds
nineteen scripted monster ids (adding `0x0d` and `0x12`) with no script errors,
and the real-asset tests boot ROOM210's attic head with its twelve segments and
ROOM70C's rematch through their scripts. Neptune and the two Tyrants remain for
the later E10 groups.

E10d closes the boss roster with the Tyrant (ids `0x0c` lab and `0x10`
heliport), `enemy/0c/enemy.lua` plus the thin `enemy/10/enemy.lua` alias over four Rust
additions. The script spells the ten-slot state table (init, think+act, the
hit reaction that restores the `+0x174` state backup, the no-pathfind forced
state and the SCD state), the two dispatch bases that make one behaviour byte
mean different things in state 1 and state 3, the sixteen behaviours and the
eighteen-slot SCD table exactly as the original: the lab's pod floats between
`y = -200` and `-350` until `SysFlags 0x1F` lifts it, the capsule break fires
its 53-shard burst (four draws per shard, in table order) with Wesker's line
and voice bit, the scripted impale writes the victim slot's status/state word
and drags it with the root motion, and the heliport rocket death seeds
`srand(1534)`, launches joints 2/4/7/10/12 as free bodies and smoulders to
`health = -1` and `SysFlags 0x1F` where the milestone stops (the ending films
remain the ending milestone's). `enemy/custom_anim.rs` gains the Tyrant's
root-motion extractor: it composes the entity yaw with the walk (`set 0`) or
run (`set 1`) claw chain, subtracts the chain end's previous-frame world
translation, stores the planar speed and optionally slides the entity, which
is what makes the walk, thrust and impale read as root motion. The new
`enemy/tyrant.rs` holds the three render-only machines: the nine-slot ribbon
history (matrix pairs, element-wise midpoints, near/far blades, the store and
scroll branches and the `0x8000` arm sweep), the two claw ghosts with their
signed-byte 3000..6000 scale pair, and the five severed limbs' free-body
physics (X/Y/Z velocity, gravity, bounce budget and tumble rotation). The
ribbon's and ghosts' state is exact and unit-tested; their rasterisation and
the per-joint static tints they and the claw hits apply remain the documented
approximations (the claw ghosts draw as two claw mesh objects, the ribbon's
quads are computed but not rasterised). The exposed heart is a companion-arena
clone riding joint 1 with its own 22-entry beat ramp, wobble scale and
rocket-death drop; the render pass draws ghost, limb and heart meshes from
their scratch matrices. On the player side the Tyrant's own state-6 table now
runs in `player_script.rs`: the backhand/swipe staggers (with the claw and
player blood sheets and the decaying slide) and the full knock-back, whose
room probe aborts the slide into the wall slam and whose reverse get-up hands
the player back. `e:tyrant_root_motion`, `e:tyrant_heart_spawn`/
`e:tyrant_heart_tick`/`e:tyrant_heart_drop`, `e:tyrant_ghosts_tick`,
`e:tyrant_trail_alloc`/`e:tyrant_trail_arm`/`e:tyrant_trail_update`,
`e:tyrant_wander_turn`, `e:tyrant_limb_launch`/`e:tyrant_limb_update`/
`e:tyrant_limb_pos`, `e:latch_player_attacker`, `e:set_player_attacker`,
`e:snap_grab_slot`, `e:add_entity_anim_offset`, `e:angle_quadrant`,
`e:store_camera_speed` and the `ty_*` scratch views are the new Lua surface;
`e:resolve_collision` now also returns the travelled distance the walk
steering reads. Documented deviations: the look-at mode targets the player's
position rather than its joint-1 world, the impale drags the held player by
the same per-frame velocity instead of through the player's own root vertex,
`TexturePage_DeleteSet`/`StMask` are inert (no page-gardening consumer), the
room-action probe is a no-op (only observable in the ending sequence), and
the shared ghost-scale statics reset with the room instead of persisting
across encounters. The corpus census now finds twenty-one scripted monster ids
(adding `0x0c` and `0x10`) with no script errors, and the real-asset tests
boot ROOM5130's pod Tyrant with its heart companion and ROOM3030's suspended
heliport Tyrant through their scripts. With E10d the E10a–d boss roster is
complete; Neptune (id `0x0b`), which the E10 grouping left unassigned, and the
Forest zombie stub remain documented follow-ups.

E11a lands the player weapon runtime. The converter now packs every shipped
weapon asset — the 30 `W*.EMW` animations the locomotion pair did not already
carry, as `player/w{id:02x}.emw`, and the seven `WS*.TMD` held-weapon meshes
as `player/ws{id:03x}.tmd` — and `arklay verify` reports the new `emw`/`tmd`
rows green (since reorganised into `player/<id>/weapon/<item>.emw` and
`enemy/<id>/weapon/<behavior>.tmd`). `src/weapons.rs` transcribes the original's aim/fire machine: the
entry from the aim input, the raise/hold poses, the reticle scan and lock-on
turn, the auto-aim fire with its ammo frame, damage frame, big muzzle flash,
second flash and shotgun double-fire, the quick-fire and reversed recover, the
holster, the empty-clip click and the reload motion with its per-weapon FX
routines, plus the knife's aim/hold/swing/holster/turn. The fire tables (fire
frames, sound ids, billboard offsets and depths, end frames, special-weapon
frame windows, aim heights) select the packed clips and time the FX; the
damage rides the existing pipeline through `apply_weapon_damage_with`. The
ammo model matches the original: the equipped weapon's own quantity byte is
the clip, the matching ammunition item (`weapon + 9`) is the reserve, the
reload moves rounds from the largest stack, and the special weapons and the
flagged rocket launcher refill their clips. The held weapon draws at joint 14
with the player's texture page (the `WS*.TMD` meshes with their character's),
the muzzle/secondary flashes attach to the same joint, and the state-8 tail
now stores the posed joint worlds so the character fire handler's FX ride the
weapon hand and the two flamethrower cues queue through the room's enemy bank.
The player-side inputs gain port-only aim (`C`) and fire (`Z`) bindings, and
the engine's tick routes the unfrozen player through `weapons::update` with
the locomotion machine as the fall-through. Documented deviations: the
joint-anchored body correction
(`EntityUpdateWeaponJoint`, subsumed by the port's per-frame pose composition)
and the projectile effect behaviours' own damage (the flamethrower, acid,
flame and rocket rounds) remain as before. The beretta's detached magazine
(the accessory joint 15) landed later with issue #124: the reload's frame-0xa
volley gate arms the player EMD's own 16th mesh object and
`weapons::update_detached_clip` runs the original's fall/bounce physics with
its floor-hit dust and cue, shared by the empty-clip and tactical reloads.
The synthetic suite covers the
weapon matrix (each class fires and damages, ammo decrements, the empty clip
clicks, the reload refills), the aim/holster locomotion hand-off, the
hand-joint FX attach and the flamethrower cues; the real-asset
`tests/real/weapons_real.rs` parses every packed weapon asset, kills ROOM30C0's web
through the weapon input path, and captures the raised weapon in a shipped
room.

E11b closes the death flow. The state-1 entry now runs the original's three
pre-control checks: a player whose health went negative flips the facing when
the back-shot sentinel is set, enters the death state and is held at one
health inside an effect zone (the cannot-die guard); a raised attacked flag
pre-empts control into the generic hit reaction; and the poison status drains
two health on its shared timer, with only the fast variant allowed to kill.
The collision callback byte now mirrors the live health-status byte before the
player's physics, so a script that clears the `0x10` bit really stops skipping
the shape-5 floor volumes. The death fall (player state 3) screams, plays the
body clip, slides the corpse, re-colours and shrinks the ground quad into the
blood pool and grows it 16 units a frame for 148 frames before blocking input;
the three scripted-death rooms (the armor room, the drug storehouse and the
morgue) skip the pool. The game-over machine waits the original's 90 frames,
fades the screen to white over 128 frames and opens the DIED screen; the two
Yawn rooms and the Neptune water tank fade immediately instead, and the
attract/death-variant flags skip the screen. `src/ui/death.rs` transcribes the
screen: the `died.tim` page is drawn as four mirrored 160x120 static
quadrants, the camera is built from the death position (7000 above, looking
1000 short of the body) and the corpse spins 8 units a frame, the "YOU DIED"
strip draws 256 one-pixel columns with the original's exact sine phase
(`0x100` per column) and amplitude ramp (59 down to 4, then back up during
the fade-out), the black overlay ramps 3 a frame, and the confirm button skips
to the title. The first death screen hides the player's head joint through the
shared grab/death one-shot, whose original image value is non-zero. The
converter packs `ui/died.tim` (66,080 bytes, 8bpp) and `arklay verify` reports
the new `tim` row green; `--ui death` boots the deterministic capture over
ROOM101 with `--character` selecting the corpse. The synthetic suite covers
the control gate (death, hit pre-emption, poison), the fall and pool, the
room-variant dispatch and the screen machine's frame counts and strip math;
the real-asset `tests/real/death_real.rs` decodes the shipped page, captures the
deterministic DIED screen, and kills ROOM100's player through the real damage
helper to reach the screen and return to the title. Documented deviations: the
Plant 42 DIED-screen joint-hide variant and the countdown-timer death branch
stay deferred (see `docs/m18-deviations.md`).

E7 ports the hound (id `0x02`) as `enemy/02/enemy.lua` and the chimera (id `0x09`)
as `enemy/09/enemy.lua`. The hound script spells out the single 16-slot dispatch
block once and reads it through both bases — the state view indexed by `state`
and the behaviour view biased by five and indexed by `ignore` — then runs the
five sub-machines (patrol's begin/pick-turn/turn, chase's run/wall-follow/skid,
the five leap sub-states, the strafe's hold/accel/spring, the stalk's
begin/arc/reface/wall-follow) and the wall-follow switch that reduces each
sub-step's return to a sign. Its ten behaviours cover the idle walk/run patrol,
the chase with its pathfinder give-up into the stalk, the scripted entrances
(running leap, window crash, ceiling drop), the strafe, the bark/alert pause,
the maul, the close-range bite, the scripted idle and the stalking patience
meter with the room-shared bark latch. Nearly all steering runs through the
Rust probes: the forward probe steps 500 units along the full rotation vector,
resolves the room at radius 400 and steps back; the turn probe scales the live
speed, steers a yaw delta, probes and restores the accepted `position` words,
the local translation and the speed. The head tracker integrates the neck
yaw into the swerve word with its `+/-0x100` clamp, the leap/bite/maul chain
runs the shared ballistic step, the kill branch forces the player's maul
animation and the maul pins both sides' grab offsets, greys the player at
frame 0x66 and owes its blood billboards. The helpers the hound needed landed
with it: the probe pair with the `0x6C` save/restore, the spawn nudge, the
projectile-arc exposure, the head-track integration, the shared probe scratch
vector, the `+0x16C`..`+0x18A` typed scratch views, the 16-bit `position`-word
accessors, the room-shared bark latch and the player flags/offset writes.

The chimera script transcribes the four-state table (spawn, AI driver, hit
layer, death layer), the three stacked index bytes and the seventeen-entry
behaviour table whose last three slots are the same pointers as the variant
brain table. The three brains run the floor chase/walk hand-over, the coin-flip
spit or claw roll gated on the angular view and line of sight, and the ceiling
variant's mirrored yaw (the whole body flips 180 degrees and back) with its
flee, turn-back, swipe, drop and hit-fall hand-overs; hard mode injects the
variant toggle and the harder damage/threshold rows. The behaviours cover the
floor and ceiling idles, the zone-path walk, the timed turn, the double-claw
swipe with its two reach tests and the scratch overwrite quirk, the grab with
its mash meter bleeding into the player's attack direction, both variant
toggles, the drop and swoop dive arcs read in opposite directions, the acid
spit, the flee and turn-back (yaw-flipped steering), the claw with its wall-bit
grab cancel and hard-mode touch, the hit-fall and the stagger/knockdown
drivers, and both death dissolves with the shadow recolour/growth and the
death-event flag. The ceiling attachment parks the model at `-6120` with the
180-degree roll, and the per-frame shadow resize reads joint 3's world height
with the 600-clamp band and the freeze gate. The helpers the chimera needed
landed with it: the four chimera scratch views, the player's attack-direction
and animation-offset accessors, the torso/claw joint-attached billboards and
the entity-matrix acid fan. The corpus census now finds twelve scripted
monster ids (`0x00`..`0x05`, `0x07`, `0x09`, `0x0a`, `0x0e`, `0x11`, `0x13`)
and no script errors.

E12 closes the arc with the verification sweep. The corpus census replays
every `enemy` record in all 348 rooms into a fresh `GameState` and finds
twenty-one scripted monster ids (`0x00`-`0x15`, the DC-only `0x16` warning
park) allocating their recorded id/behaviour/position/animation/slot with
zero script errors and an exact deferred set; `enemy_real` boots one shipped
room per id through its script; `weapons_real` fires the real weapon set
through the input path at a shipped web and captures the held weapon in the
hand; `combat_real` drives the built-in combat tables through the shared
pipeline and kills a real web; `death_real` reaches the
shipped DIED screen from a real room and returns to the title; the NPC
differential and the actor/walk/entities/fire suites stay green, so the
character port and the monster scripts share one verified runtime. The pack
reconverts deterministically with the `emw`/`tmd` rows green and
the `tim` row at 124/124; the remaining 28 `rdt` failures are the documented
four-byte stub rooms. The milestone deviations (render-contract items, the
remaining death-screen variants and the DC branches) are catalogued in
`docs/m18-deviations.md` and the module docs.

The F1 debug tools are a port-only development aid the retail game never
shipped. They are always available (`--debug-menu` is accepted for
compatibility only); they open only on F1, so captures and goldens that never
press the key are unaffected. F1 freezes the room behind a dark panel listing
ten tools; confirm opens one, cancel backs out one level and F1 closes from
anywhere. Reopening returns to the same tool, and ROOM SELECT follows the room
the player moved to while the panel was hidden. `--ui debug` captures the list.

- ROOM SELECT: the pack's rooms in seven stage columns, the current room in
  green; a jump uses the pending character's room variant when it is packed.
- INVENTORY: edit each slot's item and quantity, with an icon picker. Left
  and right step the quantity down to zero (the item stays), and START
  clears the slot.
- PLAYER: character and outfit (normal, alt 1, alt 2) are pending and apply at
  the next room load, like the wardrobe; the alternate outfits load
  `em1030 + character + 2 * variant` as the original's `LoadEntityEMD` does.
  The outfit does not survive a save/load: the costume bit lives in the
  transient flag bank the load clears and the outfit byte is not in the save
  block, so a loaded game wears the normal outfit.
- FLAGS: thirteen flag banks (scenario, items, enemies, rooms, maps, ...)
  with port-only names; confirm toggles, L1/R1 set/clear.
- CHEATS: INVINCIBLE pins health and blocks death; GHOST hides the player from
  every hostile script, including those that read the player position directly
  (partners still follow; the per-entity hidden flag is on the player entity).
  AMMO cycles OFF/RELOAD/INFINITE: RELOAD still spends
  rounds and leaves the clip empty until the next trigger press, which plays
  the normal reload animation and fills the clip without using the reserve;
  INFINITE pins the clip at capacity and never decrements it. Session-only,
  never saved.
- SAVE: save opens the typewriter save screen; load opens the load screen and
  continues from the picked save (a numbered slot, or the file a registered
  screen chose with `api:load`).
- ITEM BOX: opens the item box anywhere.
- CLEAR GAME: plays ending 1-7 (defaulting to the one the current flags
  select) and the result screen.
- CONTROLS: the input rebind screen (keyboard, mouse and gamepad), saving to
  the `[bindings]` table of `arklay.toml`.
- DISPLAY: the presented frame rate (30/60/120/144/165/240) and the aspect
  ratio (`4:3`, `16:9`, `16:10`), saved to `[graphics]`.

`[graphics] aspect` (also `--aspect`) presents a wider frame by cropping the
320x240 view: in a room the band follows the player's chest row, films show
the centred band, door transitions stay a pillarboxed 4:3 frame, and the F1
panel fills the aspect's whole width over the room. The message window and
overlays draw in the band, while the pause menu and its sub-screens take a
full-height canvas of the aspect's width. Captures, traces, the oracle and the
goldens always run 4:3.

The in-game F9 return-to-title prompt follows the original: the first F9
freezes the room, dims the frame and pauses the game sounds; a second F9
returns to the title screen and any other key cancels and resumes the sounds.

M19 moves the user-facing knobs into `arklay.toml` (in `~/.config/arklay`,
else `$XDG_CONFIG_HOME/arklay`, `%APPDATA%\arklay`, or `$ARKLAY_CONFIG_DIR`):
`save_dir` (default `<config dir>/save` — saved games moved out of the
`saves/` directory beside the pack) and `language`. A missing file is the
default configuration, so a run without one behaves exactly like the port did
before apart from the save location, and `--save-dir`/`--language` override the
file.

The milestone's other half is the Lua screen surface, and the proof is that
the Classic REbirth-style screen is an ordinary mod rather than an engine
feature. `mods/crloadsave/` carries `manifest.toml`, a `mod.lua` whose
`on_init` calls `api:register_save_screen("script/loadsave.lua")`, the screen
script and its `UI_SAVE_CR_*` strings; nothing in the engine or the base pack
names it. The new `on_init` hook runs once the pack's scripts load and before
any room, the app runs it in a throwaway VM before the title appears (so the
title's LOAD GAME and the load screen honour the registration), and every
session runs it over its live state for the typewriter. A registered screen
also makes the title's existence probe count every loadable file in the save
directory rather than the eight `savedatN.dat` slots, since a script lists the
directory itself.

The screen script runs on the general Lua screen surface documented in
`docs/modding.md` — `on_open`/`update`/`draw`, `api:saves()` (the folder scan
with each file's parsed character, save count, stage/room, resolved location,
timestamp and loadability), `api:save`/`api:load`/`api:remove`/`api:exit`, the
pad edges and words, `api:sound` for the screen cues and immediate-mode
drawing (`background`, `rect`, `text`, `sprite`, `sprite_region`,
`image_size`, `fade`) in the 320x240 layout space. The ported original screen
stays the default, and a script that cannot load or errors while running falls
back to it (or leaves the screen) instead of trapping the player, so a broken
mod never costs the typewriter. The CR layout follows the disassembly notes
and the running DLL's captures: ten 15-px rows from y 49, the measured panel
and selection band, the centred detail line, folder-based save names,
overwrite-by-delete and a white `Create New` row at the top of the save screen.

Mod hooks are read per layer now: a mod's manifest `lua` list used to be
ignored when the base pack named hooks, so a mod had to bury its registration
in `lua/**.lua` discovery; `Pack::own_paths` and the layered hook walk fixed
that, and `mods/crloadsave` keeps `mod.lua` at its root.

The milestone also bounds two debug stack frames: `run_ui_impl` and `App::boot`
gave their session arms ~1 MB each (a debug build puts a stack slot behind
every by-value `GameSession` temporary), which overflowed the Windows debug
binary's 1 MiB main-thread stack for every `--ui` boot; both now build the
session in a small helper, the split `App::apply` already used.

The default save location is a user-visible change: saves written by earlier
builds under `saves/` beside the pack are not found automatically. Point
`save_dir` (or `--save-dir`) at the old directory to keep using them, or copy
the `savedat*.dat` files into the new one; the block layout is unchanged. The
milestone's approximations (the panel/band geometry, the save-mode flow, the
`save_NN.dat` naming and the unmigrated legacy saves) are catalogued in
`docs/m19-deviations.md`.

## Format coverage

`arklay verify <pack>` classifies every entry by path/extension, runs it
through the reader the engine uses, and reports a per-format table. The
converted base pack on the reference install reports:

```
pack: re1.akpak
3266 entries, 484370780 bytes
format       entries        ok    failed          bytes
manifest           1         1         0            106
rdt              348       320        28       51364907
roomcut          842       842         0       42258554
roommask         601       601         0        4194392
bmp               17        17         0         600063
tim              124       124         0         784016
ivm               77        77         0        1581983
emd               63        63         0        4062003
emw               32        32         0         446942
tmd                7         7         0           8822
dor               34        34         0         888924
wav             1039      1039         0      126174536
language           8         8         0         205617
biocard            1         1         0            335
avi               27        27         0      251624740
lua               39        39         0         151338
opaque             6         6         0          23502
verify: 3266 entries, 3238 ok, 28 failed, 6 opaque
```

The `bmp`/`tim` rows grew with the DIED screen's and the RESULT plates' art
since the previous measurement. The CR save-screen script is not in the base
pack: it ships in `mods/crloadsave` and is layered like any other mod.

The 28 failing room entries (`room/*.xml`; `room/*.rdt` in a pack converted
before the XML room format) are the converter's four-byte stub rooms
whose header declares no camera cuts: the engine refuses them on load and the
soak skips them, so `verify` reports them instead of green-lighting rooms the
engine cannot load. Every other entry parses. The six opaque entries are the
four attract reels (`demo/*.dat`), whose bespoke format the classifier does
not cover yet, and `data/core00.esp`/`data/core00.etm`, the effect tables
whose pair parser is not in the path classifier; `--strict` turns them into
failures by design.

A mod layer's replacement entries are classified by their own formats too:
a room's `cut/*` and `mask/*` accept any image-class extension, and `png`,
`webp` and `ogg` get their own rows with the replacement decoders behind them
(the `hd` feature). The converted base pack above carries none of them, so its
table is unchanged. `arklay verify` therefore validates a replacement mod pack
before it is ever layered, exactly as `tests/real/hd_real.rs` does for the
checked-in `mods/hd`.

The one pack carries everything: the 533 positional voice `wav` entries and
the 27 shipped `avi` films (251,624,740 bytes) sit beside the room, audio, UI
and `language/` entries, and all three formats verify clean. `language/`
replaces the retired executable `text/*.bin` tables.

## Verification spine

| Commitment | Where | State |
|---|---|---|
| Golden capture harness | `tests/real/m16_ui_real.rs` (ignored, local goldens) | present; goldens are regression cross-checks, not an oracle |
| Movie decode goldens | `tests/real/m14_real.rs` (ignored) | present |
| Deterministic captures/states/saves | M16 tests, `tests/real/save_real.rs`, soak round-trip | present |
| Save round-trip | `tests/real/save_real.rs`, `tests/soak_real.rs`, synthetic M16 slice-10 test | present |
| Stable torture matrix (>=10k mutated inputs) | `tests/torture.rs` (parallel budget slice) | present, 18 formats |
| cargo-fuzz targets and seeds | `fuzz/` package (parallel fuzz slice) | present, 19 targets with committed synthetic seeds |
| Enemy spawn census | `tests/real/enemy_census.rs` (ignored) | present; every corpus record allocates, scripted set exact, zero script errors |
| Per-id enemy boots | `tests/real/enemy_real.rs` (ignored) | present; one shipped room per scripted id |
| Weapon/damage/death suites | `tests/real/weapons_real.rs`, `tests/real/combat_real.rs`, `tests/real/death_real.rs` (ignored) | present; real input kills over the built-in tables, shipped DIED screen |
| Scheduled fuzz workflow | `.github/workflows/fuzz.yml` | present, nightly, non-gating |
| Decode budgets and memory ceilings | `src/budget.rs` (parallel budget slice), `tests/memory.rs` | present |
| `arklay verify` | `src/verify.rs`, `tests/cli.rs` | present |
| CLI error paths and atomic outputs | `src/atomic.rs`, `tests/cli.rs` | present |
| `--stats` frame-time report | `src/stats.rs`, `tests/cli.rs`, `tests/perf_real.rs` | present |
| Full-game soak | `tests/soak_real.rs` (ignored, `ARKLAY_SOAK=1`, release only) | present |
| CI matrix (Linux/Windows/release/MSRV/artifact) | `.github/workflows/ci.yml` | present |
| Local check tiers (session, `--ci`, `--full`, `--soak`) | `scripts/check.sh`, `AGENTS.md` | present |
| `docs/architecture.md`, `docs/status.md`, `docs/m17-deviations.md` | this docs set | present |

Still missing by design:

- The cargo-fuzz runs need nightly and run on a weekly schedule, never as a
  pull-request gate; every change is covered by the always-run torture matrix
  and the stable `cargo check --manifest-path fuzz/Cargo.toml` compile gate.
- The real-asset tests, goldens and budgets need a local install and are never
  run in CI with game data; they are ignored tests behind
  `ARKLAY_RE1_ROOT`/`ARKLAY_RE1_PACK`. The full-game soak is additionally
  behind `ARKLAY_SOAK=1` and runs only at release time.
- Performance budgets are machine-specific policy; the numbers and the
  reference machine are recorded in `docs/performance.md`.

## `TODO(parity)` dispositions

1 site (`rg -n "TODO\(parity\)" src`):

| Group | Count | Sites | Disposition |
|---|---|---|---|
| Gameplay | 1 | backward clip `player.rs:64` | actor/world or death work |

The UI site (the title idle timer) closed with the attract demo and the Lua
title screen.

E4 closed the force-init/saved-state restore site: `spawn_enemy` now consults
the saved-enemy snapshot before re-initialising an occupied slot.

M18 closed nine of the sites this table used to list and removed their
comments: the collision-resolved walk, zone-graph walker, SCA resolve and
obstacle pathfinder in `enemy/walk.rs`; the idle init in `enemy/idle.rs`; the
spawn SCA-record TODO in `game.rs`; the pan law and per-bank one-shot restart
in `audio.rs`; and the per-record background blend weight in `engine.rs`. See
`docs/m18-deviations.md` for the replacements and their capture re-baselines.

E11a closed three more of the listed sites: the weapon-joint refresh
(`enemy/scd.rs`) now stores the posed weapon-hand joint worlds, the
flamethrower's two cues (`enemy/scd.rs`) queue through the room's enemy bank,
and the held-weapon TMD conversion (`convert.rs`) packs every `W*.EMW` and
`WS*.TMD`. The remaining table matches the current `rg` output.

E11b closed the health-byte aliasing site: the engine mirrors
`health_status` into the player's collision-callback byte before the player's
physics each tick, so a script that clears the spawn's `0x10` bit stops
skipping the shape-5 floor volumes exactly like the original.

The player's reaction windows are complete. The EMD directory's first chunk
(the original's damage scratch pair, `emdScratchPtr1/2`) is parsed as
`Emd::damage_clips`; the model loader repoints `GameState::player_damage` at
the last loaded monster model, exactly like the original's global pointer, and
`ClipSource::Damage` poses those clips on the player's own skeleton. All
fourteen entries of the original's animation-function table run: the zombie
bite and wasp pin (state 5), the monster plant, Plant 42, Tyrant, crow,
chimera and Neptune holds (state 6), and the hound maul, hunter pounce, Plant
42 eat, Neptune devour and Yawn swallow (state 7), plus the state-2 hit
reactions from the same bank. The Lua surface writes the original's field
pairs (`set_player_animation`/`player_state`/`player_anim_frame_id` at
`+0x84`, `player_attack_anim` at `+0xBD`), so a bite, grab or strike plays its
recorded clips and always releases the player. The per-joint tints, joint
billboards and joint-velocity physics those handlers use in the original stay
the documented render-contract approximations.

## Road to enemies

The enemy milestone is complete. Every monster id the corpus spawns is
scripted in the game pack (ids `0x00`-`0x15`; the DC-only Forest zombie
`0x16` parks inert), the scripted characters are fully ported, the player
weapons and the death/game-over flow are live, and the verification spine
covers them end to end: the spawn census across all 348 rooms, the per-id
real-asset boots, the real weapon/damage/death suites, the reconverted
`verify` table with the weapon rows green, and the always-run
synthetic suites for every machine.

Still open, by design:

- The ending path's gameplay trigger: the RESULT screen, the rocket-launcher
  epilogue and the next-cycle save run end to end under `--ending`, but no
  combat/boss path reaches `ending_state` yet, so the surviving flags stay
  unset in normal play.
- The DC-only Forest zombie (`0x16`) and the DC branches the port does not
  model.
- The documented render contracts: per-joint colour tints and joint-object
  gore chunks/trails, the ribbon rasterisation, the Plant 42 DIED-screen
  joint-hide pose, and the flamethrower/acid/rocket per-frame projectile
  damage. Each is catalogued in the module docs and
  `docs/m18-deviations.md`.

RE2 work is tracked by the milestone issue #244 and its sub-issues. R0
(#15) adds conversion only: `convert-game` detects an RE2 PC install (or takes
`--game re2`) and writes an `re2.akpak` base pack with no runtime dialect yet.
Against the retail install it packs 501 rooms, 5028 camera backgrounds (1242
distinct `roomcut.bin` streams decoding to 1205 distinct images), 234 enemy
files, 221 player files, 599 voice lines, 118 BGM tracks, 55 doors, 26 films
and 6 demo reels in about 13 s. `arklay verify` classifies the RE2 formats by
content and passes them unparsed (they fail under `--strict` until the
runtime milestones parse them). Retail-data quirks the converter handles:

- `ROOM7051.RDT` is a broken stub and is skipped with a warning.
- Room 7071 has 8 camera pointers past the end of `roomcut.bin`; those
  cameras get no background.
- Rooms 30C0, 30C1, C0C0 and C0C1 declare 17 cameras, one more than the 16
  pointers per room the table holds; the 17th is dropped with a warning.
- `Pl0/emd0/EM13A.TIM` is a stray, differing copy of the disc-1 texture; the
  file in its own disc's folder wins.
- The `Pdemo**p.dat` alternate demo takes are not packed.
- A decoded ADT stream whose camera has mask sprites carries its mask page,
  an 8bpp TIM, after the background; R4 (#248) packs it.

R1 (#245) loads RE2 rooms. Room loading detects the RDT dialect and reads an
RE2 RDT through `rdt2` into the same `RoomSource` RE1 rooms build from:
cameras (RID, focal length `perspective >> 7` as the executable sets it),
camera switch zones (RVD) and the SCA collision records, which keep their own
`Sca` type and are drawn by the collision debug views. `RoomId` takes the
letter stages A-G (stages 10-16, `room/g010`), the debug room select grows a
tab per extra stage, and the pack manifest's `start_room` names the new-game
room (`100` for RE2). `F4`/`--render-mode collision-overlay` draws the
collision over the background from the cut camera. All 501 rooms and 5040
cameras load against the retail install (`tests/real/re2_real.rs`); the 12
cameras without a background (above) log a warning instead of failing the
room. RE2 rooms run no scripts and have no player yet; the RVD
floor/flag bytes are not read. The TeamX RE2 HD pack converts with
`convert-mod --base re2.akpak`; its backgrounds are named by the CRC of the
camera's whole 256 x 256 main page (`bio2hd.asi`: the 240 picture rows with
black clear, then 16 zero rows), which names 4740 of the 5028 cameras'
backgrounds. The rest are rooms and camera states the pack has no file for
(`ARKLAY_RE2_HD` runs the count in `tests/real/re2_real.rs`).

RE2 R2 (#246): every EMD (116), PLD (40) and PLW (181) parses through
`src/emd2.rs` and draws textured and animated with `--model <entry>` in
any room (`src/model_viewer.rs`, no AI; `tests/real/re2_real.rs` checks
each one). A PLW is composited onto its disc's player model; knife PLWs
have no animation and keep the player's clips. Each camera's LIT record
is read as three point lights (radius = brightness) plus ambient on the
RE1 scales. RE2 HD model skins are hashed per 128-texel page through
that page's CLUT row (359 of 383 pages named); weapon-composited pages
no longer match their skin.

The RE2 pack follows the RE1 pack's conventions (convert version 9): each
room is a `room.xml` (`game="re2"`, converted through `rdt2` and checked to
read back and build the same room), each enemy texture an indexed
`enemy_<s>.bmp` page plus a 16x16 palette swatch `enemy_<s>_clut_<nn>.bmp`
per further CLUT row (a 256-entry BMP palette holds one row; the 118 retail
textures read back exactly), and each BGM a numbered `bgm/<id:03x>.wav`
(`main<nn>` is `0nn`, `sub<nn>` `1nn`, a `_<k>` alternate take adds
`0x200 * k`). Room names moved out of the language files into each
`room.xml`'s `<strings>` (RE1's now checked in with each room, RE2's
English names from the debug room table in the install's Classic REbirth
`ddraw.dll`, 30 slots of 24 bytes per stage; 500 of 501 rooms are named,
707 has no slot), and a
non-RE1 pack takes only the `UI_` strings from the built-in RE1 language
files, so RE2 rooms no longer show RE1 names.

RE2 R3 (#247) reads RE2 scripts (`src/scd2/`). An RE2 RDT's section slots 16
and 17 hold the init and main procedure lists: a `u16` offset table, then the
procedures. The last procedure's end is not stored; it ends at the first
`evt_end` past every block it opens, and the bytes after it are not script.
The opcode table has 0x8F fixed-width opcodes, with widths and block
semantics checked against `bio2.exe`'s handlers (`endif` is 2 bytes; IDA's
`Vib_set1` at 0x14 is `case` and `Case` at 0x18 is `gosub`). `scd export`
writes an RE2 room as `.s` or `.akscr`; `tests/real/re2_real.rs` checks that
all 501 rooms (5090 procedures) rebuild their lists byte for byte and
round-trip through both forms, with every block decompiled to its structured
form. Running the scripts, `scd build` for RE2 and the `.lst` listing come
later.

RE2 R4 (#248) adds doors, masks and footstep zones.

- Doors (`src/door2.rs`, `src/door2/`): a DO2 is an EDT sound table, a
  PlayStation VAB, then a sector-aligned block holding an RE2 SCD procedure
  list, an MD1 mesh of up to ten door parts and an 8bpp TIM. `door2::vm`
  runs the procedures the way `Door_init`/`Door_move`/`Door_Trans`/
  `Door_exit` do: four tasks (10-13) on the RE2 scheduler, the part
  hierarchy and speeds, the fades and `se_on`. Its frame is the RE1 door
  VM's, so the existing door transition and renderer play it. As
  `Door_load` does, the sounds are the slots of a PC door bank
  (`door%02d.sap`, converted by R5; `door2::sound_bank` maps the doors that
  share one) at the volume of the tone the DO2's sound table names; a pack
  without the bank falls back to the DO2's PlayStation VAB, decoded from
  ADPCM. All 55 retail DO2s parse and play to the end with every door type
  and no fault (`tests/real/re2_real.rs`). Doors `20`, `21`, `32`, `34` and
  `36` are fade-only (no visible part), and the second sound of doors `33`
  and `35` is silent, as in the original: their banks hold slot 0 only. `arklay re2.akpak --door ID [--door-type N] --capture F` plays one
  headlessly. Gaps: the parts are drawn full-bright, attribute `0x4000`
  (semi-transparency) is ignored, the transition keeps RE1's timing, and
  no room triggers a door yet (there is no RE2 player).
- Masks: the camera record's `+0x1C` word points at its mask table (every
  camera has one; 1646 of the 5040 start with `-1`, no sprites). The table is a
  `u16` group and sprite count, 8-byte groups (count, `0x7800`, origin) and
  8- or 12-byte sprites (`u, v, x, y`, depth, then a square size or `0`
  and an explicit width and height). `mask::MaskTable::parse_re2` reads it,
  `room.xml` round-trips it, and the renderer orders the sprites by
  `AddMask`'s key, `max(depth << 5, focal / 2)`, in file order. The mask
  page is the 8bpp TIM after the decoded background (256, 128 or 64
  square); `convert-game` packs it as `room/<id>/mask/<nn>.bmp` for the
  3394 cameras with sprites. `F4`/`--render-mode masks` shows them, and
  `--model-pos X,Y,Z` stands a `--model` behind a masked foreground.
- HD masks: TeamX names an RE2 mask page by `bio2hd.asi`'s CRC of the page
  as the mask decoder keys it (black clear, 5-to-8-bit replication) over
  the page's own size, with a 1024 x 1024 replacement whose top-left `4w x
  4h` holds it; `htexture::image_candidates` offers that name for the 128
  and 64 pages too.
- Footsteps: section slot 11 (`rdt + 0x34`, `Floor_check`/`Flr_set`) is a
  `u16` count and 12-byte records (`x`, `z`, width, depth, sound, flags:
  the low 7 bits the floor level, bit 7 off). `rdt2` reads them into the
  room's footstep zones (1387 in the retail rooms), kept in `room.xml` with
  a `flags` attribute; nothing plays them until RE2 has a player.

RE2 R5 (#249) makes RE2 audio play. `adpcm` decodes MS-ADPCM (WAVE tag 2),
ported from biohazard-utils' `MSADPCMDecoder`, and `audio::parse_wav` uses it,
so every voice, BGM and sound WAV loads; `verify` parses them instead of
listing them as unsupported. The port predicts with an arithmetic shift where
ffmpeg divides; `tests/real/re2_real.rs` decodes all 2893 MS-ADPCM WAVs (1454
distinct, 316M samples) and checks that the dividing variant matches ffmpeg
sample for sample and the shipped shift stays within 79 (the bound is 128).
Each `.sap` bank's 64-bit mask gives the slot of each WAV it fronts, so the
converter splits the door, arms, room, enemy and core banks into
`sound/<kind>/<id>/<slot>.wav` (hex ids; the `.sap` names are decimal, the
`.EDH` names hex), footstep sets into `sound/step/<id>/`, and stores each
bank's EDH sound table (EDT entries plus the VAB header the tone volumes live
in) as `bank.edh`; a room's own table, RDT sections 0 and 1, becomes
`room/<id>/sound.edh` (476 of 501 rooms). `se2::Banks` loads banks the way
`SsLoadBanks` does (the room bank, its three footstep sets at slots 23, 26 and
29 from `footstep_tbl`, `room000` for the rooms `Room_fs_ck` lists, core 0x16
over every core but 0x15, the reverb enemy bank for 0x25/0x26) and resolves
a `Snd_se_on` id (type byte 3, slot byte 2) to a WAV and the `SsSetVol` gain
of its EDT tone. `bgm2` holds the room music table: `bgm_tbl` has one `u16`
per room (main id low, sub id high, `0x3f` none) written only by script opcode
0x57 `sce_bgmtbl_set`, and the `bgm_lut`/`sbgm_lut` channel tables and the
track name tables from `bio2.exe`; every referenced track is packed except
`main26`, which the game does not ship. The `.BGM` files are PSX leftovers (a
pQES sequence and a pBAV VAB): the PC executable has no sequencer and only
reads them for a size check, so they are not packed and need no sequencer;
the streamed `.sap` WAVs are the music. Not modelled yet: the `Snd_se_on` and
`SsPlay` per-room special cases, 3D panning, the `SsLoadHack` dual-BGM
speaker setup, and running the music table, which waits for the RE2 script VM
(R8).

RE2 R6 (#250) plays the RE2 films. Conversion already packs each retail
`.bin` raw as `movie/<name>.mpg`: an MPEG-1 program stream with 640x480
MPEG-1 video at 29.97 fps and 192 kbit/s 44.1 kHz stereo MP2 audio. Shipped
formats use the port's own decoders, so `src/mpeg/` decodes them in-house:
a program stream demuxer, an MPEG-1 video decoder ported from pl_mpeg
(MIT) and an MP2 decoder after kjmp2. There are three departures from the
references. A damaged slice is counted and its picture kept. Motion
vectors that point outside the picture read clamped edge pixels. MP2
requantisation and output scaling follow the standard, as ffmpeg does,
not kjmp2's approximations, which played about 2% quiet. Against ffmpeg's decode,
video is above 40 dB PSNR on the unit fixture and above 35 dB on a retail
film, and audio is above 40 dB SNR. A film has one display frame per
coded picture. Its audio starts at the PTS offset from the video (both
start at 0.403 s in every retail film) and is resampled to the mixer's
22050 Hz. Frames are shown full size through the HD film path, which no
longer needs the `hd` feature. RE2 films have no id table, so
`MovieSession::open_named` opens one by name and `--fmv` takes a name or
an RE1 id. A mod's `movie/<name>.mp4` or `.avi` still wins over the
`.mpg`. `tests/real/re2_real.rs` plays all 26 films end to end and checks
each one's picture count and MP2 frame count against `ffprobe`: 120 to
9173 pictures, with a duration within a frame of ffprobe's, whose duration
ends half a frame early. No film has a damaged slice. One thread decodes
about 6 ms per 640x480 frame, so decoding runs on the caller's thread.
`verify` now parses the MPEG entries, so strict mode passes them.

The manifest no longer carries `rdt_version`/`scd_version`: formats are
detected per file (`src/format.rs`), so one pack may hold both dialects.
