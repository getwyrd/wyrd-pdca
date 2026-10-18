# Build notes — issue 656, iteration 3 (multipart-admission-create-abort)

Target: `getwyrd/wyrd` `main` @ `7bf7eb9`, worktree `$PDCA_WORKTREE`. Line numbers below are in
the patched tree (new files) or the patched copy of an existing file; `metadata.rs` citations
are unchanged from `main`.

This iteration starts from iteration 2's patch (`iteration-v2/patch.diff`, applied cleanly to
`7bf7eb9`) and fixes the two bugs the iteration-2 sign-off reproduced, the two doc wordings it
named, and one coverage weakness in the DST scenario that this iteration's changes exposed. The
shape, scope and slice size the sign-off accepted are unchanged: the store functions in
`crates/core/src/multipart/store.rs`, the shared `fence_batch`, the one-line restore change,
the DST scenario, and the docs.

## What changed this iteration

### Bug 1 — transition-size headroom (store.rs:349-365, multipart.rs:2352-2408)

**The bug.** Create admitted an `Open@0` record of any size the store accepted. Every later
transition puts the whole record, and each later state is wider: `Aborting` spells the state
4 bytes longer, `Completing` carries `publish_target`, which names the object a second time,
and every state may carry wider numbers. So a session admitted near the value ceiling could
never be fenced (client Abort or restore) or torn down on FoundationDB, and its admission slot
never came back.

**The fix.** Before Create reads or writes anything, it weighs the session in its widest later
form against the ceiling, and refuses it if it does not fit:

