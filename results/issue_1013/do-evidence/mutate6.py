#!/usr/bin/env python3
"""Iteration-6 mutations of crates/custodian/src/retirement/sessions.rs.

Each is an exact, one-match text replacement in production code. Usage:
    mutate6.py --list            print the mutation names
    mutate6.py NAME FILE         apply NAME to FILE in place (exit 1 if it does not match once)
"""
import sys

CHARGE = """    if !idle {
        pass.steps_left -= 1;
    }
"""
STOP = "    Ok(if idle || any_held || pass.steps_left == 0 {"
YIELD = """    let chosen = advanced
        .filter(|_| !passing)
        .or(if elsewhere { None } else { kept });"""
PUT = """            Some(value) => batch
                .require(session_key.to_vec(), record.to_vec())
                .put(bookmark_key, value),"""

M = {
    # Rule 1a: a step that marks and deletes nothing is not charged, and ends the visit.
    "C-always": (CHARGE, "    pass.steps_left -= 1;\n"),
    "C-unless-held": (CHARGE, CHARGE.replace("if !idle {", "if !idle || !any_held {")),
    "C-if-held": (CHARGE, CHARGE.replace("if !idle {", "if !idle || any_held {")),
    "C-idle-only": (CHARGE, CHARGE.replace("if !idle {", "if idle {")),
    "S-idle-goes-on": (STOP, "    Ok(if any_held || pass.steps_left == 0 {"),
    # The literal option (a) of the sign-off, whole: uncharged only while the step holds
    # something, and a charged step goes on as before.
    "A-literal": (
        CHARGE + "@@" + STOP,
        CHARGE.replace("if !idle {", "if !idle || !any_held {")
        + "@@"
        + "    Ok(if any_held || pass.steps_left == 0 {",
    ),
    # Rule 1b: a step that moves on past held entries leaves the bookmark to the held.
    "Y-never": (YIELD, YIELD.replace("        .filter(|_| !passing)\n", "")),
    "Y-when-idle": (YIELD, YIELD.replace("!passing", "!idle")),
    "Y-inverted": (YIELD, YIELD.replace("!passing", "passing")),
    "Y-passing-any": (
        "    let passing = idle && passed > 0;",
        "    let passing = idle || passed > 0;",
    ),
    # Bug 2: the bookmark is put only while the session record is the one read.
    "P-no-session": (PUT, "            Some(value) => batch.put(bookmark_key, value),"),
    "P-session-absent": (
        PUT,
        PUT.replace(
            ".require(session_key.to_vec(), record.to_vec())",
            ".require_absent(session_key.to_vec())",
        ),
    ),
}


def apply(name, path):
    old, new = M[name]
    text = open(path).read()
    for o, n in zip(old.split("@@"), new.split("@@")):
        if text.count(o) != 1:
            print(f"{name}: {text.count(o)} matches for {o[:50]!r}", file=sys.stderr)
            return 1
        text = text.replace(o, n)
    open(path, "w").write(text)
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--list"]:
        print(" ".join(M))
    else:
        sys.exit(apply(sys.argv[1], sys.argv[2]))
