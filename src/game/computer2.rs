//! RE2's computer: game mode 7 (`Sce_Windows`, `0x4EA0B0`), the `Windows`
//! task (`0x4D1F50`) and room 200's `Computer200` (`0x4D1170`), the counter's
//! DOOR LOCK SERVICE console that takes the Blue Card Key.
//!
//! The original runs it as task 1 while task 0, the room, is suspended, so
//! [`Computer::tick`] is one scheduler pass of that task: it holds through
//! the task's sleeps and returns what the pass asked of the game as
//! [`Effect`]s. The state is the original's: the task's `R0` to `R2` and
//! sleep count, `c_mode` (`0x98E52D`), the frame (`Wframe_move`) and the
//! console `Cprint` types into (`pCom_work`). It knows nothing of the game,
//! which hands it a [`World`] each tick, so it can be tested on its own.
//!
//! Mode 1 (`Computer600`, room 613's terminal with its keyboard) isn't
//! ported: it resumes at once.

/// The pack entry the converter writes the console's strings to, one a line
/// (`0x533AD0` and on, [`Strings`]).
pub const TEXT_ENTRY: &str = "ui/computer/rc200.txt";

/// The console's rows and columns (`pCom_work`'s `+4` chars, 12 rows of 30).
pub const ROWS: usize = 12;
pub const COLUMNS: usize = 30;

/// Room-bank sounds (`Snd_se_on(0x2NN0000)`).
const SOUND_OPEN: u8 = 0x06;
const SOUND_CLOSE: u8 = 0x07;
const SOUND_TYPE: u8 = 0x10;
const SOUND_DOOR: u8 = 0x12;

/// The frame's box: `Wframe_move(0xC, 0xA, 0xFC, 0x8B)`.
const FRAME: [i32; 4] = [0xC, 0xA, 0xFC, 0x8B];
/// `Wframe_move`'s steps: 13 draws, the fourteenth ends it.
const FRAME_STEPS: u8 = 12;

/// The cuts `Computer200` shows: the doors (9, then 2, and 0xC in
/// scenario B).
const CUT_DOOR_1: u8 = 9;
const CUT_DOOR_2: u8 = 2;
const CUT_DOOR_B: u8 = 0xC;
/// `Task_sleep(60)` between the door shots.
const DOOR_SLEEP: u16 = 60;

/// The console's strings (`0x533AD0` and on), Cprint's bytes without the
/// NUL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Strings {
    /// `DOOR LOCK SERVICE` and its rule.
    pub title: Vec<u8>,
    /// `Hall side doors: RELEASED`.
    pub released: Vec<u8>,
    /// `Hall side doors: LOCKED`.
    pub locked: Vec<u8>,
    /// `The doors can be unlocked by a CARD KEY.`
    pub card: Vec<u8>,
    /// `]`, printed at rate 0 after a no.
    pub newline: Vec<u8>,
    /// `Checking up ID CARD....`
    pub checking: Vec<u8>,
    /// `OK! Hall side doors lock released.`
    pub ok: Vec<u8>,
}

/// Which string Cprint is typing (`pCom_work+0x2F8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Text {
    Title,
    Released,
    Locked,
    Card,
    Newline,
    Checking,
    Ok,
}

impl Strings {
    fn get(&self, text: Text) -> &[u8] {
        match text {
            Text::Title => &self.title,
            Text::Released => &self.released,
            Text::Locked => &self.locked,
            Text::Card => &self.card,
            Text::Newline => &self.newline,
            Text::Checking => &self.checking,
            Text::Ok => &self.ok,
        }
    }
}

/// What the game holds this tick, as the computer reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct World {
    /// Confirm held (`Key & 0x1000`, the held keys, not the trigger).
    pub confirm: bool,
    /// `fg_status & 0x40000000`, scenario B (flag 1:1).
    pub scenario_b: bool,
    /// 4:70, the hall's side doors released.
    pub unlocked: bool,
    /// `fg_key` bit 17 (31:17).
    pub key17: bool,
    /// `Search_item_id(0x35) >= 0`: the Blue Card Key is in a slot.
    pub card: bool,
    /// `fg_message & 0x80`: a message is open.
    pub message_open: bool,
    /// `fg_message & 1`: the message closed on "no".
    pub message_no: bool,
    /// `scd_var_cut`, the cut on screen.
    pub cut: u8,
}

