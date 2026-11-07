# `plc_cnt`

Sets the motion counter (`Move_cnt`) of the task's character or the player.

| | |
| --- | --- |
| Opcode | `0x5B` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
plc_cnt(count);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `count` | `u8` | The new `Move_cnt`: the frame the work's clip plays from. |

## Behaviour

Sets `Move_cnt` at once. [`plc_motion`](plc_motion.md) starts its clip with
it at 0; `plc_cnt` after it starts the clip further in. What the routine
does with the counter afterwards (counting frames, ending the clip) is the
routine's.

It applies to the player and to a character. On a room object or a work the
engine does not model it does nothing, and the log notes
`RE2 script: plc_cnt is not modelled` once per run.

## Example

```c
proc main_17
{
    work_set(3, 0);
    plc_motion(3, 4, 0);
    plc_cnt(20);                 // from frame 20
    evt_end(0);
}
```

Illustrative: a motion that starts twenty frames into its clip. The routine
and clip numbers are placeholders.

## See also

- [`plc_motion`](plc_motion.md): starts the clip.
