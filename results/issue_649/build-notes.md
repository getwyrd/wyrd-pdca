# Build notes — issue 649 (iteration 10)

*Withheld from the reviewer; written for the human at sign-off.*

## 0. What this iteration is

Iteration 9's patch cleared `C4-verify` (red→green), `C5-mutants` and every §6 item except
**one blocking T4 finding**, which the sign-off named as the whole reason to iterate:

> `crates/core/src/metadata.rs:2162` — the resolve-retry arbiter collapses an absent root into
> the same `false` result as a superseded generation, so a deletion observed on the final
> allowed retry attempt returns `MapResolutionUnstable` instead of `None`.

So this build is **iteration 9's patch plus that fix, plus the tests that bind it**. Nothing
else about the slice's shape changed: the resolver, the two read call sites, the gateway
wiring, the DST property and the docs paragraph are iteration 9's, which the brief's *Salvage*
clause explicitly permits (it names `iteration-v7/patch.diff` as the starting point; v9 is that
same lineage after two rounds of accepted narrowing, and the carry-forward directs me at the
one finding rather than at a re-derivation).

## 1. The defect, precisely

On the previous rendering (`iteration-v9/patch.diff`, `crates/core/src/metadata.rs:2157-2167`):

```rust
async fn root_still_names(...) -> Result<bool> {
    let Some(bytes) = store.get(root_key).await? else {
        return Ok(false);            // ← "gone" and "moved on" become the same answer
    };
    ...
}
```

`false` meant *"this generation is no longer named"*, and `retired_or` (`:2183-2198`) turned
that into `Ok(None)` = **restart**. For a *superseded* root that is right. For an *absent* root
it is wrong in a way that only shows on the last attempt: `resolve_current_chunk_map`
(`:2446-2469`) loops `MAX_RESOLVE_RESTARTS` times, and a drop on the final pass falls out of
the loop into `Err(MapResolutionUnstable)`. A plain `DELETE` racing a read therefore surfaced
as *"this object's map will not settle"* instead of *"there is no such object"* — an error the
store had already answered definitively, one attempt earlier, by returning `None` for the root.

Why it matters beyond a wrong error code (this is the C-1 axis the brief names): every consumer
this resolver is being built for reads `Ok(None)` as "no live committed generation" and a typed
error as "I could not determine what this object owns". The first is reclaimable, the second is
*reclaim-blocking* and, at a reader, a 500 rather than a 404. Collapsing the two hands the
maintenance passes of #650/#651 a permanent "unknown" for objects that are genuinely deleted —
which is exactly the "resolution is total" half of the invariant the brief asks to restore.

## 2. The fix

`crates/core/src/metadata.rs:2160-2254` (post-patch line numbers; `2154-2198` pre-patch):

* **`enum Resolution<T> { Answer(T), Superseded, Gone }`** (`:2160-2193`) — one type carrying
  *what* resolved or *why* it was dropped, replacing the `Option` whose `None` had two meanings.
  Threaded through `read_segments` (`:2388`), `resolve_snapshot` (`:2441`), `resolve_chunk_map`
  (`:2481`) and `resolve_current_chunk_map` (`:2509`).
* **`root_still_names` → `root_dropped<T>`** (`:2195-2217`): `Ok(None)` = the root still names
  this generation (so an anomaly under it is real, fail closed); `Some(Superseded)` = it names
  something else (restart); `Some(Gone)` = it is absent (terminal). It is generic in an answer
  type it holds no value of, so `Resolution::Answer` is *unrepresentable* there — the root
  re-read cannot accidentally be made to decide the map.
* **The two entries act on the difference**: `resolve_chunk_map` answers `Ok(None)` for `Gone`
  without the extra root `get` it used to spend restarting onto a root it had just seen absent
  (`:2486-2493`); `resolve_current_chunk_map` returns `Ok(None)` for `Gone` **inside** the loop
  (`:2528-2532`), so a delete never consumes a restart.
