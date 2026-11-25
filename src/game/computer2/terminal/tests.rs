use super::super::{Computer, Effect, World, test_strings};
use super::*;

/// Room 613's request: mode 1, cut 6.
const REQUEST: u8 = 0x61;

fn terminal() -> Computer {
    Computer::new(REQUEST, test_strings())
}

/// Ticks until `done` holds, at most `limit`, with `world` each tick; the
/// effects of every tick.
fn until(
    computer: &mut Computer,
    world: World,
    limit: usize,
    done: impl Fn(&Computer) -> bool,
) -> Vec<Effect> {
    let mut effects = Vec::new();
    for _ in 0..limit {
        if done(computer) {
            return effects;
        }
        effects.extend(computer.tick(&world));
    }
    assert!(
        done(computer),
        "not reached in {limit} ticks: {}",
        computer.trace()
    );
    effects
}

fn confirm_held() -> World {
    World {
        confirm: true,
        ..World::default()
    }
}

/// A first visit, confirming every line, up to the keyboard's first pass.
fn at_keyboard() -> Computer {
    let mut computer = terminal();
    until(&mut computer, confirm_held(), 2000, |c| {
        c.c_mode == 5 && c.keyboard.shown
    });
    computer
}

/// One pass with `world`, its effects.
fn press(computer: &mut Computer, world: World) -> Vec<Effect> {
    computer.tick(&world)
}

fn confirm() -> World {
    World {
        confirm_pressed: true,
        ..World::default()
    }
}

fn repeat(bits: u16) -> World {
    World {
        repeat: bits,
        ..World::default()
    }
}

/// Moves the keyboard's cursor to `(col, row)` with one press a pass.
fn go_to(computer: &mut Computer, col: i8, row: i8) {
    while computer.keyboard.row != row {
        press(computer, repeat(DOWN));
    }
    while computer.keyboard.col != col {
        press(computer, repeat(RIGHT));
    }
}

/// Types `name` on the keyboard, then the enter.
fn type_name(computer: &mut Computer, name: &[u8]) {
    for &letter in name {
        let key = letter - MOJI;
        let (col, row) = if key < 10 {
            (key - 1, 0)
        } else if key < 19 {
            (key - 10, 1)
        } else {
            (key - 19, 2)
        };
        go_to(computer, col as i8, row as i8);
        press(computer, confirm());
    }
    go_to(computer, 9, 1);
    press(computer, confirm());
}

#[test]
fn a_first_visit_opens_the_frame_and_types_the_registry_to_the_name() {
    let mut computer = terminal();
    let effects = until(&mut computer, confirm_held(), 2000, |c| c.c_mode == 4);
    assert_eq!(effects[0], Effect::FadeOff);
    assert!(effects.contains(&Effect::Cut(6)));
    assert!(effects.contains(&Effect::Sound(SOUND_OPEN)));
    let console = &computer.console;
    assert_eq!(&console.chars[0][..8], b"UMBRELLA");
    assert!(
        console
            .chars
            .iter()
            .any(|row| row.starts_with(b"Accessing....."))
    );
    // The prompt, then the keyboard's frame opens, with the open's sound,
    // over 13 passes; the fourteenth puts the cursor on A.
    let effects = until(&mut computer, confirm_held(), 200, |c| c.kb_mode == 5);
    assert!(effects.contains(&Effect::Sound(SOUND_OPEN)));
    assert!(computer.view().frame.is_some());
    assert_eq!(computer.view().keyboard, None);
    until(&mut computer, World::default(), 20, |c| c.keyboard.shown);
    assert_eq!(
        (
            computer.kb_mode,
            computer.keyboard.col,
            computer.keyboard.row
        ),
        (0, 1, 0)
    );
    let view = computer.view();
    let keyboard = view.keyboard.expect("the keyboard is drawn");
    assert_eq!(keyboard.picture, [0x30, 0x8E, 0xF7, 0x57, 0, 0x1E]);
    assert_eq!(keyboard.cursor, [0x4C, 0x9A, 0x18, 0x17, 0x1C, 0x2A]);
}

#[test]
fn the_keys_are_the_letters_bs_esc_and_the_tall_enter() {
    let mut computer = at_keyboard();
    let mut keys = Vec::new();
    for row in 0..3 {
        for col in 0..10 {
            computer.keyboard.col = col;
            computer.keyboard.row = row;
            keys.push(computer.keyboard(&confirm(), &mut Vec::new()));
        }
    }
    let mut expected = vec![KEY_ESC];
    expected.extend(2..=9);
    expected.push(KEY_BACK);
    expected.extend(10..=18);
    expected.push(KEY_ENTER);
    expected.extend(19..=27);
    expected.push(KEY_ENTER);
    assert_eq!(keys, expected);
    // `Get_moji_code`: 2 is A, 27 is Z.
    assert_eq!((2 + MOJI, 27 + MOJI), (b'A', b'Z'));
}

