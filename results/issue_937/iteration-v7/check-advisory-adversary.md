# Adversarial review — #937 (743.1, the capability matrix)

Method: I copied `crates/validate/src/matrix.rs` byte-for-byte (diff-checked against
`$PDCA_TARGET`) and the shipped `crates/validate/tests/capability_matrix.rs` into a scratch
crate, with `ErrorCode` copied verbatim from `crates/validate/src/s3/error.rs:64-76,191-199`.
The shipped suite passes there 57/57, the same as `gate-logs/C4-verify.log`. I then ran my own
logic mutations and probe cases against that copy.

## Red→green evidence: attempted to refute, could not

- I made each of the brief's ten required reds (`brief.md:143-150`) as an edit to production
  `matrix.rs`. Every one turned the shipped suite red, and every test calls the production
  `wyrd_validate::matrix` functions, not a copy. (1) unsupported 2xx → pass, `:915`: 4 tests fail.
  (2) 2xx with a failed body → no answer: 2 fail. (3) ETag accepts any non-empty value, `:1052`:
  3 fail. (4) the non-matching conditional GET accepts 304, `:1101-1103`: 1 fails. (5) bulk delete
  passes on its document alone, `:1225`: 5 fail. (6) the selector ignores the version, `:601`:
  5 fail. (7) `+git.unknown` read as `0.0.0`, `:543`: 1 fails. (8) a not-checked row counted as
  unexercised, `:1412`: 2 fail. (9) the PUT ignores an early head, `:1046`: 2 fail. (10) a probe
  with no answer → `oracle`, `:1173`: 4 fail. C4-verify's UNVERIFIABLE (exit 77, the test cannot
  compile with production reverted) is the net-new exception the brief declared in advance
  (`brief.md:138-141`). I see no sign the tests pass for the wrong reason.
- I also tried to break the fix and could not, in these areas:
  - Version parsing against every shape `normalize_describe` emits
    (`crates/server/src/version/derivation.rs:40-68`), including `0.1.0+git.dirty` for a dirty
    build exactly on a tag, and `+git.unknown` on a fallback base other than `0.0.0`.
  - Pairing each bulk-delete probe with the key it asked about, retries, and probes of keys the
    delete did not request.
  - Torn bodies on every bodiless answer.
  - All 22 `pre-0.1` citations: each matches the line on this base (`crates/gateway-s3/src/lib.rs`
    :121, :410-422, :1766-1788, :1797-1815, :1819-1826, :1842-1849, :1865-1871, :2008-2028,
    :2713-2719, :2785-2803, :2293-2300, :3401).
  - The 0017 *Dependencies* contradiction raised in iteration 4, now fixed at
    `docs/design/proposals/draft/0017-blackbox-validation-tool.md:873`.

## Findings

- NEEDS-HUMAN [human] — **The gating C4-ci red comes from the base, not this patch, and
  rebuilding cannot clear it.** The only failing steps are the two `cargo deny` advisory checks:
  RUSTSEC-2026-0385 on ctutils 0.4.2 and RUSTSEC-2026-0386 on tokio 1.52.3
  (`gate-logs/C4-ci.log:3880`, `:3962`, `:5178-5179`). Both versions come from the base's
  `Cargo.lock:1000-1001` and `:3953-3954`, which the patch does not touch. Everything else passed:
  typos, `lint_docs`, the docs render, clippy, and the tests, `capability_matrix` included
  (`gate-logs/C4-ci.log:17-25`, `:3179`). The base (`93a6f37`) still lacks #950 and #954, although
  the iteration-3 carry-forward (`brief.md:235`) asked for a rebuild on `b552ca2`. This is the
  fourth round red for the same reason, and `Cargo.lock`/`deny*.toml` are out of this slice's
  scope (`brief.md:130-134`). Someone has to rebase the bundle onto a base that has #950/#954, or
  accept the red as inherited.

- NEEDS-HUMAN [human] — **The T4 "pass" is thin evidence.** `check-gates.json:108-116` records
  `T4-batch-review` as a gating pass: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in
  **19.58 s**. That would be three review passes over 1,487 changed lines (about 100 KB of diff) in
  under 20 seconds, where earlier rounds of the same gate found 5 and then 2 blocking findings. I
  cannot see `review-batch.md`. Before treating T4 as the rubric's "one deep, multi-pass review",
  confirm the file holds real passes over this diff and not a cached or short-circuited result.

