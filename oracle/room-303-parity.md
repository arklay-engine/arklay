# Room 303, footsteps and head bursts against the reference

Differences left after comparing `arklay` with the C++ reference
(`scripts/oracle-ui.sh`, `scripts/oracle-sweep.sh`) tick for tick and pixel for
pixel. The reference is a port, so each entry says which side matches
`Bio.exe` and which side `arklay` keeps.

## The reference differs from `Bio.exe`; `arklay` follows `Bio.exe`

- **Effect blend.** The reference's `ps1Blend` is a port-only PS1 mode;
  `arklay` keeps the 1997 band-table rule. Compare with `EFFECT_BLEND_PC` forced
  in the reference, or room 100's smoke differs on 24% of a frame.
- **Screen shake.** The reference moved `ApplyScreenShake` before the task pass
  and reads the flag live when it draws, so it shows a stale roll on the frame
  the flag rises and none on the frame it falls. The original rolls at the end
  of the frame (`0x004297f6`), which `arklay` follows. Room 700's shake frames
  differ only at those two edges.
- **Gore strips.** Colour scale and UVs come from the worker at `0x4399b0`
  (`1/128`, `u/256`, first material record, `0x439b8f..0x439ba6`). The
  reference tuned them.
- **`cmd_effect_flags_modify`.** The reference's walk overruns the effect pool
  into `g_EnemiesList`, setting bit 31 of entity 1's Z. The trace comparison has
  to mask that bit. `arklay` does not copy it.
- **Text.** `arklay` packs `PBFONT0/1/3N.TIM`; the reference draws `FONT.TIM`.
- **Viewer backdrop.** Navy, as the original; the reference clears to black.
- **Save screen.** The reference's descriptions and `01/16` pages are its own.
- **FILE reader backdrop.** The reference draws variant-2 menu sprites additively
  (`SPRITE_FLAG_ADDITIVE`, port-only); the original's single blend averages them,
  as `arklay` does. Clearing the bit in the reference with `gdb` takes the
  `file_open` scenario's worst frame from 14.80% of pixels to 0.40%.

## `arklay` follows the reference, not `Bio.exe`

- **Map.** The map screen's additive floor plan is the same port-only blend, and
  `games/re1/ui/map.lua` keeps it, so the map and the FILE reader disagree about
  which blend they follow. The map's frames differ by about 33 pixels. The
  opening zoom's grid lines differ at t130 and t135.
- **Slash ribbon.** The original has no registered texture for it and the
  reference's `tyrant_trail_submit` is its reconstruction of `FUN_004865a0`.
  `render::ribbon_polys` mirrors that reconstruction: red at `tint/128`, alpha
  0.5, keyed at the mean view depth.

## `arklay` differs from the reference and from `Bio.exe`

- **Undefined reads.** The Tyrant's scripted puffs read joint 15 past the array;
  `arklay` uses the entity origin. A footstep off every table plays whatever the
  stale column holds in the reference and nothing in `arklay`.
- **Pan height.** `Calc3DSndPan`'s `dy` reads a matrix only the menu and death
  screen set, so it is 0 in play. `arklay` measures from the floor.
- **Zone walk.** An enemy outside every camera zone (zone `0xFF`) picks another
  path than the reference's stale read: rooms 11A and 300 with dogs off the map.

## Found by riding the lift

