# Build notes — issue 666 / multipart-gateway-seam (iteration 3)

Target: `getwyrd/wyrd` @ `main` = `7bf7eb9` (worktree base; `git ls-remote origin main` still
answers `7bf7eb9` today). `patch.diff` applies cleanly to `7bf7eb9` (`git apply --cached --check`
against a throwaway index read from `7bf7eb9`). All `path:line` below are on the patched tree
unless marked `base`.

This iteration starts from iteration 2's patch (`iteration-v2/patch.diff`). Five of the six files
are byte-identical to iteration 2 (checked by diffing the two patches with the `gateway-core`
hunk removed). The one code change is a unit-test module at the end of
`crates/gateway-core/src/lib.rs` (`:805-822`). The named test file
`crates/server/tests/multipart_gateway_seam.rs` is byte-identical to iteration 2's.

## Read first — one decision for the human

### Leg (4) is red on the base, and this time the declaration is machine-checkable

`cargo xtask ci` fails at `cargo deny --all-features --config deny-all-features.toml check
advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, reached only
through the off-by-default `tikv` feature (`tikv-client 0.4.0 → tonic 0.10 → tokio-rustls 0.24.1
→ rustls 0.21.12`). It fails the same way on the untouched base checkout `../wyrd` (`7bf7eb9`),
run from the harness root exactly as the doctor probe will run it:

```
    ├ ID: RUSTSEC-2026-0346
    ├ ID: RUSTSEC-2026-0349
