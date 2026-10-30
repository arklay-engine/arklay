//! RE2 SCD opcode table.
//!
//! RE2 has one script VM with one opcode space, 0x00-0x8E, dispatched
//! through `Sce_jmp_tbl` (0x53AE10 in `bio2 1.10.exe`). Every opcode has a
//! fixed width; each handler advances the data pointer itself, so the
//! widths are not a table in the executable. They follow biohazard-utils'
//! `Bio2ConstantTable`, except that `endif` is two bytes (its handler skips
//! two), and every retail room script decodes with them. The mnemonics are
//! biohazard-utils' where it names the opcode and otherwise the handler's
//! name from the IDA database.
//!
//! Operand signatures use one character per operand:
//!
//! - `u` u8, `b` i8, `U` u16, `I` i16
//! - `L` u16 block length, counted from the end of the instruction
//! - `J` i16 jump, counted from the start of the instruction
//! - `C` u8 length of the inline condition region (`while`, `edwhile`)

/// One opcode table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    /// The opcode byte.
    pub op: u8,
    /// Display mnemonic.
    pub mnemonic: &'static str,
    /// Operand signature, one character per operand; it covers every byte
    /// after the opcode.
    pub operands: &'static str,
    /// The handler can return 0, which abandons the enclosing `if` (or,
    /// outside one, retries the instruction next frame).
    pub condition: bool,
    /// Width in bytes, including the opcode.
    pub width: usize,
}

const fn op(op: u8, mnemonic: &'static str, operands: &'static str, width: usize) -> Op {
    Op {
        op,
        mnemonic,
        operands,
        condition: false,
        width,
    }
}

const fn cond(op: u8, mnemonic: &'static str, operands: &'static str, width: usize) -> Op {
    Op {
        op,
        mnemonic,
        operands,
        condition: true,
        width,
    }
}

pub const NOP: u8 = 0x00;
pub const EVT_END: u8 = 0x01;
pub const IF: u8 = 0x06;
pub const ELSE: u8 = 0x07;
pub const ENDIF: u8 = 0x08;
pub const FOR: u8 = 0x0D;
pub const NEXT: u8 = 0x0E;
pub const WHILE: u8 = 0x0F;
pub const EWHILE: u8 = 0x10;
pub const DO: u8 = 0x11;
pub const EDWHILE: u8 = 0x12;
pub const SWITCH: u8 = 0x13;
pub const CASE: u8 = 0x14;
pub const DEFAULT: u8 = 0x15;
pub const ESWITCH: u8 = 0x16;
pub const GOTO: u8 = 0x17;
pub const FOR2: u8 = 0x1B;

