- **Slug:** deps-bump-h2-rustsec-2026-0258
- **Kind:** dependency / security bump
- **Defect:** `cargo deny check` fails on a clean `main` @ `65ca4fd`:
  `error[vulnerability]: h2 unbounded empty DATA frames` — RUSTSEC-2026-0258
  (GHSA-q83h-524g-xf6h), `h2 0.4.15`, `patched = [">= 0.4.16"]`, advisory published
  2026-08-17. `cargo_deny_check()` runs inside `run_ci` (`xtask/src/main.rs:1563`), and
  `C4-ci` is this instance's one **gating** gate, so the entire repo's gate is red before any
  patch applies. The exposure is real and on normal (shipped) edges, not dev-only:
  `cargo tree -e normal -i h2@0.4.15` reaches it from `wyrd-server` three ways — via `axum`
  (the S3 gateway's HTTP surface), via `tonic` (`wyrd-proto`, `wyrd-chunkstore-grpc`,
  `tonic-health`), and via `opentelemetry-otlp` (`wyrd-telemetry`, `wyrd-custodian`). The
  flaw queues empty HTTP/2 DATA frames without limit: unbounded memory growth, or a panic on
  length overflow. Low severity, unauthenticated, on the request-serving path.
- **Success criterion:** BINDING, and the gate itself is the oracle. On the patched tree:
  1. `cargo deny check` exits 0 and reports `advisories ok, bans ok, licenses ok, sources
     ok` (the pre-existing `license-not-encountered` warning for `"ISC"` at `deny.toml:121`
     is expected and unchanged — it is present on base too);
  2. `Cargo.lock` names `h2 0.4.16` and no `h2 0.4.15` remains;
  3. `cargo xtask ci` exits 0 — the whole-tree gate, which is what proves the transitive
     re-unification below actually compiles;
  4. **no manifest is edited** — `git diff --name-only` against the base lists `Cargo.lock`
     and nothing else. In particular: no `deny.toml` ignore/waiver entry. Waiving the
     advisory instead of fixing it satisfies the gate and not the defect, and would be
     rejected at review.
- **Falsifiability:** The RED is the base itself and needs no construction: on `main` @
  `65ca4fd`, clean tree, `cargo deny check advisories` exits non-zero naming
  RUSTSEC-2026-0258. Verified today, twice — once via `cargo deny --manifest-path
  ../wyrd/Cargo.toml check advisories` against the live checkout, and once in a throwaway
  `git archive` copy where applying the bump flipped it to `advisories ok, bans ok, licenses
  ok, sources ok`. Red and green are both demonstrated, on the ordinary developer harness Do
  is pointed at — no topology, no service.
