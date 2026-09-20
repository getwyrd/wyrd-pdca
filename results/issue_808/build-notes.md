# Build notes — #808 (664.1) staged-drain-status

Withheld from the reviewer; written for the human at sign-off.

All `path:line` citations are against the target branch as checked out in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`), which is at **`97fc2f9`** — `origin/main` after #812
(the `804-gc-reclaim-intent-and-mark-shapes` merge) landed on top of the brief's
`f41e9c5`. The brief's line numbers were taken at `f41e9c5`, so two of them have drifted
and I cite the current ones:

| brief cites (`f41e9c5`) | now (`97fc2f9`) |
|---|---|
| `gc.rs:669` (`deferred: #663, #664`) | `crates/custodian/src/gc.rs:893` |
| `gc.rs:690 / :705 / :809` (`StagedSet`) | `gc.rs:914 / :929 / :1033` |
| `desired_state.rs:181-196` | `crates/custodian/src/desired_state.rs:181-197` |
| `rebalance.rs:257` (`plan_evacuations`) | `crates/custodian/src/rebalance.rs:257` (unchanged) |
| `06-runtime-view.md:78` | `docs/design/architecture/06-runtime-view.md:80` |

#804 is listed in the brief's **Conflicts with**; it is already merged into this base, so
there was no conflict to resolve — its hunks are in the tree I built on.

---

## 1. What the change is

**One production behaviour change, in one function.** `reconciliation_status` now reads the
staged protection class beside the committed reference set and counts a staged fragment as
held (`crates/custodian/src/desired_state.rs:225-240`).

```
desired_state.rs:225   let staged = staged_fragments(meta).await?;      // staged FIRST (0016:793-800)
desired_state.rs:226   let referenced = referenced_fragments(meta).await?;
desired_state.rs:232-237  genuinely_holds = referenced.placed ∪ staged.placed  names dserver  → Pending
desired_state.rs:275      referenced.unresolvable ∪ staged.unresolvable  non-empty → PendingUnresolvable
desired_state.rs:302      referenced.malformed    ∪ staged.held          non-empty → PendingMalformed
desired_state.rs:298      otherwise                                                → Satisfied
```

It **reuses the existing class** — `crate::gc::staged_fragments` / `StagedSet`
(`gc.rs:1033`, `gc.rs:896-935`) — rather than rebuilding one, which the brief's Scope
requires and which is also why no new public symbol appears anywhere. That matters for
falsifiability: the shipped test names only base-visible symbols, so it compiles against
`f41e9c5`/`97fc2f9` and its red is an **assertion** failure, never a compile error.

**No new enum variants.** The two damage classes map onto the two answers the committed
class already uses, which is exactly what the brief's leg (E) asks for ("mirror
`StagedSet::protection`"):

| staged condition (`StagedSet`) | answer | the committed rule it mirrors |
|---|---|---|
| `placed` names `dserver` | `Pending` | a valid committed placement names it |
| `unresolvable` non-empty | `PendingUnresolvable { objects }` | `ReferenceSet::unresolvable` |
| `held` non-empty | `PendingMalformed { chunks }` | `ReferenceSet::malformed` |

The other four files are consequences, not separate changes:

* `crates/custodian/src/rebalance.rs:231-250` — a doc paragraph on `plan_evacuations`
  recording that staged bytes are out of its scan **by construction** (it scans `inode:`),
  that this is the design and not an omission, and that `reconciliation_status` is the
  operator's verdict while `Reconciled::Satisfied` is the loop's answer about its own
  question. **No behaviour change** — see §3.
* `crates/custodian/src/gc.rs:893-895` — #664's half of the `deferred: #663, #664` marker
  discharged, leaving #663's. `gc.rs:872-877` and `gc.rs:264-266` updated where they said
  "the drain-status query reads no upload record", which is no longer true.
* `crates/custodian/tests/staged_protection.rs` — leg (F) narrowed to scrub; see §4.
* `docs/design/architecture/06-runtime-view.md:80` — the living-doc sentence "Scrub and the
  drain-status query read committed references only" corrected (rubric: *Docs currency*).

## 2. Why this shape, and what I ruled out

