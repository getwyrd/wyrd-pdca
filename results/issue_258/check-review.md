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
