use super::*;

/// Banks of `clips` clips of `frames` frames each, whose root moves `step`
/// along x a frame.
fn assets(clips: usize, frames: usize, step: i16) -> PageAssets {
    banks(clips, frames, step, Skeleton::default(), |_| Vec::new())
}

/// [`assets`] on a skeleton whose legs reach out along x (the handguns'
/// foot chain, parts 0, 1, 5, 6 and 7, and the other leg's), turned by
/// part 1 a little more each frame, so the foot moves through a clip.
fn posed_assets(clips: usize, frames: usize) -> PageAssets {
    let skeleton = Skeleton {
        relative: vec![
            [0; 3],
            [0, -100, 0],
            [100, 0, 0],
            [100, 0, 0],
            [100, 0, 0],
            [-100, 0, 0],
            [-100, 0, 0],
            [-100, 0, 0],
        ],
        children: vec![
            vec![1],
            vec![2, 5],
            vec![3],
            vec![4],
            vec![],
            vec![6],
            vec![7],
            vec![],
        ],
    };
    banks(clips, frames, 0, skeleton, |index| {
        let mut rotations = vec![[0; 3]; 8];
        rotations[1] = [0, index as i16 * 0x40, 0];
        rotations
    })
}

fn banks(
    clips: usize,
    frames: usize,
    step: i16,
    skeleton: Skeleton,
    rotations: impl Fn(usize) -> Vec<[i16; 3]>,
) -> PageAssets {
    let keyframes: Vec<Keyframe> = (0..frames)
        .map(|index| Keyframe {
            offset: [0; 3],
            vertex: [step * (index as i16 + 1), 0, 0],
            rotations: rotations(index),
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
        marni: None,
        skeleton: skeleton.clone(),
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
        skeleton,
        keyframes,
        clips: vec![clip; clips],
        mesh: Default::default(),
    };
    PageAssets::new(body, weapon)
}

/// Claire holding her handgun, as the title makes her.
fn claire() -> PageModel {
    PageModel::new(1, 3, Statics::default(), HandgunFrame::default())
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
    let model = PageModel::new(0, 2, statics, HandgunFrame::default());
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
    let mut sherry = PageModel::new(SHERRY, 0, Statics::default(), HandgunFrame::default());
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
    let assets = assets(16, 6, 80);
    let mut model = settled(&assets);
    let mut keys = Vec::new();
    let mut states = Vec::new();
    for _ in 0..220 {
        model.demonstrate(&assets, ROW_FIRE);
        keys.push(model.statics.key);
        let w = &model.work;
        states.push((w.routine[1], w.routine[2], w.bank, w.move_no));
    }
    // The action key crouches on `Main`'s clip 6, and its release stands
    // the model up again before the aim.
    assert_eq!(states[0], (MOVE_ITEM, 1, Bank::Pld, ITEM_CLIP));
    assert_eq!(states[55].0, MOVE_IDLE);
    // The aim key raises the gun: the raise, then the level hold.
    assert_eq!(states[56], (MOVE_AIM, 0, Bank::Weapon, AIM_RAISE_CLIP));
    assert_eq!(states[104], (MOVE_AIM, 1, Bank::Weapon, AIM_HOLD_CLIPS[1]));
    // The fire key fires the level shot, which goes back to the hold at its
    // end and fires again, for as long as it's held.
    // (the hold hands over and returns; the shot starts its clip next tick).
    assert_eq!(states[105], (MOVE_AIM, 2, Bank::Weapon, AIM_HOLD_CLIPS[1]));
    assert_eq!(states[106], (MOVE_AIM, 2, Bank::Weapon, AIM_FIRE_CLIPS[1]));
    let shots = states[105..179]
        .windows(2)
        .filter(|pair| pair[0].1 != 2 && pair[1].1 == 2)
        .count();
    assert!(shots > 3, "{shots} shots");
    // Let go, the shot plays out, then it lowers (the raise backward) and
    // stands.
    assert!(
        states[179..190]
            .iter()
            .any(|state| state.1 == 3 && state.3 == AIM_RAISE_CLIP)
    );
    assert_eq!(states[218].0, MOVE_IDLE);
    // The next cycle crouches again.
    assert_eq!(states[219].0, MOVE_ITEM);
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
    let assets = posed_assets(16, 30);
    let mut model = settled(&assets);
    model.work.routine = [1, MOVE_AIM, 3, 0];
    model.work.move_no = 0xB;
    let mut speeds = Vec::new();
    let mut places = Vec::new();
    for _ in 0..13 {
        model.demonstrate(&assets, ROW_FORWARD);
        speeds.push(model.work.spd[0]);
        places.push(model.body.pos);
    }
    // Its first tick counts 1 to 2; the tick that counts past 11 sets 1.
    assert_eq!(speeds[..10], [0; 10]);
    assert_eq!(speeds[10..], [1; 3]);
    assert_eq!(model.work.routine[2..], [3, 14]);
    // Until then the foot stays where the last draw had it, so the body
    // moves off its place as the leg turns; from then it stays in its place.
    assert!(
        places[..10].iter().all(|place| *place != PLACE),
        "{places:?}"
    );
    assert_eq!(places[10..], [PLACE; 3]);
}

#[test]
fn the_planting_keeps_the_drawn_foot_where_it_was() {
    let assets = posed_assets(16, 30);
    let mut model = settled(&assets);
    model.work.routine = [1, MOVE_AIM, 3, 0];
    model.work.move_no = 0xB;
    let chain = page_foot_chain(model.id, model.frames);
    model.demonstrate(&assets, ROW_FORWARD);
    let drawn = model.drawn_feet[0].unwrap();
    model.demonstrate(&assets, ROW_FORWARD);
    // This tick's foot, at the place the planting moved the body to, is
    // the last draw's, to the x and z.
    let now = model
        .with(&assets, |mover, _| mover.chain_position(chain))
        .unwrap();
    assert_eq!([now[0], now[2]], [drawn[0], drawn[2]]);
    assert_ne!(model.body.pos, PLACE);
}

#[test]
fn a_later_option_visit_keeps_the_routine_and_motion_bytes() {
    let assets = assets(16, 40, 80);
    let mut first = settled(&assets);
    first.demonstrate(&assets, ROW_AIM);
    let routine = first.work.routine;
    let motion = (first.work.move_no, first.work.move_cnt, first.work.hokan);
    // OPTION left and entered again: Leon this time, placed and facing.
    let again = first.again(0, 2, HandgunFrame::default());
    assert_eq!((again.id, again.work.kind), (0, 2));
    assert_eq!((again.body.pos, again.body.angle), (PLACE, FACING));
    assert_eq!(again.work.routine, routine);
    assert_eq!(
        (again.work.move_no, again.work.move_cnt, again.work.hokan),
        motion
    );
    assert_eq!(again.statics, first.statics);
    assert!(
        again
            .trace()
            .starts_with("01050001,-1530,2020,2700,3072,09")
    );
}

#[test]
fn the_statics_survive_as_two_words() {
    for statics in [
        Statics::default(),
        Statics {
            turned: true,
            returning: true,
            key: 0x201,
            count: -3,
            step: 6,
            kept: 2,
            row: 5,
            last_key: 0x140,
        },
    ] {
        assert_eq!(Statics::from_words(statics.to_words()), statics);
    }
}

#[test]
fn leons_alternates_hold_eights_handgun_and_its_frame() {
    assert_eq!(plw_holder(0xA, 2), 8);
    assert_eq!(plw_holder(8, 2), 8);
    // `0x4D3010`: the handgun's first byte is 0x85, or 0x03 for 8 and 0xA.
    let fresh = HandgunFrame::default();
    assert_eq!(page_frames(2, fresh.equip(0, 2)), [0x85, 0x87, 8]);
    assert_eq!(page_frames(3, fresh.equip(1, 3)), [0x85, 0x87, 8]);
    assert_eq!(page_frames(2, fresh.equip(8, 2)), [0x03, 0x87, 8]);
    assert_eq!(page_frames(2, fresh.equip(0xA, 2)), [0x03, 0x87, 8]);
    let frames = |id| page_frames(2, fresh.equip(id, 2));
    assert_eq!(page_foot_chain(0, frames(0)), &AIM_FOOT_CHAIN);
    assert_eq!(page_foot_chain(8, frames(8)), &AIM_OTHER_FOOT_CHAIN);
    assert_eq!(page_foot_chain(0xA, frames(0xA)), &AIM_OTHER_FOOT_CHAIN);
}

#[test]
fn pl_weapon_ch_reads_each_characters_plw_from_its_holders_folder() {
    // `0x4D2D20`: even Ids up to 0xA are Leon's (0), odd ones up to 9
    // Claire's (1), the rest their own; 8 and 0xA hold 8's handgun.
    for (id, holder) in [
        (0, 0),
        (1, 1),
        (2, 0),
        (3, 1),
        (4, 0),
        (5, 1),
        (6, 0),
        (7, 1),
        (8, 0),
        (9, 1),
        (0xA, 0),
        (0xB, 0xB),
        (0xC, 0xC),
        (0xD, 0xD),
        (ADA, ADA),
        (SHERRY, SHERRY),
    ] {
        assert_eq!(plw_holder(id, 3), holder, "id {id:#x}");
    }
    assert_eq!(plw_holder(8, 2), 8);
    assert_eq!(plw_holder(0xA, 2), 8);
    assert_eq!(plw_holder(4, 2), 0);
    assert_eq!(plw_holder(0xB, 2), 0xB);
}

#[test]
fn the_handguns_first_frame_carries_from_one_load_to_the_next() {
    let fresh = HandgunFrame::default();
    assert_eq!(fresh, HandgunFrame(0x85));
    // 8 or 0xA's handgun, then Leon's: 0x03 ORed with 0x80.
    let after = fresh.equip(8, 2).equip(0, 2);
    assert_eq!(after, HandgunFrame(0x83));
    assert_eq!(page_frames(2, after), [0x83, 0x87, 8]);
    // Another weapon leaves it, and 0x83 stays.
    assert_eq!(fresh.equip(0xA, 2).equip(0, 3), HandgunFrame(0x03));
    assert_eq!(after.equip(1, 2), HandgunFrame(0x83));
    // The tilt down after it comes past frame 3, on the foot of 0x80.
    assert_eq!(page_foot_chain(0, page_frames(2, after)), &AIM_FOOT_CHAIN);
}

#[test]
fn leons_alternates_tilt_down_past_frame_three() {
    let assets = assets(16, 40, 0);
    let fresh = HandgunFrame::default();
    // Leon after 8 or 0xA loaded the handgun reads 0x83, and tilts as soon.
    let after = fresh.equip(8, 2).equip(0, 2);
    for (id, handgun, ticks) in [
        (0, fresh.equip(0, 2), 6),
        (8, fresh.equip(8, 2), 4),
        (0xA, fresh.equip(0xA, 2), 4),
        (0, after, 4),
    ] {
        let mut model = PageModel::new(id, 2, Statics::default(), handgun);
        model.work.routine = [1, MOVE_AIM, 0, 0];
        model.statics.row = ROW_AIM;
        model.statics.step = 2;
        let mut count = 0;
        while model.work.routine[2] == 0 {
            model.demonstrate(&assets, ROW_AIM);
            count += 1;
        }
        assert_eq!(count, ticks, "id {id:#x}");
        assert_eq!(model.work.status_flg & 0xE000, TILT_DOWN);
    }
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

#[test]
fn the_reflections_shadow_is_under_the_root_at_the_negated_x() {
    let assets = posed_assets(16, 30);
    let model = settled(&assets);
    let joints = model.joints(&assets);
    let shadow = model.shadow(&joints).unwrap();
    assert_eq!(shadow.pos, [-joints[0].t[0], PLACE[1], joints[0].t[2]]);
    // Turned by `-Cdir_y`: facing 0xC00, 0x400.
    assert_eq!(shadow.angle, 0x400);
    assert_eq!(shadow.half, (0x1A4, 0x208));
    assert_eq!(model.shadow(&[]), None);
}

#[test]
fn the_shadows_sizes_follow_the_character() {
    // `FUN_00447a20`'s `Kage_work9_set` sizes.
    assert_eq!(shadow_half(0), (500, 600));
    assert_eq!(shadow_half(1), (0x1A4, 0x208));
    assert_eq!(shadow_half(8), (500, 600));
    assert_eq!(shadow_half(9), (0x1A4, 0x208));
    assert_eq!(shadow_half(0xB), (500, 600));
    assert_eq!(shadow_half(0xE), (0x1A4, 0x208));
    assert_eq!(shadow_half(0xF), (0x172, 0x1CC));
}

#[test]
fn the_joints_are_the_drawn_parts() {
    let assets = posed_assets(16, 30);
    let model = settled(&assets);
    let joints = model.joints(&assets);
    assert_eq!(joints.len(), 8);
    // The root at the place, and the foot where the planting reads it.
    let foot = model.drawn_feet[0].unwrap();
    assert_eq!(joints[7].t, foot);
    // Drawing doesn't move the model.
    assert_eq!(model.joints(&assets), joints);
}
