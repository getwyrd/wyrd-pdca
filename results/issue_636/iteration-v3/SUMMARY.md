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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #636: implement the `crates/core` multipart commit record family, bounded retirement state machine, and seeded concurrency evidence without gateway wiring.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative proposal, scoped protocol slice, typed outcomes, fixed knobs, dependencies, and A–J evidence obligations are explicit enough to judge without inventing design. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the compile-shaped pre-fix red only after reviewing the withheld F/G/E negation output — my clean-base replay ran 0 tests, so it proves criterion absence rather than that the concurrency and drain assertions at `crates/core/tests/multipart_admission_and_drain.rs:357` and `crates/core/tests/multipart_admission_and_drain.rs:610` are load-bearing. |
| C3 Change | PASS | The change remains a protocol-layer implementation over the metadata/chunk seams, with the record family entering through `crates/core/src/multipart.rs:1697` and no S3 verb wiring. |
| C4 Verification (red→green) | PASS | With the patch applied, the two focused suites passed 30/30 and the workspace build/tests, typos, docs render, fmt, clippy, machete, isolated cargo-deny, conformance, statics, and 50-seed DST tier passed; the clean base plus added tests failed compilation with 0 tests run. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Require relational `mpuctl` validation plus a corruption regression — decode accepts `max_sessions != profile.max_sessions()` at `crates/core/src/multipart.rs:1477`, then admission trusts the inconsistent stored limit at `crates/core/src/multipart.rs:1753`, so an oversized torn value can violate the `W_ref` bound; the full mutation row also timed out. |
| T1 Structure | FAIL | The DST file is constrained to the three specified race classes, but it adds a standalone flat-publication scenario at `crates/dst/tests/concurrency.rs:1117`, expanding that suite beyond the stipulated structure. |
| T2 Shape | PASS | The new keyspaces are disjoint and the additive `PendingEntry` fields preserve legacy serialization identity via omission when absent at `crates/core/src/metadata.rs:2707`. |
| T3 Runtime | PASS | Real in-memory metadata/chunk paths passed exact-byte publication and 4,001-part draining, while the seeded tier exercised segmented flip loss through `crates/dst/tests/concurrency.rs:1013`. |
| T4 Contribution | NEEDS-HUMAN | Decide the disposition of the 21 recorded blocking batch-review findings — `scripts/review-branch` and its report are absent from the permitted target, so that red is provisional; the contribution gate passed and affected-path merged/closed prior-art searches found no earlier multipart implementation. |
| T5 Judgment | PASS | The suite observes durable bytes, reclamation evidence, and exact position/epoch classification rather than return values alone at `crates/core/tests/multipart_protocol.rs:3045`, and the patch adds no capability-probe symptom guard. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a protocol-only landing is fit before the required #625/#637/#508 follow-ons — the gateway still refuses multipart forms at `crates/gateway-s3/src/lib.rs:1696`, so users cannot exercise it and lifecycle exit/protection depends on correct follow-on sequencing. |

### Advisory — adversary

# Adversarial review — issue 636 (multipart commit protocol), advisory

Method: re-ran the bundle's own suites at `$PDCA_TARGET` (`cargo test -p wyrd-core` — 24 + 15
tests green), then attacked the fix with (a) two throw-away probe tests driving the **production**
verbs, and (b) two targeted source mutations in a scratch copy of the tree. Every `path:line`
below is the target source with the patch applied.

## Refutations that landed

- **NEEDS-HUMAN [impl] — a partly drained fleet answers `NoSuchUpload` for a session that is
  demonstrably `Open`.** `crates/core/src/write.rs:807` collapses "no admissible D server" into
  the same `StageOutcome::Refused` as "the fence was lost", `crates/core/src/multipart.rs:2218`
  flattens that to `CommitOutcome::Conflict`, and `crates/core/src/multipart.rs:2688` answers
  `Refusal::NoSuchUpload`. Demonstrated with a probe against the real verbs: mark 4 of the 12 D
  servers draining (`metadata::desired_dserver_key`), leaving 8 admissible failure domains — one
  short of RS(6,3)'s nine — then `mpu::upload_part` on an untouched session prints
  `outcome=Refused(NoSuchUpload)` while `read_session` still reports `Some(Open)`. That is a wrong
  *typed* answer, which is exactly what leg C of the brief pins: `Refusal::NoSuchUpload`'s own
  contract at `crates/core/src/multipart.rs:485-487` is "the upload id is unknown, aborted,
  already completed …, or its record is gone", none of which holds, and there is no
  `Backpressure` arm for an admissibility shortfall (`crates/core/src/multipart.rs:454-478`).
  #508 will map it to S3 `404 NoSuchUpload`, which aws-cli/boto3 treat as fatal — every client
  with a live upload abandons it and re-uploads, on a fleet that is merely draining. Exhausting
  `MAX_STAGE_REPLANS` (`crates/core/src/write.rs:870,:874`) is collapsed into the same answer.
