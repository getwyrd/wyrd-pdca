# Build notes — issue 651 (iteration 7) / restore-and-desired-state-contained-and-attributed

> Withheld from the reviewer by the driver. For the human at sign-off.

## 0. READ THIS FIRST — two environment findings for sign-off

### (a) A second builder process was writing this same worktree, concurrently

At 22:36:22, while I was mid-edit, `crates/custodian/src/gc.rs` in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`) changed underneath me — a `ReferenceSet.committed:
Vec<CommittedChunk>` field I did not write. `ps` showed **two** builder processes pinned to the
same worktree:

```
2098261  (ppid 8500, systemd --user)  claude -p --agent builder ... --add-dir /home/eddie/wyrd/wyrd.pdca-wt-l0   [started ~2 min before me]
2099836  (ppid 2099809 = wyrd-pdca flow 651 652)  ... --add-dir /home/eddie/wyrd/wyrd.pdca-wt-l0                 [me]
```

PID 2098261's env carried `PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l0` and it was clearly
building **this same issue** (its gc.rs doc text names `crate::restore::RestoreReport::misplaced`).
Its parent had already died (re-parented to `systemd --user`), i.e. it was an orphan of an earlier
run of this flow that the driver never reaped.

**What I did.** I moved all work into a private worktree off the same base
(`$PDCA_SCRATCH/pdca-builder-651-build`, detached at `d50f0ca`) and built there. The orphan exited
on its own at ~22:47 leaving `gc.rs` + `restore.rs` dirty and **no bundle artifacts** (it never
wrote `patch.diff`). I then `git reset --hard d50f0ca`'d `$PDCA_WORKTREE`, copied my eight files
in, and generated `patch.diff` from it — so the worktree the C4-ci gate will run is exactly my
patch, with **no trace of the orphan's gc.rs edit** (verified: `git diff --name-only` lists my
eight files only). My own scratch worktree (3.3 GB) has been removed.

**One orphan left to sweep, deliberately not by me:** the dead process's own scratch worktree
`/var/tmp/pdca/pdca-builder-651-tree` (8.3 MB, registered in `git -C ../wyrd worktree list`).
It is not mine to remove and the sandbox refused the compound command; a
`git -C ../wyrd worktree remove --force /var/tmp/pdca/pdca-builder-651-tree` clears it. Note it
sits at `$PDCA_SCRATCH`'s *un-suffixed* root (`/var/tmp/pdca`) rather than the per-bundle dir,
which is how it escaped the bundle-scoped sweep.

**Why the human should care:** this is a driver-level defect (a duplicate/orphaned builder on a
shared worktree), not a code one. If it recurs while a builder is *finishing*, the bundle's
`patch.diff` would silently contain two agents' half-work. Worth an Act item:
`pdca` should reap or lane-scope orphaned leaf processes before starting a Do beat.

### (b) `cargo xtask ci` fails at a `cargo deny` step on this host, on the pristine base

`xtask ci` runs `cargo deny --all-features --config deny-all-features.toml check advisories`
(hard-coded at `xtask/src/lib.rs:163-183` on the base). The installed **cargo-deny 0.19.9**
rejects a global `--config` before the subcommand (`error: unexpected argument '--config' found;
tip: 'check --config' exists`). I reproduced it in the **pristine** `../wyrd` checkout (clean
`git status`, on `main`) — same error — so it is a host/tooling drift, **independent of this
patch** (which touches no `xtask/`, `Cargo.*` or `deny.toml` file). Everything before it in the
pipeline — the prose gates, `fmt --check`, `clippy -D warnings`, build, the whole test suite,
`cargo-machete`, `cargo deny check` — passed. If C4-ci comes back red on this step, it is that
drift, not the change; the fix is upgrading the invocation in `xtask` (out of this slice's scope)
or pinning cargo-deny.

## 1. What the fix is

Two surfaces that report *whether a reconciliation is complete* now answer **contained and
attributed** over a reference set with a hole in it, and the operator command says so and exits
non-zero.

| file | change | line (post-patch) |
|---|---|---|
| `crates/custodian/src/restore.rs` | `RestoreReport::unresolvable: Vec<String>`; `is_clean()` false while it is non-empty | `:142`, `:171` |
| | report half reads records through `metadata::resolve_chunk_map` and **contains** a record-level fault instead of `?`-ing out | `:473` (`committed_chunks`) |
| | union-of-both-reads naming + audit emission, before the fleet walk | `:241`, `:550`, `:669` |
| | summary line says `INCOMPLETE` when it could not read everything | `:688` |
| `crates/custodian/src/desired_state.rs` | `ReconciliationStatus::PendingUnresolvable { objects }` + attribution + drain audit event | `:105`, `:225`, `:257` |
| `crates/server/src/cli.rs` | `restore_verdict()` — the printed lines **and** the exit decision as one value; `unresolvable` joins `dangling`/`misplaced` in both | `:1258`, `:1203` |
| `docs/.../06-runtime-view.md` §6.2 step 2 | the two reporting sentences the brief names | `:31` |
| `docs/.../m4-first-deployment-blueprint.md` | the runbook's third bill (`UNREADABLE`) | `:599`, `:609` |

Tests: the new discriminator `crates/custodian/tests/segmented_map_restore.rs` (4 legs, criteria
1/2a/2b/3, base-visible symbols only); new legs in `crates/custodian/tests/restore_reconcile.rs`
(`:846`, `:922`) and three in `crates/server/src/cli.rs`'s existing `mod tests` (`:2695`, `:2726`,
`:2748`); the two `segmented_map_consumers.rs` drain assertions re-pinned onto the attributed
variant (`:723`, `:1101`).

Budget: **654 semantic added lines** (non-blank, non-comment) across **8 files** — under the
brief's ≤700 / ≤8.

## 2. The carry-forward from iteration 6, addressed

`review-batch.md` carried three blocking findings. All three are **fixed**, none rejected:

**(1) + (2) `restore.rs:449` / `:461` — keying `Expected` by `ChunkId` merges distinct committed
references.** v6 rebuilt the report half by regrouping `ReferenceSet.schemes` / `.placed` (both
id-keyed maps). That is genuinely fail-open for a *report*: two committed objects may carry the
same chunk id with different placements, and the union answers for both. Concretely — and this is
now a test — object 1 places chunk 61 on d0 (fragment present), object 2 places the same id on d1
(nothing there):

| | base / this patch | v6 (id-merged) |
|---|---|---|
| verdict | `misplaced: [61]` | `under_replicated: [61]` |
| CLI | NEEDS-HUMAN, **exit 1** | silent, **exit 0** |

Object 2 is unreadable (the read path and reconstruction both fetch strictly through *its*
placement, `restore.rs:327-334`), so v6 printed a hollow green over a down object — the very
failure class this slice exists to close. Not exotic, either: chunk ids are minted
`inode << 64 | seq` from the inode counter (`crates/server/src/cli.rs:1714-1723`), and a restore
rewinds that counter, so post-restore id reuse is this pass's own world (#652 owns the allocator
floor).

**Fix:** the report half keeps restore's **own** record walk — the one the base already had at
`restore.rs:390` — and upgrades it in place: resolve each committed root through the shared
`metadata::resolve_chunk_map`, contain a `ChunkMapError` into `unresolvable`, propagate anything
else, and emit **one entry per committed reference** exactly as the base did. The judging loop
(`:367-393`) is untouched, so the per-reference semantics of `dangling` / `misplaced` /
`under_replicated` are bit-for-bit the base's.

*Alternatives weighed:*
- **Extend `ReferenceSet` with a per-reference list** (what the orphaned sibling process was
  doing in `gc.rs`, and what would let the report half stay on one read): it makes `gc.rs` a
  **9th file** against the brief's ≤8 cap, changes a struct `#681`/`#682` both build on while
  they are in flight, and costs one `(u16, DServerId)` per committed fragment in *every*
  consumer's memory — GC, scrub and the drain-status query included, none of which read it. I
  rejected it on the file cap and on the cross-slice conflict, not on taste.
