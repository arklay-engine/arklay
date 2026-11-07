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
| b17-foresee | 1c3b51c+P | P: a door into an unmet, unready fight's room meets it at the doorstep | 12/17 | 30 | 70 | 108 | 202232.0 | 1 | yes | reject (completion 12<13, deaths 30>16; 30C's web and tiger foreseen) |
| b18-foresee1 | 1c3b51c+P2 | P2: P, but only into a room with a single fight (not 30C) | 13/17 | 30 | 68 | 108 | 203457 | 1 | yes | reject (deaths 30>16; 20C's Yawn 2 foreseen in every run) |
| b19-foresee513 | 1c3b51c+P3 | P3: foresee only fights marked foresee="yes" (513's Tyrant) - a look-ahead, against Ted's rule | 13/17 | 16 | 68 | 94 | 202349 | 3 | yes | reject (identical to b16 seed for seed; reverted) |
| b20-tyrant-outrun | 1c3b51c+Q | Q: 513's Tyrant can be outrun (speed 113 < run 0xD2) | 12/17 | 17 | 68 | 96 | 202976.5 | 3 | yes | reject (completion 12<13; seed 8 runs from it in 513 and dies 3 times) |
| b21-L-det | 1c3b51c+L | L (re-run on the deterministic base): a met fight's weapon on the frontier is a needed pickup | 13/17 | 16 | 68 | 94 | 202349 | 3 | yes | reject (identical to b16 seed for seed) |
| b22-E-det | 1c3b51c+E | E (re-run): red herbs out of the box to mix with a held one | 13/17 | 16 | 68 | 94 | 202349 | 3 | yes | void: never fired (Lua `and` truncated item_kind's heals; so b08's E never fired either) |
| b23-E2 | 1c3b51c+E2 | E2: E with the and-truncation fixed | 6/17 | 16 | 54 | 99 | 191546.0 | 8 | NO | reject (6/17: a held red herb is boxed and taken back in a loop) |
| b24-E3 | 1c3b51c+E3 | E3: E2, and a red herb that mixes with a held herb is kept and mixed at once | 10/17 | 25 | 67 | 121 | 202462.5 | 5 | NO | reject (10/17, default stalls in 304; deaths fell in seeds 1, 2, 8, 16) |
| b25-spareherb | 1c3b51c+F2 | F2: a second stack of a held non-healing herb stays on the floor while 2 or fewer slots are free | 11/17 | 38 | 73 | 116 | 202034 | 2 | yes | reject (11/17, deaths 38>16) |
| b26-metforce | 1c3b51c+G1 | G1: an empty frontier also forces a met fight still pending elsewhere (a reload drops the marks) | 13/17 | 16 | 68 | 98 | 202349 | 3 | yes | reject (no change at any level; seed 5 has no route out of 302) |
| b27-stairfirst | 1c3b51c+G2 | G2: a door whose every approach failed takes an untried stair first | 13/17 | 16 | 68 | 94 | 202349 | 3 | yes | reject (identical to b16: both of 301's stairs were already used that entry) |
| b28-keyswap | 1c3b51c+H3 | H3: a key pickup boxes loadout ammo whose gun is not held | 7/17 | 18 | 57 | 87 | 206320 | 8 | NO | reject (7/17, 8 stalls) |
| b29-rearmhere | 1c3b51c+J2 | J2: at a box here, a boxed reserve gun comes out while no held firearm has a round | 11/17 | 10 | 54 | 88 | 202680 | 6 | yes | reject (11/17: the launcher went in and out in a loop; deaths 10) |
| b30-rearmhere2 | 1c3b51c+J2b | J2b: J2, and the only held firearm with a round is no spare gun | 14/17 | 10 | 57 | 88 | 200607.0 | 3 | yes | accept |
| b31-E3-on-J2b | 329eae4+E3 | E3 re-run on J2b: red herbs kept and mixed | 8/17 | 19 | 63 | 111 | 199429.0 | 7 | NO | reject (8/17, default stalls in 304) |
| b32-G1-on-J2b | 329eae4+G1 | G1 re-run on J2b: an empty frontier forces a met fight still pending elsewhere | 14/17 | 13 | 57 | 88 | 200607.0 | 2 | yes | reject (deaths 13>10: seed 16 forced into 40C with nothing to shoot) |
| b33-G1b | 329eae4+G1b | G1b: G1, but not into a fight short of a gun or rounds | 14/17 | 10 | 57 | 88 | 200607.0 | 3 | yes | reject (identical to b30) |
| b34-fightheal | 329eae4+K3 | K3: heal to the fight threshold in a room with a <fight> still to be won | 11/17 | 31 | 63 | 97 | 196216 | 2 | NO | reject (11/17, deaths 31>10, default dies) |
| b35-fightheal2 | 329eae4+K3b | K3b: K3 only while no combat is active | 13/17 | 14 | 56 | 90 | 198534 | 3 | yes | reject (13/17: seed 4 dies 4 times; 303 deaths unchanged) |
| b36-rearmtrip | 329eae4+J3 | J3: J2b's reserve-gun rearm as a box trip of its own | 13/17 | 14 | 57 | 88 | 206078 | 3 | yes | reject (13/17: seed 8 dies 5 times) |
