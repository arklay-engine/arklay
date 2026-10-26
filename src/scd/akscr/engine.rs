//! The compiler's own names for engine values: what an argument means to the
//! engine rather than to a game's data, so every game shares them.
//!
//! Each family names the values one kind of argument takes. A value whose
//! meaning is not known yet still has a placeholder name (`B_0A`, `W_04`,
//! `TASK_09`), so a script never needs a bare number for one and the name can
//! be given its meaning later.

use std::sync::LazyLock;

/// Which values of a family have a placeholder name.
#[derive(Clone, Copy)]
enum Placeholders {
    /// Only the listed values have names.
    None,
    /// Every byte value: `B_0A`.
    Byte(&'static str),
    /// Every bit of a field this many bits wide: `RUN_0010`.
    Bits(&'static str, u32),
}

/// One kind of engine value.
pub(super) struct Family {
    names: &'static [(&'static str, i64)],
    placeholders: Placeholders,
}

impl Family {
    /// The name of `value`.
    pub(super) fn name(&self, value: i64) -> Option<String> {
        if let Some(&(name, _)) = self.names.iter().find(|&&(_, named)| named == value) {
            return Some(name.to_string());
        }
        match self.placeholders {
            Placeholders::None => None,
            Placeholders::Byte(prefix) => (0..=0xFF)
                .contains(&value)
                .then(|| format!("{prefix}{value:02X}")),
            Placeholders::Bits(prefix, width) => {
                (value > 0 && value & (value - 1) == 0 && value < 1 << width).then(|| {
                    format!(
                        "{prefix}{value:0digits$X}",
                        digits = width.div_ceil(4) as usize
                    )
                })
            }
        }
    }

    /// `value` as its bits ORed together, largest first, each by name; bits
    /// without one stay a number. `None` for 0.
    pub(super) fn bits(&self, value: i64) -> Option<String> {
        if value <= 0 {
            return None;
        }
        let mut parts = Vec::new();
        let mut rest = 0;
        for bit in (0..63).rev().map(|shift| 1i64 << shift) {
            if value & bit == 0 {
                continue;
            }
            match self.name(bit) {
                Some(name) => parts.push(name),
                None => rest |= bit,
            }
        }
        if rest != 0 {
            parts.push(rest.to_string());
        }
        Some(parts.join(" | "))
    }

    /// Every name of the family and its value.
    fn all(&self) -> impl Iterator<Item = (String, i64)> + '_ {
        let values: Vec<i64> = match self.placeholders {
            Placeholders::None => Vec::new(),
            Placeholders::Byte(_) => (0..=0xFF).collect(),
            Placeholders::Bits(_, width) => (0..width).map(|shift| 1 << shift).collect(),
        };
        let named = self
            .names
            .iter()
            .map(|&(name, value)| (name.to_string(), value));
        let placeholders = values
            .into_iter()
            .filter_map(|value| self.name(value).map(|name| (name, value)));
        named.chain(placeholders)
    }
}

/// `ck` / `set`: the engine's 0 is true, and `set` can toggle.
pub(super) const TRUTH: Family = Family {
    names: &[("TRUE", 0), ("FALSE", 1), ("TOGGLE", 2)],
    placeholders: Placeholders::None,
};

/// `cmpb` / `cmpw` comparisons, the state on the left.
pub(super) const CMP: Family = Family {
    names: &[
        ("CMP_EQ", 0),
        ("CMP_GT", 1),
        ("CMP_GE", 2),
        ("CMP_LT", 3),
        ("CMP_LE", 4),
        ("CMP_NE", 5),
    ],
    placeholders: Placeholders::None,
};