* Docs corrected where they asserted the old collapse: `ChunkMapError::MapResolutionUnstable`
  (`:558-571`) now states that only a supersede spends an attempt, and
  `docs/design/architecture/06-runtime-view.md:29` states three arms (superseded → restart,
  gone → no such object, still-named anomaly → fail closed) instead of two.

Net production change vs iteration 9: **+56 semantic lines in one file**, no signature change
outside the module (all four touched functions are private except the two `pub` entries, whose
signatures are unchanged), no new dependency, no behaviour change on any non-delete path.

## 3. Alternatives considered, with their cost

1. **Keep the collapse; special-case the loop tail.** After the loop, re-`get` the root and
   answer `Ok(None)` if absent. ~6 lines, and it *would* make the failing case pass — but it
   pays another round trip to re-ask a question already answered, and it leaves the shape
   defect intact: `resolve_chunk_map`'s snapshot entry would still route a known-deleted object
   through `resolve_current_chunk_map`, and the next consumer (#650/#651) would inherit the same
   two-meanings-one-`None`. The brief's *Invariant to restore* framing decides this: the target
   is the smallest change that restores the invariant, not the smallest diff. Rejected.
2. **Return `Ok(None)` from `retired_or` and let each caller re-read.** Zero new types, but it
   pushes the "retired or corrupt?" decision back out to three call sites — the exact
   single-arbiter property the module comment at `:2219-2233` exists to protect, and the thing
   0016 decision 7(h) is written against. Rejected.
3. **A dedicated `enum RootStanding { Names, Superseded, Absent }` alongside the existing
   `Option`s.** Honest, but the drop reason then has to be carried by hand through every stage
   that reshapes the answer: a 3-arm `match` in `read_segments`, in `resolve_snapshot` and at
   both entries — four hand-written mappings, ~5 lines each, against `Resolution<T>`'s single
   6-line `map()` plus the `match` the two entries need anyway. The line count is close; what
   decided it is that `map()` hands the closure *only the answer*, so a stage physically cannot
   rewrite *why* a resolution was dropped, whereas four hand-written mappings are four places
   the collapse this iteration is fixing could be reintroduced.
4. **Extend the DST property to the delete arm.** ~20 lines in `crates/dst/tests/custodian.rs`.
   Not done: the DST leg is explicitly *not* a Check discriminator for this slice (brief,
   *Verification posture* 1), the deterministic core test binds the arm exactly, and the brief's
   budget is already over on tests (§6 below). If the human wants it, it is a one-property add.

## 4. The test — and my own attempt to refute it

Both files stay base-visible-only (no symbol this patch adds is imported), so the `C4-verify`
RED leg still compiles. Added to `crates/core/tests/segmented_map_resolution.rs`:

* `a_delete_met_on_the_readers_last_attempt_is_no_such_object`
  (`crates/core/tests/segmented_map_resolution.rs:1088-1119`) — the binding case. It first
  **measures** the reader's restart budget by running the existing never-settles campaign and
  counting how many scripted retirements the reader actually consumed
  (`assert_retired_under_every_restart_refuses`, `:1034-1066`), then builds an interleaving of
  *budget − 1* overwrites followed by a **delete** on the attempt the reader has left. The
  budget is measured, never pinned: a pinned copy of `MAX_RESOLVE_RESTARTS` would silently stop
  testing the last attempt if the constant moved (and the constant is not base-visible, so it
  cannot be imported anyway).
* The two arms of `a_generation_retired_mid_resolve_...` (`:888-921`) now also assert **what the
  answer cost in root reads**: 3 for a supersede (re-read + the live root it restarts onto), 2
  for a delete (the re-read *is* the answer). The deleted arm is a second, independent red for
  the same fix — it binds the snapshot entry, where the loop is not involved at all.
* `every_anomaly_on_a_generation_the_root_still_names_fails_closed` (`:757-767`) now asserts a
  fail-closed refusal costs exactly the reader's constant (2 root reads) rather than the whole
  restart budget. This one is not a red→green discriminator; it kills the mutant that turns
  `retired_or`'s fail-closed arm into a restart, which would otherwise still end in *a* typed
  error and pass the old assertion.

**(a) Genuine red?** Yes — twice over, and against *two different baselines*:
  * against **iteration 9's production code** (the only change reverted being
    `crates/core/src/metadata.rs`): `a_delete_met_on_the_readers_last_attempt_is_no_such_object`
    fails with `called Result::unwrap() on an Err value: MapResolutionUnstable { attempts: 3 }`,
    and the deleted arm fails `left: 3, right: 2`. That is the finding, reproduced as an
    assertion. Re-applying the fixed file turns both green.
  * against the **slice base** (what `C4-verify` measures): 13/13 core cases and 2/2 server
    cases fail, then all pass with the patch — `run-verify.sh: PASS — red without the fix,
    green with it`.

