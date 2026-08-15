# Build notes — issue #697 (iteration 2)

*Withheld from the reviewer; written for the human at sign-off.*

Target branch: `getwyrd/wyrd @ main`, base `origin/main @ 339da46`. Worktree:
`/home/eddie/wyrd/wyrd.pdca-wt-l1`. Two files, as the brief's budget requires:
`crates/custodian/src/reconstruction.rs` (modified) and
`crates/custodian/tests/segmented_map_reconstruction.rs` (new).

---

## 1. What the change is

Reconstruction stops reading the chunk map inline out of the inode record at three sites
(`origin/main:reconstruction.rs:332`, `:583`, `:636`) and instead reads the committed
namespace **once per pass** through the shared resolver, indexing only the queued
obligations' chunks:

* `locate_queued_chunks` (`reconstruction.rs:797`) — one `meta.scan(b"inode:")`, each
  record resolved via `metadata::resolve_chunk_map`, containment by exactly gc.rs's
  downcast rule (`gc.rs:402-416`): `Ok(ChunkMapError)` names the object and continues, any
  other error propagates.
* **Rule A** (`reconstruction.rs:853`): if the resolve restarted onto a generation the scan
  never read (`resolved.record.as_ref() != &record`), the object is contained — nothing
  written, obligation kept, re-read next pass.
