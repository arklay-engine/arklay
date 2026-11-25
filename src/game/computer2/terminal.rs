//! Room 613's terminal: `Computer600` (`0x4D1820`), the culture experiment
//! room's staff registry. The player types a user name on the on-screen
//! keyboard (`Keyboard_Input`, `0x4D1FE0`, and `Keyboard`, `0x4D2240`); the
//! registry knows only `GUEST`, which raises 4:91 and `fg_status`'s `0x200`
//! and leaves at once, for the room's script to take the fingerprint and
//! open the terminal a second time. That visit prints the registration and
//! lowers `0x200` again.

use super::{Computer, Effect, Hint, SOUND_CLOSE, SOUND_OPEN, Text, World, cprint};
use crate::game::pad_repeat::{DOWN, LEFT, RIGHT, UP};

/// `Keyboard`'s sounds: a move (`0x2100000`) and a key (`0x2090000`).
const SOUND_MOVE: u8 = 0x10;
const SOUND_KEY: u8 = 0x09;

/// The keyboard's frame: `Wframe_move(0x30, 0x8E, 0xF7, 0x57)`.
const KEYBOARD_FRAME: [i32; 4] = [0x30, 0x8E, 0xF7, 0x57];

/// The keyboard's picture, sprite 0 as `Keyboard_set` places it: the
/// table's `(0, 0)` moved by `(0x30, 0x8E)`, `0xF7` by `0x57` from the
/// page's `(0, 0x1E)`.
const KEYBOARD_PICTURE: [i32; 6] = [0x30, 0x8E, 0xF7, 0x57, 0, 0x1E];

/// The keys `Keyboard` reads (`+0x2D9`): the top left key is ESC, which
/// cancel also gives; the top right is BS; the tall key on the right is
/// the enter. The letters are 2 (A) to 27 (Z), which `Get_moji_code`
/// (`0x4D2790`) adds `0x3F` to.
const KEY_ESC: u8 = 1;
const KEY_ENTER: u8 = 0x1C;
const KEY_BACK: u8 = 0x1D;
const MOJI: u8 = 0x3F;
/// `Keyboard_Input(8, 0)`: up to 8 letters, shown as typed.
const NAME_LENGTH: u8 = 8;

/// The fg_common flag the name raises (`Flg_on(fg_common, 0x5B)`).
const GUEST_FLAG: u8 = 0x5B;

/// The hint lines (`Mess_print` of the room's first message table).
const HINT_CULTURE: Hint = Hint(0xC);
const HINT_ACCESSING: Hint = Hint(2);
const HINT_ENTER_NAME: Hint = Hint(3);
const HINT_NOT_FOUND: Hint = Hint(4);
const HINT_REGISTER: Hint = Hint(6);
const HINT_PLEASE_WAIT: Hint = Hint(8);
const HINT_COMPLETE: Hint = Hint(9);
const HINT_VALID_TOP: Hint = Hint(0xA);
const HINT_VALID: Hint = Hint(0xB);

/// The keyboard's part of `pCom_work`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Keyboard {
    /// `+2` and `+3`: the key under the cursor, columns 0 to 9 and rows 0
    /// to 2.
    col: i8,
    row: i8,
    /// `+0x2D9`: the key `Keyboard` read this pass, 0 for none.
    pub(super) key: u8,
    /// Whether the memory's `key` is known: it was before this work, or
    /// `Keyboard` wrote it.
    pub(super) key_known: bool,
    /// `+0x2D8`: how many letters are typed.
    len: u8,
    /// `+0x2E4`: the typed keys, then on the enter their letters and a NUL.
    typed: [u8; NAME_LENGTH as usize + 1],
    /// `+0x2DA` and `+0x2DB`: the cursor's grey, pulsing between 0x40 and
    /// 0x60.
    pulse: u8,
    pulse_step: i8,
    /// `+0x2DE`: a key went down on the last pass, so the cursor's key is
    /// drawn pressed for one.
    pressed: u8,
    /// `+0x30F` and `+0x31F`: the picture's and the cursor's sprites are
    /// on (`Keyboard_set`).
    shown: bool,
    /// The cursor's sprite (`+0x310`): `[x, y, w, h, u, v]`.
    cursor: [i32; 6],
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            key: 0,
            key_known: false,
            len: 0,
            typed: [0; NAME_LENGTH as usize + 1],
            pulse: 0x40,
            pulse_step: 4,
            pressed: 0,
            shown: false,
            // `0x533CA0`, the table's sprite 1.
            cursor: [0, 0, 0x18, 0x18, 0x10, 0x20],
        }
    }
}