/// The opcode table, indexed by opcode.
pub const OPS: [Op; 0x8F] = [
    op(0x00, "nop", "", 1),
    op(0x01, "evt_end", "u", 2),
    op(0x02, "evt_next", "", 1),
    op(0x03, "evt_chain", "uuu", 4),
    op(0x04, "evt_exec", "uuu", 4),
    op(0x05, "evt_kill", "u", 2),
    op(0x06, "if", "uL", 4),
    op(0x07, "else", "uJ", 4),
    op(0x08, "endif", "u", 2),
    op(0x09, "sleep", "uU", 4),
    op(0x0A, "sleeping", "U", 3),
    op(0x0B, "wsleep", "", 1),
    op(0x0C, "wsleeping", "", 1),
    op(0x0D, "for", "uLU", 6),
    op(0x0E, "next", "u", 2),
    op(0x0F, "while", "CL", 4),
    op(0x10, "ewhile", "u", 2),
    op(0x11, "do", "uL", 4),
    op(0x12, "edwhile", "C", 2),
    op(0x13, "switch", "uL", 4),
    op(0x14, "case", "uLU", 6),
    op(0x15, "default", "u", 2),
    op(0x16, "eswitch", "u", 2),
    op(0x17, "goto", "uuuJ", 6),
    op(0x18, "gosub", "u", 2),
    op(0x19, "return", "u", 2),
    op(0x1A, "break", "u", 2),
    op(0x1B, "for2", "uLuu", 6),
    op(0x1C, "nop_1c", "", 1),
    op(0x1D, "work_copy", "uuu", 4),
    op(0x1E, "nop_1e", "", 1),
    op(0x1F, "nop_1f", "", 1),
    op(0x20, "nop_20", "", 1),
    cond(0x21, "ck", "uuu", 4),
    op(0x22, "set", "uuu", 4),
    cond(0x23, "cmp", "uuuI", 6),
    op(0x24, "save", "uI", 4),
    op(0x25, "copy", "uu", 3),
    op(0x26, "calc", "uuuI", 6),
    op(0x27, "calc2", "uuu", 4),
    op(0x28, "sce_rnd", "", 1),
    op(0x29, "cut_chg", "u", 2),
    op(0x2A, "cut_old", "", 1),
    op(0x2B, "message_on", "uuuuu", 6),
    op(0x2C, "aot_set", "uuuuuIIIIuuuuuu", 20),
    op(0x2D, "obj_model_set", "uuuuuUUIIIIIIIIIIIIuuuu", 38),
    op(0x2E, "work_set", "uu", 3),
    op(0x2F, "speed_set", "uI", 4),
    op(0x30, "add_speed", "", 1),
    op(0x31, "add_aspeed", "", 1),
    op(0x32, "pos_set", "uIII", 8),
    op(0x33, "dir_set", "uIII", 8),
    op(0x34, "member_set", "uI", 4),
    op(0x35, "member_set2", "uu", 3),
    op(0x36, "se_on", "uIIIII", 12),
    op(0x37, "sca_id_set", "uU", 4),
    op(0x38, "flr_set", "uu", 3),
    cond(0x39, "dir_ck", "uIII", 8),
    op(0x3A, "sce_espr_on", "uUUUIIII", 16),
    op(0x3B, "door_aot_set", "uuuuuIIIIIIIIuuuuuuuuuu", 32),
    op(0x3C, "cut_auto", "u", 2),
    op(0x3D, "member_copy", "uu", 3),
    cond(0x3E, "member_cmp", "uuuI", 6),
    op(0x3F, "plc_motion", "uuu", 4),
    op(0x40, "plc_dest", "uuuII", 8),
    op(0x41, "plc_neck", "uIIIuu", 10),
    op(0x42, "plc_ret", "", 1),
    op(0x43, "plc_flg", "uU", 4),
    op(0x44, "sce_em_set", "uuuuuuuuuIIIIUU", 22),
    op(0x45, "col_chg_set", "uuuu", 5),
    op(0x46, "aot_reset", "uuuuuuuuu", 10),
    op(0x47, "aot_on", "u", 2),
    op(0x48, "super_set", "uuuIIIIII", 16),
    op(0x49, "super_reset", "uIII", 8),
    op(0x4A, "plc_gun", "u", 2),
    op(0x4B, "cut_replace", "uu", 3),
    op(0x4C, "sce_espr_kill", "uuuu", 5),
    op(0x4D, "door_model_set", "uuuuuUIIIIUUuu", 22),
    op(0x4E, "item_aot_set", "uuuuuIIUUUUUuu", 22),
    cond(0x4F, "sce_key_ck", "uU", 4),
    cond(0x50, "sce_trg_ck", "uU", 4),
    op(0x51, "sce_bgm_control", "uuuuu", 6),
    op(0x52, "sce_espr_control", "uuuuu", 6),
    op(0x53, "sce_fade_set", "uuuI", 6),
    op(0x54, "sce_espr3d_on", "uUUUIIIIIII", 22),
    op(0x55, "member_calc", "uUI", 6),
    op(0x56, "member_calc2", "uuu", 4),
    op(0x57, "sce_bgmtbl_set", "uuuUU", 8),
    op(0x58, "plc_rot", "uU", 4),
    op(0x59, "xa_on", "uU", 4),
    op(0x5A, "weapon_chg", "u", 2),
    op(0x5B, "plc_cnt", "u", 2),
    op(0x5C, "sce_shake_on", "bb", 3),
    cond(0x5D, "mizu_div_set", "u", 2),
    cond(0x5E, "keep_item_ck", "u", 2),
    op(0x5F, "xa_vol", "u", 2),
    op(0x60, "kage_set", "uuuuuIIII", 14),
    op(0x61, "cut_be_set", "uuu", 4),
    op(0x62, "sce_item_lost", "u", 2),
    op(0x63, "nop_63", "", 1),
    op(0x64, "sce_espr_on2", "uUUUIIIU", 16),
    op(0x65, "sce_espr_kill2", "u", 2),
    op(0x66, "plc_stop", "", 1),
    op(0x67, "aot_set_4p", "uuuuuIIIIIIIIuuuuuu", 28),
    op(0x68, "door_aot_set_4p", "uuuuuIIIIIIIIIIIIuuuuuuuuuu", 40),
    op(0x69, "item_aot_set_4p", "uuuuuIIIIIIIIUUUuu", 30),
    op(0x6A, "light_pos_set", "uuuI", 6),
    op(0x6B, "light_kido_set", "uI", 4),
    op(0x6C, "rbj_reset", "", 1),
    op(0x6D, "sce_scr_move", "uI", 4),
    op(0x6E, "parts_set", "uuuI", 6),
    op(0x6F, "movie_on", "u", 2),
    op(0x70, "splc_ret", "", 1),
    op(0x71, "splc_sce", "", 1),
    op(0x72, "super_on", "uuuIIIIII", 16),
    op(0x73, "mirror_set", "uUUU", 8),
    op(0x74, "sce_fade_adjust", "uI", 4),
    op(0x75, "sce_espr3d_on2", "uUUUIIIIIIU", 22),
    cond(0x76, "sce_item_get", "uu", 3),
    op(0x77, "sce_line_start", "uU", 4),
    op(0x78, "sce_line_main", "uII", 6),
    op(0x79, "sce_line_end", "", 1),
    op(0x7A, "sce_parts_bomb", "uuuuuIIIII", 16),
    op(0x7B, "sce_parts_down", "uIIIuuIII", 16),
    op(0x7C, "light_color_set", "uuuuu", 6),
    op(0x7D, "light_pos_set2", "uuuI", 6),
    op(0x7E, "light_kido_set2", "uuuU", 6),
    op(0x7F, "light_color_set2", "uuuuu", 6),
    op(0x80, "se_vol", "u", 2),
    cond(0x81, "sce_81", "uu", 3),
    op(0x82, "sce_82", "uu", 3),
    op(0x83, "sce_heal", "", 1),
    op(0x84, "sce_84", "u", 2),
    cond(0x85, "sce_85", "uuuU", 6),
    cond(0x86, "poison_ck", "", 1),
    op(0x87, "poison_clr", "", 1),
    op(0x88, "sce_item_ck_lost", "uu", 3),
    op(0x89, "heal_partner", "", 1),
    op(0x8A, "vib_set0", "uUU", 6),
    op(0x8B, "vib_set1", "uUU", 6),
    op(0x8C, "vib_fade_set", "uUUU", 8),
    op(0x8D, "sce_8d", "uIIIIIIIIUuuuu", 24),
    op(0x8E, "sce_em_set2", "uuuuuuuuuIIIIUUuu", 24),
];

