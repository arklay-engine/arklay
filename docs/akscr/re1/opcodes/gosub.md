# `gosub`

Runs one of the script's procedures and comes back.

| | |
| --- | --- |
| Opcode | `0x54` |
| Group | Flow |
| Kind | Flow |
| Size | 2 bytes |

## Syntax

```c
NAME();
```

`gosub` is written as a call of a `proc NAME` section. Calling `gosub(...)`
by name is an error.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| procedure | `u8` | Written by the compiler: the procedure's number, in the order the `proc NAME` sections stand in the script (0 to 255). |

## Behaviour

The VM runs procedure N, block N of the script's procedure container, to its
end and then goes on with the instruction after the call. The procedure
starts with no open `if` of its own: it shares the caller's branch stack but
never unwinds below the caller's depth. So its `return;`, a failed condition
at its top level, and its implicit final [`end`](end.md) all return to the
caller. They do not end the caller's block, and the caller's open `if`s are
as they were.

A call may stand in `proc init`, `proc main`, another procedure, or an
event (in a `run { }` block or as a statement of its own). Calls nest up to
16 deep; a deeper call is skipped. The compiler rejects recursion, direct or
through other procedures, so only hand-assembled bytecode reaches that
limit. A call to a procedure number that does not exist does nothing. In a
command block, a procedure that runs into bytes that are not commands stops
the caller's block list too.

The opcode and the procedure container are the port's own; the original's
RDTs have no room for them. A script with procedures runs only in Arklay.

## Example

```c
proc main
{
    if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, FALSE))
    {
        kenneth_zombie_checks();
        if (ck(FG_STATUS, F_STATUS_PLAYER_JILL, FALSE))
        {
            piano_music_volume();
        }
    }
}
```

Room 104 (`games/re1/room/104/room.akscr`): the main block calls two
procedures, the second only in Chris's game.

## See also

- [`end`](end.md): `return;`, which leaves a procedure.
- [Procedures](../index.md#procedures) in the language reference.
