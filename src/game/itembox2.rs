//! RE 2's item box: the lid's swing (`Item_box_entrance`, `0x4E9A20`) and
//! the status screen's item box mode (`St` 1: `St_init01` at `0x4FBEB0`,
//! `St_main01` at `0x4FC090`), its routines (`0x53DB78`: the fade out's set
//! and wait, the EXIT button `St_select_menu_m` and the page
//! `St_select_item_m`) and `Exchange_item` (`0x4FC5B0`).
//!
//! The page's bytes are kept under the executable's own numbers, so the
//! oracle's `ib` field compares one for one:
//!
//! | byte | field |
//! |---|---|
//! | `0x691F61` | [`BoxPage::routine`]: 0 and 1 the close, 2 EXIT, 3 the page |
//! | `0x691F62` | [`BoxPage::sub`]: 0 the inventory, 1 the box, 2 and 3 a scroll, 4 the exchange, 5 a message |
//! | `0x691F63` | [`BoxPage::count`], a scroll's ticks |
//! | `0x691F6C` | [`BoxPage::cursor`], the inventory's slot |
//! | `0x691F76` | [`BoxPage::gate`]: START closes only while it is 1 |
//! | `0x691F84` | [`BoxPage::top`]: the box's top row; the middle one, `top + 2`, is the chosen slot |
//! | `0x691F85` | [`BoxPage::scroll`], the rows' offset in a scroll |
//! | `0x692FBE` | [`BoxPage::pulse`], the chosen row's lines |
//!
//! The inventory's two-slot weapons carry a mark in `Item_work`'s third
//! byte, 1 on the left half and 2 on the right; arklay's inventory has no
//! such byte, so [`marks`] reads them off the slots, a two-slot id in two
//! neighbouring slots. The box's own mark is 3 for a two-slot weapon.

use super::GameState;
use super::status2::{Input, Sound};
use crate::scd2::host::item_max;

/// `Item_boxwork`'s slots; the box's rows wrap around them.
pub const BOX_SLOTS: usize = 64;

/// The routines (`0x53DB78`).
pub const ROUTINE_FADE_SET: u8 = 0;
pub const ROUTINE_FADE_WAIT: u8 = 1;
pub const ROUTINE_EXIT: u8 = 2;
pub const ROUTINE_PAGE: u8 = 3;

/// The page's subs.
pub const SUB_INVENTORY: u8 = 0;
pub const SUB_BOX: u8 = 1;
pub const SUB_UP: u8 = 2;
pub const SUB_DOWN: u8 = 3;
pub const SUB_EXCHANGE: u8 = 4;
pub const SUB_MESSAGE: u8 = 5;

/// `Exchange_item`'s message 8, "There's no room for it."
const MESSAGE_NO_ROOM: u8 = 8;

/// The box's mark for a two-slot weapon.
const BOX_WIDE: u8 = 3;

/// The ids the box stacks onto a stack in the inventory (`0x13 < id < 0x20`).
fn stacks(item: u8) -> bool {
    (0x14..0x20).contains(&item)
}

/// The two-slot weapons, whose icon takes two slots (`0x0E` to `0x12`).
pub fn is_wide(item: u8) -> bool {
    (0x0E..=0x12).contains(&item)
}

/// `Snd_se_on`'s ids for the box: the lid's `0x2150000` and a row's
/// scroll, `0x2140000`, from the room's bank.
pub const SOUND_LID: u32 = 0x0215_0000;
pub const SOUND_SCROLL: u32 = 0x0214_0000;

/// The lid's swing (`Item_box_entrance`): `WORD_00689C90` and
/// `0x00689C98` turn the box's object (`0x98E533`) about z until it passes
/// -399, then ease it to a stop, which opens the screen. The words keep
/// their last values after the swing, as the original's do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Entrance {
    /// `0x98E542`, which the typewriter's back routine shares
    /// ([`super::save2`]). Whether the lid's routine is set
    /// (`DWORD_0098E794`) is [`GameState::re2_back`].
    pub state: u8,
    pub speed: i16,
    pub step: i16,
    /// The box's object slot.
    pub object: u8,
    /// `fg_stop` before the swing (`0x991FC4`), or before the typewriter's
    /// prompt.
    pub saved_stop: u32,
}

