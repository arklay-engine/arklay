#!/usr/bin/env bash
# Run the bot down every story branch and report the coverage: one
# bot-batch.sh batch per parameter set below, then bot-coverage.py report.
#
#   ./scripts/bot-coverage.sh [-n NAME] [-s SEEDS] [--only COMBO] [--no-build]
#
#   -n, --name NAME   batch name prefix (default: coverage-<timestamp>)
#   -s, --seeds LIST  seeds per parameter set (default 1; see bot-batch.sh)
#       --only COMBO  run only the named combination (repeatable)
#       --no-build    skip the build
#
# Output: target/bot-batches/<NAME>-<combo>/ per combination, and
# target/bot-batches/<NAME>-coverage.txt. The combinations are the regression
# set: each one is a set of --bot-param values (mods/bot/params/re1.xml)
# that steers Jill down a branch of the story; together they should reach
# every scene the names file (mods/bot/params/re1-story.xml) lists.
set -uo pipefail
cd "$(dirname "$0")/.."

# name:space-separated K=V list (empty: the defaults).
COMBOS=(
  "default:"                                   # Yes, Yes, Follow: Barry ending
  "richard-leave:richard=leave"                # Richard dies in 20D
  "cave-yes-no:cave_q2=no"                     # Barry goes with Jill: Barry's branch
  "cave-no-yes:cave_q1=no"                     # Barry alone: Tell my family (lab)
  "cave-no-no:cave_q1=no cave_q2=no"           # Barry ahead (as Yes, Yes), Follow
  "cave-backtrack:cave_exit=backtrack"         # Tell my family (cave)
  "vjolt:vjolt=yes"                            # V-JOLT on Plant 42's root, no Barry in 40C
)

NAME="coverage-$(date +%Y%m%d-%H%M%S)"
SEEDS=1
ONLY=()
BUILD=()
while [ $# -gt 0 ]; do
  case "$1" in
    -n|--name) NAME="${2:?}"; shift ;;
    -s|--seeds) SEEDS="${2:?}"; shift ;;
    --only) ONLY+=("${2:?}"); shift ;;
    --no-build) BUILD=(--no-build) ;;
    -h|--help) sed -n 2,17p "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

if [ ${#BUILD[@]} = 0 ]; then
  cargo build --profile test-release 2>&1 | grep -E "^(error|warning)|Finished" || true
fi
dirs=()
for combo in "${COMBOS[@]}"; do
  name="${combo%%:*}"
  params="${combo#*:}"
  if [ ${#ONLY[@]} -gt 0 ] && ! printf '%s\n' "${ONLY[@]}" | grep -qx "$name"; then
    continue
  fi
  args=()
  for p in $params; do args+=(-P "$p"); done
  # One batch per combination, all at once: each run is one core.
  ( ./scripts/bot-batch.sh -n "$NAME-$name" -s "$SEEDS" --no-build "${args[@]}" > /dev/null
    echo "$name done" ) &
  dirs+=("target/bot-batches/$NAME-$name")
done
wait
python3 -I scripts/bot-coverage.py report "${dirs[@]}" | tee "target/bot-batches/$NAME-coverage.txt"
