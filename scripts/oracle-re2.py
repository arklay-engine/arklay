#!/usr/bin/env python3
"""Record a trace from RE2's original executable, for the differential oracle.

RE2 has no reference source to build a tracer into, so this runs ``bio2 1.10``
and traces it from outside: on Linux under Wine with ``ptrace``, on Windows
natively with the Win32 debug API. It stops the game once per tick at the
one place every tick passes through, ``Pad_set``, straight after its pad read:
that is where a line of state is recorded and the scripted pad value goes in.
The game's code is otherwise untouched, and ticks are counted rather than
timed, so a slow host doesn't change what happens.

Usage:
    scripts/oracle-re2.py record --trace OUT --ticks N [--input SCRIPT]
        [--start leon|claire] [--tick-delay SECONDS] [--shot-dir DIR --shot-every N]
    scripts/oracle-re2.py scenario --input SCRIPT --ticks N --out DIR
        [--start leon|claire] [--arklay BIN] [--pack PACK] [--fields LIST]
    scripts/oracle-re2.py determinism --ticks N --out DIR [--input SCRIPT]
    scripts/oracle-re2.py patches

``--game`` names a retail install with Classic Rebirth's ``ddraw.dll``, by
default ``$ARKLAY_RE2_ROOT``. Tick 0 is the first tick of play, once the room's
load has finished, and a trace line holds the state after its tick, as arklay's
does. Input scripts (the format of ``oracle/inputs``) count from there. See
``oracle/re2.md``.
"""

from __future__ import annotations

import argparse
import bisect
import ctypes
import os
import re
import shutil
import signal
import struct
import subprocess
import sys
import threading
import time
from pathlib import Path

# `Pad_set`, on the instruction after its `CALL` to the pad read: EAX holds this
# tick's raw pad value and nothing has used it yet.
BREAK_PAD = 0x004D0F58
# Classic Rebirth doesn't patch this code. Refuse to run if it isn't what we expect.
BREAK_PAD_BYTES = bytes.fromhex("8B0D6C9E9800")  # MOV ECX,[fg_system]
# Where `Movie` (the film's loop, which `WinMain` runs in place of `Psx_main` while a film plays) returns to from its
# `Pad_set`, at `[esp + 0x14]` on `BREAK_PAD` (`Pad_set` has pushed five words by then).
FILM_PAD_RETURN = 0x004CAFB8
FILM_PAD_RETURN_AT = 0x14

# `SsPlay`: traced for the sound effects each tick plays (`snd=`).
BREAK_SOUND = 0x004338F0
VOL_3D_L = 0x00693C44
VOL_3D_R = 0x00693C46
VOL_3D_PAN = 0x00689DE4

# Addresses and offsets in bio2 1.10, named as in the IDA database and openre.
PL = 0x00989EF0  # the player's work
ENEMIES = 0x0098A114  # EnemyEntity*[32]: `enemy_slots` says how many are run
# `enemy_work` holds 32 slots, and `Game_loop` runs those below this end.
ENEMIES_END = 0x0098862C
ESP = 0x00992CA0  # `Esp_`: the effect pool (`Esp_init_R`, `Esp_call`, `Esp_move`)
ESP_SLOTS = 0x60
ESP_SLOT = 0x7C  # +0 and +1 the behaviours, +0x18 the flags, +0x1C the kind, +0x1E the row, +0x21 the frame, +0x34 the position
SPLAYER = 0x0098A110  # splayer_work
FLG_ADDR = 0x0053B218  # the flag banks
FG_SYSTEM = 0x00989E6C
DIE_VP = 0x99CEEC
DIE_VR = 0x99CEF8
DIE_PHASE = 0x99CEE2
DIE_COUNT = 0x99CEE4
DIE_SPR = 0x99CEC8
FG_STATUS = 0x00989ED0
FG_STOP = 0x00989ED4
FG_SCENARIO = 0x0098EB2C
FG_COMMON = 0x0098EB4C
FG_ROOM = 0x0098EB6C
FG_USE = 0x00989ED8  # `fg_use`: the key items the areas under the player want, four words
PL_AT_SCE = 0x00989EFB  # the player's `At_sce_no` (`+0xB`, script member 9): the last area `Sce_at_check` found it in
SCD_VARIABLES = 0x0098EAE4  # `scd_variables`: the scripts' variables, 0 the forward area, 1 the body area, 2 the used item
FG_ITEM = 0x0098EBB4  # bank 8, `fg_item`: the pick-ups taken in rooms 1xx-4xx (`scd_var_stage` 0-3; `St_get_item`, `St_get_file`)
FG_ITEM_2 = 0x0098EBD4  # bank 34, `fg_item_2`: rooms 5xx-7xx (`scd_var_stage` 4 up), 44 bytes to bank 35's 0x98EC00
STAGE = 0x0098EB14
ROOM = 0x0098EB16
CUT = 0x0098EB18
RANDOM = 0x00988610  # `Rnd`'s state
RANDOM_BASE = 0x00695E58  # `Sce_rnd_set`'s state
# The title (`Title`, `0x5035B0`) and its task, task 0 of `Tasks` (0x24 bytes
# each: +8 the init step, +9 the sub-step).
TASKS = 0x0068A220
TITLE_MV_STATE = 0x00691B80  # `title_mv_state`, the step `Title_mv` runs
TITLE_DISP = 0x00691B81  # `Ti_Disp_add`, `Title_main`'s step
TITLE_MENU = 0x00691B82  # the menu's page, then its step
TITLE_SEL_MODE = 0x00691B86  # `Ti_Sel_mode`
TITLE_CUR = 0x00691B87  # `Ti_Cur`
TITLE_PULSE = 0x00691B88  # the cursor's pulse
TITLE_SLIDE = 0x00691B8A  # the menu's slide count
TITLE_DEMO = 0x00691B90  # `demo_countdown`
TITLE_KIDO = 0x00691B92  # `Ti_kido`
TITLE_ADD = 0x00691B94  # `Ti_Add`
TITLE_FLASH = 0x00691B96  # the white tiles' level
TITLE_LOGO = 0x00691B98  # the logo's offset
TITLE_LOGO_COUNT = 0x00691B9C  # the logo's slide count
FADE = 0x0098F07C  # `Fade_set`'s slot 0: level, step, mode
FADE_SLOT = 0x4C  # the size of a fade slot: slot n is at FADE + n * FADE_SLOT
TITLE_MENU_2 = 0x00691B84  # the title's second page byte (the 4TH title's)
TITLE_DIM = 0x00691B8B  # the slide's dim
TITLE_B8C = 0x00691B8C  # the slide's second count (the 4TH title's logo level)
TITLE_EF2 = 0x00691EF2  # EX.BATTLE's level page bytes
# The option screen (`0x44A040`, `Config_main` 0x44A0B0), task 1 of `TASKS` while it runs, and the same screen as
# `Game_loop` starts it from play (`Config`, 0x449FD0).
OPTION_TASK = 0x0044A040
CONFIG_TASK = 0x00449FD0
OPTION_TASKS = (OPTION_TASK, CONFIG_TASK)
KEY_WORD = 0x0099CF64  # the input class's key word as `FUN_0043bb00` leaves it each tick
# KEYCONFIG's page (`Config_main`'s states 2 and 9 to 0xF) and its model (`FUN_0044b3c0`).
KEY_COUNT = 0x0099CF60
KEY_SLIDE = 0x0099CF34
KEY_CHOICE = 0x006893C8
KEY_TABLE_TEMP = 0x0099CF40
KEY_JOYPAD_TEMP = 0x0099CF80
KEY_MODEL = 0x00988628  # the work the page moves and draws: `&pPl`
KEY_TURNED = 0x006893D4
KEY_TURNING = 0x006893CC
KEY_DEMO_KEYS = 0x00689384
KEY_DEMO_COUNT = 0x006893A0
KEY_DEMO_STEP = 0x006893D8  # and the kept step after it (`0x6893DC`)
KEY_DEMO_ROW = 0x00525AB4
GAME_LOOP = 0x004BF810  # `Game_loop`, which the title's task chains to: play
TITLE = 0x005035B0  # `Title`, task 0's routine on the title
RESULT = 0x004B57C0  # `Result`, which `Game_loop` chains on `fg_system`'s 0x200000: a cleared game's results
# `Result`'s work: the routine `Result_mvtbl` runs (`0x995B80`: 0 `Result_00`'s fade in, 1 `Result_01`, 2 `Result_02`'s
# fade out), `Result_01`'s step (`0x995C24`) and count (`0x995C38`), the flags (`0x995B48`), the pieces' fade step
# (`0x995B84`), the skip's state (`0x99C870`), the rank (`0x99CE20`) and the three scores (`0x995B45` time, `0x995B44`
# saves, `0x99CE18` the scenario's), the digits `Result_retrieve` writes (`Reslt`, `0x995B30`, ten glyphs) and the
# clear time and saves it read (`0x99CE22`, four bytes).
RESULT_MV = 0x00995B80
RESULT_STEP = 0x00995C24
RESULT_COUNT = 0x00995C38
RESULT_FLAGS = 0x00995B48
RESULT_FADE_STEP = 0x00995B84
RESULT_SKIP = 0x0099C870
RESULT_RANK = 0x0099CE20
RESULT_DIGITS = 0x00995B30
RESULT_TIME = 0x0099CE22
# The pieces `Result_draw_quads` draws (`text_result_0`, 33 of 12 bytes): the flags (bit 0 shown, 4 fading in, 8
# fading out), the depth, the string, the place and, at +8, the colour.
RESULT_PIECES = 0x0052D178
RESULT_PIECE_COUNT = 33
MUSIC_FADE_RTN = 0x00693E8C  # `Fade_rtn`: `Snd_bgm_fade_ON`'s fade of the music, 0 when none runs
MUSIC_FADE_TIME = 0x0069346F  # `Fade_time`, its count
# `Prologue` (0x4CEFF0), task 1's routine from `Title_game_init`'s step 2 until it ends, and its work: the scroll's
# and the zoom's waits, the sequence (`Prologue_mv`'s index), the step in it, its count, the sprites' mask, the
# routine (`Prologue_r`'s index), the fade's flags and slot, the skip's gate and state, the frames drawn, the
# sprite table and its length, and the two pictures' sizes in `intromap_gfx` (entries 4 and 9), which zoom.
PROLOGUE = 0x004CEFF0
# The gallery (`Gallery`, 0x4BF760), task 1 of `TASKS` while it runs, and its work (`pGallery`).
GALLERY_TASK = 0x004BF760
GALLERY = 0x0066BEC8
# config.ini's GALLERY0 to GALLERY4: the bits of what play has opened in the gallery.
GALLERY_FLAGS = 0x0068059C
PROLOGUE_SCROLL_WAIT = 0x009800A4
PROLOGUE_ZOOM_WAIT = 0x009800A6
PROLOGUE_MV = 0x009800A8
PROLOGUE_FRAMES = 0x009800AC
PROLOGUE_FADE_SLOT = 0x009800B4
PROLOGUE_STEP = 0x009800B6
PROLOGUE_FADE_FLAGS = 0x009800B8
PROLOGUE_SKIP_OK = 0x009800B9
PROLOGUE_SPRITE_COUNT = 0x009800BA
PROLOGUE_SPRITES = 0x009800BC
PROLOGUE_R = 0x009800C0
PROLOGUE_SKIP = 0x009800C1
PROLOGUE_COUNT = 0x00980AE2
PROLOGUE_MASK = 0x00980AE4
INTROMAP_GFX = 0x00533490
OPTION_STATE = 0x0099D001  # `Config_main`'s state
OPTION_CURSOR = 0x0099CF68  # the menu's row
OPTION_TYPE = 0x0068915C  # the sound page's row
OPTION_WAIT = 0x0068914C  # a change's wait
OPTION_BLINKS = (0x00689158, 0x00689160, 0x00689154, 0x006880F0, 0x00689148)  # the arrows' counts: mode, quality, BGM, SE, exit
SPEAKER_CONFIG = 0x00669CFC  # 1 monaural, 2 stereo
SOUND_FREQUENCY = 0x00669D9C
SOUND_DEPTH = 0x00669F40
BGM_VOL = 0x00693471
SFX_VOL = 0x00693C48
EX_LEVEL = 0x00989E91
HARD_MODE = 0x006805B1  # `hard_mode`: the difficulty's HARD
CENSORSHIP_OFF = 0x006805B3  # `censorship_off`: ARRANGE MODE's
# `fg_system`'s bits the new-game choice sets: EASY (0x20), `censorship_off`'s
# (0x40) and the difficulty confirm's (0x20000).
FG_SYSTEM_CHOICE = 0x00020060

ENTITY_ROUTINES = 0x04
ENTITY_ID = 0x08
ENTITY_POS = 0x38  # m.t: x, y, z as i32
ENTITY_CDIR_Y = 0x76
ENTITY_TYPE = 0x10E
ENTITY_LIFE = 0x156
ENTITY_BE_FLG = 0x00
ENTITY_SPD = 0x144  # x, y, z as i16
ENTITY_MOTION = 0x14C  # move_no, move_cnt, hokan_flg, mplay_flg
ENTITY_D_LIFE_U = 0x151
ENTITY_PARTS_NUM = 0x107  # how many `PARTS_W` `Joint_trans2` visits
ENTITY_DEST = 0x1C4  # `Dest`: x, z as i16, where `Root_ck` steers the work
ENTITY_PARTS = 0x198  # `pSin_parts_ptr`: 0xAC bytes a part, `Be_flg` at +0 and `Poly_rgb` at +0x70

# Raw pad bits, the left column of the table `Pad_set` maps them with.
BUTTONS = {
    "up": 0x1000,
    "right": 0x2000,
    "down": 0x4000,
    "left": 0x8000,
    "confirm": 0x0080,
    "action": 0x0080,
    "cross": 0x0080,
    "cancel": 0x0040,
    "run": 0x0040,
    "aim": 0x0008,
    "fire": 0x0080,
    "start": 0x0800,
    "inventory": 0x0800,
    # The PC's map key: `Pad_set`'s table maps it to `Key`'s 0x4000.
    "map": 0x0010,
    # The PC's option key: `Pad_set` maps it to no `Key` bit, and `Game_loop` opens the option screen in play on
    # its edge (`0x9885FE`'s 0x100).
    "option": 0x0100,
    # ARRANGE MODE's pause key: `Pad_set`'s table maps it to no `Key` bit, and `Psx_main` (0x4C3C70) pauses and
    # resumes the game on its edge (`0x9885F8`'s 0x2). The executable's keys leave it unbound (`KeyDef` entry 6);
    # a pad button raises it.
    "pause": 0x0002,
    # The keyboard's own arrows, bits 25 to 28 of the input class's key word, which KEYCONFIG's page reads
    # (`KEY_WORD`), not the pad.
    "kbup": 0x02000000,
    "kbdown": 0x04000000,
    "kbleft": 0x08000000,
    "kbright": 0x10000000,
}
# The keys of an input line that are the keyboard's own (`KEY_WORD`'s fixed keys), and the pad's.
KEY_WORD_KEYS = 0xFE000000
PAD_KEYS = 0x01FFFFFF
# A key by its name (`key:X`, a letter or a digit), which KEYCONFIG's page binds (`Config_main`'s states 0xB and 0xF
# read it from `Kb_get_state`): its virtual key, above the line's 32 bits of pad and key word.
KB_KEY_SHIFT = 32
# `Kb_get_state` (0x4354D0): `GetKeyboardState` over its table of 0x43 virtual keys, the first newly down returned.
# On the tick a line's key goes down the recorder makes it return that key (`MOV EAX, vk; RET`) and puts the code
# back on the next stop, so the page sees the key on that tick alone, as a real key's edge. Only `Config_main` calls
# it.
KB_GET_STATE = 0x004354D0
KB_GET_STATE_BYTES = bytes.fromhex("81EC00010000")  # SUB ESP,0x100


def key_word(word: str) -> int | None:
    """An input word's virtual key (`key:X`), or None for a word that names no key."""
    if not word.startswith("key:"):
        return None
    name = word[4:]
    if len(name) == 1 and (name.isdigit() or "A" <= name.upper() <= "Z"):
        return ord(name.upper())
    raise SystemExit(f"unknown key {word!r}: a letter or a digit")


class KbPress:
    """An input line's key to `Kb_get_state`, on the tick it goes down."""

    def __init__(self) -> None:
        self.patched = False

    def tick(self, game: "Game", pad: int, held: int) -> None:
        vk = pad >> KB_KEY_SHIFT & 0xFF
        if vk and vk != held >> KB_KEY_SHIFT & 0xFF:
            if not self.patched and game.read(KB_GET_STATE, len(KB_GET_STATE_BYTES)) != KB_GET_STATE_BYTES:
                raise SystemExit("Kb_get_state isn't the code the recorder expects")
            game.write(KB_GET_STATE, bytes((0xB8, vk, 0, 0, 0, 0xC3)))
            self.patched = True
        elif self.patched:
            game.write(KB_GET_STATE, KB_GET_STATE_BYTES)
            self.patched = False

BOOT_TICKS = 3000
# The ticks from the first with a live player to the first of play: `Pl_init`,
# then the room's load collision pass, then the tick the load's `fg_stop` ends.
SETTLE_TICKS = 3
# The title screen to the first tick of play, as `(tick, raw pad)` from the first
# tick the tool sees. The game is deterministic, so fixed ticks are enough.
def press(tick: int, button: str) -> list[tuple[int, int]]:
    return [(tick, BUTTONS[button]), (tick + 3, 0)]


CHARACTERS = {"leon": 0, "claire": 1}
DIFFICULTIES = ("normal", "easy", "hard")


