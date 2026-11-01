# Adversarial review — issue 1005 (capability matrix core, 743.1a)

**Bottom line: I tried to refute the fix and could not.** No NEEDS-HUMAN findings. Below are
the attacks I ran and the two small leftovers I found; neither is a defect in today's code.

## The evidence (red→green)

- `C4-verify` reports UNVERIFIABLE (`gate-logs/C4-verify.log`: the test does not compile
  without the new module). The brief expects this and accepts it. Since `build-notes.md` is
  withheld from me, I re-ran the reds myself. In a scratch copy of `$PDCA_TARGET` I made 14
  targeted mutations to the production `crates/validate/src/matrix.rs` and ran the real
  `cargo test -p wyrd-validate --test capability_matrix` against each one. 13 turned the test
  red, including all four the brief names:
  - removing `unexpected-support` (`matrix.rs:943`): 4 tests red;
  - letting any code match (`matrix.rs:75`): 2 red;
  - reading `+git.unknown` as a release (`matrix.rs:472`): 2 red;
  - dropping the "required-row checks not yet defined" finding (`matrix.rs:1002-1004`), so an
    unsettled `required` row would fold into PASS: 3 red.

  The other nine also went red: `unread` ignoring a code that was read (`:986`), `settle` not
  returning on the first fatal (`:875`), the 5xx fact exempting every `unsupported` row
  (`:928`), the vacuity check removed (`:1128`), undeclared operations ignored (`:1078`),
  pre-release ignored in `cmp_bound` (`:374`), a `HeadBucket` cut after 0 bytes treated as
  `protocol` (`:962`), body judged before status (`:864-871`), and a `not checked` row
  passing (`:915`). Every test calls `wyrd_validate::matrix::{classify, verdict,
  select_reported, parse_version}` directly. Nothing re-implements the code under test.
- The one mutation that survived is a test gap, not a production bug. If the code check is
  skipped whenever the body is `Body::Bytes` (`matrix.rs:986`, e.g.
  `unread = … || matches!(response.body, Body::Bytes(_))`), all 30 tests still pass. The input
  nothing pins: `CopyObject` answered `501`, `error_code: Code("InvalidRequest")`,
  `body: Body::Bytes(b"<Error>…")`. Production calls that `protocol`, but a broken version
  would pass it. The only `Bytes` leg (`capability_matrix.rs:604`) uses the right code. I
  did **not** raise this as NEEDS-HUMAN: the real client cannot produce this shape, because
  `S3Error::Service` carries the code but no raw body
  (`crates/validate/src/s3/error.rs:16-25`), so #939 will map a readable error to
  `Body::Empty` plus the code, which is the form the tests do pin. Adding one leg would be
  cheap hardening, nothing more.

## The fix — inputs I tried that did not break it

I ran a throwaway probe test against the production functions:
- A cut error body on the declared `501` whose bytes before the cut already hold a wrong
  `<Code>InvalidRequest</Code>`, with no code read by the client: **unsettled**
  (`matrix.rs:974-976`, `:986`). This matches brief (D) ("a cut-off error body on the
  declared status is unsettled"). The client buffers error bodies whole and never reads a
  code from a partial one. Not a refutation. #939 should keep mapping a cut error body to
  `Arrived::Count`, as `Arrived` (`matrix.rs:624-630`) says.
- A `204` delete whose follow-up `HEAD` got a `503`: unsettled. Follow-up checks belong to
  child-2 under the brief. Not this diff's job.
- A 5xx to an operation the profile has no row for: INCONCLUSIVE, not FAIL
  (`matrix.rs:1077-1079`). Brief (H) asks for exactly this, and `pre-0.1` cannot reach it,
  since all 22 `Operation` variants have rows (`matrix.rs:284-337`).
- Odd statuses (0, 99, 600, 999): `protocol` on an `unsupported` row and unsettled on a
  `required` row. Sound.
- Contradictory observations (`501` + `ErrorCode::NoBody` + cut; `400` `HeadBucket` +
  `Unreadable`): `protocol`, which is fail-closed.
- Version grammar: `0.0.0-rc.1+git.unknown` gives Unknown. `x+git.unknown` gives a parse
  error. `0.1.0--rc`, `0.0.0+git.0123abc` and `1.0.0-0` parse as valid SemVer, and select
  `pre-0.1` / `pre-0.1` / no match. `0.1.0-rc.1+git.dirty` selects `pre-0.1`. Leading
  zeros, `+` signs, empty identifiers and whitespace are all refused (`matrix.rs:426-493`),
  which meets the rubric's grammar-strictness rule.
- Citations: I checked every `pre-0.1` row's cited line in
  `crates/gateway-s3/src/lib.rs` on the target (`:410-422`, `:1548-1553`, `:1766-1788`,
  `:1797-1826`, `:1842-1849`, `:1865-1871`, `:2008-2028`, `:2293-2300`, `:2704-2725`,
  `:2785-2803`, `:2995`, `:3401-3404`). Each one gives the answer its row declares.
  `HeadBucket` reaches the bucket-path `400` and loses its body at `:1552-1553`, so the
  client sees `ErrorCode::NoBody`.

## The verdict — claims I checked

- `check-gates.json`: C4-ci ran the 30 new tests green, plus typos and docs-check
  (`gate-logs/C4-ci.log:11-19`, `:3202-3236`). C5-mutants reports 89 caught and 0 missed;
  that is honest, but it does not mean every mutation was tried (see the surviving one
  above). Nothing a gate claims is wrong.
- Diff coverage misses `matrix.rs:1093-1098`. That arm (`RowResult::NotChecked` on a row
  that must be proved) cannot run: only a `supported, not checked` row produces
  `NotChecked`, and the `!must_prove()` arm at `:1088` catches it first. It is harmless
  defensive code, not a gap in the tests.
- Scope: `lib.rs` changes by one line. The proposal is `status: draft`, so editing it is
  allowed. No `Cargo.lock`, `deny*.toml` or gateway change.

<!-- pdca:leaf-complete -->
