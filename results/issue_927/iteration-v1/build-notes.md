# Build notes — #927 (657.2): UploadPart commit, classification, compensation, re-upload, ceilings

Target: `getwyrd/wyrd` @ `main`, base `376b106` (the worktree HEAD, equal to `origin/main`; #932 and
#933 are already folded in — `crates/core/src/multipart/stage.rs` carries `StagedAttempt` and
`StagedAttempt::renew`). `patch.diff` applies cleanly to that base (`git apply --check`).
All `path:line` citations below are against the **patched** tree unless marked `base:`.

## What the patch adds

| File | Change |
|---|---|
| `crates/core/src/multipart/commit.rs` (new) | `commit_part`, `upload_part`, the compensation, the classification, `CommitError`/`InDoubt`/`Stop`/`DoubtfulBatch`, `UploadPartError`, `COMMIT_COVER_MILLIS`, `COMMIT_ROUNDS`, `COMPENSATION_ROUNDS` |
| `crates/core/src/multipart.rs` | `pub mod commit;` (`:158`); `PartRecord::commit` (`:3029`) and `PartRecord::summary` (`:3058`), the crate-private writer + the `psum:` value derived from it; `RetireObligation::part_chunks` (`:4102`), the per-part `{chunks}` row (returns `None` for an empty list); `RecordError::PartChunkCountOverflow` (`:375`, Display `:714`); `Refusal::OperationAborted` doc widened (`:4666-4676`), no new variant; doc lines that said these writers did not exist yet (`:99-113`, `PartRecord` `:3006-3011`, `RetirePayload`/`RetireObligation` docs) |
| `docs/design/architecture/05-building-block-view.md` | sentences appended to the staging paragraph (line 246, the one #932/#933 wrote); line 244 untouched, as the brief asks |
| `crates/core/tests/multipart_upload_part_commit.rs` (new) | 18 tests, its own copy of the harness |

Untouched, as scoped: `crates/core/src/lib.rs`, `08-crosscutting-concepts.md`, proposal 0016, every
ADR, `Cargo.lock`, `deny*.toml`, `stage.rs` (the hand-off needed nothing from 657.1 that its public
`StagedAttempt` API does not already give: `session_bytes`, `slot_key`, `slot_bytes`, `owned`,
`chunks`, `digest`, `chunk_bound_exceeded`, `renew`). No production caller is added.

## Design, and why

**Flow** (`commit.rs:610-822`): per-part bound refusal first (`:618-626`; no renewal needed, a
compensation does not move a lease) → quiesce renewal `attempt.renew(store, clock,
COMMIT_COVER_MILLIS)` (`:627`; `COMMIT_COVER_MILLIS = E_TX_MILLIS` = 5 s, `:140`, the commit's
transaction deadline; a `Refused` from it is `NoSuchUpload`; any `StageError` is
`CommitError::Renewal` and no commit is sent) → `psum:` range read and cumulative ceiling
(`staged_elsewhere`, `:507`, excludes the attempt's own part number, since a re-upload replaces it)
→ part key read → one commit batch (`commit_batch`, `:528`).

**The re-read order is the whole design** (`commit.rs:726-760`): part key, session (σ1), own
slot, session (σ2), each processed in order through a `Doubt` ledger (`:376-439`):

* `cannot_land()` (`:401`) when the part key no longer holds what the commit requires **and that
  value cannot come back** (absent → present; a non-empty prior gone). An empty prior's bytes can
  recur (C1), so `can_recur` (`:681`) exempts it.
* `session_seen()` (`:392`) marks any doubtful batch whose required session bytes are gone as
  "may have landed, cannot land any more".
* `slot_held()` (`:409`) — the slot still at the bytes this attempt last wrote — drops every
  doubtful batch that can no longer land (it would have deleted that slot). Because σ1 and the part
  read happen **before** the slot read, a held commit is settled only by a precondition observed
  gone *before* the slot was observed held. That ordering is what makes "settled" sound: if a held
  batch landed in between, the slot read would show it gone.
* The own-commit proof (`:761-772`) is checked **first**: slot gone (absent or another attempt's
  id, `judge_slot` `:465`) **and** σ2 — read after the slot — still the commit's `Open@E` bytes
  **and** a commit of this attempt in doubt ⇒ `Committed`. Slot gone with nothing in doubt is
  `SlotLost` (damage), never `Committed`.
* Slot held with anything still in doubt ⇒ retry the **same** batch (`:795-800`), never compensate.
* Only once nothing is in doubt: same session + part unchanged ⇒ unexplained conflict, retry;
  same session + part moved ⇒ compensate under `E`, `OperationAborted` (`:806`); a newer `Open`
  ⇒ compensate under it, `OperationAborted` (`:812`); not `Open` ⇒ `NoSuchUpload` (`:817`).

**Compensation** (`compensate`, `:828-955`): reads every obligation key it sent (exact bytes ⇒
landed), then the slot, then the session. Slot gone + session read after it equal to the bytes a
doubtful compensation required ⇒ landed (`:912`). Slot held + newer `Open` ⇒ the epoch chase
(`:941`). Bounded by `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1` (`:154`), the same budget and
reasoning as `abort::ABORT_ROUNDS` (base: `crates/core/src/multipart/abort.rs:58`).

**Token for the per-part obligation**: `s:<id>:<epoch>:<part>:<attempt>` with the epoch the batch
requires. A commit's re-upload obligation and a compensation's would share a key at the same epoch,
but they are never both durable: the compensation is sent only after the commit is settled as not
landed and unable to land, and both carry `require_absent` on the key and require the same slot
bytes. The compensation re-read compares **bytes**, not presence.

**Slot bytes come from the attempt's own state, never from a read**: `commit_batch` `commit.rs:543`
and `compensation_batch` `commit.rs:572` both take `attempt.slot_bytes()` (base:
`stage.rs:493`, updated by the quiesce renewal). The wrapper checks this on every batch of every
leg (`tests/…commit.rs:488`, `check_slot_bytes`).

**`committed_at_millis`** is read once from the caller's `StageClock` (`commit.rs:664`), the clock
that owns the attempt's lease lifecycle (ADR-0009; stated in the module doc `commit.rs:101-105`),
so every retry of one commit puts the same bytes.

### Choices worth a look at sign-off

1. **⚑ `Refused(OperationAborted)` for both live-session losses** — the Plan's flagged choice; the
   variant's doc is widened (`multipart.rs:4666-4676`), no new variant. #669 maps it to the wire.
2. **A slot carrying this attempt's id but bytes it did not last write** (`SlotSeen::Rewritten`)
   is a `SlotLost` stop, not 0016's "benign retry: re-read the slot, rebuild the batch"
   (`0016:1636-1645`). After the quiesce renewal nothing of this attempt writes the slot, so the
   state is unreachable in a conforming run, and the brief forbids requiring re-read slot bytes
   ("never bytes re-read from the key"). Failing closed honours both.
3. **Slot gone after the session moved, with a commit in doubt** ⇒ `InDoubt { cause:
   SessionMoved, may_still_commit: false }` rather than `NoSuchUpload`. A teardown can remove the
   slot too, and for a `Completing` session a landed part could be published, so "no part" is not
   provable. C2 accepts either answer.
4. **`InDoubt::may_still_commit` is computed**, not copied: `true` only for a batch whose store
   report said so **and** whose required session bytes were still in place at the last read. A
   held commit whose session then moved is reported `false` (it can never land; epochs only rise).
   `outcome` keeps the store's own report. This mirrors Abort clearing its held doubt on a moved
   session (base: `abort.rs:419-431`).
5. **A quiesce-renewal error is returned, not compensated**: the residue stays for the reaper, the
   posture staging already takes for its own stops (`stage.rs` `StageError::Stopped` doc). A
   compensation there would have to reason about a renewal round still in doubt inside `stage.rs`.
6. **`max_staged_chunks` is a parameter** of `commit_part`/`upload_part`, like `StageRequest`'s
   `max_part_chunks`/`max_inflight_parts` (profile values). The test still uses the shipped
   `MAX_STAGED_CHUNKS` and plants `psum:` records, so no test-sized ceiling is involved.
7. **`EntityTooLarge` for the per-part bound reports `chunks: limit + 1`**: staging stopped at the
   bound without reading the rest of the body, so the true count is unknown; `limit + 1` is the
   least it can be. Documented on `commit_part`.
8. **`RecordError::PartChunkCountOverflow`** (new variant): `PartSummary.chunks` is a `u32`, so a
   chunk list longer than `u32::MAX` is refused by the writer rather than truncated. Unreachable in
   practice; the alternative was a panic or a silent saturation in shipping code.

### Ruled out

* **Reading the session only once, after the slot** (`part → slot → session`): treating "slot
  held, then session moved" as settled would be wrong, because a held commit can land between
  those two reads (while the session is still `Open@E`) and the session move only proves it cannot
  land *after*. With one session read, a held commit could only be settled by the part key or by
  a later round. Reading the session once before the slot (σ1, for settlement) and once after
  (σ2, for the own-commit proof) costs one `get` per round and makes both rules follow from read
  order alone.
* **Using part bytes as own-commit evidence for non-empty parts** (chunk ids are unique): sound in
  production, but it adds a branch no leg needs and the brief's rule is "never without the slot
  proof". Left out.
* **A session-wide counter or precondition for the cumulative ceiling**: 0016 rejects it
  (`0016:1686-1742`); the read stays unfenced and the witness test shows the bounded overshoot.

## Verification

### The project's gate

`./engine/xtask.sh ci` (the C4-ci row, `cargo xtask ci` in `$PDCA_WORKTREE`) exited 0 on the
patched tree: typos, `lint_docs.py`, `render_site.py --check` (link audit OK), gitlink/unsafe/
blackbox guards, `cargo fmt --check`, clippy `--workspace --all-targets` (warnings denied), build,
`cargo test --workspace` (223 test binaries `ok`, 0 `FAILED`; the new target's 18 tests ran),
cargo-machete, cargo-deny (advisories incl. the all-features config, licenses, bans, sources),
statics, deploy-guard, and the DST tier (`--cfg madsim` clippy + tests).
It was run twice: once on the first complete patch, and again on the **final** patch (after the
deferral marker, the header line and the added test assertions). Both exited 0, the second
ending `xtask ci: all checks passed` (223 `ok`, 0 `FAILED`).

### Red → green

The test file is net-new and calls `commit_part`/`upload_part`, which do not exist on the base, so
C4-verify's reverted leg cannot compile it (the brief's pre-declared UNVERIFIABLE, exit 77). The
binding evidence is instead the demonstrated reds below (recorded before the last test change,
which only added `Display`/class assertions to the error helpers and C2, so it cannot turn any of
them green): each is a single exact-string edit of
the production code, applied by `$PDCA_SCRATCH/pdca-builder-927-reds/reds.py`, the named test run
alone (`cargo test -p wyrd-core --test multipart_upload_part_commit <name> -- --exact`), then the
file restored (verified byte-identical with `cmp`, suite green again: 18/18).

| # | Edit (in `commit.rs` unless noted) | Named leg | Red, because |
|---|---|---|---|
| M1 | commit arm `Some(unknown) => doubt.observe(..)` → `return compensate(.., OperationAborted)` | `b_a_landed_commit_answered_unknown_is_committed` | answered `Stopped{SlotLost}` instead of `Committed` (it compensated a landed commit) |
| M2 | before the `cannot_land` check: `if doubt.any() && part_now.is_some() { return Ok(committed) }` | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` | one commit sent, not two: answered `Committed` on the prior part |
| M3 | same site: `if doubt.any() && part_now == Some(encode(&record)) { return Ok(committed) }` | `c1_an_empty_part_is_never_committed_on_its_bytes` | "a Committed attempt holds no slot": left 1, right 0 |
| M4 | `judge_session(last_found..)` → `judge_session(first..)` | `c2_a_torn_read_never_answers_committed` | gap 2 answered `Committed` |
| M5 | commit arm: observe the unknown only if `may_still_commit` | `c3_a_landed_commit_then_a_rival_is_committed` | `Stopped{SlotLost}` instead of `Committed` |
| M6 | `SessionSeen::Open(newer) => compensate(..)` → `Ok(Refused(NoSuchUpload))` | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | `NoSuchUpload` instead of `OperationAborted` |
| M7 | compensation arm: observe the unknown only if `!may_still_commit` | `c_an_unknown_compensation_outcome_is_settled_by_a_re_read` | `Stopped{CompensationRoundsSpent}` instead of `InDoubt` |
| M8a | commit loop `rounds += 1` → `rounds += 0` | `c_every_loop_ends_after_exactly_its_bound` | wrapper's runaway guard (`tests/…:592`) |
| M8b | compensation loop `rounds += 1` → `rounds += 0` | `c_every_loop_ends_after_exactly_its_bound` | runaway guard |
| M9 | `judge_slot`: another attempt's slot → `Held` instead of `Gone` | `c3_a_landed_commit_then_a_rival_is_committed` (C3′ run) | `Stopped{CompensationRoundsSpent}` instead of `Committed` |
| M10 | held branch `if doubt.any() { continue }` → `{ return compensate(..) }` | `b_a_held_commit_beside_an_unknown_empty_compensation_never_answers_operation_aborted` | "no compensation while the commit was held": left 1, right 0 |
| M11 | before `match slot`: `if part_now != required && !can_recur { return compensate(..) }` | `c3_a_landed_commit_then_a_rival_is_committed` | `Stopped{SlotLost}` instead of `Committed` |
| M12 | `multipart.rs` `part_chunks`: drop the `if chunks.is_empty() { return Ok(None) }` | `c4_empty_parts_never_install_an_empty_obligation` | the wrapper's decode check (`tests/…:411`) refuses the empty obligation |
| M13a | rival branch `Refusal::OperationAborted` → `NoSuchUpload` | `b_a_same_part_loser_compensates_its_own_chunks` | wrong refusal |
| M13b | rollback branch `Refusal::OperationAborted` → `NoSuchUpload` | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | wrong refusal |

The brief's list maps one-to-one: compensating without re-reading (M1); "a part is there" (M2);
part bytes as proof (M3); a session read before the slot read (M4); a possibly-landed commit
dropped from the doubt (M5); the rollback sent to the reaper (M6); an unknown compensation treated
as not landed with `may_still_commit: true` (M7); a loop counter that never reaches its bound
(M8a/b); slot gone read as key absent (M9); a compensation sent while held (M10); the rival-record
branch before the own-commit proof (M11); an empty `{chunks}` obligation (M12); a refusal other
than `OperationAborted` (M13a/b). The full log is `$PDCA_SCRATCH/pdca-builder-927-reds/reds.log`.

### Refuting my own test (forced)

* **(a) Genuine red?** Yes, for each named defect: the 15 edits above each turn the named leg red
  and the suite returns to 18/18 once the file is restored. Reverting the whole fix is not a
  meaningful red here: without `commit.rs` the test does not compile (net-new functions), which is
  the brief's declared UNVERIFIABLE posture, not a pass.
* **(b) Production path?** Yes. Every leg calls `wyrd_core::multipart::commit::commit_part` or
  `upload_part` over `RedbMetadataStore::in_memory()`; every session is made by the real
  `create_session`, every part staged by the real `stage_part`, every Abort is the real
  `abort_session`, and every rival commit is the real `commit_part` on the inner store. The only
  doubles are the store wrapper (it delegates to the real store and only reorders or withholds
  answers) and the chunk store (the production code under test never reads fragments back).
* **(c) Fixture includes the fault?** Yes: the rival, the Abort + teardown, the rollback, the
  third reserver (via the real `reserve_slot`) and the held batch are all injected by the wrapper
  at the exact point the leg names (before the commit, after its answer, or after the k-th read),
  and the held batch is really released onto the store afterwards in every leg that holds one,
  showing it lands or conflicts as the leg claims. The rollback writes a real `Open@E+2` record
  that `decode_session_record` accepts (`tests/…:1114`); no leg curates the failing element out.

### Slot bytes, by invariant (brief: no red assigned)

(1) `check_slot_bytes` (`tests/…:488`) runs on every batch through the wrapper in every leg and
fails the leg on a slot precondition whose bytes are not the ones that attempt last wrote (recorded
from its own landed reserve, intent, renewal and released held batches), and on a slot delete not
guarded by such a precondition. (2) The builders take the bytes from the attempt's own state
(`commit.rs:543`, `:572`). (3) A "require re-read slot bytes" mutant is unreachable under the
classification: a conforming path sends no batch after another attempt holds the index (it returns
`Committed` from the own-commit proof first), and while the slot is still its own the re-read bytes
equal its own. Listed among the survivors below for that reason.

## C5 (advisory): `cargo mutants --in-diff`

Run as the C5 row runs it (`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true`),
scoped to the new test target (`-- --test multipart_upload_part_commit`) so it finishes in
minutes; scoping can only over-report survivors, never hide one. Output under
`$PDCA_SCRATCH/pdca-builder-927-reds/m2/`.

**First run** (before the error-`Display` checks): 63 mutants, 38 caught, 12 unviable, 13 missed.
I then added `Display`/class checks to the `in_doubt` and `stopped` helpers (as
`multipart_abort.rs` and `multipart_create.rs` do), exact `Stop` text in the bounded-rounds leg,
and `!may_still_commit` in C2. **Second run, on the final patch: 63 mutants, 41 caught, 12
unviable, 10 missed.** The 12 unviable replace a return value with `Default::default()` on types
that have no `Default`; they do not compile, so nothing is tested.

| Survivor | Status | What would kill it |
|---|---|---|
| `multipart.rs:599` `RecordError` `Display` → `Ok(..)` | **caught by the full `wyrd-core` suite** (re-run alone with `-p wyrd-core`, no scoping: 1 caught) — `multipart_keys.rs` asserts exact `Display` text | — |
| `commit.rs:154` `MAX_COMPLETE_ATTEMPTS + 1` → `-` / `*` | **not killed — gap.** The bounded-rounds leg compares against the exported constant, so changing it moves both sides | a leg whose compensation loses to exactly `MAX_COMPLETE_ATTEMPTS` rollbacks and must then land (`OperationAborted`, obligation under epoch `2 × MAX_COMPLETE_ATTEMPTS`) |
| `commit.rs:348` `UploadPartError` `Display`; `commit.rs:357` its `source` → `None` | **not killed — gap.** No leg makes `upload_part` return an error (leg A only drives its success path) | `upload_part` ending in doubt (held commit, conflicts after), asserting `classify == Indeterminate` through `source` and the `Display` text; and a staging error through it |
| `commit.rs:419` `Doubt::required` → `true` | **not killed — gap.** No leg has a doubtful compensation whose session moves while the slot is gone | a held compensation followed by Abort + teardown before the re-read: must be `InDoubt { Compensation, SessionMoved }`, never `OperationAborted` |
| `commit.rs:862` `delete !` in the obligation dedup (the obligation is then never read back) | **not killed — gap.** In every leg where the obligation read is evidence, slot-gone-at-the-same-session is evidence too | a compensation that lands (answered unknown), then an Abort before the re-read: only the obligation proves it landed |
| `commit.rs:879` match guard `found == *bytes` → `true`; `commit.rs:449` `lossy` → `""` / `"xyzzy"` | **not killed — gap.** No leg reaches `Stop::ObligationKeyTaken` or `Stop::SlotLost`, so no leg reads the key text | foreign bytes planted at the attempt's obligation key (must be `ObligationKeyTaken { key }`, never "landed"); the attempt's slot deleted by something else under a definite `Conflict` (must be `SlotLost { key }`, never `Committed`) |
| (not generated) a builder that requires slot bytes re-read from the key | **unreachable under the classification** — see "Slot bytes, by invariant" above | — |

None of the 9 is an equivalent mutant: each changes an answer on a path the code can take. They
are all on damage or second-order paths (the brief's listed reds are all killed). My step to add
legs for them was interrupted before any of it ran, so those legs are **not** in this patch, and
I did not retry it. A follow-up (or an iterate-do) should add the legs in the right-hand column.

## NEEDS-HUMAN

None from external dependencies: the brief's `typos` and `docs-renderer` are both installed and
both ran inside the gate. The sign-off items are the brief's own: the ⚑ `OperationAborted` choice,
and C4-verify's pre-declared UNVERIFIABLE (net-new functions).
