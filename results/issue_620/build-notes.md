# Build notes — issue 620 / s3-object-key-length-limit (iteration 7)

Withheld from the reviewer; for the human at sign-off.

## What this iteration changes, and why

The iteration-6 sign-off rejected one thing: every PUT the tests send is a single-shot
**signed** body, so a key check that had slipped inside the `PayloadHash::Signed` arm passed all
six wire legs and all three in-crate tests while a stock SDK (`aws-chunked`) or an
`UNSIGNED-PAYLOAD` client stored a 1,025-byte key. The shipped production code was already right
(both checks run before the payload `match`); only the regression guard was missing. This
iteration keeps iteration 6's production behaviour byte-for-byte and adds the guard in both test
layers, plus two one-clause doc touches that state the property the guard pins.

Base: `$PDCA_WORKTREE` is `getwyrd/wyrd` `origin/main` @ `8ebd1c0`, the base iterations 5 and 6
built on (the brief cites `7bf7eb9`; #671 merged in between and iteration 5 adapted to it).
`path:line` below are post-patch in the worktree unless marked pre-patch.

### Production (comment only)

- `crates/gateway-s3/src/lib.rs:1839-1846` — the comment above the two refusals now says they sit
  BEFORE the payload-form dispatch, so they hold alike for a signed body, an `aws-chunked` stream
  and `UNSIGNED-PAYLOAD`. No code line changes: the `InvalidBucketName` refusal (`:1853-1860`)
  and the `KeyTooLongError` refusal (`:1864-1871`) are where iteration 6 left them, above the
  `match payload` at `:1924` (`Streaming` `:1931`, `Signed` `:1946`, `Unsigned` `:1959`).

### Wire test — `crates/server/tests/s3_key_length_limit.rs`

- NEW leg 6, `refusals_hold_for_aws_chunked_and_unsigned_payload_puts` (`:708-754`). For each of
  `PayloadForm::AwsChunked` and `PayloadForm::Unsigned` (`:391-396`): a 1,024-byte key is stored
  and HEAD-found (the form itself is accepted, so a `400` later is the key refusal and not a
  framing or signature failure), a 1,025-byte key is `400 KeyTooLongError` with HEAD `404`, and
  `PUT /b%2Fa/<'y' × 1023>` is `400 InvalidBucketName` with HEAD `/b/a/<tail>` `404`.
- The `aws-chunked` client (`put_aws_chunked`, `:332-387`) signs the seed request with the
  crate's public `sign_with_payload_hash_and_headers` (`crates/gateway-s3/src/sigv4.rs:691`,
  pre-existing) over the `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` sentinel plus `content-encoding`
  and `x-amz-decoded-content-length` (what an SDK signs), takes the seed signature off the
  `Authorization` value, builds a `StreamingContext` (`sigv4.rs:74`, all fields `pub`) with
  `signing_key_for` (`sigv4.rs:366`), and frames the body with the decoder's own
  `streaming::sign_chunk` (`crates/gateway-s3/src/streaming.rs:138`, `pub`, documented as shared
  by the decoder and test harnesses "so the two can never drift"). `frame_signed` (`:315-326`) is
  the 12-line framer the peer `s3_put_object_ceiling.rs:235-250` and `streaming.rs:672-686` each
  carry too; a test binary cannot import another's helpers, and a new `pub` framer in the crate
  would be production API the reverted red leg could not compile against.
- The `UNSIGNED-PAYLOAD` client (`put_unsigned_payload`, `:278-310`) uses the public
  `sign_with_payload_hash` (`sigv4.rs:661`).
- Small refactor so the three PUT forms share one socket path: `request_head` (`:185-199`) and
  `exchange` (`:206-232`) were extracted from `signed` (`:237-274`). `exchange` now tolerates a
  write error after the head and a reset after the response, as `s3_put_object_ceiling.rs:276-290`
  does: a gateway that refuses from the headers may close before the body is written. The
  response is what is asserted, and a missing header terminator still panics with the label.
  `credentials()` (`:134`), `PAYLOAD` (`:105`) and `excerpt` on request paths (`:456`) remove
  repeats. Legs 1 to 5 are otherwise unchanged from iteration 6.
- Module doc (`:1-50`): item 6 added; the RED paragraph names leg 6 among the red legs and lists
  the pre-existing client primitives the file relies on.

### In-crate test — `crates/gateway-s3/src/lib.rs` `mod tests`

- NEW `an_object_put_refuses_an_over_limit_key_in_every_payload_form` (`:4905-4945`): the same two
  refusals in both forms through the real `S3Gateway::router()` against the `RecordingGateway`
  double, plus the seam fact the wire cannot show — the at-limit PUT of each form reaches
  `put_object_streaming`, the refused ones do not (`puts == expected_puts`).
- `signed_key_limit_request` (`:4703-4738`) now delegates the router + 30 s timeout to
  `drive_key_limit_request` (`:4678-4699`), shared with `put_key_limit_request` (`:4766-4839`),
  which builds the `UNSIGNED-PAYLOAD` or `aws-chunked` PUT (`PutPayloadForm`, `:4742-4748`;
  `frame_signed_chunks`, `:4752-4763`). The six existing callsites are untouched.

