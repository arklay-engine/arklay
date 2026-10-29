use super::opcode::{self, OPS, operand_size};
use super::reader::{decode_procedure, parse, parse_list};
use super::{ProcList, Procedure, Scripts, akscr, asm};

/// Bytes of one procedure written as `.akscr` statements.
fn compiled(body: &str) -> Vec<u8> {
    akscr::compile_procedure(body).unwrap().to_bytes()
}

/// Two lists from raw procedure bodies.
fn scripts(init: &[&[u8]], main: &[&[u8]]) -> Scripts {
    let list = |bodies: &[&[u8]]| ProcList {
        procs: bodies
            .iter()
            .enumerate()
            .map(|(index, body)| decode_procedure(body, index + 1 == bodies.len()))
            .collect(),
    };
    Scripts {
        init: list(init),
        main: list(main),
    }
}

/// An RDT with the two lists in slots 16 and 17 and a section after them.
fn rdt(scripts: &Scripts) -> Vec<u8> {
    let mut data = vec![0; 8 + 23 * 4];
    let put = |data: &mut Vec<u8>, slot: usize, bytes: &[u8]| {
        let at = data.len() as u32;
        data[8 + slot * 4..12 + slot * 4].copy_from_slice(&at.to_le_bytes());
        data.extend_from_slice(bytes);
        while !data.len().is_multiple_of(4) {
            data.push(0);
        }
    };
    put(&mut data, 16, &scripts.init.to_bytes());
    put(&mut data, 17, &scripts.main.to_bytes());
    put(&mut data, 18, &[0xAA; 8]);
    data
}

#[test]
fn signatures_match_widths() {
    for (index, op) in OPS.iter().enumerate() {
        assert_eq!(usize::from(op.op), index, "{}", op.mnemonic);
        let size: usize = op.operands.bytes().map(operand_size).sum();
        assert_eq!(size + 1, op.width, "{}", op.mnemonic);
        assert_eq!(opcode::by_mnemonic(op.mnemonic).unwrap().op, op.op);
    }
    assert!(opcode::lookup(0x8F).is_none());
}

#[test]
fn encode_and_decode_operands() {
    let cmp = opcode::by_mnemonic("cmp").unwrap();
    let bytes = opcode::encode(cmp, &[0, 24, 3, -2]).unwrap();
    assert_eq!(bytes, [0x23, 0, 24, 3, 0xFE, 0xFF]);
    assert_eq!(opcode::operands(cmp, &bytes), [0, 24, 3, -2]);
    assert_eq!(opcode::encode(cmp, &[0, 24, 3, 0xFFFE]).unwrap(), bytes);
    assert!(opcode::encode(cmp, &[256, 0, 0, 0]).is_err());
    assert!(opcode::encode(cmp, &[0, 0, 0]).is_err());
}

#[test]
fn last_procedure_ends_at_evt_end_outside_blocks() {
    // if (ck) { evt_end } endif; evt_end; junk
    let body = [
        0x06, 0, 6, 0, 0x21, 1, 2, 0, 0x01, 0, 0x08, 0, 0x01, 0, 0xFF, 0xEE,
    ];
    let last = decode_procedure(&body, true);
    assert_eq!(last.insns.len(), 5);
    assert!(last.trailing.is_empty());
    let middle = decode_procedure(&body, false);
    assert_eq!(middle.insns.len(), 5);
    assert_eq!(middle.trailing, [0xFF, 0xEE]);
}

#[test]
fn list_round_trips() {
    let original = scripts(
        &[&[0x22, 5, 3, 1, 0x01, 0, 0xED]],
        &[&[0x02, 0x01, 0], &[0x18, 0, 0x01, 0]],
    );
    let data = rdt(&original);
    assert_eq!(parse(&data).unwrap(), original);
    let (list, len) = parse_list(&original.main.to_bytes()).unwrap();
    assert_eq!(list, original.main);
    assert_eq!(len, original.main.to_bytes().len());
}

#[test]
fn bad_tables_are_errors() {
    assert!(parse_list(&[1, 0]).is_err());
    assert!(parse_list(&[0, 0]).is_err());
    assert!(parse_list(&[4, 0, 2, 0, 0x01, 0]).is_err());
    assert!(parse_list(&[2, 0x10]).is_err());
    assert!(parse(&[0; 16]).is_err());
}

#[test]
fn asm_round_trips() {
    let original = scripts(
        &[
            &[
                0x06, 0, 6, 0, 0x21, 1, 25, 0, 0x22, 5, 3, 1, 0x08, 0, 0x01, 0, 0xED, 0x47,
            ],
            &[0x01, 0],
        ],
        &[&[0x23, 0, 24, 3, 0xFE, 0xFF, 0x01, 0]],
    );
    let text = asm::render(&original);
    assert!(
        text.contains("    if                      0, 6\n"),
        "{text}"
    );
    assert!(text.contains(".db 0xED, 0x47"), "{text}");
    assert_eq!(asm::assemble(&text).unwrap(), original);
}

#[test]
fn asm_errors() {
    assert!(asm::assemble("if 0, 0\n").is_err());
    assert!(asm::assemble(".version 1\n.game re2\n.init\nck 1, 2\n").is_err());
    assert!(asm::assemble(".version 1\n.game re2\n.init\n.proc\nbogus 1\n").is_err());
    assert!(asm::assemble(".version 1\n.game re1\n").is_err());
}

