# Adversarial review — issue_364 (iter-9), orphan-ledger high-water-mark fix

Scope of this diff's headline fix: `high_water_marks` now also scans `orphan:`
(`crates/core/src/metadata.rs:615-621`) so `Gateway::recover`
(`crates/server/src/lib.rs:101-109`) bumps `next_chunk` past a deleted object's
still-live orphan chunk id, plus a new red→green test
`restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`
(`crates/server/tests/s3_http_wire.rs:628-707`).

## Findings

- **NEEDS-HUMAN — The RED half of the red→green is synthetic: the test's "reclaim" bypasses
  the production `ReferenceSet::protects` gate, so it demonstrates a data-loss that the real
  custodian GC would NOT produce.** The test drives production `put_object`/`delete_object`/
  `recover`, but then models GC reclaim with a **test-local loop** that unconditionally calls
  `store.delete_fragment(frag)` for every `orphan:` record
  (`crates/server/tests/s3_http_wire.rs:679-685`), asserting it is "exactly the reclaim the
  orphan ledger authorises … gc.rs:152" (`:668-670`). It is **not** exactly that: the real
  GC runs the reference-safety gate *first* — `if referenced.protects(dserver, frag) { …skip }`
  (`crates/custodian/src/gc.rs:124`, built from committed inodes at `gc.rs:100,207-215`) —
  and only reaches `delete_fragment` at `gc.rs:152` for **unreferenced** fragments. In the
  buggy (no-orphan-scan) world the test targets, object B re-mints chunk 1 and **commits** an
  inode referencing it *before* the reclaim runs (`put_object` returns fully committed at
  `:663-665`), so `protects(dserver, {chunk:1,index:0})` is TRUE → the real GC **skips** the
  fragment and `GET B` **succeeds**. The genuine production consequence of the missing
  orphan-scan in that ordering is a **silent, permanent fragment leak** (A's orphan record is
  never reclaimable), not the "GET B fails / data loss" the test asserts (`:698-706`). The
  fix is still defensible, but the evidence proves a failure mode the production path
  prevents — a human should decide whether this counts as a valid red→green for the claimed
  defect.

- **NEEDS-HUMAN — The "permanent data loss" framing in the doc/test overstates the reachable
  defect; the only real data-loss window is one the test does not exercise.** The doc comment
  hedges "either leaks the old bytes permanently … or reclaims a fragment the re-minting
  object has just written **but not yet committed** (data loss)"
  (`crates/core/src/metadata.rs:578-582`). Data loss under the real GC requires GC to fire in
  the narrow window between B *writing* its fragment and B *committing* its inode (while B is
  still `Pending`, hence unprotected, and A's grace has already elapsed). The test instead
  fully commits B and then reclaims (`:663-685`), which is precisely the ordering in which the
  real `protects` gate makes data loss impossible. So the test cannot distinguish "leak" (the
  real outcome of its scenario) from "data loss" (the outcome it asserts) — it only reaches
  data loss because its reclaim loop drops the safety gate. The uncovered concurrent
  write-before-commit window has **no regression test**.

- The fix is correct for the hazard it names, on every angle I could attack: chunk-id
  projection to the `<2^64` in-process space is consistent with the `inode:`/`pending:` scans
  (`metadata.rs:599,606,617`); `parse_orphan_key` cleanly rejects non-orphan keys sharing the
  prefix (`metadata.rs:66-73`); `recover` is monotone `fetch_max` (`lib.rs:103-108`);
  PUT-overwrite's superseded chunks are covered because `commit_overwrite` writes the same
  orphan records the scan reads (`lib.rs:161-165`); and inode re-mint after DELETE is
  genuinely safe because `unlink` removes the inode key so `create`'s `require_absent`
  succeeds. **Attempted to refute chunk-projection truncation, orphan-key mis-parse,
  overwrite-orphan coverage, and inode re-mint safety; could not.**

## Bottom line

The code change is sound and closes a real leak/re-mint hazard. My objection is to the
**evidence**: the new test's reclaim is a re-implementation that omits the production GC's
reference-safety gate (`gc.rs:124`), so its RED demonstrates a data-loss outcome the real
custodian would prevent, and it leaves the one genuinely data-losing window (reclaim between
B's fragment write and B's commit) untested. Advisory only — human adjudicates at sign-off.
