# Brief — issue 440 / server-fdb-backend-selection

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** server-fdb-backend-selection
- **Defect:** `MetadataBackend` (`crates/server/src/cli.rs:88`) offers only `Redb` and
  `Tikv`. #438 landed the FoundationDB driver (`crates/metadata-fdb`, ADR-0042, merged as
  PR #492), but no `server`-side selection arm reaches it — the driver's own constructor
  says so: *"no `server`-side selection arm exists yet (that is a later, blocked issue)"*
  (`crates/metadata-fdb/src/lib.rs:895-896`). An operator therefore cannot run the shipped
  FDB backend by configuration; `--metadata-backend fdb` is rejected as an unknown value.
- **Success criterion:** In the **default build** (no `fdb` feature, no `libfdb_c`, no
  cluster), `cargo test -p wyrd-server --test fdb_backend_selection` passes, asserting all
  three of:
  1. **[gate with `#[cfg(not(feature = "fdb"))]`]** `MetadataBackend::from_config(Some("fdb"))`
     is an `Err` **whose message contains** ``requires building `wyrd` with `--features fdb` ``
     (mirroring the TiKV text at `cli.rs:110`);
  2. `MetadataBackend::from_config(Some("nonsense"))` is an `Err` **whose message contains
     the substring `fdb`** (the unknown-value text must list all three backends). Note the
     probe value is `nonsense`, not `fdb`, precisely so the echoed value cannot satisfy the
     substring check;
  3. running `env!("CARGO_BIN_EXE_wyrd")` with no arguments emits usage on **stderr**
     containing `redb|tikv|fdb`.

  Plus `cargo xtask ci` (the gating `C4-ci`) stays green.

  **Assertion 1 MUST be `#[cfg(not(feature = "fdb"))]`-gated**, with the positive
  `assert_eq!(from_config(Some("fdb")).unwrap(), MetadataBackend::Fdb)` under
  `#[cfg(feature = "fdb")]` — exactly the etcd pattern at `backend_selection.rs:47-56`.
  Ungated, assertion 1 would *fail* under the `--features fdb` run this brief requires
  below, because `from_config(Some("fdb"))` correctly returns `Ok(Fdb)` there.

  **⚠ The RED is message-content, not `is_err()`.** Pre-fix, `from_config(Some("fdb"))`
  *already* returns `Err("unknown metadata backend `fdb` (expected `redb` or `tikv`)")`
  (`cli.rs:112-115`, text at `:113`). A bare `assert!(…is_err())` is **green before the
  fix** and proves nothing. Every assertion above must bind on message content or on the
  binary's stderr.
- **Falsifiability:** All three binding assertions go RED on the plain
  `$PDCA_WORKTREE` / `../wyrd-verify` checkout with the stock toolchain — no FDB, no
  Docker, no feature flag. Under `C4-verify` (production change reverted, test kept):
  (1) and (2) fail on message content — pre-fix the message is `(expected `redb` or
  `tikv`)`, which contains neither the feature-hint text nor `fdb`; (3) fails because
  `usage()` (`cli.rs:234-240`) prints `redb|tikv`. The added test file survives the RED
  phase (it is an `ADDED_TEST`, kept while `cli.rs` and `Cargo.toml` are reverted) and
  still compiles, because every reference to `MetadataBackend::Fdb` sits behind
  `#[cfg(feature = "fdb")]`, which evaluates false once the reverted manifest no longer
  declares the feature. (That reversion makes `feature = "fdb"` an undeclared cfg;
  `unexpected_cfgs` is `level = "warn"` in the root `[workspace.lints.rust]`, so it does
  not harden into an error and the RED still comes from the assertions.)
  **Verified:** `cargo check -p wyrd-metadata-fdb
  --features fdb` compiles clean on this host (`/usr/lib/libfdb_c.so`,
  `/usr/include/foundationdb/`, `fdbcli`, Docker all present), so the supplementary
  feature-on evidence below is reachable too — no Plan-blocking gap.
