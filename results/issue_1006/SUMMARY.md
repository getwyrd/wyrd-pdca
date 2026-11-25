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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (42 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 333 of 333 instrumentable changed lines executed (floor 80%); 333 of 479 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 78 mutants tested in 5m: 75 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1006/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.62s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1006: complete required-row grading and written-key 404 classification so a fully proved `pre-0.1` capability run can pass.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The closed evidence table, failure precedence, and unfit-ask rules make acceptance falsifiable within the pure-classifier scope (`brief.md:24`, `brief.md:85`, `crates/validate/src/matrix.rs:999`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing only production while retaining the new tests produced 39 assertion failures and 3 passes after successful compilation; the missing behavior is independently reproduced (`reviewer-cargo-red.log:3`, `reviewer-cargo-red.log:326`). |
| C3 Change | PASS | The two-file patch stays within scope; public declarations, the prerequisite test file, and the verdict fold remain unchanged, preserving the prerequisite's contract (`reviewer-scope.log:1`, `reviewer-scope.log:4`, `crates/validate/src/matrix.rs:1020`). |
| C4 Verification (red→green) | PASS | Restoring production passes all 42 new and 30 prerequisite tests; frozen CI passes, while the independent full-CI rerun has the snapshot gitlink caveat explained below (`reviewer-cargo-restored.log:38`, `reviewer-cargo-restored.log:86`, `gate-logs/C4-ci.log:4686`). |
| C5 Causal adequacy | PASS | Fatal facts survive incomplete observations, and a clean retry cannot erase an unanswered/truncated probe; regression cases exercise both delete rows and the full verdict (`crates/validate/src/matrix.rs:1391`, `crates/validate/tests/capability_matrix_required.rs:896`). |
| T1 Structure | PASS | The existing message-order evidence fold remains the single verdict authority, keeping row checks subordinate to the specified precedence (`crates/validate/src/matrix.rs:864`, `crates/validate/src/matrix.rs:1020`). |
| T2 Shape | PASS | The existing observation/API surface suffices; strict range parsing and exact document membership prevent malformed answers from passing without broadening the contract (`crates/validate/src/matrix.rs:1342`, `crates/validate/src/matrix.rs:1361`, `crates/validate/src/matrix.rs:1462`). |
| T3 Runtime | PASS | Classification performs finite work over recorded inputs, with range bounds checked before slicing and no new clock, I/O, shared mutable state, or task lifecycle (`crates/validate/src/matrix.rs:1054`, `crates/validate/src/matrix.rs:1246`, `crates/validate/src/matrix.rs:1391`). |
| T4 Contribution | N/A | Commit/PR drafts are intentionally absent during Check; the substantive contribution audit is required at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Path-based prior art finds only the prerequisite matrix implementation and no overlap among 19 closed-unmerged PRs; isolated wrong-status regressions address the carried test weakness (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:214`, `crates/validate/tests/capability_matrix_required.rs:1143`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated pure-classifier contract is sufficient for the #939 handoff — the runner and wire-to-observation mapping remain assigned to #939, so this slice does not establish deployed smoke fitness (`brief.md:172`, `brief.md:191`, `crates/validate/src/lib.rs:10`). |

No builder-fixable defect was found. The independent red→green run, source review, and path-based prior-art check support this advisory judgment. Fitness-to-purpose remains a human decision.

Source paths above are relative to `$PDCA_TARGET`; evidence-log paths are relative to this report. The supplied patch reverses cleanly against that target, and the production change was restored after the red run (`reviewer-scope.log:3`, `reviewer-stash-restore.log:12`).

The independent reproduction used `cargo test --offline -p wyrd-validate --test capability_matrix_required --test capability_matrix`, with scratch/build outputs inside this review directory. `git stash push -- crates/validate/src/matrix.rs` retained the new test for the red run; `git stash pop` restored the fix. No production or test edits were made. Standalone `typos`, formatting, and whitespace checks passed; the full-CI rerun also executed `typos`, satisfying the brief's named external dependency (`reviewer-ci.log:2`, `brief.md:184`).

The independent `cargo xtask ci` rerun exited 1 with exactly two failing checks, both caused by the snapshot gitlink omission; the remaining CI stages, including DST, completed successfully (`reviewer-ci.log:4241`, `reviewer-ci.log:4231`). The environment caveat is: the disposable base and index both omit `.github/advisory-db`'s gitlink. Consequently `xtask/tests/advisory_db.rs:66` fails and the dependency-wall pin step cannot run (`reviewer-host-caveat.log:1`, `reviewer-ci.log:3595`, `reviewer-ci.log:3614`). The patch changes neither that index entry nor the dependency configuration. The frozen gate records the actual pin fetch, successful dependency scans, and completed CI (`gate-logs/C4-ci.log:4039`, `gate-logs/C4-ci.log:4053`, `gate-logs/C4-ci.log:4686`). This snapshot limitation is not a C4 patch defect or evidence that the original gate skipped its dependency checks.

The instance-scoped coverage, mutation, and batch-review wrappers were adjudicated from their captured logs:

- **C4-verify PASS:** the frozen run agrees with the independent reproduction: 39 failures before the fix, 42 passes after (`gate-logs/C4-verify.log:336`, `gate-logs/C4-verify.log:14`).
- **C4-diff-cov PASS:** 333/333 instrumentable changed lines executed; 146 other changed lines are explicitly unscored. The inspected diff introduces no conditional-compilation exclusion (`gate-logs/C4-diff-cov.log:78`, `gate-logs/C4-diff-cov.log:91`).
- **C5-mutants PASS:** the captured campaign reports 75 caught and 3 unviable mutants, with no survivors (`gate-logs/C5-mutants.log:13`).
- **T4-batch-review PASS as reported:** its captured summary reports zero blocking findings. Detailed reviewer reasoning is not in that output; this report supplies an independent source review (`gate-logs/T4-batch-review.log:10`).
- **host-tikv PASS:** the captured output completes both requested feature compilations; it makes no claim of a live TiKV test (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).
- **T4-contribution N/A:** the publish-time audit remains due by design (`gate-logs/T4-contribution.log:10`).

The source review checked header/body precedence, unfit asks, duplicate delete requests, probe identity, incomplete retries, and grammar boundaries. The new wrong-status fixtures isolate the status check instead of obtaining the same class from malformed bodies (`crates/validate/tests/capability_matrix_required.rs:1143`). The added guards validate recorded inputs; none probes an optional capability or masks a load-time cause. The gateway citation now grounds at its actual bulk-delete contract (`crates/gateway-s3/src/lib.rs:2396`, `crates/gateway-s3/src/lib.rs:2548`).

The prior-art investigation queried merged history by both affected file paths and inspected changed-file lists for all 19 closed-unmerged PRs. Matrix history contains only [the prerequisite implementation](https://github.com/getwyrd/wyrd/commit/a4cdce13c2c94cb740203c056793d0991c9ec268); the new test path has no merged history, and none of those PRs overlaps either affected path (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:9`, `reviewer-prior-art.json:214`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 1006 (capability matrix, `required` rows)

**Result: I could not refute the fix.** The red→green proof reproduces, the tests call production code, and no input I tried gives a wrong class or a false pass in a run. Two claims in `check-gates.json` go further than the logs support, and one sentence in the brief conflicts with another. None of these is a defect in the patch.

## Findings

- NEEDS-HUMAN [human] — `check-gates.json:108`: the gating T4 row says three codex review passes ran on this diff of about 2,200 lines, with "0 blocking, 0 recorded-rejected, 0 noise-dropped", all in 18.64 seconds (`gate-logs/T4-batch-review.log`). That is very fast for three passes, and it found nothing at all, not even noise. The log holds only that one summary line, so it does not show whether the passes reviewed this patch, reused an earlier result, or returned nothing. `review-batch.md` is outside my inputs and I did not read it. Please open `results/issue_1006/review-batch.md` before counting this row as the "deep, multi-pass review" the rubric requires. This is doubt about the gate's evidence. I have no evidence the review was wrong.
- `check-gates.json:48`: "42 test(s) ran red" is wrong. The same log says `3 passed; 39 failed` (`gate-logs/C4-verify.log:336`), and my own run with production reverted gave the same counts. The three tests green before the fix are `crates/validate/tests/capability_matrix_required.rs:725`, `:1381` and `:1390`. They assert no-verdict or unsettled, which child-1 already answered. That is harmless, because my code changes show two of them guard real behaviour. The wording comes from the harness's `run-verify.sh`, not this patch. Read the proof as 39 red tests, not 42.
- `crates/validate/src/matrix.rs:1119` against `brief.md:117-118`: the brief conflicts with itself on a `DeleteObjects` of no keys. (G) says only the two facts apply, but `brief.md:73-76` runs the key-independent checks on every follow-up of a key the delete did not request. The patch follows (C). A no-keys delete whose follow-up answers 404 with one body byte is `protocol` (`capability_matrix_required.rs:1631-1638`), where (G) read literally gives unsettled. I don't count this as a defect: the patch can only report more failures here, and the pinned test makes the choice visible. Smoke is also unlikely to send a no-keys delete.

## What I tried and could not break

- **Red→green:** I re-ran it from a clean copy of the base commit. Base gave 3 passed and 39 failed; the fix gave 42 passed; child-1's `capability_matrix.rs` still passes unedited (30 passed). `patch.diff` matches the target. `gate-logs/C4-ci.log:3316` shows the new test file ran inside the gate.
- **Wrong-reason passes:** the assertion helpers at `capability_matrix_required.rs:262-305` call the production `classify` and `verdict`, and nothing is copied or mocked. I made 55 small deliberate changes to `matrix.rs` by hand, beyond what `cargo mutants` (the mutation-testing tool behind gate C5) generates: dropped checks, swapped classes, reordered stages, changed boundaries. The tests caught 52. The other 3 give the same class on every input, so they cannot be caught by a class assertion: limiting the early-PUT check to status 200 (`matrix.rs:1033`), moving the 404 fact to the request stage, and swapping the request and status stages.
- **Precedence:** a cut, an unfit ask, a lost follow-up or an unprobed key never hid a later fatal fact. Moving follow-ups, body or headers earlier in `Evidence::settle` turned the tests red.
- **Grammar:** `parse_content_range` (`matrix.rs:1462`) refused every malformed value I sent: tab, double space, trailing space, `+` signs, `*`, `=`, an extra `/`, and an empty string. It accepts `BYTES` and leading zeros, which the rubric and RFC 9110 allow.
- **Boundaries:** an empty object asked for `0-0`, and `u64::MAX` for both range ends, are unsettled with no panic. A count equal to the size before a cut is unsettled, and one above it is `integrity`.
- **Follow-ups:** a 404 then a 200 is `oracle`, and a 304 is `protocol`. A lost probe then a 404 stays unsettled (`matrix.rs:1391`). That means a run whose retry succeeded stays INCONCLUSIVE, which is what the carry-forward at `brief.md:262` asked for, so I don't count it against the patch.
- **Inputs that give an odd result only after the run has already failed:**
  - A recorded weak tag: a `GetObjectIfNoneMatchMiss` with `sent` `"abc"` and object tag `W/"abc"` answered 304 is `protocol` (`matrix.rs:1079`), though RFC weak comparison makes 304 correct.
  - A recorded `HeadObject` tag that isn't valid, such as `garbage`, passes when the HEAD answers the same value (`matrix.rs:1227`).
  - Neither can turn a run into a false PASS on its own: the PUT that produced such a tag is already `protocol`, as long as #939 records the HEAD's tag from that PUT. The table at `brief.md:94` also asks only for equality.
- **Generic exercises:** `Exercise::Other { operation: HeadObject, .. }` answered 404 is unsettled, not `oracle`. The test at `capability_matrix_required.rs:1390` pins this, and it fits the brief: a generic exercise records no written key.
- **Gateway facts:** the new citation `crates/gateway-s3/src/lib.rs:2396-2397` and `:2548` is correct on this target, which fixes the stale citation from iteration 1. `serve_head` declares `content-length`, and `put_object_response` declares `content-length: 0`, matching the fixtures in `defined_required` (`capability_matrix_required.rs:322`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: no introduced correctness bugs, misleading tests, or actionable reuse, simplification, or efficiency issues found.

Reviewed the required-row checks and helpers in `crates/validate/src/matrix.rs:1020` and their tests in `crates/validate/tests/capability_matrix_required.rs:382`, grounded on `$PDCA_TARGET`.

Validation uses the frozen gate evidence; no builds were rerun. It records 42 new tests and 30 existing matrix tests passing, 39 new tests failing with production reverted, 100% instrumentable diff coverage, and 75 mutants caught with 3 unviable and none surviving.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decide whether the demonstrated pure-classifier contract is sufficient for the #939 handoff — the runner and wire-to-observation mapping remain assigned to #939, so this slice does not establish deployed smoke fitness (`brief.md:172`, `brief.md:191`, `crates/validate/src/lib.rs:10`).
- [x] `check-gates.json:108`: the gating T4 row says three codex review passes ran on this diff of about 2,200 lines, with "0 blocking, 0 recorded-rejected, 0 noise-dropped", all in 18.64 seconds (`gate-logs/T4-batch-review.log`). That is very fast for three passes, and it found nothing at all, not even noise. The log holds only that one summary line, so it does not show whether the passes reviewed this patch, reused an earlier result, or returned nothing. `review-batch.md` is outside my inputs and I did not read it. Please open `results/issue_1006/review-batch.md` before counting this row as the "deep, multi-pass review" the rubric requires. This is doubt about the gate's evidence. I have no evidence the review was wrong.
- [x] A retry rule that the brief does not state. `crates/validate/src/matrix.rs:1406-1411` drops a lost or cut probe once another probe of the same key got a clean 404, so `[a → no answer, a → 404]` and `[a → 404, a → no answer]` both PASS (`capability_matrix_required.rs:868-877`). Brief (C) lists "a follow-up with no head or a cut-off body" as unsettled, with no exception. In favour of the code: child-1's type allows retries (`matrix.rs:705-707`), a clean 404 does prove the key gone, and a fatal probe is never dropped. I think the rule is sound, but it should be accepted knowingly, not by default.
- [x] The brief conflicts with itself on a `DeleteObjects` of no keys. (G) says "only the two facts", and those two facts are about the exercise's own answer. The patch also judges follow-ups (`crates/validate/src/matrix.rs:1119`), so `keys = []` with a follow-up answering 503 is a FAIL (`protocol`), where (G) read literally gives unsettled (`capability_matrix_required.rs:1430-1433`). (C)'s rule for follow-ups of unrequested keys supports the patch. Decide which rule wins. This input is unlikely in a real run.
- [x] Check the T4 review evidence by hand (`check-gates.json:104-113`). This gating row claims three codex passes with 0 blocking, 0 recorded-rejected and 0 noise-dropped, in 19.55 seconds over a 2,040-line diff. Its log is one summary line, so I can't tell a cached result or a real review from passes that returned nothing. Open `results/issue_1006/review-batch.md` and confirm it holds real review output. I did not read that file because it is outside my inputs, so this is a doubt, not a proven fault.
- [x] Diff coverage was never measured (`check-gates.json:55-59`). The row says `fail` because `patch.diff` does not apply on `origin/main` ("the bundle is stale; rebase Do"). The patch does match this target exactly at base `635e0bd`, and the target has no remote, so I can't tell whether #1005 simply hasn't landed or `main` has moved. Confirm the base before publishing. The brief forbids Do from rebasing to get past a base problem, so routing this back to Do as a rebase may be the wrong fix.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- T4 batch-review log should show how much each pass actually reviewed; a one-line summary can't show review depth (issue_1006, 18.64s for three passes).
- `run-verify.sh` overstates its red count: it reported 42 tests red where 39 failed and 3 already passed (issue_1006).
