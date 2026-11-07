# `costume_ck`

Passes when the alternate costume is set.

| | |
| --- | --- |
| Opcode | `0x50` |
| Group | Conditions |
| Kind | Condition |
| Size | 2 bytes |

## Syntax

```c
costume_ck(costume);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `costume` | `u8` | `COSTUME_xx`. The original ignores it, and so does the port. |

## Behaviour

`costume_ck` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It passes when the one-bit costume variant is 1, as
[`costume_set`](costume_set.md) or the player's outfit choice left it,
whatever the argument. So `costume_ck(COSTUME_00)` passes for the second
alternate outfit, the one the variant bit 1 stands for, despite its
argument.

## Example

```c
if (ck(FG_SCENARIO, F_SCENARIO_ALT_OUTFIT, TRUE))
{
    if (costume_ck(COSTUME_00))
    {
        aot_set(AOT_04_EVENT_COSTUME_DIFFERENT, 9200, 1900, 7300, 1500, SCE_MESSAGE, SAT_MANUAL | SAT_PLAYER, 128, 0, 69, 0, 0, 0);
        ...
    }
}
```

Room 11C (`games/re1/room/11c/room.akscr`), the changing room: which
wardrobe offers which outfit depends on the one already worn.

## See also

- [`costume_set`](costume_set.md): sets the costume bit.