advisories FAILED
exit=1
```

The patch touches no `Cargo.toml`, no `Cargo.lock` and neither deny config, so no change inside
this slice's scope can turn that step green. Every other step of the gate passes (see "Leg (4)").

**Why iterations 1 and 2 kept looping on this.** Iteration 2 declared it, but the harness could
not check the declaration. `iteration-v2/dependency-adjudication.json` says `unconfirmed`, "no
parseable proposed row names it (a proposed TOML block failed to parse)". Two mistakes on the
builder's side, both mine to fix:

1. The declared name was the long phrase "RustSec advisory database (RUSTSEC-…)", while the
   proposed row's `id` was `wyrd-base-advisories`. The adjudicator matches name to `id`
   exactly, ignoring case (`src/pdca_harness/dependency_halt.py:128-135`).
2. The waiver sketch sat in its own TOML fence above the marker. Its loose
   `{ id = …, reason = … },` lines are not a TOML document, so that fence failed to parse
   (`dependency_halt.py:106-111`).

This round the declared name and the row `id` are both `wyrd-base-advisories`, there is exactly
one TOML fence in this file (the row), and the waiver sketch is in a `text` fence. I checked it
with the harness's own functions (`declared_dependencies`, `proposed_rows`, `_resolve`) on a
draft of this exact marker and row: `names ['wyrd-base-advisories']`, `rows
['wyrd-base-advisories'] malformed False`, `resolved proposed wyrd-base-advisories`. The row's
`cmd` exits 1 on this host today (output above), so the expected verdict is **confirmed**.

**What that means for this round** (`dependency_halt.py:21-25`): the driver skips the reviewer,
the adversary and the gates (recorded N/A) and stops at AWAITING_SIGNOFF with this item open.
So this round's one code change (the `gateway-core` unit tests, 19 lines) gets its independent
review when the run resumes, not now. Everything else in the patch is what iteration 2's Check
reviewed (T4: 0 blocking).

**The decision.** Either:

- **(A) Fix the base on `main` first, then iterate-do.** In its own PR, add the two IDs to the
  reviewed tikv-client waiver list ("THE tikv-client EXPOSURE BOUNDARY (#543)",
  `deny-all-features.toml:50-153`, base), which has seven entries today, each with a reason and a
  review trigger. Sketch, two entries after `deny-all-features.toml:152` (base), plus the boundary
  comment's count "seven" → "nine":

  ```text
      { id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12, `Debug` impls expose TLS secrets; reachable only via tikv-client 0.4.0 behind the off-by-default `tikv` feature; absent from the shipped graph. Same unreleased client-rust#541 story (#543)." },
      { id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12, unauthenticated ServerHello downgrade; same tikv-client-only rustls 0.21 chain as RUSTSEC-2026-0346 (#543)." },
  ```

  Once `main` passes, the row's `cmd` exits 0 and the next attempt runs the full Check band.
- **(B) Waive leg (4) for this slice at sign-off**, on the evidence below that every other gate
  step passes on the patched tree.

I did not put the waiver in this patch. A security waiver is a policy change, and the rubric
sends out-of-scope fixes to their own PR ("Out of scope: … a decline-with-issue-reference, not an
in-PR fix"). Two independent reviewers in iteration 2 also framed this as the human's call.

NEEDS-HUMAN external dependency: wyrd-base-advisories — `cargo xtask ci` fails on base 7bf7eb9 at its all-features `cargo deny … check advisories` step (RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against rustls 0.21.12, reached only through the optional tikv-client 0.4.0), so leg (4) "the full gate is green" cannot be produced by this patch without a security-waiver or dependency change on main; every other gate step passes on the patched tree

```toml
[[doctor.checks]]
group = "engine"
id    = "wyrd-base-advisories"
cmd   = "bash -c '. engine/lib/ensure-cargo.sh && ensure_cargo && cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories'"
hint  = "a RustSec advisory has landed against the base Cargo.lock: on main, waive it in deny-all-features.toml (the #543 tikv-client exposure boundary) or bump the crate, then iterate-do"
level = "WARN"
```

`level = "WARN"`: the slice builds and every other step passes without it; only the gate's
all-features advisory step is blocked. `cd ../wyrd` is the target's primary checkout, from the
harness root (the probe's cwd, `src/pdca_harness/doctor.py:312`); it is on `7bf7eb9`, the same
lockfile as this worktree. `ensure-cargo.sh` is the same cargo lookup the gate wrapper uses
(`engine/xtask.sh:29-33`).

## What changed in this iteration, and why

### 1. The surviving mutants are killed (carry-forward: "C5 … 10 missed")

All 10 survivors were `NamedParts::as_slice` (`crates/gateway-core/src/lib.rs:669-671`) and
`NamedParts::into_vec` (`:674-676`) replaced with fixed values. They survived because
`cargo mutants` runs only the mutated package's tests, and `wyrd-gateway-core` had none. The
server-crate test that does call both (`an_empty_complete_list_cannot_reach_the_seam`) is never
run for a `gateway-core` mutant.

Fix: a `#[cfg(test)] mod tests` at `crates/gateway-core/src/lib.rs:805-822`, two tests:
`named_parts_refuses_an_empty_list` (`:810`) and `named_parts_keeps_every_pair_in_the_order_sent`
(`:815`), which checks both accessors against a two-pair list sent out of order (`(3, "c"), (1,
"a")`), so none of the mutants' fixed values (`[]`, `[(0, "")]`, `[(1, "xyzzy")]`, …) can match.

Red → green, through the gate's own script (`PDCA_BUNDLE=… PDCA_WORKTREE=…
scripts/mutants-in-diff`, i.e. `cargo mutants --in-diff patch.diff --no-shuffle
--cap-lints=true`):

| | iteration 2 (`iteration-v2/gate-logs/C5-mutants.log`) | iteration 3 (this patch) |
|---|---|---|
| result | `16 mutants tested in 72s: 10 missed, 6 unviable` | `16 mutants tested in 63s: 10 caught, 6 unviable` |

`mutants.out/caught.txt` now lists all ten `as_slice` / `into_vec` mutants. The 6 unviable ones
are `Ok(Default::default())` for each server stub (`crates/server/src/lib.rs:770`, `:789`, `:803`,
`:816`, `:829`, `:837`). They cannot compile because no answer enum derives `Default`, on
purpose: there is no "default" multipart answer. A wrong answer from a stub is still caught, by
leg (1) (red (i) below). `NamedParts::new` produced no mutant in either run.

### 2. The shared HTTP parser (carry-forward advisory: "Extract a shared test-support parser")

**Tried, built, green, then backed out, because the harness's C4-verify would misreport it.**

I did the extraction: the strict parser moved to `crates/server/tests/support/http_response.rs`
(144 lines), included by both suites with `#[path = "support/http_response.rs"] mod
http_response;`. `s3_chunk_size_flag.rs` went −119/+4 and the seam test lost its copy. Both
suites passed (`s3_chunk_size_flag` 4/4, `multipart_gateway_seam` 7/7), and loosening the
shared parser turned the seam test's torn-framing fixture red, so the fixtures guarded both
callers.

The cost that made me back it out, measured: the instance's C4-verify script classifies any
added `.rs` under a `tests/` directory as a test target. Its `_is_test_file` is a shell `case`
on `*/tests/*.rs` (`engine/scripts/run-verify.sh:144`), and in a `case` pattern `*` also matches
`/`, so `crates/server/tests/support/http_response.rs` counts. It then asks cargo for
`--test http_response` (`run-verify.sh:408`). Cargo has no such target (only `tests/*.rs` and
`tests/*/main.rs` are targets), and the GREEN leg dies before running anything:

```
$ cargo test --quiet -p wyrd-server --test multipart_gateway_seam -p wyrd-server --test http_response
error: no test target named `http_response` in `wyrd-server`, ... packages
rc=101
```

`run-verify.sh:478-481` reports that as "FAIL — the bundle's test does not pass with the fix
applied", where the brief pre-declares UNVERIFIABLE (77). A false FAIL would look like a real
defect at sign-off and would likely send the next round off to "fix" it.

The layouts that avoid the misclassification would bend the target repo around a harness quirk:
a top-level `tests/http_response.rs` that is both its own test target and a `#[path]` module
(its tests then run in three binaries), or a non-standard `crates/server/test-support/` directory.
The repo's own pattern for shared test code is `tests/support/` (`crates/dst/tests/support/mod.rs`,
`crates/metadata-fdb/tests/support/mod.rs`), which is the one that trips the gate.

So the seam test keeps its own strict copy, as iteration 2 shipped it. `crates/server/tests/`
already has a per-file parser in six suites (`s3_http_wire.rs`, `s3_head_object.rs`,
`s3_copy_object_guard.rs`, `s3_streaming_trailer.rs`, `s3_list_objects.rs`,
`s3_chunk_size_flag.rs`). This is a recorded rejection under the rubric's definition of done,
for an item the reviewer marked "advisory cleanup, not a correctness blocker".

**For the human (harness side, not this patch):** `_is_test_file` at `run-verify.sh:144` should
not count `tests/<dir>/*.rs` as a target. A file `tests/<name>/main.rs` is the target `<name>`,
not `main`, and any other `tests/<dir>/*.rs` is a support module. Once that is fixed, the
extraction above is a mechanical follow-up (move `Response`, `find`, `parse_response`,
`decode_chunked` out of `multipart_gateway_seam.rs:491-624` into `tests/support/`, and delete
`s3_chunk_size_flag.rs:223-226` and `:286-398`, base).

### 3. Not changed

- **C2** ("accept the declared compile-only baseline") is a sign-off decision the brief
  pre-declares (Verification posture (a)). The reverted-fix run below reproduces it.
- The other deferred findings in `deferred-findings.json` are either this same advisory item
  (three copies), Plan-review findings the brief already absorbed (typed homes for validation
  failures, leg-2/leg-3 claims, `cargo xtask ci` as leg 4), or a note about trusting iteration
  2's T4 row, which is the human's to weigh.

## Chosen names (open question 1) — for #667–#670 to cite

All in `wyrd_gateway_core`. Unchanged from iteration 2.

| Seam type (`gateway-core`) | Mirrors core (`crates/core/src/multipart.rs`) |
|---|---|
| `MultipartGateway` (trait, `:711`) | — |
| `MultipartRefusal` (`:488`) | `Refusal` + 2 seam arms |
| `MultipartBackpressure` | `Backpressure` |
| `MultipartInvalidPart` | `InvalidPart` |
| `MultipartCreateAnswer { Created { upload_id: String }, Refused }` | `CreateOutcome` |
| `MultipartUploadPartAnswer { Committed { part_number: u32, etag: String }, Refused }` | `UploadPartOutcome` |
| `MultipartCompleteAnswer { Published(MultipartPublication), AlreadyCompleted(MultipartPublication), Refused }` | `CompleteOutcome` |
| `MultipartAbortAnswer { Fenced, AlreadyAborting, Refused }` | `AbortOutcome` |
| `MultipartListPartsAnswer { Listed(Vec<ListedPart>), Refused }` | — |
| `MultipartListUploadsAnswer { Listed(Vec<ListedUpload>), Refused }` | — |
| `MultipartPublication { etag: String, modified: u64 }` | `Publication` |
| `ListedPart { part_number: u32, etag: String, size: u64, modified: u64 }` | from `PartRecord` |
| `ListedUpload { key: String, upload_id: String, initiated: u64 }` | from `SessionRecord` |
| `NamedParts` (`:656`; `new(Vec<(u32, String)>) -> Option<Self>` `:660`, `as_slice` `:669`, `into_vec` `:674`) | core's `&[(PartNumber, Digest)]`, non-empty |

`MultipartRefusal` arms: `NoSuchUpload`, `NoSuchContainer`, `OperationAborted`,
`InvalidPart { part_number: u32, reason }`, `EntityTooLarge { chunks, limit }`,
`SlowDown { pressure }`, `ProfileSkew`, `CompleteAttemptsExhausted`, and the seam's own
`PartNumberOutOfRange { part_number: u32 }` and `NotImplemented`. The "not implemented" answer of
verb V is `MultipartVAnswer::Refused(MultipartRefusal::NotImplemented)`.

For #670: build `NamedParts` from the decoded XML list; a `None` is S3's `400 MalformedXML`,
answered before the seam is called. For #669/#670: check the part-number range before parsing the
upload id (trait doc `crates/gateway-core/src/lib.rs:694-703`).

Earlier mapping decisions still hold (prefixed names to avoid core's `UploadPartAnswer` etc.;
core's `NoSuchBucket` is the seam's `NoSuchContainer`; `digest` → `etag`; `ProfileSkew` without
payload; `MultipartPublication` drops inode/version; list timestamps are `u64` epoch millis,
which #668 must confirm against core's clock source; uploads listed by key, then upload id).

## Where each change is (patched tree)

- Trait, vocabulary and `NamedParts`: `crates/gateway-core/src/lib.rs:433-803`; unit tests
  `:805-822` (new this round).
- Composition root: `impl<M, C, Co> MultipartGateway for Gateway<M, C, Co>`,
  `crates/server/src/lib.rs:757`, one explicit stub per verb, each with a one-line comment naming
  the slice that replaces it (`:769` #668 create, `:788` #669 upload part, `:802` #670 complete,
  `:815` / `:828` / `:836` #668 abort / list parts / list uploads); import at `:47`.
- S3 crate bounds (for Check's source review of leg 2): `crates/gateway-s3/src/lib.rs:229`
  (`S3Gateway::new`), `:797`, `:1527`, `:1638`, `:2002`, test helper `:5148`; import `:79`.
  Empty impls for the test doubles: `NoGateway` `:3711` and `:4007`, `StoredMetaGateway` `:4697`,
  `DefaultRangeSeamGateway` `:5203`, `VersionSkewGateway` `:5325`. The denylist and both guards
  are untouched.
- `crates/server/tests/request_capacity_planes.rs`: `FaultyGateway` `:195`,
  `MidStreamFaultGateway` `:266`, helper bound `:362`, import `:78`.
- Docs currency: `docs/design/architecture/05-building-block-view.md:137`.

## The test — `crates/server/tests/multipart_gateway_seam.rs` (new, 7 tests, unchanged from iteration 2)

- **Leg (1)** `the_composition_root_answers_every_multipart_verb_not_implemented` (`:109`): a
  real `Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>` over
  `RedbMetadataStore::in_memory` (`real_gateway`, `:98`), passed to
  `assert_every_verb_not_implemented` (`:141`), which calls all six methods fully qualified,
  `.expect`s `Ok`, and `assert_eq!`s the exact `Refused(NotImplemented)` value per verb.
- `the_trait_defaults_answer_every_multipart_verb_not_implemented` (`:121`): the same helper over
  an empty `impl MultipartGateway for NoMultipart {}`, so the six default bodies run too.
- `an_empty_complete_list_cannot_reach_the_seam` (`:129`).
- **Leg (2)** `the_real_gateway_meets_the_s3_crates_gateway_bound` (`:243`): `start_s3<G:
  ObjectGateway + ContainerGateway + MultipartGateway>` (`:225`) takes the real `Gateway`, calls
  `S3Gateway::new`, serves on loopback; all six forms pass `judge_501`. As the brief says, this
  shows the real `Gateway` meets the triple bound, not that the S3 crate requires it; that is
  the source review of the bound lines listed above.
- **Leg (3)** `every_multipart_form_still_answers_501_without_entering_the_seam` (`:388`): the
  `Sentinel` (`:259`) counts calls in two struct-field counters (not statics); all six forms pass
  `judge_501`; both counters must be 0 afterwards.
- `the_wire_oracle_accepts_a_well_framed_501` (`:675`) and `the_wire_oracle_refuses_torn_framing`
  (`:690`): the oracle self-tests (18 torn variants, each still carrying every asserted XML
  fragment, each required to fail at the framing step).

Plus, in the patch but outside the named file: the two `gateway-core` unit tests (item 1).

## Running it

The project runner (`./engine/xtask.sh`, i.e. `cargo xtask`) has no single-test subcommand
(`xtask/src/main.rs:119-152`). For the quick red/green passes I ran `timeout 1800 cargo test -p
wyrd-server --test multipart_gateway_seam` in the worktree, always under `timeout`. The full gate,
conformance and statics ran through the runner; the mutants check ran through the gate's own
script.

Green on the final tree (from the full gate's `cargo test` step):

```
     Running tests/multipart_gateway_seam.rs (target/debug/deps/multipart_gateway_seam-ea2c9e867ac06f1c)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test tests::named_parts_refuses_an_empty_list ... ok
test tests::named_parts_keeps_every_pair_in_the_order_sent ... ok
```

### Demonstrated red — every run on this iteration's final tree

Each mutation was made on a saved copy, the test run, the file restored, and `cmp` used to check
the restore. After all of them, `git diff | cmp - patch.diff` matched.

**(i) A server stub answers something other than "not implemented"** (the brief's required
demonstration). `Gateway::complete_multipart_upload` (`crates/server/src/lib.rs:804-806`)
temporarily answered `Refused(NoSuchUpload)`. Only leg (1) went red:

```
test the_composition_root_answers_every_multipart_verb_not_implemented ... FAILED
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:173:5:
  left: Refused(NoSuchUpload)
 right: Refused(NotImplemented)
test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

**(ii) The wire oracle stops checking `content-length`.** `parse_response`'s length check
(`multipart_gateway_seam.rs:556`) temporarily became `if false && …`:

```
test the_wire_oracle_refuses_torn_framing ... FAILED
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:804:22:
fixed: body shorter than the declared content-length: expected a framing failure, got Ok(())
```

**(iii) A multipart form enters the seam and still answers the same 501.** Just inside the
object-path subresource guard (`crates/gateway-s3/src/lib.rs:1777`) I temporarily added `if sub
== "uploads" { let _ = state.gateway.create_multipart_upload("b", "k", None).await; }`. Every
status and XML check still passed and leg (2) stayed green; only leg (3)'s counter caught it:

```
test every_multipart_form_still_answers_501_without_entering_the_seam ... FAILED
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:398:5:
  left: 1
 right: 0
```

**(iv) `NamedParts` accessors.** The mutants table in item 1: 10 missed without the new unit
tests, 10 caught with them.

### Reverted-fix run (what C4-verify's red leg will see)

The five modified files reverted with `git checkout --` (the new test file kept), the test run,
the files restored from saved copies (`git diff | cmp - patch.diff` matched):

```
error[E0432]: unresolved imports `wyrd_gateway_core::MultipartAbortAnswer`, … `wyrd_gateway_core::MultipartGateway`, … `wyrd_gateway_core::NamedParts`
error: could not compile `wyrd-server` (test "multipart_gateway_seam") due to 1 previous error
```

So C4-verify will report UNVERIFIABLE (77), as the brief pre-declares. (With the parser
extraction of item 2 it would have reported FAIL at the green leg instead.)

## Self-refutation (forced)

- **(a) Genuine red? Yes, with the caveat the brief pre-declares.** Reverting the whole fix makes
  the test fail to compile (E0432) because the trait is new, so the whole-revert red is
  compile-only. The behavioural reds are the mutations above, all re-run on this tree: a server
  stub answering anything but `NotImplemented` turns leg (1) red; a loose oracle turns the
  oracle self-test red; routing a form into the seam turns leg (3) red while every status stays
  `501`; mutating the `NamedParts` accessors is caught by the new unit tests (0 of 10 survive).
  Leg (3) is green on the base by design (it pins "routing untouched").
- **(b) Production path? Yes.** Leg (1) calls the real `impl MultipartGateway for
  wyrd_server::Gateway` (`crates/server/src/lib.rs:757`); the defaults test runs the trait's real
  provided bodies; the `gateway-core` unit tests call the real `NamedParts` in its own crate. Legs
  (2) and (3) drive the real `S3Gateway` router over a real loopback socket with real SigV4
  signing. The sentinel stands in only for the backend behind the seam, which is exactly what
  leg (3) watches. The oracle self-tests drive the same `judge_501` the live legs use.
- **(c) Fixture includes the fault? Yes.** The sentinel answers `NotImplemented`, which is exactly
  what would make "entered the seam, mapped to 501" look right on the wire, and the counter
  catches it (red iii). All six forms, including bucket-scoped `GET /b?uploads`, are in
  `MULTIPART_FORMS`. The torn-framing variants each keep every asserted XML fragment (the test
  checks that before judging). The `NamedParts` unit test uses an out-of-order two-pair list,
  which none of the mutants' fixed values can equal.

## Leg (4) — the full gate

**Not green as one command, because of the base advisories in "Read first".** Every other step
passes on the patched tree.

`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l2 ./engine/xtask.sh ci` (= `cargo xtask ci`), on the
final tree, exit 1:

| Step | Result |
|---|---|
| `typos` | pass |
| `lint_docs.py` | pass (`lint_docs: OK`) |
| `render_site.py --check` | pass (`render_site: link audit OK`) |
| gitlink / unsafe / blackbox guards | pass |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --exclude wyrd-dst --all-targets` | pass, no warnings (re-checked `wyrd-gateway-core`, `wyrd-gateway-s3` with its `#[cfg(test)]` doubles, `wyrd-server`, `wyrd-validate`) |
| `cargo build --workspace --exclude wyrd-dst --all-targets` | pass |
| `cargo test --workspace --exclude wyrd-dst` | pass: 197 test binaries, 1,685 tests, 0 failed (iteration 2: 1,683; the 2 new ones are the `gateway-core` tests) |
| `cargo-machete` | pass |
| `cargo deny check` | pass (`advisories ok, bans ok, licenses ok, sources ok`) |
| `cargo deny --all-features --config deny-all-features.toml check advisories` | **FAIL**: RUSTSEC-2026-0346, RUSTSEC-2026-0349 on `rustls 0.21.12`, the same as on base `../wyrd` |
| conformance, statics, orchestrator guard, DST | not reached by `ci` (it stops at the first failure, `xtask/src/main.rs:1671-1690`); see below |

The unreached steps:

| Step | How | Result |
|---|---|---|
| conformance | `./engine/xtask.sh conformance`, final tree | pass (`5 valid + 6 invalid vectors pass`) |
| statics | `./engine/xtask.sh statics`, final tree | pass (`no DST-reachable shared mutable global state (ADR-0035)`) |
| orchestrator guard | its scan runs as `scan_dir_is_green_over_the_real_workspace_crates` inside the gate's `cargo test` step | pass (`… ok`) |
| DST | **not re-run this round** | `crates/dst/Cargo.toml:15-40` lists none of the touched crates, and none of its dependencies (`wyrd-core`, `wyrd-traits`, `wyrd-metadata-redb`, `wyrd-metadata-conformance`, `wyrd-chunkstore-fs`, `wyrd-chunkstore-grpc`, `wyrd-chunk-format`, `wyrd-testkit`) depends on `wyrd-gateway-core`, `wyrd-gateway-s3` or `wyrd-server` (only `gateway-s3`, `metadata-fdb`, `server` and `validate` name them). So no file this patch touches is in DST's build. Iteration 2 ran it green (10 binaries, 85 tests) on the same production code; this round adds only a `#[cfg(test)]` module to `gateway-core`. |

**Tools: nothing was warn-skipped for a missing tool.** `typos`, `cargo-machete` and
`cargo-deny` are installed, and the docs renderer ran (`link audit OK`). The brief's `External
dependencies` (`typos`, `docs-renderer`) were both present.

## Commit-readiness

`cargo fmt --all` run over the tree and `cargo fmt --all -- --check` passes (also inside the
gate). The target has no pre-commit hook config (no `.pre-commit-config.yaml`, no `.githooks/`);
its commit-time checks are the ones `cargo xtask ci` runs. DCO sign-off is the publish step's.

## Self-review against the rubric (this round's delta)

The delta is 19 lines of `#[cfg(test)]` code in `gateway-core`. It reads no clock, adds no
global state, no await, no port, API, flag or persisted field (so no docs change). The crate root
keeps `#![forbid(unsafe_code)]`. The rest of the patch is iteration 2's, which T4 passed with 0
blocking findings.

## Ruled out

- **Waiving the two rustls advisories in this PR**: a security-policy change for its own PR (see
  "Read first", option A).
- **Extracting the parser to `tests/support/`**: built and green, backed out for the measured
  C4-verify false FAIL (item 2). **A self-hosting `tests/http_response.rs`** or a **non-standard
  `test-support/` directory**: would shape the target repo around a harness quirk.
- **A `gateway-core` test for the six default bodies**: the server-crate test already runs them,
  and no viable mutant exists there (the mutants run produced none for the default bodies).
- Everything iterations 1 and 2 ruled out (six methods on `ObjectGateway`, core types in the
  seam, refusals as `GatewayError`, no default bodies, unprefixed names, `Option` timestamps, a
  compile-fail harness, a doc-only precondition for the empty Complete list, a new refusal arm
  for it, making `consistency_observable.rs`'s parser public) still stands.

## Scratch

Throwaway files only under `/var/tmp/pdca/wyrd-pdca-9c587031/issue_666/pdca-builder-666-iter3`
(saved copies used to restore after each mutation, the gate, mutants, conformance and statics
logs, a throwaway git index, the iteration-2 file extract, the marker draft). `cargo mutants`
left its usual git-ignored `mutants.out/` in the worktree, as the C5 gate does. Per the harness
rules I issued no `rm` on any harness root.
