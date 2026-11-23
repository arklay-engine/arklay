use super::*;

/// Banks of `clips` clips of `frames` frames each, whose root moves `step`
/// along x a frame.
fn assets(clips: usize, frames: usize, step: i16) -> PageAssets {
    let keyframes: Vec<Keyframe> = (0..frames)
        .map(|index| Keyframe {
            offset: [0; 3],
            vertex: [step * (index as i16 + 1), 0, 0],
            rotations: Vec::new(),
        })
        .collect();
    let clip = Clip {
        frames: (0..frames)
            .map(|index| ClipFrame {
                keyframe: index as u16,
                timing: 0,
                flags: 0,
            })
            .collect(),
    };
    let body = crate::model::Emd {
        skeleton: Skeleton::default(),
        keyframes: keyframes.clone(),
        clips: vec![clip.clone(); clips],
        damage_clips: Vec::new(),
        damage_keyframes: Vec::new(),
        banks: Vec::new(),
        mesh: Default::default(),
        texture: crate::model::Texture8 {
            width: 0,
            height: 0,
            indices: Vec::new(),
            palettes: Vec::new(),
            stp: Vec::new(),
        },
        morphs: Vec::new(),
    };
    let weapon = crate::model::Emw {
        skeleton: Skeleton::default(),
        keyframes,
        clips: vec![clip; clips],
        mesh: Default::default(),
    };
    PageAssets::new(body, weapon)
}

/// Claire holding her handgun, as the title makes her.
fn claire() -> PageModel {
    PageModel::new(1, 3, Statics::default())
}

/// The model walked in, turned and settled, as state 0xB finds it.
fn settled(assets: &PageAssets) -> PageModel {
    let mut model = claire();
    model.start(assets);
    model.walk(assets);
    model.walk_on();
    model.place();
    while !model.statics.turned {
        model.face(assets);
    }
    model
}

#[test]
fn a_new_model_stands_in_its_place_with_the_executables_statics() {
    // The trace before the page first opens (title_option_key's 409).
    assert_eq!(
        claire().trace(),
        "00000000,-1530,2020,2700,3072,00000000,3,0,0,0,0,0,0,99"
    );
}

#[test]
fn the_statics_outlive_the_model() {
    let statics = Statics {
        turned: true,
        key: 0x140,
        count: 7,
        step: 3,
        kept: 2,
        row: 5,
        ..Statics::default()
    };
    let model = PageModel::new(0, 2, statics);
    assert_eq!(model.statics, statics);
    assert!(model.trace().ends_with(",2,1,0,140,7,3,2,5"));
}

#[test]
fn the_walk_in_starts_off_to_the_left_with_two_steps() {
    // `FUN_0044d990(0)` then the draw's `FUN_0044d990(1)`: the idle's snap,
    // the walk's start and a step, and a second step (title_option_key's
    // first tick on the page: `01000100`, motion `00020500`).
    let assets = assets(16, 40, 80);
    let mut model = claire();
    model.start(&assets);
    model.walk(&assets);
    assert_eq!(model.work.routine, [1, 0, 1, 0]);
    assert_eq!(
        (model.work.move_no, model.work.move_cnt, model.work.hokan),
        (0, 2, 5)
    );
    assert_eq!(model.body.angle, 0);
    assert_eq!(model.body.pos[1..], PLACE[1..]);
    assert!(model.body.pos[0] > START_X);
    // Sherry starts nearer.
    let mut sherry = PageModel::new(SHERRY, 0, Statics::default());
    sherry.start(&assets);
    assert!(sherry.body.pos[0] > model.body.pos[0]);
}

#[test]
fn state_nines_walk_keeps_stepping_with_routine_one() {
    let assets = assets(16, 40, 80);
    let mut model = claire();
    model.start(&assets);
    model.walk(&assets);
    let x = model.body.pos[0];
    model.walk_on();
    model.walk(&assets);
    assert_eq!(model.work.routine, [1, 1, 1, 0]);
    assert_eq!(model.work.move_cnt, 3);
    assert!(model.body.pos[0] > x);
    // The decide puts it in its place.
    model.place();
    assert_eq!(model.body.pos, PLACE);
}

