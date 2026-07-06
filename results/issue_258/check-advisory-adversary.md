# Adversarial review — issue 258 / m4.7-dst-pin-second-impl

Lens: refute the red→green evidence and the reviewer's verdict. Grounded on the target
source at `/home/eddie/wyrd/wyrd.pdca-wt-l1`. Advisory only — I gate nothing.

## What I attempted and could NOT refute

I tried hard to break the fix and mostly failed — recording that, because a confirmatory
reviewer already gives the benefit of the doubt and this is the counterweight:

- **Re-ran the asserted proof.** Under `--cfg madsim` (the flag `cargo xtask dst` sets),
  all six new/changed tests are **green**: `concurrency.rs` (4) and `conformance.rs` (2).
  Green and stable across the **full 50-seed sweep** (`MADSIM_TEST_NUM=50`), run twice.
- **Tried to break exactly-one-winner over the sim-TiKV model.** Cannot. The decisive step
  is an atomic prewrite lock-grab inside a single `Mutex` critical section
  (`crates/dst/tests/support/mod.rs:428-446`); a second writer either sees the lock
  (`:434` → `Conflict`) or, if the winner already applied, fails the version precondition
  (`:438`). Two winners is unreachable; zero winners is unreachable (all four start from the
  matching prior version). Held across 50 seeds.
- **Checked for a lock held across `.await` (single-threaded madsim deadlock).** Each
  critical section closes (`:446`, `:453-460`) *before* `network_hop().await`
  (`:424, :450`). No re-entrant lock, no leaked lock on either `Conflict` return path.
- **Checked the demonstrated-red twin is not a tautology.** `synchronous_redb_shaped...`
  (`concurrency.rs:182-193`) is `#[should_panic(expected = "observes another writer
  mid-commit")]`; it passed with that specific message, i.e. it genuinely reached the
  `mid_commit_lock_conflicts >= 1` assertion with a count of 0 under the synchronous model —
  the red is real, not an unrelated panic being caught.

## Findings a human must adjudicate

- **NEEDS-HUMAN — the deterministic red→green gate never demonstrated green; acceptance
  rests on a hand-run.** `check-gates.json:46` records C4-verify as **FAIL**
  ("the bundle's test is RED *with* the fix applied (not green)") and it is non-gating, so
  `overall: pass` survives on the reviewer's word. The cause is a harness/flag mismatch, not
  a masked defect: both test files are `#![cfg(madsim)]` (`crates/dst/tests/concurrency.rs:34`,
  `crates/dst/tests/conformance.rs:19`), so a plain `cargo test` I ran sees **0 tests** in
  each — the named tests do not exist without `--cfg madsim`, which `run-verify.sh` evidently
  does not pass. I reproduced green manually under the flag, so the fix is real; but the
  BINDING criterion "green and seed-reproducible, **demonstrable at Check**" (brief lines
  35-43) is *not* machine-demonstrated by the deterministic gate. A human should confirm the
  gate ran with `--cfg madsim` (or accept the manual run) rather than read `overall: pass` as
  automated proof.

- **NEEDS-HUMAN — the "red" direction is encoded, never observed as a failing run.** The
  synchronous→await red→green is expressed as a `Fidelity` toggle plus a `#[should_panic]`
  twin (`concurrency.rs:162-193`, `support/mod.rs:291-303, :405-414`). This is sound and the
  brief permits it (lines 96-101), but note the redb-shaped defect is proven *structurally*
  (a should_panic that stays green forever), not by ever watching a real test go RED. The
  reviewer/human should treat "demonstrated red" here as "asserted-via-should_panic," not a
  witnessed failing CI run.

- **NEEDS-HUMAN — the pin binds a hand-written model, and its conflict-detection fidelity is
  the pre-declared open point (#264).** `SimTikvMetadataStore` in `tests/support/mod.rs` is
  the builder's *model* of TiKV, not the `wyrd-metadata-tikv` production commit path — so the
  "second implementation" pins a guess, by design (0015 forbids real TiKV in DST). Concretely,
  its conflict detection is a **pessimistic lock over the read+write union**: `write_set`
  includes precondition keys as well as put/delete keys (`support/mod.rs:384-392`) and any
  overlap with an in-flight commit's locks yields `Conflict` (`:434`). That is *stricter* than
  an optimistic/percolator backend and than redb, and it is exercised concurrently on exactly
  **one key by one race shape** (`concurrency.rs:75-135`), while the shared conformance clauses
  run with **zero concurrency** (`crates/metadata-conformance/src/lib.rs:291-309` is sequential).
  So the await-inside interleaving coverage — the whole point of the slice — rests on a single
  seeded race on a single inode key. Whether that fidelity/coverage is adequate is exactly the
  issue #264 / interleaving-adequacy sign-off the brief flags (lines 138-148); it is not
  resolved by the green suite and should not be read as resolved.

## Bottom line

I could not refute the fix's **correctness** — exactly-one-winner holds across 50 seeds on
both backends, and the corrected `concurrency.rs` rationale is accurate. My live refutations
are about the **evidence and scope**: the deterministic red→green gate is recorded as FAIL and
only a flagged manual run shows green; the pin binds a narrow, hand-written model whose
fidelity is the explicitly-open #264 judgment. Those are for the human at sign-off.
