# Build notes — issue 620 / s3-object-key-length-limit (iteration 6)

Withheld from the reviewer; for the human at sign-off.

## What this iteration changes, and why

The iteration-5 sign-off rejected one thing: leg 3 only proved "no bulk-delete length bound"
up to its 70,000-byte fixture, so a parser check `key.len() > 100_000` passed every test while
stranding every key between 100,001 and 131,072 bytes that `wyrd put --key` can write. This
iteration keeps iteration 5's production change and docs byte-for-byte and strengthens the
tests. **Production code is unchanged from `iteration-v5/patch.diff`**; the diff grows by
test-only lines.

Base: `$PDCA_WORKTREE` is `getwyrd/wyrd` `origin/main` @ `8ebd1c0`, the same base iteration 5
built on (the brief cites `7bf7eb9`; #671 merged in between, and iteration 5 already adapted to
it). Pre-patch `path:line` below are against `8ebd1c0`; post-patch ones say so.

Three additions:

1. `crates/server/tests/s3_key_length_limit.rs` — a NEW leg,
   `bulk_delete_removes_the_longest_key_the_body_cap_can_carry` (post-patch `:426-467`). It
   seeds `b/<'d' × 8,388,563>` through `Gateway::put_object` (`crates/server/src/lib.rs:257`),
   confirms it exists through `Gateway::get_object` (`:338`), sends one signed `DeleteObjects`
   whose body is `<Delete><Object><Key>…</Key></Object></Delete>` with that key — **exactly
   8,388,608 bytes**, the cap — and asserts `200`, a `<Deleted>` row, no `<Error>`, and
   `get_object` → `None` afterwards. Two constants back it (post-patch `:74-84`):
   `DELETE_BODY_CAP = 8 * 1024 * 1024`, restating `MAX_DELETE_BODY_BYTES`
   (`crates/gateway-s3/src/lib.rs:507`), and `ONE_KEY_ENVELOPE`, the 45 non-key bytes of a
   one-key document. The leg asserts `at_cap.len() == 8_388_563` (the brief's own figure) and
   `xml.len() == DELETE_BODY_CAP`.
2. `crates/gateway-s3/src/lib.rs` — the in-crate twin,
   `a_bulk_delete_deletes_the_longest_key_the_body_cap_can_carry` (post-patch `:4820-4859`),
   against the `RecordingGateway` double: the same at-cap body through the real router, asserting
   `200`, `<Deleted>`, no `<Error>`, and that exactly one delete, `b/<key>`, reached the seam. It
   reads `MAX_DELETE_BODY_BYTES` itself, so it moves with the cap.
3. `crates/server/tests/s3_key_length_limit.rs` — an `excerpt(&str) -> &str` helper (post-patch
   `:271-277`) replacing leg 3's three `&body[..body.len().min(600)]` slices. Same 600-byte cut,
   but at a char boundary: a byte slice through a multi-byte character panics inside the
   assertion message, which would hide the real failure. Leg 3's keys are ASCII today, so this
   is hygiene, not a fix; the new leg uses it too.

The module doc (`:15-24`) and the new legs' doc comments say what the at-cap key proves.

## Why exactly the cap, not "about 8,388,000"

`buffer_capped` refuses only when the body goes *over* the cap (`if buf.len() + chunk.len() >
cap`, `crates/gateway-s3/src/lib.rs:1990`), so a body of exactly `MAX_DELETE_BODY_BYTES` is
admitted. A one-key document carries 45 bytes of envelope, so the longest key bulk delete can
carry is 8,388,608 − 45 = 8,388,563 bytes — the number the brief's Invariant names. A test at
that key length goes red against a length bound at **any** threshold below the cap, because every
shorter key rides in a smaller body; "about 8,388,000" would leave a 563-byte window. The two
tests also pin the cap itself from below: lowering it makes them fail, which is the right
signal, since the brief's promise ("any key the cap can carry is deletable") would have
narrowed. Raising it keeps them green.

Cost of the exact-cap choice: none measurable. The whole wire file, including the 8 MiB seed,
sign (one SHA-256 over 8 MiB), send, parse and delete, runs in 1.37 s (C4-verify's green leg,
below), where iteration 5's five legs took about the same.

## What I ruled out, with cost

- **A fourth key in leg 3's three-key request.** Possible (the key would be 8,317,480 bytes to
  fill the cap alongside the other three), but criterion (3) is phrased as "one signed
  `DeleteObjects` naming all three keys", and the number would depend on the other fixtures.
  A separate leg is ~45 lines, names its failure (`…the_longest_key_the_body_cap_can_carry`),
  and keeps leg 3 literal to the criterion. Chosen.