/// The item box's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BoxPage {
    pub routine: u8,
    pub sub: u8,
    pub count: u8,
    pub cursor: u8,
    pub gate: u8,
    pub top: u8,
    pub scroll: i8,
    pub pulse: i16,
    pub pulse_falling: bool,
    /// The equipped slot (`DWORD_00691F68`'s low byte), 0x80 for none.
    pub equipped: u8,
    /// `St_init01`'s step 0 has run: the page's routines run from the next
    /// tick.
    pub started: bool,
    /// The arrows the draw shows pressed this tick, while up or down is
    /// held off the inventory (`0x9885FC & 0x1004` and `& 0x4008`): bit 0
    /// up, bit 1 down. The engine sets it from the held pad.
    pub arrows: u8,
}

/// No slot equipped.
const NONE: u8 = 0x80;

impl BoxPage {
    /// `St_init01`'s step 0 (`0x4FBEB0`): the page on the inventory's first
    /// slot, the box's rows as they were left, the lines' pulse at 1.
    pub fn new(top: u8, equipped: u8) -> Self {
        Self {
            routine: ROUTINE_PAGE,
            sub: SUB_INVENTORY,
            count: 0,
            cursor: 0,
            gate: 1,
            top,
            scroll: 0,
            pulse: 1,
            pulse_falling: false,
            equipped,
            started: true,
            arrows: 0,
        }
    }

    /// The box slot the middle row shows.
    pub fn chosen(&self) -> usize {
        usize::from(self.top.wrapping_add(2) & 0x3F)
    }

    /// `St_disp_itembox`'s pulse of the chosen row's lines: up by 2 to
    /// 0x3F, down by 2 to 0, on every tick the screen is drawn.
    pub fn step_pulse(&mut self) {
        self.pulse += if self.pulse_falling { -2 } else { 2 };
        if self.pulse >= 0x3F {
            self.pulse = 0x3F;
            self.pulse_falling = true;
        } else if self.pulse < 1 {
            self.pulse = 0;
            self.pulse_falling = false;
        }
    }

    /// The oracle's `ib` page bytes.
    pub fn trace(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.routine,
            self.sub,
            self.count,
            self.cursor,
            self.gate,
            self.top,
            self.scroll,
            self.pulse
        )
    }
}

/// What a tick of the box asks of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoxOutcome {
    /// The core bank's cues.
    pub sounds: Vec<Sound>,
    /// The room bank's: a row's scroll.
    pub room_sounds: Vec<u32>,
    /// The page has gone to routine 0 (or 1, from START): the close starts.
    pub close: bool,
}

/// The inventory's two-slot marks, 1 on a two-slot weapon's left half and
/// 2 on its right.
pub fn marks(slots: &[[u8; 2]]) -> Vec<u8> {
    let mut marks = vec![0; slots.len()];
    let mut index = 0;
    while index < slots.len() {
        let id = slots[index][0];
        if is_wide(id) && slots.get(index + 1).is_some_and(|next| next[0] == id) {
            marks[index] = 1;
            marks[index + 1] = 2;
            index += 2;
        } else {
            index += 1;
        }
    }
    marks
}

/// `Item_work` as the exchange sees it: the player's slots with their marks.
struct Work {
    slots: Vec<[u8; 3]>,
    equipped: u8,
}

impl Work {
    fn size(&self) -> usize {
        self.slots.len()
    }

    /// `Search_item(0)`: the first empty slot.
    fn first_empty(&self) -> Option<usize> {
        self.slots.iter().position(|slot| slot[0] == 0)
    }

    /// `Search_item(1)`: how many slots are empty.
    fn empties(&self) -> usize {
        self.slots.iter().filter(|slot| slot[0] == 0).count()
    }

    /// `Sort_item` (`0x502690`): the slots after the first empty one move
    /// up one, the equipped slot with them, and the last is cleared.
    fn sort(&mut self) {
        let Some(empty) = self.first_empty() else {
            return;
        };
        let size = self.size();
        for slot in empty + 1..size {
            self.slots[slot - 1] = self.slots[slot];
            if usize::from(self.equipped) == slot {
                self.equipped -= 1;
            }
        }
        self.slots[size - 1] = [0; 3];
    }

