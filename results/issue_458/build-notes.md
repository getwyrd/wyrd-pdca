# Build notes — issue 458 / d-server-advertise-addr

## What changed, and why

The bug: `DServer::bind` (`crates/server/src/dserver.rs:185`, pre-fix) always sets
`endpoint = format!("http://{addr}")` from `listener.local_addr()` (`dserver.rs:191`).
There was no seam to override it, so a server bound to `0.0.0.0:50051` (the
containerized `deploy/small-multi-node/` posture) registers the un-dialable wildcard
into L5 `Coordination`.

The fix mirrors the cited peer pattern exactly (`with_identity`, `dserver.rs:202-206`
pre-fix): a builder method that consumes `self` and overrides one field, called after
`bind` and before `register`/`registration`.

1. **`crates/server/src/dserver.rs:210-219`** (new) — `DServer::with_advertise_addr`:
   ```rust
   pub fn with_advertise_addr(mut self, advertise: impl Into<String>) -> Self {
       self.endpoint = format!("http://{}", advertise.into());
       self
   }
   ```
   Takes `impl Into<String>` (a `host:port` string), not a `SocketAddr` — deliberately,
   because the whole point is to accept a value `--bind`'s `SocketAddr` parse (cited
   `cli.rs:463`) cannot: a routable DNS service name like `dserver1:50051` never parses
   as a `SocketAddr`. Also updated `bind`'s doc comment (`dserver.rs:180-186`) to stop
   asserting the now-false "split-horizon advertisement is a later deployment concern"
   and instead point at the new method — the brief's "Invariant to restore" names this
   exact deferral note as the thing now due.

2. **`crates/server/src/cli.rs`** — thread it from the CLI, mirroring the citations:
   - `cmd_d_server` (`cli.rs:463-472` pre-fix): parse `--advertise-addr` right after
     `--bind`, as a bare `Option<String>` (`parsed.flag("advertise-addr").map(str::to_string)`)
     — no `SocketAddr` parse, same reasoning as above.
   - `DServerParams` (`cli.rs:891-900` pre-fix): new field `advertise_addr: Option<String>`.
   - the struct literal (`cli.rs:519-528` pre-fix): pass it through.
   - `run_d_server` (`cli.rs:906-917` pre-fix, its `DServer::bind` call at `cli.rs:914`
     and `.with_identity` call at `cli.rs:916`): `server` becomes `mut`; `if let Some(addr)
     = params.advertise_addr { server = server.with_advertise_addr(addr); }` right after
     the existing builder chain, before the `eprintln!`/`register` calls that already
     read `server.endpoint()`.
   - usage string (`cli.rs:238` pre-fix): added `[--advertise-addr ADDR]`.

3. **`deploy/small-multi-node/docker-compose.yml`** — one `--advertise-addr
   dserverN:50051` appended to each of the 9 `dserverN` `command:` arrays (lines ~238,
   250, 262, 274, 286, 298, 310, 322, 334 pre-fix), and rewrote the stale in-file CAVEAT
   comment (pre-fix `deploy/small-multi-node/docker-compose.yml:224-229`) that documented
   the exact gap this issue closes, so it doesn't keep asserting a now-false limitation.
   This is the scope's config half; per "Verification posture" its *live* cross-container
   effect is off-Check, maintainer-confirmed on the stood-up stack, not gate-checked here.

## What I ruled out (with cost)

- **A `--advertise-addr` that parses to `SocketAddr` like `--bind` does.** Rejected
  per-scope: the brief's whole premise (`dserver.rs:184`/citations) is that a routable
  *DNS name* (`dserver1:50051`, no literal IP) must be advertisable, and `SocketAddr`
  parsing rejects a bare hostname (`"dserver1:50051".parse::<SocketAddr>()` errors — no
  A/AAAA resolution in `std::net::SocketAddr::from_str`). Keeping it a plain `String`
  costs nothing extra (one field, no new type) and is what the compose-file change
  (a DNS service name) actually needs.
- **Overriding `endpoint` directly as a full URL param (`--advertise-addr
  http://dserver1:50051`) instead of host:port.** Rejected to keep the CLI flag
  consistent with `--bind ADDR` (host:port, no scheme) and because the brief's out-of-scope
  line is explicit: "TLS/scheme negotiation (keep the `http://` scheme derivation)" — the
  builder derives `http://` itself (`dserver.rs:217`, mirroring `bind`'s `format!("http://{addr}")`
  at `dserver.rs:191`), so `with_advertise_addr` never takes a scheme.
- **A second builder that replaces `bind` entirely (i.e., merge advertise into `bind`'s
  signature: `bind(store, bind_addr, Option<advertise>)`).** Rejected: `with_identity` is
  the cited peer pattern precisely because it's a *post-bind* override — `bind` already has
  4 conceptual inputs (store, addr, id-to-be-set-later, domain-to-be-set-later) and adding a
  5th optional parameter to an `async fn` breaks every existing call site of `bind` (the two
  test files `dserver.rs` test module, `failure_domain_registration.rs:36`, and the new test)
  for a change touching one field. Diff cost of the rejected alternative: every one of those
  ~6 call sites gains a `None`/`Some(..)` argument (6 one-line diffs) for zero behavioural gain
  over the builder method (1 new method, 0 changed call sites for existing callers) — strictly
  worse by the "smallest change that restores the invariant" standard.

## Citations (target branch `feat/m4-production-metadata-backend`, pre-fix line numbers as read)

- `crates/server/src/dserver.rs:185` `DServer::bind` — endpoint derivation, `:191`.
- `crates/server/src/dserver.rs:202` `with_identity` — the mirrored builder pattern.
- `crates/server/src/dserver.rs:235` `registration`, `:246` `register` — what
  `with_advertise_addr` feeds (it only ever changes `self.endpoint`, which both read).
