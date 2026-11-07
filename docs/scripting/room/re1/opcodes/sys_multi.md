# `sys_multi`

Changes one property of the player, chosen by a sub-command.

| | |
| --- | --- |
| Opcode | `0x33` |
| Group | The player |
| Kind | Command |
| Size | 4 bytes; 2 for `SYS_UNEQUIP`, `SYS_FORCE_ACTION`, `SYS_CLEAR_8C` and `SYS_IDLE` |

## Syntax

```c
sys_multi(sub, value);
sys_multi(sub);          // SYS_UNEQUIP, SYS_FORCE_ACTION, SYS_CLEAR_8C, SYS_IDLE
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `sub` | `u8` | The sub-command (`SYS_*`, below). It sets whether a value follows. |
| `value` | `u16` | The sub-command's value; absent for the four that take none. |

The sub-commands, all on the player:

| Sub-command | Value | Does |
| --- | --- | --- |
| `SYS_UNEQUIP` (0) | none | unequips the weapon and clears the scripted clip index |
| `SYS_HIT` (1) | `HIT_xx` (low byte) | puts the player in the being-attacked state (state 1, action state 1) with that attack kind, stopping its movement and animation |
| `SYS_PLAYER_FLAGS` (3) | `SYS_SET`, `SYS_OR` or `SYS_XOR`, ORed with `PLF_xx` bits | writes, ORs or flips the player flags byte |
| `SYS_FORCE_ACTION` (4) | none | sets action behaviour 1, action state 6 |
| `SYS_ANGLE` (5) | angle | sets the facing angle (low 12 bits, 0 to 4095) |
| `SYS_CLEAR_8C` (6) | none | clears an internal player byte (`+0x8C` in the original's entity) |
| `SYS_IDLE` (7) | none | returns the player to idle: state 1, action state 2, the being-attacked byte and the scripted clip cleared |
| `SYS_HEALTH_STATUS` (8) | `SYS_SET`, `SYS_OR` or `SYS_XOR`, ORed with `HS_xx` bits | writes, ORs or flips the health-status flags (also the `B_HEALTH_STATUS` state byte); `HS_LOCKED` locks the controls |
| `SYS_JOINTS` (9) | `JOINT_xxxx` bits | flips the player's joint flags |
| `SYS_E0_40` (10) | `SYS_ON` or `SYS_OFF` | sets or clears bit `0x40` of an internal player byte (`+0xE0`) |

## Behaviour

The instruction's length depends on `sub`: the compiler takes the value
argument only for the sub-commands that have one and checks the count.
Sub-commands 2 and past 10 have no length in the original (its interpreter
would spin on them), so the compiler rejects them.

For `SYS_PLAYER_FLAGS` and `SYS_HEALTH_STATUS` the value's high byte is
the mode (0 set, 1 OR, 2 XOR) and its low byte the bits; another mode
leaves the flags as they are. For `SYS_E0_40` any non-zero high byte sets
the bit. Most of the player flag bits, health-status bits, hit kinds and
joint bits have no meaning-based names yet, so they use the placeholder
names (`PLF_04`, `HS_20`, `HIT_80`, `JOINT_0001`).

`SYS_HIT` and `SYS_IDLE` also clear the player's ignore flag. The joint
flags `SYS_JOINTS` flips are recorded, but the port's renderer does not
hide joints from them yet.

## Example

```c
plw_anim(PLW_02, ANIM_30, 0);
sys_multi(SYS_PLAYER_FLAGS, SYS_OR | PLF_04);
sys_multi(SYS_UNEQUIP);
```

```c
sys_multi(SYS_ANGLE, 2048);
```

Room 100 (`games/re1/room/100/room.akscr`) raises a player flag and takes
the weapon away for Chris's poisoned wake-up; room 20d turns the player
to face angle 2048 (half a turn).

## See also

- [`plw_anim`](plw_anim.md): plays a scripted player animation.
- [`dir_set`](dir_set.md): places and turns the player.
- [`eml_state`](eml_state.md): the same kind of setter for an enemy.
