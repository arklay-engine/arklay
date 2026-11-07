# `tw_set_field`

Writes one byte of the selection's state block: its state, ignore byte, behaviour, action state, health or hit state.

| | |
| --- | --- |
| Opcode | `0x08` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 3 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_set_field(field, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `field` | `u8` | The byte to write (`TW_*`, below). |
| `value` | `u8` | The value. |

## Behaviour

| `field` | Writes |
| --- | --- |
| `TW_STATE` (0) | the entity state: 1 idle, 8 scripted action |
| `TW_IGNORE` (1) | the ignore byte |
| `TW_BEHAVIOUR` (2) | the action behaviour |
| `TW_ACTION_STATE` (3) | the action state |
| `TW_HEALTH_LO` (4) | the low byte of the health |
| `TW_HEALTH_HI` (5) | the high byte of the health |
| `TW_HIT_STATE` (6) | the hit state, as [`tw_set_8a`](tw_set_8a.md) |

Any other `field` writes nothing. It does not yield.

These are the state-block bytes at `+0x84` to `+0x8A` of an entity. On a
room object or item model they are only the record's tween scratch, with
one effect: `TW_STATE` and `TW_IGNORE` are the same word as the z rotation
step of [`tw_set_rot`](tw_set_rot.md). No shipped script uses
`tw_set_field`.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_tween_begin_alt();
tw_set_field(TW_STATE, 1);
tw_set_field(TW_BEHAVIOUR, 0);
tw_end();
```

Illustrative, in room 105 (`games/re1/room/105/room.akscr`): puts Barry
back in his idle state, as [`act_idle`](act_idle.md) does in an actor
sequence.

## See also

- [`tw_set_sel`](tw_set_sel.md): writes the health as a word, or a position
  axis.
- [`act_idle`](act_idle.md): resets the state block.