- `crates/server/src/cli.rs:463` `--bind` parse (why `--advertise-addr` can't reuse
  `SocketAddr` parsing).
- `crates/server/src/cli.rs:891` `DServerParams`, `:906` `run_d_server`, `:914`
  `DServer::bind` call, `:916` `.with_identity` call.
- `crates/server/src/cli.rs:238` usage string.
- `crates/server/tests/failure_domain_registration.rs:36-53` — the bind→register→discover→decode
  harness the new test mirrors.
- `deploy/small-multi-node/docker-compose.yml:224-229` (pre-fix) — the in-file CAVEAT this
  closes; the 9 `dserverN` `command:` lines it edits.

(Line numbers are as read pre-fix on `feat/m4-production-metadata-backend` at
`d422061` — the tip both `feat/m4-production-metadata-backend` and
`feat/m4.5-deploy-tikv-pd-etcd` resolved to when Do ran; `patch.diff`'s hunk headers are
the authoritative post-context locations.)

## Verification — red→green, through the project's own runner

Ran `PDCA_BUNDLE=results/issue_458 ./engine/scripts/run-verify.sh` from the `wyrd-pdca`
root (the `C4-verify` gate wired in `pdca.toml`, `cmd = "./engine/scripts/run-verify.sh"`,
`scope = "bundle"`) — NOT a hand-rolled `cargo test` invocation. It:

1. applies `patch.diff` to a clean worktree at the brief's target base
   (`origin/feat/m4-production-metadata-backend`, resolved from the brief's "Repo + branch
   target" field) and runs `cargo test -p wyrd-server --test advertise_addr_registration`:
   **GREEN** — `test result: ok. 2 passed; 0 failed`.
2. resets to that same base, re-applies the patch, then reverts every production file the
   patch touches (`cli.rs`, `dserver.rs`, `docker-compose.yml`) while KEEPING the added test
   file, and re-runs the same test command: **RED** —
   `error[E0599]: no method named `with_advertise_addr` found for struct `DServer<S>``
   (the test doesn't even compile against the pre-fix surface, exactly as the brief's
   "Falsifiability" predicted: "the builder the test drives does not yet exist, so it does
   not compile against the shipped surface").
3. `run-verify.sh: PASS — red without the fix, green with it.`

Also ran, ahead of the gate, as fast local sanity (not the authoritative red/green proof,
which is (1)-(3) above): `cargo fmt --check` (clean), `cargo clippy -p wyrd-server --tests
--all-targets` (clean, workspace lints from root `Cargo.toml:185-216` apply automatically
via `crates/server/Cargo.toml:113` `[lints] workspace = true`), `cargo check --workspace
--all-targets` (clean), and `docker compose config -q` on the edited
`deploy/small-multi-node/docker-compose.yml` (exits 0 — the compose edit is syntactically
valid; its *live* cross-container dial effect is the off-Check, maintainer-confirmed half
per "Verification posture", not reproducible here without standing up the stack).

## Refutation checklist (per Do discipline)

- **(a) Genuine red?** Yes — see run-verify.sh step 2 above: reverting the production
  diff while keeping the test produces a **compile error**, not just a failing assertion
  (the brief's own predicted failure mode). Confirmed by directly reading the gate's own
  stderr output, not asserted.
- **(b) Production path?** Yes — the test calls `wyrd_server::dserver::DServer::bind`,
  `.with_identity`, `.with_advertise_addr`, `.register`, and
  `wyrd_coordination_mem::MemCoordination::{new,discover}` — the real production types
  the fix changes, not a copy/mock. `DServerRegistration::decode` is the same decode path
  `Coordination`-backed discovery uses in production (`dserver.rs:154-166`).
- **(c) Fixture includes the fault?** Yes — the RED leg is the *actual* pre-fix production
  code (the revert is `git checkout -- <file>` on the same 3 modified files the patch
  touches, done BY the gate, not a hand-curated exclusion); nothing about the test's
  `MemCoordination` fixture excludes the code path being fixed — it's the same
  bind→register→discover→decode round trip `failure_domain_registration.rs` already
  exercises for the (unrelated) failure-domain label, just asserting on `.endpoint`
  instead of `.failure_domain`.

## Scope discipline

Out of scope per the brief, and NOT touched: consuming etcd-discovered D-server addresses
(no live consumer exists yet, per "Production reach" — the custodian dials static
`--endpoints`, the gateway is standalone until #454); `--bind`'s `SocketAddr` type (kept
numeric, unchanged); TLS/scheme negotiation (the `http://` prefix is still hardcoded, only
now applied to either the bound or the advertised address). No files outside
`crates/server/src/{cli,dserver}.rs`, the new test, and the one compose file were touched —
matches the brief's "Difficulty: low ... one logical change, one call site" framing; in
particular I did not touch `crates/server/src/cli.rs`'s `cmd_s3` / usage lines the brief's
"Ordering note" flags as shared with #454 beyond the single `d-server` usage line already
cited, so this stays out of #454's edited region (`cli.rs` ~236-249 mentions BOTH usage
lines; I only changed the `d-server` one at `cli.rs:238`, not the adjacent `s3`/`custodian`
lines #454 might touch).

## External dependencies

None beyond the base toolchain (cargo/rustc/clippy/rustfmt, already on PATH via
`engine/lib/ensure-cargo.sh`). `docker compose config` was available locally for the extra
compose-syntax sanity check but is not required by any Do-side gate; no etcd, no Docker
container was started to prove this slice (per the brief's "External dependencies" and
"Verification posture" fields, that's intentionally off-Check).
