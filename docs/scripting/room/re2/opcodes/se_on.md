# `se_on`

Plays a sound effect at an offset from a body, an object or a fixed point.

| | |
| --- | --- |
| Opcode | `0x36` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 12 bytes |
| Status | yes |

## Syntax

```c
se_on(type, sound, work, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `type` | `u8` | The sound bank: `0` doors, `1` weapons, `2` the room's, `3` the enemies', `4` the core bank. Any other value plays nothing. |
| `sound` | `i16` | Low byte: the sound's number in the bank. High byte: `0` for a sound with no position, non-zero for one panned from its position; `2` also stops walls from muffling it. |
| `work` | `i16` | Low byte: where the sound is, `0` a fixed point, `1` the player, `2` the partner, `3` the character in the enemy slot in the high byte, `4` the room object in the high byte. |
| `x`, `y`, `z` | `i16` | The offset from that body or object, or the point itself for `0`. |

## Behaviour

`se_on` queues the sound, as the original's `Sce_se_on` does, at the
position of the body or object `work` names plus `x, y, z`. A partner,
character or object that is not there gives the player's position instead,
and so does a `work` byte past 4. The sound is played after the scripts have
run for the tick.

## Example

```c
se_on(2, 0x0103, 1, 0, 0, 0);
```

Illustrative: room sound 3, panned from the player's position.

## See also

- [`sce_bgm_control`](sce_bgm_control.md): the music.
- [`xa_on`](xa_on.md): a voice line.
