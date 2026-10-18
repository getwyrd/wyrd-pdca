# Build notes — #894, multipart admission + Create (636.3a)

Target: `getwyrd/wyrd @ main`, built in the per-cycle worktree off the stacked integration base
`pdca-integration/main-rc423c4501a6d` @ `d2bd810` (`stack-base`, `stack-base-tip`). Line numbers
below are on that tree: "base" = before this patch, otherwise the patched file. The brief's
citations were pinned at `main @ 7bf7eb9`; the base here carries #671 and #767 on top, so several
of the brief's line numbers moved (for example `MAX_UPLOAD_ID_ATTEMPTS` is base
`crates/core/src/multipart.rs:5009`, the brief's `:4983`).

## What changed

**New store-I/O module** `crates/core/src/multipart/create.rs`, declared as a child of the pure
module (`crates/core/src/multipart.rs:146`, `pub mod create;`). A child module, not a sibling
crate module, so it can use the record types' crate-private creation routes without making them
public.

- `CreateRequest` (`create.rs:49`): bucket, dirent parent + object name, content type,
  `created_at_millis`, `clock_source` — all the caller's. Mirrors `write::commit_create`
  (base `crates/core/src/write.rs:266`): the store as `&impl MetadataStore`, the dirent identity
  `(parent, name)` spelled by the caller.
