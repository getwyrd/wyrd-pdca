# Build notes — #842 (809.4): restore fences resurrected `Completing` sessions

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `022d76f` (origin/main + child-1
#839, child-2 #840, child-3 #841, plus unrelated integrated slices). Line numbers below are on that
base **with `patch.diff` applied**.

## What changed, and why

**Core — the `Completing` teardown** (`crates/core/src/multipart.rs`)

- `SessionRecord::completing_teardown` (`multipart.rs:2294`) mints, from a decoded `Completing@E`
  record: the same record at `Aborting@E+1`; `retire:bytes:s:<id>:<E>` = `{session, all}` (reusing
  child-3's `RetireObligation::session_teardown`, `:3648`); and `retire:records:s:<id>:<E>` =
  `{seg: (nonce, E)}` from the new private constructor `RetireObligation::attempt_segments`
  (`:3669`), whose token epoch is taken *from the group*, so key and payload cannot disagree with
  `checked_against_key`'s "seg epoch == token epoch" rule. `None` below `Completing` or at
  `u64::MAX` (`checked_add`), so no wrap and no panic (H(vi)).
- The group comes from child-2's `attempt_segment_group()` (nonce + `publish_target.epoch`), which
  decode already forces equal to the session epoch — so the records key is `<E>`.
- `{session, all}`, not `{session, parts}`: the human's Plan decision (option a). I re-measured the
  rejected shape: `{"session":true,"parts":[[1,1],[3,3],…,[19999,19999]]}` is exactly 128,916 bytes
  (> `MAX_VALUE_BYTES` = 100,000, `crates/core/src/metadata.rs:549`). `all` is sound because no part
  commits outside `Open` (`upload_part_answer`, `multipart.rs:4468`; `0016:1030`).
- Docs: the retire-rows table (`:3304`) now lists the restore `Completing` fence under
  `{session, all}` with the reason, and the `{session, parts}` row as the reaper's / operator abort's
  only; header, `SessionRecord`, `ALL_PARTS_COMPONENT`, `RetirePayload` and `RetireObligation` docs
  no longer say the `Open` teardown is the only writer route.

**Custodian — the fence** (`crates/custodian/src/restore.rs`)

- `plan_fence` (`restore.rs:882`) replaces child-3's `open_teardown` helper: `Open` → one
  obligation; `Completing` → both obligations plus the attempt to check; `Aborting` → re-check
  (`Plan::Fenced`); `Completed` → nothing; no next epoch → `EpochExhausted` (now said for both
  states). `SessionUnsettled::Completing` is removed — a decodable `Completing` session is never
  "not fenced because Completing" any more.
- `fence_session` (`:814`) builds ONE batch: `require(mpu, bytes read)`, `require_absent` on
  **every** obligation key (`:839`), the session put, every obligation put. On `Conflict` the cause
  re-read now checks each obligation key in order and names the first taken one (G-collision); an
  `Err` is still never read as a conflict.
- After the fence lands, `check_attempt` (`:965`) reads the frozen `seg:<nonce>:<E>:` range in
  bounded pages (`staged_page`), and only if it holds anything reads the session's `part:` range
  (`part_chunks`, `:1018`). The first record that (v) is not a segment key of the group, (iv) does
  not decode as a `SegmentRecord`, or (ii) names a chunk no decodable part holds, names the session
  in the new `RestoreReport::segments_unaccounted` (`:228`) and on the audit seam
  (`action=session-segments-unaccounted`, `:1495`). The session stays fenced.
- K: for an `Aborting@E'` session, `recheck_fenced` (`:926`) does one keyed read of
  `retire:records:s:<id>:<E'-1>` (only a `Completing → Aborting` fence files it beside
  `Aborting@E'`, per the brief), decodes it, and re-runs `check_attempt` on the group *that
  obligation names* (the range that will actually be deleted). An undecodable obligation names the
  session too (no silent skip). `// deferred: #659` (`:924`) marks the half-drained-range misread.
- The `session-fenced` audit line's `obligation` field now carries every obligation key the commit
  installed, space-separated (two for a `Completing` session); child-3's field name is kept.
- `needs_human()` includes `segments_unaccounted` (`:375`), so the exit status follows. The summary
  audit line carries its count (`:1556`). `fence_open_sessions` is renamed `fence_live_sessions`
  (`:781`) because it now fences `Completing` sessions too.

**CLI** (`crates/server/src/cli.rs`): the summary line counts the new list; a NEEDS-HUMAN paragraph
names each session with its first faulty record (`:1402`); `unsettled_causes` drops the
`Completing` slot and the "Open at the last epoch" wording (`:1481`). The agreement test gains a
`segments` report (`:3222`) so the paragraph ↔ exit-status pinning covers it.

**Child-3's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its leg H seeded a plain
`Completing@3` as "cannot be fenced". That is now false, so the fixture became `Completing@u64::MAX`
(still unfenceable, still named) — the same assertions hold.

**Docs**: `06-runtime-view.md:65` (§6.5 fence paragraph) and the m4 blueprint's step 7
(`m4-first-deployment-blueprint.md:633`, `:640`): `Completing` sessions are fenced, the second
obligation, why parts are owed as "all", the new SEGMENTS bill.

## The test — `crates/custodian/tests/restore_completing_fence.rs` (NEW)

Doubles live in the file (an in-memory `MetadataStore` with fail-on-put, a value-ceiling refusal
that rejects the whole batch like FDB's `2103 value_too_large`, and an applied-batch log; a
`ChunkStore`; an audit-seam capture). Legs: G (`:407`), G-atomic (`:430`), G-collision (`:455`),
G-sparse (`:495`), H (`:574`, cases i, ii, iv, v, vi via `seed_case` `:527`), K (`:601`),
Order (`:641`). It names only base symbols; new report fields are read through `Debug`.

### Red → green, through the project's runner

`PDCA_BUNDLE=… PDCA_LANE=1 PDCA_BASE=pdca-integration/r-a834e98f…/main ./engine/scripts/run-verify.sh`:

- GREEN (fix applied): 7 passed.
- RED (production reverted, test kept): **7 of 7 ran red, by assertion** (no compile failure):
  every `Completing` session is still `Completing@3` and named `cause: Completing`; G-atomic and
  Order get `Ok` instead of `Err`. H fails at case (ii) after (i) passes, as the brief predicts.
- `run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).`

### Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted every production file and kept the test: 7/7
  failed on assertions (output above). Plus a hand mutant (below) for G-collision.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_after_restore`, which
  runs the real `fence_session` → `SessionRecord::completing_teardown` → `MetadataStore::commit`.
  Only the store and disks are doubles; nothing in the fence is re-implemented in the test.
- **(c) Fixture includes the fault?** Yes. G-sparse's store really refuses an oversized value (the
  leg first commits a `MAX_VALUE_BYTES + 1` probe and asserts the refusal) and holds the real 10,000
  sparse parts; G-atomic really fails the commit putting each of the three keys; G-collision seeds
  a real, decodable foreign `{seg}` under the records key; H seeds the actual faulty records (a
  foreign chunk, a torn value, a stray key, a nonce-less record, `u64::MAX`).

### Hand mutants (each applied to the worktree, test run, file restored; patch re-checked identical)

| Mutant (in production) | Legs that went red (by assertion) |
|---|---|
| Blind put of the records obligation (`require_absent` only on the bytes key) | G-collision |
| Fence `Completing` without the `seg:` obligation (X57 left open) | G, G-atomic, G-collision, G-sparse, H, K (6 of 7) |
| Bytes obligation `{session, parts}` over the 10,000 sparse part numbers | G-sparse — the ceiling double refuses `value_too_large: Some(128916)` |
| Second pass skips the `Aborting` re-check (`recheck_fenced` not called) | K |
| `needs_human()` ignores `segments_unaccounted` | H, K |

These are the brief's three SELF-TEST cases plus two of my own (the #664-iteration-1 K bug, and
the exit-status link).

## Things I ruled out, with cost

- **Putting fenced-but-flagged sessions into `sessions_unsettled`** with new causes instead of a new
  field. Cost is not smaller: the CLI paragraph for that list says "could NOT be fenced … left each
  as it found it", so it would need splitting by cause anyway, and the field's doc ("each left as
  read, with no obligation") would become false. A separate field keeps both statements true.
- **Reading the `seg:` range before the commit.** A segment write requires `Completing@E`, so after
  the fence lands the range is frozen; reading it after the commit is the stable read, and the K
  re-check uses the same function.
- **Counting every faulty record.** The first version reported a count; I dropped it to stay inside
  the size budget — the report names the first faulty record by key, the operator inspects the range.
- **A core unit test for `completing_teardown`.** Written and green, then dropped for the size
  budget: leg G decodes both obligations against their keys through production, and G-sparse covers
  the ceiling. `attempt_segments`' doc points at leg G.

## Size budget

7 files (the cap). `patch.diff` is 81,791 bytes (79.9 KB by bytes/1024; 1,150 insertions, 95 deletions) — under the brief's 80 KB as the harness counts
it (`size_signal.py`: bytes/1024), but with little margin. The first complete version was 107 KB; I
cut it by compacting the test (same legs), dropping the core unit test and the per-record fault
count, and tightening docs. A review round that adds code will likely cross 80 KB.

## Downstream (not done here, per the brief)

0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`) still specify
`{session, parts}` and hit the same 100,000-byte ceiling for a sparse session. Flag to #656 and #659.
No edit to 0016 or any ADR. `#810` should depend on this child, not on #809.

## Commit-readiness

`cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server
--all-targets` clean (workspace lints, `-D warnings`). The full gate, `./engine/xtask.sh ci` (`cargo xtask ci` in the
worktree), passed: `xtask ci: all checks passed`, exit 0 — including `typos`, `lint_docs` and
`render_site --check`, so both brief-named external dependencies were present and exercised.

Scratch: I wrote a few small files under `$PDCA_SCRATCH` (`pdca-builder-842-*`: two code
fragments spliced into `restore.rs`, and the CI / verify logs). I did not delete them; the harness
owns cleanup of that root.
