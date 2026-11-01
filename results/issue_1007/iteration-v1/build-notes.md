# Build notes — #1007 (636.5a): the Complete fence, its release, and a budget client errors never spend

Base: `getwyrd/wyrd` `main` at `376b106` (the worktree's HEAD). `patch.diff` applies cleanly to it
(`git apply --cached --check`). Line numbers below are on the **patched** tree unless marked "base".

The brief's citations were taken at `4bac538`; on `376b106` several moved. Base line numbers I
used instead: `SessionState` `multipart.rs:2198` (brief `:2173`), `PublishTarget` `:2144`
(`:2119`), `SessionRecord::open` `:2385` (`:2360`), `complete_fingerprint` `:4486` (`:4423`),
`InvalidPart` `:4508` (`:4445`), `complete_answer` `:4845` (`:4782`), `MAX_COMPLETE_ATTEMPTS`
`:5279` (`:5200`). `part_key` `:1321`, `psum_key` `:1336` and `require_bucket` `create.rs:700`
were unchanged.

## What changed

| File | Change |
|---|---|
| `crates/core/src/multipart/complete.rs` (new) | The fence phase: `fence_complete` (`:501`), the request (`CompleteRequest` `:145`, `NamedParts` `:125`), the held fence (`HeldFence` `:183`), the two-arm error (`CompleteError` `:250`: `InDoubt` `:260` with `DoubtfulCommit` `:275`, and `Stopped` `:292` with `FenceState` `:303` and `Stop` `:322`), the round bounds `FENCE_ROUNDS` `:99` and `RELEASE_ROUNDS` `:110`, validation (`validate` `:666`, `read_part_set` `:696`) and the release (`release` `:751`, `moved` `:804`). |
| `crates/core/src/multipart.rs` | `pub mod complete;` (`:159`); two crate-private transitions modelled on `SessionRecord::open`: `complete_fence` (`:2626`, `Open@E → Completing@E+1`, `attempts + 1`) and `fence_released` (`:2655`, `Completing@E → Open@E+1`, `attempts` as given); header and `SessionRecord` doc updated to name the new writer. |
| `crates/core/src/multipart/abort.rs` | Doc only, `ABORT_ROUNDS` (`:48-60`). Its old argument ("a session spends at most `MAX_COMPLETE_ATTEMPTS` Complete fences") stops being true once a rejected Complete gives its attempt back, so I corrected the sentence. The value and the behaviour are unchanged. |
| `docs/design/architecture/05-building-block-view.md` | One new paragraph (`:248`) on the fence, as `AGENTS.md`'s docs-currency rule requires: a rejected request releases its fence without spending the budget, and nothing publishes yet. |
| `crates/core/tests/multipart_complete.rs` (new) | The test the brief names: 16 tests, every async leg under a 120 s watchdog (`within`, `:78`). |

No production caller is added; `fence_complete` is reachable only from the test (brief, Production reach (c)). No existing production path changes (d): the only edit to an existing `.rs` body is the two new crate-private methods.

## Design decisions, and what I ruled out

**1. Pre-validation before the fence: kept, as an optimization only.** The brief allows it. `fence_complete` calls the same `validate` before the fence commit (`complete.rs:547`) and again behind it (`:603`); only the second one decides. I kept it because without it every client typo costs a fence and a release (two commits, the epoch +2), and every in-flight `UploadPart` on that session dies: staging's intents require the session on the exact bytes they were fenced on (`05-building-block-view.md:246`). S3 answers `InvalidPart` without disturbing in-flight uploads. Cost of keeping it: one extra bucket `get` and one extra walk of the `psum:` range per valid request (at most 79 pages of 128 at the shipped 10,000-part cap). D2 proves the budget does not rest on it: there the part is replaced after the first check and before the fence commit, and the refund is what keeps `attempts` at 0.

**2. The frozen set is the `psum:` range, not `part:`.** A summary is written in the same batch as its part (`0016:660`) and holds the digest, which is all validation needs. It also holds each part's chunk count, so #1011 can apply the value ceiling before reading any chunk list. Reading `part:` instead would load every chunk list of the session: up to `MAX_STAGED_CHUNKS` = 80,896 refs × `MAX_CHUNKREF_BYTES` = 315 B ≈ 25.5 MB per request, against about 10,000 × ~150 B ≈ 1.5 MB for the summaries. The brief's Scope says "the bounded read of the frozen `psum:` / `part:` ranges". I read only `psum:`, and the module doc (`complete.rs:36-40`) says why. If the human wants `part:` presence checked too, that belongs with #1008, which reads the named `part:` records anyway.

**3. A store fault behind the fence is not released.** Only a *rejection* (invalid list, bucket gone) releases. A read fault or a torn `psum:` record behind the fence returns `Stopped { fence: Stands { epoch } }` (`complete.rs:607`). C-1 allows that: only an error may leave the Complete's own fence standing. The cost is the one 0016 already accepts for a crashed completer, a wait of up to `W_completing` (`0016:956-962`). Rejected alternative: release on a fault too. It needs a producer for a "released" fence state, plus a second cause in `Stop` for when that release fails as well (roughly +40 lines and two more test legs). The store just failed, so that release is the commit most likely to fail too. If the human prefers it, it fits in #1008, which adds the "released" state anyway for (J).

**4. `FenceState` has `NotPlaced`, `Stands { epoch }` and `Gone { epoch }`, and no `Released`.** The brief says #1008 uses a "released" case for (J). Nothing in this slice produces one, so I left it for #1008 to add with its producer. The enums are not `#[non_exhaustive]`, so the new variant will break every match at compile time (the repo's convention, `multipart.rs:4501-4504` base). `Gone` covers one rare path: a release `Conflict`, then a re-read finds another writer moved the session and the record does not decode. The fence no longer stands, and the Complete did not release it.

**5. Two more checks the brief does not spell out:**
- *Target.* The request carries `parent`/`object`. A session whose target differs answers `NoSuchUpload` (`complete.rs:618`). Without this the request's key would be ignored, and #1008 would publish the session's key while the client asked to complete another. S3 addresses an upload by bucket, key and upload id.
- *Clock source.* 0016 says every operation that writes a session-scoped timestamp MUST first check its clock source against the session's (`0016:1966-1976`, normative). The fence writes `fenced_at_millis`, so the request carries `clock_source`, and a mismatch is `Stopped { NotPlaced, ForeignClock }` with zero commits (`complete.rs:526`). I made it an error rather than a `Refusal` because 0016 maps it to a 503 with an alarm. A new `Refusal` variant would also break the exhaustive matches in `crates/core/tests/multipart_state_machine.rs:772` and the gateway mapping. This is also how the rubric's "a new clock read states which source owns its lifecycle" is met: core reads no clock, and the stamp's owner is the session's residency clock (module doc `complete.rs:64-71`).

**6. The release is minted together with the fence** (`complete.rs:541`): the fence is computed only if its release `Open@E+2` is also representable. A fence at `u64::MAX - 1` that could never be released would wedge the session on a client error, so that case is `Stop::EpochExhausted` before anything is written. There is no unreachable branch later.

**7. Round bounds.** `FENCE_ROUNDS = RELEASE_ROUNDS = ABORT_ROUNDS` (4). The brief asked me to pick and cite one. The fence and the release contend on the same key with the same writers as Abort, so they get the same budget. As for Abort, the bound limits work, not correctness: running out is a typed `Stopped` error. D4 asserts exactly `RELEASE_ROUNDS` release commits.

**8. `NamedParts` refuses an empty list** (`complete.rs:125`). An empty list has no honest protocol answer (`InvalidPart` needs a part number), and S3 refuses it as malformed XML before it reaches a store. An out-of-order or duplicate list is still a value, answered `InvalidPart { OutOfOrder }` (D1). I wrote the order check as a small `windows(2)` scan (`out_of_order`, `:639`) instead of exposing `canonical_named_parts`. Mapping that function's `RecordError` would need a catch-all arm over a large enum, which the repo's no-`_ =>` convention rules out.

**9. Rubric "new concurrent path lands with seeded Tier-0 DST coverage".** The brief puts DST (#660) out of scope, so I left an in-code `// deferred: #660` marker (`complete.rs:444`), which the rubric's reviewer protocol treats as settled.

**Open question in the brief (visibility).** I kept the brief's proposal: `fence_complete`, `HeldFence`, the request and the error types are `pub`, so the integration test can drive the phase before the verb exists, and #1008 can narrow them. `HeldFence` has private fields, no `Clone`, and no constructor outside `fence_complete`, so code outside core cannot forge a held fence.

## How the test maps to the Success criterion

The wrapper `Logged` (`multipart_complete.rs:292`) logs every batch and, per commit, can: answer `Conflict` unapplied (`OnCommit::ConflictUnmoved`), answer `CommitUnknownResult` applied or not with either `may_still_commit` (`Unknown`), answer a definite `Err` (`Fail`), write a blind batch first (`WriteFirst`, used to re-plant a part, delete the bucket, or move the session). It can also fail reads under a prefix (`fail_reads_under`). `OnCommit` (`:255`) is the extension point the brief asks for, e.g. a later hold step. Sessions come from `create_session`. Parts are planted through `part_key`/`psum_key` with canonical bytes, checked by decoding and re-encoding them (`plant_part`, `:148`).

| Brief leg | Test |
|---|---|
| (E) `Completed`: identical → recorded publication; different → `NoSuchUpload`; zero commits | `e_a_completed_tombstone_answers_an_identical_retry_with_the_recorded_publication` (`:646`) |
| (E) another publisher's `Completing` → `OperationAborted`, record and `seg:` range byte-for-byte unchanged, zero commits | `e_another_publishers_completing_fence_is_refused_and_left_untouched` (`:676`) |
| (D1), (D2), and then a valid request fences at `Completing@E'+1` with `attempts` one higher | `d_typos_and_a_part_replaced_before_the_fence_never_spend_the_budget` (`:773`): 3 typos and 3 replacements on one session, then `Held` with `attempts` 0 → 1 |
| After a rejected request, Abort answers `Fenced` | `d_after_a_rejected_request_an_abort_fences` (`:872`) |
| (D4) rounds run out, and a definite store error | `d4_a_fence_it_owns_and_cannot_release_stops_with_the_fence_standing` (`:906`) |
| (H) bucket deleted | `h_a_bucket_deleted_while_the_session_is_open_answers_no_such_bucket` (`:977`): removed before the request (no fence), and by the wrapper just before the fence commit (fence, then release, `Open@2`, attempts 0) |
| (G0-fence) unknown on the fence (applied/not × both `may_still_commit`), on the release, failed first read | `g0_an_unknown_fence_outcome_…` (`:1014`), `g0_an_unknown_release_outcome_…` (`:1052`), `g0_a_failed_first_read_…` (`:1095`) |

The extra legs cover the remaining branches: the cap (`:1116`), a foreign clock (`:1136`), a fence CAS lost to a rival / to an unexplained conflict / until rounds run out (`:1164`), a release that finds the session moved to `Aborting` or rolled back to `Open` (`:1207`), a definite fence failure and a torn session (`:1251`), and the empty list (`:1295`).

## Running it

I used `timeout 1200 cargo test -p wyrd-core --test multipart_complete` in the worktree. That is the plain target the brief's Falsifiability names, wrapped in a timeout so a hang can't stall the beat. The project's gate runner (`./engine/xtask.sh ci`, i.e. `cargo xtask ci`) has no single-test subcommand, and C4-verify (`engine/scripts/run-verify.sh`) builds its own worktree off the live checkout, which is outside my roots. Final state: 16 passed.

What else I ran (all green): `cargo fmt --all -- --check`; `cargo clippy -p wyrd-core --tests` (workspace lints, `warnings = deny`); the whole `cargo test -p wyrd-core` (0 failures); `typos` (1.48.0, the brief's external dependency, installed); `python3 docs/publishing/tools/lint_docs.py`; `cargo xtask statics`; `cargo xtask ci-dry-run` (every repo-hygiene guard, including `forbid(unsafe_code)`); `RUSTFLAGS="--cfg madsim" cargo check -p wyrd-core --tests` in a scratch target dir, since `crates/dst` builds core that way. `cargo doc -p wyrd-core` shows no warnings in the changed files; the 23 it reports are all on the base. I did **not** run the whole `cargo xtask ci` (workspace build/test, DST seeds, conformance, deny); Check's C4-ci runs it. The change is additive inside `wyrd-core`: no other crate names the new module, and no existing enum gained a variant.

No git hooks are installed in the repo (`.git/hooks` has only samples) and there is no pre-commit config. The commit-time checks are the ones above.

## Demonstrated reds (required by the brief's Verification posture)

Each is a temporary edit to `complete.rs` (scripted, applied to a copy of the final file, run through the test target, then the good file restored and checked with `cmp`). All were re-run against the final shipped file.

| # | Temporary edit | Result |
|---|---|---|
| R1 | A rejected request counted against `attempts`: the pre-fence rejection falls through to the fence (no pre-validation), and the release keeps the charge (`fence_released(fenced.attempts())`), i.e. #636's shape | **D1 red**: `typo 0: no attempt spent` (left 1, right 0). H also red. |
| R2 | Pre-fence validation kept, release keeps the charge | **D2 red**: `the attempt is given back` (left 1, right 0); D1, earlier in the same test, passed. H also red. |
| R3 | The session left `Completing` after a rejection: `release` answers the rejection without committing | **D's Abort leg red**: `Ok(Refused(OperationAborted))`. D2, D4, H, G0-release and the moved-release leg also red. |
| R4 | A request meeting another publisher's `Completing` fence commits a release of it first | **(E) red**: `zero commits`. The rival-fence leg also red (`one fence, lost`). |
| R5a | A release that fails definitely (rounds spent, or a store error) answered with its refusal | **D4 red**: `expected the stopped error, got Ok(Answered(Refused(InvalidPart …)))` |
| R5b | The same, reported as in doubt | **D4 red**: `expected the stopped error, got Err(InDoubt(… commit: Release { epoch: 1 } …))` |
| R6 | An unknown initial fence treated as success (`break` as if `Committed`) | **G0-fence red**: `expected the in-doubt error, never an answer: got Ok(Held(…))` |

**Reverted base.** With `multipart.rs` and `abort.rs` reset to `376b106` and `complete.rs` removed (test kept), the test target does not compile: `error[E0432]: unresolved import wyrd_core::multipart::complete`. So C4-verify will report **UNVERIFIABLE (exit 77)**, as the brief pre-declares: no test runs on the red leg. The reds above are the evidence that each leg binds.

## Refuting my own test

- **(a) Genuine red?** Yes. With the fix fully reverted the test cannot build (above). With each of the six named defects put back one at a time, the leg the brief names goes red (R1–R6), each at the assertion that names the property, not by accident.
- **(b) Production path?** Yes. Every leg calls the production `wyrd_core::multipart::complete::fence_complete` over `RedbMetadataStore::in_memory()`, the real embedded backend, through a wrapper that only delegates or withholds. Sessions are made by the production `create_session`, the Abort leg uses the production `abort_session`, and records are checked with the production decoders. Nothing in the test re-implements the fence or the release.
- **(c) Fixture includes the fault?** Yes. Each fault is injected at the commit it targets, not curated out: D2/D4/G0-release re-plant a named part with a new digest just before the fence commit; H deletes the real `bucket:` key before the fence commit; D4 refuses every release with `Conflict` (and, in run 2, a definite `TransientFault`) while the fence really landed in redb; G0 withholds the outcome of a fence/release that was or was not applied to redb; the read leg fails the real first `mpu:` read. The E leg plants a real `Completing` record plus two real `seg:` records under its group, and compares their bytes before and after.

## Not done here (by the brief's scope)

Assembly and publication (#1008, #1009); the release's `retire:records:{seg}` obligation and the charging rule for fences that wrote segments (#1009); re-reads that settle an unknown outcome (#1010); the value ceiling (#1011); the reaper (#625), drain (#659), S3 verb (#670), DST (#660, marked in code). No edit to 0016, an ADR, `Cargo.lock` or `deny*.toml`.
