//! Split out of the parent module so that edits there do not move these
//! definitions: types inside `GameState` are inlined into many codegen
//! units, and moving one recompiles all of them (#266).

/// The original's text tint table: the palette row (`clutY - 0x1E0`) picks a
/// colour multiplier over the sheet's grey ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    White,
    Green,
    Red,
    Grey,
    Yellow,
    /// An explicit per-channel multiplier, for a caller that has a colour of
    /// its own (the Lua screen API's `#RRGGBB`).
    Rgb(u8, u8, u8),
}
