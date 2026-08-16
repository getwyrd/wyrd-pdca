# Brief — issue 685 / dependabot-advisories-tikv-boundary

> **This bundle ships no code.** #685 asked two questions: bump or account for five Dependabot
> advisories, and close (or state) the gap between the advisory corpus `cargo deny` reads and the
> one Dependabot reads. Both have since been answered on `main` by work that landed under **#543**
> and **#547**, and all five alerts were dismissed as `tolerable_risk` on **2026-08-16**. This brief
> records that accounting and hands the human one residual documentation decision.
>
> `Disposition hint: likely-close` routes the driver past the builder and reviewer leaves straight
> to sign-off (`driver.py:210-217`; `likely-close` is in `[driver].close_dispositions`), so the gate
> matrix lands N/A by construction with no gate command executed (`gates.py:153-171`).

- **Slug:** dependabot-advisories-tikv-boundary
- **Defect:** #685 recorded five open Dependabot alerts on `main` and asserted that *"the dependency
  wall does not see any of them"* — a HIGH-severity advisory invisible to the gate that exists to
  catch exactly that. **The second half of that assertion is no longer true**, and the first half is
  accounted for. Every one of the five reaches the graph through exactly one crate, `tikv-client`
  0.4.0, which is pulled in only by the **off-by-default `tikv` feature**; the shipped default
  artifact contains none of the vulnerable versions. `cargo xtask ci` audits that off-by-default
  tree on every PR through a second `cargo deny` invocation over `deny-all-features.toml`
  (`xtask/src/lib.rs:140-172`), where all five are recorded under their RUSTSEC identifiers with a
  stated exposure boundary, a no-fix-available rationale, and a dated review trigger. Nothing is
  unfixed here that a fix could reach; what remains is a wording question, stated below.
- **Success criterion:** the close is **complete and correct** — every element of #685's stated
  acceptance is either satisfied on `origin/main @ 65ca4fd` or explicitly dispositioned in the
  mapping below, with no element left unassigned. Re-checkable at sign-off with three commands:
  `gh api repos/getwyrd/wyrd/dependabot/alerts -q '.[]|"\(.number) \(.state) \(.dismissed_reason)"'`
  (expect five `dismissed` / `tolerable_risk`), `grep -c '^    { id = ' deny-all-features.toml`
  (expect 8 — two inherited from `deny.toml` plus the six tikv entries), and
  `cargo deny --all-features --config deny-all-features.toml check advisories` (expect green).
- **Falsifiability:** the criterion goes RED if any of the five alerts is not dismissed, if any of
  the five vulnerable *packages* has no corresponding RUSTSEC entry in `deny-all-features.toml`, if
  `cargo tree -i tikv-client` resolves on the **default** graph (which would put the vulnerable
  versions in the shipped artifact and void the whole exposure-boundary argument), or if the
  all-features advisory wall is red. All four were checked at Plan on 2026-08-16 against
  `origin/main @ 65ca4fd`. **No gate can evaluate this** — a close-disposition bundle runs no
  gates — which is why it is a sign-off check, declared under `Verification posture`.
- **Invariant to restore:** none here — no code ships from this bundle. The invariant the existing
  machinery already holds, and which this close must not weaken: *the advisory wall guarding the
  **shipped** artifact carries zero exceptions for an off-by-default backend* — an advisory `ignore`
  suppresses by ID across the whole graph it is applied to, so a tikv exception parked in
  `deny.toml` would silently hole the shipped wall if a future default dependency ever pulled an
  affected version (`deny.toml:19-24`, `deny-all-features.toml:4-12`). That is precisely why the two
  configs are split, and why the correct disposition here is *close*, not *waive in `deny.toml`*.
