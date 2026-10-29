# Adversarial review — #937 (743.1, the capability matrix), iteration 9

I re-ran the evidence and could not break the core logic. I found two concrete gaps where the
verdict says less than the answer proves. Both are small fixes inside `matrix.rs`.

How I tested: a scratch crate holding a byte-identical copy of `crates/validate/src/matrix.rs`
(checked with `cmp`), a copy of the test file, and a stub `ErrorCode` with the same shape as
`crates/validate/src/s3/error.rs:68-76`. That way I avoided building `wyrd-server`. The suite
passes 39/39 there, matching `gate-logs/C4-ci.log:3179-3181`.

## Findings

- NEEDS-HUMAN [impl] — **The declared `Content-Length` is ignored on every object read, so a
  head that already shows the wrong size reads as "unsettled", not `integrity`.**
  `expect_bytes` (`crates/validate/src/matrix.rs:889-934`, called at `:1009`, `:1037` and
  `:1062`) never looks at `response.content_length`. The field is read in one place only:
  `HeadObject` at `:1067`. Concrete cases I ran against the production code:
  (a) whole GET, `Content-Length: 17` for the 16-byte object, all 16 correct bytes arrive, then
  a cut → `Unsettled { "cut off after 16 correct bytes of 16" }`;
  (b) `Content-Length: 15`, cut after 10 correct bytes → `Unsettled`;
  (c) ranged 206 `bytes 2-5/16` with `Content-Length: 10`, cut after 2 correct bytes →
  `Unsettled`;
  (d) a whole 16-byte correct body under `Content-Length: 15` → `Pass`.
  This is the same defect the gating T4 review found (`gate-logs/T4-batch-review.log:10`). I
  confirm it, and it applies to the ranged and non-matching GETs too, not only the whole GET.
  It breaks the brief's invariant ("never less") and the rubric's *Protocol input* rule
  ("Enforce declared `Content-Length`"). §7 already defines `integrity` as "bytes, digest, or
  size differ". The reviewer might lean on (B)'s line "a cut after a correct prefix … stays
  unsettled", but that line is about the bytes. The brief's *Design* lists `Content-Length`
  among "the headers the checks read", so leaving the header out of every read check does not
  follow from the brief.
  Fix: in `expect_bytes`, if `content_length` is `Some(n)` and `n != want.len()`, return
  `integrity` before the cut arm. Add tests for both arrival forms next to
  `crates/validate/tests/capability_matrix.rs:691-697`; none of those tests sets a
  `Content-Length` today. The patch is at 99,636 of its 100,000-byte budget, so the builder will
  need to trim rustdoc to make room.

- NEEDS-HUMAN [impl] — **If the exercise requests a key twice, `DeleteObjects` hides an `<Error>`
  row or an unreadable document as "unsettled".** `check_delete_result`
  (`crates/validate/src/matrix.rs:1186-1193`) returns early on a duplicate key. That check runs
  before `whole_document` (`:1194`) and the `<Error>` check (`:1199`), and neither of those
  needs to match keys one by one. Concrete cases: keys `["p/a","p/a"]`, a 200 whose
  `<DeleteResult>` has an `<Error>` row for `p/a`, follow-up `HEAD` 404 →
  `Unsettled { "the exercise requested key \"p/a\" twice" }`, where (B3) makes an `<Error>` row
  `protocol`. Same keys with a 200 `Body::Unreadable` → `Unsettled`, where (B) says `protocol`
  ("XML shape"). It takes a mapping slip in 743.3 to reach this, and the result is INCONCLUSIVE,
  never PASS, so the severity is low. Still, a fatal fact gets hidden. Fix: run the document
  and `<Error>` checks first, and keep the duplicate guard only in front of the `<Deleted>`
  key-by-key match. Add a case beside `crates/validate/tests/capability_matrix.rs:986`.

- NEEDS-HUMAN [human] — **The open 743.3 case in the brief can be partly settled by the matrix
  today.** The brief leaves "a PUT … 2xx … a body framed and then cut before its first byte"
  for a human to assign to 743.3. The matrix already has the fact that decides it when the head
  carries a length: PUT 200, strong `ETag`, `Content-Length: 5`, cut after 0 bytes →
  `Unsettled` (`crates/validate/src/matrix.rs:943-944`, reached from `:1004`). A success that
  declares a body has a body, which is `protocol`. Whether the matrix should judge this from
  the header (and so close the length-framed half of the open case), or leave it all to 743.3,
  is a scope call. It does nothing for chunked framing, which declares no length.

- `FailureClass`'s `Display` (`crates/validate/src/matrix.rs:737-746`) is the one surviving
  mutant (`gate-logs/C5-mutants.log:13`). No test pins the §7 class names (`integrity`,
  `oracle`, `unexpected-support`, `protocol`) that 743.3's report will print. One
  `assert_eq!(Protocol.to_string(), "protocol")`-style line per class would kill the mutant.
  This is a nit and not blocking; I checked by hand that the strings are correct today.

## What I tried and could not refute

- **The evidence.** C4-verify is UNVERIFIABLE (`gate-logs/C4-verify.log`), but that is the
  net-new exception the brief accepts. To stand in for it, I made 23 hand-written changes to my
  copy of the module: all 15 reds the brief requires, plus the iteration-8 HeadBucket ordering
  bug (moving `expect_no_body` back after the cut arm at `matrix.rs:827-835`), a 5xx on the
  follow-up `HEAD` read as unsettled, the hit-form tag guard removed, a cut-off listing called
  `protocol`, an empty listing passing, the strict-retry rule removed, and the vacuity rule
  removed. **Every one turned the suite red**, each in the test named for that leg. The tests
  call `wyrd_validate::matrix` directly, not a copy, and none of them passes for the wrong
  reason that I could find.
- **The rules.** I attacked the 5xx-is-`protocol` rule on every row (`:867-874`, `:1133-1135`),
  404 → `oracle` on reads only (`:878-885`), the conditional-GET tag guards (`:1040`, `:1055`),
  the early PUT acknowledgement (`:991-993`, which fires before the ETag check), the NoBody
  fix (`:827-829`), and the `<Deleted>`/probe folding (`:1142-1183`, including retries and
  probes of keys that were never requested). None of them broke.
- **The grammars.** `parse_version` handles `+git.unknown[.dirty]` after a pre-release, signs,
  leading zeros, empty identifiers and u64 overflow. `0.1.0-rc.1` selects `pre-0.1`.
  `parse_content_range` refuses signs, `*` and a missing size. `is_strong_etag` refuses `*`,
  `W/`, unquoted values, inner quotes and DEL. All of these hold.
- **The citations.** Every `pre-0.1` citation matches the gateway source in the target tree
  (`crates/gateway-s3/src/lib.rs:121`, `:410-422`, `:1693-1699`, `:1766-1788`, `:1797-1815`,
  `:1819-1826`, `:1842-1849`, `:1865-1871`, `:2008-2028`, `:2293-2300`, `:2713-2719`,
  `:2785-2803`, `:2995`, `:3401`).
- **The C4-ci red.** It is only the inherited `cargo deny` advisories (RUSTSEC-2026-0385/-0386,
  `gate-logs/C4-ci.log:3856-3945`), which the human accepted. The matrix test is green in the
  same run.
<!-- pdca:leaf-complete -->
