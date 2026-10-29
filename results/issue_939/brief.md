# validate: the smoke scenario — the matrix run against a live endpoint, touching only what it wrote (743.3)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`: §3 (`0017:188-275`), §7
> (`0017:482-524`), §8's `smoke` row and scale rules (`0017:526-561`), §15 *Safety*
> (from `0017:782`; ownership `:791-837`). Third child of #743; builds on 743.1 (the matrix) and 743.2 (the verbs).
> Siblings: 743.1 = #937 (the matrix), 743.2 = #938 (the client verbs), 743.3 = #939 (smoke).

- **Slug:** validate-smoke-scenario
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** `wyrd-validate` resolves its configuration and always exits INCONCLUSIVE
  (`crates/validate/src/lib.rs:132-160`): no scenario runs. The in-tree test
  `crates/server/tests/s3_copy_object_guard.rs:175` already proves this tree's gateway refuses
  `CopyObject`. What is missing is a check an operator runs against a **deployed** endpoint,
  keyed to the version it reports, covering every unsupported operation and every required
  one. That is `smoke`. It is also the first part of the tool that writes and deletes on
  someone else's storage, so it must never delete, overwrite or change anything it did not
  write in this run. The first attempt broke that three ways at Check: it could delete a
  bucket it did not own, it reported PASS and "nothing left" after a bulk delete that was
  acknowledged but not done, and it accepted and reused a malformed `ETag`.
