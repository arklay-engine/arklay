# RE 2 scripts

An RE 2 room runs two lists of procedures, `init` and `main`, which the
original keeps in sections 16 and 17 of the room's RDT. Arklay writes them as
an `.akscr` script that starts with `#version 1` and then `#game re2`. The
RE 2 dialect shares its outline with the [RE 1 dialect](re1.md) (procedures,
`name(args);` instructions, structured `if` and loops; see the
[overview](index.md)), but it is a separate, smaller language with its own
compiler (`src/scd2/akscr.rs`). It has no names, includes or defines: every
operand is a number, written out field by field.

```c
#version 1
#game re2

proc init_00
{
    if (ck(1, 25, 0) && cmp(0, 24, 0, 3))
    {
        set(5, 3, 1);
    }
    else
    {
        gosub(2);
    }
    evt_end(0);
}

proc main_00
{
    while (ck(5, 3, 0))
    {
        evt_next();
    }
    evt_end(0);
}
```

The [differences from RE 1](#differences-from-re-1) are listed at the end of
this page.

## Where the scripts live

`convert-game` writes each RE 2 room into the folder of its RDT,
`room/<stage><room><disc>/`, with the room's scripts in `room.akscr` next to
`room.xml`. The stage is `1` to `7` or one of the letter stages `a` to `g`,
the room is two hex digits, and the disc is `0` for Leon's files (`Pl0`) and
`1` for Claire's (`Pl1`). So `ROOM1000.RDT` becomes `room/1000/room.akscr`,
and Claire's copy of the same room is `room/1001/room.akscr`: each RDT has
its own script. The conversion decompiles the RDT's lists, compiles the text
back and refuses to write a script that does not give the same lists. A room
that the XML cannot carry exactly stays a raw `room.rdt` with no
`room.akscr`. Its scripts come from the RDT.

The engine compiles `room.akscr` each time it loads the room. A script that
does not compile stops the room loading, with the error under
`compiling room/1000/room.akscr`. A folder with a `room.xml` and no
`room.akscr` loads with no scripts, and so without any of the areas and
doors that the scripts would set up.

To get the script of one RDT without converting the whole game:

```sh
arklay scd export ROOM1000.RDT --out room.akscr   # structured script
arklay scd export ROOM1000.RDT --out room.s       # one line per instruction
```

`scd export` detects that the RDT is RE 2's. The `.s` form (`.version 1`,
`.game re2`, `.init`/`.main`, one `.proc` per procedure, `.db` for trailing
bytes, `;` comments) also reassembles byte for byte, but no command builds it
back yet. `--list` is not supported for RE 2 rooms.

`arklay scd build` and `arklay mod build` compile `.akscr` with the RE 1
compiler only. An RE 2 script fails there with
``line 2, column 1: expected a name, found `#` `` (the `#game` line).

## Replacing a room's script in a mod

A mod replaces an RE 2 room's scripts by shipping that room's
`room/<id>/room.akscr`. The file replaces both lists of the room. A mod
cannot patch a single procedure. Layers resolve the entry as they resolve any
other file, so the highest layer that has the file wins.

