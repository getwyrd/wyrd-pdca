# Build notes — issue #667 (508.2): multipart routing classifier, iteration 5

Base: getwyrd/wyrd `main` @ `376b106` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`). Lines cited as `lib.rs:N` or
`s3_multipart_routing.rs:N` are after `patch.diff`, in `crates/gateway-s3/src/` and `crates/server/tests/`; lines
cited as `base:N` are on `376b106` before the patch. Patch: 99,635 bytes, 4 files. A copy of the test is in this bundle.

## What changed since iteration 4: nothing in the patch, and why

`patch.diff` is byte-identical to iteration 4's apart from git's `index` lines (checked with `diff` over both files
with those lines removed). Round 4 raised no defect in the patch. Its two findings were both filed as "needs human
judgment, not build work", and the driver iterated anyway. Re-deriving the classifier would only risk reopening
what rounds 1–3 fixed, so this round keeps the code and supplies the evidence each finding asked for:

1. **C4 — the intermittent access-log assertion.** Decision: **a recorded flake disposition, not a correction in
   this PR.** It is a pre-existing race inside `tracing-core`'s callsite-interest cache, reproduced on the unpatched
   base, and it does not touch the routing change. Details, reproduction counts and the fix I recommend for a
   separate issue are in the next section.
2. **T4 — the affected-path search of merged history and closed/rejected work.** Done against the real repository
   (the reviewer's copy had one synthetic commit and no remote); results are in the section after that.

## Finding 1: the access-log flake (`a_bodyless_response_is_recorded_complete_not_aborted`)

**What fails.** `lib.rs:4919` (`base:4666`), the first assertion of that test: the JSON capture that should hold the
`"transfer":"complete"` row is **empty**. The test is in the base tree unchanged; the patch touches nothing it
executes: `finish_response` (`lib.rs:1730`), `record_access` (`lib.rs:1562`), `AccessLogged` and the `Capture`
writer (`lib.rs:4529`) are untouched, and its `op_label(&method, "/bucket/key", "")` calls take the GET/PUT/DELETE/HEAD
arms (`lib.rs:309-313`), not the POST arm the patch changed (`lib.rs:320-325`).

**Reproduction, same binary invocation, same host (32 cores), one run after another, no other load.**

| tree | runs | failures | which |
|---|---|---|---|
| patched (first sample) | 60 | 2 (runs 17, 39) | this assertion, both times |
| base `376b106`, production reverted, lib tests rebuilt | 100 | 1 (run 25) | this assertion, `base:4666` |
| patched (second sample) | 100 | 0 | |

Totals: patched 2 / 160, base 1 / 100. So the flake is on `main` before this patch, at about 1 % per run of the
`wyrd-gateway-s3` lib test binary, and the two trees are indistinguishable at these sample sizes. The reviewer's 30
clean base runs did not have the power to see a 1 % event.

**Mechanism** (tracing-core 0.1.36, the version in `Cargo.lock`; paths under
`~/.cargo/registry/src/*/tracing-core-0.1.36/src/`).

- The `tracing::info!` macro consults a **process-global** per-callsite interest cache before it looks at the
  current thread's subscriber. A callsite cached as `Interest::never` is skipped without consulting anyone
  (`callsite.rs:347-354`).
- The test installs its capturing subscriber as a **thread-scoped** default (`set_default`, `lib.rs:4888`). The
  cache is shared with every other test thread in the binary.
- How a callsite's interest is first computed: `DefaultCallsite::register` (`callsite.rs:308-342`) pushes the
  callsite and then calls `rebuild_callsite_interest(self, &DISPATCHERS.rebuilder())`. `rebuilder()`
  (`callsite.rs:544-549`) returns `Rebuilder::JustOne` whenever the dispatcher registry holds **at most one** live
  dispatcher (`has_just_one`, set at `callsite.rs:551-558` from `dispatchers.len() <= 1` after pruning dead ones).
  `JustOne` means: ask **the registering thread's current default** (`callsite.rs:562-567`,
  `dispatcher::get_default`).
- A thread with no subscriber has `Dispatch::none()` as its default, and `NoSubscriber::register_callsite` returns
  `Interest::never()` (`subscriber.rs:674-677`).
- The interleaving: `tests::a_bodyless…` runs early (the `tests::` module follows the fast `checksum`/`crypto`/
  `sigv4`/`streaming` modules in run order). It creates its `Dispatch` (`lib.rs:4881`); at that moment the earlier
  tests' dispatchers are dead, so the registry is `[A]` and `has_just_one` becomes true. That registration also
  raises the global max level from its initial `OFF` (`metadata.rs:245`) to `TRACE`, which is what lets every other
  thread start evaluating `info!` callsites at all. Before `A` reaches `record_access`, some other test thread —
  one that drives `handle` with **no** subscriber, which is most router tests — hits the `record_access` `info!`
  callsite (`lib.rs:1576`) for the first time in the process. Its `register()` runs under `JustOne`, asks its own
  default (`none`), and caches **`never`**. `A`'s own `tracing::info!` then reads that cache and emits nothing;
  the capture stays empty. Nothing in `A` rebuilds the cache after that point, so no retry inside the test can
  help; only a new `Dispatch::new` elsewhere would recompute it, and by then the test has asserted.
- Why it is rare: it needs this test to be the first to hit that callsite in the process (so it runs only in the
  first second of the binary), the registry to hold exactly one live dispatcher at that instant, and another
  thread to win the race to the callsite within the few microseconds between `Dispatch::new` and `finish_response`.
  That is a start-of-run race between test threads, which is why isolated, serial and `--test-threads=1` reruns are
  always clean and why it shows up only in full parallel runs.

**Why not fixed in this PR.** It is outside the brief's scope (`brief.md:81`: `lib.rs` for the classifier, and the new
test) and the rubric's rule for a real finding outside scope is decline-with-issue-reference, not an in-PR fix. It is
also not a routing defect, so bundling it would put an unrelated test-infrastructure change into the data-loss fix's
review.

**The fix I recommend for the separate issue** (test module only, about 12 lines, no production change): install a
process-wide fallback subscriber once per test binary, so a callsite's interest is never computed from
`NoSubscriber`. With a `Registry` as the global default, `JustOne` on a subscriber-less thread asks the Registry,
whose `register_callsite` is `always` (`tracing-subscriber-0.3.23/src/registry/sharded.rs:222-224`), and when the
registry holds several dispatchers the global one is always live, so the AND over them is at worst `sometimes` and
the thread-scoped capture is then consulted. Scoped defaults still win over the global for emission, so the tests
that assert a row is **absent** under an `EnvFilter` (`lib.rs:4205`, `:4392`, `:4427`) are unaffected. Sketch:

```rust
// in `mod tests`
fn install_fallback_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Ignored if another test already set one: any always-interested global will do.
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    });
}
```

called first in each test that asserts a row **exists** through a scoped capture (`lib.rs:4138`, `:4303`, `:4473`,
`:4622`, `:4720`, `:4822`, `:4881`). The alternative of calling `tracing::callsite::rebuild_interest_cache()` after
`set_default` does not close the race (the other thread can still register the callsite after the rebuild), so I
would not propose it. The upstream shape of the problem is tracing-core's choice to compute a `JustOne` interest
from the registering thread's default; the fallback subscriber sidesteps it without waiting on upstream.

Proposed issue title: *gateway-s3: access-log capture tests race tracing-core's callsite-interest cache
(`a_bodyless_response_is_recorded_complete_not_aborted` fails ~1–3 % of parallel runs on `main`)*.

## Finding 2: prior art on the affected path

Searched against `github.com/getwyrd/wyrd` on 2026-11-01 from the worktree's `origin` and with `gh`.

- **Merged history of `crates/gateway-s3/src/lib.rs`** (`git log origin/main -- crates/gateway-s3/src/lib.rs`): 40
  commits. The two that touch this routing: `19d3457` *fix(509): keep the subresource denylist closed on the
  bulk-delete route* (PR #612; it added `foreign_subresource_on_delete`, `base:578`) and `b879e7b` *gateway-core: add
  a MultipartGateway seam that answers not implemented* (PR #896; it introduced the seam this slice routes to and
  `crates/server/tests/multipart_gateway_seam.rs`). Neither decoded the object route's query; `19d3457` decoded only
  the bulk-delete and bucket routes, which is the asymmetry this patch removes.
- **Pull requests, all states, matching `multipart`, `denylist`, `subresource`, `percent-encoded`, `uploadId`,
  `partNumber`, `501`:** every hit is merged work on the core multipart protocol, the custodian, or the gateway's
  other guards (#448, #594, #597, #609, #612, #896, #899, #924 …). None adds or changes a query classifier. The only
  **closed-unmerged** PRs touching the S3 gateway are two dependabot bumps (#525 `sha2`, #526 `hmac`).
- **Issues:** #507 (ListObjects) and #509 (DeleteObjects) are closed as implemented; their adversary findings are
  the raw-vs-decoded gap this patch closes. #491 (percent-encoded subresource names bypass the denylist) is **open**:
  this patch closes it for the object route too (`lib.rs:2031-2033`, every remaining denylist key is matched
  decoded), so the human may want to close it or narrow it to the listing-parameter residual noted below. #508
  (parent) and #668–#670 (verb children) are open and wait on this slice. No issue records an earlier attempt at
  this routing being rejected.
- Limitation: `gh` search is keyword-based. The `git log` over the file is exhaustive for merged history.

## The red→green record (this patch, this worktree)

Runner: `PDCA_BUNDLE=/home/eddie/wyrd/wyrd-pdca/results/issue_667 PDCA_LANE=0 PDCA_BASE=376b106
./engine/scripts/run-verify.sh` on the final `patch.diff`.

Exit 0: `run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red)`. That summary line counts the
tests that ran on the RED leg, not the ones that failed; the leg counts are:

- GREEN: 11 ran, 11 passed.
- RED (production reverted, test kept): **11 ran, 7 failed, 4 passed**. All 7 are assertion panics at
  `s3_multipart_routing.rs:303` (the leg collector) and `:900`; nothing failed to build. The 4 that pass are leg 4
  and leg 7 (regression guards) and the two harness tests, which touch no production code.
- Failed on RED: `percent_encoded_spellings_classify_as_their_decoded_key`,
  `ill_formed_multipart_shapes_answer_400_invalid_argument`,
  `well_formed_multipart_shapes_answer_501_naming_the_operation`, `a_denylisted_subresource_wins_in_any_spelling`,
  `a_bucket_alias_is_refused_on_every_multipart_route`, `an_encoded_bulk_delete_marker_runs_the_bulk_delete`,
  `an_encoded_bulk_delete_marker_keeps_every_refusal`.
- What `main` answered, read from this run's RED log (the brief's *Reproduction* asked for the DELETE form):
  - `PUT /b/k?part%4Eumber=1` → **200**, and `k` then read back overwritten: HEAD 200 with a different ETag, GET 200
    with the request's 60 bytes.
  - `DELETE /b/k?upload%49d=0123456789abcdef0123456789abcdef` → **204**, then `k` answered HEAD 404 and GET 404:
    **deleted**. This confirms the brief's "should answer 204". The same for `?%75ploadId=…` with a well-formed, an
    unknown and a malformed (`zz`) id, and for `DELETE /b/k?d%65lete`.
  - `POST /b/k?upload%73` → 405 MethodNotAllowed.
  - `POST /b?d%65lete&part%4Eumber=1` → 501 naming `delete`, not `partNumber`; `POST /b?d%65lete` with a `<Delete>`
    body → 501, deleting nothing.
  - Every ill-formed shape (leg 2, 18 rows) → 501 naming the first multipart key, never the required 400; every
    alias form (leg 6) → 501, never `InvalidBucketName`.

### Refuting my own test (the three questions)

- **(a) Genuine red? Yes.** With production reverted by `run-verify.sh` and the test kept, 7 of 11 fail by assertion, with the rows above. The file
  uses only base API (`serve_s3`, `Gateway`, `sigv4::{sign, format_amz_date, Credentials}`, `RedbMetadataStore`,
  `WriteBatch`) and no new dev-dependency, so the RED leg compiles and fails by assertion, not by build.
- **(b) Production path? Yes.** Requests go over TCP to the real axum router, started by the production `serve_s3`
  (`crates/server/src/cli.rs:2658`), through `handle` → `dispatch` → `sigv4::verify` → `DecodedQuery::decode`
  (`lib.rs:1951`) → `is_delete_subresource` / `classify_query` (`lib.rs:1968`, `:1993`, `:2031`). Nothing is mocked.
  The backend is redb in memory + `FsChunkStore` + `MemCoordination` (`s3_multipart_routing.rs:137-230`).
- **(c) Fixture includes the fault? Yes.** The objects the destructive forms would hit are real and checked after
  every row (`s3_multipart_routing.rs:312`); on the RED leg they are overwritten and deleted. Leg 6 also seeds
  `b/a/k`, the object the `/b%2Fa/k` alias names. Leg 7's control and legs 8–9 end with real deletions, so the body
  would delete them if a refusal did not stop it.

## Decisions for the human (not build work; carried from round 4, still open)

- **Encoded listing parameters.** `GET /b?pref%69x=zzz` lists the whole bucket. The base does the same; this patch
  only documents it (`lib.rs:1946-1950`, `05-building-block-view.md:139`). The rubric's out-of-scope rule points to a
  separate issue; the brief's "whatever spelling the client uses" could be read to include it.
- **Bucket alias on bulk delete: a second spelling in.** On the base `POST /b%2Fa?d%65lete` answered `501`; with this
  patch it answers `200` and deletes `b/a/k`, as the plain `POST /b%2Fa?delete` already did on the base. It follows
  from decoding the marker, which round 2 required. `brief.md:48-54` limits the alias rule to multipart shapes, so I
  did not extend it. No test covers it.
- **UploadPartCopy** lands in the UploadPart arm. The comment on `multipart_response` (`lib.rs:2252-2255`) says that
  arm must refuse `x-amz-copy-source` itself. #669's brief needs to carry this.
- **The encoded marker turns one `501` into a working delete.** `POST /b?d%65lete` deleted nothing on `main` and now
  deletes what its body names, as `POST /b?delete` does. To undo: revert the `is_delete_subresource` change
  (`lib.rs:694`) and legs 8 and 9, then narrow the wording again.
- `docs/design/proposals/draft/0017-blackbox-validation-tool.md:245-248` expects the #491 encoded-`partNumber`
  vector to answer `501`; here it answers `400`, as brief (1) requires. It is a draft, not a living doc.
- `?uploadid=U`, `?uploadId%20=U` and `?x=1;uploadId=U` still reach the plain verb, as on the base. Each signs as a
  key other than the three selectors.

## Brief inconsistencies — what I followed and why

1. **Docs.** Scope excludes docs (`brief.md:94`); the round-2 carry-forward and the rubric's docs-currency rule
   require the paragraph at `05-building-block-view.md:139`, and round 3 asked for its wording to be narrowed.
2. **Scope says `lib.rs` and the new test only.** The round-3 carry-forward names the comment in
   `multipart_gateway_seam.rs:20-21` and `:77-78`, so that file is in the patch: comments only.
3. **Falsifiability says "(3)'s malformed id … red on exact status".** Criterion (3) requires `DELETE /b/k?uploadId=zz`
   to answer **501**, and `main` answers 501 too, so that row reds only on the message. I built to the criterion.
4. **Falsifiability says (5) is green on both trees**, but criterion (5) says the 501 names `tagging` / `acl`, and
   `main` names the first multipart key. With the naming asserted, leg 5 reds on `main`.
5. Leg 3's plain-spelled rows red on `main` only on the operation name. I asserted what the criterion states.

## Decisions carried from earlier rounds, and alternatives I ruled out

- **Decode once, not twice.** SigV4 canonicalisation decodes once, so `?part%254Eumber=1` signs as the literal key
  `part%4Eumber` and stays a plain PUT (`lib.rs:463-473`; pinned at `lib.rs:5574` and in leg 4).
- **The denylist is matched decoded on the object route too.** Brief (5) requires it. As a result the residual
  documented at `base:460-465` closes for every remaining denylist entry: on `main`, `PUT /b/k?t%61gging=1` overwrote
  `k` and `DELETE /b/k?t%61gging` deleted it; both now answer 501. Rejected: decoding only when a multipart key is
  present — about 5 more lines in `classify_query` that keep a known overwrite/delete path open.
- **Alias check placement.** After the denylist, before `InvalidArgument`, on every multipart shape and on both
  routes (`lib.rs:2242-2244`). `PartRead` is not alias-checked: plain GetObject does not refuse the alias today.
- **`partNumber` above `u32::MAX`** → 400 at decode (`lib.rs:580-589`). The seam takes `u32`.
- **Exotic methods** with a multipart key → 400 rather than the method table's 405. **GET with both `uploadId` and
  `partNumber`** and **HEAD with `uploadId`** → 400.
- **Rejected: two route-specific enums.** About 30 more lines for a property the single `multipart_response` gives.
- **Rejected: factoring the chunked decoder into shared test support.** It would rewrite parts of
  `multipart_gateway_seam.rs`. Cost of keeping a copy: about 55 lines (`s3_multipart_routing.rs:477-527`).
- `MultipartOp`'s `upload_id` / `part_number` fields are only read through the derived `PartialEq` in this slice;
  the verb children read them.

## For the verb children (#668–#670)

- Replace your operation's arm in `multipart_response` (`lib.rs:2256`). It and `query_route_response` (`lib.rs:2227`)
  take only `request_id`, the op and the bucket today, so you will need to pass in `state`, the decoded key,
  headers, payload hash and body.
- **UploadPartCopy:** see above.
- `MAX_KEY_BYTES` is enforced only in the plain PUT arm; Create (#668) should apply it as well.
- `partNumber` arrives canonical but unranged (0 and 10001 route to UploadPart).
- #668 should update `05-building-block-view.md:139` as each verb lands.

## Gates I ran (on the final tree)

- `cargo fmt --all -- --check`: clean. `typos` on all four touched files: clean (the brief's one external dependency
  is present). `git diff --check`: clean. The repo configures no commit hooks (`core.hooksPath` unset, no
  `.pre-commit-config.yaml`); its commit-time rules are the `cargo xtask ci` ones: rustfmt, clippy `-D warnings`,
  typos, docs lint.
- `cargo clippy -p wyrd-gateway-s3 --all-targets -- -D warnings` and
  `cargo clippy -p wyrd-server --test s3_multipart_routing --test multipart_gateway_seam -- -D warnings`: clean, no
  warning or error line.
- `cargo test -p wyrd-gateway-s3 --lib`: 97 passed (and the loop above: every failure in 260 runs across both trees
  was the pre-existing flake, never a classifier test). `cargo test -p wyrd-server --test s3_multipart_routing --test
  multipart_gateway_seam`: 11 + 7 passed.
- `cargo xtask statics`: passed (ADR-0035 gate). `docs/publishing/tools/lint_docs.py`: OK. I did not run the site
  render with its link audit; this patch adds no links.
- `run-verify.sh`: see the record above.
- **Not run: the full `cargo xtask ci` or `scripts/mutants-in-diff`.** Check's gates run both. Production logic is
  identical to iterations 3 and 4, where CI passed and `mutants-in-diff` reported 56 caught, 0 missed, 2 unviable
  (round 3) and the round-4 run was advisory-only.

Self-review against `## Review rubric & protocol`:

- **Docs currency:** met (`05-building-block-view.md:137-139` states the `501` / `400` split and the raw-match limit).
- **Clock:** the one clock read is the test file's `SystemTime::now()` for SigV4 dates, with its stated exemption
  (`s3_multipart_routing.rs:40-44`).
- **Crates and seams:** no new crate root; no port, RPC, CLI flag or persisted field changes.
- **Await discipline:** every await in the new test is bounded (`HTTP_TIMEOUT`, `s3_multipart_routing.rs:66`), and
  the spawned server is aborted on drop (`s3_multipart_routing.rs:131`), checked by
  `dropping_the_fixture_stops_its_server` (`:594`).
- **Protocol input / grammar strictness:** `parse_part_number` (`lib.rs:580`) takes canonical digits only; the test's
  response parser refuses torn chunked framing (`:530`).
- **Tier-0 DST:** I read "a new destructive path" as not triggered. The encoded marker enters the existing
  `delete_objects` handler, with no new delete code or concurrency.
