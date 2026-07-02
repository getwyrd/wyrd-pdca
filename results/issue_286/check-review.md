# Check review — issue 286 / dserver-container-non-root-user

**Task under review:** the D-server runtime container image
(`crates/chunkstore-grpc/tests/dserver/Dockerfile`) runs as root (uid 0) because its
runtime stage never creates an unprivileged user or sets `USER` before `ENTRYPOINT`.
The fix must make the shipped image run the d-server role as a non-root user with a
writable `/data`, guarded by a Rust test that encodes the non-root-`USER`-before-`ENTRYPOINT`
seam.

Grounded on target `/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch applied). Bash sandbox blocked
direct `cargo`/`docker`/`git` re-invocation; red→green re-derived statically and cross-checked
against the harness-run gates in `check-gates.json` (C4-ci `pass`, C4-verify `pass`).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's binding observable — image runs role as non-root uid + writes a fragment — is well-formed; in-gate seam (non-root `USER` before `ENTRYPOINT`) is the encodable slice, runtime write DEFERRED to Docker host by design (brief L33-43). |
| C2 Reproduction (red pre-fix) | PASS | Pre-fix runtime stage had no `USER` → test's `user_before_entrypoint` is `None` → `unwrap_or_else` panics (dserver_image.rs:119-130); matches repro `docker run --entrypoint id` → `uid=0(root)` (brief L27-29). C4-verify gate confirms red without fix. |
| C3 Change | PASS | Dockerfile:30-33 adds `--system` user/group `dserver` (10001:10001), `mkdir -p /data` + `chown dserver:dserver /data`; `USER dserver:dserver` at :38 before `ENTRYPOINT` :39 — minimal, scoped to the runtime stage, build stage untouched (in scope per brief L22-26). |
| C4 Verification (red→green) | PASS | Post-fix `is_root_user("dserver:dserver")` → `"dserver"` ≠ `0`/`root` → assert passes (dserver_image.rs:93-96,132-135); C4-verify gate `pass` (red→green) and C4-ci gate `pass` in check-gates.json. Note: brief's iteration-1 carry-forward records the prior gating C4-ci failure as base-drift/transient, now green — not a patch defect. |
| C5 Causal adequacy | PASS | Fix removes the cause (no privilege drop) structurally — a dedicated user + `USER` directive + `/data` ownership — rather than guarding a symptom. No capability-probe / runtime-guard smell (no `hasattr`/try-fallback/feature check); symptom-guard trigger does not fire. |
| T1 Structure | PASS | Test lives at `crates/chunkstore-grpc/tests/dserver_image.rs` (integration test beside the fixture it guards); reads Dockerfile via `CARGO_MANIFEST_DIR` (:103-104). |
| T2 Shape | PASS | Asserts the order-sensitive seam: multi-stage split takes the *last* `FROM` (:59-71), requires `USER` before `ENTRYPOINT` (:114-122), and rejects every root form `0`/`0:0`/`root`/`root:root` (:93-96) — not merely "some arg present". |
| T3 Runtime | PASS | Test compiles and runs inside `cargo xtask ci`; C4-ci gate `pass` in check-gates.json (could not re-invoke cargo directly — sandbox-blocked — relying on harness re-run). |
| T4 Contribution | PASS | Genuine regression guard: any future edit dropping/rooting the `USER` directive, or placing it after `ENTRYPOINT`, fails the test. Not a tautology. |
| T5 Judgment | PASS | Text-assertion level is the right in-gate slice given Docker is outside the DST/gate substrate (brief L33-43); the behavioral half (actual non-root runtime write) is explicitly deferred — see Validation row, not a test-quality defect. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: the in-gate test proves only the Dockerfile *text* encodes a non-root `USER`; the BINDING observable — built image runs the role as a non-root uid AND writes a fragment to `/data` as that user — is unverified (needs a Docker host, unavailable in-gate). Human must run brief L44-63 on a Docker host before §6 accept: (1) `docker build -f crates/chunkstore-grpc/tests/dserver/Dockerfile -t wyrd-dserver:test .`; (2) `docker run --rm --entrypoint id wyrd-dserver:test` → `uid=` MUST be non-zero; (3) start `d-server --data-dir /data` and confirm a fragment lands under `/data` owned by uid 10001 (or `cargo xtask integration`, which subsumes 1-3). If `/data` is not writable by the dropped user, the fix is incomplete — iterate, do not accept on the text assertion alone. |
