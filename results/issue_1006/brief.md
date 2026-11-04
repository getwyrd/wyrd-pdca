# validate: capability matrix — the required-row checks, the 404 fact, the full `pre-0.1` run (743.1b)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`, as amended by child-1)
> before writing code. Second of two children of #937 (itself 743.1); builds on child-1's
> accepted module and changes **no public type**. Pure; it sends nothing. Kin: 743.2 = #938
> (client verbs, split into #955, #956), 743.3 = #939 (smoke), which consumes both children.
>
> **Why this shape:** #937's iterations 7–9 each fixed the last edge case in the `required`-row
> grading and review found the next one in the same code. Both iteration-9 defects had one
> shape: a check that can only make an exercise unsettled ran before a check that would have
> found it fatal (`matrix.rs:889` there ignored a declared `Content-Length` once a cut arrived;
> `:1186` returned "unsettled" on a duplicate key before the `<Error>` and document-shape
> checks). This brief states each outcome as one rule over every row, under the one ladder
> child-1 built, with a closed table of what each row reads. Fixtures are chosen per class and
> per precedence pair, not per case. The decisions of 2026-10-25 (the human's, that Plan
> session) are marked **(decided 2026-10-25)**.

- **Slug:** validate-capability-matrix-required-rows
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** after child-1, the matrix judges a `required` row by the 5xx fact alone and
  otherwise reports it unsettled. So no run against `pre-0.1` can PASS, and a `required`
  operation that returns the wrong bytes, a wrong header, a wrong document, a 404 for a key the
  run wrote, or a bulk delete that did not delete is never fatal. Smoke (#939) needs the
  `required` half of the verdict before its PASS means anything.
- **Success criterion:** with child-1's module as the base and no public type changed, the
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
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  capability_matrix_required` target that `cargo xtask ci` (gate C4-ci, gating) runs. It is a
  plain integration test with no `cfg` or feature gate, and every leg calls a pure production
  function with a fixed input. With production reverted to child-1's module the file compiles
  (no public type changes) and fails: every `required`-row leg expects a class or a pass where
  child-1 answers "unsettled", so the file is red on the base and green after. A classifier
  that lets a cut, an unfit ask or a lost follow-up hide a fatal fact; ignores a declared
  `Content-Length`; passes a weak or missing `ETag` on a PUT; passes a `<DeleteResult>` that
  omits, repeats or errors a key; passes a listing with an extra key; reads a 200 to a
  follow-up of a key the delete did not request as fatal, or a 200 to one of a requested key as
  anything but `oracle`; or reads a 404 to a written key as anything but `oracle` fails an
  assertion. No network, Docker or second process.
- **Invariant to restore:** the verdict never claims more than the answers prove, and never
  less. A `required` operation passes only on an answer showing it was done (`0017:190-195`);
  an answer the server gave that is not the defined one is a FAIL, never budgeted (the human's
  rule, 2026-10-24; §7 amended by child-1 to match); a fact that already proves the answer
  wrong is never hidden by anything that only leaves the exercise unproved (a cut never undoes
  a wrong byte, `0017:484-485`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** on child-1's module, `classify` of any `required` row with the defined
  answer returns unsettled, and `verdict(pre_0_1, <full defined run>)` is INCONCLUSIVE naming
  every `required` row. The new test file's first leg asserts PASS for both and is red.
- **Depends on:** 1005
- **Conflicts with:**
- **Ordering note:** second of two children of #937; builds on child-1's accepted result (the
  module, the observation type, the ladder, the verdict fold) and edits `matrix.rs` in place.
  #939 (743.3) must depend on BOTH children, not on #937 (`flow.py:1616-1620` in this harness
  levels a dependent by its own edges); its `Depends on:` is changed when the split is accepted.
  **Base:** child-1 folded onto `main` at or after `9b1a3e2` (#744, #853 and the cargo-deny
  fixes `6a0b369`, `6c79af3` present). Do must not touch `Cargo.lock` or `deny*.toml`, and
  records in `build-notes.md` which base it built on. **Do's first step, before any code:** in
  the worktree, run `git merge-base --is-ancestor 9b1a3e2 HEAD` and check that child-1's
  `crates/validate/src/matrix.rs` exists. If either fails, stop: write no code, put the
  supplied `HEAD` and what is missing at the top of `build-notes.md`, and do not rebase, merge
  `main`, or repair `Cargo.lock` / `deny*.toml` to get past it.
- **Scope:** in `crates/validate/src/matrix.rs`: the `required` rows' entries of the check
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
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/validate/tests/capability_matrix_required.rs` (NEW)
- **Verification posture:** default: a flippable red→green on C4-verify. The new test
  compiles against child-1's public surface, which this child does not change, so with
  production reverted it compiles and fails (see Falsifiability). If Do finds it must add a
  public item after all, that is a scope question for the human at sign-off, not a reason to
  switch to UNVERIFIABLE silently — say so in `build-notes.md`.
- **Production reach:** consumed by #939 in this same run; until then nothing in the binary
  calls the module. Every test leg calls the production functions, not a copy.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer to start from:** #937 iteration 9's `check_required` and helpers,
    `results/issue_937/iteration-v9/patch.diff` (`matrix.rs` hunk lines `:907-1270`:
    `expect_bytes`, `expect_no_body`, `whole_document`, `worst`, `check_required`,
    `probe_deleted`, `check_delete_objects`, `check_delete_result`, `is_strong_etag`,
    `parse_content_range`), and its `required`-row tests (`capability_matrix.rs` hunk,
    `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended` through
    `a_lost_or_unacknowledged_delete_result_never_hides_the_probes`, and
    `a_full_passing_run_against_pre_0_1_passes`). Review found no defect in the two grammars.
    The classification is restated by (B)–(G), not patched: the early returns
    (`check_delete_result`'s duplicate guard, `matrix.rs:1187-1193` there; `expect_bytes`,
    `:889-934`, which never reads `content_length`) are what (B) step 3 removes. Every check
    runs; the first failure in message order names the class.
  - The (F) facts against the gateway (`crates/gateway-s3/src/lib.rs` on `9b1a3e2`): PUT 200
    declares `content-length: 0` (`put_object_response`, `:3401-3404`); a 204 declares none
    (`empty_response`, `:3384`); ranged 206 declares `content-length` = the span and
    `Content-Range` (`:2704-2725`); 304 carries no body and no length
    (`not_modified_response`, `:2785-2803`); `DeleteObject` 204 `:2028`; `DeleteObjects` 200
    with errors `:2293-2300`. The client exposes a GET's declared length
    (`ObjectBody::content_length`, `crates/validate/src/s3/body.rs:431-433`).
  - The PUT receipt rule: `PutObject`'s doc, `crates/validate/src/s3.rs:169-178`;
    `BodyError::AcknowledgedEarly`, `crates/validate/src/s3/error.rs:101-114`.
  - Grammars: RFC 9110 §8.8.3 (entity tags) and §14.4 (`Content-Range`), under the
    grammar-strictness rule `AGENTS.md:165-169`.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md` across merged, open and
  closed-unmerged PRs on getwyrd/wyrd (re-run 2026-10-24; `main` unchanged at `9b1a3e2` on
  2026-10-25). Merged: #845, #849, #859, #860, #865, #886, #765, #931 (#744), #934 (#853).
  Open: none on these paths besides child-1's own draft PR. Closed-unmerged: none overlapping
  (iteration 9's reviewer checked all 19). The only prior `required`-row grading is #937's
  nine attempts (`results/issue_937/iteration-v1/` … `iteration-v9/`).
- **Disposition hint:** new-feature

## Design

- **Fixtures per class and per precedence pair, not per case.** For each class in (C), one
  fixture per rule that produces it; for each precedence pair the re-plan was written for, one
  fixture where the unproving fact comes first in message order and the fatal fact later, e.g.
  a whole GET whose `Content-Length` declares the wrong count and whose body is then cut after
  a correct prefix (`integrity`, not unsettled); a `DeleteObjects` naming a key twice whose
  `<DeleteResult>` carries an `<Error>` row (`protocol`, not unsettled); a bulk delete whose
  result document was cut off but whose follow-up `HEAD` answered 200 (`oracle`, not
  unsettled); a ranged GET with `b ≥ size` answered 404 (`oracle`, not unsettled); a matching
  conditional GET whose sent tag is not the object's (an unfit ask) answered 304 with one body
  byte (`protocol`, not unsettled). Where two fatal facts of different classes arrive, one
  fixture shows the first in message order winning: a ranged 206 with a wrong `Content-Range`
  and wrong bytes is `protocol` (the header comes before the body).
- **The follow-up key pair.** A follow-up's key decides which checks run, so these fixtures
  come in sets: `DeleteObject` of `a` → 204, then a follow-up `HEAD` of `b` (not requested)
  → 200 is unsettled, not fatal; of `a` → 200 is `oracle`; of `b` → 500 is `protocol` (a check
  that does not depend on the key still runs). The same three on `DeleteObjects` requesting
  `a` only.
- **The observation type is used, not changed.** Every fact (F) reads is already a field of
  child-1's type (status, `ETag`, `Content-Range`, `Content-Length`, error code; object bytes,
  listing, `<DeleteResult>`, unreadable-document, cut-with-arrived; the PUT's body-ended flag;
  the ask: range, sent conditional value, the object's tag and bytes for both conditional
  forms, size and tag for `HEAD`, requested keys, keys and sizes under the prefix; follow-ups
  keyed by the key asked about). If a fact is missing from the type, stop and report it in
  `build-notes.md` — it is a child-1 defect — rather than add a field here.
- **Version selection, the 0017 amendments and the verdict fold are child-1's** and are not
  touched. A run verdict's wording for a fatal `required` row (row and class) comes from the
  fold as child-1 wrote it; this child only makes the fatal results reachable.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Preserve unanswered/truncated follow-up evidence and correct the retry test — a complete 404 currently erases that evidence and falsely makes both delete rows and the full run PASS despite `brief.md:73`; `crates/validate/src/matrix.rs:1410`, `crates/validate/tests/capability_matrix_required.rs:868`, `reviewer-probe.log:2`.; The "wrong status" test passes for the wrong reason on three rows. `crates/validate/tests/capability_matrix_required.rs:1028` sends each wrong status with an empty body, and a different check then gives the same `protocol` class. I deleted each status check in turn and all 37 legs stayed green: `crates/validate/src/matrix.rs:1061` (ranged), `:1111` (`DeleteObjects`) and `:1127` (listings). Add legs where only the status is wrong: a ranged 200 with `Content-Range: bytes 2-5/16` and the right bytes; `DeleteObjects` answering 204 with a correct `<DeleteResult>`; a listing answering 201 with the correct keys. Production answers `protocol` on all three (I ran them), so this is a test gap only. The brief's (C) status rule applies to every `required` row, and these three rows have no leg that proves it.; Four more changes to production keep all 37 legs green, so nothing pins these behaviours:; Stale citation added by this patch. `crates/validate/src/matrix.rs:1358` cites `crates/gateway-s3/src/lib.rs:2293-2300` for "the gateway answers 200 even when keys failed". On this target those lines are the doc comment and first lines of `declared_content_length`, which has nothing to do with deletes. The fact is at `crates/gateway-s3/src/lib.rs:2396-2397` and `:2548`. The number was copied from the brief, which pins it to `9b1a3e2`, but the brief requires citations on the target branch.. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