Because `arklay mod build` cannot compile RE 2 scripts yet (it tries to
compile any `room/<id>/*.akscr` as RE 1), load the mod as a
[mod source directory](../modding.md#mod-source-directories): pass the
directory to `--mod`, or put it in a `mods/` directory that discovery scans.
A directory layer's files are read as they are, and the engine compiles the
script when the room loads.

```text
mods/leon-100/
    manifest.toml          kind = "mod", base = "re2"
    room/1000/room.akscr   Leon's room 100
```

A script applies only to a room that is loaded from its `room.xml`. If the
room comes from a `room.rdt` (a room the conversion kept as an RDT, or one a
layer replaces with an RDT), its scripts come from that RDT and the engine
ignores `room.akscr`.

## A script's layout

```text
#version 1
#game re2

proc init_00 { ... }
proc init_01 { ... }
proc main_00 { ... }
proc main_01 { ... }
proc main_02 { ... }
```

- Both directive lines must come before the first token, in either order.
  `#version` takes only `1` and `#game` only `re2`. No other `#` line is
  accepted.
- `proc init_NN` is procedure `NN` of the init list and `proc main_NN` is
  procedure `NN` of the main list. `NN` is hexadecimal (`main_0A`, then
  `main_0B`). Each list's procedures must be dense and ascending from `00`,
  and every `init` procedure comes before the first `main` one. Either list
  may be empty.
- `//` starts a comment that runs to the end of the line. There are no block
  comments. Apart from the `#` lines, the layout is free: a statement can
  span several lines.
- A procedure is its statements and nothing else. The compiler adds no
  `evt_end`, so end every procedure with `evt_end(0);` yourself.

In a stored list, a procedure runs from its offset to the next procedure's.
The last procedure's end is not stored: a reader ends it at its first
`evt_end` outside every block. So end the last procedure of each list with
an `evt_end` at its top level, as the original's RDTs do. Without it, the
list does not read back the same from its bytes. The compiler does not check
this.

### What runs when

The port's scheduler follows `Sce_scheduler`. It has 14 task slots, and each
task runs one procedure until a `sleep`, `evt_next` or `wsleeping` gives up
the tick, or until the procedure ends.

- **`init_00`** runs once while the room loads, before the room is shown.
  It runs in task 0 until it ends. A sleep in it lasts that many passes, not
  ticks, and the init stops after 256 passes in any case. The other init
  procedures run only if `init_00` reaches them with `gosub` or
  `evt_chain`.
- **`main_00`** then starts in task 0 and runs one pass during the load,
  up to its first `sleep`, `evt_next` or end.
- **`main_01`** starts again from its beginning in task 0 at every tick of
  play. This replaces whatever task 0 was running, so code that must keep
  running across ticks belongs in another task.
- **The other main procedures** run when something starts them: an
  interaction area of kind 5 (an event area, whose data gives the task and the
  procedure), or `evt_exec`, `evt_chain` or `gosub`.
- A task that `init_00` starts with `evt_exec` does not run during the init.
  It first runs in the load's pass, alongside `main_00`, and it runs the
  *main* procedure with that number.

Within a tick the tasks run in slot order, so a task that `evt_exec` starts in
a later slot runs in the same tick. A task can run at most 65,536
instructions in one tick. After that the port stops it for the tick, so that
a script that waits for a state the engine never reaches cannot hang the game.

## Instructions

```c
name(operand, operand, ...);
```

An instruction is its mnemonic from the [opcode table](#opcodes) with one
number for **every** operand field of the opcode, padding bytes included, in
the order of the bytes. `ck(1, 25, 0)` is the bytes `21 01 19 00`. An opcode
without operands is written `name();`.

- A number is decimal (`25`, `-3`) or hexadecimal (`0x19`, `-0x10`). A
  leading `+` and doubled signs are errors.
- An 8-bit field takes `-128` to `255` and a 16-bit field takes `-32768` to
  `65535`, so either way of writing the bits fits: `-1` and `0xFFFF` give the
  same word. A 16-bit field is stored little-endian.
- There are no names: no `TRUE`/`FALSE`, no constants, no `#define`, and no
  `|` to combine values. Compute the value and write it as a number.

The fourteen block opcodes (`if`, `else`, `endif`, `for`, `next`, `while`,
`ewhile`, `do`, `edwhile`, `switch`, `case`, `default`, `eswitch`, `for2`)
cannot be written as plain instructions, because their lengths depend on what
they enclose. Write them with the [structured forms](#blocks), which compute
the lengths, or with an `@` in front (`@if(0, 6);`), which writes the opcode
with the operands as given, lengths included, and checks nothing. The
decompiler uses the `@` form for any block whose stored lengths or padding are
not what the structured form would compile to. `break`, `goto`, `gosub` and
`return` are ordinary instructions: `break(0);`, not `break;`.

### Trailing bytes

```c
proc main_03
{
    evt_end(0);
    raw(0xED, 0x47);
}
```

`raw(...)` gives bytes that follow a procedure's last instruction, as some of
the original's procedures have. It must be the procedure's last statement
and may appear only once in it. Each value must be a byte. The last
procedure of a list cannot have trailing bytes, because a reader ends that
procedure at its closing `evt_end`.

## Blocks

Each structured form compiles to a fixed byte pattern, given below in the
`.s` mnemonics. A length `L` is a 16-bit byte count from the end of the
instruction that holds it. The decompiler recognises a block only when its
bytes match the pattern exactly: every padding operand 0, and every length
pointing exactly at the closing instruction.

Blocks may nest. The compiler accepts up to 256 levels, and the decompiler
writes up to 64 as structured blocks and anything deeper as `@` instructions.

### `if` and `else`

```c
if (ck(5, 3, 0) && cmp(0, 16, 2, 4))
{
    set(5, 3, 1);
}
```

compiles to `if(0, L)`, the conditions, the body and `endif(0)`, where `L`
runs to the end of the `endif`.

```c
if (ck(5, 3, 0))
{
    set(5, 3, 1);
}
else
{
    gosub(2);
}
```

compiles to `if(0, L)`, the conditions, the body, `else(0, J)`, the `else`
body, and two `nop()` bytes. `L` runs to the end of the `else` instruction,
so a failed condition goes into the `else` body. `J` is a signed jump
counted from the *start* of the `else` instruction to the end of the two
`nop` bytes. There is no `endif`. The two `nop` bytes are how the original's
scripts close an `else`, and they are part of the pattern.

- The conditions are instructions joined by `&&`. `||` is an error in an
  `if`, and there is no `!`. To test the opposite, use the operand that
  selects it (`ck(.., .., 0)` for a clear bit, `cmp` with comparison 5 for
  "not equal").
- `if ()` with no condition is allowed, and its body always runs.
- `else if` is not accepted. Write `else { if (...) { ... } }`.
- The head is a list of instructions, not an expression. The compiler takes
  any instruction there, but only the [condition opcodes](#conditions) can
  fail. When it decompiles, the head is every condition opcode that follows
  the `if` within its reach, so a condition at the start of the body comes
  back as part of the head. The bytes and the behaviour are the same.

### `while`

```c
while (ck(5, 3, 0))
{
    evt_next();
}
```

compiles to `while(C, L)`, the condition, the body and `ewhile(0)`. `C` is
the byte length of the condition region (at most 255) and `L` runs to the end
of the `ewhile`. The condition is tested before each pass, and `ewhile` jumps
back to the `while`.

### `do` ... `while`

```c
do
{
    evt_next();
}
while (ck(5, 4, 1));
```

compiles to `do(0, L)`, the body, `edwhile(C)` and the condition. `L` runs to
the end of the condition. The body runs once, then again for as long as the
condition holds. The statement ends with `;`.

### Several conditions in a loop

A `while` or `do` condition may join several conditions with `&&` and `||`:

```c
while (ck(1, 2, 0) || ck(1, 3, 0) && ck(1, 4, 0))
{
    evt_next();
}
```

The condition region is the first condition, then a 16-bit join word before
each further one (0 for `&&`, 1 for `||`), as the executable's `While_main`
reads it. Every condition runs, from left to right, with no precedence and no
short cut. The example above means `(a || b) && c`. The retail scripts never
use this form, so the decompiler writes it as `@while(...)` or
`@edwhile(...)` followed by the instructions the bytes decode to. A join word
decodes as `nop(); nop();` (`&&`) or `evt_end(0);` (`||`). The bytes still
round-trip.

### `for` and `for2`

```c
for (30)
{
    evt_next();
}
for2 (16)
{
    gosub(4);
}
```

`for (count)` compiles to `for(0, L, count)`, the body and `next(0)`, with
`L` running from the end of the 6-byte `for` to the end of the `next`. The
body runs `count` times (a 16-bit count). `for (0)` skips it.

`for2 (variable)` compiles to `for2(0, L, 0, variable)`, the body and
`next(0)`. The count is the value of script variable `variable`, read once
when the loop starts.

### `switch`

```c
switch (16)
{
    case (1)
    {
        cut_chg(2);
        break(0);
    }
    case (2)
    {
    }
    default
    {
        cut_chg(0);
    }
}
```

`switch (variable)` tests the value of script variable `variable` (8-bit
operand). It compiles to `switch(variable, L)`, then each `case (value)` as
`case(0, Lc, value)` followed by its body (with `Lc` running from the end of
the 6-byte `case` to the end of its body), then `default(0)` and its body if
there is one, and `eswitch(0)`. `L` runs to the end of the `eswitch`.

- A `case` value is 16-bit. The `switch` jumps to the first `case` whose
  value matches, or else to `default`, or else past the block.
- `default` must be the last section. A switch can have no sections at all.
- As in C, a `case` body runs on into the next section unless it ends in
  `break(0);`: a `case` or `default` instruction does nothing when the flow
  reaches it.

### `break`, `goto`, `gosub`, `return`

- `break(0);` leaves the innermost `for`, `for2`, `while`, `do` or `switch`,
  and also a `sleep` or `wsleep` wait in progress. It jumps to the end of the
  block and drops any `if` opened inside it. Outside a loop it does nothing.
- `gosub(N);` calls procedure `N` of the list that is running and comes back
  to the instruction after the call. Calls nest at most four deep, and a
  deeper `gosub` is skipped.
- `return(0);` and `evt_end(0);` both return to the caller, or end the task
  when the procedure was not called.
- `goto(if_depth, loop_depth, 0, jump);` jumps by `jump` bytes, counted from
  the start of the `goto`, and keeps `if_depth + 1` open `if`s and
  `loop_depth + 1` open loops (255 for none). There are no labels, so the
  byte distance must be worked out by hand. The decompiler writes it as a
  plain instruction.

## Conditions

These opcodes can fail: `ck`, `cmp`, `dir_ck`, `member_cmp`, `sce_key_ck`,
`sce_trg_ck`, `mizu_div_set`, `keep_item_ck`, `sce_item_get`, `sce_81`,
`sce_85` and `poison_ck`. Where a condition fails decides what happens:

- In an `if` head, or anywhere in an `if` body, a failure jumps to the end
  of the innermost open `if`: into its `else` body if it has one, otherwise
  past its `endif`. The rest of the head is not evaluated. A condition in a
  body therefore works like RE 1's `require(...)`.
- In a `while` or `do` condition, a failure only makes the condition false.
- Anywhere else, a failure ends the task's tick. The next tick goes on with
  the instruction after the condition.

The condition opcodes the engine does not model yet (`mizu_div_set`,
`sce_81`, `sce_85`, `poison_ck`) always pass, and so does `sce_item_get`.

## Flags and variables

`ck(bank, index, value)` tests a flag bit, and passes when the bit is set and
`value` is non-zero, or when the bit is clear and `value` is 0.
`set(bank, index, op)` clears the bit when `op` is 0, sets it when `op` is 1
and toggles it when `op` is 7. Any other `op` does nothing.

There are 32 banks of 256 bits each. A bank number from 32 up reads as clear
and ignores writes. Bit `index` is `0x80000000 >> (index & 31)` of the
bank's 32-bit word `index >> 5`, so the bits are numbered from the most
significant bit of each word, as in the original's `Flg_addr`. Saves keep
every bank. Some banks have a meaning the engine itself reads or writes:

| Bank | What the engine uses in it |
|---|---|
| 0 | `fg_system`: bit 26 Easy, bit 25 censorship off |
| 1 | `fg_status`: bit 0 the player is Claire, bit 1 scenario B, bit 26 a voice line is playing, bit 27 a scene is running. The engine sets bits 0 and 1 on every room entry. |
| 2 | `fg_stop`: bit 7 holds the player's input, bit 3 holds the effects |
| 5 | `fg_room`: the room's own bits. The first 32 are cleared on every room entry. |
| 6, 7 | enemy flags (`sce_em_set`'s "dead" flag). Stages 1 to 3 use bank 6 unless bit 6 of bank 1 is set. The rest use bank 7. Bank 7 also holds the rooms visited, for the map. |
| 8 | items picked up (an item area's flag) |
| 11 | `fg_message`: a question's answer |
| 13 to 28 | a camera's mask groups: bank `13 + camera`, bit `group`. All are set on every room entry, and a script clears a bit to hide that group. |

There are 256 script variables of 16 bits each. `cmp` treats them as signed.

| Variable | Meaning |
|---|---|
| 0, 1 | the interaction area in front of the player and the area the player stands in, set by the area probe |
| 24, 25, 26 | the stage (from 0), the room and the camera on screen. Reads come from the engine. |
| 29 | `sce_rnd`'s random number |

`cmp(0, variable, comparison, value)` compares the variable, as a signed
number, with `value` using comparison `0` `==`, `1` `>`, `2` `>=`, `3` `<`,
`4` `<=` or `5` `!=`. Comparison `6` passes when `variable & value` is not
zero, and any other comparison passes when the variable is not zero.
`calc(0, operation, variable, value)` and `calc2(operation, variable,
source)` change a variable by a constant or by another variable, with
operation `0` add, `1` subtract, `2` multiply, `3` divide, `4` remainder,
`5` or, `6` and, `7` xor, `8` not, `9` shift left, `10` shift right, or
`11` arithmetic shift right. Division and remainder are signed, and dividing
by 0 leaves the variable as it is.

## Opcodes

These are the opcodes 0x00 to 0x8E, from `src/scd2/opcode.rs`. Every opcode
has a fixed width. The mnemonics are biohazard-utils' where it names the
opcode, and otherwise the name of the handler in the executable. The operand
lists give each field's width (`u8`, `i8`, `u16`, `i16`) and, where the
engine reads it, its meaning. `_` is a field the engine does not read, which
is usually 0. **Cond** marks a condition opcode.

The status column shows how far the port runs each opcode:

- **VM**: run by the script VM itself (control flow, flags, variables).
- **yes**: the engine models it.
- **partial**: the engine models only the cases noted. Other cases do
  nothing.
- **no**: not modelled yet. The instruction does nothing, a condition
  passes, and the log notes it once per run as `RE2 script: NAME is not
  modelled`. For the `nop` opcodes, doing nothing is the whole of their
  behaviour.

Every opcode compiles and round-trips, whatever its status.

### Control flow

| Op | Instruction | Status | Description |
|---|---|---|---|
| 00 | `nop()` | no | Does nothing. Two close every `else`. |
| 01 | `evt_end(u8 _)` | VM | Ends the procedure: back to the `gosub` caller, or the task ends |
| 02 | `evt_next()` | VM | Gives up the rest of the tick |
| 03 | `evt_chain(u8 _, u8 _, u8 proc)` | VM | Restarts this task on procedure `proc` |
| 04 | `evt_exec(u8 task, u8 _, u8 proc)` | VM | Starts procedure `proc` in task slot `task`. A `task` from 10 up takes the first free slot from 2 to 9. |
| 05 | `evt_kill(u8 task)` | VM | Stops task `task` |
| 06 | `if(u8 0, u16 L)` | VM | [`if`](#if-and-else) |
| 07 | `else(u8 0, i16 J)` | VM | [`else`](#if-and-else) |
| 08 | `endif(u8 0)` | VM | Closes an `if` without `else` |
| 09 | `sleep(u8 10, u16 ticks)` | VM | Waits `ticks` ticks. The first byte is the `sleeping` opcode (10) that the VM runs next: always write 10. |
| 0A | `sleeping(u16 ticks)` | VM | The counting half of `sleep`. It is not written on its own. |
| 0B | `wsleep()` | VM | Starts a wait for a message or voice line. Follow it with `wsleeping();`. |
| 0C | `wsleeping()` | VM | Gives up the tick while a message window is open or a voice line plays (bank 1 bit 26), at least once |
| 0D | `for(u8 0, u16 L, u16 count)` | VM | [`for`](#for-and-for2) |
| 0E | `next(u8 0)` | VM | Closes `for` and `for2` |
| 0F | `while(u8 C, u16 L)` | VM | [`while`](#while) |
| 10 | `ewhile(u8 0)` | VM | Closes `while` |
| 11 | `do(u8 0, u16 L)` | VM | [`do`](#do--while) |
| 12 | `edwhile(u8 C)` | VM | Closes `do`. The condition follows it. |
| 13 | `switch(u8 variable, u16 L)` | VM | [`switch`](#switch) |
| 14 | `case(u8 0, u16 L, u16 value)` | VM | A `switch` section |
| 15 | `default(u8 0)` | VM | The `switch` section for any other value |
| 16 | `eswitch(u8 0)` | VM | Closes `switch` |
| 17 | `goto(u8 if_depth, u8 loop_depth, u8 _, i16 jump)` | VM | [Jump](#break-goto-gosub-return) |
| 18 | `gosub(u8 proc)` | VM | Calls procedure `proc` of the running list |
| 19 | `return(u8 _)` | VM | Returns to the caller, or the task ends |
| 1A | `break(u8 _)` | VM | Leaves the innermost loop, `switch` or wait |
| 1B | `for2(u8 0, u16 L, u8 0, u8 variable)` | VM | [`for2`](#for-and-for2) |
| 1C | `nop_1c()` | no | No operation |
| 1D | `work_copy(u8 variable, u8 offset, u8 wide)` | VM | Writes the variable into this procedure's own bytes, `offset` bytes past the instruction's end: its low byte, or both bytes when `wide` is not 0. The change lasts until the room's scripts are loaded again. |
| 1E | `nop_1e()` | no | No operation |
| 1F | `nop_1f()` | no | No operation |
| 20 | `nop_20()` | no | No operation |

### Flags, variables and pad

| Op | Instruction | Status | Description |
|---|---|---|---|
| 21 | `ck(u8 bank, u8 index, u8 value)` | VM, cond | [Tests a flag](#flags-and-variables) |
| 22 | `set(u8 bank, u8 index, u8 op)` | VM | Clears (0), sets (1) or toggles (7) a flag |
| 23 | `cmp(u8 _, u8 variable, u8 comparison, i16 value)` | VM, cond | Compares a variable with a constant |
| 24 | `save(u8 variable, i16 value)` | VM | `variable = value` |
| 25 | `copy(u8 to, u8 from)` | VM | `to = from`, between variables |
| 26 | `calc(u8 _, u8 operation, u8 variable, i16 value)` | VM | Arithmetic with a constant |
| 27 | `calc2(u8 operation, u8 variable, u8 source)` | VM | Arithmetic with another variable |
| 28 | `sce_rnd()` | VM | Variable 29 = the next random number |
| 4F | `sce_key_ck(u8 value, u16 mask)` | yes, cond | Passes when any of the pad buttons in `mask` is held and `value` is not 0, or none is held and `value` is 0 |
| 50 | `sce_trg_ck(u8 value, u16 mask)` | yes, cond | As `sce_key_ck`, for the buttons pressed this tick |

### Interaction areas and doors

The `aot_*` instructions fill an interaction-area slot. They share a layout:
`slot`, `sce` (the kind: 0 none, 1 door, 2 item, 4 message, 5 event,
6 flag change, 7 water, 9 typewriter, 10 item box, 11 damage), `sat` (the
trigger byte), `floor`, `super` (an area that moves with a character or an
object), then the box `x, z, w, d`, then the kind's data. The `_4p` forms
replace the box with four corners, `x0, z0, x1, z1, x2, z2, x3, z3`. An event
area's data is the task (a 16-bit word), a padding byte and the main
procedure to start.

| Op | Instruction | Status | Description |
|---|---|---|---|
| 2C | `aot_set(u8 slot, u8 sce, u8 sat, u8 floor, u8 super, i16 x, i16 z, i16 w, i16 d, u8 data[6])` | yes | An interaction area |
| 67 | `aot_set_4p(u8 slot, u8 sce, u8 sat, u8 floor, u8 super, i16 corners[8], u8 data[6])` | yes | An area with four corners |
| 3B | `door_aot_set(u8 slot, u8 sce, u8 sat, u8 floor, u8 super, i16 x, i16 z, i16 w, i16 d, i16 next_x, i16 next_y, i16 next_z, i16 next_dir, u8 next_stage, u8 next_room, u8 next_cut, u8 _, u8 do2, u8 door_type, u8 sound, u8 lock, u8 key, u8 _)` | yes | A door: the area, then the position, facing, stage (from 0), room and camera to arrive at, the DO2 file and door type of the animation, its sound, the lock and the key |
| 68 | `door_aot_set_4p(...)` | yes | `door_aot_set` with four corners: 40 bytes |
| 4E | `item_aot_set(u8 slot, u8 sce, u8 sat, u8 floor, u8 super, i16 x, i16 z, u16 w, u16 d, u16 item, u16 count, u16 flag, u8 model, u8 action)` | yes | An item to pick up. `flag` is its bit in bank 8. When that bit is set, the area does nothing and the item's object stays hidden. |
| 69 | `item_aot_set_4p(...)` | yes | `item_aot_set` with four corners: 30 bytes |
| 46 | `aot_reset(u8 slot, u8 sce, u8 sat, u8 data[6])` | yes | Changes an existing area's kind, trigger and first three data words. Its box, floor and the rest stay. |
| 47 | `aot_on(u8 slot)` | yes | Fires the area as if the player had triggered it |
| 4D | `door_model_set(...)` | no | 22 bytes |

### Characters, objects and the work

`work_set(kind, index)` chooses what the task's later instructions act on:
kind 1 the player, 2 the partner, 3 the character in enemy slot `index`,
4 room object `index`. It also clears the task's speeds. The position,
member and `plc_*` instructions then apply to that choice. On a character or
object the engine does not model, they do nothing.

| Op | Instruction | Status | Description |
|---|---|---|---|
| 2E | `work_set(u8 kind, u8 index)` | yes | Chooses the task's work |
| 2F | `speed_set(u8 index, i16 value)` | yes | Sets one of the task's twelve speeds |
| 30 | `add_speed()` | yes | Moves the work by its speeds |
| 31 | `add_aspeed()` | yes | Adds speeds 6 to 11 to speeds 0 to 5 |
| 32 | `pos_set(u8 _, i16 x, i16 y, i16 z)` | yes | Places the work |
| 33 | `dir_set(u8 _, i16 x, i16 y, i16 z)` | yes | Turns the work. A character uses `y`. |
| 34 | `member_set(u8 member, i16 value)` | yes | Writes a field of the work |
| 35 | `member_set2(u8 member, u8 variable)` | yes | Writes a field from a variable |
| 3D | `member_copy(u8 variable, u8 member)` | yes | Reads a field into a variable |
| 3E | `member_cmp(u8 _, u8 member, u8 comparison, i16 value)` | yes, cond | Compares a field. A field with nothing behind it fails. |
| 55 | `member_calc(u8, u16, i16)` | no | |
| 56 | `member_calc2(u8, u8, u8)` | no | |
| 39 | `dir_ck(u8 _, i16 x, i16 z, i16 range)` | yes, cond | Passes when the point is within `range` of the way the work faces |
| 3F | `plc_motion(u8, u8, u8)` | yes | Starts a scripted motion |
| 40 | `plc_dest(u8 _, u8 mode, u8 flag, i16 x, i16 z)` | yes | Walks the work to `x, z`. Bank 5 bit `flag` is set on arrival. A character the engine does not model arrives at once. |
| 41 | `plc_neck(u8 mode, i16 x, i16 y, i16 z, u8 rate, u8 rate)` | yes | Turns the head |
| 42 | `plc_ret()` | yes | Ends the scripted motion |
| 43 | `plc_flg(u8 how, u16 value)` | yes | The scene flags: or (0), set (1), xor (2) |
| 58 | `plc_rot(u8 which, u16 value)` | yes | |
| 5B | `plc_cnt(u8 count)` | yes | The motion counter |
| 66 | `plc_stop()` | yes | Stops the work |
| 4A | `plc_gun(u8)` | no | |
| 44 | `sce_em_set(u8 _, u8 slot, u8 id, u8 kind_lo, u8 kind_hi, u8 floor, u8 sound_type, u8 model_type, u8 dead_flag, i16 x, i16 y, i16 z, i16 dir, u16, u16)` | yes | Places a character in enemy slot `slot` (255: the partner). `dead_flag` is its bit in the enemy bank, and a character whose bit is set is not placed (255: never). |
| 8E | `sce_em_set2(...)` | no | 24 bytes |
| 2D | `obj_model_set(...)` | yes | Places a room object's model: 38 bytes, the fields of the object table |
| 60 | `kage_set(u8 kind, u8 slot, u8 r, u8 g, u8 b, i16, i16, i16, i16)` | partial | A shadow or pool under a body. Modelled for kind 3 (an enemy slot) only. |
| 6E | `parts_set(u8 _, u8 part, u8 case, i16 value)` | partial | Modelled for case 2 only (a part's flags) |
| 7A | `sce_parts_bomb(u8 _, u8 part, ...)` | partial | Hides the part. The pieces that fly off are not drawn. |
| 7B | `sce_parts_down(u8 part, i16 angle, i16 along, i16 up, u8 gravity, u8 _, i16 x, i16 y, i16 z)` | yes | A zombie drops a part |
| 48 | `super_set(...)` | no | 16 bytes |
| 49 | `super_reset(u8, i16, i16, i16)` | no | |
| 72 | `super_on(...)` | no | 16 bytes |
| 6C | `rbj_reset()` | no | |
| 70 | `splc_ret()` | no | |
| 71 | `splc_sce()` | no | |

### Camera, room and screen

| Op | Instruction | Status | Description |
|---|---|---|---|
| 29 | `cut_chg(u8 cut)` | yes | Switches to camera `cut` and holds it. The camera on screen is kept for `cut_old`. |
| 2A | `cut_old()` | yes | Back to the kept camera, released |
| 3C | `cut_auto(u8 on)` | yes | 0 holds the camera, anything else lets the player's position switch it again |
| 61 | `cut_be_set(u8 cut, u8 index, u8 value)` | yes | Sets the gate of camera-switch zone `index` of camera `cut` |
| 4B | `cut_replace(u8 a, u8 b)` | yes | Swaps two camera ids in every camera-switch zone |
| 37 | `sca_id_set(u8 index, u16 id)` | yes | Changes a collision record's id |
| 38 | `flr_set(u8, u8)` | no | |
| 45 | `col_chg_set(u8, u8, u8, u8)` | no | |
| 6D | `sce_scr_move(u8 _, i16 y)` | yes | Scrolls the background down by `y` |
| 53 | `sce_fade_set(u8 fade, u8 kind, u8 _, i16 add)` | yes | Starts a fade |
| 74 | `sce_fade_adjust(u8 fade, i16 value)` | yes | Adjusts a fade |
| 5C | `sce_shake_on(i8 x, i8 y)` | yes | Offsets the picture until the next `sce_shake_on` |
| 73 | `mirror_set(u8, u16, u16, u16)` | no | |
| 5D | `mizu_div_set(u8)` | no, cond | |
| 6F | `movie_on(u8)` | no | |
| 6A | `light_pos_set(u8 _, u8 light, u8 axis, i16 value)` | yes | Moves a light of the camera on screen. `axis` is 11 for x, 12 for y and 13 for z. |
| 6B | `light_kido_set(u8 light, i16 radius)` | yes | A light's radius, on the camera on screen |
| 7C | `light_color_set(u8 light, u8 r, u8 g, u8 b, u8 _)` | yes | A light's colour, on the camera on screen |
| 7D | `light_pos_set2(u8 cut, u8 light, u8 axis, i16 value)` | yes | `light_pos_set` on camera `cut` |
| 7E | `light_kido_set2(u8 _, u8 cut, u8 light, u16 radius)` | yes | `light_kido_set` on camera `cut` |
| 7F | `light_color_set2(u8 cut, u8 light, u8 r, u8 g, u8 b)` | yes | `light_color_set` on camera `cut` |
| 77 | `sce_line_start(u8, u16)` | no | |
| 78 | `sce_line_main(u8, i16, i16)` | no | |
| 79 | `sce_line_end()` | no | |

### Messages, sound and music

| Op | Instruction | Status | Description |
|---|---|---|---|
| 2B | `message_on(u8 _, u8 message, u8 _, u8 stop_lo, u8 stop_hi)` | yes | Shows room message `message`. The last two bytes are a word of `fg_stop` bits held while it shows: `0xFFFF` for a check in play, 0 in a scene. |
| 36 | `se_on(u8 type, i16 sound, i16 work, i16 x, i16 y, i16 z)` | yes | Plays the sound that the first two operands select, at an offset from the player, the partner, an enemy slot or an object (`work`'s low byte 1 to 4, its high byte the slot), or at a fixed point (0) |
| 80 | `se_vol(u8)` | no | |
| 51 | `sce_bgm_control(u8, u8, u8, u8, u8)` | yes | Starts, stops or changes a music channel |
| 57 | `sce_bgmtbl_set(u8 _, u8 room, u8 stage, u16 low, u16 high)` | yes | Sets the music-table entry of `room` in `stage` (from 1) to `low \| high` |
| 59 | `xa_on(u8 mode, u16 line)` | yes | Plays voice line `line`: one of the stage's own lines when `mode` is 0, one of stage 0's otherwise. Sets bank 1 bit 26 while it plays. |
| 5F | `xa_vol(u8 level)` | yes | The voice volume (below 128) for the next line |

### Effects

| Op | Instruction | Status | Description |
|---|---|---|---|
| 3A | `sce_espr_on(u8 _, u16 id, u16 matrix, u16 size, i16 x, i16 y, i16 z, i16 dir)` | yes | An effect sprite. `id`'s low byte is the type and its high byte the row. `matrix` attaches it to a body. |
| 64 | `sce_espr_on2(u8 group, u16 id, u16 matrix, u16 size, i16 x, i16 y, i16 z, u16 dir)` | yes | The same, in a group for `sce_espr_kill2` |
| 54 | `sce_espr3d_on(u8 _, u16 id, u16 matrix, u16 size, i16 x, i16 y, i16 z, i16 rx, i16 ry, i16 rz, i16 dir)` | yes | A 3D effect |
| 75 | `sce_espr3d_on2(u8 group, ..., i16 rz, u16 dir)` | yes | A 3D effect in a group |
| 4C | `sce_espr_kill(u8, u8, u8, u8)` | yes | Ends effects by the first two bytes |
| 65 | `sce_espr_kill2(u8 group)` | yes | Ends a group's effects |
| 52 | `sce_espr_control(u8, u8, u8, u8, u8)` | no | |

### Items and the player's state

| Op | Instruction | Status | Description |
|---|---|---|---|
| 5E | `keep_item_ck(u8 item)` | yes, cond | Passes when the player carries `item` |
| 76 | `sce_item_get(u8 item, u8 count)` | yes, cond | Gives the player `count` of `item`. It always passes. |
| 62 | `sce_item_lost(u8 item)` | yes | Takes `item` away |
| 5A | `weapon_chg(u8 item)` | yes | Equips `item` (0: nothing) |
| 88 | `sce_item_ck_lost(u8, u8)` | no | |
| 86 | `poison_ck()` | no, cond | |
| 87 | `poison_clr()` | no | |
| 83 | `sce_heal()` | no | |
| 89 | `heal_partner()` | no | |
| 8A | `vib_set0(u8, u16, u16)` | yes | Does nothing: the PC executable has no rumble either |
| 8B | `vib_set1(u8, u16, u16)` | yes | Does nothing, as `vib_set0` |
| 8C | `vib_fade_set(u8, u16, u16, u16)` | no | |

### Unnamed and unused

| Op | Instruction | Status | Description |
|---|---|---|---|
| 63 | `nop_63()` | no | |
| 81 | `sce_81(u8, u8)` | no, cond | |
| 82 | `sce_82(u8, u8)` | no | |
| 84 | `sce_84(u8)` | no | |
| 85 | `sce_85(u8, u8, u8, u16)` | no, cond | |
| 8D | `sce_8d(...)` | no | 24 bytes |

A byte from 0x8F up is not an opcode. In an RDT, such a byte ends the
procedure's instructions, and the rest of the procedure becomes its
[trailing bytes](#trailing-bytes).

## A worked example

This script shows the common patterns. The area coordinates and the trigger
byte (`0x01`) are placeholders. Copy the real ones from one of the room's own
`aot_set`s.

```c
#version 1
#game re2

proc init_00
{
    // Area 2: an event area (kind 5) that runs main procedure 2 in the first
    // free task (10): data is the task word, a padding byte, the procedure.
    if (ck(5, 4, 0))
    {
        aot_set(2, 5, 0x01, 0, 0, -1200, 4400, 1500, 1200, 10, 0, 0, 2, 0, 0);
    }
    evt_end(0);
}

proc main_00
{
    evt_end(0);
}

proc main_01
{
    // Restarted every tick.
    evt_end(0);
}

proc main_02
{
    set(5, 4, 1);                // once only
    aot_reset(2, 0, 0, 0, 0, 0, 0, 0, 0);
    set(2, 7, 1);                // hold the player's input
    cut_chg(3);
    sleep(10, 30);
    message_on(0, 5, 0, 0, 0);
    wsleep();
    wsleeping();
    cut_old();
    set(2, 7, 0);
    evt_end(0);
}
```

## Round trips

- **Bytes to text to bytes.** Any procedure lists decompile to a script that
  compiles back to identical bytes, including blocks with unusual lengths
  (written with `@`), undecodable bytes (written with `raw`) and blocks
  nested deeper than the decompiler structures. The tests check this on
  random bytes. All 501 retail RE 2 rooms decompile with every block
  structured. `convert-game` checks the round trip for every room it writes.
- **Text to bytes to text.** This gives back the canonical form, not the
  same text. Comments and layout are lost, a condition at the start of an
  `if` body moves into the head, and a loop with several conditions comes
  back as `@` instructions. Compiling the result again gives the same bytes.
- The decompiler writes every operand as a decimal number, except trailing
  bytes, which it writes as hexadecimal. It names nothing.

## Compiler errors

Errors give the line (not the column) and stop the compilation. The most
common ones:

| Error | Cause |
|---|---|
| ``the script must start with `#version 1` and `#game re2` `` | A token before both directives, or a directive missing |
| `` unsupported `#version 2`; expected 1 `` / `` `#game re1` is not re2 `` / ``unknown directive `#define ...` `` | Any other `#` line |
| ``unexpected `!` `` | A character outside the grammar: `!`, a single `\|`, `=`, `"` and so on |
| `` `main_1G` is not `init_NN` or `main_NN` `` | A bad procedure name |
| ``expected `proc main_01`; procedures must be dense and ascending`` | A procedure number skipped or out of order |
| `` `proc init_00` must come before the main procedures `` | An `init` procedure after a `main` one |
| ``unknown instruction `msg_on` `` | A mnemonic the opcode table does not have |
| `` `endif` is a block opcode; use its structured form or `@endif(...)` `` | A block opcode written as a plain instruction |
| `` `ck` takes 3 operands, got 2 `` | Every operand field must be given |
| ``operand 1 of `cmp` (256) does not fit its 8-bit field`` | A value out of range |
| ``an `if` condition joins with `&&` only`` | `\|\|` in an `if` |
| ``expected `{`, found `if` `` | `else if`. Nest the `if` inside an `else` block instead. |
| ``expected `(`, found `;` `` | `break;` or `return;`. Write `break(0);` and `return(0);`. |
| `` `default` must be the switch's last section `` | A `case` after `default` |
| `` `raw(...)` must be the procedure's last statement `` | A statement, or a second `raw`, after `raw` |
| `the block is N bytes, over 65535` / `the else block is over 32767 bytes` / `the condition is N bytes, over 255` | A block or condition region too long for its length field |
| `blocks nest more than 256 deep` | Nesting deeper than the compiler accepts |
| `main procedure 1F starts at byte N, past the list's 64 KiB` | The list's offset table is 16-bit |
| `the last main procedure cannot have trailing bytes` | `raw` in a list's last procedure |

## Differences from RE 1

The two dialects look alike, but they differ in ways that are easy to trip
over:

- **`ck` and `set` read their last operand the other way round.** In RE 2,
  `ck(bank, bit, 1)` passes when the bit is set, and `set(.., .., 7)`
  toggles. In RE 1, `TRUE` is 0 and `TOGGLE` is 2.
- **No names and no preprocessor.** RE 2 scripts have no `#include`,
  `#define`, `#if`, `*.akinc` files, engine constants, `TRUE`/`FALSE` or `|`.
  Every operand, padding included, is a number.
- **No implicit end.** RE 1 closes every block with an implicit `end 0`
  unless the section says `noreturn`. RE 2 adds nothing and has no
  `noreturn`, so write `evt_end(0);` at the end of each procedure.
- **Numbered procedures, no events.** RE 2 has no event VM and no `event`
  sections: the main procedures, run as tasks, take that role. Procedures are
  `init_NN` and `main_NN`, and are called with `gosub(N)`, not `name();`.
- **Instructions in the place of keywords.** RE 2 writes `return(0);`,
  `break(0);` and `sleep(10, ticks);`, has no `require` (a condition in an
  `if` body does the same) and no `wait_message()` (`wsleep(); wsleeping();`).
- **Smaller `if`.** RE 2 has no `else if`, no `!` and no `||` in an `if`.
  The `&&`/`||` of a loop condition are evaluated left to right with no
  precedence.
- **`@name(...)` instead of `asm { }`.** Raw forms are single instructions
  with explicit operands. There is no embedded `.s` block.
- **One script per RDT.** Leon's and Claire's rooms are separate files, and
  there are no merged variants, region tests, `when` or `nopad`.
- **Generated, not checked in.** The RE 1 scripts are maintained as source
  under `games/re1/room/`. The RE 2 scripts are written by `convert-game` from
  the install, and the repository has none.
- **Tooling.** `scd build` and `mod build` take only RE 1 scripts. Errors
  give a line but not a column.
