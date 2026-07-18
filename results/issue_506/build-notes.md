# Build notes — issue 506 / head-object (iteration 2)

## What this iteration changes vs. iteration 1

Iteration 1's **code** was reviewed PASS on every substantive item (C1/C2/C3/C5, T1/T2/T3/T5).
It was auto-iterated only because Check §6 carried two **tooling/triage** NEEDS-HUMAN items
the previous builder asserted but could not produce evidence for. This iteration keeps the
same minimal correct fix (a missing method arm — brief: "principles.md §1.1: minimal
reviewable delta"; gratuitously reshaping working code would *violate* that) and instead
**produces the missing evidence**:

1. **C4 — `cargo machete`.** Iteration 1 claimed "full CI" while this host lacked the
   scanner, so the unused-dependency check was silently skipped
   (`xtask/src/main.rs:1613-1620`: missing binary → warn-and-skip when not in CI). This
   iteration **installed `cargo-machete v0.9.2`** (`cargo install cargo-machete --locked`)
   and **ran it on the patched tree**: `cargo-machete didn't find any unused dependencies
   in this directory. Good job!` — **exit 0**. This is now real evidence, not an assertion.
   Note the patch adds **zero** `Cargo.toml` dependencies (see stat below), so machete
   *cannot* regress from this change regardless — the test file reuses `wyrd-server`'s
   existing deps (`wyrd-metadata-redb`, `wyrd-chunkstore-fs`, `wyrd-coordination-mem`,
   `wyrd-gateway-s3` — regular deps; `tempfile`, `tokio` — dev-deps, all already declared in
   `crates/server/Cargo.toml:39-126`).

2. **T4 — closed/rejected PR history.** Iteration 1's artifacts carried only *local* git
   searches. This iteration queried **GitHub directly** (`gh`):
   - `gh pr list --repo getwyrd/wyrd --state all --search "HeadObject in:title,body"` → only
     PR **#594** (`enhancement/503-object-metadata-model`, OPEN) — the #503 metadata model
     HEAD *depends on*, not a competing HEAD arm.
   - `--search '"head object"'` → no results.
   - `--search "506"` → only #594 again; **issue #506 is OPEN** with no closed/rejected PR.
   - `git log --all -i --grep="HeadObject|head_object|Method::HEAD"` → only #503's commit
     `76dd913`; `git grep "fn head_object"` across all refs → nothing.
   No prior, in-flight, or rejected HeadObject implementation exists. This clears the
   triage question with the PR metadata the reviewer said was missing.

3. **Validation (fitness-to-purpose — real AWS CLI/SDK).** Still genuinely a human sign-off
   call and covered off-Check by the registered "aws cli (S3 gateway round-trip)" doctor row
   (`pdca.toml:706-709`). The in-process signed HTTP/1.1 loopback exercises the full
   production wire path (see (b) below); a real-client run is supplementary evidence a human
   weighs at sign-off, not a Check gate.

## What changed (path:line on the target base — the wave-folded `origin/pdca-integration/main`
@ 669384c, per the bundle's `stack-base` and the brief's Ordering note {504,505}→{503}→{506})

Patch stat: `4 files changed, 472 insertions(+), 3 deletions(-)` — **no `Cargo.toml` touched.**

1. **`crates/gateway-core/src/lib.rs:62-83`** — new `ObjectMeta` struct: the four
   header-bearing fields a HEAD answers (`size`, `etag`, `content_type`, `modified`). Its own
   type, not `ObjectRead` with `stream` ignored — see "Alternatives" below.
2. **`crates/gateway-core/src/lib.rs:167-172`** — new `head_object` seam method on
   `ObjectGateway`, beside `get_object_streaming` (`:141-144`) and `delete_object` (`:148`).
   Doc-comment states the contract: metadata only, no fragment stream.
3. **`crates/server/src/lib.rs:40`** — import `ObjectMeta`.
4. **`crates/server/src/lib.rs:373-388`** — `Gateway::head_object` impl: calls
   `read::committed_inode(&self.meta, ROOT, key)` — the exact metadata-only lookup
   `get_object_streaming` does at `:327` *before* it spawns the fragment-reader task
   (`:352-363`) — and returns `None`/`Some(ObjectMeta{..})` directly. No channel, no
   `tokio::spawn`, no chunk read: the "stat-like read that does NOT open the fragment stream"
   the brief's Scope names as the target, built by **reusing** the existing `committed_inode`
   primitive.
5. **`crates/gateway-s3/src/lib.rs:75`** — import `ObjectMeta`.
6. **`crates/gateway-s3/src/lib.rs:697-742`** — new `Method::HEAD` dispatch arm between GET
   and DELETE. Mirrors the GET arm's `Ok(None)` → `NoSuchKey` mapping (`:689-694`) exactly
   (same `error_response(request_id, StatusCode::NOT_FOUND, "NoSuchKey", …)`), and on
   `Ok(Some(ObjectMeta{..}))` builds the *same four headers GET sets* — `content-type` via the
   same `content_type_header` fallback, `content-length` from the real `size`, optional `etag`
   via the same `etag_header` degrade-not-panic helper, optional `last-modified` via the same
   `http_date` — answering `Body::empty()` instead of `Body::from_stream(stream)`.
7. **`crates/gateway-s3/src/lib.rs:753`** — `_` fallback message updated "…PUT, GET, and
   DELETE…" → "…PUT, GET, HEAD, and DELETE…" now that HEAD is supported.
