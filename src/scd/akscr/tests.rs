use super::*;
use crate::scd::asm;
use crate::scd::ir::{Block, Scripts, Stream};
use crate::scd::reader;
use std::collections::BTreeMap;

/// Compile `text` as the original game's bytecode (`AUTHENTIC`), lay it
/// out as a container and decode it back.
fn round(text: &str) -> Scripts {
    round_variant(text, Variant::for_room(0x1000).authentic())
}

fn round_variant(text: &str, variant: Variant) -> Scripts {
    let container = compile_variant(text, variant)
        .unwrap()
        .to_container()
        .unwrap();
    reader::parse(&container).unwrap()
}

fn block_bytes(block: &Block) -> Vec<u8> {
    let mut bytes: Vec<u8> = block
        .insns
        .iter()
        .flat_map(|insn| insn.bytes.iter().copied())
        .collect();
    bytes.extend_from_slice(&block.trailing);
    bytes
}

fn event_bytes(stream: &Stream) -> Vec<u8> {
    let mut bytes: Vec<u8> = stream
        .insns
        .iter()
        .flat_map(|insn| insn.bytes.iter().copied())
        .collect();
    bytes.extend_from_slice(&stream.trailing);
    bytes
}

/// Every stream's exact bytes, in container order.
fn all_bytes(scripts: &Scripts) -> Vec<Vec<u8>> {
    scripts
        .init
        .iter()
        .chain(&scripts.main)
        .map(block_bytes)
        .chain(scripts.events.iter().map(event_bytes))
        .collect()
}

/// Decompile `.s` source's streams and check the script compiles back to the
/// same bytes. Returns the script.
fn decompiles_exactly(source: &str) -> String {
    let container = asm::assemble(source).unwrap().to_container().unwrap();
    let scripts = reader::parse(&container).unwrap();
    let text = decompile(&scripts);
    assert_eq!(all_bytes(&round(&text)), all_bytes(&scripts), "{text}");
    text
}

fn compile_error(text: &str) -> String {
    format!("{:#}", compile(text).unwrap_err())
}

#[test]
fn proc_blocks_end_in_an_implicit_return() {
    let scripts =
        round("#version 1\nproc init\n{\n    set(FG_COMMON, 1, 1);\n}\nproc main\n{\n}\n");
    assert_eq!(
        block_bytes(&scripts.init[0]),
        [0x05, 0x01, 0x01, 0x01, 0x00, 0x00]
    );
    assert_eq!(block_bytes(&scripts.main[0]), [0x00, 0x00]);

    let scripts = round("#version 1\nproc init noreturn\n{\n    nop(0);\n}\n");
    assert_eq!(block_bytes(&scripts.init[0]), [0x0E, 0x00]);
}

#[test]
fn if_else_chains_lower_to_skips_and_jumps() {
    let text = "#version 1
proc init
{
    if (ck(FG_COMMON, 2, 0) && ck(FG_ROOM, 3, 1))
    {
        set(FG_COMMON, 2, 1);
    }
    else if (ck(FG_ROOM, 4, 0))
    {
        return;
    }
    else
    {
        require(ck(FG_ROOM, 5, 1));
    }
}
";
    let scripts = round(text);
    #[rustfmt::skip]
    let expected = [
        0x01, 0x0E,                   // if: skip to the first else
        0x04, 0x01, 0x02, 0x00,       // ck
        0x04, 0x04, 0x03, 0x01,       // ck
        0x05, 0x01, 0x02, 0x01,       // set
        0x02, 0x10,                   // else: jump past the chain
        0x01, 0x08,                   // if: skip to the inner else
        0x04, 0x04, 0x04, 0x00,       // ck
        0x00, 0x00,                   // return
        0x02, 0x06,                   // else
        0x04, 0x04, 0x05, 0x01,       // require
        0x00, 0x00,                   // implicit end
    ];
    assert_eq!(block_bytes(&scripts.init[0]), expected);
    assert_eq!(
        decompile(&scripts)
            .lines()
            .filter(|line| line.contains("else"))
            .count(),
        2
    );
}

