# Build notes — issue 657 / multipart-upload-part-staging (attempt 1)

Target: `getwyrd/wyrd` `main` @ `12dd082` (the base the brief verified; the worktree HEAD).
`patch.diff` is `git diff 12dd082` of the worktree plus `git diff --no-index /dev/null <file>` for
the two new files (the git index was not touched). `git apply --check` passes on a clean export of
`12dd082`, and `git apply --check -R` passes on the tree every result below was taken on. Every
`path:line` cites the **patched** tree.

Size: 5 files changed (+241 / −62) plus two new files, `upload_part.rs` (1,601 lines, about half
of it doc comments) and the test (2,088 lines after rustfmt, 23 tests). The brief's success
criterion has about twenty separate legs; I did not find a smaller test that covers them.

## What the patch does

| File | Change |
|---|---|
| `crates/core/src/multipart/upload_part.rs` (new) | `UploadPart`'s staging: `reserve_slot` (`:354`), `upload_part` (`:450`), and the attempt state machine behind them. |
| `crates/core/src/multipart.rs` | `pub mod upload_part;` (`:157`); writer-side constructors `SlotRecord::new` / `renewed` (`:2764`, `:2781`), `PartRecord::new` / `summary` (`:3010`, `:3032`), `RetireObligation::part_chunks` (`:4077`, the per-part `{chunks}` row the `RetirePayload` doc asked for); a streaming `DigestBuilder` (`:1117`) that `Digest::sha256` now uses, so the algorithm is still named in one place; docs (header, `OperationAborted` at `:4645`, the "no writer-side constructor" notes, the `OwnedEntry` minting note). |
| `crates/core/src/metadata.rs` | `DESIRED_DSERVER_PREFIX`, `desired_dserver_key`, `parse_desired_dserver_key` (`:99-113`): the one home of the drain-record key. |
| `crates/custodian/src/desired_state.rs` | uses the core key (`:28`, `:42`, `:171`); bytes and parse rule unchanged. The two `deferred: #657` notes are settled (`:218-226`, `:241-244`); `deferred: #665` stays. |
| `crates/core/src/write.rs` | `encode_chunk` made `pub(crate)` (`:139`) so staging encodes a chunk exactly as the write path does. |
| `docs/design/architecture/05-building-block-view.md` | the multipart records paragraph (`:244`) now says what the staging writes, the fences, the ceilings, and that nothing calls it until #669. |
| `crates/core/tests/multipart_upload_part.rs` (new) | the brief's test file. Copies are in the bundle at `multipart_upload_part.rs` and `crates/core/tests/multipart_upload_part.rs`. |

No production caller is added. The one production path touched is the custodian's use of the
`desired:dserver:` key; its bytes are pinned by `the_drain_fence_key_keeps_the_custodians_bytes`
and the custodian's own suite is green.

## How the staging is built (the decisions worth checking)

**One invariant carries all the unknown-outcome handling.** Every batch after the reserve requires
the attempt's own slot at exactly the bytes it last saw land, and every such batch either rewrites
the slot with a **strictly later** lease or deletes it (`next_lease`, `upload_part.rs:993`:
`max(t + ttl, current lease + 1)`). So of the batches that require one slot value, at most one can
ever land. A batch whose outcome was unknown is kept as a pending `Effect` (`:710`, `:737`); the
next read (`look`, `:1066`) recognises a landed one by what only it writes (`Look::shows`, `:806`):
an intent by its owned entry's exact bytes, a renewal by its slot bytes, the commit by its exact
`part:` bytes, a compensation by its obligation. That is the brief's "settle identity, not
presence" rule, and it is what kills the "a part is there ⇒ Committed" mutant.

I chose the strictly-later lease over reading an extra witness for renewals. Without it, an intent
at an unchanged clock reading could rewrite the slot to the same bytes, a held renewal requiring
the old bytes could then still land, and the bookkeeping would read that as `SlotLost`. The cost is
a lease that creeps by 1 ms per batch while the clock does not move.

**Classification after a send that did not land** (`judge`, `:1105`): first a pending batch found
landed; then the session (absent or not `Open` ⇒ `NoSuchUpload`, no compensation; `Open` at another
epoch ⇒ compensate under it, answer `OperationAborted`); then the slot (moved with no own batch to
explain it ⇒ a release whose evidence has moved on, else `SlotLost`); else `Unmoved`, and the step
judges the rest — the part key (`commit`, `:1494`), the drain records (`stage_chunk`, re-plan), or
a resend of the same batch.

**Held commit.** While a commit may still land, the attempt resends the identical commit (same
bytes, same slot precondition) and never compensates on a read alone. If its rounds run out it
returns `InDoubt` and sends nothing more (`Halt::Keep`, `:1467`), so the brief's run (iii) ends
with exactly the held part durable. A compensation at that point would race a part that probably
committed.

