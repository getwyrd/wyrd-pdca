# Build notes — #697 (iteration 4)

**Target branch:** `getwyrd/wyrd @ main` = `339da46` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`,
clean off that commit). Every `path:line` below indexes the **patched** worktree unless it says
"base", in which case it indexes `origin/main @ 339da46`.

Two files, as budgeted:

| file | added semantic lines | cap |
|---|---|---|
| `crates/custodian/src/reconstruction.rs` | **229** | 230 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` (new) | **400 semantic / 614 raw** | 380 / 620 |

The test file is **20 semantic lines over** its cap; §4 below accounts for every one of them and for
why I judged the trade the right way. Both of the brief's **hard STOP** conditions hold: exactly 2
files, and 614 ≤ 620 raw.

---

## 1. What this iteration changes, and why

This is a rebuild of iteration 3. I kept iteration 3's production shape — which passed C1–C4,
C4-verify red→green and C5, and which is itself the brief-mandated salvage of
`results/issue_681/iteration-v7/patch.diff` — and fixed the round-3 carry-forward. Nothing else
changed, because nothing else was found wanting.

### (a) T4 BLOCKING, the primary bug — a repair could still destroy bytes the pass never read

The finding (`review-batch.md:3-4`, `reconstruction.rs:316`/`:313` on the **v3** tree): with the
namespace reading **incomplete**, repair still proceeds, and `put_fragment` can **overwrite** a
fragment belonging to an object this pass could not read but which claims the same `ChunkId`. Sound,
and the sharpest edge of C-1:

* `put_fragment` overwrites in place (base `reconstruction.rs:566`);
* it runs **before** the CAS ("write the rebuilt fragments to their new D servers FIRST", base
  `:548-549`), so the version-conditional commit cannot reject it;
* GC's `incomplete-reference-set` backstop — the one rule B rests on — withholds **deletes**
  (`gc.rs:306-316`, consulted at `:191-194`), so it reaches an overwrite not at all;
* and `CommittedIndex::note` (`reconstruction.rs:799`), the apparatus that refuses a duplicate id,
  only sees records the pass **could** read. A duplicate hidden in an unreadable object is invisible
  to it by construction.

**Fix — close the overwrite path before landing the fragment** (`reconstruction.rs:656-676`). A
re-placement may land rebuilt bytes only in the slot the reference's **own committed placement
already names** (`claimed == false` — rebuilding a rotted fragment where the record already places
it IS the repair), or in a **claimed** slot that `may_land` proves it takes nothing from
(`reconstruction.rs:744-760`):

| occupant of the claimed slot | verdict | why |
|---|---|---|
| absent (`Ok(None)`) | land | nothing to destroy |
| byte-identical (`Ok(Some(b))`, `b == bytes`) | land | the write is a no-op; `erasure::encode` + `encode_ec_fragment` are deterministic, so **every benign occupant** (a re-placement that lost its CAS, an unreclaimed orphan of this same chunk) is byte-identical |
| already unrecoverable at the device (`Err`, `is_permanent_read_fault`) | land | those bytes are lost to their owner whoever it is; refusing would stall this repair for as long as the rot sits at the target — itself a permanence C-1 forbids |
| anything else (`Err`, transient) | **refuse** | unknown is not empty; fail closed |

A refusal writes nothing, keeps the obligation, names chunk/index/server on the audit seam with its
own offset counter `reconstruction_would_overwrite` (`:1154-1166`), and makes the pass
non-certifying through the existing `refused` tally (`:326`, `:339` — `Blocked` outranks `Changed`
in `reconciliation.rs:55-61`). New `RepairOutcome::Refused` (`:613`) rather than reusing `Aborted`,
so the metric accounting stated at `:277-284` stays exact
(`repaired − conflict − aborted − would_overwrite`).

**Rejected alternative — the finding's own first option**, *"withhold repairs (not just drains) for
a chunk ID whenever there's any object this pass could not read"*. Not rejected on cost: it **fails
the Success criterion**. Leg 3 is a binding, base-red leg whose pinned conjunction includes *"the
healthy object's repair still happens"* (brief.md:63-65) over a store holding two unreadable
objects, and rule B is pinned "do not relitigate" (brief.md:172-181). Withholding also re-creates
the store-wide repair stall this issue exists to remove (brief.md:19-20) — one damaged record again
costing every healthy object its redundancy. Concretely it turns
`crates/custodian/tests/segmented_map_reconstruction.rs:327` (`assert_repair_landed`, called from
legs 1 and 3) red. The finding's **second** option — *"or otherwise close the overwrite path before
landing the fragment"* — is what I built.