    /// `FUN_00502720(1)`: every slot moves down two, for a two-slot weapon
    /// at the top; an equipped slot pushed off the end is unequipped.
    fn shift_down(&mut self) {
        let size = self.size();
        for slot in (2..size).rev() {
            self.slots[slot] = self.slots[slot - 2];
        }
        if usize::from(self.equipped) < size - 2 {
            self.equipped += 2;
        } else {
            self.equipped = NONE;
        }
    }

    /// Unequip when the equipped slot is one of these.
    fn unequip(&mut self, slots: &[usize]) {
        if slots.contains(&usize::from(self.equipped)) {
            self.equipped = NONE;
        }
    }
}

impl GameState {
    /// `Sce_Itembox` (`0x4E99F0`): set the lid's routine for the box's
    /// object (its area's first parameter).
    /// The original has no guard; arklay keeps a swing that is running.
    pub fn open_re2_box_entrance(&mut self, object: u8) {
        if self.re2_back == super::save2::BackRoutine::ItemBox {
            return;
        }
        self.re2_back = super::save2::BackRoutine::ItemBox;
        let entrance = &mut self.re2_box_entrance;
        entrance.state = 0;
        entrance.object = object;
    }

    fn re2_lid(&mut self) -> Option<&mut i16> {
        let object = usize::from(self.re2_box_entrance.object);
        self.objects
            .record_mut(object)
            .map(|record| &mut record.rotation[2])
    }

    /// One tick of `Item_box_entrance`. Returns true on the tick the swing
    /// ends and the screen opens (game mode 1, `fg_status | 0x8000`).
    pub fn tick_re2_box_entrance(&mut self) -> bool {
        if self.re2_back != super::save2::BackRoutine::ItemBox {
            return false;
        }
        let mut entrance = self.re2_box_entrance;
        let mut lid = self.re2_lid().map_or(0, |lid| *lid);
        let mut open = false;
        let mut shut = false;
        let swing = |entrance: &mut Entrance, lid: &mut i16| {
            *lid = lid.wrapping_sub(entrance.speed);
            entrance.speed = entrance.speed.wrapping_add(entrance.step);
            if *lid < -399 {
                entrance.state = 2;
                entrance.step = -entrance.step;
            }
        };
        match entrance.state {
            0 => {
                let stop = self.re2.fg_stop_mut();
                entrance.saved_stop = *stop;
                *stop |= 0x7F00_0000;
                entrance.speed = 1;
                entrance.step = 3;
                self.push_re2_sound(SOUND_LID);
                entrance.state = 1;
                swing(&mut entrance, &mut lid);
            }
            1 => swing(&mut entrance, &mut lid),
            2 => {
                lid = lid.wrapping_sub(entrance.speed);
                entrance.speed = entrance.speed.wrapping_add(entrance.step);
                if entrance.speed <= 0 {
                    open = true;
                    entrance.speed = 0;
                    entrance.state = 3;
                    shut = Self::count_lid(&mut entrance, &mut lid);
                }
            }
            _ => shut = Self::count_lid(&mut entrance, &mut lid),
        }
        if shut {
            self.re2_back = super::save2::BackRoutine::None;
        }
        if let Some(record) = self.re2_lid() {
            *record = lid;
        }
        self.re2_box_entrance = entrance;
        open
    }

    /// The swing's last state: five ticks, then the lid back shut and the
    /// routine cleared: true when the caller clears [`GameState::re2_back`].
    fn count_lid(entrance: &mut Entrance, lid: &mut i16) -> bool {
        entrance.speed += 1;
        if entrance.speed > 4 {
            *lid = 0;
            entrance.state = 0;
            return true;
        }
        false
    }

    pub(crate) fn push_re2_sound(&mut self, id: u32) {
        if let Some(id) = crate::se2::SoundId::decode(id) {
            self.re2
                .sounds
                .push(crate::scd2::host::SoundRequest::new(id, [0; 3]));
        }
    }

