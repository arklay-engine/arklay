//! RE1 SCD opcode tables.
//!
//! Three separate opcode spaces exist: the command VM (0x00-0x50), the event
//! VM state-0 table (0x00-0x09) with the shared control ops (0xF4-0xFF), and
//! the two event sub-ISAs entered from state 0: actor (state 1) and tween
//! (state 2). Operand signatures use one character per operand:
//!
//! - `u` u8, `U` u16, `I` i16, `b` i8
//! - `l` command jump label (`if`/`else`), `j` event jump label (`evt_do`)
//! - `L` u16 forward jump label (the port's variant jumps)
//! - `p` payload length (`evt_block`/`evt_single`)
//! - `r` raw remainder, not decoded as operands

/// One opcode table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    /// The opcode byte.
    pub op: u8,
    /// Display mnemonic.
    pub mnemonic: &'static str,
    /// Operand signature, one character per operand.
    pub operands: &'static str,
    /// Command-VM opcode that ends a straight-line run when it returns 0.
    pub condition: bool,
    /// Length in bytes including the opcode; `None` means the width depends on
    /// the operand bytes (0x17, 0x28, 0x33, `evt_block`, `evt_single`,
    /// `evt_do`, `act_motion`) and must be computed by the reader.
    pub width: Option<usize>,
}

const fn op(
    op: u8,
    mnemonic: &'static str,
    operands: &'static str,
    condition: bool,
    width: Option<usize>,
) -> Op {
    Op {
        op,
        mnemonic,
        operands,
        condition,
        width,
    }
}

