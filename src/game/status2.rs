//! RE 2's status screen: its item page, the routine at `0x4F8160` (page 3
//! of the screen's work block at `0x691F60`), the tab bar above it
//! (`0x4F8050`, page 2), the condition panel the page animates (`0x4FFAB0`),
//! and the MAP and FILE pages' steps (`0x4FA240`, page 4, and `0x4F95B0`,
//! page 5; the pages' table is at `0x53D9EC`).
//!
//! The work block's bytes are kept under the executable's own numbers, so
//! the oracle's `gi` field compares one for one:
//!
//! | byte | field |
//! |---|---|
//! | `+1` | [`ItemPage::page`] |
//! | `+2` | [`ItemPage::sub`]: 0 the grid, 1 the command menu, 2 a command, 3 and 4 the menu opening and closing |
//! | `+3` | [`ItemPage::command`], or the count of subs 3 and 4 |
//! | `+4`, `+5` | [`ItemPage::step`], [`ItemPage::count`] |
//! | `+6` | [`ItemPage::follow`] |
//! | `+0xC`, `+0xD` | [`ItemPage::cursor`], [`ItemPage::target`] |
//! | `+0xE`, `+0xF` | [`ItemPage::tab`], [`ItemPage::command_cursor`] |
//! | `+0x26` | [`ItemPage::result`] |
//! | `+0x62` | [`Panel::mode`] |
//!
//! The menu holds three commands, top to bottom 3 (EQUIP for a weapon, USE
//! for the rest), 2 (CHECK) and 1 (COMBN); there is no MOVE. Slot 10 is the
//! personal item, which the cursor reaches by going up from the top row.
//! While the page is up the inventory keeps the original's holes: a used or
//! combined item leaves its slot empty, and COMBN's step 2 shifts one slot
//! down every four ticks.

use super::GameState;
use crate::scd2::host::item_max;

pub mod binders;
pub mod map;

/// `Item_work`'s eleventh slot: the personal item.
pub const PERSONAL_SLOT: u8 = 10;

/// The page byte (`0x691F61`): the tab bar and the item page.
pub const PAGE_TABS: u8 = 2;
pub const PAGE_ITEMS: u8 = 3;
pub const PAGE_MAP: u8 = 4;
pub const PAGE_FILE: u8 = 5;

/// The MAP page's subs (`0x4FA64C`): 0 loads the floor's map, 1 slides it in,
/// 2 slides it out, 3 shows it and 4 loads another floor.
pub const MAP_LOAD: u8 = 0;
pub const MAP_IN: u8 = 1;
pub const MAP_OUT: u8 = 2;
pub const MAP_SHOWN: u8 = 3;
pub const MAP_FLOOR: u8 = 4;
/// The FILE page's subs (`0x4FA178`) as far as the binders go: 0 loads them
/// and starts 1, the slide in; 2 slides out, 3 waits on the binders, and 4
/// and 5 turn them left and right. Subs 6 to 18 open a binder and read its
/// files, which no route state reaches (`docs/internals/deviations/re2.md`).
pub const FILE_LOAD: u8 = 0;
pub const FILE_IN: u8 = 1;
pub const FILE_OUT: u8 = 2;
pub const FILE_SHOWN: u8 = 3;
pub const FILE_LEFT: u8 = 4;
pub const FILE_RIGHT: u8 = 5;
/// The binders on FILE's carousel (`0x691F71` wraps 0 to 2).
pub const BINDERS: u8 = 3;

/// The item page's subs (`0x691F62`).
pub const SUB_GRID: u8 = 0;
pub const SUB_MENU: u8 = 1;
pub const SUB_COMMAND: u8 = 2;
pub const SUB_OPENING: u8 = 3;
pub const SUB_CLOSING: u8 = 4;

/// The commands (`0x4F8610`), as the menu's cursor counts them.
pub const COMMAND_COMBINE: u8 = 1;
pub const COMMAND_CHECK: u8 = 2;
pub const COMMAND_USE: u8 = 3;

/// The tab bar's EXIT.
const TAB_EXIT: u8 = 3;
/// The ITEMS tab: the page is `5 - tab`, so FILE (tab 0) is page 5 and MAP
/// (tab 1) page 4.
const TAB_ITEMS: u8 = 2;

/// The executable's system messages the page raises, by the original's ids
/// (`MESSAGE_<nn>` in the pack's language file).
/// `0x4F8C5A`'s message 2, "It's already fully loaded."
pub const MESSAGE_LOADED: u8 = 2;
/// USE's message 5 for ammunition, "These bullets can't be used by
/// themselves."
pub const MESSAGE_BULLETS: u8 = 5;
/// USE's message 7, "You don't need to use this right now."
pub const MESSAGE_NOT_NOW: u8 = 7;
/// COMBN step 11's message 0xB, "Will you mix the herb?" with yes and no.
pub const MESSAGE_MIX: u8 = 0xB;