Placed saves never armed the self-destruct, so three bugs only showed on the
real route (room 501's battery, the lift through 50D, then 303):

- **The clock.** `arklay` had the flag and the seconds word but no clock. It now
  counts, draws the green `" 2:59:91"` text, turns red for the last minute, and at
  180 seconds drops the frame, blanks the screen, plays film 2 and opens DIED with
  no body. `Biohazard.exe` `0x48d00d` pushes the text's `y` `0x20` and `x` `0x68`.
- **A frame lost per menu.** `arklay` opened a menu ahead of the room tick, so each
  press lost one tick of scripts and entities. The press frame now runs whole with
  the player's message bits cleared, and the menu opens on the next frame.
- **The handback blend.** A scripted motion left the player's blend step at
  `0x200`, so the breathe it hands back to blended for 7 ticks instead of 3.

## Found by dying

An ordinary death had not been compared: the placed routes all survive, and the
explosion leaves no body. Letting room 101's zombie kill a player at 5 health
(`dn`, 592 ticks against the reference) showed:

- **Screen timing.** The machine reads the fade before `main_loop` steps it, so the
  frame after the white fade wraps runs the room and only then does `die_state` start.
  `arklay` opened the screen a frame early, and missed the original's two
  fallthroughs (flash to reveal, arm to re-wave), which cancelled out in the middle
  and not at either end.
- **The flash and fade-out.** The screen published the post-step fade; the original
  draws the pre-step value. That was all of the backdrop's 4 levels of radial banding.
- **The last frame.** One white frame showed after the fade-out finished, from the
  stale pre-screen fade. `die_state`'s last frame has no sprites queued, so it is black.
- **The black rect.** It darkens the backdrop only. `arklay` drew it over the body.
- **The wound.** `JointApplyColorTint` on the player was never modelled, and the
  Lua of the zombie and Cerberus tinted the attacker instead. All the sites now tint
  the player's joints (zombie kill and feed, Hunter, Plant 42, Tyrant, Cerberus, Yawn,
  grabbed), and the DIED screen's corpse draws them too.
- **The feed's two sprays.** Two blood billboards at the player's joints 0 and 2 at
  the zombie's feed frame `0x0E`.
- **The head.** `arklay` set the grab one-shot at boot on the strength of a comment
  about the image value. `Biohazard.exe`'s `0x4cc750` is zero in the image, so the
  first corpse kept its head.
- **The corpse's lights.** `display_die_screen` loads `died.tim` into `0xc75c90`, the
  buffer the RDT pointer `0xc3abac` also points at (`0x43d423`), so
  `update_entity_lighting` reads the lights from the TIM's first bytes. The result
  is the dim pink body: three directional lights coloured `(9,0,10)`, `(3,0,3)` and
  `(33,4,33)`. `arklay` does the same overwrite when the screen opens.

## Found by picking up the launcher

The route that takes room 303's launcher (`pk`, 3760 ticks) carried 32 trace
differences. All were real, and all came from one mistake of order or one late
draw:

- **Viewer and message order.** The menu task runs before `main_loop`'s
  `UpdateMessageDisplay`. `arklay` ran them the other way round in the pick-up
  viewer, so the prompt's first glyph came a frame late and the closing animation
  started a frame early. The viewer now steps first. It sees the message clear on
  the next frame, and the answer's cue and the award move with it.
- **Frame seed.** The menu's entry `rand()` ran when the viewer first ticked, 19
  frames late, so the traced stream differed until then. It now runs on the frame
  the request reaches the main loop.
- **Counter write.** The opening fade's counter was written two frames before the
  original's `set_fading`. It had no visual effect, but the trace showed it.
- **Yes/No row.** The frame the text stream reaches the prompt only draws the text
  so far. The row now starts on the next frame, as state 4 does.
- **Item cursor.** The pad came back a frame early after Equip, so the cursor turned
  bright a frame early. It now returns a frame after the box is gone.

## Still different, not understood or not fixed

- **Monster Plant (rooms 10C, 40C).** `e.status` turns 5 ticks apart; the states
  flip afterwards.
- **Cerberus in 304, Yawn segments in 210, room 310's door camera, 513's repeated
  tick.** Each diverges late in a 620-tick idle run.
- **Clock digits.** Same glyphs and places; the shades differ with the font page
  (see the font entry above).
- **After RESULT.** The hand-over back to the title runs 5 to 10 ticks apart.
- **Effect zone dust.** See item 3 of `docs/internals/deviations/m13.md`.
- **The corpse's edges.** About 150 isolated edge pixels of the corpse differ on the
  DIED screen; the pose, colours and lights match.
- **Before the title.** The reference waits in `die_state` on `g_SndFadeType`, which
  never clears in its test mode, so the hand-over to the title after DIED is not
  compared; `arklay` goes on the next frame.
