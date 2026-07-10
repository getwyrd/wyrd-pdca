# Adversarial review — issue #470 (wyrd-fdb-oci-image)

Advisory only; nothing here gates. Grounded on `$PDCA_TARGET` @ `f23848d`.

## Refutations of the evidence / verdict

- **NEEDS-HUMAN — the red→green gate does not exercise the defect it claims to fix.**
  The brief's Defect (§1/§2) is that the old Dockerfile installed only
  `cmake protobuf-compiler libssl-dev pkg-config` and **no** FDB client headers /
  `libfdb_c` / `libclang`, so the `fdb` feature cannot build and the runtime cannot
  link. The fix *does* install them (`deploy/docker/wyrd/Dockerfile:52-56` build stage,
  `:80-82` runtime stage). **But the binding gate asserts none of it:**
  `dockerfile_is_multistage_nonroot_and_parameterized`
  (`xtask/tests/fdb_image.rs:424-468`) checks only FROM-count, `COPY --from=build`,
  `USER`-before-`ENTRYPOINT`, `ARG FEATURES`, `ARG FDB_VERSION` — the words
  `foundationdb-clients`, `libclang`, `clang`, `fdbcli`, `libfdb_c` appear **nowhere**
  in the test (verified: zero matches). Concrete regression that keeps the gate green:
  delete `Dockerfile:52-56` (the `*,fdb,*` build-stage install). `cargo test -p xtask
  --test fdb_image` still passes — the image is back to the exact pre-fix broken state
  the brief describes, and the gate cannot tell. The C4-verify red is therefore red only
  because the new files are *absent* pre-fix (`read()` panics), not because the fix's
  substance is tested. The substance is entirely deferred to the off-gate `docker build`
  in `fdb-image.yml`. A human must decide whether a shape-only gate is adequate sign-off
  for a defect that is about *package contents*.

- **NEEDS-HUMAN — the headline capability (empty external-client dir is load-bearing and
  connects) is unverified by any gate; this is exactly what sank iteration v1.** The
  Dockerfile bakes an *empty* `WYRD_FDB_EXTERNAL_CLIENT_DIR`
  (`deploy/docker/wyrd/Dockerfile:114`, `:189-206` in the diff comment) and asserts, in
  prose only, that empty-dir + env-set is "byte-safe" and the linked primary connects
  normally, while a populated-with-own-version dir would misreport `Unreachable`. The
  #441 path that consumes it is present in the target
  (`crates/metadata-fdb/src/lib.rs:1114-1123`, `ensure_network` →
  `NetworkOption::ExternalClientDirectory`), so it is no longer inert decoration — but
  whether an *empty* dir actually connects (leg c) and whether a 7.1.x cluster yields the
  bounded #441 guided error rather than an anonymous hang (leg d) is pure runtime
  behaviour that **no row in `check-gates.json` exercises** (both are "human at
  sign-off"). v1 was rejected for leaving precisely these legs unverified. I cannot
  reproduce them here (no live cluster; `build-notes.md` withheld from this lens), so
  this is **provisional** — the human must confirm legs (c)/(d) were actually run this
  iteration and passed, not merely re-declared deferred.

## Attempted and could not refute

- **Smoke grep `fdb-image.yml:81` `grep -Eq 'redb\|tikv\|fdb'`.** Suspected a bug: in ERE
  (`grep -E`) `\|` is a *literal* pipe, not alternation, so this matches the literal
  string `redb|tikv|fdb`. But the usage output at `crates/server/src/cli.rs:277-281`
  literally prints `--metadata-backend redb|tikv|fdb` with pipes, so the match succeeds.
  Correct, if coincidentally. Could not refute. (Off-gate CI leg regardless.)

- **Version-consistency parsing.** `cargo_fdb_major_minor`
  (`xtask/tests/fdb_image.rs:305-316`) uses the *first* `fdb-` substring in `Cargo.toml`;
  I checked the target and the only `fdb-` is line 108 (`fdb-7_3`) — lines 18/51
  (`metadata-fdb`) contain `fdb` but not `fdb-`, so no earlier false match. Dockerfile
  `ARG FDB_VERSION=7.3.77` == compose `foundationdb/foundationdb:7.3.77` == crate `7.3`;
  the exact/major-minor checks all hold. The planted-red test
  (`:561-600`) drives the *same* `check_fdb_version_consistency` with `7.1.99` and
  genuinely errors naming the version — not vacuous. Could not refute the consistency
  logic on this tree. (Note for the human, not a break today: the first-match parse is
  fragile — a future `Cargo.toml` line introducing an earlier `fdb-` token would silently
  misread the version while the gate stays green.)

- **Workflow build↔run coupling** (`xtask/tests/fdb_image.rs:493-557`): `docker_builds`
  and `docker_run_images` correctly resolve `wyrd:fdb` as both built (`-t wyrd:fdb`) and
  run; the `--rm` / `--entrypoint fdbcli` flag skipping matches this workflow's exact
  token surface. Could not refute for this diff.
