# Adversarial review — issue 803 (staged protection class in the shared reference set)

Re-ran the asserted proof against the target source. `cargo test -p wyrd-custodian --test
staged_protection` is **green** (14/14), and the frozen `gate-logs/C4-verify.log:15-123` shows all
14 going **red by assertion** (not by compile error) with production reverted. The red→green
evidence is real, drives the production `reconcile_step` / `reconcile_after_restore`, and I could
not make it pass for the wrong reason. The findings below are about the fix, not the proof.

## Findings

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/restore.rs:339` (and the gate at `:408`): the
  post-restore pass **still marks a live upload's staged fragment**, and a later GC pass deletes
  it — the exact chain `brief.md:14-17` says this slice exists to break. The pass reads the
  committed namespace **twice** (`referenced_fragments` at `:309`, then `committed_chunks` at
  `:337`) and diffs them through `appeared_since` precisely so "an object that committed between
  the two reads cannot have its live fragments marked on the strength of the older one"
  (`:339-342`). The patch adds the staged class to the **first** read only; nothing re-reads
  `mpu:` / `sidx:` / `part:`, so any upload whose intent record lands after
  `referenced_fragments` and whose fragment lands before the fleet walk at `:373-379` is
  unprotected at the gate. Concrete failing case, run against the target source: a session
  admitted with one owned `sidx:` entry the instant the reference build's `inode:` scan returns,
  with the fragment written right after it (the real client order: intent, then `put_fragment`) —
  `reconcile_after_restore` returns `RestoreReport { stranded_marked: 1, ... }` and
  `orphan:3:<chunk>:0` is written. The module doc added at `restore.rs:71-75` ("Nor is a fragment
  a multipart upload has **staged** but not yet published **ever** marked") and the brief's
  invariant ("protection overlaps across handoffs — no gaps, never a partition", `brief.md:76`)
  both claim more than the code delivers. Fix is either a staged half for `appeared_since` or a
  narrowed doc claim plus a `// deferred: #N` marker at `restore.rs:339`.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/gc.rs:791`, `:799`, `:802` (the three `?` in
  `staged_fragments`) reached from `crates/custodian/src/scrub.rs:88`: a store fault under a
  namespace scrub never reads **aborts the scrub pass**, so a committed fragment that is genuinely
  missing is never found and its repair obligation is never enqueued. This corroborates both T4
  blocking rows (`gate-logs/T4-batch-review.log:10`) with a demonstration rather than an argument:
  over one committed object whose only fragment is absent plus one `Open` session with an owned
  entry, `reconcile_step(.., ScrubContext, ..)` answers `Changed` with `queued_repairs() == [chunk]`
  when healthy, and `Err(Store(StoreFault("sidx:<id>:")))` with **zero** repairs queued once
  `scan("sidx:<id>:")` faults. The specific unwarranted claim is the doc the patch itself adds at
  `gc.rs:215-217`: *"it reads no staged record, so a staged one it cannot read is no hole in its
  reading"* — true for a **damaged record** (correctly kept out of `ReferenceSet::unresolvable`,
  pinned by `staged_protection.rs:1142`), false for an **unreachable read**, which the same commit
  makes fatal to scrub. The same fault also turns `reconciliation_status` into an `Err`
  (`crates/custodian/src/desired_state.rs:188`), against that function's own contract at `:178-180`
  ("One damaged object never turns this query into an `Err` ... blanking the fleet's drain status
  over one record is the outage the containment rule exists to prevent").
  **Test gap that let this through:** the E(iii) legs (`crates/custodian/tests/staged_protection.rs:1417`,
  `:1423`, `:1429`) assert only that GC and restore return `Err`; no leg asserts what scrub or
  drain status answer under a staged store fault, so the "scrub and drain status keep today's
  answers" claim (`brief.md:96-99`) is pinned for torn records and unpinned for unreachable ones.

- **NEEDS-HUMAN [human]** — `crates/custodian/src/gc.rs:790-806` read from
  `crates/custodian/src/desired_state.rs:188` and `crates/custodian/src/scrub.rs:88`: every
  consumer of the shared builder now pays `1 + 2N` serial metadata `scan`s for a staged set two of
  them immediately discard (`reconciliation_status` reads `placed` / `unresolvable` only; scrub the
  same). `N` is the live session count, capped at `MAX_SESSIONS = 46`
  (`crates/core/src/multipart.rs:4619-4624`), so a single drain-status query goes from 1 scan to up
  to 93, and a sweep across an `M`-server fleet — the surface is read per D server — from `M` to
  `93M`. This is the same root cause as the row above (the shared builder doing staged work for
  consumers that did not ask for it); it wants **one** decision, not two. Judgment call because the
  brief's whole framing is "the SHARED reference set every destructive pass reads" (`brief.md:74`),
  so scoping the staged read to its two consumers is a design change, not a bug fix.

## Attempted and could not refute

- **The red→green itself.** All 14 legs fail by assertion on reverted production and pass on it;
  the file names no symbol the slice adds; the doubles sit under the real `reconcile_step` fenced
  control point, not a re-implementation.
- **Bounded per-session reads (leg D).** `scan(MPU_PREFIX)` cannot return the admission singleton
  (`multipart.rs:1126-1132`, `mpuctl` has no `:`), `MAX_SESSIONS = 46` and
  `MAX_PARTS_PER_SESSION = 10_000` both sit far under `SCAN_CAP = 1 << 20`, so no range read here
  can blow the cap at the shipped profile.
- **A non-canonical `mpu:` key silently mis-deriving its ranges.** `UploadId::new` validates rather
  than normalises (`crates/core/src/multipart.rs:844-848`), so a key that is not the canonical spelling lands in
  `staged.unresolvable` and blocks, rather than sending `sidx_range` / `part_range` at a prefix
  that holds nothing.
- **Both build-window handoffs (leg C).** Source-before-destination holds for every interleaving I
  could construct: `sidx:` → `part:` per session, then the `inode:` scan; a publication landing
  before the `mpu:` listing is caught by the `inode:` scan, one landing after it by the part range
  or the inode scan.
- **`add_chunk`'s length rule.** `ChunkRef::fragment_count()` depends on `scheme` alone
  (`crates/core/src/metadata.rs:148`), so the synthetic `len: 0` at `gc.rs:711` cannot skew it;
  the empty-placement case is held rather than identity-filled and is covered at
  `staged_protection.rs:1282`.
- **Ordering inside `protection()`.** `placed` before `malformed` before `staged.placed` before
  `staged.held` before `is_incomplete` (`gc.rs:468-480`) — I could not construct a fragment that
  gets filed under a rule that did not actually hold.
- **Scrub / drain status answering differently over a *damaged* staged record.** They read
  `referenced.unresolvable` (`scrub.rs:205`, `desired_state.rs:225`), which the patch leaves
  committed-only; `staged_protection.rs:1142` pins it.
- **The drain answering `Satisfied` while `staged.placed` names the draining server.** Explicitly
  deferred to #664 in `brief.md:96-99` and at `staged_protection.rs:1167-1170` — settled per the
  rubric's deferral rule, not re-raised.
