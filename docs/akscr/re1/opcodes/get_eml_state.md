# `get_eml_state`

Reads an enemy's behaviour byte back.

| | |
| --- | --- |
| Opcode | `0x39` |
| Group | Enemies and characters |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
get_eml_state(enemy);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `enemy` | `u8` | The enemy slot (`ENEMY_xx`, a room define). |

## Behaviour

`get_eml_state` copies the behaviour byte of the entity in slot `enemy`
(the byte [`enemy`](enemy.md) and `eml_state(.., EML_BEHAVIOUR, ..)` write)
into a byte the scripts can test. A slot past the entity table leaves it
unchanged.

Every shipped use is followed by a test of the state byte `B_0A`
(`cmpb(B_0A, CMP_NE, 0)`), which suggests the original writes the byte
there. The port keeps the value in a field of its own, which `cmpb` does
not read, so those tests see whatever `B_0A` held before.

## Example

```c
get_eml_state(ENEMY_00_CROW);
if (ck(FG_ENEMY, F_ENEMY_117_CROW_0, FALSE) && cmpb(B_0A, CMP_NE, 0))
{
    pos_set(ENEMY_00_CROW, 0, 1024, 0, 19000, -2000, 4847);
}
get_eml_state(ENEMY_01_CROW);
if (ck(FG_ENEMY, F_ENEMY_117_CROW_1, FALSE) && cmpb(B_0A, CMP_NE, 0))
{
    pos_set(ENEMY_01_CROW, 0, 2048, 0, 19000, -2000, 4247);
}
```

Room 117 (`games/re1/room/117/room.akscr`) moves each crow that is still
alive once the gallery puzzle is solved.

## See also

- [`eml_state`](eml_state.md): changes an enemy's state.
- [`cmpb`](cmpb.md): compares a state byte.
