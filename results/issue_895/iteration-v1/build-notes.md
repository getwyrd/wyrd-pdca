# Build notes — issue 895 / multipart-abort-fence (attempt 1)

Target: `getwyrd/wyrd` `main`, built on the bundle's base `07631d3` (`pdca-integrate: issue_894`,
the stack base `pdca-integration/main-rc423c4501a6d`), which is `main` plus the sibling Create
patch (`f9514c4`). `patch.diff` is `git diff 07631d3` of the worktree; `git apply --check -R`
confirms base + patch equals the tree every result below was taken on.

The brief's `multipart.rs` citations were taken on `main @ 7bf7eb9`; the sibling's patch moved
them. On the base: `open_teardown` is `crates/core/src/multipart.rs:2536-2549` (brief `:2360`),
`decode_retire_obligation` `:3886` (brief `:3710`), `Refusal` `:4481` (brief `:4302`),
`AbortOutcome` `:4631` (brief `:4440`), `MAX_COMPLETE_ATTEMPTS` `:5194` (brief `:4975`). The
`restore.rs` citations hold on the base as written (`fence_session` `:1027-1092`, the batch
`:1045-1057`, the conflict branch `:1067-1083`). Lines below cite the **patched** tree unless
marked "base".

## What changed

1. **New `crates/core/src/multipart/abort.rs`** (the store round trip, beside the sibling's
   `create.rs`).
   - `ABORT_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1` (`abort.rs:58`), derivation in its doc
     (`:48-57`).
   - `fence_batch` (`abort.rs:77-93`): the one teardown fence batch builder. `require(mpu:<id> ==
     read)`, `require_absent` per obligation key in order, put the session, put each obligation
     in order. This is base `restore.rs:1050-1057` moved verbatim in shape.
   - `AbortError` / `InDoubt` / `Stop` (`abort.rs:105-159`), `Display` (`:161-204`),
     `Error::source` (`:206-222`: `InDoubt` → the `CommitUnknownResult`, so `classify` reads
     `Indeterminate`; `Stopped` → the underlying fault, so it keeps its class).
   - `abort_session` (`abort.rs:282-353`), `lost` (`:370-388`), `Doubt` (`:392-447`).
2. **`crates/core/src/multipart.rs`**: `pub mod abort;` (`:150`); module header names the new
   child module and says Abort now writes `mpu:` and `retire:bytes:` (`:8-10`, `:103-106`,
   `:114-118`); `RetireObligation` doc names Abort as an `open_teardown` writer and says why
   `fence_batch` only writes decodable obligations (`:3906-3911`); `AbortOutcome` doc cites the
   new function (`:4634-4635`). Doc only, plus the `mod` line.
3. **`crates/custodian/src/restore.rs`**: `use wyrd_core::multipart::abort::fence_batch;`
   (`:109`); `fence_session` calls `fence_batch(key, read, &fence.session, &fence.obligations)`
   (`:1054-1055`) in place of base `:1050-1057`; the pass doc says so (`:505-511`). `keys`
   (`:1049-1053`) stays: the committed and conflict branches still use it.
4. **`docs/design/architecture/05-building-block-view.md:206`**: the Abort sentences (the fence
   and its same-batch obligation, one builder for both writers, no count change, the refusals,
   the conflict classification, the in-doubt ending, no production caller until #668).
5. **Tests**: `crates/core/tests/multipart_abort.rs` (21 tests) and
   `crates/dst/tests/multipart_abort.rs` (6 tests, madsim). Both NEW.

No production caller of `abort_session` is added (Production reach (c)): `git grep abort_session
-- crates/*/src` finds only `abort.rs` and doc links.

## Why this shape

**Abort's loop** (`abort.rs:293-352`) answers every read the same way, the read after the last
commit included: a found record that is exactly a session this Abort's unknown-outcome fence puts
→ `Fenced` (`:295`); otherwise settle the doubt (`:298`) and answer from `abort_answer` — the
existing table function (`multipart.rs:4805`), so the table is not re-spelled. Only an `Open`
session reaches the round check (`:314`) and a commit. That is what makes "an unknown outcome on
the last round whose batch landed answers `Fenced`" hold without a special case.

**Conflict classification** (`lost`, `abort.rs:370-388`) mirrors restore's order (session, then
key; `restore.rs:1067-1083`) and adds the two things the brief's Design adds: an absent key is
`Unexplained` and spends a round (not `LostConflict`), and a present key triggers a second
session read before `Taken`.

