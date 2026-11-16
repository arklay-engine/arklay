# `plc_ret`

Ends the scripted motion of the task's character or the player and hands it back.

| | |
| --- | --- |
| Opcode | `0x42` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
plc_ret();
```

## Parameters

None.

## Behaviour

As the original's `Plc_ret`: the routine becomes `1, 0, 0, 0` (the move mode,
standing) and `Neck_flg` 0, so the head follows the pose again.

- **The player**: is back under the pad from the next tick. `plc_ret` does
  not clear `fg_stop`: a script that held the input with `set(2, 7, 1)` must
  also release it with `set(2, 7, 0)`.
- **A character**: leaves the script's routine, and its `Type` loses `0x4000`
  and `0x1000` (a player id's high byte keeps only its low nibble), an
  enemy's as well as a scene character's. The routine is what sends Sherry or
  Ada to `Spl_Auto` to follow the player, as at the end of Sherry's scene in
  room 21B; the port has no `Spl_Auto` for anyone else yet, so another
  character handed back stands still.
- **A room object or a work the engine does not model**: nothing happens, and
  the log notes `RE2 script: plc_ret is not modelled` once per run.

## Example

```c
proc main_14
{
    work_set(1, 0);
    plc_dest(0, 4, 41, 2000, -3500);
    do
    {
        evt_next();
    }
    while (ck(5, 41, 0));
    plc_ret();
    set(2, 7, 0);                // give the input back
    evt_end(0);
}
```

Illustrative: the end of a scene that walks the player to a point and then
hands them back to the pad.

## See also

- [`plc_motion`](plc_motion.md), [`plc_dest`](plc_dest.md): start a scripted motion.
- [`plc_stop`](plc_stop.md): holds the motion without ending it.
- [`set`](set.md): `set(2, 7, ...)` holds and releases the player's input.