/// What the art draws of the keyboard: its picture, and the key under the
/// cursor again at the cursor's grey (`0x80` is the page's own colours),
/// in front of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardView {
    /// `[x, y, w, h, u, v]` on the page.
    pub picture: [i32; 6],
    pub cursor: [i32; 6],
    pub grey: u8,
}

impl Keyboard {
    /// `Computer_init`'s part: the cursor at the top left, nothing typed,
    /// the pulse from 0x40 and every sprite off.
    pub(super) fn init(&mut self) {
        self.col = 0;
        self.row = 0;
        self.len = 0;
        self.pulse = 0x40;
        self.pulse_step = 4;
        self.shown = false;
    }

    /// `Keyboard_set` (`0x4D21D0`): the sprites on or off, and the cursor
    /// on `A` either way.
    fn set(&mut self, shown: bool) {
        self.shown = shown;
        self.col = 1;
        self.row = 0;
        self.pressed = 0;
    }

    /// The typed name, up to its NUL.
    fn name(&self) -> &[u8] {
        let end = self
            .typed
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.typed.len());
        &self.typed[..end]
    }

    pub(super) fn view(&self) -> Option<KeyboardView> {
        self.shown.then_some(KeyboardView {
            picture: KEYBOARD_PICTURE,
            cursor: self.cursor,
            grey: self.pulse,
        })
    }

    /// The trace's keyboard: its column and row, the key read and the
    /// letters typed.
    pub(super) fn trace(&self) -> String {
        format!("{},{},{},{}", self.col, self.row, self.key, self.len)
    }
}

impl Computer {
    /// `Computer600` (`0x4D1820`). Returns whether the pass slept.
    pub(super) fn computer600(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        match self.r1 {
            1 => {
                // The second visit: no frame, and its first line at once.
                if self.computer_init(world, effects, false) {
                    return true;
                }
                let mut printed = false;
                cprint(
                    &mut self.console,
                    &self.strings,
                    0,
                    Text::RegisterNewline,
                    &mut printed,
                    effects,
                );
                return self.start600(world, effects);
            }
            2 => {
                if self.computer_init(world, effects, true) {
                    return true;
                }
                return self.start600(world, effects);
            }
            3 | 4 => return self.computer_exit(effects, true),
            5 => return self.computer_exit(effects, false),
            _ => {}
        }
        if self.wframe_in_out(effects) {
            self.tail600();
            return false;
        }
        // `0x689B7C` picks the confirm; nothing sets it, so it is the held
        // confirm, as `Computer200`'s.
        if self.printed && world.confirm {
            self.confirmed = true;
        }
        if self.confirmed {
            self.hint_bottom = None;
            self.hint_top = None;
        }
        self.prev_mode = self.c_mode;
        self.inner600(world, effects)
    }

    /// `0x4D196C`: the console is open and its first line starts.
    fn start600(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        self.printed = false;
        self.confirmed = false;
        if world.registered {
            self.c_mode = 8;
        } else {
            self.c_mode = 1;
            self.print(1, Text::Umbrella, effects);
            if self.printed {
                self.c_mode = 2;
            }
        }
        self.def600()
    }

