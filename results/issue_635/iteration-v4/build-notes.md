# Build notes — issue #635 (segmented-chunk-map), iteration 4

> Withheld from the reviewer; written for the human at sign-off.

## 0. What this iteration is

Iteration 3's bundle passed `cargo xtask ci` and the per-fix red→green gate, and was
auto-iterated on two gating/advisory rows: the **T4 batched rubric review** (8 blocking
findings) and two Check cells tagged `[impl]` (C5 causal adequacy, T5 judgment). This
iteration keeps iteration 3's design — the settled `Flat | Segmented` encoding, the
`seg:`/`seggrp:` records, the one shared resolver every consumer goes through, and the
staged-publication committer — and rebuilds the **eight** places those findings said the
implementation was wrong. No finding was answered by argument; all eight are code changes
with a test each, so `review-rejected.md` gains nothing this round (the two entries it
already carries are from rounds 1–2 and stand).

The design was not re-litigated because none of the findings challenged it: every one was
an enforcement hole *inside* it.

## 1. The eight findings, and what each became

| # | Finding (round-3 `review-batch.md`) | Fix | Test |
|---|---|---|---|
| 1 | `metadata.rs:2499` — the flip validates only **aggregate** transaction bytes, so a single caller value over the backend's per-value limit passes locally and fails permanently at commit | `flip_batch` now charges the assembled batch on **both** limits (`crates/core/src/metadata.rs:2656`,`:2668`) | `no_published_batch_may_carry_a_record_over_the_backend_ceilings` (`:3896`) |
| 6 | `metadata.rs:2468` — same hole in the **segment-write** builders | same check inside the split loop, on the **assembled** batch (`:2573`) | same test, second half |
| 8 | `metadata.rs:292` — `100 * 1024` overstates FoundationDB's hard **100 000**-byte ceiling | decimal constants: `MAX_VALUE_BYTES = 100_000` (`:297`), new `MAX_KEY_BYTES = 10_000` (`:301`), `MAX_BATCH_BYTES = 5_000_000` (`:307`) | `the_capacity_constants_are_the_ones_the_arithmetic_assumes` (`:4242`) |
| 3 | `metadata.rs:2219` — an **unfenced** segment phase lets a rollback delete the segments before the flip, publishing a live root that names missing records | the per-batch contribution is no longer `Option` (`:2379`) and **every** phase batch must state ≥1 caller precondition; the **flip** must too (`:2657`, `check_fenced` at `:2795`) | `an_unfenced_publication_is_refused_in_either_phase` (`:4001`) |
| 4 | `metadata.rs:879` — `SegmentRecord`'s public fields let `repoint_chunk` persist a record whose `byte_len` no longer matches its chunks (it then fails its own decode) | fields private + accessors + validating `replace_chunk` (`:1044`), used by `repoint_chunk` (`:2301`) | `a_segment_repoint_may_not_re_length_the_chunk_it_moves` (`:4864`) |
| 5 | `metadata.rs:1425` — `commit_chunk_map` installs a **flat** map even over a segmented `prior`, stranding that generation's `seg:` records | typed refusal before any commit (`:1571`) | `a_whole_map_recommit_refuses_a_segmented_prior_rather_than_stranding_it` (`:4919`) |
| 2 | `backfill.rs:153` — after the resolve **restarts** onto a superseding root, the shape check still consults the stale snapshot | the resolver hands back the generation it resolved (`crates/custodian/src/resolve.rs:43`) and backfill decides and CASes on it (`crates/custodian/src/backfill.rs:112`,`:202`) | `backfill_follows_the_live_generation_when_the_resolve_restarts` (`crates/custodian/tests/backfill.rs:470`) |
| 7 | `dst/tests/custodian.rs:223` — the crash injector dressed a definite `Conflict` as `CommitUnknownResult` | only a commit that **landed** is reported unknown (`crates/dst/tests/custodian.rs:232`) | assertion at `crates/dst/tests/custodian.rs:1766` |

And the two `[impl]` Check cells:

* **C5 (causal adequacy)** — "a direct probe was accepted with a 102 401-byte value": that
  probe is now refused, and at the *right* threshold (100 001, since the constant was also
  wrong). Findings 1/6/8 are one fix in three parts.
