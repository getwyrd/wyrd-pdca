# Build notes — issue 330 / scrub-detect-missing-placed-fragment (iteration 2)

## Carry-forward disposition

Iteration 1's sign-off (`iteration-v1/SUMMARY.md:94`) accepted the **patch content
as-is** — "no change of approach needed" — and blocked only on two toolchain-absence
gate artifacts:

- C4-ci: `./engine/xtask.sh: line 30: exec: cargo: not found`
- C4-verify: `run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied`
  (also a `cargo: not found` symptom, mis-surfaced by the gate as a red-with-fix
  failure rather than a missing-binary error)

The instruction was explicit: **re-run in an environment with the Rust toolchain**
and confirm the two new scrub tests are red pre-fix / green post-fix — not to change
the fix. This iteration does exactly that: **the production diff and the test diff
are byte-for-byte identical to iteration 1's** (`diff /tmp/regen_patch.diff
iteration-v1/patch.diff` → identical; confirmed below). What changed is that I
installed a working toolchain in this sandbox and **actually executed** the tests,
the target's own gate scripts, and the broader suite — the thing iteration 1 could
only reason about "by inspection" (per the reviewer's advisory, `iteration-v1/
check-review.md` embedded in `SUMMARY.md:58`: "Credible by inspection; not executed
(no toolchain)").

## Toolchain setup (this sandbox has none pre-installed on PATH)

- `rustup` and the pinned `1.96.0` toolchain (`$PDCA_WORKTREE/rust-toolchain.toml:4`)
  were already present under `~/.cargo` / `~/.rustup` from a prior session, but
  `~/.cargo/bin` was not on `PATH` in this fresh session — `export
  PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"` fixed that; `cargo 1.96.1` /
  `rustc 1.96.1` resolve after.
- No C compiler is installed and `apt-get`/`dpkg` are root-locked in this sandbox
  (`E: Unable to acquire the dpkg frontend lock … are you root?`). A prior session had
  left a user-local `ziglang` 0.16.0 wheel unpacked at `/tmp/pyzig/…/ziglang/zig`; I
  wired `~/.local/bin/{cc,gcc,x86_64-linux-gnu-gcc}` as thin wrappers around `zig cc`
  so `cargo`'s default linker driver resolves, and set `CC=cc`.

## What I ran, and what it proved

**1. The brief's repro, direct — `cargo test -p wyrd-custodian --test scrub`**
(`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt`, target branch tip
`1a9a3c7`, `getwyrd/wyrd@main`):

- **Post-fix (patch applied):** all 12 tests pass, including the brief's central
  test `detects_a_missing_placed_fragment_and_enqueues_for_reconstruction`
  (`crates/custodian/tests/scrub.rs:888-920` post-fix) and the false-positive
  guardrail `does_not_flag_an_in_flight_pending_writes_fragment_as_missing`
  (`crates/custodian/tests/scrub.rs:925-973` post-fix).
- **Pre-fix (production files reverted, test kept):** `git checkout --
  crates/custodian/src/scrub.rs crates/traits/src/lib.rs` (test file
  `crates/custodian/tests/scrub.rs` left modified), then the same command:
  **`detects_a_missing_placed_fragment_and_enqueues_for_reconstruction` FAILS**
  — `assertion left == right failed … left: Satisfied, right: Changed` — exactly the
  brief's repro ("Pre-fix: the chunk is never enqueued on the shared repair queue.").
  The other 11 tests, including the guardrail test, still pass — proving the
  guardrail assertion is fix-independent (as it must be: it asserts NO false
  positive, which holds whether or not the missing-fragment arm exists).
- Re-applied the fix (`git checkout -- crates/custodian/tests/scrub.rs; git apply
  patch.diff`) and re-ran: 12/12 green again. This is the flippable, red→green
  discriminator the Success criterion names.

