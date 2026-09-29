# Build notes — #809 restore-session-fence

Target: `getwyrd/wyrd @ main`, base `243241e` (origin/main at Do time; it already contains child-1,
#808, and the #813/#814 work). All `path:line` below are on that base unless marked "patched".
`restore.rs` and `multipart.rs` are unchanged since the brief's `f41e9c5`, so the brief's line
numbers for them still hold; `gc.rs` and `metadata.rs` moved, and are re-located by symbol.

## What changed, in one paragraph

A `Completing` session now stores its segment-group nonce in `publish_target.segment_nonce`
(`crates/core/src/multipart.rs:1954-1961` on base; patched `PublishTarget` + `segment_group()`).
`reconcile_after_restore` (`crates/custodian/src/restore.rs:312`) gains a **fence**: every session
the restored image holds `Open` or `Completing` is CASed to `Aborting@E+1` in one batch with the
retirement obligations its teardown owes. `Open` → `retire:bytes:s:<id>:<E>` `{session, all}`;
`Completing` → `retire:bytes:s:<id>:<E>` `{session, parts}` **and** `retire:records:s:<id>:<E>`
`{seg: (nonce, E)}` (X57). The report gains `staged_skipped`, `sessions_fenced`,
`staged_untrusted` and `sessions_unsettled`; `is_clean()` counts `sessions_fenced` and
`staged_untrusted`; `needs_human()` counts `sessions_unsettled`. The `deferred: #664` marker
(`restore.rs:819`) is replaced by a pointer to leg H(iii). `restore_verdict`
(`crates/server/src/cli.rs:1256`) prints the new counts, a NEEDS-HUMAN paragraph for unsettled
sessions and an informational line (no NEEDS-HUMAN) for untrusted staged records. Docs: 05
(`05-building-block-view.md:202`), 06 (§6.7, new paragraph after `:82`), the M4 blueprint's step 7
and its "what a restore does" list.

## Decisions, and what I ruled out

1. **Where the nonce lives: inside `PublishTarget`, after `epoch`.** The brief's settled call says
   "beside the fence epoch `PublishTarget` already carries" and cites `multipart.rs:1954-1961`
   (the struct). With both there, `PublishTarget::segment_group()` names the whole group
   `(nonce, E)`. Ruled out: a sibling field on the `Completing` variant — same blast radius on
   fixtures (every `Completing` literal changes either way), but splits the group across two
   places. `SegmentNonce` deliberately has no `Deserialize` (`metadata.rs:979-982` on base), so the
   field decodes through a `deserialize_with` that calls `SegmentNonce::new` — no edit to
   `metadata.rs`. `segment_group()` rebuilds through `SegmentGroup::new(...).expect(..)`; the
   `expect` cannot fire (the nonce was validated by the same constructor) and mirrors the one
   existing `expect` in that module (`multipart.rs:3899`). Rejected: adding an infallible
   `SegmentGroup::from_nonce` — 6 lines in `metadata.rs` plus a doc edit to its "the only way to
   obtain a SegmentGroup" claim (`metadata.rs:1026-1028`), for no behaviour change.

2. **Writer-side constructors in `multipart.rs`.** Every record type there had "no writer-side
   constructor" by rule (`multipart.rs:2127-2130`, `:3180-3182`), and the restore fence is the first
   writer. I added the narrowest API that keeps the rule's intent: `SessionRecord::fence_to_aborting`
   (a transition from a *decoded* record; `None` for `Aborting`/`Completed`, and for `epoch ==
   u64::MAX` rather than wrapping), and `RetirePayload::open_teardown` /
   `RetirePayload::completing_teardown`, which return the **key together with the payload**, so a
   writer cannot file a payload under a key its decoder refuses. The module header, the
   `SessionRecord` doc and the `RetirePayload` doc say so. `completing_teardown` takes the token
   epoch from the attempt's own group, so the segment-epoch rule
   (`RetireSegmentEpochMismatch`, `multipart.rs:3346-3353`) holds by construction.

3. **The `Open` fence installs `{session, all}`, not `{session}`.** The retire-rows table lists
   `{session}` as "the abort/reap fence's own spelling (0016:664)" (`multipart.rs:3141`), but
   `{session}` owes only the `sidx:` residue (`RetirePayload::session`, `multipart.rs:3206-3212`):
   an `Open` session's committed part records, and their bytes, would be left with no deleter,
   which breaks the brief's invariant. `{session, all}` is the reaper's `Open` teardown
   (`0016:2187`, the pseudo-code line itself), the only row the wildcard has, and the right shape for
   a fence that preconditions on the session record alone (a part commit does not write it, so a
   list read before the fence could miss one; `PartScope` doc, `multipart.rs:2727-2736`). Whether
   the client-Abort row at `0016:664` really means `{session}` is #656's to settle; I did not
   touch that table row.

4. **The `Completing` fence always installs `{seg}`**, whatever `segments_written` says: 0016's row
   is "1 put", not "≤ 1" (`0016:665`), and deciding by the cursor would leave records a damaged
   cursor under-counts. An empty range drains to nothing. Both obligations sit at token `E` (the
   epoch fenced from), which is what the key/segment relation requires (`multipart.rs:3313-3318`).

5. **The fence runs LAST in the pass (after the mark commits), not first.** I first put it before
   the staged read. That broke three existing concurrency legs in `staged_protection.rs`
   (`restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_1`, `..._after_read_2`,
   `..._flipped_after_read_1_and_drained_after_read_2`). They failed at the hook-outcome assertion
   (`staged_protection.rs:1478-1485` on base: outcomes came back `[Conflict, Committed]`) and at
   the read-order assertion (`:1497-1501` on base). Cause: those legs land a publication (a root
   flip preconditioned on `Completing@E`) *during* the restore's protection reads; with the fence
   first the session is already `Aborting`, so the flip can no longer land at those points at all,
   and the fence's own `part:<id>:` read precedes the staged `sidx:<id>:` read. Keeping fence-first
   would have meant rewriting the whole `publication_during_restore` harness
   (`staged_protection.rs:1432-1502` on base, 71 lines, shared by all three legs) and giving up the
   interleavings it tests. Fence-last keeps every existing reading and its order exactly as before
   (all three legs pass unchanged), and keeps the DST restore sweep's read timing unchanged
   (`RESTORE_NEMESIS_SPAN`, `crates/dst/tests/custodian.rs:1819` on base, is measured from the pass
   start). Cost of fence-last: a store fault earlier in the pass (e.g. a failed mark batch) leaves
   sessions unfenced until the next run. That is acceptable because gateways must wait for the
   fence anyway (`0016:723-728`), and the durable "fence ran" record is #810's (the patched doc
   says so).