### Docs

- `docs/design/architecture/05-building-block-view.md:139` — one clause: the refusal applies "in
  every payload form alike (a single-shot signed body, an `aws-chunked` stream,
  `UNSIGNED-PAYLOAD`)". No port, operation, flag or field changed; the clause states the
  property the new tests pin.

## What I ruled out, with cost

- **Only the wire leg (the carry-forward's minimum).** C4-verify runs only the wire test, so this
  alone satisfies the gate. But the in-crate tests are the fast guard `cargo xtask ci` runs on
  every PR and they were exactly as blind to the mutant; leaving them blind costs the same
  iteration again the next time the PUT arm gains a header-time refusal (#671 already landed
  there). The in-crate addition is ~130 lines, all test code, no production change.
- **Only the UNSIGNED-PAYLOAD form** (no framer needed, ~35 lines less per layer). Rejected: it
  does not exercise the arm the stock SDK takes (`Streaming`, `lib.rs:1931`), and a mutant that
  moved the checks into `Signed | Unsigned` would pass it. The `aws-chunked` form is the one the
  sign-off named; `UNSIGNED-PAYLOAD` rides along at ~35 lines because it is the third arm.
- **Reusing `s3_put_object_ceiling.rs`'s hand-rolled signer by copy** (its `sign_request`,
  `ChunkSigner`, `streaming_headers`: ~120 lines). Rejected in favour of the crate's own public
  `sign_with_payload_hash_and_headers` + `StreamingContext` + `sign_chunk`, which were added for
  exactly this ("Exposed so a client / test harness can drive the real streaming wire path",
  `sigv4.rs:657-659`). Less copied code (the framer is 12 lines) and no second chunk
  string-to-sign to drift.
- **A `pub fn frame_signed` in `streaming.rs`** (1 visibility change + a move). Rejected: the wire
  test must compile against the reverted base for C4-verify's red leg, so it may use nothing the
  patch adds.
- **Separate at-limit control keys per form vs one shared key.** Per-form keys
  (`AwsChunked-sss…`, `Unsigned-sss…`, each cut to exactly 1,024 bytes) so the second form's
  HEAD `200` cannot be satisfied by the first form's object.

## Mutant check (the carry-forward's concrete case)

Reproduced the sign-off's mutant in the worktree: both checks wrapped in
`if matches!(payload, sigv4::PayloadHash::Signed(_)) { … }` at `lib.rs:1853-1871`, so they fire
only for a signed single-shot body. Captured that tree as a scratch bundle
(`$PDCA_SCRATCH/pdca-builder-620-mutant7/patch.diff` + a copy of `brief.md`), reverted the
mutant, and ran the configured C4-verify (`engine/scripts/run-verify.sh`, `PDCA_LANE=0`) on it:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test s3_key_length_limit (fix applied)
running 7 tests
refusals_hold_for_aws_chunked_and_unsigned_payload_puts --- FAILED
  panicked at crates/server/tests/s3_key_length_limit.rs:446:5:
  AwsChunked PUT 1,025-byte key: expected 400 KeyTooLongError … left: 200 right: 400
