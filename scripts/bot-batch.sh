#!/usr/bin/env bash
# Run many full bot games in parallel, one per --bot-seed, and summarise them.
# A single run is deterministic, so one run per change says little; a batch of
# seeds shows the spread.
#
#   ./scripts/bot-batch.sh [options]
#   ./scripts/bot-batch.sh compare <batch-dir-A> <batch-dir-B>
#   ./scripts/bot-batch.sh summary <batch-dir>
#
# Options (each also an environment variable):
#   -n, --name NAME      batch name              (BATCH_NAME, default: timestamp)
#   -o, --out DIR        output root             (BATCH_OUT, default target/bot-batches)
#   -s, --seeds LIST     comma list or range     (BATCH_SEEDS, default 1,2,3,4,5,6,7,8; 1-16 works;
#                        seed 0 is the default run, without --bot-seed)
#   -r, --risk R         --bot-risk              (BATCH_RISK, default 0.25)
#   -j, --jobs N         parallel runs           (BATCH_JOBS, default: CPU count)
#   -p, --pack PATH      game pack               (BOT_PACK, default
#                        /home/ted/repos/arklay/target/bot-pack/re1.akpak)
#   -t, --timeout SEC    per-run timeout         (BATCH_TIMEOUT, default 1100)
#       --max-ticks N    --bot-max-ticks         (BATCH_MAX_TICKS, default 400000)
#   -P, --param K=V      --bot-param, repeatable (BATCH_PARAMS, space-separated K=V list):
#                        a story decision (mods/bot/params/re1.xml declares them)
#       --no-build       skip `cargo build --profile test-release`
#       --keep-saves     keep seed-<s>.saves/ after each run (KEEP_SAVES=1), to
#                        --load a save of the run later
#
# Output: <out>/<name>/seed-<s>.log (large, ~30 MB), seed-<s>.saves/, and
# summary.txt + summary.json. Hits are `damage health A -> B` log lines with
# A-B > 2 (poison and web ticks do 2 and are excluded).
set -uo pipefail
cd "$(dirname "$0")/.."
here="$PWD"

case "${1:-}" in
  compare) shift; exec python3 -I scripts/bot-batch-summary.py compare "$@" ;;
  summary) shift; exec python3 -I scripts/bot-batch-summary.py summary "$@" ;;
  -h|--help) sed -n 2,28p "$0"; exit 0 ;;
esac

NAME="${BATCH_NAME:-$(date +%Y%m%d-%H%M%S)}"
OUT="${BATCH_OUT:-target/bot-batches}"
SEEDS="${BATCH_SEEDS:-1,2,3,4,5,6,7,8}"
RISK="${BATCH_RISK:-0.25}"
JOBS="${BATCH_JOBS:-$(nproc)}"
PACK="${BOT_PACK:-/home/ted/repos/arklay/target/bot-pack/re1.akpak}"
TIMEOUT="${BATCH_TIMEOUT:-1100}"
MAX_TICKS="${BATCH_MAX_TICKS:-400000}"
PARAMS="${BATCH_PARAMS:-}"
BUILD=1
while [ $# -gt 0 ]; do
  case "$1" in
    -n|--name) NAME="${2:?}"; shift ;;
    -o|--out) OUT="${2:?}"; shift ;;
    -s|--seeds) SEEDS="${2:?}"; shift ;;
    -r|--risk) RISK="${2:?}"; shift ;;
    -j|--jobs) JOBS="${2:?}"; shift ;;
    -p|--pack) PACK="${2:?}"; shift ;;
    -t|--timeout) TIMEOUT="${2:?}"; shift ;;
    --max-ticks) MAX_TICKS="${2:?}"; shift ;;
    -P|--param) PARAMS="${PARAMS:+$PARAMS }${2:?}"; shift ;;
    --no-build) BUILD=0 ;;
    --keep-saves) KEEP_SAVES=1 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

# Expand "1-16" or "1,2,5-7" into one seed per line.
seed_list="$(python3 -I - "$SEEDS" <<'P'
import sys
out = []
for part in sys.argv[1].split(","):
    part = part.strip()
    if "-" in part:
        a, b = part.split("-", 1)
        out += range(int(a), int(b) + 1)
    elif part:
        out.append(int(part))
print("\n".join(map(str, out)))
P
)" || { echo "bad --seeds '$SEEDS'" >&2; exit 2; }

[ -f "$PACK" ] || { echo "pack not found: $PACK" >&2; exit 2; }
if [ "$BUILD" = 1 ]; then
  cargo build --profile test-release 2>&1 | grep -E "^(error|warning)|Finished" || true
fi
BIN="$here/target/test-release/arklay"
[ -x "$BIN" ] || { echo "FAIL: $BIN is missing" >&2; exit 1; }

DIR="$OUT/$NAME"
mkdir -p "$DIR"
DIR="$(cd "$DIR" && pwd)"
printf 'pack=%s\nrisk=%s\nseeds=%s\nmax_ticks=%s\nparams=%s\ncommit=%s\n' \
  "$PACK" "$RISK" "$(echo $seed_list | tr ' ' ',')" "$MAX_TICKS" "$PARAMS" \
  "$(git rev-parse --short HEAD 2>/dev/null)" > "$DIR/batch.txt"

export BIN PACK RISK MAX_TICKS TIMEOUT DIR here PARAMS KEEP_SAVES
run_one() {
  s="$1"
  rm -rf "$DIR/seed-$s.saves"; mkdir -p "$DIR/seed-$s.saves"
  start=$(date +%s)
  # Seed 0 is the default run: no --bot-seed, the game's own RNG.
  seed_arg=(--bot-seed "$s")
  [ "$s" = 0 ] && seed_arg=()
  param_arg=()
  for p in $PARAMS; do param_arg+=(--bot-param "$p"); done
  ( cd "$here" && timeout "$TIMEOUT" "$BIN" "$PACK" --mod mods/bot --bot --bot-headless \
      --bot-max-ticks "$MAX_TICKS" --bot-risk "$RISK" "${seed_arg[@]}" "${param_arg[@]}" \
      --save-dir "$DIR/seed-$s.saves" ) > "$DIR/seed-$s.log" 2>&1
  code=$?
  echo "exit=$code secs=$(( $(date +%s) - start ))" > "$DIR/seed-$s.exit"
  [ -n "$KEEP_SAVES" ] || rm -rf "$DIR/seed-$s.saves"
  echo "seed $s done (exit $code, $(( $(date +%s) - start ))s)"
}
export -f run_one

echo "batch $NAME: seeds $(echo $seed_list | tr ' ' ',') risk $RISK${PARAMS:+ params $PARAMS} jobs $JOBS -> $DIR"
echo "$seed_list" | xargs -P "$JOBS" -I{} bash -c 'run_one {}'
python3 -I scripts/bot-batch-summary.py summary "$DIR" | tee "$DIR/summary.txt"
