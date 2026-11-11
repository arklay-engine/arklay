# Rooms

`room/` holds one folder per room, named by the room id. Everything the
room needs is in its folder and found by its name, so nothing in a room
refers to a path: copying or renaming a folder moves the whole room to
another id or another pack.

A room id is the stage, then the room in the stage as two hex digits:
`room/100/` is stage 1's room 0, `room/10b/` stage 1's room 11. The stage
is a digit `1` to `7` or a letter `a` to `g`. An id may carry a fourth
character after the room, for packs that keep a room per scenario
(`room/1000/`).

One folder holds every **variant** of the room: each player's game and each
visit. What differs between variants is said in `room.xml` (see
[Variants](#variants)), not by more folders.

## The room folder

```text
room/100/
├── room.xml           the room: lights, cameras, masks, zones, collision, text
├── room.scd           the room's scripts, compiled
├── cut/00.png         camera 0's background
├── mask/00.bmp        camera 0's mask page
├── object/00.tmd      object model 00, with its texture object/00.bmp
├── item/00.tmd        item model 00, with its texture item/00.bmp
├── effect/00.xml      effect sprite 00, with its sheet effect/00.bmp
├── anim/00.emr        a player animation set, with anim/00.edd
└── anim/01.emr
```

Cameras and files are numbered by two hex digits from zero: `cut/0a` is
camera 10.

| File | What | Formats |
|---|---|---|
| `room.xml` | the room definition (below) | XML |
| `room.scd`, `room.akscr` | the room's scripts (see [Scripts](#scripts)) | SCD, `.akscr` |
| `cut/<cam>` | the camera's background | `webp`, `png`, `bmp`, `tim`, `pix` |
| `mask/<cam>` | the camera's mask page: the parts of the background drawn over the 3D scene; black is see-through | `webp`, `png`, `bmp`, `tim`, `pix` |
| `scroll/<cam>` | a scrolling background, for a camera that has one | `webp`, `png`, `bmp`, `tim` |
| `object/<nn>.tmd`, `object/<nn>` | the room's object models and their texture pages | TMD; indexed `bmp`, `tim` |
| `item/<nn>.tmd`, `item/<nn>` | the models of the items lying in the room, and their pages | TMD; indexed `bmp`, `tim` |
| `effect/<nn>.xml`, `effect/<nn>` | the room's effect sprites (see [Effects](effects.md#a-rooms-effect-sprites)) | XML; `bmp`, `tim` |
| `anim/<nn>.emr`, `.edd` | the player's animations for the room's own actions: pushing, climbing | EMR, EDD |
| `sound.edh`, `sound.vb` | the room's sound table and sound bank | EDH, VB |
| `esp.bin` | the room's effect sprites, in the RE 2 format | |

A camera background may be any size: it is stretched over the 320x240
frame, so a 640x480 image is a 2x background. A mask page keeps its size.

A room may instead be a single RDT file: `room/<id>/room.rdt`, or
one per variant, named for it (`room/<id>/jill-mansion2.rdt`,
`room/<id>/chris.rdt`). When both a `room.xml` and an RDT define the room,
the one from the higher layer wins, and the XML wins a tie.

An RE 2 pack also keeps, under `heap/`, the files the original loads into its
work memory, as they are: each room's RDT (`heap/<id>.rdt`), each enemy's TIM
(`heap/em<s><id>.tim`) and the music banks (`heap/main<nn>.bgm`,
`heap/sub_<nn>.bgm`). The engine reads them only to know what a new enemy's
memory holds before its init sets it (see "What a zombie's slot held before"
in `docs/internals/deviations/re2.md`); a pack without them plays the same,
except that those leftover values start at 0.

## The room definition

`room.xml` is the room as the engine reads it: hand-edit it to move a
light, retime a message or reshape a collision box. Every element defines
every variant at once:

```xml
<room format="3">
  <ambient r="896" g="976" b="656"/>
  <lights>
    <light x="4680" y="-2222" z="6360" r="100" g="100" b="100" kind="0" radius="7000"/>
  </lights>
  <cameras>
    <camera x="3960" y="-3132" z="11538" lookX="5274" lookY="-2430" lookZ="5652" roll="0" fov="221">
      <maskGroup originX="-119" originY="-55" texBits="0x7800">
        <sprite u="0" v="0" x="128" y="128" depthData="380" blend="0" colorMode="1" width="8" height="40"/>
      </maskGroup>
    </camera>
  </cameras>
  <cameraSwitches>
    <switches from="0">
      <view zoneIndex="0" to="9" x0="1100" z0="893" x1="600" z1="10702" x2="9807" z2="10600" x3="9685" z3="900"/>
      <switch zoneIndex="1" to="1" x0="9442" z0="7099" x1="1000" z1="6514" x2="785" z2="7657" x3="9285" z3="7699"/>
    </switches>
  </cameraSwitches>
  <collision cellX="5188" cellZ="6038">
    <quadrant index="0">
      <box xMax="10864" zMax="12330" xMin="8490" zMin="4" shape="1" cf1="true" cf2="true"/>
    </quadrant>
  </collision>
  <walkZones>
    <walk x1="1700" z1="1800" x2="8000" z2="8100" field8="0x03FF" links=""/>
  </walkZones>
  <footsteps count="1">
    <footstep x="900" z="1100" width="8700" height="0x2648" sound="0x002D"/>
  </footsteps>
  <slots>...</slots>
  <strings>...</strings>
</room>
```

| Element | What |
|---|---|
| `<ambient>` | the room's ambient light colour |
| `<lights>` | the point lights |
| `<cameras>` | each camera's position, target and field of view, with its mask sprites: rectangles of the mask page drawn over the scene at a depth |
| `<cameraSwitches>` | per camera, the floor zones that switch to another camera (`to`) when the player walks in |
| `<collision>` | the walls and obstacles, in four quadrants around a centre |
| `<walkZones>` | the zones characters find their way through, with the zones each connects to |
| `<footsteps>` | the floor areas and the footstep sound each plays |
| `<slots>` | which model, effect and animation files each variant uses (see [Slots](#slots)) |
| `<strings>` | the room's name and messages, per language (see [Text](#text)) |

Numbers are decimal or `0x` hex. An unknown element or attribute is
refused, not ignored. Packed flag words are split into named attributes:

- a collision `<box>` has its `shape`, a signed `step` height (a multiple
  of 100), the collision flags `cf1`, `cf2`, `cf4` and `cf8` (the bits a
  room script can toggle) and any other bits as `otherBits`;
- a mask `<sprite>` has its texture `page`, `blend`, `colorMode` and
  `otherBits`, and a square `size` or a `width` and `height`;
- a `<walk>` zone lists the zones it connects to as `links="1 7"`.

The camera switch zones are grouped by camera, but each has a flat
`zoneIndex`, the number the scripts use. The indices count up from 0 with
no gaps.

The `<room>` element may carry `game="re2"` for a room in the RE 2 format,
whose cameras carry their own lighting.

### Variants

An element only some variants have carries `when`: terms separated by
spaces that must all hold, each `CHRIS`, `JILL`, `MANSION1` or `MANSION2`,
with `!` before a term to negate it. `CHRIS` and `JILL` are player 0's and
player 1's game; `MANSION1` and `MANSION2` the first and second visit.

```xml
<ambient when="MANSION1" r="696" g="680" b="504"/>
<ambient when="MANSION2" r="888" g="888" b="888"/>
<walkZones>
  <walk when="MANSION2" x1="7000" z1="11600" x2="11000" z2="15600" field8="0x0041" links="1 2"/>
</walkZones>
```

The room is read for the variant being entered, and an element whose
`when` fails is not there. A list (lights, walk zones, camera switches,
footsteps) is the elements that hold, in order.

### Slots

The room's object and item models, effect sprites and player animation are
used by **slot**: the scripts name an item model by its slot, and the
room's effects are its effect slots. Slot `n` reads file `n` of its folder
(`item/03.tmd` for slot 3), unless `<slots>` names other files.

Where the variants' files differ, each different file has its own number,
and `<slots>` lists the file each slot of a variant reads, with `-` for an
empty slot:

```xml
<slots>
  <object when="MANSION2">01</object>
  <object>00</object>
  <item when="JILL">03 01 02</item>
  <item>00 01 02</item>
  <effect when="MANSION2"/>
  <anim when="JILL">01</anim>
  <anim>00</anim>
</slots>
```

Here player 1's item slot 0 reads `item/03` where player 0's reads
`item/00`, the second visit has no effects, and player 1's animation set is
`anim/01`. The first element of a kind whose `when` holds gives that kind's
whole list, so one without `when` after the others is the default. A kind
with no element reads every file of its folder in order. Without
`<slots>`, models are read from `00` up, and effects from `00` up to the
first missing `.xml`.

When the room declares more object or item slots than it has files, the
`<room>` element carries `objectSlots` or `itemSlots`, the most any variant
declares.

### Text

The room's text is last in `room.xml`: a `<language lang>` per language,
with the room's `name` (in the debug menu and the save list), its
`save_name` (the save screen's short label), and then the messages, each
keyed by its message id in decimal and written as in the language files:

```xml
<strings>
  <language lang="en-US">
    <string key="name">SAVE ROOM</string>
    <string key="save_name">M. Room 1F</string>
    <string key="0">{scroll 0}\n{scroll 2}{timed 70}</string>
    <string key="1" when="JILL">{scroll 0}\n{scroll 2}{timed 105}</string>
    <string key="42">\nVitamins and serums.{scroll 2}</string>
  </language>
</strings>
```

A message's subtitle speaker goes beside it as `<id>_speaker`, or
`<id>_speaker_<page>` for one page.

They become the language keys `ROOM_<room>_NAME`, `ROOM_<room>_NAME_SAVE`,
`ROOM_<room>_TEXT_<nn>` and `ROOM_<room>_TEXT_<nn>_SPEAKER[_<page>]`, with
`nn` in hex (message `42` is `TEXT_2A`). A string with a `when` (one player,
one visit, or both) is that variant's text, keyed with the variant after the
room: `ROOM_100_JILL_TEXT_01`, `ROOM_100_MANSION2_TEXT_01`,
`ROOM_100_JILL_MANSION2_TEXT_01`. A variant reads the most specific key that
exists. A language file's key still wins over the room's (see
[Language](language.md)).

## Scripts

A room's scripts set it up when the player enters, run every frame while
the player is in it, and drive its events: doors, items, cut scenes,
enemies. They are one file in the room's folder:

| File | What |
|---|---|
| `room.scd` | the scripts as bytecode. A mod source directory may hold the script as text, `room.akscr`, and `arklay mod build` compiles it into `room.scd` (RE 1 format). |
| `room.akscr` | the scripts as text; the engine compiles it when the room loads (RE 2 format). |

A room's scripts are replaced as a whole: a mod that ships one replaces all
of the room's init, per-frame and event scripts, and a mod that changes only
`room.xml` keeps the scripts below it. A room loaded from an RDT runs the
RDT's own scripts unless a `room.scd` comes from the same layer or a higher
one.

The language is documented in [Room scripts](../scripting/room/index.md).

## HD textures

An HD texture pack can name a room's model page by path instead of by
hash: `hires/room/<id>/object/<slot>` or `hires/room/<id>/item/<slot>`,
covering the whole page (see [HD texture packs](../mods.md#hd-texture-packs)).
