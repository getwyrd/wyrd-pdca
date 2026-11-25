#!/usr/bin/env python3
"""Iteration 5 hand mutations of crates/custodian/src/retirement/sessions.rs.

Each is one exact, single-match replacement. Usage: mutate5.py <name> <file>  (rewrites file).
`mutate5.py --list` prints the names.
"""
import sys

M = {
 # the scan position is never written
 "P-nowrite": ("    meta.commit(batch).await?;\n    Ok(())\n}", "    let _ = (meta, batch);\n    Ok(())\n}"),
 # the lap: a position with entries after it is dropped anyway
 "P-lap-or": ("if after.is_some() && read.0.is_empty() {", "if after.is_some() || read.0.is_empty() {"),
 # the lap is never taken
 "P-lap-never": ("if after.is_some() && read.0.is_empty() {", "if false {"),
 # the head delete on every step
 "P-head-or": ("if stored_scan.is_some() && after.is_none() {", "if stored_scan.is_some() || after.is_none() {"),
 # a stored position the step no longer reads from is left
 "P-head-never": ("if stored_scan.is_some() && after.is_none() {", "if false {"),
 # a position naming nothing of the range is used as it is
 "P-anyrange": (".filter(|at| at.starts_with(&range));\n    if let (Some(stored), None) = (&stored_scan, &after) {", ".filter(|_| true);\n    if let (Some(stored), None) = (&stored_scan, &after) {"),
 # the position moves on a step that marked or deleted
 "P-notidle": ("if idle && passed > 0 {", "if passed > 0 {"),
 # the position never moves past held entries
 "P-nopass": ("if idle && passed > 0 {", "if false {"),
 # a page whose first entry is not held is passed too
 "P-passed-ge": ("if idle && passed > 0 {", "if idle && !keys.is_empty() {\n        let passed = passed.max(1);"),
 # a wholly held page is always taken to have a follower
 "P-noprobe": ("let follows = passed < keys.len()\n            || (more.is_some()", "let follows = passed <= keys.len()\n            || (more.is_some()"),
 # a partly held page is probed like a whole one
 "P-partial": ("let follows = passed < keys.len()\n            || (more.is_some()", "let follows = passed > keys.len()\n            || (more.is_some()"),
 # a full page is a follower without the one-row read
 "P-full-is-more": ("|| (more.is_some()\n                && !checked_page", "|| (more.is_some()\n                || !checked_page"),
 # the one-row read's answer inverted
 "P-probe-inverted": ("&& !checked_page(meta, &range, WALK, Some(last), 1)", "&& checked_page(meta, &range, WALK, Some(last), 1)"),
 # written when unchanged, left when it moved
 "P-moved-eq": ("        if to != after {\n            move_scan(meta, session, &scan_key, to).await?;", "        if to == after {\n            move_scan(meta, session, &scan_key, to).await?;"),
 # the position put without the session precondition
 "P-nosession": ("        Some(passed) => WriteBatch::new()\n            .require(session_key.to_vec(), record.to_vec())\n            .put(scan_key.to_vec(), passed),", "        Some(passed) => {\n            let _ = (session_key, record);\n            WriteBatch::new().put(scan_key.to_vec(), passed)\n        }"),
 # the terminal delete leaves the position / the bookmark
 "T-keeps-position": ("        .delete(owned_walk_key(upload_id))\n        .delete(owned_scan_key(upload_id));", "        .delete(owned_walk_key(upload_id));"),
 "T-keeps-bookmark": ("        .delete(owned_walk_key(upload_id))\n        .delete(owned_scan_key(upload_id));", "        .delete(owned_scan_key(upload_id));"),
 # the unusable position is not named
 "P-noaudit": ("        emit_unusable(session_key, UNUSABLE_POSITION, stored);", "        let _ = (stored, UNUSABLE_POSITION);"),
 # the bookmark: elsewhere, the first arm
 "B-else-lt": ("after.as_deref().is_some_and(|at| named <= at)", "after.as_deref().is_some_and(|at| named < at)"),
 "B-else-eq": ("after.as_deref().is_some_and(|at| named <= at)", "after.as_deref().is_some_and(|at| named == at)"),
 # the second arm
 "B-else-ge": ("keys.last().is_some_and(|last| named > last.as_slice())", "keys.last().is_some_and(|last| named >= last.as_slice())"),
 "B-else-never-after": ("keys.last().is_some_and(|last| named > last.as_slice())", "keys.last().is_some_and(|last| named < last.as_slice())"),
 "B-else-more-or": ("|| (more.is_some() && keys.last()", "|| (more.is_some() || keys.last()"),
 # a bookmark elsewhere is dropped when nothing takes it
 "B-else-dropped": ("        None if elsewhere => stored.clone(),", "        None if elsewhere => None,"),
 # a held entry of this page takes a bookmark that names an entry elsewhere
 "B-kept-over-elsewhere": ("let chosen = advanced.or(if elsewhere { None } else { kept });", "let chosen = advanced.or(kept);"),
 # iteration 4's rule: only an entry that covered a named position is bookmarked (carry-forward item 2)
 "B-iter4-covered": ("        if step.held.len() == held_before {\n            advanced.get_or_insert((key, covered));\n        } else if covered > 0 {", "        if step.held.len() == held_before {\n            if covered > 0 {\n                advanced.get_or_insert((key, covered));\n            }\n        } else if covered > 0 {"),
 # a held entry with nothing counted keeps the bookmark
 "B-kept-zero": ("        } else if covered > 0 {\n            kept.get_or_insert((key, covered));", "        } else {\n            kept.get_or_insert((key, covered));"),
}

if sys.argv[1] == "--list":
    print("\n".join(M))
    sys.exit(0)
name, path = sys.argv[1], sys.argv[2]
old, new = M[name]
s = open(path).read()
assert s.count(old) == 1, (name, s.count(old))
open(path, "w").write(s.replace(old, new))
