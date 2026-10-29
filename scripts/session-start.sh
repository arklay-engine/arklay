#!/usr/bin/env bash
# Claude Code hooks (see `.claude/settings.json`) that keep a worktree's build
# ahead of the session (#266). A session spends most of its time reading and
# thinking, so build then rather than when it asks.
#
#   session-start.sh            SessionStart: set RUSTC_WRAPPER for the
#                               session's shell; in a linked worktree, seed
#                               `target/` and start a background build.
#   session-start.sh edited     PostToolUse: if a source file changed since the
#                               background build started, restart it.
#   session-start.sh cargo      PreToolUse on Bash: before a cargo command,
#                               stop a background build that is out of date.
#
# The background build is what the session check builds: the test-release
# unit tests, then the binary and fast integration tests, then clippy with
# the check's arguments. A cargo command run while an up-to-date one is still
# going waits on cargo's lock ("Blocking waiting for file lock") and then has
# nothing left to do. The log is `target/prebuild.log`.
#
# Set ARKLAY_PREBUILD=0 to turn the background builds off. The main checkout
# is never built. Every mode returns at once and exits 0.
set -u

cd "${CLAUDE_PROJECT_DIR:-.}" 2>/dev/null || exit 0
mode=${1:-start}
input=$(timeout 1 cat 2>/dev/null)

wrapper=
if command -v unshare >/dev/null 2>&1 && [[ -x scripts/rustc-wrapper.sh ]]; then
    wrapper="$PWD/scripts/rustc-wrapper.sh"
    if [[ "$mode" == start && -n "${CLAUDE_ENV_FILE:-}" ]]; then
        echo "export RUSTC_WRAPPER=\"$wrapper\"" >>"$CLAUDE_ENV_FILE"
    fi
fi

[[ "${ARKLAY_PREBUILD:-1}" != 0 ]] || exit 0
command -v cargo >/dev/null 2>&1 || exit 0
[[ "$(git rev-parse --git-dir 2>/dev/null)" != "$(git rev-parse --git-common-dir 2>/dev/null)" ]] || exit 0

target=${CARGO_TARGET_DIR:-target}
log=$target/prebuild.log
pidfile=$target/prebuild.pid
# Touched as a background build starts: sources newer than it are not in it.
stamp=$target/prebuild.stamp

# Whether a source file changed since the last background build started.
changed() {
    [[ -e "$stamp" ]] || return 0
    [[ -n "$(find src tests benches build.rs Cargo.toml Cargo.lock -newer "$stamp" \
        \( -name '*.rs' -o -name 'Cargo.*' -o -path 'src/*' \) -type f -print -quit 2>/dev/null)" ]]
}

running() {
    [[ -f "$pidfile" ]] && kill -0 -- "-$(<"$pidfile")" 2>/dev/null
}

stop() {
    if running; then
        kill -- "-$(<"$pidfile")" 2>/dev/null
        echo "prebuild: stopped (sources changed)" >>"$log"
    fi
    rm -f "$pidfile"
}

kick() {
    stop
    touch "$stamp"
    export RUSTC_WRAPPER=$wrapper
    # `setsid` makes the build its own process group, so `stop` ends all of it.
    setsid bash -c '
        target=$1
        echo "prebuild: building ($(date +%T))"
        nice cargo test --profile test-release --lib --no-run &&
            nice cargo test --profile test-release --bins \
                --test cli --test memory --test torture --no-run &&
            CARGO_TARGET_DIR="$target/clippy" nice cargo clippy --all-targets \
                --all-features -- -D warnings
        echo "prebuild: done ($(date +%T))"
    ' prebuild "$target" </dev/null >>"$log" 2>&1 &
    echo $! >"$pidfile"
}

case "$mode" in
    start)
        [[ "$input" =~ \"source\":\ *\"(startup|resume)\" ]] || exit 0
        mkdir -p "$target" || exit 0
        ./scripts/seed-target.sh >>"$log" 2>&1
        kick
        echo "This worktree builds in the background, at session start and after each edit to a source file (log: $target/prebuild.log). A cargo command run while that build is still going waits for it (\"Blocking waiting for file lock\"); that is expected and quicker than building again. Edit and build in separate tool calls, so the background build gets a head start while you write the build command."
        ;;
    edited)
        [[ -d "$target" ]] && changed && kick
        ;;
    cargo)
        [[ "$input" =~ cargo\ |scripts/check\.sh ]] || exit 0
        # A command that edits and then builds (`python3 - <<EOF ...; cargo
        # test`) makes the running build out of date too.
        if running && { changed || [[ "$input" =~ python|perl|sed\ -i|\<\<|\>\ *src/ ]]; }; then
            stop
        fi
        ;;
esac
exit 0