- **Invariant to restore:** *Selecting a metadata backend is passing a different concrete
  behind the unchanged `MetadataStore` seam — never a refactor of any consumer.* Adding a
  backend must not change `alloc_inode`, `local_store_put`, `serve_s3`, the custodian
  reconciliation loop, or any generic helper: it adds arms at the composition root only
  (`server` is the one crate that knows concretes). Source: the `MetadataBackend` doc
  comment itself — *"selecting a backend is 'pass a different concrete', not a refactor of
  any consumer (ADR-0008/0016)"*, `cli.rs:82-86` (authoritative, internal); **ADR-0010**
  (composition root) as cited in `crates/server/Cargo.toml` `[dependencies]`;
  **ADR-0042** *"selection is not deployment"*
  (`docs/design/adr/0042-production-metadata-backend-reevaluation.md:463`).
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`; #438 landed
  as PR #492. INTEGRATION §2's M4 integration-branch caveat no longer applies — target
  `main` directly.)
- **Ordering note:** #438 (the driver) is **merged**, so no `Depends on:` is set. #439
  (dev/CI harness) is **open** and owns the compose cluster + doctor rows + CI conformance
  workflow; this bundle deliberately does **not** depend on it — see `Verification
  posture`. Sequencing per the issue ("Depends on #438 … pairs with #439") is satisfied.
- **Surfaces:** data
- **Difficulty:** medium
  (One file dominates and Rust's exhaustiveness check catches a missed arm, but the reach
  is 8 store-construction sites + a combinatorial cfg-gated tuple match + a manifest
  change — more than a reviewer holds in one glance.)
- **Scope:** Extend metadata-backend selection with an `Fdb` variant, compiled only under a
  new OFF-by-default `fdb` feature on `wyrd-server`, reaching the already-shipped
  `wyrd_metadata_fdb::FdbMetadataStore`. Concretely: the `MetadataBackend` enum and
  `from_config`; **every** store-construction dispatch site; the four usage strings; the
  `wyrd-server` `[features]`/`[dependencies]` manifest; and the regression test.
  **Cluster-file semantics (resolved here — do not re-decide):** the FDB driver
  *deliberately* falls back to `/etc/foundationdb/fdb.cluster` when `WYRD_FDB_CLUSTER_FILE`
  is unset — documented at `metadata-fdb/src/lib.rs:424-433` and unit-tested
  (`an_absent_or_blank_value_falls_back_to_the_package_default`, `:479-482`). So the fdb
  arm MUST call `FdbMetadataStore::connect()` and let it own env resolution + fallback, and
  surface `fdb backend: set WYRD_FDB_CLUSTER_FILE to the cluster file path` as **error
  context when the connect fails**. Do NOT copy TiKV's error-when-env-unset shape
  (`cli.rs:135-137`): despite the issue text's "parallel to the `WYRD_TIKV_PD_ENDPOINTS`
  hint", a hard pre-check would contradict the driver's contract and break a stock FDB
  install.
  **`connect()` is synchronous** (`pub fn connect() -> Result<Self>`,
  `metadata-fdb/src/lib.rs:898`), unlike `async fn open_tikv_meta()`. Do not blindly copy
  the `.await`.
  **`Cargo.lock` IS in scope:** optional deps are recorded in the lockfile (`wyrd-server`'s
  `dependencies` list already contains the optional `wyrd-coordination-etcd`), so adding
  `wyrd-metadata-fdb` changes it. `patch.diff` MUST carry that hunk — `cargo xtask ci` does
  not pass `--locked` (`xtask/src/main.rs`, `fn cargo`), so a missing hunk regenerates
  silently and ships an incomplete PR. No new third-party crates enter the graph;
  `foundationdb` et al. are already locked and `deny.toml`-allowed by #438.
  / **out of scope:** any change to `crates/metadata-fdb`; the `redb` default and the
  `tikv` selection path (byte-for-byte unchanged); `cmd_demo` (`cli.rs:964-969` — no
  backend selection by design); `alloc_inode`'s budgeted retry loop (`ALLOC_INODE_BUDGET`,
  `cli.rs:75-80`) — **verified safe unchanged**: it attaches `require`/`require_absent` on
  every attempt (`cli.rs:1035-1040`), so its batches are always *conditional*, which is
  exactly the case #438's driver maps to `1020 not_committed → Ok(Conflict)`. (FDB's blind
  batches can never yield `Conflict`; `alloc_inode` never issues one, so the loop's
  `CommitOutcome::Conflict` contract holds. Do not "fix" it.) Also out of scope: the
  docker-compose cluster, doctor rows, and CI conformance workflow (**#439**); an `fdb` row
  in xtask's `feature_gated_checks()` (**follow-up** — see `Verification posture`).
- **Repro instruction:** On `origin/main` in the target checkout:
  `cargo run -p wyrd-server --bin wyrd -- put /etc/hostname --key k --metadata-backend fdb`
  → fails with ``wyrd: unknown metadata backend `fdb` (expected `redb` or `tikv`)``.
  `cargo run -p wyrd-server --bin wyrd` (no args) → usage lists `[--metadata-backend
  redb|tikv]`, with no `fdb`.
- **External dependencies:** **none** for the binding success criterion and for `C4-ci` —
  the default build compiles no FDB code and links no `libfdb_c`, exactly as the `tikv`
  feature is arranged (`crates/server/Cargo.toml` `default = []`).
  For the *supplementary* evidence below (already confirmed present on this host, so
  neither may be silently skipped): `libfdb_c` ≥ 7.3 (`/usr/lib/libfdb_c.so`) + headers
  (`/usr/include/foundationdb/`) to link `--features fdb`; Docker +
  `deploy/fdb-single-node/docker-compose.yml` + `fdbcli` for the live demo flow. If any of
  these turns out to be unavailable, **declare it** — do not substitute a code-read.
- **Test file:** `crates/server/tests/fdb_backend_selection.rs` — a **NEW file the patch
  ADDS**. This is load-bearing and deviates from the issue text ("add the FDB case to
  `crates/server/tests/backend_selection.rs`") **deliberately**: `C4-verify` discriminates
  on files the patch *adds* (`_added_files` matches only `--- /dev/null` hunks,
  `engine/scripts/run-verify.sh:68`; `ADDED_TESTS`, `:195`). If the regression merely
  **modifies** the existing `backend_selection.rs`, `ADDED_TESTS` is empty, `:244` fires,
  and the gate degrades to **`PASS (green-only)`** — no RED is ever demonstrated, and
  `TEST_ARGS` falls back to the whole `-p wyrd-server` suite (`:212-218`). Worse, in the
  RED phase a *modified* test file is reverted like any other non-added file (`:264`), so
  edits there would vanish exactly when they are needed.
  Therefore: put **all three binding assertions** in the new file and **do not touch**
  `crates/server/tests/backend_selection.rs`. The issue's intent (backend-selection
  integration coverage) is satisfied; only the file boundary moves.
- **Verification posture:** The three binding assertions are a normal flippable
  regression — red pre-fix, green post-fix at Check, default features. **One additional
  assertion is deferred off-Check:** `MetadataBackend::from_config(Some("fdb")) ==
  MetadataBackend::Fdb`, which must be `#[cfg(feature = "fdb")]`-gated exactly as the etcd
  assertion at `backend_selection.rs:52-56`, because `C4-ci` builds default features only.
  It is *not unbuilt*: `from_config` is a pure function needing no cluster, and the whole
  feature-on tree compiles on this host. Do MUST therefore run, and paste the verbatim
  output into `build-notes.md`:
  - `cargo check -p wyrd-server --features fdb --tests` — proves the new `#[cfg(feature =
    "fdb")]` arms at all 8 dispatch sites actually compile. **This is load-bearing:**
    `cargo xtask ci` builds default features, and `feature_gated_checks()`
    (`xtask/src/main.rs`) checks only `wyrd-metadata-tikv` — so *no gate compiles these
    arms*, exactly as no gate compiles today's tikv arms. Absent this, a broken arm ships
    green.
  - `cargo test -p wyrd-server --features fdb --test fdb_backend_selection` — runs the
    gated `Fdb`-selected assertion offline (no cluster: `from_config` never connects).
    This is why assertion 1 must be `#[cfg(not(feature = "fdb"))]`-gated.
  - **The live round-trip — MUST run and record, gates nothing.** The issue's AC #1
    ("runs the demo flow against the compose cluster") *is* reachable today:
    `deploy/fdb-single-node/docker-compose.yml` already landed with #438, and Docker +
    `fdbcli` + `libfdb_c` are present. So Do MUST boot it and drive a real
    `wyrd put` → `wyrd get` round-trip with `--metadata-backend fdb` and
    `WYRD_FDB_CLUSTER_FILE` set, pasting the transcript into `build-notes.md`. Reuse the
    compose / `configure new single memory` / cluster-file recipe from
    `run_fdb_conformance` (`xtask/src/main.rs:292`). This is **not** in the binding
    criterion — `C4-verify` cannot boot Docker, and ADR-0042 holds that *"selection is not
    deployment"* (`:463`) — but it is the only check that exercises `open_fdb_meta` against
    a real cluster, so **it may not be silently skipped**. If the cluster cannot be brought
    up, say so explicitly (a Check §6 item); do not substitute a code-read.
    **"Demo flow" means a `put`/`get` round-trip, NOT `wyrd demo`:** `cmd_demo()`
    (`cli.rs:964-969`) takes no arguments and hardcodes `RedbMetadataStore::in_memory()`,
    so it has no backend selection and is explicitly out of scope.
