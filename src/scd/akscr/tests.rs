use super::*;
use crate::scd::asm;
use crate::scd::ir::{Block, Scripts, Stream};
use crate::scd::reader;
use std::collections::BTreeMap;

/// Compile `text` for RDT 1000, lay it out as a container and decode it
/// back.
fn round(text: &str) -> Scripts {
    round_variant(text, Variant::for_room(0x1000))
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
fn malformed_jumps_keep_asm() {
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
    // The original, as `asm`.
    assert!(
        text.contains("    run noreturn\n    {\n        asm"),
        "{text}"
    );
    assert!(!text.contains("#if"), "{text}");
}

#[test]
fn an_unpaired_endif_stays_asm() {
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
        text.contains("    asm\n    {\n            endif                   0\n    }\n"),
        "{text}"
    );
    assert!(!text.contains("#if"), "{text}");
}

#[test]
fn a_message_id_past_one_byte_takes_the_long_form() {
    let scripts = round(
        "#version 1\nproc init\n{\n    message(0x0C | MSG_FAST, 255);\n    \
         message(0x106 | MSG_FAST, 255);\n}\n",
    );
    assert_eq!(
        block_bytes(&scripts.init[0]),
        [
            0x0B, 0x8C, 0xFF, 0x00, 0x53, 0x00, 0x86, 0x01, 0xFF, 0x00, 0x00, 0x00
        ]
    );
    let error = compile_error("#version 1\nproc init\n{\n    message(0x10000, 0);\n}\n");
    assert!(error.contains("16 bits"), "{error}");
    let error = compile_error("#version 1\nproc init\n{\n    message(0x140, 0);\n}\n");
    assert!(error.contains("global"), "{error}");
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
    // A generic `event_XX` is event XX in an RDT variant's build; the
    // runtime build numbers every variant's events, so there it is a name.
    let error = compile_variant(
        "#version 1\nevent event_01\n{\n    evt_finish();\n}\n",
        Variant::for_room(0x1030),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("event_00"), "{error:#}");
    assert!(compile("#version 1\nevent event_01\n{\n    evt_finish();\n}\n").is_ok());
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

/// The `nop` operands a command script's main blocks run, in order, for
/// the room variant `bits`.
fn main_nops(container: &[u8], bits: u8) -> Vec<i64> {
    use crate::scd::host::{ScdHost, StepResult};
    use crate::scd::ir::Operand;
    use crate::scd::opcode::Op;
    struct Host {
        bits: u8,
        nops: Vec<i64>,
    }
    impl ScdHost for Host {
        fn on_flow(&mut self, op: &Op, operands: &[Operand]) -> StepResult {
            if op.op == 0x0E {
                self.nops.push(operands[0].value);
            }
            StepResult::Continue
        }
        fn room_variant(&mut self) -> u8 {
            self.bits
        }
    }
    let scripts = reader::parse(container).unwrap();
    let mut host = Host {
        bits,
        nops: Vec::new(),
    };
    crate::scd::vm::CommandVm::new(&scripts).run_main(&mut host);
    host.nops
}

/// `text` built for `variant` (its `nop`s, as the bytes run them) and its
/// runtime build run as that variant give the same `nop`s.
fn variant_nops(text: &str, variant: Variant) -> Vec<i64> {
    let folded = compile_variant(text, variant)
        .unwrap()
        .to_container()
        .unwrap();
    let nops = main_nops(&folded, variant.bits());
    let runtime = compile(text).unwrap().to_container().unwrap();
    assert_eq!(main_nops(&runtime, variant.bits()), nops, "{variant:?}");
    nops
}

#[test]
fn variant_tests_pick_lines_per_variant() {
    let text = "#version 1
proc main noreturn
{
    if (MANSION1)
    {
        nop(1);
        if (CHRIS)
        {
            nop(2);
        }
        else
        {
            nop(3);
        }
    }
    else
    {
        nop(4);
    }
    if (JILL)
    {
        nop(5);
    }
}
";
    let nops = |id: u16| variant_nops(text, Variant::for_room(id));
    assert_eq!(nops(0x1030), [1, 2]);
    assert_eq!(nops(0x1031), [1, 3, 5]);
    assert_eq!(nops(0x6030), [4]);
    assert_eq!(nops(0x7031), [4, 5]);
    // Outside the mansion neither visit holds.
    assert_eq!(nops(0x3001), [4, 5]);
    // A variant's build holds only its own lines; the runtime build jumps.
    let folded = compile_variant(text, Variant::for_room(0x1030))
        .unwrap()
        .to_container()
        .unwrap();
    assert_eq!(
        block_bytes(&reader::parse(&folded).unwrap().main[0]),
        [0x0E, 1, 0x0E, 2]
    );

    // A variant test holds only variants, and some variant must take it.
    let mixed = compile_error(
        "#version 1\nproc main\n{\n    if (JILL && ck(FG_COMMON, 1, TRUE))\n    {\n    }\n}\n",
    );
    assert!(mixed.contains("cannot mix"), "{mixed}");
    let never =
        compile_error("#version 1\nproc main\n{\n    if (JILL && CHRIS)\n    {\n    }\n}\n");
    assert!(never.contains("never holds"), "{never}");
    // The variants are not preprocessor symbols.
    let directive = compile_error("#version 1\n#if JILL\n#endif\n");
    assert!(directive.contains("if (JILL)"), "{directive}");
    let unknown = compile_error("#version 1\n#if REBECCA\n#endif\n");
    assert!(unknown.contains("REBECCA"), "{unknown}");
    for broken in ["#endif\n", "#else\n", "#if AUTHENTIC\n#endif\n"] {
        assert!(compile(broken).is_err(), "{broken}");
    }
}

#[test]
fn variant_tests_fold_sections_and_return() {
    // A section only some RDTs carry, and a `return` inside a variant test:
    // the jump keeps the `if` nesting, so it still ends the block.
    let text = "#version 1
proc main when (CHRIS && MANSION2)
{
    nop(4);
}
proc main noreturn (MANSION2)
{
    nop(1);
    if (JILL)
    {
        nop(2);
        return;
    }
    nop(3);
}
proc main
{
    nop(5);
}
event on_enter when (JILL)
{
    evt_finish();
}
event on_aot
{
    evt_finish();
}
";
    // Running off the end of a `noreturn` block ends the block list.
    let nops = |id: u16| variant_nops(text, Variant::for_room(id));
    assert_eq!(nops(0x1030), [1, 3, 5]);
    assert_eq!(nops(0x1031), [1, 2, 5]);
    assert_eq!(nops(0x6030), [4, 1, 3]);
    assert_eq!(nops(0x6031), [1, 2, 5]);
    let build = |id: u16| {
        let container = compile_variant(text, Variant::for_room(id))
            .unwrap()
            .to_container()
            .unwrap();
        reader::parse(&container).unwrap()
    };
    // `on_aot` is Chris's event 0 and Jill's event 1.
    assert_eq!(build(0x1030).events.len(), 1);
    assert_eq!(build(0x1031).events.len(), 2);
    assert_eq!(build(0x6030).main.len(), 3);
    assert_eq!(build(0x1030).main.len(), 2);
    // `noreturn (MANSION2)`: no final `end` on the second visit.
    assert_eq!(
        block_bytes(&build(0x6031).main[0]),
        [0x0E, 1, 0x0E, 2, 0x00, 0x00, 0x0E, 3]
    );
    assert!(block_bytes(&build(0x1031).main[0]).ends_with(&[0x0E, 3, 0x00, 0x00]));
    let runtime = reader::parse(&compile(text).unwrap().to_container().unwrap()).unwrap();
    assert_eq!(runtime.events.len(), 2);
    assert_eq!(runtime.main.len(), 3);
}

#[test]
fn event_variant_tests_follow_each_variants_state() {
    // Chris's branch leaves the actor sequence, Jill's stays in it: the
    // instructions after the shared sleep decode in each one's own state.
    let text = "#version 1
event event_00
{
    evt_actor_begin();
    if (CHRIS)
    {
        act_reset();
        evt_nop();
    }
    else
    {
        act_anim_flags(1, 2, 0);
    }
    sleep(20);
    if (CHRIS)
    {
        evt_finish();
    }
    else
    {
        act_anim_flags(3, 4, 0);
        act_finish();
    }
}
";
    for id in [0x1030, 0x1031] {
        let folded = compile_variant(text, Variant::for_room(id)).unwrap();
        assert!(folded.to_container().is_ok());
    }
    let runtime = compile(text).unwrap().to_container().unwrap();
    let scripts = reader::parse(&runtime).unwrap();
    let decoded: Vec<_> = scripts.events[0]
        .insns
        .iter()
        .map(|insn| insn.decoded)
        .collect();
    assert!(
        decoded
            .iter()
            .filter(|decoded| matches!(decoded, crate::scd::ir::Decoded::Actor(_)))
            .count()
            >= 3,
        "{decoded:?}"
    );
    // A state the variants do not share before an instruction that needs
    // one is an error.
    let error = compile_error(
        "#version 1\nevent event_00\n{\n    if (CHRIS)\n    {\n        evt_actor_begin();\n    }\n    act_reset();\n    evt_finish();\n}\n",
    );
    assert!(error.contains("different event states"), "{error}");
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
        let kept: Vec<&str> =
            preprocess::kept_lines(&merged, Some(preprocess::Build::Legacy(variant)))
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
fn region_tests_pick_the_region() {
    let text = "#version 1
proc main noreturn
{
    if (USA)
    {
        nop(1);
    }
    else
    {
        nop(2);
    }
    if (!JPN)
    {
        nop(3);
    }
    if (EUR)
    {
        nop(4);
    }
    if (!JILL)
    {
        nop(5);
    }
}
";
    let nops = |variant: Variant| variant_nops(text, variant);
    assert_eq!(nops(Variant::for_room(0x1030)), [1, 3, 5]);
    assert_eq!(nops(Variant::for_room(0x1031).in_region(Region::Jpn)), [2]);
    let eur = Variant::for_room(0x1030).in_region(Region::Eur);
    let container = compile_variant(text, eur).unwrap().to_container().unwrap();
    assert_eq!(main_nops(&container, eur.bits()), [2, 3, 4, 5]);
    assert_eq!(Region::parse("eur"), Some(Region::Eur));
    assert_eq!(Region::parse("STEAM"), None);
    let unknown = compile_error("#version 1\nproc main\n{\n    if (!GOG)\n    {\n    }\n}\n");
    assert!(unknown.contains("`GOG`"), "{unknown}");
}

#[test]
fn merged_regions_differ_under_their_symbols() {
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
    let usa = rooms("#version 1\n\nproc main\n{\n    nop(1);\n    nop(3);\n}\n");
    let jpn = rooms("#version 1\n\nproc main\n{\n    nop(1);\n    nop(4);\n}\n");
    let merged = merge_regions(&[
        (Region::Usa, &usa),
        (Region::Jpn, &jpn),
        (Region::Eur, &jpn),
    ])
    .unwrap();
    // Chris's script is the same in every region, and Jill's in JPN and EUR,
    // so only Jill's USA line stands apart.
    assert_eq!(
        merged[&0x103],
        "#version 1\n\nproc main\n{\n    nop(1);\n    if (CHRIS)\n    {\n        nop(2);\n    }\n\
         \x20   else if (USA)\n    {\n        nop(3);\n    }\n    else\n    {\n        nop(4);\n    }\n}\n"
    );
    // Regions with the same scripts merge as one.
    assert_eq!(
        merge_regions(&[(Region::Usa, &usa), (Region::Eur, &usa)]).unwrap(),
        merge_rooms(&usa).unwrap()
    );
    // A region may hold RDTs the others lack.
    let mut extra = jpn.clone();
    extra.insert(
        0x6030,
        "#version 1\n\nproc main\n{\n    nop(5);\n}\n".to_string(),
    );
    let merged = merge_regions(&[(Region::Usa, &usa), (Region::Jpn, &extra)]).unwrap();
    assert!(merged[&0x103].contains("MANSION"), "{}", merged[&0x103]);
    let error = format!(
        "{:#}",
        merge_regions(&[(Region::Usa, &usa), (Region::Usa, &jpn)]).unwrap_err()
    );
    assert!(error.contains("given twice"), "{error}");
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

/// Every checked-in room script compiles for each of its variants, and for
/// all of them at once.
#[test]
fn checked_in_room_scripts_compile() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/re1/room");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let paths: Vec<_> = entries
        .map(|entry| entry.unwrap().path().join(super::ROOM_SCRIPT_FILE))
        .filter(|path| path.is_file())
        .collect();
    assert!(!paths.is_empty(), "no scripts under {}", dir.display());
    // Every script compiles in each room variant and region, which is a few
    // seconds of work: spread the scripts over the cores.
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        for chunk in paths.chunks(paths.len().div_ceil(threads)) {
            let dir = &dir;
            scope.spawn(move || {
                let load = includes_from(dir);
                for path in chunk {
                    let text = std::fs::read_to_string(path).unwrap();
                    let folder = path.parent().unwrap().file_name().unwrap();
                    let folder = folder.to_string_lossy();
                    let base = u16::from_str_radix(&folder, 16).unwrap();
                    let builds = room_variants(base)
                        .into_iter()
                        .flat_map(|id| {
                            Region::ALL.into_iter().map(move |region| {
                                Build::Variant(Variant::for_room(id).in_region(region))
                            })
                        })
                        .chain([Build::Runtime]);
                    for build in builds {
                        if let Err(error) = compile_build(&text, build, &load)
                            .and_then(|assembled| assembled.to_container())
                        {
                            panic!("{} as {build:?}: {error:#}", path.display());
                        }
                    }
                }
            });
        }
    });
}