- **Keep v6's id-keyed merge and add a collision guard**: undetectable from the aggregates. The
  set has no per-object information at all, so there is nothing to guard on.
- **Cost of what I chose:** one extra `inode:` scan (which the base also had — I removed nothing
  the base did not do) and a second bounded `seg:` range read per *segmented* object. That is the
  honest price of per-reference fidelity in a pass that is an operator one-shot run with writers
  stopped. Documented at `restore.rs:454-472`, with a `deferred: #681` marker for the shared walk
  that will unify the two.

**(3) `cli.rs:1281` — no CLI-level test of the new diagnosis + exit code.** The print/exit block
is now `restore_verdict(&RestoreReport) -> RestoreVerdict { lines, needs_human }`
(`cli.rs:1258`), called by `cmd_custodian` at `:1203`. Three tests in cli.rs's existing
`#[cfg(test)] mod tests` pin it: an unreadable-only report exits non-zero, never prints
"reconciliation complete", and names `inode:7`; a clean report still prints "complete" and exits
0 (so the fix is not satisfiable by failing every run); and the inline-name cap counts the
remainder rather than dropping it. They live *inside* the modified `cli.rs` deliberately — a new
`crates/server/tests/*.rs` file would add a second `ADDED_TEST` to `run-verify.sh --classify` and
change the discriminator invocation the brief pinned.

