# Brief — issue 470 / wyrd-fdb-oci-image

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** wyrd-fdb-oci-image
- **Defect:** Wyrd has **no first-class container image**. Verified against `origin/main`
  (`b1ccca3`): the only Dockerfile that builds the `wyrd` binary is
  `crates/chunkstore-grpc/tests/dserver/Dockerfile` — a *test fixture*, inside a crate's
  `tests/` directory — reused ad hoc by the production-shaped single-zone stack via a
  build-arg (`deploy/small-multi-node/docker-compose.yml:232-237`, `FEATURES: "tikv,etcd"`,
  tagged `wyrd-single-zone:local`). Two consequences:
  1. That Dockerfile **cannot build the `fdb` feature at all.** Its build stage installs
     `cmake protobuf-compiler libssl-dev pkg-config` when `FEATURES` is non-empty
     (`:27-31`) and then runs `cargo build --release --locked --bin wyrd --features …`
     (`:35`). `--features fdb` pulls `foundationdb-sys`, whose build script runs `bindgen`
     against **`libfdb_c` headers** and `dlopen`s `libclang` (the reason `deny.toml:38-45`
     had to allowlist ISC for `libloading`). Neither is installed. The build fails.
  2. Its runtime stage (`:38` `debian:bookworm-slim`) carries no `libfdb_c` and no
     `fdbcli`, so even a successfully-built binary could not link at load time, and the
     doctor row (#439) would have nothing to probe.

  ADR-0042 chose FoundationDB as the **production** metadata backend. The image an operator
  would actually run does not exist.
- **Success criterion:** `cargo test -p xtask --test fdb_image` passes on the plain
  worktree (**no Docker needed, no image built, stock toolchain**), asserting all four of:
  1. `deploy/docker/wyrd/Dockerfile` exists, is multi-stage, declares a non-root `USER`
     before its `ENTRYPOINT`, and takes an `ARG FEATURES` and an `ARG FDB_VERSION`.
  2. **Single-source version consistency** — the `FDB_VERSION` pinned in that Dockerfile
     equals *both* the image tag in `deploy/fdb-single-node/docker-compose.yml`
     (`foundationdb/foundationdb:7.3.77`) *and* the line implied by the crate pin
     `foundationdb = { version = "0.10", default-features = false, features = ["fdb-7_3"] }`
     (`Cargo.toml:108`). Do must assert the *major.minor* line agrees across all three and
     the exact patch agrees between the Dockerfile and the compose tag. This is the
     load-bearing assertion: version coupling is the whole point of the image, and a silent
     drift between the baked client and the deployed cluster is the failure mode #441
     exists to diagnose.
  3. `.github/workflows/fdb-image.yml` exists; every `cargo xtask <sub>` or `docker`
     invocation it names resolves (the `xtask/tests/readme_dev_section.rs` doc↔dispatch
     technique), and its pull-request path filter includes `crates/metadata-fdb/**` and
     `deploy/docker/wyrd/**`.
  4. A **demonstrated red**: a temp-fixture Dockerfile with a mismatched `FDB_VERSION` makes
     the consistency check in (2) fail — the check is load-bearing, not resting on
     file-non-existence. (Planted-red pattern:
     `xtask/tests/deploy_no_orchestrator_coupling.rs:67`.)

  Plus `cargo xtask ci` (the gating `C4-ci`) stays green.
- **Falsifiability:** Assertions 1–3 go RED on the plain `$PDCA_WORKTREE` / `../wyrd-verify`
  checkout because neither file exists (`git -C ../wyrd ls-tree -r --name-only origin/main |
  grep -i dockerfile` → only `crates/chunkstore-grpc/tests/dserver/Dockerfile` and
  `deploy/tikv-multi-replica/iptables-agent/Dockerfile`). Assertion 4 is a *live* red on a
  planted fixture, so the version check is proven load-bearing rather than vacuous. No
  Docker and no network are required for the binding criterion — deliberately, because
  `cargo xtask ci` is container-free by policy (ADR-0016).
  **Supplementary live evidence, runnable here:** this host has Docker (`docker info` OK),
  `libfdb_c` **7.3.77**, `fdbcli` **7.3.77**, and network egress (verified: `gh` reaches
  GitHub). Do MUST attempt, and record in `build-notes.md`:
  (a) `docker build --build-arg FEATURES=fdb,etcd -f deploy/docker/wyrd/Dockerfile -t wyrd:fdb .`
  (b) `docker run --rm wyrd:fdb` (no args) prints usage containing `redb|tikv|fdb`
  (`crates/server/src/cli.rs:268-269` on `main` after #440 merged). **Expect a non-zero exit** —
  `wyrd` with no subcommand prints usage and fails; assert on captured **stderr**, not on the
  container's exit status,
  (c) against the `deploy/fdb-single-node/` cluster, `wyrd --metadata-backend fdb` connects,
  (d) **the mismatched-version check**: point the image at a `foundationdb/foundationdb:7.1.x`
  cluster and confirm #441's guided `protocol`-naming error appears within a bounded
  deadline — *not* an anonymous timeout and *not* an indefinite hang. If any of (a)–(d)
  cannot run, Do MUST declare it rather than substituting a code-read.
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`; branch
  deleted. Prereq #438 merged as PR #492.)
- **Depends on:** 441
- **Ordering note:** 470 depends on 441 because the image is where #441's deliverables
  become real: (i) the multi-version external-client directory this image bakes is loaded
  by `ensure_network()`'s `NetworkOption::ExternalClientDirectory` support, which **441
  adds** (today `crates/metadata-fdb/src/lib.rs:868` calls `foundationdb::boot()`, which
  accepts no network options — so without 441 the directory would be inert decoration);
  and (ii) #470's own body states *"the image is where #441 item 4's version-skew guard is
  verified"*, and #441's acceptance criterion 1 ("CI builds an image where `wyrd
  --metadata-backend fdb` connects") is explicitly *"discharged here"*. The wave fold gives
  470 the accepted 441 diff without waiting for a human merge, so no `Onto branch` is
  needed. 470 is deliberately **not** made to depend on 439: 439's conformance workflow runs
  `cargo xtask fdb-conformance` against a host-installed client, not against this image, so
  the two workflows are independent files and can build in the same run.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Promote the ad-hoc test-Dockerfile build to a first-class, version-pinned OCI
  image for the FoundationDB backend, and make the baked client version mechanically
  consistent with the cluster line the repo deploys. Concretely: a parameterized multi-stage
  Dockerfile at `deploy/docker/wyrd/Dockerfile` (`ARG FEATURES`, `ARG FDB_VERSION`) that,
  when `FEATURES` contains `fdb`, installs the pinned `foundationdb-clients` package in the
  **build** stage (headers + `libfdb_c` + `libclang` for `bindgen`) and in the **runtime**
  stage (`libfdb_c` + `fdbcli` for the doctor and for operators); an external-client
  directory baked into the image layout so a lockstep cluster upgrade is an image rebuild
  with two client libraries; OCI labels recording the `libfdb_c`/`fdbcli` version; a
  non-root runtime user; a CI job that builds it on PRs touching the FDB surface; and the
  `xtask/tests/fdb_image.rs` consistency guard above.

  / **out of scope:**
  - **`wyrd:tikv` (#471, still open).** Item 5 asks to *coordinate* the shared build
    skeleton, not to build the tikv flavor. Do lands `deploy/docker/wyrd/Dockerfile`
    **parameterized** so #471 can adopt it by passing `FEATURES=tikv,etcd`, and leaves
    `deploy/small-multi-node/docker-compose.yml:232-237` pointing at the existing test
    Dockerfile. Do MUST NOT migrate the TiKV stack — that is #471's slice, and moving it
    here would collide with #469's rename.
  - **`deploy/README.md` — do not touch it.** Documenting the new `deploy/docker/` image
    home there is the obvious instinct and it is **469**'s, not this bundle's: 469 rewrites
    that file into the full profile matrix. 469 `Depends on` this bundle, so it lands in a
    LATER wave and builds on the fold containing this diff — if this bundle also edited
    `deploy/README.md`, 469's hunks would carry context created here, and `C4-verify` (which
    resets to `origin/main`, not the folded tip — `run-verify.sh:_resolve_base_ref`,
    `:109-113`) would reject 469's patch as stale (`:234`). Cite the file freely; write
    nothing to it. The same rule is why this bundle keeps its test helpers local and touches
    no `xtask/src/`.
  - **Publishing to GHCR.** The issue permits deferring it; defer it. Local tag parity with
    the existing `wyrd-single-zone:local` convention (`deploy/small-multi-node/docker-compose.yml:237`)
    is the bar: tag `wyrd:fdb`.
  - **Consuming the image from the deploy profiles (#469)** and from #439's conformance
    workflow. #469 wires `small-multi-node-fdb` to this image in a later wave.
  - **The version-skew guard itself (#441).** This bundle *verifies* it; it does not
    implement it. If `preflight` is absent when Do runs, the wave fold has failed — stop and
    say so, do not reimplement it here.
  - Deleting or moving `crates/chunkstore-grpc/tests/dserver/Dockerfile`, which
    `cargo xtask integration` and the Tier-1 Jepsen leg still build from (`:47` installs the
    `iptables` the jepsen sidecar reuses).
- **Repro instruction:** On `origin/main` in the target checkout:
  `git -C ../wyrd ls-tree -r --name-only origin/main | grep -i dockerfile` → two files,
  neither a `wyrd` production image. Then
  `git -C ../wyrd show origin/main:crates/chunkstore-grpc/tests/dserver/Dockerfile | sed -n '21,35p'`
  → the `FEATURES` build-arg path installs `cmake protobuf-compiler libssl-dev pkg-config`
  and nothing FoundationDB-related, so `FEATURES=fdb,etcd` cannot build. Confirm directly:
  in a throwaway container without the FDB client package, `cargo build --features fdb -p
  wyrd-metadata-fdb` fails in `foundationdb-sys`'s build script.
- **External dependencies:**
  - *Binding criterion (`cargo test -p xtask --test fdb_image`):* **none** beyond the base
    Rust toolchain. The test reads files and compares strings; it must not shell out to
    `docker`. Keep the parsing helpers **pure and local to the test file** — see `Test file`.
  - **No new Cargo dependency, and no `xtask/src/` change at all.** `xtask/Cargo.toml` has no
    YAML or Dockerfile parser (`wyrd-chunk-format`, `serde`, `serde_json` only). Assert
    Dockerfile and workflow content with plain-text/substring checks, and keep the helpers
    **local to `xtask/tests/fdb_image.rs`** — exactly as `xtask/tests/readme_dev_section.rs`
    keeps `workspace_root()` / `read()` local and imports nothing from `xtask`'s lib. Adding a
    crate triggers the ADR-0003 §2 audit + `deny.toml` allowlist, a **human-only** decision
    (INTEGRATION §4). Keeping helpers test-local also avoids an `xtask/src/lib.rs` hunk, which
    **439** (wave 1) also edits — and `C4-verify` applies this patch to a worktree reset to
    `origin/main`, not to the wave-folded base (`engine/scripts/run-verify.sh:_resolve_base_ref`,
    `:109-113`), so a shared-file hunk would fail to apply (`run-verify.sh:234`, "the bundle is
    stale"). This bundle's file set is therefore disjoint from 439's and from 469's.
  - *Supplementary live legs (a)–(d):* Docker + compose plugin; **network egress** to
    `docker.io` (for `rust:1.96.0-bookworm`, `debian:bookworm-slim`,
    `foundationdb/foundationdb:7.3.77` **and `:7.1.x`**) and to
    `github.com/apple/foundationdb/releases` (the `foundationdb-clients` `.deb`). Expect a
    multi-GB, multi-minute cold build — `cargo build --release --features fdb,etcd` compiles
    `bindgen` and the etcd/tonic tree. Budget for it; do not silently downgrade to a
    `--features fdb`-only build to save time without saying so.
  - *Accepted-diff dependency:* #441's `preflight` module and
    `WYRD_FDB_EXTERNAL_CLIENT_DIR` support, arriving via the wave fold.
- **Test file:** `xtask/tests/fdb_image.rs` (new). **Its parsing helpers live IN THAT FILE.**
  Do MUST NOT add a module under `xtask/src/`, and MUST NOT touch `xtask/src/lib.rs`. Follow
  `xtask/tests/readme_dev_section.rs`, which defines `workspace_root()` / `read()` locally and
  imports nothing from `xtask`'s lib — the helpers here are file reads and substring compares,
  not shipped logic, so the `xtask::deploy_guard` "pure helper in the lib" precedent does not
  apply. This is load-bearing for scheduling, not style: **439** (wave 1) writes
  `xtask/src/lib.rs`; an `xtask/src/` hunk here would make this bundle's write-set overlap
  439's across waves, and `C4-verify` — which resets to `origin/main`, not the folded tip
  (`run-verify.sh:_resolve_base_ref`, `:109-113`) — would then reject one of the two patches as
  stale (`:234`). Keeping the helpers test-local is what keeps the batch conflict-free.
- **Verification posture:** Mixed, declared here so it lands as a pre-declared sign-off item.
  - **Flippable at Check (the binding criterion):** the four assertions above — file shape,
    cross-file version consistency, workflow↔dispatch consistency, and a *live planted red*.
    No Docker, no network.
  - **Deferred (off-Check), with named confirmers:** (i) the `docker build` itself and the
    end-to-end connect — confirmed by the new `.github/workflows/fdb-image.yml` on its first
    run and by Do's recorded local run; (ii) the **mismatched-version guided error**, which
    is #441's acceptance criterion 3 discharged here — confirmed by Do's local run (d) and
    by the maintainer at sign-off. `cargo xtask ci` is container-free by policy and will
    never build an image.
  - **Deferred ≠ unbuilt (the #146 forcing function):** what is BUILT AND EXERCISED at Check
    is (i) the whole Dockerfile, parsed and asserted, with its version pin cross-checked
    against two other files and proven load-bearing by a planted red; (ii) the workflow file,
    asserted against the real command surface; (iii) the pure parsing module, unit-tested.
    The deferred item is the *execution* of a build whose every input is pinned and checked.
    Docker IS available in this environment, so Do is expected to actually run legs (a)–(d)
    — a deferred posture here is a fallback, not a licence to skip them.
- **Production reach:** The image is the production artifact; nothing stands in for it. But
  note what the image does **not** yet prove: `deploy/README.md` records that
  `deploy/small-multi-node/`'s S3 gateways are standalone islands (#454) and there is **no
  closed write path** (#455). So "`wyrd --metadata-backend fdb` connects to the compose
  cluster" means *the store opens and answers*, not *an object round-trips through the
  cluster*. That gap is #455's, not this bundle's; stated so it is not mistaken for a
  regression at sign-off.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. This
  is a composition slice — Do MAY open these cited peer callsites to mirror the pattern:
  - `crates/chunkstore-grpc/tests/dserver/Dockerfile` (whole file, 65 lines) — the multi-stage
    shape, the pinned `rust:1.96.0-bookworm` matching `rust-toolchain.toml` (`:12-14`), the
    conditional `ARG FEATURES` install (`:21-31`), `--locked` (`:35`), and the **non-root
    precedent from #286** at `:56-64` (`groupadd --system --gid 10001` … `USER
    dserver:dserver` placed after `chown` and before `ENTRYPOINT`, with the comment
    explaining why the order matters). Reuse uid/gid 10001.
  - `deploy/small-multi-node/docker-compose.yml:232-237` — the build stanza and
    `wyrd-single-zone:local` tag convention this image parallels.
  - `deploy/fdb-single-node/docker-compose.yml` — the pinned `foundationdb/foundationdb:7.3.77`
    tag one side of the consistency assertion reads, and its header comment explaining the
    cluster-file/host-networking snag.
  - `Cargo.toml:108` — the `features = ["fdb-7_3"]` crate pin, the other side.
  - `xtask/tests/readme_dev_section.rs` (whole file) — the **helpers-local-to-the-test**
    precedent this bundle follows (`workspace_root()`, `read()`, substring assertions, no lib
    import); `xtask/tests/deploy_no_orchestrator_coupling.rs:67` — the planted-red shape.
    (`xtask/src/lib.rs:1-19` states the *other* pattern — a pure helper promoted to the lib so
    a test can unit-test it. It is cited to be **rejected** here: this bundle writes no
    `xtask/src/`.)
  - `.github/workflows/integration-nightly.yml` — container-job workflow shape (`docker
    info` step, `timeout-minutes`, failure-artifact upload).
- **Prior-art check (triage cycles):** Searched by affected file path across merged history,
  open PRs, and closed/rejected PRs.
  - `git -C ../wyrd ls-tree -r --name-only origin/main | grep -i dockerfile` → two files,
    neither under `deploy/docker/`. `deploy/docker/` **does not exist**. `.github/workflows/`
    history (`4b0c759`, `34bee68`, `91aa8ed`, `7780d70`) contains no image-build job.
  - `#471` (`OCI image wyrd:tikv — promote the ad-hoc test-Dockerfile build`) is **OPEN and
    unstarted** — so this slice lays the shared skeleton first, as item 5 asks; there is no
    prior skeleton to adopt.
  - Rejected work: one non-merged closed PR in the last 60 (#400, docs/proposal scope) —
    unrelated to these paths. `#286` (non-root runtime user) is the merged precedent cited
    above.
- **Disposition hint:** new-feature


## Deliberate deviation from the tracker — flagged, not silent

Issue #470's body says the image is consumed by *"the #439 conformance workflow and the #442
battery … rather than ad-hoc builds"*, and its Sequencing repeats *"Consumed by #469 …, #439's
workflow, and #442's battery."* **These briefs do not wire #439's workflow to the image.** The
reasons, so the human can overrule:

1. **It is technically unnecessary.** #439's workflow runs `cargo xtask fdb-conformance`, which
   drives `cargo test -p wyrd-metadata-fdb --features fdb` against a `foundationdb/foundationdb`
   container. That is a *test-harness* run, not a `wyrd` binary run. The `wyrd:fdb` image ships
   the `wyrd` binary, not the test suite — it has nothing the conformance job needs. The job also
   builds no ad-hoc image, so the tracker's stated harm ("rather than ad-hoc builds") does not
   arise. Issue #439's own body agrees: item 3 says the workflow "boots the FDB container".
2. **It would be unschedulable.** `.github/workflows/fdb-conformance.yml` is **439's** file, and
   439 lands in wave 1 — *before* 470 exists. Wiring it to the image means either 470 edits 439's
   file (a cross-wave shared file, and the `C4-verify` stale-patch hazard this batch was scoped to
   avoid) or 439 depends on 470, which would invert the natural order and serialise the batch.

**Where the requirement actually lands:** #469 (the deploy profiles) and #442 (the battery) both
consume `wyrd:fdb` genuinely — they run the `wyrd` binary. #470's acceptance criterion says only
*"CI builds the image; the FDB deploy profiles consume it"*, which these briefs satisfy. The
"#439's workflow consumes it" clause is left **unimplemented on purpose**; if the human wants it,
it is a follow-up slice against `.github/workflows/fdb-conformance.yml` once both have merged,
not a wave edge in this batch.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected because the bundle was built and verified against a base WITHOUT #441: its preflight module was absent from the wave fold (crates/metadata-fdb/src had only lib.rs; lib.rs:868 still called bare foundationdb::boot()). #441 has since merged as PR #495. What to change next: 1. Re-fold onto the base that now contains #441 (PR #495) and exercise the two legs that were blocked and left honestly unverified: (c) `wyrd --metadata-backend fdb` connects, and (d) the #441 version-skew GUIDED error against a 7.1.x cluster within a bounded deadline (not an anonymous hang). These prove the baked external-client directory / WYRD_FDB_EXTERNAL_CLIENT_DIR is actually LOADED, not the inert decoration the brief warns about — the headline capability of the image and #441 acceptance criterion 3 "discharged here." The binding container-free gate and legs (a) build / (b) usage smoke already pass and need no rework. 2. Correct the adversarial reviewer feedback: - .github/workflows/fdb-image.yml:78 — the failure-branch echo uses back-ticked `wyrd` inside a double-quoted string, so bash runs command substitution instead of printing the literal word (guard outcome intact, diagnostic corrupted). Quote it so the literal `wyrd` prints. - Harden the runtime `cp /usr/lib/libfdb_c.so` (Dockerfile:191) against a multiarch install path (/usr/lib/x86_64-linux-gnu/) so a deb that lands the lib elsewhere does not silently break `docker build` while the file-parse gate stays green. - The `cargo xtask <sub>` half of the workflow↔dispatch check (xtask/tests/fdb_image.rs) is vacuous — fdb-image.yml names no `cargo xtask` command, so that assertion passes on an empty set. Either drop it or make the check meaningful for this workflow's real command surface.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
