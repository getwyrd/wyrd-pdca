# Adversarial review — issue 620 (s3-object-key-length-limit)

The PUT-side fix held against every input I tried. I found one concrete regression: the new
bulk-delete bound removes the only S3 way to delete an object whose key is too long to fit in a
single-object DELETE request. That object is now stranded, which breaks the brief's own invariant.
All runs below used a scratch copy of `$PDCA_TARGET`, with the patch's own test harness.

- NEEDS-HUMAN [human] — **The patch strands objects whose keys are too long for a request line.**
  `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The
  patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for
  *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and
  `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on
  `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request
  target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over
  65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed
  `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`,
  DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the
  pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is
  gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key
  from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no
  `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys
  that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte
  becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it
  can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3)
  demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable,
  and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and
  keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and
  record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of
  scope).

- NEEDS-HUMAN [impl] — **The bulk-delete leg doesn't check that the parser counts bytes rather than
  characters.** Leg 3 uses only ASCII keys (`crates/server/tests/s3_key_length_limit.rs:266`,
  `:290`). I changed `crates/gateway-s3/src/lib.rs:2266` from `key.len()` to
  `key.chars().count()`, and all 5 tests still pass. Under that change, a 342 × `€` entry
  (1,026 bytes) would be accepted. The current code does refuse it: I probed it, and it returns
  `400 KeyTooLongError` with the ordinary key untouched. Fix: add a multi-byte over-limit entry to
  leg 3. This is moot if the human drops the bulk bound per the finding above.

- **The C5-mutants red is a scoping artifact, not a weak test.** The mutants run executed only
  `wyrd-gateway-s3`'s own tests (about 1 s per mutant). The new test lives in `crates/server/tests`.
  I applied all five "missed" boundary mutants by hand: `:1835` `>=` and `==`, and `:2266` `>=`,
  `==` and `<`. The new test kills every one. It also kills these: measuring the raw wire key,
  measuring `bucket/key`, counting characters on the PUT side, and dropping the `/`-in-bucket check
  (`:1825`). `key_too_long → Default::default()` returns `200`, which every leg's `400` assertion
  catches. The remaining two (`delete_objects → Default`, the deleted `"Object"` arm) are in
  pre-existing code covered by `s3_delete_objects.rs`.

- **The C4-ci red is not this patch.** The failure is `RUSTSEC-2026-0346` / `RUSTSEC-2026-0349`
  on `rustls 0.21.12`, pulled in by `tikv-client` (`Cargo.lock:302`), in the `--all-features`
  `cargo deny` advisories step. The patch touches no dependency. In that same run every test passed,
  including the new file and the 22 `s3_delete_objects` tests.

- **The red→green evidence holds.** I re-ran the new test and got 5/5 green. The C4-verify log
  shows legs 1, 2, 3 and 5 failing on the base for the right reason (`200` where `400` was
  expected), and leg 4 green both ways, as the brief intends. The test goes through the real
  path: a loopback listener, SigV4, `dispatch`, and the redb + fs backend. It does not use a copy
  of production code.

- **Things I tried that did not break the fix:**
  - A lowercase `%2f` in the bucket is refused with `InvalidBucketName`.
  - `%FF` × 342 is lossily decoded to 1,026 bytes and refused. The string measured at `:1835` is
    the exact string stored at `:1775`, so measurement and storage can't disagree.
  - `//b/<key>` splits to bucket `b`, so there's no alias.
  - The backend key is the flat `dirent:<root>/<bucket>/<key>` (`crates/core/src/metadata.rs:39`)
    with no normalization, so the `%2F` bucket was the only alias to close.
  - A bulk key of 1,024 × `&amp;` is accepted, because the parser measures after entity decoding.
  - A PUT with `Expect: 100-continue` and a declared 1 MiB body, with the body never sent, gets
    `400 KeyTooLongError` within 5 s. The refusal really does come before the body is read.
  - Both refusals sit after SigV4 verification, so an unauthenticated caller learns nothing.

- **About the T4 review's 7 blocking findings (for the human weighing that gate).** Four are about
  the test harness: no socket timeout (`s3_key_length_limit.rs:161-165`) and a discarded server
  `JoinHandle` (`:95`). Every peer suite the brief told the builder to mirror does the same
  (`s3_delete_objects.rs:96,204`, `s3_list_objects.rs:113,206`,
  `s3_range_conditional.rs:176,228`). Fixing it in one file only would split the style; a
  repo-wide issue fits better. The other three are docs-currency findings. `docs/design/architecture/`
  never documents per-operation S3 errors: it doesn't mention DeleteObjects, `MalformedXML`, or
  CopyObject's `501`. So there is no living section to update, and whether a new `400` counts as
  "altering an API operation" is a judgment call.

<!-- pdca:leaf-complete -->