**C5 (advisory, 7 missed mutants in v6):** reduced by construction rather than by chasing the
report — v6's sort-and-regroup helpers are gone (fewer mutable knobs), and the three new
observables (the audit `INCOMPLETE` summary assertion in the discriminator, the `misplaced`
regression test, the three CLI verdict tests) each kill a distinct mutant class (message
selection, per-reference grouping, exit predicate). I did not re-run `cargo mutants` (8 min, and
the gate re-runs it).

## 3. The three forced questions

**(a) Genuine red?** **Yes** — proven by the project's own runner, not by hand:
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → `run-verify.sh: PASS — red without the fix,
green with it.` The RED leg (production reverted, discriminator kept) fails **on behaviour, not
on compilation**, all four legs:

```
a_segmented_object_no_longer_stops_the_post_restore_pass
  → SegmentedMapUnsupported { operation: "restore::committed_chunks" }
an_unreadable_object_is_contained_and_the_run_is_not_certified            → same
an_unreadable_object_does_not_starve_the_objects_the_pass_could_read      → same
a_drain_over_an_incomplete_reference_set_names_the_blocking_record
  → "the blocker must be reported on wyrd.custodian.drain.audit … got: "   (empty audit trail)
test result: FAILED. 0 passed; 4 failed
```

`--classify` returns exactly one `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs`
(+ `CRATE crates/custodian`, `CRATE crates/server`), so the leg runs
`cargo test -p wyrd-custodian --test segmented_map_restore` as the brief predicted.

**(b) Production path?** **Yes.** Every leg calls the real exported entries —
`wyrd_custodian::reconcile_after_restore`, `wyrd_custodian::desired_state::reconciliation_status`
— and the CLI legs call the real `restore_verdict` that `cmd_custodian` itself calls. The only
doubles are the `MetadataStore` / `ChunkStore` *seams* (the trait boundary the custodian is
defined over, and the same doubles `restore_reconcile.rs` / `segmented_map_consumers.rs` already
use); nothing about the unit under test is re-implemented. The attribution legs read what the
production `tracing` callsites actually emitted, through a capturing subscriber — not a mock of
them.

**(c) Fixture includes the fault?** **Yes, and it asserts the fault is real.** `seed_damaged`
(`segmented_map_restore.rs:341-355`) seeds a committed segmented root naming two segments with
only the first written, then asserts `metadata::resolve_chunk_map(...).is_err()` — so a leg can
never pass because the fault quietly stopped being one. Criterion (2b) seeds that damaged object
**beside** the readable one that carries the genuine loss (nothing curated out), and `MemMeta` is
a `BTreeMap` so the damaged record (`inode:1`) is always the one the walk meets **first** —
otherwise "the readable object was still reported" could pass on an implementation that abandons
the walk at the first blocker. The containment legs also assert a *positive* observable (a stray
that would be marked on a complete set is not marked; the damaged object's own fragment is still
on disk and carries no `orphan:` record), never merely "no error was raised".

