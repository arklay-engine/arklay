#!/usr/bin/env bash
# Local bot benchmark: runs the headless bot on three scenarios and compares
# the --bot-stats JSON against a recorded baseline.
#
#   ./scripts/bot-bench.sh                     run all, compare with baselines
#   ./scripts/bot-bench.sh --only re1          run one scenario
#   ./scripts/bot-bench.sh --record-baseline   save this run as the baseline
#   BOT_BENCH_MAX_TICKS=60000 ./scripts/bot-bench.sh
#
# Results: target/bot-bench/<label>-<timestamp>.json
# Baselines: target/bot-bench/baseline-<label>.json
set -euo pipefail
cd "$(dirname "$0")/.."

MAX_TICKS="${BOT_BENCH_MAX_TICKS:-40000}"
CACHE="${HOME}/.cache/arklay-biorand"
OUT=target/bot-bench
BIN=target/test-release/arklay
declare -A PACKS=(
  [re1]="$CACHE/re1.akpak"
  [rando-123456]="$CACHE/games/biorand-123456.akpak"
  [rando-777777]="$CACHE/games/biorand-777777.akpak"
)
# The room whose entry ends a scenario's run (--bot-goal-room); none for the
# randomised packs, which run until the planner finishes or the tick budget.
declare -A GOALS=(
  [re1]=20D
)
LABELS=(re1 rando-123456 rando-777777)
record=0
only=""
while [ $# -gt 0 ]; do
  case "$1" in
    --record-baseline) record=1 ;;
    --only) only="${2:?--only needs a label}"; shift ;;
    -h|--help) sed -n 2,11p "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
if [ -n "$only" ]; then
  [ -n "${PACKS[$only]:-}" ] || { echo "unknown scenario '$only' (${LABELS[*]})" >&2; exit 2; }
  LABELS=("$only")
fi

cargo build --profile test-release 2>&1 | grep -E "^(error|warning)|Finished" || true
[ -x "$BIN" ] || { echo "FAIL: $BIN is missing after the build" >&2; exit 1; }
mkdir -p "$OUT"
stamp="$(date +%Y%m%d-%H%M%S)"

results=()
failed=()
for label in "${LABELS[@]}"; do
  pack="${PACKS[$label]}"
  [ -f "$pack" ] || { echo "FAIL: pack not found: $pack" >&2; exit 1; }
  json="$OUT/$label-$stamp.json"
  log="$OUT/$label-$stamp.log"
  echo "== $label ($pack)"
  goal=()
  [ -n "${GOALS[$label]:-}" ] && goal=(--bot-goal-room "${GOALS[$label]}")
  start=$EPOCHREALTIME
  "$BIN" "$pack" --mod mods/bot --bot --bot-headless "${goal[@]}" \
    --bot-max-ticks "$MAX_TICKS" --bot-stats "$json" >"$log" 2>&1 || \
    echo "warning: $label exited non-zero (see $log)" >&2
  wall=$(python3 -I -c "import sys;print(float(sys.argv[1])-float(sys.argv[2]))" "$EPOCHREALTIME" "$start")
  if [ ! -s "$json" ]; then
    echo "FAIL: $label produced no stats file ($json); see $log" >&2
    tail -5 "$log" >&2
    failed+=("$label")
    continue
  fi
  python3 -I - "$json" "$wall" <<'PY'
import json, sys
p, wall = sys.argv[1], float(sys.argv[2])
d = json.load(open(p)); d["wall_secs"] = round(wall, 2)
json.dump(d, open(p, "w"), indent=2)
PY
  results+=("$label:$json")
done

python3 -I - "$OUT" "$record" "${results[@]}" <<'PY'
import json, os, shutil, sys
out, record, items = sys.argv[1], sys.argv[2] == "1", sys.argv[3:]
def flat(d):
    return {
        "ticks": d["ticks"], "rooms": d["rooms_reached"], "uniq": d["unique_rooms"],
        "hp": d["health"]["final"], "minhp": d["health"]["min"],
        "dmg": d["health"]["damage_taken"], "deaths": d["deaths"],
        "fire": d["combat"]["fire_presses"], "turn": d["movement"]["turn_in_place_ticks"],
        "msg": d["states"]["message_ticks"], "scene": d["states"]["scene_ticks"],
        "trans": d["states"]["transition_ticks"],
        "unlk": d["unlock_to_room_change_ticks_total"], "wall": d["wall_secs"],
    }
cols = ["ticks","rooms","uniq","hp","minhp","dmg","deaths","fire","turn","msg","scene","trans","unlk","wall"]
print()
print(f"{'scenario':<14}{'outcome':<11}{'last':<6}" + "".join(f"{c:>8}" for c in cols))
for item in items:
    label, path = item.split(":", 1)
    d = json.load(open(path)); cur = flat(d)
    row = f"{label:<14}{d['outcome']:<11}{str(d['last_room']):<6}" + "".join(f"{cur[c]:>8}" for c in cols)
    print(row)
    base_path = os.path.join(out, f"baseline-{label}.json")
    if os.path.exists(base_path):
        b = json.load(open(base_path)); base = flat(b)
        delta = "".join(f"{cur[c]-base[c]:>+8.4g}" if isinstance(cur[c],(int,float)) and isinstance(base[c],(int,float)) else f"{'?':>8}" for c in cols)
        print(f"{'  vs base':<14}{b['outcome']:<11}{str(b['last_room']):<6}" + delta)
    else:
        print(f"{'  (no baseline)':<14}")
    if d.get("finish_reason"):
        print(f"  finish: {d['finish_reason']}")
    if record:
        shutil.copyfile(path, base_path)
        print(f"  baseline recorded: {base_path}")
PY

if [ ${#failed[@]} -gt 0 ]; then
  echo "FAIL: no stats for: ${failed[*]}" >&2
  exit 1
fi
