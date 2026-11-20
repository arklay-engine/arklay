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

A title script may pick the reel itself, by its number:
`api:exit("attract", 3)` plays `demo/03.dat`, and the title stays up when the
pack has no such file. Resident Evil 2's title does: its six reels are three
pairs, Leon's and Claire's, and it plays the pairs in turn with one of each
pair picked at random.

To watch one reel, give its position in that sorted list:

```sh
arklay --game re1.akpak --ui demo --demo 2
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

## Resident Evil 2's reels

A reel in a Resident Evil 2 pack (one whose new game starts in a room of that
game's) has that game's format. It is a `0x3E`-byte header and then the
controller words, little-endian:

| Offset | Size | Field |
|---|---|---|
| `0x00` | 1 | stage, from 0 |
| `0x01` | 1 | room number in the stage |
| `0x02` | 1 | the camera |
| `0x03` | 1 | the equipped item's id |
| `0x04` | 1 | the inventory slot that holds it |
| `0x05` | 1 | the controller configuration the words were recorded with |
| `0x06` | 1 | the player (`0` Leon, `1` Claire) |
| `0x08` | 44 | the inventory: 11 slots of item id, quantity and two more bytes; the last is the personal item |
| `0x34` | 2 | how many controller words to play |
| `0x36` | 6 | player X, Y and Z |
| `0x3C` | 2 | facing angle |
| `0x3E` | | the controller: one word per frame |

The player starts with full health and eight inventory slots. The file may
be longer than its count of words says; the rest is not played.
