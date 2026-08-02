# Build notes — issue 649 / shared-segmented-map-resolver-and-read-paths (iteration 5)

Withheld from the reviewer; for the human at sign-off.

Line citations are against `$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`,
whose HEAD is `6e7c255` (`pdca-integrate: issue_648`) = `origin/pdca-integration/main` — the
base `C4-verify` resolves for this wave>0 bundle, and the tree every line below is measured
on **with `patch.diff` applied**.

Iterations 1–4 are preserved in `iteration-v1/` … `iteration-v4/`. **This round is a delta on
iteration 4's patch**, which was green on every gate except the batched review
(`iteration-v4/check-gates.md`: `C4-ci` pass, `C4-verify` pass, `C5` 0 missed,
`T4-contribution` pass, `T4-batch-review` **fail — 3 blocking**). §1 is the disposition of
those three, §2 the change and how it was measured, §3 the inherited patch in brief, §§4-8
the costs, the budget position, the gate evidence and the three refutation answers.

---

## 1. The carry-forward, one line each

| Finding (`review-batch.md`, round 4) | Disposition |
|---|---|
| `crates/core/src/metadata.rs:2375` **TEST-GAP** — "No test repeatedly retires all `MAX_RESOLVE_RESTARTS` generations, leaving the new bounded-exhaustion and fail-closed `MapResolutionUnstable` behavior unverified" | **FIXED** (§2.1–2.2): two new cases, one at the base-visible read path and one co-located naming the typed variant |
| `crates/core/src/metadata.rs:2375` **TEST-GAP** — same class, second pass: "every race fixture flips only once to a flat generation" | **FIXED** by the same two cases — the new double flips *repeatedly*, and to a **segmented** generation each time, which is what makes restarting stop helping |
| `docs/design/architecture/06-runtime-view.md:29` **CONVENTION** — "claims every consumer uses the resolver, but the custodian loops still access `chunk_map.as_flat()` and the new wrapper is not wired until #650/#651" | **FIXED** (§2.3): the sentence now states what is true *today* |
| C3 `NEEDS-HUMAN` (deferred) — the lockfile-only `event-listener` 5.4.1→5.4.2 hunk | **Still the human's call** (§4.3); evidence for the decision is now concrete (RUSTSEC id + the dependency chain) |
| C1 `NEEDS-HUMAN` (deferred) — is universal consumer routing required in this slice? | The documentation half of it is now moot: the doc no longer claims universal routing (§2.3). The invariant half is the human's |

Nothing in the rejected-approach set was re-attempted: no round has previously answered this
finding, and the fix is additive test coverage plus one sentence of prose.

## 2. What changed this round

### 2.1 The gap, precisely

`resolve_current_chunk_map` restarts while a generation is retired under it and, past
`MAX_RESOLVE_RESTARTS` (`crates/core/src/metadata.rs:2351`), fails closed with
`ChunkMapError::MapResolutionUnstable` (`:2389`). Rounds 1–4 exercised the *benign* arm from
every angle — root superseded, root deleted, segment reclaimed, malformed key, over-ceiling
root superseded — but **every one of those fixtures flipped once, to a flat generation**, so
the restart always succeeded on the first retry. Nothing drove the loop to its bound. Two
behaviours therefore had no oracle at all:

- **termination** — a resolver that simply kept restarting would spin forever on a hot
  object; and
- **the answer at the bound** — whether giving up is a typed error *for this object* or the
  data-losing `Ok(None)` ("this object owns no bytes"), which is the exact C-1 failure mode
  decision 7(h) exists to prevent.

### 2.2 The two cases

Both are driven by the same idea and neither could be reached by the existing doubles: the
object is republished — **whole, segmented and resolvable at every instant** — faster than a
reader can resolve it, so each snapshot is retired before its own re-check settles it.

**(a) Base-visible, in the mandated discriminator file.**
`crates/core/tests/segmented_map_resolution.rs:1131`,
`a_generation_retired_under_every_restart_fails_closed_and_terminates`, reached only through
`read_object`:

