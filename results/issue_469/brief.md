# Brief — issue 469 / fdb-deploy-profiles

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** fdb-deploy-profiles
- **Defect:** TiKV has deploy recipes at all three ADR-0043 fixture tiers; FoundationDB —
  the *chosen production* metadata backend (ADR-0042) — has one. Verified against
  `origin/main` (`b1ccca3`), `git -C ../wyrd ls-tree --name-only origin/main deploy/`:
  `tikv-single-node/`, `tikv-multi-replica/` (with its `iptables-agent/` fault sidecar),
  `small-multi-node/`, `etcd-single-node/`, and `fdb-single-node/`. So:
  1. There is **no `deploy/fdb-multi-replica/`**. #442's fault battery — the go/no-go gate
     for making FDB the default — has no cluster to run against. It needs a ≥3-process FDB
     cluster plus a fault sidecar that can kill `fdbserver` mid-commit and partition
     client↔cluster.
  2. The canonical single-zone stack `deploy/small-multi-node/` is **TiKV-wired**: its
     services build with `FEATURES: "tikv,etcd"` and the custodians open
     `--metadata-backend tikv` (`deploy/small-multi-node/docker-compose.yml:232-237`;
     `deploy/README.md` §"Genuinely wired backends"). There is no FoundationDB peer, so the
     production track has never been stood up at the topology that is supposed to prove it.
  3. Neither stack is *named* as one of a pair, so `small-multi-node/` reads as "the" stack
     while the backend it wires is the retained fallback (#443), not the production track.
- **Success criterion:** `cargo test -p xtask --test fdb_deploy_profiles` (a NEW test file —
  see `Test file`) passes, split so the binding red does **not** depend on Docker:
  1. **Unconditional (pure filesystem, no Docker):** `deploy/fdb-multi-replica/docker-compose.yml`
     and `deploy/small-multi-node-fdb/docker-compose.yml` exist; the FDB single-zone stack's
     compose text names `--metadata-backend fdb`; `deploy/README.md` contains a profile
     matrix naming all six profiles, states **which small-multi-node setup is currently
     canonical**, and — because the rename is deferred — **explicitly records that
     `small-multi-node/` is the TiKV peer of `small-multi-node-fdb/`**, so the unqualified
     name is not read as "the" stack. RED pre-fix by non-existence.
  2. **Behind `docker_compose_available()`** (the existing convention at
     `xtask/tests/deploy_no_orchestrator_coupling.rs:133` — hard failure in CI, warn-and-skip
     locally): `docker compose config` parses `deploy/fdb-multi-replica/docker-compose.yml`
     and it declares **three `fdbserver` processes** plus the fault sidecar; and parses
     `deploy/small-multi-node-fdb/docker-compose.yml`, which declares the 3-node etcd
     ensemble, the FDB cluster, 9 D servers, the custodian role and the S3-gateway role —
     the same role-completeness assertion `small_multi_node_compose_config_is_structurally_valid`
     (`:148`) makes for the TiKV stack.
  3. `cargo test -p xtask --test deploy_no_orchestrator_coupling` still passes **with that
     file byte-unchanged** — its two existing signals (the ADR-0010 planted-red at `:67` /
     green-on-real-tree at `:102`, and the TiKV stack's compose-config check at `:148`, whose
     compose path at `:163` still resolves) keep gating. The rename is deferred, so nothing in
     that file moves.

  Plus `cargo xtask ci` (the gating `C4-ci`) stays green.
- **Falsifiability:** Assertion 1 is a pure-filesystem red on the plain `$PDCA_WORKTREE` /
  `../wyrd-verify` checkout — the two compose files do not exist. **This split is
  deliberate and load-bearing:** the peer test `small_multi_node_compose_config_is_structurally_valid`
  *skips cleanly* when Docker is absent (`:147-160`), so an assertion written only in that
  style could never be relied on to go RED at Check. Assertion 2 gives the stronger,
  Docker-backed red where Docker exists — and it does here (`docker info` OK).
  **Supplementary live evidence, runnable here:** `docker compose -f
  deploy/fdb-multi-replica/docker-compose.yml up -d` then `fdbcli --exec "status minimal"`
  reports the cluster healthy; the `iptables-agent` sidecar partitions and heals a named
  link. Do MUST run these and record the output.
  **What is NOT falsifiable here, stated plainly:** a full `small-multi-node-fdb` bring-up
  is 21 containers and needs the `wyrd:fdb` image built (a multi-GB `cargo build --release
  --features fdb,etcd`). It is declared deferred below with a named confirmer; it is **not**
  part of the binding criterion, and Do must not pretend to have run it.
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`; branch
  deleted. Prereqs #438 (PR #492) and #440 (PR #493, which added `--metadata-backend fdb` —
  `crates/server/src/cli.rs:120`, `:168`) are both merged, so the flag the FDB stack passes
  its services genuinely exists on the target branch. `deploy/` stays outside the Cargo
  workspace per ADR-0010.)
- **Depends on:** 470
- **Ordering note:** **Depends on 470** — `deploy/small-multi-node-fdb/` runs the wyrd roles
  from the `wyrd:fdb` image, and no Dockerfile on `main` can build `--features fdb` (see
  #470's brief: `crates/chunkstore-grpc/tests/dserver/Dockerfile:21-35` installs no
  FoundationDB client, so `foundationdb-sys`' `bindgen` build script cannot run). The wave
  fold delivers 470's accepted image before this bundle builds, so no `Onto branch` is needed.
  **No `Conflicts with` — and that is a decision, not an oversight.** The human deferred the
  rename (see "Recorded decisions" below), so this bundle's write-set is
  `deploy/fdb-multi-replica/**`, `deploy/small-multi-node-fdb/**`, `deploy/README.md`, and the
  new `xtask/tests/fdb_deploy_profiles.rs` — **disjoint from every other bundle in the batch.**
  It touches no `xtask/src/`, does not modify `xtask/tests/deploy_no_orchestrator_coupling.rs`,
  and does not touch the root `README.md`. So there is no shared file with 439, no fold
  conflict, and no cross-wave `C4-verify` stale-patch risk (`run-verify.sh:234`; upstream
  eduralph/pdca-harness#273). **Two scope decisions hold that property and must not be
  silently reversed by Do:** the deferred rename, and the dropped `xtask` bring-up arm. Taking
  either would add `xtask/src/main.rs` to this write-set, which 439 also writes, and would
  re-require `Conflicts with: 439`. Issue item 1 (`deploy/fdb-single-node/`) creates no
  ordering edge because it is **already landed**.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Bring FoundationDB to parity with TiKV across the ADR-0043 fixture tiers, and
  make the canonical single-zone stack an explicit, named **pair**. Three deliverables:
  (a) **`deploy/fdb-multi-replica/`** — a 3-process FoundationDB cluster in
  double-or-triple redundancy mode, on a bridge network with one netns per node, plus the
  fault-injection sidecar. Reuse `deploy/tikv-multi-replica/iptables-agent/` **as-is**: it
  is a generic `iptables` entrypoint image, not TiKV-specific (read its Dockerfile — the
  ENTRYPOINT is literally `["iptables"]`). This is the stack #442 drives.
  (b) **`deploy/small-multi-node-fdb/`** — the identical role topology to
  `deploy/small-multi-node/` (3 etcd + 9 D servers + 3 custodians + 3 S3 gateways) with the
  PD+TiKV tier replaced by the FDB cluster, wyrd services running `--metadata-backend fdb`,
  built from #470's `wyrd:fdb` image (`--features fdb,etcd`). **Two directories, not a
  compose override** — the metadata tier swap is a whole service-set change, and the
  existing per-backend convention already uses one directory per backend profile.
  (c) **`deploy/README.md`** documents the full profile matrix (single-node, multi-replica,
  small-multi-node × TiKV/FDB), which suite targets which profile (conformance → single-node;
  #442 battery → multi-replica; #367 first-deployment gate → small-multi-node), and **which
  small-multi-node setup is currently canonical**: TiKV until #442 records go, FDB after
  (#443's retained-fallback posture). Both stay runnable. Because the **rename is deferred**
  (Recorded decision 1), the README is where the pairing is made explicit: it must say that
  `small-multi-node/` IS the TiKV peer of `small-multi-node-fdb/`, so the unqualified name is
  not read as "the" stack. That sentence is the issue's "two clearly-named peer setups"
  requirement, discharged in prose rather than in path names.

  / **out of scope:**
  - **`deploy/fdb-single-node/` (issue item 1) — ALREADY LANDED.** #438 shipped it (commit
    `22d39b6`); `cargo xtask fdb-conformance` (`xtask/src/main.rs:292`) drives it and
    `deploy/README.md` must simply document it. Do MUST NOT re-create or restructure it —
    its host-networking + pinned `FDB_CLUSTER_FILE_CONTENTS` design is load-bearing for the
    host-side client, and its header comment explains why.
  - **The #442 fault battery itself.** This slice ships the *stack* the battery drives, and
    proves the sidecar can partition and heal. It does not write the battery.
  - **Flipping the default backend.** `small-multi-node/` (whatever it ends up named) stays
    the canonical stack and keeps its current behaviour *unchanged*; FDB becomes canonical
    only after #442 records go (#443/#471).
  - **The `wyrd:tikv` image (#471)** and any migration of the TiKV stack onto #470's shared
    Dockerfile skeleton.
  - **Renaming `deploy/small-multi-node/` → `small-multi-node-tikv/`.** DEFERRED by the human
    (Recorded decision 1). Do MUST NOT `git mv` it, nor edit `xtask/src/main.rs`,
    `xtask/tests/deploy_no_orchestrator_coupling.rs`, or the root `README.md`. The naming
    asymmetry is recorded in `deploy/README.md` instead, which the issue explicitly permits.
  - **An `xtask deploy-small-multi-node-fdb` bring-up arm.** Dropped deliberately — see
    Recorded decision 2. It is the ONLY thing that would make this bundle edit
    `xtask/src/main.rs`, and **439** (wave 1) edits that file's dispatch `match` too. `C4-verify`
    applies `patch.diff` to a worktree reset to **`origin/main`** (`run-verify.sh`
    `_resolve_base_ref`, `:109-113` → the brief's branch target → `origin/main`), so a hunk here
    whose *context* was created by 439's added dispatch arm would fail to apply —
    `run-verify.sh:234`, "patch.diff does not apply on $BASE_REF — the bundle is stale". The wave
    fold gives Do 470's diff; it does **not** move C4-verify's base. Dropping the arm makes this
    bundle's file set disjoint from 439's.
  - The closed write path (#455) and the standalone-gateway limitation (#454) — see
    Production reach.
- **Repro instruction:** On `origin/main` in the target checkout:
  `git -C ../wyrd ls-tree --name-only origin/main deploy/` → no `fdb-multi-replica`, no
  `small-multi-node-fdb`. `git -C ../wyrd show origin/main:deploy/small-multi-node/docker-compose.yml | sed -n '232,237p'`
  → `FEATURES: "tikv,etcd"`. `git -C ../wyrd grep -n "fdb" origin/main -- deploy/README.md`
  → no output: the FDB single-node profile that *does* exist is not even documented.
- **External dependencies:**
  - *Binding criterion, assertion 1:* **none** beyond the base Rust toolchain (pure
    filesystem reads).
  - *Binding criterion, assertion 2:* Docker + the compose plugin (**present**; `docker
    compose config` only parses, it starts nothing).
  - *Supplementary live legs:* Docker; ability to pull `foundationdb/foundationdb:7.3.77`;
    a built `wyrd-iptables:local` sidecar image; `NET_ADMIN` / `--privileged` for the
    sidecar's `iptables` in the target netns (the `deploy/tikv-multi-replica/iptables-agent/`
    Dockerfile documents the exact `docker run --rm --privileged --network container:<node>`
    invocation). `fdbcli` **7.3.77** on the host (present) for `status minimal`.
  - *Accepted-diff dependency:* #470's `wyrd:fdb` image and `deploy/docker/wyrd/Dockerfile`,
    arriving via the wave fold. If it is absent when Do runs, the fold has failed — **stop
    and say so**; do not fall back to the test Dockerfile, which cannot build `--features fdb`.
  - *Topology shape:* a ≥3-process FDB cluster is required for (a) to be meaningful — a
    single-process cluster cannot exhibit the replica-loss and mid-commit-kill faults #442
    samples. Do not "simplify" it to one process.
- **Test file:** `xtask/tests/fdb_deploy_profiles.rs` — **a NEW file. Do MUST NOT extend
  `xtask/tests/deploy_no_orchestrator_coupling.rs`.** This reverses an earlier draft of this
  brief; the reason is mechanical. `engine/scripts/run-verify.sh` keys its red→green on a patch
  that **ADDS** a `*/tests/*.rs` (`_is_test_file`, `:69`; `_added_files`, `:68`). A patch that
  only *modifies* an existing test file yields **no `ADDED_TEST`**, so `C4-verify` falls through
  to `TEST_ARGS=(-p xtask)` and takes the **green-only** branch (`:244-254`) — the bundle would
  never demonstrate a per-fix RED. Confirmed with the real classifier: modify-in-place ⇒ `CRATE
  xtask`, **no `ADDED_TEST`**; own-file ⇒ `ADDED_TEST xtask/tests/fdb_deploy_profiles.rs`.
  Follow `xtask/tests/readme_dev_section.rs`, which keeps its helpers (`workspace_root`, `read`)
  **local to the test file** and imports nothing from `xtask`'s lib — so this bundle needs no
  `xtask/src/` change at all. `deploy_no_orchestrator_coupling.rs` stays byte-untouched and its
  two existing signals keep gating.
- **Verification posture:** Mixed, declared here so it lands as a pre-declared sign-off item.
  - **Flippable at Check:** assertions 1–3 above. Assertion 1 needs nothing; assertion 2
    needs only `docker compose config` (parse, no bring-up); assertion 3 is the existing
    green.
  - **Live at Check, runnable here:** the `fdb-multi-replica` bring-up + `fdbcli --exec
    "status minimal"` healthy + one partition/heal cycle through the sidecar.
  - **Deferred (off-Check), with a named confirmer:** the full `small-multi-node-fdb`
    bring-up (21 containers, requires the `wyrd:fdb` image to be built) and the "an S3 gateway
    answers with `--metadata-backend fdb` end to end" smoke bar. **There is no
    `cargo xtask deploy-small-multi-node-fdb` arm** — Recorded decision 2 keeps it out of this
    slice, because it is the one change that would put `xtask/src/main.rs` in this write-set.
    The confirmer is therefore the **maintainer, by hand**: `docker compose -f
    deploy/small-multi-node-fdb/docker-compose.yml up -d`, then an S3 `GET` against a gateway.
    `deploy/README.md` must spell that command out, since no runner encodes it. A one-command
    arm is a named follow-up (natural home: alongside #442's battery, which needs a runner).
    Never `cargo xtask ci` — container-free by policy (ADR-0016).
  - **Deferred ≠ unbuilt (the #146 forcing function):** what is BUILT AND EXERCISED at Check is
    (i) both compose files, parsed by `docker compose config` and asserted role-complete;
    (ii) `deploy/README.md`'s matrix, asserted to name every profile **and** the TiKV/FDB
    pairing; and (iii) the `fdb-multi-replica` stack, brought **fully up** and partitioned/healed
    through the sidecar. The only deferred thing is the *execution* of a 21-container bring-up
    whose every service definition has been parsed and role-checked. Nothing here is inert
    scaffolding — and nothing here is a dispatch arm, because this slice ships none.
- **Production reach:** `small-multi-node-fdb` reaches the **same smoke bar** as its TiKV
  peer, which `deploy/README.md` already documents as *not* an end-to-end object pipeline:
  the `wyrd s3` gateways are standalone islands (`cmd_s3` hardcodes local redb + FS +
  `MemCoordination`, #454) and there is **no closed write path** (#455). So "an S3 gateway
  answers with `--metadata-backend fdb`" means the role starts and serves, not that an object
  round-trips through FDB and the 9 D servers. The full closed write path remains #455's
  demonstration. Declared here so the recurring "is this stack causally sufficient?" question
  is pre-answered rather than surfacing as a NEEDS-HUMAN at sign-off: it is sufficient for
  *topology bring-up parity*, which is what this slice claims and what #442 needs.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. This is
  a composition slice — Do MAY open these cited peer callsites to mirror the pattern:
  - `deploy/tikv-multi-replica/docker-compose.yml:1-28` — **read the header comment
    carefully.** It records the "iteration-13 topology fix": host networking made every
    node's outbound traffic source from `127.0.0.1`, so an `iptables -s/-d <ip>` cut never
    matched and the fault silently did not materialize. The bridge-network + one-netns-per-node
    topology (`:30-35`, `networks.tier1`, `subnet: 172.30.57.0/24`) is what makes a partition
    genuinely bidirectional. `deploy/fdb-multi-replica/` MUST use the bridge topology for the
    same reason — and note this is the **opposite** choice from `deploy/fdb-single-node/`,
    which deliberately uses `network_mode: host` so a *host* client can dial it.
  - `deploy/tikv-multi-replica/iptables-agent/Dockerfile` (whole file, 14 lines) — the
    generic sidecar to reuse, and its "Invariant B: the worst-case residue dies with the
    container" rationale.
  - `deploy/small-multi-node/docker-compose.yml:232-237` (build stanza / `FEATURES` /
    `wyrd-single-zone:local` tag) and its per-role service definitions — the topology to
    mirror.
  - `deploy/README.md` §"`small-multi-node/`" — the role-cardinality table (why 9 D servers:
    one per failure domain for the default `rs(6,3)`), the "Genuinely wired backends"
    paragraph, and the "Two honest limits" paragraph the FDB peer inherits verbatim.
  - `xtask/tests/deploy_no_orchestrator_coupling.rs:1-29` (module doc: the two Check-time
    signals), `:67` (planted red), `:102` (green on the real tree), `:133`
    (`docker_compose_available`), `:147-163` (the compose-config structural check to
    parallel).
  - `xtask/src/main.rs:658-660` (`SMALL_MULTI_NODE_PROJECT` + the compose path) — **read-only
    context**, cited so Do can see what the deferred rename would have had to move. This slice
    writes **nothing** in `xtask/src/`; there is no bring-up arm to mirror.
  - `crates/server/src/cli.rs:120` / `:168` — proof `--metadata-backend fdb` resolves and
    opens a store on `main` (merged #440), so the compose `command:` lines are not
    speculative.
- **Prior-art check (triage cycles):** Searched by affected file path across merged history,
  open PRs, and closed/rejected PRs.
  - `deploy/` history: `22d39b6` (#438 — added `fdb-single-node/`), `06cbfe5` (#460),
    `cf45b08` (#459), `20fd2af` (PR #457 — consolidated the single-zone stack, ADR-0043).
    `git -C ../wyrd grep -in "fdb" origin/main -- deploy/README.md` → **no output**: no FDB
    deploy documentation has ever existed.
  - `deploy/fdb-multi-replica/` and `deploy/small-multi-node-fdb/` have never existed on any
    branch (`git -C ../wyrd log --all --oneline -- deploy/fdb-multi-replica` → empty).
  - Rejected work: one non-merged closed PR in the last 60 (#400, docs/proposal scope) —
    unrelated to `deploy/`. PR #457 (the stack this pairs) is **merged**. #442, #443, #471
    are all **open** and downstream of this slice.
- **Disposition hint:** new-feature

## Recorded decisions (settled at Plan — Do MUST NOT reopen them)

### 1. The rename is DEFERRED. `deploy/small-multi-node/` keeps its name.

The issue asks that, once the FDB peer lands, `deploy/small-multi-node/` be renamed to
`deploy/small-multi-node-tikv/` so neither reads as "the" stack — while **explicitly allowing
the rename to be deferred**, "to avoid churning PR #457's landed paths", and recorded in the
README instead. The human took the deferral.

**What Do does instead:** add the two new directories, and make `deploy/README.md` state the
naming asymmetry plainly — `small-multi-node/` **is** the TiKV peer of `small-multi-node-fdb/`;
it is canonical until #442 records go, FDB after (#443's retained-fallback posture). The
issue's requirement ("two clearly-named peer setups") is met by the README's matrix, which the
success criterion's assertion 1 pins.

**What Do must NOT do:** rename or `git mv` anything under `deploy/`; edit `xtask/src/main.rs`;
edit `xtask/tests/deploy_no_orchestrator_coupling.rs`; edit the root `README.md`.

Why it matters beyond taste — the rename would have touched these landed references:

| Reference | Caught if missed? |
|---|---|
| `xtask/src/main.rs:658` (`SMALL_MULTI_NODE_PROJECT`), `:660` (compose path) | **yes** — the bring-up smoke fails |
| `xtask/tests/deploy_no_orchestrator_coupling.rs:163` (compose path) | **yes** — `small_multi_node_compose_config_is_structurally_valid` (`:148`) goes red |
| `xtask/src/main.rs:39-46`, `.../deploy_no_orchestrator_coupling.rs:19` (doc comments) | no — prose |
| `deploy/README.md` §"`small-multi-node/`" | **no** |
| `README.md:177-180` (the `deploy/` fixtures paragraph) | **no** |

`xtask/tests/readme_dev_section.rs` does **not** cover the last two — verified: `git -C ../wyrd
grep -n "deploy" origin/main -- xtask/tests/readme_dev_section.rs` returns nothing; it pins only
the `## Development & testing` section's `cargo xtask` subcommands and the `wyrd demo` line. So
a rename would have drifted both README files silently.

Deferring also removes `xtask/src/main.rs` from this bundle's write-set, which is what makes it
**disjoint from 439** and lets `Conflicts with` be dropped — no fold conflict, no cross-wave
`C4-verify` stale-patch risk. A follow-up slice can take the rename on its own, where it is a
one-bundle mechanical change with no batch coupling.

### 2. No `xtask deploy-small-multi-node-fdb` bring-up arm in this slice.

Same reason, same file: the arm is the other thing that would put `xtask/src/main.rs` in this
write-set. Cost: the deferred `small-multi-node-fdb` bring-up smoke has no one-command runner
and stays a documented `docker compose up` until a follow-up slice adds it (natural home:
alongside #442's battery, which needs a runner anyway).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: #470 is now merged and ready — the hard-precondition blocker (absent `deploy/docker/wyrd/Dockerfile` / `wyrd:fdb` image) is cleared, so the single-zone stack is now buildable in the target tree. Rebuild against that merged base and address the reviewer's actionable items: - Tighten the tautological backend-wiring assertion in `xtask/tests/fdb_deploy_profiles.rs:948-951`. `contains("--metadata-backend") && contains("fdb")` is true regardless of what backend the roles pass ("fdb" appears 84x via image/service/volume names). Assert that EVERY wyrd role (dservers, custodians, gateways) opens `--metadata-backend fdb` and that NO role uses `tikv` — the current guard passes even if the three gateways are flipped to tikv. - Now that #470 supplies `wyrd:fdb`, exercise the deferred single-zone leg beyond parse-only: at minimum confirm the build context resolves and the image builds; run the full configured harness (`./engine/xtask.sh ci`), not just the scratch substitute, so C4 has the configured-oracle evidence.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