#[test]
fn the_turn_steps_the_raw_angle_and_snaps_onto_the_facing() {
    // `FUN_00449600` from 0 by 0xD8: the trace's 472 to 476, the turn never
    // masked.
    let assets = assets(16, 40, 80);
    let mut model = claire();
    model.start(&assets);
    model.walk(&assets);
    model.walk_on();
    let mut angles = Vec::new();
    for _ in 0..5 {
        model.face(&assets);
        angles.push(model.body.angle);
    }
    assert_eq!(angles, [65320, 65104, 64888, 64672, 3072]);
    // Still the walk's routine and clip, which the idle's move steps on.
    assert_eq!(model.work.routine, [1, 1, 1, 0]);
    assert_eq!(model.work.move_no, 0);
    assert!(!model.statics.turned);
    // The next tick latches the idle: `pl+4 = 1` and the idle's start.
    model.face(&assets);
    assert!(model.statics.turned);
    assert_eq!(model.work.routine, [1, 0, 1, 0]);
    assert_eq!(
        (model.work.move_no, model.work.move_cnt, model.work.hokan),
        (2, 1, 6)
    );
    // Facing, it stays.
    model.face(&assets);
    assert_eq!(model.body.angle, FACING);
    assert_eq!(model.work.routine, [1, 0, 1, 0]);
}

#[test]
fn the_turn_goes_the_nearer_way_and_to_the_other_target() {
    let assets = assets(16, 40, 80);
    let mut model = claire();
    model.body.angle = 0x900;
    model.with(&assets, |mover, _| mover.page_turn(FACE_STEP));
    assert_eq!(model.body.angle, 0x900 + 0xD8);
    // Bit 15 of the step aims at 0x400 with the step negated.
    model.body.angle = 0x300;
    model.with(&assets, |mover, _| mover.page_turn(-0x40));
    assert_eq!(model.body.angle, 0x340);
}

#[test]
fn the_aim_row_holds_level_then_up_then_down() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    let mut keys = Vec::new();
    for _ in 0..184 {
        model.demonstrate(&assets, ROW_AIM);
        keys.push(model.statics.key);
        assert_eq!(model.body.pos[1..], PLACE[1..]);
    }
    // The entry: count 60, step 3, kept 0, row 4 (title_option_key's 526).
    let mut expected = vec![KEY_AIM; 61];
    expected.extend([KEY_AIM | KEY_TILT_UP; 61]);
    expected.extend([KEY_AIM | KEY_TILT_DOWN; 61]);
    expected.push(KEY_AIM);
    assert_eq!(keys, expected);
    // The aim's routine is up.
    assert_eq!(model.work.routine[1], MOVE_AIM);
}

#[test]
fn the_aim_rows_entry_sets_its_count() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    model.demonstrate(&assets, ROW_AIM);
    assert!(
        model.trace().ends_with(",100,60,3,0,4"),
        "{}",
        model.trace()
    );
    // The idle's branch on the aim key, then the raise's first step.
    assert_eq!(model.work.routine, [1, 5, 0, 1]);
    assert_eq!(
        (model.work.move_no, model.work.move_cnt, model.work.hokan),
        (AIM_RAISE_CLIP, 1, 6)
    );
}

#[test]
fn the_fire_row_holds_the_action_key_then_aims_and_fires() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    let mut keys = Vec::new();
    for _ in 0..220 {
        model.demonstrate(&assets, ROW_FIRE);
        keys.push(model.statics.key);
    }
    let mut expected = vec![KEY_ACTION; 25];
    expected.extend([0; 30]);
    expected.push(0);
    expected.extend([KEY_AIM; 49]);
    expected.extend([KEY_AIM | KEY_FIRE; 0x4A]);
    expected.extend([0; 40]);
    expected.push(KEY_ACTION);
    assert_eq!(keys, expected);
}

#[test]
fn the_fire_rows_waits_follow_the_whole_type_word() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    // A `Type` with bits above 0xFFF matches none of the row's cases.
    model.work.kind = 0x1008;
    model.statics.row = ROW_FIRE;
    model.statics.step = 2;
    model.statics.count = 1;
    model.demonstrate(&assets, ROW_FIRE);
    assert_eq!((model.statics.step, model.statics.count), (3, 0x4A));
    model.work.kind = 8;
    model.statics.step = 2;
    model.statics.count = 1;
    model.demonstrate(&assets, ROW_FIRE);
    assert_eq!((model.statics.step, model.statics.count), (3, 0x5E));
}

#[test]
fn the_backward_row_walks_a_tick_then_backs_away() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    model.demonstrate(&assets, ROW_BACKWARD);
    assert_eq!(model.statics.key, KEY_FORWARD);
    assert_eq!(model.work.routine[1], MOVE_FORWARD);
    model.demonstrate(&assets, ROW_BACKWARD);
    assert_eq!(model.statics.key, KEY_BACKWARD);
    // The walk's branch lets go first: the idle.
    assert_eq!(model.work.routine, [1, 0, 1, 0]);
    model.demonstrate(&assets, ROW_BACKWARD);
    assert_eq!(model.work.routine[1], MOVE_BACKWARD);
    assert_eq!(model.work.bank, Bank::Pld);
    // In place: no `Add_speedXZ`.
    assert_eq!(model.body.pos, PLACE);
}

