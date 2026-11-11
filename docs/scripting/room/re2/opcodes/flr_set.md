# `flr_set`

Turns one of the room's floor (footstep) zones on or off.

| | |
| --- | --- |
| Opcode | `0x38` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
flr_set(index, on);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `index` | `u8` | The zone, an index into the room's floor table. |
| `on` | `u8` | 1 turns the zone on; any other value turns it off. |

## Behaviour

The original's handler (`0x4E8BE0`) changes bit `0x80` of the zone's flags
word, the bit that switches it off for `Floor_check`, which chooses the sound
of footsteps. `on` 1 clears the bit and anything else sets it. An index past
the room's zones changes nothing. The port applies the change after the
scripts' pass, as it does `sca_id_set`'s, and a room's load starts from the
zones its data holds.

Room 109's event 15 and 16 turn on zone 1, a puddle whose footsteps play the
room's sound `0x1a`, and Leon A's route through 109 hears it.

## Example

```c
proc main_10
{
    flr_set(1, 1);
    evt_end(0);
}
```

## See also

- [`sca_id_set`](sca_id_set.md): changes a collision record.