- **Production reach:** Not a seam-ahead-of-consumer slice — the production `wyrd` binary
  built `--features fdb` traverses the new arms to the real `FdbMetadataStore`. The live
  end-to-end demo against a running cluster is nonetheless deferred to #439 (above), so at
  Check the fdb arms are proven **compiled and selected**, not **exercised against a
  cluster**. That is the intended, pre-declared boundary, matching the tikv precedent.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change.
  This is a **composition slice** — mirror the peer `tikv` composition exactly:
  - the `Fdb` enum variant + `from_config` arms mirror `MetadataBackend::Tikv`,
    `cli.rs:95-96` and `cli.rs:104-116` (note the `#[cfg(feature)]` / `#[cfg(not(feature))]`
    pair at `:106-111` that yields the friendly build-error);
  - `open_fdb_meta()` mirrors `open_tikv_meta()`, `cli.rs:131-141` — **but synchronous**,
    wrapping `wyrd_metadata_fdb::FdbMetadataStore::connect()`
    (`crates/metadata-fdb/src/lib.rs:898`), not an env pre-check;
  - the **8** store-construction dispatch sites to extend: `cli.rs:336` (`cmd_put`),
    `:417` (`cmd_get`), `:666`, `:783` (`run_reconstruction_over_backend`), `:1476`,
    `:1519`, plus the two tuple arms in `serve_s3_dispatch`, `cli.rs:1396-1422` (2×2 → 3×2;
    `(Fdb, Mem)` under `#[cfg(feature = "fdb")]`, `(Fdb, Etcd)` under
    `#[cfg(all(feature = "fdb", feature = "etcd"))]`, per the tikv/etcd arms at `:1403`
    and `:1416`);
  - the four usage strings: `cli.rs:236, 237, 239, 240`;
  - the manifest: `crates/server/Cargo.toml` — add `wyrd-metadata-fdb = { workspace = true,
    optional = true }` and `fdb = ["dep:wyrd-metadata-fdb", "wyrd-metadata-fdb/fdb"]`,
    mirroring the `tikv` feature line and its comment block. The workspace dep already
    exists at root `Cargo.toml:51`;
  - the test: the gated-assertion shape at `backend_selection.rs:44-56` (etcd) — copy the
    `#[cfg(not(feature))]` / `#[cfg(feature)]` pairing into the NEW file — and the
    binary-invocation idiom `env!("CARGO_BIN_EXE_wyrd")` + stderr assertion at
    `cli_roundtrip.rs:9-14, 133`;
  - `Cargo.lock`: the `wyrd-server` `dependencies` list, mirroring the optional
    `wyrd-coordination-etcd` entry already there.