#[test]
fn event_statements_lower_to_the_event_vm() {
    let text = "#version 1
event event_00
{
    run
    {
        set(FG_RUN, 23, 1);
    }
    evt_exec(0, 6, event_0A);
    single(10) effect(0, 5, 0, 8900, -300, 0, 0);
    sleep(30);
    wait_message();
    for (3)
    {
        evt_next();
    }
    do
    {
        evt_next();
    }
    while (ck(FG_STATUS, 14, 0));
    evt_finish();
}
";
    let scripts = round(text);
    #[rustfmt::skip]
    let expected = [
        0x06, 0x0B, 0x08, 0x00, 0x05, 0x06, 0x17, 0x01, 0x00, 0x00, 0x00, // run
        0x00,                                                             // its evt_nop
        0x07, 0x06, 0x14, 0x00, 0x06, 0x0A,                               // evt_single
        0x07, 0x0A, 0x2A, 0x00, 0x05, 0x00, 0xC4, 0x22, 0xD4, 0xFE,       // single(10)
        0xF8, 0xF9, 0x1E, 0x00,                                           // sleep
        0xF6, 0xF7,                                                       // wait_message
        0xFA, 0x00, 0x03, 0x00, 0xFE, 0xFB,                               // for
        0xFC, 0x06, 0x04, 0x05, 0x0E, 0x00, 0xFE, 0xFD,                   // do/while
        0xFF, 0x00, 0x00, 0x00,                                           // finish + padding
    ];
    assert_eq!(event_bytes(&scripts.events[0]), expected);
}

#[test]
fn raw_trailing_bytes_replace_the_padding() {
    let scripts =
        round("#version 1\nevent event_00\n{\n    evt_finish();\n    raw(0x17, 0x00, 0xFF);\n}\n");
    assert_eq!(event_bytes(&scripts.events[0]), [0xFF, 0x17, 0x00, 0xFF]);
}

#[test]
fn asm_blocks_embed_assembly_with_labels() {
    let text = "#version 1
proc init noreturn
{
    asm
    {
        if      off_0004 + 1
        ck      FG_ROOM, 2, 1
    off_0004:
        endif   0
        end     0
    }
}
";
    let scripts = round(text);
    assert_eq!(
        block_bytes(&scripts.init[0]),
        [0x01, 0x07, 0x04, 0x04, 0x02, 0x01, 0x03, 0x00, 0x00, 0x00]
    );
}

#[test]
fn structured_shapes_decompile_exactly() {
    let text = decompiles_exactly(
        ".version 1

.init
.block
    if                      off_A
    ck                      FG_COMMON, 1, 0
    set                     FG_COMMON, 1, 1
off_A:
    else                    off_B
    ck                      FG_COMMON, 2, 1
off_B:
    end                     0

.main
.block
    set                     FG_COMMON, 3, 1

.event event_00
    evt_block               15
    evt_block_sub           12
    set                     FG_RUN, 23, 1
    set                     FG_STATUS, 15, 0
    end                     0
    evt_block_sub           0
    evt_nop
    evt_do                  off_C
    ck                      FG_STATUS, 14, 0
off_C:
    evt_next
    evt_dountil
    evt_sleep               249, 30, 0
    evt_finish
    db                      0x00, 0x00, 0x00
",
    );
    assert!(
        text.contains("else\n    {\n        require(ck(FG_COMMON, 2, FALSE));"),
        "{text}"
    );
    assert!(text.contains("proc main noreturn"), "{text}");
    assert!(text.contains("sleep(30);"), "{text}");
    assert!(!text.contains("asm"), "{text}");
}

#[test]
fn malformed_jumps_keep_asm_as_authentic() {
    // The `if` skip lands one byte into its `endif`, as in ROOM1051.
    let text = decompiles_exactly(
        ".version 1

.event event_00
    evt_block               19
    evt_block_sub           16
    if                      off_A + 1
    cmpb                    2, 0, 1
    set                     FG_ROOM, 2, 0
off_A:
    endif                   0
    end                     0
    evt_block_sub           0
    evt_nop
    evt_finish
",
    );
    assert!(text.contains("event event_00 nopad"), "{text}");
    // The original as `asm`, and the fixed jump otherwise.
    assert!(
        text.contains("#if AUTHENTIC\n    run noreturn\n    {\n        asm"),
        "{text}"
    );
    assert!(
        text.contains(
            "#else\n    run\n    {\n        if (cmpb(B_CAMERA, CMP_EQ, CAM_01))\n        {\n            \
             set(FG_ROOM, F_ROOM_02, TRUE);\n        }\n    }\n#endif"
        ),
        "{text}"
    );
    // The fixed `if` skips to its `endif`.
    let fixed = round_variant(&text, Variant::for_room(0x1000));
    let bytes = event_bytes(&fixed.events[0]);
    assert_eq!(&bytes[4..6], [0x01, 0x0A], "{text}");
}

