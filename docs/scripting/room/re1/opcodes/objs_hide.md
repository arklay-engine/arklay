# `objs_hide`

Tints the player's model dark red.

| | |
| --- | --- |
| Opcode | `0x4D` |
| Group | The player |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
objs_hide(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; written as 0. |

## Behaviour

Despite its name, `objs_hide` hides nothing: it tints every joint of the
player's model dark red (`0x30, 0, 0` as the colour multiplier), the
original's death tint. The original also tints the mirror copies of the
joints; the port's mirror pass draws the same mesh, so it follows.

The tint stays for the rest of the session: nothing in the port resets
it. The shipped scripts use it only as the player dies, so it never
outlives the game over.

## Example

```c
run
{
    objs_hide(0);
    set(FG_STATUS, F_STATUS_SCREEN_SHAKE, FALSE);
    set(FG_STATUS, F_STATUS_PLAYER_DEAD, TRUE);
    bgm_stop(BGM_SUB_1);
}
evt_finish();
```

Room 30b (`games/re1/room/30b/room.akscr`): the boulder has crushed the
player, so the event darkens the model and raises the death flag.

## See also

- [`sys_multi`](sys_multi.md): the player's other properties.
