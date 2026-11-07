# RE 1 scripts

This page is the reference for RE 1 `.akscr` room scripts: the files, the
language, the room variants, the names a script can use and every
instruction. [Room scripts](../index.md) covers what the RE 1 and RE 2
dialects share.

An RE 1 room has three kinds of script, run by two virtual machines:

- **`proc init`** runs once when the room loads. It sets up the room's doors,
  items, examine spots, enemies and objects.
- **`proc main`** runs once every frame while the player is in the room.
- **Events** are the room's cutscenes and interactions. Up to eight run side
  by side, one per event slot, and each is stepped once a frame until it
  yields. `proc init`, `proc main` or an examine spot starts them.

`proc init`, `proc main` and the procedures they call run on the command VM.
Each runs from start to end in one go. Events run on the event VM, which can
wait across frames and steps an actor or a moving model one instruction at a
time.

A short room script (room 104, the tea room, abridged):

```c
#version 1

#include "inc/doors.akinc"
#include "inc/enemies.akinc"
#include "inc/flags.akinc"
#include "inc/items.akinc"
#include "inc/messages.akinc"
#include "inc/movies.akinc"
#include "inc/rooms.akinc"

#define AOT_03_DOOR_105 0x03
#define AOT_04_EVENT_KENNETH 0x04
#define CAM_04_KENNETH 0x04
#define ENEMY_00_FIRST_ZOMBIE 0x00

proc init
{
    door_aot_set(AOT_03_DOOR_105, 10600, 0, 2800, 2300, 1, DOOR_SND_00_DR_WD, DOOR_03_DOOR03, 4, 0, ROOM_105_DINING_ROOM, 6300, 0, 14100, 1024, UNLOCKED, SAT_MANUAL | SAT_PLAYER);
    if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, FALSE))
    {
        aot_set(AOT_04_EVENT_KENNETH, 3300, 5600, 2000, 2000, SCE_EVENT, SAT_MANUAL | SAT_PLAYER, 9, 0, examine_kenneth, 0, 0, 0);
    }
}

proc main
{
    if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, FALSE))
    {
        kenneth_zombie_checks();
    }
}

// The first zombie film starts when the camera reaches Kenneth's body, and
// his examine spot goes once he has been examined.
proc kenneth_zombie_checks
{
    if (ck(FG_COMMON, F_COMMON_KENNETH_ZOMBIE_SEEN, FALSE))
    {
        if (ck(FG_STATUS, F_STATUS_CAMERA_REDRAW, TRUE))
        {
            if (cmpb(B_CAMERA, CMP_EQ, CAM_04_KENNETH))
            {
                evt_exec(0, TASK_09, first_zombie_film);
            }
        }
    }
    if (ck(FG_SCENARIO, F_SCENARIO_KENNETH_EXAMINED, TRUE))
    {
        aot_delete(AOT_04_EVENT_KENNETH, SCE_NONE, SAT_PLAYER);
    }
}

event examine_kenneth
{
    run
    {
        cutnext(CAM_05_KENNETH_CLOSEUP);
        set(FG_RUN, F_RUN_CONTROLS, FALSE);
        message(MSG_HE_IS_KENNETH_FROM_THE_STARS | MSG_FAST, 0);
    }
    do
    {
        evt_next();
    }
    while (cmpb(B_MENU_CHOICE, CMP_GE, 128));
    run
    {
        cutnext(CAM_04_KENNETH);
        set(FG_RUN, F_RUN_CONTROLS, TRUE);
        cut_auto(CUT_FREE);
    }
    evt_finish();
}

event first_zombie_film
{
    run
    {
        bgm_stop_all(0);
        movie_on(MOVIE_03_FIRST_ZOMBIE);
    }
    sleep(1);
    run
    {
        eml_state(0, ENEMY_00_FIRST_ZOMBIE, EML_BEHAVIOUR, EB_05);
        set(FG_COMMON, F_COMMON_KENNETH_ZOMBIE_SEEN, TRUE);
    }
    evt_finish();
}
```

## Files and builds

### The checked-in scripts

