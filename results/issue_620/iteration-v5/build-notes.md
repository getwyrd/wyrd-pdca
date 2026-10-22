# Build notes — issue 620 / s3-object-key-length-limit (iteration 5)

Withheld from the reviewer; for the human at sign-off.

## What changed, and why this shape

Base: the worktree `$PDCA_WORKTREE` is `getwyrd/wyrd` `origin/main` @ `8ebd1c0`, not the
`7bf7eb9` the brief cites. Every `path:line` below is against `8ebd1c0` (pre-patch). The
drift matters: **#671 has merged** (PR #899, `71e9b9f`), the very change the brief listed under
`Conflicts with` because it adds a header-time refusal to the same PUT arm. So the iteration-4
starting point no longer applied verbatim; the adaptations are listed under "Base drift".

Three files:

1. `crates/gateway-s3/src/lib.rs` — the fix, plus two in-crate unit tests.
   - `MAX_KEY_BYTES = 1024`, inserted before `MAX_DELETE_BODY_BYTES` (`:489-507`). Its doc
     comment states the one place it is enforced (object PUT) and why no delete path checks it.
   - The PUT arm decodes bucket and key separately (`:1775-1779` composed them straight into
     the flat key) so each can be measured, then composes the same `object_key`.
   - Two refusals right after the CopyObject guard (`:1810-1817`), the peer callsite the brief
     named, using the same inline `error_response` shape: `400 InvalidBucketName` when the
     decoded bucket contains `/` (criterion 5), then `400 KeyTooLongError` when
     `key_name.len() > MAX_KEY_BYTES` (criteria 1, 2). `str::len` is UTF-8 bytes of the decoded
     key alone. Both run before `content_type`/`declared_len` are read and before the body
     stream is constructed (`:1866-1869`), so no body byte is stored.
   - `MAX_DELETE_BODY_BYTES` doc comment (`:489-507`): the floor is still sized on a 1024-byte key
     (now named `MAX_KEY_BYTES`), and a new paragraph says plainly that the cap is the ONLY bound
     on a `<Key>`'s length, that `parse_delete_request` accepts any non-empty key, why (legacy
     keys; the `414` ceiling makes bulk delete the only S3 exit), and the residual (a key too
     large for the cap has no S3 exit; pre-existing, not widened).
   - Bulk delete: **no code change**. `parse_delete_request` (`:2241-2315`) keeps only its
     empty-key check (`:2279-2281`); `DeleteRequestError` (`:2008-2017`) keeps its two variants;
     `delete_objects` (`:2064`) is untouched. This is the iteration-4 sign-off decision.
2. `crates/server/tests/s3_key_length_limit.rs` — NEW, the discriminator. Legs 1, 2, 4, 5 are
   the accepted v4 legs; leg 3 is rewritten to criterion (3).
3. `docs/design/architecture/05-building-block-view.md` — a paragraph after the L1 seam
   paragraph (`:137`): where the limit applies, that bulk delete has no per-key bound and why,
   and the residual. Replaces v4's paragraph, which described the rejected whole-request refusal.

## Base drift from the brief (#671 merged) — what had to adapt

- `ObjectGateway::put_object_streaming` now takes `declared_len: u64`
  (`crates/gateway-core/src/lib.rs:330-339`). The in-crate `RecordingGateway` double implements
  the new signature and `MultipartGateway` (an empty impl, as `NoGateway` at `:4091-4098` does),
  because every router bound is now `ObjectGateway + ContainerGateway + MultipartGateway`
  (`:229`, `:2072`).
- A PUT with no `Content-Length` is now `411 MissingContentLength` (`:1840-1855`). The in-crate
  oneshot requests therefore send `content-length`; the wire test's `signed()` always did.
- Ordering: the key refusals sit after CopyObject and **before** the #671 length checks, so a PUT
  that is both over-limit and length-less answers `KeyTooLongError`. Either order is header-time;
  this one keeps the brief's "mirror CopyObject's placement" literal.
- The architecture doc's L1 paragraph grew a #671 sentence; the new paragraph goes after it.

## Leg 3, as built