/// The command-VM opcode table, indexed by opcode (0x00-0x53).
///
/// 0x26 and 0x2E are dead slots: the original handler returns without
/// consuming anything, so the interpreter spins. They are kept as zero-width
/// "hang" rows so the table stays a direct index.
///
/// 0x51 and 0x52 are the port's own: `variant_if` skips forward when the room
/// variant does not match its mask ([`variant_matches`]) and `variant_jump`
/// skips forward always. Neither touches the `if` branch stack, so `return`
/// and a failed condition inside one behave exactly as they would without it.
///
/// 0x53 is the port's too: `message_long` requests a message by an id word
/// (a zero byte, the word, the pause word), whose high byte reaches the room
/// messages past the 64 that `message`'s id byte does
/// ([`crate::text::message_index`]).
pub const COMMAND_OPS: [Op; 84] = [
    op(0x00, "end", "u", false, Some(2)),
    op(0x01, "if", "l", false, Some(2)),
    op(0x02, "else", "l", false, Some(2)),
    op(0x03, "endif", "u", false, Some(2)),
    op(0x04, "ck", "uuu", true, Some(4)),
    op(0x05, "set", "uuu", false, Some(4)),
    op(0x06, "cmpb", "uuu", true, Some(4)),
    op(0x07, "cmpw", "uuuI", true, Some(6)),
    op(0x08, "setb", "uuu", false, Some(4)),
    op(0x09, "cutnext", "u", false, Some(2)),
    op(0x0A, "cutcurr", "u", false, Some(2)),
    op(0x0B, "message", "uU", false, Some(4)),
    op(0x0C, "door_aot_set", "uIIIIuuuuuuIIIIuu", false, Some(26)),
    op(0x0D, "aot_set", "uIIIIuuUUU", false, Some(18)),
    op(0x0E, "nop", "u", false, Some(2)),
    op(0x0F, "scene_setup", "uUUU", false, Some(8)),
    op(0x10, "testitem", "u", true, Some(2)),
    op(0x11, "testpickup", "u", true, Some(2)),
    op(0x12, "aot_reset", "uuuUUU", false, Some(10)),
    op(0x13, "aot_delete", "uuu", false, Some(4)),
    op(0x14, "evt_exec", "uuu", false, Some(4)),
    op(0x15, "bgm_play", "u", false, Some(2)),
    op(0x16, "bgm_stop", "u", false, Some(2)),
    op(0x17, "se_play_3d", "uubuuII", false, None),
    op(0x18, "item_aot_set", "uIIIIuuuuIIIUuuU", false, Some(26)),
    op(0x19, "model_flag_set", "uuu", false, Some(4)),
    op(0x1A, "item_ck", "u", true, Some(2)),
    op(0x1B, "enemy", "uuuuuIUUUIUuuuu", false, Some(22)),
    op(0x1C, "timer_setup", "uIU", false, Some(6)),
    op(0x1D, "ck_last_item", "u", true, Some(2)),
    op(0x1E, "voice_play", "uU", false, Some(4)),
    op(0x1F, "obj", "uuuIIIUuuuuuuuuuuuuuuuu", false, Some(28)),
    op(0x20, "dir_set", "uIIIIII", false, Some(14)),
    op(0x21, "pos_set", "bIIIIII", false, Some(14)),
    op(0x22, "ck_item_count", "uuu", true, Some(4)),
    op(0x23, "cut_auto", "u", false, Some(2)),
    op(0x24, "aot_on", "uuu", false, Some(4)),
    op(0x25, "aot_switch", "uuu", false, Some(4)),
    op(0x26, "hang", "", false, Some(0)),
    op(0x27, "snd_fade_set", "u", false, Some(2)),
    op(0x28, "eml_state", "uuuUu", false, None),
    op(0x29, "movie_on", "u", false, Some(2)),
    op(0x2A, "effect", "uuuIIIU", false, Some(12)),
    op(0x2B, "plw_anim", "uuu", false, Some(4)),
    op(0x2C, "remove_item", "u", false, Some(2)),
    op(0x2D, "give_item", "uuu", false, Some(4)),
    op(0x2E, "hang", "", false, Some(0)),
    op(0x2F, "snd_pan_vol_set", "uuu", false, Some(4)),
    op(0x30, "inst_cfg", "uuuUUUU", false, Some(12)),
    op(0x31, "setw", "uU", false, Some(4)),
    op(0x32, "nop_wide", "uuu", false, Some(4)),
    op(0x33, "sys_multi", "uU", false, None),
    op(0x34, "model_op", "ubuuuuu", false, Some(8)),
    op(0x35, "objtbl_b_set", "uuu", false, Some(4)),
    op(0x36, "ck_anim", "uuu", true, Some(4)),
    op(0x37, "room_bgm_state_set", "uuu", false, Some(4)),
    op(0x38, "ck_bits", "uU", true, Some(4)),
    op(0x39, "get_eml_state", "u", false, Some(2)),
    op(0x3A, "msgnode_set", "uuu", false, Some(4)),
    op(0x3B, "eml_rot", "uUU", false, Some(6)),
    op(0x3C, "ck_counter", "uUU", true, Some(6)),
    op(0x3D, "effect_tracked", "uuuUUUU", false, Some(12)),
    op(0x3E, "effect_kill_a", "u", false, Some(2)),
    op(0x3F, "ck_tween", "uUU", true, Some(6)),
    op(0x40, "obj_xfm", "uIIIIIII", false, Some(16)),
    op(0x41, "spd_set", "uU", false, Some(4)),
    op(0x42, "effect_kill_b", "uU", false, Some(4)),
    op(0x43, "bgm_volume_ramp", "ubu", false, Some(4)),
    op(0x44, "task_kill", "u", false, Some(2)),
    op(0x45, "spd_add", "b", false, Some(2)),
    op(
        0x46,
        "msg_list",
        "uIIIuuuuIIIIuuuuIIIIuuuuIIII",
        false,
        Some(44),
    ),
    op(0x47, "eml_pos", "uIIIIII", false, Some(14)),
    op(0x48, "effect_clear", "u", false, Some(2)),
    op(0x49, "bank_bit", "u", false, Some(2)),
    op(0x4A, "bgm_restore", "u", false, Some(2)),
    op(0x4B, "bgm_stop_all", "u", false, Some(2)),
    op(0x4C, "swap_var", "uuu", false, Some(4)),
    op(0x4D, "objs_hide", "u", false, Some(2)),
    op(0x4E, "mass_mask", "uU", false, Some(4)),
    op(0x4F, "costume_set", "u", false, Some(2)),
    op(0x50, "costume_ck", "u", true, Some(2)),
    op(0x51, "variant_if", "uL", false, Some(4)),
    op(0x52, "variant_jump", "L", false, Some(3)),
    op(0x53, "message_long", "uUU", false, Some(6)),
];