**Merging staged placements into `ReferenceSet::placed`** — one set, one read, the smallest
possible diff in `desired_state.rs` (about 3 lines instead of ~30). Rejected on the design,
not on cost: 0016's own failure table names it as a way to implement this wrong
(`0016:881`, "Merge staged fragments into `placed` instead of a disjoint set — rebalance's
evacuation plan for a draining server holding only staged fragments MUST be empty, while
`reconciliation_status` for that server MUST be `Pending`"). Merged, `plan_evacuations`
(which reads the committed namespace) and GC's safety gate would inherit staged entries
with committed-set semantics, and rebalance would start planning moves it must not perform.
It would also break the per-class attribution — `StagedSet::protection` returns `"staged"` /
`"untrusted-staged-record"` where `ReferenceSet` returns `"referenced"` /
`"malformed-placement"`, and a merged set files a staged skip under a committed reason.

**Adding `PendingStaged` / `PendingUnresolvableStaged` variants.** Rejected: it adds public
symbols the test would have to name, which loses the compile-on-base property the brief's
Falsifiability section requires, and it splits one operator instruction ("a record is
blocking every drain — here it is") into two for no operator benefit. The class is already
carried in the *name* the answer gives: `inode:1` vs
`part:e1e1…:000002` tells a human which namespace to repair. The distinct **audit action**
(`unresolvable-staged-record`, `desired_state.rs:348`) carries it on the seam too.

**Emitting on the seam for the `held` (untrusted) case.** Deliberately not done, for
symmetry: the committed twin (`referenced.malformed`) is not emitted from this query either
— GC owns that emission (`gc.rs:1607` `emit_untrusted_staged`, `gc.rs:1592`
`emit_unresolvable_staged`) — and both blockers are named in the answer itself. Emitting
one and not the other is the inconsistency a reviewer would (rightly) flag.

**Ordering.** Staged is read **before** committed (`desired_state.rs:222-226`), the order
`gc::reconcile` uses (`gc.rs:281`) and for the same documented reason (`0016:793-800`): a
publication moves a chunk's protection from its `part:` record onto a committed inode, so
reading inodes first can miss the flip and then miss the part record the publication
deleted, seeing the chunk in neither. Getting this backwards would be a real (if narrow)
hole, not a style point.

**Cost, stated plainly.** This query now issues the staged reading on every poll: one paged
listing of `mpu:` plus two bounded ranges per session (`0016:890`,
`gc.rs:1033-1066`) — the same bounded reading GC already does every pass, bounded by the
same `MAX_SESSIONS` admission budget. There is no cheaper way to be right about a live
upload's bytes: the information is only in those records. A store fault under that reading
propagates as `Err` exactly as a fault under the committed reading already does; a damaged
*record* is contained, never an `Err` (`desired_state.rs:215-217`).

## 3. Rebalance: which `Reconciled` it returns, and why that is not "the drain is done"

The brief asks for this explicitly.

For a draining server holding **only** staged fragments, `rebalance::reconcile` returns
**`Reconciled::Satisfied`**. The path: `plan_evacuations` scans `b"inode:"`
(`rebalance.rs:281`) — an upload's records live under `mpu:` / `part:` / `sidx:`, so no
plan is built, `EvacScan::withheld` stays `false`, no move runs so `unmoved` stays `false`
and `changed` stays `false`, and `rebalance.rs:199-204` therefore answers `Satisfied`.

That is honest, and it is **not** the operator's drain verdict:

* `Reconciled` is *one loop's answer about its own pass* — "reality already matched the
  desired state" for the namespace **that loop** reconciles, the committed one. The base
  already says this in as many words at `rebalance.rs:194-196`: "The operator's per-server
  query (`crate::desired_state::reconciliation_status`) stays the authority on *which*
  server is still referenced; this is one loop's answer about its own pass."
* The surface an operator reads before pulling a box is `reconciliation_status`, which now
  answers `Pending` for that same server. Leg (D) asserts both in one test, over one store,
  in one pass — which is the only way the "these two must agree" contract of `0016:881` can
  actually be checked.
* The staged bytes have an exit that is not an evacuation: the session publishes, aborts,
  or is reaped within `W_session` (0016 decision 2, "Bounding the drain stall"). Rebalance
  repointing a `part:` record from outside the session fence would race the upload that owns
  it and buy no durability; a staged chunk's placement is rewritten by reconstruction under
  the session precondition (`0016:875`), not here.

So **`plan_evacuations` needed no real change** — the brief's expected outcome ("a test and
a comment, no behaviour change"). What I added is the comment
(`rebalance.rs:231-250`) plus the test that pins it, because "disjoint by construction" is
the kind of property that is true until someone widens the scan.

## 4. The one peer test I had to change

`crates/custodian/tests/staged_protection.rs`'s leg (F) asserted that scrub **and** the
drain-status query answer identically with and without upload records, and that neither
reads under `mpu:` / `sidx:` / `part:`. That is exactly what this slice makes false for the
drain half — and #803 anticipated it: the in-file marker read `deferred: #663, #664 — those
slices add upload records to scrub and to drain status, and own changing this leg`
(`staged_protection.rs:2160` at `f41e9c5`).

I discharged #664's half and left #663's:

* `Answers` loses its `drain` field; `scrub_and_drain_status` → `scrub_answers`; the test
  renamed `scrub_and_drain_status_do_not_read_upload_records` →
  `scrub_does_not_read_upload_records`. The scrub half — all eight fixture variants,
  including the three armed store faults — is unchanged and still green.
* The marker becomes `deferred: #663`, and the doc says where the drain half went.
* `reconciliation_status` / `ReconciliationStatus` dropped from that file's imports (they
  would be unused, and the workspace builds with `-D warnings`).

Had I left leg (F) alone it would have failed two ways: `Healthy` and the damaged-record
fixtures answer differently now, and `SessionListingFails` / `OwnedRangeFails` /
`PartRangeFails` make the query return `Err`, which that helper turns into a panic. No way
to keep it; the marker says this slice owns changing it.

`f_store` still calls `set_lifecycle` — the fixture's shape is unchanged, so the
scrub-only comparison is still the same comparison it was before.

## 5. The test — red→green, and the three refutation questions

**File:** `crates/custodian/tests/staged_drain_status.rs` (NEW, as the brief requires — the
C4-verify gate earns its red only from an added `*/tests/*.rs`). 7 tests. No `Cargo.toml`
change; every dev-dependency it uses (`tokio`, `async-trait`, `bytes`,
`wyrd-coordination-mem`, `wyrd-testkit`, `wyrd-chunk-format`, `tracing-subscriber`) is
already declared.

**Red count: 6 of 7 failed with the production change reverted, every one by assertion**
(no compile error — verified by the runner reporting `running 7 tests` on the red leg,
which it cannot do if the binary failed to build):

| test | leg | reverted | with the fix |
|---|---|---|---|
| `an_in_flight_owned_fragment_holds_the_drain` | A | FAILED (`Satisfied`, want `Pending`) | ok |
| `a_committed_parts_fragment_holds_the_drain` | B | FAILED (`Satisfied`) | ok |
| `a_server_holding_none_of_the_staged_bytes_still_drains` | C | **ok** (the guard — green on base by design) | ok |
| `every_server_that_does_carry_staged_bytes_holds_its_own_drain` | C-liveness | FAILED (all three `Satisfied`) | ok |
| `rebalance_leaves_staged_bytes_alone_while_the_drain_stays_pending` | D | FAILED (`Satisfied`, want `Pending`) | ok |
| `a_staged_record_the_query_cannot_read_blocks_every_drain` | E(i) | FAILED (`Satisfied`, want `PendingUnresolvable`) | ok |
| `a_staged_record_the_query_cannot_trust_blocks_every_drain` | E(ii) | FAILED (`Satisfied`, want `PendingMalformed`) | ok |

Red leg: `1 passed; 6 failed`. Green leg: `7 passed; 0 failed`.

**Why leg C is split into two tests.** The brief says C is *green on the base* — a guard.
My first draft folded its liveness check ("the three servers that DO hold staged bytes
answer `Pending`") into the same test, which made it red on base and cost it its guard
property. Split: `a_server_holding_none_of_the_staged_bytes_still_drains` holds only the
`Satisfied` assertion and is green on base and under the fix;
`every_server_that_does_carry_staged_bytes_holds_its_own_drain` runs over the **same
fixture function** (`seed_three_holders`) and supplies the liveness, red on base. The
mutant the brief names (`*server != dserver`) is killed by the guard: under it, staged
fragments on 0/1/2 would make server 3 answer `Pending`.

### (a) Genuine red?

**Yes** — measured, not assumed. Procedure: `cp` the fixed `desired_state.rs` to scratch,
`git checkout -- crates/custodian/src/desired_state.rs` (the only file with a behaviour
change), re-run, restore. Table above. Note the revert leaves the `rebalance.rs` /
`gc.rs` comment edits in place, which is the point — they are comments, so they cannot
carry the red.

I also re-ran `staged_protection.rs` with the production reverted: `26 passed`. So the peer
test's edit in §4 is itself consistent with the base, which is what the C4-verify gate
needs (it reverts production and keeps test files).

### (b) Production path?

**Yes.** Every leg calls the real exported `wyrd_custodian::reconciliation_status`; leg (D)
drives the real fenced control point `reconcile_step` with a real `RebalanceContext` over a
`MemCoordination`-elected `Custodian`. Nothing is mocked or re-implemented: the only doubles
are the two trait seams the loops are *designed* to run over (`MetadataStore`, `ChunkStore`),
which is the same Option-A shape every other custodian test uses (`0005:519-523`). The
staged records are seeded because no client can create a session until #508 — the brief's
own "Production reach" note says so.

Every seeded record is round-tripped through the **production decoder** before a pass reads
it (`decode_session_record` / `decode_part_record` / `decode_owned_entry`,
`staged_drain_status.rs:263-315`), and the session and part records additionally assert
byte-identity against `metadata::encode`, so a fixture cannot drift into a shape the real
decoders would reject.

### (c) Fixture includes the fault?

**Yes**, and this is where I spent the most care:

* Legs A and B assert `meta.records_under(b"inode:").is_empty()` before querying — so the
  `Pending` cannot be a committed reference sneaking in. The failing element (the staged
  fragment on the draining server) is the *only* thing in the store that can produce it.
* Leg C's guard is paired with the liveness test over the same fixture, so "`Satisfied` for
  server 3" can never be the query simply not seeing the staged records.
* Leg D's fixture contains the fault twice over: the draining server holds one `part:` and
  one `sidx:` fragment, and a **committed object is seeded on a non-draining server** so the
  pass has a real namespace to walk. Then the control appends a committed fragment **on the
  draining server** and the same pass, same context, returns `Reconciled::Changed`, copies
  the fragment onto a non-draining server, and repoints the placement record — so the
  "wrote nothing" assertions above it are demonstrably not the verdict of a pass that could
  not write. (The source copy stays put and is orphan-marked for GC's grace window, so that
  half is asserted on the *record*, not on the source bytes — `rebalance.rs:560-566`.)
* Leg E(i) damages a real `part:` record (a value that will not decode under a key the
  parser accepts) and asserts the block on **two servers that hold nothing at all** — the
  cluster-wide claim, over the servers a base build certifies. It also reads back the audit
  line from a `tracing` capture and asserts the seam, the action, and the record name, so
  the new `emit_unresolvable_staged` is executed and checked rather than merely present.
* Leg E(ii) damages a real `sidx:` record (an `RS(2,1)` chunk with a one-server planned
  placement — accepted at decode, held by `StagedSet::place`) and then seeds a malformed
  *committed* placement beside it, asserting both blockers arrive in one sorted list. That
  second half is what exercises the `chain` + `sort_unstable` + `dedup` at
  `desired_state.rs:303-309`; without it those lines would be reachable but unasserted.

## 6. Gates

* `cargo xtask ci` (fmt + clippy `-D warnings` + build + test incl. DST + cargo-deny +
  conformance), run through the project's own wrapper `./engine/xtask.sh ci` with
  `PDCA_WORKTREE` pointed at the lane: **`xtask ci: all checks passed`**, exit 0.
  (Run twice: once mid-build and once on the final tree after a last comment-only edit.)
  Log: `$PDCA_SCRATCH/pdca-builder-808-redleg/ci-final.log`.
* `cargo fmt --all -- --check`: clean — the patch is commit-ready for the target's own
  hooks.
* `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean. One finding fixed
  during the build (`unnecessary_sort_by` in the test's `inventory` helper).
* `cargo test -p wyrd-custodian`: all green, including the 26 legs of `staged_protection.rs`
  after the §4 edit.

No external dependency was missing: `typos` and `docs-renderer` (the brief's
`External dependencies`) are both exercised inside `cargo xtask ci`, which passed. Nothing
to declare as NEEDS-HUMAN.

## 7. Iteration-1 carry-forward

The previous attempt's blocking gate was `cargo xtask ci` → `cargo test --workspace
--exclude wyrd-dst` exit 101, attributed at sign-off to "an unrelated health test" whose
confirmation run timed out, leaving criterion F unproven. I did not re-use that attempt's
patch (I did not read it), and I did not rely on a partial run: criterion F here is a
**complete** `cargo xtask ci` on the final tree, green end-to-end, twice. I saw no flake in
either run.

## 8. Scope discipline

Untouched, as the brief's out-of-scope list requires: `crates/custodian/src/restore.rs`
(including its own `deferred: #664` marker at `restore.rs:819`, which is about
`RestoreReport::needs_human` and lives in a file the brief puts out of scope),
`crates/core/src/multipart.rs`, `crates/server/src/cli.rs`, `scrub.rs`,
`reconstruction.rs`, `crates/dst/tests/custodian.rs`, and proposal 0016 / every ADR. I
checked that no other caller of `reconciliation_status` exists outside
`crates/custodian/` (`grep -rl` over the workspace: only the custodian source and its own
tests), so no other crate's behaviour moves.

## 9. Housekeeping

Scratch used: `$PDCA_SCRATCH/pdca-builder-808-redleg/` (the saved copy of the fixed
`desired_state.rs` for the revert cycle, and the two CI logs). No files were written
outside `$PDCA_WORKTREE` and the bundle directory. `git add -N` was used on the new test
file so `git diff` would include it in `patch.diff`; nothing is staged for commit and no
branch was created, pushed, or PR'd.
