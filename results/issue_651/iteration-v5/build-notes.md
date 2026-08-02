# Build notes — issue 651 / repair-passes-through-resolver-with-containment (iteration 5)

*Withheld from the reviewer; written for the human at sign-off.*

---

## 1. What round 5 was asked to do

The carry-forward names two things:

| Carry-forward item | What I did |
|---|---|
| **C4 (gating)** — `run-verify.sh: patch.diff does not apply on origin/pdca-integration/main — the bundle is stale; rebase Do` | **Rebased the whole patch onto the base the driver and the gate now agree on** (`6bc344e`), which is *not* the base the brief assumed. §2 is the whole story — it is the most important thing on this page. |
| **T4 (gating)** — 8 blocking batch-review findings, 0 recorded-rejected | **6 fixed, 2 recorded-rejected** with reasons in `review-rejected.md` (§4), each fix bound by a test that goes red when only that fix is reverted (§5). |

Iteration 4's design — the shared maintenance walk, the per-pass chunk index, plan-before-write,
typed refusals, the repoint that pins read bytes — is **kept**. It was reviewed PASS on C1/C2/C4/C5/
T1/T2/T3/T5 last round; what failed was the *base* it was cut against and the review triage.

---

## 2. The base moved, and it moved AWAY from this slice's prerequisite — read this first

**Facts, all checkable:**

