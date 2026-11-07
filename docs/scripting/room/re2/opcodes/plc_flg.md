# `plc_flg`

Changes the scene flags (`Sce_flg`) of the task's character or the player.

| | |
| --- | --- |
| Opcode | `0x43` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
plc_flg(how, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `how` | `u8` | `0` ors `value` in, `1` sets the flags to `value`, `2` xors `value` in. Anything else does nothing. |
| `value` | `u16` | The bits. |

## Behaviour

Changes `Sce_flg` (member 22), the 16 bits that the work's scenario routine
reads, at once. Which bits mean what depends on the routine. One the port
reads itself: bit 2 (value 4) keeps a [`plc_dest`](plc_dest.md) walk's last
state running on arrival, and lets a second `plc_dest` with the same routine
go on without restarting it.

[`plc_motion`](plc_motion.md) sets the flags outright, and `plc_dest` clears
them, so `plc_flg` comes after either.

It applies to the player and to a character. On a room object or a work the
engine does not model it does nothing, and the log notes
`RE2 script: plc_flg is not modelled` once per run.

## Example

```c
proc main_15
{
    work_set(3, 0);
    plc_dest(0, 5, 42, -4000, 6000);
    plc_flg(0, 4);               // keep running on arrival
    evt_end(0);
}
```

Illustrative: the character in enemy slot 0 runs to a point and keeps
running when it gets there, until another instruction changes its routine.

## See also

- [`plc_motion`](plc_motion.md): sets the flags with a motion.
- [`member_set`](member_set.md): `member_set(22, ...)` writes them too.
- [`member_cmp`](member_cmp.md): `member_cmp(0, 22, 6, mask)` tests them.
