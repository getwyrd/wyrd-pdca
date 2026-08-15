# Adversarial review — issue #697 (advisory, non-gating)

Toolchain was available; the red→green was re-run in a scratch copy of `$PDCA_TARGET`
(`cargo 1.96.0`), green with the patch (8/8 pass) and red with `crates/custodian/src/reconstruction.rs`
reverted to `origin/main @ 339da46` (**7** of 8 fail). Findings below are grounded on the target tree.

## Refutations that landed

- **NEEDS-HUMAN [impl]** — `crates/custodian/tests/segmented_map_reconstruction.rs:427`: leg 1's
  binding sub-assertion is **swallowed by a comment and never executes**. The line reads
  `    // referenced by nothing — which only a COMPLETE reading may answer (leg 3 is the other side).    assert!(store.queued().await.is_empty(), "both discharged");`
  — the `assert!` sits on the same physical line, after `//`. The brief's leg 1 (brief.md:52-55)
  requires "the flat chunk's placement moves, **and its obligation is drained**"; the drain half is
  unasserted, so leg 1 as shipped would stay green against a regression that stopped draining
  `C_UNSEEN`. Verified: restoring the assertion onto its own line still passes with the fix, so the
  property holds and the fix is fine — the **test** is the defect. Neither `cargo fmt` nor clippy can
  see this (rustfmt does not reflow comments), so C4-ci's green says nothing about it.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/reconstruction.rs:253-256` (+ `:1035`
  `emit_refused`): **Rule D is not implemented.** brief.md:202 pins, do-not-relitigate, "*Rule D — a
  refusal is reported once per **object**, not once per chunk*". `emit_refused` is called from inside
  `for &chunk in &queue`, i.e. once per **obligation**. Measured concrete case (probe driven through
  `reconcile_step` over the patch's own fixture): one segmented object `inode:006` holding two queued
  chunks (`S_A`, `S_B`) emits **2** `action=refused` audit lines, **both** carrying
  `"inode":"inode:006"`, and increments `monotonic_counter.reconstruction_repair_refused` **twice**.
  A real multipart object whose `seg:` records hold Q queued chunks therefore floods the durability
  seam with Q copies of one object's refusal and makes the counter measure obligations rather than
  objects. No leg binds Rule D (the brief claims "each is bound by a leg above" at brief.md:155 —
  that claim is unwarranted for rule 6), which is why nothing caught it.

- **NEEDS-HUMAN [human]** — **the test file busts the brief's hard STOP budget.**
  `crates/custodian/tests/segmented_map_reconstruction.rs` is **657 raw** lines (`wc -l`; the diff
  header is `@@ -0,0 +1,657 @@`) and ~**474 semantic** (non-blank, non-comment). brief.md:238-247
  caps it at "**≤ 380 semantic / 620 raw**" and states: "*a test file past 620 raw means the shape is
  wrong: STOP and hand back rather than finish*". Production is within budget (189 added semantic vs
  the 230 cap), so this is purely the discriminator. `check-gates.json` carries **no** budget row and
  the reviewer's verdict does not mention it, so the STOP condition the Plan wrote was silently
  skipped rather than adjudicated.

- **NEEDS-HUMAN [impl]** — brief.md:97-102 / :114-116 declare leg 8
  (`a_fault_that_is_not_one_objects_map_still_ends_the_pass`) **not base-red** — "*it passes before
  and after*", "*legs 7 and 8 are declared non-red*". **Measured false.** On `339da46` with
  production reverted, leg 8 FAILS:
  `the pass absorbed it: reconciliation store access: key must be a string at line 1 column 2`.
  Its fixture seeds `seed_damaged()` first, so on the base the pass aborts at the undecodable
  `inode:0` record long before the injected `get` fault is reachable — the leg goes red for a reason
  that has nothing to do with the over-containment property it exists to guard. Exactly 7 of 8 legs
  are red on the base, not the 6 the brief predicts. Either the pre-declaration or leg 8's fixture
  (drop `seed_damaged()`, or assert only the store-fault half) needs correcting.

