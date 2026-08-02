# Build notes — issue 636 / multipart-commit-protocol

Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `origin/pdca-integration/main` (wave-2 stack base, worktree HEAD
`d15d555 pdca-integrate: issue_635`). Every `path:line` below is against that tree.

---

## 1. What was built, and why in this shape

`crates/core/src/multipart.rs` (new, 4,341 lines incl. docs + unit tests) is the protocol:
the record family, the key helpers and their parsers, the seven verbs (create · slot reserve ·
stage · part commit · fence+publish · abort · bounded teardown drain), the settled knob values,
the ETag composition, and the classification sweep. `crates/core/src/write.rs:735-899` adds the
staged-write intent. `crates/core/src/metadata.rs` adds `StagedPlacement`, `PendingEntry`'s two
additive optional fields with their constructors, and the `desired:dserver:` key helper.

Three structural decisions worth the human's attention:

**(a) `MAX_SESSIONS` is derived, not a constant.** `Budget` (`multipart.rs:246-333`) carries the
profile 0016 stores in `mpuctl` and computes `u_ref()` / `max_sessions()` from it. There is
deliberately **no `MAX_SESSIONS` constant in the module** — 0016 says a hard-coded one is a
defect, not a value choice (`0016:2118-2124`). This also gave the tests an honest lever: leg E
reaches the cap by shrinking `W_ref` (`multipart_protocol.rs:budget_for`), never by overriding
the limit.

**(b) `knob_clamps_hold()` is a function, not a comment.** `multipart.rs:335-400` checks every
bounding invariant of 0016's knob table (the `V/2` value rule, `MAX_PART_CHUNKS ≤ B_ops` (X110),
the four `MAX_INFLIGHT_PARTS` clamps (X77/X98), `MAX_STAGED_CHUNKS`'s range, `max_sessions > 0`)
and the module's own unit test asserts it for the deployed set. #508 inherits the values; a
deployment that retunes one can check the set is still legal.

**(c) The backoff is a *schedule*, not a sleep.** `core` reads no clock and has no runtime, so
`admission_backoff_millis(attempt, jitter)` is a pure, capped, fully-jittered schedule and
`create_session` takes a `backoff(attempt, millis)` hook the caller awaits. Tests pass a no-op
(the *budget* is what the shipped defect was about); #508 passes a timer sleep.

## 2. The value set, recorded explicitly so it is reviewable as a set

