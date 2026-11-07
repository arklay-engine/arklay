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
noise. From the marker on, `pairs` is deterministic, but until b16 a skipped
cutscene still fast-forwarded in 12 ms wall-clock bursts after every reload,
so where control came back hung on the machine's load (the same commit's
seeds still parted by a few hits and up to one death: b05 to b15). From b16
on, a headless bot run fast-forwards a scene to its end in one update, and
two batches of one commit match seed for seed.

| batch | commit | change | completed | deaths | median hits | p90 hits | median ticks | stalls | default | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| b00-base | 55cc220 | baseline (acid fix #444, no look-ahead) | 10/17 | 37 | 88 | 103 | 202170.0 | 4 | yes | base |
| b01-ready | - | B: readiness counts any loading of a boxed launcher; fight job fails with no banded gun | 9/17 | 42 | 85 | 122 | 197787 | 3 | NO | reject (completion, deaths, default) |
| b02-dodge | 55cc220+C | C: dodge acid globs in the air within 1500 | 11/17 | 26 | 65 | 119 | 190053 | 4 | yes | accept |
| b03-forcefight | dbcb9ad+D | D: an empty frontier walks to a fight held back elsewhere and fights it | 13/17 | 26 | 72 | 119 | 192516 | 2 | yes | accept |
| **deterministic** | | *rows below run with bot/order.lua* | | | | | | | | |
| b05-order | 46e36a7+F | F: deterministic pairs (sorted keys) in the bot VM | 13/17 | 25 | 72 | 119 | 192520 | 2 | yes | accept (new base) |
| b06-ablate-C | 2d59397-C | ablation: without C (glob dodge) | 9/17 | 47 | 87 | 125 | 202753 | 4 | NO | C kept (removing it is worse) |
| b07-ablate-D | 2d59397-D | ablation: without D (remote forced fight) | 11/17 | 26 | 65 | 119 | 196708 | 4 | yes | D kept (removing it is worse) |
| b08-redmix | 2d59397+E | E: a boxed red herb comes out to mix with a held herb (bot:mix_result) | 12/17 | 28 | 72 | 119 | 196801.0 | 3 | yes | reject (completion 12<13, deaths 28>25) |
| b09-rearm | 2d59397+G | G: the rearm beats the pickup reserve when no held firearm has a round | 12/17 | 27 | 72 | 106 | 193505.5 | 2 | yes | reject (completion 12<13, deaths 27>25) |
| b10-bossgun | 2d59397+H | H: a reserve gun with rounds is the last spare gun to box (after the staple) | 11/17 | 33 | 65 | 128 | 196954 | 3 | NO | reject (completion 11<13, deaths 33>25, default fails) |
| b11-rearm-reserve | 2d59397+J | J: rearm a boxed reserve gun when no held firearm has a round | 12/17 | 24 | 53 | 119 | 189060.5 | 3 | yes | reject (completion 12<13; deaths 24, median 53 better: retry after K) |
| b12-yawn-up | 2d59397+K | K: Yawn 2 takes aim-up shots (grown form inside 4000) | 13/17 | 14 | 57 | 92 | 202042 | 3 | yes | accept |
| b13-rearm-reserve | 218c1d3+J | J: rearm a boxed reserve gun when no held firearm has a round (on K) | 12/17 | 23 | 70 | 100 | 203012.0 | 3 | yes | reject (12<13, deaths 23>14) |
| b14-loadout-pickup | 218c1d3+L | L: a met fight's weapon on the frontier is a needed pickup | 13/17 | 15 | 57 | 93 | 201797 | 3 | yes | reject (deaths 15>14; seed 10 still loops) |
| b15-kite | 218c1d3+M | M: the fight job keeps away from its target when nothing held can fire | 13/17 | 16 | 57 | 91 | 202436 | 3 | yes | reject (deaths 16>14) |
| b16-nowall | 218c1d3+N | N: a headless bot run fast-forwards a skipped cutscene without the 12 ms wall-clock bound (reloads too) | 13/17 | 16 | 68 | 94 | 202349 | 3 | yes | accept (new base: a second batch matches it seed for seed) |