**2. The target's own bundle-scoped gate — `engine/scripts/run-verify.sh`**
(`getwyrd/wyrd-pdca` repo, `engine/scripts/run-verify.sh:1-181`), run with
`PDCA_BUNDLE=results/issue_330`, `WYRD_REPO=/home/eddie/wyrd/wyrd` — this is
the actual `C4-verify` gate command (`pdca.toml:434`), not a hand-rolled
invocation:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian -p wyrd-traits (fix applied)
... (all packages pass) ...
run-verify.sh: PASS (green-only) — test is co-located with the fix (no separate
               */tests/*.rs), so the per-fix RED can't be isolated; C4-ci gates
               the whole tree.
```

The script classifies `crates/custodian/tests/scrub.rs` as a **modified**, not
**added**, file (`_added_files` at `run-verify.sh:64` only matches a
`--- /dev/null` preimage; this patch extends the pre-existing scrub test file
rather than adding a new one) — so its own red/green isolation logic
(`run-verify.sh:154-180`) cannot run automatically and it accepts green-only by
design (`run-verify.sh:155-159`), same as any co-located fix. That is an
intentional script limitation, not a defect: I supplied the missing red-half
manually in step 1 above, on the exact same test, and it is unambiguously red
pre-fix / green post-fix.

**3. Regression scope — `cargo test -p wyrd-custodian`** (all custodian test
binaries: `gc.rs`, `gc_telemetry.rs`, `rebalance.rs`, `reconstruction.rs`,
`scrub.rs`, `skeleton.rs`, `tier1_disk_faults.rs` (root-gated, ignored)):
**44 pass / 0 fail** with `-- --test-threads=1`. Under the default *parallel*
runner two DIFFERENT, untouched test files intermittently fail —
`rebalance.rs::emits_per_failure_domain_utilization_on_the_durability_seam` and
`reconstruction.rs::an_aborted_repair_is_not_counted_as_a_successful_repair` — a
process-global `tracing` callsite-registration race (exactly the hazard
`crates/custodian/tests/scrub.rs:194-214`'s own `enable_metric_callsites()` doc
comment already describes for *this* file, but `rebalance.rs`/`reconstruction.rs`
don't call it). **I confirmed this is pre-existing and unrelated to the patch**: I
`git stash`ed the fix in a second worktree (`wyrd-verify`, back to origin/main tip
`1a9a3c7` unmodified) and re-ran the identical `cargo test -p wyrd-custodian -p
wyrd-traits` — the same `reconstruction.rs` test fails intermittently there too (2
of 3 repeated runs passed, 1 failed, on the **unpatched** tree). Neither failing
test is touched by this diff (`patch.diff` touches only `scrub.rs`,
`crates/custodian/tests/scrub.rs`, and `traits/src/lib.rs`'s doc comment) — a
flaky, pre-existing test-parallelism hazard in the target's own suite, not a
regression this patch introduces.

**4. Style/lint on the touched crates — clean:**
- `cargo fmt --all -- --check` — no diff, whole workspace.
- `cargo clippy -p wyrd-custodian -p wyrd-traits --all-targets -- -D warnings` —
  clean, zero warnings.

**5. Whole-tree `cargo xtask ci` (the actual `C4-ci` gate,
`engine/xtask.sh:1-30` → `cargo xtask ci`) — blocked by a sandbox-only C-toolchain
gap, unrelated to this patch:**

`cargo xtask ci`'s `clippy --workspace --all-targets` step (`xtask/src/main.rs:537-
542`) fails building `alloca v0.4.0` — a transitive dev-dependency of `criterion`
(`Cargo.toml:52`, `wyrd-core/Cargo.toml:28`), pulled in only because
`--all-targets` also builds `wyrd-core`'s **benches**:

```
warning: alloca@0.4.0: error: unable to parse target query 'x86_64-unknown-linux-gnu': UnknownOperatingSystem
error: failed to run custom build command for `alloca v0.4.0`
```

This is `zig cc`'s clang frontend rejecting `cc-rs`'s `--target=x86_64-unknown-
linux-gnu` query format — a defect in the sandbox's substitute C toolchain (there is
no real `gcc`/`cc` here and no `apt`/root to install one:
`E: Unable to acquire the dpkg frontend lock … are you root?`), **not** in this
patch: no bench target anywhere touches `scrub`/`gc`/`custodian`, and this exact
failure is already documented as pre-existing in iteration 1's build-notes
(`iteration-v1/build-notes.md:132-144`, same `alloca`/`zig cc --target` symptom,
same root cause, same conclusion: "Not a real-CI concern: the project's actual
C4-ci/C4-verify gates run with a real cc, where this shim workaround is
unnecessary"). I confirmed the same failure reproduces identically scoped to just
`clippy --workspace --exclude wyrd-dst --all-targets` (the literal first
`--all-targets` step `run_ci` performs, `xtask/src/main.rs:537-542`) — it fails
before ever reaching my changed files, on a completely unrelated bench-only
dependency. When the touched crates are targeted directly (§4 above, no
`--workspace`), no bench graph is pulled in and clippy is clean.

**I did not attempt to work around this by disabling benches or vendoring a
different `alloca`/`criterion` version** — that would touch files this brief's
scope explicitly does not authorize (`Cargo.toml`/`Cargo.lock` outside the
scrub/traits seam) to paper over a sandbox toolchain gap, not a code defect. The
right fix is a real `cc` in the gate-running environment, which the actual CI
(GitHub Actions, per `docs/INTEGRATION.md:73-74`, "runs identically on a laptop and
in CI (ADR-0016)") has and this ad hoc sandbox does not.

## Alternatives considered

Same as iteration 1 (patch content unchanged) — see `iteration-v1/build-notes.md`
§"Alternative considered and ruled out" for the full cost comparison against (a) a
`list_fragments()`-driven diff pass and (b) moving detection into `read.rs`. Both
already ruled out there on concrete grounds; nothing in the toolchain re-run changes
that analysis, so I have not re-litigated it here.

## False-positive guardrails — reconfirmed executed, not just inspected

Iteration 1's reviewer flagged (`SUMMARY.md:64`) that the in-flight/pending-GC/
orphan guardrails were asserted "by inspection," not run. This iteration runs them
for real:
- **Orphan fragments:** `walks_and_verifies_referenced_fragments_through_reconcile_step`
  (`crates/custodian/tests/scrub.rs:219-259`) passes post-fix — an unreferenced,
  corrupt-looking fragment produces no finding, because the walk is now entirely
  `referenced`-driven (`crates/custodian/src/scrub.rs:85-94`).
- **In-flight (pending, uncommitted) writes:**
  `does_not_flag_an_in_flight_pending_writes_fragment_as_missing`
  (`crates/custodian/tests/scrub.rs:925-973`) passes post-fix (and, being
  fix-independent, also pre-fix) — `referenced_fragments` only resolves `Committed`
  inodes.
- **Pending-GC / expired-lease races:** not independently re-tested this iteration
  (same as iteration 1) — the argument rests on `referenced_fragments` being the
  identical set `gc.rs`'s own safety gate reads (`crate::gc::referenced_fragments`,
  imported at `crates/custodian/src/scrub.rs:42`), so scrub and GC can never
  disagree about what is "referenced." This is a structural argument, not an
  executed test; flagged again below for the human, unchanged from iteration 1.
- **Killed/partitioned D server (out of scope):**
  `scrub_propagates_a_transient_get_fault_without_enqueuing`
  (`crates/custodian/tests/scrub.rs`, pre-existing, unmodified) still passes
  post-fix — a transient `Err` still propagates via `Err(e) => return Err(e)`
  (`crates/custodian/src/scrub.rs:152`), never mistaken for the new `Ok(None)` arm.

## What remains for the human at sign-off

- The pending-GC/expired-lease guardrail is structural (shared `referenced_fragments`
  set with GC's safety gate), not covered by its own dedicated test — same gap
  iteration 1's reviewer flagged (`SUMMARY.md:64`), still open. Adding one is a
  reasonable follow-up but out of this brief's named test file's minimum bar (the
  brief's one required repro + a false-positive guardrail are both present and
  green).
- The whole-tree `cargo xtask ci` could not be completed end-to-end in this sandbox
  (§5 above) — the failure is demonstrably a sandbox C-toolchain gap on an unrelated
  bench dependency, reproduced identically scoped to the exact first `--all-targets`
  step and independent of any file this patch touches, but I have not observed a
  **full, unbroken** green `cargo xtask ci` run. Recommend the human (or a CI run
  with a real `cc`) confirms the whole-tree gate once outside this sandbox.
- The `codex` advisory leaf's absence (`check-advisory-codex.error.log`) is a
  driver/environment configuration matter (`[Errno 2] No such file or directory:
  'codex'`), not something the builder can fix — carried forward from iteration 1
  unchanged, per the brief's own scope (this brief only names `crates/custodian/
  tests/scrub.rs`, not leaf availability).

## Files touched (identical to iteration 1; verified byte-identical diff)

- `crates/custodian/src/scrub.rs:37,64-94,118-133,194-208` — the fix: group the
  reference set by placed D server, fetch each placed fragment directly by id via
  `ChunkStore::get_fragment`, treat `Ok(None)` as a durable loss (new `emit_missing`
  + `repair::enqueue_repair`), same as the corruption arm.
- `crates/custodian/tests/scrub.rs:875-973` — two new tests: the brief's repro
  (`detects_a_missing_placed_fragment_and_enqueues_for_reconstruction`) and the
  in-flight-write false-positive guardrail
  (`does_not_flag_an_in_flight_pending_writes_fragment_as_missing`).
- `crates/traits/src/lib.rs:251-266` (`ChunkStore::list_fragments` doc) — doc-only:
  clarifies scrub no longer drives off this listing (GC still does), now that
  `scrub.rs`'s own header doc (`scrub.rs:1-35`) says the same.