- **Repo + branch target:** getwyrd/wyrd @ main   (**No PR opens from this bundle** — publish exits
  0 with *"nothing to contribute; close the tracker item by hand"* (`publish.py:161-166`), so the
  tracker action is the human's, at sign-off.)
- **Ordering note:** no `Depends on` / `Conflicts with` — deliberately. This bundle touches no file
  and builds nothing, so it can neither block nor collide with anything, and it may run in any wave
  or alongside any bundle.
- **Surfaces:** data
- **Difficulty:** low — zero files changed; the blast radius of a disposition record.
- **Scope:** record the accounting for #685's five advisories and the corpus-gap question, and close
  the tracker item. / **out of scope:** bumping any dependency (no reachable fix exists — see the
  mapping); adding a second advisory source or a Dependabot-severity CI step (that is the residual
  decision below, and if taken it is its own item, not this one); re-opening #543's or #547's
  settled two-config split; any change to `deny.toml`, `deny-all-features.toml` or `xtask`.
- **Repro instruction:** confirm the disposition rather than a defect. On `origin/main @ 65ca4fd`:
  `deny-all-features.toml:50-153` carries the tikv-client exposure boundary and its six entries;
  `xtask/src/lib.rs:163-172` shows the three `cargo deny` invocations `cargo xtask ci` makes;
  `deny.toml:7-57` carries the AUDIT POLICY header. On the tracker: all five Dependabot alerts read
  `dismissed` / `tolerable_risk`, dismissed 2026-08-16T19:51Z.
- **External dependencies:** none
- **Test file:** none — no code ships from this bundle, so there is no test to flip. The standing
  machine-check that keeps this disposition honest already exists and is not this bundle's to add:
  `cargo xtask ci`'s second `cargo deny` invocation over `deny-all-features.toml`, which turns red
  on a **new** advisory entering the tikv tree rather than letting it sit unnoticed.
- **Verification posture:** declared, because the default does not hold. This is a close / no-fix
  disposition: there is no production change, so no flippable red→green exists and every gate
  element lands N/A without running (`gates.py:155-165`). Verification is the **human's confirmation
  at sign-off** against the mapping below. Nothing is deferred-but-unbuilt: the accounting this
  bundle records is already built and already running on every PR.
- **Citations expected:** none of the code kind — nothing is built. The claims here are cited to
  `deny.toml`, `deny-all-features.toml` and `xtask/src/lib.rs` on `origin/main @ 65ca4fd`, to the
  Dependabot alerts API, and to closed issues #543 and #547.
- **Prior-art check (triage cycles):** by affected file path — `deny.toml` last changed by `28ff7b3`
  (*deny: waive RUSTSEC-2026-0253, lru via aws-sdk-s3, dev-only*), before that `7258fec` (#547) and
  `8fb1b70` (#543). **#543** (*Record the tikv-client advisory exposure boundary*) and **#547** (*The
  ADR-0003 license wall does not cover the off-by-default backend graphs*) are both **CLOSED** and
  are the work that answers this issue. No open PR touches either deny config.
- **Disposition hint:** likely-close

## #685's acceptance, fully mapped

| #685 acceptance element | Disposition | Evidence |
|---|---|---|
| each of the five is bumped, **or recorded as unreachable with the evidence** | **recorded as unreachable** | `deny-all-features.toml:50-153` — the tikv-client exposure boundary, six entries, each naming its chain and why no fix is reachable |
| `deny.toml` either checks the GitHub corpus **or documents that it does not** | **partially — the residual decision below** | `deny.toml:1-57` names RUSTSEC as its corpus five times but never states the GHSA blind spot in those words |
| `cargo xtask ci` green | **satisfied** | the three-invocation wall, `xtask/src/lib.rs:163-172`; all five advisories are ignored-with-reason, not unnoticed |

### The five alerts, each traced

All five resolve through `tikv-client` 0.4.0 (off-by-default `tikv` feature, `crates/server/Cargo.toml`).
Reverse-dependency edges read from `origin/main`'s `Cargo.lock`:

| Alert | Package | Chain from `tikv-client` 0.4.0 | RUSTSEC entry already waived |
|---|---|---|---|
| GHSA-82j2-j2ch-gfr8 (**HIGH**, CRL panic) | `rustls-webpki` 0.101.7 | → `tonic` 0.10.2 → `rustls` 0.21.12 → webpki | **RUSTSEC-2026-0104** |
| GHSA-xgp8-3hg3-c2mh (LOW) | `rustls-webpki` 0.101.7 | same chain | **RUSTSEC-2026-0098** |
| GHSA-965h-392x-2mh5 (LOW) | `rustls-webpki` 0.101.7 | same chain | **RUSTSEC-2026-0099** |
| GHSA-2gh3-rmm4-6rq5 / CVE-2025-53605 (MEDIUM) | `protobuf` 2.28.0 | → `prometheus` 0.13.4 → protobuf | **RUSTSEC-2024-0437** |
| GHSA-cq8v-f236-94qc (unsound) | `rand` 0.7.3 | → `fail` 0.4.0 → rand | **RUSTSEC-2026-0097** |

Two facts make this a close rather than a fix:

1. **The shipped artifact is unaffected.** `cargo deny check` (no feature flags, the `cargo xtask ci`
   default run) resolves none of `foundationdb`, `libloading` or `tikv-client` into its graph. The
   default graph carries the *patched* versions alongside — `rustls-webpki` 0.103.13, `protobuf`
   3.7.2, `rand` 0.8.6/0.9.4/0.10.2 — which is why `Cargo.lock` shows both majors and why the issue's
   own "something still pulls the old major" reading resolves to the `tikv` feature.
2. **No fix is reachable.** `tikv-client` 0.4.0 pins `tonic` 0.10 → `rustls` 0.21 → the vulnerable
   webpki. There is no newer `tikv-client` release; upstream declares 0.4.0 not production-ready
   (#435). The tonic/TLS upgrade that clears the chain merged upstream on 2026-06-26
   (tikv/client-rust#541) and has never been released. Under the #443 stand-down the `tikv` feature
   is **retained**, so these entries have no scheduled end — they are deleted by a `tikv-client`
   release carrying #541, or by a decision to decommission the backend that #443 deliberately does
   not make. A dated review trigger (**2027-01-31**, plus *immediately* on any new high/critical in
   the tree) is recorded in the file so "tracked, not waived" cannot decay into a silent waiver.

### The corpus-gap half — what actually changed

#685's premise was that `cargo deny` structurally cannot see these, because it reads RustSec and
Dependabot reads the GitHub Advisory Database, and none of the five GHSAs carries a RUSTSEC
identifier. **The identifier half of that is still true** — re-checked at Plan, `gh api
/advisories/<ghsa>` returns only the GHSA (and, for the protobuf one, its CVE) for all five, no
RUSTSEC cross-reference.

**The conclusion drawn from it is not.** The same five *vulnerabilities* are seen by `cargo deny`
under their RUSTSEC IDs, in the graph where they actually live — the all-features one — and that
invocation runs on every PR and every laptop. The gap is a missing cross-reference between two
identifier namespaces, not a blind spot in coverage. `deny.toml`'s claim of a zero-tolerance wall
over the shipped artifact is accurate as written.

Worth recording because it cuts against the issue's framing: `RUSTSEC-2025-0134` (`rustls-pemfile`,
unmaintained, same chain) is caught by `cargo deny` and **not** reported by Dependabot. The corpora
differ in both directions; RustSec is the stricter one here.

## What sign-off decides

1. **Confirm the close** (or override with iterate-to-Do, which archives the close marker and
   re-enables the full Do+Check band).
2. **The one residual — `deny.toml`'s header wording.** The header documents the *feature-graph*
   boundary at length (default vs `--all-features`, and where the tikv advisories are recorded) but
   never states the *corpus* boundary in the terms #685 asked for: that a GHSA advisory with no
   RUSTSEC identifier is not matched by ID, and that Dependabot triage is the process guarantee
   covering it. Three defensible answers, and **no default is assumed here**:
   - **close as-is** — the header already sends a reader to `deny-all-features.toml` for the tikv
     advisory surface, and the ID-namespace point is a detail below the line the header draws;
   - **close with a follow-up item** for a short header paragraph naming the corpus boundary
     (documentation only, no gate change) — the cheapest way to answer #685's second half literally;
   - **reopen to a fix path** if the maintainer wants the stronger option #685 floated — a second
     advisory source in `cargo deny`, or a CI step failing on open Dependabot alerts above a
     severity threshold. That is a gate change and belongs in its own item, not this bundle.
3. **Choose the tracker action for #685 itself** — the driver does none of it (publish exits with
   *"close the tracker item by hand"*). If closed, the comment should point at #543/#547 and at the
   2026-08-16 dismissals, so the next reader does not re-derive this mapping.

## A note on the addendum's merge-gating bullet

`docs/2026-07-31-alpha-reslicing-proposal.md`'s addendum asks that `rust`, `gate` and `dco` be added
to `main`'s required status checks before the first merge-mode batch, because `gh pr merge` fails
closed only on *required* checks. That bullet is **live again as of 2026-08-16** and is worth reading
before the next batch, though it is not this bundle's scope: `[driver].auto_merge` went back to
`true` on that date, so the driver once more merges each non-final wave's PRs itself. What now
carries the weight the addendum asked branch protection to carry is harness-side rather than
host-side — `merge_requires = "all"` reads the PR's **full** check rollup (not just host-required
checks) and refuses on any failing, pending or empty rollup, `merge_wait_secs = 1800` lets that
rollup settle before it is read, and `merge_sync_base = true` brings a PR up to date with its base
first so the rollup describes the tree the PR actually merges into. Adding `rust` / `gate` / `dco`
to branch protection would still be defence in depth, and remains a sensible standalone item.

## STOP discipline

Nothing to build, nothing to publish. Draft only until Check sign-off; the tracker action for #685
is the human's.
