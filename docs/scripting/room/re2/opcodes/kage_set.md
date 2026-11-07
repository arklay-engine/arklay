# `kage_set`

Gives a body a shadow or a pool under it, such as a corpse's pool of blood (modelled for enemy slots only).

| | |
| --- | --- |
| Opcode | `0x60` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 14 bytes |
| Status | partial |

## Syntax

```c
kage_set(kind, slot, r, g, b, half_x, half_z, off_x, off_z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `kind` | `u8` | Whose body, as [`work_set`](work_set.md)'s kinds. Only `3`, an enemy slot, is modelled. |
| `slot` | `u8` | The enemy slot. |
| `r` | `u8` | The colour's red. |
| `g` | `u8` | Its green. |
| `b` | `u8` | Its blue. |
| `half_x` | `i16` | Half the shape's size in x. |
| `half_z` | `i16` | Half its size in z. |
| `off_x` | `i16` | Its offset from the body in x (`Off_x`), turned with the body. |
| `off_z` | `i16` | Its offset in z (`Off_z`). |

## Behaviour

With `kind` 3, the character in enemy slot `slot` gets this shadow in place
of the one its own init gave it: the shadow texture, tinted with the colour,
`2 * half_x` by `2 * half_z`, on the ground at the offset, which turns as the
body does. A later `kage_set` replaces it. A zombie whose own routines size
its shadow keeps their size, and a zombie lying in its pool of blood draws it
in the pool's colour. An empty slot, or one whose enemy the port does not
model, is left alone.

Any other `kind` does nothing and logs `RE2 script: kage_set is not
modelled` once per run. The retail scripts only give it kind 3.

## Example

```c
proc main_1A
{
    kage_set(3, 0, 96, 0, 0, 900, 700, 0, -300);
    evt_end(0);
}
```

Illustrative: a dark red pool, 1800 by 1400, spreading behind the body in
enemy slot 0.

## See also

- [`sce_em_set`](sce_em_set.md): places the character.
