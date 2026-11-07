//! The lab computer terminal of room 5060 (`ComputerLab.cpp`,
//! `display_computer_lab` at 0x00412390).
//!
//! The terminal is the second of the original's interactive screens: the
//! game loop keeps running underneath it (the room scripts, the camera
//! switch, the entities and the render), and `check_and_display_interactive_
//! screen` calls into it once a frame, right after the room scripts, while
//! scenario flag 0x20 is up and system flag 0x1E selects it.
//!
//! It is a first-person state: the player is parked 9000 units below the
//! floor, the room shows camera 4 (the monitor) or 5 (the keyboard), the
//! RDT's lights are swapped for a fixed set, and the two forearm props in
//! enemy slots 0 and 1 (`enemy/14`, `enemy/15`) are driven as the on-screen
//! hands through their `behavior_flags` command byte (bit 0x20 is their
//! "done" answer).
//!
//! Everything on the monitor is drawn from the `umb00`-`umb02` and
//! `umbrella` texture pages as depth-keyed scene sprites that interleave with
//! the arms' triangles, exactly like the original's ordering table. This
//! module is a transcription of the reference port's state machine: the
//! field names follow `ComputerLab.cpp`, and the comments there explain the
//! many overlaps and quirks kept here.

use crate::game::{self, GameState};
use crate::player::PlayerState;
use crate::render::{EffectBlend, EffectQuad};
use crate::state::{Image, Light, RoomState};

/// The pack entries of the terminal's texture pages, indexed by [`Page`].
pub const PAGE_ENTRIES: [&str; 5] = [
    "ui/umb00.tim",
    "ui/umb01.tim",
    "ui/umb02.tim",
    "ui/umbrella.tim",
    "ui/font.tim",
];

/// One of the terminal's texture pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Page {
    /// `umb00.tim` at tpage 7 (texture slot 0x17).
    Umb00 = 0,
    /// `umb01.tim` at tpage 9 (slot 0x18).
    Umb01 = 1,
    /// `umb02.tim` at tpage 0xB (slot 0x19).
    Umb02 = 2,
    /// `umbrella.tim` at tpage 0 (slot 0x1C).
    Umbrella = 3,
    /// `FONT.TIM`, the 8x8 debug font the masked fields print `*` through.
    Font = 4,
}

/// The game-space screen centre (`g_ScreenOffsetX/Y`, `g_SubpixelOffsetX/Y`).
const CENTRE: [i32; 2] = [160, 120];

/// The room camera that shows the monitor.
const CAMERA_MONITOR: u8 = 4;
/// The room camera that shows the keyboard.
const CAMERA_KEYBOARD: u8 = 5;
/// Bit 7 of `pending_camera`: a deferred switch is waiting.
const CAMERA_PENDING: u8 = 0x80;

/// `g_LocksFlags`: the bank the cell doors' lock bits live in.
const BANK_LOCKS: u8 = 2;
/// The lab map's room-flag bit (`ROOM_FLAG_MAP_BASE + MAP_INDEX_LABORATORY`).
const LAB_MAP_FLAG: u8 = 0x81;
/// Scenario flag 0x20: an interactive screen is up.
const SCENARIO_INTERACTIVE: u8 = 0x20;
/// System flag 0x1E selects the lab computer.
const SYSTEM_COMPUTER: u8 = 0x1E;

/// The three lights the terminal installs over the room's own
/// (`s_labLights`, 0x004b3f20).
const LAB_LIGHTS: [([i32; 3], u16); 3] = [
    ([14300, -1950, 9650], 6735),
    ([4450, -2650, 12000], 7055),
    ([14700, -1600, 5600], 7495),
];
/// The ambient the terminal installs.
const LAB_AMBIENT: i16 = 0x708;

// ---- const tables -------------------------------------------------------

/// One row of art inside a window: `width << 3` by `height << 4` pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    tex_u: u8,
    tex_v: u8,
    width: i16,
    height: i16,
    page: i16,
}

const fn row(tex_u: u8, tex_v: u8, width: i16, height: i16, page: i16) -> Row {
    Row {
        tex_u,
        tex_v,
        width,
        height,
        page,
    }
}

/// The login window (0x004b3f60); row 2's width is patched to reveal the
/// password line, see [`Terminal::login_row2_width`].
const ROWS_LOGIN: [Row; 3] = [
    row(0x00, 0x90, 32, 2, 9),
    row(0x00, 0xe0, 7, 1, 7),
    row(0x00, 0xf0, 0, 1, 7),
];
const ROWS_PANEL_A: [Row; 3] = [
    row(0x00, 0xb0, 32, 1, 9),
    row(0x00, 0x00, 0, 0, 7),
    row(0x60, 0xf0, 13, 1, 7),
];
const ROWS_SIDE: [Row; 3] = [
    row(0x38, 0xe0, 3, 1, 7),
    row(0x50, 0xe0, 3, 1, 7),
    row(0xc8, 0xe0, 7, 1, 7),
];
const ROWS_MENU: [Row; 4] = [
    row(0xc8, 0xf0, 6, 1, 7),
    row(0x00, 0x00, 0, 0, 7),
    row(0xe0, 0x68, 4, 1, 7),
    row(0xe0, 0x78, 4, 1, 7),
];
const ROWS_PANEL_B: [Row; 4] = [
    row(0x00, 0xc0, 21, 1, 9),
    row(0x00, 0xd0, 21, 1, 9),
    row(0x00, 0x00, 0, 0, 7),
    row(0x00, 0xf0, 12, 1, 7),
];

/// Which row list a window points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Rows {
    #[default]
    Login,
    PanelA,
    Side,
    Menu,
    PanelB,
}

/// The expected inputs, as virtual-keyboard codes (1 = A): JOHN, ADA, MOLE.
const EXPECT_USER: [u8; 4] = [10, 15, 8, 14];
const EXPECT_PASS: [u8; 3] = [1, 4, 1];
const EXPECT_CMD: [u8; 4] = [13, 15, 12, 5];

/// Key code to forearm and reach (0x004b40a8): bit 7 picks the left arm
/// (enemy slot 1), the low bits the reach command.
const ARM_FOR_KEY: [u8; 32] = [
    0x83, 0x82, 0x81, 0x01, 0x02, 0x02, 0x02, 0x03, //
    0x83, 0x82, 0x81, 0x01, 0x01, 0x02, 0x02, 0x03, //
    0x83, 0x82, 0x81, 0x01, 0x01, 0x01, 0x02, 0x03, //
    0x83, 0x82, 0x81, 0x01, 0x01, 0x01, 0x03, 0x03,
];

/// A drawable quad of `cl_draw_quads`: `flags` bit 0 tiles `repeat` down
/// instead of across, bits 6-7 are the mirror bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Quad {
    x: i16,
    y: i16,
    tex_u: u8,
    tex_v: u8,
    width: i16,
    height: i16,
    flags: u8,
    repeat: u8,
}

#[allow(clippy::too_many_arguments)]
const fn quad(
    x: i16,
    y: i16,
    tex_u: u8,
    tex_v: u8,
    width: i16,
    height: i16,
    flags: u8,
    repeat: u8,
) -> Quad {
    Quad {
        x,
        y,
        tex_u,
        tex_v,
        width,
        height,
        flags,
        repeat,
    }
}

/// The 9-slice window frame (0x004b40c8); six fields are rewritten from the
/// window's size before each draw.
const FRAME_QUADS: [Quad; 8] = [
    quad(0, 0, 0x50, 0x20, 16, 16, 0x00, 0),
    quad(0, 0, 0x70, 0x20, 16, 16, 0x00, 0),
    quad(0, 0, 0x50, 0x40, 16, 16, 0x00, 0),
    quad(0, 0, 0x70, 0x40, 16, 16, 0x00, 0),
    quad(16, 0, 0x60, 0x20, 16, 16, 0x00, 0),
    quad(16, 0, 0x60, 0x40, 16, 16, 0x00, 0),
    quad(0, 16, 0x50, 0x30, 16, 16, 0x01, 0),
    quad(0, 16, 0x70, 0x30, 16, 16, 0x01, 0),
];

/// The virtual keyboard's frame (0x004b4128).
const KEYBOARD_FRAME: [Quad; 10] = [
    quad(16, 16, 0x10, 0x20, 192, 96, 0x00, 0),
    quad(8, 8, 0x00, 0x48, 16, 8, 0x00, 12),
    quad(200, 8, 0x00, 0x48, 8, 8, 0x00, 0),
    quad(8, 16, 0x08, 0x50, 8, 16, 0x00, 0),
    quad(8, 32, 0x08, 0x60, 8, 16, 0x01, 5),
    quad(8, 112, 0x08, 0x70, 8, 8, 0x00, 0),
    quad(16, 112, 0x20, 0x80, 16, 8, 0x00, 11),
    quad(192, 112, 0x30, 0x80, 24, 8, 0x00, 0),
    quad(208, 8, 0x50, 0x80, 8, 16, 0x01, 6),
    quad(208, 104, 0x50, 0x80, 8, 8, 0x00, 0),
];

/// The full-screen monitor pictures by `msgFlags & 3` (0x004b4194); index 0
/// is the never-drawn overlap slot.
const MSG_PICTURES: [Quad; 4] = [
    quad(0, 0, 0x00, 0x00, 0, 0, 0x00, 0),
    quad(0, 0, 0x00, 0x00, 120, 104, 0x00, 0),
    quad(0, 0, 0x78, 0x00, 120, 104, 0x00, 0),
    quad(0, 0, 0x00, 0x68, 120, 104, 0x00, 0),
];

/// The status banners by `statusIdx` (0x004b41bc); index 4 is two quads.
const STATUS_BANNERS: [Quad; 6] = [
    quad(0, 0, 0x00, 0x00, 0, 0, 0x00, 0),
    quad(0, 0, 0x78, 0x68, 100, 32, 0x00, 0),
    quad(0, 0, 0x78, 0x88, 100, 32, 0x00, 0),
    quad(0, 0, 0x78, 0xa8, 100, 32, 0x00, 0),
    quad(0, 0, 0x78, 0xc8, 100, 24, 0x00, 0),
    quad(0, 24, 0x78, 0xa0, 100, 8, 0x00, 0),
];

/// `msgFlags & 0x10` (0x004b4220).
const STRIP_A: [Quad; 2] = [
    quad(12, 29, 0x84, 0x17, 42, 3, 0x00, 0),
    quad(12, 32, 0x84, 0x14, 20, 3, 0x00, 0),
];
/// `msgFlags & 0x20` (0x004b4238).
const STRIP_B: [Quad; 3] = [
    quad(12, 39, 0x84, 0x17, 30, 3, 0x00, 0),
    quad(12, 42, 0x8a, 0x17, 30, 3, 0x00, 0),
    quad(12, 47, 0x84, 0x14, 16, 3, 0x00, 0),
];

/// The "door released" marker origins and quads, one per door.
const DOOR_ORIGIN: [[u8; 2]; 2] = [[50, 20], [28, 8]];
const DOOR_MARKER: [Quad; 2] = [
    quad(0, 8, 0x68, 0xe0, 20, 8, 0x01, 3),
    quad(12, 4, 0x68, 0xe0, 12, 12, 0x00, 0),
];

/// The two Umbrella-mark wedge quads (0x004c4420), in ring order:
/// `(x, y, u, v)` per corner, +y up, origin at the wedge's base.
const WEDGE_QUADS: [[[i16; 4]; 4]; 2] = [
    [
        [-16, 40, 32, 24],
        [16, 40, 64, 24],
        [16, 0, 64, 64],
        [-16, 0, 32, 64],
    ],
    [
        [-16, 40, 0, 24],
        [16, 40, 32, 24],
        [16, 0, 32, 64],
        [-16, 0, 0, 64],
    ],
];

// ---- state ---------------------------------------------------------------

/// The shared `g_TextureDesc` every draw goes through. It is one global in
/// the original, and several draws rely on what an earlier one left in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Desc {
    page: i16,
    clut_y: i16,
    pivot_x: i16,
    pivot_y: i16,
    screen_x: i16,
    screen_y: i16,
    tex_u: u8,
    tex_v: u8,
    width: u16,
    height: u16,
    flags: u32,
    colour: [u8; 3],
    scale_x: i16,
    scale_y: i16,
}

impl Default for Desc {
    fn default() -> Self {
        Self {
            page: 0,
            clut_y: 0,
            pivot_x: 0,
            pivot_y: 0,
            screen_x: 0,
            screen_y: 0,
            tex_u: 0,
            tex_v: 0,
            width: 0,
            height: 0,
            flags: 0x0100_0000,
            colour: [0x80; 3],
            scale_x: 0x1000,
            scale_y: 0x1000,
        }
    }
}

