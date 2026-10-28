//! INVENTORY: the slot editor and its icon picker.
//!
//! One row per capacity slot shows the slot number, the item icon, its name
//! and the quantity. Left/right step the quantity inside the item's limit
//! (down to zero, which keeps the item) and START clears the slot. Confirm
//! opens the picker: a grid of every item icon, as many to a row as fit, with
//! an EMPTY cell first. Every edit goes through [`GameState`] and its slot
//! bookkeeping; the equipped and selected items are dropped when an edit
//! leaves them dangling.

use super::widgets::{self, Grid, Label, ROW_H, ROW_W, Scroll, Value, grid_step, step_choice};
use super::{Cx, Hit, Step};
use crate::game::{GameState, InventoryItem};
use crate::gui::canvas::Rect;
use crate::gui::text::Align;
use crate::gui::theme::{Role, palette};
use crate::gui::widgets::Painter;
use crate::item_catalog::ItemCatalog;
use crate::items;
use crate::ui::UiInput;

/// The inventory sizes the size row steps through.
pub const INVENTORY_SIZES: [u8; 5] = [2, 4, 6, 8, 10];
/// The size row's pointer index; slots are indexed from 0.
const SIZE_ROW: usize = usize::MAX;

/// Step the inventory size; one equal to the character's own clears the
/// override so it follows a character change again.
fn step_size(game: &mut GameState, input: UiInput) {
    let current = game.inventory_capacity();
    let index = INVENTORY_SIZES
        .iter()
        .position(|&size| usize::from(size) >= current)
        .unwrap_or(INVENTORY_SIZES.len() - 1);
    if let Some(next) = step_choice(index, INVENTORY_SIZES.len(), input) {
        let size = INVENTORY_SIZES[next];
        let own = usize::from(size) == game.character_inventory_capacity();
        game.set_inventory_size((!own).then_some(size));
    }
}

/// The last pickable id: the PC-only Minimi.
pub const PICKER_LAST_ITEM: u8 = items::ITEM_MINIMI;
/// The picker's cell count: EMPTY plus `0x01..=0x70`.
pub const PICKER_CELLS: usize = PICKER_LAST_ITEM as usize + 1;

/// The highest quantity `item` can hold: the merge cap for an item that
/// merges (the ammunition and the ribbon), its `stack` otherwise (0: no
/// quantity).
pub fn quantity_limit(defs: &ItemCatalog, item: u8) -> u8 {
    let def = defs.get(item);
    if def.merge {
        items::ITEM_QUANTITY_CAP
    } else {
        def.stack
    }
}

/// The quantity a freshly picked item starts with: its `stack`, and at least
/// one.
pub fn default_quantity(defs: &ItemCatalog, item: u8) -> u8 {
    defs.get(item).stack.max(1)
}

/// Put `item` into `slot` (`0` clears it) and return the slot it landed in.
/// A slot past the held stacks appends, since the inventory is a compact
/// list, so the item lands in the first empty slot.
pub fn assign(game: &mut GameState, slot: usize, item: u8) -> usize {
    if slot >= game.inventory_capacity() {
        return slot;
    }
    let mut landed = slot;
    if item == 0 {
        if slot < game.inventory.len() {
            game.inventory.remove(slot);
        }
    } else {
        let stack = InventoryItem {
            id: item,
            quantity: default_quantity(&game.item_defs, item),
        };
        match game.inventory.get_mut(slot) {
            Some(existing) => *existing = stack,
            None => {
                landed = game.inventory.len();
                game.inventory.push(stack);
            }
        }
    }
    game.clear_dangling_items();
    game.rebuild_slots();
    landed
}

/// Step `slot`'s quantity by `delta` inside `0..=quantity_limit`; zero keeps
/// the item in its slot. An item without a quantity is left alone.
pub fn adjust(game: &mut GameState, slot: usize, delta: i32) {
    let Some(stack) = game.inventory.get(slot).copied() else {
        return;
    };
    let limit = i32::from(quantity_limit(&game.item_defs, stack.id));
    if limit == 0 {
        return;
    }
    let quantity = i32::from(stack.quantity).min(limit) + delta;
    game.inventory[slot].quantity = quantity.clamp(0, limit) as u8;
    game.rebuild_slots();
}

