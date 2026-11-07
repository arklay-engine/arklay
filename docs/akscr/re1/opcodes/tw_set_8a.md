# `tw_set_8a`

Writes the selection's hit-state byte.

| | |
| --- | --- |
| Opcode | `0x09` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 2 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_set_8a(value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `value` | `u8` | The new hit state. |

## Behaviour

Writes the byte at `+0x8A` of the selection's state block, the hit state,
the same as `tw_set_field(TW_HIT_STATE, value)`. It does not yield.

On the player the original's byte at `+0x8A` is the being-attacked flag;
the port keeps that flag apart, and `tw_set_8a` writes the hit-state field
rather than it ([`act_idle`](act_idle.md) does clear the flag). On a room
object or item model the byte is only tween scratch. No shipped script
uses `tw_set_8a`.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_tween_begin_alt();
tw_set_8a(0);
tw_end();
```

Illustrative, in room 105 (`games/re1/room/105/room.akscr`): clears
Barry's hit state.

## See also

- [`tw_set_field`](tw_set_field.md): writes any of the state bytes.
