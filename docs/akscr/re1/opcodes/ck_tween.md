# `ck_tween`

Passes when the player faces within a range of angles.

| | |
| --- | --- |
| Opcode | `0x3F` |
| Group | Conditions |
| Kind | Condition |
| Size | 6 bytes |

## Syntax

```c
ck_tween(0, from, to);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `from` | `u16` | The start of the range, in the engine's angle units (4096 to a full turn). |
| `to` | `u16` | The end of the range. |

## Behaviour

`ck_tween` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It reads the player's facing angle (0 to 4095) and passes when
`(angle - from) <= (to - from)`, both differences taken as unsigned 16-bit
values. So the range runs from `from` up to `to` and both ends pass; when
`from` is greater than `to` the range wraps through 0
(`ck_tween(0, 3072, 1024)` is the half turn from 3072 through 0 to 1024).
This is the original's `cmd_player_dir_test`; the name is the original
tool's.

## Example

```c
if (ck_tween(0, 1024, 3072))
{
    evt_exec(0, TASK_04, crushed_by_boulder);
}
```

Room 30B (`games/re1/room/30b/room.akscr`): which crushing event plays
depends on which way the player faces when the boulder arrives.

## See also

- [`ck_counter`](ck_counter.md): tests the player's distance to something.
- [`dir_set`](dir_set.md): places and turns the player.
