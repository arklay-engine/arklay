# Enemies

`enemy/` holds every enemy and every character other than the player: one
folder per entity id, two hex digits. What each id is, is up to the pack:
a room places an entity by its id, and the folder of that id says what it
looks like, how it behaves and what it sounds like.

```text
enemy/00/
├── enemy.lua             its behaviour
├── enemy_0.emd           its model in player 0's game
├── enemy_1.emd           its model in player 1's game
└── sound/
    ├── 00_00.wav         sound slot 00, default style
    ├── 00_01.wav
    └── 01_04.wav         slot 04 in style 01
enemy/20/
├── enemy.lua
├── enemy.emd             one model for every game
└── weapon/
    └── 01.tmd            the weapon it holds in behaviour 01
```

An id whose folder has no script stands still, and one with no model is
not drawn. A folder is self-contained, so copying it to another id or
another pack moves the whole entity.

## Models

| File | Meaning |
|---|---|
| `enemy_<n>.emd` | the model in player `n`'s game (`0` or `1`) |
| `enemy.emd` | the model in every game; read when there is no `enemy_<n>.emd` |
| `enemy_<n>.bmp`, `enemy_<n>_clut_<nn>.bmp` | the texture page of a model that keeps its texture apart, and one palette per extra palette row |

An EMD holds the mesh, the skeleton, the animations and, in the RE 1
format, the texture. The engine reads both the RE 1 and the RE 2 format.

A mod replaces a model by shipping a whole file. An HD texture pack can
replace the texture alone by its hash (see
[HD texture packs](../mods.md#hd-texture-packs)).

To look at a model without playing up to it, the model viewer stands it in
a room:

```sh
arklay re1.akpak --room 100 --model enemy/06/enemy_0.emd --clip 3
```

## The script

`enemy.lua` is the behaviour. The engine calls its global `update(e)` once
per tick for each live entity of that id:

```lua
-- enemy/13/enemy.lua
function update(e)
    if e.state == 0 then
        e.state = 1
        e.health = 0x37
        e:reset_joints()
    end
    e:advance_anim(0x400)
end
```

Entities that share behaviour share a module under `lib/`, and each id's
script passes only its own numbers:

```lua
-- enemy/00/enemy.lua
local zombie = require("/lib/zombie")

sounds = { styles = zombie.SOUND_STYLES }

function update(e)
    zombie.run(e, { radius = 422 })
end
```

What `e` can do is in the [Enemy scripts](../scripting/lua/enemy.md) and
[RE 2 enemy scripts](../scripting/lua/re2-enemy.md) references, one for each
model format.

## Sounds

An entity's sounds are in its `sound/` folder, named `<style>_<slot>`, two
hex digits each. Formats: `ogg`, `wav`. The script plays one with
`e:play_enemy_sound(slot)`.

A **style** is a set of variations. Style `00` is the default, and another
style ships only the slots it changes: a missing slot falls back to `00`.

```text
enemy/00/sound/00_04.wav    slot 04
enemy/00/sound/01_04.wav    slot 04 in style 01
enemy/00/sound/01_05.wav    slot 05 in style 01
```

The script's global `sounds` declaration picks the style per room and can
borrow another folder's sounds:

| Declaration | Meaning |
|---|---|
| `sounds = "00"` | after its own folder, look in `enemy/00/sound/` |
| `sounds = { base = "00", styles = { [0x103] = 0x02 } }` | the same, and play style `02` in room `103` |
| `sounds = "/shared/beast"` | after its own folder, look in `shared/beast/sound/` |

A borrowed folder's own `sounds` declaration is followed in turn, up to
eight folders. The first file found plays; none is silence.

## Held weapons

A character can hold a weapon in a scene: `enemy/<id>/weapon/<nn>.tmd`,
where `nn` is the character's behaviour byte. The mesh takes its texture
from the character's own page. A behaviour with no file holds nothing.