**(b) Production path?** Yes. Every assertion is driven through `wyrd_core::read::read_object` /
`read_path` (and, in the server file, `ObjectGateway`), which call the production
`metadata::resolve_chunk_map`. The fake `MetadataStore` is a *store*, not a stand-in for the
resolver: it owns rows and answers `get`/`scan_page` from them, and it delegates to nothing.
Criterion 1 additionally runs over the real redb backend.

**(c) Fixture includes the fault?** Yes. The delete is *injected into the live interleaving* —
the store applies it at the moment the reader pages the `seg:` range of the generation it is
mid-way through, and it removes both the root and that generation's records, so no
old-generation answer survives for the reader to succeed with by accident. `pending_left() == 0`
is asserted so a fixture that never reached the delete cannot pass as proof, and
`assert_retired_under_every_restart_refuses` asserts the reader stopped of its own accord
(2 ≤ attempts < scripted), so the campaign cannot silently be the thing that ended the read.

## 5. Gate evidence (run in `$PDCA_WORKTREE`, base `6e7c255` = `origin/pdca-integration/main`)

| Check | Command | Result |
|---|---|---|
| C4-verify | `PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh` | **PASS** — green 13+2, red 13 failed |
| fmt | `cargo fmt --all -- --check` | clean (ran `cargo fmt --all` first; commit-hook ready) |
| clippy | `cargo clippy -p wyrd-core -p wyrd-server --all-targets -- -D warnings` | clean |
| tests | `cargo test -p wyrd-core -p wyrd-server` | all green |
| conformance | `./engine/xtask.sh conformance` | 5 valid + 6 invalid vectors pass |
| statics (ADR-0035) | `./engine/xtask.sh statics` | no DST-reachable shared mutable global state |
| DST sweep | `./engine/xtask.sh dst` | exit 0; `segmented_resolve_never_tears ... ok` across the seed sweep |
| C5 mutants | `./scripts/mutants-in-diff` | **0 missed** — "77 mutants tested in 2m: 16 caught, 61 unviable" (iteration 8 left 2 alive; iteration 9 killed them; the new code adds none) |
| C4-ci | `./engine/xtask.sh ci` | **red on the BASE** at `cargo deny check` — RUSTSEC-2026-0221 in the base lockfile, tracked as getwyrd/wyrd#673. Not this patch's, and `deny.toml:19-24` is a declared zero-tolerance wall, so it is neither suppressed nor bumped here. Because deny runs *before* conformance/statics/DST (`xtask/src/main.rs:1563-1567`), those three were run individually — rows above. |

## 6. Budget — honest measurement

Semantic (non-blank, non-comment) added lines, 7 files:

| file | semantic | brief's expected shape |
|---|---|---|
| `crates/core/src/metadata.rs` | 292 | ~350 ✓ |
| `crates/core/src/read.rs` | 28 | ~30 ✓ |
| `crates/server/src/lib.rs` | 27 | ~30 ✓ |
| `docs/design/architecture/06-runtime-view.md` | 1 | ~10 ✓ |
| `crates/dst/tests/custodian.rs` | 136 | ~100 |
| `crates/server/tests/segmented_object_read.rs` | 196 | ~130 |
| `crates/core/tests/segmented_map_resolution.rs` | 714 | ~350 |
| **total** | **1394** | ≤ ~1000 |

