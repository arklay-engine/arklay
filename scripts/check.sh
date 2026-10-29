#!/usr/bin/env bash
# The local check, in tiers. The default is the session tier: the one check an
# agent or contributor runs before pushing, budgeted at five minutes from a
# cold worktree. The slower tiers are opt-in; CI runs the `--ci` tier on every
# PR, so a session does not need to.
#
#   ./scripts/check.sh          session: fmt, clippy, the unit and fast
#                               integration tests in one profile
#   ./scripts/check.sh --ci     everything the Linux CI job runs: the session
#                               tier plus the second test profile, the real
#                               test binaries' non-ignored tests,
#                               --no-default-features, the fuzz compile check
#                               and the docs
#   ./scripts/check.sh --full   --ci plus every ignored test (the real-asset
#                               suite and the asset-gated unit tests) when
#                               ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK are set; the
#                               RE2 tests run when ARKLAY_RE2_ROOT is set, alone
#                               if the RE1 variables are not
#   ./scripts/check.sh --soak   --full plus the full-game soak (half an hour+)
#
# --full and --soak are release-cycle checks. Never run them by default.
#
# The session tier builds the tests in the `test-release` profile only
# (release code without LTO, incremental), the same profile as the documented
# single-test loop, so the check reuses the loop's build and the loop reuses
# the check's. `--ci` adds the debug test build, as CI does. Clippy runs in the
# background, in `target/clippy`, while the tests build.
#
# The script prints each step's wall-clock time and the total, and warns when
# the session tier goes over its budget (ARKLAY_CHECK_BUDGET, seconds,
# default 300).
set -euo pipefail

cd "$(dirname "$0")/.."

tier=session
while (($#)); do
    case "$1" in
        --ci) tier=ci ;;
        --full) tier=full ;;
        --soak) tier=soak ;;
        -h | --help)
            sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "check.sh: unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

budget=${ARKLAY_CHECK_BUDGET:-300}
start=$SECONDS
timings=()

# Runs one step, echoing it and recording its wall-clock time.
step() {
    local label="$*"
    local t0=$SECONDS
    echo "==> $label"
    if ! "$@"; then
        timings+=("$(printf '%5ss  %s  (FAILED)' $((SECONDS - t0)) "$label")")
        summary
        echo "check.sh: $tier tier FAILED at: $label" >&2
        exit 1
    fi
    timings+=("$(printf '%5ss  %s' $((SECONDS - t0)) "$label")")
}

summary() {
    echo
    echo "check.sh: $tier tier timings"
    printf '  %s\n' "${timings[@]}"
    printf '  %5ss  total\n' $((SECONDS - start))
}

# The test binaries that hold only real-asset, soak or performance tests are
# not built in the session tier: clippy --all-targets still type-checks them,
# and --ci runs their few non-ignored tests. Every other integration test
# target is picked up automatically.
slow_tests=(real perf_real soak_real)
session_tests=(--lib --bins)
for path in tests/*.rs tests/*/main.rs; do
    [[ -e "$path" ]] || continue
    name=${path#tests/}
    name=${name%/main.rs}
    name=${name%.rs}
    [[ " ${slow_tests[*]} " == *" $name "* ]] && continue
    session_tests+=(--test "$name")
done

# Clippy only type-checks, so it shares no build output with the test
# builds. It runs in the background, in its own target directory so cargo's
# build lock does not serialise it behind them, while the tests build: on a
# cold worktree the two together take about as long as the test build alone.
clippy=(cargo clippy --all-targets --all-features -- -D warnings)
clippy_dir="${CARGO_TARGET_DIR:-target}/clippy"
clippy_log="$clippy_dir.log"
clippy_pid=
trap '[[ -n "$clippy_pid" ]] && kill "$clippy_pid" 2>/dev/null; true' EXIT

# Waits for the background clippy and reports it like a foreground step.
wait_clippy() {
    local rc=0
    wait "$clippy_pid" || rc=$?
    clippy_pid=
    echo "==> ${clippy[*]} (ran in the background; its output follows)"
    cat "$clippy_log"
    local took
    took=$(<"$clippy_log.time")
    if ((rc != 0)); then
        timings+=("$(printf '%5ss  %s  (FAILED, in the background)' "$took" "${clippy[*]}")")
        summary
        echo "check.sh: $tier tier FAILED at: ${clippy[*]}" >&2
        exit 1
    fi
    timings+=("$(printf '%5ss  %s  (in the background)' "$took" "${clippy[*]}")")
}

step cargo fmt --check

mkdir -p "$clippy_dir"
(
    t0=$SECONDS
    rc=0
    CARGO_TARGET_DIR="$clippy_dir" "${clippy[@]}" >"$clippy_log" 2>&1 || rc=$?
    echo $((SECONDS - t0)) >"$clippy_log.time"
    exit "$rc"
) &
clippy_pid=$!

if [[ "$tier" == "session" ]]; then
    step cargo test --profile test-release "${session_tests[@]}"
else
    step cargo check --no-default-features
    step cargo check --manifest-path fuzz/Cargo.toml
    step cargo test
    step cargo test --profile test-release
    step cargo doc --no-deps
fi
wait_clippy

if [[ "$tier" == "full" || "$tier" == "soak" ]]; then
    if [[ -n "${ARKLAY_RE1_ROOT:-}" && -n "${ARKLAY_RE1_PACK:-}" ]]; then
        if [[ "$tier" == "soak" ]]; then
            ARKLAY_SOAK=1 step cargo test --profile test-release -- --ignored
        else
            step cargo test --profile test-release -- --ignored
            echo "    (the full-game soak skipped itself; pass --soak to include it)"
        fi
    elif [[ -n "${ARKLAY_RE2_ROOT:-}" ]]; then
        step cargo test --profile test-release --test real re2_real:: -- --ignored
        echo "    (the RE1 suite was skipped: set ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK)"
    else
        echo "==> skipped the ignored suite: set ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK (and ARKLAY_RE2_ROOT for RE2)"
    fi
fi

summary
case "$tier" in
    session)
        echo "  skipped here, covered by CI on the PR: the debug test build, the"
        echo "  real/perf_real/soak_real binaries' non-ignored tests,"
        echo "  --no-default-features, the fuzz compile check, docs, Windows,"
        echo "  the MSRV check and the artifact smoke (./scripts/check.sh --ci"
        echo "  runs the Linux part locally)."
        echo "  release cycle only: --full (real assets), --soak, perf_real."
        if ((SECONDS - start > budget)); then
            echo "check.sh: WARNING: the session tier took $((SECONDS - start))s, over its ${budget}s budget (#266)" >&2
        fi
        ;;
    ci)
        echo "  not run here: Windows, MSRV and the artifact smoke (CI only);"
        echo "  --full and --soak (release cycle)."
        ;;
esac
echo "check.sh: $tier tier all green in $((SECONDS - start))s"
