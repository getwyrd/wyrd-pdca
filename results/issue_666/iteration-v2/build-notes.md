# Build notes — issue 666 / multipart-gateway-seam (iteration 2)

Target: `getwyrd/wyrd` @ `main` = `7bf7eb9` (worktree `HEAD`; `origin/main` is the same commit).
`patch.diff` applies cleanly to `7bf7eb9` (`git apply --cached --check` against a throwaway index
read from `7bf7eb9`). All `path:line` below are on the patched tree unless marked `base`.

This iteration starts from iteration 1's patch (`iteration-v1/patch.diff`) and changes what the
carry-forward asked for. The seam, the server stubs, the S3 bounds, the implementer updates and
the docs paragraph are iteration 1's, unchanged except where listed under "What changed in this
iteration".

## Read first — one thing for the human

### Leg (4) is still red on base, for a reason outside this patch

`cargo xtask ci` fails at `cargo deny --all-features --config deny-all-features.toml check
advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (`Cargo.lock:302`),
reached only through the off-by-default `tikv` feature (`tikv-client 0.4.0 → tonic 0.10 →
tokio-rustls 0.24.1 → rustls 0.21.12`). I re-checked it this iteration on an untouched export of
the base commit (`git archive 7bf7eb9`, under my scratch dir):

```
    ├ ID: RUSTSEC-2026-0346
    ├ ID: RUSTSEC-2026-0349
