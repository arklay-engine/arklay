# `ck_item_count`

Compares the quantity the player carries of a weapon's item group with a count.

| | |
| --- | --- |
| Opcode | `0x22` |
| Group | Conditions |
| Kind | Condition |
| Size | 4 bytes |

## Syntax

```c
ck_item_count(group, comparison, count);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The item group: `IGRP_ANY` (0x0A) for any item, or `IGRP_xx` for the others below. |
| `comparison` | `u8` | `CMP_EQ`, `CMP_GT`, `CMP_GE`, `CMP_LT`, `CMP_LE` or `CMP_NE` (0 to 5), the total on the left. |
| `count` | `u8` | The count to compare with. |

## Behaviour

`ck_item_count` is a [condition](index.md#conditions): it stands in an
`if` head, a `require` or a `do`/`while` test.

It sums the quantities of the inventory stacks in the group and passes when
at least one stack is in the group and the total compares to `count` as
`comparison` says. With no stack in the group it fails, whatever the
comparison. The groups are the weapons and their loaded ammunition:

| Group | Conditions |
| --- | --- |
| `IGRP_ANY` (0x0A) | every stack (see below) |
| `IGRP_0B` | item 2, the handgun |
| `IGRP_0C` | item 3, the shotgun |
| `IGRP_0D` | items 4 and 5, the magnums |
| `IGRP_0F` | item 6, the flamethrower |
| `IGRP_10` `IGRP_11` `IGRP_12` | items 7 to 9, the grenade launcher's three loads |

`IGRP_0E` and any other group match nothing, so the test fails. For
`IGRP_ANY` the original does not sum: it keeps overwriting the total with
each stack's quantity, so the total is the last stack's quantity. The port
keeps that.

## Example

```c
if (ck(FG_ROOM, F_ROOM_ONLY_ROCKET_LAUNCHER, FALSE) && item_ck(IT_0A_WEAPON_ROCKET_LAUNCHER) && ck_item_count(IGRP_ANY, CMP_EQ, 0))
{
    set(FG_ROOM, F_ROOM_ONLY_ROCKET_LAUNCHER, TRUE);
}
```

Room 303 (`games/re1/room/303/room.akscr`), the Tyrant fight on the
heliport.

## See also

- [`item_ck`](item_ck.md): whether one item is carried.