/// What a pass asks of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `Snd_se_on(0x2000000 | slot << 16)`: a room-bank sound, not placed.
    Sound(u8),
    /// `BYTE_00989EEA = cut; BYTE_0098F07B = 1`: `Bg_loadPC` shows the cut
    /// this tick.
    Cut(u8),
    /// `Flg_on(fg_common, 0x46)`: 4:70.
    Unlock,
    /// `Item_work[slot] = {0, 0, 0}; Sort_item()`: the Blue Card Key leaves.
    TakeCard,
    /// `Mess_set(0, 0x100, 7, 0xFF000000)` with the card selected: the door
    /// table's "Will you use the {item}?".
    AskCard,
    /// `Task_signal(0); Task_exit()`: the room's task runs again from the
    /// next tick.
    Resume,
}

/// `pCom_work`'s console: `Cprint`'s cursor, colour and delay, and the
/// characters it has typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Console {
    pub chars: [[u8; COLUMNS]; ROWS],
    pub attrs: [[u8; COLUMNS]; ROWS],
    /// `+0` and `+1`.
    pub col: u8,
    pub row: u8,
    /// `+0x2D7`.
    colour: u8,
    /// `+0x2D5`, the cursor's blink counter.
    blink: u8,
    /// `+0x2D6`, Cprint's delay.
    delay: u8,
    /// `+0x2F8` and `+0x2FC`: the string being typed and the read position.
    text: Option<Text>,
    pos: usize,
    /// Whether `Console_trans` showed the cursor this tick.
    pub cursor: bool,
}

impl Default for Console {
    fn default() -> Self {
        Self {
            chars: [[b' '; COLUMNS]; ROWS],
            attrs: [[0; COLUMNS]; ROWS],
            col: 0,
            row: 0,
            colour: 0,
            blink: 0,
            delay: 0,
            text: None,
            pos: 0,
            cursor: false,
        }
    }
}

impl Console {
    /// `Console_clr` (`0x4D2A50`).
    fn clear(&mut self) {
        self.chars = [[b' '; COLUMNS]; ROWS];
        self.attrs = [[0; COLUMNS]; ROWS];
        self.colour = 0;
        self.col = 0;
        self.row = 0;
    }

    /// `Console_roll_up` (`0x4D29E0`): rows 1 to 11 move up one, and the
    /// last becomes spaces.
    fn roll_up(&mut self) {
        self.chars.copy_within(1.., 0);
        self.attrs.copy_within(1.., 0);
        self.chars[ROWS - 1] = [b' '; COLUMNS];
        self.attrs[ROWS - 1] = [0; COLUMNS];
    }

    /// `Console_trans`'s cursor: the blink counter steps, and the cursor
    /// shows while its bit 4 is set.
    fn trans(&mut self) {
        self.blink = self.blink.wrapping_add(1) & 0x1F;
        self.cursor = self.blink & 0x10 != 0;
    }
}