    /// Open the box's page with the screen (`St_init01`): the inventory
    /// takes its slots' places, as the item page's does, and the box's rows
    /// start from its first slot (`St + 0x24`, `+ 0x25`).
    pub fn open_re2_box(&mut self) {
        let capacity = self.inventory_capacity();
        self.inventory
            .resize(capacity, super::InventoryItem::default());
        let equipped = self
            .equipped
            .and_then(|item| {
                self.inventory
                    .iter()
                    .position(|stack| stack.id == item && item != 0)
            })
            .map_or(NONE, |slot| slot as u8);
        self.re2_box = Some(BoxPage {
            equipped,
            started: false,
            ..self.re2_box_bytes
        });
    }

    /// `St_init01`'s step 0, on the screen's sixth tick, once its loads
    /// are done: the page's bytes set, and `fg_stop` back to what it was
    /// before the lid's swing (the original's is 0 again from this tick).
    pub fn start_re2_box(&mut self) {
        if let Some(page) = self.re2_box.as_mut() {
            *page = BoxPage::new(0, page.equipped);
            *self.re2.fg_stop_mut() = self.re2_box_entrance.saved_stop;
        }
    }

    /// Close the box's page: the inventory compacts again.
    pub fn close_re2_box(&mut self) {
        if let Some(page) = self.re2_box.take() {
            self.re2_box_bytes = page;
            self.rebuild_slots();
        }
    }

    /// The inventory's slots with their marks.
    fn re2_box_work(&self, equipped: u8) -> Work {
        let size = self.inventory_capacity();
        let plain: Vec<[u8; 2]> = (0..size)
            .map(|index| {
                self.inventory
                    .get(index)
                    .map_or([0, 0], |stack| [stack.id, stack.quantity])
            })
            .collect();
        let marks = marks(&plain);
        Work {
            slots: plain
                .iter()
                .zip(&marks)
                .map(|(&[id, count], &mark)| [id, count, mark])
                .collect(),
            equipped,
        }
    }

    fn set_re2_box_work(&mut self, work: &Work) {
        for (index, slot) in work.slots.iter().enumerate() {
            if index < self.inventory.len() {
                self.inventory[index] = super::InventoryItem {
                    id: slot[0],
                    quantity: slot[1],
                };
            }
        }
    }

    /// The equipped item after the page: the slot's, or none.
    fn sync_re2_box_equip(&mut self, equipped: u8) {
        let item = self
            .inventory
            .get(usize::from(equipped))
            .map(|stack| stack.id)
            .filter(|&id| id != 0 && equipped != NONE);
        if item != self.equipped {
            self.set_equipped(item);
        }
    }

    /// One tick of the box's page (`St_main01`'s routine), then its draw's
    /// pulse. `None` while the page isn't up.
    pub fn tick_re2_box(&mut self, input: Input) -> Option<BoxOutcome> {
        let mut page = self.re2_box?;
        let mut out = BoxOutcome::default();
        if !page.started {
            return Some(out);
        }
        page.gate = 1;
        match page.routine {
            ROUTINE_FADE_SET => page.routine = ROUTINE_FADE_WAIT,
            ROUTINE_FADE_WAIT => {}
            ROUTINE_EXIT => {
                // `St_select_menu_m` (`0x4FC0C0`).
                if input.down {
                    page.routine = ROUTINE_PAGE;
                    page.cursor = 1;
                    out.sounds.push(Sound::Cursor);
                } else if input.confirm || input.cancel {
                    out.sounds.push(Sound::Decide);
                    page.routine = ROUTINE_FADE_SET;
                    out.close = true;
                }
            }
            _ => self.re2_box_page(&mut page, input, &mut out),
        }
        page.step_pulse();
        self.re2_box = Some(page);
        Some(out)
    }

    /// The close's ticks, while the screen fades out: its routines and its
    /// draw run until the screen's task ends.
    pub fn tick_re2_box_close(&mut self) {
        if let Some(page) = self.re2_box.as_mut()
            && page.started
        {
            if page.routine == ROUTINE_FADE_SET {
                page.routine = ROUTINE_FADE_WAIT;
            }
            page.gate = 1;
            page.step_pulse();
        }
    }

