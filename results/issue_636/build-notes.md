# Build notes — issue 636 / multipart-commit-protocol (iteration 4)

Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `origin/pdca-integration/main` (wave-2 stack base, worktree HEAD
`d15d555 pdca-integrate: issue_635`). Every `path:line` below is against that tree **with this
patch applied**. Iterations 1–3 are preserved in `iteration-v1/`…`iteration-v3/`.

---

## 1. What this iteration is

Iteration 3 passed `C4-ci`, `C4-verify` and `T4-contribution`, and failed **one** gate:
`T4 batched multi-pass rubric review` — 21 blocking findings (`review-batch.md`), plus a C5
sign-off item (relational `mpuctl` validation and a corruption regression) and a C2 item (read the
negation output). So this round is again **not a redesign**: the record family, the state machine,
the knob derivations and the twelve success-criterion legs are iterations 1–3's and still hold, and
**the value set is unchanged** — §2 of `iteration-v1/build-notes.md` still states it; nothing here
re-chose a knob. Three constants are *added* (§3), none of them a re-choice.

What changed: **19 of the 21 findings fixed, 2 recorded-rejected** (one finding seen twice —
`review-rejected.md`), the C5 item fixed as the headline change, **twelve new crate-level tests and
four new unit tests**, and **twenty-five negation runs** (§4), every one of them behavioural.

The C5 item and the largest finding turned out to be the same shape twice: *a record whose content
authorizes work was trusted without checking the relation between its own fields*. `mpuctl` said
"my limit is 1,000" over a profile that derives 9; a `part:` record said "I am 65 bytes" over chunks
that span 64; a `sidx:` value said "reclaim me" under an EC scheme that names no fragment at all.
Each is now an error at decode, where the rubric puts it (`AGENTS.md:146-149`).

## 2. The findings, and what each actually was

Round 3's 21 rows dedupe to 16 distinct defects (`multipart.rs:2501` was reported three times,
`metadata.rs:4662`/`:4666`, `metadata.rs:2646` and `write.rs:849`/`:851` twice each). Ordered by
harm.

