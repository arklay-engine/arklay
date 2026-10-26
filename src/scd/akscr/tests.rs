use super::*;
use crate::scd::asm;
use crate::scd::ir::{Block, Scripts, Stream};
use crate::scd::reader;

/// Compile `text`, lay it out as a container and decode it back.
fn round(text: &str) -> Scripts {
    let container = compile(text).unwrap().to_container().unwrap();
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
        set(FG_6, 23, 1);
    }
    evt_exec(0, 6, event_0A);
    single(10) effect(0, 5, 0, 196, 34, 212, 254, 116);
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
    set                     FG_6, 23, 1
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
        text.contains("else\n    {\n        require(ck(FG_COMMON, 2, 1));"),
        "{text}"
    );
    assert!(text.contains("proc main noreturn"), "{text}");
    assert!(text.contains("sleep(30);"), "{text}");
    assert!(!text.contains("asm"), "{text}");
}

#[test]
fn malformed_jumps_fall_back_to_asm() {
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
    assert!(text.contains("run noreturn"), "{text}");
    assert!(text.contains("asm"), "{text}");
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

/// Every checked-in room script compiles.
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
        if let Err(error) = compile(&text).and_then(|assembled| assembled.to_container()) {
            panic!("{}: {error:#}", path.display());
        }
        count += 1;
    }
    assert!(count > 0, "no scripts under {}", dir.display());
}
