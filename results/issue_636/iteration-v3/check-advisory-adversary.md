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