- `CreateRuntime` (`create.rs:72`): `mint()`, `jitter()`, `wait(millis)`. Core stays runtime-free
  (ADR-0009): identities, jitter and the backoff wait come from the caller, and the function reads
  no clock (the session's clock is the `created_at_millis`/`clock_source` pair the caller passes;
  that pair is what the reaper's `W_session` guard reads, `0016:1957-1990`).
- `CreateError` / `InDoubt` / `Unsettled` (`create.rs:87`, `:108`, `:124`): `IdentityCollisions`,
  `InDoubt` (upload id, nonce, `may_still_commit`, cause), `Ledger` (torn `mpuctl`), `Store`.
- `create_session` (`create.rs:272`). The batch is `admission_batch` (`create.rs:544`) with the
  bucket precondition in one place, `require_bucket` (`create.rs:568`), carrying the
  `// deferred: #511` marker (`create.rs:569`).

**Additions to `crates/core/src/multipart.rs`:**

- Creation routes, crate-private: `AdmissionRecord::bootstrap` (`:1894`) and `admit_one`
  (`:1907`); `SessionRecord::open` (`:2351`, Open at epoch 0, attempts 0) and
  `SessionRecord::widest_reachable_bytes` (`:2384`).
- `Refusal::SessionTooLarge { bytes, limit }` (`:4482`).
- `Budget::admission_cas_attempts` (`:1707`) = the profile's own `max_sessions`;
  `MAX_ADMISSION_CAS_ATTEMPTS` now *is* that derivation at the shipped profile (`:5195`; base
  `:5023` was `MAX_SESSIONS as u32`, same value 46).
- Doc updates the brief scoped: the header's "deliberately pure" paragraph (`:5-11`, base `:5-7`)
  and the living-architecture clause (`:95-115`, base `:93-110`); `MAX_UPLOAD_ID_ATTEMPTS`
  (`:5163-5172`), `MAX_ADMISSION_CAS_ATTEMPTS` (`:5174-5195`, no longer "fixed at
  `MAX_SESSIONS`", no longer "a refusal is the capacity bound"), `ADMISSION_BACKOFF_CAP_MILLIS`
  (`:5201-5204`); `Backpressure::AdmissionContention` (`:4437-4443`); the section-14 header comment
  (`:4837-4844`, base `:4679`, which said nothing is enforced); the `Budget` and `SessionRecord`
  "no free constructor" paragraphs (`:1566-1569`, `:2314-2319`).

**Other files:**

- `crates/core/tests/multipart_state_machine.rs:769` — the exhaustive-match test gains the new
  `Refusal` arm (the enum is deliberately not `#[non_exhaustive]`, base `multipart.rs:4256-4259`,
  so this break is the intended one).
- `crates/dst/tests/support/mod.rs:516-520`, `:700-724`, `:805-809` — a **mixed** nemesis:
  `Nemesis.code` becomes `Option<i32>`, `arm_mixed_commit_ambiguity(budget)` arms `None`, and a
  `None` strike draws its code from the existing seeded coin. The coin is drawn only on a mixed
  strike, so a single-code nemesis consumes the RNG exactly as before; `commit_ambiguity.rs` still
  passes 14/14 under `cargo xtask dst`.
- `docs/design/architecture/05-building-block-view.md:206` — the multipart paragraph now says
  Create writes `mpuctl`, `mpu:` and `seggrp:` with no production caller until #668, what it
  refuses, and what a contention refusal means.
- New tests: `crates/core/tests/multipart_create.rs` (19 tests) and
  `crates/dst/tests/multipart_admission.rs` (6 tests, `#![cfg(madsim)]`). Copies are in this
  bundle under the same paths.

No `Cargo.lock`, `deny*.toml`, ADR or proposal edit. No production caller (no gateway, CLI or
custodian wiring); `crates/server/src/lib.rs` still answers `NotImplemented`.

## The algorithm, and why each piece

One loop iteration = read `mpu:<id>`, `seggrp:<nonce>`, `mpuctl`, `bucket:<name>` (in that order,
`create.rs:516-541`), decide, then either return, re-mint, back off, or commit one batch.

1. **Size headroom first** (`create.rs:281-287`), before any read: the Open record built for the
   first identity is measured in its widest reachable state. The nonce is fixed width and the upload
   id lives only in the key, so a re-mint cannot change the answer and one check suffices.
   *Widest* = max over Open/Aborting/Completing/Completed, with `epoch`/timestamps/inode/version at
   `u64::MAX`, `attempts` at `u32::MAX`, `segments_written` at `MAX_SEGMENT_INDEX + 1`, an ETag at
   `MAX_PART_NUMBER` parts. Completing carries the object name twice, so it dominates for long
   names. Measured with `metadata::encode`, so escapes count.
2. **Ownership** (`create.rs:303-310`): a session found under the current identity is adopted only
   if a commit of ours under it came back unknown (`doubt.unknown`); since every identity's first
   iteration reads its two keys before any commit, evidence (1) of the brief's Design holds by
   construction. No content comparison anywhere.
3. **Collision** (`create.rs:311-328`): present `mpu:` or `seggrp:` with no unknown outcome ⇒
   re-mint, collision budget `MAX_UPLOAD_ID_ATTEMPTS`. If a `may_still_commit` doubt is held, the
   create may **not** re-mint (the old batch could still land unnamed) and ends `InDoubt`
   (`NonceTaken`) after a re-check.
4. **Bucket** (`create.rs:330-338`): the marker must be present and equal to the bytes this create
   first read; otherwise `NoSuchBucket`.
5. **Ledger** (`create.rs:340-367`): torn ⇒ `Ledger` error; profile skew ⇒ `ProfileSkew`; full
   (stored limit) ⇒ `SessionCap`; absent ⇒ bootstrap from the caller's profile.
6. **Contention accounting** (`create.rs:369-380`): only after the reads above, so an unknown
   outcome on the last budgeted attempt is settled by a read before the budget check can refuse
   (F4). A Conflict that turns out to be a collision or a bucket change never reaches this line, so
   a collision never spends contention. Unknown outcomes count as losses ("or the store could not
   report an outcome"). After a counted loss the create waits
   `admission_backoff_millis(losses - 1, jitter)` and reads again, so the next commit is built
   from state read after the wait.
7. **Commit** (`create.rs:382-395`): Committed ⇒ Created. Conflict ⇒ classified at the next read.
   `CommitUnknownResult` ⇒ `doubt.observe(msc)`; `may_still_commit` is OR-ed in and never
   cleared (`create.rs:462-467`). Any other `Err` is a definite failure of that batch (the trait's
   contract, base `crates/traits/src/lib.rs:1296-1318`) and ends the create, with earlier doubt
   still applied.
8. **`finish`** (`create.rs:490-513`): while a batch may have landed unseen (`may_still_commit` or
   an unsettled out-of-flight unknown) it reads `mpu:<id>` once more and answers `Created` if
   present; otherwise `InDoubt`. This is the brief's "settle before giving up", applied to every
   non-`Created` end, not only to the capacity refusal.

**Read order (identity keys first).** With `mpu:<id>` read first, a held batch released between
the reads makes the later `mpuctl` read see a full ledger while the earlier `mpu:<id>` read saw
nothing — exactly the F5 schedule — and only the re-check in `finish` answers it. Ledger-first
order would make the same schedule pass without the re-check, so F5 would not discriminate it.
Safety does not depend on the order: every re-drive carries `require_absent(mpu:<id>)`.

**Initial epoch 0.** 0016 never states the first epoch: §2's diagram writes `Open@E`
(`0016:528-560`), §1 calls it a monotone counter every fence bumps. 0 is that counter's first
value; `attempts` starts at 0 because it counts Complete fences (`0016:350`). `FenceGeneration`
counts from 1, but it is a different record whose 0 is reserved for "absent".

**Open question 1 (creation routes).** Neither route needed a decoder change.
`SessionRecord::open` writes only `Open`, which no cross-field rule binds; `content_type: None` is
omitted by the existing `skip_serializing_if`. `AdmissionRecord::bootstrap` sets
`max_sessions = profile.max_sessions()` (G8) and `admit_one` carries it unchanged. Both round-trip
byte for byte: `crates/core/tests/multipart_create.rs:202-229` asserts
`encode(decode(bytes)) == bytes` on every ledger and every session the tests read back, with and
without a content type (the (A) storm alternates them).

**Deletion fence carry-forward (#511).** ADR-0046 §4 (base
`docs/design/adr/0046-bucket-model-real-namespace.md:93-95`) asks for `require` on the marker
bytes **and** `require_absent` on the bucket's deletion fence (decision 5, `:104-121`). No fence
key exists, so only the marker half is implemented. The precondition lives in one function,
`require_bucket` (`create.rs:568`), marked `// deferred: #511`; #511 adds the fence's
`require_absent` there. Carry this forward to #511.

## Alternatives considered (and their cost)

- **Return an `InDoubt` error plus a public re-drive entry point** (the brief allows either).
  Rejected: the gateway would have to keep the identity and call back, and the re-drive would need
  the same loop anyway. Cost of the rejected shape, sketched: a
  `pub async fn resume_create(store, budget, request, doubt: InDoubt, runtime)` that re-enters the
  loop with a seeded `Identity`/`Doubt` (~25 lines), plus `InDoubt` gaining the session bytes so
  the resume builds the same record (~5 lines), plus a test leg per F case driving the resume
  (~60 lines). Core running the re-drive itself costs nothing extra: the loop already exists for
  contention.
- **A whole-record sanity check before adopting** (the brief permits one on top of the evidence).
  Not added. It can only be wrong in our disfavour: the post-restore fence
  (`crates/custodian/src/restore.rs`) scans `mpu:` and moves every `Open@E` session to
  `Aborting@E+1`, so a session of ours that landed a moment before the re-read can legitimately be
  found with other bytes; refusing to adopt it would strand it. And when the bytes do match, they
  prove nothing (two invocations write identical bytes). Cost of adding it: ~6 lines in
  `create.rs:303-310`; benefit only outside the stated guarantee.
- **The size refusal as a `CreateError` variant instead of a `Refusal`.** Rejected: an over-long
  object name or content type is the client's request, which the gateway maps to a 4xx
  (`KeyTooLongError`-like); `CreateError` holds the 5xx-like ends. Cost of the chosen shape: one
  arm in the exhaustive test, `multipart_state_machine.rs:769`.
- **Contention budget reset on re-mint.** Rejected: the budget is per create invocation; a
  re-mint is not evidence the ledger stopped moving. Losses persist across identities
  (`create.rs:293`); only `lost` (the unclassified previous commit) resets.

## Demonstrated reds (each a temporary edit to `create.rs`, then restored)

`sha256(create.rs)` before the edits was `06f4967b…8091`; it was re-checked to that exact value
after every restore. Commands: `cargo test -p wyrd-core --test multipart_create <filter>`; the DST
one with `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test multipart_admission`.

| Mutation | Leg | Result |
|---|---|---|
| `contention_budget = MAX_UPLOAD_ID_ATTEMPTS` | (A′) both runs | FAILED: `expected Created, got Ok(Refused(SlowDown { pressure: AdmissionContention { attempts: 2 } }))` |
| *(extra)* `contention_budget = MAX_ADMISSION_CAS_ATTEMPTS` (fixed shipped) | (A′) second run | FAILED: `... AdmissionContention { attempts: 46 }`; first run passes, as it should |
| re-mint on `CommitUnknownResult` | F1–F7 | all 7 FAILED (`no second upload id was minted: left 2, right 1`; F4/F5 `SessionCap`; F6 contention refusal; F7 mint count) |
| absence read removed, ownership by comparing the record found (`view.session == our bytes`) | (C) byte-identical | FAILED: `the existing session is never answered as Created`, left = right = `…0001` |
| recovery replays the original batch unchanged | F7 | FAILED: `... AdmissionContention { attempts: 46 }` (the stale `require(mpuctl == c)` never commits) |
| `may_still_commit = latest` instead of OR | F6 | FAILED: `expected the doubt to be kept, got Ok(Refused(SlowDown { pressure: AdmissionContention { attempts: 2 } }))` |
| *(extra)* no re-check in `finish` | F5 | FAILED: `got Err(InDoubt { …, cause: Refused(SlowDown { pressure: SessionCap { count: 3, max_sessions: 3 } }) })` |
| (G) `return Created` on a 1021 (`!may_still_commit`) without reading | (G) 1021 leg, mixed leg, 50-seed sweep | FAILED: `photos/object-1 answered Created for d171…07a5, but no such session exists — a Created that is not real` |

The DST file also keeps two **permanent** demonstrated reds (`#[should_panic]`,
`multipart_admission.rs:471`, `:483`), done with a misreporting store wrapper rather than a
production edit: a struck 1021 reported as `Committed` trips the reverse assertion (4), and a
struck 1031 reported as `Conflict` makes the create mistake its own landed session for a collision
and re-mint, stranding it, which trips (3). Both pass, so neither assertion is decorative.

## The three refutation questions

- **(a) Genuine red?** Yes, but not by reverting the whole fix: `create_session` is net-new, so the
  test cannot compile on the reverted base (C4-verify is expected to report **UNVERIFIABLE**, exit
  77, as the brief pre-declares). Instead each required property was reverted on its own (table
  above) and the named leg went red every time, then green on restore (19/19, 6/6).
- **(b) Production path?** Yes. Every leg calls the production `create_session` over the
  production `RedbMetadataStore` (core) or the shared `SimFdbMetadataStore` model (DST). The
  wrappers only delay, misreport or interpose *store* answers; none re-implements any create logic.
  The "other creates" a wrapper admits are real `create_session` calls on the inner store.
- **(c) Fixture includes the fault?** Yes. (A′) forces the losing schedule (the k-th admitted
  creator lost exactly k CASes, asserted as the multiset `{0..N-1}`); F legs inject real
  `CommitUnknownResult`s, applied / unapplied / held-and-released-late; the size leg goes through a
  wrapper that refuses a value over `MAX_VALUE_BYTES` the way FoundationDB does; the DST legs arm
  the simulator's own nemesis and require, from its counters, at least one unapplied 1021
  (`ambiguous_commits > ambiguous_commits_that_landed`), at least one 1031 left in flight and
  settled late, and at least one writer meeting 1031 then 1021.

Sweep evidence (one instrumented run, 64 seeds per leg, 6 creators for 4 places, 8 strikes;
the temporary stats test was removed afterwards):

| Leg | Created | Refused | In doubt | Strikes | Landed | Left in flight | 1031→1021 writers |
|---|---|---|---|---|---|---|---|
| 1021 | 255 | 129 | 0 | 420 | 227 | 0 | 0 |
| 1031 | 256 | 68 | 60 | 387 | 153 | 119 | 0 |
| mixed | 256 | 89 | 39 | 396 | 179 | 62 | 15 |

## Gate commands (run from the worktree through `engine/xtask.sh`)

- `cargo xtask statics` — pass: "no DST-reachable shared mutable global state (ADR-0035)". The
  wrappers keep state in struct fields only.
- `cargo xtask conformance` — pass: "5 valid + 6 invalid vectors pass".
- `cargo xtask dst` — pass (exit 0): `cargo clippy -p wyrd-dst (--cfg madsim)` clean, and
  `cargo test -p wyrd-dst` with `MADSIM_TEST_NUM=50` all green, including
  `multipart_admission` 6/6 and the unchanged `commit_ambiguity` 14/14.
- `cargo xtask ci` — pass (exit 0, "xtask ci: all checks passed"): typos, `lint_docs`, the docs
  renderer's link audit (99 pages), the gitlink/unsafe/blackbox guards, fmt, workspace clippy,
  build and test, cargo-machete, all three cargo-deny steps (`advisories ok`, so the
  RUSTSEC-2026-0346/0349 failure the brief inherited is gone on this base; no `Cargo.lock` or
  `deny*.toml` change was made), conformance, statics, the deploy guard, and the DST step.
- `cargo fmt --all -- --check` clean; `typos` clean on every changed file; the docs renderer is
  installed and runs inside `cargo xtask ci`.
- `cargo doc -p wyrd-core --no-deps` reports 22 pre-existing rustdoc errors (none gated); none is in
  `create.rs` or in a line this patch adds (the one in `multipart.rs`, `max_part_bytes` → private
  `one_value_bytes`, predates it).

## Self-review against the target's review rubric (`AGENTS.md`, "Review rubric & protocol")

- **One clock per lifecycle.** The create reads no clock. `created_at_millis` and `clock_source`
  come from the caller and are the pair the reaper's guard reads; the rustdoc says so
  (`create.rs:25-32`, `:62-68`). The tests' only time use is tokio's leg timeout; the DST runtime
  waits on madsim's virtual clock.
- **Narrow seams / dependency direction.** Core depends only on `wyrd-traits`'
  `MetadataStore`/`WriteBatch`/`CommitUnknownResult`; no new dependency, no concrete backend in
  `src/`.
- **Validation boundaries.** A torn `mpuctl` is a typed `CreateError::Ledger` (or the `InDoubt`
  cause), never a value; a ledger over a lowered cap is read liberally and refused as
  `SessionCap`, not as a decode error.
- **No DST-reachable global state.** `cargo xtask statics` passes; all wrapper state is in fields.
- **Docs currency.** New API operation and new persisted writers: the living-architecture
  paragraph (`05-building-block-view.md:206`) is updated in this patch.
- **Serialization identity.** Both creation routes round-trip byte for byte, asserted on every
  record the tests read back (`multipart_create.rs:202-229`), with and without `content_type`.
- **Absent or unsupported entries.** Every non-admitting end is an explicit `Refused(..)` or a
  typed `CreateError`; none is a silent success. A collision is never adopted.
- **Transactions.** `CommitUnknownResult` outranks every other end: while a batch may still land,
  the answer is `InDoubt`, never a refusal (F6).
- **Await discipline.** The loop is bounded by the two budgets (rustdoc "Termination",
  `create.rs:263-271`); each store call is bounded by the store contract; waits are capped at
  50 ms. No spawned tasks in production code.
- **Test fidelity.** The concurrent path lands with seeded Tier-0 DST coverage. The mixed nemesis
  mirrors production (FDB can return either code for any commit), and the existing single-code
  legs are bit-for-bit unchanged.

## Not done / for the human

- No `NEEDS-HUMAN external dependency`: `typos` (1.48.0) and the docs renderer (`markdown_it`,
  `yaml`) are installed and both ran inside `cargo xtask ci`.
- C4-verify will report UNVERIFIABLE (exit 77): `create_session` does not exist on the base, so
  the new test cannot compile there. This is the pre-declared sign-off item; the per-property reds
  above are the evidence in its place.
- Carry-forward to #511: the bucket deletion-fence `require_absent` belongs in `require_bucket`
  (`create.rs:568`).
- Carry-forward to #668: map `Refusal::SessionTooLarge` to a 4xx (it is the client's object name or
  content type), and decide what the S3 answer to `CreateError::InDoubt` is; the error names the
  upload id, and the reaper reaches the session if its batch landed.
- Scratch left for the harness to reclaim (not deleted, per the harness's filesystem rule):
  `$PDCA_SCRATCH/pdca-builder-894-{ci.log,dst.log,index,doc}` (the `doc` target dir is 118 MB).
