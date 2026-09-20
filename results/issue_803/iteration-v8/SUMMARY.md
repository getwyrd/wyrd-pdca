# Result — issue 803 / staged-protection-gc-restore

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged bytes have no protection class. `ReferenceSet` holds committed placements
  only (`crates/custodian/src/gc.rs:383-413`), built from the `inode:` scan alone (`:478-573`).
  A committed part's fragments (`part:`) and an upload's in-flight owned fragments (`sidx:`,
  #772) are in no protected set, so GC reclaims one as soon as it carries an `orphan:` mark past
  grace (`:272`, `:277-326`). Restore gates on the same predicate
  (`crates/custodian/src/restore.rs:385`) and its pending skip (`:435-438`) no longer sees owned
  entries, so it marks a live upload's fragments stranded (`:440-443`) and the next GC pass
  deletes them.
- Success criterion: the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Records are seeded as raw JSON the base decoders accept (`SessionRecord`
  and `PartRecord` have no writer-side constructor, `crates/core/src/multipart.rs:2127`,
  `:2492`; shapes as the helpers in `crates/core/tests/multipart_session_records.rs:81-141`,
  with the `Completed` state spelled as at `:298-304`), each round-tripped through
  `decode_session_record` / `decode_part_record` / `decode_owned_entry` first. Every protection
  leg also seeds an unprotected control the pass does reclaim or mark. Legs:
  **(A) GC protects both staged classes, in every session state.** For each of an `Open`, a
  `Completing`, an `Aborting` and a `Completed` session: a committed `part:` record (fragment
  `F1`) and an owned `sidx:` entry (`F2`) on D-server doubles, each with an `orphan:` mark past
  grace. After `reconcile_step` with a `GcContext`, every one survives. (Unmarked, GC's
  conservative arm keeps any fragment, `gc.rs:307-310`, so the mark is what makes the leg bite.)
  Base: all reclaimed.
  **(B) Restore protects them through the same rule.** `reconcile_after_restore` over the same
  store, unmarked, writes no `orphan:` key for any staged fragment and `stranded_marked` counts
  the control alone; a GC pass past grace then keeps every staged fragment. Base: marked, then
  deleted. (Staged counters are #664's.)
  **(C) Source before destination, both handoffs (`0016:782-800`, X67 `:2596`).** A double
  acts right after the first of the two reads involved completes — the source range or the
  destination range/scan, whichever GC issues first: (i) a part commit, ONE atomic batch that
  deletes the chunk's `sidx:` entry and writes its `part:` record (`0016:782-784`; source
  `sidx:<id>:`, destination `part:<id>:`); (ii) a publication, which is TWO batches, not one
  (`0016:793-800`, `:941-944`, `:964-966`): first the root flip — the session goes to
  `Completing` before it, and the flip batch writes the committed inode naming the chunk and
  moves the session to `Completed`, leaving the `part:` record in place; then, as a separate later
  batch, the retirement drain deletes that `part:` record (source `part:<id>:`, destination the
  `inode:` scan). Leg C(ii) runs two schedules: flip AND drain both between the two reads (the
  only one a destination-first build sees in neither class), and flip between the reads with the
  drain after the second. The double omits the `retire:records:` obligation key itself: no pass
  in this slice reads `retire:` (#804's), and the drain batch is what deletes the record. In every
  schedule the fragment is marked past grace and is not reclaimed. Base: reclaimed.
  **(D) Bounded per-session reads (`0016:890`).** With the `scan` cap lowered, more
  sessions-with-parts than a global `scan("part:")` could return: `reconcile_step` (GC) and
  `reconcile_after_restore` both succeed, and the double records no `scan`/`scan_page` of the
  bare `part:` or `sidx:` prefix. A guard.
  **(E) What GC and restore cannot read or trust fails closed (ADR-0045 decision 3,
  `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).** (i) A `part:` value that
  will not decode; a key inside a listed session's `part:<id>:` range that `parse_part_key`
  rejects, seeded with a value `decode_part_record` accepts (e.g. an unpadded part number — key
  and value are validated separately, `crates/core/src/multipart.rs:1279`, `:2578`); an `sidx:`
  key naming no chunk; or an `mpu:` key naming no upload (one test each): GC reclaims
  nothing and answers `Reconciled::Blocked` (as `gc.rs:348-355`); restore marks nothing and names
  the record in `RestoreReport::unresolvable`. (ii) A staged placement of the wrong length, or an
  undecodable owned value under an `sidx:` key that names its chunk: that whole chunk is held in
  both passes and the record is named on each pass's audit seam, while unrelated fragments are
  still judged. (iii) A store fault: the metadata double fails the read of exactly one of `mpu:`,
  a session's `sidx:<id>:`, a session's `part:<id>:` (one test each). GC and restore both return
  `Err` whose text names the failed read, every staged fragment is still on disk and unmarked, and
  the double's read log shows the faulted read was issued. Healed, the same store runs a clean GC
  pass that still keeps them (the control). Base: reclaimed or marked; base restore returns `Ok`.
  **(F) Scrub and drain status do not see upload records.** One store holds a committed object
  whose only fragment is missing from its D server, a committed object with fragments on server
  `S` (draining, via `set_lifecycle(.., DServerLifecycle::Draining)`), and a listed session with a `part:` record and an owned
  `sidx:` entry whose fragments sit on servers other than `S`. Compared with the same store minus
  the upload records, `reconcile_step` with a `ScrubContext` gives the same `Reconciled` and
  leaves the same `repair:` key for the missing chunk (`wyrd_core::repair::repair_key`), and
  `reconciliation_status(S)` gives the same answer. That holds with the upload records healthy,
  with each E(i) damaged record in place (neither call answers `Blocked` or
  `PendingUnresolvable` on its account), and with each E(iii) store fault armed (neither call
  returns `Err`). The double logs no read under `mpu:`, `sidx:` or `part:` from either call. A
  guard: green on base, red against the rejected design where all four consumers share the
  staged read. Mark the leg `// deferred: #663, #664` — those slices add upload records to scrub
  and drain status and own changing it.
  **(G) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53`; no new DST file): a concurrent part commit, then the publication
  flip (session to `Completing` before, `Completed` within the batch that writes the inode), then
  the retirement drain's deletion of the `part:` record as its own batch — each at its own
  seed-chosen instant during GC's reads — never gets the chunk reclaimed; a coverage property
  proves landings between and outside those reads are both reached, as `prop_restore_two_readings_cover_the_divergence_window` (`:2139-2173`)
  does. Registered in the campaign as its neighbours are.
  **(H) `cargo xtask ci` green** (it runs the madsim DST suite and leg I).
  **(I) The operator verdict names staged records as staged.** A new `#[test]` in
  `crates/server/src/cli.rs`'s own test module, beside `:2978-3011` (`restore_verdict` is private
  to the server lib, so no integration test can reach it). A `RestoreReport` whose `unresolvable`
  is `["inode:7", "mpu:<id>", "part:<id>:000001", "sidx:<id>:000001:9"]` (keys as restore names
  them, `gc::object_name`, `gc.rs:588`): `restore_verdict(&report).needs_human` is `true`; the
  printed lines contain `INCOMPLETE`, `4 record(s) UNREADABLE`, `4 record(s) could not be READ`,
  each of the four names, `staged multipart record`, `action=unresolvable-chunk-map` and
  `action=unresolvable-staged-record` (the audit action restore's staged arm emits — use this
  exact string in `restore.rs` too); they contain neither `committed object(s) UNREADABLE` nor
  `committed object(s) could not be READ`. A second report holding only `part:<id>:000001`: the
  same, with `1 record(s)`. The existing test's total-count phrase (`:3004-3010`) moves to
  `record(s) could not be READ`; its other asserts stay. Base: red by assertion — base prints
  `4 committed object(s) could not be READ` for all four names (`cli.rs:1331-1341`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the staged protection class, for the two passes that delete or mark: GC's reclaim
  and the post-restore mark gate. It covers the committed `part:` records and owned `sidx:`
  entries of every session listed under `mpu:`, whatever its state (0016 counts fewer; covering
  more only keeps more), read through each session's own bounded ranges and never a global
  `part:` or `sidx:` scan; within each pass's reading, `sidx:` before `part:` before the `inode:`
  scan. The class is disjoint from committed placements and has its own audit reasons. A staged
  record that cannot be read makes the set incomplete for GC and restore (E(i)); one that reads
  but cannot be trusted holds its chunk (E(ii)); a store fault fails the pass (E(iii)). The human
  accepted at sign-off that one unreadable upload record stalls GC and restore fleet-wide until it
  is repaired — keep that. **Scrub and drain status stay as on `main`:** they read no upload
  record, so an upload record's damage, a fault reading one, or the cost of reading them cannot
  reach their answers (F); the committed reference build they share (`gc.rs:478-573`) keeps its
  behaviour. **Restore** names each staged record it cannot read in `RestoreReport::unresolvable`
  (the existing field, so the report is not clean), and each record it holds as untrusted on its
  audit seam; whether a held record sets `needs_human()` is #664's, marked `// deferred: #664` at
  the site. Restore's docs claim only what the code does: its protection covers upload records
  already durable when the pass read them, and a write or upload that starts mid-pass is #805's,
  marked `// deferred: #805` where the pass reads them. The post-restore command's summary line
  and NEEDS-HUMAN paragraph (`crates/server/src/cli.rs:1263`, `:1331`) name staged records beside
  committed objects, with the text leg I pins; the runbook's UNREADABLE entry
  (`docs/design/architecture/m4-first-deployment-blueprint.md:609`) says the same. No change to the signatures of `reconcile_step`,
  `reconcile_after_restore` or `reconciliation_status`; no new field on a context struct or
  `RestoreReport`. Docs: one paragraph in `docs/design/architecture/06-runtime-view.md` §6.7 step
  2 (`:74`) — GC never reclaims, and restore never marks, a staged fragment; scrub and drain status
  read committed references only. / out of scope: mark shapes, reclaim intent, retirement
  protection (#804); drain status and rebalance reading upload records, restore's staged counters
  and fence (#664); scrub and reconstruction reading them (#663); restore's mid-pass window for
  writes and uploads (#805); the fragment-less sweep (#800); any edit to `desired_state.rs`,
  `rebalance.rs`, `scrub.rs`, `reconstruction.rs`, `crates/core/src/metadata.rs`,
  `crates/core/src/multipart.rs`; 0016 and the ADRs.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (23 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.8% — 249 of 252 instrumentable changed lines executed (floor 80%); 252 of 628 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 84s: 9 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.89s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: protect multipart `part:` and `sidx:` staged fragments from GC and post-restore marking across handoffs, without changing scrub/drain behavior, and report unreadable staged records accurately.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes both staged classes, both destructive consumers, handoff ordering, failure containment, unaffected consumers, and operator output independently falsifiable; the corresponding boundaries are explicit at `crates/custodian/src/gc.rs:641` and `crates/custodian/src/restore.rs:233`. |
| C2 Reproduction (red pre-fix) | PASS | With the new test retained and production reverted, the base compiled and failed 21 of 23 assertions while the two intended guards stayed green, reproducing both reclaim and marking failures exercised at `crates/custodian/tests/staged_protection.rs:844` and `crates/custodian/tests/staged_protection.rs:909`; frozen evidence agrees at `gate-logs/C4-verify.log:173`. |
| C3 Change | PASS | The patch confines staged reads to a separate protection class consumed before GC/restore decisions, leaves the shared committed-reference builder for scrub/drain unchanged, and updates the operator/docs/DST surfaces at `crates/custodian/src/gc.rs:647`, `crates/custodian/src/restore.rs:340`, and `docs/design/architecture/06-runtime-view.md:78`. |
| C4 Verification (red→green) | PASS | Independent replay was 21/23 red to 23/23 green; the CLI regression and both 50-seed staged-handoff DST tests pass, and frozen full CI is green; my full-CI replay stopped only on the sandbox's read-only Cargo advisory-lock path after docs, fmt, clippy, build, and workspace tests passed (`gate-logs/C4-ci.log:3563`). |
| C5 Causal adequacy | PASS | The change removes the protection gap at its cause by reading each source before its destination and combining disjoint protection classes, rather than adding a capability probe or fallback; scripted and simulated handoffs exercise the losing counter-order at `crates/custodian/src/gc.rs:783` and `crates/dst/tests/custodian.rs:2995`. |
| T1 Structure | PASS | The staged reader is one internal custodian abstraction reused by only the two destructive passes, with existing public entry-point signatures preserved at `crates/custodian/src/gc.rs:809` and `crates/custodian/src/restore.rs:312`. |
| T2 Shape | PASS | Exact placements, whole-chunk holds, and fleet-wide incompleteness have distinct representations, preserving the required malformed-versus-unreadable semantics at `crates/custodian/src/gc.rs:671`. |
| T3 Runtime | PASS | Staged namespaces are walked in checked pages of 512 through per-session ranges, store faults fail closed, and the concurrent handoffs pass a seeded simulator rather than a curated non-concurrent fixture (`crates/custodian/src/gc.rs:794`, `crates/custodian/src/gc.rs:863`, `crates/dst/tests/custodian.rs:2995`). |
| T4 Contribution | N/A | Contribution text is intentionally absent during Check and its substantive audit is mandatory at publish; the frozen batch review and TiKV compile passed (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The tests distinguish exact-placement protection from whole-chunk and fleet-wide containment, force both handoff counter-schedules, preserve unaffected consumers, and the recorded prior-art audit covers affected paths across merged, open, closed, and rejected work (`crates/custodian/tests/staged_protection.rs:1651`, `crates/custodian/tests/staged_protection.rs:2004`, `brief.md:210`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the production safety/availability tradeoff: an unreadable staged record withholds fleet-wide reclaim and mark certification until repair, preventing data loss at the cost of suspending destructive maintenance (`crates/custodian/src/gc.rs:695`, `crates/custodian/src/restore.rs:242`). |

### Advisory — adversary

# Adversarial review — issue #803 (staged protection class for GC and restore)

Red→green reproduced in a scratch clone of `$PDCA_TARGET` (cargo 1.96.0 present, so no
toolchain caveat): with `gc.rs` / `restore.rs` / `reconciliation.rs` reverted to the base
commit, `crates/custodian/tests/staged_protection.rs` gives **21 failed / 2 passed** — every
failure an assertion, none a compile error, and the 2 passes are exactly the two guard legs (D
and F). With the patch applied, 23/23 green. The evidence exercises the production
`gc::reconcile` / `restore::reconcile_after_restore` over in-memory doubles, not a parallel
re-implementation, and the DST double (`crates/dst/tests/custodian.rs:2660`) is a recording tap
over the real `SimTikvMetadataStore`, not a hand-rolled store.

I then hand-injected 14 mutants into the patched production code. Eleven were caught —
part-before-sidx read order, staged-read-after-committed in **both** passes, dropping a page of
a per-session range, dropping a page of the `mpu:` listing, skipping `parse_part_key`,
collapsing the owned-decode failure into a fleet-wide block, either of `StagedSet::protection`'s
`held` / `unresolvable` arms, zeroing the fragment index, identity-filling the D server, and
short-circuiting `place()` after one fragment. Three survived; those are the findings below.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:756`: the exact-length rule is **not covered
  for an empty staged placement**, the one length the committed-side classifier
  (`ChunkRef::placement_is_valid`, `crates/core/src/metadata.rs:204`) calls *valid*. Relaxing
  the check to `chunk.placement.is_empty() || chunk.placement.len() == usize::from(expected)` —
  i.e. drifting toward the shared committed gate, which `crates/core/src/metadata.rs:215` tells
  every maintenance loop to use — leaves **all 23 tests green**. The rule the patch's own doc at
  `gc.rs:660-663` makes load-bearing therefore has no red test. Concrete failing case: seed the
  E(ii) harness with `part(&[chunk_ref(held, RS_2_1, &[])])`; under that one-line change the
  post-restore pass marks `FragmentId { chunk, index: 2 }` on server 3 and the next GC pass
  deletes it (verified — the probe fails on the relaxed build, passes on the patch). Fix: add an
  empty-placement leg beside `a_part_with_a_wrong_length_placement_holds_its_chunk`
  (`crates/custodian/tests/staged_protection.rs:1732`) and its `sidx:` twin.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:719` +
  `crates/custodian/tests/staged_protection.rs:740`: the `ErasureCoded` fixture installs a stray
  copy only for the **committed part's** chunk (`let stray = (3, frag(part_chunk, 0));`), never
  for the **owned (`sidx:`)** chunk — although the fixture's own doc at `:693-696` names that
  guard ("a protection that held the chunk whole instead of placing it would keep the stray").
  So nothing distinguishes "the owned entry's placement was expanded" from "the chunk was
  quarantined whole" on the `sidx:` side. Concrete failing case: replace
  `scheme: staged.scheme()` with `EcScheme::None` at `gc.rs:719` — all 23 tests stay green,
  while every healthy multi-fragment owned entry silently becomes a `held` chunk: GC and restore
  emit `action=untrusted-staged-record` for a healthy record on every pass, `gc_untrusted_staged_records`
  ticks against sound data, and every stray copy carrying that chunk id is protected for as long
  as the record lives (a leak the patch elsewhere takes care to close). Adding the twin stray —
  `(3, frag(owned_chunk, 0))`, server 3 being the one `OWNED_PLACEMENT` does not name — catches
  it (verified: green on the patch, red on the mutant). Note this is the same defect family as
  iteration 7's item 2; the fix landed for `part:` only.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:820` vs `crates/server/src/cli.rs:1339-1341`
  and `docs/design/architecture/m4-first-deployment-blueprint.md:611-612`: the reader never
  decodes a session **value** (`for (key, _session) in &sessions`), a deliberate choice
  documented at `gc.rs:803-805` — but the operator-facing text promises the opposite. Both the
  CLI paragraph and the runbook say an unreadable staged record is "an upload session, a
  committed part or an in-flight staging entry **whose key or value** will not parse or decode";
  a torn `mpu:` value is in fact never named, never counted in `RestoreReport::unresolvable`,
  and never alarmed, so an operator who repairs by that description will not find it. The rule
  is also untested in either direction: no leg seeds a session record whose value the decoder
  rejects, so a plausible "harden the reader" edit — decode the session value and `continue` on
  failure — leaves **all 23 tests green** while silently stripping protection from every one of
  that session's staged fragments, which is precisely the data loss this slice exists to prevent
  (verified). Note also that 0016:406 states a non-decoding `mpu:` value makes the reader fail
  closed and alarm; the patch's choice is safer for protection but diverges from that sentence
  without saying so. Fix: narrow the two operator strings to the key, and add a leg pinning that
  a session with an undecodable value still has its ranges walked and its fragments protected.

Attempted and could **not** refute: the source-before-destination ordering at both handoffs
(`gc.rs:258` before `:273`; `restore.rs:340` before `:350`/`:361`) — mutants moving the staged
read after either committed reading are caught by the leg-C tests; the per-session and
session-listing paging loops (`gc.rs:837`, `:856`) — both single-page mutants are caught; the
`part:` key/value split validation (`gc.rs:740`); the `held` vs `unresolvable` split; the
`Reconciled::Blocked` widening at `gc.rs:411`; the read-cost boundary for scrub and drain status
(`reconciliation.rs:143` dispatches `scrub::reconcile` on its own arm and neither `scrub.rs` nor
`desired_state.rs` is touched, so leg F is not a mocked-away guard); `StagedReadFault::source()`
does preserve the chain `wyrd_traits::classify` walks (`crates/traits/src/lib.rs:801`); no other
key lives under `MPU_PREFIX` (`crates/core/src/multipart.rs:1122-1132`, and 0016:350 lists only
`mpu:<upload-id>`); the session record outlives its `part:` records as a tombstone
(0016:962-967), so the "records whose session is no longer listed are not read" caveat at
`gc.rs:797-799` is not a reachable hole; and no stale `committed object(s) …` string survives
outside the new test's negative assertions. The three `STAGED_PAGE` derivation numbers at
`gc.rs:144-152` check out against `MAX_PART_CHUNKS` (158) and `U_REF` (85,952), and the
`const _: () = assert!(…)` at `gc.rs:155` holds them.

Deliberately **not** raised, as already settled: the fleet-wide GC/restore stall on one
unreadable upload record (accepted at the 2026-09-16 re-plan and again at iteration 6); the
`mpuctl` budget-profile preflight (`// deferred: #806`, `gc.rs:810`); restore's mid-pass window
(`// deferred: #805`, `restore.rs:335`); held-record `needs_human` (`// deferred: #664`,
`restore.rs:818`); the tracker-vs-brief title mismatch; and the patch size against the 100 KB
backstop. No architectural or fitness-to-purpose finding of my own — all three items above are
build defects a Do round can close.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Approve the production safety/availability tradeoff: an unreadable staged record withholds fleet-wide reclaim and mark certification until repair, preventing data loss at the cost of suspending destructive maintenance (`crates/custodian/src/gc.rs:695`, `crates/custodian/src/restore.rs:242`).
- [ ] `crates/custodian/src/gc.rs:756`: the exact-length rule is **not covered
- [ ] `crates/custodian/src/gc.rs:719` +
- [ ] `crates/custodian/src/gc.rs:820` vs `crates/server/src/cli.rs:1339-1341`
- [ ] The re-plan is not supported by the supplied tracker record. `notes.json` still titles #803 “staged protection class in the shared reference set” and its Scope says “the staged protection class in the shared reference set”; it has no comments recording the claimed 2026-09-16 re-plan or sign-off. The brief instead says “Do not re-attempt the shared-builder placement” (`brief.md:189-206`) and adds separate GC/restore reads plus CLI/runbook work (`brief.md:130-159`). Revise the brief to match the tracker-authorized change, or explicitly surface this authorization/scope conflict for adjudication rather than presenting the replacement design as recorded fact.
- [ ] Success leg C(ii) tests the wrong publication handoff. It says one atomic batch writes the committed inode and removes the `part:` record (`brief.md:36-43`), but the target contract says the root flip moves protection to the inode and installs `retire:records:{parts}`, “whose drain then deletes those part records” (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:793-800`); the flip atomically commits the inode and `Completed` session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:941-965`), while the record-mode obligation is explicitly the later deleter (`crates/core/src/multipart.rs:3144-3150`, `crates/core/src/multipart.rs:3424-3432`). A double that collapses flip and drain into a nonexistent transaction does not exercise the real three-event window; revise C and G to schedule publication and retirement-drain deletion separately.
- [ ] The requested operator-verdict change has no red success check. Scope requires the NEEDS-HUMAN paragraph to name staged records beside committed objects (`brief.md:142-150`), but current code hard-codes “committed object(s),” “chunk maps,” and `action=unresolvable-chunk-map` for every `RestoreReport::unresolvable` entry (`crates/server/src/cli.rs:1318-1341`). The existing verdict test seeds only `inode:` names (`crates/server/src/cli.rs:2893-2899`), and legs A–G never assert CLI output, so `cargo xtask ci` can stay green if this semantic change is omitted or staged keys are mislabeled. Add a red assertion using `mpu:`/`part:`/`sidx:` report names and pin the required text and exit verdict.
- [ ] The fail-closed criterion leaves a key-validation hole. Scope says a staged record that cannot be read makes GC/restore incomplete (`brief.md:130-143`), but E(i) covers a bad `part:` value and malformed `sidx:`/`mpu:` keys, not a malformed key inside a session's `part:<id>:` range (`brief.md:48-59`). On the target, `part:` key validation is a separate `parse_part_key` operation (`crates/core/src/multipart.rs:1267-1280`) from value validation by `decode_part_record` (`crates/core/src/multipart.rs:2575-2584`); the proposed tests can therefore pass even if the implementation consumes or silently skips a noncanonical `part:` key without the promised block/audit behavior. Define and test that case.
- [ ] size backstop — this slice is behaving oversized: patch is 162 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Human accepts the size/round overage (166 KB vs 100 KB threshold, 3 rounds) and chose iterate-do over iterate-plan despite the bundle's own recommendation. Scope the next round to the three verified implementation gaps from the adversary review: 1. gc.rs:756 — empty staged placement not covered by the exact-length rule (relaxing the check to match the committed-side rule leaves all tests green); add an empty-placement leg beside a_part_with_a_wrong_length_placement_holds_its_chunk and its sidx: twin. 2. gc.rs:719 + staged_protection.rs:740 — the stray-copy leak-detection fixture only injects a stray copy for the committed part's chunk, never the owned (sidx:) chunk; add the owned-side twin stray. 3. gc.rs:820 vs cli.rs:1339-1341 / runbook — the code never decodes a session's value (key-only read, deliberate), but operator text promises "key or value" is checked; narrow the CLI/runbook strings to "key" and add a leg pinning that a session with an undecodable value still has its ranges walked and fragments protected. Remaining §6 items (fitness-to-purpose tradeoff sign-off, tracker/brief authorization mismatch, leg C(ii) testing the wrong publication handoff, missing red check for the operator-verdict text, and the key-validation hole in the fail-closed rule) are deliberately left open — human will address these after this iteration, not folded into this round's scope.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