| # | Finding (line on iteration-v3) | What it actually was | Fix (line with this patch applied) |
|---|---|---|---|
| 1 | `multipart.rs:1477` **+ the C5 item** | `mpuctl` decoded with **no relation** between `max_sessions` and `profile`, and no clamp check on the profile itself — while admission deliberately enforces the *stored* limit so the whole fleet agrees on one number. A torn or forged ledger naming a bigger limit is therefore believed by **every** host, and `Σ_sessions(U_ref) ≤ W_ref` — the bound the reconcile pass is sized for — is exceeded where nobody observes it | `max_sessions == profile.max_sessions()` at decode (`multipart.rs:700-731`), and `knob_clamps_hold` applied to every `Budget` that arrives from the store (`multipart.rs:388-414`) |
| 2 | `multipart.rs:2501` ×3 | a re-upload of a **zero-chunk** part (a zero-length part is legal) installed `RetirePayload::Chunks { chunks: [] }`, which the payload's own decode rejects — an obligation the drain can never read, in the session's own range, so its teardown and its `mpuctl` decrement never happen | the obligation is installed only for a prior that names chunks (`multipart.rs:2977-2987`) |
| 3 | `multipart.rs:3096` | one absent re-read cannot settle an unknown **fence** commit (`may_still_commit`), and the fence is the one transition with no client resume: a fence that lands afterwards leaves the session `Completing`, where every verb answers `OperationAborted` and the only other exit is #625's reaper | the fence's own value is carried out as `IndeterminateCommit` (`multipart.rs:3430-3444`) and `CompleteParams::resume` adopts it by exact bytes (`:3324-3341`), which is 0016's stated "the owning gateway recovering its own unknown result at the same epoch" (`0016:952-961`) |
| 4 | `multipart.rs:2450` | a part-value size error returned bare, stranding slot **and** owned entries on a still-`Open` session — and the 302-byte chunk-ref estimate does not cover an arbitrary EC fanout, so a **count-legal** part can encode past `V`. The fixture proved a second half the finding did not name: the *compensation* carries the same chunk list, so it is over the ceiling too and the refusal cannot be written either | the ceiling is charged in **bytes**, from the plan (`multipart.rs:3078-3090`), again as the list grows (`multipart.rs:2530-2540`), and the commit's own check compensates (`multipart.rs:2887-2903`); `MAX_PART_VALUE_BYTES` / `max_part_chunks_for` at `multipart.rs:145-181` |
| 5 | `multipart.rs:2869` | after a lost release CAS, **any** `Completing` session was released — including a rival publisher's *newer* fence, rolled back on the authority of an error belonging to a request that was already over | the release retry requires the same `fence_epoch` the publish retry requires (`multipart.rs:3126-3151`) |
| 6 | `multipart.rs:2432` | `commit_part` took a **fresh** budget, so a caller could reserve under the fleet's ledger-approved caps and commit under larger ones of its own | the ledger-approved value set travels with the handle (`PartUpload::budget`, `multipart.rs:2043-2052`) and `commit_part` takes no budget at all (`:2769`) |
| 7 | `metadata.rs:4662`/`:4666` ×2 | `flip_retires` accepted **any** matching put, so a contribution could carry the right obligation and then overwrite it in the same batch — puts apply in order, so the *last* one is what survives the commit | the witness is the last put to that key (`metadata.rs:4707-4712`) |
| 8 | `metadata.rs:2646` ×2 | `StagedPlacement` decoded an EC scheme the coder cannot code (`RS(0,0)`), which names **no** fragment position — so `reclaim_owned` marks nothing and then deletes the only record naming those bytes: unreferenced *and* unevidenced, outcome (a), produced by the reclamation path itself | the coder's own predicate at decode (`metadata.rs:2665-2694`, `crate::erasure::supported`) |
| 9 | `multipart.rs:4577` | `reclaim_owned` committed the first owned entry **even when it alone exceeded the budget** — and unlike `drain_step`'s O(1) unit, that unit is the deployment's EC fanout, so the batch is permanently over the envelope: the walk re-derives the same shape for ever, the session is never torn down and its admission slot never returns | an entry that cannot fit alone is a named boundary error (`multipart.rs:4874-4895`), and `MAX_STAGED_FRAGMENTS` (`:184-195`) makes it unreachable for anything this protocol stages (`:2519-2528`) |
| 10 | `multipart.rs:1983` | an **absent** admission ledger was silently accepted while reserving a slot for a session that exists — but the create writes session and counter in one batch, so that is a torn store, and work would be admitted against an admission state that bounds nothing | read after the session and surfaced (`multipart.rs:2129-2146`) |
| 11 | `multipart.rs:1827`, `:2065` | the **settling re-read** after an unknown result used `?`, so a store that could not answer it returned the read's error and dropped the minted identity — the one thing that makes "re-drive THIS id" performable | best-effort read, identity always delivered (`multipart.rs:2074-2094`, `:2318-2349`) |
| 12 | `multipart.rs:397` | `knob_clamps_hold` admitted `max_inflight_parts == B_ops`, but the terminal delete carries five more operations — a teardown batch permanently over the envelope | `TERMINAL_DELETE_FIXED_OPS` in the clamp (`multipart.rs:437-449`, constant at `:503-513`) |
| 13 | `multipart.rs:1969` | admission checked only the live budget, so an unchecked retune could accept a part number the fixed-width key grammar cannot spell — a record that can be written and never read back | the grammar bound restated at the seam (`multipart.rs:2085-2098`) **and** the cause removed: a budget is validated before it can become the ledger's profile (`:1998-2005`) |
| 14 | `multipart.rs:744` | `PartRecord` did not check `len` against its own chunk list, though Complete sums `len` into the published `size` while the read path reads the chunks — a published object whose declared size is not the bytes a reader can get | validating decode (`multipart.rs:800-828`) |
| 15 | `multipart.rs:2123` | lease renewal stamped an expiry from an arbitrary clock — the #557/#565 mixed-clock class on the record whose only purpose is liveness | `clock_source` is a parameter and is checked (`multipart.rs:2391-2404`) |
| 16 | `write.rs:849`/`:851` ×2 | fragment writes have no `W_write` deadline | **recorded-rejected** — see `review-rejected.md`. The proposed fix does not close the named hazard: a caller-side timeout bounds only how long this process *waits*, not whether an accepted write lands. What closes it is the **server-enforced** deadline, which is #638's (this brief's declared dependency, `0016:1551-1576`), and the brief's *Prior-art check* forbids re-earning "a second timeout inside `core`'s fan-out". The protection half is already by design: `reclaim_owned` marks the chunk's **full planned placement** before deleting the entry, so a late fragment lands on a position already covered |

