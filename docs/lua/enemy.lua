---@meta arklay.enemy

-- RE 1 enemy scripts: the per-id `enemy/{id:02x}/enemy.lua` update scripts
-- and the shared modules they `require`.
--
-- This file is the source of the "Enemy scripts" page of the documentation
-- and a LuaLS definition file: add the docs/lua directory to your editor's
-- `workspace.library` to get completion and type checks for enemy scripts.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.
--
-- The methods are checked against `impl UserData for EnemyApi` in
-- src/enemy/lua.rs by `scripts/lua-api-doc.py check`. The properties
-- (`add_fields` in the same impl) are not checked by the script: keep the
-- field classes at the end of this file in step with it by hand.

---Entity behaviour is pack data. For every active entity slot whose id is a
---monster (`0x00..=0x1F`), the engine looks up the pack entry
---`enemy/{id:02x}/enemy.lua` and calls its global
---[`update(e)`](#update) once per fixed 30 Hz tick, after the room's event
---scripts and before the player's physics. The scripted characters (ids
---`0x20..=0x2E`) use the same mechanism, but only while their state is `0`,
---`1`, `8` or `9` and no message window pauses them; in their other states, or
---when the pack carries no script for them, the native character driver runs
---instead.
---
---```lua
----- enemy/13/enemy.lua
---function update(e)
---    if e.state == 0 then
---        e.state = 1
---        e.health = 0x37
---        e:reset_joints()
---    end
---    e:advance_anim(0x400)
---end
---```
---
---The `e` argument is the live entity behind a scoped userdata: its scalar
---fields are Lua properties (`e.state`, `e.health`, `e.pos_x`, see
---[the property tables](#EnemyEntity)), and the composite operations are
---methods called with colon syntax (`e:advance_anim(step)`). It is valid only
---for the duration of the one `update` call; storing it and using it later
---raises an error.
---
---A monster script owns its whole tick: its own message gate (most monsters
---do nothing while `e.monster_paused` is true; the spider web keeps running
---behind a message window) and its own tail (collision, separation, switch
---zone). After `update` returns, the engine refreshes the entity's SCA hit
---volume and recomputes its joint world matrices from the pose the update
---left behind, so the next tick's `e:joint_world_x` and the other joint
---readers see this tick's pose.
---
---## Enemy folders
---
---Everything about one entity id lives in its own folder, named by the id in
---two lowercase hex digits:
---
---```text
---enemy/
---    0a/
---        enemy.lua       the update script
---        enemy.emd       the model, or for a monster (0x00..=0x15) one per
---        enemy_0.emd     scenario: enemy_0 for Chris and enemy_1 for Jill,
---        enemy_1.emd     falling back to enemy.emd
---        sound/
---            00_00.wav   style 00 (the default), slot 00
---            01_00.wav   style 01, slot 00
---lib/
---    npc.lua             the scripted-character driver
---    zombie.lua          the zombie machine shared by 0x00, 0x01 and 0x11
---```
---
---[`e:play_enemy_sound(slot)`](#EnemyApi.play_enemy_sound) plays
---`sound/<style>_<slot>.wav` (both two lowercase hex digits) from the
---enemy's folder. A style only ships the slots it changes; any other slot
---falls back to style `00`. A script picks its style and borrows another
---set with the global [`sounds`](#EnemySounds) declaration.
---
---Enemy sounds are not also packed under `se/`. The exception is a sound a
---room script plays through the room table (`se_play_3d(SND_BANK_ROOM, ..)`),
---which keeps its `se/` copy: today room 309's `He_Nout` and room 706's
---`HU_att`.
---
---The base game's scripts live in the engine repository under
---`games/re1/enemy/` and `games/re1/lib/` and are packed by `convert-game`.
---A mod overrides any one of them by carrying an `enemy/{id:02x}/enemy.lua`
---(or model) entry of its own: pack layering is the override mechanism, as
---for any other entry.
---
---## Shared modules
---
---Shared behaviour lives once, in a module under `lib/`, and each id's script
---is a thin file carrying that id's data. The scripted characters show the
---pattern: every id calls the shared driver in `lib/npc.lua`, where the
---state-0 spawn init, the state-1 idle behaviours, the state-8 scripted-action
---handlers and the state-9 follow/pathfind driver live:
---
---```lua
----- enemy/23/enemy.lua
---local npc = require("/lib/npc")
---local data = {
---    radius = 372,
---    corpse = false,
---    idle0_plays = false,
---    rebecca = true,
---    wesker = false,
---    sca = { 372, 0x5FA, 0, -0x5FA, 0 },
---}
---
---function update(e)
---    npc.update(e, data)
---end
---```
---
---A pack can override one character's data without touching the shared
---driver, or shadow `lib/npc.lua` wholesale to replace the behaviour for all
---of them. See [`require`](#require) for how module names resolve.
---
---## Statelessness
---
---**Scripts must keep no mutable state of their own.** Every value a script
---reads or writes between ticks lives on the Rust entity (or in the game state
---behind `e`); the VM holds only the compiled functions and the module cache.
---The engine may drop and recreate the whole VM between any two updates, which
---re-runs every chunk and every `require`, and behaviour must stay identical;
---the test suite runs whole scripted scenes with and without a reset on every
---tick and compares the results. Module-level tables of constants are fine;
---a module-level counter or cache is not. The per-monster scratch views below
---exist so that a script never needs one.
---
---Each property getter returns the field's own width (an `i16` reads back
---signed, a `u16` unsigned) and each assignment truncates to it exactly like
---the original's C store, so a script may compute in 64-bit integers and
---assign without narrowing by hand. Read-only properties have no setter and
---raise an error on assignment, like any missing field.
---
---## Determinism
---
---Two random sources exist, and a script must use the one the original used:
---
---- [`e:random()`](#EnemyApi.random) draws from the platform `rand()` stream,
---  which every spawn, clone and debris roll advances.
---  [`e:srand(seed)`](#EnemyApi.srand) reseeds it before a scripted burst.
---- `e.rand_seed` is the per-frame snapshot (`g_RandSeed`) the look-at,
---  wander and many init-time picks read without consuming the stream.
---  [`e:lfsr_step()`](#EnemyApi.lfsr_step) steps it in place, and a write
---  overwrites it for the rest of the frame.
---
---No clock, no `math.random` and no other entropy are reachable, so a capture
---or headless run with scripts stays deterministic.
---
---## Sandbox and errors
---
---The enemy scripts run in their own per-session VM with the same sandbox as
---the [Lua hooks](hooks.md): the base, `string` and `table` libraries only
---(no `os`, `io`, `math`, `debug`, `coroutine` or `package`), `load`,
---`loadfile`, `dofile` and `collectgarbage` removed, a 16 MiB allocation
---ceiling and an instruction budget per call. The language is Lua 5.4, so
---integers, `//` and the bitwise operators are available without `math`. A
---chunk is at most 1 MiB and a VM loads at most 256 modules. Each chunk runs
---in its own environment table that falls back to the VM's globals, so two
---scripts' globals never collide.
---
---A script that fails to load, defines no `update` function, or whose call
---raises an error (including a failed `require`, a budget overrun or a
---wrong argument type) is logged once on stderr with an `[enemy]` prefix and
---parks that entity id for the rest of the session; the other ids keep
---running. A pack with no enemy scripts loads no VM at all.
---
---Method arguments documented as `integer` take Lua integers (a float with an
---exact integer value also converts) and are truncated to the width the
---original used, so `-1` and `0xFFFF` are the same 16-bit word. Arguments
---documented as `boolean` follow Lua truthiness: only `nil` and `false` are
---false, so pass a comparison (`code ~= 0`), not a number.
---
---Positions and distances are room units; angles are the game's 12-bit
---angles, `0x1000` a full turn. RE 2's enemies use the same folders and
---host but a different `e`; their surface lives in `src/zombie2/script.rs`,
---and none of the calls on this page exist there.

-- # Entry points

---Called once per fixed tick for every active entity slot whose id this
---script's folder names (for a scripted character, only in states 0, 1, 8 and
---9). This is the only function the engine calls; a chunk that defines no
---global `update` parks its id.
---@param e EnemyApi The entity being updated, valid for this call only.
function update(e) end

---The sandboxed, pack-scoped `require` every enemy chunk and module gets in
---its environment. It loads any `.lua` entry of the game pack, executes it
---once per VM and caches its return value under the resolved entry, so every
---later `require` of the same module returns the same value. A module that
---returns nothing caches `true`.
---
---| Name form | Resolves to |
---| --- | --- |
---| `/lib/npc` | Absolute from the pack root. |
---| `./npc`, `../0c/enemy`, `../../lib/npc` | Relative to the directory of the requiring module. |
---| `lib/npc` | The exact pack entry if it exists, otherwise relative to the requiring module. |
---
---The `.lua` suffix is optional, paths are case-insensitive, and `..` may not
---escape the pack root. A missing module, a require loop or the 256-module
---limit raises an error, which parks the requiring id like any other error.
---There is no filesystem access: only pack entries can be reached.
---@param name string A pack path, `.lua` optional.
---@return any module The module's return value, or `true` when it returned nothing.
function require(name) end

---The optional global `sounds` declaration of an `enemy.lua`, which the
---engine reads once when the script loads. It picks where
---[`e:play_enemy_sound`](#EnemyApi.play_enemy_sound) looks after the enemy's
---own `sound/` folder, and the style each room plays:
---
---| Declaration | Meaning |
---| --- | --- |
---| `sounds = "03"` | After its own folder, fall back to `enemy/03/sound/`. |
---| `sounds = { base = "00", styles = { [0x103] = 0x02 } }` | The same with `enemy/00`, and play style `02` in room `0x103` (`stage << 8 \| room`). |
---
---A base naming another id (two lowercase hex digits) follows that id's own
---declaration in turn, up to eight sets. A base starting with `/` is a folder
---from the pack root (`/shared/beast` reads `shared/beast/sound/`); any other
---base is a folder under `enemy/`. The style comes from the first set in the
---chain that declares `styles`. The first existing file plays, and none means
---silence. A declaration of any other type is logged and ignored.
---@alias EnemySounds string|EnemySoundsTable

---The table form of [`sounds`](#EnemySounds).
---@class EnemySoundsTable
---@field base string? The set to fall back to: an enemy id in two lowercase hex digits, a `/`-rooted pack folder, or a folder under `enemy/`.
---@field styles table<integer, integer>? Style id per room, keyed `stage << 8 | room`.
local EnemySoundsTable = {}

---@type EnemySounds?
sounds = nil

-- source: src/enemy/lua.rs#EnemyApi
---The entity an [`update`](#update) call drives, as a scoped userdata over the
---live game state. Call its methods with colon syntax (`e:move(0, 40)`). Its
---properties are listed in the tables after the methods:
---[the entity record](#EnemyEntity), [the shared behaviour scratch](#EnemyScratch),
---[the room and session state](#EnemyWorld), [the player](#EnemyPlayer), and
---the per-monster views of the scratch block ([zombie](#EnemyZombieViews),
---[adder](#EnemyAdderViews), [wasp](#EnemyWaspViews), [crow](#EnemyCrowViews),
---[spiders](#EnemySpiderViews), [hound](#EnemyHoundViews),
---[chimera](#EnemyChimeraViews), [hunter](#EnemyHunterViews),
---[monster plant](#EnemyMonsterPlantViews), [computer arms](#EnemyArmViews),
---[Plant 42](#EnemyPlant42Views), [Yawn](#EnemyYawnViews) and
---[Tyrant](#EnemyTyrantViews)).
---
---The methods are the composite steps the original's own monster code is
---built from: anything that needs the model's clips, the room's collision,
---joint matrices, other slots or the effect pool is a method, so the original
---arithmetic (16-bit wraps, fixed-point rounding, shared scratch side effects)
---stays in Rust and every script applies it identically.
---@class EnemyApi : EnemyEntity, EnemyScratch, EnemyWorld, EnemyPlayer, EnemyZombieViews, EnemyAdderViews, EnemyWaspViews, EnemyCrowViews, EnemySpiderViews, EnemyHoundViews, EnemyChimeraViews, EnemyHunterViews, EnemyMonsterPlantViews, EnemyArmViews, EnemyPlant42Views, EnemyYawnViews, EnemyTyrantViews
local EnemyApi = {}

-- # Animation and pose

---Advance the entity's animation clock one tick, exactly like the original's
---`Joint_move`. While `timing_control` is above 1 the frame is held;
---otherwise the pending frame is applied, its hold reloaded and the clip
---advanced, wrapping after its last frame. `blend_counter` is consumed as the
---blend state on every consuming tick.
---@param step integer The blend step that scales the pose interpolation (`0x1000` a full step; see [`e:blend_step()`](#EnemyApi.blend_step)).
---@param reverse? boolean Read the frame data backwards. Defaults to `false`.
---@return boolean completed Whether the clip just wrapped.
function EnemyApi:advance_anim(step, reverse) end

---The walk layer's blend step for the entity's current `blend_counter`:
---`0x1000 / (blend_counter + 1)`.
---@return integer step
function EnemyApi:blend_step() end

---The original's `ResetJointTransforms`: restart the clip clock at the bind
---pose and clear every scripted joint state (the hidden-joint mask, armed
---joints, gore flags, blood scratch, joint trails and joint tints). The model's
---keyframes and skeleton stay installed.
function EnemyApi:reset_joints() end

---Show or hide one joint of the model. Joints `0..32` only; any other index is
---ignored.
---@param joint integer
---@param visible boolean
function EnemyApi:set_joint_visible(joint, visible) end

---One joint's flag byte: bit 0 is the active bit (clear means hidden), bits
---1-7 the gore state the death paths read (severed limbs, blown head).
---@param joint integer
---@return integer flags
function EnemyApi:joint_flag(joint) end

---Store one joint's flag byte: bit 0 drives the hidden mask, bits 1-7 the
---gore state.
---@param joint integer
---@param value integer
function EnemyApi:set_joint_flag(joint, value) end

---The original's `joint_setup_attack_effect`: arm one joint's attack effect.
---The joint is hidden and marked armed (its active flag cleared, which
---[`e:leg_reach`](#EnemyApi.leg_reach) and the zombie death paths read) and
---its gore strip starts. Joints `0..32` only.
---@param joint integer
---@param kind integer The gore strip kind.
---@param timer integer The strip's countdown.
---@param frame integer The strip's start frame.
function EnemyApi:arm_joint_effect(joint, kind, timer, frame) end

---The original's `entity_apply_anim_vertex`: set the entity's X/Z from the
---displayed clip frame's root vertex, rotated by the yaw, plus the
---`unk_c6`/`unk_c8` grab offsets. Y is left alone.
function EnemyApi:apply_anim_vertex() end

---The original's `EntityUpdateWeaponJoint`: shift the entity's X/Z so this
---pose's held-weapon hand lands where the previous frame's hand was (the
---held-weapon root-motion compensation of the SCD state-8 tail).
---@param hand integer The weapon chain: `0` or `1` (`scd_entity_flags` bit 3).
function EnemyApi:update_weapon_joint(hand) end

---Refresh the live look-at target (an entity or record target follows its
---position) and slew the tracking joint's yaw and pitch toward it: the
---character driver's shared tail. The pitch reads the tracking joint's world
---translation from the previous update.
function EnemyApi:update_look_at() end

---Look at nothing: reseed the wander countdown from `e.rand_seed`, clear the
---wander state and park the look-at fields.
function EnemyApi:lookat_reset() end

---Look at a point, with the same reseed as
---[`e:lookat_reset()`](#EnemyApi.lookat_reset).
---@param x integer
---@param z integer
function EnemyApi:lookat_target(x, z) end

---The random look-at wander: count the reseed down and, when it expires,
---repaint the look-at angles from `e.rand_seed`, always differing from the
---previous pick.
---@param param integer Selects the draw mask; the behaviours pass the id parity.
function EnemyApi:lookat_wander(param) end

---The X world translation of one of the entity's joints, from the pose the
---previous update left (the original's `joints[i].world.t`).
---@param joint integer
---@return integer x `0` when the joint does not exist.
function EnemyApi:joint_world_x(joint) end

---The Y world translation of one of the entity's joints; see
---[`e:joint_world_x`](#EnemyApi.joint_world_x).
---@param joint integer
---@return integer y `0` when the joint does not exist.
function EnemyApi:joint_world_y(joint) end

---The Z world translation of one of the entity's joints; see
---[`e:joint_world_x`](#EnemyApi.joint_world_x).
---@param joint integer
---@return integer z `0` when the joint does not exist.
function EnemyApi:joint_world_z(joint) end

---The original's `ApplyMatrixLV`: rotate a vector by a joint's world matrix
---(translation ignored, with the game's Y-sign conjugation and per-term
---truncation). The grabs use it for their lift offset.
---@param joint integer
---@param x integer
---@param y integer
---@param z integer
---@return integer x `0, 0, 0` when the joint does not exist.
---@return integer y
---@return integer z
function EnemyApi:apply_matrix_lv(joint, x, y, z) end

-- # Collision volume, shadow and switch zone

---Set the entity's whole SCA collision volume record, which the shared
---separation pass reads. It also drops a second volume set with
---[`e:set_sca2`](#EnemyApi.set_sca2).
---@param radius integer Also the `sca_radius` property.
---@param half_height integer Also the `sca_half_height` property.
---@param offset_x integer The volume's local centre.
---@param offset_y integer
---@param offset_z integer
function EnemyApi:set_sca(radius, half_height, offset_x, offset_y, offset_z) end

---Add a second SCA profile volume (the Black Tiger's two-box walk record).
---The room collision still reads only the first volume's radius, like the
---original.
---@param radius integer
---@param half_height integer
---@param offset_x integer
---@param offset_y integer
---@param offset_z integer
function EnemyApi:set_sca2(radius, half_height, offset_x, offset_y, offset_z) end

---The per-frame SCA hit retarget: a world-space bias added to the primary
---volume's rotated centre. The bosses write the posed joint's offset from the
---body every gated frame to move a segmented hitbox onto a limb; `0, 0, 0`
---restores the record.
---@param dx integer
---@param dy integer
---@param dz integer
function EnemyApi:set_sca_hit_point(dx, dy, dz) end

---Set the ground-shadow quad's local offset. Its extents and tint are the
---`shadow_half_x`, `shadow_half_z` and `shadow_tint` properties.
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:set_shadow_offset(x, y, z) end

---The original's `BillboardAdjSize` on the ground quad: add signed deltas to
---the shadow's half extents, with 16-bit wrap.
---@param half_x integer
---@param half_z integer
function EnemyApi:adjust_shadow_size(half_x, half_z) end

---Recompute whether the entity stands in the current camera's switch zone,
---store it in `has_enter_switch_zone` and return it. The renderer hides an
---entity that has not entered the zone.
---@return integer zone `1` inside, `0` outside.
function EnemyApi:update_switch_zone() end

---[`e:update_switch_zone()`](#EnemyApi.update_switch_zone) that keeps bit
---`0x80` of `has_enter_switch_zone` (the death-park bit) and replaces only
---the zone bit.
---@return integer zone `1` inside, `0` outside.
function EnemyApi:update_switch_zone_keep_high() end

-- # Movement and steering

---The original's `Add_speedXZ`: move `distance` units along the yaw plus
---`offset`, with no collision test (a scripted walk crosses geometry exactly
---like the original; the driver's own collision pass pushes it back). The
---rotated vector is stored in the `speed_x`/`speed_y`/`speed_z` words.
---@param offset integer Added to the yaw for this step.
---@param distance integer
function EnemyApi:move(offset, distance) end

---The characters' `MovePlayerXZ` step: move `distance` along the yaw through
---the full `RotMatrix`, which rounds differently from
---[`e:move`](#EnemyApi.move), and leave the stored speed vector alone.
---@param distance integer
function EnemyApi:walk_xz(distance) end

---Re-add the stored `Add_speedXZ` vector without recomputing the yaw, so a
---direction fixed at launch keeps carrying the entity while it turns.
function EnemyApi:advance_speed() end

---[`e:move(offset, distance)`](#EnemyApi.move) followed by a room-collision
---test of the new point at the entity's radius; a blocked point rolls the
---step back.
---@param offset integer
---@param distance integer
---@return boolean moved `false` when the step was rolled back.
function EnemyApi:try_move(offset, distance) end

---Store the current position as the collision-accepted rollback point (the
---original's `position` word, read back as `prev_pos_x`/`y`/`z`).
function EnemyApi:save_pos() end

---The original's `entity_apply_walk_speed`: set `move_speed_current`, then
---trim it on the clip's planted-foot frames so the walk does not slide.
---@param speed integer
function EnemyApi:apply_walk_speed(speed) end

---The turn the original's `turn_toward_target` returns, for the caller to add
---to the yaw.
---@param x integer
---@param z integer
---@param step integer
---@return integer delta `0` when aligned within `2 * step`, otherwise `step` or `-step` along the shorter turn.
function EnemyApi:turn_toward_target(x, z, step) end

---The original's `entity_rotate_toward_target`: turn the yaw toward a point
---by at most `step`, snapping when under two steps remain.
---@param x integer
---@param z integer
---@param step integer Bit 15 flips the direction (face away from the point).
function EnemyApi:rotate_toward_target(x, z, step) end

---The characters' `npc_walk_turn_toward_heading`: step the yaw toward a
---heading, snapping once the difference is within the step. Odd entity ids
---turn eight units faster.
---@param heading integer
---@param step integer
function EnemyApi:turn_toward_heading(heading, step) end

---Whether a heading lies within `half` of the entity's yaw.
---@param heading integer
---@param half integer
---@return boolean
function EnemyApi:heading_within(heading, half) end

---The XZ distance from the entity to a point, rounded down.
---@param x integer
---@param z integer
---@return integer distance
function EnemyApi:xz_distance_to(x, z) end

---The 12-bit XZ angle from the entity to a point.
---@param x integer
---@param z integer
---@return integer angle
function EnemyApi:angle_to(x, z) end

---The original's `entity_swerve_around_obstacle`, steering toward the
---player: a newly blocked entity picks a 90-degree sidestep and arms the
---latch, a latched one holds its turn while frames remain. Updates the
---`crow_swerve`/`crow_swerve_latch` scratch.
---@param step integer The magnitude of the held turn.
---@param blocked boolean Whether this frame's room resolve was blocked (its code was nonzero).
---@param seed integer The frame count a fresh swerve holds (the crow passes 4).
---@return integer delta The yaw delta to add this frame.
function EnemyApi:swerve(step, blocked, seed) end

---The original's `entity_update_wander_turn`, the zombies' stuck-detection
---steering: steer at the stored waypoint, updating `dir_control_flags` and
---the counter at `+0x179` (`zombie_unk_179`) in place.
---@param movement_dist integer The distance moved, the word at `+0x17A`.
---@param angle_step integer The turn magnitude; bit 15 flips the direction.
---@param turn_limit integer
---@return integer owned `1` while the flags' `0x80` path owns the turn, `0` while steering at the waypoint.
function EnemyApi:wander_turn(movement_dist, angle_step, turn_limit) end

---[`e:wander_turn`](#EnemyApi.wander_turn) with the hunter's fields: the
---control byte is `hunter_room_hit` and the counter the low byte of
---`hunter_path_latch`.
---@param movement_dist integer
---@param angle_step integer
---@param turn_limit integer
---@return integer owned
function EnemyApi:hunter_wander_turn(movement_dist, angle_step, turn_limit) end

---[`e:wander_turn`](#EnemyApi.wander_turn) with the Tyrant's fields: the
---control byte at `+0x16C` (`attacking_direction`) and the counter at
---`+0x17E` (`behavior_step`).
---@param movement_dist integer
---@param angle_step integer
---@param turn_limit integer
---@return integer owned
function EnemyApi:tyrant_wander_turn(movement_dist, angle_step, turn_limit) end

---[`e:wander_turn`](#EnemyApi.wander_turn) with Neptune's fields: the
---movement distance is read from its own room-hit word (`sca_touch`, the
---distance [`e:probe_ahead_code`](#EnemyApi.probe_ahead_code) latched), the
---control byte is `splatter_flag` and the counter `angle_turn_delta`.
---@param angle_step integer
---@param turn_limit integer
---@return integer owned
function EnemyApi:neptune_wander_turn(angle_step, turn_limit) end

---One frame of the shared projectile arc: walk `fwd` along the yaw and
---subtract `vy0 + air_ticks * gravity` from Y with 16-bit wrap, where
---`air_ticks` is `death_timer`, counted while the entity is above `ground`
---(Y grows downward).
---@param fwd integer
---@param vy0 integer
---@param gravity integer
---@param ground integer
---@return integer landing The landing velocity, `0` while airborne.
function EnemyApi:ballistic(fwd, vy0, gravity, ground) end

---The hound's spawn nudge: rotate `distance` by the entity's full rotation
---and add the X/Z components to the accepted `position` words
---(`saved_x`/`saved_z`); the shared `scratch_x`/`scratch_z` vector picks up
---the step too.
---@param distance integer
function EnemyApi:nudge_spawn(distance) end

---The spiders' `leg_reach`: the stride length of one leg pair, from the
---current pose composed against the previous frame's tip joint, stored in
---`move_speed_current`. An armed joint (see
---[`e:arm_joint_effect`](#EnemyApi.arm_joint_effect)) zeroes the stride. The
---Black Tiger (id `0x04`) and the WebSpinner chain their legs differently;
---the id picks the shape.
---@param part integer The leg pair.
---@param scale integer Scales the chain's rotation columns when nonzero (`ScaleMatrixCols`).
function EnemyApi:leg_reach(part, scale) end

---The original's `zombie_body_part_physics`: derive the crawl speed from the
---posed leg chain the `param` nibble selects (joints `3 + 3*param` to
---`5 + 3*param`) and store it in `move_speed_current`.
---@param param integer
function EnemyApi:body_part_speed(param) end

---Queue the footstep sound for the current animation frame: the floor zone's
---sound of `sound_type`, at the entity's position. A room with no matching
---floor zone stays silent.
---@param sound_type integer
function EnemyApi:footstep(sound_type) end

-- # Room collision and probes

---The room-collision resolve and accept: push the entity out of the walls or
---roll X/Z back to the stored position, then store the result as the new
---accepted position.
---@return integer code `0` clear, `1` pushed, `2` rolled back, `3` a floor/step zone crossed.
---@return integer floor_step The crossed floor step (also `e.floor_step`), the flying crow's ground reference.
---@return integer distance The distance actually travelled since the previous accepted position (the original's `g_tempVar`).
function EnemyApi:resolve_collision() end

---`check_room_collision` at an explicit radius (Neptune's devour drag uses
---500). Clears `collision_flags` bit 3 and does nothing while `status_flags`
---bit 2 (hidden) is set; otherwise pushes or rolls back and accepts like
---[`e:resolve_collision()`](#EnemyApi.resolve_collision).
---@param radius integer
---@return integer code `0` for a hidden entity.
---@return integer dx The X delta the resolve applied.
---@return integer dz The Z delta the resolve applied.
function EnemyApi:resolve_collision_radius(radius) end

---The SCA separation pass: the player pair first, then every other entity
---with a status bit, in slot order.
---@return boolean touched Whether the entity touched the player's volume, the value the attacking monsters park as their touch word.
function EnemyApi:separate() end

---Whether a point is inside the room's blocking geometry for the entity's
---radius and collision flags.
---@param x integer
---@param y integer
---@param z integer
---@return boolean
function EnemyApi:position_blocked(x, y, z) end

---The hound's forward probe: step `distance` along the facing, resolve the
---room at `radius` and step back. The resolve is destructive (it may push the
---position and advances the accepted `position` words), like the original;
---[`e:probe_turn`](#EnemyApi.probe_turn) restores everything around it.
---@param distance integer
---@param radius integer
---@return boolean hit Whether the resolve reported a push, rollback or floor crossing.
function EnemyApi:probe_ahead(distance, radius) end

---[`e:probe_ahead`](#EnemyApi.probe_ahead) returning the raw result: the
---shark's nose probe ORs the code into `splatter_flag` and latches the
---distance in its room-hit word.
---@param distance integer
---@param radius integer
---@return integer code The resolve's result code.
---@return integer travelled The distance its accept recorded.
function EnemyApi:probe_ahead_code(distance, radius) end

---The hound's turn probe: "would I hit a wall if I turned by `delta` and ran
---at `mul` times my speed?" The scaled step and probe run for real, then the
---position, accepted `position` words and speed are restored.
---@param delta integer The yaw offset.
---@param mul integer The speed multiplier.
---@return boolean blocked
function EnemyApi:probe_turn(delta, mul) end

---The original's `check_room_collision_two_point`, the prone body's floor
---probe: rotate two body-local endpoints by the yaw and test them (end B
---first); a still-blocked end A rolls the position and angle back.
---@param ax integer End A, body-local.
---@param az integer
---@param bx integer End B, body-local.
---@param bz integer
---@return integer flags End B's flag bits (`0`/`1`/`2`) on acceptance, `0x80` after a rollback, `0` for an inactive entity.
function EnemyApi:two_point_probe(ax, az, bx, bz) end

---The Tyrant's room-action probe (`update_player_position(ENTITY, 2)`). It is
---only observable in the heliport and ending sequence, which the port does
---not model, so the call does nothing.
---@param mask integer Ignored.
function EnemyApi:probe_room_actions(mask) end

-- # Zones and pathfinding

---Find the walk zone containing the entity, store it in `splatter_flag` and
---return it.
---@return integer zone `0xFF` when none.
function EnemyApi:update_walk_zone() end

---One tick of the obstacle pathfinder (`entity_pathfind_update`) against a
---target: `pathfind_state` cycles a 5-bit counter plus the line-of-sight bit,
---and on the counter's third frame with a clear sight line the target is
---latched into `player_pos_x`/`player_pos_z` as the waypoint. Reports
---blocked while the GHOST cheat hides the player.
---@param x integer
---@param z integer
---@return integer result `0` blocked at the waypoint frame, `1` waypoint refreshed, `2` still counting.
function EnemyApi:pathfind_update(x, z) end

---The adder's path-result keep: reduce a
---[`e:pathfind_update`](#EnemyApi.pathfind_update) result to bit 0 of the
---scratch byte at `+0x16C` (`e.path_bit`), only when no higher bit is set.
---@param path integer
function EnemyApi:pathfind_keep(path) end

---The wasp's path-result keep: the same reduction into bit 0 of the byte at
---`+0x16E` (`e.wasp_path_bit`), which the hover and victory behaviours gate
---on.
---@param path integer
function EnemyApi:wasp_path_keep(path) end

---The WebSpinner's and Black Tiger's path-result keep: the same reduction
---into the `+0x16E` word (`ws_path_word`/`bt_path_word`).
---@param path integer
function EnemyApi:spider_path_keep(path) end

---The original's `walk_zone_shared_edge`: the midpoint of the edge two walk
---zones share. Zones that share no edge fall through to B's `z1` as the
---crossing Z, like the original.
---@param a integer
---@param b integer
---@return integer flag `0` a shared X edge (span in Z), `1` a shared Z edge (span in X).
---@return integer x The midpoint.
---@return integer z
function EnemyApi:zone_shared_edge(a, b) end

---The original's `zone_path_find`: walk the zone graph from the entity's zone
---toward a target with its distance-weighted ring walks. A target with
---`z == 0` names a zone index in `x` and aims at that zone's midpoint.
---@param x integer
---@param z integer
---@return integer kind `0` same zone (walk straight), `1` cross into another zone, `2` unreachable.
---@return integer from For `0` the shared zone; for `1` the entity's zone; otherwise `0`.
---@return integer next For `1` the next zone to walk into; otherwise `0`.
---@return integer crossing_x For `1` the shared-edge crossing; otherwise `0`.
---@return integer crossing_z
function EnemyApi:zone_path_find(x, z) end

---The zombie distance driver's out-parameter form of
---[`e:zone_path_find`](#EnemyApi.zone_path_find): write the steered waypoint
---(the target itself, or the shared-edge crossing) into
---`player_pos_x`/`player_pos_z`. An unreachable target, or a player the GHOST
---cheat hides, writes nothing.
---@param x integer
---@param z integer
function EnemyApi:zone_path_update(x, z) end

---The original's corridor test: whether `(x, z)` sits inside the overlapping
---span of two zones' shared edge for a mover of the entity's radius, with the
---span ends nudged inward where a wall probe hits them.
---@param flag integer The edge orientation [`e:zone_shared_edge`](#EnemyApi.zone_shared_edge) returned.
---@param x integer
---@param z integer
---@param from integer
---@param next integer
---@return boolean
function EnemyApi:corridor_open(flag, x, z, from, next) end

---The original's `zone_crossing_heading`: clamp the entity's own position into
---the corridor between zone `from` and zone `next`, and head for it. An
---invalid zone falls back to the entity's position and heading `0`.
---@param from integer
---@param next integer
---@return integer x The clamped waypoint.
---@return integer y
---@return integer z
---@return integer heading The 12-bit heading toward it.
function EnemyApi:crossing_heading(from, next) end

-- # Perception

---The player's live position.
---@return integer x
---@return integer y
---@return integer z
function EnemyApi:player_pos() end

---The player character's collision radius.
---@return integer radius
function EnemyApi:player_radius() end

---Whether the GHOST debug cheat hides the player from this entity (only
---hostile ids below `0x20` lose track). Port-only, for branches that read the
---player's position directly; the perception methods below already honour it.
---@return boolean
function EnemyApi:player_hidden() end

---The shared `is_facing_toward_entity` test: whether the player's yaw points
---at this entity, within half a turn.
---@return boolean
function EnemyApi:player_facing_entity() end

---The original's `entity_check_visual_range`: the flat distance to the
---player, raising `status_flags` bit 5 (`0x20`) when it is inside `range`.
---While the GHOST cheat hides the player the bit is cleared and the distance
---reads `0x7FFF`.
---@param range integer
---@return integer distance
function EnemyApi:check_visual_range(range) end

---[`e:check_visual_range`](#EnemyApi.check_visual_range) raising (or, for a
---hidden player, clearing) `status_flags` bit 7 (`0x80`) instead.
---@param range integer
---@return integer distance
function EnemyApi:check_alert_range(range) end

---The zombie's `checkAngularViewAndDistance`: whether the player is inside the
---entity's wedge of `fov_half` either side of its yaw and within
---`max_distance` (a folded Manhattan range).
---@param fov_half integer
---@param max_distance integer
---@return integer seen `1` when both hold, otherwise `0`.
function EnemyApi:angular_view_and_distance(fov_half, max_distance) end

---The boundary sight-ray test from the entity to the player. Like the
---original, it also stores its result in `e.player_distance_z`, which the
---hunter reads back.
---@return integer blocked `0` clear, `1` a fully blocking boundary crosses it (always `1` for a hidden player).
function EnemyApi:line_of_sight() end

---The spiders' player distance (`ws_dist`): the flat Manhattan sum of the two
---axis differences, `0x7FFF` for a hidden player.
---@return integer distance
function EnemyApi:web_distance() end

---The square joint-reach box the monsters test against the player: compose
---the local offset onto the joint's world matrix and test the player against
---a `±radius` square in XZ. Height never participates, and both points wrap
---through 16 bits like the original.
---@param joint integer
---@param x integer The local probe offset.
---@param y integer
---@param z integer
---@param radius integer The box half-extent.
---@return boolean reached `false` when the joint does not exist.
function EnemyApi:reach_test(joint, x, y, z, radius) end

---The hound's head tracking: add the accumulated head yaw to the neck chain,
---aim the head at the player, integrate the turn into `cb_swerve` and clamp it
---to `±0x100`.
---@return boolean clamped Whether the clamp applied.
function EnemyApi:head_track() end

---The world position of one of the player's joints, from the pose the last
---frame drew.
---@param joint integer
---@return integer x The player's position before the first draw or for a missing joint.
---@return integer y
---@return integer z
function EnemyApi:player_joint_world(joint) end

-- # Attacking the player

---The shared player-hurt helper: subtract `damage` from the player's health
---with the original's 16-bit wrap and mirror it into the BioCard copy. This
---entity becomes the owner of the player's reaction clips.
---@param damage integer
---@param flags? integer `0x01` clamps a killing blow to 1; with it, `0x02` resolves a flagged lethal blow to the scripted `-1` death (the big wasp's sting). Defaults to `0`.
---@return integer health The written health.
function EnemyApi:hurt_player(damage, flags) end

---Raise the poison status bit (`0x02`) and restart the 150-frame poison timer.
function EnemyApi:poison_player() end

---The wasp's grab pose: both the player's and this entity's animation offsets
---latch to the player's X/Z, the attacked flag is set and the player enters
---the grabbed animation (state 5, frame id 7, the crawl pin).
function EnemyApi:grab_player() end

---Write the player's `animationId`/`animFrameId` bytes at `+0x84` (the state
---and ignore bytes that select the reaction window), as the original's dword
---store does. See [the reaction windows](#EnemyPlayer).
---@param id integer The player state (`animationId`).
---@param frame integer The window entry (`animFrameId`).
function EnemyApi:set_player_animation(id, frame) end

---Write the player's action-behaviour and action-state bytes (for example the
---sting kill's behaviour `200`).
---@param behavior integer
---@param state integer
function EnemyApi:set_player_action(behavior, state) end

---The original's `GetPlayerInputMasked`: whether any d-pad direction or face
---button went down this frame (the crow grab's struggle-shake gate). Like the
---original's edge-detected pad word, a button held from an earlier frame does
---not count.
---@return boolean
function EnemyApi:player_mashing() end

---The original's `reduce_attack_time_by_btn_press`: how many frames the
---controls pressed this frame shorten the zombie bite: 3 for a d-pad
---direction, 2 more for a face button.
---@return integer frames `0`, `2`, `3` or `5`.
function EnemyApi:mash_reduce() end

---The original's `snap_player_to_grab_position`: pin this entity's and the
---player's `unk_c6`/`unk_c8` offsets to the root-motion vertex's world
---position, the bite anchor. Does nothing when the clip has no root vertex.
function EnemyApi:snap_grab() end

---[`e:snap_grab()`](#EnemyApi.snap_grab) aimed at a non-player victim: the
---offsets are mirrored onto entity slot `target` instead of the player.
---@param target integer The victim's entity slot.
function EnemyApi:snap_grab_slot(target) end

---Set the player's position directly (the plant's hold behaviours).
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:set_player_pos(x, y, z) end

---Store the grab speed vector
---[`e:add_player_speed()`](#EnemyApi.add_player_speed) adds to the player.
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:set_player_speed(x, y, z) end

---Add the stored grab speed vector to the player's position, with wrap.
function EnemyApi:add_player_speed() end

---Add this entity's stored `Add_speedXZ` vector (`speed_x`, `speed_z`) to the
---player's X/Z: the hunter's swipe recovery drags the grabbed player along.
function EnemyApi:drag_player() end

---The Tyrant's `tyrant_latch_player_attacker`: make this entity the player's
---attacker and store the player's facing latch (`attackAnim` 1 when the
---player faces the entity with the yaw bias applied, else 0). The entity's
---own yaw is unchanged.
---@param yaw_bias integer Added to the entity's yaw for the facing test only.
function EnemyApi:latch_player_attacker(yaw_bias) end

---The bare attacker latch (`e.player_attacker = this slot`, the impale's
---store), with no facing write.
function EnemyApi:set_player_attacker() end

---Raise the hunter's pounce-grab one-shot flag (`e.hunter_grab_one_shot`),
---which the death screen consumes.
function EnemyApi:raise_grab_one_shot() end

---The shared special-weapon check: when the player's equipped item is in the
---high special range (`> 0x6E`) and the current animation frame lands on a
---five-frame boundary, clear this entity's hit latch so a heavy weapon cancels
---the reaction.
function EnemyApi:check_special_weapon() end

---The hunter's `hunter_track_player_joint`: recompose the grab chain around
---the mouth joint and store the resulting joint-1 world translation into the
---player's tracked-joint record (read back with
---[`e:entity_joint_track(0)`](#EnemyApi.entity_joint_track)).
---@param sel integer The mouth-joint selector (`hunter_joint_sel`).
---@return boolean tracked `false` when the entity has no pose.
function EnemyApi:track_player_joint(sel) end

---The hunter's `hunter_scd_track_joint`: the same tracking with the scripted
---bite's fixed chain, stored into entity slot 1 (the SCD target).
---@param sel integer
---@return boolean tracked
function EnemyApi:track_target_joint(sel) end

---The hunter's `hunter_recenter_on_joint`: shift the body's X/Z so a falling
---body pivots around the joint that reached the ground instead of sliding
---through it.
---@param which integer `0` the chain ending at joint 12, `1` the chain ending at joint 15.
function EnemyApi:recenter_on_joint(which) end

-- # Other entity slots

---The hunter's partner pick: slot 1 (the enemy-list head), or slot 2 when
---this hunter is slot 1. Stores it in `e.hunter_partner`.
---@return integer slot
function EnemyApi:pick_partner() end

---The partner slot's health.
---@return integer health `0` for a missing slot.
function EnemyApi:partner_health() end

---The partner slot's `hit_state`.
---@return integer hit_state `0` for a missing slot.
function EnemyApi:partner_hit_state() end

---The partner slot's `move_speed_current`.
---@return integer speed `0` for a missing slot.
function EnemyApi:partner_move_speed() end

---The partner slot's `status_flags`.
---@return integer status `0` for a missing slot.
function EnemyApi:partner_status() end

---The partner slot's X/Z position.
---@return integer x `0, 0` for a missing slot.
---@return integer z
function EnemyApi:partner_pos() end

---Another slot's joint flag byte (see [`e:joint_flag`](#EnemyApi.joint_flag)).
---Slot `0` is the player.
---@param slot integer
---@param joint integer
---@return integer flags `0` for a missing slot.
function EnemyApi:entity_joint_flag(slot, joint) end

---Store another slot's joint flag byte. Clearing bit 0 on slot `0` hides one
---of the player's limbs.
---@param slot integer
---@param joint integer
---@param value integer
function EnemyApi:set_entity_joint_flag(slot, joint, value) end

---Another slot's four state bytes as one dword (see `e.state_word`).
---@param slot integer
---@return integer word `0` for a missing slot.
function EnemyApi:entity_state_word(slot) end

---Write another slot's four state bytes as one dword.
---@param slot integer
---@param value integer
function EnemyApi:set_entity_state_word(slot, value) end

---Another slot's `status_flags`.
---@param slot integer
---@return integer status `0` for a missing slot.
function EnemyApi:entity_status(slot) end

---Write another slot's `status_flags`.
---@param slot integer
---@param value integer
function EnemyApi:set_entity_status(slot, value) end

---Another slot's health.
---@param slot integer
---@return integer health `0` for a missing slot.
function EnemyApi:entity_health(slot) end

---Write another slot's health.
---@param slot integer
---@param value integer
function EnemyApi:set_entity_health(slot, value) end

---Another slot's position.
---@param slot integer
---@return integer x `0, 0, 0` for a missing slot.
---@return integer y
---@return integer z
function EnemyApi:entity_pos(slot) end

---Another slot's yaw.
---@param slot integer
---@return integer? angle `nil` for a missing slot.
function EnemyApi:entity_angle(slot) end

---Write another slot's yaw.
---@param slot integer
---@param value integer
function EnemyApi:set_entity_angle(slot, value) end

---Another slot's `hit_state`.
---@param slot integer
---@return integer hit_state `0` for a missing slot.
function EnemyApi:entity_hit_state(slot) end

---Write another slot's `hit_state`.
---@param slot integer
---@param value integer
function EnemyApi:set_entity_hit_state(slot, value) end

---Add to another slot's animation grab offsets (`unk_c6`/`unk_c8`, with
---wrap): the Tyrant's scripted impale drags its victim with its own root
---motion.
---@param slot integer
---@param dx integer
---@param dz integer
function EnemyApi:add_entity_anim_offset(slot, dx, dz) end

---The joint-1 world translation a hunter track helper last stored into a
---slot (see [`e:track_player_joint`](#EnemyApi.track_player_joint)).
---@param slot integer
---@return integer x `0, 0, 0` for a missing slot.
---@return integer y
---@return integer z
function EnemyApi:entity_joint_track(slot) end

-- # Flags and events

---Whether a flag bit is set (the original's `Flg_ck`).
---@param bank integer
---@param sel integer The bit.
---@return boolean set `false` for a bank that does not exist.
function EnemyApi:flag_bit(bank, sel) end

---Set a flag bit (the original's `Flg_on`): the plant's combat-progression
---flag and the bosses' other handshake bits.
---@param bank integer
---@param sel integer The bit.
function EnemyApi:raise_flag(bank, sel) end

---Raise the SCD completion flag `scd_anim_param` in the system bank, the way
---a finished scripted-action handler signals the event script waiting on it.
function EnemyApi:raise_scd_flag() end

---Raise the spawn record's death-event bit in the enemy flag bank (the
---original's `Flg_on(g_EnemiesFlags, id)`), which room scripts poll. Does
---nothing for a record with no death event.
function EnemyApi:raise_death_event() end

---Count one dispatch of an `action_behavior` that has no handler in the
---original's table (the `npc_placeholders` record its empty slot leaves
---behind), for the diagnostics.
---@param behavior integer
function EnemyApi:count_placeholder(behavior) end

-- # Randomness

---The next draw from the platform `rand()` stream, for the monsters that
---consume it directly rather than reading the frame seed.
---@return integer value `0..=0x7FFF`.
function EnemyApi:random() end

---Reseed the platform stream (`srand`), so a scripted burst of draws is
---fixed per action.
---@param seed integer
function EnemyApi:srand(seed) end

---One Fibonacci-LFSR step of the frame seed `e.rand_seed` (taps at bits 1
---and 9), the shark's swim pick. It mutates the stored seed, so later readers
---in the same frame see the stepped value.
---@return integer byte The shifted seed's low byte.
function EnemyApi:lfsr_step() end

-- # Sound and voice

---The original's `Snd_em(id)`: queue one of the enemy's own sounds at its
---position, from its folder's `sound/` slots (see
---[`sounds`](#EnemySounds)). The spawn record's sound-group nibble picks the
---bank record, so cues of one group restart each other.
---@param id integer The slot, `0..=9`; any other id, or a record past the 48-entry bank, queues nothing.
function EnemyApi:play_enemy_sound(id) end

---The original's `Play3DSnd(bank, id)` at the entity's position.
---@param bank integer `0` the room's door pair, `2` the room table (the enemy bank), `3` the character table; `1` (weapons) and `4` (the BGM pan channel) queue nothing.
---@param id integer
function EnemyApi:play_3d_sound(bank, id) end

---[`e:play_3d_sound`](#EnemyApi.play_3d_sound) at an explicit position: the
---attacking monsters play their cue at the player.
---@param bank integer
---@param id integer
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:play_3d_sound_at(bank, id, x, y, z) end

---The raw one-shot cue the computer arms use; identical to
---[`e:play_3d_sound`](#EnemyApi.play_3d_sound) for the banks it names.
---@param bank integer
---@param id integer
function EnemyApi:play_sfx(bank, id) end

---Queue the lab terminal's voice line: the id resolves to the stage row's
---positional `voice/<row>_<id>.wav` entry (1-based row, lowercase hex id; a
---mod override must use this name, not the shipped `V004_00`-style basename).
---The caller owns the `e.voice_playing` handshake bit, and an id with no line
---in the pack stays silent.
---@param id integer
function EnemyApi:play_voice(id) end

-- # Effects and tint

---Spawn a billboard attached to this entity's matrix (the idle handlers'
---blood sprays). Blood effects are skipped when the player turned blood off.
---@param type integer The effect type.
---@param depth integer
---@param x integer The local offset.
---@param y integer
---@param z integer
---@param yaw integer
---@param light integer The light factor.
---@return integer? slot The first-frame pool slot, `nil` when nothing spawned.
function EnemyApi:spawn_effect(type, depth, x, y, z, yaw, light) end

---Spawn a billboard anchored to one of this entity's posed joints, with no
---local offset (the retreat's blood sprays).
---@param type integer
---@param depth integer
---@param joint integer
---@param yaw integer
---@return integer? slot
function EnemyApi:spawn_joint_effect(type, depth, joint, yaw) end

---Spawn a billboard anchored to one of this entity's posed joints with a
---local offset (the zombie attack sprays).
---@param type integer
---@param depth integer
---@param joint integer
---@param x integer
---@param y integer
---@param z integer
---@param yaw integer
---@return integer? slot
function EnemyApi:spawn_joint_effect_at(type, depth, joint, x, y, z, yaw) end

---[`e:spawn_joint_effect_at`](#EnemyApi.spawn_joint_effect_at) with an
---explicit light factor (the dust puffs' `0x14`).
---@param type integer
---@param depth integer
---@param joint integer
---@param x integer
---@param y integer
---@param z integer
---@param yaw integer
---@param light integer
---@return integer? slot
function EnemyApi:spawn_joint_effect_lit(type, depth, joint, x, y, z, yaw, light) end

---Spawn a billboard anchored to the player's matrix with a local offset (the
---adder's bite spray).
---@param type integer
---@param depth integer
---@param x integer
---@param y integer
---@param z integer
---@param yaw integer
---@return integer? slot
function EnemyApi:spawn_player_effect(type, depth, x, y, z, yaw) end

---Spawn a billboard anchored to one of the player's joints (a kill's sprays).
---@param type integer
---@param depth integer
---@param joint integer
---@param x integer
---@param y integer
---@param z integer
---@param yaw integer
---@return integer? slot
function EnemyApi:spawn_player_joint_effect(type, depth, joint, x, y, z, yaw) end

---Spawn a billboard at a world point on the identity transform, the shared
---dead-move matrix the gore spawns use.
---@param type integer
---@param depth integer
---@param x integer The world position.
---@param y integer
---@param z integer
---@param yaw integer
---@param light? integer Defaults to `0`.
---@return integer? slot
function EnemyApi:spawn_world_effect(type, depth, x, y, z, yaw, light) end

---Write one header byte of a spawned effect slot (the original's
---`fire_fx_tag`, which tags a flash with the weapon id). A missing slot or
---byte is ignored.
---@param slot integer The pool slot a spawn returned.
---@param field integer The header byte index.
---@param value integer
function EnemyApi:tag_effect(slot, field, value) end

---The original's `blood_splatter_physics` on one joint (the zombie's
---head-joint bleed): step the joint's blood scratch, probe walls and floor,
---and spawn a blood billboard and the impact cue on each hit.
---@param joint integer
---@param gravity integer
function EnemyApi:blood_splatter(joint, gravity) end

---Arm one joint's blood scratch (the vomit attack): its X velocity, Y
---acceleration, gravity counter and bounce/frame flags. The direction's Z word
---keeps what the pose set. The joint index wraps to `0..32`.
---@param joint integer
---@param vel_x integer
---@param vel_y integer
---@param counter integer
---@param flags integer
function EnemyApi:set_joint_blood(joint, vel_x, vel_y, counter, flags) end

---The shared additive model tint (`scd_model_tint_apply`'s enemy path):
---accumulate the deltas into this entity's tint queue record and add them to
---every live enemy with the same id. The channels are signed deltas in units
---of 1/31 from white, clamped to `±31`.
---@param r integer
---@param g integer
---@param b integer
---@param word_a integer The queue record's first parameter word.
---@param word_b integer The second parameter word.
function EnemyApi:tint_model(r, g, b, word_a, word_b) end

---Overwrite and arm the tint queue record without touching the live model
---(the original's `FUN_00473d10`).
---@param r integer
---@param g integer
---@param b integer
---@param word_a integer
---@param word_b integer
function EnemyApi:retarget_tint(r, g, b, word_a, word_b) end

---The original's `JointApplyColorTint`: draw one joint unlit in a packed
---colour. As in the original, only the second argument reaches the tint; the
---last two are ignored.
---@param joint integer
---@param colour integer `r | g << 8 | b << 16`, each channel a 1/128 multiplier.
---@param b integer Ignored.
---@param rgb integer Ignored.
function EnemyApi:tint_joint(joint, colour, b, rgb) end

---[`e:tint_joint`](#EnemyApi.tint_joint) on one of the player's joints: the
---wound a kill leaves on the body.
---@param joint integer
---@param colour integer
---@param b integer Ignored.
---@param rgb integer Ignored.
function EnemyApi:tint_player_joint(joint, colour, b, rgb) end

-- # Spider webs

---The per-type web-joint registry (the original's `ws_web_joints_used`):
---whether joint `chosen` is free, and if so mark it at `index & 7`. The two
---registries (WebSpinner `0x03` and Black Tiger `0x04`) are shared by every
---spider of that id and reset with the room.
---@param chosen integer
---@param index integer
---@return boolean free `false` when the joint already carries a thread, or for any other id.
function EnemyApi:web_joint_use(chosen, index) end

---Shoot this spider's web-thread chain (the original's `ws_clone_entity`):
---drop the previous chain and clone up to `count` threads, each fanned by
---`0x100` of yaw and consuming one platform draw. The count is clamped to the
---free clone slots.
---@param count integer
---@return integer? head The chain head, `nil` when none spawned.
function EnemyApi:web_spawn(count) end

---Tick this spider's web-thread chain (the original's `ws_update_webs`).
---@param count integer The thread count the spider shot.
function EnemyApi:web_update(count) end

-- # Plant 42

---Plant 42's own animator, in place of the shared `Joint_move` clock: it
---always poses the skeleton, decrements `timing_control` and the blend
---counter every tick, sub-frame blends once the counter has run out, and only
---advances the frame when the hold expires.
---@param reverse boolean
---@param blend integer
---@return boolean completed Whether the clip wrapped.
function EnemyApi:plant42_advance(reverse, blend) end

---Build the plant's two companions, the flower body and the root ball, as
---full entity clones outside the enemy list, and store their handles on the
---entity. Consumes the two clone draws from the platform stream. The entity's
---`behavior_flags` picks the set-up: low nibble `8` tints the poison body,
---`4` parks the fallen core, `5` is the poison arena (whose scale the script
---handles).
---@return integer? body The body handle, `nil` when nothing spawned.
---@return integer? roots The roots handle.
function EnemyApi:plant42_spawn() end

---Tick both companion machines in the original's order (the body's
---idle/shrivel/fall, then the roots' idle/pulse/rise) inside the monster
---message gate, and mark them live for the render pass. Does nothing for a
---plant with no companions (a split-vine record).
function EnemyApi:plant42_tick() end

---The body companion's handle: the plant's own clone, or the room-shared body
---for a split-vine record (the original's `scd_target_ptr`).
---@return integer? handle
function EnemyApi:plant42_body() end

---The root-ball companion's handle.
---@return integer? handle
function EnemyApi:plant42_roots() end

---The body's `hit_state` latch.
---@return integer? hit_state `nil` without a body.
function EnemyApi:plant42_body_hit() end

---The body's vine pool (its `+0x70` word), the plant's real health pool.
---@return integer? vines `nil` without a body.
function EnemyApi:plant42_body_vines() end

---Write the body's vine pool.
---@param value integer
function EnemyApi:set_plant42_body_vines(value) end

---The body's own health.
---@return integer? health `nil` without a body.
function EnemyApi:plant42_body_health() end

---Latch the body's damage reaction (`hit_state = 1`, `behavior_flags = 1`).
function EnemyApi:plant42_body_react() end

---Clear the body's damage reaction (`hit_state = 0`, `behavior_flags = 0`).
function EnemyApi:plant42_body_react_clear() end

---The withering sequence's companion writes, one call per plant sub-state.
---@param sub integer `0` arms the shrivel, `2` parks the fallen core, `4` regrows, `6` restores the companions to idle.
function EnemyApi:plant42_body_wither(sub) end

---The plant's first death frame: the body dies and parks on its fall state,
---and the roots park with it.
function EnemyApi:plant42_body_kill() end

---The last-vine payoff: raise `SCENARIO_FLAG_PLANT42_DEAD`, give Jill her
---message bit, and wither every live enemy in slots 1 to 6.
function EnemyApi:plant42_award_kill() end

---A companion's world position.
---@param handle integer
---@return integer x `0, 0, 0` for an unknown handle.
---@return integer y
---@return integer z
function EnemyApi:companion_pos(handle) end

---Spawn a billboard attached to a companion's matrix with a local offset (the
---SCD effect timer's body sprays).
---@param handle integer
---@param type integer
---@param depth integer
---@param x integer
---@param y integer
---@param z integer
---@param yaw integer
---@return integer? slot
function EnemyApi:spawn_companion_effect(handle, type, depth, x, y, z, yaw) end

---Build the shared Plant 42 capture matrix from a grabbing joint's world
---matrix and the player's transform, so the hold can re-apply the player's
---relative transform to the moving joint. Does nothing for a missing joint.
---@param joint integer
function EnemyApi:plant42_capture_setup(joint) end

---Re-apply the capture through a joint to the player: the player's position
---follows the joint, and the grabbed zone bit `0x80` is raised.
---@param joint integer
function EnemyApi:plant42_hold_player(joint) end

---Replace the capture matrix's rotation with one derived from a fixed 3x3
---block (the Chris hold) through the game's integer atan2/sqrt; the
---translation is kept. Fewer than nine values do nothing.
---@param ... integer The nine values in the original's storage order, `m[row + col * 3]`.
function EnemyApi:plant42_capture_orient(...) end

---Store the whole capture translation (the hold writes the lift offset over
---the setup's relative translation).
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:set_plant42_capture_t(x, y, z) end

---Store the capture translation's X, truncated to 16 bits: it doubles as the
---knock-back facing the sweep stores and the thrown player reads (read back as
---`e.plant42_capture_t0`).
---@param value integer
function EnemyApi:set_plant42_capture_t0(value) end

---The shared scale-to-4096 normaliser (`VectorNormal`), with the original's
---double-precision truncation. A zero vector stays zero.
---@param x integer
---@param y integer
---@param z integer
---@return integer x
---@return integer y
---@return integer z
function EnemyApi:vector_normal(x, y, z) end

-- # Yawn

---The head's init back-off: subtract the head joint's local X, rotated by the
---entity's rotation, from the entity's position.
function EnemyApi:yawn_back_off() end

---The init's standard-world pass (`EntityComputeJointWorldMatrices`) over the
---shared clock's pose.
---@param zero_root boolean Model the second pass, where the head joint's local X/Z are cleared.
function EnemyApi:yawn_standard_worlds(zero_root) end

---Lay down the fixed-point joint chain from the frame the clock shows, before
---the twelve segments read their joints (`yawn_pose_init`).
function EnemyApi:yawn_pose_init() end

---One tick of Yawn's own chain animator (`yawn_anim_advance`), in place of the
---shared clock.
---@param reverse boolean
---@param blend integer
---@return boolean wrapped Whether the clip wrapped, the value every caller adds into `action_state`.
function EnemyApi:yawn_anim(reverse, blend) end

---The head's end-of-frame tail: the player SCA pair (skipped for the three
---scripted behaviours), room collision with the wall-stuck counter, and the
---follow-the-leader pass over joints 2 to 14.
---@param step integer The angular step the chain follows with.
function EnemyApi:yawn_post_move(step) end

---Build the twelve body-segment slots from the head's record: each a copy of
---the head with `behavior_flags = 1`, linked to the joint it mirrors and
---counted into the live enemy count. Lua cannot allocate slots, so this is
---the only way to make them.
---@return integer count The number of segments built.
function EnemyApi:yawn_spawn_segments() end

---One body segment's whole frame: mirror its joint, take room collision,
---route damage to the head, drag the chain and refresh the switch-zone bit.
---Every Yawn slot's `update` starts with
---`if e.behavior_flags == 1 then e:yawn_segment_tick() return end`.
function EnemyApi:yawn_segment_tick() end

---The head's batched status write over its enemy-list range
---`first..=last` (`0` the head, `1..=12` the segments):
---`status = (status & and_mask) | or_mask`. The swallow, flee and death
---loops use it.
---@param first integer
---@param last integer
---@param and_mask integer
---@param or_mask integer
function EnemyApi:yawn_set_status_range(first, last, and_mask, or_mask) end

---Recolour the head's and every segment's ground-shadow quad (the death
---dissolve).
---@param tint integer `0xRRGGBB`.
function EnemyApi:yawn_set_shadow_tint(tint) end

---Scale the rotation columns of the animator's joint worlds `first..=last`
---(`ScaleMatrixCols`), the swallow and death ramps.
---@param first integer
---@param last integer
---@param sx integer 4.12 fixed point.
---@param sy integer
---@param sz integer
function EnemyApi:yawn_scale_worlds(first, last, sx, sy, sz) end

---Scale one joint transform's rotation columns.
---@param joint integer
---@param sx integer 4.12 fixed point.
---@param sy integer
---@param sz integer
function EnemyApi:yawn_scale_transform(joint, sx, sy, sz) end

---Build the swallow's capture matrix from a joint's world matrix and the
---player's transform. Does nothing for a missing joint.
---@param joint integer
function EnemyApi:yawn_capture_setup(joint) end

---Right-multiply the capture rotation by a yaw rotation (`MulMatrix`), the
---swallow's shake.
---@param angle integer
function EnemyApi:yawn_capture_rotate_y(angle) end

---Left-multiply the capture rotation by a Z rotation, the setup's tilt.
---@param angle integer
function EnemyApi:yawn_capture_rotate_z(angle) end

---Add to the capture translation, the swallow's per-frame offsets.
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:yawn_capture_t_step(x, y, z) end

---Store the capture translation.
---@param x integer
---@param y integer
---@param z integer
function EnemyApi:set_yawn_capture_t(x, y, z) end

---Re-apply the capture through the head joint 0 to the player: the swallowed
---hold writes the player's position and raises the grabbed zone bit `0x80`.
function EnemyApi:yawn_hold_player() end

-- # Neptune

---Build Neptune's devour matrix from the mouth joint's world matrix and the
---player's transform (the shared capture set-up Yawn and Plant 42 also use).
---@param joint integer
function EnemyApi:neptune_capture_setup(joint) end

---The devour swallow's per-frame drag on the capture translation:
---`t[0] -= 0x41`, `t[1] += 0x0D`.
function EnemyApi:neptune_capture_drag() end

-- # Tyrant

---The Tyrant's root motion, the only pose that feeds back into movement:
---compose the yaw with the walk or run claw chain from the pose the clock just
---applied, subtract the chain end's previous-frame world translation, and
---store the planar remainder in `move_speed_current`.
---@param set integer `0` the walk clip set, `1` the run set.
---@param apply boolean Also slide the entity by the leftover XZ delta (the lunges).
---@return integer speed The stored speed; the untouched speed when the entity has no pose.
function EnemyApi:tyrant_root_motion(set, apply) end

---Clone the exposed heart into the companion arena (the init's one clone
---draw). Its handle reads back as `e.ty_heart`.
---@return integer handle `-1` when the arena is full.
function EnemyApi:tyrant_heart_spawn() end

---The heart's per-frame update: the gated beat/scale step and its composition
---under joint 1. Call it before the switch-zone refresh, like the original; it
---does nothing (and hides the heart) until the Tyrant has entered the
---camera's switch zone.
function EnemyApi:tyrant_heart_tick() end

---The rocket death's heart drop: integrate the free fall while the heart's
---stored world height is below -400 (Y grows downward), tumble it and grow
---the velocity. Runs even while the heart is hidden.
function EnemyApi:tyrant_heart_drop() end

---Re-arm the heart's drop velocity (the rocket setup stores `-0x12C`).
---@param velocity integer
function EnemyApi:tyrant_heart_set_speed(velocity) end

---The heart's stored world height (`+0x6E`).
---@return integer y `0` when there is no heart.
function EnemyApi:tyrant_heart_world_y() end

---The two claw ghosts' state: refresh the unscaled and scaled claw copies and
---step `ty_claw_scale_a`/`ty_claw_scale_b` by `ty_claw_scale_step`, which
---flips once the scale leaves `3000..=6000`. Does nothing while a message
---pauses the monsters.
function EnemyApi:tyrant_ghosts_tick() end

---Reserve the slash ribbon's nine-slot pool and store its `0x70` tint (the
---init call).
function EnemyApi:tyrant_trail_alloc() end

---The ribbon's `0x8000` arm sweep: seed the whole history from the current
---claw pose, sweeping the far blade in `+100` steps and ending on the
---degenerate slot-8 terminator.
function EnemyApi:tyrant_trail_arm() end

---One frame of the ribbon: push this frame's segment and, outside a
---monster-pausing message, retire the oldest and count the tail timer down.
function EnemyApi:tyrant_trail_update() end

---Launch one of the five severed limbs: copy its joint's world matrix into the
---free-body slot and arm its physics.
---@param index integer The limb, `0..5`; any other index is ignored.
---@param vx integer The launch velocity.
---@param vy integer
---@param vz integer
---@param tx integer The tumble rotation step.
---@param ty integer
---@param tz integer
function EnemyApi:tyrant_limb_launch(index, vx, vy, vz, tx, ty, tz) end

---One frame of a severed limb's free-body physics: X/Z velocity, gravity into
---Y, the bounce on the `-200` floor with half the velocity inverted, and the
---tumble.
---@param index integer `0..5`; any other index is ignored.
function EnemyApi:tyrant_limb_update(index) end

---A severed limb's live world translation, the rocket death's smoke anchor.
---@param index integer
---@return integer x `0, 0, 0` for an unknown limb.
---@return integer y
---@return integer z
function EnemyApi:tyrant_limb_pos(index) end

---The rocket death's camera-aim store: write the vector toward the live room
---camera (divided by `0x32`) into the entity's speed words. The smoke trail
---uses a fixed vector, so this is observable state only.
function EnemyApi:store_camera_speed() end

---The game's `GetAngleQuadrantValue`: a fixed-point slope to a 12-bit angle
---(the knock-back facing).
---@param slope integer
---@return integer angle
function EnemyApi:angle_quadrant(slope) end

-- # Math

---The shared 4.12 yaw rotation (`RotMatrixY` + `ApplyMatrixSV`) of an XZ
---vector; the zombie's wander waypoint rotates a 5000-unit forward vector by
---its yaw plus eight.
---@param angle integer
---@param x integer
---@param z integer
---@return integer x
---@return integer z
function EnemyApi:rotate_xz(angle, x, z) end

-- The property classes below mirror `add_fields` in `impl UserData for
-- EnemyApi` (src/enemy/lua.rs); `check` does not cover them.

---The entity's own record, as properties of `e`. Every numeric property reads
---at the field's width and an assignment truncates to it (see
---[Statelessness](#statelessness)). Properties marked read-only raise an error
---on assignment.
---@class EnemyEntity
---@field id integer Read-only. The entity type id, the `{id:02x}` of the script's folder.
---@field active boolean Bit 0 of `status_flags`, the active/visible bit. Clearing it stops the slot's updates.
---@field state integer `u8` at `+0x84`. The state byte the update dispatches on: by convention `0` init, `1` idle, `2` damaged, `3` dying; the weapon pipeline writes `2` or `3` on a hit.
---@field ignore integer `u8` at `+0x85`, the state word's second byte (the sub-state).
---@field action_behavior integer `u8` at `+0x86`, the animation behaviour byte.
---@field action_state integer `u8` at `+0x87`, the animation action-state byte.
---@field health integer `i16`. The weapon pipeline subtracts damage from it.
---@field hit_state integer `u8`. The damage hit latch the weapon pipeline sets.
---@field death_timer integer `u8` at `+0xBC`, the hit/death reaction countdown; [`e:ballistic`](#EnemyApi.ballistic) counts airborne ticks in it.
---@field status_flags integer `u8`. Bit 0 active, bit 2 hidden, bit 5 in visual range, bit 7 in alert range.
---@field behavior_flags integer `u8`. The spawn record's behaviour selector (the spawn kind); the adder rewrites it when it drops, emerges or respawns, and Yawn's segments carry `1`.
---@field scd_anim_param integer Read-only `u8`. The SCD completion flag id [`e:raise_scd_flag()`](#EnemyApi.raise_scd_flag) raises.
---@field angle integer `u16`, the 12-bit yaw.
---@field pitch integer `u16`, the rotation's X component.
---@field roll integer `u16`, the rotation's Z component.
---@field animation_id integer `u8`, the clip id the clock plays.
---@field animation_frame_id integer `u8`, the clip's pending frame.
---@field timing_control integer `u8`, the animation clock's hold counter.
---@field blend_counter integer `u8`, the animation blend state.
---@field unk_bf integer `u8`, an animation scratch byte the grab handlers clear.
---@field joint_scale integer `u16` at `+0xCA`, the joint world-matrix scale; `0` means identity (`0x1000`).
---@field sca_radius integer `i16`, the SCA collision radius (see [`e:set_sca`](#EnemyApi.set_sca)).
---@field sca_half_height integer `i16`, the SCA volume's centre height offset.
---@field shadow_half_x integer `i16`, the ground-shadow quad's half extent in X; `0` means no quad.
---@field shadow_half_z integer `i16`, the ground-shadow quad's half extent in Z.
---@field shadow_tint integer `u32`, the ground-shadow tint, `0xRRGGBB`.
---@field shadow_suppressed boolean When true the renderer does not queue the scripted ground quad (the ceiling spider until it drops).
---@field move_speed_current integer `u16`, this tick's walk speed.
---@field action_ticks_counter integer `u16` at `+0xC4`, the frame/phase counter of the hold and walk phases.
---@field collision_flags integer `u8`, the room-collision flags (bit 2 asks for floor zones, bit 3 is cleared by [`e:resolve_collision_radius`](#EnemyApi.resolve_collision_radius)).
---@field flags integer `u16`, `scd_entity_flags`, which the event scripts' `act_flag_op` edits.
---@field scd_timer integer `u16`, the SCD animation timer in frames.
---@field unk_c6 integer `u16`, the animation position offset X the grabs latch.
---@field unk_c8 integer `u16`, the animation position offset Z.
---@field pos_x integer `i32`, the room position X.
---@field pos_y integer `i32`, the room position Y (Y grows downward).
---@field pos_z integer `i32`, the room position Z.
---@field prev_pos_x integer Read-only. The stored collision-accepted position X (the original's `position` word); the current position until a driver stores one.
---@field prev_pos_y integer Read-only. See `prev_pos_x`.
---@field prev_pos_z integer Read-only. See `prev_pos_x`.
---@field saved_x integer `i16` view of the accepted `position` word X (`+0x6C`), which the hound's spawn nudge and probes save and restore.
---@field saved_y integer `i16` view of the accepted `position` word Y.
---@field saved_z integer `i16` view of the accepted `position` word Z.
---@field speed_x integer Read-only `i16`. The stored `Add_speedXZ` vector [`e:move`](#EnemyApi.move) wrote, X.
---@field speed_y integer Read-only `i16`. The stored vector's Y.
---@field speed_z integer Read-only `i16`. The stored vector's Z.
---@field target_x integer `i32`, the look-at target X (also the computer arms' home).
---@field target_y integer `i32`, the look-at target Y.
---@field target_z integer `i32`, the look-at target Z.
---@field floor_step integer `i16` at `+0x8E`, the floor step the last room resolve crossed.
---@field look_at_flags integer `u8`. `0x10` enables the look-at slew, bit 0 its yaw and bit 1 its pitch.
---@field look_at_joint integer `u8`, the joint the look-at aims; `0` means joint 1.
---@field look_at_yaw_step integer `u8`, the look-at's per-tick yaw step.
---@field look_at_pitch_step integer `u8`, the look-at's per-tick pitch step.
---@field pathfind_state integer `u8` at `+0x164`, the pathfinder's 5-bit counter plus the line-of-sight bit 5.
---@field has_enter_switch_zone integer `u8`. Bit 0 is the camera switch-zone bit; bit `0x80` the death-park bit.
---@field model_tint_r integer Read-only. The live model tint's red multiplier, `0..=255`.
---@field model_tint_g integer Read-only. The live green multiplier.
---@field model_tint_b integer Read-only. The live blue multiplier.
---@field tint_queue_r integer Read-only. The tint queue record's red accumulator, `-31..=31`.
---@field tint_queue_g integer Read-only. The green accumulator.
---@field tint_queue_b integer Read-only. The blue accumulator.
---@field tint_queue_word_a integer Read-only. The tint queue record's first parameter word.
---@field tint_queue_word_b integer Read-only. The second parameter word.
---@field tint_queue_armed boolean Read-only. Whether the tint queue record is armed.
local EnemyEntity = {}

---The behaviour scratch block every monster shares (entity `+0x16C` to
---`+0x18A` and a few words below it). The per-monster classes after this one
---re-read the same bytes under their own names and widths, so prefer those in
---a monster's own script. Each composed view owns the little-endian
---composition, so a script never shifts bytes together.
---@class EnemyScratch
---@field attacking_direction integer `u8` at `+0x16C`.
---@field dir_control_flags integer `u8` at `+0x16D`, the wander steering's direction-control flags.
---@field tex_bank integer `u8` at `+0x16E`.
---@field seq_counter integer `u8` at `+0x16F`, the look-at wander countdown.
---@field angle_turn_delta integer `u8` at `+0x170`.
---@field move_timer integer `u8` at `+0x171`.
---@field is_moving integer `u8` at `+0x172`.
---@field move_max_steps integer `u8` at `+0x173`.
---@field splatter_flag integer `u8` at `+0x174` ([`e:update_walk_zone`](#EnemyApi.update_walk_zone) stores the zone here).
---@field bob_speed integer `u8` at `+0x175`.
---@field reaction_timer integer `i16` at `+0x176`, the hit-reaction countdown; the walk layer reuses it as the heading.
---@field player_pos_x integer `i16`, the waypoint X the pathfinder latches.
---@field player_pos_z integer `i16`, the waypoint Z.
---@field action_speed integer `u8` at `+0x17C`.
---@field hit_threshold integer `u8` at `+0x17D`.
---@field behavior_step integer `u8` at `+0x17E`.
---@field action_counter integer `u8` at `+0x17F`.
---@field move_speed_byte integer `u8` at `+0x180`, the zombie's chase speed.
---@field turn_speed integer `u8` at `+0x181`.
---@field internal_timer integer `u8` at `+0x182`, the zombie's moan gate.
---@field stagger_timer integer `u8` at `+0x188`, the poise budget bullets spend.
---@field next_turn_timer integer `u16` at `+0xE2`, the weave timer.
---@field state_word integer `u32`, the four state bytes at `+0x84` (`state`, `ignore`, `action_behavior`, `action_state`) as one dword.
---@field state_mirror integer `u32` at `+0x184`, the state dword's mirror the damage reaction restores.
---@field writhe_velocity integer `i16` over `attacking_direction`/`dir_control_flags`.
---@field writhe_amplitude integer `i16` over `tex_bank`/`seq_counter`.
---@field tint_flashes integer `i16` over `angle_turn_delta`/`move_timer`.
---@field groan_timer integer `i16` over `action_speed`/`hit_threshold`.
---@field sink_wobble integer `i16` over `behavior_step`/`action_counter`.
---@field stored_pos_x integer `i32` over `splatter_flag`/`bob_speed`/`reaction_timer`.
---@field stored_pos_z integer `i32`, the `+0x178` dword.
local EnemyScratch = {}

---Room, session and engine-wide state, shared by every entity.
---@class EnemyWorld
---@field stage integer Read-only. The room's stage.
---@field room integer Read-only. The room number within the stage.
---@field mansion2 boolean Read-only. The room is on its second mansion visit.
---@field player_character integer Read-only. `0` Chris, `1` Jill.
---@field second_playthrough boolean Read-only. The scenario's second-playthrough flag (`0x7B`), which selects the harder damage rows.
---@field monster_paused boolean Read-only. The displayed message pauses the monsters; a monster's `update` should do nothing while it is true.
---@field attract_demo boolean Read-only. Attract playback is running, which makes several init-time picks reuse the frame seed instead of the platform stream.
---@field rand_seed integer `u16`, the per-frame random snapshot (`g_RandSeed`). A write reseeds it for the rest of the frame; the platform stream is [`e:srand`](#EnemyApi.srand).
---@field entity_bkp integer `u32`, the shared pathfinder-result scratch (`g_entity_bkp`) the zombie walks read back on frames that skip the call; the room resolves also write it.
---@field scratch_x integer `i32`, the shared probe scratch vector X (`g_playerPosScratch`) the hound's leap recovery steers at.
---@field scratch_z integer `i32`, the shared probe scratch vector Z.
---@field voice_playing boolean The `MSF_VOICE_PLAYING` handshake bit: the computer arms raise it after queueing their voice ([`e:play_voice`](#EnemyApi.play_voice)) and wait for the engine to clear it.
local EnemyWorld = {}

---The player as the attacking monsters see it. Writing the reaction fields
---(`player_state`, `player_anim_frame_id`, `player_attack_anim`) makes this
---entity the owner of the player's reaction clips, as do
---[`e:set_player_animation`](#EnemyApi.set_player_animation),
---[`e:set_player_action`](#EnemyApi.set_player_action),
---[`e:grab_player`](#EnemyApi.grab_player),
---[`e:hurt_player`](#EnemyApi.hurt_player) and the attacker latches. The
---reaction clips then play from this entity's model's damage bank (its second
---EDD chunk), so a room may mix enemy types; before any enemy has attacked,
---the last loaded model's bank is used. The damage itself is never a script's
---job for the monster's side: the weapon runtime writes the target's `health`,
---`hit_state` and `state` and the script reacts on its next update.
---
---The reaction windows run in Rust: writing `player_state` with
---`player_anim_frame_id` selects one, and the window hands control back when
---the attacker writes the release value into `player_action_state`:
---
---| `player_state` | `player_anim_frame_id` | Window |
---| --- | --- | --- |
---| 5 | 0 | The zombie's bite (`player_attack_anim` 0/3/6/9 by direction); released by `player_action_state = 3`. |
---| 5 | 7 | The wasp's grab, the crawl pin. |
---| 6 | 5 | The crow's peck-grab; released by `player_action_state = 4`. |
---| 6 | 8 | Plant 42's sweep, release and hold, dispatched on `player_action_behavior` (knockdown, grabbed, thrown). |
---| 6 | 9 | The chimera's grab-hold and its mash meter. |
---| 6 | 0x0B | Neptune's jaw. |
---| 6 | 0x0C | The Tyrant's staggers and knock-back, dispatched on `player_action_behavior`. |
---| 6 | 0x0F | The monster plant's three-state hold (entry, struggle, release); released by `player_action_state = 2`. |
---| 7 | 2 | The hound's maul. |
---| 7 | 6 | The hunter's pounce bite. |
---| 7 | 8 | Plant 42's eat. |
---| 7 | 0x0B | Neptune's devour. |
---| 7 | 0x0C | The Tyrant's impale. |
---| 7 | 0x0D | Yawn's swallow (the carried animation, then the capture hold). |
---
---`player_state = 2` with `player_action_behavior = 0x64` runs the generic hit
---reaction, which plays the body clip out and clears `player_attacked`.
---@class EnemyPlayer
---@field player_health integer `i16`, the player's health. Lowering it rumbles the pad.
---@field player_health_status integer `u8`, the health-status flags (bit `0x02` poison).
---@field player_poison_timer integer Read-only. The poison countdown.
---@field player_attacked integer `u8`, the player's being-attacked flag.
---@field player_attacker integer The entity slot the player's hit reaction latched, `-1` for none; an out-of-range write clears it.
---@field player_flags integer `u8`, the player's flags byte; the hound's kill and maul force the no-control bits `2`/`6`.
---@field player_zone_flags integer `u8`, the player's zone flags; the holds raise the grabbed bit `0x80` and the releases clear it.
---@field player_state integer `u8`, the player's `animationId` at `+0x84`, which selects the reaction window.
---@field player_anim_frame_id integer `u8`, the player's `animFrameId` at `+0x85`, the window's entry.
---@field player_animation_id integer Read-only. The same byte as `player_state`, under the original's name (the perched crow's scatter cue reads it).
---@field player_animation_frame_id integer `u8`, the frame of the player's current clip (the crow's peck cadence).
---@field player_attack_anim integer `u8`, the player's `attackAnim`; a write also sets the player's clip id, like the original's dword store.
---@field player_attack_direction integer `u16`, the player's `+0xC4` word.
---@field player_attack_timer integer `u16`, the player's `+0xE2` word the bite reads back.
---@field player_action_behavior integer `u8`, the player's action-behaviour byte.
---@field player_action_state integer `u8`, the player's action-state byte; the attackers write the release value here.
---@field player_angle integer `u16`, the player's yaw (the facing a grab latches).
---@field player_move_speed integer `u16`, the player's live speed (the knockdown crush's "moving" gate).
---@field player_pos_y integer Read-only. The player's height.
---@field player_weapon integer Read-only. The equipped item id, `0` for none.
---@field player_unk_c6 integer `u16`, the player's animation grab offset X.
---@field player_unk_c8 integer `u16`, the player's animation grab offset Z.
---@field player_unk_8c integer `u8`, a player animation scratch byte the grab handlers clear.
---@field player_unk_bf integer `u8`, another player animation scratch byte the grab handlers clear.
---@field player_displacement integer `i32`, a shared scratch distance the hunter's decisions run through.
---@field player_distance_z integer `i32`, a shared scratch distance; [`e:line_of_sight`](#EnemyApi.line_of_sight) overwrites it with its result, like the original.
---@field scaled_down_dist integer `i32`, a shared scratch distance.
local EnemyPlayer = {}

---The zombies' views (ids `0x00`, `0x01`, `0x11`, `lib/zombie.lua`), besides
---the `state_word` and byte fields in [`EnemyScratch`](#EnemyScratch).
---@class EnemyZombieViews
---@field attack_timer integer `u8`, the high byte of the `+0x176` word, the bite countdown.
---@field zombie_turn_delta integer `i16` at `+0x170`.
---@field zombie_move_word integer `u16` at `+0x172`.
---@field zombie_part_word integer `u16` at `+0x174`; while set, the head joint bleeds.
---@field zombie_wander_word integer `u16` at `+0x17A`, the movement distance [`e:wander_turn`](#EnemyApi.wander_turn) takes.
---@field zombie_unk_178 integer `u8` at `+0x178`.
---@field zombie_unk_179 integer `u8` at `+0x179`, the wander-turn counter.
local EnemyZombieViews = {}

---The adder's views over the scratch block.
---@class EnemyAdderViews
---@field sca_active integer `u16` over `splatter_flag`/`bob_speed`.
---@field sca_touch integer `i16` over `tex_bank`/`seq_counter`.
---@field room_collision integer `u16` over `is_moving`/`move_max_steps`.
---@field stuck_frames integer `u16`, the `reaction_timer` word.
---@field player_distance integer `u16` over `angle_turn_delta`/`move_timer`.
---@field path_bit integer Read-only. Bit 0 of the `+0x16C` byte, written by [`e:pathfind_keep`](#EnemyApi.pathfind_keep).
local EnemyAdderViews = {}

---The wasp's views over the scratch block.
---@class EnemyWaspViews
---@field wasp_drift integer `i16` over `attacking_direction`/`dir_control_flags`.
---@field wasp_touch integer `i16` over `angle_turn_delta`/`move_timer`.
---@field wasp_climb integer `i16` over `action_speed`/`hit_threshold`.
---@field wasp_timer integer `i16` over `behavior_step`/`action_counter`.
---@field wasp_bob integer `i16`, the `reaction_timer` word.
---@field wasp_distance integer `u16`, the low half of the `+0x178` dword; a store keeps the other half.
---@field wasp_collision integer `u16`, the high half of the `+0x178` dword.
---@field wasp_speed integer `i16` over `move_speed_current`.
---@field wasp_dwell integer `i16` over `action_ticks_counter`.
---@field wasp_anim_done integer `u16` at `+0x180`, the animation-completion flag the sting waits on.
---@field wasp_big integer `u8` at `+0x182`, the big-wasp flag (double shadow, health and sting; no grab).
---@field wasp_respawns integer `u8` at `+0x184`, the nest respawn counter.
---@field wasp_sound_latch integer `u8` at `+0x18A`, the one-shot crush-cue latch.
---@field wasp_path_bit integer Read-only. Bit 0 of the `+0x16E` path byte, written by [`e:wasp_path_keep`](#EnemyApi.wasp_path_keep).
local EnemyWaspViews = {}

---The crow's views over the scratch block, and its own fields.
---@class EnemyCrowViews
---@field crow_turn_rate integer `i16` over `attacking_direction`/`dir_control_flags`.
---@field crow_path_word integer `u16` at `+0x16E`, the path-result word.
---@field crow_path_bit integer Read-only. Bit 0 of `crow_path_word`.
---@field crow_touch integer `i16` at `+0x170`, the volume-touch word.
---@field crow_floor_limit integer `i16` at `+0x172`.
---@field crow_ceil_limit integer `i16` at `+0x174`.
---@field crow_vy integer `i16`, the `reaction_timer` word, the vertical velocity.
---@field crow_dist integer `u16`, the low half of the `+0x178` dword.
---@field crow_coll integer `u16`, the high half of the `+0x178` dword.
---@field crow_stuck integer `u16` at `+0x17C`, the stuck counter.
---@field crow_phase integer `i16` at `+0x17E`, the wobble phase.
---@field crow_swerve integer `i16` at `+0x182`, the angle [`e:swerve`](#EnemyApi.swerve) holds.
---@field crow_swerve_latch integer `u8` at `+0x184`: bit 7 means no swerve is pending, bits 0-6 the frames left.
---@field crow_struggle integer `i8` at `+0x186`, the grab struggle counter.
---@field crow_alt_bias integer `i16` at `+0x18A`, the altitude bias (`-400` for the scripted, unkillable crow).
local EnemyCrowViews = {}

---The two spiders' views over the scratch block: the WebSpinner's `ws_*`
---and the Black Tiger's `bt_*`.
---@class EnemySpiderViews
---@field ws_turn integer `i16` at `+0x16C`, the signed turn step.
---@field ws_path_word integer `u16` at `+0x16E`, written by [`e:spider_path_keep`](#EnemyApi.spider_path_keep).
---@field ws_touch integer `i16` at `+0x170`, the touch latch.
---@field ws_delay integer `i16` at `+0x172`, the post-attack delay.
---@field ws_splat integer `i16` at `+0x174`, the splat flag.
---@field ws_trail integer `i16` at `+0x176`, the room-collision trail.
---@field ws_count integer `i16` at `+0x178`, the chase count.
---@field ws_web_index integer `i16` at `+0x17A`, the web-joint registry index.
---@field bt_turn integer `i16` at `+0x16C`.
---@field bt_path_word integer `u16` at `+0x16E`.
---@field bt_touch integer `i16` at `+0x170`.
---@field bt_delay integer `i16` at `+0x172`.
---@field bt_splat integer `i16` at `+0x174`.
---@field bt_trail integer `i16` at `+0x176`.
---@field bt_count integer `i16` at `+0x178`.
---@field bt_cflag integer `i16` at `+0x17A`, the per-run counter.
---@field bt_web_index integer `i16` at `+0x17C`, the web-joint registry index.
---@field bt_cooldown integer `i16` at `+0x17E`, the post-attack cooldown.
local EnemySpiderViews = {}

---The hound's (Cerberus's) views over the scratch block.
---@class EnemyHoundViews
---@field cb_dist integer `i32` at `+0x16C`, the distance dword.
---@field cb_turn_step integer `i16` at `+0x172`.
---@field cb_launch_vy integer `i16` at `+0x174`, the leap's launch velocity.
---@field cb_probe integer `i16` at `+0x178`, the probe history.
---@field cb_path integer `u8` at `+0x17A`.
---@field cb_swerve integer `i16` at `+0x180`, the head-track yaw [`e:head_track`](#EnemyApi.head_track) integrates.
---@field cb_blood integer `i16` at `+0x182`, the owed blood billboards.
---@field cb_alert integer `u8` at `+0x184`, the alert latch.
---@field cb_behflags integer `i16` at `+0x186`, the behaviour flags.
---@field cb_behflags_byte integer `u8`, the low byte of `cb_behflags`.
---@field cb_aiflags integer `i16` at `+0x188`.
---@field cb_repause integer `i16` at `+0x18A`.
---@field cerberus_barked boolean The room-shared "a hound has already barked" latch.
local EnemyHoundViews = {}

---The chimera's views over the scratch block. Its turn step, path latch,
---touch latch, wall hit and grab meter use the generic fields.
---@class EnemyChimeraViews
---@field c_repause integer `i16` at `+0x172`, the re-target pause.
---@field c_fade_freeze integer `i16` at `+0x174`.
---@field c_wall_frames integer `i16` at `+0x178`, the wall-stuck count.
---@field c_far_latch integer `i16` at `+0x17A`, the far-target latch.
local EnemyChimeraViews = {}

---The hunter's views over its scratch block, and the room-shared state its
---decisions use.
---@class EnemyHunterViews
---@field hunter_speed integer `i16` at `+0xC2`.
---@field hunter_ticks integer `i16` at `+0xC4`.
---@field hunter_path_latch integer `i16` at `+0x170`; its low byte is the wander-turn counter.
---@field hunter_grab_word integer `i16` at `+0x172`.
---@field hunter_partner integer `u8` at `+0x174`, the partner slot [`e:pick_partner`](#EnemyApi.pick_partner) stores.
---@field hunter_target_x integer `i16`, the low half of the `+0x178` dword; a store keeps the other half.
---@field hunter_target_z integer `i16`, the high half of the `+0x178` dword.
---@field hunter_joint_sel integer `u16` at `+0x17C`, the mouth-joint selector.
---@field hunter_step_word integer `u16` at `+0x17E`.
---@field hunter_room_hit integer `u8` at `+0x180`, the wander-turn control byte.
---@field hunter_pounce_latch integer `u8` at `+0x182`.
---@field hunter_death_cnt_a integer `u8` at `+0x183`, the damage latch.
---@field hunter_death_cnt_b integer `u8` at `+0x184`.
---@field hunter_strafe_dir integer `u8` at `+0x185`.
---@field hunter_approach_cnt integer `u8` at `+0x187`.
---@field hunter_poise integer `u8` at `+0x188`.
---@field hunter_repause integer `u8` at `+0x189`.
---@field hunter_leap_flag integer `u8` at `+0x18A`.
---@field hunter_scream_latch integer `u8`, the room-shared one-per-room howl latch.
---@field hunter_intro_jump_kind integer `u8`, the room-shared scripted-intro row.
---@field hunter_grab_one_shot boolean The pounce grab's one-shot flag the death screen consumes.
---@field attract_room_camera_id integer Read-only `u8`. The outgoing room id the scripted intro's camera selector reads.
local EnemyHunterViews = {}

---The monster plant's views over the scratch block.
---@class EnemyMonsterPlantViews
---@field mp_dist integer `i32`, the Manhattan player distance.
---@field mp_holdoff integer `i16`.
---@field mp_anim_end integer `i16`.
---@field mp_seg integer `i16`.
---@field mp_timer_a integer `i16`.
---@field mp_timer_b integer `i16`.
---@field mp_fidget integer `i16`.
---@field mp_alerted integer `i16`.
---@field mp_angle_bk integer `i16`.
---@field mp_swerve integer `i16`.
---@field mp_shadow integer `i16`.
---@field mp_hits integer `i16`.
---@field mp_step_word integer `u16`, the behaviour/step word at `+0x86`.
---@field mp_state_bk integer `u32`, the state-dword snapshot the damaged state restores (the same field as `state_mirror`).
---@field monster_plant_sides integer `u32`, the room-shared side-picking register two poison plants alternate through.
local EnemyMonsterPlantViews = {}

---The computer arms' 16.16 fixed-point scratch. Each arm also records its home
---in `target_x`/`target_y`/`target_z` at init (the left arm's `target_y` is
---its command-7 floor).
---@class EnemyArmViews
---@field arm_vel_x integer `i32`.
---@field arm_vel_z integer `i32`.
---@field arm_pos_x integer `i32`.
---@field arm_pos_z integer `i32`.
---@field arm_y integer `i32`, the left arm's Y.
---@field arm_vel_y integer `i32`.
---@field arm_gate integer `i16`, the typing frame gate.
local EnemyArmViews = {}

---Plant 42's views over the scratch block, and the shared capture and body
---state.
---@class EnemyPlant42Views
---@field p42_yaw_jitter integer `i8`, the idle yaw sway.
---@field p42_roll_jitter integer `i8`, the idle roll sway.
---@field p42_osc_a integer `i16`, a suspend oscillator.
---@field p42_osc_b integer `i16`, the other suspend oscillator.
---@field p42_step integer `i16`, the sweep speed.
---@field p42_sweep_dir integer `u8`, the sweep direction.
---@field p42_grab_joint integer `i8`, the active grab joint.
---@field p42_fx_timer_a integer `u8`, an effect timer.
---@field p42_fx_timer_b integer `u8`, the other effect timer.
---@field p42_dist integer `u32`, the player distance.
---@field p42_life integer `i16`, the ambient timer.
---@field p42_death_counter integer `i8`.
---@field p42_pod_counter integer `i8`.
---@field p42_ticks integer `i16`, the frame timer.
---@field plant42_capture_t0 integer Read-only. The shared capture matrix's `t[0]` as `i16`, the sweep's knock-back facing; set it with [`e:set_plant42_capture_t0`](#EnemyApi.set_plant42_capture_t0).
---@field plant42_body_y integer? Read-only. The flower body's height, which gates the ambient spray; `nil` without a body.
local EnemyPlant42Views = {}

---Yawn's views over the scratch block.
---@class EnemyYawnViews
---@field yawn_turn_accel integer `i8`.
---@field yawn_form integer `u8`.
---@field yawn_bites integer `u8`.
---@field yawn_scale_ramp integer `i16`.
---@field yawn_speed integer `i16`, the XZ speed.
---@field yawn_ground_y integer `i16`.
---@field yawn_shrink integer `i16`, the death shrink.
---@field yawn_state_bk integer `u32`, the saved state dword a hit reaction restores.
---@field yawn_bite_dir integer `i8`.
---@field yawn_recovery integer `u8`.
---@field yawn_path_index integer `u8`.
---@field yawn_nearly_dead integer `u8`.
---@field yawn_sparkle integer `u16`.
---@field yawn_stuck integer `i8`.
---@field yawn_stuck_cool integer `i8`.
---@field yawn_hiss integer `i8`, the hiss cooldown.
---@field yawn_bite_cool integer `i8`, the bite cooldown.
local EnemyYawnViews = {}

---The Tyrant's views over the scratch block, and its render scratch.
---@class EnemyTyrantViews
---@field ty_pad_counter integer `u8` at `+0x16D`, the rumble phase counter.
---@field ty_hit_mask integer `u8` at `+0x16E`: which swing connected, plus the `0x80` would-have-killed latch.
---@field ty_look_mode integer `u8` at `+0x16F`.
---@field ty_state_bk integer `u32` at `+0x174`, the state dword the hit reaction restores.
---@field ty_flags integer `u8` at `+0x179`: the shadow, live look-at, connected-sound and hidden bits.
---@field ty_walk_dist integer `u16` at `+0x17C`, the travelled distance the walk steering reads.
---@field ty_repause integer `i8` at `+0x17F`, the attack repause.
---@field ty_rocket_flag integer `u8` at `+0x180`, the rocket-launcher latch.
---@field ty_cooldown integer `u8` at `+0x181`, the combo cooldown.
---@field ty_crowd integer `i8` at `+0x182`.
---@field ty_close integer `u8` at `+0x183`.
---@field ty_heart integer Read-only. The exposed heart's companion handle, `-1` for none.
---@field ty_claw_scale_a integer `i16`, the claw ghost's first scale.
---@field ty_claw_scale_b integer `i16`, the claw ghost's second scale.
---@field ty_claw_scale_step integer `i8`, the step both scales move by (`-56`).
---@field ty_trail_timer integer `u16`, the slash ribbon's tail timer.
---@field ty_trail_segments integer `i32`, the ribbon's live segment count.
local EnemyTyrantViews = {}
