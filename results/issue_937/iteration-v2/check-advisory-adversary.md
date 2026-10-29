# Adversarial review — #937 (validate: capability matrix)

How I checked: I compiled an unmodified copy of `crates/validate/src/matrix.rs` and the
shipped `crates/validate/tests/capability_matrix.rs` in a scratch crate (the only outside
symbol, `s3::ErrorCode`, was copied from `crates/validate/src/s3/error.rs:69-76`). Result:
46/46 pass. I then made each of the brief's ten required "demonstrated reds" as a temporary
edit. **All ten turn the shipped suite red**: (1) 2xx to an unsupported row passes → 4 tests
fail; (2) torn 2xx read as no answer → 1; (3) any non-empty ETag accepted → 3; (4) 304 to the
non-matching tag accepted → 1; (5) bulk delete passes on its document alone → 4; (6) selection
ignores the version → 5; (7) `+git.unknown` read as `0.0.0` → 1; (8) a not-checked row counted
as unexercised → 1; (9) an early PUT 200 accepted → 2; (10) a probe with no answer called
`oracle` → 3. The tests call the real module, not a copy of its logic. So the red→green
evidence holds even though C4-verify could not run it (the net-new exception the brief
declared in advance).

## Findings

- NEEDS-HUMAN [impl] — **The gating T4 failure is real and still open: a torn bulk-delete
  document hides a readable key.** `crates/validate/src/matrix.rs:1225` returns `Unsettled` as
  soon as the `<DeleteResult>` body failed, before any follow-up `HEAD` is judged. Concrete
  case (run on production code): `DeleteObjects` keys `[p/a, p/b]`, `200` + `Body::Failed`,
  then `HEAD p/a → 200`, `HEAD p/b → 404` gives `Unsettled`, so the run is INCONCLUSIVE, not
  FAIL. This breaks three things: the brief's per-HEAD rule ("a 2xx → `oracle`", no exception
  for a torn document); the module's own fold rule at `matrix.rs:1010-1011` ("none is skipped
  because another already failed"); and its handling of the single-key case. There, a torn
  `204` followed by `HEAD 200` is `oracle` (`matrix.rs:1144`, pinned by
  `capability_matrix.rs:800-806`). A small fix works: on `Body::Failed`, skip the document
  checks, push the `unsettled` result into `checks`, and still run the probe loop. With that
  edit my probe gives `oracle`, a torn body with all `404`s stays `Unsettled`, and all 46
  shipped tests stay green. Add the torn-body + `HEAD 200` fixture beside
  `capability_matrix.rs:753`. (Counter-argument, for the record: 0017 §7's outcome-resolution
  rule, `0017:535-539`, could read a torn bulk delete as an unresolved mutation where `HEAD 200`
  just records "not deleted". But the brief settled the per-probe classes, and the module
  already rejects that reading for `DeleteObject`.)

- NEEDS-HUMAN [human] — **One timed-out retry makes a row INCONCLUSIVE, which the brief's (E)
  does not say.** The brief: INCONCLUSIVE when a row was "exercised **only** with no answer".
  The code marks a row unproved if *any* exercise of it got no answer or was unsettled
  (`matrix.rs:1400-1401`). The test `capability_matrix.rs:1252-1258` pins that stricter rule.
  Concrete case: a full passing run plus one extra `GetObject` that timed out → INCONCLUSIVE.
  By the brief's wording that run is PASS. The rustdoc contradicts itself on this exact input:
  `matrix.rs:1376-1377` says "exercised only with no answer", and `:1377-1378` says "proved
  only when … every exercise of it passed". The stricter rule never over-claims, and for a
  timed-out *mutation* §7 (`0017:528-539`) supports it. But for reads, it means one transient
  timeout in any run that feeds every attempt to `verdict` can never be PASS. That cuts
  against §7's budgeted `availability` class (`0017:508`, `0017:499-500`). Decide which rule
  `verdict` keeps (or whether 743.3 feeds it retries at all), then make the rustdoc say one
  thing.

- NEEDS-HUMAN [human] — **The diff-coverage gate measured nothing, and the base needs
  confirming before publishing.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on
  origin/main". The target's base commit is `93a6f37`, which is neither base the brief names
  (`main` `4bac538`, integration tip `582da99`). This is not evidence against the fix:
  C5-mutants caught 127 of the 128 viable mutants, and the one survivor is equivalent (next
  bullet). But the PR's base is not settled. Whether `ErrorCode` (including `MissingInXml`,
  used at `capability_matrix.rs:337`) is identical on the base the PR finally targets is the
  brief's claim, and I could not check it here.

- C5's one surviving mutant is equivalent, not a test gap. `matrix.rs:572`, first `||` → `&&`
  turns the guard into "leading zero only". Then a non-digit string reaches `u64::from_str`,
  which would accept a leading `+`. But `parse_numeric` only ever sees text cut before the
  first `+` and the first `-` (`matrix.rs:513-528`), or text already checked to be all digits
  (`:546`). So no input can tell the two apart. No action needed.

- Class choice only, the outcome is unaffected: a `GET` or `HEAD` answering `404` for a key the
  run just wrote is classed `protocol` ("answered 404; expected 200",
  `matrix.rs:1080`, `:1123`). The brief says `HeadObject` "… else `oracle`", and a missing
  written key is the "model and server disagree" case of `0017:505`. Both classes are fatal,
  so the run still FAILs. Only the report's label differs. Worth a one-line decision, not a
  rebuild on its own.

## Attempted and could not refute

- **ETag grammar** (`matrix.rs:1284-1292`): `*`, unquoted, `W/"…"`, empty, a lone `"`, an
  inner `"`, DEL (`0x7F`), spaces and leading whitespace are all refused; `""` and obs-text
  are accepted, which matches RFC 9110 §8.8.3.
- **Content-Range** (`:1296-1311`): `+`/`-` signs, `*/size`, a missing size, extra spaces are
  all `protocol`; unit is case-insensitive; leading zeros are accepted (allowed by `1*DIGIT`).
- **Versions** (`:505-577`): `+git.unknown[.dirty]` → unknown; `+git.unknownx` → a release;
  the derivation's other real shapes (`0.1.0+git.dirty`, `0.1.0-rc.1+git.3.<sha>`, checked
  against `crates/server/src/version/derivation.rs:40-67`) select correctly; leading zeros,
  empty identifiers, `v`/`wyrd/` prefixes are refused.
- **Unsupported rows**: any 2xx, including a torn one, is `unexpected-support`; a 5xx other
  than the declared status is `protocol` (as the brief decided); no head → no verdict.
- **Bulk-delete probes**: wrong-key pairing, a later `HEAD 200` after a `404`, a `503` then
  `404` retry, an unrequested probed key, duplicate requested keys: all behave as the brief
  and the prior round asked.
- **Fold**: two fatal exercises of one row are both listed; a fatal row outranks an unproved
  one; a not-checked row is listed and never passes.
- **Citations**: every `pre-0.1` row's `crates/gateway-s3/src/lib.rs` line matches today's
  code (spot-checked `:1797-1826`, `:1842-1849`, `:1865-1871`, `:2008`, `:2017`, `:2025-2028`,
  `:2293-2300`, `:2713-2719`, `:2785-2803`, `:2995`, `:823`, `:855`, `:868`).
- Patch size 96,265 bytes, under the 100 KB backstop.

<!-- pdca:leaf-complete -->
