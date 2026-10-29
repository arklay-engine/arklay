#!/usr/bin/env bash
# Seeds a fresh worktree's build output from another checkout of this
# repository, so the first build compiles only the `arklay` crate instead of
# every dependency (SDL, OpenH264's C++, ...) from cold (#266).
#
#   ./scripts/seed-target.sh            seed from the other worktree whose
#                                       build is closest to this checkout
#   ./scripts/seed-target.sh <checkout> seed from that checkout
#
# With scripts/rustc-wrapper.sh as RUSTC_WRAPPER the arklay crate is reused too.
#
# It copies `target/test-release` and `target/clippy` (the session check's two
# build directories) with copy-on-write reflinks, which are instant and take
# no extra space on btrfs, XFS and APFS-like filesystems. Where reflinks are
# not supported it copies nothing and says so; a plain copy of several GB is
# rarely faster than building. It never overwrites an existing build.
set -euo pipefail

cd "$(dirname "$0")/.."
here=$(pwd -P)
target=${CARGO_TARGET_DIR:-target}

if [[ -d "$target/test-release" ]]; then
    echo "seed-target: $target/test-release already exists; nothing to do"
    exit 0
fi

src=${1:-}
best=
if [[ -z "$src" ]]; then
    # The closer the seed's sources are to this checkout's, the less the first
    # incremental build recompiles: seeding from a branch a few merges behind
    # costs minutes rather than seconds. So pick the checkout whose build's
    # commit (plus its uncommitted edits) differs from this HEAD by the fewest
    # lines, and among equals the one built last. The build time is the newest `arklay`
    # artifact: the directory's own mtime only changes when an entry is added.
    head=$(git rev-parse HEAD)
    while read -r line; do
        [[ "$line" == worktree\ * ]] || continue
        tree=${line#worktree }
        [[ "$(cd "$tree" 2>/dev/null && pwd -P)" == "$here" ]] && continue
        [[ -d "$tree/target/test-release/deps" ]] || continue
        stamp=$(find "$tree/target/test-release/deps" -maxdepth 1 \
            \( -name 'arklay-*' -o -name 'libarklay-*' \) -printf '%T@\n' 2>/dev/null |
            sort -n | tail -n 1)
        [[ -n "$stamp" ]] || continue
        # What the build was made from: HEAD as of the build, from the reflog
        # (a checkout pulled since its last build still has the old build).
        theirs=$(git -C "$tree" reflog show --date=unix --format='%H %gd' HEAD 2>/dev/null |
            awk -v t="${stamp%.*}" '{ gsub(/[^0-9]/, "", $2) } $2 <= t && !n++ { print $1 }')
        [[ -n "$theirs" ]] || theirs=$(git -C "$tree" rev-parse HEAD 2>/dev/null) || continue
        lines=$({
            git diff --numstat "$theirs" "$head" 2>/dev/null
            git -C "$tree" diff --numstat HEAD 2>/dev/null
        } | awk '{ n += $1 + $2 } END { print n + 0 }')
        key="$lines ${stamp%.*}"
        if [[ -z "$best" ]] || ((lines < ${best%% *})) ||
            ((lines == ${best%% *} && ${stamp%.*} > ${best#* })); then
            best=$key
            src=$tree
        fi
    done < <(git worktree list --porcelain)
fi
if [[ -z "$src" || ! -d "$src/target/test-release" ]]; then
    echo "seed-target: no other checkout has a target/test-release build to seed from"
    exit 0
fi

mkdir -p "$target"
for dir in test-release clippy; do
    [[ -d "$src/target/$dir" && ! -e "$target/$dir" ]] || continue
    if ! cp -a --reflink=always "$src/target/$dir" "$target/$dir" 2>/dev/null; then
        rm -rf "$target/$dir"
        echo "seed-target: this filesystem cannot reflink; not seeding (the first build compiles everything)"
        exit 0
    fi
    echo "seed-target: seeded $target/$dir from $src${best:+ (${best%% *} lines of source apart)}"
done
