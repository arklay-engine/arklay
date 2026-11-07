# Room 506, the lab computer against the reference

The room-5060 terminal (`display_computer_lab`, `ComputerLab.cpp`, with the
two forearm models of the `ComputerArms` enemy) is `src/computer_lab.rs`. It
runs inside the game loop where the reference runs
`check_and_display_interactive_screen`: after the room scripts, before
`check_camera_switch(0)` and the entity update. The world keeps ticking under
it, so the arms, the camera switches and the random stream are compared
through the trace as well as through the frames.

## Scenarios

`scripts/oracle-ui.sh --save oracle/saves/computer_<chris|jill>_506.dat
oracle/inputs/computer_<scenario>.txt 3600 OUT 20`, for both characters:

| Scenario | Trace | Frames |
|---|---|---|
| `computer_unlock` | identical, 3600 ticks | byte-identical except the two rows below |
| `computer_reject` | identical, 3600 ticks | the same |
| `computer_cancel` | identical, 3600 ticks | the same |

Jill's hands type every other character at fixed gaps (13 or 6 frames),
Chris's at a random gap (`(rand() & 0xf) + 10`), so the same input takes Jill
through the credentials-again path at B2. Both match.

## Differences left

- **Logo wedges.** During the boot logo (t260 to t400, and each later boot)
  up to 1.1% of the frame differs, along the edges of the umbrella's wedges.
  They are perspective quads, sampled bilinearly and blended by texel alpha
  on both sides; the reference's GL rasteriser and `arklay`'s software one
  round the edge coverage differently. The colours inside the wedges match.
- **The examine message** before the terminal opens (t20 to t140) is the text
  font difference in `room-303-parity.md`, not the terminal.
- **GPU renderer.** `--renderer gpu` draws the wedges nearest-sampled; the
  software renderer is the one compared.

## Port notes

- **Zone group.** The terminal's forced `check_camera_switch(1)` to cameras 4
  and 5, which have no switch-zone groups, leaves the reference's
  `g_CurrentRdtDataTypePtr` on the previous camera's group, and the arms'
  zone test reads it. `RoomState::zone_group` keeps that group; without it
  the forearms are culled.
- **Cancel.** The terminal reads cancel off the raw pad bit `0x0040`, which
  run shares. `arklay` binds cancel and run to separate keys, so the terminal
  folds the cancel key into that bit (`Terminal::pad_from_dpad`).
- **Masked characters.** The password and command fields print `*` with
  `PrintText8x8(.., 0xf0, 0)`: brightness nibble 15 draws the glyph at
  15 * 255 / 30 = 127, half grey.
