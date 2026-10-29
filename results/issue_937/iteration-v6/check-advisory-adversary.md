# Adversarial review — #937 (capability matrix), round 6

**How I checked.** I built a small scratch crate that compiles the target's own
`crates/validate/src/matrix.rs` through `#[path]` (the production file, not a copy), with a
stub `ErrorCode` that matches `crates/validate/src/s3/error.rs:69-76`. The shipped
`capability_matrix.rs` passes 53/53 against it, which matches `gate-logs/C4-verify.log` and
`gate-logs/C4-diff-cov.log`. Then I ran probe inputs and two hand-made mutants against the
same file.

**C4-verify is UNVERIFIABLE (exit 77).** The brief declares this up front (`brief.md:138-141`):
the module is new, so the test cannot compile with production reverted. That is not a
refutation.

## Findings

- NEEDS-HUMAN [human] — **The gating C4-ci red comes from the base, not from this patch, for
  the third round running.** `cargo deny` fails on ctutils 0.4.2 (RUSTSEC-2026-0385) and tokio
  1.52.3 (RUSTSEC-2026-0386) (`gate-logs/C4-ci.log:3873`, `:3955`, `:9367-9368`). The target
  pins exactly those versions (`Cargo.lock:1000-1001`, `Cargo.lock:3953-3954`), and its base
  commit is `93a6f37`. The carry-forward at `brief.md:235` said to rebuild on `b552ca2`, which
  carries #950 and #954. The patch touches no dependency. Someone has to refresh the base, or
  waive the red at sign-off. Another Do round will not fix it.

- NEEDS-HUMAN [impl] — **Two guards can be deleted with the suite still 53/53 green.** I ran
  both mutants. (a) `crates/validate/src/matrix.rs:908-914`, the check that the exercise matches
  the row. With it deleted, `classify(PRE_0_1.row(PutObject), &Exercise::GetObject { object,
  200 + the right bytes })` returns `Pass`, so a GET answer passes the PUT row. `verdict` cannot
  reach this, because it looks the row up from the exercise. But `classify` is `pub`, and 743.3
  may call it directly. Coverage agrees: lines 909 and 911 never ran
  (`gate-logs/C4-diff-cov.log`, the MISS list). (b) `matrix.rs:990-992`, which makes "a document
  where object bytes were expected" a `protocol` failure. Changed to `Ok(&[])`, a GET of an empty
  object answered `200` with a `Body::Listing` passes. For a non-empty object the failure class
  quietly changes from `protocol` to `integrity`. Line 991 is also on the MISS list. Fix: add one
  fixture for each, beside `capability_matrix.rs:538-546` and `:677-697`.