* **Rule C**: the plan carries `inode_key: Arc<[u8]>` (the store's own spelling) and
  `prior_bytes: Arc<[u8]>` (the stored bytes), so the CAS requires the exact bytes read
  back, never a re-encoding (`RepairPlan`, `reconstruction.rs:116-124`; the guarded
  re-index at `:678`).
* A `seg:`-resident obligation is **refused**, not drained: `Assessment::Refused`
  (`:406`, `:255`) keeps it queued and makes the pass answer `Reconciled::Blocked` (`:331`).
* An obligation the incomplete reading could not prove unreferenced is **withheld**
  (`Assessment::Withheld`, `:259`) — never drained.

The correctness core is salvaged from `results/issue_681/iteration-v7/patch.diff` as the
brief directs, plus rules A and C, plus this iteration's Rule D rework (below).

## 2. Carry-forward from iteration 1 — item by item

| Carry-forward finding | What I did |
|---|---|
| **T4 BLOCKING** `reconstruction.rs:320` [BUG] — only `Refused` increments the non-certifying count, so `Malformed`/`Unrepairable` can still yield `Satisfied` | **Recorded-rejected**, with evidence, in `review-rejected.md`. It describes base behaviour this slice must preserve (`origin/main:reconstruction.rs:214`, `:223`, `:278-282`), and the existing suite asserts exactly that answer at `crates/custodian/tests/reconstruction.rs:827-831` and `:927-935` — which brief.md:231-233 forbids editing. |
| **T4 BLOCKING** ×3 [TEST-GAP] `segmented_map_reconstruction.rs:426/427` — leg 1's queue-drain assertion swallowed by a `//` comment | Fixed. The assertion is on its own line at `segmented_map_reconstruction.rs:392`: `assert!(store.queued().await.is_empty(), "both discharged")`. Verified it is *executed* (it is the only statement on that line and the leg is base-red). |
| **C5 [impl]** — refusal accounting per object needs a **multi-obligation** fixture | Leg 2 now queues **both** of `inode:006`'s `seg:`-resident chunks (`S_A`, `S_B`) and asserts exactly **one** `action="refused"` row (`:427`), exactly **one** counter increment (`:428`), and `"obligations":2` on it (`:429`). |
| **T3 FAIL / adversary [impl]** — Rule D not implemented: `emit_refused` called from inside `for &chunk in &queue` | Reworked. `emit_refused` is now called **once per object**, at the read site (`reconstruction.rs:880`), with the obligation count as a field; the queue loop emits nothing (`:255`). The "withheld" half — which belongs to no single object — is reported once per **pass** (`emit_withheld`, `:297`/`:1076`) on its own counter. |
| **T5 [impl]** — leg 5 checks only index 0, not the duplicate second placement | Fixed. The in-record duplicate now sits at a *distinct* placement (`&[2, 3, 4]` vs `&[0, 1, 4]`) and the second reference is asserted unmoved at `segmented_map_reconstruction.rs:536`, plus `!store.holds(2, frag(C_REPAIR, 2))` (nothing rebuilt at all). |
| **T2 FAIL** — 657 raw lines vs the brief's hard 620 ceiling (STOP/restructure) | Test file is now **620 raw** exactly (`wc -l`). See §4 for the semantic figure, which I could not bring to 380 and am flagging rather than hiding. |
| **Adversary [impl]** — leg 8 declared non-red but measured red, for a reason unrelated to its property | Fixed by making the declaration true (§3). Measured partition is now **exactly** the brief's: legs 1–6 red on the base, legs 7 and 8 green. |
| **Adversary** — C4-verify's "8 test(s) ran red" is a *ran* count, not a *failed* count | Still true of the gate's wording; the actual per-leg numbers are in §5 for the human. |

## 3. The one place I deviated from the brief's letter, and why

**Leg 8's Rule E half.** brief.md:97-102 specifies leg 8 as a conjunction — a
non-`ChunkMapError` `get` fault makes the pass return `Err`, **and** "the unreadable
object's name is **already** on the audit seam even though the pass returns `Err`" — and
declares the whole leg **not base-red**.

Those two cannot both hold. The base emits **nothing** on
`wyrd.custodian.reconstruction.audit` for an object it cannot read (there is no such
emission on `origin/main` at all), so any assertion that a name reached the seam is
base-**red** by construction. That is precisely what the v1 adversary measured
(7 red / 1 green instead of 6 / 2) and what the reviewer failed C1/C2 on.

Resolution — keep every property bound, make the declaration true:

* **Leg 8** now asserts only the over-containment property (base-green, fix-green): the
  pass returns `Err`, **nothing at all was written** (`assert_eq!(store.rows(), before)`),
  and the obligation is still queued (`segmented_map_reconstruction.rs:607-621`). If the
  fix contained the store fault, the pass would answer `Ok(Blocked)` **and** repair
  `C_REPAIR` — all three assertions fail. It is a real guard, not a vacuous one.
* **Rule E's placement** — "attribution emitted where the object is read, **before the work
  loop**" — moved into **leg 3**, where it is asserted *directly* rather than indirectly:
  `assert!(said(&logged, &named(key)) < repair, "named first")`
  (`segmented_map_reconstruction.rs:462`), i.e. each damaged object's name appears on the
  seam at a **lower line index** than the `action="repair"` row. Leg 3 is base-red anyway,
  so this costs no partition change. This is arguably a stronger binding of the placement
  than "the name survives an `Err`".

This is the adversary's own remedy ("*drop `seed_damaged()`, or assert only the store-fault
half*", `iteration-v1/check-advisory-adversary.md:49`).

One consequence worth the human's eye: leg 8 needs a `get` to be *reachable*, and the only
code path that reaches `MetadataStore::get` under this pass is a segmented generation's root
re-read (`crates/core/src/metadata.rs:2323-2340`). So leg 8's fixture keeps the unresolvable
segmented object (`seed_unresolvable`) and drops only the undecodable record.

## 4. Budget — met on raw, **over on semantic**; the number and the calibration

| | brief cap | this patch |
|---|---|---|
| files | exactly 2 | **2** ✔ |
| `src/reconstruction.rs` added semantic (non-blank, non-comment) | ≤ 230 | **199** ✔ |
| `tests/segmented_map_reconstruction.rs` raw | ≤ 620 | **620** ✔ (the stated STOP trigger) |
| `tests/segmented_map_reconstruction.rs` semantic | ≤ 380 | **452** ✘ (+72) |

I could not reach 380 semantic and did not want to reach it by deleting mandated
assertions or by hiding code behind `#[rustfmt::skip]` one-liners past the point of
readability. The evidence that the pairing, not the shape, is what does not fit:

* The brief's **own scale reference** — `crates/custodian/tests/segmented_map_restore.rs`,
  #651's discriminator, which brief.md:246-247 says "is 731 lines in total and **covers
  more**" — measures **458 semantic / 731 raw** on the base (same `awk` definition:
  non-blank, non-comment). This file is **452 semantic / 620 raw**: *fewer* semantic lines
  than the artifact the brief points at as the larger one, and 111 fewer raw.
* At that file's density (63% semantic), a 620-raw file is ~390 semantic. Getting 452
  semantic under 620 raw already means **27%** non-code lines here versus **37%** in the
  sibling — i.e. this file is *already* leaner in comments than its family.
* Every prescribed compression rule is satisfied: **one** `BTreeMap`-backed metadata double
  carrying both counters *and* the injected `get` fault *and* the retired snapshot (not
  three store types); **one** seeding helper family (`seed` / `seed_seg` / `segmented` /
  `root` + the parameterised `seed_fixture`); **one** audit-capture helper (`seam` / `said`)
  shared by legs 2, 3 and 5.
* This iteration *added* binding content the previous one lacked (Rule D's multi-obligation
  fixture, Rule E's ordering assertion, leg 5's second-reference oracle) while cutting the
  file from 657 raw to 620.

Formatter facts that dominate the count, in case the cap was set without them:
rustfmt's default `fn_call_width = 60` explodes any `assert_eq!(a, b, "msg")` whose argument
text exceeds 60 characters into **4** lines. I rewrote every assertion in the file to fit
inside 60 (that alone recovered ~40 lines); the remainder is irreducible structure.