#[test]
fn structured_if_else() {
    let bytes = compiled(
        "if (ck(1, 25, 0) && cmp(0, 24, 0, 3)) { set(5, 3, 1); } else { gosub(2); } \
         evt_end(0);",
    );
    assert_eq!(
        bytes,
        [
            0x06, 0, 18, 0, // if, to past the else header
            0x21, 1, 25, 0, 0x23, 0, 24, 0, 3, 0, // conditions
            0x22, 5, 3, 1, // body
            0x07, 0, 8, 0, // else, jump from its start
            0x18, 2, 0, 0, // else body and its closing nops
            0x01, 0,
        ]
    );
}

#[test]
fn structured_loops_and_switch() {
    let bytes = compiled(
        "while (ck(5, 3, 0)) { evt_next(); } \
         do { evt_next(); } while (ck(5, 4, 1)); \
         for (30) { evt_next(); } \
         for2 (7) { evt_next(); } \
         switch (16) { case (1) { break(0); } default { evt_next(); } }",
    );
    assert_eq!(
        bytes,
        [
            0x0F, 4, 7, 0, 0x21, 5, 3, 0, 0x02, 0x10, 0, // while
            0x11, 0, 7, 0, 0x02, 0x12, 4, 0x21, 5, 4, 1, // do
            0x0D, 0, 3, 0, 30, 0, 0x02, 0x0E, 0, // for
            0x1B, 0, 3, 0, 0, 7, 0x02, 0x0E, 0, // for2
            0x13, 16, 13, 0, // switch
            0x14, 0, 2, 0, 1, 0, 0x1A, 0, // case
            0x15, 0, 0x02, // default
            0x16, 0, // eswitch
        ]
    );
}

#[test]
fn multi_condition_while_writes_join_words() {
    let bytes = compiled("while (ck(1, 2, 0) || ck(1, 3, 0) && ck(1, 4, 0)) { evt_next(); }");
    assert_eq!(&bytes[..4], [0x0F, 16, 19, 0]);
    assert_eq!(&bytes[8..10], [1, 0]);
    assert_eq!(&bytes[14..16], [0, 0]);
}

/// Decompile and compile back: the same scripts.
fn assert_akscr_round_trip(original: &Scripts) -> String {
    let text = akscr::decompile(original);
    let back = akscr::compile(&text).unwrap_or_else(|error| panic!("{error}\n{text}"));
    for (a, b) in [(&back.init, &original.init), (&back.main, &original.main)] {
        assert_eq!(a.to_bytes(), b.to_bytes(), "{text}");
    }
    text
}

#[test]
fn akscr_round_trips_structured() {
    let body = compiled(
        "if (ck(1, 25, 0)) { while (ck(5, 3, 0)) { if () { evt_next(); } } } \
         else { switch (3) { case (0) { for (2) { sleep(0, 10); } } case (1) { } default { } } } \
         do { if (cmp(0, 1, 2, 3)) { break(0); } } while (ck(5, 4, 1)); \
         for2 (9) { gosub(1); } evt_end(0);",
    );
    let original = scripts(&[&body], &[&[0x01, 0]]);
    let text = assert_akscr_round_trip(&original);
    assert!(!text.contains('@'), "{text}");
    assert!(text.contains("    if (ck(1, 25, 0))\n"), "{text}");
    assert!(text.contains("            if ()\n"), "{text}");
}

#[test]
fn akscr_falls_back_to_raw_blocks() {
    // An if with a non-zero pad, an endif that is off by two, an else
    // without its closing nops, and a procedure with trailing bytes.
    let original = scripts(
        &[
            &[0x06, 1, 2, 0, 0x02, 0x08, 0, 0x01, 0, 0xED],
            &[0x06, 0, 2, 0, 0x02, 0x02, 0x08, 0, 0x02, 0x01, 0],
        ],
        &[&[0x06, 0, 5, 0, 0x02, 0x07, 0, 5, 0, 0x02, 0x01, 0]],
    );
    let text = assert_akscr_round_trip(&original);
    assert!(text.contains("@if(1, 2);"), "{text}");
    assert!(text.contains("@endif(0);"), "{text}");
    assert!(text.contains("@else(0, 5);"), "{text}");
    assert!(text.contains("raw(0xED);"), "{text}");
}

#[test]
fn akscr_errors() {
    let compile = |body: &str| akscr::compile_procedure(body).unwrap_err().to_string();
    assert!(compile("endif(0);").contains("block opcode"));
    assert!(compile("bogus(1);").contains("unknown instruction"));
    assert!(compile("ck(1, 2);").contains("takes 3 operands"));
    assert!(compile("if (ck(1, 2, 0) || ck(1, 3, 0)) { }").contains("`&&` only"));
    assert!(compile("raw(1); evt_end(0);").contains("last statement"));
    assert!(compile("switch (1) { default { } case (2) { } }").contains("last section"));
    assert!(akscr::compile("proc main_00 { }").is_err());
    let skipped = akscr::compile("#version 1\n#game re2\nproc main_01 { }").unwrap_err();
    assert!(skipped.to_string().contains("dense"));
    let order = akscr::compile("#version 1\n#game re2\nproc main_00 { }\nproc init_00 { }");
    assert!(order.unwrap_err().to_string().contains("before the main"));
}

#[test]
fn empty_lists_are_empty() {
    let empty = Scripts::default();
    assert!(empty.init.to_bytes().is_empty());
    assert_eq!(akscr::compile(&akscr::decompile(&empty)).unwrap(), empty);
    assert_eq!(asm::assemble(&asm::render(&empty)).unwrap(), empty);
    let _ = Procedure::default();
}