- **Success criterion:** the NEW file `crates/validate/tests/smoke_loopback.rs` passes. Every
  leg drives the **real binary** (`env!("CARGO_BIN_EXE_wyrd-validate")`) or the existing
  `wyrd_validate::run` (`lib.rs:132`, signature unchanged) against an in-process Wyrd gateway
  with the test's buckets seeded, through a recording relay where a leg must see or change the
  wire; `--workers 1` and a short `--duration` (e.g. `1s`) unless the leg says otherwise.
  - **(A) PASS against loopback.** Gateway `server_version` `0.0.0+git.<sha>`: exit 0; the
    summary names every `required` and `unsupported` row of `pre-0.1` with its passes; the relay
    saw every unsupported operation's request, `CopyObject` included, answered as declared;
    afterwards nothing is under `<run-id>/` and no key outside it was written or changed.
    **The relay also saw every required row's request, in each wire form the row covers,
    answered by the gateway** (added 2026-10-23; the rows are #937's, the forms 0017 §3's,
    `0017:205-210`): a `PUT` of a key under `<run-id>/`; a whole `GET` with no `Range`, 200; a
    `GET` with `Range: bytes=a-b`, 206; a `GET` with `If-None-Match` equal to the object's
    `ETag`, 304, and one with a different strong tag, 200; a `HEAD`, 200; a single-object
    `DELETE /<bucket>/<run-id>/…` with no query, 204, then a `HEAD` of that key, 404; a
    `POST /<bucket>?delete`, 200, then a `HEAD` of each key it named, 404; a `ListObjectsV2`
    (`list-type=2`) and a v1 `ListObjects` (no `list-type`) under `prefix=<run-id>/`, each
    200. The test asserts each form from the relay's own record, not from the summary: a run
    that skips one (for example one that cleans up only by bulk delete) fails (A) even when
    its summary prints a pass for that row.
    **This slice never edits the matrix** (revised 2026-10-22): the six multipart rows are
    whatever #937's `pre-0.1` declares on your base. #937 records the gateway on its base, and
    #508 moves the rows when it routes the verbs (`results/issue_508/brief.md`, *Scope*). If
    your base has #508 (the gateway answers the multipart forms) while those rows still say
    `unsupported`, (A) fails with `unexpected-support`, and that is the matrix doing its job:
    stop and record it in `build-notes.md` as an item for the human. Do not edit `matrix.rs`
    or `capability_matrix.rs`, and never weaken the classifier to make (A) pass.
  - **(B) A wrong success fails the run.** (i) The relay answers `CopyObject` itself with a 200
    and a `<CopyObjectResult>`: exit with the FAIL status (new, distinct from 0–3), naming
    `unexpected-support` for `CopyObject`. (ii) The relay answers `GetBucketLocation` with a
    200 head and cuts the connection mid-body: still FAIL, `unexpected-support`, never
    INCONCLUSIVE. (`GetBucketLocation` is `unsupported (#511)` in `pre-0.1` whether or not #508
    is on the base; a multipart operation here would stop being unsupported when #508 lands.)
    (iii) **Wrong bytes fail the run** (added 2026-10-22). The relay changes one byte of the
    whole-object GET body and keeps the status, `Content-Length` and every header as the
    gateway sent them. FAIL naming `integrity` for `GetObject`. 0017 makes `integrity` "fatal —
    stop, preserve state and the object" (`0017:489`), so after it the run sends no further
    request: the affected object is still on the gateway, byte-identical to what the run PUT,
    and it is listed as left in place with every other recorded key. (The gateway sends no
    response checksum on GET, so nothing but smoke's own comparison can catch this.)
    (iv) **Wrong bytes in a range fail the run** (added 2026-10-23; 0017 checks "every `Range`
    slice", `0017:407`). The relay changes one byte of a 206 body and keeps the status,
    `Content-Range`, `Content-Length` and every other header as the gateway sent them: FAIL
    naming `integrity` for the ranged `GetObject` (`0017:489`, "bytes … differ"), with the
    same stop-and-preserve rule as (iii). If #937's classifier on your base names another
    fatal class for that answer, assert that class and record it in `build-notes.md` as an
    item for the human; do not edit the matrix.
  - **(C) The version source (decided).** A `Server: wyrd/0.0.0+git.<sha>` header is used and
    the echo says `(from Server header)`. `--server-version` wins over it and the echo says
    `(given)`. With `wyrd/unknown`, `wyrd/0.0.0+git.unknown`, `wyrd/0.0.0+git.unknown.dirty`, or
    no `Server` header, and no flag: INCONCLUSIVE (exit 3) naming `--server-version`, no
    mutating request sent. `--server-version banana`: usage status (2) naming the flag, no
    connection opened. `--server-version 9.0.0` (matches no profile): INCONCLUSIVE naming the
    version, no mutating request.
  - **(D) It touches only what it wrote in this run** (0017 §15):
    - **Occupied prefix** (`0017:791-794`): with one object planted under `<run-id>/`, exit 2
      naming the prefix and `--run-id`; no mutating request; the planted object is
      byte-identical and still the only key there.
    - **Acknowledged, not done** (sign-off red): the relay answers `POST ?delete` with a
      well-formed `<DeleteResult>` listing every requested key under `<Deleted>` and does not
      forward it. FAIL naming `oracle` for `DeleteObjects` and the keys still present; the
      report lists them as left in place and never says nothing was left; they are in fact
      still there.
    - **Bucket probes (decided; sign-off red), one verb per run** (revised 2026-10-23:
      `unexpected-support` is fatal, `0017:491`, so with both verbs faked in one run the first
      probe would end it and the second path would go untested). Two runs. In the first the
      relay answers `CreateBucket` with 2xx, as a server implementing it would, and forwards
      everything else to the gateway; in the second it does the same for `DeleteBucket` only.
      In each run: the relay recorded that verb's request (the test fails if the targeted verb
      never reached it); the run FAILs naming `unexpected-support` for that verb; the name the
      request carried breaks S3's bucket-naming rules and is not `--bucket`; and the test's
      buckets (`--bucket` and a second, pre-existing one holding an object) still exist with
      their objects byte-identical. A run whose `--bucket` equals the name its bucket probes
      would use exits 2 before any request.
    - **Probe-bucket absence check (sign-off red; added 2026-10-23):** before any write, the
      run lists the probe name (`ListObjectsV2`, `max-keys=1`, from a client bound to that
      name; the client binds one bucket, `crates/validate/src/s3.rs:122`, `:164`). Only
      `404 NoSuchBucket` or `400 InvalidBucketName` lets it go on. With the relay answering
      that listing `200` with an empty `ListBucketResult`: INCONCLUSIVE (exit 3) naming the
      probe bucket; the relay saw that listing and no `PUT`, `POST` or `DELETE` at all. (The
      base binary sends no request, so the recorded listing keeps this leg red pre-fix even
      though the base also exits 3.)
    - **Malformed ETag (sign-off red):** the relay rewrites the `ETag` on PUT and HEAD answers
      to `*` (one run) and to the unquoted `bare-etag` (another): FAIL naming `protocol` for
      `PutObject`; the relay saw no `If-None-Match` carrying `*` or the malformed value.
    - **Conditional negative control (decided):** the relay answers every `If-None-Match` GET
      with 304: FAIL naming `protocol` for the non-matching conditional GET.
    - On any FAIL, every key the run wrote and did not prove deleted is listed and left in place.
  - **(E) Scale and arguments** (`0017:528-531`, `:555-561`). Smoke repeats its sweep until
    `--duration` has elapsed (at least one full sweep; a sweep running at the deadline
    finishes). `--duration 2s`: PASS, the summary states sweeps and elapsed time, elapsed ≥ 2 s.
    `--workers` other than `1`: INCONCLUSIVE (exit 3) naming `--workers`, after the
    configuration echo and before any request (the relay sees none). `--duration 7x`: usage
    status naming the flag. The units `s`, `m`, `h`, `d` each parse to the right number of
    seconds, shown in the echo, checked with `--workers 7` so nothing is sent (`1d` is 86400,
    `2h` is 7200). `--scenario endurance`: usage status naming the flag and the scenarios that
    exist (`smoke`), no connection. `crates/validate/tests/cli_surface.rs` passes
    **unmodified**: its `--workers 7` (`cli_surface.rs:30`) now ends INCONCLUSIVE before any
    request, keeping those tests off the network.
- **Falsifiability:** RED is reachable on C4-verify itself, not only by code-reading. The test
  file uses only the binary, the existing `run`, test-local relays and the gateway, so it
  compiles on this slice's base (the integration line plus 743.1 and 743.2). With production
  reverted, the base binary exits 3 for every run, so (A)'s exit-0 and every FAIL-status
  assertion go red: a real red→green. Every fault is a relay on `127.0.0.1`; no Docker.
- **Invariant to restore:** the tool never removes, overwrites or changes a byte or a bucket it
  did not create in this run, and never reports a result its answers do not prove. **For keys,
  this holds on one stated assumption: the run prefix `<run-id>/` is used by this run alone
  while it runs** (revised 2026-10-22). The occupied-prefix check proves only that nothing was
  under the prefix when the run started; 0017 claims exactly that and nothing more
  (`0017:791-794`, `:802-805`). A writer that enters the prefix after the check cannot be
  detected: the client's PUT sends no conditional-create header
  (`crates/validate/src/s3.rs:236-247` on the line), and the gateway evaluates preconditions
  only on GET and HEAD (`crates/gateway-s3/src/lib.rs:2478`, `:2579`), so a run could
  overwrite that key and later delete it. `--run-id` is required and never defaulted
  (`crates/validate/src/args.rs:26-30`), so picking one no other client writes under is the
  operator's half of the contract. The docs paragraph states the assumption, and nothing in
  the summary claims more. **For buckets, it holds on a second stated assumption: no bucket by
  the probe name appears on the endpoint while the run runs** (revised 2026-10-23; the earlier
  text said such a bucket "cannot exist", which S3's naming rules do not prove for every
  target). On Wyrd a bucket exists when its `bucket:<name>` record does
  (`crates/server/src/lib.rs:746-758`), that record's key takes any name
  (`crates/core/src/metadata.rs:48-53`), and the gateway refuses a bucket name only for a `/`
  (`crates/gateway-s3/src/lib.rs:1887-1893`). So the forbidden name is the first layer, for a
  conforming endpoint, and the absence check is the second: before any write the run confirms
  that a listing of the probe name answers `404 NoSuchBucket` (as Wyrd answers for a name with
  no record, `crates/gateway-s3/src/lib.rs:935-944`) or `400 InvalidBucketName`, and sends no
  bucket probe otherwise. What the check cannot see is a bucket by that name created after
  it; the docs paragraph states that assumption beside the prefix one. Sources:
  0017 §15, "Ownership may under-approximate. It must never over-approximate." (`0017:831`),
  membership not name shape (`0017:811-816`), occupied prefix (`0017:791-794`); #754's invariant
  ("never removes a byte it did not write"); the vacuity rule (`0017:555-561`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 937, 955, 956, 744, 853
- **Conflicts with:**
- **Ordering note:** 743.1 (the matrix) and 743.2 (the verbs) are its inputs. **743.2 (#938) was
  split on 2026-10-23** into #955 (the reply-capture core, HEAD, ranged and conditional GET,
  both listings) and #956 (bulk delete, the unsupported-operation probes, the client's own
  endpoint resolution; builds on #955). `Depends on` names both children rather than #938: the
  driver does not re-point a dependent of a split parent at its children
  (`src/pdca_harness/flow.py:1615-1619`), so naming #938 would let this slice share a wave with
  them (human decision, #938 Plan session). The verbs' types are as #938's brief and its
  iteration-v3 salvage name them, now shipped across the two children. #744 gives the
  payloads it sends and is the last slice to edit `args.rs` and `lib.rs`; #853 gives the
  response reading. Both are COMPLETE, but their code is **not on `main`** (PRs #931 and #934
  are open). It is on the integration line of the run that split #743,
  `pdca-integration/main-r42c5348f203d` (tip `582da99`: #744 folded as `61e5341`, #853 as
  `582da99`, on `main` `4bac538`), and every `crates/validate/` citation here is to that line
  (re-checked 2026-10-22). Outside that run it is held until PRs #931 and #934 merge into
  `main`, or until 744 and 853 are named in the same `pdca flow` command
  (`src/pdca_harness/flow.py:755-760`). Do records its base commit in `build-notes.md`.
- **Scope:** the `smoke` scenario and its runner; mapping the client's results onto the
  matrix's observations; `--server-version` (optional, joining #744's optional flags) and
  reading the `Server` header; typing `--duration`, `--workers` and `--scenario` for smoke; the
  FAIL exit status; the first-writer ownership minimum (refuse an occupied run prefix; delete
  only what this run wrote and recorded, and forget a key only once its absence is proved;
  bucket probes that cannot touch an existing bucket or `--bucket`, sent only after the
  probe-bucket absence check); `lib.rs` and `main.rs`
  wiring and the module doc's "#743" sentence (`lib.rs:4-11`); the `Cargo.toml` description;
  the docs; the test file. / out of scope:
  - `--allow-non-empty`, forced cleanup-off, orphan sets, delete-time verification beyond this
    run's own record (#754); `--dry-run` (#755); worker pools (#749);
  - authentication vectors and #491's known-open handling (#745), although 0017 turns them on
    for smoke;
  - the cross-sweep oracle, event stream and verdict artifact (#746, #747, #764); MinIO (#751);
    other scenarios;
  - **every size class** (`0017:535` asks for it): smoke sends #744's payloads at a few small
    sizes (at most one chunk), carried from the 2026-10-20 brief. #744's classes reach the
    single-PUT limit (about 158 MiB at default tuning) and large objects are #761's;
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `deny*.toml`; a
    `[dev-dependencies]` entry the test needs that 743.2 did not add (the red leg reverts
    `Cargo.toml`, so the test would not compile).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/smoke_loopback.rs` (NEW)
- **Verification posture:** default: a flippable red→green on C4-verify (see Falsifiability).
  Do confirms it before handing off: revert production, keep the test, show it fails by
  assertion (exit 3 against an expected 0 or FAIL status), not by a compile error.
  **Demonstrated reds (required, in `build-notes.md`):** (1) bulk-deleted keys dropped from the
  run's record on the document alone → (D) acknowledged-not-done; (2) bucket probes naming a
  valid derived name → (D) bucket probes, both runs; (3) a `*` ETag accepted and reused → (D) malformed
  ETag; (4) the 2xx head dropped on a cut body → (B)(ii); (5) the `--scenario` check removed →
  (E); (6) a day computed as `24 + 60 * 60` seconds → (E); (7) `+git.unknown` accepted as a
  version → (C); (8) the occupied-prefix check removed → (D); (9) `--duration` ignored (one
  sweep, then PASS) → (E); (10) the whole-GET check comparing length only, not bytes →
  (B)(iii); (11) the single-object `DELETE` left out of the sweep (cleanup by bulk delete
  only) → (A); (12) the ranged-GET check comparing length only, not bytes → (B)(iv); (13) the
  probe-bucket absence check removed → (D) absence check.
- **Production reach:** the binary runs smoke for real; the test drives that binary. No Wyrd
  server code changes.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323-350`; set `server_version` on its
    `S3Config` (`crates/gateway-s3/src/lib.rs:111-116`); seed bucket markers as `seed_bucket`
    does, `crates/server/tests/s3_list_objects.rs:82-88`.
  - **Peer, relay:** `Downstream` and `relay_connection`,
    `crates/validate/tests/s3_client_roundtrip.rs:465-477`, `:699`.
  - **The `Server` header:** value built at `crates/gateway-s3/src/lib.rs:126-145`, sent at
    `:1697-1699`; default `wyrd/unknown` at `:121`.
  - **Exit statuses** `crates/validate/src/lib.rs:43-54` (0–3 taken: FAIL needs its own);
    `run` `:132-160`; flags `crates/validate/src/args.rs:31-50`, `parse` `:162`.
  - **Docs currency** (`AGENTS.md:154-157`, a new CLI flag): add a **new paragraph** after the
    `validate` paragraph, `docs/design/architecture/05-building-block-view.md:297`, covering the
    matrix and its version keying, `smoke`, the ownership rules (including the exclusive-use
    assumption on `<run-id>/` and the probe-bucket assumption with its absence check, see
    *Invariant*) and the exit statuses. Do not
    rewrite line 297 itself: it is one 8.9 KB line, and rewriting it costs about 20 KB of diff.
- **Prior-art check (triage cycles):** searched by path `crates/validate/`,
  `docs/design/architecture/05-building-block-view.md` across merged, open and closed-unmerged
  PRs (2026-10-22): merged #845, #849, #859, #860, #865, #886; open #931, #934; closed-unmerged:
  none. No scenario code exists. The rejected first attempt's `smoke.rs` is the source of the
  three sign-off defects named in (D).
- **Disposition hint:** new-feature

Plan-review response (2026-10-22): all three findings are addressed in place. (1) Ownership:
the reviewer is right that one emptiness check cannot keep out a writer that arrives later, and
this gateway cannot enforce a conditional create (PUT reads no preconditions, `lib.rs:2478`,
`:2579` are GET and HEAD only). The invariant now states the exclusive-use assumption on
`<run-id>/` that 0017's guard already rests on, and the docs must state it too. ⚑ Human: an
enforced guarantee needs conditional-create support in the gateway, which is outside this
slice; file an issue if the assumption is not acceptable. (2) New (B)(iii): a relay flips one
GET byte with status, length and headers intact; the run FAILs with `integrity`, stops, and
leaves the object in place. (3) This slice no longer edits the matrix at all; a #508 mismatch
is reported to the human, and (B)(ii) uses `GetBucketLocation`, which stays unsupported whether
or not #508 lands. The in-place `pre-0.1` rule itself is #937's 0017 §3 amendment
(`results/issue_743/split-proposal.md:207-210`, not marked decided there), so the human
confirms it at #937's sign-off.

Plan-review response (2026-10-23): all three findings are addressed in place. (1) Bucket
safety: the reviewer is right that "cannot exist" was not proved for this target. Wyrd holds a
bucket as a `bucket:<name>` record with any name and refuses a bucket name only for `/`. The
invariant now states a second assumption (no bucket by the probe name appears during the
run), and a new read-only absence check before any write enforces the rest: a listing of the
probe name must answer `404 NoSuchBucket` or `400 InvalidBucketName`, else INCONCLUSIVE with
no write sent (new (D) leg, red 13). ⚑ Human: this adds a guard to the decided bucket-probe
design without changing the forbidden-name choice; veto it at sign-off if unwanted. (2)
Required coverage: (A) now asserts from the relay's record that every required row's wire
form was sent and answered (single `DELETE` with its follow-up `HEAD`, ranged GET, both
conditional GETs, v1 and v2 listings, bulk delete with its `HEAD`s), red 11; new (B)(iv)
flips one byte of a 206 body and expects `integrity`, red 12. (3) Bucket fault leg: split
into one run per verb, each asserting the targeted verb reached the relay, its name, and that
the test's buckets survived; red 2 covers both runs.

## Design

- **Order of a run.** Parse and echo the configuration (the echo also shows the bucket-probe
  name and the parsed duration). Refuse what can be refused offline: `--workers` ≠ 1, an
  unknown `--scenario`, an unparseable `--duration` or `--server-version`, a `--bucket` equal to
  the probe name. Then one `ListObjectsV2` with `prefix=<run-id>/` and `max-keys=1`, which both
  checks the prefix is empty and reads the `Server` header. Then one `ListObjectsV2` of the
  probe name, `max-keys=1`: anything but `404 NoSuchBucket` or `400 InvalidBucketName` ends
  the run INCONCLUSIVE naming the probe bucket. Only then send any write.
- **The run's record.** A key is recorded before the request that could create it, and is
  forgotten only once a `HEAD` answers 404. Cleanup deletes from that record only, never from a
  listing (`0017:811-816`). On FAIL, everything still recorded is listed and left in place.
- **Bucket probes.** `CreateBucket` and `DeleteBucket` name a bucket S3's naming rules forbid,
  never `--bucket`, and are sent only after the absence check above. Reason for the forbidden
  name: no run can prove it owns a bucket from a 2xx (S3's us-east-1 answers 200 to
  `CreateBucket` on a bucket the caller already owns), so a valid name the run "created" could
  still be someone else's. A listing does show whether a name holds a bucket right now (Wyrd
  answers `NoSuchBucket` for a name with no record), which is why the absence check exists,
  but it cannot make a 2xx prove ownership. A conforming endpoint cannot hold a bucket by a
  forbidden name; the absence check covers one that does not conform. A server that
  implements the verb answers a 2xx or an error that differs from the declared row, and the
  run FAILs.
- **`HeadBucket`, `GetBucketLocation`, `ListBuckets`** are read-only and may name `--bucket`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
