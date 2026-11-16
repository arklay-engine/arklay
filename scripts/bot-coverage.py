#!/usr/bin/env python3
"""Story coverage of bot runs: which scenes and branches each run reached.

  bot-coverage.py report <batch-dir>...    coverage per batch, and the union
  bot-coverage.py diff <batch-A> <batch-B> flags one batch reached, the other not

A bot run logs every story flag it changes (`story: common 0x61 set`, banks 0
scenario and 1 common), and bot-batch.sh writes the run's story parameters
(`--bot-param`) to batch.txt. A batch is one parameter set over some seeds;
its coverage is the union of the flags its runs set. The names file
(mods/bot/params/re1-story.xml) labels the flags that mark a scene or a
branch; unnamed flags are counted but not listed.
"""
import glob, os, re, sys
import xml.etree.ElementTree as ET

STORY = re.compile(r"^\[bot\] tick (\d+) room (\w+): story: (scenario|common) 0x([0-9A-F]{2}) (set|cleared)")
FINISH = re.compile(r"FINISH (.*)")
BANKS = {"scenario": 0, "common": 1}
HERE = os.path.dirname(os.path.abspath(__file__))
NAMES = os.path.join(HERE, "..", "mods", "bot", "params", "re1-story.xml")


def names():
    """(bank, bit) -> label, from the names file when it exists."""
    if not os.path.exists(NAMES):
        return {}
    out = {}
    for flag in ET.parse(NAMES).getroot().iter("flag"):
        out[(int(flag.get("bank"), 0), int(flag.get("bit"), 0))] = flag.get("label")
    return out


# Jill's ending from (common 0xC0 Barry gone, common 0x4B Chris freed), as
# src/ending.rs select_id picks it at the 303 escape.
ENDINGS = {(False, False): "Barry", (False, True): "Best", (True, False): "Bad",
           (True, True): "Chris"}
BARRY_GONE, CHRIS_FREED = (1, 0xC0), (1, 0x4B)


def run_coverage(path):
    """The flags a run set, and its finish line with the ending it earned."""
    flags, now, finish = set(), set(), ""
    with open(path, errors="replace") as f:
        for line in f:
            m = STORY.match(line)
            if m:
                key = (BANKS[m[3]], int(m[4], 16))
                if m[5] == "set":
                    flags.add(key)
                    now.add(key)
                else:
                    now.discard(key)
                continue
            if "FINISH " in line:
                finish = FINISH.search(line)[1][:60]
    if "game completed" in finish:
        ending = ENDINGS[(BARRY_GONE in now, CHRIS_FREED in now)]
        finish = f"{ending} ending ({finish.split('(')[0].strip()})"
    return flags, now, finish


def batch(dir_):
    params = ""
    info = os.path.join(dir_, "batch.txt")
    if os.path.exists(info):
        for line in open(info):
            if line.startswith("params="):
                params = line.split("=", 1)[1].strip()
    runs = {}
    for log in sorted(glob.glob(os.path.join(dir_, "seed-*.log")),
                      key=lambda p: int(re.search(r"seed-(\d+)", p)[1])):
        runs[int(re.search(r"seed-(\d+)\.log", log)[1])] = run_coverage(log)
    union = set().union(*(r[0] for r in runs.values())) if runs else set()
    return dict(name=os.path.basename(os.path.normpath(dir_)), params=params or "(defaults)",
                runs=runs, union=union)


def label(key, named):
    bank = "scenario" if key[0] == 0 else "common"
    text = f"{bank} {key[1]:#04x}"
    return f"{text} {named[key]}" if key in named else text


def report(dirs):
    named = names()
    batches = [batch(d) for d in dirs]
    everything = set().union(*(b["union"] for b in batches)) if batches else set()
    for b in batches:
        done = sum(1 for r in b["runs"].values() if " ending (" in r[2])
        others = set().union(*(o["union"] for o in batches if o is not b))
        only = sorted(b["union"] - others)
        print(f"== {b['name']}: {b['params']}")
        print(f"   runs {len(b['runs'])}, completed {done}, story flags {len(b['union'])}"
              f" ({len(only)} only here)")
        for seed, (_, _, finish) in b["runs"].items():
            print(f"   seed {seed}: {finish or 'no finish line'}")
        for key in only if len(batches) > 1 else []:
            print(f"   only here: {label(key, named)}")
        missing = sorted(k for k in named if k in everything and k not in b["union"])
        for key in missing:
            print(f"   missed:    {label(key, named)}")
    print(f"== union: {len(everything)} story flags over {len(batches)} batch(es)")
    unseen = sorted(k for k in named if k not in everything)
    for key in unseen:
        print(f"   never reached: {label(key, named)}")


def diff(a, b):
    named = names()
    left, right = batch(a), batch(b)
    for key in sorted(left["union"] - right["union"]):
        print(f"- {label(key, named)}  (only {left['name']})")
    for key in sorted(right["union"] - left["union"]):
        print(f"+ {label(key, named)}  (only {right['name']})")


def main(argv):
    if len(argv) >= 2 and argv[0] == "report":
        report(argv[1:])
    elif len(argv) == 3 and argv[0] == "diff":
        diff(argv[1], argv[2])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