advisories FAILED
exit=1
```

The patch touches no `Cargo.toml`, no `Cargo.lock`, and neither deny config, which are the only
inputs that step reads besides the advisory database. So it fails the same way on every bundle
cut from `7bf7eb9`.

I did **not** add the two IDs to the waiver list in this PR. The repo has a deliberate, reviewed
policy for exactly this chain ("THE tikv-client EXPOSURE BOUNDARY (#543)",
`deny-all-features.toml:50-153`, seven entries today, each with a reason and a review trigger),
and the two new advisories fit it. But a security waiver is a policy change that belongs in its
own PR on `main`, not inside a multipart-seam PR (the rubric's "Out of scope: decline with an
issue reference, not an in-PR fix"). The change is small. Sketch for a separate PR, two entries
after `deny-all-features.toml:152`:

```toml
    { id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12, `Debug` impls expose TLS secrets; reachable only via tikv-client 0.4.0 behind the off-by-default `tikv` feature; absent from the shipped graph. Same unreleased client-rust#541 story (#543)." },
    { id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12, unauthenticated ServerHello downgrade; same tikv-client-only rustls 0.21 chain as RUSTSEC-2026-0346 (#543)." },
```

(plus the boundary comment's count "seven" → "nine"). That is the human's decision, so leg (4)
stays an open sign-off item, recorded below with the full step-by-step result.

NEEDS-HUMAN external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9 (reproduced on a clean export), so leg (4) "the full gate is green" cannot be produced by this patch without a security-waiver change that belongs in its own PR on main; every other gate step passes on the patched tree (see "Leg (4)")

```toml
[[doctor.checks]]
id    = "wyrd-base-advisories"
cmd   = "cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories"
hint  = "a new RustSec advisory has landed against the base lockfile: waive it in deny-all-features.toml (the #543 tikv-client boundary) or bump the crate, on main, before cycling bundles"
level = "WARN"
```

## What changed in this iteration, and why

### 1. An empty Complete list now cannot reach the seam (carry-forward: "An empty Complete list has no typed answer")

Core refuses an empty named-part list with `RecordError::NoPartsNamed`
(`crates/core/src/multipart.rs:531`, raised at `:4096`). Iteration 1's seam took
`Vec<(u32, String)>`, so #670 would have had no typed answer for `vec![]`.

The carry-forward proposed a doc precondition ("`named_parts` is non-empty"). I made the
precondition a type instead:

- `pub struct NamedParts(Vec<(u32, String)>)`, `crates/gateway-core/src/lib.rs:656`, with a
  private field, so the only way to build one is `NamedParts::new` (`:660`), which answers
  `None` for an empty list. Accessors `as_slice` / `into_vec`. Doc at `:646-655` says why: the
  wire layer finds the empty list while decoding the request and answers it as malformed there
  (S3: `MalformedXML`), as ADR-0045 puts structural checks at decode.
- `complete_multipart_upload` takes `named_parts: NamedParts` (`:753-762`); the server impl
  (`crates/server/src/lib.rs:800`, import `:47`) and the test's sentinel follow.

Why the type and not only a doc line: a doc precondition leaves an implementer that *receives* an
empty list with the same three bad options the finding listed (invent a part number, return
`Err`, or change the trait). The type removes the case. Cost, counted: the struct, its `impl` and
its doc are 32 lines in `gateway-core` (`:646-677`); the signature changes are 1 line each in the trait, the
server impl and the sentinel; the test gains 9 lines (`an_empty_complete_list_cannot_reach_the_seam`).
A doc-only fix would have been about 3 lines. The answer enums are unchanged, so the brief's
fixed shape (values, core mirrored one to one, plus `PartNumberOutOfRange` and `NotImplemented`,
no `#[non_exhaustive]`) holds.

### 2. Which arm wins when `upload_part` has two faults (same carry-forward item)

The trait doc (`crates/gateway-core/src/lib.rs:694-703`) now says: when a request has more than
one fault, the part-number range check wins, because it needs only the argument and so runs
before the upload id is parsed or anything is looked up. `upload_part` with an out-of-range part
number answers `PartNumberOutOfRange` whatever its upload id; in Complete's list the first
out-of-range number is the one carried. This also matches how S3 servers order it (part number is
checked as a request argument before the upload lookup).

### 3. The wire oracle now proves framing before it reads XML (T4 findings ×4, C5 finding)

Iteration 1's `send_signed` ignored `content-length` and its `dechunk` treated a bad size as the
end, returned partial bytes on truncation and never required the last chunk or the final CRLF.
The fix, in `crates/server/tests/multipart_gateway_seam.rs`:

- `send_signed` (`:454`) now returns the raw bytes; `judge_501` (`:434`) is the whole oracle:
  `parse_response` first, then status `501`, then `<Code>NotImplemented</Code>`, then the
  subresource name. Every check returns an `Err` naming what failed, and `assert_refused_501`
  (`:414`) panics with it and the raw response.
- `parse_response` (`:506`) and `decode_chunked` (`:574`) are the strict checks from the cited
  peer, `crates/server/tests/s3_chunk_size_flag.rs:294` and `:357`, with three tightenings: a
  repeated `transfer-encoding` is refused (`:542`), the chunk-size `+ 2` is `checked_add`
  (`:589-591`), and a trailer line without a colon is refused (`:604-618`).
- Two new self-tests drive `judge_501` itself:
  `the_wire_oracle_accepts_a_well_framed_501` (`:675`: fixed-length, chunked, chunked with a
  trailer, all `Ok`) and `the_wire_oracle_refuses_torn_framing` (`:690`: 18 variants, each one
  framing mutation of a passing response, each asserted to still contain every XML fragment the
  oracle checks, each required to fail with `malformed response:`, i.e. at the framing step).
  The variants cover every case the reviewers named: short and long fixed bodies against the
  declared length, repeated and signed `content-length`, no framing, both framings, repeated
  `transfer-encoding`, EOF after the first chunk, EOF before the last chunk, last chunk without
  the final CRLF, bytes after it, unparsable and signed chunk sizes, a bad trailer line, a chunk
  shorter than its size, chunk data not followed by CRLF, no CRLF CRLF after the head, and a bad
  status code.

Copy vs. extract: the carry-forward said "extract/reuse". I copied the peer's two functions into
the new file rather than moving them into a shared `crates/server/tests/support/` module. Cost
of the extract, counted: a new support file (~115 lines), `s3_chunk_size_flag.rs` −113/+3, the new
file +3 instead of +115. The net line count is about the same; what decides it is that the
extract edits a test file outside this slice's scope (the brief's scope list does not include
it, and it was not in the batch's conflict analysis), and `crates/server/tests/` has no shared
support module today (every `s3_*.rs` file carries its own parser: `s3_http_wire.rs:141`,
`s3_head_object.rs:136`, `s3_copy_object_guard.rs:125`, `s3_streaming_trailer.rs:168`, …), so
the copy follows house style.

The live responses go through the strict chunked path. I trapped `decode_chunked` temporarily
(panic unless the first chunk-size line starts with `8`); both live legs hit it, and the bytes
show the server's real framing:

```
LIVE CHUNKED PATH: "D1\r\n<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>NotImplemented</Code><Message>the `uploads` S3 subresource/operation is not supported</Message><RequestId>b4ab…</RequestId></Error>\r\n0\r\n\r\n"
```

Restored byte-for-byte afterwards (`cmp` against a saved copy).

### 4. Smaller

- The served S3 task is now held in `Served` (`:211-221`), whose `Drop` aborts it, so no server
  outlives its test (rubric: spawned helper tasks are aborted on drop). Iteration 1 detached it.
- The 30 s exchange bound is a named const, `HTTP_TIMEOUT` (`:74`).

### Not changed

- C2 ("accept or reject the compile-only baseline evidence") is a sign-off decision. The brief
  pre-declares it (Verification posture (a)): the test names a new public trait, so with the fix
  reverted it cannot compile, and C4-verify will report UNVERIFIABLE (77). The behavioural red
  evidence is the three production mutations below.
- The other three deferred findings in `deferred-findings.json` were Plan-review findings the
  brief already absorbed (typed home for validation failures, leg-2/leg-3 claims, `cargo xtask
  ci` as leg 4); iteration 1 built to them and nothing here changes that.

## Chosen names (open question 1) — for #667–#670 to cite

All in `wyrd_gateway_core`. Unchanged from iteration 1 except the new `NamedParts`.

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
| **`NamedParts`** (`:656`; `new(Vec<(u32, String)>) -> Option<Self>`, `as_slice`, `into_vec`) | core's `&[(PartNumber, Digest)]`, non-empty |

`MultipartRefusal` arms: `NoSuchUpload`, `NoSuchContainer`, `OperationAborted`,
`InvalidPart { part_number: u32, reason }`, `EntityTooLarge { chunks, limit }`,
`SlowDown { pressure }`, `ProfileSkew`, `CompleteAttemptsExhausted`, and the seam's own
`PartNumberOutOfRange { part_number: u32 }` and `NotImplemented`. The "not implemented" answer of
verb V is `MultipartVAnswer::Refused(MultipartRefusal::NotImplemented)`.

For #670: build `NamedParts` from the decoded XML list; a `None` is S3's `400 MalformedXML`,
answered before the seam is called. For #669/#670: check the part-number range before parsing the
upload id (trait doc `:694-703`).

Iteration 1's mapping decisions still hold (prefixed names to avoid core's `UploadPartAnswer`
etc.; core's `NoSuchBucket` is the seam's `NoSuchContainer`; `digest` → `etag`; `ProfileSkew`
without payload; `MultipartPublication` drops inode/version; list timestamps `u64` epoch millis,
which #668 must confirm against core's clock source; uploads listed by key then upload id).

## The test — `crates/server/tests/multipart_gateway_seam.rs` (new, 7 tests)

- **Leg (1)** `the_composition_root_answers_every_multipart_verb_not_implemented` (`:109`): a real
  `Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>` over `RedbMetadataStore::in_memory`
  (`real_gateway`, `:98`), handed to `assert_every_verb_not_implemented` (`:141`), which calls all
  six methods fully qualified, `.expect`s `Ok`, and `assert_eq!`s the exact
  `Refused(NotImplemented)` value per verb.
- `the_trait_defaults_answer_every_multipart_verb_not_implemented` (`:121`): same helper over an
  empty `impl MultipartGateway for NoMultipart {}`, so the six default bodies are executed too.
- `an_empty_complete_list_cannot_reach_the_seam` (`:129`): `NamedParts::new(vec![])` is `None`;
  a non-empty list is kept in the order sent.
- **Leg (2)** `the_real_gateway_meets_the_s3_crates_gateway_bound` (`:243`): `start_s3<G:
  ObjectGateway + ContainerGateway + MultipartGateway>` (`:225`) takes the real `Gateway`, calls
  `S3Gateway::new`, serves on loopback; all six forms pass `judge_501`. As the brief says, this
  proves the real `Gateway` meets the triple bound, not that the S3 crate requires it; the bound
  lines for source review are `crates/gateway-s3/src/lib.rs:229` (`S3Gateway::new`), `:797`,
  `:1527`, `:1638`, `:2002`, and the test helper `:5148`.
- **Leg (3)** `every_multipart_form_still_answers_501_without_entering_the_seam` (`:388`): the
  `Sentinel` (`:259`) counts calls in two struct-field counters (not statics); all six forms pass
  `judge_501`; both counters must be 0 afterwards.
- `the_wire_oracle_accepts_a_well_framed_501` (`:675`) and `the_wire_oracle_refuses_torn_framing`
  (`:690`): see item 3 above.

## Running it

The project runner (`./engine/xtask.sh`, i.e. `cargo xtask`) has no single-test subcommand
(`xtask/src/main.rs:116-150`). For the quick red/green passes I ran
`timeout 1800 cargo test -p wyrd-server --test multipart_gateway_seam` in the worktree, always
under `timeout`. The full gate ran through the runner (leg 4 below).

Green on the final tree:

```
test an_empty_complete_list_cannot_reach_the_seam ... ok
test the_trait_defaults_answer_every_multipart_verb_not_implemented ... ok
test the_wire_oracle_accepts_a_well_framed_501 ... ok
test the_wire_oracle_refuses_torn_framing ... ok
test every_multipart_form_still_answers_501_without_entering_the_seam ... ok
test the_composition_root_answers_every_multipart_verb_not_implemented ... ok
test the_real_gateway_meets_the_s3_crates_gateway_bound ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Demonstrated red (the brief's Verification posture) — all on this iteration's tree

Each mutation was made on a saved copy, the test run, and the file restored byte-for-byte (`cmp`).

**(i) A server method answers something other than "not implemented".**
`Gateway::complete_multipart_upload` (`crates/server/src/lib.rs:802-806`) temporarily answered
`Refused(NoSuchUpload)`. Only leg (1) went red, naming the verb:

```
test the_composition_root_answers_every_multipart_verb_not_implemented ... FAILED
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:173:5:
assertion `left == right` failed: complete_multipart_upload
  left: Refused(NoSuchUpload)
 right: Refused(NotImplemented)
test result: FAILED. 6 passed; 1 failed
```

**(ii) `NamedParts` accepts an empty list.** `NamedParts::new` (`crates/gateway-core/src/lib.rs:660`)
temporarily returned `Some(Self(parts))` unconditionally:

```
test an_empty_complete_list_cannot_reach_the_seam ... FAILED
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:130:5:
assertion `left == right` failed
  left: Some(NamedParts([]))
 right: None
test result: FAILED. 6 passed; 1 failed
```

**(iii) A multipart form enters the seam and still answers the same 501.** In `dispatch`, just
inside the object-path subresource guard (`crates/gateway-s3/src/lib.rs:1777`), I temporarily
called `state.gateway.create_multipart_upload("b", "k", None).await` for `sub == "uploads"` and
then returned the unchanged 501. Every status and XML check still passed, and leg (2) stayed
green; only leg (3)'s counter caught it:

```
test every_multipart_form_still_answers_501_without_entering_the_seam ... FAILED
test the_real_gateway_meets_the_s3_crates_gateway_bound ... ok
thread '…' panicked at crates/server/tests/multipart_gateway_seam.rs:398:5:
assertion `left == right` failed: a multipart form entered the MultipartGateway seam; routing is #667's, not this slice's
  left: 1
 right: 0
```

**(iv) The oracle self-test binds.** With `judge_501` temporarily switched to iteration 1's loose
parser (`dechunk` with `unwrap_or(0)`, no `content-length` check), the self-test went red, and a
per-case run showed what the loose oracle did with the 18 torn variants: **15 passed as a clean
501** (`Ok(())`), 2 failed but at the wrong step (`expected the NotImplemented code`,
`expected 501, got 0`), and 1 made the loose parser panic. With the strict parser all 18 fail at
`malformed response:`.

```
test the_wire_oracle_refuses_torn_framing ... FAILED
thread '…' panicked at …: fixed: body shorter than the declared content-length: expected a framing failure, got Ok(())
```

### Reverted-fix run (what C4-verify will see)

Every production change reverted (`git checkout -- crates docs`, keeping only the new test file),
then restored from a saved diff (`git diff | cmp` against it):

```
error[E0432]: unresolved imports `wyrd_gateway_core::MultipartAbortAnswer`, … `wyrd_gateway_core::MultipartGateway`, … `wyrd_gateway_core::NamedParts`
error: could not compile `wyrd-server` (test "multipart_gateway_seam") due to 1 previous error
```

So C4-verify will report UNVERIFIABLE (77), as the brief pre-declares.

## Self-refutation (forced)

- **(a) Genuine red? Yes, with the caveat the brief pre-declares.** Reverting the whole fix makes
  the test fail to compile (E0432), because the trait is new. The behavioural reds are the
  mutations above: a server stub answering anything but `NotImplemented` turns leg (1) red;
  `NamedParts` accepting empty turns its test red; routing a form into the seam turns leg (3) red
  while every status stays `501`; a loose oracle turns the oracle self-test red. Leg (3) is green
  on the base by design (it pins "routing untouched").
- **(b) Production path? Yes.** Leg (1) calls the real `impl MultipartGateway for
  wyrd_server::Gateway` (`crates/server/src/lib.rs:757`); the defaults test runs the trait's real
  provided bodies; the `NamedParts` test calls the real constructor in `gateway-core`. Legs (2)
  and (3) drive the real `S3Gateway` router over a real loopback socket with real SigV4 signing.
  The sentinel stands in only for the backend behind the seam, which is the thing whose non-entry
  leg (3) measures. The oracle self-tests drive the same `judge_501` the live legs use, not a copy.
- **(c) Fixture includes the fault? Yes.** The sentinel answers `NotImplemented`, which is exactly
  what would make "entered the seam, mapped to 501" look right on the wire, and the counter
  catches it (mutation iii). All six forms, including bucket-scoped `GET /b?uploads`, are in
  `MULTIPART_FORMS`. The torn-framing variants each keep every asserted XML fragment (the test
  asserts that before judging), so they are exactly the "torn but plausible" responses the
  finding described.

## Leg (4) — the full gate

**Not green as one command, because of the base advisories in "Read first".** Every other step
passes on the patched tree.

`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l2 ./engine/xtask.sh ci` (= `cargo xtask ci`,
`xtask/src/main.rs:1671-1691`), on the final tree, exit 1:

| Step | Result |
|---|---|
| `typos` | pass |
| `lint_docs.py` | pass (`lint_docs: OK`) |
| `render_site.py --check` | pass (`render_site: link audit OK`) |
| gitlink / unsafe / blackbox guards | pass |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --exclude wyrd-dst --all-targets` | pass, no warnings (builds the S3 crate's `#[cfg(test)]` doubles and the `:5148` helper) |
| `cargo build --workspace --exclude wyrd-dst --all-targets` | pass |
| `cargo test --workspace --exclude wyrd-dst` | pass: 197 test binaries, 1,683 tests, 0 failed (incl. the 7 new tests and `request_capacity_planes.rs`) |
| `cargo-machete` | pass |
| `cargo deny check` | pass (`advisories ok, bans ok, licenses ok, sources ok`) |
| `cargo deny --all-features --config deny-all-features.toml check advisories` | **FAIL**: RUSTSEC-2026-0346, RUSTSEC-2026-0349 on `rustls 0.21.12` (fails identically on a clean export of base `7bf7eb9`) |
| conformance, statics, orchestrator guard, DST | not reached by `ci` (it stops at the first failure); run separately below |

The four unreached steps:

| Step | How | Result |
|---|---|---|
| conformance | `./engine/xtask.sh conformance`, final tree | pass (`5 valid + 6 invalid vectors pass`) |
| statics | `./engine/xtask.sh statics`, final tree | pass (`no DST-reachable shared mutable global state (ADR-0035)`) |
| orchestrator guard | no subcommand of its own; the same `xtask::deploy_guard::scan_dir` over the real `crates/` runs as `scan_dir_is_green_over_the_real_workspace_crates` (`xtask/tests/deploy_no_orchestrator_coupling.rs`) inside the gate's `cargo test` step | pass (`… ok`); also no added line in `patch.diff` contains any of its needles |
| DST | `./engine/xtask.sh dst` (`cargo clippy -p wyrd-dst` + `cargo test -p wyrd-dst` under `--cfg madsim`) | pass: 10 test binaries, 85 tests, 0 failed (one binary ran 1,252 s) |

Tree timing, so nothing is overstated: the `ci` run in the first table, conformance and statics
are on the final tree. The DST run started 14 s before my last edit, which changed only the
wording of three doc-comment lines on the `MultipartGateway` trait
(`crates/gateway-core/src/lib.rs:697-699`); no code changed after DST started.

**Tools: nothing was warn-skipped for a missing tool.** `typos`, `cargo-machete` and
`cargo-deny` are installed (`~/.cargo/bin`), and the docs renderer ran (`link audit OK`). The
brief's `External dependencies` (`typos`, `docs-renderer`) were both present.

## Commit-readiness

`cargo fmt --all` run over the tree; the gate's `cargo fmt --all -- --check` passes (see leg 4).
The target repo has no pre-commit hook config (no `.pre-commit-config.yaml`, no `.githooks/`);
its commit-time checks are the ones `cargo xtask ci` runs. DCO sign-off is the publish step's.

## Ruled out

- **Waiving the two rustls advisories in this PR**: a security-policy change for its own PR (see
  "Read first").
- **A doc-only precondition for the empty Complete list**: leaves the receiving implementer
  without a typed answer; the type costs ~40 lines more and removes the case (item 1).
- **A new `MultipartRefusal` arm for an empty list**: the brief fixes the arm set (core one to one
  plus two seam arms); `NamedParts` makes the arm unnecessary.
- **Extracting a shared HTTP parser module**: edits an out-of-scope test file and adds a support
  module convention `crates/server/tests/` does not have (item 3, with counts).
- **Making `consistency_observable.rs`'s private `parse_response` (`crates/server/src/consistency_observable.rs:440`) public for tests**: a production API change for a test helper.
- Everything iteration 1 ruled out (six methods on `ObjectGateway`, core types in the seam,
  refusals as `GatewayError`, no default bodies, unprefixed names, `Option` timestamps, a
  compile-fail harness) still stands.

## Scratch

Throwaway files only under
`/var/tmp/pdca/wyrd-pdca-9c587031/issue_666/pdca-builder-666-iter2` (saved copies used to
restore after each mutation, the base export for the advisory check, two throwaway git index
files, the gate log). Per the harness rules I issued no `rm`; the harness owns that root.
