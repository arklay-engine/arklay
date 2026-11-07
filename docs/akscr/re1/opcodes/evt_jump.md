# `evt_jump`

Jumps forward over an event `else` body; written as `else`.

| | |
| --- | --- |
| Opcode | `0xF5` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 3 bytes |
| Valid in | every state |

## Syntax

```c
if (cond)
{
    ...
}
else
{
    ...
}
```

## Parameters

None written in `.akscr`. The compiler fills in the jump:

| Parameter | Width | Description |
| --- | --- | --- |
| `target` | `u16` | Bytes from the opcode to the instruction after the `else` body. |

## Behaviour

The port's own instruction, emitted at the end of the `then` body of an
event `if` that has an `else`. It goes on at `target`, always, in the same
frame. It does not yield or touch the loop stack. The original's event VM
does not have it.

## Example

```c
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
```

Room 104 (`games/re1/room/104/room.akscr`), event `defer_camera_switch`:
the two characters' versions of the original write the same flag in two
shapes; in the build for every variant the `evt_jump` ends the first branch.

## See also

- [`evt_if`](evt_if.md): the `if` it belongs to.
- [`else_long`](else_long.md): the command code's long `else`.
