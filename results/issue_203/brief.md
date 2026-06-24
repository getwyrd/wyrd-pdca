# Brief — issue 203 / fschunkstore-unique-temp-per-write

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** fschunkstore-unique-temp-per-write
- **Defect:** `FsChunkStore::put_fragment` writes to a temp file whose name is keyed on
  the `FragmentId` **alone** (`crates/chunkstore-fs/src/lib.rs:44-49`,
  `{chunk}/{index}.tmp`), then atomically renames it onto the final path. The temp path
  is not unique per call, so two concurrent writes of the **same** `FragmentId` race on
  the same `.tmp` file (`put_fragment` at `:71-85`: `fs::write(&temp, …)` then
  `fs::rename(&temp, final_path)`). Interleaved `fs::write`s can clobber the shared temp,
  and the second `fs::rename` can hit `NotFound` and **spuriously error a concurrent
  duplicate/repair write**. Reached because the d-server accepts arbitrary concurrent
  `PutFragment` over an `Arc<store>` with no serialization at the gRPC/service layer
  (`crates/chunkstore-grpc/src/server.rs:33-36`, `:46-60`); same-id collisions arise from
  idempotent client retries and from repair/reconstruction writing a fragment a
  foreground write (or another repair pass) is also writing — including the in-process
  single-binary profile where gateway and custodian share the store
  (`ChunkStoreService::from_arc`). Mitigated, not corrupting (same id ⇒ same
  checksum-verified EC bytes; the publish is an atomic rename; reads re-verify), but a
  real robustness corner that spuriously fails legitimate concurrent writes.
- **Success criterion:** N concurrent `put_fragment` calls for the **same** `FragmentId`
  all complete **successfully** (no spurious `NotFound`/clobber error), and the resulting
  `<index>.frag` is a complete file that verifies; `list_fragments` continues to ignore
  any temp scratch files (it accepts only names ending exactly `.frag`,
  `parse_fragment_file_name` at `:221-223`). Demonstrable at C4-verify by a flippable
  concurrency test that spawns many same-id writes and asserts every one is `Ok` and the
  final fragment verifies. BINDING is "concurrent same-id writes no longer race / no
  spurious error, atomic publish preserved"; *how* per-write privacy is achieved (an
  atomic counter + pid in the temp name, the `tempfile` crate's `NamedTempFile` +
  `.persist`, or another private-scratch scheme) is ILLUSTRATIVE — Do's call.
- **Invariant to restore:** Two concurrent writes of the same `FragmentId` against the
  filesystem store must each use **private scratch** and publish via an **atomic rename**,
  so neither write can observe or clobber the other's partial bytes and neither fails
  spuriously — the atomic rename is the sole serialization point, last-writer-wins is a
  semantic no-op (same id ⇒ identical bytes), and concurrent writes of *different*
  fragments stay independent. Source: POSIX `rename(2)` atomicity / same-filesystem
  atomic-replace guarantee (Tier A platform canon, authoritative) — the write idiom the
  store already intends ("write-then-rename so a crash never leaves a half-written
  fragment visible", `lib.rs:42-43`) but breaks by sharing the scratch path. (Structural
  fix — object/scratch-file lifetime per principles.md §1.2; target is the smallest
  change that restores the invariant, not a per-fragment mutex map. Self-test: the
  invariant is over *concurrent same-id writes*, not one call site, so it is not
  satisfiable by guarding a single path.)
- **Repo + branch target:** getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- **Conflicts with:** none   (this brief is the BASE of the `crates/chunkstore-fs/src/lib.rs`
  merge chain; the two other briefs that edit that file declare a merged-dependency on this
  one, so neither co-schedules with it — run this first)
- **Surfaces:** data

