# Result — issue 440 / server-fdb-backend-selection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `MetadataBackend` (`crates/server/src/cli.rs:88`) offers only `Redb` and
- Success criterion: In the **default build** (no `fdb` feature, no `libfdb_c`, no
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Extend metadata-backend selection with an `Fdb` variant, compiled only under a

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
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

Reviewing issue 440: expose the shipped FoundationDB metadata backend through `wyrd-server` metadata-backend selection without changing the `MetadataStore` consumer seam.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed decision is concrete: default builds must reject `fdb` with a feature hint, list it in unknown/usage text, and keep FDB linkage out of default CI; the brief states those checks at `brief.md:12`. |
| C2 Reproduction (red pre-fix) | PASS | Reverting the production hunks reproduced the user defect: `--metadata-backend fdb` returned unknown-backend text and no-args usage omitted `fdb`, matching `brief.md:111`. |
| C3 Change | PASS | The patch reaches the composition surface the operator depends on: `fdb` feature/dependency at `crates/server/Cargo.toml:31`, config selection at `crates/server/src/cli.rs:120`, usage at `crates/server/src/cli.rs:268`, and lockfile dependency at `Cargo.lock:5183`. |
| C4 Verification (red->green) | NEEDS-HUMAN | The exact `./engine/scripts/run-verify.sh` harness is absent here; my manual red phase went red before the intended assertions because `#[cfg(feature = "fdb")]` in `crates/server/tests/fdb_backend_selection.rs:44` becomes an undeclared cfg under workspace `warnings = "deny"` at `Cargo.toml:195`, while patched green checks passed. |
| C5 Causal adequacy | PASS | The owed root-cause decision is satisfied: missing server selection is fixed by composition-root arms to `FdbMetadataStore::connect()` at `crates/server/src/cli.rs:168`, with no capability-probe/runtime-guard smell introduced. |
| T1 Structure | PASS | The boundary decision holds: changes stay in server composition, manifest, lockfile, and a regression test; no consumer refactor is needed beyond backend dispatch arms such as `crates/server/src/cli.rs:373`. |
| T2 Shape | PASS | The feature shape matches the risk boundary: default builds keep FDB optional at `crates/server/Cargo.toml:31`, feature-on builds select `MetadataBackend::Fdb` in the gated test at `crates/server/tests/fdb_backend_selection.rs:44`. |
| T3 Runtime | NEEDS-HUMAN | The binding runtime checks passed (`cargo test -p wyrd-server --test fdb_backend_selection`, `cargo xtask ci`, feature-on check/test), but the live FDB `put`/`get` required by `brief.md:154` was not exercised because Docker socket access was denied. |
| T4 Contribution | PASS | Prior-art by affected file path was mechanically checked; `git log -G 'Fdb|fdb|metadata-fdb'` over server files found no previous server-side FDB selection attempt, consistent with `brief.md:199`. |
| T5 Judgment | PASS | The human decision is not blocked by scope creep: this implements selection, not deployment, and leaves #439's compose/doctor/CI workflow boundary intact as scoped at `brief.md:109`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether default-build selection plus feature-on compile/test is sufficient before the unavailable live FDB round-trip is rerun in a Docker-enabled environment, because `brief.md:161` says that check is non-gating but not silently skippable. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red->green) — The exact `./engine/scripts/run-verify.sh` harness is absent here; my manual red phase went red before the intended assertions because `#[cfg(feature = "fdb")]` in `crates/server/tests/fdb_backend_selection.rs:44` becomes an undeclared cfg under workspace `warnings = "deny"` at `Cargo.toml:195`, while patched green checks passed.
- [ ] T3 Runtime — The binding runtime checks passed (`cargo test -p wyrd-server --test fdb_backend_selection`, `cargo xtask ci`, feature-on check/test), but the live FDB `put`/`get` required by `brief.md:154` was not exercised because Docker socket access was denied.
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether default-build selection plus feature-on compile/test is sufficient before the unavailable live FDB round-trip is rerun in a Docker-enabled environment, because `brief.md:161` says that check is non-gating but not silently skippable.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on evidence integrity, not on the code. The patch itself is believed correct and should be preserved substantially as-is — this is a rebuild of the *verification record*, not a redesign. Do not re-litigate the composition-root approach, the sync `open_fdb_meta`, or the cluster-file semantics; the brief resolved those and the reviewer passed C3/C5/T1/T2. What to change, in priority order: 1. `build-notes.md` §4 is a FABRICATED transcript presented as "pasted verbatim". The same `/tmp/input.txt` is shown as `bytes=38` on one put and `bytes=17` on another; `bytes` is `data.len()` (`cli.rs:377-378`), so the same file cannot be two sizes. The recorded `inode=4` is also wrong. Verified at sign-off by running the live round-trip: the true output is `inode=2 ... bytes=38`. The round-trip DOES work — every claim in §4 was independently reproduced (put/get byte-identical, conflict `already exists`, not-found, default build rejecting `fdb` with the feature hint) — so the conclusion was sound and the presentation was not. Re-run the flow and paste the ACTUAL terminal output. A transcript labelled verbatim must be verbatim; if a step is reconstructed from memory or from the code, label it as such or omit it. 2. The C4 RED is a compile error, not an assertion failure. Reverting the manifest makes `#[cfg(feature = "fdb")]` an undeclared cfg, and workspace `warnings = "deny"` (`Cargo.toml:195`) promotes `unexpected_cfgs` to a hard error despite its explicit `level = "warn"` — rustc says so directly: `-D unexpected-cfgs implied by -D warnings`. The brief's Falsifiability paragraph (`brief.md:47-49`) asserts the opposite and is simply wrong; the builder caught this and disclosed it honestly, which is to its credit. `run-verify.sh` accepts any non-zero exit as RED (`if run_test; then FAIL`), so the gate passed on a RED that proves "the crate does not build" rather than "the test catches the bug". The discrimination IS real — verified at sign-off by declaring `fdb` a known cfg (`RUSTFLAGS=--check-cfg=cfg(feature,values("fdb"))`, no lint disabled, no code changed) in the RED state: `0 passed; 3 failed`, all three failing on message content exactly as the brief predicted. Rebuild so the harness can SHOW that: the RED transcript must exhibit three assertion failures, not a compile error. Declaring the `fdb` feature value in the workspace `check-cfg` list is the obvious route; any route is acceptable that leaves the RED assertion-driven under the unmodified `run-verify.sh`. 3. `protoc` IS installed (`/usr/bin/protoc`, libprotoc 3.21.12). The builder's claim that it could not compile-verify the `(Fdb, Etcd)` arm at `cli.rs:1487-1492` does not hold on this host. Verified at sign-off: `cargo check -p wyrd-server --features fdb,etcd --tests` is clean and really builds `wyrd-coordination-etcd`, so that arm really compiles. Run it and record it. Before declaring an external dependency unavailable, check it with `command -v` and paste the result. 4. The builder's own `NEEDS-HUMAN external dependency: protoc` note lived only in `build-notes.md` and never reached `SUMMARY.md` §6, because the reviewer is decorrelated from `build-notes.md` by design and nothing else propagates it. Any NEEDS-HUMAN the builder declares must surface where the human actually clears items. Standing instruction for the rebuild: evidence is the deliverable here, not the diff. A claim that cannot be reproduced from the pasted commands is worse than an admitted gap — an admitted gap gets checked at sign-off, an unadmitted one ships.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
