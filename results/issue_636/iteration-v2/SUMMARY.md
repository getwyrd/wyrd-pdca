# Result — issue 636 / multipart-commit-protocol

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  the multipart record family and its state machine, in `crates/core`, over the
  `MetadataStore` seam: `mpuctl` (the fleet admission ledger), `mpu:<id>` sessions,
  `slot:<id>:<k>` (the per-session in-flight key space), `part:<id>:<n>` with its `psum:` summary
  sibling, `sidx:<id>:<part>:<chunk>` (the **disjoint owned-staging** entry carrying `owner`
  **and** `staged`, deliberately *not* under `pending:`), `retire:bytes:` / `retire:records:`,
  and the verbs that move between them — create, stage, fence, publish, abort, drain, terminal
  delete. A caller with a `MetadataStore` and a `ChunkStore` can create a session, stage parts
  out of order, and publish an object whose bytes are the parts concatenated in part-number
  order — with every batch inside the transaction envelope and every failure leaving either a
  protected record or reclamation evidence.
- Success criterion:
  **two NEW test files** plus seeded DST cases appended to an existing
  one (see `Test file`), all crate-level over an in-memory store, **no gateway**.
  **(A) The happy path is exact.** Create → stage parts **out of order**, with at least one
  non-final part that is **not** a whole multiple of the chunk size (e.g. 5 MiB + 7 B) →
  Complete publishes a map that reads back **byte-identical** to the parts concatenated in
  part-number order. Assert the published bytes, not a chunk count: multipart chunking follows
  part boundaries, so a correct implementation legitimately yields a different chunk count from a
  plain PUT of the same bytes.
  **(B) The ETag is the settled pure function.** `etag = lowercase_hex( SHA-256( d₁ ‖ d₂ ‖ … ‖ d_N ) ) + "-" + N`,
  where `dᵢ` is the **raw 32 binary digest bytes** (not their hex text, no separators, no part
  numbers mixed in) of the *i*-th part in **ascending part-number order over exactly the parts
  the caller named**, and `N` is that count. **Never MD5** — ADR-0047 closed the basis
  (`docs/design/adr/0047-object-metadata-model.md:73-89`, lowercase-hex SHA-256 as an opaque
  change-token) and deferred only the composition (`:112`, `0016:3064-3070`). The test computes
  the expected value itself from the part bodies, so the oracle is independent of the
  implementation's choice.
  **The subset case needs BYTES and EVIDENCE, not just an ETag** (0016's own oracle, `:1033`:
  "Stage parts 1–3, Complete naming 1 and 3: the object MUST be parts 1+3, and part 2's **bytes**
  MUST end up orphan-marked while parts 1 and 3's bytes must not"). An ETag-only assertion is
  vacuous here: an implementation that computes the digest from the named subset, **publishes every
  staged part**, and deletes the unnamed records without evidence passes leg A's separate
  all-parts byte check, this leg's ETag check, and leg H's residue check — while returning wrong
  object bytes and leaking fragments. So assert, for a Complete naming a strict subset: the
  read-back bytes are **exactly the named parts in part-number order**; every **unnamed** part's
  fragments carry `orphan:` evidence **before** the record naming them disappears (marks precede
  the deletion of the records that name the bytes — X104, `0016:2633`); and **no published**
  fragment is orphan-marked.
  **(C) The decision-3 answer table, cell by cell, for every state this slice can reach without
  a reaper** (`0016:969-978`). Each asserted as a **typed outcome**, never "an error":
  `UploadPart` / `Complete` / `ListParts` after Abort → `NoSuchUpload`; a second Abort →
  idempotent success; an **identical** Complete retry inside the tombstone window → success with
  the **recorded** ETag; a Complete reusing the upload id with a **different** part list →
  `NoSuchUpload` (the `complete_fingerprint` rule, `0016:898-908`, `:2610`); a wrong part list →
  the invalid-part outcome **without publishing**; a part whose chunk count exceeds
  `MAX_PART_CHUNKS` → the too-large refusal **with the session still usable and abortable**
  (decision 4.4). The S3 status/code mapping is **#508's**; this slice pins the typed answers the
  wire layer will map.
  **(D) The staging class is the disjoint owned one, observed WHILE the part is in flight.** Not
  merely absent afterwards: at a mid-stage checkpoint assert `scan("sidx:<id>:")` is **non-empty**
  while `scan("pending:")` is **empty**; after the part commits assert **no `sidx:` entry and no
  `slot:` record remains for that part**. A post-hoc scan alone proves nothing — an implementation
  that staged under `pending:` and deleted it during the commit passes it while re-entering the
  global `pending:` scans and the #557 cross-clock expiry semantics for the whole life of the
  upload, which is exactly what 0016's disjoint class exists to prevent (`0016:480-490`,
  restore's bound re-derivation at `0016:834-841`).
  **(E) Admission is exact and bounded — and a `Completed` tombstone STAYS COUNTED.** `mpuctl`
  bootstraps on the first create (`require_absent` + put, `{count, max_sessions, profile}`, `0016`
  F12/X53); two open sessions read `count == 2`; a create past `MAX_SESSIONS` is refused with the
  typed backpressure outcome. For create + **abort** assert **both halves separately**: the abort
  *returns* from the fence commit alone (`count` unchanged — teardown is **not** on the request
  path, 0016 F9), and `count` returns to its prior value **only after** the bounded drain and the
  terminal delete.
  **CORRECTED 2026-07-26 — do NOT release the slot on Complete.** An earlier revision of this
  brief (inherited from the issue body's carried-forward list) demanded that `Completed` sessions
  release their admission slot and that ≥ 3 × `MAX_SESSIONS` sequential create→complete cycles all
  succeed. **That is unsatisfiable by a conforming implementation**, and requiring it would push Do
  into one of two defects. The authority is explicit, three times over: `count` "is the number of
  `mpu:` records that exist in any state" (`0016:348`); "Tombstones are **counted by the admission
  counter** (they still hold an `mpu:` record) and their retention is bounded by `W_tombstone`"
  (`0016:966-968`); "the counter counts **all** session records in any state
  (Open/Completing/Aborting/Completed tombstones)" (`0016:2029-2031`). The decrement happens in the
  **terminal session-delete batch**, and for a `Completed` session that batch is driven by
  `W_tombstone` — which is **#625's**, explicitly out of scope here. So the ≈70-upload ceiling an
  earlier attempt hit was **correct behaviour with no reaper running**, not a defect: satisfying
  the old leg would have meant either a second, non-authoritative counter (a deviation a previous
  builder already flagged in `results/issue_508/iteration-v6/build-notes.md`) or deleting the
  tombstone early, which destroys the identical-Complete-retry idempotence leg C and #508 both
  depend on.
  **What to assert instead:** after a Complete, `count` is **unchanged** and the `mpu:` record
  survives in `Completed`; the tombstone still answers an identical retry (leg C); `count`
  decrements **exactly once**, in the terminal delete, and this slice's only terminal-delete path
  is the **abort/teardown** one. Assert the counter is exact across every transition this slice can
  drive, and that no path decrements twice.
  **(F) Concurrent creates on an empty store all succeed — the carried-forward `503` bug.**
  **16 concurrent** `create` calls against an empty store all succeed. A single retry budget used
  for *both* upload-id collision (a 2^-128 event) and the globally serialized admission CAS makes
  the k-th concurrent creator need O(k) attempts: measured 8 → 2 refused, 10 → 3, 16 → 3, with
  the ledger nowhere near its bound, and aws-cli's default `max_concurrent_requests` is 10. The
  two retries are **separate concerns with separate bounds**, and the CAS-contention retry gets a
  real bound with jittered backoff. Assert at 8, 10 and 16 — not one number.
  **"All succeed" alone is vacuous** — an implementation that answers success without landing a
  session passes it. Assert additionally: the N upload ids are **pairwise distinct**; **N `mpu:`
  records exist** afterwards; and `mpuctl.count == N` **exactly** (not ≥ N, not N±1) — so a lost
  increment, a double increment, or a fictitious success all fail.
  **(G) The drain converges past the real boundary, and is idempotent.** A Complete naming
  **≥ 4,001 parts** (straddling `B_ops`, not sitting below it) drains to empty in **byte-budgeted**
  batches — assert the maximum observed batch's **encoded mutation bytes**, not its record count
  (`0016:1496`). Earlier attempts failed this twice: one re-derived the same first `B_ops` keys
  forever with no cursor (never converged), and a later one truncated derivation while still
  marking the obligation **fully drained** (silent permanent part loss). So assert **both**: the
  walk terminates, and every named record is actually gone. Running the drain **twice** over a
  partially drained obligation converges to the same terminal state with **no double-decrement**
  of `mpuctl.count`.
  **(H) After the terminal delete, nothing is left.** The session's `sidx:` range is empty, its
  `slot:`, `part:`, `psum:` records are gone, the `mpuctl.count` decrement happened **exactly
  once**, and the terminal delete is preconditioned on the session record's **exact bytes**. A
  session that reserved a `seggrp:` nonce it never adopted deletes the marker in the same batch;
  one whose group **was** adopted leaves it (`0016:513-527`, the two-arm rule — #635 ships the
  bounded predicate this gates on).
  **(H2) The no-gap classification invariant holds after every scenario.** 0016 earns one shared
  test helper from this protocol: given a store and a fleet, assert **every** on-disk fragment is
  in **at least one** safe class — committed-referenced, staged-with-a-session, or
  evidenced-for-reclamation — and that it is in no *genuinely incompatible* combination
  (evidenced-for-reclamation with its grace elapsed **while** still committed-referenced is the
  pair GC would act on wrongly). **"Exactly one" would be WRONG and would fail on correct
  executions** — this protocol deliberately overlaps protection across both handoffs
  (`0016:2906-2921`). Invariant (2) is a **no-gaps** claim, not a partition. Ship the helper in
  this slice and **run it after every scenario in legs A–H**; without it, a fragment that falls out
  of all three classes at a handoff is invisible to every other leg.
  **(I) Seeded DST for this slice's own races** (ADR-0009 is the correctness authority for
  interleavings, `0016:2877-2905`), **appended to the existing `crates/dst/tests/concurrency.rs`
  — NOT a new DST file** (see `Test file` for the gate reason; this is not stylistic). Three, and
  only these three, belong here — the rest of 0016's list is #625's or #637's: **(i)** publication CAS loss — two flip attempts against a prior that
  moves; the published `version` is `prior.version + 1` computed from the **re-read** prior at
  each attempt, never frozen at fence time (`0016:350`, matching
  `crates/core/src/metadata.rs:551`,`:595`,`:656`); **(ii)** the slot-reserve race at the cap
  (X41/X55) — concurrent part starts at `MAX_INFLIGHT_PARTS` produce no
  `MAX_INFLIGHT_PARTS + 1`-th key and no starvation; **(iii)** two concurrent drainers over one
  owned `sidx:` range (X56) — exactly-once effects, no double-decrement.
  **(J) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope:
  the multipart record family and its key helpers, decisions 1/3/4/5 and the
  protocol half of decision 6, the staged-write changes in `crates/core/src/write.rs`, the ETag
  composition, and the knob values inside 0016's ranges — all in `crates/core`, over the
  `MetadataStore`/`ChunkStore` seams. **Out of scope:** the S3 verbs, XML, HTTP status/error
  codes and routing (#508); the reaper loop and every window-driven exit (#625); operator abort /
  terminal expiry / foreign-clock alarm (#633); the custodian-side protection class (#637) — and
  in particular **no change to `reconcile_step`'s signature or `GcContext`'s fields**; any file
  under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016` itself.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 38 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #636: implement the core multipart commit protocol, including bounded admission, staged publication, retirement, and reclamation evidence.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the record model, state transitions, budgets, ETag, scope, prerequisites, and falsifiable A–J outcomes, so no material design choice remains open. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the mandatory F/G/E mechanism-negation evidence is persuasive at sign-off—the independent base replay failed at compile time with 0 tests run, so it proves criterion absence but not that those assertions are load-bearing. |
| C3 Change | PASS | The change stays on the specified protocol and maintenance seams with no gateway wiring; affected-path searches across merged history and every closed/unmerged PR found no prior multipart implementation to reconcile. |
| C4 Verification (red→green) | PASS | Independent stash/reapply replay produced the declared compile-shaped RED, then 18/18 focused tests, the workspace suite, all deny/conformance/statics checks, and the 50-seed DST campaign passed GREEN; the initial advisory-lock error was reproduced as a sandbox fault and all deny scans passed with a scratch-local Cargo home. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether causal sign-off can precede both a conclusive in-diff mutant run and stacked dependency #638—the configured mutant campaign timed out, while staged fragment writes still expose no server-enforced authorization deadline, leaving the late-fragment bound unexercised (`crates/traits/src/lib.rs:892`, `crates/core/src/write.rs:849`). |
| T1 Structure | FAIL | Persisted retirement ranges and cursors need validated decoding before this can satisfy the repository boundary rule—raw deserialization accepts reversed/overlapping ranges that can underflow or mis-drive cleanup (`crates/core/src/metadata.rs:197`, `crates/core/src/metadata.rs:224`, `crates/core/src/multipart.rs:3613`). |
| T2 Shape | PASS | The public and persisted shapes are additive, narrowly exposed through the existing store traits, preserve legacy optional-field serialization, and carry the required architecture-document updates. |
| T3 Runtime | FAIL | Exact admission accounting must reject a zero ledger paired with a live session—saturating subtraction silently deletes the session without performing the required exact decrement, concealing torn metadata (`crates/core/src/multipart.rs:4137`). |
| T4 Contribution | NEEDS-HUMAN | Decide the disposition of the recorded 38 blocking batch-review findings—the permitted target and artifacts do not contain `scripts/review-branch` or its report, so that red gate cannot be independently reproduced or triaged; the separate contribution-artifact gate passed. |
| T5 Judgment | NEEDS-HUMAN [impl] | The no-gap oracle must distinguish the exact segmented epoch before it can validate reclamation—the helper constructs the epoch-specific group but scans every epoch under the nonce, so another attempt can falsely protect missing chunks (`crates/core/src/multipart.rs:4397`, `crates/core/src/multipart.rs:4399`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this stack slice is fit to land before its production caller and #638—the protocol is intentionally unreachable from clients and its current fragment-write seam has no server deadline, so end-to-end late-write safety remains deferred (`crates/traits/src/lib.rs:892`, `crates/core/src/write.rs:849`). |

### Advisory — adversary

# Adversarial review — issue 636 (multipart commit protocol)

Attempted to refute: the ETag composition (leg B), the subset publish + evidence ordering
(leg B), the separated retry budgets (leg F), the admission counter's exactness (leg E), the
slot-space cap and the publication-CAS/version rule (DST i/ii), the fence-then-walk teardown,
and the `PendingEntry` round-trip identity — **could not**. Those legs drive the production
verbs, assert durable store state, and I could not construct an input that breaks them. Four
attacks did land; all four were reproduced by building and running the suite in a scratch clone
(`cargo 1.96.0`; both new test binaries green as shipped before each negation).

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:3423-3425`: `next_session_part` throws
  its cursor away and asks for `usize::MAX` rows, so the session drain silently truncates the
  moment a page comes back short.** `cursor.as_deref().map(|_| &[][..])` discards the computed
  key and passes `Some(b"")` (which `page_start` resolves to "start of prefix"), the limit is
  `usize::MAX`, and the returned `next` is dropped — so every call re-reads the *whole*
  `part:<id>:` range from row 0 and treats one page as the complete range. The seam forbids
  exactly this: `crates/traits/src/lib.rs:1054-1056` makes `next` the sole authority for
  exhaustion, and `:1069-1073` says a `limit` above the store's effective cap is **clamped**,
  never an error. Concrete failing case: a store built with `with_scan_cap(1_000)` (a supported
  inherent knob on all three backends — `crates/metadata-redb/src/lib.rs:86`,
  `crates/metadata-tikv/src/lib.rs:1083`, `crates/metadata-fdb/src/lib.rs:1336`) hands back
  1,000 rows; the marking walk then reports "no unit at or after 1001", declares marking
  finished, and the deleting phase removes **all 4,001** `part:`/`psum:` records anyway.
  Reproduced by changing only that limit to `1_000` in a scratch copy:
  `g_a_partially_drained_teardown_converges_with_no_double_decrement` fails at
  `crates/core/tests/multipart_admission_and_drain.rs:771` with `left: 1000, right: 4001` —
  3,001 fragments' bytes left unreferenced **and** unevidenced, which is refutation outcome (a)
  and the "truncated derivation marked fully drained = silent permanent part loss" defect the
  brief says a rejected attempt already shipped. Nothing but the accident that
  `SCAN_CAP (2^20) >= MAX_PARTS_PER_SESSION (10_000)` keeps this latent at default settings.
  Two further consequences of the same line: the doc at `:3408-3410` claims the range is read
  "one row at a time", but each unit materializes every `part:` record **with its values** (up
  to 10,000 × ~50 KB at `MAX_PART_CHUNKS`), which on TiKV/FDB is one transaction per unit
  against a 5 s deadline — a large teardown that cannot complete, permanently; and the walk is
  O(N²) — replacing it with a real exclusive cursor (`part_key(id, from - 1)`) and `limit 1`
  keeps **all 18 tests green** and drops that one test from 38.7 s to 27.5 s, i.e. the suite as
  written cannot see the difference.

- **NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:2934` and `:3050`: the live
  DELETE/overwrite path stops writing reclamation evidence, and nothing in this repository
  drains the obligation that replaces it.** `unlink` and `commit_chunk_map_superseding{,_leased}`
  now route through `retire_generation`, and `generation_retires_inline`
  (`crates/core/src/metadata.rs:486-500`) keeps the inline `orphan:` fan-out only up to 250
  marks — i.e. 27 chunks at the server's default RS(6,3) + 1 MiB chunk
  (`crates/server/src/lib.rs:48`,`:100`). Reproduced: a 30-chunk (270-fragment) object deleted
  through the production `metadata::unlink` — the call `crates/server/src/lib.rs:582` makes on
  every DELETE — commits, writes **0** `orphan:` marks, installs one `retire:bytes:g:42:1`
  obligation, and the fleet still holds all 270 fragments. `crates/custodian/src/gc.rs:150-198`
  reclaims only from `orphan:` marks or expired `pending:` leases and otherwise retains
  conservatively; it has no knowledge of `retire:` (that is #637), and `grep` finds **no
  non-test caller** of `drain_obligation`/`drain_step`/`teardown_session` anywhere in
  `crates/` or `xtask/` (the loop is #625). So from the instant this merges until #625 lands,
  every DeleteObject and every overwrite above ~28 MiB leaks its bytes permanently. That
  contradicts the brief's "**Client-visible: nothing.** No verb reaches this code until #508"
  and the code's own claim at `crates/core/src/metadata.rs:2929-2932` ("the only thing that
  moves is *when* the grace clock starts — at drain, never earlier than today"): with no
  drainer, the grace clock never starts. It is also the exact half of this slice's stated
  invariant that forbids "unprotecting without evidence". The human call — ship with a tracked
  merge-order obligation extended from #508 to *this* slice, keep the inline route until the
  reaper exists, or have some existing custodian pass drain `retire:` — is a scope/fitness
  decision, not something Do can settle by iterating.

- **NEEDS-HUMAN [impl] — `crates/core/tests/multipart_admission_and_drain.rs:735-753`: the test
  named `..._with_no_double_decrement` cannot detect a double decrement.** Its fixture creates
  exactly one session, so `before == 1`, and `terminal_delete`'s
  `admission.count.saturating_sub(..)` (`crates/core/src/multipart.rs:4136-4139`) clamps at
  zero: `before - 1 == 0` is satisfied by a decrement of 1 **or** 2. Reproduced by negating the
  production line to `saturating_sub(2)` — that test stays green (leg E,
  `crates/core/tests/multipart_protocol.rs:1168`, does catch it, so the property is covered, but
  not by the test that claims it). Two sessions in the fixture, or an assertion that the ledger
  moved by exactly one from a count > 1, restores the teeth the name promises.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:4232-4234` and `:4440-4441`: the leg-H2
  helper counts an undrained `retire:bytes:` obligation as a safe class, so the no-gap invariant
  passes over precisely the fragments of the finding above.** `FragmentClass::ObligatedForRetirement`
  is honest about 0016's overlap rule but is not a class any production consumer acts on, so
  `assert_no_classification_gap` (`crates/core/tests/multipart_protocol.rs:392`) reports
  "sound" for a store in which 270 fragments are unreferenced, unmarked and unreachable by GC.
  Two neighbouring arms widen it further: `:4363-4375` marks *every* inventory fragment sharing
  a chunk id as staged **regardless of D server**, and `obligated` at `:4440` is keyed by chunk
  alone — so a genuine gap on a sibling position is absorbed. The helper is run after every
  scenario in legs A–H, so its permissiveness sets the ceiling on what any of those legs can
  catch.

- **NEEDS-HUMAN [impl] — `crates/dst/tests/concurrency.rs:738` and `:867` assert a mark is
  "written when absent and SKIPPED when present, never re-stamped", which is not what
  `mark_fragment` does.** Production replaces the stamp on the present arm under
  `require(key == prior)` (`crates/core/src/multipart.rs:3582-3585`, documented at `:3556-3562`),
  and `crates/core/tests/multipart_protocol.rs:1993` asserts exactly that re-stamp. The DST
  `puts == 1` assertion holds only because the losing drainer's batch conflicts *whole*, not
  because anything skips — so the stated rationale is wrong, and if the present arm ever became
  reachable from `reclaim_owned` the DST leg would fail on correct behaviour. Same class, one
  file over: `crates/server/src/lib.rs:565-575` still tells the reader that `unlink` "writes an
  orphan grace record for each fragment in the *same atomic batch*", which is now false for
  every object above the inline threshold (AGENTS.md docs-currency rule).

**On the evidence itself (no bullet, pre-declared).** C4-verify's PASS is not evidence here and
the brief says so: the red is a build failure on a net-new module, and `run-verify.sh` scores
that as red without counting a single test. The compensating obligation — the F/G/E
mechanism-negation runs — lives in `build-notes.md`, which no gate reads and which this leaf is
not given, so I verified what I could myself: leg E does catch a doubled decrement (above),
and leg G's orphan-count assertion does catch a truncated marking walk — but only because the
in-test `MemMeta` has no scan cap, which is the first finding. `check-gates.json` already
carries C5 as `unverifiable` (7200 s timeout) and T4 as `fail` (38 blocking), so nothing here
rests on a green I did not re-run.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the mandatory F/G/E mechanism-negation evidence is persuasive at sign-off—the independent base replay failed at compile time with 0 tests run, so it proves criterion absence but not that those assertions are load-bearing.
- [ ] C5 Causal adequacy — Decide whether causal sign-off can precede both a conclusive in-diff mutant run and stacked dependency #638—the configured mutant campaign timed out, while staged fragment writes still expose no server-enforced authorization deadline, leaving the late-fragment bound unexercised (`crates/traits/src/lib.rs:892`, `crates/core/src/write.rs:849`).
- [ ] T4 Contribution — Decide the disposition of the recorded 38 blocking batch-review findings—the permitted target and artifacts do not contain `scripts/review-branch` or its report, so that red gate cannot be independently reproduced or triaged; the separate contribution-artifact gate passed.
- [ ] T5 Judgment — The no-gap oracle must distinguish the exact segmented epoch before it can validate reclamation—the helper constructs the epoch-specific group but scans every epoch under the nonce, so another attempt can falsely protect missing chunks (`crates/core/src/multipart.rs:4397`, `crates/core/src/multipart.rs:4399`).
- [ ] Validation — fitness-to-purpose — Decide whether this stack slice is fit to land before its production caller and #638—the protocol is intentionally unreachable from clients and its current fragment-write seam has no server deadline, so end-to-end late-write safety remains deferred (`crates/traits/src/lib.rs:892`, `crates/core/src/write.rs:849`).
- [ ] C5 surviving mutants on the bundle diff (cargo mutants --in-diff) unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 38 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Decide whether the mandatory F/G/E mechanism-negation evidence is persuasive at sign-off—the independent base replay failed at compile time with 0 tests run, so it proves criterion absence but not that those assertions are load-bearing.; T4 Contribution — Decide the disposition of the recorded 38 blocking batch-review findings—the permitted target and artifacts do not contain `scripts/review-branch` or its report, so that red gate cannot be independently reproduced or triaged; the separate contribution-artifact gate passed.; T5 Judgment — The no-gap oracle must distinguish the exact segmented epoch before it can validate reclamation—the helper constructs the epoch-specific group but scans every epoch under the nonce, so another attempt can falsely protect missing chunks (`crates/core/src/multipart.rs:4397`, `crates/core/src/multipart.rs:4399`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 38 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