/// The first id USE doesn't equip (`0x4F8620`): ammunition from here.
const FIRST_AMMO: u8 = 0x14;
const LAST_AMMO: u8 = 0x1D;

/// One tick of the page's input: the pad's edges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
    pub start: bool,
}

/// The sounds the page plays: `Snd_se_on`'s core-bank cues `0x4040000`,
/// `0x4050000`, `0x4060000` and `0x4070000` (`0x4ED950`), which play at the
/// slot's own volume, unpanned; MAP's `0x4090000` once the map has slid in,
/// and FILE's `0x40A0000` for a turn of the binders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Cursor,
    Cancel,
    Decide,
    Buzzer,
    Map,
    Turn,
}

impl Sound {
    /// The `Snd_se_on` id.
    pub fn id(self) -> u32 {
        let slot = match self {
            Self::Cursor => 4,
            Self::Cancel => 5,
            Self::Decide => 6,
            Self::Buzzer => 7,
            Self::Map => 9,
            Self::Turn => 10,
        };
        0x0400_0000 | slot << 16
    }
}

/// What a tick asks of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub sounds: Vec<Sound>,
    /// Page 0: the screen closes.
    pub close: bool,
    /// CHECK's step 6: the item's description opens (message `0x10 + id`).
    pub describe: Option<u8>,
}

/// The condition panel (`0x4FFAB0`, `0x691FC2` on): a heal sweeps it, the
/// blue herb's cure drains it, and USE's step 4 waits for it to finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panel {
    /// 0 idle, 1 a heal, 2 a cure, 3 both.
    pub mode: u8,
    pub done: bool,
    pub sweep: i16,
    pub cure: i16,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            mode: 0,
            done: false,
            sweep: PANEL_SWEEP_START,
            cure: PANEL_CURE_START,
        }
    }
}

const PANEL_SWEEP_START: i16 = 10;
const PANEL_SWEEP_END: i16 = 0x80;
const PANEL_CURE_START: i16 = 0x46;
const PANEL_CURE_END: i16 = -20;

impl Panel {
    /// One tick, after the page's.
    fn tick(&mut self) {
        match self.mode {
            1 | 3 => {
                if self.done {
                    *self = Self::default();
                } else {
                    self.sweep += 3;
                    self.done = self.sweep > PANEL_SWEEP_END;
                }
            }
            2 => {
                if self.done {
                    *self = Self::default();
                } else {
                    self.cure -= 3;
                    self.done = self.cure < PANEL_CURE_END;
                }
            }
            _ => {}
        }
    }
}

/// The status screen's work block, for its tab bar and item page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemPage {
    pub page: u8,
    pub sub: u8,
    pub command: u8,
    pub step: u8,
    pub count: u8,
    pub follow: bool,
    pub cursor: u8,
    pub target: u8,
    pub tab: u8,
    pub command_cursor: u8,
    /// COMBN's result id (`0x691F86`).
    pub result: u8,
    /// FILE's binder (`0x691F71`), 0 to 2.
    pub binder: u8,
    /// USE's heal, applied once the panel is done.
    pub amount: i16,
    pub panel: Panel,
}

impl Default for ItemPage {
    fn default() -> Self {
        Self {
            page: PAGE_ITEMS,
            sub: SUB_GRID,
            command: 0,
            step: 0,
            count: 0,
            follow: false,
            cursor: 0,
            target: 0,
            tab: TAB_ITEMS,
            // `0x4FF060` puts it on the top entry when the menu opens.
            command_cursor: 0,
            result: 0,
            binder: 0,
            amount: 0,
            panel: Panel::default(),
        }
    }
}

impl ItemPage {
    /// [`ItemPage::trace`] of the work block before the screen first opens.
    pub const CLEARED_TRACE: &str = "0,0,0,0,0,0,0,0";

    /// The work block's bytes the oracle's `gi` ends with: `+2` to `+5`, the
    /// two cursors, the command cursor and the panel's mode.
    pub fn trace(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.sub,
            self.command,
            self.step,
            self.count,
            self.cursor,
            self.target,
            self.command_cursor,
            self.panel.mode
        )
    }

    /// Hand a command over to its next step, with the step's count cleared.
    fn goto(&mut self, step: u8) {
        self.step = step;
        self.count = 0;
    }

    /// Close the command menu: sub 4.
    fn close_menu(&mut self) {
        self.sub = SUB_CLOSING;
        self.command = 0;
    }

    /// Back to the grid, everything cleared (COMBN's step 3, CHECK's
    /// step 13).
    fn back_to_grid(&mut self) {
        self.sub = SUB_GRID;
        self.command = 0;
        self.step = 0;
        self.count = 0;
    }
}

