# `work_set`

Chooses the work (the player, the partner, an enemy slot or a room object) that the task's later instructions act on.

| | |
| --- | --- |
| Opcode | `0x2E` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
work_set(kind, index);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `kind` | `u8` | What to act on: `1` the player, `2` the partner (`splayer_work`), `3` the character in enemy slot `index`, `4` room object `index`. |
| `index` | `u8` | The enemy slot (kind 3, `0` to `31`) or the object slot (kind 4). Not read for kinds 1 and 2; write 0. |

## Behaviour

Each of the 14 task slots has its own work. `work_set` sets the running
task's work and clears the task's twelve speeds (see
[`speed_set`](speed_set.md)). The choice belongs to the task slot, not to the
procedure: it lasts until the slot runs another `work_set`, and a slot that
has never run one acts on the player.

These instructions then apply to the chosen work: [`pos_set`](pos_set.md),
[`dir_set`](dir_set.md), [`add_speed`](add_speed.md),
[`member_set`](member_set.md), [`member_set2`](member_set2.md),
[`member_copy`](member_copy.md), [`member_cmp`](member_cmp.md),
[`dir_ck`](dir_ck.md), the `plc_*` instructions, [`parts_set`](parts_set.md),
[`sce_parts_bomb`](sce_parts_bomb.md) and
[`sce_parts_down`](sce_parts_down.md).

The port checks the target when `work_set` runs. Kind 2 selects the partner
only if a character is in the partner's slot, kind 3 only if a character is in
enemy slot `index`, and kind 4 only if the room has object record `index`.
Any other case, including a kind the port does not know, selects a work the
engine does not model. On such a work the instructions above do nothing, and
those without a case for it (`pos_set`, `dir_set` and most `plc_*`) log
`RE2 script: NAME is not modelled` once per run. A character slot whose id
the port does not model is empty (see [`sce_em_set`](sce_em_set.md)), so
`work_set(3, slot)` on it selects nothing.

## Example

```c
proc main_04
{
    work_set(3, 0);              // the character in enemy slot 0
    pos_set(0, -12000, 0, 8400);
    dir_set(0, 0, 2048, 0);
    work_set(1, 0);              // back to the player
    evt_end(0);
}
```

Illustrative: it places the character in enemy slot 0, turns it to face the
other way (2048 is half a turn), and points the task back at the player.

## See also

- [`sce_em_set`](sce_em_set.md): places a character in an enemy slot.
- [`obj_model_set`](obj_model_set.md): places a room object.
- [`member_set`](member_set.md): writes a field of the work.
- [RE 2 scripts](index.md#characters-objects-and-the-work): the group's overview.