#[test]
fn an_unpaired_endif_is_authentic_only() {
    let text = decompiles_exactly(
        ".version 1

.init
.block
    set                     FG_ROOM, 2, 0
    endif                   0
    set                     FG_ROOM, 3, 0
    end                     0
",
    );
    assert!(
        text.contains(
            "#if AUTHENTIC\n    asm\n    {\n            endif                   0\n    }\n#endif"
        ),
        "{text}"
    );
    let fixed = round_variant(&text, Variant::for_room(0x1000));
    assert_eq!(
        block_bytes(&fixed.init[0]),
        [0x05, 0x04, 0x02, 0x00, 0x05, 0x04, 0x03, 0x00, 0x00, 0x00]
    );
}

#[test]
fn errors_name_the_line() {
    assert!(compile_error("proc init\n{\n}\n").contains("#version 1"));
    assert!(compile_error("#version 1\nproc init\n{\n    sleep(3);\n}\n").contains("line 4"));
    assert!(
        compile_error("#version 1\nproc init\n{\n    if (set(FG_COMMON, 1, 1))\n    {\n    }\n}\n")
            .contains("not a condition")
    );
    assert!(
        compile_error("#version 1\nevent event_00\n{\n    return;\n}\n").contains("inside a `run`")
    );
    assert!(
        compile_error("#version 1\nevent event_01\n{\n    evt_finish();\n}\n").contains("event_00")
    );
    assert!(
        compile_error("#version 1\nproc init\n{\n    set(FG_COMMON, 1, 1)\n}\n").contains("`;`")
    );
}

/// Commands written by field pack into the engine's operand bytes, words
/// little-endian and signed fields two's complement.
#[test]
fn field_commands_pack_their_operands() {
    let scripts = round(
        "#version 1
proc init
{
    voice_play(VOICE_PLAY, 0x0123);
    sys_multi(SYS_PLAYER_FLAGS, SYS_OR | PLF_04);
    spd_add(-25);
}
event event_00
{
    evt_actor_begin();
    act_anim_seq(AB_ADDITIVE | AB_06, -2, 300, 0x3F);
    act_finish();
}
",
    );
    assert_eq!(
        block_bytes(&scripts.init[0]),
        [
            0x1E, 0x01, 0x23, 0x01, 0x33, 0x03, 0x04, 0x01, 0x45, 0xE7, 0x00, 0x00
        ]
    );
    assert_eq!(
        event_bytes(&scripts.events[0])[..8],
        [0x01, 0x83, 0x16, 0xFE, 0xFF, 0x2C, 0x01, 0x3F]
    );
    let error = compile_error("#version 1\nproc init\n{\n    setw(W_04, 1, 2);\n}\n");
    assert!(
        error.contains("line 4") && error.contains("takes 2 arguments"),
        "{error}"
    );
}

#[test]
fn preprocessor_selects_lines_per_variant() {
    let text = "#version 1
proc main noreturn
{
#if MANSION1
    nop(1);
#if CHRIS
    nop(2);
#else
    nop(3);
#endif
#else
    nop(4);
#endif
#if JILL
    nop(5);
#endif
}
";
    let nops = |id: u16| -> Vec<u8> {
        let container = compile_variant(text, Variant::for_room(id))
            .unwrap()
            .to_container()
            .unwrap();
        block_bytes(&reader::parse(&container).unwrap().main[0])
            .chunks(2)
            .map(|insn| insn[1])
            .collect()
    };
    assert_eq!(nops(0x1030), [1, 2]);
    assert_eq!(nops(0x1031), [1, 3, 5]);
    assert_eq!(nops(0x6030), [4]);
    assert_eq!(nops(0x7031), [4, 5]);
    // Outside the mansion neither mansion symbol is defined.
    assert_eq!(nops(0x3001), [4, 5]);

    assert!(compile_error(text).contains("--room"));
    let unknown = format!(
        "{:#}",
        compile_variant(
            "#version 1\n#if REBECCA\n#endif\n",
            Variant::for_room(0x1030)
        )
        .unwrap_err()
    );
    assert!(unknown.contains("REBECCA"), "{unknown}");
    for broken in ["#endif\n", "#if JILL\n", "#if JILL\n#else\n#else\n#endif\n"] {
        assert!(
            compile_variant(broken, Variant::for_room(0x1030)).is_err(),
            "{broken}"
        );
    }
    // Dropped lines keep their place, so errors name the source line.
    let error = format!(
        "{:#}",
        compile_variant(
            "#version 1\nproc init\n{\n#if CHRIS\n    nop(0);\n#endif\n    sleep(1);\n}\n",
            Variant::for_room(0x1031)
        )
        .unwrap_err()
    );
    assert!(error.contains("line 7"), "{error}");
}

