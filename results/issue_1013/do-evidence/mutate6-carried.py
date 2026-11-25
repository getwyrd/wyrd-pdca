#!/usr/bin/env python3
"""Earlier iterations' mutations whose text iteration 6 changed, re-spelled for its sessions.rs.

Usage:  mutate6-carried.py --list | mutate6-carried.py NAME FILE
Each mutation is a list of exact one-match replacements. `iter3` is mutate5-carried.py's list with
its last pair re-spelled; the rest are single pairs.
"""
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
CHOSEN = """    let chosen = advanced
        .filter(|_| !passing)
        .or(if elsewhere { None } else { kept });"""
PASSING = "    let passing = idle && passed > 0;"


def iter3():
    head = (HERE / "mutate5-carried.py").read_text().split("\nf = pathlib.Path")[0]
    scope = {}
    exec(head, scope)  # defines M, the carried lists
    pairs = scope["M"]["iter3"][:-1]
    return pairs + [(CHOSEN, CHOSEN[:-1] + "\n        .filter(|(_, covered)| *covered > 0);")]


M = {
    # mutate.py R5: a walk step's commit spends no step.
    "R5-walk-step-spends-no-step": [("    if !idle {\n        pass.steps_left -= 1;\n    }\n", "")],
    # mutate5-carried.py: iteration 3's bookmark rule; a held entry's count ahead of an entry
    # the walk can work on.
    "iter3": iter3,
    "keptfirst": [(CHOSEN, "    let chosen = (if elsewhere { None } else { kept })\n"
                           "        .or(advanced.filter(|_| !passing));")],
    # mutate5.py: the scan position moved on a step that marked or deleted; never moved; moved
    # past a page whose first entry is not held; a bookmark on another page overridden by a held
    # entry of this one.
    "P-notidle": [(PASSING, "    let passing = passed > 0;")],
    "P-nopass": [(PASSING, "    let passing = false;")],
    "P-passed-ge": [(
        "    let passed = keys.iter().take_while(is_held).count();\n" + PASSING,
        "    let passed = keys.iter().take_while(is_held).count().max(1);\n"
        "    let passing = idle && !keys.is_empty();",
    )],
    "B-kept-over-elsewhere": [(CHOSEN, CHOSEN.replace("if elsewhere { None } else { kept }", "kept"))],
}

if __name__ == "__main__":
    if sys.argv[1:] == ["--list"]:
        print(" ".join(M))
        sys.exit(0)
    name, path = sys.argv[1], pathlib.Path(sys.argv[2])
    pairs = M[name]() if callable(M[name]) else M[name]
    text = path.read_text()
    for old, new in pairs:
        if text.count(old) != 1:
            print(f"{name}: {text.count(old)} matches for {old[:60]!r}", file=sys.stderr)
            sys.exit(1)
        text = text.replace(old, new)
    path.write_text(text)
