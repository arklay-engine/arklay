#!/usr/bin/env python3
"""Record a trace from RE2's original executable, for the differential oracle.

RE2 has no reference source to build a tracer into, so this runs ``bio2 1.10``
under Wine and traces it from outside. It stops the game once per tick at the
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
import ctypes
import os
import signal
import struct
import subprocess
import sys
import time
from pathlib import Path

# `Pad_set`, on the instruction after its `CALL` to the pad read: EAX holds this
# tick's raw pad value and nothing has used it yet.
BREAK_PAD = 0x004D0F58
# Classic Rebirth doesn't patch this code. Refuse to run if it isn't what we expect.
BREAK_PAD_BYTES = bytes.fromhex("8B0D6C9E9800")  # MOV ECX,[fg_system]

# Addresses and offsets in bio2 1.10, named as in the IDA database and openre.
PL = 0x00989EF0  # the player's work
ENEMIES = 0x0098A114  # EnemyEntity*[16]
FG_SYSTEM = 0x00989E6C
FG_STATUS = 0x00989ED0
FG_STOP = 0x00989ED4
FG_SCENARIO = 0x0098EB2C
FG_COMMON = 0x0098EB4C
FG_ROOM = 0x0098EB6C
STAGE = 0x0098EB14
ROOM = 0x0098EB16
CUT = 0x0098EB18
RANDOM = 0x00988610  # `Rnd`'s state

ENTITY_ROUTINES = 0x04
ENTITY_ID = 0x08
ENTITY_POS = 0x38  # m.t: x, y, z as i32
ENTITY_CDIR_Y = 0x76
ENTITY_LIFE = 0x156
ENTITY_BE_FLG = 0x00
ENTITY_SPD = 0x144  # x, y, z as i16
ENTITY_MOTION = 0x14C  # move_no, move_cnt, hokan_flg, mplay_flg
ENTITY_D_LIFE_U = 0x151

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
}

BOOT_TICKS = 3000
# The ticks from the first with a live player to the first of play: `Pl_init`,
# then the room's load collision pass, then the tick the load's `fg_stop` ends.
SETTLE_TICKS = 3
# The title screen to the first tick of play, as `(tick, raw pad)` from the first
# tick the tool sees. The game is deterministic, so fixed ticks are enough.
def press(tick: int, button: str) -> list[tuple[int, int]]:
    return [(tick, BUTTONS[button]), (tick + 3, 0)]


TITLE = press(200, "down") + press(240, "confirm")
ROUTES = {
    "leon": TITLE + [entry for tick in range(400, BOOT_TICKS, 60) for entry in press(tick, "confirm")],
    "claire": TITLE + press(340, "right") + [
        entry for tick in range(400, BOOT_TICKS, 60) for entry in press(tick, "confirm")
    ],
}
CHARACTERS = {"leon": 0, "claire": 1}
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
PR_SET_CHILD_SUBREAPER = 36

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
            if word not in BUTTONS:
                raise SystemExit(f"{path}: unknown button {word!r}")
            pad |= BUTTONS[word]
        entries.append((int(words[0]), pad))
    return sorted(entries, key=lambda entry: entry[0])


def prepare_game(game: Path, work: Path) -> Path:
    """A writable copy of the install's settings, with links to its read-only data."""
    tree = work / "game"
    (tree / "savedata").mkdir(parents=True, exist_ok=True)
    for name in ("bio2 1.10.exe", "ddraw.dll", "data"):
        link = tree / name
        if link.is_symlink() or link.exists():
            link.unlink()
        link.symlink_to(game / name)
    settings = (game / "config.ini").read_text(errors="replace")
    # A fixed config: no dialogs or sound device to wait on, none of Classic
    # Rebirth's optional gameplay changes, no optional art.
    fixed = {
        "BootConfig": "0", "MovieEnable": "0", "DirectSoundEnable": "0",
        "QuickturnEnable": "0", "TacticalReloadEnable": "0", "AutoAimEnable": "0",
        "CutsceneSkip": "0", "DebugMode": "0", "FocusIgnore": "1",
    }
    lines = []
    for line in settings.splitlines():
        key = line.split("=", 1)[0].strip()
        lines.append(f"{key} = {fixed[key]}" if key in fixed else line)
    (tree / "config.ini").write_text("\n".join(lines) + "\n")
    return tree


