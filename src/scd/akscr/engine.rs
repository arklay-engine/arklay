//! The compiler's own names for engine values: what an argument means to the
//! engine rather than to a game's data, so every game shares them.

/// Every engine name and its value.
pub(super) const ENGINE_NAMES: &[(&str, i64)] = &[
    // `ck` / `set`: the engine's 0 is true, and `set` can toggle.
    ("TRUE", 0),
    ("FALSE", 1),
    ("TOGGLE", 2),
    // `cmpb` / `cmpw` comparisons, the state on the left.
    ("CMP_EQ", 0),
    ("CMP_GT", 1),
    ("CMP_GE", 2),
    ("CMP_LT", 3),
    ("CMP_LE", 4),
    ("CMP_NE", 5),
    // `cmpb` / `setb` state bytes.
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
    // `cmpw` / `setw` state words.
    ("W_LIGHT_STATE", 1),
    ("W_LIGHT_DELTA", 2),
    ("W_RANDOM", 3),
    ("W_DPAD_HELD", 6),
    ("W_DPAD_PRESSED", 7),
    // AOT trigger flags: who sets it off, and how.
    ("SAT_PLAYER", 0x01),
    ("SAT_OBJECT", 0x04),
    ("SAT_UNDER", 0x40),
    ("SAT_MANUAL", 0x80),
    // `message`: reveal the text at once.
    ("MSG_FAST", 0x80),
    // `voice_play`.
    ("VOICE_PLAY", 1),
    ("VOICE_STOP", 2),
    // `se_play_3d`: where the sound plays.
    ("SND_AT_POINT", 0),
    ("SND_AT_PLAYER", 1),
    ("SND_AT_ENEMY", 2),
    ("SND_NO_POS", 3),
    // `room_bgm_state_set`: channel enables, the load type, and no music;
    // the low three bits pick the room's music group.
    ("BGM_CH0", 0x08),
    ("BGM_CH1", 0x10),
    ("BGM_CH2", 0x20),
    ("BGM_RELOAD", 0x40),
    ("BGM_RESTART", 0x80),
    ("BGM_OFF", 0xFF),
    // `eml_state` sub-commands.
    ("EML_BEHAVIOUR", 0),
    ("EML_REVIVE", 1),
    ("EML_ACTION", 2),
    ("EML_STATUS", 3),
    ("EML_ANGLE", 5),
    ("EML_BLEND_RESET", 6),
    ("EML_JOINTS", 9),
    ("EML_ACTION_STATE", 10),
    // `eml_state(.., EML_STATUS, mode | bits)`: how the bits apply.
    ("EML_STATUS_SET", 0x000),
    ("EML_STATUS_OR", 0x100),
    ("EML_STATUS_XOR", 0x200),
];

/// The first of `names` whose value is `value`.
pub(super) fn name_in(names: &[&'static str], value: i64) -> Option<&'static str> {
    names.iter().copied().find(|name| {
        ENGINE_NAMES
            .iter()
            .any(|&(named, v)| named == *name && v == value)
    })
}

/// The engine names starting with `prefix`.
pub(super) fn prefixed(prefix: &str) -> Vec<&'static str> {
    ENGINE_NAMES
        .iter()
        .map(|&(name, _)| name)
        .filter(|name| name.starts_with(prefix))
        .collect()
}

/// `value` as `A | B | rest` over the named bits starting with `prefix`,
/// largest first; `None` when no named bit is set.
pub(super) fn bits_of(prefix: &str, value: i64) -> Option<String> {
    let mut rest = value;
    let mut parts = Vec::new();
    let mut bits: Vec<(&str, i64)> = ENGINE_NAMES
        .iter()
        .filter(|(name, bit)| name.starts_with(prefix) && *bit != 0 && bit & (bit - 1) == 0)
        .copied()
        .collect();
    bits.sort_by_key(|&(_, bit)| std::cmp::Reverse(bit));
    for (name, bit) in bits {
        if rest & bit != 0 {
            parts.push(name.to_string());
            rest &= !bit;
        }
    }
    if parts.is_empty() {
        return None;
    }
    if rest != 0 {
        parts.push(rest.to_string());
    }
    Some(parts.join(" | "))
}
