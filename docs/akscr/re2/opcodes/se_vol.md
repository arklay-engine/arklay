# `se_vol`

Not modelled: a sound-effect volume setting.

| | |
| --- | --- |
| Opcode | `0x80` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 2 bytes |
| Status | no |

## Syntax

```c
se_vol(value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `value` | `u8` | Its meaning is not known to the port; by the name, a volume. |

## Behaviour

The port does not model `se_vol`. It does nothing, and the first one a run
executes logs `RE2 script: se_vol is not modelled`. The name, from
biohazard-utils, suggests the original sets the volume of the sound effects;
the port plays them at their banks' own volumes.

## Example

```c
se_vol(100);
```

Illustrative: it compiles and round-trips, and has no effect in the port.

## See also

- [`se_on`](se_on.md): plays a sound effect.
- [`xa_vol`](xa_vol.md): the voice volume, which the port does model.