/// A combine record (`0x53DE28`'s per-item list, four bytes each): the
/// target id, the step it starts and the result id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recipe {
    pub item: u8,
    pub target: u8,
    pub step: u8,
    pub result: u8,
}

const fn r(item: u8, target: u8, step: u8, result: u8) -> Recipe {
    Recipe {
        item,
        target,
        step,
        result,
    }
}

/// COMBN's steps the records name.
const STEP_COMBINE: u8 = 4;
const STEP_LOAD_CURSOR: u8 = 5;
const STEP_LOAD_TARGET: u8 = 6;
const STEP_SWAP_CURSOR: u8 = 7;
const STEP_SWAP_TARGET: u8 = 8;
const STEP_MIX: u8 = 11;

/// Every combine record of the item table (`0x53DE28`: per id the maximum,
/// two bytes, the record count and a pointer to the records), in the
/// table's order. A pair with no record buzzes.
pub const RECIPES: &[Recipe] = &[
    // The weapons: their ammunition loads them, a part upgrades them.
    r(0x02, 0x14, STEP_LOAD_CURSOR, 0),
    r(0x02, 0x20, STEP_COMBINE, 0x04),
    r(0x03, 0x14, STEP_LOAD_CURSOR, 0),
    r(0x04, 0x14, STEP_LOAD_CURSOR, 0),
    r(0x05, 0x16, STEP_LOAD_CURSOR, 0),
    r(0x05, 0x21, STEP_COMBINE, 0x06),
    r(0x06, 0x16, STEP_LOAD_CURSOR, 0),
    r(0x07, 0x15, STEP_LOAD_CURSOR, 0),
    r(0x07, 0x22, STEP_COMBINE, 0x08),
    r(0x08, 0x15, STEP_LOAD_CURSOR, 0),
    r(0x09, 0x18, STEP_LOAD_CURSOR, 0),
    r(0x09, 0x19, STEP_SWAP_CURSOR, 0),
    r(0x09, 0x1A, STEP_SWAP_CURSOR, 0),
    r(0x0A, 0x19, STEP_LOAD_CURSOR, 0),
    r(0x0A, 0x18, STEP_SWAP_CURSOR, 0),
    r(0x0A, 0x1A, STEP_SWAP_CURSOR, 0),
    r(0x0B, 0x1A, STEP_LOAD_CURSOR, 0),
    r(0x0B, 0x18, STEP_SWAP_CURSOR, 0),
    r(0x0B, 0x19, STEP_SWAP_CURSOR, 0),
    r(0x0C, 0x1D, STEP_LOAD_CURSOR, 0),
    r(0x0D, 0x14, STEP_LOAD_CURSOR, 0),
    r(0x0E, 0x1C, STEP_LOAD_CURSOR, 0),
    r(0x0F, 0x1B, STEP_LOAD_CURSOR, 0),
    r(0x10, 0x17, STEP_LOAD_CURSOR, 0),
    r(0x13, 0x14, STEP_LOAD_CURSOR, 0),
    // The ammunition: into its weapons, onto its own kind, and the grenade
    // rounds swap with a launcher loaded with another kind.
    r(0x14, 0x02, STEP_LOAD_TARGET, 0),
    r(0x14, 0x03, STEP_LOAD_TARGET, 0),
    r(0x14, 0x04, STEP_LOAD_TARGET, 0),
    r(0x14, 0x0D, STEP_LOAD_TARGET, 0),
    r(0x14, 0x13, STEP_LOAD_TARGET, 0),
    r(0x14, 0x14, STEP_LOAD_CURSOR, 0),
    r(0x15, 0x07, STEP_LOAD_TARGET, 0),
    r(0x15, 0x08, STEP_LOAD_TARGET, 0),
    r(0x15, 0x15, STEP_LOAD_CURSOR, 0),
    r(0x16, 0x05, STEP_LOAD_TARGET, 0),
    r(0x16, 0x06, STEP_LOAD_TARGET, 0),
    r(0x16, 0x16, STEP_LOAD_CURSOR, 0),
    r(0x17, 0x10, STEP_LOAD_TARGET, 0),
    r(0x17, 0x17, STEP_LOAD_CURSOR, 0),
    r(0x18, 0x09, STEP_LOAD_TARGET, 0),
    r(0x18, 0x0A, STEP_SWAP_TARGET, 0),
    r(0x18, 0x0B, STEP_SWAP_TARGET, 0),
    r(0x18, 0x18, STEP_LOAD_CURSOR, 0),
    r(0x19, 0x0A, STEP_LOAD_TARGET, 0),
    r(0x19, 0x09, STEP_SWAP_TARGET, 0),
    r(0x19, 0x0B, STEP_SWAP_TARGET, 0),
    r(0x19, 0x19, STEP_LOAD_CURSOR, 0),
    r(0x1A, 0x0B, STEP_LOAD_TARGET, 0),
    r(0x1A, 0x09, STEP_SWAP_TARGET, 0),
    r(0x1A, 0x0A, STEP_SWAP_TARGET, 0),
    r(0x1A, 0x1A, STEP_LOAD_CURSOR, 0),
    r(0x1B, 0x0F, STEP_LOAD_TARGET, 0),
    r(0x1B, 0x1B, STEP_LOAD_CURSOR, 0),
    r(0x1C, 0x0E, STEP_LOAD_TARGET, 0),
    r(0x1C, 0x1C, STEP_LOAD_CURSOR, 0),
    r(0x1D, 0x0C, STEP_LOAD_TARGET, 0),
    r(0x1D, 0x1D, STEP_LOAD_CURSOR, 0),
    r(0x1E, 0x1E, STEP_LOAD_CURSOR, 0),
    // The parts, onto their weapon.
    r(0x20, 0x02, STEP_COMBINE, 0x04),
    r(0x21, 0x05, STEP_COMBINE, 0x06),
    r(0x22, 0x07, STEP_COMBINE, 0x08),
    // The herbs, through the mix question: green 0x26, red 0x27, blue 0x28.
    r(0x26, 0x27, STEP_MIX, 0x2A),
    r(0x26, 0x26, STEP_MIX, 0x29),
    r(0x26, 0x28, STEP_MIX, 0x2B),
    r(0x26, 0x29, STEP_MIX, 0x2C),
    r(0x26, 0x2B, STEP_MIX, 0x2D),
    r(0x27, 0x26, STEP_MIX, 0x2A),
    r(0x27, 0x2B, STEP_MIX, 0x2E),
    r(0x28, 0x26, STEP_MIX, 0x2B),
    r(0x28, 0x2A, STEP_MIX, 0x2E),
    r(0x28, 0x29, STEP_MIX, 0x2D),
    r(0x29, 0x28, STEP_MIX, 0x2D),
    r(0x29, 0x26, STEP_MIX, 0x2C),
    r(0x2A, 0x28, STEP_MIX, 0x2E),
    r(0x2B, 0x27, STEP_MIX, 0x2E),
    r(0x2B, 0x26, STEP_MIX, 0x2D),
    // The key items in halves.
    r(0x38, 0x39, STEP_COMBINE, 0x37),
    r(0x39, 0x38, STEP_COMBINE, 0x37),
    r(0x40, 0x41, STEP_COMBINE, 0x42),
    r(0x41, 0x40, STEP_COMBINE, 0x42),
];