    /// `Computer600`'s inner switch on `c_mode`. Returns whether the pass
    /// slept.
    fn inner600(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        let done = |s: &Self| s.printed && s.confirmed;
        match self.c_mode {
            0 => {
                // `pCom_work = Mem_top`, then the visit's open.
                self.r1 = if world.registered { 1 } else { 2 };
                return self.computer600(world, effects);
            }
            1 => {
                self.print(1, Text::Umbrella, effects);
                if self.printed {
                    self.c_mode = 2;
                }
            }
            2 => {
                self.hint_bottom = Some(HINT_CULTURE);
                self.print(1, Text::Culture, effects);
                if done(self) {
                    self.c_mode = 3;
                }
            }
            3 => {
                self.hint_bottom = Some(HINT_ACCESSING);
                self.print(1, Text::Accessing, effects);
                if self.printed {
                    self.c_mode = 4;
                }
            }
            4 => {
                self.hint_bottom = Some(HINT_ENTER_NAME);
                self.print(1, Text::EnterName, effects);
                if done(self) {
                    self.c_mode = 5;
                    self.kb_mode = 1;
                }
            }
            5 => {
                if self.keyboard_input(world, effects) {
                    self.kb_mode = 2;
                    if self.keyboard.name() == self.strings.terminal.guest.as_slice() {
                        self.c_mode = 7;
                        self.console.clear();
                    } else {
                        self.c_mode = 6;
                    }
                }
            }
            6 => {
                self.hint_bottom = Some(HINT_NOT_FOUND);
                self.print(1, Text::NotFound, effects);
                if done(self) {
                    self.c_mode = 4;
                }
            }
            7 => {
                self.hint_bottom = Some(HINT_REGISTER);
                self.print(1, Text::Register, effects);
                if done(self) {
                    effects.push(Effect::Registered(true));
                    effects.push(Effect::Common(GUEST_FLAG));
                    self.c_mode = 0xC;
                }
            }
            8 => {
                self.hint_bottom = Some(HINT_PLEASE_WAIT);
                self.print(1, Text::PleaseWait, effects);
                if self.printed {
                    self.c_mode = 9;
                }
            }
            9 => {
                self.hint_bottom = Some(HINT_COMPLETE);
                self.print(1, Text::Complete, effects);
                if done(self) {
                    self.c_mode = 0xA;
                }
            }
            0xA => {
                self.hint_top = Some(HINT_VALID_TOP);
                self.hint_bottom = Some(HINT_VALID);
                self.print(1, Text::Valid, effects);
                if done(self) {
                    effects.push(Effect::Registered(false));
                    self.r1 = 3;
                    return self.computer_exit(effects, true);
                }
            }
            0xB => {
                self.r1 = 4;
                return self.computer_exit(effects, true);
            }
            0xC => {
                // The fingerprint is the script's: the frame stays up.
                self.r1 = 5;
                return self.computer_exit(effects, false);
            }
            _ => {}
        }
        self.def600()
    }

    /// `0x4D1A1F`: ESC leaves, then the tail.
    fn def600(&mut self) -> bool {
        if self.keyboard.key == KEY_ESC {
            self.keyboard.key = 0;
            self.c_mode = 0xB;
        }
        self.tail600();
        false
    }

    /// `0x4D1A36`: the hints, the reset after a confirm or a change of
    /// state, `Console_trans(1)` and `Sprite_Trans(0x16C)`.
    fn tail600(&mut self) {
        if (self.printed && self.confirmed) || self.prev_mode != self.c_mode {
            self.printed = false;
            self.confirmed = false;
            self.hint_bottom = None;
            self.hint_top = None;
        }
        self.console.trans();
        self.view.drawn = true;
        self.r1 = 0;
    }

    /// `Wframe_in_out` (`0x4D2480`) for the keyboard's frame (modes 1 and
    /// 2, and their 5 and 6 while it moves; `Computer600` sets no other).
    /// Returns whether the frame is moving, which skips the pass's switch.
    fn wframe_in_out(&mut self, effects: &mut Vec<Effect>) -> bool {
        match self.kb_mode {
            1 | 5 => {
                if self.kb_mode == 1 {
                    effects.push(Effect::Sound(SOUND_OPEN));
                }
                self.frame_dir = 0;
                self.kb_mode = 5;
                if self.wframe_move(KEYBOARD_FRAME) {
                    return true;
                }
                self.keyboard.set(true);
                self.kb_mode = 0;
                false
            }
            2 | 6 => {
                if self.kb_mode == 2 {
                    effects.push(Effect::Sound(SOUND_CLOSE));
                }
                self.keyboard.set(false);
                self.frame_dir = 1;
                self.kb_mode = 6;
                if self.wframe_move(KEYBOARD_FRAME) {
                    return true;
                }
                self.kb_mode = 0;
                false
            }
            _ => {
                self.kb_mode = 0;
                false
            }
        }
    }

