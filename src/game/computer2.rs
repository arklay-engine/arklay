//! RE2's computer: game mode 7 (`Sce_Windows`, `0x4EA0B0`), the `Windows`
//! task (`0x4D1F50`), room 200's `Computer200` (`0x4D1170`), the counter's
//! DOOR LOCK SERVICE console that takes the Blue Card Key, and room 613's
//! `Computer600` (`0x4D1820`, in `terminal.rs`), the lab's staff registry with
//! its on-screen keyboard.
//!
//! The original runs it as task 1 while task 0, the room, is suspended, so
//! [`Computer::tick`] is one scheduler pass of that task: it holds through
//! the task's sleeps and returns what the pass asked of the game as
//! [`Effect`]s. The state is the original's: the task's `R0` to `R2` and
//! sleep count, `c_mode` (`0x98E52D`), the frame (`Wframe_move`) and the
//! console `Cprint` types into (`pCom_work`). It knows nothing of the game,
//! which hands it a [`World`] each tick, so it can be tested on its own.

mod terminal;

pub use terminal::KeyboardView;

/// The pack entry the converter writes the consoles' strings to, one a line
/// (`0x533AD0` and on, then `0x533C70` and on, [`Strings`]).
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
    /// Room 613's terminal (`Computer600`), from `0x533C70` down. Empty in
    /// a pack converted before it was ported, which leaves the terminal
    /// unported.
    pub terminal: TerminalStrings,
}

/// `Computer600`'s strings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalStrings {
    /// `Register your fingerprint.]`, printed at once on the second visit.
    pub register_newline: Vec<u8>,
    /// `UMBRELLA "RODEM" SYSTEM` and its version.
    pub umbrella: Vec<u8>,
    /// `Culture Experiment Room Staff Registry.`
    pub culture: Vec<u8>,
    /// `Accessing.....`
    pub accessing: Vec<u8>,
    /// `Enter your user name.]>`
    pub enter_name: Vec<u8>,
    /// `GUEST`, the one name the registry knows.
    pub guest: Vec<u8>,
    /// `This name is not in the researcher data base.`
    pub not_found: Vec<u8>,
    /// `Register your fingerprint.`
    pub register: Vec<u8>,
    /// `Please wait.....`
    pub please_wait: Vec<u8>,
    /// `Registration complete.`
    pub complete: Vec<u8>,
    /// `Guest registration is valid for 24 hours.`
    pub valid: Vec<u8>,
}

impl TerminalStrings {
    /// The strings in the order the pack holds them, one a line.
    pub fn lines(&self) -> [&[u8]; 11] {
        [
            &self.register_newline,
            &self.umbrella,
            &self.culture,
            &self.accessing,
            &self.enter_name,
            &self.guest,
            &self.not_found,
            &self.register,
            &self.please_wait,
            &self.complete,
            &self.valid,
        ]
    }

    /// The strings from [`Self::lines`]' order, or `None` short of 11.
    pub fn from_lines(mut lines: impl Iterator<Item = Vec<u8>>) -> Option<Self> {
        let mut next = || lines.next().filter(|line| !line.is_empty());
        Some(Self {
            register_newline: next()?,
            umbrella: next()?,
            culture: next()?,
            accessing: next()?,
            enter_name: next()?,
            guest: next()?,
            not_found: next()?,
            register: next()?,
            please_wait: next()?,
            complete: next()?,
            valid: next()?,
        })
    }