/// `cmpb` / `setb` state bytes.
pub(super) const BYTE: Family = Family {
    names: &[
        ("B_ROOM", 1),
        ("B_CAMERA", 2),
        ("B_PREV_ROOM", 3),
        ("B_CUT", 4),
        ("B_MENU_CHOICE", 5),
        ("B_SELECTED_ITEM", 6),
        ("B_TOTAL_HELD", 7),
        ("B_LIGHT_R", 8),
        ("B_CHARACTER_MODEL", 9),
        ("B_FWD_AOT", 16),
        ("B_ENT_AOT", 17),
        ("B_USED_ITEM", 18),
        ("B_PICKED_ITEM", 19),
        ("B_SAVES", 40),
        ("B_EQUIPPED", 41),
        ("B_CHARACTER", 43),
        ("B_HEALTH_STATUS", 50),
    ],
    placeholders: Placeholders::Byte("B_"),
};

/// `cmpw` / `setw` state words.
pub(super) const WORD: Family = Family {
    names: &[
        ("W_LIGHT_STATE", 1),
        ("W_LIGHT_DELTA", 2),
        ("W_RANDOM", 3),
        ("W_DPAD_HELD", 6),
        ("W_DPAD_PRESSED", 7),
    ],
    placeholders: Placeholders::Byte("W_"),
};

/// AOT trigger flags: who sets it off (the player, an object), where (at
/// its feet rather than its reach) and how (on the action button).
pub(super) const SAT: Family = Family {
    names: &[
        ("SAT_PLAYER", 0x01),
        ("SAT_OBJECT", 0x04),
        ("SAT_UNDER", 0x40),
        ("SAT_MANUAL", 0x80),
    ],
    placeholders: Placeholders::Bits("SAT_", 8),
};

/// The systems that run each frame (the `FG_RUN` word): `message`'s pause
/// word clears these while the window is open.
pub(super) const RUN: Family = Family {
    names: &[
        ("RUN_PLAYER", 0x01),
        ("RUN_ENTITIES", 0x02),
        ("RUN_MONSTERS", 0x04),
        ("RUN_EFFECTS", 0x08),
        ("RUN_READY", 0x40),
        ("RUN_CONTROLS", 0x100),
        ("RUN_DEMO", 0x200),
    ],
    placeholders: Placeholders::Bits("RUN_", 16),
};

/// `voice_play`.
pub(super) const VOICE: Family = Family {
    names: &[("VOICE_PLAY", 1), ("VOICE_STOP", 2)],
    placeholders: Placeholders::None,
};

/// `se_play_3d`: the sound bank.
pub(super) const SND_BANK: Family = Family {
    names: &[
        ("SND_BANK_DOOR", 0),
        ("SND_BANK_WEAPON", 1),
        ("SND_BANK_ROOM", 2),
        ("SND_BANK_PLAYER", 3),
        ("SND_BANK_BGM", 4),
    ],
    placeholders: Placeholders::None,
};

/// `se_play_3d`: where the sound plays.
pub(super) const SND_AT: Family = Family {
    names: &[
        ("SND_AT_POINT", 0),
        ("SND_AT_PLAYER", 1),
        ("SND_AT_ENEMY", 2),
        ("SND_NO_POS", 3),
    ],
    placeholders: Placeholders::None,
};

/// The music channels of `bgm_play`, `bgm_stop` and the volume commands.
pub(super) const BGM_CHANNEL: Family = Family {
    names: &[("BGM_MAIN", 0), ("BGM_SUB_1", 1), ("BGM_SUB_2", 2)],
    placeholders: Placeholders::None,
};

/// `room_bgm_state_set`: the channels to start and how; the low three bits
/// pick the room's music group and stay a number.
pub(super) const BGM_STATE: Family = Family {
    names: &[
        ("BGM_MAIN_ON", 0x08),
        ("BGM_SUB_1_ON", 0x10),
        ("BGM_SUB_2_ON", 0x20),
        ("BGM_RELOAD", 0x40),
        ("BGM_RESTART", 0x80),
    ],
    placeholders: Placeholders::None,
};

/// `room_bgm_state_set`: no music.
pub(super) const BGM_OFF: Family = Family {
    names: &[("BGM_OFF", 0xFF)],
    placeholders: Placeholders::None,
};

