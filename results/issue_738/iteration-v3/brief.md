# Brief — issue 738 / s3-role-chunk-size-flag

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md`
> §Dependencies ("The `s3` role does not expose `--chunk-size`"). Read in place in the
> target checkout — never copied here.

- **Slug:** s3-role-chunk-size-flag
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** The `s3` role cannot be told what chunk size to use, so every deployed
  gateway chunks at the 1 MiB default regardless of workload, hardware, or the D-server
  fan-out it fronts. This is a wiring gap, not a design position — the seam exists and the
  sibling role already exposes it. Verified on `main` at `65ca4fd`:
  `Gateway::with_chunk_size` exists at `crates/server/src/lib.rs:145`, doc-commented
  "mainly so tests can force multi-chunk objects"; `wyrd put --chunk-size N` is parsed at
  `crates/server/src/cli.rs:555-560`; and `cmd_s3` (`cli.rs:2113`) parses `--s3-listen`,
  `--data-dir`, `--region`, `--access-key`, `--secret-key`, `--metadata-backend`,
  `--coordination-backend`, `--endpoints`, `--otlp-endpoint` and no chunk size, so none of
  the six `Gateway::new` arms in `serve_s3_dispatch` (`cli.rs:2314,2320,2327,2334,2340,2347`)
  ever calls `with_chunk_size` and `DEFAULT_CHUNK_SIZE` (`lib.rs:51`, `1 << 20`) always
  wins. Two independent definitions of that default exist and can drift
  (`lib.rs:51` and `cli.rs:64`, both `1 << 20`).
- **Goal:** `wyrd s3 --chunk-size N` is honoured end to end; the flag's ABSENCE selects the
  single shared `DEFAULT_CHUNK_SIZE` (1 MiB) and produces the same on-disk chunking as
  today; an invalid value is refused at parse time rather than clamped silently; and the
  deployment templates can record the value a deployment runs.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check, no container, no
  cluster): a new integration test drives the **built `wyrd` binary** as an `s3` role over
  a loopback listener with `--chunk-size N`, PUTs an object of size `4N` through it, and
  asserts the local chunk store under `<data-dir>/chunks` holds exactly the number of chunk
  directories `N` implies (`FsChunkStore` keys each chunk as a `<32-hex>` directory of
  `<index>.frag` fragments — `crates/chunkstore-fs/src/lib.rs:64,78`). The same test asserts
  the DEFAULT case, stated as the exact observable it is: the identical object PUT with NO
  `--chunk-size` produces the chunk count 1 MiB implies — i.e. absence selects
  `DEFAULT_CHUNK_SIZE`, unchanged from today. ("Byte-identical persisted state" is NOT the
  claim and would be false: the gateway records `modified: Some(now_millis())` and mints
  chunk ids from a fresh random epoch — `crates/server/src/lib.rs:194-198,245-279` — so no
  two runs agree byte for byte. The chunk count is the property the flag actually changes,
  and it is what is asserted.)
  Plus the two refusal boundaries, as an exact accepted/rejected pair each:
  * `--chunk-size 0` exits non-zero, before binding a listener, naming the flag on stderr;
    `--chunk-size 1` is ACCEPTED (the role starts and prints its listen line);
  * `--chunk-size 1073741824` (1 GiB, the ceiling, inclusive) is ACCEPTED and
    `--chunk-size 1073741825` is REJECTED, non-zero, naming the flag. The ceiling value is
    SETTLED in this brief — see Design — and is not Do's to choose. The acceptance legs
    only start the role; they never PUT, so no `Vec::with_capacity(chunk_size)`
    (`crates/core/src/write.rs:561-568`) is ever allocated at the ceiling.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p wyrd-server --test s3_chunk_size_flag`, no Docker, no FDB, no feature
  flag. On the pre-fix tree the chunk-count assertion FAILS rather than errors, and it does
  so for the honest reason: `ParsedArgs::parse` (`crates/server/src/cli.rs:2501-2523`)
  records an unrecognised `--flag value` into its map and never rejects it, so pre-fix
  `--chunk-size 65536` is accepted, ignored, and the object chunks at 1 MiB. **This is why
  the test must drive the BINARY and not `serve_s3_role`.** This instance's `C4-verify`
  reverts the production change and keeps the added test
  (`engine/scripts/run-verify.sh:499-517`); a test calling a new `serve_s3_role` parameter
  cannot COMPILE against the reverted tree, and the gate correctly scores that
  `UNVERIFIABLE` (exit 77 → SUMMARY §6) rather than as a proven red
  (`run-verify.sh:201-215`). Driving `env!("CARGO_BIN_EXE_wyrd")` references no net-new
  symbol, so the red leg compiles and fails on the assertion — a genuine red.
  The child process MUST be killed by the test on every exit path (the `s3` role blocks
  forever): spawn with `--s3-listen 127.0.0.1:0`, read stderr until the role prints its
  `wyrd s3: serving S3-compatible HTTP on <addr> (data-dir …)` line (`cli.rs:2183-2186`,
  which reports `listener.local_addr()` — so the ephemeral port is parsed from that line
  rather than guessed, and two tests can run concurrently), drive the request, then kill. A
  test that lets the pre-fix binary run unbounded turns a red into a hang, which is the one
  outcome worse than either verdict.