**The residual, stated rather than hidden.** With a duplicate id hidden inside an unreadable object,
a slot that is *empty* may still be one that object's placement names; landing there turns "that
object is missing fragment i" into "that object has a wrong fragment i". `may_land` does not close
that, and nothing can without either (a) refusing all repair while any object is unreadable — the
option ruled out above — or (b) content-addressed fragment ids, an on-disk format change (ADR-0002
vectors; #652 owns the id floor). What IS closed is the **destructive** half: no byte that was still
readable is taken. I judged that the right boundary because a destroyed byte is unrecoverable while
a hole is not, and because the hidden object is by construction one no reader can resolve today
(#694 tracks the operator surface). Leg 9's audit row is what tells an operator the anomaly exists.

### (b) T4 BLOCKING — the Tier-0 DST test-gap finding

Recorded-rejected, as the carry-forward directed, in `results/issue_697/review-rejected.md:8`, with
the brief's own pinned reasoning (brief.md:262-265) plus the round-4-specific note that the fragment
seam this iteration adds is likewise a *refusal to write*.

### (c) Advisory (adversary) — the refusal's obligation count counted references, not obligations

`emit_refused`'s count is now over **distinct** queued chunk ids (`reconstruction.rs:955`): a
`repair:` obligation is keyed by chunk alone (`crates/core/src/repair.rs:32`), so a record naming one
queued chunk twice holds one repair, not two. Bound, not merely fixed: `seed_fixture`
(`tests/segmented_map_reconstruction.rs:293-294`) now seeds the segmented object with **three**
references over **two** queued chunks, so leg 2's `"obligations":2` assertion (`:123`, `:397`) reads
`"obligations":3` and fails without the change.

### (d) Advisory (adversary) — "one repair per multi-obligation object per pass, reported as conflict"

Left alone, deliberately: the reviewer confirmed it identical on the base and left it a human call,
and the carry-forward says "Not required for this rebuild". Touching the conflict/abort vocabulary
would change what `reconcile_step`'s `least_certified` fold reports for a condition this slice does
not own.

## 2. What did NOT change from iteration 3

The correctness core the brief told me to salvage, and iteration 3's three fixes, are untouched:
one resolving reading of the namespace per pass (`locate_queued_chunks`, `:860`), per-object
containment by exactly gc.rs's downcast rule (`:894-909`), rule A's superseded-generation check
(`:924`), rule C's raw-key CAS (`RepairPlan::inode_key`, `:117-121`), rule D's per-object refusal
(`:950-955`), rule E's attribution before the work loop (`:791`, and the named-then-propagated store
fault at `:906-908`), and the withheld-drain gate above **both** routes to a no-op drain (`:229`).

## 3. Red → green, through the project's own runner

`./engine/scripts/run-verify.sh` (the configured `C4-verify` gate cmd, `PDCA_BUNDLE=results/issue_697`),
which applies `patch.diff` to a clean `../wyrd-verify` worktree cut off `origin/main`:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_reconstruction (fix applied)
test result: ok. 9 passed; 0 failed
run-verify.sh: RED — ... (production reverted, test kept)
test result: FAILED. 1 passed; 8 failed
run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
```

The one leg that passes on the base is `an_empty_queue_reads_nothing_and_certifies` — the declared
non-red regression guard (brief.md:88-96). All eight others, **including leg 9**, fail on the base
on behavioural assertions.

`./engine/xtask.sh ci` (the `C4-ci` gate: typos, docs, fmt `--check`, clippy, build, test incl. DST,
machete, deny, conformance) → `xtask ci: all checks passed`. The existing suite
`crates/custodian/tests/reconstruction.rs` stays green **unmodified**, as brief.md:231-233 requires.

`./scripts/mutants-in-diff` (the advisory `C5-mutants` gate) → **45 mutants tested: 25 caught, 20
unviable, 0 missed**. The first run of it did surface one survivor —
`reconstruction.rs:674 replace != with ==`, i.e. the pre-existing `old != target` displacement test,
newly in scope because my hunk moved those lines. I fixed it rather than waiving it: the guard and
the orphan-mark now turn on ONE named boolean (`let claimed = old != target;`, `:658`), so flipping
that operator skips the landing guard too and leg 9 catches it. That is also the better code — the
two halves can no longer disagree about which slot is whose.

### Refutation of my own test (forced, recorded)

**(a) Genuine red?** Yes — proven twice, both by reverting production wholesale and by reverting the
single new decision:
* whole-patch revert: the C4-verify red leg above, 8 of 9 legs red, leg 9 among them;
* single-arm revert: flipping `may_land`'s permanent-fault arm to `Err(_) => false` turns leg 9 red
  at `tests/segmented_map_reconstruction.rs:612` (`[161, 167]` vs `[161]` — the dead-slot repair
  refused and its obligation kept). Restored and re-greened afterwards; and the mutation run above
  is the same question asked mechanically over all 45 mutants of the diff.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real fenced
control point, entered through `Custodian::elect` + `FencedZone::new` over `MemCoordination`
(`tests/segmented_map_reconstruction.rs:313-322`) — never `reconcile`, never `repair_chunk`, never
any internal helper. The doubles implement the `MetadataStore` / `ChunkStore` **traits**, i.e. the
production seams; chunk-map resolution, the EC rebuild and the CAS are all production code. No symbol
this patch introduces is named by the test, which is what lets the red leg compile at all.

**(c) Fixture includes the fault?** Yes, in every leg, and the fixture asserts its own faults are
real rather than assuming them: `Store::root` (`:282-287`) asserts a root seeded unreadable really
fails `metadata::resolve_chunk_map`, and leg 4 asserts the resolver really restarts onto the live
generation (`:450`) before driving the pass. Leg 9's fixture contains the failing elements themselves
— the foreign fragment sitting at the address the re-placement claims (`:598`) and the armed dead
sector at the other target (`:599`) — and asserts over those exact bytes afterwards (`:606-613`),
not over a curated view.

## 4. The shape overage, accounted for

400 semantic vs a 380 cap. The whole of it is leg 9 and what it needs:

| item | semantic lines |
|---|---|
| leg 9 (`:582-614`), the oracle for §1(a) | 22 |
| `MemDServer.rot` + its `get_fragment` arm + `fx::dead_sector` (`:85-99`, `:150`) | 4 |
| `const C_DEAD` (`:190`) | 1 |
| honest compressions applied to pay some of it back (enqueue/seed loops in legs 2, 5 and 6; `seed_fixture`; leg 9's own bindings) | −7 |
| **net** | **+20** |

Two ways to fit the cap that I considered and did **not** take, recorded so the human can overrule
me:

* **Fold leg 9's oracle into leg 3** as a sub-assertion: ~6 lines instead of 22 (≈384 total). I chose
  the dedicated leg because the round-3 sign-off asks specifically to watch this seam for a finding
  re-emerging, and a fact buried in another leg's four-way conjunction is exactly what makes that
  hard to see.
* **`#![rustfmt::skip]` at file level**, deleting the 12 per-item `#[rustfmt::skip]` attributes
  (≈388 total; I verified this toolchain's rustfmt 1.9.0 honours it). Rejected: opting a whole file
  out of the formatter to fit a line budget is the sort of thing the budget exists to prevent.

The production file's own budget is respected (229 ≤ 230): the guard was absorbed by compressing the
distinct-id count to one line (`:955`) and dropping a redundant `reason` field from the new audit
row.

## 5. Commit-readiness

`cargo fmt --check` clean over both files; `cargo clippy -p wyrd-custodian --all-targets` clean;
`cargo xtask ci` green — it runs fmt/clippy/machete/deny/typos itself, which is what the target's own
pre-commit hooks re-run. No `Cargo.toml` change, no docs change, no third file, no edit to
`crates/custodian/tests/reconstruction.rs`.

## 6. External dependencies

None beyond the brief's `External dependencies` line, and no NEEDS-HUMAN external dependency was
hit: everything ran on the plain Rust toolchain over in-memory trait doubles — no Docker, no protoc,
no live backend, no new dev-dependency, no DST leg.