- **Prior-art check (triage cycles):** Searched by affected file path.
  `crates/server/src/cli.rs` merged history: `c1f6e58` (redb/tikv selection, #255),
  `5a54643` (alloc_inode budget), `20d06f4` (#449 etcd), `cf45b08` (#459), `fdd34f1`
  (#487) — none adds an fdb arm. `crates/server/tests/backend_selection.rs`: three
  commits, same set. `crates/metadata-fdb/**`: PR #492 (merged) added the driver, xtask
  `fdb-conformance`, `deploy/fdb-single-node/docker-compose.yml`, `deny.toml` rows — and
  touched **no** `crates/server` file. **Open PRs: none** (`gh pr list --state open` → `[]`).
  Closed/rejected: no prior or abandoned attempt at server-side fdb selection. No
  superseding-ADR concern: ADR-0042 is Accepted and this implements it; no ADR is edited.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on evidence integrity, not on the code. The patch itself is believed correct and should be preserved substantially as-is — this is a rebuild of the *verification record*, not a redesign. Do not re-litigate the composition-root approach, the sync `open_fdb_meta`, or the cluster-file semantics; the brief resolved those and the reviewer passed C3/C5/T1/T2. What to change, in priority order: 1. `build-notes.md` §4 is a FABRICATED transcript presented as "pasted verbatim". The same `/tmp/input.txt` is shown as `bytes=38` on one put and `bytes=17` on another; `bytes` is `data.len()` (`cli.rs:377-378`), so the same file cannot be two sizes. The recorded `inode=4` is also wrong. Verified at sign-off by running the live round-trip: the true output is `inode=2 ... bytes=38`. The round-trip DOES work — every claim in §4 was independently reproduced (put/get byte-identical, conflict `already exists`, not-found, default build rejecting `fdb` with the feature hint) — so the conclusion was sound and the presentation was not. Re-run the flow and paste the ACTUAL terminal output. A transcript labelled verbatim must be verbatim; if a step is reconstructed from memory or from the code, label it as such or omit it. 2. The C4 RED is a compile error, not an assertion failure. Reverting the manifest makes `#[cfg(feature = "fdb")]` an undeclared cfg, and workspace `warnings = "deny"` (`Cargo.toml:195`) promotes `unexpected_cfgs` to a hard error despite its explicit `level = "warn"` — rustc says so directly: `-D unexpected-cfgs implied by -D warnings`. The brief's Falsifiability paragraph (`brief.md:47-49`) asserts the opposite and is simply wrong; the builder caught this and disclosed it honestly, which is to its credit. `run-verify.sh` accepts any non-zero exit as RED (`if run_test; then FAIL`), so the gate passed on a RED that proves "the crate does not build" rather than "the test catches the bug". The discrimination IS real — verified at sign-off by declaring `fdb` a known cfg (`RUSTFLAGS=--check-cfg=cfg(feature,values("fdb"))`, no lint disabled, no code changed) in the RED state: `0 passed; 3 failed`, all three failing on message content exactly as the brief predicted. Rebuild so the harness can SHOW that: the RED transcript must exhibit three assertion failures, not a compile error. Declaring the `fdb` feature value in the workspace `check-cfg` list is the obvious route; any route is acceptable that leaves the RED assertion-driven under the unmodified `run-verify.sh`. 3. `protoc` IS installed (`/usr/bin/protoc`, libprotoc 3.21.12). The builder's claim that it could not compile-verify the `(Fdb, Etcd)` arm at `cli.rs:1487-1492` does not hold on this host. Verified at sign-off: `cargo check -p wyrd-server --features fdb,etcd --tests` is clean and really builds `wyrd-coordination-etcd`, so that arm really compiles. Run it and record it. Before declaring an external dependency unavailable, check it with `command -v` and paste the result. 4. The builder's own `NEEDS-HUMAN external dependency: protoc` note lived only in `build-notes.md` and never reached `SUMMARY.md` §6, because the reviewer is decorrelated from `build-notes.md` by design and nothing else propagates it. Any NEEDS-HUMAN the builder declares must surface where the human actually clears items. Standing instruction for the rebuild: evidence is the deliverable here, not the diff. A claim that cannot be reproduced from the pasted commands is worse than an admitted gap — an admitted gap gets checked at sign-off, an unadmitted one ships.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