/// `eml_state` sub-commands.
pub(super) const EML: Family = Family {
    names: &[
        ("EML_BEHAVIOUR", 0),
        ("EML_REVIVE", 1),
        ("EML_ACTION", 2),
        ("EML_STATUS", 3),
        ("EML_ANGLE", 5),
        ("EML_BLEND_RESET", 6),
        ("EML_JOINTS", 9),
        ("EML_ACTION_STATE", 10),
    ],
    placeholders: Placeholders::Byte("EML_"),
};

/// `eml_state(.., EML_STATUS, mode | bits)`: how the bits apply.
pub(super) const EML_STATUS: Family = Family {
    names: &[
        ("EML_STATUS_SET", 0x000),
        ("EML_STATUS_OR", 0x100),
        ("EML_STATUS_XOR", 0x200),
    ],
    placeholders: Placeholders::None,
};

/// `sys_multi` sub-commands, all on the player.
pub(super) const SYS: Family = Family {
    names: &[
        ("SYS_UNEQUIP", 0),
        ("SYS_HIT", 1),
        ("SYS_PLAYER_FLAGS", 3),
        ("SYS_FORCE_ACTION", 4),
        ("SYS_ANGLE", 5),
        ("SYS_CLEAR_8C", 6),
        ("SYS_IDLE", 7),
        ("SYS_HEALTH_STATUS", 8),
        ("SYS_JOINTS", 9),
        ("SYS_E0_40", 10),
    ],
    placeholders: Placeholders::Byte("SYS_"),
};

/// `sys_multi(SYS_PLAYER_FLAGS | SYS_HEALTH_STATUS, mode | bits)`: how the
/// bits apply.
pub(super) const SYS_MODE: Family = Family {
    names: &[("SYS_SET", 0x000), ("SYS_OR", 0x100), ("SYS_XOR", 0x200)],
    placeholders: Placeholders::None,
};

/// `sys_multi(SYS_E0_40, ..)`: set or clear the bit.
pub(super) const SYS_SWITCH: Family = Family {
    names: &[("SYS_OFF", 0x000), ("SYS_ON", 0x100)],
    placeholders: Placeholders::None,
};

/// The player flags byte `sys_multi(SYS_PLAYER_FLAGS, ..)` writes.
pub(super) const PLAYER_FLAG: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("PLF_", 8),
};

/// The player's health-status flags; `0x80` locks the controls.
pub(super) const HEALTH_STATUS: Family = Family {
    names: &[("HS_LOCKED", 0x80)],
    placeholders: Placeholders::Bits("HS_", 8),
};

/// `sys_multi(SYS_HIT, ..)`: how the player is being attacked.
pub(super) const HIT: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("HIT_"),
};

/// `sys_multi(SYS_JOINTS, ..)`: the player joints to toggle.
pub(super) const JOINT: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("JOINT_", 16),
};

/// The event slots `evt_exec`, `evt_fork`, `evt_disable` and `task_kill`
/// name.
pub(super) const TASK: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("TASK_"),
};

/// `door_aot_set`'s lock byte: locked until its `FG_LOCK` bit (the low six
/// bits) is raised.
pub(super) const DOOR_LOCK: Family = Family {
    names: &[("DOOR_LOCKED", 0x80), ("DOOR_LOCK_40", 0x40)],
    placeholders: Placeholders::None,
};

/// `obj`'s slot byte: the object (the low six bits) and how it behaves.
pub(super) const OBJ_SLOT: Family = Family {
    names: &[("OBJ_PUSH_GRUNT", 0x40), ("OBJ_TEX_QUEUE", 0x80)],
    placeholders: Placeholders::None,
};

/// A room object's (or item model's) flags byte.
pub(super) const OBJ_FLAG: Family = Family {
    names: &[
        ("OF_ACTIVE", 0x01),
        ("OF_INTANGIBLE", 0x02),
        ("OF_NO_FLOOR_PROBE", 0x04),
        ("OF_NO_COLLISION", 0x08),
        ("OF_NOT_PUSHABLE", 0x20),
        ("OF_CLIMBABLE", 0x40),
    ],
    placeholders: Placeholders::Bits("OF_", 8),
};

