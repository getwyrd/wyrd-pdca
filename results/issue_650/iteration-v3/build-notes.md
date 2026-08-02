# Build notes — issue 650 / gc-scrub-through-resolver-fail-closed-containment

**Iteration 3.** Target branch `pdca-integration/main` (`stack-base`), base `4e78aeb`
(= `origin/pdca-integration/main`, which already carries #648 and #649). Every `path:line`
below is on the patched worktree `$PDCA_WORKTREE` unless it says "base".

This round **keeps** iteration 2's design (it passed C1/C2/C4/T1/T2 and the whole
`cargo xtask ci`) and fixes the four implementation-level findings the carry-forward
carried, at the **cause** each time rather than at the symptom. Nothing rejected last round
is re-attempted: the caller-side timeout stays rejected (standing rejection (i)), and
`protects`' blanket short-circuit stays the settled reclamation rule (iv).

## What the carry-forward asked for, and where each answer landed

| Carry-forward item | Answer |
|---|---|
| **C5 [impl]** — a driven valid-root→unreadable-root race returned `Err(Store(...))` because `crates/core/src/metadata.rs:2517` (base) emits a raw decode error that GC propagates instead of attributing to the object | Fixed **at the cause**: the resolver now types every fault it raises about an object's own bytes. `crates/core/src/metadata.rs:2405` (`decode_root_record`) + `ChunkMapError::RootRecordUndecodable` (`:575`), used at **both** root re-reads — the settle-read `:2244` and the restart-read `:2572`. Bound by `crates/custodian/tests/segmented_map_consumers.rs:919` (two arms, one per site; the shared body at `:953`) |
| **T5 [impl]** — the reverse-order aggregation leg's "converging" GC actually returned `Satisfied`, so it only ever proved `Satisfied`→refusal | `crates/custodian/tests/segmented_map_consumers.rs:1256-1278` — the converging store now carries two independent pieces of collectable garbage: an explicit fixture check asserts that GC context alone answers `Reconciled::Changed` (`:1273`), and `:1310` asserts the second piece was reclaimed **inside the two-loop step** |
| **T4 / review `gc.rs:445` ×3** — `String::from_utf8_lossy` is not injective, so two damaged keys collapse into one blocker | `crates/custodian/src/gc.rs:294` — the blocker map is keyed by the record's **raw bytes** (`BTreeMap<Vec<u8>, String>`), and `object_name` (`:461`) escapes rather than replaces. Bound by `crates/custodian/tests/segmented_map_consumers.rs:1028`, which counts **distinct** names in the audit trail and in the drain answer over three keys that collide under two different non-injective renderings |
| **T4 / review `dst:1525`** — the segmented object's fragments had no deletion evidence, so omitting them from the reference set would not have deleted them | `crates/dst/tests/custodian.rs:1632` — every fragment of the generation that is live *while the pass runs* gets a lapsed grace record, so protection is load-bearing on all three arms |
| **T4 / review `dst:1585` ×2** — the retirement arm resolved its expectation **before** the root flip, i.e. against the RETIRED generation, and never made successor fragments reclamation-eligible | `crates/dst/tests/custodian.rs:1622` — the expectation is now the **successor** (`fragments_of(&plan.chunk_refs())`) on arm 1, and those successor fragments carry the lapsed grace records |
| **T4 Contribution / deferred C3** — reviewer could not reproduce `scripts/review-branch` / `scripts/pdca` (absent from artifact-only inputs); C3 asks Plan whether the drain-status surface belongs here | Not mine to answer: both are sign-off items. The C3 subject (`desired_state.rs`) is unchanged from iteration 2 and is deferred in `deferred-findings.json` |

## The change (12 files, 1,293 semantic added lines — 137 production, 1,156 tests)

* **`crates/core/src/metadata.rs`** (+16 semantic) — the resolver's **typed-verdict contract**
  gains its missing case. `ChunkMapError::RootRecordUndecodable` (`:575`) and the private
  `decode_root_record` (`:2405`), a direct copy of the shape `decode_segment_record` (`:2427`)
  already uses for the *other* record a resolve reads. Both root re-reads use it: `root_dropped`
  (`:2244`) and `resolve_current_chunk_map` (`:2572`).