- `origin/pdca-integration/main` is at **`6bc344e`** ("build: update event-listener 5.4.1 → 5.4.2
  (RUSTSEC-2026-0221)"), whose parent is `32ab72d` = `pdca-integrate: issue_649`.
- The local `pdca-integration/main` is at **`42c0842`** = `pdca-integrate: issue_650`.
- `git reflog show origin/pdca-integration/main` → `6bc344e … fetch -q origin: forced-update`,
  previous value `42c0842`. So the remote branch was **force-pushed off** the fold that carried
  #650, which iteration 4 built on.
- `worktree.ensure` hard-resets Do's worktree to `origin/<stack-base>` on every run
  (`src/pdca_harness/worktree.py:222-268`), so `$PDCA_WORKTREE` came to me at `6bc344e`; and
  `run-verify.sh` applies `patch.diff` onto that same ref with a plain `git apply`
  (`engine/scripts/run-verify.sh:369-372`).

So the brief's premise — "the patch applies onto a tree already carrying #648–#650" — **is false in
this environment**, and building on the local fold again (iteration 4's choice) is the one thing
guaranteed to reproduce the failing gate. I built on `6bc344e`.

**What that cost, concretely.** #650 is a real dependency, and five of its symbols were missing:

| Missing on `6bc344e` (added by #650) | v4 used it for | What this patch does instead |
|---|---|---|
| `gc::ReferenceSet::unresolvable` + a resolver-routed `referenced_fragments` | restore's mark gate; `desired_state`'s drain status | `resolve::protected_fragments(&walk)` — the same ADR-0040 decision-4 classification, derived from **this slice's own walk** (`crates/custodian/src/resolve.rs:164-233`). `gc.rs` / `scrub.rs` are **not touched**. |
| `gc::object_name` | naming a damaged record for the operator | `resolve::object_name` (`resolve.rs:150-160`), the same injective escaping |
| `Reconciled::Blocked` | every pass's non-certifying verdict | added here, with the fold rule (`crates/custodian/src/reconciliation.rs:25-49`) |
| `decode_root_record` / `ChunkMapError::RootRecordUndecodable` | `RootGeneration::decode` | `decode::<InodeRecord>` directly (`crates/core/src/metadata.rs:2781-2791`) — the decode error is already a statement about *these bytes*, which is all the walk's containment needs, and it avoids duplicating #650's enum variant |
| `crates/custodian/tests/segmented_map_consumers.rs`, #650's §6.2 paragraph, DST property 10 | v4 edited all three | dropped: the file does not exist here, the docs paragraph is written self-contained (`docs/design/architecture/06-runtime-view.md:31`), and only property **11** ships |

**Why I did NOT re-do #650's half.** Routing `gc::referenced_fragments` through the resolver is the
smaller diff (≈6 lines) — and it is wrong here: the base's own test asserts GC *fails closed* on a
segmented map (`crates/custodian/tests/gc.rs:840-872`, `SegmentedMapUnsupported { operation:
"gc::referenced_fragments" }`), so the change would have forced me to rewrite #650's test as well —
i.e. re-land #650 inside #651 and then conflict with PR #676 head-on. Instead this patch leaves
`gc.rs` and `scrub.rs` **byte-untouched**.

**NEEDS-HUMAN, at sign-off:** this patch is cut against a base *without* #650. If #650's fold returns
to `origin/pdca-integration/main` before this lands, the two overlap in `crates/core/src/metadata.rs`,
`crates/custodian/src/{reconciliation,restore,desired_state}.rs`, `crates/dst/tests/custodian.rs` and
the §6.2 docs paragraph, and one of them must be rebased on the other. The cheap resolution, if #650
lands first: drop this patch's `Reconciled::Blocked` variant + fold (#650 brings them), point
`resolve::protected_fragments`'s two consumers back at `gc::referenced_fragments`, and delete
`resolve::{object_name, ProtectedFragments}` — ≈95 semantic lines, all in files this patch already
owns. Nothing else in the slice depends on the difference.

```
NEEDS-HUMAN external dependency: prerequisite slice #650 (PR #676) is absent from origin/pdca-integration/main — the wave base was force-pushed back to #649 + a RUSTSEC bump, so this patch could not build on the ReferenceSet containment the brief's "Depends on: 650" assumed, and carries ~95 lines of its own equivalent instead; the two must be reconciled before both land.
```

```toml
[[doctor.checks]]
id    = "wave-base-carries-prereqs"   # the token Plan should have put in `External dependencies`
cmd   = "bash -c 'b=$(python3 - <<\"PY\"\nimport re,os,pathlib\np=pathlib.Path(os.environ[\"PDCA_BUNDLE\"])/\"brief.md\"\nm=re.search(r\"Depends on:\\*\\*\\s*([0-9, ]+)\", p.read_text())\nprint(\" \".join(m.group(1).replace(\",\",\" \").split()) if m else \"\")\nPY\n); for id in $b; do git -C ../wyrd log --oneline origin/$(cat \"$PDCA_BUNDLE/stack-base\") | grep -q \"pdca-integrate: issue_$id\" || exit 1; done'"
hint  = "the bundle's declared prerequisite slice is not folded into the wave's integration branch on origin: re-run `pdca integrate` (or push the local fold) before Do, or the dependent is built against a base its prereq is missing from"
level = "MISSING"
```

---

## 3. The one place the criterion had to be re-cut (and why it is not a weakening)

Criterion (1) says *"`RestoreReport::stranded_marked == 0` **across a full reconcile step** over
segmented objects … and every fragment the object owns is still present"*. Iteration 4 spelled the
"teeth" as: run `reconcile_step` **with a GC context** past the grace window, and show nothing was
deleted. On this base GC cannot read a segmented map at all, so that step returns
`Err(SegmentedMapUnsupported)` — the leg would be asserting **#650's** behaviour, not this slice's.

The teeth are now:

1. **the `orphan:` ledger is empty** (`crates/custodian/tests/segmented_map_repair.rs:490-497`,
   `:563-568`) — a mark *is* an `orphan:` record, and that record is the sole evidence GC reclaims
   on, so "nothing was marked" is checked where the evidence would be, through the production key
   grammar (`metadata::parse_orphan_key`); and
2. **a full fenced `reconcile_step` over this slice's two loops** (`:499-532`) certifies over the
   segmented store, with every fragment still at the server its committed placement names.

That is strictly *more* binding than the old form, because the old one would also have passed if the
pass had marked nothing for the wrong reason. The negative control still shows the zero is a
decision: a genuine stray is marked, and the ledger then names **exactly** it (`:563-568`).

---

## 4. The eight review findings — dispositions

**Fixed (6):**

| Finding | Fix | Bound by |
|---|---|---|
| `metadata.rs:2883` **BUG** ×3 — the caller's own `RootGeneration` was never checked for `Committed`, so a Pending root returned homes against the API's `Ok(None)` promise | the check moved to the **entry** of `resolve_chunk_homes`, where the contract is stated (`crates/core/src/metadata.rs:2832-2841`) | `crates/core/tests/segmented_map_resolution.rs:1012-1064` — flat **and** segmented |
| `reconstruction.rs:157` **BUG** — an empty repair queue short-circuited before the walk and certified | the walk runs first; the idle path emits the levels at a **measured** zero and returns non-certifying while any object is unreadable (`crates/custodian/src/reconstruction.rs:150-183`) | `segmented_map_repair.rs:1478-1543` |
| `metadata.rs:4311` **TEST-GAP** — the serialization-identity test respelled only the root | the `seg:` record is respelled too; both preconditions **and** the written record are asserted (`crates/core/src/metadata.rs:4266-4310`) | same test |
| `restore.rs:340` **TEST-GAP** — no test drove an unresolvable object through post-restore reconciliation | criterion (1)'s leg now seeds one: named on the audit seam, `is_clean` false, nothing marked, and a readable object's loss still reported (`segmented_map_repair.rs:570-625`) | same leg |

**Recorded-rejected (2, one finding seen twice)** — `reconstruction.rs:454`: *"preferring a readable
reference while other maps are unresolvable can repoint the chunk and drain its shared obligation"*.
Full reasoning in `review-rejected.md`; in short: the obligation is retired **because the repair
completed**, never because the pass concluded the chunk is referenced by nothing (that path is
`Found::Unassessable` and stays queued, `reconstruction.rs:468-470`); refusing every repair while any
record is damaged is precisely the starvation the brief's own containment invariant forbids; and the
pass reports non-certifying throughout, so nothing rests on the incomplete reading.

---

## 5. Refutation — every round-5 fix reverted on its own

Each mutation was applied alone, the named test run, then the file restored from a scratch snapshot
(`$PDCA_SCRATCH/pdca-builder-651-refute`, deleted at the end).

| Fix reverted to… | Test that went RED |
|---|---|
| `resolve_chunk_homes`: drop the entry-side `Committed` check | `resolving_homes_refuses_a_generation_that_is_not_committed` (1 failed / 1 passed of the pair) |
| `reconstruction::reconcile`: short-circuit `Satisfied` on an empty queue, before the walk | `an_idle_repair_pass_over_an_unreadable_object_still_refuses_to_certify` |
| `repoint_chunk`: `require(seg, encode(decode(prior)))` instead of the stored bytes | `a_repoint_pins_the_bytes_the_store_holds_not_a_re_encoding` |
| `ProtectedFragments::protects`: drop the incomplete-walk arm | `restore_over_segmented_objects_marks_nothing_and_keeps_every_fragment` — `stranded_marked: 3`, i.e. three fragments of the unreadable object handed to GC |
| `Reconciled::fold`: let `Blocked` collapse to `Satisfied` | 6 of the 12 legs |

### The three forced questions

**(a) Genuine red?** Yes — through the project's own runner, on the gate's own base:
`PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh` →
**`PASS — red without the fix, green with it`**, GREEN `12 passed; 0 failed`, RED `0 passed; 12
failed`. It is an **assertion/runtime** red, not a compile red: the first draft of this rebase *did*
go compile-red (the file named `Reconciled::Blocked` and `RestoreReport::unresolvable`, both of which
this patch adds on this base — on #650's tree `Blocked` already exists, which is why v4 never saw
it). That is the brief's explicit prohibition, so the discriminator was rewritten to assert the
*property* through base-visible symbols: `assert_not_certified` (`segmented_map_repair.rs:416-432`)
says "neither `Satisfied` nor `Changed`", which compiles on the pre-fix tree and fails there. The
report's `unresolvable` field is likewise observed through the audit seam instead
(`:593-601`). Per-fix reds are the table above.

**(b) Production path?** Yes. Every leg drives a real entry over the `MetadataStore`/`ChunkStore`
trait seams with in-memory doubles: `wyrd_custodian::reconcile_step` (the fenced control point),
`reconcile_after_restore`, `backfill::reconcile`. Placements are read back through
`metadata::resolve_current_chunk_map` (the production resolver), fragments verified with
`repair::fragment_intact`, payloads built with `erasure::encode` + `write::encode_ec_fragment`, the
orphan ledger read through `metadata::parse_orphan_key`. Nothing is re-implemented in the test. The
two `wyrd-core` tests drive `resolve_chunk_homes` / `repoint_chunk` themselves.

**(c) Fixture includes the fault?** Yes:
- the restore leg's store contains the **unreadable** object while the assertions are made (the
  refutation shows it would otherwise mark 3 of its fragments), and the ledger assertions are over
  the same store, not a curated one;
- the idle leg's queue is asserted **empty** before the pass, so the verdict comes from the walk;
- both ceiling legs seed a record that is **legal now** with **less headroom than the move needs**
  (asserted before the pass) — the legal→oversize transition iteration 1 was bounced for;
- the ambiguity leg seeds two committed objects over one chunk id with a genuine lost fragment;
- the counted legs (Q=9/N=3, N=2) assert the queue actually drained / the pass actually ran.

---

## 6. Budget — still over, with the numbers and the split

Non-blank, non-comment added lines (the reviewer's method), **13 files** (cap 15):

| | v4 | v5 | what moved |
|---|---|---|---|
| `crates/core/src/metadata.rs` — production | 236 | **243** | the entry-side committed check; `RootGeneration::decode` simplified |
| `crates/core/src/metadata.rs` — unit tests | 264 | **282** | the segment-CAS identity assertions |
| `crates/custodian/src/*` (5 passes + walk + step) | 288 | **389** | `ProtectedFragments` (+32), `Reconciled::Blocked`/fold (+23), idle-path walk, restore's own protection gate |
| `crates/core/tests/segmented_map_resolution.rs` | 58 | **88** | the entry-arm test |
| `crates/custodian/tests/segmented_map_repair.rs` | 1,048 | **1,148** | the idle leg, the restore-containment leg, `assert_not_certified` |
| `crates/dst/tests/custodian.rs` | 131 | **131** | property 11, unchanged |
| others (`lib.rs`, docs) | 3 | **2** | `segmented_map_consumers.rs` edits dropped |
| **total** | 2,077 | **2,282** | of which **1,649 is tests** (72%) |

Against the brief's ≤ ~1,500. It is over, this is the hand-back, and I finished the round rather than
stopping because (i) the budget is already a **deferred human decision** carried since round 3
(`deferred-findings.json`), (ii) the round-5 carry-forward asked for a rebase and a review triage,
and (iii) stopping would have left the gating C4 red for a sixth round. Nothing here pretends the
number is met.

**The split I would hand back** — cut at "see it" vs "move it":

* **A (this issue, reduced):** the shared walk + containment + the per-pass index (defect 2) +
  restore / backfill / desired-state adoption. Drops `repoint_chunk`, the ceiling helpers,
  `RootGeneration`, both moving passes' repoint, the DST property and the four ceiling/repoint legs:
  **≈ 1,150 semantic** (the rows above minus metadata production 243, metadata unit tests 282, dst
  131, and ~480 of discriminator legs).
* **B (new slice):** `repoint_chunk` + ceilings + `RootGeneration` + the repoint callers + property
  11 + those legs: **≈ 1,130 semantic**.

**Why I judge the split worse, not cheaper** (unchanged from v4): A ships a reconstruction that can
*see* a segmented chunk is under-replicated and cannot repair it, and a rebalance that can see a
fragment on a draining server and cannot move it — both then reporting non-certifying forever. That
is a pass reporting work it cannot do, which is the failure mode this slice exists to remove. It is
the human's call.

---

## 7. Verification

- **`./engine/scripts/run-verify.sh`** (`PDCA_VERIFY_BASE=origin/pdca-integration/main`) →
  **`PASS — red without the fix, green with it`**; `--classify` confirms the single discriminator
  `ADDED_TEST crates/custodian/tests/segmented_map_repair.rs`.
- **`./engine/xtask.sh ci`** (`cargo xtask ci`) → **`xtask ci: all checks passed`** — typos, docs
  lint + render + link audit, gitlink and unsafe guards, `cargo fmt --check`,
  `clippy -D warnings --all-targets`, build, the whole workspace test suite, machete, `cargo deny`,
  conformance, statics, orchestrator guard and the 50-seed madsim DST sweep, in which property 11
  (`segmented_repoint_loses_to_a_concurrent_supersede`) passes.
- **`scripts/mutants-in-diff`** (advisory C5) → **154 mutants tested in 3m: 56 caught, 98 unviable,
  0 missed** (v4: 141/0 missed; v3: 129/1 missed; v1: 62/18 missed).
- `cargo fmt --all` applied to every touched file; the target's commit hooks run the same formatter
  and clippy set `xtask ci` just ran.
- Scratch: `$PDCA_SCRATCH/pdca-builder-651-refute` (snapshots + the CI log), removed at the end.

## 8. Files touched (13)

| File | Why |
|---|---|
| `crates/core/src/metadata.rs` | `RootGeneration` (pins read bytes); `resolve_chunk_homes` (+ the committed contract at its entry); `repoint_chunk` + the record-ceiling helpers; canonical public `parse_inode_key`; unit tests |
| `crates/core/tests/segmented_map_resolution.rs` | the restart-arm test and the entry-arm test |
| `crates/custodian/src/resolve.rs` (**new**) | the shared per-pass maintenance walk, `object_name`, and the walk-derived protection set |
| `crates/custodian/src/reconstruction.rs` | per-pass `ChunkIndex` (defect 2), ambiguity, `Unassessable`, repoint-before-write, refusal, the idle-path walk |
| `crates/custodian/src/rebalance.rs` | homed walk, containment, repoint-before-copy, refusal, idle zero sample |
| `crates/custodian/src/backfill.rs` | one walk per pass, conditional decline with a stated reason, exact-bytes CAS |
| `crates/custodian/src/restore.rs` | one walk for both halves; the protection gate and the report both come from it; `RestoreReport::unresolvable` |
| `crates/custodian/src/desired_state.rs` | resolves through the walk; `PendingUnresolvable` attributes a stalled drain |
| `crates/custodian/src/reconciliation.rs` | `Reconciled::Blocked` + the step's fold rule |
| `crates/custodian/src/lib.rs` | `mod resolve;` |
| `crates/custodian/tests/segmented_map_repair.rs` (**new**) | the discriminator: 12 legs |
| `crates/dst/tests/custodian.rs` | property 11: a segmented repoint loses to a concurrent supersede |
| `docs/design/architecture/06-runtime-view.md` | §6.2 step 2: the repair/evacuation-walk paragraph |