Two consequences worth the human's eye, both API shape:

* **`commit_part` loses its `budget` parameter** and **`renew_part_lease` gains `clock_source`**;
  `CompleteParams` gains `resume`. #508 inherits all three. Each removes a way for the wire layer
  to disagree with the ledger, the session's clock, or its own fence.
* **The per-part ceiling is now scheme-aware** (`max_part_chunks_for`). At the settled RS(6,3) it is
  *larger* than `MAX_PART_CHUNKS`, so the knob still binds and nothing about the settled deployment
  changes; at a wide fanout it binds first, which is the point.

## 3. The three constants added (no knob re-chosen)

| Constant | Value | Derivation |
|---|---|---|
| `MAX_PART_VALUE_BYTES` (`multipart.rs:152`) | `V/2` = 50 000 B | the very quantity `MAX_PART_CHUNKS = ⌊V/2 / MAX_CHUNKREF_BYTES⌋` divides (`0016:1050-1053`), now also stated in bytes because **two** records carry a part's chunk list (the `part:` record and its retirement obligation) |
| `MAX_STAGED_FRAGMENTS` (`multipart.rs:195`) | `(B_ops − 3)/2` = 498 | one owned entry's reclamation is `2n` mark operations plus its own `require`+`delete` under the session fence, in one batch. Far above any deployable width (RS(6,3) needs 9) — it exists to make "unreclaimable" unrepresentable |
| `TERMINAL_DELETE_FIXED_OPS` (`multipart.rs:513`) | 5 | `require(mpu)`, `require(mpuctl)`, `delete(mpu:)`, `put(mpuctl)`, `delete(seggrp:)` — the operations the terminal delete carries besides its slot deletes |

`W_REF`, `MAX_SESSIONS`'s derivation and every knob in `Budget::deployed()` are unchanged from
iteration 1.

## 4. Demonstrated red — twenty-five negation runs (**no gate consumes these**)

`build-notes.md` is withheld from the reviewer and no `[[gates.checks]]` row reads it, so this is
**sign-off evidence for the human**: please read the recorded output rather than treating the
C4-verify PASS as proof. Every negation was applied to one file by an exact-anchor edit (an edit
whose anchor does not match **aborts** rather than reporting a false red), run through cargo with a
900 s timeout, then reverted and the file verified byte-identical (`filecmp`, enforced by the
harness). The observed output is **transcribed below**; the harness and its 25 logs lived under
`$PDCA_SCRATCH/pdca-builder-636-neg4/` and were removed with the rest of this run's scratch (§9),
so this transcription is the record.

### The brief's three mandatory legs

**Leg F — collapse the two retry budgets into one** (`MAX_ADMISSION_CAS_ATTEMPTS: 64 → 4`):

```
test f_concurrent_creates_on_an_empty_store_all_succeed ... FAILED
8 concurrent creates against an EMPTY store: one was refused with
SlowDown { pressure: AdmissionContention { attempts: 4 } }. The admission ledger is nowhere near
its bound — this is the false `503 SlowDown` a single retry budget shared between the 2^-128
upload-id collision and the globally serialized admission CAS produced. 4 of 8 succeeded.
```

**Leg G1 — truncate derivation while declaring the obligation drained** (`finished = true` on a
budget-full batch):

```
test g_a_4001_part_complete_drains_to_empty_in_byte_budgeted_batches ... FAILED
8,002 record deletes under a 200-operation budget must take more than 40 batches, saw 1
```

**Leg G2 — a cursor that never advances within a phase** (re-expressed: the first spelling,
`cursor: None`, only produced a `-D warnings` dead-assignment error, which is not evidence of
behaviour):

```
test g_a_partially_drained_teardown_converges_with_no_double_decrement ... FAILED
called `Result::unwrap()` on an `Err` value:
DrainStalled { obligation: "retire:bytes:s:00000000000000000000000000000002:1" }
```

**Leg E1 — skip the terminal-delete decrement** / **E2 — apply it twice**:

```
E1: assertion `left == right` failed: the decrement happens in the terminal delete
      left: 2   right: 1
E2: assertion `left == right` failed: the decrement happens in the terminal delete
      left: 0   right: 1
```