6. **`sessions_unsettled` (a field the brief does not name).** Legs H(i)/H(ii)/K require naming a
   session "as needing a human", and the test may only observe that through `Debug` +
   `needs_human()`. I did **not** fold these into `unresolvable`: that list withholds every mark
   in the fleet (`restore.rs:383`), but an undecodable session *value* hides nothing about which
   fragments are staged — the staged reader walks a session's ranges by key and never decodes the
   value (`gc.rs:1505-1510` on base). Withholding the whole fleet's marks over it would be wrong
   in the other direction. The list also carries the fence's other failure modes: `Conflict`,
   epoch overflow, an undecodable records obligation on re-check.

7. **`is_clean()` also counts `sessions_fenced`.** The brief only says it must count
   `staged_untrusted`. The predicate's own doc says it "also counts the work this pass DID"
   (`restore.rs:189-192`); a run that aborted uploads is work an operator absorbs, like marks. No
   caller branches on `is_clean()` except the summary audit line (`restore.rs:1038`) — the CLI uses
   `needs_human()` only. Leg H(iii)'s **second** pass (nothing fenced, only the untrusted record
   left) pins that `staged_untrusted` alone takes the run off clean, so the brief's requirement is
   tested independently of this choice.

8. **K: re-checking an already-`Aborting` session.** A fence `Completing@E → Aborting@E+1` files
   `retire:records:s:<id>:<E>`; nothing else files a records obligation at `E' - 1` for a session
   at `Aborting@E'` (a rollback files at the epoch it leaves and lands in `Open`). So one keyed
   `get` finds the one attempt to re-check, never a range read of `retire:` — which also avoids
   false positives from *earlier* rolled-back attempts, whose segments may legitimately name
   chunks a later re-upload replaced. Deferred and marked in code (`deferred: #659`): once the
   drain exists, a run between it deleting part records and it deleting segment records would see
   strays. Not reachable today (no drain).