**I am flagging this rather than declaring it met.** If the human wants 380 enforced, the
honest next step is dropping legs or assertions — which needs a Plan decision, not a
builder's.

## 5. Verification — evidence, not assertion

Both runs through the project's own runners, in `$PDCA_WORKTREE`.

**C4-verify** (`./engine/scripts/run-verify.sh`, the configured red→green gate):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_reconstruction (fix applied)
               test result: ok. 8 passed; 0 failed
run-verify.sh: RED   — (production reverted, test kept)
               test result: FAILED. 2 passed; 6 failed
run-verify.sh: PASS — red without the fix, green with it
```

The 6 that fail on `339da46` are exactly legs 1–6; the 2 that pass are legs 7 and 8 —
**the brief's declared partition, measured**:

| leg | test | base |
|---|---|---|
| 1 | `a_segmented_object_ends_no_pass_and_the_flat_repair_still_happens` | RED (`Err`: "met a segmented chunk map") |
| 2 | `work_in_a_segmented_record_is_refused_per_object_never_discarded` | RED (same `Err`) |
| 3 | `an_unreadable_object_is_named_and_the_walk_continues` | RED (`Err`: "key must be a string") |
| 4 | `a_resolve_that_restarted_is_acted_on_by_nothing` | RED |
| 5 | `a_duplicate_committed_chunk_id_is_repaired_by_neither_reference` | RED (`Satisfied`, not `Blocked` — the base repaired one and drained) |
| 6 | `the_namespace_is_read_once_per_pass_not_once_per_obligation` | RED — **`namespace: left 3, right 1`** |
| 7 | `an_empty_queue_reads_nothing_and_certifies` | GREEN (declared) |
| 8 | `a_fault_that_is_not_one_objects_map_still_ends_the_pass` | GREEN (declared) |

Note leg 6: I **reordered** its assertions so the scan count is checked *first*. In v1 the
leg went red on an earlier `Reconciled::Changed` assertion, which *masked* the #647
property (the v1 adversary had to relax that assertion by hand to confirm the count
discriminated). It now goes red on `namespace == 3` vs `1` — Q namespace scans for Q=3
obligations — which **is** the finding #647 left open, measured directly.

**C4-ci** (`./engine/xtask.sh ci` → `cargo xtask ci`: typos, docs lint/render, fmt --check,
clippy -D warnings, build, test incl. DST, cargo-machete, cargo-deny, conformance):
`xtask ci: all checks passed` (exit 0), run twice — once mid-build and once on the final
tree. `crates/custodian/tests/reconstruction.rs` is **unmodified** and green.

**Commit-readiness:** `cargo fmt -p wyrd-custodian` is a no-op on the final tree and
`cargo clippy -p wyrd-custodian --all-targets` is clean; both are inside `cargo xtask ci`,
which is what the target's hooks run. No line exceeds 100 *characters* in either file
(the `awk` byte count is misleading — em-dashes and `→` are multi-byte).

## 6. The three forced refutations

**Isolated proof that the Rule D fix is what leg 2 binds** (the carry-forward's C5/T3 item):
I reverted *only* the Rule D hunk — moved `emit_refused` back inside the per-`hits` loop with
`count = 1`, v1's shape — leaving everything else in place, and re-ran the binary:

```
test work_in_a_segmented_record_is_refused_per_object_never_discarded ... FAILED
  panicked at segmented_map_reconstruction.rs:427: assertion `left == right` failed: once per object
    left: 2   right: 1
test result: FAILED. 7 passed; 1 failed
```

Exactly one leg moved, and it moved on exactly the property in question — so leg 2 is a
discriminator for Rule D specifically, not merely for the base's abort. Production was
restored from a copy and the binary re-confirmed at `8 passed; 0 failed`.

**(a) Genuine red?** **Yes.** Not argued — *measured*, by reverting production and keeping
the test, through the project's own gate: `run-verify.sh` restores
`crates/custodian/src/reconstruction.rs` to `origin/main @ 339da46`, leaves the new test
file in place, and the binary reports `FAILED. 2 passed; 6 failed`. Each of the six fails
on a **behavioural** assertion or a propagated `Err`, not a compile error (the gate treats
"red without running a test" as UNVERIFIABLE/77 and it did not take that branch). No
assertion names any symbol this patch introduces, which is what keeps the red a measurement
rather than a build failure.

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step` — the
real fenced control point — through a `FencedZone` holding a live leadership term from
`Custodian::elect` over `wyrd_coordination_mem::MemCoordination`
(`segmented_map_reconstruction.rs:344-358`). No internal helper is called, nothing is
re-implemented: `reconcile_step` dispatches the production `reconstruction::reconcile`,
which calls the production `locate_queued_chunks` / `assess` / `repair_chunk`. The doubles
are `MetadataStore` / `ChunkStore` **trait implementations** (the store and the D servers
the production code talks to), not stand-ins for the unit under test. The erasure bytes are
real: fragments are produced by `wyrd_core::erasure::encode` + `encode_ec_fragment`, so a
survivor passes the production `repair::intact_shard`, and the repaired fragment is asserted
present on the server the record now names. The chunk maps are built with the real
validating constructors (`SegmentGroup::new`, `SegmentRecord::new`, `SegmentedMap::new`,
`metadata::seg_key`), and the queue is the real one (`repair::enqueue_repair` /
`queued_repairs`).