/// What an object or item model is attached to: nothing, the player, an
/// enemy (`SCA_ENEMY | slot`) or another object (its index).
pub(super) const SCA: Family = Family {
    names: &[
        ("SCA_ENEMY", 0x80),
        ("SCA_PLAYER", 0xFE),
        ("SCA_NONE", 0xFF),
    ],
    placeholders: Placeholders::None,
};

/// `item_aot_set`: the AOT byte's alternate rotation, and the flags word:
/// the reach animation, the pick-up sparkle and its effect (the `0x0F00`
/// nibble); the `0x00F0` nibble is the sparkle's height and stays a number.
pub(super) const ITEM: Family = Family {
    names: &[
        ("ITEM_ALT_ROT", 0x80),
        ("ITEM_REACH", 0x0001),
        ("ITEM_SPARKLE", 0x8000),
        ("ITEM_FX_1", 0x0100),
        ("ITEM_FX_2", 0x0200),
        ("ITEM_FX_3", 0x0300),
        ("ITEM_FX_4", 0x0400),
        ("ITEM_FX_5", 0x0500),
        ("ITEM_FX_6", 0x0600),
        ("ITEM_FX_7", 0x0700),
        ("ITEM_FX_8", 0x0800),
        ("ITEM_FX_9", 0x0900),
        ("ITEM_FX_A", 0x0A00),
        ("ITEM_FX_B", 0x0B00),
        ("ITEM_FX_C", 0x0C00),
        ("ITEM_FX_D", 0x0D00),
        ("ITEM_FX_E", 0x0E00),
        ("ITEM_FX_F", 0x0F00),
    ],
    placeholders: Placeholders::None,
};

/// `enemy`: no flag keeps the enemy away.
pub(super) const NO_FLAG: Family = Family {
    names: &[("NO_FLAG", 0xFF)],
    placeholders: Placeholders::None,
};

/// `enemy`'s behaviour byte, whose meaning depends on the enemy type.
pub(super) const ENEMY_BEHAVIOUR: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("EB_"),
};

/// `enemy`: restore a saved enemy, or set it up afresh.
pub(super) const SPAWN: Family = Family {
    names: &[("SPAWN_RESTORE", 0), ("SPAWN_FORCE", 1)],
    placeholders: Placeholders::None,
};

/// `enemy`'s model variant.
pub(super) const ENEMY_VARIANT: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("EVAR_"),
};

/// An entity's animation clip.
pub(super) const ANIM: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("ANIM_"),
};

/// An entity's action behaviour; `0x10` in `act_anim_seq` keeps the
/// collision lock.
pub(super) const ACTION: Family = Family {
    names: &[("AB_ADDITIVE", 0x10)],
    placeholders: Placeholders::Byte("AB_"),
};

/// `plw_anim`: the player's action state, less two.
pub(super) const PLW: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("PLW_"),
};

/// `act_anim_flags`: the entity flags it sets, shifted down two bits.
pub(super) const ANIM_FLAG: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("AFL_", 8),
};

/// `act_flag_op`: how the entity flags apply.
pub(super) const ENTITY_FLAG_MODE: Family = Family {
    names: &[("EF_OR", 0), ("EF_SET", 1), ("EF_XOR", 2)],
    placeholders: Placeholders::None,
};

/// An entity's flags word.
pub(super) const ENTITY_FLAG: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("EF_", 16),
};

/// `act_motion`: the look-at and motion mode; `MOTION_TARGET` follows a live
/// target instead of a position.
pub(super) const MOTION: Family = Family {
    names: &[("MOTION_TARGET", 0x93)],
    placeholders: Placeholders::Byte("MOTION_"),
};

