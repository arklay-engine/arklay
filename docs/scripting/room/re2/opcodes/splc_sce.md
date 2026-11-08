# `splc_sce`

Gives the partner back to the scripts: a scene character until [`splc_ret`](splc_ret.md).

| | |
| --- | --- |
| Opcode | `0x71` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
splc_sce();
```

## Parameters

None.

## Behaviour

As the original's `Splc_sce` (`0x4e8d00`), on the task's character (the
partner, chosen with [`work_set`](work_set.md)`(2, 0)`):

- its routine becomes `Spl_Scenario` standing (`4, 6, 0, 0`), so the
  `plc_*` instructions steer it;
- its head looks ahead (`Neck_flg` `0x92`, at its own neck), not at the player;
- its `Type` loses the partner's `0x400` and takes a scene character's
  `0x5000`, and its life and maximum life are -1;
- `fg_status`'s partner bit (`0x10000000`, `ck(1, 3, ...)`) comes down, so the
  player's head stops looking at it and a door no longer waits for it.

On a work the port doesn't model, the log notes
`RE2 script: splc_sce is not modelled` once per run.

## Example

```c
proc main_23
{
    work_set(2, 0);              // the partner
    splc_sce();
    plc_dest(0, 4, 44, -3000, 2500);
    evt_end(0);
}
```

Illustrative: take Sherry out of her following to walk her in a scene.

## See also

- [`splc_ret`](splc_ret.md): hands her back as the partner.
- [`plc_motion`](plc_motion.md), [`plc_dest`](plc_dest.md): scripted motions on the task's work.
