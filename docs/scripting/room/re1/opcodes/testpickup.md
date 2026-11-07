# `testpickup`

Passes when the item the player has just picked up is the one named.

| | |
| --- | --- |
| Opcode | `0x11` |
| Group | Conditions |
| Kind | Condition |
| Size | 2 bytes |

## Syntax

```c
testpickup(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id (`IT_xx_*` from `inc/items.akinc`). |

## Behaviour

`testpickup` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It compares the picked-item state byte (`B_PICKED_ITEM`) with `item`. A
pickup writes that byte, and the engine clears it after every frame's main
scripts have run, so the test passes on one pass of `proc main`. It is a
plain byte compare: `testpickup(IT_00_NONE)` passes on every frame nothing
was picked up.

## Example

```c
proc main
{
    if (testpickup(IT_2F_KEY_INK_RIBBON))
    {
        aot_delete(AOT_05_MESSAGE_NOTHING, SCE_MESSAGE, SAT_MANUAL | SAT_PLAYER);
    }
}
```

Room 112 (`games/re1/room/112/room.akscr`): once the ink ribbon is taken,
its spot turns into a plain examine message.

## See also

- [`testitem`](testitem.md): the item just used.
- [`item_ck`](item_ck.md): whether an item is carried.