- `Quirk::SupersedeEachGeneration` (`:341`) + `Probe::superseding_each_generation` (`:445`) +
  the `scan_page` arm (`:550`): the **first page of each distinct `seg:` range** publishes the
  next queued generation. Retiring per *range* rather than per *page* keeps the fixture
  independent of how many pages a group takes.
- `segmented_generation` (`:237`) builds each generation as ONE batch — root **and** both
  `seg:` rows, as a publisher must — over the *same* chunks, so the payload can never explain
  a refusal. `segmented_batch` (`:191`) was factored out of the pre-existing
  `commit_segmented` so the seeding path and the churn path mint identical records.
- The queue holds `CHURN_GENERATIONS` = 4 (`:389`) — exactly one per resolve attempt the
  reader may make — and runs dry immediately after, so a **wider** budget resolves the next
  generation cleanly and the test sees `Ok`. That is the bind from above.
- The bind from below is `EXHAUSTED_RESOLVE_ROOT_GETS = 8` (`:382`), asserted through
  `assert_gets_only_the_root_of` (`:469`): the read's own root read plus one re-check per
  attempt, over the first resolve and all three restarts. Deliberately a **literal**, not a
  formula over an exported constant — the budget is the reader's own policy, so a change to it
  must be a test failure, not a silently rescaled expectation.
- The control at the end: with the queue dry the **same store, same object, same `seg:`
  records** read back byte-identical to the flat payload. That is what separates "the race"
  from "broken data", and it is why the refusal cannot be a fixture artefact.

