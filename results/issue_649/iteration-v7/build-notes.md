# Build notes — issue 649 (iteration 7)

> Withheld from the reviewer; written for the human at sign-off.
> All `path:line` citations are against the patched worktree
> `/home/eddie/development/wyrd/wyrd.pdca-wt` at base `6e7c255` (= `origin/pdca-integration/main`,
> i.e. `origin/main` `9120f7a` + slice #648). `patch.diff` was re-checked with `git apply --check`
> against a pristine `6e7c255` checkout.

---

## 1. The two grounds the human rejected iteration 6 on — what changed

> **1. Unresolved BUG finding** (T4 batch review, `crates/core/src/metadata.rs:2248`): segment
> values are decoded without enforcing `MAX_VALUE_BYTES` or a per-segment chunk-size ceiling, so
> a structurally-valid but oversized segment record can force unbounded allocation despite
> `MAX_ROOT_SEGMENTS` bounding the count. Must be fixed (add the size ceiling) or explicitly
> recorded-rejected with justification.

**Fixed, not rejected.** The resolver now refuses a `seg:` row whose stored value is past
[`MAX_VALUE_BYTES`] **before the row is kept and therefore before anything decodes it**
(`crates/core/src/metadata.rs:2294`), as a new typed variant
`ChunkMapError::SegmentValueOverCeiling { index, bytes, ceiling }`
(`crates/core/src/metadata.rs:540`, Display at `:661`). §2 is the design argument; §3 is what
was rejected and why; §4 has the red→green refutation for both new tests.

> **2. Unscoped Cargo.lock change:** event-listener bumped 5.4.1→5.4.2 with no manifest change …
> Drop this lockfile edit from the patch.

**Not present, and verifiable in one command.** The lockfile edit was already dropped in
iteration 6 (it was iteration 5's), and this patch touches no manifest and no lockfile:

```
$ grep -c '^diff --git' results/issue_649/patch.diff      # 19 files
$ grep -n 'Cargo.lock\|^diff --git.*Cargo.toml' results/issue_649/patch.diff
(nothing — the single `Cargo.toml` hit in the file is a citation inside a code comment)
```

`Cargo.lock` in the worktree is byte-identical to the base (`git status` lists it nowhere), so
`event-listener` is still `5.4.1` at `Cargo.lock:1205` — untouched by this slice, per the human's
own note that the base advisory is out of scope here and tracked separately.

> Note (from the same sign-off): the pre-existing base-tree **RUSTSEC-2026-0221** advisory is
> accepted as out of scope for this fix and tracked in a separate issue — not part of the
> rejection.

That decision is carried, not re-litigated. It does mean the gating `C4-ci` row is still red at
`cargo deny check` on a tree whose every other step is green, and because `cargo_deny_check()`
runs before `run_conformance()` / `run_statics()` / `run_dst()` (`xtask/src/main.rs:1562-1567`),
a red `deny` also stops `ci` reaching the DST tier this slice's verification posture names. I ran
those three steps individually instead — all green, including the 50-seed
`segmented_resolve_never_tears` (§5).

**NEEDS-HUMAN external dependency: RUSTSEC-2026-0221 (event-listener 5.4.1 in the BASE lockfile) — blocks the gating `C4-ci` (`cargo deny check`) on the base tree independently of this patch; already adjudicated out-of-scope at the iteration-6 sign-off (SUMMARY §10), repeated here only because the gate row is still red and C6 will ask again.**

Registration that would have caught it before a cycle burned on it (unchanged from iteration 6):

```toml
[[doctor.checks]]
id    = "deny-clean-base"   # the token Plan should have put in `External dependencies`
cmd   = "cd ${PDCA_WORKTREE:-../wyrd} && cargo deny check advisories"
hint   = "The base tree carries an unresolved RUSTSEC advisory. Land the dependency bump on its own (dependabot, or `cargo update -p event-listener --precise 5.4.2`) before running a cycle — a feature slice cannot make `cargo xtask ci` green while the base lock is red."
level = "MISSING"
```

One line makes it green if you want it green before sign-off, run in `$PDCA_WORKTREE` **outside
this patch**: `cargo update -p event-listener --precise 5.4.2`.

---

## 2. The fix — why a per-record value ceiling, and why *there*

The finding is real and it is squarely this slice's invariant. The brief's C-1 bullet reads: *"The
work a record can demand of a reader is bounded by the reader, not the record. A root's own table
may not set the budget spent on its behalf."* Iteration 6 bounded **how many** rows a root can
make a reader hold (`accounted > MAX_ROOT_SEGMENTS`, refused unread,
`crates/core/src/metadata.rs:2238`) and left **how large each row is** entirely to the record. The
product of the two is the reader's real cost, so half a bound is not the invariant.

**Where the check goes: at the point the row is kept, not at `decode`.** The finding named
`decode_segment_record`, but a ceiling there would already be too late: `read_group_range` collects
every row of the range into `rows` and `read_segments` decodes them afterwards
(`crates/core/src/metadata.rs:2342`), so up to `accounted` oversized values would be *retained*
before the first decode ran. The check therefore sits in the walk, immediately after the
"index the table does not name" check and immediately before `rows.insert`
(`crates/core/src/metadata.rs:2294`) — the same structural position, for the same stated reason
("Checked BEFORE the row is kept, so the range's size can never become the caller's memory cost",
`:2272-2274`). With both ceilings in place the resolve holds at most
`MAX_ROOT_SEGMENTS × MAX_VALUE_BYTES` (512 × 100 000 ≈ 51 MB), two numbers the reader sets;
`decode_segment_record`'s doc now records that its input is bounded and by whom (`:2319-2321`).

**Why `MAX_VALUE_BYTES` and not a new knob.** `V = 100 000` is the value ceiling every backend
inherits (FoundationDB's, the tightest in play — `crates/traits/src/lib.rs:994-997`), so a larger
value is one no conforming writer could have stored at all. It is also exactly **twice** the
`V / 2` a *publishable* segment value is bounded by (`0016:1467`, the `MAX_SEG_CHUNKS` row: `same
rule against a seg: record`), so the ceiling cannot refuse a record a conforming publication could
have written — there is a 2× margin before it bites. Inventing a per-segment **chunk-count** knob
(`MAX_SEG_CHUNKS`) was the other option the finding offered; I did not take it, because that knob
is a capacity constant whose value 0016 assigns to a different slice and whose enforcement point
is the *writer* (the salvage's `check_record_ceilings` / `check_value_ceiling`, which are #653's
committer and already use `MAX_VALUE_BYTES` for exactly these records — `sources/salvage.diff:4192`,
`:4215`). Read side and write side now name the same number.

**Why the refusal is *described*, not raised.** It returns `Ok(GroupRange::Anomaly(…))` like every
other shape, so the one arbiter (`retired_or`, `crates/core/src/metadata.rs:2156`) decides
retirement-vs-fault. Raising it where it is noticed would turn an ordinary overwrite into a
permanent read failure on an object whose live generation is perfectly fine — the exact C-1 arm
decision 7(h) exists for. That is not a hypothetical: mutating the `Ok(Anomaly)` into an `Err`
turns `an_over_ceiling_value_in_a_retired_generation_restarts_onto_the_live_one` red (§4).

**Why fail-closed at all, given C-1 forbids permanent read failure.** Refusing an object is a
loss; decoding an arbitrarily large value is a *worse* loss, and a differently-scoped one. A
resolver that OOMs takes the whole process down and with it every **other** object's read — a
store-wide availability loss where the brief permits only "fail-closed **scoped to the object that
failed**". The refusal keeps the blast radius at one object, and no object a conforming publisher
could have written can be in that set (the 2× margin above). This reasoning is recorded at the
variant (`crates/core/src/metadata.rs:523-539`) so the next reader does not have to re-derive it.

**Not bounded here, and deliberately:** the bytes one *page* costs before this code sees it.
`MetadataStore::scan_page` is bounded in **rows**, never in bytes
(`crates/traits/src/lib.rs:1067-1074`), so a backend that materialises a huge value has already
done so by the time the loop runs; that is the store's own budget, exactly as the network bound is
(standing rejection (i)). The doc says so rather than implying a bound this module cannot give
(`crates/core/src/metadata.rs:2220-2226`). Likewise the **root** value is not ceilinged: a flat
root may legitimately be large today on a backend without a value limit, and refusing those would
make existing objects unreadable — the very failure mode this fix is avoiding. Segment records
carry no such legacy population (nothing publishes a segmented map yet — this slice lands no
producer), which is what makes the read-side ceiling free of compatibility risk.

**Docs currency:** `docs/design/architecture/06-runtime-view.md:29` gains one clause in the
resolver paragraph the brief scopes ("…as is a segment record whose value is past the ceiling any
backend would have stored — how much work a record can demand of a reader is the reader's number,
not the record's"). No containment or staged-publication sentences (those are #650/#651/#653).

---

## 3. Alternatives ruled out, with their costs

Carried from iteration 6 (unchanged, all still hold): keeping a `read_object_from(&record)`
convenience wrapper (would keep one consumer opaque to the segmented shape — the thing 0016 7(e)
forbids; cost of *not* keeping it is the 205-line declared mechanical migration); clamping an
over-ceiling table and reading its first 512 segments (1 line cheaper, and is the quiet
under-approximation C-1 names); judging each anomaly where it is noticed (~18 lines cheaper, turns
an overwrite into a hard failure); resolving without the post-range root re-check (saves one `get`,
loses the torn-generation guarantee); a caller-side timeout on the store awaits (standing rejection
(i), recorded in `review-rejected.md`).

New this iteration:

- **Put the ceiling in `decode_segment_record`, where the finding pointed.** 4 lines instead of
  10 (no `GroupRange::Anomaly` construction, no arbiter round trip). Rejected: the rows are
  already **retained** by then (`read_group_range` fills `rows`, `read_segments` decodes after the
  walk ends), so it would bound the decode's allocation but not the walk's — the OOM the finding
  describes is reachable with `accounted` large values held before a single decode. It would also
  have to be raised, not described, or be plumbed back through `GroupRange` anyway.
- **Check both places (walk *and* decode), for defence in depth.** +8 lines. Rejected on a
  concrete cost: with the walk check present the decode check is unreachable, so it is an
  equivalent mutant by construction — `cargo mutants` would report it as a surviving mutant every
  run (the gate is at `0 missed` today, §5), and a guard no test can turn red is a guard no future
  edit is protected by.
- **A new `MAX_SEG_CHUNKS` chunk-count ceiling** instead of the byte ceiling. Rejected: the
  brief puts "the record-ceiling helpers" out of scope (#651), 0016's knob table assigns the value
  to a different slice (`0016:1467`), and a chunk-count bound does not bound the value's bytes
  anyway (`placement` vectors and 39-digit `u128` ids vary a `ChunkRef` from ~50 to ~300 bytes).
  The byte ceiling bounds the thing that is actually allocated.
- **Refuse at `>=` instead of `>`.** Zero lines either way. Rejected because a record of exactly
  `V` bytes is one every backend in play *would* have stored, so refusing it is the permanent
  unreadability C-1 forbids — and this is not a matter of taste: the unit test's two fixtures are
  the same record at values one byte apart, so `>=` fails it (§4).

---

## 4. Refutation — the three questions

**(a) Genuine red?** Yes, measured twice, by actually reverting the production change and
re-running the project's test runner.

*Ceiling check deleted, everything else intact:*

```
$ cargo test -p wyrd-core --test segmented_map_resolution
test a_segment_record_over_the_value_ceiling_is_refused_before_the_rest_of_the_range ... FAILED
   a value over the ceiling: must fail closed; the read answered Some([0, 1, 2, 3, 0, 1, 2, 3, …])
test result: FAILED. 11 passed; 1 failed

$ cargo test -p wyrd-core --lib metadata::segmented_shape_invariants::a_segment_value
test a_segment_value_one_byte_over_the_ceiling_is_refused_and_at_it_resolves ... FAILED
   the call must fail closed: Some(CurrentChunkMap { … })
test result: FAILED. 0 passed; 1 failed
```

Note *how* the integration case fails without the fix: the read **succeeds and returns the
object's bytes**. That is the fixture's whole design — the oversized record is real, decodable,
canonically keyed and exactly the extent the root's table names, so the only thing that can refuse
it is the ceiling. The red is not an artefact of a bent fixture.

*Refusal raised locally instead of described (the arbiter mutant):*

```
$ cargo test -p wyrd-core --test segmented_map_resolution
test an_over_ceiling_value_in_a_retired_generation_restarts_onto_the_live_one ... FAILED
test result: FAILED. 11 passed; 1 failed
```

So the second test binds the *routing* of the refusal, not just its existence.

And the whole-file red on the slice's base is unchanged from iteration 6 (`run-verify.sh` reverts
`metadata.rs`/`read.rs`/`server/src/lib.rs` and removes `custodian/src/resolve.rs`, keeping both
test files): both files still compile there, because the two symbols the new fixtures add —
`ChunkRef` and `MAX_VALUE_BYTES` — are base-visible from #648
(`git show HEAD:crates/core/src/metadata.rs` → `:127`, `:325`). Nothing this patch adds is
imported by either test file.

**(b) Production path?** Yes. The integration cases drive `wyrd_core::read::read_object` — a
base-visible production entry — over a **real** `RedbMetadataStore` and a **real** `FsChunkStore`
whose fragments were written by `wyrd_core::write`; the only test-owned code is the `Probe` store
double, which wraps the real redb store and records/perturbs accesses. The unit case drives
`resolve_current_chunk_map` (the production resolver) over a **real** in-memory redb store seeded
with a raw `WriteBatch` — no fake store, no re-implementation. Cross-check by mutation:
`cargo mutants --in-diff` over this bundle's diff — **17 caught, 42 unviable, 0 missed** (was 14
caught before this iteration; the three new viable mutants in the ceiling are all caught).

**(c) Fixture includes the fault?** Yes — in each case the fixture *is* the fault, and it is
present in the store the read runs against:

- the integration fixture seeds a genuine oversized `seg:` value: one of the object's own chunks
  repeated until `encode(record).len() > MAX_VALUE_BYTES`
  (`crates/core/tests/segmented_map_resolution.rs:747`), committed into redb as segment 0 of a
  two-segment generation whose table names its extent exactly (`:761`). Nothing is curated out:
  the second segment is present and correct, and the store serves one row per page
  (`Quirk::PageCap(1)`) so the assertion that exactly **one** page was requested
  (`:803`) is what proves the walk stopped at the offending row instead of reading the rest;
- the retirement case seeds the same oversized generation and then applies a **real** concurrent
  `WriteBatch` mid-resolve (`When::Page`), so the retired-vs-corrupt question is asked against a
  root that genuinely moved (`:815-826`);
- the unit case seeds two values of the same record one byte apart — exactly `MAX_VALUE_BYTES`
  and `MAX_VALUE_BYTES + 1` (`crates/core/src/metadata.rs:3476`, asserted at `:3529`) — so the boundary itself is in
  the fixture, not near it. Both are real stored values in a real backend; the at-ceiling one must
  resolve to the record's own chunks, the one-byte-larger one must be refused with the typed
  variant, its `bytes` and `ceiling` asserted.

All other iteration-6 fixtures are unchanged and re-verified green (decoy groups actually present
in the store, a root really naming `MAX_ROOT_SEGMENTS + 1` segments, a real 512-segment object
over a three-page walk, six live-root anomalies each seeded, four retirement interleavings, and
the DST nemesis that reclaims the old generation on half its 50 seeds).

---

## 5. Evidence log (this iteration, on the final tree)

| check | command (project runner) | result |
|---|---|---|
| whole gate | `./engine/xtask.sh ci` | typos, docs, gitlink, unsafe-forbid, **fmt**, **clippy**, build, **test (156 targets, 0 failures)**, machete all pass; **FAILS at `cargo deny check`** — base advisory only (§1) |
| conformance | `./engine/xtask.sh conformance` | pass — 5 valid + 6 invalid vectors |
| statics | `./engine/xtask.sh statics` | pass — no DST-reachable shared mutable global state |
| DST (50 seeds, `--cfg madsim`) | `./engine/xtask.sh dst` | **pass, exit 0**, incl. `test segmented_resolve_never_tears ... ok` |
| mutants on the diff | `PDCA_BUNDLE=… ./scripts/mutants-in-diff` | 59 mutants — **17 caught, 42 unviable, 0 missed** |
| new core integration | `cargo test -p wyrd-core --test segmented_map_resolution` | 12 passed (10 carried + 2 new) |
| new resolver unit | `cargo test -p wyrd-core --lib metadata::` | 22 passed (21 carried + 1 new) |
| red legs | production change reverted (§4a), on the final test code — three runs: check deleted, check deleted (unit), and the refusal raised locally instead of described | both new tests **red** in the right runs, everything else green. The only tree edit after the last red leg is a comment block (`crates/core/src/metadata.rs:2096-2102`, why the *root* value is deliberately not ceilinged) |
| formatter (commit-hook readiness) | `cargo fmt --all -- --check` | clean. The target configures no `pre-commit`/`husky`/`core.hooksPath`; its commit-time formatter/linters are `xtask ci`'s `fmt --check` + `clippy -D warnings`, both green |
| patch applies to base | `git apply --check patch.diff` in a pristine `6e7c255` worktree | clean |

---

## 6. Budget

Non-mechanical semantic additions (added lines, non-blank, non-comment, the brief's own rule):
**1,754** across **19 files** (≤ 15 non-mechanical files: 9, the other 10 are the declared
`read_object_from` → `read_object_chunks` migration, 205 lines, counted separately per the brief).

| file | semantic + | vs iteration 6 |
|---|---|---|
| `crates/core/tests/segmented_map_resolution.rs` | 646 | +52 (two new tests + the oversized-record fixture) |
| `crates/core/src/metadata.rs` | 437 | +89 (**+25 production**: the variant, its Display arm, the check; **+64** the boundary unit test) |
| `crates/server/tests/segmented_object_read.rs` | 265 | — |
| `crates/custodian/src/resolve.rs` | 202 | — |
| `crates/dst/tests/custodian.rs` | 146 | — |
| `crates/server/src/lib.rs` / `read.rs` / `custodian/lib.rs` / docs | 58 | — |
| **non-mechanical total** | **1,754** | **+141** |

Production side is **481** semantic lines against the brief's ~660 model; the residue is test
bodies. The number is ~17 % over the brief's `~1,500` (iteration 6 shipped 1,613, ~7 % over, and
its C3 review row passed). I trimmed the new material twice before landing it — the unit fixture
went from widening chunk ids to padding one record's value with the whitespace `decode` ignores
(−30), and the integration fixture from three segments to two (−11) — but I did not buy the last
250 lines back by deleting assertions: every remaining candidate binds a distinct production
branch. If the human wants it under 1,500, the cheapest cut is still the gateway restart pair
(−121, `crates/server/tests/segmented_object_read.rs`), which would leave `served`
(`crates/server/src/lib.rs:363`,`:451`) untested, and then the two ceiling-retirement cases
(−22 each), which would leave the arbiter routing bound only by the unit level.

---

## 7. Scratch and STOP discipline

Everything this leaf created went under `$PDCA_SCRATCH/pdca-builder-649-{redleg,apply}` (gate
logs, the reverted-production copies of `metadata.rs`, and a throwaway `6e7c255` worktree for the
`git apply --check`); the worktree was removed, `git worktree prune` run, and both directories
deleted — the transcripts above are copied here because the logs are disposable. No branch pushed,
no PR opened, nothing marked ready. `patch.diff`, the two test files and these notes are the whole
output.

**One thing a reviewer may raise that is answered in the code, not here:** why the *root* value is
not ceilinged too. The answer is at `crates/core/src/metadata.rs:2096-2102` — the root is one plain
`get`, exactly as at every pre-existing `.chunk_map` site, and a flat record may legitimately fill
that value on a backend with no limit of its own, so a read-side refusal there would make objects
that were published perfectly well unreadable (C-1). Bounding what may be *written* is the
publication path's job (#653).
