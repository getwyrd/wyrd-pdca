# Build notes — issue 1010 (636.5d), iteration 4: a torn re-read settles nothing

Base: the stacked integration tip `db1a066` (`stack-base-tip`), which carries child-3 (#1009).
`patch.diff` is `git diff HEAD` of the worktree: iteration 3's patch, applied unchanged, plus the
changes below. Every `path:line` is on the patched worktree.

## What the carry-forward asked, and what changed

| Carry-forward item | Change |
|---|---|
| Fence phase: a re-read that does not decode must not settle the doubt (`settle` ran before the decode) | `crates/core/src/multipart/complete.rs:890-897`: the decode now comes first, and `doubt.settle(&read)` runs only on a record that decoded. A torn record goes to `stop(&doubt, Stop::Record(..))`, which is the in-doubt error while a doubt stands |
| Publication phase: the torn read sent to `moved()` | `complete.rs:1551-1559`: `moved(..)`'s error goes through `doubt.end(..)`, so it is the in-doubt error while a doubt stands |
| Regression `[write_after(Unknown{applied: true, may_still_commit: false}, put(mpu, b"not a record"))]` → `InDoubt` | `g3_torn_a_session_that_does_not_decode_after_an_unknown_fence_is_in_doubt`, `crates/core/tests/multipart_complete.rs:6478-6529`, plus an unapplied and a held case |
| Regression `[Pass, write_after(Unknown{applied: true, ..}, torn)]` → `InDoubt`, not `Stopped{Gone}` | `g3_torn_a_session_that_does_not_decode_after_an_unknown_flip_is_in_doubt`, `multipart_complete.rs:6531-6598`, plus an unapplied and a held case |
| `~:945` `[Hold, Fail{transient: true}]` → `InDoubt{Fence{1}, true}`, 2 commits, `release_held()` commits | `g6_held_a_fence_sent_again_and_refused_for_good_ends_in_doubt`, `multipart_complete.rs:6168-6207` |
| `~:1562` and `~:1577`: a failed bucket-marker or dirent read after a held flip → `InDoubt{Flip{1}, true}`, no release; then `clear_read_faults` + `release_held` lands, `assert_published_once` | `g2_unread_a_read_that_fails_while_a_flip_is_held_sends_no_release`, `multipart_complete.rs:6692-6753`, one loop over both reads |
| `~:1468`: the flip build fails after a held batch (prior at `u64::MAX`), "if it can be set up" | It can: `g4_unbuilt_a_flip_that_cannot_be_built_while_a_segment_batch_is_held_sends_no_release`, `multipart_complete.rs:7054-7120` |

Docs, to keep them true:

* module doc, *Unknown outcomes*, `complete.rs:192-196`: a record that does not decode settles
  neither kind of doubt;
* `Doubt::settle`, `complete.rs:1872-1893` (see below);
* `docs/design/architecture/05-building-block-view.md:252` (fence phase: a failed or undecodable
  read after an unknown outcome settles nothing) and `:254` (publication: the same sentence now
  names an undecodable record next to a failed read);
* the test file's module doc, `multipart_complete.rs:121-124`, `:132-133`, `:137`, `:146-147`.

Test helpers: `bound_at_last_version` (`multipart_complete.rs:4111-4125`) is the "target at the
last version" fixture lifted out of `a_damaged_part_or_target_behind_the_fence_stops_after_one_release`
(now `:4187`) so the new G4 leg can share it. `TORN` and `session_torn` (`:6471-6476`).

## Choices, and what I ruled out

**Fence phase: move `settle` after the decode.** The smallest change that makes the rule true:
`settle` sees only records that decoded. I did not add a separate check before `settle`
(`if decode fails { return doubt.in_doubt }`): the existing `stop` closure already returns the
in-doubt error while a doubt stands (`complete.rs:880`), so ordering is all that was wrong.

**`Doubt::settle` now takes `&Bytes`, not `Option<&Bytes>`.** Once the decode comes first, no
caller passes `None` any more: the fence phase returns `NoSuchUpload` on an absent record before
reaching `settle` (it did so before too, right after a `settle(None)` whose effect nothing read),
and the publication phase always passed `Some(&own.bytes)`. Leaving the `(None, None)` arm would
have been dead code with a doc that described a case that cannot happen. The new signature is
3 lines shorter and says what is true. Behaviour is the same for every input a caller can pass.

**Publication phase: `.map_err(|stop| doubt.end(..))` on `moved`.** With `why =
Rejection(OperationAborted)`, `moved` (`complete.rs:1321-1345`) returns `Err` only for a record
that does not decode; every other branch is `Ok`. So the `map_err` changes exactly the torn case,
and it applies the module doc's own rule ("while a doubt stands, every stop is the in-doubt
error") through the existing helper. Alternatives I rejected:

* decode the read again in `publish_held` before calling `moved` (`if session.is_some_and(|b|
  decode_session_record(b).is_err()) { if let Some(d) = doubt.in_doubt(..) { return Err(d) } }`):
  about 6 lines, decodes twice, and repeats a check `moved` already makes;
* pass the doubt into `moved`: `moved` has three callers (`complete.rs:1287`, `:1527`, `:1557`),
  and the other two never hold a doubt (`:1517` returned the in-doubt error before `:1527`, and
  `release` has none), so that is a signature change on all three for one caller.

**What a torn record means for a held doubt.** Before the fix, a torn read cleared a held doubt
(the bytes differ from the pinned ones). That is wrong in the direction the carry-forward named:
"can no longer land" is not "did not land". The held batch may have landed before the record was
damaged. So the held case now ends in doubt too, and both new torn legs include it.

## Red → green evidence

All runs: `timeout 1500 cargo test -p wyrd-core --test multipart_complete` in the worktree (each
leg also has the file's own 120 s `LEG_TIMEOUT`). Logs and the mutant script are under
`$PDCA_SCRATCH/pdca-builder-1010-redleg4/`.

* **Green:** 85 passed (80 from iteration 3 plus the 5 new legs).
* **Red, fence fix reverted** (`no_fence_fix`: `settle` before the decode, iteration 3's order):
  1 fail, `g3_torn_..._unknown_fence_is_in_doubt`, with exactly the adversary's probe result:
  `Stopped { fence: NotPlaced, cause: Record(MalformedRecordValue { namespace: "mpu:", .. }) }`.
* **Red, publication fix reverted** (`no_pub_fix`: no `map_err`): 1 fail,
  `g3_torn_..._unknown_flip_is_in_doubt`, with `Stopped { fence: Gone { epoch: 1 }, cause:
  Record(..) }`, the adversary's second probe.
* **Red, the four "stay in doubt" mutants the adversary listed** (each branch reverted to its
  pre-patch form, a release or `not_placed`):
  * `m945` (`not_placed` for a fence refused for good): 1 fail, `g6_held_a_fence_sent_again_..`,
    `Stopped { fence: NotPlaced, cause: Store(TransientFault ..) }`;
  * `m1468` (release when the flip cannot be built): 1 fail, `g4_unbuilt_..`, `Stopped { fence:
    Released { epoch: 1 }, cause: Publication(VersionOverflow { inode: 900 }) }`;
  * `m1562` (release on a failed bucket-marker read): 1 fail, `g2_unread_..` (bucket case),
    `Stopped { fence: Released { epoch: 1 }, cause: Store(..) }`;
  * `m1577` (release on a failed target read): 1 fail, `g2_unread_..` (dirent case), same shape.

  Before this iteration, all four survived the 80 legs (the adversary's finding 1).
* **cargo-mutants** (`scripts/mutants-in-diff` with this bundle, the C5 gate's own command):
  27 mutants, 21 caught, 6 unviable, 0 missed, 0 timeouts, rc 0. The 6 unviable are
  `Default::default()` stubs for types with no `Default`, as in iterations 2 and 3. The `settle`
  mutants (`-> ()`, `==`→`!=`, `delete !`) are all caught.

## Refuting my own test

* **(a) Genuine red?** Yes. Reverting each fix on its own fails its torn leg with the exact bad
  outcome the adversary reported (`Stopped{NotPlaced, Record}` and `Stopped{Gone, Record}`).
  Each of the four hand mutants fails the leg aimed at it, and no other leg.
* **(b) Production path?** Yes. The fence-phase legs call the production `fence_complete`, the
  others `complete_upload`, on a real `RedbMetadataStore` through the logging wrapper; the flat
  and wide fixtures stage their parts through the real `upload_part`. The wrapper only decides
  commit outcomes, fails reads, and writes other writers' records on the real store.
* **(c) Fixture includes the fault?** Yes. The torn record is really written to `mpu:<id>` on the
  store, after the commit and before the request's re-read (`OnCommit::WriteAfter`, which asserts
  the write landed), and the legs check afterwards that it is still there. In the applied cases
  the fence or flip really landed first (the flip case checks the dirent is bound to inode 500).
  In the held legs the held batch is the real batch, and `release_held` shows it can still land
  (`Committed`) where the leg says it may, or cannot (`Conflict`) where its required bytes are
  gone. In `g4_unbuilt` the target really is bound to an inode at `u64::MAX`, and the held
  segment batch really lands afterwards, writing every `seg:` record once.

## Rubric self-review

* One clock per lifecycle: no clock read added or moved.
* Narrow seams: only `MetadataStore::get`/`commit`, as before.
* Metadata validation boundaries (ADR-0045): a session record that fails decode stays an error,
  never a value; the change only stops that error from being taken as evidence about an unknown
  commit.
* Absent/unsupported entries: an undecodable record now always ends in an explicit error (the
  in-doubt error while a doubt stands, the stopped error otherwise), never an answer.
* Transactions: `CommitUnknownResult` is still never read as `Conflict`, and no new early return
  skips a release this Complete owes (the doubt paths send none on purpose; module doc).
* Test fidelity: the new legs drive the production verb against the real redb adapter. Tier-0
  DST stays deferred to #660 (`complete.rs:792-799`).
* Docs currency: module doc, `Doubt::settle`'s doc and the building-block view say the new rule.
  No port, API, RPC, CLI flag or persisted field changes.

## Commit-readiness

`cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-core --all-targets` clean (the
workspace denies warnings); `typos` clean on all four changed files. Full gate: see *Full gate*
below. `patch.diff` is byte-identical to `git diff HEAD` of the worktree and reverse-applies
cleanly.

## Full gate

`PDCA_WORKTREE=<this worktree> ./engine/xtask.sh ci` ends `xtask ci: all checks passed`, rc 0
(fmt, clippy, build, tests including DST, deny, conformance, statics), with `multipart_complete`
at 85 passed inside it (`ci.log:481-570`). After that run I shortened two section-header
comments in the test file; fmt, `typos` and a final run of the file (85 passed) were repeated on
that final state, which is what `patch.diff` holds.

## External dependencies

`typos` (the brief's only one) is installed and passes. Nothing undeclared was needed.

## Size

The size backstop was overridden at sign-off; no split. Against iteration 3, `complete.rs` gains
3 lines (the reorder, the `map_err`, the narrowed `settle` and their comments); the
building-block view changes two sentences in place; the test file grows by 320 lines
(6800 → 7120), all of it the five legs and helpers the carry-forward asked for, plus a 12-line
helper shared with one existing leg (which loses 11 lines).

## Scratch

Logs, the mutant script, the saved fixed copy and an iteration-3 tree for the interdiff are under
`$PDCA_SCRATCH/pdca-builder-1010-redleg4/`. `cargo mutants` left `mutants.out/` and
`mutants.out.old/` (git-ignored) in the worktree. I left removal of both to the harness.