    /// `St_select_item_m` (`0x4FC110`).
    fn re2_box_page(&mut self, page: &mut BoxPage, input: Input, out: &mut BoxOutcome) {
        match page.sub {
            SUB_INVENTORY => self.re2_box_inventory(page, input, out),
            SUB_BOX => {
                if input.up {
                    out.room_sounds.push(SOUND_SCROLL);
                    page.sub = SUB_UP;
                } else if input.down {
                    out.room_sounds.push(SOUND_SCROLL);
                    page.sub = SUB_DOWN;
                } else if input.cancel {
                    out.sounds.push(Sound::Cancel);
                    page.sub = SUB_INVENTORY;
                } else if input.confirm {
                    out.sounds.push(Sound::Decide);
                    page.sub = SUB_EXCHANGE;
                }
            }
            SUB_UP | SUB_DOWN => {
                page.gate = 0;
                let up = page.sub == SUB_UP;
                if page.count < 6 {
                    page.scroll += if up { 3 } else { -3 };
                    page.count += 1;
                } else {
                    // A held key would keep scrolling, with the cue for each
                    // row; the pad's edges don't hold.
                    page.sub = SUB_BOX;
                    page.scroll = 0;
                    page.count = 0;
                    page.top = if up {
                        page.top.wrapping_sub(1)
                    } else {
                        page.top.wrapping_add(1)
                    } & 0x3F;
                }
            }
            SUB_EXCHANGE => {
                page.gate = 0;
                page.sub = SUB_INVENTORY;
                self.re2_exchange(page);
            }
            SUB_MESSAGE => {
                page.gate = 0;
                if !self.message.active {
                    page.sub = SUB_BOX;
                }
            }
            _ => {}
        }
    }

    /// Sub 0: the inventory's cursor, held to the player's slots, up from
    /// the top row to EXIT.
    fn re2_box_inventory(&mut self, page: &mut BoxPage, input: Input, out: &mut BoxOutcome) {
        let size = self.inventory_capacity() as u8;
        let work = self.re2_box_work(page.equipped);
        let mark = |slot: u8| work.slots.get(usize::from(slot)).map_or(0, |slot| slot[2]);
        let before = page.cursor;
        let mut sound_from = before;
        let mut cursor = page.cursor;
        if input.right && cursor < size.saturating_sub(1) {
            if mark(cursor) == 1 {
                cursor += 1;
            }
            cursor += 1;
        }
        if input.left && cursor != 0 && cursor != 10 {
            if mark(cursor) == 2 {
                cursor = if cursor == 1 { 2 } else { cursor - 1 };
            }
            cursor -= 1;
        }
        if input.down {
            if cursor < size.saturating_sub(2) {
                cursor += 2;
            } else if cursor == 10 {
                cursor = 1;
            }
        }
        if input.up {
            if cursor < 2 || cursor == 10 {
                sound_from = 10;
                page.routine = ROUTINE_EXIT;
            } else {
                cursor -= 2;
            }
        }
        page.cursor = cursor;
        if sound_from != page.cursor {
            out.sounds.push(Sound::Cursor);
        }
        if input.confirm {
            out.sounds.push(Sound::Decide);
            page.sub = SUB_BOX;
        } else if input.cancel {
            out.sounds.push(Sound::Cancel);
            page.routine = ROUTINE_FADE_SET;
            out.close = true;
        } else if input.start && page.gate == 1 {
            out.sounds.push(Sound::Cancel);
            page.routine = ROUTINE_FADE_WAIT;
            out.close = true;
        }
    }