* **`crates/custodian/src/gc.rs`** (+64) — the reference build resolves each committed record
  through `metadata::resolve_chunk_map` (`:393`); `ReferenceSet::unresolvable`
  (`:294`, byte-keyed) carries what it could not read; `protection`/`protects` (`:306`, `:331`)
  answer *why* a fragment is withheld; `reconcile` answers `Reconciled::Blocked` over an
  incomplete set (`:248`) and attributes each blocker before the fleet walk (`:164-166`).
  `object_name` (`:461`) is the injective name.
* **`crates/custodian/src/scrub.rs`** (+16) — the same set, its own attribution (`:115`), the
  same answer for the same condition (`:210`).
* **`crates/custodian/src/reconciliation.rs`** (+30) — `Reconciled::Blocked` (`:44`) and
  `least_certified` (`:55`), folded through every loop in `reconcile_step` (`:118`…`:139`).
* **`crates/custodian/src/desired_state.rs`** (+11) — `PendingUnresolvable` (`:124`, returned
  at `:213`), named through the same `object_name`.
* **Tests** — the added discriminator file (8 legs), plus the positive-variant legs it cannot
  carry: `tests/gc.rs:812`, `tests/scrub.rs:1033`, `tests/rebalance.rs:1518`, and the seeded
  Tier-0 DST property `crates/dst/tests/custodian.rs:1550`.
* **`docs/design/architecture/06-runtime-view.md:29-31`** — the containment sentences the brief
  names, plus one clause for the typed-root rule this patch lands.
* **`Cargo.lock`** — iteration 2's advisory bump (`event-listener` 5.4.2) retained; that is what
  makes `cargo deny check` green.

## What I ruled out, with the cost

**C5: a caller-side probe in `gc.rs` instead of typing the fault in the resolver — rejected,
and here is the diff it would have been.** GC could have kept the resolver untouched and, on
the non-`ChunkMapError` arm, re-read the root itself to decide whose fault it was:

```rust
// NOT taken — ~12 lines in crates/custodian/src/gc.rs, replacing `Err(err) => return Err(err)`
Err(err) => match meta.get(&key).await {              // +1 store read per fault
    Ok(Some(bytes)) if metadata::decode::<InodeRecord>(&bytes).is_err() => {
        unresolvable.insert(key.clone(), err.to_string());   // contain
        continue;
    }
    _ => return Err(err),                             // propagate
},
```

12 lines against the 16 the cause fix costs — so it is **not** cheaper, and it is worse three
ways: (a) it is a *probe*, racing the same writer the fault came from, so a third write
between the resolver's read and this one flips the verdict; (b) it adds a store read on the
error path, where the store may be exactly what is failing; (c) it fixes nothing for the read
path (`crates/core/src/read.rs`), which meets the identical untyped error. The resolver is the
component that *knows* which record it could not parse, and every other object-local fault it
raises is already typed — `SegmentRecordUndecodable`, `SegmentAbsent`, `SegmentBoundsMismatch`.
The hole was the root record; it is now closed at the same seam, for both re-read sites.

**Scoping the core edit to only `resolve_current_chunk_map` (the line the finding named) —
rejected.** `root_dropped` (`:2244`) decodes a re-read root too, and a garbage root arriving
there propagates identically. Fixing only the cited line would have left half the defect: the
new test's arm 1 fails with `root_dropped` untyped, arm 2 with `resolve_current_chunk_map`
untyped (both demonstrated below). Restoring the invariant, not minimising the diff.

