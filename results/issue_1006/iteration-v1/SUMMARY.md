# Result — issue 1006 / validate-capability-matrix-required-rows

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: after child-1, the matrix judges a `required` row by the 5xx fact alone and
  otherwise reports it unsettled. So no run against `pre-0.1` can PASS, and a `required`
  operation that returns the wrong bytes, a wrong header, a wrong document, a 404 for a key the
  run wrote, or a bulk delete that did not delete is never fatal. Smoke (#939) needs the
  `required` half of the verdict before its PASS means anything.
- Success criterion: with child-1's module as the base and no public type changed, the
  NEW file `crates/validate/tests/capability_matrix_required.rs` shows (B), (C), (F), (G) for
  every `required` row of `pre-0.1`, the 404 fact, and (H)'s full passing run; and child-1's
  `crates/validate/tests/capability_matrix.rs` still passes unedited.
  - **(B) The ladder, now with the `required` rows' checks as its data.** An exercise carries
    its evidence in message order: what it asked, the response head (status, then headers),
    the body (whole, or cut with what arrived before the cut), and any follow-up request, each
    its own answer. Judged in child-1's order: (1) another row's exercise is unsettled; (2) no
    response head is no verdict, follow-ups not judged; (3) **fatal** — every check (F) names
    for the row runs on everything that arrived, and the result is the class of the **first
    failing check in message order**: request-side facts, status, headers in (F)'s order,
    body, follow-ups in their order **(decided 2026-10-25)**; nothing that only leaves the
    exercise unproved — a cut, an ask that cannot test the row, a follow-up with no answer, an
    unprobed key, an empty listing — stops a later check from running; (4) **unsettled** only
    when step 3 found nothing and something left the row unproved; (5) **pass** when every
    check passed and nothing was left unproved. The tests show at least one precedence pair
    per class: a fixture where the unproving fact arrives before the fatal one, asserting the
    fatal class.
  - **(C) The classes, each one rule over every `required` row** (0017 §7 as amended).
    - **`oracle`** — the server's account of what the store holds disagrees with the model: a
      404 to any `GetObject` form or to `HeadObject` of a key the run wrote; a `HeadObject` 200
      whose `Content-Length` is not the object's size or whose `ETag` is not the one its PUT
      answered (absent counts as different); a listing 200 whose keys and sizes are not exactly
      those written under the prefix (one missing, extra, listed twice, another size); a
      follow-up `HEAD` **of a key the delete requested**, after an acknowledged delete (one
      that answered its defined status), answering 2xx (`0017:490`). A follow-up of any other
      key proves nothing about this delete: the gateway deletes only the named key
      (`crates/gateway-s3/src/lib.rs:2026-2028`) and rightly answers 200 for a key that still
      exists (`:2583-2584`, `:2612-2617`).
    - **`integrity`** — the payload a read delivered is not the object's: on an answer that
      carries object bytes (whole `GetObject` 200, ranged 206, a conditional `GetObject` 200),
      a declared `Content-Length` other than the expected count (the object's size, or the
      range's length); bytes that arrived whole and differ from the expected ones; bytes before
      a cut that are not a prefix of them; or a cut reporting more bytes than expected.
    - **`protocol`** — any other way the answer is not the defined one: a status other than
      the row's defined one (a 5xx included); a header the table requires, absent or malformed;
      a byte (whole, or before a cut) on an answer that carries no body; a non-zero declared
      `Content-Length` on a `PutObject` 200 or a `DeleteObject` 204, which declares a body the
      answer must not have, whatever arrived after it **(decided 2026-10-25: the matrix owns
      the length-framed case)**; a body that arrived whole and is not the document its status
      calls for (0017 §7, "XML shape"); a `PutObject` success whose head arrived before the
      request body had ended; a `<DeleteResult>` with any `<Error>` row, or whose `<Deleted>`
      does not name each requested key exactly once and nothing else.
    - **Unsettled** (step 4): a body cut off where the check needs the rest (after a correct
      prefix, after no byte, or a document cut before it could be read); an ask that cannot
      test the row ((G)); a follow-up with no head or a cut-off body; a requested key no
      follow-up proved gone; a follow-up of a key the delete did not request, whatever it
      answered — only the checks that do not depend on the key run on it (a 5xx is `protocol`;
      a byte on its no-body answer is `protocol`), never the follow-up's 404 status check or the
      deleted-key `oracle`; a listing of a prefix with nothing written; a `required` row whose
      exercise carries no check of its own.
    - **No verdict** (step 2): the exercise's own request got no response head.
    - **The two facts for every exercise that got a head:** a 5xx is `protocol` unless the row
      declares that status (child-1 shows it; it must still hold here); **a 404 to a typed read
      of a written key is `oracle`**, whatever was asked — shown here on every `GetObject` form
      and `HeadObject`, including an unfit ask. A typed read is a `GetObject` form's or
      `HeadObject`'s own request, never a delete's follow-up `HEAD`.
  - **(F) The evidence table — closed: a row reads these facts and no others.**

    | Row | Defined status | Headers read (in this order) | Body |
    |---|---|---|---|
    | `PutObject` | 200 | `ETag` strong (RFC 9110 §8.8.3), else `protocol`; `Content-Length` absent or 0, else `protocol` | none; request body had ended before the head, else `protocol` |
    | `GetObject` whole | 200 | `Content-Length` absent or the object's size, else `integrity` | the object's bytes |
    | ranged | 206 | `Content-Range` exactly `bytes a-b/size` (unit `bytes`, digits only, no signs), else `protocol`; `Content-Length` absent or `b-a+1`, else `integrity` | bytes `a..=b` |
    | conditional, matching | 304 | none | none |
    | conditional, non-matching | 200 | as whole `GetObject` | the object's bytes |
    | `HeadObject` | 200 | `Content-Length` = size, else `oracle`; `ETag` = the PUT's, else `oracle` | none |
    | `DeleteObject` | 204 | `Content-Length` absent or 0, else `protocol` | none; then the follow-up `HEAD` |
    | `DeleteObjects` | 200 | none | a `<DeleteResult>` ((C)); then a follow-up `HEAD` per requested key |
    | follow-up `HEAD` of a requested key | 404 | none | none |
    | `ListObjectsV2`, v1 | 200 | none | the listing, every page together ((C)) |

    A follow-up `HEAD` of a key the delete did not request has no defined status: only the
    key-independent checks run on it, then it is unsettled ((C)).

    Not read by this slice, and stated so a reviewer reads a request for one as scope, not a
    defect: a GET's `ETag` or `Last-Modified`; a 304's `Content-Length` (RFC 9110 §8.6 lets it
    describe the 200's length; the gateway sends none, `crates/gateway-s3/src/lib.rs:2785-2803`);
    `Content-Length` on a `HEAD` 404, a listing or a `<DeleteResult>`; a 204's
    `Content-Length: 0`. The `unsupported` rows' line of the table is child-1's and is not
    changed here.
  - **(G) Asks that cannot test their row.** The exercise records what it asked. An unfit ask
    never passes; it is judged by every check that does not depend on the ask, then is
    unsettled:
    - conditional GET whose sent value is not a strong tag equal to the object's (matching
      form) or not a strong tag different from it (non-matching form): 200 and 304 are both
      acceptable statuses, any other is judged by (C); a 200 is held to the object-bytes checks
      and a 304 to the no-body check. Each conditional exercise already records the sent value,
      the object's tag and the object's bytes (child-1's type).
    - ranged GET with `a > b` or `b ≥ size`, and `DeleteObjects` of no keys: only the two
      facts that hold for every exercise ((C), last bullet).
    - `DeleteObjects` naming a key twice: every check runs except the `<Deleted>` exact match.
    - a listing of a prefix with nothing written: every check runs (a listed key is `oracle`).
  - **(H) The run verdict, now reachable.** A full run against `pre-0.1` — every `required`
    row exercised with the defined answer, every `unsupported` row with its declared refusal —
    is PASS. Replacing any one exercise with a fatal one is FAIL naming that row and its class;
    replacing it with an unsettled or unanswered one is INCONCLUSIVE naming that row; a row with
    one passing and one unsettled exercise is not proved. The fold itself is child-1's and is
    not changed here.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: in `crates/validate/src/matrix.rs`: the `required` rows' entries of the check
  table — (F)'s header, body, document, follow-up and listing checks, (G)'s unfit-ask rules,
  the 404 fact — and the private grammar helpers they need (`is_strong_etag` if child-1 did not
  keep it, `parse_content_range`); the new test file with its own fixtures. No new public type,
  variant, field or function; a private helper is fine. / out of scope:
  - child-1's test file (`crates/validate/tests/capability_matrix.rs`) — not edited; the
    profiles, version keying, observation type, class and result types, the `unsupported` and
    `supported, not checked` checks, the ladder's order, and the verdict fold — not changed
    (a defect found in them is filed as a finding against child-1, not fixed here);
  - `crates/validate/src/lib.rs`; 0017 and every other doc; `Cargo.toml`;
  - sending anything: the client verbs are #955/#956; the runner, `--server-version`, reading
    the `Server` header, occupied-prefix and cleanup rules are #939;
  - mapping the client's results onto the observation type (#939), including telling the base
    client's one `S3Error::Unreadable` apart for a chunked PUT/DELETE 2xx;
  - a digest-based body for large objects (#761 adds one when it needs it); smoke sends small
    sizes only;
  - any fact (F) does not list; the cross-sweep oracle, event stream and verdict artifact
    (#746, #747, #764); MinIO's profile (#751); authentication vectors (#745); the availability
    budget itself;
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `Cargo.lock`; `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (37 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 81 mutants tested in 5m: 78 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1006/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.75s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1006’s required capability-matrix rows, written-key 404 classification, and full `pre-0.1` verdict: one reproducible false-PASS defect needs builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The closed evidence table and fatal-before-unsettled rule give falsifiable acceptance criteria without widening the public surface; `brief.md:32`, `brief.md:71`, `brief.md:85`. |
| C2 Reproduction (red pre-fix) | PASS | Keeping the new tests while stashing production independently reproduced assertion failures, not a compilation failure: 34 failed and 3 passed; `reviewer-red.log:542`. |
| C3 Change | PASS | The diff stays within the allowed production module and new integration test; public declarations, the verdict fold, and child-1’s tests remain unchanged; `crates/validate/src/matrix.rs:1020`, `crates/validate/tests/capability_matrix_required.rs:1`. |
| C4 Verification (red→green) | PASS | Restoring production independently passes all 37 new tests and 30 unchanged tests; this confirms the asserted red→green, with the separate C5 counterexample and unmeasured diff coverage recorded below; `reviewer-green.log:39`, `reviewer-green.log:82`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve unanswered/truncated follow-up evidence and correct the retry test — a complete 404 currently erases that evidence and falsely makes both delete rows and the full run PASS despite `brief.md:73`; `crates/validate/src/matrix.rs:1410`, `crates/validate/tests/capability_matrix_required.rs:868`, `reviewer-probe.log:2`. |
| T1 Structure | PASS | Private checks retain the existing evidence-stage ordering and dependency boundary; the independent blackbox guard passes; `crates/validate/src/matrix.rs:1020`, `reviewer-scanners.log:231`. |
| T2 Shape | PASS | Content-Range parsing checks units, digit grammar, overflow, and exact span; formatting and clippy pass without public API expansion; `crates/validate/src/matrix.rs:1474`, `crates/validate/tests/capability_matrix_required.rs:1231`, `reviewer-scanners.log:228`. |
| T3 Runtime | PASS | Processing remains pure and finite, with invalid ranges checked before slicing and no new clock, external await, or shared mutable state; `crates/validate/src/matrix.rs:1054`, `reviewer-scanners.log:244`. |
| T4 Contribution | PASS | Independent changed-path inspection found only prerequisite PR #1031 on the affected paths, no merged replacement or closed-unmerged overlap; contribution-artifact audit is separately N/A until publish; `reviewer-prior-art.json:2`, `reviewer-prior-art.json:120`, `reviewer-prior-art.json:305`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | No additional scope or capability-probe concern: the guards validate the recorded ask rather than hide a load-time side effect; the incorrect retry expectation is the single C5 finding above; `crates/validate/src/matrix.rs:1054`, `crates/validate/src/matrix.rs:1098`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the closed observation-level contract is sufficient for the #939 smoke handoff after C5 is resolved — this slice exercises classification, while real client-to-observation mapping is explicitly outside its scope; `brief.md:172`, `brief.md:174`, `crates/validate/tests/capability_matrix_required.rs:383`. |

Source citations refer to the supplied `$PDCA_TARGET`; brief and evidence citations refer to this review directory. The disposable target is usable and matches both patch postimage hashes (`cf4ab47`, `32891c9`). Production was restored after the red leg; no implementation changes were made.

**C5 finding: a successful probe must not erase an incomplete observation under this brief.** `brief.md:71` lists an unanswered or cut-off follow-up separately from a requested key lacking proof, and `brief.md:42` permits PASS only when nothing remains unproved. `judge_follow_ups` gathers every key with any complete 404, then drops every nonfatal finding for those keys at `crates/validate/src/matrix.rs:1410`. Consequently, a delete acknowledged with its defined status followed by `[timeout, complete 404]`, `[complete 404, timeout]`, or `[cut-off 404, complete 404]` returns PASS. A complete response proves the key absent, but it does not satisfy the brief’s independent requirement concerning the other incomplete response.

The standalone `reviewer_probe.rs` links the actual compiled production library. Its six cases cover both `DeleteObject` and `DeleteObjects`; each returned `Pass`, and substituting each into an otherwise defined full run also returned `Pass` (`reviewer-probe.log:1`). The expected results are `Unsettled` and `Inconclusive`. The new test at `crates/validate/tests/capability_matrix_required.rs:868` explicitly endorses the contradictory behavior, which explains why the green suite and mutation result do not catch it. Retain those unproved findings, use successful probes only to discharge the separate missing-key obligation, and change the retry regression expectations. If retry forgiveness is intended instead, it requires an explicit change to the accepted contract.

**Verification supports the patch’s existing tests, not the contradictory retry contract.** The independent commands were `cargo test --locked --offline -p wyrd-validate --test capability_matrix_required` with production stashed, then the same command plus `--test capability_matrix` after restoration. Independent `typos`, formatting, diff whitespace, clippy with warnings denied, blackbox guard, statics guard, and docs lint passed (`reviewer-scanners.log:1`, `reviewer-scanners.log:228`, `reviewer-scanners.log:249`). The brief’s external dependency, `typos`, was actually exercised.

The instance wrappers are absent from this disposable target, so their remaining verdicts are based on the supplied frozen logs, not claimed as independent reruns:

- C4-ci: PASS in captured evidence; the log shows spelling, docs, formatting, workspace clippy/build/tests, dependency checks, and DST, ending with all checks passed (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4681`). Attempts to invoke internal `unsafe-guard` and `gitlink-guard` as standalone xtask subcommands were rejected as unknown commands (`reviewer-scanners.log:234`); their actual CI executions are visible at `gate-logs/C4-ci.log:21` and `gate-logs/C4-ci.log:24`, so those invocation errors are not patch defects.
- C4-verify: captured red→green agrees with the independent rerun (`gate-logs/C4-verify.log:10`).
- C4-diff-cov: reported FAIL, with no coverage measured because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`). Prerequisite #1005 is present in this disposable target and its matrix PR #1031 remains open. This is a stacked-base caveat, not a demonstrated compilation or applicability defect in the supplied target.
- C5-mutants: captured baseline passed; 81 mutants were tested, 78 caught and 3 unviable (`gate-logs/C5-mutants.log:11`). This cannot validate an incorrect test expectation.
- T4-batch-review: captured output reports zero blocking findings, zero recorded rejections, and zero noise drops (`gate-logs/T4-batch-review.log:10`).
- T4-contribution: N/A — publish artifacts are intentionally absent; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- host-tikv: captured clippy executions for the metadata adapter and server feature selection both completed (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

**Prior art was checked by changed file paths.** GitHub file-list inspection covered 100 recent merged PRs (including the validator’s introduction), all 19 closed-unmerged PRs, and all six open PRs. Neither affected file appears in the inspected merged or rejected work; open #1031 contains the prerequisite matrix, while #1034 changes adjacent S3 client files. The exact queries’ scopes, PR numbers, and matching filenames are recorded in `reviewer-prior-art.json:2`. No tracked-issue deferral was found for the C5 behavior. No `INTEGRATION.md` exists in the supplied target, so no additional project-specific human-only items could be applied.

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

- No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in the required-row checks (`crates/validate/src/matrix.rs:1020`), body/document and follow-up helpers (`crates/validate/src/matrix.rs:1246`, `crates/validate/src/matrix.rs:1391`), range parser (`crates/validate/src/matrix.rs:1474`), or regression tests (`crates/validate/tests/capability_matrix_required.rs:383`, `crates/validate/tests/capability_matrix_required.rs:630`).

Validation used the frozen gate evidence; tests were not rerun. CI passed all 30 existing and 37 new matrix tests. The red→green check passed, and mutation testing caught all 78 viable mutants (3 unviable). Diff coverage was unavailable because the patch did not apply to that gate's base.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve unanswered/truncated follow-up evidence and correct the retry test — a complete 404 currently erases that evidence and falsely makes both delete rows and the full run PASS despite `brief.md:73`; `crates/validate/src/matrix.rs:1410`, `crates/validate/tests/capability_matrix_required.rs:868`, `reviewer-probe.log:2`.
- [ ] Validation — fitness-to-purpose — Confirm that the closed observation-level contract is sufficient for the #939 smoke handoff after C5 is resolved — this slice exercises classification, while real client-to-observation mapping is explicitly outside its scope; `brief.md:172`, `brief.md:174`, `crates/validate/tests/capability_matrix_required.rs:383`.
- [ ] The "wrong status" test passes for the wrong reason on three rows. `crates/validate/tests/capability_matrix_required.rs:1028` sends each wrong status with an empty body, and a different check then gives the same `protocol` class. I deleted each status check in turn and all 37 legs stayed green: `crates/validate/src/matrix.rs:1061` (ranged), `:1111` (`DeleteObjects`) and `:1127` (listings). Add legs where only the status is wrong: a ranged 200 with `Content-Range: bytes 2-5/16` and the right bytes; `DeleteObjects` answering 204 with a correct `<DeleteResult>`; a listing answering 201 with the correct keys. Production answers `protocol` on all three (I ran them), so this is a test gap only. The brief's (C) status rule applies to every `required` row, and these three rows have no leg that proves it.
- [ ] Four more changes to production keep all 37 legs green, so nothing pins these behaviours:
- [ ] Stale citation added by this patch. `crates/validate/src/matrix.rs:1358` cites `crates/gateway-s3/src/lib.rs:2293-2300` for "the gateway answers 200 even when keys failed". On this target those lines are the doc comment and first lines of `declared_content_length`, which has nothing to do with deletes. The fact is at `crates/gateway-s3/src/lib.rs:2396-2397` and `:2548`. The number was copied from the brief, which pins it to `9b1a3e2`, but the brief requires citations on the target branch.
- [ ] A retry rule that the brief does not state. `crates/validate/src/matrix.rs:1406-1411` drops a lost or cut probe once another probe of the same key got a clean 404, so `[a → no answer, a → 404]` and `[a → 404, a → no answer]` both PASS (`capability_matrix_required.rs:868-877`). Brief (C) lists "a follow-up with no head or a cut-off body" as unsettled, with no exception. In favour of the code: child-1's type allows retries (`matrix.rs:705-707`), a clean 404 does prove the key gone, and a fatal probe is never dropped. I think the rule is sound, but it should be accepted knowingly, not by default.
- [ ] The brief conflicts with itself on a `DeleteObjects` of no keys. (G) says "only the two facts", and those two facts are about the exercise's own answer. The patch also judges follow-ups (`crates/validate/src/matrix.rs:1119`), so `keys = []` with a follow-up answering 503 is a FAIL (`protocol`), where (G) read literally gives unsettled (`capability_matrix_required.rs:1430-1433`). (C)'s rule for follow-ups of unrequested keys supports the patch. Decide which rule wins. This input is unlikely in a real run.
- [ ] Check the T4 review evidence by hand (`check-gates.json:104-113`). This gating row claims three codex passes with 0 blocking, 0 recorded-rejected and 0 noise-dropped, in 19.55 seconds over a 2,040-line diff. Its log is one summary line, so I can't tell a cached result or a real review from passes that returned nothing. Open `results/issue_1006/review-batch.md` and confirm it holds real review output. I did not read that file because it is outside my inputs, so this is a doubt, not a proven fault.
- [ ] Diff coverage was never measured (`check-gates.json:55-59`). The row says `fail` because `patch.diff` does not apply on `origin/main` ("the bundle is stale; rebase Do"). The patch does match this target exactly at base `635e0bd`, and the target has no remote, so I can't tell whether #1005 simply hasn't landed or `main` has moved. Confirm the base before publishing. The brief forbids Do from rebasing to get past a base problem, so routing this back to Do as a rebase may be the wrong fix.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Preserve unanswered/truncated follow-up evidence and correct the retry test — a complete 404 currently erases that evidence and falsely makes both delete rows and the full run PASS despite `brief.md:73`; `crates/validate/src/matrix.rs:1410`, `crates/validate/tests/capability_matrix_required.rs:868`, `reviewer-probe.log:2`.; The "wrong status" test passes for the wrong reason on three rows. `crates/validate/tests/capability_matrix_required.rs:1028` sends each wrong status with an empty body, and a different check then gives the same `protocol` class. I deleted each status check in turn and all 37 legs stayed green: `crates/validate/src/matrix.rs:1061` (ranged), `:1111` (`DeleteObjects`) and `:1127` (listings). Add legs where only the status is wrong: a ranged 200 with `Content-Range: bytes 2-5/16` and the right bytes; `DeleteObjects` answering 204 with a correct `<DeleteResult>`; a listing answering 201 with the correct keys. Production answers `protocol` on all three (I ran them), so this is a test gap only. The brief's (C) status rule applies to every `required` row, and these three rows have no leg that proves it.; Four more changes to production keep all 37 legs green, so nothing pins these behaviours:; Stale citation added by this patch. `crates/validate/src/matrix.rs:1358` cites `crates/gateway-s3/src/lib.rs:2293-2300` for "the gateway answers 200 even when keys failed". On this target those lines are the doc comment and first lines of `declared_content_length`, which has nothing to do with deletes. The fact is at `crates/gateway-s3/src/lib.rs:2396-2397` and `:2548`. The number was copied from the brief, which pins it to `9b1a3e2`, but the brief requires citations on the target branch.. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
