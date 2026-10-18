# Build notes — issue 656, iteration 4 (multipart-admission-create-abort)

Target: `getwyrd/wyrd` `main` @ `7bf7eb9`, worktree `$PDCA_WORKTREE`. Line numbers below are in
the patched tree (new files) or the patched copy of an existing file.

This iteration starts from iteration 3's patch (`iteration-v3/patch.diff`, applied cleanly to
`7bf7eb9`) and fixes the one bug and two gaps the iteration-3 sign-off named. Shape, scope and
slice size are unchanged: the store functions in `crates/core/src/multipart/store.rs`, the shared
`fence_batch`, the one-line restore change, the DST scenario, and the docs. `Cargo.lock` and
`deny*.toml` are untouched, as the sign-off asked.

## What changed this iteration

### 1. R1 — Abort keeps the doubt of a fence that may still land (store.rs:728-814)

**The bug.** Abort kept "the last round's unknown outcome" in one slot. A later unknown with
`may_still_commit: false` overwrote an earlier `true`; a failing re-read after an unknown
returned the read's own fault (`?`); and a later plain store fault returned that fault. In all
three the Abort ended with an error that says no fence of it is pending, while a 1031 fence was
still in flight and could land after the Abort returned.

**The fix** — the same rule `create_session` already applies (`store.rs:548-550`):

- `doubt: Option<CommitUnknownResult>` (`store.rs:737`) holds the unknown outcome of a fence
  that requires the bytes the session held at the last read. A new unknown replaces it only if
  the held one is not `may_still_commit: true` (`store.rs:775-777`).
- A plain commit fault ends the Abort through `in_doubt_or` (`store.rs:771`, `:809-814`): the held
  doubt if it may still land, else the fault.
- A failed re-read (`store.rs:781-791`) ends the Abort with the doubt when this round's outcome
  was unknown (the read that was to settle it failed, so it is still unknown), or when the held
  doubt may still land; after a `Conflict` with only a settled out-of-flight doubt, the read's
  own fault is returned, as before.
- Only a read that finds the session **moved** clears the doubt (`store.rs:792-796`). This is
  the sign-off's "Conflict (session bytes moved) may still clear it", applied to every re-read:
  every fence requires the exact bytes it read, and the session's epoch only rises, so a fence
  for bytes the session has left can never land. A fence that did land shows as `Aborting` and is
  answered at the top of the loop.
- At the end of the rounds the Abort returns the doubt if any, else `RoundsExhausted`
  (`store.rs:753-758`).
- Rustdoc: a new `# A fence that may still land` section and a rewritten `# Errors`
  (`store.rs:705-727`); `AbortFault::RoundsExhausted` now says no fence of the Abort can land
  (`store.rs:652-656`).

**Behaviour changes beyond the three named cases** (all deliberate, each pinned by a test):
- A re-read that fails right after an out-of-flight unknown (1021) with nothing else in doubt now
  returns that `CommitUnknownResult { may_still_commit: false }` rather than the read fault: the
  fence's outcome is still unknown, and that error tells the caller a re-read settles it.
- If the re-read after an unknown finds the session moved to a new `Open` epoch, the old doubt
  is dropped (iteration 3 kept it), so the Abort can end `RoundsExhausted` rather than with a
  stale unknown outcome for a fence that cannot land.

**What I ruled out.**
- *Keeping a `BoxError` and re-boxing on every path.* `CommitUnknownResult` is `Clone`; holding
  the typed value (as Create does) lets the guards read `may_still_commit` directly. Re-boxing a
  clone gives the same `downcast_ref` behaviour to the caller.