* **T5 (judgment)** — "extend the envelope oracle to adversarial caller contributions":
  the new test drives the ceiling from the **caller's** contribution (flip *and* per-batch),
  not from this committer's own well-behaved fixture, and asserts in the same breath that
  the assembled batch is far **inside** the aggregate envelope — i.e. that the aggregate
  check is exactly the one that would have passed it.

## 2. Why these shapes, and what was rejected

**Per-record ceilings as a separate check, not a bigger aggregate.** The tempting cheap fix
is to shrink the aggregate budget so an oversized value can't fit. It does not work: the
two limits are independent — a 200 KB value sits inside a 5 MB transaction — and the
failure modes differ (aggregate: re-split and retry; per-value: permanent rejection, no
retry). `check_record_ceilings` (`crates/core/src/metadata.rs:2759`) is therefore checked
**before** the re-split loop's budget arithmetic: re-splitting can never make one record
smaller, so looping on it would just delay the same answer by `MAX_SPLIT_ATTEMPTS`.

I also charge **keys** (`MAX_KEY_BYTES`), which the finding did not ask for: it is the same
permanent-rejection class one field over, three lines of code in the same loop, and a
caller-contributed `retire:` key is exactly where an unbounded key could appear.
Precondition *expected* values are deliberately **not** charged: an over-ceiling expected
value can only ever fail to match (a `Conflict` the caller owns), never a permanent
backend rejection, so refusing it here would invent a failure the seam does not have.

**The fence as a required parameter, not a documented expectation.** Two weaker options:

* *Document it and trust the caller* — that is what iteration 3 did (`segment_batch:
  Option<…>` plus prose), and the finding is precisely that the option made an unsafe
  publication expressible.
* *Have the flip re-verify the segments* — `require(seg:<g>:<E>:<i> == bytes)` for every
  segment in the flip batch. Concrete cost: up to `MAX_ROOT_SEGMENTS` = 512 preconditions
  carrying the full segment values, ~50 000 bytes each ⇒ ~25 MB in one batch against a
  5 MB envelope — it cannot be built at all for any map big enough to need segmentation,
  which is every map this feature exists for.