**(c) Fixture includes the fault?** **Yes** — and each leg *asserts its own fault is real*
before relying on it:

* The damaged objects are **seeded first in key order** over a `BTreeMap`-backed store
  (`inode:0` < `inode:00` < `inode:006` < `inode:007` < `inode:7`), so the walk meets each
  blocker **before** the healthy work — "the healthy object was still repaired" cannot pass
  on a walk that gives up early.
* `Store::root` (`segmented_map_reconstruction.rs:309-313`) calls the real
  `metadata::resolve_chunk_map` on every seeded root and asserts `resolved.is_ok() ==
  readable`, so a root seeded to be unreadable **provably** is (the brief asks for exactly
  this).
* Leg 4 asserts the resolver **really restarts** onto the live generation before driving the
  pass (`:484-487`), so Rule A is exercised, not assumed.
* Leg 8 arms the `get` fault on a store that really reaches a `get` (an unresolvable
  segmented root's re-read), and asserts the whole row set is byte-identical afterwards.
* Nothing is curated out: leg 3 keeps *both* damaged shapes **and** the healthy work in one
  store; leg 5 keeps all three claimants; leg 6 keeps the segmented objects beside the flat
  ones. Every "unchanged" assertion is against a `rows()` snapshot of the actual store.

## 7. Alternatives considered and rejected — with costs

* **Fix the `Malformed`/`Unrepairable` → `Satisfied` finding in this patch.** Rejected on
  scope, not on cost: it edits an outcome contract this slice does not touch and turns two
  existing assertions red (`tests/reconstruction.rs:827-831`, `:927-935`), which brief.md:231
  forbids repairing. Cost if done anyway: 1 production line + **2** edits to a suite the
  brief says must stay unmodified — and those edits are exactly the "an answer changed
  further than intended" signal. Recorded in `review-rejected.md` instead.
* **Report the refusal per obligation (v1's shape).** Rejected: it is the Rule D violation.
  Concrete cost measured by the v1 adversary — one `inode:006` holding two queued chunks
  emitted **2** `action=refused` rows and **2** counter increments, both naming the same
  record. The fix is 1 line moved plus a `count` field; leg 2 now binds it.
* **Fold "withheld for want of a reading" into the same `refused` counter.** Rejected: it
  would make `reconstruction_repair_refused` mean "objects **or** obligations" depending on
  the branch, so an operator could not read it. Cost of the alternative: 4 lines
  (`emit_withheld`, `reconstruction.rs:1076-1088`) for a counter that stays one thing.
* **Keep leg 8 as the brief's conjunction and declare 7-red.** Rejected: §3. It re-runs the
  exact C1/C2 failure the previous round took, for no additional bound property.
* **`#[rustfmt::skip]` on all eight test bodies to hit 380 semantic.** Rejected: it would
  save ~50 lines by making the assertions unreadable one-liners, i.e. buying a budget number
  with the reviewability the budget exists to protect. `#[rustfmt::skip]` is used only where
  the file family already uses it (trait impls and one-expression helpers).
* **Sharing one namespace walk across GC / scrub / rebalance / reconstruction.** Out of
  scope per brief.md:223-225; leg 6 is deliberately scoped to a reconstruction-only context
  so a store-wide scan count cannot demand it by the back door.

## 8. Pre-declared, restated for the record

No seeded Tier-0 DST case ships in this child, per brief.md:260-265 (settled at Plan as
recorded-rejected). The reason holds and is now *stronger*: every write this slice performs
is on a flat object read from the generation it scanned — Rule A, and leg 4 tests it — under
the same version-conditional CAS, and what it adds on the segmented side is a refusal, which
writes nothing at all. The seeded Tier-0 case for the segmented **write** path belongs to
#682.

No external dependency beyond the base Rust toolchain was needed; the five registered
`[[doctor.checks]]` ids were sufficient. **No NEEDS-HUMAN external dependency.**

## 9. Scratch

All throwaway work lived under `$PDCA_SCRATCH/pdca-builder-697-work` (the extracted v1 test
file and the two `xtask ci` logs); removed at the end of the run.
