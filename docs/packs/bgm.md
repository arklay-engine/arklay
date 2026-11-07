# BGM

`bgm/` holds the music tracks. Each track is one file named by its **track
id**, three lower-case hex digits:

```text
bgm/000.ogg     track 0x000
bgm/013.ogg     track 0x013
bgm/101.ogg     track 0x101
```

Formats, highest first: `ogg`, `wav`. A track may be any length and sample
rate. Rooms and scripts only ever name a track by its id.

## Track ids

The id also says what kind of sound the track is, which decides the volume
setting it follows:

| Ids | Kind | Volume |
|---|---|---|
| `000`-`0ff`, `200`-`fff` | music | music |
| `100`-`1ff` | a background loop: rain, machinery | `ambient_volume` |
| `118` | a spoken line played as a track | voice |

## Room music

Which tracks a room plays is set in `game/music.lua`, which runs once when
a game session starts:

```lua
-- game/music.lua
local api = ...

-- A group: up to three tracks played together, each with a loop flag.
api:set_music_group(0x0B, {
    { 0x013, true },          -- bgm/013, looping
})
api:set_music_group(0x08, {
    { 0x00a, true },
    { 0x00b, false },         -- plays once
    { 0x105, true },          -- a background loop under the music
})

-- A room: up to four groups. Stage 1, room 0x00 is room 100.
api:set_room_music(1, 0x00, { 0x0B })
-- The same room on its second visit.
api:set_room_music(1, 0x00, { 0x0F, 0x0A }, { mansion2 = true })
```

A group holds up to three tracks, one per music channel. A room holds up to
four groups, and the room's state byte picks one: its low three bits are
the group, and the room's scripts change it as the story goes on. A new
game's state bytes are written by `game/initial_state.lua` (see
[Scripting](scripting.md#the-new-game-state)) with
`api:set_room_music_state(stage, room, state)`.

`game/music.lua` replaces the whole file: a mod that ships one restates
every group and room it wants to keep.

A looping track loops the whole file.