    /// `Exchange_item` (`0x4FC5B0`): the inventory's slot under the cursor
    /// and the box's chosen slot change places, a stack of rounds tops up
    /// the inventory's, and a two-slot weapon takes or frees two slots.
    fn re2_exchange(&mut self, page: &mut BoxPage) {
        let chosen = page.chosen();
        let mut slot = usize::from(page.cursor);
        let mut work = self.re2_box_work(page.equipped);
        let mut boxed = self.re2.item_box[chosen];
        let Some(&[id, count, mark]) = work.slots.get(slot) else {
            return;
        };
        let mut kind = if mark != 0 { 2 } else { 0 };
        if boxed[2] != 0 {
            kind += 1;
        }
        let [box_id, box_count, _] = boxed;
        if stacks(box_id) {
            let max = item_max(box_id);
            let mut merge = None;
            if box_id == id {
                if count == max {
                    work.slots[slot][1] = box_count;
                    boxed[1] = count;
                    return self.finish_re2_exchange(page, &work, chosen, boxed);
                }
                let sum = u16::from(count) + u16::from(box_count);
                if u16::from(max) < sum {
                    work.slots[slot][1] = max;
                    boxed[1] = (sum - u16::from(max)) as u8;
                    return self.finish_re2_exchange(page, &work, chosen, boxed);
                }
                merge = Some(slot);
            } else if id == 0 {
                merge = work
                    .slots
                    .iter()
                    .position(|stack| stack[0] == box_id && stack[1] <= max - box_count);
            }
            if let Some(into) = merge {
                work.slots[into][1] = work.slots[into][1].wrapping_add(box_count);
                boxed = [0; 3];
                return self.finish_re2_exchange(page, &work, chosen, boxed);
            }
        }
        match kind {
            0 => {
                work.unequip(&[slot]);
                if let Some(empty) = work.first_empty()
                    && empty < slot
                {
                    slot = empty;
                }
                work.slots[slot] = boxed;
                boxed = [id, count, mark];
                work.sort();
            }
            1 => {
                let empties = work.empties();
                if empties == 0 || (empties == 1 && work.slots[slot][0] == 0) {
                    self.show_re2_global(MESSAGE_NO_ROOM);
                    page.sub = SUB_MESSAGE;
                    return;
                }
                work.unequip(&[slot]);
                work.slots[slot][0] = 0;
                work.sort();
                work.shift_down();
                work.slots[0] = [box_id, box_count, 1];
                work.slots[1] = [box_id, box_count, 2];
                boxed = [id, count, mark];
            }
            2 => {
                if mark == 2 {
                    work.unequip(&[slot, slot.wrapping_sub(1)]);
                    work.slots[slot][0] = 0;
                    work.sort();
                    work.slots[slot - 1][0] = 0;
                    work.sort();
                } else {
                    work.unequip(&[slot, slot + 1]);
                    work.slots[slot][0] = 0;
                    work.sort();
                    work.slots[slot][0] = 0;
                    work.sort();
                }
                if let Some(empty) = work.first_empty() {
                    work.slots[empty] = boxed;
                }
                boxed = [id, count, BOX_WIDE];
            }
            _ => {
                if mark == 1 {
                    work.unequip(&[slot, slot + 1]);
                    work.slots[slot] = [box_id, box_count, 1];
                    work.slots[slot + 1] = [box_id, box_count, 2];
                } else {
                    work.unequip(&[slot, slot.wrapping_sub(1)]);
                    work.slots[slot - 1] = [box_id, box_count, 1];
                    work.slots[slot] = [box_id, box_count, 2];
                }
                boxed = [id, count, BOX_WIDE];
            }
        }
        self.finish_re2_exchange(page, &work, chosen, boxed);
    }

    fn finish_re2_exchange(
        &mut self,
        page: &mut BoxPage,
        work: &Work,
        chosen: usize,
        boxed: [u8; 3],
    ) {
        self.set_re2_box_work(work);
        self.re2.item_box[chosen] = boxed;
        page.equipped = work.equipped;
        self.sync_re2_box_equip(work.equipped);
    }