#[test]
fn the_cursor_wraps_and_up_skips_the_enters_top() {
    let mut computer = at_keyboard();
    let mut effects = Vec::new();
    computer.keyboard.col = 9;
    computer.keyboard.row = 2;
    computer.keyboard(&repeat(UP), &mut effects);
    assert_eq!((computer.keyboard.col, computer.keyboard.row), (9, 0));
    computer.keyboard(&repeat(UP), &mut effects);
    assert_eq!(computer.keyboard.row, 2);
    computer.keyboard(&repeat(DOWN), &mut effects);
    assert_eq!(computer.keyboard.row, 0);
    computer.keyboard(&repeat(RIGHT), &mut effects);
    assert_eq!(computer.keyboard.col, 0);
    computer.keyboard(&repeat(LEFT), &mut effects);
    assert_eq!(computer.keyboard.col, 9);
    assert_eq!(
        effects
            .iter()
            .filter(|e| **e == Effect::Sound(SOUND_MOVE))
            .count(),
        5
    );
    assert_eq!(
        effects.iter().filter(|e| **e == Effect::PadRepeat).count(),
        5
    );
    // The enter's cursor is two keys tall.
    computer.keyboard.row = 1;
    computer.keyboard(&World::default(), &mut effects);
    assert_eq!(
        computer.keyboard.cursor,
        [0x10C, 0xB2, 0x18, 0x2F, 0xDC, 0x42]
    );
}

#[test]
fn the_cursor_pulses_between_0x40_and_0x60() {
    let mut computer = at_keyboard();
    let greys: Vec<u8> = (0..20)
        .map(|_| {
            computer.keyboard(&World::default(), &mut Vec::new());
            computer.keyboard.pulse
        })
        .collect();
    assert_eq!(greys.iter().max(), Some(&0x60));
    assert_eq!(greys.iter().min(), Some(&0x40));
    assert!(greys.windows(2).all(|pair| pair[0].abs_diff(pair[1]) == 4));
}

#[test]
fn a_letter_shows_its_key_pressed_on_the_next_pass_and_bs_takes_it_back() {
    let mut computer = at_keyboard();
    let (col, row) = (computer.console.col, computer.console.row);
    let effects = press(&mut computer, confirm());
    assert!(effects.contains(&Effect::Sound(SOUND_KEY)));
    assert_eq!(
        computer.console.chars[usize::from(row)][usize::from(col)],
        b'A'
    );
    assert_eq!((computer.console.col, computer.keyboard.len), (col + 1, 1));
    press(&mut computer, World::default());
    assert_eq!(computer.keyboard.cursor[3], 0x16, "pressed for one pass");
    press(&mut computer, World::default());
    assert_eq!(computer.keyboard.cursor[3], 0x17);
    go_to(&mut computer, 9, 0);
    press(&mut computer, confirm());
    assert_eq!(
        computer.console.chars[usize::from(row)][usize::from(col)],
        b' '
    );
    assert_eq!((computer.console.col, computer.keyboard.len), (col, 0));
    // A BS with nothing typed does nothing.
    press(&mut computer, confirm());
    assert_eq!(computer.console.col, col);
}

#[test]
fn a_name_takes_eight_letters_at_most() {
    let mut computer = at_keyboard();
    for _ in 0..10 {
        press(&mut computer, confirm());
    }
    assert_eq!(computer.keyboard.len, 8);
}

#[test]
fn a_wrong_name_is_refused_and_asked_again() {
    let mut computer = at_keyboard();
    type_name(&mut computer, b"LEON");
    assert_eq!((computer.c_mode, computer.kb_mode), (6, 2));
    assert_eq!(computer.keyboard.name(), b"LEON");
    // The keyboard's frame closes with its sound before the refusal types.
    let effects = until(&mut computer, World::default(), 20, |c| c.kb_mode == 0);
    assert!(effects.contains(&Effect::Sound(SOUND_CLOSE)));
    assert!(!computer.keyboard.shown);
    until(&mut computer, confirm_held(), 500, |c| {
        c.c_mode == 5 && c.keyboard.shown
    });
    assert!(
        computer
            .console
            .chars
            .iter()
            .any(|row| row.starts_with(b"researcher data base."))
    );
}

#[test]
fn guest_registers_and_leaves_with_the_frame_up() {
    let mut computer = at_keyboard();
    type_name(&mut computer, b"GUEST");
    assert_eq!(computer.c_mode, 7);
    assert!(
        computer.console.chars.iter().all(|row| row == &[b' '; 30]),
        "cleared"
    );
    let effects = until(&mut computer, confirm_held(), 500, |c| c.done());
    let at = |effect| effects.iter().position(|e| *e == effect);
    assert!(at(Effect::Registered(true)) < at(Effect::Resume));
    assert!(at(Effect::Common(0x5B)).is_some());
    assert!(at(Effect::Cut(0)).is_some(), "the saved cut comes back");
    // `Computer_exit(0)`: no close, so no close sound after the keyboard's.
    let closes = effects
        .iter()
        .filter(|e| **e == Effect::Sound(SOUND_CLOSE))
        .count();
    assert_eq!(closes, 1);
}

