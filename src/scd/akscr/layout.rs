//! The operand layouts `.akscr` writes commands with: one argument per field
//! the engine reads, at the width it reads it.
//!
//! The `.s` assembler keeps the original tool's display signatures, which
//! split some words into bytes and print other commands as raw bytes. A
//! command listed here is written in `.akscr` by its real fields instead: the
//! decompiler unpacks the instruction's bytes into them and the compiler packs
//! them back, then hands the assembler the bytes in its own shape.

use anyhow::{Result, bail};

use crate::scd::asm::Arg;
use crate::scd::disasm::{ACTOR_SIG, COMMAND_SIG, TWEEN_SIG};

/// The fields of the commands and event instructions written by field, after
/// the opcode: `u` a byte, `c` a signed byte, `U` a word, `I` a signed word.
const LAYOUTS: &[(&str, &str)] = &[
    // aot | alt rotation, zone x/z/w/d, item, quantity, item model, parent,
    // x/y/z, yaw, item flag, trigger flags, item flags.
    ("item_aot_set", "uIIIIuuuuIIIIuuU"),
    // type, behaviour, guard flag, init, hit records, pitch/yaw/roll,
    // x/y/z, slot, animation, frame, variant.
    ("enemy", "uuuuuIIIUIUuuuu"),
    // mode, delta, channel mask.
    ("timer_setup", "uIU"),
    ("ck_last_item", "u"),
    // play/stop, line.
    ("voice_play", "uU"),
    // slot | flags, object flags, parent, x/y/z, yaw, probe x0/z0/x1/z1,
    // radius, height, half width x/z.
    ("obj", "uuuIIIIIIIIIIII"),
    // item group, comparison, count.
    ("ck_item_count", "uuu"),
    // pad, sprite, on/off.
    ("aot_switch", "uuu"),
    // type, animation row, parent, x/y/z, yaw.
    ("effect", "uuuIIII"),
    // action state, animation, frame.
    ("plw_anim", "uuu"),
    // channel, pan, volume.
    ("snd_pan_vol_set", "ucc"),
    // collision list, record, flags, box.
    ("inst_cfg", "uuuUUUU"),
    // word, value.
    ("setw", "uI"),
    ("model_op", "uuuuuuu"),
    // table, index, flags.
    ("objtbl_b_set", "uuu"),
    // object, comparison, push count.
    ("ck_anim", "uuu"),
    // invert, d-pad mask.
    ("ck_bits", "uU"),
    // camera zone, cameras.
    ("msgnode_set", "uuu"),
    // model, rotation x/z.
    ("eml_rot", "uII"),
    // pad, target type, target, distance.
    ("ck_counter", "uuuU"),
    ("effect_tracked", "uuuUUUI"),
    // pad, angle range.
    ("ck_tween", "uUU"),
    // light, position, colour, kind.
    ("obj_xfm", "uIIIIIII"),
    // entity, floor height.
    ("spd_set", "uI"),
    // type, animation row.
    ("effect_kill_b", "uU"),
    // channel, volume, frames.
    ("bgm_volume_ramp", "ucu"),
    ("task_kill", "u"),
    ("spd_add", "c"),
    // pad, three lights (x/y/z, r/g/b, kind, radius), ambient.
    ("msg_list", "uIIIuuuuIIIIuuuuIIIIuuuuIIII"),
    // object, rotation x/y/z, position x/y/z.
    ("eml_pos", "uIIIIII"),
    // mode, AOT, state byte.
    ("swap_var", "uuu"),
    // mode, effect flags.
    ("mass_mask", "uU"),
    ("costume_set", "u"),
    ("costume_ck", "u"),
    // Actor instructions.
    ("act_motion", "u"),
    // motion, x/y/z (or target type, target), yaw step, pitch step.
    ("act_motion_path", "uIIIuu"),
    // behaviour, walk target x/z, done flag.
    ("act_anim_seq", "uIIu"),
    // animation, done flag, entity flags >> 2.
    ("act_anim_flags", "uuu"),
    // behaviour, animation, done flag.
    ("act_anim_set", "uuu"),
    // mode, entity flags.
    ("act_flag_op", "uU"),
    // pad, turn step.
    ("act_param_set", "uU"),
    // Tween instructions.
    ("tw_set_pos", "ccc"),
    ("tw_set_rot", "ccc"),
    ("tw_abs_pos", "uIII"),
    ("tw_set_field", "uu"),
    ("tw_set_sel", "uI"),
    ("tw_abs_rot", "uIII"),
];

/// The field layout of `mnemonic`, if `.akscr` writes it by field.
pub(super) fn layout(mnemonic: &str) -> Option<&'static str> {
    LAYOUTS
        .iter()
        .find(|&&(name, _)| name == mnemonic)
        .map(|&(_, fields)| fields)
}

fn field_size(kind: u8) -> usize {
    match kind {
        b'U' | b'I' => 2,
        _ => 1,
    }
}