/// A terminal window (frame plus rows of art). Window 3 has no geometry:
/// its `enabled` bit 7 is the keyboard's "cursor moved" flag and its
/// `row_count` is the "cancelled" flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Window {
    enabled: u8,
    row_count: u8,
    x: i16,
    y: i16,
    cols: i16,
    rows: i16,
    row_data: Rows,
    anim_cmd: u8,
    anim_state: u8,
    alpha: i16,
    visible: i16,
    big: i16,
}

/// A text-entry field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Text {
    enabled: u8,
    win: u8,
    x: i16,
    y: i16,
    text: [u8; 9],
    masked: u8,
}

impl Text {
    /// The byte at `index` of the field record from its text on: past the
    /// nine text bytes the original reads on into the `masked` byte.
    fn byte(&self, index: usize) -> u8 {
        match index {
            0..=8 => self.text[index],
            9 => self.masked,
            _ => 0,
        }
    }

    /// The field as `strcmp` sees it, up to its first zero.
    fn word(&self) -> &[u8] {
        let end = self.text.iter().position(|&c| c == 0).unwrap_or(9);
        &self.text[..end]
    }
}

/// The "hands type what is on screen" animator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Typer {
    active: u8,
    state: u8,
    char_idx: i32,
    timer: i16,
    line: i16,
}

/// One slot of the Umbrella boot-logo animation (0x00ac3698).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LogoSlot {
    active: u8,
    behaviour: u8,
    sub: u8,
    x: i32,
    y: i32,
    counter: i16,
    phase: i16,
    parent: Option<usize>,
    tex_u: u8,
    tex_v: u8,
    width: i16,
    height: i16,
    depth_base: i16,
    pivot_x: i16,
    pivot_y: i16,
    scale_x: i16,
    scale_y: i16,
    angle: i32,
}

const LOGO_SLOTS: usize = 9;

/// The room state `computer_lab_init` swaps out and `computer_lab_finish`
/// restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Saved {
    camera: usize,
    player_y: i32,
    lights: [Light; 3],
    ambient: [i16; 3],
}

/// The pad words the terminal reads this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pad {
    /// `g_PlayerDpadPressed >> 8`: 0x40 confirm, 0x80 cancel (edges).
    pub dpad_pressed: u8,
    /// `g_PlayerDpadHeld >> 8`.
    pub dpad_held: u8,
    /// `g_PlayerPadHeld >> 8`, the raw pad's rising edges: 0x10 up, 0x40
    /// down.
    pub pad_edge: u8,
    /// `g_button_pressed_id & 0xF000`: the raw held D-pad (0x1000 up, 0x2000
    /// right, 0x4000 down, 0x8000 left).
    pub raw_held: u16,
    /// `g_PlayerPadHeld & 0xF000`: the raw D-pad's rising edges.
    pub raw_edge: u16,
}

impl Pad {
    /// The terminal's pad words from the arklay remapped D-pad words
    /// ([`game::dpad_word`]: up 1, down 2, left 4, right 8, run/cancel 0x10,
    /// confirm 0x80).
    pub fn from_dpad(held: u16, pressed: u16) -> Self {
        let remapped = |word: u16| {
            let mut byte = 0u8;
            if word & 0x0080 != 0 {
                byte |= 0x40;
            }
            if word & 0x0010 != 0 {
                byte |= 0x80;
            }
            byte
        };
        let raw = |word: u16| {
            let mut bits = 0u16;
            if word & 0x0001 != 0 {
                bits |= 0x1000;
            }
            if word & 0x0008 != 0 {
                bits |= 0x2000;
            }
            if word & 0x0002 != 0 {
                bits |= 0x4000;
            }
            if word & 0x0004 != 0 {
                bits |= 0x8000;
            }
            bits
        };
        let raw_edge = raw(pressed);
        Self {
            dpad_pressed: remapped(pressed),
            dpad_held: remapped(held),
            pad_edge: (raw_edge >> 8) as u8,
            raw_held: raw(held),
            raw_edge,
        }
    }
}

/// The terminal's whole state: the shared interactive-screen block, the
/// `.data` tables it mutates and the statics that persist across visits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Terminal {
    /// The cancel key's held level last frame, for its press edge.
    cancel_prev: bool,
    /// `g_labSlidesState`: the gate's value after the previous dispatch.
    was_up: bool,
    func: u8,
    sub: u8,
    sub2: u8,
    sub3: u8,
    col: i32,
    row: i32,
    timer_a: i16,
    timer_b: i16,
    saved: Option<Saved>,

    win: [Window; 4],
    text: [Text; 2],
    typer: Typer,
    login_row2_width: i16,

    input_enabled: u8,
    caret_win: u8,
    caret_base_x: i16,
    caret_base_y: i16,
    caret_mode: u8,
    caret_pos: u16,
    text_rec: u8,
    blink_counter: u8,
    blink_hold: u8,
    led_flags: u8,
    msg_flags: u8,
    status_idx: u8,
    door_sel: u8,
    col_r: u8,
    col_g: u8,
    col_b: u8,
    pad_repeat_latch: u16,
    pad_repeat_timer: u8,
    pending_camera: u8,
    pending_cam_delay: u8,
    simple_mode: u8,
    intro_logo_state: u8,
    logo: [LogoSlot; LOGO_SLOTS],

    /// Which texture slots hold a page: bit per [`Page`].
    loaded: u8,
    desc: Desc,
    pad: Pad,
    /// This frame's sprites, in submission order.
    sprites: Vec<EffectQuad>,
}

/// What a tick needs from the session.
pub struct Host<'a> {
    pub game: &'a mut GameState,
    pub room: &'a mut RoomState,
    pub player: &'a mut PlayerState,
    pub pad: Pad,
}

impl Terminal {
    /// This frame's pad from the game's remapped D-pad words, with the
    /// cancel key folded into the run bit (`0x10`): the original reads one
    /// raw bit for both. Call once per frame, terminal up or not, so the
    /// cancel edge stays current.
    pub fn pad_from_dpad(&mut self, held: u16, pressed: u16, cancel_held: bool) -> Pad {
        let cancel = if cancel_held { 0x10 } else { 0 };
        let cancel_edge = if cancel_held && !self.cancel_prev {
            0x10
        } else {
            0
        };
        self.cancel_prev = cancel_held;
        Pad::from_dpad(held | cancel, pressed | cancel_edge)
    }

    /// `lab_slides_reset` (0x0042a020), which `room_set` runs: a room load
    /// drops a terminal left mid-sequence, so the next visit starts over at
    /// `computer_lab_init` (the room's own textures replace the pages).
    pub fn room_reset(&mut self) {
        self.func = 0;
        self.was_up = false;
        self.free_textures();
        self.sprites.clear();
    }

    /// This frame's sprites. Their `page` indexes [`PAGE_ENTRIES`].
    pub fn sprites(&self) -> &[EffectQuad] {
        &self.sprites
    }

