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

/// `sys_multi` sub-commands.
pub(super) const SYS: Family = Family {
    names: &[],
    placeholders: Placeholders::Byte("SYS_"),
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

const FAMILIES: [&Family; 18] = [
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
