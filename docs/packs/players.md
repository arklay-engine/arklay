# Players

`player/` holds the playable characters: one folder per character, named by
its number in two hex digits. The number is the one the player picks at the
character select, and the one `game/new_game.lua` is given.

A character's files are in one of two model formats.

## RE 1 format

```text
player/00/
├── skin/
│   ├── 00.emd      the character's own model
│   ├── 01.emd      the first alternate outfit
│   └── 02.emd      the second alternate outfit
├── weapon/
│   ├── 00.emw      no weapon: walking, running, the empty-handed actions
│   ├── 02.emw      holding weapon item 02
│   └── 6f.emw      holding weapon item 6f
└── sound/
    ├── 00.wav      a hurt sound
    ├── ...
    └── 07.wav
```

| File | What | Formats |
|---|---|---|
| `skin/<nn>.emd` | the body model: `00` the character's own, `01` and `02` alternate outfits | EMD |
| `weapon/<item>.emw` | the animations and held mesh for one weapon, named by the weapon's item id; `00` is no weapon | EMW |
| `sound/<nn>` | the character's voice: `00` to `03` hurt sounds, `04` to `07` the same in a room that echoes | `ogg`, `wav` |

The body moves with the clips of the weapon it holds, so each weapon a
character can hold needs its own `.emw`. A weapon item with no file for the
character can't be drawn in their hands. Firing, aiming and reloading are
timed against the clips' frames, so a replacement `.emw` keeps its frame
count.

A room can give the player extra animations for its own actions, such as
pushing and climbing, in `room/<id>/anim/` (see
[Rooms](rooms.md#the-room-folder)).

## RE 2 format

```text
player/00/
├── model_0.pld          the model in scenario 0
├── model_1.pld          the model in scenario 1
├── model_ch_0.pld       the alternate model
└── weapon/
    ├── 02_0.plw         holding weapon item 02, scenario 0
    └── 0a_1.plw
```

| File | What |
|---|---|
| `model_<n>.pld` | the body: mesh, skeleton, base animations, texture |
| `model_ch_<n>.pld` | the character's alternate model |
| `weapon/<item>_<n>.plw` | one weapon's animations and mesh |

`n` is the scenario, `0` or `1`. The engine reads scenario 0's file first,
and scenario 1's when there is none.

## Looking at a model

The model viewer stands a model in a room, posed and animated:

```sh
arklay re1.akpak --room 100 --model player/00/skin/00.emd
arklay re2.akpak --room 100 --model player/00/weapon/02_0.plw
```