def title_route(start: str, difficulty: str = "normal", arrange: bool = False) -> list[tuple[int, int]]:
    """The title screen to play: ORIGINAL MODE (one down) or ARRANGE MODE (two), the character, then the difficulty.
    The difficulty's rows have NORMAL and EASY side by side and HARD under NORMAL (oracle/re2.md, "The new-game
    choice"). The gaps are at least those the `newgame_*` boot recordings prove: 40 ticks from a confirm to the
    next page's first press, and 30 from a press to the confirm."""
    route = press(200, "down")
    route += press(240, "down") + press(280, "confirm") if arrange else press(240, "confirm")
    if start == "claire":
        route += press(340, "right")
    route += press(400, "confirm")
    first = 460
    if difficulty != "normal":
        route += press(440, "down" if difficulty == "hard" else "right")
        first = 480
    # The confirms go on until play begins: the ones after the difficulty's skip the loading.
    return route + [entry for tick in range(first, BOOT_TICKS, 60) for entry in press(tick, "confirm")]


def choice_state(difficulty: str, arrange: bool) -> tuple[int, int, int]:
    """What the choice leaves when play begins: `fg_system` masked to `FG_SYSTEM_CHOICE` (the difficulty confirm's
    `0x20000`, EASY's `0x20`, ARRANGE MODE's `0x40`), `hard_mode` and `censorship_off`."""
    fg = 0x20000 | (0x20 if difficulty == "easy" else 0) | (0x40 if arrange else 0)
    return fg, int(difficulty == "hard"), int(arrange)


def lua_str(text: str) -> str:
    """`text` as a Lua string literal."""
    out = []
    for byte in text.encode():
        char = chr(byte)
        if char in '"\\' or byte < 0x20 or byte >= 0x7F:
            out.append(f"\\{byte:03d}")
        else:
            out.append(char)
    return '"' + "".join(out) + '"'


def arklay_driver(args: argparse.Namespace, trace: Path, input_path: str | None, frames: Path | None = None,
                  every: int = 1, room: str | None = None) -> str:
    """The arklay driver script for the same new game: the choice, `ticks` traced ticks of the scripted input (an
    input traces every frame, a door's and the death screen's too, as the original's `--test-input` does) and, with
    `frames`, a capture every `every` traced ticks."""
    options = [f"difficulty = {lua_str(args.difficulty)}"] + (["arrange = true"] if args.arrange else [])
    boot = [f"character = {CHARACTERS[args.start]}"] + ([f"room = {lua_str(room)}"] if room else [])
    held = {}
    if input_path:
        for line in Path(input_path).read_text().splitlines():
            words = line.split("#", 1)[0].split()
            if words:
                held[int(words[0])] = words[1:]
    table = ", ".join(f"[{tick}] = {{ {', '.join(lua_str(b) for b in buttons)} }}"
                      for tick, buttons in sorted(held.items()))
    return f"""strict(false)
load_game_pack({lua_str(str(Path(args.pack).resolve()))}, {{ {', '.join(options)} }})
new_game({{ {', '.join(boot)} }})
trace_start({lua_str(str(trace))}, {{ all_frames = {'true' if input_path else 'false'} }})
local held = {{ {table} }}
local ticks, every = {args.ticks}, {every}
local frames = {lua_str(str(frames)) if frames else 'nil'}
local applied
local updates, bound = 0, ticks * 2 + 10000
while trace_ticks() < ticks and updates < bound do
    local index = trace_ticks()
    if index ~= applied then
        applied = index
        local buttons = held[index]
        if buttons then
            release()
            if #buttons > 0 then hold(table.unpack(buttons)) end
        end
    end
    tick()
    updates = updates + 1
    if frames and trace_ticks() > index and index % every == 0 then
        capture(frames .. string.format("/f%05d.bmp", index))
    end
end
"""


def run_arklay(args: argparse.Namespace, out: Path, driver: str) -> None:
    """Run arklay on a driver script, kept beside the traces as `ark.lua`."""
    path = out / "ark.lua"
    path.write_text(driver)
    subprocess.run([str(Path(args.arklay).resolve()), "script", "--headless", str(path)], check=True,
                   stdout=subprocess.DEVNULL)


# What the two engines can compare today. arklay's random stream, fades and frames
# aren't RE2's yet.
RE2_FIELDS = "room,cam,p.x,p.y,p.z,p.angle,p.beh,p.as,p.hp,mot"

PTRACE_CONT = 7
PTRACE_SINGLESTEP = 9
PTRACE_GETREGS = 12
PTRACE_SETREGS = 13
PTRACE_SEIZE = 0x4206
PTRACE_O_TRACECLONE = 1 << 3
PTRACE_O_EXITKILL = 1 << 20
PTRACE_EVENT_CLONE = 3
PTRACE_EVENT_STOP = 128
WALL = 0x40000000
PR_SET_PDEATHSIG = 1
PR_SET_CHILD_SUBREAPER = 36

WINDOWS = sys.platform == "win32"
if WINDOWS:
    import _winapi

    import oracle_re2_windows as win
else:
    libc = ctypes.CDLL(None, use_errno=True)
    libc.ptrace.argtypes = [ctypes.c_long, ctypes.c_long, ctypes.c_void_p, ctypes.c_void_p]
    libc.ptrace.restype = ctypes.c_long


class Regs(ctypes.Structure):
    """x86-64 `user_regs_struct`: Wine's WoW64 runs the 32-bit game in a 64-bit process."""

    _fields_ = [(name, ctypes.c_ulonglong) for name in (
        "r15", "r14", "r13", "r12", "rbp", "rbx", "r11", "r10", "r9", "r8", "rax",
        "rcx", "rdx", "rsi", "rdi", "orig_rax", "rip", "cs", "eflags", "rsp", "ss",
        "fs_base", "gs_base", "ds", "es", "fs", "gs",
    )]


def ptrace(request: int, pid: int, addr: int = 0, data: int | ctypes.Array | None = 0) -> int:
    result = libc.ptrace(request, pid, addr, data)
    if result == -1:
        err = ctypes.get_errno()
        raise OSError(err, f"ptrace({request}, {pid}): {os.strerror(err)}")
    return result


# Where the game's 640x480 frame starts on the virtual display, its window's
# border being left and above it.
FRAME_ORIGIN = (3, 29)
SOUND_SINK = "arklay_oracle"
SCD_VAR_STAGE = 0x0098EB14
SCD_VAR_ROOM = 0x0098EB16
BGM_TBL = 0x0098E9C8
BGM_STAGE_START = 0x0053C790
BGM_CURRENT = 0x00693498
SS_NAME_BGM = 0x006934E0
SS_NAME_SBGM = 0x00693EA0
SEQ_CTR = 0x00693800
XA_VOLUME = 0x00693E88  # `xa_vol`'s level for the voice channel
MOVIE_IDX = 0x006805C0  # `Movie_idx`: the frames `WinMain` still holds (`Movie_set`)
MOVIE_R0 = 0x0068058E  # `Movie_r0`: the film's state, 0 when none plays


def parse_input(path: Path | None) -> list[tuple[int, int]]:
    """An `oracle/inputs` script as ascending `(tick, raw pad)` entries."""
    if path is None:
        return []
    entries = []
    for line in path.read_text().splitlines():
        words = line.split("#", 1)[0].split()
        if not words:
            continue
        pad = 0
        for word in words[1:]:
            vk = key_word(word)
            if vk is not None:
                if pad >> KB_KEY_SHIFT:
                    raise SystemExit(f"{path}: {words[0]}: one key per line, as `Kb_get_state` returns one")
                pad |= vk << KB_KEY_SHIFT
                continue
            if word not in BUTTONS:
                raise SystemExit(f"{path}: unknown button {word!r}")
            pad |= BUTTONS[word]
        entries.append((int(words[0]), pad))
    return sorted(entries, key=lambda entry: entry[0])


# `--saves DIR`: the save folder a record starts with (the game's `savedata/`); empty without it.
SAVES_FROM: Path | None = None


def prepare_saves(tree: Path) -> None:
    """Empty the game's save folder, or fill it from `--saves`: the save screen lists what is there, so a
    record must not depend on the saves an earlier one left."""
    saves = tree / "savedata"
    if SAVES_FROM is not None and (SAVES_FROM == saves.resolve() or saves.resolve() in SAVES_FROM.parents):
        raise SystemExit(f"--saves {SAVES_FROM} is inside the work tree's save folder, which a record empties")
    if saves.exists():
        shutil.rmtree(saves)
    saves.mkdir(parents=True)
    if SAVES_FROM is not None:
        for source in sorted(SAVES_FROM.iterdir()):
            if source.is_file():
                shutil.copy2(source, saves / source.name)


def keep_saves(tree: Path, dest: Path) -> None:
    """Copy the save folder a record left to `--keep-saves`, with the files' times. They replace what the folder
    held: a save's name comes from the wall clock, so a record again over an earlier one would list both."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for source in sorted((tree / "savedata").iterdir()):
        if source.is_file():
            shutil.copy2(source, dest / source.name)


def prepare_game(game: Path, work: Path, sound: bool = False) -> Path:
    """A writable copy of the install's settings, with links to its read-only data."""
    tree = work / "game"
    prepare_saves(tree)
    for name in ("bio2 1.10.exe", "ddraw.dll", "data"):
        link = tree / name
        if link.is_symlink() or link.exists():
            link.unlink()
        link.symlink_to(game / name)
    write_config(game, tree, sound)
    return tree


def prepare_game_windows(game: Path, work: Path, sound: bool = False) -> Path:
    """The install's code copied and its data joined, with settings of our own, under `work`.

    A symbolic link needs a privilege on Windows, so the executable and Classic Rebirth's
    DLL are copied (they are small) and `data` is a junction to the install's."""
    tree = work / "game"
    prepare_saves(tree)
    for name in ("bio2 1.10.exe", "ddraw.dll"):
        source, copy = game / name, tree / name
        if not copy.exists() or copy.read_bytes() != source.read_bytes():
            shutil.copyfile(source, copy)
    data = tree / "data"
    if data.is_junction() and Path(os.readlink(data)).resolve() != (game / "data").resolve():
        data.rmdir()  # removes the junction only, never what it points at
    if not data.is_junction():
        if data.exists():
            raise SystemExit(f"{data} is in the way of the game data's junction")
        _winapi.CreateJunction(str((game / "data").resolve()), str(data))
    write_config(game, tree, sound)
    return tree


# `--config KEY=VALUE`: keys of config.ini set for this run (SPECIAL below 15, for one).
CONFIG_OVERRIDES: dict[str, str] = {}


def write_config(game: Path, tree: Path, sound: bool) -> None:
    settings = (game / "config.ini").read_text(errors="replace")
    # A fixed config: no dialogs or sound device to wait on, none of Classic
    # Rebirth's optional gameplay changes, no optional art. The colour mode and
    # the mixing rate are the stored references': with `ColorMode = legacy` every
    # frame is about 5 levels brighter.
    fixed = {
        "BootConfig": "0", "MovieEnable": "0", "DirectSoundEnable": "1" if sound else "0",
        "QuickturnEnable": "0", "TacticalReloadEnable": "0", "AutoAimEnable": "0",
        "CutsceneSkip": "0", "DebugMode": "0", "FocusIgnore": "1",
        "ColorMode": "full", "SoundFrequency": "44100",
    } | CONFIG_OVERRIDES
    lines = []
    present = set()
    for line in settings.splitlines():
        key = line.split("=", 1)[0].strip()
        present.add(key)
        lines.append(f"{key} = {fixed[key]}" if key in fixed else line)
    # A `--config` key the install's config.ini lacks would be dropped and the
    # run recorded with the game's own value: refuse it instead.
    missing = sorted(set(CONFIG_OVERRIDES) - present)
    if missing:
        raise SystemExit(f"--config: {game / 'config.ini'} has no {', '.join(missing)} to set")
    (tree / "config.ini").write_text("\n".join(lines) + "\n")


def music_state(game: Game) -> str:
    """What the music has loaded, a line for `--sound`'s log."""
    def names(address: int, count: int) -> list[str]:
        stems = []
        for index in range(count):
            raw = game.read(address + index * 0x104, 0x104).split(b"\0", 1)[0].decode(errors="replace")
            stems.append(raw.rsplit("\\", 1)[-1])
        return stems
    stage, room = game.i16(SCD_VAR_STAGE), game.i16(SCD_VAR_ROOM)
    table = game.u16(BGM_TBL + 2 * (game.u8(BGM_STAGE_START + stage) + room))
    return (
        f"room={stage + 1}{room:02x} table={table:04x} current={game.u16(BGM_CURRENT):04x} "
        f"main={names(SS_NAME_BGM, 3)} sub={names(SS_NAME_SBGM, 2)} "
        f"seq={game.read(SEQ_CTR, 3).hex()},{game.read(SEQ_CTR + 8, 3).hex()},"
        f"{game.read(SEQ_CTR + 16, 3).hex()}"
    )


def music_trace(game: Game) -> str:
    """The `bgm` field: the room's word, each sequence's state and hold, the channels' tracks."""
    def track(raw: bytes) -> str:
        stem = raw.split(b"\0", 1)[0].decode(errors="replace").rsplit("\\", 1)[-1].lower()
        stem = stem.removesuffix(".sap")
        match = re.fullmatch(r"(main|sub)([0-9a-f]{2})(?:_([1-7]))?", stem)
        if not match:
            return "-"
        take = int(match[3] or 0)
        return f"{take * 0x200 + (0x100 if match[1] == 'sub' else 0) + int(match[2], 16):03x}"
    sequences = ",".join(game.read(SEQ_CTR + index * 8, 2).hex() for index in range(3))
    tracks = [track(game.read(SS_NAME_BGM + index * 0x104, 0x104)) for index in range(3)]
    tracks += [track(game.read(SS_NAME_SBGM + index * 0x104, 0x104)) for index in range(2)]
    return f"bgm={game.u16(BGM_CURRENT):04x},{sequences},{','.join(tracks)}"


def wine_env(work: Path, display: str) -> dict[str, str]:
    return dict(
        os.environ,
        WINEPREFIX=str(work / "prefix"),
        WINEDLLOVERRIDES="mscoree,mshtml=;ddraw=n,b",
        WINEDEBUG="-all",
        DISPLAY=display,
    )


def prepare_prefix(work: Path, env: dict[str, str], sound: bool = False) -> None:
    if (work / "prefix" / "system.reg").exists():
        return
    subprocess.run(["wineboot", "-i"], env=env, check=True, capture_output=True)
    subprocess.run(
        ["wine", "reg", "add", r"HKCU\Software\Wine\Drivers", "/v", "Audio", "/t", "REG_SZ",
         "/d", "pulse" if sound else "", "/f"],
        env=env, check=True, capture_output=True,
    )


# Display numbers the virtual display may take. They start well above a desktop's
# own (`:0`, `:1`): an X server unlinks its socket file when it exits, so a record
# on the desktop's number would leave the desktop's X clients unable to connect.
DISPLAY_FIRST = 100
DISPLAY_COUNT = 100


def display_taken(number: int) -> bool:
    """Whether display `number` has a lock file, or a socket some process has bound (by path or abstract name).

    The recorder's own servers, started with `-displayfd`, write no lock file, so a lock
    is another X server's, and it blocks the number even when its process is gone: that
    process may be alive in another PID namespace that shares /tmp. The socket file a
    killed server leaves behind doesn't count, since a new server replaces it."""
    socket_path = f"/tmp/.X11-unix/X{number}"
    if os.path.lexists(f"/tmp/.X{number}-lock"):
        return True
    try:
        with open("/proc/net/unix") as table:
            return any(line.rstrip().endswith((" " + socket_path, " @" + socket_path)) for line in table)
    except OSError:
        return os.path.lexists(socket_path)


def display_answers(number: int) -> bool:
    """Whether an X server accepts a connection on display `number` (the setup's first reply byte is 1).

    It tries the socket file, then the abstract socket of the same name."""
    import socket
    path = f"/tmp/.X11-unix/X{number}"
    for address in (path, "\0" + path):
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(10)
                connection.connect(address)
                # Little-endian, protocol 11.0, no authorisation.
                connection.sendall(b"l\0\x0b\0" + bytes(8))
                if connection.recv(1) == b"\x01":
                    return True
        except OSError:
            pass
    return False


def read_display_number(read_end: int) -> str | None:
    """The number the X server writes to `-displayfd`, or None if it exits first.

    The server writes the number and its newline separately, and exits if either write
    fails, so the pipe stays open until the newline has arrived."""
    received = b""
    while b"\n" not in received:
        chunk = os.read(read_end, 16)
        if not chunk:
            return None
        received += chunk
    return received.decode().strip()


def tail(path: Path, lines: int = 5) -> str:
    try:
        return " | ".join(path.read_text(errors="replace").strip().splitlines()[-lines:])
    except OSError:
        return ""


STOP_SIGNALS = {signal.SIGINT, signal.SIGTERM} | ({signal.SIGHUP} if hasattr(signal, "SIGHUP") else set())


class signals_held:
    """Hold Ctrl-C, SIGTERM and SIGHUP back until the block ends, so that a second one can't cut it short.

    The first that arrives meanwhile is raised again when the block ends. Handlers,
    not the signal mask, so that the children started inside the block (which get
    the default handlers back when they start their program) don't inherit it. A
    terminal's Ctrl-C or hang-up and `timeout` signal the whole process group, so
    the block's children run in a session of their own (`ALONE`), out of its reach."""

    def __enter__(self) -> None:
        self.arrived: list[int] = []
        self.previous = {
            number: signal.signal(number, lambda received, frame: self.arrived.append(received))
            for number in STOP_SIGNALS
        }

    def __exit__(self, *exc: object) -> None:
        for number, handler in self.previous.items():
            signal.signal(number, signal.SIG_DFL if handler is None else handler)
        if self.arrived:
            signal.raise_signal(self.arrived[0])


# For the children of a `signals_held` block: a session of their own, so that a signal
# sent to the recorder's process group doesn't stop them half done.
ALONE = {"start_new_session": True} if not WINDOWS else {}


def stop_with_recorder() -> None:
    """In a child, before it runs its program: have the kernel send it SIGTERM when the recorder dies, even by SIGKILL.

    The signal follows the thread that started the child; the recorder has only one."""
    libc.prctl(PR_SET_PDEATHSIG, signal.SIGTERM, 0, 0, 0)


