# Build notes — issue #635 (segmented-chunk-map), **iteration 12**

Withheld from the reviewer; written for the human at sign-off.

## What this iteration is

Iteration 11 was accepted on substance and **rejected on the T4 gate**: 8 findings in the
bundle's `review-batch.md`, 0 recorded-rejected, plus the human's own manual re-run of
`scripts/review-branch` (12 findings, overlapping). The brief's `## Iteration 11 —
carry-forward` names the required work, and this iteration is exactly that work — the
record shape, the resolver, the committer, leg A/B/C and the docs edits are **unchanged from
iteration 11** except where a carry-forward item required a change. I started from
`iteration-v11/patch.diff` applied to the brief's base (`origin/main` @ `9120f7a`, verified
`git log --oneline -1` in `$PDCA_WORKTREE`), so nothing already-accepted was re-derived.

Environment checks the brief demands (Falsifiability 2): `$PDCA_BASE` **unset**,
`$PDCA_VERIFY_BASE` **unset**, no `stack-base` file in the bundle, worktree at `9120f7a`
which carries #634 (`MetadataStore::scan_page` is a required method). Build base == test base.

## The carry-forward, item by item

### 1. The fence/rollback race — the primary must-fix (reported 5× across two runs)

`check_fence_never_cycles` was applied to the **segment phase** only
(`metadata.rs:3500` on the v11 tree). So `A→B` in the phase followed by `B→A` at the flip
satisfied every rule that existed — the per-batch "must move its fence"
(`check_fence_transitioned`) is satisfied, and the flip is not in the phase the cycle rule
saw. The consequence is the exact hazard the fence exists for: a rollback fenced on `A`
(`require(mpu == Completing@E)` + the `seg:` deletes) has its precondition satisfied again
*after* the publication instant, so it deletes the segments the now-live root names.

**Fixed at the boundary the rule belongs to, not at a call site.** The cycle rule is a rule
about the *publication*: `check_fence_never_cycles` now takes an iterator
(`crates/core/src/metadata.rs:4379`) and the flip is charged against **the phase plus
itself** (`flip_batch_after`, `:3823`; entered from `flip_batch` `:3806` and from `publish`
`:4104`, which assembles the phase once and passes it in). The refusal names the flip's
position in the publication.

Deliberate boundary, documented in the code: when the segment phase **cannot be assembled at
all** (a caller-set budget too small, an unfenced hook, …), `flip_batch` falls back to
checking the flip alone rather than re-reporting the phase's error as if it were the flip's.
Both entry points that write segments assemble the phase first, so the refusal is never lost;
and answering "is this flip publishable?" with "your batch budget is too small for phase 1"
would be a worse error than the one the caller asked for. This also keeps the pre-existing
envelope-boundary test honest (`the_flip_batch_refuses_a_caller_contribution_over_the_envelope`
sets the budget to the *flip's* exact size, which no segment phase fits inside).

What is **not** covered, and is not coverable at this level: a cycle across two *attempts*
(attempt 1's hook states are not in attempt 2's batches). Recorded in the code and left to
#636's session grammar, consistent with the round-10 decline.

### 2. Decode / error-classification precision

* **`metadata.rs:1937`** — the fallback classified *any* undecodable inode value whose
  `chunk_map` claimed the segmented shape as a `ChunkMapError`, including one whose `state`,
  `version` or `etag` was the actual fault. That silently converts today's fail-loud
  behaviour for a value of unknown shape into a per-object skip. Fixed by requiring the map to
  be **why** the record failed: `decodes_without` (`:2062`) re-decodes the same record with the
  empty flat map through the production `Deserialize` path; if that still fails, the caller
  keeps serde's error. The residual (a duplicate `state` key, which a `serde_json::Value`
  collapses) is stated in the doc rather than hidden.
* **`gc.rs:307`** — the containment arm ran **before** the `state != Committed` skip, so a
  malformed *uncommitted* record became a fleet-wide blocker (`ReferenceSet::protects` turns an
  incomplete set into "reclaim nothing"), even though a pending inode's map is outside the
  reference set by definition. Fixed with `metadata::inode_state_hint`
  (`crates/core/src/metadata.rs:1900`), read from the bytes that *are* readable, and
  deliberately conservative in the safe direction: a duplicate `state` field is a serde
  duplicate-field error (there is no `Value` to collapse it — the probe is a streaming
  `from_slice` of a one-field struct), an unknown state string is an error, non-JSON is an
  error, and every one of them answers `None` = "assume it could be committed". Reading a
  committed record as pending would be the #508-attempt-4 data loss, so that direction is
  unreachable by construction rather than by care. The skip is **attributed**
  (`emit_uncommitted_unreadable`, `gc.rs:418`) — not silent.
* **`gc.rs:305` (truncated `inode:` value stays a serde error)** — **recorded-rejected**, with
  the reason in `review-rejected.md`. A truncated value proves nothing about its class: the
  bytes that would say "segmented" are the missing ones, so containing it is the
  over-classification the negative control forbids, it is pre-existing base behaviour that the
  brief's containment table explicitly admits ("aborting the pass is acceptable"), and the
  asymmetry with a `seg:` value is principled (a `seg:` key names exactly one object; an
  `inode:` value of unknown class does not tell you what it was).

### 3. The `InodeRecord` write boundary (`:1655` CONVENTION + `:1663` BUG — one root)

Both findings are the same door: the fields are public (they must be — the `Flat | Segmented`
change had to stay a one-line edit at 103 construction sites), so two illegal records are
representable. Closed at the committers, which are the only path from a record in this module
to the store:

* a **segmented root offered to a create** is refused (`created_inode`, `:2105`,
  `ChunkMapError::SegmentedPublicationBypassed`), and refused *before* the leased create's
  lease read so a permanent refusal is not reported as a retryable `Conflict`;
* the **size-vs-table** invariant now has one body for both directions
  (`InodeRecord::checked_shape`, `:1764`) called by the decode's `TryFrom` and by
  `encode_inode` (`:1847`), which every inode **put** in the module goes through. A
  precondition on a prior keeps plain `encode` — it must reproduce whatever bytes the store
  holds.

**Alternative considered and rejected: sealing the fields.** Making `size`/`chunk_map` private
with a validating constructor is the "unrepresentable" fix, and the cost is concrete, not
adjectival: `rg -n 'InodeRecord \{' crates | wc -l` ⇒ **103** construction sites across **30**
files (65 of them relying on `..Default::default()`), every one of which would change from a
struct literal to a fallible constructor call plus an `unwrap()`/`?` — well over 200 changed
lines, most of them in test files, on top of a patch already at 14.7k lines, and the brief explicitly forbids exactly that
churn ("keep it to one uniform, one-line-per-site form … a slice whose diff makes the resolver
hard to find among the churn is the reviewability failure this whole re-plan exists to
avoid"). The committer-side check gets the same *store* invariant — no metadata path can
persist a record its own decode rejects — for ~25 lines.

The span half is **defence in depth** once creates refuse the segmented shape: no reachable
public path can now offer a segmented record with a bad span. I left it in and tested it
directly (`encode_inode` is module-private, so the co-located test calls it) because the
finding is about what is *representable*, and because sharing one body with the decode means
the decode's own tests kill any mutant in it (verified: neutering `checked_shape` fails 4
tests, including `a_segmented_root_whose_size_disagrees_with_its_table_is_rejected`).

### 4. The recurring id-floor scan-cost finding — re-recorded precisely

Same decision as round 11 (Deferred: follow-up; a base-wide `Gateway::recover` question, the
chunk floor is discarded by `recover` on `main` too, and dropping the `seg:` walk is the
under-approximation leg A(vii)(a) and the containment table forbid). It kept being re-flagged
because the gate binds `(file:line, class)` **and** a rationale substring, and both drifted.
Re-recorded at **four** rows — the two lines the code now occupies (`:5125` the call, `:4986`
the function) × BUG and CONVENTION — with the MATCH shortened to `startup`, which every
wording of that claim contains. The other standing decisions (round 1/5's destination-drain
fence, round 6's floor-may-not-fail, round 10's terminal-fence-value ABA, round 11's
duplicate-`chunk_map`-key and dirent-precondition) are re-pinned at their post-edit lines for
the same reason.

Also fixed in passing (docs hygiene a reviewer would have filed): a doc comment describing
`segment_group_adopted` had been left attached to `reserve_segment_group`; moved to the
function it documents (`:3295`, `:3302`).

### 5. Docs currency (a merge requirement, `AGENTS.md:154-157`)

The v11 doc edits stand; this round extended two sentences so the living docs match the new
behaviour: 06-runtime-view gains the "must not move the fence *back*, and the rule spans the
phase and the flip" clause; 08-crosscutting gains "enforced at decode **and by the same body at
the write boundary**" and the staged-publication-is-the-only-publisher-of-a-segmented-root
sentence. `typos` clean; both prose gates ran inside `cargo xtask ci`.

## 6. Pre-handoff review — I ran the T4 gate myself, and it found five more claims

Rounds 10 and 11 set the precedent, and the last three rounds all died on this gate, so I ran
`scripts/review-branch --bundle` (3 codex passes, `--out` to a scratch path so the bundle's
`review-batch.md` stays the driver's) against the finished patch **before** handing off. It
returned **8 findings in 5 distinct claims**; seven are fixed, one declined, all recorded in
`review-rejected.md` § *Round 11 pre-check*:

* **the resumed-publication half of the fence-cycle rule (4 findings, and they were right).**
  My first fix charged the flip against `planned[resume_from..]` — this attempt's batches. A
  recovery that resumes after a *complete* phase 1 commits **no** batches, so the flip was
  charged against nothing and could restore any state the durable prefix had moved through.
  Fixed by re-deriving the prefix's contributions from the same plan and the same hook
  (`prefix_contributions`, `metadata.rs:3832`) and checking prefix + batches + flip. Only the
  caller's contribution is rebuilt, never the segment puts, so nothing durable is re-committed.
  This is the finding I would most have regretted shipping: it is the same defect class the
  brief's carry-forward is about, one level deeper.
* **a repoint can grow a segment past `MAX_VALUE_BYTES`** (a `DServerId` renders in 1–20
  digits). Re-checked where the record is rewritten (`metadata.rs:3318`), so a move that cannot
  fit is a typed `ValueOverCeiling` naming the `seg:` key instead of a permanent backend
  rejection.
* **`SegmentRecord::checked` admitted `byte_offset + byte_len` overflowing `u64`.** Rejected at
  decode and in the constructor through one body, with a new `SegmentSpanUnrepresentable`
  carrying the two numbers (I did **not** reuse `SegmentSpanOverflow`, which would have needed
  a fabricated segment index — a record does not know its own index; the key does).
* **the DST fixture's second attempt minted group epoch 12 while its fence stayed
  `Completing@11`** — a state no completer can be in. The epoch is now a parameter of both
  fence helpers with a constant per attempt, and the second attempt has its own hook. The
  corrected fixture *fails* without the matching hook, which is the proof it was vacuous
  before.
* **declined:** widening `SEGMENT_TARGET_BYTES`'s 2× headroom to cover worst-case placement
  widths. It is 0016's normative arithmetic, it cannot bound a *later* repoint's growth
  anyway, and the cost is the feature's own requirement — the largest publishable object is
  `MAX_ROOT_SEGMENTS × SEGMENT_TARGET_BYTES` worth of chunk refs, so 4× headroom halves it,
  against a >10 GiB launch target. The hazard is fixed where the record actually grows.

**The second (confirmation) pass then returned six findings in four claims** — none of the
first pass's — and three of them were about the fix I had just made, which is why running it
twice was worth the ~15 minutes:

* **the prefix reconstruction was split-dependent (3 findings).** I had split the prefix at the
  plain budget, ignoring the contribution-reserve fixed point that decided the *real* batch
  boundaries, so it could invent states the earlier attempt never wrote and miss ones it did.
  Rewritten to be **split-independent**: ask the hook for every cursor position the prefix
  passes, one segment at a time. For a cursor-keyed contribution the states of any coarser
  split are a **subset** of these (a batch spanning `[i..j]` pins what the one-segment batch at
  `i` pins and puts what the one at `j-1` puts), so nothing visited can be missed whatever
  budget or reserve produced the real batches. The test gained a case that restores a cursor
  position *inside* the prefix rather than at a boundary — the state the old reconstruction
  would have missed. This also deleted a parameter and a code path, so the fix is smaller than
  what it replaced.
* **`prior.version + 1`** on a version read from the store: panics with overflow checks on,
  wraps to zero without them. `checked_add` + `RootVersionOverflow`, with the boundary tested
  both sides.
* **`flip_batch` was public**, handing out the root mutation without the whole-plan durability
  check that `flip` exists for. Made module-private — it had no user outside `metadata.rs`, so
  the footgun closed at zero cost.
* **declined:** a timeout around `read_group_range`'s `scan_page` await. No store call anywhere
  in the workspace has one; the deadline is the backend's and is stated on the trait's envelope,
  and one call site failing differently from every other store call in the same pass is worse
  than failing the same way. The bound this slice *does* owe is in work, not time, and it is
  enforced (one page past the accounted table, and a refusal over `MAX_ROOT_SEGMENTS` before a
  row is read).

Per the rubric's *Definition of done* ("do not iterate review rounds chasing silence") that is
where the review loop stops: two passes, every finding fixed or recorded. Every standing
decision is additionally re-pinned at its **final** line in `review-rejected.md`, because the
gate binds a decision to `(file:line, class)` and this round moved the code twice.

## Verification

| What | Result |
|---|---|
| `./engine/xtask.sh ci` (the project runner ⇒ `cargo xtask ci`: fmt, clippy `-D warnings`, build, workspace tests, DST, cargo-deny, conformance vectors, `typos`, docs renderer) | **`xtask ci: all checks passed`** (re-run after every fix above) |
| `./engine/xtask.sh dst` (50-seed madsim campaign, incl. `staged_publication_is_atomic_at_the_flip`) | 13 passed |
| `PDCA_BUNDLE=results/issue_635 ./engine/scripts/run-verify.sh` (C4-verify) | **PASS — red without the fix, green with it**, base `origin/main`, one added test target |
| Leg A red leg composition | **9 tests ran, 9 failed, every one an assertion failure** (`invalid type: map, expected a sequence` surfacing through `reconcile_step` / `high_water_marks` / the read path) — **not** a build error, which is what the brief requires |
| Leg A green | 9 passed (`cargo test -p wyrd-custodian --test segmented_map_consumers`) |
| `cargo fmt --all` | applied; `--check` clean (the target's commit hook would have rejected the first draft) |

## The three refutation questions

**(a) Genuine red?** Yes, for every change this iteration makes — each was probed by
reverting the fix in place and re-running, and each went red with a message naming the
property:

* whole-publication fence cycle → replace the sequence with `once(&self.flip)` ⇒
  `a_flip_that_walks_the_fence_back_into_the_segment_phase_is_refused` fails with
  `called Result::unwrap_err() on an Ok value: WriteBatch { … "Completing@7|state=b" … puts …
  "Completing@7|state=a" }`;
* classification precision → `if false && !decodes_without(record)` ⇒
  `a SEGMENTED root whose state is not a state: not a chunk-map structural fault … got
  malformed segmented chunk map`;
* uncommitted-record precision → `if false && metadata::inode_state_hint(…)` ⇒
  `left: Satisfied, right: Changed` ("the elapsed orphan is reclaimed");
* create refusal → `if false && record.chunk_map.is_segmented()` ⇒ `unwrap_err()` on an `Ok`;
* `checked_shape` → neutered ⇒ **4** tests fail, including the pre-existing decode-side one;
* resumed-prefix trajectory → drop the prefix from the flip's sequence ⇒ `a flip may not
  restore a state the durable prefix moved through: … "Completing@7|written=6" … puts …
  "Completing@7|written=0"`;
* repoint ceiling → skip `check_record_ceilings` ⇒ `unwrap_err()` on an `Ok`;
* checked version → `wrapping_add(1)` ⇒ `a_publication_over_the_last_version_refuses_rather_than_wrapping`
  fails at the typed-refusal assertion;
* the DST epoch fixture → keep the epoch-11 hook under an epoch-12 group ⇒ `the second batch
  landed and was reported unknown: Conflict` (the fence no longer holds), which is the proof
  the corrected fixture exercises the coupling rather than describing it.

The `SegmentSpanUnrepresentable` and `flip_batch`-privacy changes are the two whose "red" is
structural rather than assertional: the first has its own test (stored bytes, constructor, and
the exact boundary), the second is a visibility change whose regression guard is the compiler
plus `cargo xtask ci` (no workspace user is lost).

And leg A as a whole is red on the unmodified base through C4-verify (above).

**(b) Production path?** Yes. The new tests drive the real committer
(`SegmentedPublication::{flip_batch, publish}`) over a real `RedbMetadataStore::in_memory()`,
the real `metadata::decode` boundary, the real `metadata::create`/`create_leased`, and — for
the containment fix — the real `reconcile_step` → `gc::reconcile` → `referenced_fragments`
path through `crates/custodian/tests/gc.rs`'s in-memory trait stores. No stand-in for the unit
under test; the only doubles are the *fleet* and the caller's fence contribution, which is
what the brief's `Production reach` section says they must be.

**(c) Fixture includes the fault?** Yes, and this is where I spent the most care:

* the fence-cycle test seeds the session at the value the first batch pins, so **without** the
  rule every precondition genuinely holds; and its second half **drives the hazard by hand** —
  commits the same phase batch, commits the same flip, asserts the object resolves, then
  commits the `A`-fenced rollback (which the restored fence re-arms), and asserts the live root
  now resolves `SegmentAbsent` for ever. The refusal is not asserted against a description of
  the hazard, it is asserted against the hazard actually happening;
* the gc test seeds a **real** undecodable segmented root in the `inode:` namespace and asserts
  it genuinely fails `metadata::decode` before relying on it, and its control is the
  **committed** spelling of the same bytes, which must still freeze the pass — so a "fix" that
  bought precision by weakening containment fails the same test;
* the write-boundary test asserts the poison bytes are genuinely undecodable before asserting
  the encoder refuses them, and asserts `encode_inode == encode` for every acceptable record
  (a check that would catch a "fix" that changed the CAS bytes).

## Open items for the human at sign-off (unchanged from v11 unless noted)

1. **T3 / `Open questions` 4** — landing a `Completing`-less precursor committer before #636
   supplies the real session fence. Still the brief's stated choice; still a human call.
2. **T4 contribution history** — I re-ran `scripts/review-branch --bundle` myself before
   handoff (as rounds 10/11 did), output to a scratch path so the bundle's `review-batch.md`
   stays the driver's. See "Pre-handoff review" below for what it said and what I did.
3. **C5 mutants** — advisory; the surviving-mutant set from v11 was concentrated in the
   byte-level id recovery, whose oracle tests are already independent. Not re-run here (each
   run is ~16 min and the diff's shape is unchanged); the human may treat the advisory row as
   carried.
4. The **reconstruction/rebalance containment gap** for a damaged object (an aborting
   byte-moving pass) remains an Act candidate per iteration 10's sign-off, not this bundle's.