    /// The oracle's `ib` and `box` fields: the lid's state, speed, step and
    /// turn and the page's bytes; then the inventory's marks and the box's
    /// filled slots.
    pub fn re2_box_trace(&self) -> String {
        let entrance = self.re2_box_entrance;
        let lid = self
            .objects
            .records
            .get(usize::from(entrance.object))
            .map_or(0, |record| record.rotation[2]);
        let page = self.re2_box.unwrap_or(self.re2_box_bytes).trace();
        let plain: Vec<[u8; 2]> = (0..10)
            .map(|index| {
                self.inventory
                    .get(index)
                    .map_or([0, 0], |stack| [stack.id, stack.quantity])
            })
            .collect();
        let marks: String = marks(&plain).iter().map(u8::to_string).collect();
        let slots: Vec<String> = self
            .re2
            .item_box
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot[0] != 0)
            .map(|(index, slot)| format!("{index}:{:x}:{}:{}", slot[0], slot[1], slot[2]))
            .collect();
        let slots = if slots.is_empty() {
            "-".to_owned()
        } else {
            slots.join(";")
        };
        format!(
            "ib={},{},{},{},{} box={marks}/{slots}",
            entrance.state, entrance.speed, entrance.step, lid, page
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{GameState, InventoryItem};
    use crate::state::{RoomId, RoomState};

    /// The `box_201` recording's start: the handgun, the knife and 30
    /// rounds, and the box's rounds, Spark Shot and herb.
    fn game() -> GameState {
        let mut game = GameState::new(RoomId::parse("2010").unwrap(), &RoomState::default());
        game.is_re2_room = true;
        game.inventory = [(2, 18), (1, 1), (0x14, 30)]
            .iter()
            .map(|&(id, quantity)| InventoryItem { id, quantity })
            .collect();
        game.re2.item_box[0] = [0x14, 15, 0];
        game.re2.item_box[1] = [0x0E, 7, 3];
        game.re2.item_box[2] = [0x26, 1, 0];
        game.open_re2_box();
        game.start_re2_box();
        game
    }

    fn press(game: &mut GameState, input: Input) {
        game.tick_re2_box(input);
        // The recording's presses are 30 ticks apart: a scroll ends in them.
        for _ in 0..29 {
            game.tick_re2_box(Input::default());
        }
    }

    const UP: Input = Input {
        up: true,
        down: false,
        left: false,
        right: false,
        confirm: false,
        cancel: false,
        start: false,
    };
    const DOWN: Input = Input {
        down: true,
        up: false,
        ..UP
    };
    const CONFIRM: Input = Input {
        confirm: true,
        up: false,
        ..UP
    };

    fn slots(game: &GameState) -> Vec<(u8, u8)> {
        game.inventory
            .iter()
            .map(|stack| (stack.id, stack.quantity))
            .collect()
    }

    #[test]
    fn the_box_stacks_rounds_then_takes_and_gives_a_two_slot_weapon() {
        // `box_201`: down to the rounds, into the box, up twice to its rounds.
        let mut game = game();
        press(&mut game, DOWN);
        press(&mut game, CONFIRM);
        press(&mut game, UP);
        press(&mut game, UP);
        assert_eq!(game.re2_box.unwrap().chosen(), 0);
        press(&mut game, CONFIRM);
        press(&mut game, CONFIRM);
        // Tick 601: the box's 15 rounds onto the inventory's 30.
        assert_eq!(slots(&game)[2], (0x14, 45));
        assert_eq!(game.re2.item_box[0], [0; 3]);
        // Tick 691: the rounds for the Spark Shot, which takes slots 0 and 1.
        press(&mut game, DOWN);
        press(&mut game, CONFIRM);
        assert_eq!(&slots(&game)[..4], [(0x0E, 7), (0x0E, 7), (2, 18), (1, 1)]);
        assert_eq!(game.re2.item_box[1], [0x14, 45, 0]);
        // Tick 841: up to the Spark Shot, the box's herb for it.
        press(&mut game, UP);
        press(&mut game, CONFIRM);
        press(&mut game, DOWN);
        press(&mut game, CONFIRM);
        assert_eq!(&slots(&game)[..3], [(2, 18), (1, 1), (0x26, 1)]);
        assert_eq!(game.re2.item_box[2], [0x0E, 7, 3]);
    }

    #[test]
    fn a_two_slot_weapon_marks_both_halves() {
        let slots = [[0x0E, 7], [0x0E, 7], [2, 18], [0x14, 30]];
        assert_eq!(marks(&slots), [1, 2, 0, 0]);
    }

    #[test]
    fn the_pulse_runs_up_and_down_between_0_and_0x3f() {
        let mut page = BoxPage::new(0, NONE);
        let mut seen = vec![page.pulse];
        for _ in 0..40 {
            page.step_pulse();
            seen.push(page.pulse);
        }
        // The original's trace: 1, 3, ... 63, 61, ...
        assert_eq!(seen[31], 63);
        assert_eq!(seen[32], 61);
    }

    #[test]
    fn sort_moves_the_slots_after_a_hole_up_one() {
        let mut work = Work {
            slots: vec![[2, 18, 0], [0, 0, 0], [1, 1, 0], [0x14, 30, 0]],
            equipped: 0,
        };
        work.sort();
        assert_eq!(
            work.slots,
            [[2, 18, 0], [1, 1, 0], [0x14, 30, 0], [0, 0, 0]]
        );
    }
}
