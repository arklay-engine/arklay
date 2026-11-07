# `ck_last_item`

Passes when the player's equipped weapon is the item named.

| | |
| --- | --- |
| Opcode | `0x1D` |
| Group | Conditions |
| Kind | Condition |
| Size | 2 bytes |

## Syntax

```c
ck_last_item(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id (`IT_xx_*` from `inc/items.akinc`). |

## Behaviour

`ck_last_item` is a [condition](index.md#conditions): it stands in an
`if` head, a `require` or a `do`/`while` test.

It passes when the equipped item is `item`. With nothing equipped it fails
for every item, `IT_00_NONE` included. Despite the name, which is the
original tool's, it tests the equipped weapon, not the last item used (that
is [`testitem`](testitem.md)). The state byte `B_EQUIPPED` holds the same
value for `cmpb`.

## Example

```c
if (ck_last_item(IT_02_WEAPON_HANDGUN))
{
    set(FG_ROOM, 3, TRUE);
}
```

Illustrative: no shipped room uses it. The flag is set while the handgun is
equipped.

## See also

- [`item_ck`](item_ck.md): whether an item is carried.
- [`testitem`](testitem.md): the item just used.
