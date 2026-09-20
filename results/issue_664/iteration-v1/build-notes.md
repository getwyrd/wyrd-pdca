# Build notes — issue 664 / staged-drain-and-restore-fence

Withheld from the reviewer. Every `path:line` below is against the cycle worktree
`/home/eddie/wyrd/wyrd.pdca-wt`, whose base is `f41e9c5` = `origin/main` + #661 (PR #802) +
#803 (PR #807) — the bundle base the brief names (wave 3 landed, and #662 was re-planned into
#803, so the worktree already carries the staged protection class).

---

## 1. What I built, and why in that shape

Four things, in the brief's order.

### (1) Drain status counts the staged class as held

`crates/custodian/src/desired_state.rs:232` reads `gc::staged_fragments` **before**
`referenced_fragments` and folds all three of its classes into the answer:

- `staged.placed` joins `referenced.placed` in the `genuinely_holds` test
  (`desired_state.rs:241-248`) → `Pending`;
- `staged.unresolvable` joins `referenced.unresolvable` in `PendingUnresolvable`
  (`desired_state.rs:284-306`);
- `staged.held` joins `referenced.malformed` in `PendingMalformed`
  (`desired_state.rs:317-332`).

**Why no new `ReconciliationStatus` variants.** The two existing "cannot certify" arms already
say exactly the right thing about the staged versions of the same faults — one hides *where* a
chunk's fragments are, the other hides *which chunks a record names* — and the operator's next
move (repair the named record, re-ask) is identical. A `PendingStagedUnresolvable` would be a
second spelling of one stall, and the brief's Falsifiability clause forbids the test naming a new
variant anyway. I widened both variants' docs instead and added a per-class audit emitter
(`emit_unresolvable_staged`, `desired_state.rs:372`) so a `part:`/`sidx:` key is never reported to
an operator as a committed object's chunk map — the same split GC, restore and the CLI verdict
already make.

**Read order.** Staged before committed, matching every other reader of the class
(`0016:793-800`): a publication moves a chunk's protection from its `part:` record to a committed
inode, so this order sees it in at least one of the two.

**Cost this accepts, explicitly.** The drain query now issues the staged read — the `mpu:`
listing plus two bounded ranges per session — on every non-`NotRequested` call, and a store fault
under it now fails the query closed instead of being invisible to it. That is the price of the
invariant; the alternative (answer over committed placements only) is the F6 trace the brief
names.

### (2) Rebalance — confirmed disjoint, **no code change**

`rebalance::plan_evacuations` (`crates/custodian/src/rebalance.rs:264`) scans `b"inode:"` and
nothing else; every write it makes is an `inode:` repoint plus `orphan:` marks
(`rebalance.rs:539-547`). It therefore cannot move a staged fragment or rewrite a `part:` record.
The brief said "a code change only if it currently moves or rewrites staged records" — it does
not, so the slice ships the proof (leg D) and the discharged marker in `gc.rs:672-675`, not code.

**Which `Reconciled` the pass returns in leg D, and why it is not an operator's "drain done".**
`Satisfied`. With the draining server holding only staged fragments, `draining` is non-empty,
`plan_evacuations` returns zero plans, `withheld` is false and no move was attempted, so
`reconcile` falls through to `Reconciled::Satisfied` (`rebalance.rs:199-203`). That is a true
statement *about the pass* — it planned nothing and therefore failed nothing — and
`rebalance.rs:193-197` already says in so many words that the per-server query
(`desired_state::reconciliation_status`) is the authority on whether a specific server may be
pulled. Leg D asserts both halves together precisely so that the pair can never read as "the
drain is done": rebalance `Satisfied` **and** the drain query `Pending`.

### (3) Restore's staged accounting and the session fence

**Accounting.** `staged_skipped` is counted at `restore.rs:540`, *after* the committed gates, so
it answers "how many fragments did only the staged class keep?" rather than counting predicate
hits. The `incomplete` arm stays uncounted: it withholds the whole fleet, and attributing every
fragment in the fleet to the staged class would be a number about nothing.

