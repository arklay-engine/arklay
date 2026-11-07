---@meta arklay.re2-enemy

-- RE 2 enemy scripts: the `enemy/{id}/enemy.lua` behaviour of an RE 2 pack.
--
-- This file is the source of the "RE 2 enemy scripts" page of the
-- documentation, and a LuaLS definition file for linting and completion in
-- an editor. `arklay mod lua-types --out DIR` writes every definition file
-- the engine ships; add DIR to your editor's `workspace.library`.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.

---An RE 2 pack keeps its enemies' behaviour in Lua. The engine keeps what the
---original's `Em_*` functions do around the behaviour (sight and routing,
---collision pushes, walls, thrown parts, the shadow, the draw) and hands the
---behaviour itself (each family's state machine, its tables and its choices)
---to a script, once per tick per body. The script works on the original's own
---data: the work's routines, its motion fields and its spare fields, through
---the low-level calls the original's code is built from.
---
---```lua
----- enemy/10/enemy.lua: zombie 0x10, a variant of the shared machine
---local zombie = require("/lib/zombie")
---
---local VARIANT <const> = { burns_unmarked = true }
---
---function update(e)
---    zombie.run(e, VARIANT)
---end
---```
---
---## Folders and ids
---
---Every scripted id has an `enemy/{id}/enemy.lua` pack entry, the id in two
---lowercase hex digits. Shared machines are ordinary pack modules under
---`lib/`, loaded with `require`:
---
---```
---enemy/
---    10/enemy.lua ... 1f/enemy.lua   zombies: zombie.run(e, VARIANT)
---    20/enemy.lua                    the dog (Em_20)
---    21/enemy.lua                    the crow (Em_21)
---    22/enemy.lua, 24/enemy.lua      the lickers: licker.run(e, VARIANT)
---    23/enemy.lua                    the alligator (Em_23)
---    25/enemy.lua                    the giant spider (Em_25)
---    26/enemy.lua                    the young spiders (Em_26)
---    2a/enemy.lua                    Mr. X (Em_2A)
---    2e/enemy.lua, 39/enemy.lua      the ivy and the poison ivy: ivy.update(e)
---    2f/enemy.lua                    the ivy's vines (Em_2F)
---    30/enemy.lua, 31/enemy.lua      G1 and G2 (Em_30, Em_31)
---lib/
---    zombie.lua    Em_Zombie's machine, shared by the zombie ids
---    licker.lua    Em_22's machine, shared by 0x22 and 0x24
---    ivy.lua       Em_2E's machine, shared by 0x2e and 0x39
---```
---
---The engine only hands a body to a script for these ids: `0x10` to `0x21`,
---`0x22` and `0x24` (lickers), `0x23`, `0x25`, `0x26`, `0x2A`, `0x2E`, `0x2F`,
---`0x39`, `0x30` and `0x31`. Any other enemy id a room places isn't modelled
---yet and takes no slot. An id the pack carries no script for leaves its
---bodies standing: the engine still runs its own part of the tick, but nothing
---moves them.
---
---The family code around the script is chosen by id, not by the script, so a
---mod that replaces `enemy/20/enemy.lua` still gets the dog's engine side
---(its ground and jaw fields, its stuck counter, its pushes). A variant id
---reuses a family by calling that family's `lib/` module with a table of what
---sets it apart, as the shipped lickers and zombies do.
---
---## Loading, `require` and the sandbox
---
---RE 2 scripts share the enemy-script host with RE 1's (see
---[RE 1 enemy scripts](enemy.md)): one sandboxed VM per session with the same libraries, memory
---ceiling and instruction budget as the Lua hooks. Each `enemy.lua` is
---compiled on its id's first tick and runs in its own environment, which falls
---back to the VM's globals. It must define a global `update` function; a
---script without one is logged and parked.
---
---`require(name)` loads a `.lua` entry of the pack once per VM and caches its
---return value (a module that returns nothing caches `true`). `/lib/zombie` is
---absolute from the pack root; `./x` and `../x` are relative to the requiring
---module; a bare `lib/zombie` is tried as an exact entry first and relative
---second. The `.lua` suffix is optional, paths are case-insensitive, `..` may
---not escape the pack root, and a cycle or a missing module is an error.
---
---## Lifecycle
---
---`update(e)` runs once per fixed 30 Hz tick for each body in play whose id
---names the script, in the original's `player_work` order. It also runs during
---a room load, as the original's `Em_init_move` does: once for each body the
---room placed, and a second time if the room gave it a start motion. A body
---another script hands out with [`e:place_spawn`](#Re2EnemyApi.place_spawn)
---runs at once, in the same tick, after the body that spawned it. A body is
---not run while the room has the enemies stopped, nor once its `Be_flg` bit 0
---is clear (it is out of play) or its `Type` holds it still.
---
---What the engine does around the call depends on the family:
---
---| Family | Before `update` | After `update` |
---| --- | --- | --- |
---| Zombies (`0x10`-`0x1F`) | the parts dropped by room scripts, `Root_ck` | the arm a shove pulls off, the neck, the frame's sound, pushes (`Oba_ck_em`), walls (`Sca_ck_em`), thrown parts |
---| Lickers (`0x22`, `0x24`) | `Root_ck`, the ground under it | the surface turn asked with `e:super_add`, the tongue, the frame's sound, pushes and walls from its three boxes, the joints |
---| Dog (`0x20`) | the ground and the jaw's place into its fields | pushes, walls, the stuck counter |
---| Crow (`0x21`) | the ground into its field | pushes, walls |
---| Mr. X (`0x2A`) | `Root_ck`, the ground, both hands' places | pushes, walls |
---| Alligator (`0x23`) | the parts as the last draw left them | its three boxes from its head |
---| Giant spider (`0x25`) | the ground, leg 1's foot, the posed parts | its floor from its height, pushes, walls (mask `0x600`) |
---| Young spiders (`0x26`) | the ground, the posed parts | its floor, then pushes and walls unless it killed itself |
---| G1, G2 (`0x30`, `0x31`) | `Root_ck` (G1's to its own point while it limps away) | pushes, walls, the posed parts |
---| Ivy, vines (`0x2E`, `0x2F`, `0x39`) | | pushes, walls, the parts its vines hang from |
---
---Every family except the plants also counts `Damage_cnt` down by one before
---the call while its low seven bits are non-zero.
---
---## Keep no state in Lua
---
---All mutable state lives on the engine's side of the body: **a script must
---keep no state of its own** between calls (no upvalues or globals that
---change, no tables it writes to). The engine may drop and recreate the VM,
---and a save or a room reload restores only what the body holds. What lasts
---past a tick goes in the work: its routines, its motion fields, its timers,
---and its spare fields, which [`e:var`](#Re2EnemyApi.var) and
---[`e:set_var`](#Re2EnemyApi.set_var) read and write. Read-only tables of
---constants (`local VARIANT <const> = {...}`, a module's tables) are fine.
---
---## How the work maps to the original
---
---The `e` methods are named after the original's fields and functions, so a
---script reads like the decompiled `Em_*` code it replaces:
---
---- **Routines.** `Routine_0` to `Routine_3` ([`e:routine`](#Re2EnemyApi.routine))
---  are the state machine: `Routine_0` is the mode (1 the normal move, then
---  damage, death and so on), `Routine_1` the state within it, `Routine_2` the
---  step within the state. [`e:go(state)`](#Re2EnemyApi.go) is the usual way to
---  change state; [`e:step(ended)`](#Re2EnemyApi.step) moves to the next step
---  when a clip ended.
---- **Spare fields.** `e:var(i)` is the part of the work past the engine's
---  fields that only the script reads: 34 integers on every family but the
---  lickers, 20 on a licker. A shipped script names each one with a
---  `local NAME <const> = i` and a comment giving the original's offset.
---  A few are written by the engine before the call, and a script reads them
---  there (see the table below).
---- **Motion.** `Move_no` is the clip, `Move_cnt` the frame in it, `Hokan` the
---  blend into it; [`e:clip`](#Re2EnemyApi.clip) sets all three, and
---  [`e:joint`](#Re2EnemyApi.joint) and [`e:spd`](#Re2EnemyApi.spd) play a frame
---  and read the root motion it gives, which `e:add_speed_xz` and friends move
---  the body by.
---- **Flags.** `Be_flg` ([`e:be_flg`](#Re2EnemyApi.be_flg)), `Type`
---  ([`e:kind`](#Re2EnemyApi.kind)) and the family word at `+0x21a` (or
---  `+0x22c` on a licker, [`e:flags`](#Re2EnemyApi.flags)) are raw bit fields;
---  the shipped scripts define the bits they use as constants.
---
---The spare fields the engine writes before `update`, by family:
---
---| Family | Field | What the engine writes |
---| --- | --- | --- |
---| Dog, crow, Mr. X | 20 | `Ground`, the floor height under the body ([`e:ground`](#Re2EnemyApi.ground); a licker's is there too, outside its own 20 fields, and `e:ground` is how it reads it) |
---| Dog | 21, 22 | the jaw's (part 4's) x and z as the last tick left it ([`e:jaw`](#Re2EnemyApi.jaw)) |
---| Dog | 18 | counted up after `update` on every tick the dog didn't move or touched a body, else cleared |
---| Mr. X | 14 to 19 | parts 12 and 15's x, y and z ([`e:hand`](#Re2EnemyApi.hand)) |
---| Giant spider | 30; 32, 33 | `Ground`; leg 1's foot (part 12) x and z |
---| Young spiders | 30; 5 | `Ground`; field 5 (`Water`) is copied from the spawner into a body `e:spawn` gives out |
---| G1 | 3, 7, 8 | read, not written: bit 8 of field 3 sends `Root_ck` to the point in fields 7 and 8 |
---| Ivy | 29, 30, 31 | 29 (which side of its clips it's on, 0 or 1) turns the aim the engine computes for it, 30 non-zero leaves the player out of its pushes, 31 is counted up after `update` on every tick it ended where it began |
---
---Some reads only have data in the families that need them: the flag banks
---and the room's objects ([`e:flag`](#Re2EnemyApi.flag),
---[`e:object_pos`](#Re2EnemyApi.object_pos)) exist only in the alligator's
---tick, and the water areas ([`e:water_ck`](#Re2EnemyApi.water_ck)) only in
---the spiders'. Elsewhere they read as empty.
---
---## Units
---
---Positions are the room's world units, `y` growing downward (a higher body
---has a smaller `y`). Angles are 12-bit: `0x1000` is a full turn, `0x400` a
---quarter, and a facing of 0 looks along `+x`. A *rate* is an angle step per
---tick unless a method says otherwise. Values are passed as Lua integers; each
---setter truncates to the field's own width exactly like the original's C
---store, so a script may compute in 64-bit integers and store without
---narrowing by hand. Collision records are numbered from 1 in the order the
---room's `.sca` holds them.
---
---## Errors
---
---A script that fails to load, lacks `update`, or raises an error during a
---call is logged once (`[enemy] 22 update disabled: ...`) and its id is parked
---for the session: every body of that id stops being run, and the other ids
---keep going. Methods raise an error for an index out of range (`no var 40`,
---`no routine 4`, `no timer 5`, `no record 0`, `no bank 3`), for a
---licker-only method called on another family (`this isn't a licker`), and
---for a `parent_*` read on a body without a parent (`no parent`).
---
---## What isn't here
---
---The RE 1 enemy surface (`e.state`, `e.health`, `e:advance_anim`,
---`e:play_enemy_sound` and the rest) doesn't exist for RE 2: `e` has no
---fields, only the methods below. RE 2 enemy sounds come from the room's enemy
---sound bank through [`e:sound`](#Re2EnemyApi.sound), not from
---`enemy/{id}/sound/` files, so a `sounds` declaration has no effect on them.

-- # Entry point

---Called once per tick for each body of the script's id (see Lifecycle
---above). The engine has already run its own part of the tick before the
---call and runs the rest after it; the script advances the body's state
---machine by one tick through `e`.
---
---`e` is valid only for the duration of the call: it is a scoped userdata
---over the live body, and keeping it (in an upvalue or a global) and using it
---later is an error.
---@param e Re2EnemyApi The body this call runs.
function update(e) end

-- source: src/zombie2/script.rs#Api
---One body's view of its work and the world around it, for one `update`
---call. Call the methods with Lua colon syntax (`e:life()`).
---
---Methods that change the body, its work, the player's work or the player's
---life change them at once, so a later read in the same call sees the change.
---The player's position and facing, flags, status bits, the camera cut, the
---room's objects, sounds, knocks, calls to the others and spawns are kept
---and applied by the engine once the body's tick is over; the matching reads
---(`e:player_pos`, `e:flag`, `e:status`) already count this call's changes.
---@class Re2EnemyApi
local Re2EnemyApi = {}

-- # State machine and spare fields

---One of the work's spare fields, the state the script keeps across ticks.
---@param index integer From 0: up to 33, or up to 19 on a licker.
---@return integer value The stored value (32-bit signed).
function Re2EnemyApi:var(index) end

---Store a spare field.
---@param index integer From 0: up to 33, or up to 19 on a licker.
---@param value integer Truncated to 32 bits.
function Re2EnemyApi:set_var(index, value) end

---One of the routine bytes `Routine_0` to `Routine_3`: the mode, the state,
---the step and the sub-step of the state machine.
---@param index integer 0 to 3.
---@return integer routine
function Re2EnemyApi:routine(index) end

---Set one routine byte.
---@param index integer 0 to 3.
---@param value integer Truncated to a byte.
function Re2EnemyApi:set_routine(index, value) end

---Set all four routine bytes at once.
---@param r0 integer `Routine_0`, the mode.
---@param r1 integer `Routine_1`, the state.
---@param r2 integer `Routine_2`, the step.
---@param r3 integer `Routine_3`.
function Re2EnemyApi:routines(r0, r1, r2, r3) end

---Put back the routines the body had at the end of its last tick (the
---original's `+0x228`), as a hit that is shrugged off does.
function Re2EnemyApi:restore_routine() end

---One of the routine bytes as the last tick ended, which `e:restore_routine`
---puts back.
---@param index integer 0 to 3.
---@return integer routine
function Re2EnemyApi:last_routine(index) end

---Enter a state of the normal move: the routines become `1, state, 0, 0`.
---@param state integer The new `Routine_1`.
function Re2EnemyApi:go(state) end

---Move to the state's next step when `ended` is true: `Routine_2` goes up by
---one. Typically called with what `e:joint` returned.
---@param ended boolean Whether to advance.
function Re2EnemyApi:step(ended) end

---The family's flag word: `+0x21a` on most families, `+0x22c` on a licker.
---The engine reads some of its bits (a zombie that is down, thrown, or has a
---still neck; a licker's quiet feet and its tick without walls).
---@return integer flags A 16-bit word.
function Re2EnemyApi:flags() end

---Set the family's flag word.
---@param flags integer Truncated to 16 bits.
function Re2EnemyApi:set_flags(flags) end

---One of `Timer0` to `Timer3`, which keep their own widths.
---@param index integer 0 to 3.
---@return integer timer `Timer0` and `Timer1` are unsigned 16-bit, `Timer2` an unsigned byte, `Timer3` a signed byte.
function Re2EnemyApi:timer(index) end

---Set one of `Timer0` to `Timer3`, truncated to its width.
---@param index integer 0 to 3.
---@param value integer
function Re2EnemyApi:set_timer(index, value) end

-- # Motion

---Start a clip: set `Move_no`, `Move_cnt` and `Hokan` together.
---@param clip integer The clip in the current bank.
---@param count integer The frame to start from, usually 0.
---@param hokan integer The blend into the clip's first frame.
function Re2EnemyApi:clip(clip, count, hokan) end

---`Move_no`, the clip that plays.
---@return integer clip
function Re2EnemyApi:move_no() end

---Set `Move_no` alone, leaving the frame and the blend.
---@param clip integer
function Re2EnemyApi:set_move_no(clip) end

---`Move_cnt`, the frame of the clip that plays next.
---@return integer frame
function Re2EnemyApi:move_cnt() end

---Set `Move_cnt`, the frame of the clip that plays next.
---@param count integer Truncated to a byte.
function Re2EnemyApi:set_move_cnt(count) end

---`Hokan`, the blend into the clip.
---@return integer hokan
function Re2EnemyApi:hokan() end

---Set `Hokan`.
---@param hokan integer Truncated to a byte.
function Re2EnemyApi:set_hokan(hokan) end

---`Joint_move` on the model's own bank: pose this tick's frame of the clip
---and advance `Move_cnt`.
---@param late integer How fast the pose blends toward the frame (`Joint_move2`'s blend).
---@param reverse? boolean Play the clip backwards. Defaults to `false`.
---@return boolean ended Whether the clip ended with this frame (also true for a clip that doesn't exist or has no frames).
function Re2EnemyApi:joint(late, reverse) end

---`Joint_move` on any bank. Without `reverse` it plays backwards exactly when
---the original does: in a scripted motion (`Routine_0` the scenario mode)
---with `Sce_flg` bit `0x80`.
---@param bank integer 0 the model's own clips, 1 the weapon bank, 2 the room's.
---@param late integer The blend toward the frame.
---@param reverse? boolean Force the direction instead.
---@return boolean ended Whether the clip ended with this frame.
function Re2EnemyApi:joint_bank(bank, late, reverse) end

---Choose the bank `e:spd`, `e:skip_frame` and the other motion reads use,
---without playing a frame.
---@param bank integer 0 the model's own clips, 1 the weapon bank, 2 the room's.
function Re2EnemyApi:set_bank(bank) end

---`Spd_get` on the model's own bank: set the work's speed from the root
---motion between this frame and the last, reversed when a scripted motion
---plays backwards.
function Re2EnemyApi:spd() end

---`Spd_get` on any bank.
---@param bank integer 0 the model's own clips, 1 the weapon bank, 2 the room's.
function Re2EnemyApi:spd_bank(bank) end

---`Spd_get` on the model's own bank, read as if the clip played backwards.
function Re2EnemyApi:spd_reversed() end

---Step to the clip's next frame without posing it, keeping its frame flags
---(the original's `FUN_004c2bc0`); wraps to frame 0 at the end.
function Re2EnemyApi:skip_frame() end

---`Now_seq`, the flags of the frame the clip last played. Bit `0x8000000`
---asks for a sound, whose slot is the top four bits.
---@return integer seq A 32-bit word.
function Re2EnemyApi:now_seq() end

---`Em_seq_sound`: play the sound the last frame asks for in `Now_seq`, if any.
function Re2EnemyApi:seq_sound() end

---`Spl_Scenario` on the model's own bank: play the motion a room script gave
---the body, every other tick when `Sce_flg` has `0x10`, keeping a foot
---planted when its bits 1 and 2 ask.
function Re2EnemyApi:scenario() end

---How far the current pose has the root off the ground
---(`pSin_parts_ptr[0].Pos_y`).
---@return integer y
function Re2EnemyApi:root_y() end

---Where a part hangs from its parent in the skeleton.
---@param part integer The part index.
---@return integer x 0 for a part the skeleton doesn't have.
---@return integer y
---@return integer z
function Re2EnemyApi:part_offset(part) end

---A part's own rotation on this tick's pose (`+0x68`).
---@param part integer The part index.
---@return integer x Each 0 without a pose of the part.
---@return integer y
---@return integer z
function Re2EnemyApi:part_rot(part) end

---`Sa_parts_allmv`: place a part's children by the weights of its morph, as
---the model's morph data has them. Does nothing for a part with no morph.
---@param part integer The morphing part.
---@param ... integer The weight of each shape.
function Re2EnemyApi:morph(part, ...) end

---The alligator's jaws: the roll of parts 6 and 7 on top of their pose.
---@param six integer Part 6's roll.
---@param seven integer Part 7's roll.
function Re2EnemyApi:set_jaw(six, seven) end

---The foot of the current pose in the world (`Foot_set_pl`'s chain: the
---root, the hips and that leg's three parts).
---@return integer? x All three `nil` without a pose.
---@return integer? y
---@return integer? z
function Re2EnemyApi:foot_position() end

---`Foot_set_pl`: move the body along the floor so the foot stays where it was
---planted. Without all three coordinates it does nothing.
---@param x? integer The planted foot, as `e:foot_position` gave it.
---@param y? integer
---@param z? integer
function Re2EnemyApi:foot_set(x, y, z) end

---`Null_pos_set`: stand the body where the clip's frame puts it from its
---base (`e:pl_set_base`, `e:grab_body`), turned `turn` more than it faces.
---@param turn integer An angle.
function Re2EnemyApi:null_pos_set(turn) end

-- # Speed and moving

---The work's speed `Spd`, as `Spd_get` or `e:set_speed` left it.
---@return integer x Along the facing.
---@return integer y Up and down.
---@return integer z Across the facing.
function Re2EnemyApi:speed() end

---Set the work's speed.
---@param x integer Truncated to 16 bits, as are `y` and `z`.
---@param y integer
---@param z integer
function Re2EnemyApi:set_speed(x, y, z) end

---Set `Spd_base`, which `Spd_get` takes the next frame's speed from.
---@param x integer
---@param y integer
---@param z integer
function Re2EnemyApi:set_speed_base(x, y, z) end

---The speed's `x`.
---@return integer x
function Re2EnemyApi:speed_x() end

---Set the speed's `x` alone.
---@param x integer
function Re2EnemyApi:set_speed_x(x) end

---Set the speed's `z` alone.
---@param z integer
function Re2EnemyApi:set_speed_z(z) end

---`Add_speedXZ`: move the body by its speed turned by its facing, on the
---floor.
function Re2EnemyApi:add_speed_xz() end

---`Add_speedXYZ` for a body that doesn't lean: `e:add_speed_xz`, and the
---speed's `y` added to the height.
function Re2EnemyApi:add_speed_xyz() end

---`Add_speedXZ` with an `Add_dir`: the speed turned by the facing plus `dir`.
---@param dir integer An angle added to the facing.
function Re2EnemyApi:add_speed_xz_at(dir) end

---`e:add_speed_xz_at`, and the speed's `y` added to the height.
---@param dir integer An angle added to the facing.
function Re2EnemyApi:add_speed_xyz_at(dir) end

---`Approach`: move the body `rate` toward a point on the floor, landing on it
---when the step would pass it.
---@param x integer
---@param z integer
---@param rate integer The step, in world units.
---@return boolean arrived Whether the body is now on the point.
function Re2EnemyApi:approach(x, z, rate) end

---`Approach_one` on the height: move `y` by `rate` toward a height, landing
---on it when the step would pass it.
---@param y integer
---@param rate integer The step, in world units.
---@return boolean arrived Whether the body is now at that height.
function Re2EnemyApi:approach_y(y, rate) end

-- # Position and facing

---Where the body stands.
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:pos() end

---Move the body to a point.
---@param x integer
---@param y integer
---@param z integer
function Re2EnemyApi:set_pos(x, y, z) end

---Move the body by an offset.
---@param x integer
---@param y integer
---@param z integer
function Re2EnemyApi:add_pos(x, y, z) end

---Set the body's height alone.
---@param y integer
function Re2EnemyApi:set_pos_y(y) end

---`Old_x` to `Old_z`: where the body was when the tick began.
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:old_pos() end

---The body's facing.
---@return integer angle `0..0xFFF`.
function Re2EnemyApi:angle() end

---Set the body's facing.
---@param angle integer Truncated to 16 bits.
function Re2EnemyApi:set_angle(angle) end

---`Goto01`: turn the body `rate` toward a facing, landing on it when close. A
---negative rate turns it toward the opposite facing.
---@param to integer The facing to turn to.
---@param rate integer
function Re2EnemyApi:goto_angle(to, rate) end

---`Goto01_ck`: which way the body would turn toward a facing.
---@param angle integer The facing.
---@param rate integer
---@return integer turn `0` once the facing is within `rate`, else `rate` or `-rate`, the way to turn.
function Re2EnemyApi:turn_to(angle, rate) end

---`Goto00`: turn the body `rate` toward a point on the floor (away from it
---with a negative rate).
---@param x integer
---@param z integer
---@param rate integer
function Re2EnemyApi:goto_point(x, z, rate) end

---`Goto00` toward the player.
---@param rate integer Negative to turn away.
function Re2EnemyApi:goto_player(rate) end

---`Goto00` toward the work's destination `Dest_x`, `Dest_z`, which `Root_ck`
---and `e:root_ck` set.
---@param rate integer Negative to turn away.
function Re2EnemyApi:goto_dest(rate) end

---The work's destination `Dest_x`, `Dest_z`: where `Root_ck` last routed the
---body (or a room script gave it).
---@return integer x
---@return integer z
function Re2EnemyApi:dest() end

---`Cdir_x` and `Cdir_z`: the body's pitch and roll, which turn it as it is
---posed as well as drawn.
---@return integer x
---@return integer z
function Re2EnemyApi:cdir() end

---Set `Cdir_x` and `Cdir_z`; the drawn tilt (`e:tilt`) follows.
---@param x integer
---@param z integer
function Re2EnemyApi:set_cdir(x, z) end

---The crow's drawn tilt, its pitch and roll for the draw alone.
---@return integer pitch
---@return integer roll
function Re2EnemyApi:tilt() end

---Set the crow's drawn tilt.
---@param pitch integer
---@param roll integer
function Re2EnemyApi:set_tilt(pitch, roll) end

---How far a hit has the body leaning: the extra roll `Em10Dm00` gives the
---first part and takes from the second. The engine clears it before a
---zombie's `update`.
---@param lean integer Truncated to 16 bits.
function Re2EnemyApi:set_lean(lean) end

-- # Geometry

---Turn an offset on the floor by an angle, with the original's rounding.
---@param angle integer
---@param x integer
---@param z integer
---@return integer x The turned offset, each 16-bit.
---@return integer z
function Re2EnemyApi:rotate(angle, x, z) end

---`RotMatrix` of three angles, then `ApplyMatrixSV` on a point.
---@param ax integer The angle about `x`.
---@param ay integer The angle about `y`.
---@param az integer The angle about `z`.
---@param x integer
---@param y integer
---@param z integer
---@return integer x The turned point, each 16-bit.
---@return integer y
---@return integer z
function Re2EnemyApi:rot_apply(ax, ay, az, x, y, z) end

---`Direction_ck`: the facing from one point on the floor to another.
---@param x0 integer
---@param z0 integer
---@param x1 integer
---@param z1 integer
---@return integer angle `0..0xFFF`.
function Re2EnemyApi:direction(x0, z0, x1, z1) end

---The original's integer square root of `dx² + dz²` (`SquareRoot0`).
---@param dx integer
---@param dz integer
---@return integer length
function Re2EnemyApi:hypot(dx, dz) end

---Whether two points on the floor are closer than `within`.
---@param x0 integer
---@param z0 integer
---@param x1 integer
---@param z1 integer
---@param within integer
---@return boolean near
function Re2EnemyApi:point_within(x0, z0, x1, z1, within) end

---`Goto00_ck`: which way the body would turn toward a point on the floor.
---@param x integer
---@param z integer
---@param rate integer
---@return integer turn `0` once it faces the point within `rate`, else `rate` or `-rate`.
function Re2EnemyApi:goto_ck(x, z, rate) end

---`Goto00_ck` from any point and facing.
---@param x0 integer Where the turn is from.
---@param z0 integer
---@param angle integer The facing there.
---@param x integer The point to face.
---@param z integer
---@param rate integer
---@return integer turn `0` within `rate`, else `rate` or `-rate`.
function Re2EnemyApi:goto_ck_at(x0, z0, angle, x, z, rate) end

-- # Parts and the neck

---A point given on a part, in the world, as this tick's frame poses the part.
---@param part integer The part index.
---@param x integer The point in the part's own space.
---@param y integer
---@param z integer
---@return integer? x All three `nil` without a pose of the part.
---@return integer? y
---@return integer? z
function Re2EnemyApi:part_point(part, x, y, z) end

---A point given on a part, in the world, as the last draw left the part
---(`Workm`); `e:part_point` poses it on this tick's frame instead.
---@param part integer
---@param x integer
---@param y integer
---@param z integer
---@return integer? x All three `nil` before the part's first draw.
---@return integer? y
---@return integer? z
function Re2EnemyApi:drawn_part_point(part, x, y, z) end

---The reverse of `e:drawn_part_point`: a world point in a part's own space,
---as the last draw left the part.
---@param part integer
---@param x integer
---@param y integer
---@param z integer
---@return integer x The point unchanged before the part's first draw.
---@return integer y
---@return integer z
function Re2EnemyApi:part_local(part, x, y, z) end

---A part's place (`Workm.t`) as the last composition of the body's parts left
---it.
---@param part integer
---@return integer x Each 0 before the part has one.
---@return integer y
---@return integer z
function Re2EnemyApi:own_part(part) end

---Stop drawing a part: clear its `Be_flg` as `Parts_set` does. The part stays
---in the skeleton.
---@param part integer 0 to 31; others do nothing.
function Re2EnemyApi:hide_part(part) end

---The parts that aren't drawn, one bit per part.
---@return integer parts A 32-bit mask.
function Re2EnemyApi:hidden_parts() end

---Set which parts aren't drawn.
---@param parts integer A 32-bit mask, bit `n` for part `n`.
function Re2EnemyApi:set_hidden_parts(parts) end

---Scale the pose's root height: what a zombie that crawls or kneels sets.
---@param scale integer 4.12 fixed point: `0x1000` keeps the clip's height.
function Re2EnemyApi:set_parts0_y(scale) end

---`Neck_flg`: how the neck turns (bit 1 holds it still, bit 4 aims it at a
---point).
---@return integer flags
function Re2EnemyApi:neck_flags() end

---Set `Neck_flg`.
---@param flags integer
function Re2EnemyApi:set_neck_flags(flags) end

---Clear `Neck_flg`.
function Re2EnemyApi:clear_neck() end

---The neck's yaw on top of its pose (`+0x98`).
---@return integer turn
function Re2EnemyApi:neck_turn() end

---Set the neck's yaw.
---@param turn integer Truncated to 16 bits.
function Re2EnemyApi:set_neck_turn(turn) end

---`FUN_0045f5b0`: `Rot_neck_em` on both of the dog's neck joints, toward the
---player's head.
function Re2EnemyApi:rot_neck() end

---`Rot_neck_em` on a joint, from where the last draw left it toward the
---player's head.
---@param joint integer The joint that turns (`Neck_no`).
---@param rate integer How far it turns per tick.
---@param limit integer How far it may turn from its pose.
function Re2EnemyApi:rot_neck_part(joint, rate, limit) end

---`Rot_neck` with `Neck_flg`'s 4: a joint turns toward a point rather than the
---player's head. Only the yaw turns.
---@param joint integer
---@param rate integer
---@param limit integer
---@param x integer The point to look at.
---@param y integer
---@param z integer
function Re2EnemyApi:rot_neck_point(joint, rate, limit, x, y, z) end

---`Rot_neck_em` on a joint as though the body faced `angle`, from where the
---joint was last tick toward the player's head (or the player's position
---before their neck has one).
---@param joint integer
---@param angle integer The facing to turn from.
---@param rate integer
---@param limit integer
function Re2EnemyApi:rot_neck_joint(joint, angle, rate, limit) end

---Where Mr. X's hand was as the last tick ended (parts 12 and 15), from the
---spare fields the engine writes for him.
---@param hand integer 0 the first hand, 1 (or more) the second.
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:hand(hand) end

---The dog's jaw (part 4) as the tick began, from the spare fields the engine
---writes for it.
---@return integer x
---@return integer z
function Re2EnemyApi:jaw() end

-- # The work

---`Type`, the work's kind bits, from the room's placement.
---@return integer kind A 16-bit word.
function Re2EnemyApi:kind() end

---Set `Type`.
---@param kind integer Truncated to 16 bits.
function Re2EnemyApi:set_kind(kind) end

---The body's slot among the characters.
---@return integer slot
function Re2EnemyApi:slot() end

---The work's entity id (`Id`).
---@return integer? id `nil` if the slot has none.
function Re2EnemyApi:id() end

---`Be_flg`, the body's state bits (bit 0 in play, 2 no pushes, 8 no walls,
---`0x4000000` hit low, `0x8000000` hit high, and so on).
---@return integer flags A 32-bit word.
function Re2EnemyApi:be_flg() end

---Set `Be_flg`.
---@param value integer Truncated to 32 bits.
function Re2EnemyApi:set_be_flg(value) end

---The body's life.
---@return integer life Signed 16-bit; below 0 is dead.
function Re2EnemyApi:life() end

---Set the body's life.
---@param life integer Signed 16-bit.
function Re2EnemyApi:set_life(life) end

---`D_life_u`, `D_life_c` and `D_life_d`: how much more each region takes
---before it gives.
---@param index integer 0 to 2.
---@return integer life
function Re2EnemyApi:d_life(index) end

---Set all three regions' `D_life` to one value.
---@param value integer Truncated to a byte.
function Re2EnemyApi:set_d_life(value) end

---`Damage_cnt`: the shot that hit this tick (its count in the low seven bits),
---which the engine counts down.
---@return integer count
function Re2EnemyApi:damage_cnt() end

---Set `Damage_cnt`.
---@param value integer Truncated to a byte.
function Re2EnemyApi:set_damage_cnt(value) end

---`Damage_no`: the last shot's band times three plus its region.
---@return integer no
function Re2EnemyApi:damage_no() end

---Set `Damage_no`.
---@param value integer Truncated to a byte.
function Re2EnemyApi:set_damage_no(value) end

---`Damage_flg`: which collision record and which side the last shot came
---from.
---@return integer flags
function Re2EnemyApi:damage_flg() end

---Set `Damage_flg`.
---@param value integer
function Re2EnemyApi:set_damage_flg(value) end

---Whether the last shot was aimed up.
---@return boolean high
function Re2EnemyApi:shot_high() end

---Whether `Root_ck`'s line from the body's neck to the player's is clear
---(`Status_flg`'s `0x800`). It is checked one tick in eight.
---@return boolean seen
function Re2EnemyApi:in_sight() end

---`At_em_no`: the body the last pushes found touching this one.
---@return integer slot `0xFF` none, `0xFE` the player, else a slot.
function Re2EnemyApi:at_em_no() end

---Whether the body touching this one is the player or their partner.
---@return boolean touching
function Re2EnemyApi:touching() end

---`Sca_info`, what the last wall check found.
---@return integer info A 32-bit word.
function Re2EnemyApi:sca_info() end

---`Em_dead_flg_on`: raise the body's dead flag, so the room doesn't place it
---again.
function Re2EnemyApi:set_died() end

---`Kill_enemy_work`: take the body out of play. `Be_flg` and the routines go
---to 0, and the room counts one enemy fewer.
function Re2EnemyApi:kill() end

---`Model_type`, from the room's placement.
---@return integer type
function Re2EnemyApi:model_type() end

---Choose a zombie's clothes: the texture page `Em10_Init` picks.
---@param skin integer From 0.
function Re2EnemyApi:set_skin(skin) end

---Set `Sce_flg`, a scripted motion's options.
---@param flags integer Truncated to 16 bits.
function Re2EnemyApi:set_sce_flg(flags) end

---`Sce_free0` or `Sce_free1`: a turn rate, or a turn at a scripted motion's
---end, as a room script gave them.
---@param index integer 0 or 1.
---@return integer value Signed 16-bit.
function Re2EnemyApi:sce_free(index) end

---Set `Sce_free0` or `Sce_free1`.
---@param index integer 0 or 1.
---@param value integer Truncated to 16 bits.
function Re2EnemyApi:set_sce_free(index, value) end

---`Floor`, the floor level the body is on, which the wall checks use.
---@return integer floor
function Re2EnemyApi:floor() end

---Put the body on the player's floor level.
function Re2EnemyApi:take_player_floor() end

---`Ground`, the floor height the engine found under the body before the call
---(spare field 20).
---@return integer y
function Re2EnemyApi:ground() end

---The distance to the player on the floor, as the tick began.
---@return integer distance
function Re2EnemyApi:distance() end

---The distance to the player on the floor, from where the body stands now.
---@return integer distance
function Re2EnemyApi:distance_now() end

-- # Collision boxes, shadow and fire

---Set one of the body's collision boxes (`AT_DATA`), which the pushes use.
---@param which integer 0 the body (`Atd[0]`); on a licker 1 the head and 2 (or more) the back.
---@param ofs_x integer The box's offset ahead of the body.
---@param ofs_y integer Its offset in height.
---@param at_w integer Its half-width for the pushes.
---@param at_h integer Its height.
---@param w integer Its half-width for the walls.
function Re2EnemyApi:set_atd(which, ofs_x, ofs_y, at_w, at_h, w) end

---Drop a licker's back box, which only counts on a wall or the ceiling.
function Re2EnemyApi:clear_back_atd() end

---The body box `Atd[0]`'s size.
---@return integer ofs_y
---@return integer at_w
---@return integer at_h
---@return integer w
function Re2EnemyApi:body_box() end

---Set the body box's size, leaving where it stands ahead of the body.
---@param ofs_y integer
---@param at_w integer
---@param at_h integer
---@param w integer
function Re2EnemyApi:set_body_box(ofs_y, at_w, at_h, w) end

---The body box's offset ahead of the body.
---@return integer ofs_x
function Re2EnemyApi:body_ofs_x() end

---Set the body box's offset ahead of the body.
---@param ofs integer Signed 16-bit.
function Re2EnemyApi:set_body_ofs_x(ofs) end

---Size the body's shadow (`pKage_work`'s `Half_x` and `Half_z`) for a body
---that sizes its own.
---@param half integer The half-size; 0 hides it.
function Re2EnemyApi:set_shadow(half) end

---How far the blood pool under a dead body has grown.
---@return integer pool
function Re2EnemyApi:pool() end

---Set the blood pool's size.
---@param pool integer Truncated to 16 bits.
function Re2EnemyApi:set_pool(pool) end

---How far a burnt body's colours have gone down.
---@return integer charred
function Re2EnemyApi:charred() end

---Set how far a burnt body's colours have gone down.
---@param charred integer Truncated to a byte.
function Re2EnemyApi:set_charred(charred) end

---Mark the body burnt, for its draw.
---@param burnt boolean
function Re2EnemyApi:set_burnt(burnt) end

---`FUN_00456780`: light a flame on a part, which the engine then carries with
---the part.
---@param size integer The effect's size.
---@param part integer The part it burns on.
---@param x integer Where on the part.
---@param y integer
---@param z integer
function Re2EnemyApi:light_flame(size, part, x, y, z) end

-- # Room collision

---`Sca_ck_info` with no push: the first collision record of the zombie mask
---(`0x400`) on the body's floor whose outline, grown by `radius`, holds the
---point.
---@param x integer
---@param y integer
---@param z integer
---@param radius integer
---@return integer? x The record's corner, size, or all four `nil` with none.
---@return integer? z
---@return integer? width
---@return integer? depth
function Re2EnemyApi:first_wall(x, y, z, radius) end

---`Sca_ck_info`'s list: the records of `mask` on a floor whose outline, grown
---by `radius`, holds the point.
---@param x integer
---@param z integer
---@param floor integer The floor level (its low five bits are used).
---@param mask integer The record bits to look for.
---@param radius? integer Defaults to 10.
---@return integer[] records Record numbers, from 1; empty in a room without collision.
function Re2EnemyApi:sca_list(x, z, floor, mask, radius) end

---`FUN_004e3960`: the room's first records for some areas and a mask,
---wherever they are, at most 32.
---@param areas integer The area bits.
---@param mask integer The record bits.
---@return integer[] records Record numbers, from 1.
function Re2EnemyApi:sca_all(areas, mask) end

---A collision record's outline and bits.
---@param record integer From 1.
---@return integer x
---@return integer z
---@return integer width
---@return integer depth
---@return integer id Its id word: the shape in bits 0-3, the rest its blocking bits.
---@return integer floor Its floor word.
function Re2EnemyApi:sca_rect(record) end

---`Sca_ck_info` with no push: its answer and its list.
---@param x integer
---@param z integer
---@param floor integer The floor level (its low five bits are used).
---@param mask integer
---@param radius integer
---@return integer info Or-ed flags: 1 a box or a shape that holds the point, 3 a climb, 4 a drop, 8 and `0x10` shapes 11 and 12.
---@return integer[] records As `e:sca_list` gives them.
function Re2EnemyApi:sca_ck_info(x, z, floor, mask, radius) end

---`FUN_004e2ec0`: which side of a record, seen from above, a point faces.
---@param record integer From 1.
---@param x integer
---@param z integer
---@return integer side 0 its right (`+x`) edge, 1 its near (`z`) edge, 2 its left, 3 its far.
function Re2EnemyApi:sca_side(record, x, z) end

---`FUN_004e3250`: where one side of a record lies, on that side's own axis.
---@param record integer From 1.
---@param side integer 0 to 3, as `e:sca_side` numbers them.
---@return integer at The `x` or `z` of that edge.
function Re2EnemyApi:sca_edge(record, side) end

---`FUN_004e2f30`: which side of a record a point faces, seen from the plane
---of the surface the body is on (`e:super_plane`).
---@param record integer From 1.
---@param x integer
---@param y integer
---@param z integer
---@return integer side 0 to 3.
function Re2EnemyApi:sca_face(record, x, y, z) end

---`FUN_004e32b0`: a point moved onto the side of a record it faces, in the
---plane of the surface the body is on.
---@param record integer From 1.
---@param x integer
---@param y integer
---@param z integer
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:sca_onto_edge(record, x, y, z) end

---`Sca_ck_hit`'s two answers at a point, without the room's objects: the
---surface over it and the one under it.
---@param x integer
---@param y integer
---@param z integer
---@param mask integer
---@return integer over The ceiling over a blocked point, else the floor it stands on.
---@return integer under The higher of the floors a record starts from below and above it.
function Re2EnemyApi:sca_hit(x, y, z, mask) end

---`Sca_ck_hit` of a point grown by `radius`, without the room's objects:
---whether a record holds it or starts above it.
---@param x integer
---@param y integer
---@param z integer
---@param radius integer
---@param mask integer
---@return boolean struck
function Re2EnemyApi:sca_struck(x, y, z, radius, mask) end

---`Sca_ck_info` at a point on the body's floor: whether a record of `mask`,
---grown by `radius`, holds it.
---@param x integer
---@param y integer
---@param z integer
---@param radius integer
---@param mask integer
---@return boolean touching
function Re2EnemyApi:sca_touch(x, y, z, radius, mask) end

---`FUN_004e2ec0` on the record the last wall check hit (`pSca_hit_data`):
---which side of it the body is on.
---@return integer? side 0 to 3, `nil` if the check hit none.
function Re2EnemyApi:hit_side() end

---The middle of the record the last wall check hit.
---@return integer? x Both `nil` if the check hit none.
---@return integer? z
function Re2EnemyApi:hit_middle() end

---`FUN_004e2ec0` as the crow's code calls it, with its stray argument: which
---side of the hit record the body is on.
---@param return_to integer What the original's stack holds where it reads `z`.
---@return integer side 0 to 3; 0 if the check hit none.
function Re2EnemyApi:wall_side(return_to) end

---Whether the line from the body to the player crosses no record of `mask`.
---@param mask integer
---@return boolean clear
function Re2EnemyApi:line_to_player(mask) end

---`Water_ck`: the level of the first water area over a point. Only the
---spiders' ticks have the room's water areas.
---@param x integer
---@param z integer
---@return integer level 0 outside water.
function Re2EnemyApi:water_ck(x, z) end

-- # Climbing surfaces

---Ask the surface the body walks on to turn by this much at the end of the
---tick (a licker going onto a wall or the ceiling).
---@param x integer
---@param y integer
---@param z integer
function Re2EnemyApi:super_add(x, y, z) end

---Put the body back on a plain floor (`Super_matrix` cleared).
function Re2EnemyApi:init_super() end

---`Rot_vector_super`: a vector turned by an angle on the surface the body is
---on.
---@param angle integer
---@param x integer
---@param y integer
---@param z integer
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:super_rotate(angle, x, y, z) end

---`Get_axis2`: which way the surface's up points.
---@return integer axis 0 up, 1 down, 2 and 3 along `x` each way, 4 and 5 along `z`.
function Re2EnemyApi:super_axis() end

---`FUN_004e3150`: the plane the surface lies in.
---@return integer plane 0 a floor or a ceiling, 1 a wall across `x`, 2 a wall across `z`.
function Re2EnemyApi:super_plane() end

---Move the body by its speed on the surface it walks on, turned by its facing
---plus `turn_y` and rolled by `turn_z`. On a plain floor with no turns it is
---`e:add_speed_xyz`.
---@param turn_y integer
---@param turn_z integer
function Re2EnemyApi:add_speed_super(turn_y, turn_z) end

---`Goto00_super`: turn `rate` toward a point, on the surface.
---@param x integer
---@param y integer
---@param z integer
---@param rate integer
function Re2EnemyApi:goto_super(x, y, z, rate) end

---`Goto01_super`: turn `rate` toward a facing, on the surface.
---@param to integer
---@param rate integer
function Re2EnemyApi:goto_angle_super(to, rate) end

-- # Lickers

---How far ahead of its chest a licker's wall check looks (`+0x23a`).
---Licker only.
---@return integer reach
function Re2EnemyApi:reach() end

---Set how far ahead of its chest the wall check looks. Licker only.
---@param reach integer Signed 16-bit.
function Re2EnemyApi:set_reach(reach) end

---A licker's `Timer1`. Licker only.
---@return integer timer
function Re2EnemyApi:timer1() end

---Set a licker's `Timer1`. Licker only.
---@param timer integer Signed 16-bit.
function Re2EnemyApi:set_timer1(timer) end

---Start or stop a dead licker's colour fading (`+0x21b`). Licker only.
---@param fading integer
function Re2EnemyApi:set_fading(fading) end

---How far out the tongue is (`+0x21a`). Licker only.
---@return integer tongue 0 in.
function Re2EnemyApi:tongue() end

---Send the tongue out; the engine runs it after `update` and gives the room's
---tongue back when it's in. Licker only.
function Re2EnemyApi:tongue_out() end

---The licker whose tongue is out: one per room (`DWORD_006894f8`).
---@return integer owner A `Work_no`, 0 none.
function Re2EnemyApi:tongue_owner() end

---Claim or give back the room's tongue.
---@param owner integer
function Re2EnemyApi:set_tongue_owner(owner) end

---Place the parts the licker's boxes follow (parts 0 and 1) at the body, as
---its init does. Licker only.
function Re2EnemyApi:init_parts() end

-- # Routing

---`Root_ck` with a target: route the body toward a point through the room's
---guide areas, setting its destination.
---@param x integer
---@param z integer
---@return integer result 0 no way, 1 the destination is set, 2 a tick without a look.
function Re2EnemyApi:root_ck(x, z) end

---`Root_ck` toward the middle of a guide area.
---@param area integer From 0.
---@return integer result 0 no way, 1 the destination is set, 2 a tick without a look.
function Re2EnemyApi:root_ck_area(area) end

---`Rnd_area`: a random guide area. Draws from the random stream.
---@return integer area From 0, or -1 in a room without guide areas.
function Re2EnemyApi:rnd_area() end

---How many guide areas the room has.
---@return integer count
function Re2EnemyApi:area_count() end

---Whether the room has guide areas.
---@return boolean has
function Re2EnemyApi:has_guides() end

---`Hit_ck_point4` of the body's place on the current cut's view: whether the
---camera sees where it stands.
---@return boolean seen True when the cut has no view area.
function Re2EnemyApi:in_cut_view() end

-- # Reading the player

---Where the player stands.
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:player_pos() end

---`Old_x` and `Old_z`: where the player ended the last tick.
---@return integer x
---@return integer z
function Re2EnemyApi:player_old() end

---The player's facing.
---@return integer angle
function Re2EnemyApi:player_angle() end

---The player's floor level.
---@return integer floor
function Re2EnemyApi:player_floor() end

---Whether the body is on the player's floor level.
---@return boolean same
function Re2EnemyApi:same_floor() end

---The player's life.
---@return integer life Signed 16-bit.
function Re2EnemyApi:player_life() end

---The player character's `Id`, which picks how a crawler's end looks.
---@return integer id
function Re2EnemyApi:player_id() end

---The player's body width (`Atd[0]`'s `At_w`).
---@return integer at_w 0 if the player has no body box.
function Re2EnemyApi:player_at_w() end

---The floor height under the player.
---@return integer y
function Re2EnemyApi:player_ground() end

---The player's `At_em_flg`: whether a body touched them this tick.
---@return boolean touched
function Re2EnemyApi:player_touched() end

---A point given on the player's root matrix, in the world.
---@param x integer
---@param y integer
---@param z integer
---@return integer? x All three `nil` before the player has one.
---@return integer? y
---@return integer? z
function Re2EnemyApi:player_root_point(x, y, z) end

---The turn from the body's facing to the player.
---@return integer angle `0..0xFFF`.
function Re2EnemyApi:angle_to_player() end

---`Direction_ck` from the body to the player.
---@return integer angle
function Re2EnemyApi:direction_to_player() end

---`Direction_ck` from the player to the body.
---@return integer angle
function Re2EnemyApi:direction_from_player() end

---`Goto00_ck` toward the player: which way the body would turn.
---@param rate integer
---@return integer turn 0 once it faces them within `rate`, else `rate` or `-rate`.
function Re2EnemyApi:turn_to_player(rate) end

---`Dir_pos_ck`: whether the player is within `within` of a facing, seen from
---the body.
---@param angle integer
---@param within integer Half the arc.
---@return boolean inside
function Re2EnemyApi:player_within(angle, within) end

---`Cdir_ck` on the player: whether they face within a quarter turn of the
---way the body faces (the body is behind them).
---@return boolean same
function Re2EnemyApi:cdir_player() end

---`Goto01_ck` of the player toward the body: which way they would turn to
---face it.
---@param rate integer
---@return integer turn 0 within `rate`, else `rate` or `-rate`.
function Re2EnemyApi:player_turn_here(rate) end

---One of the player's routine bytes.
---@param index integer 0 to 3.
---@return integer routine
function Re2EnemyApi:pl_routine(index) end

---The player's `Damage_cnt`.
---@return integer count
function Re2EnemyApi:pl_damage_cnt() end

---`Gacha_ch`: whether a button was mashed this tick, to break a hold.
---@return boolean mashed
function Re2EnemyApi:gacha() end

-- # Acting on the player

---Set the player's life.
---@param life integer Signed 16-bit.
function Re2EnemyApi:set_player_life(life) end

---`Pl_life_down`: take life off the player. Above 120 life a blow under 41 is
---made half as big again; one that would kill is turned to 0 life once, or
---always with `low`.
---@param down integer The life to take.
---@param low? boolean Never kill. Defaults to `false`.
---@return integer result 0 the player lives, 1 they were left at 0, 2 they died (their `Damage_cnt` gets `0x80`).
function Re2EnemyApi:pl_life_down(down, low) end

---Face the player this way at the end of the tick. `e:player_angle` still
---reads the facing the tick began with; `e:player_goto_angle` and
---`e:player_goto_point` turn on from this one.
---@param angle integer Its low 12 bits.
function Re2EnemyApi:set_player_angle(angle) end

---`Goto01` on the player: turn them `rate` toward a facing, from where this
---tick has already turned them.
---@param to integer
---@param rate integer
function Re2EnemyApi:player_goto_angle(to, rate) end

---`Goto00` on the player: turn them `rate` toward a point, from where this
---tick has already turned them.
---@param x integer
---@param z integer
---@param rate integer
function Re2EnemyApi:player_goto_point(x, z, rate) end

---Move the player on the floor.
---@param x integer
---@param z integer
function Re2EnemyApi:set_player_xz(x, z) end

---`Approach` on the player: move them `rate` toward a point, landing on it
---when the step would pass it.
---@param x integer
---@param z integer
---@param rate integer
---@return boolean arrived
function Re2EnemyApi:player_approach(x, z, rate) end

---Set all four of the player's routine bytes.
---@param r0 integer
---@param r1 integer
---@param r2 integer
---@param r3 integer
function Re2EnemyApi:pl_set_routine(r0, r1, r2, r3) end

---Set one of the player's routine bytes.
---@param index integer 0 to 3.
---@param value integer
function Re2EnemyApi:pl_set_routine_at(index, value) end

---Set the player's `Damage_cnt`.
---@param value integer Truncated to a byte.
function Re2EnemyApi:pl_set_damage_cnt(value) end

---Set bits in the player's `Be_flg`.
---@param bits integer
function Re2EnemyApi:pl_or_be_flg(bits) end

---Clear bits in the player's `Be_flg`.
---@param bits integer
function Re2EnemyApi:pl_clear_be_flg(bits) end

---Set one of the player's `Timer0` to `Timer2` of the held states.
---@param index integer 0 to 2.
---@param value integer Truncated to 16 bits.
function Re2EnemyApi:pl_set_timer(index, value) end

---Give the player a motion on this body's model's second bank (the player's
---`pSub1` clips), as a grab or a fall does.
---@param clip integer The player's `Move_no`.
---@param count integer Their `Move_cnt`.
---@param hokan integer Their `Hokan`.
---@param x integer Their speed.
---@param y integer
---@param z integer
---@param neck? integer Bits or-ed into their `Neck_flg`. Defaults to 0.
function Re2EnemyApi:pl_set_motion(clip, count, hokan, x, y, z, neck) end

---Point the player's `pSub1` clips at this body's model's second bank, for
---`Pl_damage`'s falls.
function Re2EnemyApi:pl_set_sub1() end

---Take hold of the player: they are held by this body's slot.
function Re2EnemyApi:pl_hold() end

---Mark the hold's bite as one that kills outright.
function Re2EnemyApi:pl_held_kill() end

---A bite took the player's last life, but their death waits until the body
---has them down.
function Re2EnemyApi:pl_hold_back_die() end

---`Set_base_pos` for the player's hold: where the clip must start for the
---body to be where it is.
function Re2EnemyApi:pl_set_base() end

---`Set_base_pos` for a body this one takes hold of (`FUN_00459ef0`).
function Re2EnemyApi:grab_body() end

---Let go of the player if this body holds them, as a shot does: they go back
---to their normal move.
function Re2EnemyApi:let_go() end

---Knock another body back, as a zombie's stumble does into the ones it runs
---into; the engine plays enemy sound slot 4 when it falls.
---@param slot integer The other body's slot.
function Re2EnemyApi:knock(slot) end

-- # Parts that come off

---`Be_flg |= 0x1062`: throw a part (and the parts below it) off the body,
---`along` away from its facing and `up`. The engine lets it fall.
---@param part integer
---@param along integer
---@param up integer
---@param gravity integer
function Re2EnemyApi:throw_part(part, along, up, gravity) end

---Blow a part off in a direction.
---@param part integer
---@param dir integer The direction it flies.
---@param speed? integer Defaults to 800.
---@param up? integer Defaults to 0.
---@param gravity? integer Defaults to 10.
function Re2EnemyApi:blow_part(part, dir, speed, up, gravity) end

---`FUN_00461f90`: fling a part off the other way from `dir`, coming to rest
---turned `rest`.
---@param part integer
---@param dir integer
---@param along integer
---@param gravity integer
---@param rest integer Its yaw once down.
function Re2EnemyApi:fling_part(part, dir, along, gravity, rest) end

---Keep both arms (parts 9 and 12) as they stand, before a clip that throws
---one moves them.
function Re2EnemyApi:keep_arms() end

---`e:throw_part` from where `e:keep_arms` kept the arm.
---@param part integer 9 the first arm; any other the second.
---@param along integer
---@param up integer
---@param gravity integer
function Re2EnemyApi:throw_kept_arm(part, along, up, gravity) end

---`FUN_00453510`: start pulling an arm off in a shove; the engine runs it
---over the next ticks.
---@param part integer The arm's first part.
---@param behind boolean The player is behind the body.
function Re2EnemyApi:pull_part(part, behind) end

-- # Other bodies

---`Ck_em_id`: the slot of another body in play with an id.
---@param id integer
---@return integer? slot `nil` if none.
function Re2EnemyApi:other_of(id) end

---`Ck_em_id(work, 0, id, 0)`: how many other bodies in play have an id.
---@param id integer
---@return integer count
function Re2EnemyApi:count_of(id) end

---`Ck_em_id` of this body's own id: how many others of it the room has.
---@return integer count
function Re2EnemyApi:same_id() end

---`Set_enemy_work(id)`: take a work for a new body: a dead one of the id, or
---a new one copied from the last of the id. The id must already be in the
---room.
---@param id integer
---@param kind integer Its `Type`.
---@param vars table<integer, integer> Spare fields to set before its first run, `{ [index] = value }`, each index below 34.
---@return integer? slot The slot it gave, `nil` if none.
function Re2EnemyApi:spawn(id, kind, vars) end

---Place the last `e:spawn`'s body and set more of its fields: it then runs at
---once, after this one, as `FUN_004732a0` runs `Em_26`. An error without a
---spawn this tick.
---@param x integer
---@param y integer
---@param z integer
---@param angle integer Its facing.
---@param vars table<integer, integer> More spare fields, `{ [index] = value }`.
function Re2EnemyApi:place_spawn(x, y, z, angle, vars) end

---Whether this body has a parent (a vine's ivy, or the body `e:set_parent`
---named), as the parent stood after its own run this tick.
---@return boolean has
function Re2EnemyApi:has_parent() end

---Keep another body as this one's parent from the next tick: the giant
---spider's partner (`+0x240`).
---@param slot? integer `nil` clears it.
function Re2EnemyApi:set_parent(slot) end

---One of the parent's spare fields.
---@param index integer
---@return integer value 0 out of range.
function Re2EnemyApi:parent_var(index) end

---Where the parent stands.
---@return integer x
---@return integer y
---@return integer z
function Re2EnemyApi:parent_pos() end

---The parent's facing.
---@return integer angle
function Re2EnemyApi:parent_angle() end

---The floor height under the parent.
---@return integer y
function Re2EnemyApi:parent_ground() end

---The parent's life.
---@return integer life
function Re2EnemyApi:parent_life() end

---The parent's `Type`.
---@return integer kind
function Re2EnemyApi:parent_kind() end

---One of the parent's routine bytes.
---@param index integer 0 to 3.
---@return integer routine
function Re2EnemyApi:parent_routine(index) end

---A part of the parent's place (`Workm.t`), as the 16-bit values a vine reads.
---@param part integer
---@return integer x Each 0 if the parent has no such part.
---@return integer y
---@return integer z
function Re2EnemyApi:parent_part(part) end

---A part of the parent's own rotation this tick (`+0x68`).
---@param part integer
---@return integer x Each 0 if it has none.
---@return integer y
---@return integer z
function Re2EnemyApi:parent_part_rot(part) end

---`FUN_00463070`: call to every body of this id. Each one (this body too, at
---once) whose spare field `flags` has none of `skip` gets `bits` there and
---its fields `one` and `two` cleared.
---@param flags integer The spare field to test and add to.
---@param skip integer The bits that keep a body out.
---@param bits integer The bits to set.
---@param one integer A spare field to clear.
---@param two integer Another.
function Re2EnemyApi:call_kin(flags, skip, bits, one, two) end

---Whether the player's partner (`splayer_work`) is in play.
---@return boolean partner
function Re2EnemyApi:partner() end

---How many enemies the room has.
---@return integer count
function Re2EnemyApi:enemies() end

---`fg_room_enemy`'s low byte: what the room's dogs told each other.
---@return integer value
function Re2EnemyApi:room_enemy() end

---Set `fg_room_enemy`'s low byte.
---@param value integer Truncated to a byte.
function Re2EnemyApi:set_room_enemy(value) end

---Where the body a room script named for this one (`Sce_free3`) stands.
---@return integer x This body's own place if it named none.
---@return integer y
---@return integer z
function Re2EnemyApi:scene_target() end

-- # The game and the room

---The player's weapon (`DAT_00989ffe`).
---@return integer weapon
function Re2EnemyApi:weapon() end

---The noises the player made (`WORD_00989eee`).
---@return integer noise
function Re2EnemyApi:noise() end

---Whether the game is on easy (`fg_system`'s easy bit).
---@return boolean easy
function Re2EnemyApi:easy() end

---`fg_common`'s `0xA4`, which takes some life off a licker.
---@return boolean set
function Re2EnemyApi:common_a4() end

---`IsScenarioClaire`: whether this is Claire's scenario.
---@return boolean claire
function Re2EnemyApi:scenario_claire() end

---Which draw buffer this frame fills (`DWORD_009888d8`'s low byte), which
---some of the original's choices alternate on.
---@return integer buffer
function Re2EnemyApi:draw_buffer() end

---The room, as `(stage + 1) * 0x100 + room`.
---@return integer code
function Re2EnemyApi:room_code() end

---Whether any of some bits is set in `fg_status`'s first word, counting what
---scripts set and cleared this tick.
---@param bits integer
---@return boolean set
function Re2EnemyApi:status(bits) end

---Set bits in `fg_status`'s first word.
---@param bits integer
function Re2EnemyApi:set_status(bits) end

---Clear bits in `fg_status`'s first word.
---@param bits integer
function Re2EnemyApi:clear_status(bits) end

---`Flg_ck` of a flag bank, counting what scripts changed this tick. Only the
---alligator's tick has the banks; elsewhere a flag reads unset unless a
---script set it this tick.
---@param bank integer
---@param index integer The flag, 0 to 255.
---@return boolean set
function Re2EnemyApi:flag(bank, index) end

---Set or clear a flag.
---@param bank integer
---@param index integer
---@param on boolean
function Re2EnemyApi:set_flag(bank, index, on) end

---`scd_var_cut`, the camera's cut as the tick began.
---@return integer cut
function Re2EnemyApi:cut() end

---`Cut_change`: switch the camera to a cut.
---@param cut integer
function Re2EnemyApi:cut_change(cut) end

---Where one of the room's objects (`obj_model_set`) stands. Only the
---alligator's tick has them.
---@param index integer
---@return integer x Each 0 for an object it doesn't have.
---@return integer y
---@return integer z
function Re2EnemyApi:object_pos(index) end

---Move one of the room's objects. Does nothing for one it doesn't have.
---@param index integer
---@param x integer
---@param y integer
---@param z integer
function Re2EnemyApi:set_object_pos(index, x, y, z) end

---Take one of the room's objects away (its `Be_flg` cleared, as
---`Om_explosion_set` ends).
---@param index integer
function Re2EnemyApi:hide_object(index) end

-- # Sound, effects and randomness

---Play a sound of the room's enemy bank, from where the body stands now (the
---second enemy type's half when `Be_flg` has `0x2000`).
---@param slot integer The bank slot.
function Re2EnemyApi:sound(slot) end

---`Rnd`: the next draw of the stream everything random in the game shares.
---@return integer value 0 to 255.
function Re2EnemyApi:rnd() end

---`Esp_call` at a point on a part: start an effect (blood, sparks, acid).
---@param kind integer The effect.
---@param row integer Its row of the effect table.
---@param size integer
---@param dir integer Its direction.
---@param part integer The part it starts on; the body's place without a pose of it.
---@param x integer Where on the part.
---@param y integer
---@param z integer
function Re2EnemyApi:effect(kind, row, size, dir, part, x, y, z) end

---`Esp_call` at a world point.
---@param kind integer
---@param row integer
---@param size integer
---@param dir integer
---@param x integer Each truncated to 16 bits.
---@param y integer
---@param z integer
function Re2EnemyApi:call_effect(kind, row, size, dir, x, y, z) end
