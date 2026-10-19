# Build notes — issue #768, last-modified-never-decreases (iteration 6)

Builder: Do leaf, attempt 6. The iteration-5 carry-forward says the production fix is correct and
the rejection is for test gaps only, and lists three items plus "keep the additions tight". This
attempt starts from `iteration-v5/patch.diff` (applies cleanly to the base, `git apply --check`),
changes **no production code**, and addresses the three items in the two test surfaces. The
brief's design and rule are unchanged: the stored `+1 ms` rule with no cap and no heal, the typed
refusal of an unstamped overwrite of a stamped version, the far-ahead warning, and the S3
shown-time cap at the response's `Date`.

## Base and precondition

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, detached at `8ebd1c0` (the same
  base iteration 5 built on; it is `origin/main` and carries #898/#767, #899/#671, #908/#894 and
  #909/#895, so every "Conflicts with" slice is merged underneath).
- **Waiver precondition met:** `git merge-base --is-ancestor 0674f4d HEAD` succeeds and both ids
  are in `deny-all-features.toml:160` (RUSTSEC-2026-0346) and `:167` (RUSTSEC-2026-0349). No stop.
- Citations below are `path:line` on the patched tree.

## What changed since iteration 5 (test-only)

`patch.diff` differs from `iteration-v5/patch.diff` in two files only; the hunks for
`crates/core/src/metadata.rs`, `crates/core/src/write.rs` and
`docs/design/architecture/05-building-block-view.md` are byte-identical to v5 (checked by
extracting each file's hunks from both patches and diffing them, `index` lines dropped). In
`crates/gateway-s3/src/lib.rs` the only delta is one new unit test (`:5409-5441`); the
production hunks are unchanged. Patch size 139354 → 142778 bytes (+3.4 KB, all test).

### 1. Every await in the stored-value legs (1)–(6) is bounded (the gating T4 finding)

`bounded` moved up to sit beside `DEADLINE` (`crates/server/tests/last_modified_monotonic.rs:125`
the constant, `:127-133` the helper, whose doc now says every helper that awaits the store, the
gateway or the socket goes through it). Each helper the stored-value legs call now wraps its own
awaits, so a caller cannot reach the store or the gateway unbounded:

- `seed_file` `:191-204` — `write::intent`, `write::write_fragments`, `write::commit_create` and
  `write::release` under one `bounded("seeding {name}")`;
- `put` `:224-241` — both PUT paths (`put_object` / `put_object_streaming`);
- `stored` `:246` — `head_object`; the `get_object` read-back `:276`;
- `seed_record` `:379-385` — the raw `store.commit`; `record_bytes` `:391` — `store.get`;
- `overwrite` `:412-435` — both core commits (`commit_chunk_map_superseding` /
  `commit_chunk_map_superseding_leased`) under one `bounded("the {api:?} commit")`;
- `seed_bucket` `:612-617` — the bucket-marker commit. `start_s3` (`:636-648`) lost its outer
  `bounded("seeding the store", ..)` wrapper, since the two helpers it calls now bound themselves;
  the listener bind (`:655`) and every wire exchange (`send`, `:895`) were already bounded.

The finding was at v5's `:183` (`seed_file`'s `write_fragments` await), now `:193` inside the
wrapper. The 30 s `DEADLINE` is the same bound the wire legs used; all 9 tests finish in 0.12 s.
`tokio::time::timeout` cancels the inner future on expiry, so a stalled store op fails the test
with the helper's name instead of hanging the suite.

### 2. The ranged-GET cap is pinned on the real wire (leg 7)

`check_wire` (the checks run both before and after the overwrite of `ahead`) now also drives
`serve_get`'s ranged arm (`crates/gateway-s3/src/lib.rs:2444-2473`, the cap at `:2451-2452`):

- a plain ranged GET `Range: bytes=0-3` of the future-stored `ahead` answers `206` with its
  `Last-Modified` in the window `before ≤ shown ≤ Date ≤ after` and `Content-Range:
  bytes 0-3/17` (`last_modified_monotonic.rs:1022-1038`);
- the same ranged GET with `If-Unmodified-Since: probe` answers `206`, not `412`
  (`:1063-1076`);
- with `If-Modified-Since: probe` it answers `304`, whose own `Last-Modified` is in its window
  (`:1086-1094`).

The module doc's leg-7 line says so (`:43-48`). I put this in leg (7) rather than the
router-level test because the carry-forward offered either, and the wire leg is where the
unranged probes already live; the diff-coverage runner measures the server test, so `:2452`
is executed by the per-fix test. Ten requests per `check_wire` call instead of seven.

### 3. The handler-written `Date` on `304`, and the HEAD precondition branch

New gateway unit test `a_304_for_a_future_stored_time_shows_its_own_date_on_get_and_head`
(`crates/gateway-s3/src/lib.rs:5409-5441`): a stored time of `u64::MAX - 1` through the real
router (`signed_request_through_router`, `:5269`), GET and HEAD each with `If-Modified-Since`
set to a minute past the test's clock (`:5422`, so at or after the shown time → `304`), asserting
`304`, then `assert_shown_as_the_clock` (`:5350`): `Last-Modified == Date`, a well-formed
IMF-fixdate, between the clock readings before and after. Through `oneshot` no hyper is in the
path, so the `Date` it finds is the handler's own (`not_modified_response`, `:2745-2747`), which
is the pin the carry-forward asked for. The HEAD arm runs `serve_head`'s precondition branch
(`:2536-2545`), the line the v5 diff-coverage log listed as its one production MISS (`:2543`).

## Refuting my own tests (forced)

- **(a) Genuine red? Yes, twice over.**
  - Whole file through the project's runner: `engine/scripts/run-verify.sh` (lane 1,
    `PDCA_BRIEF_BASE=origin/main`, `PDCA_BUNDLE` = this bundle) on the final `patch.diff`:
    `GREEN` 9 passed; `RED` (production reverted, test kept) 4 passed, 5 failed by assertion;
    `PASS — red without the fix, green with it (9 test(s) ran red)`, exit 0. The red failures:
    (1)/(2)/(6) `the stored time must step exactly 1 ms past the prior, never back to this
    host's clock` (seed `1792371303500` an hour ahead, `1792540503500` two days ahead); (5) both
    refusals and the no-reset chain on both APIs (`must be refused with a typed error, not
    answer Committed (stored modified now None)`) plus the equal/ahead/far-ahead cases; (7)
    `HEAD ahead: the shown time 1792371303 ... must lie between the clock just before the
    request (1792367704) and the response's own Date (1792367704)`. (3), (4), (8) pass on the
    base by design. Log: `$PDCA_SCRATCH/pdca-builder-768-verify-v6.log`.
  - The three **new** assertions, each against a deliberate one-line production break (the
    gateway file saved first and restored byte-for-byte after, `cmp` clean, 0 markers left):
    - R1, `not_modified_response` writes no `Date`: the new gateway test fails at
      `header_text`, `the response carries date` (GET arm).
    - R2, the ranged GET arm skips `shown_modified`: leg (7) fails at the first ranged GET,
      `before the overwrite: ranged GET ahead: the shown time 1792371237 (Mon, 19 Oct 2026
      00:53:57 GMT) must lie between the clock just before the request (1792367637) and the
      response's own Date (1792367637)`; the unranged `200` just before it passed, so the
      ranged pin is its own.
    - R3, `serve_head` evaluates the preconditions against an empty header map: the new gateway
      test fails on its HEAD arm, `HEAD If-Modified-Since ...: left: 200, right: 304` (R1 and R2
      restored first, so this failure is R3's alone; leg (7)'s own `If-Unmodified-Since` HEAD
      still answers `200` under R3, which is why the carry-forward wanted the router-level
      HEAD + `If-Modified-Since`).
  - Item 1 has no behavioural red: a timeout that never fires looks like no timeout. Its
    evidence is structural: every store/gateway await in the file is inside a `bounded(..)`
    (the list above; `grep -n "\.await" ` on the file shows no store/gateway await outside one),
    and the test still passes in 0.12 s, so the bound is not biting.
