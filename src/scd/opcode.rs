//! RE1 SCD opcode tables.
//!
//! Three separate opcode spaces exist: the command VM (0x00-0x54), the event
//! VM state-0 table (0x00-0x09) with the shared control ops (0xF4-0xFF), and
//! the two event sub-ISAs entered from state 0: actor (state 1) and tween
//! (state 2). Operand signatures use one character per operand:
//!
//! - `u` u8, `U` u16, `I` i16, `b` i8
//! - `l` command jump label (`if`/`else`), `j` event jump label (`evt_do`)
//! - `L` u16 forward jump label (the port's long `if`/`else` and event jumps)
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

/// The command-VM opcode table, indexed by opcode (0x00-0x54).
///
/// 0x26 and 0x2E are dead slots: the original handler returns without
/// consuming anything, so the interpreter spins. They are kept as zero-width
/// "hang" rows so the table stays a direct index.
///
/// 0x51 and 0x52 are the port's own: `if_long` and `else_long` are `if` and
/// `else` with a `u16` jump, for a body past the 255 bytes the original's
/// one-byte jump reaches. Each behaves exactly as its short form; their jump
/// is a forward delta from the start of the instruction, to where the flow
/// resumes (the `else` body or past the `endif` for `if_long`, past the
/// `else` body for `else_long`).
///
/// 0x53 is the port's too: `message_long` requests a message by an id word
/// (a zero byte, the word, the pause word), whose high byte reaches the room
/// messages past the 64 that `message`'s id byte does
/// ([`crate::text::message_index`]).
///
/// 0x54 is the port's too: `gosub` runs user procedure N, block N of the
/// container's procedure list, to its end and comes back. The procedure has
/// its own `if` branch stack: its `return`, a failed condition at its top
/// level or its last `end 0` returns to the instruction after the call.
pub const COMMAND_OPS: [Op; 85] = [
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
    op(0x40, "cam_set", "uIIIIIII", false, Some(16)),
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
    op(0x51, "if_long", "L", false, Some(3)),
    op(0x52, "else_long", "L", false, Some(3)),
    op(0x53, "message_long", "uUU", false, Some(6)),
    op(0x54, "gosub", "u", false, Some(2)),
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
/// 0xF4 and 0xF5 are the port's own: the original's event VM has no `if`.
/// `evt_if` is followed by condition commands, as `evt_do` is, up to its
/// first operand (the delta from the instruction to the first body byte);
/// when one fails, the event continues at its second operand (a `u16`
/// forward delta from the instruction). `evt_jump` skips forward always,
/// over an `else` body.
/// Neither yields or touches the slot's stacks.
pub const EVENT_CONTROL_OPS: &[Op] = &[
    op(0xF4, "evt_if", "jL", false, Some(4)),
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

/// The room-variant bits a variant test (`if (JILL)`) masks: one dimension
/// each for the player, the mansion visit and the release region. A build
/// for one RDT variant has exactly one bit of each dimension set and folds
/// the tests away; the runtime build tests the flags that stand for them.
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

    /// The status flag bank, which holds the player and release bits.
    pub const FLAG_BANK_STATUS: u8 = 5;
    /// The status bit set while the player is Jill
    /// ([`crate::game::MSF_CHAR_VARIANT`]).
    pub const FLAG_JILL: u8 = 8;
    /// The port's status bit for the Japanese release
    /// ([`crate::game::MSF_REGION_JPN`]).
    pub const FLAG_REGION_JPN: u8 = 4;
    /// The port's status bit for the French and German releases
    /// ([`crate::game::MSF_REGION_EUR`]).
    pub const FLAG_REGION_EUR: u8 = 10;
    /// The scenario flag bank.
    pub const FLAG_BANK_SCENARIO: u8 = 0;
    /// The scenario flag set for the second mansion visit
    /// ([`crate::game::SCENARIO_FLAG_MANSION2`]).
    pub const FLAG_MANSION2: u8 = 0;
}

/// Flag bank 5 bits the port adds, in the range BioRand reserves (`0xC0` to
/// `0xFF`; the originals' scripts use only the bits below it). A generated
/// pack's manifest raises the ones it needs on every path into a session
/// (`biorand_flags`, [`crate::manifest::Manifest::system_flags`]), and an
/// ordinary pack leaves the bank alone.
///
/// Only the bits are defined so far: nothing in the engine reads them, and the
/// script compiler does not know the names yet, so a room script can test one
/// only by its group and bit (`ck(FG_STATUS, 0xC3, TRUE)`). The engine behaviour each name
/// describes (doors, locks, enemies, the BioRand patches) is not ported.
pub mod system {
    /// The flag bank the system flags live in.
    pub const BANK: u8 = 5;

    /// The pack is a generated game: a manifest with a `seed` raises it.
    pub const RANDOMIZER: u8 = 0xC0;
    /// Doors are randomized (`doors/random`).
    pub const DOOR_RANDOMIZER: u8 = 0xC1;
    /// Enemies are randomized (`enemies/random`).
    pub const ENEMY_RANDOMIZER: u8 = 0xC2;
    /// Items are randomized (`items/random`).
    pub const ITEM_RANDOMIZER: u8 = 0xC3;
    /// Locks are randomized (`locks/random`).
    pub const LOCK_RANDOMIZER: u8 = 0xC4;
    /// Cutscenes are disabled (`cutscenes/disable`); patches.md section E
    /// lists the guards on this flag.
    pub const CUTSCENES_DISABLED: u8 = 0xC5;
    /// `progression/tyrant2` is Always, so the rooftop Tyrant is forced
    /// (patches.md section B, `ForceHelipadTyrant`).
    pub const FORCE_TYRANT2: u8 = 0xC6;
    /// The generated game reaches the laboratory.
    pub const LAB: u8 = 0xC7;
    /// BioRand patches 16 to 21: the whole lock byte is the lock flag index,
    /// for 256 lock flags (patches.md section A).
    pub const BR_LOCK_FULL_BYTE: u8 = 0xC8;
    /// BioRand patches 5 and 6: Neptune can die (Cerberus's death routine) and
    /// takes weapon damage.
    pub const BR_NEPTUNE_KILLABLE: u8 = 0xC9;
    /// BioRand patch 7: under door randomization, Yawn's bite gives ordinary
    /// poison, which a blue herb cures, with no trip to room 100.
    pub const BR_YAWN_POISON: u8 = 0xCA;
    /// BioRand patches 8 and 9: the extra blue herb mixes, two of which are
    /// BioRand's own (patches.md section A).
    pub const BR_HERB_MIXES: u8 = 0xCB;
    /// BioRand patches 11 to 13: ink reads scenario flag 122 for both
    /// characters.
    pub const BR_INK_FLAG: u8 = 0xCC;
    /// BioRand patch 1: no attract demo from the title screen.
    pub const BR_NO_TITLE_DEMO: u8 = 0xCD;

    /// Every system flag, by the name scripts and manifests give it.
    pub const BY_NAME: [(&str, u8); 14] = [
        ("RANDOMIZER", RANDOMIZER),
        ("DOOR_RANDOMIZER", DOOR_RANDOMIZER),
        ("ENEMY_RANDOMIZER", ENEMY_RANDOMIZER),
        ("ITEM_RANDOMIZER", ITEM_RANDOMIZER),
        ("LOCK_RANDOMIZER", LOCK_RANDOMIZER),
        ("CUTSCENES_DISABLED", CUTSCENES_DISABLED),
        ("FORCE_TYRANT2", FORCE_TYRANT2),
        ("LAB", LAB),
        ("BR_LOCK_FULL_BYTE", BR_LOCK_FULL_BYTE),
        ("BR_NEPTUNE_KILLABLE", BR_NEPTUNE_KILLABLE),
        ("BR_YAWN_POISON", BR_YAWN_POISON),
        ("BR_HERB_MIXES", BR_HERB_MIXES),
        ("BR_INK_FLAG", BR_INK_FLAG),
        ("BR_NO_TITLE_DEMO", BR_NO_TITLE_DEMO),
    ];

    /// The proof of concept's `RANDOMIZER`, `DOOR_RANDOMIZER` and
    /// `ENEMY_RANDOMIZER`, before they moved to `0xC0`: a pack it generated
    /// (a seed and no `biorand_flags`) tests these bits, so the engine raises
    /// them for such a pack, and clears them for any other.
    pub const PROOF_OF_CONCEPT: [u8; 3] = [0xF0, 0xF1, 0xF2];

    /// Every bank-5 bit the engine sets or clears from a pack's manifest.
    pub fn managed() -> impl Iterator<Item = u8> {
        BY_NAME.iter().map(|&(_, bit)| bit).chain(PROOF_OF_CONCEPT)
    }

    /// The bit of the system flag `name` names, or `None` for any other name.
    pub fn bit_named(name: &str) -> Option<u8> {
        BY_NAME
            .iter()
            .find(|(known, _)| *known == name)
            .map(|&(_, bit)| bit)
    }
}

/// Whether the room variant `current` (one bit of each dimension) matches
/// `mask`: in every dimension where the mask sets any bit, `current`'s bit
/// is one of them. A dimension the mask leaves clear always matches.
pub fn variant_matches(mask: u8, current: u8) -> bool {
    variant::DIMENSIONS
        .iter()
        .all(|&dimension| mask & dimension == 0 || mask & current & dimension != 0)
}

/// One room variant: a bit of each dimension of [`variant`] (an outside
/// room has no visit to test).
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

/// Whether condition command `bytes` may pass and may fail in room variant
/// `combo`. A `ck` of a flag that tells the variant
/// ([`variant::FLAG_JILL`] and the rest) is fixed by it; any other
/// condition may go either way.
fn condition_outcomes(bytes: &[u8], combo: u8) -> (bool, bool) {
    use variant::*;
    let &[0x04, bank, flag, mode] = bytes else {
        return (true, true);
    };
    // `ck` passes on a set flag for mode 0.
    let set = mode == 0;
    let bit = match (bank, flag) {
        (FLAG_BANK_STATUS, FLAG_JILL) => combo & JILL != 0,
        (FLAG_BANK_STATUS, FLAG_REGION_JPN) => combo & JPN != 0,
        (FLAG_BANK_STATUS, FLAG_REGION_EUR) => combo & EUR != 0,
        // Outside the mansion the visit flag tells nothing.
        (FLAG_BANK_SCENARIO, FLAG_MANSION2) if combo & OUTSIDE == 0 => combo & MANSION2 != 0,
        _ => return (true, true),
    };
    (bit == set, bit != set)
}

/// The event-VM state (0 top level, 1 actor, 2 tween) each room variant
/// reaches an instruction in, across the port's event `if`s and jumps.
///
/// An event stream decodes each byte in one state. Without jumps the state
/// is linear. An `evt_if` or `evt_jump` sends the flow to its target too,
/// where it meets the flow that ran into it. A variant can reach a point in
/// two states (one branch began or ended an actor or tween sequence the
/// other did not), and then only a control opcode, which is valid in every
/// state, may follow. An `evt_if` that tests only the flags that tell the
/// variant sends each variant down the one branch it takes, so the branches
/// of a test of the player, say, may each leave the event in its own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventFlow {
    states: [FlowState; 18],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowState {
    /// The variant does not reach here.
    Unreachable,
    State(u8),
    /// The variant reaches here in different states.
    Mixed,
}

impl EventFlow {
    /// Every variant, in `state`.
    pub fn new(state: u8) -> Self {
        Self {
            states: [FlowState::State(state); 18],
        }
    }

    /// No variant: the code after an unconditional jump.
    pub fn unreachable() -> Self {
        Self {
            states: [FlowState::Unreachable; 18],
        }
    }

    /// Gather the flow of `other` (a jump's) in.
    pub fn merge(&mut self, other: &EventFlow) {
        for (mine, theirs) in self.states.iter_mut().zip(other.states) {
            *mine = match (*mine, theirs) {
                (FlowState::Unreachable, theirs) => theirs,
                (mine, FlowState::Unreachable) => mine,
                (mine, theirs) if mine == theirs => mine,
                _ => FlowState::Mixed,
            };
        }
    }

    /// The one state every variant here is in; `None` when they differ or
    /// none reaches here.
    pub fn state(&self) -> Option<u8> {
        let mut reached = self
            .states
            .iter()
            .filter(|state| **state != FlowState::Unreachable);
        let FlowState::State(first) = *reached.next()? else {
            return None;
        };
        reached
            .all(|state| *state == FlowState::State(first))
            .then_some(first)
    }

    /// Whether any variant reaches here.
    pub fn is_reachable(&self) -> bool {
        self.states
            .iter()
            .any(|state| *state != FlowState::Unreachable)
    }

    /// Move every variant here to `state`.
    pub fn set(&mut self, state: u8) {
        for slot in &mut self.states {
            if *slot != FlowState::Unreachable {
                *slot = FlowState::State(state);
            }
        }
    }

    /// The flow into the body of an `evt_if` of the condition commands
    /// `conditions` (their bytes), and the flow it sends to its target.
    pub fn branch<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a [u8]> + Clone,
    ) -> (Self, Self) {
        let mut taken = Self::unreachable();
        let mut skipped = Self::unreachable();
        for (index, state) in self.states.iter().enumerate() {
            let combo = VARIANT_COMBOS[index];
            let outcomes = conditions
                .clone()
                .into_iter()
                .map(|bytes| condition_outcomes(bytes, combo));
            let (mut all_may_pass, mut any_may_fail) = (true, false);
            for (may_pass, may_fail) in outcomes {
                all_may_pass &= may_pass;
                any_may_fail |= may_fail;
            }
            if all_may_pass {
                taken.states[index] = *state;
            }
            if any_may_fail {
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
    fn system_flags_are_distinct_bits_in_the_reserved_range() {
        for (index, (name, bit)) in system::BY_NAME.iter().enumerate() {
            assert!((0xC0..=0xFF).contains(bit), "{name} is outside 0xC0..=0xFF");
            assert_eq!(system::bit_named(name), Some(*bit), "{name}");
            for (other, other_bit) in &system::BY_NAME[..index] {
                assert!(other != name && other_bit != bit, "{name} repeats {other}");
            }
        }
        for bit in system::PROOF_OF_CONCEPT {
            assert!(
                (0xD0..=0xFF).contains(&bit),
                "{bit:#x} is outside the spare bits"
            );
            assert!(system::BY_NAME.iter().all(|&(_, named)| named != bit));
        }
        assert_eq!(system::managed().count(), system::BY_NAME.len() + 3);
        assert_eq!(system::bit_named("NOT_A_FLAG"), None);
        assert_eq!(system::bit_named("lab"), None, "names are exact");
    }

    #[test]
    fn command_table_indexes_every_opcode() {
        assert_eq!(COMMAND_OPS.len(), 85);
        for (index, entry) in COMMAND_OPS.iter().enumerate() {
            assert_eq!(usize::from(entry.op), index);
            assert_eq!(command_op(entry.op), Some(entry));
        }
        assert_eq!(command_op(0x55), None);
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
        assert_eq!(event_control_op(0xF4).unwrap().mnemonic, "evt_if");
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
