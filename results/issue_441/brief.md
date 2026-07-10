# Design proposal — issue 441 / fdb-packaging-and-version-coupling

> The Plan artifact for the **exception**: a change significant enough to warrant a
> design proposal. Do reads ONLY this file and implements it; Check runs the regular
> gated check on the code.

- **Slug:** fdb-packaging-and-version-coupling
- **Kind:** enhancement (design proposal)
- **Goal:** Under `--features fdb`, Wyrd (a) **fails closed with a guided, actionable
  error** when the client library and the cluster disagree on protocol version, instead
  of surfacing an anonymous timeout; (b) can load a **multi-version client** directory so
  a lockstep FoundationDB cluster upgrade is a configuration/image change rather than an
  architecture change; and (c) carries a written packaging contract — container path,
  bare-metal path, the upgrade dance, and the explicit single-binary trade.
- **Success criterion:** `cargo test -p wyrd-metadata-fdb --test preflight` passes on the
  **default build** (no `fdb` feature, no `libfdb_c`, no Docker, stock toolchain),
  asserting all three of:
  *(`--test preflight`, not `--lib`: `--lib` runs only the library target's inline unit tests
  and would never execute `crates/metadata-fdb/tests/preflight.rs`. This is also the exact
  command `run-verify.sh` reconstructs from the added test file — `TEST_ARGS+=("-p" "$pkg"
  "--test" "$(basename "$t" .rs)")`, `:205`.)*
  1. `wyrd_metadata_fdb::preflight` exists as a **non-feature-gated, pure** module —
     `verdict(status: Option<&ClientStatus>, elapsed, deadline) -> Verdict` with
     `Verdict ∈ {Ready, VersionSkew{…}, Unreachable{…}}` — sibling to the two pure
     modules that already ship this way, `classify` (`crates/metadata-fdb/src/lib.rs:141`)
     and `config` (`:382`, doc-commented "Pure input →").
  2. `preflight::message(Verdict::VersionSkew{..})` renders text containing **all** of:
     the substring `protocol`, the **cluster's** protocol version, and a pointer to the
     multi-version external-client-directory upgrade procedure. A `Verdict::Unreachable`
     message must NOT claim version skew.
  3. `verdict()` maps each of the **three real status shapes captured at Plan** (§Design 1's
     table — skew / unreachable / healthy) to `VersionSkew` / `Unreachable` / `Ready`. These
     are fixtures from a live `libfdb_c` 7.3.77, not invented inputs, so the classifier is
     pinned against observed reality. (A *demonstrated* red — negate the `Compatible` arm and
     watch assertion 3 fail — is a Do-time procedure to record in `build-notes.md`, not a
     shipped test.)

  Plus `cargo xtask ci` (the gating `C4-ci`) stays green **on a machine with no FDB**.
