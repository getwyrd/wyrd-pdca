# review-rejected.md — issue 695 (iteration 9)

Machine-readable triage decisions for `scripts/review-branch` (T4-batch-review). Each
non-comment line is `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from
the finding's own rationale (case-insensitive substring).

**Line numbers refreshed for iteration 9's `patch.diff`.** A recorded decision binds to an
EXACT `<file:line>`, so the iteration-7 locations this file used to carry had stopped binding
to anything. Every decision below is the same decision, re-keyed to the line it now lives at,
verified against the patched file in the cycle worktree.

## The lost-CAS class — RECORDED-REJECTED per the round-8 sign-off

Round 8's sole blocker: **`backfill.rs:268` [BUG]** — *"A CAS conflict proves the scanned
generation is stale, yet the pass neither re-reads the winner nor marks itself incomplete, so
it can return `Satisfied` with a stale or zero drain gauge while the winning record still has
empty placements."* It is round 6's blocker (2) resurfacing at a new line, and the human's
round-8 sign-off directed it be recorded-rejected rather than fixed in code. Three independent
grounds, each from a pinned requirement rather than from taste:

1. **`Satisfied` after a lost CAS is REQUIRED, not an oversight.** `brief.md` §Success
   criterion: *"A lost CAS is not a blocker … `crates/custodian/tests/backfill.rs:278-325`
   already pins `Reconciled::Satisfied` after a racing writer wins the version-conditional
   commit. 'Declined work ⇒ `Blocked`' must NOT be generalised to 'any unfilled record ⇒
   `Blocked`', or that existing test goes red."* Marking the pass incomplete on a conflict is
   exactly that generalisation: it turns every store with one racing writer into a permanently
   `Blocked` one — the failure mode answer rule 1 exists to prevent.
2. **The gauge is pinned to what the pass READ.** `brief.md` §Scope, answer rule 2: *"An empty
   placement this pass READ stays on the remaining-gauge until a fill is known to have landed —
   including one it declined, and **including one whose CAS was lost**. Only a committed fill
   takes it off."* So the retained placement is the specified value, not a stale one. It is also
   never "zero": `remaining += to_fill.len()` runs at `:202` before the commit and only a
   COMMITTED fill subtracts (`:259`), so a lost CAS leaves the placement on the gauge by
   construction. Erring high is corrected by the next pass's own reading; erring low tells an
   operator the store converged (`docs/principles.md` §5 C-1, the invariant this slice restores).
3. **Re-reading the winner would break leg 4.** `brief.md` §Success criterion leg 4 requires
   *exactly one* `scan(b"inode:")` across `reconcile` and its gauge. Reading the winning record
   back is a second reading of a record this pass already read, and telling the two generations
   apart is a generation comparison — the residue carved out to **#699** at Plan (§Scope: *"A
   generation-restart comparison … DO NOT BUILD. Tracked as #699"*). An in-code
   `// deferred: #699` marker now sits at the conflict arm (`backfill.rs:271-275`), which
   `AGENTS.md` §*Reviewer protocol* ("Deferrals are settled") treats as resolved for review.

A code fix would therefore either double-read the namespace (leg 4 red) or flip the pinned
`Satisfied` (`tests/backfill.rs:278-325` red) — breaking the brief either way.

