//! `.akscr` room scripts: structured source for a room's SCD streams.
//!
//! A script holds one `proc init` / `proc main` section per command block
//! (in container order) and one `event NAME` section per event script, each
//! name standing for the event's number. The decompiler renders the original
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
//!         set(FG_6, 23, FALSE);
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
//! One merged script covers every variant of a room: `#if CHRIS`, `#if JILL`,
//! `#if MANSION1` and `#if MANSION2` (with `#else` / `#endif`, nestable) keep
//! or drop whole lines per variant before the script is parsed; see
//! [`compile_variant`] and [`merge`].
//!
//! Anywhere a statement is allowed, `asm { ... }` embeds `.s` assembly lines
//! verbatim (closed by a line holding only `}`); the decompiler uses it for
//! bytecode that has no structured form.

mod compile;
mod decompile;
mod engine;
mod merge;
mod names;
mod preprocess;

pub use compile::{compile, compile_variant, compile_with};
pub use decompile::{decompile, decompile_with};
pub use merge::{merge, merge_rooms};
pub use names::{Names, load_names};
pub use preprocess::{
    Loader, Preprocessed, SYMBOLS, Variant, includes_from, merged_id, no_includes, preprocess,
    room_variants,
};

#[cfg(test)]
mod tests;