/// `act_motion_path(MOTION_TARGET, ..)`: what kind of target.
pub(super) const LOOK_AT: Family = Family {
    names: &[
        ("LOOK_AT_PLAYER", 0),
        ("LOOK_AT_ENEMY", 1),
        ("LOOK_AT_OBJ", 2),
        ("LOOK_AT_ITEM", 3),
    ],
    placeholders: Placeholders::None,
};

/// `ck_counter`: what kind of target the distance is measured to.
pub(super) const DIST: Family = Family {
    names: &[("DIST_ENEMY", 0), ("DIST_OBJ", 1), ("DIST_ITEM", 2)],
    placeholders: Placeholders::None,
};

/// `tw_set_field`: the entity state byte to write.
pub(super) const TWEEN_FIELD: Family = Family {
    names: &[
        ("TW_STATE", 0),
        ("TW_IGNORE", 1),
        ("TW_BEHAVIOUR", 2),
        ("TW_ACTION_STATE", 3),
        ("TW_HEALTH_LO", 4),
        ("TW_HEALTH_HI", 5),
        ("TW_HIT_STATE", 6),
    ],
    placeholders: Placeholders::Byte("TW_FIELD_"),
};

/// `tw_set_sel`: the value to write.
pub(super) const TWEEN_SEL: Family = Family {
    names: &[
        ("TW_POS_X", 0),
        ("TW_POS_Y", 1),
        ("TW_POS_Z", 2),
        ("TW_HEALTH", 3),
        ("TW_TARGET_X", 4),
        ("TW_TARGET_Z", 5),
    ],
    placeholders: Placeholders::Byte("TW_SEL_"),
};

/// The animation row of an effect, which `effect_kill_b` matches.
pub(super) const FX_ROW: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("FX_ROW_"),
};

/// What an effect is attached to: the world, the player, an enemy
/// (`FX_AT_ENEMY_<slot>`, defined per room) or an object (`FX_AT_OBJ | n`).
pub(super) const FX_AT: Family = Family {
    names: &[("FX_AT_WORLD", 0), ("FX_AT_PLAYER", 1), ("FX_AT_OBJ", 0x80)],
    placeholders: Placeholders::None,
};

/// `timer_setup`: how the special room light draws.
pub(super) const LIGHT_FADE: Family = Family {
    names: &[("LIGHT_FADE_VEIL", 2), ("LIGHT_FADE_HALF", 3)],
    placeholders: Placeholders::Byte("LIGHT_FADE_"),
};

/// `timer_setup`: the channels the special room light saturates.
pub(super) const FADE_CHANNEL: Family = Family {
    names: &[("FADE_B", 1), ("FADE_G", 2), ("FADE_R", 4)],
    placeholders: Placeholders::Bits("FADE_", 16),
};

/// One of the room's three lights.
pub(super) const LIGHT: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("LIGHT_"),
};

/// `msg_list`: a light's kind.
pub(super) const LIGHT_KIND: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("LIGHT_KIND_"),
};

/// `inst_cfg`: one of the room's collision lists.
pub(super) const COL_LIST: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("COL_LIST_"),
};

/// `inst_cfg`: the collision flags it sets.
pub(super) const COL_FLAG: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("CF_", 4),
};

/// `objtbl_b_set`: the table to write.
pub(super) const TABLE: Family = Family {
    names: &[("TBL_OBJ", 0), ("TBL_ITEM", 1)],
    placeholders: Placeholders::None,
};

/// `eml_rot`: a room object rather than an item model.
pub(super) const ROT: Family = Family {
    names: &[("ROT_OBJ", 0x80)],
    placeholders: Placeholders::None,
};

/// `aot_switch`: show or hide the room sprite.
pub(super) const SPRITE: Family = Family {
    names: &[("SPR_ON", 0), ("SPR_OFF", 1)],
    placeholders: Placeholders::None,
};

/// `ck_item_count`: the item group counted.
pub(super) const ITEM_GROUP: Family = Family {
    names: &[("IGRP_ANY", 0x0A)],
    placeholders: Placeholders::Byte("IGRP_"),
};