/// One `Cprint` (`0x4D27A0`) call: `rate` ticks between characters (0
/// prints the rest at once), with the sounds it starts. Returns whether it
/// reached the string's end, which also sets `printed`.
fn cprint(
    console: &mut Console,
    strings: &Strings,
    rate: u8,
    text: Text,
    printed: &mut bool,
    effects: &mut Vec<Effect>,
) -> bool {
    if *printed {
        return false;
    }
    if console.text != Some(text) {
        console.text = Some(text);
        console.pos = 0;
        console.delay = 1;
    }
    console.delay = console.delay.wrapping_sub(1);
    if console.delay != 0 {
        return false;
    }
    console.delay = rate;
    let bytes = strings.get(text);
    let digit = |at: usize| bytes.get(at).map_or(0, |&b| b.wrapping_sub(b'0'));
    loop {
        let Some(&byte) = bytes.get(console.pos) else {
            console.text = None;
            *printed = true;
            return true;
        };
        let mut printable = false;
        match byte {
            b'[' => {
                let at = console.pos;
                let delay = u32::from(digit(at + 1)) * 100
                    + u32::from(digit(at + 2)) * 10
                    + u32::from(digit(at + 3));
                console.delay = delay as u8;
                console.pos += 4;
                return false;
            }
            b']' => {
                console.col = 0;
                console.row += 1;
            }
            b'^' => {
                let back = digit(console.pos + 1) * 10 + digit(console.pos + 2);
                console.col = console.col.wrapping_sub(back);
                console.pos += 2;
            }
            b'|' => {
                console.colour = digit(console.pos + 1);
                console.pos += 1;
            }
            _ => {
                let (row, col) = (usize::from(console.row), usize::from(console.col));
                if let (Some(chars), Some(attrs)) =
                    (console.chars.get_mut(row), console.attrs.get_mut(row))
                    && col < COLUMNS
                {
                    chars[col] = byte;
                    attrs[col] = console.colour;
                }
                console.blink = 0x10;
                console.col += 1;
                printable = true;
            }
        }
        if usize::from(console.col) >= COLUMNS {
            console.col = 0;
            console.row += 1;
        }
        if usize::from(console.row) >= ROWS {
            console.row -= 1;
            console.roll_up();
        }
        effects.push(Effect::Sound(SOUND_TYPE));
        console.pos += 1;
        if rate != 0 && printable {
            return false;
        }
    }
}

/// The hint lines `Mess_print` draws (offsets into the table at
/// `0x5339E0`), kept for the trace and the art; the English build's text
/// for them isn't drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint(pub u8);

/// What the art draws this tick.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct View {
    /// The rectangle `Wframe_set` drew, `(x0, y0, x1, y1)`.
    pub frame: Option<[i32; 4]>,
    /// `Console_trans` and `Sprite_Trans` ran: the black tile, the glyphs,
    /// the panel and the cursor.
    pub drawn: bool,
    /// The non-space glyphs, `(x, y, u, v)`, in record order.
    pub cells: Vec<[i32; 4]>,
    /// The cursor's place, when it shows.
    pub cursor: Option<[i32; 2]>,
    /// The hint lines `Mess_print` draws at the top and bottom (indices into
    /// `0x5339E0`'s table); the art doesn't draw them.
    pub hints: [Option<Hint>; 2],
}

/// Room 200's computer while game mode 7 runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Computer {
    /// `0x989E74`: the area's first byte plus its third shifted by 4
    /// (room 200: `0xF0`, mode 0 and cut 15).
    request: u8,
    /// The `Windows` task's `R0`, `R1` and `R2` (task 1's `+8` to `+0xA`).
    r0: u8,
    r1: u8,
    r2: u8,
    /// Task 1's sleep count (`+2`), as the scheduler leaves it.
    sleep: u16,
    /// `c_mode` (`0x98E52D`) and its value before this tick's inner switch
    /// (`0x689B5C`).
    c_mode: u8,
    prev_mode: u8,
    /// `0x98E52F`: 0 opens the frame, 1 closes it.
    frame_dir: u8,
    /// `pCom_work+0x2DC`, the frame's step.
    step: u8,
    /// `pCom_work+0x2DD`, the cut to come back to.
    saved_cut: u8,
    /// `0x689B50`, the cut saved for the door shots.
    door_cut: u8,
    /// `0x689B6C` and `0x689B68`.
    printed: bool,
    confirmed: bool,
    /// `0x689B64`: the card's search at state 4.
    card: bool,
    hint_top: Option<Hint>,
    hint_bottom: Option<Hint>,
    console: Console,
    view: View,
    strings: Strings,
    /// `Task_exit` has run.
    done: bool,
}

