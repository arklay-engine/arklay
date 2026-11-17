#!/usr/bin/env bash
# Differential oracle runner for one room, reference vs arklay.
#
# Boots the C++ reference (test-only room hooks) and arklay through the
# session flow for the same room and player, writes both oracle traces and
# capture sets into OUTDIR, then runs scripts/oracle.py to compare game state
# tick-for-tick and frames pixel-by-pixel. See oracle/trace-v1.md for the
# trace contract.
#
# Usage:
#   scripts/oracle-room.sh ROOM PLAYER [TICKS] [OUTDIR]
#
#   ROOM    three hex digits, stage + room (e.g. 100, 11C), in the
#           reference's numbering: 6xx and 7xx are the second mansion visit
#           of rooms 1xx and 2xx (arklay's room 1xx with `mansion2`)
#   PLAYER  0 = Chris, 1 = Jill (selects the RDT variant on both sides)
#   TICKS   traced gameplay ticks (default 600)
#   OUTDIR  output directory (default target/oracle/room<ROOM>_p<PLAYER>)
#
# Environment overrides:
#   RE1_REF_REPO    reference checkout (default ../ref)
#   RE1_REF_BIN     reference binary (default <ref>/build/linux/residentevil)
#   RE1_REF_SYSROOT 32-bit sysroot (default <ref>/build/sysroot32)
#   RE1_ASSETS      game install root (default /usr/share/games/re1)
#   ARKLAY_BIN      arklay binary (default target/release/arklay)
#   ARKLAY_RE1_PACK game pack (default games/re1.akpak)
#   ARKLAY_LANGUAGE display language tag (default ja-JP, matching the
#                   reference's JPN assets)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REF_REPO="${RE1_REF_REPO:-$REPO_ROOT/../ref}"
REF_BIN="${RE1_REF_BIN:-$REF_REPO/build/linux/residentevil}"
SYSROOT="${RE1_REF_SYSROOT:-$REF_REPO/build/sysroot32}"
ASSETS="${RE1_ASSETS:-/usr/share/games/re1}"
ARK_BIN="${ARKLAY_BIN:-$REPO_ROOT/target/release/arklay}"
PACK="${ARKLAY_RE1_PACK:-$REPO_ROOT/games/re1.akpak}"
LANGUAGE="${ARKLAY_LANGUAGE:-ja-JP}"

ROOM="${1:?usage: oracle-room.sh ROOM PLAYER [TICKS] [OUTDIR]}"
PLAYER="${2:?usage: oracle-room.sh ROOM PLAYER [TICKS] [OUTDIR]}"
TICKS="${3:-600}"
OUT="${4:-$REPO_ROOT/target/oracle/room${ROOM}_p${PLAYER}}"

for path in "$REF_BIN" "$ARK_BIN" "$PACK"; do
    if [ ! -e "$path" ]; then
        echo "missing $path" >&2
        echo "  reference: cmake -S $REF_REPO -B $REF_REPO/build/linux -DCMAKE_BUILD_TYPE=Release" >&2
        echo "             RE1_M32_LIBDIR=$SYSROOT/usr/lib32 RE1_M32_INCLUDE=$SYSROOT/usr/include \\" >&2
        echo "             cmake --build $REF_REPO/build/linux -j" >&2
        echo "  pack:      $ARK_BIN pack convert game $ASSETS --out $PACK" >&2
        exit 2
    fi
done

case "$OUT" in
    /*) ;;
    *) OUT="$PWD/$OUT" ;;
esac

mkdir -p "$OUT/refcaps" "$OUT/arkcaps"

# The reference reads config.ini from its own directory; create one on first
# run (320x240 so captures and hashes match arklay's logical size).
REF_CONFIG="$(dirname "$REF_BIN")/config.ini"
if [ ! -f "$REF_CONFIG" ]; then
    cat > "$REF_CONFIG" <<EOF
[Display]
FullScreen=0
Width=320
Height=240
BitDepth=32
VSync=0

[Assets]
Path=$ASSETS
Version=JPN

[Save]
Path=

[Game]
Mode=OG
Ps1EndingCredits=0
SkipUnskippableFmv=0

[Debug]
EnableDebug=0
EOF
    echo "wrote $REF_CONFIG"
fi

echo "== reference: room $ROOM player $PLAYER, $TICKS ticks"
# The reference binary's ELF interpreter points into the sysroot (CMakeLists
# links it with --dynamic-linker), so it runs directly. Do not invoke the
# loader explicitly: /proc/self/exe would then name the loader and the game
# would resolve config.ini and assets from the loader's directory. Run it from
# OUT so its single-instance lock file and a crash log stay per-run, which
# also lets several oracle runs execute concurrently.
( cd "$OUT" && \
SDL_VIDEODRIVER=offscreen \
LD_LIBRARY_PATH="$SYSROOT/usr/lib32:$SYSROOT/usr" \
__EGL_VENDOR_LIBRARY_FILENAMES="$SYSROOT/usr/share/glvnd/egl_vendor.d/50_mesa.json" \
LIBGL_DRIVERS_PATH="$SYSROOT/usr/lib32/dri" \
LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe MESA_LOADER_DRIVER_OVERRIDE=llvmpipe \
    "$REF_BIN" \
    --test-newgame "$PLAYER" --test-room "$ROOM" \
    --test-trace "$OUT/ref.trace" \
    --test-capture-every 30 --test-capture-dir "$OUT/refcaps" \
    --test-exit-after "$TICKS" --test-nolimit )

# arklay names the second mansion visit by its room and `mansion2`.
ARK_ROOM="$ROOM"
ARK_VISIT=false
case "$ROOM" in
    6??|7??)
        ARK_ROOM="$(( ${ROOM:0:1} - 5 ))${ROOM:1}"
        ARK_VISIT=true
        ;;
esac

echo "== arklay: room $ROOM player $PLAYER, $TICKS ticks"
# The reference refuses a healing item with no effect, so compare under the
# authentic item rules. The run goes on until TICKS gameplay ticks are traced
# (a door transition's frames are not traced), and a frame is captured after
# every 30th traced tick, counting from 0, as the reference's
# --test-capture-every does.
"$ARK_BIN" script --headless - "$PACK" "$OUT" "$LANGUAGE" "$ARK_ROOM" "$PLAYER" "$ARK_VISIT" "$TICKS" <<'LUA'
local pack, out, language, room = arg[1], arg[2], arg[3], arg[4]
local player, mansion2, ticks = tonumber(arg[5]), arg[6] == "true", tonumber(arg[7])
strict(false)
load_game_pack(pack, { language = language, authentic = true })
new_game({ room = room, player = player, character = player, mansion2 = mansion2 })
trace_start(out .. "/ark.trace")
local updates = 0
while trace_ticks() < ticks do
    local index = trace_ticks()
    tick()
    updates = updates + 1
    if updates > ticks + 100000 then
        error("the game stopped tracing after " .. index .. " ticks")
    end
    if trace_ticks() > index and index % 30 == 0 then
        capture(out .. "/arkcaps/" .. string.format("f%05d.bmp", index))
    end
end
LUA

echo "== compare"
python3 "$REPO_ROOT/scripts/oracle.py" compare "$OUT/ref.trace" "$OUT/ark.trace" \
    --first 3 --context 2
python3 "$REPO_ROOT/scripts/oracle.py" pixels "$OUT/refcaps" "$OUT/arkcaps" \
    --report 10
echo "artifacts: $OUT"