- **Invariant to restore:** *Every knob that changes what the write path does on disk is
  settable by the operator who runs the role, and its default is defined once.* Stated over
  the category — the chunk size is a deployment property (it sets the unit of erasure
  coding, the fan-out width per object, the metadata chunk-map growth rate, and the peak
  resident bytes per in-flight PUT: "Peak resident bytes are one `chunk_size` piece plus its
  fragments, independent of object size", `crates/core/src/write.rs:528-529`) — not over the
  one call site that happens to be missing. Source: the repo's own configuration convention,
  applied at every peer role: `resolve_backend` for put/get/custodian (#255) and
  `resolve_coordination_backend` for d-server (#449), quoted in `cmd_s3`'s own comment at
  `crates/server/src/cli.rs:2138-2145` — "Select the gateway's backends BY CONFIGURATION,
  exactly as every other cluster-facing role does".
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 736
- **Ordering note:** #736 and #738 both edit `crates/server/src/cli.rs`, and both edit
  `cmd_s3`: #736 adds a `version` field to the `role started` event (`cli.rs:2199-2206`) and
  sets a new `S3Config` field in `serve_s3` (`cli.rs:2377-2384`); this one adds
  `--chunk-size` parsing at the top of `cmd_s3` and threads a parameter through
  `serve_s3_role` (`:2243`) → `serve_s3_dispatch` (`:2298`) → all six `Gateway::new` arms
  beneath it. Overlapping hunks in one function, so they must not be built blind on the same
  base. The declared conflict puts #736 in the earlier wave (the driver orients a conflict
  pair by id — `src/pdca_harness/waves.py:165-177`) and this bundle rebuilds on the folded
  result. There is no dependency in either direction; the order is arbitrary but
  deterministic. Re-verified with the driver's own scheduler after #740 was split into
  #773 → #774 → #775 (2026-08-18): waves are
  `[773] → [736, 774] → [738, 775] → [741] → [742]`, and no conflicting pair shares one.
  This bundle now shares its wave with **#775**, whose file set (`xtask/**`) is disjoint from
  this one's; #741 has moved to a later wave. #741's brief still explicitly
  forbids its fixture from calling `serve_s3_role`, precisely so this signature change
  cannot break it.
  **What the base may already contain — check, do not assume.** The wave fold puts each
  earlier wave's
  ACCEPTED work on the base this bundle builds against, so IF #736 was accepted, `cmd_s3`
  will already carry its `version` field on the `role started` event and its new `S3Config`
  field in `serve_s3` — the same two functions this slice edits. That is a conditional, not
  a promise: `Conflicts with` orders the waves, it does not make #736 a prerequisite, and
  #736 may be iterated or rejected. So **read those two functions as they stand on YOUR
  base** (`git -C <checkout> show HEAD:crates/server/src/cli.rs`), not as this brief's line
  numbers describe them on `65ca4fd`. If the `version` field is there, extend around it — a
  mechanical reapplication of the pre-batch shape would silently revert an accepted slice.
  If it is NOT there, that is the ordinary case too: #736 did not land, and this slice must
  not add it. Nothing in this brief's criterion depends on either outcome; say in
  `build-notes.md` which base you found.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** (a) parse `--chunk-size` in `cmd_s3`, defaulting to the existing value so an
  invocation without it is byte-identical to today; (b) thread it through `serve_s3_role` →
  `serve_s3_dispatch` to `Gateway::with_chunk_size` at each of the six composition arms; (c)
  refuse an invalid value at parse time — non-numeric, zero, and above a stated ceiling —
  rather than relying on the silent `.max(1)` clamp; (d) update `with_chunk_size`'s doc
  comment: it is no longer a test-only affordance; (e) add the flag to the `wyrd s3` usage
  line (`cli.rs:492`); (f) add it to `deploy/dist/env/s3.env.example` and the three
  `small-multi-node-fdb` gateway commands, and to the flag list
  `xtask/tests/dist_templates.rs:174-187` asserts; (g) collapse the two `DEFAULT_CHUNK_SIZE`
  definitions (`lib.rs:51`, `cli.rs:64`) into one so the role and the CLI cannot drift —
  **deliberate extra scope beyond the tracker's "consider", and here is its full review
  surface**, so it is judged rather than smuggled: `crates/server/src/lib.rs:51` becomes
  `pub` (an ADDITIVE change to `wyrd-server`'s public API — a new `pub const`, no existing
  item's signature or value changes), `crates/server/src/cli.rs:64` is deleted, and the ONE
  existing consumer of the CLI-local copy, `cmd_put` (`cli.rs:555-560`), reads the shared
  constant instead. Both consts are private today and both are `1 << 20` (verified on
  `main` at `65ca4fd`), so the collapse is value-preserving; the two `assert_eq!(…, 1 << 20)`
  unit tests (`lib.rs:775`, `cli.rs:2856`) collapse to one. It is IN scope because this
  slice's own compatibility claim — "absence selects `DEFAULT_CHUNK_SIZE`" — is a claim
  about a constant that currently exists twice and can drift; that is the difference between
  a claim that is checkable and one that is merely true today. If sign-off would rather see
  it split out, it is cleanly separable (drop (g); the criterion still holds against
  `cli.rs:64`'s copy) — say so at §9 rather than after the fact.
  **/ out of scope:** per-object or per-bucket chunk size (this is one process-level
  default, matching how every other role knob works); any change to the default VALUE;
  `--chunk-size` on any other role; **any change to `wyrd put`'s validation** — `cmd_put`
  accepts `0` today (`cli.rs:555-560`) and continues to; (g) changes which constant it reads
  and nothing else about its behaviour; the chunk-map budget constants
  `MAX_ROOT_VALUE_BYTES` / `MAX_ROOT_SEGMENTS` (that is #739, and proposal 0017
  §Dependencies says it must not be built until an architecture question about per-gateway
  ceilings is settled); removing the `.max(1)` floor inside `with_chunk_size` (see Design).
- **Repro instruction:** On `main` at `65ca4fd`, in the target checkout:
  `cargo run --bin wyrd -- s3 --help` (or read `crates/server/src/cli.rs:492`) — the usage
  line lists no `--chunk-size`. Start the role with `--chunk-size 65536 --access-key k
  --secret-key s --s3-listen 127.0.0.1:8080 --data-dir /tmp/wyrd-cs`; the flag is accepted
  and ignored (`ParsedArgs::parse` never rejects an unknown flag, `cli.rs:2501-2523`). PUT a
  256 KiB object through the gateway and count directories under `/tmp/wyrd-cs/chunks`:
  one, not four. `grep -n "DEFAULT_CHUNK_SIZE" crates/server/src/lib.rs crates/server/src/cli.rs`
  shows the two definitions.
- **External dependencies:** none — base Rust toolchain only. The test runs entirely
  in-process plus one child `wyrd` process on a loopback port; no Docker, no FDB, no etcd
  (the default `redb` + `mem` + local-FS composition is the one exercised).
- **Test file:** `crates/server/tests/s3_chunk_size_flag.rs` — a NEW file (this instance's
  C4-verify classifies a discriminator on an ADDED `*/tests/*.rs`,
  `engine/scripts/run-verify.sh:141-144`; appending to `cli_roundtrip.rs` or to the
  `#[cfg(test)]` module inside `cli.rs` would degrade the gate to green-only and prove
  nothing per-fix).
- **Verification posture:** Default — a flippable regression test, red pre-fix and green
  post-fix at Check. Nothing is deferred. The pure parse/validation decisions (absent ⇒ the
  single `DEFAULT_CHUNK_SIZE`; `0` and a non-numeric value refused; the ceiling refused) are
  additionally worth a unit test in `cli.rs`'s existing `#[cfg(test)] mod tests`
  (`cli.rs:2534`) — supplementary, not the discriminator.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer
  callsites Do MAY open and should mirror:
  * **The flag's own precedent, one function away** — `crates/server/src/cli.rs:555-560`
    (`cmd_put` parsing `--chunk-size` with `parse().map_err(|_| format!("put: invalid
    --chunk-size `{s}`"))`, defaulting to `DEFAULT_CHUNK_SIZE`). Parse it the same way, with
    the `s3:` prefix the role's other diagnostics use (`cli.rs:2119,2128,2135`), and add the
    zero/ceiling refusal that `cmd_put` does not have.
  * **The threading target** — `crates/server/src/cli.rs:2298-2350` (`serve_s3_dispatch`'s
    six `(metadata, coordination)` arms, each building `Arc::new(Gateway::new(...))`). Each
    arm must apply `with_chunk_size`; missing one is a silent, per-composition bug that only
    an fdb/etcd build would show. `Gateway::with_chunk_size` is the builder at
    `crates/server/src/lib.rs:145-149`.
  * **The other caller of the signature you are changing** —
    `crates/server/tests/s3_gateway_cluster.rs:153-166` calls `serve_s3_role` with 8
    arguments and must be updated. It is `pub`, so treat the signature change as a
    deliberate one and say so in `build-notes.md`.
  * **Driving the built binary from an integration test** —
    `crates/server/tests/cli_roundtrip.rs:11-18` (`const WYRD: &str =
    env!("CARGO_BIN_EXE_wyrd");` and its `run` helper). That is the idiom; this test differs
    only in that its child is a long-running server, so it must be spawned and killed rather
    than `output()`-ed.
  * **Signing an S3 request by hand against a loopback gateway** —
    `crates/server/tests/s3_http_wire.rs:95-110` (the production `sigv4::sign` over a
    `TcpStream`, with `format_amz_date(now)` so the signature is inside the freshness
    window). Reuse that shape rather than inventing one.
  * **What the deployment templates must name** — `deploy/dist/env/s3.env.example`
    (`WYRD_S3_ARGS=…`), the three gateway services in
    `deploy/small-multi-node-fdb/docker-compose.yml:387,404,421`, and the assertion list at
    `xtask/tests/dist_templates.rs:174-187` that pins every load-bearing flag the example
    must carry.
- **Prior-art check (triage cycles):** searched by affected path on `main` at `65ca4fd`.
  `git log --oneline -- crates/server/src/cli.rs` → recent history is #652 recovery
  totality, the custodian restore/drain containment, and the #619 lint sweeps; nothing has
  touched `cmd_s3`'s argument surface since it was written. `gh pr list --state all
  --search "chunk-size"` returns #647 (CLOSED — segmented chunk maps beyond one value, a
  `crates/core` metadata-record change, not a role flag), #672, #675 and #610, none of which
  adds a role knob. No open or closed PR has attempted `--chunk-size` on the `s3` role.
- **Disposition hint:** new-feature

## Motivation

**Operators cannot tune the write path.** Chunk size sets the unit of erasure coding, the
fan-out width per object, the metadata chunk-map growth rate, and the peak resident bytes
per in-flight PUT (`crates/core/src/write.rs:528-529`). One value cannot be right for a
9-node NVMe fleet and a single-node loopback both, and the value is not even recordable
today.

**It blocks measurement.** `wyrd-validate`'s size classes are chosen to straddle the chunk
boundary deliberately (proposal 0017 §4). With the chunk size fixed and unexposed, the
driver has to assume 1 MiB and its size coverage is accurate only by coincidence. The same
holds for any future throughput sweep — chunk size is the first knob a benchmark would want.

**The dist package cannot record it.** `deploy/dist/env/s3.env.example` mirrors the
blueprint's production invocations, and a deployment's chunk size is part of its identity.

## Design

### Parse, validate, thread

`cmd_s3` reads `--chunk-size` exactly as `cmd_put` does (`cli.rs:555-560`) and defaults to
the single shared `DEFAULT_CHUNK_SIZE`, so an invocation without the flag composes the
identical `Gateway` it composes today. The value then travels `cmd_s3` → `serve_s3_role` →
`serve_s3_dispatch` → `Gateway::with_chunk_size` on each of the six arms. There is no
shortcut here: `serve_s3_dispatch` monomorphizes a distinct `Gateway<M, C, Co>` per
`(metadata, coordination)` pair, and four of the six arms are `#[cfg]`-gated behind the
`tikv` / `etcd` / `fdb` features, so an arm missed in the default build is invisible until
someone builds the production feature set. Apply it in all six.

### Refuse, do not clamp

`with_chunk_size` currently does `chunk_size.max(1)` (`lib.rs:146`) — a silent clamp that
turns `--chunk-size 0` into a one-byte chunk size and an unbootable-in-practice gateway
that reports nothing wrong. The CLI must refuse instead: non-numeric, zero, and above a
stated ceiling, all before the listener is bound, with the flag named on stderr in the
`s3:` style the role's other refusals use.

Keep the `.max(1)` floor inside `with_chunk_size` rather than removing it. It is a
library-caller guard against a division by zero on a path this slice is not testing, and
removing it widens the change into `crates/server`'s public API for no gain. Update its doc
comment instead — it is no longer "mainly so tests can force multi-chunk objects", it is
the role's configuration seam.

**The ceiling — SETTLED, not Do's to choose.** Some upper bound must exist or "absurd" is
unenforceable, and the bound has a physical meaning: peak resident bytes are one chunk plus
its fragments (`crates/core/src/write.rs:528-529`), and the write path allocates
`Vec::with_capacity(chunk_size)` per in-flight piece (`write.rs:561-568`), so under the
default RS(6,3) an in-flight PUT holds roughly 1.5× the chunk size per gateway worker.

The constant is **`MAX_CHUNK_SIZE = 1 << 30` (1 GiB), and the bound is INCLUSIVE**: the
accepted range is `1 ..= 1_073_741_824`, so `1_073_741_824` starts and `1_073_741_825` is
refused. That exact pair is in the binding criterion above, which is what makes "above a
stated ceiling" falsifiable rather than satisfiable by any number Do likes. The reasoning,
recorded so a later reader sees a decision: 1 GiB already implies ~1.5 GiB resident per
in-flight PUT, which no deployment should reach, and it is far enough above any plausible
tuning (the largest sensible values are tens of MiB) that the refusal never obstructs a
real operator. Introduce it as a named constant beside `DEFAULT_CHUNK_SIZE` with that
rationale in its doc comment. If Do believes a different bound is more defensible, that is
a finding to REPORT in `build-notes.md` — not a value to change unilaterally, because the
criterion names this one.

### One default, not two

`crates/server/src/lib.rs:51` and `crates/server/src/cli.rs:64` both declare
`DEFAULT_CHUNK_SIZE = 1 << 20`, privately, and neither knows about the other. The issue
calls collapsing them a "consider"; this brief puts it in scope, because the slice's own
criterion — "default behaviour is byte-identical to today when the flag is absent" — is a
claim about a constant that currently exists twice. Make `lib.rs:51` the definition
(`pub`), have `cli.rs` use it, delete the duplicate. That is the smallest change that makes
the claim checkable rather than merely true today.

### Templates

`deploy/dist/env/s3.env.example`'s `WYRD_S3_ARGS` gains the flag with a comment in the file's
established style (each flag gets a short "what it decides" note), and the three
`small-multi-node-fdb` gateway `command:` arrays gain it too, so the compose stack documents
a non-default value rather than silently inheriting one. `xtask/tests/dist_templates.rs`'s
`env_examples_name_every_load_bearing_flag` (`:174-183`) is the container-free assertion
that keeps the example honest; add `--chunk-size` to its list.

## Alternatives considered

**Environment variable instead of a flag.** Rejected: every other role knob on this binary
is a flag, and the env-var slots that exist (`WYRD_S3_ACCESS_KEY`, `WYRD_FDB_CLUSTER_FILE`)
are for secrets and for paths a supervisor injects. A tuning knob belongs on the command
line, where `WYRD_S3_ARGS` puts it anyway.

**Per-bucket or per-object chunk size.** Explicitly out of scope in the issue, and it is a
different kind of change: a per-object size has to be recorded in the object's metadata and
honoured on read, which is a format question, not a wiring one.

**Clamp loudly (warn and continue) instead of refusing.** A gateway that starts with a
configuration the operator did not ask for is the failure mode `s3.env.example` already
guards against for credentials ("an empty-but-set `WYRD_S3_ACCESS_KEY=` would start the
gateway with empty-string credentials instead of refusing — the fail-closed contract would
be silently voided by the template itself", `dist_templates.rs:188-198`). Same principle.

**Test through `serve_s3_role` rather than the binary.** Shorter and much nicer to write —
and it cannot earn a per-fix red under this instance's gate, because the test would
reference the new parameter and fail to compile once the production change is reverted. See
Falsifiability. Drive the binary.

## Impact & compatibility

Backwards compatible by construction: the flag is optional and its default is the current
constant, so every existing invocation — the compose stacks, the systemd units, the
`s3.env.example`, and every test — composes the identical gateway. Two source changes reach
beyond `cmd_s3`, both named so a reviewer holds them in view: `serve_s3_role`'s signature,
which is `pub` and has exactly one caller outside the module
(`crates/server/tests/s3_gateway_cluster.rs:153`); and scope (g)'s constant collapse, which
makes `wyrd_server::DEFAULT_CHUNK_SIZE` `pub` (additive — nothing existing changes shape or
value) and redirects `cmd_put`'s default to it. `cmd_put`'s *behaviour* is untouched: same
value, same acceptance of `0`, no new refusal.

Two review points worth naming up front. A chunk size chosen per gateway means a FLEET of
gateways can disagree — objects written by one node chunk differently from another's. That
is benign today (the chunk map is per object and read is driven by the map, not by a
process-global assumption) and is worth stating explicitly in `build-notes.md` rather than
leaving a reviewer to work it out. And a very small chunk size multiplies chunk-map
entries, which interacts with the segmented-root ceiling #739 tracks — another reason the
CLI must refuse the absurd end of the range rather than clamp it.

## Plan-review response (#301 revision pass)

Five findings; four revised, one narrowed with its claim corrected.

* **"Byte-identical is not what the test observes."** Correct, and the word was wrong rather
  than merely loose — `modified: Some(now_millis())` and the random chunk-id epoch
  (`crates/server/src/lib.rs:194-198,245-279`) make two runs differ by construction. Goal and
  criterion now state the exact contract: absence selects `DEFAULT_CHUNK_SIZE` and yields
  the 1 MiB chunk count.
* **"The ceiling is undecided, so the refusal is satisfiable by any value."** Correct.
  `MAX_CHUNK_SIZE = 1 << 30`, inclusive, is now settled IN the brief, the criterion carries
  the exact accepted/rejected pair (`1073741824` / `1073741825`), and the open question is
  closed. Do reports disagreement; it does not change the number.
* **"An optional tracker suggestion is promoted to a required public-API change."** Kept, but
  no longer implicit: Scope (g) now names the whole review surface — `lib.rs:51` becomes
  `pub` (additive), `cli.rs:64` is deleted, `cmd_put` reads the shared constant, the two
  duplicate unit tests collapse to one — states why it is in scope (the compatibility claim
  is a claim about a constant that exists twice), and states that it is cleanly droppable at
  §9 if sign-off prefers.
* **"The `wyrd put` open question authorizes a drive-by."** Correct; withdrawn. `wyrd put`
  validation is now explicitly out of scope — it keeps accepting `0` — and (g) changes only
  which constant it reads.
* **"The asserted build base is false in the supplied target."** Correct as written: the base
  claim was stated as a fact when it is a conditional on #736 being ACCEPTED and folded. The
  ordering note now says check the base, extend what is there, add nothing that is not, and
  report which base was found. No dependency is declared, because there is none — the two
  slices are independent and only share a file.

## Open questions

1. ~~**The ceiling value.**~~ **SETTLED in this brief — `MAX_CHUNK_SIZE = 1 << 30`,
   inclusive.** See Design § "The ceiling"; the criterion names the accepted/rejected pair,
   so this is no longer open and Do must not vary it.
2. **Should the compose stack use a non-default value** (to prove the flag rather than
   merely document it) or the default (to keep the stack's behaviour unchanged)? The brief
   leans to naming the default explicitly, so the file records the deployment's identity
   without changing what the stack does.
3. ~~**Does `wyrd put`'s `--chunk-size` deserve the same zero/ceiling refusal?**~~
   **Withdrawn — explicitly OUT of scope** (see Scope). It is a second, independently
   observable CLI behaviour change on a command this issue is not about, with its own
   compatibility effect (`wyrd put --chunk-size 0` is accepted today,
   `crates/server/src/cli.rs:555-560`, and would start exiting non-zero). A "do it if it
   costs nothing" authorization is how an unasserted behaviour change ships; if it is
   wanted, it is a follow-up issue with its own red/green.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide how to discharge the advisory coverage evidence — independent red→green succeeds, but the frozen 0/16 report has mismatched source positions and fresh profiling omits proven serving paths; the 80% claim is unresolved (`reviewer-restored-green.log:10`; `gate-logs/C4-diff-cov.log:366`; `crates/server/tests/s3_chunk_size_flag.rs:53`).; **The new template assertion cannot go red.** `xtask/tests/dist_templates.rs:180` adds `"--chunk-size"`, but `:187` checks `s.contains(flag)` across the whole file. The comment this diff adds at `deploy/dist/env/s3.env.example:12` (`# --chunk-size bytes per chunk: …`) already satisfies it. **Reproduced:** I deleted ` --chunk-size 1048576` from the live `WYRD_S3_ARGS=` line (`s3.env.example:23`), leaving only the comment, and `cargo test -p xtask --test dist_templates env_examples_name_every_load_bearing_flag` still passes. Fix: for the new entry, find the line that starts with `WYRD_S3_ARGS=` and assert that it contains `--chunk-size`. The older `--s3-listen` / `--region` entries have the same weakness, but that predates this diff and is not in scope here.; **P1: Accepted chunk sizes exceed the cluster transport limit.** `crates/server/src/cli.rs:2308` accepts `--chunk-size 33554432`, and the new wiring applies it to the gRPC composition. A full 32 MiB chunk under RS(6,3) produces fragments over 5 MiB (`crates/core/src/erasure.rs:80`), but `ChunkStoreServer::new(service)` retains tonic's default 4 MiB decoding limit (`crates/server/src/dserver.rs:1206`). Thus an accepted configuration makes sufficiently large PUTs fail. The client also retains its default decoding limit (`crates/chunkstore-grpc/src/client.rs:251`), so increasing only the server limit leaves GETs broken. Make the transport support the brief's advertised range, accounting for fragment/protobuf overhead, and cover a PUT/GET above this boundary through the gRPC composition. This confirms the transport finding in the frozen batch-review log.; **P2: Pin the new integration test's local backends.** `crates/server/tests/s3_chunk_size_flag.rs:82` inherits the process environment without passing `--metadata-backend redb --coordination-backend mem`. The role honors `WYRD_METADATA_BACKEND` and `WYRD_COORDINATION_BACKEND` (`crates/server/src/cli.rs:286`, `crates/server/src/cli.rs:376`), so a developer environment selecting TiKV/FDB/etcd makes this supposedly self-contained test fail on the default build; feature-enabled builds can instead contact external services and write metadata outside the temporary directory. Pass the two backend flags explicitly so the chunk-size test always exercises its intended local composition.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 16 instrumentable changed lines executed (below the 80% floor); 16 of 99 changed lines were in
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide how to discharge the advisory coverage result — independent red→green passes, but frozen 34.8% coverage cites non-executable locations and fresh profiling omits observed serving execution; the 80% claim is unestablished (`reviewer-green.log:17`; `gate-logs/C4-diff-cov.log:504`; `reviewer-coverage-analysis.log:3`).; T5 Judgment — Repair the new test helper's response framing checks — malformed HTTP can satisfy its success/body assertions, weakening the wire evidence and violating the standing protocol-input rubric (`crates/server/tests/s3_chunk_size_flag.rs:295`; `crates/server/tests/s3_chunk_size_flag.rs:318`; `reviewer-parser.log:3`).; **DST D-server models no longer match production (rubric: *Test fidelity*).** Only `DServer::serve` raises the server-side limit (`dserver.rs:1210-1214`). The DST D-server models still mount a bare `ChunkStoreServer::new(...)` (`crates/dst/tests/network.rs:234,662,843,987`), which keeps tonic's 4 MiB limit. Their clients go through `GrpcChunkStore::new` (`client.rs:254`), which now sends up to 192 MiB. So a DST scenario with a chunk over ~24 MiB would get `OUT_OF_RANGE` where production succeeds. No current DST scenario uses fragments that large, so the gap is latent. The cause is the design choice in `crates/chunkstore-grpc/src/lib.rs:41-42` that every host must remember to apply the limit itself. Fix: export a constructor from `wyrd-chunkstore-grpc` that returns the bounded server, and use it in `dserver.rs` and the DST mounts.; **The rolling-upgrade order is not documented.** I reproduced both failure modes. With a new gateway (`--chunk-size 33554432`) and an old-limit D server, PUT returns `500 InternalError`. With an old-limit client reading those chunks, GET returns 200 with an empty body. So every D server, custodian and gateway must run the new binary before any gateway sets `--chunk-size` above ~24 MiB. Neither `08-crosscutting-concepts.md:111` nor the `--chunk-size` comment in `deploy/dist/env/s3.env.example:12-16` says so. The neighbouring W_write entry (`08-crosscutting-concepts.md:110`) already records its own mixed-version caveat, so adding one sentence here follows the doc's existing style.; **The T4 blocking finding is real but cannot cause a false pass.** `dechunk` (`s3_chunk_size_flag.rs:307`) accepts a truncated terminator, but it only runs on chunked error bodies. The success checks are status 200 plus exact byte equality (`:518`). The real gap sits next to it: `signed_request` ignores the declared `Content-Length` (`:295-297`). In my client-only revert, a truncated GET therefore showed up as "GET returned 0 bytes that differ" instead of being reported as a truncation. Cheap fix: assert that the body length equals `Content-Length`, and make `dechunk` require the final CRLF. T4 is a blocking gate, so this has to be handled either way.; **P2: Reject malformed response framing in the new regression client.** `crates/server/tests/s3_chunk_size_flag.rs:318` returns immediately on a zero chunk without requiring the terminal CRLF; `:322` skips two bytes without checking that they are CRLF, and `:295`–`:301` ignores declared `Content-Length`. For example, `dechunk(b"1
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide how to discharge the advisory coverage result — independent red→green passes, but frozen 34.8% coverage cites non-executable locations and fresh profiling omits observed serving execution; the 80% claim is unestablished (`reviewer-green.log:17`; `gate-logs/C4-diff-cov.log:504`; `reviewer-coverage-analysis.log:3`).; T5 Judgment — Repair the new test helper's response framing checks — malformed HTTP can satisfy its success/body assertions, weakening the wire evidence and violating the standing protocol-input rubric (`crates/server/tests/s3_chunk_size_flag.rs:295`; `crates/server/tests/s3_chunk_size_flag.rs:318`; `reviewer-parser.log:3`).; **DST D-server models no longer match production (rubric: *Test fidelity*).** Only `DServer::serve` raises the server-side limit (`dserver.rs:1210-1214`). The DST D-server models still mount a bare `ChunkStoreServer::new(...)` (`crates/dst/tests/network.rs:234,662,843,987`), which keeps tonic's 4 MiB limit. Their clients go through `GrpcChunkStore::new` (`client.rs:254`), which now sends up to 192 MiB. So a DST scenario with a chunk over ~24 MiB would get `OUT_OF_RANGE` where production succeeds. No current DST scenario uses fragments that large, so the gap is latent. The cause is the design choice in `crates/chunkstore-grpc/src/lib.rs:41-42` that every host must remember to apply the limit itself. Fix: export a constructor from `wyrd-chunkstore-grpc` that returns the bounded server, and use it in `dserver.rs` and the DST mounts.; **The rolling-upgrade order is not documented.** I reproduced both failure modes. With a new gateway (`--chunk-size 33554432`) and an old-limit D server, PUT returns `500 InternalError`. With an old-limit client reading those chunks, GET returns 200 with an empty body. So every D server, custodian and gateway must run the new binary before any gateway sets `--chunk-size` above ~24 MiB. Neither `08-crosscutting-concepts.md:111` nor the `--chunk-size` comment in `deploy/dist/env/s3.env.example:12-16` says so. The neighbouring W_write entry (`08-crosscutting-concepts.md:110`) already records its own mixed-version caveat, so adding one sentence here follows the doc's existing style.; **The T4 blocking finding is real but cannot cause a false pass.** `dechunk` (`s3_chunk_size_flag.rs:307`) accepts a truncated terminator, but it only runs on chunked error bodies. The success checks are status 200 plus exact byte equality (`:518`). The real gap sits next to it: `signed_request` ignores the declared `Content-Length` (`:295-297`). In my client-only revert, a truncated GET therefore showed up as "GET returned 0 bytes that differ" instead of being reported as a truncation. Cheap fix: assert that the body length equals `Content-Length`, and make `dechunk` require the final CRLF. T4 is a blocking gate, so this has to be handled either way.; **P2: Reject malformed response framing in the new regression client.** `crates/server/tests/s3_chunk_size_flag.rs:318` returns immediately on a zero chunk without requiring the terminal CRLF; `:322` skips two bytes without checking that they are CRLF, and `:295`–`:301` ignores declared `Content-Length`. For example, `dechunk(b"1\r\naxx0\r\n")` returns `b"a"` despite both malformed delimiters and a truncated terminator. Thus PUT assertions can accept an incomplete success response, and GET payload equality can pass despite invalid framing. Validate the declared length and chunk delimiters, including the final trailer-section terminator, and cover rejection of these malformed responses. This independently confirms the frozen T4 parser finding.; **P2: Correct the new PUT memory-sizing guidance.** `crates/server/src/lib.rs:57` and `deploy/dist/env/s3.env.example:14` describe peak PUT memory as approximately 1.5 times the chunk size. The input buffer remains live across the write (`crates/core/src/write.rs:574`, `:591`) while all nine RS(6,3) fragments are retained (`crates/core/src/write.rs:428`, `:453`): these allocations alone total approximately 2.5 times the chunk size. The gRPC path additionally copies fragment bytes (`crates/chunkstore-grpc/src/client.rs:294`). Operators sizing concurrent uploads from the new guidance would underprovision memory. Correct or remove the peak-memory estimate while retaining the settled 1 GiB ceiling.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b. 10 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 34.8% — 8 of 23 instrumentable changed lines executed (below the 80% floor); 23 of 152 changed lines were
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Ceiling (1 GiB) was set without considering the transport. Settle a max chunk size that fits the D-server gRPC limit (or make the limit configurable/derived) before building. Do not ship a fixed 192 MiB limit fleet-wide; it raises worst-case memory to ~12 GiB per D server. Split: (1) `--chunk-size` flag with a ceiling inside today's limit; (2) transport limit change as its own issue. Fix brief defects: drop "byte-identical", drop `wyrd put` validation and the forced DEFAULT_CHUNK_SIZE merge unless intended, resolve the #736 base claim. Doc threshold is ">= 24 MiB", not "above ~24 MiB".
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Ceiling (1 GiB) was set without considering the transport. Settle a max chunk size that fits the D-server gRPC limit (or make the limit configurable/derived) before building. Do not ship a fixed 192 MiB limit fleet-wide; it raises worst-case memory to ~12 GiB per D server.
  Split: (1) `--chunk-size` flag with a ceiling inside today's limit; (2) transport limit change as its own issue.
  Fix brief defects: drop "byte-identical", drop `wyrd put` validation and the forced DEFAULT_CHUNK_SIZE merge unless intended, resolve the #736 base claim.
  Doc threshold is ">= 24 MiB", not "above ~24 MiB".
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 40.0% — 10 of 25 instrumentable changed lines executed (below the 80% floor); 25 of 182 changed lines were
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