#[test]
fn room_ids_map_to_merged_scripts() {
    assert_eq!(merged_id(0x1030), 0x103);
    assert_eq!(merged_id(0x6031), 0x103);
    assert_eq!(merged_id(0x70C0), 0x20C);
    assert_eq!(merged_id(0x5141), 0x514);
    assert_eq!(room_variants(0x103), [0x1030, 0x1031, 0x6030, 0x6031]);
    assert_eq!(room_variants(0x514), [0x5140, 0x5141]);
}

#[test]
fn merged_scripts_reproduce_every_variant() {
    let chris_1 = "#version 1\n\nproc main\n{\n    nop(1);\n    nop(2);\n    nop(3);\n}\n";
    let jill_1 = "#version 1\n\nproc main\n{\n    nop(1);\n    nop(9);\n    nop(3);\n}\n";
    let chris_2 = "#version 1\n\nproc main\n{\n    nop(1);\n    nop(2);\n}\n";
    let jill_2 = "#version 1\n\nproc main\n{\n    nop(1);\n    nop(9);\n}\n";
    let variants = [
        (Variant::for_room(0x1030), chris_1),
        (Variant::for_room(0x1031), jill_1),
        (Variant::for_room(0x6030), chris_2),
        (Variant::for_room(0x6031), jill_2),
    ];
    let merged = merge(&variants).unwrap();
    assert_eq!(
        merged,
        "#version 1\n\nproc main\n{\n    nop(1);\n#if CHRIS\n    nop(2);\n#else\n    \
         nop(9);\n#endif\n#if MANSION1\n    nop(3);\n#endif\n}\n"
    );
    for (variant, text) in variants {
        let kept: Vec<&str> = preprocess::kept_lines(&merged, Some(variant))
            .unwrap()
            .into_iter()
            .map(|(_, line)| line)
            .collect();
        assert_eq!(kept, text.lines().collect::<Vec<_>>());
    }

    // A room missing a variant merges what is there.
    let merged = merge(&variants[..2]).unwrap();
    assert!(!merged.contains("MANSION"), "{merged}");
}

#[test]
fn steam_picks_the_release() {
    let text = "#version 1
proc main noreturn
{
#if STEAM
    nop(1);
#else
    nop(2);
#endif
#if !STEAM
    nop(3);
#endif
#if !JILL
    nop(4);
#endif
}
";
    let nops = |variant: Variant| -> Vec<u8> {
        let container = compile_variant(text, variant)
            .unwrap()
            .to_container()
            .unwrap();
        block_bytes(&reader::parse(&container).unwrap().main[0])
            .chunks(2)
            .map(|insn| insn[1])
            .collect()
    };
    assert_eq!(nops(Variant::for_room(0x1030)), [1, 4]);
    assert_eq!(nops(Variant::for_room(0x1031).mediakite()), [2, 3]);
    assert_eq!(
        nops(Variant::for_room(0x1030).mediakite().authentic()),
        [2, 3, 4]
    );
    let unknown = format!(
        "{:#}",
        compile_variant("#version 1\n#if !GOG\n#endif\n", Variant::for_room(0x1030)).unwrap_err()
    );
    assert!(unknown.contains("`!GOG`"), "{unknown}");
}