### This iteration's own fixes, negated the same way

| Negation | Observed failure |
|---|---|
| `mpuctl`'s limit not checked against its profile | `a limit its own profile does not derive is a torn ledger: Some((b"{\"count\":1,\"max_sessions\":1001,\"profile\":{\"w_ref\":89760,…` |
| `Budget`'s clamps not checked at decode | `the refusal names the clamp: mpuctl records max_sessions=1 but its own profile derives 9 (U_ref=414480, W_ref=4000000)` |
| an empty `Chunks` obligation installed | `every obligation this protocol writes must be drainable: retire:bytes:s:…:0:1:… — a retire:bytes:{chunks} obligation names no chunks` |
| the fence's identity built but not delivered | `the fence value to re-drive with must reach the caller` |
| the release retry without the epoch check | `the rival publisher's fence is untouched: a finished request may not roll it back` — left `…"state":"Open","epoch":8…`, right `…"state":"Completing","epoch":7…` |
| `commit_part` using a locally derived budget | `the ceiling is the ledger-approved one` — left `Committed { part_number: 1, … }`, right `Refused(EntityTooLarge { chunks: 2, limit: 1 })` |
| no part-value byte budget (plan **and** accumulation) | `an over-value part is a typed refusal, not Committed { part_number: 1, … }` |
| `flip_retires` back to an any-put witness | `a near-miss retirement must not publish: Committed` |
| `StagedPlacement`'s scheme unchecked | `an entry that can evidence nothing must not be deleted: ()` (the walk deleted it) |
| the over-budget owned entry committed anyway | `a batch that could never commit is not a batch to commit: ()` |
| the reserve's grammar bound and torn-ledger check removed | `a part number the key grammar cannot spell is refused, not written` |
| an illegal budget allowed to bootstrap the ledger | `an illegal value set must not become the ledger's profile: Created(SessionSnapshot { … })` |
| the settling read back to `?` (create **and** reserve) | `the identity to re-drive with must reach the caller` |
| lease renewal without the clock check | `a lease renewed from another clock epoch must be refused` |
| `TERMINAL_DELETE_FIXED_OPS` dropped from the clamp | `max_inflight_parts == B_ops leaves no room for the fixed operations: ()` |
| `PartRecord`'s `len` unchecked | `a declared size its own chunks cannot serve must not decode: PartRecord { chunks: [… len: 64 …], len: 65, … }` |
| the staged fanout unbounded | `a chunk that could never be reclaimed must not be staged: Refused(NoSuchUpload)` |
| the staging error propagated without compensating | `the slot this attempt held is released, so the session stays usable` |

**Two negations changed the patch rather than confirming it**, which is why they are run:

* the first **part-value** negation did not merely fail the assertion — it revealed that the fix as
  first written (compensate on the encode error) *cannot work*, because the compensation obligation
  carries the same oversized chunk list and fails identically
  (`ValueOverCeiling { bytes: 121555, limit: 100000 }` raised **from the compensation**). The fix
  moved to where the list grows: the plan check and the per-chunk accumulation check. Without that
  run the patch would have shipped a refusal path that cannot execute on exactly the parts that
  need it;
* the first **over-budget entry** negation (`reclaimed >= 1 || true`) turned the walk into an
  infinite spin and hit the harness's 900 s timeout rather than reproducing the pre-fix behaviour.
  Re-expressed as the exact pre-fix shape (commit the over-budget batch), it fails on the
  assertion, which is what makes the fix bound.

Two more were **re-expressed** because their first spelling failed at compile time under
`-D warnings` (a dead-code / unused-assignment error is not evidence about behaviour): leg G2 above,
and the fence-identity negation.

### The C4-verify RED leg, measured honestly

Unchanged from iterations 1–3, and re-stated because it is a **declared** limitation of this
slice's shape: the red is **criterion-absence**. With production reverted and the two added test
files kept, the failures are build errors (`unresolved import wyrd_core::multipart`, `no field
'owner' on PendingEntry`), so **0 tests ran and 0 tests failed**. `run-verify.sh` scores a build
failure as a red without ever counting tests (its `TESTS_RAN == 0` guard sits inside the
cargo-*succeeded* branch, `engine/scripts/run-verify.sh:416-427`). **Stated plainly: the C4-verify
red is a build error, not a flipped assertion** — the load-bearing evidence is the twenty-five
negation runs above.