> **Merge chain — `crates/chunkstore-fs/src/lib.rs`: 203 → 207 → 204.** Three briefs edit
> this one file, so they are serialized by `Depends on (merged)` and each Do builds on the
> prior's **merged** file state (worktrees are cut from `origin/main`, so an unserialized
> later brief would build against a pre-merge tree and collide at merge). **203** (this
> brief — unique temp in `put_fragment` + reaping) is the base, no prerequisite; **207**
> (corruption-error contract in `get_fragment`/`verify`) stacks on 203's merged result;
> **204** (offload all blocking I/O off the reactor) lands last, wrapping the finalized
> bodies. Under 4 lanes, 203 runs in the first wave alongside the independent briefs
> (197, 198, 205); 207 then 204 are released as their predecessors' PRs merge.
- **Scope:** make `put_fragment`'s temp scratch private per write so concurrent same-id
  writes no longer race, with the atomic rename as the publish point; and reap stale temp
  scratch left by a crash (unique temps replace the old self-cleaning single-`.tmp`
  behaviour, so absent reaping they accumulate as litter) — leave *where* the reap runs
  to Do. / out of scope: `fsync` durability of the temp/parent dir (an acknowledged
  fragment can still be lost on power failure — a separate **durability** concern the
  issue explicitly flags for separate filing); offloading the blocking `std::fs` calls
  off the reactor (#204); the per-fragment serialization-via-mutex alternative (rejected:
  in-process-only, adds machinery, gives no crash-safety the rename doesn't already).
- **Repro instruction:** On `main` @ `c2223a5`, from a test against a temp-dir
  `FsChunkStore`, launch many concurrent `put_fragment` calls for one fixed `FragmentId`
  with the same valid fragment bytes. Observe that some calls return an error (the second
  `fs::rename` racing on the shared `{index}.tmp` can fail `NotFound`, or an interleaved
  `fs::write` clobbers the in-flight temp), rather than all succeeding.
- **Test file:** crates/chunkstore-fs/tests/concurrent_put.rs   (net-new; N concurrent
  same-id writes all `Ok` + final fragment verifies + `list_fragments` ignores temps —
  see Verification posture for the red-state note)
- **Verification posture:** the regression is a concurrency stress assertion. Its post-fix
  green (all writes `Ok`) is deterministic; its pre-fix red (at least one spurious error)
  depends on interleaving and can be intermittent on a fast/quiet machine. Do should make
  the red robust — e.g. enough concurrent writers / iterations, or a seam that forces the
  write→rename interleave — and, if a reliably-flipping timing red proves impractical at
  C4-verify, record that (per CLAUDE.md "no test because X") plus a manual repro and rest
  the regression on the deterministic post-fix invariant (private scratch + atomic
  rename, asserted structurally). This is a net-new concurrency test, not a flip of a
  prior failing assertion.
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched `crates/chunkstore-fs/src/lib.rs` across
  merged history (`4cd77d2` list/delete, `093732d` placement, `f428ec7` fragment-id
  evolution, `85` M0.4 fs store — none made the temp path unique), open PRs (`gh pr list
  --state open` — none touch this file), and closed PRs — no prior or in-flight fix for
  this race.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on §6.1 (C4 gating CI red). The gating `cargo xtask ci` failed on the madsim DST tier (exit 101) WITH the patch applied. Decisive test run at sign-off: the madsim DST tier on bare `main @ c2223a5` (no patch) is reliably GREEN — 3/3 runs exit 0, all tests pass (concurrency, custodian x7, network x5), deterministic timings, not seed-flaky. So the DST failure is attributable to the patch, not pre-existing/environmental. §6.1 cannot be cleared and the bundle cannot be accepted with a red gating gate. The same-id race fix and the per-fix red->green (C4-verify) look sound; the problem is the DST-tier interaction. The reviewer (artifact-only) saw no mechanism, but `wyrd-dst` does compile in the patched `FsChunkStore` (concurrency.rs / network.rs exercise it). What to root-cause / change next: - Determine why the patched FsChunkStore flips the madsim DST tier red. Prime suspects, all newly compiled into wyrd-dst under `--cfg madsim`: the new `reap_stale_temps()` fs::read_dir traversal that now runs at every `open`; `std::process::id()` and the process-global `AtomicU64` (TEMP_SEQ) in `temp_path`. Any of these can perturb madsim's deterministic schedule/fs. - Reproduce under madsim, fix the regression (or make the new scratch/reap logic simulation-safe) so `cargo xtask ci` passes, then re-verify the same-id red->green still holds. - While iterating, also address the still-open advisory items: §6.2 confirm/scope the one-opener-per-root assumption of reap_stale_temps (matches `.tmp` by suffix, not pid), and §6.3 the regression guard rests on dynamic interleaving (64x16) rather than a structural assertion of per-write scratch uniqueness.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: madsim DST tests failed with exit status: 101
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
