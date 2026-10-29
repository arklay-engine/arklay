//! The entity record, in a file of its own (#266).
//!
//! `Entity` is a large struct whose derived `Clone`, `PartialEq`, `Default`
//! and `Debug` are inlined into most of the crate's codegen units. In
//! `src/game.rs` every line inserted above it moved the derives' spans, which
//! recompiled all of those units: about 48s instead of 15s for a one-line
//! edit near the top of `game.rs`. Here an edit to `game.rs` leaves it alone.

use super::*;

/// One scripted entity.
///
/// Slot 0 is the player; slots 1.. are enemies and objects that later
/// milestones spawn. The actor and tween sub-ISAs operate on the entity the
/// current event selected with `evt_work_set`.
///
/// `state_field` packs the two state-block bytes at entity offset 0x84: the
/// low byte is the entity state (1 idle, 8 scripted animation) and the high
/// byte is the player-ignore flag. The tween `tw_set_rot` instruction stores
/// a sign-extended byte into that same word, so it is kept as one u16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Entity {
    /// Entity type id; `0x20..=0x2E` are scripted characters.
    pub id: u8,
    /// Room position.
    pub pos: [i32; 3],
    /// The original's `position` word (+0x6C): the last position a room
    /// collision resolve accepted. The state-9 tail's tunnelling test and
    /// rollback read this, not the live `pos`, so an object-pass push after
    /// the tail does not become the rollback point. `None` until a driver
    /// binds it, which keeps fixtures and non-collision paths on `pos`.
    pub saved_pos: Option<[i32; 3]>,
    /// 12-bit yaw.
    pub angle: u16,
    /// Rotation X component of the entity's rotation vector.
    pub pitch: u16,
    /// Rotation Z, the companion of `pitch` in the rotation SVECTOR.
    pub roll: u16,
    /// Entity status flags; bit 0 is active/visible.
    pub status_flags: u8,
    /// Port-only: hostile enemies cannot perceive this entity (the GHOST
    /// cheat sets it on the player). Per entity, so another player or an NPC
    /// can be hidden independently.
    pub hidden_from_enemies: bool,
    /// The spawn record's behaviour/weapon selector.
    pub behavior_flags: u8,
    /// Animation behavior byte (`action_behavior`).
    pub action_behavior: u8,
    /// Animation action state byte.
    pub action_state: u8,
    /// EDD clip id published by the driver.
    pub animation_id: u8,
    /// Scripted animation frame id published by the driver.
    pub animation_frame_id: u8,
    /// Hold counter of the animation clock (`timing_control`).
    pub timing_control: u8,
    /// Animation blend step (`blend_counter`).
    pub blend_counter: u8,
    /// This tick's walk speed.
    pub move_speed_current: u16,
    /// SCD animation timer (frames).
    pub scd_timer: u16,
    /// SCD animation parameter.
    pub scd_anim_param: u8,
    /// `scd_entity_flags`, OR/SET/XORed by `act_flag_op`.
    pub flags: u16,
    /// Per-tick translation from the tween `tw_set_pos` instruction.
    pub move_speed: [i16; 3],
    /// Per-tick rotation steps from the tween `tw_set_rot` instruction:
    /// `[0]` drives `pitch`, `[2]` drives `angle`.
    pub move_step: [i16; 3],
    /// The entity state word (`state | ignore << 8`).
    pub state_field: u16,
    /// Damage hit state (`tw_set_8a`).
    pub hit_state: u8,
    /// The hit/death reaction countdown at entity +0xBC (`death_timer`). The
    /// adder zeroes it when a reaction setup starts; the projectile arc
    /// helpers count airborne ticks in it.
    pub death_timer: u8,
    /// Last `tw_set_sel` selector.
    pub selector: u16,
    /// Health word written by `tw_set_sel` selector 3 and `tw_set_field`.
    pub health: i16,
    /// Scripted attack animation (`attackAnim`, 0x2B).
    pub attack_anim: u8,
    /// Animation scratch byte `unk_bf` (0x2B/0x33 clear it).
    pub unk_bf: u8,
    /// Animation scratch byte `unk_8c` (0x33 sub-commands 1/6/7).
    pub unk_8c: u8,
    /// `isBeingAttackedFlag` (0x33 sub-command 1). Combat never raises it.
    pub is_being_attacked: u8,
    /// `unk_e0` scratch word (0x33 sub-command 10 toggles bit 0x40).
    pub unk_e0: u16,
    /// Whether the player's ground-shadow quad (`pushVelocity`, entity
    /// +0xE4) has been built. Only the player's state-0 init builds it (500 x
    /// 700) and only the new-game memset clears it, so it survives doors; a
    /// session that opens straight into a room whose init script holds the
    /// player in state 8 (`plw_anim`) keeps the zero-sized quad, which
    /// `DrawFadeSpr` skips.
    pub shadow_quad: bool,
    /// Animation position offsets (`unk_c6`/`unk_c8`); the state-8 walk
    /// handlers steer at this pair.
    pub unk_c6: u16,
    /// Animation position offset Z.
    pub unk_c8: u16,
    /// Collision callback flags (`collisionFlags`). `act_anim_seq` clears or
    /// sets bit 7; the state-8 walk handlers keep their behaviour running on
    /// arrival while it is set.
    pub collision_flags: u8,
    /// 16-bit frame/phase counter at entity +0xC4 (`action_ticks_counter`):
    /// the state-8 hold counter and the walk deceleration phase count.
    pub action_ticks_counter: u16,
    /// Look-at control byte set by `act_motion`.
    pub look_at_flags: u8,
    /// The joint the look-at slew aims (`lookAtJointIdx`). `0` means the
    /// shared default (joint 1, the character models' head pivot); the Tyrant
    /// sets 2.
    pub look_at_joint: u8,
    /// Look-at target position; for an entity target this is refreshed every
    /// tick from the target's current position.
    pub target: [i32; 3],
    /// Entity slot this entity is moving toward, when the target is an entity.
    pub target_entity: Option<u8>,
    /// Per-tick yaw step of the active `act_motion`.
    pub look_at_yaw_step: u8,
    /// Per-tick pitch step of the active `act_motion`.
    pub look_at_pitch_step: u8,
    /// Interaction/zone state bits (`zone_flags` in the original entity):
    /// `0x01` inside the current camera zone, `0x10` door swings the other way
    /// or ladder variant, `0x20` inside a stairs/ladder/door zone, `0x40` door
    /// direction modifier, `0x80` grabbed. `set_stairs_zone` raises `0x20`
    /// (and `0x10` for the ladder variant).
    pub zone_flags: u8,
    /// Scenario variant packed by the `enemy` spawn: the slot nibble, the
    /// record's selector high nibble and the force-init bit.
    pub variant: u8,
    /// Room event index to raise when the entity dies.
    pub death_event_id: u8,
    /// Collision radius in room units.
    pub sca_radius: i16,
    /// SCA volume centre height offset (the record's local Y).
    pub sca_half_height: i16,
    /// SCA volume local centre offset X/Y/Z (the record's local position).
    pub sca_offset: [i16; 3],
    /// World-space bias added to the primary SCA volume's rotated centre, the
    /// port's model of a per-frame `pSca_hit_data` retarget (the monster
    /// plant's head and the other bosses' segmented hitboxes). Zero for every
    /// entity whose collision record is not overridden.
    pub sca_hit_delta: [i16; 3],
    /// `pSca_hit_data` for the primary SCA volume: the local record centre
    /// rotated into world space (with [`Entity::sca_hit_delta`] folded in), as
    /// last written by the entity's own update. The original keeps this in a
    /// shared pool that starts zeroed and is only refreshed inside the owning
    /// entity's update, so an entity that has not run yet this frame lends a
    /// zero world centre to every other entity's SCA resolve.
    pub sca_world: [i16; 3],
    /// The second SCA volume's world centre ([`Entity::sca2`]), zero when the
    /// entity carries a single-volume record.
    pub sca_world2: [i16; 3],
    /// Whether the entity has entered a camera switch zone (behaviour
    /// scratch).
    pub has_enter_switch_zone: u8,
    /// Direction the entity is attacking towards (behaviour scratch).
    pub attacking_direction: u8,
    /// Movement direction-control flags (behaviour scratch).
    pub dir_control_flags: u8,
    /// Texture bank the model uses.
    pub tex_bank: u8,
    /// Action sequence counter/timer (behaviour scratch).
    pub seq_counter: u8,
    /// Turn delta accumulated by the walk behaviours.
    pub angle_turn_delta: u8,
    /// Effect countdown (footsteps/blood) for the current movement.
    pub move_timer: u8,
    /// Whether a walk behaviour is moving the entity.
    pub is_moving: u8,
    /// Maximum steps of the current movement sequence.
    pub move_max_steps: u8,
    /// Blood-splatter flag (behaviour scratch).
    pub splatter_flag: u8,
    /// Signed rotation speed for bobbing/weaving.
    pub bob_speed: u8,
    /// Waypoint X the pathfind behaviours steer at.
    pub player_pos_x: i16,
    /// Waypoint Z the pathfind behaviours steer at.
    pub player_pos_z: i16,
    /// 16-bit countdown at entity +0x176 (hit reaction / damage recovery). The
    /// state-9 walk layer reuses the slot as the walk heading (the target yaw
    /// of the current path step), exactly like the original.
    pub reaction_timer: i16,
    /// The four state bytes at entity +0x184 (`state_mirror`,
    /// `ignore_player_flag_mirror`, `action_behavior_mirror`,
    /// `attack_behavior_mirror`), read and written as one dword. The zombie
    /// update mirrors its live state block here every frame and the damage
    /// reaction restores it byte-for-byte.
    pub state_mirror: u32,
    /// The behaviour byte at entity +0x180 (`move_speed`), distinct from the
    /// tween step array: the zombie's chase speed and the short-push-back
    /// upgrade write it.
    pub move_speed_byte: u8,
    /// The behaviour byte at entity +0x181 (`turn_speed`).
    pub turn_speed: u8,
    /// The behaviour byte at entity +0x182 (`internal_timer`), the zombie's
    /// 150-frame moan gate.
    pub internal_timer: u8,
    /// The behaviour byte at entity +0x188 (`stagger_timer`), the poise
    /// budget the bullet counter spends.
    pub stagger_timer: u8,
    /// The 16-bit weave timer at entity +0xE2 (`next_turn_timer`; the player
    /// entity reinterprets the same offset as `attackTimer`).
    pub next_turn_timer: u16,
    /// The mirror-angle backup at entity +0x7E the two-point floor probe
    /// stores on acceptance and reads on rollback.
    pub mirror_angle: u16,
    /// The hunter's partner slot at entity +0x174 (the original stores an
    /// `Entity*` there): the pair the spawn sets up. `pick_partner` writes the
    /// enemy-list head, or the next slot when this hunter is the head. Only
    /// the hunter reads it, and only after its init has picked the partner.
    pub hunter_partner: u8,
    /// The hunter's strafe direction byte at entity +0x185 (`H_STRAFE_DIR`).
    pub scratch_185: u8,
    /// The cross-slot joint-1 world translation the hunter's track helpers
    /// write (the original's `joints[1].world.t[0..2]` plus its `unk_64`
    /// word): the held-player and scripted-bite tracking contract. The port
    /// stores it for the render-side consumer instead of a joint matrix the
    /// animation clock would immediately recompose.
    pub joint_track: [i32; 4],
    /// Bits 1-7 of each joint's flag byte (entity `joints[i] + 0`), the gore
    /// state `joint_setup_attack_effect` and the zombie limb handlers write.
    /// Bit 0 (the active/hidden bit) lives in [`Entity::joint_flags`].
    pub joint_flags_hi: [u8; 32],
    /// The per-joint blood-scratch block `blood_splatter_physics` drives:
    /// counter, flags, X velocity and Y acceleration of the bleeding joint.
    pub joint_blood: [JointBlood; 32],
    /// Per-joint `JointSetColorTint` colours (`r | g << 8 | b << 16`, each a
    /// 1/128 multiplier): a non-zero entry draws that joint unlit in the
    /// colour, the Hunter's blood-red claws after a lunge. Zero is untinted.
    pub joint_colours: [[u8; 3]; 32],
    /// Joint visibility bits XORed by `eml_state` sub-command 9: bit `i` is
    /// joint `i`'s flag bit. Scripted enemies set or clear the same bits
    /// through the `set_joint_visible` script helper. The spiders carry 20
    /// joints, so the mask is 32 bits wide.
    pub joint_flags: u32,
    /// Joints whose active flag bit 0 was cleared by an armed attack effect
    /// (`joint_setup_attack_effect`): bit `i` means joint `i` is armed. The
    /// spiders' `leg_reach` treats an armed joint as inactive and zeroes the
    /// stride; `reset_joints` clears the mask.
    pub joint_armed: u32,
    /// The second SCA volume of a two-record profile (the Black Tiger's walk
    /// and approach boxes). `None` means the single-volume record; the first
    /// volume always lives in `sca_radius`/`sca_half_height`/`sca_offset`.
    pub sca2: Option<ScaVolume>,
    /// The first clone of this entity's web-thread chain, when the spider has
    /// shot one. The chain lives in `GameState::web_clones`; the parent's
    /// script passes the thread count to each update.
    pub clone_head: Option<u16>,
    /// The number of clones the current chain holds (the spawned count the
    /// update walks, capped by the script's requested count).
    pub clone_count: u8,
    /// Scripted ground-shadow queue gate: when set the renderer does not queue
    /// this entity's scripted shadow quad, the original's per-enemy
    /// `entity_add_fade_sprite` test. Defaults to queued.
    pub shadow_suppressed: bool,
    /// Joint world-matrix scale (`pad_ca`, offset 0xCA); `0` means identity
    /// (0x1000) for entities that never set it.
    pub joint_scale: u16,
    /// Scripted ground-shadow quad half-extents; `0` means the entity has no
    /// dynamic quad (characters read theirs from the character table).
    pub shadow_half_x: i16,
    /// Scripted ground-shadow quad half-extent along Z.
    pub shadow_half_z: i16,
    /// Scripted ground-shadow quad local offset (X/Y/Z).
    pub shadow_offset: [i16; 3],
    /// Scripted ground-shadow tint (`0xRRGGBB`).
    pub shadow_tint: u32,
    /// The obstacle pathfinder's state byte (entity +0x164): bits 0-4 are the
    /// frame counter, bit 5 the last line-of-sight result. State 9's
    /// `entity_pathfind_update` cycles it and latches the player waypoint.
    pub pathfind_state: u8,
    /// Low byte of the shared sound-cue timer word (entity +0x17C); see
    /// [`Entity::groan_timer`].
    pub action_speed: u8,
    /// High byte of the shared sound-cue timer word (entity +0x17D).
    pub hit_threshold: u8,
    /// Low byte of the shared wobble word (entity +0x17E); see
    /// [`Entity::sink_wobble`].
    pub behavior_step: u8,
    /// High byte of the shared wobble word (entity +0x17F).
    pub action_counter: u8,
    /// High byte of the scratch word at entity +0x182 (`internal_timer` is the
    /// low byte); the hound's blood-billboard counter re-views the pair.
    pub scratch_183: u8,
    /// First byte of the scratch word at entity +0x186, the hound's behaviour
    /// flags word.
    pub scratch_186: u8,
    /// Second byte of the scratch word at entity +0x186.
    pub scratch_187: u8,
    /// High byte of the scratch word at entity +0x188 (`stagger_timer` is the
    /// low byte); the hound's AI flags re-view the pair.
    pub scratch_189: u8,
    /// The 32-bit fixed accumulator at entity +0x178. The roots machine pins
    /// this through [`Entity::stored_pos_z`]; the wasp re-views its two halves
    /// as the player-distance and room-collision scratch words
    /// ([`Entity::wasp_distance`], [`Entity::wasp_collision`]).
    pub subpixel_pos_x: i32,
    /// The wasp's animation-completion word at entity +0x180: the `Joint_move`
    /// loop flag the sting waits on (`WA_ANIM_DONE`).
    pub wasp_anim_done: u16,
    /// The wasp's big-wasp flag at entity +0x182, consumed from spawn-kind
    /// bit 1 at init. Doubles the shadow, the health and the sting damage and
    /// blocks the grab attack.
    pub wasp_big: u8,
    /// The wasp's nest respawn counter at entity +0x184; the tenth respawn
    /// comes back as nest kind `0x12` (the big wasp).
    pub wasp_respawns: u8,
    /// The wasp's one-shot sound latch at entity +0x18A, so the knockdown
    /// crush cue plays once.
    pub wasp_sound_latch: u8,
    /// The crow's stored `Add_speedXZ` velocity at entity +0x78: the last
    /// rotated speed vector. Y stays zero for the yaw-only rotate; a later
    /// [`crate::enemy::walk::advance_speed`] can re-add the vector without
    /// recomputing the yaw.
    pub speed: [i16; 3],
    /// The crow's floor/step height at entity +0x8E (`CR_FLOOR_STEP`): the step
    /// the room resolver reports when `collision_flags` bit 2 asks for floor
    /// zones, or `0`/`1` when none was crossed.
    pub floor_step: i16,
    /// The crow's swerve angle held by the obstacle steering at entity +0x182
    /// (`CR_SWERVE`).
    pub swerve: i16,
    /// The crow's swerve latch at entity +0x184 (`CR_SWERVE_LATCH`): bit 7
    /// means no swerve is pending, bits 0-6 the frames left.
    pub swerve_latch: u8,
    /// The crow's grab struggle counter at entity +0x186 (`CR_STRUGGLE`), a
    /// signed byte tested `< 0`.
    pub struggle: i8,
    /// The crow's altitude bias at entity +0x18A (`CR_ALT_BIAS`): `0` normally,
    /// `-400` for the scripted, unkillable variant.
    pub alt_bias: i16,
    /// Live model tint, one signed delta per RGB channel in units of 1/31
    /// from white (`0` untinted, `-31` fully dark). The shared additive tint
    /// accumulates here; the renderer shades the mesh with
    /// [`Entity::model_tint_rgb`].
    pub model_tint: [i8; 3],
    /// The model tint queue entry's RGB accumulator bytes, clamped `+/-31`.
    /// The queue record is bound to the entity rather than to a shared
    /// four-slot model-load pool; see [`Entity::queue_model_tint`].
    pub tint_queue: [i8; 3],
    /// The tint queue entry's first parameter word.
    pub tint_queue_word_a: u16,
    /// The tint queue entry's second parameter word.
    pub tint_queue_word_b: u16,
    /// Whether the tint queue entry is armed.
    pub tint_queue_armed: bool,
    /// Plant 42's flower-body companion handle (the port's model of the
    /// `scd_target_ptr` clone pointer at entity +0xB8). `None` for a
    /// split-vine record, which reads the room-shared body instead.
    pub plant42_body: Option<u16>,
    /// Plant 42's root-ball companion handle (the port's model of the clone
    /// pointer at entity +0x178).
    pub plant42_roots: Option<u16>,
    /// The Yawn head slot a body segment mirrors (the port's model of the
    /// segment clone's `scd_target_ptr` back to the head). `None` on the head
    /// itself and on every non-Yawn entity.
    pub yawn_head: Option<u8>,
    /// The joint index (0..14) a Yawn body segment mirrors; only meaningful
    /// when `yawn_head` is set. The clone at enemy-list index `k` tracks
    /// joint `k + 2`.
    pub yawn_joint: u8,
    /// The Tyrant's exposed-heart companion handle (the port's model of the
    /// clone pointer at entity +0x170). `None` for a Tyrant whose init has
    /// not run and for every other entity.
    pub tyrant_heart: Option<u16>,
}

/// The per-joint blood-splatter scratch `blood_splatter_physics` keeps at
/// joint offsets +2 (counter), +3 (flags), +4 (X velocity) and +6 (Y
/// acceleration).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JointBlood {
    /// The joint's `field_02` byte: the at-rest gravity accumulator step.
    pub counter: u8,
    /// The joint's `pad_03` byte: bits 0-4 are the fall frame counter, bit 6
    /// the horizontal bounce direction and bit 7 the stuck latch.
    pub flags: u8,
    /// The joint translation's X word, halved on each wall hit.
    pub vel_x: i16,
    /// The joint translation's Y word, the falling velocity.
    pub vel_y: i16,
    /// The joint translation's Z word, the blood direction's second axis.
    pub rot_z: i16,
}