/// `ck_bits`: the d-pad buttons tested.
pub(super) const PAD: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("PAD_", 16),
};

/// `swap_var`: which way a pick-up quantity moves.
pub(super) const XFER: Family = Family {
    names: &[("XFER_SAVE", 0), ("XFER_RESTORE", 1), ("XFER_HELD", 2)],
    placeholders: Placeholders::None,
};

/// `mass_mask`: how the effect flags apply.
pub(super) const MASK: Family = Family {
    names: &[("MASK_OR", 0), ("MASK_CLEAR", 1), ("MASK_XOR", 2)],
    placeholders: Placeholders::None,
};

/// The flags word of every live effect `mass_mask` changes.
pub(super) const FX_FLAG: Family = Family {
    names: &[],
    placeholders: Placeholders::Bits("FXF_", 16),
};

/// `spd_set`: the entity; the enemies follow the player.
pub(super) const ENTITY: Family = Family {
    names: &[("ENT_PLAYER", 0)],
    placeholders: Placeholders::Byte("ENT_"),
};

/// `cut_auto`: whether the camera follows the player's camera zones.
pub(super) const CUT: Family = Family {
    names: &[("CUT_FREE", 0), ("CUT_LOCKED", 1)],
    placeholders: Placeholders::None,
};

/// `model_op`'s form.
pub(super) const MODEL_OP: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("MODEL_OP_"),
};

/// The player's costume.
pub(super) const COSTUME: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("COSTUME_"),
};

const FAMILIES: [&Family; 63] = [
    &TRUTH,
    &CMP,
    &BYTE,
    &WORD,
    &SAT,
    &RUN,
    &VOICE,
    &SND_BANK,
    &SND_AT,
    &BGM_CHANNEL,
    &BGM_STATE,
    &BGM_OFF,
    &EML,
    &EML_STATUS,
    &SYS,
    &TASK,
    &DOOR_LOCK,
    &SYS_MODE,
    &SYS_SWITCH,
    &PLAYER_FLAG,
    &HEALTH_STATUS,
    &HIT,
    &JOINT,
    &OBJ_SLOT,
    &OBJ_FLAG,
    &SCA,
    &ITEM,
    &NO_FLAG,
    &ENEMY_BEHAVIOUR,
    &SPAWN,
    &ENEMY_VARIANT,
    &ANIM,
    &ACTION,
    &PLW,
    &ANIM_FLAG,
    &ENTITY_FLAG_MODE,
    &ENTITY_FLAG,
    &MOTION,
    &LOOK_AT,
    &DIST,
    &TWEEN_FIELD,
    &TWEEN_SEL,
    &FX_ROW,
    &FX_AT,
    &LIGHT_FADE,
    &FADE_CHANNEL,
    &LIGHT,
    &LIGHT_KIND,
    &COL_LIST,
    &COL_FLAG,
    &TABLE,
    &ROT,
    &SPRITE,
    &ITEM_GROUP,
    &PAD,
    &XFER,
    &MASK,
    &FX_FLAG,
    &ENTITY,
    &MODEL_OP,
    &COSTUME,
    &CUT,
    &Family {
        // `message`: reveal the text at once.
        names: &[("MSG_FAST", 0x80)],
        placeholders: Placeholders::None,
    },
];

/// Every engine name and its value.
pub(super) fn engine_names() -> &'static [(String, i64)] {
    static NAMES: LazyLock<Vec<(String, i64)>> =
        LazyLock::new(|| FAMILIES.iter().flat_map(|family| family.all()).collect());
    &NAMES
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// No two families give one name different values.
    #[test]
    fn engine_names_are_unique() {
        let mut seen: HashMap<&str, i64> = HashMap::new();
        for (name, value) in engine_names() {
            if let Some(&other) = seen.get(name.as_str()) {
                assert_eq!(other, *value, "`{name}` names two values");
            }
            seen.insert(name, *value);
        }
    }
}