- **NEEDS-HUMAN [human] — three client-side invalid Completes permanently wedge a healthy
  session, contradicting two claims this patch itself ships.** `crates/core/src/multipart.rs:3073`
  charges `attempts` on *every* Complete fence, and `fenced_session`'s `Open` arm
  (`crates/core/src/multipart.rs:2783-2790`) preserves it across the release. Probe output: three
  Completes carrying a stale digest each answer
  `Refused(InvalidPart { part_number: 1, reason: DigestMismatch })`; the **fourth, correct**
  Complete answers `Refused(CompleteAttemptsExhausted)` with the session still `Open`,
  `attempts=3` and every part durable — the only exit is Abort and a full re-upload. The
  `Refusal::InvalidPart` doc at `crates/core/src/multipart.rs:493-494` says "the fence is
  **released** before this is returned, so a client typo never wedges a session", and the
  architecture paragraph this patch adds at `docs/design/architecture/06-runtime-view.md:24` says
  "a rejected assemble list … leaves the session usable and abortable, **so a client's typo never
  wedges an upload**". Both are false at the third typo. `0016:660` does state the fence
  precondition as `attempts < MAX_COMPLETE_ATTEMPTS` unconditionally, while `0016:1477` / X58
  (`0016:2567`) derive the cap solely from fences that *mint segment epochs* — a validation
  refusal mints none — so which side gives (charge only fences that wrote segments, vs. correct
  the two claims) is a design call, not a builder call.
- **NEEDS-HUMAN [impl] — the drain fence, one of the two things the brief's Scope makes this
  slice's own, ships with zero red-able coverage.** Mutation experiment in a scratch clone:
  replacing `draining_servers(meta)`'s result with an empty set **and** deleting the whole
  `require_absent(metadata::desired_dserver_key(*dserver))` loop
  (`crates/core/src/write.rs:828-830`) leaves `cargo test -p wyrd-core` **fully green** — the
  mutant survives. Cause: no test in the repository ever writes a `desired:dserver:` key
  (`grep -rn desired crates/core/tests/*.rs crates/dst/tests/concurrency.rs` → 0 hits;
  `stage_intent` has exactly two references in the tree, its definition and
  `multipart::stage_chunk`). So `Topology::excluding(draining)`, the per-server precondition, the
  re-plan loop (`crates/core/src/write.rs:866-871`) and the fail-closed malformed-key arm
  (`crates/core/src/write.rs:748-752`) are all unasserted, and the finding above is what that
  blind spot was hiding.
- **NEEDS-HUMAN [impl] — the write path and the custodian read the same ledger record with two
  different rules.** `crates/core/src/write.rs:731-757` treats *presence* of
  `desired:dserver:<id>` as draining and never inspects the value, while
  `crates/custodian/src/desired_state.rs:152-167` requires the value to parse as
  `draining`/`decommissioning` and skips it otherwise. Concrete case: a record
  `desired:dserver:7 = "paused"` (any value the enum does not spell) makes server 7 permanently
  un-placeable for every staged intent — `require_absent` can never clear — while the rebalance
  loop never evacuates it, i.e. a drain that never progresses and a capacity loss with no
  operator-visible cause. The patch created this second reader, so the divergence is this diff's.
- **NEEDS-HUMAN [impl] — `DrainReport::marks`' contract is stale w.r.t. the three-arm guard the
  same patch introduces.** `crates/core/src/multipart.rs:3578-3579` documents "a position already
  marked is **skipped**, so this counts evidence created, not positions visited", but
  `mark_fragment` (`crates/core/src/multipart.rs:3998-4001`) now *re-stamps* an existing mark, so
  the counter includes re-stamps. Leg G asserts on this observable
  (`crates/core/tests/multipart_admission_and_drain.rs:725`, `:776`), so the doc and the assertion
  oracle disagree about what a non-zero `marks` means.
- **NEEDS-HUMAN [impl] — the leg-H2 oracle is position-blind for the `pending:` class, which is
  the exact defect its own comment forbids twenty lines below.**
  `crates/core/src/multipart.rs:4902-4914` marks **every** inventory fragment whose *chunk id*
  matches a `pending:` key as `StagedWithSession`, on any D server and any index, while the
  obligation arm at `:4916-4921` insists protection be keyed by `(chunk, index, dserver)`
  "because collapsing it to a chunk id would let *any* fragment bearing that chunk's id count as
  protected … That is the one thing this oracle exists to catch, answered 'safe'". A fragment at a
  position no record names is therefore reported sound whenever a lease for that chunk id exists,
  and `assert_no_classification_gap` runs after every scenario in legs A–H.
- **NEEDS-HUMAN [human] — the C4-verify row overstates what was measured (pre-declared, but it
  reaches sign-off as a gate PASS).** `check-gates.json` records "run-verify.sh: PASS — red
  without the fix, green with it." I confirmed on the bundle base that
  `git show HEAD:crates/core/src/multipart.rs` and `git show HEAD:crates/core/tests/multipart_protocol.rs`
  both fail — the module and both new suites are absent — so the pre-fix leg cannot have executed
  a single assertion; the "red" is a compile error, exactly as the brief's *Falsifiability*
  section declares. The load-bearing evidence (the F/G/E mechanism-negation runs) lives only in
  `build-notes.md`, which no gate reads and which is withheld from review, so the human must read
  §9 rather than treat this row as proof.

