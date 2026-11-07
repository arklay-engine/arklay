# Room scripts

A room's scripts set it up when the player enters, run every frame while
the player is in it, and drive its events: doors, items, cut scenes,
enemies. They are written in `.akscr`, Arklay's structured text form of
the original games' room scripts: the bytecode the original games keep in each room's RDT
file (SCD scripts). A script reads like C: procedures in braces, commands as
function calls, and `if`, `else` and loops instead of the bytecode's jump
opcodes. The compiler turns a script back into bytecode, and the decompiler
turns the original bytecode into a script. For every retail room of both
games, decompiling and compiling again gives the same bytes.

There are two dialects, one per game. Their instruction sets have nothing in
common, and each dialect has its own compiler:

- [RE 1](re1/index.md): the RE 1 command and event VMs, the include
  files and names, the room variants, and the `.s` assembler.
- [RE 2](re2/index.md): the RE 2 procedure lists and their block opcodes.

This page covers what the two dialects share: where scripts live, how they
are built, and the common shape of the language.

## Where scripts live

A room's scripts are one file in its folder: `room.scd`, the compiled
bytecode, or `room.akscr`, the text the engine compiles when the room loads.
A mod replaces them as a whole; there is no way to patch one procedure or
event. See [Rooms](../../packs/rooms.md#scripts).

`arklay mod build` compiles a mod source directory's `room/<room>/room.akscr`
or `room/<room>.akscr` into `room/<room>/room.scd` (see
[Building a mod](../../mods.md#building-a-mod)). It compiles every `.akscr`
as an RE 1 format script, so an RE 2 format script goes in a mod directory
that is run as it is, without `mod build`.

## Commands

| Command | Does |
|---|---|
| `arklay scd export ROOM.RDT --out room.akscr` | Decompile one RDT's scripts. The command detects whether the RDT is from RE 1 or RE 2 and writes that game's dialect. It also reads a standalone `.scd` container (`scd export room.scd --out room.akscr`). `--out x.s` writes the `.s` disassembly instead, and for RE 1 `--out x.bio` writes a read-only pseudo-C listing. `--list` also writes a `.lst` byte listing (RE 1 only). |
| `arklay scd build room.akscr --out room.scd` | Compile an RE 1 script, or assemble a `.s` source, into a standalone `.scd` container. `--room 6031` builds the bytecode of a single RDT variant instead of the runtime build, and `--region jpn` picks that variant's release. The command does not build RE 2 scripts yet. |
| `arklay scd export-rooms usa=<install> jpn=<install> eur=<install> --out games/re1/room` | Decompile every RE 1 room of one or more installs into one merged script per room, named from the include files (see [The exporter](re1/index.md#the-exporter)). |
| `arklay mod build <dir> --out mod.akpak` | Build a mod source directory. Each RE 1 room script in it is compiled into its room's `room.scd`. |

There is no separate check command. Compiling a script is the check: `scd
build` and `mod build` print the first error, with its line and column.

## The header

Every script starts with a version line. An RE 2 script also has a game line
after it:

```c
#version 1
```

```c
#version 1
#game re2
```

`#version 1` is the only version so far, in both dialects. An RE 2 script
must have `#game re2` before its first statement, and the RE 2 compiler
rejects any other `#game` value. An RE 1 script has no `#game` line, and the
RE 1 compiler rejects one. The line does not choose the compiler. The game
of the room does: an RE 1 room's script always goes through the RE 1
compiler, and an RE 2 room's through the RE 2 one. The line marks which
dialect the file is written in.

## The shared shape

Both dialects use the same layout:

- `//` starts a comment that runs to the end of the line. There are no block
  comments.
- Numbers are decimal (`300`) or hexadecimal (`0x12C`), with an optional
  leading `-`.
- A script is a list of sections: `proc NAME { ... }`, and in RE 1 also
  `event NAME { ... }`. A section's name says which bytecode list or table
  entry it compiles to.
- A statement is one instruction, `name(arg, ...);`, or a control-flow
  statement with a braced body. The body of every `if`, `else` and loop is a
  braced block, even when it holds a single statement.
- `if (a && b) { ... } else { ... }` runs its body when every condition
  passes. Each condition is a condition instruction, such as `ck(...)`.
- `do { ... } while (cond);` runs its body, then repeats it while the
  condition passes.
- `for (N) { ... }` repeats its body N times.
- `raw(0x.., ...);` as the last statement of a section gives the bytes that
  follow its last instruction in the original data.

The rest differs between the dialects:

| | RE 1 | RE 2 |
|---|---|---|
| Sections | `proc init`, `proc main`, named procedures, `event NAME` | `proc init_NN`, `proc main_NN` |
| `#include`, `#define` | yes | no |
| Names for values | engine names, plus a game's include files | numbers only |
| `else if` | yes | no: nest an `if` in the `else` |
| Loops | `for` and `do`/`while` in events only | `while`, `do`/`while`, `for`, `for2` and `switch` in any procedure; `while` and `do` take `&&` and `\|\|` |
| Procedure calls | `NAME();` | `gosub(N);` |
| Fallback for bytecode with no structured form | `asm { ... }` with `.s` lines | `@name(...)` block opcodes |

The dialect pages have the full grammar.
