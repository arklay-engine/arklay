# `aot_on`

Fires an interaction area as if the player had triggered it.

| | |
| --- | --- |
| Opcode | `0x47` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
aot_on(slot);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The area slot to fire. |

## Behaviour

`aot_on` runs the handler of the area in slot `slot` at once, whatever its
trigger byte and wherever the player is. A slot with no area does nothing.
What happens is the area's kind's (see [`aot_set`](aot_set.md)):

- A **door** opens, or answers as a locked door does (see
  [`door_aot_set`](door_aot_set.md)). The scripts open a door after the
  original has moved the player and checked the camera for the tick, so in
  the port the player, the camera switch and the characters sit out the tick
  a script opens a door.
- An **item** opens the get-item window, or starts the crouch first for an
  item picked up crouching.
- A **message** area shows its message, and an **event** area queues its
  procedure for the start of the next tick.
- A **flag change**, **water** or **damage** area applies as if the player
  were inside it.
- A kind-0 area does nothing.

## Example

```c
proc main_05
{
    set(2, 7, 1);
    work_set(1, 0);
    plc_dest(0, 4, 20, -4600, -800);
    do
    {
        evt_next();
    }
    while (ck(5, 20, 0));
    set(2, 7, 0);
    aot_on(0);                   // through the door in area 0
    evt_end(0);
}
```

Illustrative: a scene walks the player to a door, which sets room bit 20 on
arrival, then opens the door in area 0.

## See also

- [`aot_set`](aot_set.md): the area kinds.
- [`aot_reset`](aot_reset.md): changes an area.
- [`door_aot_set`](door_aot_set.md): the door it opens.
