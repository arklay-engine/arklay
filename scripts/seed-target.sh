#!/usr/bin/env bash
# Seeds a fresh worktree's build output from another checkout of this
# repository, so the first build compiles only the `arklay` crate instead of
# every dependency (SDL, OpenH264's C++, ...) from cold (#266).
#
#   ./scripts/seed-target.sh            seed from the other worktree with the
#                                       most recent test-release build
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
if [[ -z "$src" ]]; then
    newest=0
    while read -r line; do
        [[ "$line" == worktree\ * ]] || continue
        tree=${line#worktree }
        [[ "$(cd "$tree" 2>/dev/null && pwd -P)" == "$here" ]] && continue
        [[ -d "$tree/target/test-release" ]] || continue
        stamp=$(stat -c %Y "$tree/target/test-release" 2>/dev/null || echo 0)
        if ((stamp > newest)); then
            newest=$stamp
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
    echo "seed-target: seeded $target/$dir from $src"
done
