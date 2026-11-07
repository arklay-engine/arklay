# `vib_set0`

Starts a pad rumble on the console version, and does nothing on the PC, in the original as in the port.

| | |
| --- | --- |
| Opcode | `0x8A` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
vib_set0(f0, w0, w1);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. |
| `w0` | `u16` | Not read by the port. |
| `w1` | `u16` | Not read by the port. |

## Behaviour

The PC executable has no rumble: its handler for both `vib_set0` and
`vib_set1` (`0x4E9110`) only steps over the operands, so the port does the
same. That is the whole of its behaviour, which is why its status is `yes`.
The operands are the console version's rumble settings, which nothing on the
PC reads.

## Example

```c
proc main_3C
{
    vib_set0(0, 120, 30);
    sce_shake_on(2, 1);
    evt_end(0);
}
```

Illustrative: a jolt that rumbles the console's pad and offsets the picture;
on the PC only the offset is seen. Rooms 101 and 102 use both `vib_set`
forms in their scenes.

## See also

- [`vib_set1`](vib_set1.md): the second form.
- [`vib_fade_set`](vib_fade_set.md): a rumble's fade, not modelled.
- [`sce_shake_on`](sce_shake_on.md): shakes the picture.
