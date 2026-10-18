# Build notes — issue 656 / multipart-admission-create-abort

Target: `getwyrd/wyrd` `main` @ `7bf7eb9`. Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1`.
Every `path:line` below is on the patched tree unless it says "base".

## What the patch does

**New store-I/O module** `crates/core/src/multipart/store.rs` (642 lines), a child of the pure
module, declared at `crates/core/src/multipart.rs:148`. A child rather than a sibling because a
child can use the parent's private constructors, so `SessionRecord` and `AdmissionRecord` still
have no public constructor.

- `create_session` (`store.rs:270`) — Create, one batch per attempt (`create_batch`, `:403`):
  `require(bucket:<name> == marker read)`, `require(mpuctl == bytes read)` or
  `require_absent(mpuctl)`, `require_absent(mpu:<id>)`, `require_absent(seggrp:<nonce>)`, then
  puts of the `Open` session, the `seggrp:` marker (`SEGGRP_MARKER`) and the ledger at `count + 1`
  (or the bootstrap `{1, max_sessions, profile}`).
  - Profile is checked before the cap (`:360`): the stored limit belongs to the stored profile.
  - Lost batch → `lost()` (`:433`) re-reads in this order: own session at `mpu:<id>` → `Created`;
    bucket marker gone/changed → `NoSuchBucket`; foreign `mpu:<id>` or taken `seggrp:<nonce>` →
    collision; otherwise → contention.
  - Two counters, never shared: `minted` (collision budget, `:302`) and `contended` (contention
    budget, `:310`). A collision does no backoff; contention backs off with
    `admission_backoff_millis(contended - 1, host.jitter())`.
- Unknown outcome (`CommitUnknownResult`): the identity is kept. The next attempt first re-reads
  `mpu:<id>` (`:334`): own session → `Created` (no second increment). With
  `may_still_commit == false` the absent re-read clears the doubt. With `true` the doubt stays
  until a batch of this identity is seen committed; while in doubt every exit except `Created` is
  `CreateFault::Unsettled { upload_id, segment_nonce, unknown, ended }` — never a refusal, never a
  re-mint. The re-drive rebuilds the ledger precondition from a fresh read because every attempt
  re-reads `mpuctl`. Unknown outcomes spend the contention budget so a store that never reports
  an outcome cannot loop the create forever.
- `abort_session` (`store.rs:563`) — reads `mpu:<id>`, answers through the existing pure
  `abort_answer` table (so the table stays in one place), fences an `Open` session with
  `fence_batch` over `open_teardown`'s output. On `Conflict` it re-reads once: unchanged session
  ⇒ the obligation key is taken ⇒ `AbortFault::ObligationKeyTaken` (damage, as restore treats
  it); changed ⇒ answer from the new state (re-fence at a later `Open@E'`). Unknown outcome ⇒
  re-read and continue (the identical batch is self-excluding: both copies `require` the same
  session bytes). Bounded by `ABORT_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1` (`:493`); exhaustion is
  `RoundsExhausted`, or the last `CommitUnknownResult` if the last round was unknown. Never
  touches `mpuctl`.
- `fence_batch` (`store.rs:626`) — the shared teardown batch builder: `require(mpu:<id> == read)`,
  `require_absent` each obligation key in order, put the session, put each obligation in order.
  Same order as restore's inline code at base `crates/custodian/src/restore.rs:1045-1057`.

**`crates/core/src/multipart.rs`**
- Header updated (`:5-9`, `:95-117`) to say this file stays pure and the round trips live in
  `store`, which writers exist, and that no production path calls Create/Abort until #668.
- `AdmissionRecord::bootstrap` (`:1880`) and `AdmissionRecord::admit` (`:1892`) — private
  `const fn`s. Both keep G8 (`max_sessions == profile.max_sessions()`) by construction; `admit`
  returns `None` at `count >= max_sessions`, so the cap check *is* the constructor.
- `SessionRecord::open` (`:2338`) — the one creation route: request fields + minted nonce,
  `epoch 0`, `attempts 0`, `Open`. Private.
- Doc updates on `SessionRecord` (`:2302`) and `RetireObligation` (`:3796`).