9. **Leg J's location.** The brief says "the codec's own test module in `multipart.rs`", but
   `multipart.rs` has no in-file test module; every multipart codec leg lives in
   `crates/core/tests/multipart_*.rs`. J is in `crates/core/tests/multipart_session_records.rs`
   (the session codec's own file). It is green-only either way: C4-verify reverts modified test
   files on the red leg.

10. **Staged skips count only after the committed gate.** A fragment both a committed map and a
    staged record name counts as referenced (uncounted), as before; `staged_skipped` is "kept
    because of the staged class alone".

## Existing tests I had to change (and why)

- `crates/core/tests/multipart_session_records.rs`, `multipart_state_machine.rs`: every
  `Completing` fixture / `PublishTarget` literal gains the nonce (the field is required). Added
  leg J (4 tests: round trip + group; no nonce refused; bad nonce refused; `fence_to_aborting`).
- `crates/core/tests/multipart_retire_obligation.rs`: 3 tests for the two constructors, each
  minted obligation decoded back against its key through the file's identity-checking helper;
  header sentence updated.
- `crates/custodian/tests/staged_protection.rs`: the `Completing` fixture gains the nonce (it is
  decode-checked, 8 legs panicked without it). Leg E's
  `a_session_whose_value_will_not_decode_still_protects_its_records` asserted the pass never names
  that session on its audit seam — false by design now (the fence must decode the value, and names
  one it cannot). It now asserts: not in `unresolvable`, not named as `unresolvable-staged-record`,
  named in `sessions_unsettled` and as `session-unsettled`; the protection assertions are
  unchanged. Leg B now also asserts `staged_skipped == 14`. Stale comments fixed.
- `crates/custodian/tests/staged_scrub.rs`, `staged_repair.rs`: `Completing` fixture nonce.
- **`crates/dst/tests/custodian.rs`** (brief: say so if touched): the staged-handoff DST's
  `Completing` fixture (`custodian.rs:2914-2917` on base) is decode-checked by `handoff_session`
  (`:2766`) and would have panicked. Only the fixture changed; the properties are GC's and
  reconstruction's, not restore's.

## Red → green (the test file the brief names)

`crates/custodian/tests/restore_session_fence.rs`, 11 tests, names only base-visible symbols; new
report fields are read from `Debug`.

- **Red**, run 1 (before any production edit, 8 legs): 8 ran, 8 failed by assertion, compile OK.
- **Red**, final (all 11 legs; every changed file except the new test reverted to base, exactly
  what C4-verify does): `cargo test -p wyrd-custodian --test restore_session_fence` → exit 101,
  **11 ran, 11 failed**, 0 compile errors. Each failure is the missing behaviour: session still
  `Open@3` (F); no fence commit ever attempted (F-atomic, race); no `staged_skipped` (E); no
  `staged_untrusted` (H-iii); `needs_human()` false (H-i, epoch-overflow, K-undecodable); no
  `sessions_fenced: 3` (K); the `Completing` fixtures stay undecodable-on-base, i.e. never fenced
  (G, H-ii) — the brief anticipates base rejecting the nonce field.
- **Green**: 11 passed.
- Runner: `cargo xtask` has no single-test subcommand, so the red/green runs used the exact
  invocation `engine/scripts/run-verify.sh` uses (`cargo test -p <pkg> --test <name>`), inside the
  cycle worktree, wrapped in `timeout 1200`. The whole-tree gate ran through `engine/xtask.sh ci`.

## Refuting my own test

- **(a) Genuine red?** Yes — reverted every production file (and the modified tests) to `243241e`
  with the new test kept; 11/11 failed by assertion (above). Restored afterwards.
- **(b) Production path?** Yes — every leg calls the production `reconcile_after_restore` through
  `GcContext`; the fence, the codec and the obligation constructors are the production ones. The
  doubles are only the `MetadataStore`/`ChunkStore` seams (in-memory map with atomic
  preconditions, the seam's own paging helpers, a scan cap of 2 in G and K to force paging).
- **(c) Fixture includes the fault?** Yes — the resurrected sessions are really `Open`/`Completing`
  with parts and segment records present; H(i) seeds the actual pre-decision shape; H(ii) seeds a
  segment naming a chunk no part holds; the atomicity leg fails the real fence commit (both split
  orders: the session put and the obligation put) and the race leg lands a real concurrent
  Complete fence between the pass's read and commit; K re-runs over the fenced store itself.

## Whole-tree gate

`engine/xtask.sh ci` (= `cargo xtask ci`) in the cycle worktree, final tree: **exit 0, "xtask ci:
all checks passed"** — typos, `lint_docs.py`, `render_site.py --check` (link audit OK), both repo
guards, `cargo fmt --check`, clippy (`-D` via workspace lints), build, `cargo test --workspace
--exclude wyrd-dst`, cargo-machete, cargo-deny (both configs), conformance, the statics gate, the
deploy guard, and the DST tier (clippy + `cargo test -p wyrd-dst`, `--cfg madsim`, 50 seeds).
Summed test results across the run: 1524 passed, 0 failed. (The first full run stopped at
clippy on two `cloned_ref_to_slice_refs` lints in the new test file; fixed before the final run.)

`patch.diff` was checked by applying it to a pristine `git archive 243241e` tree: it applies
cleanly and the patched `crates/` and `docs/` trees are identical to the worktree's.

## For the human

- No external dependency was missing: `typos` (1.48.0) and the docs-renderer deps were present,
  and the gate's prose steps ran (not skipped).
- **No seeded DST property covers the fence racing a Complete.** The rubric wants seeded Tier-0
  coverage for a new concurrent path; the brief puts `crates/dst/tests/custodian.rs` out of scope.
  The race is covered deterministically in-process (leg "a fence that loses its compare-and-set"):
  the concurrent Complete lands, the fence writes nothing, the session is named, and the next run
  fences it. Worth a follow-up issue if you want the DST sweep.
- Downstream (not done here): #658 must write `segment_nonce` when it fences into `Completing`;
  #656 can reuse `fence_to_aborting` + the two teardown constructors (and should settle the
  `{session}` vs `{session, all}` question for the client Abort row); #659 owns the drain order
  noted in the `deferred: #659` marker; #810's durable fence record belongs right after
  `fence_sessions` returns.
