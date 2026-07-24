# Adversarial review — issue 626 / multipart-commit-protocol (iteration 2)

Lens: refute the artifact against the brief's Refutation standard (outcomes (a)–(d) absent
from the execution register) and the gates' claims. Deliverable is a design document; all
citations are on the patched tree at `$PDCA_TARGET`. Every proposal citation I spot-checked
(`crates/traits/src/lib.rs:286`/`:746-751`, `crates/core/src/metadata.rs:346-350`/`:763-796`,
`crates/custodian/src/gc.rs:77-104`, `crates/custodian/src/restore.rs:100`,
`crates/core/src/write.rs:474-500`, `crates/server/src/lib.rs:49-53`) verifies against the
target source, and the template section set + frontmatter + index row (leg A) conform.

## Refutations that landed

- NEEDS-HUMAN [impl] — **F18's "Eliminated" disposition is refuted by a rollback/re-Complete
  race the execution register does not contain (outcome (c)).** Trace: Complete fences and
  writes `seg:<id>:*` records, completer crashes before the flip; the reaper rolls back after
  `W_completing` and installs `retire:records:{seg:<id>}`
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:791`, batch rows `:324-325`,
  drain semantics `:885`); the drain is asynchronous and may lag — the design itself provides
  an oldest-obligation-age alarm for exactly that. The client then retries Complete: segment
  keys are deliberately **upload-id-scoped, not per-attempt** (`0016:891`) and are "recomputed
  fresh" at the same keys (`0016:887`), so the new attempt re-writes `seg:<id>:*` and flips —
  object published, session `Completed`. The still-pending old obligation now drains and
  **deletes the published object's segment records** (or deletes them mid-phase, before the
  flip, so the root publishes over missing segments). The committed root resolves
  `Segmented{owner}` → empty range → the chunks fall out of the reference set → GC reclaims —
  a preserved chunk map over reclaimed bytes, outcome (c). Exact-value delete preconditions
  don't save it: when the part set is unchanged the recomputed segment bytes are identical.
  X37 (`0016:1021`) covers crash→resume and crash→rollback, but **not**
  rollback→re-Complete-while-obligation-pending; the F18 row (`0016:1143`) claims
  "Eliminated" on that incomplete enumeration. Iterable fix (builder's choice): the re-fence
  batch cancels/deletes the session's pending `retire:records:{seg}` obligation (O(1) — one
  record), or segment keys become attempt/epoch-scoped with the bounded-generation story
  reworked.
- NEEDS-HUMAN [impl] — **F11a's disposal is refuted: crashed mid-stream part attempts
  accumulate owned `pending:`/`sidx:` residue while the session stays `Open`, unbounded by
  the stated formula (outcome (a) at the limit).** The claimed per-session bound
  `≤ MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` (`0016:215`, `:664-667`, `:765`) counts only live
  attempts. But: loser compensation fires only on a *definitively failed commit*
  (`0016:643`) — a client that drops TCP mid-stream (or a crash-looping gateway, X8
  `0016:992`) never reaches one; the backstop reclaims only "when the session leaves
  `Open`... or vanishes" (`0016:992`); and the expiry sweep MUST skip owned entries
  (`0016:658`). So a session that loops start-part→disconnect for its `W_session` lifetime
  grows its `sidx:<id>:` range without any stated bound; past `SCAN_CAP` (≈2,600 crashed
  381-chunk attempts) the teardown's `scan("sidx:<id>:")` (`0016:789`) fails
  `ScanCapExceeded` — with no cursor-keyed walk stated for `sidx:` (unlike `retire:`,
  `0016:786`) the backstop reclaims nothing, which is the exact halt F11a was reworked to
  close. Compounding it: `MAX_INFLIGHT_PARTS` has **no stated enforcement mechanism**
  (`0016:664` — "admits at most", by whom, serialized against what?); after F12's own lesson
  (`0016:1137`) an unstated mechanism is observed-not-enforced, the class iteration 1 was
  rejected for. The F11 "Eliminated" row (`0016:1136`) is unwarranted as written. Iterable
  fix: define in-flight accounting to include unreclaimed owned residue (refuse part intents
  past the cap), state the enforcement point, and/or give `sidx:` teardown the cursor-keyed
  walk.
- NEEDS-HUMAN [impl] — **Internal contradiction: Complete under `Completing`.** The verb ×
  state table answers `409 OperationAborted` (`0016:489`), while decision 3's F5 bullet says
  a Complete that finds `Completing@E` **resumes** — "re-run any missing segment writes …
  then retry the flip" (`0016:473`) — and decision 7(c) says the resumed Complete is "the
  client retries, or a second gateway picks it up" (`0016:880-882`). The S3 wire has no
  complete-token, so an implementation cannot distinguish "the same client's retry" from "a
  second Complete"; the two normative statements are incompatible, and #508 cannot lift a
  success criterion from a cell that is simultaneously 409 and resume. One of the two must
  win (and the crash-recovery story changes materially if 409 does: recovery then waits on
  the `W_completing` rollback, a latency cost the accepted-costs register doesn't carry).
- NEEDS-HUMAN [impl] — **The terminal delete can double-decrement `mpuctl:count`, eroding
  D-C's "exact" bound.** The batch `delete mpu:<id> + CAS mpuctl:count -1` (`0016:795`,
  `:800-801`) is executed by *either* the inline-draining gateway or the reaper — explicitly
  concurrent-capable ("Whichever party runs next — the gateway inline, or the reaper",
  `0016:515-518`) — yet no `require(mpu:<id> == exact bytes)` precondition is stated on it.
  Two drainers racing the same fully-drained session: both read count `c`; the loser's CAS
  conflicts, retries by re-reading the *counter* (delete of an absent key is not a failing
  precondition), and decrements again. Each occurrence drifts the counter low → admission
  beyond `MAX_SESSIONS`, contradicting F12's "exactly `MAX_SESSIONS` admitted, no overshoot"
  disposition (`0016:1137`) — and the failure-mode table's only nearby row is *crash*
  mid-teardown (`0016:828`), not the concurrent-dual-drainer race. One sentence fixes it
  (the terminal batch is preconditioned on the session record's exact bytes).
- NEEDS-HUMAN [impl] — **"No value inside a settled range can break an invariant"
  (`0016:1273`) is overstated for `MAX_SESSIONS ∈ [1, SCAN_CAP)` (`0016:596`, `:1155`).**
  The staged reference-set build iterates every session's `part:<id>:` range per reconcile
  pass (`0016:411-414`): at legal top-of-range values that is up to ~10^10 record reads and
  an in-memory staged set of the same order, every pass — a custodian plane halted by
  work/memory is the same F7 halt class, but the knob's binding invariant covers only the
  `mpu:` scan cap. The range needs a work/memory-coupled bound (the scrub row `0016:420`
  half-admits the cost, then says "admission bounds" it — admission bounds the *count*, not
  the per-pass work at the range's top).

## Verdict on the gates and the brief's claims

- `check-gates.json` is honest as recorded: `T4-batch-review` is red (7 blocking findings,
  gating) and `overall: fail`; the vacuity of `C4-verify`/`C5-mutants` on a no-code diff is
  labelled exactly as the brief requires. No rationalization to refute there. The claims the
  findings above *do* refute are leg B(ii)'s F18 and F11 "Eliminated" dispositions
  (`0016:1143`, `:1136`) — a confirmatory pass could accept those rows as settled; they are
  not.

## Attempted and could not refute

The load-bearing carried constructions held under attack: the fence/epoch machine and the
O(1) session-precondition publication proof (I could not construct a lost-CAS/rollback race
that publishes stale state — every path fails the `require(mpu == …@E)`); the D-B restore
fence disposes iteration 1's F13 trace (X17, `0016:1001`) including the
snapshot-during-`Completing` variant; the batch inventory (`0016:316-328`) stays inside the
envelope in every row I recomputed; the arithmetic (D-F) checks out independently —
⌊50000/302⌋=165 / ⌊50000/131⌋=381 chunks, 312–520 segments, 50.3–193.5 GiB at 1 MiB chunks,
26.5–101.8 MiB chunks for 5 TiB, part-count not binding — and the counter-based admission
(modulo the double-decrement above) closes F12. Scratch note: no writable clone or build
dir was created; nothing to sweep.
