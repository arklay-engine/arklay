# Doors

`door/` holds the door transitions: the short 3D scene of a door opening,
a staircase or a ladder that plays between two rooms. Each transition is
one file, named by its door type in two hex digits. A room's door says
which type it plays.

```text
door/00.dor
door/16.dor
door/1f.do2
```

There are two formats: the RE 1 format, `.dor`, and the RE 2 format,
`.do2`. A pack's doors are in the format its rooms are.

To watch one transition without walking up to it:

```sh
arklay re2.akpak --door 1f --ticks 30 --capture door.bmp
```

## RE 1 format: `.dor`

A `.dor` is all in one file: a small bytecode program per phase of the
animation, a TMD mesh of the moving parts and a 128x256 8-bit texture with
one 256-colour palette. The header is three little-endian 32-bit offsets
into the file:

```text
0x00  the script table: offsets relative to the table, ending with 0
0x04  the TMD mesh
0x08  the 8-bit TIM texture
```

Script 0 is the main script, and it starts the others.

Some types play as more than a door:

| Types | Plays as |
|---|---|
| `16`-`19` | a staircase |
| `1a`, `1b` | a ladder |

A type past `21` plays `door/00.dor`.

## RE 2 format: `.do2`

A `.do2` carries its own sounds and animates up to ten parts with RE 2
format script procedures:

```text
0x00  four 4-byte sound entries (0xFFFFFFFF is none)
0x10  a VAB sound bank
base  two offsets (the MD1 mesh and the TIM), then the procedures
```

## Replacing a door

A mod replaces a door by shipping a whole file at the same path: there is
no way to replace only the mesh or only the texture. For a higher-resolution
texture, an HD texture pack can replace the door's texture page by its hash
(see [HD texture packs](../mods.md#hd-texture-packs)).
