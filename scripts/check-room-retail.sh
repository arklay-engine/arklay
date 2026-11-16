#!/usr/bin/env bash
# Compare every room's variant builds (each player, mansion visit and region)
# between a git revision (default HEAD) and the working tree. A change that
# must keep the retail bytes (such as converting item tests to `picked()`)
# shows no DIFF. Usage: scripts/check-room-retail.sh [rev] [room ...]
set -u
cd "$(dirname "$0")/.."
rev=${1:-HEAD}; shift || true
bin=target/test-release/arklay
old=target/retail-check/old
rm -rf "$old"; mkdir -p "$old" target/retail-check
git archive "$rev" games/re1/room games/re1/inc | tar -x -C "$old"
rooms=${*:-$(ls games/re1/room | grep -E '^[0-9a-f]{3}$')}
bad=0; n=0
for r in $rooms; do
  st=${r:0:1}; rm2=${r:1}; ids="${st}${rm2}0 ${st}${rm2}1"
  [ "$st" -le 2 ] && ids="$ids $((st+5))${rm2}0 $((st+5))${rm2}1"
  for id in $ids; do for reg in usa jpn eur; do
    for t in old new; do
      src=games/re1/room/$r/room.akscr; [ $t = old ] && src=$old/games/re1/room/$r/room.akscr
      $bin scd build "$src" --out target/retail-check/$t.scd --room "$id" --region $reg >/dev/null 2>&1 \
        || echo "FAIL $t $r $id $reg"
    done
    n=$((n+1))
    cmp -s target/retail-check/old.scd target/retail-check/new.scd || { echo "DIFF $r $id $reg"; bad=$((bad+1)); }
  done; done
done
echo "$n builds compared, $bad differ"
[ $bad -eq 0 ]
