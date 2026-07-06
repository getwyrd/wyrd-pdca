# Result — issue 258 / m4.7-dst-pin-second-impl

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: in `crates/dst` (and, if a shared harness is chosen, the existing shared

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (a redb-shaped rationale/coverage defect plus a
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied (not green).
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 258 / m4.7-dst-pin-second-impl

**Task under review:** M4 is the *second* `MetadataStore` implementation, so it must **pin
and harden** the trait. Deliver a second implementation *inside* the deterministic simulator
(a deterministic simulated-TiKV model or a trait-level contract harness) so the **identical**
property suite drives both backends green and seed-reproducibly, and correct the redb-shaped
"no await inside" determinism rationale in `crates/dst/tests/concurrency.rs:3-6` for an
**await-inside-commit** backend, committing a reproduces-forever seed.

**Target:** `/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch applied; verified against source). The
sandbox blocks `cargo`/`git` in the target, so I could not re-run the suite myself; the
authoritative evidence is the **gating** `xtask ci` row in `check-gates.json`, which itself
runs the full madsim DST suite (see below).

## Key reconciliation — the advisory C4-verify "FAIL"

`check-gates.json` carries two C4 rows. The **gating** one — `C4-ci` (`xtask ci`) — is
**pass** ("xtask ci: all checks passed"). `run_ci()` calls `run_dst()` at
`xtask/src/main.rs:823`, which runs `cargo test -p wyrd-dst` under `--cfg madsim` with
`MADSIM_TEST_NUM=50` (`xtask/src/main.rs:834-855`). That command runs **every** test in
`wyrd-dst`, including the patch's new `concurrency.rs` regression tests and the `conformance.rs`
both-backends suite; if any were red, `run_dst` → `run_ci` would `Err` and "all checks passed"
would never print. So the fix-applied state — including the per-fix regression — is genuinely
**green** under the correct `--cfg madsim` environment across 50 seeds.

The **non-gating** `C4-verify` row (`run-verify.sh`) is `fail` ("test RED *with* the fix
applied"). This is a harness/posture mismatch, **not** a patch defect: (a) the DST tests are
`#![cfg(madsim)]` (`concurrency.rs:34`) and compile to nothing without `--cfg madsim`, which a
generic per-fix runner does not set; and (b) the demonstrated-red is encoded **in-suite** as
the `SynchronousRedbShaped` `#[should_panic]` twin (`concurrency.rs:182-193`), not as a
git-stash flip — exactly the **MIXED verification posture** the brief pre-authorized
(brief "Disposition hint"; "Test file"). Per reviewer discipline I do not lift this into a
blocking C4 FAIL.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief gives binding, checkable success criteria — both backends through the identical suite, corrected `concurrency.rs:3-6` rationale, committed reproduces-forever seed (brief "Success criterion" (a)(b)(c)). Spec is unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | The red is demonstrated in-suite: `SynchronousRedbShaped` fidelity keeps `mid_commit_lock_conflicts == 0`, so the `>= 1` assertion panics — caught by `#[should_panic(expected="observes another writer mid-commit")]` at `concurrency.rs:182-193`. That this twin *and* its green `AwaitInsideCommit` counterpart (`concurrency.rs:162-172`) both pass under `xtask ci` proves the seed genuinely flips red→green on the modeled await boundary. |
| C3 Change | PASS | Change is scoped to `crates/dst` (`concurrency.rs`, new `conformance.rs`, new `support/mod.rs`) + a `wyrd-metadata-conformance` dev-dep (`crates/dst/Cargo.toml:24`, `Cargo.lock`). No `metadata-tikv` code, no trait edit, no real TiKV in DST — all declared out-of-scope stay untouched. |
| C4 Verification (red→green) | PASS | Gating `xtask ci` runs the DST suite under `--cfg madsim`×50 seeds via `run_dst` (`xtask/src/main.rs:823,834-855`) and passed; the fix-applied regression is green. Advisory `run-verify.sh` FAIL is a `#![cfg(madsim)]` / declared-MIXED-posture harness mismatch, not a real red-with-fix (see reconciliation above). Human should confirm the MIXED posture is acceptable. |
| C5 Causal adequacy | NEEDS-HUMAN | The fix removes the true cause (models the real await-inside-commit and rewrites the "no await inside" rationale) rather than weakening the assertion, and adds no capability-probe/runtime guard — so the symptom-guard smell-test does **not** fire. But the **model-fidelity choice** (pessimistic-lock at an atomic prewrite, `support/mod.rs:404-462`) is the explicitly-open crux #264 (0015:798-801): whether that fidelity keeps "exactly one wins" coverage *honest* vs a real 2PC/TSO interleaving is the human's ratification, not the reviewer's. |
| T1 Structure | PASS | `support/mod.rs` sits under `tests/` (dev/test scope, never shipped), reuses the shared `wyrd_metadata_conformance` suite ("shared, not forked"), uses only instance state via `Mutex<Inner>` — no `static` — so it stays outside the ADR-0035 `src/`-only global-state gate. Mirrors the existing `demonstrated_red.rs` discipline. |
| T2 Shape | PASS | `SimTikvMetadataStore` implements the unchanged trait exactly: `get`/`scan`/`commit` signatures match `traits/src/lib.rs:340-350`; `Precondition{key,expected}` and `WriteBatch{preconditions,puts,deletes}` field use matches `:365-382`; `run_all(|_tag| async {…})` matches the `FnMut(&'static str)->Fut<Output=S>` bound at `metadata-conformance/src/lib.rs:291-296`. Trait (`crates/traits`) untouched — invariant held. |
| T3 Runtime | PASS | Passed under `xtask ci` across 50 seeds. Determinism sources checked: `truth` is a `BTreeMap` (ordered `scan`), `locks` is a `HashSet` only ever `contains`/`insert`/`remove`d — never iterated (`support/mod.rs:326-328,434-457`) — so no iteration-order nondeterminism; single-threaded madsim means `Mutex` never truly contends. |
| T4 Contribution | PASS | Delivers the three DoD artifacts: both backends through the identical shared suite (`conformance.rs`), corrected rationale (`concurrency.rs:1-33`), and a committed reproduces-forever seed `PINNED_INTERLEAVING_SEED` (`concurrency.rs:58`). Prior-art on these DST files could not be mechanically confirmed (git blocked in sandbox), but the brief states `crates/dst`/`concurrency.rs` are this slice's exclusive territory with no sibling edits. |
| T5 Judgment | NEEDS-HUMAN | Interleaving-coverage adequacy (0015 known-human item): exactly-one-winner holds under CAS regardless, so the human must judge whether the required change is a *genuinely new set of reachable interleavings* (the await boundary) or *only* a corrected comment. The patch asserts the former via `mid_commit_lock_conflicts >= 1` (`concurrency.rs:162-172`); sign-off owed on whether that coverage is load-bearing and honest. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human ratifies at sign-off that this second-implementation-in-DST pin actually fulfils M4.7's purpose — that the fidelity level catches the interleavings that matter and the corrected rationale is sound — before the draft PR is marked ready/merged (builder must not self-merge). Also standing: `tikv-client` futures `Send+Sync` for the real object-safe trait remains a *confirm-at-build* item for the real backend (0015:778-779), out of this slice's model scope. |

### Advisory — adversary

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

### Advisory — codex

- NEEDS-HUMAN — crates/dst/tests/support/mod.rs:27 proposes the simulated-TiKV fidelity as "pessimistic-lock at an atomic prewrite"; this is exactly the open #264 judgment from the brief, so sign-off should explicitly ratify whether this model is faithful enough for the await-inside-commit coverage.
- NEEDS-HUMAN — crates/dst/tests/concurrency.rs:177 adds the pinned interleaving test, but `check-gates.json` reports `run-verify.sh` red with the fix applied. I could run the narrow DST tests green locally with `RUSTFLAGS='--cfg madsim' CARGO_TARGET_DIR=/tmp/pdca-advisory-idcys5md/target cargo test -p wyrd-dst --test concurrency --test conformance -- --nocapture`, so this looks like a verification-posture/tooling adjudication rather than a reproduced test failure.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — The fix removes the true cause (models the real await-inside-commit and rewrites the "no await inside" rationale) rather than weakening the assertion, and adds no capability-probe/runtime guard — so the symptom-guard smell-test does **not** fire. But the **model-fidelity choice** (pessimistic-lock at an atomic prewrite, `support/mod.rs:404-462`) is the explicitly-open crux #264 (0015:798-801): whether that fidelity keeps "exactly one wins" coverage *honest* vs a real 2PC/TSO interleaving is the human's ratification, not the reviewer's.
- [x] T5 Judgment — Interleaving-coverage adequacy (0015 known-human item): exactly-one-winner holds under CAS regardless, so the human must judge whether the required change is a *genuinely new set of reachable interleavings* (the await boundary) or *only* a corrected comment. The patch asserts the former via `mid_commit_lock_conflicts >= 1` (`concurrency.rs:162-172`); sign-off owed on whether that coverage is load-bearing and honest.
- [x] Validation — fitness-to-purpose — Human ratifies at sign-off that this second-implementation-in-DST pin actually fulfils M4.7's purpose — that the fidelity level catches the interleavings that matter and the corrected rationale is sound — before the draft PR is marked ready/merged (builder must not self-merge). Also standing: `tikv-client` futures `Send+Sync` for the real object-safe trait remains a *confirm-at-build* item for the real backend (0015:778-779), out of this slice's model scope.
- [x] crates/dst/tests/support/mod.rs:27 proposes the simulated-TiKV fidelity as "pessimistic-lock at an atomic prewrite"; this is exactly the open #264 judgment from the brief, so sign-off should explicitly ratify whether this model is faithful enough for the await-inside-commit coverage.
- [x] crates/dst/tests/concurrency.rs:177 adds the pinned interleaving test, but `check-gates.json` reports `run-verify.sh` red with the fix applied. I could run the narrow DST tests green locally with `RUSTFLAGS='--cfg madsim' CARGO_TARGET_DIR=/tmp/pdca-advisory-idcys5md/target cargo test -p wyrd-dst --test concurrency --test conformance -- --nocapture`, so this looks like a verification-posture/tooling adjudication rather than a reproduced test failure.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_258: `run-verify.sh` (C4-verify) has no way to run a crate needing a bespoke test environment (e.g. `--cfg madsim` via `cargo xtask dst`); it runs a plain `cargo test` and reports a bogus "RED with fix applied" FAIL. Give it a per-repo/per-crate run hook (a parameter, or an alternate validation script) so madsim-gated bundles earn a real automated green-with-fix instead of relying solely on the gating C4-ci.