#[test]
fn esc_or_cancel_leaves_through_the_frames_close() {
    for world in [
        World {
            cancel_pressed: true,
            ..World::default()
        },
        World::default(),
    ] {
        let mut computer = at_keyboard();
        press(&mut computer, confirm());
        if !world.cancel_pressed {
            // ESC, the top left key.
            go_to(&mut computer, 0, 0);
            press(&mut computer, confirm());
        } else {
            press(&mut computer, world);
        }
        assert_eq!((computer.c_mode, computer.kb_mode), (0xB, 2));
        assert_eq!(computer.keyboard.key, 0);
        let effects = until(&mut computer, World::default(), 100, |c| c.done());
        let closes = effects
            .iter()
            .filter(|e| **e == Effect::Sound(SOUND_CLOSE))
            .count();
        assert_eq!(closes, 2, "the keyboard's, then the console's");
        assert!(!effects.contains(&Effect::Registered(true)));
    }
}

#[test]
fn the_second_visit_prints_its_first_line_at_once_and_closes_the_registration() {
    let mut computer = terminal();
    let registered = World {
        registered: true,
        ..World::default()
    };
    let effects = until(&mut computer, registered, 10, |c| c.c_mode == 8);
    assert!(!effects.contains(&Effect::Sound(SOUND_OPEN)), "no frame");
    assert_eq!(&computer.console.chars[0][..8], b"Register");
    assert_eq!((computer.console.col, computer.console.row), (0, 1));
    let held = World {
        confirm: true,
        ..registered
    };
    let effects = until(&mut computer, held, 2000, |c| c.done());
    let at = |effect| effects.iter().position(|e| *e == effect);
    assert!(at(Effect::Registered(false)) < at(Effect::Sound(SOUND_CLOSE)));
    assert!(at(Effect::Sound(SOUND_CLOSE)) < at(Effect::Resume));
}

#[test]
fn the_trace_adds_the_keyboard_in_room_613() {
    let computer = at_keyboard();
    let trace = computer.trace();
    assert!(trace.ends_with(",0,1,0,0,0"), "{trace}");
    assert_eq!(trace.split(',').count(), 16);
}

#[test]
fn a_pack_without_the_terminals_strings_resumes_at_once() {
    let mut strings = test_strings();
    strings.terminal = Default::default();
    let mut computer = Computer::new(REQUEST, strings);
    let effects = computer.tick(&World::default());
    assert!(computer.done());
    assert!(effects.contains(&Effect::Resume));
}

/// `+0x2D9` isn't cleared by a new work: the second visit finds the enter
/// the first left, and a work whose memory holds ESC leaves at once.
#[test]
fn the_key_byte_outlives_the_work() {
    let mut computer = at_keyboard();
    type_name(&mut computer, b"GUEST");
    until(&mut computer, confirm_held(), 500, |c| c.done());
    assert_eq!(computer.key(), Some(KEY_ENTER));
    let second = terminal().with_key(computer.key());
    assert!(second.trace().ends_with(",28,0"), "{}", second.trace());
    assert_eq!(terminal().with_key(None).key(), None);

    let mut stale = terminal().with_key(Some(KEY_ESC));
    let effects = until(&mut stale, World::default(), 100, |c| c.done());
    // One character typed (the type sound is the move's slot), then the
    // close.
    let typed = effects
        .iter()
        .filter(|e| **e == Effect::Sound(SOUND_MOVE))
        .count();
    assert_eq!(typed, 1);
    assert!(effects.contains(&Effect::Sound(SOUND_CLOSE)));
    assert_eq!(stale.key(), Some(0));
}

/// Room 200's tail tests the same byte: an ESC the memory holds closes it
/// after its first character.
#[test]
fn a_stale_esc_closes_room_200s_console_too() {
    let mut computer = Computer::new(0xF0, test_strings()).with_key(Some(KEY_ESC));
    let effects = until(&mut computer, World::default(), 100, |c| c.done());
    let typed = effects
        .iter()
        .filter(|e| **e == Effect::Sound(SOUND_MOVE))
        .count();
    assert_eq!(typed, 1);
    assert_eq!(computer.key(), Some(0));
}

/// The last line's confirm exits without the tail: `printed` and
/// `confirmed` stay up for the next work of the mode until its open.
#[test]
fn printed_and_confirmed_outlive_a_confirmed_exit() {
    let registered = World {
        registered: true,
        confirm: true,
        ..World::default()
    };
    let mut computer = terminal();
    until(&mut computer, registered, 2000, |c| c.done());
    assert_eq!(computer.latches(), [true, true]);
    let mut next = terminal().with_latches(computer.latches());
    next.tick(&registered);
    let trace = next.trace();
    let fields: Vec<&str> = trace.split(',').collect();
    assert_eq!((fields[7], fields[8]), ("1", "1"), "{trace}");
    until(&mut next, registered, 10, |c| c.c_mode == 8);
    assert!(!next.printed && !next.confirmed);
}