**The fence** (`restore.rs:466-476`, `:851-1080`). Per listed session, paged through the same
`gc::staged_page` / `gc::walk_staged_range` the staged class uses (made `pub(crate)`, `gc.rs:855`
and `:878`, so there is one paging bound and one range-naming failure, not two):

| state | batch |
|---|---|
| `Open@E` | CAS `mpu:` → `Aborting@E+1`; `require_absent` + put `retire:bytes:s:<id>:<E>` = `{session, parts:"all"}` (`0016:2187` — the wildcard is legal here precisely because *this* fence freezes the part range) |
| `Completing@E` | CAS `mpu:` → `Aborting@E+1`; `retire:bytes:s:<id>:<E>` = `{session, parts:<explicit set>}` (`0016:665`); and, when the attempt has segment records, `retire:records:s:<id>:<E>` = `{seg:{nonce,E}}` |
| `Aborting`, `Completed` | nothing — neither can publish over restored bytes |

One `WriteBatch` per session, so the "cannot publish" half and the "has a deleter" half land
together or not at all (legs F/G atomicity, and the DST race property).

**Token epoch is `E`, not `E+1`.** `RetirePayload::checked_against_key` requires a `{seg}` group's
epoch to equal the token's, and the `seg:` records were written under the `Completing` epoch `E`;
`multipart.rs:3477-3496` already states the rule as "the fence that ends attempt `E` installs
under `s:<id>:<E>`". Both obligations therefore share epoch `E` while the session moves to `E+1`.

**Why the `{seg}` obligation is installed on `segments_present || segments_written > 0`**
(`restore.rs:1075-1077`) rather than on the cursor alone: the cursor is one number in a restored
record and the range is ground truth. Taking either as sufficient means a cursor lying low cannot
leave the records with no deleter, and one lying high cannot skip the obligation.

**X57 / leg H(ii).** `completing_plan` reads both ranges and compares the chunks the `seg:`
records name against the chunks the surviving `part:` records name (`restore.rs:1018-1080`). Any
uncovered chunk, or any unreadable record in either range, makes the teardown unprovable: the
session is **still fenced** (leaving it publishable is worse) and named in
`sessions_fenced_with_residue`. #637 v1 built the teardown from whatever `part:` keys were present
and reported the leftovers afterwards; this reports them as a finding that fails the command.

### (4) The generation record, the nonce, and the CLI report

`mpufence` (`multipart.rs:3687`) holds `{generation: u64, complete: bool}`, 1-based so that
**absence** is the only spelling of "no pass has ever run" (a stored `0` is
`RecordError::RestoreFenceGenerationZero`, `multipart.rs:298`). The pass takes the next
generation and writes it not-complete as its **first** write (`restore.rs:391`, `:710`) and marks
that same generation complete as its **last**, and only when `!report.needs_human()`
(`restore.rs:693`, `:745`). Both commits are compare-and-set on the record's exact bytes, so two
concurrent one-shots cannot both certify; a loss is a typed `FenceGenerationRaced`
(`restore.rs:781`) rather than a quiet skip.

`RestoreReport` gains `staged_skipped`, `sessions_fenced`, `sessions_unfenceable` and
`sessions_fenced_with_residue`, plainly, no `#[non_exhaustive]` as decided at Plan. The last two
join `needs_human()`; `cli.rs:1292-1325` gives each its own NEEDS-HUMAN paragraph and
`cli.rs:2966-2998` extends the existing "each finding alone flips the status *and* prints its
paragraph" test to cover them.

---

## 2. The nonce — the design call, and how I implemented option (i)

