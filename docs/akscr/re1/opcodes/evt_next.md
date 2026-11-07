# `evt_next`

Yields: the event goes on with the next instruction in the next frame.

| | |
| --- | --- |
| Opcode | `0xFE` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
evt_next();
```

## Parameters

None.

## Behaviour

Moves past itself and ends the slot's turn for this frame. The VM then
steps the next active slot. Next frame the event resumes at the
instruction after `evt_next`.

It is the one-frame wait, and the yield that loops need: a `do`/`while` or
`for` body without one runs entirely in one frame. In a tween sequence it
paces a movement one step per frame.

`evt_next` works in every state and leaves the state as it is.

## Example

```c
do
{
    evt_next();
}
while (ck(FG_ROOM, F_ROOM_J_BARRY_ANIM_DONE, FALSE));
```

Room 105 (`games/re1/room/105/room.akscr`): waits a frame at a time until
Barry's animation raises its done flag.

## See also

- [`evt_sleep`](evt_sleep.md): waits several frames.
- [`evt_do`](evt_do.md): the loop it usually sits in.
