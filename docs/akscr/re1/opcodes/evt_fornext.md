# `evt_fornext`

Ends a `for` loop's body: counts down and goes back to the body until the count runs out.

| | |
| --- | --- |
| Opcode | `0xFB` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
for (N)
{
    ...
}
```

The closing `}` of the `for`.

## Parameters

None.

## Behaviour

Decrements the counter on top of the slot's loop stack. When it reaches
zero it pops the entry and moves on; otherwise it jumps back to the start
of the body that [`evt_for`](evt_for.md) recorded. It does not yield in
either case. With an empty stack it moves on.

It uses whatever entry is on top of the stack, so a body must leave the
stack as it found it: every `sleep`, `do` and `wait_message` in it
complete. The compiler's `for` always does.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_FALLING_FRAME);
evt_tween_begin_alt();
tw_set_rot(100, 0, 0);
for (2)
{
    tw_rot_add();
    evt_next();
}
```

Room 117 (`games/re1/room/117/room.akscr`), event `frame_falls`: the
picture frame turns for two frames; the `}` is the `evt_fornext`.

## See also

- [`evt_for`](evt_for.md): the start of the loop.
