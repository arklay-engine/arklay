# Bot scoreboard

One row per benchmark batch of the RE1 bot (`mods/bot`), newest last. Each
batch is the same fixed set: seeds 1 to 16 and the default run (seed 0, no
`--bot-seed`), all at `--bot-risk 0.25`, `--bot-max-ticks 400000`:

```sh
./scripts/bot-batch.sh -n NAME -s 0-16 -r 0.25
python3 -I scripts/bot-batch-summary.py row target/bot-batches/NAME COMMIT "CHANGE" VERDICT
```

Columns: completed runs out of 17, deaths summed over every run, median and
p90 hits over every run (a hit is a health drop of more than 2), median game
ticks of the completed runs, stalls (runs the bot ended without the ending
and not on deaths: a loop, a stuck room, an empty frontier, the tick cap),
and whether the default run completed.

A change is kept only if, ranked: (a) completion does not drop, (b) deaths
do not rise, (c) then the median hits fall, (d) then the median ticks fall,
it is at least as good at every level above the one it improves, and the
default run still completes. Otherwise it is reverted and logged as
rejected.

Rows above the "deterministic" marker ran before `bot/order.lua`: Lua 5.4
seeds its string hash from the clock and ASLR, so `pairs` order, and with it
some seeds' runs, varied between runs of the same commit (seeds 6, 12 and
16 parted by up to 20 hits and 2 deaths). Their verdicts were taken on that
noise. From the marker on, a seed replays the same run.

| batch | commit | change | completed | deaths | median hits | p90 hits | median ticks | stalls | default | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| b00-base | 55cc220 | baseline (acid fix #444, no look-ahead) | 10/17 | 37 | 88 | 103 | 202170.0 | 4 | yes | base |
| b01-ready | - | B: readiness counts any loading of a boxed launcher; fight job fails with no banded gun | 9/17 | 42 | 85 | 122 | 197787 | 3 | NO | reject (completion, deaths, default) |
| b02-dodge | 55cc220+C | C: dodge acid globs in the air within 1500 | 11/17 | 26 | 65 | 119 | 190053 | 4 | yes | accept |
| b03-forcefight | dbcb9ad+D | D: an empty frontier walks to a fight held back elsewhere and fights it | 13/17 | 26 | 72 | 119 | 192516 | 2 | yes | accept |
| **deterministic** | | *rows below run with bot/order.lua* | | | | | | | | |
| b05-order | 46e36a7+F | F: deterministic pairs (sorted keys) in the bot VM | 13/17 | 25 | 72 | 119 | 192520 | 2 | yes | accept (new base) |