/// The record for `item` onto `target`, if the table has one.
pub fn recipe(item: u8, target: u8) -> Option<Recipe> {
    RECIPES
        .iter()
        .copied()
        .find(|recipe| recipe.item == item && recipe.target == target)
}

/// USE's heals (`0x4F8878`): the share of the maximum life each restores, as
/// a right shift (`None` restores none), and the panel mode it starts.
fn heal(item: u8) -> Option<(Option<u8>, u8)> {
    Some(match item {
        // The first aid spray (it also counts the sprays, `0x98E9BA`).
        0x23 => (Some(0), 1),
        0x26 => (Some(2), 1),
        // The blue herb only cures.
        0x28 => (None, 2),
        0x29 => (Some(1), 1),
        0x2A => (Some(0), 1),
        0x2B => (Some(2), 3),
        0x2C => (Some(0), 1),
        0x2D => (Some(1), 3),
        0x2E => (Some(0), 3),
        _ => return None,
    })
}

/// One tick of the MAP page (`0x4FA240`). Sub 0 loads the floor's map and
/// its pictures in the tick (`common\\file\\map<nn>.adt`, the floor from
/// `0x4FAF80`); sub 1 slides it in for 14 ticks, the last of which plays
/// cue 9 and shows it (sub 3). There the cancel key plays the cancel cue and
/// slides it out (sub 2) for 14 ticks, back to the tab bar. Up and down
/// change floors only on a floor with others above or below it
/// (`0x53DA28`, `0x53DA40`) whose map the player holds, which no route
/// room has: the route's floor, the city's map 0, has none. START does
/// nothing on this page (`0x691F76` is 0).
fn re2_map(page: &mut ItemPage, input: Input, out: &mut Outcome) {
    match page.sub {
        MAP_LOAD => {
            page.sub = MAP_IN;
            page.command = 0;
        }
        MAP_FLOOR => page.sub = MAP_SHOWN,
        MAP_IN | MAP_OUT => {
            let count = page.command;
            page.command += 1;
            if count > 12 {
                page.command = 0;
                if page.sub == MAP_IN {
                    out.sounds.push(Sound::Map);
                    page.sub = MAP_SHOWN;
                } else {
                    page.page = PAGE_TABS;
                    page.sub = 0;
                }
            }
        }
        MAP_SHOWN if input.cancel => {
            out.sounds.push(Sound::Cancel);
            page.sub = MAP_OUT;
        }
        _ => {}
    }
}