- **NEEDS-HUMAN [impl]** — `check-gates.json:48`'s C4-verify evidence line, "*red without the fix,
  green with it (**8 test(s) ran red**)*", reads as "8 legs discriminate" but
  `engine/scripts/run-verify.sh:508` interpolates `$TESTS_RAN` — the number of tests that **ran**,
  not that **failed**. Measured: 7 failed, 1 passed
  (`an_empty_queue_reads_nothing_and_certifies`, correctly non-red by brief.md:88-96). The row is not
  wrong about the gate's verdict, but it is not evidence for the per-leg red the brief asserts, and
  it is the only red→green artifact in the bundle. Worth one line at sign-off so the "8" is not read
  as a per-leg count.

## Attacked and could not refute

- **Rule A containment** (`reconstruction.rs:837`, value-compare `resolved.record` against the
  scanned record). I tried to get a repoint through on a generation the pass never scanned, and I
  tried the quieter sibling: the `Ok(None) => continue` arm at `reconstruction.rs:813`, hoping a
  `seg:`-resident obligation would be **drained** (violating leg 2's "refused, never discarded")
  when the root is retired under the resolve. It does not reach: `root_dropped`
  (`crates/core/src/metadata.rs:2323-2340`) compares only the segment **group**, so a Pending
  overwrite sharing the group is not "dropped" at all (my probe came back `Blocked`, obligation
  kept, reason `segmented-chunk-map`); and where `Ok(None)` genuinely fires the object really has no
  live committed generation, so the drain is the correct answer. Flat records never take this arm
  (`metadata.rs:2585` answers `Cow::Borrowed`), so Rule A is trivially satisfied for every record
  this slice writes to.
- **The Q×N discriminator.** In the shipped file leg 6 goes red on the base at
  `assert_eq!(got, Reconciled::Changed)` (base answers `Satisfied`, because the fixture's `stored()`
  spelling defeats the base's `require(key, encode(&prior))` CAS) — so the headline scan-count
  assertion is *masked* and I expected it to be untested. It is not: relaxing that earlier assertion
  and re-running on `339da46` gives `namespace == 3` vs `1` at
  `segmented_map_reconstruction.rs:606`. The #647 property is a genuine discriminator.
- **`repair_chunk`'s new guards** (`reconstruction.rs:653-669`): I could not construct a
  `prior_bytes` whose `chunk_index` is in range but names a different chunk, nor a non-flat one —
  `hits` is enumerated off the same generation and the id filter is a second guard on the same fact.
- **Rule C** (`reconstruction.rs:676` CASes on the raw scan key): `inode:007` beside `inode:7` is
  correctly separated, and I could not find a spelling that reads one and commits the other.
- **The new `parse_inode_key` containment** (`reconstruction.rs:805`) over-contains relative to GC
  (`gc.rs:360-455` has no such check), so a stray key under the `inode:` prefix would pin the pass at
  `Blocked` and block every drain forever. Not raised as a defect: `metadata::inode_key`
  (`crates/core/src/metadata.rs:35`) is `format!("inode:{id}")`, so no production writer can mint
  such a key, and the direction is fail-closed.
- **Memory / bounded work**: retention is one `Arc<[u8]>` of stored bytes and one `ChunkRef` per
  *obligation*, ≤ Q objects; no decoded chunk list is kept per object. The whole-namespace
  `meta.scan(b"inode:")` materialisation is pre-existing and shared with `gc.rs:365`.
- **Not re-raised, per the target rubric's reviewer protocol** (`AGENTS.md:200-203`): the absent
  Tier-0 DST leg (settled recorded-rejected at brief.md:260-265) and the four `review-rejected.md`
  findings about orphan-marking a fragment a hidden object still references (rule B).
