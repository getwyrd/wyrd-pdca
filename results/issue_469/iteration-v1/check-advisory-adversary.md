# Adversarial review — issue #469 / fdb-deploy-profiles

Lens: refute the red→green evidence and the reviewer's verdict; find the input that
breaks the fix. Grounded on the target at `/home/eddie/wyrd/wyrd.pdca-wt` (HEAD `84a9afb`).

## Evidence re-run (could not refute)

- **Re-ran the per-fix proof.** `cargo test -p xtask --test fdb_deploy_profiles` → **7/7
  green**, and the two Docker-backed tests *actually executed* (not skipped): `docker
  compose` v5.2.0 is present and `docker compose -f
  deploy/small-multi-node-fdb/docker-compose.yml config` renders a real 901-line config
  (exit 0), likewise `fdb-multi-replica` with `--profile fault` (exit 0). The asserted RED
  is over-determined and honest: on `origin/main` the two compose files do not exist
  (`both_new_fdb_stacks_exist` panics) and `deploy/README.md` has zero `fdb` occurrences
  (`git show origin/main:deploy/README.md | grep -ci fdb` → 0), so
  `readme_profile_matrix_names_all_six_profiles` also fails pre-fix. **Attempted to refute
  the red-without / green-with claim; could not.**
- **Attempted to refute the role-completeness assertion as weaker-than-claimed; could
  not.** The brief says the new `small_multi_node_fdb_compose_config_is_structurally_valid`
  makes "the same role-completeness assertion" as the TiKV peer. It does: the peer at
  `xtask/tests/deploy_no_orchestrator_coupling.rs:184-198` uses the identical
  `dserver0`/`dserver8` spot-check + one-each `custodian0`/`gateway0` convention. The new
  test is in fact *stronger* on backend wiring — the TiKV peer asserts no
  `--metadata-backend` at all.

## Fix — concrete cases that break / slip past the guard

- **NEEDS-HUMAN — `deploy/small-multi-node-fdb/docker-compose.yml:178` references a
  Dockerfile that does not exist in the target tree.** The `dserver0` build stanza points
  at `deploy/docker/wyrd/Dockerfile` (context `../..`), which is #470's artifact — but
  `deploy/docker/` is absent at HEAD; that Dockerfile lives only on the unmerged branch
  `origin/enhancement/470-wyrd-fdb-oci-image` (commit `c233380`, `git merge-base
  --is-ancestor c233380 HEAD` → **not** an ancestor). The tests stay green because `docker
  compose config` only parses and never validates build-context contents, but the
  README/compose-documented `docker compose -f .../small-multi-node-fdb up -d` would fail
  immediately at build. The brief made #470 a *hard* precondition ("If it is absent when Do
  runs, the fold has failed — **stop and say so**"). Whether the wave fold supplies #470 at
  merge time, or the bundle was authored on a broken precondition, is a human call —
  verdict provisional (fold-state, not a patch defect I can prove).
- **Over-broad / tautological backend-wiring assertion —
  `xtask/tests/fdb_deploy_profiles.rs:948-951.`** The structural test's final check is
  `merged.contains("--metadata-backend") && merged.contains("fdb")`. The token `fdb`
  appears unconditionally in the rendered config via the image (`foundationdb/foundationdb`),
  the service names (`fdb0..fdb2`) and the volume (`fdb-cluster`) — 84 occurrences — so this
  conjunction is true regardless of what backend the wyrd roles actually pass. The only
  *precise* guard is the pure-fs test at `:761`,
  `compose.contains("\"--metadata-backend\", \"fdb\"")`, which requires just **one**
  occurrence, and the three custodians (`docker-compose.yml:305,316,327`) already satisfy
  it. **Concrete regression the guard misses:** flip the three `gatewayN` commands
  (`:344,361,378`) to `"--metadata-backend", "tikv"` while leaving custodians on `fdb` —
  all 7 tests still pass, yet the S3 front door of the "FDB single-zone stack" would be
  wired to the wrong backend. Neither test asserts *every* role uses fdb, nor that *no*
  role uses tikv.
- **Check exercises parse-only, never bring-up — semantic validity is unverified.** Every
  Docker-backed assertion is `docker compose config` (YAML render). It cannot catch: a
  non-existent/unpullable image tag, an `fdbserver` that never reaches `configuration
  missing`→configured, a `command:` arg the wyrd binary rejects, or that FDB is actually
  reachable. The brief pre-declares the 21-container bring-up as deferred/maintainer-
  confirmed, so this is advisory rather than a refutation — but it means "the FDB stack
  works" is not among the facts this patch demonstrates at Check; only "its YAML parses and
  names the expected services/strings" is.

## Verdict / reviewer claims to weigh

- The `C4-verify` row ("red without the fix, green with it") is sound as far as the *test
  file* goes, but note its RED is carried entirely by the pure-filesystem
  non-existence checks; the Docker legs would be green-or-red purely on whether `docker
  compose` is installed, and add no independent binding signal beyond "the YAML parses."
- **The one claim I would not sign without a human:** that this slice ships a *runnable*
  `small-multi-node-fdb` stack (brief "Deferred ≠ unbuilt … every service definition has
  been parsed and role-checked"). Parsed, yes; **buildable in this target, no** — its wyrd
  image Dockerfile is not present (finding above). Adjudicate whether the fold is
  contracted to land #470 before this merges.