/// Hint offsets (`0x5339E0`'s entries, by index).
const HINT_TITLE: Hint = Hint(0);
const HINT_RELEASED_TOP: Hint = Hint(1);
const HINT_RELEASED: Hint = Hint(2);
const HINT_LOCKED_TOP: Hint = Hint(3);
const HINT_LOCKED: Hint = Hint(4);
const HINT_CARD_TOP: Hint = Hint(5);
const HINT_CARD: Hint = Hint(6);
const HINT_CHECKING: Hint = Hint(7);
const HINT_OK: Hint = Hint(8);

impl Computer {
    /// The computer `Sce_Windows` asked for with `request` (`0x989E74`).
    pub fn new(request: u8, strings: Strings) -> Self {
        Self {
            request,
            r0: 0,
            r1: 0,
            r2: 0,
            sleep: 1,
            c_mode: 0,
            prev_mode: 0,
            frame_dir: 0,
            step: 0,
            saved_cut: 0,
            door_cut: 0,
            printed: false,
            confirmed: false,
            card: false,
            hint_top: None,
            hint_bottom: None,
            console: Console::default(),
            view: View::default(),
            strings,
            done: false,
        }
    }

    /// Whether `Task_exit` has run: the room's task runs again.
    pub fn done(&self) -> bool {
        self.done
    }

