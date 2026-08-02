# Build notes — issue 635 / segmented-chunk-map (iteration 10)

Withheld from the reviewer; written for the human at sign-off.

## What this iteration is

Iteration 9's bundle was rejected on **T4** (gating): five blocking findings, none triaged.
The sign-off rationale was specific about *how* to fix the first two — "fix at the foundation,
not by whack-a-mole per call site … a third call site with the same pattern must not become a
future review round" — and directed the other two to be fixed on their merits. This iteration
starts from iteration 9's patch (applied to a clean `origin/main` @ `9120f7a` worktree) and
changes **five things** plus their tests — then, after running the T4 reviewer myself before
handing off, **eleven more** (see the two pre-check sections below). Nothing else in the slice
moved; the rest of the diff is round 1–9 work the carry-forward says not to regress.

## The five changes the carry-forward asked for

### 1. `gc.rs:309` — structural faults must be typed, and the fix is at the decode boundary

The finding: a `SegmentRecord` (or segmented-root) structural failure is raised inside a
`Deserialize` impl, so `serde` erases it into `D::Error::custom` and it reaches a consumer as
`serde_json::Error`. The containment rule is written as `err.downcast_ref::<ChunkMapError>()`,
so it missed them: one malformed segment aborted the fleet-wide reference build and blanked the
drain surface for every healthy object.

**Where I put the fix.** `metadata::decode` (`crates/core/src/metadata.rs:1720`) — the one call
every consumer already makes. On a decode failure it re-derives the fault by handing the same
wire shapes to `serde_json` and the *same validators* the `Deserialize` impls use
(`SegmentedMap::from_wire` `:1125`, `SegmentRecord::from_wire` `:1342`), so what it reports and
what the store admits cannot drift; when the bytes claim a shape but the re-parse cannot say why
they failed — a field of the wrong JSON type, an unknown field, a *duplicate* field (which a
`serde_json::Value` collapses) — it returns the class-level `SegmentedRootMalformed` /
`SegmentRecordMalformed` (`:470`, `:477`).

The boundary can only classify what it can **prove from the bytes**, and the second pre-check
round showed where that stops: a `seg:` value that is truncated or missing a field does not parse
at all, so nothing can be proved and it stayed a serde error. The completion is
`decode_segment_record` (`:1741`), used at the **two** production reads of a `seg:` value: there
the *key* already names the class and the object, so no proof is needed. That is not the
per-call-site whack-a-mole the directive warned about — it is two sites that share one helper,
where the alternative is per-consumer downcasts at nine.

Alternatives I rejected, with their cost:

* **Typed decode helpers at every consumer** instead of at the boundary — `read_segments`,
  `root_still_names`, `repoint_chunk`, `segment_chunk_floor`, and the root decodes in `gc.rs`,
  `restore.rs`, `rebalance.rs`, `reconstruction.rs`, `backfill.rs`: **9 call sites**, each a place
  a future consumer forgets, and unenforceable — nothing stops the tenth writing
  `metadata::decode`.
* **Recovering the type from serde's message** (string-matching `err.to_string()`). Rejected: it
  makes the *Display text* of every variant load-bearing, so a message reword becomes a silent
  containment failure.
* **Moving validation out of `Deserialize`** so the errors stay typed. Rejected: it breaks
  parse-don't-validate (`AGENTS.md:146-149`) — `decode::<SegmentRecord>` would then *accept* a
  structurally invalid record, which is precisely what leg B(ii) exists to forbid.

