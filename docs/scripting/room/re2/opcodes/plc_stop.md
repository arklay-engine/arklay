# `plc_stop`

Holds the motion of the task's character or the player, or releases a hold.

| | |
| --- | --- |
| Opcode | `0x66` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
plc_stop();
```

## Parameters

None.

## Behaviour

A toggle, as the original's `Plc_stop`. If the work is not held, its routine
is saved and replaced by the hold, `4, 20, 0, 0`, which keeps the body where
it is. If it is held, the saved routine comes back and carries on from where
it was. Use it in pairs.

On the player, a hold on the room's first tick of play, before the player's
first motion has run, also sets the animation blend (`Hokan`) to 0, as the
original's does when a scene starts on entry (room 200).

It applies to the player and to a character. On a room object or a work the
engine does not model it does nothing, and the log notes
`RE2 script: plc_stop is not modelled` once per run.

## Example

```c
proc main_18
{
    work_set(1, 0);
    plc_stop();                  // hold the player
    message_on(0, 6, 0, 0, 0);
    wsleep();
    wsleeping();
    plc_stop();                  // and let go
    evt_end(0);
}
```

Illustrative: the player stands still while a message shows.

## See also

- [`plc_ret`](plc_ret.md): ends a scripted motion.
- [`plc_motion`](plc_motion.md): starts one.
