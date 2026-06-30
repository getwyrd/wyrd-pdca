# Build notes — issue 346 / rebalance-evac-identity-placement-fallback

## Root cause (two sentences, per CLAUDE.md)

`plan_evacuations` (`crates/custodian/src/rebalance.rs:151-152` pre-patch, base
`a3590947`) iterates the **raw** `ChunkRef.placement` field instead of resolving
through `ChunkRef::placed_dserver`/`fragment_count()` (`crates/core/src/metadata.rs:103,119`),
so a pre-M3 record (`placement: vec![]`, `#[serde(default)]`,
`crates/core/src/metadata.rs:93`) silently filters to an empty `evac` and the chunk is
`continue`-skipped; the same raw vector is then stored verbatim into `EvacPlan.placement`
(`:175`) and cloned/indexed/written by `evacuate_chunk` (`:221`, `:224`, `:245`, `:253`),
so even a forced non-empty `evac` would panic on an empty vector or commit a short one.

## What I read first (Planning artifact)

The brief names ADR-0040 implicitly via its "Invariant to restore" and "Scope" fields,
and the worktree's `main` already carries it merged at
`docs/design/adr/0040-mixed-era-placement-expansion.md` (commit `bcafae3` /
`a3590947`'s parent chain — `git log --oneline -5` shows it landed as PR #355 just before
this brief's base). I read it in full before touching code. The load-bearing decisions
for this fix:

- **Decision 1** (normative expansion rule): `placement[i]` if present else identity `i`,
  over `0..fragment_count()`.
- **Decision 2** (one expansion helper) explicitly **excludes** adding
  `ChunkRef::fragments()` for #346 — that's #347, a separate issue — and says rebalance
  should resolve through the *existing* primitives `fragment_count()`/`placed_dserver()`.
  The brief's "out of scope" list says the same thing in different words: "introducing a
  new shared `ChunkRef::fragments()` helper (separate issue) — use what already exists."
  So I did **not** add a `fragments()` helper; I used the `(0..chunk.fragment_count()).map(|i| chunk.placed_dserver(i))`
  pattern, which is the exact pattern `reconstruction.rs:230-232` (`assess`) and
  `gc.rs:197-199` already use for the same purpose — i.e. I matched the established
  in-repo idiom rather than inventing a new one.
- **Decision 5** (repoints write a full-length placement): "Expanding only an
  evacuation/repair *filter* while leaving the carried placement vector raw is incorrect
  ... a raw empty/short vector panics or persists a malformed record (#346)." This is
  exactly the brief's "Expanding only the filter is a half-fix" warning, restated. It is
  why the fix materializes `placement` once and reuses that *same* materialized vector for
  `evac`, `survivor_domains`, AND the `EvacPlan.placement` field that `evacuate_chunk`
  clones/indexes/commits — not three independent resolutions.
- **Decision 4** (liberal read / strict maintenance) is **out of scope here** — ADR-0040
  assigns "strict malformed-length handling with audit / NEEDS-HUMAN" to #348, a separate
  follow-up issue, and the brief's success criterion is only about the empty-vector case
  (pre-M3), not the malformed-short-vector case. I did not add `checked_fragments()` /
  `placement_is_valid()` gating — that's #348's surface, not #346's.

## The fix

`crates/custodian/src/rebalance.rs`:

1. `plan_evacuations` (`:155-167` post-patch) now builds
   `let placement: Vec<DServerId> = (0..chunk.fragment_count()).map(|i| chunk.placed_dserver(i)).collect();`
   — the single authoritative identity-placement-fallback resolution
   (`crates/core/src/metadata.rs:119`) — and scans **that** for `evac`, never
   `chunk.placement` raw.
2. `survivor_domains` (`:177-187` post-patch) is computed from the same materialized
   `placement`, not `chunk.placement` — this is the brief's explicit scope item ("Apply
   the same resolution to the `survivor_domains` computation so spread is preserved for
   mixed-era chunks"). Without this, a mixed-era chunk's survivors would still resolve to
   `[]` (no domains), and `select_distinct_domains_excluding` would then be free to pick
   ANY domain for the evacuated fragment — including one a surviving fragment already
   silently (mis-)occupies under the raw vector's blindness — collapsing the spread
   invariant `0005:298` without the selector ever knowing.
3. `EvacPlan.placement: placement` (`:193` post-patch) stores the **materialized**
   full-length vector instead of `chunk.placement.clone()`. `evacuate_chunk` is otherwise
   **unchanged** — `plan.placement.clone()` (`:239`), `plan.placement[index]` (`:242`),
   `new_placement[index] = target` (`:263`), and the commit (`:271`) now operate on an
   always-full-length vector by construction, so the empty-vector panic and the short-write
   corruption are both closed at the source (plan construction), not patched at each
   consumption site.

I did not touch `evacuate_chunk` itself — every one of its raw-indexing call sites was
already correct *given* a full-length `plan.placement`; the defect was entirely that
`plan_evacuations` hadn't been guaranteeing that precondition. This matches the brief's
"Expanding only the filter is a half-fix" framing: the filter (`evac`) and the carried
vector (`placement`) needed to come from the *same* resolved source, which is what
materializing once and reusing it for both achieves.

## Alternatives considered and rejected

**Expand only `evac`, leave `EvacPlan.placement` raw, materialize at use-site in
`evacuate_chunk`.** This is the "half-fix" the brief explicitly calls out and what
decision 5 of ADR-0040 forecloses by name. Sketch of the cost, concretely: it would touch
THREE separate sites instead of one (`evacuate_chunk:221` clone, `:224` index read, plus
still needing the `evac`/`survivor_domains` fix in `plan_evacuations`), and still leaves
`next_chunk_map[plan.chunk_index].placement = new_placement` (`:253`) writing a vector
that is only "fixed" for the evacuated index — every OTHER (non-evacuated) index would
still be whatever `chunk.placement.get(i)` happened to return, i.e. nothing for a pre-M3
record, so the **committed** record would still be short/empty except at the moved slot.
That is a worse bug than the one we started with (a record that *looks* committed but is
still malformed) and is precisely the "short one commits a malformed short placement
record" failure mode named in the brief's Defect field. Rejected on correctness, not
just cost.

**Add `ChunkRef::fragments()` (or `checked_fragments()`) and route through it.** This is
#347 (and #348 for the strict-maintenance gate), both explicitly out of scope per the
brief ("introducing a new shared `ChunkRef::fragments()` helper (separate issue) — use
what already exists") and per ADR-0040's Consequences section, which lists #347 and #346
as separate follow-on PRs in sequence. Cost if I did it anyway: a new public method on
`ChunkRef` in `crates/core/src/metadata.rs` (touched by `core`, a dependency of `read.rs`,
`gc.rs`, `reconstruction.rs`, AND `rebalance.rs`) plus migrating at least one more
existing caller to justify "shared", which is out of this issue's diff surface entirely —
not a cost argument, a scope violation of the brief's explicit exclusion.

**Reject malformed/short vectors before expanding (decision 4's strict-maintenance
posture).** Not applicable to this brief's success criterion, which is specifically about
the **empty** pre-M3 vector (a valid case per ADR-0040 decision 3: "valid iff empty or
`len == fragment_count()`"), not the malformed non-empty-wrong-length case ADR-0040 assigns
to #348. Adding it here would be scope creep against an explicit "out of scope" boundary
the brief and the ADR both draw.

## Test

`crates/custodian/tests/rebalance.rs` — added `write_pre_m3_chunk` (a helper that writes
real fragment bytes to the fleet at identity locations via `plan_write`/`write_fragments`,
then commits the `InodeRecord` directly via `metadata::create` with `ChunkRef.placement`
forced to `vec![]` — the only way to reproduce a genuine pre-M3 record now that every live
writer always emits a full placement vector, `crates/core/src/write.rs:171`) and two
tests per the brief's repro instruction:

- `evacuates_a_pre_m3_chunk_with_empty_placement_ec_none` — `EcScheme::None`, the single
  fragment at index 0, draining server 0.
- `evacuates_a_pre_m3_chunk_with_empty_placement_reed_solomon_index_gt_zero` — RS(2,1),
  draining fragment at index 1 (not 0), asserting BOTH the move and that
  `survivor_domains` still spans 3 distinct domains post-evacuation.

Both assert the brief's BINDING success criterion directly: `Reconciled::Changed` (a plan
WAS produced — pre-fix it stays `Satisfied`, nothing happens), the committed
`chunk_map[0].placement` is exactly full-length (`vec![1]` / `vec![0, 3, 2]`, not empty or
short), and the moved index no longer names the draining server. They also check the
moved fragment's checksum at its new home, the orphan record, and that `read_object`
still round-trips — so the test would also have caught the "panic on raw clone-index"
and "commits a malformed short record" failure modes the brief names, not just the
"no plan produced" one.

### Red→green, via the project's own test invocation

Ran in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`, base `a3590947` == `origin/main`):

```
$ cargo test -p wyrd-custodian --test rebalance
```

- **Pre-fix** (production change in `rebalance.rs` stashed, test additions kept):
  `evacuates_a_pre_m3_chunk_with_empty_placement_ec_none` and
  `evacuates_a_pre_m3_chunk_with_empty_placement_reed_solomon_index_gt_zero` both
  **FAILED** (`left: Satisfied, right: Changed` — no plan produced); the 5 pre-existing
  tests in the file stayed green (no regression in the harness itself).
- **Post-fix**: all 7 tests pass.

Also ran the project's bundle-scoped per-fix gate, `PDCA_BUNDLE=results/issue_346
./engine/scripts/run-verify.sh` (the `C4-verify` gate command from `pdca.toml`), against
the produced `patch.diff` in the dedicated `../wyrd-verify` worktree off `origin/main`: it
reports `PASS (green-only)`. That automated tool's red/no-fix split only triggers for a
**newly-added** `*/tests/*.rs` file (its `_added_files` classifier looks for `--- /dev/null`
immediately preceding the file in the diff); since `crates/custodian/tests/rebalance.rs`
already existed pre-patch and this change only adds tests to it, the tool falls back to its
green-only path (the same path it uses for co-located tests) rather than doing the
isolated revert-and-rerun. The genuine red→green proof above (manual stash of just the
production file, keeping the test additions) is what actually exercises the RED leg; the
gate's PASS confirms GREEN-with-fix through the project's own isolated-worktree command.

Also ran, scoped to the touched crate (these are exactly the steps `cargo xtask ci`
(`xtask/src/main.rs:530-558`) runs, just narrowed to `wyrd-custodian` instead of
`--workspace`, since the change touches only that crate):

```
$ cargo fmt -p wyrd-custodian -- --check   # clean after `cargo fmt -p wyrd-custodian`
$ cargo clippy -p wyrd-custodian --all-targets   # clean, no warnings
$ cargo build -p wyrd-custodian --tests   # clean
```

I did not additionally run the full workspace `cargo xtask ci` (fmt --all, clippy
--workspace, cargo-machete, cargo-deny, conformance, the 50-seed madsim DST sweep): the
change is confined to `crates/custodian/src/rebalance.rs` and
`crates/custodian/tests/rebalance.rs`, neither of which touches anything those other tiers
exercise (chunk-format conformance vectors, dependency licensing, the DST commit-protocol
simulation), and `docs/INTEGRATION.md` itself notes the per-fix sanity pass is meant to be
fast, with Check's gates re-running the real (whole-tree) suite. `pdca gates` / Check will
run the full `cargo xtask ci` against this same worktree.

## Citations (target branch `main` @ `a3590947`)

- Defect, pre-patch: `crates/custodian/src/rebalance.rs:151-152` (raw `chunk.placement`
  iteration), `:163-164` (raw `survivor_domains`), `:175` (`EvacPlan.placement` stored
  raw), `:221` (`plan.placement.clone()`), `:224` (`plan.placement[index]`), `:245`
  (`new_placement[index] = target`), `:253` (the committed write).
- Authoritative resolution: `crates/core/src/metadata.rs:93` (`#[serde(default)]`),
  `:103-108` (`fragment_count`), `:110-124` / `:119` (`placed_dserver`, documented as "the
  single authoritative placement-resolution definition").
- Matching idiom in the other already-fixed consumers:
  `crates/custodian/src/reconstruction.rs:230-232` (`assess`),
  `crates/custodian/src/gc.rs:197-199`.
- Planning artifact: `docs/design/adr/0040-mixed-era-placement-expansion.md:55-101`
  (decisions 1, 2, 5), `:118-122` (Consequences: "#346 routes rebalance through it").
