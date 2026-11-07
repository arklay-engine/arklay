# `plc_motion`

Starts a scripted motion on the task's character or the player.

| | |
| --- | --- |
| Opcode | `0x3F` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
plc_motion(routine, clip, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `routine` | `u8` | The scenario routine to run (`Routine_1`): which of the original's `Spl_sce*` / `Pl_sce*` routines plays the motion. |
| `clip` | `u8` | The clip (`Move_no`) the routine starts with. |
| `flags` | `u8` | The new scene flags, `Sce_flg` (member 22), which the routine reads. |

## Behaviour

On the work chosen with [`work_set`](work_set.md), as the original's
`Plc_motion`:

- `Routine_0` to `Routine_3` become `4` (the scenario mode), `routine`, 0, 0,
- `Move_no` becomes `clip` and `Move_cnt` (the frame, see
  [`plc_cnt`](plc_cnt.md)) 0,
- `Sce_flg` becomes `flags`, and `Sce_free0` and `Sce_free1` 0.

From the next tick the work's motion code runs that routine instead of the
pad's (for the player) or standing (for a character), until a
[`plc_ret`](plc_ret.md) or another `plc_*` changes it. What a routine does
with the clip and the flags is the routine's: the port follows the original's
routines for the player and for the characters it models.

It applies to the player and to a character (kinds 1 to 3). On a room object
or a work the engine does not model it does nothing, and the log notes
`RE2 script: plc_motion is not modelled` once per run.

## Example

```c
proc main_11
{
    work_set(3, 0);
    plc_motion(3, 2, 0);
    sleep(10, 45);
    plc_ret();
    evt_end(0);
}
```

Illustrative: the character in enemy slot 0 plays a scripted motion for 45
ticks and is then handed back. The routine and clip numbers are placeholders:
take real ones from a room's own script.

## See also

- [`plc_dest`](plc_dest.md): a scripted walk to a point.
- [`plc_ret`](plc_ret.md): ends the scripted motion.
- [`plc_flg`](plc_flg.md), [`plc_rot`](plc_rot.md), [`plc_cnt`](plc_cnt.md): change the motion's state.
- [`plc_stop`](plc_stop.md): holds the motion.
