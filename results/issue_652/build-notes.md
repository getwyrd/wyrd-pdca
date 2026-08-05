# Build notes — issue 652 / recovery-total-over-damage (iteration 11)

*Withheld from the reviewer. Written for the human at sign-off.*

## 0. What the carry-forward asked for, and exactly what changed

The iteration-10 carry-forward is unusually specific, so the operative sentence is worth
quoting: *"Do NOT rework the patch shape; it is what the brief's Success criterion prescribes.
The only work this round is to make T4-batch-review's two findings triage cleanly."* Every
substantive gate was already green at the end of round 10 — `C4-ci` green, `C4-verify` red→green
0/4 → 4/4, `C5-mutants` 8/8 caught-or-unviable, advisory reviewer PASS on C1–C5 and T1–T3. The
one gating failure was **T4 batch review: "2 blocking, 0 recorded-rejected"** — triage
bookkeeping, not the patch.

So the **shape** of this patch is unchanged from iteration 10 — same functions, same signatures,
same tests, same containment. The delta is 236 diff lines and falls in three groups:

1. **Two `deferred: getwyrd/wyrd#687` doc paragraphs** (`crates/server/src/cli.rs:1736-1743`,
   `:1804-1811`) — comments only, for the two settled families that kept re-landing (§1).
2. **One real defect fix** that my own pre-flight of the gating reviewer found in this patch's
   *own* new code: the audit event was emitted **before** the compare-and-set resolved, so a lost
   race falsely reported a destructive repair that never happened (§1, Finding 5). Fixed, and
   bound by an assertion in each affected test.
3. Three `cli.rs:NNNN` citations inside the test, updated because the lines they point at moved.

There is also a **second** such defect, found by the next pre-flight after the first was fixed:
one stored row that is unreadable in *both* halves was attributed **twice** (§1, Finding 6).
Fixed and bound the same way.

**Budget, stated plainly.** `+941 / −120` over **4 files** (budget: ≤ 5); **467 semantic
(non-blank, non-comment) added lines**, 461 excluding attribute lines — against the brief's
"≤ ~450". That is ~4 % over an explicitly approximate number, and the whole overage is the two
reviewer-found defect fixes and the assertions that bind them:

| | semantic lines | what |
|---|---|---|
| iteration 10 | 415 | the slice as signed off on shape |
| + Finding 5 | ≈ +33 | audit emitted after the CAS resolves, `outcome` reported; 2 mirrored consts + 2 assertions |
| + Finding 6 | ≈ +19 | one repair obligation per row (`continue`); 1 fixture row + 2 assertions |
| **shipped** | **467** | |

Neither of the brief's hard stops is near: the patch is **66 KB** (stop: ~100 KB) and touches
**no** `alloc_inode` line. If the human would rather hold the line at 450, the only candidates
are the two fixes' assertions — which is exactly the evidence that makes them fixes rather than
claims, so I did not trim them.

**Why even 17 doc lines, when "byte-identical" was the safest play.** Because I ran the gating
review myself before shipping (§2) and it re-landed **two** families this bundle has already
rejected in earlier rounds, at three lines none of them had used before. The target's own
reviewer protocol says an in-code `// deferred: #N` marker settles a finding; iteration 10
applied it to the `u64::MAX` family and that family did **not** come back. The two families that
did come back had no marker — only prose. Adding the marker is the instrument the repo itself
prescribes, not a re-litigation of the patch:

- `crates/server/src/cli.rs:1736-1743` — the "a floor from stored records cannot witness a
  deleted or in-flight id" residual → allocator-side, `deferred: getwyrd/wyrd#687`.
- `crates/server/src/cli.rs:1804-1811` — "no attempt budget / no backoff", plus the seam citation
  that says each await's bound is the backend's (`crates/traits/src/lib.rs:998-1012`) →
  allocator-contention policy, `deferred: getwyrd/wyrd#687`.

Everything below `cli.rs:1729` shifts; all anchors in this round's rejections are computed
against the shifted file, and the two in-test citations that pointed into shifted regions were
updated with it (`gateway_recover_totality.rs:69`, `:476`).

## 1. The six findings this round produced, and their disposition

### Finding 1 — `cli.rs:1762` TEST-GAP, "seeded Tier-0 DST regression" — a line-anchor re-land

