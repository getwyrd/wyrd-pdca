# Result — issue 620 / s3-object-key-length-limit

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway enforces S3's 1,024-byte object-key limit nowhere. A `PUT
  /{bucket}/{key}` stores a key of any length: the decoded key is composed at
  `crates/gateway-s3/src/lib.rs:1765-1769` and streamed straight into `put_object_streaming`
  (`:1829-1868`). `parse_delete_request` (`:2171-2245`) accepts any non-empty `<Key>`
  (`:2209-2212`) up to the whole 8 MiB body cap. Yet `MAX_DELETE_BODY_BYTES` (`:488-507`) is sized
  on "1000 keys × a 1024-byte key", so the cap and the parser disagree about what one entry can
  be (the #612 review follow-up in this issue's thread). The limit cannot land in bulk delete
  alone. PUT would still create a 2,000-byte key, and bulk delete would then refuse to delete
  it, which is the "undeletable object" this issue was deferred to avoid.
- Success criterion: A NEW wire-level test over the in-process gateway (signed raw HTTP and/or
  the stock SDK, as the peer suites do) shows:
  (1) **Creation refuses an over-limit key.** A PUT whose decoded key is 1,025 UTF-8 bytes answers
  `400` with S3 error code `KeyTooLongError`, before the body is stored: afterwards HEAD of that
  key is `404` and a listing of the bucket does not show it.
  (2) **The limit counts UTF-8 bytes of the decoded key, not characters or wire bytes.** A key of
  512 × `é` (1,024 bytes, 512 characters) is accepted and round-trips (HEAD `200`). A key of
  342 × `€` (1,026 bytes, 342 characters) is refused `KeyTooLongError`. A 1,024-byte key sent
  percent-encoded on the wire (so its request path is far longer than 1,024) is accepted. The
  limit applies to the key alone, not to `"{bucket}/{key}"`.
  (3) **Bulk delete's per-entry bound matches.** A `DeleteObjects` request naming a 1,025-byte key
  next to an existing ordinary key is refused as a whole with `400` `KeyTooLongError`, and no key
  is touched: the ordinary object still exists afterwards. A request whose keys are all ≤ 1,024
  bytes, including one of exactly 1,024 bytes, behaves as it does today.
  (4) **Deletion never strands an object.** An over-limit object that already exists stays
  removable by single-object `DELETE`. The test seeds `b/<'a' × 1025>` through the existing
  backend API, `Gateway::put_object` (`crates/server/src/lib.rs:235`), on an `Arc<Gateway>` clone
  it keeps before handing the gateway to `S3Gateway::new` (`crates/gateway-s3/src/lib.rs:232`
  takes `Arc<G>`). Then: signed HEAD of that key is `200`; signed `DELETE` is `204`; HEAD again
  is `404`. The HEAD-before and HEAD-after legs are required, because DELETE answers `204` for
  an absent key too (`crates/gateway-s3/src/lib.rs:1887-1889`), so `204` alone proves nothing.
  This is the way out for any over-limit object stored before this fix or written outside the S3
  surface (the data-plane `wyrd put --key`, `crates/server/src/cli.rs:562-568`).
  (5) **No bucket-separator alias around the limit.** The gateway splits the path on the first
  literal `/` before decoding (`crates/gateway-s3/src/lib.rs:567-572`) and joins the decoded
  bucket and key into one flat key (`:1765-1769`). So `PUT /b%2Fa/<'x' × 1023>` has a 1,023-byte
  request key yet stores the same object as `/b/a/<'x' × 1023>`, whose key under bucket `b` is
  1,025 bytes. A PUT whose decoded bucket segment contains `/` must be refused `400`
  `InvalidBucketName` (S3 bucket names cannot contain `/`) before the body is stored: afterwards
  HEAD of `/b/a/<'x' × 1023>` is `404`. Send this one as signed raw HTTP (the SDK will not put
  `%2F` in a bucket name).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: S3 key-length enforcement where an object is created (PUT), the refusal of a PUT
  whose decoded bucket segment contains `/` (criterion 5), and the per-entry bound
  in the bulk-delete parser, with the `MAX_DELETE_BODY_BYTES` sizing comment kept true / out of
  scope: CopyObject, which answers `501` today (`crates/gateway-s3/src/lib.rs:1800-1806`; when
  #504 implements it, the same rule applies to its destination key); single-object GET, HEAD and
  DELETE (unchanged, see criterion 4); listing parameters (`prefix`, `start-after`,
  `continuation-token`); the data-plane CLI `wyrd put --key`, which is not an S3 surface; bucket
  naming rules, except the one criterion (5) needs (no `/` in a decoded bucket, on PUT only; GET,
  HEAD and DELETE on such a path stay as they are, since they create nothing); the multipart verbs (#508's slices apply the same rule to
  CreateMultipartUpload's key when they land); the 8 MiB cap value itself.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `cargo deny all-features advisories` confirmed absent (detect cmd `cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories` exited 1). Install hint: the target's all-features advisory wall (the second cargo-deny step of cargo xtask ci) fails on the base, so every bundle's gating C4-ci is red until a separate deny-all-features.toml PR records or clears the new advisory (#543; precedent #790/#773). Offline and read-only (--frozen: cached advisory DB, Cargo.lock unchanged), so it reflects the DB the last C4-ci run fetched; it checks ../wyrd as checked out, or $WYRD_REPO. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] External dependency `cargo deny all-features advisories` confirmed absent (detect cmd `cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories` exited 1). Install hint: the target's all-features advisory wall (the second cargo-deny step of cargo xtask ci) fails on the base, so every bundle's gating C4-ci is red until a separate deny-all-features.toml PR records or clears the new advisory (#543; precedent #790/#773). Offline and read-only (--frozen: cached advisory DB, Cargo.lock unchanged), so it reflects the DB the last C4-ci run fetched; it checks ../wyrd as checked out, or $WYRD_REPO. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [x] external dependency: cargo deny all-features advisories — the gating C4-ci row (`cargo xtask ci`) stops at `cargo deny --all-features --config deny-all-features.toml check advisories` on the unpatched base `7bf7eb9`, on RUSTSEC-2026-0346/0349 in tikv-client's rustls 0.21.12, so no bundle on this base can show a green C4-ci; the ready fix is `base-advisory-waiver.diff` in this bundle (both advisories are medium/low, below the file's high/critical review trigger; no rustls 0.21.x fix exists); land it as its own PR as #790 did, bring the bundle's base up to that main, then iterate-do.
- [x] **The creation invariant has a bucket-separator bypass that the scope excludes.** `brief.md:42-43` promises that no oversized S3 key can be created, but `brief.md:27-28` measures only the decoded request key and `brief.md:69-70` excludes bucket naming. The target splits on a literal slash before decoding (`crates/gateway-s3/src/lib.rs:567-572`), then decodes both components into one flat storage key (`:1765-1769`), without rejecting a decoded slash in the bucket. Source-derived counterexample: a signed `PUT /b%2Fa/<1023 ASCII x characters>` has a 1,023-byte request key, so the proposed check accepts it; it addresses exactly the same stored object as `/b/a/<1023 ASCII x characters>`, whose key under bucket `b` is 1,025 bytes. Bulk deletion through `/b?delete` naming that canonical key would then be refused by criterion (3). This leaves the thread's write-path-first concern reachable through an alias despite all proposed boundary examples passing. Revise the brief to include a narrow separator-alias refusal and a wire regression, or explicitly qualify the invariant and record this bypass as a deferral; the unconditional promise and current exclusion cannot both stand.
- [x] **Criterion (4) can pass without proving its promised escape path.** `brief.md:33-36` requires only `DELETE` → `204` to establish that legacy or non-S3 oversized objects remain deletable. The target deliberately returns `204` for both present and absent keys (`crates/gateway-s3/src/lib.rs:1887-1889`); the backend distinguishes these as `Ok(true)` and `Ok(false)` (`crates/server/src/lib.rs:650-665`). After criterion (1)'s rejected PUT, deleting that same absent key would satisfy (4) without exercising removal of any oversized object. Require a pre-existing oversized fixture seeded through the existing backend API (`crates/server/src/lib.rs:235`), HEAD `200` before deletion, then signed DELETE `204` and HEAD `404` afterwards. Allow that fixture setup explicitly alongside the wire assertions in `brief.md:39`; it needs no new production API and remains runnable on the reverted base.
- [ ] **The patch strands objects whose keys are too long for a request line.** `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over 65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`, DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3) demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable, and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of scope).
- [ ] Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- [x] The only failing gating row, C4 `cargo xtask ci`, fails on `cargo deny --all-features ... check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls v0.21.12` (`gate-logs/C4-ci.log:3475-3557`, `Cargo.lock:3294-3295`). The patch touches only `crates/gateway-s3/src/lib.rs`, the new test file and the architecture doc. It changes no `Cargo.toml` or `Cargo.lock`. Every earlier stage in that log passed: fmt, clippy `--all-targets`, build, the full test run including the 3 new unit tests and the 5 wire tests, typos, and docs-check. No rebuild of this patch can clear the row. A dependency bump or a recorded advisory exception has to be decided separately.
- [x] external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci; a maintainer must waive or otherwise resolve the two advisories in a separate PR against `deny-all-features.toml` (sketch above, subject to the file's #543 review trigger), after which this bundle's C4-ci can be re-run.
- [ ] **One stored over-limit key blocks bulk cleanup of up to 999 other keys.** `crates/gateway-s3/src/lib.rs:2270` (with `:2114`) refuses a whole DeleteObjects (S3 bulk delete) request if any entry is over 1,024 bytes. Over-limit objects can still exist: those stored before this fix, or written with `wyrd put --key`. `list_container` lists them without filtering (`crates/server/src/lib.rs:714-741`). Concrete case:
- [x] **C4-ci is red for a reason outside this patch.** The `cargo deny --all-features … check advisories` step fails on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (security advisories) in `rustls v0.21.12`, which comes in through `tikv-client v0.4.0` → `wyrd-metadata-tikv` (`gate-logs/C4-ci.log:3480-3557`; the same at `:6484-6561` on retry). Every earlier stage passed: typos, docs lint/render, guards, fmt, clippy, build, the full test run (including all 5 new wire tests and both new unit tests), machete, and the default `cargo deny check`. This is the third round with the same red, and rebuilding this patch cannot change it. It needs a separate dependency bump or a deny exception. This is not evidence against the fix.
- [x] external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12, published 2026-10-14) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci. A ready fix is in this bundle as `base-advisory-waiver.diff` (both are medium/low, below the file's high/critical review trigger; no rustls 0.21.x fix exists). Land it as its own PR (as #790 did for RUSTSEC-2026-0258), then re-run this bundle's C4-ci. A full `cargo xtask ci` with patch + waiver applied is recorded under Verification.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Bulk-delete design must change; the PUT-side limit (incl. %2F bucket refusal) is right and should carry over unchanged. - Criterion (3)'s whole-request refusal breaks bucket-emptying tools: one legacy over-limit key fails every 1,000-key DeleteObjects batch it sits in (aws s3 rm --recursive, rclone purge), leaving the other 999 keys (§6 items 6, 9). - It also strands keys above the request-line ceiling (~64 KiB, ~21 KiB percent-encoded): HEAD/DELETE answer 414 there, and bulk delete was their only exit (§6 item 5). - Re-plan bulk delete: per-key <Error><Code>KeyTooLongError</Code> rows (other keys still deleted), or no per-entry bound (body cap only). Check what real S3 does for an over-limit key in DeleteObjects before choosing. - Decide whether `wyrd put --key` gets the same 1,024-byte cap (closes the source of new over-limit keys) — in this slice or a follow-up.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