def stop_display(server: subprocess.Popen) -> None:
    """Stop the X server so that it removes its lock and socket files: terminated, and killed only if it is still running after 5 s."""
    server.terminate()
    try:
        server.wait(timeout=5)
    except subprocess.TimeoutExpired:
        server.kill()
        server.wait()


def start_display(log: Path) -> tuple[subprocess.Popen, str]:
    """A private virtual display on the first free number from `DISPLAY_FIRST`, answering.

    The server's output goes to `log`. A number another server takes meanwhile is
    skipped; a server that dies otherwise, or doesn't answer, stops the record. Until
    this returns, any error or interruption stops the server it started; after, the
    caller stops it with `stop_display`, and the kernel does if the recorder dies."""
    for number in range(DISPLAY_FIRST, DISPLAY_FIRST + DISPLAY_COUNT):
        if display_taken(number):
            continue
        read_end, write_end = os.pipe()
        with open(log, "w") as output:
            server = subprocess.Popen(
                ["Xvfb", f":{number}", "-displayfd", str(write_end),
                 "-screen", "0", "1024x768x24", "-nolisten", "tcp"],
                pass_fds=(write_end,), stdout=output, stderr=subprocess.STDOUT,
                preexec_fn=stop_with_recorder,
            )
        os.close(write_end)
        try:
            try:
                reported = read_display_number(read_end)
            finally:
                os.close(read_end)
            if reported is None:
                server.wait()
                if display_taken(number):
                    continue  # Another server took the number between our check and its start.
                raise SystemExit(f"Xvfb exited before giving its display: {tail(log)}")
            if reported != str(number):
                raise SystemExit(f"Xvfb took display :{reported}, not :{number}")
            if server.poll() is not None or not display_answers(number):
                raise SystemExit(f"Xvfb on :{number} doesn't answer: {tail(log)}")
        except BaseException:
            if server.poll() is None:
                stop_display(server)
            raise
        return server, f":{number}"
    raise SystemExit(f"no free display from :{DISPLAY_FIRST} to :{DISPLAY_FIRST + DISPLAY_COUNT - 1}")


def find_game(deadline: float) -> int:
    """The game's process. Wine double-forks it, so it arrives as an orphan we adopt."""
    me = str(os.getpid())
    while time.monotonic() < deadline:
        for pid in filter(str.isdigit, os.listdir("/proc")):
            try:
                cmdline = Path(f"/proc/{pid}/cmdline").read_bytes()
                stat = Path(f"/proc/{pid}/stat").read_text()
                maps = Path(f"/proc/{pid}/maps").read_text()
            except OSError:
                continue
            ppid = stat.rsplit(")", 1)[1].split()[1]
            if ppid == me and (cmdline[:2] in (b"C:", b"Z:") or cmdline[:1] == b"/") and b"bio2" in cmdline \
                    and "bio2 1.10.exe" in maps and "ddraw.dll" in maps:
                return int(pid)
        time.sleep(0.02)
    raise SystemExit("the game did not start")


class Game:
    """The game's memory and what the tool notes from it; a backend below runs the process."""

    def __init__(self) -> None:
        self.sound_byte: int | None = None  # What `watch_sounds` overwrote.
        self.sounds: list[str] = []  # What's played since `trace_line` last took them.

    def read(self, address: int, size: int) -> bytes:
        raise NotImplementedError

    def write(self, address: int, data: bytes) -> None:
        raise NotImplementedError

    def watch_sounds(self) -> None:
        """Note every sound effect `SsPlay` starts in `self.sounds`."""
        self.sound_byte = self.u8(BREAK_SOUND)
        self.write(BREAK_SOUND, b"\xcc")

    def record_sound(self, esp: int) -> None:
        """Note the sound `SsPlay` is starting, from its arguments on the stack at `esp`."""
        kind, index = struct.unpack("<2i", self.read((esp & 0xFFFFFFFF) + 4, 8))
        if kind < 5:
            volume = max(self.u16(VOL_3D_L), self.u16(VOL_3D_R))
            pan = struct.unpack("<i", self.read(VOL_3D_PAN, 4))[0]
            self.sounds.append(f"{kind}:{index:x}:{volume}:{pan}")

    def seize_threads(self) -> None:
        """Take over the game's threads, where the backend needs to."""

    def hold(self, tid: int, seconds: float) -> None:
        """Keep the thread `tid` at its breakpoint for `seconds` while the others run."""
        time.sleep(seconds)

    def u8(self, address: int) -> int:
        return self.read(address, 1)[0]

    def i16(self, address: int) -> int:
        return struct.unpack("<h", self.read(address, 2))[0]

    def u16(self, address: int) -> int:
        return struct.unpack("<H", self.read(address, 2))[0]

    def u32(self, address: int) -> int:
        return struct.unpack("<I", self.read(address, 4))[0]

    def i32s(self, address: int, count: int) -> tuple[int, ...]:
        return struct.unpack(f"<{count}i", self.read(address, 4 * count))


class PtraceGame(Game):
    """The game's process under Wine, traced: stops at `BREAK_PAD` and otherwise runs freely."""

    def __init__(self, pid: int) -> None:
        super().__init__()
        self.pid = pid
        self.mem = os.open(f"/proc/{pid}/mem", os.O_RDWR)
        self.traced: set[int] = set()
        self.breaks = 0  # Stops at `BREAK_PAD` so far.

    def exited(self, status: int) -> SystemExit:
        """What to stop with when the game's main thread has ended with `status`."""
        if self.breaks == 0 and os.WIFEXITED(status) and os.WEXITSTATUS(status) == 13:
            # `WinMain`'s `error_no` 0xD: no Direct3D device, which under Wine
            # means no display driver. See game.log and xvfb.log under --work.
            return SystemExit("the game exited at start-up (code 13: no display or GPU; see game.log and xvfb.log)")
        return SystemExit("the game exited")

    def note_sound(self, tid: int, regs: Regs) -> None:
        """Record a stop on `SsPlay`, and run the instruction under it."""
        self.record_sound(regs.rsp)
        regs.rip = BREAK_SOUND
        ptrace(PTRACE_SETREGS, tid, 0, ctypes.addressof(regs))
        self.step_over(tid, BREAK_SOUND, self.sound_byte)
        ptrace(PTRACE_CONT, tid)

    def seize_threads(self) -> None:
        # Threads can start while we walk the list; TRACECLONE covers those
        # started by threads we already have, the second pass the others.
        options = PTRACE_O_TRACECLONE | PTRACE_O_EXITKILL
        for _ in range(2):
            for name in os.listdir(f"/proc/{self.pid}/task"):
                tid = int(name)
                if tid not in self.traced:
                    ptrace(PTRACE_SEIZE, tid, 0, options)
                    self.traced.add(tid)

    def read(self, address: int, size: int) -> bytes:
        return os.pread(self.mem, size, address)

    def write(self, address: int, data: bytes) -> None:
        os.pwrite(self.mem, data, address)

    def run_to_break(self) -> tuple[int, Regs]:
        """Run every thread until one stops on the breakpoint; it stays stopped."""
        while True:
            tid, status = os.waitpid(-1, WALL)
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                self.traced.discard(tid)
                if tid == self.pid:
                    raise self.exited(status)
                continue
            if not os.WIFSTOPPED(status):
                continue
            self.traced.add(tid)
            sig = os.WSTOPSIG(status)
            event = status >> 16
            if event in (PTRACE_EVENT_CLONE, PTRACE_EVENT_STOP):
                # A new thread, or a group stop: neither is ours to deliver.
                ptrace(PTRACE_CONT, tid)
                continue
            if sig == signal.SIGTRAP:
                regs = Regs()
                ptrace(PTRACE_GETREGS, tid, 0, ctypes.addressof(regs))
                if regs.rip - 1 == BREAK_PAD:
                    self.breaks += 1
                    return tid, regs
                if self.sound_byte is not None and regs.rip - 1 == BREAK_SOUND:
                    self.note_sound(tid, regs)
                    continue
            ptrace(PTRACE_CONT, tid, 0, sig)

    def resume_from_break(self, tid: int, regs: Regs, original: int) -> None:
        """Run the instruction under the breakpoint, put the breakpoint back, go on."""
        regs.rip = BREAK_PAD
        ptrace(PTRACE_SETREGS, tid, 0, ctypes.addressof(regs))
        self.step_over(tid, BREAK_PAD, original)
        ptrace(PTRACE_CONT, tid)

    def step_over(self, tid: int, address: int, original: int) -> None:
        """Run the instruction under the breakpoint at `address`, and put it back."""
        self.write(address, bytes([original]))
        ptrace(PTRACE_SINGLESTEP, tid)
        while True:
            stopped, status = os.waitpid(tid, WALL)
            if os.WIFSTOPPED(status) and os.WSTOPSIG(status) == signal.SIGTRAP \
                    and status >> 16 == 0:
                break
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                raise self.exited(status) if stopped == self.pid else SystemExit("the game exited")
            # Another signal arrived first: deliver it and step again.
            ptrace(PTRACE_SINGLESTEP, tid, 0, os.WSTOPSIG(status) if status >> 16 == 0 else 0)
        self.write(address, b"\xcc")


ITEM_WORK = 0x98ED34
EQUIPPED = 0x691F68  # three bytes, the last the weapon
PL_WEAPON = 0x10E
DOOR_RECORD = 0x9886F8 + 0x54 * 4  # `em_die_table[0x54]`: `Sce_Door`'s record
GAME_STATE = 0x991F80
ITEM_SLOTS = 0x98E9A4  # how many of `ITEM_WORK`'s first ten slots the player has
PL_ID = 0x8  # the player's `Id`: 0 Leon, 1 Claire, 0xE Ada, 0xF Sherry
PL_MAX_LIFE = 0x162
PL_STASH = 0x9888DA  # `St_chenge_pl`'s stash of Leon's or Claire's equipped slot (and, next, the slot count) while another player plays
LIFE_STASH = 0x98E9B6  # `Set_room`'s stash of their life
POISON_STATUS = 0x98A108
POISON_TIMER = 0x98A10A
STATUS_WORK = 0x68A204  # the status screen's work: its routines at +8 and +9
ITEM_GET_ROUTINE = 0x691F61  # the get-item window's routine and sub-routine
FG_KEY = 0x98ED2C  # `fg_key`: the doors' lock bits, two words (`Flg_ck`, MSB first)
MESSAGE_STATE = 0x98504C  # the message's state: the top bit while it shows, bit 0 a question's "no"
# The status screen's item page (`0x4F8160`): the command and its step
# (`0x691F63`, `0x691F64`), the step's tick count, the item and target
# cursors, the command cursor, and the condition panel's animation.
ITEM_PAGE = [0x691F63, 0x691F64, 0x691F65, 0x691F6C, 0x691F6D, 0x691F6F, 0x691FC2]
SCRATCH = 0x99EF00  # the unused end of the last page of `.data`


