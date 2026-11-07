# `eml_state`

Changes one property of an enemy: its behaviour, action, status, angle, joints and so on.

| | |
| --- | --- |
| Opcode | `0x28` |
| Group | Enemies and characters |
| Kind | Command |
| Size | 6 bytes; 8 for `EML_REVIVE`, 4 for `EML_BLEND_RESET` and `EML_08` |

## Syntax

```c
eml_state(0, enemy, sub, value);                 // most sub-commands
eml_state(0, enemy, EML_REVIVE, health, hit_state, 0);
eml_state(0, enemy, EML_BLEND_RESET);            // and EML_08
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; the shipped scripts write 0. |
| `enemy` | `u8` | The enemy slot (`ENEMY_xx`, a room define). |
| `sub` | `u8` | The sub-command (`EML_*`, below). It sets how many arguments follow. |
| `value` | `u16` | The sub-command's value; absent for `EML_BLEND_RESET` and `EML_08`. |
| `hit_state` | `u8` | `EML_REVIVE` only: the hit state byte. |
| *pad* | `u8` | `EML_REVIVE` only: unused, written as 0. |

The sub-commands:

| Sub-command | Value | Does |
| --- | --- | --- |
| `EML_BEHAVIOUR` (0) | `EB_xx` | writes the behaviour byte (its low byte) |
| `EML_REVIVE` (1) | health, then the hit state byte | puts the enemy in state 2 with its ignore flag, action behaviour and action state cleared, and sets its health and hit state |
| `EML_ACTION` (2) | `AB_xx` | writes the action behaviour (low byte) and clears the action state |
| `EML_STATUS` (3) | `EML_STATUS_SET`, `EML_STATUS_OR` or `EML_STATUS_XOR`, ORed with the bits | writes, ORs or flips the status flags byte |
| `EML_ANGLE` (5) | angle | writes the facing angle |
| `EML_BLEND_RESET` (6) | none | zeroes the animation blend counter |
| `EML_08` (8) | none | puts the enemy in state 9, clearing its ignore flag, action behaviour and action state |
| `EML_JOINTS` (9) | `JOINT_xxxx` bits | flips the joint flags: each bit toggles one joint, which hides or shows it |
| `EML_ACTION_STATE` (10) | number | writes the action state (low byte) |

## Behaviour

The instruction's length depends on `sub`, and the compiler checks the
argument count against it. Sub-commands 4, 7 and past 10 have no length
in the original (its interpreter would spin on them), so the compiler
rejects them.

The change applies at once to the entity of the slot, whether or not the
slot is active; a slot past the entity table is ignored. A state set here
(`EML_REVIVE`'s 2, `EML_08`'s 9) is the entity's own state machine, read
by its type's code on its next update, so what it does depends on the
type. `EML_08`'s meaning is not known yet; the shipped scripts use it on
Barry.

## Example

```c
act_reset();
sleep(100);
pos_set(ENEMY_01_HUNTER, 0, 808, 0, 9431, 0, 7822);
sleep(65);
eml_state(0, ENEMY_01_HUNTER, EML_ACTION, AB_0D);
evt_fork(TASK_09, rebecca_hunter_attack_hit, 0);
sleep(60);
eml_state(0, ENEMY_01_HUNTER, EML_ACTION, AB_12);
evt_finish();
```

```c
eml_state(0, ENEMY_00_REBECCA, EML_STATUS, EML_STATUS_SET | 1);
```

Room 101 (`games/re1/room/101/room.akscr`) drives the Hunter's attack on
Rebecca by switching its action behaviour; room 206 sets Rebecca's status
flags.

## See also

- [`enemy`](enemy.md): sets an enemy up.
- [`get_eml_state`](get_eml_state.md): reads an enemy's behaviour byte.
- [`pos_set`](pos_set.md): moves an enemy.
