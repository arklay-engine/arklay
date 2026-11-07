# `evt_dountil`

Ends a `do` loop's body: tests the condition and goes back to the body while it passes.

| | |
| --- | --- |
| Opcode | `0xFD` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
do
{
    ...
}
while (cond);
```

The `while (cond);` of the `do`.

## Parameters

None. The condition is stored after the matching [`evt_do`](evt_do.md).

## Behaviour

Takes the entry on top of the slot's loop stack and tests the condition
command it points at. When it passes, the event goes back to the start of
the body; when it fails (or the address holds no condition command), the
entry is popped and the event goes on. It does not yield in either case.
With an empty stack it moves on.

Despite the name, the loop repeats while the condition passes: `while
(ck(FG_ROOM, F_ROOM_X, FALSE))` waits until the flag is set.

## Example

```c
do
{
    evt_next();
}
while (ck(FG_STATUS, F_STATUS_VOICE_PLAYING, TRUE));
do
{
    evt_next();
}
while (cmpb(B_MENU_CHOICE, CMP_GE, 128));
```

Room 105 (`games/re1/room/105/room.akscr`), event
`hall_door_not_done_or_barry_walks_in`: waits for the voice line to end,
then for the message to close.

## See also

- [`evt_do`](evt_do.md): the start of the loop.