/// Lifting the merged `#if` form gives a script whose every variant's build
/// is the merged script's: shared and renamed events, a reference split by
/// variant, an `evt_chain` by number and a header-only `nopad` block.
#[test]
fn lifted_scripts_build_each_variant_alike() {
    let merged = "#version 1

proc init
{
    nop(1);
#if CHRIS
    evt_exec(0, 9, on_enter);
#else
    evt_exec(0, 9, on_enter);
    nop(2);
#endif
}

#if CHRIS
event on_enter
{
    evt_finish();
}

event on_enter_2
#else
event on_enter
#endif
{
    evt_chain(1);
    evt_finish();
}

#if MANSION1
event on_aot nopad
#else
event on_aot
#endif
{
#if USA
    sleep(1);
#endif
    evt_finish();
}
";
    let lifted = lift(merged, true).unwrap();
    for variant in super::lift::variants(true) {
        let old = compile_build(merged, preprocess::Build::Legacy(variant), &no_includes)
            .unwrap()
            .to_container()
            .unwrap();
        let new = compile_variant(&lifted, variant)
            .unwrap()
            .to_container()
            .unwrap();
        assert_eq!(old, new, "{variant:?}\n{lifted}");
    }
    assert!(compile(&lifted).is_ok(), "{lifted}");
    // Jill's `on_enter` is Chris's `on_enter_2`: one event, one name.
    assert!(lifted.contains("event on_enter when (CHRIS)\n"), "{lifted}");
    assert!(lifted.contains("event on_enter_2\n"), "{lifted}");
    assert!(lifted.contains("evt_chain(on_enter_2);"), "{lifted}");
    assert!(
        lifted.contains("event on_aot nopad (MANSION1)\n"),
        "{lifted}"
    );
    assert!(lifted.contains("    if (USA)\n"), "{lifted}");
}
