# Adversarial review — issue #813 (663.1, staged scrub + keep)

Method: re-ran the patch's own suite on a writable copy of `$PDCA_TARGET`
(`cargo test --offline -p wyrd-custodian`, all green), then wrote four probe tests against the
production entry points to try to break the fix. Three probes went red and one mutant survived
the whole crate suite. The scratch tree was deleted afterwards.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/scrub.rs:144`: scrub reads the committed
  namespace before the `part:` records, so a publication landing between the two reads hides a
  lost fragment from both classes and the pass still answers `Satisfied`.** Probe (production
  `reconcile_step` + the `Meta::hook` leg H already uses): an `Completing` session, one `part:`
  record naming chunk `0x9001` on D server 2, that fragment lost, and a hook that commits the
  publication batch (inode + dirent + session→`Completed` + `delete part:`) right after the
  pass's first `inode:` read returns. Result: `outcome=Satisfied repairs=0
  hook=[Some(Committed)]` — the corrupt/absent fragment was never fetched and the pass certified
  the store. Moving line 144 above `let referenced = referenced_fragments(...)` at
  `scrub.rs:104` turns the probe green and leaves the entire `wyrd-custodian` suite green (I ran
  both). The inline justification at `scrub.rs:139-143` — "reading this class before or after
  the committed one above costs nothing here … scrub only ever adds a check, never removes a
  protection, so it carries no race to guard" — is the unwarranted claim: 0016 states the
  opposite in the lines the brief cites, "**The rule is general, not local to this build:**
  wherever one batch atomically moves a fact from one key range to another, every reader of both
  ranges reads the source first", and names publication as the third instance
  (`0016:782-800`). The sibling pass in this same patch honours it and ships leg H
  (`staged_protection.rs:2463`) to prove it; scrub gets the same handoff and no guard. (Three of
  the three T4 review passes flagged this independently — this bullet adds the executable repro
  and the one-line fix.)

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1584`: `read_staged_part` runs a part
  record's placement through `ChunkRef::checked_fragments()`, which calls an EMPTY vector valid
  and identity-fills it, so a damaged `part:` record makes scrub check fabricated locations and
  enqueue a phantom repair.** Probe: a decodable `part:` record whose chunk carries
  `"placement":[]` (the wire field is required but an empty array decodes — `multipart.rs:2414`
  says length is deliberately unchecked), with the chunk's real, intact fragment on D server 3.
  Result: `outcome=Changed repair_queued=true` — scrub asked identity server 0, got nothing, and
  called an intact chunk lost. GC's own reader over the *same record* does the opposite:
  `StagedSet::place` (`gc.rs:1410-1429`) requires `placement.len() == fragment_count()` and
  otherwise *holds* the chunk as an untrusted staged record. Identity placement is a pre-M3
  legacy affordance for `inode:` records; 0016's own Backfill row says part records "are born
  with an explicit full-length placement written by the current write path", so an empty one in
  a `part:` record can only be corruption. With the reconstruction half of this patch the
  phantom obligation is then never drained (a staged record names the chunk), so the pass
  answers `Blocked` forever. This also falsifies the surviving comment at `scrub.rs:214-222`
  ("an `Ok(None)` here can only mean genuine loss"), which still reasons only about the
  committed set. The branch is untested either way — `C4-diff-cov` MISSes `gc.rs:1599-1601` (the
  malformed arm) and `scrub.rs:149-150` (its emit).

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/staged_scrub.rs:511-514`: the "wrong EC scheme"
  leg does not prove what it says it proves.** Its doc claims "This proves the PART record's
  scheme is what scrub checks", but every part record in the file declares `EcScheme::None`
  (`chunk_ref(..., EcScheme::None, ...)` at every seed site), so the assertion cannot separate
  "the record's scheme" from a constant. I replaced `chunk.scheme` with a hardcoded
  `EcScheme::None` at `gc.rs:1595` and **all 7 tests in the new file passed, and so did the
  entire `wyrd-custodian` suite** (25 test binaries, 0 failures). The brief's leg A and 0016's
  scrub row both turn on "using the scheme recorded in the part record"; nothing in the bundle
  covers it. One discriminating case closes it: a part record declaring `ReedSolomon{k,m}` with
  matching intact fragments — green only if the record's own scheme is used.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:211`: the unreadable-staged-record
  attribution is emitted after `read_committed(...).await?`, so an `inode:` store fault throws
  away the names of the records a human has to repair.** The comment directly above the emit
  loop (`reconstruction.rs:214-216`) says "Attributed the moment the staged reading returns,
  before any later store read, exactly as GC attributes it (`gc.rs:420-425`)" — GC really does
  emit between its two reads (`gc.rs:423-425`), this code does not. Probe: one undecodable
  `part:` record, one queued obligation, `fail_reads_of(b"inode:")`. Result:
  `err=true named=false` — the pass returned `Err` and the damaged `part:` key never reached
  `wyrd.custodian.reconstruction.audit`. Fix is moving lines 217-219 into the `else` branch,
  between the two reads.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/scrub.rs:144` + `crates/custodian/src/reconstruction.rs:823`:
  the two halves together make scrub flag a fully intact, published chunk as lost for the whole
  retirement window.** Probe: a `Completed` session whose `part:` record has not been retired yet
  and still names the pre-repair placement (server 0), plus the repointed committed map naming
  server 1 where the intact fragment actually sits. Result: `outcome=Changed
  repairs=[repair:37377]` — scrub checked the stale staged location independently of the
  committed map and enqueued. Reconstruction then finds `missing.is_empty()` and drains
  (`reconstruction.rs:823`), so every pass repeats the enqueue→drain cycle and emits a false
  `fragment missing` durability signal until the `retire:records:{parts}` drain lands. This
  patch makes the window likelier rather than rarer: the keep-obligation half holds a staged
  chunk's repair queued until publication, which is exactly when the repair runs and repoints
  it. Deciding what scrub should do when the two classes disagree about one chunk's placement
  (prefer the committed map for a chunk that has one, or skip staged entries for
  already-committed chunks) is a small change, but it is a decision the brief does not make.

- **NEEDS-HUMAN [human] — `crates/server/src/custodian.rs:505` and
  `crates/custodian/src/reconstruction.rs:111-116`: the new `clock` seam is filled from a
  different time source than the pass it belongs to, and both doc claims about it are false.**
  `run_reconstruction_until` builds its own `let clock_seam = wyrd_testkit::SystemClock;` and
  says it is "the same clock this loop's own `clock` closure already advances" — but that
  closure is a caller-supplied `FnMut() -> u64`, and `custodian_day_one.rs:1093-1097` passes a
  logical counter starting at 500. So `ctx.clock` (wall) and the pass's `now_millis` (logical)
  are two sources inside one lifecycle, which is the `#557`/`#565` class the rubric's first hard
  convention (ADR-0009, "one clock per correctness lifecycle") forbids. The same split appears
  at every fill site: `staged_protection.rs:518` pairs `SystemClock` with `NOW`, and
  `dst/tests/custodian.rs:652,706,828,1032,1107,1214,1325` pair it with literal `now_millis`
  values (200, …). The field doc also claims "every construction site in this crate's tests
  [fills it] from a `wyrd_testkit::Clock` double" — they all pass `SystemClock`, the production
  wall-clock arm, not `ManualClock`. Nothing reads the field in this slice, so no behaviour is
  wrong today; the cost lands on #814, whose whole purpose for the seam is to compare it against
  a pre-mark stamped from the pass's own time. Fixing it properly means `run_reconstruction_until`
  taking the `Clock` seam and deriving `now_millis` from it (a signature change reaching
  `cli.rs:1476-1519`) — an architectural call, not a rebuild-and-go, which is why this is tagged
  `[human]`.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:1262`: `emit_staged` fires
  once per chunk per pass and these obligations persist for the life of an upload.** The sibling
  `emit_refused` right above it is deliberately "once per **object**, not once per chunk — two
  obligations inside one segmented object are one refusal" (`reconstruction.rs:1237-1238`). A
  staged obligation is kept, by design, until publication (hours, per the brief), so every pass
  re-emits one warn plus one `reconstruction_kept_staged` increment for every staged chunk in
  the backlog — a D-server loss during a wave of open uploads reproduces the log volume the
  `emit_refused` rule exists to avoid.

## Attacked and could not refute

- **The C4-verify red→green is real.** I re-ran the green side (7/7 pass on the patched tree),
  and the frozen `gate-logs/C4-verify.log` red side shows five distinct assertion failures with
  real values (`Satisfied` vs `Changed`/`Blocked`), not compile or harness noise. The legs drive
  the production `reconcile_step`/`ScrubContext`, seed every record through the real
  `decode_session_record`/`decode_part_record`/`decode_owned_entry` and assert on
  `metadata::encode` round-trip identity, and the intact-fragment control rules out "queues
  everything". Leg B being green on base is stated in the brief, not concealed.
- **Aborted uploads do not produce phantom findings.** I expected scrub to flag part-named
  fragments while an abort's `retire:bytes:` drain reclaimed them; GC's reclaim gate consults
  `staged.protection` first (`gc.rs:653-657`), so those bytes cannot be reclaimed while the
  record that names them exists. No window.
- **`staged_committed_parts`' paging (`gc.rs:1552-1571`) is a faithful copy of
  `staged_fragments`' (`gc.rs:1474-1497`)** — same `staged_page`/`walk_staged_range` bound, same
  cursor advance, same containment of an unparsable session key. No new unbounded scan.
- **The committed half of scrub's new merge (`scrub.rs:170-181`) changes nothing measurable**:
  `ReferenceSet::placed` is a `HashSet` (`gc.rs:1097`), so moving from a per-server `Vec` push to
  a `HashMap` insert cannot drop or duplicate a `scrubbed` coverage emission.
- **Legs G–I discriminate what they claim.** I could not re-run their red (the brief's posture
  puts them in a reverted file), but leg H's hook shape genuinely separates the two read orders —
  I built the mirror-image probe for scrub with the same fixture and it is red, which is only
  possible because the hook fires between the two reads as advertised.