/// The icon picker's cursor.
#[derive(Debug, Clone, Copy)]
pub struct Picker {
    cell: usize,
    scroll: Scroll,
    /// The grid's columns as last laid out.
    columns: usize,
}

impl Picker {
    /// Open on `item`'s cell.
    fn new(item: u8, columns: usize) -> Self {
        Self {
            cell: usize::from(item.min(PICKER_LAST_ITEM)),
            scroll: Scroll::default(),
            columns: columns.max(1),
        }
    }

    /// The highlighted item id (0 = EMPTY).
    pub fn item(&self) -> u8 {
        self.cell as u8
    }

    /// The first grid row drawn.
    pub fn scroll_row(&self) -> usize {
        self.scroll.top()
    }
}

/// The INVENTORY tool's state.
#[derive(Debug, Clone)]
pub struct InventoryEditor {
    slot: usize,
    /// The cursor is on the size row above the slots.
    on_size: bool,
    scroll: Scroll,
    picker: Option<Picker>,
    /// The picker's columns as last laid out.
    columns: usize,
}

impl Default for InventoryEditor {
    fn default() -> Self {
        Self {
            slot: 0,
            on_size: false,
            scroll: Scroll::default(),
            picker: None,
            columns: 8,
        }
    }
}

impl InventoryEditor {
    /// The slot under the cursor.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// Whether the cursor is on the size row.
    pub fn on_size(&self) -> bool {
        self.on_size
    }

    /// The open picker, if any.
    pub fn picker(&self) -> Option<&Picker> {
        self.picker.as_ref()
    }

    /// Clamp the remembered slot to the live capacity after the menu was
    /// closed: a character change can shrink the inventory while it is
    /// hidden, leaving the cursor past the last row.
    pub(super) fn refresh(&mut self, game: &GameState) {
        let capacity = game.inventory_capacity();
        self.slot = self.slot.min(capacity.saturating_sub(1));
    }

    /// Scroll the slots, or the open picker, by `rows` without moving the
    /// cursor.
    pub(super) fn scroll_by(&mut self, rows: isize) {
        match self.picker.as_mut() {
            Some(picker) => picker.scroll.by(rows),
            None => self.scroll.by(rows),
        }
    }

    /// Put the cursor on pointer target `index`: a picker cell or a slot.
    pub(super) fn point(&mut self, index: usize, game: &GameState) {
        match self.picker.as_mut() {
            Some(picker) => picker.cell = index.min(PICKER_CELLS - 1),
            None if index == SIZE_ROW => self.on_size = true,
            None => {
                self.on_size = false;
                self.slot = index.min(game.inventory_capacity().saturating_sub(1));
            }
        }
    }

    /// One input edge against the live inventory.
    pub(super) fn handle_input(&mut self, input: UiInput, game: &mut GameState) -> Step {
        let capacity = game.inventory_capacity();
        self.slot = self.slot.min(capacity.saturating_sub(1));
        if let Some(picker) = self.picker.as_mut() {
            if input.cancel {
                self.picker = None;
            } else if input.confirm {
                let item = picker.item();
                self.picker = None;
                // The cursor follows the item to the slot it landed in.
                self.slot = assign(game, self.slot, item);
                self.scroll.follow();
            } else if let Some(cell) = grid_step(picker.cell, PICKER_CELLS, picker.columns, input) {
                picker.cell = cell;
                picker.scroll.follow();
            }
            return Step::Stay;
        }
        if input.cancel {
            return Step::Back;
        }
        // The size row sits above slot 1; up/down wrap through both.
        if input.up || input.down {
            if self.on_size {
                self.on_size = false;
                self.slot = if input.up { capacity - 1 } else { 0 };
            } else if (input.up && self.slot == 0) || (input.down && self.slot + 1 == capacity) {
                self.on_size = true;
            } else if input.up {
                self.slot -= 1;
            } else {
                self.slot += 1;
            }
            self.scroll.follow();
        } else if self.on_size {
            step_size(game, input);
            self.slot = self.slot.min(game.inventory_capacity() - 1);
        } else if input.left {
            adjust(game, self.slot, -1);
        } else if input.right {
            adjust(game, self.slot, 1);
        } else if input.start {
            self.slot = assign(game, self.slot, 0);
        } else if input.confirm {
            let item = game.inventory.get(self.slot).map_or(0, |stack| stack.id);
            self.picker = Some(Picker::new(item, self.columns));
        }
        Step::Stay
    }