def wine_env(work: Path, display: str) -> dict[str, str]:
    return dict(
        os.environ,
        WINEPREFIX=str(work / "prefix"),
        WINEDLLOVERRIDES="mscoree,mshtml=;ddraw=n,b",
        WINEDEBUG="-all",
        DISPLAY=display,
    )


def prepare_prefix(work: Path, env: dict[str, str]) -> None:
    if (work / "prefix" / "system.reg").exists():
        return
    subprocess.run(["wineboot", "-i"], env=env, check=True, capture_output=True)
    subprocess.run(
        ["wine", "reg", "add", r"HKCU\Software\Wine\Drivers", "/v", "Audio", "/t", "REG_SZ",
         "/d", "", "/f"],
        env=env, check=True, capture_output=True,
    )


def start_display() -> tuple[subprocess.Popen, str]:
    """A private virtual display, numbered by the X server itself."""
    read_end, write_end = os.pipe()
    server = subprocess.Popen(
        ["Xvfb", "-displayfd", str(write_end), "-screen", "0", "1024x768x24", "-nolisten", "tcp"],
        pass_fds=(write_end,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    os.close(write_end)
    number = os.read(read_end, 16).decode().strip()
    os.close(read_end)
    return server, f":{number}"


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
            if ppid == me and cmdline[:2] in (b"C:", b"Z:") and b"bio2" in cmdline \
                    and "bio2 1.10.exe" in maps and "ddraw.dll" in maps:
                return int(pid)
        time.sleep(0.02)
    raise SystemExit("the game did not start")


class Game:
    """The game's process, traced: stops at `BREAK_PAD` and otherwise runs freely."""

    def __init__(self, pid: int) -> None:
        self.pid = pid
        self.mem = os.open(f"/proc/{pid}/mem", os.O_RDWR)
        self.traced: set[int] = set()

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

    def run_to_break(self) -> tuple[int, Regs]:
        """Run every thread until one stops on the breakpoint; it stays stopped."""
        while True:
            tid, status = os.waitpid(-1, WALL)
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                self.traced.discard(tid)
                if tid == self.pid:
                    raise SystemExit("the game exited")
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
                    return tid, regs
            ptrace(PTRACE_CONT, tid, 0, sig)

    def resume_from_break(self, tid: int, regs: Regs, original: int) -> None:
        """Run the instruction under the breakpoint, put the breakpoint back, go on."""
        regs.rip = BREAK_PAD
        ptrace(PTRACE_SETREGS, tid, 0, ctypes.addressof(regs))
        self.write(BREAK_PAD, bytes([original]))
        ptrace(PTRACE_SINGLESTEP, tid)
        while True:
            stopped, status = os.waitpid(tid, WALL)
            if os.WIFSTOPPED(status) and os.WSTOPSIG(status) == signal.SIGTRAP \
                    and status >> 16 == 0:
                break
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                raise SystemExit("the game exited")
            # Another signal arrived first: deliver it and step again.
            ptrace(PTRACE_SINGLESTEP, tid, 0, os.WSTOPSIG(status) if status >> 16 == 0 else 0)
        self.write(BREAK_PAD, b"\xcc")
        ptrace(PTRACE_CONT, tid)


ITEM_WORK = 0x98ED34
DOOR_RECORD = 0x9886F8 + 0x54 * 4  # `em_die_table[0x54]`: `Sce_Door`'s record
GAME_STATE = 0x991F80
SCRATCH = 0x99EF00  # the unused end of the last page of `.data`


def warp(game: Game, target: str) -> None:
    """Do what `Sce_Door` does, with a door record of our own."""
    room, *place = target.split(":")
    x, y, z, angle = (int(part, 0) for part in place)
    stage, number = int(room[0], 16) - 1, int(room[1:], 16)
    floor = max(0, -y // 1800)
    game.write(SCRATCH, struct.pack("<hhhH4B8x", x, y, z, angle, stage, number, 0, floor))
    game.write(DOOR_RECORD, struct.pack("<I", SCRATCH))
    game.write(GAME_STATE, b"\x01")
    game.write(FG_STOP, struct.pack("<I", game.u32(FG_STOP) | 0xFF000000))


def remove_enemies(game: Game) -> None:
    """Switch off every live enemy: nothing runs or touches an entity whose first bit is clear."""
    for pointer in struct.unpack("<16I", game.read(ENEMIES, 64)):
        if pointer != 0:
            flags = game.u32(pointer + ENTITY_BE_FLG)
            if flags & 1:
                game.write(pointer + ENTITY_BE_FLG, struct.pack("<I", flags & ~1))


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
        "rseed=0",
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
    fields.append(f"pad={pad}")
    fields.append(f"fgsys={game.u32(FG_SYSTEM):08x}")
    fields.append(f"fgstat={game.u32(FG_STATUS):08x}")
    fields.append(f"fgstop={game.u32(FG_STOP):08x}")
    fields.append(f"fgscn={game.read(FG_SCENARIO, 32).hex()}")
    fields.append(f"fgcom={game.read(FG_COMMON, 32).hex()}")
    fields.append(f"fgroom={game.u32(FG_ROOM):08x}")
    spd = struct.unpack("<3h", game.read(PL + ENTITY_SPD, 6))
    motion = game.read(PL + ENTITY_MOTION, 4)
    fields.append(
        f"mot={routines[2]},{routines[3]},{motion[0]},{motion[1]},{motion[2]},"
        f"{game.u8(PL + ENTITY_D_LIFE_U)},{spd[0]},{spd[1]},{spd[2]}"
    )
    return " ".join(fields)


def screenshot(display: str, path: Path) -> None:
    subprocess.run(
        ["import", "-window", "root", "-crop", "640x480+0+0", str(path)],
        env=dict(os.environ, DISPLAY=display), check=False, capture_output=True,
    )


class Session:
    """The game running under Wine on its own virtual display, traced."""

    def __init__(self, game_dir: Path, work: Path) -> None:
        if not (game_dir / "bio2 1.10.exe").is_file():
            raise SystemExit("--game (or ARKLAY_RE2_ROOT) must be a retail RE2 install")
        work.mkdir(parents=True, exist_ok=True)
        # Wine double-forks the game. As a subreaper we adopt it, and Yama's
        # default policy lets a process trace its own descendants without root.
        libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0)
        self.display_server, self.display = start_display()
        self.env = wine_env(work, self.display)
        prepare_prefix(work, self.env)
        tree = prepare_game(game_dir, work)
        self.launcher = subprocess.Popen(
            ["wine", str(tree / "bio2 1.10.exe")], cwd=tree, env=self.env,
            stdout=open(work / "game.log", "w"), stderr=subprocess.STDOUT,
        )
        self.game = Game(find_game(time.monotonic() + 60))
        # Classic Rebirth applies its patches as it loads, so the code is only
        # final once the game has started running: wait for the code we break
        # on, then for the window.
        deadline = time.monotonic() + 60
        while self.game.read(BREAK_PAD, len(BREAK_PAD_BYTES)) != BREAK_PAD_BYTES:
            if time.monotonic() > deadline:
                raise SystemExit("Pad_set is not the expected code: wrong executable?")
            time.sleep(0.01)

    def close(self) -> None:
        subprocess.run(["wineserver", "-k"], env=self.env, check=False)
        self.launcher.kill()
        self.display_server.kill()


def record(args: argparse.Namespace, trace_path: Path, work: Path, tick_delay: float = 0.0) -> None:
    """Record `args.ticks` ticks of play from the title route `args.start`."""
    route = ROUTES[args.start]
    entries = parse_input(Path(args.input) if args.input else None)
    if args.shot_dir:
        Path(args.shot_dir).mkdir(parents=True, exist_ok=True)
    session = Session(game_root(args), work)
    trace_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with open(trace_path, "w") as trace:
            trace.write("oracle-trace v1 engine=ref\n")
            game = session.game
            game.seize_threads()
            original = game.u8(BREAK_PAD)
            game.write(BREAK_PAD, b"\xcc")

            started = time.monotonic()
            boot_tick = 0
            cursor = 0
            pad = 0
            settled = 0
            tick = -1  # The trace's tick, once play has begun.
            while tick <= args.ticks:
                tid, regs = game.run_to_break()
                if tick < 0:
                    if game.i16(PL + ENTITY_LIFE) != 0:
                        settled += 1
                    if settled > SETTLE_TICKS:
                        tick = 0
                        cursor = 0
                        pad = 0
                        print(f"play begins {boot_tick} ticks after boot")
                        if args.warp:
                            warp(game, args.warp)
                        for item in args.give:
                            slot, item_id, count = (int(part, 0) for part in item.split(":"))
                            game.write(ITEM_WORK + slot * 4, bytes((item_id, count, 0)))
                if settled and args.no_enemies:
                    remove_enemies(game)
                if tick < 0:
                    if boot_tick > BOOT_TICKS:
                        raise SystemExit("the title route did not reach play")
                    if args.shot_dir and args.shot_every and boot_tick % args.shot_every == 0:
                        screenshot(session.display, Path(args.shot_dir) / f"boot{boot_tick:06d}.png")
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
                        trace.write(trace_line(game, tick - 1, pad) + "\n")
                        if args.shot_dir and args.shot_every and (tick - 1) % args.shot_every == 0:
                            screenshot(session.display, Path(args.shot_dir) / f"t{tick - 1:06d}.png")
                    while cursor < len(entries) and entries[cursor][0] <= tick:
                        pad = entries[cursor][1]
                        cursor += 1
                    tick += 1
                regs.rax = pad
                if tick_delay:
                    time.sleep(tick_delay)
                game.resume_from_break(tid, regs, original)
            elapsed = time.monotonic() - started
            print(f"{boot_tick + args.ticks} ticks in {elapsed:.1f}s")
    finally:
        session.close()


def game_root(args: argparse.Namespace) -> Path:
    return Path(args.game or os.environ.get("ARKLAY_RE2_ROOT", ""))


def record_command(args: argparse.Namespace) -> int:
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
    command = [
        args.arklay, args.pack, "--ui", "game", "--character", str(CHARACTERS[args.start]),
        "--ticks", str(args.ticks), "--no-mods", "--trace", str(ark_trace),
    ]
    if args.input:
        command += ["--input", args.input]
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
    return subprocess.run(
        [sys.executable, str(repo / "scripts" / "oracle.py"), "compare", str(ref_trace),
         str(ark_trace), "--offset", "0", "--fields", args.fields, "--first", "3",
         "--context", "2"],
        check=False,
    ).returncode


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
        time.sleep(args.wait)
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    def common(command: argparse.ArgumentParser) -> None:
        command.add_argument("--game", default=None)
        command.add_argument("--work", default="target/oracle-re2/run")
        command.add_argument("--start", choices=sorted(ROUTES), default="leon")
        command.add_argument("--input", default=None)
        command.add_argument("--shot-dir", default=None)
        command.add_argument("--shot-every", type=int, default=0)
        command.add_argument("--no-enemies", action="store_true")
        command.add_argument(
            "--warp", default=None, metavar="ROOM:X:Y:Z:ANGLE",
            help="when play begins, go through a door to a room (three hex digits) and a position",
        )
        command.add_argument(
            "--give", action="append", default=[], metavar="SLOT:ID:COUNT",
            help="put an item in the inventory when play begins",
        )

    command = commands.add_parser("record", help="record a trace from the original")
    common(command)
    command.add_argument("--ticks", type=int, required=True)
    command.add_argument("--trace", required=True)
    command.add_argument("--tick-delay", type=float, default=0.0)
    command.set_defaults(func=record_command)

    command = commands.add_parser("scenario", help="record the original, run arklay, compare")
    common(command)
    command.add_argument("--ticks", type=int, required=True)
    command.add_argument("--out", required=True)
    command.add_argument("--arklay", default="target/test-release/arklay")
    command.add_argument("--pack", default="re2.akpak")
    command.add_argument("--fields", default=RE2_FIELDS)
    command.set_defaults(func=scenario_command)

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

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