**(b) Co-located, naming the typed variant.**
`crates/core/src/metadata.rs:3409`,
`a_generation_retired_under_every_restart_is_a_typed_refusal_not_an_empty_map`, in
`mod segmented_shape_invariants` where every other resolver-shape case lives (covered by
`C4-ci`, per the brief's Test-file constraint). The integration file may not import anything
this patch adds, so this is the only place the finding's own words —
`MapResolutionUnstable` — can be asserted: `resolve_current_chunk_map` answers
`ChunkMapError::MapResolutionUnstable { attempts: MAX_RESOLVE_RESTARTS }` (`:3429`), never
`Ok(None)`, at a cost of exactly `2 × MAX_RESOLVE_RESTARTS` root reads, each of which is
asserted to be the object's own root (`root_reads`, `:3361`). Same control ending: the churn
stops, the same store resolves the live generation whole.

**Measured, both directions** (production restored from a scratch copy after each mutation):

| Mutation of production | co-located case | base-visible case |
|---|---|---|
| `MAX_RESOLVE_RESTARTS: 3 → 4` (`crates/core/src/metadata.rs:2351`) | passes — its fixture and its expectation both scale with the constant | **FAILS**: `a resolution that never settles must fail closed for THIS object: Some([0, 1, 2, …])` — the queue ran dry, the widened budget resolved a generation, and the read answered bytes |
| exhaustion returns `Ok(None)` instead of `Err(MapResolutionUnstable{..})` (`:2389`) | **FAILS** | **FAILS**: `… must fail closed for THIS object: None` |
| nothing (as shipped) | passes | passes |

That is the split on purpose: the co-located case binds the **shape** of the refusal against
whatever the code declares its budget to be; the base-visible case pins the **value**
end-to-end. Neither alone covers both, and the cross-reference is recorded in the code
(`crates/core/src/metadata.rs:3442-3447`).

Everything else in production is **byte-identical to iteration 4** — the resolver, `read.rs`,
`resolve.rs`, `custodian/lib.rs`, `server/lib.rs` and the DST property are untouched this
round. In particular the line anchors the recorded rejections in `review-rejected.md` are
keyed to (`:2090`, `:2166`, `:2182`, `:2197-2199`, `:2376`) are **unchanged**: every line I
added to `metadata.rs` is inside the test module, which starts at `:2515`.

### 2.3 The living-architecture sentence

`docs/design/architecture/06-runtime-view.md:29` said *"Every consumer resolves through the
**same** call"*. The reviewer is right that this is a claim about the present tense that the
present tense does not support: nine `.chunk_map.as_flat()` sites remain in the custodian
passes (`crates/custodian/src/gc.rs:266`, `rebalance.rs:161`,`:258`, `restore.rs:386`,
`reconstruction.rs:331`,`:582`,`:635`, `backfill.rs:98`,`:180`), and their adoption is #650/#651.

It now reads: resolution is **single-sourced** — one call, and the read paths (whole-object,
streaming, ranged) go through it "so how an object's chunks are learned is written once, not
once per consumer" — followed by what the un-adopted consumers actually do: *"A consumer that
has not adopted that call refuses a segmented map outright rather than answering from the root
alone."* I verified that claim rather than assuming it: every one of the nine sites raises
`ChunkMapError::SegmentedMapUnsupported` (e.g. `crates/custodian/src/gc.rs:260-267`,
`rebalance.rs:155-164`), so none of them can under-approximate an object's bytes. The clause
that follows says *why* that matters, which is the C-1 invariant this slice restores. No
staging language and no issue numbers went into the living doc, and the containment sentences
the brief reserves for #650/#651/#653 are still not written.

## 3. The inherited patch, in brief

For the human who has not read `iteration-v4/build-notes.md`. Unchanged this round.

- **One resolver** — `crates/core/src/metadata.rs:2328` `resolve_chunk_map` (snapshot →
  ordered chunks; flat borrows, segmented reads its own range), `:2371`
  `resolve_current_chunk_map` (resolve the **live** root, for a stale-snapshot caller), `:2418`
  `resolve_live_chunk_map` (the read path's entry: the caller's snapshot, or the generation
  that replaced it). Result type `MapResolution` at `:2073`.
- **Bounded** — `read_group_range` (`:2180`) pages `seg:<nonce>:<epoch>:` only, one page wider
  than the root's own claim, with the ceiling refused **before** the first page (`:2185`), and
  keys parsed rather than trusted (`:2206`,`:2214`).
- **Total** — every anomaly is *described*, never judged where it was noticed, and settled by
  the one arbiter `retired_or` (`:2113`) which re-reads the root: retired ⇒ restart, still
  named ⇒ typed error for that object. A complete-but-stale read is settled the same way
  (`:2313`).
- **Routed** — `read.rs:506` (whole object; byte assembly now takes a resolved list at
  `:69`), `server/src/lib.rs:355` (streaming) and `:446` (ranged), each framing its response
  from the generation the bytes came from.
- **Maintenance door** — `crates/custodian/src/resolve.rs:43` `chunks_of` (live-root, never the
  pass's own scan snapshot) and `:98` `classify_root` (per-object containment of an undecodable
  record), with unit coverage in this slice; callers are #650/#651.
- **DST** — `crates/dst/tests/custodian.rs:1462` `prop_segmented_resolve_never_tears`, 50 seeds
  under `cargo xtask ci`, with a real nemesis (on half the seeds the drain has already
  reclaimed segment 1 of the retired generation).

## 4. What I ruled out, with the cost

### 4.1 Putting the exhaustion case only in the integration file

Would have saved the co-located double and fixture: **180 added lines**
(`crates/core/src/metadata.rs:3276-3455`), of which 128 are semantic. Rejected because the
finding's subject is `MapResolutionUnstable` and the brief forbids the integration files from
importing anything this patch adds ("Both MUST import only symbols visible on this slice's
base"), so the *typed* variant and its `attempts` field would remain unasserted anywhere —
exactly the hole the finding names. The integration case alone proves "an error, not a 404";
it cannot prove "*this* error".

### 4.2 Putting it only in a co-located case

Would have saved the whole churn fixture in the discriminator file — **151 added lines**
net over round 4: the test body (`crates/core/tests/segmented_map_resolution.rs:1110-1163`),
the quirk (`:331-352`), its constructor (`:442-457`), the `scan_page` arm (`:549-562`) and the
`segmented_batch`/`segmented_generation` split (`:186-260`). Rejected because a
co-located test earns no per-fix red (`run-verify.sh` classifies it into the green-only
branch), and because the behaviour the finding is about — "a read of a live object must not
answer no-such-key" — is only observable end-to-end at `read_object`. It is also the only one
of the two that catches a widened budget (§2.2 table).

### 4.3 Dropping the `Cargo.lock` hunk (the human's deferred C3)

Cost, measured rather than asserted: reverting **that one hunk** (2 insertions, 3 deletions at
`Cargo.lock:1202-1211`) and running `cargo deny check advisories` in the worktree reproduces

```
error[unsound]: `event-listener` allows `!Send` tags to cross thread boundaries via `StackSlot`
  ├ ID: RUSTSEC-2026-0221 … Solution: Upgrade to >=5.4.2
  └ event-listener v5.4.1 → event-listener-strategy → async-channel → madsim → (dev) wyrd-dst …
```

i.e. the **gating** `C4-ci` turns red. It is a pre-existing advisory on the base tree, not
something this patch introduced (this patch adds no dependency, and the chain is madsim's), and
the bump is the minimal repair the advisory itself prescribes — one patch-level version, no
manifest change. I kept it and left the finding deferred: dropping it would hand the human a
red gating gate; carrying it silently would be worse. If the human prefers it out, it is one
`git checkout Cargo.lock` and a separate PR.

### 4.4 Re-slicing to fit the line budget

See §5 — rejected as iterate-to-Plan churn that would discard four rounds of accepted review,
but the numbers are there for the human to overrule me.

## 5. Budget position — over on tests, on target on production

Counted from `patch.diff`, added lines only, excluding blanks, comment-only lines and the
declared mechanical migration (`read_object_from` → `read_object_chunks` callsites across
benches and pre-existing test files):

| | semantic added lines |
|---|---|
| **Production** (`metadata.rs` resolver 266, `read.rs` 27, `custodian/resolve.rs` 201 + `lib.rs` 1, `server/lib.rs` 29, docs 1) | **525** — under the brief's own estimate of "~660" (this count excludes doc comments, which are a large share of this tree's lines) |
| Tests (`segmented_map_resolution.rs` 826, `segmented_object_read.rs` 267, `dst/custodian.rs` 152, co-located in `metadata.rs` 128) | **1,373** |
| **Total (non-mechanical)** | **1,898** vs the brief's "≤ ~1,500" |
| Mechanical migration (10 files) | 207, allowed on top |
| Files | 20 total; **10** excluding the mechanical-migration files |

The overage is entirely test bodies, and the brief itself forecast it ("#647's full test bodies
push this to ~1,900, so prune the co-located resolver tests to the binding cases"). The
co-located resolver tests *were* pruned — the resolver's cases live in the mandated
discriminator file — and rounds 1–4 then **added** cases at reviewer insistence: the exact-ceiling
paged walk, both one-coordinate extent mismatches, the whitelist on the `get` channel. Pruning
now would re-open findings that are already closed, which is why I did not: an over-budget
patch is a Plan-level judgment, and I have put the numbers here rather than making it silently.
The brief's named fallback split (*read paths* out of *resolver*) remains available if the human
would rather take it; it would not shrink the test bodies, only distribute them.

## 6. Gate evidence (run here, before hand-off)

| Gate | Command | Result |
|---|---|---|
| `C4-ci` | `./engine/xtask.sh ci` | **pass** — `xtask ci: all checks passed`; includes `typos`, `lint_docs: OK`, `render_site: wrote 98 page(s)`, clippy, workspace tests, `cargo deny check` (advisories/bans/licenses/sources ok), conformance 5+6 vectors, statics, and `segmented_resolve_never_tears ... ok` under `--cfg madsim` |
| `C4-verify` | `PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh` | **pass** — GREEN 18/18 + 4/4 with the fix; RED 18/18 with production reverted |
| `--classify` | `run-verify.sh --classify` | exactly two `ADDED_TEST` discriminators, no cfg-gated addition |
| `C5-mutants` | `scripts/mutants-in-diff` | **pass** — `56 mutants tested in 2m: 14 caught, 42 unviable` (0 missed); first attempt aborted on a flaky baseline, §6.1 |
| formatter | `cargo fmt --all` | clean (re-run after every edit; `cargo xtask ci` re-checks it) |

### 6.1 A flaky baseline in the mutants sandbox — not a finding about this patch

The first `scripts/mutants-in-diff` run aborted before testing any mutant:
`ERROR cargo test failed in an unmutated tree`, from
`crates/server/tests/health_probe.rs:263` —
`Status { code: Unknown, message: "transport error", … ConnectionReset }` in
`the_default_empty_service_check_tracks_the_store`. That test **passes** in `cargo xtask ci`
on the same tree (`cargo test --workspace` run, `health_probe.rs` 13/13) and touches nothing
this patch changes (a gRPC health probe, no chunk map). It is a transport flake in the copied
tree under `/var/tmp`. The re-run, unchanged tree, went clean: `56 mutants tested in 2m: 14
caught, 42 unviable` — **0 missed**, the same result iteration 4 measured on the same
production code, which is unchanged this round.

## 7. The three refutation answers

**(a) Genuine red?** **Yes**, and measured twice. `run-verify.sh` reverts the production files
and keeps both test files: 18/18 core cases and 4/4 gateway cases fail on the base, including
this round's new one, whose red is the *discriminating* assertion rather than any failure —
`crates/core/tests/segmented_map_resolution.rs:55`: "must be refused by the NEW resolver's own
typed anomaly, not the base's blanket 'this build cannot yet resolve a segmented map'
refusal". Separately, each new case was run against two hand-made mutations of the shipped
production code (§2.2 table) and each mutation is caught.

**(b) Production path?** **Yes.** Both new cases drive the shipped code: the base-visible one
enters at `wyrd_core::read::read_object` and reaches the real
`metadata::resolve_live_chunk_map` → `resolve_current_chunk_map` → `read_group_range`; the
co-located one calls `resolve_current_chunk_map` itself. Nothing is re-implemented in the test:
the doubles are `MetadataStore` implementations that **wrap a real redb backend**
(`RedbMetadataStore::in_memory`) and only record accesses and commit a real `WriteBatch` at a
chosen instant. Chunk bytes come from real fragments written by `wyrd_core::write` to a real
`FsChunkStore`.

**(c) Fixture includes the fault?** **Yes.** The fault here is *the retirement itself*, and it
is injected, not curated away: the double publishes a genuine competing generation into the
same store mid-resolve, four times, and every generation it publishes is **complete and
resolvable** — the control read at the end of each case proves it by reading the object back
byte-identical from the last one. Nothing about the fixture is arranged to make the resolve
fail: remove the churn and the very same records resolve. The retired generations' `seg:`
records are left in place (a drain that has not caught up), which is the shape a real reader
meets.

## 8. Discipline

- **STOP**: no push, no branch, no PR — `patch.diff` and the two test files only.
- **Scratch**: everything under `$PDCA_SCRATCH` = `/var/tmp/pdca`
  (`pdca-builder-649-{metadata.rs.bak,Cargo.lock.fixed,verify.log,ci.log,mutants*.log}`),
  removed at hand-off; the worktree itself is the driver's.
- **Rejections**: `review-rejected.md` is unchanged and still accurate — the caller-side-deadline
  rows are anchored at `crates/core/src/metadata.rs` lines this round did not move (§2.2).
- No external dependency was missing: `typos` and the docs renderer (the brief's two registered
  `doctor.checks`) are both present here and ran inside `cargo xtask ci` (`lint_docs: OK`,
  `render_site: wrote 98 page(s)`), so the prose gates did **not** warn-and-skip.
