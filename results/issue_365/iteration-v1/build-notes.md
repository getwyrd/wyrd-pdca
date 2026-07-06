# Build notes — issue 365 / coordination-etcd-l5-backend

_Withheld from the reviewer; for the human at sign-off._

## What the brief asked for (Success criterion)

A **second, networked `Coordination` implementation over etcd** plus **one shared
contract suite both impls pass**, with the trait/consumers byte-for-byte untouched and
selection done as a `server`-composition swap. The binding facts (not the illustrative
component names) drove the build; I mirrored the already-accepted `MetadataStore` +
TiKV pattern (`crates/metadata-tikv/`, `crates/metadata-conformance/`) verbatim because
the brief names it as the template (#258/#264) and it is the discipline the reviewer
and gate already understand.

## What shipped

1. **`crates/coordination-conformance/`** — the SHARED suite. The five trait-generic
   clauses that seeded in `coordination-mem/tests/conformance.rs:26-98` are lifted here
   verbatim (over `&impl Coordination`), plus a `run_all(make_coord)` runner (mirror of
   `metadata-conformance`'s `run_all`, `crates/metadata-conformance/src/lib.rs:291`).
   `tests/demonstrated_red.rs` drives the suite against a deliberately-broken stub and
   asserts each clause **panics** — the headless proof the assertions bite.

2. **`crates/coordination-etcd/`** — the second implementation.
   - `src/lib.rs`: the **load-light** `keyspace` + `fencing` modules (no `etcd-client`
     dep) with unit tests — the production decision-logic the store calls, testable on
     every machine (mirror of `metadata-tikv`'s `keyspace`/`paging`).
   - `src/store.rs`: `EtcdCoordination` implementing all ten trait methods over etcd,
     compiled only under the OFF-by-default `etcd` feature.
   - `tests/conformance.rs` (**the brief's named test**): endpoint-gated, drives the
     shared `run_all` against real etcd via `WYRD_ETCD_ENDPOINTS`.

3. **`coordination-mem`** — its test now drives the shared `run_all` (proving the lift
   didn't regress impl #1); its backend-specific tests (ManualClock expiry, exact config
   revisions) stay.

4. **`server` composition** — `CoordinationBackend { Mem, Etcd }` + `from_config` +
   `resolve_coordination_backend` + a `#[cfg(feature="etcd")]` `open_etcd_coord`, wired
   into `cmd_d_server` via a generic `run_d_server<Co: Coordination>` helper. The exact
   mirror of `MetadataBackend` (`cli.rs:77`). **No caller edits** — `DServer`/`dserver`
   already take `&impl Coordination`/`Arc<Co>`; the trait, `core`, `custodian` are
   untouched (ADR-0008/0016; invariant held).

5. Workspace membership + `Cargo.toml` wiring; `etcd-client = "0.14"` pinned once,
   optional-and-feature-gated everywhere it is consumed.

## The etcd → trait semantics (src/store.rs)

| Trait method | etcd mechanism | Fencing/expiry property |
|---|---|---|
| `register`/`discover` | leased `put` under `reg/{key}\x00{lease}`; prefix range-get | etcd deletes on lease lapse → **native expiry**, no local clock |
| `renew`/`revoke` | one `lease_keep_alive` / `lease_revoke` | expired lease → `Err` (matches mem) |
| `elect_leader` | `campaign` (blocks until leader) → token = leader-key `rev()`; re-elect from same instance `proclaim`s a rising token (no self-deadlock) | genuine cross-process single-leader; token from etcd's one global revision |
| `lock`/`unlock` | compare-and-put txn (`create_revision == 0`) → token = put revision; `None` if held; `unlock` by token via a local map | try-acquire (non-blocking), mutually-exclusive, fenced |
| `set_config`/`get_config`/`config_revision` | `put`/`get`; max `mod_revision` under `cfg/` | monotonic revision |

Locks **and** elections both draw their token from etcd's single global revision
counter, so the "tokens rise across locks and elections" invariant holds by
construction (`fencing::token_from_revision`, unit-tested).

### A real bug my own unit test caught
My first keyspace layout terminated the discovery key with `/`
(`reg/{key}/`). Because a discovery key may itself contain `/`, `reg/svc/` is a prefix of
`reg/svc/d/…`, so `discover("svc")` would have wrongly returned `svc/d`'s members —
whereas `coordination-mem` discovers by **exact** key (a `HashMap` lookup). The
`discovery_keys_do_not_alias_one_another` unit test went red; I switched the terminator
to `NUL` (`0x00`), which cannot be aliased by a `/` continuation. This is exactly the
value of keeping the decision-logic load-light and unit-tested.

## Red → green: what is proven where

The brief's flippable regression is "the trait-generic assertions go GREEN against real
etcd, RED against a non-implementing stub." That GREEN half is **irreducibly networked**
(it needs a running etcd) — no headless runner can stand one up, exactly as
`metadata-tikv`'s conformance is endpoint-gated. So the flip is split, honestly:

- **RED half, headless (proven now):** `coordination-conformance/tests/demonstrated_red.rs`
  drives the *production* shared suite against a broken `Coordination` and asserts every
  clause rejects it (5 tests, green — i.e. the contract bites). This is the non-vacuity
  guarantee; without it a suite that silently passed a stub would be worthless.
- **No-regression, headless (proven now):** `coordination-mem`'s `run_all` drive stays
  green.
- **Load-light production units, headless (proven now):** `keyspace`/`fencing` unit tests
  (7, green) exercise the exact code `store.rs` calls.
- **GREEN half, real etcd (NEEDS-HUMAN at sign-off):** bring up etcd, then
  `WYRD_ETCD_ENDPOINTS=http://127.0.0.1:2379 cargo test -p wyrd-coordination-etcd --features etcd`.
  Without the env var the named test **skips cleanly** (the default gate stays green).

I did **not** fabricate a headless stand-in for the networked run (a fake in-memory
"etcd") — that would pass vacuously and drive a copy, not production. The store drives
the real `etcd-client` wire.

## Why this shape, and alternatives ruled out

- **Feature-off-by-default (not a hard dep).** Keeps `etcd-client`'s gRPC tree out of the
  default `cargo xtask ci` compile — the same reason `metadata-tikv` gates `tikv`. The
  store module is `#[cfg(feature="etcd")]`, so the gate never builds it; I verified every
  `etcd-client` 0.14.1 API call by reading the crate source
  (`~/.cargo/registry/.../etcd-client-0.14.1/src/`), since I couldn't compile it here
  (no `protoc`; see below). Cost of the alternative (unconditional dep): every laptop/CI
  build pulls axum+tonic-0.12+prost-0.13 (10 crates added to the lock) and needs `protoc`
  — a needless tax on the 99% of builds that never touch etcd.
- **Non-blocking try-acquire lock via txn, not etcd's blocking `Lock` RPC.** The trait's
  `lock` is try-acquire (`Result<Option<LockGuard>>`, `traits/src/lib.rs:454-458`); etcd's
  `lock()` blocks. A compare-and-put txn is the faithful mapping. A blocking acquire is
  explicitly later work (trait note `:428-433`) — surfaced, not built.
- **Election API + local re-fence, not an unconditional-put "always grant".** An
  unconditional put would pass the (single-process) shared suite but silently drop the
  real single-leader guarantee two machines depend on. Using `campaign` keeps genuine
  single-leader; tracking the `LeaderKey` locally lets a repeated `elect_leader` from the
  same instance `proclaim` a rising token instead of self-deadlocking behind its own
  leadership.

## Gate status (ran the project's own runner)

`./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (fmt, clippy `-D warnings`,
build, whole-workspace test, cargo-machete, **cargo-deny**, conformance, statics,
orchestrator-guard, madsim DST). Notably `cargo deny check` is **green**
(`advisories ok, bans ok, licenses ok, sources ok`) — the `etcd-client` tree introduces
**no** new license and no advisory, so the machine-checked half of the dependency wall is
already satisfied.

## NEEDS-HUMAN (surface, don't absorb)

1. **etcd client dependency judgment (ADR-0003 fitness, not the machine check).**
   `cargo deny` is green, but adopting a new external gRPC client — version pin (`0.14`
   vs latest `0.19`), TLS/auth posture, and the fact that **building `--features etcd`
   needs `protoc`** (etcd-client 0.14 codegens via prost-build; the workspace otherwise
   uses pure-Rust `protox`) — is a maintainer call. The `etcd-conformance` job must
   install `protoc`, or a later refinement pins an etcd-client that vendors it.
2. **Real-etcd GREEN of the named test** (the networked half of the flip) — validate at
   sign-off with a live etcd (steps above). No headless runner can prove it.
3. **DST fidelity (the #264/#258 mirror).** `madsim-etcd-client = "0.7.0+0.18"` exists —
   the direct analogue of `madsim-tonic` — so the same wire code could run under
   `--cfg madsim`. Whether to adopt simulated-etcd vs a contract harness is the open
   decision the brief flags; I pinned `etcd-client` on the tonic-0.14 line and left the
   DST wiring to that decision (out of scope here).
4. **Sequencing (0015 `:461-463`).** Explicit M4 slice vs preceding coordination
   milestone — governance choice, branch base unaffected.

Trait note: the Coordination trait is at `crates/traits/src/lib.rs:434` on this tree (the
proposal's `:258-337` is stale), as the brief predicted — I cited the tree.