    fn is_empty(&self) -> bool {
        self.guest.is_empty()
    }
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
    RegisterNewline,
    Umbrella,
    Culture,
    Accessing,
    EnterName,
    NotFound,
    Register,
    PleaseWait,
    Complete,
    Valid,
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
            Text::RegisterNewline => &self.terminal.register_newline,
            Text::Umbrella => &self.terminal.umbrella,
            Text::Culture => &self.terminal.culture,
            Text::Accessing => &self.terminal.accessing,
            Text::EnterName => &self.terminal.enter_name,
            Text::NotFound => &self.terminal.not_found,
            Text::Register => &self.terminal.register,
            Text::PleaseWait => &self.terminal.please_wait,
            Text::Complete => &self.terminal.complete,
            Text::Valid => &self.terminal.valid,
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
    /// `fg_status & 0x200`: the terminal's name was taken, and the
    /// fingerprint is next (1:22).
    pub registered: bool,
    /// `Key_trg & 0x1000` and `Key_trg & 0x2000`: confirm and cancel
    /// pressed this tick.
    pub confirm_pressed: bool,
    pub cancel_pressed: bool,
    /// The directions the keyboard moves on: `0x9885FC`'s while the repeat
    /// flag (`fg_system`'s bit 31) is up, else none.
    pub repeat: u16,
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
    /// `Fade_off(0)` (`0x4C4AB0`): fade 0's level word becomes `0xFFFF`, so
    /// it is off until a later `Fade_set`. Fades 1 to 3 run on.
    FadeOff,
    /// `Task_signal(0); Task_exit()`: the room's task runs again from the
    /// next tick.
    Resume,
    /// `Pad_rep_set(0xF000, 0x20A)`: the keyboard's key repeat, for the
    /// next tick's `Pad_set` on.
    PadRepeat,
    /// `fg_status`'s `0x200` raised or lowered.
    Registered(bool),
    /// `Flg_on(fg_common, n)`: 4:n.
    Common(u8),
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