- *A wrapper error carrying both the held doubt and the later fault.* That adds a new public
  error type the gateway (#668) would have to unpack, for no answer the client sees differently
  (an Abort error is a 500 either way; Abort is idempotent and the retry settles it). Create
  drops the settling look's own fault the same way (`store.rs:456-457`).
- *A settling look before returning, as Create's `settle` does.* The sign-off's test releases the
  held fence **after** Abort returns, so a look cannot find it; and Abort's contract is "a store
  fault is returned, never retried". Not added.

### 2. Create's keep-the-doubt rule now has a test (multipart_create_abort.rs:2765)

`f_a_batch_left_in_flight_stays_in_doubt_after_a_later_out_of_flight_outcome` — exactly the
sign-off's case: tiny profile (`max_sessions` 2, so a 2-attempt contention budget), commit 1
`Unknown::Late` (held, `may_still_commit: true`), commit 2 `Dropped { may_still_commit: false }`.
The create must end `Unsettled` with `may_still_commit: true` and
`ended: Refused(SlowDown { AdmissionContention { attempts: 2 } })`. Then the held batch is
released **after** the create returned and lands; `assert_admitted` shows that session admitted
and counted — what a contention refusal would have stranded. No production change for this item.

### 3. Rustdoc wording (store.rs:291-298, :421-429, multipart.rs:5129-5132)

The two passages the sign-off named said a contention refusal is "never a lost race" / "the
capacity bound". Both now say what `05-building-block-view.md:202` says: the budget is also
spent by an attempt whose outcome the store could not report, so a refusal means that, that many
times over, either the ledger moved or the store could not report an outcome — never ordinary
concurrency on a ledger with room. I added the same sentence to the paragraph this patch already
adds to `MAX_ADMISSION_CAS_ATTEMPTS` (`multipart.rs:5129-5132`), which made the same claim by
derivation. Doc only.

### Architecture doc (05-building-block-view.md:202)

Docs currency for item 1 (Abort's error behaviour is part of the operation): one sentence after
"refuses a `Completing`, `Completed` or unknown session with nothing written" says a fence's
unknown outcome is settled by re-reading, and an Abort that ends without an answer while a fence
may still land ends with that unknown outcome.

### Test harness additions (multipart_create_abort.rs)

- `Script::read_fault` (`:391`, armed at `:843`, fired at `:786`): fail the first point read
  after commit N with a plain fault.
- `Unknown::Held` / `Landing::OnRelease` (`:432`, `:451`): a batch held in flight until the test
  calls `Interposer::release` (`:709`), which applies any held batch and returns its outcome. The
  held batch is still held when the writer returns (asserted at the release site).
- `abort_ending_in_doubt` (`:2258`): one helper for every "Abort ends without an answer" leg —
  asserts the error kind and flag, that no fence of this Abort has landed when it returns, then
  releases the held fence and asserts whether it lands (session = `open_teardown`'s, the one
  obligation) or loses; count unchanged (E). A rival Abort's fence is accounted for.

New Abort tests (34 tests in the file now, was 29):
- `d_a_fence_that_may_still_land_outlives_later_out_of_flight_outcomes` (`:2335`) — the mixed
  1031 → 1021 run: a held fence at commit 1 (and, second leg, at commit 2 after a 1021), every
  other round 1021; must end `may_still_commit: true`; released after return, it lands.
- `d_a_failed_re_read_ends_the_abort_with_the_fence_still_in_doubt` (`:2367`) — five legs:
  re-read failing right after a 1031; after 1031 then 1021; after a lone 1021 (ends with that
  unknown, `false`); after a fence lost to a rival Abort while a 1031 is held (`true`; release
  loses); after a fence lost to a reopen with only a settled 1021 before it (the plain fault).
- `d_a_later_plain_fault_ends_the_abort_with_the_fence_still_in_doubt` (`:2452`) — 1031 then a
  fault; 1031, 1021, then a fault (both `true`, released fence lands); 1021 then a fault (the
  fault as it came).
- `d_a_session_seen_moved_clears_the_doubt_of_a_fence_that_can_no_longer_land` (`:2506`) — a
  held fence, then the session reopened under every later fence: `RoundsExhausted`; and the
  session moved under the held fence itself, then 1021s: the last 1021, `false`. Released, the
  held fence loses in both.

### DST scenario (crates/dst/tests/multipart_admission.rs)

The sign-off asked me to consider checking the flag and mixing codes. Done:

- `Nemesis` (`:148`, `impl MetadataStore` at `:208`) wraps the sim store. Under `Codes::Mixed`
  (`:133`) it re-arms the sim nemesis before each commit with 1021 or 1031 from a seed-derived
  ChaCha coin (salted, `:142`, so it draws apart from the sim's own RNG). It records, per writer
  (`mpu:` key + fence/create, `type Writer` `:158`), the codes its struck batches got, from the
  `CommitUnknownResult.code` the sim returns — so the count is of what really struck, not what
  was armed (concurrent commits re-arm between hops). State in struct fields, no `static`.
- Every exhausted Abort's `may_still_commit` is recorded (`AbortEnd::Exhausted`, `:290`). Under a
  single code it must equal the code's promise (`:368-375`). And a session every Abort of which
  ended `may_still_commit: false` must still be `Open` after `quiesce` (`:442-448`) — the store
  itself refutes a false "nothing in flight".
- New entry point `admission_invariants_hold_under_mixed_codes` (`:543`) and sweep
  `the_mixed_sweep_sees_a_writer_meet_1031_then_1021` (`:621`), which requires some batch left in
  flight, some Create and some Abort fence meeting 1031 then 1021, and some exhausted Abort.
  Measured over the 64 sweep seeds: 182 batches left in flight, 67 Creates and 24 Abort fences
  met 1031 then 1021, 6 Aborts exhausted (4 ended `true`, 2 `false`).

**What the DST does and does not catch (measured, not assumed).**
- The Create keep-the-doubt mutant (unconditional assignment at `store.rs:548-550`): **caught**
  by the new mixed leg — `admission_invariants_hold_under_mixed_codes` failed on the assertion
  now at `multipart_admission.rs:481-484` (line 475 in the copy that ran, before two later
  non-functional edits) "every session is one a Create answered Created for — none stranded,
  none lost" (a 1031 batch landed late, the mutant had dropped its doubt, re-minted on
  the resulting collision, and stranded the first session). The 1021/1031-only legs pass with the
  mutant; only the mixed leg reaches it.
- The iteration-3 Abort (the R1 bug): **not caught** by the DST — all 6 entries pass. The bug's
  window needs a 1031 fence to stay in flight through the Abort's remaining reads and commits,
  but the sim lands each in-flight batch on a fair coin at every store call by any task
  (`crates/dst/tests/support/mod.rs:608-628`), so it almost always lands (and is answered
  `AlreadyAborting`) before the Abort ends. The deterministic core tests above are the R1
  regression; the DST flag check is a sound invariant that would fire if the window were hit.

## Red → green evidence

Runner: `cargo test -p wyrd-core --test multipart_create_abort` (what C4-verify runs), under
`timeout 1500`. Green on the final tree: `test result: ok. 34 passed; 0 failed` (0.45 s). Each
red was a temporary edit to the fixed tree (scripted, from a scratch copy), then restored and
checked byte-identical with `cmp`.

1. **R1 — iteration-3 Abort body restored** (one `unknown: Option<BoxError>` slot, `?` on the
   re-read, plain fault returned as is): 31 passed, 3 failed —
   `d_a_fence_that_may_still_land_outlives_later_out_of_flight_outcomes`:
   `held at commit 1: … left: false, right: true`;
   `d_a_failed_re_read_ends_the_abort_with_the_fence_still_in_doubt` and
   `d_a_later_plain_fault_ends_the_abort_with_the_fence_still_in_doubt`:
   `expected a CommitUnknownResult, got: an injected store fault`. (The moved-session test passes
   on iteration 3's code: it pins the new clearing rule, not the old bug.)
2. **Create keep-the-doubt replaced by unconditional assignment** (`store.rs:548-550`): 33
   passed, 1 failed — `f_a_batch_left_in_flight_stays_in_doubt_after_a_later_out_of_flight_outcome`:
   `in doubt when the budget is spent: Refused(SlowDown { pressure: AdmissionContention { attempts: 2 } })`.
3. **Targeted mutations of the new Abort code**, each caught:
   - keep rule unconditional (`:775`) → 3 failed (`after 1031, then 1021 … left: false right: true`);
   - re-read guard `if false` (`:787`) → `d_a_failed_re_read…` failed;
   - `in_doubt_or` always the fault (`:811`) → `d_a_later_plain_fault…` failed;
   - `doubt = None` on a moved session deleted (`:796`) → `d_a_session_seen_moved…` failed:
     `left: None, right: Some(RoundsExhausted { rounds: 4 })`;
   - re-read guard `!lost && may` → `d_a_failed_re_read…` failed (the lone-1021 leg);
   - re-read guard `if true` → `d_a_failed_re_read…` failed:
     `after a lost fence, nothing in doubt` (got the unknown, wanted the plain fault).

The brief's three required reds, re-run on this iteration's tree:

4. **(A′) shared budget** (`contention_budget(local).min(MAX_UPLOAD_ID_ATTEMPTS)`; a plain
   replacement fails to compile on the now-unused helper under `deny(warnings)`): 6 failed, incl.
   `a_prime_every_creator_is_admitted_through_more_lost_cas_than_the_collision_budget`:
   `the create was refused: SlowDown { pressure: AdmissionContention { attempts: 2 } }`.
5. **(E) decrement on Abort** (the fence batch also `require`s `mpuctl` and puts it at
   `count - 1`): 8 failed on their (E) assertions, e.g.
   `d_abort_fences_an_open_session_with_its_obligation_in_one_batch`:
   `(E) the fence never moves the count; left: 0, right: 1`, and the new
   `held at commit 1: (E) left: 0 right: 1`.
6. **(F) re-mint on an unknown outcome** (`Step::Unknown` mints a new identity and drops the
   doubt): 9 failed, incl. `f_an_unknown_create_outcome_is_recovered_under_the_original_upload_id`:
   `F1 applied: exactly one session for this create: [UploadId("…10000000000000001"),
   UploadId("…10000000000000003")]; left: 2, right: 1`.

**DST** (`RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test
multipart_admission`, the flags `cargo xtask dst` uses): 6/6 pass on the final tree; Create
mutant red as above; iteration-3 Abort not caught (explained above).

## Refuting my own tests

- **(a) Genuine red?** Yes. Reverting R1 on its own turns the three R1 tests red (red 1);
  reverting Create's keep rule turns its test red (red 2); each guard of the new Abort code,
  broken alone, turns a test red (red 3); the brief's three reds still fire (4-6). The whole patch
  cannot be reverted with the test file in place (it names `create_session`, `abort_session`,
  `CreateFault`, which are not on `main`) — the pre-declared C4-verify UNVERIFIABLE.
- **(b) Production path?** Yes. Every leg calls the production `create_session` /
  `abort_session` / `fence_batch` over a real `RedbMetadataStore`. The interposer only orders,
  answers, fails or holds commits and reads; every forwarded and every released batch is applied
  by the real store, whose preconditions decide whether the held fence lands or loses.
- **(c) Fixture includes the fault?** Yes. The held fence is a real batch kept in flight and
  applied after the writer returned (the case the bug lies about); the failed read and plain fault
  are injected on the exact call the bug mishandled; the mixed-code leg injects both codes into
  one run. The DST arms the nemesis on every commit.

## Carry-forwards and decisions kept from earlier iterations

- **Bucket deletion fence → #511.** Create `require`s the bucket marker's exact bytes; ADR-0046
  §4's other half (`require_absent` on the deletion fence, decision 5) is deferred to #511, which
  must define the key. The one place it joins is marked in `create_batch`, `// deferred: #511`
  (`store.rs:569`). Nothing on `main` writes a fence key, and no deployment can create
  a session before #668.
- **"Metadata awaits lack a timeout"** — rejected by the human at the iteration-1 sign-off (core
  is runtime-free; the caller, #668, owns timeouts). All loops stay bounded.
- **cargo-deny advisories (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client)** — inherited
  from `main`; waiver goes via Act. `Cargo.lock` / `deny*.toml` untouched.
- **Restore's own fence** (`crates/custodian/src/restore.rs`) still classifies its outcomes its
  own way (`:1058-1089` on `main`); the sign-off did not name it, and it is outside this item.
  Only its batch construction is shared (`fence_batch`), unchanged this iteration.
- **Open question 2 (constructors).** No decoder was loosened.

## Gates I ran

- **The new test file**: `cargo test -p wyrd-core --test multipart_create_abort` —
  `34 passed; 0 failed`, about 0.5 s, on the final tree.
- **Full gate**, `./engine/xtask.sh ci` (= `cargo xtask ci`) on this worktree: `typos` ✓,
  `lint_docs.py` ✓, `render_site.py --check` ✓, gitlink / unsafe / blackbox guards ✓,
  `cargo fmt --check` ✓, `cargo clippy --workspace --exclude wyrd-dst --all-targets` ✓, build ✓,
  `cargo test --workspace --exclude wyrd-dst` ✓ (197 `test result: ok`, 0 failed),
  `cargo-machete` ✓, `cargo deny check` ✓, then
  **✗ `cargo deny --all-features --config deny-all-features.toml check advisories`** —
  RUSTSEC-2026-0346 / RUSTSEC-2026-0349 (rustls 0.21.12 via tikv-client), inherited from `main`
  exactly as in iterations 2 and 3. The steps after it, run one by one through the same wrapper:
  `./engine/xtask.sh statics` ✓ (no DST-reachable shared mutable global state — the DST wrapper
  and the test interposer keep their state in struct fields), `./engine/xtask.sh conformance` ✓
  (5 valid + 6 invalid vectors), `./engine/xtask.sh dst` ✓ — madsim clippy clean, then all 11 DST
  test files green at `MADSIM_TEST_NUM=50` (0 failed; `multipart_admission` 6/6; 1205 s of it the
  existing 33-test `custodian` file).
- After the gate runs I changed one doc comment in the test file
  (`abort_ending_in_doubt`'s, `:2254-2257`, no line-count change) and removed a temporary
  measurement print from the DST sweep before the DST run; I then re-ran `cargo fmt --check`,
  `typos`, `cargo clippy -p wyrd-core --all-targets` (0 warnings) and the test file (34/34) on the
  final tree.
- **Mutation check** (the C5 row's own script, `scripts/mutants-in-diff`, over this
  `patch.diff`, `TMPDIR` under `$PDCA_SCRATCH`): `79 mutants tested in 4m: 67 caught, 12
  unviable`, none missed.
- `patch.diff` applies cleanly to `7bf7eb9` (`git apply --cached --check` against a fresh index
  of the base tree). The repo has no commit hooks installed (`core.hooksPath` unset, only
  `.sample` hooks, no pre-commit / husky / lefthook config), so `cargo xtask ci`'s fmt / clippy /
  typos steps are the commit-time checks, and they pass.

## For sign-off

- **C4-verify will report UNVERIFIABLE** (exit 77), as the brief pre-declares: the new test file
  cannot compile on `main`. The reds above are the per-fix evidence.
- No external dependency was missing: `typos` and the docs renderer are installed.
- No production caller of Create or Abort was added; restore's change is unchanged from
  iteration 3.
- The DST does not catch the R1 bug by itself (see above); the deterministic core tests do.