- **Falsifiability:** RED is producible at Check with **zero external dependencies**, on
  the plain `$PDCA_WORKTREE` / `../wyrd-verify` checkout: the `preflight` module does not
  exist on `origin/main` (`git -C ../wyrd grep -n "mod preflight" origin/main` → no
  output), so assertions 1–2 fail to compile pre-fix, and assertion 3 is a live negation
  test. This is why the decision logic is required to be **non-feature-gated**: the whole
  `store` module is `#[cfg(feature = "fdb")]` and `cargo xtask ci` never enables that
  feature (`feature_gated_checks()`, `xtask/src/main.rs:1255`, lists only tikv) — a guard
  buried inside `store` would be code **no gate compiles**, and its RED would only ever be
  asserted by code-reading. That is the Plan-blocking shape this field exists to catch,
  and the design above avoids it by construction.
  **The live, whole-system red — a 7.3 client against a 7.1 cluster — is deliberately NOT
  this bundle's binding criterion.** It needs a second FoundationDB image and a built
  `wyrd:fdb` image to be meaningful, both of which are #470's deliverables, and #470's
  body already assigns it: *"the image is where #441 item 4's version-skew guard is
  verified."* This bundle ships the guard + a documented manual repro; #470 automates it.
  Recorded so the split is a pre-declared sign-off item rather than a surprise NEEDS-HUMAN.
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`; branch
  deleted. Prereqs #438 (PR #492) and #440 (PR #493) are both **merged into `main`** — the
  issue's "Depends on #438/#439/#440" is satisfied for #438/#440, and the #439 dependency
  is only for the *doctor row* cross-reference, which this brief handles by citation, not
  by code.)
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** One logical change: make the FDB client's version coupling explicit — in code
  (a fail-closed preflight + multi-version client support) and in prose (the packaging
  contract). / **out of scope:**
  - **Building the `wyrd:fdb` image (#470).** The issue's item 1 and its first acceptance
    criterion ("CI builds an image where `wyrd --metadata-backend fdb` connects to the
    compose cluster out of the box") are, by #470's own body, *"the concrete implementation
    of #441's container image (primary path) decision"* and *"#441's first acceptance
    criterion, discharged here."* This bundle **decides and documents** the container path
    and ships the code it needs; #470 builds it.
  - **Editing ADR-0014 or ADR-0042.** Both are `status: Accepted` and therefore **frozen**
    by the `docs-immutability` gate (`.github/workflows/docs-immutability.yml`; ADR-0001
    §"Accepted"), which permits only a one-way supersession stamp. Verified:
    `git -C ../wyrd show origin/main:docs/design/adr/0014-single-binary-dev-only.md | head`
    → `status: Accepted`; same for `0042-production-metadata-backend-reevaluation.md`. The
    issue's phrase "state explicitly in the ADR-0042 docs" must therefore be read as *the
    documentation accompanying ADR-0042*, not the ADR file. Do MUST NOT touch either ADR.
  - The doctor rows (#439), the deploy profiles (#469), the fault battery (#442).
- **External dependencies:**
  - *Binding criterion:* **none** beyond the base Rust toolchain — by design (see
    Falsifiability).
  - *Supplementary legs, all present on this host:* Docker + compose plugin; `libfdb_c.so`
    **7.3.77**; `fdbcli` **7.3.77**. The existing `cargo xtask fdb-conformance`
    (`xtask/src/main.rs:292`) already runs a `--lib` leg with `--features fdb`
    (`run_fdb_conformance_test`, `:410-417`), so the feature-gated call into `preflight`
    IS exercised by an existing job Do can run here. Do SHOULD run it and record the output.
  - *For the documented manual repro only:* the ability to pull
    `foundationdb/foundationdb:7.1.x`. If unavailable, document the procedure and say so —
    do not fabricate a result.
- **Test file:** `crates/metadata-fdb/tests/preflight.rs` — **its own file, NOT an inline
  `#[cfg(test)] mod tests` inside `src/lib.rs`.** Load-bearing, not style. Verified against the
  harness: `engine/scripts/run-verify.sh` keys its red→green on a patch that **ADDS** a
  `*/tests/*.rs` (`_is_test_file`, `:69`); a test co-located inside a modified production file
  yields **no `ADDED_TEST`**, so `C4-verify` degrades to its **green-only** branch (`:244-254`
  — "the per-fix RED can't be isolated … Ship the test as its own file to earn the full
  red->green"). Confirmed by running the real classifier over this bundle's patch shape:
  co-located ⇒ `CRATE crates/metadata-fdb`, **no `ADDED_TEST`**. Because `preflight` is a
  `pub`, non-feature-gated module, an integration test can `use wyrd_metadata_fdb::preflight::*`
  under the default build — so the own-file form costs nothing and earns the real red→green
  (RED = the reverted `src/lib.rs` has no `preflight`, the kept test fails to compile, non-zero
  exit).
- **Verification posture:**
  - **Flippable at Check:** the `preflight` unit tests above (red pre-fix by
    non-existence + a live demonstrated-red negation). No external dependency.
  - **Exercised at Check under `--features fdb`, runnable here:** `cargo xtask
    fdb-conformance`'s `--lib` leg compiles and runs `store`'s call into `preflight`.
  - **Deferred (off-Check), with a named confirmer:** the end-to-end *wrong-version cluster
    produces the guided error* run. Confirmed by **#470**, whose acceptance criterion 4 is
    exactly that check, against the image it builds. Until then a **documented manual
    repro** ships in the doc page (§Design item 4).
  - **Deferred ≠ unbuilt:** the guard itself is fully BUILT and EXERCISED at Check — the
    pure classifier by unit tests, the gated call-site by the `--lib` leg. What is deferred
    is only the *live mismatched cluster* that drives it. Nothing here is inert scaffolding.
- **Production reach:** The production path DOES traverse the new seam at Check-time-plus:
  `crates/server/src/cli.rs:168` (`open_fdb_meta`) calls `FdbMetadataStore::connect()`
  (`crates/metadata-fdb/src/lib.rs:898`), which is where the preflight lands — so every
  `wyrd … --metadata-backend fdb` invocation goes through it. The only thing standing in
  for production today is the *mismatched cluster*, not the code path. Declared so the
  recurring "is this seam causally sufficient?" question is pre-answered: it is; the
  production caller already exists (merged #440).
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. Do
  MAY open these cited peer callsites:
  - `crates/metadata-fdb/src/lib.rs:141` (`mod classify`) and `:382` (`mod config`) — the
    two existing **non-feature-gated pure modules**; `preflight` is a third sibling and must
    match their shape (pure fn, inline unit tests, no `foundationdb` types in the signature).
  - `crates/metadata-fdb/src/lib.rs:853-869` — `static NETWORK: OnceLock<NetworkAutoStop>`
    and `ensure_network()`, whose `#[allow(unsafe_code)]` + SAFETY-comment shape MUST be
    preserved when `foundationdb::boot()` (`:868`) is replaced (see Design item 2). Note
    `#![forbid(unsafe_code)]` is deliberately *not* used in this crate (`:126-127`).
  - `crates/metadata-fdb/src/lib.rs:898` (`connect`) and `:911` (`open`) — the constructors.
  - `crates/metadata-fdb/src/lib.rs:416` (`DEFAULT_TRANSACTION_TIMEOUT_MS`) — the deadline
    that already exists; see the correction in §Motivation.
  - `crates/server/src/cli.rs:168` (`open_fdb_meta`) — the production caller.
- **Prior-art check (triage cycles):** Searched by affected file path across merged
  history, open PRs, and closed/rejected PRs.
  - `crates/metadata-fdb/src/lib.rs` history: `22d39b6`, `576fc15`, `ae05a45` (all #438).
    `git -C ../wyrd grep -n "protocol_version\|get_client_status\|ExternalClientDirectory"
    origin/main` → **no hits.** No version-skew handling has ever existed.
  - Rejected work: one non-merged closed PR in the last 60 (#400, docs/proposal scope) —
    unrelated. Closed prereqs: #436 (the ADR-0042 decision), #438, #440.
  - `docs/design/architecture/07-deployment-view.md` is `status: living` (verified
    frontmatter) → editable, unlike the ADRs.
- **Disposition hint:** new-feature

## Motivation

ADR-0042 chose FoundationDB as the production metadata backend. Unlike the pure-Rust TiKV
client, the `foundationdb` crate binds a **shared C library** (`libfdb_c`) whose wire
protocol is **exactly** coupled to the cluster's: a 7.3 client cannot talk to a 7.1
cluster, at all, ever. FDB's own answer is the *multi-version client* — a directory of
additional `libfdb_c` versions loaded via `FDB_NETWORK_OPTION_EXTERNAL_CLIENT_DIRECTORY`,
which lets a client bridge a lockstep cluster upgrade.

Today Wyrd has none of this. `ensure_network()` (`crates/metadata-fdb/src/lib.rs:856`)
calls `foundationdb::boot()` (`:868`), which is a fixed
`FdbApiBuilder::default().build()?.boot()` with **no way to set a network option** — so no
external-client directory can be configured. And nothing checks that the client reached the
cluster before the first transaction is issued.

**Correction to the issue body, verified against `origin/main`.** The issue says the
symptom is *"an indefinite 'waiting for cluster'"* hang. That was true when the issue was
filed; it is **no longer accurate**. #438 subsequently landed a per-transaction deadline
(`config::DEFAULT_TRANSACTION_TIMEOUT_MS`, `crates/metadata-fdb/src/lib.rs:416`, applied
by `connect()` at `:898-907` and pinned by `crates/metadata-fdb/tests/timeout.rs`). So a
wrong-version client today produces a **bounded but anonymous** failure — a `1031`
`transaction_timed_out`, classified as an undeterminable outcome — which is
indistinguishable from "the cluster is down" or "the network is partitioned". The defect is
therefore **misdiagnosis, not hanging**: an operator who has mismatched their client sees
the same error as an operator whose cluster is genuinely unreachable, and version coupling
is the single most common FDB misconfiguration. That is worth fixing now, before #470 bakes
a client into an image and #469 stands up multi-node clusters where skew becomes likely.

## Design

**1. A pure `preflight` module (non-feature-gated).**
`crates/metadata-fdb/src/lib.rs` gains `pub mod preflight`, sibling to `classify` (`:141`)
and `config` (`:382`). Signature shape:

- `pub struct ClientStatus { pub healthy: bool, pub coordinators_reachable: bool, pub client_version: String, pub cluster_protocol: Option<String> }`
- `pub enum Verdict { Ready, VersionSkew { client: String, cluster: Option<String> }, Unreachable { waited: Duration } }`
- `pub fn verdict(status: Option<&ClientStatus>, elapsed: Duration, deadline: Duration) -> Verdict`
- `pub fn message(v: &Verdict) -> String`

It takes **no `foundationdb` type** in its signature. That is the whole point: it compiles
and its tests run in the default `cargo xtask ci` on a machine with no `libfdb_c`.

**The discrimination rule — established empirically at Plan, not guessed.** I called
`fdb_database_get_client_status` through the host's `libfdb_c` 7.3.77 against three real
clusters and captured the JSON:

| case | `Healthy` | `Connections[0].Status` | `Connections[0].Compatible` | `Connections[0].ProtocolVersion` | `NumConnectionsFailed` |
|---|---|---|---|---|---|
| **skew** (7.3 client → `foundationdb:7.1.61`) | `false` | `"connected"` | **`false`** | `"fdb00b071010000"` (the *cluster's*) | `0` |
| **unreachable** (nothing listening) | `false` | `"failed"` | `true` | **absent** | `1` |
| **healthy** (7.3 → `foundationdb:7.3.77`) | `true` | `"connected"` | `true` | `"fdb00b073000000"` | `0` |

The discriminator is **`Compatible == false` on a connection whose `Status` is `"connected"`**.
It is *not* "zero reachable coordinators" — under skew the `Coordinators` list is populated and
`CurrentCoordinator` is set — and *not* `Healthy == false` alone, which is false in **both**
failure cases. `Unreachable` is `Status == "failed"` with no `ProtocolVersion`. Both settle
within ~2 s (`LastConnectTime` 1.93 s in the skew run), so a ~5 s deadline separates all three.
Do must still fail **honest**: an unparseable or novel status degrades to `Unreachable` with a
version-coupling hint, never to a false `VersionSkew`.

**2. Multi-version client support in `ensure_network()`.**
Replace `foundationdb::boot()` (`:868`) with the builder path, which the crate exposes:
`api::FdbApiBuilder::default().build()?` → `NetworkBuilder::set_option(NetworkOption::ExternalClientDirectory(dir))?`
→ `.boot()`. Verified against the vendored crate at
`~/.cargo/registry/src/*/foundationdb-0.10.0/`: `NetworkBuilder::set_option` is
`src/api.rs:117`; `NetworkOption::ExternalClientDirectory(String)` is generated into
`OUT_DIR/options.rs` from `foundationdb-gen`'s `fdb.options` (option code 63) and is
present in this workspace's build output. The directory comes from a new
`WYRD_FDB_EXTERNAL_CLIENT_DIR` env var parsed by `config` (peer: `CLUSTER_FILE_ENV` at
`:386`); when unset, behaviour is byte-identical to today.

**Two hazards Do must respect.** `NetworkBuilder::boot` is *also* `unsafe`, so the existing
`#[allow(unsafe_code)]` + SAFETY comment at `:856-869` must be carried over, not deleted.
And the `OnceLock` "exactly one network per process, guard never dropped" contract
(`:845-853`) is load-bearing for the seven-store conformance suite — do not restructure it.

**3. The readiness probe in `connect()`.**
`FdbMetadataStore::connect()` (`:898`) — the production entry point reached from
`crates/server/src/cli.rs:168` — performs a bounded probe before returning `Ok`. Use
`Database::get_client_status()`, verified present at
`foundationdb-0.10.0/src/database.rs:135` and gated `#[cfg_api_versions(min = 730)]`, which
our pin satisfies (`Cargo.toml:108`: `features = ["fdb-7_3"]`). It returns client-side
status JSON. Feed it to `preflight::verdict`; on a non-`Ready` verdict return
`Err(preflight::message(..))`. The client library version for the message comes from
`api::get_max_api_version()` (`src/api.rs:26`) — which returns the **API version** (`730`),
*not* the library version string. **Do not conflate them** (an earlier draft of this brief did).
The real version string `"7.3.77,<sha>,fdb00b073000000"` comes from `fdb_get_client_version()`,
which `libfdb_c` exports and the generated `foundationdb-sys` bindings carry, but which
`foundationdb` 0.10 does **not** expose in its safe API (verified: no `-sys` re-export from
`foundationdb-0.10.0/src/lib.rs`). Reaching it needs a direct `foundationdb-sys` dependency
**and a second `unsafe` block** — contradicting `crates/metadata-fdb/src/lib.rs:126-128`
("exactly one `unsafe` block exists in this crate, in `store`") and tripping INTEGRATION §4's
human-only new-dependency rule. **So do not call it.** The message names (a) the API version
from the safe `get_max_api_version()` plus the `fdb-7_3` pin, and (b) the **cluster's**
`ProtocolVersion` from the status JSON — the field that actually identifies the mismatch. If
the human wants the full client version string, that is a separate, dependency-adding slice.

`open()` (`:911`) keeps its current no-probe behaviour so the conformance/scan/timeout test
harnesses — especially `tests/timeout.rs`, which deliberately points at an unreachable
coordinator — are unaffected. The probe belongs to `connect()`, the *operator* path.

**4. The packaging document.**
**A new section in `docs/design/architecture/07-deployment-view.md`** (`status: living`, so
editable; already the deployment home). This resolves what an earlier draft left ambiguous —
it said "one new page, linked from `docs/design/README.md`" in this section while Open
question 2 said "prefer extending 07". **Extending 07 is the default and the only thing Do
should do absent a human override**, and the file is pinned because it is this bundle's
exclusively: **439** (same wave) is barred from `docs/design/` for exactly this reason, since
two wave-1 bundles editing one file are built blind on the same base and a fold conflict is a
hard STOP. Do MUST NOT create a new page and MUST NOT touch `docs/design/README.md`.

It covers: container path (primary; the image itself is #470), bare-metal
`foundationdb-clients` `.deb`/`.rpm` as a host prerequisite, the cluster file, version
coupling, the multi-version upgrade dance (add the new `libfdb_c` to the external-client
directory → upgrade the cluster → drop the old library), and a **documented manual repro** of
the guided error against a deliberately mismatched `foundationdb/foundationdb:7.1.x`.

It states the single-binary trade explicitly. **No ADR is edited.** The reconciliation with
ADR-0014 needs no ADR change and no superseding ADR, because ADR-0014 already decides that
the single-binary profile is *"for development and evaluation only, explicitly not a
supported production tier"* — and `fdb` is a production tier. The default (`redb`) build
remains a true static single binary with zero new demands; `crates/metadata-fdb/Cargo.toml`
already enforces this (`default = []`, "a machine without it could not compile this crate at
all if the feature were on by default"). The doc records the trade; the ADRs already imply
it. If the human judges that a superseding ADR *is* wanted, that is a separate bundle —
INTEGRATION §4 makes any ADR change a NEEDS-HUMAN item by design.

## Alternatives considered

- **Bake the guard into `store` (feature-gated).** Rejected on a checkable basis, not an
  adjective: `cargo xtask ci` never enables `fdb` (`feature_gated_checks()`,
  `xtask/src/main.rs:1255-1264`, one entry, tikv). A guard there is code no gate compiles,
  so its RED could only be asserted by reading it. The pure-module split costs ~1 struct +
  1 enum and buys a flippable Check-time test.
- **Probe with a real transaction (`get_read_version`) instead of `get_client_status`.**
  Rejected: it cannot distinguish skew from a down cluster — both time out at
  `DEFAULT_TRANSACTION_TIMEOUT_MS` (`:416`) — which is exactly today's defect.
- **Static single binary via `libfdb_c` static linking.** Rejected: FoundationDB does not
  support it, and it would defeat the multi-version client, which is the only sanctioned
  upgrade mechanism.
- **Do nothing; document the symptom.** Rejected: #469 stands up 3-process clusters and
  #470 bakes a client version into an image. Skew stops being hypothetical at that point.

## Impact & compatibility

- **Default (`redb`) builds: no change whatsoever.** No new dependency, no `libfdb_c`
  demand, no behaviour change. The new `preflight` module is pure Rust and adds two unit
  tests to `cargo xtask ci`.
- `connect()` gains a bounded startup probe. Worst case it adds one round-trip's latency
  on a healthy cluster; on an unhealthy one it fails *faster* and more informatively than
  the current first-transaction timeout.
- `open()` is unchanged, so `tests/timeout.rs`, `tests/scan.rs`, `tests/contention.rs` and
  `tests/conformance.rs` keep their current semantics.
- New env var `WYRD_FDB_EXTERNAL_CLIENT_DIR`; unset ⇒ today's behaviour.
- Deprecations: none. Migration: none.
- Risk: the skew/unreachable discrimination depends on the shape of `get_client_status`'s
  JSON, which is not a stability-guaranteed surface. Mitigated by the design's
  fail-honest rule (§Design 1): an unparseable or ambiguous status degrades to
  `Unreachable` *with* a version-coupling hint, never to a false `VersionSkew`.

## Open questions

1. **For the human, before sign-off:** is a superseding ADR wanted for the single-binary /
   shared-library trade, or does the living deployment doc suffice? This brief takes the
   position that ADR-0014 already scopes single-binary to dev/eval and therefore needs no
   change — but ADR authorship is the architecture board's, not a model's (INTEGRATION §4,
   GOVERNANCE), so the call is yours.
2. Where should the packaging page live —
   `docs/design/architecture/07-deployment-view.md` (already `status: living`, already the
   deployment home) as a new section, or a standalone `docs/design/architecture/` page? Do
   should prefer extending 07 unless the human says otherwise.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The plan and the brief stand; the implementation missed the async seam. Rebuild against the same brief — do NOT re-plan. WHAT IS WRONG `FdbMetadataStore::preflight()` builds its own current-thread Tokio runtime and calls `Runtime::block_on`. Tokio panics ("cannot start a runtime from within a runtime") when that happens inside an existing runtime context. All seven call sites of the production entry point `open_fdb_meta()` are already inside one: cli.rs:374, :460, :1552, :1600 (directly inside `runtime.block_on(async {...})`) and cli.rs:841, :1481, :1487 (inside an `async fn`). So every `wyrd ... --metadata-backend fdb` invocation panics with exit 101 — confirmed by the reviewer's live smoke run and re-verified at sign-off by enumerating the call sites. WHY THE GATES MISSED IT The pure `preflight` module has no runtime, so its unit tests pass; and `cargo xtask ci` never enables the `fdb` feature, so no gate ever compiled the panicking code. The green C4-ci is not evidence of a working production path. The brief's own "Production reach" section (brief.md:115) is what makes this total rather than marginal: it is correct that all production traffic traverses `connect()`. WHAT TO CHANGE Make the probe async rather than spawning a nested runtime. Mirror the peer that already does this correctly: `open_tikv_meta()` is `async` and is `.await`ed by its callers. Concretely — make `connect()` (and `preflight()`) `async`, make `open_fdb_meta()` `async`, and `.await` it at all seven call sites. Update the now-stale doc comment at cli.rs:165 which asserts "`connect()` is synchronous (unlike `open_tikv_meta`'s `.await`)" — that sentence is the trap the build fell into. Keep the polling loop's semantics (re-poll until settled or deadline); only the runtime ownership changes. `open()` must stay probe-free, as the brief requires, so tests/timeout.rs, scan.rs, contention.rs, conformance.rs are unaffected. Add coverage that would have caught this: the existing unit tests exercise only the pure classifier and cannot. A test that drives `connect()` from within a Tokio runtime is what closes the hole. RESOLVED AT SIGN-OFF — do not re-litigate Brief Open question 1 (superseding ADR for the single-binary / shared-library trade): NO ADR IS NEEDED. The human confirms the brief's position — ADR-0014 already scopes the single-binary profile to development and evaluation only, and `fdb` is a production tier, so the living deployment doc (docs/design/architecture/07-deployment-view.md) suffices. Do MUST NOT touch ADR-0014 or ADR-0042. CARRIED FORWARD UNCHANGED The deferred live wrong-version-cluster validation stays #470's, as the brief pre-declared (brief.md:49). Not a defect of this bundle.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
