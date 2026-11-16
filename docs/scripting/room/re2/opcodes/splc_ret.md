# `splc_ret`

Hands a scene character back as the partner, who follows the player again.

| | |
| --- | --- |
| Opcode | `0x70` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
splc_ret();
```

## Parameters

None.

## Behaviour

As the original's `Spcl_ret` (`0x4e8c70`), on the task's character (chosen with
[`work_set`](work_set.md)`(2, 0)`):

- its routine becomes `Spl_Auto`'s (`1, 0, 0, 0`): Sherry or Ada follows the player, as Ada does after room 216's van;
- its head looks at the player (`Neck_flg` 0);
- its `Type` loses `0x4000` and `0x1000` and takes the partner's `0x400`;
- its life is the partner's saved life and its maximum life 200;
- Sherry is no longer lost (her state's `0x2000`);
- `fg_status`'s partner bit (`0x10000000`, `ck(1, 3, ...)`) goes up.

On a work the port doesn't model, the log notes
`RE2 script: splc_ret is not modelled` once per run.

## Example

```c
proc main_22
{
    work_set(2, 0);              // the partner
    splc_ret();
    evt_end(0);
}
```

Illustrative: the end of a scene that had taken Sherry with [`splc_sce`](splc_sce.md).

## See also

- [`splc_sce`](splc_sce.md): its companion.
- [`plc_ret`](plc_ret.md): ends a scripted motion on the task's work.
- [`heal_partner`](heal_partner.md): restores the partner's life.
