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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 31 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: implement issue #636’s crate-level multipart record family and bounded create, stage, fence, publish, abort, drain, and terminal-delete protocol without gateway wiring.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative proposal slices, exact typed outcomes, dependency stack, scope exclusions, and declared compile-shaped red make this implementation slice decidable without an unresolved Plan choice. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Human must accept the mandatory F/G/E mechanism-negation evidence at sign-off — my pre-fix replay failed compilation with 0 tests run, so it proves criterion absence but not that the concurrency, drain, and counter assertions are load-bearing (`crates/core/tests/multipart_admission_and_drain.rs:357`, `crates/core/tests/multipart_admission_and_drain.rs:610`, `crates/core/tests/multipart_protocol.rs:1304`). |
| C3 Change | PASS | The patch stays on the crate-level protocol and its shared metadata/write, compatibility-test, DST, and living-architecture seams, exporting the new core module without adding gateway wiring (`crates/core/src/lib.rs:13`). |
| C4 Verification (red→green) | PASS | Independent test-only base replay failed at compile time before running a test; with the patch, both new core suites passed 42/42 and full `cargo xtask ci` passed, including `typos`, docs render, dependency audits, conformance, and the 50-seed DST runner (`crates/core/tests/multipart_admission_and_drain.rs:358`, `crates/core/tests/multipart_admission_and_drain.rs:611`, `crates/dst/tests/concurrency.rs:747`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must carry a just-committed intent into compensation when fragment fan-out fails — the intent commits and the fan-out can error at `crates/core/src/write.rs:838` and `crates/core/src/write.rs:851` before `stage_chunk` records the chunk at `crates/core/src/multipart.rs:2635`, while compensation only visits the older `part.staged` list at `crates/core/src/multipart.rs:2697`; the full mutation campaign also remains provisional after its 7,200-second timeout. |
| T1 Structure | PASS | The module boundary is core protocol plus shared record/reclamation seams, compatibility fixtures, DST, and current architecture; the crate-level suite explicitly drives production functions with no gateway (`crates/core/tests/multipart_protocol.rs:4`). |
| T2 Shape | PASS | The required two new crate-level test binaries are present and the three concurrency cases extend the existing DST file rather than creating a new simulator root (`crates/core/tests/multipart_protocol.rs:1`, `crates/core/tests/multipart_admission_and_drain.rs:358`, `crates/dst/tests/concurrency.rs:747`). |
| T3 Runtime | PASS | Executed runtime evidence covers exact-byte assembly, 8/10/16 concurrent admission, the 4,001-part drain boundary, segmented flip loss, and the full seeded DST suite, all green (`crates/core/tests/multipart_admission_and_drain.rs:358`, `crates/core/tests/multipart_admission_and_drain.rs:611`, `crates/dst/tests/concurrency.rs:1013`). |
| T4 Contribution | NEEDS-HUMAN | Human must disposition the 31 reported blocking batch-review findings — `scripts/review-branch` and its report are absent from the permitted target, so that red gate cannot be independently reproduced or triaged; the contribution-artifact gate passed and affected-path merged/closed prior-art searches found no earlier multipart implementation. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add a `ChunkStore` failure after the owned intent commits and prove no `slot:` or `sidx:` residue remains — the current “staging error” case refuses an over-wide fan-out before intent (`crates/core/tests/multipart_protocol.rs:3923`), while its fleet’s fragment put always succeeds (`crates/core/tests/multipart_protocol.rs:372`), leaving the rollback-before-early-return defect untested. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer must decide fitness after the post-intent rollback fix, mandatory negation review, and batch-finding disposition — no client can exercise this deliberately crate-only slice until #508 supplies the gateway surface (`crates/core/tests/multipart_protocol.rs:4`). |

### Advisory — adversary

# Adversarial review — issue 636 (multipart commit protocol)

Advisory only; I never gate. Toolchain was available: I rebuilt the bundle in a scratch copy and
ran `cargo test -p wyrd-core` (138 lib + `multipart_protocol` 36 + `multipart_admission_and_drain` 6,
all green), then attacked the fix with targeted mutations and one purpose-built probe against the
**production** DELETE path. Two attempts landed; several did not.

## Findings

- NEEDS-HUMAN [human] — **The ordinary DELETE/overwrite path stops reclaiming space when this slice
  merges, and nothing in the shipped system ever un-stops it.** `crates/core/src/metadata.rs:3233`
  (`unlink` now calls `retire_generation`) and `crates/core/src/metadata.rs:714-728`
  (`generation_retires_inline`: inline only while `marks * 2 <= MAX_BATCH_OPS/2`, i.e. **≤ 250
  fragments**) route every larger generation to a `retire:bytes:g:<inode>:<version>` obligation —
  but no production code drains one. `drain_obligation` / `drain_step` have callers **only in
  tests** (grep across `crates/*/src`: zero); the loop that would call them is #625, explicitly out
  of scope. Concrete failing case, run in-process against the pristine bundle (probe:
  `metadata::create` a 30-chunk RS(6,3) object = 270 fragments ≈ 30 MiB at the deployed defaults
  `crates/server/src/lib.rs:49,51`; `metadata::unlink`; then `custodian::reconcile_step` at
  t=10,000,000 with a 50 ms grace):
  `orphan marks: 0 | retire obligations: 1 (retire:bytes:g:7:1) | fragments resident after GC: 270`.
  Pre-patch the same `unlink` wrote all 270 `orphan:` marks and GC reclaimed them, so this is a
  regression of a live path (`Gateway::delete_object`, `crates/server/src/lib.rs:582`, and both
  superseding committers at `:3349`,`:3407`), not new-protocol behaviour. Two claims are therefore
  unwarranted as written: `docs/design/architecture/08-crosscutting-concepts.md:85` ("a background
  drain writes the evidence in bounded batches … reclamation is if anything slightly delayed" —
  there is no background drain until #625, so it is suspended indefinitely, not delayed) and the
  brief's *Impact & compatibility* → "Client-visible: nothing" (an operator's disk fills; deleting a
  30 MiB object frees nothing). The brief carries a merge-order obligation only for **#508**
  ("#625/#633 with-or-before #508"); this regression lands with **#636** on paths #508 never
  touches, so that obligation does not cover it. The human owns the call: gate this slice's merge on
  #625, ship a caller here, or raise the inline ceiling for flat generations until the reaper exists.
  Note the bundle's own H2 oracle cannot see this — `FragmentClass::ObligatedForRetirement`
  (`crates/core/src/multipart.rs:5364`, applied at `:5587`) counts an installed, undrained
  obligation as a *safe* class, so `assert_no_classification_gap` reports "sound" on the leaked
  state, and `an_ordinary_overwrite_of_a_large_generation_retires_it_through_an_obligation`
  (`crates/core/tests/multipart_protocol.rs:1784`) only produces the evidence because the **test
  itself** calls `mpu::drain_obligation` at `:1852`.

- NEEDS-HUMAN [impl] — **The drain fence and the draining-server exclusion are not tested at all;
  both mutants survive the whole suite.** `crates/core/src/write.rs:828-830` (the per-server
  `require_absent(desired:dserver:<S>)`) and `:802` (`topology.excluding(&draining)`) are named
  in the brief's *In scope* and in `stage_intent`'s own doc as load-bearing (items 4 and 1). I
  deleted the entire `require_absent` loop → `cargo test -p wyrd-core` **green** (0 failures); I
  separately replaced `topology.excluding(&draining)` with `topology.clone()` → **green** again. No
  test anywhere writes a `desired:dserver:` key and then stages a part (grep over `crates/**`,
  including `crates/dst/tests/concurrency.rs`, whose three cases are publication CAS, slot cap and
  two drainers). Consequently the "re-plan on failure, not a refusal" behaviour
  (`MAX_STAGE_REPLANS`, `write.rs:727,853-860`) and `draining_servers`' fail-closed malformed-key
  arm (`write.rs:731-753`) are unexercised too. A part staged onto a server the operator is
  draining, with `reconciliation_status(S)` free to answer `Satisfied` while fragments are in
  flight, is exactly the hole `0016:837-857` describes — and the bundle would not go red for it.
  Buildable fix: one test that sets `desired:dserver:<S>` (a) before `upload_part` and asserts the
  placement avoids `S`, and (b) between the topology read and the commit and asserts the re-plan.

- NEEDS-HUMAN [impl] — **Stale in-code contract on the changed path.** `crates/server/src/lib.rs:567-576`
  still documents DELETE as "`metadata::unlink` writes an orphan grace record for each fragment in
  the *same atomic batch* … a crash never strands the bytes either." After this patch that is false
  for every object above the inline ceiling (finding 1). The patch updated
  `docs/design/architecture/{06,08}` but not the doc comment on the caller whose behaviour changed
  (AGENTS.md *Docs currency* / *Serialization identity* neighbourhood, `AGENTS.md:154-157`).

- NEEDS-HUMAN [human] — **A knob retune permanently wedges `CreateMultipartUpload`, with no path in
  this slice to unwedge it.** `mpuctl.profile` is written once by the bootstrap arm
  (`crates/core/src/multipart.rs:2145-2148`) and every later mutation preserves it
  (`..admission` at `:5266`, and the create CAS at `:2145-2148`); a process whose compiled
  `Budget::deployed()` differs refuses with `Refusal::ProfileSkew` (the create-path check at `:2070`). Concrete case: a later build changes `W_REF`, `MAX_CHUNKREF_BYTES` or `MAX_VALUE_BYTES`
  (the last two *derive* `MAX_PART_CHUNKS`, `multipart.rs:112-142`) — every create then fails for
  ever, including after `count` returns to 0, because nothing deletes or rewrites `mpuctl`. `0016:348`
  names the remedy ("an explicit operator CAS gated on the live population having drained"), but no
  such path is in this slice and none of #508/#625/#633 is stated to own it. This may be a legitimate
  decline-with-issue-reference (operator surface, out of scope) — that is the human's call, not a
  build defect.

## Attempted refutations that failed (the fix held)

- *Leg B is not vacuous.* I mutated `publish_fenced` to assemble the map from **every** staged part
  while keeping the ETag over the named subset (`crates/core/src/multipart.rs:3792-3800`) — exactly
  the wrong-bytes-with-a-right-ETag shape the brief warns about.
  `b_subset_complete_publishes_only_named_parts_and_evidences_the_rest` went red.
- *The `PendingEntry` round-trip is real.* Dropping `skip_serializing_if` on `owner`/`staged`
  (`crates/core/src/metadata.rs:2744-2748`) reddened
  `multipart::tests::pending_entry_round_trips_byte_identically_in_both_shapes`
  (`crates/core/src/multipart.rs:5775`) — the "`owner:null` ⇒ permanent Conflict" class is pinned.
- *The retirement routing boundary is pinned.* Forcing `generation_retires_inline` to `true`
  reddened both `an_ordinary_overwrite_of_a_large_generation_retires_it_through_an_obligation` and
  `a_stale_orphan_mark_is_restamped_by_the_event_that_unreferences_it`.
- *Leg F is not a sequential test in a concurrency costume.* `multipart_admission_and_drain.rs`
  asserts pairwise-distinct ids, exactly N `mpu:` records, `mpuctl.count == N` **and**
  `meta.commits() > creators + 1` (real CAS contention). I could not construct a passing-for-the-
  wrong-reason path through it.
- I could not find a wrong answer in `complete`'s verb×state table, in `tombstone_answer`'s
  fingerprint comparison (length-prefixed, injective), or in `terminal_delete`'s exactly-once
  decrement (`checked_sub`, fenced on exact session bytes and on `mpuctl`'s whole value).
- On the standing C2 doubt ("the red was a build error, so the new assertions may not be load-
  bearing"): the four mutations above are independent evidence that at least legs B, the routing
  boundary and the serialization-identity rule *are* load-bearing. That doubt should not be the
  reason to reject; findings 1 and 2 should be weighed instead.

## Notes on the gates

- C5 (`cargo mutants --in-diff`) is `unverifiable` — it timed out. Findings 2's two survivors are
  exactly what that gate would have reported, so treat the missing C5 verdict as material here, not
  as a formality.
- T4 (`review-branch`, 31 blocking) is red and its report is outside the permitted target, so I
  could not triage it; I did not attempt to.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Human must accept the mandatory F/G/E mechanism-negation evidence at sign-off — my pre-fix replay failed compilation with 0 tests run, so it proves criterion absence but not that the concurrency, drain, and counter assertions are load-bearing (`crates/core/tests/multipart_admission_and_drain.rs:357`, `crates/core/tests/multipart_admission_and_drain.rs:610`, `crates/core/tests/multipart_protocol.rs:1304`).
- [ ] C5 Causal adequacy — Rebuild must carry a just-committed intent into compensation when fragment fan-out fails — the intent commits and the fan-out can error at `crates/core/src/write.rs:838` and `crates/core/src/write.rs:851` before `stage_chunk` records the chunk at `crates/core/src/multipart.rs:2635`, while compensation only visits the older `part.staged` list at `crates/core/src/multipart.rs:2697`; the full mutation campaign also remains provisional after its 7,200-second timeout.
- [ ] T4 Contribution — Human must disposition the 31 reported blocking batch-review findings — `scripts/review-branch` and its report are absent from the permitted target, so that red gate cannot be independently reproduced or triaged; the contribution-artifact gate passed and affected-path merged/closed prior-art searches found no earlier multipart implementation.
- [ ] T5 Judgment — Rebuild must add a `ChunkStore` failure after the owned intent commits and prove no `slot:` or `sidx:` residue remains — the current “staging error” case refuses an over-wide fan-out before intent (`crates/core/tests/multipart_protocol.rs:3923`), while its fleet’s fragment put always succeeds (`crates/core/tests/multipart_protocol.rs:372`), leaving the rollback-before-early-return defect untested.
- [ ] Validation — fitness-to-purpose — Maintainer must decide fitness after the post-intent rollback fix, mandatory negation review, and batch-finding disposition — no client can exercise this deliberately crate-only slice until #508 supplies the gateway surface (`crates/core/tests/multipart_protocol.rs:4`).
- [ ] **The ordinary DELETE/overwrite path stops reclaiming space when this slice
- [ ] **The drain fence and the draining-server exclusion are not tested at all;
- [ ] **Stale in-code contract on the changed path.** `crates/server/src/lib.rs:567-576`
- [ ] **A knob retune permanently wedges `CreateMultipartUpload`, with no path in
- [ ] C5 surviving mutants on the bundle diff (cargo mutants --in-diff) unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 31 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
- [ ] C5 Causal adequacy — Decide whether causal sign-off can precede both a conclusive in-diff mutant run and stacked dependency #638—the configured mutant campaign timed out, while staged fragment writes still expose no server-enforced authorization deadline, leaving the late-fragment bound unexercised (`crates/traits/src/lib.rs:892`, `crates/core/src/write.rs:849`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: discontinued
- Iteration delta (if iterating): Scope too large for one slice: patch.diff is ~14,700 lines / >600KB for what the brief scoped as one logical fix. T4 batch review still has 31 unresolved blocking findings this round, C5 (mutants --in-diff) timed out unverifiable, and the adversary review found a real regression (large-file DELETE stops reclaiming space with no drain caller in this slice) plus untested load-bearing paths (drain fence, draining-server exclusion). Rather than iterate this monolithic slice again, split issue 636 into smaller, independently reviewable slices (e.g. record family/state machine vs. drain/reclamation wiring vs. admission knob handling) at the next Plan pass.
- By / date: Eduard Ralph / 2026-07-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