    /// `check_and_display_interactive_screen` (0x0042a030) for the
    /// computer: one frame of the terminal while the gate is up.
    pub fn tick(&mut self, host: &mut Host<'_>) {
        self.sprites.clear();
        let up = host
            .game
            .flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false);
        if up {
            if !self.was_up {
                self.func = 0;
                self.sub = 0;
                self.sub2 = 0;
                self.sub3 = 0;
            }
            if host
                .game
                .flag_test(game::BANK_SYSTEM, SYSTEM_COMPUTER, false)
            {
                self.pad = host.pad;
                self.display(host);
            }
        }
        self.was_up = host
            .game
            .flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false);
    }

    /// `display_computer_lab` (0x00412390).
    fn display(&mut self, host: &mut Host<'_>) {
        match self.func {
            0 => self.init(host),
            1 => self.update(host),
            2 => self.finish(host),
            _ => {}
        }
        self.draw_caret();
        self.draw_text();
        self.draw_windows();
        self.draw_keyboard();
        self.draw_door_picture();
        self.draw_status();
        self.draw_leds(host.game);
        self.camera_step(host);
    }

    fn set_state_word(&mut self, packed: u32) {
        self.func = packed as u8;
        self.sub = (packed >> 8) as u8;
        self.sub2 = (packed >> 16) as u8;
        self.sub3 = (packed >> 24) as u8;
    }

    // ---- host helpers ----------------------------------------------------

    /// `g_EnemiesList[slot].behavior_flags`.
    fn arm(host: &Host<'_>, slot: usize) -> u8 {
        host.game.entities[slot + 1].behavior_flags
    }

    fn set_arm(host: &mut Host<'_>, slot: usize, value: u8) {
        host.game.entities[slot + 1].behavior_flags = value;
    }

    fn is_jill(host: &Host<'_>) -> bool {
        host.game.id.player_flag & 1 != 0
    }

    fn rand(host: &mut Host<'_>) -> i32 {
        i32::from(game::platform_rand(&mut host.game.rand_state))
    }

    /// `play_sfx(2, id)`: a room-bank sound with no position.
    fn sfx(host: &mut Host<'_>, id: u8) {
        host.game.snd3d_requests.push(game::Snd3dRequest {
            bank: 2,
            id,
            volume: 0,
            pos: game::Snd3dPos::None,
        });
    }

    /// `g_roomCameraId = camera; check_camera_switch(1)`: the forced cut.
    /// The zone pointer only follows a camera that has a switch-zone group;
    /// the monitor cameras have none, so it stays on the group it was on.
    fn switch_camera(host: &mut Host<'_>, camera: usize) {
        if camera >= host.room.cuts.len() {
            return;
        }
        let has_group = host
            .room
            .zones
            .iter()
            .any(|zone| zone.cam_from >= 0 && zone.cam_from as usize == camera);
        host.room.zone_group = if has_group {
            None
        } else {
            Some(host.room.zone_group.unwrap_or(host.room.current_cut))
        };
        host.game.set_camera_cut(camera);
        host.room.current_cut = camera;
    }

    // ---- pad --------------------------------------------------------------

    /// `cl_keyboard_input` (0x00414010): the 8x4 key grid. Returns -2 EXIT,
    /// -1 ENTER, -3 BACKSPACE, a key code, or 0 when nothing was committed.
    fn keyboard_input(&mut self) -> i16 {
        self.win[3].enabled &= 0x7f;
        let mut cell = (self.row * 8 + self.col) as i16;
        let mut result = cell;
        if self.pad.dpad_pressed & 0x40 != 0 {
            if cell == 0 {
                result = -2;
            } else if cell == 0x17 {
                result = -1;
            } else if cell == 0x1d {
                result = -3;
            } else if cell > 0x16 {
                result = cell - 1;
            }
            self.win[3].enabled |= 0x80;
            return result;
        }
        let dpad = self.pad.raw_held & 0xf000;
        if dpad == self.pad_repeat_latch && self.pad_repeat_latch != 0 {
            if self.pad_repeat_timer != 0 {
                self.pad_repeat_timer -= 1;
                return 0;
            }
            self.pad_repeat_timer = 0xf;
        } else {
            self.pad_repeat_timer = 8;
        }
        self.pad_repeat_latch = self.pad.raw_edge & 0xf000;
        let (old_row, old_col) = (self.row, self.col);
        if dpad & 0x1000 != 0 && self.row > 0 {
            self.row -= 1;
        }
        if dpad & 0x4000 != 0 && self.row < 3 {
            self.row += 1;
        }
        if dpad & 0x8000 != 0 {
            self.col -= 1;
            if self.col < 0 {
                self.col = 7;
            }
        }
        if dpad & 0x2000 != 0 {
            self.col += 1;
            if cell == 0x1d {
                self.col += 1;
            }
            if self.col > 7 {
                self.col = 0;
            }
        }
        cell = (self.col + self.row * 8) as i16;
        if cell == 0x1f {
            self.row -= 1;
        }
        if cell == 0x1e {
            self.col -= 1;
        }
        if self.col != old_col || self.row != old_row {
            self.win[3].enabled |= 0x80;
        }
        0
    }

    // ---- fields -------------------------------------------------------------

    /// `cl_text_begin` (0x00414230).
    fn text_begin(&mut self, rec: u8) {
        let text = &mut self.text[usize::from(rec)];
        text.text = [0; 9];
        self.caret_win = text.win;
        self.caret_pos = 0;
        self.blink_counter = 0;
        self.text_rec = rec;
        self.caret_base_x = text.x;
        self.caret_base_y = text.y;
    }

    /// `cl_text_append` (0x004141b0): the caret stops at 7, so the eighth
    /// character is overwritten in place.
    fn text_append(&mut self, key: i16, rec: usize) {
        if key == -1 || key == -2 {
            return;
        }
        let pos = usize::from(self.caret_pos);
        let buffer = &mut self.text[rec].text;
        if key == -3 {
            if self.caret_pos != 0 {
                // At 7 the caret only steps back once the eighth cell is empty.
                if self.caret_pos < 7 || buffer[pos] == 0 {
                    self.caret_pos -= 1;
                }
            }
            buffer[usize::from(self.caret_pos)] = 0;
            return;
        }
        buffer[pos] = key as u8;
        if self.caret_pos < 7 {
            self.caret_pos += 1;
        }
    }

    // ---- fade and camera ------------------------------------------------------

    /// `cl_fade_step` (0x00415410): ramp the colour multiplier down, install
    /// `msg_on_fade_out` at the darkest point, then ramp it back up.
    fn fade_step(&mut self, step: u8, msg_on_fade_out: u8) -> bool {
        if self.timer_a == 2 {
            return true;
        }
        if self.timer_b == 0 {
            self.col_r = self.col_r.wrapping_sub(step);
            self.col_g = self.col_g.wrapping_sub(step);
            self.col_b = self.col_b.wrapping_sub(step);
            if self.col_r == 0 || self.col_g == 0 || self.col_b == 0 {
                self.col_r = 0;
                self.col_g = 0;
                self.col_b = 0;
                self.msg_flags = msg_on_fade_out;
                self.timer_b = self.timer_b.wrapping_add(1);
                return false;
            }
        } else {
            self.col_r = self.col_r.wrapping_add(step);
            self.col_g = self.col_g.wrapping_add(step);
            self.col_b = self.col_b.wrapping_add(step);
            if self.col_r >= 0x80 || self.col_g >= 0x80 || self.col_b >= 0x80 {
                self.col_r = 0x80;
                self.col_g = 0x80;
                self.col_b = 0x80;
                self.timer_b = self.timer_b.wrapping_add(1);
                return true;
            }
        }
        false
    }

    /// `cl_camera_step` (0x004154d0): the deferred camera switch.
    fn camera_step(&mut self, host: &mut Host<'_>) {
        if self.pending_camera & CAMERA_PENDING == 0 {
            return;
        }
        if self.pending_cam_delay != 0 {
            self.pending_cam_delay -= 1;
            return;
        }
        Self::switch_camera(host, usize::from(self.pending_camera & !CAMERA_PENDING));
        self.pending_cam_delay = 0;
        self.pending_camera &= !CAMERA_PENDING;
    }

    // ---- textures ----------------------------------------------------------------

    fn load_textures(&mut self) {
        self.loaded |=
            (1 << Page::Umb00 as u8) | (1 << Page::Umb01 as u8) | (1 << Page::Umb02 as u8);
    }

    fn load_intro_texture(&mut self) {
        self.loaded &= !(1 << Page::Umb02 as u8);
        self.loaded |= 1 << Page::Umbrella as u8;
    }

    fn free_textures(&mut self) {
        self.loaded &= 1 << Page::Font as u8;
    }

    // ---- the Umbrella boot logo (0x00488a60) -------------------------------------

    fn logo_alloc(&mut self) -> Option<usize> {
        let index = self.logo.iter().position(|slot| slot.active == 0)?;
        self.logo[index].active = 1;
        self.logo[index].sub = 0;
        Some(index)
    }

    fn logo_spawn_wedge(&mut self, parent: usize, behaviour: u8) {
        let Some(child) = self.logo_alloc() else {
            return;
        };
        let source = self.logo[parent];
        let slot = &mut self.logo[child];
        slot.behaviour = behaviour;
        slot.x = source.x;
        slot.y = source.y;
        slot.angle = source.angle;
        slot.parent = Some(parent);
    }

    fn logo_spin(slot: &mut LogoSlot) {
        slot.angle += 15;
        if slot.angle == 0x168 {
            slot.angle = 0;
        }
    }

    /// The shared fly-out (0x00489030).
    fn logo_move_out(slot: &mut LogoSlot) {
        if slot.x <= 0xffd6_0000_u32 as i32 {
            slot.sub += 1;
            slot.x = 0xffd6_0000_u32 as i32;
            return;
        }
        slot.x = slot.x.wrapping_sub(0x30000);
        slot.scale_x = slot.scale_x.wrapping_sub(0xd0);
        slot.scale_y = slot.scale_y.wrapping_sub(0xd0);
    }

    fn logo_parent(&self, index: usize) -> Option<LogoSlot> {
        self.logo[index].parent.map(|parent| self.logo[parent])
    }

    fn logo_wedge_init(slot: &mut LogoSlot, tex_u: u8) {
        slot.sub += 1;
        slot.tex_u = tex_u;
        slot.tex_v = 0x18;
        slot.width = 0x20;
        slot.height = 0x28;
        slot.pivot_x = 0x10;
        slot.pivot_y = 0x28;
        slot.scale_x = 0x1000;
        slot.scale_y = 0x1000;
        slot.depth_base = 5;
    }

    fn logo_hub_spawn(&mut self, index: usize) {
        if self.logo[index].angle % 0x5a != 0 {
            return;
        }
        let slot = &mut self.logo[index];
        slot.counter = slot.counter.wrapping_sub(1);
        if slot.counter == 0 {
            slot.sub += 1;
            slot.counter = 3;
            slot.phase = 0;
            self.logo_spawn_wedge(index, 2);
            return;
        }
        self.logo_spawn_wedge(index, 1);
    }

    fn logo_hub(&mut self, index: usize) {
        match self.logo[index].sub {
            0 => {
                let slot = &mut self.logo[index];
                slot.sub += 1;
                slot.tex_u = 0x20;
                slot.tex_v = 0x18;
                slot.width = 0x20;
                slot.height = 0x28;
                slot.pivot_x = 0x10;
                slot.pivot_y = 0x28;
                slot.scale_x = 0x1000;
                slot.scale_y = 0x1000;
                slot.angle = 0;
                slot.depth_base = 5;
                slot.counter = 3;
            }
            1 => {
                let slot = &mut self.logo[index];
                if slot.angle == 0 {
                    slot.counter = slot.counter.wrapping_sub(1);
                    if slot.counter == 0 {
                        slot.sub += 1;
                        slot.counter = 4;
                        self.logo_hub_spawn(index);
                        if let Some(word) = self.logo_alloc() {
                            self.logo[word].behaviour = 3;
                            self.logo[word].parent = Some(index);
                        }
                    }
                }
            }
            2 => self.logo_hub_spawn(index),
            3 => {
                let slot = &mut self.logo[index];
                if slot.angle == 0 {
                    slot.counter = slot.counter.wrapping_sub(1);
                    if slot.counter == 0 {
                        slot.sub += 1;
                        slot.phase = 1;
                    }
                }
            }
            4 => Self::logo_move_out(&mut self.logo[index]),
            5 => {
                let slot = &mut self.logo[index];
                if (slot.angle + 15) % 0x5a == 0 {
                    slot.phase = 2;
                    slot.active = 0;
                    slot.sub = 0;
                }
            }
            _ => {}
        }
        Self::logo_spin(&mut self.logo[index]);
    }

    /// Behaviours 1 and 2: the wedges; behaviour 1 also seeds a follower once
    /// the hub reaches 270 degrees.
    fn logo_wedge(&mut self, index: usize, seeds: bool) {
        let parent = self.logo_parent(index);
        let sub = self.logo[index].sub;
        match sub {
            0 => Self::logo_wedge_init(&mut self.logo[index], if seeds { 0x20 } else { 0 }),
            1 => {
                if seeds {
                    if parent.is_some_and(|p| p.angle == 0x10e) {
                        self.logo[index].sub += 1;
                        self.logo[index].phase = 0;
                        self.logo_spawn_wedge(index, 2);
                    }
                } else if parent.is_some_and(|p| self.logo[index].angle - p.angle == -0x2d) {
                    self.logo[index].sub += 1;
                }
            }
            2 => {
                let slot = &mut self.logo[index];
                Self::logo_spin(slot);
                if parent.is_some_and(|p| p.phase == 1) {
                    slot.sub += 1;
                    if seeds {
                        slot.phase = 1;
                    }
                }
            }
            3 => {
                let slot = &mut self.logo[index];
                Self::logo_spin(slot);
                Self::logo_move_out(slot);
            }
            _ => {
                let slot = &mut self.logo[index];
                Self::logo_spin(slot);
                if parent.is_some_and(|p| p.phase == 2) {
                    slot.phase = 2;
                    slot.active = 0;
                    slot.sub = 0;
                }
            }
        }
    }

    /// Behaviour 3: the UMBRELLA wordmark, which ends the sequence.
    fn logo_wordmark(&mut self, index: usize, voice_playing: bool) {
        let parent = self.logo_parent(index);
        let slot = &mut self.logo[index];
        match slot.sub {
            0 => {
                slot.sub += 1;
                slot.tex_u = 0x18;
                slot.tex_v = 0;
                slot.width = 0;
                slot.height = 0x18;
                slot.pivot_x = 0;
                slot.pivot_y = 0;
                slot.scale_x = 0x1000;
                slot.scale_y = 0x1000;
                slot.x = 0x3b_0000;
                slot.y = 0xffc9_0000_u32 as i32;
                slot.angle = 0;
                slot.depth_base = 4;
            }
            1 => {
                slot.x = slot.x.wrapping_sub(0x12000);
                let xi = (slot.x >> 16) as i16;
                slot.width = 0x3b - xi;
                if xi < -0x1e {
                    slot.sub += 1;
                    slot.x = 0xffe2_0000_u32 as i32;
                }
            }
            2 => {
                if parent.is_some_and(|p| p.phase == 2) {
                    slot.sub += 1;
                    slot.x = 0xffca_0000_u32 as i32;
                    slot.width = 0x70;
                    slot.tex_u = 0;
                    slot.counter = 0x5a;
                }
            }
            3 => {
                if slot.counter > 0 {
                    slot.counter -= 1;
                }
                if slot.counter < 1 && !voice_playing {
                    slot.sub += 1;
                    slot.x = 0x20000;
                    slot.y = slot.y.wrapping_add(0xc0000);
                    slot.width = 0x70;
                    slot.tex_u = 0;
                    slot.pivot_x = 0x38;
                    slot.pivot_y = 0x0c;
                    slot.scale_x = 0x1000;
                    slot.scale_y = 0x1000;
                }
            }
            _ => {
                if slot.scale_x < 0xaab {
                    slot.scale_x = 0xaaa;
                    slot.scale_y = 0xaaa;
                } else {
                    slot.scale_x -= 0x32;
                    slot.scale_y -= 0x32;
                }
                slot.y = slot.y.wrapping_add(0x10000);
                if (slot.y as u32 & 0xffff_0000) as i32 > -0x120001 {
                    slot.y = 0xffee_0000_u32 as i32;
                    slot.active = 0;
                    self.intro_logo_state = 2;
                }
            }
        }
    }

    /// `cl_intro_logo_update` (0x00488a60).
    fn intro_logo_update(&mut self, host: &mut Host<'_>) {
        if self.intro_logo_state == 0 {
            for slot in &mut self.logo {
                slot.active = 0;
                slot.sub = 0;
            }
            if let Some(hub) = self.logo_alloc() {
                self.logo[hub].behaviour = 0;
                self.logo[hub].x = 0;
                self.logo[hub].y = 0xffd5_0000_u32 as i32;
            }
            let row = host.game.id.table_row() as u8;
            host.game.voice.request = Some(game::VoiceRequest {
                path: crate::voice::pack_path(row, 0xb7),
                volume: 0,
                pan: 0,
            });
            self.intro_logo_state += 1;
            host.game.set_voice_playing();
        }
        for index in 0..LOGO_SLOTS {
            if self.logo[index].active == 0 {
                continue;
            }
            match self.logo[index].behaviour {
                0 => self.logo_hub(index),
                1 => self.logo_wedge(index, true),
                2 => self.logo_wedge(index, false),
                3 => {
                    let voice = host.game.voice_playing();
                    self.logo_wordmark(index, voice);
                }
                _ => {}
            }
        }
        self.logo_draw(host);
    }

    /// The logo's draw pass: the wordmark is a scaled sprite off umb02, the
    /// wedges rotated quads.
    fn logo_draw(&mut self, host: &Host<'_>) {
        self.desc.clut_y = 0;
        self.desc.flags = 0x0100_0040;
        let fov = host
            .room
            .cuts
            .get(host.room.current_cut)
            .map_or(589, |cut| cut.fov);
        for index in 0..LOGO_SLOTS {
            let slot = self.logo[index];
            if slot.active == 0 {
                continue;
            }
            if slot.behaviour != 3 {
                self.logo_draw_wedge(&slot, usize::from(slot.behaviour == 2), fov);
                continue;
            }
            let desc = &mut self.desc;
            desc.page = 0x0b;
            desc.clut_y = 0x1ed;
            desc.pivot_x = slot.pivot_x;
            desc.pivot_y = slot.pivot_y;
            desc.tex_u = slot.tex_u;
            desc.tex_v = slot.tex_v;
            desc.width = slot.width as u16;
            desc.height = slot.height as u16;
            desc.scale_x = slot.scale_x;
            desc.scale_y = slot.scale_y;
            desc.screen_x = (slot.x >> 16) as i16;
            desc.screen_y = (slot.y >> 16) as i16;
            desc.colour = [0x80; 3];
            self.add_sprite(10, 0x19, 1);
        }
    }

    /// One wedge: `v' = 2 * (q rotated about Z) + (px, py, pz)`, projected
    /// through the monitor camera's distance.
    fn logo_draw_wedge(&mut self, slot: &LogoSlot, quad: usize, fov: i32) {
        if self.loaded & (1 << Page::Umb02 as u8) == 0 {
            return;
        }
        let sc = i32::from(slot.scale_x);
        let px = ((sc - 0x1000) / 0x15) as f32;
        let py = ((0x1000 - sc) / 0x1a + 0x50) as f32;
        let pz = ((0x1000 - sc) / 2 + 0x4b0) as f32;
        if pz <= 1.0 {
            return;
        }
        let turns = (slot.angle * 0xb) as f32 * (1.0 / 4096.0);
        let th = turns * std::f32::consts::TAU;
        let (sn, cs) = (th.sin(), th.cos());
        let f = fov as f32;
        let mut corners = [[0i32; 2]; 4];
        for (corner, &[x, y, _, _]) in corners.iter_mut().zip(&WEDGE_QUADS[quad]) {
            let qx = f32::from(x) * 2.0;
            let qy = f32::from(y) * 2.0;
            let vx = qx * cs + qy * sn + px;
            let vy = -qx * sn + qy * cs + py;
            let iz = f / pz;
            *corner = [
                (CENTRE[0] as f32 + vx * iz) as i16 as i32,
                (CENTRE[1] as f32 - vy * iz) as i16 as i32,
            ];
        }
        let [u0, v0] = [WEDGE_QUADS[quad][0][2], WEDGE_QUADS[quad][0][3]];
        // Ring order 0, 1, 2, 3 is top left, top right, bottom right, bottom
        // left of the texture; the quad wants TL, TR, BL, BR.
        let ring = corners;
        self.sprites.push(EffectQuad {
            key: (slot.depth_base as u32 + 0x80) * 0x10 + 500,
            page: Page::Umb02 as usize,
            uv: [u0 as u8, v0 as u8, 0, 0],
            source: [32, 40],
            pos: ring[0],
            size: [1, 1],
            corners: Some([ring[0], ring[1], ring[3], ring[2]]),
            tint: [255; 3],
            blend: 0,
            sheet: [0, 256],
            op: EffectBlend::Smooth,
            mirror: [false; 2],
        });
    }

    // ---- window animation --------------------------------------------------------

    fn windows_update(&mut self) {
        for w in &mut self.win {
            if w.enabled == 0 || w.anim_cmd & 1 == 0 {
                continue;
            }
            if w.anim_state == 0 {
                w.anim_state += 1;
                if w.anim_cmd & 0x80 != 0 {
                    w.alpha = if w.big == 0 { 0xff } else { 0x80 };
                } else {
                    w.alpha = 0;
                }
                w.visible = 1;
                continue;
            }
            let step = if w.big == 0 { 16 } else { 8 };
            let target = if w.big == 0 { 0xff } else { 0x80 };
            if w.anim_cmd & 0x80 == 0 {
                w.alpha += step;
                if w.alpha >= target {
                    w.alpha = 0x80;
                    w.visible = 0;
                    w.anim_cmd = 0;
                    w.anim_state = 0;
                }
            } else {
                w.alpha -= step;
                if w.alpha < 1 {
                    w.alpha = 0;
                    w.anim_cmd = 0;
                    w.anim_state = 0;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn window_setup(
        &mut self,
        index: usize,
        x: i16,
        y: i16,
        cols: i16,
        rows: i16,
        count: u8,
        data: Rows,
    ) {
        let w = &mut self.win[index];
        w.x = x;
        w.y = y;
        w.cols = cols;
        w.rows = rows;
        w.row_count = count;
        w.row_data = data;
        w.alpha = 0x80;
        w.visible = 0;
        w.anim_state = 0;
    }

    // ---- typing --------------------------------------------------------------------

    fn typing_update(&mut self, host: &mut Host<'_>) {
        if self.typer.active == 0 {
            return;
        }
        let line = self.text[self.typer.line as usize & 1];
        match self.typer.state {
            0 => {
                self.typer.state += 1;
                self.typer.char_idx = 0;
            }
            1 => {
                self.typer.state += 1;
                let index = self.typer.char_idx as usize;
                let code = line.byte(index);
                if code == 0 {
                    self.typer.state = 3;
                    Self::set_arm(host, 0, 0x46);
                    return;
                }
                let arm = ARM_FOR_KEY[usize::from(code) & 31];
                let command = (arm & 0x3f) | 0x40;
                Self::set_arm(host, usize::from(arm & 0x80 != 0), command);
                if Self::is_jill(host) {
                    let next = ARM_FOR_KEY[usize::from(line.byte(index + 2)) & 31];
                    self.typer.timer = if (next ^ arm) & 0x80 == 0 { 13 } else { 6 };
                } else {
                    self.typer.timer = ((Self::rand(host) & 0xf) + 10) as i16;
                }
            }
            2 => {
                self.typer.timer -= 1;
                if self.typer.timer != 0 {
                    return;
                }
                self.typer.char_idx += i32::from(Self::is_jill(host)) + 1;
                if line.byte(self.typer.char_idx as usize) == 0 {
                    self.typer.state = 3;
                    Self::set_arm(host, 0, 0x46);
                    return;
                }
                self.typer.state = 1;
            }
            _ => {
                if Self::arm(host, 0) & 0x20 != 0 {
                    self.typer.active = 0;
                    self.typer.state = 0;
                }
            }
        }
    }

    // ---- sprite submission ---------------------------------------------------------

    /// The page a draw resolves to: the original's page search over the
    /// texture slots `slot + 0xF ..`, `count` of them.
    fn resolve(&self, desc: &Desc, slot: u8, count: u8) -> Option<(Page, i32)> {
        let scale = [4, 2, 1, 1][((desc.flags >> 24) & 3) as usize];
        let mut page = desc.page as i32;
        let mut v_add = 0;
        if page > 16 {
            v_add = 256;
            page -= 16;
        }
        let u_words = page * 0x40 + i32::from(desc.tex_u) / scale;
        let v_abs = v_add + i32::from(desc.tex_v);
        for index in 0..count {
            let (candidate, tpage, words) = match slot + index {
                0x17 => (Page::Umb00, 7, 128),
                0x18 => (Page::Umb01, 9, 128),
                0x19 => (Page::Umb02, 0xb, 128),
                0x1c => (Page::Umbrella, 0, 64),
                _ => continue,
            };
            if self.loaded & (1 << candidate as u8) == 0 {
                continue;
            }
            let left = tpage * 0x40;
            let right = left + words;
            if left <= u_words
                && u_words + i32::from(desc.width) / scale <= right
                && v_abs + i32::from(desc.height) <= 256
            {
                let offset = (i32::from(desc.page) - tpage) * scale * 0x40;
                return Some((candidate, offset));
            }
        }
        None
    }

    /// `AddSpriteEx_Core`: one sprite of the current descriptor.
    fn submit(&mut self, key: u32, slot: u8, count: u8, scaled: bool, effect: bool) {
        let desc = self.desc;
        let Some((page, offset)) = self.resolve(&desc, slot, count) else {
            return;
        };
        let scaled = scaled && (desc.scale_x != 0x1000 || desc.scale_y != 0x1000);
        let sx = i32::from(desc.screen_x) + CENTRE[0];
        let sy = i32::from(desc.screen_y) + CENTRE[1];
        let (w, h) = (i32::from(desc.width), i32::from(desc.height));
        let (px, py) = (i32::from(desc.pivot_x), i32::from(desc.pivot_y));
        let fix = |value: i32| (value + ((value >> 31) & 0xfff)) >> 12;
        let (x0, y0, x1, y1) = if scaled {
            let (scx, scy) = (i32::from(desc.scale_x), i32::from(desc.scale_y));
            (
                sx - (fix(px * scx) as i16 as i32),
                sy - (fix(py * scy) as i16 as i32),
                (fix((w - px) * scx) as i16 as i32) + sx - 1,
                (fix((h - py) * scy) as i16 as i32) + sy - 1,
            )
        } else {
            (sx - px, sy - py, (w - px) + sx - 1, (h - py) + sy - 1)
        };
        let (x0, y0, x1, y1) = (
            x0 as i16 as i32,
            y0 as i16 as i32,
            x1 as i16 as i32,
            y1 as i16 as i32,
        );
        let su0 = (i32::from(desc.tex_u) + offset).max(0);
        let sv0 = i32::from(desc.tex_v).max(0);
        let su1 = (i32::from(desc.tex_u) + offset + w - 1).max(0);
        let sv1 = (i32::from(desc.tex_v) + h - 1).max(0);
        let colour = desc.colour.map(|channel| {
            let value = f32::from(channel) * (2.0f32 / 255.0f32);
            ((value * 255.0f32) as i32).clamp(0, 255) as u8
        });
        let variant = if desc.flags & 0x4000_0000 != 0 {
            ((desc.flags & 0x3000_0000) >> 28) + 1
        } else {
            0
        };
        let blend = match variant {
            1 | 2 | 4 => 0x80,
            _ => 0,
        };
        let key = if effect {
            key * 0x10 + 500
        } else {
            // A NORMAL-class sprite draws in the 2D pass after the scene:
            // keep its order but sort it in front of every scene item.
            (key * 0x10 + 500) >> 6
        };
        let size = [(x1 + 1 - x0).max(1), (y1 + 1 - y0).max(1)];
        let source = [(su1 + 1 - su0).max(0), (sv1 + 1 - sv0).max(0)];
        let quad = EffectQuad {
            key,
            page: page as usize,
            uv: [su0.min(255) as u8, sv0.min(255) as u8, 0, 0],
            source: [source[0].min(255) as u8, source[1].min(255) as u8],
            pos: [x0, y0],
            size,
            corners: None,
            tint: colour,
            blend,
            sheet: [0, 256],
            op: EffectBlend::Normal,
            mirror: [desc.flags & 0x0080_0000 != 0, desc.flags & 0x0040_0000 != 0],
        };
        // A full 256-texel row (the login window's title art) is wider than
        // a quad's byte-sized source: draw it as two 1:1 halves.
        if source[0] > 255 && size[0] == source[0] && !quad.mirror[0] {
            let half = source[0] / 2;
            self.sprites.push(EffectQuad {
                source: [half as u8, quad.source[1]],
                size: [half, size[1]],
                ..quad
            });
            self.sprites.push(EffectQuad {
                uv: [(su0 + half) as u8, quad.uv[1], 0, 0],
                source: [(source[0] - half) as u8, quad.source[1]],
                pos: [x0 + half, y0],
                size: [size[0] - half, size[1]],
                ..quad
            });
            return;
        }
        self.sprites.push(quad);
    }

    /// `SubmitEffectSprite_Ex`: unscaled, scene-sorted at `key`.
    fn effect_sprite(&mut self, key: u32, slot: u8, count: u8) {
        self.submit(key, slot, count, false, true);
    }

    /// `AddSprite_Ex`: scaled about the pivot, drawn in the 2D pass.
    fn add_sprite(&mut self, depth: u32, slot: u8, count: u8) {
        self.submit(depth, slot, count, true, false);
    }

    /// `cl_draw_quads` (0x00414ad0): quads back to front, `repeat` copies of
    /// each.
    fn draw_quads(
        &mut self,
        base_x: i16,
        base_y: i16,
        flags: u8,
        quads: &[Quad],
        page: u16,
        key_b: i32,
    ) {
        self.desc.page = page as i16;
        self.desc.pivot_x = 0;
        self.desc.pivot_y = 0;
        self.desc.clut_y = ((i32::from(page) - 7) / 2 + 0x1eb) as i16;
        for q in quads.iter().rev() {
            self.desc.flags =
                (u32::from(q.flags & 0xc0) << 16) | (u32::from((flags & 0x82) | 2) << 23);
            self.desc.screen_x = q.x.wrapping_add(base_x);
            self.desc.screen_y = q.y.wrapping_add(base_y);
            self.desc.tex_u = q.tex_u;
            self.desc.tex_v = q.tex_v;
            self.desc.width = q.width as u16;
            self.desc.height = q.height as u16;
            self.desc.colour = [self.col_r, self.col_g, self.col_b];
            let reps = q.repeat.max(1);
            let key = if flags & 0x40 != 0 {
                key_b + 0x3ff
            } else {
                key_b
            };
            let key = key as u32;
            if q.flags & 1 == 0 {
                self.desc.screen_x = self
                    .desc
                    .screen_x
                    .wrapping_add((i16::from(reps) - 1) * q.width);
                for _ in 0..reps {
                    self.effect_sprite(key, 0x17, 2);
                    self.desc.screen_x = self.desc.screen_x.wrapping_sub(q.width);
                }
            } else {
                self.desc.screen_y = self
                    .desc
                    .screen_y
                    .wrapping_add((i16::from(reps) - 1) * q.height);
                for _ in 0..reps {
                    self.effect_sprite(key, 0x17, 2);
                    self.desc.screen_y = self.desc.screen_y.wrapping_sub(q.height);
                }
            }
        }
    }

    // ---- draws ----------------------------------------------------------------------

    /// `cl_draw_keyboard` (0x00414300).
    fn draw_keyboard(&mut self) {
        let saved = (self.col_r, self.col_g, self.col_b);
        if self.win[3].enabled & 0x7f != 0 && self.win[3].alpha != 0 {
            let bright = self.win[3].alpha as u8;
            let desc = &mut self.desc;
            desc.flags = 0x0100_0000;
            desc.page = 9;
            desc.width = 0x18;
            desc.height = 0x18;
            desc.clut_y = 0x1ec;
            desc.pivot_x = 0;
            desc.pivot_y = 0;
            self.col_r = bright;
            self.col_g = bright;
            self.col_b = bright;
            let (col, row) = (self.col, self.row);
            desc.screen_x = (col * 0x18 - 0x58) as i16;
            desc.screen_y = (row * 0x18 + 2) as i16;
            desc.tex_u = (col * 0x18 + 0x10) as u8;
            desc.tex_v = (row * 0x18 + 0x20) as u8;
            desc.colour = [bright; 3];
            let cell = row * 8 + col;
            if cell == 0x17 {
                desc.height = 0x30;
            }
            if cell == 0x1d {
                desc.width = 0x30;
            }
            if self.blink_counter & 0x10 == 0 || self.blink_hold != 0 {
                self.effect_sprite(10, 0x17, 2);
            }
            if self.win[3].enabled & 0x80 != 0 {
                self.blink_hold = 0x14;
            }
            if self.blink_hold != 0 {
                self.blink_hold -= 1;
            }
            self.draw_quads(-0x68, -0x0e, 0x8a, &KEYBOARD_FRAME, 9, 10);
        }
        (self.col_r, self.col_g, self.col_b) = saved;
    }

    fn rows(&self, data: Rows) -> Vec<Row> {
        let mut rows = match data {
            Rows::Login => ROWS_LOGIN.to_vec(),
            Rows::PanelA => ROWS_PANEL_A.to_vec(),
            Rows::Side => ROWS_SIDE.to_vec(),
            Rows::Menu => ROWS_MENU.to_vec(),
            Rows::PanelB => ROWS_PANEL_B.to_vec(),
        };
        if data == Rows::Login {
            rows[2].width = self.login_row2_width;
        }
        rows
    }

    /// `cl_draw_windows` (0x004144c0): windows 2, 1, 0; in simple mode each
    /// index draws one fixed photo instead.
    fn draw_windows(&mut self) {
        let saved = (self.col_r, self.col_g, self.col_b);
        for idx in 0..3i32 {
            let w = self.win[(2 - idx) as usize];
            if w.enabled == 0 || w.alpha == 0 {
                continue;
            }
            let alpha = w.alpha as u8;
            if self.simple_mode == 0 {
                let fw = (w.cols + 1) * 0x10;
                let fh = (w.rows + 1) * 0x10;
                let mut frame = FRAME_QUADS;
                frame[1].x = fw;
                frame[7].x = fw;
                frame[2].y = fh;
                frame[5].y = fh;
                frame[3].x = fw;
                frame[3].y = fh;
                frame[4].repeat = w.cols as u8;
                frame[5].repeat = w.cols as u8;
                frame[6].repeat = w.rows as u8;
                frame[7].repeat = w.rows as u8;
                self.col_r = alpha;
                self.col_g = alpha;
                self.col_b = alpha;
                let key = (4 - idx) * 0x80;
                let blend = ((if w.visible == 0 { 0x00u8 } else { 0x80 }) | 0x0b)
                    .wrapping_add(((5 - idx) * 10) as u8);
                self.draw_quads(w.x, w.y, blend, &frame, 10, key);

                let desc = &mut self.desc;
                desc.page = 9;
                desc.clut_y = 0x1ec;
                desc.width = 0x10;
                desc.height = 0x10;
                desc.pivot_x = 0;
                desc.pivot_y = 0;
                desc.flags =
                    (if w.visible == 0 { 0xc000_0000u32 } else { 0 }).wrapping_add(0x4100_0000);
                desc.colour = [alpha; 3];
                let row_left = w.x;
                desc.screen_y = w.y.wrapping_add(0x10);
                for row in self.rows(w.row_data).iter().take(usize::from(w.row_count)) {
                    self.desc.screen_x = row_left.wrapping_add(0x10);
                    if row.width != 0 {
                        let desc = &mut self.desc;
                        desc.tex_u = row.tex_u;
                        desc.tex_v = row.tex_v;
                        desc.width = (row.width << 3) as u16;
                        desc.height = (row.height << 4) as u16;
                        desc.page = row.page;
                        desc.clut_y = (((i32::from(row.page) - 7) >> 1) + 0x1eb) as i16;
                        self.effect_sprite((idx * -0x80 + 0x210) as u32, 0x17, 2);
                    }
                    let pad = i32::from(w.cols) * 2 - i32::from(row.width);
                    if row.width < (w.cols as u16 * 2) as i16 {
                        let desc = &mut self.desc;
                        desc.tex_u = 0xe0;
                        desc.tex_v = 0x30;
                        desc.width = 8;
                        desc.height = 0x10;
                        desc.page = 9;
                        desc.clut_y = 0x1ec;
                        desc.screen_x = row_left.wrapping_add(0x10).wrapping_add(row.width * 8);
                        for i in 0..pad {
                            self.effect_sprite(((i + idx * -8 + 0x22) * 0x10) as u32, 0x17, 2);
                            self.desc.screen_x = self.desc.screen_x.wrapping_add(8);
                        }
                    }
                    self.desc.screen_y = self.desc.screen_y.wrapping_add(self.desc.height as i16);
                }
            } else {
                let flags = if w.visible == 0 { 0 } else { 0x4000_0000 };
                let photo =
                    |desc: &mut Desc, x: i16, y: i16, u: u8, v: u8, width: u16, height: u16| {
                        desc.screen_x = x;
                        desc.screen_y = y;
                        desc.page = 0;
                        desc.clut_y = 0x1e0;
                        desc.tex_u = u;
                        desc.tex_v = v;
                        desc.width = width;
                        desc.height = height;
                        desc.pivot_x = 0;
                        desc.pivot_y = 0;
                        desc.flags = flags;
                        desc.colour = [alpha; 3];
                    };
                match idx {
                    0 => {
                        photo(&mut self.desc, -0x96, -0x69, 0x59, 0x5e, 0xa3, 0x50);
                        self.effect_sprite(1000, 0x1c, 1);
                        self.desc.tex_u = 0x80;
                        self.desc.tex_v = 0xaf;
                        self.desc.screen_x = 0x0d;
                        self.desc.width = 0x7e;
                        self.effect_sprite(1000, 0x1c, 1);
                    }
                    1 => {
                        photo(&mut self.desc, -0x10, -0x39, 0x56, 0x0a, 0x59, 0x50);
                        self.effect_sprite(800, 0x1c, 1);
                    }
                    _ => {
                        photo(&mut self.desc, 0x10, -0x28, 0xb3, 0x01, 0x4b, 0x5c);
                        self.effect_sprite(600, 0x1c, 1);
                    }
                }
            }
        }
        (self.col_r, self.col_g, self.col_b) = saved;
    }

    /// `cl_draw_text` (0x00414cd0): the typed characters; a masked field
    /// prints `*` through the 8x8 text layer.
    fn draw_text(&mut self) {
        let desc = &mut self.desc;
        desc.page = 9;
        desc.clut_y = 0x1ec;
        desc.width = 0x10;
        desc.height = 0x10;
        desc.pivot_x = 0;
        desc.pivot_y = 0;
        for r in 0..2 {
            let t = self.text[r];
            let w = self.win[usize::from(t.win) & 3];
            self.desc.flags =
                (if w.visible == 0 { 0xc000_0000u32 } else { 0 }).wrapping_add(0x4100_0000);
            if t.enabled == 0 || w.enabled == 0 || w.anim_cmd != 0 || w.alpha == 0 {
                continue;
            }
            self.desc.screen_x = (t.x * 8 + 0x10).wrapping_add(w.x);
            self.desc.screen_y = ((t.y + 1) * 0x10).wrapping_add(w.y);
            for i in 0..8 {
                let (sx, sy) = (self.desc.screen_x, self.desc.screen_y);
                if t.text[i] != 0 {
                    let glyph = t.text[i].wrapping_sub(1);
                    self.desc.tex_v = glyph & 0x10;
                    self.desc.tex_u = glyph.wrapping_mul(0x10);
                    if t.masked == 0 {
                        self.effect_sprite(10, 0x17, 2);
                    } else {
                        self.print_mask(i32::from(sx) + 0xa4, i32::from(sy) + 0x7c);
                        let desc = &mut self.desc;
                        desc.tex_u = 0xb0;
                        desc.tex_v = 0x0f;
                        desc.page = 9;
                        desc.clut_y = 0x1ec;
                        desc.width = 0x10;
                        desc.height = 0x10;
                        desc.flags = (if w.visible == 0 { 0xc000_0000u32 } else { 0 })
                            .wrapping_add(0x4100_0000);
                        desc.screen_x = sx;
                        desc.screen_y = sy;
                    }
                }
                self.desc.screen_x = self.desc.screen_x.wrapping_add(0x10);
            }
        }
    }

    /// `PrintText8x8("*", x, y, 0xF0, 0)`: one glyph of the 8x8 font at
    /// screen pixel `(x, y)`, in the overlay pass after every sprite.
    fn print_mask(&mut self, x: i32, y: i32) {
        if self.loaded & (1 << Page::Font as u8) == 0 {
            return;
        }
        let ch = b'*';
        let u = ch.wrapping_mul(8);
        let v = (ch.wrapping_sub(0x20) & 0xe3) >> 2;
        self.sprites.push(EffectQuad {
            key: 0,
            page: Page::Font as usize,
            uv: [u, v, 0, 0],
            source: [8, 8],
            pos: [x, y],
            size: [8, 8],
            corners: None,
            // `PrintText8x8(.., 0xf0, 0)`: brightness nibble 15 scales the
            // white glyph by 15 * 255 / 30 = 127 (`tint_sprite_color`).
            tint: [127; 3],
            blend: 0,
            sheet: [0, 256],
            op: EffectBlend::Normal,
            mirror: [false; 2],
        });
        // `PrintText8x8` leaves its own descriptor fields behind.
        let desc = &mut self.desc;
        desc.flags = 0x40;
        desc.width = 8;
        desc.height = 8;
        desc.page = 30;
        desc.clut_y = 0x1e0;
        desc.colour = [128; 3];
        desc.pivot_x = 0;
        desc.pivot_y = 0;
    }

    /// `cl_draw_caret` (0x00414ee0): the blinking block caret of a field, or
    /// the highlight bar over a menu line.
    fn draw_caret(&mut self) {
        if self.input_enabled == 0 {
            return;
        }
        let w = self.win[usize::from(self.caret_win) & 3];
        if w.enabled == 0 {
            return;
        }
        let desc = &mut self.desc;
        desc.flags = 0x0100_0040;
        desc.page = 9;
        desc.clut_y = 0x1ec;
        desc.width = 0x10;
        self.blink_counter = self.blink_counter.wrapping_add(1);
        desc.pivot_x = 0;
        desc.height = 0x10;
        desc.pivot_y = 0;
        if self.caret_mode == 0 {
            desc.tex_u = 0xb0;
            desc.tex_v = 0x10;
            desc.screen_x =
                ((self.caret_pos as i16 * 2 + 2 + self.caret_base_x) * 8).wrapping_add(w.x);
            desc.screen_y = w.y.wrapping_add((self.caret_base_y + 1) * 0x10);
            let text = &self.text[usize::from(self.text_rec) & 1];
            let filled = text
                .text
                .get(usize::from(self.caret_pos))
                .copied()
                .unwrap_or(0)
                != 0;
            if self.blink_counter & 0x10 != 0 {
                if filled {
                    return;
                }
                desc.tex_v = 0x0f;
            }
            if self.caret_pos == 7 && filled {
                desc.tex_v = 0x1f;
                desc.height = 1;
                desc.screen_y = desc.screen_y.wrapping_add(0x0f);
            }
            self.effect_sprite(0x0f, 0x17, 2);
            return;
        }
        if self.blink_counter & 0x10 != 0 {
            return;
        }
        let (width, step, bias) = match self.caret_win {
            0 => (0x3b, 0x10, 0x0e),
            1 => (0x49, 0x12, 0x0c),
            _ => return,
        };
        desc.flags = 0x4000_0000;
        desc.screen_x = (self.caret_base_x * 8 + 8).wrapping_add(w.x);
        desc.clut_y = 0x1e0;
        desc.width = width;
        desc.height = 0x11;
        desc.screen_y = ((self.caret_pos as i16 + self.caret_base_y) * step)
            .wrapping_add(w.y)
            .wrapping_add(bias);
        desc.tex_v = 0xc6;
        desc.page = 0;
        desc.tex_u = 0;
        desc.pivot_x = 0;
        desc.pivot_y = 0;
        desc.colour = [0x80; 3];
        self.effect_sprite(200, 0x1c, 1);
    }

    /// `cl_draw_status` (0x004151c0).
    fn draw_status(&mut self) {
        if self.status_idx != 0 {
            let index = usize::from(self.status_idx).min(5);
            let count = if self.status_idx == 4 { 2 } else { 1 };
            let end = (index + count).min(STATUS_BANNERS.len());
            self.draw_quads(-0x30, -0x3d, 0x7c, &STATUS_BANNERS[index..end], 7, 1000);
        }
        if self.msg_flags & 3 == 0 {
            return;
        }
        if self.msg_flags & 3 == 2 {
            if self.msg_flags & 0x10 != 0 {
                self.draw_quads(-0x3c, -0x5f, 0x54, &STRIP_A, 7, 0x5dc);
            }
            if self.msg_flags & 0x20 != 0 {
                self.draw_quads(-0x3c, -0x5f, 0x54, &STRIP_B, 7, 0x5dc);
            }
        }
        let picture = usize::from(self.msg_flags & 3);
        self.draw_quads(
            -0x3c,
            -0x5f,
            (self.msg_flags & 0xf2) | 0x72,
            &MSG_PICTURES[picture..picture + 1],
            7,
            0x640,
        );
    }

    /// `cl_draw_door_picture` (0x004152a0).
    fn draw_door_picture(&mut self) {
        if self.door_sel & 0x80 == 0 {
            return;
        }
        let desc = &mut self.desc;
        desc.screen_x = -0x28;
        desc.screen_y = -0x5c;
        desc.clut_y = 0x1e0;
        desc.width = 0x54;
        desc.height = 0x61;
        desc.page = 0;
        desc.flags = 0;
        desc.tex_u = 0;
        desc.pivot_x = 0;
        desc.tex_v = 0;
        desc.pivot_y = 0;
        if self.door_sel & 0x7f == 0 {
            desc.tex_v = 100;
            desc.screen_x = -0x26;
            desc.screen_y = -0x5d;
        }
        self.effect_sprite(0x578, 0x1c, 1);
    }

    /// `cl_draw_leds` (0x00415340): the bezel lamps; the second flickers.
    fn draw_leds(&mut self, game: &mut GameState) {
        let desc = &mut self.desc;
        desc.flags = 0x4100_0040;
        desc.page = 7;
        desc.clut_y = 0x1eb;
        desc.width = 6;
        desc.height = 4;
        desc.pivot_x = 0;
        desc.pivot_y = 0;
        if self.led_flags & 1 != 0 {
            self.desc.tex_u = 0xe8;
            self.desc.tex_v = 0x88;
            self.desc.screen_x = 0x6f;
            self.desc.screen_y = -0x28;
            self.effect_sprite(0x4b4, 0x17, 2);
        }
        if self.led_flags & 2 != 0 && game::platform_rand(&mut game.rand_state) & 1 != 0 {
            self.desc.tex_u = 0xe8;
            self.desc.tex_v = 0x98;
            self.desc.screen_x = 0x6f;
            self.desc.screen_y = -0x23;
            self.effect_sprite(4, 0x17, 2);
        }
    }

    /// The full-screen monitor image, scaled by the descriptor's scale for
    /// the CRT power-on stretch.
    fn draw_monitor(&mut self) {
        let desc = &mut self.desc;
        desc.page = 7;
        desc.clut_y = 0x1eb;
        desc.pivot_x = 0x3c;
        desc.pivot_y = 0x34;
        desc.screen_y = -0x2b;
        desc.screen_x = 0;
        desc.tex_u = 0;
        desc.height = 0x68;
        desc.width = 0x78;
        desc.tex_v = 0x68;
        self.add_sprite(3, 0x17, 1);
    }

    // ---- stage 0: boot ----------------------------------------------------------------

    fn stage_boot(&mut self, host: &mut Host<'_>) {
        if self.sub2 == 0 {
            match self.sub3 & 3 {
                0 => {
                    self.sub3 += 1;
                    self.desc.scale_y = 100;
                    self.desc.scale_x = 0;
                    self.led_flags = 1;
                    self.msg_flags = 0x83;
                    self.timer_a = 0;
                }
                1 => {
                    self.timer_a = self.timer_a.wrapping_add(0x800);
                    self.desc.flags = 0x0100_0000;
                    self.desc.colour = [0x80; 3];
                    self.desc.scale_x = self.timer_a;
                    self.desc.scale_y = 100;
                    if self.timer_a > 0xfff {
                        self.sub3 += 1;
                        self.timer_a = 100;
                    }
                }
                2 => {
                    self.timer_a = self.timer_a.wrapping_add(0x800);
                    if self.timer_a > 0x1000 {
                        self.timer_a = 0x1000;
                        self.timer_b = 0x5a;
                        self.sub3 += 1;
                        self.msg_flags = 0;
                    }
                    self.desc.flags = 0x0100_0000;
                    self.desc.scale_x = 0x1000;
                    self.desc.scale_y = self.timer_a;
                }
                _ => {
                    self.timer_b -= 1;
                    self.desc.flags = (if self.timer_b & 1 == 0 {
                        0x4000_0000
                    } else {
                        0
                    }) + 0x0100_0000;
                    let level = (i32::from(self.timer_b) / 2) as u8;
                    self.desc.colour = [level; 3];
                    self.desc.scale_x = 0x1000;
                    self.desc.scale_y = 0x1000;
                    if self.timer_b == 0x3c {
                        Self::sfx(host, 0x17);
                    }
                    if self.timer_b < 0x1e {
                        self.led_flags = 3;
                    }
                    if self.timer_b == 0 {
                        self.intro_logo_state = 0;
                        self.led_flags = 1;
                        self.sub2 += 1;
                        self.sub3 = 0;
                        if self.pad.dpad_held & 0xc0 != 0 {
                            self.msg_flags = 1;
                            self.sub3 = 1;
                            self.timer_a = 0x1e;
                        }
                    }
                }
            }
            self.draw_monitor();
            return;
        }
        // `cl_boot_logo` (0x00412870). Step 2's fade cannot be a match guard:
        // it advances the fade.
        #[allow(clippy::collapsible_match)]
        match self.sub3 {
            0 => {
                self.intro_logo_update(host);
                if self.intro_logo_state == 2 {
                    self.msg_flags = 1;
                    self.sub3 += 1;
                    self.timer_a = 0x1e;
                }
            }
            1 => {
                self.timer_a -= 1;
                if self.timer_a == 0 {
                    self.sub3 += 1;
                    self.timer_a = 0x1e;
                    self.timer_b = 0;
                }
            }
            2 => {
                if self.fade_step(8, 2) {
                    self.timer_a -= 1;
                    if self.timer_a == 0 {
                        self.sub += 1;
                        self.sub2 = 0;
                    }
                }
            }
            _ => {}
        }
    }

    // ---- stage 1: login ------------------------------------------------------------------

    fn login_init(&mut self, host: &mut Host<'_>) {
        self.load_intro_texture();
        self.simple_mode = 0;
        self.sub2 += 1;
        self.sub3 = 0;
        self.window_setup(0, -0x90, -0x68, 0x10, 4, 3, Rows::Login);
        self.win[0].enabled = 1;
        self.pad_repeat_timer = 0;
        self.login_row2_width = 0;
        self.text[0].enabled = 0;
        self.win[0].anim_cmd = 1;
        self.win[3].anim_cmd = 1;
        self.text[0].win = 0;
        self.text[0].masked = 0;
        self.text[1].masked = 0;
        self.win[3].enabled = 1;
        self.text[0].x = 7;
        self.text[0].y = 2;
        self.text_begin(0);
        self.status_idx = 0;
        self.col = 1;
        self.row = 0;
        self.pending_cam_delay = 0;
        Self::set_arm(host, 0, 0x47);
        Self::set_arm(host, 1, 0x47);
        self.pending_camera = CAMERA_PENDING | CAMERA_KEYBOARD;
    }

    /// Steps 1 and 3 of the login, also the command prompt's typing.
    fn login_typing(&mut self) {
        self.win[3].row_count = 0;
        let key = self.keyboard_input();
        if key == -1 {
            self.sub3 += 1;
            return;
        }
        if key == -2 || self.pad.dpad_pressed & 0x80 != 0 {
            self.sub3 += 1;
            self.win[3].row_count = 1;
            return;
        }
        if key != 0 {
            let rec = (self.timer_a as usize) & 1;
            self.text_append(key, rec);
            if self.caret_pos != 7 {
                self.blink_counter = 0;
            }
        }
    }

    fn login_step(&mut self, step: u8, host: &mut Host<'_>) {
        match step {
            0 => {
                if self.win[0].anim_cmd == 0 && self.win[3].anim_cmd == 0 {
                    self.sub3 += 1;
                    self.timer_a = 0;
                    self.text[0].enabled = 1;
                    self.input_enabled = 1;
                    self.caret_mode = 0;
                }
            }
            1 | 3 => self.login_typing(),
            2 => {
                if self.win[3].row_count != 0 {
                    self.set_state_word(2);
                    return;
                }
                self.sub3 += 1;
                self.text[1].y = 3;
                self.text[1].enabled = 1;
                self.login_row2_width = 0xc;
                self.text[1].x = 0xc;
                self.text[1].masked = 1;
                self.text[1].win = 0;
                self.timer_a = 1;
                self.text_begin(1);
            }
            4 => {
                if self.win[3].row_count != 0 {
                    self.set_state_word(2);
                    return;
                }
                self.sub3 += 1;
                self.input_enabled = 0;
                self.pending_cam_delay = 0;
                self.win[0].anim_cmd = 0x81;
                self.msg_flags = 2;
                self.pending_camera = CAMERA_PENDING | CAMERA_MONITOR;
                self.win[3].anim_cmd = 0x81;
                Self::set_arm(host, 0, 0x40);
                Self::set_arm(host, 1, 0x40);
            }
            5 => {
                if self.win[0].anim_cmd == 0 && self.win[3].anim_cmd == 0 {
                    self.sub2 += 1;
                    self.sub3 = 0;
                    self.text[0].enabled = 0;
                    self.text[1].enabled = 0;
                    self.win[0].enabled = 0;
                    self.win[3].enabled = 0;
                }
            }
            6 => {
                self.sub3 += 1;
                self.typer.line = 0;
                self.typer.active = 1;
            }
            7 => {
                if self.typer.active != 0 {
                    return;
                }
                self.typer.line += 1;
                if self.typer.line == 2 {
                    self.sub3 += 1;
                    return;
                }
                self.typer.active = 1;
            }
            8 => {
                let user_ok = self.text[0].word() == EXPECT_USER;
                if user_ok && self.text[1].word() == EXPECT_PASS {
                    self.sub3 = 4;
                    self.timer_a = 0x5a;
                    return;
                }
                self.sub3 = 3;
                self.status_idx = 4;
                self.timer_a = 0x5a;
                Self::sfx(host, 0x19);
            }
            9 => {
                if self.timer_a > 0 {
                    self.timer_a -= 1;
                }
                if self.timer_a == 0
                    && Self::arm(host, 0) & 0x20 != 0
                    && Self::arm(host, 1) & 0x20 != 0
                {
                    self.sub2 = 0;
                    self.sub3 = 0;
                }
                if self.timer_a == 0x4b {
                    Self::set_arm(host, 0, 0x44);
                    Self::set_arm(host, 1, 0x44);
                }
            }
            _ => {
                if self.timer_a > 0 {
                    self.timer_a -= 1;
                }
                if self.timer_a == 0 {
                    self.sub += 1;
                    self.sub2 = 0;
                }
                if self.timer_a > 0x32 {
                    self.draw_quads(-0x3c, -0x5f, 3, &STRIP_A[..1], 7, 10);
                    return;
                }
                self.msg_flags |= 0x10;
            }
        }
    }

    fn stage_login(&mut self, host: &mut Host<'_>) {
        match self.sub2 {
            0 => self.login_init(host),
            1 => self.login_step(self.sub3.min(10), host),
            _ => self.login_step(6 + self.sub3.min(4), host),
        }
    }

    // ---- stage 2: the door menu ------------------------------------------------------------

    fn menu_step(&mut self, step: u8, host: &mut Host<'_>) {
        match step {
            0 => {
                if self.timer_a != 0 {
                    self.timer_a -= 1;
                    if self.timer_a == 0 {
                        self.win[1].enabled = 1;
                        self.win[1].anim_cmd = 1;
                    }
                    return;
                }
                if self.win[0].anim_cmd == 0 && self.win[1].anim_cmd == 0 {
                    self.sub3 += 1;
                    Self::set_arm(host, 0, 0x47);
                    Self::set_arm(host, 1, 0x47);
                    self.input_enabled = 1;
                }
            }
            1 => {
                if self.pad.dpad_pressed & 0x40 != 0 {
                    match self.caret_pos {
                        0 => {
                            self.door_sel = 0;
                            self.sub3 = 4;
                        }
                        1 => {
                            self.door_sel = 1;
                            self.sub3 = 4;
                        }
                        2 => {
                            self.sub3 = 2;
                            self.input_enabled = 0;
                        }
                        _ => {}
                    }
                }
                if self.pad.dpad_pressed & 0x80 != 0 {
                    self.sub3 = 2;
                    self.input_enabled = 0;
                }
            }
            2 => {
                self.sub3 += 1;
                self.window_setup(0, 0x10, -0x28, 3, 4, 4, Rows::Menu);
                self.caret_base_x = 0;
                self.win[0].enabled = 1;
                self.input_enabled = 1;
                self.caret_win = 0;
                self.caret_pos = 0;
                self.caret_mode = 1;
                self.blink_counter = 0;
                self.text_rec = 4;
                self.win[0].alpha = 0x80;
                self.caret_base_y = 2;
                self.timer_b = 1;
                Self::set_arm(host, 0, 0x40);
            }
            3 => {
                let mut leave = self.pad.dpad_pressed & 0x80 != 0;
                if self.pad.dpad_pressed & 0x40 != 0 {
                    if self.caret_pos == 0 {
                        self.set_state_word(2);
                        return;
                    }
                    leave = true;
                }
                if !leave {
                    return;
                }
                self.sub3 = 1;
                self.caret_base_x = 0;
                self.caret_win = 1;
                self.caret_base_y = 0;
                self.caret_pos = 0;
                self.caret_mode = 1;
                self.blink_counter = 0;
                self.win[0].enabled = 0;
                self.input_enabled = 1;
                Self::set_arm(host, 0, 0x47);
                self.text_rec = 5;
                Self::set_arm(host, 1, 0x47);
                self.timer_b = 2;
            }
            4 => {
                self.sub3 += 1;
                self.input_enabled = 0;
                self.msg_flags = 0x12;
                self.win[1].anim_cmd = 0x81;
                self.timer_a = 0x0c;
                self.pending_camera = CAMERA_PENDING | CAMERA_MONITOR;
            }
            5 => {
                if self.timer_a != 0 {
                    self.timer_a -= 1;
                    if self.timer_a == 0 {
                        self.win[2].anim_cmd = 0x81;
                    }
                    return;
                }
                if self.win[1].anim_cmd == 0 && self.win[2].anim_cmd == 0 {
                    self.sub3 += 1;
                    self.win[1].enabled = 0;
                    self.win[2].enabled = 0;
                    self.timer_a = 0x5a;
                    if self.door_sel == 0 {
                        Self::set_arm(host, 0, 0x46);
                        self.timer_b = 1;
                        return;
                    }
                    Self::set_arm(host, 0, 0x48);
                    self.timer_b = 2;
                }
            }
            6 => {
                if Self::arm(host, 0) & 0x20 == 0 {
                    return;
                }
                self.timer_b -= 1;
                if self.timer_b != 0 {
                    Self::set_arm(host, 0, 0x46);
                    return;
                }
                self.sub3 += 1;
                self.led_flags |= 2;
                Self::set_arm(host, 1, 0x47);
            }
            7 => {
                self.timer_a -= 1;
                if self.timer_a != 0 {
                    self.status_idx = u8::from((self.timer_a as u16) & 0x10 == 0);
                    return;
                }
                if self.door_sel == 1 {
                    self.sub = 4;
                    self.sub2 = 0;
                    return;
                }
                self.sub3 += 1;
                self.timer_a = 0x1e;
                self.timer_b = 0;
                self.status_idx = 0;
            }
            _ => {
                self.timer_a -= 1;
                if self.timer_a == 0 {
                    self.timer_a = 0x1e;
                    self.timer_b += 1;
                    if self.timer_b == 3 {
                        self.sub = 3;
                        self.sub2 = 0;
                        self.timer_a = 0x1e;
                        self.timer_b = 0;
                        self.msg_flags = 0x32;
                    }
                }
                if self.timer_b != 0 {
                    let count = (self.timer_b as usize).min(STRIP_B.len());
                    self.draw_quads(-0x3c, -0x5f, 0x40, &STRIP_B[..count], 7, 0);
                }
            }
        }
    }

    fn stage_menu(&mut self, host: &mut Host<'_>) {
        if self.sub2 == 0 {
            self.simple_mode = 1;
            self.sub2 += 1;
            self.sub3 = 0;
            self.window_setup(1, -0x10, -0x38, 4, 3, 3, Rows::Side);
            self.win[1].enabled = 0;
            self.win[1].anim_cmd = 0;
            self.window_setup(2, -0x90, -0x68, 0x10, 3, 3, Rows::PanelA);
            self.caret_base_x = 0;
            self.caret_base_y = 0;
            self.caret_pos = 0;
            self.win[2].enabled = 1;
            self.win[2].anim_cmd = 1;
            self.blink_counter = 0;
            self.input_enabled = 0;
            self.text[0].enabled = 0;
            self.text[1].enabled = 0;
            self.pending_cam_delay = 0;
            self.status_idx = 0;
            self.caret_win = 1;
            self.caret_mode = 1;
            self.text_rec = 5;
            self.pending_camera = CAMERA_PENDING | CAMERA_KEYBOARD;
            self.timer_a = 0x0c;
            self.timer_b = 2;
            return;
        }
        if self.input_enabled != 0 {
            if self.pad.pad_edge & 0x10 != 0 && self.caret_pos != 0 {
                self.blink_counter = 0;
                self.caret_pos -= 1;
            }
            if self.pad.pad_edge & 0x40 != 0 && (self.caret_pos as i16) < self.timer_b {
                self.blink_counter = 0;
                self.caret_pos += 1;
            }
        }
        self.menu_step(self.sub3.min(8), host);
    }

    // ---- stages 3 and 4: the command prompt and the unlocked screen ----------------------------

    fn command_step(&mut self, step: u8, host: &mut Host<'_>) {
        match step {
            0 => {
                self.sub3 += 1;
                self.win[2].anim_cmd = 0x81;
                self.win[3].anim_cmd = 0x81;
                self.input_enabled = 0;
                self.msg_flags = 0x32;
                Self::set_arm(host, 0, 0x40);
                Self::set_arm(host, 1, 0x40);
            }
            1 => {
                if self.win[2].anim_cmd != 0 || self.win[3].anim_cmd != 0 {
                    return;
                }
                self.sub3 += 1;
                self.win[2].enabled = 0;
                self.win[3].anim_cmd = 0;
                self.win[3].enabled = 0;
                if self.win[3].row_count == 0 {
                    self.typer.active = 1;
                    self.typer.line = 0;
                } else {
                    Self::set_arm(host, 1, 0x46);
                }
                self.pending_cam_delay = 2;
                self.pending_camera = CAMERA_PENDING | CAMERA_MONITOR;
            }
            2 => {
                if self.win[3].row_count == 0 {
                    if self.typer.active == 0 {
                        self.sub3 += 1;
                    }
                    return;
                }
                if Self::arm(host, 1) & 0x20 != 0 {
                    self.set_state_word(0x201);
                }
            }
            3 => {
                if self.text[0].word() != EXPECT_CMD {
                    self.sub3 = 4;
                    self.timer_a = 0x5a;
                    self.status_idx = 2;
                    Self::sfx(host, 0x19);
                    return;
                }
                self.sub3 = 5;
                self.timer_a = 0x5a;
                self.status_idx = 3;
                self.led_flags |= 2;
            }
            4 => {
                if self.timer_a > 0 {
                    self.timer_a -= 1;
                }
                if self.timer_a == 0
                    && Self::arm(host, 0) & 0x20 != 0
                    && Self::arm(host, 1) & 0x20 != 0
                {
                    self.sub = 2;
                    self.sub2 = 0;
                }
                if self.timer_a == 0x4b {
                    Self::set_arm(host, 0, 0x44);
                    Self::set_arm(host, 1, 0x44);
                }
            }
            5 => {
                if self.timer_a > 0 {
                    self.timer_a -= 1;
                }
                if self.timer_a == 0 {
                    self.sub += 1;
                    self.sub2 = 0;
                    self.led_flags = 1;
                    self.text[0].enabled = 0;
                }
            }
            6 => {
                // `cl_cmd_unlock`: the point of the whole sequence.
                self.sub2 += 1;
                self.sub3 = 0;
                self.pending_camera = CAMERA_PENDING | CAMERA_MONITOR;
                self.status_idx = 0;
                self.led_flags = 1;
                self.door_sel |= 0x80;
                self.timer_a = 0x5a;
                host.game.apply_flag(game::BANK_ROOM_FLAGS, LAB_MAP_FLAG, 0);
                let lock_bit = 0x26 - u8::from(self.door_sel & 0x7f == 0);
                if host.game.flag_test(BANK_LOCKS, lock_bit, false) {
                    self.timer_a = 0x20;
                    return;
                }
                host.game.apply_flag(BANK_LOCKS, lock_bit, 0);
                Self::set_arm(host, 0, 0x45);
                Self::set_arm(host, 1, 0x45);
            }
            7 => {
                self.timer_a -= 1;
                if self.timer_a == 0 {
                    self.sub2 += 1;
                    self.timer_a = 0x10;
                }
            }
            _ => {
                if self.sub3 == 0 {
                    self.sub3 = 1;
                    self.timer_b = 0x96;
                }
                self.timer_b -= 1;
                if self.timer_b != 0 && self.pad.dpad_pressed & 0xc0 == 0 {
                    let desc = &mut self.desc;
                    desc.page = 0;
                    desc.flags = 0;
                    desc.screen_x = -0x38;
                    desc.screen_y = -0x40;
                    desc.tex_u = 3;
                    desc.tex_v = 0xd7;
                    desc.clut_y = 0x1e0;
                    desc.width = 0x72;
                    desc.pivot_x = 0;
                    desc.pivot_y = 0;
                    desc.height = 0x29;
                    self.effect_sprite(1000, 0x1c, 1);
                    return;
                }
                self.set_state_word(0x201);
                self.win[0].enabled = 0;
                self.door_sel = 0;
            }
        }
    }

    fn stage_command(&mut self, host: &mut Host<'_>) {
        if self.sub2 == 0 {
            self.simple_mode = 0;
            self.timer_a -= 1;
            if self.timer_a != 0 {
                return;
            }
            self.sub2 += 1;
            self.sub3 = 0;
            self.led_flags = 1;
            self.window_setup(2, -0x90, -0x68, 0x10, 4, 4, Rows::PanelB);
            self.win[2].enabled = 1;
            self.win[2].anim_cmd = 1;
            self.win[3].anim_cmd = 1;
            self.text[0].enabled = 1;
            self.text[0].masked = 1;
            self.text[0].win = 2;
            self.text[0].x = 0x0c;
            self.text[0].y = 3;
            self.text_begin(0);
            self.win[3].enabled = 1;
            self.caret_mode = 0;
            self.input_enabled = 0;
            self.pad_repeat_timer = 0;
            self.pending_camera = CAMERA_PENDING | CAMERA_KEYBOARD;
            self.col = 1;
            self.row = 0;
            self.timer_a = 0;
            Self::sfx(host, 0x19);
            Self::set_arm(host, 0, 0x47);
            Self::set_arm(host, 1, 0x47);
            return;
        }
        if self.sub2 == 1 {
            if self.win[2].anim_cmd != 0 || self.win[3].anim_cmd != 0 {
                return;
            }
            self.input_enabled = 1;
            if self.sub3 != 0 {
                self.sub2 += 1;
                self.sub3 = 0;
                return;
            }
            self.login_typing();
            return;
        }
        self.command_step(self.sub3.min(8), host);
    }

    fn stage_unlocked(&mut self, host: &mut Host<'_>) {
        self.command_step(6 + self.sub2.min(2), host);
        let door = usize::from(self.door_sel & 0x7f != 0);
        if self.timer_a & 0x10 != 0 {
            let [x, y] = DOOR_ORIGIN[door];
            self.draw_quads(
                i16::from(x) - 0x3c,
                i16::from(y) - 0x5f,
                5,
                &DOOR_MARKER[door..door + 1],
                10,
                0x4b0,
            );
        }
    }

    // ---- top level -------------------------------------------------------------------------------

    /// `computer_lab_init` (0x004123d0).
    fn init(&mut self, host: &mut Host<'_>) {
        self.set_state_word(1);
        self.saved = Some(Saved {
            // `g_roomCameraId`, which a deferred switch may have moved ahead
            // of the drawn cut.
            camera: host.game.camera.current_cut,
            player_y: host.player.pos[1],
            lights: host.room.lights,
            ambient: host.room.ambient,
        });
        Self::switch_camera(host, usize::from(CAMERA_MONITOR));
        host.game.message_flags &= 0xfff8;
        host.game.message_flags &= 0xfeff;
        host.player.pos[1] = -9000;
        host.game.sync_entity_from_player(host.player);
        for (light, (pos, radius)) in host.room.lights.iter_mut().zip(LAB_LIGHTS) {
            light.pos = pos;
            light.color = [0x78; 3];
            light.kind = 0;
            light.radius = radius;
        }
        host.room.ambient = [LAB_AMBIENT; 3];
        self.door_sel = 0;
        self.input_enabled = 0;
        for w in &mut self.win {
            w.enabled = 0;
            w.anim_cmd = 0;
            w.alpha = 0;
            w.big = 0;
        }
        self.win[3].big = 1;
        for text in &mut self.text {
            text.enabled = 0;
        }
        self.col_r = 0x80;
        self.col_g = 0x80;
        self.col_b = 0x80;
        Self::set_arm(host, 0, 0x40);
        Self::set_arm(host, 1, 0x40);
        self.load_textures();
        self.loaded |= 1 << Page::Font as u8;
        self.simple_mode = 0;
    }

    /// `computer_lab_update` (0x004125b0).
    fn update(&mut self, host: &mut Host<'_>) {
        match self.sub.min(4) {
            0 => self.stage_boot(host),
            1 => self.stage_login(host),
            2 => self.stage_menu(host),
            3 => self.stage_command(host),
            _ => self.stage_unlocked(host),
        }
        self.windows_update();
        self.typing_update(host);
    }

    /// `computer_lab_finish` (0x00413920).
    fn finish(&mut self, host: &mut Host<'_>) {
        if self.sub == 0 {
            match self.sub2.min(2) {
                0 => self.finish_start(),
                1 => {
                    self.timer_a = self.timer_a.wrapping_sub(2000);
                    self.desc.scale_x = 0x1000;
                    self.desc.scale_y = self.timer_a;
                    if self.timer_a < 0x65 {
                        self.sub2 += 1;
                        self.timer_a = 0x1000;
                    }
                }
                _ => {
                    self.timer_a = self.timer_a.wrapping_sub(0x800);
                    self.desc.flags = 0x0100_0000;
                    self.desc.colour = [0x80; 3];
                    self.desc.scale_x = self.timer_a;
                    self.desc.scale_y = 0x60;
                    if self.timer_a < 1 {
                        self.desc.scale_x = 0;
                        self.sub += 1;
                        self.timer_a = 0x1e;
                    }
                }
            }
            self.typing_update(host);
            self.windows_update();
            if self.sub2 != 0 {
                let desc = &mut self.desc;
                desc.colour = [0x80; 3];
                desc.flags = 0x0500_0000;
                desc.page = 7;
                desc.clut_y = 0x1eb;
                desc.screen_x = 0;
                desc.tex_u = 0;
                desc.pivot_x = 0x3c;
                desc.height = 0x68;
                desc.tex_v = 0x68;
                desc.pivot_y = 0x34;
                desc.screen_y = -0x2b;
                desc.width = 0x78;
                self.add_sprite(3, 0x17, 1);
            }
            return;
        }
        self.timer_a -= 1;
        if self.timer_a != 0 {
            return;
        }
        // `computer_lab_finish`'s order: the lights, the arms and the pages,
        // then the camera, the gate and the player.
        let saved = self.saved.take();
        if let Some(saved) = &saved {
            host.room.lights = saved.lights;
            host.room.ambient = saved.ambient;
        }
        Self::set_arm(host, 0, 0x80);
        Self::set_arm(host, 1, 0x80);
        self.func = 0;
        host.game.message_flags |= 0x0107;
        self.free_textures();
        if let Some(saved) = &saved {
            Self::switch_camera(host, saved.camera);
        }
        host.game
            .apply_flag(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, 1);
        if let Some(saved) = saved {
            host.player.pos[1] = saved.player_y;
            host.game.sync_entity_from_player(host.player);
        }
    }

    /// `cl_finish_start` (0x00413a00): close the windows and type the
    /// log-out with the hands.
    fn finish_start(&mut self) {
        if self.sub3 == 0 {
            self.sub3 = 1;
            for w in &mut self.win {
                w.anim_cmd = 0x81;
                if w.enabled == 0 {
                    w.anim_cmd = 0;
                }
            }
            self.text[0].text[..5].copy_from_slice(&[0x13, 0x08, 0x15, 0x14, 0]);
            self.typer.active = 1;
            self.typer.line = 0;
            self.input_enabled = 0;
            self.pending_cam_delay = 0;
            self.pending_camera = CAMERA_PENDING | CAMERA_MONITOR;
            return;
        }
        if self.sub3 == 1 {
            if self.typer.active == 0 {
                self.timer_b = 0;
                self.sub3 += 1;
                self.led_flags = 3;
            }
            return;
        }
        if self.sub3 != 2 {
            return;
        }
        self.fade_step(8, 0);
        if self.timer_b != 0 && self.win.iter().all(|w| w.anim_cmd == 0) {
            for w in &mut self.win {
                w.enabled = 0;
            }
            self.sub2 += 1;
            self.desc.scale_x = 0x1000;
            self.desc.scale_y = 0x1000;
            self.msg_flags = 0;
            self.timer_a = 0x1000;
            self.led_flags = 0;
        }
    }
}

/// Decode one terminal page to RGBA: every texel whose CLUT word is
/// `0x0000` is transparent, the PC renderer's colour key.
pub fn decode_page(data: &[u8]) -> Option<Image> {
    let texture = match data.get(4).map(|flags| flags & 7) {
        Some(0) => crate::tim::decode_4bpp(data).ok()?,
        Some(1) => crate::tim::decode_8bpp(data).ok()?,
        _ => return None,
    };
    let mut rgba = Vec::with_capacity(texture.indices.len() * 4);
    for &index in &texture.indices {
        let [r, g, b, _] = texture.palette(0, index);
        if [r, g, b] == [0, 0, 0] && !texture.palette_stp(0, index) {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Some(Image {
        width: texture.width,
        height: texture.height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keyboard_skips_the_covered_cells() {
        let mut terminal = Terminal {
            col: 6,
            row: 3,
            ..Terminal::default()
        };
        terminal.pad = Pad {
            raw_held: 0x2000,
            raw_edge: 0x2000,
            ..Pad::default()
        };
        // Right from BACKSPACE skips its second cell onto column 7, which is
        // ENTER's covered lower half, so the row steps up onto ENTER.
        terminal.col = 5;
        assert_eq!(terminal.keyboard_input(), 0);
        assert_eq!((terminal.col, terminal.row), (7, 2));
        // Down from ENTER stays on ENTER.
        let mut terminal = Terminal {
            col: 7,
            row: 2,
            ..Terminal::default()
        };
        terminal.pad = Pad {
            raw_held: 0x4000,
            raw_edge: 0x4000,
            ..Pad::default()
        };
        terminal.keyboard_input();
        assert_eq!((terminal.col, terminal.row), (7, 2));
    }

    #[test]
    fn a_room_load_starts_the_terminal_over() {
        let mut terminal = Terminal {
            func: 1,
            sub: 2,
            was_up: true,
            loaded: 0x1f,
            ..Terminal::default()
        };
        terminal.room_reset();
        assert_eq!((terminal.func, terminal.was_up), (0, false));
        assert_eq!(terminal.loaded, 1 << Page::Font as u8);
    }

    #[test]
    fn the_cancel_key_is_the_run_bit() {
        let mut terminal = Terminal::default();
        let pad = terminal.pad_from_dpad(0, 0, true);
        assert_eq!((pad.dpad_held, pad.dpad_pressed), (0x80, 0x80));
        // Held on: no second edge.
        let pad = terminal.pad_from_dpad(0, 0, true);
        assert_eq!((pad.dpad_held, pad.dpad_pressed), (0x80, 0));
        let pad = terminal.pad_from_dpad(0, 0, false);
        assert_eq!((pad.dpad_held, pad.dpad_pressed), (0, 0));
        // Run itself still reads as cancel.
        let pad = terminal.pad_from_dpad(0x10, 0x10, false);
        assert_eq!(pad.dpad_pressed, 0x80);
    }

    #[test]
    fn the_eighth_character_overwrites_in_place() {
        let mut terminal = Terminal::default();
        terminal.text_begin(0);
        for key in 1..=9 {
            terminal.text_append(key, 0);
        }
        assert_eq!(terminal.text[0].word(), [1, 2, 3, 4, 5, 6, 7, 9]);
        terminal.text_append(-3, 0);
        assert_eq!(terminal.text[0].word(), [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn colour_bytes_follow_the_reference_scale() {
        let mut terminal = Terminal::default();
        terminal.load_textures();
        terminal.desc.page = 7;
        terminal.desc.width = 8;
        terminal.desc.height = 8;
        // `0x40 * (2/255f)` times 255 is exactly 128.0 in f32.
        for (input, expected) in [(0x80u8, 255u8), (0x40, 128), (0xff, 255), (0, 0)] {
            terminal.desc.colour = [input; 3];
            terminal.sprites.clear();
            terminal.effect_sprite(10, 0x17, 2);
            let sprite = terminal.sprites[0];
            assert_eq!(sprite.tint[0], expected, "multiplier {input:#x}");
            assert_eq!(sprite.key, 10 * 16 + 500);
        }
    }

    #[test]
    fn a_page_outside_the_searched_slots_draws_nothing() {
        let mut terminal = Terminal::default();
        terminal.load_textures();
        terminal.desc.page = 0;
        terminal.desc.width = 8;
        terminal.desc.height = 8;
        terminal.effect_sprite(10, 0x17, 2);
        assert!(terminal.sprites.is_empty());
        terminal.desc.page = 10;
        terminal.desc.tex_u = 0x10;
        terminal.effect_sprite(10, 0x17, 2);
        // tpage 10 is umb01's right half.
        assert_eq!(terminal.sprites[0].page, Page::Umb01 as usize);
        assert_eq!(terminal.sprites[0].uv[0], 0x90);
    }
}