So the enforceable shadow of "must be fenced" is: **the caller must state at least one
precondition per phase batch**. That does not prove the precondition is the *right* fence
(`require(mpu == Completing@E)` is #636's record and #636's business) — it turns an unsafe
default into a typed error and makes the caller name what it is publishing under. The flip
is included deliberately: fencing only phase 1 leaves the window between the last segment
batch and the flip, which is the window the finding describes.

**Encapsulating `SegmentRecord` rather than re-validating on serialize.** Recomputing
`byte_len` at encode time would silently *accept* a re-lengthed chunk and write a record
that disagrees with the root's `SegmentRef` — trading a decode failure for a bounds
mismatch, one consumer later. Refusing the mutation is the parse-don't-validate shape the
repo requires (`AGENTS.md:146-149`), and it is total: the record is left untouched, so a
caller that ignored the error still holds a valid value (asserted).

**Backfill: the record travels with the chunks.** The alternative was a targeted guard in
backfill ("if the resolve restarted, skip this record"). That guards the symptom in one
consumer and leaves the seam able to hand any future consumer a stale record beside live
chunks — the exact shape of the #508-attempt-4 defect class this slice exists to close.
Making `chunks_of` return `LiveMap { record, chunks }` cost **4 lines** at the two
consumers that don't need the record (`gc.rs:265-268`, `restore.rs:383-386`) and makes the
wrong thing unspellable at the others.

**`commit_chunk_map`'s refusal** matches its two superseding siblings, which already
refused a segmented prior (`crates/core/src/metadata.rs:1616`,`:1693`); the odd one out was
the non-superseding entry. No production caller exists today (only tests call it —
`git grep 'commit_chunk_map('`), so the refusal changes no live path; it closes the door
before #636 opens it.

## 3. Red → green evidence (actual numbers, per the brief's Falsifiability clause)

**RED leg** (`engine/scripts/run-verify.sh`, the C4-verify gate, run from the bundle):

```
running 8 tests … test result: FAILED. 0 passed; 8 failed
```

All eight failures are **runtime assertion / decode failures**, not build errors — e.g.
`reconcile_step must resolve a segmented chunk map, not fail on it: Some("reconciliation
store access: invalid type: map, expected a sequence at line 1 column 23")`
(`crates/custodian/tests/segmented_map_consumers.rs:504`), which is the base's strict
`metadata::decode(&value)?` at `crates/custodian/src/gc.rs:256` meeting a segmented value.
That is the brief's requirement: leg A's red is assertions, and the base is sound.

**GREEN leg**: `cargo test -p wyrd-custodian --test segmented_map_consumers` → `8 passed;
0 failed`. `run-verify.sh` printed `PASS — red without the fix, green with it.`

**Whole gate**: `./engine/xtask.sh ci` → `xtask ci: all checks passed` (fmt, clippy incl.
`--cfg madsim`, build, test, deny, conformance, statics, deploy-guard, prose gates — `typos`
and the doc renderer are both installed on this host, so the docs edit is really gated).

**Mutation (advisory C5)**: iteration 3's row errored with *"cargo test failed in an
unmutated tree"*. It does not reproduce: `ok  Unmutated baseline in 52s build + 18s test`,
then `287 mutants tested in 7m: 3 missed, 135 caught, 149 unviable`. All **three** misses
were in this iteration's own new code — the key-ceiling comparison (`> ` vs `>=`) and two in
`rendered_key`'s truncation — so I killed them rather than explaining them: the ceiling test
now pins the key boundary on both sides and the rendered form of an over-ceiling key
(`crates/core/src/metadata.rs:3966`,`:3977`,`:3983`). Each kill was verified by hand-applying
the mutant (§5). The gate was re-run afterwards; the result is in §6.

## 4. The forced refutation (a)/(b)/(c)

**(a) Genuine red?** Yes, and checked per fix rather than in aggregate:

* Leg A: 8/8 fail with the patch reverted (above), 8/8 pass with it.
* Backfill (finding 2): I reverted the one-line seam change in
  `crates/custodian/src/backfill.rs` (decide on the scanned record again) and re-ran —
  `backfill_follows_the_live_generation_when_the_resolve_restarts` failed with
  `SegmentedPlacementUnfillable { inode: 1, unfilled: 1, records: 1 }`, i.e. the reported
  failure mode verbatim: a fillable map reported unfillable. Restored, green.
* The other four fixes: each new test was written against the *pre-fix* behaviour and I
  re-ran them with the fix disabled (§5 below records the exact reverts and failures).
* The constants test is trivially binding (it asserts the constant that changed).

**(b) Production path?** Yes. Every new assertion drives production code: `flip_batch` /
`segment_batches` / `plan` on the real `SegmentedPublication`; `repoint_chunk` and
`SegmentRecord::replace_chunk` as reconstruction and rebalance call them; `commit_chunk_map`
against a real `RedbMetadataStore::in_memory()`; `backfill::reconcile` as `reconcile_step`
runs it. The only stand-ins are the *callers* the seam is parameterised over (the session's
fence/mutations, which are #636's) and the store doubles that inject an interleaving a real
backend can produce (`StaleScan`, `Shuffling`, `CrashMeta`) — never a stand-in for the code
under test.

**(c) Fixture includes the fault?** Yes:

* the ceiling tests carry an actually-oversized value (`MAX_VALUE_BYTES + 1`) **and** assert
  the batch is inside the aggregate envelope, so the fixture cannot pass by being small;
* the unfenced test contributes a real, well-formed mutation with **no** precondition —
  the exact shape that used to be accepted;
* the backfill test's store really returns a **stale segmented** cut from `scan` while `get`
  answers the live flat root — the failing element is in the fixture, not curated out;
* the repoint test replaces a chunk with a genuinely different `len`;
* the DST fidelity assertion drives a genuinely **false** precondition at the
  unknown-result ordinal.

## 5. Per-fix refutation detail (what I reverted, what actually failed)

Executed, not assumed — each revert applied to the working tree, the test re-run, the
original restored immediately after (scripts under `$PDCA_SCRATCH`, removed). Every row went
**red**, and every red is an **assertion**, not a build error: where deleting a call site
would have produced a dead-code build failure (`-D warnings`), I neutered the *body*
instead (`if true { return Ok(()) }` / `if false`) so the call sites stayed live.

| Fix reverted | Test | Observed |
|---|---|---|
| `check_record_ceilings` body neutered (call sites kept) | `no_published_batch_may_carry_a_record_over_the_backend_ceilings` | RED — panic at `metadata.rs:3950`: `flip_batch()` returns `Ok` for a 100 001-byte caller value |
| `check_fenced` body neutered (call sites kept) | `an_unfenced_publication_is_refused_in_either_phase` | RED — panic at `metadata.rs:4045`: an unfenced phase and an unfenced flip both build |
| length guard removed from `replace_chunk` | `a_segment_repoint_may_not_re_length_the_chunk_it_moves` | RED — panic at `metadata.rs:4903`: the re-lengthing repoint is accepted |
| `is_segmented` guard removed from `commit_chunk_map` | `a_whole_map_recommit_refuses_a_segmented_prior_rather_than_stranding_it` | RED — panic at `metadata.rs:4955`: `Ok(Committed)`, the root flattened |
| `&& outcome == CommitOutcome::Committed` removed | `prop_staged_publication_is_atomic_at_the_flip` (`crates/dst/tests/custodian.rs:1764`) | RED — a definite `Conflict` surfaces as `CommitUnknownResult` |
| `let record = live.record` → the scanned record | `backfill_follows_the_live_generation_when_the_resolve_restarts` | RED — `SegmentedPlacementUnfillable { inode: 1, unfilled: 1, records: 1 }` |
| mutant `key.len() >= MAX_KEY_BYTES` | same ceiling test | RED — panic at `metadata.rs:3977` (the exactly-at-ceiling key must fit) |
| mutant `rendered_key` elides at `== SHOWN` | same ceiling test | RED — panic at `metadata.rs:3966` (the over-ceiling key's rendered form) |
| mutant `rendered_key` elides at `>= SHOWN` | same ceiling test | RED — panic at `metadata.rs:3983` (a 64-byte key is shown whole) |

## 6. For the human at sign-off

* **Base**: the worktree is `origin/main` @ `b0cd199` (the brief's wave-1 stack base
  `origin/pdca-integration/main` does not exist on this host; `#634`'s branch
  `origin/enhancement/634-scan-page-seam` is unmerged). So `MetadataStore::scan_page` does
  **not** exist on this base and the added test's `MemMeta` correctly does **not** implement
  it — as in iterations 1–3. When #634 folds in first, the added double needs the one
  delegating `scan_page` line the brief describes; that is a fold-time edit, and it is the
  same E0046 the iteration-1 carry-forward reported. **This is the one thing to re-check at
  the fold.**
* **T3 / Validation stay NEEDS-HUMAN by construction**: no production path publishes a
  segmented map until #636 lands the session, which the brief states is correct
  (`0016:2287-2299`). The runtime evidence here is precursor-only: a real `Completing@E`
  caller does not exist yet. Nothing I can do in this slice changes that; it is a maintainer
  decision about freezing the durable format and the committer API before its first
  production caller.
* **T4**: all eight round-3 findings are **fixed**, so they should leave the next run. I did
  not add rejections this round.
* The advisory mutation row's baseline error from iteration 3 did not reproduce, and after
  killing the three survivors the re-run is clean: **`287 mutants tested in 6m: 0 missed,
  138 caught, 149 unviable`** (iteration 3: ERROR; iteration 2: 1 missed).

## 7. Housekeeping

Scratch: the refutation scripts and the backfill backup lived under `$PDCA_SCRATCH`
(`pdca-builder-635-*`) and are removed. What remains is the two mutation logs
(`$PDCA_SCRATCH/pdca-builder-635-mutants{,2}.log`, kept as the evidence behind §3/§6) and
`mutants.out/` inside the worktree, which `.gitignore:14` excludes — verified it is not in
`patch.diff`.

No PR was pushed, opened, or marked ready.