def warp(game: Game, target: str) -> None:
    """Do what `Sce_Door` does, with a door record of our own."""
    room, *place = target.split(":")
    x, y, z, angle, *cut = (int(part, 0) for part in place)
    stage, number = int(room[0], 16) - 1, int(room[1:], 16)
    floor = max(0, -y // 1800)
    game.write(SCRATCH, struct.pack("<hhhH4B8x", x, y, z, angle, stage, number, cut[0] if cut else 0, floor))
    game.write(DOOR_RECORD, struct.pack("<I", SCRATCH))
    game.write(GAME_STATE, b"\x01")
    game.write(FG_STOP, struct.pack("<I", game.u32(FG_STOP) | 0xFF000000))


def set_flag(game: Game, target: str) -> None:
    """Raise a bit of a flag bank, in the numbering of `ck` and `set`."""
    bank, index = (int(part, 0) for part in target.split(":"))
    address = game.u32(FLG_ADDR + bank * 4) + (index >> 5 & 7) * 4
    game.write(address, struct.pack("<I", game.u32(address) | 0x80000000 >> (index & 31)))


def enemy_slots(game: Game) -> list[int]:
    """The enemy slots' work pointers that `Game_loop` runs: at least 16, and up to the 32 the end pointer allows (room 40B's swarm uses 24)."""
    count = max(16, min(32, (game.u32(ENEMIES_END) - ENEMIES) // 4))
    return list(struct.unpack(f"<{count}I", game.read(ENEMIES, count * 4)))


def remove_enemies(game: Game, keep: set[int]) -> None:
    """Switch off live enemies but `keep`: nothing runs or touches an entity whose first bit is clear."""
    for slot, pointer in enumerate(enemy_slots(game)):
        if pointer != 0 and slot not in keep:
            flags = game.u32(pointer + ENTITY_BE_FLG)
            if flags & 1:
                game.write(pointer + ENTITY_BE_FLG, struct.pack("<I", flags & ~1))


def traced_enemy(entity_id: int) -> bool:
    """The enemies `zm` and `zp` list."""
    return 0x10 <= entity_id <= 0x22 or entity_id in (0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x33, 0x39, 0x3A, 0x3B)


def zombie_trace(game: Game) -> str:
    """The zombies, lickers, Mr. X, the plants, the spiders, the cockroaches, the G embryos and adult, the alligator, room 205's bodies (`Em_2D`), G1, G2, G3, the giant moth and its larvae in play: slot, id, position, facing, routines, motion, `Type`, life and speed."""
    rows = []
    for slot, pointer in enumerate(enemy_slots(game)):
        if pointer == 0 or game.u32(pointer) & 1 == 0 or not traced_enemy(game.u8(pointer + ENTITY_ID)):
            continue
        x, y, z = game.i32s(pointer + ENTITY_POS, 3)
        routine = game.read(pointer + ENTITY_ROUTINES, 4)
        motion = game.read(pointer + ENTITY_MOTION, 3)
        speed = struct.unpack("<3h", game.read(pointer + ENTITY_SPD, 6))
        rows.append(
            f"{slot}:{game.u8(pointer + ENTITY_ID)},{x},{y},{z},"
            f"{game.u16(pointer + ENTITY_CDIR_Y) & 0x0FFF},"
            f"{routine[0]},{routine[1]},{routine[2]},{routine[3]},"
            f"{motion[0]},{motion[1]},{motion[2]},{game.u16(pointer + ENTITY_TYPE)},"
            f"{game.i16(pointer + ENTITY_LIFE)},{speed[0]},{speed[1]},{speed[2]}"
        )
    return "zm=" + (";".join(rows) or "-")


def zombie_dest_trace(game: Game) -> str:
    """Where each enemy `zombie_trace` lists is heading: slot and `Dest` (`+0x1c4`, `+0x1c6`), which `Root_ck` writes. Neither `Sce_em_set` nor `Em10_Init` does: the new work is allocated at `Mem_top`, so it starts as whatever last sat at that address."""
    rows = []
    for slot, pointer in enumerate(enemy_slots(game)):
        if pointer == 0 or game.u32(pointer) & 1 == 0 or not traced_enemy(game.u8(pointer + ENTITY_ID)):
            continue
        x, z = struct.unpack("<2h", game.read(pointer + ENTITY_DEST, 4))
        rows.append(f"{slot}:{x},{z}")
    return "zd=" + (";".join(rows) or "-")


def root_turn_trace(game: Game) -> str:
    """Each live enemy work's `Root_ck` turn (`+0x150`), any id: slot, id and turn. A scene character's work keeps its own, which a script that makes it a zombie (room 202's Marvin) hands on."""
    rows = []
    for slot, pointer in enumerate(enemy_slots(game)):
        if pointer == 0 or game.u32(pointer) & 1 == 0:
            continue
        rows.append(f"{slot}:{game.u8(pointer + ENTITY_ID)},{game.u8(pointer + 0x150)}")
    return "rc=" + (";".join(rows) or "-")


def zombie_parts_trace(game: Game) -> str:
    """The parts of each enemy `zombie_trace` lists, when any is hidden or coloured: slot, the mask of parts `0..Parts_num` whose `Be_flg` bit 0 is clear, part 0's `Poly_rgb` and the wrapping sum of every part's."""
    rows = []
    count = max(16, min(32, (game.u32(ENEMIES_END) - ENEMIES) // 4))
    for slot, pointer in enumerate(struct.unpack(f"<{count}I", game.read(ENEMIES, count * 4))):
        if pointer == 0 or game.u32(pointer) & 1 == 0 or not traced_enemy(game.u8(pointer + ENTITY_ID)):
            continue
        number = min(32, game.u8(pointer + ENTITY_PARTS_NUM))
        parts = game.u32(pointer + ENTITY_PARTS)
        if parts == 0 or number == 0:
            continue
        block = game.read(parts, number * 0xAC)
        hidden, total, first = 0, 0, 0
        for part in range(number):
            flags = block[part * 0xAC]
            rgb = int.from_bytes(block[part * 0xAC + 0x70 : part * 0xAC + 0x73], "little")
            hidden |= (flags & 1 == 0) << part
            total = (total + rgb) & 0xFFFFFFFF
            if part == 0:
                first = rgb
        if hidden == 0 and all(
            block[part * 0xAC + 0x70 : part * 0xAC + 0x73] == b"\x80\x80\x80" for part in range(number)
        ):
            continue
        rows.append(f"{slot}:{hidden:x},{first:06x},{total:x}")
    return "zp=" + (";".join(rows) or "-")


def esp_trace(game: Game) -> str:
    """`Esp_`'s taken slots (`+0x18` non-zero, which is what `Esp_call`'s free search tests): behaviours, kind, row, flags, frame and position, after the live count."""
    pool = game.read(ESP, ESP_SLOTS * ESP_SLOT)
    rows, live = [], 0
    for slot in range(ESP_SLOTS):
        at = slot * ESP_SLOT
        flags = struct.unpack_from("<H", pool, at + 0x18)[0]
        if flags == 0:
            continue
        live += flags >> 15
        x, y, z = struct.unpack_from("<3h", pool, at + 0x34)
        rows.append(f"{slot}:{pool[at]:x},{pool[at + 1]:x},{pool[at + 0x1C]:x},{pool[at + 0x1E]:x},{flags:x},{pool[at + 0x21]:x},{x},{y},{z}")
    return f"esp={live}/" + (";".join(rows) or "-")


SPL_POS_CK_TIMER = 0x0053CDEF  # `Spl_pos_ck_timer`, `Sherry_move`'s count to its enemy check


SPL_NEAREST_DIST = 0x00692FC8  # `Spl_enemy_ck`'s three distances (u16), nearest first
SPL_NEAREST = 0x00692FE0  # and its three enemy works


def partner_enemy_trace(game: Game) -> str:
    """What the partner's `Spl_enemy_ck` left: the cache's three distances and the slots of the works it listed (`-` for none), then, after a `|`, each enemy `zombie_trace` lists as its slot, `L_spl` (`+0x1F4`) in hex and, if listed (its top two bits), `Dir_spl` (`+0x1F8`)."""
    dists = struct.unpack("<3H", game.read(SPL_NEAREST_DIST, 6))
    pointers = struct.unpack("<3I", game.read(SPL_NEAREST, 12))
    slots = enemy_slots(game)
    listed = [str(slots.index(pointer)) if pointer != 0 and pointer in slots else "-" for pointer in pointers]
    rows = []
    for slot, pointer in enumerate(slots):
        if pointer == 0 or game.u32(pointer) & 1 == 0 or not traced_enemy(game.u8(pointer + ENTITY_ID)):
            continue
        l_spl = game.u32(pointer + 0x1F4)
        if l_spl & 0xC0000000:
            rows.append(f"{slot}:{l_spl:x},{game.u16(pointer + 0x1F8)}")
        else:
            rows.append(f"{slot}:{l_spl:x}")
    return "spe=" + ",".join(f"{d}" for d in dists) + "," + ",".join(listed) + "|" + (";".join(rows) or "-")


def partner_climb_trace(game: Game) -> str:
    """`splayer_work`'s climb and water words: `Water` (`+0x10C`), `nFloor` (`+0x106`), `Damage_cnt` (`+0x1D3`) and `Be_flg`'s 4."""
    work = game.u32(SPLAYER)
    if work == 0 or game.u8(work + ENTITY_ID) < 0x40:
        return "spc=-"
    values = [game.i16(work + 0x10C), game.u8(work + 0x106), game.u8(work + 0x1D3), game.u32(work) & 4]
    return "spc=" + ",".join(str(value) for value in values)


def partner_trace(game: Game) -> str:
    """`splayer_work`'s own words for `Spl_Auto`: `Type`, the follow point, its state, distance and wall turn, the wall and touch counts, the timers, `Dest`, the life, `Sca_info & 1`, `Root_ck`'s count, `Spl_pos_ck_timer`, `Neck_flg` and `At_em_flg`."""
    work = game.u32(SPLAYER)
    if work == 0 or game.u8(work + ENTITY_ID) < 0x40:
        return "spl=-"
    (timer2,) = struct.unpack("b", bytes((game.u8(work + 0x16A),)))
    values = [
        f"{game.u16(work + ENTITY_TYPE):x}",
        game.i16(work + 0x218),
        game.i16(work + 0x21A),
        f"{game.u16(work + 0x21C):x}",
        game.u16(work + 0x21E),
        game.u16(work + 0x220),
        game.i16(work + 0x224),
        game.u8(work + 0x223),
        game.u8(work + 0x22A),
        game.i16(work + 0x158),
        game.i16(work + 0x15A),
        timer2,
        game.u8(work + 0x16B),
        game.i16(work + 0x1C4),
        game.i16(work + 0x1C6),
        game.i16(work + ENTITY_LIFE),
        game.i16(work + 0x162),
        game.u8(work + 0x110) & 1,
        game.u8(work + 0x150),
        game.u8(SPL_POS_CK_TIMER),
        f"{game.u8(work + 0x1C0):x}",
        f"{game.u16(work + 0x0E):x}",
    ]
    return "spl=" + ",".join(str(value) for value in values)


def inventory_trace(game: Game) -> str:
    """`Item_work`'s eleven slots, `id:count` (the tenth is the lighter's or the lockpick's), then the player's slot count."""
    work = game.read(ITEM_WORK, 11 * 4)
    slots = ",".join(f"{work[n * 4]:x}:{work[n * 4 + 1]}" for n in range(11))
    return f"inv={slots}/{game.u8(ITEM_SLOTS)}"


ITEM_BOX_WORK = 0x98ED60  # `Item_boxwork`: 64 slots of id, count and the two-slot weapon's half
ITEM_BOX_ENTRANCE = 0x98E542  # `Item_box_entrance`'s state; its object at 0x98E533
ITEM_BOX_OBJECT = 0x98E533
BACK_ROUTINE = 0x98E794  # slot 2 of the back routines (`pG+0x82B4`), which `Game_loop` runs after `Sce_at`
SAVE_PLACE = 0x98E9A7  # the save place `Sce_Save` copies from its area
# The save screen (`Mem_card` 0x4C57E0, `Card_access` 0x4C58A0), task 1 of `TASKS` while it runs.
TASK_1 = TASKS + 0x24  # its routine at +4, `Mem_card`'s step at +8, `Card_access`'s at +9, `Cardaccess_init`'s or `_exit`'s at +10
MEM_CARD = 0x4C57E0
CARD_STATE = 0x689A9C  # the mode in the low byte (0 save, 1 load), the state in the high byte
CARD_CURSOR = 0x9864B4  # `Cursor`, the row under the bar
CARD_TOP = 0x986390  # the list's first row shown
CARD_ROWS = 0x986394  # the rows shown
CARD_TIMER = 0x986388  # the move's and the reveal's timer
CARD_CHOICE = 0x98638C  # the overwrite's choice
CARD_TYPED = 0x689AA4  # the reveal's typed count
CARD_SAVES = 0x663188  # `cnt0`: the folder's saves
SAVE_COUNT = 0x98E9BC  # the low word: the saves made
ITEM_BOX_LID = 0x689C90  # the lid's speed and its step (`0x689C98`)
OBJECTS = 0x98A61C  # `pOm`: 0x1F8 bytes an object, the lid's turn at +0x78
ITEM_BOX_PAGE = [0x691F61, 0x691F62, 0x691F63, 0x691F6C, 0x691F76, 0x691F84]


FILE_TAG_BK = 0x98EE60  # `File_tag_bk`: the files held, three binders of eight, 0xFF for none
# The FILE page's work: the binder in front, the slot, the page, the blink's level and way, the zoom, the carousel's turn, the page's x.
FILE_WORK = ((0x691F71, "b"), (0x691F72, "b"), (0x691F73, "B"), (0x691F88, "B"), (0x691F89, "B"), (0x691F8E, "<h"), (0x691F90, "<h"), (0x691FBC, "<h"))


def file_trace(game: Game) -> str:
    """The status screen's FILE page (`St_menu0_file`): its work bytes, then the files held."""
    work = ",".join(
        str(struct.unpack(kind, game.read(address, struct.calcsize(kind)))[0]) for address, kind in FILE_WORK
    )
    return f"fl={work}/{game.read(FILE_TAG_BK, 24).hex()}"


PAD_REPEAT_COUNT = 0x533938  # `Pad_set`'s repeat counter (BYTE_00533938)
PAD_REPEAT_MASK = 0x98F074  # `Pad_rep_set`'s mask (DWORD_0098f074)
PAD_REPEAT_DELAYS = 0x98F078  # and its two delays (WORD_0098f078: the first, then the period)


def pad_repeat_trace(game: Game) -> str:
    """`Pad_set`'s repeat state: the flag (`fg_system`'s bit 31), the counter, the mask and the delays."""
    return (
        f"prep={game.u32(FG_SYSTEM) >> 31},{game.u8(PAD_REPEAT_COUNT)},"
        f"{game.u16(PAD_REPEAT_MASK)},{game.u16(PAD_REPEAT_DELAYS)}"
    )


def item_box_trace(game: Game) -> str:
    """The item box (`Item_box_entrance` and `St_main01`): the lid's state, speed, step and turn, the screen's routine, sub, count, cursor and repeat gate, the box's top row and its scroll, the cursor lines' pulse; then the inventory's two-slot marks and the box's filled slots."""
    lid = game.i16(OBJECTS + game.u8(ITEM_BOX_OBJECT) * 0x1F8 + 0x78)
    page = ",".join(str(game.u8(address)) for address in ITEM_BOX_PAGE)
    marks = "".join(str(game.u8(ITEM_WORK + n * 4 + 2)) for n in range(10))
    work = game.read(ITEM_BOX_WORK, 64 * 4)
    slots = ";".join(
        f"{n}:{work[n * 4]:x}:{work[n * 4 + 1]}:{work[n * 4 + 2]}" for n in range(64) if work[n * 4]
    )
    return (
        f"ib={game.u8(ITEM_BOX_ENTRANCE)},{game.i16(ITEM_BOX_LID)},{game.i16(ITEM_BOX_LID + 8)},{lid},"
        f"{page},{struct.unpack('b', bytes((game.u8(0x691F85),)))[0]},{game.i16(0x692FBE)} "
        f"box={marks}/{slots or '-'}"
    )


# The MAP page's banks: `fg_map`'s five words, `fg_map_c`, `fg_map_i` and the floors' bank 35.
MAP_FLAGS = (0x98EC04, 0x98EC08, 0x98EC0C, 0x98EC10, 0x98EC14, 0x98EC18, 0x98EC1C, 0x98EC00)
STATUS_PAGE = 0x691F61  # the status screen's page: 4 is MAP
STATUS_MODE = 0x691F70  # the status task's mode: 3 is the map key's map, from the press to `St_exit`
MAP_FLOOR = 0x691F7A  # the floor the MAP page shows
MAP_BLINK = 0x691F88  # the marker's blink, then its direction


def map_flags_trace(game: Game) -> str:
    """The MAP page's banks (`mapfg`), then the map's floor and blink while the page is MAP or the status
    task is in the map key's mode (`map`)."""
    flags = ",".join(f"{game.u32(address):08x}" for address in MAP_FLAGS)
    page = "-"
    if game.u8(STATUS_PAGE) == 4 or game.u8(STATUS_MODE) == 3:
        page = f"{game.u8(MAP_FLOOR)},{game.u8(MAP_BLINK)},{game.u8(MAP_BLINK + 1)}"
    return f"mapfg={flags} map={page}"


COMPUTER_TASK = 0x68A244  # task 1: +2 its sleep count, +4 its function, +8..+0xA R0..R2
COMPUTER_REQUEST = 0x989E74  # `Sce_Windows`' request: the mode in its low two bits, the cut above
COMPUTER_WINDOWS = 0x4D1F50  # `Windows`, the function task 1 runs in game mode 7
COMPUTER_WORK = 0x9800A0  # `pCom_work`: +0 col, +1 row, +0x2DC the frame's step


CARD_SCROLL = 0x986384  # the save screen's name scroll


def save_screen_trace(game: Game) -> str:
    """The save screen while `Mem_card` runs (`sv=-` otherwise): `Card_access`'s step, init's or exit's step,
    the state, the mode, the cursor, the top row, the rows, the timer, the choice, the typed count, the
    folder's saves and the saves made."""
    if game.u32(TASK_1 + 4) != MEM_CARD:
        return "sv=-"
    task = game.read(TASK_1, 0x14)
    state = game.u32(CARD_STATE)
    cursor, = game.i32s(CARD_CURSOR, 1)
    top, rows = game.i32s(CARD_TOP, 2)
    timer, = game.i32s(CARD_TIMER, 1)
    typed, = game.i32s(CARD_TYPED, 1)
    saves, = game.i32s(CARD_SAVES, 1)
    return (
        f"sv={task[9]},{task[10]},{state >> 24:x},{state & 0xFF},{cursor},{top},{rows},{timer},"
        f"{game.u8(CARD_CHOICE)},{typed},{saves},{game.u16(SAVE_COUNT)}"
    )


def computer_trace(game: Game) -> str:
    """The computer (`Windows`, then `Computer200` or room 613's `Computer600`): task 1's R0, R1, R2 and sleep count, `c_mode`, the frame's direction and step, `printed`, `confirmed`, and the console's column and row; in room 613 then the keyboard's frame mode (`Wframe_in_out`'s), its column and row, the key `Keyboard` read and how many letters are typed. `-` unless task 1 runs `Windows`."""
    if game.u32(COMPUTER_TASK + 4) != COMPUTER_WINDOWS:
        return "pc=-"
    work = game.u32(COMPUTER_WORK)
    step, col, row = (game.u8(work + 0x2DC), game.u8(work), game.u8(work + 1)) if work else (0, 0, 0)
    terminal = game.u8(COMPUTER_REQUEST) & 3 == 1
    printed, confirmed = (0x689B78, 0x689B74) if terminal else (0x689B6C, 0x689B68)
    text = (
        f"pc={game.u8(COMPUTER_TASK + 8)},{game.u8(COMPUTER_TASK + 9)},{game.u8(COMPUTER_TASK + 10)},"
        f"{game.u16(COMPUTER_TASK + 2)},{game.u8(0x98E52D)},{game.u8(0x98E52F)},{step},"
        f"{game.u32(printed)},{game.u32(confirmed)},{col},{row}"
    )
    if terminal:
        key_col, key_row, key, typed = (
            (game.u8(work + 2), game.u8(work + 3), game.u8(work + 0x2D9), game.u8(work + 0x2D8)) if work else (0, 0, 0, 0)
        )
        text += f",{game.u8(0x98E52E)},{key_col},{key_row},{key},{typed}"
    return text


HOOK = 0x98E790  # `pG + 0x82B0`: the per-tick hook Sce_Door's `Key_lost` and Sce_Hikidashi's `Hikidashi_entrance` share
HOOK_STATE = 0x98E541  # the hook's state byte
HOOK_HIKIDASHI = 0x4E9EA0
HOOK_KEY_LOST = 0x4E95F0
RDT = 0x98861C  # the room's RDT; its byte +2 is the room's object count


def hikidashi_trace(game: Game) -> str:
    """The drawer (`Sce_Hikidashi`): the hook (0 none, 1 `Hikidashi_entrance`, 2 `Key_lost`, 9 another), its state byte, then each room object's `Be_flg & 0x11` in hex."""
    hook = game.u32(HOOK)
    kind = {0: 0, HOOK_HIKIDASHI: 1, HOOK_KEY_LOST: 2}.get(hook, 9)
    rdt = game.u32(RDT)
    count = game.u8(rdt + 2) if rdt else 0
    flags = "/".join(f"{game.u32(OBJECTS + n * 0x1F8) & 0x11:x}" for n in range(min(count, 32)))
    return f"hk={kind},{game.u8(HOOK_STATE)},{flags or '-'}"


def objects_trace(game: Game) -> str:
    """Each room object (`pOm`, 0x1F8 a slot, below the RDT's object count): `n:Be_flg,x,y,z,Cdir_y,Push_cnt,At_sce_no,nFloor`, `Be_flg` in hex, joined with `;`."""
    rdt = game.u32(RDT)
    count = game.u8(rdt + 2) if rdt else 0
    rows = []
    for n in range(min(count, 32)):
        base = OBJECTS + n * 0x1F8
        x, y, z = game.i32s(base + 0x38, 3)
        rows.append(f"{n}:{game.u32(base):x},{x},{y},{z},{game.u16(base + 0x76)},{game.u8(base + 0x144)},"
                    f"{game.u8(base + 0x0B)},{game.u8(base + 0x106)}")
    return "om=" + (";".join(rows) or "-")


ON_OBJECT = 0x98A0D4  # `pOn_om`, the player work's +0x1E4: the room object the player stands on or climbs


def on_object_trace(game: Game) -> str:
    """`pon`: the slot of the room object the player stands on (`pOn_om`), or `-`."""
    pointer = game.u32(ON_OBJECT)
    return f"pon={(pointer - OBJECTS) // 0x1F8}" if pointer else "pon=-"


def trace_line(game: Game, tick: int, pad: int) -> str:
    """One tick's state: the `oracle-trace v1` fields RE2 has, then RE2's own."""
    x, y, z = game.i32s(PL + ENTITY_POS, 3)
    routines = game.read(PL + ENTITY_ROUTINES, 4)
    angle = game.u16(PL + ENTITY_CDIR_Y) & 0x0FFF
    life = game.i16(PL + ENTITY_LIFE)
    fields = [
        f"t={tick}",
        f"room={game.u16(STAGE) + 1}:{game.u16(ROOM)}",
        f"rng={game.u32(RANDOM) & 0xFFFF}",
        f"rseed={game.u32(RANDOM_BASE) & 0xFFFF}",
        "fade=0:-1",
        f"p={x},{y},{z},{angle},{routines[0]},{routines[1]},{life}",
        f"cam={game.u16(CUT) & 0xFF}",
    ]
    for slot, pointer in enumerate(struct.unpack("<16I", game.read(ENEMIES, 64)), start=1):
        if pointer == 0 or game.u32(pointer) & 1 == 0:
            continue
        ex, ey, ez = game.i32s(pointer + ENTITY_POS, 3)
        routine = game.read(pointer + ENTITY_ROUTINES, 4)
        fields.append(
            f"e={slot},{game.u8(pointer + ENTITY_ID)},{ex},{ey},{ez},"
            f"{game.u16(pointer + ENTITY_CDIR_Y) & 0x0FFF},{routine[0]},{routine[1]},{routine[2]},"
            f"{game.i16(pointer + ENTITY_LIFE)},0,0,0"
        )
    fields.append("f=0")
    fields.append(f"pad={pad & 0xFFFFFFFF}")
    fields.append(f"fgsys={game.u32(FG_SYSTEM):08x}")
    fields.append(f"fgstat={game.u32(FG_STATUS):08x}")
    fields.append(f"fgstop={game.u32(FG_STOP):08x}")
    fields.append(f"fgscn={game.read(FG_SCENARIO, 32).hex()}")
    fields.append(f"fgcom={game.read(FG_COMMON, 32).hex()}")
    fields.append(f"fgroom={game.u32(FG_ROOM):08x}")
    fields.append(f"fgitem={game.read(FG_ITEM, 32).hex()}")
    fields.append(f"fgitem2={game.read(FG_ITEM_2, 44).hex()}")
    for name, table in (("em", ENEMIES), ("es", SPLAYER)):
        work = game.u32(table)
        if work != 0 and game.u8(work + ENTITY_ID) >= 0x40:
            ex, ey, ez = game.i32s(work + ENTITY_POS, 3)
            routine = game.read(work + ENTITY_ROUTINES, 4)
            motion = game.read(work + ENTITY_MOTION, 4)
            speed = struct.unpack("<3h", game.read(work + ENTITY_SPD, 6))
            fields.append(
                f"{name}={game.u8(work + ENTITY_ID)},{ex},{ey},{ez},"
                f"{game.u16(work + ENTITY_CDIR_Y) & 0x0FFF},{routine[0]},{routine[1]},{routine[2]},"
                f"{routine[3]},{motion[0]},{motion[1]},{motion[2]},{game.u8(work + ENTITY_D_LIFE_U)},"
                f"{speed[0]},{speed[1]},{speed[2]}"
            )
    fields.append(partner_trace(game))
    fields.append(partner_enemy_trace(game))
    fields.append(partner_climb_trace(game))
    fields.append(zombie_trace(game))
    fields.append(zombie_parts_trace(game))
    fields.append(zombie_dest_trace(game))
    fields.append(root_turn_trace(game))
    fields.append(esp_trace(game))
    spd = struct.unpack("<3h", game.read(PL + ENTITY_SPD, 6))
    motion = game.read(PL + ENTITY_MOTION, 4)
    fields.append(
        f"mot={routines[2]},{routines[3]},{motion[0]},{motion[1]},{motion[2]},"
        f"{game.u8(PL + ENTITY_D_LIFE_U)},{spd[0]},{spd[1]},{spd[2]}"
    )
    try:
        fields.append(neck_state(game))
    except OSError:
        # The game over's end frees the player's parts.
        fields.append("neck=-")
    fields.append(neck_position(game))
    fields.append(music_trace(game))
    fields.append(f"xavol={game.u32(XA_VOLUME)}")
    fields.append(
        f"movie={game.u32(FG_SYSTEM) & 0x8200:04x},{game.u32(MOVIE_IDX)},{game.u8(MOVIE_R0)}"
    )
    fields.append(inventory_trace(game))
    fields.append(map_flags_trace(game))
    status = game.u32(STATUS_WORK)
    fields.append(
        f"gi={game.u8(GAME_STATE)},{game.u8(status + 8)},{game.u8(status + 9)},"
        f"{game.u8(ITEM_GET_ROUTINE)},{game.u8(ITEM_GET_ROUTINE + 1)},{game.u8(MESSAGE_STATE):x},"
        + ",".join(str(game.u8(address)) for address in ITEM_PAGE)
    )
    fields.append(file_trace(game))
    fields.append(pad_repeat_trace(game))
    fields.append(item_box_trace(game))
    fields.append(f"fgkey={game.u32(FG_KEY):08x}{game.u32(FG_KEY + 4):08x}")
    # Slot 2's back routine (`Sce_Save_backfunc` 0x4E9930 or `Item_box_entrance` 0x4E9A20), its state and the save place.
    fields.append(f"tw={game.u32(BACK_ROUTINE):x},{game.u8(ITEM_BOX_ENTRANCE)},{game.u8(SAVE_PLACE)}")
    # The option screen from play (`Config`), and its KEYCONFIG page.
    fields.append(f"opt={option_trace(game)}")
    fields.append(key_config_trace(game))
    fields.append(save_screen_trace(game))
    fields.append(f"svs={game.i32s(CARD_SCROLL, 1)[0]}")
    # The answers' blink count: the state word's third byte, which `Card_Mess_disp` steps.
    fields.append(f"svb={(game.u32(CARD_STATE) >> 16) & 0xFF}")
    fields.append(computer_trace(game))
    fields.append(hikidashi_trace(game))
    fields.append(objects_trace(game))
    fields.append(on_object_trace(game))
    fields.append(f"fguse={game.read(FG_USE, 16).hex()}")
    fields.append("scv=" + ",".join(f"{game.u16(SCD_VARIABLES + 2 * n):04x}" for n in range(4)))
    fields.append(player_trace(game))
    fields.append(f"atsce={game.u8(PL_AT_SCE):02x}")
    fields.append(result_trace(game))
    fields.append("snd=" + ("/".join(game.sounds) or "-"))
    game.sounds.clear()
    if game.u32(FG_SYSTEM) & 0x100000:
        fields.append(die_trace(game))
    return " ".join(fields)


def player_trace(game: Game) -> str:
    """The `pl` field: who plays and what a switch keeps (`Set_room` and `St_chenge_pl` on `save(32, N)`): the player's `Id`,
    `scd_var_next_pld` (variable 32), `Max_life`, the equipped slot and weapon (`St+8`, `St+0xA`), the stashed slot and
    slot count, the stashed life, then the poison's status and timer."""
    return "pl=" + ",".join(str(value) for value in (
        game.u8(PL + PL_ID), game.u16(SCD_VARIABLES + 2 * 32), struct.unpack("<h", game.read(PL + PL_MAX_LIFE, 2))[0],
        game.u8(EQUIPPED), game.u8(EQUIPPED + 2), game.u8(PL_STASH), game.u8(PL_STASH + 1),
        struct.unpack("<h", game.read(LIFE_STASH, 2))[0], game.u16(POISON_STATUS), game.u8(POISON_TIMER),
    ))


def result_trace(game: Game) -> str:
    """The results' state while task 0 runs `Result`: `res` (see `RESULT_MV`), `rest` the pieces shown (index, flags
    and colour), `rfade` fade slot 0 and `rmus` the music's fade (`Fade_rtn`, `Fade_time`). `-` on other ticks."""
    if game.u32(TASKS + 4) != RESULT:
        return "res=- rest=- rfade=- rmus=-"
    task = game.read(TASKS, 0x14)
    pieces = game.read(RESULT_PIECES, RESULT_PIECE_COUNT * 12)
    shown = [
        f"{index}:{pieces[index * 12]:x}:{struct.unpack_from('<I', pieces, index * 12 + 8)[0]:x}"
        for index in range(RESULT_PIECE_COUNT) if pieces[index * 12]
    ]
    return (
        f"res={task[8]},{task[9]},{game.u8(RESULT_MV)},{game.u32(RESULT_STEP):x},{game.u32(RESULT_COUNT)},"
        f"{game.u32(RESULT_FLAGS):x},{game.u32(RESULT_FADE_STEP):x},{game.u8(RESULT_SKIP)},{game.u16(RESULT_RANK)},"
        f"{game.u8(0x995B45)},{game.u8(0x995B44)},{game.u8(0x99CE18)},"
        f"{game.read(RESULT_DIGITS, 10).hex()},{game.read(RESULT_TIME, 4).hex()} "
        f"rest={'/'.join(shown) or '-'} rfade={fade_slot(game, 0)} "
        f"rmus={game.u8(MUSIC_FADE_RTN)},{game.u8(MUSIC_FADE_TIME)}"
    )


def die_trace(game: Game) -> str:
    """The game over's phase, then its view: `Spiral_move`'s camera and the `Be_flg`s `Die_enemy_set` chooses."""
    eye = game.i32s(DIE_VP, 3)
    target = game.i32s(DIE_VR, 3)
    flags = [game.u32(pointer) for pointer in struct.unpack("<16I", game.read(ENEMIES, 64)) if pointer]
    sprites = game.u32(DIE_SPR)
    state = struct.unpack("<2h", game.read(sprites, 4)) if sprites else (0, 0)
    return (
        f"die={game.u16(DIE_PHASE)},{game.i16(DIE_COUNT)},{state[0]},{state[1]} "
        f"dieview={','.join(map(str, eye))},{','.join(map(str, target))},"
        f"{game.u32(PL):x},{'/'.join(f'{flag:x}' for flag in flags)}"
    )


def neck_state(game: Game) -> str:
    """The player's neck: flags, joint, whether it has a target, the joint's turn and limits."""
    flags, joint = game.read(PL + 0x1C0, 2)
    target = game.u32(PL + 0x1B8)
    work = game.u32(PL + 0x198) + joint * 0xAC
    turn = struct.unpack("<2h", game.read(work + 0x98, 4))
    rate = struct.unpack("<2h", game.read(work + 0x9C, 4))
    limit = struct.unpack("<2h", game.read(work + 0xA0, 4))
    rot = struct.unpack("<3h", game.read(work + 0x68, 6))
    return (
        f"neck={flags},{joint},{target != 0 and target != PL:d},{turn[0]},{turn[1]},"
        f"{rate[0]},{rate[1]},{limit[0]},{limit[1]},{rot[0]},{rot[1]},{rot[2]}"
    )


def neck_position(game: Game) -> str:
    """Where the player's head is in the world: the neck joint's `+0x5C`, which `Root_ck` aims at."""
    parts = game.u32(PL + 0x198)
    if parts == 0:
        return "pneck=-"
    joint = game.u8(PL + 0x1C1)
    try:
        x, y, z = game.i32s(parts + joint * 0xAC + 0x5C, 3)
    except OSError:
        return "pneck=-"
    return f"pneck={x},{y},{z}"


def screenshot(display: str, path: Path) -> str | None:
    """The display as an image at `path`; the game's frame starts at `FRAME_ORIGIN`. What went wrong, if the
    picture wasn't taken: `path` is then no file, so that an older picture there can't pass for this one."""
    part = path.with_name(path.name + ".part")
    try:
        done = subprocess.run(
            ["import", "-window", "root", f"png:{part}"],
            env=dict(os.environ, DISPLAY=display), check=False, capture_output=True, text=True,
        )
        error = None if done.returncode == 0 and part.is_file() else (done.stderr.strip() or f"import exited {done.returncode}")
    except OSError as failed:
        error = str(failed)
    if error is None:
        part.replace(path)
    else:
        part.unlink(missing_ok=True)
        path.unlink(missing_ok=True)
    return error


def display_grabber():
    """Pillow's in-process picture of an X display, or None without Pillow's XCB support."""
    try:
        from PIL import ImageGrab, features
    except ImportError:
        return None
    if not features.check_feature("xcb"):
        return None
    return ImageGrab.grab


class FrameWriter:
    """Takes the Wine display's pictures in process and writes their PNGs on worker threads.

    A record takes a picture of every tick while the game is held at its break, so the
    picture's cost sets the record's pace, and with `--sound` the pace is part of what is
    recorded: a voice line ends by DirectSound's clock, so it holds for more ticks the
    faster they go. A record that has to agree with another's ticks follows it with
    `--pace`, as the regression runner does for a reference it records again.
    ImageMagick's `import`, started once per frame, takes about 0.28 s; a picture over XCB
    takes about 0.01 s once the display has settled (`settled`), and its PNG (0.15 s to
    encode) is written by a worker while the game plays on. The pixels are the same either way. Without Pillow's XCB support, and for a picture
    XCB can't take, `import` takes it as before."""

    WORKERS = 4
    # Pictures waiting for a worker: 2.3 MB each, so a writer that falls behind holds the record.
    BACKLOG = 32
    # See `settled`.
    SETTLE = 0.04
    STILL = 0.02
    STILL_TRIES = 10
    # Under the 0.28 s `import` took of a tick with a picture on it. A black display took it
    # 0.09 s (a room's load, a door), less than this: a record that follows a stored pace
    # (`--pace`) gives each picture the time that pace leaves it (`take`'s `within`), down to
    # `SETTLE + STILL`, so that it doesn't fall behind there and then run the ticks after
    # faster than they were to catch up.
    UNCHANGED = 0.15

    def __init__(self, display: str) -> None:
        self.display = display
        self.grab = display_grabber()
        self.pool = None
        self.room = threading.BoundedSemaphore(self.BACKLOG)
        # What went wrong writing a picture; a record with any has failed.
        self.errors: list[str] = []
        # The frames whose display never held still.
        self.unsettled: list[str] = []
        self.last: bytes | None = None
        # The frame `last` is the picture of: its name's letters and its number.
        self.last_frame: tuple[str, int] | None = None
        if self.grab is not None:
            from concurrent.futures import ThreadPoolExecutor

            self.pool = ThreadPoolExecutor(max_workers=self.WORKERS, thread_name_prefix="frame")

    def take(self, path: Path, within: float | None = None) -> None:
        """The display as it is now, written to `path` by the time `close` returns. `within`: the seconds
        a stored pace leaves this picture, which a tick that draws nothing new then waits at most."""
        unchanged = self.UNCHANGED if within is None else min(self.UNCHANGED, max(self.SETTLE + self.STILL, within))
        # `last` tells a late picture only as the picture of the tick before this one: after a
        # gap (`--shot-every`, or the boot frames' end) the display may have shown ticks in
        # between that no picture was taken of.
        letters = path.stem.rstrip("0123456789")
        frame = (letters, int(path.stem[len(letters):] or 0))
        if self.last_frame != (letters, frame[1] - 1):
            self.last = None
        self.last_frame = frame
        if self.pool is not None:
            try:
                picture = self.settled(path.stem, unchanged)
            except OSError:
                picture = None
            if picture is not None:
                self.room.acquire()
                try:
                    self.pool.submit(self.write, picture, path)
                except BaseException:
                    self.room.release()
                    raise
                return
        # `import`'s picture isn't seen here, so the next tick has no last picture to tell
        # a late one by.
        self.last = None
        error = screenshot(self.display, path)
        if error is not None:
            self.errors.append(f"{path.name}: {error}")

    def settled(self, name: str, unchanged: float | None = None):
        """The display once the tick's picture is on it. `unchanged` is `UNCHANGED`, or less under a stored
        pace (`take`).

        The game's thread reaches its break before the X server has drawn the picture the
        tick presented: a picture taken at once is the tick before's on about half the ticks,
        and one taken 10 ms later was the tick's own on every tick measured. Nothing tells
        the recorder that the picture has arrived, so it waits as `import` did, but only for
        as long as the display gives it reason to:

        - the first picture waits `SETTLE` seconds;
        - the display must then hold still for `STILL` more (two pictures in a row the same);
        - a picture that is still the last tick's own is what a late one looks like, so it is
          taken again until it changes or `UNCHANGED` seconds have passed since the break,
          fifteen times as long as the latest picture measured. A tick that draws what the
          last one drew costs that long, and so does a tick with no last picture to compare
          (the first, the one after a tick no picture was taken of, and the one after a
          picture `import` took).

        What this cannot tell from the tick's own picture: one later than `UNCHANGED`, and a
        display that changes, holds still for `STILL` and then changes again within the tick,
        where `import` showed whichever it found.

        A display that never holds still (a movie, or drawing on another of the game's
        threads) gives whatever it showed last, as `import` gave whatever it showed when it
        ran; `close` names those frames."""
        unchanged = self.UNCHANGED if unchanged is None else unchanged
        start = time.monotonic()
        time.sleep(self.SETTLE)
        picture = self.grab(xdisplay=self.display).tobytes()
        moved = 0
        while True:
            time.sleep(self.STILL)
            again = self.grab(xdisplay=self.display)
            if again.tobytes() != picture:
                picture = again.tobytes()
                moved += 1
                if moved >= self.STILL_TRIES:
                    self.unsettled.append(name)
                    break
            elif (self.last is not None and picture != self.last) or time.monotonic() - start >= unchanged:
                break
        self.last = picture
        return again

    def write(self, picture, path: Path) -> None:
        try:
            picture.save(path)
        except Exception as error:  # whatever it is, the frame is missing
            self.errors.append(f"{path.name}: {error}")
            # Not half a picture, and not an older record's.
            try:
                path.unlink(missing_ok=True)
            except OSError:
                pass
        finally:
            self.room.release()

    def close(self) -> None:
        """Wait for every picture taken so far to be on the disk."""
        if self.pool is not None:
            self.pool.shutdown(wait=True)
            self.pool = None
        if self.unsettled:
            shown = ", ".join(self.unsettled[:8]) + (", ..." if len(self.unsettled) > 8 else "")
            print(f"{len(self.unsettled)} frames whose display never held still: {shown}", file=sys.stderr)

    def check(self) -> None:
        """Fail the record if a picture wasn't written."""
        if self.errors:
            shown = "; ".join(self.errors[:4]) + ("; ..." if len(self.errors) > 4 else "")
            raise SystemExit(f"{len(self.errors)} frames were not written: {shown}")


class WineSession:
    """The game running under Wine on its own virtual display, traced."""

    def __init__(self, game_dir: Path, work: Path, sound_out: str | None = None) -> None:
        if not (game_dir / "bio2 1.10.exe").is_file():
            raise SystemExit("--game (or ARKLAY_RE2_ROOT) must be a retail RE2 install")
        work.mkdir(parents=True, exist_ok=True)
        # Wine double-forks the game. As a subreaper we adopt it, and Yama's
        # default policy lets a process trace its own descendants without root.
        libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0)
        self.display_server: subprocess.Popen | None = None
        self.env: dict[str, str] | None = None
        self.launcher: subprocess.Popen | None = None
        self.sink_module = None
        self.sound_capture = None
        self.capture_started = 0.0
        self.frames: FrameWriter | None = None
        # The callers' `finally: session.close()` starts only once this returns,
        # so a start that fails stops what it has started itself.
        try:
            self.start(game_dir, work, sound_out)
        except BaseException:
            self.close()
            raise

    def start(self, game_dir: Path, work: Path, sound_out: str | None) -> None:
        self.display_server, self.display = start_display(work / "xvfb.log")
        self.env = wine_env(work, self.display)
        sound = sound_out is not None
        if sound:
            # A sink of its own: the game is heard by nothing but the recording.
            # Held, so that a module that loads is always one close() unloads.
            with signals_held():
                self.sink_module = subprocess.run(
                    ["pactl", "load-module", "module-null-sink", f"sink_name={SOUND_SINK}"],
                    check=True, capture_output=True, text=True, **ALONE,
                ).stdout.strip()
            self.env["PULSE_SINK"] = SOUND_SINK
            self.sound_capture = subprocess.Popen(
                ["parec", f"--device={SOUND_SINK}.monitor", "--file-format=wav", sound_out],
            )
            self.capture_started = time.monotonic()
        prepare_prefix(work, self.env, sound)
        tree = prepare_game(game_dir, work, sound)
        self.launcher = subprocess.Popen(
            ["wine", str(tree / "bio2 1.10.exe")], cwd=tree, env=self.env,
            stdout=open(work / "game.log", "w"), stderr=subprocess.STDOUT,
        )
        self.game = PtraceGame(find_game(time.monotonic() + 60))
        # The code we break on is the executable's own (Classic Rebirth doesn't
        # patch it), so this passes as soon as the image is mapped, before the
        # game's code has run: it checks the executable, not that patching has
        # finished.
        deadline = time.monotonic() + 60
        while self.game.read(BREAK_PAD, len(BREAK_PAD_BYTES)) != BREAK_PAD_BYTES:
            if time.monotonic() > deadline:
                raise SystemExit("Pad_set is not the expected code: wrong executable?")
            time.sleep(0.01)

    def screenshot(self, path: Path, within: float | None = None) -> None:
        if self.frames is None:
            self.frames = FrameWriter(self.display)
        self.frames.take(path, within)

    def idle(self, seconds: float) -> None:
        """Let the game run untraced for a while."""
        time.sleep(seconds)

    def close(self) -> None:
        """Stop the game, Wine, the display and the sound capture: whichever of them have started.

        A second Ctrl-C, SIGTERM or SIGHUP waits until it is done."""
        with signals_held():
            try:
                self.stop_all()
            finally:
                # The pictures are already taken: writing them needs neither the game nor the display.
                if self.frames is not None:
                    self.frames.close()

    def frames_written(self) -> None:
        """Fail the record, once it is closed, if one of its pictures wasn't written."""
        if self.frames is not None:
            self.frames.check()

    def stop_all(self) -> None:
        if self.env is not None:
            # Some Wine packages keep `wineserver` off the path; `wineboot` kills too.
            try:
                subprocess.run(["wineserver", "-k"], env=self.env, check=False, **ALONE)
            except FileNotFoundError:
                subprocess.run(["wine", "wineboot", "--kill"], env=self.env, check=False, **ALONE)
        if self.launcher:
            self.launcher.kill()
        if self.display_server:
            stop_display(self.display_server)
            self.display_server = None
        if self.sound_capture:
            self.sound_capture.terminate()
            self.sound_capture.wait()
        if self.sink_module:
            subprocess.run(["pactl", "unload-module", self.sink_module], check=False, **ALONE)


class DebugGame(Game):
    """The game's process under the Windows debugger: stops at `BREAK_PAD` and otherwise runs freely.

    As under ptrace, only the thread at `BREAK_PAD` stops there: it is suspended and the debug
    event continued, so the game's other threads (its sound's among them) run on while a
    stop's work is done. The original's voice lines end by DirectSound's clock, so freezing
    the whole game for a stop's picture would hold them for more ticks than Wine does."""

    def __init__(self, process: "win.Debuggee") -> None:
        super().__init__()
        self.process = process
        self.stepping: dict[int, int] = {}  # Thread to the breakpoint it's stepping over.
        self.held: int | None = None  # The thread suspended at `BREAK_PAD`.
        self.on_break = lambda: None  # Called at each stop at `BREAK_PAD`.

    def read(self, address: int, size: int) -> bytes:
        return self.process.read(address, size)

    def write(self, address: int, data: bytes) -> None:
        self.process.write(address, data)

    def handle(self, event: tuple[int, int, int]) -> bool:
        """Deal with an exception; True when it is a stop at `BREAK_PAD`, which stays stopped."""
        tid, code, address = event
        if code in win.SINGLE_STEPS and tid in self.stepping:
            # The instruction under a breakpoint has run: put the breakpoint back.
            self.write(self.stepping.pop(tid), b"\xcc")
            context = self.process.get_context(tid)
            if context.EFlags & win.TRAP_FLAG:
                context.EFlags &= ~win.TRAP_FLAG
                self.process.set_context(tid, context)
            return False
        if code in win.BREAKPOINTS:
            if address == BREAK_PAD:
                return True
            if self.sound_byte is not None and address == BREAK_SOUND:
                context = self.process.get_context(tid)
                self.record_sound(context.Esp)
                self.step_over(tid, context, BREAK_SOUND, self.sound_byte)
                return False
            if address not in (BREAK_PAD, BREAK_SOUND):
                # The loader's breakpoints, once in 64-bit code and once in 32-bit.
                return False
        # Not ours: the game's own handlers decide.
        self.process.pass_exception()
        return False

    def run_to_break(self) -> tuple[int, "win.WOW64_CONTEXT"]:
        """Run every thread until one stops on the breakpoint; the game stays stopped."""
        while True:
            try:
                event = self.process.next_exception()
            except win.GameExited:
                raise SystemExit("the game exited") from None
            if self.handle(event):
                tid = event[0]
                context = self.process.get_context(tid)
                self.process.suspend(tid)
                self.held = tid
                self.process.release()
                self.on_break()
                return tid, context

    def step_over(self, tid: int, context: "win.WOW64_CONTEXT", address: int, original: int) -> None:
        """Run the instruction under the breakpoint at `address`; `handle` puts it back."""
        self.write(address, bytes([original]))
        context.Eip = address
        context.EFlags |= win.TRAP_FLAG
        self.process.set_context(tid, context)
        self.stepping[tid] = address

    def resume_from_break(self, tid: int, regs: "win.WOW64_CONTEXT", original: int) -> None:
        """Run the instruction under the breakpoint, put the breakpoint back, go on."""
        self.step_over(tid, regs, BREAK_PAD, original)
        if self.held == tid:
            self.held = None
            self.process.resume(tid)

    def hold(self, tid: int, seconds: float) -> None:
        """Keep `tid` at `BREAK_PAD` for `seconds`; the game's other threads run, as under ptrace."""
        deadline = time.monotonic() + seconds
        while (left := deadline - time.monotonic()) > 0:
            try:
                event = self.process.next_exception(left)
            except win.GameExited:
                raise SystemExit("the game exited") from None
            if event is not None and self.handle(event):
                raise SystemExit("a second thread reached Pad_set")


class WindowsSession:
    """The game running natively on Windows under the debugger, in its own tree under `work`."""

    def __init__(self, game_dir: Path, work: Path, sound_out: str | None = None) -> None:
        if not (game_dir / "bio2 1.10.exe").is_file():
            raise SystemExit("--game (or ARKLAY_RE2_ROOT) must be a retail RE2 install")
        work.mkdir(parents=True, exist_ok=True)
        tree = prepare_game_windows(game_dir, work, sound_out is not None)
        self.capture_started = 0.0
        self.capture: "win.WindowCapture | None" = None
        self.stops = 0
        # `record` sets this when it takes pictures, which want the window on top.
        self.wants_pictures = False
        self.recorder: "win.SoundRecorder | None" = None
        self.process = win.Debuggee(tree / "bio2 1.10.exe", tree)
        win.keep_awake(display=False)
        try:
            self.game = DebugGame(self.process)
            self.game.on_break = self.watch_window
            if sound_out is not None:
                try:
                    self.recorder = win.SoundRecorder(self.process.pid, sound_out)
                except (OSError, RuntimeError) as error:
                    raise SystemExit(f"--sound: {error}") from None
                self.capture_started = self.recorder.started
            # The image is mapped before the process's first event; run the game until it's
            # there, and stop it before any of its code runs a tick.
            deadline = time.monotonic() + 60
            while True:
                try:
                    if self.game.read(BREAK_PAD, len(BREAK_PAD_BYTES)) == BREAK_PAD_BYTES:
                        break
                except OSError:
                    pass
                if time.monotonic() > deadline:
                    raise SystemExit("Pad_set is not the expected code: wrong executable?")
                self.idle(0.01)
        except BaseException:
            self.close()
            raise

    def watch_window(self) -> None:
        """For pictures: once the game's window is up, capture it, and at every stop keep only
        the capture's newest frame, since a full frame pool takes no more."""
        if not self.wants_pictures:
            return
        if self.capture is None:
            self.stops += 1
            if self.stops % 10:
                return
            window = win.game_window(self.process.pid)
            if window is None or win.client_size(window) != (640, 480):
                return
            try:
                self.capture = win.WindowCapture(window)
                # DWM composes nothing with the display off, so keep it on.
                win.keep_awake(display=True)
            except OSError as error:
                raise SystemExit(f"can't capture the game's window: {error}") from None
        self.capture.stopped()

    def screenshot(self, path: Path, within: float | None = None) -> None:
        """The game's 640x480 picture placed on a 1024x768 image at `FRAME_ORIGIN`, as Wine's display has it.

        Until the window is up, a boot frame (which nothing compares) is skipped; a frame of
        play that can't be taken stops the record."""
        from PIL import Image

        try:
            if self.capture is None:
                raise RuntimeError("the game has no window yet")
            width, height, pixels = self.capture.picture()
        except (OSError, RuntimeError) as error:
            if path.stem.startswith("boot"):
                return
            raise SystemExit(f"no picture of tick {path.stem}: {error}") from None
        if (width, height) != (640, 480):
            raise SystemExit(f"the game's picture is {width}x{height}, not 640x480: check config.ini's resolution")
        screen = Image.new("RGB", (1024, 768))
        screen.paste(Image.frombuffer("RGB", (width, height), pixels, "raw", "BGRX", 0, 1), FRAME_ORIGIN)
        screen.save(path, compress_level=1)

    def idle(self, seconds: float) -> None:
        """Let the game run untraced for a while."""
        deadline = time.monotonic() + seconds
        while (left := deadline - time.monotonic()) > 0:
            try:
                event = self.process.next_exception(left)
            except win.GameExited:
                raise SystemExit("the game exited") from None
            if event is not None and self.game.handle(event):
                raise SystemExit("Pad_set's breakpoint is armed outside a trace")

    def frames_written(self) -> None:
        """Each picture is written as it is taken, so a failed one has already stopped the record."""

    def close(self) -> None:
        if self.capture:
            self.capture.close()
        self.process.terminate()
        win.let_sleep()
        if self.recorder:
            self.recorder.close()
            if self.recorder.error:
                print(f"--sound: the capture stopped early: {self.recorder.error}", file=sys.stderr)


Session = WindowsSession if WINDOWS else WineSession


class FilmPads:
    """With the films on (`--config MovieEnable=1`): the game's ticks stand still under a film, which runs on the
    wall clock, so its `Pad_set`s are no ticks. The pad is clear, so it plays to its end. With the films off no stop
    is one."""

    def __init__(self) -> None:
        self.calls = 0
        self.started = 0.0

    def take(self, game: "Game", regs) -> bool:
        """Whether this stop is a film's own `Pad_set`, which is counted and given a clear pad."""
        if game.u32(regs.rsp + FILM_PAD_RETURN_AT) != FILM_PAD_RETURN:
            return False
        if not self.calls:
            self.started = time.monotonic()
        self.calls += 1
        regs.rax = 0
        return True

    def ended(self, where: str, tick: int, pace: "Pace", tick_log) -> None:
        """At the first stop after a film: say how many `Pad_set`s it took, and when it ran in the traced `tick`
        (the one this stop ends), leave its time out of the pace on both sides."""
        if not self.calls:
            return
        print(f"a film ran {self.calls} of its own Pad_sets {where}", flush=True)
        self.calls = 0
        if tick >= 0:
            seconds = time.monotonic() - self.started
            pace.film(seconds)
            if tick_log:
                tick_log.write(f"film {tick} {seconds:.4f}\n")


def record(args: argparse.Namespace, trace_path: Path, work: Path, tick_delay: float = 0.0) -> None:
    """Record `args.ticks` ticks of play from the title route `args.start`, `args.difficulty` and `args.arrange`."""
    route = title_route(args.start, args.difficulty, args.arrange)
    entries = parse_input(Path(args.input) if args.input else None)
    pace = Pace(getattr(args, "pace", None))
    if args.shot_dir:
        Path(args.shot_dir).mkdir(parents=True, exist_ok=True)
    sound_out = str(trace_path.with_suffix(".wav")) if args.sound else None
    trace_path.parent.mkdir(parents=True, exist_ok=True)
    # A prefix keeps the audio driver it was made with.
    work = work.with_name(work.name + "-sound") if args.sound else work
    session = Session(game_root(args), work, sound_out)
    session.wants_pictures = bool(args.shot_dir and args.shot_every)
    # Newlines as on Linux, so that the files are the same byte for byte.
    music_log = open(trace_path.with_suffix(".music"), "w", newline="\n") if args.sound else None
    # When each tick ended, in seconds from the capture's start, to place the
    # ticks in the wall-clock `.wav`.
    tick_log = open(trace_path.with_suffix(".ticks"), "w", newline="\n") if args.sound else None
    last_music = ""
    try:
        with open(trace_path, "w", newline="\n") as trace:
            trace.write("oracle-trace v1 engine=ref\n")
            game = session.game
            game.seize_threads()
            original = game.u8(BREAK_PAD)
            game.write(BREAK_PAD, b"\xcc")
            game.watch_sounds()

            started = time.monotonic()
            boot_tick = 0
            cursor = 0
            pad = 0
            held_keys = 0
            kb_press = KbPress()
            settled = 0
            tick = -1  # The trace's tick, once play has begun.
            films = FilmPads()
            while tick <= args.ticks:
                tid, regs = game.run_to_break()
                if films.take(game, regs):
                    game.resume_from_break(tid, regs, original)
                    continue
                where = f"before tick {tick}" if tick >= 0 else f"before boot tick {boot_tick}"
                films.ended(where, tick - 1 if tick > 0 else -1, pace, tick_log)
                if music_log and music_state(game) != last_music:
                    last_music = music_state(game)
                    music_log.write(f"t={tick} boot={boot_tick} {last_music}\n")
                    music_log.flush()
                if tick < 0:
                    if game.i16(PL + ENTITY_LIFE) != 0:
                        settled += 1
                    if settled > SETTLE_TICKS:
                        game.sounds.clear()
                        tick = 0
                        cursor = 0
                        pad = 0
                        print(f"play begins {boot_tick} ticks after boot")
                        # A route that missed a press would record the wrong game silently.
                        chosen = (
                            game.u32(FG_SYSTEM) & FG_SYSTEM_CHOICE, game.u8(HARD_MODE), game.u8(CENSORSHIP_OFF),
                        )
                        wanted = choice_state(args.difficulty, args.arrange)
                        print("choice: fg_system {:x}, hard_mode {}, censorship_off {}".format(*chosen))
                        if chosen != wanted:
                            raise SystemExit(
                                "the title route chose fg_system {:x}, hard_mode {}, censorship_off {}, not {:x}, {}, {}"
                                .format(*chosen, *wanted)
                            )
                        if music_log:
                            music_log.write(f"bgm_tbl {game.read(BGM_TBL, 284).hex()}\n")
                        for flag in args.flag:
                            set_flag(game, flag)
                        for var in args.var:
                            # Before the warp's door: `save(32, N)` this way makes its `Set_room` change the player.
                            index, value = (int(part, 0) for part in var.split(":"))
                            game.write(SCD_VARIABLES + 2 * index, struct.pack("<H", value))
                        if args.equip_load is not None:
                            # Held before the warp's door, so the room's load
                            # brings in its weapon files.
                            game.write(EQUIPPED, bytes((0, 0, args.equip_load)))
                            game.write(PL + PL_WEAPON, struct.pack("<H", args.equip_load))
                        if args.warp:
                            warp(game, args.warp)
                        for item in args.give:
                            slot, item_id, count = (int(part, 0) for part in item.split(":"))
                            game.write(ITEM_WORK + slot * 4, bytes((item_id, count, 0)))
                        for item in args.box:
                            slot, item_id, count, *mark = (int(part, 0) for part in item.split(":"))
                            game.write(ITEM_BOX_WORK + slot * 4, bytes((item_id, count, mark[0] if mark else 0)))
                        for held in args.file:
                            index, file = (int(part, 0) for part in held.split(":"))
                            game.write(FILE_TAG_BK + index, bytes((file,)))
                        if args.life is not None:
                            game.write(PL + ENTITY_LIFE, struct.pack("<h", args.life))
                if settled and args.equip is not None:
                    game.write(EQUIPPED, bytes((0, 0, args.equip)))
                    game.write(PL + PL_WEAPON, struct.pack("<H", args.equip))
                if (
                    settled
                    and args.no_enemies
                    and args.no_enemies_room in (None, game.u16(ROOM))
                ):
                    room = (game.u16(STAGE) + 1) << 8 | game.u16(ROOM)
                    keep = set(args.keep_enemy) | {
                        slot for kept_room, slot in args.keep_enemy_in if kept_room == room
                    }
                    remove_enemies(game, keep)
                if tick < 0:
                    if boot_tick > BOOT_TICKS:
                        raise SystemExit("the title route did not reach play")
                    if args.shot_dir and args.shot_every and boot_tick % args.shot_every == 0:
                        session.screenshot(Path(args.shot_dir) / f"boot{boot_tick:06d}.png")
                    if not settled:
                        while cursor < len(route) and route[cursor][0] <= boot_tick:
                            pad = route[cursor][1]
                            cursor += 1
                    else:
                        pad = 0
                    boot_tick += 1
                else:
                    # This stop is the end of the tick before it.
                    if tick > 0:
                        pace.reached(tick - 1)
                        line = trace_line(game, tick - 1, pad)
                        if game.u32(TASKS + 4) == TITLE:
                            # Task 0 has left the game for the title (the results' `Result_exit` chains it).
                            line += " " + title_trace(game)
                        trace.write(line + "\n")
                        if tick_log:
                            tick_log.write(f"{tick - 1} {time.monotonic() - session.capture_started:.4f}\n")
                        if args.shot_dir and args.shot_every and (tick - 1) % args.shot_every == 0:
                            within = pace.left(tick) if tick < args.ticks else None
                            session.screenshot(Path(args.shot_dir) / f"t{tick - 1:06d}.png", within)
                    while cursor < len(entries) and entries[cursor][0] <= tick:
                        pad = entries[cursor][1]
                        cursor += 1
                    for item in getattr(args, "give_at", []):
                        # After the warp's load: a `--var 32:N` change of player puts its own items in.
                        at, slot, item_id, count = (int(part, 0) for part in item.split(":"))
                        if at == tick:
                            game.write(ITEM_WORK + slot * 4, bytes((item_id, count, 0)))
                    tick += 1
                    if (pad | held_keys) & KEY_WORD_KEYS:
                        # The keyboard's own keys, as `record_boot` puts them in.
                        game.write(KEY_WORD, struct.pack("<I", pad & KEY_WORD_KEYS))
                    kb_press.tick(game, pad, held_keys)
                    held_keys = pad
                regs.rax = pad & PAD_KEYS
                if tick_delay:
                    game.hold(tid, tick_delay)
                # The run after the last traced tick is no one's to pace.
                if 0 < tick <= args.ticks:
                    pace.release(game, tid, tick - 1)
                game.resume_from_break(tid, regs, original)
            elapsed = time.monotonic() - started
            print(f"{boot_tick + args.ticks} ticks in {elapsed:.1f}s")
            pace.report()
    finally:
        session.close()
    session.frames_written()
    # `scenario`, `pixels` and `determinism` record through here too, without the option.
    if getattr(args, "keep_saves", None):
        keep_saves(work / "game", Path(args.keep_saves))


def read_pace(path: Path) -> dict[int, float]:
    """A `--sound` recording's `.ticks` log: each traced tick and when it was reached, in seconds."""
    pace = {}
    for line in path.read_text().splitlines():
        if line.startswith("film "):
            continue
        tick, seconds = line.split()
        pace[int(tick)] = float(seconds)
    return pace


def read_films(path: Path) -> dict[int, float]:
    """A films-on recording's `.ticks` log's films: the tick each ran in (`film TICK SECONDS`) and how long it
    took on the wall clock, which that tick's end includes."""
    films = {}
    for line in path.read_text().splitlines():
        if line.startswith("film "):
            _, tick, seconds = line.split()
            films[int(tick)] = films.get(int(tick), 0.0) + float(seconds)
    return films


class Pace:
    """Follows a stored recording's `.ticks` (`--pace`), so that the game's thread runs each tick when it ran
    it in that recording, with the game's other threads running meanwhile.

    What a voice line's ticks depend on is when the thread runs: DirectSound's clock is read inside a tick.
    The log holds when each tick ended, which is one run after the thread was let go (the stored recording
    took its picture in between, with the thread held). So the thread is let go for the run that ends a tick
    at that tick's stored time less what a run takes here (the median of the last `RUNS`), counted from the
    first tick the two recordings share: the tick then ends when it did there, however long this recorder's
    picture takes. The picture itself is taken as soon as the tick ends, before the wait.

    A tick the log doesn't hold has the time its neighbours in the log give it: between two of them in
    proportion, and past the last one the log's median tick a tick (a record longer than the stored one). A
    record can only wait for the stored pace, not catch up with it, so the ticks let go late are counted: a
    voice line that ends in one of them may hold for a tick less than it did in the stored recording. A run
    that takes much longer than the ones before it (a room's load) is late too, and the ticks after it until
    the record has caught up.

    A film (with the films on) runs on the wall clock inside its tick, in both recordings: the stored log's
    films are taken out of its times, and this record's out of its own clock (`film`), so neither is waited
    for nor counted as a run."""

    # Later than this is a tick's worth at the pace `import` gave.
    LATE = 0.25
    RUNS = 9

    def __init__(self, path: str | None, clock=time.monotonic) -> None:
        if path == "":
            raise SystemExit("--pace: no log is named")
        self.ticks = read_pace(Path(path)) if path else {}
        if path and not self.ticks:
            raise SystemExit(f"--pace: {path} holds no ticks")
        self.films = read_films(Path(path)) if path else {}
        # The stored times with the films up to each tick taken out, which the ticks between are worked out from.
        self.free = {
            tick: at - sum(seconds for film, seconds in self.films.items() if film <= tick)
            for tick, at in self.ticks.items()
        }
        self.clock = clock
        self.known = sorted(self.ticks)
        steps = sorted(
            (self.free[after] - self.free[before]) / (after - before)
            for before, after in zip(self.known, self.known[1:])
        )
        self.step = steps[len(steps) // 2] if steps else 0.0
        # This record's clock at the stored recording's 0.
        self.origin: float | None = None
        self.released: float | None = None
        self.runs: list[float] = []
        # The record's ticks the log holds, and the runs let go by a stored time and by one worked out.
        self.shared = 0
        self.stored = 0
        self.stepped = 0
        self.late = 0
        self.latest = 0.0

    def time(self, tick: int) -> float:
        """The stored time of `tick`'s end less the stored films' up to it, or the one its neighbours in the log
        give it."""
        if tick in self.free:
            return self.free[tick]
        at = bisect.bisect_left(self.known, tick)
        if at == 0:
            first = self.known[0]
            return self.free[first] - (first - tick) * self.step
        before = self.known[at - 1]
        if at == len(self.known):
            return self.free[before] + (tick - before) * self.step
        after = self.known[at]
        part = (tick - before) / (after - before)
        return self.free[before] + part * (self.free[after] - self.free[before])

    def film(self, seconds: float) -> None:
        """A film of `seconds` ran in the tick whose end is the next `reached`: it's no wait and no run."""
        if self.origin is not None:
            self.origin += seconds
        if self.released is not None:
            self.released += seconds

    def reached(self, tick: int) -> None:
        """The break that ends `tick`."""
        now = self.clock()
        if self.released is not None:
            self.runs = (self.runs + [now - self.released])[-self.RUNS:]
        if tick in self.ticks:
            self.shared += 1
            if self.origin is None:
                self.origin = now - self.time(tick)

    def run(self) -> float:
        """What a run takes here."""
        return sorted(self.runs)[len(self.runs) // 2] if self.runs else 0.0

    def left(self, tick: int) -> float | None:
        """The seconds until the run that ends `tick` is due, which the picture of the tick before has to
        fit in; None with no pace to keep."""
        if self.origin is None:
            return None
        return self.origin + self.time(tick) - self.run() - self.clock()

    def release(self, game: "Game", tid: int, tick: int) -> None:
        """Hold the thread at its break until the run that ends `tick` is due."""
        if self.origin is not None:
            if tick in self.ticks:
                self.stored += 1
            else:
                self.stepped += 1
            due = self.origin + self.time(tick) - self.run()
            if (wait := due - self.clock()) > 0:
                game.hold(tid, wait)
            # After the wait, so that a hold that overruns counts as well.
            if (over := self.clock() - due) > self.LATE:
                self.late += 1
                self.latest = max(self.latest, over)
        self.released = self.clock()

    def report(self) -> None:
        if not self.ticks:
            return
        if not self.shared:
            raise SystemExit("--pace: the log holds none of this record's ticks, so it set its own pace")
        more = f", and {self.stepped} more by its neighbours' (its median tick is {self.step:.3f}s)" if self.stepped else ""
        late = f"{self.late} ticks more than {self.LATE}s late, {self.latest:.2f}s at most" if self.late else "no tick late"
        print(f"followed the stored pace for {self.stored} ticks{more}: {late}")


def title_trace(game: Game) -> str:
    """The title's state (`Title`, `0x5035B0`): its task's steps, the menu's and the fade's, and what the new-game
    choice has set."""
    task = game.read(TASKS, 0x14)
    fade = game.read(FADE, 0x10)
    kido, add = struct.unpack_from("<hh", fade, 0)
    return (
        f"ti={game.u8(TITLE_MV_STATE)},{game.u8(TITLE_DISP)},{task[8]},{task[9]},"
        f"{game.u8(TITLE_MENU)},{game.u8(TITLE_MENU + 1)},{game.u8(TITLE_SEL_MODE)},{game.u8(TITLE_CUR)},"
        f"{game.u8(TITLE_PULSE)},{game.u8(TITLE_SLIDE)},{game.u16(TITLE_DEMO)} "
        f"tlogo={game.i16(TITLE_FLASH)},{game.i16(TITLE_LOGO)},{game.i32s(TITLE_LOGO_COUNT, 1)[0]} "
        f"tfade={kido},{add},{fade[4]},{game.u16(TITLE_KIDO)},{game.u16(TITLE_ADD)} "
        f"tsel={game.u32(FG_SYSTEM) & FG_SYSTEM_CHOICE:x},{game.u8(HARD_MODE)},{game.u8(CENSORSHIP_OFF)} "
        f"tx={task[0xA]},{game.u8(TITLE_MENU_2)},{game.u8(TITLE_DIM)},{game.u8(TITLE_B8C)},"
        f"{game.u8(TITLE_EF2)},{game.u8(TITLE_EF2 + 1)},{game.u8(EX_LEVEL)},"
        f"{game.u32(FG_SYSTEM) & 0x01020009:x},{game.u32(FG_STATUS) & 0x02004000:x} "
        f"tfade2={fade_slot(game, 2)} tfade3={fade_slot(game, 3)} opt={option_trace(game)} {prologue_trace(game)}"
    )


def prologue_trace(game: Game) -> str:
    """The prologue's state while its task runs (`Prologue`, `0x4CEFF0`). `pro`: the task's three steps, the
    sequence, its step and count, the routine, the fade's flags and slot, the skip's gate and state, the frames
    drawn, the scroll's and the zoom's waits and the sprites' mask. `prs`: each sprite's mode, the low byte of its
    colour and its y. `prz`: the two zooming pictures' sizes."""
    task = TASKS + 0x24
    if game.u32(task + 4) != PROLOGUE:
        return "pro=- prs=- prz=-"
    steps = game.read(task + 8, 3)
    state = (
        f"{steps[0]},{steps[1]},{steps[2]},{game.u16(PROLOGUE_MV)},{game.u16(PROLOGUE_STEP)},"
        f"{game.u16(PROLOGUE_COUNT)},{game.u8(PROLOGUE_R)},{game.u8(PROLOGUE_FADE_FLAGS)},"
        f"{game.u8(PROLOGUE_FADE_SLOT)},{game.u8(PROLOGUE_SKIP_OK)},{game.u8(PROLOGUE_SKIP)},"
        f"{game.u32(PROLOGUE_FRAMES)},{game.u16(PROLOGUE_SCROLL_WAIT)},{game.u16(PROLOGUE_ZOOM_WAIT)},"
        f"{game.u32(PROLOGUE_MASK):x}"
    )
    table, count = game.u32(PROLOGUE_SPRITES), game.u16(PROLOGUE_SPRITE_COUNT)
    sprites = "-"
    if table and 0 < count <= 0x40:
        records = game.read(table, count * 12)
        sprites = ",".join(
            f"{records[i]:02x}{records[i + 8]:02x}:{struct.unpack_from('<h', records, i + 6)[0]}"
            for i in range(0, len(records), 12)
        )
    sizes = ",".join(str(value) for entry in (4, 9) for value in game.read(INTROMAP_GFX + entry * 10 + 8, 2))
    return f"pro={state} prs={sprites} prz={sizes}"


def option_trace(game: Game) -> str:
    """The option screen's state while its task runs (`Config_main`, `0x44A0B0`): the task's step, the state, the
    menu's row, the sound page's row, a change's wait, the speaker mode, the sound quality (`Config_get_squal`), the
    two volumes and the arrows' five blink counts."""
    task = TASKS + 0x24
    if game.u32(task + 4) not in OPTION_TASKS:
        return "-"
    frequency, depth = game.u32(SOUND_FREQUENCY), game.u32(SOUND_DEPTH)
    quality = {(0xAC44, 16): 0, (0x5622, 16): 1, (0x5622, 8): 2}.get((frequency, depth), -1)
    blinks = ",".join(str(game.u32(address)) for address in OPTION_BLINKS)
    return (
        f"{game.u8(task + 8)},{game.u8(OPTION_STATE)},{game.u32(OPTION_CURSOR)},{game.u32(OPTION_TYPE)},"
        f"{game.u32(OPTION_WAIT)},{game.u32(SPEAKER_CONFIG)},{quality},{game.u8(BGM_VOL)},{game.u8(SFX_VOL)},{blinks}"
    )


def key_config_trace(game: Game) -> str:
    """KEYCONFIG's page while the option task runs (`Config_main`'s states 2 and 9 to 0xF), as three fields. `kc`
    is the page's own work: state 9's count (`0x99CF60`), the panel's offset (`0x99CF34`) and the choice on the
    last row or under the question (`0x6893C8`), then the key table being edited (`0x99CF40`, a virtual key for
    each of the input class's 32 keys). `kcj` is the joypad table being edited (`0x99CF80`, 32 words). `kcm` is the
    model's: its work's four routine bytes, place, turn, four motion bytes and weapon type, then whether it has
    turned (`0x6893D4`), whether it is turning back (`0x6893CC`), and `FUN_0044b3c0`'s pseudo keys (`0x689384`),
    count (`0x6893A0`), step (`0x6893D8`), kept step (`0x6893DC`) and row (`0x525AB4`)."""
    if game.u32(TASKS + 0x24 + 4) not in OPTION_TASKS:
        return "kc=- kcj=- kcm=-"
    page = ",".join(str(value) for value in (*game.i32s(KEY_COUNT, 1), *game.i32s(KEY_SLIDE, 1), game.u8(KEY_CHOICE)))
    joypad = ",".join(f"{value & 0xFFFFFFFF:x}" for value in game.i32s(KEY_JOYPAD_TEMP, 32))
    model = "-"
    work = game.u32(KEY_MODEL)
    if work:
        place = ",".join(str(value) for value in game.i32s(work + ENTITY_POS, 3))
        demo = ",".join(str(value) for value in game.i32s(KEY_DEMO_COUNT, 1) + game.i32s(KEY_DEMO_STEP, 2))
        model = (
            f"{game.read(work + ENTITY_ROUTINES, 4).hex()},{place},{game.u16(work + ENTITY_CDIR_Y)},"
            f"{game.read(work + ENTITY_MOTION, 4).hex()},{game.u16(work + PL_WEAPON)},{game.u8(KEY_TURNED)},"
            f"{game.u32(KEY_TURNING)},{game.u32(KEY_DEMO_KEYS):x},{demo},{game.i32s(KEY_DEMO_ROW, 1)[0]}"
        )
    return f"kc={page}/{game.read(KEY_TABLE_TEMP, 32).hex()} kcj={joypad} kcm={model}"


def gallery_trace(game: Game) -> str:
    """The gallery's state while its task runs with its work (`Gallery`, `0x4BF760`, which makes the work on the
    tick it starts and frees it on the tick it ends): the task's step, the screen (`GalleryLoop`'s case), the
    screen's step and row, the lit row's level, the fade's level and the flags."""
    task = TASKS + 0x24
    work = game.u32(GALLERY)
    if game.u32(task + 4) != GALLERY_TASK or not work:
        return "-"
    screen, step, row = game.i32s(work, 3)
    return (
        f"{game.u8(task + 8)},{screen},{step},{row},{game.i16(work + 0x1468)},{game.i16(work + 0x1474)},"
        f"{game.u32(work + 0x1478):x}"
    )


CR_GALLERY_FLAGS = (0xFFFFFFFF, 0xFFFFFFF0, 0xFFFFFFFE, 0xFFFFFFFF, 0xFFFFFFFF)
GALLERY_HELD: dict[int, tuple[int, bool]] = {}


def held_gallery_word(held: int, put_back: bool, now: int, classic: int) -> tuple[int, bool, bool]:
    """A gallery word's next `(held, put_back)` and whether `held` is to be written, for a word that reads `now`.
    Classic REbirth's write is put back once; after it, and wherever the word isn't Classic REbirth's, bits set
    over the held ones are the game's own and are kept, an all-opened word among them."""
    if now == held or now & held != held:
        return held, put_back, False
    if now == classic and not put_back:
        return held, True, True
    return now, put_back, False


def hold_gallery_flags(game: Game) -> None:
    """Put back the gallery's flags that `--config GALLERYn=...` set. Classic REbirth opens all of the gallery as
    the title boots (the five words become ffffffff, fffffff0, fffffffe, ffffffff and ffffffff on boot tick 34,
    whatever config.ini had), so a record of a gallery with less opened writes a word again when it first finds
    Classic REbirth's there. The game sets bits of its own (`Set_registry_flg`, `0x442EA0`: `Init_movie_work` opens
    the film it plays, the gallery's too), and a word with more set than the record's keeps them."""
    for n in range(5):
        value = CONFIG_OVERRIDES.get(f"GALLERY{n}")
        if value is None:
            continue
        held, put_back = GALLERY_HELD.get(n, (int(value, 16), False))
        held, put_back, write = held_gallery_word(held, put_back, game.u32(GALLERY_FLAGS + 4 * n), CR_GALLERY_FLAGS[n])
        GALLERY_HELD[n] = (held, put_back)
        if write:
            game.write(GALLERY_FLAGS + 4 * n, struct.pack("<I", held))


def gallery_view_trace(game: Game) -> str:
    """The gallery's pictures (`GalleryIllustrations`, `0x438DA0`), while `gallery_trace` has a state: the
    picture's index and the count of the textures it is cut into, which is 0 until the tick that loads it."""
    work = game.u32(GALLERY)
    if game.u32(TASKS + 0x24 + 4) != GALLERY_TASK or not work:
        return "-"
    return f"{game.i16(work + 0x146A)},{game.i32s(work + 0x1470, 1)[0]}"


def fade_slot(game: Game, slot: int) -> str:
    """A fade slot's level, step and mode (`Fade_set`)."""
    fade = game.read(FADE + slot * FADE_SLOT, 5)
    level, add = struct.unpack_from("<hh", fade, 0)
    return f"{level},{add},{fade[4]}"


def boot_trace_line(game: Game, tick: int, pad: int) -> str:
    """One tick from boot. Before play: the title's state, the sound and the music. In play (task 0 runs
    `Game_loop`): the game's line, with the title's fields after it as they were left."""
    if game.u32(TASKS + 4) == GAME_LOOP:
        return f"{trace_line(game, tick, pad)} {title_trace(game)}"
    fields = [
        f"t={tick}",
        f"room={game.u16(STAGE) + 1}:{game.u16(ROOM)}",
        f"pad={pad & 0xFFFFFFFF}",
        # `Rnd`'s state, which the gallery's models draw from as they load.
        f"rng={game.u32(RANDOM) & 0xFFFF}",
        f"fgsys={game.u32(FG_SYSTEM):08x}",
        title_trace(game),
        save_screen_trace(game),
        # The list's name scroll (`0x986384`), which left and right step.
        f"svs={game.i32s(CARD_SCROLL, 1)[0]}",
        f"svb={(game.u32(CARD_STATE) >> 16) & 0xFF}",
        music_trace(game),
        "snd=" + ("/".join(game.sounds) or "-"),
        "gal=" + gallery_trace(game),
        "galv=" + gallery_view_trace(game),
        key_config_trace(game),
        result_trace(game),
    ]
    game.sounds.clear()
    return " ".join(fields)


def record_boot(args: argparse.Namespace, trace_path: Path, work: Path) -> None:
    """Record `args.ticks` ticks from boot, the title's included: `--input`'s ticks count from boot."""
    if getattr(args, "keep_saves", None):
        raise SystemExit("--keep-saves isn't supported with --boot yet")
    entries = parse_input(Path(args.input) if args.input else None)
    pace = Pace(getattr(args, "pace", None))
    if args.shot_dir:
        Path(args.shot_dir).mkdir(parents=True, exist_ok=True)
    sound_out = str(trace_path.with_suffix(".wav")) if args.sound else None
    trace_path.parent.mkdir(parents=True, exist_ok=True)
    session = Session(game_root(args), work.with_name(work.name + "-sound") if args.sound else work, sound_out)
    session.wants_pictures = bool(args.shot_dir and args.shot_every)
    tick_log = open(trace_path.with_suffix(".ticks"), "w", newline="\n") if args.sound else None
    try:
        with open(trace_path, "w", newline="\n") as trace:
            trace.write("oracle-trace v1 engine=ref\n")
            game = session.game
            game.seize_threads()
            original = game.u8(BREAK_PAD)
            game.write(BREAK_PAD, b"\xcc")
            game.watch_sounds()
            started = time.monotonic()
            cursor = 0
            pad = 0
            held_keys = 0
            kb_press = KbPress()
            tick = 0
            films = FilmPads()
            while tick <= args.ticks:
                tid, regs = game.run_to_break()
                if films.take(game, regs):
                    game.resume_from_break(tid, regs, original)
                    continue
                films.ended(f"before boot tick {tick}", tick - 1 if tick > 0 else -1, pace, tick_log)
                # This stop is the end of the tick before it.
                if tick > 0:
                    pace.reached(tick - 1)
                    trace.write(boot_trace_line(game, tick - 1, pad) + "\n")
                    if tick_log:
                        tick_log.write(f"{tick - 1} {time.monotonic() - session.capture_started:.4f}\n")
                    if args.shot_dir and args.shot_every and (tick - 1) % args.shot_every == 0:
                        within = pace.left(tick) if tick < args.ticks else None
                        session.screenshot(Path(args.shot_dir) / f"t{tick - 1:06d}.png", within)
                while cursor < len(entries) and entries[cursor][0] <= tick:
                    pad = entries[cursor][1]
                    cursor += 1
                hold_gallery_flags(game)
                tick += 1
                if (pad | held_keys) & KEY_WORD_KEYS:
                    # After `FUN_0043bb00` wrote the word for this tick, before `Config_main` reads it.
                    game.write(KEY_WORD, struct.pack("<I", pad & KEY_WORD_KEYS))
                kb_press.tick(game, pad, held_keys)
                held_keys = pad
                regs.rax = pad & PAD_KEYS
                if tick <= args.ticks:
                    pace.release(game, tid, tick - 1)
                game.resume_from_break(tid, regs, original)
            print(f"{args.ticks} ticks from boot in {time.monotonic() - started:.1f}s")
            pace.report()
    finally:
        session.close()
    session.frames_written()


def game_root(args: argparse.Namespace) -> Path:
    return Path(args.game or os.environ.get("ARKLAY_RE2_ROOT", ""))


def record_command(args: argparse.Namespace) -> int:
    if args.boot:
        record_boot(args, Path(args.trace), Path(args.work).resolve())
        return 0
    record(args, Path(args.trace), Path(args.work).resolve(), args.tick_delay)
    return 0


def determinism_command(args: argparse.Namespace) -> int:
    """Record twice, once on a slowed clock: only ticks may decide what happens."""
    out = Path(args.out)
    work = Path(args.work).resolve()
    record(args, out / "first.trace", work)
    record(args, out / "slowed.trace", work, tick_delay=0.02)
    first = (out / "first.trace").read_text().splitlines()
    slowed = (out / "slowed.trace").read_text().splitlines()
    differing = [i for i, (a, b) in enumerate(zip(first, slowed)) if a != b]
    if len(first) == len(slowed) and not differing:
        print(f"deterministic: {len(first) - 1} ticks identical")
        return 0
    print(f"NOT deterministic: {len(differing)} ticks differ")
    for index in differing[:3]:
        changed = [(a, b) for a, b in zip(first[index].split(), slowed[index].split()) if a != b]
        print(f"  line {index}: {changed[:6]}")
    return 1


def scenario_command(args: argparse.Namespace) -> int:
    """Record the original, run arklay on the same input, and compare."""
    repo = Path(__file__).resolve().parent.parent
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    ref_trace = out / "ref.trace"
    ark_trace = out / "ark.trace"
    record(args, ref_trace, Path(args.work).resolve())
    run_arklay(args, out, arklay_driver(args, ark_trace.resolve(), args.input))
    return subprocess.run(
        [sys.executable, str(repo / "scripts" / "oracle.py"), "compare", str(ref_trace),
         str(ark_trace), "--offset", "0", "--fields", args.fields, "--first", "3",
         "--context", "2"],
        check=False,
    ).returncode


def pixels_command(args: argparse.Namespace) -> int:
    """Record the original and arklay on the same input and compare their frames."""
    from PIL import Image, ImageChops, ImageStat

    if args.offset % args.every:
        print(f"--offset {args.offset} isn't a multiple of --every {args.every}: "
              "arklay's frames wouldn't be the reference's", file=sys.stderr)
        return 2
    out = Path(args.out).resolve()
    ref_dir = out / "ref"
    ark_dir = out / "ark"
    cmp_dir = out / "cmp"
    for directory in (ark_dir, cmp_dir):
        directory.mkdir(parents=True, exist_ok=True)
    args.shot_dir = str(ref_dir)
    args.shot_every = args.every
    if not args.reuse_ref:
        record(args, out / "ref.trace", Path(args.work).resolve())
    run_arklay(args, out, arklay_driver(args, out / "ark.trace", args.arklay_input or args.input, ark_dir,
                                        args.every, args.arklay_room))

    def regions(diff: "Image.Image") -> str:
        """The worst of a 4x3 grid, which says where to look."""
        cells = []
        for row in range(3):
            for column in range(4):
                box = (column * 80, row * 80, (column + 1) * 80, (row + 1) * 80)
                cells.append((sum(ImageStat.Stat(diff.crop(box)).mean) / 3, column, row))
        error, column, row = max(cells)
        return f"cell {column},{row} at {error:.1f}"

    scores = []
    for ref_path in sorted(ref_dir.glob("t*.png")):
        tick = int(ref_path.stem[1:]) + args.offset
        ark_path = ark_dir / f"f{tick // args.every * args.every:05d}.bmp"
        if not ark_path.exists():
            ark_path = ark_dir / f"f{tick // args.every:05d}.bmp"
        if not ark_path.exists():
            continue
        left, top = FRAME_ORIGIN
        ref = Image.open(ref_path).convert("RGB").crop((left, top, left + 640, top + 480))
        ref = ref.resize((320, 240), Image.BOX)
        ark = Image.open(ark_path).convert("RGB")
        diff = ImageChops.difference(ref, ark)
        error = sum(ImageStat.Stat(diff).mean) / 3
        scores.append((error, tick, regions(diff)))
        sheet = Image.new("RGB", (960, 240))
        sheet.paste(ref, (0, 0))
        sheet.paste(ark, (320, 0))
        sheet.paste(diff.point(lambda value: min(255, value * 4)), (640, 0))
        sheet.save(cmp_dir / f"t{tick:06d}.png")
    if not scores:
        print("no frames to compare")
        return 1
    scores.sort(reverse=True)
    mean = sum(score[0] for score in scores) / len(scores)
    print(f"{len(scores)} frames, mean error {mean:.2f}, worst:")
    for error, tick, where in scores[:args.show]:
        print(f"  t={tick} error {error:.2f}, {where}: {cmp_dir / f't{tick:06d}.png'}")
    return 0 if scores[0][0] <= args.max_error else 1


def patches_command(args: argparse.Namespace) -> int:
    """List the code Classic Rebirth changes, with the function each change is in."""
    game_dir = game_root(args)
    image = (game_dir / "bio2 1.10.exe").read_bytes()
    pe = struct.unpack_from("<I", image, 0x3C)[0]
    optional_size = struct.unpack_from("<H", image, pe + 20)[0]
    base = struct.unpack_from("<I", image, pe + 24 + 28)[0]
    virtual_size, virtual, raw_size, raw = struct.unpack_from(
        "<IIII", image, pe + 24 + optional_size + 8
    )
    size = min(virtual_size, raw_size)
    session = Session(game_dir, Path(args.work).resolve())
    try:
        session.idle(args.wait)
        live = session.game.read(base + virtual, size)
    finally:
        session.close()
    disk = image[raw:raw + size]
    index = 0
    while index < size:
        if live[index] == disk[index]:
            index += 1
            continue
        end = index
        # Join changes closer than a pointer's width: one patched instruction.
        while end < size and (live[end] != disk[end] or live[end + 1:end + 5] != disk[end + 1:end + 5]):
            end += 1
        print(f"{base + virtual + index:08x} {end - index:3} {disk[index:end].hex()} -> {live[index:end].hex()}")
        index = end
    return 0


def build_parser() -> argparse.ArgumentParser:
    """The command line's parser: the regression runner reads a record's arguments with it too."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    def common(command: argparse.ArgumentParser) -> None:
        command.add_argument("--game", default=None)
        command.add_argument("--work", default="target/oracle-re2/run")
        command.add_argument("--start", choices=sorted(CHARACTERS), default="leon")
        command.add_argument(
            "--difficulty", choices=DIFFICULTIES, default="normal",
            help="the new game's difficulty: NORMAL, EASY, or HARD (`hard_mode`)",
        )
        command.add_argument(
            "--arrange", action="store_true",
            help="ARRANGE MODE rather than ORIGINAL MODE (`censorship_off`, `fg_system`'s 0x40)",
        )
        command.add_argument(
            "--config", action="append", default=[], metavar="KEY=VALUE",
            help="set a key of the game's config.ini for this run (repeatable), such as SPECIAL=0x1 (the game reads its numbers in hex, 0x first); with --boot a GALLERYn is also held in memory, past Classic REbirth's opening of the gallery",
        )
        command.add_argument("--input", default=None)
        command.add_argument("--shot-dir", default=None)
        command.add_argument("--shot-every", type=int, default=0)
        command.add_argument(
            "--sound", action="store_true",
            help="run with sound into a private sink: writes the audio and the music's log beside the trace",
        )
        command.add_argument("--no-enemies", action="store_true")
        command.add_argument(
            "--no-enemies-room", type=lambda text: int(text, 0), default=None
        )
        command.add_argument("--keep-enemy", action="append", type=int, default=[])
        command.add_argument(
            "--keep-enemy-in", action="append", default=[], metavar="ROOM:SLOT",
            type=lambda text: (int(text.split(":")[0], 16), int(text.split(":")[1])),
            help="with --no-enemies, leave this slot running in this room only (three hex digits)",
        )
        command.add_argument("--flag", action="append", default=[], metavar="BANK:INDEX")
        command.add_argument(
            "--var", action="append", default=[], metavar="INDEX:VALUE",
            help="when play begins, before --warp, set a script variable (`save`'s numbering): --var 32:14 makes the warp's door load Ada as the player",
        )
        command.add_argument(
            "--warp", default=None, metavar="ROOM:X:Y:Z:ANGLE[:CUT]",
            help="when play begins, go through a door to a room (three hex digits), a position and a cut",
        )
        command.add_argument(
            "--give", action="append", default=[], metavar="SLOT:ID:COUNT",
            help="put an item in the inventory when play begins",
        )
        command.add_argument(
            "--give-at", action="append", default=[], metavar="TICK:SLOT:ID:COUNT",
            help="put an item in the inventory on this trace tick, after the warp's load (the items of a player --var 32:N makes)",
        )
        command.add_argument(
            "--box", action="append", default=[], metavar="SLOT:ID:COUNT[:MARK]",
            help="put an item in the item box when play begins; MARK 3 for a two-slot weapon",
        )
        command.add_argument(
            "--file", action="append", default=[], metavar="INDEX:FILE",
            help="hold a file when play begins: File_tag_bk[INDEX] = FILE (binder INDEX / 8, slot INDEX mod 8)",
        )
        command.add_argument(
            "--equip", type=lambda text: int(text, 0), default=None, metavar="ID",
            help="hold this weapon from the moment play is settled; give its slot with --give",
        )
        command.add_argument(
            "--equip-load", type=lambda text: int(text, 0), default=None, metavar="ID",
            help="hold this weapon before --warp's door; give its slot with --give. The player keeps the handgun's clips (PLW02), so use the status screen's EQUIP for a weapon whose clips differ",
        )
        command.add_argument(
            "--life", type=int, default=None, metavar="N",
            help="set the player's life when play begins",
        )
        command.add_argument(
            "--saves", default=None, metavar="DIR",
            help="start with these files in the game's save folder (else it is empty)",
        )
        command.add_argument(
            "--pace", default=None, metavar="TICKS_LOG",
            help="with --sound, run each tick no sooner than the recording whose .ticks log this is did "
            "(the regression runner gives a reference's own when it records it again); read before "
            "anything is written, so it may be the log this record writes",
        )

    command = commands.add_parser("record", help="record a trace from the original")
    common(command)
    command.add_argument("--ticks", type=int, required=True)
    command.add_argument("--trace", required=True)
    command.add_argument("--tick-delay", type=float, default=0.0)
    command.add_argument(
        "--boot", action="store_true",
        help="trace from boot, the title included: --input's ticks count from boot, and no title route is played",
    )
    command.add_argument(
        "--keep-saves", default=None, metavar="DIR",
        help="copy the save folder the game left to DIR when the record ends",
    )
    command.set_defaults(func=record_command)

    command = commands.add_parser("scenario", help="record the original, run arklay, compare")
    common(command)
    command.add_argument("--ticks", type=int, required=True)
    command.add_argument("--out", required=True)
    command.add_argument("--arklay", default="target/fast-release/arklay")
    command.add_argument("--pack", default="re2.akpak")
    command.add_argument("--fields", default=RE2_FIELDS)
    command.set_defaults(func=scenario_command)

    command = commands.add_parser("pixels", help="compare the original's and arklay's frames")
    common(command)
    command.add_argument("--ticks", type=int, required=True)
    command.add_argument("--out", required=True)
    command.add_argument("--every", type=int, default=30)
    command.add_argument("--offset", type=int, default=0, help="arklay's tick for a reference tick")
    command.add_argument("--max-error", type=float, default=20.0)
    command.add_argument("--show", type=int, default=8)
    command.add_argument("--reuse-ref", action="store_true", help="keep the recorded reference frames")
    command.add_argument("--arklay", default="target/fast-release/arklay")
    command.add_argument("--pack", default="re2.akpak")
    command.add_argument(
        "--arklay-input", default=None,
        help="arklay's own input when its ticks are offset from the reference's",
    )
    command.add_argument(
        "--arklay-room", default=None, metavar="ROOM",
        help="start arklay's new game in this room instead of the start room",
    )
    command.set_defaults(func=pixels_command)

    command = commands.add_parser("determinism", help="check two recordings are identical")
    common(command)
    command.add_argument("--ticks", type=int, required=True)
    command.add_argument("--out", required=True)
    command.set_defaults(func=determinism_command)

    command = commands.add_parser("patches", help="list Classic Rebirth's code changes")
    command.add_argument("--game", default=None)
    command.add_argument("--work", default="target/oracle-re2/run")
    command.add_argument("--wait", type=float, default=15.0)
    command.set_defaults(func=patches_command)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    # SIGTERM (from `timeout`, `docker stop` or a queue) and SIGHUP end the record
    # as an error does, so that every `finally` runs and the session stops its
    # display and Wine.
    def stop(signum: int, frame: object) -> None:
        raise SystemExit(f"stopped by signal {signum}")
    for name in ("SIGTERM", "SIGHUP"):
        if hasattr(signal, name):
            signal.signal(getattr(signal, name), stop)
    global SAVES_FROM
    if getattr(args, "saves", None):
        SAVES_FROM = Path(args.saves).resolve()
    for setting in getattr(args, "config", []):
        key, _, value = setting.partition("=")
        CONFIG_OVERRIDES[key.strip()] = value.strip()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