    /// One tick of task 1: the scheduler counts its sleep down and runs
    /// `Windows` when it reaches 0.
    pub fn tick(&mut self, world: &World) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.done {
            return effects;
        }
        self.view.frame = None;
        self.view.drawn = false;
        if self.sleep > 1 {
            self.sleep -= 1;
            return effects;
        }
        let slept = self.windows(world, &mut effects);
        if !self.done && !slept {
            // Neither slept nor exited: status 1, count 1 (`0x7F`).
            self.sleep = 1;
        }
        effects
    }

    /// `Task_sleep(n)`.
    fn task_sleep(&mut self, ticks: u16) -> bool {
        self.sleep = ticks;
        true
    }

    /// `Task_signal(0); Task_exit()`.
    fn task_exit(&mut self, effects: &mut Vec<Effect>) {
        self.done = true;
        effects.push(Effect::Resume);
    }

    /// `Windows` (`0x4D1F50`). Returns whether the pass slept.
    fn windows(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        if self.r0 == 0 {
            // `Task_suspend(0); Fade_off(0)`: the engine holds the room.
            self.r0 = 1;
        } else if self.r0 != 1 {
            return false;
        }
        match self.request & 3 {
            0 => self.computer200(world, effects),
            mode => {
                if mode == 1 {
                    crate::logging::write(
                        "warning: RE2's Computer600 (room 613's terminal) isn't ported; it resumes at once",
                    );
                }
                self.r0 = 0;
                self.task_exit(effects);
                false
            }
        }
    }

    /// `Computer200` (`0x4D1170`). Returns whether the pass slept.
    fn computer200(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        match self.r1 {
            1 => {
                effects.push(Effect::Sound(SOUND_DOOR));
                self.r1 = 2;
                return self.task_sleep(DOOR_SLEEP);
            }
            2 => {
                effects.push(Effect::Cut(CUT_DOOR_2));
                self.r1 = 3;
                return self.task_sleep(DOOR_SLEEP);
            }
            3 => {
                effects.push(Effect::Sound(SOUND_DOOR));
                self.r1 = 4;
                return self.task_sleep(DOOR_SLEEP);
            }
            4 if world.scenario_b => {
                effects.push(Effect::Cut(CUT_DOOR_B));
                self.r1 = 5;
                return self.task_sleep(DOOR_SLEEP);
            }
            5 => {
                effects.push(Effect::Sound(SOUND_DOOR));
                self.r1 = 6;
                return self.task_sleep(DOOR_SLEEP);
            }
            4 | 6 => {
                effects.push(Effect::Cut(self.door_cut));
                self.c_mode = 8;
                self.tail();
                return false;
            }
            7 => {
                if self.computer_init(world, effects) {
                    return true;
                }
                self.printed = false;
                self.confirmed = false;
                self.c_mode = 1;
                return self.inner(world, effects);
            }
            8 => return self.computer_exit(effects),
            _ => {}
        }
        // `Wframe_in_out` is always 0 here: only state 10 sets its mode.
        if self.printed && world.confirm {
            self.confirmed = true;
        }
        if self.confirmed {
            self.hint_bottom = None;
            self.hint_top = None;
        }
        self.prev_mode = self.c_mode;
        self.inner(world, effects)
    }

    /// `Computer200`'s inner switch on `c_mode`, then its tail. Returns
    /// whether the pass slept.
    fn inner(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        let done = |s: &Self| s.printed && s.confirmed;
        match self.c_mode {
            0 => {
                // `pCom_work = Mem_top`, then outer case 7.
                self.r1 = 7;
                return self.computer200(world, effects);
            }
            1 => {
                self.hint_bottom = Some(HINT_TITLE);
                self.print(1, Text::Title, effects);
                if done(self) {
                    self.c_mode = 2;
                }
            }
            2 => {
                let (top, bottom, text, next) = if world.unlocked {
                    (HINT_RELEASED_TOP, HINT_RELEASED, Text::Released, 0xB)
                } else {
                    (HINT_LOCKED_TOP, HINT_LOCKED, Text::Locked, 3)
                };
                self.hint_top = Some(top);
                self.hint_bottom = Some(bottom);
                self.print(1, text, effects);
                if done(self) {
                    self.c_mode = next;
                }
            }
            3 => {
                self.hint_top = Some(HINT_CARD_TOP);
                self.hint_bottom = Some(HINT_CARD);
                self.print(1, Text::Card, effects);
                if done(self) {
                    self.c_mode = if world.unlocked { 0xB } else { 4 };
                }
            }
            4 => {
                self.card = world.card;
                if self.card {
                    effects.push(Effect::AskCard);
                }
                self.c_mode = 5;
            }
            5 => {
                if !world.message_open {
                    if !self.card {
                        self.c_mode = 0xB;
                        let mut printed = false;
                        cprint(
                            &mut self.console,
                            &self.strings,
                            0,
                            Text::Newline,
                            &mut printed,
                            effects,
                        );
                    } else if world.message_no {
                        let mut printed = false;
                        cprint(
                            &mut self.console,
                            &self.strings,
                            0,
                            Text::Newline,
                            &mut printed,
                            effects,
                        );
                        self.console.clear();
                        self.c_mode = 0xB;
                    } else {
                        effects.push(Effect::Unlock);
                        effects.push(Effect::TakeCard);
                        self.c_mode = 6;
                    }
                }
            }
            6 => {
                self.hint_bottom = Some(HINT_CHECKING);
                self.print(1, Text::Checking, effects);
                if self.printed {
                    self.c_mode = 7;
                }
            }
            7 => {
                self.door_cut = world.cut;
                if !(world.scenario_b && world.key17) {
                    effects.push(Effect::Cut(CUT_DOOR_1));
                    self.r1 = 1;
                } else {
                    effects.push(Effect::Cut(CUT_DOOR_2));
                    self.r1 = 3;
                }
                return self.task_sleep(DOOR_SLEEP);
            }
            8 => {
                self.hint_bottom = Some(HINT_OK);
                self.print(1, Text::Ok, effects);
                if done(self) {
                    self.c_mode = 0xB;
                }
            }
            0xB => {
                self.r1 = 8;
                return self.computer_exit(effects);
            }
            _ => {}
        }
        self.tail();
        false
    }

    /// `Cprint` into the console with this computer's `printed`.
    fn print(&mut self, rate: u8, text: Text, effects: &mut Vec<Effect>) {
        cprint(
            &mut self.console,
            &self.strings,
            rate,
            text,
            &mut self.printed,
            effects,
        );
    }

    /// `Computer200`'s tail (`0x4D13FB`): the reset after a confirm or a
    /// change of state, then `Console_trans(1)` and `Sprite_Trans(0x16C)`.
    fn tail(&mut self) {
        // `pCom_work+0x2D9`, the abort flag, is only written by
        // `Keyboard_Input`, which room 200 never reaches.
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

    /// `Computer_init` (`0x4D1D60`). Returns whether it slept.
    fn computer_init(&mut self, world: &World, effects: &mut Vec<Effect>) -> bool {
        if self.r2 == 0 {
            self.console.clear();
            self.console.blink = 0;
            self.step = 0;
            self.saved_cut = world.cut;
            effects.push(Effect::Cut(self.request >> 4));
            self.r2 = 1;
            return self.task_sleep(1);
        }
        if self.r2 == 1 {
            effects.push(Effect::Sound(SOUND_OPEN));
            self.frame_dir = 0;
            self.r2 = 2;
        }
        if self.wframe_move() {
            return self.task_sleep(1);
        }
        self.r2 = 0;
        false
    }

    /// `Computer_exit` (`0x4D1E90`). Returns whether it slept.
    fn computer_exit(&mut self, effects: &mut Vec<Effect>) -> bool {
        if self.r2 != 1 {
            effects.push(Effect::Sound(SOUND_CLOSE));
            self.frame_dir = 1;
            self.r2 = 1;
        }
        if self.wframe_move() {
            return self.task_sleep(1);
        }
        self.c_mode = 0;
        effects.push(Effect::Cut(self.saved_cut));
        self.r2 = 0;
        self.task_exit(effects);
        false
    }

    /// `Wframe_move` (`0x4D2690`): the frame's rectangle for this step, or
    /// `false` once it has run its 13 steps.
    fn wframe_move(&mut self) -> bool {
        let [x, y, w, h] = FRAME;
        let s = i32::from(self.step);
        if self.step > FRAME_STEPS {
            self.step = 0;
            self.frame_dir = 0;
            return false;
        }
        self.view.frame = Some(if self.frame_dir == 0 {
            [x, y, x + (w / 12) * s, y + (h / 12) * s]
        } else {
            [x, y, x + w - s * w / 12, y + h - s * h / 12]
        });
        self.step += 1;
        true
    }

    /// What the art draws this tick: the frame, and on the ticks the tail
    /// ran the console's glyphs and cursor (`Console_trans`, the sprite
    /// table at `0x533C90`).
    pub fn view(&self) -> View {
        let mut view = self.view.clone();
        view.hints = [self.hint_top, self.hint_bottom];
        if view.drawn {
            for (row, chars) in self.console.chars.iter().enumerate() {
                for (col, &c) in chars.iter().enumerate() {
                    let code = i32::from(c.wrapping_sub(0x20));
                    let (u, v) = ((code & 31) * 8, (code >> 5) * 10);
                    if u != 0 || v != 0 {
                        view.cells
                            .push([16 + 8 * col as i32, 21 + 10 * row as i32, u, v]);
                    }
                }
            }
            if self.console.cursor {
                view.cursor = Some([
                    i32::from(self.console.col) * 8 + 16,
                    i32::from(self.console.row) * 10 + 21,
                ]);
            }
        }
        view
    }

    /// The oracle's `pc` field: `R0,R1,R2,sleep,c_mode,frame_dir,step,
    /// printed,confirmed,col,row`.
    pub fn trace(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            self.r0,
            self.r1,
            self.r2,
            self.sleep,
            self.c_mode,
            self.frame_dir,
            self.step,
            u8::from(self.printed),
            u8::from(self.confirmed),
            self.console.col,
            self.console.row
        )
    }
}

/// The executable's strings, for tests.
#[cfg(test)]
pub(crate) fn test_strings() -> Strings {
    Strings {
        title: [b"DOOR LOCK SERVICE]".as_slice(), &[b'-'; 30]].concat(),
        released: b"Hall side doors:      |2RELEASED|0]".to_vec(),
        locked: b"Hall side doors:      |2LOCKED|0]".to_vec(),
        card: b"]The doors can be unlocked]by a |2CARD KEY|0.]".to_vec(),
        newline: b"]".to_vec(),
        checking: b"]Checking up ID CARD[025.[025.[025.[025.".to_vec(),
        ok: b"]OK!]Hall side doors lock released.".to_vec(),
    }
}

#[cfg(test)]
mod tests;
