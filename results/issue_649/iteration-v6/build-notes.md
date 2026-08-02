# Build notes — issue 649 (iteration 6)

> Withheld from the reviewer; written for the human at sign-off.
> All `path:line` citations are against the patched worktree
> `/home/eddie/development/wyrd/wyrd.pdca-wt` at base `6e7c255` (= `origin/pdca-integration/main`,
> i.e. `origin/main` `9120f7a` + slice #648).

---

## 0. READ THIS FIRST — one gating gate is red for a reason that is not this patch

`cargo xtask ci` (the gating `C4-ci`) **fails at `cargo deny check`** on this bundle, and it
fails identically on the **pristine base with no patch at all**:

```
$ git worktree add --detach $SCRATCH 6e7c255 && cd $SCRATCH && cargo deny check advisories
error[unsound]: `event-listener` allows `!Send` tags to cross thread boundaries via `StackSlot`
   ┌─ Cargo.lock:91:1
   ├ ID: RUSTSEC-2026-0221   ├ Solution: Upgrade to >=5.4.2
advisories FAILED
```

`event-listener 5.4.1` comes in via `madsim` (and `madsim-tonic` → `wyrd-chunkstore-grpc`); the
advisory was published after the base was cut, and `deny.toml:60` keeps a zero-tolerance
advisory wall by explicit policy ("the `ignore` list below carries no exception … and must not
gain one"). This patch touches no manifest and no lockfile.

**NEEDS-HUMAN external dependency: RUSTSEC-2026-0221 (event-listener 5.4.1 in the base lockfile) — blocks the gating `C4-ci` (`cargo deny check`) on the BASE tree, independent of this patch; because `cargo_deny_check()` runs before `run_conformance()` / `run_statics()` / `run_dst()` (`xtask/src/main.rs:1562-1567`), a red deny also prevents the gate from reaching the DST tier this slice's verification posture relies on.**

Proposed registration so a future cycle does not burn a round on it:

```toml
[[doctor.checks]]
id    = "deny-clean-base"   # the token Plan should have put in `External dependencies`
cmd   = "cd ${PDCA_WORKTREE:-../wyrd} && cargo deny check advisories"
hint  = "The base tree carries an unresolved RUSTSEC advisory. Land the dependency bump on its own (dependabot, or `cargo update -p event-listener --precise 5.4.2`) before running a cycle — a feature slice cannot make `cargo xtask ci` green while the base lock is red."
level = "MISSING"
```

**Why I did not fix it in this patch.** Iteration 5 *did* carry the one-line lockfile bump, and
the reviewer's only outright FAIL was exactly that: *"T2 Shape — the resolver-only slice also
refreshes `event-listener` without any manifest change at `Cargo.lock:1203`; the base red targets
compiled under the prior lock, so this is unrelated dependency churn and expands the review and
supply-chain surface."* The target's own protocol agrees (`AGENTS.md`, Reviewer protocol §Out of
scope: *"a real finding outside the PR's stated scope gets a decline-with-issue-reference, not an
in-PR fix"*), and this repo lands lock bumps through dependabot (`b0cd199`, `7042ec9`, `cb3432c`
are three such merges on the base). Re-submitting the rejected change unchanged is what the
iterate contract forbids, so the patch is now scoped strictly to the slice and the base defect is
declared instead of absorbed.

**One line makes C4-ci green if you want it green before sign-off** (run in `$PDCA_WORKTREE`,
outside this patch):

```
cargo update -p event-listener --precise 5.4.2
```

**What I ran instead, so nothing behind `deny` went unmeasured** — every later `ci` step,
individually:

| step (order in `xtask/src/main.rs:1546-1568`) | how | result |
|---|---|---|
| typos, docs, gitlink, unsafe-forbid, fmt, clippy, build, **test (workspace)**, machete | `./engine/xtask.sh ci` reached `cargo deny check`, and `ci` stops at the first failure | **all passed** |
| `cargo deny check` | same run | **FAILED** — base advisory above |
| `run_conformance()` | `./engine/xtask.sh conformance` | pass — `5 valid + 6 invalid vectors pass` |
| `run_statics()` | `./engine/xtask.sh statics` | pass — `no DST-reachable shared mutable global state` |
| `run_dst()` (50 seeds, `--cfg madsim`) | `./engine/xtask.sh dst` | **pass, exit 0**, incl. `test segmented_resolve_never_tears ... ok` in the 11-test custodian campaign |

So the brief's verification posture for the DST resolver-tear property ("built and exercised in
this cycle by the gating C4-ci … not deferred") **is** satisfied in substance — the property was
built and swept over 50 seeds in this cycle — but it was reached by invoking `xtask dst`
directly, because `deny` aborts `ci` before the DST tier. That is the second cost of the base
advisory and is why it is worth clearing before publish.

---

## 1. What this iteration changed, and why (the carry-forward)

Round 5 left three open reviewer items. Two were implementation-level and are fixed here; one is
not mine to fix.

| Round-5 finding | Disposition in this patch |
|---|---|
| **T2 Shape — FAIL**: unrelated `event-listener` lockfile churn (`Cargo.lock:1203`) | **Fixed by removal.** `Cargo.lock` is no longer in the patch (19 files, none of them a manifest or lockfile). The base defect is declared in §0 instead. |
| **C3 Change — NEEDS-HUMAN**: "≈1,925 nonblank, noncomment additions … materially above the brief's ~1,500-line ceiling, led by `crates/core/tests/segmented_map_resolution.rs:1` and `crates/core/src/metadata.rs:2040`" | **Reduced by 17 %** to 1,613 (§4 has the full arithmetic and the per-cut justification). Every assertion survived: the C5 mutants gate is still `0 missed`. |
| **C1 Spec — NEEDS-HUMAN (deferred)**: the living-architecture paragraph reads as if *every* consumer already routes through the call, while `resolve.rs` defers its maintenance callers to #650/#651 | **Fixed in both places.** The doc sentence now separates the contract from adoption state (`docs/design/architecture/06-runtime-view.md:29`: "…so how an object's chunks are learned is written once… The read paths — whole-object, streaming and ranged — resolve through that call…; a consumer that has **not yet** adopted it refuses a segmented map outright"), and the wrapper carries the repo's sanctioned deferral marker (`crates/custodian/src/resolve.rs:21`: `deferred: #650/#651 …`), which `AGENTS.md` Reviewer protocol treats as settled for review purposes. |
| **T4 Contribution — NEEDS-HUMAN** (`scripts/review-branch` / `scripts/pdca` absent from the reviewer's sandbox, so the wrapper rows could not be reproduced) | Not addressable from the builder leaf: those wrappers live in the PDCA project, not the target. No change. |
| **Validation — NEEDS-HUMAN** (operational fitness) | The human's at sign-off. No change. |

Nothing else in the patch's *behaviour* moved from round 5: the resolver, the read plumbing and
the custodian wrapper are unchanged code, re-verified red→green here.

---

## 2. The change itself — why this shape

The defect the brief names is that after #648 a `chunk_map` can be `Segmented` but **nothing can
read one**, and that each consumer is about to re-derive "which chunks does this object own"
(0016 decision 7(e) forbids exactly that). The invariant to restore is C-1: *no permanent or
data-losing failure mode is an acceptable cost*, over the category "how any process learns which
durable bytes an object owns".

**One resolver, in `crates/core/src/metadata.rs`, and every read consumer routed through it.**

- `resolve_chunk_map` (`crates/core/src/metadata.rs:2328`) — the single call that turns a
  committed root into an ordered chunk list. Flat map: borrow, zero reads. Segmented: exactly one
  bounded range read of `seg:<nonce>:<epoch>:`.
- `read_group_range` (`:2180`) — the bounded read. The ceiling is checked **before** the first
  page (`:2188`), and the range is walked with `scan_page` (`:2205`) because `scan` is
  complete-or-fail-loud at `SCAN_CAP` (`crates/traits/src/lib.rs:275-324`, the peer callsite the
  brief permitted): an unpaged scan of a half-drained range would cost the caller the whole
  namespace or the whole call, which per-object containment cannot catch. Rows are keyed by their
  **parsed** index into a `BTreeMap` (`:2131`, `:2237`), never by page-arrival order, and the
  group is re-pinned from every key (`:2216`) so another epoch's row can never be spliced in.
- `retired_or` (`:2113`) — **one** arbiter for "retired, or corrupt?". Every anomaly
  `read_group_range`/`read_segments` can meet is *described* (`GroupRange::Anomaly`, `:2131`) and
  settled here by re-reading the root (`root_still_names`, `:2085`), rather than judged where it
  was noticed. That is the whole reason the over-ceiling refusal is an `Anomaly` and not an early
  `Err`: an ordinary overwrite would otherwise become a permanent read failure on a healthy
  object (C-1).
- `resolve_current_chunk_map` (`:2371`) / `resolve_live_chunk_map` (`:2410`) — the restart
  entries, bounded by `MAX_RESOLVE_RESTARTS` (`:2351`); past the budget the answer is the typed
  `MapResolutionUnstable` (`:2389`), never `Ok(None)` — "this object owns no bytes" is the
  data-losing answer decision 7(h) forbids.
- Consumers: `read::read_object` (`crates/core/src/read.rs:513`), the gateway's streaming
  (`crates/server/src/lib.rs:355`) and ranged (`:446`) entries, and the custodian's wrapper
  (`crates/custodian/src/resolve.rs:45`). Both gateway entries frame the response from the
  generation the bytes came from (`served`, `crates/server/src/lib.rs:363`,`:451`).
- `read::read_object_from` became `read_object_chunks(chunks, &map, size)`
  (`crates/core/src/read.rs:69`): a snapshot-only entry *cannot* resolve a segmented map, so
  keeping it would have kept one `.chunk_map` consumer that understands one representation and is
  opaque to the other. Removing the entry from that population is the fix; guarding it is not.
  This is what makes the 205-line mechanical migration necessary (§4).

---

## 3. Alternatives ruled out, with their costs

- **Keep a `read_object_from(&record)` convenience wrapper** so the 10 legacy call sites stay
  one-liners. Cost of *not* doing it: 205 added semantic lines of mechanical migration across 10
  files (measured, §4). Rejected anyway: that wrapper is precisely the "consumer opaque to the
  segmented shape" 0016 decision 7(e) forbids, and it would have to fail closed on every
  segmented object forever — a permanent read failure by construction. The brief pre-authorises
  the migration ("counted separately and allowed on top; declare it as that pattern").
- **Clamp an over-ceiling table to `MAX_ROOT_SEGMENTS` and read the first 512 segments.** One
  line cheaper than refusing. Rejected: that is the quiet under-approximation C-1 names — a
  reader would answer with a *prefix* of the object's bytes and a maintenance pass would protect
  a prefix of its fragments.
- **Judge each anomaly where it is noticed** (raise `SegmentAbsent` / `SegmentKeyMalformed` /
  `TooManySegments` at the check that found it) instead of routing all of them through
  `retired_or`. Saves ~18 lines (`GroupRange`'s enum + the describe/settle round trip). Rejected:
  a half-drained retired generation shows *exactly* these shapes, so this converts an ordinary
  overwrite into a hard read failure on a live, healthy object. Two tests bind it —
  `segmented_map_resolution.rs:877` (`OverwrittenAndDrained`,
  `OverwrittenWithAMalformedRow`) and `:680` (the over-ceiling retirement arm).
- **Resolve against the caller's snapshot only** (no root re-read after a complete range read).
  Saves one `get` per segmented resolve. Rejected: the root moves first (`0016:2452-2462`), so a
  complete-looking read can still be a retired generation; `read_segments` re-checks at `:2308`.
  Cost is pinned at exactly 2 root reads per clean resolve
  (`segmented_map_resolution.rs:353`,`:711` — asserted on a 2-segment *and* a 512-segment object,
  so it cannot be a function of the table).
- **Bound the resolver's store awaits with a caller-side timeout.** Rejected on standing
  precedent — recorded in `review-rejected.md` (i, *caller-side fan-out timeout*, rejected 3×
  across #508/#636): the `MetadataStore` implementation owns the network bound
  (`crates/traits/src/lib.rs:1000-1012`); `wyrd-core` holds no runtime to spend a deadline from.
  What the resolver bounds is the **work** — one root read plus one paged range, refused above
  the ceiling — which is documented at the section header (`crates/core/src/metadata.rs:2050`).

---

## 4. The budget: 1,613 non-mechanical semantic lines (was 1,936)

Measured on `patch.diff` with the brief's own rule (added lines, non-blank, non-comment,
non-mechanical). Reproduce with the same counter the reviewer used, or:
`rg '^\+' patch.diff | rg -v '^\+\+\+' | rg -v '^\+\s*(//|/\*|\*|#!\[)?\s*$'` per file.

| file | semantic + | class |
|---|---|---|
| `crates/core/tests/segmented_map_resolution.rs` | 594 | test (new) |
| `crates/core/src/metadata.rs` | 348 | 269 production + 79 co-located test |
| `crates/server/tests/segmented_object_read.rs` | 265 | test (new) |
| `crates/custodian/src/resolve.rs` | 202 | 50 production + 152 unit test |
| `crates/dst/tests/custodian.rs` | 146 | DST property |
| `crates/server/src/lib.rs` | 29 | production |
| `crates/core/src/read.rs` | 27 | production |
| `crates/custodian/src/lib.rs` + `06-runtime-view.md` | 2 | production + docs |
| **non-mechanical total** | **1,613** | **9 files** (budget ≤ 15) |
| 10 × declared mechanical migration (`read_object_from` → `read_object_chunks`) | 205 | counted separately per the brief |

Production side is **456** semantic lines against the brief's own ~660 model
(metadata 320 + read/resolve/custodian 278 + server 60); the residue is test bodies, 1,157.

**What was cut this round (−323), and why nothing binding was lost:**

1. `crates/core/tests/segmented_map_resolution.rs` 845 → 594. Six "root unchanged ⇒ fail closed"
   cases became one labelled table over one fixture (`:755` `FAIL_CLOSED`, driven at `:807`) —
   same six bends, same six assertions; four "root moved on" cases became one enum-labelled table
   (`:858` `Retired`, driven at `:877`) — same four interleavings, same four expectations. The
   rest is formatting: shared `sref`/`committed_root`/`rows_of`/`seed_row` helpers instead of
   rustfmt-exploded struct literals, and `let`-bound results with one-line assertions.
2. `crates/core/src/metadata.rs` co-located tests 130 → 79. The redb-backed `Churn` store double
   was replaced by `EverMoving` (`metadata.rs:3277`), a 40-line double whose root names a new
   generation on every read. It binds strictly more of what a *unit* test can bind — the exact
   typed variant `MapResolutionUnstable { attempts: MAX_RESOLVE_RESTARTS }` and exactly
   `2 × MAX_RESOLVE_RESTARTS` root reads — while the end-to-end version of the same property
   (real records, real churn, and the control that the store resolves once the churn stops) stays
   in the integration file at `segmented_map_resolution.rs:934`.
3. `crates/server/tests/segmented_object_read.rs` 272 → 265: the two criterion-(1) entries share
   one fixture (`:167`), and the two restart tests share `superseded_gateway` (`:290`).
4. `crates/custodian/src/resolve.rs` tests 159 → 152 and `crates/dst/tests/custodian.rs` 153 →
   146: helper extraction only.

**It is still ~7 % over `~1,500`, and I chose to stop there.** The remaining candidates were all
*assertion*-bearing, not formatting: dropping the over-ceiling retirement test (−22), the
foreign-epoch splice test (−22), or the malformed-key-under-a-retired-root arm (−20) would each
have removed the only case binding a distinct production branch, and dropping the gateway restart
pair (−121) would have left `served` (`crates/server/src/lib.rs:363`,`:451`) untested. Trading
the strongest evidence for the last 7 % of a "~" budget is the wrong trade; the honest report is
this table. If the human disagrees, the cheapest ~120-line cut is the gateway restart pair, and
the next is the two ceiling-retirement cases — in that order.

---

## 5. Refutation — the three questions, answered with evidence

**(a) Genuine red?** Yes — measured, not asserted. `./engine/scripts/run-verify.sh` with
`PDCA_VERIFY_BASE=origin/pdca-integration/main` reverts `metadata.rs` / `read.rs` /
`server/src/lib.rs` and removes `custodian/src/resolve.rs`, keeping both test files:

```
run-verify.sh: GREEN — (fix applied)      core 10 passed 0 failed · gateway 3 passed 0 failed
run-verify.sh: RED   — (production reverted, test kept)   core 0 passed 10 failed
run-verify.sh: PASS — red without the fix, green with it.
```

Both files go red, and both **compile** on the reverted tree (they import only #648-visible
symbols). The gateway leg is masked in the gate's log because `cargo test` aborts at the first
failing target, so I ran it explicitly in the gate's own reverted worktree:

```
$ cd ../wyrd-verify && cargo test -p wyrd-server --test segmented_object_read
test result: FAILED. 0 passed; 3 failed
… panicked at crates/server/tests/segmented_object_read.rs:179:
  Err(SegmentedMapUnsupported { operation: "Gateway::get_object_streaming" })
```

The red is an **assertion/panic** red, never a compile red — and `assert_fails_closed`
(`segmented_map_resolution.rs:57`) explicitly rejects the base's blanket
`SegmentedMapUnsupported`, so "the read must fail" cannot pass pre-fix for the fail-closed cases
either.

**(b) Production path?** Yes. Every test drives base-visible production entries —
`wyrd_core::read::{read_object, read_path}` and `wyrd_gateway_core::ObjectGateway`'s trait methods
on a real `Gateway` — over a **real** `RedbMetadataStore` and a **real** `FsChunkStore` with
fragments written by `wyrd_core::write`. The only test-owned code is the *store double* (`Probe`,
`segmented_map_resolution.rs:381`), which wraps the real redb store and records/perturbs
accesses; the resolver it observes is always the production one. Nothing this patch adds is
imported by either test file (checked: no `resolve_chunk_map`, `MapResolution`,
`resolve_live_chunk_map`, or new `ChunkMapError` variant appears in them), so the tests could not
be passing against a stand-in even by accident. Cross-check: `cargo mutants --in-diff` over this
bundle's diff — **56 mutants, 14 caught, 42 unviable, 0 missed** — a mutation of the production
resolver turns these tests red.

**(c) Fixture includes the fault?** Yes, and the fixture is the fault in each case:
- bounded-access (`:584`) seeds two **decoy** groups that are actually present in the store — a
  different nonce and *the same nonce at a different epoch* — and asserts the whole recorded
  footprint (gets, unpaged scans, paged prefixes) never intersects them; the `get` channel is
  whitelisted **positively** to the object's own root (`:437`), so an unrelated metadata read
  fails it too, not just a `seg:` read;
- the ceiling cases seed a root that really names `MAX_ROOT_SEGMENTS + 1` segments (`:648`) and
  assert *zero* pages were read; the boundary case seeds a real 512-segment object read over a
  three-page walk with the cursor asserted (`:711`);
- every fail-closed case seeds the anomaly itself (absent record, garbage bytes, an unnamed row,
  a one-coordinate offset mismatch, a one-coordinate length mismatch, an unparsable key) rather
  than curating it out (`:755`);
- the retirement cases apply a **real** concurrent `WriteBatch` mid-resolve — including one that
  reclaims a named segment of the retired generation in the same batch (`:858` `Retired::
  OverwrittenAndDrained`) — and the churn case republishes a whole new generation on *every*
  restart, then proves with a control read that the same store resolves once the churn stops
  (`:934`);
- the DST property (`crates/dst/tests/custodian.rs:1457`) injects its nemesis per seed
  (`reclaimed: bool = rng.random()`), so on half the 50 seeds the old generation's map is
  genuinely incompletable — the reader has no old-generation answer left to succeed with by
  accident.

---

## 6. Evidence log (this iteration)

| check | command | result |
|---|---|---|
| core integration | `cargo test -p wyrd-core --test segmented_map_resolution` | 10 passed |
| gateway integration | `cargo test -p wyrd-server --test segmented_object_read` | 3 passed |
| custodian wrapper units | `cargo test -p wyrd-custodian --lib resolve` | 5 passed |
| resolver units | `cargo test -p wyrd-core --lib segmented` | 17 passed |
| per-fix red→green | `./engine/scripts/run-verify.sh` | **PASS** (§5a) |
| mutants on the diff | `./scripts/mutants-in-diff` | 56 tested — 14 caught, 42 unviable, **0 missed** |
| whole gate | `./engine/xtask.sh ci` | **FAIL at `cargo deny check`** — base advisory, §0; every step before it passed |
| conformance / statics / DST | `./engine/xtask.sh {conformance,statics,dst}` | pass / pass / pass (50 seeds, incl. `segmented_resolve_never_tears`) |
| formatter (commit-hook readiness) | `cargo fmt --all -- --check` | clean. The target configures no `pre-commit`/`husky`/`core.hooksPath`; its formatter and linters are `cargo xtask ci`'s `fmt --check` + `clippy -D warnings`, both green. |

Scratch: everything this leaf created under `$PDCA_SCRATCH` — the `pdca-builder-649-{dst,verify}`
logs and the throwaway `pdca-builder-649-{denybase,applytest}` worktrees — was removed
(`git worktree prune` run); the outputs above are transcribed here because the logs are gone.
The patch was also re-applied to a pristine `6e7c255` checkout (`git apply --check`) to confirm it
is commit-ready against the brief's base.

## 7. STOP discipline

No branch pushed, no PR opened, nothing marked ready. `patch.diff`, the two test files and these
notes are the whole output.