| Knob | Value | Where it comes from |
|---|---|---|
| `MAX_CHUNKREF_BYTES` (`b_ref`) | 302 | 0016's worst case (`0016:1050-1053`), not the typical one |
| `MAX_MAP_CHUNKS` = `MAX_SEG_CHUNKS` = `MAX_PART_CHUNKS` | **165** | `⌊(V/2)/b_ref⌋`; one rule, three records, each being one value (`0016:1044`). In range [165, 381]; `≤ B_ops` (1000) ✓ |
| `MAX_PARTS_PER_SESSION` | 10,000 | S3's own limit |
| `MAX_INFLIGHT_PARTS` | 16 | `aws s3 cp`'s default concurrency; all four clamps hold (asserted) |
| `MAX_STAGED_CHUNKS` | 84,480 | `MAX_ROOT_SEGMENTS(512) × MAX_SEG_CHUNKS(165)` — the publishable ceiling (`D-J`) |
| `W_ref` | 4,000,000 chunk-refs | **#625's by 0016's assignment**; defined here because `max_sessions` cannot be derived without it. 0016's own worked example (`0016:2128-2130`). Doc comment states #625 consumes it and must not re-derive it |
| `B_ops` | 1,000 (`metadata::MAX_BATCH_OPS`, #635's) | **#625's by assignment**; already present in the tree from the wave below, consumed not re-invented |
| `U_ref` (derived) | **89,760** | `min(1,652,640 raw, 84,480 + 2·16·165 = 89,760)` — the ceiling term binds, which is what buys back the order of magnitude the raw term would have cost |
| `MAX_SESSIONS` (derived) | **44** | `min(⌊4,000,000/89,760⌋, SCAN_CAP/2)` |
| `R_publish` | 3 | Safe with **no** reaper: on exhaustion Complete releases its own fence, so a client retries at once rather than waiting out `W_completing` |
| `MAX_COMPLETE_ATTEMPTS` | 3 | At the cap the session's only exit is Abort, which is reachable with no reaper |
| `MAX_UPLOAD_ID_ATTEMPTS` / `MAX_ADMISSION_CAS_ATTEMPTS` | 2 / 64 | The **separation** is the leg-F fix; 64 covers ordinary fleet concurrency |

## 3. Demonstrated red — the three mandatory negation runs (**no gate consumes these**)

The brief is explicit that this is **sign-off evidence for the human**, not a mechanical check:
these runs are recorded here, `build-notes.md` is withheld from the reviewer, and no
`[[gates.checks]]` row reads it. Please read the recorded output rather than treating the
C4-verify PASS as proof.

### Leg F — collapse the two retry budgets back into one
Negation: `MAX_ADMISSION_CAS_ATTEMPTS: u32 = 64` → `4` (the shipped single shared budget).
```
running 1 test
test f_concurrent_creates_on_an_empty_store_all_succeed ... FAILED
thread '...' panicked at crates/core/tests/multipart_admission_and_drain.rs:321:52:
8 concurrent creates against an EMPTY store: one was refused with
SlowDown { pressure: AdmissionContention { attempts: 4 } }. The admission ledger is nowhere
near its bound — this is the false `503 SlowDown` a single retry budget shared between the
2^-128 upload-id collision and the globally serialized admission CAS produced.
4 of 8 succeeded.
test result: FAILED. 0 passed; 1 failed
```
**4 of 8** — the shipped defect reproduced almost exactly (it measured 2 refused of 8; my
harness's yield schedule is harsher). Reverted.

### Leg G — two negations, because the leg has two halves
0016's leg G names two distinct shipped defects, and they need different negations.

**G1 — truncate derivation while declaring the obligation fully drained** (the silent
permanent-part-loss defect): in the deleting phase, `finished = true` on a full batch instead of
persisting the cursor.
```
running 1 test
test g_a_4001_part_complete_drains_to_empty_in_byte_budgeted_batches ... FAILED
thread '...' panicked at crates/core/tests/multipart_admission_and_drain.rs:643:9:
8,002 record deletes under a 200-operation budget must take more than 40 batches, saw 1
test result: FAILED. 0 passed; 1 failed
```

**G2 — drop the drain cursor** (the non-convergence defect): write `cursor: None` instead of
`Some(next_cursor)`.
```
running 1 test
test g_a_partially_drained_teardown_converges_with_no_double_decrement ... FAILED
thread '...' panicked at crates/core/tests/multipart_admission_and_drain.rs:733:14:
called `Result::unwrap()` on an `Err` value:
DrainStalled { obligation: "retire:bytes:s:00000000000000000000000000000002:1" }
test result: FAILED. 0 passed; 1 failed
```

**Something the first attempt at G2 taught me, and it changed the patch.** My first
cursor-dropping negation ran against the 4,001-part *records* obligation and the test **still
passed**: the deleting phase is self-cursoring by *absence* (a unit whose records are gone is
stepped over), so it converges even with no cursor. That is a robustness property, but it meant
the negation proved nothing there — and worse, on the **bytes** path (marking → deleting) a
dropped cursor produced an **infinite loop**, i.e. a hang, not a red. A hang is the worst
failure shape: invisible. So I added `DrainProgress`/`drain_step` (already needed for leg B's
ordering observation) plus a **stall detector** in `drain_obligation`
(`multipart.rs:3661-3712`): a step that commits but changes neither the obligation's bytes nor
any evidence/record it owes returns `DrainStalled`. It is work-independent (no step budget to
tune), it is a genuine production improvement — a stuck drain is now loud and actionable by
#625's drain-health alarm rather than a silent spin — and it is what turns G2 into the clean
red above.

### Leg E — the exact-count assertion, both directions
**E1 — skip the abort path's terminal-delete decrement**: `count: admission.count`.
```
thread '...' panicked at crates/core/tests/multipart_protocol.rs:1241:9:
assertion `left == right` failed: the decrement happens in the terminal delete
  left: 2   right: 1
```
**E2 — apply it twice**: `count: admission.count.saturating_sub(2)`.
```
thread '...' panicked at crates/core/tests/multipart_protocol.rs:1241:9:
assertion `left == right` failed: the decrement happens in the terminal delete
  left: 0   right: 1
```
Both reverted; the module is byte-identical to its pre-negation copy (verified by `diff`).

### The C4-verify RED leg, measured honestly
The brief predicted the red would be **criterion-absence**, and it is. I simulated the RED leg
(production reverted, the two added test files kept) and measured:

```
tests that actually ran:  0
tests that actually failed: 0
```
The failures were **build errors**, not assertions:
`error[E0432]: unresolved import ... wyrd_core::multipart`,
`error[E0609]: no field 'owner' on type 'PendingEntry'`,
`error[E0609]: no field 'staged' on type 'PendingEntry'`,
`error: could not compile 'wyrd-core' (test "multipart_protocol") due to 4 previous errors`,
`error: could not compile 'wyrd-core' (test "multipart_admission_and_drain") due to 2 previous errors`.

**Stated plainly: the C4-verify red is a build failure, not a flipped assertion.** A net-new
module has no prior assertion to flip, and `run-verify.sh` scores a build failure as a red
without counting tests (`engine/scripts/run-verify.sh:416-427`'s `TESTS_RAN == 0` guard sits
inside the cargo-*succeeded* branch). So C4-verify's PASS here means "the added tests compile and
pass with the patch, and do not compile without it" — the load-bearing evidence is §3's negation
runs, which is why they are mandatory.

### The DST cases actually ran, and how many seeds
`concurrency.rs` already carries `#![cfg(madsim)]` (kept). Under `cargo xtask ci` → `run_dst`,
which sets `--cfg madsim` and `MADSIM_TEST_NUM=50`, the block reports **11 tests** where the base
had 5 — the 6 added cases ran, not compiled-to-nothing (from the gate log,
`/var/tmp/pdca/pdca-builder-636-ci2.log`):
```
running 11 tests
test multipart_publication_recomputes_its_version_from_the_reread_prior ... ok
test concurrent_slot_reserves_never_exceed_the_cap_and_never_starve ... ok
test concurrent_slot_reserves_never_exceed_the_cap_on_sim_tikv ... ok
test two_concurrent_drainers_over_one_owned_range_are_exactly_once ... ok
test multipart_publication_race_holds_on_sim_tikv ... ok
test a_one_part_publication_stays_flat ... ok
test result: ok. 11 passed; 0 failed
```
Seeds swept: each `#[madsim::test]` is multiplied across **`MADSIM_TEST_NUM=50`** seeds
(`xtask/src/main.rs:1573`,`:1607`) — that covers cases (ii) and (iii) and the two sim-TiKV
mirrors. Case (i) is a `#[test]` that sweeps **48 seeds explicitly**
(`for seed in 0..48u64`) because it must *count reachability* across seeds: it asserts that at
least one seed put the multipart flip **after** the interloper (else the recomputed-version
assertion never runs and the coverage is vacuous) and at least one put it before.

**A DST case caught a real defect in my own test, which is worth recording.**
`two_concurrent_drainers_...` first asserted the *value* of every orphan stamp equalled the first
drainer's instant. It failed at seed `1785393362862265570`: with two drainers interleaving, a
mark carrying the **later** drainer's instant can be that position's **first** mark. The value
assertion was therefore wrong on a correct execution. The property X56 actually names is *no
re-stamp*, so I replaced it with the honest observable — a `MarkCountingStore` wrapper that
counts **committed** puts per key, asserting **exactly one per `orphan:` position**.

## 4. Before declaring done — the three forced questions

**(a) Genuine red? YES.** Recorded above, five negation runs across legs F, G (×2) and E (×2),
each reverted and re-verified green. Plus the C4-verify-shaped red, measured and honestly
labelled a build error with 0 tests run.

**(b) Production path? YES.** The tests drive `wyrd_core::multipart`'s real verbs, the real
`MetadataStore` seam (the in-memory double is the store, not the protocol — the same
`crates/custodian/tests/gc.rs:26-120` shape the repo already uses), the real
`PlacementChunkStore`, `write::stage_intent`, and `metadata`'s real committers — including
#635's `SegmentedPublication` for the 4,001-part publish. Nothing is stubbed except the *caller*,
which is the test instead of the S3 handler (#508's). The DST cases run over the **two**
production backends the campaign pins the trait with: real redb and the simulated-TiKV model.
Read-back is through `read::read_path`, the production read path.

**(c) Fixture includes the fault? YES.** Deliberately, in five places: leg B keeps part 2
staged and asserts its bytes are evidenced (nothing curated out); leg D observes the `sidx:`
range **while the part is in flight** rather than after; leg H stages a part that *crashes*
mid-stream (owned residue, no `part:` record, slot still held) and asserts the teardown reclaims
it; leg F's store **yields** so the read-then-CAS contention is genuinely produced (asserted:
the store saw strictly more commits than creators, else the leg would be sequential wearing a
concurrency label); leg G stages **4,001 real parts through the production verb** rather than
hand-writing a `retire:` payload.

## 5. What I deliberately did NOT do, and why (please weigh these at sign-off)

**(i) Existing `unlink` / `commit_chunk_map_superseding{,_leased}` still expand orphans
inline.** 0016 decision 4.1 routes them through `retire:bytes:{generation}`, and the brief's
Impact section names it. I implemented the obligation and its drain, and Complete's own
publication **does** install `retire:bytes:{generation}` for the generation it supersedes — but I
did not convert the pre-existing single-PUT/DELETE paths. Three reasons, the third decisive:

* no success-criterion leg asserts it (legs A–J are the multipart family);
* the cost is concrete, not an adjective: those three committers have **11 in-repo call sites**
  and the inline-orphan behaviour is asserted by name in `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/gc.rs`, `crates/custodian/tests/restore_reconcile.rs`,
  `crates/server/tests/custodian_gc.rs` and `crates/dst/tests/custodian.rs` — converting it means
  rewriting those oracles in the same diff as a new protocol, which is exactly the
  reviewability failure that got the seventh attempt rejected;
* **it would not be safe yet.** X92/X97 make an installed-but-undrained `retire:bytes:`
  obligation a *protection class* GC must honour by keyed lookup, and X111 makes the
  stale-mark cleanup a **migration gate** the custodian must observe before the identity-keyed
  retirement paths are enabled. Both live in `crates/custodian/src/gc.rs` — #637's file, and
  explicitly out of this slice's scope. Converting the *existing* paths now would move orphan
  evidence later for every ordinary overwrite while GC still has no notion of the obligation
  that protects those bytes in the gap. For the multipart path the same delay is harmless (no
  mark ⇒ GC's conservative arm retains), which is why Complete's obligation is in and the
  existing paths' conversion is not.

**(ii) The `orphan:` value is unchanged — still the bare decimal, and the three-arm rule is
implemented as its two safe arms.** 0016 D4.2 wants `{orphaned_at_millis, event}` so a *stale*
mark from an **older** unreference event can be re-stamped. Writing structured values from
`core` would make `gc.rs`'s bare-`u64` parse (`gc.rs:315-330`) drop those entries — leaving the
fragment unreclaimable — and the dual-format decode is assigned by 0016 itself to `gc.rs`
("What the implementing slices change"). So this slice implements **absent ⇒ write under
`require_absent`; present ⇒ skip with no mutation, original stamp intact**, which is the
concurrency half X56 needs (and the leg-I(iii) DST case pins it), and leaves the
stale-evidence re-stamp arm to the slice that owns the decoder. `multipart.rs:3401-3412`
documents the boundary in code. **This is a scope boundary, not a symptom guard**: the arm I
omitted cannot be implemented correctly from `core` alone.

**(iii) `Completed` sessions are not terminally deleted here.** Per the brief's CORRECTED
2026-07-26 note: `teardown_session` reclaims and drains for a `Completed` session but does not
delete it or decrement the counter — `W_tombstone` is #625's. Leg E asserts this positively
(count unchanged, tombstone still answers an identical retry).

**(iv) `reconcile_step`'s signature and `GcContext`'s fields are untouched** (#637's), as the
brief requires. The classification sweep takes the fleet inventory as an argument precisely so
`core` needs no fleet-iteration seam.

## 6. Two small changes outside `crates/core` — both deliberate, both minimal

* **`crates/custodian/src/desired_state.rs:33-38`** — `DESIRED_PREFIX` / `desired_key` now
  delegate to `metadata::DESIRED_DSERVER_PREFIX` / `desired_dserver_key`. The staged intent's
  drain fence must `require_absent` that exact key, and `core` cannot depend on `custodian`
  (ADR-0010's direction). Two spellings of one key protocol is the drift `orphan_key`'s own doc
  comment warns against, so there is now one definition — in `core`, beside `orphan_key`, for
  the identical stated reason. No behaviour change; 3 lines.
* **`crates/core/src/metadata.rs`** — `SegmentedPublication::segment_batch` gained
  `+ Send + Sync`. A completer that publishes inside a spawned task needs the publication future
  to be `Send`, and a bare `&dyn Fn` makes it neither — the DST campaign spawns two publishers to
  interleave them, so without this leg I(i) cannot be written at all. Every hook in the repo is
  a closure over shared references, so the bound costs nothing; the two annotated hooks in #635's
  own tests were updated (2 lines).

Plus **one new dependency**: `sha2.workspace = true` in `crates/core`'s `[dependencies]`. The
ETag composition is lowercase-hex SHA-256 (ADR-0047 closed the basis, `0047:73-89`, and deferred
only the composition, `:112`), so `core` must be able to hash. `sha2 = "0.11"` is already a
workspace dependency on the `deny.toml` allowlist, used by `gateway-s3` and `server` — **no new
crate enters the tree** (the `Cargo.lock` delta is one line). Declared here as the brief asks.

## 7. Gates

* `cargo xtask ci` — **green** (`/var/tmp/pdca/pdca-builder-636-ci3.log`, the final tree; `ci2`
  is the run before the last cleanup pass), including the prose
  gates (`typos`, `lint_docs`, `render_site --check`), which the brief flagged as the
  docs-currency risk. Both `typos` and the docs renderer are installed on this host, so the
  docs-currency edits to `docs/design/architecture/{06,08}` are genuinely gated here rather than
  warn-skipped.
* `cargo fmt --all -- --check` — clean, so the target's commit hook will not reject the patch.
* `cargo clippy --workspace --all-targets` — clean under `warnings = "deny"` + `clippy::all = "deny"`.

## 8. Open questions I did not absorb (0016's *Open questions* 2)

The brief says: "If Do finds a third thing, surface it rather than absorbing it." I found one,
and it is small: **the drain's stall detector** (§3) is a *third* thing beyond "a caller-driven
bounded drain here, a scheduled one there". It is a caller-side liveness observable, not a loop,
so I judged it inside this slice's boundary — but #625 will want to wire `DrainStalled` into its
drain-health alarm, and it should be told the type exists rather than discovering it.

## 9. Scratch

Everything under `$PDCA_SCRATCH` (`/var/tmp/pdca`): three gate logs, five negation logs and the
red-leg log, all named `pdca-builder-636-*`. No `/tmp` use; the two throwaway apply-check
checkouts and the module backups were removed. The **logs are deliberately left in place** so the
human can read the recorded negation output at sign-off (~380 KB total); delete them once §9 is
recorded.

## 10. Verification trail (paste-ready)

```
./engine/xtask.sh ci                                  -> EXIT=0  (3 runs; final: ci3.log)
cargo fmt --all -- --check                            -> clean
cargo clippy --workspace --all-targets                -> clean (warnings=deny, clippy::all=deny)
cargo test -p wyrd-core --test multipart_protocol     -> 7 passed
cargo test -p wyrd-core --test multipart_admission_and_drain -> 5 passed
cargo test -p wyrd-core --lib multipart               -> 11 passed
xtask ci -> run_dst  crates/dst/tests/concurrency.rs  -> 11 passed (5 base + 6 added), 50 seeds
git apply --check patch.diff on a pristine HEAD       -> applies cleanly
```
