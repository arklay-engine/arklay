# `sce_fade_adjust`

Sets a fade's accumulator, and so its opacity, directly.

| | |
| --- | --- |
| Opcode | `0x74` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
sce_fade_adjust(fade, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `fade` | `u8` | Which fade, as for [`sce_fade_set`](sce_fade_set.md): `0` the screen's own, `1` to `3` the overlays. Others do nothing. |
| `value` | `i16` | The new accumulator. Its value divided by 128 is the opacity; a negative value ends the fade. |

## Behaviour

As the original's `Fade_adjust`: only the accumulator changes. The colour
and the step a tick that [`sce_fade_set`](sce_fade_set.md) gave stay, so the
fade goes on stepping from the new value. A negative `value` stops the fade
and clears the screen at once.

As with `sce_fade_set`, a write to fade 0 takes effect at the end of the
tick, and a write to fades 1 to 3 at once.

## Example

```c
proc main_2A
{
    sce_fade_set(2, 0, 0, 0);       // white, not stepping
    sce_fade_adjust(2, 16384);      // half covered
    sleep(10, 15);
    sce_fade_adjust(2, -1);         // and gone
    evt_end(0);
}
```

Illustrative: a white flash at half strength for half a second. A step of 0
holds the opacity where `sce_fade_adjust` puts it.

## See also

- [`sce_fade_set`](sce_fade_set.md): starts a fade.