- **Making `MAX_DELETE_BODY_BYTES` `pub` so the wire test can read it** (a 1-word change).
  Rejected: C4-verify's RED leg reverts the production files and keeps the test, so a test that
  names a symbol the patch adds does not compile there and the gate reports UNVERIFIABLE, not
  red. The constant is restated in the test with a comment saying why.
- **Only the in-crate test** (the carry-forward's "nearly free" option). Rejected as the sole
  change: C4-verify runs only the shipped wire test, so an in-crate-only strengthening would
  leave the gate's own discriminator exactly as weak as before. Both were added; the in-crate
  one costs 40 lines and runs under `cargo xtask ci`.
- **Additional intermediate sizes (131,073; 3 MB).** Redundant once the at-cap key is present:
  any bound they would catch, the at-cap key catches.

## Mutant check (the carry-forward's concrete case)

Reproduced the sign-off's mutant in the worktree — `if key.len() > 100_000 { return
Err(DeleteRequestError::Malformed); }` right after the empty-key check in
`parse_delete_request` (`crates/gateway-s3/src/lib.rs:2279-2281` pre-patch, `:2331-2333`
post-patch) — captured that tree as a scratch bundle patch, reverted the mutant, and ran the
configured C4-verify (`engine/scripts/run-verify.sh`) against the scratch patch:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test s3_key_length_limit (fix applied)
bulk_delete_removes_the_longest_key_the_body_cap_can_carry --- FAILED
  panicked at crates/server/tests/s3_key_length_limit.rs:447:5:
  a bulk delete of the longest key the cap can carry must succeed:
  <Error><Code>MalformedXML</Code>…   left: 400  right: 200
test result: FAILED. 5 passed; 1 failed
run-verify.sh: FAIL — the bundle's test does not pass with the fix applied
```

So the mutant that passed 109 tests at iteration 5 now fails the shipped test. The worktree diff
was then checked byte-identical to the bundle's `patch.diff` before `xtask ci` ran, so no mutant
line is in the patch (`grep MUTANT patch.diff` finds nothing).

## Refutation — the three questions

- **(a) Genuine red?** Yes. `engine/scripts/run-verify.sh` (the configured C4-verify `cmd`,
  `PDCA_BUNDLE=results/issue_620`, `PDCA_LANE=0`) applied `patch.diff` to `../wyrd-verify-l0`
  reset to `origin/main` = `8ebd1c0`: GREEN leg `6 passed; 0 failed` (1.37 s). With
  `crates/gateway-s3/src/lib.rs` and the doc reverted and the test kept: RED leg `3 passed;
  3 failed` — legs 1, 2 and 5 at the `assert_refused` status check (`s3_key_length_limit.rs:261`):
  `PUT 1,025-byte key: expected 400 KeyTooLongError … left: 200`, `PUT 342 × €: … left: 200`,
  `PUT /b%2Fa/<1,023 bytes>: expected 400 InvalidBucketName … left: 200`. Legs 3, 3-at-cap and
  4 pass on both legs by design: they pin that no delete path refuses a stored key for its
  length, and the mutant run above is their red. Verdict: `run-verify.sh: PASS — red without
  the fix, green with it (6 test(s) ran red)`.
- **(b) Production path?** Yes. The wire test drives a real loopback `S3Gateway::serve` over the
  production `wyrd_server::Gateway` on `RedbMetadataStore::in_memory` + `FsChunkStore` +
  `MemCoordination`, with signed SigV4 raw HTTP (`sigv4::sign`, `uri_encode`). The at-cap body
  goes through hyper, `buffer_capped` (`lib.rs:1985-1996`), the signed-digest check,
  `parse_delete_request` and the real `delete_object` fan-out; the key is read back through the
  production `Gateway::get_object`. The in-crate test drives the real `S3Gateway::router()` with
  the recording double at the seam, the pattern the module already uses.