## Attempted and could not refute

- **The ETag composition and its oracle.** `multipart_etag`
  (`crates/core/src/multipart.rs:1352-1358`) hashes the raw 32-byte digests in ascending
  part-number order over exactly the named subset and appends `-N`; the test computes the
  expectation independently from the part bodies
  (`crates/core/tests/multipart_protocol.rs:441-452`), so it is not a tautology. Ordering,
  duplicate and out-of-range part numbers are all refused before assembly
  (`crates/core/src/multipart.rs:3181-3199`).
- **Leg B is load-bearing for the subset case.** Mutating `publication_batch` so the unnamed
  parts' `retire:bytes:` obligation is never installed (`crates/core/src/multipart.rs:2992`)
  **fails** `multipart_protocol` — the evidence assertion is real, not an ETag-only check.
- **Leg F carries its own negative control** (`crates/core/tests/multipart_admission_and_drain.rs:423-489`):
  a deliberately collapsed 4/4 retry budget is asserted to still refuse some of 16 creators, so
  the separation is what makes the green result meaningful, and the count assertion is `== N`.
- **Serialization identity.** `PendingEntry`'s two new fields are `skip_serializing_if`
  (`crates/core/src/metadata.rs:2707-2711`) with a byte-identity round trip on both a legacy
  `pending:` value and an owned `sidx:` value (`crates/core/src/multipart.rs:5184-5203`); I could
  not construct a decode→encode that differs.
- **The exactly-once admission decrement.** `terminal_delete` refuses to saturate
  (`crates/core/src/multipart.rs:4669-4674`), CASes `mpuctl` whole and pins the session's exact
  bytes; `teardown_session` never terminal-deletes a `Completed` tombstone
  (`crates/core/src/multipart.rs:4718-4760`), matching the brief's corrected leg E.
- **The drain's closing-cost accounting.** `drain_step` withholds the closing mutations from the
  walk budget up front (`crates/core/src/multipart.rs:4070-4075`); I could not find an input where
  `CLOSING_SLACK_BYTES = 256` / `CLOSING_OPS = 3` is exceeded, since the advanced obligation is
  the prior value plus one small cursor.
- **The abort obligation names the session rather than a frozen part list**
  (`crates/core/src/multipart.rs:3505-3520`), so a part landing in the fence window is still
  enumerated at drain time — the v2 leak class is genuinely closed.

Scratch worktree `${PDCA_SCRATCH}/pdca-adversary-151-mp` removed after the runs.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the compile-shaped pre-fix red only after reviewing the withheld F/G/E negation output — my clean-base replay ran 0 tests, so it proves criterion absence rather than that the concurrency and drain assertions at `crates/core/tests/multipart_admission_and_drain.rs:357` and `crates/core/tests/multipart_admission_and_drain.rs:610` are load-bearing.
- [ ] C5 Causal adequacy — Require relational `mpuctl` validation plus a corruption regression — decode accepts `max_sessions != profile.max_sessions()` at `crates/core/src/multipart.rs:1477`, then admission trusts the inconsistent stored limit at `crates/core/src/multipart.rs:1753`, so an oversized torn value can violate the `W_ref` bound; the full mutation row also timed out.
- [ ] T4 Contribution — Decide the disposition of the 21 recorded blocking batch-review findings — `scripts/review-branch` and its report are absent from the permitted target, so that red is provisional; the contribution gate passed and affected-path merged/closed prior-art searches found no earlier multipart implementation.
- [ ] Validation — fitness-to-purpose — Decide whether a protocol-only landing is fit before the required #625/#637/#508 follow-ons — the gateway still refuses multipart forms at `crates/gateway-s3/src/lib.rs:1696`, so users cannot exercise it and lifecycle exit/protection depends on correct follow-on sequencing.
- [ ] C5 surviving mutants on the bundle diff (cargo mutants --in-diff) unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
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
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the compile-shaped pre-fix red only after reviewing the withheld F/G/E negation output — my clean-base replay ran 0 tests, so it proves criterion absence rather than that the concurrency and drain assertions at `crates/core/tests/multipart_admission_and_drain.rs:357` and `crates/core/tests/multipart_admission_and_drain.rs:610` are load-bearing.; C5 Causal adequacy — Require relational `mpuctl` validation plus a corruption regression — decode accepts `max_sessions != profile.max_sessions()` at `crates/core/src/multipart.rs:1477`, then admission trusts the inconsistent stored limit at `crates/core/src/multipart.rs:1753`, so an oversized torn value can violate the `W_ref` bound; the full mutation row also timed out.; T4 Contribution — Decide the disposition of the 21 recorded blocking batch-review findings — `scripts/review-branch` and its report are absent from the permitted target, so that red is provisional; the contribution gate passed and affected-path merged/closed prior-art searches found no earlier multipart implementation.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