/// One tick of the FILE page (`0x4F95B0`), as far as the binders go. Sub 0
/// loads the binders' model (`common\\data\\st_file.ts`) and runs sub 1's
/// first tick; sub 1 slides them in for 12 ticks more and shows them (sub
/// 3). There left and right (the keys with their repeat) turn the carousel
/// for 13 ticks (subs 4 and 5) with cue 10, after which the next binder is
/// in front and a key still held turns it again in the same tick. The
/// decide key opens the binder in front only when it holds a file
/// (`0x98EE60`), which nothing on the route gives; otherwise it does
/// nothing. The cancel key plays the cancel cue and slides the binders out
/// (sub 2) for 13 ticks, back to the tab bar. START does nothing here.
fn re2_file(page: &mut ItemPage, input: Input, out: &mut Outcome) {
    let turn = |page: &mut ItemPage, out: &mut Outcome| {
        if input.left {
            out.sounds.push(Sound::Turn);
            page.sub = FILE_LEFT;
        }
        if input.right {
            out.sounds.push(Sound::Turn);
            page.sub = FILE_RIGHT;
        }
    };
    match page.sub {
        FILE_LOAD | FILE_IN | FILE_OUT => {
            if page.sub == FILE_LOAD {
                page.sub = FILE_IN;
                page.command = 0;
            }
            let count = page.command;
            page.command += 1;
            if count > 11 {
                page.command = 0;
                if page.sub == FILE_IN {
                    page.sub = FILE_SHOWN;
                } else {
                    page.page = PAGE_TABS;
                    page.sub = 0;
                }
            }
        }
        FILE_SHOWN => {
            turn(page, out);
            // No binder holds a file: the decide key falls through to the
            // cancel key's test.
            if input.cancel {
                out.sounds.push(Sound::Cancel);
                page.sub = FILE_OUT;
            }
        }
        FILE_LEFT | FILE_RIGHT => {
            let count = page.command;
            page.command += 1;
            if count > 11 {
                page.binder = if page.sub == FILE_LEFT {
                    (page.binder + BINDERS - 1) % BINDERS
                } else {
                    (page.binder + 1) % BINDERS
                };
                page.sub = FILE_SHOWN;
                page.command = 0;
                page.step = 0;
                turn(page, out);
            }
        }
        _ => {}
    }
}

/// A cursor move on the grid (`0x4F8177`) or over COMBN's target: right,
/// left, down and up, in that order, all in one tick. Returns whether up
/// left the top row for the tab bar, which only the grid allows.
fn move_cursor(cursor: &mut u8, slots: u8, input: Input, tabs: bool) -> bool {
    if input.right && *cursor < slots.saturating_sub(1) {
        *cursor += 1;
    }
    if input.left && *cursor > 0 && *cursor != PERSONAL_SLOT {
        *cursor -= 1;
    }
    if input.down {
        if *cursor < slots.saturating_sub(2) {
            *cursor += 2;
        } else if *cursor == PERSONAL_SLOT {
            *cursor = 1;
        }
    }
    if input.up {
        if *cursor == PERSONAL_SLOT {
            return tabs;
        } else if *cursor > 1 {
            *cursor -= 2;
        } else {
            *cursor = PERSONAL_SLOT;
        }
    }
    false
}

impl GameState {
    /// Open the item page with the screen: the inventory takes its slots'
    /// places, empty ones included, until [`GameState::close_re2_items`].
    pub fn open_re2_items(&mut self) {
        let capacity = self.inventory_capacity();
        self.inventory
            .resize(capacity, super::InventoryItem::default());
        self.re2_items = Some(ItemPage::default());
    }

    /// Close the item page: the inventory compacts again.
    pub fn close_re2_items(&mut self) {
        if self.re2_items.take().is_some() {
            self.rebuild_slots();
        }
    }

    /// Slot `index` of `Item_work` as `[id, count]`; 10 is the personal
    /// item.
    pub fn re2_slot(&self, index: u8) -> [u8; 2] {
        if index == PERSONAL_SLOT {
            return self.re2.personal_item;
        }
        self.inventory
            .get(usize::from(index))
            .map_or([0, 0], |stack| [stack.id, stack.quantity])
    }

