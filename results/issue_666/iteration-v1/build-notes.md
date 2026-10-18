# Build notes — issue 666 / multipart-gateway-seam

Target: `getwyrd/wyrd` @ `main` = `7bf7eb9` (worktree `HEAD`, also `origin/main`).
`patch.diff` applies cleanly to `7bf7eb9` (checked with `git apply --cached --check` against a
throwaway index). All `path:line` below are on the patched tree unless marked `base`.

## Read first — two things for the human

### 1. I killed another session's gate run (issue 686)

While stopping my own `cargo xtask ci` run (to re-run it after a rename), I used
`pkill -f 'target/debug/xtask ci'`. The pattern matched by command line, and **it also killed
issue 686's builder's gate run**, `timeout 3500 cargo xtask ci` in its own worktree (PIDs
1347403/1347404). Its log,
`/var/tmp/pdca/wyrd-pdca-9c587031/issue_686/pdca-builder-686-xtask-ci.log`, stops in the
`cargo test` stage (last write 2026-10-14 20:26:08 CEST). Whatever bundle 686's Do beat recorded
for that local run is **a kill, not a verdict on its patch**, and that run should be repeated
(Check's C4-ci re-runs it anyway). After this I stopped processes only by exact PID. Nothing
else of 686's was touched.

### 2. `cargo xtask ci` is red on base `7bf7eb9`, for a reason outside this patch

The gate fails at its `cargo deny --all-features --config deny-all-features.toml check
advisories` step on two RustSec advisories published against `rustls 0.21.12`:
RUSTSEC-2026-0346 (`Debug` impls expose TLS secrets) and RUSTSEC-2026-0349 (unauthenticated
ServerHello can downgrade later connections). `rustls 0.21.12` reaches the workspace only through
the off-by-default `tikv` feature: `tikv-client 0.4.0 → tonic 0.10.2 → tokio-rustls 0.24.1 →
rustls 0.21.12` (`Cargo.lock:302`). This patch changes no `Cargo.toml`, no `Cargo.lock`, and
neither deny config (`git diff 7bf7eb9 --stat -- '*Cargo.toml' Cargo.lock deny.toml
deny-all-features.toml` is empty), and `cargo deny` reads only those plus the live advisory
database, so the step fails identically on base. The default-features `cargo deny check` passes
(`advisories ok, bans ok, licenses ok, sources ok`).

Fixing it is out of scope here: the repo keeps a deliberate, tested waiver list for
`tikv-client` advisories ("THE tikv-client EXPOSURE BOUNDARY (#543)",
`deny-all-features.toml:50` onward, plus xtask's quarantine-boundary test, commit `4323067`).
Adding these two IDs there, or bumping off `tikv-client 0.4.0`, is a policy change for its own
issue. Until then **C4-ci will be red for every bundle**, this one included, and leg (4) of the
success criterion cannot be shown green as one command. What I could run is below (leg 4): every
step except the failing one passes, including the four steps the gate never reached.

NEEDS-HUMAN external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9, so leg (4) "the full gate is green" cannot be produced by this patch; every other gate step passes on the patched tree

```toml
[[doctor.checks]]
id    = "wyrd-base-advisories"
cmd   = "cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories"
hint  = "a new RustSec advisory has landed against the base lockfile: waive it in deny-all-features.toml (the #543 tikv-client boundary) or bump the crate, on main, before cycling bundles"
level = "WARN"
```

## What changed, and why

The brief asks for the seam only: a `MultipartGateway` companion trait, the composition root
answering "not implemented" for every verb, the S3 crate requiring the trait, every implementer
updated, and the architecture doc updated. Routing does not change.

1. **The trait and its vocabulary**, `crates/gateway-core/src/lib.rs:433-764` (new; after
   `ContainerGateway`, base `:389-431`).
   - `pub trait MultipartGateway: Send + Sync + 'static` at `:672`, mirroring `ContainerGateway`:
     ADR-0046 decision 6 / ADR-0010 framing in the doc, `impl Future<…> + Send` methods, and a
     **default body** on every method answering `NotImplemented`, so the S3 crate's object-only
     test doubles need just `impl MultipartGateway for X {}`.
   - Six methods (`:677`, `:694`, `:714`, `:728`, `:742`, `:755`), each taking `container` and
     `key` as separate `&str`. `upload_part` copies `put_object_streaming`'s stream bound
     exactly (base `:300-308`: `S: Stream<Item = Result<Bytes>> + Send + Unpin + 'static`) and
     takes a `ContentHash`. Upload id is `&str`, part number `u32`.
   - The trait doc fixes the contract the brief asks for: answers are values; `Err` means what
     it means on `put_object_streaming` (store/stream failure, or `GatewayError::PayloadMismatch`
     for `upload_part` only); an unparsable upload id answers `NoSuchUpload`; an out-of-range
     part number answers `PartNumberOutOfRange`; listings are complete and the wire paginates.
   - No enum is `#[non_exhaustive]` (stated in the section comment at `:435-445`, as core does at
     `crates/core/src/multipart.rs:4256-4259`).
2. **The composition root**, `crates/server/src/lib.rs:751-842` (new; beside the
   `ContainerGateway` impl at `:688-749`), same `M`/`C`/`Co` bounds. Every method is written
   out, answers `Ok(…::Refused(MultipartRefusal::NotImplemented))`, and carries a one-line
   comment naming the slice that replaces its body (`:768` #668 create, `:787` #669 upload part,
   `:801` #670 complete, `:814`/`:827`/`:835` #668 abort/list parts/list uploads). Unused
   arguments are consumed with `let _ = (…);` inside the body rather than renamed `_x` in the
   signature, so the later slice changes only the body. Import widened at `:43-48`.
3. **The S3 crate's bound**, `crates/gateway-s3/src/lib.rs`: import `:79`; every
   `G: ObjectGateway + ContainerGateway` gains `+ MultipartGateway` at `:229` (`S3Gateway::new`'s
   `where`), `:797` (`list_objects`), `:1527` (`handle`), `:1638` (`dispatch`), `:2002`
   (`delete_objects`), and the test helper `signed_ranged_request` at `:5148`. Those are exactly
   the brief's six lines; the edit was a `sed` over the exact text, and it changed those lines
   and nothing else. `serve_get`/`serve_head` bound only `G: ObjectGateway` and are left alone.
   Nothing in the S3 crate calls the trait. The denylist (`:410-453`) and both guards are
   untouched.
4. **Every implementer**: empty `impl MultipartGateway for X {}` beside each
   `impl ContainerGateway for X {}`: S3 doubles at `crates/gateway-s3/src/lib.rs:3711`
   (`NoGateway`, fn-local), `:4007` (`NoGateway`), `:4697` (`StoredMetaGateway`), `:5203`
   (`DefaultRangeSeamGateway`), `:5325` (`VersionSkewGateway`); server test doubles at
   `crates/server/tests/request_capacity_planes.rs:195` (`FaultyGateway`) and `:266`
   (`MidStreamFaultGateway`), import at `:77-79`.
   - **One extra site not in the brief's list:** `request_capacity_planes.rs:362`, the
     `start_s3<G>` helper bound (base `:356`). It calls `S3Gateway::new`, so the compiler requires
     the third bound. Every other `S3Gateway::new` caller in the workspace passes the real
     `Gateway` and needed no change (`grep -rn 'S3Gateway::new'`: `cli.rs:2664`, 13 test files,
     `crates/validate/tests/s3_client_roundtrip.rs:307`).
5. **Docs currency** (a new port): one paragraph after the L1 table,
   `docs/design/architecture/05-building-block-view.md:137`, naming the seam (`ObjectGateway` +
   the two companions), that multipart answers are typed values with no HTTP status, and that
   `MultipartGateway` answers "not implemented" with the wire still `501` until #668. The doc
   had no prior mention of `ObjectGateway`/`ContainerGateway`, so the paragraph introduces the
   seam briefly instead of only appending one trait name.

## Chosen names (open question 1) — for #667–#670 to cite

All in `wyrd_gateway_core`. Every name carries a `Multipart` prefix (or is a `Listed*` row
type) because core already has `UploadPartAnswer`, `CompleteAnswer`, `AbortAnswer`,
`ListPartsAnswer`, `ListUploadsAnswer` (`crates/core/src/multipart.rs:4488-4532`, the
lifecycle-half answers); #668's server mapping imports both families, and unprefixed seam names
would collide.

| Seam type (`gateway-core`) | Mirrors core (`crates/core/src/multipart.rs`) |
|---|---|
| `MultipartGateway` (trait) | — |
| `MultipartRefusal` | `Refusal` (`:4302`) + 2 seam arms |
| `MultipartBackpressure` | `Backpressure` (`:4277`) |
| `MultipartInvalidPart` | `InvalidPart` (`:4263`) |
| `MultipartCreateAnswer { Created { upload_id: String }, Refused }` | `CreateOutcome` (`:4380`) |
| `MultipartUploadPartAnswer { Committed { part_number: u32, etag: String }, Refused }` | `UploadPartOutcome` (`:4409`) |
| `MultipartCompleteAnswer { Published(MultipartPublication), AlreadyCompleted(MultipartPublication), Refused }` | `CompleteOutcome` (`:4427`) |
| `MultipartAbortAnswer { Fenced, AlreadyAborting, Refused }` | `AbortOutcome` (`:4440`) |
| `MultipartListPartsAnswer { Listed(Vec<ListedPart>), Refused }` | — (new success type) |
| `MultipartListUploadsAnswer { Listed(Vec<ListedUpload>), Refused }` | — (new success type) |
| `MultipartPublication { etag: String, modified: u64 }` | `Publication` (`:4351`) |
| `ListedPart { part_number: u32, etag: String, size: u64, modified: u64 }` | from `PartRecord` (`:2749`) |
| `ListedUpload { key: String, upload_id: String, initiated: u64 }` | from `SessionRecord` (`:2265`) |

`MultipartRefusal` arms: `NoSuchUpload`, `NoSuchContainer`, `OperationAborted`,
`InvalidPart { part_number: u32, reason }`, `EntityTooLarge { chunks, limit }`,
`SlowDown { pressure }`, `ProfileSkew`, `CompleteAttemptsExhausted`, and the seam's own
`PartNumberOutOfRange { part_number: u32 }` and `NotImplemented`. The "not implemented" answer of
verb V is therefore `MultipartVAnswer::Refused(MultipartRefusal::NotImplemented)`.

Mapping decisions worth knowing:
- **Arm names keep core's** (`Fenced`, `DigestMismatch`, `SlowDown`, …) so #668's mapping is
  arm-to-same-name and easy to review. **One exception: core's `NoSuchBucket` is the seam's
  `NoSuchContainer`.** `gateway-core`'s crate doc says no bucket vocabulary leaks into the seam
  (`crates/gateway-core/src/lib.rs:16-18`), and `ContainerGateway` follows that; the S3 crate maps
  `NoSuchContainer` to S3's `NoSuchBucket`. I first wrote `NoSuchBucket` and renamed it on
  self-review.
- **Payload field `digest` → `etag`.** The seam's existing word for the content change-token is
  `etag` (`ObjectRead.etag`, `ObjectMeta.etag`, `ListedObject.etag`, `put_object_streaming`
  returns "the ETag"). A core `Digest` becomes a lowercase-hex `String`, unquoted.
- **`ProfileSkew` carries no payload** (open question 2). Core's two `Budget`s are an operator
  diagnosis that core alarms on; a client answer needs only the arm. The arm is kept.
- **`MultipartPublication` drops core's `inode` and `version`.** The seam never names an inode
  (`ObjectRead`/`ObjectMeta` carry none), and S3's Complete response needs only the ETag. It
  keeps `etag` and the publication time (`modified`, from `completed_at_millis`).
- **List timestamps are `u64`, not `Option<u64>`.** Core's records always carry them
  (`PartRecord::committed_at_millis`, `SessionRecord::created_at_millis`), so nothing is
  fabricated and nothing is absent. Documented as epoch millis, the unit `ObjectMeta.modified`
  uses. If #668 finds core's stamps are not epoch-based (core calls them "logical
  milliseconds" with a recorded `clock_source`), that is #668's mapping to resolve; I did not
  read further into core's clock to settle it.
- **`list_multipart_uploads` order: by key, then upload id.** S3's `upload-id-marker` resumes
  within a key by upload id, so this order lets the wire layer paginate exactly. #668 may
  revisit it; it is one doc line on the trait (`:754`) and on the answer enum (`:637`).
- **`complete_multipart_upload` takes `Vec<(u32, String)>`** (owned), mirroring core's
  `&[(PartNumber, Digest)]` with plain types; the ETags arrive with the wire's quoting removed.
- `ReserveOutcome` (core `:4393`) is an internal slot-reservation step, not a client verb, so the
  seam has no mirror of it.

## The test — `crates/server/tests/multipart_gateway_seam.rs` (new, 4 tests)

- **Leg (1)** `the_composition_root_answers_every_multipart_verb_not_implemented` (`:97`): a real
  `Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>` built as the peers build it
  (`real_gateway`, `:86`; `RedbMetadataStore::in_memory`), handed to
  `assert_every_verb_not_implemented<G: MultipartGateway>` (`:116`). That helper calls all six
  methods with fully-qualified syntax (`MultipartGateway::create_multipart_upload(gateway, …)`),
  `.expect`s each result `Ok` (an `Err` fails the test), and `assert_eq!`s it against the exact
  `Refused(NotImplemented)` value, with the verb's name as the failure message.
- **Defaults** `the_trait_defaults_answer_every_multipart_verb_not_implemented` (`:109`): the same
  helper over `NoMultipart` (`:104`), whose `impl MultipartGateway for NoMultipart {}` is empty.
  Not one of the brief's legs; I added it because nothing else executes the six default bodies in
  `gateway-core` (the sentinel overrides them, and the S3 doubles that rely on them are never
  asked a multipart question). The brief relies on those defaults ("a gateway with no such
  concept … needs only an empty `impl`"), so they get the same check.
- **Leg (2)** `the_real_gateway_meets_the_s3_crates_gateway_bound` (`:210`): the generic
  `start_s3<G: ObjectGateway + ContainerGateway + MultipartGateway>` (`:192`) takes the real
  `Gateway`, builds `S3Gateway::new` over it, serves on loopback, and all six forms answer `501`.
  As the brief says, this proves the real `Gateway` meets the triple bound, not that the S3 crate
  requires it; that part is for source review (item 3 above lists every bound line).
- **Leg (3)** `every_multipart_form_still_answers_501_without_entering_the_seam` (`:355`): a
  `Sentinel` (`:226`) whose six `MultipartGateway` methods each bump `multipart_calls` and answer
  `NotImplemented`. Its `ObjectGateway` methods and an explicit `list_container` bump a second
  counter, `other_calls`, and answer an error / `Ok(None)`. Both counters are struct fields
  (ADR-0035), not statics. Each of the six brief-listed forms (`MULTIPART_FORMS`, `:67`) is sent
  as a raw SigV4-signed request (`send_signed`, `:405`, after `s3_list_objects.rs`'s `get_raw`)
  and must be `501`, carry `<Code>NotImplemented</Code>`, and name its subresource
  (`` the `uploads` S3 subresource `` etc.: the first denylisted key, which is what
  `unsupported_subresource` reports). Then both counters must be 0. Each request is wrapped in a
  30 s `tokio::time::timeout`, so a hung server fails the test instead of hanging the run.

## Running it — and how

The xtask runner (`./engine/xtask.sh`, `pdca.toml [gates] runner`) has no single-test
subcommand (`xtask/src/main.rs:119-150`). For the quick red/green passes I ran
`timeout 1500 cargo test -p wyrd-server --test multipart_gateway_seam` in the worktree (bounded
by `timeout`). The full suite ran through the project's runner,
`PDCA_WORKTREE=<worktree> ./engine/xtask.sh <subcmd>` (leg 4 below).

Green on the final tree:

```
running 4 tests
test the_trait_defaults_answer_every_multipart_verb_not_implemented ... ok
test the_composition_root_answers_every_multipart_verb_not_implemented ... ok
test every_multipart_form_still_answers_501_without_entering_the_seam ... ok
test the_real_gateway_meets_the_s3_crates_gateway_bound ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Demonstrated red (the brief's Verification posture)

The outputs below are verbatim. They were taken before a final one-line doc edit to the test's
module header, so their panic lines (`:170:5`, `:364:5`) are `:171:5` / `:365:5` on the shipped
file; the asserting code is unchanged.

`Gateway::list_parts` (`crates/server/src/lib.rs:829-831`) temporarily answered
`Ok(MultipartListPartsAnswer::Listed(Vec::new()))`. Leg (1) went red, naming the verb; the other
three stayed green. File restored byte-for-byte afterwards.

```
test the_trait_defaults_answer_every_multipart_verb_not_implemented ... ok
test the_composition_root_answers_every_multipart_verb_not_implemented ... FAILED
test every_multipart_form_still_answers_501_without_entering_the_seam ... ok
test the_real_gateway_meets_the_s3_crates_gateway_bound ... ok
thread 'the_composition_root_answers_every_multipart_verb_not_implemented' panicked at crates/server/tests/multipart_gateway_seam.rs:170:5:
assertion `left == right` failed: list_parts
  left: Listed([])
 right: Refused(NotImplemented)
test result: FAILED. 3 passed; 1 failed
```

The same mutation in the trait's default `list_parts` (`crates/gateway-core/src/lib.rs:749-751`)
turned only the defaults test red (same assertion at `:170:5`, `left: Listed([])`); leg (1)
stayed green. Restored.

### Leg (3)'s counter is what binds

I temporarily added a route in `dispatch`, just above the object-path subresource guard
(`crates/gateway-s3/src/lib.rs:1777`): for `POST …?uploads` it called
`state.gateway.create_multipart_upload(bucket, key, None)` and then returned the **identical**
`501 NotImplemented` "the `uploads` S3 subresource/operation is not supported" response. Every
status and body assertion still passed, and leg (2) over the real `Gateway` stayed green. Only
the counter caught it. Restored.

```
test every_multipart_form_still_answers_501_without_entering_the_seam ... FAILED
test the_real_gateway_meets_the_s3_crates_gateway_bound ... ok
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:364:5:
assertion `left == right` failed: a multipart form entered the MultipartGateway seam; routing is #667's, not this slice's
  left: 1
 right: 0
```

### Reverted-fix run (what C4-verify will see)

With every production change reverted (`git checkout -- crates docs`, keeping only the new test
file), the test does not build:

```
error[E0432]: unresolved imports `wyrd_gateway_core::MultipartAbortAnswer`, … `wyrd_gateway_core::MultipartGateway`, …
error: could not compile `wyrd-server` (test "multipart_gateway_seam") due to 1 previous error
```

So C4-verify will report **UNVERIFIABLE (77)**: no test ran on the red leg. The brief
pre-declares this (Verification posture (a), net-new symbol), so it is a sign-off item, not a
defect. The mutation runs above are the red evidence.

## Self-refutation (forced)

- **(a) Genuine red? Yes, with the caveat the brief pre-declares.** Reverting the whole fix makes
  the test fail to *compile* (`E0432`), not fail an assertion, because the trait is new. The
  binding red comes from mutating production: one server method answering anything but
  `NotImplemented` turns leg (1) red; the same in a trait default turns the defaults test red;
  routing one form into the seam turns leg (3) red even though every status stays `501` (outputs
  above). Leg (3) is green on the base by design (it pins "routing untouched"); it cannot build
  there only because the file names the new trait.
- **(b) Production path? Yes.** Leg (1) calls the six methods of the real
  `impl MultipartGateway for wyrd_server::Gateway` (`crates/server/src/lib.rs:756`); the defaults
  test calls the trait's real provided methods in `gateway-core`. Legs (2) and (3) drive the real
  `S3Gateway` router over a real loopback listener with real SigV4 signing. The sentinel stands in
  only for the backend *behind* the seam, which is the thing whose non-entry leg (3) measures;
  the routing under test is production.
- **(c) Fixture includes the fault? Yes.** The sentinel answers `NotImplemented`, which is exactly
  what makes "entered the seam and mapped it to 501" look correct on the wire. The fixture keeps
  that fault shape in, and the counter detects it (demonstrated). All six brief-listed forms,
  including the bucket-scoped `GET /b?uploads`, are in `MULTIPART_FORMS`.

## Leg (4) — the full gate

**Not green as one command, because of the base advisories in "Read first" item 2.** Every step
I could run passes on the patched tree:

`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l2 ./engine/xtask.sh ci` (= `cargo xtask ci`,
`xtask/src/main.rs:1671-1688`) on the final tree, exit 1:

| Step | Result |
|---|---|
| `typos` | pass |
| `lint_docs.py` | pass (`lint_docs: OK`) |
| `render_site.py --check` | pass (renderer deps installed; `link audit OK`) |
| gitlink / unsafe / blackbox guards | pass |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --exclude wyrd-dst --all-targets` | pass (builds the S3 crate's `#[cfg(test)]` doubles and the `:5148` helper) |
| `cargo build --workspace --exclude wyrd-dst --all-targets` | pass |
| `cargo test --workspace --exclude wyrd-dst` | pass: 197 test binaries, 1,680 tests, 0 failed (incl. the 4 new tests and the 7 in `request_capacity_planes.rs`) |
| `cargo-machete` | pass |
| `cargo deny check` | pass (`advisories ok, bans ok, licenses ok, sources ok`) |
| `cargo deny --all-features --config deny-all-features.toml check advisories` | **FAIL**: RUSTSEC-2026-0346, RUSTSEC-2026-0349 on `rustls 0.21.12` (base, not this patch) |
| conformance, statics, orchestrator guard, DST | not reached by `ci` (it stops at the first failure) |

The four unreached steps, run separately through the same runner:

| Step | Result |
|---|---|
| `./engine/xtask.sh conformance` | pass (`5 valid + 6 invalid vectors pass`) |
| `./engine/xtask.sh statics` | pass (`no DST-reachable shared mutable global state (ADR-0035)`), re-run on the final tree |
| `./engine/xtask.sh dst` | pass: `cargo clippy -p wyrd-dst` and `cargo test -p wyrd-dst` under `--cfg madsim`, 10 binaries, 0 failed (one test ran 1,362 s) |
| orchestrator guard | no subcommand of its own; covered by inspection: it greps `crates/` for `kube::`, `kube_client::`, `k8s_openapi::`, `kube_runtime::` (`xtask/src/deploy_guard.rs:28-35`), and no added line in `patch.diff` contains any of them |

Conformance and DST ran on the tree after the `NoSuchContainer` rename. The only change after
that was in `crates/server/tests/multipart_gateway_seam.rs` (the defaults test), which neither
step builds. The `ci` run in the table above is on the final tree except for one later edit to
that test file's module-doc comment (four comment lines). After it I re-ran
`cargo fmt --all -- --check`, `typos`, `cargo clippy -p wyrd-server --all-targets` and the test
(4 passed), and regenerated `patch.diff`.

**Tools: nothing was warn-skipped for a missing tool.** `typos`, `cargo-machete`, `cargo-deny`
and the docs renderer deps (`markdown_it`, `yaml`) are all installed. The brief's
`External dependencies` (`typos`, `docs-renderer`) were both present.

## Commit-readiness

`cargo fmt --all` run over the tree, and the gate's `cargo fmt --all -- --check` passes. `typos`
clean (the gate's `typos` step passes). The target repo has no pre-commit hook config (no
`.pre-commit-config.yaml`, no `.githooks/`); its commit-time conventions are the ones
`cargo xtask ci` checks. DCO sign-off is the publish step's concern, not this patch's.

## Ruled out

- **Six methods on `ObjectGateway`**, **reusing `wyrd_core::multipart` types**, **refusals as
  `GatewayError` variants**, **no default bodies**: all rejected by the brief (Alternatives
  considered); not re-argued.
- **Relying on the trait defaults in the server** instead of explicit bodies: the brief asks for
  explicit methods so later slices replace only a body. Cost of explicit: the six methods at
  `crates/server/src/lib.rs:762-841`, ~80 lines.
- **Unprefixed answer names (`UploadPartAnswer`, …)**: would collide with core's lifecycle-half
  types of the same names in #668's server mapping (see the names table).
- **`Option<u64>` timestamps on the list rows**: core always records them; `Option` would invite
  #668 to emit `None` for a value that always exists.
- **A `NamedPart` struct for Complete's list**: a tuple mirrors core's `(PartNumber, Digest)`
  pairs one to one; a later slice can introduce a struct if XML parsing wants one.
- **A compile-fail test for "the S3 crate requires the trait"**: the workspace has no `trybuild`
  or `compile_fail` doctest harness, and the brief assigns that property to source review.
- **Fixing the `rustls 0.21.12` advisories in this patch**: outside #666's scope (a deny-policy or
  dependency change with its own test); see "Read first" item 2.

## Scratch

Throwaway files only under `/var/tmp/pdca/wyrd-pdca-9c587031/issue_666/pdca-builder-666-redleg`
(the reverted-fix diff, file copies used to restore after each mutation, a temp git index, gate
logs). Per the harness rules I issued no `rm`; the harness owns that root.