/// The table row of opcode `byte`, or `None` past the table.
pub fn lookup(byte: u8) -> Option<&'static Op> {
    OPS.get(usize::from(byte))
}

/// The table row named `mnemonic`.
pub fn by_mnemonic(mnemonic: &str) -> Option<&'static Op> {
    OPS.iter().find(|op| op.mnemonic == mnemonic)
}

/// Bytes one operand character takes.
pub fn operand_size(kind: u8) -> usize {
    match kind {
        b'U' | b'I' | b'L' | b'J' => 2,
        _ => 1,
    }
}

/// Decode the operands of `bytes` (one whole instruction) by its signature.
pub fn operands(op: &Op, bytes: &[u8]) -> Vec<i64> {
    let mut at = 1;
    op.operands
        .bytes()
        .map(|kind| {
            let value = match kind {
                b'U' | b'L' => i64::from(u16::from_le_bytes([bytes[at], bytes[at + 1]])),
                b'I' | b'J' => i64::from(i16::from_le_bytes([bytes[at], bytes[at + 1]])),
                b'b' => i64::from(bytes[at] as i8),
                _ => i64::from(bytes[at]),
            };
            at += operand_size(kind);
            value
        })
        .collect()
}

/// Encode `op` with `values` (one per signature character). Errors name
/// the operand that does not fit its field.
pub fn encode(op: &Op, values: &[i64]) -> Result<Vec<u8>, String> {
    if values.len() != op.operands.len() {
        return Err(format!(
            "`{}` takes {} operands, got {}",
            op.mnemonic,
            op.operands.len(),
            values.len()
        ));
    }
    let mut out = Vec::with_capacity(op.width);
    out.push(op.op);
    for (index, (kind, &value)) in op.operands.bytes().zip(values).enumerate() {
        // A field takes either signedness's spelling of its bits, so `-1`
        // and `0xFFFF` both fit a 16-bit field.
        let bits = operand_size(kind) as u32 * 8;
        if !(-(1i64 << (bits - 1))..1i64 << bits).contains(&value) {
            return Err(format!(
                "operand {} of `{}` ({value}) does not fit its {bits}-bit field",
                index + 1,
                op.mnemonic,
            ));
        }
        if bits == 16 {
            out.extend_from_slice(&(value as u16).to_le_bytes());
        } else {
            out.push(value as u8);
        }
    }
    Ok(out)
}
