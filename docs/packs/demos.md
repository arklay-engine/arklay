# Demos

`demo/` holds the attract-mode demos: the recorded play the title screen
shows when it is left alone. Each demo is a **reel**, one `.dat` file named
by a hex number:

```text
demo/00.dat
demo/01.dat
demo/02.dat
```

The title screen plays every `demo/*.dat` in the pack, sorted by name, one
after the other. There is no fixed count: a pack may carry any number of
reels, or none, and a mod adds a reel by adding a file. A reel ends when its
recorded length runs out or when a button is pressed.

To watch one reel, give its position in that sorted list:

```sh
arklay re1.akpak --ui demo --demo 2
```

## The reel format

A reel records where the demo starts and then the controller input of every
frame. The engine replays that input through the ordinary game, so a reel
only plays back correctly in the room, with the scripts, that it was
recorded in.

A reel is `0x994` bytes, little-endian:

| Offset | Size | Field |
|---|---|---|
| `0x00` | 1 | room number in the stage |
| `0x01` | 1 | stage, from 0 |
| `0x02` | 1 | how many items the player holds |
| `0x03` | 1 | the equipped item's id |
| `0x04` | 1 | bit 0: the character |
| `0x06` | 2 | player X |
| `0x08` | 2 | player Z |
| `0x0A` | 2 | facing angle (low 12 bits) |
| `0x0E` | 2 | the playback counter's starting value |
| `0x10` | 2 | the reel's length in frames |
| `0x14` | 1 | the camera |
| `0x18` | 24 | the inventory: 12 pairs of item id and quantity |
| `0x30` | 2404 | the controller: 1202 words, one per frame |
| `0x990` | 2 | the controller configuration |
| `0x992` | 2 | the player's health |

The last four bytes overlap the last two controller words. A reel may be
shorter than `0x994` bytes: it needs the `0x30`-byte header and at least
four bytes after it.
