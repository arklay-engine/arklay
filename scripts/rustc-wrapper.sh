#!/usr/bin/env bash
# A rustc wrapper (RUSTC_WRAPPER) that lets a worktree reuse another
# worktree's incremental build of this crate (#299).
#
# rustc tracks its working directory and every path argument (the output and
# incremental directories, the extern crates), so after
# `scripts/seed-target.sh` copies another worktree's build output, the
# `arklay` crate still compiles from scratch: the paths name the other
# worktree. This wrapper runs rustc for this repository's own crates in a
# private mount namespace with the worktree bind-mounted at one fixed path,
# and rewrites the worktree path to that fixed path in rustc's arguments and
# OUT_DIR. Every worktree then hands rustc identical inputs, so a seeded
# worktree's first build is incremental: about 20s instead of 100s. It also
# keeps line shifts from invalidating unchanged code (see below).
#
# Paths in rustc's output and dep-info files are rewritten back, so cargo only
# ever sees the real ones. CARGO_MANIFEST_DIR is left alone, so tests that
# read files through it still work. Without unprivileged user namespaces
# (macOS, some containers and CI runners) it runs rustc unchanged.
#
# Claude Code sessions use it through `.claude/settings.json`. To use it in
# another shell: `export RUSTC_WRAPPER="$PWD/scripts/rustc-wrapper.sh"` from
# the repository root.
set -uo pipefail

real=$(pwd -P)
mnt=/tmp/arklay-rustc

# Only this repository's crates (cargo runs rustc from the package root) and
# only where a mount namespace is available.
ours=0
if [[ -e "$real/.git" && -f "$real/Cargo.toml" ]]; then
    for a in "$@"; do
        if [[ "$a" == "$real" || "$a" == *"$real/"* ]]; then
            ours=1
            break
        fi
    done
fi
if ((ours == 0)) || ! unshare -rm true 2>/dev/null; then
    exec "$@"
fi

swap() {
    local s=${1//"$real/"/"$mnt/"}
    [[ "$s" == "$real" ]] && s=$mnt
    printf '%s' "$s"
}

args=()
outdir=
crate=
extra=
prev=
for a in "$@"; do
    args+=("$(swap "$a")")
    case "$prev" in
        --out-dir) outdir=$a ;;
        --crate-name) crate=$a ;;
    esac
    [[ "$a" == extra-filename=* ]] && extra=${a#extra-filename=}
    prev=$a
done
[[ -n "${OUT_DIR:-}" ]] && export OUT_DIR="$(swap "$OUT_DIR")"

# rustc's incremental fingerprints include spans, so inserting one line near
# the top of a big file recompiles every function below it, even though only
# their line numbers moved: 48s instead of 9.5s for a line at the top of
# src/game.rs. -Zincremental-ignore-spans (unstable, hence RUSTC_BOOTSTRAP)
# leaves spans out. Compile errors still point at the right lines, but a
# reused function keeps its old line numbers in panic messages and replayed
# warnings, so a panic's line can be off by the lines inserted above it.
# Clippy (the session check's lint step) is run without it, so its spans are
# exact. ARKLAY_EXACT_SPANS=1 turns it off; switching rebuilds the crate once.
if [[ "${1##*/}" == rustc && -z "${ARKLAY_EXACT_SPANS:-}" ]]; then
    args+=(-Zincremental-ignore-spans)
    export RUSTC_BOOTSTRAP=1
fi

mkdir -p "$mnt" 2>/dev/null
[[ -d "$mnt" && -w "$mnt" ]] || exec "$@"
export ARKLAY_RUSTC_REAL=$real ARKLAY_RUSTC_MNT=$mnt
unshare -rm bash -c '
    mount --bind "$ARKLAY_RUSTC_REAL" "$ARKLAY_RUSTC_MNT" && cd "$ARKLAY_RUSTC_MNT" || exit 101
    exec "$@"
' rustc "${args[@]}" \
    > >(sed -u "s|$mnt/|$real/|g") \
    2> >(sed -u "s|$mnt/|$real/|g" >&2)
rc=$?
wait

# Cargo fingerprints from the dep-info file, so it must name real paths.
if [[ -n "$outdir" && -n "$crate" && -f "$outdir/$crate$extra.d" ]]; then
    sed -i "s|$mnt/|$real/|g" "$outdir/$crate$extra.d"
fi
exit "$rc"
