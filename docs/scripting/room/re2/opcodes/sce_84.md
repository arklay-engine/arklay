# `sce_84`

Opens the status screen straight onto a floor's map, with one room
blinking, until the player closes it.

| | |
| --- | --- |
| Opcode | `0x84` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
sce_84(target);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `target` | `u8` | Which map and room: 0 and 2 show floor 5 (POLICE STATION B1), 1 and 4 floor 3, 3 floor 0x10. The room that blinks is `0x53DB28[target]` on that floor (`0x16`, `0x0A`, `0x1A`, `0x07`, `0x08`). |

## Behaviour

As the original's handler (`0x4E8FE0`): like `Sce_Status`, it saves
`fg_stop`, raises `fg_status`'s `0x8000` and sets `Game` to 1, so the status
screen opens on the next tick over START's fades; but it opens in the status
task's mode 4 (`St_init04`, `St_main04`), which shows only the floor's map.
The map fades in from the screen's sixth tick, the room's fill blinking with
the screen's blink and a cue (`0x22B0000`) at each turn of it, and plays the
map's cue (`0x4090000`) once it is in. Cancel or START plays the cancel cue
and closes it, back to play over the status screen's close.
Play waits meanwhile.

The port is `src/game/status2/script_map.rs`. Room 301's talk with Ben and
Ada (`main_02`) ends with `sce_84(2)`, the B1 map with the sewer's room
blinking.

## Example

```c
proc main_42
{
    sce_84(0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.
