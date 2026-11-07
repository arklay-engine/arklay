# Effects

Effects are the 2D sprites drawn in the 3D scene: fire, sparks, blood,
steam, muzzle flashes. Three places in a pack hold them:

| Where | What |
|---|---|
| `effect/<nn>` | the effect sheets: the texture pages the sprites are drawn from |
| `room/<id>/effect/<slot>.xml` and `.bmp` | a room's own effect sprites |
| `data/core00.esp`, `data/core00.etm` | the effect sprites every room shares |

## Effect sheets

```text
effect/00.bmp
effect/01.bmp
...
effect/20.bmp
```

Each sheet is an indexed page (see [BMP pages](index.md#bmp-pages)).
Formats, highest first: `webp`, `png`, `bmp`, `tim`. A room's sprites are
drawn onto the sheet at their own rows when the room loads, so the sheet on
screen is the sheet with the room's sprites over it. A replacement sheet
keeps its size; for higher-resolution art, use the HD names below.

## A room's effect sprites

A room declares up to eight effect sprites, its **effect slots**. Slot `n`
is the pair `room/<id>/effect/<nn>.xml` (the sprite's frames and animation)
and `room/<id>/effect/<nn>.bmp` (its pixels):

```text
room/100/effect/00.xml
room/100/effect/00.bmp
room/100/effect/01.xml
room/100/effect/01.bmp
```

Slots are read from `00` up to the first missing `.xml`. A room's `<slots>`
element can give a variant other files (see [Rooms](rooms.md#slots)).

The `.bmp` is an 8-bit sheet of 16-colour rows: pixel values `0` to `15`,
one 16-colour palette row per palette row of the sprite, magenta for entry
0 and each entry's reserved byte its semi-transparency bit. A `.tim` may
stand in for it.

The `.xml` describes the sprite:

```xml
<effect index="38" clut="0x7840" depthTable="2 10 0 0 0 0 0 0">
  <frame uv="0" delay="1" width="24" height="24"/>
  <frame uv="4" delay="1" width="24" height="24"/>
  <frame uv="0" delay="0" width="0" height="0"/>
  <uv u="0" v="0" pivotX="116" pivotY="116"/>
  <uv u="24" v="0" pivotX="116" pivotY="116"/>
  <depth row="0">
    <animFrame>
      <block>010001140000000000000000000003400000000000000000</block>
    </animFrame>
  </depth>
</effect>
```

| Element | Meaning |
|---|---|
| `<effect index>` | the sprite id the room scripts spawn it by |
| `<effect clut>` | the texture page and palette word |
| `<effect depthTable>` | eight numbers, one per depth row |
| `<frame>` | one frame of the sprite: which `<uv>` rectangle, for how many ticks (`delay`), and its size; a frame of size 0 ends the list |
| `<uv>` | a rectangle on the sheet: its top-left `u`, `v` and its pivot |
| `<depth row>` | the animation of one depth row: a list of `<animFrame>`s |
| `<block>` | one 24-byte behaviour block of an animation frame, in hex |

An `<effect index>` without children is a declared slot with no sprite.
Unknown elements and attributes are refused.

## Shared effects

`data/core00.esp` holds the eight effect sprites every room shares, such as
muzzle flashes and hits, in the same layout as a room's sprites, and
`data/core00.etm` their textures.

A pack whose rooms are in the RE 2 format keeps its effects as one file per
set instead: each room's is `room/<id>/esp.bin`, and the shared set is
`esp/core.bin`.

## HD effects

The HD texture pack names for effects (see
[HD texture packs](../mods.md#hd-texture-packs)) can replace a whole sheet
or one sprite at a time:

| Name | Replaces |
|---|---|
| `hires/effect/<nn>` | sheet `nn`, under the room's sprites |
| `hires/room/<id>/effect/<slot>` | the rows of sprite `slot` |
| `hires/room/<id>/effect/<slot>_<row>` | those rows in palette row `row` (1 to 3) |

The GPU renderer builds the HD page at the largest scale of the images it
finds. A page with no named image is still replaced by its content hash.