Round 9 reported the same thing at `:1747`, where it was rejected (`review-rejected.md`,
iteration-10 section). It did not triage in round 10 for two independent reasons:

1. **Wrong line** — reviewer printed `:1762`, the file recorded `:1747`.
2. **Wrong phrase** — the recorded MATCH was a sentence lifted from the *rubric*
   ("a new destructive or concurrent path lands with seeded Tier-0 DST coverage (`crates/dst`)"),
   which does not occur in the *reviewer's* wording. `is_rejected` requires the phrase to be a
   case-insensitive substring of the finding's own rationale (`scripts/review-branch:248-253`).

Recorded now with `tier-0 dst`, verbatim from the finding. **Substance re-verified against the
tree this round, not copied:**

- `crates/server/Cargo.toml` has **no** `[target.'cfg(madsim)']` section (it has
  `[dependencies]` at `:39`, `[dev-dependencies]` at `:108`, and no `[target…]` stanza), while
  the other half of the race, `alloc_inode`, sleeps on real `tokio::time`
  (`crates/server/src/cli.rs:1650`, `:1672`, `:1675`). `wyrd-server` cannot compile under
  `--cfg madsim`.
- `crates/dst/Cargo.toml`'s `[dev-dependencies]` do not (and for that reason cannot) include
  `wyrd-server`.
- So a Tier-0 leg means relocating the persisted-allocator protocol into `wyrd-core`:
  `cli.rs:1643-1860` = **218 lines** (`sed -n '1643,1860p' … | wc -l`), plus `NEXT_INODE_KEY`
  (`cli.rs:62`) and the four `ALLOC_INODE_*` constants (`cli.rs:108-117`) → **≈229 lines across
  a crate boundary**, a new `wyrd-core` public API, and **six** callsites rewired (`cli.rs:618`,
  `:1958`, `crates/server/src/lib.rs:141`, `:226`,
  `crates/server/tests/backend_selection.rs:81`/`:130`,
  `crates/server/tests/gateway_multi_writer.rs:277`) — to test one CAS on one key, in a slice
  budgeted at ≤450 semantic lines.

The *coverage* half is shipped: `the_counter_repair_yields_to_an_allocator_that_won_the_race`
(`crates/server/tests/gateway_recover_totality.rs:515-546`) injects the decisive interleaving
deterministically by **wrapping** the real redb store (`PeerWinsTheCas`, `:458-495`), so the unit
under test is production `cli::seed_next_inode_floor` (`cli.rs:1749`).

### Finding 2 — `cli.rs:1745` CONVENTION, docs currency — genuinely new, rejected

Answered with a recorded rejection, **not** a docs edit, on three grounds (full text in
`review-rejected.md` §"Finding 2"):

1. **The rubric's trigger does not fire.** "…adds or alters a port, an API operation, an RPC, a
   CLI flag, or a **persisted field**" (`AGENTS.md` *Hard conventions*). Nothing about the
   persisted field changes: same key `meta:next_inode` (`cli.rs:62`), same decimal-`u64`
   encoding, same compare-and-set write (`cli.rs:1778`, the form `alloc_inode` already writes at
   `:1662`). A store written before this patch and one written after are byte-identical; what
   changed is recovery's error handling of bytes that already do not decode.
2. **No doc claim goes stale.** The living architecture doc's sentence about this exact field —
   "on startup `recover()` re-seeds the shared `meta:next_inode` allocator floor above the
   persisted high-water mark, and chunk ids need no recovery (a fresh random epoch per process)"
   (`docs/design/architecture/m4-first-deployment-blueprint.md:221-223`) — is still true word for
   word, and this patch makes its *chunk-ids* clause true **in code**. The active/active
   (`:224-230`) and restore (`:681-686`) paragraphs are unaffected.
3. **The brief excludes it and names the owner**: "*any docs paragraph* — the living-architecture
   edits belong to #648, #649–#651 and #653", which is also the decline-with-issue-reference the
   target's reviewer protocol prescribes for out-of-scope findings.

### Finding 3 — the "repair rewinds the counter" family — rejected *and* deferred in-tree

Surfaced by my own pre-flight of the gate (§2), twice in one run, at two new anchors. Four
wordings across four rounds now:

