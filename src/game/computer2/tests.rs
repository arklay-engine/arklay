use super::*;

fn strings() -> Strings {
    test_strings()
}

/// Cprint calls until the end, with the sounds of each.
fn calls(bytes: &[u8], rate: u8) -> (usize, usize, Console) {
    let strings = Strings {
        title: bytes.to_vec(),
        ..Strings::default()
    };
    let mut console = Console::default();
    let mut printed = false;
    let mut sounds = 0;
    for call in 1..1000 {
        let mut effects = Vec::new();
        if cprint(
            &mut console,
            &strings,
            rate,
            Text::Title,
            &mut printed,
            &mut effects,
        ) {
            return (call, sounds, console);
        }
        sounds += effects.len();
    }
    panic!("no end");
}

#[test]
fn cprint_types_one_character_a_tick() {
    let (calls, sounds, console) = calls(b"AB", 1);
    assert_eq!((calls, sounds), (3, 2));
    assert_eq!((console.col, console.row), (2, 0));
    assert_eq!(&console.chars[0][..2], b"AB");
}

#[test]
fn cprint_newline_and_colour_share_the_next_characters_tick() {
    let strings = Strings {
        title: b"A]|2B".to_vec(),
        ..Strings::default()
    };
    let mut console = Console::default();
    let mut printed = false;
    let mut first = Vec::new();
    cprint(
        &mut console,
        &strings,
        1,
        Text::Title,
        &mut printed,
        &mut first,
    );
    let mut second = Vec::new();
    cprint(
        &mut console,
        &strings,
        1,
        Text::Title,
        &mut printed,
        &mut second,
    );
    assert_eq!((first.len(), second.len()), (1, 3));
    assert_eq!((console.chars[1][0], console.attrs[1][0]), (b'B', 2));
    assert_eq!(console.colour, 2);
}