#[test]
fn merged_releases_differ_under_steam() {
    let rooms = |second: &str| -> BTreeMap<u16, String> {
        [
            (
                0x1030,
                "#version 1\n\nproc main\n{\n    nop(1);\n    nop(2);\n}\n",
            ),
            (0x1031, second),
        ]
        .into_iter()
        .map(|(id, text)| (id, text.to_string()))
        .collect()
    };
    let steam = rooms("#version 1\n\nproc main\n{\n    nop(1);\n    nop(3);\n}\n");
    let mediakite = rooms("#version 1\n\nproc main\n{\n    nop(1);\n    nop(4);\n}\n");
    let merged = merge_releases(&steam, &mediakite).unwrap();
    // Chris's script is the same in both releases, so only Jill's differs.
    assert_eq!(
        merged[&0x103],
        "#version 1\n\nproc main\n{\n    nop(1);\n#if CHRIS\n    nop(2);\n#else\n\
         #if STEAM\n    nop(3);\n#else\n    nop(4);\n#endif\n#endif\n}\n"
    );
    // Releases with the same scripts merge as one.
    assert_eq!(
        merge_releases(&steam, &steam).unwrap(),
        merge_rooms(&steam).unwrap()
    );
    let mut missing = mediakite.clone();
    missing.remove(&0x1031);
    let error = format!("{:#}", merge_releases(&steam, &missing).unwrap_err());
    assert!(error.contains("1031"), "{error}");
}

#[test]
fn merged_blocks_hold_whole_statements() {
    let chris = "#version 1

proc init
{
    if (ck(FG_COMMON, 1, 0))
    {
        nop(1);
        nop(2);
    }
    else
    {
        nop(3);
    }
}

event event_00
{
    evt_next();
    evt_finish();
}
";
    let jill = "#version 1

proc init
{
    if (ck(FG_COMMON, 9, 0))
    {
        nop(1);
        nop(2);
    }
    else
    {
        nop(3);
    }
}

event event_00
{
    evt_finish();
}

event event_01
{
    evt_next();
    evt_finish();
}
";
    let merged = merge(&[
        (Variant::for_room(0x5000), chris),
        (Variant::for_room(0x5001), jill),
    ])
    .unwrap();
    // The differing `if` head takes its body and `else` along, and Chris's
    // event_00 pairs with Jill's event_01 under a header-only block.
    assert_eq!(
        merged,
        "#version 1

proc init
{
#if CHRIS
    if (ck(FG_COMMON, 1, 0))
    {
        nop(1);
        nop(2);
    }
    else
    {
        nop(3);
    }
#else
    if (ck(FG_COMMON, 9, 0))
    {
        nop(1);
        nop(2);
    }
    else
    {
        nop(3);
    }
#endif
}

#if CHRIS
event event_00
#else
event event_00
{
    evt_finish();
}

event event_01
#endif
{
    evt_next();
    evt_finish();
}
"
    );
}

#[test]
fn includes_and_defines_name_values() {
    let load = |path: &str| -> anyhow::Result<String> {
        assert_eq!(path, "inc/flags.akinc");
        Ok("// flags\n#define F_COMMON_A4 0xA4\n#define F_ALIAS F_COMMON_A4\n".to_string())
    };
    let text = "#version 1
#include \"inc/flags.akinc\"
#include \"inc/flags.akinc\"
#define SEVEN 7
proc main noreturn
{
    set(FG_COMMON, F_COMMON_A4, TOGGLE);
    set(FG_COMMON, F_ALIAS, TRUE);
    nop(SEVEN);
}
";
    let container = compile_with(text, None, &load)
        .unwrap()
        .to_container()
        .unwrap();
    assert_eq!(
        block_bytes(&reader::parse(&container).unwrap().main[0]),
        [0x05, 0x01, 0xA4, 0x02, 0x05, 0x01, 0xA4, 0x00, 0x0E, 0x07]
    );

    // Game data has no built-in names; a missing include names the problem.
    let error =
        compile_error("#version 1\nproc main\n{\n    set(FG_COMMON, F_COMMON_A4, TRUE);\n}\n");
    assert!(
        error.contains("`F_COMMON_A4`") && error.contains("#include"),
        "{error}"
    );
    let error = compile_error("#version 1\nproc main\n{\n    testitem(ITEM_SWORD_KEY);\n}\n");
    assert!(error.contains("`ITEM_SWORD_KEY`"), "{error}");
    let error = compile_error("#version 1\n#include \"inc/x.akinc\"\n");
    assert!(error.contains("#include"), "{error}");
    let bad = |include: &'static str| {
        move |_: &str| -> anyhow::Result<String> { Ok(include.to_string()) }
    };
    for include in ["proc main\n", "#define A\n", "#define A 1\n#define A 2\n"] {
        let text = "#version 1\n#include \"inc/x.akinc\"\n";
        assert!(
            compile_with(text, None, &bad(include)).is_err(),
            "{include}"
        );
    }
}