Settled at Plan: store the segment-group nonce on the session record. I put it on
`PublishTarget` (`multipart.rs:1985-2015`) rather than on `SessionState::Completing`, because the
brief's own citation for "alongside the fence epoch" is `PublishTarget`, and because
`PublishTarget` already carries the epoch — a `SegmentGroup` field on `Completing` would have
stored a **third** copy of the epoch, which is exactly the "two stored spellings of one quantity"
the module refuses at length (`AdmissionRecord`'s doc, `multipart.rs:1786-1796`).

Implementation detail worth flagging: `SegmentNonce` deliberately has **no** `Deserialize`
(`metadata.rs:757-760`), so `PublishTarget` gets a hand-written `Deserialize` over a closed wire
struct that routes the string through `SegmentNonce::new` (`multipart.rs:2028-2055`) — the same
shape `SegmentGroup`'s own impl has. I did **not** add a `Deserialize` to `SegmentNonce`: that
would create a second home for the nonce rule, which is the thing the type exists to prevent.

I did add one thing to `metadata.rs`: `SegmentGroup::of(SegmentNonce, u64)`
(`metadata.rs:813-824`), an infallible constructor from an **already-validated** nonce, so
`PublishTarget::segment_group()` (`multipart.rs:2022`) does not have to round-trip through a
`String` and a `Result` it could never take. Eight lines, and it adds no new rule.

The doc comment on the new field records the `0016:354` / `:2333` disagreement it settles and
names both rejected alternatives, as the brief's Scope requires.

---

## 3. Writer seams I had to add, and how they are kept honest

`crates/core/src/multipart.rs` withholds writer-side constructors on purpose. The fence needs two
of them, so each is shaped so it cannot mint a value its own decoder would refuse:

- `SessionRecord::fenced_aborting()` (`multipart.rs:2313`) is a **transition** on an
  already-validated record, routed back through `TryFrom<SessionRecordWire>` so "one place a
  `SessionRecord` comes into existence" stays true. The epoch bump is `checked_add`
  (`RecordError::CounterExhausted`), never saturating: a fence's whole identity is that it is the
  only fence of its epoch.
- `encode_retire_obligation()` (`multipart.rs:3664`) encodes the payload and then reads it
  straight back through `decode_retire_obligation` **against the key it would sit under**. Every
  key-relation rule (mode, token scope and suffix, segment epoch, the `all` wildcard's one row)
  is therefore re-asked of the pair before a writer ever sees bytes. Its own unit test
  (`multipart.rs:5424-5486`) drives three refusals: `{session,all}` under `retire:records:`,
  `{seg}` under `retire:bytes:`, and a `{seg}` group whose epoch is not the token's.

`RetirePayload::teardown` / `::rolled_back_segments` (`multipart.rs:3386`, `:3404`) are the two
0016 writer rows the fence installs, and nothing else.

---

## 4. Decisions the brief handed me

**The `// deferred: #664` marker in `restore.rs`'s `attribute_staged`** — "should an untrusted
staged record set `needs_human()`?" **No**, discharged at `restore.rs:1319-1328`. It is the exact
peer of a malformed committed placement (read, not trusted about placement, holds its chunk
unmarked, named on the audit seam), and this pass has never failed a restore script on one of
those. What the two now share is the surface that genuinely matters for them: the drain query
refuses to certify over **both** classes cluster-wide (`PendingMalformed`) and names the chunk
ids. So the record blocks the decommission it actually endangers rather than the restore script
it does not. Recorded in `RestoreReport::needs_human`'s doc so the reasoning is where a reviewer
looks for it.

**`sessions_unfenceable` vs `sessions_fenced_with_residue` as two fields, not one.** They are
different operator instructions: one session is still publishable and must be repaired before the
gateways come back; the other cannot publish but has bytes with no deleter. One field named
"unfenceable" holding both would be dishonest about the second (it *was* fenced).

---

## 5. Alternatives I rejected, with their cost

- **Give the drain query its own staged reader** instead of `gc::staged_fragments`. Rejected: a
  second reader is a second definition of "a staged reference", and `gc.rs:655-668` states the
  rules once. It would also have duplicated the paging (`STAGED_PAGE`) and the
  `StagedReadFault` range naming — roughly the 60 lines of `gc.rs:809-877` copied.
- **Two new `ReconciliationStatus` variants** for the staged unreadable / untrusted classes.
  Rejected on the grounds in §1: same operator instruction, and the test may not name a new
  variant. Cost avoided: two variants × ~20 doc lines + a match arm in every consumer
  (`cli.rs` has none today, but #508's gateway and the CLI drain surface would each gain one).
- **Derive the nonce from `(upload id, E)` as `0016:2333` says.** Rejected at Plan and by
  `0016:499-509`, which the module already cites (`multipart.rs:3477-3487`): a nonce derived from
  the upload id collides across the `mpu:` tombstone's lifetime, which is the reuse the nonce
  exists to make impossible.
- **Put the nonce on `SessionState::Completing` as a whole `SegmentGroup`.** Cheaper in plumbing
  (it would reuse `SegmentGroup`'s existing validating `Deserialize` and drop `PublishTargetWire`,
  its `Deserialize` impl and `SegmentGroup::of` — about 45 lines of the diff). Rejected because
  it stores a third copy of the epoch beside `SessionRecord::epoch` and
  `PublishTarget::epoch`, and the module's own line is that two stored spellings of one quantity
  are a decode error.
- **Fail the whole pass on a session record that will not decode.** Rejected: #651's whole point
  is that one damaged record must not blank the post-restore picture. It is contained, named, and
  makes the run need a human.
- **Skip the generation record's CAS** (plain put at start and end). Rejected: it costs nothing to
  add and it is the only thing stopping two concurrent one-shots from both certifying.
- **Reading `seg:` only when `segments_written > 0`.** Rejected — see §1(3): one bounded read per
  `Completing` session buys immunity to a cursor that lies in either direction.

---

## 6. Tests and evidence

### Red on the base

Ran the **final** `crates/custodian/tests/staged_drain_restore.rs` against a detached scratch
worktree at `f41e9c5` (the bundle base), under `CARGO_TARGET_DIR` in `$PDCA_SCRATCH`; the worktree
and its build cache were removed afterwards.

```
test result: FAILED. 1 passed; 11 failed
```

**11 tests ran red, every one by assertion** (an `assert_eq!`/`assert!` message, or an
`expect_err`/`expect` on the pass's own answer) — never a compile error, so the file names only
base-visible symbols exactly as the Falsifiability clause requires. The one green is leg **C**,
the declared guard (`c_drain_finishes_when_the_uploads_live_elsewhere`), which kills #637 v1's
`*server != dserver` mutant.

Base failure messages, by leg:

| leg | test | base failure |
|---|---|---|
| A | `a_drain_counts_an_in_flight_part_as_held` | `left: Satisfied, right: Pending` |
| B | `b_drain_counts_a_committed_part_as_held` | `left: Satisfied, right: Pending` |
| C | `c_drain_finishes_when_the_uploads_live_elsewhere` | **green** (guard) |
| D | `d_rebalance_leaves_staged_bytes_alone_and_the_drain_stays_pending` | `left: Satisfied, right: Pending` (the drain half; the rebalance half is green on base too) |
| E | `e_restore_reports_staged_skips_separately` | "the report does not say how many fragments it kept for a live upload" |
| F | `f_restore_fences_a_resurrected_open_session` | "the report does not say it fenced the resurrected session" |
| F | `f_the_fence_and_its_obligation_land_in_one_batch` | "the refused commit fails the pass" (no commit touches `mpu:` on base) |
| G | `g_restore_fences_a_completing_session_with_its_segments_deleter` | `sessions_fenced: 1` absent from the rendering |
| G | `g_both_obligations_and_the_fence_land_in_one_batch` | "the refused commit fails the pass" |
| H | `h_a_completing_session_without_a_nonce_is_named_not_rewritten` | "a session that could not be fenced can still publish…" |
| H | `h_a_completing_session_whose_segments_outrun_its_parts_is_fenced_and_named` | `sessions_fenced: 1` absent |
| I | `i_the_restore_fence_generation_advances_and_completes_last` | "the probe fired at the first fence commit" |

### Green on the fix

`cargo test -p wyrd-custodian --test staged_drain_restore` → **12 passed**.

### Leg J (codec, green-only by nature)

`crates/core/src/multipart.rs:5258` — a new `#[cfg(test)] mod restore_fence_records`, 7 tests:
the `Completing`-with-nonce round trip (byte-identical) and its `segment_group()`, the nonce-less
refusal, five non-hex nonce spellings refused, the `fenced_aborting` transition, the generation
record's advance/complete/round-trip, the generation-zero refusal, and the decode-checked
`retire:` writer with three negations.

### Leg K

`./engine/xtask.sh ci` (→ `cargo xtask ci` in `$PDCA_WORKTREE`) — **all checks passed**, including
`cargo fmt --all -- --check`, clippy at `-D warnings`, the statics gate, the docs gate and the
full DST campaign. `typos` clean. Both the brief's external dependencies (`typos`,
`docs-renderer`'s `markdown-it-py` + `PyYAML`) are installed and green — **no NEEDS-HUMAN
external dependency**.

### DST — added beyond the brief, on the rubric's instruction

The repo's standing rubric says "a new destructive or concurrent path lands with seeded Tier-0
DST coverage". The fence **is** a new concurrent write path — a client `AbortMultipartUpload`
(#656) fences from exactly the same prior state under exactly the same token — so I added
`crates/dst/tests/custodian.rs:2248` `restore_fence_under_a_racing_fencer` plus two campaign legs
(`:2405`, `:2416`), registered in the campaign list at `:3396`, `:3402` and `:3462`. On every
schedule it asserts: the session ends fenced whoever won; **exactly one** obligation exists under
the epoch's token and it decodes against its key; the durable generation is complete iff the run
needs no human; and a run that needed one certifies on a re-run over the settled store. The
coverage leg walks the racer's whole landing span and asserts all three schedules are reached —
the pass fencing first (delays ≥ 11 ms on this model), the racer landing strictly between the
pass's session read and its commit (9–10 ms, the compare-and-set loss), and the racer landing
before the pass read at all (0–8 ms). `FENCE_RACE_SPAN = 20` with margin on both sides, and the
coverage leg is what fails loudly if a later change moves the crossing out of the span.

---

## 7. Existing tests I had to change, and why

These are the base's own assertions about behaviour this slice deliberately changes. Both were
marked `// deferred: #664` or are the same class.

- `crates/custodian/tests/staged_protection.rs:2198` — #803's leg F asserted that **scrub and the
  drain-status query** read no upload record. Its own marker said "#663, #664 — those slices add
  upload records to scrub and to drain status, and own changing this leg." Narrowed to scrub
  (`scrub_does_not_read_upload_records`); the drain-status legs are now A–D in the new file. Its
  store-fault cases would otherwise have failed the query closed, which is the correct new
  behaviour.
- `crates/custodian/tests/staged_protection.rs:1671-1780` — #803's leg E asserted the post-restore
  pass never names an undecodable **session value** on its seam. The fence must read that value,
  so the leg now keeps the never-decodes claim for GC (which still does not) and asserts the
  restore side names it as *unfenceable* (a different field from *unresolvable*) and leaves the
  record byte-identical (a new `Meta::kv_get` helper, `:188`).
- `Completing` witnesses everywhere gain the nonce: `staged_protection.rs:527`,
  `crates/core/tests/multipart_session_records.rs:107-112` + `:287`,
  `crates/core/tests/multipart_state_machine.rs:191-199`, `crates/dst/tests/custodian.rs:2851`.
- `crates/server/src/cli.rs:2966-2998` — the hollow-green test gains the two new findings as
  isolated one-at-a-time reports, and the "routine, not a human's" report gains
  `staged_skipped`/`sessions_fenced` so the fix cannot flip the status on them.

---

## 8. Docs currency (AGENTS.md's merge requirement)

New persisted record (`mpufence`) and new persisted field (`mpu:`'s nonce):

- `docs/design/architecture/05-building-block-view.md:202` — the multipart key-prefix inventory
  gains `mpufence` and the nonce, and its "nothing writes or consumes these records in production
  yet" claim is corrected: the post-restore fence is the first writer.
- `docs/design/architecture/06-runtime-view.md` — §6.7 gains the drain query's stake in the staged
  class, rebalance's disjointness, and three new paragraphs on the fence, what it cannot fence
  cleanly, and the generation record.
- `docs/design/architecture/m4-first-deployment-blueprint.md` — the post-restore step's exit
  reasons go from three bills to five (UNFENCEABLE SESSION, FENCE RESIDUE), plus the "run to a
  clean exit before step 8" ordering and a new bullet in the "what a restore costs you" list.

---

## 9. Forced refutation — the three questions

**(a) Genuine red?** **Yes.** The final test file, run unchanged against the bundle base
`f41e9c5` in a scratch worktree, went 11-red / 1-green (leg C is the declared guard). Every red
is an assertion or an `expect`/`expect_err` on the production pass's own answer, not a compile
error. Transcript summarised in §6.

**(b) Production path?** **Yes.** Every leg calls the production entry points —
`wyrd_custodian::reconciliation_status`, `reconcile_step` with a real `RebalanceContext` (so the
production rebalance loop runs), and `reconcile_after_restore` — over `MetadataStore` /
`ChunkStore` trait doubles, which is the seam these passes are defined over. Every seeded record
is validated by the **production** codec at seed time (`decode_session_record`,
`decode_part_record`, `decode_owned_entry`, `SegmentRecord::new`), and every obligation the fence
installs is read back through the production `decode_retire_obligation` against its real key. The
one deliberate exception is the `Completing` fixture, which is *not* round-tripped at seed time
because the base's decoder refuses it — documented in the fixture's own doc comment, and the leg's
red then comes from the pass's answer rather than from a fixture panic.

**(c) Fixture includes the fault?** **Yes**, and each leg carries the control that makes its
claim non-vacuous:

- A/B/D put the staged fragment **on the draining server** — nothing is curated out; C is the
  mirror that proves the rule is not "block on any staged byte anywhere".
- E seeds a genuine unreferenced stray that the same pass **does** mark (`stranded_marked == 1`),
  so "it kept the staged ones" cannot pass for "it did nothing".
- F/G atomicity: the double really refuses the fence commit, and the assertion is on the
  **absence** of the obligation afterwards — if the obligation were a second batch it would be
  there.
- H(i) seeds the actual pre-decision shape (no nonce) and asserts the stored bytes are
  **unchanged**; H(ii) seeds a `seg:` record naming a chunk no `part:` record holds, and asserts
  the session is fenced *and* named.
- I reads the generation record **during** the pass, at the real fence commit, through a store
  probe — not inferred afterwards.
- The DST legs inject a genuinely concurrent fencer at a seed-chosen instant and assert all three
  schedules are reached; without the coverage leg the seeded property could have been green with
  only one regime behind it.

---

## 10. Things the human should weigh at sign-off

1. **Production reach, as the brief declares it.** No client can create a session until #508, so
   every session in every test is seeded, and the generation record has no reader until #508's
   gateway gate. Until then the guarantee rests on the deployment ordering `0016:3017-3021` also
   allows (post-restore pass before gateways), which I wrote into both runbook docs.
2. **The drain query got more expensive and less forgiving.** It now reads every session's two
   bounded ranges per call, and a store fault under that read fails it closed. Deliberate (§1),
   but it is a behaviour change on an operator-polled surface.
3. **`Conflict` on either `mpufence` commit is an `Err`**, which loses the report for that run.
   I judged that right (two concurrent one-shots is an operator error, and "this image is fenced"
   is exactly the claim that must not be made on stale evidence), but it is the one place the
   pass can end without a report after #651 went to some trouble to stop that.
4. **Scope call:** the DST legs are beyond the brief's four items. I added them because the repo's
   standing rubric makes seeded Tier-0 coverage a MUST for a new concurrent write path. If that
   is unwanted, `crates/dst/tests/custodian.rs` is a clean, separable ~290-line hunk.