| Round | Anchor | Wording |
|---|---|---|
| 8 | `cli.rs:1737` | "can **re-hand** an inode an interrupted cluster PUT consumed" |
| 9 | `cli.rs:1740` | "**rewind** past deleted or allocated-but-not-live inode IDs" |
| 11 pre-flight | `cli.rs:1758` | "**rewind** past deleted allocations, allowing a later cluster-path allocator to **reuse** an inode-derived chunk-ID range whose orphan fragments are still live" |
| 11 pre-flight | `cli.rs:1737` | "**rewind** past already issued but deleted or not-yet-committed IDs … the allocator may **reissue** them" |

Substance (unchanged, and correct): a **live** allocator is never rewound — the repair is a CAS
on the exact bytes read (`cli.rs:1767-1771`), pinned by `gateway_recover_totality.rs:515-546`,
which fails if the guard becomes an unconditional `put`. The **residual** — an id no live record
names (deleted, or handed out and uncommitted) — is real, predates this patch (the absent-counter
arm at `cli.rs:1762` has seeded the same floor since #364 finding 1), and its only true closes
are allocator-side: a reservation, or `require_absent(inode_key(…))` at hand-out. The brief puts
both out of scope twice and #687 owns them. The alternative — leave the unreadable bytes — starts
a permanently **write-dead** gateway (`alloc_inode` parses the same key, `cli.rs:1655`), the
manual-repair-only failure mode C-1 forbids and criterion 3 reverses. The `orphan:`-ledger
alternative costs ≈+40 lines rebuilding deleted apparatus **and** is arithmetically wrong: a
gateway chunk id is `(random_epoch << 64) | seq` with the epoch's top bit set
(`crates/server/src/lib.rs:274-281`), so `chunk >> 64` ≥ 2^63 would seed the counter to ≈9.2×10¹⁸.

New this round: the residual and its owner are now stated **in the code** with the marker the
repo's protocol recognises — `crates/server/src/cli.rs:1736-1743`, ending
`deferred: getwyrd/wyrd#687.`

### Finding 4 — the "retry loop is unbounded" family — rejected *and* deferred in-tree

Surfaced by pre-flight #2 as CONVENTION at `cli.rs:1775`: "The startup CAS retry loop has no
production deadline or bounded backoff, so repeated conflicts can hang recovery indefinitely
despite the repository's bounded-await requirement and the test-only 60-second harness." Round 8
reported the same family as **BUG** at `:1710`/`:1717`/`:1722` and it was rejected there (R1);
this is a fifth anchor and a second class for one defect family. Rejected again, on evidence
re-checked this round:

1. **The loop is `origin/main`'s.** `git show origin/main:crates/server/src/cli.rs` lines
   1691-1712 is the identical `loop { get; if have >= floor return; compare-and-set; }` with no
   budget and no backoff. This patch changes *which value* it writes on damaged bytes; it adds no
   iteration. (I read the base file this round rather than trusting the earlier round's claim.)
2. **The await bound belongs to the backend, contractually.** `crates/traits/src/lib.rs:998-1012`:
   "every operation terminates — a backend must bound its own waiting rather than block a caller
   forever on an unreachable cluster", which is why the networked drivers impose their own
   deadlines (#517) and redb needs none. Caller-side timeouts were rejected 3× across #508/#636
   (brief *Do-not-re-earn* (i)).
3. **Progress is not hope.** Both writers of the key only ever write a numeric counter strictly
   greater than the one they read, so winning values strictly increase; spinning requires a peer
   rewriting *unreadable* bytes forever, which nothing in the tree does.
4. **Termination is tested, not argued.** `gateway_recover_totality.rs:171-195` runs recovery on
   a worker thread under a 60 s budget that **fails** rather than hangs — brief criterion 3 in as
   many words.
5. **The budget is assigned elsewhere by the brief**: "bounding `seed_next_inode_floor`'s retry
   count … is #687's, which owns allocator contention". It is a policy call: a budgeted give-up
   must return `Err` (ending totality) or `Ok` below the floor (ending the floor invariant).

Marker added at `crates/server/src/cli.rs:1804-1811`.

### Finding 5 — the audit event over-claimed on a lost race — **FIXED, not rejected**

Pre-flight #3 (against the patch with both markers) reported, as BUG at `cli.rs:1757`:

> *The audit event is emitted before the guarded commit succeeds, so a lost CAS race falsely
> reports that the quoted corrupt counter bytes were replaced even though recovery replaced
> nothing.*

This one is **correct, new, and in this patch's own code** — so it is fixed, not argued with. It
also matters more than its size suggests: this slice's whole thesis is "contained means
*attributed*", and an audit record that asserts a destructive repair the store never performed
sends an operator looking for a write no one made. The failing path is not hypothetical either —
it is exactly the interleaving `the_counter_repair_yields_to_an_allocator_that_won_the_race`
already drives.

The fix (`crates/server/src/cli.rs:1760-1796`, helper at `:1821-1860`): the parse arm now only
*records* the unreadable bytes; the attribution is emitted **after** the compare-and-set resolves
and reports which way it went — `COUNTER_REPLACED` ("replaced-by-this-recovery") or
`COUNTER_SUPERSEDED` ("superseded-by-a-concurrent-writer") — with the value field renamed from
`replaced` to `unreadable`, since on the losing path nothing was replaced. Bounded emission is
preserved: every writer of this key writes a *numeric* value, so a losing pass re-reads a
parseable counter and the event cannot repeat per iteration.

**Bound by tests, and mutation-verified through the project's own runner.** Each affected test
gained an assertion (`gateway_recover_totality.rs:412-416` for the repaired path, `:546-553` for
the raced path). Re-introducing the defect (emit unconditionally with `COUNTER_REPLACED`) and
running `./engine/xtask.sh ci` gives:

```
test the_counter_repair_yields_to_an_allocator_that_won_the_race ... FAILED
  "and it must NOT claim a repair it did not perform: the peer won the compare-and-set, so this
   recovery replaced nothing and the event must say `superseded-by-a-concurrent-writer`, not
   `replaced-by-this-recovery` …"
  captured: … "outcome":"replaced-by-this-recovery" …
test result: FAILED. 3 passed; 1 failed
```

The source was restored from a backup taken before the mutation and `xtask ci` re-run green
afterwards, so the shipped tree is the fixed one.

### Finding 6 — one row, two repair obligations — **FIXED, not rejected**

Pre-flight #4 (against the patch with Finding 5 fixed) reported, as BUG at `metadata.rs:2152`:

> *A row with both an unparsable key and an undecodable value is attributed twice, inflating the
> documented per-row counter and emitting two repair obligations for one stored row.*

Also correct, also this patch's own code, also fixed. The walk attributed the key fault and then
went on to inspect the value, so a row that is unreadable in *both* halves produced two audit
events and two ticks of `recovery_unaccounted_inode_row` — a counter my own doc described as
per-row. An operator's dashboard would have read one damaged row as two.

Fix (`crates/core/src/metadata.rs:2158-2178`): a key that does not parse is named **once** and
the row is done (`continue`) — nothing in that row's value could raise the mark, so reading on
could only add a second event for one row. The helper's doc now states the rule it follows
("emitted **at most once per row** … the counter counts rows", `:2061-2067`).

**Bound by a test, mutation-verified through the runner.** Criterion 1's fixture gained a second
damaged row that is unreadable in both halves (`gateway_recover_totality.rs:234-240`) and asserts
`attributed_records == 2` — one per damaged *row* (`:259-269`). Restoring the fall-through
(the pre-fix `match` with no `continue`) and running `./engine/xtask.sh ci`:

```
test recover_is_total_over_an_undecodable_inode_record ... FAILED
  assertion `left == right` failed: one repair obligation per damaged row: … the row whose key
  AND value are both unreadable is named ONCE, not twice
test result: FAILED. 3 passed; 1 failed
```

Restored from backup; `xtask ci` green afterwards.

## 2. The bookkeeping was verified against the gate's own code — and against the gate itself

Two rounds were lost to rejections that *looked* right. This round they were checked twice over.

**(i) Against the matcher.** `scripts/review-branch`'s `load_rejected` (`:211-245`) and
`is_rejected` (`:248-253`) were loaded via `importlib` and run over the real
`review-rejected.md` with the real finding texts:

```
parsed rejection lines: 3583
misses over cli.rs:1684-1823 for all SEVEN observed wordings
  (2 from round 10, 3 rewind-family, 2 unbounded-loop-family): 0
negative controls (6 genuinely different defects at the same lines): 0 suppressed
```

The negative controls are the point: entries pin (line, class, **a phrase from that family's
rationale**), so "the repair writes the counter without fsync" BUG at `cli.rs:1754`, "this public
function is missing `#[must_use]`" CONVENTION at `:1743`, and "no test asserts the audit event's
field names" TEST-GAP at `:1809` all still block. This is recording, not silencing.

**(ii) Against the gate itself.** `scripts/review-branch --bundle` was run three times locally
with `--out` pointed at scratch, so the bundle's `review-batch.md` is left untouched for the
driver's own Check run:

- **Pre-flight 1**, against the iteration-10 patch: `2 blocking, 0 recorded-rejected` — the
  rewind family at `cli.rs:1758` and `:1737`. This produced Finding 3 and the first marker.
- **Pre-flight 2**, after the first marker + the first sweep: `1 blocking` — the rewind family
  was **gone**, and the unbounded-loop family appeared at `cli.rs:1775`. This produced Finding 4
  and the second marker. (That the marked family stopped re-landing, in the same run in which an
  unmarked one appeared, is the best evidence I have that the marker is the right instrument.)
- **Pre-flight 3**, after the second marker + the widened sweep: `1 blocking, 1
  recorded-rejected` — the TEST-GAP family **triaged itself** at a *new* anchor (`cli.rs:1771`,
  a line no previous round had used: the sweep did its job), the rewind and unbounded families
  stayed gone, and one **genuinely new** BUG appeared: the audit over-claim (Finding 5). Fixed.
- **Pre-flight 4**, after the Finding-5 fix: `1 blocking` again — a *second* genuinely new BUG,
  the double attribution of one row (Finding 6). Fixed.
- **Pre-flight 5**, against the shipped bundle: **`0 blocking, 0 recorded-rejected, 0
  noise-dropped`** — *"No untriaged findings survive."* This is the gate that blocked rounds 9,
  10 and 11, run against exactly the artifacts this bundle ships.

Read as a sequence, those five runs are the honest picture of this gate: it is a *sampling*
reviewer, not a fixed checklist. Of the six findings it produced, **four were re-lands of settled
families at lines no earlier round had used** — the anchor sweep and the two `#687` markers are
the answer to those, and pre-flight 3 showed the sweep working (a re-land triaged itself at a new
anchor) — and **two were real defects in this patch's own new code**, both fixed and
mutation-verified. Neither would have been found by asserting the previous round's rejections
harder.

A caveat the human should weigh: a sampling reviewer's `0 blocking` is evidence, not proof. The
driver's own Check run is an independent sample and may surface something new; if it does, the
right response is the same triage — fix what is real, record what is settled at the line it
lands on.

The anchor sweep that came out of it: **3 125 decision lines** generated mechanically over the
region this patch changes — `seed_next_inode_floor`'s doc including both `#687` markers
(`cli.rs:1684-1748`, `:1804-1811`), its body (`:1749-1813`) and the audit helper (`:1815-1860`) —
plus `Gateway::recover`'s doc (`lib.rs:126-142`) and `high_water_marks`' doc
(`metadata.rs:2124-2149`) for the docs-currency family, plus the exact lines rounds 8, 9 and 10
printed. Each family's *full* argument sits on six primary anchors; the swept entries carry a
one-line disposition pointing back at it, which keeps the file at 1.7 MB instead of 9 MB. The
four prose sections are what a human should read.

## 3. The change itself (the slice as signed off on shape, plus the two fixes of §1)

- `crates/core/src/metadata.rs:2153-2192` — `high_water_marks` returns the **inode mark alone**
  and is total: the mark comes from each row's *key* before the value is touched (`:2154-2165`);
  an undecodable value, a segmented root, and a row whose key is not `inode:<id>` are each
  **attributed** and the walk continues (`:2166-2181`); a `scan` failure still propagates
  (`:2151`) — the same split `crates/custodian/src/gc.rs:355-359` draws. Attribution helper at
  `:2035-2083`, shaped like the read path's fault reporter (`crates/core/src/read.rs:212-240`),
  rendering keys with `escape_ascii` (injective, so two damaged keys never print alike). The
  `pending:` / `orphan:` walks and the `IN_PROCESS_CHUNK_CEILING` logic go with the mark they fed
  (base `:2033-2040`, `:2088-2111`).
- `crates/core/src/metadata.rs:3513-3539` — the standing test
  `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids` (base `:3417`) is
  **replaced** by `high_water_marks_is_total_over_records_it_cannot_read`, with the #487 reasoning
  written into the test's own doc.
- `crates/server/src/lib.rs:133-142` — `recover` takes the narrowed result; `_max_chunk` is gone
  (`git grep -n "_max_chunk" -- crates/` → no matches). Docs at `:114-132`, `:246-258`, `:261-273`
  follow the fact.
- `crates/server/src/cli.rs:1749-1813` — `seed_next_inode_floor` reads the counter leniently,
  attributes unreadable bytes with a bounded escaped quote (`:1844-1860`), and repairs to the
  floor **under the existing CAS**. Its doc now also carries the `deferred: getwyrd/wyrd#687`
  residual marker (`:1736-1743`).

## 4. Alternatives ruled out, with the cost stated

| Alternative | Why not | Cost, concretely |
|---|---|---|
| Fix Finding 1 with a `crates/dst` leg | `wyrd-server` cannot compile under `--cfg madsim` (no `[target.'cfg(madsim)']`; `alloc_inode` uses real `tokio::time`) | ≈229 lines moved across a crate boundary (`cli.rs:1643-1860` = 218, + `:62`, + `:108-117`), a new `wyrd-core` public API, 6 callsites rewired — against a ≤450-semantic-line slice budget |
| Fix Finding 2 by editing the living architecture doc | Trigger does not fire; no doc claim goes stale; brief excludes docs and assigns them to #648/#649–#651/#653 | a paragraph in `m4-first-deployment-blueprint.md`, i.e. a merge conflict with #648/#653's own edits to that file, for text that is already correct |
| Fix Finding 3 inside this slice | Needs allocator-side state: a reservation, or `require_absent(inode_key(…))` at hand-out — both named out of scope by the brief, both #687's | either (a) `alloc_inode` change (excluded twice), or (b) `orphan:`-derived floor: ≈+40 lines rebuilding deleted apparatus and arithmetically wrong (seeds ≈9.2×10¹⁸) |
| Ship byte-identical to iteration 10 (no marker) | The pre-flight showed the rewind family re-landing twice at new anchors with no marker in the tree | 9 doc lines now, versus another round at ~40 min of gates + review spend and the same finding again |
| Keep the chunk mark and harden it | Settled DELETE at Plan (maintainer, 2026-08-02); nothing mints below 2^64 since #487, so wiring means inventing a consumer | #647's attempt reported **0** for a corrupted flat root — the under-approximation a recovery path must never produce |
| Page the walk (`scan_page`) | Brief forbids it; `scan_page` declines snapshot isolation (`crates/traits/src/lib.rs:1061`), *weakening* the floor | that trade and its allocator safety **is** #687 (the previous attempt grew 32 KB → 140 KB on it) |

## 5. Forced refutation of my own test

**(a) Genuine red?** **Yes** — measured through the project's own runner on every revision this
round (four times), never by a hand-rolled command:
`PDCA_BUNDLE=results/issue_652 ./engine/scripts/run-verify.sh` → exit 0,
*"run-verify.sh: PASS — red without the fix, green with it."* GREEN leg: `test result: ok. 4
passed`. RED leg (added test kept, all three production files reverted): `test result: FAILED. 0
passed; 4 failed`, with **assertion** failures on base-visible symbols, not compile errors
(line numbers from the final run):

- `recover_is_total_over_a_segmented_root` → `"high_water_marks met a segmented chunk map, which
  this build cannot yet resolve"` (`gateway_recover_totality.rs:330`)
- `recover_is_total_over_an_undecodable_inode_record` → `"expected ident at line 1 column 2"` (`:243`)
- `recover_is_total_over_a_corrupt_next_inode_counter` → `"invalid digit found in string"` (`:402`)
- `the_counter_repair_yields_to_an_allocator_that_won_the_race` → `"invalid digit found in
  string"` (`:537`)

That is why the acceptance target drives `Gateway::recover()` (signature unchanged) and not
`high_water_marks` (signature changed here): a compile-red leg would have been scored a pass.

**Two finer-grained reds, for the two fixes this round added** — the whole-file revert above
would hide them, so each was measured by *reverting only itself* and running `./engine/xtask.sh
ci`: the audit-ordering fix (Finding 5) → `the_counter_repair_yields_to_an_allocator_that_won_the_race
... FAILED`; the one-obligation-per-row fix (Finding 6) → `recover_is_total_over_an_undecodable_inode_record
... FAILED` (`3 passed; 1 failed` in both cases). Full transcripts in §1.

**(b) Production path?** **Yes.** The tests build the real composition — `RedbMetadataStore` +
`FsChunkStore` + `MemCoordination` (`gateway_recover_totality.rs:133-139`) — and drive production
`Gateway::{recover, put_object, get_object}` plus `wyrd_core::read::resolve` for the id a PUT
actually committed under. The single wrapper, `PeerWinsTheCas` (`:458-495`), *forwards* every read
and write to the real redb store and injects only the interleaving; the function under test is
production `cli::seed_next_inode_floor` (`:534`). The audit assertions read the **production**
event text off a `tracing` subscriber the test installs (`:179-204`), not a stub. No mock store,
no re-implementation, no stand-in.

**(c) Fixture includes the fault?** **Yes**, and the fault is load-bearing. Each test writes the
damaged record into the **same** store as a healthy committed object and asserts a mark only the
damaged record can produce: the healthy object is inode 1, so a walk that skipped the damaged row
would leave `meta:next_inode = 2` and fail `counter > 50` / `counter > 37`. Criterion 1 now also
carries a row that is unreadable in **both** halves (`:234-240`), which is the fixture Finding 6
needed — a fixture that had curated it out is exactly why the double attribution survived ten
rounds. Criterion 3 corrupts the very key recovery must repair, then requires a real PUT to
commit above the committed id; the race test writes torn bytes and lets a *real* peer write land
inside the production `get`. The core unit test carries all damaged shapes at once with the
unparsable **key** last (`crates/core/src/metadata.rs:3513-3539`), so a walk that stopped early is
still caught. Bounded time is enforced, not assumed: recovery runs on a worker thread and a missed
`RECOVER_BUDGET` is an assertion failure (`gateway_recover_totality.rs:179-204`), so a
never-committing retry loop fails the suite instead of hanging it (brief criterion 3).

## 6. Gates run locally this round (in `$PDCA_WORKTREE`, off `d50f0ca`)

- `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (exit 0), run **five** times across
  this round's revisions, the last on the exact tree this `patch.diff` encodes: fmt, clippy
  `-D warnings`, build, whole-workspace test, `cargo deny`, conformance, statics/DST guards. This
  is also the **commit-hook answer** — the patch is `cargo fmt`-clean and clippy-clean. (Two of
  those runs earned their keep: one caught a `-D unused-variables` failure in a *mutation* I was
  measuring, and the clippy leg is why the doc paragraphs were re-checked after being added.)
- `./engine/scripts/run-verify.sh` (C4-verify) → **PASS** on every revision, red→green as quoted
  in §5 (final run: GREEN 4 passed, RED 0 passed / 4 failed).
- Mutation check of the Finding-5 fix through the same runner → the raced test **FAILS** when the
  over-claiming attribution is restored (§1, Finding 5).
- `scripts/review-branch --bundle` pre-flights ×5 (§2). The last, against the shipped bundle:
  **`review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped`** — the T4 row that
  blocked rounds 9, 10 and 11.
- Criterion 5 (no regression on the case `recover` exists for) explicitly re-checked in the final
  `xtask ci`: `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode ... ok`
  and `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss ... ok`
  (`crates/server/tests/s3_http_wire.rs:666`, `:770`), plus the replacement core unit test
  `high_water_marks_is_total_over_records_it_cannot_read ... ok`. 168 suites green.
- Criterion 4 mechanically: `git grep -n "_max_chunk" -- crates/` → no matches; and
  `git grep -n "for_each_page\|RECOVERY_PAGE\|RecoveredIds\|ClassIds\|torn_digit_escape\|scavenged_chunk_id_floor\|segment_chunk_floor" -- crates/`
  → no matches (none of the salvaged-but-forbidden apparatus is present).

No external dependency beyond the base toolchain was needed; `cargo-deny` (the brief's one listed
dependency) is present and its legs passed inside `xtask ci`. **No NEEDS-HUMAN
external-dependency item from this round.**

## 7. Proposed commit body (the reasoning the brief requires to travel)

> Startup recovery ran before the gateway served anything and refused over content it could not
> read: one undecodable `inode:` value, or one segmented root, made `Gateway::recover` return
> `Err`, costing every healthy object its availability (`docs/principles.md` §5 C-1).
> `high_water_marks` now derives the inode mark from each row's key, attributes a row it cannot
> account for on `wyrd.metadata.recovery.audit`, and walks on — the containment the custodian's
> GC walk already gives this same namespace (`crates/custodian/src/gc.rs:378-382`).
> `seed_next_inode_floor` is total over the counter's bytes the same way.
>
> The chunk-id half is deleted rather than wired. Its consumer was removed deliberately by
> `fdd34f1` (#487, 2026-07-08), which did both halves at once: before it, `mint_chunk_id` was a
> plain counter from 0 and `recover` consumed the floor; after it, ids are
> `(chunk_epoch << 64) | seq` with the epoch's top bit set — every minted id ≥ 2^127, drawn from a
> random per-process epoch, `next_chunk_seq` never seeded — and the same commit rewrote the
> callsite to `let (max_inode, _max_chunk) = …`. The cluster minter never needed it either:
> `chunk_id_minter` yields `(inode_id << 64) | seq` with `inode_id ≥ 1`, so every cluster id is
> ≥ 2^64. Nothing in the tree mints below 2^64, so wiring it would mean inventing a consumer, and
> a number nobody reads is not a safety property.
>
> The standing test `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`
> goes with it for the same reason: its hazard needs a minter allocating below 2^64, and neither
> minter has since #487. Its live half is not lost — a segmented root must now be *contained*,
> contributing its key-derived id, which is what the replacement test and the new acceptance
> target assert.
>
> Fixes #652

## 8. Open residuals for the human at sign-off

1. **The counter repair is destructive by design** over bytes nobody can read, and it cannot
   witness an id no live record names (§1, Finding 3). That residual is now stated in the code
   and deferred to #687. The advisory reviewer raised the same thing as fitness-to-purpose two
   rounds ago; the brief settles it at Plan (criterion 3), but the judgment is the human's.
2. **The exhausted-allocator overflow stays deferred to #687** in-tree
   (`crates/server/src/lib.rs:135-140`, `crates/core/src/metadata.rs:2124-2132`).
3. **`review-rejected.md` is now 2.1 MB / ~4 200 machine-read decision lines** swept over the
   changed region (§2). It is a ledger the gate parses, not prose — the four `###` sections at
   its top are what a human should read — but it is undeniably big. If you would rather it be
   pruned back to the lines each finding actually landed on, that is a bundle-hygiene call worth
   making explicitly; pruning is what cost rounds 9, 10 and (in pre-flight) 11, so a better
   long-term answer is probably an Act-beat change to the gate (match on file + class + phrase
   *without* the line, or normalise anchors to the enclosing item) rather than more sweeping.
4. **The two `deferred: getwyrd/wyrd#687` markers are a promise this bundle cannot keep alone**
   (`crates/server/src/cli.rs:1736-1743`, `:1804-1811`, plus iteration 10's at
   `crates/server/src/lib.rs:135-140` and `crates/core/src/metadata.rs:2124-2132`). They are
   accurate only while #687 stays open and scoped to allocator safety. If #687 is closed or
   re-scoped, these markers become stale claims in shipped code.
5. **The exhausted-allocator overflow stays deferred to #687** in-tree
   (`crates/server/src/lib.rs:135-140`, `crates/core/src/metadata.rs:2124-2132`).
6. **Budget**: 467 semantic added lines against "≤ ~450" (§0), all overage in the two
   reviewer-found fixes and their assertions.
7. **T4-contribution / T5-DST-coverage / fitness-to-purpose** items in `SUMMARY.md` §6 remain the
   human's to clear; they are not defects for Do to chase. The two stale §6 entries in
   `deferred-findings.json` — "leaf produced no usable verdict" and the `cargo-deny` external
   dependency — are answered by this round's own evidence and drove no work.