#[test]
fn events_take_names_that_stand_for_their_index() {
    let text = "#version 1
event opening
{
    evt_exec(0, 6, door_knock);
    evt_finish();
}

event door_knock
{
    evt_exec(0, 6, opening);
    evt_finish();
}
";
    let scripts = round(text);
    assert_eq!(
        event_bytes(&scripts.events[0])[..6],
        [0x07, 0x06, 0x14, 0x00, 0x06, 0x01]
    );
    assert_eq!(
        event_bytes(&scripts.events[1])[..6],
        [0x07, 0x06, 0x14, 0x00, 0x06, 0x00]
    );
    assert!(
        compile_error(
            "#version 1\nevent a\n{\n    evt_finish();\n}\nevent a\n{\n    evt_finish();\n}\n"
        )
        .contains("already defined")
    );
}

#[test]
fn include_names_decompile_game_data() {
    let names = Names::parse(&[(
        "inc/game.akinc".to_string(),
        "#define IT_33_KEY_SWORD 0x33
#define STAGE_2 0x40
#define ROOM_101_STAIRS 0x01
#define ROOM_201_HALL 0x01
#define F_COMMON_A4 0xA4
#define F_STATUS_08 0x08
"
        .to_string(),
    )])
    .unwrap();
    let source = ".version 1

.init
.block
    testitem                ITEM_SWORD_KEY
    set                     FG_COMMON, 164, 0
    set                     FG_STATUS, 8, 2
    set                     FG_ROOM, 3, 1
    nop                     0
";
    let container = asm::assemble(source).unwrap().to_container().unwrap();
    let scripts = reader::parse(&container).unwrap();
    let text = decompile_with(&scripts, &names, 0x6010);
    for expected in [
        "#include \"inc/game.akinc\"",
        "#define F_ROOM_03 0x03",
        "require(testitem(IT_33_KEY_SWORD));",
        "set(FG_COMMON, F_COMMON_A4, TRUE);",
        "set(FG_STATUS, F_STATUS_08, TOGGLE);",
        "set(FG_ROOM, F_ROOM_03, FALSE);",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
    assert_eq!(
        names.door_target(1, 0x01).as_deref(),
        Some("ROOM_101_STAIRS")
    );
    assert_eq!(
        names.door_target(1, 0x41).as_deref(),
        Some("STAGE_2 | ROOM_201_HALL")
    );
    assert_eq!(names.door_target(1, 0x45).as_deref(), Some("STAGE_2 | 5"));
}

/// The decompiler names AOT indices, events reached from an AOT or a
/// section, and comments messages with their text.
#[test]
fn decompile_names_aots_events_and_messages() {
    let mut names = Names::default();
    names.add_messages("ROOM_601_TEXT_02 = \"A {c1}red\\nbook.\"\nMESSAGE_03 = \"Saved.\"\n");
    let source = ".version 1

.init
.block
    aot_set                 2, 0, 0, 0, 0, SCE_NONE, 0, 0, 0, 0, 0, 0, 0
    aot_reset               2, SCE_EVENT, 0, 0, event_01, 0
    evt_exec                0, 6, event_00
    message                 2, 0
    message                 130, 0
    message                 67, 0
    end                     0

.event event_00
    evt_finish

.event event_01
    evt_finish
";
    for (text, name) in [
        (
            "There's something at the back of the shelf.",
            "MSG_THERE_IS_SOMETHING_AT_THE_BACK",
        ),
        ("You can't. Barry's gun.", "MSG_YOU_CANNOT"),
        (
            "He's from the \u{FF33}\u{FF34}\u{FF21}\u{FF32}\u{FF33}!",
            "MSG_HE_IS_FROM_THE_STARS",
        ),
    ] {
        assert_eq!(super::names::message_name(text, 1).as_deref(), Some(name));
    }
    assert_eq!(
        super::names::message_name("You can't. Barry's gun.", 2).as_deref(),
        Some("MSG_YOU_CANNOT_BARRYS_GUN")
    );
    let container = asm::assemble(source).unwrap().to_container().unwrap();
    let scripts = reader::parse(&container).unwrap();
    let text = decompile_with(&scripts, &names, 0x6010);
    for expected in [
        "#define AOT_02_EVENT 0x02",
        "aot_set(AOT_02_EVENT,",
        "event on_enter",
        "event on_aot_02",
        "evt_exec(0, TASK_06, on_enter);",
        "#define MSG_A_RED_BOOK 0x02",
        "message(MSG_A_RED_BOOK, 0); // \"A red book.\"",
        "message(MSG_A_RED_BOOK | MSG_FAST, 0); // \"A red book.\"",
        "message(67, 0); // \"Saved.\"",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
    let compiled = compile_with(&text, None, &no_includes).unwrap();
    let original = asm::assemble(source).unwrap();
    assert_eq!(compiled, original, "{text}");
}

/// Engine values decompile to the compiler's names, and a room's enemy
/// slots and voice lines to defines of their own.
#[test]
fn decompile_names_engine_values_and_room_slots() {
    let names = Names::parse(&[(
        "inc/game.akinc".to_string(),
        "#define EM_05_CROW 0x05\n#define ROOM_104_TEA_ROOM 0x04\n".to_string(),
    )])
    .unwrap();
    let source = ".version 1

.init
.block
    aot_set                 2, 0, 0, 0, 0, SCE_MESSAGE, 129, 0, 0, 0, 0, 0, 0
    if                      off_A
    cmpb                    5, 2, 128
    cmpb                    10, 0, 1
    setw                    1, 0, 0
off_A:
    endif                   0
    enemy                   5, 1, 255, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0
    room_bgm_state_set      0, 4, 65
    voice_play              1, 9, 0
    message                 3, 261
    cutnext                 3
    se_play_3d              2, 23, 0, 1, 0, 0, 0
    evt_exec                0, 9, event_00
    end                     0

.event event_00
    evt_work_set            WK_ENEMY, 2
    evt_finish
";
    let container = asm::assemble(source).unwrap().to_container().unwrap();
    let scripts = reader::parse(&container).unwrap();
    let text = decompile_with(&scripts, &names, 0x6010);
    for expected in [
        "#define ENEMY_02_CROW 0x02",
        "#define VO_",
        "#define MSG_VO_",
        "SCE_MESSAGE, SAT_MANUAL | SAT_PLAYER,",
        "cmpb(B_MENU_CHOICE, CMP_GE, 128)",
        "setw(W_LIGHT_STATE, 0);",
        "enemy(EM_05_CROW, EB_01, NO_FLAG, SPAWN_FORCE, 2,",
        "ENEMY_02_CROW, ANIM_00, 0, EVAR_00);",
        "room_bgm_state_set(0, ROOM_104_TEA_ROOM, BGM_RELOAD | 1);",
        "voice_play(VOICE_PLAY, VO_",
        "message(MSG_VO_",
        "evt_work_set(WK_ENEMY, ENEMY_02_CROW);",
        // Placeholders for what has no meaning yet.
        "cmpb(B_0A, CMP_EQ, 1)",
        "RUN_CONTROLS | RUN_MONSTERS | RUN_PLAYER);",
        "evt_exec(0, TASK_09, on_enter);",
        // The room's own cameras and sounds.
        "#define CAM_03 0x03",
        "cutnext(CAM_03);",
        "se_play_3d(SND_BANK_ROOM, SE_17",
        "SND_AT_PLAYER, 0, 0, 0);",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
    // Each section is checked against its bytes, falling back to `asm`.
    assert!(!text.contains("asm"), "{text}");
}

/// Every checked-in room script compiles for each of its variants.
#[test]
fn checked_in_room_scripts_compile() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/re1/room");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut count = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_none_or(|extension| extension != "akscr")
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy();
        let base = u16::from_str_radix(&stem, 16).unwrap();
        let load = includes_from(&dir);
        for id in room_variants(base) {
            let steam = Variant::for_room(id);
            for variant in [
                steam,
                steam.authentic(),
                steam.mediakite(),
                steam.mediakite().authentic(),
            ] {
                if let Err(error) = compile_with(&text, Some(variant), &load)
                    .and_then(|assembled| assembled.to_container())
                {
                    panic!("{} as {variant:?}: {error:#}", path.display());
                }
            }
        }
        count += 1;
    }
    assert!(count > 0, "no scripts under {}", dir.display());
}
