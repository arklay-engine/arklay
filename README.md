# Arklay

A from-scratch engine for the classic Resident Evil games (RE1, RE2, RE3),
written in Rust with SDL3. Each game is a game pack; everything game-specific
lives in the pack. Mods layer on top with any asset overridable.

The engine reads the player's own game installation: `convert-game` migrates a
supported install into an `.akpak` pack, and the engine runs the pack. A pack
may also be a directory of unpacked entries (the layout `arklay extract`
writes); directory entries are read on demand, not loaded wholesale.

## Build

Requires Rust stable and SDL3. If SDL3 is not installed system-wide, set
`SDL3_DIR` to a prefix containing `include/` and `lib/`, or place one in
`../vendor/sdl3` (the workspace default). On Windows, SDL3 is built from source
and linked statically, so no separate DLL is needed.

```sh
cargo build
# A lean build without the PNG/WebP/OGG replacement decoders
cargo build --no-default-features
```

Lua 5.4 is vendored and compiled from source on every build.
The `hd` feature (also default) pulls the pure-Rust `png`, `image-webp` and
`lewton` crates that decode a mod's replacement art and audio; without it the
engine still resolves replacements by stem but refuses to decode those formats.

## Launcher

Running `arklay` with no arguments opens the startup launcher. Naming a pack
on the command line skips it and boots the game directly. The launcher finds
Steam's RE1 and RE2 installs, creates or refreshes `games/re1.akpak` and
`games/re2.akpak`, plays a pack in the same window (closing the game window
ends the process), discovers mods across the data roots and remembers their
enable/disable state and order, and checks the release and nightly update
tracks. Its screens are drawn by the same UI as the in-game menus, in the
face and text size set in the options, with the button hints in its language. INSTALL / REBUILD lists the
games with the state of each pack and opens the chosen game's install; picking
an RE2 folder with that screen's folder picker sets up RE2. UPDATE has a
REBUILD row for Resident Evil and for each other game with a pack or an
install. OPTIONS is the in-game
options screen itself, key configuration included, writing every change back
to `arklay.toml` (the renderer settings take effect when a game starts). MODS
lists every discovered mod, whatever its game, in application order: the top
row is applied first (lowest priority) and the bottom row last (highest);
Ctrl+Down moves a mod toward higher priority.

The launcher also takes a gamepad. The D-pad or left stick moves and changes
UPDATE values left and right; A/Start confirms; B/Back backs out, quitting
from the main menu and asking first during an install. On MODS, X opens the
folder and LB/RB moves the selected mod toward lower/higher priority; on a
game's install, X opens the source folder picker. OPTIONS takes the in-game
menu's keys and buttons.

Updates publish a small `build.json` next to the platform binary. `release`
follows the latest normal release; `nightly` follows the rolling pre-release
that CI replaces on every push to `main`. The launcher verifies the download's
SHA-256, swaps the executable while keeping `<exe>.old`, and restarts; the
Update screen can restore that previous image. Only CI builds (with
`ARKLAY_CHANNEL` baked in) update themselves, so a local `cargo build` shows
`Dev build` instead. The Linux binary expects SDL3 to be installed on the
system; Windows links SDL3 statically.

Data resolves by precedence:

- configuration and saves: `%APPDATA%\arklay` on Windows,
  `$XDG_CONFIG_HOME/arklay` on Linux; an existing `arklay.toml` in the local
  data directory takes over instead
- games, mods, logs and `F12` screenshots: `%LOCALAPPDATA%\arklay`
  (`$XDG_DATA_HOME/arklay`)
- `ARKLAY_DATA=C:\one;C:\two` replaces the data-root list entirely, with the
  first path chosen for new files; otherwise the order is the local data
  directory, the roaming directory, then the executable's directory
- `ARKLAY_CONFIG_DIR` relocates configuration and saves

