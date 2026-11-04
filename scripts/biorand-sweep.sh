#!/usr/bin/env bash
# The BioRand seed sweep (docs/biorand/plan.md, "The seed sweep").
#
#   scripts/biorand-sweep.sh [--base re1.akpak] [--plan-cases N] [--packs N]
#                            [--first SEED] [--out DIR] [--no-plan] [--no-packs]
#
# 1. Plan level (cheap, the bulk): N cases, each planned for both players on
#    the real RE1 map and put through every static check of
#    `arklay_biorand::audit` (package test `re1_sweep`, no pack written).
# 2. Pack level: the first --packs cases are generated as real packs (one
#    player per case, alternating), verified against the base pack with
#    `arklay biorand verify`, and deleted. Needs --base, the converted game.
#
# The case matrix is the package test's (tests/re1_sweep.rs): named one-factor
# cases, random draws from a factorial of option levels, and configurations
# with every option drawn at random, in rotation. Both summaries and a repro
# line per failure (seed, player, configuration) are written to --out.
# Uses the test-release profile only.
set -u

cd "$(dirname "$0")/.."
base=""
plan_cases=2000
packs=40
first=1
out="target/scratch/sweep"
run_plan=1
run_packs=1
while [ $# -gt 0 ]; do
    case "$1" in
        --base) base="$2"; shift 2 ;;
        --plan-cases) plan_cases="$2"; shift 2 ;;
        --packs) packs="$2"; shift 2 ;;
        --first) first="$2"; shift 2 ;;
        --out) out="$2"; shift 2 ;;
        --no-plan) run_plan=0; shift ;;
        --no-packs) run_packs=0; shift ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
done
mkdir -p "$out"
out="$(cd "$out" && pwd)"
status=0
profile=(--profile test-release)

if [ "$run_plan" = 1 ]; then
    echo "== plan level: $plan_cases cases x 2 players"
    SWEEP_CASES="$plan_cases" SWEEP_FIRST="$first" SWEEP_OUT="$out/plan-summary.txt" \
        cargo test "${profile[@]}" -p arklay-biorand --test re1_sweep re1_sweep \
        -- --ignored --nocapture 2>&1 | grep -E "^sweep:|^  (factorial|named|random):|^ +[0-9]+  |^greedy spend|Finished|test result"
    # The test fails on any check failure; the summary says which.
    grep -q "^sweep:" "$out/plan-summary.txt" || status=1
    if grep -A1 "^failure classes" "$out/plan-summary.txt" | grep -q "^ "; then status=1; fi
    echo "summary: $out/plan-summary.txt"
fi

if [ "$run_packs" = 1 ] && [ "$packs" -gt 0 ]; then
    if [ -z "$base" ]; then
        echo "--base re1.akpak is needed for the pack level (or --no-packs)" >&2
        exit 2
    fi
    echo "== pack level: $packs packs"
    cargo build "${profile[@]}" 2>&1 | grep -E "^(error|warning)|Finished"
    exe=target/test-release/arklay
    rm -rf "$out/cases" "$out/packs"
    SWEEP_DUMP="$out/cases" SWEEP_CASES="$packs" SWEEP_FIRST="$first" \
        cargo test "${profile[@]}" -p arklay-biorand --test re1_sweep re1_sweep_dump \
        -- --ignored >/dev/null 2>&1 || { echo "could not write the cases" >&2; exit 1; }
    mkdir -p "$out/packs"
    : > "$out/packs-summary.txt"
    ok=0
    failed=0
    warned=0
    while read -r index seed kind label; do
        if [ $((index % 2)) = 0 ]; then player=chris; else player=jill; fi
        started=$(date +%s)
        log="$out/packs/log.txt"
        # `generate` verifies every pack it writes and deletes a failing one;
        # the explicit verify below is the CLI check a user would run.
        if "$exe" biorand generate "$base" --seed "$seed" --player "$player" \
            --config "$out/cases/case-$index.json" --out-dir "$out/packs" --force >"$log" 2>&1 \
            && pack=$(tail -1 "$log") \
            && "$exe" biorand verify "$pack" --base "$base" >>"$log" 2>&1; then
            ok=$((ok + 1))
            if grep -q "warning:" "$log"; then warned=$((warned + 1)); fi
            result=ok
        else
            failed=$((failed + 1))
            status=1
            result=FAILED
            {
                echo "FAILED case $index ($kind $label) seed $seed player $player config $(cat "$out/cases/case-$index.json")"
                sed 's/^/    /' "$log" | head -20
            } >> "$out/packs-summary.txt"
        fi
        echo "case $index seed $seed $player $kind: $result ($(( $(date +%s) - started ))s)"
        rm -f "$out"/packs/*.akpak
    done < "$out/cases/cases.txt"
    {
        echo "packs: $((ok + failed)) generated and verified, $ok ok, $failed failed, $warned with a consumable-key warning"
    } | tee -a "$out/packs-summary.txt"
    echo "summary: $out/packs-summary.txt"
fi
exit $status