- **(c) Fixture includes the fault?** Yes. The new leg seeds exactly the key a hidden bound
  would strand — the longest one the cap admits, 8,388,563 bytes, past the `414` ceiling so bulk
  delete is its only S3 exit — through `Gateway::put_object`, the same path a legacy or
  out-of-band object arrives by, and proves it exists (`get_object` → `Some`) before the delete.
  Nothing is curated out.

## Commit-readiness

- `cargo fmt --all` run over the worktree (it reformatted one `format!(…)` line in the new
  in-crate test).
- `typos` over the three touched files: clean.
- The target repo configures no git hooks (`core.hooksPath` unset; no `.githooks`,
  `.pre-commit-config.yaml` or `lefthook.yml`, unchanged from iteration 5). Its commit-time
  checks are CI's `cargo xtask ci`.
- `./engine/xtask.sh ci` (the configured C4-ci runner) in `$PDCA_WORKTREE`: see the line below.

CI RESULT: `./engine/xtask.sh ci` in `$PDCA_WORKTREE` finished `xtask ci: all checks passed`,
exit 0 — typos, docs lint + render, the gitlink/unsafe/blackbox guards, `cargo fmt --all --
--check`, `clippy --workspace --all-targets`, `build --all-targets`, `test --workspace
--exclude wyrd-dst` (which ran the three in-crate tests:
`an_object_put_refuses_an_over_limit_key_before_the_write_seam`,
`a_bulk_delete_deletes_an_over_limit_key_rather_than_refusing_it` and the new
`a_bulk_delete_deletes_the_longest_key_the_body_cap_can_carry`, each `ok`), cargo-machete, both
cargo-deny steps (so the all-features advisory wall is clean on this base), the statics and
deploy-guard gates, the conformance vectors, and the madsim DST leg. Full log:
`$PDCA_SCRATCH/pdca-builder-620-verify/xtask-ci.log`.

## Rubric self-review (AGENTS.md `## Review rubric & protocol`)

- One clock per lifecycle: no clock read added; the test file keeps the #619 file-scope
  exemption comment the peer suites use.
- Narrow seams / dependency direction: untouched (test-only delta).
- Metadata validation boundaries: untouched.
- No DST-reachable shared mutable global state: none added (the double's `Mutex` fields are
  instance state inside `mod tests`).
- `#![forbid(unsafe_code)]`: the test file carries it.
- Docs currency: no new port, API, RPC, flag or field beyond iteration 5's, whose architecture
  paragraph (`docs/design/architecture/05-building-block-view.md:139`) already states the
  residual (a key over the cap has no S3 exit).
- Protocol input / grammar strictness / serialization identity / transactions: untouched.
- Absent or unsupported entries: no silent skip; the at-cap leg asserts the row AND the
  backend state, never a count.
- Await discipline: every new await runs under `IO_BUDGET` (`bounded`, `signed`) or the
  in-crate 30 s `timeout`; the server task is aborted in `Harness::drop`.
- Test fidelity: no new destructive or concurrent production path (bulk delete is unchanged),
  so no new Tier-0 DST obligation; the wire test runs on the real redb/fs/mem stack.
- Workflow edits: none.

## External dependencies

None missing. `typos` is installed and ran clean; `docs-renderer` is an `xtask ci` warn-skip
concern and the doc change (unchanged from iteration 5) adds no links; `cargo deny all-features
advisories` is exercised by the `xtask ci` run on this tree.

## Scratch

Two throwaway dirs under the harness's `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/
issue_620/`): `pdca-builder-620-verify/` (the real C4-verify and `xtask ci` logs) and
`pdca-builder-620-mutant/` (the mutant patch and its verify log). Left in place for the human
to inspect; the builder contract forbids `rm`-style cleanup and the root is harness-owned.

## Pointers for the human

- The carry-forward says one finding is held in `deferred-findings.json` for your judgment; it
  is not build work and nothing here addresses it.
- Still open from the brief's Plan-review response: #893's text repeats the old "no stored key
  is stranded whatever its length" claim, and recovering an over-cap key is tracked nowhere.