/// Event-VM state-0 opcodes (0x00-0x09), indexed by opcode.
pub const EVENT_TOP_OPS: &[Op] = &[
    op(0x00, "evt_nop", "", false, Some(1)),
    op(0x01, "evt_actor_begin", "", false, Some(1)),
    op(0x02, "evt_tween_begin", "", false, Some(1)),
    op(0x03, "evt_tween_begin_alt", "", false, Some(1)),
    op(0x04, "evt_work_set", "uu", false, Some(3)),
    op(0x05, "evt_fork", "uuu", false, Some(4)),
    op(0x06, "evt_block", "p", false, None),
    op(0x07, "evt_single", "p", false, None),
    op(0x08, "evt_chain", "u", false, Some(2)),
    op(0x09, "evt_disable", "u", false, Some(2)),
];

/// The first event-VM control opcode.
pub const EVENT_CONTROL_BASE: u8 = 0xF4;

/// Event-VM control opcodes (0xF4-0xFF), indexed from [`EVENT_CONTROL_BASE`].
///
/// 0xF4 and 0xF5 are the port's own event-level variant jumps, the
/// counterparts of command 0x51/0x52: `evt_variant_if` skips forward when the
/// room variant does not match its mask and `evt_jump` skips forward always.
/// Neither yields or touches the slot's stacks.
pub const EVENT_CONTROL_OPS: &[Op] = &[
    op(0xF4, "evt_variant_if", "uL", false, Some(4)),
    op(0xF5, "evt_jump", "L", false, Some(3)),
    op(0xF6, "evt_push_cond", "", false, Some(1)),
    op(0xF7, "evt_skip_if", "", false, Some(1)),
    op(0xF8, "evt_sleep", "uU", false, Some(4)),
    op(0xF9, "evt_sleep_tick", "", false, Some(1)),
    op(0xFA, "evt_for", "uU", false, Some(4)),
    op(0xFB, "evt_fornext", "", false, Some(1)),
    op(0xFC, "evt_do", "j", false, Some(2)),
    op(0xFD, "evt_dountil", "", false, Some(1)),
    op(0xFE, "evt_next", "", false, Some(1)),
    op(0xFF, "evt_finish", "", false, Some(1)),
];

/// Event-VM state-1 (actor) opcodes, indexed as 0x00 followed by 0x80-0x8B.
pub const ACTOR_OPS: &[Op] = &[
    op(0x00, "act_nop", "", false, Some(1)),
    op(0x80, "act_reset", "", false, Some(1)),
    op(0x81, "act_motion", "uUUUuu", false, None),
    op(0x82, "act_motion_bitclr", "", false, Some(1)),
    op(0x83, "act_anim_seq", "UUU", false, Some(7)),
    op(0x84, "act_anim_flags", "bbb", false, Some(4)),
    op(0x85, "act_anim_set", "bbb", false, Some(4)),
    op(0x86, "act_idle", "", false, Some(1)),
    op(0x87, "act_flag_op", "uU", false, Some(4)),
    op(0x88, "act_param_set", "Uu", false, Some(4)),
    op(0x89, "act_action_a", "u", false, Some(2)),
    op(0x8A, "act_action_b", "u", false, Some(2)),
    op(0x8B, "act_end", "", false, Some(1)),
];

/// Event-VM state-2 (tween) opcodes (0x00-0x0B), indexed by opcode.
pub const TWEEN_OPS: &[Op] = &[
    op(0x00, "tw_nop", "", false, Some(1)),
    op(0x01, "tw_end", "", false, Some(1)),
    op(0x02, "tw_pos_add", "", false, Some(1)),
    op(0x03, "tw_rot_add", "", false, Some(1)),
    op(0x04, "tw_pos_rot_add", "", false, Some(1)),
    op(0x05, "tw_set_pos", "uuu", false, Some(4)),
    op(0x06, "tw_set_rot", "uuu", false, Some(4)),
    op(0x07, "tw_abs_pos", "uIII", false, Some(8)),
    op(0x08, "tw_set_field", "ub", false, Some(3)),
    op(0x09, "tw_set_8a", "u", false, Some(2)),
    op(0x0A, "tw_set_sel", "uU", false, Some(4)),
    op(0x0B, "tw_abs_rot", "uUUU", false, Some(8)),
];