    /// `Keyboard_Input(8, 0)` (`0x4D1FE0`): a letter, BS, ESC or the enter.
    /// Returns whether the name is done (the enter or ESC), with its letters
    /// in `typed`.
    fn keyboard_input(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        let key = self.keyboard(world, effects);
        if key == 0 {
            return false;
        }
        self.keyboard.pressed = 1;
        if key == KEY_ESC {
            self.keyboard.len = 0;
        } else if key != KEY_ENTER {
            if key == KEY_BACK {
                if self.keyboard.len == 0 {
                    return false;
                }
                self.console.col = self.console.col.wrapping_sub(1);
                self.keyboard.len -= 1;
                self.console.put(b' ');
                return false;
            }
            if self.keyboard.len >= NAME_LENGTH {
                return false;
            }
            self.console.put(key.wrapping_add(MOJI));
            self.keyboard.typed[usize::from(self.keyboard.len)] = key;
            self.console.col = self.console.col.wrapping_add(1);
            self.keyboard.len += 1;
            return false;
        }
        let mut printed = false;
        cprint(
            &mut self.console,
            &self.strings,
            0,
            Text::Newline,
            &mut printed,
            effects,
        );
        let len = usize::from(self.keyboard.len);
        for letter in &mut self.keyboard.typed[..len] {
            *letter = letter.wrapping_add(MOJI);
        }
        self.keyboard.typed[len] = 0;
        self.keyboard.len = 0;
        true
    }

    /// `Keyboard` (`0x4D2240`): the cursor's moves on the key repeat, its
    /// sprite, and the key a confirm or a cancel gives.
    fn keyboard(&mut self, world: &World, effects: &mut Vec<Effect>) -> u8 {
        effects.push(Effect::PadRepeat);
        let kb = &mut self.keyboard;
        let mut moved = false;
        if world.repeat & UP != 0 {
            kb.row -= 1;
            moved = true;
            // The enter is two rows tall: up from its bottom skips its top.
            if kb.col == 9 && kb.row == 1 {
                kb.row = 0;
            }
            if kb.row < 0 {
                kb.row = 2;
            }
        }
        if world.repeat & DOWN != 0 {
            moved = true;
            kb.row += 1;
            if kb.row > 2 {
                kb.row = 0;
            }
        }
        if world.repeat & LEFT != 0 {
            moved = true;
            kb.col -= 1;
            if kb.col < 0 {
                kb.col = 9;
            }
        }
        if world.repeat & RIGHT != 0 {
            moved = true;
            kb.col += 1;
            if kb.col > 9 {
                kb.col = 0;
            }
        }
        if moved {
            effects.push(Effect::Sound(SOUND_MOVE));
        }
        kb.pulse = kb.pulse.wrapping_add_signed(kb.pulse_step);
        if kb.pulse == 0x60 || kb.pulse == 0x40 {
            kb.pulse_step = -kb.pulse_step;
        }
        let (col, row, pressed) = (i32::from(kb.col), i32::from(kb.row), i32::from(kb.pressed));
        kb.cursor = [
            col * 0x18 + 0x34,
            row * 0x18 + 0x9A,
            0x18,
            0x17 - pressed,
            col * 0x18 + 4,
            row * 0x18 - pressed + 0x2A,
        ];
        if col == 9 && row > 0 {
            // The enter, two keys tall.
            kb.cursor[1] = 0xB2;
            kb.cursor[3] = 0x2F - pressed;
            kb.cursor[5] = 0x42 - pressed;
        }
        kb.pressed = 0;
        kb.key_known = true;
        if world.confirm_pressed {
            effects.push(Effect::Sound(SOUND_KEY));
            let mut key = (kb.row * 10 + kb.col) as u8;
            if key == 9 {
                key = 0x1E;
            }
            if key < 9 {
                key += 1;
            }
            if key == 0x13 {
                key = KEY_BACK;
            }
            if key > 0x13 {
                key -= 1;
            }
            kb.key = key;
            self.console.blink = 0x10;
        } else {
            kb.key = 0;
        }
        if world.cancel_pressed {
            kb.key = KEY_ESC;
        }
        kb.key
    }
}

#[cfg(test)]
mod tests;
