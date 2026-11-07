# `plc_neck`

Turns the head of the task's character or the player: at a point, at someone, by an angle, or back to the pose.

| | |
| --- | --- |
| Opcode | `0x41` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 10 bytes |
| Status | yes |

## Syntax

```c
plc_neck(mode, x, y, z, yaw_rate, pitch_rate);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | What the head does; see below. |
| `x` | `i16` | Mode 1: the x of the point to look at. Mode 5: whom to look at. Modes 3 and 4: how many times to sweep. |
| `y` | `i16` | Mode 1: the point's y. Mode 2: the yaw from the body's facing. Mode 5: the enemy slot, when `x` is neither 1 nor 2. |
| `z` | `i16` | Mode 1: the point's z. Mode 2: the pitch. |
| `yaw_rate` | `u8` | How fast the head turns sideways, in angle units a tick. |
| `pitch_rate` | `u8` | How fast it turns up and down. |

## Behaviour

Writes the work's neck state (`Neck_flg`, the three words and the two rates)
as the original's `Plc_neck` does, and the head turns towards the result over
the next ticks, at most `yaw_rate` and `pitch_rate` a tick and never past the
neck's limits. The modes, with the `Neck_flg` each one sets:

| Mode | `Neck_flg` | The head |
| --- | --- | --- |
| 0 | `0x92` | follows the pose, at the script's rates |
| 1 | `0x84` | looks at the point `x, y, z` |
| 2 | `0x88` | turns `y` from the body's facing sideways and `z` up or down |
| 3 | `0xAA` | sweeps up and down `x` times, then stops |
| 4 | `0xD8` | sweeps from side to side `x` times, then stops |
| 5 | `0x84` | looks at someone's head: `x` 1 the player, 2 the partner, otherwise enemy slot `y` |
| 6 | `0` | is let go: it follows the pose at the default rates |
| other | `0x80` | the original's bare flag |

Mode 5 aimed at someone the port does not model (a zombie, for instance)
leaves the head where it was.

It applies to the player and to a character. On a room object or a work the
engine does not model it does nothing, and the log notes
`RE2 script: plc_neck is not modelled` once per run.

## Example

```c
proc main_13
{
    work_set(3, 0);
    plc_neck(5, 1, 0, 0, 24, 16);   // look at the player
    sleep(10, 60);
    plc_neck(6, 0, 0, 0, 0, 0);     // let the head go
    evt_end(0);
}
```

Illustrative: the character in enemy slot 0 looks at the player for two
seconds.

## See also

- [`plc_motion`](plc_motion.md), [`plc_dest`](plc_dest.md): move the body.
- [`plc_ret`](plc_ret.md): ends the scripted motion, which also clears the neck's flags.
