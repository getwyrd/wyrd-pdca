# Adversarial review — issue #926 (657.1, UploadPart staging)

Re-ran the proof on a scratch copy of the patched tree: `cargo test -p wyrd-core --test multipart_upload_part_stage` is green, 54/54. C4-verify is UNVERIFIABLE because the test calls API the patch adds; the brief declared that in advance, so it is not a refutation. I found one concrete failing case (the first bullet). It is the same root cause as the gating T4 blocker, which it confirms and makes sharper.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:1334-1343`: `renewing` awaits `renew_if_due` first, then checks the write deadline, and only after that polls `work` again. So a fragment put that the D server answers **before** its deadline, while a renewal the timer started is still committing, is thrown away, and the chunk fails `WriteDeadlineElapsed`. Concrete case (a probe test I added to a scratch copy, using the file's own `Rig`):
  - `write_window_millis = HALF + 100`, so the deadline is T0+5100, and chunk 1's fragment 0 put is held.
  - The driver advances the clock to T0+5001 and fires the timer. The renewal commit (`Act::First` hook) releases the held put at T0+5001, which is inside the deadline, then advances the clock 200 ms (a slow metadata commit).
  - Result: `Err(Stopped { cause: WriteDeadlineElapsed { chunk: 1, deadline_millis: 1005100 } })`, even though every put was answered in time.

  This also contradicts the module doc at `stage.rs:29-33` ("writes still pending once their deadline has passed are abandoned"): these writes were finished, just not polled. The existing deadline leg cannot see it, because it pins `W + 1 < HALF` so that no renewal is ever due (`crates/core/tests/multipart_upload_part_stage.rs:1364`). Adding `if let Some(out) = futures_util::FutureExt::now_or_never(work.as_mut()) { return Ok(out); }` right after `renew_if_due` makes the probe pass and keeps all 54 tests green. That one change does not close T4's broader point, though: while a renewal is committing, the deadline is not enforced at all. To close both, race the renewal against `work` and the deadline, or check the deadline before starting a renewal. Either way, add the probe schedule as a test.
- NEEDS-HUMAN [human] — `stage.rs:947-951`, `:1266`: a body read has no bound in core. Renewal keeps the slot and owned leases alive for as long as `body.next()` stays pending, so a client that stalls forever holds its slot forever. With `MAX_INFLIGHT_PARTS = 16` (`multipart.rs:5094`), 16 stalled requests leave a session refusing every new part with `SlowDown`. The rubric's *Await discipline* rule says every await on external work is bounded. Today the bound is only handed to "the gateway's idle timeout, #669" in a doc comment, not through a `deferred: #669` marker or a parameter. Decide whether core takes a body-idle bound now, or whether this becomes a tracked deferral.
- NEEDS-HUMAN [human] — `stage.rs:892` vs `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1714`. Proposal 0016 says that after an ambiguous reserve, the reserver "probes onward only once it reads that index carrying *another* attempt's id". The code refuses `SlowDown` instead, and `c_a_held_index_taken_by_another_refuses` pins that. Under the trait contract (`crates/traits/src/lib.rs:240-247`), the code's reading is the safe one: if the other attempt later releases the index, the held batch's `require_absent` can still pass. So this is a gap in the spec, not a bug in the code. It costs extra 503s under contention. This slice may not edit 0016, so someone should file the spec fix.
- Note (no action needed here): `stage.rs:348-354`, `:1272-1275`: an overrun body comes back as `StageOutcome::Staged` with `chunk_bound_exceeded() == true`, and `len()`/`digest()` describe the truncated prefix. If 657.2 forgets to check the flag, it commits a truncated part whose digest still matches. The brief asked for a flag, so this is in spec. A separate outcome variant would make the mistake impossible; worth weighing before 657.2 builds on this.

## Attempted to refute, could not

- **Drain fence (B1/B2), the two mutants that survived in #636:**
  - Replacing `topology.excluding(draining)` with `topology.clone()` (`stage.rs:684`) fails B1, B2 and all three B3 legs.
  - Removing the per-server `require_absent` loop (`stage.rs:1455`) fails B1.
  - A fence on one server only would fail B1's `fenced == named` set check.
- **Same-index pin (C), which survived in #657's first attempt:**
  - Deleting `pinned = Some(index)` (`stage.rs:823`) fails 4 (C) legs, including `c_an_ambiguous_reserve_retries_the_same_index`.
  - Making a held index taken by another attempt probe onward (`stage.rs:892`) fails `c_a_held_index_taken_by_another_refuses`.
- **Intent recovery, renewal settle and the iteration-1 lease fixes.** I traced `intent` (`stage.rs:1397-1534`) and `Footprint::renew` (`:1064-1154`) by hand:
  - Every round reads the clock afresh and runs `check_live` after the drain scan.
  - Every later batch requires the slot bytes a held batch also requires, so at most one of them lands.
  - A held intent that lands between the entry read and the slot read is adopted.
  - I found no schedule that writes a fragment before its entry is observed durable, or that revives a lapsed lease.
- **C5:** 132 mutants, 0 missed. **Docs currency:** `05-building-block-view.md` is updated. **Drain key bytes:** the custodian re-exports `desired_dserver_key` as `desired_key`, and the parser is unchanged.

Scratch copy, probe and mutants were built under `$PDCA_SCRATCH/pdca-adversary-926-probe` and removed afterwards; no file under `$PDCA_TARGET` was changed.

<!-- pdca:leaf-complete -->