    fn set_re2_slot(&mut self, index: u8, [id, quantity]: [u8; 2]) {
        if index == PERSONAL_SLOT {
            self.re2.personal_item = [id, quantity];
            return;
        }
        let index = usize::from(index);
        if index >= self.inventory.len() {
            self.inventory
                .resize(index + 1, super::InventoryItem::default());
        }
        self.inventory[index] = super::InventoryItem { id, quantity };
    }

    /// Show item `item`'s description (message `0x10 + item`), from the
    /// pack's `ITEM_<id>_DESCRIPTION`.
    pub fn show_re2_description(&mut self, item: u8, bytes: &[u8]) {
        self.message.set_re2_timing(true, true);
        if !self.begin_message(0x10 + item, 0) {
            return;
        }
        self.message.feed_source(bytes);
        self.sync_message_choice();
    }

    fn re2_slot_count(&self) -> u8 {
        self.inventory_capacity() as u8
    }

    /// One tick of the status screen's item page and its panel, before the
    /// message's tick. `None` while the page isn't up.
    pub fn tick_re2_items(&mut self, input: Input) -> Option<Outcome> {
        let mut page = self.re2_items?;
        let mut out = Outcome::default();
        match page.page {
            PAGE_TABS => self.re2_tabs(&mut page, input, &mut out),
            PAGE_MAP => re2_map(&mut page, input, &mut out),
            PAGE_FILE => re2_file(&mut page, input, &mut out),
            _ => self.re2_item_page(&mut page, input, &mut out),
        }
        page.panel.tick();
        if out.close {
            page.page = 0;
        }
        out.close = page.page == 0;
        self.re2_items_trace = page.trace();
        self.re2_items = Some(page);
        Some(out)
    }

    /// The tab bar (`0x4F8050`): FILE, MAP, ITEMS and EXIT, its cursor held
    /// at both ends.
    fn re2_tabs(&mut self, page: &mut ItemPage, input: Input, out: &mut Outcome) {
        let before = page.tab;
        if input.right && page.tab < TAB_EXIT {
            page.tab += 1;
        }
        if input.left && page.tab > 0 {
            page.tab -= 1;
        }
        if page.tab != before {
            out.sounds.push(Sound::Cursor);
        }
        if input.confirm {
            if page.tab == TAB_EXIT {
                out.sounds.push(Sound::Cancel);
                out.close = true;
            } else {
                // Page `5 - tab`, with the page's bytes `+2` to `+7` and the
                // cursor cleared.
                out.sounds.push(Sound::Decide);
                page.page = 5 - page.tab;
                page.back_to_grid();
                page.follow = false;
                page.cursor = 0;
            }
        }
        if input.cancel || input.start {
            out.sounds.push(Sound::Cancel);
            out.close = true;
        }
    }

    /// The item page (`0x4F8160`), by sub (`0x4F85FC`).
    fn re2_item_page(&mut self, page: &mut ItemPage, input: Input, out: &mut Outcome) {
        match page.sub {
            SUB_GRID => self.re2_grid(page, input, out),
            SUB_MENU => {
                // `0x4F83C3`: down goes 3, 2, 1, 3; up the other way.
                if input.down {
                    page.command_cursor = if page.command_cursor <= COMMAND_COMBINE {
                        COMMAND_USE
                    } else {
                        page.command_cursor - 1
                    };
                }
                if input.up {
                    page.command_cursor = if page.command_cursor >= COMMAND_USE {
                        COMMAND_COMBINE
                    } else {
                        page.command_cursor + 1
                    };
                }
                if input.up || input.down {
                    out.sounds.push(Sound::Cursor);
                }
                if input.confirm {
                    page.sub = SUB_COMMAND;
                    page.command = page.command_cursor;
                    page.step = 0;
                    page.target = page.cursor;
                    out.sounds.push(Sound::Decide);
                } else if input.cancel {
                    page.close_menu();
                    out.sounds.push(Sound::Cancel);
                }
            }
            SUB_COMMAND => match page.command {
                COMMAND_COMBINE => self.re2_combine(page, input, out),
                COMMAND_CHECK => self.re2_check(page, out),
                COMMAND_USE => self.re2_use(page),
                _ => {}
            },
            // `0x4F856A` and `0x4F8595`: five ticks each.
            SUB_OPENING | SUB_CLOSING => {
                let count = page.command;
                page.command += 1;
                if count > 3 {
                    page.sub = if page.sub == SUB_OPENING {
                        SUB_MENU
                    } else {
                        SUB_GRID
                    };
                    page.command = 0;
                }
            }
            _ => {}
        }
    }

