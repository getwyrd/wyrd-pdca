# Adversarial review — #937 (743.1, capability matrix)

Short version: I tried hard to break the classifier and could not. Its 48 tests are real
discriminators. I re-ran them green on a scratch copy of `$PDCA_TARGET`. Then I made four
temporary edits to production code, matching the brief's reds (7), (10), (2) and (8). Each one
turned the suite red, and every leg calls `wyrd_validate::matrix` directly, not a copy. C5
reports 0 surviving mutants out of 128 caught. The one gating red is a stale-base environment
fault, not the fix. Two smaller items follow.

## Findings

- NEEDS-HUMAN [human] — **The gating `C4-ci` red comes from a stale base, not from this
  patch.** The worktree is on `93a6f37`, not the `b552ca2` that the iteration-3 carry-forward
  told this round to rebuild on (`brief.md:235`). The target's lockfile still pins
  `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`Cargo.lock:3953-3954`). Those
  are exactly RUSTSEC-2026-0385 and -0386, the only failures (`gate-logs/C4-ci.log:3865-3872`,
  `:5168-5170`). Every other CI step passed both attempts: fmt, clippy, build, the whole test
  run with `capability_matrix` at 48/48 (`C4-ci.log:3179-3229`), typos, docs lint and render,
  the blackbox and unsafe guards. The gates also measured two different bases: `C4-diff-cov`
  compiled `ctutils v0.4.5` and `tokio v1.54.1` in another worktree (`C4-diff-cov.log:12-13`),
  while `C4-ci` ran on the old one. The patch adds files plus one `lib.rs` line and a draft-doc
  edit, so it does not depend on the base. But the gating row cannot go green until the driver
  rebases the worktree onto current `main` and re-runs it. That is a harness or human action,
  not build work.

- NEEDS-HUMAN [impl] — **The 0017 amendment leaves the proposal contradicting itself.** Amended
  §3 now says every response carries `Server: wyrd/<version>`. It still points the reader to
  *Dependencies* (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-265`). That
  section still says "**The gateway advertises no version.** No `Server:` header, nothing on
  the wire … Interim: `--server-version`" (`:873-878`). Before the patch the two passages
  agreed. Now the new sentence cites a paragraph that says the opposite. Fix: in *Dependencies*,
  record that #736 landed through #886, or drop the cross-reference. The brief named only
  `0017:259-261`. If the builder treats `:873-878` as out of scope, decline it with an issue
  reference rather than leaving it silent.

- NEEDS-HUMAN [human] — **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a
  key the run just wrote is classed `protocol`, not `oracle`.** Probe on production:
  `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the
  same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`)
  running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with
  the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as
  "model and server disagree", and a written key that reads as absent fits that. But §7's
  `protocol` row ("wrong status") also fits. Only the class label in the report changes, and
  later scenarios that key on the class (#746) will inherit whichever reading is chosen. A
  human should pick one.

## Attempted, could not refute

- **The evidence.** I re-ran `cargo test -p wyrd-validate --test capability_matrix` on a copy:
  48 passed. My own reds, each restored afterwards: removing the `+git.unknown` check
  (`matrix.rs:555-557`) fails `unknown_is_never_read_as_release_0_0_0`. Making a no-answer
  probe `oracle` (`:1189-1191`) fails 4 tests. Treating a 2xx head with a failed body as no
  answer (`:929-935`) fails 1 test. Counting a `supported, not checked` row as unexercised
  (`:1425-1428`) fails 1 test. `C4-verify` UNVERIFIABLE is the brief's pre-declared new-API
  exception (`brief.md:138-150`), not a gap.
- **Content-Range grammar** (`matrix.rs:1314-1329`). These all give `protocol`: a double
  space, `bytes=2-5/16`, a tab before or after, `2 -5`, and every `+` sign form. Leading zeros
  (`02-05`) pass, which RFC 9110's `1*DIGIT` allows.
- **Strong ETag** (`:1302-1310`). DEL (0x7F), a tab, `W/""` and a list `"a", "b"` are refused.
  obs-text (`"é"`) is accepted, as the RFC allows.
- **Versions** (`:505-564`). `0.1.0+git.unknown`, `1.2.3-rc.1+git.unknown` and
  `0.0.0-alpha+git.unknown` are all Unknown. `0.1.0+git.dirty` (a dirty-tag build, see
  `crates/server/src/version/derivation.rs:59-61`) is 0.1.0, which no profile covers. `0.0.0+git.unknown+x` is a parse
  error. `0.0.0+GIT.UNKNOWN` reads as a release, but the derivation only ever emits lowercase.
- **Bulk delete.** A Quiet-mode empty `<DeleteResult>` gives `protocol`. A 404 probe that
  carries a body gives `protocol`. I tried mis-paired, duplicate, unrequested and torn-document
  probes; all are handled (`:1213-1259`).
- **A 2xx carrying an S3 error code passes** PUT, HEAD, GET and DELETE (`:962-970` never looks
  at `error_code` on a success). Not raised: today's client never produces that observation.
  For any 2xx it returns `S3Error::Unreadable` before reading an error document
  (`crates/validate/src/s3.rs:363-368`). 743.3's mapping should keep it that way.
- **A 501 to a delete's follow-up HEAD is unsettled** (`:1198`), while a 501 to a required row
  is `protocol` (`:965`). This matches the brief's "a 5xx → no verdict for the row" rule
  (`brief.md:62`).
- **An exercise the profile does not declare** makes the run INCONCLUSIVE, even a 2xx
  (`:1407-1409`). This cannot happen with `pre-0.1`, which declares all 22 operations. Worth
  remembering when MinIO's profile (#751) declares fewer.
- **`pre-0.1` citations.** I checked each one against `crates/gateway-s3/src/lib.rs` on this
  base: the copy-source guard at `:1865-1871`, the object-path denylist at `:1842-1849`, the
  bucket-path denylist at `:1797-1804`, the `_ =>` 400 arm at `:1809-1815`, the root-path 400
  at `:1819-1826`, the 204 delete at `:2026-2028`, and `UNKNOWN_SERVER_VERSION` at `:121`.
  #508 is not on this base (`uploads`, `uploadId` and `partNumber` are still at `:411-413`), so
  the six multipart rows are rightly `unsupported`.
- I did not re-raise the three open design points that sign-off left for the human
  (`brief.md:235`): strictness on retries, the `0.1.0-rc.1` profile, and an empty-prefix
  listing.

<!-- pdca:leaf-complete -->
