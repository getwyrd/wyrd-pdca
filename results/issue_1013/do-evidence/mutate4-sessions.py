import sys, pathlib
# usage: mutate.py <file> <name>; applies one named mutation as exact one-match replacements
f = pathlib.Path(sys.argv[1]); name = sys.argv[2]
s = f.read_text()
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
  ("        if covered > 0 {\n            let left = if step.held.len() > held_before {\n                &mut kept\n            } else {\n                &mut advanced\n            };\n            left.get_or_insert((key, covered));\n        }",
   "        let _ = held_before;\n        advanced.get_or_insert((key, covered));"),
  ("    let bookmark = advanced\n        .or(kept)\n        .and_then(",
   "    let bookmark = advanced\n        .or(kept)\n        .filter(|(_, covered)| *covered > 0)\n        .and_then("),
 ],
 # the two tiers kept, the bookmarked entry not walked first
 "noreorder": [
  ("    let mut entries: Vec<(Vec<u8>, Vec<u8>, Read)> = page", "    let entries: Vec<(Vec<u8>, Vec<u8>, Read)> = page"),
  ("        if let Some(at) = entries\n            .iter()\n            .position(|(key, _, _)| key.as_slice() == named)\n        {\n            entries[..=at].rotate_right(1);\n        }",
   "        let _ = entries.iter().position(|(key, _, _)| key.as_slice() == named);"),
 ],
 # a step that only moves the bookmark commits nothing
 "notmoved": [
  ("    if marked == 0 && deleted == 0 && !moved {", "    if marked == 0 && deleted == 0 {"),
 ],
 # a bookmark naming another session's entry is taken as usable
 "nofilter": [
  ("        .filter(|(entry, _)| entry.starts_with(&range));", ";"),
 ],
 "keptlt": [("                    if from > 0 {\n                        kept.get_or_insert", "                    if from == usize::MAX {\n                        kept.get_or_insert")],
 "keptge": [("                    if from > 0 {\n                        kept.get_or_insert", "                    if from != usize::MAX {\n                        kept.get_or_insert")],
 "coveredge": [("        if covered > 0 {\n            let left", "        if covered != usize::MAX {\n            let left")],
 "heldge": [("            let left = if step.held.len() > held_before {", "            let left = if step.held.len() >= held_before {")],
 "keptfirst": [("    let bookmark = advanced\n        .or(kept)", "    let bookmark = kept\n        .or(advanced)")],
}
for old, new in M[name]:
    n = s.count(old)
    assert n == 1, (name, n, old[:60])
    s = s.replace(old, new)
f.write_text(s)
print("applied", name)