    /// The grid (`0x4F8177`).
    fn re2_grid(&mut self, page: &mut ItemPage, input: Input, out: &mut Outcome) {
        let before = page.cursor;
        if move_cursor(&mut page.cursor, self.re2_slot_count(), input, true) {
            page.page = PAGE_TABS;
            out.sounds.push(Sound::Cancel);
            return;
        }
        if page.cursor != before {
            out.sounds.push(Sound::Cursor);
        }
        if input.confirm && self.re2_slot(page.cursor)[0] != 0 {
            // `0x4FF060` builds the menu with its cursor on the top entry.
            // (The custom handgun's extra entry, command 0, isn't ported.)
            page.command_cursor = COMMAND_USE;
            page.sub = SUB_OPENING;
            page.command = 0;
            out.sounds.push(Sound::Decide);
        } else if input.cancel {
            page.page = PAGE_TABS;
            out.sounds.push(Sound::Cancel);
        } else if input.start {
            out.sounds.push(Sound::Cancel);
            out.close = true;
        }
    }

    /// USE and EQUIP (`0x4F8620`).
    fn re2_use(&mut self, page: &mut ItemPage) {
        let [item, _] = self.re2_slot(page.cursor);
        match page.step {
            0 if item < FIRST_AMMO => {
                // EQUIP: the weapon under the cursor, or none if it is the one
                // equipped.
                let equipped = self.equipped == Some(item);
                self.set_equipped((!equipped).then_some(item));
                page.close_menu();
            }
            0 if item <= LAST_AMMO => {
                self.show_re2_global(MESSAGE_BULLETS);
                page.goto(1);
            }
            0 => {
                if let Some((share, mode)) = heal(item) {
                    let max = super::RE2_MAX_HEALTH;
                    page.amount = share.map_or(0, |shift| max >> shift);
                    page.panel.mode = mode;
                    if mode != 1 {
                        self.re2.player.poisoned = false;
                        self.re2.player.poison_timer = 0;
                    }
                    page.goto(4);
                } else if self.re2.flag(0, item) {
                    page.goto(3);
                } else {
                    self.show_re2_global(MESSAGE_NOT_NOW);
                    page.goto(1);
                }
            }
            1 if !self.message.active => page.close_menu(),
            2 => {
                // The item is used up, and COMBN's step 2 closes the gap.
                self.set_re2_slot(page.cursor, [0, 0]);
                page.command = COMMAND_COMBINE;
                page.goto(2);
            }
            3 => {
                // A key item the room wants: the screen closes, and the
                // scripts read the used item (`0x98EAE8`).
                self.record_used_item(item);
                page.page = 0;
            }
            4 if page.panel.mode == 0 => {
                let health = &mut self.entities[0].health;
                *health = health
                    .saturating_add(page.amount)
                    .min(super::RE2_MAX_HEALTH);
                let health = *health;
                self.set_health_copy(health);
                page.goto(2);
            }
            _ => {}
        }
    }

    /// CHECK (`0x4F9260`, steps at `0x4F9570`): the item turns in and out
    /// around its description.
    fn re2_check(&mut self, page: &mut ItemPage, out: &mut Outcome) {
        // How many ticks each step lasts. Step 0 loads the model and runs
        // step 1's code in the same tick.
        const LENGTHS: [u8; 13] = [0, 5, 5, 7, 5, 5, 9, 0, 5, 5, 7, 5, 5];
        if page.step == 0 {
            page.step = 1;
        }
        match page.step {
            7 => {
                if !self.message.active {
                    // The cancel cue, and the item's name stops being the
                    // unchecked one (`0x98EBE4`, not ported).
                    out.sounds.push(Sound::Cancel);
                    page.goto(8);
                }
            }
            13 => page.back_to_grid(),
            step => {
                page.count += 1;
                if page.count == LENGTHS[usize::from(step)] {
                    if step == 6 {
                        out.describe = Some(self.re2_slot(page.cursor)[0]);
                        page.step = 7;
                    } else {
                        page.goto(step + 1);
                    }
                }
            }
        }
    }