- NEEDS-HUMAN [human] — **The two delete rows treat a retried HEAD differently, and the
  module's own doc contradicts one of them.** In one bulk exercise, follow-ups
  `[(a, 503), (a, 404)]` give `Pass` (`matrix.rs:1248-1251`, pinned at
  `capability_matrix.rs:856-862`). `DeleteObject` carries exactly one `follow_up: Answer`
  (`matrix.rs:768-772`), so the same retry has to be sent as a second exercise. The
  "every exercise must pass" rule (`matrix.rs:1393-1396`) then makes the run INCONCLUSIVE. I
  probed it: "not proved: DeleteObject: … answered 503". So the same evidence (one 503, then a
  404 for the same key) proves the bulk row and not the single row. The bulk result also breaks
  the module invariant at `matrix.rs:26-28` ("a row with such a check and no fatal one is
  unproved"), and a strict reading of `brief.md:60-63` ("a 5xx → no verdict for the row"). This
  is open design point (1) in a new place (`brief.md:235`). Pick one retry rule, apply it to
  both rows, and make the rustdoc match.

- NEEDS-HUMAN [impl] — **A vacuous PASS is still possible at the profile level** (low
  priority: `PRE_0_1` cannot hit it today). `verdict` returns PASS whenever nothing is fatal and
  nothing is unproved (`matrix.rs:1436-1442`). A profile with no `required` or `unsupported`
  rows therefore passes an empty run. I probed it two ways: a profile holding only
  `not_checked(UploadPart)`, and a profile with no rows. Both give `verdict(&p, &[])` = PASS.
  Separately, `Profile::new` (`matrix.rs:250-252`) accepts a profile that declares the same
  operation twice, and `Profile::row` (`:267-269`) silently uses the first row. A profile
  declaring `PutObject` both `required` and `unsupported` passes a PUT 200. The no-duplicates
  check exists only for `PRE_0_1` (`capability_matrix.rs:314`). This is the rubric's
  "count-based assertion that can pass while the property fails" class. Fix: either make "zero
  proved rows" INCONCLUSIVE, or add a test over `PROFILES` requiring every profile to have at
  least one checked row and no duplicate operations.

- NEEDS-HUMAN [human] — **The brief's 5xx rule for unsupported rows conflicts with 0017 §7**
  (low priority; the code follows the brief). A `503 SlowDown` to `CopyObject` is fatal
  `protocol` (`matrix.rs:936-941`, pinned at `capability_matrix.rs:380`). The same 503 to
  `PutObject` only leaves the row unsettled (`matrix.rs:965-967`). 0017's taxonomy table budgets
  every 5xx as `availability` (`0017:508`), while §3 says an unsupported row must not answer "a
  500" (`0017:194`). The brief settled it as "a 5xx included" (`brief.md:33-34`). The cost: one
  transient 503 on any of the twelve unsupported rows turns a smoke run FAIL instead of
  INCONCLUSIVE. Confirm that this is intended.

- Note, no action needed: a GET or HEAD of a key the run just wrote that answers 404 is classed
  `protocol` (`matrix.rs:1081`, `:1124`). `brief.md:52` says HEAD "else `oracle`". Both classes
  are fatal and the run outcome is the same; only the label differs.

- Note, no action needed: `Content-Length` is not checked against the body on GET or 206 (a
  probe with `Content-Length: 16` and 4 correct range bytes passes, `matrix.rs:1079-1111`). On a
  real connection, hyper reports a short or long body as a transport error, so 743.3's mapping
  should already produce `Body::Failed`. This only matters if that mapping passes through a
  header the transport did not enforce.

## What I tried to refute and could not

- **The 10 required reds** (`brief.md:142-150`). The C5 gate shows 0 of 128 viable mutants
  missed (`gate-logs/C5-mutants.log`). Every named leg has a pinning test.
- **Unsupported rows.** Any 2xx is `unexpected-support`, including a 2xx whose body then
  failed. No response head gives no verdict. A declared refusal whose body was cut off is
  unsettled, not fatal.
- **PUT.** A strong `ETag` is required (`*`, unquoted, `W/` and empty are all refused). An
  early 200 is `protocol` whatever its `ETag`. A torn 200 is unsettled.
- **Ranged GET.** The `Content-Range` grammar refuses signs, a missing unit, `*/size` and
  other units. The unit match is case-insensitive.
- **Bulk delete.** Each follow-up is paired with its own key. A torn `<DeleteResult>` still
  lets a readable key's probe fail the row. Keys requested twice and unrequested probe keys are
  both handled.
- **Version parsing.** Leading zeros, `+` signs, empty identifiers, overflow and `_` are all
  refused. `+git.unknown[.dirty]` reads as Unknown. `+GIT.UNKNOWN` reads as release 0.0.0, but
  the version derivation only ever emits lowercase (`crates/server/src/version/derivation.rs:65`),
  so that is not a defect.
- **Every `PRE_0_1` citation** matches the gateway source at the cited line
  (`crates/gateway-s3/src/lib.rs:1766`, `:1797-1815`, `:1819-1826`, `:1842-1849`,
  `:1865-1871`, `:2008`, `:2017`, `:2025-2028`, `:2713-2719`, `:2785`, `:2995`, `:3401`).
- **#508 is not on the base.** `lib.rs:410-413` still lists `uploads`, `uploadId` and
  `partNumber`, so the six multipart rows are correctly `unsupported`.
- **The 0017 amendment's citations** check out (`lib.rs:1693-1699`, `crates/server/build.rs:117`,
  `xtask/src/dist.rs:546`). The old `*Dependencies*` contradiction is gone (`0017:873-880`).
<!-- pdca:leaf-complete -->