    /// `Console_put` (`0x4D29C0`): `c` at the cursor, its colour kept.
    fn put(&mut self, c: u8) {
        let (row, col) = (usize::from(self.row), usize::from(self.col));
        if let Some(cell) = self.chars.get_mut(row).and_then(|row| row.get_mut(col)) {
            *cell = c;
        }
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

/// The hint lines `Mess_print` draws, kept for the art: offsets into the
/// table at `0x5339E0` in room 200, the room's first message table
/// (`rdt + 0x3C`, the Japanese one) in room 613. The English build shows
/// neither.
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
    /// The hint lines `Mess_print` draws at the top and bottom; the art
    /// doesn't draw them.
    pub hints: [Option<Hint>; 2],
    /// `Windows`' mode: 0 room 200's console, 1 room 613's terminal, whose
    /// page has the keyboard.
    pub mode: u8,
    /// Room 613's keyboard, while its sprites are on.
    pub keyboard: Option<KeyboardView>,
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
    /// `0x98E52E`: `Wframe_in_out`'s mode, the keyboard's frame.
    kb_mode: u8,
    /// Room 613's keyboard, in `pCom_work`.
    keyboard: terminal::Keyboard,
    /// `0x98E52F`: 0 opens the frame, 1 closes it.
    frame_dir: u8,
    /// `pCom_work+0x2DC`, the frame's step.
    step: u8,
    /// `pCom_work+0x2DD`, the cut to come back to.
    saved_cut: u8,
    /// `0x689B50`, the cut saved for the door shots.
    door_cut: u8,
    /// `0x689B6C` and `0x689B68` (`0x689B78` and `0x689B74` in room 613).
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
            kb_mode: 0,
            keyboard: terminal::Keyboard::default(),
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

    /// `pCom_work+0x2D9`, the key room 613's keyboard last read, which
    /// nothing clears: a new work at the same `Mem_top` finds it, and an
    /// ESC there (1) leaves on its first pass.
    pub const KEY_OFFSET: usize = 0x2D9;

    /// The key byte the work's memory held before this computer, when it
    /// is known (0 when it isn't).
    pub fn with_key(mut self, key: Option<u8>) -> Self {
        self.keyboard.key = key.unwrap_or(0);
        self.keyboard.key_known = key.is_some();
        self
    }

    /// `printed` and `confirmed` as the last computer of this mode left
    /// them: they are globals, which only the tail and the open clear, so a
    /// confirmed last line that exits keeps them up.
    pub fn with_latches(mut self, [printed, confirmed]: [bool; 2]) -> Self {
        self.printed = printed;
        self.confirmed = confirmed;
        self
    }

    /// `printed` and `confirmed` as this computer leaves them.
    pub fn latches(&self) -> [bool; 2] {
        [self.printed, self.confirmed]
    }

    /// `Sce_Windows`' mode, 0 to 3.
    pub fn mode(&self) -> usize {
        usize::from(self.request & 3)
    }

    /// The key byte the work leaves in memory, when it is known: it was,
    /// or the keyboard read one.
    pub fn key(&self) -> Option<u8> {
        self.keyboard.key_known.then_some(self.keyboard.key)
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
            // `Task_suspend(0)`: the engine holds the room.
            effects.push(Effect::FadeOff);
            self.r0 = 1;
        } else if self.r0 != 1 {
            return false;
        }
        match self.request & 3 {
            0 => self.computer200(world, effects),
            1 if !self.strings.terminal.is_empty() => self.computer600(world, effects),
            mode => {
                if mode == 1 {
                    crate::logging::write(
                        "warning: the pack has no strings for RE2's Computer600 (room 613's terminal); it resumes at once",
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
                if self.computer_init(world, effects, true) {
                    return true;
                }
                self.printed = false;
                self.confirmed = false;
                self.c_mode = 1;
                return self.inner(world, effects);
            }
            8 => return self.computer_exit(effects, true),
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
                return self.computer_exit(effects, true);
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
        // `pCom_work+0x2D9`, the key room 613's keyboard last read: room
        // 200 writes it never, but an ESC (1) the memory holds leaves.
        if self.keyboard.key == 1 {
            self.keyboard.key = 0;
            self.c_mode = 0xB;
        }
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

    /// `Computer_init` (`0x4D1D60`), which opens the frame when `frame` is
    /// set. Returns whether it slept.
    fn computer_init(&mut self, world: &World, effects: &mut Vec<Effect>, frame: bool) -> bool {
        if self.r2 == 1 {
            if !frame {
                self.r2 = 0;
                return false;
            }
            effects.push(Effect::Sound(SOUND_OPEN));
            self.frame_dir = 0;
            self.r2 = 2;
        } else if self.r2 != 2 {
            // `Set_sp_work` copies the sprite table, every sprite off; the
            // key `Keyboard` last read (`+0x2D9`) and the typed name stay.
            self.console.clear();
            self.console.blink = 0;
            self.keyboard.init();
            self.step = 0;
            self.saved_cut = world.cut;
            effects.push(Effect::Cut(self.request >> 4));
            self.r2 = 1;
            return self.task_sleep(1);
        }
        // The pass's `Sprite_Trans` draws nothing: every sprite is off.
        if self.wframe_move(FRAME) {
            return self.task_sleep(1);
        }
        self.r2 = 0;
        false
    }

    /// `Computer_exit` (`0x4D1E90`), which closes the frame first when
    /// `frame` is set. Returns whether it slept.
    fn computer_exit(&mut self, effects: &mut Vec<Effect>, frame: bool) -> bool {
        if self.r2 != 1 && frame {
            effects.push(Effect::Sound(SOUND_CLOSE));
            self.frame_dir = 1;
            self.r2 = 1;
        }
        if self.r2 == 1 && self.wframe_move(FRAME) {
            return self.task_sleep(1);
        }
        self.c_mode = 0;
        effects.push(Effect::Cut(self.saved_cut));
        self.r2 = 0;
        self.task_exit(effects);
        false
    }

    /// `Wframe_move` (`0x4D2690`): the rectangle `[x, y, w, h]`'s frame for
    /// this step, or `false` once it has run its 13 steps.
    fn wframe_move(&mut self, [x, y, w, h]: [i32; 4]) -> bool {
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
        view.mode = self.request & 3;
        if view.drawn {
            view.keyboard = self.keyboard.view();
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
    /// printed,confirmed,col,row`, and in room 613 then the keyboard's
    /// frame mode, its column and row, the key it read and how many letters
    /// are typed.
    pub fn trace(&self) -> String {
        let base = format!(
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
        );
        if self.request & 3 == 1 {
            format!("{base},{},{}", self.kb_mode, self.keyboard.trace())
        } else {
            base
        }
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
        terminal: TerminalStrings {
            register_newline: b"Register your fingerprint.]".to_vec(),
            umbrella: b"UMBRELLA \"RODEM\" SYSTEM]                      Ver 5.0]".to_vec(),
            culture: b"Culture Experiment Room Staff Registry.]]".to_vec(),
            accessing: b"Accessing[020.[020.[020.[020.[020.]]".to_vec(),
            enter_name: b"Enter your user name.]>".to_vec(),
            guest: b"GUEST".to_vec(),
            not_found: b"|1This name is not in the]researcher data base.|0]".to_vec(),
            register: b"Register your fingerprint.".to_vec(),
            please_wait: b"Please wait[025.[025.[025.[025.[025.]".to_vec(),
            complete: b"Registration complete.]]".to_vec(),
            valid: b"Guest registration is valid]for 24 hours.]".to_vec(),
        },
    }
}

#[cfg(test)]
mod tests;