- **(b) Production path? Yes.** Unchanged from v5: legs (1)–(4), (6) drive the real
  `wyrd_server::Gateway`; (5) the two public core commits; (7)–(8) the real `S3Gateway::serve`
  on a loopback listener with SigV4-signed raw HTTP/1.1 through hyper → axum → `serve_get` /
  `serve_head` / `list_objects`. The new gateway test drives the real router with `oneshot`
  (the production dispatch → `serve_get` / `serve_head` → `evaluate_conditionals` →
  `not_modified_response`); the only double is `StoredMetaGateway`, which stands in for the
  store, not for the code under test. The ranged leg-7 requests go through the server's real
  `get_object_range` (`crates/server/src/lib.rs:555`).
- **(c) Fixture includes the fault? Yes, with the same honest limit as before.** The fast-clock
  fault is a stored stamp ahead of this host's clock, written through the ordinary write path;
  the store keeps no record of which gateway stamped it, so it is byte-identical to what a fast
  gateway leaves. Real two-host skew is not reproducible in-process; the brief accepts this
  stand-in. The pre-1970 clock cannot be produced on this host, so that item (v5's) stays tested
  at the renderer level with `None`.

## Gate results (in `$PDCA_WORKTREE`)

- `cargo fmt --all`: applied; `cargo fmt --all -- --check` clean afterwards.
- `cargo clippy -p wyrd-core -p wyrd-gateway-s3 -p wyrd-server --all-targets`: clean.
- `cargo test -p wyrd-gateway-s3 --lib -- future_stored_time`: 3 passed (the new test and the
  two v5 router-level tests).
- `cargo test -p wyrd-server --test last_modified_monotonic`: 9 passed in 0.12 s.
- `typos` over the five touched files: clean.
- `engine/scripts/run-verify.sh`: PASS (above).
- `scripts/mutants-in-diff` on the final `patch.diff`: **39 mutants tested in 2m: 35 caught,
  4 unviable, 0 missed** (the same as v5: the three production-file hunks are unchanged, and the
  new tests only add coverage). Log: `$PDCA_SCRATCH/pdca-builder-768-mutants-v6.log`.
- `engine/xtask.sh ci` (the gating C4-ci) on the final tree: **`xtask ci: all checks passed`,
  exit 0.** It covered `typos`, `lint_docs.py` (OK), `render_site.py --check` (99 pages, link
  audit OK), the gitlink/unsafe/blackbox guards, fmt, workspace clippy and build, `cargo test
  --workspace` (215 `test result: ok`, 0 failed), cargo-machete, `cargo deny check`
  (`advisories ok, bans ok, licenses ok, sources ok`), `cargo deny --all-features --config
  deny-all-features.toml check advisories` (`advisories ok`), the conformance vectors (5 valid +
  6 invalid), `xtask statics`, `xtask deploy-guard`, and the madsim DST seeds. All three of the
  brief's External dependencies (`cargo deny all-features advisories`, `typos`, `docs-renderer`)
  were present, so there is no NEEDS-HUMAN external-dependency item. Log:
  `$PDCA_SCRATCH/pdca-builder-768-xtask-ci-v6.log`.
- `patch.diff` reverse-applies cleanly against the worktree (`git apply --check -R`), and the
  bundle's `last_modified_monotonic.rs` is a copy of the worktree's.
- The target repo has no commit hooks of its own (`core.hooksPath` unset, no
  `.pre-commit-config.yaml`); its format rules are `cargo fmt` and `clippy -D warnings` inside
  `cargo xtask ci`, both clean here.
- Scratch: only log files and the saved gateway-file copy under `$PDCA_SCRATCH`
  (`pdca-builder-768-*-v6*`); the verify worktree `../wyrd-verify-l1` is the runner's own.
  `mutants.out/` in the worktree is git-ignored (`.gitignore:14`). No files outside the harness
  roots.

## Open for sign-off (not defects in this patch)

- Brief Open question 2 (a mirror that already holds a future time) is still the maintainer's
  call; leg (7)'s "earlier than the seed" assertion pins that the exception is real.
- The #902 clock-sync release dependency (v5 review T3) is unchanged and outside this slice.
- Intended wire changes, unchanged from v4/v5: a stored time past year 9999 is shown as the
  response time instead of being omitted; every 200/206/304/listing response that shows a
  `Last-Modified` carries a handler-set `Date`; on a host whose clock reads before 1970 the
  gateway caps nothing and leaves `Date` to hyper.