/// Look up a command-VM opcode.
pub fn command_op(op: u8) -> Option<&'static Op> {
    COMMAND_OPS.get(usize::from(op))
}

/// Look up an event-VM state-0 opcode.
pub fn event_top_op(op: u8) -> Option<&'static Op> {
    EVENT_TOP_OPS.get(usize::from(op))
}

/// Look up an event-VM control opcode (0xF4-0xFF).
pub fn event_control_op(op: u8) -> Option<&'static Op> {
    op.checked_sub(EVENT_CONTROL_BASE)
        .and_then(|index| EVENT_CONTROL_OPS.get(usize::from(index)))
}

/// Look up an event-VM state-1 (actor) opcode.
pub fn actor_op(op: u8) -> Option<&'static Op> {
    match op {
        0x00 => ACTOR_OPS.first(),
        0x80..=0x8B => ACTOR_OPS.get(usize::from(op - 0x80) + 1),
        _ => None,
    }
}

/// Look up an event-VM state-2 (tween) opcode.
pub fn tween_op(op: u8) -> Option<&'static Op> {
    TWEEN_OPS.get(usize::from(op))
}

/// Width in bytes including the opcode, applying the dynamic rules for the
/// variable-width command opcodes 0x17, 0x28 and 0x33.
///
/// Returns `None` when the deciding operand byte is not present in `bytes`.
/// An unknown 0x28/0x33 sub-command returns `Some(0)`: the original handler
/// consumes nothing and the interpreter spins.
pub fn command_width(bytes: &[u8]) -> Option<usize> {
    let entry = command_op(*bytes.first()?)?;
    match entry.op {
        0x17 => {
            let pos_type = *bytes.get(4)?;
            Some(if pos_type <= 3 { 10 } else { 6 })
        }
        0x28 => match *bytes.get(3)? {
            0 | 2 | 3 | 5 | 9 | 10 => Some(6),
            1 => Some(8),
            6 | 8 => Some(4),
            _ => Some(0),
        },
        0x33 => match *bytes.get(1)? {
            1 | 3 | 5 | 8 | 9 | 10 => Some(4),
            0 | 4 | 6 | 7 => Some(2),
            _ => Some(0),
        },
        _ => entry.width,
    }
}

/// The room-variant bits a `variant_if` mask tests: one dimension each for
/// the player, the mansion visit and the release region. The room being run
/// has exactly one bit of each dimension set (the host's `room_variant`).
pub mod variant {
    /// The player is Chris (RDT player digit 0).
    pub const CHRIS: u8 = 0x01;
    /// The player is Jill (RDT player digit 1).
    pub const JILL: u8 = 0x02;
    /// A room outside the mansion (stages 3 to 5).
    pub const OUTSIDE: u8 = 0x04;
    /// A mansion room (stage 1 or 2) on the first visit.
    pub const MANSION1: u8 = 0x08;
    /// A mansion room on the second visit (scenario flag 0 set).
    pub const MANSION2: u8 = 0x10;
    /// The English release.
    pub const USA: u8 = 0x20;
    /// The Japanese release.
    pub const JPN: u8 = 0x40;
    /// The French and German releases.
    pub const EUR: u8 = 0x80;

    /// The player dimension's bits.
    pub const PLAYER: u8 = CHRIS | JILL;
    /// The mansion dimension's bits.
    pub const MANSION: u8 = OUTSIDE | MANSION1 | MANSION2;
    /// The region dimension's bits.
    pub const REGION: u8 = USA | JPN | EUR;
    /// Every dimension.
    pub const DIMENSIONS: [u8; 3] = [PLAYER, MANSION, REGION];
}

/// Flag bank 5 bits the port adds, the same in every game and never used by
/// the originals. A randomizer's pack raises them, so a room's script can turn
/// off what only makes sense in the game's own order (`if (RANDOMIZER)`).
pub mod system {
    /// The pack is a randomizer's.
    pub const RANDOMIZER: u8 = 0xF0;
    /// The doors lead somewhere other than the original's rooms.
    pub const DOOR_RANDOMIZER: u8 = 0xF1;
    /// The enemies are not the ones the room's script places.
    pub const ENEMY_RANDOMIZER: u8 = 0xF2;
}