**The drain fence.** Each chunk reads the drain records (`draining`, `:1014`), plans against
`topology.excluding(&draining)` (`:1189`), and the intent `require_absent`s
`desired:dserver:<S>` for every planned server (`intent_batch`, `:1299`). An `Unmoved` conflict
re-reads the drains and plans again when a planned server now has one. Keys under the prefix that
do not parse are skipped: they name no server, and the fence requires each planned server's
canonical key, so skipping them cannot place a fragment on a server whose record exists (the
custodian's `draining_servers` skips them the same way). A reviewer may still raise this as a
"silent skip"; that is the answer.

**The deadline.** `t_auth` is read once per intent build, before the send (`:1192`); the deadline
`t_auth + W` (checked add, `ClockOverflow` on overflow) is stored on the staged chunk, and every
fragment of the intent that actually landed carries it (`:1283`). A re-plan reads a fresh `t_auth`;
a held intent that lands later keeps its own. `W` is a required `NonZeroU64`, so there is no
default and no `None`.

**Renewal** (`renew`, `:1315`) puts every owned entry with the new lease and the slot, under
`require(mpu == Open@E)` and `require(slot == prior)` only. I did not add `require(sidx == prior)`
per entry: while the session is `Open@E` nothing but this attempt deletes an owned entry, and its
commit and compensation delete the slot in the same batch, so the two preconditions already
exclude every deleter. Per-entry preconditions would add up to `MAX_PART_CHUNKS` (158) operations
to a batch already at 2 + 158 + 1, against `MAX_BATCH_OPS` = 500. Renewal runs when the clock
passes `renew_at` (half a TTL), and once more before the commit if the clock has moved since the
last stamp (the quiesce, `commit`, `:1394`).

**Ceilings.** Per part: refused before chunk `MAX_PART_CHUNKS + 1` is staged (`stage_chunk`,
`:1160`). Cumulative: `staged_elsewhere` (`:1375`) sums the session's other `psum:` summaries (a
re-upload's own number is left out), unfenced, as 0016 requires. Both refusals compensate first.

## Where I departed from the brief's wording, and why

1. **The reserve's give-up after ambiguous rounds is an error, not a refusal.** The brief's Design
   says "give up with a refusal". No `Backpressure` arm says "a slot of yours may yet appear", and
   `crates/gateway-core/src/lib.rs:489-510` mirrors core's `Backpressure` "arm for arm", so a new
   arm is a seam change in a crate outside this slice (two enums and their docs, about 10 lines
   across two crates). `InflightParts` would misreport the cause. I return
   `UploadPartError::InDoubt` while a reserve may still land, else `Stopped { RoundsSpent }`, as
   Abort does (`abort.rs:264-274`); `reserve_slot`'s doc says so (`upload_part.rs:349-353`). #669
   maps it to a 503.
2. **A compensated rival-loser, or a part whose session rolled back to a later `Open` epoch,
   answers `OperationAborted`.** The brief says "answers a refusal" without naming one.
   `OperationAborted` (S3: a conflicting operation is in progress, try again) is the closest
   existing arm; I extended its doc in core (`multipart.rs:4645-4650`). The gateway-core mirror's
   doc still describes only the Complete case; the S3 mapping is #669's.
3. **A session back at `Open@E'` during an intent or renewal** compensates and answers
   `OperationAborted`, the same rule the brief gives for the commit. Continuing under `E'` would
   also be sound; one rule for every step is easier to check.
4. **`MAX_PARTS_PER_SESSION` is not enforced here.** It is a capacity checked where a part number
   is admitted; the gateway seam already has `PartNumberOutOfRange`. The `psum:` scan stays inside
   `SCAN_CAP` either way (part numbers stop at 999,999).
5. **A fragment write failure** stops the attempt with `Stop::Fragments` after compensating. The
   brief does not name this case; leaving the slot held would wedge the session as a refusal would.

## Verification

### The named test, green

`timeout 900 cargo test -p wyrd-core --test multipart_upload_part` → `23 passed; 0 failed`.

I ran the test target directly, bounded by `timeout` (and every async leg by its own 120 s
`tokio::time::timeout`), because the project runner `./engine/xtask.sh` only exposes the whole gate
(`ci`, `conformance`, `dst`); the whole gate was run too, below. The test needs nothing beyond
`cargo test`: in-memory redb, an in-process chunk-store double, no display, no network.

### Demonstrated reds (each a temporary edit, test run, file restored)

A script applied each edit to a pristine copy of `upload_part.rs` (sha256 prefix
`566b282ac813ea10`, the final file), ran the named test with `--exact`, then restored the file and
compared it byte for byte (`cmp`: identical; same sha256 afterwards). Every run compiled and **ran**
the test, which failed at an assertion (`1 failed`), never at a compile error. (In a first round,
four edits hit `warnings = "deny"` compile errors; I rewrote them to compile without warnings
before counting any of them.) The reds below were re-run on the final, formatted tree after the
last gate run.

| Mutant | Edit | Test | Fails at (test file) |
|---|---|---|---|
| no drain fence | per-server `require_absent(desired:dserver:<S>)` replaced by one on an unused key | `b1_a_drain_recorded_just_before_the_intent_makes_it_plan_again` | `:1023` — one intent, not two: the first landed naming `S` |
| full topology | plan with `topology.clone()` instead of `excluding(&draining)` | `b2_a_server_drained_before_staging_is_never_planned` | `:1078` — the attempt errors: every plan picks `S`, every intent conflicts, rounds spent |
| renewal without slot | slot put dropped from the renewal batch | `d_every_renewal_rewrites_the_slot_with_the_owned_leases` | `:1332` — the attempt errors (its next batch requires slot bytes never written), before the batch-shape asserts are reached |
| blind compensation | `send`'s unknown arm skips the re-read and routes to compensation | `e_a_commit_that_landed_with_an_unknown_outcome_answers_committed_once` | `:1647` — a batch follows the landed commit |
| presence = committed | `Look::shows` for a commit tests `part.is_some()` | `e_a_re_upload_whose_commit_did_not_land_ends_with_its_own_part` | `:1693` — answered `Committed` with the prior part still stored |
| other index | after an ambiguous reserve, the pinned index is dropped and excluded | `c_an_ambiguous_reserve_retries_the_same_index_with_the_same_attempt` | `:1293` — the second send targets another index |
| deadline `None` | `put_fragment_at(…, None)` | `a_every_staged_write_carries_t_auth_plus_w_however_late_it_goes_out` | `:980` — `deadline == Some(t_auth + W)` |
| deadline `u64::MAX` | `Some(u64::MAX)` | same | `:980` |
| deadline at dispatch | `Some(now() + W)` at the put | same | `:980` |

### Refuting my own test

- **(a) Genuine red?** Yes, per objective: each of the nine reds above goes red with its one
  production edit made, and green again once restored. Reverting the whole patch makes the test
  file fail to compile (`wyrd_core::multipart::upload_part` does not exist on `12dd082`), so
  C4-verify will report UNVERIFIABLE (exit 77), as the brief's Verification posture predicts. That
  is the pre-declared sign-off item, not a gap.
- **(b) Production path?** Yes. Every leg calls the production `upload_part` / `reserve_slot` over a
  real `RedbMetadataStore::in_memory()`; the wrapper logs, delays, or injects at the commit seam and
  otherwise delegates every call to the real store. Sessions are made by the production Create,
  fences by the production `abort_session`, rivals by a second real `upload_part` on the inner
  store. The chunk store is a test double of the D server (the D-server side of `W_write` is
  #638's and out of scope); it implements the production `PlacementChunkStore` trait.
- **(c) Fixture includes the fault?** Yes. B1's drain record is written by the wrapper immediately
  before that intent commits, for a server that intent planned; B2's drained server is the one the
  selector prefers on the full topology (asserted in the fixture); B3 drains 4 and then 7 of 12
  domains; unknown outcomes, held batches and rival commits are injected on the real store; the
  deadline leg advances the clock between the intent commit and the fragment dispatch.

### The whole gate

`./engine/xtask.sh ci` (the project runner, `cargo xtask ci` in `$PDCA_WORKTREE`), run twice: once
before three doc-comment edits, once on the final tree. Both ended `xtask ci: all checks passed`,
exit 0. The final run covered `typos`, `lint_docs: OK`, `render_site … link audit OK` (99 pages),
the gitlink, unsafe and blackbox guards, `cargo fmt --all -- --check`, clippy (`-D warnings`
through `warnings = "deny"`), build, `cargo test --workspace --exclude wyrd-dst` (no failing
`test result` line), cargo-machete, cargo-deny (advisories, bans, licenses, sources; also the
all-features advisories run), the conformance vectors, the ADR-0035 statics gate, the deploy
guard, and `wyrd-dst` clippy and tests under `--cfg madsim`.

### Commit-readiness

`cargo fmt --all` was run over the tree after the last edit, and the gate's
`cargo fmt --all -- --check`, clippy, `typos` and the docs checks passed on that tree.

## External dependencies

`typos` (1.48.0) and the docs renderer were present and ran inside the gate. Nothing else was
needed: no NEEDS-HUMAN dependency.

## For the human at sign-off

- C4-verify will be UNVERIFIABLE (net-new API), as the brief pre-declared; the nine reds above are
  the per-objective evidence.
- Deviation 1 (reserve give-up as an error) and deviation 2 (`OperationAborted` as the loser's
  answer) are the two calls a reviewer is most likely to question.
- Seeded Tier-0 DST coverage of these races is marked `deferred: #660` in the module doc
  (`upload_part.rs:21-24`), per the brief's scope.