- `SessionRecord::widest_encoding` (`crates/core/src/multipart.rs:2368`, private to the
  protocol's writers like `SessionRecord::open`) builds the session in each state it can move
  to — `Open` (after a rolled-back Complete), `Aborting`, `Completing`, `Completed` — with
  every number at the widest value the decoder accepts: `epoch`, every stamp, `inode` and
  `version` at `u64::MAX`, `attempts` at `u32::MAX`, `segments_written` at the `seg:` grammar's
  bound `MAX_SEGMENT_INDEX + 1` (`crates/core/src/metadata.rs:508`, the decode check at
  `multipart.rs` `TryFrom<SessionRecordWire>`), the ETag part count at `MAX_PART_NUMBER`. It
  returns the longest `metadata::encode` output, so JSON escaping of the caller's strings is
  counted as stored.
- `create_session` (`store.rs:356-365`) passes that encoding to the existing shared ceiling
  check `metadata::flat_value_ceiling_crossed` (`crates/core/src/metadata.rs:605`, the same
  `>` boundary every other writer uses) and returns the new typed error
  `CreateFault::SessionTooLarge { bytes, ceiling }` (`store.rs:124-128`, `Display` at
  `:170-175`). One weighing covers every identity Create might mint, because ids and nonces
  are fixed-width (32 hex digits).

**Why the widest numbers, not the ones 0016 reaches today.** `attempts` is a live knob
(`multipart.rs` `SessionRecord::attempts` doc: enforced where a fence is admitted, never at
decode), so a "reachable" bound would have to track it, and an epoch bound would be one more
number to keep in step. The widest numbers cost about 60 bytes of the 100,000-byte ceiling in
the widest state (Completing: epoch 19 + attempts 9 + `fenced_at_millis` 7 + `segments_written`
4 + `publish_target.epoch` 19 bytes over realistic values). An S3 key is at most 1024 bytes, so
no real request comes near the bound; this is a backstop for caller input past a bound.

**Why a `CreateFault`, not a new `Refusal` variant.** `Refusal` is the landed protocol
vocabulary (`multipart.rs` §12) that #508 maps to S3 statuses, and 0016 names no protocol answer
for an oversize request. A new variant would change that table from this slice and touch the
other `Refusal` users (`crates/core/tests/multipart_state_machine.rs`, plus `store.rs`). The
caller (the gateway, #668) is expected to bound the key and header sizes first; reaching this
is caller input past a bound, so it sits next to `IdentitiesExhausted` as a typed error the
gateway can `downcast_ref` and map to a client error. If the human prefers a `Refusal` variant,
it is a one-variant change plus the gateway mapping.

**What I ruled out.**
- *Check only the `Open@0` record being written.* That is the narrower fix and it does not
  close the leak: `Aborting@1` is 4 bytes longer and `Completing` roughly one object-name
  longer. The new test shows it (red 1b below).
- *Count raw string lengths with a fixed overhead.* Off by every JSON escape: the test's object
  name encodes 7,000+ bytes wider than its raw length (`"`, `\`, `\u0001`), twice in
  `Completing`. Measuring the real encoding is exact by construction.
- *Bound the dirent key length too.* Complete's flip will key a dirent on the object name, and
  FDB's key limit is 10 KB. That is Complete's concern (#658), the plain PUT path has the same
  property, and it does not leak a slot (Abort still fits). Not in this slice.

### Bug 2 — ownership under an unknown outcome (store.rs:195-254, :515-522)

**The bug.** `owns()` compared only nonce, parent and object. If the identity source repeated
an existing upload's id and nonce for the same object, and the first commit answered
`CommitUnknownResult` without landing (it would have lost to the existing `mpu:<id>`), the
recovery found the existing session, `owns()` said yes, and the create answered `Created` with
the other upload's id and no count increment.

**The fix — two parts.**
1. **Read both keys before an identity's first batch** (`Identity::vacant`, `store.rs:225-229`;
   called once per identity at `store.rs:515-522`, tracked by `Identity::checked`,
   `store.rs:203`). A key found taken is a collision found without a commit: re-mint, counted
   against `MAX_UPLOAD_ID_ATTEMPTS` like any collision, never against the contention budget.
   This is the part that closes the reported case: the session that predates the create is
   seen while nothing is in doubt, so `owns()` is never asked about it. It is placed after the
   bucket / profile / cap checks, so those refusals cost the same reads as before.
2. **`owns()` compares every field Create wrote that no transition rewrites**
   (`store.rs:245-254`): nonce, parent, object, `content_type`, `created_at_millis`,
   `clock_source`. Transitions change only `epoch`, `attempts` and `state`
   (`open_teardown` / `completing_teardown`, `..self.clone()`), so a session that landed and
   was then fenced by the reaper or restore is still recognised as this create's own.

**Why both.** Part 2 alone does not fix the reported case: the regression test uses the very
same request (same stamp and clock), so every field matches and only the read in part 1 tells
the two uploads apart. Part 1 alone leaves a window: a *concurrent* create handed the same
identity could land between the read and an in-doubt commit; part 2 narrows that to a request
equal in every field. What is left — a concurrent create with the same 256-bit identity, the
same request to the millisecond, and an unknown outcome — writes byte-identical bytes, so no
read of the store can tell them apart. It is documented in `owns()`'s doc (`store.rs:242-244`).

**Cost of part 1, shown.** Two extra point reads per identity (normally one identity per
create), no extra commit. Under (A′)'s 8-creator barrier that is 16 extra `get`s against 36
commits. I did not reject a cheaper alternative here; the sign-off offered the read or the
wider comparison, and only the read closes the reported case.

### Doc wordings (multipart.rs:5136-5141, 05-building-block-view.md:202)

- `ADMISSION_BACKOFF_CAP_MILLIS` (`multipart.rs:5136-5141`): the bound is now
  `max_sessions ×` the cap, 2.3 s at the shipped profile's 46, and longer under a larger
  profile.
- Architecture doc (`docs/design/architecture/05-building-block-view.md:202`): a contention
  refusal means that, that many times over, either the ledger moved or the store could not
  report an outcome — not only a full ledger. The paragraph also now states the size refusal,
  the read before an identity's first commit, and the every-field ownership rule (docs
  currency for the new `CreateFault` variant and the changed behaviour).
- `create_session`'s own doc (`store.rs:272-347`) covers the size check, the vacancy read,
  and the ownership rule; the module header is unchanged.

### DST scenario — reaching the exhaustion path by design (multipart_admission.rs:61-76, :155-159)

After the store change, `the_1021_sweep_sees_struck_batches_land_and_not_land` failed at its
coverage check "no Abort reached the end of its round budget across the sweep"; the invariant
checks all held. Cause: every sim `get` takes a seeded network hop and may settle in-flight
batches on a seeded coin (`crates/dst/tests/support/mod.rs:836-841`), so two extra reads per
create reshuffle every struck batch's fate. The check only held by luck before: every Abort
in the scenario raced a second Abort on the same session, and that pair exhausts only when
roughly 8 struck fences in a row all miss (about 1 in 256 per session under 1021, so about 0.75
expected over 64 seeds × 3 sessions).

Change: creator `n` now aborts twice at once when `n % 3 == 0` (racing Aborts kept), once
when `n % 3 == 1` (a lone Abort exhausts when all 4 of its struck fences miss, 1 in 16 under
1021), and not at all when `n % 3 == 2` (Open sessions kept beside fenced ones). Measured with
a temporary print (removed): 1021 sweep `landed=634 not_landed=596 exhausted=6`; 1031 sweep
`in_flight=319 late=271 rejected=48 exhausted=4`. All four entry points pass at
`MADSIM_TEST_NUM=50`.

### Test changes (crates/core/tests/multipart_create_abort.rs — 29 tests, was 26)

- **New** `b_a_session_too_large_for_a_later_state_is_refused_and_writes_nothing` (`:1240`) —
  the boundary test the sign-off asked for, through a new size-enforcing wrapper
  `Plan::ValueCeiling` (`:361`, `:795-804`: refuses any put value over `MAX_VALUE_BYTES` with a
  plain fault, as FDB's `value_too_large` does). The interposer also counts point reads now
  (`:471`, `:753`), so the refusal is shown to happen before any read as well as any commit.
  The oracle is the test's own:
  `widest_later_states` (`:1151`) spells each state's widest JSON from the stored bytes, the real
  decoder accepts each one, and the test checks that one more segment or one more ETag part is
  refused, so it really is the widest. The object name is built from characters that encode
  wider than they are long, and the content type sets the size to the byte. Legs: exactly at the
  ceiling is admitted, then fenced by the client Abort and (a second one, moved to
  `Completing@1`) by the restore teardown through `fence_batch`, all through the enforcing store;
  one byte over is refused with `bytes == MAX_VALUE_BYTES + 1`, no commit attempted, store
  byte-identical, even though its `Open@0` and `Aborting@1` would both fit; and the sign-off's
  exact case — `Open@0` exactly at the ceiling — is shown to leak (its real Abort is refused by
  the enforcing store) and is refused by Create.
- **New** `c_an_identity_repeated_under_an_unknown_outcome_is_never_the_other_upload`
  (`:1658`) — the collision-plus-unknown-outcome regression: same id and nonce for the same
  request, same id with another nonce, same nonce under another id; each with
  `may_still_commit` true and false. Each is admitted under the re-minted identity, the other
  upload untouched, one count each, and the unknown commit is the second identity's.
- **New** `f_an_in_doubt_re_drive_that_loses_the_ledger_is_retried_not_taken_as_landed`
  (`:2408`) — while a batch is in doubt, the re-drive loses only the ledger CAS to a rival; the
  create must back off and re-drive again, not answer `Created` for a session that is not there.
  Added because the first mutation run on this iteration's patch found `&&` → `||` in `lost()`
  (`store.rs:597-598`, cargo-mutants' `store.rs:598:9`) surviving: the old killer of that mutant
  (`c_an_identity_repeated_for_the_same_object_…`) no longer reaches `lost()`, since the
  vacancy read now finds that collision first.
- **Extended** `f_in_doubt_a_session_differing_in_one_field_is_a_collision_not_this_create`
  (`:2643`) — `Squat` gains `ContentType`, `CreatedAt`, `ClockSource` (`:426`, `squat()`
  `:588`), and each squat runs with the request's content type absent and declared.
- **Reworked** `c_a_collision_does_not_spend_the_contention_budget` (`:1561`) — the orphan
  marker that used to be found by a lost commit is now found by the vacancy read, so the test
  runs both ways: pre-existing (found by the read, no commit spent) and planted just before
  the first commit by the new `Script::plant_marker` (`:387`, `:806-811`) so the lost-commit
  path stays covered. In both, the create is admitted on its last permitted attempt.

## Red → green evidence

Command (what C4-verify runs): `cargo test -p wyrd-core --test multipart_create_abort`, under
`timeout`. Green with the fix: `test result: ok. 29 passed; 0 failed` (0.54 s). Each red below
was a temporary edit to the fixed tree, then restored and re-checked green. (Reds 1-6 ran
before the 29th test was added, so their pass counts are out of 28.)

1. **Size check disabled** (`flat_value_ceiling_crossed(&widest).filter(|_| false)`): 
   `b_a_session_too_large_for_a_later_state_is_refused_and_writes_nothing` FAILED —
   `too large for a later state: Created { upload_id: UploadId("00000000000000080000000000000001") }`
   (27 passed, 1 failed).
   **1b. Check weighs only the `Open@0` record written** (`metadata::encode(&SessionRecord::open(..))`
   in place of `widest_encoding()`): same test, same message — the narrower fix admits the
   one-byte-over request.
2. **Vacancy read disabled** (`if false && !identity.vacant(store).await?`):
   `c_an_identity_repeated_under_an_unknown_outcome_is_never_the_other_upload` FAILED —
   `same id and nonce, may_still_commit = true: never the other upload's id`,
   `left: UploadId("…a1")` (the existing upload), `right: UploadId("…a2")`; and
   `c_a_collision_does_not_spend_the_contention_budget` FAILED (`found by the read`: the first
   commit lost to the collision). 26 passed, 2 failed.
3. **`owns()` back to three fields**: `f_in_doubt_a_session_differing_in_one_field_is_a_collision_not_this_create`
   FAILED — `a collision while in doubt: Created { upload_id: UploadId("00000000000000010000000000000001") }`
   (27 passed, 1 failed).

The brief's three required reds, re-run on this iteration's tree:

4. **(A′) shared budget** (contention budget `= MAX_UPLOAD_ID_ATTEMPTS`):
   `a_prime_every_creator_is_admitted_through_more_lost_cas_than_the_collision_budget` FAILED —
   `the create was refused: SlowDown { pressure: AdmissionContention { attempts: 2 } }`.
5. **(E) decrement on Abort** (the fence batch also `require`s `mpuctl` and puts it at
   `count - 1`): five Abort tests FAILED on their (E) assertions, e.g.
   `d_abort_fences_an_open_session_with_its_obligation_in_one_batch`:
   `assertion left == right failed: (E) the fence never moves the count; left: 0, right: 1`;
   also `d_two_concurrent_aborts_…` (`(E) the count never moves`), `d_an_unknown_fence_outcome_…`
   (`applied: (E)`), `d_a_fence_landing_under_the_last_rounds_…` and
   `d_a_rival_abort_winning_the_last_round_…` (`(E)`).
6. **(F) re-mint on an unknown outcome**: `f_an_unknown_create_outcome_is_recovered_under_the_original_upload_id`
   FAILED — `F1 applied: exactly one session for this create: [UploadId("…10000000000000001"),
   UploadId("…10000000000000003")]; left: 2, right: 1`.

7. **The surviving mutant, by hand** (`if in_doubt || found…` in `lost()`):
   `f_an_in_doubt_re_drive_that_loses_the_ledger_is_retried_not_taken_as_landed` FAILED —
   `may_still_commit = true: answered Created, so mpu:00000000000000010000000000000001 must exist`.

**DST red** (same re-mint edit, `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst
--test multipart_admission`, the flags `cargo xtask dst` uses): all four entry points FAILED —
e.g. `creator 3 was refused: SlowDown { pressure: SessionCap { count: 46, max_sessions: 46 } }`:
sessions stranded by re-minting fill the shipped ledger with only 6 creators. Restored: 4/4 pass.

## Refuting my own tests

- **(a) Genuine red?** Yes. Each fix, reverted on its own, turns its test red (reds 1, 1b, 2, 3
  above); the brief's three required reds (4-6), the hand-applied mutant (7) and the DST red
  still fire. The whole patch
  cannot be reverted with the new test file in place: the file names `CreateFault::SessionTooLarge`
  and `create_session`, which do not exist on `main` — the pre-declared C4-verify UNVERIFIABLE.
- **(b) Production path?** Yes. Every leg calls the production `create_session`,
  `abort_session` and `fence_batch` (and `open_teardown` / `completing_teardown`) over a real
  `RedbMetadataStore`; the interposer only orders, answers or refuses commits and forwards every
  read and applied batch to the real store, whose preconditions decide every `Conflict`. The size
  oracle does not call `widest_encoding`; it spells the states itself and checks them with the
  real decoder.
- **(c) Fixture includes the fault?** Yes. The size test's store enforces the ceiling the bug is
  about (and the test shows a real Abort refused by it for the sign-off's exact case); the
  ownership regression puts the existing upload in the store and forces the unapplied unknown
  outcome on the very commit the bug needs; the squat legs plant the differing session after the
  vacancy read, inside the in-doubt window; the DST scenario arms the commit-ambiguity nemesis
  on every commit.

## Carry-forwards and decisions kept from earlier iterations

- **Bucket deletion fence → #511.** Create `require`s the bucket marker's exact bytes; ADR-0046
  §4's other half (`require_absent` on the deletion fence, decision 5) is deferred to #511, which
  must define the key. The single place it joins is marked in code:
  `crates/core/src/multipart/store.rs` `create_batch`, `// deferred: #511`. Nothing on `main`
  writes a fence key, and no deployment can create a session before #668.
- **"Metadata awaits lack a timeout"** — rejected by the human at the iteration-1 sign-off and
  recorded in `review-rejected.md` (core is runtime-free; the caller, #668, owns timeouts). Both
  loops are bounded (contention budget = the profile's `max_sessions`, `MAX_UPLOAD_ID_ATTEMPTS`,
  `ABORT_ROUNDS`).
- **cargo-deny advisories (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client)** — inherited
  from `main`; the sign-off said not to touch `Cargo.lock` or `deny.toml` here, and I did not.
- **Open question 2 (constructors).** No decoder was loosened. `SessionRecord::open` and
  `AdmissionRecord::bootstrap` / `admit` (iteration 1) still round-trip through their decoders;
  `widest_encoding` builds values only to measure them and never writes one.

## Gates I ran

- **The new test file**: `cargo test -p wyrd-core --test multipart_create_abort` —
  `29 passed; 0 failed`, about 0.5 s.
- **Full gate**, `./engine/xtask.sh ci` (= `cargo xtask ci`) on this worktree: `typos` ✓,
  `lint_docs.py` ✓, `render_site.py --check` ✓, gitlink / unsafe / blackbox guards ✓,
  `cargo fmt --check` ✓, `cargo clippy --workspace --exclude wyrd-dst --all-targets` ✓, build ✓,
  `cargo test --workspace --exclude wyrd-dst` ✓ (197 `test result: ok`, no failure),
  `cargo-machete` ✓, `cargo deny check` ✓, then
  **✗ `cargo deny --all-features --config deny-all-features.toml check advisories`** —
  RUSTSEC-2026-0346 / RUSTSEC-2026-0349 on rustls 0.21.12 via tonic 0.10.2 → tikv-client 0.4.0.
  Inherited from `main`, and the sign-off said to leave `Cargo.lock` / `deny.toml` alone, so
  the gate stops there exactly as in iteration 2. The steps after it I ran one by one:
  `./engine/xtask.sh statics` ✓ (no DST-reachable shared mutable global state; the test's
  interposer keeps its state in struct fields), `./engine/xtask.sh conformance` ✓ (5 valid +
  6 invalid vectors), and `./engine/xtask.sh dst` ✓ — madsim clippy ✓, then every DST test file
  green at `MADSIM_TEST_NUM=50` (11 `test result: ok`, none failed; 1233 s of it the existing
  33-test file). That DST build was compiling while I briefly applied the `lost()` mutant by
  hand (red 7), so it may have built that version; only `multipart_admission` calls this code,
  so I re-ran that file on the final tree: 4/4 ✓.
- I edited the architecture doc once more after the gate run (two wording fixes), so I re-ran
  its checks on the final text: `lint_docs.py` ✓, `render_site.py --check` ✓ (99 pages, link
  audit OK), `typos` ✓.
- **Mutation check** (the C5 row's own script, `scripts/mutants-in-diff`, over this
  `patch.diff`): first run `73 mutants tested in 4m: 1 missed, 61 caught, 11 unviable` — the
  miss was `&&` → `||` in `lost()` (fixed by the new test above); final run
  `73 mutants tested in 4m: 62 caught, 11 unviable`, none missed.
- `cargo fmt --all -- --check` and `typos` are clean on every touched file after the last edit;
  `cargo clippy -p wyrd-core --all-targets` ✓ after the last test edit. The repo has no commit
  hooks installed (`core.hooksPath` unset, only `.sample` files in the hooks dir, no
  pre-commit / husky / lefthook config), so `cargo xtask ci`'s fmt / clippy / typos steps are
  the commit-time checks.
- `cargo doc -p wyrd-core --no-deps` fails on 20 rustdoc errors, none in a file this patch
  touches (`erasure.rs` 1, `metadata.rs` 14, `read.rs` 2, `write.rs` 3 — unchanged from
  `main`); rustdoc is not a `cargo xtask ci` step. The one new link from a public item to a
  private one that I wrote (`create_session` → `Identity::owns`) I removed.

## For sign-off

- **C4-verify will report UNVERIFIABLE** (exit 77), as the brief pre-declares: the new test file
  cannot compile on `main`. The reds above are the per-fix evidence.
- No external dependency was missing: `typos` (1.48.0) and the docs renderer (python
  `markdown_it`, `yaml`) are installed, and nothing else was needed.
- No production caller of Create or Abort was added; the only production path changed is still
  restore's fence, which calls `fence_batch` with the same preconditions and puts (unchanged
  from iteration 2; the custodian suite passes unmodified).
- One case is left that no record can settle, documented in `owns()`: a concurrent create
  handed the very same 256-bit identity for a request equal in every field, plus an unknown
  outcome. Both would write byte-identical records.