/// Whether the room variant `current` (one bit of each dimension) matches
/// `mask`: in every dimension where the mask sets any bit, `current`'s bit
/// is one of them. A dimension the mask leaves clear always matches.
pub fn variant_matches(mask: u8, current: u8) -> bool {
    variant::DIMENSIONS
        .iter()
        .all(|&dimension| mask & dimension == 0 || mask & current & dimension != 0)
}

/// Every room variant's bits: one per player, mansion visit and region.
const VARIANT_COMBOS: [u8; 18] = {
    use variant::*;
    let players = [CHRIS, JILL];
    let mansions = [OUTSIDE, MANSION1, MANSION2];
    let regions = [USA, JPN, EUR];
    let mut combos = [0u8; 18];
    let mut index = 0;
    while index < 18 {
        combos[index] = players[index / 9] | mansions[index / 3 % 3] | regions[index % 3];
        index += 1;
    }
    combos
};

/// The event-VM state (0 top level, 1 actor, 2 tween) each room variant
/// reaches an instruction in, across the variant jumps.
///
/// An event stream decodes each byte in one state, and an instruction is
/// valid only where every variant reaching it agrees on the state. Without
/// variant jumps every variant runs the same instructions, so the flow is
/// the plain linear state. A variant jump splits the variants between its
/// branches, and its target gathers them again: a branch may leave its
/// variants in another state than the other branch, which only matters if
/// an instruction both reach depends on the state (anything but a control
/// opcode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventFlow {
    states: [Option<u8>; 18],
}

impl EventFlow {
    /// Every variant, in `state`.
    pub fn new(state: u8) -> Self {
        Self {
            states: [Some(state); 18],
        }
    }

    /// No variant: the code after an unconditional jump.
    pub fn unreachable() -> Self {
        Self { states: [None; 18] }
    }

    /// Gather the variants of `other` (a jump's) in.
    pub fn merge(&mut self, other: &EventFlow) {
        for (mine, theirs) in self.states.iter_mut().zip(other.states) {
            if mine.is_none() {
                *mine = theirs;
            }
        }
    }

    /// The one state every variant here is in; `None` when they differ or
    /// none reaches here.
    pub fn state(&self) -> Option<u8> {
        let mut states = self.states.iter().flatten();
        let first = *states.next()?;
        states.all(|&state| state == first).then_some(first)
    }

    /// Whether any variant reaches here.
    pub fn is_reachable(&self) -> bool {
        self.states.iter().any(Option::is_some)
    }

    /// Move every variant here to `state`.
    pub fn set(&mut self, state: u8) {
        for slot in self.states.iter_mut().flatten() {
            *slot = state;
        }
    }

