# `evt_if`

Runs the following event code only when its condition commands pass; written as an `if` in an event.

| | |
| --- | --- |
| Opcode | `0xF4` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 4 bytes plus its condition commands |
| Valid in | every state |

## Syntax

```c
if (cond && cond ...)
{
    event statements...
}
else
{
    event statements...
}
```

## Parameters

None written in `.akscr`: the conditions are the `if` head, and the
compiler fills in the jumps.

| Parameter | Width | Description |
| --- | --- | --- |
| `body` | `u8` | Bytes from the opcode to the first byte of the body, past the condition commands. |
| `skip` | `u16` | Bytes from the opcode to where the event goes on when a condition fails: the `else` body, or past the `if`. |

## Behaviour

The port's own instruction: the original's event VM has no `if`, so an
event that uses one runs only in arklay. It is how an `if` in an event
compiles. An `if` inside a `run` block is command code instead, and
compiles to the command `if`.

The condition commands sit between the header and the body. The VM tests
them in order; when all pass it goes on at the body, and at the first that
fails (or one that does not decode) it goes on at `skip`. An `else` body
follows an [`evt_jump`](evt_jump.md) at the end of the `then` body, which
jumps over it. Neither instruction yields, nor uses the slot's loop stack,
so an `if` can stand in any state and inside `for` and `do` loops.

The head is one or more [condition commands](index.md#conditions)
joined by `&&`, as in command code.

Both branches must leave the event in the same state, both inside or both
outside the same actor or tween sequence. The compiler checks this and says
so when they differ. The exception is an `if` that tests only the
[room-variant flags](../index.md#room-variants): each variant then takes
its own branch, and a variant build folds the `if` away and emits no
`evt_if` at all. See [Event `if`](../index.md#event-if).

The body offset is one byte, so the condition commands must fit in 251
bytes. The skip is two bytes, so the body's length is not limited in
practice.

## Example

```c
evt_tween_begin_alt();
tw_nop();
if (ck(FG_STATUS, F_STATUS_PLAYER_JILL, FALSE))
{
    tw_set_sel(TW_HEALTH, 140);
}
else
{
    tw_set_sel(TW_HEALTH, 96);
}
tw_end();
```

Room 100 (`games/re1/room/100/room.akscr`), event `wake_up_after_poison`:
restores the player's health, by character, inside a tween sequence. In a
build for one variant the `if` folds away; in the build that serves every
variant it is an `evt_if`.

## See also

- [`evt_jump`](evt_jump.md): the jump over an `else` body.
- [`evt_do`](evt_do.md): a loop with a condition.
- [`if_long`](if_long.md): the command code's long `if`.