**Doubt** (`Doubt`, `abort.rs:392-447`) follows the sibling Create's held/unsettled split
(`create.rs:537-567`) with one difference: `held` carries the session bytes its batch requires,
so "only a read that finds the session moved clears it" is a byte comparison (`settle`,
`:422-431`). `end` (`:435-446`) applies the brief's priority: held → `InDoubt(true)`; else
unsettled → `InDoubt(false)` (`Indeterminate`, names the id); else `Stopped`. No settling read
before returning (the brief's rejected alternative).

**`Fenced` vs `AlreadyAborting` after an unknown outcome.** A rival Abort of the same `Open@E`
writes byte-identical `Aborting@E+1` and obligation bytes, so after an unknown outcome this Abort
cannot tell its own landed fence from a rival's. It answers `Fenced` in that case. Both answers
are success (204), and `AbortOutcome::Fenced`'s contract ("the session is `Aborting` and its
teardown obligation is durable", `multipart.rs:4638-4642`) holds either way. With no unknown
outcome of its own, a found `Aborting` is always `AlreadyAborting` — which is what the race leg
pins.

**Round budget.** `MAX_COMPLETE_ATTEMPTS + 1 = 4`. The brief offered `MAX_COMPLETE_ATTEMPTS` as a
sizing source. Its meaning here: a session returns to `Open` only via a Complete-fence release, at
most `MAX_COMPLETE_ATTEMPTS` times, so the `+1` is the round that fences. It is a bound on work,
not correctness: running out returns a typed error that still carries any doubt. **Known limit
for the human:** there is no backoff between rounds (core stays runtime-free, ADR-0009, unlike
Create which takes a `CreateRuntime`). On TiKV, lock conflicts from concurrent part uploads on the
same session could spend all 4 rounds quickly; the Abort then ends `Stopped{RoundsSpent}` (or
`InDoubt` if a fence may still land), which is retryable by the S3 caller. If #668 finds that too
tight, adding a runtime with a wait is the fix; I did not add one because the brief's Design has
none and it would widen the public signature.

**Module placement.** The brief says "adds to the store module the sibling creates". I read that
as the sibling's child-module family (the header says round trips "live beside it, in child
modules", base `multipart.rs:7-8`), and added `abort.rs` beside `create.rs` rather than growing
`create.rs`. The builder cannot go into `multipart.rs` itself: that file's stated rule is "no
`WriteBatch`" (`multipart.rs:6-7`).

### Rejected: a runtime `decode_retire_obligation` check inside `fence_batch`

The brief: "It writes only obligations that `decode_retire_obligation` accepts at the key they go
under." I meet this by type, not by a runtime check: `RetireObligation` has no public
constructor; its only constructors are the teardown rows (`multipart.rs:3919-3969`), whose
key/payload pairs are pinned to decode by the existing unit tests (`multipart.rs:5479-5583`,
`mod open_teardown`) and, for the builder's output, by
`j_the_shared_builder_keeps_restores_preconditions_and_puts_in_order`
(`crates/core/tests/multipart_abort.rs:1171`) and the D-leg `assert_fenced`.

The runtime-check alternative, sketched:

```rust
pub fn fence_batch(..) -> Result<WriteBatch, RecordError> {          // +1 changed
    for (obligation, key) in obligations.iter().zip(&keys) {           // +4
        decode_retire_obligation(key, &metadata::encode(obligation.payload()))?;
    }
    ...
    Ok(batch)                                                          // +1 changed
}
// abort.rs: a new Stop variant + Display + source arms                // ~+8
// restore.rs fence_session: match the Err, name it on the audit seam  // ~+6
// restore.rs SessionUnsettled: a new public variant + Display arm     // ~+5
```

About +25 lines. Cost that matters more than the count: it changes restore's public
`SessionUnsettled` report surface, which the brief requires to stay behaviour-preserving (J), and
both new branches are unreachable by construction, so they would be untestable dead code against
the diff-coverage gate. The type guarantee restores the same property with neither.

### Rejected: a builder taking `OpenTeardown` / `CompletingTeardown` directly

Tighter pairing of session and obligations, but restore's `Fence` struct
(`restore.rs:1103-1107`) holds them separately, so this would restructure `plan_fence` too —
more than "the one behaviour-preserving change to `fence_session`" the Scope allows.

## Demonstrated reds (required by the brief)

Each is a temporary edit to `abort.rs`, the named run, then the original restored (sha256
`c6a6b92c…` before and after each; the final file differs only by a later doc-comment edit to
`ABORT_ROUNDS`). Runs: `cargo test -p wyrd-core --test multipart_abort` under `timeout`.

| Red edit | Result |
|---|---|
| 1. An `mpuctl` decrement added to the fence batch (`require` ledger bytes, put `count - 1`) | **(E) fails** in 11 legs ("(E) mpuctl unchanged", left 0 right 1), and the D batch-shape check fails |
| 2. Attempt 4's rule: an unchanged session after `Conflict` is taken as proof the key is taken, without reading it (`lost` returns `Taken` after the first read) | **H1 and H2 fail** (H1: `Stopped{ObligationKeyTaken}` instead of `Fenced`; H2: cause `ObligationKeyTaken`, not `RoundsSpent`), plus H4 and both last-round D legs |
| 3. A present obligation key named damage without re-reading the session | **H4 fails** alone (`Stopped{ObligationKeyTaken}` instead of `AlreadyAborting`) |
| 4. The held doubt replaced by the latest outcome (`observe` overwrites `held`) | **(I) fails**: `i_a_held_fence_survives_a_later_out_of_flight_unknown` (the error said `may_still_commit: false`) |
| 5. A failed re-read after `may_still_commit: false` returned as the read's own fault (`end` ignores `unsettled`) | **(I2) fails**: got `Stopped{Store}` instead of `InDoubt` |

Red 2 needed `#[allow(dead_code)]` on `Lost` for the temporary build (the `Unexplained` variant
became unconstructed under `-D warnings`); that was part of the temporary edit, not the patch.

**Whole-production revert:** with `pub mod abort;` removed, the test does not compile
(`error[E0432]: unresolved import wyrd_core::multipart::abort`). So C4-verify will report
UNVERIFIABLE (exit 77), as the brief's Verification posture pre-declares.

## Refuting my own test

- **(a) Genuine red?** Yes. The five targeted reds above each fail their named leg, and a full
  revert does not compile. Every red leg is forced by a wrapper, not the scheduler.
- **(b) Production path?** Yes. Every leg calls the real `abort_session` / `fence_batch` in
  `wyrd-core` over a real `RedbMetadataStore::in_memory()`. Sessions are made by the real
  `create_session`. The wrappers only change what the store *answers* (the TiKV-shaped
  `Ok(Conflict)` with nothing applied, held batches, read faults); the batch they hold is the one
  the production code built and is applied to the real store on release. The DST runs the same
  functions on `SimFdbMetadataStore`. Restore's change is exercised by restore's own unmodified
  tests (custodian unit/integration and the custodian DST file).
- **(c) Fixture includes the fault?** Yes. H1/H2 inject the unexplained `Conflict`; H3 plants the
  real obligation bytes `open_teardown` mints at the real key; H4 runs a real rival Abort between
  the two reads; (I) holds the real fence batch past the Abort's return and lands it afterwards;
  (I2) applies the fence and then fails every `mpu:` read. The DST arms the commit-ambiguity
  nemesis on every leg (asserted: `ambiguous_commits >= 1` at every seed).

## Gates run (results on the final tree unless noted)

The brief's inherited-deny ruling: `cargo xtask ci` stops at the all-features cargo-deny step, so
I ran its steps separately. `Cargo.lock` and `deny*.toml` are untouched.

| Command | Result |
|---|---|
| `cargo xtask statics` (via `engine/xtask.sh`) | pass — "no DST-reachable shared mutable global state" |
| `cargo xtask conformance` (via `engine/xtask.sh`) | pass — "5 valid + 6 invalid vectors pass" |
| `cargo xtask dst` (via `engine/xtask.sh`, `MADSIM_TEST_NUM=50`) | pass, exit 0 — madsim clippy clean; `custodian.rs` 33/33 (restore fence scenarios included, J); `multipart_abort.rs` 6/6; every other DST file green. One doc-comment edit to `abort.rs` landed after this build started; the new DST file was re-run on the final sources (6/6) |
| `cargo xtask ci-dry-run` (repo-hygiene guards) | pass |
| `cargo fmt --all -- --check` | clean (rustfmt touched only the three new files) |
| `cargo clippy --workspace --exclude wyrd-dst --all-targets` | clean, exit 0 |
| `cargo test -p wyrd-core` | all green; `multipart_abort` 21/21 |
| `cargo test -p wyrd-custodian` | all green (restore's tests unmodified, J) |
| `typos` on every changed file | clean |
| `python3 docs/publishing/tools/render_site.py --check` | "link audit OK" |
| `cargo doc -p wyrd-core -p wyrd-custodian --no-deps` | no warnings on any line this patch touches (the `-D warnings` run fails on pre-existing private links in `metadata.rs` / `erasure.rs`, not a gate) |

### DST sweep tallies (64 seeds per plain-`#[test]` sweep; deterministic per seed)

Measured with a temporary `eprintln!` (removed; the file is byte-identical to before):

| Leg | Aborts | Spent every round | Ended "may still land" | …and that fence landed by `quiesce` | Sessions raced by two Aborts |
|---|---|---|---|---|---|
| 1021 | 565 | 10 | 0 | 0 | 188 |
| 1031 | 576 | **2** | 2 | 1 | 192 |
| mixed | 576 | 10 | 10 | 0 | 192 |
| witness | 384 | 66 | 66 | 63 | — |

The 1031 leg's "some Aborts spend every round" check passes on 2 of 576: stable (seeded), but
thin. Without deferral, an in-flight 1031 fence lands on a coin at nearly every later store call
(the same effect the brief notes for attempt 4), so the witness leg is the one built to reach
that case reliably. If a later change to the simulator's RNG use drops the 1031 count to 0, that
assertion will fail as vacuous rather than pass silently.

The witness leg is my addition beside the brief's three: creates first (nemesis disarmed), then
one Abort per session with 1031 armed and in-flight landings deferred to `quiesce`. It gives the
demonstrated red for invariant (3): `a_1031_misreported_as_conflict_leaves_a_fence_no_abort_reported`
(`crates/dst/tests/multipart_abort.rs:626`) panics "not Open" when a struck 1031 is reported as
`Conflict`.

## Beyond the brief's legs

Three small core legs cover code paths the brief's criteria do not name:
`d_a_session_moved_to_a_later_open_epoch_is_fenced_there` (`:836`, the Design's "fenced again at
`E'`"), `d_an_open_session_at_the_last_epoch_is_a_typed_error_writing_nothing` (`:862`), and
`d_a_torn_session_record_is_a_typed_error_writing_nothing` (`:893`). The first plants `Open@2`
over a Create-made session, the same way the brief allows `Completing`/`Completed` to be planted.

## Housekeeping

- External dependencies `typos` and `docs-renderer` were both present; no NEEDS-HUMAN dependency.
- Left in the worktree, all git-ignored: `target/madsim/` (a separate `--cfg madsim` build dir I
  used so the normal and madsim builds would not invalidate each other) and `build/` (output of
  the render check). Scratch under `$PDCA_SCRATCH/pdca-builder-895-reds/` holds the red-edit
  backups and the dst/clippy logs. All for the harness to reclaim.
- Nothing pushed; no PR opened.