test result: FAILED. 6 passed; 1 failed
run-verify.sh: FAIL — the bundle's test does not pass with the fix applied
```

So the mutant that passed 9 #620 tests at iteration 6 now fails the shipped wire test (log:
`$PDCA_SCRATCH/pdca-builder-620-verify7/verify-mutant.log`). The worktree diff was then checked
byte-identical to the bundle's `patch.diff` (`git diff | diff -q - patch.diff`), and
`grep -c MUTANT patch.diff` is 0.

## Refutation — the three questions

- **(a) Genuine red?** Yes. `engine/scripts/run-verify.sh` (the configured C4-verify `cmd`,
  `PDCA_BUNDLE=results/issue_620`, `PDCA_LANE=0`) applied `patch.diff` to `../wyrd-verify-l0` on
  `origin/main`: GREEN leg `7 passed; 0 failed`. With `crates/gateway-s3/src/lib.rs` and the doc
  reverted and the test kept: RED leg `3 passed; 4 failed` — legs 1, 2, 5 and the new 6 at the
  `assert_refused` status check (`s3_key_length_limit.rs:446`), leg 6 on
  `AwsChunked PUT 1,025-byte key: expected 400 KeyTooLongError … left: 200`. Legs 3, 3-at-cap
  and 4 pass on both legs by design (they pin that no delete path refuses a stored key); the
  iteration-6 mutant run above is their red. Verdict: `run-verify.sh: PASS — red without the
  fix, green with it (7 test(s) ran red)`. Log: `$PDCA_SCRATCH/pdca-builder-620-verify7/verify-real.log`.
- **(b) Production path?** Yes. The wire test drives a real loopback `S3Gateway::serve` over the
  production `wyrd_server::Gateway` on `RedbMetadataStore::in_memory` + `FsChunkStore` +
  `MemCoordination`. The `aws-chunked` leg goes through hyper, `sigv4::verify` (which builds the
  production `StreamingContext` and classifies the claim as `PayloadHash::Streaming`), the PUT
  arm's refusals, and — for the at-limit control — `streaming::decode` and the real
  `put_object_streaming`, then HEAD through `head_object`. The signer is the crate's public client
  half (`sign_with_payload_hash_and_headers`, `sign_chunk`), which the gateway re-verifies
  independently; no verifier code is mocked. The in-crate test drives the real
  `S3Gateway::router()` with the recording double at the seam, the module's existing pattern.
- **(c) Fixture includes the fault?** Yes. The leg sends exactly the request the sign-off named:
  a 1,025-byte key as a stock-SDK `aws-chunked` PUT (and as `UNSIGNED-PAYLOAD`), plus the
  `%2F`-bucket alias in both forms, and asserts both the refusal and the absence afterwards. The
  at-limit control in each form proves the harness reaches the store, so a `400` from a broken
  framer or signature could not masquerade as the key refusal (the `<Code>` is asserted too).
  Nothing is curated out.

## Commit-readiness

- `cargo fmt --all` run over the worktree; the patch is taken after it.
- `typos` over the three touched files: clean.
- `cargo check -p wyrd-gateway-s3 -p wyrd-server --all-targets`: clean (both test layers compile).
- The target repo configures no git hooks (`core.hooksPath` unset; no `.githooks`,
  `.pre-commit-config.yaml` or `lefthook.yml`, unchanged from iteration 6). Its commit-time
  checks are CI's `cargo xtask ci`.
- `./engine/xtask.sh ci` (the configured C4-ci runner) in `$PDCA_WORKTREE`: see the line below.

CI RESULT: `./engine/xtask.sh ci` in `$PDCA_WORKTREE` finished `xtask ci: all checks passed`,
exit 0 — typos, `cargo fmt --all -- --check`, `clippy --workspace --all-targets`,
`build --all-targets`, `test --workspace --exclude wyrd-dst` (which ran the four in-crate #620
tests — `an_object_put_refuses_an_over_limit_key_before_the_write_seam`, the new
`an_object_put_refuses_an_over_limit_key_in_every_payload_form`,
`a_bulk_delete_deletes_an_over_limit_key_rather_than_refusing_it`,
`a_bulk_delete_deletes_the_longest_key_the_body_cap_can_carry` — and the wire file's 7 legs, all
`ok`), cargo-machete, both cargo-deny steps (the all-features advisory wall is clean on this
base), the conformance vectors, the statics gate, and the madsim DST leg. Full log:
`$PDCA_SCRATCH/pdca-builder-620-verify7/xtask-ci.log`.

## Rubric self-review (AGENTS.md `## Review rubric & protocol`)

- One clock per lifecycle: no clock read added to production; the test files keep the #619
  file-scope exemption comment the peer suites use (`SystemTime::now()` stamps SigV4 dates only).
- Narrow seams / dependency direction: untouched (test-only delta plus comments).
- Metadata validation boundaries: untouched.
- No DST-reachable shared mutable global state: none added (the double's `Mutex` fields are
  instance state inside `mod tests`; the wire test's constants are `&str`/`&[u8]`).
- `#![forbid(unsafe_code)]`: the test file carries it.
- Docs currency: no new port, API, RPC, flag or field; the architecture paragraph gains one
  clause stating the payload-form property.
- Protocol input / grammar strictness / serialization identity / transactions: untouched.
- Absent or unsupported entries: no silent skip; every leg asserts the response code AND the
  stored state (HEAD, or the seam record), never a count.
- Await discipline: every new await runs under `IO_BUDGET` (`bounded`, through `exchange`) or
  the in-crate 30 s `timeout` (`drive_key_limit_request`); the server task is aborted in
  `Harness::drop`.
- Test fidelity: no new destructive or concurrent production path, so no new Tier-0 DST
  obligation; the wire test runs on the real redb/fs/mem stack and the production SigV4 /
  streaming verifier.
- Workflow edits: none.

## External dependencies

None missing. `typos` is installed and ran clean; `docs-renderer` is an `xtask ci` warn-skip
concern and the doc change adds no links; `cargo deny all-features advisories` is exercised by
the `xtask ci` run on this tree.

## Scratch

Two throwaway dirs under the harness's `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_620/`): `pdca-builder-620-verify7/` (the C4-verify
logs for the mutant and the real patch, and the `xtask ci` log) and `pdca-builder-620-mutant7/`
(the mutant patch + brief copy). Left in place for the human to inspect; the builder contract
forbids `rm`-style cleanup and the root is harness-owned.

## Pointers for the human

- The carry-forward says one finding is held in `deferred-findings.json` for your judgment; it
  is not build work and nothing here addresses it.
- Still open from the brief's Plan-review response: #893's text repeats the old "no stored key
  is stranded whatever its length" claim, and recovering an over-cap key is tracked nowhere.