#[test]
fn cprint_delay_holds_n_ticks() {
    // `[025` takes a tick, holds 24 more, then the dot prints.
    let (calls_a, _, _) = calls(b"A[025.", 1);
    assert_eq!(calls_a, 28);
    // The real strings' call counts, the end's included.
    let real = strings();
    for (bytes, expected) in [
        (&real.title, 48),
        (&real.released, 31),
        (&real.locked, 29),
        (&real.card, 40),
        (&real.checking, 124),
        (&real.ok, 34),
    ] {
        assert_eq!(
            calls(bytes, 1).0,
            expected,
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn cprint_wraps_at_thirty_columns_and_rolls_up_after_row_eleven() {
    let (_, _, console) = calls(&[b'x'; 31], 1);
    assert_eq!((console.col, console.row), (1, 1));
    let mut text = b"top".to_vec();
    text.extend([b']'; 13]);
    let (_, _, console) = calls(&text, 0);
    assert_eq!(console.row, 11);
    assert_eq!(console.chars[0], [b' '; COLUMNS]);
}

#[test]
fn cprint_rate_zero_prints_the_rest_at_once() {
    let strings = strings();
    let mut console = Console::default();
    let mut printed = false;
    let mut effects = Vec::new();
    assert!(cprint(
        &mut console,
        &strings,
        0,
        Text::Newline,
        &mut printed,
        &mut effects
    ));
    assert!(printed);
    assert_eq!(effects, [Effect::Sound(SOUND_TYPE)]);
}

#[test]
fn wframe_opens_in_thirteen_ticks_and_closes() {
    let mut computer = Computer::new(0xF0, strings());
    for s in 0..=12 {
        assert!(computer.wframe_move());
        assert_eq!(
            computer.view.frame,
            Some([12, 10, 12 + 21 * s, 10 + 11 * s])
        );
    }
    assert!(!computer.wframe_move());
    assert_eq!(computer.step, 0);
    computer.frame_dir = 1;
    let mut rects = Vec::new();
    while computer.wframe_move() {
        rects.push(computer.view.frame.unwrap());
    }
    assert_eq!(rects.len(), 13);
    assert_eq!(rects[0], [12, 10, 264, 149]);
    assert_eq!(rects[1], [12, 10, 243, 138]);
    assert_eq!(rects[2], [12, 10, 222, 126]);
    assert_eq!(rects[12], [12, 10, 12, 10]);
}

/// Drive the computer from W0 with the world `at` gives each tick (W0 is
/// tick 0), and note every tick's effects, `c_mode` and `R1`.
fn run(at: impl Fn(usize, &[Effect]) -> World) -> Vec<(Vec<Effect>, u8, u8, bool)> {
    let mut computer = Computer::new(0xF0, strings());
    let mut ticks = Vec::new();
    let mut all = Vec::new();
    for tick in 0..3000 {
        let mut world = at(tick, &all);
        // The cut on screen is the last one the computer asked for.
        world.cut = all
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::Cut(cut) => Some(*cut),
                _ => None,
            })
            .unwrap_or(world.cut);
        let effects = computer.tick(&world);
        all.extend(effects.iter().copied());
        ticks.push((effects, computer.c_mode, computer.r1, computer.view.drawn));
        if computer.done() {
            break;
        }
    }
    ticks
}

/// The first tick (from W0) whose state satisfies `pred`.
fn first(
    ticks: &[(Vec<Effect>, u8, u8, bool)],
    pred: impl Fn(&(Vec<Effect>, u8, u8, bool)) -> bool,
) -> usize {
    ticks.iter().position(pred).expect("never")
}

/// The original's run at the counter: event 6's yes on tick 400, so W0 is
/// 402, and an action press held 4 ticks every 30 from 400. The message
/// opened on 581 and closed with yes on the press at 640. Every change of
/// `c_mode` and `R1`, and the cuts and the close, on the original's ticks.
#[test]
fn computer_with_the_card_unlocks_and_takes_it_on_the_originals_ticks() {
    const W0: usize = 402;
    let ticks = run(|tick, _| {
        let t = tick + W0;
        World {
            confirm: t >= 400 && (t - 400) % 30 < 4,
            card: true,
            message_open: (582..=640).contains(&t),
            cut: 0,
            ..World::default()
        }
    });
    let at = |t: usize| first(&ticks, |s| s.1 == t as u8) + W0;
    assert_eq!(ticks[0].0, [Effect::FadeOff, Effect::Cut(15)]);
    assert_eq!(ticks[1].0, [Effect::Sound(SOUND_OPEN)]);
    assert_eq!(ticks[14].0, [Effect::Sound(SOUND_TYPE)]);
    assert_eq!(at(1), 416);
    assert_eq!(at(2), 490);
    assert_eq!(at(3), 520);
    assert_eq!(at(4), 580);
    assert_eq!(at(5), 581);
    assert!(ticks[581 - W0].0.contains(&Effect::AskCard));
    assert_eq!(at(6), 641);
    assert_eq!(ticks[641 - W0].0, [Effect::Unlock, Effect::TakeCard]);
    assert_eq!(at(7), 765);
    assert_eq!(ticks[766 - W0].0, [Effect::Cut(CUT_DOOR_1)]);
    assert_eq!(ticks[826 - W0].0, [Effect::Sound(SOUND_DOOR)]);
    assert_eq!(ticks[886 - W0].0, [Effect::Cut(CUT_DOOR_2)]);
    assert_eq!(ticks[946 - W0].0, [Effect::Sound(SOUND_DOOR)]);
    assert_eq!(ticks[1006 - W0].0, [Effect::Cut(15)]);
    assert_eq!(at(8), 1006);
    assert_eq!(at(0xB), 1060);
    assert_eq!(ticks[1061 - W0].0, [Effect::Sound(SOUND_CLOSE)]);
    assert_eq!(ticks.len() - 1 + W0, 1074);
    assert_eq!(ticks.last().unwrap().0, [Effect::Cut(0), Effect::Resume]);
    // The console is drawn only while text types or waits.
    let drawn = |t: usize| ticks[t - W0].3;
    assert!(!drawn(402) && !drawn(410) && drawn(416) && drawn(765));
    assert!(!drawn(766) && !drawn(1005) && drawn(1006) && drawn(1060));
    assert!(!drawn(1061) && !drawn(1073));
}

#[test]
fn confirm_before_the_end_does_not_advance() {
    // Held from the start: the title's end is W61, and c_mode changes on
    // W62, the first tick after it.
    let ticks = run(|_, _| World {
        confirm: true,
        card: true,
        ..World::default()
    });
    assert_eq!(first(&ticks, |s| s.1 == 2), 62);
    assert_eq!(first(&ticks, |s| s.1 == 3), 92);
    assert_eq!(first(&ticks, |s| s.1 == 4), 133);
}

#[test]
fn checking_needs_no_confirm() {
    let ticks = run(|tick, effects| World {
        confirm: !effects.contains(&Effect::AskCard),
        card: true,
        message_open: effects.contains(&Effect::AskCard) && tick < 140,
        ..World::default()
    });
    let m = first(&ticks, |s| s.1 == 6);
    assert_eq!(first(&ticks, |s| s.1 == 7), m + 124);
}

#[test]
fn no_card_exits_without_asking() {
    let ticks = run(|_, _| World {
        confirm: true,
        ..World::default()
    });
    let all: Vec<Effect> = ticks.iter().flat_map(|s| s.0.iter().copied()).collect();
    assert!(!all.contains(&Effect::AskCard) && !all.contains(&Effect::Unlock));
    assert_eq!(ticks[135].0, [Effect::Sound(SOUND_TYPE)]);
    assert_eq!(ticks[136].0, [Effect::Sound(SOUND_CLOSE)]);
    assert_eq!(ticks.len() - 1, 149);
}

#[test]
fn already_released_prints_released_and_exits() {
    let ticks = run(|_, _| World {
        confirm: true,
        unlocked: true,
        card: true,
        ..World::default()
    });
    assert_eq!(first(&ticks, |s| s.1 == 0xB), 94);
    assert_eq!(ticks[95].0, [Effect::Sound(SOUND_CLOSE)]);
    assert_eq!(ticks.len() - 1, 95 + 13);
}

#[test]
fn refused_keeps_the_card() {
    let ticks = run(|tick, effects| World {
        confirm: !effects.contains(&Effect::AskCard),
        card: true,
        message_open: effects.contains(&Effect::AskCard) && tick < 140,
        message_no: true,
        ..World::default()
    });
    let all: Vec<Effect> = ticks.iter().flat_map(|s| s.0.iter().copied()).collect();
    assert!(!all.contains(&Effect::TakeCard) && !all.contains(&Effect::Unlock));
    let m = first(&ticks, |s| s.1 == 0xB);
    assert_eq!(ticks[m].0, [Effect::Sound(SOUND_TYPE)]);
    assert_eq!(ticks.len() - 1, m + 1 + 13);
}

#[test]
fn scenario_b_shows_three_doors_and_key17_starts_at_door_two() {
    for (key17, cuts) in [
        (false, vec![CUT_DOOR_1, CUT_DOOR_2, CUT_DOOR_B]),
        (true, vec![CUT_DOOR_2, CUT_DOOR_B]),
    ] {
        let ticks = run(|tick, effects| World {
            confirm: !effects.contains(&Effect::AskCard) || tick > 150,
            scenario_b: true,
            key17,
            card: true,
            message_open: effects.contains(&Effect::AskCard) && tick < 140,
            cut: 0,
            ..World::default()
        });
        let shown: Vec<u8> = ticks
            .iter()
            .flat_map(|s| s.0.iter())
            .filter_map(|e| match e {
                Effect::Cut(cut) if ![15, 0].contains(cut) => Some(*cut),
                _ => None,
            })
            .collect();
        assert_eq!(shown, cuts);
    }
}

#[test]
fn other_modes_resume_at_once() {
    let mut computer = Computer::new(0x61, strings());
    assert_eq!(
        computer.tick(&World::default()),
        [Effect::FadeOff, Effect::Resume]
    );
    assert!(computer.done());
}

#[test]
fn windows_turns_fade_0_off_on_its_first_pass_only() {
    let mut computer = Computer::new(0xF0, strings());
    let first = computer.tick(&World::default());
    assert_eq!(first.first(), Some(&Effect::FadeOff));
    for _ in 0..40 {
        assert!(!computer.tick(&World::default()).contains(&Effect::FadeOff));
    }
}

#[test]
fn trace_field_on_w0_and_w14() {
    let mut computer = Computer::new(0xF0, strings());
    computer.tick(&World::default());
    assert_eq!(computer.trace(), "1,7,1,1,0,0,0,0,0,0,0");
    for _ in 1..=14 {
        computer.tick(&World::default());
    }
    // The original's line 416 (W14).
    assert_eq!(computer.trace(), "1,0,0,1,1,0,0,0,0,1,0");
}
