# Adversarial review — issue 626 / multipart-commit-protocol (iteration 5)

Verdict context: the gating `T4-batch-review` is currently RED (6 blocking, 0 recorded-rejected
— `check-gates.json`), so the deterministic gate already blocks publish; `C4-ci` green was
re-verified plausible (typos clean on both changed files; template section set matches
`docs/design/templates/proposal.md`; patch touches exactly the two mandated docs paths). The
vacuity of `C4-verify`/`C5-mutants` is honestly recorded, as the brief requires. The findings
below are leg-B refutations under the brief's Refutation standard, grounded on the patched tree.

- NEEDS-HUMAN [impl] — **Segmented single-PUT publication has no evidence or reclamation
  machinery — a crashed one strands `seg:` records forever (outcome (a)), and the execution is
  absent from the register.** The doc mandates uniform segmentation for single PUTs
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1164-1168` "one publication
  path for both"; `:1693-1694` "a large single PUT is published as a segmented object"), yet
  every piece of decision 7's machinery is anchored to a multipart session the single PUT does
  not have: the `seg:` key is `seg:<upload-id>:<epoch>:<i>` and its only stated writer is "the
  Completing session's segment-write phase" (§1 table, `:222`); segment batches are fenced by
  `require(mpu == Completing@E)` (`:393`); crash evidence is `publish_target` while
  `Completing@E` and the reaper's `W_completing` rollback installing
  `retire:records:{seg:<id>:<E>}` (`:1203-1219`); the reaper walks only `scan("mpu:")`
  (`:1080`); and a global `seg:` scan is forbidden (`:1252-1255`). Concrete failing case: a
  gateway publishes a 1 GiB single `PutObject` (> `MAX_MAP_CHUNKS` at default chunks ⇒ must be
  segmented) and dies between segment writes and the root flip — there is no `mpu:` record, no
  fence, no `publish_target`, no rollback arm, no `retire:records:` installer, no reaper
  coverage, and no scan allowed to find the residue; there is not even a defined `<upload-id>`
  to key the segments by. The dangling `seg:` records are stranded metadata with no bounded
  reclamation path — refutation outcome (a) — and X37/X40 (`:1387`, `:1388`) cover only the
  session-anchored variant. The open question at `:1743-1746` mislabels this as "code
  factoring"; the missing protocol half (group-token minting, fence analogue, crash evidence,
  reclaimer) gates #508's single-PUT-segmentation obligation and must be designed or explicitly
  carved out in the doc.

- NEEDS-HUMAN [impl] — **"Reader-transparent … a GET during a DELETE is untorn exactly as
  before" (`0016:1694-1696`) is unwarranted for segmented objects: `seg:` records get no reader
  grace.** Byte reclamation is grace-delayed at the drain's orphan mark (`:735-740`), but the
  drain deletes the *naming records* as soon as their fragments are marked
  (`:1102-1110` — "when all marked: delete the records the payload names"), and segmented
  resolution is non-atomic (root read, then `scan("seg:<id>:<E>:")`, `:1180-1182`). Concrete
  failing case: a GET resolves the root of a 50 GiB segmented object; a concurrent
  `DeleteObject` installs `retire:bytes:{generation}` (X45); the drain marks the fragments
  (bytes retained for hours of grace) and then immediately deletes `seg:<id>:<E>:*`; the GET's
  next segment-record read finds it absent and the stream fails mid-object. Today this tear is
  impossible — the flat map is one atomic value the reader already holds, and only bytes need
  grace. The same race transiently aborts any maintenance consumer caught between its root read
  and its segment-range read. Not an (a)–(d) outcome (availability, not durability), but the
  compatibility claim is falsified and neither decision 7's failure-mode table nor the
  execution register has the row. Doc-level fix: give `seg:`-record deletion the same
  reader-grace delay as byte reclamation (the drain already sequences; delaying the record
  delete is one line of protocol), or specify a resolve-retry rule — and add the register row.

- NEEDS-HUMAN [impl] — **X47's "no moved fragment is left unreferenced-and-unevidenced
  (outcome (a) closed)" (`0016:1397`, echoed `:1316`) is refuted by the production repair
  order.** Reconstruction writes the rebuilt fragment to the destination server *before* the
  binding CAS (`crates/custodian/src/reconstruction.rs:556` `put_fragment`, commit at
  `:593-598`); on `Conflict` the whole batch — including its orphan puts — rolls back, and for
  a superseded generation the re-queued obligation is then *drained*, not retried
  (`reconstruction.rs:188-191`, `Assessment::Drain`). Concrete failing case (X47's own
  scenario): re-place fragment *i* of `seg:<id>:<E>:<j>` onto server T (bytes durably landed on
  T), the supersede advances the inode before the CAS, the repoint conflicts and is dropped
  per X47; the retirement drain orphan-marks only the pre-repoint placement it read, so the
  fragment on T is unreferenced *and* unevidenced — GC's conservative arm keeps it forever
  (`crates/custodian/src/gc.rs:183-187`), the exact "fourth category" invariant (2) forbids.
  The leak class pre-exists on the flat repoint path (out of scope), but the *claim* that this
  diff's X47 row closes outcome (a) for the seg: repoint is what the register asserts and what
  a confirmatory pass would wave through. Doc-level fix: pre-evidence the destination position
  (an orphan pre-mark the winning CAS deletes), or dispose the residue honestly as a bounded
  cost with a named reclaimer and the register row corrected.

- NEEDS-HUMAN — **The proposal's own ⚑ sign-off question must be adjudicated, not waved
  through** (`0016:1711-1728`): the enforced F11a bound puts a `sinf:` CAS in *every* part
  commit batch (`:391`, `:400`) and a session read-precondition on every per-chunk intent
  (`:390`), which bends D-C's "part commits stay counter-free" from literal to
  in-spirit-only. The doc flags it correctly per the brief (a direction it cannot honour
  literally is a flagged question, never a silent alternative); the human must rule on the
  serialization/read cost — or bless the in-spirit reading — at sign-off. Routing, not a
  refutation.

## Attempted and could not refute

The fence/epoch machine and the O(1) session-precondition publication proof; the
empty-`sidx:`-gated, exact-bytes-preconditioned terminal delete (exactly-once `mpuctl:count`
decrement under gateway/reaper races, X42); per-attempt epoch-scoped `seg:` keys against the
rollback→re-Complete-while-obligation-pending trace (X40); the byte-budgeted batch inventory
(no row exceeds `E_tx/2`; the iteration-2 fixed-count defect is genuinely closed, `0016:377-401`);
the restore fence incl. the pre-fence serve window (X17/X17b); the late-fragment cover via
full-`staged`-placement orphan marks plus the renewal refusal (X49, verified against
`crates/core/src/write.rs:474-500`); the fenced-intent freeze of the `sidx:` range (X43); the
session-record-first clock guard (X26/X46); `PendingEntry` serialization identity for both new
optional fields (verified `crates/core/src/metadata.rs:344-350` and the re-encode CAS paths);
the derived `MAX_SESSIONS = ⌊W_ref/U_ref⌋` distribution-independence and the in-range
(`MAX_PART_CHUNKS ≤ 381`) arithmetic; and the record-prefix disjointness (`mpu:` is not a
prefix of `mpuctl:`; no `scan` returns a neighbour). Iteration-4's six carry-forward items are
each addressed in the reworked text (the stale ≈52×SCAN_CAP passages are gone; the `scan_page`
seam is named in "What the implementing slices change").
