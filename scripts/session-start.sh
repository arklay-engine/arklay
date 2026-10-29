#!/usr/bin/env bash
# Claude Code's SessionStart hook (see `.claude/settings.json`) (#266).
#
# - Sets RUSTC_WRAPPER to scripts/rustc-wrapper.sh for the session's shell.
# - In a linked worktree, on a new or resumed session: seeds `target/` with
#   scripts/seed-target.sh, then builds what the session check builds (the
#   test-release tests, then clippy) in the background. A session spends its
#   first minutes reading, so its first build is usually already done when it
#   asks for one. Cargo's lock makes a build started meanwhile wait for the
#   background one rather than race it. The log is `target/prebuild.log`.
#
# Set ARKLAY_PREBUILD=0 to skip the background build. The main checkout is
# never built. The hook always exits 0 and returns at once.
set -u

cd "${CLAUDE_PROJECT_DIR:-.}" 2>/dev/null || exit 0

wrapper=
if command -v unshare >/dev/null 2>&1 && [[ -x scripts/rustc-wrapper.sh ]]; then
    wrapper="$PWD/scripts/rustc-wrapper.sh"
    if [[ -n "${CLAUDE_ENV_FILE:-}" ]]; then
        echo "export RUSTC_WRAPPER=\"$wrapper\"" >>"$CLAUDE_ENV_FILE"
    fi
fi

input=$(timeout 1 cat 2>/dev/null)
[[ "$input" =~ \"source\":\ *\"(startup|resume)\" ]] || exit 0
[[ "${ARKLAY_PREBUILD:-1}" != 0 ]] || exit 0
command -v cargo >/dev/null 2>&1 || exit 0
[[ "$(git rev-parse --git-dir 2>/dev/null)" != "$(git rev-parse --git-common-dir 2>/dev/null)" ]] || exit 0

target=${CARGO_TARGET_DIR:-target}
mkdir -p "$target" || exit 0
./scripts/seed-target.sh >"$target/prebuild.log" 2>&1
export RUSTC_WRAPPER=$wrapper
setsid -f bash -c '
    target=$1
    nice cargo test --profile test-release --lib --bins \
        --test cli --test memory --test torture --no-run
    CARGO_TARGET_DIR="$target/clippy" nice cargo clippy --all-targets --all-features -- -D warnings
    echo "prebuild: done"
' prebuild "$target" </dev/null >>"$target/prebuild.log" 2>&1

echo "Building this worktree in the background (log: $target/prebuild.log). A cargo command run before that finishes waits for it (\"Blocking waiting for file lock\"); that is expected and quicker than building from scratch."
exit 0