- NEEDS-HUMAN [human] — **The same 503 means FAIL on an unsupported row and INCONCLUSIVE on a
  required one, and the module's rustdoc says the opposite of the first.** In probe P4,
  `CopyObject` answered `503 SlowDown` gives `Fatal { Protocol }` (`crates/validate/src/matrix.rs:920-925`,
  pinned by `capability_matrix.rs:398`). In probe P5, `PutObject` answered `503 SlowDown` gives
  `Unsettled` ("availability", `matrix.rs:949-951`). The code follows the brief ("a 5xx included",
  `brief.md:34-35`) and 0017 §3 ("Not a 500", `0017:194`). But 0017 §7 puts every 5xx under
  budgeted `availability` (`0017:508`). The module's own invariant (`matrix.rs:26-27`) and the
  `RowResult::Unsettled` doc (`:863-864`) also list "a 5xx" as an answer that neither passes nor
  fails, which is false for every unsupported row. In practice, one transient 503 from a proxy
  during the unsupported sweep FAILs a long run. Someone has to decide which rule holds. If the
  brief's rule stays, the two rustdoc passages need a builder-sized correction.

- NEEDS-HUMAN [human] — **The conditional-GET negative control cannot be enforced by the
  matrix.** `Exercise::GetObjectIfNoneMatchMiss` (`matrix.rs:744-747`) and `...Hit` (`:741-743`)
  carry no record of the `If-None-Match` value sent or of the object's tag. In probe P6, a plain
  200 with the whole body passes the Miss row. So if 743.3's Miss request omits the header, or
  sends a tag the server ignores, a server that "answers 304 to any tag" passes both rows. That is
  the exact case brief (B′) calls the decided negative control (`brief.md:49-51`). Compare
  `GetObjectRanged`, which carries `first`/`last` and calls an impossible request unsettled
  (`matrix.rs:1075-1079`). A fix would add `sent`/`object_etag` fields: unsettled unless `sent` is
  a strong tag that differs from the object's tag (Miss) or equals it (Hit). That changes the type
  743.3 builds, so someone has to decide whether this guard belongs here or in 743.3's mapping.

- NEEDS-HUMAN [human] — **The whole-body observation type will not hold the size classes later
  scenarios need.** `Exercise::GetObject.object` and `GetObjectRanged.object` are `Vec<u8>`
  (`matrix.rs:730-740`), and `Body::Bytes` is `Vec<u8>` (`:687`). Judging the 5 GiB "must
  succeed" class (`0017:306`) means holding the expected object and the response body in memory,
  about 10 GiB per GET, while the brief says "every later scenario" needs this matrix
  (`brief.md:15-16`). This is fine for smoke. Someone should decide whether to record a
  digest-based body variant as a tracked follow-up now.

- NEEDS-HUMAN [impl] — **Three pass conditions have no test.** Production is correct on all
  three; a mutation that removes each one leaves the shipped suite 57/57 green.
  - (a) `matrix.rs:971` (`Body::Empty => Ok(&[])`) never runs (`gate-logs/C4-diff-cov.log:157`).
    Changing `:1013` to `if got == want || got.is_empty()` lets a GET of a 16-byte object answered
    `200` with no body pass. Production gives `Integrity` (P1). Add `get(just(200))` → `Integrity`
    beside `capability_matrix.rs:548-564`.
  - (b) Adding `got.dedup()` after `got.sort()` at `matrix.rs:1147` survives, so a listing that
    names `p/a` twice (a pagination overlap) would pass. Production gives `Oracle` (P2). Add a
    duplicate-key case to `capability_matrix.rs:625-637`. This is the rubric's "can pass while the
    property fails" class.
  - (c) Adding `deleted.dedup()` after `matrix.rs:1274` survives, so a `<Deleted>` naming `p/a`
    twice would pass. Production gives `Protocol` (P3). This one is lower priority.
  - Optional, and the sole reason C5 is red (`gate-logs/C5-mutants.log`): under the surviving
    `&&`→`||` mutant at `matrix.rs:1455`, the report for `verdict(&PRE_0_1, &[])` would claim
    "no required or unsupported row declared". Assert the opposite in `capability_matrix.rs:1176-1181`.
    It is text only; the outcome does not change.
  - Size caution: `patch.diff` is 101,896 bytes. That is under 100 KiB (102,400 bytes) but over
    100 kB, against the brief's "100 KB backstop" (`brief.md:166-167`), so keep these fixtures terse.

- Not re-raised: the three open design points the human already holds (`brief.md:235`): verdict
  strictness on retries, `0.1.0-rc.1` selecting the edited-in-place profile, and an empty listing
  being unsettled. The behavior matches what build-notes was told to flag. I found nothing new
  about them.

<!-- pdca:leaf-complete -->