### The DST cases ran, and how many seeds

`crates/dst/tests/concurrency.rs` keeps its `#![cfg(madsim)]` and is a **modified** path (never an
added one), so it is evaluated by C4-ci, not C4-verify. In this iteration's final gate run,
`cargo xtask ci` → `run_dst`
(`RUSTFLAGS=--cfg madsim`, `MADSIM_TEST_NUM=50`, `xtask/src/main.rs:1575-1614`) reports
**12 tests where the base had 5**, including `multipart_publication_race_holds_on_sim_tikv`,
`concurrent_slot_reserves_never_exceed_the_cap_on_sim_tikv`,
`two_concurrent_drainers_over_one_owned_range_are_exactly_once` and
`a_segmented_publication_that_loses_the_root_flip_still_publishes` (iteration 2's answer to
round 1's C5 finding). Each `#[madsim::test]` is multiplied across **50** seeds; leg I(i) is a
`#[test]` that sweeps **48 seeds explicitly** and asserts both orders were reached, so its coverage
cannot go vacuous.

## 5. Before declaring done — the three forced questions

**(a) Genuine red? YES.** Twenty-five negation runs above, each reverted and re-verified
byte-identical, each producing a *named* behavioural failure (the four that first produced only a
compile error or a hang were re-expressed until they exercised behaviour). Two of them changed the
patch. Plus the C4-verify-shaped red, measured and labelled a build error with 0 tests run.

**(b) Production path? YES.** The tests drive `wyrd_core::multipart`'s real verbs (`create_session`,
`reserve_slot`, `stage_chunk`, `commit_part`, `upload_part`, `complete`, `abort`, `drain_step`,
`reclaim_owned`, `teardown_session`), the real `MetadataStore` seam, a real `PlacementChunkStore`,
`write::stage_intent`, `metadata`'s real committers and `read::read_path` for the read-back.
Nothing is stubbed except the *caller*, which is the test instead of #508's S3 handler. The one
place a test writes a record directly is where the **writer is another slice** (a forged `mpuctl`,
a torn `sidx:` value, #625's rollback obligation) — the code under test is still the production
path, and the assertion is on durable store state afterwards.

**(c) Fixture includes the fault? YES**, and this round's fixtures are built around the fault:
the forged-ledger leg *keeps the ledger at its cap and inflates only the limit*, so the test would
pass trivially if the ledger were merely absent; the settling-read leg breaks the **read** as well
as the commit; the fence leg makes the commit **land** and then breaks the read that would have
settled it, so the session really is in the wedged state before the re-drive; the rival-fence leg
injects a **newer** `Completing` record in the window between this caller's fence and its release;
the over-value leg uses a real 180-fragment scheme on a fleet of seven-digit D-server ids rather
than a hand-built record; the starved-drain leg uses a budget below one real entry's cost.

## 6. What the human should look at

1. **Two recorded rejections** (`review-rejected.md`), both the same `write.rs` finding. If the
   maintainer disagrees, the fix is #638's and this slice would need to be re-planned against it.
2. **#638 is not in this stack base.** The fold carries #634 and #635 only, so the staged fragment
   write has no server-enforced deadline yet — decision 5's late-fragment bound is caller-side
   until #638 lands. That is the brief's own *Ordering note*, and it is why the row above is a
   rejection rather than a fix. It is a **stacked-slice** dependency Plan already declared, not a
   missing tool, so it is not a NEEDS-HUMAN external dependency; it *is* a merge-order obligation.
3. **`C5-mutants` timed out with no verdict in iterations 2 and 3.** This round adds 16 fast tests
   and no new heavy fixture (the one wide-EC fixture stages 13 chunks, not 4,001), so the
   per-mutant cost is essentially unchanged; the campaign's cost is dominated by the three
   4,001-part fixtures iteration 1 introduced. If it times out again it is a **budget** question,
   not a signal about this patch.
4. **API changes #508 inherits**: `commit_part` drops its `budget` parameter, `renew_part_lease`
   gains `clock_source`, `CompleteParams` gains `resume`, and `Refusal` gains `PartValueTooLarge`.

## 7. Gates

* `cargo xtask ci` — **green** (EXIT=0), run twice on the exact tree `patch.diff` encodes,
  including the prose gates (`typos`, `lint_docs`, `render_site --check`), so the docs-currency
  edits to `docs/design/architecture/{06,08}` are genuinely gated rather than warn-skipped. Both
  external dependencies the brief declared (`typos`, `docs-renderer`) are installed on this host;
  nothing else was needed and **no NEEDS-HUMAN external dependency arose**.
* `cargo fmt --all -- --check` — clean, so the target's commit hook will not reject the patch.
* `cargo clippy --workspace --all-targets` and the `--cfg madsim` DST pass — clean under
  `warnings = "deny"` + `clippy::all = "deny"`. (Two clippy rows were fixed during this round:
  `filter(..).next_back()` → `rfind`, and a `type_complexity` alias in the test harness.)
* `git apply --check patch.diff` on a **pristine `d15d555`** worktree — applies cleanly.
* Test counts: `wyrd-core --lib` **138** (was 134), `multipart_protocol` **36** (was 24),
  `multipart_admission_and_drain` **6**, `dst/concurrency` **12** (base 5).

## 8. Self-review against the target's standing rubric (`AGENTS.md:122-210`)

* *One clock per correctness lifecycle* — now checked at **five** entry points (reserve, stage's
  slot rewrite via the handle, part commit, **lease renewal**, Complete, Abort); `core` still reads
  no clock, every instant arrives as an argument, and the drain bounds **work**, never time.
* *Metadata validation boundaries* — this round is mostly this rule: `AdmissionRecord`, `Budget`,
  `PartRecord` and `StagedPlacement`'s scheme are structural and are now errors at decode; the one
  deliberately *contextual* check (placement **length**) stays liberal on 0016's instruction, as
  recorded in `review-rejected.md`.
* *Serialization identity* — every new validating decode is a `Raw` mirror that preserves field
  order and all fields, so `decode → encode` stays byte-identical (asserted by
  `pending_entry_round_trips_byte_identically_in_both_shapes`, and relied on by every `require(k,
  prior)` CAS in the module); `deny_unknown_fields` matches the file's existing idiom
  (`SegmentGeneration`) and every one of these record classes is new in this slice.
* *Absent or unsupported entries* — four more silent paths became explicit errors
  (`AdmissionLedgerMissing` at the reserve, `OwnedEntryOverBudget`, `StagedFanoutOverBudget`,
  `StagedPartOverValueBudget`, `BudgetIllegal`).
* *Transactions* — `upload_part` now compensates before **any** early return over staged work, and
  the two indeterminate-commit paths deliver their identity rather than a read's error.
* *Await discipline* — unchanged, and the one finding against it is recorded-rejected with the
  reason the fix belongs to the seam (#638).
* *Test fidelity* — the DST campaign is unchanged and still covers the three races leg I names; the
  new crate-level tests observe durable store state, not just return values.
* *Docs currency* — no persisted field or record class is added this round (validation only), so
  `06-runtime-view.md` / `08-crosscutting-concepts.md` keep iterations 2–3's paragraphs.

## 9. Scratch

Everything this run created lived under `$PDCA_SCRATCH` (`/var/tmp/pdca`) and was named
`pdca-builder-636-*`: four `cargo xtask ci` logs, the negation directory
(`pdca-builder-636-neg4/` — 25 logs plus the harness), one probe dir, and three throwaway
`git worktree`s (two apply-checks and one iteration-v3 tree for the self-review delta). **All
removed** — the worktrees with `git worktree remove`, the rest with `rm -rf`; `git worktree list`
shows none left. No `/tmp` use. Iterations 1–3's own `pdca-builder-636-*` logs are left where they
were: they are not this run's to delete.

## 10. Verification trail (paste-ready)

```
./engine/xtask.sh ci                                          -> EXIT=0 (twice, final tree)
cargo fmt --all -- --check                                    -> clean
cargo test -p wyrd-core --lib                                 -> 138 passed
cargo test -p wyrd-core --test multipart_protocol             -> 36 passed
cargo test -p wyrd-core --test multipart_admission_and_drain  -> 6 passed
RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test concurrency -> 12 passed (50 seeds each)
25 exact-anchor negations, each reverted + verified byte-identical -> every one RED (§4)
git apply --check patch.diff on a pristine d15d555            -> applies cleanly
```