Seeds `b/<'a'×1025>` and `b/<'c'×70000>` through `Gateway::put_object`
(`crates/server/src/lib.rs:257`) on the `Arc<Gateway>` clone the harness keeps, PUTs `b/ok` over
the wire, confirms all three exist first (HEAD `200` for the two the request line can carry;
`Gateway::get_object`, `:338`, `Some` for the 70,000-byte one, read as `e2e.rs:38-40` does), then
one signed `DeleteObjects` naming all three. Asserts `200`, a `<Deleted><Key>…</Key></Deleted>`
row for each (the renderer's exact shape, `:2352-2357`), no `<Error>` substring, then HEAD `404`
for `ok` and the 1,025-byte key and `get_object` → `None` for the 70,000-byte key. The 71 KB body
is far under the 8 MiB cap (`:507`), as the brief's narrowed criterion asks.

The "exists first" assertions matter: S3 delete is idempotent, so a `<Deleted>` row alone would
also be printed for a key that was never there (`:2054-2056`).

I did NOT assert that HEAD of the 70,000-byte key answers `414`. The brief lists hyper's `414`
as out of scope, and asserting it would pin a dependency's constant (`http` `Uri::MAX_LEN`) in a
test about our gateway. The backend read proves the deletion instead, as the brief specifies.

## What I ruled out, with cost

- **Any bulk-delete length bound** (v4's `DeleteRequestError::KeyTooLong`, `key_too_long()`, the
  parser check, its `delete_objects` mapping, and the two unit tests pinning it: 60 lines of
  production + 70 of tests in `iteration-v4/patch.diff`). Removed entirely; the brief rejects it
  and leg 3 goes red against it (a 1,025-byte or 70,000-byte `<Key>` would answer `400`).
- **A `key_too_long(request_id)` helper.** v4 had one because PUT and bulk delete shared it. With
  one caller left, an 8-line helper plus a call is longer than the 6-line inline
  `error_response`, and inline is exactly how the CopyObject guard it mirrors is written.
- **Dropping the in-crate unit tests** (−211 lines in `mod tests`). Kept, because they prove two
  things the wire test cannot: a refused PUT never reaches `put_object_streaming` (the
  `RecordingGateway.puts` assertion — HEAD `404` only shows nothing was committed), and a bulk
  delete naming 1,025-, 1,026- and 70,000-byte keys reaches `delete_object` for every one, in
  order. They are not the discriminator (they reference `MAX_KEY_BYTES`, which the reverted base
  lacks, so they would not compile on the red leg); the wire test is. `cargo xtask ci` runs them.
- **Counting the flat `bucket/key`** instead of the key. Rejected by criterion (2)'s last clause
  and the brief's S3 source; the wire test's fully-percent-encoded 1,024-byte key under bucket
  `b` (flat name 1,026 bytes) pins it.
- **Refusing `/` in the bucket on GET/HEAD/DELETE too.** Out of scope per the brief (they create
  nothing); the check sits inside the `Method::PUT` arm only.

## Refutation — the three questions

- **(a) Genuine red?** Yes. `engine/scripts/run-verify.sh` (the configured C4-verify `cmd`)
  applied `patch.diff` to a clean `../wyrd-verify` worktree reset to `origin/main` = `8ebd1c0`,
  ran `cargo test -p wyrd-server --test s3_key_length_limit`: GREEN leg `5 passed; 0 failed`.
  It then reverted `crates/gateway-s3/src/lib.rs` and the doc (keeping the added test) and
  re-ran: RED leg `2 passed; 3 failed`. The three failures are legs 1, 2 and 5, each at the
  `assert_refused` status check (`s3_key_length_limit.rs:247`): `PUT 1,025-byte key: expected
  400 KeyTooLongError … left: 200`, `PUT 342 × €: … left: 200`, `PUT /b%2Fa/<1,023 bytes>:
  expected 400 InvalidBucketName … left: 200`. Legs 3 and 4 pass on both legs by design (they
  pin that no delete path refuses a stored key). Verdict line: `run-verify.sh: PASS — red
  without the fix, green with it (5 test(s) ran red)`.
- **(b) Production path?** Yes. The wire test drives a real loopback `S3Gateway::serve` over the
  production `wyrd_server::Gateway` on `RedbMetadataStore::in_memory` + `FsChunkStore` +
  `MemCoordination` (the stack `s3_delete_objects.rs:73-99` and `s3_list_objects.rs:92-117`
  use), with signed SigV4 raw HTTP (`sigv4::sign`, `sigv4.rs:629`; `uri_encode`, `:166`). No
  mock, no copy. The in-crate tests drive the real `S3Gateway::router()` with a recording
  double at the seam, the pattern this module already uses (`:4091`, `:4805`, `:5329`).
- **(c) Fixture includes the fault?** Yes. Leg 3 seeds the exact keys a refusal would strand
  (1,025 bytes; 70,000 bytes, past the `414` ceiling) through `Gateway::put_object`, the same
  path a legacy or `wyrd put --key` object arrives by, and checks they exist before the delete.
  Leg 4 seeds the 1,025-byte key the same way and HEADs it `200` before DELETE. Legs 1, 2, 5
  send the over-limit and aliasing requests themselves and check the store afterwards (HEAD,
  listing with an in-limit control object).

## Commit-readiness

- `cargo fmt --all` run over the worktree (it reformatted one `sign(...)` call in the test).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets` (lint levels from
  `[workspace.lints]`, as `xtask ci` runs it): exit 0, no warnings.
- `typos` over the three touched files: clean.
- The target repo configures no git hooks (`core.hooksPath` unset; no `.githooks`,
  `.pre-commit-config.yaml` or `lefthook.yml`). Its commit-time checks are CI's `cargo xtask ci`.
- `./engine/xtask.sh ci` (the configured C4-ci runner) in the worktree: see the line below.

CI RESULT: `./engine/xtask.sh ci` in `$PDCA_WORKTREE` finished `xtask ci: all checks passed`,
exit 0 — fmt `--check`, `clippy --workspace --all-targets`, `build --all-targets`,
`test --workspace --exclude wyrd-dst` (the step that runs the two in-crate unit tests,
`an_object_put_refuses_an_over_limit_key_before_the_write_seam` and
`a_bulk_delete_deletes_an_over_limit_key_rather_than_refusing_it`; a failure there fails the
step), the DST leg, both cargo-deny steps (so the all-features advisory wall that held
iterations 1–3 is clean on this base), and the conformance vectors. I captured only the last
120 lines of its log, so the per-test `ok` lines for `wyrd-gateway-s3` are not in my transcript;
the step verdict is.

## Rubric self-review (AGENTS.md `## Review rubric & protocol`)

- One clock per lifecycle: no clock read added. The test files carry the standing #619
  file-scope exemption comment the peer suites use.
- Narrow seams / dependency direction: no trait or crate boundary touched; the S3 crate still
  uses only the gateway seam.
- Metadata validation boundaries: not touched.
- No DST-reachable shared mutable global state: none added (the test double's `Mutex` fields are
  instance state inside `mod tests`).
- `#![forbid(unsafe_code)]`: the new test file carries it.
- Docs currency: the API change (two new PUT refusals) is in
  `05-building-block-view.md` in the same patch.
- Protocol input: the limit is on the decoded key's UTF-8 bytes, so neither a long wire
  spelling nor a short one changes the verdict (criterion 2's third case pins it).
- Absent/unsupported entries: no silent skip added; bulk delete's behaviour is unchanged.
- Await discipline: every socket/backend await in the wire test is under `IO_BUDGET` (30 s); the
  server task is aborted in `Harness::drop`; the in-crate oneshot is under a 30 s timeout.
- Test fidelity: no new destructive or concurrent path (bulk delete is unchanged), so no new
  Tier-0 DST obligation; the wire test runs on the real redb/fs/mem backends.
- Workflow edits: none.

## External dependencies

None missing. `typos` is installed and ran clean; `docs-renderer` (`markdown_it`, `yaml`) is a
`cargo xtask ci` warn-skip concern — the doc change adds no links, so the dangling-link audit
has nothing new to check. `cargo deny all-features advisories` is the base's concern, covered by
the pre-dispatch guard that released this Do; the `xtask ci` run below exercises it on this tree.

## Pointers for the human

- The brief's "Plan-review response" notes #893's text still claims "no stored key is stranded
  whatever its length" and that recovering an over-cap key is tracked nowhere; this patch's
  doc comment and architecture paragraph say the narrower, true thing. Nothing filed from here.
- Line numbers in the brief (against `7bf7eb9`) are ~20 lines off in `crates/gateway-s3/src/lib.rs`
  after #671; the ones above are against `8ebd1c0`.