## 4. What I ruled out

- **Naming any new symbol in the discriminator.** The RED leg reverts production, so a reference
  to `RestoreReport::unresolvable` or `PendingUnresolvable` would degrade the red to "a symbol is
  missing". Every new-shape assertion ships in `restore_reconcile.rs` / `segmented_map_consumers.rs`
  / `cli.rs`'s test module, which `C4-ci` gates. The discriminator asserts the *attribution* on
  the audit seam instead (the shape #650's own fixture uses).
- **Pulling `crate::resolve` / `homed_objects` / `MaintenanceWalk` from the v5 salvage.** That is
  #681's module and does not exist on this base; it would re-create #681 inside this slice. I took
  only the *shape* from v5/v6 (the variant, the report field, the doc prose) and re-pointed every
  callsite at `gc::referenced_fragments` / `gc::object_name` / `metadata::resolve_chunk_map`.
- **Scoping the drain block to servers the unreadable object might name.** Which chunks it owns is
  exactly what could not be read — the block is cluster-wide, as `PendingMalformed` is.
- **Ranking `PendingUnresolvable` above `Pending`.** While a valid committed placement still names
  the server, the drain is honestly not converged and rebalance is moving it; "wait" is true and
  actionable there. The attributed answer takes over only when that wait would otherwise be
  unbounded.
- **Containing a genuine store fault.** Standing rejection (ii): only a record-level
  `ChunkMapError` is contained (`restore.rs:495-508`); anything else propagates, as #650's
  `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed` pins.
- **A caller-side timeout on any await.** Standing rejection (i) — the store implementation owns
  the network bound (`crates/traits/src/lib.rs:1000-1012`). All entries restated in
  `review-rejected.md` at this patch's own line numbers.

## 5. Verification run here (the gate re-runs everything)

- `cargo fmt --all -- --check` → clean (the target's commit hook runs rustfmt).
- `cargo clippy -p wyrd-custodian -p wyrd-server --all-targets -- -D warnings` → clean.
- `cargo test -p wyrd-custodian --test segmented_map_restore --test restore_reconcile --test
  segmented_map_consumers` → 4 + 16 + 8 passed, 0 failed.
- `cargo test -p wyrd-server --lib cli::tests` → 17 passed, 0 failed.
- `typos` (the registered external dep) over all eight touched files → exit 0.
- `run-verify.sh` → PASS (red→green), quoted above.
- `./engine/xtask.sh ci` → reaches the `cargo deny` step and fails there for the host reason in
  §0(b); everything before it passes.

## 6. Leftovers a reviewer may raise (and my answer)

- *"Two `inode:` scans in one pass."* The base has two as well (`referenced_fragments` +
  `committed_chunks`); this patch changes neither count. What it adds is a second **resolve** for
  a segmented object, bounded per object, in an operator one-shot with writers stopped. The
  single-read alternative is what merged distinct references and printed the hollow green above.
- *"The two reads could disagree."* Only under a concurrent writer, which the runbook forbids —
  and the disagreement is taken on the fail-safe side: `report.unresolvable` is the **union** of
  both reads (`restore.rs:550`), so a record either half could not read is still a record this run
  refuses to speak for. The mark gate remains driven by the reference set alone.
- *"`committed_chunks` duplicates `referenced_fragments`'s decode/resolve/contain shape."* True,
  and marked `deferred: #681` at `restore.rs:468` — the shared maintenance walk is that slice's,
  and per AGENTS.md's reviewer protocol a `// deferred: #N` marker settles it for review purposes.
  The two halves genuinely need different granularity (a fleet-wide protection set vs per-reference
  expectations), so folding them today would mean changing `ReferenceSet` — see §2.