Each RE 1 room has one script, `games/re1/room/<room>/room.akscr`. The room
is three lower-case hex digits: the stage and the room number (`103`). The
script sits beside the room's `room.xml`. One script covers every variant of
the room: both players, both mansion visits and every release (see
[Room variants](#room-variants)). The original game's stages 6 and 7 are the
second mansion visit, so `103/room.akscr` holds RDTs 1030, 1031, 6030 and
6031. The scripts include the game's names from `games/re1/inc/*.akinc`
(see [Include files](#include-files)).

When Arklay is compiled, its build script compiles every room script once,
as the runtime build, and stores the bytecode in the executable.
`arklay convert-game` packs each room's bytecode as `room/<room>/room.scd`.
It also records, as the base manifest's `region`, the release that the
install's language belongs to: Japanese is JPN, French or German is EUR,
and any other language is USA. The scripts' region tests read that region.

The checked-in scripts are maintained as source, not as copies of the
original bytecode: they keep the original game's behaviour, not its bytes.
They differ from the RDTs where one of the original's quirks has a clean
form that behaves the same, and where a room's message ids were renumbered
so that all its variants share one table of texts. To check a change, play
the rooms against the previous build: the traces and frames must not change.
Comparing bytecode does not check it.

The regions ship different scripts. The English release (USA) gives two ink
ribbons per pickup where JPN gives three, and it adds `bgm_stop` calls to
cutscenes and adjusts waits and camera cuts. The French and German releases
(EUR) ship identical scripts, mostly following JPN for the ribbons, plus
rooms 313 and 412 that only they have. The Steam/GOG Japanese release ships
the same scripts as the original Mediakite PC release, so the differences
are regional, not per store. Every such difference tests a release bit, so
`grep -rn F_STATUS_REGION games/re1/room` lists them all.

### Builds

A script compiles to one of two builds:

- **The runtime build** carries every variant. Each place where the
  variants differ is an `if` that tests the flags the engine keeps in step
  with the variant. This is what ships, and what `scd build` and
  `mod build` write by default.
- **A variant build** is the bytecode of one RDT, for one release.
  `arklay scd build 103/room.akscr --room 6031 --out 6031.scd` builds Jill's
  second visit (the original's numbering), for the USA release unless you
  ask for another (`--region jpn`). Tests written by variant name fold away.
  A variant build exists to compare with the original RDTs; the engine does
  not run one.

A script has no preprocessor choices: the variants are run-time tests.

### Mods

A mod replaces the scripts of a whole room. It ships its scripts as
`room/<room>/room.scd`, and a mod source directory can carry them as a
script: `room/<room>/room.akscr`, laid out like the checked-in ones, or
`room/<room>.akscr` named by three hex digits. `arklay mod build` compiles
either into the room's `room/<room>/room.scd` as the runtime build, so one
script serves every player, visit and region. The `#include` paths resolve
against the script's directory and then each directory above it, so a mod
can keep its own `inc/` folder beside `room/`. `.akinc` files only feed the
scripts and are not packed. A `room/<room>/<stem>.s` source is assembled
into `<stem>.scd` the same way.

The engine takes a room's scripts from the highest layer that has them. That
layer's `room.scd` wins over the RDT's own embedded scripts. A mod that ships
its own `room.rdt` and no `room.scd` runs that RDT's scripts, and a mod that
only edits `room.xml` keeps the base scripts. A script override replaces the
room's init, main and event scripts all together. So a mod that replaces only
part of a room still has to carry the rest of its script. To start, export
the room (`scd export ROOM1000.RDT --out room.akscr`) or copy its checked-in
script. A demo room's own doors and events are inert unless the override
keeps them.

## Lexical structure

- `//` starts a comment that runs to the end of the line.
- A name is a letter or `_` followed by letters, digits and `_`. Names are
  case-sensitive.
- A number is decimal (`3072`) or hex (`0x0C00`). A `-` before a number or a
  defined name negates it (`-7200`, `-OFFSET`).
- The punctuation is `( ) { } , ; | && ! - #`. Any other character is an
  error.
- `asm` starts an embedded assembly block (see
  [Embedded assembly](#embedded-assembly)), whose lines are not tokenised.

Errors name the line and, where they can, the column. The preprocessor
blanks the lines it consumes, so line numbers always point into the source
file.

## The preprocessor

### `#version`

The script must start with `#version 1`. Only comments, blank lines and the
other directives may come before it.

### `#include`

```c
#include "inc/items.akinc"
```

`#include "path"` reads `#define` lines from a file. The path must be
relative and may not contain `..`. It resolves against the script's
directory first and then each directory above it, so
`games/re1/room/103/room.akscr` finds `games/re1/inc/items.akinc`. Each path
is read once, however many times it is included. An include file holds only
`#define` and `#include` lines, comments and blank lines.

### `#define`

```c
#define AOT_02_EVENT 0x02
#define MSG_IT_IS_LOCKED MSG_IT_IS_LOCKED_A_CARVING_OF_A_SWORD   // an alias
```

`#define NAME value` names a number. The value is one number or a name
defined earlier. A define takes no expressions and no parameters, and it can
end in a `//` comment. Defining a name a second time with the same value is
allowed. Defining it with a different value is an error.

A defined name can stand anywhere a number can: in an argument, a `for`
count, a `raw` byte or an `asm` line. Event names and procedure names share
the same namespace as defines, so an event, a procedure and a define cannot
have the same name.

Game data has no built-in names. A game names its items, enemies, rooms and
flags in include files, and using a name that is neither defined nor one of
the engine's own is an error.

### `#if`

The `#if` / `#else` / `#endif` lines belong to an older merged form, which
the exporter writes internally before it turns the variant blocks into
run-time tests. A script you compile may not use them. `#if JILL` is an
error that says to write `if (JILL) { ... }` instead (see
[Variant names](#variant-names)).

## Program structure

A script is `#version 1`, its directives, then sections. A section is a
header and a braced body:

| Section | Is |
|---|---|
| `proc init` | One block of the room's init list, run once at room load. |
| `proc main` | One block of the room's main list, run every frame. |
| `proc NAME` | A procedure, which `NAME();` calls. |
| `event NAME` | One event script. |

The order matters for some sections:

- Every `proc init` comes before every `proc main`, and every `proc main`
  before the first event. Repeat the section for a list with several blocks.
  The blocks run in order, and each ends at its own `return;` or its end.
- Events are numbered in the order they stand, from 0. Any event can name
  any other, before or after it, and the name stands for its number:
  `evt_exec(0, TASK_09, first_zombie_film)`. An event named `event_XX`
  should stand at position XX. A variant build reports an error otherwise.
  Where an instruction takes an event (`evt_exec`, `evt_fork`), `event_XX`
  with no event of that name is the number XX.
- Procedures may stand anywhere. They are numbered in the order they stand.

### Section flags

A header can carry flags after the name:

| Flag | On | Means |
|---|---|---|
| `noreturn` | `proc init`, `proc main` | The block has no implicit `end` at its close, so it runs off the end of its bytecode, which ends the whole block list: the blocks after it do not run that time. The exporter uses it where the original block has no `end`. |
| `nopad` | `event` | An event that ends in `evt_finish` is zero-padded to four bytes. `nopad` leaves the padding out. |
| `when (...)` | `proc init`, `proc main`, `event` | The section exists only in the variants named. See [Variant names](#variant-names). |

`noreturn` and `nopad` can take a variant condition too: `proc init noreturn
(MANSION2)`, `event event_05 nopad (MANSION1)`. A procedure takes no flags.

### Procedures

```c
proc main
{
    if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, FALSE))
    {
        kenneth_zombie_checks();
    }
}

proc kenneth_zombie_checks
{
    if (ck(FG_SCENARIO, F_SCENARIO_KENNETH_EXAMINED, TRUE))
    {
        aot_delete(AOT_04_EVENT_KENNETH, SCE_NONE, SAT_PLAYER);
    }
}
```

`proc NAME`, with any name but `init` and `main`, is a procedure, and
`NAME();` runs it and comes back. A call can stand in `proc init`, `proc
main`, another procedure, or an event: inside a `run` block, or on its own
as an event statement.

- A procedure takes no arguments and no `noreturn` or `when`. Test the
  variant with an `if` inside it.
- A procedure may not call itself, directly or through other procedures.
- Its `return;` returns to the caller from any depth. So does a failed
  condition at its top level, and so does its end.
- A script has at most 256 procedures.

A call compiles to the port's `gosub N` (opcode 0x54). The procedures
compile into a container of their own, which the original RDTs have no room
for. So a script with procedures runs only in Arklay, and a variant build
carries the procedures too.

## Command code

The bodies of `proc` sections and of `run` blocks are command code.

### Commands

```c
set(FG_COMMON, F_COMMON_KENNETH_ZOMBIE_SEEN, TRUE);
message(MSG_HE_IS_KENNETH_FROM_THE_STARS | MSG_FAST, 0);
```

A command is `name(args);`. The names are the mnemonics of the
[instruction reference](#the-instruction-set). An argument is:

- a number, or `-` and a number;
- a defined name, or one of the engine's own names (see
  [Names](#names));
- values ORed together with `|`: `SAT_MANUAL | SAT_PLAYER`,
  `STAGE_2 | ROOM_201_..`.

Each command takes a fixed number of arguments, and each argument must fit
its field: a byte field takes -128 to 255 and a word field -32768 to 65535.

### `if`, `else` and `else if`

```c
if (ck(FG_COMMON, F_COMMON_C_BAR_WOOD_EMBLEM_SET, TRUE) && ck(FG_STATUS, F_STATUS_PLAYER_JILL, FALSE))
{
    set(FG_COMMON, F_COMMON_C_REBECCA_LEFT_BAR, TRUE);
}
else if (ck(FG_ROOM, F_ROOM_02, FALSE))
{
    require(testitem(IT_33_KEY_MANSION_SWORD));
    return;
}
else
{
    cut_auto(CUT_FREE);
}
```

The head is one or more condition commands joined by `&&`. The body runs
when every condition passes. Only the
[condition commands](opcodes/index.md#conditions) may stand in a head, and there is no `||`
or `!`: write the opposite test (`FALSE` for `TRUE`, `CMP_NE` for `CMP_EQ`)
or put the other case in the `else`.

An `if` compiles to the original's `if` / `else` / `endif` opcodes, whose
jumps are one byte long. When a body is too long for that, the compiler
uses the port's `if_long` and `else_long` (opcodes 0x51 and 0x52, a 2-byte
jump), which behave the same. The command VM keeps the open `if`s on a
stack 16 deep, so do not nest `if`s deeper than that.

### `require`

```c
require(testitem(IT_33_KEY_MANSION_SWORD));
```

`require(cond);` tests a condition outside an `if` head. When it fails, the
flow leaves the enclosing `if` body: it goes on after the `if`, or into its
`else` body when it has one, as a failed condition of the `if` would. At the
top level of a block a failed `require` ends the block, and in a procedure
it returns to the caller.

### `return`

`return;` ends the block at any depth, inside `if` bodies too, as the
original's `end` does. Every block ends with an implicit `return;` unless
the section says `noreturn`. In a procedure, `return;` returns to the
caller.

### Embedded assembly

```c
asm
{
    set                     FG_ROOM, 3, 0
    cutnext                 2
}
```

`asm` followed by `{` embeds `.s` assembly lines verbatim. The `{` stands
alone at the end of the `asm` line or on the next line, and a line holding
only `}` closes the block. The lines use the [`.s` grammar](#the-s-assembler),
with the script's defines replaced by their values. `asm` can stand anywhere
a statement can. The decompiler writes it only where the shipped bytecode
has no structured form, such as an `if` that jumps into the middle of an
instruction.

## Event scripts

An event is a list of event statements:

```c
event first_zombie_film
{
    run
    {
        bgm_stop_all(0);
        movie_on(MOVIE_03_FIRST_ZOMBIE);
    }
    sleep(1);
    run
    {
        room_bgm_state_set(0, ROOM_104_TEA_ROOM, BGM_MAIN_ON);
        set(FG_COMMON, F_COMMON_KENNETH_ZOMBIE_SEEN, TRUE);
    }
    evt_finish();
}
```

An event runs in one of eight event slots. Each frame the event VM steps
every active slot until it yields: `evt_next()`, `sleep`, a wait, or an
actor instruction that takes several frames. A slot stays active until its
event runs `evt_finish()`. `evt_exec` (command code) and `evt_fork` (event
code) start an event in a slot, and `task_kill` and `evt_disable` stop one.

### Statements

| Statement | Does | Compiles to |
|---|---|---|
| `evt_*`, `act_*`, `tw_*` | the event VM instruction of that name (see [Event instructions](opcodes/index.md#event-top-state)) | the instruction |
| any other `name(args);` | runs one command | `evt_single` with the command |
| `single(N) name(args);` | a command whose `evt_single` declares payload length N | `evt_single N` |
| `run { ... }` | runs a block of command code to its end | `evt_block` |
| `run noreturn { ... }` | the same, without the block's implicit `end` | `evt_block` |
| `PROC();` | calls a procedure | `evt_single` with `gosub` |
| `for (N) { ... }` | runs its body N times (N up to 65535) | `evt_for` ... `evt_fornext` |
| `do { ... } while (cond);` | runs its body, then again while one condition command passes | `evt_do` ... `evt_dountil` |
| `sleep(N);` | waits N frames (N up to 65535) | `evt_sleep` and its `evt_sleep_tick` byte |
| `wait_message();` | waits while a message window, voice line or menu choice is open | `evt_push_cond`, `evt_skip_if` |
| `if (...) { ... } else { ... }` | an event `if` | `evt_if`, `evt_jump` |
| `raw(0x.., ...);` | trailing bytes, as the last statement only | the bytes |

`return;` and `require(...)` are command flow and are only valid inside a
`run` block. `run`, `for`, `do`, `single`, `sleep`, `wait_message` and `raw`
are only valid in an event.

`single(N)` exists because the original data has `evt_single` payloads that
cut a command short. The decompiler writes it for those, and a script never
needs it otherwise.

```c
single(10) effect(FX_00, FX_ROW_00, FX_AT_WORLD, 13096, -1000, 0, 0);
```

A `do`/`while` loop needs `evt_next()` (or another yield) in its body, or it
runs every frame without letting the frame end:

```c
do
{
    evt_next();
}
while (ck(FG_ROOM, F_ROOM_J_PLAYER_ANIM_DONE, FALSE));
```

The event VM keeps `for`, `do` and `sleep` counters on a stack four deep
per slot, so nest them no deeper than that.

### Event `if`

An `if` in an event works around event instructions: `sleep`, `run` blocks,
actor and tween sequences. It compiles to the port's `evt_if` (opcode 0xF4),
with `evt_jump` (0xF5) over an `else`. The original's event VM has no `if`.

```c
event defer_camera_switch
{
    sleep(1);
    if (ck(FG_STATUS, F_STATUS_PLAYER_JILL, FALSE))
    {
        run
        {
            set(FG_STATUS, F_STATUS_CAMERA_DEFER, TRUE);
        }
    }
    else
    {
        set(FG_STATUS, F_STATUS_CAMERA_DEFER, TRUE);
    }
    evt_finish();
}
```

The two branches of an event `if` must leave the event in the same state:
both in an actor or tween sequence, or both out of one. The exception is an
`if` that tests only the [variant flags](#room-variants). Then each variant
goes down its own branch, and the code after the `if` may run in a different
state for each variant, as long as only the variant's own branch reaches
each instruction that needs that state.

### Actor and tween sequences

An event moves characters and models in two sub-states of the event VM:

- `evt_work_set(kind, index)` selects what the following instructions act
  on: `WK_PLAYER` (index 0), an enemy slot (`WK_ENEMY`), a room object
  (`WK_OBJ`) or an item model (`WK_AOT`).
- `evt_actor_begin()` enters the actor state. Its `act_*` instructions set
  animations, motions and entity flags. `act_reset()` or `act_end()` returns
  to the top state.
- `evt_tween_begin()` or `evt_tween_begin_alt()` enters the tween state. Its
  `tw_*` instructions set or step the selection's position and rotation.
  `tw_end()` returns to the top state.

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_anim_flags(ANIM_21, F_ROOM_J_PLAYER_ANIM_DONE, 0);
act_reset();
evt_nop();
do
{
    evt_next();
}
while (ck(FG_ROOM, F_ROOM_J_PLAYER_ANIM_DONE, FALSE));
```

```c
evt_work_set(WK_OBJ, OBJ_02_CLOCK);
evt_tween_begin_alt();
evt_next();
tw_nop();
tw_set_pos(-20, 0, 0);
for (100)
{
    tw_pos_add();
    evt_next();
}
tw_nop();
tw_end();
evt_nop();
```

Control instructions (`evt_next`, `sleep`, `for`, `do`, `if`) work in every
state. A command (`evt_single`, `run`) works only in the top state. In the
actor or tween state, `evt_finish()` is written `act_finish()` or
`tw_finish()`; it is the same instruction and ends the event.

### Padding

An event that ends in `evt_finish` is zero-padded to four bytes unless its
header says `nopad`. `raw(...)` as the last statement gives any other
trailing bytes the original carries:

```c
raw(0x48, 0x0B, 0x00, 0x04, 0xFF, 0x00);
```

## Room variants

One script covers every variant of a room, and an `if` tests the variant
where they differ. The engine keeps four flags in step with the variant
while the room runs:

| Flag | Set while |
|---|---|
| `F_STATUS_PLAYER_JILL` | the player is Jill (the original's own selector of her room files) |
| `F_SCENARIO_MANSION2` | the second mansion visit: the guardhouse sets it, and a door into the mansion (stage 1 or 2) leads to the room's second visit exactly while it is set |
| `F_STATUS_REGION_JPN` | the base pack is the Japanese release (port-only; the original never uses the bit) |
| `F_STATUS_REGION_EUR` | the base pack is a French or German release (port-only) |

The engine sets the player and release bits on every room entry. In a
mansion room it sets the visit flag from the visit it boots or loads into;
in normal play the flag already holds that value. Outside the mansion
`F_SCENARIO_MANSION2` stays as the scenario left it, so there it does not
test the visit.

The checked-in scripts test these flags with plain `ck` conditions:

```c
if (ck(FG_STATUS, F_STATUS_PLAYER_JILL, FALSE))
{
    voice_play(VOICE_PLAY, VO_004_00);
}
else
{
    voice_play(VOICE_PLAY, VO_00C_12);
}
if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, TRUE))
{
    set(FG_ROOM, F_ROOM_04, FALSE);
}
```

USA is neither release bit:

```c
if (ck(FG_STATUS, F_STATUS_REGION_EUR, FALSE))
{
    bgm_stop(BGM_SUB_2);
}
```

### Variant names

The exporter writes the tests by variant name instead, and the compiler
reads that form too:

| Name | Variant | Tests |
|---|---|---|
| `CHRIS`, `JILL` | the player | `F_STATUS_PLAYER_JILL` clear or set |
| `MANSION1`, `MANSION2` | the mansion visit | `F_SCENARIO_MANSION2` clear or set |
| `USA`, `JPN`, `EUR` | the release the base pack was converted from (its manifest's `region`) | both release bits clear, `F_STATUS_REGION_JPN` set, `F_STATUS_REGION_EUR` set |

A test is one or more names joined by `&&`, each with an optional `!`:

```c
if (JILL && MANSION2)
{
    ...
}
if (!JPN)
{
    ...
}
```

A variant test holds only variant names: it cannot mix with condition
commands, and a condition that never holds (`CHRIS && JILL`) is an error.
The runtime build compiles each test to the flag conditions above. `!USA`
compiles to the `else` of the USA test. A variant build folds the test away
and keeps only the branch that variant takes.

A section that only some RDTs carry takes `when`:

```c
proc main when (CHRIS && MANSION2)
{
    ...
}

event on_enter_jill when (JILL)
{
    ...
}
```

A variant build leaves such a section out, so its bytes match the RDT's. In
the runtime build, `when` on an event changes nothing (the build carries
every event), and on `proc init` or `proc main` it works like wrapping the
body in an `if`. A section flag that holds for only some variants takes
their test: `proc init noreturn (MANSION2)`.

The checked-in scripts use none of these forms; they test the flags.

### The original's quirks

The original's scripts have two quirks that the exporter keeps as `asm`.
The checked-in scripts write them in their clean form, which behaves the
same:

- an `if` whose skip lands one byte past its `endif`. The original runs the
  `endif`'s zero operand byte as `end`, so the block ends either way.
- an `endif` that closes no `if`. The block's `end` resets the branch stack
  that it unbalances, so it does nothing.

## Names

### Values

An argument names a value in one of three ways:

- **Engine names**, which the compiler knows for every game: the flag
  groups, the AOT types, the work kinds and the families in
  [Engine names](#engine-names).
- **Defines** from the game's include files and from the script itself.
- **Section names**: an event's name stands for its number and a
  procedure's name is only valid as a call.

The truth values are the engine's, not C's: `TRUE` is 0, `FALSE` is 1 and
`TOGGLE` is 2, as `ck` and `set` read their last argument. `ck(.., TRUE)`
passes when the bit is set.

| Names | Values |
|---|---|
| `FG_SCENARIO` `FG_COMMON` `FG_LOCK` `FG_ENEMY` `FG_ROOM` `FG_STATUS` `FG_RUN` `FG_ITEM` `FG_MAP` `FG_9` | flag groups 0 to 9, for `ck` and `set` |
| `SCE_NONE` `SCE_DOOR` `SCE_MESSAGE` `SCE_3` `SCE_ITEM` `SCE_5` `SCE_6` `SCE_USEITEM` `SCE_ITEMBOX` `SCE_EVENT` `SCE_SAVE` `SCE_B` `SCE_C` `SCE_DOCUMENT` `SCE_HIKIDASHI` `SCE_F` `SCE_TYPEWRITER` | AOT types 0 to 16 |
| `WK_PLAYER` `WK_ENEMY` `WK_OBJ` `WK_AOT` | `evt_work_set` kinds 0 to 3 |
| `UNLOCKED` `LOCK` `LOCKED` | a door's key byte: 0, 254, 255 (any other value is the item that opens it) |
| `TRUE` `FALSE` `TOGGLE` | `ck` / `set` states 0, 1, 2 |

The flag group, AOT type, work kind and key names are valid only in the
argument that takes them, such as `ck`'s first argument or `aot_set`'s type,
and in the commands [written by field](opcodes/index.md#commands-written-by-field). Every
other name in this section is an ordinary value that works in any argument.

### Engine names

The compiler also names these engine values:

| Names | For |
| --- | --- |
| `CMP_EQ` `CMP_GT` `CMP_GE` `CMP_LT` `CMP_LE` `CMP_NE` | `cmpb` / `cmpw` comparisons, the state on the left |
| `B_ROOM` `B_CAMERA` `B_PREV_ROOM` `B_CUT` `B_MENU_CHOICE` `B_SELECTED_ITEM` `B_TOTAL_HELD` `B_LIGHT_R` `B_CHARACTER_MODEL` `B_FWD_AOT` `B_ENT_AOT` `B_USED_ITEM` `B_PICKED_ITEM` `B_SAVES` `B_EQUIPPED` `B_CHARACTER` `B_HEALTH_STATUS` | `cmpb` / `setb` state bytes |
| `W_LIGHT_STATE` `W_LIGHT_DELTA` `W_RANDOM` `W_DPAD_HELD` `W_DPAD_PRESSED` | `cmpw` / `setw` state words |
| `SAT_PLAYER` `SAT_OBJECT` `SAT_UNDER` `SAT_MANUAL` | AOT trigger flags: the player or an object, at its feet rather than its reach, on the action button |
| `MSG_FAST` | `message`: show the text at once |
| `RUN_PLAYER` `RUN_ENTITIES` `RUN_MONSTERS` `RUN_EFFECTS` `RUN_READY` `RUN_CONTROLS` `RUN_DEMO` | the systems that run each frame (`FG_RUN`); `message`'s second argument stops these while the window is open |
| `VOICE_PLAY` `VOICE_STOP` | `voice_play` |
| `SND_BANK_DOOR` `SND_BANK_WEAPON` `SND_BANK_ROOM` `SND_BANK_PLAYER` `SND_BANK_BGM` | `se_play_3d`: the sound bank |
| `SND_AT_POINT` `SND_AT_PLAYER` `SND_AT_ENEMY` `SND_NO_POS` | `se_play_3d`: where the sound plays |
| `BGM_MAIN` `BGM_SUB_1` `BGM_SUB_2` | the music channel of `bgm_play`, `bgm_stop`, `bgm_volume_ramp` and `snd_pan_vol_set` |
| `BGM_MAIN_ON` `BGM_SUB_1_ON` `BGM_SUB_2_ON` `BGM_RELOAD` `BGM_RESTART` `BGM_OFF` | `room_bgm_state_set`: the channels to start, the load type, no music; the low three bits pick the room's music group |
| `DOOR_LOCKED` `DOOR_LOCK_40` | `door_aot_set`'s lock byte, ORed with the `FG_LOCK` bit |
| `EML_BEHAVIOUR` `EML_REVIVE` `EML_ACTION` `EML_STATUS` `EML_ANGLE` `EML_BLEND_RESET` `EML_JOINTS` `EML_ACTION_STATE` | `eml_state` sub-commands |
| `EML_STATUS_SET` `EML_STATUS_OR` `EML_STATUS_XOR` | `eml_state(.., EML_STATUS, EML_STATUS_OR \| 2)`: how the bits apply |
| `SYS_UNEQUIP` `SYS_HIT` `SYS_PLAYER_FLAGS` `SYS_FORCE_ACTION` `SYS_ANGLE` `SYS_CLEAR_8C` `SYS_IDLE` `SYS_HEALTH_STATUS` `SYS_JOINTS` `SYS_E0_40` | `sys_multi` sub-commands, all on the player |
| `SYS_SET` `SYS_OR` `SYS_XOR`, `PLF_xx`, `HS_LOCKED` `HS_xx` | `sys_multi(SYS_PLAYER_FLAGS, SYS_OR \| PLF_04)` and `SYS_HEALTH_STATUS`: how the player or health-status bits apply, and which |
| `SYS_ON` `SYS_OFF`, `HIT_xx`, `JOINT_xxxx` | `sys_multi`'s `SYS_E0_40` switch, `SYS_HIT` kind and `SYS_JOINTS` mask (also `eml_state(.., EML_JOINTS, ..)`) |
| `OBJ_PUSH_GRUNT` `OBJ_TEX_QUEUE` | `obj`'s slot byte, ORed with the object |
| `OF_ACTIVE` `OF_INTANGIBLE` `OF_NO_FLOOR_PROBE` `OF_NO_COLLISION` `OF_NOT_PUSHABLE` `OF_CLIMBABLE` `OF_xx` | object and item model flags (`obj`, `objtbl_b_set`) |
| `SCA_NONE` `SCA_PLAYER` `SCA_ENEMY` | what an object or item model is attached to: nothing, the player, `SCA_ENEMY \| ENEMY_xx`, or an object (`OBJ_xx`) |
| `ITEM_ALT_ROT`, `ITEM_SPARKLE` `ITEM_FX_1`..`ITEM_FX_F` `ITEM_REACH` | `item_aot_set`: the AOT byte's alternate rotation, and the flags word (the pick-up sparkle, its effect, the reach animation; the sparkle's height stays a number) |
| `NO_FLAG`, `SPAWN_RESTORE` `SPAWN_FORCE`, `EB_xx`, `EVAR_xx` | `enemy`: no guard flag, whether a saved enemy is restored, the behaviour byte (also `eml_state(.., EML_BEHAVIOUR, ..)`) and the model variant |
| `ANIM_xx`, `AB_xx` `AB_ADDITIVE`, `PLW_xx` | an animation clip, an action behaviour (`act_anim_seq`, `act_anim_set`, `eml_state(.., EML_ACTION, ..)`), `plw_anim`'s action state |
| `MOTION_TARGET` `MOTION_xx`, `LOOK_AT_PLAYER` `LOOK_AT_ENEMY` `LOOK_AT_OBJ` `LOOK_AT_ITEM` | `act_motion`'s mode; `MOTION_TARGET` follows a live target instead of a position |
| `EF_OR` `EF_SET` `EF_XOR`, `EF_xxxx`, `AFL_xx` | `act_flag_op`'s mode and entity flags; `act_anim_flags`' flags (shifted down two bits) |
| `TW_STATE` `TW_IGNORE` `TW_BEHAVIOUR` `TW_ACTION_STATE` `TW_HEALTH_LO` `TW_HEALTH_HI` `TW_HIT_STATE`, `TW_FIELD_xx` | `tw_set_field`: the state byte to write |
| `TW_POS_X` `TW_POS_Y` `TW_POS_Z` `TW_HEALTH` `TW_TARGET_X` `TW_TARGET_Z`, `TW_SEL_xx` | `tw_set_sel`: the value to write |
| `FX_ROW_xx`, `FX_AT_WORLD` `FX_AT_PLAYER` `FX_AT_OBJ` | an effect's animation row (`effect`, `effect_kill_b`) and what it is attached to (`FX_AT_OBJ \| OBJ_xx`, or `FX_AT_ENEMY_xx`) |
| `LIGHT_FADE_VEIL` `LIGHT_FADE_HALF` `LIGHT_FADE_xx`, `FADE_R` `FADE_G` `FADE_B` | `timer_setup`: how the special room light draws, and its channels |
| `LIGHT_KIND_xx` | a light's kind (`msg_list`) |
| `COL_LIST_xx`, `CF_x` | `inst_cfg`: the collision list and the flags it sets |
| `TBL_OBJ` `TBL_ITEM`, `ROT_OBJ` | `objtbl_b_set`'s table; `eml_rot(ROT_OBJ \| OBJ_xx, ..)` rotates an object, otherwise an item model |
| `DIST_ENEMY` `DIST_OBJ` `DIST_ITEM` | `ck_counter`: what the distance is measured to |
| `SPR_ON` `SPR_OFF`, `IGRP_ANY` `IGRP_xx`, `PAD_xxxx` | `aot_switch`, `ck_item_count`'s item group, `ck_bits`' d-pad buttons |
| `XFER_SAVE` `XFER_RESTORE` `XFER_HELD`, `MASK_OR` `MASK_CLEAR` `MASK_XOR` `FXF_xxxx` | `swap_var`'s direction; `mass_mask`'s mode and effect flags |
| `ENT_PLAYER` `ENT_xx`, `CUT_FREE` `CUT_LOCKED`, `MODEL_OP_xx`, `COSTUME_xx` | `spd_set`'s entity, `cut_auto`, `model_op`'s form, the costume |
| `TASK_xx` | the event slot of `evt_exec`, `evt_fork`, `evt_disable` and `task_kill` |

Values whose meaning is not known yet have placeholder names, so a script
never needs a bare number for one and the name can be given its meaning
later. These are every state byte and word (`B_0A`, `W_04`), `eml_state`
sub-command (`EML_08`), `sys_multi` sub-command (`SYS_02`), event slot
(`TASK_09`), every unnamed bit of the trigger flags (`SAT_02`) and run word
(`RUN_0010`), and the `xx` forms in the table. A placeholder is the family
prefix and the value in hex: two digits for a byte, or one bit of the field
written at the field's width (`PAD_0001`, `RUN_0010`). It compiles to that
value.

### Include files

The game's data has names only in its include files. `games/re1/inc/` holds
seven, and every checked-in script includes all of them:

| File | Defines |
|---|---|
| `doors.akinc` | `DOOR_SND_xx_*` door sound pairs and `DOOR_xx_*` door animations, for `door_aot_set` |
| `enemies.akinc` | `EM_xx_*` enemy and character types, for `enemy` |
| `flags.akinc` | `F_<group>_*` bits of each flag group: `ck(FG_COMMON, F_COMMON_INTRO_SEEN, TRUE)` |
| `items.akinc` | `IT_xx_*` item ids, for items, keys and item checks |
| `messages.akinc` | `MSG_*` global messages (id `0x40` and up), for `message` |
| `movies.akinc` | `MOVIE_xx_*` films, for `movie_on` |
| `rooms.akinc` | `STAGE_n` stage bits and `ROOM_<stage><room>_*` rooms, for door targets and `B_ROOM` tests |

Their conventions:

- An id that never changes is in the name: `IT_2F_KEY_INK_RIBBON` is item
  `0x2F`, `EM_11_ZOMBIE_GREEN` enemy type `0x11`.
- `F_<group>_<bit>` (`F_COMMON_A3`) is a flag bit not yet named. Named bits
  say what they mean when set. `F_ITEM_<room>_<item>_<model>` is a pickup,
  set while it is still there. `F_ENEMY_<room>_<type>_<slot>` is an enemy,
  set once it is dead. `F_LOCK_<room>_<room>` is the door between two rooms,
  set once it is unlocked. `_J_` or `_C_` after the group marks a bit that
  means this only in Jill's or Chris's game.
- A door names a room of its own stage alone (`ROOM_106_MAIN_HALL`) and a
  room of another stage with its stage bits (`STAGE_2 | ROOM_201_..`).

A room's own names are `#define`s at the top of its script: its AOT
indexes, `FG_ROOM` bits, room messages, enemy slots, voice lines, cameras,
objects, item models and sounds (see [The exporter](#the-exporter)).

### Message ids

A message id is a word. Its low byte is the original's id: bit `0x80` asks
for the fast reveal (`MSG_FAST`), bit `0x40` picks the global table, and the
low six bits are the index. Its high byte carries the index's higher bits,
so a room can have more than the original's 64 messages: room message 70 is
`0x106`. `message` takes the word and compiles to the port's long form
(`message_long`, opcode 0x53) when it does not fit a byte. A global message
has no index past 63. An examine spot's message (`aot_set` with
`SCE_MESSAGE`) carries the id in its data word, the high byte after the low.

## The instruction set

Every instruction has a page of its own, with its parameters, what it does
and an example: see [RE 1 opcodes](opcodes/index.md), which lists them by
purpose.

## The exporter

`arklay scd export ROOM1000.RDT --out 1000.akscr` decompiles a room's
scripts to structured code, and `arklay scd build 1000.akscr --out
room/100/room.scd` compiles it back. Every room of the shipped game
decompiles to a script that compiles to byte-identical init, main and event
streams. Each decompiled section is compiled again and compared with the
original bytes, and a section that does not match comes out as one `asm`
block, so an export always compiles back exactly. A container that Arklay
built (`scd export room.scd --out room.akscr`) decompiles the same way, with
the port's own forms: its procedures as `proc proc_00`, `proc proc_01`, and
so on, their `gosub`s as calls (`proc_00();`), and its `if_long` and
`evt_if` as `if` statements. `scd export` of a single RDT names only engine
values: game data stays as numbers.

`arklay scd export-rooms usa=<install> jpn=<install> eur=<install> --out
games/re1/room` decompiles each installation's RDTs and writes one script
per room as `<room>/room.akscr`. A single install exports one region, USA
unless named. It lines up each variant's sections and lines by content and
writes the differences as [variant tests](#variant-names). An event that
the variants number differently gets one name (a variant's own, or one
tagged with its variants, such as `on_enter_jill`), and every reference to
it follows. An export builds back to each RDT's bytecode exactly; what has
no structured form stays `asm`.

The exporter names what it can from the `*.akinc` files in the nearest
`inc/` directory at or above `--out`, and from the English text beside it
(`language/en-US.txt`):

- items (`IT_0B_AMMO_HANDGUN`), enemies (`EM_11_ZOMBIE_GREEN`), flags
  (`F_SCENARIO_0A`, a bit of `FG_SCENARIO`), door sounds and animations
  (`DOOR_SND_01_DR_MTL`, `DOOR_16_KAI01`) and films
  (`MOVIE_03_FIRST_ZOMBIE`), by the define's prefix. `door_aot_set`'s lock is
  `DOOR_LOCKED | F_LOCK_103_104`, the `FG_LOCK` bit that unlocks it. A door
  target is a room of the same stage (`ROOM_106_MAIN_HALL`) or
  `STAGE_2 | ROOM_201_..` for another.
- each AOT index the room sets up, as a local `#define AOT_05_ITEM 0x05`
  named after its first type that is not `SCE_NONE`, and each `FG_ROOM` bit
  as `F_ROOM_xx`.
- events after what starts them: `on_aot_05_item` for an AOT's event,
  `on_enter` from `proc init`, `from_main` from `proc main`,
  `<parent>_sub1` for an event another starts, with `_2` and so on for
  repeats. An event nothing refers to stays `event_XX`.
- message ids from their English text: the first sentence, at up to six
  words (`MSG_IT_IS_LOCKED`), adding the next sentence and then the id where
  two would share a name. The global messages (id `0x40` and up) are in
  `inc/messages.akinc`, and a room's own are defined in its script.
  `MSG_.. | MSG_FAST` asks for the fast reveal, and the full text follows as
  a comment. A message without text that follows a voice line is named after
  the line (`MSG_VO_104_02`).
- each enemy slot the room fills, `#define ENEMY_02_CROW 0x02` after the
  type `enemy` puts there, for `evt_work_set(WK_ENEMY, ..)`, `eml_state` and
  the other commands on a slot.
- each voice line the room plays, `#define VO_104_02 0x09` after the line's
  file.
- the room's objects (`OBJ_02`), cameras (`CAM_03`, for `cutnext` and the
  `B_CAMERA`/`B_CUT` state bytes), effects (`FX_25`), item models
  (`IMDL_01`, for `item_aot_set`, `eml_rot`, `objtbl_b_set` and
  `evt_work_set(WK_AOT, ..)`), collision records (`COL_08`), camera zones
  (`ZONE_0D`), sprites (`SPR_02`) and effect parents on an enemy slot
  (`FX_AT_ENEMY_01`), as local placeholder defines. An AOT or enemy slot the
  room never sets up is `AOT_05` or `ENEMY_04`.
- the room's sounds after the bank `se_play_3d` plays from: `SE_17_SPRAY`
  from the room's sound table (`SE_17` where the room's variants play
  different files), and `SE_DOOR_00`, `SE_WEAPON_xx`, `SE_PL_xx` and
  `SE_BGM_xx` for the banks that depend on the door, weapon or character.
- engine values with the compiler's own [engine names](#engine-names).

Every id, enum and flag argument gets a name, even one whose meaning is not
known yet (`B_0A`, `TASK_09`, `OBJ_02`), so that it can be renamed once it
is. Only plain quantities stay numbers: positions, angles, counts, frames
and volumes. So do a few values whose meaning is still unknown (filed as
issues), padding bytes, and a zero for "no flags".

These names are a starting point for renaming by hand. Exporting again
overwrites hand edits, so the checked-in scripts are no longer re-exported.

## The `.s` assembler

`.s` is the lossless disassembly of a room's scripts, one instruction per
line in the original tool's format. `arklay scd export ROOM1000.RDT --out
1000.s` writes it, and `arklay scd build 1000.s --out room/100/room.scd`
assembles it back into a standalone container. Both `room/{id}/room.scd`
and the engine read that container through the RDT pointer-table ABI.
`--list` also writes a `.lst` listing with each instruction's offset and
bytes.

```text
.version 1

.init
.block
    evt_exec                0, 0, event_0B

.main

.event event_00
    evt_work_set            WK_PLAYER, 1
    evt_finish
```

The assembler accepts exactly the disassembler's grammar:

- `.version 1`, then the `.init` and `.main` sections, each holding its
  blocks, every block starting with `.block`;
- a `.procs` section after `.main` and before the events, which holds the
  user procedures as `.block`s: block N is what `gosub N` runs;
- `.event event_XX` sections, dense and ascending from `event_00`;
- `off_XXXX:` label definitions and `off_XXXX + delta` references for the
  jumps;
- `db 0x.., ...` lines for trailing bytes;
- every mnemonic the disassembler prints, with its named operand constants,
  the bit-packed `hi | lo` actor forms and the variable-width commands.

Unencodable input fails with the line, column and mnemonic. The assembler
never falls back to raw bytes.

## Errors and common mistakes

The compiler stops at the first error and names its line and column. The
messages say what was expected. The ones that most often need explaining:

| Message | Cause |
|---|---|
| `the script must start with #version 1` | The first line that is not a comment or directive is not `#version 1`. |
| `expected a name, found #` | A directive the RE 1 preprocessor does not know, such as an RE 2 script's `#game re2`, standing between sections. |
| `unknown name NAME (is an #include missing?)` | The name is not defined, not an engine name and not a section. Game data (items, flags, rooms) needs the include files. |
| `unknown constant NAME` / `NAME is not a value CMD takes` | A flag group, AOT type, work kind, key or `event_XX` name in an argument that does not take it. |
| `CMD takes N arguments, found M` | A command written by field got the wrong number of arguments. |
| `argument N (V) does not fit a byte` | A value out of its field's range. A byte field takes -128 to 255, a word field -32768 to 65535. |
| `CMD is not a condition` / `unknown condition CMD` | A command that is not a [condition](opcodes/index.md#conditions) in an `if` head, a `require` or a `do`/`while` test. |
| `write end with the if/else/return statements` | A flow opcode called by name. Use the statement form. |
| `call a procedure by its name` | `gosub(N)` written directly. Write `proc NAME { }` and `NAME();`. |
| `this statement is only valid in an event script` | `run`, `for`, `do`, `single`, `sleep`, `wait_message` or `raw` in command code. |
| `command flow is only valid inside a run block` | `return;` or `require(...)` directly in an event. |
| `raw must be the last statement` | `raw(...)` with statements after it. |
| `procedure P calls itself (P -> Q -> P)` | Recursion, which the command VM cannot run. |
| `a procedure takes no noreturn or when` | Test the variant inside the procedure instead. |
| `proc init must come before proc main and the events` | Sections out of order. |
| `expected event event_XX, events must be dense and ascending from event_00` | An event named `event_XX` stands at the wrong position (variant builds only). |
| `NAME is already defined as V` / `event name NAME is already defined` | Two defines, events or procedures with the same name. |
| `#if JILL picks a room variant, which is a run-time test` | A variant written as a preprocessor test. Write `if (JILL) { ... }`. |
| `expected a variant (one of CHRIS, ...); a variant test cannot mix with other conditions` | A variant name and a condition command in one `if` head. Nest one `if` in the other. |
| `message id V does not fit 16 bits` / `global message V has no index past 63` | A message id out of range. |
| `include path P must be relative and stay below the script` | An absolute path or `..` in an `#include`. |
| `act_motion_path with a zero motion nibble would decode as the 2-byte act_motion` | The mode's low four bits pick the form: 0 is the short `act_motion(mode)`, anything else the long `act_motion_path`. |

Mistakes that compile but do not behave as meant:

- Treating `TRUE` as 1. `TRUE` is 0 and `FALSE` is 1. Write the names, not
  numbers.
- A `do`/`while` or `for` loop without `evt_next()` in its body. The event
  spins through the loop in one frame instead of waiting.
- A `return;` deep inside an `if` in `proc init`. It ends the whole block,
  not just the `if`.
- Testing `F_SCENARIO_MANSION2` outside the mansion. There it is whatever
  the scenario left, not the visit.
- More than 16 nested `if`s in command code, or more than four nested
  `for`, `do` and `sleep` in an event. The VMs' stacks are that deep, and
  the original behaves the same way.
- A mod that ships only part of a room's script. The override replaces all
  of the room's init, main and event scripts, so the doors and events it
  leaves out are gone.