The **production** change is inside every per-file bound. The overage is entirely test and
fixture code, and it is the standing §6 item the previous sign-off explicitly deferred to this
round ("T2 Shape line-count overage … not adjudicated this round — revisit at the next sign-off
once this finding is fixed"). I did not prune to hit the number, and I want to be explicit about
why rather than quietly ship over budget: of the 714 lines in the core test, ~200 are the
self-contained fake `MetadataStore` the brief *mandates* (a delegating double was iteration 7's
rejected TEST-GAP), ~120 are fixtures shared by every case, and the remaining ~390 are 13 cases
each of which the brief's Success criterion names (1, 2a–2d, 3's arms, parsed-index ordering) or
which kills a specific C5 mutant (the at-ceiling case, the one-component group mismatch). This
iteration added ~120 of those lines. Splitting the file would not remove a line — it is the same
assertions in two files — so the only real reduction available is deleting cases, each of which
costs either a criterion or a live mutant. That trade is the human's to make at sign-off, which
is where the previous round left it.

## 7. C5 mutants

`./scripts/mutants-in-diff` on this bundle's `patch.diff`: **77 mutants tested in 2m — 16
caught, 61 unviable, 0 missed.** The two mutants iteration 8 left alive (`>`→`>=` at the segment
ceiling, `||`→`&&` at the group mismatch) stay killed by the cases iteration 9 added, which this
build keeps unchanged
(`a_segment_value_at_exactly_the_ceiling_still_resolves`,
`a_row_from_another_group_answered_inside_this_range_is_refused`). The new code's own mutants
are covered as follows, each by a *named* case rather than by hope:

* `root_dropped` body → `Ok(None)` ("always still named"): every retirement becomes fail-closed
  → `a_generation_retired_mid_resolve_...` (both arms).
* `root_dropped` → always `Gone` / always `Superseded`: criterion 1 stops reading bytes
  (`a_segmented_object_reads_byte_identical_to_its_flat_equivalent`) or ends in the give-up
  refusal.
* `==` → `!=` in `root_dropped`: a live generation looks dropped → criterion 1 red.
* `retired_or` → always `Superseded`: fail-closed becomes a restart campaign, which still ends
  in *a* typed error — this is the mutant the new root-read-count assertion in
  `every_anomaly_on_a_generation_the_root_still_names_fails_closed` exists to kill (2 root reads
  vs 8).
* `retired_or` → always `Gone`: a fault answers `Ok(None)` → `assert_fails_closed` fails.
* `Resolution::map` → drops the answer: criterion 1 red.

## 8. `review-rejected.md`

Splitting `root_still_names` moved every line below it in `crates/core/src/metadata.rs`, which
would have stranded the recorded rejections (the gate matches `file:line` exactly). I
re-anchored them: a new section, *Iteration-10 line numbers*, carries the **same** two settled
classes — the byte-materialisation bound (#674) and the caller-side timeout/deadline — mapped
line-for-line onto this rendering by diffing v9's file against this one; the earlier renderings
are kept, per the file's own convention. No class was added and no coverage widened: every row
is an existing decision at the line its own code now occupies, and each one was verified to
point at the code it claims (49 distinct locations, all checked). The blocking finding itself is
**fixed, not recorded** — it was a real correctness gap.

## 9. Not done, deliberately

* `crates/custodian/src/resolve.rs`, GC/scrub adoption, the committer — out of scope per the
  brief (#650/#651/#653).
* The byte-materialisation bound — settled at Plan, tracked as **getwyrd/wyrd#674**; recorded,
  not re-litigated, and no mechanism was grown inside the resolver to chase it.
* The base lockfile advisory — **getwyrd/wyrd#673**; `Cargo.lock` and `deny.toml` untouched.
* No PR was pushed, opened, or marked ready.
