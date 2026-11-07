//! `.akscr` room scripts: structured source for a room's SCD streams.
//!
//! A script holds one `proc init` / `proc main` section per command block
//! (in container order), one `event NAME` section per event script, each
//! name standing for the event's number, and any number of user procedures,
//! `proc NAME`, which `NAME();` calls (`gosub`, the port's own). The decompiler renders the original
//! bytecode as code, naming game data from a game's `inc/*.akinc` defines
//! ([`Names`]), and the compiler turns it back into byte-identical streams.
//!
//! ```text
//! #version 1
//!
//! proc init
//! {
//!     if (ck(FG_COMMON, 164, TRUE) && ck(FG_STATUS, 8, FALSE))
//!     {
//!         set(FG_COMMON, 169, TRUE);
//!     }
//!     else
//!     {
//!         require(ck(FG_ROOM, 3, FALSE));
//!         return;
//!     }
//! }
//!
//! event event_00
//! {
//!     run
//!     {
//!         set(FG_RUN, 23, FALSE);
//!     }
//!     evt_exec(0, 6, event_0A);
//!     sleep(30);
//!     do
//!     {
//!         evt_next();
//!     }
//!     while (ck(FG_STATUS, 14, 0));
//!     for (4)
//!     {
//!         evt_next();
//!     }
//!     wait_message();
//!     evt_finish();
//! }
//! ```
//!
//! Command blocks (`proc` sections and `run` blocks):
//!
//! - `name(args);` is one command; arguments are numbers, the disassembler's
//!   constant names (`FG_COMMON`, `RDT_001`, `event_0A`, ...) and `hi | lo`
//!   bit fields.
//! - `if (a && b) { ... } else { ... }` and `else if`: the conditions are the
//!   condition commands that follow the `if` opcode.
//! - `require(cond);` is a condition outside an `if` head: when false it
//!   abandons the enclosing `if` body, or the whole block at the top level.
//! - `return;` is `end 0`. Every block ends in an implicit `end 0` unless the
//!   section or `run` is marked `noreturn`.
//! - `NAME();` runs user procedure `proc NAME` and comes back: `gosub N`, N
//!   counting the procedures in source order, which the container keeps in
//!   a procedure list of its own. A procedure takes no arguments, `noreturn`
//!   or `when`, and may not call itself; its `return;` (at any depth), a
//!   failed condition at its top level and its end return to the caller.
//!
//! Event scripts:
//!
//! - an event-VM mnemonic (`evt_*`, `act_*`, `tw_*`) is that instruction; any
//!   other call is a command wrapped in `evt_single`, and
//!   `single(N) name(args);` gives the wrapper an explicit payload length
//!   (the original data has payloads that cut a command short).
//! - `run { ... }` is an `evt_block` with one command sub-block (plus the
//!   `evt_nop` that is its terminator's second byte).
//! - `for (N) { ... }` is `evt_for 0, N` ... `evt_fornext`.
//! - `do { ... } while (cond);` is `evt_do` with its one inline condition
//!   ... `evt_dountil`.
//! - `sleep(N);` is `evt_sleep` with its `0xF9` tick byte and frame count;
//!   `wait_message();` is `evt_push_cond` + `evt_skip_if`.
//! - an event that ends in `evt_finish` is zero-padded to four bytes unless
//!   its header says `nopad` (`event event_05 nopad`); `raw(0x.., ...);` as
//!   the last statement gives any other trailing bytes.
//!
//! One merged script covers every variant of a room. A room tells the
//! variant as it runs from flags the engine keeps in step with it: the
//! player's status bit `F_STATUS_PLAYER_JILL`, the second-visit scenario
//! flag `F_SCENARIO_MANSION2` and the port's release bits
//! `F_STATUS_REGION_JPN` / `F_STATUS_REGION_EUR`. The checked-in scripts
//! test those with plain `if (ck(...))` statements, in command code and in
//! events, where an `if` is the port's `evt_if`.
//!
//! The exporter writes the tests by variant name instead: `if (JILL)`,
//! `if (MANSION2)`, `if (!JPN)` (names joined by `&&`, with `else`). It
//! marks a section only some RDTs carry with `when (...)`, so a variant's
//! build can leave it out, and `noreturn` / `nopad` may hold for some
//! variants only (`noreturn (MANSION2)`). The runtime build
//! ([`Build::Runtime`]) compiles a variant name to the flag tests; a build
//! for one RDT variant ([`Build::Variant`], [`compile_variant`]) folds it
//! away. A script has no preprocessor choices of its own.
//! The exporter writes the older `#if CHRIS` form ([`merge`]) and
//! [`lift`]s it into tests ([`merge_rooms`], [`merge_regions`]).
//!
//! Anywhere a statement is allowed, `asm { ... }` embeds `.s` assembly lines
//! verbatim (closed by a line holding only `}`); the decompiler uses it for
//! bytecode that has no structured form.

mod compile;
mod decompile;
mod engine;
mod layout;
mod lift;
mod merge;
mod names;
mod preprocess;

pub use compile::{compile, compile_build, compile_variant, compile_with};
pub use decompile::{decompile, decompile_with};
pub use lift::lift;
pub use merge::{merge, merge_regions, merge_rooms};
pub use names::{Names, load_names};
pub use preprocess::{
    Build, Loader, Preprocessed, ROOM_SCRIPT_FILE, Region, SYMBOLS, Variant, includes_from,
    merged_id, no_includes, preprocess, room_script_file, room_variants,
};

#[cfg(test)]
mod tests;