crates/custodian/src/backfill.rs:254 | BUG | conflict | brief.md §Success criterion pins `Satisfied` after a lost CAS (tests/backfill.rs:278-325) and answer rule 2 pins the placement to stay owed; re-reading the winner is #699's generation comparison and a second reading leg 4 forbids. In-code deferral at :271-275.
crates/custodian/src/backfill.rs:259 | BUG | conflict | same decision at the only site that subtracts: only a fill this pass COMMITTED leaves the population it publishes.
crates/custodian/src/backfill.rs:263 | BUG | conflict | same decision at the conflict arm's rationale comment.
crates/custodian/src/backfill.rs:267 | BUG | conflict | same decision, recorded at the line round 8 cited.
crates/custodian/src/backfill.rs:268 | BUG | conflict | same decision, recorded at the line round 8 cited.
crates/custodian/src/backfill.rs:270 | BUG | conflict | same decision, recorded at the in-code `deferred: #699` marker itself (AGENTS.md §Reviewer protocol: deferrals are settled).
crates/custodian/src/backfill.rs:276 | BUG | conflict | same decision at the `CommitOutcome::Conflict` arm itself.
crates/custodian/src/backfill.rs:280 | BUG | conflict | same decision where the gauge is published: it is a sample of what this pass READ, bounded by the `incomplete` count beside it.
crates/custodian/src/backfill.rs:282 | BUG | conflict | same decision at the outcome fold: a lost CAS is not a decline, so it does not withhold certification (brief.md: "Conflicts stay exactly what they are on the base").

## The same class under its "overcount" phrasing (round 6's blocker 2)

The mirror-image reading — that retaining the placement can OVERCOUNT if the winner's write
filled it — is the same decision seen from the other side, and answered by the same answer
rule 2. Kept as its own MATCH so the decision binds whichever direction a pass argues from.

crates/custodian/src/backfill.rs:202 | BUG | overcount | brief.md §Scope answer rule 2: every empty placement this pass READ is owed until a fill is known to have landed; only a committed fill takes it off (docs/principles.md §5 C-1 — a drain signal may not err toward "converged").
crates/custodian/src/backfill.rs:259 | BUG | overcount | same decision at the only site that subtracts.
crates/custodian/src/backfill.rs:276 | BUG | overcount | same decision at the conflict arm.
crates/custodian/src/backfill.rs:280 | BUG | overcount | same decision where the gauge is published.

## The DST-leg class, pre-declared at Plan

`brief.md` §Verification posture pre-settles it: **no seeded Tier-0 DST case ships in this child,
and none is owed.** The rubric asks a *new concurrent or destructive path* for seeded Tier-0
coverage; this slice adds neither. Every write it performs is on a flat record resolved by borrow
from the generation the scan returned (`crates/core/src/metadata.rs:2585` — a flat snapshot reads
nothing and can never be superseded), committed under the base's own unmodified
version-conditional CAS; the segmented side performs a decline, which writes nothing at all. The
generation-restart comparison that made the previous brief's Rule A a concurrent path is **not in
this patch** and is carved out to **#699**.

crates/custodian/src/backfill.rs:219 | TEST-GAP | dst | brief.md §Verification posture: this slice adds no new concurrent or destructive path — the decline writes nothing and the fill is the base's own unmodified CAS on a flat snapshot (metadata.rs:2585, :2629); the generation-restart path is carved out to #699.
crates/custodian/src/backfill.rs:228 | TEST-GAP | dst | same decision at the fill site: the write is built from the generation the scan returned and conditioned on its bytes, exactly as on origin/main.
crates/custodian/src/backfill.rs:254 | TEST-GAP | dst | same decision at the commit site: this is origin/main's own version-conditional CAS, unchanged by this patch.

## The frozen key-identity lines (#698)

`parse_inode_key` (`backfill.rs:70-76`), its skip (`:146-148`), the CAS key/precondition
(`:249-252`) and the `inode_id` audit fields (`:379`, `:407`) are **byte-identical to
`origin/main`** — verified against `git show origin/main:crates/custodian/src/backfill.rs`
(`:64-70`, `:84-86`, `:142-145`, `:195`, `:223`). The non-canonical-key hazard is real,
pre-existing, unreachable today (`metadata::inode_key` is the sole writer of the `inode:` prefix,
`crates/core/src/metadata.rs:33-36`) and carved out to **#698**; removing that parse produced the
sole blocking finding in rounds 3 and 5. Nothing is recorded here for it: a finding on those lines
should be answered with that reference, since the patch does not touch them and a rejection keyed
to a line this patch never changed would be suppressing review of `origin/main`, not of this
contribution.