8. **`crates/gateway-s3/src/lib.rs`** (three test-only mock `ObjectGateway` impls: two
   `NoGateway`-style structs and `StoredMetaGateway`) — each gains a `head_object` method, or
   the crate fails to compile once the trait grows a required method. `StoredMetaGateway`'s
   mirrors its `get_object_streaming` (stored etag/content_type, fixed modified, size 11 =
   `b"object-body".len()`).
9. **New test file `crates/server/tests/s3_head_object.rs`** (358 lines) — the brief's named
   path; a NEW file under `tests/`, the shape C4-verify's classifier keys on.

No PUT/GET/DELETE behaviour change, no route-table change (`Router::new().fallback(handle::<G>)`,
`crates/gateway-s3/src/lib.rs:166` — HEAD reaches `dispatch` like every verb), no logging
change (`finish_response`'s body-less check already matches HEAD).

## Why this shape (alternatives, with cost)

- **Reuse `committed_inode` vs. a new core primitive.** `get_object_streaming`
  (`server/src/lib.rs:326-371`) already splits into "resolve the inode" (`:327-329`, cheap)
  and "stream the fragments" (`:341-363`, expensive); HEAD needs only the first half.
  Wiring `head_object` to the existing `read::committed_inode` is a ~20-line addition vs.
  inventing a second core function that re-does `resolve` + `read_inode` + the
  `InodeState::Committed` filter — strictly more code, identical behaviour. Ruled out.
- **`ObjectMeta` its own type vs. `ObjectRead` with `Option<stream>`.** Making
  `ObjectRead.stream` optional would force every GET call site
  (`gateway-s3/src/lib.rs:650-696` + the 3 mocks) to handle an absent stream — widening the
  change into files the brief keeps out of scope ("any change to PUT/GET/DELETE behaviour").
  A new 4-field struct is +21 lines in `gateway-core` and touches no GET code: strictly
  smaller *and* honours the out-of-scope line.
- **Open-a-stream-and-drop-it (the brief's stated fallback).** `get_object_streaming`
  unconditionally `tokio::spawn`s a reader task the moment it resolves a present key
  (`server/src/lib.rs:352-363`), so "open then drop" still pays a channel alloc + a spawned
  task per HEAD — the very cost HEAD exists to avoid. The brief names this "the fallback, not
  the target"; the `head_object` seam costs one `committed_inode` call and nothing else.

## Three refutation questions (forced)

- **(a) Genuine red?** **Yes.** Proven through the project's own `C4-verify` runner
  (`engine/scripts/run-verify.sh`, `PDCA_VERIFY_BASE=origin/pdca-integration/main` per
  `stack-base`), which applies `patch.diff` to a clean checkout and runs the named test with
  production reverted then applied:
  - GREEN (fix applied): `test result: ok. 3 passed; 0 failed`.
  - RED (production reverted, test kept): `3 failed` — all `left: 405 right: 200/404`, the
    exact 405 the brief's Falsifiability/Repro predict.
  - Runner verdict: `PASS — red without the fix, green with it.`
- **(b) Production path?** **Yes.** The test drives the real loopback listener
  (`S3Gateway::serve`, `crates/gateway-s3/src/lib.rs:171`) fronting the real
  `wyrd_server::Gateway` — the same production `dispatch` → `head_object` → `committed_inode`
  path `s3_http_wire.rs` drives for GET/PUT/DELETE. No mock gateway in the new test file (the
  mocks touched in the patch are pre-existing unit-test fixtures inside `gateway-s3`, edited
  only so the crate compiles against the widened trait).
- **(c) Fixture includes the fault?** **Yes.**
  `signed_head_of_a_stored_object_…` PUTs a real object through the real wire path, then HEADs
  it (present case not curated away). `signed_head_of_an_absent_key_…` HEADs a key never PUT,
  genuinely exercising the `Ok(None)` → 404 branch.

## Other verification

- `cargo fmt -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server -- --check` — clean.
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests --all-targets
  -- -D warnings` — clean (exit 0).
- `cargo-machete` (installed this iteration) — no unused dependencies (exit 0).
- `typos crates/{gateway-core,gateway-s3,server}/src/lib.rs
  crates/server/tests/s3_head_object.rs` — clean (the target's spell-check job, exit 0).
- `cargo test -p wyrd-gateway-s3` — 58/58 pass (mock edits break nothing).
- `cargo test -p wyrd-server --test s3_http_wire` — 19/19 pass (GET/PUT/DELETE unchanged).

Commit-hooks run: `cargo fmt`, `clippy -D warnings`, `cargo-machete`, `typos` — all green, so
the patch is commit-ready for the target's own hooks.

## External dependencies

None beyond what the brief declares. The C4 test runs the in-process loopback stack. The
real-AWS-CLI confirmation remains off-Check, covered by the registered doctor row. No
NEEDS-HUMAN external dependency remains: `cargo-machete` was the only tool gap and it is now
installed and green on this host.

## Scratch discipline

No scratch checkouts created. All source edits are in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`); `cargo-machete` installed to `~/.cargo/bin`
(a standing toolchain location, not a bundle-scratch dir); the red→green proof ran through
`./engine/scripts/run-verify.sh`, which owns its `../wyrd-verify` / `pdca-verify` worktree.
Nothing to `rm -rf`.