    /// COMBN (`0x4F88B0`, steps at `0x4F9230`).
    fn re2_combine(&mut self, page: &mut ItemPage, input: Input, out: &mut Outcome) {
        let cursor = page.cursor;
        let target = page.target;
        let [item, count] = self.re2_slot(cursor);
        let [other, other_count] = self.re2_slot(target);
        let sum = u16::from(count) + u16::from(other_count);
        match page.step {
            0 => {
                // The target, moved like the grid's cursor.
                let before = page.target;
                move_cursor(&mut page.target, self.re2_slot_count(), input, false);
                if page.target != before {
                    out.sounds.push(Sound::Cursor);
                }
                if input.confirm {
                    // `0x502590`: the same slot and an empty one combine with
                    // nothing.
                    let found = (target != cursor && other != 0)
                        .then(|| recipe(item, other))
                        .flatten();
                    match found {
                        Some(found) => {
                            page.result = found.result;
                            page.step = found.step;
                            out.sounds.push(Sound::Decide);
                        }
                        None => {
                            page.step = 0;
                            out.sounds.push(Sound::Buzzer);
                        }
                    }
                } else if input.cancel {
                    page.sub = SUB_MENU;
                    page.command = 0;
                    out.sounds.push(Sound::Cancel);
                }
            }
            1 => {
                page.count += 1;
                if page.count == 11 {
                    if page.follow {
                        page.cursor = page.target;
                    }
                    page.follow = false;
                    page.goto(2);
                }
            }
            2 => {
                page.count += 1;
                if page.count == 4 {
                    page.count = 0;
                    self.re2_compact_step(page);
                }
            }
            3 => {
                let first = page.count == 0;
                page.count = page.count.wrapping_add(1);
                if first {
                    self.show_re2_global(MESSAGE_LOADED);
                } else if !self.message.active {
                    page.back_to_grid();
                }
            }
            STEP_COMBINE => {
                let result = page.result;
                self.set_re2_slot(cursor, [result, item_max(result)]);
                self.set_re2_slot(target, [0, 0]);
                // The equipped weapon becomes the result, whichever of the
                // two it was.
                if matches!(self.equipped, Some(id) if id == item || id == other) {
                    self.set_equipped(Some(result));
                }
                page.goto(1);
            }
            STEP_LOAD_CURSOR => {
                let max = item_max(item);
                if (count == max && item < FIRST_AMMO) || (item == 0x0F && count == 0xFF) {
                    page.goto(3);
                    return;
                }
                if u16::from(max) >= sum {
                    self.set_re2_slot(cursor, [item, sum as u8]);
                    self.set_re2_slot(target, [0, 0]);
                } else {
                    self.set_re2_slot(cursor, [item, max]);
                    self.set_re2_slot(target, [other, (sum - u16::from(max)) as u8]);
                }
                page.follow = false;
                page.goto(1);
            }
            STEP_LOAD_TARGET => {
                let max = item_max(other);
                if (other_count >= max && other < FIRST_AMMO)
                    || (other == 0x0F && other_count == 0xFF)
                {
                    page.goto(3);
                    return;
                }
                if u16::from(max) >= sum {
                    self.set_re2_slot(target, [other, sum as u8]);
                    self.set_re2_slot(cursor, [0, 0]);
                } else {
                    self.set_re2_slot(target, [other, max]);
                    self.set_re2_slot(cursor, [item, (sum - u16::from(max)) as u8]);
                }
                page.follow = true;
                page.goto(1);
            }
            STEP_SWAP_CURSOR => {
                // The launcher under the cursor takes the target's rounds,
                // and its own go back as a stack.
                self.set_re2_slot(cursor, [other - 0xF, other_count]);
                self.set_re2_slot(target, [item + 0xF, count]);
                if self.equipped == Some(item) {
                    self.set_equipped(Some(other - 0xF));
                }
                if count == 0 {
                    self.set_re2_slot(target, [0, 0]);
                }
                page.follow = false;
                page.goto(1);
            }
            STEP_SWAP_TARGET => {
                self.set_re2_slot(target, [item - 0xF, count]);
                self.set_re2_slot(cursor, [other + 0xF, other_count]);
                if self.equipped == Some(other) {
                    self.set_equipped(Some(item - 0xF));
                }
                if other_count == 0 {
                    self.set_re2_slot(cursor, [0, 0]);
                }
                page.follow = false;
                page.goto(1);
            }
            STEP_MIX => {
                if page.count == 0 {
                    self.show_re2_global(MESSAGE_MIX);
                    page.count = 1;
                } else if !self.message.active {
                    if self.message.menu_choice_id() & 1 == 0 {
                        page.step = STEP_COMBINE;
                    } else {
                        page.follow = false;
                        page.goto(0);
                    }
                }
            }
            _ => {}
        }
    }

    /// COMBN's step 2, every fourth tick: the first hole among the slots in
    /// use takes the slot after it, else the menu closes.
    fn re2_compact_step(&mut self, page: &mut ItemPage) {
        let slots = self.re2_slot_count();
        let empty = (0..slots).find(|&slot| self.re2_slot(slot)[0] == 0);
        let empties = (0..slots)
            .filter(|&slot| self.re2_slot(slot)[0] == 0)
            .count() as u8;
        match empty {
            Some(hole) if hole < 9 && hole < slots - empties => {
                let next = self.re2_slot(hole + 1);
                self.set_re2_slot(hole, next);
                self.set_re2_slot(hole + 1, [0, 0]);
            }
            _ => {
                page.close_menu();
                page.step = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests;