The menus' retro face is [Departure Mono](https://github.com/rektdeckard/departure-mono),
MIT licensed; the notice is in `assets/DepartureMono-LICENSE`. Their modern
face is [Barlow](https://github.com/jpt/barlow), under the SIL
Open Font License; the notice is in `assets/Barlow-LICENSE`.

## Usage

```sh
# Convert an RE1 installation into a game pack (parallel across CPUs)
cargo run -- convert-game /path/to/re1 --out re1.akpak

# The same, with one line per packed file: its plain and stored size
cargo run -- convert-game /path/to/re1 --out re1.akpak --verbose

# Limit the conversion to N workers (default: one per available CPU)
cargo run -- convert-game /path/to/re1 --out re1.akpak --jobs 4

# Convert the Steam release's install root: the walk finds the language trees
# (english/USA, french/FRA, german/GER, japanese/JPN), takes the shared assets
# from the first tree (English), fills in the Japanese-only document pages from
# the other trees, resolves each release's film names to the canonical entries,
# and packs every game-text sheet it carries, so the one pack renders
# English/French/German with the original 8px Latin glyphs and `ja-JP` with the
# Japanese sheet
cargo run -- convert-game /path/to/steam/4249100_Biohazard --out re1.akpak

# Convert the RE2 PC release (the directory holding data/Pl0, data/Pl1 and
# data/Common) into re2.akpak: rooms, backgrounds, models, audio and films.
# The engine shows RE2 rooms (backgrounds, cameras and collision) and poses any
# RE2 EMD, PLD or PLW with --model, but has no RE2 player or scripts yet (see
# issue #244); --game re2 skips detection
cargo run -- convert-game /path/to/re2

# Convert a Classic REbirth mod (.zip or directory: HD texture packs, BioRand
# seeds, ...) into a mod pack in the per-user mods directory; see
# docs/modding.md for the mapping
cargo run -- convert-mod RE-ENHANCE_RE1_v2.0_GOG.zip

# List every pack entry with its size, then the entry count and total bytes
cargo run -- list re1.akpak

# Validate a pack: parse every known-format entry and print a per-format
# report; exits 1 when a known-format entry fails (--strict also fails
# unknown extensions)
cargo run -- verify re1.akpak

# Extract every pack entry below a directory, preserving relative paths
cargo run -- extract re1.akpak --out extracted/

# Launch room 100 as player 0 (RDT 1000); the pack may also be a directory
cargo run -- re1.akpak --room 100 --player 0
cargo run -- extracted/ --room 100 --player 0

# Pick the displayed language; the default is the OS locale, then en-US
cargo run -- re1.akpak --room 100 --language ja-JP

# Boot a screen directly (--ui title|select|game|demo|menu|box|file|map|view|save|load|
# death|result|epilogue|font|debug)
cargo run -- re1.akpak

# Play one attract reel directly (the reference's --test-demo oracle anchor):
# --demo N is the Nth entry of the pack's sorted demo/<hex>.dat list; with
# --ticks/--trace it emits the per-tick differential trace
cargo run -- re1.akpak --ui demo --demo 0 --ticks 600 --trace demo0.trace

# `--ui game` with `--room` boots the new game straight into that room through
# the session flow (boot narration, entry fade, camera and idle mirroring), the
# anchor the reference oracle's test-only room boot mirrors
cargo run -- re1.akpak --ui game --room 101 --player 0 --ticks 600 \
  --trace room101.trace

# Load a save block (savedatN.dat) and boot its saved game immediately: the
# file's stage, room, character, position, health, inventory and flags drive
# the session. `--ui game --load` is the oracle anchor; without `--ui` it is
# the interactive continue
cargo run -- re1.akpak --ui game --load savedat1.dat --ticks 600 \
  --trace loaded.trace
cargo run -- re1.akpak --load savedat1.dat

# Authoring/oracle seams: run a Lua chunk once on the freshly booted session
# (before its first tick; it receives the game state as `local api = ...`) and
# serialise the resulting state to a save block, or both
cargo run -- re1.akpak --ui game --room 101 \
  --setup-script oracle/saves/pause_chris_101.lua \
  --save-out out/savedat1.dat

# Headless capture (no display)
cargo run -- re1.akpak --room 100 --player 0 --capture cut0.bmp
# An RE2 room (letter stages A-G name rooms too, e.g. --room G01), in a debug
# render mode: the collision drawn over the background (F4 cycles the modes)
cargo run -- re2.akpak --room 100 --render-mode collision-overlay --capture col.bmp
# An RE2 model posed and animated in a room (no AI): any EMD, PLD or PLW
cargo run -- re2.akpak --room 100 --model enemy/10/enemy_0.emd --clip 0 --ticks 30 --capture zombie.bmp
# Run N fixed 30 Hz ticks before a room capture (deterministic, audio-free),
# so scripted NPC scenes are captured in motion
cargo run -- re1.akpak --room 20D --player 0 --ticks 30 --capture npc.bmp
cargo run -- re1.akpak --ui title --capture title.bmp
cargo run -- re1.akpak --ui select --capture select.bmp
cargo run -- re1.akpak --ui menu --capture menu.bmp
cargo run -- re1.akpak --ui box --capture box.bmp
cargo run -- re1.akpak --ui file --capture file.bmp
cargo run -- re1.akpak --ui map --capture map.bmp
cargo run -- re1.akpak --ui view --capture view.bmp
# Kill the player and capture the DIED screen (--character 0|1)
cargo run -- re1.akpak --ui death --character 1 --capture died.bmp
# Capture the post-film ending screens
cargo run -- re1.akpak --ui result --capture result.bmp
cargo run -- re1.akpak --ui epilogue --capture epilogue.bmp
# Play an ending end to end: the films, RESULT, epilogue and next-cycle save
cargo run -- re1.akpak --ending 1

# Frame-time report for a 600-tick room run: per-tick min/avg/p95/max, the
# load/update/effect/render phase totals and the entity/effect high-water
# marks. Needs no --capture; the budgets live in docs/performance.md. Adding
# --frame-rate 120 also times the frames a 120 fps display would draw between
# the ticks against the per-frame budget
cargo run -- re1.akpak --room 100 --ticks 600 --stats

# Present 120 frames per second (the simulation stays at 30 Hz; the scene is
# drawn between ticks). With --stats and no --ticks the room is played in the
# window and the actual presented rate is printed when it closes
cargo run -- re1.akpak --room 100 --frame-rate 120 --stats

# The 3D scene defaults to the GPU (SDL_GPU over Vulkan) at the window's
# resolution; --renderer software or a failed device uses the 320x240
# rasterizer instead. Captures and --stats always use software.
cargo run -- re1.akpak --render-scale 150 --msaa 4

# Port-only debug tools: press F1 while playing to freeze the room and open
# the tool list (ROOM SELECT, INVENTORY, PLAYER, FLAGS, CHEATS, SAVE, ITEM BOX,
# CLEAR GAME, CONTROLS, DISPLAY). Confirm opens a tool, Esc backs out one
# level, F1 closes from anywhere and reopening returns to the same tool.
# `--ui debug` captures the tool list
cargo run -- re1.akpak --room 100
cargo run -- re1.akpak --ui debug --capture debug.bmp

# Classic REbirth tactical reload is on by default: with a gun aimed, hold RUN
# to reload instead of firing. Turn it off for one run with:
cargo run -- re1.akpak --room 100 --no-tactical-reload

# Authentic mode keeps the original's item rules: a healing item that would do
# nothing is refused, such as the first-aid spray and the herbs at full health,
# and a blue herb while the player is not poisoned. It is off by default, and
# only the command line turns it on (for checking the port against the
# reference). It also runs each room's original script bytecode (see
# `room_script_fixes` below):
cargo run -- re1.akpak --room 100 --authentic

# Run the rooms' original script bytecode without the rest of authentic mode:
cargo run -- re1.akpak --room 100 --no-room-script-fixes
```

Captures, assembled scripts and converted packs are written to a sibling
temporary file and renamed into place, so a failed run never leaves a partial
artifact behind.

## Mods and authoring

Packs layer. Any entry of a mod pack shadows the base pack's entry, including a
whole-room `room/{id}/room.scd` script override and `lua/**` hooks. Art and audio may
be shipped in the format they are authored in — WebP or PNG images and OGG
Vorbis audio — and the engine resolves them by entry stem with a fixed
extension priority (`webp > png > bmp > tim > pix`, `ogg > wav`):

```sh
# Print the HD texture-pack file names for an original file's textures
cargo run -- hash /path/to/JPN/STAGE1/RC1000.pak   # bgd/DD17F52F.webp

# Build the checked-in demo sources into a pack (no game assets included)
cargo run -- mod build mods/demo --out demo.akpak

# Layer it explicitly; --mod is repeatable, later load_order wins. A mod
# source directory can be passed directly, without `mod build` first.
cargo run -- re1.akpak --mod demo.akpak --room 100 --ticks 40 --capture demo.bmp
cargo run -- re1.akpak --mod mods/demo --room 100 --ticks 40 --capture demo.bmp

# The replacement-format example: a PNG background, a PNG/WebP UI page and an
# OGG BGM track over the shipped art
cargo run -- mod build mods/hd --out hd.akpak
cargo run -- re1.akpak --mod hd.akpak --room 100 --ticks 120 --capture hd.bmp

# A clean vanilla run: no --mod and no discovery
cargo run -- re1.akpak --no-mods --room 100

# Every mod the launcher finds applies automatically: the pack's sibling
# mods/, each data root's mods/ (on Windows %LOCALAPPDATA%\arklay\mods, the
# roaming %APPDATA%\arklay\mods and the install directory's) and the legacy
# <config dir>/mods. Only top-level *.akpak files and mod directories are
# candidates. `[mods]` in arklay.toml enables,
# disables and orders them by manifest id (see docs/modding.md).
cargo run -- re1.akpak --room 100

# Inspect a pack's manifest, layers and merged entries
cargo run -- pack info re1.akpak --mod demo.akpak

# Disassemble/reassemble a room's scripts
cargo run -- scd export ROOM1000.RDT --out 1000.s
cargo run -- scd build 1000.s --out room/100/room.scd

# Decompile/recompile a room's scripts as structured code
cargo run -- scd export ROOM1000.RDT --out 1000.akscr
cargo run -- scd build 1000.akscr --out room/100/room.scd

# Export every room of the installs as one script per room (players, mansion
# visits and regions as run-time tests: if (JILL), if (MANSION2), if (USA)),
# build the runtime build, and build one RDT's own bytecode out of it
cargo run -- scd export-rooms path/to/install --out games/re1/room
cargo run -- scd export-rooms usa=english jpn=japanese eur=french --out games/re1/room
cargo run -- scd build games/re1/room/103/room.akscr --out room/103/room.scd
cargo run -- scd build games/re1/room/103/room.akscr --room 6031 --region jpn --out 6031.scd

# Boot a mansion room on its second visit
cargo run -- re1.akpak --room 106 --mansion2
```

`docs/modding.md` documents the manifest grammar, the replacement formats and
their override table, the `.s` grammar, the `.akscr` script language, the mod source convention, the
language-file overrides and the sandboxed Lua hook API; `docs/language.md`
documents the `language/<tag>.txt` format, the key registry, the fallback chain
and the missing-glyph box; `docs/m15-deviations.md` records the deliberate
limits (whole-room script replacement, sandbox, error/budget policy). A run
without any layer stays byte-identical to a pre-M15 run, Lua or not.

`convert-game` and `extract` report progress on stderr as a single bar for the
whole command. `convert-game` runs its asset categories (rooms, camera cuts,
BGM, player files, compression, bytes written) as internal phases under that
one bar, which names the phase in progress, so a conversion never scrolls a bar
per category. On a terminal it hides the cursor and redraws the bar in place
with a percentage, then erases it when the run finishes (restoring the cursor);
when redirected it emits periodic plain lines without control characters
instead. The warnings are buffered and
printed to stderr after the bar is gone, so they never corrupt it, and a
conversion ends with a single
`Created <pack> (<n> entries, <size> MiB) in <time>` line. With `--verbose`
the conversion additionally prints the stage and category totals, the
compression summary and one line per packed file with its plain
(pre-compression) and stored size on stdout, in processing order; the report
is identical whatever the worker count. Every entry of a converted pack is
zstd-compressed at a fixed level except formats that already carry their own
compression (`avi`, `ogg`, `png`, `webp`), which are stored raw; an entry that
would not shrink also stays raw.
`extract` rejects packs whose entry paths are absolute, contain
`..` or backslashes, or are empty, so it never writes outside `--out`.

### Gameplay options

`arklay.toml` also reads a `[game]` table of optional behaviours and the
in-game menu's options. The gameplay options are on by default; a trace or
capture still keeps them off so its output is unchanged. The in-game menu
(Esc, or the gamepad's Start) edits the same table and writes it back.

```toml
[game]
quick_turn = true      # BACKWARD + RUN turns the player 180 degrees
tactical_reload = true # AIM + RUN reloads the equipped gun
analog_movement = true # the left stick moves relative to the camera
auto_aim = true        # raising a weapon locks on to the nearest enemy
blood = true           # blood billboards, spurts and pools
chris_ink_ribbons = true # saving as Chris uses up an ink ribbon
jill_ink_ribbons = false # saving as Jill uses up an ink ribbon
autosave = true        # each door's room load writes autosave1.dat
room_script_fixes = true # false runs the rooms' original script bytecode
ui_font = "modern"     # the menus' face: "modern" or "retro"
modern_save_screen = true # the full-screen save/load list, or false for the original's
menu_sounds = true     # the in-game menu's cursor/confirm/cancel sounds
button_style = "auto"  # controller glyphs: "auto", "xbox", "playstation", "nintendo"
control_type = "a"     # the controller layout: "a", "b", "c" or "custom"
vibration = "normal"   # "off", "weak", "normal" or "strong"
music_volume = 100     # volumes in percent (0 to 100): music,
ambient_volume = 100   # ambient loops (rain, machinery),
effects_volume = 100   # sound effects
voice_volume = 100     # and voices
```

`auto_aim` locks a raised weapon on to the nearest enemy in front of the
player, as the original does; turn it off to aim where the player faces, like
the original USA release. `blood` controls the engine's blood billboards,
spurts and pools; a room script's own effect spawns are untouched. Neither
option has a per-run command-line flag; set them in `arklay.toml` or the
in-game menu.

`modern_save_screen` (Graphics › Display › HUD › Save Screen in the menu)
picks the save/load screen: a full-screen list of every save in the save
directory, newest first, with the room and the play time, where saving can
always add a new save; or the original's eight slots. `--authentic` runs and a
mod's own save screen are unaffected.

`chris_ink_ribbons` and `jill_ink_ribbons` (Gameplay › Ink Ribbons in the menu)
choose whether saving at a typewriter uses up an ink ribbon, on every
playthrough. The defaults follow the Japanese release, where Jill saves without
one. A character who needs no ink ribbons finds none either: their pickups are
hidden and count as taken, so turning the option back on mid-game does not bring
back ribbons from rooms already visited.

`room_script_fixes` (Gameplay › Room Scripts in the menu) chooses which room
scripts run. `convert-game` packs the checked-in room scripts
(`games/re1/room/*/room.akscr`, which are not packed themselves) as
`room/{id}/room.scd`, built with the port's fixes (the `#else` of each
`#if AUTHENTIC` block), and keeps the original
scripts (the `AUTHENTIC` build) beside it as `room/{id}/original.scd` where the
fixes change them. Turned off, the engine runs
`original.scd` (or the scripts in a raw `room/{id}/room.rdt`) instead. A mod
layer's `room/{id}/room.scd` still applies either way. A
change takes effect from the next room load. `--room-script-fixes` and
`--no-room-script-fixes` override the file for one run, and `--authentic`
implies `--no-room-script-fixes` unless `--room-script-fixes` is also given.
A `--trace` or `--input` run keeps the fixes unless one of those flags says
otherwise.

The Language › Subtitles options (`subtitles`, `subtitle_size`,
`subtitle_background`, `subtitle_color`, `speaker_names`, `speaker_name_color`)
style the subtitles of voiced lines. The room scripts pair each voice line with
a message that times itself out. In the interactive window the
native-resolution GUI draws that message's text centred along the bottom edge,
in place of the original's message window, with the chosen size, colour and
background band, and the speaker's name in front when the language file names
one (see [docs/language.md](docs/language.md#subtitles)). `subtitles = false`
hides these lines; their timing still runs. The French, German, Spanish,
Italian, Finnish and Japanese texts carry subtitle text. The USA release's
English (and the Russian seed) keeps only the timing, so
English shows no subtitles until a language file or mod adds them. Captures,
traces and `--ticks` runs still draw the message window as before.

`quick_turn` enables Classic REbirth's quick turn: while walking backward
(including the back diagonals), pressing RUN turns the player 180 degrees in
place over eight ticks, plays the no-weapon walk animation and its footsteps,
and hands control back to normal movement. The QUICK TURN button (`Q`)
starts the same turn without the backward press, from standing too. The
player does not move or collide during the turn, and the new facing is live
for rendering, collision and scripts. `--quick-turn` and `--no-quick-turn`
override the file for one run.

`tactical_reload` is the Classic REbirth tactical reload: while a gun is
aimed, holding RUN reloads it instead of firing, provided the clip is not full
and the matching ammunition item is in the inventory. The RELOAD button (`R`)
does the same without the aim: it raises the gun, reloads it and lowers it
again. It plays the normal
reload animation and its per-weapon cues and loads the matching ammunition
stack through the original's transfer, so no round is fired and nothing new is
animated (like the original's reload, a partial magazine is replaced by the
best spare stack, not topped up). Like Classic REbirth, only the handgun,
shotgun, Python and Magnum (weapon ids 2-5) reload this way: the knife,
flamethrower, rocket launcher and special machine guns are excluded, and a
full clip or a missing reserve is a no-op. The beretta also ejects its empty
magazine on the reload's tenth motion frame: the clip falls from the hand,
bounces once off the floor and disappears, driven by the original's joint-15
fall physics (the shotgun and the two Pythons have no such mesh, and this
applies to the empty-clip reload too). `--tactical-reload` and
`--no-tactical-reload` override the setting for one run.

`analog_movement` is the port's camera-relative stick movement, like the
RE1 remake's alternate controls: pushing the gamepad's left stick moves the
player toward that direction on screen, turning quickly toward it (a
reversal takes four ticks). A full push runs and a partial push walks
(holding RUN runs either way). A push held across a camera cut keeps
steering by the previous camera until the stick turns away from it, so the
walk does not double back over the cut line. The keyboard and the D-pad keep
the tank controls, and aiming and the menus still read the stick as
directions. Turn it off for tank controls on the stick too.

A `--trace` or `--input` run leaves quick turn and tactical reload off
unless the matching positive flag is passed, and always leaves
`analog_movement` off, so existing traces stay byte-identical. It never
turns on `--authentic` either; the oracle scripts pass that flag themselves
when they compare with the reference.

### Controls

| Key | Gamepad | Action |
| --- | --- | --- |
| `W` `A` `S` `D`, arrow keys | D-pad, left stick | Move (tank controls; the left stick moves relative to the camera, see `analog_movement`) / menu selection |
| `F`, `Return`, left mouse button | South (A / Cross) | Confirm / action / dismiss a message; skips the loading narration and a letterboxed cutscene; fires while aiming |
| Right mouse button | Left trigger | Aim the equipped weapon (hold); while the mouse button aims, moving the mouse up or down aims up or down |
| `F`, `Return`, left mouse button | Right trigger | Fire / reload with the weapon aimed |
| `R` | | Reload the equipped gun (needs `tactical_reload`) |
| `Q` | | Quick turn (needs `quick_turn`) |
| `1` … `9`, `0` | | Equip the weapon in inventory slot 1 to 10; a slot without a weapon unequips |
| Mouse wheel | | Cycle the equipped weapon through nothing and each weapon in the inventory |
| Mouse back / forward | Left / right shoulder | Previous / next auto-aim target while aiming |
| `Tab`, `I` | North (Y / Triangle) | START: open/close the inventory (pause menu), cycle the top tabs |
| `M` | Back (Select) | Open the map; again (or Cancel) closes it |
| `Esc` | Start | Open/close the in-game menu (restart, load game, options, quit) |
| `[`, `]` | Left / right shoulder | L1/R1: page the item box |
| `Esc`, `Backspace` | East (B / Circle) | Cancel (menus, character select, load screen) |
| `Shift` | West (X / Square) | Run (hold) |
| `Shift` + `,` | Previous camera cut |
| `Shift` + `.` | Next camera cut |
| `F4` | Port-only debug render modes: normal, background only, masks only, models only, the AOT overlay, collision wireframe, solid collision, and collision drawn over the scene from the cut camera (`--render-mode` picks one at launch) |
| `F1` (gamepad Back + Start together) | Port-only debug tools: freeze the room and open the tool list (room select, inventory, player, flags, cheats, save/load, item box, clear game, controls, display); Esc backs out one level, F1 closes |
| `F2`, `F3` | Port-only game speed: F2 steps down and F3 up, 1x to 3x in 0.5x steps; the new speed shows briefly in the bottom-left corner |
| `F6` | Port-only debug HUD: steps through full information (engine, FPS, resolution, memory, CPU, pack, mods, room, player, boss), a compact player panel (health, condition, ammo) with the boss's health, the FPS alone in the top-right corner, and off |
| `F9` | Return-to-title prompt: a second F9 returns to the title screen, any other key cancels |
| `F12` | Port-only screenshot: saves the game frame (without the window's bars, the debug HUD or the in-game menu) as a PNG in `screenshots/` under the data directory, named by the local time; the file name shows briefly in the bottom-left corner |

Every action except the fixed inputs (`F1`, `F9`, the gamepad's Start
and its Back + Start combination) can be rebound, with any number of keyboard, mouse (buttons and
wheel) and gamepad bindings each. The CONTROLS tool in the `F1` menu lists the
actions: up/down pick one, left/right pick a binding slot (the last slot adds
a new one), confirm captures the next key, button, wheel notch or
stick/trigger push (it gives up after about five seconds), START (inventory)
clears the slot and RESET restores the defaults. A binding shared with another
action is drawn red and the other actions are listed below. Up, down, left,
right, confirm, cancel and START must each keep a keyboard key bound to none
of the others, so the menus can always be driven from the keyboard; a change
that breaks this is refused. Changes are saved to the `[bindings]` table of
`arklay.toml`, which can also be edited by hand; only the actions that differ
from the defaults are written, and actions it leaves out keep their defaults:

```toml
[bindings]
confirm = ["key:Space", "key:Return", "pad:a", "mouse:left"]
fire = ["key:Z", "axis:righttrigger+", "mouse:left"]
```

The options' Controller page picks a controller layout: Type A, B, C or
Custom. Custom starts as Type A, and its Customize Buttons row opens the same
screen as the key configuration for the gamepad's buttons, sticks and
triggers. A button may do several things there (the layouts share them on
purpose), but up, down, left, right, confirm and cancel must each keep a
button none of the other five use, and START (inventory) needs a button, so
the menus can always be driven from the pad. Start cancels a capture. The
Custom layout is kept while another type is picked, in the `[custom_pad]`
table, which lists only the gamepad sources that differ from Type A's:

```toml
[custom_pad]
start = ["pad:b", "pad:y"]
```

Sources are `key:<SDL key name>`, `mouse:left|middle|right|x1|x2`,
`wheel:up|down|left|right`, `pad:<SDL gamepad button>` and
`axis:<SDL gamepad axis>+|-`; an input SDL has no name for is written by
number (`key:#N`, `mouse:N`, `pad:#N`, `axis:#N+`). The actions are `up`, `down`, `left`, `right`,
`confirm`, `cancel`, `run`, `aim`, `fire`, `start`, `page_left`,
`page_right`, `quick_turn`, `reload`, `map`, `weapon_prev`, `weapon_next`,
`target_prev`, `target_next` and `equip_1` to `equip_10`. Saving rewrites the file in canonical form (comments are not
kept). A bad line or value in `arklay.toml` (an unknown key, `msaa = 3`, a
binding to a key that does not exist) never stops the game: it is reported
as a warning and that setting keeps its default. The file is not rewritten
while it has such a mistake in it, so a change made in game (a rebinding, F8)
then applies for the run only. Gamepads are picked up when plugged in; extra
controller mappings can be dropped into `gamecontrollerdb.txt` in the config
directory (or `SDL_GAMECONTROLLERCONFIG`).

`F8` steps the window up to the next whole multiple of 320x240, switches to
borderless fullscreen once the next size would not fit the display, and goes
back to 320x240 after that; `Alt+Enter` toggles borderless fullscreen. The
window cannot be resized by hand, but `window_width` and `window_height` can
be set to any size. The frame (4:3 by default, or the wider aspect set below)
always fits the window or screen, centred with black bars where the shape
differs: a whole zoom stays exactly pixel-sharp, and a fractional one keeps
every pixel the same size, blending only along their edges. The window size,
fullscreen, aspect and the display it is on are saved to the `[graphics]`
table of `arklay.toml`, so the next run opens the same way on the same screen:

```toml
[graphics]
renderer = "gpu"   # "gpu" (default) or "software"; --renderer overrides it
fullscreen = false # borderless fullscreen (Alt+Enter)
vsync = true       # wait for the display refresh; false allows tearing
window_width = 960 # the window's client size in pixels (F8 sets multiples
window_height = 720 # of 320x240); a window too big for its display shrinks
display = 1        # the display to open on, numbered from 1
aspect = "4:3"     # "16:9" or "16:10": crop the frame to the wider shape
render_scale = 100 # GPU only: the 3D scene's resolution, % of the screen area it fills (25..400)
msaa = 4           # GPU only: anti-aliasing samples per pixel (1, 2, 4 or 8)
frame_rate = 120   # presented frames per second, 30..1000 (default 30)
hdr = true         # request HDR output when the display supports it
```

`hdr` is stored and editable in the menu, but the game still presents SDR
until HDR output is implemented; see issue #188.

`frame_rate` only changes presentation: the game always simulates at the
original's 30 Hz, so input timing, scripts and traces are identical at every
rate. Above 30 each frame draws the characters, objects, shadows, effect
sprites and room fades between the last two ticks (one tick behind, like any
interpolation); camera cuts, room loads, teleports, door transitions, films
and the 2D screens snap. Any rate from 30 to 1000 is accepted, though V-Sync
caps it at the display's refresh rate when on; the title, menus and door
transitions, which tick every 16 ms, present once per tick (about 62 fps) at
any rate below that. The F1 menu's DISPLAY tool steps it through 30, 60, 120,
144, 165 and 240 and saves the choice.

`vsync` (on by default) waits for the display refresh before presenting.
Turning it off lets the frame pacer own the rate and may tear, which can feel
more responsive.

`aspect` (also `--aspect` and the F1 DISPLAY tool) presents a wider frame by
cropping the 320x240 view rather than pillarboxing it. In a room the visible
band follows the player's chest row with a small dead zone, also during
cutscenes, which draw no letterbox bars at a wider aspect; films show the
centred band, and door transitions keep the whole 4:3 frame, pillarboxed, so
the door is not cropped. The F1 debug menu fills the whole widescreen area
over the room's band. The 2D screens lay out in the band, and the pause menu
and its sub-screens take a full-height 16:9 or 16:10 canvas. Captures, traces,
the oracle and the goldens always run 4:3, so a wider aspect changes
presentation only.

The GPU renderer draws its 3D scene at the size the frame fills on screen, so
`render_scale` above 100 supersamples and below 100 trades sharpness for
speed, while `msaa` smooths the polygon edges. The software renderer always
draws the original 320x240 frame and only scales it up.

Any bound or unbound key, mouse button, wheel notch or gamepad input counts as
"any key" for prompts that take one. `Shift` + `,`/`.` stay the fixed
camera-cut keys, so binding `,` or `.` to an action also steps the camera when
`Shift` is held. Scripted `--input` runs are unaffected by bindings.

The character select slides the two cards between their poses while the pick
changes; the item viewer spins the model while a direction is held and confirm
runs the item's examine check (the rotation-gated descriptions and the red
book's zoom). The map tab shows the floor-plan browser with its "Mansion 1F"
caption; up/down step the rooms of the current floor plan, left/right step
between the plans the owned maps unlock, and confirm shows the full plan of
the current area. A pack without `map/` leaves the tab blank.

Saves live as `savedat*.dat` under `--save-dir` (default `<config
dir>/save`, `~/.config/arklay/save`).

`autosave` (Gameplay › Saving › Autosave in the menu, on by default) saves the
game each time a door loads a room. The save is written once the destination's
entry fade has opened and the player has kept control for a second with no
script event starting or ending, no message, menu or letterboxed scene up and
no live monster within reach, so a load never starts inside a scene. A room
that opens with a scene autosaves once the scene hands control back; a room
the player never controls (a cutscene leading straight through another door)
is not autosaved. Two autosaves rotate in the save directory: `autosave1.dat`
is the newest (typically the start of the current room) and `autosave2.dat`
the one before it (typically the start of the previous room). Each new
autosave is written and synced to a temporary file first, then
`autosave1.dat` moves over `autosave2.dat` and the new file takes its place,
on a background thread. An autosave does not count towards the ending's save
count, and loading a save or starting a game does not autosave. Only the
interactive window autosaves: `--trace`, `--input`, captures, the oracle
scripts, the soak and the tests never write one.

Both save/load screens list the autosaves. The ported original shows them in
load mode as extra rows after the eight slots, newest first (with no autosave
on disk it is the original screen exactly), and the title offers LOAD GAME
when only an autosave exists. The folder-based screen lists them like any
other save, labelled "Autosave 1" and "Autosave 2" (`api:saves()` marks them
with `autosave`), and does not offer them as overwrite targets when saving.
RESTART in the in-game menu after loading an autosave returns to the state
that was loaded, even after later autosaves have replaced the file. The title starts on LOAD GAME when a
save exists; with none, LOAD is refused. `arklay.toml` in the same config
directory sets the save directory, the language, the control bindings and
the graphics options. The save/load screen is a
mod: the checked-in `mods/crloadsave/` replaces the ported original with a
Classic REbirth-style folder screen through `api:register_save_screen`, built
and layered like any other mod:

```sh
cargo run -- mod build mods/crloadsave --out crloadsave.akpak
cargo run -- re1.akpak --mod crloadsave.akpak
```

The ignored `tests/real/m16_ui_real.rs` golden harness compares sampled
room/message/NPC/UI frames against a locally generated set when
`ARKLAY_RE1_GOLDEN` points at it; the set is never committed and is a
regression/cross-check, not an independent oracle. `docs/m16-deviations.md`
lists every approximation M16 leaves.

## Hardening and release

- `docs/architecture.md` maps every module, the fixed 30 Hz frame flow, the
  trust boundaries, the per-format budget caps and the engine invariants.
- `docs/status.md` is the milestone/format/verification matrix;
  `docs/m17-deviations.md` records what the hardening tooling does not prove.
- `docs/performance.md` records the frame-time budgets and the reference
  machine; `ARKLAY_PERF_BUDGET_MS` overrides the per-tick p95 budget in the
  ignored performance test.
- The local check has tiers (`AGENTS.md` has the full policy):
  `./scripts/check.sh` is the session tier, which runs fmt, clippy
  `--all-targets --all-features -D warnings` and the unit and fast integration
  tests in the `test-release` profile (release code without LTO). It is
  budgeted at five minutes cold and is what to run before pushing.
  `./scripts/check.sh --ci` runs everything the Linux CI job does: also the
  debug tests, the `--no-default-features` check, the fuzz compile check and
  the doc build. `./scripts/check.sh --full` adds every ignored test (the
  real-asset suite and the asset-gated unit tests) when `ARKLAY_RE1_ROOT` and
  `ARKLAY_RE1_PACK` are set, and is a release-cycle check.
- Use `--profile test-release` for every local test, build and run
  (`cargo run --profile test-release -- ...`), so one build serves them all.
- The real-asset tests are one binary, `tests/real/main.rs`; a module name
  stays a filter, so `cargo test --profile test-release --test real
  stairs_real:: -- --ignored` runs one module's tests and `... --test real --
  --ignored` runs them all.
- CI gates those commands on Linux and Windows plus a 1.88 MSRV check, and
  uploads `arklay-linux-x86_64.tar.gz` and `arklay-windows-x86_64.zip`
  artifacts (binary, README, LICENSE). A scheduled, non-gating fuzz workflow
  runs every `fuzz/` target for 300 seconds:
  `cd fuzz && cargo +nightly fuzz run <target> -- -max_total_time=300`; the
  stable compile gate `cargo check --manifest-path fuzz/Cargo.toml` runs in
  `scripts/check.sh --ci` and CI.
- A full-game soak (`tests/soak_real.rs`, ignored) walks every RDT with
  transitions and save round-trips under memory and wall-clock ceilings. It
  takes half an hour or more, so it is release-only and opt-in and no routine
  pass runs it:
  `ARKLAY_SOAK=1 cargo test --profile test-release --test soak_real --
  --ignored --nocapture`, or `./scripts/check.sh --soak` for every tier.

## License

MIT. No game assets or converted game data are distributed with this project.
