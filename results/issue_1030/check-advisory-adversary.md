# Adversarial review — issue 1030 (657.2d), iteration 4

**Verdict: I could not refute the fix.** I re-ran the red→green myself, planted eleven bugs, and
tried the histories most likely to break the new compensation settle and the own-commit proof's
new part-key read. Every planted bug was caught. One doc sentence overstates what a cause means
(damage-only, not a wrong answer). One gate row is not the evidence it claims to be.

## The evidence

- **Red→green is real and runs the production path.** I copied the patched tree to scratch and
  ran `cargo test -p wyrd-core --test multipart_upload_part_commit`: 92 passed. With
  `crates/core/src/multipart/commit.rs` put back to the base (`git show HEAD:…`) and the patched
  test file kept, 21 of 92 fail. The tests call the public `commit_part` over a real redb store
  through the scripted wrapper, so the code under test is the production code, not a copy of it.
  Of the 21, `b_a_landed_commit_answered_unknown_is_committed`, `b_read_gap_landed_then_*`,
  `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` and
  `r_a_missing_session_behind_a_held_commit_is_in_doubt` fail only because the lap's read trace
  gained the new part read, which (K) allows. The rest fail on outcome, cause or flag. One small
  error in the brief, and it is harmless: its *Falsifiability* says (T)'s first history is green
  on the base, but `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` is red
  there. The base keeps the store's `true` flag, and the test asserts `false`, so the test is
  stricter than the brief, not weaker. C4-verify's `PASS (green-only)` was pre-declared in the
  brief, and the red above fills that gap.
- **Eleven planted bugs, all killed** (each applied alone to `commit.rs`, then the suite run):
  - the new part read's record no longer lowers the flag (`commit.rs:1046`): `k_the_own_commit_proofs_part_read_keeps_the_coverage_of_the_reads_before_it` fails;
  - torn bytes at that read taken as a record (`commit.rs:1050`): three D1/K/R tests fail;
  - the part read made in every lap, not only when the slot is gone (`commit.rs:1035`): ten tests fail;
  - the compensation settle never lowers the flag on a moved session (`commit.rs:1292`): three T/F tests fail;
  - an empty part settles on "slot gone" under any session, Open(E′)/Left included (`commit.rs:1298`): the (T) empty-part test fails;
  - other bytes at the obligation key read as absent (`commit.rs:957`), or any bytes there read as its own (`commit.rs:956`): the (O) test fails in both cases;
  - a rewritten slot under a compensation answered `OutcomeUnknown` (`commit.rs:1304`): the (F) test fails;
  - the last obligation read dropped (`commit.rs:1314-1318`): four tests fail;
  - a missing session record lowers the flag with no slot read after it (`commit.rs:1301-1303`): two tests fail;
  - the terminal delete keeps the flag (`commit.rs:978-981`): two tests fail.

  C5 agrees: 0 missed. Its 3 timeouts (`commit.rs:1279`, `:1308`) break the settle loop's
  termination, so they count as caught.

## The fix: refutation attempts that failed

- **Obligation bytes as the witness (`commit.rs:947-958`).** I checked whether a legal writer
  could leave *other* bytes at a landed compensation's key. If it could, the code would wrongly
  answer `ObligationKeyTaken` with the flag kept. 0016's drain marks fragments idempotently
  ("never re-stamp") and deletes the obligation only when it is fully drained
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2220-2222`). It never rewrites
  the obligation in place, and the drain is not yet in the tree. Not refuted.
- **Key collisions.** The re-upload commit uses the same key form (`commit.rs:1551`,
  `obligation_for` at `:1171-1187`), but every route into `compensate` (`:1541`, `:1580`,
  `:1586`, `:1591`, `:1636`, `:1650`, `:1676`) comes before any commit, or after commits
  answered `Conflict`. So that commit cannot land. The epoch chase recomputes the obligation under
  the chased epoch before each round (`:1342`), and `settle_compensation` gets that same object
  (`:1354`). Not refuted.
- **Empty-part proof (`commit.rs:1298-1300`).** I tried a teardown, an Abort, a rollback, and a
  rival reserving the freed index. The first three move the session bytes, so they never reach
  `Same`. The rival is a legal release. The only way to slot-gone under `Same` without a landing
  is damage, which is the documented limit at `commit.rs:248-251` in the module doc. Not refuted.
- **Own-commit part read timing (`commit.rs:1035-1052`).** A commit landing between the slot read
  and the new part read cannot happen without late staging rebuilding the slot first, and then
  the commit really did land, so `Committed` is true. The rival-record limit is documented and was
  settled at the iteration-3 sign-off. Not refuted.

## Findings

- `crates/core/src/multipart/commit.rs:531-533` (also `:150` and the comment at `:1038-1039`):
  the new `Stop::SlotLost` text says a slot gone under `Open@E`, with the part key still absent,
  means "no commit of this attempt landed". **Concrete case** (probed in scratch): the commit is
  scripted `Act::unknown(true, true)` (applied), and the part key is erased at gap 0 by damage.
  The answer is `InDoubt { batch: Commit, may_still_commit: true, cause: SlotLost }`, yet
  `rig.releases(&attempt) == 1`: the commit *did* land, and damage took the part record, not the
  slot. The answer itself is safe: it is in doubt, claims no release, and is never a false
  `Committed`. Only damage reaches it. But the public doc claims more than the read shows. A
  wording fix would be "no landed commit of this attempt can be shown; damage took the slot or
  the part record". Advisory only: I'm not routing it back, because a damage-only doc nit is not
  worth a rebuild.
- NEEDS-HUMAN [human] — `check-gates.json:103-113` (`gate-logs/T4-batch-review.log:10`): the
  gating T4 batch review reports `pass`, with "0 blocking, 0 recorded-rejected, 0 noise-dropped",
  in **20.8 s** on a 135 KB, ~2.2k-line patch. That is the same pattern the iteration-2 sign-off
  rejected ("ran in 17 s with an empty review-batch.md; it did not really review this patch"). The
  brief's *Verification posture* requires "a confirmed high-effort review of the final patch".
  This row's `pass` is not that evidence, and I cannot see `review-batch.md` from here. Before
  accepting, a human should confirm that a real review of *this* patch happened, by this round's
  reviewer leaf or by hand. Otherwise the "independently reviewed in depth" claim carried over
  from iteration 3 applies to an earlier patch, not this one.
- `check-gates.json:55-63` (C4-diff-cov `fail`, "does not apply on origin/main"): the brief
  predicted this, because the base is stacked on unmerged #1051 and #1060. It is not a defect in
  the patch, and I'm not counting it as a refutation.

Scratch used: one copy of the patched tree plus a cargo build dir under `$PDCA_SCRATCH`
(`pdca-adversary-1030-work`, 1.2 GB). Both are removed.

<!-- pdca:leaf-complete -->