    /// The variants a variant jump with `mask` lets through, and those it
    /// sends to its target.
    pub fn split(&self, mask: u8) -> (EventFlow, EventFlow) {
        let mut taken = Self::unreachable();
        let mut skipped = Self::unreachable();
        for (index, state) in self.states.iter().enumerate() {
            if variant_matches(mask, VARIANT_COMBOS[index]) {
                taken.states[index] = *state;
            } else {
                skipped.states[index] = *state;
            }
        }
        (taken, skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operand_bytes(signature: &str) -> usize {
        signature
            .bytes()
            .map(|byte| match byte {
                b'U' | b'I' | b'L' => 2,
                _ => 1,
            })
            .sum()
    }

    #[test]
    fn command_table_indexes_every_opcode() {
        assert_eq!(COMMAND_OPS.len(), 84);
        for (index, entry) in COMMAND_OPS.iter().enumerate() {
            assert_eq!(usize::from(entry.op), index);
            assert_eq!(command_op(entry.op), Some(entry));
        }
        assert_eq!(command_op(0x54), None);
    }

    #[test]
    fn fixed_width_matches_operand_signature() {
        for entry in &COMMAND_OPS {
            let Some(width) = entry.width else {
                continue;
            };
            if width == 0 {
                assert_eq!(entry.operands, "");
                continue;
            }
            assert_eq!(
                operand_bytes(entry.operands) + 1,
                width,
                "opcode {:#04X} ({})",
                entry.op,
                entry.mnemonic
            );
        }
    }

    #[test]
    fn condition_set_is_exactly_the_thirteen_handlers() {
        let conditions: Vec<u8> = COMMAND_OPS
            .iter()
            .filter(|entry| entry.condition)
            .map(|entry| entry.op)
            .collect();
        assert_eq!(
            conditions,
            vec![
                0x04, 0x06, 0x07, 0x10, 0x11, 0x1A, 0x1D, 0x22, 0x36, 0x38, 0x3C, 0x3F, 0x50
            ]
        );
    }

    #[test]
    fn dynamic_command_widths() {
        assert_eq!(command_width(&[0x17, 0, 0, 0, 3]), Some(10));
        assert_eq!(command_width(&[0x17, 0, 0, 0, 4]), Some(6));
        assert_eq!(command_width(&[0x17, 0, 0, 0]), None);

        assert_eq!(command_width(&[0x28, 0, 0, 0]), Some(6));
        assert_eq!(command_width(&[0x28, 0, 0, 1]), Some(8));
        assert_eq!(command_width(&[0x28, 0, 0, 6]), Some(4));
        assert_eq!(command_width(&[0x28, 0, 0, 8]), Some(4));
        assert_eq!(command_width(&[0x28, 0, 0, 4]), Some(0));

        assert_eq!(command_width(&[0x33, 0]), Some(2));
        assert_eq!(command_width(&[0x33, 3]), Some(4));
        assert_eq!(command_width(&[0x33, 10]), Some(4));
        assert_eq!(command_width(&[0x33, 2]), Some(0));

        assert_eq!(command_width(&[0x26]), Some(0));
        assert_eq!(command_width(&[0x2E]), Some(0));
        assert_eq!(command_width(&[0x00]), Some(2));
        assert_eq!(command_width(&[0xFF]), None);
    }

    #[test]
    fn event_tables_lookup_by_opcode() {
        assert_eq!(event_top_op(0x00).unwrap().mnemonic, "evt_nop");
        assert_eq!(event_top_op(0x09).unwrap().mnemonic, "evt_disable");
        assert_eq!(event_top_op(0x0A), None);

        assert_eq!(event_control_op(0xF6).unwrap().mnemonic, "evt_push_cond");
        assert_eq!(event_control_op(0xFF).unwrap().mnemonic, "evt_finish");
        assert_eq!(event_control_op(0xF4).unwrap().mnemonic, "evt_variant_if");
        assert_eq!(event_control_op(0xF3), None);

        assert_eq!(actor_op(0x00).unwrap().mnemonic, "act_nop");
        assert_eq!(actor_op(0x80).unwrap().mnemonic, "act_reset");
        assert_eq!(actor_op(0x8B).unwrap().mnemonic, "act_end");
        assert_eq!(actor_op(0x01), None);
        assert_eq!(actor_op(0x8C), None);

        assert_eq!(tween_op(0x00).unwrap().mnemonic, "tw_nop");
        assert_eq!(tween_op(0x0B).unwrap().mnemonic, "tw_abs_rot");
        assert_eq!(tween_op(0x0C), None);
    }

    #[test]
    fn variant_masks_constrain_each_dimension_separately() {
        use variant::*;
        let jill_m2_usa = JILL | MANSION2 | USA;
        assert!(variant_matches(0, jill_m2_usa));
        assert!(variant_matches(JILL, jill_m2_usa));
        assert!(!variant_matches(CHRIS, jill_m2_usa));
        assert!(variant_matches(JILL | MANSION2, jill_m2_usa));
        assert!(!variant_matches(JILL | MANSION1, jill_m2_usa));
        assert!(variant_matches(OUTSIDE | MANSION2, jill_m2_usa));
        assert!(variant_matches(USA | EUR, jill_m2_usa));
        assert!(!variant_matches(JPN, jill_m2_usa));
        // Outside the mansion neither visit matches.
        assert!(!variant_matches(MANSION1, CHRIS | OUTSIDE | USA));
    }
}