**Rendering non-UTF-8 keys as `\u{FFFD}` and disambiguating with a counter — rejected.** A
counter names nothing an operator can `get`. The escape costs 10 lines, is injective by
construction (`\` is the only escape lead-in and is itself doubled), and leaves the ordinary
`inode:1` byte-identical — which is why every existing assertion on `"inode:1"` and
`PendingUnresolvable { objects: vec!["inode:9"] }` still passes unchanged.

**Making the DST arms assert against a fixture restatement — rejected.** Arm 0/2 still take
the expectation from the production resolver's own answer (`live_fragments`,
`crates/dst/tests/custodian.rs:1400`). Only arm 1 uses the plan directly, because the
generation it must protect does not exist in the store until the flip lands *inside* the pass —
which is precisely the finding: resolving "expected" before the flip expects the RETIRED
generation.

**Everything iteration 2 ruled out still stands** (lenient `state` peek for undecodable
records: 2 dependency lines + ~8, or a new public core API — both breach the ADR-0010 boundary
`gc.rs:34-35` states; per-object-scoped freeze: impossible, the chunk ids are what the
unreadable map withholds; restore untouched, it inherits containment through `protects`).

## Refutation (the three required questions)

**(a) Genuine red?** Yes — through the project's own per-fix runner, not by hand:
`PDCA_BUNDLE=… PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh`
→ `PASS — red without the fix, green with it`. GREEN leg: **8 passed**. RED leg (production
reverted, added test kept): **0 passed, 8 failed** — every one an assertion/`expect` panic
(`Store(SegmentedMapUnsupported { operation: "gc::referenced_fragments" })`), **zero** `error[`
lines in the log, so no compile-red scored as a pass.

Four *targeted* mutations, each re-run through `cargo test`, because a whole-patch revert does
not prove the NEW legs bind:

| Mutation (production only) | Result |
|---|---|
| `root_dropped` + `resolve_current_chunk_map` back to raw `decode(&bytes)?` | `a_root_rewritten_unreadable_under_the_resolve_is_contained_not_propagated` **FAILED** — `[settle-read] … ONE object's fault: Store(Error("EOF while parsing a value"…))` |
| only `resolve_current_chunk_map` back to raw `decode` (the line the finding named) | same leg **FAILED** on `[restart-read]` — so each arm binds its own site |
| `object_name` back to `String::from_utf8_lossy` | `two_blockers_whose_keys_are_not_utf8_are_each_attributed_under_their_own_name` **FAILED**, `left: 1  right: 2` |
| `resolve_chunk_map`'s `Superseded` arm → `Ok(None)` (a build that never restarts onto the live root) | DST **FAILED**: *"a fragment the live generation's chunk map references is NEVER reclaimed, even carrying a lapsed grace record (arm 1, server 0)"* — exactly the finding, on the arm that could not see it before |
| reference build skips segmented objects (`Ok(Some(_)) => continue`) | criterion (1) **FAILED** on `"segment 0's chunk must survive GC"` — i.e. on a **real deletion**, which is what the added grace records bought |
| `object_name`'s `b'\\'` arm deleted (a rendering that escapes the invalid byte but passes `\` through) | the injectivity leg **FAILED**, `left: 2  right: 3` — the mutant the advisory gate found (below) |

**The advisory mutants gate earned its keep this round.** The first
`scripts/mutants-in-diff` run reported `MISSED crates/custodian/src/gc.rs:465: delete match
arm b'\\' in object_name`: my two-key fixture proved the escape beat `from_utf8_lossy` but not
that the escape is itself injective — a key carrying a literal `\` collides with an escaped
one. I added the third record (`br"inode:\xff"`, `segmented_map_consumers.rs:1042`) and
re-ran: **30 mutants, 6 caught, 24 unviable, 0 missed.**

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the fenced
control point — with real `GcContext`/`ScrubContext` over in-memory implementations of the
*actual* `MetadataStore`/`ChunkStore` traits. The resolver inside `referenced_fragments` is the
real `wyrd_core::metadata::resolve_chunk_map`; the drain leg calls the real
`desired_state::reconciliation_status`; the audit assertions read what the production
`tracing` callsites emitted, through a subscriber, with no test-only hook. The rewritten-root
double (`segmented_map_consumers.rs:214`) only *answers reads* — every decision is production's.

**(c) Fixture includes the fault?** Yes, and each fixture proves its own fault is real:

* `seed_damaged` (`:441`) re-reads the seeded root and requires
  `metadata::resolve_chunk_map(..).await.is_err()` before any leg asserts anything;
* the rewritten-root leg asserts `meta.unspent() == 0` (`:1000`) — the scripted rewrites were
  actually consumed, so the race genuinely happened — and asserts the attribution names the
  *root* decode (`:1013`), not some other anomaly;
* the damaged object is met **first** (`inode:1`, `BTreeMap` order), never curated out; the
  healthy object sits in the same store and the same pass;
* the store-fault leg injects a real `std::io::Error` at the `scan_page` seam and asserts that
  exact text came back;
* criterion (1) and the DST property now put **lapsed grace records on the very fragments they
  claim survive**, so "still on disk" is an observation about the reference set rather than
  about GC having no deadline to act on.

## Gates run here (the project's own runners, never hand-rolled)

* `./engine/xtask.sh ci` — **exit 0**, `xtask ci: all checks passed`. Includes `typos`,
  `render_site.py --check` (link audit OK), `cargo fmt --all -- --check`, clippy `-D warnings`
  (workspace **and** `-p wyrd-dst --cfg madsim`), the workspace test run, **`cargo deny check`
  → `advisories ok, bans ok, licenses ok, sources ok`** (iteration 1's red), machete,
  conformance vectors, statics, gitlink/unsafe/deploy guards, and the 50-seed DST sweep.
* `./engine/scripts/run-verify.sh` — **PASS** (red→green); `--classify` confirms the single
  discriminator `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`.
* `cargo fmt --all` was run over every touched file (commit-hook readiness); one rustfmt
  finding in `crates/dst/tests/custodian.rs` was fixed before the patch was cut.
* `cargo doc -p wyrd-core --no-deps` — checked because the patch adds a **public** error
  variant with doc links. My links resolve; the file's pre-existing
  `private-intra-doc-links` errors (`InodeRecord` → `InodeRecordWire`, `create` →
  `checked_for_publication`, …) are on base and untouched. The first draft added one more
  (`RootRecordUndecodable` → the private `root_dropped`); it is now a plain prose mention.
* `scripts/mutants-in-diff` (advisory C5) — **30 mutants: 6 caught, 24 unviable, 0 missed**
  (the one miss it found first is described under (a) above and is now caught).

## Budget

**12 files** (≤ 15) and **1,293** semantic added lines (non-blank, non-comment) against
≤ ~1,500. Production is **137** of them (`gc.rs` +64, `reconciliation.rs` +30, `scrub.rs` +16,
`core/metadata.rs` +16, `desired_state.rs` +11); the other 1,156 are tests and the DST
property, which is where the brief said the budget risk was. The anticipated mechanical
`match`-arm migration over the new `Reconciled` variant is still **empty in this tree**
(`cargo build --workspace --tests` clean — every consumer compares by `==`/`assert_eq!`), and
the same holds for the new `ChunkMapError` variant: `Display` is the only exhaustive match over
it in the workspace.

## Scope note for the human (not a defect)

`crates/core/src/metadata.rs` is a file the brief's **Scope** paragraph does not list. It is
edited deliberately and minimally (16 semantic lines, one private helper + one error variant)
because that is where the C5 defect's **cause** lives — the alternative was a 12-line racing
probe in `gc.rs` that leaves `crates/core/src/read.rs` meeting the same untyped error. No
behaviour of the read path changes: an undecodable root failed that read before and fails it
now, with a better-typed error.

## External dependencies

Both registered ones were present and ran for real on this host: `typos` (clean) and the docs
renderer (`markdown_it`/`yaml` → `render_site --check` OK). Nothing beyond the base Rust
toolchain; no Docker, no protoc, no live backend, no new dev-dependency. **No NEEDS-HUMAN
external dependency for this bundle.**

## Scratch

`/var/tmp/pdca/pdca-builder-650-*` (three gate logs, two file backups) — removed at handover.