    /// Draw the slot rows, or the picker in their place.
    pub(super) fn draw(&mut self, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        if self.picker.is_some() {
            self.draw_picker(p, cx, area);
            return;
        }
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let game = cx.game;
        let capacity = game.inventory_capacity();
        let row_h = u(ROW_H);
        let width = area.w.min(u(ROW_W));
        // The size row, then the slots under it.
        let size_label = cx.gui("debug.inventory_size");
        let size = format!("{capacity}");
        let at = INVENTORY_SIZES
            .iter()
            .position(|&s| usize::from(s) == capacity)
            .unwrap_or(0);
        widgets::row(
            p,
            cx,
            Rect::new(area.x, area.y, width, row_h),
            SIZE_ROW,
            self.on_size,
            Label::plain(&size_label),
            Value::Step(&size, Some((INVENTORY_SIZES.len(), at))),
        );
        let area = Rect::new(
            area.x,
            area.y + row_h + u(24.0),
            area.w,
            area.h - row_h - u(24.0),
        );
        let visible = ((area.h / row_h).floor() as usize).max(1);
        let top = self.scroll.layout(self.slot, visible, capacity);
        let empty = cx.label("UI_DEBUG_EMPTY");
        for slot in (top..capacity).take(visible) {
            let rect = Rect::new(area.x, area.y + row_h * (slot - top) as f32, width, row_h);
            let lead = format!("{}", slot + 1);
            let selected = !self.on_size && slot == self.slot;
            match game.inventory.get(slot) {
                Some(stack) => {
                    let limit = quantity_limit(&game.item_defs, stack.id);
                    let name = cx.item_name(stack.id);
                    let label = Label {
                        icon: Some(cx.icon(stack.id)),
                        ..Label::lead(&lead, &name)
                    };
                    let quantity = format!("{}/{}", stack.quantity, limit);
                    let value = if limit > 0 {
                        Value::Step(&quantity, None)
                    } else {
                        Value::None
                    };
                    widgets::row(p, cx, rect, slot, selected, label, value);
                }
                None => {
                    let label = Label {
                        icon: Some(None),
                        color: palette::TEXT_DISABLED,
                        ..Label::lead(&lead, &empty)
                    };
                    widgets::row(p, cx, rect, slot, selected, label, Value::None);
                }
            }
        }
        widgets::scrollbar(
            p,
            area.x + width + u(20.0),
            area.y,
            row_h * visible as f32,
            top,
            visible,
            capacity,
        );
    }

    /// The picker: a grid of every item's icon with the highlighted item's
    /// id and name underneath.
    fn draw_picker(&mut self, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let footer_h = u(80.0);
        let grid = Grid::new(
            p,
            Rect::new(area.x, area.y, area.w, area.h - footer_h),
            132.0,
            false,
        );
        let columns = grid.columns;
        self.columns = columns;
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        picker.columns = columns;
        let rows = PICKER_CELLS.div_ceil(columns);
        let top = picker.scroll.layout(picker.cell / columns, grid.rows, rows);
        let picker = *picker;
        let first = top * columns;
        let last = (first + grid.rows * columns).min(PICKER_CELLS);
        for cell in first..last {
            let (rect, frame) = grid.cell(cell - first);
            let selected = cell == picker.cell;
            let icon = if cell == 0 { None } else { cx.icon(cell as u8) };
            widgets::grid_picture(p, frame, icon.as_ref(), selected);
            if icon.is_none() {
                let text = if cell == 0 {
                    cx.label("UI_DEBUG_EMPTY")
                } else {
                    format!("{cell:02X}")
                };
                p.text(
                    Role::Body,
                    frame.center_x(),
                    frame.center_y(),
                    Align::Center,
                    palette::TEXT_DIM,
                    &text,
                );
            }
            cx.hits.push((rect, Hit::Item(cell)));
        }
        grid.scrollbar(p, top, rows);
        let item = picker.item();
        let cy = area.bottom() - footer_h / 2.0;
        let label = if item == 0 {
            cx.label("UI_DEBUG_EMPTY")
        } else {
            format!("{item:02X}   {}", cx.item_name(item))
        };
        p.text(Role::Row, area.x, cy, Align::Left, palette::TEXT, &label);
    }
}