**`crates/custodian/src/restore.rs`** — `fence_session` now calls
`fence_batch(&fence.upload, read, &fence.session, &fence.obligations)` (`:1052`) instead of the
inline builder; `Fence` gained `upload: UploadId` (`:1102`), set in both arms of `plan_fence`
(`:1117`, `:1126`). The `keys` vector stays: restore still uses it for the audit names and the
conflict re-check. `mpu_key(parse_mpu_key(key)) == key` holds for every key that parses (the
module header's canonical-key rule), so the `require` key bytes are identical to before.
Restore's existing tests ran unmodified and green in `cargo test --workspace` (see Gates).

**`docs/design/architecture/05-building-block-view.md:202`** — the multipart paragraph now says
"ahead of most of their writers" and describes Create and Abort as landed in `wyrd-core` with no
production caller until #668; replaces `b2d3922`'s "arrives with the store round trip
(#656–#659)" sentence.

**Tests** — `crates/core/tests/multipart_create_abort.rs` (NEW, 17 tests) and
`crates/dst/tests/multipart_admission.rs` (NEW, madsim-only, 4 tests).

## Decisions, and what I ruled out

1. **Every-mint-collides is a typed error, not a `Refusal`.** The brief's (C) says "it refuses".
   I return `Err(CreateFault::IdentitiesExhausted { attempts })` and write nothing. Reasons: the
   vocabulary has no variant for it, and the two that could be bent are wrong — `Backpressure`
   is documented as "designed behaviour, not a failure" and `AdmissionContention` as "lost the
   admission CAS"; a second 2^-128 collision means the caller's randomness is broken, which S3
   answers as `500 InternalError`, not `503 SlowDown`. Adding a new `Backpressure` variant would
   cost one more arm in `crates/core/tests/multipart_state_machine.rs:754-760`
   (`exhaust_backpressure`) plus #668's mapping — cheap, but it would label a server fault as
   client backpressure. **Human: if you read "refuses" as requiring a `Refusal` value, this is
   the one place to change** (`store.rs:302-304`).
2. **Core runs the unknown-outcome re-drive itself** (the brief let Do choose). The alternative
   — return an error carrying the identity plus a `resume` entry point — needs a second public
   entry point and a public identity type (~40 lines) and pushes the loop into #668. With core
   owning the loop, the gateway never sees the batch. The residual: when the budget runs out
   while still in doubt, the create returns `Unsettled` with the identity, so #668 can log it
   and the reaper (#625) reclaims it if it lands.
3. **In doubt, refusals become `Unsettled`.** While a `may_still_commit == true` batch is out,
   answering `SessionCap`/`ProfileSkew`/`NoSuchBucket`/`AdmissionContention` would tell the
   client "not created" while a counted session may still land under an id nobody knows — the
   stranded-session case `0016:2009-2027` exists to close. Tested by
   `f_an_in_doubt_create_never_answers_a_refusal_that_strands_its_session`.
4. **A′'s forced schedule is a commit barrier, not a read barrier.** The brief says "every
   creator's `mpuctl` read is held until all live creators have read it, before any of them may
   commit". A barrier on `get(mpuctl)` cannot tell the attempt's read from the conflict
   classifier's re-read (gets carry no creator identity). Holding every *commit* until every
   live creator has reached its commit gives the same guarantee — every creator of a round read
   the ledger before anyone committed — and the test asserts the forced shape directly: the
   per-creator loss counts are exactly `{0,1,…,7}`. The wrapper tells creators apart by the
   `mpu:<id>` put in each batch, as the brief says.
5. **(C)'s "a collision never spends the contention budget"** needs a create that loses the CAS
   `MAX_ADMISSION_CAS_ATTEMPTS - 1` (= 45) times and also collides once. Among creates each lost
   CAS is one more admitted session, and the shipped cap is 46, so the collision is on a
   pre-existing `seggrp:` marker that outlived its session — a legitimately uncounted record
   (`0016:499-527`) — so `count == #mpu` still holds. 45 rivals + the target fill the ledger to
   exactly 46.
6. **`fence_batch` takes `&UploadId`, not the raw key.** Type-safe; restore already had the
   parsed id. Cost: one field on restore's private `Fence` struct (3 lines).
7. **Initial epoch 0, attempts 0.** 0016 §1 does not name a number; every fence bumps from the
   base, and 0 makes `Aborting@1` the first teardown. Restore's recheck of an `Aborting@1` session
   looks for segments at epoch 0, which an `Open@0` session never wrote — so restore is unaffected.

## Carry-forward: the bucket deletion fence (#511)

ADR-0046 §4 asks object writes to `require` the marker bytes **and** `require_absent` the
deletion fence. No fence key exists on `main` (decision 5 only sketches `bucket-deleting:{name}`
and leaves it to #511), so Create carries only the marker half. The precondition is in one
place, `create_batch` (`store.rs:413-417`), with an in-code `// deferred: #511` marker so #511
adds the `require_absent` there. Until #511 lands nothing can put a bucket into a deleting
state, and no deployment can create a session before #668 anyway. **Human: the plan-review
response asked you to confirm this deferral.**

## Open question 2 — constructors

No decoder was loosened. Both new writers produce values the existing decoders accept and
re-encode byte-identically, because they are built from the same validated types the decoders
produce and serialized by the same `Serialize`:
`assert_admitted` re-encodes every stored session and compares bytes (both with and without
`content_type`), and every ledger read goes through `decode_admission_record`, which refuses a
non-canonical spelling.

## Demonstrated reds (required by the brief) — temporary edits, all reverted

Each was a temporary edit to `crates/core/src/multipart/store.rs`, run with
`cargo test -p wyrd-core --test multipart_create_abort` (the command C4-verify runs), then the
file was restored from a saved copy and diffed clean.

**Red 1 — (A′) with the contention budget sized like the collision budget** (the #508 defect).
Edit at `store.rs:310`: `contended >= MAX_UPLOAD_ID_ATTEMPTS.min(MAX_ADMISSION_CAS_ATTEMPTS)`
(i.e. 2). Fails on every run — 5 of 5 — because the barrier forces the losing order:

```
test a_prime_every_creator_is_admitted_through_more_lost_cas_than_the_collision_budget ... FAILED
panicked at crates/core/tests/multipart_create_abort.rs:212:44:
the create was refused: SlowDown { pressure: AdmissionContention { attempts: 2 } }
run 1..5: test result: FAILED. 0 passed; 1 failed; ... 16 filtered out
```

Two more shapes of the same defect, also red:
- one counter shared by both (`contended += 1` after a re-mint, at `store.rs:306`) —
  `c_a_collision_does_not_spend_the_contention_budget ... FAILED` /
  `the create was refused: SlowDown { pressure: AdmissionContention { attempts: 46 } }`;
- a lost CAS spending the collision budget (`minted += 1` + exhaustion check in the contention
  arm) — A, A′ and C3 all fail with `the create answered: IdentitiesExhausted { attempts: 3 }`.

**Red 2 — (E) with a temporary per-session decrement on Abort** (the Abort batch also
`require`s `mpuctl` and puts it at `count - 1`). The count assertion fires:

```
test d_abort_fences_an_open_session_with_its_obligation_in_one_batch ... FAILED
assertion `left == right` failed: (E) the fence never moves the count
  left: 0
 right: 1
test d_an_unknown_fence_outcome_is_settled_by_a_re_read ... FAILED
assertion `left == right` failed: applied: (E)   left: 0  right: 1
test d_two_concurrent_aborts_answer_fenced_and_already_aborting ... FAILED
assertion `left == right` failed: (E) the count never moves   left: 0  right: 1
test result: FAILED. 14 passed; 3 failed
```

**Red 3 — (F) with a temporary re-mint on an unknown outcome** (`Step::Unknown` split out of the
contention arm: drop the doubt, mint a new identity). The one-session / one-increment
assertion fires:

```
test f_an_unknown_create_outcome_is_recovered_under_the_original_upload_id ... FAILED
assertion `left == right` failed: F1 applied: exactly one session for this create:
  [UploadId("00000000000000010000000000000001"), UploadId("00000000000000010000000000000003")]
  left: 2
 right: 1
test f_an_in_doubt_create_never_answers_a_refusal_that_strands_its_session ... FAILED
```

**DST reds** (same two mutations, `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=5 cargo test -p
wyrd-dst --test multipart_admission`):
- re-mint on unknown → all four DST tests fail on
  `every session is one a Create answered Created for — none stranded, none lost` (the store
  holds 8 sessions; Creates answered 6 — two stranded sessions under ids no client got);
- decrement on Abort → `1. every mpu: record is counted, and nothing else is  left: 6  right: 3`.

I did not demonstrate a red for "the re-drive rebuilds the ledger precondition from a fresh
read" by mutation (it would need a stored-batch replay path added to `create_session`). The
assertion that covers it is in `f_an_unknown_create_outcome_is_recovered_under_the_original_upload_id`,
variant "F2, the ledger moved": a rival moves `mpuctl` after the unknown outcome, and the test
requires the re-drive to commit without ever losing (`state.lost.is_empty()`). A replay of the
in-doubt batch carries the old `mpuctl` bytes and would lose, so it would trip that assertion.

## Refuting my own test

- **(a) Genuine red?** Yes — each red above (three required, two extra budget shapes, two DST) turns a green test
  red on the assertion that names the property. The plain revert-the-fix red is a compile
  error, because the functions do not exist on base (see Verification posture).
- **(b) Production path?** Yes. Both test files call the production `create_session`,
  `abort_session` and `fence_batch` against a real `RedbMetadataStore::in_memory()` (core) or the
  repo's existing simulated-FoundationDB model (DST). The test-local `Interposer` only delays,
  reorders, or answers commits; every read and every applied batch goes to the real store,
  whose preconditions decide every `Conflict`. Nothing re-implements Create or Abort.
- **(c) Fixture includes the fault?** Yes. A′ forces the losing order on every run (asserted
  loss counts `{0..7}`); (C) installs a real colliding session/marker and real rival creates;
  (F) answers a real `CommitUnknownResult` in each variant, including the late landing; (B)
  really deletes/rewrites the marker between read and commit; the DST arms the 1021/1031
  nemesis and its sweeps assert that struck batches landed, did not land, landed late, and were
  rejected late.

## Verification posture

As the brief pre-declares: the functions do not exist on `main`, so C4-verify's reverted base
cannot compile the test and C4-verify will report **UNVERIFIABLE (exit 77)**. That is expected,
not a defect. The reds above stand in for it.

## Gates I ran

- **The new test**, via the command C4-verify uses:
  `cargo test -p wyrd-core --test multipart_create_abort` — `17 passed; 0 failed`, 5 runs out of
  5 (about 1.3 s each; no flakiness from the barrier or the real backoff sleeps).
- **Full gate**, `engine/xtask.sh ci` (= `cargo xtask ci`) on this worktree: typos ✓,
  `lint_docs.py` ✓, `render_site.py --check` ✓, gitlink / unsafe / blackbox guards ✓,
  `cargo fmt --check` ✓, `cargo clippy --workspace --exclude wyrd-dst --all-targets` ✓, build ✓,
  `cargo test --workspace --exclude wyrd-dst` ✓ (197 test binaries ok, 0 failed — including
  restore's **unmodified** `restore_open_fence` (11 passed) and `restore_completing_fence`
  (7 passed)), `cargo-machete` ✓, `cargo deny check` ✓, then:
- **✗ `cargo deny --all-features --config deny-all-features.toml check advisories`** — two
  RustSec advisories published against `rustls 0.21.12`: RUSTSEC-2026-0346 (`Debug` impls leak
  TLS secrets) and RUSTSEC-2026-0349 (ServerHello key-share downgrade). The chain is
  `wyrd-metadata-tikv → tikv-client 0.4.0 → tonic 0.10.2 → tokio-rustls 0.24.1 → rustls 0.21.12`.
  **Not caused by this patch:** `Cargo.lock`, `Cargo.toml` and `deny.toml` are unchanged against
  `7bf7eb9` (`git diff --quiet` confirms), so the base fails identically. The fix advised
  (`rustls >= 0.23.46`) is out of reach of `tikv-client 0.4`'s `tonic 0.10`; resolving it is a
  dependency or `deny.toml` decision for the human, outside this slice. **Expect C4-ci to be
  red on this step for every bundle until it is dealt with.**
- Because `ci` stops at the first failure, I ran the steps after it through the same wrapper:
  `engine/xtask.sh conformance` ✓, `engine/xtask.sh statics` ✓ (the ADR-0035 scan covers
  `crates/core` and `crates/dst`; the interposer and DST host keep state in fields only), and
  `engine/xtask.sh dst` ✓ — clippy under `--cfg madsim` ✓ and every DST file green at
  `MADSIM_TEST_NUM=50`, including the new `multipart_admission` (4 passed). The remaining `ci`
  step, the ADR-0010 deploy guard, has no subcommand of its own; it scans for orchestrator /
  Kubernetes imports, and this patch adds none.
- After that full run I added four test legs (Abort under an unknown outcome, Abort at the last
  epoch, Abort's round bound, the F fresh-read assertions), a content type on half of (A)'s
  creates, and rewrapped three comments. Those touch only the test file and comments; I re-ran
  `cargo fmt --all -- --check` ✓, `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` ✓,
  the test file ✓, and `typos` ✓.
- `cargo doc -p wyrd-core --no-deps` fails on base already (pre-existing broken links in
  `write.rs`); the gate does not run it. My files add no rustdoc warnings (checked by filtering
  its output to `crates/core/src/multipart*`).
- The patch applies cleanly to `7bf7eb9` (`git apply --cached --check` against a fresh index of
  the base tree).

## External dependencies

Both listed dependencies were present and ran: `typos` (typos-cli 1.48.0, clean) and the docs
renderer (`python3 -c "import markdown_it, yaml"` OK; `render_site.py --check` passed inside
`cargo xtask ci`). No unlisted dependency was needed.
