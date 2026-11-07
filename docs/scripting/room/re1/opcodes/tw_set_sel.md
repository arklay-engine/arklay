# `tw_set_sel`

Writes one word of the selection: a position axis, its health or a target coordinate.

| | |
| --- | --- |
| Opcode | `0x0A` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_set_sel(selector, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `selector` | `u8` | What to write (`TW_*`, below). |
| `value` | `i16` | The value. |

## Behaviour

| `selector` | Writes |
| --- | --- |
| `TW_POS_X` (0) | the x position |
| `TW_POS_Y` (1) | the y position |
| `TW_POS_Z` (2) | the z position |
| `TW_HEALTH` (3) | the health |
| `TW_TARGET_X` (4) | the x of the scripted target that [`act_anim_seq`](act_anim_seq.md) walks to |
| `TW_TARGET_Z` (5) | the z of that target |

The selector itself is also stored, in a word of the selection; any other
`selector` writes only that. It does not yield. On a room object or item
model only the three position axes have an effect.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_flag_op(EF_OR, EF_0040);
act_reset();
evt_nop();
evt_tween_begin_alt();
tw_nop();
if (ck(FG_STATUS, F_STATUS_PLAYER_JILL, FALSE))
{
    tw_set_sel(TW_HEALTH, 140);
}
else
{
    tw_set_sel(TW_HEALTH, 96);
}
tw_end();
```

Room 100 (`games/re1/room/100/room.akscr`), event `wake_up_after_poison`:
restores the player's health, 140 for Chris and 96 for Jill.

## See also

- [`tw_abs_pos`](tw_abs_pos.md): sets the whole position.
- [`tw_set_field`](tw_set_field.md): writes a state byte.
