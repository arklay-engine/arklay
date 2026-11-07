# `xa_vol`

Sets the volume of the next voice line.

| | |
| --- | --- |
| Opcode | `0x5F` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
xa_vol(level);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `level` | `u8` | The volume, `0` to `127`. A value from 128 up is ignored. |

## Behaviour

`xa_vol` keeps the level for the next [`xa_on`](xa_on.md): a line takes the
level when it starts, so a line already playing is not changed. The level is
120 until a script changes it, and every room's load sets it back to 120.

The level maps to an attenuation as the original's `Snd_xa_vol` does: two
straight lines meeting at 32, from −100 dB at 0 to full volume near 127. The
voice channel does not follow the game's master volume.

## Example

```c
xa_vol(90);
xa_on(1, 0x06);
wsleep();
wsleeping();
```

Illustrative: a quieter line from the common set.

## See also

- [`xa_on`](xa_on.md): plays the line.