/// The byte width of a layout's fields.
fn layout_size(fields: &str) -> usize {
    fields.bytes().map(field_size).sum()
}

/// The field values of an instruction's operand bytes (`bytes` without the
/// opcode). Bytes a clamped instruction does not store read as zero.
pub(super) fn unpack(fields: &str, bytes: &[u8]) -> Vec<i64> {
    let byte = |at: usize| bytes.get(at).copied().unwrap_or(0);
    let mut at = 0;
    fields
        .bytes()
        .map(|kind| {
            let value = match kind {
                b'u' => i64::from(byte(at)),
                b'c' => i64::from(byte(at) as i8),
                b'U' => i64::from(u16::from_le_bytes([byte(at), byte(at + 1)])),
                _ => i64::from(i16::from_le_bytes([byte(at), byte(at + 1)])),
            };
            at += field_size(kind);
            value
        })
        .collect()
}

/// The operand bytes of `values`, one per field. A byte field takes -128 to
/// 255 and a word field -32768 to 65535, either signedness.
pub(super) fn pack(fields: &str, values: &[i64]) -> Result<Vec<u8>> {
    if values.len() != fields.len() {
        bail!("takes {} arguments, found {}", fields.len(), values.len());
    }
    let mut bytes = Vec::with_capacity(layout_size(fields));
    for (index, (kind, &value)) in fields.bytes().zip(values).enumerate() {
        if field_size(kind) == 1 {
            if !(-0x80..=0xFF).contains(&value) {
                bail!("argument {} ({value}) does not fit a byte", index + 1);
            }
            bytes.push(value as u8);
        } else {
            if !(-0x8000..=0xFFFF).contains(&value) {
                bail!("argument {} ({value}) does not fit a word", index + 1);
            }
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
        }
    }
    Ok(bytes)
}

/// The `.s` display signature of `mnemonic`.
fn display_signature(mnemonic: &str) -> Option<&'static str> {
    if mnemonic == "act_motion_path" {
        return Some("act_motion_path:uUUUuu");
    }
    COMMAND_SIG
        .iter()
        .chain(&ACTOR_SIG)
        .chain(&TWEEN_SIG)
        .copied()
        .find(|signature| signature.split(':').next() == Some(mnemonic))
}

/// Operand bytes as the `.s` assembler takes them for `mnemonic`: by its
/// display signature, or one per byte for a command without one.
pub(super) fn asm_args(mnemonic: &str, bytes: &[u8]) -> Vec<Arg> {
    let params = display_signature(mnemonic)
        .and_then(|signature| signature.split_once(':'))
        .map_or("", |(_, params)| params);
    if params.is_empty() {
        return bytes
            .iter()
            .map(|&byte| Arg::Num(i64::from(byte)))
            .collect();
    }
    let byte = |at: usize| bytes.get(at).copied().unwrap_or(0);
    let mut at = 0;
    let mut args = Vec::new();
    for kind in params.bytes() {
        if at >= bytes.len() {
            break;
        }
        let value = match kind {
            b'U' => i64::from(u16::from_le_bytes([byte(at), byte(at + 1)])),
            b'I' => i64::from(i16::from_le_bytes([byte(at), byte(at + 1)])),
            _ => i64::from(byte(at)),
        };
        at += field_size(kind);
        args.push(Arg::Num(value));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scd::opcode::{ACTOR_OPS, COMMAND_OPS, TWEEN_OPS};

    /// Every layout covers exactly the operand bytes of its instruction.
    #[test]
    fn layouts_match_the_instruction_widths() {
        for &(name, fields) in LAYOUTS {
            let command = COMMAND_SIG
                .iter()
                .position(|signature| signature.split(':').next() == Some(name))
                .map(|index| &COMMAND_OPS[index]);
            let width = match name {
                "act_motion" => Some(2),
                "act_motion_path" => Some(10),
                _ => command
                    .or_else(|| ACTOR_OPS.iter().find(|op| op.mnemonic == name))
                    .or_else(|| TWEEN_OPS.iter().find(|op| op.mnemonic == name))
                    .and_then(|op| op.width),
            };
            assert_eq!(Some(layout_size(fields) + 1), width, "`{name}`");
        }
    }

    #[test]
    fn packing_round_trips_and_reshapes_for_the_assembler() {
        let fields = layout("voice_play").unwrap();
        let bytes = pack(fields, &[1, 0x0123]).unwrap();
        assert_eq!(bytes, [1, 0x23, 0x01]);
        assert_eq!(unpack(fields, &bytes), [1, 0x0123]);
        // `voice_play` has no display signature: one argument per byte.
        assert_eq!(
            asm_args("voice_play", &bytes),
            [Arg::Num(1), Arg::Num(0x23), Arg::Num(1)]
        );
        assert!(pack(fields, &[1]).is_err());
        assert!(pack(fields, &[256, 0]).is_err());
        assert_eq!(pack(layout("spd_add").unwrap(), &[-25]).unwrap(), [231]);
    }
}
