# Build notes — #956 (743.2, second child of #938), iteration 4: bulk delete, probes, endpoint resolution

## Base and starting point

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `635e0bd` (`pdca-integrate: issue_955`).
  `patch.diff` is the whole change against that commit: 6 files. The worktree is left at `635e0bd` with the change
  uncommitted.
- Salvage hashes rechecked, both as the brief states: `patch.diff` `b50cc4f5…7d69`, `build-notes.md` `f2c4fcf9…32f7`.
- Starting point: `iteration-v3/patch.diff`, which applied cleanly, because the sign-off says "Keep the routing, probe
  and row-reading code; it held up… Fix these only."
- **No production logic changed this round.** The only production-file change is the doc comment on `delete_objects`.
  Everything else is the test file and one sentence of the architecture doc.
- `crates/validate/src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `crates/validate/Cargo.toml` and the three older
  `s3_client_*` test files are untouched (`git diff --stat` on them is empty). `deny*.toml` is not in the diff.

## Iteration 3's carry-forward, item by item

Line numbers are in the patched tree. `tests` is `crates/validate/tests/s3_client_smoke_verbs.rs`.

| # | What the sign-off asked | What changed |
| --- | --- | --- |
| 1 | State the `<Error>` row's size in both places; say the 1,000-key figures assume no failures | `crates/validate/src/s3/verbs.rs:350-360`, `docs/design/architecture/05-building-block-view.md:303`. Both now give 113 bytes for `InternalError`, 83 more than `<Deleted>`, "with no failures" on the 1,000-key figures, and the 41/42 boundary. |
| 1 | Pin the 41/42 boundary in a test; keep 64 KiB | New `a_bulk_delete_with_failed_keys_fits_the_budget_with_41_internal_errors_and_not_42` (`tests:681`). The budget is unchanged (`crates/validate/src/s3/response.rs:54`). |
| 2 | Assert `received > BUFFERED_BODY_BUDGET`, and `content-length == 70115` separately | New helper `assert_refused_over_budget_beside_its_200` (`tests:611`), used at `tests:661`. |

### The numbers, checked

`<Error><Key>K</Key><Code>InternalError</Code><Message>the gateway could not complete the request</Message></Error>`
(`crates/gateway-s3/src/lib.rs:2702`, text from `classify`'s last arm, `:4064-4068`) is 113 bytes without the key.
`<Deleted><Key>K</Key></Deleted>` is 30. 1,000 keys of 32 bytes with `e` error rows: `62,115 + 83e`. With 41 that is
65,518, which fits 65,536. With 42 it is 65,601, which does not. The test reads these from the gateway's own
`Content-Length`: `322` for one row of each, then `65518`, then `65601`. They match the adversary's figure.

### Choice: the real gateway's `<Error>` rows, not a scripted copy

The adversary showed the boundary with a scripted endpoint sending rows in the gateway's format. That would prove only
that the client's budget is 65,536, against a copy of the format the test's author typed. The sentence being fixed is about
the gateway's rows, and brief (H) forbids a sentence that claims more than the tests show. So the new test makes the real
gateway write them.

The fault is a directory entry the store cannot decode, written with the same direct commit `seed_bucket` uses
(`tests:685-690`), before the store goes into the gateway. `metadata::unlink` then fails at `decode`
(`crates/core/src/metadata.rs:2153`), `delete_objects` turns that into a per-key `<Error>` row
(`crates/gateway-s3/src/lib.rs:2513-2541`), and `classify` answers `InternalError`. The test's `one of each` leg checks
that row's key, code and message as well as its size, so a fault that took a different path would fail there.

The fixture change is small: `start_gateway_seeding` splits so the serving half is `start_gateway_on(meta)` (`tests:259`).

### Alternatives rejected

- **A failing `MetadataStore` wrapper in the test** (the pattern at `crates/core/tests/multipart_abort.rs:484`). The trait
  uses `async_trait`, which `crates/validate` does not have, so it needs a new dev-dependency (a `Cargo.toml` and
  `Cargo.lock` change) plus about 30 lines for four methods. The undecodable entry needs 5 lines and no new dependency.
- **A scripted endpoint** (about 20 lines and no fixture change, against about 80 for this test and its fixture split). See
  above: it stays green if the gateway's row changes. `it4c`, `it4d` below show the chosen test goes red in that case.
- **Stating the every-key-fails figure too** (451 keys of 32 bytes, from the adversary). The sign-off did not ask, and it
  would be one more number to pin. The doc gives the row sizes, so a caller can work it out.

## The other gates from iteration 3

**C5 (advisory), still expected to show `fail`.** Iteration 3: 64 mutants, 47 caught, 14 unviable, 3 timeouts, 0 missed.
No production logic changed, so the mutant list is the same, and I did not re-run it (17 minutes). The sign-off says "Fix
these only" and does not name the three timeouts. Iteration 3's notes and its adversary found them slow failures inside
`s3_client_nonconforming_responses`, which brief (E) forbids changing. The new test adds about 0.6 s to the file's run.

**Code-review finding at `tests:638`:** this is item 2, fixed.

## Red → green

Verification posture (a), net-new. With production reverted, the test file does not build, as the brief pre-declares.
I did not re-run that revert. The build failure was shown in iteration 2 (`E0432` on the imports) and cannot have changed.

Green on the final tree, through `./engine/xtask.sh ci`: `s3_client_smoke_verbs` 53 passed.
**(E):** in that same run `s3_client_nonconforming_responses` 28, `s3_client_roundtrip` 15, `s3_client_upload_peers` 12,
all passing, all three files unmodified.

### Demonstrated reds

Each is one edit, a run of the test file, then the file restored by a script
(`$PDCA_SCRATCH/pdca-builder-956-it4.py`, log beside it). All ran this cycle on the final tree, each exited 101 with no
build error.

The six the brief requires:

| Red | Edit | Tests that fail, and how |
| --- | --- | --- |
| (1) | `probe` returns only the timeout when its body stalls after a 2xx | `a_probe_answered_200_keeps_its_200…`: `22 case(s) lost or misreported the 200` |
| (3) | The SDK's default endpoint resolution back (`DefaultResolver`) | `bucket_probes_send_every_name…`, `an_endpoint_path_comes_before…`, `the_region_is_signed_as_given…`, `a_request_stopped_before…` (`Invalid region`, not the query) |
| (5) | `<Error>` rows dropped | `a_mixed_delete_result…`, `a_delete_result_reports_requested_keys…`, `a_delete_error_row_with_no_code…`, and now `a_bulk_delete_with_failed_keys…` |
| (6) | A request refused before sending reported as `NoResponse` | `a_request_stopped_before…`, `a_read_verb_stopped_before…`, `a_probe_whose_key_and_upload_id_just_fit…` |
| (16) | A probe's expired deadline turned into `NoResponse` | both probe deadline tests |
| (17) | `UploadPart` with another `uploadId` | `every_unsupported_operation…`, `a_2xx_carrying…`, `a_304_that_frames_a_chunked_body_is_refused_only…` |

New for this iteration's items:

| Edit | What happened |
| --- | --- |
| `it4a` Budget 65,517, one byte under the 41-row result | Only the new test fails, at the 41 leg: `65518 bytes received when it stopped reading` |
| `it4b` Budget 65,601, exactly the 42-row result | Only the new test fails, at the 42 leg: `not refused beside the 200: Ok(DeleteResult…` |
| `it4c` The gateway's `InternalError` message one byte longer | Only the new test fails: `content-length` 323, not 322. A scripted copy of the row would have stayed green. |
| `it4d` The gateway's `<Message>` tag renamed `<M>` | Only the new test fails: `content-length` 310 |
| `it5a` `received` stops at the budget, not past it | Both budget tests fail, and the table test (D) |
| `it5b` `received` one byte lower, as another TCP split would report a different number | **Both budget tests stay green**, where the old `received: 70_115` would fail. This is item 2's fix. The run as a whole exited 101: `listing_and_delete_result_documents…` fails, because for a document one byte over the budget this edit gives exactly the budget, which `refused_as` rightly refuses. That failure comes from my edit and is not a defect. I predicted all green and was wrong about that case. |
| `it5c` The gateway's `<Deleted>` markup one byte longer | Both budget tests fail on the size: 189, not 187, in the two-key leg |

I did not reproduce the TCP split itself. `it5b` stands in for it by changing the number `read_whole` reports. The argument
is by reading: `read_whole` returns the running total at the first frame crossing the budget (`crates/validate/src/s3/answer.rs:617`),
so the only thing true of it on every split is that it is past the budget. That is what the helper now asserts.

Iteration 1 to 3's `cf*` and `it1*`–`it3*` edits were not re-run this round. That code did not change, and their tests still pass.

## Refuting my own test

- **(a) Genuine red? Yes.** A full revert does not build, the case the brief pre-declares. Beyond that, each behaviour was
  broken on its own, and every edit above except `it5b`'s two budget tests failed a named test at assertion level.
  `it4a` and `it4b` bracket the boundary from both sides, so neither 41 fitting nor 42 refused is vacuous.
- **(b) Production path? Yes.** The new test goes through `resolve_config` and `S3Client::with_deadlines` to the production
  `delete_objects` and `delete_rows`, in the real `aws-sdk-s3` orchestrator over the client's connector on real loopback TCP,
  to the real `S3Gateway`, real `Gateway` and real redb store. No mock of the client, SDK or gateway.
- **(c) Fixture includes the fault? Yes.** The 42 store records really cannot be decoded, the gateway really fails those
  keys, and its own code writes the rows. A sampled key with no fault really is gone after the refusal (`tests:739-750`),
  so "after the gateway has deleted the keys" holds for this case too, on one key, not all 958. What the fixture does not
  include: real S3, whose markup may differ. The docs give the numbers as the Wyrd gateway's, for `InternalError` only.
  Other codes have other lengths, and the docs do not claim otherwise.

Flakiness, since item 2 was a flake: 18 runs of the new test, in 3 rounds of 6 at once pinned to one CPU core, all passed,
each in about 0.6 s. Its size assertions read `Content-Length`, so they do not depend on how the response splits.

## Commit-readiness

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean.
- Whole gate: `./engine/xtask.sh ci` on this tree: `xtask ci: all checks passed`.
- `patch.diff` compared byte for byte with `git diff --binary HEAD`, and it reverse-applies. The test file is also copied
  to the bundle as `s3_client_smoke_verbs.rs`, compared the same way.
- Scratch: my edit scripts and logs are in `$PDCA_SCRATCH`, left for the harness to reclaim, per the builder's filesystem rule.

## External dependencies

`typos` and `docs-renderer`, as the brief lists. No Docker, no network beyond `127.0.0.1`.
No NEEDS-HUMAN external-dependency item.

## For your attention at sign-off

1. **A new test reaches into the store's key format.** It uses `wyrd_core::metadata::dirent_key(0, …)` and the gateway's
   private root inode, 0. If that layout changes, the undecodable entry stops being read, the `one of each` leg sees two
   `<Deleted>` rows, and the test fails loudly. It cannot pass by accident. `wyrd-core` was already a dev-dependency.
2. **`head_object`'s panic on an overlong key is still there, as you ruled.** It is #955's. No issue tracks it yet and I did
   not open one. It needs one. (Carried over from iteration 3.)
3. **The budget test still costs about 6 s** of the file's run, for all 1,000 40-byte keys being written and checked.
   (Carried over.)
4. **C5 will still show `fail`** for the three known timeouts, with 0 missed. Not re-run this round; see above.
5. **The low-severity notes from iteration 3's adversary were left alone**, since the sign-off says "Fix these only": "none
   goes unread" is about the root's children, and `fields` does pass over anything inside a row
   (`crates/validate/src/s3/document.rs:237`, `:293`); and `crates/validate/src/s3.rs:52-56` omits a few refusals.