- **Invariant to restore:** *The dependency wall is green on `main`: no crate in the shipped
  (normal) dependency closure carries an unwaived RUSTSEC advisory.* Stated over the
  category, not over `h2`: the remedy is upgrading the affected crate, never adding an
  `[advisories] ignore` entry — `deny.toml:59-70` records the project's own reasoning for
  auditing `unsound = "all"` across the whole graph rather than just the workspace ("the
  anyhow bump to the patched 1.0.103 lands with this, so the wall is green on a real fix, not
  a waiver", #543). That precedent is binding here.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** On `main` @ `65ca4fd` with a clean tree:
  `cargo deny check advisories` → `error[vulnerability]: h2 unbounded empty DATA frames /
  ID: RUSTSEC-2026-0258 / Solution: Upgrade to >=0.4.16`, exit non-zero. Equivalently
  `cargo xtask ci` fails at the `cargo deny check` step.
- **Scope:** exactly one generated file — `Cargo.lock` — produced by
  `cargo update -p h2@0.4.15 --precise 0.4.16` run at the workspace root. **Bare
  `cargo update -p h2` is ambiguous and will error**: the tree also carries `h2 0.3.27` (via
  `tonic 0.12.3` under `madsim-etcd-client`), which cargo-deny does *not* flag. Do must run
  the disambiguated form and must not touch `h2 0.3.27`.
  **/ out of scope:** any `Cargo.toml` edit (no Wyrd manifest names `h2` — verified across
  all 23 workspace manifests; it is purely transitive through `axum 0.8` / `tonic 0.14` /
  `hyper`); any `deny.toml` change, especially an `[advisories] ignore` entry; any other
  advisory, dependency or code change; any attempt to also fix the pre-existing
  `license-not-encountered` `"ISC"` warning.
- **EXPECTED, pre-declared so it is not a surprise at review — the diff is 13 insertions /
  13 deletions, not 2.** Beyond `h2` (version, checksum, and 5 dependent references), the
  resolver re-unifies four platform edges: `errno`, `rustix` and `tempfile` re-point
  `windows-sys` 0.61.2 → 0.52.0; `nu-ansi-term` → 0.59.0; `hyper-util` re-points `socket2`
  0.6.4 → 0.5.10. This was investigated rather than assumed:
  * **every one of those versions is already in the lockfile** — no new crate enters the
    tree, nothing new to license-audit, and `cargo deny` reports `bans ok` (no new
    duplicate-version violation);
  * it is **not** caused by `h2`: 0.4.15 and 0.4.16 have byte-identical dependency
    requirements and the same `rust_version = 1.63` (checked against the crates.io index);
  * it is **not** ambient churn: a no-op `cargo update -p equivalent@1.0.2 --precise 1.0.2`
    on the same tree changes zero bytes;
  * `--precise 0.4.16` and a plain `cargo update -p h2@0.4.15` produce the identical 13/13
    delta, so there is no more surgical form.
  The cause is the MSRV-aware resolver (workspace `rust-version = "1.96"`, cargo 1.96.1)
  normalising those edges the moment the lock is rewritten. Note `socket2` is
  cross-platform, not Windows-only, so criterion 3 (`cargo xtask ci`) is what actually
  clears it — this child was verified at the dependency-wall level, not at the compiler.
  Do must NOT hand-edit `Cargo.lock` to suppress the re-unification.
- **External dependencies:** `cargo-deny`
- **Test file:** none, deliberately — see **Verification posture**. Do MUST NOT invent one:
  an xtask test asserting `h2 >= 0.4.16` would pin a version number in two places and rot at
  the next bump, and it would assert the patch rather than the property.
- **Verification posture:** DECLARED, not the default. There is no regression test and there
  should not be: the **dependency wall is the oracle**, it already runs inside the gating
  `C4-ci` row, and it is genuinely red on the base and green with the patch — a real
  red→green, just owned by `cargo deny` rather than by `cargo test`. Nothing is deferred:
  both legs are observable at Check on the ordinary harness.
  Pre-declared gate shapes, so neither reads as a defect at sign-off:
  * **`C4-verify` → PASS with `patch touches no Wyrd crate (docs/CI only) — nothing to
    verify per-fix; the C4-ci gate covers it.`** `_crate_dir("Cargo.lock")` is empty, so no
    package is selected and the script early-exits 0 at `run-verify.sh:425-428` before it
    even needs a toolchain. Correct and expected.
  * **`C4-diff-cov` → nothing to score.** The patch contains no `.rs` lines, so there are no
    instrumentable changed lines. Advisory row.
- **Citations expected:** Do must cite `path:line` on `main`. The three that matter:
  `xtask/src/main.rs:1563` (`cargo_deny_check()` inside `run_ci` — why this is gating),
  `deny.toml:59-70` (the project's own "green on a real fix, not a waiver" precedent), and
  the advisory itself (RUSTSEC-2026-0258 /
  https://github.com/hyperium/hyper/security/advisories/GHSA-q83h-524g-xf6h).
- **Prior-art check (triage cycles):** `gh pr list --repo getwyrd/wyrd --state open` → empty
  (no dependabot PR pending for this). `git log --oneline -- Cargo.lock` shows the usual
  dependabot bump cadence (`aws-smithy-types-1.6.2`, `async-trait-0.1.92` at `da9892d` /
  `6450101`), so a lockfile-only PR is the established shape for this change. No closed or
  rejected attempt exists.
- **Surfaces:** data
- **Difficulty:** low
- **Disposition hint:** likely-fix
