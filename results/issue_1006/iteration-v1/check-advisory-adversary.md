# Adversarial review: issue 1006 (`required`-row checks)

**Result: I could not break the production logic.** Every input I tried got the answer the brief asks for. What I did find is weak tests, one stale citation, two places where the code departs from the brief's wording, and two gate rows that prove less than they appear to.

## Findings

- NEEDS-HUMAN [impl] — The "wrong status" test passes for the wrong reason on three rows. `crates/validate/tests/capability_matrix_required.rs:1028` sends each wrong status with an empty body, and a different check then gives the same `protocol` class. I deleted each status check in turn and all 37 legs stayed green: `crates/validate/src/matrix.rs:1061` (ranged), `:1111` (`DeleteObjects`) and `:1127` (listings). Add legs where only the status is wrong: a ranged 200 with `Content-Range: bytes 2-5/16` and the right bytes; `DeleteObjects` answering 204 with a correct `<DeleteResult>`; a listing answering 201 with the correct keys. Production answers `protocol` on all three (I ran them), so this is a test gap only. The brief's (C) status rule applies to every `required` row, and these three rows have no leg that proves it.
- NEEDS-HUMAN [impl] — Four more changes to production keep all 37 legs green, so nothing pins these behaviours:
  - `matrix.rs:1073`, removing `is_strong_etag(sent)`: no fixture has a weak tag equal to the object's, so `sent = etag = W/"x"` answered 304 would pass instead of being unsettled.
  - `matrix.rs:1104` and `:1348`, removing a sort: every fixture lists keys in sorted order. With the sort gone, `keys = [b, a]` or an unsorted `expected` becomes a false FAIL, and `keys = [a, b, a]` no longer counts as naming a key twice. #939 could record keys in write order, so this input can occur.
  - `matrix.rs:1177`, also accepting 206: no leg sends an unfit conditional GET answered 206.

  Production is correct on each of these inputs (I ran them). Add one leg per input. The C5 row (81 mutants, 78 caught, 3 unviable) does not rule this out: `cargo mutants` (a tool that makes small automatic code changes to see if tests notice) replaces function bodies and swaps operators, but does not make edits like these. These survivors came from my own 51 hand edits.
- NEEDS-HUMAN [impl] — Stale citation added by this patch. `crates/validate/src/matrix.rs:1358` cites `crates/gateway-s3/src/lib.rs:2293-2300` for "the gateway answers 200 even when keys failed". On this target those lines are the doc comment and first lines of `declared_content_length`, which has nothing to do with deletes. The fact is at `crates/gateway-s3/src/lib.rs:2396-2397` and `:2548`. The number was copied from the brief, which pins it to `9b1a3e2`, but the brief requires citations on the target branch.
- NEEDS-HUMAN [human] — A retry rule that the brief does not state. `crates/validate/src/matrix.rs:1406-1411` drops a lost or cut probe once another probe of the same key got a clean 404, so `[a → no answer, a → 404]` and `[a → 404, a → no answer]` both PASS (`capability_matrix_required.rs:868-877`). Brief (C) lists "a follow-up with no head or a cut-off body" as unsettled, with no exception. In favour of the code: child-1's type allows retries (`matrix.rs:705-707`), a clean 404 does prove the key gone, and a fatal probe is never dropped. I think the rule is sound, but it should be accepted knowingly, not by default.
- NEEDS-HUMAN [human] — The brief conflicts with itself on a `DeleteObjects` of no keys. (G) says "only the two facts", and those two facts are about the exercise's own answer. The patch also judges follow-ups (`crates/validate/src/matrix.rs:1119`), so `keys = []` with a follow-up answering 503 is a FAIL (`protocol`), where (G) read literally gives unsettled (`capability_matrix_required.rs:1430-1433`). (C)'s rule for follow-ups of unrequested keys supports the patch. Decide which rule wins. This input is unlikely in a real run.
- NEEDS-HUMAN [human] — Check the T4 review evidence by hand (`check-gates.json:104-113`). This gating row claims three codex passes with 0 blocking, 0 recorded-rejected and 0 noise-dropped, in 19.55 seconds over a 2,040-line diff. Its log is one summary line, so I can't tell a cached result or a real review from passes that returned nothing. Open `results/issue_1006/review-batch.md` and confirm it holds real review output. I did not read that file because it is outside my inputs, so this is a doubt, not a proven fault.
- NEEDS-HUMAN [human] — Diff coverage was never measured (`check-gates.json:55-59`). The row says `fail` because `patch.diff` does not apply on `origin/main` ("the bundle is stale; rebase Do"). The patch does match this target exactly at base `635e0bd`, and the target has no remote, so I can't tell whether #1005 simply hasn't landed or `main` has moved. Confirm the base before publishing. The brief forbids Do from rebasing to get past a base problem, so routing this back to Do as a rebase may be the wrong fix.
- The C4-verify line "37 test(s) ran red" (`check-gates.json:48`) overstates the proof. The same log's result line, and my re-run, show **34 failed and 3 passed** on the base. The three that pass are `capability_matrix_required.rs:725`, `:1207` and `:1216`, which assert no-verdict or unsettled, answers child-1 already gave. They have no red→green proof. The file as a whole is still properly red, so this is a reporting error in the gate line and needs no code change.

## What I tried that did not break it

- **Red→green, re-run in a scratch copy:** base production gave 34 failed and 3 passed; patched production gave 37 passed. Child-1's `capability_matrix.rs` is unedited and passes 30/30 on both. The patch matches the target's files exactly. The tests call the real `wyrd_validate::matrix::{classify, verdict}`, not a copy.
- **Hand edits to production (51):** 41 were caught and 7 survived (all reported above). Of the other 3, 2 did not compile and 1 turned out to change nothing.
- **15 extra attack tests, all gave the brief's answer:**
  - `last = u64::MAX` and ranges on an empty object (no panic, and a 404 stays `oracle`)
  - cuts reporting more bytes than expected
  - malformed `ETag` and `Content-Range` values, including non-ASCII digits and tabs
  - follow-up order deciding the class (`[a → 503, b → 200]` is `protocol`; the reverse is `oracle`)
  - a key named twice with an `<Error>` row
  - a follow-up of another key trying to prove the requested key gone
- **Message order and early returns:** `Evidence::settle` (`matrix.rs:864-884`) scans the stages in order and returns the first fatal finding. Only two asks skip the row's own-answer checks, and (G) allows both: the out-of-range `return` at `matrix.rs:1055-1060` and the no-keys branch at `:1098-1101`. An unproved finding never stops a later check.
- **Gateway against the (F) table, read only:** PUT declares length 0 (`crates/gateway-s3/src/lib.rs:3654-3657`), a 204 declares no length (`:3637`), and an unranged HEAD declares the real size (`:2869-2870`).

**Not tested:** a full run against a live gateway. That needs the #939 runner, so the brief's (H) PASS is shown only on hand-built exercises.

<!-- pdca:leaf-complete -->