The classification is deliberately narrow: only an `inode:` value whose `chunk_map` is a JSON
**object** (the segmented shape — no legacy record has one) and only a value carrying all three
segment-record fields. A corrupt *flat* record keeps serde's error and today's fail-loud
behaviour, which the containment table explicitly allows ("aborting the pass is acceptable — it
is what `decode(&value)?` already does today"). That negative control is asserted, because
classifying too widely would make a pass **continue** over a record it cannot read.

**The second half of the finding.** The downcast alone would still have missed a corrupt
segmented *root*, because that fails at `referenced_fragments`' own `metadata::decode(&value)?`
before any resolver runs. That arm is now contained too (`crates/custodian/src/gc.rs:307`,
sharing `contained` at `:379` with the resolve arm at `:323`).

### 2. `metadata.rs:2170` — the group range is read in bounded pages

`read_segments` materialised `scan(seg:<nonce>:<epoch>:)` whole. `scan` is
complete-or-fail-loud at `SCAN_CAP`, so a damaged generation's range either buffered in full or
raised `ScanCapExceeded` — a **store** error, which per-object containment cannot catch by
construction. Now `read_group_range` (`:2451`) pages at `table + 1` rows and stops at the first
row the caller's table does not account for (`GroupRange::Beyond`, `:2423`). A healthy
generation is still one round trip.

Two knock-ons I chose deliberately:

* the extra row is settled by the **resolve-retry rule** (`retired_or`, `:2603`) instead of being
  raised blind — a root that has since moved on makes it a retirement, not a fault. Previously
  the `SegmentUnknown` check ran before the root re-read;
* `verify_durable_range` (`:3613`) had a **second copy** of the same unbounded `scan` over the
  same range. It now goes through the same reader. That is the "third call site" the directive
  warned about, removed before it became a round-11 finding.

### 3. `metadata.rs:3924` — the id-floor scanner's byte window is gone

The fallback reader lexed the value inside a 236-byte window, so `"id":"<escaped zeros>…"`
closed nowhere it could see, fell through to bare digits, met a backslash and reported nothing.
`scanned_chunk_id` (`:4348`) now reads an optional quote, an **unbounded** run of insignificant
zeros in any spelling, then digits (literal or escaped), bounded by `u64` itself.

The linearity argument that lets the window go, because it is the reason the window existed: a
zero run contains no `"`, and a candidate site must start at a `"`, so no other site can begin
inside the bytes this reader consumed and the outer walk can never re-read them. Work stays
O(value length).

The digit grammar is one function (`decimal_digit`, `:4437`) whose claim — literal, or
`0`…`9`, and nothing else — is checked against `serde_json`'s own unescaping over all
512 `\uXXXX` spellings and the eight two-character escapes, in **both** directions.

### 4. `metadata.rs:1329` — an empty segment record is not a value

Rejected at decode and in the constructor through one body (`SegmentRecord::checked`, `:1319`;
`EmptySegmentRecord`, `:388`). Safe against stranding an already-published object: `push_segment`
closes a segment only when it has chunks, so this committer can never have written one. I also
reject the one-field-over case (chunks whose lengths sum to zero) because the root's own table
already refuses the corresponding `SegmentRef` (`EmptySegment`), and a record whose root
reference cannot exist is not a record worth admitting.

### 5. `metadata.rs:2669` — group adoption asks for one row

`segment_group_adopted` scanned every epoch of a group to answer an existence question. After
enough abandoned attempts that range passes `SCAN_CAP`, and the error lands on the terminal
delete the predicate gates — so the marker could never be reclaimed again, and the more rubbish
a group had, the less able the session was to clear it. Now `scan_page(…, 1)` (`:3032`).

## I ran the T4 reviewer myself, before handing off — and it found six more

This gate has failed nine rounds running, so rather than ship and learn at Check I ran
`scripts/review-branch --bundle` against the finished patch (with `--out` to a scratch path, so
the bundle's `review-batch.md` stays the driver's). It returned **six** findings. All six are
fixed in this bundle; `review-rejected.md` § "Round 10 pre-check" has the finding → fix → test
table. In short:

1. **The seg-record classification had a hole I had argued myself out of.** My `decode` boundary
   classifies what it can *prove from the bytes*, so a `seg:` value that is truncated, missing a
   field, or not JSON at all stayed a `serde_json::Error` — uncontained, which is the very
   finding I was fixing. The completion is `decode_segment_record` (`metadata.rs:1740`): the read
   that knows the **key** does not need to prove the class, because a `seg:` record belongs to
   exactly one group and a group to exactly one object. Both production reads of a `seg:` value
   go through it.
2. **A torn escaped digit read as the digits before it.** `{"id":"12\u003` was read as 12 rather
   than as a truncated token, which is an *under-report* of the allocator floor — the one
   direction that reader may not err in. `torn_digit_escape` (`:4326`) now treats a half-written
   escape as a cut-short digit.
3. **`resume_from == 0` compared nothing.** A second completer at the same `(nonce, epoch)` with
   a different plan would have overwritten a **live** generation's segment records one batch at a
   time and only failed at the root CAS. `verify_durable_range` now walks the whole plan: absent
   above the cursor is fine under `ResumePrefix`, present-and-not-mine is fatal everywhere.
4. **Phase 1 was not held to the flip's fence rule.** A segment batch that pins
   `Completing@E|written=N` without moving it is satisfiable again immediately, so two completers
   could each verify the range and write over each other. `check_fence_transitioned` now runs per
   segment batch; `FenceNotTransitioned` gained `phase`/`batch` so the refusal says where.

The one fixture that had to change is `crates/core/src/read.rs:967`, whose publication hook
pinned a fence it never advanced — i.e. the fixture modelled a session record without its
`segments_written` cursor. It now models the real one.

**A second pre-check pass over the fixed patch returned eight more**, of which **seven are
fixed and one is declined with a recorded reason** (`review-rejected.md` § "Round 10
pre-check, second pass"). The one that matters most for the human to see is a **TEST-GAP in my
own fixture, and the reviewer was right**: the "escaped padding" leg of the id-recovery test
had lost its backslashes somewhere between my intent and the file, so both legs tested literal
`0`s and the escaped path — the whole point of the case — was never exercised. It now asserts
the *spelling* (`serde_json` must read the padding string as `"0"`) before using it, so a
fixture that loses its escapes again fails instead of passing vacuously. That is the exact
shape of hollow evidence this beat is supposed to catch, and it was in my own work.

The declined one is the fence **ABA** transition (a flip that moves the fence to a value some
*rollback* still pins). The committer is parameterised over the fence record — `mpu:<id>` and
its values are #636's — so recognising which value is terminal needs a grammar this slice
deliberately does not have. What is format-agnostic *is* enforced and tested (every batch of
both phases must pin the declared fence and must move it; no put may restore a value the same
batch pins). The full reasoning is the recorded row.

**Where I stopped, and why.** The reviewer is non-deterministic and its own docstring says to
expect roughly one new pre-existing finding per round; the target's rubric says the definition
of done is "deterministic gates green plus **one** deep, multi-pass review whose findings are
each fixed or rejected with a recorded reason — do not iterate review rounds chasing silence".
I ran it twice because this gate has blocked the bundle for nine rounds and a pre-check round
is far cheaper than a PDCA iteration; every finding from both rounds is triaged.

A third run was attempted and **could not run at all** — see the next section.

## NEEDS-HUMAN: the T4 reviewer's quota ran out mid-cycle

```
NEEDS-HUMAN external dependency: codex reviewer quota (the T4 gate's model credits) — the third
`scripts/review-branch --bundle` run returned "You've hit your usage limit … try again at Aug 1st"
and the script correctly refused to certify: "only 0/3 passes produced a usable result after one
retry — refusing to certify a thinner union". So the T4 gate cannot be evidenced right now, and a
T4 failure at Check reading `0/3 passes` is **quota exhaustion, not a review verdict** — it is not
evidence of findings, and it is not evidence of their absence either.
```

What that does and does not leave verified:

* **Verified:** two full 3-pass reviews over this bundle's patch, whose fourteen findings are each
  fixed (13) or recorded-rejected with a reason (1) in `review-rejected.md`.
* **Not verified:** whether a *fresh* review of the final patch is clean. The last two rounds of
  fixes (the second pre-check's seven) have not themselves been re-reviewed.

The `[[doctor.checks]]` row that would have caught this before the cycle burned — the reviewer is
a gating dependency of Check and nothing in `pdca doctor` tests that it can actually run:

```toml
[[doctor.checks]]
id    = "codex reviewer quota"   # the token Plan should have put in `External dependencies`
cmd   = "codex exec --sandbox read-only --skip-git-repo-check 'reply with the single word OK' | grep -q OK"
hint  = "The T4 batch-review gate spends model credits per pass (3 passes/run). Top up at https://chatgpt.com/codex/settings/usage, or export PDCA_LEAVES_MODE=stub / disable the T4 row for a run that cannot afford it."
level = "MISSING"        # T4 is a GATING row at Check, so a cycle cannot reach sign-off without it
```

(`--version` is not the test: it passes with an exhausted account. The check has to spend one
trivial request, which is what distinguishes "installed" from "usable".)

## Two things beyond the review-batch rows

* **The `seggrp:` marker was never written by any code path** (adversary review, open). Now
  `reserve_segment_group` (`:3025`) returns the `require_absent(seggrp:<nonce>)` + marker
  mutation for the minting session's Create batch. A batch and not a commit, because a nonce
  minted in one transaction and reserved in a second is unreserved in between — the window a
  second session mints the same nonce in. I did **not** take the other option (making every
  publication batch `require` the marker): the fence already covers the same hazard (terminal
  cleanup only runs on a terminal session, which the fence refuses), so it would add a
  precondition to every batch of both phases and to the envelope accounting, for no new
  protection, and it would rewrite the contract nine review rounds have been over.
* **The ordering fixture had stopped including its own fault.** Once the resolve reads through
  `scan_page`, the testkit paging helper *sorts* — so the `Shuffling` double delivered rows in
  order and leg B(vi)'s "must not rely on scan order" assertion would have passed on a resolver
  that trusted arrival order. The double now reverses the page it returns (keeping the helper's
  cursor, so the walk still terminates), records that it did, and the test asserts that flag
  before it asserts the order (`:7432`).

## Refuting my own tests (forced; the human reads these at sign-off)

**(a) Genuine red?** Yes, at three levels.

1. *The bundle's binding test, through the project's own runner* — re-run against the FINAL
   patch, not an earlier one.
   `PDCA_BUNDLE=results/issue_635 ./engine/scripts/run-verify.sh` ⇒ **PASS — red without the
   fix, green with it**. GREEN leg: 9 tests ran, 9 passed. RED leg (production reverted, the
   added test kept): **9 tests ran, 9 failed, and the red is assertions, not a build error** —
   e.g. `a_damaged_segmented_object_never_costs_the_store_its_other_objects` panicked at
   `crates/custodian/tests/segmented_map_consumers.rs:1196` with *"one damaged object must not
   fail the id floor the gateway starts from: Error(\"invalid type: map, expected a sequence\")"*,
   and `maintenance_resolves_a_segmented_map_and_never_reclaims_its_fragments` at `:644` with
   *"reconcile_step must resolve a segmented chunk map, not fail on it"*. (The brief's
   Falsifiability clause 3 asks for exactly this count and classification.)
2. *This iteration's own fix, reverted individually* (probes run when each fix landed; the
   production paths they exercise are unchanged by the later pre-check fixes, and leg A was
   re-run green after every one of those). I reverted the `decode` classification
   (leaving everything else) and re-ran leg A: `a_damaged_segmented_object…` **failed** with
   *"one damaged object must not blank the drain surface for the fleet: Error(\"segment declares
   byte_len 17 but its chunks total 16 bytes\")"* — the finding's exact shape, in serde's
   spelling. I then restored it and reverted only the `gc.rs` root-decode arm: the same
   assertion **failed** with `SegmentCountMismatch`. Both restored afterwards and re-run green.
3. *The co-located units.* Each of the nine fixes has a test that fails on the pre-fix body:
   `every_structural_fault_reaches_a_consumer_as_a_chunk_map_error` (`metadata.rs:4803`),
   `a_group_range_no_scan_could_read_stays_one_objects_problem` (`:7535` — pre-fix the fixture's
   `scan` breaches the cap, so the resolve returns a *store* error, not `SegmentUnknown`),
   `each_id_reader_recovers_what_the_other_cannot`'s padded and torn-escape cases (`:9156` —
   pre-fix they read `0` and `12`), `an_empty_segment_record_is_not_a_value`,
   `segment_group_adoption_is_one_bounded_range_read` (pre-fix the `scan` fails),
   `a_fresh_attempt_refuses_to_overwrite_another_plans_durable_records` (`:6037` — pre-fix the
   fresh cursor compares nothing and the publication proceeds to overwrite), and the two new
   phase-1 fence legs of `a_deterministically_refused_publication_writes_no_segment_at_all`
   (`:5744` — pre-fix a pinning hook publishes happily). Line numbers are on the final tree;
   the earlier ones in this file were taken before the pre-check round moved them.

**(b) Production path?** Yes. Leg A drives the real `reconcile_step`, `reconcile_after_restore`,
`reconciliation_status`, `read_object`, `backfill::reconcile` and `metadata::high_water_marks`
over in-memory *seams* (a `MetadataStore` / `ChunkStore` double), not a copy of those loops. The
new leg-B units drive the real `RedbMetadataStore` — including its real `scan_page` and its real
scan cap (`with_scan_cap(1)`), so the paging behaviour under test is the backend's, not a
double's. The one stand-in remains the *caller* that supplies the publication precondition,
which is #636's, exactly as the brief's "Production reach" states.

**(c) Fixture includes the fault?** Yes, and I tightened two fixtures where it had stopped being
true:

* the containment leg now seeds **three** damaged objects — record **absent**, segment record
  present-and-unreadable, root present-and-unreadable — beside the healthy flat and healthy
  segmented ones, and asserts the drain surface answers *and attributes each of them*. Round 9's
  fixture had only the absent spelling, which is why the reported bug survived it;
* the range-bound test uses a real store whose `scan` of the group range **is asserted to fail**
  before the resolve is exercised, so "the resolve worked" cannot be an artefact of a scan that
  would have succeeded;
* the ordering test asserts the double **actually shuffled** a multi-row page (see above).

## Gates I ran here

* `./engine/xtask.sh ci` (the project runner ⇒ `cargo xtask ci`: fmt, clippy `-D warnings`,
  build, test incl. DST, cargo-deny, conformance, `typos`, docs renderer) — **all checks
  passed**. One `typos` hit on my own prose ("mis-tiled") was fixed and the gate re-run.
* `./engine/scripts/run-verify.sh` (C4-verify) — **PASS**, base resolved to `origin/main`, one
  `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`. No `$PDCA_BASE` /
  `$PDCA_VERIFY_BASE` in the environment and no `stack-base` in the bundle, as the brief
  requires.
* `scripts/review-branch --bundle` (T4) run **pre-handoff** against this patch, writing to a
  scratch path so the bundle's `review-batch.md` stays the driver's. Outcome is recorded in
  §"T4 pre-check" below.

## T4 pre-check

See `review-rejected.md` § "Round 9 — all five fixed at one root", which maps every round-9
finding to its fix and its test, and **re-pins the five standing decisions** (rounds 1/5/6) at
the lines they now occupy, so a re-review that reports them at a new line still meets a recorded
decision. No new declines this round: all five findings are fixed, not argued.

## Still for the human (carried, not silently dropped)

These are the §6 NEEDS-HUMAN items iteration 9 left open; none is resolvable by code:

1. **T3 — landing a `Completing`-less precursor committer** before #636 supplies the real session
   fence (brief `Open questions` 4). Unchanged by this iteration.
2. **Fitness of synthetic fixtures pre-#636** — no production path publishes a segmented map when
   this slice merges, which the brief states is correct.
3. **C5 mutants** (advisory) — I did not re-run `cargo mutants` here (≈14 min); the new tests are
   written to bind the mutable surface of each fix (`decimal_digit` has an exhaustive oracle, the
   range bound is asserted by a measured row count, `SegmentUnknown`'s index is asserted exactly).
4. **T4 contribution-history provisionality** — unchanged.

## Scratch

Everything transient lived under `$PDCA_SCRATCH` (`/var/tmp/pdca/pdca-builder-635-*.log`, plus
two `.keep` copies used for the revert probes, both removed). The only worktrees touched are
`$PDCA_WORKTREE` and the `../wyrd-verify` worktree `run-verify.sh` manages itself.
