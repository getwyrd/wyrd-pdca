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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 550 mutants tested in 88m: 162 missed, 206 caught, 180 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 51 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #636: implement the crate-level multipart record family, bounded retirement, and fenced publish/abort state machine over the metadata and chunk-store seams.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The settled records, transitions, batch bounds, ETag oracle, exclusions, and dependency boundary are specific enough to decide conformance. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Confirm the mandatory F/G/E mechanism-negation runs failed as intended — my base replay ran 0 tests because the new tests did not compile, so it proves criterion absence but not load-bearing behavior. |
| C3 Change | FAIL | The slice owes bounded `retire:bytes:` routing for ordinary delete and overwrite, but those paths still fan out inline and reject segmented generations, leaving large operations outside the transaction envelope (`crates/core/src/metadata.rs:2390`, `crates/core/src/metadata.rs:2536`). |
| C4 Verification (red→green) | PASS | Independent scratch replay reproduced the compile-shaped red, then 12/12 targeted tests and the full `cargo xtask ci` passed; the initial read-only advisory-cache lock was discharged with a scratch-local Cargo cache. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Require a segmented root-flip-loss regression — the current DST race stays flat, so it cannot catch the stale resume/fence path, consistent with 162 surviving in-diff mutants (`crates/dst/tests/concurrency.rs:451`). |
| T1 Structure | FAIL | The public classification sweep performs a fleet-wide `sidx:` scan despite the per-session bounded-read invariant, so it fails at the scale the disjoint namespace was introduced to support (`crates/core/src/multipart.rs:4054`). |
| T2 Shape | FAIL | The session decoder accepts state-forbidden or missing cursor, target, and completion shapes, allowing half-understood persisted records past the metadata validation boundary (`crates/core/src/multipart.rs:1341`). |
| T3 Runtime | FAIL | After segmented phase progress, a root-flip loss retries from stale `resume_from` and fence bytes, so fence release can conflict and strand the session in `Completing` (`crates/core/src/multipart.rs:3072`). |
| T4 Contribution | FAIL | Contribution artifacts and affected-path prior-art checks are complete, but the contribution is not review-clean because independent review confirms the required ordinary retirement surface is omitted (`crates/core/src/metadata.rs:2536`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Require event-keyed three-arm orphan restamping or equivalent reader-grace proof — unconditional “present means skip” can retain expired evidence across a later unreference event (`crates/core/src/multipart.rs:3479`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether an intentionally unreachable protocol slice is fit to merge before #625, #637, and #508 — crate tests pass, but no client, reaper, or staged-byte protection consumer exercises the production lifecycle. |

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-adversary.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Confirm the mandatory F/G/E mechanism-negation runs failed as intended — my base replay ran 0 tests because the new tests did not compile, so it proves criterion absence but not load-bearing behavior.
- [ ] C5 Causal adequacy — Require a segmented root-flip-loss regression — the current DST race stays flat, so it cannot catch the stale resume/fence path, consistent with 162 surviving in-diff mutants (`crates/dst/tests/concurrency.rs:451`).
- [ ] T5 Judgment — Require event-keyed three-arm orphan restamping or equivalent reader-grace proof — unconditional “present means skip” can retain expired evidence across a later unreference event (`crates/core/src/multipart.rs:3479`).
- [ ] Validation — fitness-to-purpose — Decide whether an intentionally unreachable protocol slice is fit to merge before #625, #637, and #508 — crate tests pass, but no client, reaper, or staged-byte protection consumer exercises the production lifecycle.
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 51 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Confirm the mandatory F/G/E mechanism-negation runs failed as intended — my base replay ran 0 tests because the new tests did not compile, so it proves criterion absence but not load-bearing behavior.; C5 Causal adequacy — Require a segmented root-flip-loss regression — the current DST race stays flat, so it cannot catch the stale resume/fence path, consistent with 162 surviving in-diff mutants (`crates/dst/tests/concurrency.rs:451`).; T5 Judgment — Require event-keyed three-arm orphan restamping or equivalent reader-grace proof — unconditional “present means skip” can retain expired evidence across a later unreference event (`crates/core/src/multipart.rs:3479`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 51 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
