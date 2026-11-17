import sys, pathlib
# usage: mutate5-carried.py <file> <name>; iteration 4's bookmark mutations, re-spelled for
# iteration 5's sessions.rs. Each is a list of exact one-match replacements.
M = {
 # iteration 3's rule: a held entry claims the bookmark, first left wins, no reorder
 "iter3": [
  ("    let mut entries: Vec<(Vec<u8>, Vec<u8>, Read)> = page", "    let entries: Vec<(Vec<u8>, Vec<u8>, Read)> = page"),
  ("    let mut kept: Option<(Vec<u8>, usize)> = None;", "    let kept: Option<(Vec<u8>, usize)> = None;"),
  ("        if let Some(at) = entries\n            .iter()\n            .position(|(key, _, _)| key.as_slice() == named)\n        {\n            entries[..=at].rotate_right(1);\n        }",
   "        let _ = entries.iter().position(|(key, _, _)| key.as_slice() == named);"),
  ("                    if from > 0 {\n                        kept.get_or_insert((key, from));\n                    }\n                    continue;",
   "                    advanced.get_or_insert((key, from));\n                    continue;"),
  ("                step.held.push(held);\n                continue;\n            }\n        };",
   "                step.held.push(held);\n                advanced.get_or_insert((key, 0));\n                continue;\n            }\n        };"),
  ("        if step.held.len() == held_before {\n            advanced.get_or_insert((key, covered));\n        } else if covered > 0 {\n            kept.get_or_insert((key, covered));\n        }",
   "        let _ = held_before;\n        advanced.get_or_insert((key, covered));"),
  ("    let chosen = advanced.or(if elsewhere { None } else { kept });",
   "    let chosen = advanced\n        .or(if elsewhere { None } else { kept })\n        .filter(|(_, covered)| *covered > 0);"),
 ],
 # the bookmarked entry not walked first
 "noreorder": [
  ("    let mut entries: Vec<(Vec<u8>, Vec<u8>, Read)> = page", "    let entries: Vec<(Vec<u8>, Vec<u8>, Read)> = page"),
  ("        if let Some(at) = entries\n            .iter()\n            .position(|(key, _, _)| key.as_slice() == named)\n        {\n            entries[..=at].rotate_right(1);\n        }",
   "        let _ = entries.iter().position(|(key, _, _)| key.as_slice() == named);"),
 ],
 # a step that only moves the bookmark commits nothing
 "notmoved": [("    if idle && !moved {", "    if idle {")],
 # a bookmark naming another session's entry is taken as usable
 "nofilter": [("        .filter(|(entry, _)| entry.starts_with(&range));", ";")],
 "keptlt": [("                    if from > 0 {\n                        kept.get_or_insert", "                    if from == usize::MAX {\n                        kept.get_or_insert")],
 "keptge": [("                    if from > 0 {\n                        kept.get_or_insert", "                    if from != usize::MAX {\n                        kept.get_or_insert")],
 # every entry left counted as held / as walked without a hold
 "heldall": [("        if step.held.len() == held_before {\n            advanced.get_or_insert", "        if step.held.len() != held_before {\n            advanced.get_or_insert")],
 # a held entry's count beats an entry the walk can work on
 "keptfirst": [("    let chosen = advanced.or(if elsewhere { None } else { kept });", "    let chosen = (if elsewhere { None } else { kept }).or(advanced);")],
}
f = pathlib.Path(sys.argv[1]); name = sys.argv[2]
if name == "--list":
    print("\n".join(M)); sys.exit(0)
s = f.read_text()
for old, new in M[name]:
    n = s.count(old)
    assert n == 1, (name, n, old[:60])
    s = s.replace(old, new)
f.write_text(s)