#[test]
fn the_turn_rows_turn_unmasked() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    model.demonstrate(&assets, ROW_LEFT);
    assert_eq!(model.work.routine[1], MOVE_ROTATE);
    assert_eq!(model.body.angle, FACING);
    model.demonstrate(&assets, ROW_LEFT);
    assert_eq!(model.body.angle, FACING - 0x50);
    // Right from the left's turn keeps turning; past zero it isn't masked.
    model.body.angle = 0x20;
    model.demonstrate(&assets, ROW_LEFT);
    assert_eq!(model.body.angle as i16, 0x20 - 0x50);
    model.demonstrate(&assets, ROW_RIGHT);
    assert_eq!(model.body.angle, 0x20);
}

#[test]
fn a_move_onto_a_turned_row_turns_back_first() {
    let assets = assets(16, 40, 80);
    let mut model = settled(&assets);
    for _ in 0..4 {
        model.demonstrate(&assets, ROW_LEFT);
    }
    let row = model.statics.row;
    model.turn_back();
    // The turn back returns before the row: its statics stand.
    model.demonstrate(&assets, ROW_BACKWARD);
    assert_eq!(model.body.angle, FACING - 0x50 * 3 + 0xD8);
    assert_eq!(model.statics.row, row);
    assert!(model.statics.returning);
    model.demonstrate(&assets, ROW_BACKWARD);
    assert_eq!(model.body.angle, FACING);
    assert!(model.statics.returning);
    // Facing, it goes on to the row in the same tick.
    model.demonstrate(&assets, ROW_BACKWARD);
    assert!(!model.statics.returning);
    assert_eq!(model.statics.row, ROW_BACKWARD);
}

#[test]
fn the_action_key_crouches_until_it_is_let_go() {
    let assets = assets(16, 6, 80);
    let mut model = settled(&assets);
    // The handgun's lowering then the idle's branch on `(char)0x80`.
    model.work.routine = [1, 0, 1, 0];
    model.statics.row = ROW_FIRE;
    model.statics.step = 0;
    model.statics.count = 0x37;
    model.demonstrate(&assets, ROW_FIRE);
    assert_eq!(model.work.routine, [1, MOVE_ITEM, 1, 0]);
    assert_eq!(
        (model.work.bank, model.work.move_no),
        (Bank::Pld, ITEM_CLIP)
    );
    while model.work.routine[2] < 3 {
        model.demonstrate(&assets, ROW_FIRE);
    }
    // Held, it waits down.
    model.demonstrate(&assets, ROW_FIRE);
    assert_eq!(model.work.routine[2], 3);
    // The run's keys have no action bit: up again, backward.
    model.demonstrate(&assets, ROW_RUN);
    assert_eq!(model.work.routine[2], 5);
    while model.work.routine[1] == MOVE_ITEM {
        model.demonstrate(&assets, ROW_RUN);
    }
    assert_eq!(model.work.routine, [1, 0, 0, 0]);
}

#[test]
fn the_lowering_stops_the_planting_past_its_tenth_tick() {
    let assets = assets(16, 30, 0);
    let mut model = settled(&assets);
    model.work.routine = [1, MOVE_AIM, 3, 0];
    model.work.move_no = 0xB;
    let mut speeds = Vec::new();
    for _ in 0..13 {
        model.demonstrate(&assets, ROW_FORWARD);
        speeds.push(model.work.spd[0]);
    }
    // Its first tick counts 1 to 2; the tick that counts past 11 sets 1.
    assert_eq!(speeds[..10], [0; 10]);
    assert_eq!(speeds[10..], [1; 3]);
    assert_eq!(model.work.routine[2..], [3, 14]);
}

#[test]
fn the_raise_tilts_on_the_tilt_keys_past_its_frames() {
    let assets = assets(16, 40, 0);
    let mut model = settled(&assets);
    model.work.routine = [1, MOVE_AIM, 0, 0];
    model.statics.row = ROW_AIM;
    model.statics.step = 1;
    // Up: past frame 7 of the raise (`0x53A305`'s 0x87).
    let mut ticks = 0;
    while model.work.routine[2] == 0 {
        model.demonstrate(&assets, ROW_AIM);
        ticks += 1;
    }
    assert_eq!(ticks, 8);
    assert_eq!(model.work.status_flg & 0xE000, TILT_UP);
    assert_eq!(model.work.routine[2..], [1, 0]);
    // The hold's clip for the tilt.
    model.demonstrate(&assets, ROW_AIM);
    assert_eq!(model.work.move_no, AIM_HOLD_CLIPS[2]);
}
